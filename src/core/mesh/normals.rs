use std::collections::VecDeque;

use super::half_edge::HalfEdgeMesh;

/// Compute per-vertex normals by averaging incident face normals.
pub fn compute_vertex_normals(mesh: &HalfEdgeMesh) -> Vec<[f64; 3]> {
    // First compute per-face normals
    let face_normals: Vec<[f64; 3]> = (0..mesh.faces.len())
        .map(|f| compute_face_normal(mesh, f))
        .collect();

    // Average face normals per vertex
    let mut vertex_normals = vec![[0.0_f64; 3]; mesh.vertices.len()];

    for (f, normal) in face_normals.iter().enumerate() {
        let verts = mesh.face_vertices(f);
        for &v in &verts {
            vertex_normals[v][0] += normal[0];
            vertex_normals[v][1] += normal[1];
            vertex_normals[v][2] += normal[2];
        }
    }

    // Normalize
    for n in &mut vertex_normals {
        let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        if len > 1e-12 {
            n[0] /= len;
            n[1] /= len;
            n[2] /= len;
        } else {
            *n = [0.0, 1.0, 0.0]; // fallback up
        }
    }

    vertex_normals
}

/// Compute the face normal using Newell's method (robust for any polygon size).
pub fn compute_face_normal(mesh: &HalfEdgeMesh, f: usize) -> [f64; 3] {
    let verts = mesh.face_vertices(f);
    if verts.len() < 3 {
        return [0.0, 1.0, 0.0];
    }

    let mut nx = 0.0_f64;
    let mut ny = 0.0_f64;
    let mut nz = 0.0_f64;

    let n = verts.len();
    for i in 0..n {
        let cur = mesh.vertices[verts[i]].position;
        let next = mesh.vertices[verts[(i + 1) % n]].position;
        nx += (cur[1] - next[1]) * (cur[2] + next[2]);
        ny += (cur[2] - next[2]) * (cur[0] + next[0]);
        nz += (cur[0] - next[0]) * (cur[1] + next[1]);
    }

    let len = (nx * nx + ny * ny + nz * nz).sqrt();
    if len > 1e-12 {
        [nx / len, ny / len, nz / len]
    } else {
        [0.0, 1.0, 0.0]
    }
}

/// Fix winding order via BFS: propagate consistent orientation from a seed face.
///
/// Uses a collect-then-rebuild approach: determines which faces need flipping
/// without modifying the mesh, then rebuilds with corrected winding.
/// Returns the number of faces that were flipped.
pub fn fix_winding(mesh: &mut HalfEdgeMesh) -> usize {
    if mesh.faces.is_empty() {
        return 0;
    }

    let num_faces = mesh.faces.len();
    let mut visited = vec![false; num_faces];
    let mut should_flip = vec![false; num_faces];

    // BFS from face 0 — propagate winding to neighbors
    let mut queue = VecDeque::new();
    queue.push_back(0);
    visited[0] = true;

    // Determine if seed face normal points outward using a majority vote
    // across ALL faces. The single-face centroid heuristic fails when meshes
    // are thin, non-convex, or when the seed face centroid is near the mesh center.
    let mesh_center = mesh_centroid(mesh);
    let mut outward_votes: f64 = 0.0;
    for f in 0..num_faces {
        let fn_ = compute_face_normal(mesh, f);
        let fc = face_centroid(mesh, f);
        let out = [
            fc[0] - mesh_center[0],
            fc[1] - mesh_center[1],
            fc[2] - mesh_center[2],
        ];
        outward_votes += fn_[0] * out[0] + fn_[1] * out[1] + fn_[2] * out[2];
    }

    // If the majority of face normals point inward, the seed face needs flipping
    if outward_votes < 0.0 {
        should_flip[0] = true;
    }

    while let Some(face) = queue.pop_front() {
        let start_he = mesh.faces[face].half_edge;
        let mut he = start_he;
        loop {
            let twin = mesh.half_edges[he].twin;
            // Guard: skip boundary half-edges (twin == usize::MAX)
            if twin < mesh.half_edges.len()
                && let Some(neighbor) = mesh.half_edges[twin].face
                && !visited[neighbor]
            {
                visited[neighbor] = true;

                // Check original winding consistency, then account for
                // virtual flip of the current face.
                let consistent = is_winding_consistent(mesh, he, twin);
                // If current face is virtually flipped, consistency inverts:
                // consistent + flipped parent → neighbor needs flip
                // inconsistent + unflipped parent → neighbor needs flip
                should_flip[neighbor] = consistent == should_flip[face];

                queue.push_back(neighbor);
            }
            he = mesh.half_edges[he].next;
            if he == start_he {
                break;
            }
        }
    }

    // Rebuild mesh with corrected winding, preserving face sizes
    let flipped_count = should_flip.iter().filter(|&&f| f).count();
    if flipped_count == 0 {
        return 0;
    }

    let positions: Vec<[f64; 3]> = mesh.vertices.iter().map(|v| v.position).collect();
    let mut poly_faces: Vec<Vec<usize>> = Vec::with_capacity(num_faces);

    for (f, &flip) in should_flip.iter().enumerate() {
        let verts = mesh.face_vertices(f);
        if verts.len() < 3 {
            continue;
        }
        if flip {
            let mut reversed = verts;
            reversed.reverse();
            poly_faces.push(reversed);
        } else {
            poly_faces.push(verts);
        }
    }

    let face_slices: Vec<&[usize]> = poly_faces.iter().map(Vec::as_slice).collect();
    *mesh = HalfEdgeMesh::from_polygons(&positions, &face_slices);
    flipped_count
}

/// Check if two adjacent faces have consistent winding across a shared edge.
///
/// For consistent winding: if he goes v_a -> v_b on face1, the twin should
/// go v_b -> v_a on face2, and the shared edge vertices should appear in
/// opposite order in each face's vertex list.
fn is_winding_consistent(mesh: &HalfEdgeMesh, he: usize, twin: usize) -> bool {
    // he: points to vertex B, prev points to vertex A. So edge is A->B on face1.
    // twin: should go B->A on face2 for consistent winding.
    // twin points to some vertex. If winding is consistent, twin.vertex should be
    // the origin of he (which is he.prev.vertex for the face vertex).
    let he_target = mesh.half_edges[he].vertex;
    let twin_target = mesh.half_edges[twin].vertex;

    // For a triangle: he goes A->B, twin should go B->A.
    // he.vertex = B, twin.vertex should = A (the origin of he).
    // Origin of he = prev(he).vertex
    let he_origin = mesh.half_edges[mesh.half_edges[he].prev].vertex;

    // Consistent: twin goes from B to A, i.e. twin.vertex == he_origin
    // AND he goes from A to B, i.e. he.vertex == twin_origin
    let twin_origin = mesh.half_edges[mesh.half_edges[twin].prev].vertex;

    he_target == twin_origin && twin_target == he_origin
}

/// Flip normals on all faces (reverse every face's winding).
///
/// Uses collect-then-rebuild to avoid corrupting twin relationships from
/// incremental in-place flips.
pub fn flip_all(mesh: &mut HalfEdgeMesh) {
    let face_count = mesh.faces.len();
    if face_count == 0 {
        return;
    }

    let positions: Vec<[f64; 3]> = mesh.vertices.iter().map(|v| v.position).collect();
    let mut poly_faces: Vec<Vec<usize>> = Vec::with_capacity(face_count);

    for f in 0..face_count {
        let mut verts = mesh.face_vertices(f);
        if verts.len() >= 3 {
            verts.reverse();
            poly_faces.push(verts);
        }
    }

    let face_slices: Vec<&[usize]> = poly_faces.iter().map(Vec::as_slice).collect();
    *mesh = HalfEdgeMesh::from_polygons(&positions, &face_slices);
}

/// Flip only cap faces aligned with a given axis.
/// `axis`: 0=X, 1=Y, 2=Z — flips faces whose normal is mostly parallel to that axis.
///
/// Uses collect-then-rebuild to avoid corrupting twin relationships from partial flips.
pub fn flip_caps(mesh: &mut HalfEdgeMesh, axis: usize) -> usize {
    let face_count = mesh.faces.len();
    if face_count == 0 {
        return 0;
    }

    let mut count = 0;
    let positions: Vec<[f64; 3]> = mesh.vertices.iter().map(|v| v.position).collect();
    let mut poly_faces: Vec<Vec<usize>> = Vec::with_capacity(face_count);

    for f in 0..face_count {
        let mut verts = mesh.face_vertices(f);
        if verts.len() < 3 {
            continue;
        }
        let normal = compute_face_normal(mesh, f);
        if normal[axis].abs() > 0.7 {
            verts.reverse();
            count += 1;
        }
        poly_faces.push(verts);
    }

    if count > 0 {
        let face_slices: Vec<&[usize]> = poly_faces.iter().map(Vec::as_slice).collect();
        *mesh = HalfEdgeMesh::from_polygons(&positions, &face_slices);
    }
    count
}

/// Flip only faces whose centroid passes a spatial filter.
///
/// Uses collect-then-rebuild to avoid corrupting twin relationships from partial flips.
pub fn flip_where(
    mesh: &mut HalfEdgeMesh,
    filter: &super::spatial_filter::SpatialFilter,
) -> usize {
    let face_count = mesh.faces.len();
    if face_count == 0 {
        return 0;
    }

    let mut count = 0;
    let positions: Vec<[f64; 3]> = mesh.vertices.iter().map(|v| v.position).collect();
    let mut poly_faces: Vec<Vec<usize>> = Vec::with_capacity(face_count);

    for f in 0..face_count {
        let mut verts = mesh.face_vertices(f);
        if verts.len() < 3 {
            continue;
        }
        if super::spatial_filter::face_matches(mesh, f, filter) {
            verts.reverse();
            count += 1;
        }
        poly_faces.push(verts);
    }

    if count > 0 {
        let face_slices: Vec<&[usize]> = poly_faces.iter().map(Vec::as_slice).collect();
        *mesh = HalfEdgeMesh::from_polygons(&positions, &face_slices);
    }
    count
}

/// Compute the centroid of a face.
fn face_centroid(mesh: &HalfEdgeMesh, f: usize) -> [f64; 3] {
    let verts = mesh.face_vertices(f);
    let mut cx = 0.0;
    let mut cy = 0.0;
    let mut cz = 0.0;
    let n = verts.len() as f64;
    for &v in &verts {
        cx += mesh.vertices[v].position[0];
        cy += mesh.vertices[v].position[1];
        cz += mesh.vertices[v].position[2];
    }
    [cx / n, cy / n, cz / n]
}

/// Compute the centroid of the entire mesh.
fn mesh_centroid(mesh: &HalfEdgeMesh) -> [f64; 3] {
    if mesh.vertices.is_empty() {
        return [0.0; 3];
    }
    let mut cx = 0.0;
    let mut cy = 0.0;
    let mut cz = 0.0;
    let n = mesh.vertices.len() as f64;
    for v in &mesh.vertices {
        cx += v.position[0];
        cy += v.position[1];
        cz += v.position[2];
    }
    [cx / n, cy / n, cz / n]
}
