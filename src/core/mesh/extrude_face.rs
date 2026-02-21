use std::collections::HashSet;

use super::half_edge::HalfEdgeMesh;
use super::normals::compute_face_normal;
use super::topology::tri_normal;

/// Extrude selected faces along their normals by `depth`.
///
/// For each selected face:
/// 1. Duplicate its vertices and offset along face normal by `depth`.
/// 2. Replace original face with the offset (raised) face.
/// 3. Emit side-wall quads only on boundary edges (where one side is selected
///    and the other is not). Internal edges between two selected faces are
///    skipped to avoid overlapping/degenerate walls.
///
/// Unselected faces pass through unchanged.
pub fn extrude_faces(mesh: &HalfEdgeMesh, depth: f64, selected: &[usize]) -> HalfEdgeMesh {
    if mesh.faces.is_empty() || selected.is_empty() || depth.abs() < 1e-12 {
        return mesh.clone();
    }

    let selected_set: HashSet<usize> = selected.iter().copied().collect();

    // Find internal edges: edges shared by two selected faces.
    // We'll skip side walls for these edges.
    let mut internal_edges: HashSet<(usize, usize)> = HashSet::new();
    for &fi in selected {
        if fi >= mesh.faces.len() {
            continue;
        }
        let start_he = mesh.faces[fi].half_edge;
        let mut he = start_he;
        loop {
            let twin = mesh.half_edges[he].twin;
            if twin < mesh.half_edges.len()
                && let Some(adj_face) = mesh.half_edges[twin].face
                && selected_set.contains(&adj_face)
                && adj_face != fi
            {
                let va = mesh.half_edges[mesh.half_edges[he].prev].vertex;
                let vb = mesh.half_edges[he].vertex;
                let key = if va < vb { (va, vb) } else { (vb, va) };
                internal_edges.insert(key);
            }
            he = mesh.half_edges[he].next;
            if he == start_he {
                break;
            }
        }
    }

    let mut positions: Vec<[f64; 3]> = mesh.vertices.iter().map(|v| v.position).collect();
    let mut poly_faces: Vec<Vec<usize>> = Vec::new();

    for fi in 0..mesh.faces.len() {
        let verts = mesh.face_vertices(fi);
        if verts.len() < 3 {
            continue;
        }

        if !selected_set.contains(&fi) {
            poly_faces.push(verts);
            continue;
        }

        let normal = compute_face_normal(mesh, fi);

        // Create offset vertices
        let offset_start = positions.len();
        for &vi in &verts {
            let p = mesh.vertices[vi].position;
            positions.push([
                p[0] + normal[0] * depth,
                p[1] + normal[1] * depth,
                p[2] + normal[2] * depth,
            ]);
        }

        // Offset (raised) face — same winding as original
        let offset_face: Vec<usize> = (0..verts.len()).map(|i| offset_start + i).collect();
        poly_faces.push(offset_face);

        // Side-wall quads — only on boundary edges
        let nv = verts.len();
        for i in 0..nv {
            let j = (i + 1) % nv;
            let oi = verts[i];
            let oj = verts[j];

            // Skip internal edges (both sides selected)
            let key = if oi < oj { (oi, oj) } else { (oj, oi) };
            if internal_edges.contains(&key) {
                continue;
            }

            let ni = offset_start + i;
            let nj = offset_start + j;

            let trial = tri_normal(positions[oi], positions[oj], positions[nj]);
            let edge_mid = [
                (positions[oi][0] + positions[oj][0]) * 0.5,
                (positions[oi][1] + positions[oj][1]) * 0.5,
                (positions[oi][2] + positions[oj][2]) * 0.5,
            ];
            let face_center = face_centroid(mesh, fi);
            let outward = [
                edge_mid[0] - face_center[0],
                edge_mid[1] - face_center[1],
                edge_mid[2] - face_center[2],
            ];
            let dot = trial[0] * outward[0] + trial[1] * outward[1] + trial[2] * outward[2];

            if dot >= 0.0 {
                poly_faces.push(vec![oi, oj, nj, ni]);
            } else {
                poly_faces.push(vec![oi, ni, nj, oj]);
            }
        }
    }

    let face_slices: Vec<&[usize]> = poly_faces.iter().map(Vec::as_slice).collect();
    HalfEdgeMesh::from_polygons(&positions, &face_slices)
}

fn face_centroid(mesh: &HalfEdgeMesh, fi: usize) -> [f64; 3] {
    let verts = mesh.face_vertices(fi);
    let n = verts.len() as f64;
    let mut c = [0.0; 3];
    for &vi in &verts {
        let p = mesh.vertices[vi].position;
        c[0] += p[0];
        c[1] += p[1];
        c[2] += p[2];
    }
    [c[0] / n, c[1] / n, c[2] / n]
}
