use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
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

/// Result from an eval execution, including any captured print output.
pub struct EvalResponse {
    /// The return value of the eval script (may be empty for void calls).
    pub result: String,
    /// Output captured from print()/push_error()/push_warning() during execution.
    pub output: Vec<CapturedOutput>,
}

/// Re-export for callers.
pub use gd_core::debug_types::CapturedOutput;

/// Send a GDScript to the live eval server and return the result string.
/// Captures output non-blockingly (instant drain, ~2ms). For void calls that need
/// to wait for Godot's output flush, use `send_eval_with_output` instead.
pub fn send_eval(script: &str, project_root: &Path, timeout: Duration) -> Result<EvalResponse> {
    send_eval_tcp(script, project_root, timeout, false)
}

/// Like `send_eval` but also captures print output via the debug protocol.
/// For void calls (empty result), polls up to ~1.5s for Godot to flush output.
/// Use this for interactive REPL, not for automation commands.
pub fn send_eval_with_output(
    script: &str,
    project_root: &Path,
    timeout: Duration,
) -> Result<EvalResponse> {
    send_eval_tcp(script, project_root, timeout, true)
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
    let _ = gd_lsp::daemon_client::query_daemon(
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
        // Drain output on error — may contain compilation error details from Godot
        let error_output = drain_output_quick();
        let mut error_msg = eval_result.error.clone();
        for line in &error_output {
            if line.r#type == "error" {
                if !error_msg.ends_with('\n') {
                    error_msg.push('\n');
                }
                error_msg.push_str(&line.message);
            }
        }
        return Err(miette!("{}", error_msg));
    }

    let result_str = eval_result.result.unwrap_or_default();

    // Drain captured output. Godot batches print() via the debug protocol (~1s).
    // - poll_output + void result: poll up to 1.5s (REPL: user expects to see print output)
    // - otherwise: single instant drain (non-blocking, captures anything already arrived)
    // Filter to only include output between eval markers (strips game log noise).
    let output = if poll_output && result_str.is_empty() {
        filter_eval_output(drain_output_with_poll(Duration::from_millis(1500)))
    } else {
        filter_eval_output(drain_output_quick())
    };

    Ok(EvalResponse {
        result: result_str,
        output,
    })
}

/// Markers printed by the GDScript eval server around `run()` execution.
/// Used to filter captured output so only eval-originated prints are shown.
const EVAL_OUTPUT_BEGIN: &str = "__GD_EVAL_BEGIN__";
const EVAL_OUTPUT_END: &str = "__GD_EVAL_END__";

/// Filter captured output to only include lines between eval markers.
/// This strips game log messages (from other nodes) that aren't from the eval script.
fn filter_eval_output(output: Vec<CapturedOutput>) -> Vec<CapturedOutput> {
    let mut inside = false;
    let mut result = Vec::new();
    for line in output {
        if line.message.contains(EVAL_OUTPUT_BEGIN) {
            inside = true;
            continue;
        }
        if line.message.contains(EVAL_OUTPUT_END) {
            inside = false;
            continue;
        }
        if inside {
            result.push(line);
        }
    }
    result
}

/// Single instant drain — no waiting. Returns whatever output has already arrived.
fn drain_output_quick() -> Vec<CapturedOutput> {
    gd_lsp::daemon_client::query_daemon(
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
        let output = gd_lsp::daemon_client::query_daemon(
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
    if let Some(r) = gd_lsp::daemon_client::query_daemon(
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
            "Invalid eval ready file format (expected pid:port).\n\
             Try restarting the game: gd run"
        ));
    };
    let pid: u32 = pid_str.parse().unwrap_or(0);
    if pid == 0 || !gd_lsp::daemon::is_process_alive(pid) {
        return Err(miette!("No eval server running. Start a game with: gd run"));
    }
    let port: u16 = port_str
        .parse()
        .map_err(|_| miette!("Invalid port in eval ready file"))?;
    Ok(SocketAddr::from(([127, 0, 0, 1], port)))
}

// ── Shared helpers ─────────────────────────────────────────────────────

/// Try to recover from a debug break: check if paused, grab stack, continue, return error.
/// Returns `Some(error_description)` if the game was paused on a debug error/breakpoint.
fn try_recover_debug_break() -> Option<String> {
    let timeout = Some(Duration::from_secs(3));

    let bp = gd_lsp::daemon_client::query_daemon(
        "debug_is_at_breakpoint",
        serde_json::json!({}),
        timeout,
    )?;
    if bp.get("at_breakpoint").and_then(serde_json::Value::as_bool) != Some(true) {
        return None;
    }

    // Use the reason from debug_enter if available (e.g. "Breakpoint", error text)
    let reason = bp
        .get("reason")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    let mut msg = if reason.is_empty() {
        String::from("GDScript error paused the game")
    } else {
        format!("GDScript error: {reason}")
    };
    if let Some(stack) =
        gd_lsp::daemon_client::query_daemon("debug_get_stack_dump", serde_json::json!({}), timeout)
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

    // Include any captured error output (runtime error details from Godot)
    let error_output: Vec<CapturedOutput> =
        gd_lsp::daemon_client::query_daemon("output_capture_drain", serde_json::json!({}), timeout)
            .and_then(|r| r.get("output").cloned())
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default();
    for line in &error_output {
        if line.r#type == "error" {
            if !msg.ends_with('\n') {
                msg.push('\n');
            }
            msg.push_str(&line.message);
        }
    }

    let _ = gd_lsp::daemon_client::query_daemon("debug_continue", serde_json::json!({}), timeout);

    Some(msg.trim_end().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(err.to_string().contains("Invalid eval ready file format"));
    }

    #[test]
    fn parse_ready_file_tcp_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let err = parse_ready_file_tcp(tmp.path()).unwrap_err();
        assert!(err.to_string().contains("No eval server running"));
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

    #[test]
    fn filter_eval_output_strips_game_logs() {
        let output = vec![
            CapturedOutput {
                message: "[ClientController] frame=466500".to_string(),
                r#type: "log".to_string(),
            },
            CapturedOutput {
                message: "__GD_EVAL_BEGIN__".to_string(),
                r#type: "log".to_string(),
            },
            CapturedOutput {
                message: "hello from eval".to_string(),
                r#type: "log".to_string(),
            },
            CapturedOutput {
                message: "__GD_EVAL_END__".to_string(),
                r#type: "log".to_string(),
            },
            CapturedOutput {
                message: "[LagCompensator] tick=9743".to_string(),
                r#type: "log".to_string(),
            },
        ];
        let filtered = filter_eval_output(output);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].message, "hello from eval");
    }

    #[test]
    fn filter_eval_output_no_markers_returns_empty() {
        let output = vec![CapturedOutput {
            message: "[ClientController] frame=100".to_string(),
            r#type: "log".to_string(),
        }];
        let filtered = filter_eval_output(output);
        assert!(filtered.is_empty());
    }

    #[test]
    fn filter_eval_output_empty_eval() {
        let output = vec![
            CapturedOutput {
                message: "__GD_EVAL_BEGIN__".to_string(),
                r#type: "log".to_string(),
            },
            CapturedOutput {
                message: "__GD_EVAL_END__".to_string(),
                r#type: "log".to_string(),
            },
        ];
        let filtered = filter_eval_output(output);
        assert!(filtered.is_empty());
    }
}
