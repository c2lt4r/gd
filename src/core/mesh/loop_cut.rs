use super::half_edge::HalfEdgeMesh;

/// Insert a loop cut: split faces that cross a plane perpendicular to an axis.
///
/// `axis`: 0=X, 1=Y, 2=Z — axis perpendicular to the cut plane.
/// `at`: world-space coordinate along the axis where the cut happens.
///
/// Triangles that cross the plane are split into smaller triangles.
/// Returns the number of triangles that were split.
pub fn loop_cut(mesh: &HalfEdgeMesh, axis: usize, at: f64) -> (HalfEdgeMesh, usize) {
    if mesh.faces.is_empty() {
        return (mesh.clone(), 0);
    }

    let mut positions: Vec<[f64; 3]> = mesh.vertices.iter().map(|v| v.position).collect();
    let mut indices: Vec<usize> = Vec::new();
    let mut splits = 0;

    for face in &mesh.faces {
        let verts = face_verts(mesh, face.half_edge);
        if verts.len() != 3 {
            // Non-triangle: keep as-is
            for i in 1..verts.len() - 1 {
                indices.push(verts[0]);
                indices.push(verts[i]);
                indices.push(verts[i + 1]);
            }
            continue;
        }

        let v0 = verts[0];
        let v1 = verts[1];
        let v2 = verts[2];

        let p0 = mesh.vertices[v0].position[axis];
        let p1 = mesh.vertices[v1].position[axis];
        let p2 = mesh.vertices[v2].position[axis];

        // Classify vertices relative to cut plane
        let s0 = (p0 - at).signum();
        let s1 = (p1 - at).signum();
        let s2 = (p2 - at).signum();

        // Count how many edges cross the plane
        // signum() returns exactly -1.0, 0.0, or 1.0 — exact comparison is correct
        #[allow(clippy::float_cmp)]
        let crossings = [
            s0 != s1 && s0 != 0.0 && s1 != 0.0,
            s1 != s2 && s1 != 0.0 && s2 != 0.0,
            s2 != s0 && s2 != 0.0 && s0 != 0.0,
        ];
        let cross_count = crossings.iter().filter(|&&c| c).count();

        if cross_count < 2 {
            // No cut needed — keep original triangle
            indices.extend_from_slice(&[v0, v1, v2]);
            continue;
        }

        // Split the triangle: find intersection points
        let edges = [(v0, v1, p0, p1), (v1, v2, p1, p2), (v2, v0, p2, p0)];
        let mut cut_points: Vec<(usize, usize, usize)> = Vec::new(); // (edge_idx, new_vert)

        for (edge_idx, &(va, vb, pa, pb)) in edges.iter().enumerate() {
            let sa = (pa - at).signum();
            let sb = (pb - at).signum();
            // signum() returns exactly -1.0, 0.0, or 1.0
            #[allow(clippy::float_cmp)]
            let crosses = sa != sb && sa != 0.0 && sb != 0.0;
            if crosses {
                let t = (at - pa) / (pb - pa);
                let pos_a = mesh.vertices[va].position;
                let pos_b = mesh.vertices[vb].position;
                let new_pos = [
                    pos_a[0] + t * (pos_b[0] - pos_a[0]),
                    pos_a[1] + t * (pos_b[1] - pos_a[1]),
                    pos_a[2] + t * (pos_b[2] - pos_a[2]),
                ];
                let new_idx = positions.len();
                positions.push(new_pos);
                cut_points.push((edge_idx, va, new_idx));
            }
        }

        if cut_points.len() == 2 {
            // Standard case: one vertex on one side, two on the other
            let tri_verts = [v0, v1, v2];
            let (e0, _va0, m0) = cut_points[0];
            let (_e1, _va1, m1) = cut_points[1];

            // Find the solo vertex (the one between the two cut edges)
            let solo = (e0 + 1) % 3;
            let solo_v = tri_verts[solo];
            let other_a = tri_verts[(solo + 1) % 3];
            let other_b = tri_verts[(solo + 2) % 3];

            // Determine which cut point is on which edge
            let (near_solo_m, far_solo_m) = if e0 == solo || e0 == (solo + 2) % 3 {
                (m0, m1)
            } else {
                (m1, m0)
            };

            // Split into 3 triangles
            indices.extend_from_slice(&[solo_v, near_solo_m, far_solo_m]);
            indices.extend_from_slice(&[near_solo_m, other_a, other_b]);
            indices.extend_from_slice(&[near_solo_m, other_b, far_solo_m]);

            splits += 1;
        } else {
            // Fallback: keep original triangle
            indices.extend_from_slice(&[v0, v1, v2]);
        }
    }

    (HalfEdgeMesh::from_triangles(&positions, &indices), splits)
}

fn face_verts(mesh: &HalfEdgeMesh, start_he: usize) -> Vec<usize> {
    let mut verts = Vec::new();
    let mut he = start_he;
    loop {
        verts.push(mesh.half_edges[he].vertex);
        he = mesh.half_edges[he].next;
        if he == start_he || verts.len() > mesh.half_edges.len() {
            break;
        }
    }
    verts
}
