use std::path::Path;

use miette::{Result, miette};
use owo_colors::OwoColorize;

use crate::core::mesh::MeshState;
use crate::cprintln;

use super::{OutputFormat, ReplayArgs, project_root};

pub fn cmd_replay(args: &ReplayArgs) -> Result<()> {
    let path = Path::new(&args.file);
    if !path.exists() {
        return Err(miette!("Replay file not found: {}", args.file));
    }
    let content =
        std::fs::read_to_string(path).map_err(|e| miette!("Failed to read replay file: {e}"))?;

    let root = project_root()?;

    // Suppress recording during replay to avoid re-recording commands
    super::record::set_suppress(true);

    // Start with a placeholder state; the first `create` command reinitializes it.
    let mut state = MeshState::new("_replay_init");
    let mut results = Vec::new();
    let total_lines = content.lines().filter(|l| !l.trim().is_empty()).count();

    for (i, line) in content.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let cmd: serde_json::Value =
            serde_json::from_str(line).map_err(|e| miette!("Line {}: invalid JSON: {e}", i + 1))?;
        let cmd_type = cmd["command"]
            .as_str()
            .ok_or_else(|| miette!("Line {}: missing 'command' field", i + 1))?;

        if args.dry_run {
            if matches!(args.format, OutputFormat::Text) {
                cprintln!("  {} {}", format!("[{}/{}]", i + 1, total_lines).dimmed(), cmd_type.cyan());
            }
            results.push(serde_json::json!({
                "command": cmd_type,
                "ok": true,
                "dry_run": true,
            }));
            continue;
        }

        let result =
            super::batch::execute_with_spatial_checks(cmd_type, &cmd, i, &mut state, &root)?;

        if matches!(args.format, OutputFormat::Text) {
            let ok = result["ok"].as_bool().unwrap_or(false);
            let status = if ok {
                "ok".green().to_string()
            } else {
                "FAILED".red().to_string()
            };
            cprintln!(
                "  {} {} — {status}",
                format!("[{}/{}]", i + 1, total_lines).dimmed(),
                cmd_type
            );
        }

        results.push(result);
    }

    super::record::set_suppress(false);

    match args.format {
        OutputFormat::Json => {
            let output = serde_json::json!({
                "commands_run": results.len(),
                "dry_run": args.dry_run,
                "results": results,
            });
            cprintln!("{}", serde_json::to_string_pretty(&output).unwrap());
        }
        OutputFormat::Text => {
            if args.dry_run {
                cprintln!(
                    "Dry run: {} commands would be replayed",
                    total_lines.to_string().green()
                );
            } else {
                cprintln!(
                    "Replay complete: {} commands executed",
                    results.len().to_string().green()
                );
            }
        }
    }

    Ok(())
}
