use crate::cprintln;
use miette::{Result, miette};
use owo_colors::OwoColorize;

pub fn exec() -> Result<()> {
    // Try via daemon first (preferred — also clears debug server state)
    if let Some(result) =
        crate::lsp::daemon_client::query_daemon("debug_stop_game", serde_json::json!({}), None)
        && result.get("error").is_none()
    {
        let pid = result
            .get("pid")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        cprintln!("{} Game stopped (pid {pid})", "✓".green());
        return Ok(());
    }

    // Fallback: read game_pid from state file (daemon may have died)
    let cwd = std::env::current_dir().unwrap_or_default();
    let root = crate::core::config::find_project_root(&cwd)
        .ok_or_else(|| miette!("Not in a Godot project"))?;
    if let Some(state) = crate::lsp::daemon::read_state_file(&root)
        && let Some(pid) = state.game_pid
    {
        kill_pid(pid);
        // Clear the game_pid from state file
        crate::lsp::daemon::clear_game_pid_in_state(&root);
        cprintln!("{} Game stopped (pid {pid})", "✓".green());
        return Ok(());
    }

    Err(miette!(
        "No game process tracked — was the game launched with `gd run`?"
    ))
}

fn kill_pid(pid: u32) {
    kill_game_process(pid);
}

/// Kill the game process. On WSL, the Linux PID is an init shim — use Windows
/// tools to find and kill the actual Windows process. On native Linux, use
/// SIGTERM then SIGKILL. On Windows, use `TerminateProcess`.
pub fn kill_game_process(pid: u32) {
    #[cfg(unix)]
    {
        if crate::core::fs::is_wsl() {
            // Kill the WSL init shim first
            // SAFETY: kill is a standard POSIX syscall with a valid pid.
            unsafe {
                libc::kill(pid as i32, libc::SIGTERM);
            }
            // Also find and kill the Windows process via tasklist.exe + taskkill.exe
            if let Some(win_pid) = find_windows_game_pid() {
                let _ = std::process::Command::new(TASKKILL)
                    .args(["/F", "/PID", &win_pid.to_string()])
                    .output();
            }
        } else {
            // SAFETY: kill is a standard POSIX syscall with a valid pid.
            unsafe {
                libc::kill(pid as i32, libc::SIGTERM);
            }
            // Wait briefly, then SIGKILL if still alive
            std::thread::sleep(std::time::Duration::from_millis(500));
            // SAFETY: kill is a standard POSIX syscall with a valid pid.
            unsafe {
                libc::kill(pid as i32, libc::SIGKILL);
            }
        }
    }
    #[cfg(windows)]
    {
        // SAFETY: OpenProcess + TerminateProcess + CloseHandle are well-defined Win32 APIs.
        unsafe {
            let handle = windows_sys::Win32::System::Threading::OpenProcess(
                windows_sys::Win32::System::Threading::PROCESS_TERMINATE,
                0,
                pid,
            );
            if !handle.is_null() {
                windows_sys::Win32::System::Threading::TerminateProcess(handle, 1);
                windows_sys::Win32::Foundation::CloseHandle(handle);
            }
        }
    }
}

#[cfg(unix)]
const TASKKILL: &str = "/mnt/c/Windows/System32/taskkill.exe";

/// Use `tasklist.exe` to find the Windows PID of a running godot process.
/// Falls back from the slower PowerShell approach — `tasklist.exe` is always available.
#[cfg(unix)]
fn find_windows_game_pid() -> Option<u32> {
    let output = std::process::Command::new("/mnt/c/Windows/System32/tasklist.exe")
        .args(["/FI", "IMAGENAME eq godot.exe", "/FO", "CSV", "/NH"])
        .output()
        .ok()?;
    // Output format: "godot.exe","12345","Console","1","123,456 K"
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        let fields: Vec<&str> = line.split(',').collect();
        if fields.len() >= 2 {
            let pid_str = fields[1].trim_matches('"');
            if let Ok(pid) = pid_str.parse::<u32>() {
                return Some(pid);
            }
        }
    }
    None
}
