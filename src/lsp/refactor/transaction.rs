use std::collections::HashMap;
use std::path::{Path, PathBuf};

use miette::Result;

// ── RefactorTransaction ─────────────────────────────────────────────────────

/// Write-ahead-log wrapper: snapshots original file contents before writing,
/// restores all on drop if not committed.
pub struct RefactorTransaction {
    /// Original content before any writes. `None` means the file didn't exist.
    snapshots: HashMap<PathBuf, Option<Vec<u8>>>,
    committed: bool,
}

impl RefactorTransaction {
    pub fn new() -> Self {
        Self {
            snapshots: HashMap::new(),
            committed: false,
        }
    }

    /// Snapshot original content on first access, then write new content.
    pub fn write_file(&mut self, path: &Path, content: &str) -> Result<()> {
        self.snapshot(path);
        std::fs::write(path, content)
            .map_err(|e| miette::miette!("cannot write {}: {e}", path.display()))?;
        Ok(())
    }

    /// Rename a file, snapshotting both source and destination.
    pub fn rename_file(&mut self, from: &Path, to: &Path) -> Result<()> {
        self.snapshot(from);
        self.snapshot(to);
        std::fs::rename(from, to).map_err(|e| {
            miette::miette!("cannot rename {} → {}: {e}", from.display(), to.display())
        })?;
        Ok(())
    }

    /// Mark transaction as successful — drop becomes a no-op.
    /// (Currently callers prefer `into_snapshots()` for undo support.)
    #[allow(dead_code)]
    pub fn commit(&mut self) {
        self.committed = true;
    }

    /// Consume the transaction and return snapshots for undo recording.
    /// Also marks the transaction as committed (no rollback on drop).
    pub fn into_snapshots(mut self) -> HashMap<PathBuf, Option<Vec<u8>>> {
        self.committed = true;
        std::mem::take(&mut self.snapshots)
    }

    fn snapshot(&mut self, path: &Path) {
        let canonical = path.to_path_buf();
        self.snapshots
            .entry(canonical)
            .or_insert_with(|| std::fs::read(path).ok());
    }
}

impl Drop for RefactorTransaction {
    fn drop(&mut self) {
        if self.committed {
            return;
        }
        for (path, original) in &self.snapshots {
            match original {
                Some(content) => {
                    let _ = std::fs::write(path, content);
                }
                None => {
                    let _ = std::fs::remove_file(path);
                }
            }
        }
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn commit_keeps_changes() {
        let tmp = tempfile::Builder::new().prefix("gdtest").tempdir().unwrap();
        let file = tmp.path().join("test.gd");
        fs::write(&file, "original").unwrap();

        let mut tx = RefactorTransaction::new();
        tx.write_file(&file, "modified").unwrap();
        tx.commit();
        drop(tx);

        assert_eq!(fs::read_to_string(&file).unwrap(), "modified");
    }

    #[test]
    fn drop_without_commit_restores() {
        let tmp = tempfile::Builder::new().prefix("gdtest").tempdir().unwrap();
        let file = tmp.path().join("test.gd");
        fs::write(&file, "original").unwrap();

        {
            let mut tx = RefactorTransaction::new();
            tx.write_file(&file, "modified").unwrap();
            assert_eq!(fs::read_to_string(&file).unwrap(), "modified");
            // tx dropped without commit
        }

        assert_eq!(fs::read_to_string(&file).unwrap(), "original");
    }

    #[test]
    fn rollback_removes_new_files() {
        let tmp = tempfile::Builder::new().prefix("gdtest").tempdir().unwrap();
        let file = tmp.path().join("new_file.gd");

        {
            let mut tx = RefactorTransaction::new();
            tx.write_file(&file, "new content").unwrap();
            assert!(file.exists());
            // tx dropped without commit
        }

        assert!(!file.exists());
    }

    #[test]
    fn into_snapshots_commits() {
        let tmp = tempfile::Builder::new().prefix("gdtest").tempdir().unwrap();
        let file = tmp.path().join("test.gd");
        fs::write(&file, "original").unwrap();

        let mut tx = RefactorTransaction::new();
        tx.write_file(&file, "modified").unwrap();
        let snapshots = tx.into_snapshots();

        assert_eq!(fs::read_to_string(&file).unwrap(), "modified");
        assert!(snapshots.contains_key(&file));
        assert_eq!(snapshots[&file].as_deref(), Some(b"original".as_slice()));
    }

    #[test]
    fn multiple_writes_snapshot_only_first() {
        let tmp = tempfile::Builder::new().prefix("gdtest").tempdir().unwrap();
        let file = tmp.path().join("test.gd");
        fs::write(&file, "v1").unwrap();

        {
            let mut tx = RefactorTransaction::new();
            tx.write_file(&file, "v2").unwrap();
            tx.write_file(&file, "v3").unwrap();
            // rollback should restore to v1, not v2
        }

        assert_eq!(fs::read_to_string(&file).unwrap(), "v1");
    }

    #[test]
    fn rename_snapshots_both() {
        let tmp = tempfile::Builder::new().prefix("gdtest").tempdir().unwrap();
        let from = tmp.path().join("from.gd");
        let to = tmp.path().join("to.gd");
        fs::write(&from, "content").unwrap();

        {
            let mut tx = RefactorTransaction::new();
            tx.rename_file(&from, &to).unwrap();
            assert!(!from.exists());
            assert!(to.exists());
            // tx dropped without commit — should restore
        }

        assert!(from.exists());
        assert!(!to.exists());
        assert_eq!(fs::read_to_string(&from).unwrap(), "content");
    }
}
