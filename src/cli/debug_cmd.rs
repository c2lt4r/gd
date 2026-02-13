use clap::{Args, Subcommand};
use miette::{Result, miette};
use owo_colors::OwoColorize;

use crate::debug::{BreakpointResult, Scope, StackFrame, Variable};

#[derive(Args)]
pub struct DebugArgs {
    #[command(subcommand)]
    pub command: DebugCommand,
}

#[derive(Subcommand)]
pub enum DebugCommand {
    /// Attach to Godot and start an interactive debug session
    Attach,
    /// Set a breakpoint, wait for hit, show stack + variables (one-shot)
    #[command(name = "break")]
    Break(BreakArgs),
    /// Show DAP server status and capabilities (one-shot)
    Status(StatusArgs),
    /// Terminate the running game
    Stop,
    /// Continue execution (resume from breakpoint)
    Continue,
    /// Step over (next line)
    Next,
    /// Step into function call
    Step,
    /// Pause execution
    Pause,
    /// Evaluate an expression in the current scope
    Eval(EvalArgs),
    /// Set a variable's value while paused at a breakpoint
    #[command(name = "set-var")]
    SetVar(SetVarArgs),
}

#[derive(Args)]
pub struct BreakArgs {
    /// Script file path (relative to project root, e.g. scripts/kart.gd)
    #[arg(long)]
    pub file: Option<String>,
    /// Line numbers to set breakpoints on
    #[arg(long, num_args = 1..)]
    pub line: Vec<u32>,
    /// Function name to break on (resolves to file:line automatically)
    #[arg(long)]
    pub name: Option<String>,
    /// Condition expression (breakpoint only triggers when true)
    #[arg(long)]
    pub condition: Option<String>,
    /// Timeout in seconds to wait for breakpoint hit (default: 30)
    #[arg(long, default_value = "30")]
    pub timeout: u64,
    /// Output format
    #[arg(long, default_value = "human")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct EvalArgs {
    /// Expression to evaluate (e.g. "self.speed", "position.x")
    #[arg(long)]
    pub expr: String,
    /// Output format
    #[arg(long, default_value = "human")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct SetVarArgs {
    /// Variable name to set
    #[arg(long)]
    pub name: String,
    /// New value (as string, e.g. "3.0", "true", "Vector3(1,2,3)")
    #[arg(long)]
    pub value: String,
    /// Scope to search: locals, members, or globals (default: searches all)
    #[arg(long)]
    pub scope: Option<String>,
}

#[derive(Args)]
pub struct StatusArgs {
    /// Output format
    #[arg(long, default_value = "human")]
    pub format: OutputFormat,
}

#[derive(Clone, Debug)]
pub enum OutputFormat {
    Human,
    Json,
}

impl std::str::FromStr for OutputFormat {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "human" => Ok(Self::Human),
            "json" => Ok(Self::Json),
            other => Err(format!("unknown format: {other}")),
        }
    }
}

impl std::fmt::Display for OutputFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Human => write!(f, "human"),
            Self::Json => write!(f, "json"),
        }
    }
}

pub fn exec(args: DebugArgs) -> Result<()> {
    match args.command {
        DebugCommand::Attach => cmd_attach(),
        DebugCommand::Break(a) => cmd_break(a),
        DebugCommand::Status(a) => cmd_status(a),
        DebugCommand::Stop => cmd_stop(),
        DebugCommand::Continue => cmd_continue(),
        DebugCommand::Next => cmd_next(),
        DebugCommand::Step => cmd_step(),
        DebugCommand::Pause => cmd_pause(),
        DebugCommand::Eval(a) => cmd_eval(a),
        DebugCommand::SetVar(a) => cmd_set_var(a),
    }
}

// ── Daemon helpers ───────────────────────────────────────────────────

/// Send a DAP method through the daemon, returning the result.
fn daemon_dap(method: &str, params: serde_json::Value) -> Option<serde_json::Value> {
    crate::lsp::daemon_client::query_daemon(method, params, None)
}

/// Send a DAP method through the daemon with a custom timeout.
fn daemon_dap_timeout(
    method: &str,
    params: serde_json::Value,
    timeout_secs: u64,
) -> Option<serde_json::Value> {
    crate::lsp::daemon_client::query_daemon(
        method,
        params,
        Some(std::time::Duration::from_secs(timeout_secs + 5)),
    )
}

/// Resolve a relative script path using the daemon's project path.
fn resolve_script_path(relative: &str) -> Option<String> {
    // Verify file exists locally
    let cwd = std::env::current_dir().ok()?;
    let project = crate::core::project::GodotProject::discover(&cwd).ok()?;
    let full = project.root.join(relative);
    if !full.exists() {
        return None;
    }

    let result = daemon_dap("dap_project_path", serde_json::json!({}))?;
    let editor_root = result.get("project_path")?.as_str()?;
    let relative_fwd = relative.replace('\\', "/");
    Some(format!("{editor_root}/{relative_fwd}"))
}

// ── Interactive session ──────────────────────────────────────────────

fn cmd_attach() -> Result<()> {
    // Verify daemon is available
    daemon_dap("dap_status", serde_json::json!({})).ok_or_else(|| {
        miette!("Could not connect to Godot DAP via daemon\n  Is the Godot editor running?")
    })?;

    println!(
        "{} {}",
        "Attached to Godot DAP".green().bold(),
        "(via daemon)".dimmed(),
    );
    println!(
        "Type {} for commands, {} to exit.\n",
        "help".cyan(),
        "quit".cyan()
    );

    let stdin = std::io::stdin();
    let mut line = String::new();

    loop {
        eprint!("{} ", "gd>".green().bold());

        line.clear();
        if stdin.read_line(&mut line).unwrap_or(0) == 0 {
            break; // EOF
        }
        let input = line.trim();
        if input.is_empty() {
            continue;
        }

        let parts: Vec<&str> = input.split_whitespace().collect();
        let cmd = parts[0];
        let args = &parts[1..];

        match cmd {
            "help" | "h" => print_help(),
            "quit" | "q" | "exit" => break,
            "continue" | "c" => {
                if daemon_dap("dap_continue", serde_json::json!({})).is_some() {
                    println!("{}", "Continued".green());
                } else {
                    println!("{}", "Failed to continue".red());
                }
            }
            "pause" | "p" => {
                if daemon_dap("dap_pause", serde_json::json!({})).is_some() {
                    println!("{}", "Paused".green());
                } else {
                    println!("{}", "Failed to pause".red());
                }
            }
            "next" | "n" => {
                if daemon_dap("dap_next", serde_json::json!({})).is_some() {
                    println!("{}", "Stepped over".green());
                } else {
                    println!("{}", "Failed to step over".red());
                }
            }
            "step" | "s" => {
                if daemon_dap("dap_step_in", serde_json::json!({})).is_some() {
                    println!("{}", "Stepped in".green());
                } else {
                    println!("{}", "Failed to step in".red());
                }
            }
            "stack" | "bt" => repl_stack(),
            "vars" => repl_vars(args.first().copied()),
            "expand" => {
                if let Some(ref_str) = args.first() {
                    if let Ok(vref) = ref_str.parse::<i64>() {
                        repl_expand(vref);
                    } else {
                        println!("Usage: expand <ref_id>");
                    }
                } else {
                    println!("Usage: expand <ref_id>");
                }
            }
            "eval" | "e" => {
                if args.is_empty() {
                    println!("Usage: eval <expression>");
                } else {
                    let expr = args.join(" ");
                    repl_eval(&expr);
                }
            }
            "break" | "b" => {
                if args.len() < 2 {
                    println!("Usage: break <file> <line> [line2 ...]");
                } else {
                    repl_break(args[0], &args[1..]);
                }
            }
            "clear" => {
                if args.is_empty() {
                    println!("Usage: clear <file>");
                } else {
                    repl_clear(args[0]);
                }
            }
            "wait" => {
                let timeout = args
                    .first()
                    .and_then(|s| s.parse::<u64>().ok())
                    .unwrap_or(30);
                repl_wait(timeout);
            }
            _ => println!("Unknown command: {}. Type 'help' for commands.", cmd.red()),
        }
    }

    println!("{}", "Disconnected.".dimmed());
    Ok(())
}

fn print_help() {
    println!("{}", "Commands:".bold());
    println!(
        "  {} {}         Set breakpoint(s)",
        "break".cyan(),
        "<file> <line> [line2 ...]".dimmed()
    );
    println!(
        "  {} {}              Clear breakpoints in file",
        "clear".cyan(),
        "<file>".dimmed()
    );
    println!(
        "  {} {}            Wait for breakpoint hit",
        "wait".cyan(),
        "[timeout_secs]".dimmed()
    );
    println!(
        "  {} / {}              Continue execution",
        "continue".cyan(),
        "c".dimmed()
    );
    println!(
        "  {} / {}                 Pause execution",
        "pause".cyan(),
        "p".dimmed()
    );
    println!(
        "  {} / {}                  Step over (next line)",
        "next".cyan(),
        "n".dimmed()
    );
    println!(
        "  {} / {}                  Step into",
        "step".cyan(),
        "s".dimmed()
    );
    println!(
        "  {} / {}              Show call stack",
        "stack".cyan(),
        "bt".dimmed()
    );
    println!(
        "  {} {}          Show variables",
        "vars".cyan(),
        "[locals|members|globals]".dimmed()
    );
    println!(
        "  {} {}            Expand nested variable",
        "expand".cyan(),
        "<ref_id>".dimmed()
    );
    println!(
        "  {} {}              Evaluate expression",
        "eval".cyan(),
        "<expr>".dimmed()
    );
    println!(
        "  {} / {}                  Disconnect and exit",
        "quit".cyan(),
        "q".dimmed()
    );
}

// ── One-shot: break ──────────────────────────────────────────────────

fn cmd_break(args: BreakArgs) -> Result<()> {
    // Resolve --name to file:line if provided
    let (file, lines) = if let Some(ref func_name) = args.name {
        let (resolved_file, resolved_line) = resolve_function_name(func_name)?;
        let lines = if args.line.is_empty() {
            vec![resolved_line]
        } else {
            args.line.clone()
        };
        (resolved_file, lines)
    } else {
        let file = args
            .file
            .as_ref()
            .ok_or_else(|| miette!("--file is required when not using --name"))?
            .clone();
        if args.line.is_empty() {
            return Err(miette!(
                "At least one --line is required when not using --name"
            ));
        }
        (file, args.line.clone())
    };

    // Resolve path using daemon's project path
    let path = resolve_script_path(&file)
        .ok_or_else(|| miette!("Cannot resolve script path — is the daemon connected to Godot?"))?;

    let lines_json: Vec<serde_json::Value> = lines.iter().map(|&l| serde_json::json!(l)).collect();

    // Build breakpoint params (with optional condition)
    let bp_params = if let Some(ref cond) = args.condition {
        serde_json::json!({"path": path, "lines": lines_json, "condition": cond})
    } else {
        serde_json::json!({"path": path, "lines": lines_json})
    };

    // Set breakpoints
    let bp_body = daemon_dap("dap_set_breakpoints", bp_params)
        .ok_or_else(|| miette!("Failed to set breakpoints — is Godot editor running?"))?;

    let results = parse_breakpoint_results(&bp_body);

    for bp in &results {
        let status = if bp.verified {
            "verified".green().to_string()
        } else {
            "unverified".yellow().to_string()
        };
        println!(
            "  {} {}:{} [{}]",
            "Breakpoint".bold(),
            file.cyan(),
            bp.line,
            status,
        );
    }

    // Continue execution
    daemon_dap("dap_continue", serde_json::json!({}));

    println!(
        "\n{} (timeout: {}s)...",
        "Waiting for breakpoint hit".dimmed(),
        args.timeout,
    );

    // Wait for stopped event
    let stopped = daemon_dap_timeout(
        "dap_wait_stopped",
        serde_json::json!({"timeout": args.timeout}),
        args.timeout,
    );

    if stopped.is_none() {
        return Err(miette!(
            "Timeout — breakpoint was not hit within {}s",
            args.timeout
        ));
    }

    println!("{}", "Breakpoint hit!".green().bold());

    // Brief pause to let Godot's debugger fully populate scope data
    std::thread::sleep(std::time::Duration::from_millis(50));

    // Get stack frames
    let frames = get_stack_frames();

    // Get variables
    let mut all_vars: Vec<(String, Vec<Variable>)> = Vec::new();
    if let Some(frame_id) = frames.first().map(|f| f.id)
        && let Some(scopes_body) = daemon_dap(
            "dap_scopes",
            serde_json::json!({"frame_id": frame_id}),
        )
        && let Some(scopes) = scopes_body["scopes"].as_array()
    {
        for scope in scopes {
            let name = scope["name"].as_str().unwrap_or("?").to_string();
            let vref = scope["variablesReference"].as_i64().unwrap_or(0);
            if vref > 0
                && let Some(vbody) = daemon_dap(
                    "dap_variables",
                    serde_json::json!({"variables_reference": vref}),
                )
            {
                all_vars.push((name, parse_variables(&vbody)));
            }
        }
    }

    match args.format {
        OutputFormat::Json => {
            let output = serde_json::json!({
                "breakpoints": results,
                "stackFrames": frames,
                "variables": all_vars.iter().map(|(name, vars)| {
                    serde_json::json!({"scope": name, "variables": vars})
                }).collect::<Vec<_>>(),
            });
            println!("{}", serde_json::to_string_pretty(&output).unwrap());
        }
        OutputFormat::Human => {
            if !frames.is_empty() {
                println!("\n{}", "Call stack:".bold());
                for (i, f) in frames.iter().enumerate() {
                    println!(
                        "  {} {} ({}:{})",
                        format!("#{i}").dimmed(),
                        f.name.green().bold(),
                        f.file.cyan(),
                        f.line,
                    );
                }
            }
            for (scope_name, vars) in &all_vars {
                let _ = print_variables(vars, &OutputFormat::Human, Some(scope_name));
            }
        }
    }

    // Resume execution after inspecting the breakpoint
    daemon_dap("dap_continue", serde_json::json!({}));

    Ok(())
}

// ── One-shot: status ────────────────────────────────────────────────

fn cmd_status(args: StatusArgs) -> Result<()> {
    let result = daemon_dap("dap_status", serde_json::json!({})).ok_or_else(|| {
        miette!("Could not connect to Godot DAP via daemon\n  Is the Godot editor running?")
    })?;

    match args.format {
        OutputFormat::Json => {
            let status = serde_json::json!({
                "connected": true,
                "capabilities": result.get("capabilities"),
                "threads": result.get("threads"),
            });
            println!("{}", serde_json::to_string_pretty(&status).unwrap());
        }
        OutputFormat::Human => {
            println!(
                "{} {}",
                "Connected to Godot DAP".green().bold(),
                "(via daemon)".dimmed(),
            );
            println!();
            if let Some(caps) = result.get("capabilities").and_then(|c| c.as_object()) {
                println!("{}", "Capabilities:".bold());
                for (k, v) in caps {
                    if v.as_bool() == Some(true) {
                        println!("  {} {}", "+".green(), k);
                    }
                }
            }
            if let Some(threads) = result.get("threads").and_then(|t| t.as_array()) {
                println!();
                println!("{}", "Threads:".bold());
                for t in threads {
                    println!(
                        "  {} {} (id: {})",
                        "*".cyan(),
                        t["name"].as_str().unwrap_or("?"),
                        t["id"].as_i64().unwrap_or(0)
                    );
                }
            }
        }
    }

    Ok(())
}

// ── One-shot: stop ──────────────────────────────────────────────────

fn cmd_stop() -> Result<()> {
    // Continue execution first in case paused at a breakpoint
    daemon_dap("dap_continue", serde_json::json!({}));
    daemon_dap("dap_terminate", serde_json::json!({})).ok_or_else(|| {
        miette!("Could not terminate game — is a game running?")
    })?;
    println!("{} Game terminated", "■".red());
    Ok(())
}

// ── One-shot: continue/next/step/pause/eval ─────────────────────────

fn cmd_continue() -> Result<()> {
    daemon_dap("dap_continue", serde_json::json!({}))
        .ok_or_else(|| miette!("Failed to continue — is a game running and paused?"))?;
    println!("{}", "Continued".green());
    Ok(())
}

fn cmd_next() -> Result<()> {
    daemon_dap("dap_next", serde_json::json!({}))
        .ok_or_else(|| miette!("Failed to step — is a game running and paused?"))?;
    println!("{}", "Stepped over".green());
    Ok(())
}

fn cmd_step() -> Result<()> {
    daemon_dap("dap_step_in", serde_json::json!({}))
        .ok_or_else(|| miette!("Failed to step — is a game running and paused?"))?;
    println!("{}", "Stepped in".green());
    Ok(())
}

fn cmd_pause() -> Result<()> {
    daemon_dap("dap_pause", serde_json::json!({}))
        .ok_or_else(|| miette!("Failed to pause — is a game running?"))?;
    println!("{}", "Paused".green());
    Ok(())
}

fn cmd_eval(args: EvalArgs) -> Result<()> {
    let frame_id = get_stack_frames().first().map(|f| f.id).unwrap_or(0);
    let result = daemon_dap(
        "dap_evaluate",
        serde_json::json!({"expression": args.expr, "context": "repl", "frame_id": frame_id}),
    )
    .ok_or_else(|| {
        miette!(
            "Evaluate failed — game must be paused at a breakpoint.\n  Godot only supports member-access expressions (e.g. self.speed)."
        )
    })?;

    let value = result["result"].as_str().unwrap_or("?");
    let type_name = result["type"].as_str().unwrap_or("");

    match args.format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&result).unwrap());
        }
        OutputFormat::Human => {
            if type_name.is_empty() {
                println!("{} = {}", args.expr.cyan(), value.green());
            } else {
                println!(
                    "{} {} = {}",
                    type_name.dimmed(),
                    args.expr.cyan(),
                    value.green()
                );
            }
        }
    }
    Ok(())
}

fn cmd_set_var(args: SetVarArgs) -> Result<()> {
    let frames = get_stack_frames();
    let frame = frames
        .first()
        .ok_or_else(|| miette!("No stack frames — game must be paused at a breakpoint"))?;

    let scopes_body = daemon_dap(
        "dap_scopes",
        serde_json::json!({"frame_id": frame.id}),
    )
    .ok_or_else(|| miette!("Failed to get scopes"))?;

    let scopes: Vec<Scope> = scopes_body["scopes"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .map(|s| Scope {
                    name: s["name"].as_str().unwrap_or("?").to_string(),
                    variables_reference: s["variablesReference"].as_i64().unwrap_or(0),
                })
                .collect()
        })
        .unwrap_or_default();

    let scope_filter = args.scope.as_deref().map(|s| s.to_lowercase());

    // Search scopes for the variable
    for scope in &scopes {
        if let Some(ref f) = scope_filter
            && !scope.name.to_lowercase().contains(f)
        {
            continue;
        }
        if scope.variables_reference <= 0 {
            continue;
        }

        let Some(vbody) = daemon_dap(
            "dap_variables",
            serde_json::json!({"variables_reference": scope.variables_reference}),
        ) else {
            continue;
        };

        let vars = parse_variables(&vbody);
        if vars.iter().any(|v| v.name == args.name) {
            let result = daemon_dap(
                "dap_set_variable",
                serde_json::json!({
                    "variables_reference": scope.variables_reference,
                    "name": args.name,
                    "value": args.value,
                }),
            )
            .ok_or_else(|| miette!("setVariable failed — Godot may not support setting this variable type"))?;

            let new_value = result["value"].as_str().unwrap_or(&args.value);
            let type_name = result["type"].as_str().unwrap_or("");
            if type_name.is_empty() {
                println!(
                    "{} {} = {}",
                    "Set".green(),
                    args.name.cyan(),
                    new_value.green()
                );
            } else {
                println!(
                    "{} {} {} = {}",
                    "Set".green(),
                    type_name.dimmed(),
                    args.name.cyan(),
                    new_value.green()
                );
            }
            return Ok(());
        }
    }

    Err(miette!(
        "Variable '{}' not found in current scope{}",
        args.name,
        if scope_filter.is_some() {
            " (try without --scope)"
        } else {
            ""
        }
    ))
}

// ── Helper: resolve function name to file:line ──────────────────────

/// Resolve a function name to (file, line) by searching project symbols.
fn resolve_function_name(name: &str) -> Result<(String, u32)> {
    let cwd =
        std::env::current_dir().map_err(|e| miette!("cannot get current directory: {e}"))?;
    let project_root = crate::core::config::find_project_root(&cwd)
        .ok_or_else(|| miette!("no project.godot found"))?;

    let files = crate::core::fs::collect_gdscript_files(&project_root)
        .map_err(|e| miette!("failed to collect GDScript files: {e}"))?;
    for file_path in &files {
        let rel = crate::core::fs::relative_slash(file_path, &project_root);
        if let Ok(symbols) = crate::lsp::query::query_symbols(&rel) {
            for sym in &symbols {
                if sym.name == name && sym.kind == "function" {
                    return Ok((rel, sym.line));
                }
            }
        }
    }
    Err(miette!("function '{}' not found in project", name))
}

// ── Shared helpers ──────────────────────────────────────────────────

fn get_stack_frames() -> Vec<StackFrame> {
    let thread_id = daemon_dap("dap_threads", serde_json::json!({}))
        .and_then(|b| b["threads"].as_array()?.first()?.get("id")?.as_i64())
        .unwrap_or(1);

    daemon_dap(
        "dap_stack_trace",
        serde_json::json!({"thread_id": thread_id}),
    )
    .and_then(|b| {
        Some(
            b["stackFrames"]
                .as_array()?
                .iter()
                .map(|f| StackFrame {
                    id: f["id"].as_i64().unwrap_or(0),
                    name: f["name"].as_str().unwrap_or("?").to_string(),
                    file: f["source"]["name"].as_str().unwrap_or("?").to_string(),
                    line: f["line"].as_u64().unwrap_or(0) as u32,
                })
                .collect(),
        )
    })
    .unwrap_or_default()
}

fn parse_breakpoint_results(body: &serde_json::Value) -> Vec<BreakpointResult> {
    body["breakpoints"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .map(|bp| BreakpointResult {
                    verified: bp["verified"].as_bool().unwrap_or(false),
                    line: bp["line"].as_u64().unwrap_or(0) as u32,
                    id: bp["id"].as_i64(),
                })
                .collect()
        })
        .unwrap_or_default()
}

fn parse_variables(body: &serde_json::Value) -> Vec<Variable> {
    body["variables"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .map(|v| Variable {
                    name: v["name"].as_str().unwrap_or("?").to_string(),
                    value: v["value"].as_str().unwrap_or("").to_string(),
                    type_name: v["type"].as_str().unwrap_or("").to_string(),
                    variables_reference: v["variablesReference"].as_i64().unwrap_or(0),
                })
                .collect()
        })
        .unwrap_or_default()
}

// ── REPL helpers (all daemon-backed) ─────────────────────────────────

fn repl_stack() {
    let frames = get_stack_frames();
    if frames.is_empty() {
        println!(
            "{}",
            "No stack frames — game may not be paused at a breakpoint.".yellow()
        );
    } else {
        println!("{}", "Call stack:".bold());
        for (i, f) in frames.iter().enumerate() {
            println!(
                "  {} {} ({}:{})",
                format!("#{i}").dimmed(),
                f.name.green().bold(),
                f.file.cyan(),
                f.line
            );
        }
    }
}

fn repl_vars(scope_filter: Option<&str>) {
    let frames = get_stack_frames();
    let Some(frame) = frames.first() else {
        println!(
            "{}",
            "No stack frames — game may not be paused at a breakpoint.".yellow()
        );
        return;
    };

    let Some(scopes_body) = daemon_dap(
        "dap_scopes",
        serde_json::json!({"frame_id": frame.id}),
    ) else {
        println!("{}", "Failed to get scopes.".red());
        return;
    };

    let scopes: Vec<Scope> = scopes_body["scopes"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .map(|s| Scope {
                    name: s["name"].as_str().unwrap_or("?").to_string(),
                    variables_reference: s["variablesReference"].as_i64().unwrap_or(0),
                })
                .collect()
        })
        .unwrap_or_default();

    let filter = scope_filter.map(|s| s.to_lowercase());
    for scope in &scopes {
        if let Some(ref f) = filter
            && !scope.name.to_lowercase().contains(f)
        {
            continue;
        }
        if scope.variables_reference > 0
            && let Some(body) = daemon_dap(
                "dap_variables",
                serde_json::json!({"variables_reference": scope.variables_reference}),
            )
        {
            let vars = parse_variables(&body);
            let _ = print_variables(&vars, &OutputFormat::Human, Some(&scope.name));
        }
    }
}

fn repl_expand(vref: i64) {
    if let Some(body) = daemon_dap(
        "dap_variables",
        serde_json::json!({"variables_reference": vref}),
    ) {
        let vars = parse_variables(&body);
        let _ = print_variables(&vars, &OutputFormat::Human, None);
    } else {
        println!("{}", "Failed to expand variable.".red());
    }
}

fn repl_eval(expr: &str) {
    // Get the top frame ID for evaluation context
    let frame_id = get_stack_frames().first().map(|f| f.id).unwrap_or(0);
    if let Some(body) = daemon_dap(
        "dap_evaluate",
        serde_json::json!({"expression": expr, "context": "repl", "frame_id": frame_id}),
    ) {
        let result = body["result"].as_str().unwrap_or("?");
        let type_name = body["type"].as_str().unwrap_or("");
        if type_name.is_empty() {
            println!("{} = {}", expr.cyan(), result.green());
        } else {
            println!(
                "{} {} = {}",
                type_name.dimmed(),
                expr.cyan(),
                result.green()
            );
        }
    } else {
        println!(
            "{}",
            "Evaluate failed or timed out. Godot only supports member-access expressions (e.g. self.speed) while paused at a breakpoint."
                .yellow()
        );
    }
}

fn repl_break(file: &str, line_strs: &[&str]) {
    let lines: Vec<u32> = line_strs
        .iter()
        .filter_map(|s| s.parse::<u32>().ok())
        .collect();
    if lines.is_empty() {
        println!("No valid line numbers provided.");
        return;
    }

    let Some(path) = resolve_script_path(file) else {
        println!("{}", "Failed to resolve script path via daemon.".red());
        return;
    };

    let lines_json: Vec<serde_json::Value> = lines.iter().map(|&l| serde_json::json!(l)).collect();
    if let Some(body) = daemon_dap(
        "dap_set_breakpoints",
        serde_json::json!({"path": path, "lines": lines_json}),
    ) {
        let results = parse_breakpoint_results(&body);
        for bp in &results {
            let status = if bp.verified {
                "verified".green().to_string()
            } else {
                "unverified".yellow().to_string()
            };
            println!(
                "  {} {}:{} [{}]",
                "Breakpoint".bold(),
                file.cyan(),
                bp.line,
                status
            );
        }
    } else {
        println!("{}", "Failed to set breakpoints.".red());
    }
}

fn repl_clear(file: &str) {
    let Some(path) = resolve_script_path(file) else {
        println!("{}", "Failed to resolve script path via daemon.".red());
        return;
    };

    let empty: Vec<serde_json::Value> = vec![];
    if daemon_dap(
        "dap_set_breakpoints",
        serde_json::json!({"path": path, "lines": empty}),
    )
    .is_some()
    {
        println!("{} {}", "Cleared breakpoints in".green(), file.cyan());
    } else {
        println!("{}", "Failed to clear breakpoints.".red());
    }
}

fn repl_wait(timeout: u64) {
    println!(
        "{} (timeout: {}s)...",
        "Waiting for breakpoint hit".dimmed(),
        timeout
    );

    if daemon_dap_timeout(
        "dap_wait_stopped",
        serde_json::json!({"timeout": timeout}),
        timeout,
    )
    .is_some()
    {
        println!("{}", "Breakpoint hit!".green().bold());
        repl_stack();
        repl_vars(None);
    } else {
        println!(
            "{}",
            format!("Timeout — no breakpoint hit within {timeout}s.").yellow()
        );
    }
}

fn print_variables(
    vars: &[Variable],
    format: &OutputFormat,
    scope_name: Option<&str>,
) -> Result<()> {
    match format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(vars).unwrap());
        }
        OutputFormat::Human => {
            if let Some(name) = scope_name {
                println!("\n{}", format!("{name}:").bold());
            }
            if vars.is_empty() {
                println!("  {}", "(empty)".dimmed());
            }
            for v in vars {
                let expand_hint = if v.variables_reference > 0 {
                    format!(" {}", format!("[ref={}]", v.variables_reference).dimmed())
                } else {
                    String::new()
                };
                if v.type_name.is_empty() {
                    println!("  {} = {}{}", v.name.cyan(), v.value.green(), expand_hint);
                } else {
                    println!(
                        "  {} {} = {}{}",
                        v.type_name.dimmed(),
                        v.name.cyan(),
                        v.value.green(),
                        expand_hint,
                    );
                }
            }
        }
    }
    Ok(())
}
