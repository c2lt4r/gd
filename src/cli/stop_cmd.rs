use miette::{Result, miette};
use owo_colors::OwoColorize;

pub fn exec() -> Result<()> {
    // Try via daemon first (preferred — also clears debug server state)
    if let Some(result) =
        crate::lsp::daemon_client::query_daemon("debug_stop_game", serde_json::json!({}), None)
        && result.get("error").is_none()
    {
        let pid = result.get("pid").and_then(serde_json::Value::as_u64).unwrap_or(0);
        println!("{} Game stopped (pid {pid})", "✓".green());
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
        println!("{} Game stopped (pid {pid})", "✓".green());
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
/// tools to find and kill the actual Windows process by matching `--remote-debug`
/// in the command line. On native platforms, a regular kill/taskkill suffices.
pub fn kill_game_process(pid: u32) {
    #[cfg(unix)]
    {
        if crate::core::fs::is_wsl() {
            // Find the Windows PID of our game (has --remote-debug in command line)
            if let Some(win_pid) = find_windows_game_pid() {
                let _ = std::process::Command::new(TASKKILL)
                    .args(["/F", "/PID", &win_pid.to_string()])
                    .output();
            }
            // Also kill the WSL init shim
            let _ = std::process::Command::new("kill")
                .args(["-9", &pid.to_string()])
                .output();
        } else {
            let _ = std::process::Command::new("kill")
                .args([&pid.to_string()])
                .output();
        }
    }
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("taskkill")
            .args(["/F", "/PID", &pid.to_string()])
            .output();
    }
}

#[cfg(unix)]
const POWERSHELL: &str = "/mnt/c/Windows/System32/WindowsPowerShell/v1.0/powershell.exe";
#[cfg(unix)]
const TASKKILL: &str = "/mnt/c/Windows/System32/taskkill.exe";

/// Use PowerShell to find the Windows PID of the godot process launched with --remote-debug.
#[cfg(unix)]
fn find_windows_game_pid() -> Option<u32> {
    let output = std::process::Command::new(POWERSHELL)
        .args([
            "-NoProfile",
            "-Command",
            "Get-CimInstance Win32_Process -Filter \"Name='godot.exe'\" | \
             Where-Object { $_.CommandLine -like '*--remote-debug*' } | \
             Select-Object -ExpandProperty ProcessId",
        ])
        .output()
        .ok()?;
    let s = String::from_utf8_lossy(&output.stdout);
    s.trim().parse().ok()
}
