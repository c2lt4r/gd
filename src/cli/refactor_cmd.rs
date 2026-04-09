use clap::{Args, Subcommand};
use gd_core::cprintln;
use miette::Result;

#[derive(Args)]
pub struct RefactorArgs {
    #[command(subcommand)]
    pub command: RefactorCommand,
}

#[derive(Subcommand)]
pub enum RefactorCommand {
    /// Rename a symbol across the project (dry-run by default, --apply to persist)
    Rename {
        /// Search by symbol name across the project (alternative to <FILE>/--line/--column)
        #[arg(long)]
        name: Option<String>,
        /// Path to the GDScript file
        #[arg()]
        file: Option<String>,
        /// Line number (1-based)
        #[arg(long)]
        line: Option<usize>,
        /// Column number (1-based)
        #[arg(long)]
        column: Option<usize>,
        /// New name for the symbol
        #[arg(long)]
        new_name: String,
        /// Actually apply the rename (default is dry-run/preview)
        #[arg(long)]
        apply: bool,
        /// Output format: json or human (default: human)
        #[arg(long)]
        format: Option<String>,
    },
    /// Extract a range of lines into a new function
    ExtractMethod {
        /// Path to the GDScript file
        #[arg()]
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
        /// Output format: json or human (default: human)
        #[arg(long)]
        format: Option<String>,
    },
    /// Move/rename a file and update all references (preload, load, ext_resource, autoload)
    MoveFile {
        /// Source file path (relative to project root)
        #[arg(long)]
        from: String,
        /// Destination file path (relative to project root)
        #[arg(long)]
        to: String,
        /// Output format: json or human (default: human)
        #[arg(long)]
        format: Option<String>,
    },
    /// Change a function's signature and update all call sites
    ChangeSignature {
        /// Path to the GDScript file
        #[arg()]
        file: String,
        /// Function name (alternative to --line)
        #[arg(long)]
        name: Option<String>,
        /// Line number of the function (1-based; alternative to --name)
        #[arg(long)]
        line: Option<usize>,
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
        /// Output format: json or human (default: human)
        #[arg(long)]
        format: Option<String>,
    },
    /// Move a symbol from one file to another
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
        /// Update preload/load paths in files that reference the source
        #[arg(long)]
        update_callers: bool,
        /// Output format: json or human (default: human)
        #[arg(long)]
        format: Option<String>,
    },
}

fn is_json(format: Option<&String>) -> bool {
    format.map(String::as_str) == Some("json")
}

fn dry_run_suffix(applied: bool) -> &'static str {
    if applied { "" } else { " (dry run)" }
}

fn print_rename_human(r: &gd_lsp::query::RenameOutput) {
    use owo_colors::OwoColorize;
    if let Some(ref summary) = r.summary {
        let file_count = r.changes.len();
        let edit_count: usize = r.changes.iter().map(|c| c.edits.len()).sum();
        cprintln!(
            "{} {} {} ({} edit{} in {} file{})",
            r.symbol.bold(),
            "→".dimmed(),
            r.new_name.green().bold(),
            edit_count,
            if edit_count == 1 { "" } else { "s" },
            file_count,
            if file_count == 1 { "" } else { "s" },
        );
        cprintln!("  {summary}");
    } else {
        let file_count = r.changes.len();
        let edit_count: usize = r.changes.iter().map(|c| c.edits.len()).sum();
        cprintln!(
            "{} {} {} ({} edit{} in {} file{}) (dry run)",
            r.symbol.bold(),
            "→".dimmed(),
            r.new_name.green().bold(),
            edit_count,
            if edit_count == 1 { "" } else { "s" },
            file_count,
            if file_count == 1 { "" } else { "s" },
        );
        for change in &r.changes {
            for edit in &change.edits {
                cprintln!("  {}:{}:{}", change.file.cyan(), edit.line, edit.column,);
            }
        }
    }
    for w in &r.warnings {
        cprintln!("  {} {w}", "warning:".yellow().bold());
    }
}

fn print_extract_method_human(r: &gd_lsp::refactor::ExtractMethodOutput) {
    use owo_colors::OwoColorize;
    cprintln!(
        "Extracted into {}() in {}{}",
        r.function.green().bold(),
        r.file.cyan(),
        dry_run_suffix(r.applied),
    );
    for w in &r.warnings {
        cprintln!("  {}: {w}", "warning".yellow());
    }
}

fn print_move_file_human(r: &gd_lsp::refactor::MoveFileOutput) {
    use owo_colors::OwoColorize;
    let total = r.updated_scripts.len() + r.updated_resources.len();
    cprintln!(
        "{} {} {} {} ({} reference{}){}",
        if r.applied { "Moved" } else { "Would move" },
        r.from.cyan(),
        "→".dimmed(),
        r.to.cyan(),
        total,
        if total == 1 { "" } else { "s" },
        dry_run_suffix(r.applied),
    );
    for u in &r.updated_scripts {
        cprintln!("  {}:{}", u.file.cyan(), u.line);
    }
    for u in &r.updated_resources {
        cprintln!("  {}:{}", u.file.cyan(), u.line);
    }
    if let Some(ref name) = r.updated_autoload {
        cprintln!("  autoload {} updated", name.green());
    }
    for w in &r.warnings {
        cprintln!("  {}: {w}", "warning".yellow());
    }
}

fn print_change_signature_human(r: &gd_lsp::refactor::ChangeSignatureOutput) {
    use owo_colors::OwoColorize;
    cprintln!(
        "{} {} {} ({} call site{}){}",
        r.old_signature.dimmed(),
        "→".dimmed(),
        r.new_signature.green().bold(),
        r.call_sites_updated,
        if r.call_sites_updated == 1 { "" } else { "s" },
        dry_run_suffix(r.applied),
    );
    for w in &r.warnings {
        cprintln!("  {}: {w}", "warning".yellow());
    }
}

fn print_move_symbol_human(r: &gd_lsp::refactor::MoveSymbolOutput) {
    use owo_colors::OwoColorize;
    cprintln!(
        "{} {} ({}) {} {} {}{}",
        if r.applied { "Moved" } else { "Would move" },
        r.symbol.bold(),
        r.kind.dimmed(),
        r.from.cyan(),
        "→".dimmed(),
        r.to.cyan(),
        dry_run_suffix(r.applied),
    );
    for w in &r.warnings {
        cprintln!("  {}: {w}", "warning".yellow());
    }
}

#[allow(clippy::too_many_lines)]
pub fn exec(args: RefactorArgs) -> Result<()> {
    match args.command {
        RefactorCommand::Rename {
            name,
            file,
            line,
            column,
            new_name,
            apply,
            format,
        } => {
            let mut result = if let Some(ref sym_name) = name {
                gd_lsp::query::query_rename_by_name(sym_name, &new_name, file.as_deref())?
            } else {
                let file_str = file
                    .as_deref()
                    .ok_or_else(|| miette::miette!("<FILE> is required when not using --name"))?;
                let line = line
                    .ok_or_else(|| miette::miette!("--line is required when not using --name"))?;
                let column = column
                    .ok_or_else(|| miette::miette!("--column is required when not using --name"))?;
                gd_lsp::query::query_rename(file_str, line, column, &new_name)?
            };

            if apply {
                let anchor = if let Some(ref f) = file {
                    std::env::current_dir()
                        .map_err(|e| miette::miette!("{e}"))?
                        .join(f)
                } else {
                    std::env::current_dir().map_err(|e| miette::miette!("{e}"))?
                };
                let project_root = gd_core::config::find_project_root(&anchor)
                    .ok_or_else(|| miette::miette!("no project.godot found"))?;

                let count = gd_lsp::query::apply_rename(&result, &project_root)?;

                result.summary = Some(format!(
                    "Applied rename across {} file{}",
                    count,
                    if count == 1 { "" } else { "s" }
                ));
            }

            if is_json(format.as_ref()) {
                let json =
                    serde_json::to_string_pretty(&result).map_err(|e| miette::miette!("{e}"))?;
                cprintln!("{json}");
            } else {
                print_rename_human(&result);
            }
            Ok(())
        }
        RefactorCommand::ExtractMethod {
            file,
            start_line,
            end_line,
            name,
            format,
        } => {
            let result = gd_lsp::query::query_extract_method(&file, start_line, end_line, &name)?;
            if is_json(format.as_ref()) {
                let json =
                    serde_json::to_string_pretty(&result).map_err(|e| miette::miette!("{e}"))?;
                cprintln!("{json}");
            } else {
                print_extract_method_human(&result);
            }
            Ok(())
        }
        RefactorCommand::MoveFile { from, to, format } => {
            let result = gd_lsp::query::query_move_file(&from, &to)?;
            if is_json(format.as_ref()) {
                let json =
                    serde_json::to_string_pretty(&result).map_err(|e| miette::miette!("{e}"))?;
                cprintln!("{json}");
            } else {
                print_move_file_human(&result);
            }
            Ok(())
        }
        RefactorCommand::ChangeSignature {
            file,
            name,
            line,
            add_param,
            remove_param,
            rename_param,
            reorder,
            class,
            format,
        } => {
            if name.is_some() && line.is_some() {
                return Err(miette::miette!("--name and --line are mutually exclusive"));
            }
            if name.is_none() && line.is_none() {
                return Err(miette::miette!("either --name or --line is required"));
            }
            let resolved_name = if let Some(ref n) = name {
                n.clone()
            } else {
                let source = std::fs::read_to_string(&file)
                    .map_err(|e| miette::miette!("cannot read {file}: {e}"))?;
                gd_lsp::refactor::resolve_line_to_name(&source, line.unwrap(), class.as_deref())?
            };
            let result = gd_lsp::query::query_change_signature(
                &file,
                &resolved_name,
                &add_param,
                &remove_param,
                &rename_param,
                reorder.as_deref(),
                class.as_deref(),
            )?;
            if is_json(format.as_ref()) {
                let json =
                    serde_json::to_string_pretty(&result).map_err(|e| miette::miette!("{e}"))?;
                cprintln!("{json}");
            } else {
                print_change_signature_human(&result);
            }
            Ok(())
        }
        RefactorCommand::MoveSymbol {
            name,
            from,
            to,
            class,
            target_class,
            update_callers,
            format,
        } => {
            let result = gd_lsp::query::query_extract(
                &name,
                &from,
                &to,
                class.as_deref(),
                target_class.as_deref(),
                update_callers,
            )?;
            if is_json(format.as_ref()) {
                let json =
                    serde_json::to_string_pretty(&result).map_err(|e| miette::miette!("{e}"))?;
                cprintln!("{json}");
            } else {
                print_move_symbol_human(&result);
            }
            Ok(())
        }
    }
}
