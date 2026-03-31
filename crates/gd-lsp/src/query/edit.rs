use miette::Result;
use serde::Serialize;

use super::{ReferenceOutput, find_root, resolve_file};

// ── Output structs ───────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct SymbolViewOutput {
    pub file: String,
    pub name: String,
    pub kind: String,
    pub start_line: u32,
    pub end_line: u32,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub references: Option<Vec<ReferenceOutput>>,
}

#[derive(Serialize)]
pub struct CreateFileOutput {
    pub file: String,
    pub extends: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub class_name: Option<String>,
    pub applied: bool,
    pub lines: u32,
}

#[derive(Serialize)]
pub struct ViewOutput {
    pub file: String,
    pub start_line: u32,
    pub end_line: u32,
    pub total_lines: u32,
    pub content: String,
}

#[derive(Serialize)]
pub struct SceneInfoOutput {
    pub file: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nodes: Option<Vec<SceneNodeOutput>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext_resources: Option<Vec<gd_core::scene::ExtResource>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sub_resources: Option<Vec<gd_core::scene::SubResource>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connections: Option<Vec<gd_core::scene::Connection>>,
}

#[derive(Serialize)]
pub struct SceneNodeOutput {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r#type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub script: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<String>,
}

// ── AST-aware edit commands ──────────────────────────────────────────────────

pub fn query_replace_body(
    file: &str,
    name: &str,
    class: Option<&str>,
    content: &str,
    no_format: bool,
) -> Result<crate::refactor::EditOutput> {
    let path = resolve_file(file)?;
    let project_root = find_root(&path)?;
    crate::refactor::replace_body(&path, name, class, content, no_format, &project_root)
}

pub fn query_insert(
    file: &str,
    anchor: &str,
    after: bool,
    class: Option<&str>,
    content: &str,
    no_format: bool,
) -> Result<crate::refactor::EditOutput> {
    let path = resolve_file(file)?;
    let project_root = find_root(&path)?;
    crate::refactor::insert(&path, anchor, after, class, content, no_format, &project_root)
}

pub fn query_replace_symbol(
    file: &str,
    name: &str,
    class: Option<&str>,
    content: &str,
    no_format: bool,
) -> Result<crate::refactor::EditOutput> {
    let path = resolve_file(file)?;
    let project_root = find_root(&path)?;
    crate::refactor::replace_symbol(&path, name, class, content, no_format, &project_root)
}

// ── Insert into class body ──────────────────────────────────────────────────

pub fn query_insert_into(
    file: &str,
    class_name: &str,
    content: &str,
    no_format: bool,
) -> Result<crate::refactor::EditOutput> {
    let path = resolve_file(file)?;
    let project_root = find_root(&path)?;
    crate::refactor::insert_into(&path, class_name, content, no_format, &project_root)
}

// ── Remove (delete symbol) ──────────────────────────────────────────────────

pub fn query_remove(
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

// ── Extract (move symbol to another file) ───────────────────────────────────

#[allow(clippy::too_many_arguments)]
pub fn query_extract(
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

// ── Create file ──────────────────────────────────────────────────────────────

pub fn query_create_file(
    file: &str,
    extends: &str,
    class_name: Option<&str>,
    custom_content: Option<&str>,
    force: bool,
) -> Result<CreateFileOutput> {
    let path = std::path::Path::new(file);

    let full_path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|e| miette::miette!("cannot get current directory: {e}"))?
            .join(path)
    };

    if full_path.exists() && !force {
        return Err(miette::miette!("file already exists: {file}"));
    }

    let content = if let Some(custom) = custom_content {
        use std::fmt::Write;
        let mut buf = String::new();
        // Prepend class_name/extends header when flags are provided
        if class_name.is_some() || extends != "Node" {
            if let Some(cn) = class_name {
                let _ = writeln!(buf, "class_name {cn}");
            }
            let _ = writeln!(buf, "extends {extends}");
            buf.push('\n');
        }
        buf.push_str(custom);
        if !buf.ends_with('\n') {
            buf.push('\n');
        }
        buf
    } else {
        use std::fmt::Write;
        let mut buf = String::new();
        if let Some(cn) = class_name {
            let _ = writeln!(buf, "class_name {cn}");
        }
        let _ = writeln!(buf, "extends {extends}");
        buf.push_str("\n\n");
        buf.push_str("func _ready() -> void:\n\tpass\n\n\n");
        buf.push_str("func _process(delta: float) -> void:\n\tpass\n");
        buf
    };

    let lines = content.lines().count() as u32;

    if let Some(parent) = full_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| miette::miette!("cannot create directories: {e}"))?;
    }
    std::fs::write(&full_path, &content)
        .map_err(|e| miette::miette!("cannot write file: {e}"))?;

    Ok(CreateFileOutput {
        file: file.to_string(),
        extends: extends.to_string(),
        class_name: class_name.map(std::string::ToString::to_string),
        applied: true,
        lines,
    })
}

// ── View ─────────────────────────────────────────────────────────────────────

pub fn query_view(
    file: &str,
    start_line: Option<usize>,
    end_line: Option<usize>,
    context: Option<usize>,
) -> Result<ViewOutput> {
    let resolved = resolve_file(file)?;
    let source =
        std::fs::read_to_string(&resolved).map_err(|e| miette::miette!("cannot read file: {e}"))?;
    let all_lines: Vec<&str> = source.lines().collect();
    let total = all_lines.len();

    let mut start = start_line.unwrap_or(1).max(1);
    let mut end = end_line.unwrap_or(total).min(total);

    if let Some(ctx) = context {
        start = start.saturating_sub(ctx).max(1);
        end = (end + ctx).min(total);
    }

    let content = all_lines[start - 1..end].join("\n");

    let project_root = find_root(&resolved)?;
    let rel = gd_core::fs::relative_slash(&resolved, &project_root);

    Ok(ViewOutput {
        file: rel,
        start_line: start as u32,
        end_line: end as u32,
        total_lines: total as u32,
        content,
    })
}

// ── View symbol ─────────────────────────────────────────────────────────────

pub fn query_view_symbol(file: &str, symbol: &str, include_refs: bool) -> Result<SymbolViewOutput> {
    let path = resolve_file(file)?;
    let project_root = find_root(&path)?;
    let rel = gd_core::fs::relative_slash(&path, &project_root);

    let source =
        std::fs::read_to_string(&path).map_err(|e| miette::miette!("cannot read file: {e}"))?;
    let tree = gd_core::parser::parse(&source)?;
    let file_ast = gd_core::gd_ast::convert(&tree, &source);

    let decl = crate::refactor::find_declaration_by_name(&file_ast, symbol)
        .ok_or_else(|| miette::miette!("symbol '{symbol}' not found in {rel}"))?;

    let kind = crate::refactor::declaration_kind_str(decl.kind()).to_string();
    let (full_start, full_end) = crate::refactor::declaration_full_range(decl, &source);

    let content = source[full_start..full_end].to_string();
    let start_line = source[..full_start].matches('\n').count() as u32 + 1;
    let end_line = source[..full_end].matches('\n').count() as u32;

    let references = if include_refs {
        let name_node = decl.child_by_field_name("name");
        let (line, column) = if let Some(n) = name_node {
            (n.start_position().row + 1, n.start_position().column + 1)
        } else {
            (
                decl.start_position().row + 1,
                decl.start_position().column + 1,
            )
        };
        // Use the references query to find all workspace references
        let refs_result = super::query_references_by_name(symbol, Some(file), None)?;
        Some(
            refs_result
                .references
                .into_iter()
                .filter(|r| {
                    // Exclude the definition itself
                    !(r.file == rel && r.line == line as u32 && r.column == column as u32)
                })
                .collect(),
        )
    } else {
        None
    };

    Ok(SymbolViewOutput {
        file: rel,
        name: symbol.to_string(),
        kind,
        start_line,
        end_line,
        content,
        references,
    })
}

// ── Scene info query ─────────────────────────────────────────────────────────

pub fn query_scene_info(file: &str, nodes_only: bool) -> Result<SceneInfoOutput> {
    let path = resolve_file(file)?;
    let data = gd_core::scene::parse_scene_file(&path)?;
    let cwd = std::env::current_dir().unwrap_or_default();
    let rel = gd_core::fs::relative_slash(&path, &cwd);

    let nodes: Vec<SceneNodeOutput> = data
        .nodes
        .iter()
        .map(|n| SceneNodeOutput {
            name: n.name.clone(),
            r#type: n.type_name.clone(),
            parent: n.parent.clone(),
            script: n.script.clone(),
            groups: n.groups.clone(),
        })
        .collect();

    if nodes_only {
        Ok(SceneInfoOutput {
            file: rel,
            nodes: Some(nodes),
            ext_resources: None,
            sub_resources: None,
            connections: None,
        })
    } else {
        Ok(SceneInfoOutput {
            file: rel,
            nodes: Some(nodes),
            ext_resources: Some(data.ext_resources),
            sub_resources: Some(data.sub_resources),
            connections: Some(data.connections),
        })
    }
}
