pub mod diagnostics;
pub mod rules;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use miette::{Result, miette};
use owo_colors::OwoColorize;
use rayon::prelude::*;
use similar::TextDiff;

use crate::core::config::{Config, find_project_root};
use crate::core::fs::collect_gdscript_files;
use crate::core::parser;

use diagnostics::{FileLintResult, print_diagnostic, print_json, print_sarif};
use rules::{Fix, LintDiagnostic, Severity, all_rules};

/// Options bundle for `run_lint()`.
pub struct LintOptions {
    pub format: String,
    pub fix: bool,
    pub dry_run: bool,
    pub severity_filter: Option<Severity>,
    pub rule_filter: Vec<String>,
    pub exclude_patterns: Vec<String>,
    pub exclude_rules: Vec<String>,
    pub summary: bool,
    pub no_fail: bool,
    pub context: Option<usize>,
}

impl Default for LintOptions {
    fn default() -> Self {
        Self {
            format: "human".to_string(),
            fix: false,
            dry_run: false,
            severity_filter: None,
            rule_filter: Vec::new(),
            exclude_patterns: Vec::new(),
            exclude_rules: Vec::new(),
            summary: false,
            no_fail: false,
            context: None,
        }
    }
}

/// Entry point for the linter.
#[allow(clippy::too_many_lines)]
pub fn run_lint(paths: &[String], opts: &LintOptions) -> Result<()> {
    let cwd =
        std::env::current_dir().map_err(|e| miette!("Failed to get current directory: {e}"))?;

    // Load config: search from the first explicit path if given, otherwise cwd
    let config_search_dir = if let Some(first) = paths.first() {
        let p = PathBuf::from(first);
        if p.is_file() {
            p.parent().unwrap_or(&cwd).to_path_buf()
        } else if p.is_dir() {
            p
        } else {
            cwd.clone()
        }
    } else {
        cwd.clone()
    };
    let config = Config::load(&config_search_dir)?;

    // Collect GDScript files
    let files = collect_files(paths, &cwd)?;

    if files.is_empty() {
        eprintln!("{}", "No .gd files found".dimmed());
        return Ok(());
    }

    // Merge rules with severity "off" into disabled list
    let mut disabled = config.lint.disabled_rules.clone();
    for (rule_name, rule_config) in &config.lint.rules {
        if rule_config.severity.as_deref() == Some("off") && !disabled.contains(rule_name) {
            disabled.push(rule_name.clone());
        }
    }
    let rules = all_rules(&disabled, &config.lint.rules);

    // Use project root (not cwd) as base for ignore patterns
    let ignore_base = find_project_root(&config_search_dir).unwrap_or_else(|| cwd.clone());

    // Merge CLI --exclude patterns with config ignore_patterns
    let mut ignore_patterns = config.lint.ignore_patterns.clone();
    ignore_patterns.extend(opts.exclude_patterns.iter().cloned());

    // Process files in parallel, skipping those matching ignore_patterns
    let file_results: Vec<(PathBuf, Vec<LintDiagnostic>)> = files
        .par_iter()
        .filter(|path| !matches_ignore_pattern(path, &ignore_base, &ignore_patterns))
        .filter_map(
            |path| match lint_file(path, &rules, &config, opts, &ignore_base) {
                Ok(diags) => Some((path.clone(), diags)),
                Err(e) => {
                    eprintln!("{}: {}", path.display().red(), e);
                    None
                }
            },
        )
        .collect();

    // Post-collection filtering
    let severity_threshold = opts.severity_filter.unwrap_or(Severity::Info);
    let rule_filter: HashSet<&str> = opts.rule_filter.iter().map(std::string::String::as_str).collect();
    let exclude_rules: HashSet<&str> = opts.exclude_rules.iter().map(std::string::String::as_str).collect();

    let filtered_results: Vec<(PathBuf, Vec<&LintDiagnostic>)> = file_results
        .iter()
        .map(|(path, diags)| {
            let filtered: Vec<&LintDiagnostic> = diags
                .iter()
                .filter(|d| d.severity >= severity_threshold)
                .filter(|d| rule_filter.is_empty() || rule_filter.contains(d.rule))
                .filter(|d| !exclude_rules.contains(d.rule))
                .collect();
            (path.clone(), filtered)
        })
        .collect();

    // Count totals
    let mut total_info = 0usize;
    let mut total_warnings = 0usize;
    let mut total_errors = 0usize;

    for (_, diags) in &filtered_results {
        for d in diags {
            match d.severity {
                Severity::Info => total_info += 1,
                Severity::Warning => total_warnings += 1,
                Severity::Error => total_errors += 1,
            }
        }
    }

    if opts.summary {
        // Summary mode: severity counts with per-rule breakdown
        print_summary(&filtered_results);
    } else {
        // Normal output
        match opts.format.as_str() {
            "json" => {
                let json_results: Vec<FileLintResult> = filtered_results
                    .iter()
                    .filter(|(_, diags)| !diags.is_empty())
                    .map(|(path, diags)| {
                        let source = opts
                            .context
                            .and_then(|_| std::fs::read_to_string(path).ok());
                        let source_lines: Vec<&str> = source
                            .as_deref()
                            .map(|s| s.lines().collect())
                            .unwrap_or_default();
                        FileLintResult {
                            file: path.display().to_string(),
                            diagnostics: diags
                                .iter()
                                .map(|d| {
                                    let context_lines = opts.context.map(|ctx| {
                                        let start = d.line.saturating_sub(ctx);
                                        let end = (d.line + ctx + 1).min(source_lines.len());
                                        source_lines[start..end]
                                            .iter()
                                            .map(std::string::ToString::to_string)
                                            .collect::<Vec<_>>()
                                    });
                                    LintDiagnostic {
                                        rule: d.rule,
                                        message: d.message.clone(),
                                        severity: d.severity,
                                        line: d.line,
                                        column: d.column,
                                        end_column: d.end_column,
                                        fix: None,
                                        context_lines,
                                    }
                                })
                                .collect(),
                        }
                    })
                    .collect();
                print_json(&json_results);
            }
            "sarif" => {
                let rule_names: Vec<&str> = rules.iter().map(|r| r.name()).collect();
                let sarif_results: Vec<FileLintResult> = filtered_results
                    .iter()
                    .filter(|(_, diags)| !diags.is_empty())
                    .map(|(path, diags)| FileLintResult {
                        file: path.display().to_string(),
                        diagnostics: diags
                            .iter()
                            .map(|d| LintDiagnostic {
                                rule: d.rule,
                                message: d.message.clone(),
                                severity: d.severity,
                                line: d.line,
                                column: d.column,
                                end_column: d.end_column,
                                fix: None,
                                context_lines: None,
                            })
                            .collect(),
                    })
                    .collect();
                print_sarif(&sarif_results, &rule_names);
            }
            _ => {
                // Human format
                for (path, diags) in &filtered_results {
                    let source = std::fs::read_to_string(path).ok();
                    for diag in diags {
                        print_diagnostic(path, diag, source.as_deref(), opts.context);
                    }
                }
            }
        }
    }

    let total = total_info + total_warnings + total_errors;
    let is_machine_output = matches!(opts.format.as_str(), "json" | "sarif");
    if !is_machine_output {
        if total > 0 {
            eprintln!(
                "\n{}: {} ({} {}, {} {}, {} {})",
                "lint result".bold(),
                format!("{total} problems").bold(),
                total_errors,
                "errors".red(),
                total_warnings,
                "warnings".yellow(),
                total_info,
                "info".cyan(),
            );
        } else {
            eprintln!("{}", "No lint issues found.".green().bold());
        }
    }

    if total_errors > 0 && !opts.no_fail {
        Err(miette!("Lint found {} error(s)", total_errors))
    } else {
        Ok(())
    }
}

/// Print summary: severity counts with per-rule breakdown, sorted by count descending.
fn print_summary(results: &[(PathBuf, Vec<&LintDiagnostic>)]) {
    let mut by_severity: HashMap<Severity, HashMap<&str, usize>> = HashMap::new();

    for (_, diags) in results {
        for d in diags {
            *by_severity
                .entry(d.severity)
                .or_default()
                .entry(d.rule)
                .or_insert(0) += 1;
        }
    }

    // Print in order: Error, Warning, Info
    for severity in [Severity::Error, Severity::Warning, Severity::Info] {
        if let Some(rule_counts) = by_severity.get(&severity) {
            let total: usize = rule_counts.values().copied().sum();
            let mut sorted: Vec<(&&str, &usize)> = rule_counts.iter().collect();
            sorted.sort_by(|a, b| b.1.cmp(a.1));

            let breakdown: Vec<String> = sorted
                .iter()
                .map(|(rule, count)| format!("{rule}({count})"))
                .collect();

            let label = match severity {
                Severity::Error => format!("{total} errors").red().bold().to_string(),
                Severity::Warning => format!("{total} warnings").yellow().bold().to_string(),
                Severity::Info => format!("{total} info").cyan().bold().to_string(),
            };

            eprintln!("{}: {}", label, breakdown.join(", "));
        }
    }
}

/// Lint a single file. Returns sorted diagnostics.
fn lint_file(
    path: &Path,
    rules: &[Box<dyn rules::LintRule>],
    config: &Config,
    opts: &LintOptions,
    ignore_base: &Path,
) -> Result<Vec<LintDiagnostic>> {
    let (source, tree) = parser::parse_file(path)?;

    let mut all_diags = Vec::new();
    for rule in rules {
        if is_rule_excluded_by_override(path, ignore_base, rule.name(), &config.lint.overrides) {
            continue;
        }
        let diags = rule.check(&tree, &source, &config.lint);
        all_diags.extend(diags);
    }

    // Apply severity overrides from per-rule config
    for diag in &mut all_diags {
        if let Some(rule_config) = config.lint.rules.get(diag.rule) {
            match rule_config.severity.as_deref() {
                Some("info") => diag.severity = Severity::Info,
                Some("warning") => diag.severity = Severity::Warning,
                Some("error") => diag.severity = Severity::Error,
                _ => {}
            }
        }
    }

    // Sort by line, then column
    all_diags.sort_by(|a, b| a.line.cmp(&b.line).then(a.column.cmp(&b.column)));

    // Filter out suppressed diagnostics
    let suppressions = parse_suppressions(&source);
    all_diags.retain(|d| !is_suppressed(d, &suppressions));

    // Apply fixes if requested
    if opts.fix {
        let fixes: Vec<&Fix> = all_diags.iter().filter_map(|d| d.fix.as_ref()).collect();

        if !fixes.is_empty() {
            let fixed_source = apply_fixes(&source, &fixes);

            if opts.dry_run {
                // Show diff instead of writing
                let diff = TextDiff::from_lines(&source, &fixed_source);
                let display_path = path.display();
                eprintln!(
                    "{}",
                    diff.unified_diff()
                        .header(&format!("a/{display_path}"), &format!("b/{display_path}"))
                );
            } else {
                std::fs::write(path, &fixed_source)
                    .map_err(|e| miette!("Failed to write {}: {e}", path.display()))?;
                eprintln!("{}: applied {} fix(es)", path.display(), fixes.len(),);
            }
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

/// Check if a file path matches any of the ignore patterns.
/// Patterns support: `dir/**` (recursive), `*.ext` (extension), exact match.
pub fn matches_ignore_pattern(path: &Path, base: &Path, patterns: &[String]) -> bool {
    if patterns.is_empty() {
        return false;
    }
    // Try plain strip_prefix first (works when both paths share the same root).
    // Only fall back to canonicalize for symlink edge cases (e.g., macOS /var -> /private/var).
    // Avoids Windows canonicalize returning \\?\C:\... extended-length paths that break strip_prefix.
    let relative = path.strip_prefix(base).map_or_else(
        |_| {
            let canon_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
            let canon_base = base.canonicalize().unwrap_or_else(|_| base.to_path_buf());
            canon_path
                .strip_prefix(&canon_base)
                .unwrap_or(&canon_path)
                .to_path_buf()
        },
        std::path::Path::to_path_buf,
    );
    // Normalize to forward slashes so patterns work on Windows
    let rel_str = path_slash::PathExt::to_slash_lossy(relative.as_path());

    for pattern in patterns {
        if pattern.ends_with("/**") {
            // "addons/**" → match anything under addons/
            let prefix = &pattern[..pattern.len() - 3];
            if rel_str.starts_with(prefix) {
                return true;
            }
        } else if let Some(suffix) = pattern.strip_prefix('*') {
            // "*.test.gd" → match files ending with .test.gd
            if rel_str.ends_with(suffix) {
                return true;
            }
        } else if rel_str == pattern.as_str() || rel_str.starts_with(&format!("{pattern}/")) {
            // Exact file match or directory prefix
            return true;
        }
    }
    false
}

/// Check if a lint rule should be skipped for a file due to `[[lint.overrides]]`.
pub fn is_rule_excluded_by_override(
    path: &Path,
    base: &Path,
    rule_name: &str,
    overrides: &[crate::core::config::LintOverride],
) -> bool {
    overrides.iter().any(|ov| {
        ov.exclude_rules.iter().any(|r| r == rule_name)
            && matches_ignore_pattern(path, base, &ov.paths)
    })
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
