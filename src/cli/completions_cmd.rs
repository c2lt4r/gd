use clap::Args;
use miette::Result;

#[derive(Args)]
pub struct CompletionsArgs {
    /// Shell to generate completions for
    pub shell: clap_complete::Shell,
}

#[allow(clippy::unnecessary_wraps)]
pub fn exec(args: &CompletionsArgs) -> Result<()> {
    use clap::CommandFactory;

    let mut cmd = super::Cli::command();
    clap_complete::generate(args.shell, &mut cmd, "gd", &mut std::io::stdout());

    Ok(())
}
