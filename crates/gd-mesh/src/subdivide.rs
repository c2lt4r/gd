use std::collections::HashMap;

use super::half_edge::HalfEdgeMesh;

/// Loop subdivision: split each triangle into 4 sub-triangles with
/// smooth weighted midpoints.
///
/// For each iteration:
/// 1. Insert a new vertex at the midpoint of each edge
/// 2. Reposition original vertices using Loop's weighting scheme
/// 3. Connect new vertices to form 4 sub-triangles per original triangle
///
/// Returns a new mesh (does not modify in place) for each iteration.
pub fn subdivide(mesh: &HalfEdgeMesh, iterations: u32) -> HalfEdgeMesh {
    let mut result = mesh.clone();
    for _ in 0..iterations {
        result = subdivide_once(&result);
    }
    result
}

fn subdivide_once(mesh: &HalfEdgeMesh) -> HalfEdgeMesh {
    if mesh.faces.is_empty() {
        return mesh.clone();
    }

    let n_verts = mesh.vertices.len();

    // Step 1: Compute new edge midpoint positions
    // Map (min_v, max_v) -> new vertex index
    let mut edge_verts: HashMap<(usize, usize), usize> = HashMap::new();
    let mut new_positions: Vec<[f64; 3]> = mesh.vertices.iter().map(|v| v.position).collect();

    for (i, he) in mesh.half_edges.iter().enumerate() {
        if he.twin >= mesh.half_edges.len() || i >= he.twin {
            continue; // Skip boundary sentinels and process each edge only once
        }
        let twin = &mesh.half_edges[he.twin];

        let v0 = mesh.half_edges[he.prev].vertex;
        let v1 = he.vertex;

        let key = if v0 < v1 { (v0, v1) } else { (v1, v0) };
        if edge_verts.contains_key(&key) {
            continue;
        }

        // Loop subdivision edge point:
        // For interior edges: 3/8 * (v0 + v1) + 1/8 * (v_left + v_right)
        // For boundary edges: 1/2 * (v0 + v1)
        let p0 = mesh.vertices[v0].position;
        let p1 = mesh.vertices[v1].position;

        let new_pos = if he.face.is_some() && twin.face.is_some() {
            // Interior edge: use Loop weights
            // Find the opposite vertices in each face
            let v_left = find_opposite_vertex(mesh, he);
            let v_right = find_opposite_vertex(mesh, twin);
            let pl = mesh.vertices[v_left].position;
            let pr = mesh.vertices[v_right].position;

            [
                3.0 / 8.0 * (p0[0] + p1[0]) + 1.0 / 8.0 * (pl[0] + pr[0]),
                3.0 / 8.0 * (p0[1] + p1[1]) + 1.0 / 8.0 * (pl[1] + pr[1]),
                3.0 / 8.0 * (p0[2] + p1[2]) + 1.0 / 8.0 * (pl[2] + pr[2]),
            ]
        } else {
            // Boundary edge: simple midpoint
            [
                (p0[0] + p1[0]) * 0.5,
                (p0[1] + p1[1]) * 0.5,
                (p0[2] + p1[2]) * 0.5,
            ]
        };

        let new_idx = new_positions.len();
        new_positions.push(new_pos);
        edge_verts.insert(key, new_idx);
    }

    // Step 2: Reposition original vertices using Loop weights
    // For interior vertex with n neighbors: (1 - n*beta) * v + beta * sum(neighbors)
    // beta = (5/8 - (3/8 + 1/4 * cos(2*PI/n))^2) / n
    let mut updated_positions = new_positions.clone();
    #[allow(clippy::needless_range_loop)]
    for v in 0..n_verts {
        let neighbors = vertex_neighbors(mesh, v);
        let n = neighbors.len();
        if n == 0 {
            continue;
        }

        let is_boundary = mesh.vertices[v]
            .half_edge
            .is_some_and(|he| is_boundary_vertex(mesh, he));

        if is_boundary {
            // Boundary vertex: keep at original position (simplified)
            continue;
        }

        let nf = n as f64;
        let beta = if n == 3 { 3.0 / 16.0 } else { 3.0 / (8.0 * nf) };

        let p = mesh.vertices[v].position;
        let mut sum = [0.0; 3];
        for &nb in &neighbors {
            let np = mesh.vertices[nb].position;
            sum[0] += np[0];
            sum[1] += np[1];
            sum[2] += np[2];
        }

        updated_positions[v] = [
            (1.0 - nf * beta) * p[0] + beta * sum[0],
            (1.0 - nf * beta) * p[1] + beta * sum[1],
            (1.0 - nf * beta) * p[2] + beta * sum[2],
        ];
    }

    // Step 3: Build new triangles (4 per original)
    let mut indices: Vec<usize> = Vec::new();

    for face in &mesh.faces {
        let verts = face_vertices(mesh, face.half_edge);
        if verts.len() != 3 {
            // Non-triangle face: keep as-is with fan triangulation
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

        // Get edge midpoint vertices
        let m01 = edge_verts[&edge_key(v0, v1)];
        let m12 = edge_verts[&edge_key(v1, v2)];
        let m20 = edge_verts[&edge_key(v2, v0)];

        // 4 sub-triangles (same winding as original)
        indices.extend_from_slice(&[v0, m01, m20]);
        indices.extend_from_slice(&[m01, v1, m12]);
        indices.extend_from_slice(&[m20, m12, v2]);
        indices.extend_from_slice(&[m01, m12, m20]);
    }

    HalfEdgeMesh::from_triangles(&updated_positions, &indices)
}

fn edge_key(v0: usize, v1: usize) -> (usize, usize) {
    if v0 < v1 { (v0, v1) } else { (v1, v0) }
}

/// Find the vertex opposite to a half-edge's edge in its face.
fn find_opposite_vertex(mesh: &HalfEdgeMesh, he: &super::half_edge::HalfEdge) -> usize {
    // The opposite vertex is the one that's neither the start nor end of this half-edge
    let next = &mesh.half_edges[he.next];
    next.vertex
}

/// Get the face vertices from a starting half-edge.
fn face_vertices(mesh: &HalfEdgeMesh, start_he: usize) -> Vec<usize> {
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

/// Get all neighbor vertex indices of a given vertex.
fn vertex_neighbors(mesh: &HalfEdgeMesh, v: usize) -> Vec<usize> {
    let mut neighbors = Vec::new();
    let Some(start_he) = mesh.vertices[v].half_edge else {
        return neighbors;
    };

    let mut he = start_he;
    loop {
        neighbors.push(mesh.half_edges[he].vertex);
        let twin = mesh.half_edges[he].twin;
        if twin >= mesh.half_edges.len() {
            break;
        }
        let next = mesh.half_edges[twin].next;
        if next >= mesh.half_edges.len() || next == start_he {
            break;
        }
        he = next;
        if neighbors.len() > mesh.vertices.len() {
            break;
        }
    }
    neighbors
}

/// Check if a vertex is on the mesh boundary.
fn is_boundary_vertex(mesh: &HalfEdgeMesh, start_he: usize) -> bool {
    let mut he = start_he;
    loop {
        if mesh.half_edges[he].face.is_none() {
            return true;
        }
        let twin = mesh.half_edges[he].twin;
        if twin >= mesh.half_edges.len() {
            return true; // Corrupted twin → treat as boundary
        }
        let next = mesh.half_edges[twin].next;
        if next >= mesh.half_edges.len() || next == start_he {
            break;
        }
        he = next;
    }
    false
}
