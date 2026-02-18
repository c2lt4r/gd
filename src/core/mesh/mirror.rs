use super::half_edge::HalfEdgeMesh;

/// Mirror a mesh across an axis (0=X, 1=Y, 2=Z).
///
/// Negates the specified coordinate of every vertex, then rebuilds the mesh
/// with flipped face windings to maintain consistent outward normals.
pub fn mirror(mesh: &mut HalfEdgeMesh, axis: usize) {
    if mesh.faces.is_empty() {
        return;
    }

    // Collect positions with the axis negated
    let positions: Vec<[f64; 3]> = mesh
        .vertices
        .iter()
        .map(|v| {
            let mut p = v.position;
            p[axis] = -p[axis];
            p
        })
        .collect();

    // Collect face indices with reversed winding
    let mut indices: Vec<usize> = Vec::with_capacity(mesh.faces.len() * 3);
    for f in 0..mesh.faces.len() {
        let verts = mesh.face_vertices(f);
        if verts.len() == 3 {
            indices.push(verts[2]);
            indices.push(verts[1]);
            indices.push(verts[0]);
        }
    }

    *mesh = HalfEdgeMesh::from_triangles(&positions, &indices);
}
