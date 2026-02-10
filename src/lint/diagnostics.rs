use owo_colors::OwoColorize;
use std::path::Path;

use super::rules::{LintDiagnostic, Severity};

/// Print a diagnostic in human-readable format.
pub fn print_diagnostic(path: &Path, diag: &LintDiagnostic) {
    let severity_str = match diag.severity {
        Severity::Warning => "warning".yellow().bold().to_string(),
        Severity::Error => "error".red().bold().to_string(),
    };
    let location = format!("{}:{}:{}", path.display(), diag.line + 1, diag.column + 1);
    eprintln!(
        "{} {}: {} [{}]",
        location.bold(),
        severity_str,
        diag.message,
        diag.rule.dimmed(),
    );
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
    println!("{}", json);
}
