use clap::Args;
use miette::Result;

#[derive(Args)]
pub struct FmtArgs {
    /// Files or directories to format (defaults to current directory)
    pub paths: Vec<String>,
    /// Check formatting without modifying files
    #[arg(long)]
    pub check: bool,
    /// Show diff of formatting changes
    #[arg(long)]
    pub diff: bool,
}

pub fn exec(args: &FmtArgs) -> Result<()> {
    crate::fmt::run_fmt(&args.paths, args.check, args.diff)
}
