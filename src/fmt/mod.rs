pub mod printer;
pub mod rules;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use miette::{Result, miette};
use owo_colors::OwoColorize;
use rayon::prelude::*;
use similar::TextDiff;

use crate::core::config::Config;
use crate::core::fs::collect_gdscript_files;
use crate::core::parser;
use printer::Printer;

/// Entry point for the formatter.
pub fn run_fmt(paths: &[String], check: bool, diff: bool) -> Result<()> {
    let cwd =
        std::env::current_dir().map_err(|e| miette!("Failed to get current directory: {e}"))?;
    let config = Config::load(&cwd)?;

    let files = collect_files(paths, &cwd)?;

    if files.is_empty() {
        eprintln!("{}", "No .gd files found.".yellow());
        return Ok(());
    }

    let has_changes = AtomicBool::new(false);
    let has_errors = AtomicBool::new(false);

    files
        .par_iter()
        .for_each(|path| match format_file(path, &config, check, diff) {
            Ok(changed) => {
                if changed {
                    has_changes.store(true, Ordering::Relaxed);
                }
            }
            Err(e) => {
                eprintln!("{}: {e}", path.display().red());
                has_errors.store(true, Ordering::Relaxed);
            }
        });

    if has_errors.load(Ordering::Relaxed) {
        return Err(miette!("Some files had errors during formatting"));
    }

    if check && has_changes.load(Ordering::Relaxed) {
        return Err(miette!("Some files need formatting. Run `gd fmt` to fix."));
    }

    if !check && !diff {
        let count = files.len();
        eprintln!(
            "{} Formatted {} file{}.",
            "done".green().bold(),
            count,
            if count == 1 { "" } else { "s" }
        );
    }

    Ok(())
}

/// Collect .gd files from the given paths (or cwd if empty).
fn collect_files(paths: &[String], cwd: &Path) -> Result<Vec<PathBuf>> {
    if paths.is_empty() {
        return collect_gdscript_files(cwd);
    }

    let mut files = Vec::new();
    for p in paths {
        let path = PathBuf::from(p);
        if path.is_file() {
            if path.extension().is_some_and(|ext| ext == "gd") {
                files.push(path);
            } else {
                return Err(miette!("Not a .gd file: {}", path.display()));
            }
        } else if path.is_dir() {
            files.extend(collect_gdscript_files(&path)?);
        } else {
            return Err(miette!("Path not found: {}", path.display()));
        }
    }
    files.sort();
    Ok(files)
}

/// Format a single file. Returns true if the file was changed (or would be in check mode).
fn format_file(path: &Path, config: &Config, check: bool, show_diff: bool) -> Result<bool> {
    let source = std::fs::read_to_string(path)
        .map_err(|e| miette!("Failed to read {}: {e}", path.display()))?;

    let tree = parser::parse(&source)?;
    let mut printer = Printer::from_config(&config.fmt);
    printer.format(&tree.root_node(), &source);
    let formatted = printer.finish();

    // Check line lengths (informational only)
    for (i, line) in formatted.lines().enumerate() {
        if line.len() > config.fmt.max_line_length {
            eprintln!(
                "  {}: line {} exceeds {} chars ({} chars)",
                path.display().dimmed(),
                i + 1,
                config.fmt.max_line_length,
                line.len()
            );
        }
    }

    if source == formatted {
        return Ok(false);
    }

    if check {
        eprintln!("{} {}", "would reformat".yellow().bold(), path.display());
    }

    if show_diff {
        print_diff(path, &source, &formatted);
    }

    if !check && !show_diff {
        std::fs::write(path, &formatted)
            .map_err(|e| miette!("Failed to write {}: {e}", path.display()))?;
        eprintln!("{} {}", "formatted".green(), path.display());
    } else if show_diff && !check {
        // --diff without --check: show diff AND write
        std::fs::write(path, &formatted)
            .map_err(|e| miette!("Failed to write {}: {e}", path.display()))?;
    }

    Ok(true)
}

/// Print a unified diff of changes using the `similar` crate.
fn print_diff(path: &Path, old: &str, new: &str) {
    let diff = TextDiff::from_lines(old, new);
    let display_path = path.display();

    println!("{}", format!("--- {display_path} (original)").red());
    println!("{}", format!("+++ {display_path} (formatted)").green());

    for hunk in diff.unified_diff().context_radius(3).iter_hunks() {
        println!("{}", format!("{hunk}").cyan());
    }
}
