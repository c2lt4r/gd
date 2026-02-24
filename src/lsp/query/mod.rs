mod analysis;
mod edit;
mod navigation;
mod refactor;

pub use analysis::{
    query_code_actions, query_find_implementations, query_safe_delete_file,
    query_symbols,
};
pub use edit::{
    CreateFileOutput, SceneInfoOutput, query_create_file, query_edit_range, query_insert,
    query_replace_body, query_replace_symbol, query_scene_info, query_view,
};
pub use navigation::{
    SceneRefOutput, SignalConnectionOutput, query_completions, query_definition, query_hover,
    query_references, query_references_by_name, query_rename, query_rename_by_name,
    query_scene_refs, query_signal_connections,
};
pub use refactor::{
    query_bulk_delete_symbol, query_bulk_rename, query_change_signature, query_convert_node_path,
    query_convert_onready, query_convert_signal, query_delete_symbol, query_extract_class,
    query_extract_guards, query_extract_method, query_inline_delegate, query_inline_method,
    query_inline_method_by_name, query_inline_variable, query_introduce_parameter,
    query_introduce_variable, query_invert_if, query_join_declaration, query_move_file,
    query_move_symbol, query_split_declaration, query_undo, query_undo_list,
};

use std::path::{Path, PathBuf};

use miette::Result;
use serde::Serialize;
use tower_lsp::lsp_types::{Position, Url, WorkspaceEdit};

// ── Output structs ───────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct RenameOutput {
    pub symbol: String,
    pub new_name: String,
    pub changes: Vec<FileEdits>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

#[derive(Serialize)]
pub struct FileEdits {
    pub file: String,
    pub edits: Vec<TextEditOutput>,
}

#[derive(Serialize)]
pub struct TextEditOutput {
    pub line: u32,
    pub column: u32,
    pub end_line: u32,
    pub end_column: u32,
    pub new_text: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub context: String,
}

#[derive(Serialize)]
pub struct ReferencesOutput {
    pub symbol: String,
    pub references: Vec<ReferenceOutput>,
}

#[derive(Serialize)]
pub struct ReferenceOutput {
    pub file: String,
    pub line: u32,
    pub column: u32,
    pub end_line: u32,
    pub end_column: u32,
    pub context: String,
}

#[derive(Serialize)]
pub struct DefinitionOutput {
    pub symbol: String,
    pub file: String,
    pub line: u32,
    pub column: u32,
    pub end_line: u32,
    pub end_column: u32,
}

#[derive(Serialize)]
pub struct HoverOutput {
    pub content: String,
    pub line: u32,
    pub column: u32,
}

#[derive(Serialize)]
pub struct CompletionOutput {
    pub label: String,
    pub kind: String,
    pub detail: Option<String>,
}

#[derive(Serialize)]
pub struct CodeActionOutput {
    pub title: String,
    pub edits: Vec<FileEditEntry>,
}

#[derive(Serialize)]
pub struct FileEditEntry {
    pub file: String,
    pub line: u32,
    pub column: u32,
    pub end_line: u32,
    pub end_column: u32,
    pub new_text: String,
}

#[derive(Serialize)]
pub struct SymbolOutput {
    pub name: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    pub line: u32,
    pub column: u32,
}

#[derive(Serialize)]
pub struct SafeDeleteFileOutput {
    pub file: String,
    pub references: Vec<FileReference>,
    pub deleted: bool,
}

#[derive(Serialize)]
pub struct FileReference {
    pub file: String,
    pub line: u32,
    pub kind: String,
    pub text: String,
}

#[derive(Serialize)]
pub struct ImplementationsOutput {
    pub method: String,
    pub implementations: Vec<ImplementationEntry>,
}

#[derive(Serialize)]
pub struct ImplementationEntry {
    pub file: String,
    pub line: u32,
    pub end_line: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extends: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub class_name: Option<String>,
}

// ── Helpers ──────────────────────────────────────────────────────────────────

pub(super) fn resolve_file(file: &str) -> Result<PathBuf> {
    let cwd = std::env::current_dir()
        .map_err(|e| miette::miette!("cannot get current directory: {e}"))?;
    let path = cwd.join(file);
    if !path.exists() {
        return Err(miette::miette!("file not found: {file}"));
    }
    Ok(path)
}

pub(super) fn make_uri(path: &Path) -> Result<Url> {
    Url::from_file_path(path).map_err(|()| miette::miette!("invalid path: {}", path.display()))
}

pub(super) fn find_root(path: &Path) -> Result<PathBuf> {
    crate::core::config::find_project_root(path)
        .ok_or_else(|| miette::miette!("no project.godot found above {}", path.display()))
}

pub(super) fn url_to_relative(url: &Url, base: &Path) -> String {
    if let Ok(path) = url.to_file_path() {
        return crate::core::fs::relative_slash(&path, base);
    }
    url.to_string()
}

fn position_to_byte_offset(source: &str, pos: Position) -> usize {
    let mut line = 0u32;
    let mut col = 0u32;
    for (i, ch) in source.char_indices() {
        if line == pos.line && col == pos.character {
            return i;
        }
        if ch == '\n' {
            if line == pos.line {
                return i;
            }
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }
    source.len()
}

// ── Apply rename ─────────────────────────────────────────────────────────────

pub fn apply_rename(output: &RenameOutput, project_root: &Path) -> Result<usize> {
    let mut files_changed = 0;
    for file_edits in &output.changes {
        let path = project_root.join(&file_edits.file);
        let mut content = std::fs::read_to_string(&path)
            .map_err(|e| miette::miette!("cannot read {}: {e}", file_edits.file))?;

        // Sort edits in reverse order to preserve byte offsets
        let mut edits: Vec<&TextEditOutput> = file_edits.edits.iter().collect();
        edits.sort_by(|a, b| b.line.cmp(&a.line).then(b.column.cmp(&a.column)));

        for edit in edits {
            let start =
                position_to_byte_offset(&content, Position::new(edit.line - 1, edit.column - 1));
            let end = position_to_byte_offset(
                &content,
                Position::new(edit.end_line - 1, edit.end_column - 1),
            );
            content.replace_range(start..end, &edit.new_text);
        }

        std::fs::write(&path, &content)
            .map_err(|e| miette::miette!("cannot write {}: {e}", file_edits.file))?;
        files_changed += 1;
    }
    Ok(files_changed)
}

/// Convert a `WorkspaceEdit` from `rename_cross_file` into a `RenameOutput`.
/// Used by `bulk_rename` to avoid going through `resolve_file` / `find_root`.
pub fn convert_rename_edit(
    edit: &WorkspaceEdit,
    project_root: &Path,
    old_name: &str,
    new_name: &str,
) -> RenameOutput {
    let changes = convert_workspace_edit(edit, project_root);
    RenameOutput {
        symbol: old_name.to_string(),
        new_name: new_name.to_string(),
        changes,
        summary: None,
        warnings: Vec::new(),
    }
}

// ── Internal converters ──────────────────────────────────────────────────────

fn convert_workspace_edit(edit: &WorkspaceEdit, base: &Path) -> Vec<FileEdits> {
    let Some(changes) = &edit.changes else {
        return vec![];
    };

    changes
        .iter()
        .map(|(url, edits)| {
            let file = url_to_relative(url, base);
            // Read file content once for context extraction
            let file_content = url
                .to_file_path()
                .ok()
                .and_then(|p| std::fs::read_to_string(p).ok())
                .unwrap_or_default();
            let lines: Vec<&str> = file_content.lines().collect();
            let edits = edits
                .iter()
                .map(|e| {
                    let context = lines
                        .get(e.range.start.line as usize)
                        .unwrap_or(&"")
                        .trim()
                        .to_string();
                    TextEditOutput {
                        line: e.range.start.line + 1,
                        column: e.range.start.character + 1,
                        end_line: e.range.end.line + 1,
                        end_column: e.range.end.character + 1,
                        new_text: e.new_text.clone(),
                        context,
                    }
                })
                .collect();
            FileEdits { file, edits }
        })
        .collect()
}
