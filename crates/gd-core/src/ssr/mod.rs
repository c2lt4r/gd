//! Structured Search & Replace (SSR) for GDScript.
//!
//! Pattern-based AST search and replace.  Write a GDScript-like template
//! with `$placeholders`, find all structural matches in the project, and
//! optionally rewrite them with a replacement template.
//!
//! # Phases
//!
//! This module is built incrementally:
//! - **Phase 1** (this): pattern language and parser.
//! - Phase 2: structural matcher.
//! - Phase 3: replacement engine.
//! - Phase 4: type-aware constraints.
//! - Phase 5: CLI integration.

mod parse;
mod pattern;

pub use parse::{parse_pattern, parse_template};
pub use pattern::{PatternKind, PlaceholderInfo, SsrPattern, SsrTemplate};

#[cfg(test)]
mod tests;
