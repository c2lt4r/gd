use clap::Args;
use miette::Result;

#[derive(Args)]
pub struct LspArgs {
    /// Port for Godot's built-in LSP server (default: 6005)
    #[arg(long, default_value = "6005")]
    pub godot_port: u16,
    /// Disable proxy to Godot's built-in LSP server
    #[arg(long)]
    pub no_godot_proxy: bool,
}

#[allow(clippy::unnecessary_wraps)]
pub fn exec(args: &LspArgs) -> Result<()> {
    let port = if args.no_godot_proxy {
        0
    } else {
        args.godot_port
    };
    crate::lsp::run_server_with_options(port);
    Ok(())
}
