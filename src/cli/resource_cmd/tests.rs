use super::*;

// ── create ──────────────────────────────────────────────────────────────────

#[test]
fn create_basic_resource() {
    let result = create::generate_resource("Resource");
    assert!(result.contains("[gd_resource type=\"Resource\" format=3]"));
    assert!(result.contains("[resource]"));
    assert!(result.ends_with('\n'));
}

#[test]
fn create_resource_with_script() {
    let result = create::generate_resource_with_script("Resource", "res://item_data.gd");
    assert!(result.contains("load_steps=2"));
    assert!(result.contains(r#"path="res://item_data.gd""#));
    assert!(result.contains(r#"script = ExtResource("1")"#));
    assert!(result.contains("[resource]"));
}

#[test]
fn create_typed_resource() {
    let result = create::generate_resource("Theme");
    assert!(result.contains("[gd_resource type=\"Theme\" format=3]"));
}

// ── set_property ────────────────────────────────────────────────────────────

#[test]
fn set_property_new() {
    let source = "[gd_resource type=\"Resource\" format=3]\n\n[resource]\n";
    let result = set_property::apply_set_property(source, "cost", "100").unwrap();
    assert!(result.contains("cost = 100"));
    assert!(result.contains("[resource]"));
}

#[test]
fn set_property_update_existing() {
    let source =
        "[gd_resource type=\"Resource\" format=3]\n\n[resource]\ncost = 50\nname = \"sword\"\n";
    let result = set_property::apply_set_property(source, "cost", "100").unwrap();
    assert!(result.contains("cost = 100"));
    assert!(!result.contains("cost = 50"));
    assert!(result.contains("name = \"sword\""));
}

#[test]
fn set_property_preserves_other_properties() {
    let source = "[gd_resource type=\"Resource\" format=3]\n\n[resource]\nalpha = 1\nbeta = 2\n";
    let result = set_property::apply_set_property(source, "gamma", "3").unwrap();
    assert!(result.contains("alpha = 1"));
    assert!(result.contains("beta = 2"));
    assert!(result.contains("gamma = 3"));
}

#[test]
fn set_property_no_resource_section() {
    let source = "[gd_resource type=\"Resource\" format=3]\n";
    let result = set_property::apply_set_property(source, "key", "val");
    assert!(result.is_err());
}

// ── get_property (via parsed data) ──────────────────────────────────────────

#[test]
fn parse_resource_properties() {
    let source =
        "[gd_resource type=\"Resource\" format=3]\n\n[resource]\ncost = 50\nname = \"sword\"\n";
    let data = scene::parse_tres(source).unwrap();
    assert_eq!(data.properties.len(), 2);
    assert_eq!(data.properties[0], ("cost".to_string(), "50".to_string()));
    assert_eq!(
        data.properties[1],
        ("name".to_string(), "\"sword\"".to_string())
    );
}

#[test]
fn parse_resource_no_properties() {
    let source = "[gd_resource type=\"Resource\" format=3]\n\n[resource]\n";
    let data = scene::parse_tres(source).unwrap();
    assert!(data.properties.is_empty());
}

// ── remove_property ─────────────────────────────────────────────────────────

#[test]
fn remove_property_basic() {
    let source =
        "[gd_resource type=\"Resource\" format=3]\n\n[resource]\ncost = 50\nname = \"sword\"\n";
    let result = remove_property::apply_remove_property(source, "cost").unwrap();
    assert!(!result.contains("cost"));
    assert!(result.contains("name = \"sword\""));
}

#[test]
fn remove_property_not_found() {
    let source = "[gd_resource type=\"Resource\" format=3]\n\n[resource]\ncost = 50\n";
    let result = remove_property::apply_remove_property(source, "nonexistent");
    assert!(result.is_err());
}

#[test]
fn remove_property_only_property() {
    let source = "[gd_resource type=\"Resource\" format=3]\n\n[resource]\ncost = 50\n";
    let result = remove_property::apply_remove_property(source, "cost").unwrap();
    assert!(!result.contains("cost"));
    assert!(result.contains("[resource]"));
}

// ── set_script ──────────────────────────────────────────────────────────────

#[test]
fn set_script_new() {
    let source = "[gd_resource type=\"Resource\" format=3]\n\n[resource]\ncost = 50\n";
    let data = scene::parse_tres(source).unwrap();
    let result = set_script::apply_set_script(source, &data, "res://item.gd").unwrap();

    assert!(result.contains("load_steps=2"));
    assert!(result.contains(r#"path="res://item.gd""#));
    assert!(result.contains("script = ExtResource("));
    assert!(result.contains("cost = 50"));
}

#[test]
fn set_script_replace_existing() {
    let source = "[gd_resource type=\"Resource\" load_steps=2 format=3]\n\n\
                  [ext_resource type=\"Script\" path=\"res://old.gd\" id=\"1\"]\n\n\
                  [resource]\nscript = ExtResource(\"1\")\ncost = 50\n";
    let data = scene::parse_tres(source).unwrap();
    let result = set_script::apply_set_script(source, &data, "res://new.gd").unwrap();

    assert!(result.contains(r#"path="res://new.gd""#));
    assert!(!result.contains("res://old.gd"));
    assert!(result.contains("cost = 50"));
}

#[test]
fn set_script_inserts_load_steps_if_missing() {
    let source = "[gd_resource type=\"Resource\" format=3]\n\n[resource]\n";
    let data = scene::parse_tres(source).unwrap();
    let result = set_script::apply_set_script(source, &data, "res://test.gd").unwrap();

    assert!(result.contains("load_steps=2"));
    assert!(result.contains("format=3"));
}

// ── remove_script ───────────────────────────────────────────────────────────

#[test]
fn remove_script_basic() {
    let source = "[gd_resource type=\"Resource\" load_steps=2 format=3]\n\n\
                  [ext_resource type=\"Script\" path=\"res://item.gd\" id=\"1\"]\n\n\
                  [resource]\nscript = ExtResource(\"1\")\ncost = 50\n";
    let result = remove_script::apply_remove_script(source, "1");

    assert!(!result.contains("script ="));
    assert!(!result.contains("[ext_resource"));
    assert!(result.contains("load_steps=1"));
    assert!(result.contains("cost = 50"));
}

#[test]
fn remove_script_keeps_other_ext_resources() {
    let source = "[gd_resource type=\"Theme\" load_steps=3 format=3]\n\n\
                  [ext_resource type=\"FontFile\" path=\"res://font.ttf\" id=\"1\"]\n\n\
                  [ext_resource type=\"Script\" path=\"res://theme.gd\" id=\"2\"]\n\n\
                  [resource]\nscript = ExtResource(\"2\")\ndefault_font = ExtResource(\"1\")\n";
    let result = remove_script::apply_remove_script(source, "2");

    assert!(!result.contains("script ="));
    assert!(!result.contains("res://theme.gd"));
    assert!(result.contains("res://font.ttf"));
    assert!(result.contains("load_steps=2"));
    assert!(result.contains("default_font = ExtResource(\"1\")"));
}

// ── info (via parse_tres) ───────────────────────────────────────────────────

#[test]
fn parse_tres_with_ext_resources() {
    let source = "[gd_resource type=\"Theme\" format=3]\n\n\
                  [ext_resource type=\"FontFile\" path=\"res://font.ttf\" id=\"1_font\"]\n\n\
                  [resource]\ndefault_font_size = 16\n";
    let data = scene::parse_tres(source).unwrap();
    assert_eq!(data.type_name, "Theme");
    assert_eq!(data.ext_resources.len(), 1);
    assert_eq!(data.ext_resources[0].id, "1_font");
    assert_eq!(data.properties.len(), 1);
}

#[test]
fn parse_tres_with_sub_resources() {
    let source = "[gd_resource type=\"Theme\" format=3]\n\n\
                  [sub_resource type=\"FontVariation\" id=\"FontVar_1\"]\n\
                  variation_embolden = 0.5\n\n\
                  [resource]\ndefault_font = SubResource(\"FontVar_1\")\n";
    let data = scene::parse_tres(source).unwrap();
    assert_eq!(data.sub_resources.len(), 1);
    assert_eq!(data.sub_resources[0].type_name, "FontVariation");
}
