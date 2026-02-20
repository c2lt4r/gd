use super::half_edge::HalfEdgeMesh;

/// Epsilon for vertex welding — positions within this distance are merged.
const WELD_EPSILON: f64 = 1e-6;

/// Epsilon for geometric tests (parallelism, degeneracy, point-on-edge).
const GEO_EPS: f64 = 1e-8;

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

fn len2(v: [f64; 3]) -> f64 {
    dot(v, v)
}

fn dist2(a: [f64; 3], b: [f64; 3]) -> f64 {
    len2(sub(a, b))
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
            if dist2(*pos, p) < eps2 {
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

// ── Ray-triangle intersection (Möller-Trumbore) ─────────────────────

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

// ── Triangle-triangle intersection ──────────────────────────────────

/// Compute where edge `(a, b)` intersects triangle `(v0, v1, v2)`.
///
/// Uses Möller-Trumbore restricted to `t ∈ [0, 1]` (edge, not ray).
/// Returns the exact 3D intersection point, or `None`.
#[allow(clippy::similar_names)]
fn edge_tri_point(
    a: [f64; 3],
    b: [f64; 3],
    v0: [f64; 3],
    v1: [f64; 3],
    v2: [f64; 3],
) -> Option<[f64; 3]> {
    let dir = sub(b, a);
    let e1 = sub(v1, v0);
    let e2 = sub(v2, v0);
    let h = cross(dir, e2);
    let det = dot(e1, h);
    if det.abs() < GEO_EPS {
        return None; // parallel or coplanar
    }
    let inv = 1.0 / det;
    let s = sub(a, v0);
    let u = inv * dot(s, h);
    if !(-GEO_EPS..=1.0 + GEO_EPS).contains(&u) {
        return None;
    }
    let q = cross(s, e1);
    let v = inv * dot(dir, q);
    if v < -GEO_EPS || u + v > 1.0 + GEO_EPS {
        return None;
    }
    let t = inv * dot(e2, q);
    if (-GEO_EPS..=1.0 + GEO_EPS).contains(&t) {
        Some(lerp(a, b, t.clamp(0.0, 1.0)))
    } else {
        None
    }
}

/// Add a point to a collection, deduplicating within a tolerance.
fn push_unique(pts: &mut Vec<[f64; 3]>, p: [f64; 3]) {
    let eps2 = WELD_EPSILON * WELD_EPSILON * 100.0; // 1e-5 radius
    for existing in pts.iter() {
        if dist2(*existing, p) < eps2 {
            return;
        }
    }
    pts.push(p);
}

/// Compute the intersection segment of two triangles, if they properly intersect.
///
/// Finds all points where edges of one triangle pierce the other, then returns
/// the two distinct intersection points as a segment. Returns `None` for
/// coplanar, non-intersecting, or point-contact cases.
#[allow(clippy::similar_names)]
fn tri_tri_segment(
    t1: &[[f64; 3]; 3],
    t2: &[[f64; 3]; 3],
) -> Option<([f64; 3], [f64; 3])> {
    // Quick AABB reject
    for ax in 0..3 {
        let mn1 = t1[0][ax].min(t1[1][ax]).min(t1[2][ax]);
        let mx1 = t1[0][ax].max(t1[1][ax]).max(t1[2][ax]);
        let mn2 = t2[0][ax].min(t2[1][ax]).min(t2[2][ax]);
        let mx2 = t2[0][ax].max(t2[1][ax]).max(t2[2][ax]);
        if mx1 < mn2 - GEO_EPS || mx2 < mn1 - GEO_EPS {
            return None;
        }
    }

    let mut pts = Vec::with_capacity(6);

    // Edges of t1 vs triangle t2
    for i in 0..3 {
        if let Some(p) = edge_tri_point(t1[i], t1[(i + 1) % 3], t2[0], t2[1], t2[2]) {
            push_unique(&mut pts, p);
        }
    }

    // Edges of t2 vs triangle t1
    for i in 0..3 {
        if let Some(p) = edge_tri_point(t2[i], t2[(i + 1) % 3], t1[0], t1[1], t1[2]) {
            push_unique(&mut pts, p);
        }
    }

    if pts.len() >= 2 {
        Some((pts[0], pts[1]))
    } else {
        None
    }
}

// ── Triangle splitting ──────────────────────────────────────────────

/// Test if 3D point `p` lies on segment `a→b` (within tolerance).
fn on_segment(p: [f64; 3], a: [f64; 3], b: [f64; 3]) -> bool {
    let ab = sub(b, a);
    let ab2 = len2(ab);
    if ab2 < GEO_EPS * GEO_EPS {
        return dist2(p, a) < GEO_EPS * GEO_EPS;
    }
    // Squared distance from p to infinite line through a,b
    let ap = sub(p, a);
    let c = cross(ab, ap);
    if len2(c) / ab2 > WELD_EPSILON * WELD_EPSILON * 100.0 {
        return false;
    }
    // Parameter along segment
    let t = dot(ap, ab) / ab2;
    (-GEO_EPS..=1.0 + GEO_EPS).contains(&t)
}

/// Which edge of `tri` does point `p` lie on?
/// Returns 0 (v0→v1), 1 (v1→v2), or 2 (v2→v0). `None` if not on any edge.
fn which_edge(p: [f64; 3], tri: &[[f64; 3]; 3]) -> Option<usize> {
    (0..3).find(|&i| on_segment(p, tri[i], tri[(i + 1) % 3]))
}

/// True if a triangle has near-zero area.
fn degenerate(tri: &[[f64; 3]; 3]) -> bool {
    len2(cross(sub(tri[1], tri[0]), sub(tri[2], tri[0]))) < GEO_EPS * GEO_EPS
}

/// Split a triangle given cut points `p` on edge `ep` and `q` on edge `eq`.
///
/// Produces 3 sub-triangles preserving the original CCW winding.
/// Edges are numbered 0 = v0→v1, 1 = v1→v2, 2 = v2→v0.
#[allow(clippy::similar_names)]
fn split_two_edges(
    tri: [[f64; 3]; 3],
    cp: [f64; 3],
    ep: usize,
    cq: [f64; 3],
    eq: usize,
) -> Vec<[[f64; 3]; 3]> {
    let [va, vb, vc] = tri;

    // Normalise so ep < eq (only 3 cases)
    let (p, e0, q, e1) = if ep < eq {
        (cp, ep, cq, eq)
    } else {
        (cq, eq, cp, ep)
    };

    let mut out = Vec::with_capacity(3);

    match (e0, e1) {
        (0, 1) => {
            // p on A→B, q on B→C; shared vertex B
            out.push([va, p, q]);
            out.push([va, q, vc]);
            out.push([p, vb, q]);
        }
        (0, 2) => {
            // p on A→B, q on C→A; shared vertex A
            out.push([va, p, q]);
            out.push([p, vb, vc]);
            out.push([p, vc, q]);
        }
        (1, 2) => {
            // p on B→C, q on C→A; shared vertex C
            out.push([va, vb, p]);
            out.push([va, p, q]);
            out.push([p, vc, q]);
        }
        _ => out.push(tri),
    }

    out.retain(|t| !degenerate(t));
    out
}

/// Split a triangle along a single intersection segment.
fn split_by_segment(
    tri: [[f64; 3]; 3],
    seg_a: [f64; 3],
    seg_b: [f64; 3],
) -> Vec<[[f64; 3]; 3]> {
    if dist2(seg_a, seg_b) < GEO_EPS * GEO_EPS {
        return vec![tri]; // degenerate segment
    }
    let ea = which_edge(seg_a, &tri);
    let eb = which_edge(seg_b, &tri);

    match (ea, eb) {
        (Some(a), Some(b)) if a != b => split_two_edges(tri, seg_a, a, seg_b, b),
        _ => vec![tri], // same edge, interior, or not on triangle — no split
    }
}

/// Iteratively split a triangle by all triangles from another mesh.
///
/// For each `other` triangle that intersects the current set of sub-triangles,
/// compute the intersection segment and split the affected sub-triangle.
const MAX_SPLITS: usize = 256;

fn split_by_mesh(tri: [[f64; 3]; 3], others: &[[[f64; 3]; 3]]) -> Vec<[[f64; 3]; 3]> {
    let mut current = vec![tri];

    for other in others {
        // Coarse AABB filter against original triangle
        if !aabb_overlap(&tri, other) {
            continue;
        }
        if current.len() >= MAX_SPLITS {
            break;
        }

        let mut next = Vec::with_capacity(current.len() + 4);
        for sub in &current {
            if let Some((p, q)) = tri_tri_segment(sub, other) {
                next.extend(split_by_segment(*sub, p, q));
            } else {
                next.push(*sub);
            }
        }
        current = next;
    }

    current
}

/// Quick per-axis AABB overlap test between two triangles.
fn aabb_overlap(t1: &[[f64; 3]; 3], t2: &[[f64; 3]; 3]) -> bool {
    for ax in 0..3 {
        let mn1 = t1[0][ax].min(t1[1][ax]).min(t1[2][ax]);
        let mx1 = t1[0][ax].max(t1[1][ax]).max(t1[2][ax]);
        let mn2 = t2[0][ax].min(t2[1][ax]).min(t2[2][ax]);
        let mx2 = t2[0][ax].max(t2[1][ax]).max(t2[2][ax]);
        if mx1 < mn2 - GEO_EPS || mx2 < mn1 - GEO_EPS {
            return false;
        }
    }
    true
}

/// Triangle centroid, nudged slightly along the face normal to avoid
/// on-surface ambiguity in `point_in_mesh` ray casting.
fn nudged_centroid(tri: &[[f64; 3]; 3]) -> [f64; 3] {
    let cx = (tri[0][0] + tri[1][0] + tri[2][0]) / 3.0;
    let cy = (tri[0][1] + tri[1][1] + tri[2][1]) / 3.0;
    let cz = (tri[0][2] + tri[1][2] + tri[2][2]) / 3.0;
    let n = cross(sub(tri[1], tri[0]), sub(tri[2], tri[0]));
    let mag = len2(n).sqrt();
    if mag < GEO_EPS {
        return [cx, cy, cz];
    }
    // Nudge 1e-5 along outward normal
    let s = 1e-5 / mag;
    [cx + n[0] * s, cy + n[1] * s, cz + n[2] * s]
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
#[cfg(test)]
pub fn subtract(target: &HalfEdgeMesh, tool: &HalfEdgeMesh, offset: [f64; 3]) -> HalfEdgeMesh {
    boolean_op(target, tool, offset, BooleanMode::Subtract)
}

/// Boolean union: combine `target` and `tool` into a single mesh.
#[allow(dead_code)]
pub fn union(target: &HalfEdgeMesh, tool: &HalfEdgeMesh, offset: [f64; 3]) -> HalfEdgeMesh {
    boolean_op(target, tool, offset, BooleanMode::Union)
}

/// Boolean intersect: keep only the volume shared by `target` and `tool`.
#[allow(dead_code)]
pub fn intersect(target: &HalfEdgeMesh, tool: &HalfEdgeMesh, offset: [f64; 3]) -> HalfEdgeMesh {
    boolean_op(target, tool, offset, BooleanMode::Intersect)
}

/// Boolean operation on two triangle meshes.
///
/// Uses triangle-triangle intersection to split faces at the exact boundary,
/// then classifies each sub-face by centroid point-in-mesh testing.
/// Correctly handles tools sitting entirely within a single large target face
/// (no pre-existing target vertices inside the tool volume).
///
/// - Subtract: keep target-outside + flipped tool-inside
/// - Union: keep target-outside + tool-outside
/// - Intersect: keep target-inside + tool-inside
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
            BooleanMode::Union => offset_mesh(tool, offset),
            _ => HalfEdgeMesh::default(),
        };
    }

    let tool_tris = extract_triangles(tool, offset);
    let target_tris = extract_triangles(target, [0.0; 3]);

    let mut builder = MeshBuilder::new();

    // ── Process target faces ─────────────────────────────────────────
    for target_tri in &target_tris {
        let subs = split_by_mesh(*target_tri, &tool_tris);
        for sub in &subs {
            let c = nudged_centroid(sub);
            let inside_tool = point_in_mesh(&c, &tool_tris);

            let keep = match mode {
                BooleanMode::Subtract | BooleanMode::Union => !inside_tool,
                BooleanMode::Intersect => inside_tool,
            };

            if keep {
                builder.triangle(sub[0], sub[1], sub[2]);
            }
        }
    }

    // ── Process tool faces ───────────────────────────────────────────
    for tool_tri in &tool_tris {
        let subs = split_by_mesh(*tool_tri, &target_tris);
        for sub in &subs {
            let c = nudged_centroid(sub);
            let inside_target = point_in_mesh(&c, &target_tris);

            match mode {
                BooleanMode::Subtract => {
                    if inside_target {
                        // Flip winding — tool face becomes cavity wall
                        builder.triangle(sub[2], sub[1], sub[0]);
                    }
                }
                BooleanMode::Union => {
                    if !inside_target {
                        builder.triangle(sub[0], sub[1], sub[2]);
                    }
                }
                BooleanMode::Intersect => {
                    if inside_target {
                        builder.triangle(sub[0], sub[1], sub[2]);
                    }
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
