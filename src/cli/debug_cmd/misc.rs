use miette::{Result, miette};
use owo_colors::OwoColorize;

use super::args::{
    MuteAudioArgs, OutputFormat, OverrideCameraArgs, ProfilerArgs, ReloadCachedArgs, SaveNodeArgs,
    ServerArgs,
};
use super::{daemon_cmd, daemon_cmd_timeout, ensure_binary_debug};

// ── One-shot: mute-audio ────────────────────────────────────────────

pub(crate) fn cmd_mute_audio(args: &MuteAudioArgs) -> Result<()> {
    ensure_binary_debug()?;
    let mute = !args.off;
    daemon_cmd("debug_mute_audio", serde_json::json!({"mute": mute}))
        .ok_or_else(|| miette!("Failed — is a game running?"))?;
    match args.format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({"muted": mute})).unwrap()
            );
        }
        OutputFormat::Text => {
            if mute {
                println!("{}", "Audio muted".green());
            } else {
                println!("{}", "Audio unmuted".green());
            }
        }
    }
    Ok(())
}

// ── One-shot: override-camera ───────────────────────────────────────

pub(crate) fn cmd_override_camera(args: &OverrideCameraArgs) -> Result<()> {
    ensure_binary_debug()?;
    let enable = !args.off;
    daemon_cmd(
        "debug_override_cameras",
        serde_json::json!({"enable": enable}),
    )
    .ok_or_else(|| miette!("Failed — is a game running?"))?;
    match args.format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({"override": enable})).unwrap()
            );
        }
        OutputFormat::Text => {
            if enable {
                println!("{}", "Camera override enabled".green());
            } else {
                println!("{}", "Camera override disabled".green());
            }
        }
    }
    Ok(())
}

// ── One-shot: save-node ─────────────────────────────────────────────

pub(crate) fn cmd_save_node(args: &SaveNodeArgs) -> Result<()> {
    ensure_binary_debug()?;
    daemon_cmd(
        "debug_save_node",
        serde_json::json!({"object_id": args.id, "path": args.path}),
    )
    .ok_or_else(|| miette!("Failed to save node {} — is a game running?", args.id))?;
    match args.format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "saved": true,
                    "object_id": args.id,
                    "path": args.path,
                }))
                .unwrap()
            );
        }
        OutputFormat::Text => {
            println!(
                "{} node {} to {}",
                "Saved".green(),
                format!("[{}]", args.id).dimmed(),
                args.path.cyan(),
            );
        }
    }
    Ok(())
}

// ── One-shot: profiler ──────────────────────────────────────────────

pub(crate) fn cmd_profiler(args: &ProfilerArgs) -> Result<()> {
    ensure_binary_debug()?;
    let enable = !args.off;
    daemon_cmd(
        "debug_toggle_profiler",
        serde_json::json!({"profiler": args.name, "enable": enable}),
    )
    .ok_or_else(|| miette!("Failed — is a game running?"))?;
    match args.format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "profiler": args.name,
                    "enabled": enable,
                }))
                .unwrap()
            );
        }
        OutputFormat::Text => {
            if enable {
                println!("{} profiler {}", "Enabled".green(), args.name.cyan());
            } else {
                println!("{} profiler {}", "Disabled".green(), args.name.cyan());
            }
        }
    }
    Ok(())
}

// ── File management (binary protocol) ───────────────────────────────

pub(crate) fn cmd_reload_cached(args: &ReloadCachedArgs) -> Result<()> {
    ensure_binary_debug()?;
    let count = args.file.len();
    daemon_cmd(
        "debug_reload_cached_files",
        serde_json::json!({"files": args.file}),
    )
    .ok_or_else(|| miette!("Failed — is a game running?"))?;
    match args.format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({"ok": true, "count": count}))
                    .unwrap()
            );
        }
        OutputFormat::Text => {
            println!("{}", format!("Reloaded {count} cached files").green());
        }
    }
    Ok(())
}

// ── Server command ───────────────────────────────────────────────────

pub(crate) fn cmd_server(args: &ServerArgs) -> Result<()> {
    // Check if already running
    if let Some(status) = daemon_cmd("debug_server_status", serde_json::json!({}))
        && status.get("running").and_then(serde_json::Value::as_bool) == Some(true)
    {
        let port = status
            .get("port")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        let connected = status.get("connected").and_then(serde_json::Value::as_bool) == Some(true);
        if connected {
            println!(
                "{} port {} (game connected)",
                "Debug server already running on".green().bold(),
                port.to_string().cyan(),
            );
            return Ok(());
        }
        println!(
            "{} port {} (waiting for game)",
            "Debug server already running on".yellow().bold(),
            port.to_string().cyan(),
        );
        if !args.wait {
            print_launch_hint(port);
            return Ok(());
        }
        // Fall through to wait
        let accept = daemon_cmd_timeout(
            "debug_accept",
            serde_json::json!({"timeout": args.timeout}),
            args.timeout + 5,
        );
        if let Some(r) = accept
            && r.get("connected").and_then(serde_json::Value::as_bool) == Some(true)
        {
            println!("{}", "Game connected!".green().bold());
            return Ok(());
        }
        return Err(miette!("Timed out waiting for game to connect"));
    }

    // Start the server
    let result = daemon_cmd("debug_start_server", serde_json::json!({"port": args.port}))
        .ok_or_else(|| miette!("Failed to start debug server (daemon not available)"))?;
    let port = result
        .get("port")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);

    println!(
        "{} port {}",
        "Debug server started on".green().bold(),
        port.to_string().cyan(),
    );
    print_launch_hint(port);

    if args.wait {
        println!("{}", "Waiting for game to connect...".dimmed());
        let accept = daemon_cmd_timeout(
            "debug_accept",
            serde_json::json!({"timeout": args.timeout}),
            args.timeout + 5,
        );
        if let Some(r) = accept
            && r.get("connected").and_then(serde_json::Value::as_bool) == Some(true)
        {
            println!("{}", "Game connected!".green().bold());
            return Ok(());
        }
        return Err(miette!("Timed out waiting for game to connect"));
    }

    Ok(())
}

/// Print a copy-pasteable Godot launch command with platform-correct project path.
pub(crate) fn print_launch_hint(port: u64) {
    let project_path = std::env::current_dir()
        .ok()
        .and_then(|cwd| crate::core::config::find_project_root(&cwd))
        .map(|root| {
            let s = root.to_string_lossy();
            crate::core::fs::wsl_to_windows_path(&s).unwrap_or_else(|| s.to_string())
        });

    if let Some(path) = project_path {
        println!(
            "Launch game with:\n  {} {} {} {}",
            "godot".bold(),
            format!("--remote-debug \"tcp://127.0.0.1:{port}\"").cyan(),
            "--path".bold(),
            format!("\"{path}\"").cyan(),
        );
    } else {
        println!(
            "Launch game with:\n  {} {}",
            "godot".bold(),
            format!("--remote-debug \"tcp://127.0.0.1:{port}\"").cyan(),
        );
    }
}
