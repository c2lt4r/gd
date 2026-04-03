//! SSR pattern and template data structures.

use std::collections::{HashMap, HashSet};

use crate::ast_owned::{OwnedExpr, OwnedStmt};

// ═══════════════════════════════════════════════════════════════════════
//  Pattern kind
// ═══════════════════════════════════════════════════════════════════════

/// Whether a pattern matches expressions or statements.
#[derive(Debug, Clone)]
pub enum PatternKind {
    /// Matches expression nodes.
    Expr(OwnedExpr),
    /// Matches statement nodes.
    Stmt(Box<OwnedStmt>),
}

/// Prefix for regular placeholder sentinels (`$name` → `__ssr_name`).
pub(super) const SSR_PREFIX: &str = "__ssr_";

/// Prefix for variadic placeholder sentinels (`$$name` → `__ssrv_name`).
pub(super) const SSRV_PREFIX: &str = "__ssrv_";

// ═══════════════════════════════════════════════════════════════════════
//  Placeholder info
// ═══════════════════════════════════════════════════════════════════════

/// Metadata about a single placeholder in a pattern.
#[derive(Debug, Clone)]
pub struct PlaceholderInfo {
    /// True for `$$args` — matches zero or more expressions in a call.
    pub variadic: bool,
    /// Type constraint from `$x:Type` syntax.  `None` = match anything.
    /// Resolved in Phase 4.
    pub type_constraint: Option<String>,
}

// ═══════════════════════════════════════════════════════════════════════
//  SSR pattern (search side)
// ═══════════════════════════════════════════════════════════════════════

/// A parsed SSR search pattern.
#[derive(Debug, Clone)]
pub struct SsrPattern {
    /// The pattern AST (single expression or statement).
    pub kind: PatternKind,
    /// Placeholder names → info (variadic flag, type constraint).
    pub placeholders: HashMap<String, PlaceholderInfo>,
    /// Original pattern string (for error messages).
    pub source: String,
}

// ═══════════════════════════════════════════════════════════════════════
//  SSR template (replace side)
// ═══════════════════════════════════════════════════════════════════════

/// A parsed SSR replacement template.
#[derive(Debug, Clone)]
pub struct SsrTemplate {
    /// The template AST (single expression or statement).
    pub kind: PatternKind,
    /// Set of placeholder names used in this template.
    pub placeholders: HashSet<String>,
    /// Original template string (for error messages).
    pub source: String,
}
