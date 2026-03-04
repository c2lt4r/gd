#![allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]

use miette::{Result, miette};
use owo_colors::OwoColorize;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use gd_core::project::GodotProject;

use super::{
    RunArgs, RunContext, TestResult, TestRunner, TestStatus, TestSummary, extract_errors,
    filter_files_by_tests, filter_noise, hprintln,
};
use gd_core::{ceprintln, cprintln};

pub struct ScriptRunner;

impl TestRunner for ScriptRunner {
    fn name(&self) -> &'static str {
        "script"
    }

    fn run(&self, ctx: &RunContext) -> Result<(Vec<TestResult>, TestSummary)> {
        run_script_tests(
            ctx.godot,
            ctx.project,
            ctx.args,
            ctx.test_files,
            ctx.json_mode,
        )
    }
}

/// Run tests by executing each test script individually with Godot.
#[allow(clippy::too_many_lines, clippy::unnecessary_wraps)]
pub fn run_script_tests(
    godot: &Path,
    project: &GodotProject,
    args: &RunArgs,
    test_files: &[std::path::PathBuf],
    json_mode: bool,
) -> Result<(Vec<TestResult>, TestSummary)> {
    // Apply --name / --class content filtering at file level
    let filtered_files: Vec<std::path::PathBuf>;
    let effective_files = if args.name.is_some() || args.class.is_some() {
        if args.name.is_some() {
            hprintln!(
                json_mode,
                "{} script runner executes entire files; --name filter applied at file level",
                "ℹ".blue()
            );
        }
        let infos = filter_files_by_tests(test_files, args.name.as_deref(), args.class.as_deref());
        filtered_files = infos.into_iter().map(|i| i.path).collect();
        filtered_files.as_slice()
    } else {
        test_files
    };

    let mut passed = 0usize;
    let mut failed = 0usize;
    let mut error_count = 0usize;
    let mut results = Vec::new();

    for (i, test_file) in effective_files.iter().enumerate() {
        let rel = gd_core::fs::relative_slash(test_file, &project.root);
        let label = format!("[{}/{}] {rel}", i + 1, effective_files.len());

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
        let Ok(output) = run_with_timeout(&mut cmd, Duration::from_secs(args.timeout), true) else {
            if let Some(sp) = spinner {
                sp.finish_and_clear();
            }
            failed += 1;
            let test_duration_ms = test_start.elapsed().as_millis() as u64;

            let timeout_result = TestResult {
                file: Some(rel.clone()),
                status: TestStatus::Timeout,
                duration_ms: test_duration_ms,
                errors: vec![],
                stderr: None,
                stdout: None,
            };

            if !json_mode {
                super::print_results(std::slice::from_ref(&timeout_result), args);
            }

            results.push(timeout_result);
            continue;
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
            let one_result = TestResult {
                file: Some(rel.clone()),
                status,
                duration_ms: test_duration_ms,
                errors: errors.clone(),
                stderr: None,
                stdout: None,
            };
            super::print_results(std::slice::from_ref(&one_result), args);

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
                        cprintln!("  {line}");
                    }
                }
                if !display_stderr.is_empty() {
                    for line in display_stderr.lines() {
                        ceprintln!("  {}", line.dimmed());
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
pub fn run_with_timeout(
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
                        Ok(0) | Err(_) => break,
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
