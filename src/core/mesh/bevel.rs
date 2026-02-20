use std::collections::{HashMap, HashSet};

use super::half_edge::HalfEdgeMesh;
use super::normals::compute_face_normal;

/// Bevel sharp edges of a mesh by inserting chamfer geometry.
///
/// `radius`: offset distance from the original edge.
/// `segments`: number of arc segments for the bevel curve (1 = flat chamfer).
/// `edge_filter`: which edges to bevel — "all", "depth", or "profile".
///
/// Uses the half-edge adjacency to find sharp edges (high dihedral angle)
/// in O(E) time, then rebuilds the mesh with inset faces, bevel strips,
/// and vertex cap polygons.
#[allow(clippy::too_many_lines)]
#[cfg(test)]
pub fn bevel(mesh: &HalfEdgeMesh, radius: f64, segments: u32, edge_filter: &str) -> HalfEdgeMesh {
    bevel_with_profile(mesh, radius, segments, edge_filter, 0.5)
}

/// Bevel with explicit profile control.
///
/// `profile`: 0.0 = concave, 0.5 = circular (default), 1.0 = convex chamfer.
/// Values outside `[0.0, 1.0]` are clamped.
#[allow(clippy::too_many_lines)]
pub fn bevel_with_profile(
    mesh: &HalfEdgeMesh,
    radius: f64,
    segments: u32,
    edge_filter: &str,
    profile: f64,
) -> HalfEdgeMesh {
    if mesh.faces.is_empty() || radius <= 0.0 || segments == 0 {
        return mesh.clone();
    }

    let sharp_edges = find_sharp_edges(mesh, edge_filter);
    if sharp_edges.is_empty() {
        return mesh.clone();
    }

    // Build canonical sharp-edge set and map to (face1, face2)
    let mut sharp_set: HashSet<(usize, usize)> = HashSet::new();
    let mut edge_faces: HashMap<(usize, usize), [usize; 2]> = HashMap::new();

    for &he_idx in &sharp_edges {
        let he = &mesh.half_edges[he_idx];
        let v_to = he.vertex;
        let v_from = mesh.half_edges[he.prev].vertex;
        let key = canonical(v_from, v_to);
        sharp_set.insert(key);

        let f1 = he.face.unwrap_or(usize::MAX);
        let f2 = mesh.half_edges[he.twin].face.unwrap_or(usize::MAX);
        edge_faces.entry(key).or_insert([f1, f2]);
    }

    // Affected vertices — those touching at least one sharp edge
    let affected: HashSet<usize> = sharp_set.iter().flat_map(|&(a, b)| [a, b]).collect();

    // Face centroids for inset direction calculation
    let face_centroids: Vec<[f64; 3]> = (0..mesh.faces.len())
        .map(|fi| {
            let verts = mesh.face_vertices(fi);
            let n = verts.len() as f64;
            let mut c = [0.0; 3];
            for &vi in &verts {
                let p = mesh.vertices[vi].position;
                c[0] += p[0];
                c[1] += p[1];
                c[2] += p[2];
            }
            [c[0] / n, c[1] / n, c[2] / n]
        })
        .collect();

    let mut positions: Vec<[f64; 3]> = mesh.vertices.iter().map(|v| v.position).collect();
    let mut poly_faces: Vec<Vec<usize>> = Vec::new();

    // Inset map: (face_idx, vertex_idx) → new position index
    let mut inset_map: HashMap<(usize, usize), usize> = HashMap::new();

    for (fi, centroid) in face_centroids.iter().enumerate() {
        let verts = mesh.face_vertices(fi);
        for &vi in &verts {
            if !affected.contains(&vi) {
                continue;
            }
            // Only inset if this vertex has a sharp edge *within this face*
            let has_sharp = verts
                .iter()
                .any(|&vj| vj != vi && sharp_set.contains(&canonical(vi, vj)));
            if !has_sharp {
                continue;
            }

            let p = mesh.vertices[vi].position;
            let c = *centroid;
            let d = sub(c, p);
            let dist = length(d);
            let move_dist = radius.min(dist * 0.4);
            let t = if dist > 1e-12 { move_dist / dist } else { 0.0 };

            let inset_pos = [p[0] + d[0] * t, p[1] + d[1] * t, p[2] + d[2] * t];
            let new_idx = positions.len();
            positions.push(inset_pos);
            inset_map.insert((fi, vi), new_idx);
        }
    }

    // ── Phase 1: Rebuild original faces using inset vertices ─────────
    for fi in 0..mesh.faces.len() {
        let verts = mesh.face_vertices(fi);
        let mapped: Vec<usize> = verts
            .iter()
            .map(|&vi| inset_map.get(&(fi, vi)).copied().unwrap_or(vi))
            .collect();
        if mapped.len() >= 3 {
            poly_faces.push(mapped);
        }
    }

    // ── Phase 2: Bevel strips along each sharp edge ──────────────────
    for (&(va, vb), &[fi1, fi2]) in &edge_faces {
        if fi1 == usize::MAX || fi2 == usize::MAX {
            continue;
        }
        let Some(&a1) = inset_map.get(&(fi1, va)) else {
            continue;
        };
        let Some(&b1) = inset_map.get(&(fi1, vb)) else {
            continue;
        };
        let Some(&a2) = inset_map.get(&(fi2, va)) else {
            continue;
        };
        let Some(&b2) = inset_map.get(&(fi2, vb)) else {
            continue;
        };

        // Determine correct winding for the strip faces.
        // The strip normal should align with the average of the two face normals.
        let expected = {
            let n1 = compute_face_normal(mesh, fi1);
            let n2 = compute_face_normal(mesh, fi2);
            [n1[0] + n2[0], n1[1] + n2[1], n1[2] + n2[2]]
        };

        // Test triangle (a1, a2, b1) — if normal aligns with expected, use this winding
        let trial_normal = cross(
            sub(positions[a2], positions[a1]),
            sub(positions[b1], positions[a1]),
        );
        let flip_strip = dot(trial_normal, expected) < 0.0;

        if segments <= 1 {
            add_strip_quad(a1, b1, a2, b2, flip_strip, &mut poly_faces);
        } else {
            // Multi-segment: quadratic Bezier with profile-controlled control point.
            // profile=0.5 → circular arc (ctrl = original vertex position)
            // profile=1.0 → flat chamfer (ctrl = midpoint of inset positions)
            // profile=0.0 → concave (ctrl pushed past original position)
            let clamped = profile.clamp(0.0, 1.0);
            let va_orig = mesh.vertices[va].position;
            let vb_orig = mesh.vertices[vb].position;
            let mid_a = midpoint(positions[a1], positions[a2]);
            let mid_b = midpoint(positions[b1], positions[b2]);
            let va_pos = lerp_pos(va_orig, mid_a, (clamped - 0.5) * 2.0);
            let vb_pos = lerp_pos(vb_orig, mid_b, (clamped - 0.5) * 2.0);

            let mut prev_a = a1;
            let mut prev_b = b1;

            for s in 1..=segments {
                let t = f64::from(s) / f64::from(segments);
                let next_a = if s < segments {
                    let idx = positions.len();
                    positions.push(bezier_quad(positions[a1], va_pos, positions[a2], t));
                    idx
                } else {
                    a2
                };
                let next_b = if s < segments {
                    let idx = positions.len();
                    positions.push(bezier_quad(positions[b1], vb_pos, positions[b2], t));
                    idx
                } else {
                    b2
                };
                add_strip_quad(prev_a, prev_b, next_a, next_b, flip_strip, &mut poly_faces);
                prev_a = next_a;
                prev_b = next_b;
            }
        }
    }

    // ── Phase 3: Vertex caps ─────────────────────────────────────────
    for &v in &affected {
        let face_ring = mesh.vertex_faces(v);
        // Collect inset vertices in face-ring order
        let ring_verts: Vec<usize> = face_ring
            .iter()
            .filter_map(|&fi| inset_map.get(&(fi, v)).copied())
            .collect();

        if ring_verts.len() < 3 {
            continue;
        }

        // Determine winding: cap normal should point outward (same direction
        // as the average of the adjacent face normals).
        let avg_normal = {
            let mut n = [0.0; 3];
            for &fi in &face_ring {
                let fn_ = compute_face_normal(mesh, fi);
                n[0] += fn_[0];
                n[1] += fn_[1];
                n[2] += fn_[2];
            }
            n
        };

        let trial = cross(
            sub(positions[ring_verts[1]], positions[ring_verts[0]]),
            sub(positions[ring_verts[2]], positions[ring_verts[0]]),
        );
        let flip_cap = dot(trial, avg_normal) < 0.0;

        // Vertex caps stay triangulated (fan from ring_verts[0])
        for i in 1..ring_verts.len() - 1 {
            if flip_cap {
                poly_faces.push(vec![ring_verts[0], ring_verts[i + 1], ring_verts[i]]);
            } else {
                poly_faces.push(vec![ring_verts[0], ring_verts[i], ring_verts[i + 1]]);
            }
        }
    }

    let face_slices: Vec<&[usize]> = poly_faces.iter().map(Vec::as_slice).collect();
    HalfEdgeMesh::from_polygons(&positions, &face_slices)
}

// ── Helpers ──────────────────────────────────────────────────────────

fn canonical(a: usize, b: usize) -> (usize, usize) {
    if a < b { (a, b) } else { (b, a) }
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn midpoint(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        (a[0] + b[0]) * 0.5,
        (a[1] + b[1]) * 0.5,
        (a[2] + b[2]) * 0.5,
    ]
}

/// Lerp between two positions. t=0 → a, t=1 → b. t can be negative (extrapolation).
fn lerp_pos(a: [f64; 3], b: [f64; 3], t: f64) -> [f64; 3] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

fn length(v: [f64; 3]) -> f64 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

/// Quadratic Bezier: P(t) = (1-t)²·p0 + 2(1-t)t·ctrl + t²·p1
fn bezier_quad(p0: [f64; 3], ctrl: [f64; 3], p1: [f64; 3], t: f64) -> [f64; 3] {
    let u = 1.0 - t;
    [
        u * u * p0[0] + 2.0 * u * t * ctrl[0] + t * t * p1[0],
        u * u * p0[1] + 2.0 * u * t * ctrl[1] + t * t * p1[1],
        u * u * p0[2] + 2.0 * u * t * ctrl[2] + t * t * p1[2],
    ]
}

/// Emit a quad strip between (a1,b1) and (a2,b2) as a single quad face.
fn add_strip_quad(
    a1: usize,
    b1: usize,
    a2: usize,
    b2: usize,
    flip: bool,
    faces: &mut Vec<Vec<usize>>,
) {
    if flip {
        faces.push(vec![a1, b1, b2, a2]);
    } else {
        faces.push(vec![a1, a2, b2, b1]);
    }
}

// ── Sharp-edge detection ─────────────────────────────────────────────

/// Find half-edge indices of sharp edges based on dihedral angle.
fn find_sharp_edges(mesh: &HalfEdgeMesh, filter: &str) -> Vec<usize> {
    let threshold = 0.7_f64; // ~45 degrees
    let mut sharp = Vec::new();

    for (i, he) in mesh.half_edges.iter().enumerate() {
        // Skip boundary sentinels and process each edge only once (lower index)
        if he.twin >= mesh.half_edges.len() || i >= he.twin {
            continue;
        }

        let twin = &mesh.half_edges[he.twin];

        // Both sides must have faces
        let (Some(f1), Some(f2)) = (he.face, twin.face) else {
            continue;
        };

        let n1 = compute_face_normal(mesh, f1);
        let n2 = compute_face_normal(mesh, f2);

        // Dihedral angle: dot product of normals
        let d = n1[0] * n2[0] + n1[1] * n2[1] + n1[2] * n2[2];

        // Sharp edge if normals differ significantly
        if d < threshold {
            // Apply filter
            let include = match filter {
                "depth" => is_depth_edge(mesh, i),
                "profile" => is_profile_edge(mesh, i),
                _ => true, // "all"
            };
            if include {
                sharp.push(i);
            }
        }
    }

    sharp
}

/// Check if an edge is roughly aligned with the depth (extrusion) axis.
fn is_depth_edge(mesh: &HalfEdgeMesh, he_idx: usize) -> bool {
    let he = &mesh.half_edges[he_idx];
    let v_to = mesh.vertices[he.vertex].position;
    let v_from = mesh.vertices[mesh.half_edges[he.prev].vertex].position;
    let dx = (v_to[0] - v_from[0]).abs();
    let dy = (v_to[1] - v_from[1]).abs();
    let dz = (v_to[2] - v_from[2]).abs();
    let len = (dx * dx + dy * dy + dz * dz).sqrt();
    if len < 1e-12 {
        return false;
    }
    // An edge is a "depth" edge if it's mostly along one axis
    let max_component = dx.max(dy).max(dz);
    max_component / len > 0.8
}

/// Check if an edge is roughly on the profile plane (cap boundary).
fn is_profile_edge(mesh: &HalfEdgeMesh, he_idx: usize) -> bool {
    !is_depth_edge(mesh, he_idx)
}
