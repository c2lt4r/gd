use miette::{Result, miette};
use owo_colors::OwoColorize;

use super::args::{NodeSelectIntArgs, OutputFormat, StepArgs, ToggleFmtArgs};
use super::{daemon_cmd, ensure_binary_debug};
use gd_core::cprintln;

// ── Node selection (binary protocol) ────────────────────────────────

pub(crate) fn cmd_node_select_type(args: &NodeSelectIntArgs) -> Result<()> {
    ensure_binary_debug()?;
    daemon_cmd(
        "debug_node_select_set_type",
        serde_json::json!({"type": args.value}),
    )
    .ok_or_else(|| miette!("Failed — is a game running?"))?;
    match args.format {
        OutputFormat::Json => {
            cprintln!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({"ok": true, "type": args.value}))
                    .unwrap()
            );
        }
        OutputFormat::Text => {
            cprintln!(
                "{}",
                format!("Node select type set to {}", args.value).green()
            );
        }
    }
    Ok(())
}

pub(crate) fn cmd_node_select_mode(args: &NodeSelectIntArgs) -> Result<()> {
    ensure_binary_debug()?;
    daemon_cmd(
        "debug_node_select_set_mode",
        serde_json::json!({"mode": args.value}),
    )
    .ok_or_else(|| miette!("Failed — is a game running?"))?;
    match args.format {
        OutputFormat::Json => {
            cprintln!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({"ok": true, "mode": args.value}))
                    .unwrap()
            );
        }
        OutputFormat::Text => {
            cprintln!(
                "{}",
                format!("Node select mode set to {}", args.value).green()
            );
        }
    }
    Ok(())
}

pub(crate) fn cmd_node_select_visible(args: &ToggleFmtArgs) -> Result<()> {
    ensure_binary_debug()?;
    let visible = !args.off;
    daemon_cmd(
        "debug_node_select_set_visible",
        serde_json::json!({"visible": visible}),
    )
    .ok_or_else(|| miette!("Failed — is a game running?"))?;
    match args.format {
        OutputFormat::Json => {
            cprintln!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({"ok": true, "visible": visible}))
                    .unwrap()
            );
        }
        OutputFormat::Text => {
            if visible {
                cprintln!("{}", "Node visibility filter enabled".green());
            } else {
                cprintln!("{}", "Node visibility filter disabled".green());
            }
        }
    }
    Ok(())
}

pub(crate) fn cmd_node_select_avoid_locked(args: &ToggleFmtArgs) -> Result<()> {
    ensure_binary_debug()?;
    let avoid = !args.off;
    daemon_cmd(
        "debug_node_select_set_avoid_locked",
        serde_json::json!({"avoid": avoid}),
    )
    .ok_or_else(|| miette!("Failed — is a game running?"))?;
    match args.format {
        OutputFormat::Json => {
            cprintln!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({"ok": true, "avoid": avoid}))
                    .unwrap()
            );
        }
        OutputFormat::Text => {
            if avoid {
                cprintln!("{}", "Avoid locked nodes enabled".green());
            } else {
                cprintln!("{}", "Avoid locked nodes disabled".green());
            }
        }
    }
    Ok(())
}

pub(crate) fn cmd_node_select_prefer_group(args: &ToggleFmtArgs) -> Result<()> {
    ensure_binary_debug()?;
    let prefer = !args.off;
    daemon_cmd(
        "debug_node_select_set_prefer_group",
        serde_json::json!({"prefer": prefer}),
    )
    .ok_or_else(|| miette!("Failed — is a game running?"))?;
    match args.format {
        OutputFormat::Json => {
            cprintln!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({"ok": true, "prefer": prefer}))
                    .unwrap()
            );
        }
        OutputFormat::Text => {
            if prefer {
                cprintln!("{}", "Prefer group enabled".green());
            } else {
                cprintln!("{}", "Prefer group disabled".green());
            }
        }
    }
    Ok(())
}

pub(crate) fn cmd_node_select_reset_cam_2d(args: &StepArgs) -> Result<()> {
    ensure_binary_debug()?;
    daemon_cmd("debug_node_select_reset_camera_2d", serde_json::json!({}))
        .ok_or_else(|| miette!("Failed — is a game running?"))?;
    match args.format {
        OutputFormat::Json => {
            cprintln!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({"ok": true})).unwrap()
            );
        }
        OutputFormat::Text => cprintln!("{}", "2D selection camera reset".green()),
    }
    Ok(())
}

pub(crate) fn cmd_node_select_reset_cam_3d(args: &StepArgs) -> Result<()> {
    ensure_binary_debug()?;
    daemon_cmd("debug_node_select_reset_camera_3d", serde_json::json!({}))
        .ok_or_else(|| miette!("Failed — is a game running?"))?;
    match args.format {
        OutputFormat::Json => {
            cprintln!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({"ok": true})).unwrap()
            );
        }
        OutputFormat::Text => cprintln!("{}", "3D selection camera reset".green()),
    }
    Ok(())
}

pub(crate) fn cmd_clear_selection(args: &StepArgs) -> Result<()> {
    ensure_binary_debug()?;
    daemon_cmd("debug_clear_selection", serde_json::json!({}))
        .ok_or_else(|| miette!("Failed — is a game running?"))?;
    match args.format {
        OutputFormat::Json => {
            cprintln!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({"ok": true})).unwrap()
            );
        }
        OutputFormat::Text => cprintln!("{}", "Selection cleared".green()),
    }
    Ok(())
}
