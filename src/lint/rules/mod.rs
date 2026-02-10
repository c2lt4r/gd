/// Lint rules module - each rule analyzes the tree-sitter AST.

/// Severity of a lint diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Warning,
    Error,
}

/// A single lint diagnostic.
#[derive(Debug)]
pub struct LintDiagnostic {
    pub rule: &'static str,
    pub message: String,
    pub severity: Severity,
    pub line: usize,
    pub column: usize,
}
