use std::fmt::Write;

use clap::Args;
use miette::{Result, miette};
use owo_colors::OwoColorize;
use serde::{Deserialize, Serialize};
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
fn pre_check(source: &str) -> Result<()> {
    let tree = crate::core::parser::parse(source)?;
    if tree.root_node().has_error() {
        return Err(miette!("Script has syntax errors"));
    }
    Ok(())
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

/// JSON result from the eval server.
#[derive(Debug, Deserialize)]
struct LiveEvalResult {
    result: Option<String>,
    error: String,
}

/// Generate a GDScript that the eval server will load and execute.
/// The script extends Node so `get_node()` with absolute paths works.
fn generate_live_eval_script(input: &str) -> String {
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
        // Single expression — wrap with return
        return format!(
            "extends Node\n\
             \n\
             func run():\n\
             \treturn {input}\n"
        );
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

/// Try live eval against a running game (started with `gd run --eval`).
/// Returns `None` if no eval-ready game is running (caller falls back to offline).
fn try_live_eval(
    input: &str,
    project_root: &Path,
    timeout: Duration,
    json_mode: bool,
) -> Option<Result<()>> {
    let godot_dir = project_root.join(".godot");
    let ready_file = godot_dir.join("gd-eval-ready");

    // Check if eval server is running
    if !ready_file.is_file() {
        return None;
    }

    // Generate and write the request script
    let script = generate_live_eval_script(input);

    // Show the script being sent (cat -n style)
    if !json_mode {
        eprintln!("{}", "Sending to running game:".dimmed());
        for (i, line) in script.lines().enumerate() {
            eprintln!("  {} {}", format!("{:>3}", i + 1).dimmed(), line);
        }
    }

    let request_path = godot_dir.join("gd-eval-request.gd");
    if let Err(e) = std::fs::write(&request_path, &script) {
        return Some(Err(miette!("Failed to write eval request: {e}")));
    }

    // Poll for the result file
    let result_path = godot_dir.join("gd-eval-result.json");
    let poll_interval = Duration::from_millis(50);
    let start = std::time::Instant::now();

    loop {
        if result_path.is_file() {
            // Read and delete the result
            let data = match std::fs::read_to_string(&result_path) {
                Ok(d) => d,
                Err(e) => return Some(Err(miette!("Failed to read eval result: {e}"))),
            };
            let _ = std::fs::remove_file(&result_path);

            match serde_json::from_str::<LiveEvalResult>(&data) {
                Ok(eval_result) => {
                    if json_mode {
                        let out = EvalOutput {
                            stdout: eval_result.result.clone().unwrap_or_default(),
                            stderr: eval_result.error.clone(),
                            exit_code: i32::from(!eval_result.error.is_empty()),
                            errors: vec![],
                        };
                        println!("{}", serde_json::to_string_pretty(&out).unwrap());
                        if !eval_result.error.is_empty() {
                            std::process::exit(1);
                        }
                    } else if !eval_result.error.is_empty() {
                        eprintln!("{} {}", "error:".red().bold(), eval_result.error);
                        std::process::exit(1);
                    } else if let Some(ref result) = eval_result.result {
                        println!("{result}");
                    }
                    return Some(Ok(()));
                }
                Err(e) => return Some(Err(miette!("Failed to parse eval result: {e}"))),
            }
        }

        // Check if the eval server is still alive
        if !ready_file.is_file() {
            // Clean up the request file if server died
            let _ = std::fs::remove_file(&request_path);
            return Some(Err(miette!("Eval server exited before returning a result")));
        }

        if start.elapsed() >= timeout {
            // Clean up the request file on timeout
            let _ = std::fs::remove_file(&request_path);
            return Some(Err(miette!(
                "Timed out waiting for eval result ({}s)",
                timeout.as_secs()
            )));
        }

        std::thread::sleep(poll_interval);
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

    // Try live eval first for expressions/stdin (if a game is running with `gd run --eval`)
    let eval_ready = project.root.join(".godot").join("gd-eval-ready").is_file();
    if eval_ready && mode != InputMode::File {
        let input_text = if mode == InputMode::Stdin {
            let mut buf = String::new();
            std::io::stdin()
                .read_to_string(&mut buf)
                .map_err(|e| miette!("Failed to read stdin: {e}"))?;
            buf
        } else {
            args.input.clone()
        };
        if let Some(result) = try_live_eval(
            &input_text,
            &project.root,
            Duration::from_secs(args.timeout),
            json_mode,
        ) {
            return result;
        }
        // Eval server disappeared between check and attempt — fall through to offline
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
            let mut source = String::new();
            std::io::stdin()
                .read_to_string(&mut source)
                .map_err(|e| miette!("Failed to read stdin: {e}"))?;
            let wrapper = generate_wrapper_script(&source);
            let path = write_temp_script(&project.root, &wrapper)?;
            (path.clone(), Some(path))
        }
        InputMode::Expression => {
            let wrapper = generate_wrapper_script(&args.input);
            let path = write_temp_script(&project.root, &wrapper)?;
            (path.clone(), Some(path))
        }
    };

    // Optional pre-check: parse-validate the script
    if args.check {
        let source = std::fs::read_to_string(&script_path)
            .map_err(|e| miette!("Failed to read {}: {e}", script_path.display()))?;
        if let Err(e) = pre_check(&source) {
            cleanup_temp(temp_file.as_ref());
            return Err(e);
        }
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
    fn try_live_eval_no_server() {
        let tmp = tempfile::tempdir().unwrap();
        // No ready file — should return None
        let result = try_live_eval("1+1", tmp.path(), Duration::from_secs(1), false);
        assert!(result.is_none());
    }

    #[test]
    fn try_live_eval_with_ready_file() {
        let tmp = tempfile::tempdir().unwrap();
        let godot_dir = tmp.path().join(".godot");
        std::fs::create_dir_all(&godot_dir).unwrap();

        // Create ready file
        std::fs::write(godot_dir.join("gd-eval-ready"), "12345").unwrap();

        // Pre-create a result file to simulate the server responding
        std::fs::write(
            godot_dir.join("gd-eval-result.json"),
            r#"{"result":"42","error":""}"#,
        )
        .unwrap();

        let result = try_live_eval("21 * 2", tmp.path(), Duration::from_secs(5), false);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        // Request file should have been written (and server would have consumed it)
        // Result file should have been cleaned up
        assert!(!godot_dir.join("gd-eval-result.json").exists());
    }
}
