use clap::Args;
use miette::{miette, Result};
use std::path::PathBuf;

#[derive(Args)]
pub struct ManArgs {
    /// Subcommand to generate man page for (omit for the main page)
    pub command: Option<String>,
    /// Write to file instead of stdout
    #[arg(short, long)]
    pub output: Option<PathBuf>,
}

pub fn exec(args: ManArgs) -> Result<()> {
    use clap::CommandFactory;

    let cmd = super::Cli::command();

    let target = if let Some(ref sub) = args.command {
        cmd.get_subcommands()
            .find(|c| c.get_name() == sub.as_str())
            .ok_or_else(|| miette!("Unknown subcommand: {sub}"))?
            .clone()
    } else {
        cmd
    };

    let man = clap_mangen::Man::new(target);
    let mut buf = Vec::new();
    man.render(&mut buf)
        .map_err(|e| miette!("Failed to render man page: {e}"))?;

    if let Some(ref path) = args.output {
        std::fs::write(path, &buf)
            .map_err(|e| miette!("Failed to write {}: {e}", path.display()))?;
    } else {
        use std::io::Write;
        std::io::stdout()
            .write_all(&buf)
            .map_err(|e| miette!("Failed to write to stdout: {e}"))?;
    }

    Ok(())
}
