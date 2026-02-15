use std::path::Path;
use std::time::Duration;

use miette::{Result, miette};
use serde::Deserialize;

/// JSON result from the eval server.
#[derive(Debug, Deserialize)]
struct LiveEvalResult {
    result: Option<String>,
    error: String,
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

    if !eval_ready && !ready_path.is_file() {
        return Err(miette!(
            "No eval server running. Start a game with: gd run --eval"
        ));
    }

    // 3. Generate a unique request ID
    let eval_id = format!(
        "{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
    );
    let tagged_script = format!("# eval-id: {eval_id}\n{script}");

    let request_path = godot_dir.join("gd-eval-request.gd");
    std::fs::write(&request_path, &tagged_script)
        .map_err(|e| miette!("Failed to write eval request: {e}"))?;

    // 4. Poll for the ID-specific result file
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
            let _ = std::fs::remove_file(&request_path);
            return Err(miette!(
                "Timed out waiting for eval result ({}s)",
                timeout.as_secs()
            ));
        }

        std::thread::sleep(poll_interval);
    }
}
