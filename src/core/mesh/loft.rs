use super::half_edge::HalfEdgeMesh;
use super::profile::triangulate_2d;

/// Loft: connect multiple cross-section profiles into a smooth surface.
///
/// Each section is a set of 2D points + a 3D position along the path.
/// Adjacent sections are connected with quad strips (split into triangles).
/// End caps are optional.
///
/// All sections must have the same number of points.
#[allow(dead_code)]
pub fn loft(sections: &[Vec<[f64; 3]>], cap_start: bool, cap_end: bool) -> Option<HalfEdgeMesh> {
    if sections.len() < 2 {
        return None;
    }

    let n_pts = sections[0].len();
    if n_pts < 3 || sections.iter().any(|s| s.len() != n_pts) {
        return None;
    }

    let n_sections = sections.len();

    // Flatten positions
    let mut positions: Vec<[f64; 3]> = Vec::with_capacity(n_sections * n_pts);
    for section in sections {
        positions.extend_from_slice(section);
    }

    let mut faces: Vec<Vec<usize>> = Vec::new();

    // Connect adjacent sections with quads
    for s in 0..n_sections - 1 {
        let cur_base = s * n_pts;
        let next_base = (s + 1) * n_pts;

        for i in 0..n_pts {
            let j = (i + 1) % n_pts;

            let ci = cur_base + i;
            let cj = cur_base + j;
            let ni = next_base + i;
            let nj = next_base + j;

            faces.push(vec![ci, cj, nj, ni]);
        }
    }

    // End caps — stay triangulated
    if cap_start {
        let pts_2d: Vec<[f64; 2]> = sections[0].iter().map(|p| [p[0], p[1]]).collect();
        if let Some(tri) = triangulate_2d(&pts_2d) {
            for t in tri.chunks(3) {
                faces.push(vec![t[2], t[1], t[0]]);
            }
        }
    }

    if cap_end {
        let last_base = (n_sections - 1) * n_pts;
        let pts_2d: Vec<[f64; 2]> = sections[n_sections - 1]
            .iter()
            .map(|p| [p[0], p[1]])
            .collect();
        if let Some(tri) = triangulate_2d(&pts_2d) {
            for t in tri.chunks(3) {
                faces.push(vec![last_base + t[0], last_base + t[1], last_base + t[2]]);
            }
        }
    }

    if faces.is_empty() {
        return None;
    }

    let face_slices: Vec<&[usize]> = faces.iter().map(Vec::as_slice).collect();
    Some(HalfEdgeMesh::from_polygons(&positions, &face_slices))
}
