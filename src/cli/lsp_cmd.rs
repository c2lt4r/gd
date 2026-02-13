use std::io::Read;

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
        /// Filter by symbol kind (repeatable, comma-separated: function, method, variable, class, constant, enum, event; aliases: field/property = variable+field)
        #[arg(long)]
        kind: Vec<String>,
    },
    /// View lines from a GDScript file (with optional line range)
    View {
        /// Path to the GDScript file
        #[arg(long)]
        file: String,
        /// First line to show (1-based, inclusive; default: 1)
        #[arg(long)]
        start_line: Option<usize>,
        /// Last line to show (1-based, inclusive; default: end of file)
        #[arg(long)]
        end_line: Option<usize>,
        /// Number of context lines around start_line/end_line
        #[arg(long)]
        context: Option<usize>,
        /// Output format: json for structured output (default: human-readable)
        #[arg(long)]
        format: Option<String>,
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
        /// Update preload/load paths in files that reference the source
        #[arg(long)]
        update_callers: bool,
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
    /// Check if a file can be safely deleted (find all cross-file references)
    SafeDeleteFile {
        /// Path to the GDScript file
        #[arg(long)]
        file: String,
        /// Delete even if references exist
        #[arg(long)]
        force: bool,
        /// Preview without deleting (default when --force is not set)
        #[arg(long)]
        dry_run: bool,
    },
    /// Find all classes that implement (define) a given method
    FindImplementations {
        /// Method name to search for
        #[arg(long)]
        name: String,
        /// Only include classes extending this type
        #[arg(long)]
        base: Option<String>,
    },
    /// Extract an expression into a local variable
    IntroduceVariable {
        /// Path to the GDScript file
        #[arg(long)]
        file: String,
        /// Line number of the expression (1-based)
        #[arg(long)]
        line: usize,
        /// Start column of the expression (1-based)
        #[arg(long)]
        column: usize,
        /// End column of the expression (1-based)
        #[arg(long)]
        end_column: usize,
        /// Name for the new variable
        #[arg(long)]
        name: String,
        /// Preview without writing changes
        #[arg(long)]
        dry_run: bool,
    },
    /// Turn an expression into a function parameter with a default value
    IntroduceParameter {
        /// Path to the GDScript file
        #[arg(long)]
        file: String,
        /// Line number of the expression (1-based)
        #[arg(long)]
        line: usize,
        /// Start column of the expression (1-based)
        #[arg(long)]
        column: usize,
        /// End column of the expression (1-based)
        #[arg(long)]
        end_column: usize,
        /// Name for the new parameter
        #[arg(long)]
        name: String,
        /// Type hint for the parameter (e.g., "float", "String")
        #[arg(long, rename_all = "snake_case")]
        r#type: Option<String>,
        /// Preview without writing changes
        #[arg(long)]
        dry_run: bool,
    },
    /// Delete multiple symbols in one pass without line-shifting issues
    BulkDeleteSymbol {
        /// Path to the GDScript file
        #[arg(long)]
        file: String,
        /// Comma-separated symbol names to delete
        #[arg(long)]
        names: String,
        /// Delete even if references exist
        #[arg(long)]
        force: bool,
        /// Preview without writing changes
        #[arg(long)]
        dry_run: bool,
    },
    /// Rename multiple symbols atomically
    BulkRename {
        /// Path to the GDScript file
        #[arg(long)]
        file: String,
        /// Comma-separated rename pairs (format: "old1:new1,old2:new2")
        #[arg(long)]
        renames: String,
        /// Preview without writing changes
        #[arg(long)]
        dry_run: bool,
    },
    /// Inline a pure pass-through delegate function
    InlineDelegate {
        /// Path to the GDScript file
        #[arg(long)]
        file: String,
        /// Name of the delegate function
        #[arg(long)]
        name: String,
        /// Preview without writing changes
        #[arg(long)]
        dry_run: bool,
    },
    /// Extract multiple symbols into a new class file
    ExtractClass {
        /// Path to the source GDScript file
        #[arg(long)]
        file: String,
        /// Comma-separated symbol names to extract
        #[arg(long)]
        symbols: String,
        /// Destination file path
        #[arg(long)]
        to: String,
        /// Preview without writing changes
        #[arg(long)]
        dry_run: bool,
    },
    /// Create a new GDScript file with boilerplate
    CreateFile {
        /// Path for the new file
        #[arg(long)]
        file: String,
        /// Base class to extend (default: "Node")
        #[arg(long, default_value = "Node")]
        extends: String,
        /// Optional class_name declaration
        #[arg(long)]
        class_name: Option<String>,
        /// Preview without writing
        #[arg(long)]
        dry_run: bool,
    },
    /// Replace a function's body (AST-aware, reads new body from stdin or --input-file)
    ReplaceBody {
        /// Path to the GDScript file
        #[arg(long)]
        file: String,
        /// Function name whose body to replace
        #[arg(long)]
        name: String,
        /// Inner class containing the function
        #[arg(long)]
        class: Option<String>,
        /// Read content from a file instead of stdin
        #[arg(long)]
        input_file: Option<String>,
        /// Skip auto-formatting the result
        #[arg(long)]
        no_format: bool,
        /// Preview without writing changes
        #[arg(long)]
        dry_run: bool,
    },
    /// Insert code before or after a named symbol (reads content from stdin or --input-file)
    Insert {
        /// Path to the GDScript file
        #[arg(long)]
        file: String,
        /// Insert after this symbol
        #[arg(long, conflicts_with = "before")]
        after: Option<String>,
        /// Insert before this symbol
        #[arg(long, conflicts_with = "after")]
        before: Option<String>,
        /// Inner class containing the anchor symbol
        #[arg(long)]
        class: Option<String>,
        /// Read content from a file instead of stdin
        #[arg(long)]
        input_file: Option<String>,
        /// Skip auto-formatting the result
        #[arg(long)]
        no_format: bool,
        /// Preview without writing changes
        #[arg(long)]
        dry_run: bool,
    },
    /// Replace an entire symbol declaration (reads new content from stdin or --input-file)
    ReplaceSymbol {
        /// Path to the GDScript file
        #[arg(long)]
        file: String,
        /// Symbol name to replace
        #[arg(long)]
        name: String,
        /// Inner class containing the symbol
        #[arg(long)]
        class: Option<String>,
        /// Read content from a file instead of stdin
        #[arg(long)]
        input_file: Option<String>,
        /// Skip auto-formatting the result
        #[arg(long)]
        no_format: bool,
        /// Preview without writing changes
        #[arg(long)]
        dry_run: bool,
    },
    /// Replace a range of lines (reads new content from stdin or --input-file)
    EditRange {
        /// Path to the GDScript file
        #[arg(long)]
        file: String,
        /// First line to replace (1-based, inclusive)
        #[arg(long)]
        start_line: usize,
        /// Last line to replace (1-based, inclusive)
        #[arg(long)]
        end_line: usize,
        /// Read content from a file instead of stdin
        #[arg(long)]
        input_file: Option<String>,
        /// Skip auto-formatting the result
        #[arg(long)]
        no_format: bool,
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

/// Read content from `--input-file` if provided, otherwise from stdin.
fn read_content(input_file: Option<&str>) -> Result<String> {
    if let Some(path) = input_file {
        std::fs::read_to_string(path)
            .map_err(|e| miette::miette!("cannot read input file '{}': {}", path, e))
    } else {
        let mut content = String::new();
        std::io::stdin()
            .read_to_string(&mut content)
            .map_err(|e| miette::miette!("cannot read stdin: {e}"))?;
        Ok(content)
    }
}

pub fn exec(args: LspArgs) -> Result<()> {
    let Some(command) = args.command else {
        // No subcommand — start the LSP server (backward compatible)
        crate::lsp::run_server();
        return Ok(());
    };

    match command {
        LspCommand::Rename {
            name,
            file,
            line,
            column,
            new_name,
            dry_run,
        } => {
            let mut result = if let Some(ref sym_name) = name {
                crate::lsp::query::query_rename_by_name(sym_name, &new_name, file.as_deref())?
            } else {
                let file_str = file
                    .as_deref()
                    .ok_or_else(|| miette::miette!("--file is required when not using --name"))?;
                let line = line
                    .ok_or_else(|| miette::miette!("--line is required when not using --name"))?;
                let column = column
                    .ok_or_else(|| miette::miette!("--column is required when not using --name"))?;
                crate::lsp::query::query_rename(file_str, line, column, &new_name)?
            };

            if !dry_run {
                let anchor = if let Some(ref f) = file {
                    std::env::current_dir()
                        .map_err(|e| miette::miette!("{e}"))?
                        .join(f)
                } else {
                    std::env::current_dir().map_err(|e| miette::miette!("{e}"))?
                };
                let project_root = crate::core::config::find_project_root(&anchor)
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
                // "field" and "property" are aliases for "variable" + "field"
                .flat_map(|k| match k.as_str() {
                    "field" | "property" => vec!["variable".to_string(), "field".to_string()],
                    other => vec![other.to_string()],
                })
                .collect();
            if !kind_filter.is_empty() {
                result.retain(|s| kind_filter.iter().any(|k| k == &s.kind));
            }
            let json = serde_json::to_string_pretty(&result).map_err(|e| miette::miette!("{e}"))?;
            println!("{json}");
            Ok(())
        }
        LspCommand::View {
            file,
            start_line,
            end_line,
            context,
            format,
        } => {
            let result = crate::lsp::query::query_view(&file, start_line, end_line, context)?;
            if format.as_deref() == Some("json") {
                let json =
                    serde_json::to_string_pretty(&result).map_err(|e| miette::miette!("{e}"))?;
                println!("{json}");
            } else {
                // Human-readable output (cat -n style)
                let width = if result.end_line > 0 {
                    result.end_line.to_string().len()
                } else {
                    1
                };
                for vl in &result.lines {
                    println!("{:>width$}\t{}", vl.line, vl.content, width = width);
                }
            }
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
            update_callers,
            dry_run,
        } => {
            let result = crate::lsp::query::query_move_symbol(
                &name,
                &from,
                &to,
                dry_run,
                class.as_deref(),
                target_class.as_deref(),
                update_callers,
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
                let result =
                    crate::lsp::query::query_inline_method_by_name(&file, func_name, all, dry_run)?;
                let json =
                    serde_json::to_string_pretty(&result).map_err(|e| miette::miette!("{e}"))?;
                println!("{json}");
            } else {
                let line = line
                    .ok_or_else(|| miette::miette!("--line is required when not using --name"))?;
                let column = column
                    .ok_or_else(|| miette::miette!("--column is required when not using --name"))?;
                let result = crate::lsp::query::query_inline_method(&file, line, column, dry_run)?;
                let json =
                    serde_json::to_string_pretty(&result).map_err(|e| miette::miette!("{e}"))?;
                println!("{json}");
            }
            Ok(())
        }
        LspCommand::SafeDeleteFile {
            file,
            force,
            dry_run,
        } => {
            let result = crate::lsp::query::query_safe_delete_file(&file, force, dry_run)?;
            let json = serde_json::to_string_pretty(&result).map_err(|e| miette::miette!("{e}"))?;
            println!("{json}");
            if !force && !result.references.is_empty() {
                std::process::exit(1);
            }
            Ok(())
        }
        LspCommand::FindImplementations { name, base } => {
            let result = crate::lsp::query::query_find_implementations(&name, base.as_deref())?;
            let json = serde_json::to_string_pretty(&result).map_err(|e| miette::miette!("{e}"))?;
            println!("{json}");
            Ok(())
        }
        LspCommand::IntroduceVariable {
            file,
            line,
            column,
            end_column,
            name,
            dry_run,
        } => {
            let result = crate::lsp::query::query_introduce_variable(
                &file, line, column, end_column, &name, dry_run,
            )?;
            let json = serde_json::to_string_pretty(&result).map_err(|e| miette::miette!("{e}"))?;
            println!("{json}");
            Ok(())
        }
        LspCommand::IntroduceParameter {
            file,
            line,
            column,
            end_column,
            name,
            r#type,
            dry_run,
        } => {
            let result = crate::lsp::query::query_introduce_parameter(
                &file,
                line,
                column,
                end_column,
                &name,
                r#type.as_deref(),
                dry_run,
            )?;
            let json = serde_json::to_string_pretty(&result).map_err(|e| miette::miette!("{e}"))?;
            println!("{json}");
            Ok(())
        }
        LspCommand::CreateFile {
            file,
            extends,
            class_name,
            dry_run,
        } => {
            let result = crate::lsp::query::query_create_file(
                &file,
                &extends,
                class_name.as_deref(),
                dry_run,
            )?;
            let json = serde_json::to_string_pretty(&result).map_err(|e| miette::miette!("{e}"))?;
            println!("{json}");
            Ok(())
        }
        LspCommand::ReplaceBody {
            file,
            name,
            class,
            input_file,
            no_format,
            dry_run,
        } => {
            let content = read_content(input_file.as_deref())?;
            let result = crate::lsp::query::query_replace_body(
                &file,
                &name,
                class.as_deref(),
                &content,
                no_format,
                dry_run,
            )?;
            let json = serde_json::to_string_pretty(&result).map_err(|e| miette::miette!("{e}"))?;
            println!("{json}");
            Ok(())
        }
        LspCommand::Insert {
            file,
            after,
            before,
            class,
            input_file,
            no_format,
            dry_run,
        } => {
            let (anchor, is_after) = match (after, before) {
                (Some(a), None) => (a, true),
                (None, Some(b)) => (b, false),
                _ => {
                    return Err(miette::miette!(
                        "exactly one of --after or --before is required"
                    ));
                }
            };
            let content = read_content(input_file.as_deref())?;
            let result = crate::lsp::query::query_insert(
                &file,
                &anchor,
                is_after,
                class.as_deref(),
                &content,
                no_format,
                dry_run,
            )?;
            let json = serde_json::to_string_pretty(&result).map_err(|e| miette::miette!("{e}"))?;
            println!("{json}");
            Ok(())
        }
        LspCommand::ReplaceSymbol {
            file,
            name,
            class,
            input_file,
            no_format,
            dry_run,
        } => {
            let content = read_content(input_file.as_deref())?;
            let result = crate::lsp::query::query_replace_symbol(
                &file,
                &name,
                class.as_deref(),
                &content,
                no_format,
                dry_run,
            )?;
            let json = serde_json::to_string_pretty(&result).map_err(|e| miette::miette!("{e}"))?;
            println!("{json}");
            Ok(())
        }
        LspCommand::EditRange {
            file,
            start_line,
            end_line,
            input_file,
            no_format,
            dry_run,
        } => {
            let content = read_content(input_file.as_deref())?;
            let result = crate::lsp::query::query_edit_range(
                &file, start_line, end_line, &content, no_format, dry_run,
            )?;
            let json = serde_json::to_string_pretty(&result).map_err(|e| miette::miette!("{e}"))?;
            println!("{json}");
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
        LspCommand::BulkDeleteSymbol {
            file,
            names,
            force,
            dry_run,
        } => {
            let result =
                crate::lsp::query::query_bulk_delete_symbol(&file, &names, force, dry_run)?;
            let json = serde_json::to_string_pretty(&result).map_err(|e| miette::miette!("{e}"))?;
            println!("{json}");
            Ok(())
        }
        LspCommand::BulkRename {
            file,
            renames,
            dry_run,
        } => {
            let result = crate::lsp::query::query_bulk_rename(&file, &renames, dry_run)?;
            let json = serde_json::to_string_pretty(&result).map_err(|e| miette::miette!("{e}"))?;
            println!("{json}");
            Ok(())
        }
        LspCommand::InlineDelegate {
            file,
            name,
            dry_run,
        } => {
            let result = crate::lsp::query::query_inline_delegate(&file, &name, dry_run)?;
            let json = serde_json::to_string_pretty(&result).map_err(|e| miette::miette!("{e}"))?;
            println!("{json}");
            Ok(())
        }
        LspCommand::ExtractClass {
            file,
            symbols,
            to,
            dry_run,
        } => {
            let result = crate::lsp::query::query_extract_class(&file, &symbols, &to, dry_run)?;
            let json = serde_json::to_string_pretty(&result).map_err(|e| miette::miette!("{e}"))?;
            println!("{json}");
            Ok(())
        }
    }
}
