use clap::Parser;
use miette::Result;

mod cli;
mod core;
mod fmt;
mod lint;
mod scaffold;
mod build;
mod doc;

use cli::Cli;

fn main() -> Result<()> {
    let cli = Cli::parse();
    cli::run(cli)
}
