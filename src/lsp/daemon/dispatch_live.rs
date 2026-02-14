use super::helpers::{get_debug_server, json_to_variant};
use super::{DaemonResponse, DaemonServer, error_response, ok_response};

pub fn dispatch_debug_live_set_root(
    server: &DaemonServer,
    params: &serde_json::Value,
) -> DaemonResponse {
    let Some(scene_path) = params.get("scene_path").and_then(|s| s.as_str()) else {
        return error_response("missing 'scene_path' parameter");
    };
    let Some(scene_file) = params.get("scene_file").and_then(|s| s.as_str()) else {
        return error_response("missing 'scene_file' parameter");
    };
    let Some(ds) = get_debug_server(server) else {
        return error_response("No debug server running");
    };
    if ds.cmd_live_set_root(scene_path, scene_file) {
        ok_response(serde_json::json!({"ok": true}))
    } else {
        error_response("live_set_root failed")
    }
}

/// Shared helper for live_node_path / live_res_path.
pub fn dispatch_debug_live_path(
    server: &DaemonServer,
    label: &str,
    params: &serde_json::Value,
) -> DaemonResponse {
    let Some(path) = params.get("path").and_then(|p| p.as_str()) else {
        return error_response("missing 'path' parameter");
    };
    let Some(id) = params.get("id").and_then(serde_json::Value::as_i64) else {
        return error_response("missing 'id' parameter");
    };
    let Some(ds) = get_debug_server(server) else {
        return error_response("No debug server running");
    };
    let ok = match label {
        "node_path" => ds.cmd_live_node_path(path, id as i32),
        "res_path" => ds.cmd_live_res_path(path, id as i32),
        _ => false,
    };
    if ok {
        ok_response(serde_json::json!({"ok": true}))
    } else {
        error_response(&format!("live_{label} failed"))
    }
}

/// Shared helper for live_node_prop / live_res_prop.
pub fn dispatch_debug_live_prop(
    server: &DaemonServer,
    label: &str,
    params: &serde_json::Value,
) -> DaemonResponse {
    let Some(id) = params.get("id").and_then(serde_json::Value::as_i64) else {
        return error_response("missing 'id' parameter");
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
    let ok = match label {
        "node_prop" => ds.cmd_live_node_prop(id as i32, property, variant),
        "res_prop" => ds.cmd_live_res_prop(id as i32, property, variant),
        _ => false,
    };
    if ok {
        ok_response(serde_json::json!({"ok": true}))
    } else {
        error_response(&format!("live_{label} failed"))
    }
}

/// Shared helper for live_node_prop_res / live_res_prop_res.
pub fn dispatch_debug_live_prop_res(
    server: &DaemonServer,
    label: &str,
    params: &serde_json::Value,
) -> DaemonResponse {
    let Some(id) = params.get("id").and_then(serde_json::Value::as_i64) else {
        return error_response("missing 'id' parameter");
    };
    let Some(property) = params.get("property").and_then(|p| p.as_str()) else {
        return error_response("missing 'property' parameter");
    };
    let Some(res_path) = params.get("res_path").and_then(|r| r.as_str()) else {
        return error_response("missing 'res_path' parameter");
    };
    let Some(ds) = get_debug_server(server) else {
        return error_response("No debug server running");
    };
    let ok = match label {
        "node_prop_res" => ds.cmd_live_node_prop_res(id as i32, property, res_path),
        "res_prop_res" => ds.cmd_live_res_prop_res(id as i32, property, res_path),
        _ => false,
    };
    if ok {
        ok_response(serde_json::json!({"ok": true}))
    } else {
        error_response(&format!("live_{label} failed"))
    }
}

/// Shared helper for live_node_call / live_res_call.
pub fn dispatch_debug_live_call(
    server: &DaemonServer,
    label: &str,
    params: &serde_json::Value,
) -> DaemonResponse {
    let Some(id) = params.get("id").and_then(serde_json::Value::as_i64) else {
        return error_response("missing 'id' parameter");
    };
    let Some(method) = params.get("method").and_then(|m| m.as_str()) else {
        return error_response("missing 'method' parameter");
    };
    let args: Vec<crate::debug::variant::GodotVariant> = params
        .get("args")
        .and_then(|a| a.as_array())
        .map(|arr| arr.iter().map(json_to_variant).collect())
        .unwrap_or_default();
    let Some(ds) = get_debug_server(server) else {
        return error_response("No debug server running");
    };
    let ok = match label {
        "node_call" => ds.cmd_live_node_call(id as i32, method, &args),
        "res_call" => ds.cmd_live_res_call(id as i32, method, &args),
        _ => false,
    };
    if ok {
        ok_response(serde_json::json!({"ok": true}))
    } else {
        error_response(&format!("live_{label} failed"))
    }
}

pub fn dispatch_debug_live_create_node(
    server: &DaemonServer,
    params: &serde_json::Value,
) -> DaemonResponse {
    let Some(parent) = params.get("parent").and_then(|p| p.as_str()) else {
        return error_response("missing 'parent' parameter");
    };
    let Some(class) = params.get("class").and_then(|c| c.as_str()) else {
        return error_response("missing 'class' parameter");
    };
    let Some(name) = params.get("name").and_then(|n| n.as_str()) else {
        return error_response("missing 'name' parameter");
    };
    let Some(ds) = get_debug_server(server) else {
        return error_response("No debug server running");
    };
    if ds.cmd_live_create_node(parent, class, name) {
        ok_response(serde_json::json!({"ok": true}))
    } else {
        error_response("live_create_node failed")
    }
}

pub fn dispatch_debug_live_instantiate_node(
    server: &DaemonServer,
    params: &serde_json::Value,
) -> DaemonResponse {
    let Some(parent) = params.get("parent").and_then(|p| p.as_str()) else {
        return error_response("missing 'parent' parameter");
    };
    let Some(scene) = params.get("scene").and_then(|s| s.as_str()) else {
        return error_response("missing 'scene' parameter");
    };
    let Some(name) = params.get("name").and_then(|n| n.as_str()) else {
        return error_response("missing 'name' parameter");
    };
    let Some(ds) = get_debug_server(server) else {
        return error_response("No debug server running");
    };
    if ds.cmd_live_instantiate_node(parent, scene, name) {
        ok_response(serde_json::json!({"ok": true}))
    } else {
        error_response("live_instantiate_node failed")
    }
}

/// Single path command helper (remove_node).
pub fn dispatch_debug_live_single_path(
    server: &DaemonServer,
    label: &str,
    params: &serde_json::Value,
) -> DaemonResponse {
    let Some(path) = params.get("path").and_then(|p| p.as_str()) else {
        return error_response("missing 'path' parameter");
    };
    let Some(ds) = get_debug_server(server) else {
        return error_response("No debug server running");
    };
    let ok = match label {
        "remove_node" => ds.cmd_live_remove_node(path),
        _ => false,
    };
    if ok {
        ok_response(serde_json::json!({"ok": true}))
    } else {
        error_response(&format!("live_{label} failed"))
    }
}

pub fn dispatch_debug_live_remove_and_keep(
    server: &DaemonServer,
    params: &serde_json::Value,
) -> DaemonResponse {
    let Some(path) = params.get("path").and_then(|p| p.as_str()) else {
        return error_response("missing 'path' parameter");
    };
    let Some(object_id) = params.get("object_id").and_then(serde_json::Value::as_u64) else {
        return error_response("missing 'object_id' parameter");
    };
    let Some(ds) = get_debug_server(server) else {
        return error_response("No debug server running");
    };
    if ds.cmd_live_remove_and_keep_node(path, object_id) {
        ok_response(serde_json::json!({"ok": true}))
    } else {
        error_response("live_remove_and_keep_node failed")
    }
}

pub fn dispatch_debug_live_restore_node(
    server: &DaemonServer,
    params: &serde_json::Value,
) -> DaemonResponse {
    let Some(object_id) = params.get("object_id").and_then(serde_json::Value::as_u64) else {
        return error_response("missing 'object_id' parameter");
    };
    let Some(path) = params.get("path").and_then(|p| p.as_str()) else {
        return error_response("missing 'path' parameter");
    };
    let Some(pos) = params.get("pos").and_then(serde_json::Value::as_i64) else {
        return error_response("missing 'pos' parameter");
    };
    let Some(ds) = get_debug_server(server) else {
        return error_response("No debug server running");
    };
    if ds.cmd_live_restore_node(object_id, path, pos as i32) {
        ok_response(serde_json::json!({"ok": true}))
    } else {
        error_response("live_restore_node failed")
    }
}

pub fn dispatch_debug_live_duplicate_node(
    server: &DaemonServer,
    params: &serde_json::Value,
) -> DaemonResponse {
    let Some(path) = params.get("path").and_then(|p| p.as_str()) else {
        return error_response("missing 'path' parameter");
    };
    let Some(new_name) = params.get("new_name").and_then(|n| n.as_str()) else {
        return error_response("missing 'new_name' parameter");
    };
    let Some(ds) = get_debug_server(server) else {
        return error_response("No debug server running");
    };
    if ds.cmd_live_duplicate_node(path, new_name) {
        ok_response(serde_json::json!({"ok": true}))
    } else {
        error_response("live_duplicate_node failed")
    }
}

pub fn dispatch_debug_live_reparent_node(
    server: &DaemonServer,
    params: &serde_json::Value,
) -> DaemonResponse {
    let Some(path) = params.get("path").and_then(|p| p.as_str()) else {
        return error_response("missing 'path' parameter");
    };
    let Some(new_parent) = params.get("new_parent").and_then(|n| n.as_str()) else {
        return error_response("missing 'new_parent' parameter");
    };
    let Some(new_name) = params.get("new_name").and_then(|n| n.as_str()) else {
        return error_response("missing 'new_name' parameter");
    };
    let Some(pos) = params.get("pos").and_then(serde_json::Value::as_i64) else {
        return error_response("missing 'pos' parameter");
    };
    let Some(ds) = get_debug_server(server) else {
        return error_response("No debug server running");
    };
    if ds.cmd_live_reparent_node(path, new_parent, new_name, pos as i32) {
        ok_response(serde_json::json!({"ok": true}))
    } else {
        error_response("live_reparent_node failed")
    }
}
