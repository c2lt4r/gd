//! Capture types for SSR match results.

use std::collections::HashMap;
use std::ops::Range;
use std::path::PathBuf;

// ═══════════════════════════════════════════════════════════════════════
//  Captured subtree
// ═══════════════════════════════════════════════════════════════════════

/// A single captured expression subtree.
///
/// Stores the byte range and original source text — enough for
/// replacement (Phase 3) and repeated-placeholder verification.
#[derive(Debug, Clone)]
pub struct CapturedExpr {
    /// Byte range in the original source.
    pub byte_range: Range<usize>,
    /// Original source text of the captured subtree.
    pub source_text: String,
}

// ═══════════════════════════════════════════════════════════════════════
//  Capture (single expr or variadic arg list)
// ═══════════════════════════════════════════════════════════════════════

/// What a single placeholder captured.
#[derive(Debug, Clone)]
pub enum Capture {
    /// A single expression subtree (from `$name`).
    Expr(CapturedExpr),
    /// Zero or more argument expressions (from `$$name`).
    ArgList(Vec<CapturedExpr>),
}

// ═══════════════════════════════════════════════════════════════════════
//  Match result
// ═══════════════════════════════════════════════════════════════════════

/// A single match found in a file.
#[derive(Debug, Clone)]
pub struct MatchResult {
    /// Placeholder name → captured subtree(s).
    pub captures: HashMap<String, Capture>,
    /// Byte range of the entire matched expression/statement in source.
    pub matched_range: Range<usize>,
    /// Line number (1-based) of the match start.
    pub line: usize,
    /// File path (filled by the caller).
    pub file: PathBuf,
}
