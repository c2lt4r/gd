use clap::Args;
use miette::Result;

use crate::lint::LintOptions;
use crate::lint::rules::Severity;

#[derive(Args)]
pub struct LintArgs {
    /// Files or directories to lint (defaults to current directory)
    pub paths: Vec<String>,
    /// Output format
    #[arg(long, default_value = "human")]
    pub format: String,
    /// Fix auto-fixable issues
    #[arg(long)]
    pub fix: bool,
    /// Preview fixes as a diff without writing (requires --fix)
    #[arg(long)]
    pub dry_run: bool,
    /// Minimum severity to show: info, warning, error
    #[arg(long)]
    pub severity: Option<String>,
    /// Only show diagnostics from these rules (comma-separated, repeatable)
    #[arg(long)]
    pub rule: Vec<String>,
    /// Exclude files matching these patterns (repeatable, same syntax as ignore_patterns)
    #[arg(long)]
    pub exclude: Vec<String>,
    /// Exclude diagnostics from these rules (repeatable)
    #[arg(long)]
    pub exclude_rule: Vec<String>,
    /// Show only summary counts per severity and rule
    #[arg(long)]
    pub summary: bool,
    /// Exit 0 even when errors are found
    #[arg(long)]
    pub no_fail: bool,
    /// Show N lines of surrounding context for each diagnostic (like grep -C)
    #[arg(long)]
    pub context: Option<usize>,
}

pub fn exec(args: LintArgs) -> Result<()> {
    let severity_filter = args
        .severity
        .as_deref()
        .map(|s| s.parse::<Severity>())
        .transpose()
        .map_err(|e| miette::miette!("{e}"))?;

    let rule_filter: Vec<String> = args
        .rule
        .iter()
        .flat_map(|s| s.split(',').map(|r| r.trim().to_string()))
        .collect();

    let opts = LintOptions {
        format: args.format,
        fix: args.fix,
        dry_run: args.dry_run,
        severity_filter,
        rule_filter,
        exclude_patterns: args.exclude,
        exclude_rules: args.exclude_rule,
        summary: args.summary,
        no_fail: args.no_fail,
        context: args.context,
    };

    crate::lint::run_lint(&args.paths, &opts)
}
