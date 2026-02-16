use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::time::Duration;

/// Outcome of sending a query to the daemon.
enum SendResult {
    /// Daemon returned a successful result.
    Ok(serde_json::Value),
    /// Daemon returned an error message (daemon is alive, just can't fulfill request).
    DaemonError(String),
    /// Could not connect to or communicate with the daemon.
    ConnectionFailed,
}

/// Send a daemon request and return the result (or None on failure).
/// Automatically spawns the daemon if it's not running.
#[allow(clippy::needless_pass_by_value)]
pub fn query_daemon(
    method: &str,
    params: serde_json::Value,
    timeout: Option<Duration>,
) -> Option<serde_json::Value> {
    let cwd = std::env::current_dir().ok()?;
    let project_root = crate::core::config::find_project_root(&cwd)?;
    query_daemon_with_root(&project_root, method, &params, timeout)
}

fn query_daemon_with_root(
    project_root: &Path,
    method: &str,
    params: &serde_json::Value,
    timeout: Option<Duration>,
) -> Option<serde_json::Value> {
    let timeout = timeout.unwrap_or(Duration::from_secs(5));

    // Try existing daemon first
    if let Some(state) = super::daemon::read_state_file(project_root)
        && is_pid_alive(state.pid)
    {
        // Check if daemon was built from the same binary — auto-restart if stale
        let current_id = super::daemon::current_build_id();
        if !state.build_id.is_empty() && state.build_id != current_id {
            kill_daemon(state.pid, state.port, project_root);
        } else {
            match send_query(state.port, method, params, timeout) {
                SendResult::Ok(result) => return Some(result),
                SendResult::DaemonError(msg) => {
                    // Daemon is alive but returned an error — don't delete state file
                    // or spawn a new daemon. Log the error for debugging.
                    eprintln!("daemon: {msg}");
                    return None;
                }
                SendResult::ConnectionFailed => {
                    // Connection actually failed — daemon may have crashed, clean up
                    let _ =
                        std::fs::remove_file(project_root.join(".godot").join("gd-daemon.json"));
                }
            }
        }
    }

    // Spawn a new daemon and retry
    if spawn_daemon(project_root).is_err() {
        return None;
    }

    // Wait for daemon to be ready (up to 3s)
    let start = std::time::Instant::now();
    while start.elapsed() < Duration::from_secs(3) {
        std::thread::sleep(Duration::from_millis(100));
        if let Some(state) = super::daemon::read_state_file(project_root)
            && is_pid_alive(state.pid)
        {
            match send_query(state.port, method, params, timeout) {
                SendResult::Ok(result) => return Some(result),
                SendResult::DaemonError(_) | SendResult::ConnectionFailed => return None,
            }
        }
    }

    None
}

fn send_query(
    port: u16,
    method: &str,
    params: &serde_json::Value,
    timeout: Duration,
) -> SendResult {
    let Ok(addr) = format!("127.0.0.1:{port}").parse() else {
        return SendResult::ConnectionFailed;
    };
    let Ok(mut stream) = TcpStream::connect_timeout(&addr, Duration::from_secs(2)) else {
        return SendResult::ConnectionFailed;
    };

    if stream.set_read_timeout(Some(timeout)).is_err()
        || stream
            .set_write_timeout(Some(Duration::from_secs(5)))
            .is_err()
    {
        return SendResult::ConnectionFailed;
    }

    // Send request
    let request = serde_json::json!({"method": method, "params": params});
    let Ok(body) = serde_json::to_string(&request) else {
        return SendResult::ConnectionFailed;
    };
    let header = format!("Content-Length: {}\r\n\r\n", body.len());
    if stream.write_all(header.as_bytes()).is_err()
        || stream.write_all(body.as_bytes()).is_err()
        || stream.flush().is_err()
    {
        return SendResult::ConnectionFailed;
    }

    // Read response
    let mut reader = BufReader::new(stream);
    let Some(content_length) = read_content_length(&mut reader) else {
        return SendResult::ConnectionFailed;
    };
    let mut response_body = vec![0u8; content_length];
    if reader.read_exact(&mut response_body).is_err() {
        return SendResult::ConnectionFailed;
    }

    let Ok(response): std::result::Result<serde_json::Value, _> =
        serde_json::from_slice(&response_body)
    else {
        return SendResult::ConnectionFailed;
    };

    // Check for daemon error response
    if let Some(error) = response.get("error").and_then(|e| e.as_str()) {
        return SendResult::DaemonError(error.to_string());
    }

    match response.get("result").cloned() {
        Some(val) => SendResult::Ok(val),
        None => SendResult::ConnectionFailed,
    }
}

/// Stop the daemon for a project. Returns true if a daemon was stopped.
pub fn stop_daemon(project_root: &Path) -> bool {
    if let Some(state) = super::daemon::read_state_file(project_root)
        && is_pid_alive(state.pid)
    {
        kill_daemon(state.pid, state.port, project_root);
        true
    } else {
        false
    }
}

/// Send shutdown to a daemon, wait for it to die, then clean up.
fn kill_daemon(pid: u32, port: u16, project_root: &Path) {
    // Try graceful shutdown first
    let _ = send_query(
        port,
        "shutdown",
        &serde_json::json!({}),
        Duration::from_secs(2),
    );

    // Wait up to 1s for graceful exit
    for _ in 0..10 {
        if !is_pid_alive(pid) {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    // Force kill if still alive
    if is_pid_alive(pid) {
        #[cfg(unix)]
        {
            let _ = std::process::Command::new("kill")
                .args(["-9", &pid.to_string()])
                .output();
        }
        #[cfg(windows)]
        {
            let _ = std::process::Command::new("taskkill")
                .args(["/F", "/PID", &pid.to_string()])
                .output();
        }
        // Wait for force-kill to take effect and port to be released
        for _ in 0..10 {
            if !is_pid_alive(pid) {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    let _ = std::fs::remove_file(project_root.join(".godot").join("gd-daemon.json"));
}

fn spawn_daemon(project_root: &Path) -> std::io::Result<()> {
    let exe = std::env::current_exe()?;
    let root_str = project_root.to_string_lossy().to_string();

    let mut cmd = std::process::Command::new(exe);
    cmd.args(["daemon", "serve", "--project-root", &root_str]);
    cmd.stdin(std::process::Stdio::null());
    cmd.stdout(std::process::Stdio::null());
    cmd.stderr(std::process::Stdio::null());

    // Detach the process so it survives after parent exits
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        const DETACHED_PROCESS: u32 = 0x00000008;
        cmd.creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS);
    }

    let mut child = cmd.spawn()?;
    // Spawn a thread to reap the child so we don't get a zombie
    std::thread::spawn(move || {
        let _ = child.wait();
    });

    Ok(())
}

/// Check if a process is alive using OS-level APIs.
fn is_pid_alive(pid: u32) -> bool {
    super::daemon::is_process_alive(pid)
}

fn read_content_length(reader: &mut impl BufRead) -> Option<usize> {
    let mut line = String::new();
    loop {
        line.clear();
        if reader.read_line(&mut line).ok()? == 0 {
            return None;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return None;
        }
        if let Some(len_str) = trimmed.strip_prefix("Content-Length:") {
            let len: usize = len_str.trim().parse().ok()?;
            loop {
                line.clear();
                if reader.read_line(&mut line).ok()? == 0 {
                    return Some(len);
                }
                if line.trim().is_empty() {
                    return Some(len);
                }
            }
        }
    }
}
