use super::half_edge::HalfEdgeMesh;

/// Epsilon for vertex welding — positions within this distance are merged.
const WELD_EPSILON: f64 = 1e-6;

// ── Vector math helpers ──────────────────────────────────────────────

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn lerp(a: [f64; 3], b: [f64; 3], t: f64) -> [f64; 3] {
    [
        a[0] + t * (b[0] - a[0]),
        a[1] + t * (b[1] - a[1]),
        a[2] + t * (b[2] - a[2]),
    ]
}

fn midpoint(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    lerp(a, b, 0.5)
}

// ── Mesh builder with vertex welding ─────────────────────────────────

struct MeshBuilder {
    positions: Vec<[f64; 3]>,
    indices: Vec<usize>,
}

impl MeshBuilder {
    fn new() -> Self {
        Self {
            positions: Vec::new(),
            indices: Vec::new(),
        }
    }

    /// Find or insert a vertex position, merging within `WELD_EPSILON`.
    fn vertex(&mut self, p: [f64; 3]) -> usize {
        let eps2 = WELD_EPSILON * WELD_EPSILON;
        for (i, pos) in self.positions.iter().enumerate() {
            let dx = pos[0] - p[0];
            let dy = pos[1] - p[1];
            let dz = pos[2] - p[2];
            if dx * dx + dy * dy + dz * dz < eps2 {
                return i;
            }
        }
        let idx = self.positions.len();
        self.positions.push(p);
        idx
    }

    /// Add a triangle (skips degenerate triangles where welding collapsed vertices).
    fn triangle(&mut self, a: [f64; 3], b: [f64; 3], c: [f64; 3]) {
        let ia = self.vertex(a);
        let ib = self.vertex(b);
        let ic = self.vertex(c);
        if ia == ib || ib == ic || ic == ia {
            return;
        }
        self.indices.extend_from_slice(&[ia, ib, ic]);
    }

    fn build(self) -> HalfEdgeMesh {
        HalfEdgeMesh::from_triangles(&self.positions, &self.indices)
    }
}

// ── Ray-triangle intersection (Moller-Trumbore) ─────────────────────

/// Returns the ray parameter `t` where the ray hits the triangle, or `None`.
/// Ray: `origin + t * dir`. Only returns `t > epsilon`.
fn ray_triangle(
    origin: [f64; 3],
    dir: [f64; 3],
    v0: [f64; 3],
    v1: [f64; 3],
    v2: [f64; 3],
) -> Option<f64> {
    let edge1 = sub(v1, v0);
    let edge2 = sub(v2, v0);
    let h = cross(dir, edge2);
    let a = dot(edge1, h);
    if a.abs() < 1e-10 {
        return None; // parallel
    }
    let f = 1.0 / a;
    let s = sub(origin, v0);
    let u = f * dot(s, h);
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = cross(s, edge1);
    let v = f * dot(dir, q);
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = f * dot(edge2, q);
    if t > 1e-10 { Some(t) } else { None }
}

// ── Point-in-mesh via ray casting ────────────────────────────────────

/// Test whether a point is inside a closed triangle mesh using ray casting.
/// Casts a jittered ray and counts crossings (odd = inside).
fn point_in_mesh(point: &[f64; 3], triangles: &[[[f64; 3]; 3]]) -> bool {
    // Slightly off-axis to avoid edge/vertex coincidences
    let dir = [1.0, 0.000_131, 0.000_071];
    let mut count = 0u32;
    for tri in triangles {
        if ray_triangle(*point, dir, tri[0], tri[1], tri[2]).is_some() {
            count += 1;
        }
    }
    count % 2 == 1
}

// ── Edge-mesh intersection ──────────────────────────────────────────

/// Find the first intersection of segment `a→b` with a triangle mesh.
/// Returns `t` in `(0, 1)` where the intersection lies on the segment.
fn edge_mesh_intersection(a: [f64; 3], b: [f64; 3], triangles: &[[[f64; 3]; 3]]) -> Option<f64> {
    let dir = sub(b, a);
    let mut best_t = f64::MAX;
    for tri in triangles {
        if let Some(t) = ray_triangle(a, dir, tri[0], tri[1], tri[2])
            && t > 1e-6
            && t < (1.0 - 1e-6)
            && t < best_t
        {
            best_t = t;
        }
    }
    if best_t < f64::MAX {
        Some(best_t)
    } else {
        None
    }
}

// ── Triangle extraction ─────────────────────────────────────────────

/// Extract all faces as flat triangle arrays, applying an offset to positions.
fn extract_triangles(mesh: &HalfEdgeMesh, offset: [f64; 3]) -> Vec<[[f64; 3]; 3]> {
    let mut tris = Vec::with_capacity(mesh.faces.len());
    for f in 0..mesh.faces.len() {
        let verts = mesh.face_vertices(f);
        if verts.len() < 3 {
            continue;
        }
        // Fan triangulation for polygons with > 3 vertices
        for i in 1..verts.len() - 1 {
            let p0 = mesh.vertices[verts[0]].position;
            let pi = mesh.vertices[verts[i]].position;
            let pj = mesh.vertices[verts[i + 1]].position;
            tris.push([
                [p0[0] + offset[0], p0[1] + offset[1], p0[2] + offset[2]],
                [pi[0] + offset[0], pi[1] + offset[1], pi[2] + offset[2]],
                [pj[0] + offset[0], pj[1] + offset[1], pj[2] + offset[2]],
            ]);
        }
    }
    tris
}

// ── Straddling face split ───────────────────────────────────────────

/// Split a triangle that straddles the tool boundary, keeping only outside parts.
///
/// Exactly one or two vertices are inside the tool. We find the two cut points
/// on the straddling edges and split into 3 sub-triangles (same pattern as
/// `loop_cut.rs`), keeping only the sub-triangles on the outside.
#[allow(clippy::similar_names)]
fn split_straddling_face(
    p: &[[f64; 3]; 3],
    inside: [bool; 3],
    tool_tris: &[[[f64; 3]; 3]],
    builder: &mut MeshBuilder,
) {
    // Find the solo vertex (the one different from the other two)
    let solo = if inside[0] != inside[1] && inside[0] != inside[2] {
        0
    } else if inside[1] != inside[0] && inside[1] != inside[2] {
        1
    } else {
        2
    };

    let other_a = (solo + 1) % 3;
    let other_b = (solo + 2) % 3;

    // Find cut points on edges from solo to each other vertex
    let cut_a = edge_mesh_intersection(p[solo], p[other_a], tool_tris).map_or_else(
        || midpoint(p[solo], p[other_a]),
        |t| lerp(p[solo], p[other_a], t),
    );

    let cut_b = edge_mesh_intersection(p[solo], p[other_b], tool_tris).map_or_else(
        || midpoint(p[solo], p[other_b]),
        |t| lerp(p[solo], p[other_b], t),
    );

    if inside[solo] {
        // Solo is inside: keep the two outside sub-triangles
        builder.triangle(cut_a, p[other_a], p[other_b]);
        builder.triangle(cut_a, p[other_b], cut_b);
    } else {
        // Solo is outside: keep only the solo sub-triangle
        builder.triangle(p[solo], cut_a, cut_b);
    }
}

// ── Boolean operation modes ──────────────────────────────────────────

/// Which boolean operation to perform.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BooleanMode {
    /// Remove tool volume from target.
    Subtract,
    /// Combine both volumes into one.
    Union,
    /// Keep only the overlapping volume.
    Intersect,
}

// ── Main boolean operation ──────────────────────────────────────────

/// Boolean subtract: remove the volume of `tool` (offset by `offset`) from `target`.
///
/// Convenience wrapper around `boolean_op` with `BooleanMode::Subtract`.
#[cfg(test)]
pub fn subtract(target: &HalfEdgeMesh, tool: &HalfEdgeMesh, offset: [f64; 3]) -> HalfEdgeMesh {
    boolean_op(target, tool, offset, BooleanMode::Subtract)
}

/// Boolean union: combine `target` and `tool` into a single mesh.
///
/// Convenience wrapper around `boolean_op` with `BooleanMode::Union`.
#[allow(dead_code)]
pub fn union(target: &HalfEdgeMesh, tool: &HalfEdgeMesh, offset: [f64; 3]) -> HalfEdgeMesh {
    boolean_op(target, tool, offset, BooleanMode::Union)
}

/// Boolean intersect: keep only the volume shared by `target` and `tool`.
///
/// Convenience wrapper around `boolean_op` with `BooleanMode::Intersect`.
#[allow(dead_code)]
pub fn intersect(target: &HalfEdgeMesh, tool: &HalfEdgeMesh, offset: [f64; 3]) -> HalfEdgeMesh {
    boolean_op(target, tool, offset, BooleanMode::Intersect)
}

/// Generic boolean operation on two meshes.
///
/// Algorithm: split-and-classify (direct triangle splitting at the intersection
/// boundary, then face classification). Follows Blender-style boolean, not CSG.
///
/// - Subtract: keep target-outside, add flipped tool-inside
/// - Union: keep target-outside + tool-outside
/// - Intersect: keep target-inside + tool-inside
#[allow(clippy::too_many_lines)]
pub fn boolean_op(
    target: &HalfEdgeMesh,
    tool: &HalfEdgeMesh,
    offset: [f64; 3],
    mode: BooleanMode,
) -> HalfEdgeMesh {
    if target.faces.is_empty() && tool.faces.is_empty() {
        return HalfEdgeMesh::default();
    }
    if tool.faces.is_empty() {
        return match mode {
            BooleanMode::Subtract | BooleanMode::Union => target.clone(),
            BooleanMode::Intersect => HalfEdgeMesh::default(),
        };
    }
    if target.faces.is_empty() {
        return match mode {
            BooleanMode::Union => {
                // Return tool with offset applied
                offset_mesh(tool, offset)
            }
            _ => HalfEdgeMesh::default(),
        };
    }

    let tool_tris = extract_triangles(tool, offset);
    let target_tris = extract_triangles(target, [0.0; 3]);

    // Classify target vertices as inside/outside tool
    let target_inside: Vec<bool> = target
        .vertices
        .iter()
        .map(|v| point_in_mesh(&v.position, &tool_tris))
        .collect();

    let mut builder = MeshBuilder::new();

    // ── Process target faces ─────────────────────────────────────────
    for f in 0..target.faces.len() {
        let verts = target.face_vertices(f);
        if verts.len() != 3 {
            continue;
        }

        let (v0, v1, v2) = (verts[0], verts[1], verts[2]);
        let in_flags = [target_inside[v0], target_inside[v1], target_inside[v2]];
        let count_inside = in_flags.iter().filter(|&&b| b).count();

        let keep_outside = mode == BooleanMode::Subtract || mode == BooleanMode::Union;
        let keep_inside = mode == BooleanMode::Intersect;

        match count_inside {
            0 => {
                if keep_outside {
                    builder.triangle(
                        target.vertices[v0].position,
                        target.vertices[v1].position,
                        target.vertices[v2].position,
                    );
                }
            }
            3 => {
                if keep_inside {
                    builder.triangle(
                        target.vertices[v0].position,
                        target.vertices[v1].position,
                        target.vertices[v2].position,
                    );
                }
            }
            _ => {
                let positions = [
                    target.vertices[v0].position,
                    target.vertices[v1].position,
                    target.vertices[v2].position,
                ];
                if keep_outside {
                    split_straddling_face(&positions, in_flags, &tool_tris, &mut builder);
                }
                if keep_inside {
                    // Invert: keep the inside parts instead
                    let inv_flags = [!in_flags[0], !in_flags[1], !in_flags[2]];
                    split_straddling_face(&positions, inv_flags, &tool_tris, &mut builder);
                }
            }
        }
    }

    // ── Process tool faces ───────────────────────────────────────────
    for f in 0..tool.faces.len() {
        let verts = tool.face_vertices(f);
        if verts.len() != 3 {
            continue;
        }

        let positions: Vec<[f64; 3]> = verts
            .iter()
            .map(|&v| {
                let p = tool.vertices[v].position;
                [p[0] + offset[0], p[1] + offset[1], p[2] + offset[2]]
            })
            .collect();

        let centroid = [
            (positions[0][0] + positions[1][0] + positions[2][0]) / 3.0,
            (positions[0][1] + positions[1][1] + positions[2][1]) / 3.0,
            (positions[0][2] + positions[1][2] + positions[2][2]) / 3.0,
        ];

        let inside_target = point_in_mesh(&centroid, &target_tris);

        match mode {
            BooleanMode::Subtract => {
                if inside_target {
                    // Cap the hole: flipped winding for inward-facing cap
                    builder.triangle(positions[2], positions[1], positions[0]);
                }
            }
            BooleanMode::Union => {
                if !inside_target {
                    // Keep tool faces that are outside target (original winding)
                    builder.triangle(positions[0], positions[1], positions[2]);
                }
            }
            BooleanMode::Intersect => {
                if inside_target {
                    // Keep tool faces that are inside target (original winding)
                    builder.triangle(positions[0], positions[1], positions[2]);
                }
            }
        }
    }

    builder.build()
}

/// Return a copy of the mesh with all vertices offset.
fn offset_mesh(mesh: &HalfEdgeMesh, offset: [f64; 3]) -> HalfEdgeMesh {
    let mut result = mesh.clone();
    for v in &mut result.vertices {
        v.position[0] += offset[0];
        v.position[1] += offset[1];
        v.position[2] += offset[2];
    }
    result
}
