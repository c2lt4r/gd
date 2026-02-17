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
            return Err(miette!("No eval server running. Start a game with: gd run"));
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

    // Write to per-ID request file so concurrent evals don't overwrite each other.
    // Retry once on ENOENT — WSL cross-filesystem writes to /mnt/c/ can transiently
    // fail under rapid I/O (e.g., navigate polling every 200ms).
    let request_path = godot_dir.join(format!("gd-eval-request-{eval_id}.gd"));
    if let Err(e) = std::fs::write(&request_path, &tagged_script) {
        if e.kind() == std::io::ErrorKind::NotFound {
            std::thread::sleep(Duration::from_millis(50));
            std::fs::write(&request_path, &tagged_script)
                .map_err(|e2| miette!("Failed to write eval request (retry): {e2}"))?;
        } else {
            return Err(miette!("Failed to write eval request: {e}"));
        }
    }

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
                // The eval server picked up the request but never returned a result.
                // Most likely cause: the eval script triggered a GDScript runtime error
                // and the debugger broke on it, freezing the main thread.
                // Try to grab the error from the debug stack and auto-continue.
                if let Some(error_msg) = try_recover_debug_break() {
                    return Err(miette!("Eval script error (game auto-resumed):\n{error_msg}"));
                }
                return Err(miette!(
                    "Timed out waiting for eval result ({}s)\n\
                     The eval server picked up the request but never returned a result.\n\
                     The game may need to be restarted: gd run",
                    timeout.as_secs()
                ));
            }

            // Request file was NOT consumed — eval server isn't scanning.
            // Could be a previous debug break freezing the main thread.
            if let Some(error_msg) = try_recover_debug_break() {
                return Err(miette!(
                    "Game was paused on a debug error (auto-resumed):\n{error_msg}\n\
                     Retry your command."
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

/// Try to recover from a debug break: check if paused, grab stack, continue, return error.
/// Returns `Some(error_description)` if the game was paused on a debug error/breakpoint.
fn try_recover_debug_break() -> Option<String> {
    let timeout = Some(Duration::from_secs(3));

    // Fast check: is the game paused at a breakpoint? (atomic flag, no network round-trip)
    let bp = crate::lsp::daemon_client::query_daemon(
        "debug_is_at_breakpoint",
        serde_json::json!({}),
        timeout,
    )?;
    if bp.get("at_breakpoint").and_then(serde_json::Value::as_bool) != Some(true) {
        return None;
    }

    // Game is paused — grab the stack dump for error context
    let mut msg = String::from("GDScript error paused the game");
    if let Some(stack) = crate::lsp::daemon_client::query_daemon(
        "debug_get_stack_dump",
        serde_json::json!({}),
        timeout,
    )
        && let Some(frames) = stack.as_array()
        && !frames.is_empty()
    {
        use std::fmt::Write;
        msg = String::new();
        for frame in frames {
            let file = frame
                .get("file")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("?");
            let line = frame.get("line").and_then(serde_json::Value::as_u64).unwrap_or(0);
            let func = frame
                .get("function")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("?");
            let _ = writeln!(msg, "  {file}:{line} in {func}()");
        }
    }

    // Resume the game so it doesn't stay frozen
    let _ = crate::lsp::daemon_client::query_daemon(
        "debug_continue",
        serde_json::json!({}),
        timeout,
    );

    Some(msg.trim_end().to_string())
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

/// Remove stale request/result files from `.godot/` that are older than 30 seconds.
/// Only purges old files to avoid deleting results from concurrent eval calls
/// (e.g., agent running navigate polls + describe in parallel).
fn purge_stale_eval_files(godot_dir: &Path) {
    let cutoff = std::time::Duration::from_secs(30);
    if let Ok(entries) = std::fs::read_dir(godot_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with("gd-eval-request-") || name.starts_with("gd-eval-result-") {
                let dominated = entry
                    .metadata()
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.elapsed().ok())
                    .is_some_and(|age| age > cutoff);
                if dominated {
                    let _ = std::fs::remove_file(entry.path());
                }
            }
        }
    }
}
