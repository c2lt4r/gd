#![allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]

pub mod gdunit;
pub mod gut;
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

use crate::core::config::Config;
use crate::core::project::GodotProject;

// Re-export run_with_timeout for use by gut.rs and gdunit.rs
pub use script::run_with_timeout;

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

#[derive(Debug, Serialize)]
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
    /// Run GDScript tests (GUT, gdUnit4, or raw scripts)
    Run(RunArgs),
}

#[derive(Args)]
#[allow(clippy::struct_excessive_bools)]
pub struct RunArgs {
    /// Paths to test files or directories
    pub paths: Vec<PathBuf>,
    /// Only run tests matching this pattern
    #[arg(short, long)]
    pub filter: Option<String>,
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
    /// Test runner: gut, gdunit4, or script (default: auto-detect)
    #[arg(long, value_parser = parse_runner)]
    pub runner: Option<Runner>,
    /// Extra args to pass to Godot
    #[arg(last = true)]
    pub extra: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Runner {
    Gut,
    GdUnit4,
    Script,
}

fn parse_runner(s: &str) -> std::result::Result<Runner, String> {
    match s.to_lowercase().as_str() {
        "gut" => Ok(Runner::Gut),
        "gdunit4" | "gdunit" => Ok(Runner::GdUnit4),
        "script" => Ok(Runner::Script),
        _ => Err(format!(
            "unknown runner '{s}' (expected: gut, gdunit4, script)"
        )),
    }
}

// --- Utilities ---

/// Print to stdout in human mode, stderr in JSON mode (so stdout stays pure JSON).
macro_rules! hprintln {
    ($json:expr) => {
        if $json { eprintln!(); } else { println!(); }
    };
    ($json:expr, $($arg:tt)*) => {
        if $json { eprintln!($($arg)*); } else { println!($($arg)*); }
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

    let godot = match crate::build::find_godot(&config) {
        Ok(g) => g,
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
            // Auto-detect: GUT > gdUnit4 > script
            if project.root.join("addons/gut").is_dir() {
                Runner::Gut
            } else if project.root.join("addons/gdUnit4").is_dir() {
                Runner::GdUnit4
            } else {
                Runner::Script
            }
        }
    };
    let test_files = match discover_test_files(&project.root, &args.paths) {
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

    if test_files.is_empty() && runner != Runner::GdUnit4 {
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
            println!(
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
                println!("  {}", rel.display().to_string().dimmed());
            }
        }
    }

    let start = Instant::now();

    let mode = runner_label(runner);
    let result = match runner {
        Runner::Gut => {
            hprintln!(json_mode, "{} Running tests with GUT", "▶".green());
            gut::run_gut_tests(&godot, &project, args, &test_files, json_mode)
        }
        Runner::GdUnit4 => {
            hprintln!(json_mode, "{} Running tests with gdUnit4", "▶".green());
            gdunit::run_gdunit4_tests(&godot, &project, args, json_mode)
        }
        Runner::Script => {
            hprintln!(
                json_mode,
                "{} Running tests with script runner",
                "▶".green()
            );
            script::run_script_tests(&godot, &project, args, &test_files, json_mode)
        }
    };

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
                let secs = elapsed.as_secs_f64();
                hprintln!(json_mode);
                let failed_display = if summary.failed > 0 {
                    summary.failed.to_string().red().to_string()
                } else {
                    summary.failed.to_string().green().to_string()
                };
                println!(
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
                println!();
                eprintln!("{} Tests failed ({:.2}s)", "✗".red().bold(), secs);
                Err(e)
            }
        }
    }
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
