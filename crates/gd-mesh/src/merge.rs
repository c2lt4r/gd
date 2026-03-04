use super::half_edge::HalfEdgeMesh;

/// Merge vertices within `distance` of each other, rebuilding the mesh.
///
/// Returns `(new_mesh, merged_count)` where `merged_count` is the number
/// of vertices that were welded into existing ones.
pub fn merge_by_distance(mesh: &HalfEdgeMesh, distance: f64) -> (HalfEdgeMesh, usize) {
    if mesh.vertices.is_empty() || distance <= 0.0 {
        return (mesh.clone(), 0);
    }

    let eps2 = distance * distance;
    let n = mesh.vertices.len();

    // Build remap: for each vertex, find the first vertex within distance
    let mut remap: Vec<usize> = (0..n).collect();
    for i in 1..n {
        let pi = mesh.vertices[i].position;
        for j in 0..i {
            let pj = mesh.vertices[remap[j]].position;
            let dx = pi[0] - pj[0];
            let dy = pi[1] - pj[1];
            let dz = pi[2] - pj[2];
            if dx * dx + dy * dy + dz * dz < eps2 {
                remap[i] = remap[j];
                break;
            }
        }
    }

    let merged_count = remap.iter().enumerate().filter(|&(i, &r)| r != i).count();

    if merged_count == 0 {
        return (mesh.clone(), 0);
    }

    // Compact positions: only keep representative vertices
    let mut new_index: Vec<Option<usize>> = vec![None; n];
    let mut positions: Vec<[f64; 3]> = Vec::new();

    for (i, &r) in remap.iter().enumerate() {
        if r == i {
            new_index[i] = Some(positions.len());
            positions.push(mesh.vertices[i].position);
        }
    }

    // For remapped vertices, use the representative's new index
    for i in 0..n {
        if remap[i] != i {
            new_index[i] = new_index[remap[i]];
        }
    }

    // Extract faces with remapped indices, preserving polygon topology
    let mut faces: Vec<Vec<usize>> = Vec::new();
    for fi in 0..mesh.faces.len() {
        let verts = mesh.face_vertices(fi);
        if verts.len() < 3 {
            continue;
        }
        // Remap and deduplicate consecutive vertices (from welding)
        let mut face: Vec<usize> = Vec::with_capacity(verts.len());
        for &v in &verts {
            let idx = new_index[v].unwrap_or(0);
            if face.last() != Some(&idx) {
                face.push(idx);
            }
        }
        // Remove wrap-around duplicate
        if face.len() > 1 && face.first() == face.last() {
            face.pop();
        }
        if face.len() >= 3 {
            faces.push(face);
        }
    }

    let face_slices: Vec<&[usize]> = faces.iter().map(Vec::as_slice).collect();
    (
        HalfEdgeMesh::from_polygons(&positions, &face_slices),
        merged_count,
    )
}
