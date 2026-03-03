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
    // Windows default stack is 1MB (vs 8MB on Unix); deep AST walks in
    // `gd check` overflow it. Spawn on an 8MB thread for consistency.
    const STACK_SIZE: usize = 8 * 1024 * 1024;
    std::thread::Builder::new()
        .stack_size(STACK_SIZE)
        .spawn(|| {
            let cli = Cli::parse();
            core::color::init(cli.no_color);
            cli::run(cli)
        })
        .expect("failed to spawn main thread")
        .join()
        .expect("main thread panicked")
}
