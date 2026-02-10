use clap::Args;
use miette::Result;

#[derive(Args)]
pub struct BuildArgs {
    /// Export preset name
    #[arg(short, long)]
    pub preset: Option<String>,
    /// Output path
    #[arg(short, long)]
    pub output: Option<String>,
    /// Build in release mode
    #[arg(long)]
    pub release: bool,
}

pub fn exec(args: BuildArgs) -> Result<()> {
    crate::build::export_project(args.preset.as_deref(), args.output.as_deref(), args.release)
}
