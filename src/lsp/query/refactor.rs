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
) -> Result<crate::lsp::refactor::DeleteSymbolOutput> {
    let path = resolve_file(file)?;
    let project_root = find_root(&path)?;
    crate::lsp::refactor::delete_symbol(&path, name, line, force, dry_run, &project_root, class)
}

pub fn query_move_symbol(
    name: &str,
    from: &str,
    to: &str,
    dry_run: bool,
    class: Option<&str>,
    target_class: Option<&str>,
    update_callers: bool,
) -> Result<crate::lsp::refactor::MoveSymbolOutput> {
    let from_path = resolve_file(from)?;
    let project_root = find_root(&from_path)?;
    let to_path = project_root.join(to);
    let mut result = crate::lsp::refactor::move_symbol(
        name,
        &from_path,
        &to_path,
        dry_run,
        &project_root,
        class,
        target_class,
    )?;

    // Update callers after successful move
    if update_callers && !dry_run && result.applied && !result.preloads.is_empty() {
        let from_relative = crate::core::fs::relative_slash(&from_path, &project_root);
        let to_relative = crate::core::fs::relative_slash(&to_path, &project_root);
        let source_res = format!("res://{from_relative}");
        let dest_res = format!("res://{to_relative}");

        match crate::lsp::refactor::update_callers_after_move(
            &source_res,
            &dest_res,
            &result.preloads,
            &project_root,
        ) {
            Ok(updates) => {
                for update in &updates {
                    result.warnings.push(format!(
                        "updated {}: added {}",
                        update.file, update.added_preload
                    ));
                }
            }
            Err(e) => {
                result.warnings.push(format!("caller update error: {e}"));
            }
        }
    }

    Ok(result)
}

pub fn query_extract_method(
    file: &str,
    start_line: usize,
    end_line: usize,
    name: &str,
    dry_run: bool,
) -> Result<crate::lsp::refactor::ExtractMethodOutput> {
    let path = resolve_file(file)?;
    let project_root = find_root(&path)?;
    crate::lsp::refactor::extract_method(&path, start_line, end_line, name, dry_run, &project_root)
}

pub fn query_inline_method(
    file: &str,
    line: usize,
    column: usize,
    dry_run: bool,
) -> Result<crate::lsp::refactor::InlineMethodOutput> {
    let path = resolve_file(file)?;
    let project_root = find_root(&path)?;
    crate::lsp::refactor::inline_method(&path, line, column, dry_run, &project_root)
}

pub fn query_inline_method_by_name(
    file: &str,
    name: &str,
    all: bool,
    dry_run: bool,
) -> Result<crate::lsp::refactor::InlineMethodByNameOutput> {
    let path = resolve_file(file)?;
    let project_root = find_root(&path)?;
    crate::lsp::refactor::inline_method_by_name(&path, name, all, dry_run, &project_root)
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
) -> Result<crate::lsp::refactor::ChangeSignatureOutput> {
    let path = resolve_file(file)?;
    let project_root = find_root(&path)?;
    crate::lsp::refactor::change_signature(
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

pub fn query_introduce_variable(
    file: &str,
    line: usize,
    column: usize,
    end_column: usize,
    name: &str,
    dry_run: bool,
) -> Result<crate::lsp::refactor::IntroduceVariableOutput> {
    let path = resolve_file(file)?;
    let project_root = find_root(&path)?;
    crate::lsp::refactor::introduce_variable(
        &path,
        line,
        column,
        end_column,
        name,
        dry_run,
        &project_root,
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
) -> Result<crate::lsp::refactor::IntroduceParameterOutput> {
    let path = resolve_file(file)?;
    let project_root = find_root(&path)?;
    crate::lsp::refactor::introduce_parameter(
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

// ── Bulk operations ──────────────────────────────────────────────────────────

pub fn query_bulk_delete_symbol(
    file: &str,
    names_str: &str,
    force: bool,
    dry_run: bool,
) -> Result<crate::lsp::refactor::BulkDeleteSymbolOutput> {
    let path = resolve_file(file)?;
    let project_root = find_root(&path)?;
    let names: Vec<String> = names_str.split(',').map(|s| s.trim().to_string()).collect();
    crate::lsp::refactor::bulk_delete_symbol(&path, &names, force, dry_run, &project_root)
}

pub fn query_bulk_rename(
    file: &str,
    renames_str: &str,
    dry_run: bool,
) -> Result<crate::lsp::refactor::BulkRenameOutput> {
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
    crate::lsp::refactor::bulk_rename(&path, &renames, dry_run, &project_root)
}

pub fn query_inline_delegate(
    file: &str,
    name: &str,
    dry_run: bool,
) -> Result<crate::lsp::refactor::InlineDelegateOutput> {
    let path = resolve_file(file)?;
    let project_root = find_root(&path)?;
    crate::lsp::refactor::inline_delegate(&path, name, dry_run, &project_root)
}

pub fn query_extract_class(
    file: &str,
    symbols_str: &str,
    to: &str,
    dry_run: bool,
) -> Result<crate::lsp::refactor::ExtractClassOutput> {
    let path = resolve_file(file)?;
    let project_root = find_root(&path)?;
    let to_path = project_root.join(to);
    let names: Vec<String> = symbols_str
        .split(',')
        .map(|s| s.trim().to_string())
        .collect();
    crate::lsp::refactor::extract_class(&path, &names, &to_path, dry_run, &project_root)
}
