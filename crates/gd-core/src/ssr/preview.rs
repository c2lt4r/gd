//! Dry-run preview types for SSR.
//!
//! All SSR operations produce a preview first — committing is a
//! separate step.  The CLI (Phase 5) uses these for `--dry-run` output.

use std::collections::HashMap;
use std::path::PathBuf;

/// Preview of all SSR changes across the project.
#[derive(Debug, Clone)]
pub struct SsrPreview {
    /// Per-file list of matches with their replacements.
    pub files: Vec<FilePreview>,
}

/// Preview of SSR changes in a single file.
#[derive(Debug, Clone)]
pub struct FilePreview {
    /// Path to the file.
    pub path: PathBuf,
    /// Individual match previews within this file.
    pub matches: Vec<MatchPreview>,
}

/// Preview of a single SSR match and its replacement.
#[derive(Debug, Clone)]
pub struct MatchPreview {
    /// Line number (1-based) of the match.
    pub line: usize,
    /// Original matched source text.
    pub original: String,
    /// Rendered replacement text.
    pub replacement: String,
    /// Placeholder name → captured source text (for display).
    pub captures: HashMap<String, String>,
}
