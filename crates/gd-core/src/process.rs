/// Kill the game process. On WSL, the Linux PID is an init shim — use
/// PowerShell to find the actual Windows process by command line args.
/// On native Linux, use SIGTERM then SIGKILL. On Windows, use `TerminateProcess`.
pub fn kill_game_process(pid: u32) {
    #[cfg(unix)]
    {
        if crate::fs::is_wsl() {
            // Kill the WSL init shim first
            // SAFETY: kill is a standard POSIX syscall with a valid pid.
            unsafe {
                libc::kill(pid as i32, libc::SIGTERM);
            }
            // Find and kill the Windows process via PowerShell command-line filtering
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
#[cfg(unix)]
const POWERSHELL: &str = "/mnt/c/Windows/System32/WindowsPowerShell/v1.0/powershell.exe";

/// Find the Windows PID of the game launched by `gd run`.
///
/// Filters godot.exe processes by command line — only our spawned game has
/// `--remote-debug` in its args, never the Godot editor.
/// Tries PowerShell (Win11+) then falls back to wmic (older Windows).
#[cfg(unix)]
fn find_windows_game_pid() -> Option<u32> {
    find_game_pid_powershell().or_else(find_game_pid_wmic)
}

#[cfg(unix)]
const WMIC: &str = "/mnt/c/Windows/System32/wbem/WMIC.exe";

#[cfg(unix)]
fn find_game_pid_powershell() -> Option<u32> {
    let output = std::process::Command::new(POWERSHELL)
        .args([
            "-NoProfile",
            "-Command",
            "Get-CimInstance Win32_Process -Filter \"Name='godot.exe' and CommandLine like '%--remote-debug%'\" | Select-Object -ExpandProperty ProcessId",
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if let Ok(pid) = line.trim().parse::<u32>() {
            return Some(pid);
        }
    }
    None
}

#[cfg(unix)]
fn find_game_pid_wmic() -> Option<u32> {
    let output = std::process::Command::new(WMIC)
        .args([
            "process",
            "where",
            "name='godot.exe' and commandline like '%--remote-debug%'",
            "get",
            "processid",
            "/VALUE",
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        let line = line.trim();
        if let Some(pid_str) = line.strip_prefix("ProcessId=")
            && let Ok(pid) = pid_str.trim().parse::<u32>()
        {
            return Some(pid);
        }
    }
    None
}
