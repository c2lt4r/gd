use super::helpers::get_debug_server;
use super::{DaemonResponse, DaemonServer, error_response, ok_response};

pub fn dispatch_debug_mute_audio(
    server: &DaemonServer,
    params: &serde_json::Value,
) -> DaemonResponse {
    let Some(mute) = params.get("mute").and_then(serde_json::Value::as_bool) else {
        return error_response("missing 'mute' parameter");
    };
    let Some(ds) = get_debug_server(server) else {
        return error_response("No debug server running");
    };
    if ds.cmd_mute_audio(mute) {
        ok_response(serde_json::json!({"ok": true}))
    } else {
        error_response("mute_audio failed")
    }
}

pub fn dispatch_debug_reload_cached_files(
    server: &DaemonServer,
    params: &serde_json::Value,
) -> DaemonResponse {
    let Some(files_arr) = params.get("files").and_then(|f| f.as_array()) else {
        return error_response("missing 'files' parameter");
    };
    let files: Vec<&str> = files_arr.iter().filter_map(|v| v.as_str()).collect();
    let Some(ds) = get_debug_server(server) else {
        return error_response("No debug server running");
    };
    if ds.cmd_reload_cached_files(&files) {
        ok_response(serde_json::json!({"ok": true}))
    } else {
        error_response("reload_cached_files failed")
    }
}

pub fn dispatch_debug_override_cameras(
    server: &DaemonServer,
    params: &serde_json::Value,
) -> DaemonResponse {
    let Some(enable) = params.get("enable").and_then(serde_json::Value::as_bool) else {
        return error_response("missing 'enable' parameter");
    };
    let Some(ds) = get_debug_server(server) else {
        return error_response("No debug server running");
    };
    if ds.cmd_override_cameras(enable) {
        ok_response(serde_json::json!({"ok": true}))
    } else {
        error_response("override_cameras failed")
    }
}

pub fn dispatch_debug_transform_camera_2d(
    server: &DaemonServer,
    params: &serde_json::Value,
) -> DaemonResponse {
    let Some(arr) = params.get("transform").and_then(|t| t.as_array()) else {
        return error_response("missing 'transform' parameter");
    };
    if arr.len() != 6 {
        return error_response("'transform' must have 6 elements");
    }
    let mut transform = [0.0f64; 6];
    for (i, v) in arr.iter().enumerate() {
        transform[i] = v.as_f64().unwrap_or(0.0);
    }
    let Some(ds) = get_debug_server(server) else {
        return error_response("No debug server running");
    };
    if ds.cmd_transform_camera_2d(transform) {
        ok_response(serde_json::json!({"ok": true}))
    } else {
        error_response("transform_camera_2d failed")
    }
}

pub fn dispatch_debug_transform_camera_3d(
    server: &DaemonServer,
    params: &serde_json::Value,
) -> DaemonResponse {
    let Some(arr) = params.get("transform").and_then(|t| t.as_array()) else {
        return error_response("missing 'transform' parameter");
    };
    if arr.len() != 12 {
        return error_response("'transform' must have 12 elements");
    }
    let mut transform = [0.0f64; 12];
    for (i, v) in arr.iter().enumerate() {
        transform[i] = v.as_f64().unwrap_or(0.0);
    }
    let perspective = params
        .get("perspective")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(true);
    let Some(fov) = params.get("fov").and_then(serde_json::Value::as_f64) else {
        return error_response("missing 'fov' parameter");
    };
    let Some(near) = params.get("near").and_then(serde_json::Value::as_f64) else {
        return error_response("missing 'near' parameter");
    };
    let Some(far) = params.get("far").and_then(serde_json::Value::as_f64) else {
        return error_response("missing 'far' parameter");
    };
    let Some(ds) = get_debug_server(server) else {
        return error_response("No debug server running");
    };
    if ds.cmd_transform_camera_3d(transform, perspective, fov, near, far) {
        ok_response(serde_json::json!({"ok": true}))
    } else {
        error_response("transform_camera_3d failed")
    }
}

pub fn dispatch_debug_request_screenshot(
    server: &DaemonServer,
    params: &serde_json::Value,
) -> DaemonResponse {
    let Some(id) = params.get("id").and_then(serde_json::Value::as_u64) else {
        return error_response("missing 'id' parameter");
    };
    let Some(ds) = get_debug_server(server) else {
        return error_response("No debug server running");
    };
    match ds.cmd_request_screenshot(id) {
        Some(result) => {
            // Read the PNG file from Godot's temp dir and base64-encode it
            let file_path = crate::core::fs::windows_to_wsl_path(&result.path);
            match std::fs::read(&file_path) {
                Ok(bytes) => {
                    use base64::Engine;
                    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
                    // Clean up the temp file
                    let _ = std::fs::remove_file(&file_path);
                    ok_response(serde_json::json!({
                        "width": result.width,
                        "height": result.height,
                        "data": b64,
                        "format": "png",
                    }))
                }
                Err(e) => error_response(&format!(
                    "Screenshot captured ({}x{}) but failed to read {}: {e}",
                    result.width, result.height, file_path
                )),
            }
        }
        None => error_response("request_screenshot failed or timed out"),
    }
}

pub fn dispatch_debug_toggle_profiler(
    server: &DaemonServer,
    params: &serde_json::Value,
) -> DaemonResponse {
    let Some(profiler) = params.get("profiler").and_then(|p| p.as_str()) else {
        return error_response("missing 'profiler' parameter");
    };
    let Some(enable) = params.get("enable").and_then(serde_json::Value::as_bool) else {
        return error_response("missing 'enable' parameter");
    };
    let Some(ds) = get_debug_server(server) else {
        return error_response("No debug server running");
    };
    if ds.cmd_toggle_profiler(profiler, enable) {
        ok_response(serde_json::json!({"ok": true}))
    } else {
        error_response("toggle_profiler failed")
    }
}
