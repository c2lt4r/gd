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

/// Triangulate a 2D polygon with holes, returning indices into the combined
/// point array (outer + all holes concatenated).
///
/// The returned indices refer to positions in `[outer..., hole0..., hole1..., ...]`.
pub fn triangulate_2d_with_holes(
    outer: &[[f64; 2]],
    holes: &[Vec<[f64; 2]>],
) -> Option<Vec<usize>> {
    if outer.len() < 3 {
        return None;
    }
    let mut flat: Vec<f64> = outer.iter().flat_map(|p| [p[0], p[1]]).collect();
    let mut hole_indices: Vec<usize> = Vec::new();
    for hole in holes {
        hole_indices.push(flat.len() / 2);
        flat.extend(hole.iter().flat_map(|p| [p[0], p[1]]));
    }
    earcutr::earcut(&flat, &hole_indices, 2).ok()
}

/// Default inset factor for quad-ring cap topology.
const CAP_INSET: f64 = 0.15;

/// Build a cap with multi-ring quad-inset topology from 3D boundary vertices.
///
/// For boundaries with >= 5 vertices: generates concentric rings of quads
/// toward the centroid, with a small earcut fan for the innermost ring.
/// For boundaries with < 5 vertices: falls back to earcut triangulation.
///
/// `boundary` — ordered vertex indices forming the cap boundary.
/// `positions` — vertex array (new ring vertices appended in-place).
/// `faces` — face output (quads + inner tris appended).
/// `flip` — `false` = earcut-order tris + default quad winding,
///           `true`  = reversed tris + reversed quad winding.
pub fn build_quad_cap_3d(
    boundary: &[usize],
    positions: &mut Vec<[f64; 3]>,
    faces: &mut Vec<Vec<usize>>,
    flip: bool,
) -> Option<()> {
    let n = boundary.len();
    if n < 3 {
        return None;
    }

    // Small polygons: just earcut
    if n < 5 {
        let pts_2d = project_boundary_to_2d(boundary, positions);
        let indices = triangulate_2d(&pts_2d)?;
        for tri in indices.chunks(3) {
            if flip {
                faces.push(vec![boundary[tri[2]], boundary[tri[1]], boundary[tri[0]]]);
            } else {
                faces.push(vec![boundary[tri[0]], boundary[tri[1]], boundary[tri[2]]]);
            }
        }
        return Some(());
    }

    // 3D centroid of boundary
    let (mut cx, mut cy, mut cz) = (0.0, 0.0, 0.0);
    for &vi in boundary {
        let p = positions[vi];
        cx += p[0];
        cy += p[1];
        cz += p[2];
    }
    let nf = n as f64;
    cx /= nf;
    cy /= nf;
    cz /= nf;

    let rings = (n / 8).clamp(1, 3);
    let mut prev: Vec<usize> = boundary.to_vec();

    // Intermediate concentric rings
    for k in 0..rings {
        let t = CAP_INSET * (k + 1) as f64 / (rings + 1) as f64;
        let ring_base = positions.len();
        for &vi in boundary {
            let p = positions[vi];
            positions.push([
                p[0] + (cx - p[0]) * t,
                p[1] + (cy - p[1]) * t,
                p[2] + (cz - p[2]) * t,
            ]);
        }

        emit_quad_ring(&prev, ring_base, n, flip, faces);
        prev = (ring_base..ring_base + n).collect();
    }

    // Innermost ring at full inset factor
    let inner_base = positions.len();
    for &vi in boundary {
        let p = positions[vi];
        positions.push([
            p[0] + (cx - p[0]) * CAP_INSET,
            p[1] + (cy - p[1]) * CAP_INSET,
            p[2] + (cz - p[2]) * CAP_INSET,
        ]);
    }
    emit_quad_ring(&prev, inner_base, n, flip, faces);

    // Inner fill: grid-fill the innermost ring with quads + 2 tris
    let inner_indices: Vec<usize> = (inner_base..inner_base + n).collect();
    grid_fill_ring(&inner_indices, faces, flip);

    Some(())
}

/// Emit a ring of quads connecting `prev` vertex indices to a new ring
/// starting at `ring_base`.
fn emit_quad_ring(
    prev: &[usize],
    ring_base: usize,
    n: usize,
    flip: bool,
    faces: &mut Vec<Vec<usize>>,
) {
    for i in 0..n {
        let j = (i + 1) % n;
        let oi = prev[i];
        let oj = prev[j];
        let ii = ring_base + i;
        let ij = ring_base + j;
        if flip {
            faces.push(vec![oi, ii, ij, oj]);
        } else {
            faces.push(vec![oi, oj, ij, ii]);
        }
    }
}

/// Fill a ring of vertices with quads by bridging opposite sides.
///
/// Splits the ring at vertices 0 and n/2, then bridges corresponding
/// vertices with quads. Produces 2 triangles (at the bridge endpoints
/// where sides share a vertex) and (n/2 - 2) quads for even n, or
/// 1 triangle and (n/2 - 1) quads for odd n.
///
/// Only meaningful for n >= 5; smaller rings should use earcut.
pub fn grid_fill_ring(boundary: &[usize], faces: &mut Vec<Vec<usize>>, flip: bool) {
    let n = boundary.len();
    if n < 3 {
        return;
    }
    let half = n / 2;
    // Side A: boundary[0..=half] (forward)
    // Side B: boundary[0], boundary[n-1], boundary[n-2], ..., boundary[half] (backward)
    for k in 0..half {
        let a0 = boundary[k];
        let a1 = boundary[k + 1];
        let b0 = boundary[if k == 0 { 0 } else { n - k }];
        let b1 = boundary[n - k - 1];

        if a0 == b0 && a1 == b1 {
            // Both shared — degenerate, skip
        } else if a0 == b0 {
            // Shared start vertex → triangle
            if flip {
                faces.push(vec![a0, b1, a1]);
            } else {
                faces.push(vec![a0, a1, b1]);
            }
        } else if a1 == b1 {
            // Shared end vertex → triangle (a0, a1=b1, b0)
            if flip {
                faces.push(vec![a0, b0, a1]);
            } else {
                faces.push(vec![a0, a1, b0]);
            }
        } else if flip {
            faces.push(vec![a0, b0, b1, a1]);
        } else {
            faces.push(vec![a0, a1, b1, b0]);
        }
    }
}

/// Project 3D boundary vertices to a local 2D coordinate frame for earcut.
fn project_boundary_to_2d(indices: &[usize], positions: &[[f64; 3]]) -> Vec<[f64; 2]> {
    if indices.len() < 2 {
        return indices.iter().map(|_| [0.0, 0.0]).collect();
    }

    // Normal via Newell's method
    let (mut nx, mut ny, mut nz) = (0.0, 0.0, 0.0);
    let n = indices.len();
    for i in 0..n {
        let j = (i + 1) % n;
        let vi = positions[indices[i]];
        let vj = positions[indices[j]];
        nx += (vi[1] - vj[1]) * (vi[2] + vj[2]);
        ny += (vi[2] - vj[2]) * (vi[0] + vj[0]);
        nz += (vi[0] - vj[0]) * (vi[1] + vj[1]);
    }
    let len = (nx * nx + ny * ny + nz * nz).sqrt();
    if len < 1e-12 {
        // Degenerate — fall back to XY
        return indices
            .iter()
            .map(|&i| [positions[i][0], positions[i][1]])
            .collect();
    }
    let normal = [nx / len, ny / len, nz / len];

    // Tangent from first edge
    let p0 = positions[indices[0]];
    let p1 = positions[indices[1]];
    let edge = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
    let elen = (edge[0] * edge[0] + edge[1] * edge[1] + edge[2] * edge[2]).sqrt();
    let tangent = if elen > 1e-12 {
        [edge[0] / elen, edge[1] / elen, edge[2] / elen]
    } else if normal[0].abs() < 0.9 {
        [1.0, 0.0, 0.0]
    } else {
        [0.0, 1.0, 0.0]
    };

    // Bitangent = cross(normal, tangent)
    let bi = [
        normal[1] * tangent[2] - normal[2] * tangent[1],
        normal[2] * tangent[0] - normal[0] * tangent[2],
        normal[0] * tangent[1] - normal[1] * tangent[0],
    ];

    indices
        .iter()
        .map(|&i| {
            let p = positions[i];
            let d = [p[0] - p0[0], p[1] - p0[1], p[2] - p0[2]];
            [
                d[0] * tangent[0] + d[1] * tangent[1] + d[2] * tangent[2],
                d[0] * bi[0] + d[1] * bi[1] + d[2] * bi[2],
            ]
        })
        .collect()
}
