use miette::{Result, miette};
use owo_colors::OwoColorize;

use super::args::{
    IgnoreErrorsArgs, OutputFormat, ReloadScriptsArgs, SetPropArgs, SetPropFieldArgs,
    SkipBreakpointsArgs, StepArgs, SuspendArgs, TimeScaleArgs,
};
use super::camera::take_screenshot;
use super::{daemon_cmd, ensure_binary_debug};

// ── One-shot: set-prop ──────────────────────────────────────────────

pub(crate) fn cmd_set_prop(args: &SetPropArgs) -> Result<()> {
    ensure_binary_debug()?;
    let json_value: serde_json::Value = serde_json::from_str(&args.value)
        .unwrap_or_else(|_| serde_json::Value::String(args.value.clone()));

    let result = daemon_cmd(
        "debug_set_property",
        serde_json::json!({
            "object_id": args.id,
            "property": args.property,
            "value": json_value,
        }),
    )
    .ok_or_else(|| {
        miette!(
            "Failed to set property '{}' on object {} — is a game running with the binary debug protocol?",
            args.property,
            args.id
        )
    })?;

    match args.format {
        OutputFormat::Json => {
            if args.screenshot {
                let (w, h, path) = take_screenshot(None)?;
                let mut combined = result.clone();
                combined["screenshot"] = serde_json::json!({
                    "width": w, "height": h, "format": "png", "path": path,
                });
                println!("{}", serde_json::to_string_pretty(&combined).unwrap());
            } else {
                println!("{}", serde_json::to_string_pretty(&result).unwrap());
            }
        }
        OutputFormat::Human => {
            println!(
                "{} {}.{} = {}",
                "Set".green(),
                format!("[{}]", args.id).dimmed(),
                args.property.cyan(),
                args.value.green(),
            );
            if args.screenshot {
                let (_w, _h, path) = take_screenshot(None)?;
                println!("{path}");
            }
        }
    }
    Ok(())
}

// ── One-shot: suspend ───────────────────────────────────────────────

pub(crate) fn cmd_suspend(args: &SuspendArgs) -> Result<()> {
    ensure_binary_debug()?;
    let suspend = !args.off;
    let result =
        daemon_cmd("debug_suspend", serde_json::json!({"suspend": suspend})).ok_or_else(|| {
            miette!(
                "Failed to {} game — is a game running with the binary debug protocol?",
                if suspend { "suspend" } else { "resume" }
            )
        })?;

    match args.format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&result).unwrap());
        }
        OutputFormat::Human => {
            if suspend {
                println!("{}", "Game suspended".green());
            } else {
                println!("{}", "Game resumed".green());
            }
        }
    }
    Ok(())
}

// ── One-shot: next-frame ────────────────────────────────────────────

pub(crate) fn cmd_next_frame(args: &StepArgs) -> Result<()> {
    ensure_binary_debug()?;
    let result = daemon_cmd("debug_next_frame", serde_json::json!({}))
        .ok_or_else(|| miette!("Failed to advance frame — is the game suspended?"))?;

    match args.format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&result).unwrap());
        }
        OutputFormat::Human => {
            println!("{}", "Advanced one frame".green());
        }
    }
    Ok(())
}

// ── One-shot: time-scale ────────────────────────────────────────────

pub(crate) fn cmd_time_scale(args: &TimeScaleArgs) -> Result<()> {
    ensure_binary_debug()?;
    let result = daemon_cmd("debug_time_scale", serde_json::json!({"scale": args.scale}))
        .ok_or_else(|| {
            miette!("Failed to set time scale — is a game running with the binary debug protocol?")
        })?;

    match args.format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&result).unwrap());
        }
        OutputFormat::Human => {
            println!("{}", format!("Time scale set to {}x", args.scale).green());
        }
    }
    Ok(())
}

// ── One-shot: reload-scripts ────────────────────────────────────────

pub(crate) fn cmd_reload_scripts(args: &ReloadScriptsArgs) -> Result<()> {
    ensure_binary_debug()?;
    let params = if args.paths.is_empty() {
        serde_json::json!({})
    } else {
        serde_json::json!({"paths": args.paths})
    };
    let result = daemon_cmd("debug_reload_scripts", params).ok_or_else(|| {
        miette!("Failed to reload scripts — is a game running with the binary debug protocol?")
    })?;

    match args.format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&result).unwrap());
        }
        OutputFormat::Human => {
            if args.paths.is_empty() {
                println!("{}", "All scripts reloaded".green());
            } else {
                println!("{} {} script(s)", "Reloaded".green(), args.paths.len());
            }
        }
    }
    Ok(())
}

// ── One-shot: reload-all-scripts ─────────────────────────────────────

pub(crate) fn cmd_reload_all_scripts(args: &StepArgs) -> Result<()> {
    ensure_binary_debug()?;
    daemon_cmd("debug_reload_all_scripts", serde_json::json!({}))
        .ok_or_else(|| miette!("Failed — is a game running with the binary debug protocol?"))?;
    match args.format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({"reloaded": true})).unwrap()
            );
        }
        OutputFormat::Human => println!("{}", "All scripts reloaded".green()),
    }
    Ok(())
}

// ── One-shot: skip-breakpoints ──────────────────────────────────────

pub(crate) fn cmd_skip_breakpoints(args: &SkipBreakpointsArgs) -> Result<()> {
    ensure_binary_debug()?;
    let skip = !args.off;
    daemon_cmd(
        "debug_set_skip_breakpoints",
        serde_json::json!({"value": skip}),
    )
    .ok_or_else(|| miette!("Failed — is a game running?"))?;
    match args.format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({"skip": skip})).unwrap()
            );
        }
        OutputFormat::Human => {
            if skip {
                println!("{}", "Breakpoints skipped".green());
            } else {
                println!("{}", "Breakpoints re-enabled".green());
            }
        }
    }
    Ok(())
}

// ── One-shot: ignore-errors ─────────────────────────────────────────

pub(crate) fn cmd_ignore_errors(args: &IgnoreErrorsArgs) -> Result<()> {
    ensure_binary_debug()?;
    let ignore = !args.off;
    daemon_cmd(
        "debug_set_ignore_error_breaks",
        serde_json::json!({"value": ignore}),
    )
    .ok_or_else(|| miette!("Failed — is a game running?"))?;
    match args.format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({"ignore": ignore})).unwrap()
            );
        }
        OutputFormat::Human => {
            if ignore {
                println!("{}", "Error breaks ignored".green());
            } else {
                println!("{}", "Error breaks re-enabled".green());
            }
        }
    }
    Ok(())
}

// ── One-shot: set-prop-field ────────────────────────────────────────

pub(crate) fn cmd_set_prop_field(args: &SetPropFieldArgs) -> Result<()> {
    ensure_binary_debug()?;
    let json_value: serde_json::Value = serde_json::from_str(&args.value)
        .unwrap_or_else(|_| serde_json::Value::String(args.value.clone()));

    daemon_cmd(
        "debug_set_property_field",
        serde_json::json!({
            "object_id": args.id,
            "property": args.property,
            "field": args.field,
            "value": json_value,
        }),
    )
    .ok_or_else(|| {
        miette!(
            "Failed to set {}.{} on object {} — is a game running?",
            args.property,
            args.field,
            args.id
        )
    })?;
    match args.format {
        OutputFormat::Json => {
            let mut out = serde_json::json!({
                "object_id": args.id,
                "property": args.property,
                "field": args.field,
                "value": json_value,
            });
            if args.screenshot {
                let (w, h, path) = take_screenshot(None)?;
                out["screenshot"] = serde_json::json!({
                    "width": w, "height": h, "format": "png", "path": path,
                });
            }
            println!("{}", serde_json::to_string_pretty(&out).unwrap());
        }
        OutputFormat::Human => {
            println!(
                "{} {}.{}.{} = {}",
                "Set".green(),
                format!("[{}]", args.id).dimmed(),
                args.property.cyan(),
                args.field.cyan(),
                args.value.green(),
            );
            if args.screenshot {
                let (_w, _h, path) = take_screenshot(None)?;
                println!("{path}");
            }
        }
    }
    Ok(())
}
