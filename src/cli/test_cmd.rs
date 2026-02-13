use clap::Args;
use miette::{Result, miette};
use owo_colors::OwoColorize;
use serde::Serialize;
use std::env;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::core::config::Config;
use crate::core::project::GodotProject;

// --- Data Model ---

#[derive(Debug, Serialize)]
struct TestReport {
    mode: &'static str,
    results: Vec<TestResult>,
    summary: TestSummary,
    duration_ms: u64,
}

#[derive(Debug, Serialize)]
struct TestResult {
    #[serde(skip_serializing_if = "Option::is_none")]
    file: Option<String>,
    status: TestStatus,
    duration_ms: u64,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    errors: Vec<TestError>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stderr: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stdout: Option<String>,
}

#[derive(Debug, Serialize)]
struct TestError {
    file: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    line: Option<usize>,
    message: String,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum TestStatus {
    Pass,
    Fail,
    Error,
    Timeout,
}

#[derive(Debug, Serialize)]
struct TestSummary {
    passed: usize,
    failed: usize,
    errors: usize,
    #[serde(skip_serializing_if = "is_zero")]
    skipped: usize,
    total: usize,
}

fn is_zero(n: &usize) -> bool {
    *n == 0
}

// --- CLI Args ---

#[derive(Args)]
pub struct TestArgs {
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
    #[arg(long, default_value = "human")]
    pub format: String,
    /// Suppress per-test output when all pass (human mode only)
    #[arg(long)]
    pub quiet: bool,
    /// Filter Godot engine noise from output
    #[arg(long)]
    pub clean: bool,
    /// Extra args to pass to Godot
    #[arg(last = true)]
    pub extra: Vec<String>,
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

/// Returns true if the line is common Godot engine noise that is not actionable.
fn is_engine_noise(line: &str) -> bool {
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
fn filter_noise(text: &str) -> String {
    text.lines()
        .filter(|line| !is_engine_noise(line))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Extract structured errors from Godot stderr output.
/// Parses the pattern:
///   SCRIPT ERROR: <message>
///    at: <function> (res://path/file.gd:42)
fn extract_errors(stderr: &str) -> Vec<TestError> {
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
/// Returns (file_path, line_number).
fn parse_at_line(line: &str) -> (Option<String>, Option<usize>) {
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
fn parse_res_location(location: &str) -> (Option<String>, Option<usize>) {
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
fn strip_res_prefix(s: &str) -> &str {
    s.strip_prefix("res://").unwrap_or(s)
}

// --- Main Entry Point ---

pub fn exec(args: TestArgs) -> Result<()> {
    let json_mode = match args.format.as_str() {
        "human" => false,
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

    let has_gut = project.root.join("addons/gut").is_dir();
    let has_gdunit4 = !has_gut && project.root.join("addons/gdUnit4").is_dir();
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

    if test_files.is_empty() && !has_gdunit4 {
        if json_mode {
            let mode = if has_gut { "gut" } else { "script" };
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

    let (mode, result) = if has_gut {
        hprintln!(json_mode, "{} Running tests with GUT", "▶".green());
        (
            "gut",
            run_gut_tests(&godot, &project, &args, &test_files, json_mode),
        )
    } else if has_gdunit4 {
        hprintln!(json_mode, "{} Running tests with gdUnit4", "▶".green());
        (
            "gdunit4",
            run_gdunit4_tests(&godot, &project, &args, json_mode),
        )
    } else {
        hprintln!(
            json_mode,
            "{} Running tests with Godot (no GUT addon)",
            "▶".green()
        );
        (
            "script",
            run_script_tests(&godot, &project, &args, &test_files, json_mode),
        )
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
fn is_test_file(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
        return false;
    };
    name.ends_with(".gd") && (stem.starts_with("test_") || stem.ends_with("_test"))
}

/// Run tests using GUT addon.
fn run_gut_tests(
    godot: &Path,
    project: &GodotProject,
    args: &TestArgs,
    test_files: &[PathBuf],
    json_mode: bool,
) -> Result<(Vec<TestResult>, TestSummary)> {
    let spinner = if !json_mode {
        let sp = indicatif::ProgressBar::new_spinner();
        sp.set_style(
            indicatif::ProgressStyle::default_spinner()
                .template("{spinner:.green} {msg}")
                .expect("invalid spinner template"),
        );
        sp.set_message("Running GUT tests...");
        sp.enable_steady_tick(std::time::Duration::from_millis(100));
        Some(sp)
    } else {
        None
    };

    let mut cmd = Command::new(godot);
    if args.headless {
        cmd.arg("--headless");
    }
    cmd.arg("--path")
        .arg(&project.root)
        .arg("-s")
        .arg("addons/gut/gut_cmdln.gd")
        .arg("-gexit");

    // If no .gutconfig.json exists, tell GUT where to find tests
    if !project.root.join(".gutconfig.json").exists() {
        // Collect unique parent directories from discovered test files
        let mut gut_dirs: Vec<String> = Vec::new();
        for file in test_files {
            if let Some(parent) = file.parent() {
                let rel = crate::core::fs::relative_slash(parent, &project.root);
                let dir_str = format!("res://{rel}");
                if !gut_dirs.contains(&dir_str) {
                    gut_dirs.push(dir_str);
                }
            }
        }
        for dir in &gut_dirs {
            cmd.arg(format!("-gdir={dir}"));
        }
        cmd.arg("-ginclude_subdirs");
    }

    if let Some(ref filter) = args.filter {
        cmd.arg(format!("-gselect={filter}"));
    }

    // Extra args from CLI (after --)
    for arg in &args.extra {
        cmd.arg(arg);
    }

    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

    let test_start = Instant::now();

    // GUT handles its own exit with -gexit, don't kill on stderr errors
    let output = run_with_timeout(&mut cmd, Duration::from_secs(args.timeout), false)?;

    let test_duration_ms = test_start.elapsed().as_millis() as u64;

    if let Some(sp) = spinner {
        sp.finish_and_clear();
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if !json_mode && args.verbose {
        if !stdout.is_empty() {
            let display_stdout = if args.clean {
                filter_noise(&stdout)
            } else {
                stdout.to_string()
            };
            if !display_stdout.is_empty() {
                println!("{display_stdout}");
            }
        }
        if !stderr.is_empty() {
            let display_stderr = if args.clean {
                filter_noise(&stderr)
            } else {
                stderr.to_string()
            };
            if !display_stderr.is_empty() {
                eprintln!("{display_stderr}");
            }
        }
    }

    // Parse GUT output for pass/fail counts
    let (gut_passed, gut_failed) = parse_gut_counts(&stdout);
    let parsed_ok = gut_passed > 0 || gut_failed > 0;

    if !output.status.success() && !parsed_ok {
        // GUT didn't produce parseable output; treat as full failure
        if !json_mode && !args.verbose {
            // Show output since we didn't already
            if !stdout.is_empty() {
                println!("{stdout}");
            }
            if !stderr.is_empty() {
                eprintln!("{stderr}");
            }
        }
        return Err(miette!("GUT exited with non-zero status"));
    }

    let errors = extract_errors(&stderr);
    let error_count = errors.len();

    let status = if gut_failed > 0 || error_count > 0 {
        TestStatus::Fail
    } else {
        TestStatus::Pass
    };

    let result = TestResult {
        file: None,
        status,
        duration_ms: test_duration_ms,
        errors,
        stderr: if json_mode && !stderr.is_empty() {
            Some(stderr.into_owned())
        } else {
            None
        },
        stdout: if json_mode && !stdout.is_empty() {
            Some(stdout.into_owned())
        } else {
            None
        },
    };

    let total = gut_passed + gut_failed;
    let summary = TestSummary {
        passed: gut_passed,
        failed: gut_failed,
        errors: error_count,
        skipped: 0,
        total,
    };

    Ok((vec![result], summary))
}

/// Run tests using gdUnit4 framework.
fn run_gdunit4_tests(
    godot: &Path,
    project: &GodotProject,
    args: &TestArgs,
    json_mode: bool,
) -> Result<(Vec<TestResult>, TestSummary)> {
    if args.filter.is_some() {
        hprintln!(
            json_mode,
            "{} --filter is not supported with gdUnit4; use -- -i SuiteName to exclude tests",
            "!".yellow().bold()
        );
    }

    let spinner = if !json_mode {
        let sp = indicatif::ProgressBar::new_spinner();
        sp.set_style(
            indicatif::ProgressStyle::default_spinner()
                .template("{spinner:.green} {msg}")
                .expect("invalid spinner template"),
        );
        sp.set_message("Running gdUnit4 tests...");
        sp.enable_steady_tick(std::time::Duration::from_millis(100));
        Some(sp)
    } else {
        None
    };

    // Determine test directories for -a flags
    let test_dirs: Vec<String> = if args.paths.is_empty() {
        ["test", "tests"]
            .iter()
            .filter(|d| project.root.join(d).is_dir())
            .map(|d| format!("res://{d}"))
            .collect()
    } else {
        let mut dirs = Vec::new();
        for p in &args.paths {
            let abs = if p.is_absolute() {
                p.clone()
            } else {
                project.root.join(p)
            };
            let dir = if abs.is_file() {
                abs.parent().unwrap_or(&abs).to_path_buf()
            } else {
                abs
            };
            let rel = crate::core::fs::relative_slash(&dir, &project.root);
            let entry = format!("res://{rel}");
            if !dirs.contains(&entry) {
                dirs.push(entry);
            }
        }
        dirs
    };

    let mut cmd = Command::new(godot);
    if args.headless {
        cmd.arg("--headless");
    }
    cmd.arg("--path")
        .arg(&project.root)
        .arg("-s")
        .arg("addons/gdUnit4/bin/GdUnitCmdTool.gd")
        .arg("-c");

    // gdUnit4 v6+ blocks headless mode by default; auto-bypass when headless
    if args.headless {
        cmd.arg("--ignoreHeadlessMode");
    }

    for dir in &test_dirs {
        cmd.arg("-a").arg(dir);
    }

    // Report output directory (temp location, cleaned up after parsing)
    let report_dir = project.root.join(".gd-test-reports");
    cmd.arg("-rd").arg("res://.gd-test-reports/");

    for arg in &args.extra {
        cmd.arg(arg);
    }

    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

    let test_start = Instant::now();
    // gdUnit4 handles its own exit, don't kill on stderr errors
    let output = run_with_timeout(&mut cmd, Duration::from_secs(args.timeout), false)?;
    let test_duration_ms = test_start.elapsed().as_millis() as u64;

    if let Some(sp) = spinner {
        sp.finish_and_clear();
    }

    let stdout_str = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr_str = String::from_utf8_lossy(&output.stderr).into_owned();

    if !json_mode && args.verbose {
        let out = if args.clean {
            filter_noise(&stdout_str)
        } else {
            stdout_str.clone()
        };
        if !out.is_empty() {
            println!("{out}");
        }
        let err = if args.clean {
            filter_noise(&stderr_str)
        } else {
            stderr_str.clone()
        };
        if !err.is_empty() {
            eprintln!("{err}");
        }
    }

    // Parse JUnit XML results (gdUnit4 writes to report_N/ subdirectories)
    let xml_path = find_results_xml(&report_dir);
    let (results, summary) = if let Some(ref xml_path) = xml_path {
        let xml = std::fs::read_to_string(xml_path).unwrap_or_default();
        parse_gdunit4_xml(&xml)
    } else {
        // No XML — fall back to exit code
        let exit_code = output.status.code().unwrap_or(1);
        let errors = extract_errors(&stderr_str);
        let ok = (exit_code == 0 || exit_code == 101) && errors.is_empty();
        let status = if ok {
            TestStatus::Pass
        } else {
            TestStatus::Fail
        };
        let (p, f) = if status == TestStatus::Pass {
            (1, 0)
        } else {
            (0, 1)
        };
        (
            vec![TestResult {
                file: None,
                status,
                duration_ms: test_duration_ms,
                errors,
                stderr: if json_mode && !stderr_str.is_empty() {
                    Some(stderr_str.clone())
                } else {
                    None
                },
                stdout: if json_mode && !stdout_str.is_empty() {
                    Some(stdout_str.clone())
                } else {
                    None
                },
            }],
            TestSummary {
                passed: p,
                failed: f,
                errors: 0,
                skipped: 0,
                total: 1,
            },
        )
    };

    // Clean up temp report directory
    if report_dir.is_dir() {
        let _ = std::fs::remove_dir_all(&report_dir);
    }

    // Non-zero exit with no parseable results
    let exit_code = output.status.code().unwrap_or(1);
    if exit_code != 0 && exit_code != 101 && summary.total == 0 {
        if !json_mode && !args.verbose {
            if !stdout_str.is_empty() {
                println!("{stdout_str}");
            }
            if !stderr_str.is_empty() {
                eprintln!("{stderr_str}");
            }
        }
        return Err(miette!("gdUnit4 exited with status {exit_code}"));
    }

    // Print per-test results in human mode
    if !json_mode {
        for result in &results {
            let label = result.file.as_deref().unwrap_or("unknown");
            let show = !args.quiet || result.status != TestStatus::Pass;
            if show {
                match result.status {
                    TestStatus::Pass => println!("{} {label}", "✓".green()),
                    TestStatus::Fail | TestStatus::Error => {
                        println!("{} {label}", "✗".red());
                        for err in &result.errors {
                            if let Some(ln) = err.line {
                                println!("  {}:{ln} {}", err.file, err.message);
                            } else if !err.file.is_empty() {
                                println!("  {} {}", err.file, err.message);
                            } else {
                                println!("  {}", err.message);
                            }
                        }
                    }
                    TestStatus::Timeout => {}
                }
            }
        }
    }

    Ok((results, summary))
}

/// Run tests by executing each test script individually with Godot.
fn run_script_tests(
    godot: &Path,
    project: &GodotProject,
    args: &TestArgs,
    test_files: &[PathBuf],
    json_mode: bool,
) -> Result<(Vec<TestResult>, TestSummary)> {
    let mut passed = 0usize;
    let mut failed = 0usize;
    let mut error_count = 0usize;
    let mut results = Vec::new();

    for (i, test_file) in test_files.iter().enumerate() {
        let rel = crate::core::fs::relative_slash(test_file, &project.root);
        let label = format!("[{}/{}] {rel}", i + 1, test_files.len());

        let spinner = if !json_mode && !args.quiet {
            let sp = indicatif::ProgressBar::new_spinner();
            sp.set_style(
                indicatif::ProgressStyle::default_spinner()
                    .template("{spinner:.green} {msg}")
                    .expect("invalid spinner template"),
            );
            sp.set_message(label.clone());
            sp.enable_steady_tick(std::time::Duration::from_millis(100));
            Some(sp)
        } else {
            None
        };

        let mut cmd = Command::new(godot);
        if args.headless {
            cmd.arg("--headless");
        }
        cmd.arg("--path")
            .arg(&project.root)
            .arg("--script")
            .arg(test_file);

        // Extra args from CLI (after --)
        for arg in &args.extra {
            cmd.arg(arg);
        }

        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

        let test_start = Instant::now();

        // Kill early on script errors (Godot hangs on assert failure in --script mode)
        let output = match run_with_timeout(&mut cmd, Duration::from_secs(args.timeout), true) {
            Ok(output) => output,
            Err(_) => {
                if let Some(sp) = spinner {
                    sp.finish_and_clear();
                }
                failed += 1;
                let test_duration_ms = test_start.elapsed().as_millis() as u64;

                if !json_mode && !args.quiet {
                    println!("{} {rel} (timed out after {}s)", "✗".red(), args.timeout);
                }

                results.push(TestResult {
                    file: Some(rel.clone()),
                    status: TestStatus::Timeout,
                    duration_ms: test_duration_ms,
                    errors: vec![],
                    stderr: None,
                    stdout: None,
                });
                continue;
            }
        };

        if let Some(sp) = spinner {
            sp.finish_and_clear();
        }

        let test_duration_ms = test_start.elapsed().as_millis() as u64;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        let errors = extract_errors(&stderr);
        let file_error_count = errors.len();

        let status = if output.status.success() {
            passed += 1;
            TestStatus::Pass
        } else if file_error_count > 0 {
            failed += 1;
            error_count += file_error_count;
            TestStatus::Fail
        } else {
            failed += 1;
            TestStatus::Error
        };

        // Human output
        if !json_mode {
            let show_result = !args.quiet || status != TestStatus::Pass;
            if show_result {
                match status {
                    TestStatus::Pass => println!("{} {rel}", "✓".green()),
                    TestStatus::Fail | TestStatus::Error => {
                        println!("{} {rel}", "✗".red());
                        // Show parsed error locations inline
                        for err in &errors {
                            if let Some(line_num) = err.line {
                                println!("  {}:{line_num} {}", err.file, err.message);
                            } else if !err.file.is_empty() {
                                println!("  {} {}", err.file, err.message);
                            } else {
                                println!("  {}", err.message);
                            }
                        }
                    }
                    TestStatus::Timeout => {} // already handled above
                }
            }

            if args.verbose || (status != TestStatus::Pass && errors.is_empty()) {
                let display_stdout = if args.clean {
                    filter_noise(&stdout)
                } else {
                    stdout.to_string()
                };
                let display_stderr = if args.clean {
                    filter_noise(&stderr)
                } else {
                    stderr.to_string()
                };

                if !display_stdout.is_empty() {
                    for line in display_stdout.lines() {
                        println!("  {line}");
                    }
                }
                if !display_stderr.is_empty() {
                    for line in display_stderr.lines() {
                        eprintln!("  {}", line.dimmed());
                    }
                }
            }
        }

        results.push(TestResult {
            file: Some(rel),
            status,
            duration_ms: test_duration_ms,
            errors,
            stderr: if json_mode && !stderr.is_empty() {
                Some(stderr.into_owned())
            } else {
                None
            },
            stdout: if json_mode && !stdout.is_empty() {
                Some(stdout.into_owned())
            } else {
                None
            },
        });
    }

    let total = passed + failed;
    let summary = TestSummary {
        passed,
        failed,
        errors: error_count,
        skipped: 0,
        total,
    };

    Ok((results, summary))
}

/// Run a command with a timeout, killing the process if it exceeds the limit.
/// When `kill_on_error` is true, monitors stderr for script errors/assertion failures
/// and kills early (used for raw --script tests where Godot hangs on assert failure).
fn run_with_timeout(
    cmd: &mut Command,
    timeout: Duration,
    kill_on_error: bool,
) -> Result<std::process::Output> {
    use std::io::Read;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };

    let mut child = cmd
        .spawn()
        .map_err(|e| miette!("Failed to start Godot: {e}"))?;

    let hit_error = Arc::new(AtomicBool::new(false));

    // Read stdout in a background thread
    let stdout_handle = child.stdout.take();
    let stdout_thread = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(mut stdout) = stdout_handle {
            let _ = stdout.read_to_end(&mut buf);
        }
        buf
    });

    // Read stderr in a background thread. When kill_on_error is true, watches
    // for script errors (Godot writes SCRIPT ERROR to stderr on assert failure).
    let stderr_handle = child.stderr.take();
    let hit_error_stderr = Arc::clone(&hit_error);
    let stderr_thread = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(mut stderr) = stderr_handle {
            if kill_on_error {
                let mut chunk = [0u8; 4096];
                loop {
                    match stderr.read(&mut chunk) {
                        Ok(0) => break,
                        Ok(n) => {
                            buf.extend_from_slice(&chunk[..n]);
                            if !hit_error_stderr.load(Ordering::Relaxed) {
                                let text = String::from_utf8_lossy(&buf);
                                if text.contains("SCRIPT ERROR")
                                    || text.contains("Assertion failed")
                                {
                                    hit_error_stderr.store(true, Ordering::Relaxed);
                                }
                            }
                        }
                        Err(_) => break,
                    }
                }
            } else {
                let _ = stderr.read_to_end(&mut buf);
            }
        }
        buf
    });

    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let stdout_buf = stdout_thread.join().unwrap_or_default();
                let stderr_buf = stderr_thread.join().unwrap_or_default();
                return Ok(std::process::Output {
                    status,
                    stdout: stdout_buf,
                    stderr: stderr_buf,
                });
            }
            Ok(None) => {
                if start.elapsed() >= timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(miette!("Test timed out after {}s", timeout.as_secs()));
                }
                // If a script error was detected, give Godot a moment then kill
                if hit_error.load(Ordering::Relaxed) {
                    std::thread::sleep(Duration::from_millis(500));
                    if child.try_wait().ok().flatten().is_none() {
                        let _ = child.kill();
                    }
                    let status = child.wait().map_err(|e| miette!("Failed to wait: {e}"))?;
                    let stdout_buf = stdout_thread.join().unwrap_or_default();
                    let stderr_buf = stderr_thread.join().unwrap_or_default();
                    return Ok(std::process::Output {
                        status,
                        stdout: stdout_buf,
                        stderr: stderr_buf,
                    });
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(e) => return Err(miette!("Failed waiting for Godot process: {e}")),
        }
    }
}

/// Parse GUT command-line output for pass/fail counts.
/// Returns (passed, failed). Both are 0 if parsing failed.
fn parse_gut_counts(output: &str) -> (usize, usize) {
    let mut passed = 0usize;
    let mut failed = 0usize;
    let mut found = false;

    for line in output.lines() {
        let trimmed = line.trim();

        // GUT 9.x "Run Summary" totals section:
        //   Passing Tests         3
        //   Failing Tests         1
        if trimmed.starts_with("Passing Tests") {
            if let Some(n) = trimmed
                .split_whitespace()
                .last()
                .and_then(|s| s.parse().ok())
            {
                passed = n;
                found = true;
            }
        } else if trimmed.starts_with("Failing Tests") {
            if let Some(n) = trimmed
                .split_whitespace()
                .last()
                .and_then(|s| s.parse().ok())
            {
                failed = n;
                found = true;
            }
        }
        // Older GUT format: "Passed: 5 Failed: 2"
        else if trimmed.contains("Passed:") && trimmed.contains("Failed:") {
            for part in trimmed.split_whitespace().collect::<Vec<_>>().windows(2) {
                if part[0] == "Passed:"
                    && let Ok(n) = part[1].parse::<usize>()
                {
                    passed = n;
                    found = true;
                }
                if part[0] == "Failed:"
                    && let Ok(n) = part[1].parse::<usize>()
                {
                    failed = n;
                    found = true;
                }
            }
        }
    }

    if found { (passed, failed) } else { (0, 0) }
}

/// Find the results.xml file in the gdUnit4 report directory.
/// gdUnit4 writes to subdirectories like `report_1/results.xml`.
fn find_results_xml(report_dir: &Path) -> Option<PathBuf> {
    // Check direct path first
    let direct = report_dir.join("results.xml");
    if direct.is_file() {
        return Some(direct);
    }
    // Search subdirectories (report_1/, report_2/, etc.)
    let entries = std::fs::read_dir(report_dir).ok()?;
    for entry in entries.flatten() {
        if entry.path().is_dir() {
            let candidate = entry.path().join("results.xml");
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

/// Parse gdUnit4 JUnit XML results into test results and summary.
fn parse_gdunit4_xml(xml: &str) -> (Vec<TestResult>, TestSummary) {
    let mut results = Vec::new();
    let mut passed = 0usize;
    let mut failed = 0usize;
    let mut skipped = 0usize;

    // Split on <testcase to isolate each test case block
    let parts: Vec<&str> = xml.split("<testcase ").collect();

    for part in parts.iter().skip(1) {
        let name = extract_xml_attr(part, "name");
        let classname = extract_xml_attr(part, "classname");
        let time_ms = extract_xml_attr(part, "time")
            .and_then(|t| t.parse::<f64>().ok())
            .map(|t| (t * 1000.0) as u64)
            .unwrap_or(0);

        // Only look up to the end of this testcase
        let block = part.split("</testcase>").next().unwrap_or(part);

        if block.contains("<skipped") {
            skipped += 1;
            continue;
        }

        let (status, errors) = if let Some(fail_start) = block.find("<failure") {
            failed += 1;
            let fail_block = &block[fail_start..];
            let msg = extract_xml_attr(fail_block, "message")
                .map(|m| decode_xml_entities(&m))
                .unwrap_or_default();
            let file = classname
                .as_deref()
                .map(strip_res_prefix)
                .unwrap_or_default()
                .to_string();
            (
                TestStatus::Fail,
                vec![TestError {
                    file,
                    line: None,
                    message: msg,
                }],
            )
        } else if let Some(err_start) = block.find("<error") {
            failed += 1;
            let err_block = &block[err_start..];
            let msg = extract_xml_attr(err_block, "message")
                .map(|m| decode_xml_entities(&m))
                .unwrap_or_default();
            let file = classname
                .as_deref()
                .map(strip_res_prefix)
                .unwrap_or_default()
                .to_string();
            (
                TestStatus::Fail,
                vec![TestError {
                    file,
                    line: None,
                    message: msg,
                }],
            )
        } else {
            passed += 1;
            (TestStatus::Pass, vec![])
        };

        let label = match (&classname, &name) {
            (Some(cls), Some(n)) => {
                let cls_clean = strip_res_prefix(cls);
                format!("{cls_clean}::{n}")
            }
            (None, Some(n)) => n.clone(),
            (Some(cls), None) => strip_res_prefix(cls).to_string(),
            (None, None) => "unknown".to_string(),
        };

        results.push(TestResult {
            file: Some(label),
            status,
            duration_ms: time_ms,
            errors,
            stderr: None,
            stdout: None,
        });
    }

    let total = passed + failed;
    let summary = TestSummary {
        passed,
        failed,
        errors: 0,
        skipped,
        total,
    };

    (results, summary)
}

/// Extract the value of an XML attribute from text.
/// Finds `attr="value"` and returns the value.
fn extract_xml_attr(text: &str, attr: &str) -> Option<String> {
    for quote in ['"', '\''] {
        let pattern = format!("{attr}={quote}");
        if let Some(start_pos) = text.find(&pattern) {
            let value_start = start_pos + pattern.len();
            if let Some(end_offset) = text[value_start..].find(quote) {
                return Some(text[value_start..value_start + end_offset].to_string());
            }
        }
    }
    None
}

/// Decode common XML entities in a string.
fn decode_xml_entities(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_test_file() {
        assert!(is_test_file(Path::new("test_player.gd")));
        assert!(is_test_file(Path::new("enemy_test.gd")));
        assert!(is_test_file(Path::new("tests/test_health.gd")));
        assert!(!is_test_file(Path::new("player.gd")));
        assert!(!is_test_file(Path::new("test_player.tscn")));
    }

    #[test]
    fn test_parse_gut_counts_9x() {
        let output = r#"
--- Run Summary ---
Passing Tests         3
Failing Tests         1
"#;
        assert_eq!(parse_gut_counts(output), (3, 1));
    }

    #[test]
    fn test_parse_gut_counts_old() {
        let output = "Passed: 5 Failed: 2";
        assert_eq!(parse_gut_counts(output), (5, 2));
    }

    #[test]
    fn test_parse_gut_counts_no_failures() {
        let output = r#"
--- Run Summary ---
Passing Tests         10
"#;
        assert_eq!(parse_gut_counts(output), (10, 0));
    }

    #[test]
    fn test_parse_gut_counts_unparseable() {
        assert_eq!(parse_gut_counts("no useful output"), (0, 0));
    }

    #[test]
    fn test_is_engine_noise() {
        assert!(is_engine_noise(
            "WARNING: ObjectDB instances leaked at exit"
        ));
        assert!(is_engine_noise("  Orphan StringName: @icon"));
        assert!(is_engine_noise("Vulkan: vkCreateInstance failed"));
        assert!(is_engine_noise("GLES3: shader compilation error"));
        assert!(is_engine_noise("SCRIPT ERROR: gut_loader.gd:35 something"));
        assert!(!is_engine_noise("SCRIPT ERROR: Assertion failed."));
        assert!(!is_engine_noise("my normal output line"));
    }

    #[test]
    fn test_filter_noise() {
        let input = "line one\nOrphan StringName: @icon\nline two\nVulkan init\nline three";
        let filtered = filter_noise(input);
        assert_eq!(filtered, "line one\nline two\nline three");
    }

    #[test]
    fn test_extract_errors_script_error() {
        let stderr = "\
SCRIPT ERROR: Assertion failed.
   at: test_health (res://tests/test_enemy.gd:42)
";
        let errors = extract_errors(stderr);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].file, "tests/test_enemy.gd");
        assert_eq!(errors[0].line, Some(42));
        assert_eq!(errors[0].message, "Assertion failed.");
    }

    #[test]
    fn test_extract_errors_multiple() {
        let stderr = "\
SCRIPT ERROR: First error
   at: func_a (res://tests/test_a.gd:10)
some other output
SCRIPT ERROR: Second error
   at: func_b (res://tests/test_b.gd:20)
";
        let errors = extract_errors(stderr);
        assert_eq!(errors.len(), 2);
        assert_eq!(errors[0].file, "tests/test_a.gd");
        assert_eq!(errors[0].line, Some(10));
        assert_eq!(errors[1].file, "tests/test_b.gd");
        assert_eq!(errors[1].line, Some(20));
    }

    #[test]
    fn test_extract_errors_no_line() {
        let stderr = "\
SCRIPT ERROR: Something went wrong
   at: some_func (res://scripts/main.gd)
";
        let errors = extract_errors(stderr);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].file, "scripts/main.gd");
        assert_eq!(errors[0].line, None);
    }

    #[test]
    fn test_extract_errors_empty() {
        assert!(extract_errors("").is_empty());
        assert!(extract_errors("normal output\nno errors here").is_empty());
    }

    #[test]
    fn test_parse_res_location() {
        assert_eq!(
            parse_res_location("res://tests/test_player.gd:42"),
            (Some("tests/test_player.gd".to_string()), Some(42))
        );
        assert_eq!(
            parse_res_location("res://tests/test_player.gd"),
            (Some("tests/test_player.gd".to_string()), None)
        );
        assert_eq!(
            parse_res_location("scripts/main.gd:10"),
            (Some("scripts/main.gd".to_string()), Some(10))
        );
    }

    #[test]
    fn test_strip_res_prefix() {
        assert_eq!(strip_res_prefix("res://tests/test.gd"), "tests/test.gd");
        assert_eq!(strip_res_prefix("tests/test.gd"), "tests/test.gd");
    }

    #[test]
    fn test_status_serialization() {
        assert_eq!(
            serde_json::to_string(&TestStatus::Pass).unwrap(),
            "\"pass\""
        );
        assert_eq!(
            serde_json::to_string(&TestStatus::Fail).unwrap(),
            "\"fail\""
        );
        assert_eq!(
            serde_json::to_string(&TestStatus::Error).unwrap(),
            "\"error\""
        );
        assert_eq!(
            serde_json::to_string(&TestStatus::Timeout).unwrap(),
            "\"timeout\""
        );
    }

    #[test]
    fn test_report_serialization() {
        let report = TestReport {
            mode: "script",
            results: vec![
                TestResult {
                    file: Some("tests/test_player.gd".to_string()),
                    status: TestStatus::Pass,
                    duration_ms: 1234,
                    errors: vec![],
                    stderr: None,
                    stdout: None,
                },
                TestResult {
                    file: Some("tests/test_enemy.gd".to_string()),
                    status: TestStatus::Fail,
                    duration_ms: 567,
                    errors: vec![TestError {
                        file: "tests/test_enemy.gd".to_string(),
                        line: Some(42),
                        message: "Assertion failed.".to_string(),
                    }],
                    stderr: Some("SCRIPT ERROR: Assertion failed.\n".to_string()),
                    stdout: None,
                },
            ],
            summary: TestSummary {
                passed: 1,
                failed: 1,
                errors: 1,
                skipped: 0,
                total: 2,
            },
            duration_ms: 1801,
        };
        let json = serde_json::to_string_pretty(&report).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["mode"], "script");
        assert_eq!(parsed["results"].as_array().unwrap().len(), 2);
        assert_eq!(parsed["results"][0]["status"], "pass");
        assert_eq!(parsed["results"][1]["status"], "fail");
        assert_eq!(parsed["results"][1]["errors"][0]["line"], 42);
        assert_eq!(parsed["summary"]["passed"], 1);
        assert_eq!(parsed["summary"]["failed"], 1);
        assert_eq!(parsed["summary"]["total"], 2);
        assert_eq!(parsed["duration_ms"], 1801);

        // Verify skip_serializing_if works: passing test has no errors/stderr/stdout keys
        assert!(parsed["results"][0].get("errors").is_none());
        assert!(parsed["results"][0].get("stderr").is_none());
        assert!(parsed["results"][0].get("stdout").is_none());
    }

    #[test]
    fn test_report_empty() {
        let report = TestReport {
            mode: "script",
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
        let json = serde_json::to_string(&report).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["results"].as_array().unwrap().len(), 0);
        assert_eq!(parsed["summary"]["total"], 0);
    }

    // --- gdUnit4 XML parsing tests ---

    #[test]
    fn test_parse_gdunit4_xml_passing() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<testsuites tests="3" failures="0" errors="0" skipped="0" time="0.5">
  <testsuite name="res://test/test_example.gd" tests="3" failures="0" time="0.5">
    <testcase name="test_one" classname="res://test/test_example.gd" time="0.1">
    </testcase>
    <testcase name="test_two" classname="res://test/test_example.gd" time="0.2">
    </testcase>
    <testcase name="test_three" classname="res://test/test_example.gd" time="0.2">
    </testcase>
  </testsuite>
</testsuites>"#;
        let (results, summary) = parse_gdunit4_xml(xml);
        assert_eq!(results.len(), 3);
        assert!(results.iter().all(|r| r.status == TestStatus::Pass));
        assert_eq!(summary.passed, 3);
        assert_eq!(summary.failed, 0);
        assert_eq!(summary.skipped, 0);
        assert_eq!(summary.total, 3);
    }

    #[test]
    fn test_parse_gdunit4_xml_failures() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<testsuites tests="3" failures="1" errors="1" skipped="0" time="0.5">
  <testsuite name="res://test/test_example.gd" tests="3" failures="1" errors="1" time="0.5">
    <testcase name="test_pass" classname="res://test/test_example.gd" time="0.1">
    </testcase>
    <testcase name="test_fail" classname="res://test/test_example.gd" time="0.2">
      <failure message="Expected &apos;10&apos; but was &apos;5&apos;" type="FAILURE">stack trace</failure>
    </testcase>
    <testcase name="test_error" classname="res://test/test_example.gd" time="0.1">
      <error message="Null reference" type="ERROR">stack trace</error>
    </testcase>
  </testsuite>
</testsuites>"#;
        let (results, summary) = parse_gdunit4_xml(xml);
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].status, TestStatus::Pass);
        assert_eq!(results[1].status, TestStatus::Fail);
        assert_eq!(results[1].errors[0].message, "Expected '10' but was '5'");
        assert_eq!(results[2].status, TestStatus::Fail);
        assert_eq!(results[2].errors[0].message, "Null reference");
        assert_eq!(summary.passed, 1);
        assert_eq!(summary.failed, 2);
        assert_eq!(summary.total, 3);
    }

    #[test]
    fn test_parse_gdunit4_xml_empty() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<testsuites tests="0" failures="0" errors="0" skipped="0" time="0.0">
</testsuites>"#;
        let (results, summary) = parse_gdunit4_xml(xml);
        assert!(results.is_empty());
        assert_eq!(summary.total, 0);
    }

    #[test]
    fn test_parse_gdunit4_xml_skipped() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<testsuites tests="2" failures="0" errors="0" skipped="1" time="0.3">
  <testsuite name="res://test/test_example.gd" tests="2" failures="0" skipped="1" time="0.3">
    <testcase name="test_pass" classname="res://test/test_example.gd" time="0.2">
    </testcase>
    <testcase name="test_skip" classname="res://test/test_example.gd" time="0.0">
      <skipped message="Not implemented yet"/>
    </testcase>
  </testsuite>
</testsuites>"#;
        let (results, summary) = parse_gdunit4_xml(xml);
        assert_eq!(results.len(), 1); // skipped tests excluded from results
        assert_eq!(results[0].status, TestStatus::Pass);
        assert_eq!(summary.passed, 1);
        assert_eq!(summary.failed, 0);
        assert_eq!(summary.skipped, 1);
        assert_eq!(summary.total, 1);
    }

    #[test]
    fn test_parse_gdunit4_xml_malformed() {
        let (results, summary) = parse_gdunit4_xml("not xml at all");
        assert!(results.is_empty());
        assert_eq!(summary.total, 0);
        assert_eq!(summary.passed, 0);
        assert_eq!(summary.failed, 0);
    }

    #[test]
    fn test_extract_xml_attr() {
        assert_eq!(
            extract_xml_attr(r#"name="test_foo" classname="TestSuite""#, "name"),
            Some("test_foo".to_string())
        );
        assert_eq!(
            extract_xml_attr(r#"name="test_foo" classname="TestSuite""#, "classname"),
            Some("TestSuite".to_string())
        );
        assert_eq!(
            extract_xml_attr(r#"time="1.234""#, "time"),
            Some("1.234".to_string())
        );
        assert_eq!(extract_xml_attr("no attrs here", "name"), None);
    }

    #[test]
    fn test_decode_xml_entities() {
        assert_eq!(decode_xml_entities("a &amp; b"), "a & b");
        assert_eq!(decode_xml_entities("&lt;tag&gt;"), "<tag>");
        assert_eq!(
            decode_xml_entities("he said &quot;hi&quot;"),
            r#"he said "hi""#
        );
        assert_eq!(decode_xml_entities("it&apos;s"), "it's");
        assert_eq!(decode_xml_entities("no entities"), "no entities");
    }

    #[test]
    fn test_parse_gdunit4_xml_self_closing() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<testsuites tests="2" failures="0">
  <testsuite name="Suite">
    <testcase name="test_a" classname="res://test/test_a.gd" time="0.05"/>
    <testcase name="test_b" classname="res://test/test_a.gd" time="0.03"/>
  </testsuite>
</testsuites>"#;
        let (results, summary) = parse_gdunit4_xml(xml);
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|r| r.status == TestStatus::Pass));
        assert_eq!(summary.passed, 2);
        assert_eq!(summary.total, 2);
    }

    #[test]
    fn test_parse_gdunit4_xml_classname_label() {
        let xml = r#"<testsuites>
  <testsuite name="Suite">
    <testcase name="test_method" classname="res://test/test_player.gd" time="0.1">
    </testcase>
  </testsuite>
</testsuites>"#;
        let (results, _) = parse_gdunit4_xml(xml);
        assert_eq!(results.len(), 1);
        assert_eq!(
            results[0].file.as_deref(),
            Some("test/test_player.gd::test_method")
        );
    }

    #[test]
    fn test_skipped_not_in_json_when_zero() {
        let summary = TestSummary {
            passed: 5,
            failed: 0,
            errors: 0,
            skipped: 0,
            total: 5,
        };
        let json = serde_json::to_string(&summary).unwrap();
        assert!(!json.contains("skipped"));
    }

    #[test]
    fn test_skipped_in_json_when_nonzero() {
        let summary = TestSummary {
            passed: 4,
            failed: 0,
            errors: 0,
            skipped: 1,
            total: 4,
        };
        let json = serde_json::to_string(&summary).unwrap();
        assert!(json.contains("\"skipped\":1"));
    }
}
