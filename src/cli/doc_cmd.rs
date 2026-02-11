use clap::Args;
use miette::Result;

#[derive(Args)]
pub struct DocArgs {
    /// Files or directories to document (defaults to current directory)
    pub paths: Vec<String>,
    /// Output directory for documentation (default: "docs")
    #[arg(long, default_value = "docs")]
    pub output_dir: String,
    /// Print documentation to stdout instead of files
    #[arg(long)]
    pub stdout: bool,
}

pub fn exec(args: DocArgs) -> Result<()> {
    crate::doc::run_doc(&args.paths, &args.output_dir, args.stdout)
}
