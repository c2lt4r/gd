use std::collections::{HashMap, HashSet};
use std::path::Path;

use miette::Result;
use serde::Serialize;
use tree_sitter::Node;

// ── Output structs ──────────────────────────────────────────────────────────

#[derive(Serialize, Debug)]
pub struct DeleteSymbolOutput {
    pub symbol: String,
    pub kind: String,
    pub file: String,
    pub removed_lines: LineRange,
    pub references: Vec<RefLocation>,
    pub applied: bool,
}

#[derive(Serialize, Debug)]
pub struct LineRange {
    pub start: u32,
    pub end: u32,
}

#[derive(Serialize, Clone, Debug)]
pub struct RefLocation {
    pub file: String,
    pub line: u32,
    pub column: u32,
    pub end_line: u32,
    pub end_column: u32,
}

#[derive(Serialize)]
pub struct MoveSymbolOutput {
    pub symbol: String,
    pub kind: String,
    pub from: String,
    pub to: String,
    pub applied: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub preloads: Vec<PreloadRef>,
}

#[derive(Serialize, Debug)]
pub struct ExtractMethodOutput {
    pub function: String,
    pub parameters: Vec<ParameterOutput>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub returns: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub return_vars: Vec<String>,
    pub call_site: String,
    pub file: String,
    pub applied: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

#[derive(Serialize, Debug)]
pub struct ParameterOutput {
    pub name: String,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub type_hint: Option<String>,
}

// ── Shared helpers ──────────────────────────────────────────────────────────

const DECLARATION_KINDS: &[&str] = &[
    "function_definition",
    "constructor_definition",
    "variable_statement",
    "const_statement",
    "signal_statement",
    "enum_definition",
    "class_definition",
    "class_name_statement",
];

fn declaration_kind_str(kind: &str) -> &str {
    match kind {
        "function_definition" | "constructor_definition" => "function",
        "variable_statement" => "variable",
        "const_statement" => "constant",
        "signal_statement" => "signal",
        "enum_definition" => "enum",
        "class_definition" => "class",
        "class_name_statement" => "class_name",
        _ => "unknown",
    }
}

fn get_declaration_name(node: Node, source: &str) -> Option<String> {
    if node.kind() == "constructor_definition" {
        return Some("_init".to_string());
    }
    if node.kind() == "class_name_statement" {
        let name_node = node.child(1)?;
        return Some(name_node.utf8_text(source.as_bytes()).ok()?.to_string());
    }
    let name_node = node.child_by_field_name("name")?;
    Some(name_node.utf8_text(source.as_bytes()).ok()?.to_string())
}

/// Find a top-level declaration by name.
fn find_declaration_by_name<'a>(root: Node<'a>, source: &str, name: &str) -> Option<Node<'a>> {
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        if !DECLARATION_KINDS.contains(&child.kind()) {
            continue;
        }
        if let Some(decl_name) = get_declaration_name(child, source)
            && decl_name == name
        {
            return Some(child);
        }
    }
    None
}

/// Find a top-level declaration whose range contains the given line (0-based).
fn find_declaration_by_line(root: Node, line: usize) -> Option<Node> {
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        if !DECLARATION_KINDS.contains(&child.kind()) {
            continue;
        }
        if child.start_position().row <= line && line <= child.end_position().row {
            return Some(child);
        }
    }
    None
}

/// Byte offsets of the start of each line in `source`.
fn line_starts(source: &str) -> Vec<usize> {
    let mut starts = vec![0];
    for (i, b) in source.bytes().enumerate() {
        if b == b'\n' {
            starts.push(i + 1);
        }
    }
    starts
}

/// Expand a declaration node's byte range to include immediately preceding
/// doc comments (contiguous comment lines with no blank-line gap above the node).
/// Returns (start_byte, end_byte) covering comments + declaration + trailing newline.
fn declaration_full_range(node: Node, source: &str) -> (usize, usize) {
    let starts = line_starts(source);
    let decl_line = node.start_position().row;

    let mut first_line = decl_line;
    let mut check = decl_line;
    while check > 0 {
        check -= 1;
        let line_start = starts[check];
        let line_end = starts.get(check + 1).copied().unwrap_or(source.len());
        let line_text = &source[line_start..line_end];
        let trimmed = line_text.trim();
        if trimmed.starts_with('#') {
            first_line = check;
        } else {
            break;
        }
    }

    let start_byte = starts[first_line];

    let mut end_byte = node.end_byte();
    if end_byte < source.len() && source.as_bytes()[end_byte] == b'\n' {
        end_byte += 1;
    }

    (start_byte, end_byte)
}

/// After removing a range, collapse runs of 3+ blank lines down to 2.
fn normalize_blank_lines(source: &mut String) {
    let mut result = String::with_capacity(source.len());
    let mut newline_count = 0;
    for ch in source.chars() {
        if ch == '\n' {
            newline_count += 1;
            if newline_count <= 3 {
                result.push(ch);
            }
        } else {
            newline_count = 0;
            result.push(ch);
        }
    }
    *source = result;
}

// ── Inner class helpers ─────────────────────────────────────────────────────

/// Find a class_definition by name among direct children of `parent`.
fn find_class_definition<'a>(parent: Node<'a>, source: &str, class_name: &str) -> Option<Node<'a>> {
    let mut cursor = parent.walk();
    for child in parent.children(&mut cursor) {
        if child.kind() == "class_definition"
            && let Some(name_node) = child.child_by_field_name("name")
            && name_node.utf8_text(source.as_bytes()).ok() == Some(class_name)
        {
            return Some(child);
        }
    }
    None
}

/// Find the body node of a class_definition.
fn class_body(class_node: Node) -> Option<Node> {
    class_node.child_by_field_name("body")
}

/// Find a declaration by name within a class's body.
fn find_declaration_in_class<'a>(
    class_node: Node<'a>,
    source: &str,
    name: &str,
) -> Option<Node<'a>> {
    let body = class_body(class_node)?;
    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        if !DECLARATION_KINDS.contains(&child.kind()) {
            continue;
        }
        if let Some(decl_name) = get_declaration_name(child, source)
            && decl_name == name
        {
            return Some(child);
        }
    }
    None
}

/// Find a declaration by line within a class's body.
fn find_declaration_in_class_by_line(class_node: Node, line: usize) -> Option<Node> {
    let body = class_body(class_node)?;
    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        if !DECLARATION_KINDS.contains(&child.kind()) {
            continue;
        }
        if child.start_position().row <= line && line <= child.end_position().row {
            return Some(child);
        }
    }
    None
}

/// Re-indent text to a target depth (measured in tabs).
/// Strips the minimum indentation and replaces it with `target_tabs` tabs.
fn re_indent_to_depth(text: &str, target_tabs: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();

    let min_indent = lines
        .iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let trimmed = l.trim_start();
            l.len() - trimmed.len()
        })
        .min()
        .unwrap_or(0);

    let prefix = "\t".repeat(target_tabs);
    lines
        .iter()
        .map(|line| {
            if line.trim().is_empty() {
                String::new()
            } else if line.len() >= min_indent {
                format!("{prefix}{}", &line[min_indent..])
            } else {
                format!("{prefix}{line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

// ── delete-symbol ───────────────────────────────────────────────────────────

pub fn delete_symbol(
    file: &Path,
    name: Option<&str>,
    line: Option<usize>,
    force: bool,
    dry_run: bool,
    project_root: &Path,
    class: Option<&str>,
) -> Result<DeleteSymbolOutput> {
    // Check for enum member syntax: "EnumName.MEMBER"
    if let Some(name) = name
        && let Some((enum_name, member_name)) = name.split_once('.')
    {
        return delete_enum_member(file, enum_name, member_name, force, dry_run, project_root);
    }

    let source =
        std::fs::read_to_string(file).map_err(|e| miette::miette!("cannot read file: {e}"))?;
    let tree = crate::core::parser::parse(&source)?;
    let root = tree.root_node();

    let decl = if let Some(class_name) = class {
        // Look inside an inner class
        let class_node = find_class_definition(root, &source, class_name)
            .ok_or_else(|| miette::miette!("no inner class named '{class_name}' found"))?;
        if let Some(name) = name {
            find_declaration_in_class(class_node, &source, name).ok_or_else(|| {
                miette::miette!("no declaration named '{name}' found in class '{class_name}'")
            })?
        } else if let Some(line) = line {
            find_declaration_in_class_by_line(class_node, line - 1).ok_or_else(|| {
                miette::miette!("no declaration found at line {line} in class '{class_name}'")
            })?
        } else {
            return Err(miette::miette!("either --name or --line is required"));
        }
    } else if let Some(name) = name {
        find_declaration_by_name(root, &source, name)
            .ok_or_else(|| miette::miette!("no declaration named '{name}' found at top level"))?
    } else if let Some(line) = line {
        find_declaration_by_line(root, line - 1)
            .ok_or_else(|| miette::miette!("no declaration found at line {line}"))?
    } else {
        return Err(miette::miette!("either --name or --line is required"));
    };

    let symbol_name = get_declaration_name(decl, &source).unwrap_or_else(|| "unknown".to_string());
    let kind = declaration_kind_str(decl.kind()).to_string();

    let (start_byte, end_byte) = declaration_full_range(decl, &source);

    // Compute 1-based line range for the removed section
    let starts = line_starts(&source);
    let start_line_1 = starts
        .iter()
        .position(|&s| s > start_byte)
        .unwrap_or(starts.len());
    let end_line_1 = starts
        .iter()
        .position(|&s| s >= end_byte)
        .unwrap_or(starts.len());

    // Check for references across the workspace
    let workspace = super::workspace::WorkspaceIndex::new(project_root.to_path_buf());
    let all_refs = super::references::find_references_by_name(&symbol_name, &workspace, None, None);

    // Filter out references within the declaration's own range
    let file_uri = tower_lsp::lsp_types::Url::from_file_path(file).ok();
    let decl_start_line = decl.start_position().row as u32;
    let decl_end_line = decl.end_position().row as u32;

    let external_refs: Vec<_> = all_refs
        .into_iter()
        .filter(|loc| {
            if let Some(ref uri) = file_uri
                && &loc.uri == uri
            {
                let ref_line = loc.range.start.line;
                if ref_line >= decl_start_line && ref_line <= decl_end_line {
                    return false;
                }
            }
            true
        })
        .collect();

    let relative_file = crate::core::fs::relative_slash(file, project_root);

    let ref_outputs: Vec<RefLocation> = external_refs
        .iter()
        .map(|loc| {
            let loc_file = super::query::url_to_relative(&loc.uri, project_root);
            RefLocation {
                file: loc_file,
                line: loc.range.start.line + 1,
                column: loc.range.start.character + 1,
                end_line: loc.range.end.line + 1,
                end_column: loc.range.end.character + 1,
            }
        })
        .collect();

    if !external_refs.is_empty() && !force {
        return Ok(DeleteSymbolOutput {
            symbol: symbol_name,
            kind,
            file: relative_file,
            removed_lines: LineRange {
                start: start_line_1 as u32,
                end: end_line_1 as u32,
            },
            references: ref_outputs,
            applied: false,
        });
    }

    if !dry_run {
        let mut new_source = String::with_capacity(source.len());
        new_source.push_str(&source[..start_byte]);
        new_source.push_str(&source[end_byte..]);
        normalize_blank_lines(&mut new_source);
        std::fs::write(file, &new_source).map_err(|e| miette::miette!("cannot write file: {e}"))?;
    }

    Ok(DeleteSymbolOutput {
        symbol: symbol_name,
        kind,
        file: relative_file,
        removed_lines: LineRange {
            start: start_line_1 as u32,
            end: end_line_1 as u32,
        },
        references: ref_outputs,
        applied: !dry_run,
    })
}

// ── delete-enum-member ──────────────────────────────────────────────────────

fn delete_enum_member(
    file: &Path,
    enum_name: &str,
    member_name: &str,
    force: bool,
    dry_run: bool,
    project_root: &Path,
) -> Result<DeleteSymbolOutput> {
    let source =
        std::fs::read_to_string(file).map_err(|e| miette::miette!("cannot read file: {e}"))?;
    let tree = crate::core::parser::parse(&source)?;
    let root = tree.root_node();

    let enum_node = find_declaration_by_name(root, &source, enum_name)
        .ok_or_else(|| miette::miette!("no enum named '{enum_name}' found"))?;
    if enum_node.kind() != "enum_definition" {
        return Err(miette::miette!("'{enum_name}' is not an enum"));
    }

    // Find enumerator_list (the { ... } body)
    let body = enum_node
        .child_by_field_name("body")
        .ok_or_else(|| miette::miette!("enum has no body"))?;

    // Collect all enumerator children
    let mut enumerators: Vec<tree_sitter::Node> = Vec::new();
    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        if child.kind() == "enumerator" {
            enumerators.push(child);
        }
    }

    // Find the target member
    let (member_idx, _member_node) = enumerators
        .iter()
        .enumerate()
        .find(|(_, e)| {
            if let Some(name_node) = e.child_by_field_name("name") {
                name_node.utf8_text(source.as_bytes()).ok() == Some(member_name)
            } else {
                // Fallback: first named child is the name identifier
                e.named_child(0)
                    .and_then(|n| n.utf8_text(source.as_bytes()).ok())
                    == Some(member_name)
            }
        })
        .ok_or_else(|| miette::miette!("no member '{member_name}' in enum '{enum_name}'"))?;

    if enumerators.len() == 1 {
        return Err(miette::miette!(
            "cannot delete the last member of enum '{enum_name}'"
        ));
    }

    // Compute byte range to remove including comma
    let member_node = enumerators[member_idx];
    let (remove_start, remove_end) =
        compute_enum_member_removal_range(&source, &enumerators, member_idx);

    let relative_file = crate::core::fs::relative_slash(file, project_root);
    let starts = line_starts(&source);
    let start_line_1 = starts
        .iter()
        .position(|&s| s > member_node.start_byte())
        .unwrap_or(starts.len());
    let end_line_1 = start_line_1; // Single member is usually one line

    // Check references
    let workspace = super::workspace::WorkspaceIndex::new(project_root.to_path_buf());
    let all_refs = super::references::find_references_by_name(member_name, &workspace, None, None);

    let file_uri = tower_lsp::lsp_types::Url::from_file_path(file).ok();
    let enum_start = enum_node.start_position().row as u32;
    let enum_end = enum_node.end_position().row as u32;

    let external_refs: Vec<_> = all_refs
        .into_iter()
        .filter(|loc| {
            if let Some(ref uri) = file_uri
                && &loc.uri == uri
            {
                let ref_line = loc.range.start.line;
                if ref_line >= enum_start && ref_line <= enum_end {
                    return false;
                }
            }
            true
        })
        .collect();

    let ref_outputs: Vec<RefLocation> = external_refs
        .iter()
        .map(|loc| {
            let loc_file = super::query::url_to_relative(&loc.uri, project_root);
            RefLocation {
                file: loc_file,
                line: loc.range.start.line + 1,
                column: loc.range.start.character + 1,
                end_line: loc.range.end.line + 1,
                end_column: loc.range.end.character + 1,
            }
        })
        .collect();

    if !external_refs.is_empty() && !force {
        return Ok(DeleteSymbolOutput {
            symbol: format!("{enum_name}.{member_name}"),
            kind: "enum_member".to_string(),
            file: relative_file,
            removed_lines: LineRange {
                start: start_line_1 as u32,
                end: end_line_1 as u32,
            },
            references: ref_outputs,
            applied: false,
        });
    }

    if !dry_run {
        let mut new_source = String::with_capacity(source.len());
        new_source.push_str(&source[..remove_start]);
        new_source.push_str(&source[remove_end..]);
        std::fs::write(file, &new_source).map_err(|e| miette::miette!("cannot write file: {e}"))?;
    }

    Ok(DeleteSymbolOutput {
        symbol: format!("{enum_name}.{member_name}"),
        kind: "enum_member".to_string(),
        file: relative_file,
        removed_lines: LineRange {
            start: start_line_1 as u32,
            end: end_line_1 as u32,
        },
        references: ref_outputs,
        applied: !dry_run,
    })
}

/// Compute byte range to remove for an enum member, including adjacent comma/whitespace.
fn compute_enum_member_removal_range(
    source: &str,
    enumerators: &[tree_sitter::Node],
    idx: usize,
) -> (usize, usize) {
    let member = enumerators[idx];

    if enumerators.len() == 1 {
        // Should not happen (checked above), but be safe
        return (member.start_byte(), member.end_byte());
    }

    if idx == 0 {
        // First member: remove from member start to next member start
        let next = enumerators[1];
        (member.start_byte(), next.start_byte())
    } else {
        // Middle or last: remove from previous member end to this member end
        let prev = enumerators[idx - 1];
        // Find the comma after prev
        let between = &source[prev.end_byte()..member.end_byte()];
        let comma_offset = between.find(',').map(|p| p + 1).unwrap_or(0);
        (prev.end_byte() + comma_offset, member.end_byte())
    }
}

// ── move-symbol ─────────────────────────────────────────────────────────────

pub fn move_symbol(
    name: &str,
    from_file: &Path,
    to_file: &Path,
    dry_run: bool,
    project_root: &Path,
    class: Option<&str>,
    target_class: Option<&str>,
) -> Result<MoveSymbolOutput> {
    let source = std::fs::read_to_string(from_file)
        .map_err(|e| miette::miette!("cannot read source file: {e}"))?;
    let tree = crate::core::parser::parse(&source)?;
    let root = tree.root_node();

    // Find the declaration (possibly within a class)
    let decl = if let Some(class_name) = class {
        let class_node = find_class_definition(root, &source, class_name)
            .ok_or_else(|| miette::miette!("no inner class named '{class_name}' found"))?;
        find_declaration_in_class(class_node, &source, name).ok_or_else(|| {
            miette::miette!("no declaration named '{name}' found in class '{class_name}'")
        })?
    } else {
        find_declaration_by_name(root, &source, name)
            .ok_or_else(|| miette::miette!("no declaration named '{name}' found at top level"))?
    };

    let kind = declaration_kind_str(decl.kind()).to_string();

    let (start_byte, end_byte) = declaration_full_range(decl, &source);
    let decl_text = &source[start_byte..end_byte];
    let decl_text = if decl_text.ends_with('\n') {
        decl_text.to_string()
    } else {
        format!("{decl_text}\n")
    };

    // Re-indent if moving between scope levels
    let decl_text = if class.is_some() && target_class.is_none() {
        // Moving out of a class to top-level: strip one indent level
        let re = re_indent_to_depth(&decl_text, 0);
        if re.ends_with('\n') {
            re
        } else {
            format!("{re}\n")
        }
    } else if class.is_none() && target_class.is_some() {
        // Moving from top-level into a class: add one indent level
        let re = re_indent_to_depth(&decl_text, 1);
        if re.ends_with('\n') {
            re
        } else {
            format!("{re}\n")
        }
    } else {
        decl_text
    };

    // Check target for duplicate
    if to_file.exists() {
        let target_source = std::fs::read_to_string(to_file)
            .map_err(|e| miette::miette!("cannot read target file: {e}"))?;
        let target_tree = crate::core::parser::parse(&target_source)?;
        let target_root = target_tree.root_node();

        let dup = if let Some(tc) = target_class {
            find_class_definition(target_root, &target_source, tc)
                .and_then(|c| find_declaration_in_class(c, &target_source, name))
        } else {
            find_declaration_by_name(target_root, &target_source, name)
        };
        if dup.is_some() {
            return Err(miette::miette!(
                "target already contains a declaration named '{name}'"
            ));
        }
    }

    // Find references for warnings
    let workspace = super::workspace::WorkspaceIndex::new(project_root.to_path_buf());
    let class_filter = class;
    let all_refs = super::references::find_references_by_name(name, &workspace, None, class_filter);

    let file_uri = tower_lsp::lsp_types::Url::from_file_path(from_file).ok();
    let decl_start_line = decl.start_position().row as u32;
    let decl_end_line = decl.end_position().row as u32;

    let external_count = all_refs
        .iter()
        .filter(|loc| {
            if let Some(ref uri) = file_uri
                && &loc.uri == uri
            {
                let ref_line = loc.range.start.line;
                if ref_line >= decl_start_line && ref_line <= decl_end_line {
                    return false;
                }
            }
            true
        })
        .count();

    let mut warnings = Vec::new();
    if external_count > 0 {
        warnings.push(format!(
            "{external_count} reference{} to '{name}' may need updating",
            if external_count == 1 { "" } else { "s" }
        ));
    }

    // Self-reference warnings when moving between classes (Feature 8)
    if target_class.is_some() || class.is_some() {
        let self_refs = collect_self_references(decl, &source);
        if !self_refs.is_empty() && to_file.exists() {
            let target_source = std::fs::read_to_string(to_file)
                .map_err(|e| miette::miette!("cannot read target file: {e}"))?;
            let target_tree = crate::core::parser::parse(&target_source)?;
            let target_root = target_tree.root_node();

            let target_scope = if let Some(tc) = target_class {
                find_class_definition(target_root, &target_source, tc)
            } else {
                Some(target_root)
            };

            if let Some(scope) = target_scope {
                for member in &self_refs {
                    if !class_has_member(scope, &target_source, member) {
                        warnings.push(format!(
                            "self.{member} referenced but '{member}' not found in target"
                        ));
                    }
                }
            }
        }
    }

    let from_relative = crate::core::fs::relative_slash(from_file, project_root);
    let to_relative = crate::core::fs::relative_slash(to_file, project_root);

    if !dry_run {
        // Write target file
        if to_file.exists() {
            let mut target_source = std::fs::read_to_string(to_file)
                .map_err(|e| miette::miette!("cannot read target file: {e}"))?;

            if let Some(tc) = target_class {
                // Insert into target class body
                let target_tree = crate::core::parser::parse(&target_source)?;
                let target_root = target_tree.root_node();
                let tc_node =
                    find_class_definition(target_root, &target_source, tc).ok_or_else(|| {
                        miette::miette!("target class '{tc}' not found in target file")
                    })?;
                let insert_byte = tc_node.end_byte();
                // Insert before end of class with proper spacing
                let spacing = "\n";
                let insert_text = format!("{spacing}{decl_text}");
                target_source.insert_str(insert_byte, &insert_text);
            } else {
                let spacing = insertion_spacing(decl.kind(), &target_source);
                target_source.push_str(&spacing);
                target_source.push_str(&decl_text);
            }
            std::fs::write(to_file, &target_source)
                .map_err(|e| miette::miette!("cannot write target file: {e}"))?;
        } else {
            std::fs::write(to_file, &decl_text)
                .map_err(|e| miette::miette!("cannot write target file: {e}"))?;
        }

        // Remove from source file
        let mut new_source = String::with_capacity(source.len());
        new_source.push_str(&source[..start_byte]);
        new_source.push_str(&source[end_byte..]);
        normalize_blank_lines(&mut new_source);
        std::fs::write(from_file, &new_source)
            .map_err(|e| miette::miette!("cannot write source file: {e}"))?;
    }

    // Detect preload/load references to the source file (Feature 1)
    let from_res = format!("res://{from_relative}");
    let preloads = find_preloads_to_file(&from_res, &workspace, project_root);

    Ok(MoveSymbolOutput {
        symbol: name.to_string(),
        kind,
        from: from_relative,
        to: to_relative,
        applied: !dry_run,
        warnings,
        preloads,
    })
}

/// Determine blank-line spacing to add before inserting a declaration into an existing file.
fn insertion_spacing(decl_kind: &str, target_source: &str) -> String {
    let trimmed = target_source.trim_end();
    if trimmed.is_empty() {
        return String::new();
    }

    // Functions and classes get 2 blank lines before them
    let needs_extra = matches!(
        decl_kind,
        "function_definition" | "constructor_definition" | "class_definition"
    );

    if needs_extra {
        // Ensure the target ends with enough newlines for 2 blank lines
        let trailing_newlines = target_source.len() - trimmed.len();
        if trailing_newlines >= 3 {
            String::new()
        } else {
            "\n".repeat(3 - trailing_newlines)
        }
    } else {
        // Variables, constants, signals: 1 blank line
        let trailing_newlines = target_source.len() - trimmed.len();
        if trailing_newlines >= 2 {
            String::new()
        } else {
            "\n".repeat(2 - trailing_newlines)
        }
    }
}

// ── Self-reference analysis (Feature 8) ─────────────────────────────────────

/// Collect all `self.member` references in a node subtree.
fn collect_self_references(node: Node, source: &str) -> Vec<String> {
    let mut members = Vec::new();
    collect_self_refs_recursive(node, source, &mut members);
    members.sort();
    members.dedup();
    members
}

fn collect_self_refs_recursive(node: Node, source: &str, members: &mut Vec<String>) {
    // `self.foo` is an `attribute` node: child(0)=self, child(1)=".", child(2)=foo
    // or with attribute_call: child(0)=self, child(1)=".", child(2)=attribute_call
    if node.kind() == "attribute"
        && let Some(obj) = node.child(0)
        && obj.kind() == "identifier"
        && obj.utf8_text(source.as_bytes()).ok() == Some("self")
    {
        // The member is child(2) for property access, or named_child(1) as fallback
        if let Some(member) = node.child(2) {
            let name_text = if member.kind() == "attribute_call" {
                // self.method() → attribute_call's first named child is the name
                member
                    .named_child(0)
                    .and_then(|n| n.utf8_text(source.as_bytes()).ok())
            } else {
                member.utf8_text(source.as_bytes()).ok()
            };
            if let Some(name) = name_text {
                members.push(name.to_string());
            }
        }
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_self_refs_recursive(child, source, members);
    }
}

/// Check if a scope (class body or root) declares a member with the given name.
fn class_has_member(scope: Node, source: &str, name: &str) -> bool {
    let search_node = if scope.kind() == "class_definition" {
        scope.child_by_field_name("body").unwrap_or(scope)
    } else {
        scope
    };
    let mut cursor = search_node.walk();
    for child in search_node.children(&mut cursor) {
        if DECLARATION_KINDS.contains(&child.kind())
            && let Some(decl_name) = get_declaration_name(child, source)
            && decl_name == name
        {
            return true;
        }
    }
    false
}

// ── Preload path detection (Feature 1) ──────────────────────────────────────

#[derive(Serialize)]
pub struct PreloadRef {
    pub file: String,
    pub line: u32,
    pub path: String,
}

/// Find all preload()/load() references to a given `res://` path across the workspace.
pub fn find_preloads_to_file(
    res_path: &str,
    workspace: &super::workspace::WorkspaceIndex,
    project_root: &Path,
) -> Vec<PreloadRef> {
    let mut refs = Vec::new();
    for (path, content) in workspace.all_files() {
        if let Ok(tree) = crate::core::parser::parse(&content) {
            find_preloads_in_tree(
                tree.root_node(),
                &content,
                res_path,
                &crate::core::fs::relative_slash(&path, project_root),
                &mut refs,
            );
        }
    }
    refs
}

fn find_preloads_in_tree(
    node: Node,
    source: &str,
    target_path: &str,
    file: &str,
    refs: &mut Vec<PreloadRef>,
) {
    if node.kind() == "call" {
        let func_name = node
            .child_by_field_name("function")
            .or_else(|| node.named_child(0));
        if let Some(func) = func_name
            && let Ok(name) = func.utf8_text(source.as_bytes())
            && (name == "preload" || name == "load")
            && let Some(args) = node.child_by_field_name("arguments")
        {
            // Find string argument
            let mut arg_cursor = args.walk();
            for arg in args.children(&mut arg_cursor) {
                if arg.kind() == "string"
                    && let Ok(text) = arg.utf8_text(source.as_bytes())
                {
                    let unquoted = text.trim_matches('"').trim_matches('\'');
                    if unquoted == target_path {
                        refs.push(PreloadRef {
                            file: file.to_string(),
                            line: node.start_position().row as u32 + 1,
                            path: unquoted.to_string(),
                        });
                    }
                }
            }
        }
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        find_preloads_in_tree(child, source, target_path, file, refs);
    }
}

// ── extract-method ──────────────────────────────────────────────────────────

#[derive(Clone)]
struct CapturedVar {
    name: String,
    type_hint: Option<String>,
    is_written: bool,
    is_used_after: bool,
}

pub fn extract_method(
    file: &Path,
    start_line: usize, // 1-based inclusive
    end_line: usize,   // 1-based inclusive
    name: &str,
    dry_run: bool,
    project_root: &Path,
) -> Result<ExtractMethodOutput> {
    let source =
        std::fs::read_to_string(file).map_err(|e| miette::miette!("cannot read file: {e}"))?;
    let tree = crate::core::parser::parse(&source)?;
    let root = tree.root_node();

    let start_line_0 = start_line - 1;
    let end_line_0 = end_line - 1;

    // Find enclosing function
    let point = tree_sitter::Point::new(start_line_0, 0);
    let func = super::references::enclosing_function(root, point)
        .ok_or_else(|| miette::miette!("no enclosing function at line {start_line}"))?;

    let body = func
        .child_by_field_name("body")
        .ok_or_else(|| miette::miette!("function has no body"))?;

    // Verify entire range is within the function body
    if start_line_0 < body.start_position().row || end_line_0 > body.end_position().row {
        return Err(miette::miette!(
            "selected range is outside the function body"
        ));
    }

    // Collect statements in the range
    let statements = collect_statements_in_range(body, start_line_0, end_line_0)?;
    if statements.is_empty() {
        return Err(miette::miette!("no statements found in the selected range"));
    }

    // Verify statement boundaries
    let first_start = statements[0].start_position().row;
    let last_end = statements.last().unwrap().end_position().row;
    if first_start != start_line_0 {
        return Err(miette::miette!(
            "start line {start_line} does not align with a statement boundary (nearest: {})",
            first_start + 1
        ));
    }
    if last_end != end_line_0 {
        return Err(miette::miette!(
            "end line {end_line} does not align with a statement boundary (nearest: {})",
            last_end + 1
        ));
    }

    // Check for return statements
    for stmt in &statements {
        if contains_node_kind(*stmt, "return_statement") {
            return Err(miette::miette!(
                "cannot extract code containing return statements"
            ));
        }
    }

    // Async detection: warn if extracted code contains await
    let mut warnings = Vec::new();
    for stmt in &statements {
        if contains_node_kind(*stmt, "await_expression") {
            warnings.push(
                "extracted code contains 'await' — the caller may need adjustment".to_string(),
            );
            break;
        }
    }

    // Variable capture analysis
    let range_idents = collect_identifiers(&statements, &source);
    let local_decls = collect_local_declarations(&statements, &source);

    let extracted_range = (
        statements[0].start_byte(),
        statements.last().unwrap().end_byte(),
    );

    let captured = find_captured_variables(
        &func,
        body,
        &source,
        &range_idents,
        &local_decls,
        &statements,
        extracted_range,
    );

    // Separate params and return vars
    let mut return_vars: Vec<CapturedVar> = Vec::new();
    for cap in &captured {
        if cap.is_written && cap.is_used_after {
            return_vars.push(cap.clone());
        }
    }

    // All captured vars are parameters
    let params: Vec<&CapturedVar> = captured.iter().collect();

    // Generate the new function and call site based on return count
    let (func_text, func_signature, call_site_line, returns_field, return_vars_field);
    let original_indent = get_indent(&source, start_line_0);

    if return_vars.len() >= 2 {
        // Multiple return values: use Dictionary
        let (ft, fs) = generate_extracted_function_multi_return(
            name,
            &params,
            &return_vars,
            &statements,
            &source,
        );
        let result_name = pick_result_name(&source, body);
        let cl = generate_call_site_multi_return(
            name,
            &params,
            &return_vars,
            &original_indent,
            &result_name,
        );
        func_text = ft;
        func_signature = fs;
        call_site_line = cl;
        returns_field = None;
        return_vars_field = return_vars.iter().map(|v| v.name.clone()).collect();
    } else {
        let return_var = return_vars.into_iter().next();
        let (ft, fs) =
            generate_extracted_function(name, &params, return_var.as_ref(), &statements, &source);
        let cl = generate_call_site(name, &params, return_var.as_ref(), &original_indent);
        func_text = ft;
        func_signature = fs;
        call_site_line = cl;
        returns_field = return_var.map(|v| v.name);
        return_vars_field = Vec::new();
    };

    let relative_file = crate::core::fs::relative_slash(file, project_root);

    if !dry_run {
        let starts = line_starts(&source);
        let mut new_source = source.clone();

        // 1. Replace extracted range with call site (higher byte offset first)
        let replace_start = starts[start_line_0];
        let replace_end = if end_line_0 + 1 < starts.len() {
            starts[end_line_0 + 1]
        } else {
            source.len()
        };
        new_source.replace_range(replace_start..replace_end, &call_site_line);

        // 2. Insert new function before the enclosing function
        // Re-compute line_starts after the first edit
        let new_starts = line_starts(&new_source);
        let func_start_line = func.start_position().row;
        // After our replacement, the enclosing function may have shifted.
        // Use the original func start line to find the insertion point.
        // The replacement was inside the function, so lines before the function are unchanged.
        let insert_byte = new_starts[func_start_line];
        let insert_text = format!("{func_text}\n\n\n");
        new_source.insert_str(insert_byte, &insert_text);

        normalize_blank_lines(&mut new_source);
        std::fs::write(file, &new_source).map_err(|e| miette::miette!("cannot write file: {e}"))?;
    }

    let param_outputs: Vec<ParameterOutput> = params
        .iter()
        .map(|p| ParameterOutput {
            name: p.name.clone(),
            type_hint: p.type_hint.clone(),
        })
        .collect();

    Ok(ExtractMethodOutput {
        function: func_signature,
        parameters: param_outputs,
        returns: returns_field,
        return_vars: return_vars_field,
        call_site: call_site_line.trim_end_matches('\n').to_string(),
        file: relative_file,
        applied: !dry_run,
        warnings,
    })
}

/// Collect direct children of `body` that are fully within [start_line, end_line] (0-based).
fn collect_statements_in_range<'a>(
    body: Node<'a>,
    start_line: usize,
    end_line: usize,
) -> Result<Vec<Node<'a>>> {
    let mut statements = Vec::new();
    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        if child.kind() == "comment" {
            continue;
        }
        let node_start = child.start_position().row;
        let node_end = child.end_position().row;

        if node_end < start_line || node_start > end_line {
            continue; // Outside range
        }

        // Check partial overlap
        if node_start < start_line || node_end > end_line {
            return Err(miette::miette!(
                "line range {}-{} does not align with statement boundaries \
                 (statement on lines {}-{} partially overlaps)",
                start_line + 1,
                end_line + 1,
                node_start + 1,
                node_end + 1
            ));
        }

        statements.push(child);
    }
    Ok(statements)
}

/// Check if any descendant has the given node kind.
fn contains_node_kind(node: Node, kind: &str) -> bool {
    if node.kind() == kind {
        return true;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if contains_node_kind(child, kind) {
            return true;
        }
    }
    false
}

/// Collect unique identifier names used in the given statement nodes.
fn collect_identifiers(statements: &[Node], source: &str) -> HashSet<String> {
    let mut names = HashSet::new();
    for stmt in statements {
        collect_idents_recursive(*stmt, source, &mut names);
    }
    names
}

fn collect_idents_recursive(node: Node, source: &str, names: &mut HashSet<String>) {
    if (node.kind() == "identifier" || node.kind() == "name")
        && let Ok(text) = node.utf8_text(source.as_bytes())
        && !text.is_empty()
    {
        names.insert(text.to_string());
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_idents_recursive(child, source, names);
    }
}

/// Collect names declared within the extracted statements (var/for declarations).
fn collect_local_declarations(statements: &[Node], source: &str) -> HashSet<String> {
    let mut decls = HashSet::new();
    for stmt in statements {
        collect_decls_recursive(*stmt, source, &mut decls);
    }
    decls
}

fn collect_decls_recursive(node: Node, source: &str, decls: &mut HashSet<String>) {
    if node.kind() == "variable_statement"
        && let Some(name_node) = node.child_by_field_name("name")
        && let Ok(text) = name_node.utf8_text(source.as_bytes())
    {
        decls.insert(text.to_string());
    }
    if node.kind() == "for_statement"
        && let Some(left) = node.child_by_field_name("left")
        && let Ok(text) = left.utf8_text(source.as_bytes())
    {
        decls.insert(text.to_string());
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_decls_recursive(child, source, decls);
    }
}

/// Find captured variables: identifiers in the range that are declared before the range
/// in the enclosing function (as parameters or local vars).
fn find_captured_variables(
    func: &Node,
    body: Node,
    source: &str,
    range_idents: &HashSet<String>,
    local_decls: &HashSet<String>,
    statements: &[Node],
    extracted_range: (usize, usize),
) -> Vec<CapturedVar> {
    // Collect function parameters with optional type hints
    let mut pre_decls: HashMap<String, Option<String>> = HashMap::new();

    if let Some(params) = func.child_by_field_name("parameters") {
        let mut cursor = params.walk();
        if cursor.goto_first_child() {
            loop {
                let child = cursor.node();
                match child.kind() {
                    "identifier" => {
                        let name = source[child.byte_range()].to_string();
                        pre_decls.insert(name, None);
                    }
                    "typed_parameter" | "typed_default_parameter" => {
                        if let Some(name_node) = child.child(0)
                            && name_node.kind() == "identifier"
                        {
                            let name = source[name_node.byte_range()].to_string();
                            let type_hint = child
                                .child_by_field_name("type")
                                .and_then(|t| t.utf8_text(source.as_bytes()).ok())
                                .map(|s| s.to_string());
                            pre_decls.insert(name, type_hint);
                        }
                    }
                    "default_parameter" => {
                        if let Some(name_node) = child.child(0)
                            && name_node.kind() == "identifier"
                        {
                            let name = source[name_node.byte_range()].to_string();
                            pre_decls.insert(name, None);
                        }
                    }
                    _ => {}
                }
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
        }
    }

    // Collect local vars declared before the range in the body
    let mut body_cursor = body.walk();
    for child in body.children(&mut body_cursor) {
        if child.start_byte() >= extracted_range.0 {
            break;
        }
        if child.kind() == "variable_statement"
            && let Some(name_node) = child.child_by_field_name("name")
            && let Ok(var_name) = name_node.utf8_text(source.as_bytes())
        {
            let type_hint = child
                .child_by_field_name("type")
                .and_then(|t| t.utf8_text(source.as_bytes()).ok())
                .map(|s| s.to_string());
            pre_decls.insert(var_name.to_string(), type_hint);
        }
    }

    // Filter to identifiers actually used in the range (and not locally declared there)
    let mut captured = Vec::new();
    for (var_name, type_hint) in &pre_decls {
        if range_idents.contains(var_name) && !local_decls.contains(var_name) {
            let is_written = is_assigned_in_statements(var_name, statements, source);
            let is_used_after = is_used_after_range(var_name, body, source, extracted_range.1);
            captured.push(CapturedVar {
                name: var_name.clone(),
                type_hint: type_hint.clone(),
                is_written,
                is_used_after,
            });
        }
    }

    // Sort for deterministic output
    captured.sort_by(|a, b| a.name.cmp(&b.name));
    captured
}

/// Check if `name` is on the left side of an assignment within the statements.
fn is_assigned_in_statements(name: &str, statements: &[Node], source: &str) -> bool {
    for stmt in statements {
        if has_assignment_to(*stmt, name, source) {
            return true;
        }
    }
    false
}

fn has_assignment_to(node: Node, name: &str, source: &str) -> bool {
    if matches!(node.kind(), "assignment" | "augmented_assignment")
        && let Some(left) = node.child_by_field_name("left")
        && (left.kind() == "identifier" || left.kind() == "name")
        && left.utf8_text(source.as_bytes()).ok() == Some(name)
    {
        return true;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if has_assignment_to(child, name, source) {
            return true;
        }
    }
    false
}

/// Check if `name` is used in the body after `range_end` byte offset.
fn is_used_after_range(name: &str, body: Node, source: &str, range_end: usize) -> bool {
    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        if child.start_byte() >= range_end && has_identifier(child, name, source) {
            return true;
        }
    }
    false
}

fn has_identifier(node: Node, name: &str, source: &str) -> bool {
    if (node.kind() == "identifier" || node.kind() == "name")
        && node.utf8_text(source.as_bytes()).ok() == Some(name)
    {
        return true;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if has_identifier(child, name, source) {
            return true;
        }
    }
    false
}

/// Generate the extracted function text and its signature string.
fn generate_extracted_function(
    name: &str,
    params: &[&CapturedVar],
    return_var: Option<&CapturedVar>,
    statements: &[Node],
    source: &str,
) -> (String, String) {
    // Build parameter list
    let param_str = params
        .iter()
        .map(|p| {
            if let Some(ref t) = p.type_hint {
                format!("{}: {}", p.name, t)
            } else {
                p.name.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(", ");

    // Build return type
    let return_type = return_var
        .and_then(|v| v.type_hint.as_ref())
        .map(|t| format!(" -> {t}"))
        .unwrap_or_default();

    let signature = format!("func {name}({param_str}){return_type}:");

    // Extract body text from the statements
    let first_byte = statements[0].start_byte();
    let last_byte = statements.last().unwrap().end_byte();
    let body_text = &source[first_byte..last_byte];

    // Re-indent to 1 level
    let re_indented = re_indent(body_text);

    // Add return statement if needed
    let mut func_body = re_indented;
    if let Some(ret) = return_var {
        func_body.push_str(&format!("\n\treturn {}", ret.name));
    }

    let func_text = format!("{signature}\n{func_body}");
    (func_text, signature)
}

/// Re-indent text: find minimum indentation, strip it, add 1 tab.
fn re_indent(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();

    let min_indent = lines
        .iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.len() - l.trim_start().len())
        .min()
        .unwrap_or(0);

    lines
        .iter()
        .map(|line| {
            if line.trim().is_empty() {
                String::new()
            } else if line.len() >= min_indent {
                format!("\t{}", &line[min_indent..])
            } else {
                format!("\t{line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Generate the call site text (with indentation, trailing newline).
fn generate_call_site(
    name: &str,
    params: &[&CapturedVar],
    return_var: Option<&CapturedVar>,
    indent: &str,
) -> String {
    let args = params
        .iter()
        .map(|p| p.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");

    if let Some(ret) = return_var {
        format!("{indent}{} = {name}({args})\n", ret.name)
    } else {
        format!("{indent}{name}({args})\n")
    }
}

/// Get the indentation string (tabs/spaces) of a line (0-based).
fn get_indent(source: &str, line: usize) -> String {
    let line_text = source.lines().nth(line).unwrap_or("");
    let indent_len = line_text.len() - line_text.trim_start().len();
    line_text[..indent_len].to_string()
}

/// Generate an extracted function that returns a Dictionary for multiple return values.
fn generate_extracted_function_multi_return(
    name: &str,
    params: &[&CapturedVar],
    return_vars: &[CapturedVar],
    statements: &[Node],
    source: &str,
) -> (String, String) {
    let param_str = params
        .iter()
        .map(|p| {
            if let Some(ref t) = p.type_hint {
                format!("{}: {}", p.name, t)
            } else {
                p.name.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(", ");

    let signature = format!("func {name}({param_str}) -> Dictionary:");

    let first_byte = statements[0].start_byte();
    let last_byte = statements.last().unwrap().end_byte();
    let body_text = &source[first_byte..last_byte];
    let re_indented = re_indent(body_text);

    let dict_entries = return_vars
        .iter()
        .map(|v| format!("\"{}\": {}", v.name, v.name))
        .collect::<Vec<_>>()
        .join(", ");
    let return_line = format!("\n\treturn {{{dict_entries}}}");

    let func_text = format!("{signature}\n{re_indented}{return_line}");
    (func_text, signature)
}

/// Generate a call site for a multi-return extraction (Dictionary destructuring).
fn generate_call_site_multi_return(
    name: &str,
    params: &[&CapturedVar],
    return_vars: &[CapturedVar],
    indent: &str,
    result_name: &str,
) -> String {
    let args = params
        .iter()
        .map(|p| p.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");

    let mut lines = format!("{indent}var {result_name} = {name}({args})\n");
    for v in return_vars {
        lines.push_str(&format!(
            "{indent}{} = {result_name}[\"{}\"]\n",
            v.name, v.name
        ));
    }
    lines
}

/// Pick a unique name for the result variable that doesn't collide with identifiers in the
/// enclosing function body.
fn pick_result_name(source: &str, body: Node) -> String {
    let mut idents = HashSet::new();
    collect_idents_recursive(body, source, &mut idents);
    let mut name = "_result".to_string();
    let mut suffix = 2;
    while idents.contains(&name) {
        name = format!("_result{suffix}");
        suffix += 1;
    }
    name
}

// ── inline-method ───────────────────────────────────────────────────────────

#[derive(Serialize, Debug)]
pub struct InlineMethodOutput {
    pub function: String,
    pub call_site_file: String,
    pub call_site_line: u32,
    pub inlined_lines: u32,
    pub function_deleted: bool,
    pub applied: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

pub fn inline_method(
    file: &Path,
    line: usize,   // 1-based
    column: usize, // 1-based
    dry_run: bool,
    project_root: &Path,
) -> Result<InlineMethodOutput> {
    let source =
        std::fs::read_to_string(file).map_err(|e| miette::miette!("cannot read file: {e}"))?;
    let tree = crate::core::parser::parse(&source)?;
    let root = tree.root_node();

    let point = tree_sitter::Point::new(line - 1, column - 1);

    // Find call node at cursor
    let call_node = find_call_at(root, point)
        .ok_or_else(|| miette::miette!("no function call found at {line}:{column}"))?;

    // Get function name from call
    let func_name_node = call_node
        .child_by_field_name("function")
        .or_else(|| call_node.named_child(0))
        .ok_or_else(|| miette::miette!("cannot determine function name from call"))?;
    let func_name = func_name_node
        .utf8_text(source.as_bytes())
        .map_err(|e| miette::miette!("cannot read function name: {e}"))?;

    // Don't inline attribute calls (e.g., obj.method()) — only simple function calls
    if func_name_node.kind() == "attribute" {
        return Err(miette::miette!(
            "cannot inline method calls (obj.method()) — only standalone function calls"
        ));
    }

    // Find function definition in the same file
    let func_def = find_declaration_by_name(root, &source, func_name)
        .ok_or_else(|| miette::miette!("cannot find definition of '{func_name}' in this file"))?;
    if !matches!(
        func_def.kind(),
        "function_definition" | "constructor_definition"
    ) {
        return Err(miette::miette!("'{func_name}' is not a function"));
    }

    let func_body = func_def
        .child_by_field_name("body")
        .ok_or_else(|| miette::miette!("function has no body"))?;

    // Collect body statements (skip comments)
    let body_stmts: Vec<tree_sitter::Node> = {
        let mut c = func_body.walk();
        func_body
            .children(&mut c)
            .filter(|n| n.kind() != "comment")
            .collect()
    };

    if body_stmts.is_empty() {
        return Err(miette::miette!("function body is empty"));
    }

    // Check for multiple return statements recursively (only allow single trailing return)
    let mut return_count = 0;
    for stmt in &body_stmts {
        count_return_statements(*stmt, &mut return_count);
    }
    if return_count > 1 {
        return Err(miette::miette!(
            "cannot inline function with multiple return statements"
        ));
    }
    // If there's a return, it must be the last top-level statement
    if return_count == 1 && body_stmts.last().map(|s| s.kind()) != Some("return_statement") {
        return Err(miette::miette!(
            "cannot inline function with non-trailing return statement"
        ));
    }

    // Check for recursion
    for stmt in &body_stmts {
        if contains_call_to(*stmt, func_name, &source) {
            return Err(miette::miette!(
                "cannot inline recursive function '{func_name}'"
            ));
        }
    }

    // Parse call arguments
    let call_args = extract_call_arguments(call_node, &source);

    // Parse function parameters
    let func_params = extract_function_params(func_def, &source);

    // Build parameter → argument mapping
    let mut param_map: HashMap<String, String> = HashMap::new();
    for (i, param) in func_params.iter().enumerate() {
        let arg = call_args
            .get(i)
            .map(|s| s.as_str())
            .or(param.default.as_deref())
            .unwrap_or(&param.name);
        param_map.insert(param.name.clone(), arg.to_string());
    }

    // Extract body text and do parameter substitution
    let body_start = body_stmts[0].start_byte();
    let body_end = body_stmts.last().unwrap().end_byte();
    let body_text = &source[body_start..body_end];

    let substituted = substitute_params(body_text, &param_map, &body_stmts, body_start, &source);

    // Handle return value
    let has_return = return_count == 1;
    let (inlined_text, return_expr) = if has_return {
        let last_stmt = body_stmts.last().unwrap();
        // Extract the return expression
        let ret_expr_text = last_stmt
            .named_child(0)
            .map(|n| {
                let rel_start = n.start_byte() - body_start;
                let rel_end = n.end_byte() - body_start;
                substituted[rel_start..rel_end].to_string()
            })
            .unwrap_or_default();

        if body_stmts.len() == 1 {
            // Single return statement — just use the expression
            (String::new(), Some(ret_expr_text))
        } else {
            // Multiple statements + trailing return
            let non_return_end = body_stmts[body_stmts.len() - 2].end_byte() - body_start;
            let prefix = &substituted[..non_return_end];
            (prefix.to_string(), Some(ret_expr_text))
        }
    } else {
        (substituted.clone(), None)
    };

    // Get call site context
    let call_line = call_node.start_position().row;
    let call_indent = get_indent(&source, call_line);

    // Build the inlined code
    let mut inlined_lines_text = String::new();

    // Check if call is part of an assignment (var x = func() or x = func())
    let call_parent = call_node.parent();
    let is_assignment = call_parent
        .map(|p| {
            matches!(
                p.kind(),
                "assignment" | "augmented_assignment" | "variable_statement"
            )
        })
        .unwrap_or(false);

    if !inlined_text.is_empty() {
        // Re-indent non-return body statements
        let re_indented = re_indent_to_depth_with_indent(&inlined_text, &call_indent);
        inlined_lines_text.push_str(&re_indented);
        if !inlined_lines_text.ends_with('\n') {
            inlined_lines_text.push('\n');
        }
    }

    if let Some(ref ret_expr) = return_expr {
        if is_assignment {
            // Replace call in assignment with return expression
            if let Some(parent) = call_parent {
                if parent.kind() == "variable_statement" {
                    // var x = func() → var x = expr + body before
                    let var_name = parent
                        .child_by_field_name("name")
                        .and_then(|n| n.utf8_text(source.as_bytes()).ok())
                        .unwrap_or("x");
                    inlined_lines_text
                        .push_str(&format!("{call_indent}var {var_name} = {ret_expr}\n"));
                } else {
                    // x = func() → body + x = expr
                    let left = parent
                        .child_by_field_name("left")
                        .and_then(|n| n.utf8_text(source.as_bytes()).ok())
                        .unwrap_or("x");
                    inlined_lines_text.push_str(&format!("{call_indent}{left} = {ret_expr}\n"));
                }
            }
        } else {
            // Standalone call with return → just add the expression (discard return value)
            if !inlined_text.is_empty() {
                // Body already added above; the return value is discarded
            } else {
                inlined_lines_text.push_str(&format!("{call_indent}{ret_expr}\n"));
            }
        }
    } else if inlined_text.is_empty() {
        // Void function, single `pass` → remove the call line entirely
        inlined_lines_text.push_str(&format!("{call_indent}pass\n"));
    }

    let total_inlined = inlined_lines_text.lines().count() as u32;

    // Count references to decide if we can delete the function
    let workspace = super::workspace::WorkspaceIndex::new(project_root.to_path_buf());
    let all_refs = super::references::find_references_by_name(func_name, &workspace, None, None);
    let func_def_start = func_def.start_position().row as u32;
    let func_def_end = func_def.end_position().row as u32;
    let file_uri = tower_lsp::lsp_types::Url::from_file_path(file).ok();
    let call_count = all_refs
        .iter()
        .filter(|loc| {
            if let Some(ref uri) = file_uri
                && &loc.uri == uri
            {
                let ref_line = loc.range.start.line;
                // Don't count references within the function definition itself
                if ref_line >= func_def_start && ref_line <= func_def_end {
                    return false;
                }
            }
            true
        })
        .count();

    let can_delete = call_count <= 1;

    let mut warnings = Vec::new();
    if !can_delete {
        warnings.push(format!(
            "function '{func_name}' has {call_count} call sites — not deleted"
        ));
    }

    // Check for self. usage
    let self_refs = collect_self_references(func_def, &source);
    if !self_refs.is_empty() {
        warnings.push("inlined code contains 'self.' references".to_string());
    }

    let relative_file = crate::core::fs::relative_slash(file, project_root);

    if !dry_run {
        let starts = line_starts(&source);
        let mut new_source = source.clone();

        // 1. Replace the call site line(s)
        let call_stmt_node = if is_assignment {
            call_parent.unwrap_or(call_node)
        } else {
            call_node
        };
        let stmt_start_line = call_stmt_node.start_position().row;
        let stmt_end_line = call_stmt_node.end_position().row;

        let replace_start = starts[stmt_start_line];
        let replace_end = if stmt_end_line + 1 < starts.len() {
            starts[stmt_end_line + 1]
        } else {
            source.len()
        };

        new_source.replace_range(replace_start..replace_end, &inlined_lines_text);

        // 2. Delete function definition if single callsite
        if can_delete {
            let new_tree = crate::core::parser::parse(&new_source)?;
            let new_root = new_tree.root_node();
            if let Some(def) = find_declaration_by_name(new_root, &new_source, func_name) {
                let (def_start, def_end) = declaration_full_range(def, &new_source);
                let mut final_source = String::with_capacity(new_source.len());
                final_source.push_str(&new_source[..def_start]);
                final_source.push_str(&new_source[def_end..]);
                normalize_blank_lines(&mut final_source);
                new_source = final_source;
            }
        }

        normalize_blank_lines(&mut new_source);
        std::fs::write(file, &new_source).map_err(|e| miette::miette!("cannot write file: {e}"))?;
    }

    Ok(InlineMethodOutput {
        function: func_name.to_string(),
        call_site_file: relative_file,
        call_site_line: line as u32,
        inlined_lines: total_inlined,
        function_deleted: can_delete && !dry_run,
        applied: !dry_run,
        warnings,
    })
}

// ── inline-method by name ────────────────────────────────────────────────────

#[derive(Serialize, Debug)]
pub struct InlineMethodByNameOutput {
    pub function: String,
    pub file: String,
    pub call_sites_inlined: u32,
    pub function_deleted: bool,
    pub applied: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

/// Inline all (or list) call sites of a function by name within a file.
/// With `all=true`, inlines every call site and deletes the function.
/// With `all=false`, reports call sites in dry-run style.
pub fn inline_method_by_name(
    file: &Path,
    name: &str,
    all: bool,
    dry_run: bool,
    project_root: &Path,
) -> Result<InlineMethodByNameOutput> {
    let source =
        std::fs::read_to_string(file).map_err(|e| miette::miette!("cannot read file: {e}"))?;
    let tree = crate::core::parser::parse(&source)?;
    let root = tree.root_node();

    // Find function definition
    let func_def = find_declaration_by_name(root, &source, name)
        .ok_or_else(|| miette::miette!("no function named '{name}' found in this file"))?;
    if !matches!(
        func_def.kind(),
        "function_definition" | "constructor_definition"
    ) {
        return Err(miette::miette!("'{name}' is not a function"));
    }

    let func_def_start = func_def.start_position().row;
    let func_def_end = func_def.end_position().row;

    // Find all call sites of this function in the file
    let mut call_sites: Vec<(usize, usize)> = Vec::new();
    collect_calls_to(root, name, &source, func_def_start, func_def_end, &mut call_sites);

    if call_sites.is_empty() {
        return Err(miette::miette!(
            "no call sites for '{name}' found in this file"
        ));
    }

    let relative_file = crate::core::fs::relative_slash(file, project_root);
    let mut warnings = Vec::new();

    if !all && call_sites.len() > 1 {
        warnings.push(format!(
            "function '{name}' has {} call sites — use --all to inline all",
            call_sites.len()
        ));
    }

    let sites_to_inline = if all {
        call_sites.clone()
    } else {
        // Just inline the first call site
        vec![call_sites[0]]
    };

    let mut inlined_count = 0u32;

    if !dry_run {
        // Inline call sites from bottom to top to preserve line numbers
        let mut sorted_sites = sites_to_inline.clone();
        sorted_sites.sort_by(|a, b| b.0.cmp(&a.0));

        for (line, column) in &sorted_sites {
            // Re-read and re-parse after each inline (source changes)
            match inline_method(file, *line, *column, false, project_root) {
                Ok(_) => inlined_count += 1,
                Err(e) => warnings.push(format!("failed to inline at {}:{}: {e}", line, column)),
            }
        }

        // If we inlined all sites and the function still exists, delete it
        if all && inlined_count == sites_to_inline.len() as u32 {
            let current_source = std::fs::read_to_string(file)
                .map_err(|e| miette::miette!("cannot read file: {e}"))?;
            let current_tree = crate::core::parser::parse(&current_source)?;
            let current_root = current_tree.root_node();
            if let Some(def) = find_declaration_by_name(current_root, &current_source, name) {
                let (def_start, def_end) = declaration_full_range(def, &current_source);
                let mut final_source = String::with_capacity(current_source.len());
                final_source.push_str(&current_source[..def_start]);
                final_source.push_str(&current_source[def_end..]);
                normalize_blank_lines(&mut final_source);
                std::fs::write(file, &final_source)
                    .map_err(|e| miette::miette!("cannot write file: {e}"))?;
            }
        }
    } else {
        inlined_count = sites_to_inline.len() as u32;
    }

    // Check if function was deleted (either by inline_method for single callsite,
    // or by our explicit deletion above for --all)
    let function_deleted = if !dry_run && inlined_count > 0 {
        let current_source = std::fs::read_to_string(file)
            .map_err(|e| miette::miette!("cannot read file: {e}"))?;
        let current_tree = crate::core::parser::parse(&current_source)?;
        let current_root = current_tree.root_node();
        find_declaration_by_name(current_root, &current_source, name).is_none()
    } else {
        false
    };

    Ok(InlineMethodByNameOutput {
        function: name.to_string(),
        file: relative_file,
        call_sites_inlined: inlined_count,
        function_deleted,
        applied: !dry_run,
        warnings,
    })
}

/// Collect all call sites of `func_name` in the AST, excluding those within
/// the function definition itself.
fn collect_calls_to(
    node: tree_sitter::Node,
    func_name: &str,
    source: &str,
    func_def_start: usize,
    func_def_end: usize,
    out: &mut Vec<(usize, usize)>,
) {
    if node.kind() == "call" {
        let callee = node
            .child_by_field_name("function")
            .or_else(|| node.named_child(0));
        if let Some(callee) = callee
            && let Ok(name) = callee.utf8_text(source.as_bytes())
            && name == func_name
            && callee.kind() != "attribute"
        {
            let row = node.start_position().row;
            // Skip calls inside the function definition itself
            if row < func_def_start || row > func_def_end {
                out.push((row + 1, node.start_position().column + 1));
            }
        }
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_calls_to(child, func_name, source, func_def_start, func_def_end, out);
    }
}

/// Count return statements recursively in a node subtree.
fn count_return_statements(node: tree_sitter::Node, count: &mut usize) {
    if node.kind() == "return_statement" {
        *count += 1;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        count_return_statements(child, count);
    }
}

/// Find a `call` node that contains or starts at the given point.
fn find_call_at(root: tree_sitter::Node, point: tree_sitter::Point) -> Option<tree_sitter::Node> {
    let leaf = root.descendant_for_point_range(point, point)?;
    let mut node = leaf;
    loop {
        if node.kind() == "call" {
            return Some(node);
        }
        node = node.parent()?;
    }
}

/// Extract argument text strings from a call node.
fn extract_call_arguments(call: tree_sitter::Node, source: &str) -> Vec<String> {
    let mut args = Vec::new();
    if let Some(arg_node) = call.child_by_field_name("arguments") {
        let mut cursor = arg_node.walk();
        for child in arg_node.children(&mut cursor) {
            if child.is_named()
                && child.kind() != "("
                && child.kind() != ")"
                && child.kind() != ","
                && let Ok(text) = child.utf8_text(source.as_bytes())
            {
                args.push(text.to_string());
            }
        }
    }
    args
}

#[derive(Debug)]
struct ParamInfo {
    name: String,
    #[allow(dead_code)]
    type_hint: Option<String>,
    default: Option<String>,
}

/// Extract function parameter info from a function definition.
fn extract_function_params(func: tree_sitter::Node, source: &str) -> Vec<ParamInfo> {
    let mut params = Vec::new();
    let Some(params_node) = func.child_by_field_name("parameters") else {
        return params;
    };
    let mut cursor = params_node.walk();
    if cursor.goto_first_child() {
        loop {
            let child = cursor.node();
            match child.kind() {
                "identifier" => {
                    params.push(ParamInfo {
                        name: source[child.byte_range()].to_string(),
                        type_hint: None,
                        default: None,
                    });
                }
                "typed_parameter" => {
                    if let Some(name_node) = child.child(0) {
                        let type_hint = child
                            .child_by_field_name("type")
                            .and_then(|t| t.utf8_text(source.as_bytes()).ok())
                            .map(|s| s.to_string());
                        params.push(ParamInfo {
                            name: source[name_node.byte_range()].to_string(),
                            type_hint,
                            default: None,
                        });
                    }
                }
                "default_parameter" => {
                    if let Some(name_node) = child.child(0) {
                        let default = child
                            .child_by_field_name("value")
                            .and_then(|v| v.utf8_text(source.as_bytes()).ok())
                            .map(|s| s.to_string());
                        params.push(ParamInfo {
                            name: source[name_node.byte_range()].to_string(),
                            type_hint: None,
                            default,
                        });
                    }
                }
                "typed_default_parameter" => {
                    if let Some(name_node) = child.child(0) {
                        let type_hint = child
                            .child_by_field_name("type")
                            .and_then(|t| t.utf8_text(source.as_bytes()).ok())
                            .map(|s| s.to_string());
                        let default = child
                            .child_by_field_name("value")
                            .and_then(|v| v.utf8_text(source.as_bytes()).ok())
                            .map(|s| s.to_string());
                        params.push(ParamInfo {
                            name: source[name_node.byte_range()].to_string(),
                            type_hint,
                            default,
                        });
                    }
                }
                _ => {}
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
    params
}

/// Substitute parameter identifiers with argument expressions in body text.
/// Uses byte-level replacement to avoid substring false matches.
fn substitute_params(
    body_text: &str,
    param_map: &HashMap<String, String>,
    stmts: &[tree_sitter::Node],
    body_offset: usize,
    source: &str,
) -> String {
    if param_map.is_empty() {
        return body_text.to_string();
    }

    // Collect all identifier positions within the body that match param names
    let mut replacements: Vec<(usize, usize, String)> = Vec::new();
    for stmt in stmts {
        collect_param_replacements(*stmt, source, param_map, body_offset, &mut replacements);
    }

    // Sort replacements by position (reverse order for safe in-place replacement)
    replacements.sort_by(|a, b| b.0.cmp(&a.0));

    let mut result = body_text.to_string();
    for (start, end, replacement) in replacements {
        if start <= result.len() && end <= result.len() {
            result.replace_range(start..end, &replacement);
        }
    }
    result
}

fn collect_param_replacements(
    node: tree_sitter::Node,
    source: &str,
    param_map: &HashMap<String, String>,
    body_offset: usize,
    replacements: &mut Vec<(usize, usize, String)>,
) {
    if (node.kind() == "identifier" || node.kind() == "name")
        && let Ok(text) = node.utf8_text(source.as_bytes())
        && let Some(replacement) = param_map.get(text)
    {
        let rel_start = node.start_byte() - body_offset;
        let rel_end = node.end_byte() - body_offset;
        replacements.push((rel_start, rel_end, replacement.clone()));
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_param_replacements(child, source, param_map, body_offset, replacements);
    }
}

/// Check if a node contains a call to a function with the given name.
fn contains_call_to(node: tree_sitter::Node, name: &str, source: &str) -> bool {
    if node.kind() == "call" {
        let func_name = node
            .child_by_field_name("function")
            .or_else(|| node.named_child(0));
        if let Some(f) = func_name
            && f.utf8_text(source.as_bytes()).ok() == Some(name)
        {
            return true;
        }
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if contains_call_to(child, name, source) {
            return true;
        }
    }
    false
}

/// Re-indent text to match a target indent string (preserves relative indentation).
fn re_indent_to_depth_with_indent(text: &str, target_indent: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();

    let min_indent = lines
        .iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.len() - l.trim_start().len())
        .min()
        .unwrap_or(0);

    lines
        .iter()
        .map(|line| {
            if line.trim().is_empty() {
                String::new()
            } else if line.len() >= min_indent {
                format!("{target_indent}{}", &line[min_indent..])
            } else {
                format!("{target_indent}{line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

// ── change-signature ────────────────────────────────────────────────────────

#[derive(Serialize, Debug)]
pub struct ChangeSignatureOutput {
    pub function: String,
    pub file: String,
    pub old_signature: String,
    pub new_signature: String,
    pub call_sites_updated: u32,
    pub applied: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

#[allow(clippy::too_many_arguments)]
pub fn change_signature(
    file: &Path,
    name: &str,
    add_params: &[String],
    remove_params: &[String],
    rename_params: &[String],
    reorder: Option<&str>,
    class: Option<&str>,
    dry_run: bool,
    project_root: &Path,
) -> Result<ChangeSignatureOutput> {
    let source =
        std::fs::read_to_string(file).map_err(|e| miette::miette!("cannot read file: {e}"))?;
    let tree = crate::core::parser::parse(&source)?;
    let root = tree.root_node();

    // Find function definition
    let func_def = if let Some(class_name) = class {
        let class_node = find_class_definition(root, &source, class_name)
            .ok_or_else(|| miette::miette!("no inner class named '{class_name}' found"))?;
        find_declaration_in_class(class_node, &source, name)
            .ok_or_else(|| miette::miette!("no function '{name}' in class '{class_name}'"))?
    } else {
        find_declaration_by_name(root, &source, name)
            .ok_or_else(|| miette::miette!("no declaration named '{name}' found"))?
    };

    if !matches!(
        func_def.kind(),
        "function_definition" | "constructor_definition"
    ) {
        return Err(miette::miette!("'{name}' is not a function"));
    }

    // Parse existing parameters
    let mut existing_params = extract_function_params(func_def, &source);
    let old_param_str = existing_params
        .iter()
        .map(|p| {
            let mut s = p.name.clone();
            if let Some(ref t) = p.type_hint {
                s.push_str(&format!(": {t}"));
            }
            if let Some(ref d) = p.default {
                s.push_str(&format!(" = {d}"));
            }
            s
        })
        .collect::<Vec<_>>()
        .join(", ");
    let old_signature = format!("func {name}({old_param_str})");

    // Apply removals
    for remove in remove_params {
        let idx = existing_params.iter().position(|p| p.name == *remove);
        if let Some(i) = idx {
            existing_params.remove(i);
        } else {
            return Err(miette::miette!("parameter '{remove}' not found"));
        }
    }

    // Apply renames
    let mut rename_map: HashMap<String, String> = HashMap::new();
    for rename in rename_params {
        let Some((old_name, new_name)) = rename.split_once('=') else {
            return Err(miette::miette!(
                "invalid --rename-param format: '{rename}' (expected 'old_name=new_name')"
            ));
        };
        let old_name = old_name.trim();
        let new_name = new_name.trim();
        if old_name.is_empty() || new_name.is_empty() {
            return Err(miette::miette!(
                "invalid --rename-param: both old and new names must be non-empty"
            ));
        }
        let idx = existing_params.iter().position(|p| p.name == old_name);
        if let Some(i) = idx {
            existing_params[i].name = new_name.to_string();
            rename_map.insert(old_name.to_string(), new_name.to_string());
        } else {
            return Err(miette::miette!("parameter '{old_name}' not found for rename"));
        }
    }

    // Apply additions
    for add in add_params {
        let parsed = parse_param_spec(add)?;
        if existing_params.iter().any(|p| p.name == parsed.name) {
            return Err(miette::miette!(
                "parameter '{}' already exists",
                parsed.name
            ));
        }
        existing_params.push(parsed);
    }

    // Apply reorder
    if let Some(order_str) = reorder {
        let order: Vec<&str> = order_str.split(',').map(|s| s.trim()).collect();
        let mut reordered = Vec::new();
        for name_ref in &order {
            let idx = existing_params.iter().position(|p| p.name == *name_ref);
            if let Some(i) = idx {
                reordered.push(existing_params.remove(i));
            } else {
                return Err(miette::miette!(
                    "parameter '{name_ref}' not found for reorder"
                ));
            }
        }
        // Append any remaining params not in the reorder list
        reordered.append(&mut existing_params);
        existing_params = reordered;
    }

    // Build new parameter string
    let new_param_str = existing_params
        .iter()
        .map(|p| {
            let mut s = p.name.clone();
            if let Some(ref t) = p.type_hint {
                s.push_str(&format!(": {t}"));
            }
            if let Some(ref d) = p.default {
                s.push_str(&format!(" = {d}"));
            }
            s
        })
        .collect::<Vec<_>>()
        .join(", ");
    let new_signature = format!("func {name}({new_param_str})");

    // Find the parameters node to replace
    let params_node = func_def.child_by_field_name("parameters");

    let relative_file = crate::core::fs::relative_slash(file, project_root);
    let mut warnings = Vec::new();

    // Find call sites to update
    let workspace = super::workspace::WorkspaceIndex::new(project_root.to_path_buf());
    let all_refs = super::references::find_references_by_name(name, &workspace, None, None);
    let file_uri = tower_lsp::lsp_types::Url::from_file_path(file).ok();
    let func_def_start = func_def.start_position().row as u32;
    let func_def_end = func_def.end_position().row as u32;

    // Collect call site info for updating
    let mut call_sites: Vec<(std::path::PathBuf, u32, u32)> = Vec::new();
    for loc in &all_refs {
        if let Some(ref uri) = file_uri
            && &loc.uri == uri
        {
            let ref_line = loc.range.start.line;
            if ref_line >= func_def_start && ref_line <= func_def_end {
                continue; // Skip references within the function itself
            }
        }
        if let Ok(path) = loc.uri.to_file_path() {
            call_sites.push((path, loc.range.start.line, loc.range.start.character));
        }
    }

    let mut call_sites_updated = 0u32;

    if !dry_run {
        // 1. Update function definition (signature + body renames)
        let mut new_source = source.clone();
        if let Some(pn) = params_node {
            // Replace content between parens
            let params_start = pn.start_byte();
            let params_end = pn.end_byte();
            let new_params_text = format!("({new_param_str})");
            new_source.replace_range(params_start..params_end, &new_params_text);
        }

        // Rename param usages in function body
        if !rename_map.is_empty() {
            // Re-parse after signature change to get correct byte offsets
            let new_tree = crate::core::parser::parse(&new_source)?;
            let new_root = new_tree.root_node();
            if let Some(new_func) = find_declaration_by_name(new_root, &new_source, name)
                && let Some(body) = new_func.child_by_field_name("body")
            {
                let body_start = body.start_byte();
                let body_end = body.end_byte();
                let mut body_text = new_source[body_start..body_end].to_string();
                for (old_name, new_name) in &rename_map {
                    body_text = rename_identifier_in_text(&body_text, old_name, new_name);
                }
                new_source.replace_range(body_start..body_end, &body_text);
            }
        }

        std::fs::write(file, &new_source).map_err(|e| miette::miette!("cannot write file: {e}"))?;

        // 2. Update call sites
        // Group call sites by file
        let mut sites_by_file: HashMap<std::path::PathBuf, Vec<(u32, u32)>> = HashMap::new();
        for (path, line, col) in &call_sites {
            sites_by_file
                .entry(path.clone())
                .or_default()
                .push((*line, *col));
        }

        for (call_file, positions) in &sites_by_file {
            let cs = std::fs::read_to_string(call_file)
                .map_err(|e| miette::miette!("cannot read {}: {e}", call_file.display()))?;
            let cs_tree = crate::core::parser::parse(&cs)?;
            let cs_root = cs_tree.root_node();

            let mut edits: Vec<(usize, usize, String)> = Vec::new();
            for &(ref_line, ref_col) in positions {
                let pt = tree_sitter::Point::new(ref_line as usize, ref_col as usize);
                if let Some(call) = find_call_at(cs_root, pt)
                    && let Some(args_node) = call.child_by_field_name("arguments")
                {
                    let old_args = extract_call_arguments(call, &cs);
                    let new_args = rewrite_call_arguments(
                        &old_args,
                        &extract_function_params(func_def, &source),
                        &existing_params,
                        remove_params,
                        add_params,
                        &rename_map,
                        reorder,
                    );
                    let new_args_text = format!("({})", new_args.join(", "));
                    edits.push((args_node.start_byte(), args_node.end_byte(), new_args_text));
                    call_sites_updated += 1;
                }
            }

            if !edits.is_empty() {
                edits.sort_by(|a, b| b.0.cmp(&a.0));
                let mut cs_new = cs;
                for (start, end, replacement) in edits {
                    cs_new.replace_range(start..end, &replacement);
                }
                std::fs::write(call_file, &cs_new)
                    .map_err(|e| miette::miette!("cannot write {}: {e}", call_file.display()))?;
            }
        }
    } else {
        call_sites_updated = call_sites.len() as u32;
    }

    // Check for non-call references (variable references to the function name)
    let non_call_count = call_sites.len() as u32 - call_sites_updated;
    if non_call_count > 0 && !dry_run {
        warnings.push(format!(
            "{non_call_count} non-call reference{} to '{name}' may need manual updating",
            if non_call_count == 1 { "" } else { "s" }
        ));
    }

    Ok(ChangeSignatureOutput {
        function: name.to_string(),
        file: relative_file,
        old_signature,
        new_signature,
        call_sites_updated,
        applied: !dry_run,
        warnings,
    })
}

/// Parse a parameter spec string like "name: Type = default" or just "name".
fn parse_param_spec(spec: &str) -> Result<ParamInfo> {
    let spec = spec.trim();

    // Check for default value
    let (before_default, default) = if let Some(eq_pos) = spec.find('=') {
        let name_type = spec[..eq_pos].trim();
        let default_val = spec[eq_pos + 1..].trim().to_string();
        (name_type, Some(default_val))
    } else {
        (spec, None)
    };

    // Check for type hint
    let (name, type_hint) = if let Some(colon_pos) = before_default.find(':') {
        let name = before_default[..colon_pos].trim().to_string();
        let type_h = before_default[colon_pos + 1..].trim().to_string();
        (name, Some(type_h))
    } else {
        (before_default.to_string(), None)
    };

    if name.is_empty() {
        return Err(miette::miette!("empty parameter name in '{spec}'"));
    }

    Ok(ParamInfo {
        name,
        type_hint,
        default,
    })
}

/// Rename an identifier in text, only matching whole words (not substrings).
fn rename_identifier_in_text(text: &str, old_name: &str, new_name: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let old_bytes = old_name.as_bytes();
    let old_len = old_bytes.len();
    let text_bytes = text.as_bytes();
    let text_len = text_bytes.len();
    let mut i = 0;

    while i < text_len {
        if i + old_len <= text_len && &text_bytes[i..i + old_len] == old_bytes {
            // Check word boundary before
            let before_ok = i == 0 || !is_ident_char(text_bytes[i - 1]);
            // Check word boundary after
            let after_ok = i + old_len >= text_len || !is_ident_char(text_bytes[i + old_len]);
            if before_ok && after_ok {
                result.push_str(new_name);
                i += old_len;
                continue;
            }
        }
        result.push(text_bytes[i] as char);
        i += 1;
    }
    result
}

fn is_ident_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Rewrite call arguments based on parameter changes.
fn rewrite_call_arguments(
    old_args: &[String],
    old_params: &[ParamInfo],
    new_params: &[ParamInfo],
    remove_params: &[String],
    _add_params: &[String],
    rename_map: &HashMap<String, String>,
    reorder: Option<&str>,
) -> Vec<String> {
    // Build old param name → arg value mapping
    let mut arg_map: HashMap<String, String> = HashMap::new();
    for (i, param) in old_params.iter().enumerate() {
        if let Some(arg) = old_args.get(i) {
            arg_map.insert(param.name.clone(), arg.clone());
        }
    }

    // Map renamed param entries: insert under new name too
    for (old_name, new_name) in rename_map {
        if let Some(arg) = arg_map.get(old_name).cloned() {
            arg_map.insert(new_name.clone(), arg);
        }
    }

    // Remove entries for removed params
    for name in remove_params {
        arg_map.remove(name.as_str());
    }

    // Build new argument list in new param order
    let mut new_args = Vec::new();
    let reorder_names: Option<Vec<&str>> =
        reorder.map(|r| r.split(',').map(|s| s.trim()).collect());

    for param in new_params {
        if let Some(arg) = arg_map.get(&param.name) {
            new_args.push(arg.clone());
        } else if let Some(ref default) = param.default {
            new_args.push(default.clone());
        } else {
            // Added param without default — use placeholder
            new_args.push(format!("/* {} */", param.name));
        }
    }

    // If reordering without add/remove, use the reorder directly
    if let Some(names) = reorder_names
        && remove_params.is_empty()
    {
        let mut reordered = Vec::new();
        for name in &names {
            if let Some(arg) = arg_map.get(*name) {
                reordered.push(arg.clone());
            }
        }
        // Add remaining args not in reorder list
        for param in new_params {
            if !names.contains(&param.name.as_str())
                && let Some(arg) = arg_map.get(&param.name)
            {
                reordered.push(arg.clone());
            }
        }
        if reordered.len() >= new_args.len() {
            return reordered;
        }
    }

    new_args
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn setup_project(files: &[(&str, &str)]) -> TempDir {
        let temp = tempfile::Builder::new()
            .prefix("gdtest")
            .tempdir()
            .expect("create temp dir");
        fs::write(
            temp.path().join("project.godot"),
            "[application]\nconfig/name=\"test\"\n",
        )
        .expect("write project.godot");
        for (name, content) in files {
            fs::write(temp.path().join(name), content).expect("write file");
        }
        temp
    }

    // ── find_declaration_by_name ──────────────────────────────────────────

    #[test]
    fn find_function_by_name() {
        let src = "func foo():\n\tpass\n";
        let tree = crate::core::parser::parse(src).unwrap();
        let node = find_declaration_by_name(tree.root_node(), src, "foo");
        assert!(node.is_some());
        assert_eq!(node.unwrap().kind(), "function_definition");
    }

    #[test]
    fn find_variable_by_name() {
        let src = "var speed = 10\n";
        let tree = crate::core::parser::parse(src).unwrap();
        let node = find_declaration_by_name(tree.root_node(), src, "speed");
        assert!(node.is_some());
        assert_eq!(node.unwrap().kind(), "variable_statement");
    }

    #[test]
    fn find_const_by_name() {
        let src = "const MAX_HP = 200\n";
        let tree = crate::core::parser::parse(src).unwrap();
        let node = find_declaration_by_name(tree.root_node(), src, "MAX_HP");
        assert!(node.is_some());
        assert_eq!(node.unwrap().kind(), "const_statement");
    }

    #[test]
    fn find_signal_by_name() {
        let src = "signal died\n";
        let tree = crate::core::parser::parse(src).unwrap();
        let node = find_declaration_by_name(tree.root_node(), src, "died");
        assert!(node.is_some());
        assert_eq!(node.unwrap().kind(), "signal_statement");
    }

    #[test]
    fn find_enum_by_name() {
        let src = "enum State { IDLE, RUN }\n";
        let tree = crate::core::parser::parse(src).unwrap();
        let node = find_declaration_by_name(tree.root_node(), src, "State");
        assert!(node.is_some());
        assert_eq!(node.unwrap().kind(), "enum_definition");
    }

    #[test]
    fn find_class_by_name() {
        let src = "class Inner:\n\tvar x = 1\n";
        let tree = crate::core::parser::parse(src).unwrap();
        let node = find_declaration_by_name(tree.root_node(), src, "Inner");
        assert!(node.is_some());
        assert_eq!(node.unwrap().kind(), "class_definition");
    }

    #[test]
    fn find_constructor() {
        let src = "func _init():\n\tpass\n";
        let tree = crate::core::parser::parse(src).unwrap();
        let node = find_declaration_by_name(tree.root_node(), src, "_init");
        assert!(node.is_some());
    }

    #[test]
    fn find_not_found() {
        let src = "var speed = 10\n";
        let tree = crate::core::parser::parse(src).unwrap();
        let node = find_declaration_by_name(tree.root_node(), src, "nonexistent");
        assert!(node.is_none());
    }

    // ── find_declaration_by_line ──────────────────────────────────────────

    #[test]
    fn find_decl_by_line() {
        let src = "var a = 1\nvar b = 2\n\n\nfunc foo():\n\tpass\n";
        let tree = crate::core::parser::parse(src).unwrap();
        // Line 4 (0-based) is "func foo():"
        let node = find_declaration_by_line(tree.root_node(), 4);
        assert!(node.is_some());
        assert_eq!(
            get_declaration_name(node.unwrap(), src),
            Some("foo".to_string())
        );
    }

    // ── declaration_full_range ────────────────────────────────────────────

    #[test]
    fn full_range_without_comments() {
        let src = "var a = 1\n\nfunc foo():\n\tpass\n";
        let tree = crate::core::parser::parse(src).unwrap();
        let node = find_declaration_by_name(tree.root_node(), src, "foo").unwrap();
        let (start, end) = declaration_full_range(node, src);
        assert_eq!(&src[start..end], "func foo():\n\tpass\n");
    }

    #[test]
    fn full_range_with_comments() {
        let src = "var a = 1\n\n## Documentation\n# More docs\nfunc foo():\n\tpass\n";
        let tree = crate::core::parser::parse(src).unwrap();
        let node = find_declaration_by_name(tree.root_node(), src, "foo").unwrap();
        let (start, end) = declaration_full_range(node, src);
        assert_eq!(
            &src[start..end],
            "## Documentation\n# More docs\nfunc foo():\n\tpass\n"
        );
    }

    #[test]
    fn full_range_comment_stops_at_blank_line() {
        let src = "# Unrelated comment\n\n# Doc comment\nfunc foo():\n\tpass\n";
        let tree = crate::core::parser::parse(src).unwrap();
        let node = find_declaration_by_name(tree.root_node(), src, "foo").unwrap();
        let (start, end) = declaration_full_range(node, src);
        assert_eq!(&src[start..end], "# Doc comment\nfunc foo():\n\tpass\n");
    }

    #[test]
    fn full_range_annotation() {
        let src = "@export var speed = 10\n";
        let tree = crate::core::parser::parse(src).unwrap();
        let node = find_declaration_by_name(tree.root_node(), src, "speed").unwrap();
        let (start, end) = declaration_full_range(node, src);
        // Annotation is part of the node, so the range covers it
        assert_eq!(&src[start..end], "@export var speed = 10\n");
    }

    // ── normalize_blank_lines ────────────────────────────────────────────

    #[test]
    fn normalize_collapses_excess() {
        let mut s = "a\n\n\n\n\nb".to_string();
        normalize_blank_lines(&mut s);
        assert_eq!(s, "a\n\n\nb");
    }

    #[test]
    fn normalize_keeps_two_blank_lines() {
        let mut s = "a\n\n\nb".to_string();
        normalize_blank_lines(&mut s);
        assert_eq!(s, "a\n\n\nb");
    }

    // ── delete-symbol ────────────────────────────────────────────────────

    #[test]
    fn delete_function() {
        let temp = setup_project(&[(
            "player.gd",
            "var health = 100\n\n\nfunc unused():\n\tpass\n\n\nfunc _ready():\n\tpass\n",
        )]);
        let result = delete_symbol(
            &temp.path().join("player.gd"),
            Some("unused"),
            None,
            false,
            false,
            temp.path(),
            None,
        )
        .unwrap();
        assert!(result.applied);
        assert_eq!(result.kind, "function");
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(!content.contains("unused"));
        assert!(content.contains("health"));
        assert!(content.contains("_ready"));
    }

    #[test]
    fn delete_variable() {
        let temp = setup_project(&[("player.gd", "var unused_var = 1\nvar keep = 2\n")]);
        let result = delete_symbol(
            &temp.path().join("player.gd"),
            Some("unused_var"),
            None,
            false,
            false,
            temp.path(),
            None,
        )
        .unwrap();
        assert!(result.applied);
        assert_eq!(result.kind, "variable");
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(!content.contains("unused_var"));
        assert!(content.contains("keep"));
    }

    #[test]
    fn delete_constant() {
        let temp = setup_project(&[("player.gd", "const OLD = 1\nconst KEEP = 2\n")]);
        let result = delete_symbol(
            &temp.path().join("player.gd"),
            Some("OLD"),
            None,
            false,
            false,
            temp.path(),
            None,
        )
        .unwrap();
        assert!(result.applied);
        assert_eq!(result.kind, "constant");
    }

    #[test]
    fn delete_signal() {
        let temp = setup_project(&[("player.gd", "signal unused\nsignal keep\n")]);
        let result = delete_symbol(
            &temp.path().join("player.gd"),
            Some("unused"),
            None,
            false,
            false,
            temp.path(),
            None,
        )
        .unwrap();
        assert!(result.applied);
        assert_eq!(result.kind, "signal");
    }

    #[test]
    fn delete_enum() {
        let temp = setup_project(&[("player.gd", "enum OldState { A, B }\nenum State { C, D }\n")]);
        let result = delete_symbol(
            &temp.path().join("player.gd"),
            Some("OldState"),
            None,
            false,
            false,
            temp.path(),
            None,
        )
        .unwrap();
        assert!(result.applied);
        assert_eq!(result.kind, "enum");
    }

    #[test]
    fn delete_class() {
        let temp = setup_project(&[(
            "player.gd",
            "class Unused:\n\tvar x = 1\n\nclass Keep:\n\tvar y = 2\n",
        )]);
        let result = delete_symbol(
            &temp.path().join("player.gd"),
            Some("Unused"),
            None,
            false,
            false,
            temp.path(),
            None,
        )
        .unwrap();
        assert!(result.applied);
        assert_eq!(result.kind, "class");
    }

    #[test]
    fn delete_with_doc_comments() {
        let temp = setup_project(&[(
            "player.gd",
            "## This is documented\n## More docs\nfunc documented():\n\tpass\n",
        )]);
        let result = delete_symbol(
            &temp.path().join("player.gd"),
            Some("documented"),
            None,
            false,
            false,
            temp.path(),
            None,
        )
        .unwrap();
        assert!(result.applied);
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(
            !content.contains("documented"),
            "function should be removed"
        );
        assert!(
            !content.contains("## This is documented"),
            "doc comments should be removed"
        );
    }

    #[test]
    fn delete_by_line() {
        let temp = setup_project(&[(
            "player.gd",
            "var a = 1\n\n\nfunc target():\n\tpass\n\n\nfunc keep():\n\tpass\n",
        )]);
        let result = delete_symbol(
            &temp.path().join("player.gd"),
            None,
            Some(4), // line 4 is "func target():"
            false,
            false,
            temp.path(),
            None,
        )
        .unwrap();
        assert!(result.applied);
        assert_eq!(result.symbol, "target");
    }

    #[test]
    fn delete_not_found() {
        let temp = setup_project(&[("player.gd", "var x = 1\n")]);
        let result = delete_symbol(
            &temp.path().join("player.gd"),
            Some("nonexistent"),
            None,
            false,
            false,
            temp.path(),
            None,
        );
        assert!(result.is_err());
    }

    #[test]
    fn delete_blocked_by_references() {
        let temp = setup_project(&[
            (
                "player.gd",
                "var speed = 10\n\n\nfunc run():\n\tprint(speed)\n",
            ),
            ("enemy.gd", "var speed = 5\n"),
        ]);
        let result = delete_symbol(
            &temp.path().join("player.gd"),
            Some("speed"),
            None,
            false,
            false,
            temp.path(),
            None,
        )
        .unwrap();
        assert!(!result.applied, "should not delete when references exist");
        assert!(
            !result.references.is_empty(),
            "should list external references"
        );
    }

    #[test]
    fn delete_force_with_references() {
        let temp = setup_project(&[(
            "player.gd",
            "var speed = 10\n\n\nfunc run():\n\tprint(speed)\n",
        )]);
        let result = delete_symbol(
            &temp.path().join("player.gd"),
            Some("speed"),
            None,
            true,  // force
            false, // not dry run
            temp.path(),
            None,
        )
        .unwrap();
        assert!(result.applied, "force should override reference check");
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(!content.contains("var speed"), "should be deleted");
    }

    #[test]
    fn delete_dry_run() {
        let temp = setup_project(&[(
            "player.gd",
            "func unused():\n\tpass\n\n\nfunc keep():\n\tpass\n",
        )]);
        let result = delete_symbol(
            &temp.path().join("player.gd"),
            Some("unused"),
            None,
            false,
            true, // dry run
            temp.path(),
            None,
        )
        .unwrap();
        assert!(!result.applied);
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(content.contains("unused"), "dry run should not modify file");
    }

    // ── move-symbol ──────────────────────────────────────────────────────

    #[test]
    fn move_to_new_file() {
        let temp = setup_project(&[("source.gd", "var keep = 1\n\n\nfunc helper():\n\tpass\n")]);
        let result = move_symbol(
            "helper",
            &temp.path().join("source.gd"),
            &temp.path().join("helpers.gd"),
            false,
            temp.path(),
            None,
            None,
        )
        .unwrap();
        assert!(result.applied);
        assert_eq!(result.kind, "function");
        assert!(
            temp.path().join("helpers.gd").exists(),
            "target file should be created"
        );
        let target = fs::read_to_string(temp.path().join("helpers.gd")).unwrap();
        assert!(target.contains("func helper()"));
        let source = fs::read_to_string(temp.path().join("source.gd")).unwrap();
        assert!(!source.contains("helper"));
        assert!(source.contains("keep"));
    }

    #[test]
    fn move_to_existing_file() {
        let temp = setup_project(&[
            (
                "source.gd",
                "func to_move():\n\tpass\n\n\nfunc stay():\n\tpass\n",
            ),
            ("target.gd", "func existing():\n\tpass\n"),
        ]);
        let result = move_symbol(
            "to_move",
            &temp.path().join("source.gd"),
            &temp.path().join("target.gd"),
            false,
            temp.path(),
            None,
            None,
        )
        .unwrap();
        assert!(result.applied);
        let target = fs::read_to_string(temp.path().join("target.gd")).unwrap();
        assert!(target.contains("func existing()"));
        assert!(target.contains("func to_move()"));
    }

    #[test]
    fn move_constant() {
        let temp = setup_project(&[("source.gd", "const A = 1\nconst B = 2\n")]);
        let result = move_symbol(
            "A",
            &temp.path().join("source.gd"),
            &temp.path().join("target.gd"),
            false,
            temp.path(),
            None,
            None,
        )
        .unwrap();
        assert!(result.applied);
        assert_eq!(result.kind, "constant");
    }

    #[test]
    fn move_signal() {
        let temp = setup_project(&[("source.gd", "signal moved\nsignal stay\n")]);
        let result = move_symbol(
            "moved",
            &temp.path().join("source.gd"),
            &temp.path().join("target.gd"),
            false,
            temp.path(),
            None,
            None,
        )
        .unwrap();
        assert!(result.applied);
        assert_eq!(result.kind, "signal");
    }

    #[test]
    fn move_class() {
        let temp = setup_project(&[("source.gd", "class Helper:\n\tvar x = 1\n\nvar keep = 2\n")]);
        let result = move_symbol(
            "Helper",
            &temp.path().join("source.gd"),
            &temp.path().join("target.gd"),
            false,
            temp.path(),
            None,
            None,
        )
        .unwrap();
        assert!(result.applied);
        assert_eq!(result.kind, "class");
    }

    #[test]
    fn move_duplicate_error() {
        let temp = setup_project(&[
            ("source.gd", "func helper():\n\tpass\n"),
            ("target.gd", "func helper():\n\treturn 1\n"),
        ]);
        let result = move_symbol(
            "helper",
            &temp.path().join("source.gd"),
            &temp.path().join("target.gd"),
            false,
            temp.path(),
            None,
            None,
        );
        assert!(result.is_err());
    }

    #[test]
    fn move_dry_run() {
        let temp = setup_project(&[(
            "source.gd",
            "func helper():\n\tpass\n\n\nfunc keep():\n\tpass\n",
        )]);
        let result = move_symbol(
            "helper",
            &temp.path().join("source.gd"),
            &temp.path().join("target.gd"),
            true, // dry run
            temp.path(),
            None,
            None,
        )
        .unwrap();
        assert!(!result.applied);
        assert!(
            !temp.path().join("target.gd").exists(),
            "dry run should not create file"
        );
        let source = fs::read_to_string(temp.path().join("source.gd")).unwrap();
        assert!(
            source.contains("helper"),
            "dry run should not modify source"
        );
    }

    #[test]
    fn move_correct_spacing() {
        let temp = setup_project(&[
            ("source.gd", "func moved():\n\tpass\n"),
            ("target.gd", "var x = 1\n"),
        ]);
        let _ = move_symbol(
            "moved",
            &temp.path().join("source.gd"),
            &temp.path().join("target.gd"),
            false,
            temp.path(),
            None,
            None,
        )
        .unwrap();
        let target = fs::read_to_string(temp.path().join("target.gd")).unwrap();
        // Functions should have 2 blank lines before them
        assert!(
            target.contains("\n\n\nfunc moved()"),
            "should have 2 blank lines before function, got: {:?}",
            target
        );
    }

    // ── inner class operations (Feature 3) ─────────────────────────────

    #[test]
    fn delete_from_inner_class() {
        let temp = setup_project(&[(
            "player.gd",
            "class Inner:\n\tvar keep = 1\n\tfunc remove_me():\n\t\tpass\n",
        )]);
        let result = delete_symbol(
            &temp.path().join("player.gd"),
            Some("remove_me"),
            None,
            false,
            false,
            temp.path(),
            Some("Inner"),
        )
        .unwrap();
        assert!(result.applied);
        assert_eq!(result.kind, "function");
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(!content.contains("remove_me"));
        assert!(content.contains("keep"));
    }

    #[test]
    fn delete_var_from_inner_class() {
        let temp = setup_project(&[("player.gd", "class Inner:\n\tvar old = 1\n\tvar keep = 2\n")]);
        let result = delete_symbol(
            &temp.path().join("player.gd"),
            Some("old"),
            None,
            false,
            false,
            temp.path(),
            Some("Inner"),
        )
        .unwrap();
        assert!(result.applied);
        assert_eq!(result.kind, "variable");
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(!content.contains("old"));
        assert!(content.contains("keep"));
    }

    #[test]
    fn delete_class_not_found() {
        let temp = setup_project(&[("player.gd", "var x = 1\n")]);
        let result = delete_symbol(
            &temp.path().join("player.gd"),
            Some("foo"),
            None,
            false,
            false,
            temp.path(),
            Some("NonExistent"),
        );
        assert!(result.is_err());
    }

    #[test]
    fn move_from_inner_class_to_top_level() {
        let temp = setup_project(&[(
            "source.gd",
            "class Inner:\n\tvar keep = 1\n\tfunc helper():\n\t\tpass\n",
        )]);
        let result = move_symbol(
            "helper",
            &temp.path().join("source.gd"),
            &temp.path().join("target.gd"),
            false,
            temp.path(),
            Some("Inner"),
            None,
        )
        .unwrap();
        assert!(result.applied);
        let target = fs::read_to_string(temp.path().join("target.gd")).unwrap();
        // Should be re-indented to top-level (no leading tab)
        assert!(
            target.contains("func helper():"),
            "should be at top-level indent, got: {target}"
        );
        assert!(
            !target.contains("\tfunc helper"),
            "should NOT have tab-indented func, got: {target}"
        );
    }

    #[test]
    fn move_top_level_into_class() {
        let temp = setup_project(&[
            ("source.gd", "func helper():\n\tpass\n"),
            ("target.gd", "class Target:\n\tvar x = 1\n"),
        ]);
        let result = move_symbol(
            "helper",
            &temp.path().join("source.gd"),
            &temp.path().join("target.gd"),
            false,
            temp.path(),
            None,
            Some("Target"),
        )
        .unwrap();
        assert!(result.applied);
        let target = fs::read_to_string(temp.path().join("target.gd")).unwrap();
        assert!(
            target.contains("\tfunc helper():"),
            "should be indented in class, got: {target}"
        );
    }

    // ── enum member operations (Feature 4) ─────────────────────────────

    #[test]
    fn delete_enum_member_first() {
        let temp = setup_project(&[("player.gd", "enum State { IDLE, RUN, JUMP }\n")]);
        let result = delete_symbol(
            &temp.path().join("player.gd"),
            Some("State.IDLE"),
            None,
            false,
            false,
            temp.path(),
            None,
        )
        .unwrap();
        assert!(result.applied);
        assert_eq!(result.kind, "enum_member");
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(!content.contains("IDLE"), "IDLE should be removed");
        assert!(content.contains("RUN"), "RUN should remain");
        assert!(content.contains("JUMP"), "JUMP should remain");
    }

    #[test]
    fn delete_enum_member_last() {
        let temp = setup_project(&[("player.gd", "enum State { IDLE, RUN, JUMP }\n")]);
        let result = delete_symbol(
            &temp.path().join("player.gd"),
            Some("State.JUMP"),
            None,
            false,
            false,
            temp.path(),
            None,
        )
        .unwrap();
        assert!(result.applied);
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(!content.contains("JUMP"), "JUMP should be removed");
        assert!(content.contains("IDLE"), "IDLE should remain");
        assert!(content.contains("RUN"), "RUN should remain");
    }

    #[test]
    fn delete_enum_member_middle() {
        let temp = setup_project(&[("player.gd", "enum State { IDLE, RUN, JUMP }\n")]);
        let result = delete_symbol(
            &temp.path().join("player.gd"),
            Some("State.RUN"),
            None,
            false,
            false,
            temp.path(),
            None,
        )
        .unwrap();
        assert!(result.applied);
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(!content.contains("RUN"), "RUN should be removed");
        assert!(content.contains("IDLE"), "IDLE should remain");
        assert!(content.contains("JUMP"), "JUMP should remain");
    }

    #[test]
    fn delete_enum_member_with_value() {
        let temp = setup_project(&[("player.gd", "enum State { IDLE = 0, RUN = 1, JUMP = 2 }\n")]);
        let result = delete_symbol(
            &temp.path().join("player.gd"),
            Some("State.RUN"),
            None,
            false,
            false,
            temp.path(),
            None,
        )
        .unwrap();
        assert!(result.applied);
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(!content.contains("RUN"), "RUN should be removed");
    }

    #[test]
    fn delete_enum_member_last_one_error() {
        let temp = setup_project(&[("player.gd", "enum State { ONLY }\n")]);
        let result = delete_symbol(
            &temp.path().join("player.gd"),
            Some("State.ONLY"),
            None,
            false,
            false,
            temp.path(),
            None,
        );
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("last member"), "should error: {err}");
    }

    #[test]
    fn delete_enum_member_not_found() {
        let temp = setup_project(&[("player.gd", "enum State { IDLE, RUN }\n")]);
        let result = delete_symbol(
            &temp.path().join("player.gd"),
            Some("State.JUMP"),
            None,
            false,
            false,
            temp.path(),
            None,
        );
        assert!(result.is_err());
    }

    // ── preload detection (Feature 1) ────────────────────────────────────

    #[test]
    fn move_detects_preloads_to_source_file() {
        let temp = setup_project(&[
            ("source.gd", "func helper():\n\tpass\n"),
            (
                "other.gd",
                "var x = preload(\"res://source.gd\")\nfunc _ready():\n\tpass\n",
            ),
        ]);
        let result = move_symbol(
            "helper",
            &temp.path().join("source.gd"),
            &temp.path().join("target.gd"),
            true, // dry run to just check
            temp.path(),
            None,
            None,
        )
        .unwrap();
        assert!(
            !result.preloads.is_empty(),
            "should detect preload to source file"
        );
        assert_eq!(result.preloads[0].path, "res://source.gd");
    }

    #[test]
    fn move_no_preloads_unrelated() {
        let temp = setup_project(&[
            ("source.gd", "func helper():\n\tpass\n"),
            ("other.gd", "var x = preload(\"res://other_thing.gd\")\n"),
        ]);
        let result = move_symbol(
            "helper",
            &temp.path().join("source.gd"),
            &temp.path().join("target.gd"),
            true,
            temp.path(),
            None,
            None,
        )
        .unwrap();
        assert!(
            result.preloads.is_empty(),
            "should not list unrelated preloads"
        );
    }

    // ── self-reference warnings (Feature 8) ──────────────────────────────

    #[test]
    fn move_self_ref_warning_missing_member() {
        let temp = setup_project(&[
            (
                "source.gd",
                "class Src:\n\tvar health = 100\n\tfunc take_damage():\n\t\tself.health -= 10\n",
            ),
            ("target.gd", "class Dst:\n\tvar armor = 50\n"),
        ]);
        let result = move_symbol(
            "take_damage",
            &temp.path().join("source.gd"),
            &temp.path().join("target.gd"),
            true,
            temp.path(),
            Some("Src"),
            Some("Dst"),
        )
        .unwrap();
        assert!(
            result.warnings.iter().any(|w| w.contains("self.health")),
            "should warn about missing self.health, got: {:?}",
            result.warnings
        );
    }

    #[test]
    fn move_self_ref_no_warning_when_present() {
        let temp = setup_project(&[
            (
                "source.gd",
                "class Src:\n\tvar health = 100\n\tfunc take_damage():\n\t\tself.health -= 10\n",
            ),
            ("target.gd", "class Dst:\n\tvar health = 200\n"),
        ]);
        let result = move_symbol(
            "take_damage",
            &temp.path().join("source.gd"),
            &temp.path().join("target.gd"),
            true,
            temp.path(),
            Some("Src"),
            Some("Dst"),
        )
        .unwrap();
        assert!(
            !result.warnings.iter().any(|w| w.contains("self.health")),
            "should NOT warn when member exists, got: {:?}",
            result.warnings
        );
    }

    #[test]
    fn move_no_self_refs_no_warning() {
        let temp = setup_project(&[
            (
                "source.gd",
                "class Src:\n\tfunc helper():\n\t\tprint(\"hello\")\n",
            ),
            ("target.gd", "class Dst:\n\tvar x = 1\n"),
        ]);
        let result = move_symbol(
            "helper",
            &temp.path().join("source.gd"),
            &temp.path().join("target.gd"),
            true,
            temp.path(),
            Some("Src"),
            Some("Dst"),
        )
        .unwrap();
        assert!(
            !result.warnings.iter().any(|w| w.contains("self.")),
            "no self refs means no self-ref warnings"
        );
    }

    // ── extract-method ───────────────────────────────────────────────────

    #[test]
    fn extract_simple_no_captures() {
        let temp = setup_project(&[(
            "player.gd",
            "func _ready():\n\tvar x = 1\n\tprint(x)\n\tprint(\"done\")\n",
        )]);
        // Extract just `print("done")` (line 4)
        let result = extract_method(
            &temp.path().join("player.gd"),
            4,
            4,
            "do_print",
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        assert!(result.parameters.is_empty());
        assert!(result.returns.is_none());
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(content.contains("func do_print():"));
        assert!(content.contains("do_print()"));
    }

    #[test]
    fn extract_with_read_params() {
        let temp = setup_project(&[(
            "player.gd",
            "func process():\n\tvar health = 100\n\tvar armor = 50\n\tprint(health)\n\tprint(armor)\n\tprint(\"end\")\n",
        )]);
        // Extract print(health) + print(armor) (lines 4-5)
        let result = extract_method(
            &temp.path().join("player.gd"),
            4,
            5,
            "show_stats",
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        assert_eq!(result.parameters.len(), 2);
        assert!(result.returns.is_none());
        let param_names: Vec<&str> = result.parameters.iter().map(|p| p.name.as_str()).collect();
        assert!(param_names.contains(&"health"));
        assert!(param_names.contains(&"armor"));
    }

    #[test]
    fn extract_with_return() {
        let temp = setup_project(&[(
            "player.gd",
            "func process():\n\tvar health = 100\n\thealth -= 10\n\tprint(health)\n",
        )]);
        // Extract `health -= 10` (line 3) — health is written and used after
        let result = extract_method(
            &temp.path().join("player.gd"),
            3,
            3,
            "take_damage",
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        assert_eq!(result.returns.as_deref(), Some("health"));
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(
            content.contains("health = take_damage(health)"),
            "call site should assign return value, got: {content}"
        );
        assert!(
            content.contains("return health"),
            "extracted function should return, got: {content}"
        );
    }

    #[test]
    fn extract_multiple_returns_dictionary() {
        let temp = setup_project(&[(
            "player.gd",
            "func process():\n\tvar a = 1\n\tvar b = 2\n\ta += 1\n\tb += 1\n\tprint(a)\n\tprint(b)\n",
        )]);
        // Extract lines 4-5: both a and b are written and used after → Dictionary return
        let result = extract_method(
            &temp.path().join("player.gd"),
            4,
            5,
            "update",
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        assert!(result.returns.is_none(), "single return should be None");
        assert_eq!(result.return_vars.len(), 2);
        assert!(result.return_vars.contains(&"a".to_string()));
        assert!(result.return_vars.contains(&"b".to_string()));

        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(
            content.contains("-> Dictionary:"),
            "should return Dictionary, got: {content}"
        );
        assert!(
            content.contains("var _result = update("),
            "should have result var, got: {content}"
        );
        assert!(
            content.contains("a = _result[\"a\"]"),
            "should destructure a, got: {content}"
        );
        assert!(
            content.contains("b = _result[\"b\"]"),
            "should destructure b, got: {content}"
        );
    }

    #[test]
    fn extract_three_return_vars() {
        let temp = setup_project(&[(
            "player.gd",
            "func process():\n\tvar a = 1\n\tvar b = 2\n\tvar c = 3\n\ta += 1\n\tb += 1\n\tc += 1\n\tprint(a)\n\tprint(b)\n\tprint(c)\n",
        )]);
        let result = extract_method(
            &temp.path().join("player.gd"),
            5,
            7,
            "update_all",
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        assert_eq!(result.return_vars.len(), 3);
    }

    #[test]
    fn extract_result_name_collision() {
        let temp = setup_project(&[(
            "player.gd",
            "func process():\n\tvar a = 1\n\tvar b = 2\n\tvar _result = 0\n\ta += 1\n\tb += 1\n\tprint(a)\n\tprint(b)\n",
        )]);
        let result = extract_method(
            &temp.path().join("player.gd"),
            5,
            6,
            "update",
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(
            content.contains("var _result2 = update("),
            "should avoid name collision, got: {content}"
        );
    }

    #[test]
    fn extract_contains_return_error() {
        let temp = setup_project(&[("player.gd", "func process():\n\tvar x = 1\n\treturn x\n")]);
        let result = extract_method(
            &temp.path().join("player.gd"),
            3,
            3,
            "helper",
            false,
            temp.path(),
        );
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("return"), "should error on return: {err}");
    }

    #[test]
    fn extract_outside_function_error() {
        let temp = setup_project(&[("player.gd", "var x = 1\nvar y = 2\n")]);
        let result = extract_method(
            &temp.path().join("player.gd"),
            1,
            1,
            "helper",
            false,
            temp.path(),
        );
        assert!(result.is_err());
    }

    #[test]
    fn extract_re_indentation() {
        let temp = setup_project(&[(
            "player.gd",
            "func process():\n\t\tvar deeply = 1\n\t\tprint(deeply)\n",
        )]);
        let result = extract_method(
            &temp.path().join("player.gd"),
            2,
            3,
            "helper",
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        // The extracted function body should be re-indented to 1 tab
        assert!(
            content.contains("\tvar deeply = 1"),
            "should re-indent to 1 tab, got: {content}"
        );
    }

    #[test]
    fn extract_type_hints() {
        let temp = setup_project(&[(
            "player.gd",
            "func process():\n\tvar health: int = 100\n\tprint(health)\n\tprint(\"end\")\n",
        )]);
        let result = extract_method(
            &temp.path().join("player.gd"),
            3,
            3,
            "show_health",
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        assert_eq!(result.parameters.len(), 1);
        assert_eq!(result.parameters[0].name, "health");
        assert_eq!(result.parameters[0].type_hint.as_deref(), Some("int"));
    }

    #[test]
    fn extract_dry_run() {
        let temp = setup_project(&[(
            "player.gd",
            "func _ready():\n\tprint(\"hello\")\n\tprint(\"world\")\n",
        )]);
        let result = extract_method(
            &temp.path().join("player.gd"),
            2,
            2,
            "greet",
            true, // dry run
            temp.path(),
        )
        .unwrap();
        assert!(!result.applied);
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(
            !content.contains("func greet"),
            "dry run should not modify file"
        );
    }

    // ── async detection ─────────────────────────────────────────────────

    #[test]
    fn extract_with_await_warns() {
        let temp = setup_project(&[(
            "player.gd",
            "func process():\n\tawait get_tree().create_timer(1.0).timeout\n\tprint(\"done\")\n",
        )]);
        let result = extract_method(
            &temp.path().join("player.gd"),
            2,
            2,
            "wait_a_bit",
            true,
            temp.path(),
        )
        .unwrap();
        assert!(!result.warnings.is_empty(), "should warn about await");
        assert!(result.warnings[0].contains("await"));
    }

    #[test]
    fn extract_without_await_no_warning() {
        let temp = setup_project(&[(
            "player.gd",
            "func process():\n\tprint(\"hello\")\n\tprint(\"world\")\n",
        )]);
        let result = extract_method(
            &temp.path().join("player.gd"),
            2,
            2,
            "greet",
            true,
            temp.path(),
        )
        .unwrap();
        assert!(result.warnings.is_empty(), "no await = no warning");
    }

    // ── re_indent ────────────────────────────────────────────────────────

    #[test]
    fn re_indent_strips_common_prefix() {
        let text = "\t\tvar x = 1\n\t\tprint(x)";
        let result = re_indent(text);
        assert_eq!(result, "\tvar x = 1\n\tprint(x)");
    }

    #[test]
    fn re_indent_single_line() {
        let text = "\tprint(42)";
        let result = re_indent(text);
        assert_eq!(result, "\tprint(42)");
    }

    // ── inline-method (Feature 6) ────────────────────────────────────────

    #[test]
    fn inline_void_function() {
        let temp = setup_project(&[(
            "player.gd",
            "func helper():\n\tprint(\"hello\")\n\n\nfunc _ready():\n\thelper()\n",
        )]);
        let result =
            inline_method(&temp.path().join("player.gd"), 6, 2, false, temp.path()).unwrap();
        assert!(result.applied);
        assert_eq!(result.function, "helper");
        assert!(result.function_deleted, "single callsite should delete");
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(
            content.contains("print(\"hello\")"),
            "should inline body, got: {content}"
        );
        assert!(
            !content.contains("func helper()"),
            "function should be deleted, got: {content}"
        );
    }

    #[test]
    fn inline_with_return() {
        let temp = setup_project(&[(
            "player.gd",
            "func double(x):\n\treturn x * 2\n\n\nfunc _ready():\n\tvar result = double(5)\n\tprint(result)\n",
        )]);
        let result = inline_method(
            &temp.path().join("player.gd"),
            6,
            16, // column of 'double' in '\tvar result = double(5)'
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(
            content.contains("var result = 5 * 2"),
            "should substitute params and inline return, got: {content}"
        );
    }

    #[test]
    fn inline_with_params() {
        let temp = setup_project(&[(
            "player.gd",
            "func greet(name):\n\tprint(name)\n\n\nfunc _ready():\n\tgreet(\"world\")\n",
        )]);
        let result =
            inline_method(&temp.path().join("player.gd"), 6, 2, false, temp.path()).unwrap();
        assert!(result.applied);
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(
            content.contains("print(\"world\")"),
            "should substitute param, got: {content}"
        );
    }

    #[test]
    fn inline_multiple_returns_error() {
        let temp = setup_project(&[(
            "player.gd",
            "func helper(x):\n\tif x > 0:\n\t\treturn 1\n\treturn 0\n\n\nfunc _ready():\n\thelper(1)\n",
        )]);
        let result = inline_method(&temp.path().join("player.gd"), 8, 2, false, temp.path());
        assert!(result.is_err());
    }

    #[test]
    fn inline_dry_run() {
        let temp = setup_project(&[(
            "player.gd",
            "func helper():\n\tprint(42)\n\n\nfunc _ready():\n\thelper()\n",
        )]);
        let result =
            inline_method(&temp.path().join("player.gd"), 6, 2, true, temp.path()).unwrap();
        assert!(!result.applied);
        assert!(!result.function_deleted);
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(
            content.contains("func helper()"),
            "dry run should not modify file"
        );
    }

    #[test]
    fn inline_multiple_callsites_keeps_function() {
        let temp = setup_project(&[(
            "player.gd",
            "func helper():\n\tprint(42)\n\n\nfunc _ready():\n\thelper()\n\thelper()\n",
        )]);
        let result =
            inline_method(&temp.path().join("player.gd"), 6, 2, false, temp.path()).unwrap();
        assert!(result.applied);
        assert!(
            !result.function_deleted,
            "multiple callsites should keep function"
        );
        assert!(
            !result.warnings.is_empty(),
            "should warn about remaining callsites"
        );
    }

    // ── inline-method by name ────────────────────────────────────────────

    #[test]
    fn inline_by_name_single_site() {
        let temp = setup_project(&[(
            "player.gd",
            "func helper():\n\tprint(42)\n\n\nfunc _ready():\n\thelper()\n",
        )]);
        let result = inline_method_by_name(
            &temp.path().join("player.gd"),
            "helper",
            false,
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        assert_eq!(result.call_sites_inlined, 1);
        assert!(result.function_deleted);
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(
            content.contains("print(42)"),
            "should inline body, got: {content}"
        );
        assert!(
            !content.contains("func helper()"),
            "function should be deleted, got: {content}"
        );
    }

    #[test]
    fn inline_by_name_all_sites() {
        let temp = setup_project(&[(
            "player.gd",
            "func helper():\n\tprint(42)\n\n\nfunc _ready():\n\thelper()\n\thelper()\n",
        )]);
        let result = inline_method_by_name(
            &temp.path().join("player.gd"),
            "helper",
            true,
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        assert_eq!(result.call_sites_inlined, 2);
        assert!(
            result.function_deleted,
            "all=true should delete function"
        );
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(
            !content.contains("func helper()"),
            "function should be deleted, got: {content}"
        );
    }

    #[test]
    fn inline_by_name_dry_run() {
        let temp = setup_project(&[(
            "player.gd",
            "func helper():\n\tprint(42)\n\n\nfunc _ready():\n\thelper()\n",
        )]);
        let result = inline_method_by_name(
            &temp.path().join("player.gd"),
            "helper",
            true,
            true,
            temp.path(),
        )
        .unwrap();
        assert!(!result.applied);
        assert!(!result.function_deleted);
        assert_eq!(result.call_sites_inlined, 1);
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(
            content.contains("func helper()"),
            "dry run should not modify file"
        );
    }

    #[test]
    fn inline_by_name_not_found() {
        let temp = setup_project(&[("player.gd", "func helper():\n\tpass\n")]);
        let result = inline_method_by_name(
            &temp.path().join("player.gd"),
            "nonexistent",
            false,
            false,
            temp.path(),
        );
        assert!(result.is_err());
    }

    // ── change-signature (Feature 7) ─────────────────────────────────────

    #[test]
    fn change_sig_add_param_with_default() {
        let temp = setup_project(&[(
            "player.gd",
            "func greet(name):\n\tprint(name)\n\n\nfunc _ready():\n\tgreet(\"world\")\n",
        )]);
        let result = change_signature(
            &temp.path().join("player.gd"),
            "greet",
            &["greeting: String = \"hello\"".to_string()],
            &[],
            &[],
            None,
            None,
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        assert!(
            result
                .new_signature
                .contains("greeting: String = \"hello\"")
        );
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(
            content.contains("greeting: String = \"hello\""),
            "should update definition, got: {content}"
        );
    }

    #[test]
    fn change_sig_remove_param() {
        let temp = setup_project(&[(
            "player.gd",
            "func greet(name, title):\n\tprint(name)\n\n\nfunc _ready():\n\tgreet(\"world\", \"mr\")\n",
        )]);
        let result = change_signature(
            &temp.path().join("player.gd"),
            "greet",
            &[],
            &["title".to_string()],
            &[],
            None,
            None,
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        assert!(!result.new_signature.contains("title"));
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(
            !content.contains("title"),
            "title should be removed, got: {content}"
        );
    }

    #[test]
    fn change_sig_reorder() {
        let temp = setup_project(&[(
            "player.gd",
            "func greet(a, b, c):\n\tprint(a)\n\n\nfunc _ready():\n\tgreet(1, 2, 3)\n",
        )]);
        let result = change_signature(
            &temp.path().join("player.gd"),
            "greet",
            &[],
            &[],
            &[],
            Some("c, a, b"),
            None,
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        assert_eq!(result.new_signature, "func greet(c, a, b)");
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(
            content.contains("func greet(c, a, b)"),
            "should reorder params, got: {content}"
        );
    }

    #[test]
    fn change_sig_remove_nonexistent_error() {
        let temp = setup_project(&[("player.gd", "func greet(name):\n\tpass\n")]);
        let result = change_signature(
            &temp.path().join("player.gd"),
            "greet",
            &[],
            &["nonexistent".to_string()],
            &[],
            None,
            None,
            false,
            temp.path(),
        );
        assert!(result.is_err());
    }

    #[test]
    fn change_sig_add_duplicate_error() {
        let temp = setup_project(&[("player.gd", "func greet(name):\n\tpass\n")]);
        let result = change_signature(
            &temp.path().join("player.gd"),
            "greet",
            &["name".to_string()],
            &[],
            &[],
            None,
            None,
            false,
            temp.path(),
        );
        assert!(result.is_err());
    }

    #[test]
    fn change_sig_dry_run() {
        let temp = setup_project(&[("player.gd", "func greet(name):\n\tpass\n")]);
        let result = change_signature(
            &temp.path().join("player.gd"),
            "greet",
            &["title".to_string()],
            &[],
            &[],
            None,
            None,
            true,
            temp.path(),
        )
        .unwrap();
        assert!(!result.applied);
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(!content.contains("title"), "dry run should not modify file");
    }

    #[test]
    fn change_sig_rename_param() {
        let temp = setup_project(&[(
            "player.gd",
            "func attack(victim_id):\n\tprint(victim_id)\n\n\nfunc _ready():\n\tattack(42)\n",
        )]);
        let result = change_signature(
            &temp.path().join("player.gd"),
            "attack",
            &[],
            &[],
            &["victim_id=target_id".to_string()],
            None,
            None,
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        assert!(
            result.new_signature.contains("target_id"),
            "signature should have new name, got: {}",
            result.new_signature
        );
        assert!(
            !result.new_signature.contains("victim_id"),
            "signature should not have old name"
        );
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(
            content.contains("func attack(target_id)"),
            "definition should be updated, got: {content}"
        );
        assert!(
            content.contains("print(target_id)"),
            "body usage should be renamed, got: {content}"
        );
        assert!(
            !content.contains("victim_id"),
            "old name should not appear, got: {content}"
        );
    }

    #[test]
    fn change_sig_rename_nonexistent_error() {
        let temp = setup_project(&[("player.gd", "func greet(name):\n\tpass\n")]);
        let result = change_signature(
            &temp.path().join("player.gd"),
            "greet",
            &[],
            &[],
            &["nonexistent=new_name".to_string()],
            None,
            None,
            false,
            temp.path(),
        );
        assert!(result.is_err());
    }
}
