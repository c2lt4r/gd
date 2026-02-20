use super::{MeshState, Transform3D};
use super::half_edge::HalfEdgeMesh;

/// Kind of spatial relationship issue between parts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IssueKind {
    /// AABBs intersect — parts occupy the same space.
    Overlap,
    /// AABBs nearly touch but a gap exists — probably should be connected.
    Floating,
}

/// A spatial relationship issue between two parts.
#[derive(Debug, Clone)]
pub struct SpatialIssue {
    pub kind: IssueKind,
    pub part_a: String,
    pub part_b: String,
    pub detail: String,
}

impl SpatialIssue {
    pub fn to_json(&self) -> serde_json::Value {
        let kind_str = match self.kind {
            IssueKind::Overlap => "overlap",
            IssueKind::Floating => "floating",
        };
        serde_json::json!({
            "kind": kind_str,
            "parts": [self.part_a, self.part_b],
            "error": self.detail,
        })
    }
}

/// Compute the world-space AABB for a mesh by transforming all 8 local AABB
/// corners through the part's transform, then taking min/max.
pub fn world_aabb(mesh: &HalfEdgeMesh, transform: &Transform3D) -> ([f64; 3], [f64; 3]) {
    let (local_min, local_max) = mesh.aabb();
    if mesh.vertices.is_empty() {
        return (local_min, local_max);
    }

    if transform.is_identity() {
        return (local_min, local_max);
    }

    // Transform all 8 corners of the local AABB
    let corners = [
        [local_min[0], local_min[1], local_min[2]],
        [local_max[0], local_min[1], local_min[2]],
        [local_min[0], local_max[1], local_min[2]],
        [local_max[0], local_max[1], local_min[2]],
        [local_min[0], local_min[1], local_max[2]],
        [local_max[0], local_min[1], local_max[2]],
        [local_min[0], local_max[1], local_max[2]],
        [local_max[0], local_max[1], local_max[2]],
    ];

    let first = transform.apply_point(corners[0]);
    let mut w_min = first;
    let mut w_max = first;

    for &corner in &corners[1..] {
        let p = transform.apply_point(corner);
        for i in 0..3 {
            if p[i] < w_min[i] {
                w_min[i] = p[i];
            }
            if p[i] > w_max[i] {
                w_max[i] = p[i];
            }
        }
    }

    (w_min, w_max)
}

/// Gap threshold for detecting floating parts (units).
const FLOATING_GAP_THRESHOLD: f64 = 0.1;

/// Check all part-pair relationships for overlaps and near-miss gaps.
pub fn check_part_relationships(state: &MeshState) -> Vec<SpatialIssue> {
    let parts: Vec<(&String, &super::MeshPart)> = state
        .parts
        .iter()
        .filter(|(_, p)| p.mesh.vertex_count() > 0)
        .collect();

    if parts.len() < 2 {
        return Vec::new();
    }

    // Pre-compute world AABBs
    let aabbs: Vec<(&String, [f64; 3], [f64; 3])> = parts
        .iter()
        .map(|(name, part)| {
            let (wmin, wmax) = world_aabb(&part.mesh, &part.transform);
            (*name, wmin, wmax)
        })
        .collect();

    let mut issues = Vec::new();

    for i in 0..aabbs.len() {
        for j in (i + 1)..aabbs.len() {
            let (name_a, min_a, max_a) = &aabbs[i];
            let (name_b, min_b, max_b) = &aabbs[j];

            // Per-axis overlap depth (positive = overlap, negative = gap)
            let mut overlap = [0.0_f64; 3];
            let mut all_overlap = true;
            let mut min_gap = f64::MAX;

            for axis in 0..3 {
                let o = min_a[axis].max(min_b[axis]);
                let e = max_a[axis].min(max_b[axis]);
                let depth = e - o;
                overlap[axis] = depth;
                if depth < 0.0 {
                    all_overlap = false;
                    let gap = -depth;
                    if gap < min_gap {
                        min_gap = gap;
                    }
                }
            }

            if all_overlap {
                // All three axes overlap — AABBs intersect
                let max_depth = overlap[0].min(overlap[1]).min(overlap[2]);
                if max_depth > 1e-6 {
                    let detail = format!(
                        "Parts '{name_a}' and '{name_b}' overlap by {max_depth:.2} units \
                         — solid parts should not share space. \
                         Use boolean subtract to cut the intersection, \
                         or reposition with translate.",
                    );
                    issues.push(SpatialIssue {
                        kind: IssueKind::Overlap,
                        part_a: (*name_a).clone(),
                        part_b: (*name_b).clone(),
                        detail,
                    });
                }
            } else if min_gap < FLOATING_GAP_THRESHOLD {
                let detail = format!(
                    "Parts '{name_a}' and '{name_b}' are {min_gap:.2} units apart \
                     — if they should connect, adjust position or use merge-verts.",
                );
                issues.push(SpatialIssue {
                    kind: IssueKind::Floating,
                    part_a: (*name_a).clone(),
                    part_b: (*name_b).clone(),
                    detail,
                });
            }
        }
    }

    issues
}

/// Count edges with != 2 adjacent faces (non-manifold or boundary edges).
pub fn count_non_manifold_edges(mesh: &HalfEdgeMesh) -> usize {
    // In a half-edge mesh, boundary edges have face == None.
    // Non-manifold edges would have more than 2 faces, but our half-edge
    // structure only supports pairs, so we count boundary half-edges
    // (each boundary half-edge pair = one boundary edge).
    mesh.half_edges
        .iter()
        .filter(|he| he.face.is_none())
        .count()
}

/// Returns true if the mesh is watertight (no boundary edges).
pub fn is_watertight(mesh: &HalfEdgeMesh) -> bool {
    if mesh.vertices.is_empty() {
        return false;
    }
    count_non_manifold_edges(mesh) == 0
}

/// Build a relationship report for describe output.
pub fn relationship_report(state: &MeshState) -> Vec<serde_json::Value> {
    let issues = check_part_relationships(state);
    issues.iter().map(|issue| {
        let mut entry = serde_json::json!({
            "parts": [issue.part_a, issue.part_b],
            "error": issue.detail,
        });
        match issue.kind {
            IssueKind::Overlap => {
                entry["status"] = serde_json::json!("overlapping");
            }
            IssueKind::Floating => {
                entry["status"] = serde_json::json!("floating");
            }
        }
        entry
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::mesh::{MeshPart, MeshState, Transform3D};
    use crate::core::mesh::half_edge::HalfEdgeMesh;

    /// Build a simple box mesh at the origin with the given half-extents.
    fn make_box(hx: f64, hy: f64, hz: f64) -> HalfEdgeMesh {
        // 8 vertices of an axis-aligned box
        let positions = vec![
            [-hx, -hy, -hz],
            [ hx, -hy, -hz],
            [ hx,  hy, -hz],
            [-hx,  hy, -hz],
            [-hx, -hy,  hz],
            [ hx, -hy,  hz],
            [ hx,  hy,  hz],
            [-hx,  hy,  hz],
        ];
        // 6 faces (quads)
        let faces: Vec<&[usize]> = vec![
            &[0, 3, 2, 1], // front (-Z)
            &[4, 5, 6, 7], // back (+Z)
            &[0, 1, 5, 4], // bottom (-Y)
            &[2, 3, 7, 6], // top (+Y)
            &[0, 4, 7, 3], // left (-X)
            &[1, 2, 6, 5], // right (+X)
        ];
        HalfEdgeMesh::from_polygons(&positions, &faces)
    }

    fn make_state_with_parts(parts: Vec<(&str, HalfEdgeMesh, [f64; 3])>) -> MeshState {
        let first_name = parts[0].0;
        let mut state = MeshState::new(first_name);
        for (name, mesh, pos) in parts {
            let mut part = MeshPart::new();
            part.mesh = mesh;
            part.transform = Transform3D {
                position: pos,
                ..Transform3D::default()
            };
            state.parts.insert(name.to_string(), part);
        }
        state.active = first_name.to_string();
        state
    }

    #[test]
    fn test_no_overlap_separate_parts() {
        let state = make_state_with_parts(vec![
            ("body", make_box(1.0, 1.0, 1.0), [0.0, 0.0, 0.0]),
            ("wing", make_box(0.5, 0.5, 0.5), [5.0, 0.0, 0.0]),
        ]);
        let issues = check_part_relationships(&state);
        assert!(issues.is_empty(), "Expected no issues for separate parts");
    }

    #[test]
    fn test_overlap_detected() {
        let state = make_state_with_parts(vec![
            ("body", make_box(1.0, 1.0, 1.0), [0.0, 0.0, 0.0]),
            ("cockpit", make_box(0.5, 0.5, 0.5), [0.5, 0.0, 0.0]),
        ]);
        let issues = check_part_relationships(&state);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].kind, IssueKind::Overlap);
        assert!(issues[0].detail.contains("overlap"));
    }

    #[test]
    fn test_floating_detected() {
        // Box A: [-1,1] on all axes at origin
        // Box B: [-0.5,0.5] on all axes at position [1.05, 0, 0]
        // Gap on X: A.max_x=1.0, B.min_x=1.05-0.5=0.55 → gap = 0 (overlaps on X since 1.0 > 0.55)
        // Actually let me recalculate: A spans [-1,1], B at x=1.05 spans [0.55, 1.55]
        // X overlap: min(1.0, 1.55) - max(-1.0, 0.55) = 1.0 - 0.55 = 0.45 > 0 (overlap)
        // They overlap on all axes. Let me place them further apart.
        let state = make_state_with_parts(vec![
            ("body", make_box(1.0, 1.0, 1.0), [0.0, 0.0, 0.0]),
            ("wing", make_box(0.5, 0.5, 0.5), [1.55, 0.0, 0.0]),
        ]);
        // A: X=[-1, 1], B: X=[1.05, 2.05] → gap = 1.05 - 1.0 = 0.05 on X
        // Y: A=[-1,1], B=[-0.5, 0.5] → overlap
        // Z: same as Y → overlap
        // min_gap = 0.05 < 0.1 → floating
        let issues = check_part_relationships(&state);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].kind, IssueKind::Floating);
        assert!(issues[0].detail.contains("apart"));
    }

    #[test]
    fn test_single_part_no_issues() {
        let state = make_state_with_parts(vec![
            ("body", make_box(1.0, 1.0, 1.0), [0.0, 0.0, 0.0]),
        ]);
        let issues = check_part_relationships(&state);
        assert!(issues.is_empty());
    }

    #[test]
    fn test_non_manifold_edge_count() {
        // A single quad has boundary edges (it's an open surface)
        let positions = vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
        ];
        let faces: Vec<&[usize]> = vec![&[0, 1, 2, 3]];
        let mesh = HalfEdgeMesh::from_polygons(&positions, &faces);
        let count = count_non_manifold_edges(&mesh);
        assert!(count > 0, "Open surface should have boundary edges");
    }

    #[test]
    fn test_watertight_closed_mesh() {
        let mesh = make_box(1.0, 1.0, 1.0);
        assert!(
            is_watertight(&mesh),
            "Closed box should be watertight"
        );
    }

    #[test]
    fn test_empty_mesh_not_watertight() {
        let mesh = HalfEdgeMesh::default();
        assert!(!is_watertight(&mesh));
    }

    #[test]
    fn test_world_aabb_with_translation() {
        let mesh = make_box(1.0, 1.0, 1.0);
        let transform = Transform3D {
            position: [5.0, 0.0, 0.0],
            ..Transform3D::default()
        };
        let (wmin, wmax) = world_aabb(&mesh, &transform);
        assert!((wmin[0] - 4.0).abs() < 1e-6);
        assert!((wmax[0] - 6.0).abs() < 1e-6);
    }

    #[test]
    fn test_spatial_issue_json() {
        let issue = SpatialIssue {
            kind: IssueKind::Overlap,
            part_a: "body".to_string(),
            part_b: "cockpit".to_string(),
            detail: "test detail".to_string(),
        };
        let json = issue.to_json();
        assert_eq!(json["kind"], "overlap");
        assert_eq!(json["parts"][0], "body");
        assert_eq!(json["parts"][1], "cockpit");
    }
}
