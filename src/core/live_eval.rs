use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
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

/// Result from an eval execution, including any captured print output.
pub struct EvalResponse {
    /// The return value of the eval script (may be empty for void calls).
    pub result: String,
    /// Output captured from print()/push_error()/push_warning() during execution.
    pub output: Vec<CapturedOutput>,
}

/// Re-export for callers.
pub use crate::debug::godot_debug_server::CapturedOutput;

/// Send a GDScript to the live eval server and return the result string.
/// Uses TCP by default, falls back to file-based IPC when `GD_EVAL_FILE_IPC` is set.
/// Captures output non-blockingly (instant drain, ~2ms). For void calls that need
/// to wait for Godot's output flush, use `send_eval_with_output` instead.
pub fn send_eval(script: &str, project_root: &Path, timeout: Duration) -> Result<EvalResponse> {
    if is_file_ipc_mode() {
        send_eval_file(script, project_root, timeout).map(|result| EvalResponse {
            result,
            output: vec![],
        })
    } else {
        send_eval_tcp(script, project_root, timeout, false)
    }
}

/// Like `send_eval` but also captures print output via the debug protocol.
/// For void calls (empty result), polls up to ~1.5s for Godot to flush output.
/// Use this for interactive REPL, not for automation commands.
pub fn send_eval_with_output(
    script: &str,
    project_root: &Path,
    timeout: Duration,
) -> Result<EvalResponse> {
    if is_file_ipc_mode() {
        send_eval_file(script, project_root, timeout).map(|result| EvalResponse {
            result,
            output: vec![],
        })
    } else {
        send_eval_tcp(script, project_root, timeout, true)
    }
}

/// Check if file-based IPC mode is requested via environment variable.
fn is_file_ipc_mode() -> bool {
    std::env::var("GD_EVAL_FILE_IPC").is_ok()
}

// ── TCP transport ──────────────────────────────────────────────────────

/// Send eval via TCP: connect to eval server, write script, read result.
/// Always starts output capture via the daemon's debug protocol.
/// When `poll_output` is true, polls for output on void results (up to ~1.5s).
/// When false, does a single instant drain (~2ms, non-blocking).
fn send_eval_tcp(
    script: &str,
    project_root: &Path,
    timeout: Duration,
    poll_output: bool,
) -> Result<EvalResponse> {
    let script = crate::cli::eval_cmd::pre_check(script)?;

    // Always start output capture — the overhead is ~2ms (one daemon query)
    let _ = crate::lsp::daemon_client::query_daemon(
        "output_capture_start",
        serde_json::json!({}),
        Some(Duration::from_secs(2)),
    );

    let addr = get_eval_address(project_root, timeout)?;

    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_secs(3))
        .map_err(|e| miette!("Cannot connect to eval server: {e}"))?;
    stream
        .set_read_timeout(Some(timeout))
        .map_err(|e| miette!("Failed to set read timeout: {e}"))?;
    stream
        .set_write_timeout(Some(Duration::from_secs(5)))
        .map_err(|e| miette!("Failed to set write timeout: {e}"))?;

    // Write: [4 bytes LE length][script bytes]
    let script_bytes = script.as_bytes();
    stream
        .write_all(&(script_bytes.len() as u32).to_le_bytes())
        .map_err(|e| miette!("Failed to send eval request: {e}"))?;
    stream
        .write_all(script_bytes)
        .map_err(|e| miette!("Failed to send eval script: {e}"))?;

    // Read: [4 bytes LE length][json bytes]
    let mut len_buf = [0u8; 4];
    if let Err(e) = stream.read_exact(&mut len_buf) {
        if e.kind() == std::io::ErrorKind::TimedOut || e.kind() == std::io::ErrorKind::WouldBlock {
            // Timeout reading result — check for debug break
            if let Some(msg) = try_recover_debug_break() {
                return Err(miette!("Eval script error (game auto-resumed):\n{msg}"));
            }
            return Err(miette!(
                "Timed out waiting for eval result ({}s)\n\
                 The eval server accepted the connection but never returned a result.\n\
                 The game may need to be restarted: gd run",
                timeout.as_secs()
            ));
        }
        return Err(miette!("Failed to read eval result: {e}"));
    }
    let json_len = u32::from_le_bytes(len_buf) as usize;
    let mut json_buf = vec![0u8; json_len];
    stream
        .read_exact(&mut json_buf)
        .map_err(|e| miette!("Failed to read eval result body: {e}"))?;

    let data =
        String::from_utf8(json_buf).map_err(|e| miette!("Invalid UTF-8 in eval result: {e}"))?;
    let eval_result: LiveEvalResult =
        serde_json::from_str(&data).map_err(|e| miette!("Failed to parse eval result: {e}"))?;

    if !eval_result.error.is_empty() {
        // Drain output even on error (there may be prints before the error)
        let _ = crate::lsp::daemon_client::query_daemon(
            "output_capture_drain",
            serde_json::json!({}),
            Some(Duration::from_secs(1)),
        );
        return Err(miette!("{}", eval_result.error));
    }

    let result_str = eval_result.result.unwrap_or_default();

    // Drain captured output. Godot batches print() via the debug protocol (~1s).
    // - poll_output + void result: poll up to 1.5s (REPL: user expects to see print output)
    // - otherwise: single instant drain (non-blocking, captures anything already arrived)
    let output = if poll_output && result_str.is_empty() {
        drain_output_with_poll(Duration::from_millis(1500))
    } else {
        drain_output_quick()
    };

    Ok(EvalResponse {
        result: result_str,
        output,
    })
}

/// Single instant drain — no waiting. Returns whatever output has already arrived.
fn drain_output_quick() -> Vec<CapturedOutput> {
    crate::lsp::daemon_client::query_daemon(
        "output_capture_drain",
        serde_json::json!({}),
        Some(Duration::from_secs(1)),
    )
    .and_then(|r| r.get("output").cloned())
    .and_then(|v| serde_json::from_value::<Vec<CapturedOutput>>(v).ok())
    .unwrap_or_default()
}

/// Poll the daemon for captured output, waiting up to `max_wait` for messages to arrive.
/// Returns as soon as output is found, or after the timeout with whatever was captured.
fn drain_output_with_poll(max_wait: Duration) -> Vec<CapturedOutput> {
    let start = std::time::Instant::now();
    let poll_interval = Duration::from_millis(150);

    loop {
        let output = crate::lsp::daemon_client::query_daemon(
            "output_capture_drain",
            serde_json::json!({}),
            Some(Duration::from_secs(2)),
        )
        .and_then(|r| r.get("output").cloned())
        .and_then(|v| serde_json::from_value::<Vec<CapturedOutput>>(v).ok())
        .unwrap_or_default();

        if !output.is_empty() || start.elapsed() >= max_wait {
            return output;
        }

        std::thread::sleep(poll_interval);
    }
}

/// Resolve the eval server's TCP address from daemon or ready file.
fn get_eval_address(project_root: &Path, timeout: Duration) -> Result<SocketAddr> {
    // Try daemon first (may have port cached)
    if let Some(r) = crate::lsp::daemon_client::query_daemon(
        "eval_status",
        serde_json::json!({"timeout": timeout.as_secs()}),
        Some(timeout + Duration::from_secs(5)),
    ) && r.get("ready") == Some(&serde_json::Value::Bool(true))
        && let Some(port) = r.get("port").and_then(serde_json::Value::as_u64)
    {
        return Ok(SocketAddr::from(([127, 0, 0, 1], port as u16)));
    }

    // Fall back to reading ready file directly
    parse_ready_file_tcp(project_root)
}

/// Parse the ready file for TCP mode: expects `{pid}:{port}`.
fn parse_ready_file_tcp(project_root: &Path) -> Result<SocketAddr> {
    let ready_path = project_root.join(".godot").join("gd-eval-ready");
    let content = std::fs::read_to_string(&ready_path)
        .map_err(|_| miette!("No eval server running. Start a game with: gd run"))?;
    let trimmed = content.trim();
    let Some((pid_str, port_str)) = trimmed.split_once(':') else {
        return Err(miette!(
            "Eval server is running in file-IPC mode.\n\
             Set GD_EVAL_FILE_IPC=1 or restart the game without --file-ipc"
        ));
    };
    let pid: u32 = pid_str.parse().unwrap_or(0);
    if pid == 0 || !crate::lsp::daemon::is_process_alive(pid) {
        return Err(miette!("No eval server running. Start a game with: gd run"));
    }
    let port: u16 = port_str
        .parse()
        .map_err(|_| miette!("Invalid port in eval ready file"))?;
    Ok(SocketAddr::from(([127, 0, 0, 1], port)))
}

// ── File-based transport (legacy) ──────────────────────────────────────

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

/// Send eval via file-based IPC (legacy mode for `--file-ipc` / `GD_EVAL_FILE_IPC`).
fn send_eval_file(script: &str, project_root: &Path, timeout: Duration) -> Result<String> {
    let script = crate::cli::eval_cmd::pre_check(script)?;

    let godot_dir = project_root.join(".godot");
    let ready_path = godot_dir.join("gd-eval-ready");

    let eval_ready = crate::lsp::daemon_client::query_daemon(
        "eval_status",
        serde_json::json!({"timeout": timeout.as_secs()}),
        Some(timeout + Duration::from_secs(5)),
    )
    .and_then(|r| r.get("ready").and_then(serde_json::Value::as_bool))
    .unwrap_or(false);

    if !eval_ready && !is_ready_file_valid(&ready_path) {
        return Err(miette!("No eval server running. Start a game with: gd run"));
    }

    purge_stale_eval_files(&godot_dir);

    let eval_id = generate_eval_id();
    let tagged_script = format!("# eval-id: {eval_id}\n{script}");

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
            let consumed = !request_path.is_file();
            let _ = std::fs::remove_file(&request_path);

            if consumed {
                if let Some(error_msg) = try_recover_debug_break() {
                    return Err(miette!(
                        "Eval script error (game auto-resumed):\n{error_msg}"
                    ));
                }
                return Err(miette!(
                    "Timed out waiting for eval result ({}s)\n\
                     The eval server picked up the request but never returned a result.\n\
                     The game may need to be restarted: gd run",
                    timeout.as_secs()
                ));
            }

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

// ── Shared helpers ─────────────────────────────────────────────────────

/// Try to recover from a debug break: check if paused, grab stack, continue, return error.
/// Returns `Some(error_description)` if the game was paused on a debug error/breakpoint.
fn try_recover_debug_break() -> Option<String> {
    let timeout = Some(Duration::from_secs(3));

    let bp = crate::lsp::daemon_client::query_daemon(
        "debug_is_at_breakpoint",
        serde_json::json!({}),
        timeout,
    )?;
    if bp.get("at_breakpoint").and_then(serde_json::Value::as_bool) != Some(true) {
        return None;
    }

    let mut msg = String::from("GDScript error paused the game");
    if let Some(stack) = crate::lsp::daemon_client::query_daemon(
        "debug_get_stack_dump",
        serde_json::json!({}),
        timeout,
    ) && let Some(frames) = stack.as_array()
        && !frames.is_empty()
    {
        use std::fmt::Write;
        msg = String::new();
        for frame in frames {
            let file = frame
                .get("file")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("?");
            let line = frame
                .get("line")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0);
            let func = frame
                .get("function")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("?");
            let _ = writeln!(msg, "  {file}:{line} in {func}()");
        }
    }

    let _ =
        crate::lsp::daemon_client::query_daemon("debug_continue", serde_json::json!({}), timeout);

    Some(msg.trim_end().to_string())
}

/// Check if the `gd-eval-ready` file is valid (PID inside is still alive).
/// Handles both formats: `{pid}` (file-ipc) and `{pid}:{port}` (TCP).
fn is_ready_file_valid(ready_path: &Path) -> bool {
    let Ok(content) = std::fs::read_to_string(ready_path) else {
        return false;
    };
    let trimmed = content.trim();
    let pid_str = trimmed.split_once(':').map_or(trimmed, |(pid, _)| pid);
    let pid: u32 = pid_str.parse().unwrap_or(0);
    if pid == 0 {
        return false;
    }
    crate::lsp::daemon::is_process_alive(pid)
}

/// Remove stale request/result files from `.godot/` that are older than 30 seconds.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ready_file_valid_pid_only() {
        let tmp = tempfile::tempdir().unwrap();
        let ready = tmp.path().join("gd-eval-ready");
        let pid = std::process::id();
        std::fs::write(&ready, pid.to_string()).unwrap();
        assert!(is_ready_file_valid(&ready));
    }

    #[test]
    fn ready_file_valid_pid_port() {
        let tmp = tempfile::tempdir().unwrap();
        let ready = tmp.path().join("gd-eval-ready");
        let pid = std::process::id();
        std::fs::write(&ready, format!("{pid}:54321")).unwrap();
        assert!(is_ready_file_valid(&ready));
    }

    #[test]
    fn ready_file_invalid_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let ready = tmp.path().join("gd-eval-ready");
        std::fs::write(&ready, "").unwrap();
        assert!(!is_ready_file_valid(&ready));
    }

    #[test]
    fn ready_file_invalid_zero_pid() {
        let tmp = tempfile::tempdir().unwrap();
        let ready = tmp.path().join("gd-eval-ready");
        std::fs::write(&ready, "0").unwrap();
        assert!(!is_ready_file_valid(&ready));
    }

    #[test]
    fn ready_file_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let ready = tmp.path().join("gd-eval-ready");
        assert!(!is_ready_file_valid(&ready));
    }

    #[test]
    fn parse_ready_file_tcp_valid() {
        let tmp = tempfile::tempdir().unwrap();
        let godot_dir = tmp.path().join(".godot");
        std::fs::create_dir_all(&godot_dir).unwrap();
        let pid = std::process::id();
        std::fs::write(godot_dir.join("gd-eval-ready"), format!("{pid}:54321")).unwrap();
        let addr = parse_ready_file_tcp(tmp.path()).unwrap();
        assert_eq!(addr.port(), 54321);
        assert_eq!(addr.ip(), std::net::Ipv4Addr::LOCALHOST);
    }

    #[test]
    fn parse_ready_file_tcp_no_port() {
        let tmp = tempfile::tempdir().unwrap();
        let godot_dir = tmp.path().join(".godot");
        std::fs::create_dir_all(&godot_dir).unwrap();
        std::fs::write(godot_dir.join("gd-eval-ready"), "12345").unwrap();
        let err = parse_ready_file_tcp(tmp.path()).unwrap_err();
        assert!(err.to_string().contains("file-IPC mode"));
    }

    #[test]
    fn parse_ready_file_tcp_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let err = parse_ready_file_tcp(tmp.path()).unwrap_err();
        assert!(err.to_string().contains("No eval server running"));
    }

    #[test]
    fn file_ipc_mode_off_by_default() {
        // Ensure GD_EVAL_FILE_IPC is not accidentally set in test env
        unsafe { std::env::remove_var("GD_EVAL_FILE_IPC") };
        assert!(!is_file_ipc_mode());
    }

    #[test]
    fn tcp_frame_roundtrip() {
        // Verify our framing logic: 4-byte LE length + payload
        let payload = b"hello world";
        let len = (payload.len() as u32).to_le_bytes();
        let mut frame = Vec::new();
        frame.extend_from_slice(&len);
        frame.extend_from_slice(payload);

        // Decode
        let decoded_len = u32::from_le_bytes([frame[0], frame[1], frame[2], frame[3]]) as usize;
        assert_eq!(decoded_len, payload.len());
        assert_eq!(&frame[4..], payload);
    }
}
