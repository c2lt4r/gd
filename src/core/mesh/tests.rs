use super::half_edge::HalfEdgeMesh;
use super::normals;
use super::{MeshState, PlaneKind};

// ── HalfEdgeMesh construction ───────────────────────────────────────

fn single_triangle() -> HalfEdgeMesh {
    let positions = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
    let indices = [0, 1, 2];
    HalfEdgeMesh::from_triangles(&positions, &indices)
}

fn two_triangles() -> HalfEdgeMesh {
    // Two triangles sharing edge 1-2:
    // Triangle 0: 0, 1, 2
    // Triangle 1: 1, 3, 2
    let positions = [
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [0.5, 1.0, 0.0],
        [1.5, 0.5, 0.0],
    ];
    let indices = [0, 1, 2, 1, 3, 2];
    HalfEdgeMesh::from_triangles(&positions, &indices)
}

fn cube_mesh() -> HalfEdgeMesh {
    // A simple cube: 8 vertices, 12 triangles
    #[rustfmt::skip]
    let positions = [
        [-0.5, -0.5, -0.5], [ 0.5, -0.5, -0.5],
        [ 0.5,  0.5, -0.5], [-0.5,  0.5, -0.5],
        [-0.5, -0.5,  0.5], [ 0.5, -0.5,  0.5],
        [ 0.5,  0.5,  0.5], [-0.5,  0.5,  0.5],
    ];
    #[rustfmt::skip]
    let indices = [
        // Front face (z = -0.5)
        0, 1, 2,  0, 2, 3,
        // Back face (z = 0.5)
        5, 4, 7,  5, 7, 6,
        // Top face (y = 0.5)
        3, 2, 6,  3, 6, 7,
        // Bottom face (y = -0.5)
        4, 5, 1,  4, 1, 0,
        // Right face (x = 0.5)
        1, 5, 6,  1, 6, 2,
        // Left face (x = -0.5)
        4, 0, 3,  4, 3, 7,
    ];
    HalfEdgeMesh::from_triangles(&positions, &indices)
}

#[test]
fn single_triangle_structure() {
    let mesh = single_triangle();
    assert_eq!(mesh.vertices.len(), 3);
    assert_eq!(mesh.faces.len(), 1);
    // 3 interior + 3 boundary = 6 half-edges
    assert_eq!(mesh.half_edges.len(), 6);
}

#[test]
fn single_triangle_face_vertices() {
    let mesh = single_triangle();
    let verts = mesh.face_vertices(0);
    assert_eq!(verts.len(), 3);
    // Should contain vertices 0, 1, 2 (in some order matching winding)
    assert!(verts.contains(&0));
    assert!(verts.contains(&1));
    assert!(verts.contains(&2));
}

#[test]
fn two_triangles_structure() {
    let mesh = two_triangles();
    assert_eq!(mesh.vertices.len(), 4);
    assert_eq!(mesh.faces.len(), 2);
    // Each triangle has 3 half-edges; shared edge has paired twins,
    // boundary edges (4 total) each get a boundary twin
    // Interior: 6, Boundary: 4 = 10
    assert!(mesh.half_edges.len() >= 6);
}

#[test]
fn two_triangles_neighbors() {
    let mesh = two_triangles();
    let neighbors_0 = mesh.face_neighbors(0);
    let neighbors_1 = mesh.face_neighbors(1);
    assert!(neighbors_0.contains(&1));
    assert!(neighbors_1.contains(&0));
}

#[test]
fn two_triangles_vertex_faces() {
    let mesh = two_triangles();
    // Vertex 0 is only in face 0
    let faces_0 = mesh.vertex_faces(0);
    assert_eq!(faces_0.len(), 1);
    assert!(faces_0.contains(&0));

    // Vertex 1 is in both faces
    let faces_1 = mesh.vertex_faces(1);
    assert_eq!(faces_1.len(), 2);
}

#[test]
fn cube_structure() {
    let mesh = cube_mesh();
    assert_eq!(mesh.vertices.len(), 8);
    assert_eq!(mesh.faces.len(), 12);
    // Closed mesh: no boundary edges
    let boundary = mesh.boundary_edges();
    assert_eq!(boundary.len(), 0, "cube should have no boundary edges");
}

#[test]
fn single_triangle_boundary() {
    let mesh = single_triangle();
    let boundary = mesh.boundary_edges();
    assert_eq!(boundary.len(), 3, "single triangle has 3 boundary edges");
}

// ── Export ───────────────────────────────────────────────────────────

#[test]
fn to_arrays_single_triangle() {
    let mesh = single_triangle();
    let (positions, normals, indices) = mesh.to_arrays();
    assert_eq!(positions.len(), 9); // 3 vertices * 3 components
    assert_eq!(normals.len(), 9);
    assert_eq!(indices.len(), 3);
}

#[test]
fn to_arrays_cube() {
    let mesh = cube_mesh();
    let (positions, normals, indices) = mesh.to_arrays();
    assert_eq!(positions.len(), 24); // 8 vertices * 3
    assert_eq!(normals.len(), 24);
    assert_eq!(indices.len(), 36); // 12 triangles * 3
}

// ── AABB ────────────────────────────────────────────────────────────

#[test]
fn aabb_cube() {
    let mesh = cube_mesh();
    let (min, max) = mesh.aabb();
    for i in 0..3 {
        assert!((min[i] - -0.5).abs() < 1e-10);
        assert!((max[i] - 0.5).abs() < 1e-10);
    }
}

#[test]
fn aabb_empty() {
    let mesh = HalfEdgeMesh::default();
    let (min, max) = mesh.aabb();
    for i in 0..3 {
        assert!(min[i].abs() < f64::EPSILON);
        assert!(max[i].abs() < f64::EPSILON);
    }
}

// ── Normals ─────────────────────────────────────────────────────────

#[test]
fn face_normal_xy_plane() {
    let mesh = single_triangle();
    let normal = normals::compute_face_normal(&mesh, 0);
    // Triangle in XY plane: normal should point in Z
    assert!(normal[2].abs() > 0.99);
    assert!(normal[0].abs() < 0.01);
    assert!(normal[1].abs() < 0.01);
}

#[test]
fn vertex_normals_single_triangle() {
    let mesh = single_triangle();
    let norms = normals::compute_vertex_normals(&mesh);
    assert_eq!(norms.len(), 3);
    // All normals should point in +Z or -Z
    for n in &norms {
        assert!(n[2].abs() > 0.99);
    }
}

// ── Fix winding ─────────────────────────────────────────────────────

#[test]
fn fix_winding_cube() {
    let mut mesh = cube_mesh();
    // Flip a few faces to make them inconsistent
    mesh.flip_face(0);
    mesh.flip_face(3);
    let flipped = normals::fix_winding(&mut mesh);
    // Should fix some faces
    assert!(flipped > 0);
}

// ── Flip all ────────────────────────────────────────────────────────

#[test]
fn flip_all_reverses() {
    let mesh = single_triangle();
    let original_normal = normals::compute_face_normal(&mesh, 0);

    let mut flipped = mesh.clone();
    normals::flip_all(&mut flipped);
    let flipped_normal = normals::compute_face_normal(&flipped, 0);

    // Normal should be reversed
    assert!((original_normal[2] + flipped_normal[2]).abs() < 0.01);
}

// ── Split edge ──────────────────────────────────────────────────────

#[test]
fn split_edge_adds_vertex() {
    let mut mesh = two_triangles();
    let original_verts = mesh.vertices.len();
    mesh.split_edge(0);
    assert_eq!(mesh.vertices.len(), original_verts + 1);
}

// ── Find half-edge ──────────────────────────────────────────────────

#[test]
fn find_half_edge_exists() {
    let mesh = single_triangle();
    // There should be an edge from 0 to 1
    assert!(mesh.find_half_edge(0, 1).is_some());
}

#[test]
fn find_half_edge_missing() {
    let mesh = single_triangle();
    // No edge from 0 to 0
    assert!(mesh.find_half_edge(0, 0).is_none());
}

// ── MeshState persistence ───────────────────────────────────────────

#[test]
fn mesh_state_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = MeshState::new("body");

    // Give it a profile and mesh
    {
        let part = state.active_part_mut().unwrap();
        part.profile_points = Some(vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]);
        part.profile_plane = Some(PlaneKind::Front);
        part.mesh = cube_mesh();
    }

    state.save(dir.path()).unwrap();
    let loaded = MeshState::load(dir.path()).unwrap();

    assert_eq!(loaded.active, "body");
    assert_eq!(loaded.parts.len(), 1);
    let part = loaded.active_part().unwrap();
    assert_eq!(part.mesh.vertices.len(), 8);
    assert_eq!(part.mesh.faces.len(), 12);
    assert_eq!(part.profile_plane, Some(PlaneKind::Front));
    assert_eq!(part.profile_points.as_ref().unwrap().len(), 4);
}

#[test]
fn mesh_state_load_missing() {
    let dir = tempfile::tempdir().unwrap();
    let result = MeshState::load(dir.path());
    assert!(result.is_err());
}

#[test]
fn mesh_state_multiple_parts() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = MeshState::new("body");
    state.parts.insert("wing".to_string(), MeshPart::new());
    state.active = "wing".to_string();

    state.save(dir.path()).unwrap();
    let loaded = MeshState::load(dir.path()).unwrap();
    assert_eq!(loaded.parts.len(), 2);
    assert_eq!(loaded.active, "wing");
}

// ── Push script generation ──────────────────────────────────────────

#[test]
fn generate_push_script_parses() {
    let mut state = MeshState::new("body");
    {
        let part = state.active_part_mut().unwrap();
        part.mesh = single_triangle();
    }

    let script = state.generate_push_script("body").unwrap();
    // Should be valid GDScript
    let tree = crate::core::parser::parse(&script).unwrap();
    assert!(
        !tree.root_node().has_error(),
        "Push script has parse errors:\n{script}"
    );
}

#[test]
fn generate_push_script_contains_arrays() {
    let mut state = MeshState::new("body");
    {
        let part = state.active_part_mut().unwrap();
        part.mesh = single_triangle();
    }

    let script = state.generate_push_script("body").unwrap();
    assert!(script.contains("PackedVector3Array"));
    assert!(script.contains("PackedInt32Array"));
    assert!(script.contains("ArrayMesh.new()"));
    assert!(script.contains("PRIMITIVE_TRIANGLES"));
}

#[test]
fn generate_push_script_preserves_material() {
    let mut state = MeshState::new("body");
    {
        let part = state.active_part_mut().unwrap();
        part.mesh = single_triangle();
    }

    let script = state.generate_push_script("body").unwrap();
    assert!(script.contains("part_color"));
    assert!(script.contains("material_override"));
}

// ── PlaneKind ───────────────────────────────────────────────────────

#[test]
fn plane_kind_extrude_axis() {
    assert_eq!(PlaneKind::Front.extrude_axis(), 2);
    assert_eq!(PlaneKind::Side.extrude_axis(), 0);
    assert_eq!(PlaneKind::Top.extrude_axis(), 1);
}

// ── fmt_f64 ─────────────────────────────────────────────────────────

#[test]
fn fmt_f64_zero() {
    assert_eq!(super::fmt_f64(0.0), "0");
}

#[test]
fn fmt_f64_integer() {
    assert_eq!(super::fmt_f64(1.0), "1");
}

#[test]
fn fmt_f64_decimal() {
    assert_eq!(super::fmt_f64(1.5), "1.5");
}

#[test]
fn fmt_f64_negative() {
    assert_eq!(super::fmt_f64(-0.25), "-0.25");
}

// ── Flip caps ───────────────────────────────────────────────────────

#[test]
fn flip_caps_z_axis() {
    let mut mesh = cube_mesh();
    let count = normals::flip_caps(&mut mesh, 2); // Z axis
    // Front and back face triangles have Z-aligned normals
    assert!(count > 0);
}

use super::MeshPart;
use super::extrude;
use super::profile;

// ── Profile triangulation ───────────────────────────────────────────

#[test]
fn profile_square_front() {
    let points = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
    let mesh = profile::triangulate_profile(&points, PlaneKind::Front).unwrap();
    assert_eq!(mesh.vertices.len(), 4);
    assert_eq!(mesh.faces.len(), 2); // 4-gon = 2 triangles
}

#[test]
fn profile_triangle_side() {
    let points = [[0.0, 0.0], [1.0, 0.0], [0.5, 1.0]];
    let mesh = profile::triangulate_profile(&points, PlaneKind::Side).unwrap();
    assert_eq!(mesh.vertices.len(), 3);
    assert_eq!(mesh.faces.len(), 1);
    // Side plane: x -> z, y -> y, so vertices should be at z=x, y=y
    assert!((mesh.vertices[0].position[0]).abs() < 1e-10); // x = 0 (side plane)
    assert!((mesh.vertices[0].position[2]).abs() < 1e-10); // z = profile_x = 0
}

#[test]
fn profile_pentagon_top() {
    let points = [
        [0.0, 1.0],
        [-0.95, 0.31],
        [-0.59, -0.81],
        [0.59, -0.81],
        [0.95, 0.31],
    ];
    let mesh = profile::triangulate_profile(&points, PlaneKind::Top).unwrap();
    assert_eq!(mesh.vertices.len(), 5);
    assert_eq!(mesh.faces.len(), 3); // pentagon = 3 triangles
    // Top plane: y = 0
    for v in &mesh.vertices {
        assert!((v.position[1]).abs() < 1e-10);
    }
}

#[test]
fn profile_too_few_points() {
    let points = [[0.0, 0.0], [1.0, 0.0]];
    assert!(profile::triangulate_profile(&points, PlaneKind::Front).is_none());
}

#[test]
fn signed_area_ccw() {
    let points = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
    let area = profile::signed_area_2x(&points);
    assert!(area > 0.0, "CCW square should have positive signed area");
}

#[test]
fn signed_area_cw() {
    let points = [[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0]];
    let area = profile::signed_area_2x(&points);
    assert!(area < 0.0, "CW square should have negative signed area");
}

// ── Extrude ─────────────────────────────────────────────────────────

#[test]
fn extrude_square_front() {
    let points = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
    let mesh = extrude::extrude(&points, PlaneKind::Front, 2.0, 1).unwrap();
    // 4 profile points × 2 sections = 8 vertices
    assert_eq!(mesh.vertices.len(), 8);
    // 2 cap triangles × 2 caps + 4 side quads = 8 faces
    assert_eq!(mesh.faces.len(), 8);
}

#[test]
fn extrude_triangle_side() {
    let points = [[0.0, 0.0], [1.0, 0.0], [0.5, 1.0]];
    let mesh = extrude::extrude(&points, PlaneKind::Side, 3.0, 1).unwrap();
    // 3 profile points × 2 sections = 6 vertices
    assert_eq!(mesh.vertices.len(), 6);
    // 1 cap tri × 2 caps + 3 side quads = 5 faces
    assert_eq!(mesh.faces.len(), 5);
}

#[test]
fn extrude_with_segments() {
    let points = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
    let mesh = extrude::extrude(&points, PlaneKind::Front, 2.0, 4).unwrap();
    // 4 profile points × 5 sections = 20 vertices
    assert_eq!(mesh.vertices.len(), 20);
    // 2 cap tris × 2 + 4 side quads × 4 segments = 4 + 16 = 20
    assert_eq!(mesh.faces.len(), 20);
}

#[test]
fn extrude_depth_range() {
    let points = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
    let mesh = extrude::extrude(&points, PlaneKind::Front, 4.0, 1).unwrap();
    let (min, max) = mesh.aabb();
    // Front plane: extrude along Z, from +2 to -2
    assert!((min[2] - -2.0).abs() < 1e-10);
    assert!((max[2] - 2.0).abs() < 1e-10);
}

#[test]
fn extrude_produces_watertight_mesh() {
    let points = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
    let mesh = extrude::extrude(&points, PlaneKind::Front, 2.0, 1).unwrap();
    // A watertight mesh should have no boundary edges
    let boundary = mesh.boundary_edges();
    assert_eq!(
        boundary.len(),
        0,
        "extruded mesh should be watertight (no boundary edges)"
    );
}

/// Assert ALL face normals point outward from mesh center (dot product > 0).
fn assert_all_normals_outward(mesh: &HalfEdgeMesh, label: &str) {
    let center = mesh_center(mesh);
    let total = mesh.faces.len();
    let mut inward = Vec::new();
    for f in 0..total {
        let normal = normals::compute_face_normal(mesh, f);
        let verts = mesh.face_vertices(f);
        if verts.is_empty() {
            continue;
        }
        let fc = face_center(mesh, &verts);
        let outward = [fc[0] - center[0], fc[1] - center[1], fc[2] - center[2]];
        let dot = normal[0] * outward[0] + normal[1] * outward[1] + normal[2] * outward[2];
        if dot < -1e-12 {
            inward.push(f);
        }
    }
    assert!(
        inward.is_empty(),
        "{label}: {}/{total} faces have inward normals (faces: {inward:?})",
        inward.len(),
    );
}

#[test]
fn extrude_normals_outward_front() {
    let points = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
    let mesh = extrude::extrude(&points, PlaneKind::Front, 2.0, 1).unwrap();
    assert_all_normals_outward(&mesh, "Front plane CCW");
}

#[test]
fn extrude_normals_outward_side() {
    let points = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
    let mesh = extrude::extrude(&points, PlaneKind::Side, 2.0, 1).unwrap();
    assert_all_normals_outward(&mesh, "Side plane CCW");
}

#[test]
fn extrude_normals_outward_top() {
    let points = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
    let mesh = extrude::extrude(&points, PlaneKind::Top, 2.0, 1).unwrap();
    assert_all_normals_outward(&mesh, "Top plane CCW");
}

#[test]
fn extrude_normals_outward_cw_input() {
    // CW winding in 2D — should still produce outward normals
    let points = [[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0]];
    let mesh = extrude::extrude(&points, PlaneKind::Front, 2.0, 1).unwrap();
    assert_all_normals_outward(&mesh, "Front plane CW");
    let mesh = extrude::extrude(&points, PlaneKind::Side, 2.0, 1).unwrap();
    assert_all_normals_outward(&mesh, "Side plane CW");
    let mesh = extrude::extrude(&points, PlaneKind::Top, 2.0, 1).unwrap();
    assert_all_normals_outward(&mesh, "Top plane CW");
}

#[test]
fn extrude_normals_outward_with_segments() {
    let points = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
    let mesh = extrude::extrude(&points, PlaneKind::Side, 2.0, 4).unwrap();
    assert_all_normals_outward(&mesh, "Side plane 4 segments");
}

#[test]
fn fix_winding_on_inverted_extrusion() {
    // Extrude, flip all normals (intentionally invert), then fix_winding should fix them
    let points = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
    let mut mesh = extrude::extrude(&points, PlaneKind::Front, 2.0, 1).unwrap();
    normals::flip_all(&mut mesh);
    let fixed = normals::fix_winding(&mut mesh);
    assert!(fixed > 0, "fix_winding should flip inverted faces");
    assert_all_normals_outward(&mesh, "fix_winding after flip_all");
}

#[test]
fn extrude_push_script_parses() {
    let points = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
    let mut state = MeshState::new("body");
    {
        let part = state.active_part_mut().unwrap();
        part.mesh = extrude::extrude(&points, PlaneKind::Front, 2.0, 1).unwrap();
        part.profile_points = Some(points.to_vec());
        part.profile_plane = Some(PlaneKind::Front);
    }

    let script = state.generate_push_script("body").unwrap();
    let tree = crate::core::parser::parse(&script).unwrap();
    assert!(
        !tree.root_node().has_error(),
        "Push script has parse errors:\n{script}"
    );
}

fn mesh_center(mesh: &HalfEdgeMesh) -> [f64; 3] {
    let n = mesh.vertices.len() as f64;
    let mut c = [0.0; 3];
    for v in &mesh.vertices {
        c[0] += v.position[0];
        c[1] += v.position[1];
        c[2] += v.position[2];
    }
    [c[0] / n, c[1] / n, c[2] / n]
}

fn face_center(mesh: &HalfEdgeMesh, verts: &[usize]) -> [f64; 3] {
    let n = verts.len() as f64;
    let mut c = [0.0; 3];
    for &v in verts {
        c[0] += mesh.vertices[v].position[0];
        c[1] += mesh.vertices[v].position[1];
        c[2] += mesh.vertices[v].position[2];
    }
    [c[0] / n, c[1] / n, c[2] / n]
}

// ── Taper ──────────────────────────────────────────────────────────

use super::taper;

#[test]
fn taper_narrows_top() {
    let points = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
    let mut mesh = extrude::extrude(&points, PlaneKind::Front, 2.0, 1).unwrap();
    let original_aabb = mesh.aabb();
    let count = taper::taper(&mut mesh, 2, 1.0, 0.5, None, None);
    assert!(count > 0, "taper should modify some vertices");
    // AABB should be same or narrower along X/Y
    let (new_min, new_max) = mesh.aabb();
    assert!(
        (new_max[0] - new_min[0]) <= (original_aabb.1[0] - original_aabb.0[0]) + 1e-9,
        "tapered mesh should be narrower or equal"
    );
}

#[test]
fn taper_with_range() {
    let points = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
    let mut mesh = extrude::extrude(&points, PlaneKind::Front, 4.0, 4).unwrap();
    let count = taper::taper(&mut mesh, 2, 1.0, 0.5, None, Some((0.0, 0.5)));
    assert!(count > 0, "taper with range should modify some vertices");
}

#[test]
fn taper_empty_mesh() {
    let mut mesh = HalfEdgeMesh::default();
    let count = taper::taper(&mut mesh, 0, 1.0, 0.5, None, None);
    assert_eq!(count, 0);
}

// ── Mirror ─────────────────────────────────────────────────────────

use super::mirror;

#[test]
fn mirror_negates_axis() {
    let points = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
    let mut mesh = extrude::extrude(&points, PlaneKind::Front, 2.0, 1).unwrap();
    let original_aabb = mesh.aabb();
    mirror::mirror(&mut mesh, 0); // Mirror across X

    let (min, max) = mesh.aabb();
    // After X mirror: min_x should be negated max_x
    assert!((min[0] - -original_aabb.1[0]).abs() < 1e-9);
    assert!((max[0] - -original_aabb.0[0]).abs() < 1e-9);
}

#[test]
fn mirror_preserves_face_count() {
    let mesh_original = cube_mesh();
    let mut mesh = mesh_original.clone();
    mirror::mirror(&mut mesh, 1);
    assert_eq!(mesh.faces.len(), mesh_original.faces.len());
}

#[test]
fn mirror_then_to_arrays_produces_valid_indices() {
    // Create an extruded square mesh (8 verts, 8 faces: 4 cap tris + 4 side quads)
    let points = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
    let mut mesh = extrude::extrude(&points, PlaneKind::Front, 2.0, 1).unwrap();
    let face_count_before = mesh.faces.len();

    // Mirror on X axis
    mirror::mirror(&mut mesh, 0);

    // Face count should be preserved
    assert_eq!(
        mesh.faces.len(),
        face_count_before,
        "mirror should preserve face count"
    );

    // Export to arrays
    let (positions, _normals, indices) = mesh.to_arrays();
    let vertex_count = positions.len() / 3;

    // Indices must be non-empty
    assert!(
        !indices.is_empty(),
        "to_arrays() produced empty indices after mirror"
    );

    // to_arrays() fan-triangulates quads, so index count >= face_count * 3
    assert!(
        indices.len() >= face_count_before * 3,
        "index count ({}) should be at least face_count * 3 ({})",
        indices.len(),
        face_count_before * 3,
    );

    // All indices must be in-bounds
    for (i, &idx) in indices.iter().enumerate() {
        assert!(
            (idx as usize) < vertex_count,
            "index[{i}] = {idx} is out of bounds (vertex_count = {vertex_count})"
        );
    }

    // No degenerate triangles (all three indices distinct within each triangle)
    for tri in indices.chunks(3) {
        assert!(
            tri[0] != tri[1] && tri[1] != tri[2] && tri[2] != tri[0],
            "degenerate triangle found: [{}, {}, {}]",
            tri[0],
            tri[1],
            tri[2],
        );
    }

    // Verify each face is still traversable (face_vertices returns 3 or 4)
    for f in 0..mesh.faces.len() {
        let verts = mesh.face_vertices(f);
        assert!(
            verts.len() >= 3,
            "face {f} has {} vertices after mirror (expected >= 3); half-edge cycle is broken",
            verts.len(),
        );
    }
}

#[test]
fn mirror_revolved_mesh_produces_valid_indices() {
    // Create a revolved mesh (wheel-like shape) — this is the typical mirror target
    let points = [[0.5, 0.0], [1.0, 0.0], [1.0, 1.0], [0.5, 1.0]];
    let mut mesh = revolve::revolve(&points, PlaneKind::Front, 1, 360.0, 8, false).unwrap();
    let face_count_before = mesh.faces.len();
    let vertex_count_before = mesh.vertices.len();

    // Mirror on X axis (typical for left wheel -> right wheel)
    mirror::mirror(&mut mesh, 0);

    assert_eq!(mesh.faces.len(), face_count_before);
    assert_eq!(mesh.vertices.len(), vertex_count_before);

    // Export to arrays — this is what gets pushed to Godot
    let (positions, _normals, indices) = mesh.to_arrays();
    let vertex_count = positions.len() / 3;

    assert!(
        !indices.is_empty(),
        "to_arrays() produced empty indices after mirroring revolved mesh"
    );

    // to_arrays() fan-triangulates quads, so index count >= face_count * 3
    assert!(
        indices.len() >= face_count_before * 3,
        "index count ({}) should be at least face_count * 3 ({})",
        indices.len(),
        face_count_before * 3,
    );

    for (i, &idx) in indices.iter().enumerate() {
        assert!(
            (idx as usize) < vertex_count,
            "index[{i}] = {idx} out of bounds (vertex_count = {vertex_count})"
        );
    }

    for tri in indices.chunks(3) {
        assert!(
            tri[0] != tri[1] && tri[1] != tri[2] && tri[2] != tri[0],
            "degenerate triangle after mirror: [{}, {}, {}]",
            tri[0],
            tri[1],
            tri[2],
        );
    }

    // Verify every face is still traversable
    for f in 0..mesh.faces.len() {
        let verts = mesh.face_vertices(f);
        assert!(
            verts.len() >= 3,
            "face {f} has {} vertices after mirror (expected >= 3)",
            verts.len(),
        );
    }
}

// ── Revolve ────────────────────────────────────────────────────────

use super::revolve;

#[test]
fn revolve_full_revolution() {
    // Semicircle profile revolved 360 around Y axis
    let points = [[1.0, 0.0], [1.0, 1.0], [0.5, 1.5], [0.0, 1.0]];
    let mesh = revolve::revolve(&points, PlaneKind::Front, 1, 360.0, 8, false);
    assert!(mesh.is_some());
    let mesh = mesh.unwrap();
    // 4 profile points × 8 rings (full revolution = n_segs rings)
    assert_eq!(mesh.vertices.len(), 32);
    assert!(!mesh.faces.is_empty(), "revolve should produce faces");
}

#[test]
fn revolve_partial_with_caps() {
    let points = [[0.5, 0.0], [1.0, 0.0], [1.0, 1.0], [0.5, 1.0]];
    let mesh = revolve::revolve(&points, PlaneKind::Front, 1, 180.0, 4, true);
    assert!(mesh.is_some());
    let mesh = mesh.unwrap();
    // n_rings = n_segs + 1 = 5 for partial; 4 points × 5 = 20 vertices
    assert_eq!(mesh.vertices.len(), 20);
    assert!(!mesh.faces.is_empty());
}

#[test]
fn revolve_too_few_points() {
    let points = [[1.0, 0.0]];
    assert!(revolve::revolve(&points, PlaneKind::Front, 1, 360.0, 8, false).is_none());
}

// ── Bevel ──────────────────────────────────────────────────────────

use super::bevel;

#[test]
fn bevel_cube_produces_more_faces() {
    let mesh = cube_mesh();
    let beveled = bevel::bevel(&mesh, 0.1, 2, "all");
    // A cube has 12 sharp edges and 8 vertices.
    // Bevel adds strip faces (2 per edge per segment) + vertex caps.
    // segments=2: 12 edges × 4 strip tris + 8 vertex caps + 12 original = 68+
    assert!(
        beveled.faces.len() > mesh.faces.len(),
        "beveled mesh ({} faces) should have more faces than original ({} faces)",
        beveled.faces.len(),
        mesh.faces.len(),
    );
}

#[test]
fn bevel_cube_segments_1_geometry() {
    let mesh = cube_mesh();
    let original_faces = mesh.faces.len(); // 12
    let beveled = bevel::bevel(&mesh, 0.1, 1, "all");
    // segments=1: 12 original + 12 edges × 2 strip + 8 vertex caps = 44
    assert!(
        beveled.faces.len() > original_faces,
        "beveled ({}) should exceed original ({original_faces})",
        beveled.faces.len(),
    );
    // Verify mesh is valid (no degenerate faces)
    let (_, _, idx) = beveled.to_arrays();
    assert!(idx.len() >= beveled.faces.len() * 3);
}

#[test]
fn bevel_depth_only_filters_edges() {
    let mesh = cube_mesh();
    let all_beveled = bevel::bevel(&mesh, 0.1, 1, "all");
    let depth_beveled = bevel::bevel(&mesh, 0.1, 1, "depth");
    // Depth-only bevel should produce fewer strip faces than all-edges
    assert!(
        depth_beveled.faces.len() <= all_beveled.faces.len(),
        "depth ({}) should have <= all ({})",
        depth_beveled.faces.len(),
        all_beveled.faces.len(),
    );
}

#[test]
fn bevel_zero_radius_returns_clone() {
    let mesh = cube_mesh();
    let beveled = bevel::bevel(&mesh, 0.0, 2, "all");
    assert_eq!(beveled.faces.len(), mesh.faces.len());
}

#[test]
fn bevel_empty_mesh() {
    let mesh = HalfEdgeMesh::default();
    let beveled = bevel::bevel(&mesh, 0.1, 2, "all");
    assert_eq!(beveled.faces.len(), 0);
}

// ── Subdivide ──────────────────────────────────────────────────────

use super::subdivide;

#[test]
fn subdivide_single_triangle_once() {
    let mesh = single_triangle();
    let result = subdivide::subdivide(&mesh, 1);
    // Each triangle splits into 4
    assert_eq!(result.faces.len(), 4);
}

#[test]
fn subdivide_cube_once() {
    let mesh = cube_mesh();
    let result = subdivide::subdivide(&mesh, 1);
    // 12 triangles × 4 = 48
    assert_eq!(result.faces.len(), 48);
}

#[test]
fn subdivide_two_iterations() {
    let mesh = single_triangle();
    let result = subdivide::subdivide(&mesh, 2);
    // 1 → 4 → 16
    assert_eq!(result.faces.len(), 16);
}

#[test]
fn subdivide_zero_iterations_returns_clone() {
    let mesh = cube_mesh();
    let result = subdivide::subdivide(&mesh, 0);
    assert_eq!(result.faces.len(), mesh.faces.len());
    assert_eq!(result.vertices.len(), mesh.vertices.len());
}

// ── Loop cut ───────────────────────────────────────────────────────

use super::loop_cut;

#[test]
fn loop_cut_cube_at_midpoint() {
    let mesh = cube_mesh();
    let (result, splits) = loop_cut::loop_cut(&mesh, 0, 0.0); // Cut at X=0
    assert!(splits > 0, "should split some triangles");
    assert!(
        result.faces.len() > mesh.faces.len(),
        "cut mesh should have more faces"
    );
}

#[test]
fn loop_cut_no_intersection() {
    let mesh = cube_mesh();
    // Cut at X=5.0 — outside the cube
    let (result, splits) = loop_cut::loop_cut(&mesh, 0, 5.0);
    assert_eq!(splits, 0, "no triangles should be cut outside mesh");
    assert_eq!(result.faces.len(), mesh.faces.len());
}

#[test]
fn loop_cut_empty_mesh() {
    let mesh = HalfEdgeMesh::default();
    let (result, splits) = loop_cut::loop_cut(&mesh, 0, 0.0);
    assert_eq!(splits, 0);
    assert_eq!(result.faces.len(), 0);
}

// ── Array ──────────────────────────────────────────────────────────

use super::array;

#[test]
fn array_creates_copies() {
    let mesh = cube_mesh();
    let result = array::array(&mesh, 3, [2.0, 0.0, 0.0]);
    assert_eq!(result.vertices.len(), mesh.vertices.len() * 3);
    assert_eq!(result.faces.len(), mesh.faces.len() * 3);
}

#[test]
fn array_count_one_returns_clone() {
    let mesh = cube_mesh();
    let result = array::array(&mesh, 1, [2.0, 0.0, 0.0]);
    assert_eq!(result.vertices.len(), mesh.vertices.len());
    assert_eq!(result.faces.len(), mesh.faces.len());
}

#[test]
fn array_offsets_correct() {
    let mesh = single_triangle();
    let result = array::array(&mesh, 2, [5.0, 0.0, 0.0]);
    // Second copy should be offset by 5.0 in X
    let n = mesh.vertices.len();
    let first_x = result.vertices[0].position[0];
    let second_x = result.vertices[n].position[0];
    assert!((second_x - first_x - 5.0).abs() < 1e-9);
}

// ── Loft ───────────────────────────────────────────────────────────

use super::loft;

#[test]
fn loft_two_sections() {
    let section0: Vec<[f64; 3]> = vec![
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [1.0, 1.0, 0.0],
        [0.0, 1.0, 0.0],
    ];
    let section1: Vec<[f64; 3]> = vec![
        [0.0, 0.0, 2.0],
        [1.0, 0.0, 2.0],
        [1.0, 1.0, 2.0],
        [0.0, 1.0, 2.0],
    ];
    let mesh = loft::loft(&[section0, section1], false, false);
    assert!(mesh.is_some());
    let mesh = mesh.unwrap();
    assert_eq!(mesh.vertices.len(), 8);
    // 4 quads = 4 faces
    assert_eq!(mesh.faces.len(), 4);
}

#[test]
fn loft_with_caps() {
    let section0: Vec<[f64; 3]> = vec![
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [1.0, 1.0, 0.0],
        [0.0, 1.0, 0.0],
    ];
    let section1: Vec<[f64; 3]> = vec![
        [0.0, 0.0, 2.0],
        [1.0, 0.0, 2.0],
        [1.0, 1.0, 2.0],
        [0.0, 1.0, 2.0],
    ];
    let mesh = loft::loft(&[section0, section1], true, true);
    assert!(mesh.is_some());
    let mesh = mesh.unwrap();
    // 4 side quads + 2 cap tris × 2 = 8 faces
    assert_eq!(mesh.faces.len(), 8);
}

#[test]
fn loft_three_sections() {
    let sections: Vec<Vec<[f64; 3]>> = (0..3)
        .map(|z| {
            vec![
                [0.0, 0.0, z as f64],
                [1.0, 0.0, z as f64],
                [1.0, 1.0, z as f64],
            ]
        })
        .collect();
    let mesh = loft::loft(&sections, false, false);
    assert!(mesh.is_some());
    let mesh = mesh.unwrap();
    // 3 × 3 = 9 vertices
    assert_eq!(mesh.vertices.len(), 9);
    // 2 sections × 3 quads = 6 faces
    assert_eq!(mesh.faces.len(), 6);
}

#[test]
fn loft_mismatched_sections() {
    let section0 = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0]];
    let section1 = vec![[0.0, 0.0, 2.0], [1.0, 0.0, 2.0]]; // Different count
    assert!(loft::loft(&[section0, section1], false, false).is_none());
}

#[test]
fn loft_single_section() {
    let section = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0]];
    assert!(loft::loft(&[section], false, false).is_none());
}

// ── Boolean subtract ────────────────────────────────────────────────

use super::boolean;

#[test]
fn subtract_overlapping_cubes() {
    let target = cube_mesh();
    let tool = cube_mesh();
    // Offset tool by 0.3 in X to avoid exact boundary coincidence
    let result = boolean::subtract(&target, &tool, [0.3, 0.0, 0.0]);
    // Should produce faces (not empty)
    assert!(!result.faces.is_empty(), "result should not be empty");
    // Face count should differ from original (material removed + cap added)
    assert_ne!(
        result.faces.len(),
        target.faces.len(),
        "faces should change after subtract"
    );
    // All faces should be proper polygons with distinct vertices
    for f in 0..result.faces.len() {
        let verts = result.face_vertices(f);
        assert!(verts.len() >= 3, "all faces should have >= 3 vertices");
    }
}

#[test]
fn subtract_no_overlap() {
    let target = cube_mesh();
    let tool = cube_mesh();
    // Offset tool far away — no overlap
    let result = boolean::subtract(&target, &tool, [10.0, 0.0, 0.0]);
    // Should produce same geometry (6 merged quad faces from the 12-triangle input)
    assert!(!result.faces.is_empty(), "result should not be empty");
    assert!(
        result.faces.len() <= target.faces.len(),
        "no-overlap subtract should not add faces"
    );
}

#[test]
fn subtract_fully_contained_tool() {
    let target = cube_mesh(); // -0.5 to 0.5
    // Small tool cube entirely inside target
    #[rustfmt::skip]
    let small_positions = [
        [-0.25, -0.25, -0.25], [ 0.25, -0.25, -0.25],
        [ 0.25,  0.25, -0.25], [-0.25,  0.25, -0.25],
        [-0.25, -0.25,  0.25], [ 0.25, -0.25,  0.25],
        [ 0.25,  0.25,  0.25], [-0.25,  0.25,  0.25],
    ];
    #[rustfmt::skip]
    let small_indices = [
        0, 1, 2,  0, 2, 3,
        5, 4, 7,  5, 7, 6,
        3, 2, 6,  3, 6, 7,
        4, 5, 1,  4, 1, 0,
        1, 5, 6,  1, 6, 2,
        4, 0, 3,  4, 3, 7,
    ];
    let tool = HalfEdgeMesh::from_triangles(&small_positions, &small_indices);
    let result = boolean::subtract(&target, &tool, [0.0, 0.0, 0.0]);
    // Target faces kept + tool cap faces added (tool is entirely inside)
    // Coplanar merge means target's 12 triangles → 6 quads, tool's 12 → 6 quads
    // Result should have target outer faces + tool inner faces = 12 total
    assert!(
        result.faces.len() > 6,
        "hollow result should have more than 6 faces (got {})",
        result.faces.len(),
    );
}

#[test]
fn subtract_empty_tool() {
    let target = cube_mesh();
    let tool = HalfEdgeMesh::default();
    let result = boolean::subtract(&target, &tool, [0.0, 0.0, 0.0]);
    assert_eq!(result.faces.len(), target.faces.len());
    assert_eq!(result.vertices.len(), target.vertices.len());
}

#[test]
fn subtract_empty_target() {
    let target = HalfEdgeMesh::default();
    let tool = cube_mesh();
    let result = boolean::subtract(&target, &tool, [0.0, 0.0, 0.0]);
    assert_eq!(result.faces.len(), 0);
}

#[test]
fn subtract_welded_vertices() {
    let target = cube_mesh();
    let tool = cube_mesh();
    let result = boolean::subtract(&target, &tool, [0.3, 0.0, 0.0]);
    // Verify no duplicate vertex positions (welding should merge coincident points)
    let unique: std::collections::HashSet<String> = result
        .vertices
        .iter()
        .map(|v| {
            format!(
                "{:.6},{:.6},{:.6}",
                v.position[0], v.position[1], v.position[2]
            )
        })
        .collect();
    assert_eq!(
        unique.len(),
        result.vertices.len(),
        "all vertices should be unique (welded)"
    );
}

#[test]
fn subtract_tool_inside_single_face() {
    // Regression test: tool entirely within a large target face.
    // Old algorithm failed because no target vertices were inside the tool.
    // New algorithm splits the target face at the intersection boundary.

    // Large flat quad (two triangles) as target
    let big_positions = [
        [-5.0, 0.0, -5.0],
        [5.0, 0.0, -5.0],
        [5.0, 4.0, -5.0],
        [-5.0, 4.0, -5.0],
        [-5.0, 0.0, -4.8],
        [5.0, 0.0, -4.8],
        [5.0, 4.0, -4.8],
        [-5.0, 4.0, -4.8],
    ];
    #[rustfmt::skip]
    let big_indices = [
        // Front
        0, 1, 2,  0, 2, 3,
        // Back
        5, 4, 7,  5, 7, 6,
        // Top
        3, 2, 6,  3, 6, 7,
        // Bottom
        4, 5, 1,  4, 1, 0,
        // Right
        1, 5, 6,  1, 6, 2,
        // Left
        4, 0, 3,  4, 3, 7,
    ];
    let target = HalfEdgeMesh::from_triangles(&big_positions, &big_indices);

    // Small door-cut tool that sits in the middle of the front face
    #[rustfmt::skip]
    let door_pos = [
        [-0.5, 0.0, -5.3], [ 0.5, 0.0, -5.3],
        [ 0.5, 2.2, -5.3], [-0.5, 2.2, -5.3],
        [-0.5, 0.0, -4.5], [ 0.5, 0.0, -4.5],
        [ 0.5, 2.2, -4.5], [-0.5, 2.2, -4.5],
    ];
    #[rustfmt::skip]
    let door_indices = [
        0, 1, 2,  0, 2, 3,
        5, 4, 7,  5, 7, 6,
        3, 2, 6,  3, 6, 7,
        4, 5, 1,  4, 1, 0,
        1, 5, 6,  1, 6, 2,
        4, 0, 3,  4, 3, 7,
    ];
    let tool = HalfEdgeMesh::from_triangles(&door_pos, &door_indices);

    let result = boolean::subtract(&target, &tool, [0.0, 0.0, 0.0]);

    // The target had 12 faces. After boolean subtract, the front face should be
    // split and the door region removed, so face count must change.
    assert!(
        result.faces.len() > target.faces.len(),
        "subtract should split target faces (got {} vs original {})",
        result.faces.len(),
        target.faces.len()
    );
    // Tool's interior faces (inside the wall) should be added as cavity walls
    assert!(!result.faces.is_empty());
}

#[test]
fn subtract_watertight() {
    use super::spatial;

    let target = cube_mesh();
    let tool = cube_mesh();
    let result = boolean::subtract(&target, &tool, [0.3, 0.0, 0.0]);

    assert!(!result.faces.is_empty(), "result should not be empty");
    let boundary = spatial::count_non_manifold_edges(&result);
    assert_eq!(
        boundary, 0,
        "subtract result should be watertight (got {boundary} boundary edges)"
    );
}

#[test]
fn subtract_preserves_quads() {
    // Build a quad cube via from_polygons so input has 4-vertex faces
    #[rustfmt::skip]
    let positions: Vec<[f64; 3]> = vec![
        [-0.5, -0.5, -0.5], [ 0.5, -0.5, -0.5],
        [ 0.5,  0.5, -0.5], [-0.5,  0.5, -0.5],
        [-0.5, -0.5,  0.5], [ 0.5, -0.5,  0.5],
        [ 0.5,  0.5,  0.5], [-0.5,  0.5,  0.5],
    ];
    let faces: Vec<&[usize]> = vec![
        &[0, 1, 2, 3], // front
        &[5, 4, 7, 6], // back
        &[3, 2, 6, 7], // top
        &[4, 5, 1, 0], // bottom
        &[1, 5, 6, 2], // right
        &[4, 0, 3, 7], // left
    ];
    let target = HalfEdgeMesh::from_polygons(&positions, &faces);

    // Small tool that only intersects a few faces
    #[rustfmt::skip]
    let tool_pos: Vec<[f64; 3]> = vec![
        [-0.1, -0.1, -0.6], [ 0.1, -0.1, -0.6],
        [ 0.1,  0.1, -0.6], [-0.1,  0.1, -0.6],
        [-0.1, -0.1,  0.6], [ 0.1, -0.1,  0.6],
        [ 0.1,  0.1,  0.6], [-0.1,  0.1,  0.6],
    ];
    let tool_faces: Vec<&[usize]> = vec![
        &[0, 1, 2, 3],
        &[5, 4, 7, 6],
        &[3, 2, 6, 7],
        &[4, 5, 1, 0],
        &[1, 5, 6, 2],
        &[4, 0, 3, 7],
    ];
    let tool = HalfEdgeMesh::from_polygons(&tool_pos, &tool_faces);

    let result = boolean::boolean_op(
        &target,
        &tool,
        [0.0, 0.0, 0.0],
        boolean::BooleanMode::Subtract,
    );
    assert!(!result.faces.is_empty());

    // Some faces should be quads (4-vertex) — the ones that weren't cut
    let quad_count = (0..result.faces.len())
        .filter(|&f| result.face_vertices(f).len() == 4)
        .count();
    assert!(
        quad_count > 0,
        "some unsplit faces should remain as quads, but all faces are non-quad"
    );
}

#[test]
fn union_watertight() {
    use super::spatial;

    let target = cube_mesh();
    let tool = cube_mesh();
    let result = boolean::boolean_op(
        &target,
        &tool,
        [0.3, 0.0, 0.0],
        boolean::BooleanMode::Union,
    );

    assert!(!result.faces.is_empty(), "union result should not be empty");
    let boundary = spatial::count_non_manifold_edges(&result);
    assert_eq!(
        boundary, 0,
        "union result should be watertight (got {boundary} boundary edges)"
    );
}

/// Reproduce the Godot live-test scenario: fuselage (2×0.8×0.8) with a
/// door cutter (0.4×0.5×0.3) offset to poke through the +Z side wall.
#[test]
fn subtract_door_watertight() {
    use super::spatial;

    // Build fuselage: cube scaled to 2×0.8×0.8
    let target = cube_mesh();
    let mut fuselage = target.clone();
    for v in &mut fuselage.vertices {
        v.position[0] *= 2.0;
        v.position[1] *= 0.8;
        v.position[2] *= 0.8;
    }

    // Build door cutter: cube scaled to 0.4×0.5×0.3
    let mut cutter = target;
    for v in &mut cutter.vertices {
        v.position[0] *= 0.4;
        v.position[1] *= 0.5;
        v.position[2] *= 0.3;
    }

    let result = boolean::boolean_op(
        &fuselage,
        &cutter,
        [0.2, 0.05, 0.3],
        boolean::BooleanMode::Subtract,
    );

    assert!(!result.faces.is_empty(), "result should not be empty");

    let boundary = spatial::count_non_manifold_edges(&result);
    assert_eq!(
        boundary, 0,
        "door subtract should be watertight (got {boundary} boundary edges)"
    );
}

// ── Circle profile ──────────────────────────────────────────────────

#[test]
fn circle_profile_triangulates() {
    use std::f64::consts::TAU;
    let segments = 16u32;
    let radius = 1.0;
    let points: Vec<[f64; 2]> = (0..segments)
        .map(|i| {
            let angle = TAU * f64::from(i) / f64::from(segments);
            [radius * angle.cos(), radius * angle.sin()]
        })
        .collect();

    let mesh = profile::triangulate_profile(&points, PlaneKind::Front);
    assert!(mesh.is_some(), "circle should triangulate");
    let mesh = mesh.unwrap();
    // earcut on 16-gon should produce 14 triangles (n-2 for convex polygon)
    assert_eq!(mesh.faces.len(), 14, "16-gon should have 14 triangles");
    assert_eq!(mesh.vertices.len(), 16);
}

#[test]
fn circle_profile_extrudes_with_correct_normals() {
    use std::f64::consts::TAU;
    let segments = 12u32;
    let radius = 0.5;
    let points: Vec<[f64; 2]> = (0..segments)
        .map(|i| {
            let angle = TAU * f64::from(i) / f64::from(segments);
            [radius * angle.cos(), radius * angle.sin()]
        })
        .collect();

    let mesh = extrude::extrude(&points, PlaneKind::Front, 2.0, 1).unwrap();
    assert!(!mesh.faces.is_empty());
    assert_all_normals_outward(&mesh, "circle extrude front");
}

#[test]
fn circle_profile_extrudes_side_plane() {
    use std::f64::consts::TAU;
    let segments = 8u32;
    let radius = 0.3;
    let points: Vec<[f64; 2]> = (0..segments)
        .map(|i| {
            let angle = TAU * f64::from(i) / f64::from(segments);
            [radius * angle.cos(), radius * angle.sin()]
        })
        .collect();

    let mesh = extrude::extrude(&points, PlaneKind::Side, 1.0, 1).unwrap();
    assert!(!mesh.faces.is_empty());
    assert_all_normals_outward(&mesh, "circle extrude side");
}

#[test]
fn ellipse_profile_triangulates() {
    use std::f64::consts::TAU;
    let segments = 12u32;
    let rx = 2.0;
    let ry = 0.5;
    let points: Vec<[f64; 2]> = (0..segments)
        .map(|i| {
            let angle = TAU * f64::from(i) / f64::from(segments);
            [rx * angle.cos(), ry * angle.sin()]
        })
        .collect();

    let mesh = profile::triangulate_profile(&points, PlaneKind::Top);
    assert!(mesh.is_some());
    let mesh = mesh.unwrap();
    assert_eq!(mesh.faces.len(), 10); // 12-gon → 10 triangles
    assert_eq!(mesh.vertices.len(), 12);
}

#[test]
fn cap_inset_adds_quad_ring() {
    use std::f64::consts::TAU;
    let segments = 16u32;
    let radius = 1.0;
    let points: Vec<[f64; 2]> = (0..segments)
        .map(|i| {
            let angle = TAU * f64::from(i) / f64::from(segments);
            [radius * angle.cos(), radius * angle.sin()]
        })
        .collect();

    let mesh_no_inset = extrude::extrude(&points, PlaneKind::Front, 2.0, 1).unwrap();
    let mesh_inset = extrude::extrude_with_inset(&points, PlaneKind::Front, 2.0, 1, 0.15).unwrap();

    // Multi-ring inset: 16 segments → 2 auto-rings per cap.
    // Each cap adds (2 intermediate + 1 inner) × 16 = 48 verts; × 2 caps = 96.
    assert_eq!(
        mesh_inset.vertices.len(),
        mesh_no_inset.vertices.len() + 96,
        "inset should add 96 vertices (48 per cap with multi-ring)"
    );

    // Inset replaces fan caps with quad-ring + inner-fan, producing more faces
    assert!(
        mesh_inset.faces.len() > mesh_no_inset.faces.len(),
        "inset ({}) should have more faces than no-inset ({})",
        mesh_inset.faces.len(),
        mesh_no_inset.faces.len(),
    );

    // All normals should still point outward
    assert_all_normals_outward(&mesh_inset, "circle extrude with cap inset");
}

#[test]
fn cap_inset_side_plane_normals_correct() {
    use std::f64::consts::TAU;
    let segments = 12u32;
    let radius = 0.5;
    let points: Vec<[f64; 2]> = (0..segments)
        .map(|i| {
            let angle = TAU * f64::from(i) / f64::from(segments);
            [radius * angle.cos(), radius * angle.sin()]
        })
        .collect();

    let mesh = extrude::extrude_with_inset(&points, PlaneKind::Side, 2.0, 1, 0.15).unwrap();
    assert_all_normals_outward(&mesh, "inset cap side plane");
}

#[test]
fn cap_inset_skipped_for_small_profiles() {
    // Rectangle (4 points) — inset should be skipped (< 5 points)
    let points = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
    let mesh_no_inset = extrude::extrude(&points, PlaneKind::Front, 2.0, 1).unwrap();
    let mesh_with_flag =
        extrude::extrude_with_inset(&points, PlaneKind::Front, 2.0, 1, 0.15).unwrap();

    // Same topology — inset was skipped because n_pts < 5
    assert_eq!(mesh_no_inset.vertices.len(), mesh_with_flag.vertices.len());
    assert_eq!(mesh_no_inset.faces.len(), mesh_with_flag.faces.len());
}

// ── Inset (standalone) ──────────────────────────────────────────────

use super::inset;

#[test]
fn inset_cube_adds_faces() {
    let mesh = cube_mesh();
    let result = inset::inset(&mesh, 0.3);
    // Each face splits into inner + quad strip → more faces
    assert!(
        result.faces.len() > mesh.faces.len(),
        "inset ({}) should have more faces than original ({})",
        result.faces.len(),
        mesh.faces.len(),
    );
}

#[test]
fn inset_zero_factor_returns_clone() {
    let mesh = cube_mesh();
    let result = inset::inset(&mesh, 0.0);
    assert_eq!(result.faces.len(), mesh.faces.len());
}

#[test]
fn inset_preserves_vertex_bounds() {
    let mesh = cube_mesh();
    let result = inset::inset(&mesh, 0.3);
    let (orig_min, orig_max) = mesh.aabb();
    let (new_min, new_max) = result.aabb();
    // Inset should not expand the mesh bounds
    for i in 0..3 {
        assert!(new_min[i] >= orig_min[i] - 1e-9);
        assert!(new_max[i] <= orig_max[i] + 1e-9);
    }
}

// ── Solidify ─────────────────────────────────────────────────────────

use super::solidify;

#[test]
fn solidify_single_triangle() {
    let mesh = single_triangle();
    let result = solidify::solidify(&mesh, 0.1);
    // Outer shell + inner shell = 2 faces, plus 3 boundary wall quads (6 tris)
    assert!(
        result.faces.len() > mesh.faces.len(),
        "solidified ({}) should have more faces than original ({})",
        result.faces.len(),
        mesh.faces.len(),
    );
    // Should have doubled vertices (outer + inner)
    assert_eq!(result.vertices.len(), mesh.vertices.len() * 2);
}

#[test]
fn solidify_cube_doubles_faces() {
    let mesh = cube_mesh();
    let result = solidify::solidify(&mesh, 0.05);
    // Cube is watertight (no boundary edges) → outer + inner shells only
    // 12 original + 12 inner = 24 faces
    assert_eq!(result.faces.len(), mesh.faces.len() * 2);
}

#[test]
fn solidify_zero_thickness_returns_clone() {
    let mesh = cube_mesh();
    let result = solidify::solidify(&mesh, 0.0);
    assert_eq!(result.faces.len(), mesh.faces.len());
}

// ── Merge by distance ────────────────────────────────────────────────

use super::merge;

#[test]
fn merge_no_duplicates_unchanged() {
    let mesh = cube_mesh();
    let (result, merged) = merge::merge_by_distance(&mesh, 0.001);
    assert_eq!(merged, 0);
    assert_eq!(result.vertices.len(), mesh.vertices.len());
}

#[test]
fn merge_near_vertices() {
    // Create mesh with deliberately near-duplicate vertices
    let positions = [
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [0.5, 1.0, 0.0],
        [0.0001, 0.0, 0.0], // near-duplicate of vertex 0
    ];
    let indices = [0, 1, 2, 3, 1, 2];
    let mesh = HalfEdgeMesh::from_triangles(&positions, &indices);
    let (result, merged) = merge::merge_by_distance(&mesh, 0.001);
    assert_eq!(merged, 1, "should merge one near-duplicate vertex");
    assert_eq!(result.vertices.len(), 3);
}

#[test]
fn merge_zero_distance_unchanged() {
    let mesh = cube_mesh();
    let (result, merged) = merge::merge_by_distance(&mesh, 0.0);
    assert_eq!(merged, 0);
    assert_eq!(result.faces.len(), mesh.faces.len());
}

// ── Boolean union/intersect ──────────────────────────────────────────

#[test]
fn boolean_union_combines_meshes() {
    let target = cube_mesh();
    let tool = cube_mesh();
    // Offset far away → union should have faces from both
    let result = boolean::boolean_op(&target, &tool, [5.0, 0.0, 0.0], boolean::BooleanMode::Union);
    // Coplanar merge turns each cube's 12 triangles into 6 quads → 12 total
    assert_eq!(
        result.faces.len(),
        12,
        "union of non-overlapping cubes should have 6+6 merged faces"
    );
}

#[test]
fn boolean_union_overlapping_produces_valid_mesh() {
    let target = cube_mesh();
    let tool = cube_mesh();
    let result = boolean::boolean_op(&target, &tool, [0.3, 0.0, 0.0], boolean::BooleanMode::Union);
    // Overlapping union: face count may increase due to splitting at the
    // intersection boundary, but should produce a non-empty valid mesh.
    assert!(!result.faces.is_empty());
    for f in 0..result.faces.len() {
        let verts = result.face_vertices(f);
        assert!(verts.len() >= 3, "all faces should have >= 3 vertices");
    }
}

#[test]
fn boolean_intersect_overlapping() {
    let target = cube_mesh();
    let tool = cube_mesh();
    let result = boolean::boolean_op(
        &target,
        &tool,
        [0.3, 0.0, 0.0],
        boolean::BooleanMode::Intersect,
    );
    // Intersection of overlapping cubes should produce some faces
    assert!(!result.faces.is_empty(), "intersection should not be empty");
    for f in 0..result.faces.len() {
        let verts = result.face_vertices(f);
        assert!(verts.len() >= 3, "all faces should have >= 3 vertices");
    }
}

#[test]
fn boolean_intersect_no_overlap_empty() {
    let target = cube_mesh();
    let tool = cube_mesh();
    let result = boolean::boolean_op(
        &target,
        &tool,
        [10.0, 0.0, 0.0],
        boolean::BooleanMode::Intersect,
    );
    assert_eq!(
        result.faces.len(),
        0,
        "no-overlap intersect should be empty"
    );
}

// ── Bevel profile ────────────────────────────────────────────────────

#[test]
fn bevel_profile_concave_differs_from_convex() {
    let mesh = cube_mesh();
    let concave = bevel::bevel_with_profile(&mesh, 0.1, 3, "all", 0.0, None);
    let convex = bevel::bevel_with_profile(&mesh, 0.1, 3, "all", 1.0, None);
    // Both should produce valid meshes with same topology
    assert_eq!(concave.faces.len(), convex.faces.len());
    // But different vertex positions (at least some differ)
    let mut any_different = false;
    for (a, b) in concave.vertices.iter().zip(convex.vertices.iter()) {
        let d = (a.position[0] - b.position[0]).abs()
            + (a.position[1] - b.position[1]).abs()
            + (a.position[2] - b.position[2]).abs();
        if d > 1e-9 {
            any_different = true;
            break;
        }
    }
    assert!(
        any_different,
        "concave and convex bevels should differ in geometry"
    );
}

#[test]
fn bevel_profile_default_matches_original() {
    let mesh = cube_mesh();
    let original = bevel::bevel(&mesh, 0.1, 2, "all");
    let with_profile = bevel::bevel_with_profile(&mesh, 0.1, 2, "all", 0.5, None);
    assert_eq!(original.faces.len(), with_profile.faces.len());
    assert_eq!(original.vertices.len(), with_profile.vertices.len());
}

// ── fix_winding: all-inverted detection ────────────────────────────

#[test]
fn fix_winding_detects_all_inverted_cube() {
    // 1. Start with a cube and ensure it's correctly oriented
    let mut mesh = cube_mesh();
    normals::fix_winding(&mut mesh);

    // Record outward-normal dot sum as baseline
    let baseline_dot_sum = outward_dot_sum(&mesh);
    assert!(
        baseline_dot_sum > 0.0,
        "After initial fix_winding, normals should point outward (dot_sum={baseline_dot_sum})"
    );

    // 2. Flip ALL normals — now everything is inverted
    normals::flip_all(&mut mesh);
    let flipped_dot_sum = outward_dot_sum(&mesh);
    assert!(
        flipped_dot_sum < 0.0,
        "After flip_all, normals should point inward (dot_sum={flipped_dot_sum})"
    );

    // 3. fix_winding should detect the all-inverted state and fix it
    let flipped = normals::fix_winding(&mut mesh);
    assert!(
        flipped > 0,
        "fix_winding should detect all-inverted normals and flip faces"
    );

    // 4. After fixing, normals should point outward again
    let recovered_dot_sum = outward_dot_sum(&mesh);
    assert!(
        recovered_dot_sum > 0.0,
        "After fix_winding recovery, normals should point outward (dot_sum={recovered_dot_sum})"
    );
}

/// Sum of dot(face_normal, face_center - mesh_center) across all faces.
/// Positive means majority outward, negative means majority inward.
fn outward_dot_sum(mesh: &HalfEdgeMesh) -> f64 {
    let n = mesh.vertices.len() as f64;
    let mut mc = [0.0; 3];
    for v in &mesh.vertices {
        mc[0] += v.position[0];
        mc[1] += v.position[1];
        mc[2] += v.position[2];
    }
    mc = [mc[0] / n, mc[1] / n, mc[2] / n];

    let mut sum = 0.0;
    for f in 0..mesh.faces.len() {
        let fn_ = normals::compute_face_normal(mesh, f);
        let verts = mesh.face_vertices(f);
        let fc = face_center(mesh, &verts);
        let out = [fc[0] - mc[0], fc[1] - mc[1], fc[2] - mc[2]];
        sum += fn_[0] * out[0] + fn_[1] * out[1] + fn_[2] * out[2];
    }
    sum
}

// ── Quad topology tests ─────────────────────────────────────────────

#[test]
fn extrude_produces_quads() {
    // Side walls of an extruded square should be quads (4 vertices per face)
    let points = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
    let mesh = extrude::extrude(&points, PlaneKind::Front, 2.0, 1).unwrap();
    let mut quad_count = 0;
    let mut tri_count = 0;
    for f in 0..mesh.faces.len() {
        let verts = mesh.face_vertices(f);
        match verts.len() {
            3 => tri_count += 1,
            4 => quad_count += 1,
            n => panic!("unexpected face with {n} vertices"),
        }
    }
    // 4 cap tris (2 per cap) + 4 side quads
    assert_eq!(tri_count, 4, "expected 4 cap triangles");
    assert_eq!(quad_count, 4, "expected 4 side quads");
}

#[test]
fn revolve_produces_quads() {
    // Side faces of a revolved shape should be quads
    let points = [[0.5, 0.0], [1.0, 0.0], [1.0, 1.0], [0.5, 1.0]];
    let mesh = revolve::revolve(&points, PlaneKind::Front, 1, 360.0, 8, false).unwrap();
    let quad_count = (0..mesh.faces.len())
        .filter(|&f| mesh.face_vertices(f).len() == 4)
        .count();
    // 3 edge-pairs per ring × 8 rings = 24 quads
    assert_eq!(quad_count, 24, "revolve side faces should all be quads");
}

#[test]
fn to_arrays_triangulates_quads() {
    // to_arrays() should produce only triangles (GPU-ready)
    let points = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
    let mesh = extrude::extrude(&points, PlaneKind::Front, 2.0, 1).unwrap();
    let (_, _, indices) = mesh.to_arrays();
    // All indices should be in groups of 3
    assert_eq!(indices.len() % 3, 0, "indices should be divisible by 3");
    // Quads become 2 triangles each: 4 tris (caps) + 4 quads × 2 = 12 triangles = 36 indices
    assert_eq!(indices.len(), 36);
}

#[test]
fn from_polygons_basic() {
    // Build a simple quad from from_polygons
    let positions = [
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [1.0, 1.0, 0.0],
        [0.0, 1.0, 0.0],
    ];
    let faces: Vec<&[usize]> = vec![&[0, 1, 2, 3]];
    let mesh = HalfEdgeMesh::from_polygons(&positions, &faces);
    assert_eq!(mesh.faces.len(), 1);
    assert_eq!(mesh.face_vertices(0).len(), 4);
}

#[test]
fn from_polygons_mixed_tri_quad() {
    // Build a mesh with both triangles and quads
    let positions = [
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [1.0, 1.0, 0.0],
        [0.0, 1.0, 0.0],
        [2.0, 0.5, 0.0],
    ];
    let faces: Vec<&[usize]> = vec![&[0, 1, 2, 3], &[1, 4, 2]];
    let mesh = HalfEdgeMesh::from_polygons(&positions, &faces);
    assert_eq!(mesh.faces.len(), 2);
    assert_eq!(mesh.face_vertices(0).len(), 4);
    assert_eq!(mesh.face_vertices(1).len(), 3);
}

#[test]
fn loft_produces_quads() {
    let section0 = vec![
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [1.0, 1.0, 0.0],
        [0.0, 1.0, 0.0],
    ];
    let section1 = vec![
        [0.0, 0.0, 2.0],
        [1.0, 0.0, 2.0],
        [1.0, 1.0, 2.0],
        [0.0, 1.0, 2.0],
    ];
    let mesh = loft::loft(&[section0, section1], false, false).unwrap();
    // All 4 faces should be quads
    for f in 0..mesh.faces.len() {
        assert_eq!(
            mesh.face_vertices(f).len(),
            4,
            "loft face {f} should be a quad"
        );
    }
}

/// Verify that GPU triangle winding from `to_arrays()` matches outward normals.
///
/// For each triangle emitted by `to_arrays()`, computes the geometric normal
/// from the cross product of edges (GPU winding-based) and checks it points
/// outward from the mesh center. This catches bugs where the Rust face normals
/// are correct but the GPU triangle winding produces back-facing triangles.
#[test]
fn gpu_triangle_winding_matches_godot_cw() {
    // Godot uses CW front-face winding: cross product (e1×e2) should point INWARD
    // (toward mesh center) for front-facing triangles.  This matches BoxMesh behavior.
    fn check_gpu_winding(mesh: &HalfEdgeMesh, label: &str) {
        let center = mesh_center(mesh);
        let (positions, _normals, indices) = mesh.to_arrays();

        let mut outward = Vec::new();
        for (t, tri) in indices.chunks(3).enumerate() {
            let v0 = [
                positions[tri[0] as usize * 3],
                positions[tri[0] as usize * 3 + 1],
                positions[tri[0] as usize * 3 + 2],
            ];
            let v1 = [
                positions[tri[1] as usize * 3],
                positions[tri[1] as usize * 3 + 1],
                positions[tri[1] as usize * 3 + 2],
            ];
            let v2 = [
                positions[tri[2] as usize * 3],
                positions[tri[2] as usize * 3 + 1],
                positions[tri[2] as usize * 3 + 2],
            ];

            // Geometric normal from GPU triangle winding
            let e1 = [v1[0] - v0[0], v1[1] - v0[1], v1[2] - v0[2]];
            let e2 = [v2[0] - v0[0], v2[1] - v0[1], v2[2] - v0[2]];
            let geo_n = [
                e1[1] * e2[2] - e1[2] * e2[1],
                e1[2] * e2[0] - e1[0] * e2[2],
                e1[0] * e2[1] - e1[1] * e2[0],
            ];

            // Triangle centroid → outward direction from mesh center
            let tc = [
                (v0[0] + v1[0] + v2[0]) / 3.0,
                (v0[1] + v1[1] + v2[1]) / 3.0,
                (v0[2] + v1[2] + v2[2]) / 3.0,
            ];
            let dir = [tc[0] - center[0], tc[1] - center[1], tc[2] - center[2]];
            let dot = geo_n[0] * dir[0] + geo_n[1] * dir[1] + geo_n[2] * dir[2];

            // CW winding means cross product should point inward (dot < 0)
            if dot > 1e-12 {
                outward.push(t);
            }
        }

        let total = indices.len() / 3;
        assert!(
            outward.is_empty(),
            "{label}: {}/{total} GPU triangles have wrong winding (CCW instead of Godot CW) (tris: {outward:?})",
            outward.len(),
        );
    }

    // Front plane
    let points = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
    let mesh = extrude::extrude(&points, PlaneKind::Front, 2.0, 1).unwrap();
    check_gpu_winding(&mesh, "Front plane extrude");

    // Side plane
    let mesh = extrude::extrude(&points, PlaneKind::Side, 2.0, 1).unwrap();
    check_gpu_winding(&mesh, "Side plane extrude");

    // Top plane
    let mesh = extrude::extrude(&points, PlaneKind::Top, 2.0, 1).unwrap();
    check_gpu_winding(&mesh, "Top plane extrude");

    // Revolve
    let half_profile = [[0.5, 0.0], [0.5, 1.0], [0.0, 1.0]];
    let mesh = revolve::revolve(&half_profile, PlaneKind::Front, 1, 360.0, 8, false).unwrap();
    check_gpu_winding(&mesh, "Revolve front Y-axis");

    // Thin extrusion (similar to agent's gun body)
    let thin_profile = [[0.0, 0.0], [0.25, 0.0], [0.25, 0.055], [0.15, 0.068], [0.0, 0.068]];
    let mesh = extrude::extrude(&thin_profile, PlaneKind::Side, 0.055, 1).unwrap();
    check_gpu_winding(&mesh, "Thin side plane extrude");
}

// ── Bevel after cap-inset ───────────────────────────────────────────

#[test]
fn bevel_works_after_cap_inset_pentagon() {
    // Pentagon (>= 5 pts to trigger inset) with cap-inset, then bevel
    let pentagon = [
        [1.0, 0.0],
        [0.309, 0.951],
        [-0.809, 0.588],
        [-0.809, -0.588],
        [0.309, -0.951],
    ];
    let mesh =
        extrude::extrude_with_inset(&pentagon, PlaneKind::Front, 2.0, 1, 0.15).unwrap();
    let original_faces = mesh.faces.len();

    let beveled = bevel::bevel(&mesh, 0.1, 1, "all");
    assert!(
        beveled.faces.len() > original_faces,
        "bevel after cap-inset should produce more faces: got {} (same as original {original_faces})",
        beveled.faces.len(),
    );
}

#[test]
fn bevel_works_after_cap_inset_circle() {
    // Circle with 16 segments — the exact case from the agent's barrel
    use std::f64::consts::TAU;
    let segments = 16;
    let circle: Vec<[f64; 2]> = (0..segments)
        .map(|i| {
            let angle = TAU * i as f64 / segments as f64;
            [0.5 * angle.cos(), 0.5 * angle.sin()]
        })
        .collect();
    let mesh =
        extrude::extrude_with_inset(&circle, PlaneKind::Front, 2.0, 1, 0.15).unwrap();
    let original_faces = mesh.faces.len();

    let beveled = bevel::bevel(&mesh, 0.1, 1, "all");
    assert!(
        beveled.faces.len() > original_faces,
        "bevel after cap-inset (circle 16-seg) should produce more faces: got {} (same as {original_faces})",
        beveled.faces.len(),
    );
}

// ── Spatial filter ──────────────────────────────────────────────────

use super::spatial_filter;

#[test]
fn spatial_filter_parse() {
    let sf = spatial_filter::parse_where("y>0.12").unwrap();
    assert_eq!(sf.axis, 1);
    assert_eq!(sf.op, std::cmp::Ordering::Greater);
    assert!((sf.value - 0.12).abs() < 1e-9);

    let sf = spatial_filter::parse_where("z<-0.5").unwrap();
    assert_eq!(sf.axis, 2);
    assert_eq!(sf.op, std::cmp::Ordering::Less);
    assert!((sf.value - -0.5).abs() < 1e-9);

    let sf = spatial_filter::parse_where("x>=0").unwrap();
    assert_eq!(sf.axis, 0);
    assert_eq!(sf.op, std::cmp::Ordering::Greater);
    assert!(sf.value.abs() < 1e-9);
}

#[test]
fn spatial_filter_parse_errors() {
    assert!(spatial_filter::parse_where("").is_err());
    assert!(spatial_filter::parse_where("w>1").is_err());
    assert!(spatial_filter::parse_where("y").is_err());
    assert!(spatial_filter::parse_where("y>abc").is_err());
}

// ── Bevel with --where ──────────────────────────────────────────────

#[test]
fn bevel_where_top_only() {
    // Cube from -0.5 to 0.5. Top edges have midpoint y > 0.4.
    let mesh = cube_mesh();
    let sf = spatial_filter::parse_where("y>0.4").unwrap();
    let beveled = bevel::bevel_with_profile(&mesh, 0.1, 1, "all", 0.5, Some(&sf));
    // Should produce fewer bevel faces than all-edges bevel
    let beveled_all = bevel::bevel(&mesh, 0.1, 1, "all");
    assert!(
        beveled.faces.len() > mesh.faces.len(),
        "where-filtered bevel should still add faces"
    );
    assert!(
        beveled.faces.len() < beveled_all.faces.len(),
        "top-only bevel ({}) should have fewer faces than all-edges ({})",
        beveled.faces.len(),
        beveled_all.faces.len(),
    );
}

// ── Inset with --where ──────────────────────────────────────────────

#[test]
fn inset_where_top_only() {
    // Cube: top faces have centroid y > 0.4
    let mesh = cube_mesh();
    let sf = spatial_filter::parse_where("y>0.4").unwrap();
    let selected: Vec<usize> = (0..mesh.faces.len())
        .filter(|&fi| spatial_filter::face_matches(&mesh, fi, &sf))
        .collect();
    assert!(!selected.is_empty(), "should select some top faces");

    let result = inset::inset_selected(&mesh, 0.3, Some(&selected));
    let result_all = inset::inset(&mesh, 0.3);

    assert!(
        result.faces.len() > mesh.faces.len(),
        "where-filtered inset should add faces"
    );
    assert!(
        result.faces.len() < result_all.faces.len(),
        "top-only inset ({}) should have fewer faces than all-face inset ({})",
        result.faces.len(),
        result_all.faces.len(),
    );
}

// ── Extrude-face ────────────────────────────────────────────────────

use super::extrude_face;

#[test]
fn extrude_face_top() {
    // Cube: extrude top faces upward
    let mesh = cube_mesh();
    let sf = spatial_filter::parse_where("y>0.4").unwrap();
    let selected: Vec<usize> = (0..mesh.faces.len())
        .filter(|&fi| spatial_filter::face_matches(&mesh, fi, &sf))
        .collect();
    assert!(!selected.is_empty());

    let result = extrude_face::extrude_faces(&mesh, 0.5, &selected);

    // Should have more faces (side walls added)
    assert!(
        result.faces.len() > mesh.faces.len(),
        "extrude-face should add side wall faces: got {} vs {}",
        result.faces.len(),
        mesh.faces.len(),
    );

    // Extrude duplicates selected-face vertices → more vertices than original.
    assert!(
        result.vertex_count() > mesh.vertex_count(),
        "extrude-face should add offset vertices: got {} vs {}",
        result.vertex_count(),
        mesh.vertex_count(),
    );
}

#[test]
fn extrude_face_no_selection_returns_clone() {
    let mesh = cube_mesh();
    let result = extrude_face::extrude_faces(&mesh, 0.5, &[]);
    assert_eq!(result.faces.len(), mesh.faces.len());
}

// ── Boolean array ───────────────────────────────────────────────────

#[test]
fn boolean_array_subtract() {
    let target = cube_mesh();
    // Small tool cube
    #[rustfmt::skip]
    let small_pos = [
        [-0.1, -0.1, -0.6], [ 0.1, -0.1, -0.6],
        [ 0.1,  0.1, -0.6], [-0.1,  0.1, -0.6],
        [-0.1, -0.1,  0.6], [ 0.1, -0.1,  0.6],
        [ 0.1,  0.1,  0.6], [-0.1,  0.1,  0.6],
    ];
    #[rustfmt::skip]
    let small_idx = [
        0, 1, 2,  0, 2, 3,
        5, 4, 7,  5, 7, 6,
        3, 2, 6,  3, 6, 7,
        4, 5, 1,  4, 1, 0,
        1, 5, 6,  1, 6, 2,
        4, 0, 3,  4, 3, 7,
    ];
    let tool = HalfEdgeMesh::from_triangles(&small_pos, &small_idx);

    // Array subtract: 3 cuts along X axis
    let offset = [0.0, 0.0, 0.0];
    let spacing = [0.25, 0.0, 0.0];
    let mut current = target.clone();
    for k in 0..3_u32 {
        let iter_offset = [
            offset[0] + spacing[0] * k as f64,
            offset[1] + spacing[1] * k as f64,
            offset[2] + spacing[2] * k as f64,
        ];
        current = boolean::boolean_op(
            &current,
            &tool,
            iter_offset,
            boolean::BooleanMode::Subtract,
        );
    }
    // Should have more faces than original (material removed + caps added per cut)
    assert!(
        current.faces.len() > target.faces.len(),
        "3 array subtracts should produce more faces: {} vs {}",
        current.faces.len(),
        target.faces.len(),
    );
}

// ── Multi-contour profiles (holes) ──────────────────────────────────

#[test]
fn profile_with_hole() {
    // Outer square with inner square hole
    let outer = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
    let hole = vec![[0.25, 0.25], [0.75, 0.25], [0.75, 0.75], [0.25, 0.75]];

    let indices = profile::triangulate_2d_with_holes(&outer, &[hole]);
    assert!(indices.is_some(), "should triangulate with hole");
    let indices = indices.unwrap();
    // 8 points total, earcut should produce triangles filling the area minus the hole
    assert_eq!(indices.len() % 3, 0, "indices should be multiple of 3");
    // Square with square hole → 8 triangles (8 total points, outer ring + hole ring)
    assert!(
        indices.len() >= 18,
        "should produce at least 6 triangles: got {}",
        indices.len() / 3,
    );
}

#[test]
fn extrude_with_hole() {
    // Extruded ring: outer square with inner square hole
    let outer = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
    let hole = vec![[0.25, 0.25], [0.75, 0.25], [0.75, 0.75], [0.25, 0.75]];

    let mesh =
        extrude::extrude_with_holes(&outer, &[hole], PlaneKind::Front, 2.0, 1);
    assert!(mesh.is_some(), "should extrude with holes");
    let mesh = mesh.unwrap();

    // Should have outer side walls + inner hole side walls + caps
    // 8 total points × 2 sections = 16 vertices
    assert_eq!(mesh.vertices.len(), 16);

    // Outer walls: 4 quads, inner walls: 4 quads, caps: triangulated with holes
    assert!(
        mesh.faces.len() > 8,
        "extruded ring should have wall + cap faces: got {}",
        mesh.faces.len(),
    );

    // Check that inner walls exist (more faces than a solid extrude)
    let solid = extrude::extrude(&outer, PlaneKind::Front, 2.0, 1).unwrap();
    assert!(
        mesh.faces.len() > solid.faces.len(),
        "ring ({}) should have more faces than solid ({})",
        mesh.faces.len(),
        solid.faces.len(),
    );
}

// ── Multi-ring cap ──────────────────────────────────────────────────

#[test]
fn multi_ring_cap() {
    use std::f64::consts::TAU;
    let segments = 16u32;
    let circle: Vec<[f64; 2]> = (0..segments)
        .map(|i| {
            let angle = TAU * f64::from(i) / f64::from(segments);
            [1.0 * angle.cos(), 1.0 * angle.sin()]
        })
        .collect();

    // With inset: should produce multi-ring quads
    let mesh = extrude::extrude_with_inset(&circle, PlaneKind::Front, 2.0, 1, 0.15).unwrap();

    // Count quads on the cap faces (multi-ring should produce more quads than single ring)
    let quad_count = (0..mesh.faces.len())
        .filter(|&f| mesh.face_vertices(f).len() == 4)
        .count();

    // 16-segment circle with auto rings = max(1, 16/8) = 2 rings
    // Each ring has 16 quads, so 2 rings × 2 caps = at least 64 quad-ring faces
    // Plus the inner earcut + 16 outer side quads
    assert!(
        quad_count >= 48,
        "multi-ring cap should produce many quads: got {quad_count}"
    );

    // All normals should still point outward
    assert_all_normals_outward(&mesh, "multi-ring cap circle 16-seg");
}

#[test]
fn multi_ring_cap_24_segments() {
    use std::f64::consts::TAU;
    let segments = 24u32;
    let circle: Vec<[f64; 2]> = (0..segments)
        .map(|i| {
            let angle = TAU * f64::from(i) / f64::from(segments);
            [1.0 * angle.cos(), 1.0 * angle.sin()]
        })
        .collect();

    let mesh = extrude::extrude_with_inset(&circle, PlaneKind::Front, 2.0, 1, 0.15).unwrap();
    // 24 segments → 3 rings (max(1, 24/8) = 3, capped at 3)
    // 3 rings × 24 quads × 2 caps + 1 inner quad ring × 2 caps = at least many quads
    let quad_count = (0..mesh.faces.len())
        .filter(|&f| mesh.face_vertices(f).len() == 4)
        .count();
    assert!(
        quad_count >= 96,
        "24-seg circle should produce >= 96 quads: got {quad_count}"
    );
    assert_all_normals_outward(&mesh, "multi-ring cap circle 24-seg");
}

// ── Bug fix: Bevel on concave profile ──────────────────────────────

#[test]
fn bevel_concave_profile_no_panic() {
    // 10-point concave L-shape — previously panicked at vertex_faces() bounds check
    let points: Vec<[f64; 2]> = vec![
        [0.0, 0.0],
        [0.1, 0.0],
        [0.1, 0.02],
        [0.03, 0.02],
        [0.03, 0.05],
        [0.1, 0.05],
        [0.1, 0.07],
        [0.0, 0.07],
        [0.0, 0.05],
        [0.02, 0.05],
    ];
    let mesh = extrude::extrude(&points, PlaneKind::Front, 0.05, 1).unwrap();
    // Should not panic, should produce valid output
    let beveled = bevel::bevel(&mesh, 0.005, 1, "all");
    assert!(
        beveled.vertex_count() > mesh.vertex_count(),
        "bevel should add vertices: {} vs {}",
        beveled.vertex_count(),
        mesh.vertex_count()
    );
    assert!(beveled.face_count() > 0, "bevel should produce faces");
}

// ── Bug fix: Boolean with scaled tool ──────────────────────────────

#[test]
fn boolean_subtract_scaled_tool() {
    use super::boolean::{self, BooleanMode};

    // Target: unit cube
    let target = cube_mesh();

    // Tool: small cube at 0.01 scale — simulates a scaled tool part
    let tool_base = cube_mesh();
    // Apply scale transform to tool vertices (simulating transform_mesh)
    let scale = [0.3, 0.3, 0.3];
    let mut tool = tool_base.clone();
    for v in &mut tool.vertices {
        v.position[0] *= scale[0];
        v.position[1] *= scale[1];
        v.position[2] *= scale[2];
    }

    // Boolean subtract the small tool from the target
    let result = boolean::boolean_op(&target, &tool, [0.0, 0.0, 0.0], BooleanMode::Subtract);
    assert!(
        result.face_count() > 0,
        "boolean with scaled tool should not produce empty mesh"
    );
    assert!(
        result.face_count() >= target.face_count(),
        "subtract should produce at least as many faces as target: {} vs {}",
        result.face_count(),
        target.face_count()
    );
}

// ── Bug fix: flip-normals --where ──────────────────────────────────

#[test]
fn flip_normals_where_top_only() {
    let mesh = cube_mesh();
    let fc = mesh.faces.len();

    // Count faces with upward-pointing normals (y > 0.4 centroid)
    let filter = spatial_filter::parse_where("y>0.4").unwrap();
    let top_faces: Vec<usize> = (0..fc)
        .filter(|&f| spatial_filter::face_matches(&mesh, f, &filter))
        .collect();
    assert!(
        !top_faces.is_empty(),
        "cube should have faces with centroid y > 0.4"
    );

    // Flip only top faces
    let mut flipped = mesh.clone();
    let count = normals::flip_where(&mut flipped, &filter);
    assert_eq!(count, top_faces.len(), "should flip only top faces");
    assert_eq!(flipped.face_count(), fc, "face count should not change");

    // Verify top face normals are now inverted
    for &f in &top_faces {
        let n_orig = normals::compute_face_normal(&mesh, f);
        let n_flip = normals::compute_face_normal(&flipped, f);
        // After flip, the y component should have opposite sign
        assert!(
            n_orig[1] * n_flip[1] < 0.0,
            "face {f} normal y should be inverted: {:.2} vs {:.2}",
            n_orig[1],
            n_flip[1]
        );
    }
}

// ── Bevel vertex-cap gap fix ────────────────────────────────────────

#[test]
fn bevel_depth_after_taper_no_holes() {
    // Reproduces the A380 fuselage hole: ellipse extruded, tapered at ends,
    // then bevel --edges depth.  Before fix, vertex caps at cap-ring vertices
    // were skipped (only 2 inset copies), leaving holes at front and back.
    use std::f64::consts::TAU;

    let segments = 24;
    let circle: Vec<[f64; 2]> = (0..segments)
        .map(|i| {
            let angle = TAU * i as f64 / segments as f64;
            [3.57 * angle.cos(), 4.2 * angle.sin()]
        })
        .collect();

    let mut mesh =
        extrude::extrude_with_inset(&circle, PlaneKind::Front, 73.0, 8, 0.15).unwrap();
    let pre_taper_verts = mesh.vertex_count();
    let pre_taper_faces = mesh.face_count();
    assert!(pre_taper_verts > 0);
    assert!(pre_taper_faces > 0);

    // Taper nose (front) to 12% and tail (back) to 25%
    super::taper::taper(&mut mesh, 2, 0.12, 1.0, None, Some((0.0, 0.16)));
    super::taper::taper(&mut mesh, 2, 1.0, 0.25, None, Some((0.75, 1.0)));

    // Bevel depth edges — this is where the holes appeared
    let beveled = bevel::bevel(&mesh, 0.3, 2, "depth");
    assert!(
        beveled.face_count() > mesh.face_count(),
        "bevel should produce more faces: {} vs {}",
        beveled.face_count(),
        mesh.face_count()
    );

    // Verify watertightness: every edge should have exactly 2 adjacent faces.
    // A hole means some edge has only 1 adjacent face (boundary edge).
    let boundary = count_boundary_edges(&beveled);
    assert_eq!(
        boundary, 0,
        "beveled tapered mesh should be watertight (0 boundary edges), found {boundary}"
    );
}

#[test]
fn bevel_depth_tapered_wing_no_holes() {
    // Reproduces the wing hole: 6-point profile, taper to 0.25x, bevel depth.
    let wing_profile = [
        [3.6, -8.0],
        [15.0, -3.0],
        [30.0, 2.0],
        [40.0, 5.0],
        [40.0, 9.0],
        [3.6, 9.0],
    ];
    let mut mesh = extrude::extrude(&wing_profile, PlaneKind::Top, 2.0, 2).unwrap();

    super::taper::taper(&mut mesh, 0, 1.0, 0.25, None, None);

    let beveled = bevel::bevel(&mesh, 0.3, 2, "depth");
    assert!(beveled.face_count() > mesh.face_count());

    let boundary = count_boundary_edges(&beveled);
    assert_eq!(
        boundary, 0,
        "beveled tapered wing should be watertight, found {boundary} boundary edges"
    );
}

/// Count boundary edges (edges with only one adjacent face).
/// Boundary half-edges have `face: None` — their twin has a face.
fn count_boundary_edges(mesh: &HalfEdgeMesh) -> usize {
    mesh.half_edges.iter().filter(|he| he.face.is_none()).count()
}

/// Count faces by vertex count: (triangles, quads, n-gons).
fn face_type_counts(mesh: &HalfEdgeMesh) -> (usize, usize, usize) {
    let mut tris = 0;
    let mut quads = 0;
    let mut ngons = 0;
    for f in 0..mesh.face_count() {
        match mesh.face_vertices(f).len() {
            3 => tris += 1,
            4 => quads += 1,
            _ => ngons += 1,
        }
    }
    (tris, quads, ngons)
}

#[test]
fn bevel_depth_seg1_tapered_wing() {
    let wing_profile = [
        [3.6, -8.0],
        [15.0, -3.0],
        [30.0, 2.0],
        [40.0, 5.0],
        [40.0, 9.0],
        [3.6, 9.0],
    ];
    let mut mesh = extrude::extrude(&wing_profile, PlaneKind::Top, 2.0, 2).unwrap();
    super::taper::taper(&mut mesh, 0, 1.0, 0.25, None, None);

    let beveled = bevel::bevel_seg1(&mesh, 0.3, "depth");
    assert_eq!(
        count_boundary_edges(&beveled),
        0,
        "tapered wing seg1 bevel should be watertight"
    );
}

#[test]
fn bevel_depth_seg1_cube() {
    let cube = [
        [1.0, 1.0],
        [-1.0, 1.0],
        [-1.0, -1.0],
        [1.0, -1.0],
    ];
    let mesh = extrude::extrude(&cube, PlaneKind::Front, 2.0, 1).unwrap();
    let beveled = bevel::bevel_seg1(&mesh, 0.2, "depth");
    assert_eq!(count_boundary_edges(&beveled), 0, "cube bevel depth should be watertight");
}

#[test]
fn bevel_depth_seg1_untapered_wing() {
    let wing_profile = [
        [3.6, -8.0],
        [15.0, -3.0],
        [30.0, 2.0],
        [40.0, 5.0],
        [40.0, 9.0],
        [3.6, 9.0],
    ];
    let mesh = extrude::extrude(&wing_profile, PlaneKind::Top, 2.0, 2).unwrap();
    assert_eq!(count_boundary_edges(&mesh), 0, "input mesh should be watertight");

    let beveled = bevel::bevel_seg1(&mesh, 0.3, "depth");
    assert_eq!(
        count_boundary_edges(&beveled),
        0,
        "untapered wing bevel should be watertight"
    );
}

#[test]
fn bevel_all_seg1_cube() {
    let cube = [
        [1.0, 1.0],
        [-1.0, 1.0],
        [-1.0, -1.0],
        [1.0, -1.0],
    ];
    let mesh = extrude::extrude(&cube, PlaneKind::Front, 2.0, 1).unwrap();
    let beveled = bevel::bevel_seg1(&mesh, 0.2, "all");
    assert_eq!(count_boundary_edges(&beveled), 0, "cube bevel all should be watertight");
}

#[test]
fn bevel_caps_produce_quads() {
    use std::f64::consts::TAU;

    // Cube bevel --edges all, seg1: each vertex has 3 faces, 3 sharp edges.
    // Cap polygon has 3 vertices → must be triangle (unavoidable).
    let cube = [
        [1.0, 1.0],
        [-1.0, 1.0],
        [-1.0, -1.0],
        [1.0, -1.0],
    ];
    let mesh = extrude::extrude(&cube, PlaneKind::Front, 2.0, 1).unwrap();
    let beveled = bevel::bevel_seg1(&mesh, 0.2, "all");
    let (tris, quads, ngons) = face_type_counts(&beveled);
    assert_eq!(ngons, 0, "no N-gons");
    // 8 vertices × 1 tri cap each = 8 tris. Rest should be quads.
    assert!(quads > tris, "quads ({quads}) should outnumber tris ({tris})");

    // Wing bevel --edges depth, seg1: profile vertices have 3+ faces,
    // some with 4+ (even cap vertex count → quads).
    let wing = [
        [3.6, -8.0],
        [15.0, -3.0],
        [30.0, 2.0],
        [40.0, 5.0],
        [40.0, 9.0],
        [3.6, 9.0],
    ];
    let wmesh = extrude::extrude(&wing, PlaneKind::Top, 2.0, 2).unwrap();
    let wbev = bevel::bevel_seg1(&wmesh, 0.3, "depth");
    let (wtris, wquads, wngons) = face_type_counts(&wbev);
    assert_eq!(wngons, 0, "no N-gons in wing bevel");
    assert!(wquads > 0, "wing bevel should produce quads");
    assert!(
        wquads > wtris,
        "wing should be quad-dominant: {wquads} quads vs {wtris} tris"
    );
    assert_eq!(count_boundary_edges(&wbev), 0, "watertight");

    // Fuselage bevel seg2: more faces per vertex → larger caps → more quads.
    let segments = 24;
    let circle: Vec<[f64; 2]> = (0..segments)
        .map(|i| {
            let angle = TAU * i as f64 / segments as f64;
            [3.57 * angle.cos(), 4.2 * angle.sin()]
        })
        .collect();
    let mut mesh =
        extrude::extrude_with_inset(&circle, PlaneKind::Front, 73.0, 8, 0.15).unwrap();
    super::taper::taper(&mut mesh, 2, 0.12, 1.0, None, Some((0.0, 0.16)));
    super::taper::taper(&mut mesh, 2, 1.0, 0.25, None, Some((0.75, 1.0)));
    let beveled = bevel::bevel(&mesh, 0.3, 2, "depth");
    let (tris, quads, ngons) = face_type_counts(&beveled);
    assert_eq!(ngons, 0, "no N-gons in fuselage bevel");
    assert!(
        quads > tris,
        "fuselage bevel should be quad-dominant: {quads} quads vs {tris} tris"
    );
    assert_eq!(count_boundary_edges(&beveled), 0, "watertight");
}

#[test]
fn bevel_complex_profile_no_panic() {
    // 32-point side profile, extruded 4 segments, tapered with midpoint.
    // Reproduces usize underflow crash on non-manifold edges in Phase 2.5.
    use std::f64::consts::TAU;
    let n = 32;
    let profile: Vec<[f64; 2]> = (0..n)
        .map(|i| {
            let angle = TAU * i as f64 / n as f64;
            [3.0 * angle.cos(), 2.0 * angle.sin()]
        })
        .collect();
    let mut mesh =
        extrude::extrude_with_inset(&profile, PlaneKind::Side, 10.0, 4, 0.1).unwrap();
    super::taper::taper(&mut mesh, 2, 0.3, 1.0, Some(0.5), Some((0.0, 0.5)));
    super::taper::taper(&mut mesh, 2, 1.0, 0.3, Some(0.5), Some((0.5, 1.0)));
    // Should not panic (previously caused usize underflow at bevel.rs:250)
    let beveled = bevel::bevel(&mesh, 0.08, 2, "depth");
    assert!(beveled.face_count() > 0);
}

#[test]
fn extrude_with_holes_correct_winding() {
    // Circle with a circular hole — hole walls should face outward, not inward.
    use std::f64::consts::TAU;
    let n = 16;
    let outer: Vec<[f64; 2]> = (0..n)
        .map(|i| {
            let angle = TAU * i as f64 / n as f64;
            [1.0 * angle.cos(), 1.0 * angle.sin()]
        })
        .collect();
    let hole: Vec<[f64; 2]> = (0..n)
        .map(|i| {
            let angle = TAU * i as f64 / n as f64;
            [0.3 * angle.cos(), 0.3 * angle.sin()]
        })
        .collect();

    let mesh = extrude::extrude_with_holes(&outer, &[hole], PlaneKind::Front, 1.0, 1).unwrap();
    assert!(mesh.face_count() > 0);
    assert_eq!(count_boundary_edges(&mesh), 0, "hole extrusion should be watertight");

    // Verify normals: for a closed mesh, fix_winding should flip 0 faces
    // (if winding is already correct, all normals point outward consistently).
    let mut check = mesh.clone();
    let flipped = normals::fix_winding(&mut check);
    assert_eq!(
        flipped, 0,
        "hole walls should have correct outward winding (fix_winding flipped {flipped} faces)"
    );
}

#[test]
fn mesh_stats_basic() {
    // Build a 2-part state and verify stats computation.
    let mut state = MeshState::new("body");
    state.active_part_mut().unwrap().mesh = cube_mesh(); // 12 tris

    let mut part2 = super::MeshPart::new();
    part2.mesh = single_triangle(); // 1 tri
    state.parts.insert("wing".to_string(), part2);

    // Active part is "body" (cube: 12 tris, all triangle faces)
    let stats = crate::cli::mesh_cmd::mesh_stats(&state);
    assert_eq!(stats["active_part"], "body");
    assert_eq!(stats["part_faces"], 12); // cube has 12 triangle faces
    assert_eq!(stats["part_tris"], 12);
    assert_eq!(stats["part_quads"], 0);
    assert_eq!(stats["part_ngons"], 0);
    assert_eq!(stats["total_parts"], 2);
    // 12 tris from cube + 1 tri from wing = 13 total
    assert_eq!(stats["total_tris_godot"], 13);
}

// ── Group tests ─────────────────────────────────────────────────────

#[test]
fn group_create_and_list() {
    let mut state = MeshState::new("body");

    let mut wing = super::MeshPart::new();
    wing.mesh = single_triangle();
    state.parts.insert("wing-L".to_string(), wing);

    let mut wing_r = super::MeshPart::new();
    wing_r.mesh = single_triangle();
    state.parts.insert("wing-R".to_string(), wing_r);

    // Create a group
    state
        .groups
        .insert("wings".to_string(), vec!["wing-L".to_string(), "wing-R".to_string()]);

    assert_eq!(state.groups.len(), 1);
    assert_eq!(state.groups["wings"].len(), 2);
    assert!(state.groups["wings"].contains(&"wing-L".to_string()));
    assert!(state.groups["wings"].contains(&"wing-R".to_string()));

    // Remove group (parts remain)
    state.groups.remove("wings");
    assert!(state.groups.is_empty());
    assert!(state.parts.contains_key("wing-L"));
    assert!(state.parts.contains_key("wing-R"));
}

#[test]
fn group_duplicate() {
    let mut state = MeshState::new("body");

    let mut eng = super::MeshPart::new();
    eng.mesh = cube_mesh();
    state.parts.insert("engine-1".to_string(), eng);

    let mut intake = super::MeshPart::new();
    intake.mesh = single_triangle();
    state.parts.insert("intake-1".to_string(), intake);

    state.groups.insert(
        "eng-1".to_string(),
        vec!["engine-1".to_string(), "intake-1".to_string()],
    );

    // Simulate group duplicate with --replace "1" --with "2"
    let members = state.groups["eng-1"].clone();
    let mut new_members = Vec::new();
    for src_name in &members {
        let new_name = src_name.replace('1', "2");
        let src_part = state.parts[src_name].clone();
        state.parts.insert(new_name.clone(), src_part);
        new_members.push(new_name);
    }
    state
        .groups
        .insert("eng-2".to_string(), new_members.clone());

    assert_eq!(state.parts.len(), 5); // body + engine-1 + intake-1 + engine-2 + intake-2
    assert_eq!(state.groups.len(), 2);
    assert_eq!(state.groups["eng-2"], vec!["engine-2", "intake-2"]);
    assert_eq!(
        state.parts["engine-2"].mesh.vertices.len(),
        state.parts["engine-1"].mesh.vertices.len()
    );
}

// ── Agent replay: multi-part model build ────────────────────────────
//
// Replays the core mesh operations from a real agent session (179
// commands, ~27 parts). Exercises profile→extrude→taper→bevel→mirror
// →duplicate pipeline and validates mesh integrity at each step.

/// Generate CCW ellipse points centered at origin.
fn ellipse(rx: f64, ry: f64, segments: u32) -> Vec<[f64; 2]> {
    use std::f64::consts::TAU;
    (0..segments)
        .map(|i| {
            let angle = TAU * f64::from(i) / f64::from(segments);
            [rx * angle.cos(), ry * angle.sin()]
        })
        .collect()
}

/// Generate a tapered 2D profile (leaf/fin shape) for extrusion.
fn tapered_profile(chord: f64, thickness: f64, n: u32) -> Vec<[f64; 2]> {
    use std::f64::consts::TAU;
    (0..n)
        .map(|i| {
            let t = f64::from(i) / f64::from(n);
            let x = chord * (1.0 - (TAU * t / 2.0).cos()) / 2.0;
            let y = if t < 0.5 {
                thickness * (t * 2.0)
            } else {
                thickness * (2.0 - t * 2.0)
            };
            [x, y]
        })
        .collect()
}

/// Assert mesh is non-degenerate and watertight (0 boundary edges).
fn assert_closed(mesh: &HalfEdgeMesh, label: &str) {
    assert!(mesh.vertex_count() > 0, "{label}: 0 vertices");
    assert!(mesh.face_count() > 0, "{label}: 0 faces");
    let boundary = count_boundary_edges(mesh);
    assert_eq!(boundary, 0, "{label}: {boundary} boundary edges (not watertight)");
}

/// Assert mesh is non-degenerate (may have boundary edges for open shapes).
fn assert_valid(mesh: &HalfEdgeMesh, label: &str) {
    assert!(mesh.vertex_count() > 0, "{label}: 0 vertices");
    assert!(mesh.face_count() > 0, "{label}: 0 faces");
}

#[test]
#[allow(clippy::too_many_lines)]
fn agent_replay_multipart_build() {
    use super::extrude;
    use super::taper;
    use super::bevel;
    use super::mirror;
    use super::normals;

    let mut state = MeshState::new("fuselage");

    // ── Step 1: Fuselage ──────────────────────────────────────────────
    // profile --plane front --shape ellipse --rx 1.0 --ry 0.8 --segments 16
    let fuse_profile = ellipse(1.0, 0.8, 16);
    // extrude --depth 16.0 --segments 10
    let fuse_mesh = extrude::extrude(&fuse_profile, PlaneKind::Front, 16.0, 10).unwrap();
    assert_closed(&fuse_mesh, "fuselage after extrude");

    let part = state.active_part_mut().unwrap();
    part.mesh = fuse_mesh;

    // taper --axis z --from 0.0 --to 0.35 --from-scale 0.12 --to-scale 1.0
    taper::taper(&mut part.mesh, 2, 0.12, 1.0, None, Some((0.0, 0.35)));
    assert_closed(&part.mesh, "fuselage after nose taper");

    // taper --axis z --from 0.65 --to 1.0 --from-scale 1.0 --to-scale 0.55
    taper::taper(&mut part.mesh, 2, 1.0, 0.55, None, Some((0.65, 1.0)));
    assert_closed(&part.mesh, "fuselage after tail taper");
    let fuse_verts = part.mesh.vertex_count();
    let fuse_faces = part.mesh.face_count();

    // ── Step 2: Wing (profile + extrude + taper + mirror) ─────────────
    let wing_pts = tapered_profile(6.0, 0.15, 21);
    let wing_mesh = extrude::extrude(&wing_pts, PlaneKind::Side, 1.0, 1).unwrap();
    assert_valid(&wing_mesh, "wing-r after extrude");

    let mut wing_part = super::MeshPart::new();
    wing_part.mesh = wing_mesh;
    // taper --axis x --from-scale 1.0 --to-scale 0.3
    taper::taper(&mut wing_part.mesh, 0, 1.0, 0.3, None, None);
    assert_valid(&wing_part.mesh, "wing-r after taper");
    state.parts.insert("wing-r".to_string(), wing_part);

    // duplicate-part --name wing-r --as wing-l --mirror x
    let mut wing_l = state.parts["wing-r"].clone();
    mirror::mirror(&mut wing_l.mesh, 0); // axis x
    assert_valid(&wing_l.mesh, "wing-l after mirror");
    assert_eq!(wing_l.mesh.vertex_count(), state.parts["wing-r"].mesh.vertex_count());
    state.parts.insert("wing-l".to_string(), wing_l);

    // ── Step 3: Canards ───────────────────────────────────────────────
    let canard_pts = tapered_profile(1.5, 0.08, 12);
    let canard_mesh = extrude::extrude(&canard_pts, PlaneKind::Side, 0.4, 1).unwrap();
    let mut canard_part = super::MeshPart::new();
    canard_part.mesh = canard_mesh;
    taper::taper(&mut canard_part.mesh, 0, 1.0, 0.4, None, None);
    assert_valid(&canard_part.mesh, "canard-r");
    state.parts.insert("canard-r".to_string(), canard_part);

    let mut canard_l = state.parts["canard-r"].clone();
    mirror::mirror(&mut canard_l.mesh, 0);
    state.parts.insert("canard-l".to_string(), canard_l);

    // ── Step 4: Vertical fin ──────────────────────────────────────────
    let fin_pts = tapered_profile(3.0, 0.1, 16);
    let fin_mesh = extrude::extrude(&fin_pts, PlaneKind::Top, 0.15, 1).unwrap();
    let mut fin_part = super::MeshPart::new();
    fin_part.mesh = fin_mesh;
    taper::taper(&mut fin_part.mesh, 1, 1.0, 0.3, None, None);
    assert_valid(&fin_part.mesh, "fin");
    state.parts.insert("fin".to_string(), fin_part);

    // ── Step 5: Intakes (simple boxes via ellipse extrude) ────────────
    let intake_profile = ellipse(0.4, 0.3, 8);
    let intake_mesh = extrude::extrude(&intake_profile, PlaneKind::Front, 2.0, 2).unwrap();
    assert_closed(&intake_mesh, "intake-r");
    let mut intake_part = super::MeshPart::new();
    intake_part.mesh = intake_mesh;
    state.parts.insert("intake-r".to_string(), intake_part);

    let mut intake_l = state.parts["intake-r"].clone();
    mirror::mirror(&mut intake_l.mesh, 0);
    state.parts.insert("intake-l".to_string(), intake_l);

    // ── Step 6: Exhausts (circle profile + extrude, not cylinder prim)
    let exhaust_profile = ellipse(0.3, 0.3, 12);
    let exhaust_mesh = extrude::extrude(&exhaust_profile, PlaneKind::Front, 1.5, 2).unwrap();
    assert_closed(&exhaust_mesh, "exhaust-r");
    let mut exhaust_part = super::MeshPart::new();
    exhaust_part.mesh = exhaust_mesh;
    state.parts.insert("exhaust-r".to_string(), exhaust_part);

    let mut exhaust_l = state.parts["exhaust-r"].clone();
    mirror::mirror(&mut exhaust_l.mesh, 0);
    state.parts.insert("exhaust-l".to_string(), exhaust_l);

    // ── Step 7: Canopy (ellipse + extrude + taper) ────────────────────
    let canopy_profile = ellipse(0.5, 0.35, 12);
    let canopy_mesh = extrude::extrude(&canopy_profile, PlaneKind::Front, 2.5, 4).unwrap();
    let mut canopy_part = super::MeshPart::new();
    canopy_part.mesh = canopy_mesh;
    taper::taper(&mut canopy_part.mesh, 2, 0.3, 1.0, None, Some((0.0, 0.4)));
    taper::taper(&mut canopy_part.mesh, 2, 1.0, 0.2, None, Some((0.7, 1.0)));
    assert_closed(&canopy_part.mesh, "canopy");
    state.parts.insert("canopy".to_string(), canopy_part);

    // ── Step 8: Cockpit interior parts ────────────────────────────────
    // Simplified: tub, seat, panel, stick, throttle, pedals
    let box_profile = vec![[-0.3, -0.2], [0.3, -0.2], [0.3, 0.2], [-0.3, 0.2]];
    for name in &["cockpit-tub", "ejection-seat", "instrument-panel",
                   "stick", "throttle", "pedals", "console-l", "console-r",
                   "hud", "coaming"] {
        let mesh = extrude::extrude(&box_profile, PlaneKind::Front, 0.5, 1).unwrap();
        assert_closed(&mesh, name);
        let mut p = super::MeshPart::new();
        p.mesh = mesh;
        state.parts.insert((*name).to_string(), p);
    }

    // ── Step 9: Landing gear ──────────────────────────────────────────
    // Struts (thin boxes) + wheels (circle extrude)
    let strut_profile = vec![[-0.05, -0.05], [0.05, -0.05], [0.05, 0.05], [-0.05, 0.05]];
    for name in &["nose-strut", "main-strut-l", "main-strut-r"] {
        let mesh = extrude::extrude(&strut_profile, PlaneKind::Front, 1.2, 1).unwrap();
        let mut p = super::MeshPart::new();
        p.mesh = mesh;
        state.parts.insert((*name).to_string(), p);
    }

    let wheel_profile = ellipse(0.15, 0.15, 8);
    let wheel_mesh = extrude::extrude(&wheel_profile, PlaneKind::Front, 0.1, 1).unwrap();
    assert_closed(&wheel_mesh, "wheel");
    for name in &["nose-wheel", "main-wheel-l", "main-wheel-r"] {
        let mut p = super::MeshPart::new();
        p.mesh = wheel_mesh.clone();
        state.parts.insert((*name).to_string(), p);
    }

    // ── Step 10: fix-normals --all ────────────────────────────────────
    let names: Vec<String> = state.parts.keys().cloned().collect();
    for name in &names {
        let part = state.parts.get_mut(name).unwrap();
        normals::fix_winding(&mut part.mesh);
    }

    // ── Step 11: Bevel on fuselage ────────────────────────────────────
    // Agent's bevel crashed on tapered geometry (now fixed). Verify it works.
    let fuse = &state.parts["fuselage"].mesh;
    let beveled = bevel::bevel(fuse, 0.08, 2, "depth");
    assert_valid(&beveled, "fuselage after bevel");
    assert!(beveled.vertex_count() > fuse_verts, "bevel should add vertices");
    assert!(beveled.face_count() > fuse_faces, "bevel should add faces");
    state.parts.get_mut("fuselage").unwrap().mesh = beveled;

    // ── Step 12: Groups ───────────────────────────────────────────────
    state.groups.insert("engine-assembly".to_string(), vec![
        "intake-r".to_string(), "exhaust-r".to_string(),
    ]);
    state.groups.insert("landing-gear".to_string(), vec![
        "nose-strut".to_string(), "nose-wheel".to_string(),
        "main-strut-l".to_string(), "main-wheel-l".to_string(),
        "main-strut-r".to_string(), "main-wheel-r".to_string(),
    ]);

    // ── Step 13: Checkpoint + restore ─────────────────────────────────
    let checkpoint_name = "exterior-done";
    state.checkpoints.insert(checkpoint_name.to_string(), state.parts.clone());
    state.group_checkpoints.insert(checkpoint_name.to_string(), state.groups.clone());

    // Verify restore brings back groups
    let saved_groups = state.group_checkpoints[checkpoint_name].clone();
    assert_eq!(saved_groups.len(), 2);
    assert_eq!(saved_groups["landing-gear"].len(), 6);

    // ── Step 14: Stats validation ─────────────────────────────────────
    state.active = "fuselage".to_string();
    let stats = crate::cli::mesh_cmd::mesh_stats(&state);
    assert_eq!(stats["active_part"], "fuselage");
    assert_eq!(stats["total_parts"], state.parts.len());
    assert!(stats["total_tris_godot"].as_u64().unwrap() > 0);
    assert!(stats["part_faces"].as_u64().unwrap() > 0);

    // ── Step 15: Verify all parts survived ────────────────────────────
    let expected_parts = [
        "fuselage", "wing-r", "wing-l", "canard-r", "canard-l", "fin",
        "intake-r", "intake-l", "exhaust-r", "exhaust-l", "canopy",
        "cockpit-tub", "ejection-seat", "instrument-panel", "stick",
        "throttle", "pedals", "console-l", "console-r", "hud", "coaming",
        "nose-strut", "main-strut-l", "main-strut-r",
        "nose-wheel", "main-wheel-l", "main-wheel-r",
    ];
    assert_eq!(state.parts.len(), expected_parts.len());
    for name in &expected_parts {
        assert!(state.parts.contains_key(*name), "missing part: {name}");
        let mesh = &state.parts[*name].mesh;
        assert!(mesh.vertex_count() > 0, "{name}: 0 vertices after full pipeline");
        assert!(mesh.face_count() > 0, "{name}: 0 faces after full pipeline");
    }

    // ── Step 16: Verify to_arrays works (snapshot path) ───────────────
    // Every part must be able to triangulate for Godot push / snapshot
    for name in &expected_parts {
        let part = &state.parts[*name];
        let (positions, normals_arr, indices) = part.mesh.to_arrays_shaded(part.shading);
        assert!(!positions.is_empty(), "{name}: empty positions from to_arrays");
        assert!(!normals_arr.is_empty(), "{name}: empty normals from to_arrays");
        assert!(!indices.is_empty(), "{name}: empty indices from to_arrays");
    }
}
