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
    /// Run without the eval server (disables `gd eval` and `gd debug` input commands)
    #[arg(long)]
    pub bare: bool,
    /// Use file-based IPC for eval server instead of TCP
    #[arg(long)]
    pub file_ipc: bool,
    /// Extra args to pass to Godot
    #[arg(last = true)]
    pub extra: Vec<String>,
}

pub fn exec(args: &RunArgs) -> Result<()> {
    crate::build::run_project(
        args.scene.as_deref(),
        args.debug,
        args.verbose,
        !args.bare,
        args.file_ipc,
        &args.extra,
    )
}
