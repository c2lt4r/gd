pub mod rules;
pub mod diagnostics;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use miette::{miette, Result};
use owo_colors::OwoColorize;
use rayon::prelude::*;

use crate::core::config::Config;
use crate::core::fs::collect_gdscript_files;
use crate::core::parser;

use diagnostics::{print_diagnostic, print_json, print_sarif, FileLintResult};
use rules::{all_rules, Fix, LintDiagnostic, Severity};

/// Entry point for the linter.
pub fn run_lint(paths: &[String], format: &str, fix: bool) -> Result<()> {
    let cwd = std::env::current_dir()
        .map_err(|e| miette!("Failed to get current directory: {e}"))?;
    let config = Config::load(&cwd)?;

    // Collect GDScript files
    let files = collect_files(paths, &cwd)?;

    if files.is_empty() {
        eprintln!("{}", "No .gd files found".dimmed());
        return Ok(());
    }

    let rules = all_rules(&config.lint.disabled_rules);

    // Process files in parallel
    let file_results: Vec<(PathBuf, Vec<LintDiagnostic>)> = files
        .par_iter()
        .filter_map(|path| {
            match lint_file(path, &rules, &config, fix) {
                Ok(diags) => Some((path.clone(), diags)),
                Err(e) => {
                    eprintln!("{}: {}", path.display().red(), e);
                    None
                }
            }
        })
        .collect();

    // Output results
    let mut total_warnings = 0usize;
    let mut total_errors = 0usize;

    match format {
        "json" => {
            let json_results: Vec<FileLintResult> = file_results
                .iter()
                .filter(|(_, diags)| !diags.is_empty())
                .map(|(path, diags)| {
                    for d in diags {
                        match d.severity {
                            Severity::Warning => total_warnings += 1,
                            Severity::Error => total_errors += 1,
                        }
                    }
                    FileLintResult {
                        file: path.display().to_string(),
                        diagnostics: diags.iter().map(|d| LintDiagnostic {
                            rule: d.rule,
                            message: d.message.clone(),
                            severity: d.severity,
                            line: d.line,
                            column: d.column,
                            end_column: d.end_column,
                            fix: None,
                        }).collect(),
                    }
                })
                .collect();
            print_json(&json_results);
        }
        "sarif" => {
            // Collect rule names for SARIF tool.driver.rules
            let rule_names: Vec<&str> = rules.iter().map(|r| r.name()).collect();

            let sarif_results: Vec<FileLintResult> = file_results
                .iter()
                .filter(|(_, diags)| !diags.is_empty())
                .map(|(path, diags)| {
                    for d in diags {
                        match d.severity {
                            Severity::Warning => total_warnings += 1,
                            Severity::Error => total_errors += 1,
                        }
                    }
                    FileLintResult {
                        file: path.display().to_string(),
                        diagnostics: diags.iter().map(|d| LintDiagnostic {
                            rule: d.rule,
                            message: d.message.clone(),
                            severity: d.severity,
                            line: d.line,
                            column: d.column,
                            end_column: d.end_column,
                            fix: None,
                        }).collect(),
                    }
                })
                .collect();
            print_sarif(&sarif_results, &rule_names);
        }
        _ => {
            // Human format - read source for span display
            for (path, diags) in &file_results {
                let source = std::fs::read_to_string(path).ok();
                for diag in diags {
                    match diag.severity {
                        Severity::Warning => total_warnings += 1,
                        Severity::Error => total_errors += 1,
                    }
                    print_diagnostic(path, diag, source.as_deref());
                }
            }
        }
    }

    let total = total_warnings + total_errors;
    if total > 0 {
        eprintln!(
            "\n{}: {} ({} {}, {} {})",
            "lint result".bold(),
            format!("{} problems", total).bold(),
            total_errors,
            "errors".red(),
            total_warnings,
            "warnings".yellow(),
        );
    } else {
        eprintln!("{}", "No lint issues found.".green().bold());
    }

    if total_errors > 0 {
        Err(miette!("Lint found {} error(s)", total_errors))
    } else {
        Ok(())
    }
}

/// Lint a single file. Returns sorted diagnostics.
fn lint_file(
    path: &Path,
    rules: &[Box<dyn rules::LintRule>],
    config: &Config,
    fix: bool,
) -> Result<Vec<LintDiagnostic>> {
    let (source, tree) = parser::parse_file(path)?;

    let mut all_diags = Vec::new();
    for rule in rules {
        let diags = rule.check(&tree, &source, &config.lint);
        all_diags.extend(diags);
    }

    // Sort by line, then column
    all_diags.sort_by(|a, b| a.line.cmp(&b.line).then(a.column.cmp(&b.column)));

    // Filter out suppressed diagnostics
    let suppressions = parse_suppressions(&source);
    all_diags.retain(|d| !is_suppressed(d, &suppressions));

    // Apply fixes if requested
    if fix {
        let fixes: Vec<&Fix> = all_diags
            .iter()
            .filter_map(|d| d.fix.as_ref())
            .collect();

        if !fixes.is_empty() {
            let fixed_source = apply_fixes(&source, &fixes);
            std::fs::write(path, &fixed_source)
                .map_err(|e| miette!("Failed to write {}: {e}", path.display()))?;
            eprintln!(
                "{}: applied {} fix(es)",
                path.display(),
                fixes.len(),
            );
        }
    }

    Ok(all_diags)
}

/// Parse suppression comments from source code.
/// Returns a map of line numbers (0-indexed, matching LintDiagnostic) to rule suppressions (None = suppress all).
fn parse_suppressions(source: &str) -> HashMap<usize, Option<HashSet<String>>> {
    let mut suppressions = HashMap::new();

    for (line_idx, line) in source.lines().enumerate() {
        // Look for "# gd:ignore" patterns in the line
        if let Some(pos) = line.find("# gd:ignore") {
            let rest = &line[pos + "# gd:ignore".len()..];

            if let Some(rule_rest) = rest.strip_prefix("-next-line") {
                // Applies to the next line (0-indexed)
                let rules = parse_rule_list(rule_rest);
                suppressions.insert(line_idx + 1, rules);
            } else {
                // Applies to current line (0-indexed)
                let rules = parse_rule_list(rest);
                suppressions.insert(line_idx, rules);
            }
        }
    }

    suppressions
}

/// Parse the rule list from a suppression comment.
/// Returns None for "suppress all", Some(set) for specific rules.
fn parse_rule_list(text: &str) -> Option<HashSet<String>> {
    let text = text.trim();
    if text.starts_with('[') {
        if let Some(end) = text.find(']') {
            let rules_str = &text[1..end];
            let rules: HashSet<String> = rules_str
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            if rules.is_empty() { None } else { Some(rules) }
        } else {
            None // malformed, suppress all
        }
    } else {
        None // no bracket = suppress all
    }
}

/// Check if a diagnostic is suppressed by suppression comments.
fn is_suppressed(
    diag: &LintDiagnostic,
    suppressions: &HashMap<usize, Option<HashSet<String>>>,
) -> bool {
    if let Some(rules) = suppressions.get(&diag.line) {
        match rules {
            None => true, // suppress all
            Some(rule_set) => rule_set.contains(diag.rule),
        }
    } else {
        false
    }
}

/// Apply non-overlapping fixes to source code (from last to first to preserve offsets).
fn apply_fixes(source: &str, fixes: &[&Fix]) -> String {
    let mut sorted: Vec<&&Fix> = fixes.iter().collect();
    sorted.sort_by(|a, b| b.byte_start.cmp(&a.byte_start));

    // Remove overlapping fixes (keep the first one encountered = last in original order)
    let mut result = source.to_string();
    let mut last_start = usize::MAX;
    for fix in sorted {
        if fix.byte_end <= last_start {
            result.replace_range(fix.byte_start..fix.byte_end, &fix.replacement);
            last_start = fix.byte_start;
        }
    }
    result
}

/// Collect .gd files from the given paths, or from cwd if none specified.
fn collect_files(paths: &[String], cwd: &Path) -> Result<Vec<PathBuf>> {
    if paths.is_empty() {
        return collect_gdscript_files(cwd);
    }

    let mut files = Vec::new();
    for p in paths {
        let path = PathBuf::from(p);
        if path.is_file() {
            if path.extension().is_some_and(|e| e == "gd") {
                files.push(path);
            }
        } else if path.is_dir() {
            files.extend(collect_gdscript_files(&path)?);
        } else {
            return Err(miette!("Path not found: {}", p));
        }
    }
    files.sort();
    Ok(files)
}
