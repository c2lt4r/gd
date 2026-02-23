#![allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]

use miette::{Result, miette};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::core::project::GodotProject;

use crate::{ceprintln, cprintln};

use super::{
    RunArgs, RunContext, TestError, TestResult, TestRunner, TestStatus, TestSummary,
    extract_errors, filter_files_by_tests, filter_noise, run_with_timeout, strip_res_prefix,
};

pub struct GdUnit4Runner;

impl TestRunner for GdUnit4Runner {
    fn name(&self) -> &'static str {
        "gdUnit4"
    }

    fn run(&self, ctx: &RunContext) -> Result<(Vec<TestResult>, TestSummary)> {
        run_gdunit4_tests(
            ctx.godot,
            ctx.project,
            ctx.args,
            ctx.test_files,
            ctx.json_mode,
        )
    }
}

/// Run tests using gdUnit4 framework.
#[allow(clippy::too_many_lines)]
pub fn run_gdunit4_tests(
    godot: &Path,
    project: &GodotProject,
    args: &RunArgs,
    test_files: &[PathBuf],
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
        sp.set_message("Running gdUnit4 tests...");
        sp.enable_steady_tick(std::time::Duration::from_millis(100));
        Some(sp)
    };

    let has_content_filters = args.name.is_some() || args.class.is_some();

    // Build -a (add) and -i (ignore) flags based on filters
    let (add_args, ignore_args) = build_gdunit4_filter_args(
        test_files,
        &project.root,
        args,
        has_content_filters || args.filter.is_some(),
    );

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

    for a in &add_args {
        cmd.arg("-a").arg(a);
    }
    for i in &ignore_args {
        cmd.arg("-i").arg(i);
    }

    // Report output directory (temp location, cleaned up after parsing)
    let report_dir = project.root.join(".godot/gd-test-reports");
    cmd.arg("-rd").arg("res://.godot/gd-test-reports/");

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
        // No XML — try parsing stdout for test results, then fall back to exit code
        let stdout_results = parse_gdunit4_stdout(&stdout_str);
        if stdout_results.is_empty() {
            let exit_code = output.status.code().unwrap_or(1);
            // Filter out addon-internal errors (framework noise, not user test failures)
            let errors: Vec<TestError> = extract_errors(&stderr_str)
                .into_iter()
                .filter(|e| !e.file.starts_with("addons/"))
                .collect();
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
        } else {
            build_results_from_stdout(&stdout_results, test_duration_ms)
        }
    };

    // If user requested --junit, copy the XML to their path before cleanup
    if let Some(ref user_junit) = args.junit
        && let Some(ref src) = xml_path
    {
        let _ = std::fs::copy(src, user_junit);
    }

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
        super::print_results(&results, args);
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
            let (msg, line) = extract_failure_details(fail_block, "failure");
            let file = classname
                .as_deref()
                .map(strip_res_prefix)
                .unwrap_or_default()
                .to_string();
            (
                TestStatus::Fail,
                vec![TestError {
                    file,
                    line,
                    message: msg,
                }],
            )
        } else if let Some(err_start) = block.find("<error") {
            failed += 1;
            let err_block = &block[err_start..];
            let (msg, line) = extract_failure_details(err_block, "error");
            let file = classname
                .as_deref()
                .map(strip_res_prefix)
                .unwrap_or_default()
                .to_string();
            (
                TestStatus::Fail,
                vec![TestError {
                    file,
                    line,
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

/// Extract the failure/error message from a JUnit XML element.
///
/// Prefers the CDATA body content (used by GUT) over the `message` attribute,
/// since GUT sets `message="failed"` while putting the actual assertion details
/// in a `<![CDATA[...]]>` block.
/// Extract failure/error details from a JUnit XML element.
///
/// Returns `(message, line_number)`. Handles two formats:
/// - **GUT**: `message="failed"`, actual details in CDATA body with "at line N"
/// - **gdUnit4**: `message="FAILED: res://path/file.gd:7"`, details in CDATA body
///
/// Extracts the best message text and line number from whichever source has them.
fn extract_failure_details(block: &str, tag: &str) -> (String, Option<usize>) {
    // Get raw body with line breaks preserved (needed for "at line N" extraction)
    let raw_body = extract_failure_body_raw(block, tag);

    // Extract line number from body (GUT's "at line N" pattern) before normalizing
    let (cleaned_body, body_line) = parse_failure_line(&raw_body);

    // Normalize to compact single line
    let msg = normalize_whitespace(&cleaned_body);

    // If we got a line from the body (GUT format), use it
    if body_line.is_some() {
        return (msg, body_line);
    }

    // Otherwise check the message attribute for gdUnit4's "FAILED: res://path:N" format
    if let Some(attr) = extract_xml_attr(block, "message")
        && let Some(line) = parse_message_attr_line(&attr)
    {
        return (msg, Some(line));
    }

    (msg, None)
}

/// Parse line number from gdUnit4's message attribute: "FAILED: res://path/file.gd:7"
fn parse_message_attr_line(attr: &str) -> Option<usize> {
    // Look for "res://...path:N" pattern
    let rest = attr.strip_prefix("FAILED: ").unwrap_or(attr);
    let (_file, line) = super::parse_res_location(rest);
    line
}

/// Extract raw failure text from CDATA body, message attribute, or text body.
/// Returns the raw text with original line breaks preserved (for line number extraction).
///
/// Priority: CDATA (most detailed, used by both GUT and gdUnit4) > message attribute > text body.
fn extract_failure_body_raw(block: &str, tag: &str) -> String {
    // Try CDATA body first: <failure ...><![CDATA[actual message]]></failure>
    if let Some(cdata_start) = block.find("<![CDATA[") {
        let content_start = cdata_start + "<![CDATA[".len();
        if let Some(cdata_end) = block[content_start..].find("]]>") {
            let body = block[content_start..content_start + cdata_end].trim();
            if !body.is_empty() {
                return decode_xml_entities(body);
            }
        }
    }

    // Try message attribute: <failure message="Expected '10' but was '5'">
    if let Some(msg) = extract_xml_attr(block, "message") {
        let decoded = decode_xml_entities(&msg);
        if !decoded.is_empty() {
            return decoded;
        }
    }

    // Fall back to text body: <failure ...>text content</failure>
    let close_tag = format!("</{tag}>");
    if let Some(gt_pos) = block.find('>') {
        let after_open = gt_pos + 1;
        if let Some(close_pos) = block[after_open..].find(&close_tag) {
            let body = block[after_open..after_open + close_pos].trim();
            if !body.is_empty() {
                return decode_xml_entities(body);
            }
        }
    }

    String::new()
}

/// Normalize multi-line CDATA content into a compact single line.
/// Trims each line and joins with spaces so assertion messages like:
///   `Expecting:\n '3'\n but was\n '2'`
/// become:
///   `Expecting: '3' but was '2'`
fn normalize_whitespace(s: &str) -> String {
    s.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Parse GUT's "at line N" suffix from a failure message.
/// Returns `(clean_message, line_number)`.
fn parse_failure_line(msg: &str) -> (String, Option<usize>) {
    // GUT appends "at line N" on its own line (or end of message)
    // Only extract valid (positive) line numbers
    for line in msg.lines().rev() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("at line ")
            && let Ok(n) = rest.trim().parse::<i64>()
        {
            let clean = msg
                .lines()
                .filter(|l| l.trim() != trimmed)
                .collect::<Vec<_>>()
                .join("\n");
            if n > 0 {
                return (clean.trim().to_string(), Some(n as usize));
            }
            // Negative line (-1) = no valid line info, strip it
            return (clean.trim().to_string(), None);
        }
    }
    (msg.to_string(), None)
}

/// Decode common XML entities in a string.
pub fn decode_xml_entities(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
}

/// A single parsed test result from gdUnit4 stdout.
struct StdoutTestResult {
    file: String,
    name: String,
    status: TestStatus,
    duration_ms: u64,
    report: Option<String>,
    line: Option<usize>,
}

/// Strip ANSI escape sequences from a string.
fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // Skip ESC [ ... final_byte sequences
            if chars.peek() == Some(&'[') {
                chars.next(); // consume '['
                // Consume until we hit a letter (the final byte, 0x40-0x7E)
                for cc in chars.by_ref() {
                    if cc.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Parse gdUnit4 stdout for per-test results.
///
/// gdUnit4 prints lines like:
///   `  res://test/test_example.gd > test_failing STARTED`
///   `  res://test/test_example.gd > test_failing FAILED 21ms`
///   `  Report:`
///   `    line 7: Expecting: ...`
fn parse_gdunit4_stdout(stdout: &str) -> Vec<StdoutTestResult> {
    // gdUnit4 output contains ANSI color codes; strip them before parsing
    let clean = strip_ansi(stdout);
    let mut results = Vec::new();
    let lines: Vec<&str> = clean.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let trimmed = lines[i].trim();

        // Match: "res://path/file.gd > test_name PASSED 17ms" or "FAILED 21ms"
        if let Some((file, name, status, duration_ms)) = parse_gdunit4_result_line(trimmed) {
            let mut report = None;
            let mut line_num = None;

            // Look ahead for "Report:" block
            if status == TestStatus::Fail && i + 1 < lines.len() {
                let next = lines[i + 1].trim();
                if next == "Report:" {
                    i += 2;
                    let mut report_lines = Vec::new();
                    while i < lines.len() {
                        let rl = lines[i].trim();
                        if rl.is_empty()
                            || rl.contains("> ")
                                && (rl.contains(" STARTED")
                                    || rl.contains(" PASSED")
                                    || rl.contains(" FAILED"))
                        {
                            break;
                        }
                        // Extract line number: "line 7: Expecting: ..."
                        if line_num.is_none()
                            && let Some(rest) = rl.strip_prefix("line ")
                            && let Some(colon_pos) = rest.find(':')
                            && let Ok(n) = rest[..colon_pos].trim().parse::<usize>()
                        {
                            line_num = Some(n);
                        }
                        report_lines.push(rl);
                        i += 1;
                    }
                    if !report_lines.is_empty() {
                        report = Some(report_lines.join("\n"));
                    }
                    // Don't increment i again since we already advanced past the report
                    results.push(StdoutTestResult {
                        file,
                        name,
                        status,
                        duration_ms,
                        report,
                        line: line_num,
                    });
                    continue;
                }
            }

            results.push(StdoutTestResult {
                file,
                name,
                status,
                duration_ms,
                report,
                line: line_num,
            });
        }
        i += 1;
    }

    results
}

/// Parse a single gdUnit4 result line like:
/// `res://test/test_example.gd > test_failing FAILED 21ms`
fn parse_gdunit4_result_line(line: &str) -> Option<(String, String, TestStatus, u64)> {
    // Must contain " > " separator and end with PASSED/FAILED + duration
    let arrow_pos = line.find(" > ")?;
    let file = strip_res_prefix(line[..arrow_pos].trim()).to_string();
    let rest = &line[arrow_pos + 3..];

    let (name, status, duration_ms) = if let Some(idx) = rest.rfind(" PASSED ") {
        let name = rest[..idx].trim().to_string();
        let dur = parse_ms_duration(&rest[idx + 8..]);
        (name, TestStatus::Pass, dur)
    } else if let Some(idx) = rest.rfind(" FAILED ") {
        let name = rest[..idx].trim().to_string();
        let dur = parse_ms_duration(&rest[idx + 8..]);
        (name, TestStatus::Fail, dur)
    } else {
        return None;
    };

    Some((file, name, status, duration_ms))
}

/// Parse "21ms" to 21.
fn parse_ms_duration(s: &str) -> u64 {
    s.trim()
        .strip_suffix("ms")
        .and_then(|n| n.trim().parse().ok())
        .unwrap_or(0)
}

/// Build TestResult/TestSummary from parsed stdout results.
fn build_results_from_stdout(
    stdout_results: &[StdoutTestResult],
    _total_duration_ms: u64,
) -> (Vec<TestResult>, TestSummary) {
    let mut results = Vec::new();
    let mut passed = 0;
    let mut failed = 0;

    for r in stdout_results {
        let label = format!("{}::{}", r.file, r.name);
        let errors = if let Some(ref report) = r.report {
            vec![TestError {
                file: r.file.clone(),
                line: r.line,
                message: report.clone(),
            }]
        } else {
            vec![]
        };

        match r.status {
            TestStatus::Pass => passed += 1,
            _ => failed += 1,
        }

        results.push(TestResult {
            file: Some(label),
            status: r.status,
            duration_ms: r.duration_ms,
            errors,
            stderr: None,
            stdout: None,
        });
    }

    let summary = TestSummary {
        passed,
        failed,
        errors: 0,
        skipped: 0,
        total: passed + failed,
    };

    (results, summary)
}

/// Build `-a` (add) and `-i` (ignore) flag values for gdUnit4 CLI.
///
/// When content filters (`--name`, `--class`, `--filter`) are active, uses per-file
/// `-a res://path/to/test.gd` args instead of directory-level args. For `--name`,
/// also builds `-i` args to exclude non-matching test functions within included files.
pub fn build_gdunit4_filter_args(
    test_files: &[PathBuf],
    project_root: &Path,
    args: &RunArgs,
    has_filters: bool,
) -> (Vec<String>, Vec<String>) {
    if !has_filters || test_files.is_empty() {
        // No content filters — use directory-based args (original behavior)
        let dirs: Vec<String> = if args.path.is_empty() {
            ["test", "tests"]
                .iter()
                .filter(|d| project_root.join(d).is_dir())
                .map(|d| format!("res://{d}"))
                .collect()
        } else {
            let mut dirs = Vec::new();
            for p in &args.path {
                let abs = if p.is_absolute() {
                    p.clone()
                } else {
                    project_root.join(p)
                };
                let dir = if abs.is_file() {
                    abs.parent().unwrap_or(&abs).to_path_buf()
                } else {
                    abs
                };
                let rel = crate::core::fs::relative_slash(&dir, project_root);
                let entry = format!("res://{rel}");
                if !dirs.contains(&entry) {
                    dirs.push(entry);
                }
            }
            dirs
        };
        return (dirs, Vec::new());
    }

    // Content filters active — filter at file level
    let file_infos = filter_files_by_tests(test_files, args.name.as_deref(), args.class.as_deref());

    let mut add_args = Vec::new();
    let mut ignore_args = Vec::new();

    for info in &file_infos {
        let rel = crate::core::fs::relative_slash(&info.path, project_root);
        let res_path = format!("res://{rel}");
        add_args.push(res_path.clone());

        // If filtering by --name, exclude non-matching tests in this file
        if let Some(ref name_filter) = args.name {
            for test_name in &info.tests {
                if !test_name.contains(name_filter.as_str()) {
                    // gdUnit4 ignore format: "res://path/file.gd:test_name"
                    ignore_args.push(format!("{res_path}:{test_name}"));
                }
            }
        }
    }

    (add_args, ignore_args)
}
