use miette::Result;
use serde::Serialize;

use super::{find_root, resolve_file};

// ── Output structs ───────────────────────────────────────────────────────────

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
    dry_run: bool,
) -> Result<crate::refactor::EditOutput> {
    let path = resolve_file(file)?;
    let project_root = find_root(&path)?;
    crate::refactor::replace_body(
        &path,
        name,
        class,
        content,
        no_format,
        dry_run,
        &project_root,
    )
}

pub fn query_insert(
    file: &str,
    anchor: &str,
    after: bool,
    class: Option<&str>,
    content: &str,
    no_format: bool,
    dry_run: bool,
) -> Result<crate::refactor::EditOutput> {
    let path = resolve_file(file)?;
    let project_root = find_root(&path)?;
    crate::refactor::insert(
        &path,
        anchor,
        after,
        class,
        content,
        no_format,
        dry_run,
        &project_root,
    )
}

pub fn query_replace_symbol(
    file: &str,
    name: &str,
    class: Option<&str>,
    content: &str,
    no_format: bool,
    dry_run: bool,
) -> Result<crate::refactor::EditOutput> {
    let path = resolve_file(file)?;
    let project_root = find_root(&path)?;
    crate::refactor::replace_symbol(
        &path,
        name,
        class,
        content,
        no_format,
        dry_run,
        &project_root,
    )
}

pub fn query_edit_range(
    file: &str,
    start_line: usize,
    end_line: usize,
    content: &str,
    no_format: bool,
    dry_run: bool,
) -> Result<crate::refactor::EditOutput> {
    let path = resolve_file(file)?;
    let project_root = find_root(&path)?;
    crate::refactor::edit_range(
        &path,
        start_line,
        end_line,
        content,
        no_format,
        dry_run,
        &project_root,
    )
}

// ── Create file ──────────────────────────────────────────────────────────────

pub fn query_create_file(
    file: &str,
    extends: &str,
    class_name: Option<&str>,
    custom_content: Option<&str>,
    force: bool,
    dry_run: bool,
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

    if !dry_run {
        if let Some(parent) = full_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| miette::miette!("cannot create directories: {e}"))?;
        }
        std::fs::write(&full_path, &content)
            .map_err(|e| miette::miette!("cannot write file: {e}"))?;
    }

    Ok(CreateFileOutput {
        file: file.to_string(),
        extends: extends.to_string(),
        class_name: class_name.map(std::string::ToString::to_string),
        applied: !dry_run,
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
