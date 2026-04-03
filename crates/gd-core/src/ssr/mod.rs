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
//! - **Phase 3** (this): replacement engine.
//! - Phase 4: type-aware constraints.
//! - Phase 5: CLI integration.

mod captures;
mod equality;
mod matcher;
mod parse;
mod pattern;
mod preview;
mod replace;

pub use captures::{Capture, CapturedExpr, MatchResult};
pub use equality::{structurally_equal_expr, structurally_equal_stmt};
pub use matcher::find_matches;
pub use parse::{parse_pattern, parse_template};
pub use pattern::{PatternKind, PlaceholderInfo, SsrPattern, SsrTemplate};
pub use preview::{FilePreview, MatchPreview, SsrPreview};
pub use replace::{apply_replacements, render_replacement};

#[cfg(test)]
mod tests;
