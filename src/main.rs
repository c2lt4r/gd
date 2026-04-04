use clap::Parser;
use miette::Result;

mod build;
mod cli;
mod doc;
mod scaffold;

use cli::Cli;

fn main() -> Result<()> {
    const STACK_SIZE: usize = 8 * 1024 * 1024;

    // Reset SIGPIPE to default (die silently) so piped output
    // (e.g. `gd lsp symbols | head`) does not trigger a println! panic.
    #[cfg(unix)]
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }

    // Windows default stack is 1MB (vs 8MB on Unix); deep AST walks in
    // `gd check` overflow it. Spawn on an 8MB thread for consistency.
    let result = std::thread::Builder::new()
        .stack_size(STACK_SIZE)
        .spawn(|| {
            let cli = Cli::parse();
            gd_core::color::init(cli.no_color);
            cli::run(cli)
        })
        .expect("failed to spawn main thread")
        .join();

    match result {
        Ok(inner) => inner,
        Err(payload) => {
            // If the worker thread panicked from a broken pipe, exit quietly.
            let msg = payload
                .downcast_ref::<String>()
                .map(String::as_str)
                .or_else(|| payload.downcast_ref::<&str>().copied())
                .unwrap_or("");
            if msg.contains("Broken pipe") {
                std::process::exit(141);
            }
            std::panic::resume_unwind(payload);
        }
    }
}
