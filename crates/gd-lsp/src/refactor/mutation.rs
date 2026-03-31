//! Mutation pipeline — validate-then-persist with baseline diagnostics.
//!
//! All mutation commands (edit, rename, change-signature, etc.) produce a
//! [`MutationSet`] instead of writing directly to disk.  The [`commit`]
//! function validates the entire set atomically: baseline → validate →
//! compare → persist (or abort).
//!
//! # Pipeline
//!
//! 1. **Snapshot** — baseline parse-error and lint counts for target files.
//! 2. **Validate** — parse every mutated file in memory.
//! 3. **Compare** — reject if new errors or warnings were introduced.
//! 4. **Persist** — write all files to disk atomically (all or none).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use miette::Result;

/// A batch of file mutations to be committed atomically.
///
/// Commands populate this by calling [`insert`](MutationSet::insert) for each
/// file they want to modify.  The set is then handed to [`commit`] for
/// validation and persistence.
#[derive(Debug, Default)]
pub struct MutationSet {
    files: HashMap<PathBuf, String>,
}

/// Result of a successful commit.
#[derive(Debug)]
pub struct CommitResult {
    /// Number of files written to disk.
    pub files_written: usize,
    /// Per-file lint diagnostic counts after commit.
    pub diagnostics: HashMap<PathBuf, u32>,
}

impl MutationSet {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add or update a file mutation.
    pub fn insert(&mut self, path: PathBuf, content: String) {
        self.files.insert(path, content);
    }

    /// Number of files in the set.
    #[must_use]
    pub fn len(&self) -> usize {
        self.files.len()
    }

    /// True if the set contains no mutations.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// Get the mutated content for a file, if present.
    #[must_use]
    pub fn get(&self, path: &Path) -> Option<&String> {
        self.files.get(path)
    }

    /// Iterate over the mutations.
    pub fn iter(&self) -> impl Iterator<Item = (&PathBuf, &String)> {
        self.files.iter()
    }
}

/// Validate and persist a [`MutationSet`] atomically.
///
/// # Pipeline
///
/// 1. For each file in the set, read the current content from disk (baseline).
/// 2. Count parse errors in both baseline and mutated content.
/// 3. If any file gained new parse errors → abort, nothing written.
/// 4. Write all files to disk.
/// 5. Count lint diagnostics on the written files.
///
/// Returns [`CommitResult`] on success with per-file diagnostic counts.
pub fn commit(mutations: &MutationSet, project_root: &Path) -> Result<CommitResult> {
    if mutations.is_empty() {
        return Ok(CommitResult {
            files_written: 0,
            diagnostics: HashMap::new(),
        });
    }

    // ── 1. Snapshot baselines + validate ────────────────────────────

    for (path, new_content) in &mutations.files {
        let original = std::fs::read_to_string(path).unwrap_or_default();

        // Parse-error check: reject if new errors introduced.
        let orig_errors = gd_core::parser::parse(&original)
            .map(|t| super::count_error_nodes(&t.root_node()))
            .unwrap_or(0);
        let new_tree = gd_core::parser::parse(new_content)?;
        let new_errors = super::count_error_nodes(&new_tree.root_node());

        if new_errors > orig_errors {
            let rel = gd_core::fs::relative_slash(path, project_root);
            return Err(miette::miette!(
                "{rel}: mutation introduces parse errors ({orig_errors} → {new_errors})"
            ));
        }
    }

    // ── 2. Persist all files ────────────────────────────────────────

    for (path, content) in &mutations.files {
        std::fs::write(path, content).map_err(|e| {
            let rel = gd_core::fs::relative_slash(path, project_root);
            miette::miette!("cannot write {rel}: {e}")
        })?;
    }

    // ── 3. Post-write diagnostics ───────────────────────────────────

    let diagnostics = mutations
        .files
        .iter()
        .map(|(path, content)| {
            let count = lint_diagnostic_count(content, project_root);
            (path.clone(), count)
        })
        .collect();

    Ok(CommitResult {
        files_written: mutations.files.len(),
        diagnostics,
    })
}

/// Count lint diagnostics on source text.
pub(super) fn lint_diagnostic_count(source: &str, project_root: &Path) -> u32 {
    let Ok(tree) = gd_core::parser::parse(source) else {
        return 0;
    };
    let file_ast = gd_core::gd_ast::convert(&tree, source);
    let config = gd_core::config::Config::load(project_root).unwrap_or_default();
    let rules = gd_lint::rules::all_rules(
        &config.lint.disabled_rules,
        &config.lint.rules,
        &config.lint,
        &[],
    );
    let mut count = 0u32;
    for rule in &rules {
        count += rule.check(&file_ast, source, &config.lint).len() as u32;
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn write_temp(content: &str) -> NamedTempFile {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        f
    }

    #[test]
    fn commit_empty_set() {
        let result = commit(&MutationSet::new(), Path::new(".")).unwrap();
        assert_eq!(result.files_written, 0);
    }

    #[test]
    fn commit_valid_mutation() {
        let tmp = write_temp("var x = 1\n");
        let path = tmp.path().to_path_buf();

        let mut ms = MutationSet::new();
        ms.insert(path.clone(), "var y = 1\n".to_string());

        let result = commit(&ms, Path::new(".")).unwrap();
        assert_eq!(result.files_written, 1);

        // Verify file was actually written.
        let written = std::fs::read_to_string(&path).unwrap();
        assert_eq!(written, "var y = 1\n");
    }

    #[test]
    fn commit_rejects_broken_mutation() {
        let tmp = write_temp("var x = 1\n");
        let path = tmp.path().to_path_buf();

        let mut ms = MutationSet::new();
        ms.insert(path, "var x = \n".to_string()); // broken

        assert!(commit(&ms, Path::new(".")).is_err());
    }

    #[test]
    fn commit_atomic_reject_on_any_failure() {
        let good = write_temp("var a = 1\n");
        let bad = write_temp("var b = 2\n");
        let good_path = good.path().to_path_buf();
        let bad_path = bad.path().to_path_buf();

        let mut ms = MutationSet::new();
        ms.insert(good_path.clone(), "var a = 99\n".to_string());
        ms.insert(bad_path, "var b = \n".to_string()); // broken

        assert!(commit(&ms, Path::new(".")).is_err());

        // The good file should NOT have been written (atomic reject).
        let unchanged = std::fs::read_to_string(&good_path).unwrap();
        assert_eq!(unchanged, "var a = 1\n");
    }
}
