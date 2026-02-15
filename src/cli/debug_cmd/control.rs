use miette::{Result, miette};
use owo_colors::OwoColorize;

use super::args::{BreakpointBinArgs, EvalBinArgs, OutputFormat, StepArgs, VarsArgs};
use super::rewrite::rewrite_eval_expression;
use super::scene::format_variant_display;
use super::{daemon_cmd, debug_break_for_eval, ensure_binary_debug};

// ── Execution control (binary protocol) ─────────────────────────────

pub(crate) fn cmd_exec_continue(args: &StepArgs) -> Result<()> {
    ensure_binary_debug()?;
    // Send debugger continue (resumes from breakpoint)
    daemon_cmd("debug_continue", serde_json::json!({}));
    // Also unsuspend the scene tree and re-enable input (in case the game
    // was paused via suspend rather than a debugger breakpoint)
    daemon_cmd("debug_suspend", serde_json::json!({"suspend": false}));
    daemon_cmd("debug_node_select_set_type", serde_json::json!({"type": 0}));
    match args.format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({"ok": true})).unwrap()
            );
        }
        OutputFormat::Human => println!("{}", "Continued".green()),
    }
    Ok(())
}

pub(crate) fn cmd_exec_pause(args: &StepArgs) -> Result<()> {
    ensure_binary_debug()?;
    // Use scene-level suspend (freezes game loop + disables input)
    // rather than debugger break (which halts script execution)
    daemon_cmd("debug_suspend", serde_json::json!({"suspend": true}))
        .ok_or_else(|| miette!("Failed — is a game running?"))?;
    match args.format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({"ok": true})).unwrap()
            );
        }
        OutputFormat::Human => println!("{}", "Paused".green()),
    }
    Ok(())
}

pub(crate) fn cmd_exec_next(args: &StepArgs) -> Result<()> {
    ensure_binary_debug()?;
    daemon_cmd("debug_next_step", serde_json::json!({}))
        .ok_or_else(|| miette!("Failed — is a game running?"))?;
    match args.format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({"ok": true})).unwrap()
            );
        }
        OutputFormat::Human => println!("{}", "Stepped over".green()),
    }
    Ok(())
}

pub(crate) fn cmd_exec_step_in(args: &StepArgs) -> Result<()> {
    ensure_binary_debug()?;
    daemon_cmd("debug_step_in", serde_json::json!({}))
        .ok_or_else(|| miette!("Failed — is a game running?"))?;
    match args.format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({"ok": true})).unwrap()
            );
        }
        OutputFormat::Human => println!("{}", "Stepped in".green()),
    }
    Ok(())
}

pub(crate) fn cmd_exec_step_out(args: &StepArgs) -> Result<()> {
    ensure_binary_debug()?;
    daemon_cmd("debug_step_out", serde_json::json!({}))
        .ok_or_else(|| miette!("Failed — is a game running?"))?;
    match args.format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({"ok": true})).unwrap()
            );
        }
        OutputFormat::Human => println!("{}", "Stepped out".green()),
    }
    Ok(())
}

// ── Debugging (binary protocol) ─────────────────────────────────────

pub(crate) fn cmd_breakpoint(args: &BreakpointBinArgs) -> Result<()> {
    ensure_binary_debug()?;
    let enabled = !args.off;

    // Resolve --name to path:line if provided
    let (path, line) = if let Some(ref func_name) = args.name {
        let (p, l) = resolve_function_to_location(func_name)?;
        // --path/--line override --name if both given
        let path = args.path.clone().unwrap_or(p);
        let line = args.line.unwrap_or(l);
        (path, line)
    } else {
        let path = args
            .path
            .clone()
            .ok_or_else(|| miette!("--path is required (or use --name to resolve by function)"))?;
        let line = args
            .line
            .ok_or_else(|| miette!("--line is required (or use --name to resolve by function)"))?;
        (path, line)
    };

    let mut bp_params = serde_json::json!({"path": path, "line": line, "enabled": enabled});
    if let Some(ref condition) = args.condition {
        bp_params["condition"] = serde_json::Value::String(condition.clone());
    }
    daemon_cmd("debug_breakpoint", bp_params)
        .ok_or_else(|| miette!("Failed — is a game running?"))?;

    match args.format {
        OutputFormat::Json => {
            let mut out = serde_json::json!({
                "path": path,
                "line": line,
                "enabled": enabled,
            });
            if let Some(ref condition) = args.condition {
                out["condition"] = serde_json::Value::String(condition.clone());
            }
            if let Some(ref name) = args.name {
                out["name"] = serde_json::Value::String(name.clone());
            }
            println!("{}", serde_json::to_string_pretty(&out).unwrap());
        }
        OutputFormat::Human => {
            let cond_info = args
                .condition
                .as_ref()
                .map(|c| format!(" when {c}"))
                .unwrap_or_default();
            if enabled {
                println!(
                    "{} at {}:{}{}",
                    "Breakpoint set".green(),
                    path.cyan(),
                    line,
                    cond_info.dimmed(),
                );
            } else {
                println!(
                    "{} at {}:{}",
                    "Breakpoint cleared".green(),
                    path.cyan(),
                    line,
                );
            }
        }
    }
    Ok(())
}

/// Resolve a function name to a res:// path and line number by searching project GDScript files.
fn resolve_function_to_location(func_name: &str) -> Result<(String, u32)> {
    let cwd = std::env::current_dir().unwrap_or_default();
    let project = crate::core::project::GodotProject::discover(&cwd)?;
    let files = crate::core::fs::collect_gdscript_files(&project.root)?;

    for file in &files {
        let Ok(source) = std::fs::read_to_string(file) else {
            continue;
        };
        // Search for "func <name>" pattern
        for (i, line_text) in source.lines().enumerate() {
            let trimmed = line_text.trim();
            if trimmed.starts_with("func ")
                && trimmed[5..].trim_start().starts_with(func_name)
                && trimmed[5..]
                    .trim_start()
                    .get(func_name.len()..)
                    .is_some_and(|rest| {
                        rest.starts_with('(')
                            || rest.starts_with(':')
                            || rest.starts_with(' ')
                            || rest.is_empty()
                    })
            {
                // Convert to res:// path
                let rel = file
                    .strip_prefix(&project.root)
                    .unwrap_or(file)
                    .to_string_lossy()
                    .replace('\\', "/");
                let res_path = format!("res://{rel}");
                return Ok((res_path, (i + 1) as u32));
            }
        }
    }

    Err(miette!(
        "Function '{}' not found in any .gd file in the project",
        func_name
    ))
}

pub(crate) fn cmd_stack(args: &StepArgs) -> Result<()> {
    ensure_binary_debug()?;
    let result = daemon_cmd("debug_get_stack_dump", serde_json::json!({}))
        .ok_or_else(|| miette!("Failed — is a game running?"))?;
    match args.format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&result).unwrap());
        }
        OutputFormat::Human => {
            if let Some(frames) = result.as_array() {
                if frames.is_empty() {
                    println!("{}", "(no stack frames)".dimmed());
                }
                for (i, f) in frames.iter().enumerate() {
                    let name = f["function"]
                        .as_str()
                        .or_else(|| f["name"].as_str())
                        .unwrap_or("?");
                    let file = f["file"].as_str().unwrap_or("?");
                    let line = f["line"].as_u64().unwrap_or(0);
                    println!(
                        "  {} {} ({}:{})",
                        format!("#{i}").dimmed(),
                        name.green().bold(),
                        file.cyan(),
                        line,
                    );
                }
            } else {
                println!("{}", serde_json::to_string_pretty(&result).unwrap());
            }
        }
    }
    Ok(())
}

pub(crate) fn cmd_vars(args: &VarsArgs) -> Result<()> {
    ensure_binary_debug()?;
    let result = daemon_cmd(
        "debug_get_stack_frame_vars",
        serde_json::json!({"frame": args.frame}),
    )
    .ok_or_else(|| miette!("Failed — is a game running?"))?;
    match args.format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&result).unwrap());
        }
        OutputFormat::Human => {
            if let Some(vars) = result.as_array() {
                if vars.is_empty() {
                    println!("{}", "(no variables)".dimmed());
                }
                for v in vars {
                    let name = v["name"].as_str().unwrap_or("?");
                    let value = format_variant_display(&v["value"]);
                    println!("  {} = {}", name.cyan(), value.green());
                }
            } else if let Some(obj) = result.as_object() {
                // Daemon may return named scope groups
                for (scope_name, scope_vars) in obj {
                    println!("\n{}", format!("{scope_name}:").bold());
                    if let Some(vars) = scope_vars.as_array() {
                        for v in vars {
                            let name = v["name"].as_str().unwrap_or("?");
                            let value = format_variant_display(&v["value"]);
                            println!("  {} = {}", name.cyan(), value.green());
                        }
                    }
                }
            } else {
                println!("{}", serde_json::to_string_pretty(&result).unwrap());
            }
        }
    }
    Ok(())
}

pub(crate) fn cmd_evaluate(args: &EvalBinArgs) -> Result<()> {
    ensure_binary_debug()?;

    let input = args.expr.trim();
    let (expr, was_rewritten) = rewrite_eval_expression(input);
    if was_rewritten && !matches!(args.format, OutputFormat::Json) {
        eprintln!("  {} {}", "Rewritten:".dimmed(), expr.dimmed());
    }

    // Auto-break: set a temporary breakpoint on _process so we get a real
    // GDScript context. The binary protocol's evaluate only works inside
    // Godot's debug() loop with an active script stack frame.
    let break_ctx = debug_break_for_eval();

    let result = daemon_cmd(
        "debug_evaluate",
        serde_json::json!({"expression": expr, "frame": args.frame}),
    );

    break_ctx.cleanup();

    let result = result.ok_or_else(|| miette!("Evaluate failed — is a game running?"))?;

    match args.format {
        OutputFormat::Json => {
            let mut json = result.clone();
            if was_rewritten {
                json["rewritten_expression"] = serde_json::json!(expr);
                json["original_expression"] = serde_json::json!(input);
            }
            println!("{}", serde_json::to_string_pretty(&json).unwrap());
        }
        OutputFormat::Human => {
            let variant = result.get("value").unwrap_or(&result);
            let display = format_variant_display(variant);
            if type_name_from_variant(&result).is_empty() {
                println!("{} = {}", input.cyan(), display.green());
            } else {
                println!(
                    "{} {} = {}",
                    type_name_from_variant(&result).dimmed(),
                    input.cyan(),
                    display.green()
                );
            }
        }
    }
    Ok(())
}

/// Extract the type name from a binary protocol variant result.
fn type_name_from_variant(v: &serde_json::Value) -> &str {
    v.get("type")
        .or_else(|| v.get("value").and_then(|val| val.get("type")))
        .and_then(|t| t.as_str())
        .unwrap_or("")
}
