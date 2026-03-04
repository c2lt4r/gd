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

    // Collect face indices with reversed winding (any face size)
    let mut poly_faces: Vec<Vec<usize>> = Vec::with_capacity(mesh.faces.len());
    for f in 0..mesh.faces.len() {
        let mut verts = mesh.face_vertices(f);
        if verts.len() >= 3 {
            verts.reverse();
            poly_faces.push(verts);
        }
    }

    let face_slices: Vec<&[usize]> = poly_faces.iter().map(Vec::as_slice).collect();
    *mesh = HalfEdgeMesh::from_polygons(&positions, &face_slices);
}
