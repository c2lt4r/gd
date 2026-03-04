#![allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]

use miette::{Result, miette};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use gd_core::project::GodotProject;

use gd_core::{ceprintln, cprintln};

use super::gdunit::parse_gdunit4_xml;
use super::{
    RunArgs, RunContext, TestResult, TestRunner, TestStatus, TestSummary, extract_errors,
    filter_noise, run_with_timeout,
};

pub struct GutRunner;

impl TestRunner for GutRunner {
    fn name(&self) -> &'static str {
        "GUT"
    }

    fn run(&self, ctx: &RunContext) -> Result<(Vec<TestResult>, TestSummary)> {
        run_gut_tests(
            ctx.godot,
            ctx.project,
            ctx.args,
            ctx.test_files,
            ctx.json_mode,
        )
    }
}

/// Run tests using GUT addon.
#[allow(clippy::too_many_lines)]
pub fn run_gut_tests(
    godot: &Path,
    project: &GodotProject,
    args: &RunArgs,
    test_files: &[std::path::PathBuf],
    json_mode: bool,
) -> Result<(Vec<TestResult>, TestSummary)> {
    let spinner = if json_mode {
        None
    } else {
        let sp = indicatif::ProgressBar::new_spinner();
        sp.set_style(
            indicatif::ProgressStyle::default_spinner()
                .template("{spinner:.green} {msg}")
                .expect("invalid spinner template"),
        );
        sp.set_message("Running GUT tests...");
        sp.enable_steady_tick(std::time::Duration::from_millis(100));
        Some(sp)
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
                let rel = gd_core::fs::relative_slash(parent, &project.root);
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
    if let Some(ref name) = args.name {
        cmd.arg(format!("-gunit_test_name={name}"));
    }
    if let Some(ref class) = args.class {
        cmd.arg(format!("-ginner_class={class}"));
    }

    // Always generate JUnit XML to a temp file for per-test granularity
    let temp_junit = project.root.join(".godot/gd-gut-junit.xml");
    cmd.arg(format!("-gjunit_xml_file={}", temp_junit.display()));

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
                cprintln!("{display_stdout}");
            }
        }
        if !stderr.is_empty() {
            let display_stderr = if args.clean {
                filter_noise(&stderr)
            } else {
                stderr.to_string()
            };
            if !display_stderr.is_empty() {
                ceprintln!("{display_stderr}");
            }
        }
    }

    // Parse GUT output for pass/fail counts (used as fallback)
    let (gut_passed, gut_failed) = parse_gut_counts(&stdout);
    let parsed_ok = gut_passed > 0 || gut_failed > 0;

    if !output.status.success() && !parsed_ok {
        // GUT didn't produce parseable output; treat as full failure
        if !json_mode && !args.verbose {
            if !stdout.is_empty() {
                cprintln!("{stdout}");
            }
            if !stderr.is_empty() {
                ceprintln!("{stderr}");
            }
        }
        let _ = std::fs::remove_file(&temp_junit);
        return Err(miette!("GUT exited with non-zero status"));
    }

    // Try to parse JUnit XML for per-test granularity
    let xml_content = std::fs::read_to_string(&temp_junit).unwrap_or_default();

    // If user requested --junit, copy the XML to their path
    if let Some(ref user_junit) = args.junit
        && !xml_content.is_empty()
    {
        let _ = std::fs::copy(&temp_junit, user_junit);
    }

    // Clean up temp XML
    let _ = std::fs::remove_file(&temp_junit);

    let (results, summary) = if xml_content.is_empty() {
        // No XML file — fall back to aggregate counts
        build_aggregate_result(
            gut_passed,
            gut_failed,
            &stderr,
            &stdout,
            test_duration_ms,
            json_mode,
        )
    } else {
        // Parse JUnit XML for per-test results
        let (mut xml_results, xml_summary) = parse_gdunit4_xml(&xml_content);

        if xml_summary.total > 0 {
            // Print per-test results in human mode
            if !json_mode {
                super::print_results(&xml_results, args);
            }

            // Attach stderr/stdout to first result for JSON mode
            if json_mode && !xml_results.is_empty() {
                if !stderr.is_empty() {
                    xml_results[0].stderr = Some(stderr.into_owned());
                }
                if !stdout.is_empty() {
                    xml_results[0].stdout = Some(stdout.into_owned());
                }
            }

            (xml_results, xml_summary)
        } else {
            // XML was empty/unparseable — fall back to aggregate
            build_aggregate_result(
                gut_passed,
                gut_failed,
                &stderr,
                &stdout,
                test_duration_ms,
                json_mode,
            )
        }
    };

    Ok((results, summary))
}

/// Parse GUT command-line output for pass/fail counts.
/// Returns (passed, failed). Both are 0 if parsing failed.
pub fn parse_gut_counts(output: &str) -> (usize, usize) {
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

/// Build an aggregate (single-entry) result when JUnit XML is unavailable.
fn build_aggregate_result(
    gut_passed: usize,
    gut_failed: usize,
    stderr: &str,
    stdout: &str,
    duration_ms: u64,
    json_mode: bool,
) -> (Vec<TestResult>, TestSummary) {
    let errors = extract_errors(stderr);
    let error_count = errors.len();

    let status = if gut_failed > 0 || error_count > 0 {
        TestStatus::Fail
    } else {
        TestStatus::Pass
    };

    let result = TestResult {
        file: None,
        status,
        duration_ms,
        errors,
        stderr: if json_mode && !stderr.is_empty() {
            Some(stderr.to_string())
        } else {
            None
        },
        stdout: if json_mode && !stdout.is_empty() {
            Some(stdout.to_string())
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

    (vec![result], summary)
}
