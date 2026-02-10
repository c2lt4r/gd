use clap::Args;
use miette::{miette, Result};
use owo_colors::OwoColorize;
use std::env;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;

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
            let rel = f
                .strip_prefix(&project.root)
                .unwrap_or(f);
            println!("  {}", rel.display().to_string().dimmed());
        }
    }

    let start = Instant::now();

    let result = if has_gut {
        println!(
            "{} Running tests with GUT",
            "▶".green()
        );
        run_gut_tests(&godot, &project, &args, &test_files)
    } else {
        println!(
            "{} Running tests with Godot (no GUT addon)",
            "▶".green()
        );
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
            eprintln!(
                "{} Tests failed ({:.2}s)",
                "✗".red().bold(),
                secs,
            );
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
    cmd.arg("--headless")
        .arg("--path")
        .arg(&project.root)
        .arg("-s")
        .arg("addons/gut/gut_cmdln.gd");

    // Pass test directories or specific files to GUT
    if !args.paths.is_empty() {
        for path in &args.paths {
            let dir_str = format!("-gdir=res://{}", path.display());
            cmd.arg(dir_str);
        }
    }

    if let Some(ref filter) = args.filter {
        cmd.arg(format!("-gselect={filter}"));
    }

    // Extra args from CLI (after --)
    for arg in &args.extra {
        cmd.arg(arg);
    }

    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

    let output = cmd
        .output()
        .map_err(|e| miette!("Failed to start Godot: {e}"))?;

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
        let rel = test_file
            .strip_prefix(&project.root)
            .unwrap_or(test_file);
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
        cmd.arg("--headless")
            .arg("--path")
            .arg(&project.root)
            .arg("--script")
            .arg(test_file);

        // Extra args from CLI (after --)
        for arg in &args.extra {
            cmd.arg(arg);
        }

        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

        let output = cmd
            .output()
            .map_err(|e| miette!("Failed to start Godot: {e}"))?;

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

/// Parse GUT command-line output for pass/fail counts.
fn parse_gut_output(output: &str, file_count: usize) -> TestSummary {
    let mut passed = 0usize;
    let mut failed = 0usize;

    for line in output.lines() {
        let trimmed = line.trim();
        // GUT outputs lines like: "Passed: 5 Failed: 2"
        if trimmed.contains("Passed:") && trimmed.contains("Failed:") {
            for part in trimmed.split_whitespace().collect::<Vec<_>>().windows(2) {
                if part[0] == "Passed:" {
                    if let Ok(n) = part[1].parse::<usize>() {
                        passed = n;
                    }
                }
                if part[0] == "Failed:" {
                    if let Ok(n) = part[1].parse::<usize>() {
                        failed = n;
                    }
                }
            }
            return TestSummary { passed, failed };
        }
    }

    // Fallback: if we couldn't parse the output, estimate from file count
    TestSummary {
        passed: 0,
        failed: file_count,
    }
}
