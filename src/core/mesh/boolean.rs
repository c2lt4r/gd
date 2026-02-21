use super::half_edge::HalfEdgeMesh;
use super::topology::{
    self, GEO_EPS, Plane, Side, WELD_EPSILON, classify_vertex, cross, dist2, dot, face_plane, len2,
    lerp, sub, uf_find, uf_union, weld_position,
};

// ── Polygon splitting by plane ──────────────────────────────────────

/// Split a polygon by a plane into positive-side and negative-side fragments.
///
/// Adjacent polygons sharing an edge are split by the SAME plane at the SAME
/// endpoints. The split point `lerp(a, b, t)` is deterministic for a given
/// plane + edge, so T-junctions are eliminated by construction.
fn split_polygon_by_plane(poly: &[[f64; 3]], plane: &Plane) -> (Vec<[f64; 3]>, Vec<[f64; 3]>) {
    let n = poly.len();
    if n < 3 {
        return (Vec::new(), Vec::new());
    }

    let sides: Vec<Side> = poly.iter().map(|p| classify_vertex(plane, *p)).collect();

    let has_pos = sides.contains(&Side::Positive);
    let has_neg = sides.contains(&Side::Negative);

    if !has_neg {
        return (poly.to_vec(), Vec::new());
    }
    if !has_pos {
        return (Vec::new(), poly.to_vec());
    }

    let mut pos_verts = Vec::with_capacity(n + 2);
    let mut neg_verts = Vec::with_capacity(n + 2);

    for i in 0..n {
        let j = (i + 1) % n;
        let si = sides[i];
        let sj = sides[j];
        let vi = poly[i];
        let vj = poly[j];

        match si {
            Side::Positive => pos_verts.push(vi),
            Side::Negative => neg_verts.push(vi),
            Side::On => {
                pos_verts.push(vi);
                neg_verts.push(vi);
            }
        }

        if (si == Side::Positive && sj == Side::Negative)
            || (si == Side::Negative && sj == Side::Positive)
        {
            let di = dot(plane.normal, vi) - plane.d;
            let dj = dot(plane.normal, vj) - plane.d;
            let t = di / (di - dj);
            let intersection = lerp(vi, vj, t);
            pos_verts.push(intersection);
            neg_verts.push(intersection);
        }
    }

    (pos_verts, neg_verts)
}

/// Iteratively split a polygon by multiple planes, producing fragments.
fn split_polygon_by_planes(poly: &[[f64; 3]], planes: &[Plane]) -> Vec<Vec<[f64; 3]>> {
    let mut fragments = vec![poly.to_vec()];

    for plane in planes {
        let mut next = Vec::with_capacity(fragments.len() + 4);
        for frag in &fragments {
            if frag.len() < 3 {
                continue;
            }
            let (pos, neg) = split_polygon_by_plane(frag, plane);
            if pos.len() >= 3 {
                next.push(pos);
            }
            if neg.len() >= 3 {
                next.push(neg);
            }
        }
        fragments = next;
    }

    fragments
}

// ── AABB helpers ────────────────────────────────────────────────────

fn polygon_aabb(verts: &[[f64; 3]]) -> ([f64; 3], [f64; 3]) {
    let mut mn = [f64::MAX; 3];
    let mut mx = [f64::MIN; 3];
    for v in verts {
        for ax in 0..3 {
            mn[ax] = mn[ax].min(v[ax]);
            mx[ax] = mx[ax].max(v[ax]);
        }
    }
    (mn, mx)
}

fn aabb_overlap(a_min: &[f64; 3], a_max: &[f64; 3], b_min: &[f64; 3], b_max: &[f64; 3]) -> bool {
    for ax in 0..3 {
        if a_max[ax] < b_min[ax] - GEO_EPS || b_max[ax] < a_min[ax] - GEO_EPS {
            return false;
        }
    }
    true
}

// ── Polygon centroid ────────────────────────────────────────────────

/// Polygon centroid, nudged slightly along a normal direction to avoid
/// on-surface ambiguity in `point_in_mesh` ray casting.
///
/// `inward`: if true, nudge toward the polygon's own mesh interior (negate
/// the outward normal). Target faces use `inward=true` so the test point
/// enters the target volume; tool faces use `inward=false` so the test
/// point stays on the tool's exterior side.
fn nudged_centroid_poly(verts: &[[f64; 3]], inward: bool) -> [f64; 3] {
    let n = verts.len() as f64;
    let mut cx = 0.0;
    let mut cy = 0.0;
    let mut cz = 0.0;
    for v in verts {
        cx += v[0];
        cy += v[1];
        cz += v[2];
    }
    cx /= n;
    cy /= n;
    cz /= n;

    let plane = face_plane(verts);
    let sign = if inward { -1e-5 } else { 1e-5 };
    [
        cx + plane.normal[0] * sign,
        cy + plane.normal[1] * sign,
        cz + plane.normal[2] * sign,
    ]
}

// ── Coplanar face merging ────────────────────────────────────────────

/// Trace boundary polygon loop(s) for a group of merged coplanar faces.
fn trace_merged_boundary(
    group_faces: &[usize],
    welded_faces: &[Vec<usize>],
    welded_positions: &[[f64; 3]],
    edge_to_face: &std::collections::HashMap<(usize, usize), usize>,
    offset: [f64; 3],
) -> Vec<Vec<[f64; 3]>> {
    use std::collections::{HashMap, HashSet};

    let face_set: HashSet<usize> = group_faces.iter().copied().collect();

    let mut boundary_next: HashMap<usize, usize> = HashMap::new();
    for &fi in group_faces {
        let wface = &welded_faces[fi];
        let n = wface.len();
        for i in 0..n {
            let from = wface[i];
            let to = wface[(i + 1) % n];
            let is_internal = edge_to_face
                .get(&(to, from))
                .is_some_and(|&fj| face_set.contains(&fj));
            if !is_internal {
                boundary_next.insert(from, to);
            }
        }
    }

    if boundary_next.is_empty() {
        return Vec::new();
    }

    let mut visited: HashSet<usize> = HashSet::new();
    let mut polys = Vec::new();
    for &start_v in boundary_next.keys() {
        if visited.contains(&start_v) {
            continue;
        }
        let mut loop_verts = Vec::new();
        let mut current = start_v;
        loop {
            if visited.contains(&current) {
                break;
            }
            visited.insert(current);
            let p = welded_positions[current];
            loop_verts.push([p[0] + offset[0], p[1] + offset[1], p[2] + offset[2]]);

            if let Some(&next_v) = boundary_next.get(&current) {
                current = next_v;
            } else {
                break;
            }
        }
        if loop_verts.len() >= 3 {
            loop_verts.reverse();
            polys.push(loop_verts);
        }
    }
    polys
}

/// Merge coplanar adjacent faces into larger polygons before splitting.
///
/// Works with meshes that have unshared vertices (e.g. Godot primitives with
/// per-face normals) by welding vertex positions to establish adjacency.
fn extract_merged_polygons(mesh: &HalfEdgeMesh, offset: [f64; 3]) -> Vec<Vec<[f64; 3]>> {
    use std::collections::HashMap;

    let nf = mesh.faces.len();
    if nf == 0 {
        return Vec::new();
    }

    let mut welded_positions: Vec<[f64; 3]> = Vec::new();
    let mut welded_faces: Vec<Vec<usize>> = Vec::with_capacity(nf);

    for f in 0..nf {
        let vis = mesh.face_vertices(f);
        if vis.len() < 3 {
            welded_faces.push(Vec::new());
            continue;
        }
        let welded: Vec<usize> = vis
            .iter()
            .map(|&vi| weld_position(&mut welded_positions, mesh.vertices[vi].position))
            .collect();
        welded_faces.push(welded);
    }

    let mut edge_to_face: HashMap<(usize, usize), usize> = HashMap::new();
    for (fi, wface) in welded_faces.iter().enumerate() {
        let n = wface.len();
        for i in 0..n {
            let from = wface[i];
            let to = wface[(i + 1) % n];
            edge_to_face.insert((from, to), fi);
        }
    }

    let face_planes: Vec<Plane> = welded_faces
        .iter()
        .map(|wface| {
            if wface.len() < 3 {
                return Plane {
                    normal: [0.0, 1.0, 0.0],
                    d: 0.0,
                };
            }
            let verts: Vec<[f64; 3]> = wface.iter().map(|&wi| welded_positions[wi]).collect();
            face_plane(&verts)
        })
        .collect();

    let mut parent: Vec<usize> = (0..nf).collect();
    for (fi, wface) in welded_faces.iter().enumerate() {
        let n = wface.len();
        for i in 0..n {
            let from = wface[i];
            let to = wface[(i + 1) % n];
            if let Some(&fj) = edge_to_face.get(&(to, from))
                && fi != fj
                && topology::planes_coplanar(&face_planes[fi], &face_planes[fj])
            {
                uf_union(&mut parent, fi, fj);
            }
        }
    }

    let mut groups: HashMap<usize, Vec<usize>> = HashMap::new();
    for f in 0..nf {
        let root = uf_find(&mut parent, f);
        groups.entry(root).or_default().push(f);
    }

    let mut polys = Vec::with_capacity(groups.len());
    for group_faces in groups.values() {
        if group_faces.len() == 1 {
            let wface = &welded_faces[group_faces[0]];
            if wface.len() < 3 {
                continue;
            }
            let mut poly: Vec<[f64; 3]> = wface
                .iter()
                .map(|&wi| {
                    let p = welded_positions[wi];
                    [p[0] + offset[0], p[1] + offset[1], p[2] + offset[2]]
                })
                .collect();
            poly.reverse();
            polys.push(poly);
            continue;
        }

        polys.extend(trace_merged_boundary(
            group_faces,
            &welded_faces,
            &welded_positions,
            &edge_to_face,
            offset,
        ));
    }

    polys
}

/// Extract all faces as triangles (fan-triangulating polygons) for ray casting only.
fn extract_tris_for_classification(mesh: &HalfEdgeMesh, offset: [f64; 3]) -> Vec<[[f64; 3]; 3]> {
    let mut tris = Vec::with_capacity(mesh.faces.len());
    for f in 0..mesh.faces.len() {
        let verts = mesh.face_vertices(f);
        if verts.len() < 3 {
            continue;
        }
        let p0 = mesh.vertices[verts[0]].position;
        let p0 = [p0[0] + offset[0], p0[1] + offset[1], p0[2] + offset[2]];
        for i in 1..verts.len() - 1 {
            let pi = mesh.vertices[verts[i]].position;
            let pj = mesh.vertices[verts[i + 1]].position;
            tris.push([
                p0,
                [pi[0] + offset[0], pi[1] + offset[1], pi[2] + offset[2]],
                [pj[0] + offset[0], pj[1] + offset[1], pj[2] + offset[2]],
            ]);
        }
    }
    tris
}

// ── Ray-triangle intersection (Möller-Trumbore) ─────────────────────

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
        return None;
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

fn point_in_mesh(point: &[f64; 3], triangles: &[[[f64; 3]; 3]]) -> bool {
    let dir = [1.0, 0.000_131, 0.000_071];
    let mut count = 0u32;
    for tri in triangles {
        if ray_triangle(*point, dir, tri[0], tri[1], tri[2]).is_some() {
            count += 1;
        }
    }
    count % 2 == 1
}

// ── T-junction repair ───────────────────────────────────────────────

/// Check if point P lies on segment A→B (excluding endpoints).
/// Returns the parameter t ∈ (0, 1) if so.
fn point_on_segment(a: [f64; 3], b: [f64; 3], p: [f64; 3]) -> Option<f64> {
    let ab = sub(b, a);
    let ap = sub(p, a);
    let ab_len2 = len2(ab);
    if ab_len2 < GEO_EPS * GEO_EPS {
        return None; // degenerate edge
    }
    let t = dot(ap, ab) / ab_len2;
    if t <= GEO_EPS || t >= 1.0 - GEO_EPS {
        return None; // at or beyond endpoints
    }
    // Check distance from P to the line
    let proj = lerp(a, b, t);
    if dist2(proj, p) < WELD_EPSILON * WELD_EPSILON {
        Some(t)
    } else {
        None
    }
}

/// Fix T-junctions: find vertices that lie on edges of other polygons
/// and insert them, so all shared edges have matching vertices.
///
/// This is the "cheese bite" fix — only polygons whose edges contain
/// stray split points get extra vertices. Faces far from the cut are
/// completely untouched.
fn fix_t_junctions(polys: &mut [Vec<[f64; 3]>]) {
    // Collect all unique vertices from all polygons (welded)
    let mut all_verts: Vec<[f64; 3]> = Vec::new();
    let eps2 = WELD_EPSILON * WELD_EPSILON;
    for poly in polys.iter() {
        for &v in poly {
            if !all_verts.iter().any(|p| dist2(*p, v) < eps2) {
                all_verts.push(v);
            }
        }
    }

    // For each polygon, check each edge for vertices that lie on it
    let mut changed = true;
    let mut iterations = 0;
    while changed && iterations < 3 {
        changed = false;
        iterations += 1;

        for poly in polys.iter_mut() {
            let n = poly.len();
            if n < 3 {
                continue;
            }

            let mut insertions: Vec<(usize, Vec<[f64; 3]>)> = Vec::new();

            for i in 0..n {
                let a = poly[i];
                let b = poly[(i + 1) % n];

                // Find all vertices that lie on this edge
                let mut on_edge: Vec<(f64, [f64; 3])> = Vec::new();
                for &v in &all_verts {
                    // Skip if v is already an endpoint
                    if dist2(v, a) < eps2 || dist2(v, b) < eps2 {
                        continue;
                    }
                    if let Some(t) = point_on_segment(a, b, v) {
                        on_edge.push((t, v));
                    }
                }

                if !on_edge.is_empty() {
                    on_edge.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap());
                    let verts: Vec<[f64; 3]> = on_edge.into_iter().map(|(_, v)| v).collect();
                    insertions.push((i, verts));
                }
            }

            if !insertions.is_empty() {
                changed = true;
                // Insert vertices in reverse order so indices stay valid
                for (edge_idx, verts) in insertions.into_iter().rev() {
                    let insert_pos = edge_idx + 1;
                    for (j, v) in verts.into_iter().enumerate() {
                        poly.insert(insert_pos + j, v);
                    }
                }
            }
        }
    }
}

// ── Polygon mesh builder with vertex welding ────────────────────────

struct PolygonMeshBuilder {
    positions: Vec<[f64; 3]>,
    faces: Vec<Vec<usize>>,
}

impl PolygonMeshBuilder {
    fn new() -> Self {
        Self {
            positions: Vec::new(),
            faces: Vec::new(),
        }
    }

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

    fn polygon(&mut self, verts: &[[f64; 3]]) {
        if verts.len() < 3 {
            return;
        }
        let indices: Vec<usize> = verts.iter().map(|v| self.vertex(*v)).collect();

        let mut dedup = Vec::with_capacity(indices.len());
        for &idx in &indices {
            if dedup.last() != Some(&idx) {
                dedup.push(idx);
            }
        }
        if dedup.len() > 1 && dedup.first() == dedup.last() {
            dedup.pop();
        }
        if dedup.len() < 3 {
            return;
        }

        // Check that the polygon is not degenerate (all vertices collinear).
        // For polygons with T-junction vertices inserted, the first 3 might be
        // collinear, so check all consecutive triples.
        let mut valid = false;
        let nd = dedup.len();
        for i in 0..nd {
            let pa = self.positions[dedup[i]];
            let pb = self.positions[dedup[(i + 1) % nd]];
            let pc = self.positions[dedup[(i + 2) % nd]];
            if len2(cross(sub(pb, pa), sub(pc, pa))) >= GEO_EPS * GEO_EPS {
                valid = true;
                break;
            }
        }
        if !valid {
            return;
        }

        self.faces.push(dedup);
    }

    fn build(self) -> HalfEdgeMesh {
        let face_slices: Vec<&[usize]> = self.faces.iter().map(Vec::as_slice).collect();
        HalfEdgeMesh::from_polygons(&self.positions, &face_slices)
    }
}

// ── Boolean operation modes ──────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BooleanMode {
    Subtract,
    Union,
    Intersect,
}

// ── Main boolean operation ──────────────────────────────────────────

#[cfg(test)]
pub fn subtract(target: &HalfEdgeMesh, tool: &HalfEdgeMesh, offset: [f64; 3]) -> HalfEdgeMesh {
    boolean_op(target, tool, offset, BooleanMode::Subtract)
}

#[allow(dead_code)]
pub fn union(target: &HalfEdgeMesh, tool: &HalfEdgeMesh, offset: [f64; 3]) -> HalfEdgeMesh {
    boolean_op(target, tool, offset, BooleanMode::Union)
}

#[allow(dead_code)]
pub fn intersect(target: &HalfEdgeMesh, tool: &HalfEdgeMesh, offset: [f64; 3]) -> HalfEdgeMesh {
    boolean_op(target, tool, offset, BooleanMode::Intersect)
}

/// Boolean operation on two meshes using plane-based polygon splitting.
///
/// Preserves quad topology for faces far from the intersection boundary.
/// T-junctions eliminated by construction: adjacent polygons sharing an edge
/// are split by the SAME plane at the SAME `lerp` point.
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

    // Auto-expand tool when its bounding box is flush with the target's on all
    // axes. A same-size cutter at the same position gets enlarged by 0.1% so it
    // fully consumes the target instead of producing coplanar ambiguity.
    let expanded_tool = auto_expand_tool(target, tool, offset);
    let tool_ref = expanded_tool.as_ref().unwrap_or(tool);

    // Extract polygons, merging coplanar adjacent faces to eliminate internal
    // edges (triangle diagonals) that would create T-junctions.
    let target_polys = extract_merged_polygons(target, [0.0; 3]);
    let tool_polys = extract_merged_polygons(tool_ref, offset);

    // Triangle soups for point-in-mesh classification only
    let target_tris = extract_tris_for_classification(target, [0.0; 3]);
    let tool_tris = extract_tris_for_classification(tool_ref, offset);

    let tool_planes: Vec<Plane> = tool_polys.iter().map(|p| face_plane(p)).collect();
    let tool_aabbs: Vec<([f64; 3], [f64; 3])> =
        tool_polys.iter().map(|p| polygon_aabb(p)).collect();
    let target_planes: Vec<Plane> = target_polys.iter().map(|p| face_plane(p)).collect();
    let target_aabbs: Vec<([f64; 3], [f64; 3])> =
        target_polys.iter().map(|p| polygon_aabb(p)).collect();

    // Collect output polygons, then fix T-junctions before building.
    let mut output_polys: Vec<Vec<[f64; 3]>> = Vec::new();

    // ── Process target faces ─────────────────────────────────────────
    for target_poly in &target_polys {
        let (t_min, t_max) = polygon_aabb(target_poly);

        let relevant_planes: Vec<Plane> = tool_planes
            .iter()
            .zip(tool_aabbs.iter())
            .filter(|(_, (b_min, b_max))| aabb_overlap(&t_min, &t_max, b_min, b_max))
            .map(|(plane, _)| *plane)
            .collect();

        let fragments = if relevant_planes.is_empty() {
            vec![target_poly.clone()]
        } else {
            split_polygon_by_planes(target_poly, &relevant_planes)
        };

        for frag in &fragments {
            let c = nudged_centroid_poly(frag, true);
            let inside_tool = point_in_mesh(&c, &tool_tris);

            let keep = match mode {
                BooleanMode::Subtract | BooleanMode::Union => !inside_tool,
                BooleanMode::Intersect => inside_tool,
            };

            if keep {
                output_polys.push(frag.clone());
            }
        }
    }

    // ── Process tool faces ───────────────────────────────────────────
    for tool_poly in &tool_polys {
        let (t_min, t_max) = polygon_aabb(tool_poly);

        let relevant_planes: Vec<Plane> = target_planes
            .iter()
            .zip(target_aabbs.iter())
            .filter(|(_, (b_min, b_max))| aabb_overlap(&t_min, &t_max, b_min, b_max))
            .map(|(plane, _)| *plane)
            .collect();

        let fragments = if relevant_planes.is_empty() {
            vec![tool_poly.clone()]
        } else {
            split_polygon_by_planes(tool_poly, &relevant_planes)
        };

        for frag in &fragments {
            let c = nudged_centroid_poly(frag, false);
            let inside_target = point_in_mesh(&c, &target_tris);

            match mode {
                BooleanMode::Subtract => {
                    if inside_target {
                        let mut flipped = frag.clone();
                        flipped.reverse();
                        output_polys.push(flipped);
                    }
                }
                BooleanMode::Union => {
                    if !inside_target {
                        output_polys.push(frag.clone());
                    }
                }
                BooleanMode::Intersect => {
                    if inside_target {
                        output_polys.push(frag.clone());
                    }
                }
            }
        }
    }

    // ── Fix T-junctions ─────────────────────────────────────────────
    // Split points from cut faces may land on edges of unsplit neighbors.
    // Insert those vertices so all shared edges have matching vertices.
    fix_t_junctions(&mut output_polys);

    let mut builder = PolygonMeshBuilder::new();
    for poly in &output_polys {
        builder.polygon(poly);
    }
    let raw = builder.build();

    // ── Dissolve coplanar edges ──────────────────────────────────────
    // Merge coplanar fragments back into larger polygons. This collapses
    // spinal edges from plane-based splitting (e.g. a cube face split
    // into dozens of fragments gets restored to a single quad).
    let dissolved = topology::dissolve_coplanar_edges(&raw);

    // ── Quadrangulate n-gons ────────────────────────────────────────
    // Convert boundary n-gons (5+ vertices) to quad ring topology for
    // clean bevel and subdivision behavior.
    topology::quadrangulate_ngons(&dissolved)
}

/// Auto-expand the tool mesh when its bounding box is flush with the target's
/// on all 3 axes. Returns an optional expanded clone of the tool that's 0.1%
/// larger — enough to break coplanar degeneracy and fully consume the target
/// in same-size cutter scenarios.
fn auto_expand_tool(
    target: &HalfEdgeMesh,
    tool: &HalfEdgeMesh,
    offset: [f64; 3],
) -> Option<HalfEdgeMesh> {
    let mut t_min = [f64::MAX; 3];
    let mut t_max = [f64::MIN; 3];
    for v in &target.vertices {
        for ax in 0..3 {
            t_min[ax] = t_min[ax].min(v.position[ax]);
            t_max[ax] = t_max[ax].max(v.position[ax]);
        }
    }

    let mut s_min = [f64::MAX; 3];
    let mut s_max = [f64::MIN; 3];
    for v in &tool.vertices {
        for ax in 0..3 {
            let p = v.position[ax] + offset[ax];
            s_min[ax] = s_min[ax].min(p);
            s_max[ax] = s_max[ax].max(p);
        }
    }

    let flush_eps = 1e-4;

    let mut flush_axes = 0u32;
    for ax in 0..3 {
        let both_flush =
            (s_min[ax] - t_min[ax]).abs() < flush_eps && (s_max[ax] - t_max[ax]).abs() < flush_eps;
        if both_flush {
            flush_axes += 1;
        }
    }

    if flush_axes < 3 {
        return None;
    }

    // Scale tool outward from its center by 0.1% on each axis.
    let mut expanded = tool.clone();
    let n = expanded.vertices.len() as f64;
    let mut center = [0.0; 3];
    for v in &expanded.vertices {
        center[0] += v.position[0];
        center[1] += v.position[1];
        center[2] += v.position[2];
    }
    for c in &mut center {
        *c /= n;
    }
    let scale = 1.001;
    for v in &mut expanded.vertices {
        v.position[0] = center[0] + (v.position[0] - center[0]) * scale;
        v.position[1] = center[1] + (v.position[1] - center[1]) * scale;
        v.position[2] = center[2] + (v.position[2] - center[2]) * scale;
    }
    Some(expanded)
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
