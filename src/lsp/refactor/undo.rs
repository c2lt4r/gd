use std::collections::HashMap;
use std::path::{Path, PathBuf};

use miette::Result;
use serde::{Deserialize, Serialize};

// ── Undo types ──────────────────────────────────────────────────────────────

const MAX_ENTRIES: u64 = 50;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct UndoEntry {
    pub id: u64,
    pub command: String,
    pub description: String,
    pub timestamp: String,
    pub files: Vec<UndoFileEntry>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct UndoFileEntry {
    pub path: String,
    pub action: UndoAction,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum UndoAction {
    Modified,
    Created,
    Deleted,
}

// ── UndoStack ───────────────────────────────────────────────────────────────

pub struct UndoStack {
    undo_dir: PathBuf,
}

impl UndoStack {
    /// Open (or create) the undo stack for a project.
    pub fn open(project_root: &Path) -> Self {
        let undo_dir = project_root.join(".godot").join("gd-undo");
        Self { undo_dir }
    }

    /// Record a refactoring's snapshots as a new undo entry.
    /// Returns the entry ID.
    pub fn record(
        &self,
        command: &str,
        description: &str,
        snapshots: &HashMap<PathBuf, Option<Vec<u8>>>,
        project_root: &Path,
    ) -> Result<u64> {
        if snapshots.is_empty() {
            return Err(miette::miette!("no files to record"));
        }

        let entries_dir = self.undo_dir.join("entries");
        std::fs::create_dir_all(&entries_dir)
            .map_err(|e| miette::miette!("cannot create undo dir: {e}"))?;

        let id = self.next_id()?;
        let entry_dir = entries_dir.join(format!("{id:04}"));
        let files_dir = entry_dir.join("files");
        std::fs::create_dir_all(&files_dir)
            .map_err(|e| miette::miette!("cannot create entry dir: {e}"))?;

        let mut file_entries = Vec::new();
        for (path, original) in snapshots {
            let relative = crate::core::fs::relative_slash(path, project_root);

            match original {
                Some(content) => {
                    // File existed — save original content, action is Modified
                    let save_path = files_dir.join(&relative);
                    if let Some(parent) = save_path.parent() {
                        std::fs::create_dir_all(parent).ok();
                    }
                    std::fs::write(&save_path, content)
                        .map_err(|e| miette::miette!("cannot save snapshot: {e}"))?;
                    file_entries.push(UndoFileEntry {
                        path: relative,
                        action: UndoAction::Modified,
                    });
                }
                None => {
                    // File didn't exist before — action is Created (undo = delete)
                    file_entries.push(UndoFileEntry {
                        path: relative,
                        action: UndoAction::Created,
                    });
                }
            }
        }

        let entry = UndoEntry {
            id,
            command: command.to_string(),
            description: description.to_string(),
            timestamp: chrono_now(),
            files: file_entries,
        };

        let meta_path = entry_dir.join("meta.json");
        let json = serde_json::to_string_pretty(&entry)
            .map_err(|e| miette::miette!("cannot serialize undo entry: {e}"))?;
        std::fs::write(&meta_path, json)
            .map_err(|e| miette::miette!("cannot write undo metadata: {e}"))?;

        // Prune old entries
        self.prune()?;

        Ok(id)
    }

    /// List entries, most recent first.
    pub fn list(&self) -> Result<Vec<UndoEntry>> {
        let entries_dir = self.undo_dir.join("entries");
        if !entries_dir.exists() {
            return Ok(Vec::new());
        }

        let mut entries = Vec::new();
        let mut dirs: Vec<_> = std::fs::read_dir(&entries_dir)
            .map_err(|e| miette::miette!("cannot read undo dir: {e}"))?
            .filter_map(std::result::Result::ok)
            .filter(|e| e.file_type().is_ok_and(|ft| ft.is_dir()))
            .collect();
        dirs.sort_by_key(std::fs::DirEntry::file_name);

        for dir_entry in &dirs {
            let meta_path = dir_entry.path().join("meta.json");
            if let Ok(json) = std::fs::read_to_string(&meta_path)
                && let Ok(entry) = serde_json::from_str::<UndoEntry>(&json)
            {
                entries.push(entry);
            }
        }

        entries.reverse(); // most recent first
        Ok(entries)
    }

    /// Undo an entry: restore original files, remove the entry.
    /// If `id` is None, undoes the most recent entry.
    pub fn undo(&self, id: Option<u64>, project_root: &Path) -> Result<UndoEntry> {
        let entries = self.list()?;
        if entries.is_empty() {
            return Err(miette::miette!("no undo entries available"));
        }

        let entry = if let Some(target_id) = id {
            entries
                .into_iter()
                .find(|e| e.id == target_id)
                .ok_or_else(|| miette::miette!("undo entry {target_id} not found"))?
        } else {
            entries
                .into_iter()
                .next()
                .ok_or_else(|| miette::miette!("no undo entries available"))?
        };

        let entry_dir = self
            .undo_dir
            .join("entries")
            .join(format!("{:04}", entry.id));
        let files_dir = entry_dir.join("files");

        for file_entry in &entry.files {
            let target = project_root.join(&file_entry.path);
            match file_entry.action {
                UndoAction::Modified => {
                    // Restore original content
                    let saved = files_dir.join(&file_entry.path);
                    if saved.exists() {
                        let content = std::fs::read(&saved)
                            .map_err(|e| miette::miette!("cannot read snapshot: {e}"))?;
                        std::fs::write(&target, content)
                            .map_err(|e| miette::miette!("cannot restore file: {e}"))?;
                    }
                }
                UndoAction::Created => {
                    // File was created by the refactoring — delete it
                    if target.exists() {
                        std::fs::remove_file(&target)
                            .map_err(|e| miette::miette!("cannot remove file: {e}"))?;
                    }
                }
                UndoAction::Deleted => {
                    // File was deleted by the refactoring — restore from snapshot
                    let saved = files_dir.join(&file_entry.path);
                    if saved.exists() {
                        if let Some(parent) = target.parent() {
                            std::fs::create_dir_all(parent).ok();
                        }
                        let content = std::fs::read(&saved)
                            .map_err(|e| miette::miette!("cannot read snapshot: {e}"))?;
                        std::fs::write(&target, content)
                            .map_err(|e| miette::miette!("cannot restore file: {e}"))?;
                    }
                }
            }
        }

        // Remove the undo entry directory
        std::fs::remove_dir_all(&entry_dir).ok();

        Ok(entry)
    }

    fn next_id(&self) -> Result<u64> {
        let entries_dir = self.undo_dir.join("entries");
        if !entries_dir.exists() {
            return Ok(1);
        }
        let mut max_id = 0u64;
        for dir_entry in std::fs::read_dir(&entries_dir)
            .map_err(|e| miette::miette!("cannot read undo dir: {e}"))?
            .filter_map(std::result::Result::ok)
        {
            if let Some(name) = dir_entry.file_name().to_str()
                && let Ok(id) = name.parse::<u64>()
            {
                max_id = max_id.max(id);
            }
        }
        Ok(max_id + 1)
    }

    fn prune(&self) -> Result<()> {
        let entries_dir = self.undo_dir.join("entries");
        if !entries_dir.exists() {
            return Ok(());
        }
        let mut dirs: Vec<_> = std::fs::read_dir(&entries_dir)
            .map_err(|e| miette::miette!("cannot read undo dir: {e}"))?
            .filter_map(std::result::Result::ok)
            .filter(|e| e.file_type().is_ok_and(|ft| ft.is_dir()))
            .collect();
        dirs.sort_by_key(std::fs::DirEntry::file_name);

        if dirs.len() as u64 > MAX_ENTRIES {
            let to_remove = dirs.len() as u64 - MAX_ENTRIES;
            for dir_entry in dirs.iter().take(to_remove as usize) {
                std::fs::remove_dir_all(dir_entry.path()).ok();
            }
        }
        Ok(())
    }
}

/// Simple ISO 8601 timestamp without pulling in the chrono crate.
fn chrono_now() -> String {
    use std::time::SystemTime;
    let duration = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = duration.as_secs();
    // Convert to a basic UTC timestamp: YYYY-MM-DDTHH:MM:SSZ
    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    // Approximate date calculation (good enough for display purposes)
    let (year, month, day) = days_to_date(days);
    format!("{year:04}-{month:02}-{day:02}T{hours:02}:{minutes:02}:{seconds:02}Z")
}

fn days_to_date(days_since_epoch: u64) -> (u64, u64, u64) {
    // Algorithm from https://howardhinnant.github.io/date_algorithms.html
    let z = days_since_epoch + 719_468;
    let era = z / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn setup_project() -> tempfile::TempDir {
        let tmp = tempfile::Builder::new().prefix("gdtest").tempdir().unwrap();
        fs::create_dir_all(tmp.path().join(".godot")).unwrap();
        fs::write(
            tmp.path().join("project.godot"),
            "[application]\nconfig/name=\"test\"\n",
        )
        .unwrap();
        tmp
    }

    #[test]
    fn record_and_list() {
        let tmp = setup_project();
        let stack = UndoStack::open(tmp.path());

        let file = tmp.path().join("player.gd");
        fs::write(&file, "original").unwrap();

        let mut snapshots = HashMap::new();
        snapshots.insert(file, Some(b"original".to_vec()));

        let id = stack
            .record("test-cmd", "test description", &snapshots, tmp.path())
            .unwrap();
        assert_eq!(id, 1);

        let entries = stack.list().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, 1);
        assert_eq!(entries[0].command, "test-cmd");
        assert_eq!(entries[0].description, "test description");
        assert_eq!(entries[0].files.len(), 1);
        assert_eq!(entries[0].files[0].action, UndoAction::Modified);
    }

    #[test]
    fn undo_restores_modified_file() {
        let tmp = setup_project();
        let stack = UndoStack::open(tmp.path());

        let file = tmp.path().join("player.gd");
        fs::write(&file, "original").unwrap();

        let mut snapshots = HashMap::new();
        snapshots.insert(file.clone(), Some(b"original".to_vec()));
        stack
            .record("test-cmd", "test", &snapshots, tmp.path())
            .unwrap();

        // Simulate the refactoring having modified the file
        fs::write(&file, "modified").unwrap();

        let entry = stack.undo(None, tmp.path()).unwrap();
        assert_eq!(entry.command, "test-cmd");
        assert_eq!(fs::read_to_string(&file).unwrap(), "original");
    }

    #[test]
    fn undo_removes_created_file() {
        let tmp = setup_project();
        let stack = UndoStack::open(tmp.path());

        let file = tmp.path().join("new_file.gd");

        let mut snapshots = HashMap::new();
        snapshots.insert(file.clone(), None); // file didn't exist before

        stack
            .record("test-cmd", "test", &snapshots, tmp.path())
            .unwrap();

        // Simulate the refactoring having created the file
        fs::write(&file, "new content").unwrap();

        stack.undo(None, tmp.path()).unwrap();
        assert!(!file.exists());
    }

    #[test]
    fn undo_specific_id() {
        let tmp = setup_project();
        let stack = UndoStack::open(tmp.path());

        let file1 = tmp.path().join("a.gd");
        let file2 = tmp.path().join("b.gd");
        fs::write(&file1, "a-original").unwrap();
        fs::write(&file2, "b-original").unwrap();

        let mut snap1 = HashMap::new();
        snap1.insert(file1.clone(), Some(b"a-original".to_vec()));
        stack.record("cmd1", "first", &snap1, tmp.path()).unwrap();

        let mut snap2 = HashMap::new();
        snap2.insert(file2.clone(), Some(b"b-original".to_vec()));
        let id2 = stack.record("cmd2", "second", &snap2, tmp.path()).unwrap();

        // Modify both files
        fs::write(&file1, "a-modified").unwrap();
        fs::write(&file2, "b-modified").unwrap();

        // Undo only the second entry
        stack.undo(Some(id2), tmp.path()).unwrap();
        assert_eq!(fs::read_to_string(&file1).unwrap(), "a-modified");
        assert_eq!(fs::read_to_string(&file2).unwrap(), "b-original");
    }

    #[test]
    fn empty_undo_stack() {
        let tmp = setup_project();
        let stack = UndoStack::open(tmp.path());
        let entries = stack.list().unwrap();
        assert!(entries.is_empty());
        assert!(stack.undo(None, tmp.path()).is_err());
    }

    #[test]
    fn prune_old_entries() {
        let tmp = setup_project();
        let stack = UndoStack::open(tmp.path());

        let file = tmp.path().join("test.gd");
        fs::write(&file, "content").unwrap();

        // Create MAX_ENTRIES + 5 entries
        for i in 0..(MAX_ENTRIES + 5) {
            let mut snapshots = HashMap::new();
            snapshots.insert(file.clone(), Some(b"content".to_vec()));
            stack
                .record("cmd", &format!("entry {i}"), &snapshots, tmp.path())
                .unwrap();
        }

        let entries = stack.list().unwrap();
        assert!(entries.len() as u64 <= MAX_ENTRIES);
    }

    #[test]
    fn list_most_recent_first() {
        let tmp = setup_project();
        let stack = UndoStack::open(tmp.path());

        let file = tmp.path().join("test.gd");
        fs::write(&file, "content").unwrap();

        for i in 1..=3 {
            let mut snapshots = HashMap::new();
            snapshots.insert(file.clone(), Some(b"content".to_vec()));
            stack
                .record("cmd", &format!("entry {i}"), &snapshots, tmp.path())
                .unwrap();
        }

        let entries = stack.list().unwrap();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].description, "entry 3"); // most recent first
        assert_eq!(entries[2].description, "entry 1");
    }

    #[test]
    fn undo_entry_is_removed() {
        let tmp = setup_project();
        let stack = UndoStack::open(tmp.path());

        let file = tmp.path().join("test.gd");
        fs::write(&file, "content").unwrap();

        let mut snapshots = HashMap::new();
        snapshots.insert(file, Some(b"content".to_vec()));
        stack.record("cmd", "test", &snapshots, tmp.path()).unwrap();

        assert_eq!(stack.list().unwrap().len(), 1);
        stack.undo(None, tmp.path()).unwrap();
        assert_eq!(stack.list().unwrap().len(), 0);
    }
}
