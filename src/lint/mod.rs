pub mod rules;
pub mod diagnostics;

use miette::Result;

/// Entry point for the linter.
pub fn run_lint(paths: &[String], format: &str, fix: bool) -> Result<()> {
    let _ = (paths, format, fix);
    todo!("Linter not yet implemented")
}
