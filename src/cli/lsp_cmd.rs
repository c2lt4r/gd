use clap::{Args, Subcommand};
use miette::Result;

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
        /// Search by symbol name across the project (alternative to --file/--line/--column)
        #[arg(long)]
        name: Option<String>,
        /// Path to the GDScript file
        #[arg(long)]
        file: Option<String>,
        /// Line number (1-based)
        #[arg(long)]
        line: Option<usize>,
        /// Column number (1-based)
        #[arg(long)]
        column: Option<usize>,
        /// Filter results to a specific class (requires --name)
        #[arg(long, requires = "name")]
        class: Option<String>,
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
        /// Maximum number of results to return
        #[arg(long)]
        limit: Option<usize>,
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
        /// Filter by symbol kind (repeatable, comma-separated: function, method, variable, field, class, constant, enum, event)
        #[arg(long)]
        kind: Vec<String>,
    },
    /// Delete a symbol from a file (top-level or within an inner class)
    DeleteSymbol {
        /// Path to the GDScript file
        #[arg(long)]
        file: String,
        /// Symbol name to delete (alternative to --line)
        #[arg(long)]
        name: Option<String>,
        /// Line number of declaration to delete (1-based; alternative to --name)
        #[arg(long)]
        line: Option<usize>,
        /// Inner class to operate within
        #[arg(long)]
        class: Option<String>,
        /// Delete even if references exist elsewhere
        #[arg(long)]
        force: bool,
        /// Preview without writing changes
        #[arg(long)]
        dry_run: bool,
    },
    /// Move a symbol from one file to another (top-level or between classes)
    MoveSymbol {
        /// Symbol name to move
        #[arg(long)]
        name: String,
        /// Source file
        #[arg(long)]
        from: String,
        /// Destination file (created if doesn't exist)
        #[arg(long)]
        to: String,
        /// Source inner class
        #[arg(long)]
        class: Option<String>,
        /// Target inner class (defaults to top-level)
        #[arg(long)]
        target_class: Option<String>,
        /// Preview without writing changes
        #[arg(long)]
        dry_run: bool,
    },
    /// Extract a range of lines into a new function
    ExtractMethod {
        /// Path to the GDScript file
        #[arg(long)]
        file: String,
        /// First line to extract (1-based, inclusive)
        #[arg(long)]
        start_line: usize,
        /// Last line to extract (1-based, inclusive)
        #[arg(long)]
        end_line: usize,
        /// Name for the extracted function
        #[arg(long)]
        name: String,
        /// Preview without writing changes
        #[arg(long)]
        dry_run: bool,
    },
    /// Inline a function call, replacing it with the function body
    InlineMethod {
        /// Path to the GDScript file
        #[arg(long)]
        file: String,
        /// Function name to inline everywhere (alternative to --line/--column)
        #[arg(long)]
        name: Option<String>,
        /// Inline all call sites and delete the function (requires --name)
        #[arg(long, requires = "name")]
        all: bool,
        /// Line number of call site (1-based)
        #[arg(long)]
        line: Option<usize>,
        /// Column number of call site (1-based)
        #[arg(long)]
        column: Option<usize>,
        /// Preview without writing changes
        #[arg(long)]
        dry_run: bool,
    },
    /// Change a function's signature and update all call sites
    ChangeSignature {
        /// Path to the GDScript file
        #[arg(long)]
        file: String,
        /// Function name
        #[arg(long)]
        name: String,
        /// Add parameter (format: "name: Type = default" or just "name"; repeatable)
        #[arg(long)]
        add_param: Vec<String>,
        /// Remove parameter by name (repeatable)
        #[arg(long)]
        remove_param: Vec<String>,
        /// Rename parameter (format: "old_name=new_name"; repeatable)
        #[arg(long)]
        rename_param: Vec<String>,
        /// Reorder parameters (comma-separated names in new order)
        #[arg(long)]
        reorder: Option<String>,
        /// Inner class containing the function
        #[arg(long)]
        class: Option<String>,
        /// Preview without writing changes
        #[arg(long)]
        dry_run: bool,
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
            let mut result =
                crate::lsp::query::query_rename(&pos.file, pos.line, pos.column, &new_name)?;

            if !dry_run {
                let project_root = crate::core::config::find_project_root(
                    &std::env::current_dir()
                        .map_err(|e| miette::miette!("{e}"))?
                        .join(&pos.file),
                )
                .ok_or_else(|| miette::miette!("no project.godot found"))?;

                let count = crate::lsp::query::apply_rename(&result, &project_root)?;
                result.summary = Some(format!(
                    "Applied rename across {} file{}",
                    count,
                    if count == 1 { "" } else { "s" }
                ));
            }

            let json = serde_json::to_string_pretty(&result).map_err(|e| miette::miette!("{e}"))?;
            println!("{json}");
            Ok(())
        }
        LspCommand::References {
            name,
            file,
            line,
            column,
            class,
        } => {
            let result = if let Some(ref name) = name {
                crate::lsp::query::query_references_by_name(
                    name,
                    file.as_deref(),
                    class.as_deref(),
                )?
            } else {
                let file = file
                    .ok_or_else(|| miette::miette!("--file is required when not using --name"))?;
                let line = line
                    .ok_or_else(|| miette::miette!("--line is required when not using --name"))?;
                let column = column
                    .ok_or_else(|| miette::miette!("--column is required when not using --name"))?;
                crate::lsp::query::query_references(&file, line, column)?
            };
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
        LspCommand::Completions { pos, limit } => {
            let mut result = crate::lsp::query::query_completions(&pos.file, pos.line, pos.column)?;
            if let Some(n) = limit {
                result.truncate(n);
            }
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
        LspCommand::Symbols { file, kind } => {
            let mut result = crate::lsp::query::query_symbols(&file)?;
            let kind_filter: Vec<String> = kind
                .iter()
                .flat_map(|s| s.split(',').map(|k| k.trim().to_lowercase().to_string()))
                .collect();
            if !kind_filter.is_empty() {
                result.retain(|s| kind_filter.iter().any(|k| k == &s.kind));
            }
            let json = serde_json::to_string_pretty(&result).map_err(|e| miette::miette!("{e}"))?;
            println!("{json}");
            Ok(())
        }
        LspCommand::DeleteSymbol {
            file,
            name,
            line,
            class,
            force,
            dry_run,
        } => {
            if name.is_none() && line.is_none() {
                return Err(miette::miette!("either --name or --line is required"));
            }
            if name.is_some() && line.is_some() {
                return Err(miette::miette!("--name and --line are mutually exclusive"));
            }
            let result = crate::lsp::query::query_delete_symbol(
                &file,
                name.as_deref(),
                line,
                force,
                dry_run,
                class.as_deref(),
            )?;
            let json = serde_json::to_string_pretty(&result).map_err(|e| miette::miette!("{e}"))?;
            println!("{json}");
            if !force && !result.references.is_empty() {
                std::process::exit(1);
            }
            Ok(())
        }
        LspCommand::MoveSymbol {
            name,
            from,
            to,
            class,
            target_class,
            dry_run,
        } => {
            let result = crate::lsp::query::query_move_symbol(
                &name,
                &from,
                &to,
                dry_run,
                class.as_deref(),
                target_class.as_deref(),
            )?;
            let json = serde_json::to_string_pretty(&result).map_err(|e| miette::miette!("{e}"))?;
            println!("{json}");
            Ok(())
        }
        LspCommand::ExtractMethod {
            file,
            start_line,
            end_line,
            name,
            dry_run,
        } => {
            let result = crate::lsp::query::query_extract_method(
                &file, start_line, end_line, &name, dry_run,
            )?;
            let json = serde_json::to_string_pretty(&result).map_err(|e| miette::miette!("{e}"))?;
            println!("{json}");
            Ok(())
        }
        LspCommand::InlineMethod {
            file,
            name,
            all,
            line,
            column,
            dry_run,
        } => {
            if let Some(ref func_name) = name {
                let result = crate::lsp::query::query_inline_method_by_name(
                    &file, func_name, all, dry_run,
                )?;
                let json =
                    serde_json::to_string_pretty(&result).map_err(|e| miette::miette!("{e}"))?;
                println!("{json}");
            } else {
                let line = line
                    .ok_or_else(|| miette::miette!("--line is required when not using --name"))?;
                let column = column.ok_or_else(|| {
                    miette::miette!("--column is required when not using --name")
                })?;
                let result = crate::lsp::query::query_inline_method(&file, line, column, dry_run)?;
                let json =
                    serde_json::to_string_pretty(&result).map_err(|e| miette::miette!("{e}"))?;
                println!("{json}");
            }
            Ok(())
        }
        LspCommand::ChangeSignature {
            file,
            name,
            add_param,
            remove_param,
            rename_param,
            reorder,
            class,
            dry_run,
        } => {
            let result = crate::lsp::query::query_change_signature(
                &file,
                &name,
                &add_param,
                &remove_param,
                &rename_param,
                reorder.as_deref(),
                class.as_deref(),
                dry_run,
            )?;
            let json = serde_json::to_string_pretty(&result).map_err(|e| miette::miette!("{e}"))?;
            println!("{json}");
            Ok(())
        }
    }
}
