/// Lint rules module - each rule analyzes the tree-sitter AST.

pub mod naming_convention;
pub mod unused_variable;
pub mod missing_type_hint;
pub mod empty_function;
pub mod long_function;
pub mod duplicate_signal;
pub mod self_assignment;
pub mod unreachable_code;
pub mod shadowed_variable;
pub mod comparison_with_boolean;
pub mod unnecessary_pass;
pub mod preload_type_hint;
pub mod integer_division;
pub mod signal_name_convention;
pub mod magic_number;

use tree_sitter::Tree;

use crate::core::config::LintConfig;

/// Severity of a lint diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Warning,
    Error,
}

/// A single lint diagnostic.
#[derive(Debug, serde::Serialize)]
pub struct LintDiagnostic {
    pub rule: &'static str,
    pub message: String,
    pub severity: Severity,
    pub line: usize,
    pub column: usize,
    /// If this diagnostic is auto-fixable, this holds the replacement.
    #[serde(skip)]
    pub fix: Option<Fix>,
}

/// An auto-fix: replace a byte range with new text.
#[derive(Debug, Clone)]
pub struct Fix {
    pub byte_start: usize,
    pub byte_end: usize,
    pub replacement: String,
}

/// Trait implemented by each lint rule.
pub trait LintRule: Send + Sync {
    /// Unique rule identifier (e.g. "naming-convention").
    fn name(&self) -> &'static str;

    /// Run the rule against a parsed file and return diagnostics.
    fn check(&self, tree: &Tree, source: &str, config: &LintConfig) -> Vec<LintDiagnostic>;
}

/// Return all built-in rules, excluding those listed in `disabled`.
pub fn all_rules(disabled: &[String]) -> Vec<Box<dyn LintRule>> {
    let all: Vec<Box<dyn LintRule>> = vec![
        Box::new(naming_convention::NamingConvention),
        Box::new(unused_variable::UnusedVariable),
        Box::new(missing_type_hint::MissingTypeHint),
        Box::new(empty_function::EmptyFunction),
        Box::new(long_function::LongFunction),
        Box::new(duplicate_signal::DuplicateSignal),
        Box::new(self_assignment::SelfAssignment),
        Box::new(unreachable_code::UnreachableCode),
        Box::new(shadowed_variable::ShadowedVariable),
        Box::new(comparison_with_boolean::ComparisonWithBoolean),
        Box::new(unnecessary_pass::UnnecessaryPass),
        Box::new(preload_type_hint::PreloadTypeHint),
        Box::new(integer_division::IntegerDivision),
        Box::new(signal_name_convention::SignalNameConvention),
        Box::new(magic_number::MagicNumber),
    ];
    all.into_iter()
        .filter(|r| !disabled.iter().any(|d| d == r.name()))
        .collect()
}
