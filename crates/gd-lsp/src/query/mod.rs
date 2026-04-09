mod analysis;
mod edit;
mod navigation;
mod refactor;

pub use analysis::{
    query_code_actions, query_find_implementations, query_safe_delete_file, query_symbols,
};
pub use edit::{
    CreateFileOutput, SceneInfoOutput, SceneNodeOutput, SymbolViewOutput, query_create_file,
    query_extract, query_insert_cmd, query_remove, query_replace, query_scene_info, query_view,
    query_view_symbol,
};
pub use navigation::{
    SceneRefOutput, SignalConnectionOutput, query_completions, query_definition, query_hover,
    query_references, query_references_by_name, query_rename, query_rename_by_name,
    query_scene_refs, query_signal_connections,
};
pub use refactor::{query_change_signature, query_extract_method, query_move_file};

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
    gd_core::config::find_project_root(path)
        .ok_or_else(|| miette::miette!("no project.godot found above {}", path.display()))
}

pub(super) fn url_to_relative(url: &Url, base: &Path) -> String {
    if let Ok(path) = url.to_file_path() {
        return gd_core::fs::relative_slash(&path, base);
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
    use crate::refactor::mutation::{self, MutationSet};
    use gd_core::ast_owned::OwnedFile;
    use std::collections::HashSet;

    let mut ms = MutationSet::new();

    for file_edits in &output.changes {
        let path = project_root.join(&file_edits.file);
        let content = std::fs::read_to_string(&path)
            .map_err(|e| miette::miette!("cannot read {}: {e}", file_edits.file))?;

        // Build target byte ranges from edit positions
        let targets: HashSet<(usize, usize)> = file_edits
            .edits
            .iter()
            .map(|edit| {
                let start = position_to_byte_offset(
                    &content,
                    Position::new(edit.line - 1, edit.column - 1),
                );
                let end = position_to_byte_offset(
                    &content,
                    Position::new(edit.end_line - 1, edit.end_column - 1),
                );
                (start, end)
            })
            .collect();

        // Parse → typed AST → owned AST
        let tree = gd_core::parser::parse(&content)
            .map_err(|e| miette::miette!("parse error in {}: {e}", file_edits.file))?;
        let gd_file = gd_core::gd_ast::convert(&tree, &content);
        let mut owned = OwnedFile::from_borrowed(&gd_file);

        // Phase 1: rename declaration-level names (func, var, signal, etc.)
        rewrite_rename_decls(
            &mut owned,
            &output.symbol,
            &output.new_name,
            &targets,
            &content,
        );

        // Phase 2: rename expression-level names via AST rewriter
        let old = &output.symbol;
        let new = &output.new_name;
        let rewritten = gd_core::rewriter::rewrite_file(owned, &|expr| {
            rewrite_rename_expr(expr, old, new, &targets, &content)
        });

        let result = gd_core::printer::print_file(&rewritten, &content);
        ms.insert(path, result);
    }

    let result = mutation::commit(&ms, project_root)?;
    Ok(result.files_written)
}

// ── Rewriter rule for expression-level renames ──────────────────────────────

fn rewrite_rename_expr(
    expr: gd_core::ast_owned::OwnedExpr,
    old_name: &str,
    new_name: &str,
    targets: &std::collections::HashSet<(usize, usize)>,
    source: &str,
) -> gd_core::ast_owned::OwnedExpr {
    use gd_core::ast_owned::OwnedExpr;

    enum Kind {
        Ident,
        Method,
        Property { dot_pos: usize },
        Super,
    }

    let kind = match &expr {
        OwnedExpr::Ident { name, span } if name == old_name => span
            .filter(|s| targets.contains(&(s.start, s.end)))
            .map(|_| Kind::Ident),
        OwnedExpr::MethodCall {
            span: Some(s),
            method,
            ..
        } if method == old_name => {
            let text = &source[s.start..s.end];
            let pat = format!(".{method}(");
            text.find(&pat).and_then(|idx| {
                let m_start = s.start + idx + 1;
                let m_end = m_start + method.len();
                targets.contains(&(m_start, m_end)).then_some(Kind::Method)
            })
        }
        OwnedExpr::PropertyAccess {
            span: Some(s),
            property,
            ..
        } if property == old_name => {
            let p_start = s.end - property.len();
            targets
                .contains(&(p_start, s.end))
                .then_some(Kind::Property {
                    dot_pos: p_start.saturating_sub(1),
                })
        }
        OwnedExpr::SuperCall {
            span: Some(s),
            method: Some(m),
            ..
        } if m == old_name => {
            let m_start = s.start + 6; // after "super."
            let m_end = m_start + m.len();
            targets.contains(&(m_start, m_end)).then_some(Kind::Super)
        }
        _ => None,
    };

    match kind {
        Some(Kind::Ident) => OwnedExpr::Ident {
            span: None,
            name: new_name.to_string(),
        },
        Some(Kind::Method) => {
            let OwnedExpr::MethodCall { receiver, args, .. } = expr else {
                unreachable!()
            };
            OwnedExpr::MethodCall {
                span: None,
                receiver,
                method: new_name.to_string(),
                args,
            }
        }
        Some(Kind::Property { dot_pos }) => {
            let OwnedExpr::PropertyAccess { mut receiver, .. } = expr else {
                unreachable!()
            };
            // Chained property access nodes may share the parent
            // tree-sitter node span (covering the whole chain).
            // Trim the receiver span so the printer only emits the
            // receiver text, not the full chain including this property.
            if receiver.span().is_some_and(|rs| rs.end > dot_pos) {
                trim_expr_span(&mut receiver, dot_pos);
            }
            OwnedExpr::PropertyAccess {
                span: None,
                receiver,
                property: new_name.to_string(),
            }
        }
        Some(Kind::Super) => {
            let OwnedExpr::SuperCall { args, .. } = expr else {
                unreachable!()
            };
            OwnedExpr::SuperCall {
                span: None,
                method: Some(new_name.to_string()),
                args,
            }
        }
        None => expr,
    }
}

// ── Declaration-level rename helpers ────────────────────────────────────────

fn rewrite_rename_decls(
    file: &mut gd_core::ast_owned::OwnedFile,
    old_name: &str,
    new_name: &str,
    targets: &std::collections::HashSet<(usize, usize)>,
    source: &str,
) {
    if file.class_name.as_deref() == Some(old_name)
        && let Some(span) = file.span
        && let Some(r) = find_kw_name(source, span, "class_name", old_name)
        && targets.contains(&r)
    {
        file.class_name = Some(new_name.to_string());
        file.span = None;
    }
    let mut dirty = false;
    for d in &mut file.declarations {
        dirty |= rename_in_decl(d, old_name, new_name, targets, source);
    }
    if dirty {
        file.span = None;
    }
}

#[allow(clippy::too_many_lines)]
fn rename_in_decl(
    decl: &mut gd_core::ast_owned::OwnedDecl,
    old_name: &str,
    new_name: &str,
    targets: &std::collections::HashSet<(usize, usize)>,
    source: &str,
) -> bool {
    use gd_core::ast_owned::OwnedDecl;
    match decl {
        OwnedDecl::Func(f) => {
            let mut changed = false;
            if f.name == old_name
                && let Some(span) = f.span
                && let Some(r) = find_kw_name(source, span, "func", old_name)
                && targets.contains(&r)
            {
                f.name = new_name.to_string();
                f.span = None;
                changed = true;
            }
            for p in &mut f.params {
                if p.name == old_name
                    && let Some(ps) = p.span
                    && let Some(r) = find_name_at(source, ps, old_name)
                    && targets.contains(&r)
                {
                    p.name = new_name.to_string();
                    p.span = None;
                    f.span = None;
                    changed = true;
                }
            }
            for s in &mut f.body {
                if rename_in_stmt(s, old_name, new_name, targets, source) {
                    f.span = None;
                    changed = true;
                }
            }
            changed
        }
        OwnedDecl::Var(v) => rename_var_decl(v, old_name, new_name, targets, source),
        OwnedDecl::Signal(s) => {
            let mut changed = false;
            if s.name == old_name
                && let Some(span) = s.span
                && let Some(r) = find_kw_name(source, span, "signal", old_name)
                && targets.contains(&r)
            {
                s.name = new_name.to_string();
                s.span = None;
                changed = true;
            }
            for p in &mut s.params {
                if p.name == old_name
                    && let Some(ps) = p.span
                    && let Some(r) = find_name_at(source, ps, old_name)
                    && targets.contains(&r)
                {
                    p.name = new_name.to_string();
                    p.span = None;
                    s.span = None;
                    changed = true;
                }
            }
            changed
        }
        OwnedDecl::Enum(e) => {
            let mut changed = false;
            if e.name == old_name
                && let Some(span) = e.span
                && let Some(r) = find_kw_name(source, span, "enum", old_name)
                && targets.contains(&r)
            {
                e.name = new_name.to_string();
                e.span = None;
                changed = true;
            }
            let mut member_renamed = false;
            for m in &mut e.members {
                if m.name == old_name
                    && let Some(ms) = m.span
                    && let Some(r) = find_name_at(source, ms, old_name)
                    && targets.contains(&r)
                {
                    m.name = new_name.to_string();
                    member_renamed = true;
                }
            }
            if member_renamed {
                // Clear all member spans for consistent printer output
                for m in &mut e.members {
                    m.span = None;
                }
                e.span = None;
                changed = true;
            }
            changed
        }
        OwnedDecl::Class(c) => {
            let mut changed = false;
            if c.name == old_name
                && let Some(span) = c.span
                && let Some(r) = find_kw_name(source, span, "class", old_name)
                && targets.contains(&r)
            {
                c.name = new_name.to_string();
                c.span = None;
                changed = true;
            }
            for d in &mut c.declarations {
                if rename_in_decl(d, old_name, new_name, targets, source) {
                    c.span = None;
                    changed = true;
                }
            }
            changed
        }
        OwnedDecl::Stmt(s) => rename_in_stmt(s, old_name, new_name, targets, source),
    }
}

fn rename_var_decl(
    v: &mut gd_core::ast_owned::OwnedVar,
    old_name: &str,
    new_name: &str,
    targets: &std::collections::HashSet<(usize, usize)>,
    source: &str,
) -> bool {
    if v.name != old_name {
        return false;
    }
    let Some(span) = v.span else { return false };
    let kw = if v.is_const { "const" } else { "var" };
    let Some(r) = find_kw_name(source, span, kw, old_name) else {
        return false;
    };
    if !targets.contains(&r) {
        return false;
    }
    v.name = new_name.to_string();
    v.span = None;
    true
}

fn rename_in_stmt(
    stmt: &mut gd_core::ast_owned::OwnedStmt,
    old_name: &str,
    new_name: &str,
    targets: &std::collections::HashSet<(usize, usize)>,
    source: &str,
) -> bool {
    use gd_core::ast_owned::OwnedStmt;
    match stmt {
        OwnedStmt::Var(v) => rename_var_decl(v, old_name, new_name, targets, source),
        OwnedStmt::For {
            span, var, body, ..
        } => {
            let mut changed = false;
            if var.as_str() == old_name
                && let Some(s) = *span
                && let Some(r) = find_kw_name(source, s, "for", old_name)
                && targets.contains(&r)
            {
                *var = new_name.to_string();
                *span = None;
                changed = true;
            }
            for s in body.iter_mut() {
                if rename_in_stmt(s, old_name, new_name, targets, source) {
                    *span = None;
                    changed = true;
                }
            }
            changed
        }
        OwnedStmt::If(i) => {
            let mut changed = false;
            for s in &mut i.body {
                changed |= rename_in_stmt(s, old_name, new_name, targets, source);
            }
            for (_, stmts) in &mut i.elif_branches {
                for s in stmts {
                    changed |= rename_in_stmt(s, old_name, new_name, targets, source);
                }
            }
            if let Some(ref mut eb) = i.else_body {
                for s in eb {
                    changed |= rename_in_stmt(s, old_name, new_name, targets, source);
                }
            }
            if changed {
                i.span = None;
            }
            changed
        }
        OwnedStmt::While { span, body, .. } => {
            let mut changed = false;
            for s in body.iter_mut() {
                changed |= rename_in_stmt(s, old_name, new_name, targets, source);
            }
            if changed {
                *span = None;
            }
            changed
        }
        OwnedStmt::Match { span, arms, .. } => {
            let mut changed = false;
            for arm in arms.iter_mut() {
                for s in &mut arm.body {
                    if rename_in_stmt(s, old_name, new_name, targets, source) {
                        arm.span = None;
                        changed = true;
                    }
                }
            }
            if changed {
                *span = None;
            }
            changed
        }
        _ => false,
    }
}

/// Find `"keyword name"` in source within `span`, returning byte range of
/// `name`.  Skips matches where `name` is a prefix of a longer identifier.
fn find_kw_name(
    source: &str,
    span: gd_core::ast_owned::Span,
    keyword: &str,
    name: &str,
) -> Option<(usize, usize)> {
    let text = &source[span.start..span.end];
    let pat = format!("{keyword} {name}");
    let mut pos = 0;
    while let Some(idx) = text[pos..].find(&pat) {
        let abs = pos + idx;
        let after = abs + pat.len();
        if after >= text.len()
            || !(text.as_bytes()[after].is_ascii_alphanumeric() || text.as_bytes()[after] == b'_')
        {
            let start = span.start + abs + keyword.len() + 1;
            return Some((start, start + name.len()));
        }
        pos = abs + 1;
    }
    None
}

/// Find the first word-bounded occurrence of `name` within `span`.
fn find_name_at(
    source: &str,
    span: gd_core::ast_owned::Span,
    name: &str,
) -> Option<(usize, usize)> {
    let text = &source.as_bytes()[span.start..span.end];
    let nb = name.as_bytes();
    let mut i = 0;
    while i + nb.len() <= text.len() {
        if &text[i..i + nb.len()] == nb {
            let ok_before = i == 0 || !(text[i - 1].is_ascii_alphanumeric() || text[i - 1] == b'_');
            let j = i + nb.len();
            let ok_after = j >= text.len() || !(text[j].is_ascii_alphanumeric() || text[j] == b'_');
            if ok_before && ok_after {
                return Some((span.start + i, span.start + j));
            }
        }
        i += 1;
    }
    None
}

/// Trim an expression's span so it ends at `max_end`.
/// Used to fix chained `PropertyAccess` nodes that share the parent tree-sitter
/// node span (covering the entire chain instead of just their own text).
fn trim_expr_span(expr: &mut gd_core::ast_owned::OwnedExpr, max_end: usize) {
    use gd_core::ast_owned::{OwnedExpr, Span};
    match expr {
        OwnedExpr::PropertyAccess { span, .. }
        | OwnedExpr::MethodCall { span, .. }
        | OwnedExpr::Ident { span, .. } => {
            if let Some(s) = span
                && s.end > max_end
            {
                *span = Some(Span {
                    start: s.start,
                    end: max_end,
                });
            }
        }
        _ => {}
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
