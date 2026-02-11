pub mod fmt_cmd;
pub mod init_cmd;
pub mod lint_cmd;
pub mod new_cmd;
pub mod run_cmd;
pub mod build_cmd;
pub mod check_cmd;
pub mod clean_cmd;
pub mod test_cmd;
pub mod completions_cmd;
pub mod tree_cmd;
pub mod doc_cmd;
pub mod watch_cmd;
pub mod addons_cmd;
pub mod stats_cmd;
pub mod ci_cmd;
pub mod lsp_cmd;
pub mod deps_cmd;

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
    /// Initialize gd toolchain in an existing Godot project
    Init(init_cmd::InitArgs),
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
    /// Run GDScript tests
    Test(test_cmd::TestArgs),
    /// Generate shell completions
    Completions(completions_cmd::CompletionsArgs),
    /// Show project class hierarchy
    Tree(tree_cmd::TreeArgs),
    /// Generate documentation from GDScript doc comments
    Doc(doc_cmd::DocArgs),
    /// Watch files and run fmt/lint on changes
    Watch(watch_cmd::WatchArgs),
    /// Manage project addons
    Addons(addons_cmd::AddonsArgs),
    /// Show project statistics
    Stats(stats_cmd::StatsArgs),
    /// Generate CI/CD pipeline configuration
    Ci(ci_cmd::CiArgs),
    /// Start the Language Server Protocol server
    Lsp(lsp_cmd::LspArgs),
    /// Show script dependency graph
    Deps(deps_cmd::DepsArgs),
}

pub fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Command::New(args) => new_cmd::exec(args),
        Command::Init(args) => init_cmd::exec(args),
        Command::Fmt(args) => fmt_cmd::exec(args),
        Command::Lint(args) => lint_cmd::exec(args),
        Command::Run(args) => run_cmd::exec(args),
        Command::Build(args) => build_cmd::exec(args),
        Command::Check(args) => check_cmd::exec(args),
        Command::Clean(args) => clean_cmd::exec(args),
        Command::Test(args) => test_cmd::exec(args),
        Command::Completions(args) => completions_cmd::exec(args),
        Command::Tree(args) => tree_cmd::exec(args),
        Command::Doc(args) => doc_cmd::exec(args),
        Command::Watch(args) => watch_cmd::exec(args),
        Command::Addons(args) => addons_cmd::exec(args),
        Command::Stats(args) => stats_cmd::exec(args),
        Command::Ci(args) => ci_cmd::exec(args),
        Command::Lsp(args) => lsp_cmd::exec(args),
        Command::Deps(args) => deps_cmd::exec(args),
    }
}
