use miette::Result;

use super::{find_root, resolve_file};

// ── Refactoring queries ──────────────────────────────────────────────────────

pub fn query_delete_symbol(
    file: &str,
    name: Option<&str>,
    line: Option<usize>,
    force: bool,
    dry_run: bool,
    class: Option<&str>,
) -> Result<crate::refactor::DeleteSymbolOutput> {
    let path = resolve_file(file)?;
    let project_root = find_root(&path)?;
    crate::refactor::delete_symbol(&path, name, line, force, dry_run, &project_root, class)
}

pub fn query_move_symbol(
    name: &str,
    from: &str,
    to: &str,
    dry_run: bool,
    class: Option<&str>,
    target_class: Option<&str>,
    update_callers: bool,
) -> Result<crate::refactor::MoveSymbolOutput> {
    let from_path = resolve_file(from)?;
    let project_root = find_root(&from_path)?;
    let to_path = project_root.join(to);
    crate::refactor::move_symbol(
        name,
        &from_path,
        &to_path,
        dry_run,
        &project_root,
        class,
        target_class,
        update_callers,
    )
}

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

pub fn query_inline_method(
    file: &str,
    line: usize,
    column: usize,
    dry_run: bool,
) -> Result<crate::refactor::InlineMethodOutput> {
    let path = resolve_file(file)?;
    let project_root = find_root(&path)?;
    crate::refactor::inline_method(&path, line, column, dry_run, &project_root)
}

pub fn query_inline_method_by_name(
    file: &str,
    name: &str,
    all: bool,
    dry_run: bool,
) -> Result<crate::refactor::InlineMethodByNameOutput> {
    let path = resolve_file(file)?;
    let project_root = find_root(&path)?;
    crate::refactor::inline_method_by_name(&path, name, all, dry_run, &project_root)
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

#[allow(clippy::too_many_arguments)]
pub fn query_introduce_variable(
    file: &str,
    line: usize,
    column: usize,
    end_column: usize,
    name: &str,
    as_const: bool,
    replace_all: bool,
    dry_run: bool,
) -> Result<crate::refactor::IntroduceVariableOutput> {
    let path = resolve_file(file)?;
    let project_root = find_root(&path)?;
    crate::refactor::introduce_variable(
        &path,
        line,
        column,
        end_column,
        name,
        as_const,
        replace_all,
        dry_run,
        &project_root,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn query_extract_constant(
    file: &str,
    line: usize,
    column: usize,
    end_column: usize,
    name: &str,
    replace_all: bool,
    dry_run: bool,
    class: Option<&str>,
) -> Result<crate::refactor::ExtractConstantOutput> {
    let path = resolve_file(file)?;
    let project_root = find_root(&path)?;
    crate::refactor::extract_constant(
        &path,
        line,
        column,
        end_column,
        name,
        replace_all,
        dry_run,
        &project_root,
        class,
    )
}

pub fn query_introduce_parameter(
    file: &str,
    line: usize,
    column: usize,
    end_column: usize,
    name: &str,
    type_hint: Option<&str>,
    dry_run: bool,
) -> Result<crate::refactor::IntroduceParameterOutput> {
    let path = resolve_file(file)?;
    let project_root = find_root(&path)?;
    crate::refactor::introduce_parameter(
        &path,
        line,
        column,
        end_column,
        name,
        type_hint,
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

// ── Bulk operations ──────────────────────────────────────────────────────────

pub fn query_bulk_delete_symbol(
    file: &str,
    names_str: &str,
    force: bool,
    dry_run: bool,
) -> Result<crate::refactor::BulkDeleteSymbolOutput> {
    let path = resolve_file(file)?;
    let project_root = find_root(&path)?;
    let names: Vec<String> = names_str.split(',').map(|s| s.trim().to_string()).collect();
    crate::refactor::bulk_delete_symbol(&path, &names, force, dry_run, &project_root)
}

pub fn query_bulk_rename(
    file: &str,
    renames_str: &str,
    scope: Option<&str>,
    dry_run: bool,
) -> Result<crate::refactor::BulkRenameOutput> {
    let path = resolve_file(file)?;
    let project_root = find_root(&path)?;
    let mut renames = Vec::new();
    for pair in renames_str.split(',') {
        let pair = pair.trim();
        let parts: Vec<&str> = pair.splitn(2, ':').collect();
        if parts.len() != 2 {
            return Err(miette::miette!(
                "invalid rename pair '{pair}': expected 'old:new'"
            ));
        }
        renames.push((parts[0].trim().to_string(), parts[1].trim().to_string()));
    }
    let file_only = matches!(scope, Some("file"));
    crate::refactor::bulk_rename(&path, &renames, dry_run, file_only, &project_root)
}

pub fn query_inline_delegate(
    file: &str,
    name: &str,
    dry_run: bool,
) -> Result<crate::refactor::InlineDelegateOutput> {
    let path = resolve_file(file)?;
    let project_root = find_root(&path)?;
    crate::refactor::inline_delegate(&path, name, dry_run, &project_root)
}

pub fn query_extract_class(
    file: &str,
    symbols_str: &str,
    to: &str,
    dry_run: bool,
) -> Result<crate::refactor::ExtractClassOutput> {
    let path = resolve_file(file)?;
    let project_root = find_root(&path)?;
    let to_path = project_root.join(to);
    let names: Vec<String> = symbols_str
        .split(',')
        .map(|s| s.trim().to_string())
        .collect();
    crate::refactor::extract_class(&path, &names, &to_path, dry_run, &project_root)
}

// ── Extract superclass ────────────────────────────────────────────────────────

pub fn query_extract_superclass(
    file: &str,
    symbols_str: &str,
    to: &str,
    class_name: Option<&str>,
    dry_run: bool,
) -> Result<crate::refactor::ExtractSuperclassOutput> {
    let path = resolve_file(file)?;
    let project_root = find_root(&path)?;
    let to_path = project_root.join(to);
    let names: Vec<String> = symbols_str
        .split(',')
        .map(|s| s.trim().to_string())
        .collect();
    crate::refactor::extract_superclass(&path, &names, &to_path, class_name, dry_run, &project_root)
}

// ── Inline variable ──────────────────────────────────────────────────────────

pub fn query_inline_variable(
    file: &str,
    line: usize,
    column: usize,
    dry_run: bool,
) -> Result<crate::refactor::InlineVariableOutput> {
    let path = resolve_file(file)?;
    let project_root = find_root(&path)?;
    crate::refactor::inline_variable(&path, line, column, dry_run, &project_root)
}

// ── Extract guards ───────────────────────────────────────────────────────────

pub fn query_extract_guards(
    file: &str,
    name: &str,
    dry_run: bool,
) -> Result<crate::refactor::ExtractGuardsOutput> {
    let path = resolve_file(file)?;
    let project_root = find_root(&path)?;
    crate::refactor::extract_guards(&path, name, dry_run, &project_root)
}

// ── Split/join declaration ────────────────────────────────────────────────────

pub fn query_split_declaration(
    file: &str,
    line: usize,
    dry_run: bool,
) -> Result<crate::refactor::SplitDeclarationOutput> {
    let path = resolve_file(file)?;
    let project_root = find_root(&path)?;
    crate::refactor::split_declaration(&path, line, dry_run, &project_root)
}

pub fn query_join_declaration(
    file: &str,
    line: usize,
    dry_run: bool,
) -> Result<crate::refactor::JoinDeclarationOutput> {
    let path = resolve_file(file)?;
    let project_root = find_root(&path)?;
    crate::refactor::join_declaration(&path, line, dry_run, &project_root)
}

// ── Convert onready ───────────────────────────────────────────────────────────

pub fn query_convert_onready(
    file: &str,
    name: &str,
    to_ready: bool,
    dry_run: bool,
) -> Result<crate::refactor::ConvertOnreadyOutput> {
    let path = resolve_file(file)?;
    let project_root = find_root(&path)?;
    crate::refactor::convert_onready(&path, name, to_ready, dry_run, &project_root)
}

// ── Convert signal ────────────────────────────────────────────────────────────

pub fn query_convert_signal(
    scene: &str,
    signal: &str,
    from: &str,
    method: &str,
    to_code: bool,
    dry_run: bool,
) -> Result<crate::refactor::ConvertSignalOutput> {
    let path = resolve_file(scene)?;
    let project_root = find_root(&path)?;
    crate::refactor::convert_signal(&path, signal, from, method, to_code, dry_run, &project_root)
}

// ── Convert node path ─────────────────────────────────────────────────────────

pub fn query_convert_node_path(
    file: &str,
    line: usize,
    column: usize,
    dry_run: bool,
) -> Result<crate::refactor::ConvertNodePathOutput> {
    let path = resolve_file(file)?;
    let project_root = find_root(&path)?;
    crate::refactor::convert_node_path(&path, line, column, dry_run, &project_root)
}

// ── Encapsulate field ─────────────────────────────────────────────────────────

pub fn query_encapsulate_field(
    file: &str,
    name: &str,
    backing_field: bool,
    dry_run: bool,
) -> Result<crate::refactor::EncapsulateFieldOutput> {
    let path = resolve_file(file)?;
    let project_root = find_root(&path)?;
    crate::refactor::encapsulate_field(&path, name, backing_field, dry_run, &project_root)
}

// ── Push down member ──────────────────────────────────────────────────────────

pub fn query_push_down_member(
    file: &str,
    name: &str,
    to: &[String],
    force: bool,
    dry_run: bool,
) -> Result<crate::refactor::PushDownMemberOutput> {
    let path = resolve_file(file)?;
    let project_root = find_root(&path)?;
    crate::refactor::push_down_member(&path, name, to, force, dry_run, &project_root)
}

// ── Pull up member ────────────────────────────────────────────────────────────

pub fn query_pull_up_member(
    file: &str,
    name: &str,
    dry_run: bool,
) -> Result<crate::refactor::PullUpMemberOutput> {
    let path = resolve_file(file)?;
    let project_root = find_root(&path)?;
    crate::refactor::pull_up_member(name, &path, dry_run, &project_root)
}

// ── Invert if ────────────────────────────────────────────────────────────────

pub fn query_invert_if(
    file: &str,
    line: usize,
    dry_run: bool,
) -> Result<crate::refactor::InvertIfOutput> {
    let path = resolve_file(file)?;
    let project_root = find_root(&path)?;
    crate::refactor::invert_if(&path, line, dry_run, &project_root)
}

// ── Undo ─────────────────────────────────────────────────────────────────────

pub fn query_undo_list() -> Result<Vec<crate::refactor::UndoEntry>> {
    let cwd = std::env::current_dir().map_err(|e| miette::miette!("cannot get cwd: {e}"))?;
    let project_root = find_root(&cwd)?;
    let stack = crate::refactor::UndoStack::open(&project_root);
    stack.list()
}

pub fn query_undo(id: Option<u64>, dry_run: bool) -> Result<crate::refactor::UndoEntry> {
    let cwd = std::env::current_dir().map_err(|e| miette::miette!("cannot get cwd: {e}"))?;
    let project_root = find_root(&cwd)?;
    let stack = crate::refactor::UndoStack::open(&project_root);

    if dry_run {
        // Just return the entry info without actually undoing
        let entries = stack.list()?;
        if entries.is_empty() {
            return Err(miette::miette!("no undo entries available"));
        }
        if let Some(target_id) = id {
            entries
                .into_iter()
                .find(|e| e.id == target_id)
                .ok_or_else(|| miette::miette!("undo entry {target_id} not found"))
        } else {
            entries
                .into_iter()
                .next()
                .ok_or_else(|| miette::miette!("no undo entries available"))
        }
    } else {
        stack.undo(id, &project_root)
    }
}
