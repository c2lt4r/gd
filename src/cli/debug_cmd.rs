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
    Continue(StepArgs),
    /// Step over (next line)
    Next(StepArgs),
    /// Step into function call
    Step(StepArgs),
    /// Step out of current function
    #[command(name = "step-out")]
    StepOut(StepArgs),
    /// Pause execution
    Pause(StepArgs),
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
    /// Variable name to set (member variable, e.g. "speed", "max_health")
    #[arg(long)]
    pub name: String,
    /// New value (as string, e.g. "3.0", "true", "Vector3(1,2,3)")
    #[arg(long)]
    pub value: String,
    /// Output format
    #[arg(long, default_value = "human")]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct StepArgs {
    /// Output format
    #[arg(long, default_value = "human")]
    pub format: OutputFormat,
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
        DebugCommand::Continue(a) => cmd_continue(a),
        DebugCommand::Next(a) => cmd_next(a),
        DebugCommand::Step(a) => cmd_step(a),
        DebugCommand::StepOut(a) => cmd_step_out(a),
        DebugCommand::Pause(a) => cmd_pause(a),
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
            "out" | "o" => repl_step_out(),
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
        "  {} / {}                   Step out of function",
        "out".cyan(),
        "o".dimmed()
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
        let (resolved_file, resolved_line) =
            resolve_function_name(func_name, args.file.as_deref())?;
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

    // Set breakpoints (don't send condition to Godot — it ignores it)
    let bp_body = daemon_dap(
        "dap_set_breakpoints",
        serde_json::json!({"path": path, "lines": lines_json}),
    )
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
    if let Some(ref cond) = args.condition {
        println!(
            "  {} {}",
            "Condition:".dimmed(),
            cond.cyan(),
        );
    }

    // Continue execution
    daemon_dap("dap_continue", serde_json::json!({}));

    println!(
        "\n{} (timeout: {}s)...",
        "Waiting for breakpoint hit".dimmed(),
        args.timeout,
    );

    // Wait for stopped event — with client-side condition evaluation
    let deadline = std::time::Instant::now()
        + std::time::Duration::from_secs(args.timeout);

    loop {
        let remaining = deadline
            .saturating_duration_since(std::time::Instant::now())
            .as_secs()
            .max(1);

        let stopped = daemon_dap_timeout(
            "dap_wait_stopped",
            serde_json::json!({"timeout": remaining}),
            remaining,
        );

        if stopped.is_none() {
            return Err(miette!(
                "Timeout — breakpoint was not hit within {}s",
                args.timeout
            ));
        }

        // Client-side condition check
        if let Some(ref cond) = args.condition {
            // Brief pause for scope data
            std::thread::sleep(std::time::Duration::from_millis(200));

            let frame_id = get_stack_frames().first().map(|f| f.id).unwrap_or(0);
            let eval_result = daemon_dap(
                "dap_evaluate",
                serde_json::json!({
                    "expression": cond,
                    "context": "repl",
                    "frame_id": frame_id,
                }),
            );

            let is_falsy = eval_result
                .as_ref()
                .and_then(|v| v["result"].as_str())
                .is_none_or(|r| {
                    matches!(
                        r,
                        "false" | "False" | "0" | "0.0" | "" | "null"
                            | "Null" | "<null>"
                    )
                });

            if is_falsy {
                // Condition not met — resume and wait again
                daemon_dap("dap_continue", serde_json::json!({}));
                if std::time::Instant::now() >= deadline {
                    return Err(miette!(
                        "Timeout — breakpoint hit but condition `{}` was never true within {}s",
                        cond,
                        args.timeout,
                    ));
                }
                continue;
            }
        }

        break; // Breakpoint hit (and condition met if any)
    }

    println!("{}", "Breakpoint hit!".green().bold());

    // Wait for Godot's debugger to populate scope data (too fast → scope_list errors)
    std::thread::sleep(std::time::Duration::from_millis(500));

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

fn cmd_continue(args: StepArgs) -> Result<()> {
    daemon_dap("dap_continue", serde_json::json!({}))
        .ok_or_else(|| miette!("Failed to continue — is a game running and paused?"))?;
    match args.format {
        OutputFormat::Human => println!("{}", "Continued".green()),
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({"action": "continue"})).unwrap()
            );
        }
    }
    Ok(())
}

fn cmd_next(args: StepArgs) -> Result<()> {
    daemon_dap("dap_next", serde_json::json!({}))
        .ok_or_else(|| miette!("Failed to step — is a game running and paused?"))?;
    match args.format {
        OutputFormat::Human => println!("{}", "Stepped over".green()),
        OutputFormat::Json => print_step_json("next"),
    }
    Ok(())
}

fn cmd_step(args: StepArgs) -> Result<()> {
    daemon_dap("dap_step_in", serde_json::json!({}))
        .ok_or_else(|| miette!("Failed to step — is a game running and paused?"))?;
    match args.format {
        OutputFormat::Human => println!("{}", "Stepped in".green()),
        OutputFormat::Json => print_step_json("step"),
    }
    Ok(())
}

fn cmd_step_out(args: StepArgs) -> Result<()> {
    // Synthetic step-out: repeat `next` until stack depth decreases.
    // Godot's DAP doesn't support stepOut natively (the VS Code plugin
    // uses the same approach via the binary debug protocol).
    let initial_depth = get_stack_frames().len();
    if initial_depth <= 1 {
        return Err(miette!(
            "Cannot step out — already at the top-level frame."
        ));
    }

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);

    loop {
        daemon_dap("dap_next", serde_json::json!({}))
            .ok_or_else(|| miette!("Failed to step — is a game running and paused?"))?;

        // Wait for Godot to stop after the step
        let stopped = daemon_dap_timeout(
            "dap_wait_stopped",
            serde_json::json!({"timeout": 5}),
            5,
        );
        if stopped.is_none() {
            return Err(miette!("Step-out timed out waiting for execution to stop."));
        }

        std::thread::sleep(std::time::Duration::from_millis(50));

        let new_depth = get_stack_frames().len();
        if new_depth < initial_depth {
            break; // Successfully stepped out
        }

        if std::time::Instant::now() >= deadline {
            return Err(miette!(
                "Step-out timed out after 15s — function may have a long-running loop.\n  \
                 Use `gd debug continue` to resume, or set a breakpoint in the caller instead."
            ));
        }
    }

    match args.format {
        OutputFormat::Human => println!("{}", "Stepped out".green()),
        OutputFormat::Json => print_step_json("step-out"),
    }
    Ok(())
}

/// Wait for stopped event after a step, print JSON with stack frames + variables.
fn print_step_json(action: &str) {
    let stopped = daemon_dap_timeout("dap_wait_stopped", serde_json::json!({"timeout": 3}), 3);
    if stopped.is_some() {
        // Brief pause for Godot to populate scope data
        std::thread::sleep(std::time::Duration::from_millis(100));
        let frames = get_stack_frames();
        let vars = collect_frame_variables(frames.first().map(|f| f.id));
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "action": action, "stopped": true, "stackFrames": frames, "variables": vars,
            }))
            .unwrap()
        );
    } else {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "action": action, "stopped": false,
            }))
            .unwrap()
        );
    }
}

/// Collect variables from all scopes for a given frame.
fn collect_frame_variables(frame_id: Option<i64>) -> Vec<serde_json::Value> {
    let Some(fid) = frame_id else {
        return vec![];
    };
    let Some(scopes_body) = daemon_dap(
        "dap_scopes",
        serde_json::json!({"frame_id": fid}),
    ) else {
        return vec![];
    };
    let Some(scopes) = scopes_body["scopes"].as_array() else {
        return vec![];
    };
    let mut result = Vec::new();
    for scope in scopes {
        let name = scope["name"].as_str().unwrap_or("?");
        let vref = scope["variablesReference"].as_i64().unwrap_or(0);
        if vref > 0
            && let Some(vbody) = daemon_dap(
                "dap_variables",
                serde_json::json!({"variables_reference": vref}),
            )
        {
            result.push(serde_json::json!({
                "scope": name,
                "variables": parse_variables(&vbody),
            }));
        }
    }
    result
}

fn cmd_pause(args: StepArgs) -> Result<()> {
    daemon_dap("dap_pause", serde_json::json!({}))
        .ok_or_else(|| miette!("Failed to pause — is a game running?"))?;
    match args.format {
        OutputFormat::Human => println!("{}", "Paused".green()),
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({"action": "pause"})).unwrap()
            );
        }
    }
    Ok(())
}

fn cmd_eval(args: EvalArgs) -> Result<()> {
    let expr = args.expr.trim();
    if expr.is_empty() {
        return Err(eval_error(&args, "--expr cannot be empty"));
    }

    // Warn on assignment syntax — Godot's eval doesn't persist direct assignments
    if is_likely_assignment(expr) {
        if matches!(args.format, OutputFormat::Json) {
            // In JSON mode, include warning in the output later
        } else {
            eprintln!(
                "{} Direct assignment via eval may return <null> and not persist.",
                "Warning:".yellow().bold(),
            );
            if let Some(lhs) = extract_assignment_lhs(expr) {
                eprintln!(
                    "  Use: gd debug eval --expr \"self.set('{}', ...)\"",
                    lhs
                );
            }
        }
    }

    let frame_id = get_stack_frames().first().map(|f| f.id).unwrap_or(0);
    let result = daemon_dap(
        "dap_evaluate",
        serde_json::json!({"expression": expr, "context": "repl", "frame_id": frame_id}),
    )
    .ok_or_else(|| {
        eval_error(
            &args,
            "Evaluate failed — game must be paused at a breakpoint.\n  Use `gd debug break` to pause (not `gd debug pause`, which lacks stack frame context).",
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
                println!("{} = {}", expr.cyan(), value.green());
            } else {
                println!(
                    "{} {} = {}",
                    type_name.dimmed(),
                    expr.cyan(),
                    value.green()
                );
            }
            // Hint on <null> results — but not for method calls (void return is expected)
            if (value == "<null>" || value == "Null") && !expr.contains('(') {
                eprintln!(
                    "  {}",
                    "Hint: <null> may indicate an undefined variable or unsupported expression"
                        .dimmed()
                );
            }
        }
    }
    Ok(())
}

/// Detect direct assignment syntax (= but not ==, !=, <=, >=, :=, +=, etc.)
fn is_likely_assignment(expr: &str) -> bool {
    // Skip set() calls — those are intentional
    if expr.contains(".set(") || expr.contains(".set_") {
        return false;
    }
    let bytes = expr.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if b != b'=' {
            continue;
        }
        let prev = if i > 0 { bytes[i - 1] } else { 0 };
        let next = bytes.get(i + 1).copied().unwrap_or(0);
        // Skip ==
        if next == b'=' {
            continue;
        }
        // Skip !=, <=, >=, :=, +=, -=, *=, /=, == (second =)
        if matches!(
            prev,
            b'!' | b'<' | b'>' | b':' | b'+' | b'-' | b'*' | b'/' | b'='
        ) {
            continue;
        }
        return true;
    }
    false
}

/// Extract the left-hand side of an assignment (e.g. "self.speed" from "self.speed = 5")
fn extract_assignment_lhs(expr: &str) -> Option<&str> {
    let eq_pos = expr.find('=')?;
    let lhs = expr[..eq_pos].trim();
    let prop = lhs.strip_prefix("self.").unwrap_or(lhs);
    if prop.is_empty() {
        return None;
    }
    Some(prop)
}

fn cmd_set_var(args: SetVarArgs) -> Result<()> {
    let frames = get_stack_frames();
    let frame = frames
        .first()
        .ok_or_else(|| set_var_error(&args, "No stack frames — game must be paused at a breakpoint.\n  Use `gd debug break` to pause at a breakpoint first."))?;

    // Use eval with self.set() — fast path (Godot's DAP setVariable is broken)
    let val_literal = gdscript_value_literal(&args.value);
    let set_expr = format!("self.set(\"{}\", {val_literal})", args.name);
    daemon_dap(
        "dap_evaluate",
        serde_json::json!({"expression": set_expr, "context": "repl", "frame_id": frame.id}),
    )
    .ok_or_else(|| set_var_error(&args, &format!("Failed to set '{}' — game must be paused at a breakpoint.", args.name)))?;

    // Verify by reading back
    let verify_expr = format!("self.{}", args.name);
    let verify_result = daemon_dap(
        "dap_evaluate",
        serde_json::json!({"expression": verify_expr, "context": "repl", "frame_id": frame.id}),
    );
    let new_val = verify_result
        .as_ref()
        .and_then(|v| v["result"].as_str())
        .unwrap_or("<null>");

    // If verification shows <null>, the property might not exist or it's a local
    if new_val == "<null>" || new_val == "Null" {
        // Check if it's a local variable (more specific error)
        if is_local_variable(frame.id, &args.name) {
            return Err(set_var_error(
                &args,
                &format!(
                    "Cannot modify local variable '{}' — Godot's DAP does not support setting locals.\n  \
                     Only member variables can be modified via `set-var` or `eval --expr \"self.set('name', value)\"`.",
                    args.name,
                ),
            ));
        }
        return Err(set_var_error(
            &args,
            &format!(
                "Failed to set '{}' — variable not found as a member property on self.\n  \
                 Only member variables (declared with `var` at class level) can be set.",
                args.name,
            ),
        ));
    }

    // Get type from verify result; fall back to inferring from value.
    // If we auto-quoted the input, we know it's a String (Godot's eval returns
    // string values without quotes, so infer_gdscript_type can't detect them).
    let was_auto_quoted = val_literal != args.value && val_literal.starts_with('"');
    let type_name = verify_result
        .as_ref()
        .and_then(|v| v["type"].as_str())
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| {
            if was_auto_quoted {
                "String"
            } else {
                infer_gdscript_type(new_val)
            }
        });

    match args.format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "name": args.name,
                    "value": new_val,
                    "type": type_name,
                    "input": args.value,
                }))
                .unwrap()
            );
        }
        OutputFormat::Human => {
            print_set_result(type_name, &args.name, new_val);
        }
    }
    Ok(())
}

/// Build a set-var error that outputs JSON when --format json is active.
fn set_var_error(args: &SetVarArgs, message: &str) -> miette::Report {
    if matches!(args.format, OutputFormat::Json) {
        // Print JSON error and exit with non-zero (miette will set exit code)
        eprintln!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "error": message,
                "name": args.name,
                "input": args.value,
            }))
            .unwrap()
        );
    }
    miette!("{}", message)
}

/// Infer a GDScript type name from a value string.
fn infer_gdscript_type(value: &str) -> &str {
    if value == "true" || value == "false" || value == "True" || value == "False" {
        return "bool";
    }
    if value.parse::<i64>().is_ok() {
        return "int";
    }
    if value.parse::<f64>().is_ok() {
        return "float";
    }
    if (value.starts_with('"') && value.ends_with('"'))
        || (value.starts_with('\'') && value.ends_with('\''))
    {
        return "String";
    }
    // Constructor types: Vector2(...), Color(...), etc.
    if let Some(paren) = value.find('(') {
        return &value[..paren];
    }
    ""
}

/// Build an eval error that outputs JSON when --format json is active.
fn eval_error(args: &EvalArgs, message: &str) -> miette::Report {
    if matches!(args.format, OutputFormat::Json) {
        eprintln!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "error": message,
                "expression": args.expr,
            }))
            .unwrap()
        );
    }
    miette!("{}", message)
}

/// Check if a variable name exists in the Locals scope.
fn is_local_variable(frame_id: i64, name: &str) -> bool {
    let Some(scopes_body) = daemon_dap(
        "dap_scopes",
        serde_json::json!({"frame_id": frame_id}),
    ) else {
        return false;
    };
    let Some(scopes) = scopes_body["scopes"].as_array() else {
        return false;
    };
    for scope in scopes {
        let scope_name = scope["name"].as_str().unwrap_or("");
        if !scope_name.to_lowercase().contains("local") {
            continue;
        }
        let vref = scope["variablesReference"].as_i64().unwrap_or(0);
        if vref <= 0 {
            continue;
        }
        if let Some(vbody) = daemon_dap(
            "dap_variables",
            serde_json::json!({"variables_reference": vref}),
        ) {
            return vbody["variables"]
                .as_array()
                .is_some_and(|vars| vars.iter().any(|v| v["name"].as_str() == Some(name)));
        }
    }
    false
}

/// Convert a CLI value string to a GDScript literal expression.
/// Bare words like `bike` become `"bike"` (quoted strings).
/// Numbers, bools, constructors, and already-quoted strings pass through.
fn gdscript_value_literal(value: &str) -> String {
    // Already quoted
    if (value.starts_with('"') && value.ends_with('"'))
        || (value.starts_with('\'') && value.ends_with('\''))
    {
        return value.to_string();
    }
    // Number (int or float, including negatives)
    if value.parse::<f64>().is_ok() {
        return value.to_string();
    }
    // Boolean, null
    if matches!(value, "true" | "false" | "null") {
        return value.to_string();
    }
    // Constructor or expression: Vector3(1,2,3), Color.RED, Array(), etc.
    if value.contains('(') || value.contains('.') {
        return value.to_string();
    }
    // Bare word — treat as string literal
    format!("\"{value}\"")
}

fn print_set_result(type_name: &str, name: &str, value: &str) {
    if type_name.is_empty() {
        println!(
            "{} {} = {}",
            "Set".green(),
            name.cyan(),
            value.green()
        );
    } else {
        println!(
            "{} {} {} = {}",
            "Set".green(),
            type_name.dimmed(),
            name.cyan(),
            value.green()
        );
    }
}

// ── Helper: resolve function name to file:line ──────────────────────

/// Resolve a function name to (file, first_statement_line) by searching project symbols.
///
/// If `file_filter` is provided, only search that file. Otherwise search all
/// project files and error with a candidate list when the name is ambiguous.
/// Returns the first executable statement line inside the function body
/// (not the `func` declaration line, which Godot won't break on).
fn resolve_function_name(name: &str, file_filter: Option<&str>) -> Result<(String, u32)> {
    let cwd =
        std::env::current_dir().map_err(|e| miette!("cannot get current directory: {e}"))?;
    let project_root = crate::core::config::find_project_root(&cwd)
        .ok_or_else(|| miette!("no project.godot found"))?;

    let files = crate::core::fs::collect_gdscript_files(&project_root)
        .map_err(|e| miette!("failed to collect GDScript files: {e}"))?;

    let mut candidates: Vec<(String, u32)> = Vec::new();

    for file_path in &files {
        let rel = crate::core::fs::relative_slash(file_path, &project_root);
        if let Some(filter) = file_filter
            && rel != filter
        {
            continue;
        }
        if let Ok(symbols) = crate::lsp::query::query_symbols(&rel) {
            for sym in &symbols {
                if sym.name == name && sym.kind == "function" {
                    // Find the first statement line inside the function body
                    let body_line = find_first_body_line(file_path, sym.line)
                        .unwrap_or(sym.line);
                    candidates.push((rel.clone(), body_line));
                }
            }
        }
    }

    match candidates.len() {
        0 => {
            if let Some(filter) = file_filter {
                Err(miette!(
                    "function '{}' not found in '{}'",
                    name,
                    filter,
                ))
            } else {
                Err(miette!("function '{}' not found in project", name))
            }
        }
        1 => Ok(candidates.into_iter().next().unwrap()),
        _ => {
            if file_filter.is_some() {
                // Multiple overloads in same file — just use the first
                Ok(candidates.into_iter().next().unwrap())
            } else {
                let list = candidates
                    .iter()
                    .map(|(f, l)| format!("  {}:{}", f, l))
                    .collect::<Vec<_>>()
                    .join("\n");
                Err(miette!(
                    "function '{}' is ambiguous — found in {} files:\n{}\n\n\
                     Use --file to disambiguate, e.g.:\n  \
                     gd debug break --name {} --file {}",
                    name,
                    candidates.len(),
                    list,
                    name,
                    candidates[0].0,
                ))
            }
        }
    }
}

/// Find the line number of the first executable statement inside a function body.
/// `func_line` is 1-based (the `func` declaration line from symbols).
/// Returns the 1-based line of the first non-comment, non-empty statement in the body.
fn find_first_body_line(file_path: &std::path::Path, func_line: u32) -> Option<u32> {
    let source = std::fs::read_to_string(file_path).ok()?;
    let tree = crate::core::parser::parse(&source).ok()?;
    let root = tree.root_node();

    // Find the function_definition or constructor_definition at this line
    let target_row = func_line - 1; // tree-sitter is 0-based
    let func_node = find_function_at_line(root, target_row)?;

    // Get the body node
    let body = func_node.child_by_field_name("body")?;

    // Find the first non-comment child of the body
    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        if child.is_named() && child.kind() != "comment" {
            return Some(child.start_position().row as u32 + 1); // 1-based
        }
    }
    None
}

/// Recursively find a function_definition or constructor_definition node at the given row.
fn find_function_at_line(node: tree_sitter::Node, target_row: u32) -> Option<tree_sitter::Node> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if matches!(child.kind(), "function_definition" | "constructor_definition")
            && child.start_position().row as u32 == target_row
        {
            return Some(child);
        }
        // Recurse into class bodies
        if let Some(found) = find_function_at_line(child, target_row) {
            return Some(found);
        }
    }
    None
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

fn repl_step_out() {
    let initial_depth = get_stack_frames().len();
    if initial_depth <= 1 {
        println!(
            "{}",
            "Cannot step out — already at the top-level frame.".yellow()
        );
        return;
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    loop {
        if daemon_dap("dap_next", serde_json::json!({})).is_none() {
            println!("{}", "Failed to step.".red());
            return;
        }
        if daemon_dap_timeout(
            "dap_wait_stopped",
            serde_json::json!({"timeout": 5}),
            5,
        )
        .is_none()
        {
            println!("{}", "Step-out timed out waiting for stop.".yellow());
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
        if get_stack_frames().len() < initial_depth {
            println!("{}", "Stepped out".green());
            return;
        }
        if std::time::Instant::now() >= deadline {
            println!(
                "{}",
                "Step-out timed out after 15s — function may have a long-running loop.".yellow()
            );
            return;
        }
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
