use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use miette::{Result, miette};
use serde::Deserialize;

/// JSON result from the eval server.
#[derive(Debug, Deserialize)]
struct LiveEvalResult {
    result: Option<String>,
    error: String,
}

/// Monotonic counter to ensure unique eval IDs even within the same millisecond.
static EVAL_COUNTER: AtomicU32 = AtomicU32::new(0);

/// Generate a unique eval ID: timestamp_millis + monotonic counter + pid.
fn generate_eval_id() -> String {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let seq = EVAL_COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    format!("{ts}-{pid}-{seq}")
}

/// Clean up stale eval files from `.godot/` (request files, result files, ready marker).
/// Called when the daemon detects the game has exited.
pub fn cleanup_stale_eval_files(project_root: &Path) {
    let godot_dir = project_root.join(".godot");
    let _ = std::fs::remove_file(godot_dir.join("gd-eval-ready"));
    // Remove any lingering request/result files
    if let Ok(entries) = std::fs::read_dir(&godot_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with("gd-eval-request-") || name.starts_with("gd-eval-result-") {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
}

/// Send a GDScript to the live eval server and return the result string.
/// Returns `Err` if no eval server is running or the script fails.
pub fn send_eval(script: &str, project_root: &Path, timeout: Duration) -> Result<String> {
    // 1. Syntax check + sanitize escape sequences
    let script = crate::cli::eval_cmd::pre_check(script)?;

    // 2. Check if eval server is ready — try daemon first, fall back to ready file
    let godot_dir = project_root.join(".godot");
    let ready_path = godot_dir.join("gd-eval-ready");

    let eval_ready = crate::lsp::daemon_client::query_daemon(
        "eval_status",
        serde_json::json!({"timeout": timeout.as_secs()}),
        Some(timeout + Duration::from_secs(5)),
    )
    .and_then(|r| r.get("ready").and_then(serde_json::Value::as_bool))
    .unwrap_or(false);

    if !eval_ready {
        // Daemon says not ready (or unreachable) — check ready file as fallback,
        // but verify the PID inside is still alive to avoid stale files
        if !is_ready_file_valid(&ready_path) {
            return Err(miette!(
                "No eval server running. Start a game with: gd run"
            ));
        }
    }

    // 3. Clean up any stale request/result files from previous failed evals.
    //    If a prior eval timed out and the delete raced with Godot reading the file,
    //    the stale request stays behind. The eval server processes requests in directory
    //    order, so a stale file would be picked up instead of our new request, writing
    //    a result with the wrong eval_id — causing a cascading timeout.
    purge_stale_eval_files(&godot_dir);

    // 4. Generate a unique request ID (timestamp + pid + counter — no collisions)
    let eval_id = generate_eval_id();
    let tagged_script = format!("# eval-id: {eval_id}\n{script}");

    // Write to per-ID request file so concurrent evals don't overwrite each other
    let request_path = godot_dir.join(format!("gd-eval-request-{eval_id}.gd"));
    std::fs::write(&request_path, &tagged_script)
        .map_err(|e| miette!("Failed to write eval request: {e}"))?;

    // 5. Poll for the ID-specific result file
    let result_path = godot_dir.join(format!("gd-eval-result-{eval_id}.json"));
    let poll_interval = Duration::from_millis(50);
    let start = std::time::Instant::now();

    loop {
        if result_path.is_file() {
            let data = std::fs::read_to_string(&result_path)
                .map_err(|e| miette!("Failed to read eval result: {e}"))?;
            let _ = std::fs::remove_file(&result_path);

            let eval_result: LiveEvalResult = serde_json::from_str(&data)
                .map_err(|e| miette!("Failed to parse eval result: {e}"))?;

            if !eval_result.error.is_empty() {
                return Err(miette!("{}", eval_result.error));
            }
            return Ok(eval_result.result.unwrap_or_default());
        }

        if !godot_dir.join("gd-eval-ready").is_file() {
            let _ = std::fs::remove_file(&request_path);
            return Err(miette!("Eval server exited before returning a result"));
        }

        if start.elapsed() >= timeout {
            // Check if the request file was consumed (Godot picked it up)
            let consumed = !request_path.is_file();
            let _ = std::fs::remove_file(&request_path);
            if consumed {
                return Err(miette!(
                    "Timed out waiting for eval result ({}s)\n\
                     The eval server picked up the request but never returned a result.\n\
                     The game may need to be restarted: gd run",
                    timeout.as_secs()
                ));
            }
            return Err(miette!(
                "Timed out waiting for eval result ({}s)\n\
                 The eval server did not pick up the request file.\n\
                 Try restarting the game: gd run",
                timeout.as_secs()
            ));
        }

        std::thread::sleep(poll_interval);
    }
}

/// Check if the `gd-eval-ready` file is valid (PID inside is still alive).
fn is_ready_file_valid(ready_path: &Path) -> bool {
    let Ok(content) = std::fs::read_to_string(ready_path) else {
        return false;
    };
    let pid: u32 = content.trim().parse().unwrap_or(0);
    if pid == 0 {
        return false;
    }
    crate::lsp::daemon::is_process_alive(pid)
}

/// Remove any lingering request and result files from `.godot/`.
/// This prevents stale files from earlier timed-out evals from being
/// processed before new requests (the eval server picks files in
/// directory order, not creation order).
fn purge_stale_eval_files(godot_dir: &Path) {
    if let Ok(entries) = std::fs::read_dir(godot_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with("gd-eval-request-") || name.starts_with("gd-eval-result-") {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
}
