#![allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]

use std::path::PathBuf;
use std::time::Instant;

use gd_core::gd_ast;
use miette::Result;
use owo_colors::OwoColorize;

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

#[allow(clippy::too_many_lines, clippy::unnecessary_wraps)]
fn run_native_tests(ctx: &RunContext) -> Result<(Vec<TestResult>, TestSummary)> {
    let args = ctx.args;
    let json_mode = ctx.json_mode;
    let project_root = &ctx.project.root;

    // Filter test files by --name / --class
    let file_infos =
        filter_files_by_tests(ctx.test_files, args.name.as_deref(), args.class.as_deref());
    let effective_files: Vec<&PathBuf> = file_infos.iter().map(|i| &i.path).collect();

    let mut results = Vec::new();
    let mut passed = 0usize;
    let mut failed = 0usize;

    for test_file in &effective_files {
        let rel = gd_core::fs::relative_slash(test_file, project_root);

        let source = match std::fs::read_to_string(test_file) {
            Ok(s) => s,
            Err(e) => {
                hprintln!(json_mode, "{} {} (read error: {e})", "✗".red(), rel);
                results.push(TestResult {
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
                });
                failed += 1;
                continue;
            }
        };

        let tree = match gd_core::parser::parse(&source) {
            Ok(t) => t,
            Err(e) => {
                hprintln!(json_mode, "{} {} (parse error: {e})", "✗".red(), rel);
                results.push(TestResult {
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
                });
                failed += 1;
                continue;
            }
        };

        let file = gd_ast::convert(&tree, &source);

        // Collect test_* functions
        let test_funcs: Vec<&str> = file
            .funcs()
            .filter(|f| f.name.starts_with("test_"))
            .filter(|f| {
                args.name
                    .as_ref()
                    .is_none_or(|n| f.name.contains(n.as_str()))
            })
            .map(|f| f.name)
            .collect();

        if test_funcs.is_empty() {
            continue;
        }

        for func_name in &test_funcs {
            let label = format!("{rel}::{func_name}");
            let test_start = Instant::now();

            // Build a fresh interpreter for each test
            let mut interp = match Interpreter::from_file(&file) {
                Ok(i) => i,
                Err(e) => {
                    let duration_ms = test_start.elapsed().as_millis() as u64;
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
                    failed += 1;
                    continue;
                }
            };

            let Some(func) = interp.lookup_func(func_name) else {
                continue;
            };

            let result = exec::exec_func(func, &[], &mut interp);
            let duration_ms = test_start.elapsed().as_millis() as u64;

            match result {
                Ok(_) => {
                    if !json_mode && !args.quiet {
                        hprintln!(json_mode, "{} {label}", "✓".green());
                    }
                    results.push(TestResult {
                        file: Some(label),
                        status: TestStatus::Pass,
                        duration_ms,
                        errors: vec![],
                        stderr: None,
                        stdout: if json_mode {
                            let output = interp.env.take_output();
                            if output.is_empty() {
                                None
                            } else {
                                Some(output.join("\n"))
                            }
                        } else {
                            None
                        },
                    });
                    passed += 1;
                }
                Err(e) => {
                    let is_assertion = e.kind == ErrorKind::AssertionFailed;
                    let status = if is_assertion {
                        TestStatus::Fail
                    } else {
                        TestStatus::Error
                    };

                    if !json_mode {
                        hprintln!(json_mode, "{} {label}", "✗".red());
                        // Show assertion message with context
                        if e.line > 0 {
                            hprintln!(json_mode, "  {}:{} {}", rel, e.line, e.message);
                        } else {
                            hprintln!(json_mode, "  {}", e.message);
                        }
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
                            let output = interp.env.take_output();
                            if output.is_empty() {
                                None
                            } else {
                                Some(output.join("\n"))
                            }
                        } else {
                            None
                        },
                    });
                    failed += 1;
                }
            }
        }
    }

    let total = passed + failed;
    let summary = TestSummary {
        passed,
        failed,
        errors: 0,
        skipped: 0,
        total,
    };

    Ok((results, summary))
}
