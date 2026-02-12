use clap::{Args, Subcommand};
use miette::Result;
use owo_colors::OwoColorize;

#[derive(Args)]
pub struct LspArgs {
    #[command(subcommand)]
    pub command: Option<LspCommand>,
}

#[derive(Subcommand)]
pub enum LspCommand {
    /// Rename a symbol across the project
    Rename {
        #[command(flatten)]
        pos: QueryPositionArgs,
        /// New name for the symbol
        #[arg(long)]
        new_name: String,
        /// Preview the rename without writing changes
        #[arg(long)]
        dry_run: bool,
    },
    /// Find all references to a symbol
    References {
        #[command(flatten)]
        pos: QueryPositionArgs,
    },
    /// Go to the definition of a symbol
    Definition {
        #[command(flatten)]
        pos: QueryPositionArgs,
    },
    /// Show hover information for a symbol
    Hover {
        #[command(flatten)]
        pos: QueryPositionArgs,
    },
    /// List completions at a position
    Completions {
        #[command(flatten)]
        pos: QueryPositionArgs,
    },
    /// List available code actions at a position
    CodeActions {
        #[command(flatten)]
        pos: QueryPositionArgs,
    },
    /// Run diagnostics on files (same as gd lint --format json)
    Diagnostics {
        /// Files or directories to check (defaults to current directory)
        paths: Vec<String>,
    },
    /// List symbols in a file
    Symbols {
        /// Path to the GDScript file
        #[arg(long)]
        file: String,
    },
}

#[derive(Args)]
pub struct QueryPositionArgs {
    /// Path to the GDScript file
    #[arg(long)]
    pub file: String,
    /// Line number (1-based)
    #[arg(long)]
    pub line: usize,
    /// Column number (1-based)
    #[arg(long)]
    pub column: usize,
}

pub fn exec(args: LspArgs) -> Result<()> {
    let Some(command) = args.command else {
        // No subcommand — start the LSP server (backward compatible)
        crate::lsp::run_server();
        return Ok(());
    };

    match command {
        LspCommand::Rename {
            pos,
            new_name,
            dry_run,
        } => {
            let result =
                crate::lsp::query::query_rename(&pos.file, pos.line, pos.column, &new_name)?;
            let json = serde_json::to_string_pretty(&result).map_err(|e| miette::miette!("{e}"))?;
            println!("{json}");

            if !dry_run {
                let project_root = crate::core::config::find_project_root(
                    &std::env::current_dir()
                        .map_err(|e| miette::miette!("{e}"))?
                        .join(&pos.file),
                )
                .ok_or_else(|| miette::miette!("no project.godot found"))?;

                let count = crate::lsp::query::apply_rename(&result, &project_root)?;
                eprintln!(
                    "{} Applied rename across {} file{}",
                    "✓".green().bold(),
                    count,
                    if count == 1 { "" } else { "s" }
                );
            }
            Ok(())
        }
        LspCommand::References { pos } => {
            let result = crate::lsp::query::query_references(&pos.file, pos.line, pos.column)?;
            let json = serde_json::to_string_pretty(&result).map_err(|e| miette::miette!("{e}"))?;
            println!("{json}");
            Ok(())
        }
        LspCommand::Definition { pos } => {
            let result = crate::lsp::query::query_definition(&pos.file, pos.line, pos.column)?;
            let json = serde_json::to_string_pretty(&result).map_err(|e| miette::miette!("{e}"))?;
            println!("{json}");
            Ok(())
        }
        LspCommand::Hover { pos } => {
            let result = crate::lsp::query::query_hover(&pos.file, pos.line, pos.column)?;
            let json = serde_json::to_string_pretty(&result).map_err(|e| miette::miette!("{e}"))?;
            println!("{json}");
            Ok(())
        }
        LspCommand::Completions { pos } => {
            let result = crate::lsp::query::query_completions(&pos.file, pos.line, pos.column)?;
            let json = serde_json::to_string_pretty(&result).map_err(|e| miette::miette!("{e}"))?;
            println!("{json}");
            Ok(())
        }
        LspCommand::CodeActions { pos } => {
            let result = crate::lsp::query::query_code_actions(&pos.file, pos.line, pos.column)?;
            let json = serde_json::to_string_pretty(&result).map_err(|e| miette::miette!("{e}"))?;
            println!("{json}");
            Ok(())
        }
        LspCommand::Diagnostics { paths } => crate::lsp::query::query_diagnostics(&paths),
        LspCommand::Symbols { file } => {
            let result = crate::lsp::query::query_symbols(&file)?;
            let json = serde_json::to_string_pretty(&result).map_err(|e| miette::miette!("{e}"))?;
            println!("{json}");
            Ok(())
        }
    }
}
