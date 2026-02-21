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

// ── Camera ───────────────────────────────────────────────────────────

#[test]
fn switch_camera_front_parses() {
    assert_parses(&gdscript::generate_switch_camera("Front"));
}

#[test]
fn switch_camera_perspective_parses() {
    assert_parses(&gdscript::generate_switch_camera("FrontRight"));
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

// ── Mirror part ─────────────────────────────────────────────────────

#[test]
fn mirror_part_x_parses() {
    assert_parses(&gdscript::generate_mirror_part(
        "wing-right",
        "wing-left",
        "x",
        false,
    ));
}

#[test]
fn mirror_part_z_parses() {
    assert_parses(&gdscript::generate_mirror_part("eng1", "eng2", "z", false));
}

#[test]
fn mirror_part_symmetric_x_parses() {
    assert_parses(&gdscript::generate_mirror_part(
        "wheel-fr", "wheel-fl", "x", true,
    ));
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
    assert_parses(&gdscript::generate_translate(
        Some("wing"),
        5.0,
        -2.0,
        0.0,
        false,
    ));
}

#[test]
fn translate_relative_parses() {
    assert_parses(&gdscript::generate_translate(None, 0.0, 1.5, -3.0, true));
}

// ── Translate relative-to ────────────────────────────────────────────

#[test]
fn translate_relative_to_parses() {
    assert_parses(&gdscript::generate_translate_relative_to(
        Some("engine-1"),
        "wing-right",
        0.0,
        -2.0,
        0.0,
    ));
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
    assert_parses(&gdscript::generate_scale(
        Some("engine"),
        0.15,
        0.15,
        1.0,
        false,
    ));
}

#[test]
fn scale_active_parses() {
    assert_parses(&gdscript::generate_scale(None, 2.0, 2.0, 2.0, false));
}

#[test]
fn scale_remap_parses() {
    assert_parses(&gdscript::generate_scale(Some("wing"), 1.0, 0.5, 1.0, true));
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

// ── Material ─────────────────────────────────────────────────────────

#[test]
fn material_hex_parses() {
    assert_parses(&gdscript::generate_material(Some("body"), "ff3300"));
}

#[test]
fn material_active_parses() {
    assert_parses(&gdscript::generate_material(None, "00ff00"));
}

// ── Material preset ─────────────────────────────────────────────────

#[test]
fn material_preset_glass_parses() {
    assert_parses(&gdscript::generate_material_preset(None, "glass", None));
}

#[test]
fn material_preset_metal_with_color_parses() {
    assert_parses(&gdscript::generate_material_preset(
        Some("body"),
        "metal",
        Some("aaaaaa"),
    ));
}

#[test]
fn material_preset_rubber_parses() {
    assert_parses(&gdscript::generate_material_preset(None, "rubber", None));
}

#[test]
fn material_preset_chrome_parses() {
    assert_parses(&gdscript::generate_material_preset(None, "chrome", None));
}

// ── Material multi ──────────────────────────────────────────────────

#[test]
fn material_multi_glob_parses() {
    assert_parses(&gdscript::generate_material_multi("wheel-*", "333333"));
}

#[test]
fn material_multi_comma_parses() {
    assert_parses(&gdscript::generate_material_multi("body,roof", "ff0000"));
}

#[test]
fn material_preset_multi_parses() {
    assert_parses(&gdscript::generate_material_preset_multi(
        "window-*", "glass", None,
    ));
}

// ── Material preset multi (indentation regression) ──────────────────

#[test]
fn material_preset_multi_glass_with_color_parses() {
    assert_parses(&gdscript::generate_material_preset_multi(
        "windshield,rear-window",
        "glass",
        Some("8ab8d0"),
    ));
}

#[test]
fn material_preset_multi_metal_parses() {
    assert_parses(&gdscript::generate_material_preset_multi(
        "wheel-*", "metal", None,
    ));
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

// ── Edge overlay ─────────────────────────────────────────────────────

#[test]
fn edge_overlay_parses() {
    use super::overlay::EdgeOverlayData;

    let data = EdgeOverlayData {
        positions: vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
        ],
        boundary: vec![(0, 1)],
        sharp: vec![(1, 2)],
        interior: vec![(2, 3), (3, 0)],
    };
    assert_parses(&gdscript::generate_edge_overlay(&data));
}

#[test]
fn remove_edge_overlay_parses() {
    assert_parses(&gdscript::generate_remove_edge_overlay());
}

// ── Classified edges ────────────────────────────────────────────────

#[test]
fn classified_edges_cube() {
    let mesh = crate::core::mesh::primitives::cube();
    let edges = mesh.classified_edges();
    // A closed cube has 12 edges, all sharp (90° dihedral)
    assert!(
        edges.boundary.is_empty(),
        "Cube should have no boundary edges"
    );
    assert_eq!(edges.sharp.len(), 12, "Cube should have 12 sharp edges");
    assert!(
        edges.interior.is_empty(),
        "Cube should have no interior edges"
    );
}

#[test]
fn classified_edges_sphere() {
    let mesh = crate::core::mesh::primitives::sphere(8, 4);
    let edges = mesh.classified_edges();
    assert!(
        edges.boundary.is_empty(),
        "Closed sphere should have no boundary edges"
    );
}
