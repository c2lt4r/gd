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

    // Extract faces with remapped indices, skip degenerate
    let mut indices: Vec<usize> = Vec::new();
    for fi in 0..mesh.faces.len() {
        let verts = mesh.face_vertices(fi);
        if verts.len() < 3 {
            continue;
        }
        for i in 1..verts.len() - 1 {
            let a = new_index[verts[0]].unwrap_or(0);
            let b = new_index[verts[i]].unwrap_or(0);
            let c = new_index[verts[i + 1]].unwrap_or(0);
            // Skip degenerate triangles
            if a != b && b != c && c != a {
                indices.extend_from_slice(&[a, b, c]);
            }
        }
    }

    (HalfEdgeMesh::from_triangles(&positions, &indices), merged_count)
}
