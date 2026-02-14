use std::collections::HashMap;
use std::fmt::Write;
use std::path::Path;

use miette::Result;
use serde::Serialize;
use tree_sitter::Node;

use super::{declaration_full_range, find_declaration_by_name, line_starts, normalize_blank_lines};

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

#[allow(clippy::too_many_lines)]
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
    let body_stmts: Vec<Node> = {
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
    if return_count == 1
        && body_stmts.last().map(tree_sitter::Node::kind) != Some("return_statement")
    {
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
            .map(std::string::String::as_str)
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
    let is_assignment = call_parent.is_some_and(|p| {
        matches!(
            p.kind(),
            "assignment" | "augmented_assignment" | "variable_statement"
        )
    });

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
                    let _ = writeln!(
                        inlined_lines_text,
                        "{call_indent}var {var_name} = {ret_expr}"
                    );
                } else {
                    // x = func() → body + x = expr
                    let left = parent
                        .child_by_field_name("left")
                        .and_then(|n| n.utf8_text(source.as_bytes()).ok())
                        .unwrap_or("x");
                    let _ = writeln!(inlined_lines_text, "{call_indent}{left} = {ret_expr}");
                }
            }
        } else {
            // Standalone call with return → just add the expression (discard return value)
            if inlined_text.is_empty() {
                let _ = writeln!(inlined_lines_text, "{call_indent}{ret_expr}");
            }
            // else: Body already added above; the return value is discarded
        }
    } else if inlined_text.is_empty() {
        // Void function, single `pass` → remove the call line entirely
        let _ = writeln!(inlined_lines_text, "{call_indent}pass");
    }

    let total_inlined = inlined_lines_text.lines().count() as u32;

    // Count references to decide if we can delete the function
    let workspace = crate::lsp::workspace::WorkspaceIndex::new(project_root.to_path_buf());
    let all_refs =
        crate::lsp::references::find_references_by_name(func_name, &workspace, None, None);
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
    collect_calls_to(
        root,
        name,
        &source,
        func_def_start,
        func_def_end,
        &mut call_sites,
    );

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

    if dry_run {
        inlined_count = sites_to_inline.len() as u32;
    } else {
        // Inline call sites from bottom to top to preserve line numbers
        let mut sorted_sites = sites_to_inline.clone();
        sorted_sites.sort_by(|a, b| b.0.cmp(&a.0));

        for (line, column) in &sorted_sites {
            // Re-read and re-parse after each inline (source changes)
            match inline_method(file, *line, *column, false, project_root) {
                Ok(_) => inlined_count += 1,
                Err(e) => warnings.push(format!("failed to inline at {line}:{column}: {e}")),
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
    }

    // Check if function was deleted (either by inline_method for single callsite,
    // or by our explicit deletion above for --all)
    let function_deleted = if !dry_run && inlined_count > 0 {
        let current_source =
            std::fs::read_to_string(file).map_err(|e| miette::miette!("cannot read file: {e}"))?;
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

// ── Private helpers ─────────────────────────────────────────────────────────

/// Collect all call sites of `func_name` in the AST, excluding those within
/// the function definition itself.
fn collect_calls_to(
    node: Node,
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
fn count_return_statements(node: Node, count: &mut usize) {
    if node.kind() == "return_statement" {
        *count += 1;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        count_return_statements(child, count);
    }
}

/// Find a `call` node that contains or starts at the given point.
pub(super) fn find_call_at(root: Node<'_>, point: tree_sitter::Point) -> Option<Node<'_>> {
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
pub(super) fn extract_call_arguments(call: Node, source: &str) -> Vec<String> {
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
pub(super) struct ParamInfo {
    pub(super) name: String,
    #[allow(dead_code)]
    pub(super) type_hint: Option<String>,
    pub(super) default: Option<String>,
}

/// Extract function parameter info from a function definition.
pub(super) fn extract_function_params(func: Node, source: &str) -> Vec<ParamInfo> {
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
                            .map(std::string::ToString::to_string);
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
                            .map(std::string::ToString::to_string);
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
                            .map(std::string::ToString::to_string);
                        let default = child
                            .child_by_field_name("value")
                            .and_then(|v| v.utf8_text(source.as_bytes()).ok())
                            .map(std::string::ToString::to_string);
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
    stmts: &[Node],
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
    node: Node,
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
fn contains_call_to(node: Node, name: &str, source: &str) -> bool {
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

// ── Self-reference analysis ─────────────────────────────────────────────────

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

// ── Indent helper ───────────────────────────────────────────────────────────

/// Get the indentation string (tabs/spaces) of a line (0-based).
fn get_indent(source: &str, line: usize) -> String {
    let line_text = source.lines().nth(line).unwrap_or("");
    let indent_len = line_text.len() - line_text.trim_start().len();
    line_text[..indent_len].to_string()
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

    // ── inline-method ────────────────────────────────────────────────────

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
        assert!(result.function_deleted, "all=true should delete function");
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
}
