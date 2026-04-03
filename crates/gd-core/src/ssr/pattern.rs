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

// ═══════════════════════════════════════════════════════════════════════
//  Sentinel prefixes + shared helpers
// ═══════════════════════════════════════════════════════════════════════

/// Prefix for regular placeholder sentinels (`$name` → `__ssr_name`).
pub(super) const SSR_PREFIX: &str = "__ssr_";

/// Prefix for variadic placeholder sentinels (`$$name` → `__ssrv_name`).
pub(super) const SSRV_PREFIX: &str = "__ssrv_";

/// Returns `true` if `b` can start a GDScript identifier.
pub(super) fn is_ident_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_'
}

/// Returns `true` if `b` can continue a GDScript identifier.
pub(super) fn is_ident_continue(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

// ═══════════════════════════════════════════════════════════════════════
//  Type constraints
// ═══════════════════════════════════════════════════════════════════════

/// A type constraint on an SSR placeholder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeConstraint {
    /// `$x:Node` — nominal type check with inheritance.
    Nominal(String),
    /// `$x:{predicate}` — structural/duck-typing check.
    Structural(StructuralPredicate),
    /// `$x:Variant` — must be dynamically typed.
    VariantOnly,
}

/// A structural predicate for duck-typing constraints.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StructuralPredicate {
    /// `has_method("name")` — type has a method with this name.
    HasMethod(String),
    /// `has_property("name")` — type has a property/variable with this name.
    HasProperty(String),
    /// `has_signal("name")` — type has a signal with this name.
    HasSignal(String),
    /// `extends("ClassName")` — alias for nominal constraint.
    Extends(String),
}

// ═══════════════════════════════════════════════════════════════════════
//  Placeholder info
// ═══════════════════════════════════════════════════════════════════════

/// Metadata about a single placeholder in a pattern.
#[derive(Debug, Clone)]
pub struct PlaceholderInfo {
    /// True for `$$args` — matches zero or more expressions in a call.
    pub variadic: bool,
    /// Type constraint from `$x:Type` or `$x:{predicate}` syntax.
    /// `None` = match anything.
    pub constraint: Option<TypeConstraint>,
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
