use clap::Args;
use miette::Result;

#[derive(Args)]
pub struct LspArgs {
    /// Use stdio transport (default)
    #[arg(long, default_value_t = true)]
    pub stdio: bool,
}

pub fn exec(_args: LspArgs) -> Result<()> {
    crate::lsp::run_server();
    Ok(())
}
