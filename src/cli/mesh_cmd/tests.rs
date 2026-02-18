use super::gdscript;

fn assert_parses(script: &str) {
    let tree = crate::core::parser::parse(script).unwrap();
    assert!(
        !tree.root_node().has_error(),
        "Script has parse errors:\n{script}"
    );
}

// ── Create ───────────────────────────────────────────────────────────

#[test]
fn create_empty_parses() {
    assert_parses(&gdscript::generate_create("TestMesh", "empty"));
}

#[test]
fn create_cube_parses() {
    assert_parses(&gdscript::generate_create("TestMesh", "cube"));
}

#[test]
fn create_sphere_parses() {
    assert_parses(&gdscript::generate_create("TestMesh", "sphere"));
}

#[test]
fn create_cylinder_parses() {
    assert_parses(&gdscript::generate_create("TestMesh", "cylinder"));
}

// ── Profile ──────────────────────────────────────────────────────────

#[test]
fn profile_front_parses() {
    let pts = vec![(0.0, 0.0), (2.0, 0.0), (2.0, 3.0), (0.0, 3.0)];
    assert_parses(&gdscript::generate_profile(&pts, "front"));
}

#[test]
fn profile_side_parses() {
    let pts = vec![(0.0, 0.0), (1.0, 0.0), (1.0, 2.0)];
    assert_parses(&gdscript::generate_profile(&pts, "side"));
}

#[test]
fn profile_top_parses() {
    let pts = vec![(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)];
    assert_parses(&gdscript::generate_profile(&pts, "top"));
}

// ── Extrude ──────────────────────────────────────────────────────────

#[test]
fn extrude_parses() {
    assert_parses(&gdscript::generate_extrude(1.5));
}

#[test]
fn extrude_small_depth_parses() {
    assert_parses(&gdscript::generate_extrude(0.1));
}

// ── Revolve ──────────────────────────────────────────────────────────

#[test]
fn revolve_y_360_parses() {
    assert_parses(&gdscript::generate_revolve("y", 360.0, 16));
}

#[test]
fn revolve_x_180_parses() {
    assert_parses(&gdscript::generate_revolve("x", 180.0, 8));
}

#[test]
fn revolve_z_90_parses() {
    assert_parses(&gdscript::generate_revolve("z", 90.0, 4));
}

// ── Move vertex ──────────────────────────────────────────────────────

#[test]
fn move_vertex_parses() {
    assert_parses(&gdscript::generate_move_vertex(0, 0.5, -0.3, 1.0));
}

#[test]
fn move_vertex_large_index_parses() {
    assert_parses(&gdscript::generate_move_vertex(999, 0.0, 0.0, 0.0));
}

// ── Camera ───────────────────────────────────────────────────────────

#[test]
fn switch_camera_front_parses() {
    assert_parses(&gdscript::generate_switch_camera("Front"));
}

#[test]
fn switch_camera_iso_parses() {
    assert_parses(&gdscript::generate_switch_camera("Iso"));
}

#[test]
fn restore_camera_parses() {
    assert_parses(&gdscript::generate_restore_camera());
}

// ── Screenshot ───────────────────────────────────────────────────────

#[test]
fn capture_screenshot_parses() {
    assert_parses(&gdscript::generate_capture_screenshot("front", 12345));
}

// ── Add part ─────────────────────────────────────────────────────────

#[test]
fn add_part_empty_parses() {
    assert_parses(&gdscript::generate_add_part("wing", "empty"));
}

#[test]
fn add_part_cube_parses() {
    assert_parses(&gdscript::generate_add_part("door", "cube"));
}

// ── Duplicate part ──────────────────────────────────────────────────

#[test]
fn duplicate_part_parses() {
    assert_parses(&gdscript::generate_duplicate_part("eng1", "eng2"));
}

// ── Focus ────────────────────────────────────────────────────────────

#[test]
fn focus_part_parses() {
    assert_parses(&gdscript::generate_focus("fuselage"));
}

#[test]
fn focus_all_parses() {
    assert_parses(&gdscript::generate_focus_all());
}

// ── Snapshot ─────────────────────────────────────────────────────────

#[test]
fn snapshot_parses() {
    assert_parses(&gdscript::generate_snapshot("res://test.tscn"));
}

// ── Translate ────────────────────────────────────────────────────────

#[test]
fn translate_absolute_parses() {
    assert_parses(&gdscript::generate_translate(Some("wing"), 5.0, -2.0, 0.0, false));
}

#[test]
fn translate_relative_parses() {
    assert_parses(&gdscript::generate_translate(None, 0.0, 1.5, -3.0, true));
}

// ── Rotate ───────────────────────────────────────────────────────────

#[test]
fn rotate_part_parses() {
    assert_parses(&gdscript::generate_rotate(Some("fin"), 0.0, 0.0, 15.0));
}

#[test]
fn rotate_active_parses() {
    assert_parses(&gdscript::generate_rotate(None, 45.0, -30.0, 0.0));
}

// ── Scale ────────────────────────────────────────────────────────────

#[test]
fn scale_part_parses() {
    assert_parses(&gdscript::generate_scale(Some("engine"), 0.15, 0.15, 1.0));
}

#[test]
fn scale_active_parses() {
    assert_parses(&gdscript::generate_scale(None, 2.0, 2.0, 2.0));
}

// ── Remove part ──────────────────────────────────────────────────────

#[test]
fn remove_part_parses() {
    assert_parses(&gdscript::generate_remove_part("engine_1"));
}

// ── List vertices ────────────────────────────────────────────────────

#[test]
fn list_vertices_all_parses() {
    assert_parses(&gdscript::generate_list_vertices(None));
}

#[test]
fn list_vertices_region_parses() {
    let region: super::BoundingBox = ((-1.0, -1.0, -1.0), (1.0, 1.0, 1.0));
    assert_parses(&gdscript::generate_list_vertices(Some(&region)));
}

// ── Taper ────────────────────────────────────────────────────────────

#[test]
fn taper_y_parses() {
    assert_parses(&gdscript::generate_taper("y", 1.0, 0.0));
}

#[test]
fn taper_z_parses() {
    assert_parses(&gdscript::generate_taper("z", 1.0, 0.5));
}

#[test]
fn taper_x_parses() {
    assert_parses(&gdscript::generate_taper("x", 0.5, 1.5));
}

// ── Bevel ────────────────────────────────────────────────────────────

#[test]
fn bevel_parses() {
    assert_parses(&gdscript::generate_bevel(0.1, 2));
}

#[test]
fn bevel_high_segments_parses() {
    assert_parses(&gdscript::generate_bevel(0.05, 4));
}

// ── Info ─────────────────────────────────────────────────────────────

#[test]
fn info_parses() {
    assert_parses(&gdscript::generate_info());
}

#[test]
fn info_all_parses() {
    assert_parses(&gdscript::generate_info_all());
}

// ── Autofit camera ──────────────────────────────────────────────────

#[test]
fn autofit_cameras_parses() {
    assert_parses(&gdscript::generate_autofit_cameras(1.0));
}

#[test]
fn autofit_cameras_zoom_parses() {
    assert_parses(&gdscript::generate_autofit_cameras(2.5));
}

// ── Grid ─────────────────────────────────────────────────────────────

#[test]
fn grid_front_parses() {
    assert_parses(&gdscript::generate_grid("front", 5.0));
}

#[test]
fn grid_side_parses() {
    assert_parses(&gdscript::generate_grid("side", 5.0));
}

#[test]
fn grid_top_parses() {
    assert_parses(&gdscript::generate_grid("top", 5.0));
}

#[test]
fn remove_grid_parses() {
    assert_parses(&gdscript::generate_remove_grid());
}

// ── Normal debug ────────────────────────────────────────────────────

#[test]
fn normal_debug_parses() {
    assert_parses(&gdscript::generate_normal_debug());
}

#[test]
fn normal_debug_clear_parses() {
    assert_parses(&gdscript::generate_normal_debug_clear());
}

// ── Parse helpers ────────────────────────────────────────────────────

#[test]
fn parse_points_valid() {
    let result = super::parse_points("0,0 2,0 2,3 0,3").unwrap();
    assert_eq!(result.len(), 4);
    assert_eq!(result[0], (0.0, 0.0));
    assert_eq!(result[2], (2.0, 3.0));
}

#[test]
fn parse_points_negative() {
    let result = super::parse_points("-1,-1 1,-1 1,1").unwrap();
    assert_eq!(result.len(), 3);
    assert_eq!(result[0], (-1.0, -1.0));
}

#[test]
fn parse_points_too_few() {
    assert!(super::parse_points("0,0 1,1").is_err());
}

#[test]
fn parse_points_invalid_format() {
    assert!(super::parse_points("0,0,0 1,1 2,2").is_err());
}

#[test]
fn parse_points_not_numbers() {
    assert!(super::parse_points("a,b c,d e,f").is_err());
}

#[test]
fn parse_3d_valid() {
    let (x, y, z) = super::parse_3d("0.5,-0.3,1.0").unwrap();
    assert!((x - 0.5).abs() < f64::EPSILON);
    assert!((y - -0.3).abs() < f64::EPSILON);
    assert!((z - 1.0).abs() < f64::EPSILON);
}

#[test]
fn parse_3d_with_spaces() {
    let (x, y, z) = super::parse_3d("1, 2, 3").unwrap();
    assert!((x - 1.0).abs() < f64::EPSILON);
    assert!((y - 2.0).abs() < f64::EPSILON);
    assert!((z - 3.0).abs() < f64::EPSILON);
}

#[test]
fn parse_3d_invalid() {
    assert!(super::parse_3d("1,2").is_err());
    assert!(super::parse_3d("1,2,3,4").is_err());
    assert!(super::parse_3d("a,b,c").is_err());
}
