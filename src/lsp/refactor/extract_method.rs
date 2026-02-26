use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::path::Path;

use miette::Result;
use tree_sitter::Node;

use super::{ExtractMethodOutput, ParameterOutput, line_starts, normalize_blank_lines};

// ── extract-method ──────────────────────────────────────────────────────────

#[derive(Clone)]
struct CapturedVar {
    name: String,
    type_hint: Option<String>,
    is_written: bool,
    is_used_after: bool,
    needs_var_declaration: bool, // true for vars declared inside extracted range
}

#[allow(clippy::too_many_lines)]
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
    let func = crate::lsp::references::enclosing_function(root, point)
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

    // Check for break/continue that escape the selection (their enclosing loop is outside)
    let selection_byte_range = (
        statements[0].start_byte(),
        statements.last().unwrap().end_byte(),
    );
    for stmt in &statements {
        if let Some(kind) = find_escaping_loop_control(*stmt, selection_byte_range) {
            return Err(miette::miette!(
                "cannot extract code containing '{kind}' — it would be invalid outside its loop"
            ));
        }
    }

    // Name collision detection
    let mut warnings = Vec::new();
    let scope_names = super::collision::collect_scope_names(root, &source, point);
    if let Some(kind) = super::collision::check_collision(name, &scope_names) {
        warnings.push(format!("'{name}' collides with a {kind}"));
    }

    // Async detection: warn if extracted code contains await
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

    // Find locally-declared variables that are used after the range — they must be returned
    let local_returns =
        find_local_return_vars(&local_decls, &statements, body, &source, extracted_range.1);
    return_vars.extend(local_returns);

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
    }

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
        super::validate_no_new_errors(&source, &new_source)?;
        std::fs::write(file, &new_source).map_err(|e| miette::miette!("cannot write file: {e}"))?;

        let mut snaps: std::collections::HashMap<std::path::PathBuf, Option<Vec<u8>>> =
            std::collections::HashMap::new();
        snaps.insert(file.to_path_buf(), Some(source.as_bytes().to_vec()));
        let stack = super::undo::UndoStack::open(project_root);
        let _ = stack.record(
            "extract-method",
            &format!("extract {name}"),
            &snaps,
            project_root,
        );
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
fn collect_statements_in_range(
    body: Node<'_>,
    start_line: usize,
    end_line: usize,
) -> Result<Vec<Node<'_>>> {
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

/// Find a `break` or `continue` whose enclosing loop is outside the given byte range.
/// Returns the keyword name (`"break"` / `"continue"`) on first hit, or `None`.
fn find_escaping_loop_control(node: Node, selection: (usize, usize)) -> Option<&'static str> {
    if matches!(node.kind(), "break_statement" | "continue_statement") {
        // Walk ancestors — if the nearest enclosing loop is outside the selection, it escapes
        let keyword = if node.kind() == "break_statement" {
            "break"
        } else {
            "continue"
        };
        let mut ancestor = node.parent();
        while let Some(a) = ancestor {
            if matches!(a.kind(), "for_statement" | "while_statement") {
                // Loop found — is it within the selection?
                if a.start_byte() >= selection.0 && a.end_byte() <= selection.1 {
                    return None; // enclosed loop is inside selection — safe
                }
                return Some(keyword); // loop is (partially) outside — escapes
            }
            ancestor = a.parent();
        }
        // No enclosing loop found at all (would be a syntax error in GDScript,
        // but still reject since extracting it would be invalid)
        return Some(keyword);
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if let Some(kind) = find_escaping_loop_control(child, selection) {
            return Some(kind);
        }
    }
    None
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
                                .map(std::string::ToString::to_string);
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
                .map(std::string::ToString::to_string);
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
                needs_var_declaration: false,
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

/// Find locally-declared variables (inside the extracted range) that are used after the range,
/// meaning they must be returned from the extracted function and re-declared at the call site.
fn find_local_return_vars(
    local_decls: &HashSet<String>,
    statements: &[Node],
    body: Node,
    source: &str,
    range_end: usize,
) -> Vec<CapturedVar> {
    let mut result = Vec::new();
    for var_name in local_decls {
        if !is_used_after_range(var_name, body, source, range_end) {
            continue;
        }
        // Extract type hint from the variable_statement node in the extracted range
        let type_hint = find_var_type_hint(var_name, statements, source);
        result.push(CapturedVar {
            name: var_name.clone(),
            type_hint,
            is_written: true,
            is_used_after: true,
            needs_var_declaration: true,
        });
    }
    result.sort_by(|a, b| a.name.cmp(&b.name));
    result
}

/// Find the type hint for a variable declaration within the given statements.
fn find_var_type_hint(name: &str, statements: &[Node], source: &str) -> Option<String> {
    for stmt in statements {
        if let Some(hint) = find_var_type_hint_recursive(*stmt, name, source) {
            return Some(hint);
        }
    }
    None
}

fn find_var_type_hint_recursive(node: Node, name: &str, source: &str) -> Option<String> {
    if node.kind() == "variable_statement"
        && let Some(name_node) = node.child_by_field_name("name")
        && name_node.utf8_text(source.as_bytes()).ok() == Some(name)
    {
        return node
            .child_by_field_name("type")
            .and_then(|t| t.utf8_text(source.as_bytes()).ok())
            .map(std::string::ToString::to_string);
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if let Some(hint) = find_var_type_hint_recursive(child, name, source) {
            return Some(hint);
        }
    }
    None
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

    // Extract body text from the statements (include first line's indentation
    // so re_indent sees consistent indentation across all lines)
    let first_line_start = source[..statements[0].start_byte()]
        .rfind('\n')
        .map_or(0, |pos| pos + 1);
    let last_byte = statements.last().unwrap().end_byte();
    let body_text = &source[first_line_start..last_byte];

    // Re-indent to 1 level
    let re_indented = re_indent(body_text);

    // Add return statement if needed
    let mut func_body = re_indented;
    if let Some(ret) = return_var {
        let _ = write!(func_body, "\n\treturn {}", ret.name);
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
        if ret.needs_var_declaration {
            format!("{indent}var {} = {name}({args})\n", ret.name)
        } else {
            format!("{indent}{} = {name}({args})\n", ret.name)
        }
    } else {
        format!("{indent}{name}({args})\n")
    }
}

/// Get the indentation string (tabs/spaces) of a line (0-based).
pub(super) fn get_indent(source: &str, line: usize) -> String {
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

    let first_line_start = source[..statements[0].start_byte()]
        .rfind('\n')
        .map_or(0, |pos| pos + 1);
    let last_byte = statements.last().unwrap().end_byte();
    let body_text = &source[first_line_start..last_byte];
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
        if v.needs_var_declaration {
            let _ = writeln!(
                lines,
                "{indent}var {} = {result_name}[\"{}\"]",
                v.name, v.name
            );
        } else {
            let _ = writeln!(lines, "{indent}{} = {result_name}[\"{}\"]", v.name, v.name);
        }
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

    // ── local var return detection (issue #23) ─────────────────────────

    #[test]
    fn extract_local_var_used_after_becomes_return() {
        // The core bug: var declared in range, used after → must be returned
        let temp = setup_project(&[(
            "player.gd",
            "func process(delta):\n\tvar velocity = speed * delta\n\tvelocity = clamp(velocity, 0, max_speed)\n\tposition += velocity\n",
        )]);
        // Extract lines 2-3: var velocity = ... ; velocity = clamp(...)
        let result = extract_method(
            &temp.path().join("player.gd"),
            2,
            3,
            "calculate_velocity",
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        assert_eq!(
            result.returns.as_deref(),
            Some("velocity"),
            "velocity should be a return value"
        );
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(
            content.contains("var velocity = calculate_velocity("),
            "call site should declare var, got:\n{content}"
        );
        assert!(
            content.contains("return velocity"),
            "extracted function should return velocity, got:\n{content}"
        );
    }

    #[test]
    fn extract_local_var_not_used_after_no_return() {
        // var declared and consumed entirely within range → no return needed
        let temp = setup_project(&[(
            "player.gd",
            "func process():\n\tvar tmp = 42\n\tprint(tmp)\n\tprint(\"done\")\n",
        )]);
        // Extract lines 2-3: var tmp = 42; print(tmp)
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
        assert!(
            result.returns.is_none(),
            "tmp not used after range → no return"
        );
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(
            !content.contains("var tmp = helper("),
            "should not have var declaration at call site, got:\n{content}"
        );
    }

    #[test]
    fn extract_multiple_local_return_vars() {
        // Two local vars used after → Dictionary return with var declarations
        let temp = setup_project(&[(
            "player.gd",
            "func process():\n\tvar a = 1\n\tvar b = 2\n\tprint(a)\n\tprint(b)\n",
        )]);
        // Extract lines 2-3: var a = 1; var b = 2
        let result = extract_method(
            &temp.path().join("player.gd"),
            2,
            3,
            "init_vars",
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        assert_eq!(result.return_vars.len(), 2);
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(
            content.contains("-> Dictionary:"),
            "should return Dictionary, got:\n{content}"
        );
        assert!(
            content.contains("var a = _result[\"a\"]"),
            "should declare var a at call site, got:\n{content}"
        );
        assert!(
            content.contains("var b = _result[\"b\"]"),
            "should declare var b at call site, got:\n{content}"
        );
    }

    #[test]
    fn extract_mix_preexisting_and_local_return_vars() {
        // pre-existing var (x) modified + local var (y) declared, both used after
        let temp = setup_project(&[(
            "player.gd",
            "func process():\n\tvar x = 1\n\tx += 10\n\tvar y = x * 2\n\tprint(x)\n\tprint(y)\n",
        )]);
        // Extract lines 3-4: x += 10; var y = x * 2
        let result = extract_method(
            &temp.path().join("player.gd"),
            3,
            4,
            "compute",
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        assert_eq!(result.return_vars.len(), 2);
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(
            content.contains("-> Dictionary:"),
            "should return Dictionary, got:\n{content}"
        );
        // x is pre-existing → plain assignment; y is local → var declaration
        assert!(
            content.contains("x = _result[\"x\"]"),
            "x should be plain assignment, got:\n{content}"
        );
        assert!(
            !content.contains("var x = _result[\"x\"]"),
            "x should NOT have var, got:\n{content}"
        );
        assert!(
            content.contains("var y = _result[\"y\"]"),
            "y should have var declaration, got:\n{content}"
        );
    }

    #[test]
    fn extract_local_var_with_type_hint_returned() {
        let temp = setup_project(&[(
            "player.gd",
            "func process():\n\tvar speed: float = 10.0\n\tprint(speed)\n",
        )]);
        // Extract line 2: var speed: float = 10.0
        let result = extract_method(
            &temp.path().join("player.gd"),
            2,
            2,
            "get_speed",
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        assert_eq!(result.returns.as_deref(), Some("speed"));
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(
            content.contains("-> float:"),
            "return type should be float, got:\n{content}"
        );
        assert!(
            content.contains("var speed = get_speed()"),
            "call site should have var declaration, got:\n{content}"
        );
    }

    // ── break/continue rejection ──────────────────────────────────────

    #[test]
    fn extract_break_outside_loop_error() {
        // break as a direct statement in function body (no enclosing loop in selection)
        let temp = setup_project(&[(
            "player.gd",
            "func process():\n\tvar x = 1\n\tbreak\n\tprint(x)\n",
        )]);
        // Extract lines 2-3: var x = 1; break — break has no enclosing loop
        let result = extract_method(
            &temp.path().join("player.gd"),
            2,
            3,
            "helper",
            true,
            temp.path(),
        );
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("break"),
            "should error on break: {err}"
        );
    }

    #[test]
    fn extract_continue_outside_loop_error() {
        // continue as a direct statement in function body (no enclosing loop in selection)
        let temp = setup_project(&[(
            "player.gd",
            "func process():\n\tvar x = 1\n\tcontinue\n\tprint(x)\n",
        )]);
        // Extract lines 2-3: var x = 1; continue — continue has no enclosing loop
        let result = extract_method(
            &temp.path().join("player.gd"),
            2,
            3,
            "helper",
            true,
            temp.path(),
        );
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("continue"),
            "should error on continue: {err}"
        );
    }

    #[test]
    fn extract_whole_loop_with_break_ok() {
        let temp = setup_project(&[(
            "player.gd",
            "func process():\n\tfor i in range(10):\n\t\tif i == 5:\n\t\t\tbreak\n\t\tprint(i)\n\tprint(\"done\")\n",
        )]);
        // Extract lines 2-5: the entire for-loop including break
        let result = extract_method(
            &temp.path().join("player.gd"),
            2,
            5,
            "loop_work",
            true,
            temp.path(),
        );
        assert!(
            result.is_ok(),
            "extracting entire loop with break should be allowed: {:?}",
            result.unwrap_err()
        );
    }

    #[test]
    fn extract_while_with_continue_ok() {
        let temp = setup_project(&[(
            "player.gd",
            "func process():\n\tvar i = 0\n\twhile i < 10:\n\t\ti += 1\n\t\tif i == 5:\n\t\t\tcontinue\n\t\tprint(i)\n\tprint(\"done\")\n",
        )]);
        // Extract lines 3-7: entire while-loop with continue inside
        let result = extract_method(
            &temp.path().join("player.gd"),
            3,
            7,
            "loop_work",
            true,
            temp.path(),
        );
        assert!(
            result.is_ok(),
            "extracting entire while-loop with continue should be allowed: {:?}",
            result.unwrap_err()
        );
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
}
