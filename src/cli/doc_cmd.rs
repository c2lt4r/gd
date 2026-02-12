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
    /// Output format
    #[arg(long, default_value = "human")]
    pub format: String,
    /// Check that all public methods have doc comments (exit 1 if not)
    #[arg(long)]
    pub check: bool,
}

pub fn exec(args: DocArgs) -> Result<()> {
    if args.check {
        return crate::doc::run_doc_check(&args.paths);
    }
    match args.format.as_str() {
        "human" => crate::doc::run_doc(&args.paths, &args.output_dir, args.stdout),
        "json" => crate::doc::run_doc_json(&args.paths),
        _ => Err(miette::miette!("invalid format '{}' (expected 'human' or 'json')", args.format)),
    }
}
