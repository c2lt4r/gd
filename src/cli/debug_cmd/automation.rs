use std::time::{Duration, Instant};

use miette::{Result, miette};
use owo_colors::OwoColorize;

use super::args::{
    AwaitArgs, CallArgs, DescribeArgs, DragArgs, FindArgs, GetPropArgs, HoverArgs, MoveToArgs,
    NavigateArgs, OutputFormat, SetNodeArgs,
};
use crate::core::live_eval::send_eval;
use crate::core::project::GodotProject;
use crate::cprintln;

/// Default timeout for automation eval commands.
const AUTO_TIMEOUT: Duration = Duration::from_secs(10);

/// Resolve the project root.
fn project_root() -> Result<std::path::PathBuf> {
    let cwd = std::env::current_dir().unwrap_or_default();
    let project = GodotProject::discover(&cwd)?;
    Ok(project.root)
}

/// Run a generated GDScript via live eval and return the raw result string.
fn run_eval(script: &str) -> Result<String> {
    let root = project_root()?;
    let result = send_eval(script, &root, AUTO_TIMEOUT)?.result;
    if result.starts_with("ERROR:") {
        return Err(miette!("{result}"));
    }
    Ok(result)
}

/// Run eval with a custom timeout.
fn run_eval_timeout(script: &str, timeout: Duration) -> Result<String> {
    let root = project_root()?;
    let result = send_eval(script, &root, timeout)?.result;
    if result.starts_with("ERROR:") {
        return Err(miette!("{result}"));
    }
    Ok(result)
}

// ── Shared GDScript helpers ──────────────────────────────────────────

/// Generate GDScript to look up a node by name, path, or object ID.
/// Returns a snippet that sets `var node` and returns an error string if not found.
fn node_lookup_gdscript(node: &str) -> String {
    if node.starts_with('/') {
        format!(
            "\tvar node = get_tree().get_root().get_node_or_null(\"{node}\")\n\
             \tif node == null: return \"ERROR: node '{node}' not found\""
        )
    } else {
        format!(
            "\tvar node = get_tree().get_root().find_child(\"{node}\", true, false)\n\
             \tif node == null: return \"ERROR: node '{node}' not found\""
        )
    }
}

/// Generate GDScript to look up a node by object ID.
fn node_lookup_by_id_gdscript(id: u64) -> String {
    format!(
        "\tvar node = instance_from_id({id})\n\
         \tif node == null: return \"ERROR: object ID {id} not found\""
    )
}

/// Generate GDScript to resolve the screen position of a node.
/// Assumes `var node` is already set. Sets `var pos`.
fn node_position_gdscript() -> &'static str {
    "\tvar pos = Vector2.ZERO\n\
     \tif node is Control:\n\
     \t\tpos = node.get_global_rect().get_center()\n\
     \telif node is Node2D:\n\
     \t\tpos = node.get_viewport().get_canvas_transform() * node.global_position\n\
     \telif node is Node3D:\n\
     \t\tvar cam = node.get_viewport().get_camera_3d()\n\
     \t\tif cam:\n\
     \t\t\tpos = cam.unproject_position(node.global_position)\n\
     \telse:\n\
     \t\treturn \"ERROR: node '%s' is not a spatial or control node\" % node.name"
}

/// Generate a mouse motion event script to move cursor to (x, y).
fn mouse_motion_script(x: &str, y: &str) -> String {
    format!(
        "extends Node\n\
         \n\
         func run():\n\
         \tvar pos = Vector2({x}, {y})\n\
         \tvar ev = InputEventMouseMotion.new()\n\
         \tev.position = pos\n\
         \tev.global_position = pos\n\
         \tInput.parse_input_event(ev)\n\
         \treturn \"(%s, %s)\" % [pos.x, pos.y]\n"
    )
}

/// Parse "X,Y" into (x_str, y_str), validating as f64.
fn parse_pos(pos: &str) -> Result<(String, String)> {
    let parts: Vec<&str> = pos.split(',').collect();
    if parts.len() != 2 {
        return Err(miette!(
            "Invalid position '{pos}' — expected format: X,Y (e.g. 100,200)"
        ));
    }
    let x = parts[0].trim().to_string();
    let y = parts[1].trim().to_string();
    x.parse::<f64>()
        .map_err(|_| miette!("Invalid X coordinate: {x}"))?;
    y.parse::<f64>()
        .map_err(|_| miette!("Invalid Y coordinate: {y}"))?;
    Ok((x, y))
}

// ── 0. Describe ──────────────────────────────────────────────────────

/// Generate the GDScript for `describe`. A single eval that returns a full
/// game-state snapshot: reference node, nearby nodes, scene info, input actions.
fn generate_describe_script(node_spec: Option<&str>, radius: Option<f64>) -> String {
    // If node is specified, use it; otherwise auto-detect common player names
    let find_ref = if let Some(spec) = node_spec {
        if spec.starts_with('/') {
            format!(
                "\tvar ref_node = root.get_node_or_null(\"{spec}\")\n\
                 \tif ref_node == null: return \"ERROR: node '{spec}' not found\""
            )
        } else {
            format!(
                "\tvar ref_node = root.find_child(\"{spec}\", true, false)\n\
                 \tif ref_node == null: return \"ERROR: node '{spec}' not found\""
            )
        }
    } else {
        "\tvar ref_node = null\n\
         \tfor n in [\"Player\", \"player\", \"Character\", \"character\"]:\n\
         \t\tref_node = root.find_child(n, true, false)\n\
         \t\tif ref_node: break\n\
         \tif ref_node == null: return \"ERROR: no player node found (use --node to specify)\""
            .to_string()
    };

    // Radius: use provided, or default based on 2D vs 3D detection
    let radius_expr = if let Some(r) = radius {
        format!("{r}")
    } else {
        "500.0 if ref_node is Node2D else 20.0 if ref_node is Node3D else 500.0".to_string()
    };

    format!(
        "extends Node\n\
         \n\
         func run():\n\
         \tvar root = get_tree().get_root()\n\
         {find_ref}\n\
         \tvar ref_pos = ref_node.global_position\n\
         \tvar radius = {radius_expr}\n\
         \tvar is_3d = ref_node is Node3D\n\
         \n\
         \t# Nearby nodes\n\
         \tvar nearby = []\n\
         \t_scan(root, ref_node, ref_pos, radius, is_3d, nearby)\n\
         \tnearby.sort_custom(func(a, b): return a[\"distance\"] < b[\"distance\"])\n\
         \n\
         \t# Scene info\n\
         \tvar scene_path = get_tree().current_scene.scene_file_path if get_tree().current_scene else \"unknown\"\n\
         \n\
         \t# Input actions\n\
         \tvar actions = []\n\
         \tfor act in InputMap.get_actions():\n\
         \t\tif not act.begins_with(\"ui_\"): actions.append(str(act))\n\
         \n\
         \tvar d = {{}}\n\
         \td[\"reference_node\"] = {{\"name\": ref_node.name, \"class\": ref_node.get_class(), \"position\": str(ref_pos), \"groups\": _groups(ref_node)}}\n\
         \td[\"scene\"] = scene_path\n\
         \td[\"nearby\"] = nearby\n\
         \td[\"input_actions\"] = actions\n\
         \td[\"radius\"] = radius\n\
         \treturn JSON.stringify(d)\n\
         \n\
         func _scan(node, ref_node, ref_pos, radius, is_3d, out):\n\
         \tif node == ref_node: \n\
         \t\tfor child in node.get_children():\n\
         \t\t\t_scan(child, ref_node, ref_pos, radius, is_3d, out)\n\
         \t\treturn\n\
         \tvar pos = null\n\
         \tif is_3d and node is Node3D:\n\
         \t\tpos = node.global_position\n\
         \telif not is_3d and node is Node2D:\n\
         \t\tpos = node.global_position\n\
         \telif node is Control:\n\
         \t\tpos = node.get_global_rect().get_center()\n\
         \tif pos != null:\n\
         \t\tvar dist = pos.distance_to(ref_pos)\n\
         \t\tif dist <= radius:\n\
         \t\t\tout.append({{\"name\": node.name, \"class\": node.get_class(), \"position\": str(pos), \"distance\": snappedf(dist, 0.1), \"groups\": _groups(node)}})\n\
         \tfor child in node.get_children():\n\
         \t\t_scan(child, ref_node, ref_pos, radius, is_3d, out)\n\
         \n\
         func _groups(node):\n\
         \tvar g = []\n\
         \tfor gr in node.get_groups():\n\
         \t\tg.append(str(gr))\n\
         \treturn g\n"
    )
}

pub fn cmd_describe(args: &DescribeArgs) -> Result<()> {
    let script = generate_describe_script(args.node.as_deref(), args.radius);
    let result = run_eval(&script)?;
    let parsed: serde_json::Value = serde_json::from_str(&result)
        .map_err(|e| miette!("Failed to parse describe result: {e}"))?;

    match args.format {
        OutputFormat::Json => {
            cprintln!("{}", serde_json::to_string_pretty(&parsed).unwrap());
        }
        OutputFormat::Text => {
            // Reference node
            let ref_node = &parsed["reference_node"];
            let name = ref_node["name"].as_str().unwrap_or("?");
            let cls = ref_node["class"].as_str().unwrap_or("?");
            let pos = ref_node["position"].as_str().unwrap_or("?");
            let groups = format_groups(&ref_node["groups"]);
            cprintln!(
                "{} {} at {}{groups}",
                name.green().bold(),
                cls.dimmed(),
                pos.cyan()
            );

            // Scene
            let scene = parsed["scene"].as_str().unwrap_or("?");
            cprintln!("Scene: {}", scene.dimmed());

            // Nearby
            let radius = parsed["radius"].as_f64().unwrap_or(0.0);
            let nearby = parsed["nearby"].as_array();
            if let Some(nodes) = nearby {
                cprintln!(
                    "\n{} ({} within {radius:.0}):",
                    "Nearby".bold(),
                    nodes.len()
                );
                for n in nodes {
                    let n_name = n["name"].as_str().unwrap_or("?");
                    let n_cls = n["class"].as_str().unwrap_or("?");
                    let n_dist = n["distance"].as_f64().unwrap_or(0.0);
                    let n_groups = format_groups(&n["groups"]);
                    cprintln!(
                        "  {:.0} — {} {}{n_groups}",
                        n_dist,
                        n_name.green(),
                        n_cls.dimmed()
                    );
                }
            }

            // Input actions
            if let Some(acts) = parsed["input_actions"].as_array()
                && !acts.is_empty()
            {
                let act_strs: Vec<&str> =
                    acts.iter().filter_map(serde_json::Value::as_str).collect();
                cprintln!("\n{}: {}", "Input actions".bold(), act_strs.join(", "));
            }
        }
    }
    Ok(())
}

/// Format groups array as a display string.
fn format_groups(groups: &serde_json::Value) -> String {
    let arr = match groups.as_array() {
        Some(a) if !a.is_empty() => a,
        _ => return String::new(),
    };
    let strs: Vec<&str> = arr.iter().filter_map(serde_json::Value::as_str).collect();
    if strs.is_empty() {
        String::new()
    } else {
        format!(" [{}]", strs.join(", "))
    }
}

// ── 1. Find ──────────────────────────────────────────────────────────

fn generate_find_by_name_script(name: &str) -> String {
    if name.starts_with('/') {
        // Absolute path — single node lookup
        format!(
            "extends Node\n\
             \n\
             func run():\n\
             \tvar node = get_tree().get_root().get_node_or_null(\"{name}\")\n\
             \tif node == null: return \"[]\"\n\
             \tvar path = str(node.get_path())\n\
             \tvar cls = node.get_class()\n\
             \tvar oid = node.get_instance_id()\n\
             \treturn JSON.stringify([{{\"name\": node.name, \"class\": cls, \"object_id\": oid, \"path\": path}}])\n"
        )
    } else {
        // Recursive search — may find multiple
        format!(
            "extends Node\n\
             \n\
             func run():\n\
             \tvar results = []\n\
             \tvar root = get_tree().get_root()\n\
             \t_search(root, \"{name}\", results)\n\
             \treturn JSON.stringify(results)\n\
             \n\
             func _search(node, pattern, results):\n\
             \tif node.name == pattern:\n\
             \t\tresults.append({{\"name\": node.name, \"class\": node.get_class(), \"object_id\": node.get_instance_id(), \"path\": str(node.get_path())}})\n\
             \tfor child in node.get_children():\n\
             \t\t_search(child, pattern, results)\n"
        )
    }
}

fn generate_find_by_type_script(type_name: &str) -> String {
    format!(
        "extends Node\n\
         \n\
         func run():\n\
         \tvar results = []\n\
         \tvar root = get_tree().get_root()\n\
         \t_search(root, results)\n\
         \treturn JSON.stringify(results)\n\
         \n\
         func _search(node, results):\n\
         \tif node.get_class() == \"{type_name}\" or node.is_class(\"{type_name}\"):\n\
         \t\tresults.append({{\"name\": node.name, \"class\": node.get_class(), \"object_id\": node.get_instance_id(), \"path\": str(node.get_path())}})\n\
         \tfor child in node.get_children():\n\
         \t\t_search(child, results)\n"
    )
}

fn generate_find_by_group_script(group: &str) -> String {
    format!(
        "extends Node\n\
         \n\
         func run():\n\
         \tvar results = []\n\
         \tfor node in get_tree().get_nodes_in_group(\"{group}\"):\n\
         \t\tresults.append({{\"name\": node.name, \"class\": node.get_class(), \"object_id\": node.get_instance_id(), \"path\": str(node.get_path())}})\n\
         \treturn JSON.stringify(results)\n"
    )
}

pub fn cmd_find(args: &FindArgs) -> Result<()> {
    let script = match (&args.name, &args.type_, &args.group) {
        (Some(name), None, None) => generate_find_by_name_script(name),
        (None, Some(t), None) => generate_find_by_type_script(t),
        (None, None, Some(g)) => generate_find_by_group_script(g),
        (None, None, None) => {
            return Err(miette!(
                "Specify --name <name>, --type <class>, or --group <group>"
            ));
        }
        _ => {
            return Err(miette!("Specify only one of --name, --type, or --group"));
        }
    };

    let result = run_eval(&script)?;
    let nodes: Vec<serde_json::Value> =
        serde_json::from_str(&result).map_err(|e| miette!("Failed to parse find results: {e}"))?;

    match args.format {
        OutputFormat::Json => {
            cprintln!("{}", serde_json::to_string_pretty(&nodes).unwrap());
        }
        OutputFormat::Text => {
            if nodes.is_empty() {
                cprintln!("{}", "No nodes found".yellow());
            } else {
                for node in &nodes {
                    let path = node["path"].as_str().unwrap_or("?");
                    let cls = node["class"].as_str().unwrap_or("?");
                    let oid = node["object_id"].as_u64().unwrap_or(0);
                    cprintln!("{} ({}) [id: {}]", path.green(), cls.dimmed(), oid);
                }
            }
        }
    }
    Ok(())
}

// ── 2. Get-prop ──────────────────────────────────────────────────────

fn generate_get_prop_script(lookup: &str, property: &str) -> String {
    format!(
        "extends Node\n\
         \n\
         func run():\n\
         {lookup}\n\
         \tvar val = node.get(\"{property}\")\n\
         \tvar d = {{\"node\": node.name, \"property\": \"{property}\", \"value\": val}}\n\
         \treturn JSON.stringify(d)\n"
    )
}

pub fn cmd_get_prop(args: &GetPropArgs) -> Result<()> {
    let lookup = match (&args.node, args.id) {
        (Some(node), None) => node_lookup_gdscript(node),
        (None, Some(id)) => node_lookup_by_id_gdscript(id),
        (Some(_), Some(_)) => return Err(miette!("Specify either --node or --id, not both")),
        (None, None) => return Err(miette!("Specify --node <name> or --id <N>")),
    };

    let script = generate_get_prop_script(&lookup, &args.property);
    let result = run_eval(&script)?;

    match args.format {
        OutputFormat::Json => cprintln!("{result}"),
        OutputFormat::Text => {
            let parsed: serde_json::Value = serde_json::from_str(&result)
                .map_err(|e| miette!("Failed to parse result: {e}"))?;
            let node_name = parsed["node"].as_str().unwrap_or("?");
            let value = &parsed["value"];
            cprintln!("{}.{} = {}", node_name.green(), args.property.cyan(), value);
        }
    }
    Ok(())
}

// ── 3. Call ──────────────────────────────────────────────────────────

fn generate_call_script(lookup: &str, method: &str, gdscript_args: &str) -> String {
    format!(
        "extends Node\n\
         \n\
         func run():\n\
         {lookup}\n\
         \tvar args = {gdscript_args}\n\
         \tvar result = node.callv(\"{method}\", args)\n\
         \tvar d = {{\"node\": node.name, \"method\": \"{method}\", \"result\": result}}\n\
         \treturn JSON.stringify(d)\n"
    )
}

pub fn cmd_call(args: &CallArgs) -> Result<()> {
    let lookup = match (&args.node, args.id) {
        (Some(node), None) => node_lookup_gdscript(node),
        (None, Some(id)) => node_lookup_by_id_gdscript(id),
        (Some(_), Some(_)) => return Err(miette!("Specify either --node or --id, not both")),
        (None, None) => return Err(miette!("Specify --node <name> or --id <N>")),
    };

    let script = generate_call_script(&lookup, &args.method, &args.args);
    let result = run_eval(&script)?;

    match args.format {
        OutputFormat::Json => cprintln!("{result}"),
        OutputFormat::Text => {
            let parsed: serde_json::Value = serde_json::from_str(&result)
                .map_err(|e| miette!("Failed to parse result: {e}"))?;
            let node_name = parsed["node"].as_str().unwrap_or("?");
            let ret = &parsed["result"];
            if ret.is_null() {
                cprintln!("Called {}.{}()", node_name.green(), args.method.cyan());
            } else {
                cprintln!(
                    "Called {}.{}() → {}",
                    node_name.green(),
                    args.method.cyan(),
                    ret
                );
            }
        }
    }
    Ok(())
}

// ── 4. Set ───────────────────────────────────────────────────────────

fn generate_set_script(lookup: &str, property: &str, value: &str) -> String {
    format!(
        "extends Node\n\
         \n\
         func run():\n\
         {lookup}\n\
         \tnode.set(\"{property}\", {value})\n\
         \tvar d = {{\"node\": node.name, \"property\": \"{property}\", \"value\": str({value})}}\n\
         \treturn JSON.stringify(d)\n"
    )
}

pub fn cmd_set(args: &SetNodeArgs) -> Result<()> {
    let lookup = node_lookup_gdscript(&args.node);
    let script = generate_set_script(&lookup, &args.property, &args.value);
    let result = run_eval(&script)?;

    match args.format {
        OutputFormat::Json => cprintln!("{result}"),
        OutputFormat::Text => {
            cprintln!(
                "Set {}.{} = {}",
                args.node.green(),
                args.property.cyan(),
                args.value
            );
        }
    }

    if args.screenshot {
        super::camera::cmd_screenshot(&super::args::ScreenshotArgs {
            output: None,
            format: args.format.clone(),
        })?;
    }

    Ok(())
}

// ── 5. Await ─────────────────────────────────────────────────────────

fn generate_node_exists_script(node: &str) -> String {
    let lookup = if node.starts_with('/') {
        format!("get_tree().get_root().get_node_or_null(\"{node}\") != null")
    } else {
        format!("get_tree().get_root().find_child(\"{node}\", true, false) != null")
    };
    format!(
        "extends Node\n\
         \n\
         func run():\n\
         \treturn str({lookup})\n"
    )
}

fn generate_property_read_script(node: &str, property: &str) -> String {
    let lookup = node_lookup_gdscript(node);
    format!(
        "extends Node\n\
         \n\
         func run():\n\
         {lookup}\n\
         \treturn JSON.stringify({{\"value\": node.get(\"{property}\")}})\n"
    )
}

/// Check if a JSON value matches a comparator condition.
fn check_condition(value: &serde_json::Value, op: &str, target: &str) -> bool {
    match op {
        "==" => {
            let val_str = if value.is_string() {
                value.as_str().unwrap_or("").to_string()
            } else {
                value.to_string()
            };
            val_str == target
        }
        ">" => {
            value.as_f64().unwrap_or(f64::NEG_INFINITY)
                > target.parse::<f64>().unwrap_or(f64::INFINITY)
        }
        "<" => {
            value.as_f64().unwrap_or(f64::INFINITY)
                < target.parse::<f64>().unwrap_or(f64::NEG_INFINITY)
        }
        "contains" => {
            let val_str = if value.is_string() {
                value.as_str().unwrap_or("").to_string()
            } else {
                value.to_string()
            };
            val_str.contains(target)
        }
        _ => false,
    }
}

/// Parse the comparator from await args.
fn parse_comparator(args: &AwaitArgs) -> Result<(&'static str, String)> {
    if let Some(ref eq) = args.equals {
        Ok(("==", eq.clone()))
    } else if let Some(ref gt) = args.gt {
        Ok((">", gt.clone()))
    } else if let Some(ref lt) = args.lt {
        Ok(("<", lt.clone()))
    } else if let Some(ref ct) = args.contains {
        Ok(("contains", ct.clone()))
    } else {
        Err(miette!(
            "Property await requires a condition: --equals, --gt, --lt, or --contains"
        ))
    }
}

/// Shared polling config extracted from `AwaitArgs`.
struct PollConfig {
    timeout: Duration,
    interval: Duration,
    timeout_secs: f64,
}

/// Poll a property on a node until a condition is met.
fn await_property(
    node: &str,
    property: &str,
    op: &str,
    target: &str,
    cfg: &PollConfig,
    format: &OutputFormat,
) -> Result<()> {
    let start = Instant::now();
    let script = generate_property_read_script(node, property);

    loop {
        if start.elapsed() > cfg.timeout {
            return Err(miette!(
                "Timeout after {:.1}s waiting for {node}.{property} {op} {target}",
                cfg.timeout_secs
            ));
        }

        let result = run_eval_timeout(&script, AUTO_TIMEOUT)?;
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap_or_default();
        let value = &parsed["value"];

        if check_condition(value, op, target) {
            let elapsed = start.elapsed().as_secs_f64();
            match format {
                OutputFormat::Json => {
                    cprintln!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "condition": format!("{node}.{property} {op} {target}"),
                            "met": true,
                            "elapsed_seconds": elapsed,
                            "final_value": value,
                        }))
                        .unwrap()
                    );
                }
                OutputFormat::Text => {
                    cprintln!(
                        "{}",
                        format!("Condition met: {node}.{property} {op} {target} ({elapsed:.1}s)")
                            .green()
                    );
                }
            }
            return Ok(());
        }

        std::thread::sleep(cfg.interval);
    }
}

/// Poll for a node's existence or removal.
fn await_node(node: &str, removed: bool, cfg: &PollConfig, format: &OutputFormat) -> Result<()> {
    let start = Instant::now();
    let script = generate_node_exists_script(node);

    loop {
        if start.elapsed() > cfg.timeout {
            let action = if removed { "to be removed" } else { "to exist" };
            return Err(miette!(
                "Timeout after {:.1}s waiting for {node} {action}",
                cfg.timeout_secs
            ));
        }

        let result = run_eval_timeout(&script, AUTO_TIMEOUT)?;
        let exists = result.trim() == "True" || result.trim() == "true";
        let met = if removed { !exists } else { exists };

        if met {
            let elapsed = start.elapsed().as_secs_f64();
            let action = if removed { "removed" } else { "exists" };
            match format {
                OutputFormat::Json => {
                    cprintln!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "condition": format!("{node} {action}"),
                            "met": true,
                            "elapsed_seconds": elapsed,
                        }))
                        .unwrap()
                    );
                }
                OutputFormat::Text => {
                    cprintln!(
                        "{}",
                        format!("Condition met: {node} {action} ({elapsed:.1}s)").green()
                    );
                }
            }
            return Ok(());
        }

        std::thread::sleep(cfg.interval);
    }
}

pub fn cmd_await(args: &AwaitArgs) -> Result<()> {
    let cfg = PollConfig {
        timeout: Duration::from_secs_f64(args.timeout),
        interval: Duration::from_millis(args.interval),
        timeout_secs: args.timeout,
    };

    match (&args.node, &args.property) {
        (Some(node), Some(property)) => {
            let (op, target) = parse_comparator(args)?;
            await_property(node, property, op, &target, &cfg, &args.format)
        }
        (Some(node), None) => await_node(node, args.removed, &cfg, &args.format),
        (None, _) => Err(miette!("Specify --node <name> for await")),
    }
}

// ── 6. Navigate ──────────────────────────────────────────────────────

/// Generate GDScript to find the NavigationAgent on a node and set the target.
/// Supports both 2D (Vector2) and 3D (Vector3) targets.
fn generate_navigate_start_script(node_lookup: &str, target_expr: &str) -> String {
    format!(
        "extends Node\n\
         \n\
         func run():\n\
         {node_lookup}\n\
         \tvar agent = null\n\
         \tfor child in node.get_children():\n\
         \t\tif child is NavigationAgent2D or child is NavigationAgent3D:\n\
         \t\t\tagent = child\n\
         \t\t\tbreak\n\
         \tif agent == null: return \"ERROR: no NavigationAgent found on '\" + node.name + \"'\"\n\
         \tagent.target_position = {target_expr}\n\
         \tvar d = {{\"node\": node.name, \"agent\": agent.get_class(), \"target\": str({target_expr})}}\n\
         \treturn JSON.stringify(d)\n"
    )
}

/// Generate GDScript to check if navigation is finished.
/// Returns `finished`, `position`, `target`, and `distance` so Rust can apply
/// distance-based fallback when `is_navigation_finished()` is unreliable.
fn generate_navigate_poll_script(node_lookup: &str) -> String {
    format!(
        "extends Node\n\
         \n\
         func run():\n\
         {node_lookup}\n\
         \tvar agent = null\n\
         \tfor child in node.get_children():\n\
         \t\tif child is NavigationAgent2D or child is NavigationAgent3D:\n\
         \t\t\tagent = child\n\
         \t\t\tbreak\n\
         \tif agent == null: return \"ERROR: agent lost\"\n\
         \tvar pos = node.global_position\n\
         \tvar tgt = agent.target_position\n\
         \tvar dist = pos.distance_to(tgt)\n\
         \tvar d = {{\"finished\": agent.is_navigation_finished(), \"position\": str(pos), \"target\": str(tgt), \"distance\": dist}}\n\
         \treturn JSON.stringify(d)\n"
    )
}

/// Parse "X,Y" or "X,Y,Z" into a GDScript Vector expression.
fn parse_target_coords(coords: &str) -> Result<String> {
    let parts: Vec<&str> = coords.split(',').collect();
    match parts.len() {
        2 => {
            let x = parts[0].trim();
            let y = parts[1].trim();
            x.parse::<f64>()
                .map_err(|_| miette!("Invalid X coordinate: {x}"))?;
            y.parse::<f64>()
                .map_err(|_| miette!("Invalid Y coordinate: {y}"))?;
            Ok(format!("Vector2({x}, {y})"))
        }
        3 => {
            let x = parts[0].trim();
            let y = parts[1].trim();
            let z = parts[2].trim();
            x.parse::<f64>()
                .map_err(|_| miette!("Invalid X coordinate: {x}"))?;
            y.parse::<f64>()
                .map_err(|_| miette!("Invalid Y coordinate: {y}"))?;
            z.parse::<f64>()
                .map_err(|_| miette!("Invalid Z coordinate: {z}"))?;
            Ok(format!("Vector3({x}, {y}, {z})"))
        }
        _ => Err(miette!(
            "Invalid coordinates '{coords}' — expected X,Y (2D) or X,Y,Z (3D)"
        )),
    }
}

/// Resolve target from a node's global_position via eval.
fn generate_navigate_to_node_script(node_lookup: &str, target_node: &str) -> String {
    let target_lookup = if target_node.starts_with('/') {
        format!(
            "\tvar target = get_tree().get_root().get_node_or_null(\"{target_node}\")\n\
             \tif target == null: return \"ERROR: target node '{target_node}' not found\""
        )
    } else {
        format!(
            "\tvar target = get_tree().get_root().find_child(\"{target_node}\", true, false)\n\
             \tif target == null: return \"ERROR: target node '{target_node}' not found\""
        )
    };
    format!(
        "extends Node\n\
         \n\
         func run():\n\
         {node_lookup}\n\
         {target_lookup}\n\
         \tvar agent = null\n\
         \tfor child in node.get_children():\n\
         \t\tif child is NavigationAgent2D or child is NavigationAgent3D:\n\
         \t\t\tagent = child\n\
         \t\t\tbreak\n\
         \tif agent == null: return \"ERROR: no NavigationAgent found on '\" + node.name + \"'\"\n\
         \tagent.target_position = target.global_position\n\
         \tvar d = {{\"node\": node.name, \"agent\": agent.get_class(), \"target\": str(target.global_position)}}\n\
         \treturn JSON.stringify(d)\n"
    )
}

pub fn cmd_navigate(args: &NavigateArgs) -> Result<()> {
    let lookup = node_lookup_gdscript(&args.node);

    // 1. Set target on the NavigationAgent
    let start_script = match (&args.to, &args.to_node) {
        (Some(coords), None) => {
            let target_expr = parse_target_coords(coords)?;
            generate_navigate_start_script(&lookup, &target_expr)
        }
        (None, Some(target_node)) => generate_navigate_to_node_script(&lookup, target_node),
        (Some(_), Some(_)) => return Err(miette!("Specify either --to or --to-node, not both")),
        (None, None) => return Err(miette!("Specify --to X,Y or --to-node <name>")),
    };

    let start_result = run_eval(&start_script)?;
    let start_info: serde_json::Value =
        serde_json::from_str(&start_result).map_err(|e| miette!("Failed to parse result: {e}"))?;

    let agent_type = start_info["agent"].as_str().unwrap_or("?");
    let target_str = start_info["target"].as_str().unwrap_or("?");

    // 2. Poll until navigation finishes
    let poll_script = generate_navigate_poll_script(&lookup);
    let start = Instant::now();
    let timeout = Duration::from_secs_f64(args.timeout);
    let interval = Duration::from_millis(args.interval);

    // Distance threshold: consider "arrived" if within this many units of the target.
    // Godot's is_navigation_finished() uses a very tight tolerance and can return false
    // even when the node is visually at the destination.
    let distance_threshold = 25.0_f64;

    // Stall detection: if position doesn't change for N consecutive polls, the node is
    // either arrived or permanently stuck — either way, stop polling.
    let mut last_position = String::new();
    let mut stall_count: u32 = 0;
    let stall_limit: u32 = 5; // 5 * 200ms = 1s of no movement

    loop {
        if start.elapsed() > timeout {
            return Err(miette!(
                "Navigation timeout after {:.1}s — {} didn't reach {target_str}",
                args.timeout,
                args.node,
            ));
        }

        std::thread::sleep(interval);

        let poll_result = run_eval_timeout(&poll_script, AUTO_TIMEOUT)?;
        let poll: serde_json::Value = serde_json::from_str(&poll_result).unwrap_or_default();

        let finished = poll["finished"].as_bool() == Some(true);
        let distance = poll["distance"].as_f64();
        let close_enough = distance.is_some_and(|d| d < distance_threshold);

        // Stall detection: position unchanged between consecutive polls
        let current_pos = poll["position"].as_str().unwrap_or("").to_string();
        if current_pos == last_position && !current_pos.is_empty() {
            stall_count += 1;
        } else {
            stall_count = 0;
        }
        last_position = current_pos;

        let stalled = stall_count >= stall_limit && close_enough;

        if finished || close_enough || stalled {
            let elapsed = start.elapsed().as_secs_f64();
            let final_pos = poll["position"].as_str().unwrap_or("?");
            let final_dist = distance.unwrap_or(0.0);
            match args.format {
                OutputFormat::Json => {
                    cprintln!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "node": args.node,
                            "agent": agent_type,
                            "target": target_str,
                            "final_position": final_pos,
                            "distance": final_dist,
                            "elapsed_seconds": elapsed,
                            "finished": true,
                        }))
                        .unwrap()
                    );
                }
                OutputFormat::Text => {
                    cprintln!(
                        "{}",
                        format!(
                            "{} navigated to {target_str} ({elapsed:.1}s, {agent_type})",
                            args.node
                        )
                        .green()
                    );
                }
            }
            return Ok(());
        }
    }
}

// ── 7. Move-to ───────────────────────────────────────────────────────

fn generate_move_to_node_script(node: &str) -> String {
    let lookup = node_lookup_gdscript(node);
    let pos_resolve = node_position_gdscript();
    format!(
        "extends Node\n\
         \n\
         func run():\n\
         {lookup}\n\
         {pos_resolve}\n\
         \tvar ev = InputEventMouseMotion.new()\n\
         \tev.position = pos\n\
         \tev.global_position = pos\n\
         \tInput.parse_input_event(ev)\n\
         \treturn \"(%s, %s)\" % [pos.x, pos.y]\n"
    )
}

pub fn cmd_move_to(args: &MoveToArgs) -> Result<()> {
    match (&args.pos, &args.node) {
        (Some(pos), None) => {
            let (x, y) = parse_pos(pos)?;
            if let Some(dur) = args.duration {
                // Smooth move: interpolate from (0,0) isn't useful, so we just
                // move in steps from current pos approximation. For simplicity,
                // generate N motion events spread over duration.
                let steps = 10u32;
                let step_delay = Duration::from_secs_f64(dur / f64::from(steps));
                // We need a starting position — use a get-pos eval first
                let target_x: f64 = x.parse().unwrap_or(0.0);
                let target_y: f64 = y.parse().unwrap_or(0.0);
                for i in 1..=steps {
                    let t = f64::from(i) / f64::from(steps);
                    let cx = target_x * t;
                    let cy = target_y * t;
                    let script = mouse_motion_script(&format!("{cx:.1}"), &format!("{cy:.1}"));
                    run_eval(&script)?;
                    if i < steps {
                        std::thread::sleep(step_delay);
                    }
                }
                match args.format {
                    OutputFormat::Json => {
                        cprintln!(
                            "{}",
                            serde_json::to_string_pretty(&serde_json::json!({
                                "action": "move_to",
                                "position": [target_x, target_y],
                                "duration": dur,
                            }))
                            .unwrap()
                        );
                    }
                    OutputFormat::Text => {
                        cprintln!("{}", format!("Moved to ({x}, {y}) over {dur}s").green());
                    }
                }
                Ok(())
            } else {
                let script = mouse_motion_script(&x, &y);
                let result = run_eval(&script)?;
                match args.format {
                    OutputFormat::Json => {
                        cprintln!(
                            "{}",
                            serde_json::to_string_pretty(&serde_json::json!({
                                "action": "move_to",
                                "position": result.trim(),
                            }))
                            .unwrap()
                        );
                    }
                    OutputFormat::Text => cprintln!("{}", format!("Moved to {result}").green()),
                }
                Ok(())
            }
        }
        (None, Some(node)) => {
            let script = generate_move_to_node_script(node);
            let result = run_eval(&script)?;
            match args.format {
                OutputFormat::Json => {
                    cprintln!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "action": "move_to",
                            "node": node,
                            "position": result.trim(),
                        }))
                        .unwrap()
                    );
                }
                OutputFormat::Text => {
                    cprintln!("{}", format!("Moved to {node} at {result}").green());
                }
            }
            Ok(())
        }
        (Some(_), Some(_)) => Err(miette!("Specify either --pos or --node, not both")),
        (None, None) => Err(miette!("Specify --pos X,Y or --node <name>")),
    }
}

// ── 7. Drag ──────────────────────────────────────────────────────────

fn mouse_button_constant(button: &str) -> Result<&'static str> {
    match button.to_lowercase().as_str() {
        "left" => Ok("MOUSE_BUTTON_LEFT"),
        "right" => Ok("MOUSE_BUTTON_RIGHT"),
        "middle" => Ok("MOUSE_BUTTON_MIDDLE"),
        other => Err(miette!(
            "Unknown mouse button '{other}' (expected: left, right, middle)"
        )),
    }
}

/// Generate script for mouse button press/release at a position.
fn mouse_button_script(x: &str, y: &str, button: &str, pressed: bool) -> Result<String> {
    let btn = mouse_button_constant(button)?;
    let state = if pressed { "true" } else { "false" };
    Ok(format!(
        "extends Node\n\
         \n\
         func run():\n\
         \tvar pos = Vector2({x}, {y})\n\
         \tvar ev = InputEventMouseButton.new()\n\
         \tev.button_index = {btn}\n\
         \tev.pressed = {state}\n\
         \tev.position = pos\n\
         \tev.global_position = pos\n\
         \tInput.parse_input_event(ev)\n\
         \treturn \"ok\"\n"
    ))
}

/// Resolve node coordinates via eval — returns "(x, y)" string.
fn resolve_node_pos(node: &str) -> Result<(f64, f64)> {
    let lookup = node_lookup_gdscript(node);
    let pos_resolve = node_position_gdscript();
    let script = format!(
        "extends Node\n\
         \n\
         func run():\n\
         {lookup}\n\
         {pos_resolve}\n\
         \treturn \"%s,%s\" % [pos.x, pos.y]\n"
    );
    let result = run_eval(&script)?;
    let parts: Vec<&str> = result.trim().split(',').collect();
    if parts.len() != 2 {
        return Err(miette!("Failed to resolve position for node '{node}'"));
    }
    let x = parts[0]
        .parse::<f64>()
        .map_err(|_| miette!("Invalid X from node position"))?;
    let y = parts[1]
        .parse::<f64>()
        .map_err(|_| miette!("Invalid Y from node position"))?;
    Ok((x, y))
}

pub fn cmd_drag(args: &DragArgs) -> Result<()> {
    // Resolve start position
    let (start_x, start_y) = match (&args.from, &args.from_node) {
        (Some(pos), None) => {
            let (x, y) = parse_pos(pos)?;
            (x.parse::<f64>().unwrap(), y.parse::<f64>().unwrap())
        }
        (None, Some(node)) => resolve_node_pos(node)?,
        (Some(_), Some(_)) => {
            return Err(miette!("Specify either --from or --from-node, not both"));
        }
        (None, None) => return Err(miette!("Specify --from X,Y or --from-node <name>")),
    };

    // Resolve end position
    let (end_x, end_y) = match (&args.to, &args.to_node) {
        (Some(pos), None) => {
            let (x, y) = parse_pos(pos)?;
            (x.parse::<f64>().unwrap(), y.parse::<f64>().unwrap())
        }
        (None, Some(node)) => resolve_node_pos(node)?,
        (Some(_), Some(_)) => return Err(miette!("Specify either --to or --to-node, not both")),
        (None, None) => return Err(miette!("Specify --to X,Y or --to-node <name>")),
    };

    let steps = args.steps.max(1);
    let step_delay = Duration::from_secs_f64(args.duration / f64::from(steps));

    // 1. Move to start position
    let move_script = mouse_motion_script(&format!("{start_x:.1}"), &format!("{start_y:.1}"));
    run_eval(&move_script)?;

    // 2. Mouse button down at start
    let down_script = mouse_button_script(
        &format!("{start_x:.1}"),
        &format!("{start_y:.1}"),
        &args.button,
        true,
    )?;
    run_eval(&down_script)?;

    // 3. Interpolate motion events
    for i in 1..=steps {
        let t = f64::from(i) / f64::from(steps);
        let cx = start_x + (end_x - start_x) * t;
        let cy = start_y + (end_y - start_y) * t;
        let motion = mouse_motion_script(&format!("{cx:.1}"), &format!("{cy:.1}"));
        run_eval(&motion)?;
        if i < steps {
            std::thread::sleep(step_delay);
        }
    }

    // 4. Mouse button up at end
    let up_script = mouse_button_script(
        &format!("{end_x:.1}"),
        &format!("{end_y:.1}"),
        &args.button,
        false,
    )?;
    run_eval(&up_script)?;

    match args.format {
        OutputFormat::Json => {
            cprintln!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "action": "drag",
                    "from": [start_x, start_y],
                    "to": [end_x, end_y],
                    "button": args.button,
                    "duration": args.duration,
                    "steps": steps,
                }))
                .unwrap()
            );
        }
        OutputFormat::Text => {
            cprintln!(
                "{}",
                format!("Dragged from ({start_x:.0}, {start_y:.0}) to ({end_x:.0}, {end_y:.0})")
                    .green()
            );
        }
    }
    Ok(())
}

// ── 8. Hover ─────────────────────────────────────────────────────────

pub fn cmd_hover(args: &HoverArgs) -> Result<()> {
    match (&args.node, &args.pos) {
        (Some(node), None) => {
            let script = generate_move_to_node_script(node);
            let result = run_eval(&script)?;
            // Wait for hover duration to let game process mouse_enter events
            std::thread::sleep(Duration::from_secs_f64(args.duration));
            match args.format {
                OutputFormat::Json => {
                    cprintln!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "action": "hover",
                            "node": node,
                            "position": result.trim(),
                            "duration": args.duration,
                        }))
                        .unwrap()
                    );
                }
                OutputFormat::Text => {
                    cprintln!("{}", format!("Hovering over {node} at {result}").green());
                }
            }
            Ok(())
        }
        (None, Some(pos)) => {
            let (x, y) = parse_pos(pos)?;
            let script = mouse_motion_script(&x, &y);
            run_eval(&script)?;
            std::thread::sleep(Duration::from_secs_f64(args.duration));
            match args.format {
                OutputFormat::Json => {
                    cprintln!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "action": "hover",
                            "position": [x.parse::<f64>().unwrap_or(0.0), y.parse::<f64>().unwrap_or(0.0)],
                            "duration": args.duration,
                        }))
                        .unwrap()
                    );
                }
                OutputFormat::Text => {
                    cprintln!("{}", format!("Hovering at ({x}, {y})").green());
                }
            }
            Ok(())
        }
        (Some(_), Some(_)) => Err(miette!("Specify either --node or --pos, not both")),
        (None, None) => Err(miette!("Specify --node <name> or --pos X,Y")),
    }
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_parses(script: &str) {
        let tree = crate::core::parser::parse(script).unwrap();
        assert!(
            !tree.root_node().has_error(),
            "Script has parse errors:\n{script}"
        );
    }

    // -- Describe scripts --

    #[test]
    fn describe_auto_detect_parses() {
        assert_parses(&generate_describe_script(None, None));
    }

    #[test]
    fn describe_named_node_parses() {
        assert_parses(&generate_describe_script(Some("Hero"), None));
    }

    #[test]
    fn describe_absolute_path_parses() {
        assert_parses(&generate_describe_script(Some("/root/Main/Player"), None));
    }

    #[test]
    fn describe_custom_radius_parses() {
        assert_parses(&generate_describe_script(Some("Player"), Some(1000.0)));
    }

    // -- Find scripts --

    #[test]
    fn find_by_name_script_parses() {
        assert_parses(&generate_find_by_name_script("Player"));
    }

    #[test]
    fn find_by_name_absolute_path_parses() {
        assert_parses(&generate_find_by_name_script("/root/Main/Player"));
    }

    #[test]
    fn find_by_type_script_parses() {
        assert_parses(&generate_find_by_type_script("CharacterBody2D"));
    }

    #[test]
    fn find_by_group_script_parses() {
        assert_parses(&generate_find_by_group_script("enemies"));
    }

    // -- Get-prop scripts --

    #[test]
    fn get_prop_by_name_parses() {
        let lookup = node_lookup_gdscript("Player");
        assert_parses(&generate_get_prop_script(&lookup, "velocity"));
    }

    #[test]
    fn get_prop_by_path_parses() {
        let lookup = node_lookup_gdscript("/root/Main/Player");
        assert_parses(&generate_get_prop_script(&lookup, "position"));
    }

    #[test]
    fn get_prop_by_id_parses() {
        let lookup = node_lookup_by_id_gdscript(12345);
        assert_parses(&generate_get_prop_script(&lookup, "text"));
    }

    // -- Call scripts --

    #[test]
    fn call_script_no_args_parses() {
        let lookup = node_lookup_gdscript("Player");
        assert_parses(&generate_call_script(&lookup, "queue_free", "[]"));
    }

    #[test]
    fn call_script_with_args_parses() {
        let lookup = node_lookup_gdscript("Player");
        assert_parses(&generate_call_script(&lookup, "take_damage", "[10]"));
    }

    #[test]
    fn call_script_by_id_parses() {
        let lookup = node_lookup_by_id_gdscript(99999);
        assert_parses(&generate_call_script(&lookup, "set_health", "[50]"));
    }

    // -- Set scripts --

    #[test]
    fn set_script_number_parses() {
        let lookup = node_lookup_gdscript("Player");
        assert_parses(&generate_set_script(&lookup, "speed", "200"));
    }

    #[test]
    fn set_script_vector_parses() {
        let lookup = node_lookup_gdscript("Player");
        assert_parses(&generate_set_script(
            &lookup,
            "position",
            "Vector2(100, 200)",
        ));
    }

    #[test]
    fn set_script_string_parses() {
        let lookup = node_lookup_gdscript("/root/Main/Label");
        assert_parses(&generate_set_script(&lookup, "text", "\"Game Over\""));
    }

    // -- Await scripts --

    #[test]
    fn node_exists_script_parses() {
        assert_parses(&generate_node_exists_script("Player"));
    }

    #[test]
    fn node_exists_path_script_parses() {
        assert_parses(&generate_node_exists_script("/root/Main/GameOver"));
    }

    #[test]
    fn property_read_script_parses() {
        assert_parses(&generate_property_read_script("Player", "health"));
    }

    // -- Move-to scripts --

    #[test]
    fn mouse_motion_script_parses() {
        assert_parses(&mouse_motion_script("400", "300"));
    }

    #[test]
    fn move_to_node_script_parses() {
        assert_parses(&generate_move_to_node_script("StartButton"));
    }

    #[test]
    fn move_to_node_path_script_parses() {
        assert_parses(&generate_move_to_node_script("/root/UI/Button"));
    }

    // -- Navigate scripts --

    #[test]
    fn navigate_start_2d_parses() {
        let lookup = node_lookup_gdscript("Player");
        assert_parses(&generate_navigate_start_script(
            &lookup,
            "Vector2(500, 300)",
        ));
    }

    #[test]
    fn navigate_start_3d_parses() {
        let lookup = node_lookup_gdscript("Player");
        assert_parses(&generate_navigate_start_script(
            &lookup,
            "Vector3(10, 0, 5)",
        ));
    }

    #[test]
    fn navigate_poll_parses() {
        let lookup = node_lookup_gdscript("Player");
        assert_parses(&generate_navigate_poll_script(&lookup));
    }

    #[test]
    fn navigate_to_node_parses() {
        let lookup = node_lookup_gdscript("Player");
        assert_parses(&generate_navigate_to_node_script(&lookup, "Chest"));
    }

    #[test]
    fn navigate_to_node_path_parses() {
        let lookup = node_lookup_gdscript("Player");
        assert_parses(&generate_navigate_to_node_script(
            &lookup,
            "/root/Main/Exit",
        ));
    }

    #[test]
    fn parse_target_coords_2d() {
        assert_eq!(parse_target_coords("500,300").unwrap(), "Vector2(500, 300)");
    }

    #[test]
    fn parse_target_coords_3d() {
        assert_eq!(parse_target_coords("10,0,5").unwrap(), "Vector3(10, 0, 5)");
    }

    #[test]
    fn parse_target_coords_invalid() {
        assert!(parse_target_coords("100").is_err());
        assert!(parse_target_coords("1,2,3,4").is_err());
        assert!(parse_target_coords("abc,200").is_err());
    }

    // -- Drag scripts --

    #[test]
    fn mouse_button_down_script_parses() {
        assert_parses(&mouse_button_script("100", "200", "left", true).unwrap());
    }

    #[test]
    fn mouse_button_up_script_parses() {
        assert_parses(&mouse_button_script("300", "400", "right", false).unwrap());
    }

    #[test]
    fn mouse_button_invalid() {
        assert!(mouse_button_constant("back").is_err());
    }

    // -- Node lookup helpers --

    #[test]
    fn node_lookup_absolute_path() {
        let lookup = node_lookup_gdscript("/root/Main/Player");
        assert!(lookup.contains("get_node_or_null"));
    }

    #[test]
    fn node_lookup_find_child() {
        let lookup = node_lookup_gdscript("Player");
        assert!(lookup.contains("find_child"));
    }

    #[test]
    fn node_lookup_by_id() {
        let lookup = node_lookup_by_id_gdscript(42);
        assert!(lookup.contains("instance_from_id(42)"));
    }

    // -- Parse position --

    #[test]
    fn parse_pos_valid() {
        let (x, y) = parse_pos("100,200").unwrap();
        assert_eq!(x, "100");
        assert_eq!(y, "200");
    }

    #[test]
    fn parse_pos_with_spaces() {
        let (x, y) = parse_pos("100 , 200").unwrap();
        assert_eq!(x, "100");
        assert_eq!(y, "200");
    }

    #[test]
    fn parse_pos_float() {
        let (x, y) = parse_pos("100.5,200.7").unwrap();
        assert_eq!(x, "100.5");
        assert_eq!(y, "200.7");
    }

    #[test]
    fn parse_pos_invalid_format() {
        assert!(parse_pos("100").is_err());
        assert!(parse_pos("100,200,300").is_err());
    }

    #[test]
    fn parse_pos_invalid_number() {
        assert!(parse_pos("abc,200").is_err());
        assert!(parse_pos("100,xyz").is_err());
    }
}
