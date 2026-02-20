use super::PlaneKind;
use super::half_edge::HalfEdgeMesh;
use super::profile::{map_2d_to_3d_at_depth, signed_area_2x, triangulate_2d};

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

    let mut indices: Vec<usize> = Vec::new();
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
            &mut indices,
        )?;
    } else {
        build_standard_caps(&cap_indices, cap_flip, n_pts, n_segs, &mut indices);
    }

    build_side_walls(wall_flip, n_pts, n_segs, &mut indices);

    Some(HalfEdgeMesh::from_triangles(&positions, &indices))
}

/// Standard fan-triangulated caps (no inset ring).
fn build_standard_caps(
    cap_indices: &[usize],
    cap_flip: bool,
    n_pts: usize,
    n_segs: usize,
    indices: &mut Vec<usize>,
) {
    // Front cap (section 0, at +half)
    for tri in cap_indices.chunks(3) {
        if cap_flip {
            indices.extend_from_slice(&[tri[2], tri[1], tri[0]]);
        } else {
            indices.extend_from_slice(&[tri[0], tri[1], tri[2]]);
        }
    }

    // Back cap (section n_segs, at -half)
    let back_offset = n_segs * n_pts;
    for tri in cap_indices.chunks(3) {
        if cap_flip {
            indices.extend_from_slice(&[
                back_offset + tri[0],
                back_offset + tri[1],
                back_offset + tri[2],
            ]);
        } else {
            indices.extend_from_slice(&[
                back_offset + tri[2],
                back_offset + tri[1],
                back_offset + tri[0],
            ]);
        }
    }
}

/// Caps with an inset quad ring to prevent centre-vertex pinching.
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
    indices: &mut Vec<usize>,
) -> Option<()> {
    // Compute 2D centroid
    let cx: f64 = points.iter().map(|p| p[0]).sum::<f64>() / n_pts as f64;
    let cy: f64 = points.iter().map(|p| p[1]).sum::<f64>() / n_pts as f64;

    // Create inset 2D points (lerp toward centroid)
    let inset_2d: Vec<[f64; 2]> = points
        .iter()
        .map(|p| {
            [
                p[0] + (cx - p[0]) * inset_factor,
                p[1] + (cy - p[1]) * inset_factor,
            ]
        })
        .collect();

    let inner_cap_indices = triangulate_2d(&inset_2d)?;

    // Add inset vertices for front cap (at +half) and back cap (at -half)
    let front_inset_base = positions.len();
    for ip in &inset_2d {
        positions.push(map_2d_to_3d_at_depth(ip, plane, half));
    }
    let back_inset_base = positions.len();
    for ip in &inset_2d {
        positions.push(map_2d_to_3d_at_depth(ip, plane, -half));
    }

    // Front cap: outer quad ring + inner polygon
    build_inset_cap_one_side(
        0,
        front_inset_base,
        cap_flip,
        false,
        n_pts,
        &inner_cap_indices,
        indices,
    );

    // Back cap: outer quad ring + inner polygon (reversed)
    build_inset_cap_one_side(
        n_segs * n_pts,
        back_inset_base,
        cap_flip,
        true,
        n_pts,
        &inner_cap_indices,
        indices,
    );

    Some(())
}

/// Build one inset cap (front or back).
fn build_inset_cap_one_side(
    outer_base: usize,
    inset_base: usize,
    cap_flip: bool,
    is_back: bool,
    n_pts: usize,
    inner_indices: &[usize],
    indices: &mut Vec<usize>,
) {
    // Outer quad ring
    for i in 0..n_pts {
        let j = (i + 1) % n_pts;
        let oi = outer_base + i;
        let oj = outer_base + j;
        let ii = inset_base + i;
        let ij = inset_base + j;
        if cap_flip == is_back {
            indices.extend_from_slice(&[oi, oj, ii]);
            indices.extend_from_slice(&[oj, ij, ii]);
        } else {
            indices.extend_from_slice(&[oi, ii, oj]);
            indices.extend_from_slice(&[oj, ii, ij]);
        }
    }

    // Inner polygon
    for tri in inner_indices.chunks(3) {
        if cap_flip == is_back {
            indices.push(inset_base + tri[0]);
            indices.push(inset_base + tri[1]);
            indices.push(inset_base + tri[2]);
        } else {
            indices.push(inset_base + tri[2]);
            indices.push(inset_base + tri[1]);
            indices.push(inset_base + tri[0]);
        }
    }
}

/// Side walls connecting adjacent cross-sections.
fn build_side_walls(wall_flip: bool, n_pts: usize, n_segs: usize, indices: &mut Vec<usize>) {
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
                indices.extend_from_slice(&[fi, fj, bi]);
                indices.extend_from_slice(&[fj, bj, bi]);
            } else {
                indices.extend_from_slice(&[fj, fi, bi]);
                indices.extend_from_slice(&[fj, bi, bj]);
            }
        }
    }
}
