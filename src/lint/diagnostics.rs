use owo_colors::OwoColorize;
use serde_json::json;
use std::path::Path;

use super::rules::{LintDiagnostic, Severity};
use crate::{ceprintln, cprintln};

/// Print a diagnostic in human-readable format with optional source spans.
pub fn print_diagnostic(
    path: &Path,
    diag: &LintDiagnostic,
    source: Option<&str>,
    context: Option<usize>,
) {
    let severity_str = match diag.severity {
        Severity::Info => "info".cyan().bold().to_string(),
        Severity::Warning => "warning".yellow().bold().to_string(),
        Severity::Error => "error".red().bold().to_string(),
    };
    let location = format!("{}:{}:{}", path.display(), diag.line + 1, diag.column + 1);
    ceprintln!(
        "{} {}: {} [{}]",
        location.bold(),
        severity_str,
        diag.message,
        diag.rule.dimmed(),
    );

    let Some(source) = source else { return };
    let lines: Vec<&str> = source.lines().collect();

    if let Some(ctx) = context {
        // Context mode: show N lines before and after the diagnostic line
        let start = diag.line.saturating_sub(ctx);
        let end = (diag.line + ctx + 1).min(lines.len());
        let max_line_num = end; // 1-indexed
        let gutter_width = format!("{max_line_num}").len();

        ceprintln!("{:>width$} {}", "", "|".cyan(), width = gutter_width);

        for (i, line) in lines[start..end].iter().enumerate() {
            let line_idx = start + i;
            let num = format!("{}", line_idx + 1);
            if line_idx == diag.line {
                // Highlight the diagnostic line
                ceprintln!("{} {} {}", num.cyan().bold(), "|".cyan(), line,);
                // Print underline if we have end_column
                if let Some(end_col) = diag.end_column {
                    let col = diag.column;
                    let span_len = if end_col > col { end_col - col } else { 1 };
                    let underline = "^".repeat(span_len);
                    let colored_underline = match diag.severity {
                        Severity::Info => underline.cyan().bold().to_string(),
                        Severity::Warning => underline.yellow().bold().to_string(),
                        Severity::Error => underline.red().bold().to_string(),
                    };
                    ceprintln!(
                        "{:>width$} {} {:>col$}{}",
                        "",
                        "|".cyan(),
                        "",
                        colored_underline,
                        width = gutter_width,
                        col = col,
                    );
                }
            } else {
                ceprintln!("{} {} {}", num.dimmed(), "|".cyan(), line.dimmed(),);
            }
        }
    } else if let Some(end_col) = diag.end_column
        && let Some(line_text) = lines.get(diag.line)
    {
        // Default: show just the diagnostic line with underline (existing behavior)
        let line_num = format!("{}", diag.line + 1);
        let gutter_width = line_num.len();

        ceprintln!("{:>width$} {}", "", "|".cyan(), width = gutter_width);
        ceprintln!("{} {} {}", line_num.cyan(), "|".cyan(), line_text);

        let col = diag.column;
        let span_len = if end_col > col { end_col - col } else { 1 };
        let underline = "^".repeat(span_len);
        let colored_underline = match diag.severity {
            Severity::Info => underline.cyan().bold().to_string(),
            Severity::Warning => underline.yellow().bold().to_string(),
            Severity::Error => underline.red().bold().to_string(),
        };
        ceprintln!(
            "{:>width$} {} {:>col$}{}",
            "",
            "|".cyan(),
            "",
            colored_underline,
            width = gutter_width,
            col = col,
        );
    }
}

/// Serializable file-level result for JSON output.
#[derive(serde::Serialize)]
pub struct FileLintResult {
    pub file: String,
    pub diagnostics: Vec<LintDiagnostic>,
}

/// Print all results as JSON.
pub fn print_json(results: &[FileLintResult]) {
    let json = serde_json::to_string_pretty(results).unwrap_or_else(|_| "[]".to_string());
    cprintln!("{json}");
}

/// Get a short description for a lint rule.
fn rule_description(name: &str) -> &'static str {
    match name {
        "naming-convention" => "Check snake_case/PascalCase naming conventions",
        "unused-variable" => "Detect assigned but unused variables",
        "missing-type-hint" => "Warn on missing parameter and return type hints",
        "empty-function" => "Detect functions with only pass in body",
        "long-function" => "Warn on functions exceeding line threshold",
        "duplicate-signal" => "Detect duplicate signal declarations",
        "self-assignment" => "Detect variables assigned to themselves",
        "unreachable-code" => "Detect code after return/break/continue",
        "shadowed-variable" => "Detect variable shadowing in inner scopes",
        "comparison-with-boolean" => "Flag explicit == true/false comparisons",
        "unnecessary-pass" => "Detect pass in non-empty function bodies",
        "preload-type-hint" => "Warn on untyped preload/load assignments",
        "integer-division" => "Warn on integer literal division truncation",
        "signal-name-convention" => "Warn on signals with on_ prefix",
        "magic-number" => "Flag unexplained numeric literals in functions",
        "float-comparison" => "Warn on float equality comparisons",
        "missing-super-call" => "Warn on lifecycle overrides without super()",
        "return-type-mismatch" => "Detect void/non-void return mismatches",
        "private-method-access" => "Warn on calling private methods externally",
        "untyped-array" => "Suggest typed array annotations",
        "duplicate-function" => "Detect duplicate function definitions",
        "unused-signal" => "Detect signals that are never emitted",
        "duplicate-key" => "Detect duplicate dictionary keys",
        "await-in-ready" => "Warn about await in _ready()",
        "missing-return" => "Detect missing return in typed functions",
        "unused-preload" => "Detect preloaded variables that are never used",
        "static-type-inference" => "Suggest explicit types for trivially inferable literals",
        "node-ready-order" => "Warn on node access in _init() before tree is ready",
        _ => "Lint rule",
    }
}

/// Print all results as SARIF 2.1.0 JSON for GitHub Code Scanning.
pub fn print_sarif(results: &[FileLintResult], rules: &[&str]) {
    let sarif_rules: Vec<serde_json::Value> = rules
        .iter()
        .map(|name| {
            json!({
                "id": name,
                "shortDescription": { "text": rule_description(name) }
            })
        })
        .collect();

    let mut sarif_results = Vec::new();
    for file_result in results {
        for diag in &file_result.diagnostics {
            let mut region = json!({
                "startLine": diag.line + 1,
                "startColumn": diag.column + 1
            });
            if let Some(end_col) = diag.end_column {
                region["endColumn"] = json!(end_col + 1);
            }

            sarif_results.push(json!({
                "ruleId": diag.rule,
                "level": match diag.severity {
                    super::rules::Severity::Info => "note",
                    super::rules::Severity::Warning => "warning",
                    super::rules::Severity::Error => "error",
                },
                "message": { "text": diag.message },
                "locations": [{
                    "physicalLocation": {
                        "artifactLocation": {
                            "uri": file_result.file,
                            "uriBaseId": "%SRCROOT%"
                        },
                        "region": region
                    }
                }]
            }));
        }
    }

    let sarif = json!({
        "$schema": "https://raw.githubusercontent.com/oasis-tcs/sarif-spec/main/sarif-2.1/schema/sarif-schema-2.1.0.json",
        "version": "2.1.0",
        "runs": [{
            "tool": {
                "driver": {
                    "name": "gd",
                    "version": env!("CARGO_PKG_VERSION"),
                    "rules": sarif_rules
                }
            },
            "results": sarif_results
        }]
    });

    cprintln!("{}", serde_json::to_string_pretty(&sarif).unwrap());
}
