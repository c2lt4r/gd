use clap::Args;
use miette::Result;
use owo_colors::OwoColorize;

#[derive(Args)]
pub struct RunArgs {
    /// Scene to run (defaults to main scene)
    pub scene: Option<String>,
    /// Run in debug mode
    #[arg(short, long)]
    pub debug: bool,
    /// Run in verbose mode
    #[arg(short, long)]
    pub verbose: bool,
    /// Skip DAP launch (always spawn Godot directly)
    #[arg(long)]
    pub no_dap: bool,
    /// Extra args to pass to Godot
    #[arg(last = true)]
    pub extra: Vec<String>,
}

pub fn exec(args: RunArgs) -> Result<()> {
    // Try launching through the daemon's DAP connection (Godot editor running).
    // Skip DAP if: --no-dap, extra args, or a specific scene (DAP can't select scenes).
    if !args.no_dap
        && args.extra.is_empty()
        && args.scene.is_none()
        && let Some(result) = try_dap_launch()
    {
        return result;
    }

    // Fallback: spawn Godot directly
    crate::build::run_project(args.scene.as_deref(), args.debug, args.verbose, &args.extra)
}

/// Try to launch the game via DAP (through the daemon).
/// Returns Some(result) if DAP was available, None to fall back to direct launch.
fn try_dap_launch() -> Option<Result<()>> {
    use std::time::Duration;

    // Send launch request with long timeout (game startup can be slow)
    let result = crate::lsp::daemon_client::query_daemon(
        "dap_launch",
        serde_json::json!({}),
        Some(Duration::from_secs(30)),
    )?;

    // DAP is available — we're committed to this path now
    let launched = result.get("launched").and_then(|l| l.as_bool()) == Some(true);
    if !launched {
        eprintln!(
            "{} DAP launch failed, falling back to direct launch",
            "!".yellow()
        );
        return None;
    }

    let binary = result
        .pointer("/process/name")
        .and_then(|n| n.as_str())
        .unwrap_or("Godot");

    println!(
        "{} Launched via DAP ({}) — debugging active",
        "▶".green(),
        binary.bold()
    );
    println!(
        "  Use {} to interact, {} to terminate",
        "gd debug".cyan(),
        "gd debug stop".cyan(),
    );

    Some(Ok(()))
}
