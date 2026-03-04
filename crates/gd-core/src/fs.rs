use miette::{Result, miette};
use path_slash::PathExt;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Collect all .gd files under `root`, respecting .gdignore.
pub fn collect_gdscript_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in WalkDir::new(root)
        .into_iter()
        .filter_entry(|e| !is_hidden_or_ignored(e))
    {
        let entry = entry.map_err(|e| miette!("Error walking directory: {e}"))?;
        if entry.file_type().is_file()
            && let Some(ext) = entry.path().extension()
            && ext == "gd"
        {
            files.push(entry.into_path());
        }
    }
    files.sort();
    Ok(files)
}

/// Skip hidden dirs, .godot/, addons/ build dirs, etc.
fn is_hidden_or_ignored(entry: &walkdir::DirEntry) -> bool {
    // Never filter the root entry (e.g. "." passed as the walk root)
    if entry.depth() == 0 {
        return false;
    }
    let name = entry.file_name().to_string_lossy();
    name.starts_with('.') || name == "build" || name == ".godot" || name == ".import"
}

/// Collect all .tscn and .tres files under `root`, skipping hidden/ignored dirs.
pub fn collect_resource_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in WalkDir::new(root)
        .into_iter()
        .filter_entry(|e| !is_hidden_or_ignored(e))
    {
        let entry = entry.map_err(|e| miette!("Error walking directory: {e}"))?;
        if entry.file_type().is_file()
            && let Some(ext) = entry.path().extension()
            && (ext == "tscn" || ext == "tres")
        {
            files.push(entry.into_path());
        }
    }
    files.sort();
    Ok(files)
}

/// Make `path` relative to `base` and return a forward-slash string.
///
/// Uses `strip_prefix` (no canonicalization) to avoid Windows `\\?\` issues.
/// Falls back to the full path if stripping fails.
pub fn relative_slash(path: &Path, base: &Path) -> String {
    let rel = path.strip_prefix(base).unwrap_or(path);
    rel.to_slash_lossy().into_owned()
}

// ── Eval file cleanup ───────────────────────────────────────────────────────

/// Clean up stale eval files from `.godot/` (request files, result files, ready marker).
/// Called when the daemon detects the game has exited.
pub fn cleanup_stale_eval_files(project_root: &Path) {
    let godot_dir = project_root.join(".godot");
    let _ = std::fs::remove_file(godot_dir.join("gd-eval-ready"));
    // Remove any lingering request/result files
    if let Ok(entries) = std::fs::read_dir(&godot_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with("gd-eval-request-") || name.starts_with("gd-eval-result-") {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
}

// ── Ignore pattern matching ─────────────────────────────────────────────────

/// Check if a file path matches any of the ignore patterns.
/// Patterns support: `dir/**` (recursive), `*.ext` (extension), exact match.
pub fn matches_ignore_pattern(path: &Path, base: &Path, patterns: &[String]) -> bool {
    if patterns.is_empty() {
        return false;
    }
    // Try plain strip_prefix first (works when both paths share the same root).
    // Only fall back to canonicalize for symlink edge cases (e.g., macOS /var -> /private/var).
    // Avoids Windows canonicalize returning \\?\C:\... extended-length paths that break strip_prefix.
    let relative = path.strip_prefix(base).map_or_else(
        |_| {
            let canon_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
            let canon_base = base.canonicalize().unwrap_or_else(|_| base.to_path_buf());
            canon_path
                .strip_prefix(&canon_base)
                .unwrap_or(&canon_path)
                .to_path_buf()
        },
        std::path::Path::to_path_buf,
    );
    // Normalize to forward slashes so patterns work on Windows
    let rel_str = path_slash::PathExt::to_slash_lossy(relative.as_path());

    for pattern in patterns {
        if pattern.ends_with("/**") {
            // "addons/**" → match anything under addons/
            let prefix = &pattern[..pattern.len() - 3];
            if rel_str.starts_with(prefix) {
                return true;
            }
        } else if let Some(suffix) = pattern.strip_prefix('*') {
            // "*.test.gd" → match files ending with .test.gd
            if rel_str.ends_with(suffix) {
                return true;
            }
        } else if rel_str == pattern.as_str() || rel_str.starts_with(&format!("{pattern}/")) {
            // Exact file match or directory prefix
            return true;
        }
    }
    false
}

/// Check if a lint rule should be skipped for a file due to `[[lint.overrides]]`.
pub fn is_rule_excluded_by_override(
    path: &Path,
    base: &Path,
    rule_name: &str,
    overrides: &[crate::config::LintOverride],
) -> bool {
    overrides.iter().any(|ov| {
        ov.exclude_rules.iter().any(|r| r == rule_name)
            && matches_ignore_pattern(path, base, &ov.paths)
    })
}

// ── WSL path utilities ───────────────────────────────────────────────────────

/// Check if running under WSL (Windows Subsystem for Linux).
pub fn is_wsl() -> bool {
    use std::sync::OnceLock;
    static IS_WSL: OnceLock<bool> = OnceLock::new();
    *IS_WSL.get_or_init(|| {
        std::fs::read_to_string("/proc/version")
            .map(|v| v.to_ascii_lowercase().contains("microsoft"))
            .unwrap_or(false)
    })
}

/// Convert a Windows path (`C:\Users\...` or `C:/Users/...`) to a WSL path (`/mnt/c/Users/...`).
/// Returns the path unchanged if already a Unix path.
pub fn windows_to_wsl_path(path: &str) -> String {
    let path = path.replace('\\', "/");
    if path.len() >= 3 && path.as_bytes()[1] == b':' && path.as_bytes()[2] == b'/' {
        let drive = path.as_bytes()[0].to_ascii_lowercase() as char;
        return format!("/mnt/{drive}{}", &path[2..]);
    }
    path
}

/// Convert a WSL path (`/mnt/c/Users/...`) to a Windows path (`C:/Users/...`).
/// Returns `None` if the path is not a WSL mount path.
pub fn wsl_to_windows_path(path: &str) -> Option<String> {
    let rest = path.strip_prefix("/mnt/")?;
    let drive = rest.chars().next()?;
    if !drive.is_ascii_alphabetic() {
        return None;
    }
    let remainder = &rest[1..]; // everything after the drive letter (starts with / or is empty)
    Some(format!("{}:{}", drive.to_ascii_uppercase(), remainder))
}

/// Check if a binary path is a Windows executable (ends with `.exe`).
pub fn is_windows_binary(path: &Path) -> bool {
    path.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("exe"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wsl_to_windows_path() {
        assert_eq!(
            wsl_to_windows_path("/mnt/c/projects/game"),
            Some("C:/projects/game".to_string())
        );
        assert_eq!(
            wsl_to_windows_path("/mnt/d/games"),
            Some("D:/games".to_string())
        );
        assert_eq!(wsl_to_windows_path("/mnt/c"), Some("C:".to_string()));
        assert_eq!(wsl_to_windows_path("/home/user/project"), None);
        assert_eq!(wsl_to_windows_path("C:/already/windows"), None);
    }

    #[test]
    fn test_windows_to_wsl_path() {
        assert_eq!(
            windows_to_wsl_path("C:/Users/user/project"),
            "/mnt/c/Users/user/project"
        );
        assert_eq!(
            windows_to_wsl_path("C:\\Users\\user\\project"),
            "/mnt/c/Users/user/project"
        );
        assert_eq!(windows_to_wsl_path("D:\\Games"), "/mnt/d/Games");
        // Already a Unix path — returned as-is
        assert_eq!(
            windows_to_wsl_path("/home/user/project"),
            "/home/user/project"
        );
    }

    #[test]
    fn test_is_windows_binary() {
        assert!(is_windows_binary(Path::new("C:/Godot/godot.exe")));
        assert!(is_windows_binary(Path::new("godot.EXE")));
        assert!(!is_windows_binary(Path::new("/usr/bin/godot")));
        assert!(!is_windows_binary(Path::new("godot")));
    }
}
