use std::collections::{HashMap, HashSet};

use super::half_edge::HalfEdgeMesh;

/// Epsilon for vertex welding — positions within this distance are merged.
pub const WELD_EPSILON: f64 = 1e-6;

/// Epsilon for geometric tests (plane classification, degeneracy).
pub const GEO_EPS: f64 = 1e-8;

/// Edge tag: boolean intersection boundary.
pub const EDGE_TAG_BOOLEAN: u32 = 1;

// ── Vector math helpers ──────────────────────────────────────────────

pub fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

pub fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

pub fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub fn lerp(a: [f64; 3], b: [f64; 3], t: f64) -> [f64; 3] {
    [
        a[0] + t * (b[0] - a[0]),
        a[1] + t * (b[1] - a[1]),
        a[2] + t * (b[2] - a[2]),
    ]
}

pub fn len2(v: [f64; 3]) -> f64 {
    dot(v, v)
}

pub fn dist2(a: [f64; 3], b: [f64; 3]) -> f64 {
    len2(sub(a, b))
}

/// Normal of a triangle (unnormalized cross product).
pub fn tri_normal(a: [f64; 3], b: [f64; 3], c: [f64; 3]) -> [f64; 3] {
    let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    [
        u[1] * v[2] - u[2] * v[1],
        u[2] * v[0] - u[0] * v[2],
        u[0] * v[1] - u[1] * v[0],
    ]
}

/// Canonical edge key: smaller index first.
pub fn canonical_edge(a: usize, b: usize) -> (usize, usize) {
    if a < b { (a, b) } else { (b, a) }
}

// ── Plane representation ─────────────────────────────────────────────

/// A plane defined by normal·p = d.
#[derive(Clone, Copy)]
pub struct Plane {
    pub normal: [f64; 3],
    pub d: f64,
}

/// Which side of a plane a vertex lies on.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Positive,
    Negative,
    On,
}

/// Compute the plane of a polygon using Newell's method.
pub fn face_plane(verts: &[[f64; 3]]) -> Plane {
    let mut nx = 0.0_f64;
    let mut ny = 0.0_f64;
    let mut nz = 0.0_f64;
    let n = verts.len();
    for i in 0..n {
        let cur = verts[i];
        let next = verts[(i + 1) % n];
        nx += (cur[1] - next[1]) * (cur[2] + next[2]);
        ny += (cur[2] - next[2]) * (cur[0] + next[0]);
        nz += (cur[0] - next[0]) * (cur[1] + next[1]);
    }
    let len = (nx * nx + ny * ny + nz * nz).sqrt();
    let normal = if len > 1e-12 {
        [nx / len, ny / len, nz / len]
    } else {
        [0.0, 1.0, 0.0]
    };
    let d = dot(normal, verts[0]);
    Plane { normal, d }
}

/// Classify a point relative to a plane.
pub fn classify_vertex(plane: &Plane, point: [f64; 3]) -> Side {
    let dist = dot(plane.normal, point) - plane.d;
    if dist > GEO_EPS {
        Side::Positive
    } else if dist < -GEO_EPS {
        Side::Negative
    } else {
        Side::On
    }
}

/// Test if two face planes are coplanar (same plane, same or opposite orientation).
pub fn planes_coplanar(a: &Plane, b: &Plane) -> bool {
    let cos = dot(a.normal, b.normal);
    if cos.abs() <= 1.0 - 1e-6 {
        return false;
    }
    let plane_dist = (a.d - b.d * cos.signum()).abs();
    plane_dist < 1e-6
}

// ── Union-find ───────────────────────────────────────────────────────

/// Union-find: find root with path compression.
pub fn uf_find(parent: &mut [usize], x: usize) -> usize {
    let mut r = x;
    while parent[r] != r {
        r = parent[r];
    }
    let mut c = x;
    while parent[c] != r {
        let next = parent[c];
        parent[c] = r;
        c = next;
    }
    r
}

/// Union-find: merge two sets.
pub fn uf_union(parent: &mut [usize], a: usize, b: usize) {
    let ra = uf_find(parent, a);
    let rb = uf_find(parent, b);
    if ra != rb {
        parent[rb] = ra;
    }
}

// ── Vertex welding ───────────────────────────────────────────────────

/// Weld a position into a canonical position list, returning its welded index.
pub fn weld_position(positions: &mut Vec<[f64; 3]>, p: [f64; 3]) -> usize {
    let eps2 = WELD_EPSILON * WELD_EPSILON;
    for (i, pos) in positions.iter().enumerate() {
        if dist2(*pos, p) < eps2 {
            return i;
        }
    }
    let idx = positions.len();
    positions.push(p);
    idx
}

// ── Edge dissolution ─────────────────────────────────────────────────

/// Compute the raw Newell normal magnitude for a polygon (proportional to area).
fn newell_magnitude(verts: &[[f64; 3]]) -> f64 {
    let n = verts.len();
    let mut nx = 0.0_f64;
    let mut ny = 0.0_f64;
    let mut nz = 0.0_f64;
    for i in 0..n {
        let cur = verts[i];
        let next = verts[(i + 1) % n];
        nx += (cur[1] - next[1]) * (cur[2] + next[2]);
        ny += (cur[2] - next[2]) * (cur[0] + next[0]);
        nz += (cur[0] - next[0]) * (cur[1] + next[1]);
    }
    (nx * nx + ny * ny + nz * nz).sqrt()
}

/// Merge coplanar adjacent faces via union-find, AND merge degenerate
/// (near-zero area) faces with any neighbor unconditionally.
fn merge_coplanar_and_degenerate(
    welded_faces: &[Vec<usize>],
    welded_positions: &[[f64; 3]],
    face_planes: &[Plane],
    edge_to_face: &HashMap<(usize, usize), usize>,
    parent: &mut [usize],
) {
    // Pass 1: merge coplanar adjacent faces
    for (fi, wface) in welded_faces.iter().enumerate() {
        let n = wface.len();
        for i in 0..n {
            let from = wface[i];
            let to = wface[(i + 1) % n];
            if let Some(&fj) = edge_to_face.get(&(to, from))
                && fi != fj
                && planes_coplanar(&face_planes[fi], &face_planes[fj])
            {
                uf_union(parent, fi, fj);
            }
        }
    }

    // Pass 2: merge degenerate (near-zero area) faces with any neighbor.
    // These have unreliable normals so planes_coplanar fails. Safe because
    // the degenerate face contributes essentially zero area.
    for (fi, wface) in welded_faces.iter().enumerate() {
        if wface.len() < 3 {
            continue;
        }
        let verts: Vec<[f64; 3]> = wface.iter().map(|&wi| welded_positions[wi]).collect();
        if newell_magnitude(&verts) >= 1e-10 {
            continue;
        }
        let n = wface.len();
        for i in 0..n {
            let from = wface[i];
            let to = wface[(i + 1) % n];
            if let Some(&fj) = edge_to_face.get(&(to, from))
                && fi != fj
            {
                uf_union(parent, fi, fj);
                break;
            }
        }
    }
}

/// Dissolve edges between coplanar adjacent faces, merging them into larger polygons.
///
/// Returns a new mesh with fewer faces. Only dissolves where BOTH adjacent faces
/// lie on the same plane (within epsilon). Degenerate faces (near-zero area) are
/// unconditionally merged with a neighbor to prevent fallback normals.
pub fn dissolve_coplanar_edges(mesh: &HalfEdgeMesh) -> HalfEdgeMesh {
    let nf = mesh.faces.len();
    if nf == 0 {
        return HalfEdgeMesh::default();
    }

    // Step 1: weld vertex positions to establish adjacency
    let mut welded_positions: Vec<[f64; 3]> = Vec::new();
    let mut welded_faces: Vec<Vec<usize>> = Vec::with_capacity(nf);

    for f in 0..nf {
        let vis = mesh.face_vertices(f);
        if vis.len() < 3 {
            welded_faces.push(Vec::new());
            continue;
        }
        let welded: Vec<usize> = vis
            .iter()
            .map(|&vi| weld_position(&mut welded_positions, mesh.vertices[vi].position))
            .collect();
        welded_faces.push(welded);
    }

    // Step 2: build edge-to-face map
    let mut edge_to_face: HashMap<(usize, usize), usize> = HashMap::new();
    for (fi, wface) in welded_faces.iter().enumerate() {
        let n = wface.len();
        for i in 0..n {
            let from = wface[i];
            let to = wface[(i + 1) % n];
            edge_to_face.insert((from, to), fi);
        }
    }

    // Step 3: compute face planes
    let face_planes: Vec<Plane> = welded_faces
        .iter()
        .map(|wface| {
            if wface.len() < 3 {
                return Plane {
                    normal: [0.0, 1.0, 0.0],
                    d: 0.0,
                };
            }
            let verts: Vec<[f64; 3]> = wface.iter().map(|&wi| welded_positions[wi]).collect();
            face_plane(&verts)
        })
        .collect();

    // Step 4: union-find to group coplanar + degenerate adjacent faces
    let mut parent: Vec<usize> = (0..nf).collect();
    merge_coplanar_and_degenerate(
        &welded_faces, &welded_positions, &face_planes, &edge_to_face, &mut parent,
    );

    // Step 5: group faces by union-find root
    let mut groups: HashMap<usize, Vec<usize>> = HashMap::new();
    for f in 0..nf {
        let root = uf_find(&mut parent, f);
        groups.entry(root).or_default().push(f);
    }

    // Step 6: for single-face groups, emit directly; for multi-face groups, trace boundary
    let mut out_faces: Vec<Vec<usize>> = Vec::with_capacity(groups.len());
    for group_faces in groups.values() {
        if group_faces.len() == 1 {
            let wface = &welded_faces[group_faces[0]];
            if wface.len() < 3 {
                continue;
            }
            out_faces.push(wface.clone());
            continue;
        }

        let boundary_polys = trace_dissolution_boundary(
            group_faces,
            &welded_faces,
            &welded_positions,
            &edge_to_face,
        );
        out_faces.extend(boundary_polys);
    }

    let face_slices: Vec<&[usize]> = out_faces.iter().map(Vec::as_slice).collect();
    HalfEdgeMesh::from_polygons(&welded_positions, &face_slices)
}

/// Trace boundary polygon loop(s) for a group of merged coplanar faces.
///
/// Returns vertex index loops (not positions). Preserves the original face winding.
fn trace_dissolution_boundary(
    group_faces: &[usize],
    welded_faces: &[Vec<usize>],
    _welded_positions: &[[f64; 3]],
    edge_to_face: &HashMap<(usize, usize), usize>,
) -> Vec<Vec<usize>> {
    use std::collections::HashSet;

    let face_set: HashSet<usize> = group_faces.iter().copied().collect();

    // Find boundary edges: edges whose twin is NOT in the same group
    let mut boundary_next: HashMap<usize, usize> = HashMap::new();
    for &fi in group_faces {
        let wface = &welded_faces[fi];
        let n = wface.len();
        for i in 0..n {
            let from = wface[i];
            let to = wface[(i + 1) % n];
            let is_internal = edge_to_face
                .get(&(to, from))
                .is_some_and(|&fj| face_set.contains(&fj));
            if !is_internal {
                boundary_next.insert(from, to);
            }
        }
    }

    if boundary_next.is_empty() {
        return Vec::new();
    }

    // Trace loops
    let mut visited: HashSet<usize> = HashSet::new();
    let mut result_polys = Vec::new();
    for &start_v in boundary_next.keys() {
        if visited.contains(&start_v) {
            continue;
        }
        let mut loop_verts = Vec::new();
        let mut current = start_v;
        loop {
            if visited.contains(&current) {
                break;
            }
            visited.insert(current);
            loop_verts.push(current);

            if let Some(&next_v) = boundary_next.get(&current) {
                current = next_v;
            } else {
                break;
            }
        }
        if loop_verts.len() >= 3 {
            result_polys.push(loop_verts);
        }
    }
    result_polys
}

// ── Pole detection ───────────────────────────────────────────────────

/// Pole classification by vertex valence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub enum PoleType {
    /// Regular vertex (4 edges) — ideal for subdivision.
    Regular,
    /// N-pole (3 edges) — forms convex corners.
    NPole,
    /// E-pole (5 edges) — forms concave corners / transition points.
    EPole,
    /// High-valence pole (6+ edges) — causes pinching under subdivision.
    High,
}

/// Count edges meeting at a vertex by walking its half-edge fan.
#[allow(dead_code)]
pub fn vertex_valence(mesh: &HalfEdgeMesh, v: usize) -> usize {
    let Some(start_he) = mesh.vertices[v].half_edge else {
        return 0;
    };
    let mut count = 0;
    let mut he_idx = start_he;
    loop {
        count += 1;
        let twin = mesh.half_edges[he_idx].twin;
        if twin >= mesh.half_edges.len() {
            // Boundary — count the final edge too
            count += 1;
            break;
        }
        let next = mesh.half_edges[twin].next;
        if next == usize::MAX || next >= mesh.half_edges.len() || next == start_he {
            break;
        }
        he_idx = next;
        if count > mesh.half_edges.len() {
            break; // safety
        }
    }
    count
}

/// Classify pole type from valence.
#[allow(dead_code)]
pub fn pole_type(valence: usize) -> PoleType {
    match valence {
        0..=3 => PoleType::NPole,
        4 => PoleType::Regular,
        5 => PoleType::EPole,
        _ => PoleType::High,
    }
}

/// Find all poles (non-4-valence vertices) with their positions and types.
#[allow(dead_code)]
pub fn find_poles(mesh: &HalfEdgeMesh) -> Vec<(usize, PoleType, [f64; 3])> {
    let mut poles = Vec::new();
    for v in 0..mesh.vertices.len() {
        let val = vertex_valence(mesh, v);
        let pt = pole_type(val);
        if pt != PoleType::Regular {
            poles.push((v, pt, mesh.vertices[v].position));
        }
    }
    poles
}

// ── N-gon quadrangulation ───────────────────────────────────────────

/// Convert n-gon faces (5+ vertices) to quad ring topology.
///
/// Faces with ≤4 vertices pass through unchanged. N-gon faces are converted
/// to concentric quad rings with an earcut core, reusing `build_quad_cap_3d`.
pub fn quadrangulate_ngons(mesh: &HalfEdgeMesh) -> HalfEdgeMesh {
    let nf = mesh.faces.len();
    if nf == 0 {
        return HalfEdgeMesh::default();
    }

    // Fast path: if no face has >4 vertices, return clone
    let has_ngons = (0..nf).any(|f| mesh.face_vertices(f).len() > 4);
    if !has_ngons {
        return mesh.clone();
    }

    let mut positions: Vec<[f64; 3]> = mesh.vertices.iter().map(|v| v.position).collect();
    let mut faces: Vec<Vec<usize>> = Vec::with_capacity(nf);

    for f in 0..nf {
        let vis = mesh.face_vertices(f);
        if vis.len() <= 4 {
            faces.push(vis.clone());
            continue;
        }
        // N-gon: convert to quad rings via build_quad_cap_3d
        let pos_before = positions.len();
        let faces_before = faces.len();
        if super::profile::build_quad_cap_3d(&vis, &mut positions, &mut faces, false).is_none() {
            // Earcut failed — restore state and keep the original n-gon
            positions.truncate(pos_before);
            faces.truncate(faces_before);
            faces.push(vis.clone());
        }
    }

    let face_slices: Vec<&[usize]> = faces.iter().map(Vec::as_slice).collect();
    HalfEdgeMesh::from_polygons(&positions, &face_slices)
}

// ── Edge tagging ────────────────────────────────────────────────────

/// Quantize a position to a grid for hash-based edge matching.
fn quantize(p: [f64; 3]) -> [i64; 3] {
    // 1e5 scale → 1e-5 tolerance, well above WELD_EPSILON (1e-6)
    let s = 1e5;
    [
        (p[0] * s).round() as i64,
        (p[1] * s).round() as i64,
        (p[2] * s).round() as i64,
    ]
}

/// Canonical quantized edge key (smaller first for order independence).
pub fn quantized_edge_key(a: [f64; 3], b: [f64; 3]) -> ([i64; 3], [i64; 3]) {
    let qa = quantize(a);
    let qb = quantize(b);
    if qa < qb { (qa, qb) } else { (qb, qa) }
}

/// Tag edges in `mesh` that match the given boundary edge position set.
///
/// Sets `mesh.edge_tags` to a per-half-edge tag vector. Edges whose
/// position pairs appear in `boundary_edges` get `EDGE_TAG_BOOLEAN`.
pub fn tag_edges_from_positions(
    mesh: &mut HalfEdgeMesh,
    boundary_edges: &HashSet<([i64; 3], [i64; 3])>,
) {
    if boundary_edges.is_empty() {
        return;
    }
    let mut tags = vec![0u32; mesh.half_edges.len()];
    for (he_idx, he) in mesh.half_edges.iter().enumerate() {
        if he.prev >= mesh.half_edges.len() {
            continue; // skip boundary half-edges with unlinked prev
        }
        let p_to = mesh.vertices[he.vertex].position;
        let p_from = mesh.vertices[mesh.half_edges[he.prev].vertex].position;
        let key = quantized_edge_key(p_from, p_to);
        if boundary_edges.contains(&key) {
            tags[he_idx] = EDGE_TAG_BOOLEAN;
        }
    }
    mesh.edge_tags = tags;
}
