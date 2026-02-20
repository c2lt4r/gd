pub mod addons_cmd;
pub mod build_cmd;
pub mod check_cmd;
pub mod ci_cmd;
pub mod clean_cmd;
pub mod completions_cmd;
pub mod daemon_cmd;
pub mod debug_cmd;
pub mod deps_cmd;
pub mod doc_cmd;
pub mod env_cmd;
pub mod eval_cmd;
pub mod fmt_cmd;
pub mod init_cmd;
pub mod lint_cmd;
pub mod llm_cmd;
pub mod log_cmd;
pub mod lsp_cmd;
pub mod man_cmd;
pub mod mesh_cmd;
pub mod new_cmd;
pub mod resource_cmd;
pub mod run_cmd;
pub mod scene_cmd;
pub mod stats_cmd;
pub mod stop_cmd;
pub mod test_cmd;
pub mod tree_cmd;
pub mod upgrade_cmd;
pub mod watch_cmd;

use clap::{Parser, Subcommand};
use miette::Result;

#[derive(Parser)]
#[command(name = "gd", version, about = "The Godot toolchain")]
pub struct Cli {
    /// Disable colored output (also respects NO_COLOR env)
    #[arg(long, global = true)]
    pub no_color: bool,
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
    /// Stop the running Godot game
    Stop,
    /// Build/export the Godot project
    Build(build_cmd::BuildArgs),
    /// Check project for errors without building
    Check(check_cmd::CheckArgs),
    /// Clean build artifacts
    Clean(clean_cmd::CleanArgs),
    /// Test runner and automation
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
    /// Manage the background daemon
    Daemon(daemon_cmd::DaemonArgs),
    /// Debug a running Godot game via Godot's binary debug protocol
    Debug(debug_cmd::DebugArgs),
    /// Manage .tres resource files
    Resource(resource_cmd::ResourceArgs),
    /// Manage .tscn scene files
    Scene(scene_cmd::SceneArgs),
    /// Show project statistics
    Stats(stats_cmd::StatsArgs),
    /// Generate CI/CD pipeline configuration
    Ci(ci_cmd::CiArgs),
    /// View game output log (print, errors, warnings)
    Log(log_cmd::LogArgs),
    /// Start the Language Server Protocol server
    Lsp(lsp_cmd::LspArgs),
    /// AI-assisted 3D mesh building (experimental)
    Mesh(mesh_cmd::MeshArgs),
    /// Evaluate a GDScript expression or run a script
    Eval(eval_cmd::EvalArgs),
    /// Show environment info (gd version, Godot version, paths)
    Env(env_cmd::EnvArgs),
    /// Show script dependency graph
    Deps(deps_cmd::DepsArgs),
    /// Generate man pages
    Man(man_cmd::ManArgs),
    /// Upgrade gd to the latest version
    Upgrade(upgrade_cmd::UpgradeArgs),
    /// Print AI-readable command reference (like llms.txt)
    Llm,
}

pub fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Command::New(ref args) => new_cmd::exec(args),
        Command::Init(ref args) => init_cmd::exec(args),
        Command::Fmt(ref args) => fmt_cmd::exec(args),
        Command::Lint(args) => lint_cmd::exec(args),
        Command::Run(ref args) => run_cmd::exec(args),
        Command::Stop => stop_cmd::exec(),
        Command::Build(ref args) => build_cmd::exec(args),
        Command::Check(ref args) => check_cmd::exec(args),
        Command::Clean(ref args) => clean_cmd::exec(args),
        Command::Test(ref args) => test_cmd::exec(args),
        Command::Completions(ref args) => completions_cmd::exec(args),
        Command::Tree(ref args) => tree_cmd::exec(args),
        Command::Doc(ref args) => doc_cmd::exec(args),
        Command::Watch(ref args) => watch_cmd::exec(args),
        Command::Addons(args) => addons_cmd::exec(args),
        Command::Daemon(args) => daemon_cmd::exec(args),
        Command::Debug(ref args) => debug_cmd::exec(args),
        Command::Resource(ref args) => resource_cmd::exec(args),
        Command::Scene(ref args) => scene_cmd::exec(args),
        Command::Stats(ref args) => stats_cmd::exec(args),
        Command::Ci(args) => ci_cmd::exec(args),
        Command::Log(ref args) => log_cmd::exec(args),
        Command::Lsp(args) => lsp_cmd::exec(args),
        Command::Mesh(ref args) => mesh_cmd::exec(args),
        Command::Eval(ref args) => eval_cmd::exec(args),
        Command::Env(ref args) => env_cmd::exec(args),
        Command::Deps(ref args) => deps_cmd::exec(args),
        Command::Man(ref args) => man_cmd::exec(args),
        Command::Upgrade(ref args) => upgrade_cmd::exec(args),
        Command::Llm => llm_cmd::exec(),
    }
}
