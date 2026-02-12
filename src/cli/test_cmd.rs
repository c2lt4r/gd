use clap::Args;
use miette::{Result, miette};
use owo_colors::OwoColorize;
use std::env;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::core::config::Config;
use crate::core::project::GodotProject;

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
    /// Extra args to pass to Godot
    #[arg(last = true)]
    pub extra: Vec<String>,
}

pub fn exec(args: TestArgs) -> Result<()> {
    let cwd = env::current_dir().unwrap_or_default();
    let config = Config::load(&cwd)?;
    let project = GodotProject::discover(&cwd)?;
    let godot = crate::build::find_godot(&config)?;

    let has_gut = project.root.join("addons/gut").is_dir();
    let test_files = discover_test_files(&project.root, &args.paths)?;

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

    if test_files.is_empty() {
        println!(
            "{} No test files found{}",
            "!".yellow().bold(),
            args.filter
                .as_ref()
                .map(|f| format!(" matching '{f}'"))
                .unwrap_or_default()
        );
        return Ok(());
    }

    println!(
        "{} Found {} test file{}",
        "●".blue(),
        test_files.len(),
        if test_files.len() == 1 { "" } else { "s" }
    );

    if args.verbose {
        for f in &test_files {
            let rel = f.strip_prefix(&project.root).unwrap_or(f);
            println!("  {}", rel.display().to_string().dimmed());
        }
    }

    let start = Instant::now();

    let result = if has_gut {
        println!("{} Running tests with GUT", "▶".green());
        run_gut_tests(&godot, &project, &args, &test_files)
    } else {
        println!("{} Running tests with Godot (no GUT addon)", "▶".green());
        run_script_tests(&godot, &project, &args, &test_files)
    };

    let elapsed = start.elapsed();
    let secs = elapsed.as_secs_f64();

    match result {
        Ok(summary) => {
            println!();
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
            if summary.failed > 0 {
                std::process::exit(1);
            }
            Ok(())
        }
        Err(e) => {
            println!();
            eprintln!("{} Tests failed ({:.2}s)", "✗".red().bold(), secs,);
            Err(e)
        }
    }
}

struct TestSummary {
    passed: usize,
    failed: usize,
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
) -> Result<TestSummary> {
    let spinner = indicatif::ProgressBar::new_spinner();
    spinner.set_style(
        indicatif::ProgressStyle::default_spinner()
            .template("{spinner:.green} {msg}")
            .expect("invalid spinner template"),
    );
    spinner.set_message("Running GUT tests...");
    spinner.enable_steady_tick(std::time::Duration::from_millis(100));

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

    // GUT handles its own exit with -gexit, don't kill on stderr errors
    let output = run_with_timeout(&mut cmd, Duration::from_secs(args.timeout), false)?;

    spinner.finish_and_clear();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if args.verbose {
        if !stdout.is_empty() {
            println!("{stdout}");
        }
        if !stderr.is_empty() {
            eprintln!("{stderr}");
        }
    }

    // Parse GUT output for pass/fail counts
    let summary = parse_gut_output(&stdout, test_files.len());

    if !output.status.success() && summary.passed == 0 && summary.failed == 0 {
        // GUT didn't produce parseable output; treat as full failure
        if !args.verbose {
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

    Ok(summary)
}

/// Run tests by executing each test script individually with Godot.
fn run_script_tests(
    godot: &Path,
    project: &GodotProject,
    args: &TestArgs,
    test_files: &[PathBuf],
) -> Result<TestSummary> {
    let mut passed = 0usize;
    let mut failed = 0usize;

    for (i, test_file) in test_files.iter().enumerate() {
        let rel = test_file.strip_prefix(&project.root).unwrap_or(test_file);
        let label = format!("[{}/{}] {}", i + 1, test_files.len(), rel.display());

        let spinner = indicatif::ProgressBar::new_spinner();
        spinner.set_style(
            indicatif::ProgressStyle::default_spinner()
                .template("{spinner:.green} {msg}")
                .expect("invalid spinner template"),
        );
        spinner.set_message(label.clone());
        spinner.enable_steady_tick(std::time::Duration::from_millis(100));

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

        // Kill early on script errors (Godot hangs on assert failure in --script mode)
        let output = match run_with_timeout(&mut cmd, Duration::from_secs(args.timeout), true) {
            Ok(output) => output,
            Err(_) => {
                spinner.finish_and_clear();
                failed += 1;
                println!(
                    "{} {} (timed out after {}s)",
                    "✗".red(),
                    rel.display(),
                    args.timeout
                );
                continue;
            }
        };

        spinner.finish_and_clear();

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        if output.status.success() {
            passed += 1;
            println!("{} {}", "✓".green(), rel.display());
        } else {
            failed += 1;
            println!("{} {}", "✗".red(), rel.display());
        }

        if args.verbose || !output.status.success() {
            if !stdout.is_empty() {
                for line in stdout.lines() {
                    println!("  {line}");
                }
            }
            if !stderr.is_empty() {
                for line in stderr.lines() {
                    eprintln!("  {}", line.dimmed());
                }
            }
        }
    }

    Ok(TestSummary { passed, failed })
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
/// Supports multiple GUT output formats across versions and log levels.
fn parse_gut_output(output: &str, file_count: usize) -> TestSummary {
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

    if found {
        // If no failing tests line was found, GUT omits it when 0
        TestSummary { passed, failed }
    } else {
        // Fallback: if we couldn't parse the output, estimate from file count
        TestSummary {
            passed: 0,
            failed: file_count,
        }
    }
}
