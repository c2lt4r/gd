use miette::{Result, miette};
use serde::Serialize;
use std::path::Path;
use tree_sitter::Node;

use super::resource_parser;

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct ExtResource {
    pub id: String,
    pub type_name: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uid: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SubResource {
    pub id: String,
    pub type_name: String,
    pub properties: Vec<(String, String)>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SceneNode {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub type_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instance: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub script: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub properties: Vec<(String, String)>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Connection {
    pub signal: String,
    pub from: String,
    pub to: String,
    pub method: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SceneData {
    pub ext_resources: Vec<ExtResource>,
    pub sub_resources: Vec<SubResource>,
    pub nodes: Vec<SceneNode>,
    pub connections: Vec<Connection>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ResourceData {
    pub type_name: String,
    pub ext_resources: Vec<ExtResource>,
    pub sub_resources: Vec<SubResource>,
    pub properties: Vec<(String, String)>,
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

/// Parse a .tscn file and extract structured scene data.
pub fn parse_scene(source: &str) -> Result<SceneData> {
    // Normalize &" → " so tree-sitter offsets match the text we extract from
    let normalized = resource_parser::normalize_for_extraction(source);
    let tree = resource_parser::parse_resource(&normalized)?;
    let root = tree.root_node();
    let src = normalized.as_bytes();

    let mut data = SceneData {
        ext_resources: Vec::new(),
        sub_resources: Vec::new(),
        nodes: Vec::new(),
        connections: Vec::new(),
    };

    let mut cursor = root.walk();
    for child in root.named_children(&mut cursor) {
        if child.kind() != "section" {
            continue;
        }
        let section_type = section_identifier(&child, src);
        match section_type.as_deref() {
            Some("ext_resource") => {
                if let Some(ext) = parse_ext_resource(&child, src) {
                    data.ext_resources.push(ext);
                }
            }
            Some("sub_resource") => {
                data.sub_resources.push(parse_sub_resource(&child, src));
            }
            Some("node") => {
                data.nodes.push(parse_node(&child, src));
            }
            Some("connection") => {
                if let Some(conn) = parse_connection(&child, src) {
                    data.connections.push(conn);
                }
            }
            _ => {}
        }
    }

    Ok(data)
}

/// Parse a .tres file and extract structured resource data.
pub fn parse_tres(source: &str) -> Result<ResourceData> {
    // Normalize &" → " so tree-sitter offsets match the text we extract from
    let normalized = resource_parser::normalize_for_extraction(source);
    let tree = resource_parser::parse_resource(&normalized)?;
    let root = tree.root_node();
    let src = normalized.as_bytes();

    let mut data = ResourceData {
        type_name: String::new(),
        ext_resources: Vec::new(),
        sub_resources: Vec::new(),
        properties: Vec::new(),
    };

    let mut cursor = root.walk();
    for child in root.named_children(&mut cursor) {
        if child.kind() != "section" {
            continue;
        }
        let section_type = section_identifier(&child, src);
        match section_type.as_deref() {
            Some("gd_resource") => {
                let attrs = collect_attributes(&child, src);
                data.type_name = attr_value(&attrs, "type").unwrap_or_default();
            }
            Some("ext_resource") => {
                if let Some(ext) = parse_ext_resource(&child, src) {
                    data.ext_resources.push(ext);
                }
            }
            Some("sub_resource") => {
                data.sub_resources.push(parse_sub_resource(&child, src));
            }
            Some("resource") => {
                data.properties = collect_properties(&child, src);
            }
            _ => {}
        }
    }

    Ok(data)
}

/// Parse a .tscn file from disk.
pub fn parse_scene_file(path: &Path) -> Result<SceneData> {
    let source = std::fs::read_to_string(path)
        .map_err(|e| miette!("Failed to read {}: {e}", path.display()))?;
    parse_scene(&source)
}

/// Parse a .tres file from disk.
pub fn parse_tres_file(path: &Path) -> Result<ResourceData> {
    let source = std::fs::read_to_string(path)
        .map_err(|e| miette!("Failed to read {}: {e}", path.display()))?;
    parse_tres(&source)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Get the section type identifier (e.g., "ext_resource", "node").
fn section_identifier(section: &Node, src: &[u8]) -> Option<String> {
    let mut cursor = section.walk();
    for child in section.named_children(&mut cursor) {
        if child.kind() == "identifier" {
            return child
                .utf8_text(src)
                .ok()
                .map(std::string::ToString::to_string);
        }
    }
    None
}

/// Collect all attributes from a section header as (name, value) pairs.
fn collect_attributes(section: &Node, src: &[u8]) -> Vec<(String, String)> {
    let mut attrs = Vec::new();
    let mut cursor = section.walk();
    for child in section.named_children(&mut cursor) {
        if child.kind() == "attribute"
            && let (Some(name_node), Some(value_node)) =
                (child.named_child(0), child.named_child(1))
        {
            let name = node_text(&name_node, src);
            let value = unquote(&node_text(&value_node, src));
            attrs.push((name, value));
        }
    }
    attrs
}

/// Collect all properties from a section body as (name, raw_value) pairs.
fn collect_properties(section: &Node, src: &[u8]) -> Vec<(String, String)> {
    let mut props = Vec::new();
    let mut cursor = section.walk();
    for child in section.named_children(&mut cursor) {
        if child.kind() == "property"
            && let (Some(name_node), Some(value_node)) =
                (child.named_child(0), child.named_child(1))
        {
            let name = node_text(&name_node, src);
            let value = node_text(&value_node, src);
            props.push((name, value));
        }
    }
    props
}

/// Find an attribute value by name.
fn attr_value(attrs: &[(String, String)], name: &str) -> Option<String> {
    attrs
        .iter()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v.clone())
}

/// Parse an ext_resource section into an ExtResource.
fn parse_ext_resource(section: &Node, src: &[u8]) -> Option<ExtResource> {
    let attrs = collect_attributes(section, src);
    let id = attr_value(&attrs, "id")?;
    let type_name = attr_value(&attrs, "type").unwrap_or_default();
    let path = attr_value(&attrs, "path").unwrap_or_default();
    let uid = attr_value(&attrs, "uid");
    Some(ExtResource {
        id,
        type_name,
        path,
        uid,
    })
}

/// Parse a sub_resource section into a SubResource.
fn parse_sub_resource(section: &Node, src: &[u8]) -> SubResource {
    let attrs = collect_attributes(section, src);
    SubResource {
        id: attr_value(&attrs, "id").unwrap_or_default(),
        type_name: attr_value(&attrs, "type").unwrap_or_default(),
        properties: collect_properties(section, src),
    }
}

/// Parse a node section into a SceneNode.
fn parse_node(section: &Node, src: &[u8]) -> SceneNode {
    let attrs = collect_attributes(section, src);
    let props = collect_properties(section, src);

    // Detect script — look for `script = ExtResource("id")` in properties
    let script = props
        .iter()
        .find(|(k, _)| k == "script")
        .map(|(_, v)| v.clone());

    // Detect groups — look for `groups` attribute in section header
    let groups = attr_value(&attrs, "groups")
        .map(|g| parse_groups_array(&g))
        .unwrap_or_default();

    // Detect instance — look for `instance` attribute
    let instance = attr_value(&attrs, "instance");

    SceneNode {
        name: attr_value(&attrs, "name").unwrap_or_default(),
        type_name: attr_value(&attrs, "type"),
        parent: attr_value(&attrs, "parent"),
        instance,
        script,
        groups,
        properties: props,
    }
}

/// Parse a connection section into a Connection.
fn parse_connection(section: &Node, src: &[u8]) -> Option<Connection> {
    let attrs = collect_attributes(section, src);
    Some(Connection {
        signal: attr_value(&attrs, "signal")?,
        from: attr_value(&attrs, "from")?,
        to: attr_value(&attrs, "to")?,
        method: attr_value(&attrs, "method")?,
    })
}

/// Get text content of a node.
fn node_text(node: &Node, src: &[u8]) -> String {
    node.utf8_text(src).unwrap_or("").to_string()
}

/// Remove surrounding quotes from a string value.
fn unquote(s: &str) -> String {
    s.trim_matches('"').trim_matches('\'').to_string()
}

/// Parse a groups array like `["group1", "group2"]` into a Vec<String>.
fn parse_groups_array(s: &str) -> Vec<String> {
    let trimmed = s.trim();
    if !trimmed.starts_with('[') || !trimmed.ends_with(']') {
        return Vec::new();
    }
    let inner = &trimmed[1..trimmed.len() - 1];
    inner
        .split(',')
        .map(|part| part.trim().trim_matches('"').trim_matches('&').to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

// ---------------------------------------------------------------------------
// Utilities for other modules
// ---------------------------------------------------------------------------

/// Resolve a `res://` path relative to the project root.
pub fn resolve_res_path(res_path: &str, project_root: &Path) -> Option<std::path::PathBuf> {
    let stripped = res_path.strip_prefix("res://")?;
    let resolved = project_root.join(stripped);
    Some(resolved)
}

/// Check if an ext_resource id is referenced anywhere in the scene.
pub fn is_ext_resource_referenced(id: &str, data: &SceneData) -> bool {
    let pattern = format!("ExtResource(\"{id}\")");
    let pattern_alt = format!("ExtResource( \"{id}\" )");

    let matches_pattern = |value: &str| value.contains(&pattern) || value.contains(&pattern_alt);

    // Check node properties and attributes (script, instance)
    for node in &data.nodes {
        if node.script.as_deref().is_some_and(&matches_pattern) {
            return true;
        }
        if node.instance.as_deref().is_some_and(&matches_pattern) {
            return true;
        }
        for (_, value) in &node.properties {
            if matches_pattern(value) {
                return true;
            }
        }
    }

    // Check sub_resource properties
    for sub in &data.sub_resources {
        for (_, value) in &sub.properties {
            if matches_pattern(value) {
                return true;
            }
        }
    }

    false
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_TSCN: &str = r#"[gd_scene load_steps=3 format=3 uid="uid://abc123"]

[ext_resource type="Script" path="res://player.gd" id="1_abc"]

[ext_resource type="PackedScene" path="res://weapon.tscn" id="2_def"]

[sub_resource type="CapsuleShape3D" id="CapsuleShape3D_xyz"]
radius = 0.5
height = 1.8

[node name="Player" type="CharacterBody3D"]
script = ExtResource("1_abc")

[node name="CollisionShape" type="CollisionShape3D" parent="."]
shape = SubResource("CapsuleShape3D_xyz")

[node name="Weapon" parent="." instance=ExtResource("2_def")]

[connection signal="body_entered" from="Area3D" to="." method="_on_body_entered"]
"#;

    const SAMPLE_TRES: &str = r#"[gd_resource type="Theme" format=3 uid="uid://theme1"]

[ext_resource type="FontFile" path="res://fonts/main.ttf" id="1_font"]

[sub_resource type="FontVariation" id="FontVariation_abc"]
base_font = ExtResource("1_font")
variation_embolden = 0.5

[resource]
default_font = SubResource("FontVariation_abc")
default_font_size = 16
"#;

    #[test]
    fn parse_scene_ext_resources() {
        let data = parse_scene(SAMPLE_TSCN).unwrap();
        assert_eq!(data.ext_resources.len(), 2);
        assert_eq!(data.ext_resources[0].id, "1_abc");
        assert_eq!(data.ext_resources[0].type_name, "Script");
        assert_eq!(data.ext_resources[0].path, "res://player.gd");
        assert_eq!(data.ext_resources[1].type_name, "PackedScene");
    }

    #[test]
    fn parse_scene_sub_resources() {
        let data = parse_scene(SAMPLE_TSCN).unwrap();
        assert_eq!(data.sub_resources.len(), 1);
        assert_eq!(data.sub_resources[0].type_name, "CapsuleShape3D");
        assert_eq!(data.sub_resources[0].properties.len(), 2);
    }

    #[test]
    fn parse_scene_nodes() {
        let data = parse_scene(SAMPLE_TSCN).unwrap();
        assert_eq!(data.nodes.len(), 3);

        // Root node
        assert_eq!(data.nodes[0].name, "Player");
        assert_eq!(data.nodes[0].type_name.as_deref(), Some("CharacterBody3D"));
        assert!(data.nodes[0].parent.is_none());
        assert!(data.nodes[0].script.is_some());

        // Child node
        assert_eq!(data.nodes[1].name, "CollisionShape");
        assert_eq!(data.nodes[1].parent.as_deref(), Some("."));

        // Instance node
        assert_eq!(data.nodes[2].name, "Weapon");
        assert!(data.nodes[2].instance.is_some());
    }

    #[test]
    fn parse_scene_connections() {
        let data = parse_scene(SAMPLE_TSCN).unwrap();
        assert_eq!(data.connections.len(), 1);
        assert_eq!(data.connections[0].signal, "body_entered");
        assert_eq!(data.connections[0].from, "Area3D");
        assert_eq!(data.connections[0].to, ".");
        assert_eq!(data.connections[0].method, "_on_body_entered");
    }

    #[test]
    fn parse_tres_resource() {
        let data = parse_tres(SAMPLE_TRES).unwrap();
        assert_eq!(data.type_name, "Theme");
        assert_eq!(data.ext_resources.len(), 1);
        assert_eq!(data.sub_resources.len(), 1);
        assert_eq!(data.properties.len(), 2);
    }

    #[test]
    fn ext_resource_referenced() {
        let data = parse_scene(SAMPLE_TSCN).unwrap();
        assert!(is_ext_resource_referenced("1_abc", &data));
        assert!(is_ext_resource_referenced("2_def", &data));
    }

    #[test]
    fn ext_resource_not_referenced() {
        // Manually create scene data with unreferenced ext_resource
        let source = r#"[gd_scene format=3]

[ext_resource type="Texture2D" path="res://icon.png" id="unused_1"]

[node name="Root" type="Node2D"]
"#;
        let data = parse_scene(source).unwrap();
        assert!(!is_ext_resource_referenced("unused_1", &data));
    }

    #[test]
    fn resolve_res_path_basic() {
        let root = Path::new("/project");
        let resolved = resolve_res_path("res://scripts/player.gd", root).unwrap();
        assert_eq!(resolved, Path::new("/project/scripts/player.gd"));
    }

    #[test]
    fn resolve_res_path_invalid() {
        let root = Path::new("/project");
        assert!(resolve_res_path("invalid/path.gd", root).is_none());
    }

    #[test]
    fn parse_groups_array_values() {
        let groups = parse_groups_array(r#"["enemy", "damageable"]"#);
        assert_eq!(groups, vec!["enemy", "damageable"]);
    }

    #[test]
    fn parse_groups_array_empty() {
        assert!(parse_groups_array("[]").is_empty());
        assert!(parse_groups_array("not_array").is_empty());
    }
}
