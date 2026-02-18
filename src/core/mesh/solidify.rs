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

    let mut indices: Vec<usize> = Vec::new();

    // Outer faces: same winding
    for fi in 0..mesh.faces.len() {
        let verts = mesh.face_vertices(fi);
        for i in 1..verts.len() - 1 {
            indices.extend_from_slice(&[verts[0], verts[i], verts[i + 1]]);
        }
    }

    // Inner faces: reversed winding, offset indices by n_verts
    for fi in 0..mesh.faces.len() {
        let verts = mesh.face_vertices(fi);
        for i in 1..verts.len() - 1 {
            indices.extend_from_slice(&[
                verts[0] + n_verts,
                verts[i + 1] + n_verts,
                verts[i] + n_verts,
            ]);
        }
    }

    // Side walls along boundary edges
    let boundary = mesh.boundary_edges();
    for &he_idx in &boundary {
        let he = &mesh.half_edges[he_idx];
        let v_to = he.vertex;
        let v_from = mesh.half_edges[he.prev].vertex;

        // Outer edge: v_from → v_to
        // Inner edge: v_from+n → v_to+n
        // Quad: (outer_from, outer_to, inner_to, inner_from)
        let of = v_from;
        let ot = v_to;
        let it = v_to + n_verts;
        let if_ = v_from + n_verts;

        indices.extend_from_slice(&[of, ot, it]);
        indices.extend_from_slice(&[of, it, if_]);
    }

    HalfEdgeMesh::from_triangles(&positions, &indices)
}
