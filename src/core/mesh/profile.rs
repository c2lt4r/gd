use super::PlaneKind;
use super::half_edge::HalfEdgeMesh;

/// Triangulate a 2D polygon using ear-clipping and build a flat `HalfEdgeMesh`.
///
/// The profile is a flat polygon on the given plane, useful as a starting point
/// for extrude/revolve operations.
pub fn triangulate_profile(points: &[[f64; 2]], plane: PlaneKind) -> Option<HalfEdgeMesh> {
    if points.len() < 3 {
        return None;
    }

    // Run earcutr on the 2D polygon
    let flat: Vec<f64> = points.iter().flat_map(|p| [p[0], p[1]]).collect();
    let tri_indices = earcutr::earcut(&flat, &[], 2).ok()?;

    // Map 2D points to 3D positions based on plane
    let positions: Vec<[f64; 3]> = points.iter().map(|p| map_2d_to_3d(p, plane)).collect();

    // earcut always normalises input to CCW, so output winding is CCW regardless
    // of whether the input polygon was CW or CCW.  The flip therefore depends only
    // on the plane's coordinate-system parity:
    //   Front (XY→Z): CCW in XY → normal +Z (outward) → no flip
    //   Side  (ZY→X): CCW mapped to (0,y,x) → normal −X (inward) → flip
    //   Top   (XZ→Y): CCW mapped to (x,0,y) → normal −Y (inward) → flip
    let flip = plane != PlaneKind::Front;

    // Build triangle indices with correct winding
    let indices: Vec<usize> = if flip {
        tri_indices
            .chunks(3)
            .flat_map(|tri| [tri[2], tri[1], tri[0]])
            .collect()
    } else {
        tri_indices
    };

    Some(HalfEdgeMesh::from_triangles(&positions, &indices))
}

/// Map a 2D profile point to 3D based on the drawing plane.
///
/// - Front: XY plane → (x, y, 0)
/// - Side:  ZY plane → (0, y, z) where z=x
/// - Top:   XZ plane → (x, 0, z) where z=y
pub fn map_2d_to_3d(p: &[f64; 2], plane: PlaneKind) -> [f64; 3] {
    match plane {
        PlaneKind::Front => [p[0], p[1], 0.0],
        PlaneKind::Side => [0.0, p[1], p[0]],
        PlaneKind::Top => [p[0], 0.0, p[1]],
    }
}

/// Map a 2D profile point to 3D at a given depth along the extrusion axis.
pub fn map_2d_to_3d_at_depth(p: &[f64; 2], plane: PlaneKind, depth: f64) -> [f64; 3] {
    match plane {
        PlaneKind::Front => [p[0], p[1], depth],
        PlaneKind::Side => [depth, p[1], p[0]],
        PlaneKind::Top => [p[0], depth, p[1]],
    }
}

/// Compute 2x the signed area of a 2D polygon (positive = CCW).
pub fn signed_area_2x(points: &[[f64; 2]]) -> f64 {
    let n = points.len();
    let mut area = 0.0;
    for i in 0..n {
        let j = (i + 1) % n;
        area += points[i][0] * points[j][1] - points[j][0] * points[i][1];
    }
    area
}

/// Triangulate a 2D polygon, returning indices into the original point array.
/// Returns `None` if triangulation fails.
pub fn triangulate_2d(points: &[[f64; 2]]) -> Option<Vec<usize>> {
    if points.len() < 3 {
        return None;
    }
    let flat: Vec<f64> = points.iter().flat_map(|p| [p[0], p[1]]).collect();
    earcutr::earcut(&flat, &[], 2).ok()
}
