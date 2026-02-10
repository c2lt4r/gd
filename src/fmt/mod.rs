pub mod rules;
pub mod printer;

use miette::Result;

/// Entry point for the formatter.
pub fn run_fmt(paths: &[String], check: bool, diff: bool) -> Result<()> {
    let _ = (paths, check, diff);
    todo!("Formatter not yet implemented")
}
