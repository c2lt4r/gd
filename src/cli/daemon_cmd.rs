use crate::cprintln;
use clap::{Args, Subcommand};
use miette::Result;

#[derive(Args)]
pub struct DaemonArgs {
    #[command(subcommand)]
    pub command: DaemonCommand,
}

#[derive(Subcommand)]
pub enum DaemonCommand {
    /// Show daemon connection status
    Status,
    /// Stop the background daemon
    Stop,
    /// Restart the background daemon
    Restart,
    /// Run the daemon server (internal — auto-started, not called directly)
    #[command(hide = true)]
    Serve {
        /// Project root directory
        #[arg(long)]
        project_root: String,
        /// Port for Godot's built-in LSP server (default: 6005)
        #[arg(long, default_value = "6005")]
        godot_port: u16,
    },
}

pub fn exec(args: DaemonArgs) -> Result<()> {
    match args.command {
        DaemonCommand::Status => {
            if let Some(result) =
                crate::lsp::daemon_client::query_daemon("status", serde_json::json!({}), None)
            {
                cprintln!("{}", serde_json::to_string_pretty(&result).unwrap());
            } else {
                cprintln!("Daemon not running");
            }
            Ok(())
        }
        DaemonCommand::Stop => {
            let cwd = std::env::current_dir().unwrap_or_default();
            match crate::core::config::find_project_root(&cwd) {
                Some(root) => {
                    if crate::lsp::daemon_client::stop_daemon(&root) {
                        cprintln!("Daemon stopped");
                    } else {
                        cprintln!("No daemon running");
                    }
                }
                None => cprintln!("Not in a Godot project"),
            }
            Ok(())
        }
        DaemonCommand::Restart => {
            let cwd = std::env::current_dir().unwrap_or_default();
            match crate::core::config::find_project_root(&cwd) {
                Some(root) => {
                    crate::lsp::daemon_client::stop_daemon(&root);
                    if let Some(result) = crate::lsp::daemon_client::query_daemon(
                        "status",
                        serde_json::json!({}),
                        None,
                    ) {
                        cprintln!(
                            "Daemon restarted\n{}",
                            serde_json::to_string_pretty(&result).unwrap()
                        );
                    } else {
                        cprintln!("Daemon stopped but failed to restart");
                    }
                }
                None => cprintln!("Not in a Godot project"),
            }
            Ok(())
        }
        DaemonCommand::Serve {
            project_root,
            godot_port,
        } => {
            let root = std::path::PathBuf::from(&project_root);
            if !root.join("project.godot").exists() {
                return Err(miette::miette!("no project.godot found in {project_root}"));
            }
            crate::lsp::daemon::run(&root, godot_port)
        }
    }
}
