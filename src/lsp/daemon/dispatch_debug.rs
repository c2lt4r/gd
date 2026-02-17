use std::sync::Arc;
use std::time::Duration;

use super::helpers::{get_debug_server, json_to_variant, variant_set_field};
use super::{DaemonResponse, DaemonServer, error_response, ok_response};

pub fn dispatch_set_game_pid(server: &DaemonServer, params: &serde_json::Value) -> DaemonResponse {
    let Some(pid) = params.get("pid").and_then(serde_json::Value::as_u64) else {
        return error_response("missing 'pid' parameter");
    };
    server.set_game(pid as u32);
    ok_response(serde_json::json!({"pid": pid}))
}

pub fn dispatch_debug_stop_game(server: &DaemonServer) -> DaemonResponse {
    let Some(pid) = server.game_pid() else {
        return error_response("No game process tracked — was the game launched with `gd run`?");
    };

    crate::cli::stop_cmd::kill_game_process(pid);
    server.clear_game();

    ok_response(serde_json::json!({"stopped": true, "pid": pid}))
}

pub fn dispatch_debug_start_server(
    server: &DaemonServer,
    params: &serde_json::Value,
) -> DaemonResponse {
    let port = params
        .get("port")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(crate::debug::godot_debug_server::GodotDebugServer::DEFAULT_PORT as u64)
        as u16;

    // Reuse existing server if it's on the same port
    {
        let guard = server.debug_server.lock().unwrap();
        if let Some(existing) = guard.as_ref()
            && existing.port() == port
        {
            return ok_response(serde_json::json!({"port": port}));
        }
    }

    // Drop existing server first to release the old port
    *server.debug_server.lock().unwrap() = None;
    // Brief pause to let the OS release the port if needed
    std::thread::sleep(Duration::from_millis(50));

    let Some(ds) = crate::debug::godot_debug_server::GodotDebugServer::new(port) else {
        return error_response("Failed to create debug server (port may be in use)");
    };
    let actual_port = ds.port();
    *server.debug_server.lock().unwrap() = Some(Arc::new(ds));
    ok_response(serde_json::json!({"port": actual_port}))
}

pub fn dispatch_debug_accept(server: &DaemonServer, params: &serde_json::Value) -> DaemonResponse {
    let timeout = params
        .get("timeout")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(30);

    // Clone the Arc so we can release the mutex before blocking on accept.
    // This is critical — accept() can block for up to 30s and we must not
    // hold the lock during that time or all debug queries will time out.
    let ds = {
        let guard = server.debug_server.lock().unwrap();
        match guard.as_ref() {
            Some(ds) => Arc::clone(ds),
            None => {
                return error_response("No debug server running — call debug_start_server first");
            }
        }
    };

    let connected = ds.accept(Duration::from_secs(timeout));
    if connected {
        // Register a disconnect callback so the daemon learns when the game disconnects
        let game_state = Arc::clone(&server.game_state);
        let project_root = server.project_root.clone();
        ds.set_on_disconnect(move || {
            eprintln!("daemon: game disconnected via debug TCP");
            *game_state.lock().unwrap() = None;
            super::update_game_pid_in_state(&project_root, None);
        });
    }
    ok_response(serde_json::json!({"connected": connected}))
}

pub fn dispatch_debug_scene_tree(server: &DaemonServer) -> DaemonResponse {
    let Some(ds) = get_debug_server(server) else {
        return error_response("No debug server running");
    };
    match ds.cmd_request_scene_tree() {
        Some(tree) => ok_response(serde_json::to_value(&tree).unwrap_or_default()),
        None => error_response("scene tree request failed or timed out"),
    }
}

pub fn dispatch_debug_inspect(server: &DaemonServer, params: &serde_json::Value) -> DaemonResponse {
    let Some(object_id) = params.get("object_id").and_then(serde_json::Value::as_u64) else {
        return error_response("missing 'object_id' parameter");
    };
    let Some(ds) = get_debug_server(server) else {
        return error_response("No debug server running");
    };
    match ds.cmd_inspect_object(object_id) {
        Some(info) => ok_response(serde_json::to_value(&info).unwrap_or_default()),
        None => error_response(&format!(
            "object {object_id} not found — it may have been freed (try refreshing the scene tree)"
        )),
    }
}

pub fn dispatch_debug_set_property(
    server: &DaemonServer,
    params: &serde_json::Value,
) -> DaemonResponse {
    let Some(object_id) = params.get("object_id").and_then(serde_json::Value::as_u64) else {
        return error_response("missing 'object_id' parameter");
    };
    let Some(property) = params.get("property").and_then(|p| p.as_str()) else {
        return error_response("missing 'property' parameter");
    };
    let value_param = params
        .get("value")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let variant = json_to_variant(&value_param);

    let Some(ds) = get_debug_server(server) else {
        return error_response("No debug server running");
    };
    if ds.cmd_set_object_property(object_id, property, variant) {
        ok_response(serde_json::json!({"set": true}))
    } else {
        error_response("set_object_property failed")
    }
}

pub fn dispatch_debug_suspend(server: &DaemonServer, params: &serde_json::Value) -> DaemonResponse {
    let suspend = params
        .get("suspend")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(true);
    let Some(ds) = get_debug_server(server) else {
        return error_response("No debug server running");
    };
    if ds.cmd_suspend(suspend) {
        ok_response(serde_json::json!({"suspended": suspend}))
    } else {
        error_response("suspend command failed")
    }
}

pub fn dispatch_debug_next_frame(server: &DaemonServer) -> DaemonResponse {
    let Some(ds) = get_debug_server(server) else {
        return error_response("No debug server running");
    };
    if ds.cmd_next_frame() {
        ok_response(serde_json::json!({"advanced": true}))
    } else {
        error_response("next_frame command failed")
    }
}

pub fn dispatch_debug_time_scale(
    server: &DaemonServer,
    params: &serde_json::Value,
) -> DaemonResponse {
    let Some(scale) = params.get("scale").and_then(serde_json::Value::as_f64) else {
        return error_response("missing 'scale' parameter");
    };
    let Some(ds) = get_debug_server(server) else {
        return error_response("No debug server running");
    };
    if ds.cmd_set_speed(scale) {
        ok_response(serde_json::json!({"scale": scale}))
    } else {
        error_response("set_speed command failed")
    }
}

pub fn dispatch_debug_reload_scripts(
    server: &DaemonServer,
    params: &serde_json::Value,
) -> DaemonResponse {
    let Some(ds) = get_debug_server(server) else {
        return error_response("No debug server running");
    };
    let paths: Vec<String> = params
        .get("paths")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    if paths.is_empty() {
        // No specific paths -> reload all scripts unconditionally
        if ds.cmd_reload_all_scripts() {
            ok_response(serde_json::json!({"reloaded": true, "mode": "all"}))
        } else {
            error_response("reload_all_scripts command failed")
        }
    } else if ds.cmd_reload_scripts(&paths) {
        ok_response(serde_json::json!({"reloaded": true, "mode": "selective", "paths": paths}))
    } else {
        error_response("reload_scripts command failed")
    }
}

pub fn dispatch_debug_server_status(server: &DaemonServer) -> DaemonResponse {
    match get_debug_server(server) {
        Some(ds) => ok_response(serde_json::json!({
            "running": true,
            "port": ds.port(),
            "connected": ds.is_connected(),
        })),
        None => ok_response(serde_json::json!({"running": false})),
    }
}

/// Fast breakpoint state check -- reads an atomic flag, no network round-trip.
pub fn dispatch_debug_is_at_breakpoint(server: &DaemonServer) -> DaemonResponse {
    let Some(ds) = get_debug_server(server) else {
        return error_response("No debug server running");
    };
    ok_response(serde_json::json!({"at_breakpoint": ds.is_at_breakpoint()}))
}

/// Simple execution control command (continue/break/next/step/out).
pub fn dispatch_debug_cmd_simple(server: &DaemonServer, action: &str) -> DaemonResponse {
    let Some(ds) = get_debug_server(server) else {
        return error_response("No debug server running");
    };
    let ok = match action {
        "continue" => ds.cmd_continue(),
        "break" => ds.cmd_break(),
        "next" => ds.cmd_next(),
        "step" => ds.cmd_step(),
        "out" => ds.cmd_out(),
        _ => false,
    };
    if ok {
        ok_response(serde_json::json!({"ok": true}))
    } else {
        error_response(&format!("{action} failed"))
    }
}

/// Simple no-arg command that returns success/failure.
pub fn dispatch_debug_simple(server: &DaemonServer, label: &str) -> DaemonResponse {
    let Some(ds) = get_debug_server(server) else {
        return error_response("No debug server running");
    };
    let ok = match label {
        "reload_all_scripts" => ds.cmd_reload_all_scripts(),
        "clear_selection" => ds.cmd_clear_selection(),
        "node_select_reset_2d" => ds.cmd_runtime_node_select_reset_camera_2d(),
        "node_select_reset_3d" => ds.cmd_runtime_node_select_reset_camera_3d(),
        _ => false,
    };
    if ok {
        ok_response(serde_json::json!({"ok": true}))
    } else {
        error_response(&format!("{label} failed"))
    }
}

pub fn dispatch_debug_breakpoint(
    server: &DaemonServer,
    params: &serde_json::Value,
) -> DaemonResponse {
    let Some(path) = params.get("path").and_then(|p| p.as_str()) else {
        return error_response("missing 'path' parameter");
    };
    let Some(line) = params.get("line").and_then(serde_json::Value::as_u64) else {
        return error_response("missing 'line' parameter");
    };
    let enabled = params
        .get("enabled")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(true);
    let Some(ds) = get_debug_server(server) else {
        return error_response("No debug server running");
    };
    if ds.cmd_breakpoint(path, line as u32, enabled) {
        ok_response(serde_json::json!({"ok": true}))
    } else {
        error_response("breakpoint command failed")
    }
}

/// Boolean command helper -- dispatches to set_skip_breakpoints or set_ignore_error_breaks.
pub fn dispatch_debug_bool_cmd(
    server: &DaemonServer,
    label: &str,
    params: &serde_json::Value,
) -> DaemonResponse {
    let Some(value) = params.get("value").and_then(serde_json::Value::as_bool) else {
        return error_response("missing 'value' parameter");
    };
    let Some(ds) = get_debug_server(server) else {
        return error_response("No debug server running");
    };
    let ok = match label {
        "skip_breakpoints" => ds.cmd_set_skip_breakpoints(value),
        "ignore_error_breaks" => ds.cmd_set_ignore_error_breaks(value),
        _ => false,
    };
    if ok {
        ok_response(serde_json::json!({"ok": true}))
    } else {
        error_response(&format!("{label} failed"))
    }
}

pub fn dispatch_debug_get_stack_dump(server: &DaemonServer) -> DaemonResponse {
    let Some(ds) = get_debug_server(server) else {
        return error_response("No debug server running");
    };
    match ds.cmd_get_stack_dump() {
        Some(frames) => ok_response(serde_json::to_value(&frames).unwrap_or_default()),
        None => error_response("get_stack_dump failed or timed out"),
    }
}

pub fn dispatch_debug_get_stack_frame_vars(
    server: &DaemonServer,
    params: &serde_json::Value,
) -> DaemonResponse {
    let Some(frame) = params.get("frame").and_then(serde_json::Value::as_u64) else {
        return error_response("missing 'frame' parameter");
    };
    let Some(ds) = get_debug_server(server) else {
        return error_response("No debug server running");
    };
    match ds.cmd_get_stack_frame_vars(frame as u32) {
        Some(vars) => ok_response(serde_json::to_value(&vars).unwrap_or_default()),
        None => error_response("get_stack_frame_vars failed or timed out"),
    }
}

pub fn dispatch_debug_evaluate(
    server: &DaemonServer,
    params: &serde_json::Value,
) -> DaemonResponse {
    let Some(expression) = params.get("expression").and_then(|e| e.as_str()) else {
        return error_response("missing 'expression' parameter");
    };
    let frame = params
        .get("frame")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let Some(ds) = get_debug_server(server) else {
        return error_response("No debug server running");
    };
    match ds.cmd_evaluate(expression, frame as u32) {
        Some(result) => ok_response(serde_json::to_value(&result).unwrap_or_default()),
        None => error_response("evaluate failed or timed out"),
    }
}

pub fn dispatch_debug_inspect_objects(
    server: &DaemonServer,
    params: &serde_json::Value,
) -> DaemonResponse {
    let Some(ids_arr) = params.get("ids").and_then(|i| i.as_array()) else {
        return error_response("missing 'ids' parameter");
    };
    let ids: Vec<u64> = ids_arr
        .iter()
        .filter_map(serde_json::Value::as_u64)
        .collect();
    let selection = params
        .get("selection")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let Some(ds) = get_debug_server(server) else {
        return error_response("No debug server running");
    };
    match ds.cmd_inspect_objects(&ids, selection) {
        Some(results) => ok_response(serde_json::to_value(&results).unwrap_or_default()),
        None => error_response("inspect_objects failed"),
    }
}

pub fn dispatch_debug_save_node(
    server: &DaemonServer,
    params: &serde_json::Value,
) -> DaemonResponse {
    let Some(object_id) = params.get("object_id").and_then(serde_json::Value::as_u64) else {
        return error_response("missing 'object_id' parameter");
    };
    let Some(path) = params.get("path").and_then(|p| p.as_str()) else {
        return error_response("missing 'path' parameter");
    };
    let Some(ds) = get_debug_server(server) else {
        return error_response("No debug server running");
    };
    match ds.cmd_save_node(object_id, path) {
        Some(saved_path) => ok_response(serde_json::json!({"saved": saved_path})),
        None => error_response("save_node failed"),
    }
}

pub fn dispatch_debug_set_property_field(
    server: &DaemonServer,
    params: &serde_json::Value,
) -> DaemonResponse {
    let Some(object_id) = params.get("object_id").and_then(serde_json::Value::as_u64) else {
        return error_response("missing 'object_id' parameter");
    };
    let Some(property) = params.get("property").and_then(|p| p.as_str()) else {
        return error_response("missing 'property' parameter");
    };
    let Some(field) = params.get("field").and_then(|f| f.as_str()) else {
        return error_response("missing 'field' parameter");
    };
    let value_param = params
        .get("value")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let new_field_val = json_to_variant(&value_param);
    let Some(ds) = get_debug_server(server) else {
        return error_response("No debug server running");
    };

    // Godot's fieldwise_assign casts the value to the property's type, so passing a
    // scalar (e.g. Float(7.0)) for a Vector3 sub-field silently fails.
    // Fix: inspect -> modify sub-field client-side -> set the full property value.
    let Some(info) = ds.cmd_inspect_object(object_id) else {
        return error_response("failed to inspect object — is the game running?");
    };
    let Some(prop) = info.properties.iter().find(|p| p.name == property) else {
        return error_response(&format!(
            "property '{property}' not found on object {object_id}"
        ));
    };
    let mut current = prop.value.clone();
    if !variant_set_field(&mut current, field, &new_field_val) {
        return error_response(&format!("cannot set field '{field}' on {current:?}"));
    }
    if ds.cmd_set_object_property(object_id, property, current) {
        ok_response(serde_json::json!({"ok": true}))
    } else {
        error_response("set_object_property failed")
    }
}

/// Integer parameter command helper (node selection type/mode).
pub fn dispatch_debug_int_cmd(
    server: &DaemonServer,
    label: &str,
    params: &serde_json::Value,
    param_name: &str,
) -> DaemonResponse {
    let Some(value) = params.get(param_name).and_then(serde_json::Value::as_i64) else {
        return error_response(&format!("missing '{param_name}' parameter"));
    };
    let Some(ds) = get_debug_server(server) else {
        return error_response("No debug server running");
    };
    let ok = match label {
        "node_select_type" => ds.cmd_runtime_node_select_set_type(value as i32),
        "node_select_mode" => ds.cmd_runtime_node_select_set_mode(value as i32),
        _ => false,
    };
    if ok {
        ok_response(serde_json::json!({"ok": true}))
    } else {
        error_response(&format!("{label} failed"))
    }
}

/// Boolean parameter command helper (node selection bools).
pub fn dispatch_debug_bool_param(
    server: &DaemonServer,
    label: &str,
    params: &serde_json::Value,
    param_name: &str,
) -> DaemonResponse {
    let Some(value) = params.get(param_name).and_then(serde_json::Value::as_bool) else {
        return error_response(&format!("missing '{param_name}' parameter"));
    };
    let Some(ds) = get_debug_server(server) else {
        return error_response("No debug server running");
    };
    let ok = match label {
        "node_select_visible" => ds.cmd_runtime_node_select_set_visible(value),
        "node_select_avoid_locked" => ds.cmd_runtime_node_select_set_avoid_locked(value),
        "node_select_prefer_group" => ds.cmd_runtime_node_select_set_prefer_group(value),
        _ => false,
    };
    if ok {
        ok_response(serde_json::json!({"ok": true}))
    } else {
        error_response(&format!("{label} failed"))
    }
}

pub fn dispatch_output_capture_start(server: &DaemonServer) -> DaemonResponse {
    let Some(ds) = get_debug_server(server) else {
        return error_response("No debug server running");
    };
    ds.start_output_capture();
    ok_response(serde_json::json!({"capturing": true}))
}

pub fn dispatch_output_capture_drain(server: &DaemonServer) -> DaemonResponse {
    let Some(ds) = get_debug_server(server) else {
        return error_response("No debug server running");
    };
    let output = ds.drain_output();
    ok_response(serde_json::json!({"output": output}))
}

pub fn dispatch_log_query(server: &DaemonServer, params: &serde_json::Value) -> DaemonResponse {
    let Some(ds) = get_debug_server(server) else {
        return error_response("No debug server running");
    };
    let after_seq = params
        .get("after_seq")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let count = params
        .get("count")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0) as usize;
    let type_filter = params
        .get("type_filter")
        .and_then(serde_json::Value::as_str);
    let entries = ds.query_log(after_seq, count, type_filter);
    ok_response(serde_json::json!({"entries": entries}))
}

pub fn dispatch_log_clear(server: &DaemonServer) -> DaemonResponse {
    let Some(ds) = get_debug_server(server) else {
        return error_response("No debug server running");
    };
    ds.clear_log();
    ok_response(serde_json::json!({"cleared": true}))
}
