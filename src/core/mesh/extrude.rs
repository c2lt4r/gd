use super::PlaneKind;
use super::half_edge::HalfEdgeMesh;
use super::profile::{
    map_2d_to_3d_at_depth, signed_area_2x, triangulate_2d, triangulate_2d_with_holes,
};

/// Face storage: mix of triangles and quads.
type PolyFaces = Vec<Vec<usize>>;

/// Linear extrusion with no cap inset. Convenience wrapper for tests and
/// simple profiles (< 8 vertices).
#[cfg(test)]
pub fn extrude(
    points: &[[f64; 2]],
    plane: PlaneKind,
    depth: f64,
    segments: u32,
) -> Option<HalfEdgeMesh> {
    extrude_with_inset(points, plane, depth, segments, 0.0)
}

/// Linear extrusion with optional cap inset.
///
/// Creates front/back caps, side walls, and optionally a quad-ring inset on
/// each cap to prevent pinching at the center vertex.
///
/// `inset_factor`: 0.0 = no inset (standard fan caps),
///                 0.15 = typical quad-ring inset.
#[allow(clippy::too_many_lines)]
pub fn extrude_with_inset(
    points: &[[f64; 2]],
    plane: PlaneKind,
    depth: f64,
    segments: u32,
    inset_factor: f64,
) -> Option<HalfEdgeMesh> {
    if points.len() < 3 || segments == 0 {
        return None;
    }

    let n_pts = points.len();
    let n_segs = segments as usize;
    let half = depth / 2.0;

    let cap_indices = triangulate_2d(points)?;

    // CAP FLIP: earcut always outputs CCW. Flip depends on plane parity.
    let cap_flip = plane != PlaneKind::Front;

    // WALL FLIP: depends on both input winding AND plane.
    let area2 = signed_area_2x(points);
    let wall_flip = (area2 > 0.0) != (plane == PlaneKind::Front);

    // Build cross-sections: (n_segs + 1) rings of n_pts vertices each
    let mut positions: Vec<[f64; 3]> = Vec::with_capacity((n_segs + 1) * n_pts);
    for s in 0..=n_segs {
        let t = s as f64 / n_segs as f64;
        let d = half - t * depth;
        for p in points {
            positions.push(map_2d_to_3d_at_depth(p, plane, d));
        }
    }

    let mut faces: PolyFaces = Vec::new();
    let use_inset = inset_factor > 0.0 && n_pts >= 5;

    if use_inset {
        build_inset_caps(
            points,
            plane,
            &cap_indices,
            cap_flip,
            n_pts,
            n_segs,
            half,
            inset_factor,
            &mut positions,
            &mut faces,
        )?;
    } else {
        build_standard_caps(&cap_indices, cap_flip, n_pts, n_segs, &mut faces);
    }

    build_side_walls(wall_flip, n_pts, n_segs, &mut faces);

    let face_slices: Vec<&[usize]> = faces.iter().map(Vec::as_slice).collect();
    Some(HalfEdgeMesh::from_polygons(&positions, &face_slices))
}

/// Standard fan-triangulated caps (no inset ring).
fn build_standard_caps(
    cap_indices: &[usize],
    cap_flip: bool,
    n_pts: usize,
    n_segs: usize,
    faces: &mut PolyFaces,
) {
    // Front cap (section 0, at +half) — triangulated
    for tri in cap_indices.chunks(3) {
        if cap_flip {
            faces.push(vec![tri[2], tri[1], tri[0]]);
        } else {
            faces.push(vec![tri[0], tri[1], tri[2]]);
        }
    }

    // Back cap (section n_segs, at -half) — triangulated
    let back_offset = n_segs * n_pts;
    for tri in cap_indices.chunks(3) {
        if cap_flip {
            faces.push(vec![
                back_offset + tri[0],
                back_offset + tri[1],
                back_offset + tri[2],
            ]);
        } else {
            faces.push(vec![
                back_offset + tri[2],
                back_offset + tri[1],
                back_offset + tri[0],
            ]);
        }
    }
}

/// Caps with multi-ring concentric inset for clean edge-loop topology.
///
/// For profiles with >= 8 vertices, generates N concentric rings of quads
/// instead of a single inset ring. The innermost ring gets a small earcut fan.
/// This eliminates the pole singularity from fan triangulation.
#[allow(clippy::too_many_arguments)]
fn build_inset_caps(
    points: &[[f64; 2]],
    plane: PlaneKind,
    _cap_indices: &[usize],
    cap_flip: bool,
    n_pts: usize,
    n_segs: usize,
    half: f64,
    inset_factor: f64,
    positions: &mut Vec<[f64; 3]>,
    faces: &mut PolyFaces,
) -> Option<()> {
    // Compute 2D centroid
    let cx: f64 = points.iter().map(|p| p[0]).sum::<f64>() / n_pts as f64;
    let cy: f64 = points.iter().map(|p| p[1]).sum::<f64>() / n_pts as f64;

    // Determine ring count: auto = max(1, n_pts / 8), capped at 3
    let rings = (n_pts / 8).clamp(1, 3);

    // Build one cap side (front or back) with multi-ring inset
    build_multi_ring_cap_one_side(
        points, plane, cx, cy, inset_factor, rings, cap_flip, false, 0, n_pts, half, positions,
        faces,
    )?;

    build_multi_ring_cap_one_side(
        points,
        plane,
        cx,
        cy,
        inset_factor,
        rings,
        cap_flip,
        true,
        n_segs * n_pts,
        n_pts,
        -half,
        positions,
        faces,
    )?;

    Some(())
}

/// Build one multi-ring inset cap (front or back).
#[allow(clippy::too_many_arguments)]
fn build_multi_ring_cap_one_side(
    points: &[[f64; 2]],
    plane: PlaneKind,
    cx: f64,
    cy: f64,
    inset_factor: f64,
    rings: usize,
    cap_flip: bool,
    is_back: bool,
    outer_base: usize,
    n_pts: usize,
    depth: f64,
    positions: &mut Vec<[f64; 3]>,
    faces: &mut PolyFaces,
) -> Option<()> {
    let mut prev_base = outer_base;

    for k in 0..rings {
        // Lerp factor: (k+1) / (rings+1) * inset_factor toward centroid
        // This distributes rings evenly between outer boundary and
        // inset_factor fraction toward centroid
        let t = inset_factor * (k + 1) as f64 / (rings + 1) as f64;
        let ring_2d: Vec<[f64; 2]> = points
            .iter()
            .map(|p| [p[0] + (cx - p[0]) * t, p[1] + (cy - p[1]) * t])
            .collect();

        let ring_base = positions.len();
        for rp in &ring_2d {
            positions.push(map_2d_to_3d_at_depth(rp, plane, depth));
        }

        // Quad ring between prev_base and ring_base
        for i in 0..n_pts {
            let j = (i + 1) % n_pts;
            let oi = prev_base + i;
            let oj = prev_base + j;
            let ii = ring_base + i;
            let ij = ring_base + j;
            if cap_flip == is_back {
                faces.push(vec![oi, oj, ij, ii]);
            } else {
                faces.push(vec![oi, ii, ij, oj]);
            }
        }

        prev_base = ring_base;
    }

    // Innermost ring: triangulate with earcut
    // Build the final inset 2D points at full inset_factor
    let inner_2d: Vec<[f64; 2]> = points
        .iter()
        .map(|p| {
            [
                p[0] + (cx - p[0]) * inset_factor,
                p[1] + (cy - p[1]) * inset_factor,
            ]
        })
        .collect();

    let inner_base = positions.len();
    for ip in &inner_2d {
        positions.push(map_2d_to_3d_at_depth(ip, plane, depth));
    }

    // Quad ring between last intermediate ring and innermost ring
    for i in 0..n_pts {
        let j = (i + 1) % n_pts;
        let oi = prev_base + i;
        let oj = prev_base + j;
        let ii = inner_base + i;
        let ij = inner_base + j;
        if cap_flip == is_back {
            faces.push(vec![oi, oj, ij, ii]);
        } else {
            faces.push(vec![oi, ii, ij, oj]);
        }
    }

    // Inner polygon — earcut for the innermost ring
    let inner_cap_indices = triangulate_2d(&inner_2d)?;
    for tri in inner_cap_indices.chunks(3) {
        if cap_flip == is_back {
            faces.push(vec![
                inner_base + tri[0],
                inner_base + tri[1],
                inner_base + tri[2],
            ]);
        } else {
            faces.push(vec![
                inner_base + tri[2],
                inner_base + tri[1],
                inner_base + tri[0],
            ]);
        }
    }

    Some(())
}

/// Linear extrusion with hole contours.
///
/// Outer profile is extruded as usual. Each hole contour generates inner
/// side walls (reversed winding) and holes in the caps via earcut.
#[allow(clippy::too_many_lines)]
pub fn extrude_with_holes(
    outer: &[[f64; 2]],
    holes: &[Vec<[f64; 2]>],
    plane: PlaneKind,
    depth: f64,
    segments: u32,
) -> Option<HalfEdgeMesh> {
    if outer.len() < 3 || segments == 0 {
        return None;
    }

    let n_outer = outer.len();
    let n_segs = segments as usize;
    let half = depth / 2.0;

    // Triangulate caps with holes
    let cap_indices = triangulate_2d_with_holes(outer, holes)?;

    let cap_flip = plane != PlaneKind::Front;
    let area2 = signed_area_2x(outer);
    let wall_flip = (area2 > 0.0) != (plane == PlaneKind::Front);

    // Build combined point list: outer + all holes
    let mut all_2d: Vec<[f64; 2]> = outer.to_vec();
    let mut hole_offsets: Vec<usize> = Vec::new(); // start index of each hole in all_2d
    for hole in holes {
        hole_offsets.push(all_2d.len());
        all_2d.extend_from_slice(hole);
    }
    let n_all = all_2d.len();

    // Build cross-sections: (n_segs + 1) rings of n_all vertices each
    let mut positions: Vec<[f64; 3]> = Vec::with_capacity((n_segs + 1) * n_all);
    for s in 0..=n_segs {
        let t = s as f64 / n_segs as f64;
        let d = half - t * depth;
        for p in &all_2d {
            positions.push(map_2d_to_3d_at_depth(p, plane, d));
        }
    }

    let mut faces: PolyFaces = Vec::new();

    // Caps (triangulated with holes)
    // Front cap (section 0)
    for tri in cap_indices.chunks(3) {
        if cap_flip {
            faces.push(vec![tri[2], tri[1], tri[0]]);
        } else {
            faces.push(vec![tri[0], tri[1], tri[2]]);
        }
    }
    // Back cap (section n_segs)
    let back_offset = n_segs * n_all;
    for tri in cap_indices.chunks(3) {
        if cap_flip {
            faces.push(vec![
                back_offset + tri[0],
                back_offset + tri[1],
                back_offset + tri[2],
            ]);
        } else {
            faces.push(vec![
                back_offset + tri[2],
                back_offset + tri[1],
                back_offset + tri[0],
            ]);
        }
    }

    // Outer side walls
    build_side_walls(wall_flip, n_outer, n_segs, &mut faces);

    // Hole side walls (reversed winding — inner walls face inward toward hollow)
    for (hi, hole) in holes.iter().enumerate() {
        let n_hole = hole.len();
        let hole_start = hole_offsets[hi];
        for seg in 0..n_segs {
            let fwd_base = seg * n_all + hole_start;
            let bwd_base = (seg + 1) * n_all + hole_start;
            for i in 0..n_hole {
                let j = (i + 1) % n_hole;
                let fi = fwd_base + i;
                let fj = fwd_base + j;
                let bi = bwd_base + i;
                let bj = bwd_base + j;
                // Reversed winding compared to outer walls
                if wall_flip {
                    faces.push(vec![fj, fi, bi, bj]);
                } else {
                    faces.push(vec![fi, fj, bj, bi]);
                }
            }
        }
    }

    let face_slices: Vec<&[usize]> = faces.iter().map(Vec::as_slice).collect();
    Some(HalfEdgeMesh::from_polygons(&positions, &face_slices))
}

/// Side walls connecting adjacent cross-sections — emits quads.
fn build_side_walls(wall_flip: bool, n_pts: usize, n_segs: usize, faces: &mut PolyFaces) {
    for seg in 0..n_segs {
        let fwd_base = seg * n_pts;
        let bwd_base = (seg + 1) * n_pts;
        for i in 0..n_pts {
            let j = (i + 1) % n_pts;
            let fi = fwd_base + i;
            let fj = fwd_base + j;
            let bi = bwd_base + i;
            let bj = bwd_base + j;

            if wall_flip {
                faces.push(vec![fi, fj, bj, bi]);
            } else {
                faces.push(vec![fj, fi, bi, bj]);
            }
        }
    }
}
