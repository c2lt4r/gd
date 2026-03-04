use std::collections::{HashMap, HashSet};

use super::half_edge::HalfEdgeMesh;
use super::normals::compute_face_normal;
use super::spatial_filter::{self, SpatialFilter};
use super::topology::{canonical_edge as canonical, cross, dot, sub};

/// Bevel sharp edges of a mesh by inserting chamfer geometry.
///
/// `radius`: offset distance from the original edge.
/// `segments`: number of arc segments for the bevel curve (1 = flat chamfer).
/// `edge_filter`: which edges to bevel — "all", "depth", "profile", or "tagged".
///
/// Uses the half-edge adjacency to find sharp edges (high dihedral angle)
/// in O(E) time, then rebuilds the mesh with inset faces, bevel strips,
/// and vertex cap polygons.
#[allow(clippy::too_many_lines)]
#[cfg(test)]
pub fn bevel(mesh: &HalfEdgeMesh, radius: f64, segments: u32, edge_filter: &str) -> HalfEdgeMesh {
    bevel_with_profile(mesh, radius, segments, edge_filter, 0.5, None)
}

/// Bevel with segments=1 (flat chamfer, no arc intermediates).
#[cfg(test)]
pub fn bevel_seg1(mesh: &HalfEdgeMesh, radius: f64, edge_filter: &str) -> HalfEdgeMesh {
    bevel_with_profile(mesh, radius, 1, edge_filter, 0.5, None)
}

/// Bevel with explicit profile control.
///
/// `profile`: 0.0 = concave, 0.5 = circular (default), 1.0 = convex chamfer.
/// Values outside `[0.0, 1.0]` are clamped.
/// `spatial`: optional spatial filter — only bevel edges whose midpoint passes.
#[allow(clippy::too_many_lines)]
pub fn bevel_with_profile(
    mesh: &HalfEdgeMesh,
    radius: f64,
    segments: u32,
    edge_filter: &str,
    profile: f64,
    spatial: Option<&SpatialFilter>,
) -> HalfEdgeMesh {
    if mesh.faces.is_empty() || radius <= 0.0 || segments == 0 {
        return mesh.clone();
    }

    let sharp_edges = find_sharp_edges(mesh, edge_filter, spatial);
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

    for (fi, centroid_ref) in face_centroids.iter().enumerate() {
        let verts = mesh.face_vertices(fi);
        let n = verts.len();
        for (vi_idx, &vi) in verts.iter().enumerate() {
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

            // Per-edge inset: average the inward-pointing edge normals at this vertex.
            // For vertex vi at index vi_idx in face, the two edges are:
            //   prev_v → vi  and  vi → next_v
            // The inward direction for each edge is the edge perpendicular projected
            // onto the face plane, pointing toward the face interior.
            let p = mesh.vertices[vi].position;
            let prev_v = verts[(vi_idx + n - 1) % n];
            let next_v = verts[(vi_idx + 1) % n];
            let pp = mesh.vertices[prev_v].position;
            let np = mesh.vertices[next_v].position;

            let face_normal = compute_face_normal(mesh, fi);

            // Edge vectors
            let e_prev = sub(p, pp); // prev → vi
            let e_next = sub(np, p); // vi → next

            // Inward perpendicular: cross(face_normal, edge_dir) points inward
            let in_prev = cross(face_normal, e_prev);
            let in_next = cross(face_normal, e_next);

            // Average inward direction (bisector)
            let d = [
                in_prev[0] + in_next[0],
                in_prev[1] + in_next[1],
                in_prev[2] + in_next[2],
            ];
            let dist = length(d);
            // Limit inset to radius or 40% of distance to centroid (whichever is smaller)
            let to_center = length(sub(*centroid_ref, p));
            let move_dist = radius.min(to_center * 0.4);
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
    // Also record arc intermediate vertices for enhanced vertex caps.
    // arc_map: (vertex, face_from, face_to) → intermediate bezier vertex indices
    let mut arc_map: HashMap<(usize, usize, usize), Vec<usize>> = HashMap::new();

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

        // Derive strip winding from the half-edge direction in face fi1.
        // If fi1 has edge va→vb, the inset face has a1→b1.
        // The strip must have twin b1→a1, which is flip=false: [a1, a2, b2, b1].
        // If fi1 has edge vb→va, the inset face has b1→a1.
        // The strip must have twin a1→b1, which is flip=true: [a1, b1, b2, a2].
        let fi1_verts = mesh.face_vertices(fi1);
        let fi1_n = fi1_verts.len();
        let fi1_has_va_to_vb =
            (0..fi1_n).any(|i| fi1_verts[i] == va && fi1_verts[(i + 1) % fi1_n] == vb);
        let flip_strip = !fi1_has_va_to_vb;

        if segments <= 1 {
            add_strip_quad(a1, b1, a2, b2, flip_strip, &mut poly_faces);
        } else {
            let clamped = profile.clamp(0.0, 1.0);
            let va_orig = mesh.vertices[va].position;
            let vb_orig = mesh.vertices[vb].position;
            // For circular profile (0.5), use spherical interpolation on the
            // bevel sphere centered at the original vertex.  This places arc
            // intermediates ON the sphere instead of inside it (bezier with
            // control at the original vertex only reaches ~43% of radius for
            // 90-degree cube corners).
            let use_sphere = (clamped - 0.5).abs() < 0.001;
            let mid_a = midpoint(positions[a1], positions[a2]);
            let mid_b = midpoint(positions[b1], positions[b2]);
            let va_pos = lerp_pos(va_orig, mid_a, (clamped - 0.5) * 2.0);
            let vb_pos = lerp_pos(vb_orig, mid_b, (clamped - 0.5) * 2.0);

            let mut prev_a = a1;
            let mut prev_b = b1;
            let mut va_mids = Vec::new();
            let mut vb_mids = Vec::new();

            // Cache endpoint positions before pushing intermediates
            let a1_pos = positions[a1];
            let a2_pos = positions[a2];
            let b1_pos = positions[b1];
            let b2_pos = positions[b2];

            for s in 1..=segments {
                let t = f64::from(s) / f64::from(segments);
                let next_a = if s < segments {
                    let idx = positions.len();
                    if use_sphere {
                        positions.push(sphere_interp(a1_pos, a2_pos, va_orig, t));
                    } else {
                        positions.push(bezier_quad(a1_pos, va_pos, a2_pos, t));
                    }
                    va_mids.push(idx);
                    idx
                } else {
                    a2
                };
                let next_b = if s < segments {
                    let idx = positions.len();
                    if use_sphere {
                        positions.push(sphere_interp(b1_pos, b2_pos, vb_orig, t));
                    } else {
                        positions.push(bezier_quad(b1_pos, vb_pos, b2_pos, t));
                    }
                    vb_mids.push(idx);
                    idx
                } else {
                    b2
                };
                add_strip_quad(prev_a, prev_b, next_a, next_b, flip_strip, &mut poly_faces);
                prev_a = next_a;
                prev_b = next_b;
            }

            // Store arc intermediates (both directions for face ring walk)
            let va_rev: Vec<usize> = va_mids.iter().copied().rev().collect();
            arc_map.insert((va, fi1, fi2), va_mids);
            arc_map.insert((va, fi2, fi1), va_rev);

            let vb_rev: Vec<usize> = vb_mids.iter().copied().rev().collect();
            arc_map.insert((vb, fi1, fi2), vb_mids);
            arc_map.insert((vb, fi2, fi1), vb_rev);
        }
    }

    // ── Phase 2.5: Transition faces for non-sharp edges ──────────────
    // Non-sharp edges between faces with different vertex mappings (inset
    // vs original) create gaps.  Bridge them with transition quads/tris
    // whose winding is derived from half-edge direction (manifold-safe).
    {
        let mut visited: HashSet<(usize, usize)> = HashSet::new();
        let he_len = mesh.half_edges.len();
        for he in &mesh.half_edges {
            let Some(f_a) = he.face else { continue };
            if he.twin >= he_len || he.prev >= he_len {
                continue;
            }
            let Some(f_b) = mesh.half_edges[he.twin].face else {
                continue;
            };

            let v_to = he.vertex;
            let v_from = mesh.half_edges[he.prev].vertex;
            let key = canonical(v_from, v_to);

            if sharp_set.contains(&key) || !visited.insert(key) {
                continue;
            }

            let a_from = inset_map.get(&(f_a, v_from)).copied().unwrap_or(v_from);
            let a_to = inset_map.get(&(f_a, v_to)).copied().unwrap_or(v_to);
            let b_from = inset_map.get(&(f_b, v_from)).copied().unwrap_or(v_from);
            let b_to = inset_map.get(&(f_b, v_to)).copied().unwrap_or(v_to);

            if a_from == b_from && a_to == b_to {
                continue;
            }

            // Winding derived from directed edge:
            //   f_a has edge a_from → a_to   (mapped v_from → v_to)
            //   f_b has edge b_to   → b_from (mapped v_to   → v_from)
            // Transition face must contain twin edges:
            //   a_to → a_from  (twin of f_a's edge)
            //   b_from → b_to  (twin of f_b's edge)
            if a_from == b_from {
                poly_faces.push(vec![a_from, b_to, a_to]);
            } else if a_to == b_to {
                poly_faces.push(vec![a_to, a_from, b_from]);
            } else {
                poly_faces.push(vec![a_from, b_from, b_to, a_to]);
            }
        }
    }

    // ── Phase 3: Vertex caps ─────────────────────────────────────────
    // Walk the face ring around each affected vertex, collecting inset
    // copies plus bevel arc intermediates to form a complete cap polygon.

    // Build edge lookup from all previously-emitted faces for empirical
    // cap winding detection.  If a cap edge A→B already exists in a
    // neighbor face, the cap must use B→A (the twin direction).
    let emitted_edges: HashSet<(usize, usize)> = {
        let mut set = HashSet::new();
        for face in &poly_faces {
            let fl = face.len();
            for i in 0..fl {
                set.insert((face[i], face[(i + 1) % fl]));
            }
        }
        set
    };

    for &v in &affected {
        let ring = vertex_face_ring(mesh, v);
        let n = ring.len();
        if n < 2 {
            continue;
        }

        let mut cap: Vec<usize> = Vec::new();

        for i in 0..n {
            let (fi, shared_v) = ring[i];
            let fi_next = ring[(i + 1) % n].0;
            let v_fi = inset_map.get(&(fi, v)).copied().unwrap_or(v);

            cap.push(v_fi);

            // For sharp edges between fi and fi_next, add bevel arc intermediates
            if sharp_set.contains(&canonical(v, shared_v))
                && let Some(mids) = arc_map.get(&(v, fi, fi_next))
            {
                cap.extend_from_slice(mids);
            }
        }

        // Dedup consecutive equal vertices (non-inset faces share original v)
        cap.dedup();
        if cap.len() >= 2 && cap.first() == cap.last() {
            cap.pop();
        }

        if cap.len() < 3 {
            continue;
        }

        // Determine winding empirically: if a cap polygon edge A→B already
        // exists in a neighbor face, the cap must emit B→A (the twin).
        let flip = {
            let mut result = None;
            for i in 0..cap.len() {
                let j = (i + 1) % cap.len();
                if emitted_edges.contains(&(cap[i], cap[j])) {
                    result = Some(true); // reverse to create twin
                    break;
                }
                if emitted_edges.contains(&(cap[j], cap[i])) {
                    result = Some(false); // keep direction as twin
                    break;
                }
            }
            result.unwrap_or_else(|| {
                // Fallback: normal-based (rarely needed)
                let avg_normal = {
                    let mut normal = [0.0; 3];
                    for &(fi, _) in &ring {
                        let fn_ = compute_face_normal(mesh, fi);
                        normal[0] += fn_[0];
                        normal[1] += fn_[1];
                        normal[2] += fn_[2];
                    }
                    normal
                };
                let trial = cross(
                    sub(positions[cap[1]], positions[cap[0]]),
                    sub(positions[cap[2]], positions[cap[0]]),
                );
                dot(trial, avg_normal) < 0.0
            })
        };

        // Count distinct inset vertices (BoundVerts) in the cap.
        let k_count = cap
            .iter()
            .enumerate()
            .filter(|&(idx, _)| idx % (segments.max(1) as usize) == 0)
            .count();
        let is_structured = k_count >= 3
            && segments >= 2
            && cap.len() == k_count * (segments as usize)
            && cap.iter().all(|&vi| vi != v);

        if cap.len() < 5 {
            if flip {
                cap.reverse();
            }
            poly_faces.push(cap);
        } else if is_structured
            && fill_corner_patch(
                &cap,
                k_count,
                segments as usize,
                mesh.vertices[v].position,
                &mut positions,
                &mut poly_faces,
                flip,
            )
        {
            // Structured corner patch fill succeeded
        } else if super::profile::build_quad_cap_3d(&cap, &mut positions, &mut poly_faces, flip)
            .is_none()
        {
            // Fallback: paired fan from cap[0] if ring fill fails (degenerate projection)
            if flip {
                cap.reverse();
            }
            let cn = cap.len();
            let mut ci = 1;
            while ci + 2 < cn {
                poly_faces.push(vec![cap[0], cap[ci], cap[ci + 1], cap[ci + 2]]);
                ci += 2;
            }
            if ci + 1 < cn {
                poly_faces.push(vec![cap[0], cap[ci], cap[ci + 1]]);
            }
        }
    }

    let face_slices: Vec<&[usize]> = poly_faces.iter().map(Vec::as_slice).collect();
    HalfEdgeMesh::from_polygons(&positions, &face_slices)
}

// ── Corner patch fill ────────────────────────────────────────────────

/// Fill a structured bevel corner cap with sphere-projected vertices.
///
/// For S=2: center vertex on the bevel sphere + K kite quads.
/// For S>=3: concentric sphere-projected rings + inner grid fill.
///
/// `cap` — ordered boundary vertices (K*S total, K arcs of S each).
/// `k` — number of beveled edges at this vertex (BoundVerts).
/// `s` — segment count.
/// `v_pos` — original vertex position (sphere center).
fn fill_corner_patch(
    cap: &[usize],
    k: usize,
    s: usize,
    v_pos: [f64; 3],
    positions: &mut Vec<[f64; 3]>,
    faces: &mut Vec<Vec<usize>>,
    flip: bool,
) -> bool {
    let n = cap.len();
    if n < 6 || k < 3 || n != k * s {
        return false;
    }

    // Compute average bevel sphere radius from cap vertex distances
    let avg_r = {
        let sum: f64 = cap
            .iter()
            .map(|&vi| length(sub(positions[vi], v_pos)))
            .sum();
        sum / n as f64
    };
    if avg_r < 1e-12 {
        return false;
    }

    // Average direction from original vertex to cap → sphere center direction
    let mut avg_dir = [0.0; 3];
    for &vi in cap {
        let d = sub(positions[vi], v_pos);
        avg_dir[0] += d[0];
        avg_dir[1] += d[1];
        avg_dir[2] += d[2];
    }
    let dir_len = length(avg_dir);
    if dir_len < 1e-12 {
        return false;
    }

    let center_pos = [
        v_pos[0] + avg_dir[0] / dir_len * avg_r,
        v_pos[1] + avg_dir[1] / dir_len * avg_r,
        v_pos[2] + avg_dir[2] / dir_len * avg_r,
    ];

    if s == 2 {
        // S=2: single center vertex on sphere + K kite quads
        let center_idx = positions.len();
        positions.push(center_pos);

        for j in 0..k {
            let bv = cap[j * 2];
            let mid_fwd = cap[j * 2 + 1];
            let mid_bwd = cap[((j * 2) + k * 2 - 1) % (k * 2)];
            if flip {
                faces.push(vec![mid_bwd, center_idx, mid_fwd, bv]);
            } else {
                faces.push(vec![bv, mid_fwd, center_idx, mid_bwd]);
            }
        }
        return true;
    }

    // S >= 3: concentric rings projected onto the bevel sphere.
    // Same topology as build_quad_cap_3d but with sphere projection
    // to eliminate the flat-centroid pinch artifact.
    let rings = (n / 8).clamp(1, 3);
    let mut prev: Vec<usize> = cap.to_vec();

    for r in 0..rings {
        let t = 0.15 * (r + 1) as f64 / (rings + 1) as f64;
        let ring_base = positions.len();
        for &vi in cap {
            let p = positions[vi];
            let interp = lerp_pos(p, center_pos, t);
            positions.push(sphere_project(interp, v_pos, avg_r));
        }
        emit_quad_ring_slice(&prev, ring_base, n, flip, faces);
        prev = (ring_base..ring_base + n).collect();
    }

    // Innermost ring
    let inner_base = positions.len();
    for &vi in cap {
        let p = positions[vi];
        let interp = lerp_pos(p, center_pos, 0.15);
        positions.push(sphere_project(interp, v_pos, avg_r));
    }
    emit_quad_ring_slice(&prev, inner_base, n, flip, faces);

    // Grid fill innermost ring
    let inner_indices: Vec<usize> = (inner_base..inner_base + n).collect();
    super::profile::grid_fill_ring(&inner_indices, faces, flip);

    true
}

/// Emit a ring of quads connecting `prev` vertex indices to a new ring
/// starting at `ring_base`.  (Local version that takes a slice.)
fn emit_quad_ring_slice(
    prev: &[usize],
    ring_base: usize,
    n: usize,
    flip: bool,
    faces: &mut Vec<Vec<usize>>,
) {
    for i in 0..n {
        let j = (i + 1) % n;
        let oi = prev[i];
        let oj = prev[j];
        let ii = ring_base + i;
        let ij = ring_base + j;
        if flip {
            faces.push(vec![oi, ii, ij, oj]);
        } else {
            faces.push(vec![oi, oj, ij, ii]);
        }
    }
}

// ── Helpers ──────────────────────────────────────────────────────────

/// Walk half-edges around vertex `v`, returning `(face_idx, edge_target)`
/// for each face.  `edge_target` is the other vertex on the edge separating
/// this face from the next face in the ring.
fn vertex_face_ring(mesh: &HalfEdgeMesh, v: usize) -> Vec<(usize, usize)> {
    let mut ring = Vec::new();
    let Some(start_he) = mesh.vertices[v].half_edge else {
        return ring;
    };
    let mut he_idx = start_he;
    loop {
        let he = &mesh.half_edges[he_idx];
        if let Some(f) = he.face {
            ring.push((f, he.vertex));
        }
        let twin = he.twin;
        if twin >= mesh.half_edges.len() {
            break;
        }
        let next = mesh.half_edges[twin].next;
        if next == usize::MAX || next >= mesh.half_edges.len() || next == start_he {
            break;
        }
        he_idx = next;
        if ring.len() > mesh.faces.len() {
            break;
        }
    }
    ring
}

/// Spherical interpolation of two points on a sphere centered at `center`.
/// Returns a point on the sphere at parameter `t` (0 → a, 1 → b).
fn sphere_interp(a: [f64; 3], b: [f64; 3], center: [f64; 3], t: f64) -> [f64; 3] {
    let da = sub(a, center);
    let db = sub(b, center);
    let ra = length(da);
    let rb = length(db);
    if ra < 1e-12 || rb < 1e-12 {
        return lerp_pos(a, b, t);
    }
    let r = ra * (1.0 - t) + rb * t;
    // Normalized linear interpolation of directions (nlerp)
    let dx = da[0] * (1.0 - t) + db[0] * t;
    let dy = da[1] * (1.0 - t) + db[1] * t;
    let dz = da[2] * (1.0 - t) + db[2] * t;
    let d_len = (dx * dx + dy * dy + dz * dz).sqrt();
    if d_len < 1e-12 {
        return lerp_pos(a, b, t);
    }
    [
        center[0] + dx / d_len * r,
        center[1] + dy / d_len * r,
        center[2] + dz / d_len * r,
    ]
}

/// Project a point onto the sphere centered at `center` with radius `r`.
fn sphere_project(p: [f64; 3], center: [f64; 3], r: f64) -> [f64; 3] {
    let d = sub(p, center);
    let d_len = length(d);
    if d_len < 1e-12 {
        return p;
    }
    [
        center[0] + d[0] / d_len * r,
        center[1] + d[1] / d_len * r,
        center[2] + d[2] / d_len * r,
    ]
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
fn find_sharp_edges(
    mesh: &HalfEdgeMesh,
    filter: &str,
    spatial: Option<&SpatialFilter>,
) -> Vec<usize> {
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
            // Apply edge-type filter
            let include = match filter {
                "depth" => is_depth_edge(mesh, i),
                "profile" => is_profile_edge(mesh, i),
                "tagged" => !mesh.edge_tags.is_empty() && mesh.edge_tags[i] != 0,
                _ => true, // "all"
            };
            // Apply spatial filter
            let spatial_ok = spatial.is_none_or(|sf| spatial_filter::edge_matches(mesh, i, sf));
            if include && spatial_ok {
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
