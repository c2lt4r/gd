use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use miette::Result;
use serde::Serialize;

use crate::core::gd_ast;

// ── Output ──────────────────────────────────────────────────────────────────

#[derive(Serialize, Debug)]
pub struct ConvertSignalOutput {
    pub signal: String,
    pub from_node: String,
    pub method: String,
    pub direction: String, // "to-code" or "to-scene"
    pub scene_file: String,
    pub script_file: String,
    pub applied: bool,
}

// ── Public entry point ──────────────────────────────────────────────────────

/// Convert a signal connection between scene wiring (.tscn) and code (.connect()).
///
/// `to_code = true`: Remove `[connection]` from scene, add `.connect()` in `_ready()`.
/// `to_code = false`: Remove `.connect()` from script, add `[connection]` in scene.
pub fn convert_signal(
    scene_file: &Path,
    signal: &str,
    from: &str,
    method: &str,
    to_code: bool,
    dry_run: bool,
    project_root: &Path,
) -> Result<ConvertSignalOutput> {
    let relative_scene = crate::core::fs::relative_slash(scene_file, project_root);

    if to_code {
        convert_to_code(
            scene_file,
            signal,
            from,
            method,
            dry_run,
            project_root,
            &relative_scene,
        )
    } else {
        convert_to_scene(
            scene_file,
            signal,
            from,
            method,
            dry_run,
            project_root,
            &relative_scene,
        )
    }
}

// ── Scene → Code ────────────────────────────────────────────────────────────

fn convert_to_code(
    scene_file: &Path,
    signal: &str,
    from: &str,
    method: &str,
    dry_run: bool,
    project_root: &Path,
    relative_scene: &str,
) -> Result<ConvertSignalOutput> {
    // Parse scene
    let scene_source = std::fs::read_to_string(scene_file)
        .map_err(|e| miette::miette!("cannot read scene: {e}"))?;
    let scene_data = crate::core::scene::parse_scene(&scene_source)?;

    // Find the connection
    let conn = scene_data
        .connections
        .iter()
        .find(|c| c.signal == signal && c.from == from && c.method == method)
        .ok_or_else(|| miette::miette!("connection not found: {from}.{signal} → {method}"))?;
    let to_node = conn.to.clone();

    // Find the script file attached to the target node
    let script_path = find_script_for_node(&scene_data, &to_node, scene_file, project_root)?;
    let relative_script = crate::core::fs::relative_slash(&script_path, project_root);

    if dry_run {
        return Ok(ConvertSignalOutput {
            signal: signal.to_string(),
            from_node: from.to_string(),
            method: method.to_string(),
            direction: "to-code".to_string(),
            scene_file: relative_scene.to_string(),
            script_file: relative_script,
            applied: false,
        });
    }

    // 1. Remove [connection] from scene
    let new_scene = remove_connection_line(&scene_source, signal, from, &to_node, method)?;
    std::fs::write(scene_file, &new_scene)
        .map_err(|e| miette::miette!("cannot write scene: {e}"))?;

    // 2. Add .connect() to script's _ready()
    let script_source = std::fs::read_to_string(&script_path)
        .map_err(|e| miette::miette!("cannot read script: {e}"))?;

    let connect_call = build_connect_call(from, signal, method);

    // Guard against duplicate .connect() calls (#26)
    if script_source.contains(&connect_call) {
        return Err(miette::miette!(
            "duplicate: {connect_call} already exists in script"
        ));
    }

    let new_script = add_to_ready(&script_source, &connect_call)?;

    super::validate_no_new_errors(&script_source, &new_script)?;
    std::fs::write(&script_path, &new_script)
        .map_err(|e| miette::miette!("cannot write script: {e}"))?;

    // Record undo for both files
    record_undo(
        scene_file,
        &scene_source,
        &script_path,
        &script_source,
        project_root,
        signal,
        from,
        "to-code",
    );

    Ok(ConvertSignalOutput {
        signal: signal.to_string(),
        from_node: from.to_string(),
        method: method.to_string(),
        direction: "to-code".to_string(),
        scene_file: relative_scene.to_string(),
        script_file: relative_script,
        applied: true,
    })
}

// ── Code → Scene ────────────────────────────────────────────────────────────

fn convert_to_scene(
    scene_file: &Path,
    signal: &str,
    from: &str,
    method: &str,
    dry_run: bool,
    project_root: &Path,
    relative_scene: &str,
) -> Result<ConvertSignalOutput> {
    // Parse scene to find script and validate nodes
    let scene_source = std::fs::read_to_string(scene_file)
        .map_err(|e| miette::miette!("cannot read scene: {e}"))?;
    let scene_data = crate::core::scene::parse_scene(&scene_source)?;

    // The target node for code connections is typically "." (the root with the script)
    let to_node = ".";

    // Find the script file attached to root
    let script_path = find_script_for_node(&scene_data, to_node, scene_file, project_root)?;
    let relative_script = crate::core::fs::relative_slash(&script_path, project_root);

    // Verify the .connect() call exists in the script
    let script_source = std::fs::read_to_string(&script_path)
        .map_err(|e| miette::miette!("cannot read script: {e}"))?;

    if !find_connect_call_in_source(&script_source, from, signal, method) {
        return Err(miette::miette!(
            "no matching .connect() call found for {from}.{signal} → {method} in {}",
            relative_script
        ));
    }

    if dry_run {
        return Ok(ConvertSignalOutput {
            signal: signal.to_string(),
            from_node: from.to_string(),
            method: method.to_string(),
            direction: "to-scene".to_string(),
            scene_file: relative_scene.to_string(),
            script_file: relative_script,
            applied: false,
        });
    }

    // 1. Remove .connect() from script, ensure _ready() isn't left empty
    let mut new_script = remove_connect_call(&script_source, from, signal, method)?;
    new_script = ensure_ready_has_body(&new_script)?;
    super::validate_no_new_errors(&script_source, &new_script)?;
    std::fs::write(&script_path, &new_script)
        .map_err(|e| miette::miette!("cannot write script: {e}"))?;

    // 2. Add [connection] to scene
    let new_scene = append_connection_line(&scene_source, signal, from, to_node, method)?;
    std::fs::write(scene_file, &new_scene)
        .map_err(|e| miette::miette!("cannot write scene: {e}"))?;

    record_undo(
        scene_file,
        &scene_source,
        &script_path,
        &script_source,
        project_root,
        signal,
        from,
        "to-scene",
    );

    Ok(ConvertSignalOutput {
        signal: signal.to_string(),
        from_node: from.to_string(),
        method: method.to_string(),
        direction: "to-scene".to_string(),
        scene_file: relative_scene.to_string(),
        script_file: relative_script,
        applied: true,
    })
}

// ── Helpers ─────────────────────────────────────────────────────────────────

/// Find the script file attached to a node in the scene.
fn find_script_for_node(
    data: &crate::core::scene::SceneData,
    node_path: &str,
    scene_file: &Path,
    project_root: &Path,
) -> Result<PathBuf> {
    let node = if node_path == "." {
        // Root node is the one without a parent
        data.nodes.iter().find(|n| n.parent.is_none())
    } else {
        data.nodes.iter().find(|n| {
            if n.parent.as_deref() == Some(".") || n.parent.is_none() {
                n.name == node_path
            } else {
                let path = format!("{}/{}", n.parent.as_deref().unwrap_or("."), n.name);
                path == node_path
            }
        })
    };

    let node = node.ok_or_else(|| miette::miette!("node '{node_path}' not found in scene"))?;

    // Get script from node's script property or ext_resource
    let script_ref = node
        .script
        .as_ref()
        .ok_or_else(|| miette::miette!("node '{node_path}' has no script attached"))?;

    // script_ref can be:
    // - ExtResource("id") — reference to ext_resource
    // - "res://path.gd" — direct path
    resolve_script_path(script_ref, data, scene_file, project_root)
}

/// Resolve a script reference to an absolute file path.
fn resolve_script_path(
    script_ref: &str,
    data: &crate::core::scene::SceneData,
    scene_file: &Path,
    project_root: &Path,
) -> Result<PathBuf> {
    // Try ExtResource("id") format
    if let Some(id) = script_ref
        .strip_prefix("ExtResource(\"")
        .and_then(|s| s.strip_suffix("\")"))
        .or_else(|| {
            script_ref
                .strip_prefix("ExtResource( \"")
                .and_then(|s| s.strip_suffix("\" )"))
        })
    {
        let ext = data
            .ext_resources
            .iter()
            .find(|r| r.id == id)
            .ok_or_else(|| miette::miette!("ext_resource '{id}' not found"))?;
        return resolve_res_path(&ext.path, project_root);
    }

    // Try direct res:// path
    if script_ref.starts_with("res://") {
        return resolve_res_path(script_ref, project_root);
    }

    // Try relative to scene file
    let scene_dir = scene_file
        .parent()
        .ok_or_else(|| miette::miette!("cannot get scene directory"))?;
    let path = scene_dir.join(script_ref);
    if path.exists() {
        return Ok(path);
    }

    Err(miette::miette!("cannot resolve script path: {script_ref}"))
}

/// Resolve a `res://` path to an absolute filesystem path.
fn resolve_res_path(res_path: &str, project_root: &Path) -> Result<PathBuf> {
    let relative = res_path
        .strip_prefix("res://")
        .ok_or_else(|| miette::miette!("not a res:// path: {res_path}"))?;
    let path = project_root.join(relative);
    if !path.exists() {
        return Err(miette::miette!("file not found: {}", path.display()));
    }
    Ok(path)
}

/// Append a `[connection]` line to scene source.
fn append_connection_line(
    source: &str,
    signal: &str,
    from: &str,
    to: &str,
    method: &str,
) -> Result<String> {
    let conn_line =
        format!("[connection signal=\"{signal}\" from=\"{from}\" to=\"{to}\" method=\"{method}\"]");

    // Check for duplicate
    if source.contains(&conn_line) {
        return Err(miette::miette!("connection already exists in scene"));
    }

    let mut output = source.trim_end().to_string();
    // Add separator: blank line if no existing connections, single newline if there are
    if source.contains("[connection") {
        output.push('\n');
    } else {
        output.push_str("\n\n");
    }
    output.push_str(&conn_line);
    output.push('\n');
    Ok(output)
}

/// Build a `.connect()` call string for insertion into `_ready()`.
fn build_connect_call(from: &str, signal: &str, method: &str) -> String {
    if from == "." {
        format!("{signal}.connect({method})")
    } else {
        format!("${from}.{signal}.connect({method})")
    }
}

/// Remove a `[connection]` line from scene source.
fn remove_connection_line(
    source: &str,
    signal: &str,
    from: &str,
    to: &str,
    method: &str,
) -> Result<String> {
    let target =
        format!("[connection signal=\"{signal}\" from=\"{from}\" to=\"{to}\" method=\"{method}\"]");

    let lines: Vec<&str> = source.lines().collect();
    let mut found = false;
    let mut result: Vec<&str> = Vec::with_capacity(lines.len());

    for line in &lines {
        if line.trim() == target {
            found = true;
            continue;
        }
        result.push(line);
    }

    if !found {
        return Err(miette::miette!(
            "connection line not found in scene: {target}"
        ));
    }

    let mut output = result.join("\n");
    if !output.ends_with('\n') {
        output.push('\n');
    }
    // Clean up double blank lines
    while output.contains("\n\n\n") {
        output = output.replace("\n\n\n", "\n\n");
    }
    Ok(output)
}

/// Check if source contains a matching .connect() call.
fn find_connect_call_in_source(source: &str, from: &str, signal: &str, method: &str) -> bool {
    // Look for patterns like:
    //   $From.signal.connect(method)
    //   signal.connect(method)  (when from == ".")
    let pattern = if from == "." {
        format!("{signal}.connect({method})")
    } else {
        format!("${from}.{signal}.connect({method})")
    };
    source.contains(&pattern)
}

/// Remove a .connect() call line from script source.
fn remove_connect_call(source: &str, from: &str, signal: &str, method: &str) -> Result<String> {
    let pattern = if from == "." {
        format!("{signal}.connect({method})")
    } else {
        format!("${from}.{signal}.connect({method})")
    };

    let lines: Vec<&str> = source.lines().collect();
    let mut found = false;
    let mut result: Vec<&str> = Vec::with_capacity(lines.len());

    for line in &lines {
        if !found && line.trim().contains(&pattern) {
            // Verify the line is essentially just this connect call (possibly indented)
            let trimmed = line.trim();
            if trimmed == pattern || trimmed.starts_with(&pattern) {
                found = true;
                continue;
            }
        }
        result.push(line);
    }

    if !found {
        return Err(miette::miette!(".connect() call not found: {pattern}"));
    }

    let mut output = result.join("\n");
    if !output.ends_with('\n') {
        output.push('\n');
    }
    Ok(output)
}

/// If _ready() body is empty after removing a statement, insert `pass`.
fn ensure_ready_has_body(source: &str) -> Result<String> {
    let tree = crate::core::parser::parse(source)?;
    let file = gd_ast::convert(&tree, source);

    let Some(ready_func) = super::find_declaration_by_name(&file, "_ready") else {
        return Ok(source.to_string());
    };
    let Some(body) = ready_func.child_by_field_name("body") else {
        // No body at all — insert one
        let end = ready_func.end_byte();
        let mut new_source = String::with_capacity(source.len() + 8);
        new_source.push_str(&source[..end]);
        new_source.push_str("\n\tpass");
        new_source.push_str(&source[end..]);
        return Ok(new_source);
    };

    let body_text = super::invert_if::node_text(&body, source);
    let has_stmts = body_text.trim().chars().any(|c| !c.is_whitespace());

    if !has_stmts {
        let mut new_source = String::with_capacity(source.len());
        new_source.push_str(&source[..body.start_byte()]);
        new_source.push_str("\n\tpass");
        new_source.push_str(&source[body.end_byte()..]);
        return Ok(new_source);
    }

    Ok(source.to_string())
}

/// Add a line to _ready(), creating it if necessary.
fn add_to_ready(source: &str, line: &str) -> Result<String> {
    let tree = crate::core::parser::parse(source)?;
    let file = gd_ast::convert(&tree, source);

    if let Some(ready_func) = super::find_declaration_by_name(&file, "_ready") {
        let body = ready_func
            .child_by_field_name("body")
            .ok_or_else(|| miette::miette!("_ready() has no body"))?;

        let body_text = super::invert_if::node_text(&body, source);
        let body_trimmed = body_text.trim();

        if body_trimmed == "pass" {
            // Replace body using byte offsets (not splice, which uses
            // line_start_offset and would clobber the func header)
            let mut new_source = String::with_capacity(source.len());
            new_source.push_str(&source[..body.start_byte()]);
            write!(new_source, "\n\t{line}").unwrap();
            new_source.push_str(&source[body.end_byte()..]);
            return Ok(new_source);
        }

        // Append to end of body
        let body_end = body.end_byte();
        let mut new_source = String::with_capacity(source.len() + line.len() + 2);
        new_source.push_str(&source[..body_end]);
        write!(new_source, "\n\t{line}").unwrap();
        new_source.push_str(&source[body_end..]);
        Ok(new_source)
    } else {
        // Create _ready()
        let ready_func = format!("\nfunc _ready():\n\t{line}\n");
        let mut new_source = source.trim_end().to_string();
        new_source.push_str(&ready_func);
        Ok(new_source)
    }
}

#[allow(clippy::too_many_arguments)]
fn record_undo(
    scene_file: &Path,
    scene_source: &str,
    script_file: &Path,
    script_source: &str,
    project_root: &Path,
    signal: &str,
    from: &str,
    direction: &str,
) {
    let mut snaps: HashMap<PathBuf, Option<Vec<u8>>> = HashMap::new();
    snaps.insert(
        scene_file.to_path_buf(),
        Some(scene_source.as_bytes().to_vec()),
    );
    snaps.insert(
        script_file.to_path_buf(),
        Some(script_source.as_bytes().to_vec()),
    );
    let stack = super::undo::UndoStack::open(project_root);
    let _ = stack.record(
        "convert-signal",
        &format!("convert {from}.{signal} {direction}"),
        &snaps,
        project_root,
    );
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn setup_project(files: &[(&str, &str)]) -> TempDir {
        let temp = tempfile::Builder::new()
            .prefix("gdtest")
            .tempdir()
            .expect("create temp dir");
        fs::write(
            temp.path().join("project.godot"),
            "[application]\nconfig/name=\"test\"\n",
        )
        .expect("write project.godot");
        for (name, content) in files {
            let path = temp.path().join(name);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).ok();
            }
            fs::write(&path, content).expect("write file");
        }
        temp
    }

    fn basic_scene(script: &str) -> String {
        format!(
            concat!(
                "[gd_scene load_steps=2 format=3]\n\n",
                "[ext_resource type=\"Script\" path=\"res://{script}\" id=\"1\"]\n\n",
                "[node name=\"Main\" type=\"Node2D\"]\n",
                "script = ExtResource(\"1\")\n\n",
                "[node name=\"Button\" type=\"Button\" parent=\".\"]\n\n",
                "[connection signal=\"pressed\" from=\"Button\" to=\".\" method=\"_on_button_pressed\"]\n",
            ),
            script = script
        )
    }

    #[test]
    fn scene_to_code() {
        let temp = setup_project(&[
            ("main.tscn", &basic_scene("main.gd")),
            (
                "main.gd",
                "func _ready():\n\tpass\n\nfunc _on_button_pressed():\n\tprint(\"pressed\")\n",
            ),
        ]);
        let result = convert_signal(
            &temp.path().join("main.tscn"),
            "pressed",
            "Button",
            "_on_button_pressed",
            true,
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        assert_eq!(result.direction, "to-code");

        let scene = fs::read_to_string(temp.path().join("main.tscn")).unwrap();
        assert!(
            !scene.contains("[connection"),
            "should remove connection from scene, got:\n{scene}"
        );

        let script = fs::read_to_string(temp.path().join("main.gd")).unwrap();
        assert!(
            script.contains("$Button.pressed.connect(_on_button_pressed)"),
            "should add .connect() to _ready(), got:\n{script}"
        );
    }

    #[test]
    fn code_to_scene() {
        let scene_without_conn = concat!(
            "[gd_scene load_steps=2 format=3]\n\n",
            "[ext_resource type=\"Script\" path=\"res://main.gd\" id=\"1\"]\n\n",
            "[node name=\"Main\" type=\"Node2D\"]\n",
            "script = ExtResource(\"1\")\n\n",
            "[node name=\"Button\" type=\"Button\" parent=\".\"]\n",
        );
        let temp = setup_project(&[
            ("main.tscn", scene_without_conn),
            (
                "main.gd",
                "func _ready():\n\t$Button.pressed.connect(_on_button_pressed)\n\nfunc _on_button_pressed():\n\tprint(\"pressed\")\n",
            ),
        ]);
        let result = convert_signal(
            &temp.path().join("main.tscn"),
            "pressed",
            "Button",
            "_on_button_pressed",
            false,
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        assert_eq!(result.direction, "to-scene");

        let scene = fs::read_to_string(temp.path().join("main.tscn")).unwrap();
        assert!(
            scene.contains("[connection signal=\"pressed\" from=\"Button\" to=\".\" method=\"_on_button_pressed\"]"),
            "should add connection to scene, got:\n{scene}"
        );

        let script = fs::read_to_string(temp.path().join("main.gd")).unwrap();
        assert!(
            !script.contains(".connect("),
            "should remove .connect() from script, got:\n{script}"
        );
    }

    #[test]
    fn scene_to_code_dry_run() {
        let temp = setup_project(&[
            ("main.tscn", &basic_scene("main.gd")),
            ("main.gd", "func _ready():\n\tpass\n"),
        ]);
        let result = convert_signal(
            &temp.path().join("main.tscn"),
            "pressed",
            "Button",
            "_on_button_pressed",
            true,
            true,
            temp.path(),
        )
        .unwrap();
        assert!(!result.applied);

        let scene = fs::read_to_string(temp.path().join("main.tscn")).unwrap();
        assert!(
            scene.contains("[connection"),
            "dry run should not modify scene"
        );
    }

    #[test]
    fn scene_to_code_creates_ready() {
        let temp = setup_project(&[
            ("main.tscn", &basic_scene("main.gd")),
            (
                "main.gd",
                "func _on_button_pressed():\n\tprint(\"pressed\")\n",
            ),
        ]);
        let result = convert_signal(
            &temp.path().join("main.tscn"),
            "pressed",
            "Button",
            "_on_button_pressed",
            true,
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);

        let script = fs::read_to_string(temp.path().join("main.gd")).unwrap();
        assert!(
            script.contains("func _ready():"),
            "should create _ready(), got:\n{script}"
        );
        assert!(
            script.contains("$Button.pressed.connect(_on_button_pressed)"),
            "should have connect call, got:\n{script}"
        );
    }

    #[test]
    fn to_code_rejects_duplicate_connect() {
        let temp = setup_project(&[
            ("main.tscn", &basic_scene("main.gd")),
            (
                "main.gd",
                "func _ready():\n\t$Button.pressed.connect(_on_button_pressed)\n\nfunc _on_button_pressed():\n\tprint(\"pressed\")\n",
            ),
        ]);
        let result = convert_signal(
            &temp.path().join("main.tscn"),
            "pressed",
            "Button",
            "_on_button_pressed",
            true,
            false,
            temp.path(),
        );
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("duplicate"),
            "expected duplicate error, got: {msg}"
        );
    }

    #[test]
    fn missing_connection_errors() {
        let temp = setup_project(&[
            ("main.tscn", &basic_scene("main.gd")),
            ("main.gd", "func _ready():\n\tpass\n"),
        ]);
        let result = convert_signal(
            &temp.path().join("main.tscn"),
            "nonexistent",
            "Button",
            "_handler",
            true,
            false,
            temp.path(),
        );
        assert!(result.is_err());
    }
}
