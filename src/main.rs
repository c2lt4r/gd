use clap::Parser;
use miette::Result;

mod build;
mod class_db;
mod cli;
mod core;
mod debug;
mod doc;
mod fmt;
mod lint;
mod lsp;
mod scaffold;

use cli::Cli;

fn main() -> Result<()> {
    let cli = Cli::parse();
    core::color::init(cli.no_color);
    cli::run(cli)
}
