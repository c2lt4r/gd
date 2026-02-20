use super::half_edge::HalfEdgeMesh;
use super::normals::compute_face_normal;

/// Inset all faces of a mesh by moving each vertex toward the face centroid.
///
/// `factor`: how far to inset (0.0 = no change, 1.0 = collapse to centroid).
/// `selected`: if `Some`, only inset those face indices; unselected faces pass through.
/// Returns a new mesh with the original faces shrunk and quad strips connecting
/// the original boundary to the inset boundary.
pub fn inset(mesh: &HalfEdgeMesh, factor: f64) -> HalfEdgeMesh {
    inset_selected(mesh, factor, None)
}

pub fn inset_selected(
    mesh: &HalfEdgeMesh,
    factor: f64,
    selected: Option<&[usize]>,
) -> HalfEdgeMesh {
    if mesh.faces.is_empty() || factor <= 0.0 {
        return mesh.clone();
    }

    let factor = factor.min(0.99);

    let mut positions: Vec<[f64; 3]> = mesh.vertices.iter().map(|v| v.position).collect();
    let mut poly_faces: Vec<Vec<usize>> = Vec::new();

    for fi in 0..mesh.faces.len() {
        let verts = mesh.face_vertices(fi);
        if verts.len() < 3 {
            continue;
        }

        // If a selection is active and this face is not in it, pass through unchanged
        if let Some(sel) = selected
            && !sel.contains(&fi)
        {
            poly_faces.push(verts);
            continue;
        }

        // Compute centroid
        let n = verts.len() as f64;
        let mut cx = 0.0;
        let mut cy = 0.0;
        let mut cz = 0.0;
        for &vi in &verts {
            let p = mesh.vertices[vi].position;
            cx += p[0];
            cy += p[1];
            cz += p[2];
        }
        let centroid = [cx / n, cy / n, cz / n];

        // Create inset vertices
        let inset_start = positions.len();
        for &vi in &verts {
            let p = mesh.vertices[vi].position;
            positions.push([
                p[0] + (centroid[0] - p[0]) * factor,
                p[1] + (centroid[1] - p[1]) * factor,
                p[2] + (centroid[2] - p[2]) * factor,
            ]);
        }

        // Face normal for winding check
        let face_n = compute_face_normal(mesh, fi);

        // Inner face: preserve original face size (tri stays tri, quad stays quad)
        let inner: Vec<usize> = (0..verts.len()).map(|i| inset_start + i).collect();
        if inner.len() >= 3 {
            let trial = tri_normal(
                positions[inner[0]],
                positions[inner[1]],
                positions[inner[2]],
            );
            let same_dir = dot(trial, face_n) > 0.0;

            if same_dir {
                poly_faces.push(inner);
            } else {
                let mut reversed = inner;
                reversed.reverse();
                poly_faces.push(reversed);
            }
        }

        // Quad strip: emit quads connecting outer to inner boundary
        let nv = verts.len();
        for i in 0..nv {
            let j = (i + 1) % nv;
            let outer_i = verts[i];
            let outer_j = verts[j];
            let inner_i = inset_start + i;
            let inner_j = inset_start + j;

            let q_normal = tri_normal(positions[outer_i], positions[outer_j], positions[inner_j]);
            if dot(q_normal, face_n) > 0.0 {
                poly_faces.push(vec![outer_i, outer_j, inner_j, inner_i]);
            } else {
                poly_faces.push(vec![outer_i, inner_i, inner_j, outer_j]);
            }
        }
    }

    let face_slices: Vec<&[usize]> = poly_faces.iter().map(Vec::as_slice).collect();
    HalfEdgeMesh::from_polygons(&positions, &face_slices)
}

fn tri_normal(a: [f64; 3], b: [f64; 3], c: [f64; 3]) -> [f64; 3] {
    let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    [
        u[1] * v[2] - u[2] * v[1],
        u[2] * v[0] - u[0] * v[2],
        u[0] * v[1] - u[1] * v[0],
    ]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
