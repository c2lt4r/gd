use clap::{Args, Subcommand};
use miette::{Result, miette};
use owo_colors::OwoColorize;

use crate::debug::dap_client::DapClient;
use crate::debug::{BreakpointResult, Scope, StackFrame, Variable};

#[derive(Args)]
pub struct DebugArgs {
    /// DAP server port (Godot default: 6006)
    #[arg(long, default_value = "6006")]
    pub port: u16,

    /// DAP server host
    #[arg(long, default_value = "localhost")]
    pub host: String,

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
}

#[derive(Args)]
pub struct BreakArgs {
    /// Script file path (relative to project root, e.g. scripts/kart.gd)
    #[arg(long)]
    pub file: String,
    /// Line numbers to set breakpoints on
    #[arg(long, num_args = 1..)]
    pub line: Vec<u32>,
    /// Timeout in seconds to wait for breakpoint hit (default: 30)
    #[arg(long, default_value = "30")]
    pub timeout: u64,
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
        DebugCommand::Attach => cmd_attach(&args.host, args.port),
        DebugCommand::Break(a) => cmd_break(&args.host, args.port, a),
        DebugCommand::Status(a) => cmd_status(&args.host, args.port, a),
    }
}

// ── Helpers ──────────────────────────────────────────────────────────

fn connect_and_handshake(host: &str, port: u16) -> Result<DapClient> {
    let client = DapClient::connect(host, port).ok_or_else(|| {
        miette!(
            "Could not connect to Godot DAP server on {host}:{port}\n  Is the Godot editor running?"
        )
    })?;
    client
        .handshake()
        .ok_or_else(|| miette!("DAP handshake failed — Godot may not be in a debug session"))?;
    Ok(client)
}

/// Resolve a relative script path using the editor's project path from DAP.
/// Godot's DAP requires the exact path prefix the editor is using.
fn resolve_script_path(relative: &str, client: &DapClient) -> Result<String> {
    // Verify the file exists locally
    let cwd =
        std::env::current_dir().map_err(|e| miette!("Failed to get current directory: {e}"))?;
    let project = crate::core::project::GodotProject::discover(&cwd)?;
    let full = project.root.join(relative);

    if !full.exists() {
        return Err(miette!("Script not found: {relative}"));
    }

    // Use the editor's project path discovered during DAP handshake.
    // This ensures the path prefix matches exactly what Godot expects,
    // regardless of platform differences (WSL casing, Windows \\?\, etc.)
    let editor_root = client.project_path().ok_or_else(|| {
        miette!("Could not determine the editor's project path — breakpoints may not work")
    })?;

    // Normalize relative to forward slashes and join with editor root
    let relative_fwd = relative.replace('\\', "/");
    Ok(format!("{editor_root}/{relative_fwd}"))
}

// ── Interactive session ──────────────────────────────────────────────

fn cmd_attach(host: &str, port: u16) -> Result<()> {
    let client = connect_and_handshake(host, port)?;
    println!(
        "{} {}:{}",
        "Attached to Godot DAP".green().bold(),
        host,
        port
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
                if client.continue_execution(1).is_some() {
                    println!("{}", "Continued".green());
                } else {
                    println!("{}", "Failed to continue".red());
                }
            }
            "pause" | "p" => {
                if client.pause(1).is_some() {
                    println!("{}", "Paused".green());
                } else {
                    println!("{}", "Failed to pause".red());
                }
            }
            "next" | "n" => {
                if client.next(1).is_some() {
                    println!("{}", "Stepped over".green());
                } else {
                    println!("{}", "Failed to step over".red());
                }
            }
            "step" | "s" => {
                if client.step_in(1).is_some() {
                    println!("{}", "Stepped in".green());
                } else {
                    println!("{}", "Failed to step in".red());
                }
            }
            "stack" | "bt" => repl_stack(&client),
            "vars" => repl_vars(&client, args.first().copied()),
            "expand" => {
                if let Some(ref_str) = args.first() {
                    if let Ok(vref) = ref_str.parse::<i64>() {
                        repl_expand(&client, vref);
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
                    repl_eval(&client, &expr);
                }
            }
            "break" | "b" => {
                if args.len() < 2 {
                    println!("Usage: break <file> <line> [line2 ...]");
                } else {
                    repl_break(&client, args[0], &args[1..]);
                }
            }
            "clear" => {
                if args.is_empty() {
                    println!("Usage: clear <file>");
                } else {
                    repl_clear(&client, args[0]);
                }
            }
            "wait" => {
                let timeout = args
                    .first()
                    .and_then(|s| s.parse::<u64>().ok())
                    .unwrap_or(30);
                repl_wait(&client, timeout);
            }
            _ => println!("Unknown command: {}. Type 'help' for commands.", cmd.red()),
        }
    }

    client.disconnect();
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

fn repl_stack(client: &DapClient) {
    let frames = get_stack_frames(client);
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

fn repl_vars(client: &DapClient, scope_filter: Option<&str>) {
    let frames = get_stack_frames(client);
    let Some(frame) = frames.first() else {
        println!(
            "{}",
            "No stack frames — game may not be paused at a breakpoint.".yellow()
        );
        return;
    };

    let Some(scopes_body) = client.scopes(frame.id) else {
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
            && let Some(body) = client.variables(scope.variables_reference)
        {
            let vars = parse_variables(&body);
            let _ = print_variables(&vars, &OutputFormat::Human, Some(&scope.name));
        }
    }
}

fn repl_expand(client: &DapClient, vref: i64) {
    if let Some(body) = client.variables(vref) {
        let vars = parse_variables(&body);
        let _ = print_variables(&vars, &OutputFormat::Human, None);
    } else {
        println!("{}", "Failed to expand variable.".red());
    }
}

fn repl_eval(client: &DapClient, expr: &str) {
    if let Some(body) = client.evaluate(expr, "repl") {
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

fn repl_break(client: &DapClient, file: &str, line_strs: &[&str]) {
    let lines: Vec<u32> = line_strs
        .iter()
        .filter_map(|s| s.parse::<u32>().ok())
        .collect();
    if lines.is_empty() {
        println!("No valid line numbers provided.");
        return;
    }

    let path = match resolve_script_path(file, client) {
        Ok(p) => p,
        Err(e) => {
            println!("{}", e);
            return;
        }
    };

    if let Some(body) = client.set_breakpoints(&path, &lines) {
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

fn repl_clear(client: &DapClient, file: &str) {
    let path = match resolve_script_path(file, client) {
        Ok(p) => p,
        Err(e) => {
            println!("{}", e);
            return;
        }
    };

    if client.set_breakpoints(&path, &[]).is_some() {
        println!("{} {}", "Cleared breakpoints in".green(), file.cyan());
    } else {
        println!("{}", "Failed to clear breakpoints.".red());
    }
}

fn repl_wait(client: &DapClient, timeout: u64) {
    println!(
        "{} (timeout: {}s)...",
        "Waiting for breakpoint hit".dimmed(),
        timeout
    );

    if client.wait_for_stopped(timeout).is_some() {
        println!("{}", "Breakpoint hit!".green().bold());
        repl_stack(client);
        repl_vars(client, None);
    } else {
        println!(
            "{}",
            format!("Timeout — no breakpoint hit within {timeout}s.").yellow()
        );
    }
}

// ── One-shot: break --wait ──────────────────────────────────────────

fn cmd_break(host: &str, port: u16, args: BreakArgs) -> Result<()> {
    if args.line.is_empty() {
        return Err(miette!("At least one --line is required"));
    }

    let client = connect_and_handshake(host, port)?;
    let path = resolve_script_path(&args.file, &client)?;

    let body = client.set_breakpoints(&path, &args.line).ok_or_else(|| {
        miette!("Failed to set breakpoints — check that the file path is correct")
    })?;

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
            args.file.cyan(),
            bp.line,
            status,
        );
    }

    // Continue and wait for hit
    println!(
        "\n{} (timeout: {}s)...",
        "Waiting for breakpoint hit".dimmed(),
        args.timeout,
    );

    let _ = client.continue_execution(1);

    if client.wait_for_stopped(args.timeout).is_none() {
        client.disconnect();
        return Err(miette!(
            "Timeout — breakpoint was not hit within {}s",
            args.timeout
        ));
    }

    println!("{}", "Breakpoint hit!".green().bold());

    let frames = get_stack_frames(&client);
    let mut all_vars: Vec<(String, Vec<Variable>)> = Vec::new();
    if let Some(frame_id) = frames.first().map(|f| f.id)
        && let Some(scopes_body) = client.scopes(frame_id)
        && let Some(scopes) = scopes_body["scopes"].as_array()
    {
        for scope in scopes {
            let name = scope["name"].as_str().unwrap_or("?").to_string();
            let vref = scope["variablesReference"].as_i64().unwrap_or(0);
            if vref > 0
                && let Some(vbody) = client.variables(vref)
            {
                all_vars.push((name, parse_variables(&vbody)));
            }
        }
    }

    client.disconnect();

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
                print_variables(vars, &OutputFormat::Human, Some(scope_name))?;
            }
        }
    }

    Ok(())
}

// ── One-shot: status ────────────────────────────────────────────────

fn cmd_status(host: &str, port: u16, args: StatusArgs) -> Result<()> {
    let client = DapClient::connect(host, port).ok_or_else(|| {
        miette!(
            "Could not connect to Godot DAP server on {host}:{port}\n  Is the Godot editor running?"
        )
    })?;

    let caps = client
        .handshake()
        .ok_or_else(|| miette!("DAP handshake failed"))?;

    let threads_body = client.threads();
    client.disconnect();

    match args.format {
        OutputFormat::Json => {
            let status = serde_json::json!({
                "connected": true,
                "host": host,
                "port": port,
                "capabilities": caps,
                "threads": threads_body.map(|t| t["threads"].clone()),
            });
            println!("{}", serde_json::to_string_pretty(&status).unwrap());
        }
        OutputFormat::Human => {
            println!(
                "{} {}:{}",
                "Connected to Godot DAP".green().bold(),
                host,
                port
            );
            println!();
            println!("{}", "Capabilities:".bold());
            if let Some(obj) = caps.as_object() {
                for (k, v) in obj {
                    if v.as_bool() == Some(true) {
                        println!("  {} {}", "+".green(), k);
                    }
                }
            }
            if let Some(body) = threads_body
                && let Some(threads) = body["threads"].as_array()
            {
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

// ── Shared helpers ──────────────────────────────────────────────────

fn get_stack_frames(client: &DapClient) -> Vec<StackFrame> {
    let thread_id = client
        .threads()
        .and_then(|b| b["threads"].as_array()?.first()?.get("id")?.as_i64())
        .unwrap_or(1);

    client
        .stack_trace(thread_id)
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
