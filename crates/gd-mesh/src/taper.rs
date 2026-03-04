use super::half_edge::HalfEdgeMesh;

/// Taper a mesh along an axis by scaling vertices.
///
/// `axis`: 0=X, 1=Y, 2=Z — the axis along which to taper.
/// `start_scale`: scale factor at the min end of the axis.
/// `end_scale`: scale factor at the max end of the axis.
/// `midpoint`: optional peak position (0.0–1.0) for two-segment taper.
/// `range`: optional (from, to) normalized positions limiting the taper region.
pub fn taper(
    mesh: &mut HalfEdgeMesh,
    axis: usize,
    start_scale: f64,
    end_scale: f64,
    midpoint: Option<f64>,
    range: Option<(f64, f64)>,
) -> usize {
    if mesh.vertices.is_empty() {
        return 0;
    }

    let (aabb_min, aabb_max) = mesh.aabb();
    let axis_min = aabb_min[axis];
    let axis_max = aabb_max[axis];
    let axis_len = axis_max - axis_min;

    if axis_len < 1e-12 {
        return 0;
    }

    // Other two axes to scale
    let (a1, a2) = match axis {
        0 => (1, 2),
        1 => (0, 2),
        _ => (0, 1),
    };

    // Scale relative to the mesh's AABB center (not world origin)
    let center1 = (aabb_min[a1] + aabb_max[a1]) * 0.5;
    let center2 = (aabb_min[a2] + aabb_max[a2]) * 0.5;

    let (range_from, range_to) = range.unwrap_or((0.0, 1.0));
    let mut modified = 0;

    for v in &mut mesh.vertices {
        let t = (v.position[axis] - axis_min) / axis_len; // 0.0 to 1.0

        // Skip vertices outside taper range
        if t < range_from - 1e-9 || t > range_to + 1e-9 {
            continue;
        }

        // Remap t to within the range
        let range_len = range_to - range_from;
        let t_local = if range_len > 1e-12 {
            ((t - range_from) / range_len).clamp(0.0, 1.0)
        } else {
            0.5
        };

        let scale = if let Some(mid) = midpoint {
            // Two-segment taper: ramp up to midpoint, then down
            if t_local <= mid {
                let seg_t = if mid > 1e-12 { t_local / mid } else { 1.0 };
                start_scale + (end_scale - start_scale) * seg_t
            } else {
                let seg_t = if (1.0 - mid) > 1e-12 {
                    (t_local - mid) / (1.0 - mid)
                } else {
                    1.0
                };
                end_scale + (start_scale - end_scale) * seg_t
            }
        } else {
            // Linear taper from start to end
            start_scale + (end_scale - start_scale) * t_local
        };

        // Scale relative to AABB center so the mesh tapers in place
        v.position[a1] = center1 + (v.position[a1] - center1) * scale;
        v.position[a2] = center2 + (v.position[a2] - center2) * scale;
        modified += 1;
    }

    modified
}
