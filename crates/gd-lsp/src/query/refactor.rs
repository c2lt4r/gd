use miette::Result;

use super::{find_root, resolve_file};

// ── Refactoring queries ──────────────────────────────────────────────────────

pub fn query_extract_method(
    file: &str,
    start_line: usize,
    end_line: usize,
    name: &str,
    dry_run: bool,
) -> Result<crate::refactor::ExtractMethodOutput> {
    let path = resolve_file(file)?;
    let project_root = find_root(&path)?;
    crate::refactor::extract_method(&path, start_line, end_line, name, dry_run, &project_root)
}

#[allow(clippy::too_many_arguments)]
pub fn query_change_signature(
    file: &str,
    name: &str,
    add_params: &[String],
    remove_params: &[String],
    rename_params: &[String],
    reorder: Option<&str>,
    class: Option<&str>,
    dry_run: bool,
) -> Result<crate::refactor::ChangeSignatureOutput> {
    let path = resolve_file(file)?;
    let project_root = find_root(&path)?;
    crate::refactor::change_signature(
        &path,
        name,
        add_params,
        remove_params,
        rename_params,
        reorder,
        class,
        dry_run,
        &project_root,
    )
}

// ── Move file ────────────────────────────────────────────────────────────────

pub fn query_move_file(
    from: &str,
    to: &str,
    dry_run: bool,
) -> Result<crate::refactor::MoveFileOutput> {
    let from_path = resolve_file(from)?;
    let project_root = find_root(&from_path)?;
    let to_path = if std::path::Path::new(to).is_absolute() {
        std::path::PathBuf::from(to)
    } else {
        project_root.join(to)
    };
    crate::refactor::move_file(&from_path, &to_path, dry_run, &project_root)
}
