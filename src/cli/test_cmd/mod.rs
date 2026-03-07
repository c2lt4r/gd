#![allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]

pub mod gdunit;
pub mod gut;
mod native;
mod script;
#[cfg(test)]
mod tests;

use clap::{Args, Subcommand};
use miette::{Result, miette};
use owo_colors::OwoColorize;
use serde::Serialize;
use std::env;
use std::path::{Path, PathBuf};
use std::time::Instant;

use gd_core::config::Config;
use gd_core::gd_ast;
use gd_core::project::GodotProject;
use gd_core::{ceprintln, cprintln};

// Re-export run_with_timeout for use by gut.rs and gdunit.rs
pub use script::run_with_timeout;

// --- TestRunner trait ---

/// Shared context passed to all runners.
pub struct RunContext<'a> {
    pub godot: Option<&'a Path>,
    pub project: &'a GodotProject,
    pub args: &'a RunArgs,
    pub test_files: &'a [PathBuf],
    pub json_mode: bool,
}

/// Unified interface for test runners.
pub trait TestRunner {
    fn name(&self) -> &'static str;
    fn run(&self, ctx: &RunContext) -> Result<(Vec<TestResult>, TestSummary)>;
}

// --- Data Model ---

#[derive(Debug, Serialize)]
pub struct TestReport {
    pub mode: &'static str,
    pub results: Vec<TestResult>,
    pub summary: TestSummary,
    pub duration_ms: u64,
}

#[derive(Debug, Serialize)]
pub struct TestResult {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    pub status: TestStatus,
    pub duration_ms: u64,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<TestError>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stderr: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stdout: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TestError {
    pub file: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
    pub message: String,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TestStatus {
    Pass,
    Fail,
    Error,
    Timeout,
}

#[derive(Debug, Serialize)]
pub struct TestSummary {
    pub passed: usize,
    pub failed: usize,
    pub errors: usize,
    #[serde(skip_serializing_if = "is_zero")]
    pub skipped: usize,
    pub total: usize,
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_zero(n: &usize) -> bool {
    *n == 0
}

fn runner_label(runner: Runner) -> &'static str {
    match runner {
        Runner::Native => "native",
        Runner::Gut => "gut",
        Runner::GdUnit4 => "gdunit4",
        Runner::Script => "script",
    }
}

// --- CLI Args ---

#[derive(Args)]
pub struct TestArgs {
    #[command(subcommand)]
    pub command: TestCommand,
}

#[derive(Subcommand)]
pub enum TestCommand {
    /// Run GDScript tests (native, GUT, gdUnit4, or raw scripts)
    Run(RunArgs),
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
pub struct RunArgs {
    /// Run tests whose name contains this string
    pub name: Option<String>,
    /// Paths to test files or directories
    #[arg(short = 'p', long)]
    pub path: Vec<PathBuf>,
    /// Only run test files matching this pattern
    #[arg(short, long)]
    pub filter: Option<String>,
    /// Only run tests in this inner class
    #[arg(short, long)]
    pub class: Option<String>,
    /// List matching tests without running them
    #[arg(short, long)]
    pub list: bool,
    /// Export results to JUnit XML file
    #[arg(long)]
    pub junit: Option<PathBuf>,
    /// Show detailed test output
    #[arg(short, long)]
    pub verbose: bool,
    /// Run in headless mode (default: true)
    #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
    pub headless: bool,
    /// Timeout per test in seconds (default: 60)
    #[arg(short, long, default_value_t = 60)]
    pub timeout: u64,
    /// Output format
    #[arg(long, default_value = "text")]
    pub format: String,
    /// Suppress per-test output when all pass (human mode only)
    #[arg(long)]
    pub quiet: bool,
    /// Filter Godot engine noise from output
    #[arg(long)]
    pub clean: bool,
    /// Test runner: native, gut, gdunit4, or script (default: auto-detect)
    #[arg(long, value_parser = parse_runner)]
    pub runner: Option<Runner>,
    /// Extra args to pass to Godot
    #[arg(last = true)]
    pub extra: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Runner {
    Native,
    Gut,
    GdUnit4,
    Script,
}

fn parse_runner(s: &str) -> std::result::Result<Runner, String> {
    match s.to_lowercase().as_str() {
        "native" => Ok(Runner::Native),
        "gut" => Ok(Runner::Gut),
        "gdunit4" | "gdunit" => Ok(Runner::GdUnit4),
        "script" => Ok(Runner::Script),
        _ => Err(format!(
            "unknown runner '{s}' (expected: native, gut, gdunit4, script)"
        )),
    }
}

// --- Utilities ---

/// Print to stdout in human mode, stderr in JSON mode (so stdout stays pure JSON).
/// Respects `--no-color` by routing through `cprintln!`/`ceprintln!`.
macro_rules! hprintln {
    ($json:expr) => {
        if $json { eprintln!(); } else { println!(); }
    };
    ($json:expr, $($arg:tt)*) => {
        if $json { gd_core::ceprintln!($($arg)*); } else { gd_core::cprintln!($($arg)*); }
    };
}

// Make macro available to submodules
pub(crate) use hprintln;

/// Returns true if the line is common Godot engine noise that is not actionable.
pub fn is_engine_noise(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.contains("Orphan StringName")
        || trimmed.contains("ObjectDB instances leaked")
        || trimmed.contains("ObjectDB::cleanup")
        || trimmed.starts_with("WARNING: ObjectDB")
        || trimmed.starts_with("Leaked instance:")
        || trimmed.contains("GDExtension")
        || trimmed.contains("Vulkan")
        || trimmed.contains("vk_")
        || trimmed.contains("MESA")
        || trimmed.starts_with("OpenGL")
        || trimmed.starts_with("GLES")
        || (trimmed.contains("gut_loader.gd") && trimmed.contains("SCRIPT ERROR"))
}

/// Filter engine noise lines from output text.
pub fn filter_noise(text: &str) -> String {
    text.lines()
        .filter(|line| !is_engine_noise(line))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Extract structured errors from Godot stderr output.
/// Parses the pattern:
///   SCRIPT ERROR: \<message\>
///    at: \<function\> (`res://path/file.gd:42`)
pub fn extract_errors(stderr: &str) -> Vec<TestError> {
    let mut errors = Vec::new();
    let lines: Vec<&str> = stderr.lines().collect();

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if let Some(msg) = trimmed.strip_prefix("SCRIPT ERROR:") {
            let message = msg.trim().to_string();

            // Skip GUT's known bug noise
            if i > 0 || !trimmed.contains("gut_loader.gd") {
                // Look ahead for "at:" line with location
                let (file, line_num) = if i + 1 < lines.len() {
                    parse_at_line(lines[i + 1])
                } else {
                    (None, None)
                };

                if let Some(file) = file {
                    errors.push(TestError {
                        file,
                        line: line_num,
                        message,
                    });
                } else if !is_engine_noise(trimmed) {
                    errors.push(TestError {
                        file: String::new(),
                        line: None,
                        message,
                    });
                }
            }
        }
    }
    errors
}

/// Parse a Godot "at:" line like `   at: test_health (res://tests/test_enemy.gd:42)`
/// Returns (`file_path`, `line_number`).
pub fn parse_at_line(line: &str) -> (Option<String>, Option<usize>) {
    let trimmed = line.trim();
    let Some(rest) = trimmed.strip_prefix("at:") else {
        return (None, None);
    };

    // Find the last parenthesized section
    let Some(paren_start) = rest.rfind('(') else {
        return (None, None);
    };
    let Some(paren_end) = rest.rfind(')') else {
        return (None, None);
    };
    if paren_start >= paren_end {
        return (None, None);
    }

    let location = &rest[paren_start + 1..paren_end];
    parse_res_location(location)
}

/// Parse a `res://path/file.gd:42` location string.
pub fn parse_res_location(location: &str) -> (Option<String>, Option<usize>) {
    let path_str = strip_res_prefix(location);

    if let Some(colon_pos) = path_str.rfind(':') {
        let file = path_str[..colon_pos].to_string();
        let line = path_str[colon_pos + 1..].parse::<usize>().ok();
        (Some(file), line)
    } else {
        (Some(path_str.to_string()), None)
    }
}

/// Strip `res://` prefix if present.
pub fn strip_res_prefix(s: &str) -> &str {
    s.strip_prefix("res://").unwrap_or(s)
}

// --- Shared Display Helpers ---

/// Print per-test results in human mode. Used by all runners.
pub fn print_results(results: &[TestResult], args: &RunArgs) {
    for result in results {
        let label = result.file.as_deref().unwrap_or("unknown");
        let show = !args.quiet || result.status != TestStatus::Pass;
        if show {
            match result.status {
                TestStatus::Pass => cprintln!("{} {label}", "✓".green()),
                TestStatus::Fail | TestStatus::Error => {
                    cprintln!("{} {label}", "✗".red());
                    for err in &result.errors {
                        if let Some(ln) = err.line {
                            cprintln!("  {}:{ln} {}", err.file, err.message);
                        } else if !err.file.is_empty() {
                            cprintln!("  {} {}", err.file, err.message);
                        } else {
                            cprintln!("  {}", err.message);
                        }
                    }
                }
                TestStatus::Timeout => cprintln!("{} {label} (timed out)", "✗".red()),
            }
        }
    }
}

/// Group test results by file, returning `(file, passed, failed)` tuples.
/// Preserves insertion order (first seen). Results with `file: None` are grouped as "unknown".
pub fn group_results_by_file(results: &[TestResult]) -> Vec<(String, usize, usize)> {
    let mut groups: Vec<(String, usize, usize)> = Vec::new();

    for result in results {
        // Extract file portion: for "path/test.gd::test_method", use "path/test.gd"
        let raw = result.file.as_deref().unwrap_or("unknown");
        let file_key = raw.split("::").next().unwrap_or(raw).to_string();

        let is_pass = result.status == TestStatus::Pass;

        if let Some(entry) = groups.iter_mut().find(|(f, _, _)| *f == file_key) {
            if is_pass {
                entry.1 += 1;
            } else {
                entry.2 += 1;
            }
        } else {
            groups.push((file_key, usize::from(is_pass), usize::from(!is_pass)));
        }
    }

    groups
}

// --- Shared test content filtering ---

/// Info about test functions and classes found in a single file.
#[derive(Debug)]
pub struct FileTestInfo {
    pub path: PathBuf,
    /// All top-level `test_*` function names in this file.
    pub tests: Vec<String>,
    /// Inner class names that contain matching test functions.
    #[allow(dead_code)] // used by tests; future: gdUnit4 class-level exclusion
    pub classes: Vec<String>,
}

/// Filter test files to those containing matching test functions or classes.
///
/// When `name` is set, only files containing a `test_*` function whose name contains
/// the filter string are included. When `class` is set, only files containing an
/// inner class whose name contains the filter string are included.
///
/// Returns info about each matching file, including which test functions matched.
pub fn filter_files_by_tests(
    test_files: &[PathBuf],
    name: Option<&str>,
    class: Option<&str>,
) -> Vec<FileTestInfo> {
    if name.is_none() && class.is_none() {
        return test_files
            .iter()
            .map(|p| FileTestInfo {
                path: p.clone(),
                tests: Vec::new(),
                classes: Vec::new(),
            })
            .collect();
    }

    let mut parser = tree_sitter::Parser::new();
    if parser
        .set_language(&tree_sitter_gdscript::LANGUAGE.into())
        .is_err()
    {
        return Vec::new();
    }

    let mut result = Vec::new();

    for path in test_files {
        let Ok(source) = std::fs::read_to_string(path) else {
            continue;
        };
        let Some(tree) = parser.parse(&source, None) else {
            continue;
        };
        let gd_file = gd_ast::convert(&tree, &source);

        // Collect matching top-level test functions
        let matching_tests: Vec<String> = gd_file
            .funcs()
            .filter(|f| f.name.starts_with("test_"))
            .filter(|f| name.is_none_or(|n| f.name.contains(n)))
            .map(|f| f.name.to_string())
            .collect();

        // Collect matching inner classes
        let matching_classes: Vec<String> = gd_file
            .inner_classes()
            .filter(|cls| {
                let class_matches = class.is_none_or(|c| cls.name.contains(c));
                let has_tests = cls.declarations.iter().any(|d| {
                    if let gd_ast::GdDecl::Func(f) = d {
                        f.name.starts_with("test_") && name.is_none_or(|n| f.name.contains(n))
                    } else {
                        false
                    }
                });
                class_matches && has_tests
            })
            .map(|cls| cls.name.to_string())
            .collect();

        // Include file if it has any matching tests or classes
        let has_match = if class.is_some() {
            // When filtering by class, only class matches count
            !matching_classes.is_empty()
        } else {
            !matching_tests.is_empty() || !matching_classes.is_empty()
        };

        if has_match {
            // Collect ALL test names in the file (for runners that need exclusion lists)
            let all_tests: Vec<String> = gd_file
                .funcs()
                .filter(|f| f.name.starts_with("test_"))
                .map(|f| f.name.to_string())
                .collect();

            result.push(FileTestInfo {
                path: path.clone(),
                tests: all_tests,
                classes: matching_classes,
            });
        }
    }

    result
}

// --- Main Entry Point ---

#[allow(clippy::too_many_lines)]
pub fn exec(args: &TestArgs) -> Result<()> {
    match &args.command {
        TestCommand::Run(run_args) => exec_run(run_args),
    }
}

#[allow(clippy::too_many_lines)]
fn exec_run(args: &RunArgs) -> Result<()> {
    let json_mode = match args.format.as_str() {
        "text" => false,
        "json" => true,
        _ => {
            // Exit code 2 for infrastructure errors
            eprintln!(
                "Error: invalid format '{}' (expected 'human' or 'json')",
                args.format
            );
            std::process::exit(2);
        }
    };

    let cwd = env::current_dir().unwrap_or_default();

    let config = match Config::load(&cwd) {
        Ok(c) => c,
        Err(e) => {
            if json_mode {
                emit_infra_error_json(&format!("{e}"));
            }
            eprintln!("Error: {e}");
            std::process::exit(2);
        }
    };

    let project = match GodotProject::discover(&cwd) {
        Ok(p) => p,
        Err(e) => {
            if json_mode {
                emit_infra_error_json(&format!("{e}"));
            }
            eprintln!("Error: {e}");
            std::process::exit(2);
        }
    };

    let runner = match args.runner {
        Some(r) => r,
        None => {
            // Auto-detect: GUT > gdUnit4 > native
            if project.root.join("addons/gut").is_dir() {
                Runner::Gut
            } else if project.root.join("addons/gdUnit4").is_dir() {
                Runner::GdUnit4
            } else {
                Runner::Native
            }
        }
    };

    // Native runner doesn't need Godot
    let godot = if runner == Runner::Native {
        None
    } else {
        match crate::build::find_godot(&config) {
            Ok(g) => Some(g),
            Err(e) => {
                if json_mode {
                    emit_infra_error_json(&format!("{e}"));
                }
                eprintln!("Error: {e}");
                std::process::exit(2);
            }
        }
    };
    let test_files = match discover_test_files(&project.root, &args.path) {
        Ok(f) => f,
        Err(e) => {
            if json_mode {
                emit_infra_error_json(&format!("{e}"));
            }
            return Err(e);
        }
    };

    // Apply filter
    let test_files: Vec<PathBuf> = match &args.filter {
        Some(pattern) => test_files
            .into_iter()
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|name| name.contains(pattern.as_str()))
            })
            .collect(),
        None => test_files,
    };

    let gdunit4_no_filters = runner == Runner::GdUnit4
        && args.filter.is_none()
        && args.name.is_none()
        && args.class.is_none();
    if test_files.is_empty() && !gdunit4_no_filters {
        if json_mode {
            let mode = runner_label(runner);
            let report = TestReport {
                mode,
                results: vec![],
                summary: TestSummary {
                    passed: 0,
                    failed: 0,
                    errors: 0,
                    skipped: 0,
                    total: 0,
                },
                duration_ms: 0,
            };
            println!("{}", serde_json::to_string_pretty(&report).unwrap());
        } else {
            cprintln!(
                "{} No test files found{}",
                "!".yellow().bold(),
                args.filter
                    .as_ref()
                    .map(|f| format!(" matching '{f}'"))
                    .unwrap_or_default()
            );
        }
        return Ok(());
    }

    if args.list {
        return list_tests(&test_files, &project.root, args, json_mode);
    }

    if !test_files.is_empty() {
        hprintln!(
            json_mode,
            "{} Found {} test file{}",
            "●".blue(),
            test_files.len(),
            if test_files.len() == 1 { "" } else { "s" }
        );

        if args.verbose && !json_mode {
            for f in &test_files {
                let rel = f.strip_prefix(&project.root).unwrap_or(f);
                cprintln!("  {}", rel.display().to_string().dimmed());
            }
        }
    }

    let start = Instant::now();

    let ctx = RunContext {
        godot: godot.as_deref(),
        project: &project,
        args,
        test_files: &test_files,
        json_mode,
    };
    let runner_impl: Box<dyn TestRunner> = match runner {
        Runner::Native => Box::new(native::NativeRunner),
        Runner::Gut => Box::new(gut::GutRunner),
        Runner::GdUnit4 => Box::new(gdunit::GdUnit4Runner),
        Runner::Script => Box::new(script::ScriptRunner),
    };

    let mode = runner_label(runner);
    hprintln!(
        json_mode,
        "{} Running tests with {}",
        "▶".green(),
        runner_impl.name()
    );
    let result = runner_impl.run(&ctx);

    let elapsed = start.elapsed();
    let duration_ms = elapsed.as_millis() as u64;

    match result {
        Ok((results, summary)) => {
            let has_failures = summary.failed > 0 || summary.errors > 0;

            if json_mode {
                let report = TestReport {
                    mode,
                    results,
                    summary,
                    duration_ms,
                };
                println!("{}", serde_json::to_string_pretty(&report).unwrap());
            } else {
                // Per-file breakdown in --quiet mode (per-test output was suppressed)
                if args.quiet {
                    let file_groups = group_results_by_file(&results);
                    if file_groups.len() > 1 {
                        cprintln!();
                        for (file, p, f) in &file_groups {
                            let icon = if *f > 0 {
                                "✗".red().to_string()
                            } else {
                                "✓".green().to_string()
                            };
                            cprintln!("  {icon} {file}: {p} passed, {f} failed");
                        }
                    }
                }

                let secs = elapsed.as_secs_f64();
                cprintln!();
                let failed_display = if summary.failed > 0 {
                    summary.failed.to_string().red().to_string()
                } else {
                    summary.failed.to_string().green().to_string()
                };
                cprintln!(
                    "{} {} passed, {} failed  ({:.2}s)",
                    "✓".green().bold(),
                    summary.passed.to_string().green(),
                    failed_display,
                    secs,
                );
            }

            if has_failures {
                std::process::exit(1);
            }
            Ok(())
        }
        Err(e) => {
            if json_mode {
                let report = TestReport {
                    mode,
                    results: vec![],
                    summary: TestSummary {
                        passed: 0,
                        failed: 0,
                        errors: 1,
                        skipped: 0,
                        total: 0,
                    },
                    duration_ms,
                };
                println!("{}", serde_json::to_string_pretty(&report).unwrap());
                std::process::exit(1);
            } else {
                let secs = elapsed.as_secs_f64();
                cprintln!();
                ceprintln!("{} Tests failed ({:.2}s)", "✗".red().bold(), secs);
                Err(e)
            }
        }
    }
}

/// A test entry for JSON output of `--list`.
#[derive(Debug, Serialize)]
pub(crate) struct TestListEntry {
    pub file: String,
    pub tests: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub classes: Vec<TestListClass>,
}

/// An inner class with its test functions for `--list` JSON output.
#[derive(Debug, Serialize)]
pub(crate) struct TestListClass {
    pub name: String,
    pub tests: Vec<String>,
}

/// List test functions discovered via tree-sitter parsing.
fn list_tests(
    test_files: &[PathBuf],
    project_root: &Path,
    args: &RunArgs,
    json_mode: bool,
) -> Result<()> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_gdscript::LANGUAGE.into())
        .map_err(|e| miette!("Failed to set parser language: {e}"))?;

    let mut entries = Vec::new();
    let mut total_tests = 0usize;

    for path in test_files {
        let source = std::fs::read_to_string(path)
            .map_err(|e| miette!("Failed to read {}: {e}", path.display()))?;
        let Some(tree) = parser.parse(&source, None) else {
            continue;
        };
        let gd_file = gd_ast::convert(&tree, &source);

        // Collect top-level test functions
        let mut tests: Vec<String> = gd_file
            .funcs()
            .filter(|f| f.name.starts_with("test_"))
            .map(|f| f.name.to_string())
            .collect();

        // Collect inner class test functions
        let mut classes: Vec<TestListClass> = Vec::new();
        for cls in gd_file.inner_classes() {
            if let Some(ref cls_filter) = args.class
                && !cls.name.contains(cls_filter.as_str())
            {
                continue;
            }
            let class_tests: Vec<String> = cls
                .declarations
                .iter()
                .filter_map(|d| {
                    if let gd_ast::GdDecl::Func(f) = d {
                        Some(f)
                    } else {
                        None
                    }
                })
                .filter(|f| f.name.starts_with("test_"))
                .map(|f| f.name.to_string())
                .collect();
            if !class_tests.is_empty() {
                classes.push(TestListClass {
                    name: cls.name.to_string(),
                    tests: class_tests,
                });
            }
        }

        // Apply name filter
        if let Some(ref name_filter) = args.name {
            tests.retain(|t| t.contains(name_filter.as_str()));
            for cls in &mut classes {
                cls.tests.retain(|t| t.contains(name_filter.as_str()));
            }
            classes.retain(|c| !c.tests.is_empty());
        }

        // Apply class filter to skip top-level tests when filtering by class
        let show_top_level = args.class.is_none();

        let file_test_count = if show_top_level { tests.len() } else { 0 }
            + classes.iter().map(|c| c.tests.len()).sum::<usize>();

        if file_test_count == 0 {
            continue;
        }
        total_tests += file_test_count;

        let rel = gd_core::fs::relative_slash(path, project_root);
        if show_top_level {
            entries.push(TestListEntry {
                file: rel,
                tests,
                classes,
            });
        } else {
            entries.push(TestListEntry {
                file: rel,
                tests: vec![],
                classes,
            });
        }
    }

    if json_mode {
        println!("{}", serde_json::to_string_pretty(&entries).unwrap());
    } else {
        for entry in &entries {
            cprintln!("{}", entry.file.bold());
            for t in &entry.tests {
                cprintln!("  {t}");
            }
            for cls in &entry.classes {
                cprintln!("  {}", cls.name.dimmed());
                for t in &cls.tests {
                    cprintln!("    {t}");
                }
            }
        }
        cprintln!(
            "\n{} file{}, {} test{}",
            entries.len(),
            if entries.len() == 1 { "" } else { "s" },
            total_tests,
            if total_tests == 1 { "" } else { "s" },
        );
    }

    Ok(())
}

/// Emit a minimal JSON error report to stdout for infrastructure failures, then let caller exit.
fn emit_infra_error_json(message: &str) {
    let report = TestReport {
        mode: "unknown",
        results: vec![TestResult {
            file: None,
            status: TestStatus::Error,
            duration_ms: 0,
            errors: vec![TestError {
                file: String::new(),
                line: None,
                message: message.to_string(),
            }],
            stderr: None,
            stdout: None,
        }],
        summary: TestSummary {
            passed: 0,
            failed: 0,
            errors: 1,
            skipped: 0,
            total: 0,
        },
        duration_ms: 0,
    };
    println!("{}", serde_json::to_string_pretty(&report).unwrap());
}

/// Discover test files in the project.
fn discover_test_files(project_root: &Path, paths: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let search_dirs: Vec<PathBuf> = if paths.is_empty() {
        // Default: look in test/ and tests/ directories
        ["test", "tests"]
            .iter()
            .map(|d| project_root.join(d))
            .filter(|d| d.is_dir())
            .collect()
    } else {
        paths
            .iter()
            .map(|p| {
                if p.is_absolute() {
                    p.clone()
                } else {
                    project_root.join(p)
                }
            })
            .collect()
    };

    let mut test_files = Vec::new();
    for dir in &search_dirs {
        if dir.is_file() {
            // Direct file path given
            test_files.push(dir.clone());
            continue;
        }
        if !dir.is_dir() {
            continue;
        }
        collect_test_files(dir, &mut test_files)?;
    }

    test_files.sort();
    Ok(test_files)
}

/// Recursively collect test files matching test_*.gd or *_test.gd.
fn collect_test_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    let entries = std::fs::read_dir(dir)
        .map_err(|e| miette!("Failed to read directory {}: {e}", dir.display()))?;

    for entry in entries {
        let entry = entry.map_err(|e| miette!("Failed to read entry: {e}"))?;
        let path = entry.path();
        if path.is_dir() {
            collect_test_files(&path, out)?;
        } else if is_test_file(&path) {
            out.push(path);
        }
    }
    Ok(())
}

/// Check if a file is a test file (test_*.gd or *_test.gd).
pub fn is_test_file(path: &Path) -> bool {
    let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
        return false;
    };
    path.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("gd"))
        && (stem.starts_with("test_") || stem.ends_with("_test"))
}
