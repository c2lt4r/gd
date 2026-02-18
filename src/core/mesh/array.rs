use super::half_edge::{Face, HalfEdge, HalfEdgeMesh, Vertex};

/// Create N copies of a mesh along an offset vector.
///
/// The original mesh is at position 0; copy k is offset by `k * offset`.
/// All copies are merged into a single `HalfEdgeMesh`.
#[allow(dead_code)]
pub fn array(mesh: &HalfEdgeMesh, count: usize, offset: [f64; 3]) -> HalfEdgeMesh {
    if count <= 1 || mesh.vertices.is_empty() {
        return mesh.clone();
    }

    let n_verts = mesh.vertices.len();
    let n_he = mesh.half_edges.len();
    let n_faces = mesh.faces.len();

    let mut combined = HalfEdgeMesh {
        vertices: Vec::with_capacity(n_verts * count),
        half_edges: Vec::with_capacity(n_he * count),
        faces: Vec::with_capacity(n_faces * count),
    };

    for k in 0..count {
        let v_offset = combined.vertices.len();
        let he_offset = combined.half_edges.len();
        let f_offset = combined.faces.len();

        let dx = offset[0] * k as f64;
        let dy = offset[1] * k as f64;
        let dz = offset[2] * k as f64;

        // Copy vertices with offset
        for v in &mesh.vertices {
            combined.vertices.push(Vertex {
                position: [v.position[0] + dx, v.position[1] + dy, v.position[2] + dz],
                half_edge: v.half_edge.map(|he| he + he_offset),
            });
        }

        // Copy half-edges with remapped indices
        for he in &mesh.half_edges {
            combined.half_edges.push(HalfEdge {
                vertex: he.vertex + v_offset,
                face: he.face.map(|f| f + f_offset),
                twin: if he.twin == usize::MAX {
                    usize::MAX
                } else {
                    he.twin + he_offset
                },
                next: if he.next == usize::MAX {
                    usize::MAX
                } else {
                    he.next + he_offset
                },
                prev: if he.prev == usize::MAX {
                    usize::MAX
                } else {
                    he.prev + he_offset
                },
            });
        }

        // Copy faces
        for face in &mesh.faces {
            combined.faces.push(Face {
                half_edge: face.half_edge + he_offset,
            });
        }
    }

    combined
}
