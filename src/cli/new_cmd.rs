use clap::Args;
use miette::Result;

#[derive(Args)]
pub struct NewArgs {
    /// Name of the project to create
    pub name: String,
    /// Template to use (default, 2d, 3d)
    #[arg(short, long, default_value = "default")]
    pub template: String,
    /// GitHub repository to use as template (owner/repo or owner/repo@ref)
    #[arg(long)]
    pub from: Option<String>,
}

pub fn exec(args: NewArgs) -> Result<()> {
    if let Some(from) = &args.from {
        crate::scaffold::create_from_github(&args.name, from)
    } else {
        crate::scaffold::create_project(&args.name, &args.template)
    }
}
