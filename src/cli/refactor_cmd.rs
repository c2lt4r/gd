use crate::cprintln;
use clap::{Args, Subcommand};
use miette::Result;

#[derive(Args)]
pub struct RefactorArgs {
    #[command(subcommand)]
    pub command: RefactorCommand,
}

#[derive(Subcommand)]
pub enum RefactorCommand {
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
    /// Inline a variable: replace all usages with its initializer, then delete the declaration
    InlineVariable {
        /// Path to the GDScript file
        #[arg(long)]
        file: String,
        /// Line number of the variable (1-based)
        #[arg(long)]
        line: usize,
        /// Column number of the variable (1-based)
        #[arg(long)]
        column: usize,
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
        /// Introduce as const instead of var
        #[arg(long = "const")]
        as_const: bool,
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
    /// Invert an if/else: negate condition and swap branches
    InvertIf {
        /// Path to the GDScript file
        #[arg(long)]
        file: String,
        /// Line number of the if statement (1-based)
        #[arg(long)]
        line: usize,
        /// Preview without writing changes
        #[arg(long)]
        dry_run: bool,
        /// Output format: json or human (default: human)
        #[arg(long)]
        format: Option<String>,
    },
    /// Convert between $NodePath and get_node() syntax
    ConvertNodePath {
        /// Path to the GDScript file
        #[arg(long)]
        file: String,
        /// Line number (1-based)
        #[arg(long)]
        line: usize,
        /// Column number (1-based)
        #[arg(long)]
        column: usize,
        /// Preview without writing changes
        #[arg(long)]
        dry_run: bool,
        /// Output format: json or human (default: human)
        #[arg(long)]
        format: Option<String>,
    },
    /// Convert between @onready var and _ready() assignment
    ConvertOnready {
        /// Path to the GDScript file
        #[arg(long)]
        file: String,
        /// Variable name to convert
        #[arg(long)]
        name: String,
        /// Convert @onready → _ready() assignment
        #[arg(long)]
        to_ready: bool,
        /// Convert _ready() assignment → @onready
        #[arg(long)]
        to_onready: bool,
        /// Preview without writing changes
        #[arg(long)]
        dry_run: bool,
        /// Output format: json or human (default: human)
        #[arg(long)]
        format: Option<String>,
    },
    /// Convert signal connection between scene wiring and code
    ConvertSignal {
        /// Path to the .tscn scene file
        #[arg(long)]
        file: String,
        /// Signal name (e.g., "pressed")
        #[arg(long)]
        signal: String,
        /// Source node path (e.g., "Button")
        #[arg(long)]
        from: String,
        /// Handler method name (e.g., "_on_button_pressed")
        #[arg(long)]
        method: String,
        /// Move connection from scene to code (.connect() in _ready())
        #[arg(long)]
        to_code: bool,
        /// Move connection from code to scene ([connection] in .tscn)
        #[arg(long)]
        to_scene: bool,
        /// Preview without writing changes
        #[arg(long)]
        dry_run: bool,
        /// Output format: json or human (default: human)
        #[arg(long)]
        format: Option<String>,
    },
    /// Flatten nested ifs to early return/continue guard clauses
    ExtractGuards {
        /// Path to the GDScript file
        #[arg(long)]
        file: String,
        /// Function name
        #[arg(long)]
        name: String,
        /// Preview without writing changes
        #[arg(long)]
        dry_run: bool,
        /// Output format: json or human (default: human)
        #[arg(long)]
        format: Option<String>,
    },
    /// Split `var x = expr` into separate declaration and assignment
    SplitDeclaration {
        /// Path to the GDScript file
        #[arg(long)]
        file: String,
        /// Line number of the variable declaration (1-based)
        #[arg(long)]
        line: usize,
        /// Preview without writing changes
        #[arg(long)]
        dry_run: bool,
        /// Output format: json or human (default: human)
        #[arg(long)]
        format: Option<String>,
    },
    /// Join bare `var x` with following `x = expr` into `var x = expr`
    JoinDeclaration {
        /// Path to the GDScript file
        #[arg(long)]
        file: String,
        /// Line number of the bare variable declaration (1-based)
        #[arg(long)]
        line: usize,
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
    /// Check if a file can be safely deleted (find all cross-file references)
    SafeDeleteFile {
        /// Path to the GDScript file
        #[arg(long)]
        file: String,
        /// Actually delete the file (without this flag, only reports references)
        #[arg(long)]
        force: bool,
        /// Preview references without deleting (this is the default behavior)
        #[arg(long)]
        dry_run: bool,
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
        /// Preview without writing changes
        #[arg(long)]
        dry_run: bool,
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
    /// Undo a refactoring operation (most recent by default), or list undo entries with --list
    Undo {
        /// List recent refactoring operations that can be undone
        #[arg(long)]
        list: bool,
        /// Undo a specific entry by ID (default: most recent)
        #[arg(long)]
        id: Option<u64>,
        /// Preview without restoring files
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

fn print_rename_human(r: &crate::lsp::query::RenameOutput) {
    use owo_colors::OwoColorize;
    if let Some(ref summary) = r.summary {
        // Applied: show summary
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
        // Dry run: show change list
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

fn print_delete_symbol_human(r: &crate::lsp::refactor::DeleteSymbolOutput) {
    use owo_colors::OwoColorize;
    cprintln!(
        "{} {} {} (lines {}-{}){}",
        if r.applied { "Deleted" } else { "Would delete" },
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

fn print_move_symbol_human(r: &crate::lsp::refactor::MoveSymbolOutput) {
    use owo_colors::OwoColorize;
    cprintln!(
        "Moved {} ({}) from {} {} {}{}",
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

fn print_extract_method_human(r: &crate::lsp::refactor::ExtractMethodOutput) {
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

fn print_inline_method_human(r: &crate::lsp::refactor::InlineMethodOutput) {
    use owo_colors::OwoColorize;
    cprintln!(
        "Inlined {}() at {}:{} ({} line{}){}",
        r.function.bold(),
        r.call_site_file.cyan(),
        r.call_site_line,
        r.inlined_lines,
        if r.inlined_lines == 1 { "" } else { "s" },
        dry_run_suffix(r.applied),
    );
    if r.function_deleted {
        cprintln!("  function definition removed");
    }
    for w in &r.warnings {
        cprintln!("  {}: {w}", "warning".yellow());
    }
}

fn print_inline_method_by_name_human(r: &crate::lsp::refactor::InlineMethodByNameOutput) {
    use owo_colors::OwoColorize;
    cprintln!(
        "Inlined {}() in {} ({} call site{}){}",
        r.function.bold(),
        r.file.cyan(),
        r.call_sites_inlined,
        if r.call_sites_inlined == 1 { "" } else { "s" },
        dry_run_suffix(r.applied),
    );
    if r.function_deleted {
        cprintln!("  function definition removed");
    }
    for w in &r.warnings {
        cprintln!("  {}: {w}", "warning".yellow());
    }
}

fn print_inline_variable_human(r: &crate::lsp::refactor::InlineVariableOutput) {
    use owo_colors::OwoColorize;
    cprintln!(
        "Inlined {} = {} ({} usage{}, line {}) in {}{}",
        r.variable.green().bold(),
        r.expression.dimmed(),
        r.reference_count,
        if r.reference_count == 1 { "" } else { "s" },
        r.definition_line,
        r.file.cyan(),
        dry_run_suffix(r.applied),
    );
    for w in &r.warnings {
        cprintln!("  {}: {w}", "warning".yellow());
    }
}

fn print_inline_delegate_human(r: &crate::lsp::refactor::InlineDelegateOutput) {
    use owo_colors::OwoColorize;
    cprintln!(
        "Inlined delegate {}() {} {}() ({} call site{}){}",
        r.function.bold(),
        "→".dimmed(),
        r.delegate_target.green().bold(),
        r.call_sites_replaced,
        if r.call_sites_replaced == 1 { "" } else { "s" },
        dry_run_suffix(r.applied),
    );
    if r.function_deleted {
        cprintln!("  function definition removed");
    }
    for w in &r.warnings {
        cprintln!("  {}: {w}", "warning".yellow());
    }
}

fn print_introduce_variable_human(r: &crate::lsp::refactor::IntroduceVariableOutput) {
    use owo_colors::OwoColorize;
    let keyword = if r.is_const { "const" } else { "var" };
    cprintln!(
        "Introduced {} {} = {} in {}{}",
        keyword.bold(),
        r.variable.green().bold(),
        r.expression.dimmed(),
        r.file.cyan(),
        dry_run_suffix(r.applied),
    );
    for w in &r.warnings {
        cprintln!("  {}: {w}", "warning".yellow());
    }
}

fn print_introduce_parameter_human(r: &crate::lsp::refactor::IntroduceParameterOutput) {
    use owo_colors::OwoColorize;
    cprintln!(
        "Added parameter {} to {}() in {}{}",
        r.parameter.green().bold(),
        r.function.bold(),
        r.file.cyan(),
        dry_run_suffix(r.applied),
    );
}

fn print_invert_if_human(r: &crate::lsp::refactor::InvertIfOutput) {
    use owo_colors::OwoColorize;
    cprintln!(
        "Inverted if at {}:{} ({} {} {}){}",
        r.file.cyan(),
        r.line,
        r.original_condition.dimmed(),
        "→".dimmed(),
        r.inverted_condition.green().bold(),
        dry_run_suffix(r.applied),
    );
}

fn print_convert_node_path_human(r: &crate::lsp::refactor::ConvertNodePathOutput) {
    use owo_colors::OwoColorize;
    cprintln!(
        "Converted {} {} {} in {}:{}{}",
        r.original.dimmed(),
        "→".dimmed(),
        r.converted.green().bold(),
        r.file.cyan(),
        r.line,
        dry_run_suffix(r.applied),
    );
}

fn print_convert_onready_human(r: &crate::lsp::refactor::ConvertOnreadyOutput) {
    use owo_colors::OwoColorize;
    let dir = if r.direction == "to-ready" {
        "@onready → _ready()"
    } else {
        "_ready() → @onready"
    };
    cprintln!(
        "Converted {} ({}) in {}:{}{}",
        r.variable.green().bold(),
        dir.dimmed(),
        r.file.cyan(),
        r.line,
        dry_run_suffix(r.applied),
    );
}

fn print_convert_signal_human(r: &crate::lsp::refactor::ConvertSignalOutput) {
    use owo_colors::OwoColorize;
    let dir = if r.direction == "to-code" {
        "scene → code"
    } else {
        "code → scene"
    };
    cprintln!(
        "Converted {}.{} → {} ({}) [{} {} {}]{}",
        r.from_node,
        r.signal.green().bold(),
        r.method.green().bold(),
        dir.dimmed(),
        r.scene_file.cyan(),
        "↔".dimmed(),
        r.script_file.cyan(),
        dry_run_suffix(r.applied),
    );
}

fn print_extract_guards_human(r: &crate::lsp::refactor::ExtractGuardsOutput) {
    use owo_colors::OwoColorize;
    cprintln!(
        "Extracted {} guard{} in {} ({}){}",
        r.guards.len().to_string().green().bold(),
        if r.guards.len() == 1 { "" } else { "s" },
        r.function.cyan(),
        r.file.cyan(),
        dry_run_suffix(r.applied),
    );
    for g in &r.guards {
        cprintln!(
            "  {} {} {} → {} {}",
            "guard:".dimmed(),
            g.original_condition.dimmed(),
            "→".dimmed(),
            format!("if {}:", g.negated_condition).green(),
            g.exit_keyword.yellow(),
        );
    }
}

fn print_split_declaration_human(r: &crate::lsp::refactor::SplitDeclarationOutput) {
    use owo_colors::OwoColorize;
    cprintln!(
        "Split {} at {}:{}{}",
        r.variable.green().bold(),
        r.file.cyan(),
        r.line,
        dry_run_suffix(r.applied),
    );
}

fn print_join_declaration_human(r: &crate::lsp::refactor::JoinDeclarationOutput) {
    use owo_colors::OwoColorize;
    cprintln!(
        "Joined {} at {}:{}{}",
        r.variable.green().bold(),
        r.file.cyan(),
        r.line,
        dry_run_suffix(r.applied),
    );
}

fn print_bulk_delete_human(r: &crate::lsp::refactor::BulkDeleteSymbolOutput) {
    use owo_colors::OwoColorize;
    if !r.deleted.is_empty() {
        let names: Vec<&str> = r.deleted.iter().map(|d| d.name.as_str()).collect();
        cprintln!(
            "Deleted {} symbol{} from {}: {}{}",
            r.deleted.len(),
            if r.deleted.len() == 1 { "" } else { "s" },
            r.file.cyan(),
            names.join(", ").bold(),
            dry_run_suffix(r.applied),
        );
    }
    for s in &r.skipped {
        cprintln!("  {}: {} ({})", "skipped".yellow(), s.name, s.reason);
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
        cprintln!(
            "Renamed in {}: {}{}",
            r.file.cyan(),
            pairs.join(", "),
            dry_run_suffix(r.applied),
        );
    }
    for s in &r.skipped {
        cprintln!(
            "  {}: {}→{} ({})",
            "skipped".yellow(),
            s.old_name,
            s.new_name,
            s.reason
        );
    }
}

fn print_extract_class_human(r: &crate::lsp::refactor::ExtractClassOutput) {
    use owo_colors::OwoColorize;
    let names: Vec<&str> = r.extracted.iter().map(|s| s.name.as_str()).collect();
    cprintln!(
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
        cprintln!("  {}: {w}", "warning".yellow());
    }
}

fn print_safe_delete_file_human(r: &crate::lsp::query::SafeDeleteFileOutput) {
    use owo_colors::OwoColorize;
    if r.references.is_empty() {
        if r.deleted {
            cprintln!("Deleted {}", r.file.bold());
        } else {
            cprintln!("{} can be safely deleted", r.file.bold());
        }
    } else {
        cprintln!(
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
            cprintln!(
                "  {}:{}  {} {}",
                loc.file.cyan(),
                loc.line,
                loc.kind.dimmed(),
                loc.text.dimmed()
            );
        }
    }
}

fn print_move_file_human(r: &crate::lsp::refactor::MoveFileOutput) {
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

fn print_change_signature_human(r: &crate::lsp::refactor::ChangeSignatureOutput) {
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

fn print_undo_list_human(entries: &[crate::lsp::refactor::UndoEntry]) {
    use owo_colors::OwoColorize;
    if entries.is_empty() {
        cprintln!("No undo entries.");
        return;
    }
    cprintln!(
        "{} undo entr{}:",
        entries.len(),
        if entries.len() == 1 { "y" } else { "ies" }
    );
    for entry in entries {
        cprintln!(
            "  {} {} {} ({})",
            format!("#{}", entry.id).yellow().bold(),
            entry.command.bold(),
            entry.description.dimmed(),
            entry.timestamp.dimmed(),
        );
        for f in &entry.files {
            cprintln!("    {} {:?}", f.path.cyan(), f.action);
        }
    }
}

fn print_undo_human(entry: &crate::lsp::refactor::UndoEntry, dry_run: bool) {
    use owo_colors::OwoColorize;
    cprintln!(
        "{} {} {}{}",
        if dry_run { "Would undo" } else { "Undone" },
        format!("#{}", entry.id).yellow().bold(),
        entry.description.dimmed(),
        dry_run_suffix(!dry_run),
    );
    for f in &entry.files {
        cprintln!("  {} {:?}", f.path.cyan(), f.action);
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

                // Snapshot affected files for undo before applying
                let mut snaps: std::collections::HashMap<std::path::PathBuf, Option<Vec<u8>>> =
                    std::collections::HashMap::new();
                for fe in &result.changes {
                    let p = project_root.join(&fe.file);
                    if let Ok(content) = std::fs::read(&p) {
                        snaps.insert(p, Some(content));
                    }
                }

                let count = crate::lsp::query::apply_rename(&result, &project_root)?;

                // Record undo
                let stack = crate::lsp::refactor::UndoStack::open(&project_root);
                let _ = stack.record(
                    "rename",
                    &format!("rename {} → {}", result.symbol, result.new_name),
                    &snaps,
                    &project_root,
                );

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
        RefactorCommand::DeleteSymbol {
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
                cprintln!("{json}");
            } else {
                print_delete_symbol_human(&result);
            }
            if !force && !result.references.is_empty() {
                std::process::exit(1);
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
                cprintln!("{json}");
            } else {
                print_move_symbol_human(&result);
            }
            Ok(())
        }
        RefactorCommand::ExtractMethod {
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
                cprintln!("{json}");
            } else {
                print_extract_method_human(&result);
            }
            Ok(())
        }
        RefactorCommand::InlineMethod {
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
                    cprintln!("{json}");
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
                    cprintln!("{json}");
                } else {
                    print_inline_method_human(&result);
                }
            }
            Ok(())
        }
        RefactorCommand::InlineVariable {
            file,
            line,
            column,
            dry_run,
            format,
        } => {
            let result = crate::lsp::query::query_inline_variable(&file, line, column, dry_run)?;
            if is_json(format.as_ref()) {
                let json =
                    serde_json::to_string_pretty(&result).map_err(|e| miette::miette!("{e}"))?;
                cprintln!("{json}");
            } else {
                print_inline_variable_human(&result);
            }
            Ok(())
        }
        RefactorCommand::InlineDelegate {
            file,
            name,
            dry_run,
            format,
        } => {
            let result = crate::lsp::query::query_inline_delegate(&file, &name, dry_run)?;
            if is_json(format.as_ref()) {
                let json =
                    serde_json::to_string_pretty(&result).map_err(|e| miette::miette!("{e}"))?;
                cprintln!("{json}");
            } else {
                print_inline_delegate_human(&result);
            }
            Ok(())
        }
        RefactorCommand::IntroduceVariable {
            file,
            line,
            column,
            end_column,
            name,
            as_const,
            dry_run,
            format,
        } => {
            let result = crate::lsp::query::query_introduce_variable(
                &file, line, column, end_column, &name, as_const, dry_run,
            )?;
            if is_json(format.as_ref()) {
                let json =
                    serde_json::to_string_pretty(&result).map_err(|e| miette::miette!("{e}"))?;
                cprintln!("{json}");
            } else {
                print_introduce_variable_human(&result);
            }
            Ok(())
        }
        RefactorCommand::IntroduceParameter {
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
                cprintln!("{json}");
            } else {
                print_introduce_parameter_human(&result);
            }
            Ok(())
        }
        RefactorCommand::InvertIf {
            file,
            line,
            dry_run,
            format,
        } => {
            let result = crate::lsp::query::query_invert_if(&file, line, dry_run)?;
            if is_json(format.as_ref()) {
                let json =
                    serde_json::to_string_pretty(&result).map_err(|e| miette::miette!("{e}"))?;
                cprintln!("{json}");
            } else {
                print_invert_if_human(&result);
            }
            Ok(())
        }
        RefactorCommand::ConvertNodePath {
            file,
            line,
            column,
            dry_run,
            format,
        } => {
            let result = crate::lsp::query::query_convert_node_path(&file, line, column, dry_run)?;
            if is_json(format.as_ref()) {
                let json =
                    serde_json::to_string_pretty(&result).map_err(|e| miette::miette!("{e}"))?;
                cprintln!("{json}");
            } else {
                print_convert_node_path_human(&result);
            }
            Ok(())
        }
        RefactorCommand::ConvertOnready {
            file,
            name,
            to_ready,
            to_onready,
            dry_run,
            format,
        } => {
            let direction = match (to_ready, to_onready) {
                (true, false) => true,
                (false, true) => false,
                _ => {
                    return Err(miette::miette!(
                        "specify exactly one of --to-ready or --to-onready"
                    ));
                }
            };
            let result =
                crate::lsp::query::query_convert_onready(&file, &name, direction, dry_run)?;
            if is_json(format.as_ref()) {
                let json =
                    serde_json::to_string_pretty(&result).map_err(|e| miette::miette!("{e}"))?;
                cprintln!("{json}");
            } else {
                print_convert_onready_human(&result);
            }
            Ok(())
        }
        RefactorCommand::ConvertSignal {
            file,
            signal,
            from,
            method,
            to_code,
            to_scene,
            dry_run,
            format,
        } => {
            let direction = match (to_code, to_scene) {
                (true, false) => true,
                (false, true) => false,
                _ => {
                    return Err(miette::miette!(
                        "specify exactly one of --to-code or --to-scene"
                    ));
                }
            };
            let result = crate::lsp::query::query_convert_signal(
                &file, &signal, &from, &method, direction, dry_run,
            )?;
            if is_json(format.as_ref()) {
                let json =
                    serde_json::to_string_pretty(&result).map_err(|e| miette::miette!("{e}"))?;
                cprintln!("{json}");
            } else {
                print_convert_signal_human(&result);
            }
            Ok(())
        }
        RefactorCommand::ExtractGuards {
            file,
            name,
            dry_run,
            format,
        } => {
            let result = crate::lsp::query::query_extract_guards(&file, &name, dry_run)?;
            if is_json(format.as_ref()) {
                let json =
                    serde_json::to_string_pretty(&result).map_err(|e| miette::miette!("{e}"))?;
                cprintln!("{json}");
            } else {
                print_extract_guards_human(&result);
            }
            Ok(())
        }
        RefactorCommand::SplitDeclaration {
            file,
            line,
            dry_run,
            format,
        } => {
            let result = crate::lsp::query::query_split_declaration(&file, line, dry_run)?;
            if is_json(format.as_ref()) {
                let json =
                    serde_json::to_string_pretty(&result).map_err(|e| miette::miette!("{e}"))?;
                cprintln!("{json}");
            } else {
                print_split_declaration_human(&result);
            }
            Ok(())
        }
        RefactorCommand::JoinDeclaration {
            file,
            line,
            dry_run,
            format,
        } => {
            let result = crate::lsp::query::query_join_declaration(&file, line, dry_run)?;
            if is_json(format.as_ref()) {
                let json =
                    serde_json::to_string_pretty(&result).map_err(|e| miette::miette!("{e}"))?;
                cprintln!("{json}");
            } else {
                print_join_declaration_human(&result);
            }
            Ok(())
        }
        RefactorCommand::BulkDeleteSymbol {
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
                cprintln!("{json}");
            } else {
                print_bulk_delete_human(&result);
            }
            Ok(())
        }
        RefactorCommand::BulkRename {
            file,
            renames,
            dry_run,
            format,
        } => {
            let result = crate::lsp::query::query_bulk_rename(&file, &renames, dry_run)?;
            if is_json(format.as_ref()) {
                let json =
                    serde_json::to_string_pretty(&result).map_err(|e| miette::miette!("{e}"))?;
                cprintln!("{json}");
            } else {
                print_bulk_rename_human(&result);
            }
            Ok(())
        }
        RefactorCommand::ExtractClass {
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
                cprintln!("{json}");
            } else {
                print_extract_class_human(&result);
            }
            Ok(())
        }
        RefactorCommand::SafeDeleteFile {
            file,
            force,
            dry_run,
            format,
        } => {
            let result = crate::lsp::query::query_safe_delete_file(&file, force, dry_run)?;
            if is_json(format.as_ref()) {
                let json =
                    serde_json::to_string_pretty(&result).map_err(|e| miette::miette!("{e}"))?;
                cprintln!("{json}");
            } else {
                print_safe_delete_file_human(&result);
            }
            if !force && !result.references.is_empty() {
                std::process::exit(1);
            }
            Ok(())
        }
        RefactorCommand::MoveFile {
            from,
            to,
            dry_run,
            format,
        } => {
            let result = crate::lsp::query::query_move_file(&from, &to, dry_run)?;
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
                cprintln!("{json}");
            } else {
                print_change_signature_human(&result);
            }
            Ok(())
        }
        RefactorCommand::Undo {
            list,
            id,
            dry_run,
            format,
        } => {
            if list {
                let entries = crate::lsp::query::query_undo_list()?;
                if is_json(format.as_ref()) {
                    let json = serde_json::to_string_pretty(&entries)
                        .map_err(|e| miette::miette!("{e}"))?;
                    cprintln!("{json}");
                } else {
                    print_undo_list_human(&entries);
                }
            } else {
                let entry = crate::lsp::query::query_undo(id, dry_run)?;
                if is_json(format.as_ref()) {
                    let json = serde_json::to_string_pretty(&entry)
                        .map_err(|e| miette::miette!("{e}"))?;
                    cprintln!("{json}");
                } else {
                    print_undo_human(&entry, dry_run);
                }
            }
            Ok(())
        }
    }
}
