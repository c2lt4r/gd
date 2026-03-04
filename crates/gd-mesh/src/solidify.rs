use super::half_edge::HalfEdgeMesh;
use super::normals::compute_vertex_normals;

/// Give a mesh shell thickness by duplicating and offsetting along inverted normals.
///
/// Creates an outer shell (original), inner shell (offset inward), and connects
/// boundary edges with side walls. Returns a new mesh.
pub fn solidify(mesh: &HalfEdgeMesh, thickness: f64) -> HalfEdgeMesh {
    if mesh.faces.is_empty() || thickness <= 0.0 {
        return mesh.clone();
    }

    let vnormals = compute_vertex_normals(mesh);
    let n_verts = mesh.vertices.len();

    // Positions: [outer_0..outer_n, inner_0..inner_n]
    let mut positions: Vec<[f64; 3]> = Vec::with_capacity(n_verts * 2);

    // Outer shell (original positions)
    for v in &mesh.vertices {
        positions.push(v.position);
    }

    // Inner shell (offset inward along -normal)
    for (i, v) in mesh.vertices.iter().enumerate() {
        let n = vnormals[i];
        positions.push([
            v.position[0] - n[0] * thickness,
            v.position[1] - n[1] * thickness,
            v.position[2] - n[2] * thickness,
        ]);
    }

    let mut faces: Vec<Vec<usize>> = Vec::new();

    // Outer faces: preserve original polygon topology
    for fi in 0..mesh.faces.len() {
        let verts = mesh.face_vertices(fi);
        if verts.len() >= 3 {
            faces.push(verts.clone());
        }
    }

    // Inner faces: reversed winding, offset indices by n_verts
    for fi in 0..mesh.faces.len() {
        let verts = mesh.face_vertices(fi);
        if verts.len() >= 3 {
            let mut inner: Vec<usize> = verts.iter().map(|&v| v + n_verts).collect();
            inner.reverse();
            faces.push(inner);
        }
    }

    // Side walls along boundary edges — emit quads
    let boundary = mesh.boundary_edges();
    for &he_idx in &boundary {
        let he = &mesh.half_edges[he_idx];
        let v_to = he.vertex;
        let v_from = mesh.half_edges[he.prev].vertex;

        faces.push(vec![v_from, v_to, v_to + n_verts, v_from + n_verts]);
    }

    let face_slices: Vec<&[usize]> = faces.iter().map(Vec::as_slice).collect();
    HalfEdgeMesh::from_polygons(&positions, &face_slices)
}
