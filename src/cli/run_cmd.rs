use clap::Args;
use miette::Result;

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
    /// Stream Godot's stdout/stderr to the terminal
    #[arg(short, long)]
    pub log: bool,
    /// Extra args to pass to Godot
    #[arg(last = true)]
    pub extra: Vec<String>,
}

pub fn exec(args: &RunArgs) -> Result<()> {
    crate::build::run_project(
        args.scene.as_deref(),
        args.debug,
        args.verbose,
        args.log,
        &args.extra,
    )
}
