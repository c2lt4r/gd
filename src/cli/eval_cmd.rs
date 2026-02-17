use std::fmt::Write;

use clap::Args;
use miette::{Result, miette};
use owo_colors::OwoColorize;
use serde::Serialize;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use crate::build::{find_godot, path_for_godot};
use crate::cli::test_cmd::{extract_errors, filter_noise, run_with_timeout};
use crate::core::config::Config;
use crate::core::project::GodotProject;

#[derive(Args)]
pub struct EvalArgs {
    /// GDScript expression, .gd file path, or "-" for stdin
    pub input: String,
    /// Validate script before running
    #[arg(long)]
    pub check: bool,
    /// Run headless (default: true)
    #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
    pub headless: bool,
    /// Timeout in seconds (default: 30)
    #[arg(short, long, default_value_t = 30)]
    pub timeout: u64,
    /// Output format (text or json)
    #[arg(long, default_value = "text")]
    pub format: String,
    /// Show Godot engine output
    #[arg(short, long)]
    pub verbose: bool,
    /// Skip sandbox checks (allow dangerous APIs like OS.execute)
    #[arg(long, alias = "no-sandbox")]
    pub r#unsafe: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InputMode {
    Expression,
    File,
    Stdin,
}

#[derive(Debug, Serialize)]
struct EvalOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<EvalError>,
}

#[derive(Debug, Serialize)]
struct EvalError {
    pub file: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
    pub message: String,
}

/// Detect whether input is stdin ("-"), an existing .gd file, or an expression.
fn detect_input_mode(input: &str, project_root: &Path) -> InputMode {
    if input == "-" {
        return InputMode::Stdin;
    }
    let path = Path::new(input);
    if path.extension().is_some_and(|e| e == "gd")
        && (path.is_file() || project_root.join(path).is_file())
    {
        return InputMode::File;
    }
    InputMode::Expression
}

/// Generate a wrapper GDScript that evaluates an expression or runs statements.
fn generate_wrapper_script(input: &str) -> String {
    // Split on semicolons for multi-statement support, but also handle newlines
    let statements: Vec<&str> = if input.contains(';') {
        input
            .split(';')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect()
    } else if input.contains('\n') {
        input.lines().collect()
    } else {
        // Single expression — wrap with result printing
        return format!(
            "extends SceneTree\n\
             \n\
             func _init():\n\
             \tvar __result = {input}\n\
             \tif __result != null:\n\
             \t\tprint(__result)\n\
             \tquit()\n"
        );
    };

    let mut body = String::new();
    for stmt in &statements {
        let _ = writeln!(body, "\t{stmt}");
    }

    format!(
        "extends SceneTree\n\
         \n\
         func _init():\n\
         {body}\tquit()\n"
    )
}

/// Write the temporary eval script into .godot/gd-eval-tmp.gd
fn write_temp_script(project_root: &Path, content: &str) -> Result<PathBuf> {
    let godot_dir = project_root.join(".godot");
    if !godot_dir.is_dir() {
        std::fs::create_dir_all(&godot_dir)
            .map_err(|e| miette!("Failed to create .godot directory: {e}"))?;
    }
    let path = godot_dir.join("gd-eval-tmp.gd");
    std::fs::write(&path, content).map_err(|e| miette!("Failed to write temp script: {e}"))?;
    Ok(path)
}

/// Parse-validate a script without running Godot.
pub fn pre_check(source: &str) -> Result<String> {
    let tree = crate::core::parser::parse(source)?;
    if tree.root_node().has_error() {
        let details = find_parse_errors(&tree, source);
        return Err(miette!("Script has syntax errors:\n{details}"));
    }
    // Strip invalid escape sequences in string literals — Godot crashes
    // on these instead of reporting a parse error gracefully.
    Ok(sanitize_escapes(source))
}

/// Walk the tree-sitter AST and collect human-readable error descriptions.
fn find_parse_errors(tree: &tree_sitter::Tree, source: &str) -> String {
    let mut errors = Vec::new();
    let mut cursor = tree.root_node().walk();
    collect_parse_errors(&mut cursor, source, &mut errors);
    if errors.is_empty() {
        return "unknown parse error".to_string();
    }
    errors.join("\n")
}

fn collect_parse_errors(
    cursor: &mut tree_sitter::TreeCursor,
    source: &str,
    errors: &mut Vec<String>,
) {
    loop {
        let node = cursor.node();
        if node.is_error() || node.is_missing() {
            let start = node.start_position();
            let line = source.lines().nth(start.row).unwrap_or("");
            errors.push(format!(
                "  line {}:{} — {}\n  | {}",
                start.row + 1,
                start.column + 1,
                if node.is_missing() {
                    format!("missing {}", node.kind())
                } else {
                    "unexpected token".to_string()
                },
                line.trim(),
            ));
        }
        if cursor.goto_first_child() {
            collect_parse_errors(cursor, source, errors);
            cursor.goto_parent();
        }
        if !cursor.goto_next_sibling() {
            break;
        }
    }
}

/// GDScript valid escape characters after `\`.
const VALID_ESCAPES: &[char] = &[
    '\\', '\'', '"', 'n', 't', 'r', 'a', 'b', 'f', 'v', '0', 'x', 'u', 'U',
];

/// Strip invalid escape sequences in string literals (`\!` → `!`).
/// Godot crashes on these instead of reporting a parse error.
fn sanitize_escapes(source: &str) -> String {
    let mut result = String::with_capacity(source.len());
    let mut chars = source.chars().peekable();
    let mut in_string: Option<char> = None;

    while let Some(ch) = chars.next() {
        match in_string {
            None => {
                result.push(ch);
                if ch == '#' {
                    // Copy rest of comment line
                    for c in chars.by_ref() {
                        result.push(c);
                        if c == '\n' {
                            break;
                        }
                    }
                } else if ch == '"' || ch == '\'' {
                    in_string = Some(ch);
                }
            }
            Some(quote) => {
                if ch == '\\' {
                    if let Some(&next) = chars.peek() {
                        if VALID_ESCAPES.contains(&next) || next == '\n' {
                            result.push(ch); // keep the backslash
                        }
                        // else: drop the backslash (invalid escape)
                    } else {
                        result.push(ch);
                    }
                } else {
                    result.push(ch);
                    if ch == quote {
                        in_string = None;
                    }
                }
            }
        }
    }
    result
}

/// APIs blocked by the sandbox. These are system-level escapes that can cause
/// damage outside the game process.
const SANDBOX_BLOCKED: &[&str] = &[
    // Process execution
    "OS.execute",
    "OS.create_process",
    "OS.kill",
    "OS.shell_open",
    "OS.crash",
    // Network
    "HTTPRequest",
    "HTTPClient",
    // Native code loading
    "GDExtension",
    "GDExtensionManager",
    // Threading (could bypass sandbox)
    "Thread",
];

/// Check if a script contains blocked API calls or unsafe file access.
/// Returns a list of violations found.
fn sandbox_check(source: &str) -> Vec<String> {
    let mut violations = Vec::new();
    for &pattern in SANDBOX_BLOCKED {
        if source.contains(pattern) {
            violations.push(pattern.to_string());
        }
    }
    // Check FileAccess/DirAccess paths — only res:// and user:// are allowed
    for api in &["FileAccess.open(", "DirAccess.open("] {
        let mut search_from = 0;
        while let Some(pos) = source[search_from..].find(api) {
            let after = search_from + pos + api.len();
            search_from = after;
            let rest = source[after..].trim_start();
            let Some(quote @ ('"' | '\'')) = rest.chars().next() else {
                continue; // variable path — can't check statically
            };
            if let Some(end) = rest[1..].find(quote) {
                let path = &rest[1..=end];
                if !path.starts_with("res://") && !path.starts_with("user://") {
                    violations.push(format!(
                        "{} with path \"{path}\" — only res:// and user:// paths are allowed",
                        &api[..api.len() - 1], // strip trailing '('
                    ));
                }
            }
        }
    }
    violations
}

/// Validate that a .gd file extends SceneTree or MainLoop (required for --script).
/// Without this check, Godot shows an OS error dialog on Windows that blocks execution.
fn validate_script_base_class(path: &Path) -> Result<()> {
    let source = std::fs::read_to_string(path)
        .map_err(|e| miette!("Failed to read {}: {e}", path.display()))?;
    for line in source.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("extends") {
            let base = rest.trim();
            if base == "SceneTree" || base == "MainLoop" {
                return Ok(());
            }
            return Err(miette!(
                "Script extends '{base}', but --script requires 'extends SceneTree' or 'extends MainLoop'"
            ));
        }
        // Skip comments and empty lines at top of file
        if !trimmed.is_empty() && !trimmed.starts_with('#') && !trimmed.starts_with('@') {
            break;
        }
    }
    Err(miette!(
        "Script has no 'extends' declaration. --script requires 'extends SceneTree' or 'extends MainLoop'"
    ))
}

/// GDScript builtin functions that return void (not in ClassDB).
const VOID_BUILTINS: &[&str] = &[
    "print",
    "print_rich",
    "printt",
    "prints",
    "printraw",
    "printerr",
    "print_verbose",
    "push_error",
    "push_warning",
    "assert",
    "breakpoint",
];

/// Heuristic: does this single expression look like a void-returning call?
/// `return void_call()` is a compile error that pauses the remote debugger,
/// so we must detect these on the Rust side before generating the wrapper.
fn looks_like_void_call(expr: &str) -> bool {
    let trimmed = expr.trim();

    // Direct builtin calls: print(...), push_error(...)
    if let Some(name) = extract_function_name(trimmed)
        && VOID_BUILTINS.contains(&name)
    {
        return true;
    }

    // Dotted method calls: check the last method in a chain against ClassDB
    if let Some(method_name) = extract_last_method_name(trimmed) {
        // Check every class in ClassDB — if this method returns void on ANY class,
        // assume it's void (safe: we lose the return value of null/void, no crash)
        if crate::class_db::is_method_void_anywhere(method_name) {
            return true;
        }
    }

    false
}

/// Extract the function name from a bare call like `print("hi")`.
/// Returns `None` for dotted calls or non-call expressions.
fn extract_function_name(expr: &str) -> Option<&str> {
    let paren = expr.find('(')?;
    let name = &expr[..paren];
    // Must be a simple identifier (no dots)
    if name.chars().all(|c| c.is_alphanumeric() || c == '_') && !name.is_empty() {
        Some(name)
    } else {
        None
    }
}

/// Extract the last method name from a dotted call chain.
/// `get_tree().set_pause(false)` → `Some("set_pause")`
/// `print("hi")` → `None` (no dot)
fn extract_last_method_name(expr: &str) -> Option<&str> {
    let bytes = expr.as_bytes();
    let mut depth = 0i32;
    let mut last_dot = None;
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'(' | b'[' => depth += 1,
            b')' | b']' => depth -= 1,
            b'.' if depth == 0 => last_dot = Some(i),
            _ => {}
        }
    }
    let after_dot = &expr[last_dot? + 1..];
    // Extract just the method name (before the opening paren)
    let paren = after_dot.find('(')?;
    Some(&after_dot[..paren])
}

/// Generate a GDScript that the eval server will load and execute.
/// The script extends Node so `get_node()` with absolute paths works.
pub fn generate_live_eval_script(input: &str) -> String {
    // If the input is already a complete script (has extends + run method), use as-is
    let trimmed = input.trim();
    if trimmed.starts_with("extends ") && trimmed.contains("func run()") {
        return input.to_string();
    }

    // Check if it looks like multi-statement (contains ; or newlines)
    let statements: Vec<&str> = if input.contains(';') {
        input
            .split(';')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect()
    } else if input.contains('\n') {
        input.lines().collect()
    } else {
        // Single expression — wrap with return, unless it's a void call
        return if looks_like_void_call(input) {
            format!(
                "extends Node\n\
                 \n\
                 func run():\n\
                 \t{input}\n"
            )
        } else {
            format!(
                "extends Node\n\
                 \n\
                 func run():\n\
                 \treturn {input}\n"
            )
        };
    };

    let mut body = String::new();
    for stmt in &statements {
        let _ = writeln!(body, "\t{stmt}");
    }

    format!(
        "extends Node\n\
         \n\
         func run():\n\
         {body}"
    )
}

/// GDScript keywords for syntax highlighting.
const GDSCRIPT_KEYWORDS: &[&str] = &[
    "extends",
    "class_name",
    "func",
    "var",
    "const",
    "signal",
    "enum",
    "return",
    "if",
    "elif",
    "else",
    "for",
    "while",
    "match",
    "break",
    "continue",
    "pass",
    "self",
    "super",
    "class",
    "static",
    "await",
    "yield",
    "true",
    "false",
    "null",
    "not",
    "and",
    "or",
    "in",
    "is",
    "as",
    "void",
    "int",
    "float",
    "bool",
    "String",
];

/// Print a GDScript source with line numbers and basic syntax highlighting.
fn print_highlighted_script(source: &str) {
    let lines: Vec<&str> = source.lines().collect();
    let width = lines.len().to_string().len().max(3);
    for (i, line) in lines.iter().enumerate() {
        let num = format!("{:>width$}", i + 1);
        eprint!("  {} ", num.dimmed());
        eprintln!("{}", highlight_gdscript_line(line));
    }
}

/// Apply basic ANSI syntax highlighting to a single GDScript line.
fn highlight_gdscript_line(line: &str) -> String {
    // Handle empty/whitespace-only lines
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return line.to_string();
    }

    // Comments — whole line dimmed green
    if trimmed.starts_with('#') {
        return format!("{}", line.green().dimmed());
    }

    let mut result = String::new();
    let mut chars = line.chars().peekable();

    while let Some(&ch) = chars.peek() {
        if ch == '"' || ch == '\'' {
            // String literal — green
            let quote = ch;
            let mut s = String::new();
            s.push(chars.next().unwrap());
            let mut escaped = false;
            for c in chars.by_ref() {
                s.push(c);
                if escaped {
                    escaped = false;
                } else if c == '\\' {
                    escaped = true;
                } else if c == quote {
                    break;
                }
            }
            let _ = write!(result, "{}", s.green());
        } else if ch.is_ascii_digit()
            || (ch == '-'
                && chars.clone().nth(1).is_some_and(|c| c.is_ascii_digit())
                && (result.is_empty()
                    || result.ends_with(|c: char| !c.is_alphanumeric() && c != '_')))
        {
            // Numeric literal — yellow
            let mut s = String::new();
            if ch == '-' {
                s.push(chars.next().unwrap());
            }
            while let Some(&c) = chars.peek() {
                if c.is_ascii_digit() || c == '.' || c == '_' || c == 'x' || c == 'b' || c == 'o' {
                    s.push(chars.next().unwrap());
                } else {
                    break;
                }
            }
            let _ = write!(result, "{}", s.yellow());
        } else if ch.is_alphabetic() || ch == '_' || ch == '@' {
            // Word — check if it's a keyword
            let mut word = String::new();
            while let Some(&c) = chars.peek() {
                if c.is_alphanumeric() || c == '_' || (word.is_empty() && c == '@') {
                    word.push(chars.next().unwrap());
                } else {
                    break;
                }
            }
            if GDSCRIPT_KEYWORDS.contains(&word.as_str()) {
                let _ = write!(result, "{}", word.cyan());
            } else if word.starts_with('@') {
                let _ = write!(result, "{}", word.yellow());
            } else {
                result.push_str(&word);
            }
        } else {
            result.push(chars.next().unwrap());
        }
    }

    result
}

/// Try live eval against a running game (started with `gd run`).
/// Returns `None` if no eval-ready game is running (caller falls back to offline).
fn try_live_eval(
    input: &str,
    project_root: &Path,
    timeout: Duration,
    json_mode: bool,
    sandbox: bool,
) -> Option<Result<()>> {
    // 1. Generate the request script (instant)
    let script = generate_live_eval_script(input);

    // 2. Sandbox check — fail fast before waiting for daemon
    if sandbox {
        let violations = sandbox_check(&script);
        if !violations.is_empty() {
            return Some(Err(miette!(
                "Sandbox blocked: {}\n\
                 Use {} to bypass sandbox checks",
                violations.join(", "),
                "--unsafe".bold(),
            )));
        }
    }

    // 3. Check if eval server is ready (quick check before showing script)
    let godot_dir = project_root.join(".godot");
    let ready_path = godot_dir.join("gd-eval-ready");

    let eval_ready = crate::lsp::daemon_client::query_daemon(
        "eval_status",
        serde_json::json!({"timeout": timeout.as_secs()}),
        Some(timeout + Duration::from_secs(5)),
    )
    .and_then(|r| r.get("ready").and_then(serde_json::Value::as_bool))
    .unwrap_or(false);

    // Fallback: daemon may have restarted (build ID mismatch) and lost state,
    // but the eval server in Godot is still running and wrote the ready file.
    if !eval_ready && !ready_path.is_file() {
        return None;
    }

    // 4. Show the script being sent with syntax highlighting
    if !json_mode {
        print_highlighted_script(&script);
    }

    // 5. Delegate to shared send_eval (with output capture for REPL)
    match crate::core::live_eval::send_eval_with_output(&script, project_root, timeout) {
        Ok(response) => {
            // Show captured print output first
            if !json_mode {
                for line in &response.output {
                    match line.r#type.as_str() {
                        "error" => eprintln!("{}", line.message.red()),
                        "warning" => eprintln!("{}", line.message.yellow()),
                        _ => println!("{}", line.message),
                    }
                }
            }
            if json_mode {
                let out = EvalOutput {
                    stdout: response.result,
                    stderr: String::new(),
                    exit_code: 0,
                    errors: vec![],
                };
                println!("{}", serde_json::to_string_pretty(&out).unwrap());
            } else if !response.result.is_empty() {
                println!("{}", response.result);
            }
            Some(Ok(()))
        }
        Err(e) => {
            if json_mode {
                let out = EvalOutput {
                    stdout: String::new(),
                    stderr: e.to_string(),
                    exit_code: 1,
                    errors: vec![],
                };
                println!("{}", serde_json::to_string_pretty(&out).unwrap());
                std::process::exit(1);
            } else {
                Some(Err(e))
            }
        }
    }
}

#[allow(clippy::too_many_lines)]
pub fn exec(args: &EvalArgs) -> Result<()> {
    let json_mode = match args.format.as_str() {
        "text" => false,
        "json" => true,
        other => {
            return Err(miette!(
                "Invalid format '{other}' (expected 'text' or 'json')"
            ));
        }
    };

    let cwd = std::env::current_dir().unwrap_or_default();
    let config = Config::load(&cwd)?;
    let project = GodotProject::discover(&cwd)?;

    let mode = detect_input_mode(&args.input, &project.root);

    // Read stdin once up front so both live and offline paths can use it
    let stdin_text = if mode == InputMode::Stdin {
        let mut buf = String::new();
        std::io::stdin()
            .read_to_string(&mut buf)
            .map_err(|e| miette!("Failed to read stdin: {e}"))?;
        Some(buf)
    } else {
        None
    };

    // Try live eval first for expressions/stdin (if a game is running with `gd run`)
    if mode != InputMode::File {
        let input_text = stdin_text.as_deref().unwrap_or(&args.input);
        if let Some(result) = try_live_eval(
            input_text,
            &project.root,
            Duration::from_secs(args.timeout),
            json_mode,
            !args.r#unsafe,
        ) {
            return result;
        }
        // No eval server running — fall through to offline
    }

    let godot = find_godot(&config)?;

    // Determine the script path and content
    let (script_path, temp_file) = match mode {
        InputMode::File => {
            let path = Path::new(&args.input);
            let resolved = if path.is_file() {
                path.to_path_buf()
            } else {
                project.root.join(path)
            };
            validate_script_base_class(&resolved)?;
            (resolved, None)
        }
        InputMode::Stdin => {
            let source = stdin_text.as_deref().unwrap_or("");
            let trimmed = source.trim();
            // Complete script (has extends) — use as-is, validate base class
            let content = if trimmed.starts_with("extends ") {
                source.to_string()
            } else {
                generate_wrapper_script(source)
            };
            let path = write_temp_script(&project.root, &content)?;
            if trimmed.starts_with("extends ") {
                validate_script_base_class(&path)?;
            }
            (path.clone(), Some(path))
        }
        InputMode::Expression => {
            let wrapper = generate_wrapper_script(&args.input);
            let path = write_temp_script(&project.root, &wrapper)?;
            (path.clone(), Some(path))
        }
    };

    // Sandbox + optional pre-check
    let source = std::fs::read_to_string(&script_path)
        .map_err(|e| miette!("Failed to read {}: {e}", script_path.display()))?;

    if !args.r#unsafe {
        let violations = sandbox_check(&source);
        if !violations.is_empty() {
            cleanup_temp(temp_file.as_ref());
            return Err(miette!(
                "Sandbox blocked: {}\n\
                 Use {} to bypass sandbox checks",
                violations.join(", "),
                "--unsafe".bold(),
            ));
        }
    }

    // Always syntax-check + sanitize escape sequences before launching Godot
    let source = match pre_check(&source) {
        Ok(sanitized) => sanitized,
        Err(e) => {
            cleanup_temp(temp_file.as_ref());
            return Err(e);
        }
    };
    // Re-write the temp file if escapes were sanitized
    if let Some(ref path) = temp_file {
        let _ = std::fs::write(path, &source);
    }

    let mut cmd = Command::new(&godot);
    if args.headless {
        cmd.arg("--headless");
    }
    cmd.arg("--no-header")
        .arg("--path")
        .arg(path_for_godot(&godot, &project.root))
        .arg("--script")
        .arg(path_for_godot(&godot, &script_path));
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

    let result = run_with_timeout(&mut cmd, Duration::from_secs(args.timeout), true);

    cleanup_temp(temp_file.as_ref());

    match result {
        Ok(output) => {
            format_output(&output, args.verbose, json_mode);
            Ok(())
        }
        Err(e) => {
            if json_mode {
                let eval_out = EvalOutput {
                    stdout: String::new(),
                    stderr: e.to_string(),
                    exit_code: -1,
                    errors: vec![],
                };
                println!("{}", serde_json::to_string_pretty(&eval_out).unwrap());
                std::process::exit(1);
            } else {
                Err(e)
            }
        }
    }
}

fn format_output(output: &std::process::Output, verbose: bool, json_mode: bool) {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let exit_code = output.status.code().unwrap_or(-1);

    if json_mode {
        let errors: Vec<EvalError> = extract_errors(&stderr)
            .into_iter()
            .map(|e| EvalError {
                file: e.file,
                line: e.line,
                message: e.message,
            })
            .collect();
        let eval_out = EvalOutput {
            stdout: stdout.trim().to_string(),
            stderr: filter_noise(&stderr).trim().to_string(),
            exit_code,
            errors,
        };
        println!("{}", serde_json::to_string_pretty(&eval_out).unwrap());
        if !output.status.success() {
            std::process::exit(1);
        }
        return;
    }

    // Text mode: print stdout directly
    let display = stdout.trim();
    if !display.is_empty() {
        println!("{display}");
    }

    if !output.status.success() {
        let errors = extract_errors(&stderr);
        if errors.is_empty() {
            let filtered = filter_noise(&stderr);
            if !filtered.trim().is_empty() {
                eprintln!("{}", filtered.trim());
            }
        } else {
            for err in &errors {
                if let Some(line_num) = err.line {
                    eprintln!(
                        "{} {}:{line_num} {}",
                        "error:".red().bold(),
                        err.file,
                        err.message
                    );
                } else {
                    eprintln!("{} {}", "error:".red().bold(), err.message);
                }
            }
        }
        std::process::exit(1);
    }

    // Show stderr in verbose mode (engine output)
    if verbose {
        let filtered = filter_noise(&stderr);
        if !filtered.trim().is_empty() {
            for line in filtered.trim().lines() {
                eprintln!("{}", line.dimmed());
            }
        }
    }
}

fn cleanup_temp(temp_file: Option<&PathBuf>) {
    if let Some(path) = temp_file {
        let _ = std::fs::remove_file(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_expression() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(
            detect_input_mode("1 + 1", tmp.path()),
            InputMode::Expression
        );
        assert_eq!(
            detect_input_mode("Vector2(1,2).normalized()", tmp.path()),
            InputMode::Expression
        );
        assert_eq!(
            detect_input_mode("var x = 1; print(x)", tmp.path()),
            InputMode::Expression
        );
    }

    #[test]
    fn detect_stdin() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(detect_input_mode("-", tmp.path()), InputMode::Stdin);
    }

    #[test]
    fn detect_file() {
        let tmp = tempfile::tempdir().unwrap();
        let gd_file = tmp.path().join("script.gd");
        std::fs::write(&gd_file, "extends Node\n").unwrap();
        assert_eq!(
            detect_input_mode(gd_file.to_str().unwrap(), tmp.path()),
            InputMode::File
        );
    }

    #[test]
    fn detect_file_relative() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("test.gd"), "extends Node\n").unwrap();
        assert_eq!(detect_input_mode("test.gd", tmp.path()), InputMode::File);
    }

    #[test]
    fn detect_nonexistent_gd_as_expression() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(
            detect_input_mode("nonexistent.gd", tmp.path()),
            InputMode::Expression
        );
    }

    #[test]
    fn wrapper_single_expression() {
        let script = generate_wrapper_script("1 + 1");
        assert!(script.contains("var __result = 1 + 1"));
        assert!(script.contains("print(__result)"));
        assert!(script.contains("quit()"));
        assert!(script.contains("extends SceneTree"));
    }

    #[test]
    fn wrapper_multi_statement_semicolons() {
        let script = generate_wrapper_script("var x = 1; print(x * 2)");
        assert!(script.contains("var x = 1"));
        assert!(script.contains("print(x * 2)"));
        assert!(script.contains("quit()"));
        assert!(!script.contains("__result"));
    }

    #[test]
    fn wrapper_multi_line() {
        let script = generate_wrapper_script("var x = 1\nprint(x * 2)");
        assert!(script.contains("var x = 1"));
        assert!(script.contains("print(x * 2)"));
        assert!(script.contains("quit()"));
    }

    #[test]
    fn wrapper_parses_cleanly() {
        let cases = ["1 + 1", "Vector2(1,2).normalized()", "var x = 1; print(x)"];
        for input in cases {
            let script = generate_wrapper_script(input);
            let tree = crate::core::parser::parse(&script).unwrap();
            assert!(
                !tree.root_node().has_error(),
                "Wrapper for '{input}' should parse cleanly, got:\n{script}"
            );
        }
    }

    #[test]
    fn pre_check_valid() {
        assert!(pre_check("extends SceneTree\nfunc _init():\n\tprint(1)\n\tquit()\n").is_ok());
    }

    #[test]
    fn pre_check_invalid() {
        assert!(pre_check("extends SceneTree\nfunc _init():\n\tif if if\n").is_err());
    }

    #[test]
    fn validate_base_class_scene_tree() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("ok.gd");
        std::fs::write(&path, "extends SceneTree\nfunc _init():\n\tquit()\n").unwrap();
        assert!(validate_script_base_class(&path).is_ok());
    }

    #[test]
    fn validate_base_class_main_loop() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("ok.gd");
        std::fs::write(&path, "extends MainLoop\nfunc _init():\n\tquit()\n").unwrap();
        assert!(validate_script_base_class(&path).is_ok());
    }

    #[test]
    fn validate_base_class_rejects_node() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("bad.gd");
        std::fs::write(&path, "extends Node\nfunc _ready():\n\tpass\n").unwrap();
        let err = validate_script_base_class(&path).unwrap_err();
        assert!(err.to_string().contains("extends 'Node'"));
    }

    #[test]
    fn validate_base_class_with_annotations() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("tool.gd");
        std::fs::write(&path, "@tool\nextends SceneTree\nfunc _init():\n\tquit()\n").unwrap();
        assert!(validate_script_base_class(&path).is_ok());
    }

    #[test]
    fn live_script_single_expression() {
        let script = generate_live_eval_script("1 + 1");
        assert!(script.contains("extends Node"));
        assert!(script.contains("func run():"));
        assert!(script.contains("return 1 + 1"));
    }

    #[test]
    fn live_script_multi_statement() {
        let script = generate_live_eval_script("var x = 1; print(x)");
        assert!(script.contains("extends Node"));
        assert!(script.contains("func run():"));
        assert!(script.contains("var x = 1"));
        assert!(script.contains("print(x)"));
        assert!(!script.contains("return"));
    }

    #[test]
    fn live_script_multi_line() {
        let script = generate_live_eval_script("var x = 1\nprint(x)");
        assert!(script.contains("extends Node"));
        assert!(script.contains("var x = 1"));
        assert!(script.contains("print(x)"));
    }

    #[test]
    fn live_script_parses_cleanly() {
        let cases = [
            "1 + 1",
            "get_tree().get_root().get_child_count()",
            "var x = 1; print(x)",
        ];
        for input in cases {
            let script = generate_live_eval_script(input);
            let tree = crate::core::parser::parse(&script).unwrap();
            assert!(
                !tree.root_node().has_error(),
                "Live script for '{input}' should parse cleanly, got:\n{script}"
            );
        }
    }

    #[test]
    fn live_script_complete_passthrough() {
        let full_script =
            "extends Node\n\nfunc run():\n\tvar label = Label.new()\n\treturn label\n";
        let result = generate_live_eval_script(full_script);
        assert_eq!(result, full_script);
    }

    #[test]
    fn try_live_eval_no_server() {
        let tmp = tempfile::tempdir().unwrap();
        // No ready file — should return None
        let result = try_live_eval("1+1", tmp.path(), Duration::from_secs(1), false, true);
        assert!(result.is_none());
    }

    #[test]
    fn try_live_eval_no_daemon() {
        // Without a daemon running, try_live_eval returns None (fall back to offline)
        let tmp = tempfile::tempdir().unwrap();
        let result = try_live_eval("21 * 2", tmp.path(), Duration::from_secs(1), false, true);
        assert!(result.is_none());
    }

    #[test]
    fn try_live_eval_sandbox_blocks() {
        // Sandbox blocks before reaching daemon, returns Some(Err)
        let tmp = tempfile::tempdir().unwrap();
        let result = try_live_eval(
            "OS.execute('cmd', [])",
            tmp.path(),
            Duration::from_secs(1),
            false,
            true,
        );
        assert!(result.is_some());
        assert!(result.unwrap().is_err());
    }

    #[test]
    fn sandbox_blocks_os_execute() {
        let violations = sandbox_check("OS.execute('rm', ['-rf', '/'])");
        assert!(violations.iter().any(|v| v.contains("OS.execute")));
    }

    #[test]
    fn sandbox_blocks_http() {
        let violations = sandbox_check("var client = HTTPClient.new()");
        assert!(violations.iter().any(|v| v.contains("HTTPClient")));
    }

    #[test]
    fn sandbox_allows_safe_code() {
        let violations = sandbox_check("get_tree().get_root().get_child_count()");
        assert!(violations.is_empty());
    }

    #[test]
    fn sandbox_allows_res_path() {
        let violations = sandbox_check("FileAccess.open('res://data.json', FileAccess.READ)");
        assert!(violations.is_empty());
    }

    #[test]
    fn sandbox_allows_user_path() {
        let violations =
            sandbox_check("FileAccess.open(\"user://saves/game.dat\", FileAccess.READ)");
        assert!(violations.is_empty());
    }

    #[test]
    fn sandbox_blocks_absolute_path() {
        let violations = sandbox_check("FileAccess.open('/home/user/.env', FileAccess.READ)");
        assert!(violations.iter().any(|v| v.contains(".env")));
    }

    #[test]
    fn sandbox_blocks_dotenv() {
        let violations = sandbox_check("FileAccess.open(\".env\", FileAccess.READ)");
        assert!(violations.iter().any(|v| v.contains("FileAccess.open")));
    }

    #[test]
    fn sandbox_blocks_dir_access_absolute() {
        let violations = sandbox_check("DirAccess.open(\"/etc\")");
        assert!(violations.iter().any(|v| v.contains("DirAccess.open")));
    }

    #[test]
    fn sandbox_allows_variable_path() {
        // Can't statically check variable paths — allow through
        let violations = sandbox_check("var p = get_path(); FileAccess.open(p, FileAccess.READ)");
        assert!(violations.is_empty());
    }

    #[test]
    fn sandbox_blocks_multiple() {
        let violations = sandbox_check("OS.execute('cmd'); var t = Thread.new()");
        assert!(violations.iter().any(|v| v.contains("OS.execute")));
        assert!(violations.iter().any(|v| v.contains("Thread")));
    }

    #[test]
    fn sanitize_valid_escapes_unchanged() {
        let src = r#"var s = "hello\nworld\t""#;
        assert_eq!(sanitize_escapes(src), src);
        let src2 = r#"var s = "path\\to\\file""#;
        assert_eq!(sanitize_escapes(src2), src2);
        let src3 = r#"var s = "say \"hi\"""#;
        assert_eq!(sanitize_escapes(src3), src3);
        let src4 = r"var s = '\r\0\a\b\f\v'";
        assert_eq!(sanitize_escapes(src4), src4);
    }

    #[test]
    fn sanitize_strips_invalid_escapes() {
        assert_eq!(
            sanitize_escapes(r#"var s = "hello\!""#),
            r#"var s = "hello!""#
        );
        assert_eq!(
            sanitize_escapes(r#"var s = "test\q""#),
            r#"var s = "testq""#
        );
        assert_eq!(sanitize_escapes(r"var s = 'bad\z'"), "var s = 'badz'");
    }

    #[test]
    fn sanitize_skips_comments() {
        let src = "# this has \\! in a comment\nvar x = 1";
        assert_eq!(sanitize_escapes(src), src);
    }

    #[test]
    fn sanitize_no_strings_unchanged() {
        let src = "var x = 1 + 2";
        assert_eq!(sanitize_escapes(src), src);
    }

    #[test]
    fn pre_check_sanitizes_invalid_escape() {
        let result =
            pre_check("extends SceneTree\nfunc _init():\n\tvar s = \"test\\!\"\n\tquit()\n");
        assert!(result.is_ok());
        assert!(result.unwrap().contains("\"test!\""));
    }

    #[test]
    fn pre_check_node_script_with_run() {
        // This is the exact format that `generate_live_eval_script` passthrough produces
        let src = "extends Node\n\nfunc run():\n\treturn 42\n";
        let result = pre_check(src);
        assert!(
            result.is_ok(),
            "pre_check should accept extends Node script: {result:?}"
        );
    }

    #[test]
    fn pre_check_multiline_node_script() {
        // More complex script like the FPS overlay
        let src = "extends Node\n\
                    \n\
                    func run():\n\
                    \tvar label = Label.new()\n\
                    \tlabel.text = \"hello\"\n\
                    \treturn label\n";
        let result = pre_check(src);
        assert!(
            result.is_ok(),
            "pre_check should accept multi-line node script: {result:?}"
        );
    }

    #[test]
    fn void_call_detection() {
        // Builtin void calls should NOT get return prefix
        assert!(looks_like_void_call("print(\"hello\")"));
        assert!(looks_like_void_call("push_error(\"bad\")"));
        assert!(looks_like_void_call("printerr(\"fail\")"));
        // Dotted void methods (from ClassDB)
        assert!(looks_like_void_call("node.queue_free()"));
        assert!(looks_like_void_call("get_tree().get_root().add_child(n)"));
        assert!(looks_like_void_call("get_tree().set_pause(false)"));
        // Non-void calls should NOT match
        assert!(!looks_like_void_call("1 + 1"));
        assert!(!looks_like_void_call(
            "get_tree().get_root().get_child_count()"
        ));
        assert!(!looks_like_void_call("Vector2(1,2).normalized()"));
        assert!(!looks_like_void_call("str(42)"));
    }

    #[test]
    fn live_script_void_call_no_return() {
        let script = generate_live_eval_script("print(\"hello\")");
        assert!(script.contains("func run():"));
        assert!(script.contains("\tprint(\"hello\")"));
        assert!(!script.contains("return print"));
    }

    #[test]
    fn live_script_non_void_has_return() {
        let script = generate_live_eval_script("1 + 1");
        assert!(script.contains("return 1 + 1"));
    }

    #[test]
    fn pre_check_error_includes_details() {
        let err = pre_check("extends SceneTree\nfunc _init():\n\tif if if\n")
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("line 3"),
            "Error should include line number: {err}"
        );
        assert!(
            err.contains("if if if") || err.contains("unexpected"),
            "Error should include context: {err}"
        );
    }
}
