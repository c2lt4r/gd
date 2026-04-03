//! Structured Search & Replace (SSR) for GDScript.
//!
//! Pattern-based AST search and replace.  Write a GDScript-like template
//! with `$placeholders`, find all structural matches in the project, and
//! optionally rewrite them with a replacement template.
//!
//! # Phases
//!
//! This module is built incrementally:
//! - **Phase 1**: pattern language and parser.
//! - **Phase 2**: structural matcher.
//! - **Phase 3**: replacement engine.
//! - **Phase 4** (this): type-aware constraints.
//! - Phase 5: CLI integration.

mod captures;
mod constraints;
mod equality;
mod matcher;
mod parse;
mod pattern;
mod preview;
mod replace;

pub use captures::{Capture, CapturedExpr, MatchResult};
pub use constraints::satisfies_constraints;
pub use equality::{structurally_equal_expr, structurally_equal_stmt};
pub use matcher::find_matches;
pub use parse::{parse_pattern, parse_template};
pub use pattern::{
    PatternKind, PlaceholderInfo, SsrPattern, SsrTemplate, StructuralPredicate, TypeConstraint,
};
pub use preview::{FilePreview, MatchPreview, SsrPreview};
pub use replace::{apply_replacements, render_replacement};

/// Find matches with type constraint filtering.
///
/// Convenience wrapper that runs structural matching (Phase 2) followed
/// by type constraint filtering (Phase 4).  Matches that don't satisfy
/// their placeholder type constraints are removed.
pub fn find_matches_constrained(
    pattern: &SsrPattern,
    file: &crate::gd_ast::GdFile<'_>,
    source: &str,
    file_path: std::path::PathBuf,
    project: Option<&crate::workspace_index::ProjectIndex>,
) -> Vec<MatchResult> {
    let mut results = find_matches(pattern, file, source, file_path);

    // If any placeholders have constraints, filter matches.
    let has_constraints = pattern
        .placeholders
        .values()
        .any(|p| p.constraint.is_some());
    if has_constraints {
        results.retain(|m| {
            satisfies_constraints(&m.captures, &pattern.placeholders, file, source, project)
        });
    }

    results
}

#[cfg(test)]
mod tests;
