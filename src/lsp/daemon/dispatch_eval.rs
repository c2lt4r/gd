use std::time::{Duration, Instant};

use super::{DaemonResponse, DaemonServer, error_response, ok_response};

/// Called by `gd run --eval` to tell the daemon that eval mode is active.
pub fn dispatch_set_eval_mode(server: &DaemonServer, params: &serde_json::Value) -> DaemonResponse {
    let enabled = params
        .get("enabled")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(true);
    server
        .eval_mode
        .store(enabled, std::sync::atomic::Ordering::Release);
    ok_response(serde_json::json!({"eval_mode": enabled}))
}

/// Called by `gd eval` to check if the eval server is ready.
/// Blocks until the ready file appears or the timeout expires.
pub fn dispatch_eval_status(server: &DaemonServer, params: &serde_json::Value) -> DaemonResponse {
    let eval_active = server.eval_mode.load(std::sync::atomic::Ordering::Acquire);
    let game_running = server.is_game_running();

    if !eval_active && !game_running {
        return ok_response(serde_json::json!({
            "eval_mode": false,
            "ready": false,
        }));
    }

    let ready_path = server.project_root.join(".godot").join("gd-eval-ready");

    // If already ready, return immediately
    if ready_path.is_file() {
        return ok_response(serde_json::json!({
            "eval_mode": eval_active,
            "ready": true,
        }));
    }

    // Poll with timeout (default 30s, configurable via params)
    let timeout_secs = params
        .get("timeout")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(30);
    let timeout = Duration::from_secs(timeout_secs);
    let poll_interval = Duration::from_millis(200);
    let start = Instant::now();

    loop {
        if ready_path.is_file() {
            return ok_response(serde_json::json!({
                "eval_mode": true,
                "ready": true,
            }));
        }

        if start.elapsed() >= timeout {
            return error_response("Eval server did not start in time");
        }

        std::thread::sleep(poll_interval);
    }
}
