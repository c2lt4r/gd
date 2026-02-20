use super::PlaneKind;
use super::half_edge::HalfEdgeMesh;
use super::profile::{signed_area_2x, triangulate_2d};

/// Revolve a 2D profile around an axis to create a surface of revolution.
///
/// `axis`: 0=X, 1=Y, 2=Z — the axis to revolve around.
/// `angle_deg`: how many degrees to revolve (360 = full revolution).
/// `segments`: number of angular subdivisions.
/// `cap`: whether to cap open ends for partial revolves (< 360).
pub fn revolve(
    points: &[[f64; 2]],
    plane: PlaneKind,
    axis: usize,
    angle_deg: f64,
    segments: u32,
    cap: bool,
) -> Option<HalfEdgeMesh> {
    if points.len() < 2 || segments == 0 {
        return None;
    }

    let n_pts = points.len();
    let n_segs = segments as usize;
    let angle_rad = angle_deg.to_radians();
    let full_revolution = (angle_deg - 360.0).abs() < 0.01;

    // Build ring vertices: for each angular step, rotate the profile
    let n_rings = if full_revolution { n_segs } else { n_segs + 1 };

    let mut positions: Vec<[f64; 3]> = Vec::with_capacity(n_rings * n_pts);
    for ring in 0..n_rings {
        let t = ring as f64 / n_segs as f64;
        let theta = t * angle_rad;
        let cos_t = theta.cos();
        let sin_t = theta.sin();

        for p in points {
            let pos = rotate_profile_point(p, plane, axis, cos_t, sin_t);
            positions.push(pos);
        }
    }

    let mut faces: Vec<Vec<usize>> = Vec::new();

    // Determine winding from first quad's normal direction
    let area2 = signed_area_2x(points);
    let flip = area2 < 0.0;

    // Side faces: connect adjacent rings — emit quads
    for ring in 0..n_segs {
        let cur_base = (ring % n_rings) * n_pts;
        let next_base = ((ring + 1) % n_rings) * n_pts;

        for i in 0..n_pts - 1 {
            let ci = cur_base + i;
            let cj = cur_base + i + 1;
            let ni = next_base + i;
            let nj = next_base + i + 1;

            if flip {
                faces.push(vec![ci, ni, nj, cj]);
            } else {
                faces.push(vec![ci, cj, nj, ni]);
            }
        }
    }

    // End caps for partial revolves — stay triangulated
    if cap
        && !full_revolution
        && points.len() >= 3
        && let Some(cap_tri) = triangulate_2d(points)
    {
        // Start cap (ring 0)
        for tri in cap_tri.chunks(3) {
            if flip {
                faces.push(vec![tri[2], tri[1], tri[0]]);
            } else {
                faces.push(vec![tri[0], tri[1], tri[2]]);
            }
        }

        // End cap (last ring)
        let end_base = (n_rings - 1) * n_pts;
        for tri in cap_tri.chunks(3) {
            if flip {
                faces.push(vec![
                    end_base + tri[0],
                    end_base + tri[1],
                    end_base + tri[2],
                ]);
            } else {
                faces.push(vec![
                    end_base + tri[2],
                    end_base + tri[1],
                    end_base + tri[0],
                ]);
            }
        }
    }

    if faces.is_empty() {
        return None;
    }

    let face_slices: Vec<&[usize]> = faces.iter().map(Vec::as_slice).collect();
    Some(HalfEdgeMesh::from_polygons(&positions, &face_slices))
}

/// Rotate a 2D profile point around the given axis.
fn rotate_profile_point(
    p: &[f64; 2],
    plane: PlaneKind,
    axis: usize,
    cos_t: f64,
    sin_t: f64,
) -> [f64; 3] {
    // Map 2D point to 3D on the profile plane
    let (px, py) = (p[0], p[1]);

    match (plane, axis) {
        // Front plane (XY), revolve around Y → rotate X/Z
        (PlaneKind::Front, 1) => [px * cos_t, py, px * sin_t],
        // Front plane, revolve around X → rotate Y/Z
        (PlaneKind::Front, 0) => [px, py * cos_t, -py * sin_t],
        // Front plane, revolve around Z → rotate X/Y
        (PlaneKind::Front, _) => [px * cos_t - py * sin_t, px * sin_t + py * cos_t, 0.0],

        // Side plane (ZY), revolve around Y → rotate Z/X
        (PlaneKind::Side, 1) => [px * sin_t, py, px * cos_t],
        // Side plane, revolve around X → rotate Y/Z
        (PlaneKind::Side, 0) => [0.0, py * cos_t - px * sin_t, px * cos_t + py * sin_t],
        // Side plane, revolve around Z → rotate X/Y
        (PlaneKind::Side, _) => [-py * sin_t, py * cos_t, px],

        // Top plane (XZ), revolve around Y → rotate X/Z
        (PlaneKind::Top, 1) => [px * cos_t + py * sin_t, 0.0, -px * sin_t + py * cos_t],
        // Top plane, revolve around X → rotate Y/Z
        (PlaneKind::Top, 0) => [px, -py * sin_t, py * cos_t],
        // Top plane, revolve around Z → rotate X/Y
        (PlaneKind::Top, _) => [px * cos_t, px * sin_t, py],
    }
}
