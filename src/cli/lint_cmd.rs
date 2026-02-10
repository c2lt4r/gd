use clap::Args;
use miette::Result;

#[derive(Args)]
pub struct LintArgs {
    /// Files or directories to lint (defaults to current directory)
    pub paths: Vec<String>,
    /// Output format
    #[arg(long, default_value = "human")]
    pub format: String,
    /// Fix auto-fixable issues
    #[arg(long)]
    pub fix: bool,
}

pub fn exec(args: LintArgs) -> Result<()> {
    crate::lint::run_lint(&args.paths, &args.format, args.fix)
}
