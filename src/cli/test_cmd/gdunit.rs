#![allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]

use miette::{Result, miette};
use owo_colors::OwoColorize;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::core::project::GodotProject;

use crate::{ceprintln, cprintln};

use super::{
    RunArgs, TestError, TestResult, TestStatus, TestSummary, extract_errors, filter_noise,
    hprintln, run_with_timeout, strip_res_prefix,
};

/// Run tests using gdUnit4 framework.
#[allow(clippy::too_many_lines)]
pub fn run_gdunit4_tests(
    godot: &Path,
    project: &GodotProject,
    args: &RunArgs,
    json_mode: bool,
) -> Result<(Vec<TestResult>, TestSummary)> {
    if args.filter.is_some() {
        hprintln!(
            json_mode,
            "{} --filter is not supported with gdUnit4; use -- -i SuiteName to exclude tests",
            "!".yellow().bold()
        );
    }
    if args.name.is_some() || args.class.is_some() {
        hprintln!(
            json_mode,
            "{} --name and --class filters are only supported with GUT",
            "!".yellow().bold()
        );
    }

    let spinner = if json_mode {
        None
    } else {
        let sp = indicatif::ProgressBar::new_spinner();
        sp.set_style(
            indicatif::ProgressStyle::default_spinner()
                .template("{spinner:.green} {msg}")
                .expect("invalid spinner template"),
        );
        sp.set_message("Running gdUnit4 tests...");
        sp.enable_steady_tick(std::time::Duration::from_millis(100));
        Some(sp)
    };

    // Determine test directories for -a flags
    let test_dirs: Vec<String> = if args.path.is_empty() {
        ["test", "tests"]
            .iter()
            .filter(|d| project.root.join(d).is_dir())
            .map(|d| format!("res://{d}"))
            .collect()
    } else {
        let mut dirs = Vec::new();
        for p in &args.path {
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
            cprintln!("{out}");
        }
        let err = if args.clean {
            filter_noise(&stderr_str)
        } else {
            stderr_str.clone()
        };
        if !err.is_empty() {
            ceprintln!("{err}");
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
                cprintln!("{stdout_str}");
            }
            if !stderr_str.is_empty() {
                ceprintln!("{stderr_str}");
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
                    TestStatus::Timeout => {}
                }
            }
        }
    }

    Ok((results, summary))
}

/// Find the results.xml file in the gdUnit4 report directory.
/// gdUnit4 writes to subdirectories like `report_1/results.xml`.
pub fn find_results_xml(report_dir: &Path) -> Option<PathBuf> {
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

/// Parse `gdUnit4` `JUnit` XML results into test results and summary.
pub fn parse_gdunit4_xml(xml: &str) -> (Vec<TestResult>, TestSummary) {
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
            .map_or(0, |t| (t * 1000.0) as u64);

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
pub fn extract_xml_attr(text: &str, attr: &str) -> Option<String> {
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
pub fn decode_xml_entities(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
}
