use clap::Args;
use miette::Result;

use gd_lint::LintOptions;
use gd_lint::rules::{LintDiagnostic, Severity};

use super::check_cmd;

#[derive(Args)]
#[command(after_long_help = "\
INLINE SUPPRESSION:
  # gd:ignore                       Suppress all warnings on this line
  # gd:ignore[rule-name]            Suppress a specific rule on this line
  # gd:ignore[rule-a, rule-b]       Suppress multiple rules on this line
  # gd:ignore-next-line             Suppress all warnings on the next line
  # gd:ignore-next-line[rule-name]  Suppress a specific rule on the next line
")]
pub struct LintArgs {
    /// Files or directories to lint (defaults to current directory)
    pub paths: Vec<String>,
    /// Output format
    #[arg(long, default_value = "text")]
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
        .map(str::parse::<Severity>)
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

    let extra: &gd_lint::ExtraDiagnosticsFn = &|file, source, project| {
        check_cmd::check_classdb_errors(file, source, project)
            .into_iter()
            .map(|err| LintDiagnostic {
                rule: "compiler-error",
                message: err.message,
                severity: Severity::Error,
                line: err.line as usize,
                column: err.column as usize,
                end_column: None,
                fix: None,
                context_lines: None,
            })
            .collect()
    };

    gd_lint::run_lint(&args.paths, &opts, Some(extra))
}
