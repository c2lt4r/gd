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
    // 2 cap triangles × 2 caps + 4 side quads × 2 triangles = 12 faces
    assert_eq!(mesh.faces.len(), 12);
}

#[test]
fn extrude_triangle_side() {
    let points = [[0.0, 0.0], [1.0, 0.0], [0.5, 1.0]];
    let mesh = extrude::extrude(&points, PlaneKind::Side, 3.0, 1).unwrap();
    // 3 profile points × 2 sections = 6 vertices
    assert_eq!(mesh.vertices.len(), 6);
    // 1 cap tri × 2 caps + 3 side quads × 2 tris = 8 faces
    assert_eq!(mesh.faces.len(), 8);
}

#[test]
fn extrude_with_segments() {
    let points = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
    let mesh = extrude::extrude(&points, PlaneKind::Front, 2.0, 4).unwrap();
    // 4 profile points × 5 sections = 20 vertices
    assert_eq!(mesh.vertices.len(), 20);
    // 2 cap tris × 2 + 4 side quads × 4 segments × 2 tris = 4 + 32 = 36
    assert_eq!(mesh.faces.len(), 36);
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
    // Create an extruded square mesh (8 verts, 12 faces)
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

    // Expected: each face is a triangle, so indices = faces * 3
    let expected_index_count = face_count_before * 3;
    assert_eq!(
        indices.len(),
        expected_index_count,
        "index count ({}) should equal face_count * 3 ({}); some faces were lost in traversal",
        indices.len(),
        expected_index_count,
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

    // Verify each face is still traversable (face_vertices returns exactly 3)
    for f in 0..mesh.faces.len() {
        let verts = mesh.face_vertices(f);
        assert_eq!(
            verts.len(),
            3,
            "face {f} has {} vertices after mirror (expected 3); half-edge cycle is broken",
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

    let expected_index_count = face_count_before * 3;
    assert_eq!(
        indices.len(),
        expected_index_count,
        "index count ({}) != face_count * 3 ({}); faces lost in half-edge traversal",
        indices.len(),
        expected_index_count,
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
        assert_eq!(
            verts.len(),
            3,
            "face {f} has {} vertices after mirror (expected 3)",
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
    // 4 quads × 2 triangles = 8 faces
    assert_eq!(mesh.faces.len(), 8);
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
    // 8 side + 2 cap tris × 2 = 12 faces
    assert_eq!(mesh.faces.len(), 12);
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
    // 2 sections × 3 quads × 2 tris = 12 faces
    assert_eq!(mesh.faces.len(), 12);
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
    // All faces should be proper triangles with distinct vertices
    for f in 0..result.faces.len() {
        let verts = result.face_vertices(f);
        assert_eq!(verts.len(), 3, "all faces should be triangles");
        assert_ne!(verts[0], verts[1]);
        assert_ne!(verts[1], verts[2]);
        assert_ne!(verts[2], verts[0]);
    }
}

#[test]
fn subtract_no_overlap() {
    let target = cube_mesh();
    let tool = cube_mesh();
    // Offset tool far away — no overlap
    let result = boolean::subtract(&target, &tool, [10.0, 0.0, 0.0]);
    // Should be unchanged (all vertices outside tool, no tool centroids inside target)
    assert_eq!(result.faces.len(), target.faces.len());
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
    // All target faces kept (none inside small tool) + all tool cap faces added
    assert!(
        result.faces.len() > target.faces.len(),
        "hollow result should have more faces than solid (got {} vs {})",
        result.faces.len(),
        target.faces.len()
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
    let mesh_inset =
        extrude::extrude_with_inset(&points, PlaneKind::Front, 2.0, 1, 0.15).unwrap();

    // Inset adds 2 rings of n_pts inset vertices (front + back)
    assert_eq!(
        mesh_inset.vertices.len(),
        mesh_no_inset.vertices.len() + 2 * 16,
        "inset should add 32 vertices (16 per cap)"
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

    let mesh =
        extrude::extrude_with_inset(&points, PlaneKind::Side, 2.0, 1, 0.15).unwrap();
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
    let result = boolean::boolean_op(
        &target,
        &tool,
        [5.0, 0.0, 0.0],
        boolean::BooleanMode::Union,
    );
    assert_eq!(
        result.faces.len(),
        target.faces.len() + tool.faces.len(),
        "union of non-overlapping cubes should have all faces from both"
    );
}

#[test]
fn boolean_union_overlapping_reduces_faces() {
    let target = cube_mesh();
    let tool = cube_mesh();
    let result = boolean::boolean_op(
        &target,
        &tool,
        [0.3, 0.0, 0.0],
        boolean::BooleanMode::Union,
    );
    // Overlapping union should have fewer faces than both combined
    assert!(
        result.faces.len() < target.faces.len() + tool.faces.len(),
        "union overlap ({}) should be less than sum ({})",
        result.faces.len(),
        target.faces.len() + tool.faces.len()
    );
    assert!(!result.faces.is_empty());
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
    // Smaller than either input
    assert!(result.faces.len() < target.faces.len() + tool.faces.len());
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
    assert_eq!(result.faces.len(), 0, "no-overlap intersect should be empty");
}

// ── Bevel profile ────────────────────────────────────────────────────

#[test]
fn bevel_profile_concave_differs_from_convex() {
    let mesh = cube_mesh();
    let concave = bevel::bevel_with_profile(&mesh, 0.1, 3, "all", 0.0);
    let convex = bevel::bevel_with_profile(&mesh, 0.1, 3, "all", 1.0);
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
    assert!(any_different, "concave and convex bevels should differ in geometry");
}

#[test]
fn bevel_profile_default_matches_original() {
    let mesh = cube_mesh();
    let original = bevel::bevel(&mesh, 0.1, 2, "all");
    let with_profile = bevel::bevel_with_profile(&mesh, 0.1, 2, "all", 0.5);
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

