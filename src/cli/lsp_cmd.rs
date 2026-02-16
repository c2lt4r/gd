use std::io::{IsTerminal, Read};

use clap::{Args, Subcommand};
use miette::Result;

#[derive(Args)]
pub struct LspArgs {
    #[command(subcommand)]
    pub command: Option<LspCommand>,
    /// Port for Godot's built-in LSP server (default: 6005)
    #[arg(long, default_value = "6005")]
    pub godot_port: u16,
    /// Disable proxy to Godot's built-in LSP server
    #[arg(long)]
    pub no_godot_proxy: bool,
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
        /// Output format: json or human (default: human)
        #[arg(long)]
        format: Option<String>,
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
        /// Output format: json or human (default: human)
        #[arg(long, default_value = "human")]
        format: String,
    },
    /// Go to the definition of a symbol
    Definition {
        #[command(flatten)]
        pos: QueryPositionArgs,
        /// Output format: json or human (default: human)
        #[arg(long, default_value = "human")]
        format: String,
    },
    /// Show hover information for a symbol
    Hover {
        #[command(flatten)]
        pos: QueryPositionArgs,
        /// Output format: json or human (default: human)
        #[arg(long, default_value = "human")]
        format: String,
    },
    /// List completions at a position
    Completions {
        #[command(flatten)]
        pos: QueryPositionArgs,
        /// Maximum number of results to return
        #[arg(long)]
        limit: Option<usize>,
        /// Filter by kind (e.g. function, method, variable, property, constant, class, enum, enum_member, event, keyword)
        #[arg(long)]
        kind: Option<String>,
        /// Output format: json or human (default: human)
        #[arg(long, default_value = "human")]
        format: String,
    },
    /// List available code actions at a position
    CodeActions {
        #[command(flatten)]
        pos: QueryPositionArgs,
        /// Output format: json or human (default: human)
        #[arg(long)]
        format: Option<String>,
    },
    /// Run diagnostics on files (same as gd lint)
    Diagnostics {
        /// Files or directories to check (defaults to current directory)
        paths: Vec<String>,
        /// Output format: json or human (default: human)
        #[arg(long)]
        format: Option<String>,
    },
    /// List symbols in a file
    Symbols {
        /// Path to the GDScript file
        #[arg(long)]
        file: String,
        /// Filter by symbol kind (repeatable, comma-separated: function, method, variable, class, constant, enum, event; aliases: field/property = variable+field)
        #[arg(long)]
        kind: Vec<String>,
        /// Output format: json or human (default: human)
        #[arg(long, default_value = "human")]
        format: String,
    },
    /// View lines from a GDScript file (with optional line range)
    View {
        /// Path to the GDScript file
        #[arg(long)]
        file: String,
        /// Line range as START-END (e.g. 5-20; 1-based, inclusive)
        #[arg(long, conflicts_with_all = ["start_line", "end_line"])]
        range: Option<String>,
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
        /// Output format: json or human (default: human)
        #[arg(long)]
        format: Option<String>,
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
        /// Output format: json or human (default: human)
        #[arg(long)]
        format: Option<String>,
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
        /// Output format: json or human (default: human)
        #[arg(long)]
        format: Option<String>,
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
        /// Output format: json or human (default: human)
        #[arg(long)]
        format: Option<String>,
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
        /// Output format: json or human (default: human)
        #[arg(long)]
        format: Option<String>,
    },
    /// Find all classes that implement (define) a given method
    FindImplementations {
        /// Method name to search for
        #[arg(long)]
        name: String,
        /// Only include classes extending this type
        #[arg(long)]
        base: Option<String>,
        /// Output format: json or human (default: human)
        #[arg(long)]
        format: Option<String>,
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
        /// Output format: json or human (default: human)
        #[arg(long)]
        format: Option<String>,
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
        /// Output format: json or human (default: human)
        #[arg(long)]
        format: Option<String>,
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
        /// Output format: json or human (default: human)
        #[arg(long)]
        format: Option<String>,
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
        /// Output format: json or human (default: human)
        #[arg(long)]
        format: Option<String>,
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
        /// Output format: json or human (default: human)
        #[arg(long)]
        format: Option<String>,
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
        /// Output format: json or human (default: human)
        #[arg(long)]
        format: Option<String>,
    },
    /// Create a new GDScript file with boilerplate (or custom content from stdin/--input-file)
    CreateFile {
        /// Path for the new file
        #[arg(long)]
        file: String,
        /// Base class to extend (default: "Node"; ignored when --input-file or stdin is used)
        #[arg(long, default_value = "Node")]
        extends: String,
        /// Optional class_name declaration (ignored when --input-file or stdin is used)
        #[arg(long)]
        class_name: Option<String>,
        /// Read initial content from a file instead of generating boilerplate
        #[arg(long)]
        input_file: Option<String>,
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
        /// Output format: json or human (default: human)
        #[arg(long)]
        format: Option<String>,
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
        /// Output format: json or human (default: human)
        #[arg(long)]
        format: Option<String>,
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
        /// Output format: json or human (default: human)
        #[arg(long)]
        format: Option<String>,
    },
    /// Replace a range of lines (reads new content from stdin or --input-file)
    EditRange {
        /// Path to the GDScript file
        #[arg(long)]
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
    /// Show scene structure from a .tscn file (nodes, resources, connections)
    SceneInfo {
        /// Path to the .tscn file
        #[arg(long)]
        file: String,
        /// Show only nodes (compact output)
        #[arg(long)]
        nodes_only: bool,
        /// Output format: json or human (default: human)
        #[arg(long)]
        format: Option<String>,
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
        /// Output format: json or human (default: human)
        #[arg(long)]
        format: Option<String>,
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

fn print_references_human(result: &crate::lsp::query::ReferencesOutput) {
    use owo_colors::OwoColorize;
    println!(
        "{} ({} reference{})",
        result.symbol.bold(),
        result.references.len(),
        if result.references.len() == 1 {
            ""
        } else {
            "s"
        }
    );
    for r in &result.references {
        println!(
            "  {}:{}:{}  {}",
            r.file.cyan(),
            r.line,
            r.column,
            r.context.dimmed()
        );
    }
}

fn print_definition_human(result: &crate::lsp::query::DefinitionOutput) {
    use owo_colors::OwoColorize;
    println!(
        "{} {} {}:{}:{}",
        result.symbol.bold(),
        "→".dimmed(),
        result.file.cyan(),
        result.line,
        result.column
    );
}

fn print_definition_from_json(val: &serde_json::Value) {
    use owo_colors::OwoColorize;
    let symbol = val.get("symbol").and_then(|v| v.as_str()).unwrap_or("?");
    let file = val.get("file").and_then(|v| v.as_str()).unwrap_or("?");
    let line = val
        .get("line")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let col = val
        .get("column")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    println!(
        "{} {} {}:{line}:{col}",
        symbol.bold(),
        "→".dimmed(),
        file.cyan()
    );
}

fn print_completions_human(items: &[crate::lsp::query::CompletionOutput]) {
    use owo_colors::OwoColorize;
    if items.is_empty() {
        println!("  (no completions)");
        return;
    }
    let max_kind = items.iter().map(|s| s.kind.len()).max().unwrap_or(0);
    let max_label = items.iter().map(|s| s.label.len()).max().unwrap_or(0);

    for item in items {
        let kind_colored = color_kind(&item.kind, max_kind);
        let detail = item
            .detail
            .as_deref()
            .map_or(String::new(), |d| format!("  {}", d.dimmed()));
        println!(
            "  {kind_colored}  {:width$}{detail}",
            item.label.bold(),
            width = max_label,
        );
    }
}

fn color_kind(kind: &str, width: usize) -> String {
    use owo_colors::OwoColorize;
    let padded = format!("{kind:width$}");
    match kind {
        "function" | "method" => padded.cyan().to_string(),
        "constant" => padded.yellow().to_string(),
        "variable" | "property" => padded.blue().to_string(),
        "event" => padded.magenta().to_string(),
        "enum" | "enum_member" => padded.green().to_string(),
        "class" => padded.red().to_string(),
        "keyword" => padded.dimmed().to_string(),
        _ => padded,
    }
}

fn print_symbols_human(symbols: &[crate::lsp::query::SymbolOutput]) {
    use owo_colors::OwoColorize;
    if symbols.is_empty() {
        println!("  (no symbols)");
        return;
    }
    let max_kind = symbols.iter().map(|s| s.kind.len()).max().unwrap_or(0);
    let max_name = symbols.iter().map(|s| s.name.len()).max().unwrap_or(0);

    for s in symbols {
        let kind_colored = color_kind(&s.kind, max_kind);
        let detail = s
            .detail
            .as_deref()
            .map_or(String::new(), |d| format!("  {}", d.dimmed()));
        println!(
            "  {kind_colored}  {:width$}  L{}{detail}",
            s.name.bold(),
            s.line,
            width = max_name,
        );
    }
}

fn is_json(format: Option<&String>) -> bool {
    format.map(String::as_str) == Some("json")
}

fn dry_run_suffix(applied: bool) -> &'static str {
    if applied { "" } else { " (dry run)" }
}

// ── Group A: Reference-heavy ────────────────────────────────────────────────

fn print_rename_human(r: &crate::lsp::query::RenameOutput) {
    use owo_colors::OwoColorize;
    if let Some(ref summary) = r.summary {
        // Applied: show summary
        let file_count = r.changes.len();
        let edit_count: usize = r.changes.iter().map(|c| c.edits.len()).sum();
        println!(
            "{} {} {} ({} edit{} in {} file{})",
            r.symbol.bold(),
            "→".dimmed(),
            r.new_name.green().bold(),
            edit_count,
            if edit_count == 1 { "" } else { "s" },
            file_count,
            if file_count == 1 { "" } else { "s" },
        );
        println!("  {summary}");
    } else {
        // Dry run: show change list
        let file_count = r.changes.len();
        let edit_count: usize = r.changes.iter().map(|c| c.edits.len()).sum();
        println!(
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
                println!("  {}:{}:{}", change.file.cyan(), edit.line, edit.column,);
            }
        }
    }
}

fn print_delete_symbol_human(r: &crate::lsp::refactor::DeleteSymbolOutput) {
    use owo_colors::OwoColorize;
    println!(
        "{} {} {} (lines {}-{}){}",
        if r.applied { "Deleted" } else { "Would delete" },
        r.kind.dimmed(),
        r.symbol.bold(),
        r.removed_lines.start,
        r.removed_lines.end,
        dry_run_suffix(r.applied),
    );
    if !r.references.is_empty() {
        println!(
            "  {} dangling reference{}:",
            r.references.len().to_string().yellow(),
            if r.references.len() == 1 { "" } else { "s" }
        );
        for loc in &r.references {
            println!("  {}:{}:{}", loc.file.cyan(), loc.line, loc.column);
        }
    }
}

fn print_find_implementations_human(r: &crate::lsp::query::ImplementationsOutput) {
    use owo_colors::OwoColorize;
    let count = r.implementations.len();
    println!(
        "{} ({} implementation{})",
        r.method.bold(),
        count,
        if count == 1 { "" } else { "s" }
    );
    for entry in &r.implementations {
        let class_info = match (&entry.class_name, &entry.extends) {
            (Some(cn), Some(ext)) => format!("  {} extends {}", cn.green(), ext.dimmed()),
            (Some(cn), None) => format!("  {}", cn.green()),
            (None, Some(ext)) => format!("  extends {}", ext.dimmed()),
            (None, None) => String::new(),
        };
        println!("  {}:{}{}", entry.file.cyan(), entry.line, class_info);
    }
}

fn print_safe_delete_file_human(r: &crate::lsp::query::SafeDeleteFileOutput) {
    use owo_colors::OwoColorize;
    if r.references.is_empty() {
        if r.deleted {
            println!("Deleted {}", r.file.bold());
        } else {
            println!("{} can be safely deleted", r.file.bold());
        }
    } else {
        println!(
            "{} has {} reference{} — {}",
            r.file.bold(),
            r.references.len(),
            if r.references.len() == 1 { "" } else { "s" },
            if r.deleted {
                "deleted anyway (--force)".to_string()
            } else {
                "blocked".to_string()
            }
        );
        for loc in &r.references {
            println!(
                "  {}:{}  {} {}",
                loc.file.cyan(),
                loc.line,
                loc.kind.dimmed(),
                loc.text.dimmed()
            );
        }
    }
}

// ── Group B: Edit-based one-liners ──────────────────────────────────────────

fn print_edit_human(r: &crate::lsp::refactor::EditOutput) {
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
    println!(
        "{verb}{symbol_part} in {} ({} line{}){}",
        r.file.cyan(),
        r.lines_changed,
        if r.lines_changed == 1 { "" } else { "s" },
        dry_run_suffix(r.applied),
    );
    for w in &r.warnings {
        println!("  {}: {w}", "warning".yellow());
    }
}

fn print_move_symbol_human(r: &crate::lsp::refactor::MoveSymbolOutput) {
    use owo_colors::OwoColorize;
    println!(
        "Moved {} ({}) from {} {} {}{}",
        r.symbol.bold(),
        r.kind.dimmed(),
        r.from.cyan(),
        "→".dimmed(),
        r.to.cyan(),
        dry_run_suffix(r.applied),
    );
    for w in &r.warnings {
        println!("  {}: {w}", "warning".yellow());
    }
}

fn print_extract_method_human(r: &crate::lsp::refactor::ExtractMethodOutput) {
    use owo_colors::OwoColorize;
    println!(
        "Extracted into {}() in {}{}",
        r.function.green().bold(),
        r.file.cyan(),
        dry_run_suffix(r.applied),
    );
    for w in &r.warnings {
        println!("  {}: {w}", "warning".yellow());
    }
}

fn print_inline_method_human(r: &crate::lsp::refactor::InlineMethodOutput) {
    use owo_colors::OwoColorize;
    println!(
        "Inlined {}() at {}:{} ({} line{}){}",
        r.function.bold(),
        r.call_site_file.cyan(),
        r.call_site_line,
        r.inlined_lines,
        if r.inlined_lines == 1 { "" } else { "s" },
        dry_run_suffix(r.applied),
    );
    if r.function_deleted {
        println!("  function definition removed");
    }
    for w in &r.warnings {
        println!("  {}: {w}", "warning".yellow());
    }
}

fn print_inline_method_by_name_human(r: &crate::lsp::refactor::InlineMethodByNameOutput) {
    use owo_colors::OwoColorize;
    println!(
        "Inlined {}() in {} ({} call site{}){}",
        r.function.bold(),
        r.file.cyan(),
        r.call_sites_inlined,
        if r.call_sites_inlined == 1 { "" } else { "s" },
        dry_run_suffix(r.applied),
    );
    if r.function_deleted {
        println!("  function definition removed");
    }
    for w in &r.warnings {
        println!("  {}: {w}", "warning".yellow());
    }
}

fn print_change_signature_human(r: &crate::lsp::refactor::ChangeSignatureOutput) {
    use owo_colors::OwoColorize;
    println!(
        "{} {} {} ({} call site{}){}",
        r.old_signature.dimmed(),
        "→".dimmed(),
        r.new_signature.green().bold(),
        r.call_sites_updated,
        if r.call_sites_updated == 1 { "" } else { "s" },
        dry_run_suffix(r.applied),
    );
    for w in &r.warnings {
        println!("  {}: {w}", "warning".yellow());
    }
}

fn print_introduce_variable_human(r: &crate::lsp::refactor::IntroduceVariableOutput) {
    use owo_colors::OwoColorize;
    println!(
        "Introduced {} = {} in {}{}",
        r.variable.green().bold(),
        r.expression.dimmed(),
        r.file.cyan(),
        dry_run_suffix(r.applied),
    );
}

fn print_introduce_parameter_human(r: &crate::lsp::refactor::IntroduceParameterOutput) {
    use owo_colors::OwoColorize;
    println!(
        "Added parameter {} to {}() in {}{}",
        r.parameter.green().bold(),
        r.function.bold(),
        r.file.cyan(),
        dry_run_suffix(r.applied),
    );
}

fn print_bulk_delete_human(r: &crate::lsp::refactor::BulkDeleteSymbolOutput) {
    use owo_colors::OwoColorize;
    if !r.deleted.is_empty() {
        let names: Vec<&str> = r.deleted.iter().map(|d| d.name.as_str()).collect();
        println!(
            "Deleted {} symbol{} from {}: {}{}",
            r.deleted.len(),
            if r.deleted.len() == 1 { "" } else { "s" },
            r.file.cyan(),
            names.join(", ").bold(),
            dry_run_suffix(r.applied),
        );
    }
    for s in &r.skipped {
        println!("  {}: {} ({})", "skipped".yellow(), s.name, s.reason);
    }
}

fn print_bulk_rename_human(r: &crate::lsp::refactor::BulkRenameOutput) {
    use owo_colors::OwoColorize;
    if !r.renames.is_empty() {
        let pairs: Vec<String> = r
            .renames
            .iter()
            .map(|rn| {
                format!(
                    "{}→{} ({})",
                    rn.old_name,
                    rn.new_name.green(),
                    rn.occurrences
                )
            })
            .collect();
        println!(
            "Renamed in {}: {}{}",
            r.file.cyan(),
            pairs.join(", "),
            dry_run_suffix(r.applied),
        );
    }
    for s in &r.skipped {
        println!(
            "  {}: {}→{} ({})",
            "skipped".yellow(),
            s.old_name,
            s.new_name,
            s.reason
        );
    }
}

fn print_inline_delegate_human(r: &crate::lsp::refactor::InlineDelegateOutput) {
    use owo_colors::OwoColorize;
    println!(
        "Inlined delegate {}() {} {}() ({} call site{}){}",
        r.function.bold(),
        "→".dimmed(),
        r.delegate_target.green().bold(),
        r.call_sites_replaced,
        if r.call_sites_replaced == 1 { "" } else { "s" },
        dry_run_suffix(r.applied),
    );
    if r.function_deleted {
        println!("  function definition removed");
    }
    for w in &r.warnings {
        println!("  {}: {w}", "warning".yellow());
    }
}

fn print_extract_class_human(r: &crate::lsp::refactor::ExtractClassOutput) {
    use owo_colors::OwoColorize;
    let names: Vec<&str> = r.extracted.iter().map(|s| s.name.as_str()).collect();
    println!(
        "Extracted {} symbol{} from {} {} {}: {}{}",
        r.extracted.len(),
        if r.extracted.len() == 1 { "" } else { "s" },
        r.from.cyan(),
        "→".dimmed(),
        r.to.cyan(),
        names.join(", ").bold(),
        dry_run_suffix(r.applied),
    );
    for w in &r.warnings {
        println!("  {}: {w}", "warning".yellow());
    }
}

fn print_create_file_human(r: &crate::lsp::query::CreateFileOutput) {
    use owo_colors::OwoColorize;
    let class_part = r
        .class_name
        .as_deref()
        .map_or_else(String::new, |cn: &str| {
            format!(", class_name {}", cn.green())
        });
    println!(
        "Created {} (extends {}{}, {} line{}){}",
        r.file.cyan().bold(),
        r.extends.green(),
        class_part,
        r.lines,
        if r.lines == 1 { "" } else { "s" },
        dry_run_suffix(r.applied),
    );
}

// ── Group C: Structured data ────────────────────────────────────────────────

fn print_scene_info_human(r: &crate::lsp::query::SceneInfoOutput) {
    use owo_colors::OwoColorize;
    println!("{}", r.file.bold());

    if let Some(ref nodes) = r.nodes {
        for (i, n) in nodes.iter().enumerate() {
            let depth = match n.parent.as_deref() {
                None => 0,
                Some(".") => 1,
                Some(p) => p.chars().filter(|&c| c == '/').count() + 2,
            };

            let indent = if depth == 0 {
                String::new()
            } else {
                let is_last = nodes.get(i + 1).is_none_or(|next| {
                    let nd = match next.parent.as_deref() {
                        None => 0,
                        Some(".") => 1,
                        Some(p) => p.chars().filter(|&c| c == '/').count() + 2,
                    };
                    nd <= depth
                });
                let connector = if is_last { "└── " } else { "├── " };
                format!("{}{}", "│   ".repeat(depth.saturating_sub(1)), connector)
            };

            let type_part = n
                .r#type
                .as_deref()
                .map_or_else(String::new, |t: &str| format!(" ({})", t.dimmed()));
            let script_part = n
                .script
                .as_deref()
                .map_or_else(String::new, |s: &str| format!(" [{}]", s.cyan()));
            let groups_part = if n.groups.is_empty() {
                String::new()
            } else {
                format!(" {{{}}}", n.groups.join(", ").dimmed())
            };

            println!(
                "{indent}{}{type_part}{script_part}{groups_part}",
                n.name.bold()
            );
        }
    }

    if let Some(ref connections) = r.connections
        && !connections.is_empty()
    {
        println!("\n{}:", "Connections".bold());
        for c in connections {
            println!(
                "  {} {} {} {} {}",
                c.from.cyan(),
                format!(".{}", c.signal).dimmed(),
                "→".dimmed(),
                c.to.cyan(),
                format!(".{}", c.method).dimmed()
            );
        }
    }
}

fn print_code_actions_human(actions: &[crate::lsp::query::CodeActionOutput]) {
    use owo_colors::OwoColorize;
    if actions.is_empty() {
        println!("  (no code actions)");
        return;
    }
    for (i, action) in actions.iter().enumerate() {
        println!(
            "  {}. {}",
            (i + 1).to_string().dimmed(),
            action.title.bold()
        );
    }
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
    // Accept pipes (echo "x" | gd lsp ...) and file redirects (< file).
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
pub fn exec(args: LspArgs) -> Result<()> {
    let Some(command) = args.command else {
        // No subcommand — start the LSP server (backward compatible)
        let port = if args.no_godot_proxy {
            0
        } else {
            args.godot_port
        };
        crate::lsp::run_server_with_options(port);
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
            format,
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

            if is_json(format.as_ref()) {
                let json =
                    serde_json::to_string_pretty(&result).map_err(|e| miette::miette!("{e}"))?;
                println!("{json}");
            } else {
                print_rename_human(&result);
            }
            Ok(())
        }
        LspCommand::References {
            name,
            file,
            line,
            column,
            class,
            format,
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
            if format == "json" {
                let j =
                    serde_json::to_string_pretty(&result).map_err(|e| miette::miette!("{e}"))?;
                println!("{j}");
            } else {
                print_references_human(&result);
            }
            Ok(())
        }
        LspCommand::Definition { pos, format } => {
            // Try daemon for rich Godot results
            if !args.no_godot_proxy
                && let Some(result) = crate::lsp::daemon_client::query_daemon(
                    "definition",
                    serde_json::json!({"file": pos.file, "line": pos.line, "column": pos.column}),
                    None,
                )
            {
                if format == "json" {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&result)
                            .map_err(|e| miette::miette!("{e}"))?
                    );
                } else {
                    print_definition_from_json(&result);
                }
                return Ok(());
            }
            // Fallback: static analysis only
            let result =
                crate::lsp::query::query_definition(&pos.file, pos.line, pos.column, None)?;
            if format == "json" {
                let j =
                    serde_json::to_string_pretty(&result).map_err(|e| miette::miette!("{e}"))?;
                println!("{j}");
            } else {
                print_definition_human(&result);
            }
            Ok(())
        }
        LspCommand::Hover { pos, format } => {
            // Try daemon for rich Godot results
            if !args.no_godot_proxy
                && let Some(result) = crate::lsp::daemon_client::query_daemon(
                    "hover",
                    serde_json::json!({"file": pos.file, "line": pos.line, "column": pos.column}),
                    None,
                )
            {
                if format == "json" {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&result)
                            .map_err(|e| miette::miette!("{e}"))?
                    );
                } else if let Some(content) = result.get("content").and_then(|v| v.as_str()) {
                    println!("{content}");
                }
                return Ok(());
            }
            // Fallback: static analysis only
            let result = crate::lsp::query::query_hover(&pos.file, pos.line, pos.column, None)?;
            if format == "json" {
                let j =
                    serde_json::to_string_pretty(&result).map_err(|e| miette::miette!("{e}"))?;
                println!("{j}");
            } else {
                println!("{}", result.content);
            }
            Ok(())
        }
        LspCommand::Completions {
            pos,
            limit,
            kind,
            format,
        } => {
            // Try daemon for rich Godot results
            if !args.no_godot_proxy
                && let Some(result) = crate::lsp::daemon_client::query_daemon(
                    "completion",
                    serde_json::json!({"file": pos.file, "line": pos.line, "column": pos.column}),
                    None,
                )
            {
                let mut items: Vec<serde_json::Value> = if let Some(arr) = result.as_array() {
                    arr.clone()
                } else {
                    vec![result]
                };
                if let Some(ref filter) = kind {
                    items.retain(|v| {
                        v.get("kind")
                            .and_then(serde_json::Value::as_str)
                            .is_some_and(|k| k == filter)
                    });
                }
                if let Some(n) = limit {
                    items.truncate(n);
                }
                println!(
                    "{}",
                    serde_json::to_string_pretty(&items).map_err(|e| miette::miette!("{e}"))?
                );
                return Ok(());
            }
            // Fallback: static analysis only
            let mut result =
                crate::lsp::query::query_completions(&pos.file, pos.line, pos.column, None)?;
            if let Some(ref filter) = kind {
                result.retain(|c| c.kind == *filter);
            }
            if let Some(n) = limit {
                result.truncate(n);
            }
            if format == "json" {
                let j =
                    serde_json::to_string_pretty(&result).map_err(|e| miette::miette!("{e}"))?;
                println!("{j}");
            } else {
                print_completions_human(&result);
            }
            Ok(())
        }
        LspCommand::CodeActions { pos, format } => {
            let result = crate::lsp::query::query_code_actions(&pos.file, pos.line, pos.column)?;
            if is_json(format.as_ref()) {
                let json =
                    serde_json::to_string_pretty(&result).map_err(|e| miette::miette!("{e}"))?;
                println!("{json}");
            } else {
                print_code_actions_human(&result);
            }
            Ok(())
        }
        LspCommand::Diagnostics { paths, format } => {
            crate::lsp::query::query_diagnostics(&paths, is_json(format.as_ref()))
        }
        LspCommand::Symbols { file, kind, format } => {
            let mut result = crate::lsp::query::query_symbols(&file)?;
            let kind_filter: Vec<String> = kind
                .iter()
                .flat_map(|s| s.split(',').map(|k| k.trim().to_lowercase()))
                // "field" and "property" are aliases for "variable" + "field"
                .flat_map(|k| match k.as_str() {
                    "field" | "property" => vec!["variable".to_string(), "field".to_string()],
                    other => vec![other.to_string()],
                })
                .collect();
            if !kind_filter.is_empty() {
                result.retain(|s| kind_filter.iter().any(|k| k == &s.kind));
            }
            if format == "json" {
                let j =
                    serde_json::to_string_pretty(&result).map_err(|e| miette::miette!("{e}"))?;
                println!("{j}");
            } else {
                print_symbols_human(&result);
            }
            Ok(())
        }
        LspCommand::View {
            file,
            range,
            start_line,
            end_line,
            context,
            format,
        } => {
            let (start_line, end_line) = if let Some(ref r) = range {
                let (s, e) = parse_range(r)?;
                (Some(s), Some(e))
            } else {
                (start_line, end_line)
            };
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
                for (i, line) in result.content.lines().enumerate() {
                    let line_num = result.start_line as usize + i;
                    println!("{line_num:>width$}\t{line}");
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
            format,
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
            if is_json(format.as_ref()) {
                let json =
                    serde_json::to_string_pretty(&result).map_err(|e| miette::miette!("{e}"))?;
                println!("{json}");
            } else {
                print_delete_symbol_human(&result);
            }
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
            format,
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
            if is_json(format.as_ref()) {
                let json =
                    serde_json::to_string_pretty(&result).map_err(|e| miette::miette!("{e}"))?;
                println!("{json}");
            } else {
                print_move_symbol_human(&result);
            }
            Ok(())
        }
        LspCommand::ExtractMethod {
            file,
            start_line,
            end_line,
            name,
            dry_run,
            format,
        } => {
            let result = crate::lsp::query::query_extract_method(
                &file, start_line, end_line, &name, dry_run,
            )?;
            if is_json(format.as_ref()) {
                let json =
                    serde_json::to_string_pretty(&result).map_err(|e| miette::miette!("{e}"))?;
                println!("{json}");
            } else {
                print_extract_method_human(&result);
            }
            Ok(())
        }
        LspCommand::InlineMethod {
            file,
            name,
            all,
            line,
            column,
            dry_run,
            format,
        } => {
            if let Some(ref func_name) = name {
                let result =
                    crate::lsp::query::query_inline_method_by_name(&file, func_name, all, dry_run)?;
                if is_json(format.as_ref()) {
                    let json = serde_json::to_string_pretty(&result)
                        .map_err(|e| miette::miette!("{e}"))?;
                    println!("{json}");
                } else {
                    print_inline_method_by_name_human(&result);
                }
            } else {
                let line = line
                    .ok_or_else(|| miette::miette!("--line is required when not using --name"))?;
                let column = column
                    .ok_or_else(|| miette::miette!("--column is required when not using --name"))?;
                let result = crate::lsp::query::query_inline_method(&file, line, column, dry_run)?;
                if is_json(format.as_ref()) {
                    let json = serde_json::to_string_pretty(&result)
                        .map_err(|e| miette::miette!("{e}"))?;
                    println!("{json}");
                } else {
                    print_inline_method_human(&result);
                }
            }
            Ok(())
        }
        LspCommand::SafeDeleteFile {
            file,
            force,
            dry_run,
            format,
        } => {
            let result = crate::lsp::query::query_safe_delete_file(&file, force, dry_run)?;
            if is_json(format.as_ref()) {
                let json =
                    serde_json::to_string_pretty(&result).map_err(|e| miette::miette!("{e}"))?;
                println!("{json}");
            } else {
                print_safe_delete_file_human(&result);
            }
            if !force && !result.references.is_empty() {
                std::process::exit(1);
            }
            Ok(())
        }
        LspCommand::FindImplementations { name, base, format } => {
            let result = crate::lsp::query::query_find_implementations(&name, base.as_deref())?;
            if is_json(format.as_ref()) {
                let json =
                    serde_json::to_string_pretty(&result).map_err(|e| miette::miette!("{e}"))?;
                println!("{json}");
            } else {
                print_find_implementations_human(&result);
            }
            Ok(())
        }
        LspCommand::IntroduceVariable {
            file,
            line,
            column,
            end_column,
            name,
            dry_run,
            format,
        } => {
            let result = crate::lsp::query::query_introduce_variable(
                &file, line, column, end_column, &name, dry_run,
            )?;
            if is_json(format.as_ref()) {
                let json =
                    serde_json::to_string_pretty(&result).map_err(|e| miette::miette!("{e}"))?;
                println!("{json}");
            } else {
                print_introduce_variable_human(&result);
            }
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
            format,
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
            if is_json(format.as_ref()) {
                let json =
                    serde_json::to_string_pretty(&result).map_err(|e| miette::miette!("{e}"))?;
                println!("{json}");
            } else {
                print_introduce_parameter_human(&result);
            }
            Ok(())
        }
        LspCommand::CreateFile {
            file,
            extends,
            class_name,
            input_file,
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
            let result = crate::lsp::query::query_create_file(
                &file,
                &extends,
                class_name.as_deref(),
                custom_content.as_deref(),
                dry_run,
            )?;
            if is_json(format.as_ref()) {
                let json =
                    serde_json::to_string_pretty(&result).map_err(|e| miette::miette!("{e}"))?;
                println!("{json}");
            } else {
                print_create_file_human(&result);
            }
            Ok(())
        }
        LspCommand::ReplaceBody {
            file,
            name,
            class,
            input_file,
            no_format,
            dry_run,
            format,
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
            if is_json(format.as_ref()) {
                let json =
                    serde_json::to_string_pretty(&result).map_err(|e| miette::miette!("{e}"))?;
                println!("{json}");
            } else {
                print_edit_human(&result);
            }
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
            let result = crate::lsp::query::query_insert(
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
                println!("{json}");
            } else {
                print_edit_human(&result);
            }
            Ok(())
        }
        LspCommand::ReplaceSymbol {
            file,
            name,
            class,
            input_file,
            no_format,
            dry_run,
            format,
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
            if is_json(format.as_ref()) {
                let json =
                    serde_json::to_string_pretty(&result).map_err(|e| miette::miette!("{e}"))?;
                println!("{json}");
            } else {
                print_edit_human(&result);
            }
            Ok(())
        }
        LspCommand::EditRange {
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
            let result = crate::lsp::query::query_edit_range(
                &file, start, end, &content, no_format, dry_run,
            )?;
            if is_json(format.as_ref()) {
                let json =
                    serde_json::to_string_pretty(&result).map_err(|e| miette::miette!("{e}"))?;
                println!("{json}");
            } else {
                print_edit_human(&result);
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
            format,
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
            if is_json(format.as_ref()) {
                let json =
                    serde_json::to_string_pretty(&result).map_err(|e| miette::miette!("{e}"))?;
                println!("{json}");
            } else {
                print_change_signature_human(&result);
            }
            Ok(())
        }
        LspCommand::BulkDeleteSymbol {
            file,
            names,
            force,
            dry_run,
            format,
        } => {
            let result =
                crate::lsp::query::query_bulk_delete_symbol(&file, &names, force, dry_run)?;
            if is_json(format.as_ref()) {
                let json =
                    serde_json::to_string_pretty(&result).map_err(|e| miette::miette!("{e}"))?;
                println!("{json}");
            } else {
                print_bulk_delete_human(&result);
            }
            Ok(())
        }
        LspCommand::BulkRename {
            file,
            renames,
            dry_run,
            format,
        } => {
            let result = crate::lsp::query::query_bulk_rename(&file, &renames, dry_run)?;
            if is_json(format.as_ref()) {
                let json =
                    serde_json::to_string_pretty(&result).map_err(|e| miette::miette!("{e}"))?;
                println!("{json}");
            } else {
                print_bulk_rename_human(&result);
            }
            Ok(())
        }
        LspCommand::InlineDelegate {
            file,
            name,
            dry_run,
            format,
        } => {
            let result = crate::lsp::query::query_inline_delegate(&file, &name, dry_run)?;
            if is_json(format.as_ref()) {
                let json =
                    serde_json::to_string_pretty(&result).map_err(|e| miette::miette!("{e}"))?;
                println!("{json}");
            } else {
                print_inline_delegate_human(&result);
            }
            Ok(())
        }
        LspCommand::ExtractClass {
            file,
            symbols,
            to,
            dry_run,
            format,
        } => {
            let result = crate::lsp::query::query_extract_class(&file, &symbols, &to, dry_run)?;
            if is_json(format.as_ref()) {
                let json =
                    serde_json::to_string_pretty(&result).map_err(|e| miette::miette!("{e}"))?;
                println!("{json}");
            } else {
                print_extract_class_human(&result);
            }
            Ok(())
        }
        LspCommand::SceneInfo {
            file,
            nodes_only,
            format,
        } => {
            let result = crate::lsp::query::query_scene_info(&file, nodes_only)?;
            if is_json(format.as_ref()) {
                let json =
                    serde_json::to_string_pretty(&result).map_err(|e| miette::miette!("{e}"))?;
                println!("{json}");
            } else {
                print_scene_info_human(&result);
            }
            Ok(())
        }
    }
}
