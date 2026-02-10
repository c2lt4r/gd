/// Diagnostic reporting for lint results.
use owo_colors::OwoColorize;
use std::path::Path;

use super::rules::{LintDiagnostic, Severity};

pub fn print_diagnostic(path: &Path, diag: &LintDiagnostic) {
    let severity_str = match diag.severity {
        Severity::Warning => "warning".yellow().bold().to_string(),
        Severity::Error => "error".red().bold().to_string(),
    };
    eprintln!(
        "{}:{}:{} {}: {} [{}]",
        path.display(),
        diag.line + 1,
        diag.column + 1,
        severity_str,
        diag.message,
        diag.rule,
    );
}
