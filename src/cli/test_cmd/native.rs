#![allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Instant;

use gd_core::gd_ast;
use miette::Result;
use owo_colors::OwoColorize;
use rayon::prelude::*;

use gd_interp::error::ErrorKind;
use gd_interp::exec;
use gd_interp::interpreter::Interpreter;

use super::{
    RunContext, TestError, TestResult, TestRunner, TestStatus, TestSummary, filter_files_by_tests,
    hprintln,
};

pub struct NativeRunner;

impl TestRunner for NativeRunner {
    fn name(&self) -> &'static str {
        "native"
    }

    fn run(&self, ctx: &RunContext) -> Result<(Vec<TestResult>, TestSummary)> {
        run_native_tests(ctx)
    }
}

/// Run all tests in a single file, returning results for each test function.
#[allow(clippy::too_many_lines)]
fn run_file_tests(
    test_file: &PathBuf,
    project_root: &std::path::Path,
    name_filter: Option<&str>,
    json_mode: bool,
    quiet: bool,
    output: &Mutex<()>,
) -> Vec<TestResult> {
    let rel = gd_core::fs::relative_slash(test_file, project_root);

    let source = match std::fs::read_to_string(test_file) {
        Ok(s) => s,
        Err(e) => {
            let _lock = output
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            hprintln!(json_mode, "{} {} (read error: {e})", "✗".red(), rel);
            return vec![TestResult {
                file: Some(rel),
                status: TestStatus::Error,
                duration_ms: 0,
                errors: vec![TestError {
                    file: test_file.display().to_string(),
                    line: None,
                    message: format!("failed to read: {e}"),
                }],
                stderr: None,
                stdout: None,
            }];
        }
    };

    let tree = match gd_core::parser::parse(&source) {
        Ok(t) => t,
        Err(e) => {
            let _lock = output
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            hprintln!(json_mode, "{} {} (parse error: {e})", "✗".red(), rel);
            return vec![TestResult {
                file: Some(rel),
                status: TestStatus::Error,
                duration_ms: 0,
                errors: vec![TestError {
                    file: test_file.display().to_string(),
                    line: None,
                    message: format!("parse error: {e}"),
                }],
                stderr: None,
                stdout: None,
            }];
        }
    };

    let file = gd_ast::convert(&tree, &source);

    // Detect lifecycle hooks
    let has_before_each = file.funcs().any(|f| f.name == "before_each");
    let has_after_each = file.funcs().any(|f| f.name == "after_each");
    let has_before_all = file.funcs().any(|f| f.name == "before_all");
    let has_after_all = file.funcs().any(|f| f.name == "after_all");

    // Collect test_* functions
    let test_funcs: Vec<&str> = file
        .funcs()
        .filter(|f| f.name.starts_with("test_"))
        .filter(|f| name_filter.is_none_or(|n| f.name.contains(n)))
        .map(|f| f.name)
        .collect();

    if test_funcs.is_empty() {
        return vec![];
    }

    let mut results = Vec::new();

    // Run before_all once per file
    if has_before_all {
        let Ok(mut interp) = Interpreter::from_file_with_source(&file, &source) else {
            return results;
        };
        if let Some(func) = interp.lookup_func("before_all") {
            let _ = exec::exec_func(func, &[], &mut interp);
        }
    }

    for func_name in &test_funcs {
        let label = format!("{rel}::{func_name}");
        let test_start = Instant::now();

        // Build a fresh interpreter for each test
        let mut interp = match Interpreter::from_file_with_source(&file, &source) {
            Ok(i) => i,
            Err(e) => {
                let duration_ms = test_start.elapsed().as_millis() as u64;
                let _lock = output
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                hprintln!(json_mode, "{} {label} (init error: {e})", "✗".red());
                results.push(TestResult {
                    file: Some(label),
                    status: TestStatus::Error,
                    duration_ms,
                    errors: vec![TestError {
                        file: rel.clone(),
                        line: Some(e.line),
                        message: e.message.clone(),
                    }],
                    stderr: None,
                    stdout: None,
                });
                continue;
            }
        };

        // Run before_each
        if has_before_each
            && let Some(hook) = interp.lookup_func("before_each")
            && let Err(e) = exec::exec_func(hook, &[], &mut interp)
        {
            let duration_ms = test_start.elapsed().as_millis() as u64;
            let _lock = output
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            hprintln!(json_mode, "{} {label} (before_each error: {e})", "✗".red());
            results.push(TestResult {
                file: Some(label),
                status: TestStatus::Error,
                duration_ms,
                errors: vec![TestError {
                    file: rel.clone(),
                    line: if e.line > 0 { Some(e.line) } else { None },
                    message: format!("before_each: {}", e.message),
                }],
                stderr: None,
                stdout: None,
            });
            continue;
        }

        let Some(func) = interp.lookup_func(func_name) else {
            continue;
        };

        let result = exec::exec_func(func, &[], &mut interp);
        let duration_ms = test_start.elapsed().as_millis() as u64;

        // Run after_each regardless of test result
        if has_after_each && let Some(hook) = interp.lookup_func("after_each") {
            let _ = exec::exec_func(hook, &[], &mut interp);
        }

        match result {
            Ok(_) => {
                if !json_mode && !quiet {
                    let _lock = output
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    hprintln!(json_mode, "{} {label}", "✓".green());
                }
                results.push(TestResult {
                    file: Some(label),
                    status: TestStatus::Pass,
                    duration_ms,
                    errors: vec![],
                    stderr: None,
                    stdout: if json_mode {
                        let out = interp.env.take_output();
                        if out.is_empty() {
                            None
                        } else {
                            Some(out.join("\n"))
                        }
                    } else {
                        None
                    },
                });
            }
            Err(e) => {
                let is_assertion = e.kind == ErrorKind::AssertionFailed;
                let status = if is_assertion {
                    TestStatus::Fail
                } else {
                    TestStatus::Error
                };

                if !json_mode {
                    let _lock = output
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    hprintln!(json_mode, "{} {label}", "✗".red());
                    print_error_message(&rel, &e.message, e.line, json_mode);
                }

                results.push(TestResult {
                    file: Some(label),
                    status,
                    duration_ms,
                    errors: vec![TestError {
                        file: rel.clone(),
                        line: if e.line > 0 { Some(e.line) } else { None },
                        message: e.message.clone(),
                    }],
                    stderr: None,
                    stdout: if json_mode {
                        let out = interp.env.take_output();
                        if out.is_empty() {
                            None
                        } else {
                            Some(out.join("\n"))
                        }
                    } else {
                        None
                    },
                });
            }
        }
    }

    // Run after_all once per file
    if has_after_all {
        let Ok(mut interp) = Interpreter::from_file_with_source(&file, &source) else {
            return results;
        };
        if let Some(func) = interp.lookup_func("after_all") {
            let _ = exec::exec_func(func, &[], &mut interp);
        }
    }

    results
}

#[allow(clippy::unnecessary_wraps)]
fn run_native_tests(ctx: &RunContext) -> Result<(Vec<TestResult>, TestSummary)> {
    let args = ctx.args;
    let json_mode = ctx.json_mode;
    let project_root = &ctx.project.root;

    // Filter test files by --name / --class
    let file_infos =
        filter_files_by_tests(ctx.test_files, args.name.as_deref(), args.class.as_deref());
    let effective_files: Vec<&PathBuf> = file_infos.iter().map(|i| &i.path).collect();

    // Mutex for synchronized terminal output
    let output_lock = Mutex::new(());
    let name_filter = args.name.as_deref();
    let quiet = args.quiet;

    // Run test files in parallel using rayon
    let all_results: Vec<Vec<TestResult>> = effective_files
        .par_iter()
        .map(|test_file| {
            run_file_tests(
                test_file,
                project_root,
                name_filter,
                json_mode,
                quiet,
                &output_lock,
            )
        })
        .collect();

    // Flatten results and compute summary
    let results: Vec<TestResult> = all_results.into_iter().flatten().collect();
    let passed = results
        .iter()
        .filter(|r| r.status == TestStatus::Pass)
        .count();
    let failed = results.len() - passed;

    let summary = TestSummary {
        passed,
        failed,
        errors: 0,
        skipped: 0,
        total: results.len(),
    };

    Ok((results, summary))
}

/// Print an error message, indenting continuation lines to align with the first.
fn print_error_message(rel: &str, message: &str, line: usize, json_mode: bool) {
    if line > 0 {
        let prefix = format!("  {rel}:{line} ");
        let indent = " ".repeat(prefix.len());
        let mut first = true;
        for msg_line in message.lines() {
            if first {
                hprintln!(json_mode, "{prefix}{msg_line}");
                first = false;
            } else {
                hprintln!(json_mode, "{indent}{msg_line}");
            }
        }
    } else {
        for msg_line in message.lines() {
            hprintln!(json_mode, "  {msg_line}");
        }
    }
}
