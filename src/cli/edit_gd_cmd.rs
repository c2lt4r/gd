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
        /// Preview without writing
        #[arg(long)]
        dry_run: bool,
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
        /// Preview without writing changes
        #[arg(long)]
        dry_run: bool,
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
        /// Preview without writing changes
        #[arg(long)]
        dry_run: bool,
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
        /// Preview without writing changes
        #[arg(long)]
        dry_run: bool,
        /// Output format: json or human (default: human)
        #[arg(long)]
        format: Option<String>,
    },
    /// Replace a range of lines (reads new content from stdin or --input-file)
    EditRange {
        /// Path to the GDScript file
        #[arg()]
        file: String,
        /// Line range as START-END (e.g. 5-20; 1-based, inclusive)
        #[arg(long, conflicts_with_all = ["start_line", "end_line"])]
        range: Option<String>,
        /// First line to replace (1-based, inclusive)
        #[arg(long)]
        start_line: Option<usize>,
        /// Last line to replace (1-based, inclusive)
        #[arg(long)]
        end_line: Option<usize>,
        /// Read content from a file instead of stdin
        #[arg(long)]
        input_file: Option<String>,
        /// Skip auto-formatting the result
        #[arg(long)]
        no_format: bool,
        /// Preview without writing changes
        #[arg(long)]
        dry_run: bool,
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
        "edit_range" => "Replaced range in",
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

/// Parse a range string like "5-20" into (start, end) line numbers.
fn parse_range(range: &str) -> Result<(usize, usize)> {
    let parts: Vec<&str> = range.splitn(2, '-').collect();
    if parts.len() != 2 {
        return Err(miette::miette!(
            "invalid range '{range}' — expected START-END (e.g. 5-20)"
        ));
    }
    let start: usize = parts[0]
        .parse()
        .map_err(|_| miette::miette!("invalid start line in range: '{}'", parts[0]))?;
    let end: usize = parts[1]
        .parse()
        .map_err(|_| miette::miette!("invalid end line in range: '{}'", parts[1]))?;
    if start == 0 || end == 0 {
        return Err(miette::miette!("line numbers are 1-based"));
    }
    if start > end {
        return Err(miette::miette!(
            "start ({start}) must be <= end ({end}) in range"
        ));
    }
    Ok((start, end))
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
            dry_run,
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
                dry_run,
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
            dry_run,
            format,
        } => {
            let content = read_content(input_file.as_deref())?;
            let result = gd_lsp::query::query_replace_body(
                &file,
                &name,
                class.as_deref(),
                &content,
                no_format,
                dry_run,
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
            dry_run,
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
                dry_run,
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
            dry_run,
            format,
        } => {
            let content = read_content(input_file.as_deref())?;
            let result = gd_lsp::query::query_replace_symbol(
                &file,
                &name,
                class.as_deref(),
                &content,
                no_format,
                dry_run,
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
        EditGdCommand::EditRange {
            file,
            range,
            start_line,
            end_line,
            input_file,
            no_format,
            dry_run,
            format,
        } => {
            let (start, end) = if let Some(ref r) = range {
                parse_range(r)?
            } else {
                let s = start_line
                    .ok_or_else(|| miette::miette!("--start-line or --range is required"))?;
                let e =
                    end_line.ok_or_else(|| miette::miette!("--end-line or --range is required"))?;
                (s, e)
            };
            let content = read_content(input_file.as_deref())?;
            let result =
                gd_lsp::query::query_edit_range(&file, start, end, &content, no_format, dry_run)?;
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
