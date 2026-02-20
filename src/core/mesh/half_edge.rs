use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Index-based half-edge mesh data structure for topology-aware geometry.
///
/// All connectivity is stored as indices into the respective vectors.
/// No `Rc`/`RefCell` — fully `Clone + Serialize + Deserialize`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct HalfEdgeMesh {
    pub vertices: Vec<Vertex>,
    pub half_edges: Vec<HalfEdge>,
    pub faces: Vec<Face>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Vertex {
    pub position: [f64; 3],
    /// One outgoing half-edge (arbitrary but stable).
    pub half_edge: Option<usize>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HalfEdge {
    /// Vertex this half-edge points TO.
    pub vertex: usize,
    /// Face to the left (`None` = boundary).
    pub face: Option<usize>,
    /// Opposite half-edge.
    pub twin: usize,
    /// Next half-edge around the face (CCW).
    pub next: usize,
    /// Previous half-edge around the face (CCW).
    pub prev: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Face {
    /// Any half-edge on this face's boundary loop.
    pub half_edge: usize,
}

impl HalfEdgeMesh {
    /// Build a half-edge mesh from indexed triangle data.
    ///
    /// `positions`: vertex positions `[x, y, z]`
    /// `indices`: triangle indices (length must be divisible by 3)
    pub fn from_triangles(positions: &[[f64; 3]], indices: &[usize]) -> Self {
        assert!(
            indices.len().is_multiple_of(3),
            "indices length must be divisible by 3"
        );

        let num_faces = indices.len() / 3;

        let mut mesh = Self {
            vertices: positions
                .iter()
                .map(|&p| Vertex {
                    position: p,
                    half_edge: None,
                })
                .collect(),
            half_edges: Vec::with_capacity(indices.len() * 2),
            faces: Vec::with_capacity(num_faces),
        };

        // Map (from_vertex, to_vertex) -> half-edge index for twin finding
        let mut edge_map: HashMap<(usize, usize), usize> = HashMap::new();

        for face_idx in 0..num_faces {
            let base = face_idx * 3;
            let tri = [indices[base], indices[base + 1], indices[base + 2]];

            let f = mesh.faces.len();
            mesh.faces.push(Face {
                half_edge: mesh.half_edges.len(),
            });

            let he_base = mesh.half_edges.len();

            // Create 3 half-edges for this triangle
            for i in 0..3 {
                let from = tri[i];
                let to = tri[(i + 1) % 3];
                let he_idx = he_base + i;

                mesh.half_edges.push(HalfEdge {
                    vertex: to,
                    face: Some(f),
                    twin: usize::MAX, // placeholder
                    next: he_base + (i + 1) % 3,
                    prev: he_base + (i + 2) % 3,
                });

                // Set vertex half-edge if not yet set
                if mesh.vertices[from].half_edge.is_none() {
                    mesh.vertices[from].half_edge = Some(he_idx);
                }

                edge_map.insert((from, to), he_idx);
            }
        }

        // Wire up twins: for each half-edge (u,v) find twin (v,u)
        let num_he = mesh.half_edges.len();
        let mut boundary_edges: Vec<(usize, usize)> = Vec::new();

        for face_idx in 0..num_faces {
            let base = face_idx * 3;
            let tri = [indices[base], indices[base + 1], indices[base + 2]];
            for i in 0..3 {
                let from = tri[i];
                let to = tri[(i + 1) % 3];
                let he_idx = base + i; // half-edge index = face_idx * 3 + i
                if let Some(&twin_idx) = edge_map.get(&(to, from)) {
                    mesh.half_edges[he_idx].twin = twin_idx;
                } else {
                    boundary_edges.push((from, to));
                }
            }
        }

        // Create boundary half-edges for unpaired edges
        for &(from, to) in &boundary_edges {
            if let Some(&interior_he) = edge_map.get(&(from, to)) {
                if mesh.half_edges[interior_he].twin != usize::MAX {
                    continue; // already paired
                }
                let boundary_he_idx = mesh.half_edges.len();
                mesh.half_edges.push(HalfEdge {
                    vertex: from, // boundary goes opposite direction
                    face: None,
                    twin: interior_he,
                    next: usize::MAX, // will be linked below
                    prev: usize::MAX,
                });
                mesh.half_edges[interior_he].twin = boundary_he_idx;

                // Set vertex half-edge for boundary vertex
                if mesh.vertices[to].half_edge.is_none() {
                    mesh.vertices[to].half_edge = Some(boundary_he_idx);
                }
            }
        }

        // Link boundary half-edges into chains (next/prev)
        link_boundary_chains(&mut mesh, num_he);

        mesh
    }

    /// Build a half-edge mesh from polygon data (variable-length faces).
    ///
    /// `positions`: vertex positions `[x, y, z]`
    /// `faces`: slice of face vertex-index slices (triangles, quads, or n-gons)
    pub fn from_polygons(positions: &[[f64; 3]], faces: &[&[usize]]) -> Self {
        let total_he: usize = faces.iter().map(|f| f.len()).sum();

        let mut mesh = Self {
            vertices: positions
                .iter()
                .map(|&p| Vertex {
                    position: p,
                    half_edge: None,
                })
                .collect(),
            half_edges: Vec::with_capacity(total_he * 2),
            faces: Vec::with_capacity(faces.len()),
        };

        let mut edge_map: HashMap<(usize, usize), usize> = HashMap::new();

        for face in faces {
            let n = face.len();
            if n < 3 {
                continue;
            }

            let f = mesh.faces.len();
            mesh.faces.push(Face {
                half_edge: mesh.half_edges.len(),
            });

            let he_base = mesh.half_edges.len();

            for i in 0..n {
                let from = face[i];
                let to = face[(i + 1) % n];
                let he_idx = he_base + i;

                mesh.half_edges.push(HalfEdge {
                    vertex: to,
                    face: Some(f),
                    twin: usize::MAX,
                    next: he_base + (i + 1) % n,
                    prev: he_base + (i + n - 1) % n,
                });

                if mesh.vertices[from].half_edge.is_none() {
                    mesh.vertices[from].half_edge = Some(he_idx);
                }

                edge_map.insert((from, to), he_idx);
            }
        }

        // Wire up twins
        let num_he = mesh.half_edges.len();
        let mut boundary_edges: Vec<(usize, usize)> = Vec::new();

        let mut he_idx = 0;
        for face in faces {
            let n = face.len();
            if n < 3 {
                continue;
            }
            for i in 0..n {
                let from = face[i];
                let to = face[(i + 1) % n];
                if let Some(&twin_idx) = edge_map.get(&(to, from)) {
                    mesh.half_edges[he_idx].twin = twin_idx;
                } else {
                    boundary_edges.push((from, to));
                }
                he_idx += 1;
            }
        }

        // Create boundary half-edges for unpaired edges
        for &(from, to) in &boundary_edges {
            if let Some(&interior_he) = edge_map.get(&(from, to)) {
                if mesh.half_edges[interior_he].twin != usize::MAX {
                    continue;
                }
                let boundary_he_idx = mesh.half_edges.len();
                mesh.half_edges.push(HalfEdge {
                    vertex: from,
                    face: None,
                    twin: interior_he,
                    next: usize::MAX,
                    prev: usize::MAX,
                });
                mesh.half_edges[interior_he].twin = boundary_he_idx;

                if mesh.vertices[to].half_edge.is_none() {
                    mesh.vertices[to].half_edge = Some(boundary_he_idx);
                }
            }
        }

        // Link boundary half-edges into chains
        link_boundary_chains(&mut mesh, num_he);

        mesh
    }

    // ── Adjacency queries ───────────────────────────────────────────

    /// All face indices surrounding a vertex.
    #[allow(dead_code)]
    pub fn vertex_faces(&self, v: usize) -> Vec<usize> {
        let mut faces = Vec::new();
        let Some(start_he) = self.vertices[v].half_edge else {
            return faces;
        };

        let mut he = start_he;
        loop {
            if let Some(f) = self.half_edges[he].face {
                faces.push(f);
            }
            // Move to next outgoing half-edge from v (CCW): twin → next
            let twin = self.half_edges[he].twin;
            if twin >= self.half_edges.len() {
                break;
            }
            let next = self.half_edges[twin].next;
            if next == usize::MAX || next >= self.half_edges.len() || next == start_he {
                break;
            }
            he = next;
            // Safety: prevent infinite loop on broken meshes
            if faces.len() > self.faces.len() {
                break;
            }
        }
        faces
    }

    /// Vertex indices of a face, in order.
    pub fn face_vertices(&self, f: usize) -> Vec<usize> {
        let mut verts = Vec::new();
        let start_he = self.faces[f].half_edge;
        let mut he = start_he;
        loop {
            // The vertex this half-edge points to
            verts.push(self.half_edges[he].vertex);
            he = self.half_edges[he].next;
            if he == start_he {
                break;
            }
            if verts.len() > self.half_edges.len() {
                break;
            }
        }
        verts
    }

    /// Neighboring face indices (faces sharing an edge).
    #[allow(dead_code)]
    pub fn face_neighbors(&self, f: usize) -> Vec<usize> {
        let mut neighbors = Vec::new();
        let start_he = self.faces[f].half_edge;
        let mut he = start_he;
        loop {
            let twin = self.half_edges[he].twin;
            if let Some(neighbor_face) = self.half_edges[twin].face {
                neighbors.push(neighbor_face);
            }
            he = self.half_edges[he].next;
            if he == start_he {
                break;
            }
            if neighbors.len() > self.half_edges.len() {
                break;
            }
        }
        neighbors
    }

    /// Half-edge indices on the mesh boundary (face == None).
    #[allow(dead_code)]
    pub fn boundary_edges(&self) -> Vec<usize> {
        self.half_edges
            .iter()
            .enumerate()
            .filter(|(_, he)| he.face.is_none())
            .map(|(i, _)| i)
            .collect()
    }

    /// Half-edge from vertex `from` to vertex `to`, if it exists.
    #[allow(dead_code)]
    pub fn find_half_edge(&self, from: usize, to: usize) -> Option<usize> {
        let start_he = self.vertices[from].half_edge?;
        let mut he = start_he;
        let mut limit = self.half_edges.len();
        loop {
            if self.half_edges[he].vertex == to {
                return Some(he);
            }
            // Move to next outgoing half-edge from `from` (CCW): twin → next
            let twin = self.half_edges[he].twin;
            let next = self.half_edges[twin].next;
            if next == usize::MAX || next == start_he {
                break;
            }
            he = next;
            limit -= 1;
            if limit == 0 {
                break;
            }
        }
        None
    }

    /// Flip a face's winding order (reverses half-edge cycle).
    ///
    /// WARNING: This corrupts twin relationships between adjacent faces.
    /// Only use for isolated test scenarios. For production code, use
    /// `normals::flip_all` or `normals::flip_caps` which rebuild the mesh.
    #[cfg(test)]
    pub fn flip_face(&mut self, f: usize) {
        let verts = self.face_vertices(f);
        if verts.len() < 3 {
            return;
        }

        // Collect all half-edges of this face
        let mut hes = Vec::new();
        let start_he = self.faces[f].half_edge;
        let mut he = start_he;
        loop {
            hes.push(he);
            he = self.half_edges[he].next;
            if he == start_he {
                break;
            }
        }

        // Reverse the cycle: swap next/prev, and shift vertex assignments
        let n = hes.len();
        // Store original vertices (each he points TO a vertex)
        let orig_verts: Vec<usize> = hes.iter().map(|&h| self.half_edges[h].vertex).collect();

        for i in 0..n {
            let h = hes[i];
            // Swap next and prev
            let he = &mut self.half_edges[h];
            std::mem::swap(&mut he.next, &mut he.prev);
            // Shift vertex: after reversal, he[i] should point to what was he[i-1]'s target
            self.half_edges[h].vertex = orig_verts[(i + n - 1) % n];
        }
    }

    /// Split an edge at its midpoint, returning the new vertex index.
    #[allow(dead_code)]
    pub fn split_edge(&mut self, he_idx: usize) -> usize {
        let twin_idx = self.half_edges[he_idx].twin;
        let v_from = self.half_edges[self.half_edges[he_idx].prev].vertex;
        let v_to = self.half_edges[he_idx].vertex;

        // Midpoint
        let mid = [
            (self.vertices[v_from].position[0] + self.vertices[v_to].position[0]) * 0.5,
            (self.vertices[v_from].position[1] + self.vertices[v_to].position[1]) * 0.5,
            (self.vertices[v_from].position[2] + self.vertices[v_to].position[2]) * 0.5,
        ];

        let new_v = self.vertices.len();
        self.vertices.push(Vertex {
            position: mid,
            half_edge: Some(he_idx),
        });

        // Insert new half-edges to split the edge
        let new_he = self.half_edges.len();
        let new_twin = new_he + 1;

        // New half-edge continues from midpoint to original target
        self.half_edges.push(HalfEdge {
            vertex: v_to,
            face: self.half_edges[he_idx].face,
            twin: new_twin,
            next: self.half_edges[he_idx].next,
            prev: he_idx,
        });

        // New twin continues from midpoint back
        self.half_edges.push(HalfEdge {
            vertex: self.half_edges[twin_idx].vertex,
            face: self.half_edges[twin_idx].face,
            twin: new_he,
            next: self.half_edges[twin_idx].next,
            prev: twin_idx,
        });

        // Update original half-edge to point to midpoint
        self.half_edges[he_idx].vertex = new_v;
        self.half_edges[he_idx].next = new_he;

        // Update original twin
        self.half_edges[twin_idx].vertex = new_v;
        self.half_edges[twin_idx].next = new_twin;

        // Fix prev pointers of the successors
        let new_he_next = self.half_edges[new_he].next;
        self.half_edges[new_he_next].prev = new_he;
        let new_twin_next = self.half_edges[new_twin].next;
        self.half_edges[new_twin_next].prev = new_twin;

        // Set vertex half-edge
        self.vertices[new_v].half_edge = Some(he_idx);

        new_v
    }

    // ── Export ───────────────────────────────────────────────────────

    /// Export with shading mode: Smooth, Flat, or AutoSmooth.
    pub fn to_arrays_shaded(&self, mode: super::ShadingMode) -> (Vec<f64>, Vec<f64>, Vec<u32>) {
        match mode {
            super::ShadingMode::Smooth => self.to_arrays(),
            super::ShadingMode::Flat => self.to_arrays_flat(),
            super::ShadingMode::AutoSmooth(angle) => self.to_arrays_auto_smooth(angle),
        }
    }

    /// Export as flat arrays with smooth (averaged vertex) normals.
    /// Returns `(positions, normals, indices)`.
    pub fn to_arrays(&self) -> (Vec<f64>, Vec<f64>, Vec<u32>) {
        let normals = super::normals::compute_vertex_normals(self);

        let mut positions = Vec::with_capacity(self.vertices.len() * 3);
        let mut normal_data = Vec::with_capacity(self.vertices.len() * 3);

        for (i, v) in self.vertices.iter().enumerate() {
            positions.extend_from_slice(&v.position);
            if i < normals.len() {
                normal_data.extend_from_slice(&normals[i]);
            } else {
                normal_data.extend_from_slice(&[0.0, 1.0, 0.0]);
            }
        }

        // Godot uses CW front-face winding (cross product points inward for front
        // faces).  Our internal representation is CCW (cross product outward), so we
        // swap the last two indices of every triangle to match Godot's convention.
        let mut indices = Vec::with_capacity(self.faces.len() * 3);
        for face in &self.faces {
            let verts = self.face_vertices_from_he(face.half_edge);
            if verts.len() == 3 {
                indices.push(verts[0] as u32);
                indices.push(verts[2] as u32);
                indices.push(verts[1] as u32);
            } else if verts.len() > 3 {
                for i in 1..verts.len() - 1 {
                    indices.push(verts[0] as u32);
                    indices.push(verts[i + 1] as u32);
                    indices.push(verts[i] as u32);
                }
            }
        }

        (positions, normal_data, indices)
    }

    /// Export with flat shading: duplicate vertices per face, each gets face normal.
    fn to_arrays_flat(&self) -> (Vec<f64>, Vec<f64>, Vec<u32>) {
        let mut positions = Vec::with_capacity(self.faces.len() * 9);
        let mut normal_data = Vec::with_capacity(self.faces.len() * 9);
        let mut indices = Vec::with_capacity(self.faces.len() * 3);
        let mut vi = 0u32;

        for f in 0..self.faces.len() {
            let verts = self.face_vertices_from_he(self.faces[f].half_edge);
            let face_n = super::normals::compute_face_normal(self, f);

            // CW winding for Godot: swap last two vertices per triangle
            if verts.len() == 3 {
                for &v in &[verts[0], verts[2], verts[1]] {
                    positions.extend_from_slice(&self.vertices[v].position);
                    normal_data.extend_from_slice(&face_n);
                    indices.push(vi);
                    vi += 1;
                }
            } else if verts.len() > 3 {
                for i in 1..verts.len() - 1 {
                    for &v in &[verts[0], verts[i + 1], verts[i]] {
                        positions.extend_from_slice(&self.vertices[v].position);
                        normal_data.extend_from_slice(&face_n);
                        indices.push(vi);
                        vi += 1;
                    }
                }
            }
        }

        (positions, normal_data, indices)
    }

    /// Export with auto-smooth: smooth below angle threshold, flat above.
    ///
    /// Vertices at sharp edges (dihedral angle > threshold) are split so each
    /// face gets its own copy with the face normal. Vertices at smooth edges
    /// share averaged normals.
    fn to_arrays_auto_smooth(&self, angle_deg: f64) -> (Vec<f64>, Vec<f64>, Vec<u32>) {
        let cos_threshold = angle_deg.to_radians().cos();
        let face_normals: Vec<[f64; 3]> = (0..self.faces.len())
            .map(|f| super::normals::compute_face_normal(self, f))
            .collect();

        // For each (vertex, face) pair, determine which smooth group it belongs to.
        // Two adjacent faces sharing a vertex are in the same smooth group if
        // their dihedral angle < threshold.
        //
        // Simple approach: for each face, emit vertices. If an edge is smooth,
        // share the vertex index; if sharp, create a new one.
        // Easier: just go full-flat then merge smooth vertices.
        //
        // Simplest correct approach: per-vertex, accumulate normals only from
        // faces in the same smooth group (connected via smooth edges).

        // Build vertex→face adjacency
        let mut vert_faces: Vec<Vec<usize>> = vec![Vec::new(); self.vertices.len()];
        for f in 0..self.faces.len() {
            for &v in &self.face_vertices(f) {
                vert_faces[v].push(f);
            }
        }

        // For each vertex, compute smooth-group normal: average of adjacent face
        // normals that are within the angle threshold of each other (transitive).
        // We use a simple greedy approach: for each (vertex, face), the normal is
        // the average of all adjacent face normals within threshold of this face.
        let mut vert_face_normal: HashMap<(usize, usize), [f64; 3]> = HashMap::new();

        for (v, faces) in vert_faces.iter().enumerate() {
            for &f in faces {
                let mut nx = 0.0_f64;
                let mut ny = 0.0_f64;
                let mut nz = 0.0_f64;
                for &f2 in faces {
                    let dot = face_normals[f][0] * face_normals[f2][0]
                        + face_normals[f][1] * face_normals[f2][1]
                        + face_normals[f][2] * face_normals[f2][2];
                    if dot >= cos_threshold {
                        nx += face_normals[f2][0];
                        ny += face_normals[f2][1];
                        nz += face_normals[f2][2];
                    }
                }
                let len = (nx * nx + ny * ny + nz * nz).sqrt();
                let normal = if len > 1e-12 {
                    [nx / len, ny / len, nz / len]
                } else {
                    face_normals[f]
                };
                vert_face_normal.insert((v, f), normal);
            }
        }

        // Now emit vertices: for each face, look up the (vertex, face) normal.
        // Vertices with the same position+normal get merged.
        let mut positions = Vec::new();
        let mut normal_data = Vec::new();
        let mut indices = Vec::new();
        let mut vert_map: HashMap<(usize, [i64; 3]), u32> = HashMap::new();

        for (f, face) in self.faces.iter().enumerate() {
            let verts = self.face_vertices_from_he(face.half_edge);
            let emit_tri =
                |vlist: &[usize],
                 positions: &mut Vec<f64>,
                 normal_data: &mut Vec<f64>,
                 indices: &mut Vec<u32>,
                 vert_map: &mut HashMap<(usize, [i64; 3]), u32>,
                 vert_face_normal: &HashMap<(usize, usize), [f64; 3]>| {
                    for &v in vlist {
                        let n = vert_face_normal
                            .get(&(v, f))
                            .copied()
                            .unwrap_or(face_normals[f]);
                        // Quantize normal to detect sharing (6 decimal places)
                        let nq = [
                            (n[0] * 1_000_000.0) as i64,
                            (n[1] * 1_000_000.0) as i64,
                            (n[2] * 1_000_000.0) as i64,
                        ];
                        let key = (v, nq);
                        if let Some(&idx) = vert_map.get(&key) {
                            indices.push(idx);
                        } else {
                            let idx = (positions.len() / 3) as u32;
                            positions.extend_from_slice(&self.vertices[v].position);
                            normal_data.extend_from_slice(&n);
                            vert_map.insert(key, idx);
                            indices.push(idx);
                        }
                    }
                };

            // CW winding for Godot: swap last two vertices per triangle
            if verts.len() == 3 {
                let cw = [verts[0], verts[2], verts[1]];
                emit_tri(
                    &cw,
                    &mut positions,
                    &mut normal_data,
                    &mut indices,
                    &mut vert_map,
                    &vert_face_normal,
                );
            } else if verts.len() > 3 {
                for i in 1..verts.len() - 1 {
                    let tri = [verts[0], verts[i + 1], verts[i]];
                    emit_tri(
                        &tri,
                        &mut positions,
                        &mut normal_data,
                        &mut indices,
                        &mut vert_map,
                        &vert_face_normal,
                    );
                }
            }
        }

        (positions, normal_data, indices)
    }

    /// Face vertices from a starting half-edge (avoids face index lookup).
    fn face_vertices_from_he(&self, start_he: usize) -> Vec<usize> {
        let mut verts = Vec::new();
        let mut he = start_he;
        loop {
            verts.push(self.half_edges[he].vertex);
            he = self.half_edges[he].next;
            if he == start_he {
                break;
            }
            if verts.len() > self.half_edges.len() {
                break;
            }
        }
        verts
    }

    /// Compute the axis-aligned bounding box: `(min, max)`.
    pub fn aabb(&self) -> ([f64; 3], [f64; 3]) {
        if self.vertices.is_empty() {
            return ([0.0; 3], [0.0; 3]);
        }
        let mut min = [f64::MAX; 3];
        let mut max = [f64::MIN; 3];
        for v in &self.vertices {
            for i in 0..3 {
                if v.position[i] < min[i] {
                    min[i] = v.position[i];
                }
                if v.position[i] > max[i] {
                    max[i] = v.position[i];
                }
            }
        }
        (min, max)
    }

    /// Number of faces (triangles, quads, or n-gons).
    pub fn face_count(&self) -> usize {
        self.faces.len()
    }

    /// Number of vertices.
    pub fn vertex_count(&self) -> usize {
        self.vertices.len()
    }
}

/// Link boundary half-edges into proper next/prev chains.
///
/// Each boundary half-edge has an origin (where it starts) and a target (vertex field).
/// The origin is determined from the twin interior half-edge: if interior goes A→B,
/// the boundary twin goes B→A, so its origin = B = twin.vertex.
fn link_boundary_chains(mesh: &mut HalfEdgeMesh, interior_count: usize) {
    // Map: origin vertex of boundary half-edge → boundary half-edge index
    let mut by_origin: HashMap<usize, usize> = HashMap::new();

    for i in interior_count..mesh.half_edges.len() {
        let twin_idx = mesh.half_edges[i].twin;
        let origin = mesh.half_edges[twin_idx].vertex;
        by_origin.insert(origin, i);
    }

    // Link: boundary he's next = boundary he whose origin matches our target
    for i in interior_count..mesh.half_edges.len() {
        let target = mesh.half_edges[i].vertex;
        if let Some(&next_he) = by_origin.get(&target) {
            mesh.half_edges[i].next = next_he;
            mesh.half_edges[next_he].prev = i;
        }
    }
}
