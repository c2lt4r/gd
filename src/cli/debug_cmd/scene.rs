use miette::{Result, miette};
use owo_colors::OwoColorize;

use super::args::{CameraViewArgs, InspectArgs, InspectObjectsArgs, OutputFormat, SceneTreeArgs};
use super::{daemon_cmd, daemon_cmd_timeout, ensure_binary_debug};

// ── One-shot: scene-tree ─────────────────────────────────────────────

pub(crate) fn cmd_scene_tree(args: &SceneTreeArgs) -> Result<()> {
    ensure_binary_debug()?;
    let result = daemon_cmd("debug_scene_tree", serde_json::json!({}))
        .ok_or_else(|| miette!("Failed to get scene tree"))?;

    match args.format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&result).unwrap());
        }
        OutputFormat::Human => {
            println!("{}", "Scene tree:".bold());
            if let Some(nodes) = result.get("nodes").and_then(|n| n.as_array()) {
                for node in nodes {
                    print_scene_node(node, 1);
                }
            } else if let Some(nodes) = result.as_array() {
                for node in nodes {
                    print_scene_node(node, 1);
                }
            } else {
                print_scene_node(&result, 1);
            }
        }
    }
    Ok(())
}

fn print_scene_node(node: &serde_json::Value, indent: usize) {
    let name = node["name"].as_str().unwrap_or("?");
    let class = node["class_name"].as_str().unwrap_or("");
    let id = node["object_id"].as_u64().unwrap_or(0);
    let scene = node["scene_file_path"].as_str().unwrap_or("");
    let pad = "  ".repeat(indent);
    let scene_info = if scene.is_empty() {
        String::new()
    } else {
        format!(" {}", scene.dimmed())
    };
    if class.is_empty() {
        println!("{pad}{name} {}{scene_info}", format!("[id: {id}]").dimmed());
    } else {
        println!(
            "{pad}{} {} {}{scene_info}",
            name.cyan(),
            format!("({class})").dimmed(),
            format!("[id: {id}]").dimmed(),
        );
    }
    if let Some(children) = node["children"].as_array() {
        for child in children {
            print_scene_node(child, indent + 1);
        }
    }
}

// ── Variant display helper ───────────────────────────────────────────

/// Format a serialized GodotVariant JSON value for human display.
/// GodotVariant serializes as `{"type": "Int", "value": 42}`.
pub(crate) fn format_variant_display(v: &serde_json::Value) -> String {
    let Some(typ) = v.get("type").and_then(|t| t.as_str()) else {
        return if let Some(s) = v.as_str() {
            s.to_string()
        } else {
            v.to_string()
        };
    };
    let val = v.get("value");
    match typ {
        "Nil" => "null".to_string(),
        "Bool" | "Int" | "Float" => val
            .map(std::string::ToString::to_string)
            .unwrap_or_default(),
        "String" | "StringName" | "NodePath" => {
            val.and_then(|v| v.as_str()).unwrap_or("").to_string()
        }
        "Vector2" | "Vector3" | "Vector4" | "Vector2i" | "Vector3i" | "Vector4i" | "Color"
        | "Rect2" | "Rect2i" | "Transform2D" | "Basis" | "Transform3D" | "Quaternion" | "AABB"
        | "Plane" | "Projection" => {
            if let Some(arr) = val.and_then(|v| v.as_array()) {
                let parts: Vec<String> = arr.iter().map(std::string::ToString::to_string).collect();
                format!("{typ}({})", parts.join(", "))
            } else {
                val.map(std::string::ToString::to_string)
                    .unwrap_or_default()
            }
        }
        "ObjectId" => val.map(|v| format!("Object#{v}")).unwrap_or_default(),
        "Array" => {
            if let Some(arr) = val.and_then(|v| v.as_array()) {
                let parts: Vec<String> = arr.iter().map(format_variant_display).collect();
                format!("[{}]", parts.join(", "))
            } else {
                "[]".to_string()
            }
        }
        "Dictionary" => {
            // Wire format: [[key_variant, val_variant], ...]
            if let Some(pairs) = val.and_then(|v| v.as_array()) {
                let parts: Vec<String> = pairs
                    .iter()
                    .filter_map(|pair| {
                        let arr = pair.as_array()?;
                        let k = format_variant_display(arr.first()?);
                        let v = format_variant_display(arr.get(1)?);
                        Some(format!("{k}: {v}"))
                    })
                    .collect();
                format!("{{{}}}", parts.join(", "))
            } else {
                "{}".to_string()
            }
        }
        _ => val.map_or_else(|| typ.to_string(), std::string::ToString::to_string),
    }
}

// ── One-shot: inspect ───────────────────────────────────────────────

pub(crate) fn cmd_inspect(args: &InspectArgs) -> Result<()> {
    ensure_binary_debug()?;
    let result = daemon_cmd("debug_inspect", serde_json::json!({"object_id": args.id}))
        .ok_or_else(|| {
            miette!(
                "Failed to inspect object {} — is a game running with the binary debug protocol?",
                args.id
            )
        })?;

    if args.brief {
        return print_inspect_brief(&result, args.id, &args.format);
    }

    // Optionally enrich with ClassDB docs
    let result = if args.rich {
        crate::debug::enrich::enrich_inspect(&result)
    } else {
        result
    };

    match args.format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&result).unwrap());
        }
        OutputFormat::Human => {
            let class = result["class_name"].as_str().unwrap_or("Object");
            println!(
                "{} {}",
                class.cyan().bold(),
                format!("(id: {})", args.id).dimmed(),
            );
            // Show class docs if enriched
            if let Some(class_docs) = result.get("class_docs") {
                if let Some(brief) = class_docs["brief"].as_str() {
                    println!("  {}", brief.dimmed());
                }
                if let Some(url) = class_docs["docs_url"].as_str() {
                    println!("  {}", url.dimmed());
                }
            }
            println!("{}", "Properties:".bold());
            if let Some(props) = result["properties"].as_array() {
                if props.is_empty() {
                    println!("  {}", "(none)".dimmed());
                }
                for p in props {
                    let pname = p["name"].as_str().unwrap_or("?");
                    let pval = format_variant_display(&p["value"]);
                    if let Some(docs) = p.get("docs") {
                        let doc_brief = docs["brief"].as_str().unwrap_or("");
                        println!(
                            "  {} = {}  {}",
                            pname.cyan(),
                            pval.green(),
                            doc_brief.dimmed()
                        );
                    } else {
                        println!("  {} = {}", pname.cyan(), pval.green());
                    }
                }
            } else {
                println!("  {}", "(no properties returned)".dimmed());
            }
        }
    }
    Ok(())
}

/// Properties to hide in --brief mode (Godot internals, not useful for debugging).
/// Uses usage flags: bit 1 (PROPERTY_USAGE_EDITOR) = 2, bit 13 (PROPERTY_USAGE_INTERNAL) = 8192
const BRIEF_HIDDEN_PROPS: &[&str] = &[
    "script",
    "owner",
    "multiplayer",
    "process_mode",
    "process_priority",
    "process_physics_priority",
    "process_thread_group",
    "process_thread_group_order",
    "process_thread_messages",
    "physics_interpolation_mode",
    "auto_translate_mode",
    "editor_description",
    "unique_name_in_owner",
];

/// Print inspect output in brief mode: just {name: value} pairs, no Godot internals.
#[allow(clippy::unnecessary_wraps)]
fn print_inspect_brief(result: &serde_json::Value, id: u64, format: &OutputFormat) -> Result<()> {
    let props = result["properties"].as_array();
    match format {
        OutputFormat::Json => {
            let mut brief = serde_json::Map::new();
            brief.insert(
                "object_id".to_string(),
                serde_json::Value::Number(id.into()),
            );
            brief.insert("class_name".to_string(), result["class_name"].clone());
            let mut members = serde_json::Map::new();
            if let Some(props) = props {
                for p in props {
                    let name = p["name"].as_str().unwrap_or("?");
                    if BRIEF_HIDDEN_PROPS.contains(&name) {
                        continue;
                    }
                    members.insert(name.to_string(), p["value"].clone());
                }
            }
            brief.insert("properties".to_string(), serde_json::Value::Object(members));
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::Value::Object(brief)).unwrap()
            );
        }
        OutputFormat::Human => {
            let class = result["class_name"].as_str().unwrap_or("Object");
            println!("{} {}", class.cyan().bold(), format!("(id: {id})").dimmed(),);
            if let Some(props) = props {
                for p in props {
                    let pname = p["name"].as_str().unwrap_or("?");
                    if BRIEF_HIDDEN_PROPS.contains(&pname) {
                        continue;
                    }
                    let pval = format_variant_display(&p["value"]);
                    println!("  {} = {}", pname.cyan(), pval.green());
                }
            }
        }
    }
    Ok(())
}

// ── Multi-object inspection (binary protocol) ───────────────────────

pub(crate) fn cmd_inspect_objects(args: &InspectObjectsArgs) -> Result<()> {
    ensure_binary_debug()?;
    let result = daemon_cmd(
        "debug_inspect_objects",
        serde_json::json!({"ids": args.id, "selection": args.selection}),
    )
    .ok_or_else(|| miette!("Failed — is a game running?"))?;
    match args.format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&result).unwrap());
        }
        OutputFormat::Human => {
            let objects = result.as_array().map_or(&[][..], std::vec::Vec::as_slice);
            for obj in objects {
                let class = obj["class_name"].as_str().unwrap_or("Object");
                let oid = obj["object_id"].as_u64().unwrap_or(0);
                println!(
                    "{} {}",
                    class.cyan().bold(),
                    format!("(id: {oid})").dimmed(),
                );
                println!("{}", "Properties:".bold());
                if let Some(props) = obj["properties"].as_array() {
                    if props.is_empty() {
                        println!("  {}", "(none)".dimmed());
                    }
                    for p in props {
                        let pname = p["name"].as_str().unwrap_or("?");
                        let pval = format_variant_display(&p["value"]);
                        println!("  {} = {}", pname.cyan(), pval.green());
                    }
                } else {
                    println!("  {}", "(no properties returned)".dimmed());
                }
                println!();
            }
            if objects.is_empty() {
                println!("{}", "(no objects returned)".dimmed());
            }
        }
    }
    Ok(())
}

// ── Camera view: structured spatial data ─────────────────────────────
//
// Alternative approach not yet implemented: inject a temporary GDScript via
// reload-scripts that collects spatial data engine-side (frustum culling,
// physics layer info, etc). Would give true visibility data but is more
// invasive — modifies the project filesystem and risks game state changes.
// The current client-side batch approach (scene-tree + batch inspect) is
// non-invasive and sufficient for most AI debugging workflows.

#[allow(clippy::too_many_lines)]
pub(crate) fn cmd_camera_view(args: &CameraViewArgs) -> Result<()> {
    /// Check if a class is a known spatial type via the engine class DB.
    fn is_spatial_engine_class(class: &str) -> bool {
        class == "Node3D"
            || class == "Node2D"
            || crate::class_db::inherits(class, "Node3D")
            || crate::class_db::inherits(class, "Node2D")
    }

    /// Check if a class (engine name or script path) looks like a camera.
    /// Script paths like "res://scripts/player_camera.gd" use case-insensitive match.
    fn is_camera_class(class: &str, node_name: &str) -> bool {
        class == "Camera3D"
            || class == "Camera2D"
            || crate::class_db::inherits(class, "Camera3D")
            || crate::class_db::inherits(class, "Camera2D")
            || class.to_ascii_lowercase().contains("camera")
            || node_name.to_ascii_lowercase().contains("camera")
    }

    /// Script paths (res://...) aren't in class_db so we can't determine
    /// inheritance. Include them as spatial candidates — they'll be filtered
    /// after inspection based on whether they actually have transform properties.
    fn is_script_class(class: &str) -> bool {
        class.starts_with("res://")
    }

    fn walk_tree(
        node: &serde_json::Value,
        spatial_ids: &mut Vec<(u64, String, String)>,
        camera_ids: &mut Vec<(u64, String, String)>,
    ) {
        let name = node["name"].as_str().unwrap_or("").to_string();
        let class = node["class_name"].as_str().unwrap_or("").to_string();
        let id = node["object_id"].as_u64().unwrap_or(0);
        if id != 0 && !class.is_empty() {
            let camera = is_camera_class(&class, &name);
            if camera {
                camera_ids.push((id, name.clone(), class.clone()));
            }
            if is_spatial_engine_class(&class) || is_script_class(&class) || camera {
                spatial_ids.push((id, name, class));
            }
        }
        if let Some(children) = node["children"].as_array() {
            for child in children {
                walk_tree(child, spatial_ids, camera_ids);
            }
        }
    }

    ensure_binary_debug()?;

    // Step 1: Get the scene tree
    let tree = daemon_cmd("debug_scene_tree", serde_json::json!({}))
        .ok_or_else(|| miette!("Failed to get scene tree — is a game running?"))?;

    // Step 2: Collect all spatial node IDs and find camera nodes
    let mut spatial_ids: Vec<(u64, String, String)> = Vec::new(); // (id, name, class)
    let mut camera_ids: Vec<(u64, String, String)> = Vec::new();

    // The tree may be a single root node or an array of nodes
    if let Some(nodes) = tree.get("nodes").and_then(|n| n.as_array()) {
        for node in nodes {
            walk_tree(node, &mut spatial_ids, &mut camera_ids);
        }
    } else if let Some(nodes) = tree.as_array() {
        for node in nodes {
            walk_tree(node, &mut spatial_ids, &mut camera_ids);
        }
    } else {
        walk_tree(&tree, &mut spatial_ids, &mut camera_ids);
    }

    if spatial_ids.is_empty() {
        return Err(miette!("No spatial nodes found in the scene tree"));
    }

    // Step 3: Batch inspect all spatial nodes
    let all_ids: Vec<u64> = spatial_ids.iter().map(|(id, _, _)| *id).collect();
    // Scale timeout: ~0.5s per node + 5s base, capped at 60s
    let inspect_timeout = (all_ids.len() as u64 / 2 + 5).min(60);
    let inspect_result = daemon_cmd_timeout(
        "debug_inspect_objects",
        serde_json::json!({"ids": all_ids, "selection": false}),
        inspect_timeout,
    )
    .ok_or_else(|| miette!("Failed to batch inspect spatial nodes"))?;

    let inspected = inspect_result
        .as_array()
        .map_or(&[][..], std::vec::Vec::as_slice);

    // Build lookup by object_id (responses may arrive out-of-order or be partial)
    let mut inspect_by_id: std::collections::HashMap<u64, &serde_json::Value> =
        std::collections::HashMap::new();
    for obj in inspected {
        if let Some(oid) = obj["object_id"].as_u64() {
            inspect_by_id.insert(oid, obj);
        }
    }

    // Step 4: Extract spatial properties from each inspected node
    let spatial_props = [
        "position",
        "global_position",
        "rotation",
        "rotation_degrees",
        "scale",
    ];
    let camera_props = ["fov", "size", "near", "far", "current", "projection"];

    let mut nodes_out: Vec<serde_json::Value> = Vec::new();
    let mut camera_out: Option<serde_json::Value> = None;

    for (id, name, class) in &spatial_ids {
        let obj = inspect_by_id.get(id);
        let mut node_data = serde_json::json!({
            "name": name,
            "class": class,
            "object_id": id,
        });

        let mut has_spatial = false;
        if let Some(obj) = obj
            && let Some(props) = obj["properties"].as_array()
        {
            let is_camera = camera_ids.iter().any(|(cid, _, _)| cid == id);
            for p in props {
                let pname = p["name"].as_str().unwrap_or("");
                if spatial_props.contains(&pname) {
                    node_data[pname] = format_spatial_value(&p["value"]);
                    has_spatial = true;
                } else if is_camera && camera_props.contains(&pname) {
                    node_data[pname] = format_spatial_value(&p["value"]);
                }
            }
        }

        // Script classes were included speculatively — drop if no spatial props
        if !has_spatial && is_script_class(class) {
            // Still check if it's a camera (cameras are useful even without transforms)
            if !camera_ids.iter().any(|(cid, _, _)| cid == id) {
                continue;
            }
        }

        // If this is a camera, also store as the camera info
        if camera_ids.iter().any(|(cid, _, _)| cid == id) {
            camera_out = Some(node_data.clone());
        }

        nodes_out.push(node_data);
    }

    let output = serde_json::json!({
        "camera": camera_out,
        "node_count": nodes_out.len(),
        "nodes": nodes_out,
    });

    match args.format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&output).unwrap());
        }
        OutputFormat::Human => {
            if let Some(cam) = &camera_out {
                let cam_name = cam["name"].as_str().unwrap_or("?");
                let cam_class = cam["class"].as_str().unwrap_or("?");
                println!(
                    "{} {} {}",
                    "Camera:".bold(),
                    cam_name.cyan(),
                    format!("({cam_class})").dimmed(),
                );
                if let Some(pos) = cam.get("global_position") {
                    println!("  position: {}", format!("{pos}").green());
                }
                if let Some(rot) = cam.get("rotation_degrees").or_else(|| cam.get("rotation")) {
                    println!("  rotation: {}", format!("{rot}").green());
                }
                if let Some(fov) = cam.get("fov") {
                    println!("  fov: {}", format!("{fov}").green());
                }
                println!();
            } else {
                println!("{}", "No camera found in scene".dimmed());
                println!();
            }
            println!("{} ({} spatial nodes)", "Nodes:".bold(), nodes_out.len());
            for node in &nodes_out {
                let name = node["name"].as_str().unwrap_or("?");
                let class = node["class"].as_str().unwrap_or("?");
                let pos = node.get("global_position").or_else(|| node.get("position"));
                let rot = node
                    .get("rotation_degrees")
                    .or_else(|| node.get("rotation"));
                let pos_str = pos.map_or_else(|| "?".to_string(), |v| format!("{v}"));
                let rot_str = rot.map_or_else(|| "?".to_string(), |v| format!("{v}"));
                println!(
                    "  {} {} pos={} rot={}",
                    name.cyan(),
                    format!("({class})").dimmed(),
                    pos_str.green(),
                    rot_str.green(),
                );
            }
        }
    }
    Ok(())
}

/// Format a variant value for spatial display (simplify vectors to arrays).
fn format_spatial_value(value: &serde_json::Value) -> serde_json::Value {
    // Godot variants come as {"Vector3": [x,y,z]} — flatten to just [x,y,z]
    if let Some(obj) = value.as_object()
        && obj.len() == 1
        && let Some(inner) = obj.values().next()
        && inner.is_array()
    {
        return inner.clone();
    }
    value.clone()
}
