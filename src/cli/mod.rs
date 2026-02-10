pub mod fmt_cmd;
pub mod lint_cmd;
pub mod new_cmd;
pub mod run_cmd;
pub mod build_cmd;
pub mod check_cmd;
pub mod clean_cmd;

use clap::{Parser, Subcommand};
use miette::Result;

#[derive(Parser)]
#[command(name = "gd", version, about = "The Godot toolchain")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Create a new Godot project
    New(new_cmd::NewArgs),
    /// Format GDScript files
    Fmt(fmt_cmd::FmtArgs),
    /// Lint GDScript files
    Lint(lint_cmd::LintArgs),
    /// Run the Godot project
    Run(run_cmd::RunArgs),
    /// Build/export the Godot project
    Build(build_cmd::BuildArgs),
    /// Check project for errors without building
    Check(check_cmd::CheckArgs),
    /// Clean build artifacts
    Clean(clean_cmd::CleanArgs),
}

pub fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Command::New(args) => new_cmd::exec(args),
        Command::Fmt(args) => fmt_cmd::exec(args),
        Command::Lint(args) => lint_cmd::exec(args),
        Command::Run(args) => run_cmd::exec(args),
        Command::Build(args) => build_cmd::exec(args),
        Command::Check(args) => check_cmd::exec(args),
        Command::Clean(args) => clean_cmd::exec(args),
    }
}
