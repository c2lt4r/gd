use std::io::{IsTerminal, Read};

use clap::{Args, Subcommand};
use gd_core::cprintln;
use miette::Result;

#[derive(Args)]
pub struct EditGdArgs {
    #[command(subcommand)]
    pub command: EditGdCommand,
}

#[derive(Subcommand)]
pub enum EditGdCommand {
    /// Create a new GDScript file with boilerplate (or custom content from stdin/--input-file)
    CreateFile {
        /// Path for the new file
        file: String,
        /// Base class to extend (default: "Node"; prepended to --input-file content when non-default)
        #[arg(long, default_value = "Node")]
        extends: String,
        /// Optional class_name declaration (prepended to --input-file content when set)
        #[arg(long)]
        class_name: Option<String>,
        /// Read initial content from a file instead of generating boilerplate
        #[arg(long)]
        input_file: Option<String>,
        /// Overwrite the file if it already exists
        #[arg(long)]
        force: bool,
        /// Output format: json or human (default: human)
        #[arg(long)]
        format: Option<String>,
    },
    /// Replace a function's body (AST-aware, reads new body from stdin or --input-file)
    ReplaceBody {
        /// Path to the GDScript file
        #[arg()]
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
        /// Output format: json or human (default: human)
        #[arg(long)]
        format: Option<String>,
    },
    /// Insert code before or after a named symbol (reads content from stdin or --input-file)
    Insert {
        /// Path to the GDScript file
        #[arg()]
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
        /// Output format: json or human (default: human)
        #[arg(long)]
        format: Option<String>,
    },
    /// Remove a symbol (function, variable, signal, enum, class) from a file
    Remove {
        /// Path to the GDScript file
        #[arg()]
        file: String,
        /// Symbol name to remove (alternative to --line)
        #[arg(long)]
        name: Option<String>,
        /// Line number of declaration to remove (1-based; alternative to --name)
        #[arg(long)]
        line: Option<usize>,
        /// Inner class to operate within
        #[arg(long)]
        class: Option<String>,
        /// Remove even if references exist elsewhere
        #[arg(long)]
        force: bool,
        /// Output format: json or human (default: human)
        #[arg(long)]
        format: Option<String>,
    },
    /// Move a symbol from one file to another
    Extract {
        /// Symbol name to extract
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
    /// Insert code into a class body (reads content from stdin or --input-file)
    InsertInto {
        /// Path to the GDScript file
        #[arg()]
        file: String,
        /// Class name to insert into (inner class or top-level class_name)
        #[arg(long)]
        class: String,
        /// Read content from a file instead of stdin
        #[arg(long)]
        input_file: Option<String>,
        /// Skip auto-formatting the result
        #[arg(long)]
        no_format: bool,
        /// Output format: json or human (default: human)
        #[arg(long)]
        format: Option<String>,
    },
    /// Replace an entire symbol declaration (reads new content from stdin or --input-file)
    ReplaceSymbol {
        /// Path to the GDScript file
        #[arg()]
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

fn print_edit_human(r: &gd_lsp::refactor::EditOutput) {
    use owo_colors::OwoColorize;
    let verb = match r.operation {
        "replace_body" => "Replaced body of",
        "insert_after" => "Inserted after",
        "insert_before" => "Inserted before",
        "replace_symbol" => "Replaced",
        "insert-into" => "Inserted into",
        _ => r.operation,
    };
    let symbol_part = r
        .symbol
        .as_deref()
        .map_or(String::new(), |s| format!(" {}", s.bold()));
    cprintln!(
        "{verb}{symbol_part} in {} ({} line{}){}",
        r.file.cyan(),
        r.lines_changed,
        if r.lines_changed == 1 { "" } else { "s" },
        dry_run_suffix(r.applied),
    );
    for w in &r.warnings {
        cprintln!("  {}: {w}", "warning".yellow());
    }
}

fn print_remove_human(r: &gd_lsp::refactor::DeleteSymbolOutput) {
    use owo_colors::OwoColorize;
    cprintln!(
        "{} {} {} (lines {}-{}){}",
        if r.applied { "Removed" } else { "Would remove" },
        r.kind.dimmed(),
        r.symbol.bold(),
        r.removed_lines.start,
        r.removed_lines.end,
        dry_run_suffix(r.applied),
    );
    if !r.references.is_empty() {
        cprintln!(
            "  {} dangling reference{}:",
            r.references.len().to_string().yellow(),
            if r.references.len() == 1 { "" } else { "s" }
        );
        for loc in &r.references {
            cprintln!("  {}:{}:{}", loc.file.cyan(), loc.line, loc.column);
        }
    }
}

fn print_extract_human(r: &gd_lsp::refactor::MoveSymbolOutput) {
    use owo_colors::OwoColorize;
    cprintln!(
        "{} {} ({}) {} {} {}{}",
        if r.applied {
            "Extracted"
        } else {
            "Would extract"
        },
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

fn print_create_file_human(r: &gd_lsp::query::CreateFileOutput) {
    use owo_colors::OwoColorize;
    let class_part = r
        .class_name
        .as_deref()
        .map_or_else(String::new, |cn: &str| {
            format!(", class_name {}", cn.green())
        });
    cprintln!(
        "Created {} (extends {}{}, {} line{}){}",
        r.file.cyan().bold(),
        r.extends.green(),
        class_part,
        r.lines,
        if r.lines == 1 { "" } else { "s" },
        dry_run_suffix(r.applied),
    );
}

/// Read content from `--input-file` if provided, otherwise from stdin.
/// Uses the ripgrep `is_readable_stdin()` pattern (fstat-based) to avoid
/// blocking when stdin is a terminal, /dev/null, or a closed descriptor.
fn read_content(input_file: Option<&str>) -> Result<String> {
    if let Some(path) = input_file {
        std::fs::read_to_string(path)
            .map_err(|e| miette::miette!("cannot read input file '{}': {}", path, e))
    } else if is_stdin_readable() {
        let mut content = String::new();
        std::io::stdin()
            .read_to_string(&mut content)
            .map_err(|e| miette::miette!("cannot read stdin: {e}"))?;
        Ok(content)
    } else {
        Err(miette::miette!(
            "no input provided — use --input-file <path> or pipe content via stdin"
        ))
    }
}

/// Check if stdin has readable data (pipe, file, or socket).
/// Returns false for terminals, /dev/null (char device), and closed descriptors.
/// Based on ripgrep's `grep_cli::is_readable_stdin()` pattern.
fn is_stdin_readable() -> bool {
    if std::io::stdin().is_terminal() {
        return false;
    }
    is_stdin_pipe_or_file()
}

#[cfg(unix)]
fn is_stdin_pipe_or_file() -> bool {
    use std::os::{fd::AsFd, unix::fs::FileTypeExt};
    let stdin = std::io::stdin();
    let Ok(fd) = stdin.as_fd().try_clone_to_owned() else {
        return false;
    };
    let file = std::fs::File::from(fd);
    let Ok(md) = file.metadata() else {
        return false;
    };
    let ft = md.file_type();
    // Accept pipes (echo "x" | gd edit ...) and file redirects (< file).
    // Exclude sockets — background process managers often attach stdin to a
    // socket with no writer, which would block read_to_string forever.
    ft.is_file() || ft.is_fifo()
}

#[cfg(windows)]
fn is_stdin_pipe_or_file() -> bool {
    use std::os::windows::io::AsRawHandle;
    let handle = std::io::stdin().as_raw_handle() as windows_sys::Win32::Foundation::HANDLE;
    // SAFETY: GetFileType is a well-defined Win32 API; we pass a valid handle.
    let ft = unsafe { windows_sys::Win32::Storage::FileSystem::GetFileType(handle) };
    ft == windows_sys::Win32::Storage::FileSystem::FILE_TYPE_DISK
        || ft == windows_sys::Win32::Storage::FileSystem::FILE_TYPE_PIPE
}

#[cfg(not(any(unix, windows)))]
fn is_stdin_pipe_or_file() -> bool {
    true // Best-effort: assume readable on unknown platforms
}

#[allow(clippy::too_many_lines)]
pub fn exec(args: EditGdArgs) -> Result<()> {
    match args.command {
        EditGdCommand::CreateFile {
            file,
            extends,
            class_name,
            input_file,
            force,
            format,
        } => {
            // Read custom content from --input-file or stdin (if piped).
            // Falls back to generating boilerplate when neither is provided.
            let custom_content = if input_file.is_some() || is_stdin_readable() {
                Some(read_content(input_file.as_deref())?)
            } else {
                None
            };
            let result = gd_lsp::query::query_create_file(
                &file,
                &extends,
                class_name.as_deref(),
                custom_content.as_deref(),
                force,
            )?;
            if is_json(format.as_ref()) {
                let json =
                    serde_json::to_string_pretty(&result).map_err(|e| miette::miette!("{e}"))?;
                cprintln!("{json}");
            } else {
                print_create_file_human(&result);
            }
            Ok(())
        }
        EditGdCommand::ReplaceBody {
            file,
            name,
            class,
            input_file,
            no_format,
            format,
        } => {
            let content = read_content(input_file.as_deref())?;
            let result = gd_lsp::query::query_replace_body(
                &file,
                &name,
                class.as_deref(),
                &content,
                no_format,
            )?;
            if is_json(format.as_ref()) {
                let json =
                    serde_json::to_string_pretty(&result).map_err(|e| miette::miette!("{e}"))?;
                cprintln!("{json}");
            } else {
                print_edit_human(&result);
            }
            Ok(())
        }
        EditGdCommand::Insert {
            file,
            after,
            before,
            class,
            input_file,
            no_format,
            format,
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
            let result = gd_lsp::query::query_insert(
                &file,
                &anchor,
                is_after,
                class.as_deref(),
                &content,
                no_format,
            )?;
            if is_json(format.as_ref()) {
                let json =
                    serde_json::to_string_pretty(&result).map_err(|e| miette::miette!("{e}"))?;
                cprintln!("{json}");
            } else {
                print_edit_human(&result);
            }
            Ok(())
        }
        EditGdCommand::ReplaceSymbol {
            file,
            name,
            class,
            input_file,
            no_format,
            format,
        } => {
            let content = read_content(input_file.as_deref())?;
            let result = gd_lsp::query::query_replace_symbol(
                &file,
                &name,
                class.as_deref(),
                &content,
                no_format,
            )?;
            if is_json(format.as_ref()) {
                let json =
                    serde_json::to_string_pretty(&result).map_err(|e| miette::miette!("{e}"))?;
                cprintln!("{json}");
            } else {
                print_edit_human(&result);
            }
            Ok(())
        }
        EditGdCommand::Remove {
            file,
            name,
            line,
            class,
            force,
            format,
        } => {
            if name.is_none() && line.is_none() {
                return Err(miette::miette!("either --name or --line is required"));
            }
            if name.is_some() && line.is_some() {
                return Err(miette::miette!("--name and --line are mutually exclusive"));
            }
            let result =
                gd_lsp::query::query_remove(&file, name.as_deref(), line, force, class.as_deref())?;
            if is_json(format.as_ref()) {
                let json =
                    serde_json::to_string_pretty(&result).map_err(|e| miette::miette!("{e}"))?;
                cprintln!("{json}");
            } else {
                print_remove_human(&result);
            }
            if !force && !result.references.is_empty() {
                std::process::exit(1);
            }
            Ok(())
        }
        EditGdCommand::Extract {
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
                print_extract_human(&result);
            }
            Ok(())
        }
        EditGdCommand::InsertInto {
            file,
            class,
            input_file,
            no_format,
            format,
        } => {
            let content = read_content(input_file.as_deref())?;
            let result = gd_lsp::query::query_insert_into(&file, &class, &content, no_format)?;
            if is_json(format.as_ref()) {
                let json =
                    serde_json::to_string_pretty(&result).map_err(|e| miette::miette!("{e}"))?;
                cprintln!("{json}");
            } else {
                print_edit_human(&result);
            }
            Ok(())
        }
    }
}
