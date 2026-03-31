use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::path::Path;

use miette::Result;
use tree_sitter::Node;

use super::{ExtractMethodOutput, ParameterOutput, normalize_blank_lines};
use gd_core::ast_owned::{
    OwnedDecl, OwnedExpr, OwnedFile, OwnedFunc, OwnedParam, OwnedStmt, OwnedTypeRef, OwnedVar,
};
use gd_core::gd_ast::GdFile;
use gd_core::printer::print_file;
use gd_core::type_inference::{InferredType, infer_expression_type};

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
    project_root: &Path,
) -> Result<ExtractMethodOutput> {
    let source =
        std::fs::read_to_string(file).map_err(|e| miette::miette!("cannot read file: {e}"))?;
    let tree = gd_core::parser::parse(&source)?;
    let root = tree.root_node();
    let gd_file = gd_core::gd_ast::convert(&tree, &source);

    let start_line_0 = start_line - 1;
    let end_line_0 = end_line - 1;

    // Find enclosing function
    let point = tree_sitter::Point::new(start_line_0, 0);
    let func = crate::references::enclosing_function(root, point)
        .ok_or_else(|| miette::miette!("no enclosing function at line {start_line}"))?;

    let body = func
        .child_by_field_name("body")
        .ok_or_else(|| miette::miette!("function has no body"))?;

    // Detect static modifier on the enclosing function
    let is_static = has_static_keyword(&func);

    // Detect whether the function is inside an inner class
    let inner_class = find_enclosing_class(&func);

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
    let scope_names = super::collision::collect_scope_names(root, &source, point, &gd_file);
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
        &gd_file,
    );

    // Separate params and return vars
    let mut return_vars: Vec<CapturedVar> = Vec::new();
    for cap in &captured {
        if cap.is_written && cap.is_used_after {
            return_vars.push(cap.clone());
        }
    }

    // Find locally-declared variables that are used after the range — they must be returned
    let local_returns = find_local_return_vars(
        &local_decls,
        &statements,
        body,
        &source,
        extracted_range.1,
        &gd_file,
    );
    return_vars.extend(local_returns);

    // All captured vars are parameters
    let params: Vec<&CapturedVar> = captured.iter().collect();

    // ── Output metadata ──────────────────────────────────────────
    let original_indent = get_indent(&source, start_line_0);
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
    let static_prefix = if is_static { "static " } else { "" };

    let (func_signature, call_site_line, returns_field, return_vars_field, result_name);
    if return_vars.len() >= 2 {
        func_signature = format!("{static_prefix}func {name}({param_str}) -> Dictionary:");
        result_name = pick_result_name(&source, body);
        call_site_line = generate_call_site_multi_return(
            name,
            &params,
            &return_vars,
            &original_indent,
            &result_name,
        );
        returns_field = None;
        return_vars_field = return_vars.iter().map(|v| v.name.clone()).collect();
    } else {
        result_name = String::new();
        let return_var = return_vars.first();
        let return_type_str = return_var
            .and_then(|v| v.type_hint.as_ref())
            .map(|t| format!(" -> {t}"))
            .unwrap_or_default();
        func_signature = format!("{static_prefix}func {name}({param_str}){return_type_str}:");
        call_site_line = generate_call_site(name, &params, return_var, &original_indent);
        returns_field = return_var.map(|v| v.name.clone());
        return_vars_field = Vec::new();
    }

    let relative_file = gd_core::fs::relative_slash(file, project_root);

    // ── Typed AST mutation ───────────────────────────────────────
    let mut owned_file = OwnedFile::from_borrowed(&gd_file);
    owned_file.span = None;

    let func_name_str = func
        .child_by_field_name("name")
        .and_then(|n| n.utf8_text(source.as_bytes()).ok())
        .unwrap_or("_init")
        .to_string();
    let first_stmt_byte = statements[0].start_byte();
    let last_stmt_byte = statements.last().unwrap().end_byte();

    // Build owned parameters for the new function
    let owned_params: Vec<OwnedParam> = params
        .iter()
        .map(|p| OwnedParam {
            span: None,
            name: p.name.clone(),
            type_ann: p.type_hint.as_ref().map(|t| OwnedTypeRef {
                span: None,
                name: t.clone(),
                is_inferred: false,
            }),
            default: None,
        })
        .collect();

    // Build return type
    let return_type_owned = if return_vars.len() >= 2 {
        Some(OwnedTypeRef {
            span: None,
            name: "Dictionary".to_string(),
            is_inferred: false,
        })
    } else {
        return_vars
            .first()
            .and_then(|v| v.type_hint.as_ref())
            .map(|t| OwnedTypeRef {
                span: None,
                name: t.clone(),
                is_inferred: false,
            })
    };

    // Build call expression
    let call_args: Vec<OwnedExpr> = params
        .iter()
        .map(|p| OwnedExpr::Ident {
            span: None,
            name: p.name.clone(),
        })
        .collect();
    let call_expr = OwnedExpr::Call {
        span: None,
        callee: Box::new(OwnedExpr::Ident {
            span: None,
            name: name.to_string(),
        }),
        args: call_args,
    };

    // Build call-site and return statements from owned types
    let call_stmts = build_call_stmts(call_expr, &return_vars, &result_name);
    let return_stmt_owned = build_return_stmt(&return_vars);

    // Apply mutation to the owned AST
    let inner_class_name = inner_class.as_ref().and_then(|cn| {
        cn.child_by_field_name("name")
            .and_then(|n| n.utf8_text(source.as_bytes()).ok())
            .map(String::from)
    });
    apply_extract_mutation(
        &mut owned_file,
        inner_class_name.as_deref(),
        &func_name_str,
        first_stmt_byte,
        last_stmt_byte,
        name,
        owned_params,
        return_type_owned,
        return_stmt_owned,
        call_stmts,
        is_static,
    )?;

    // Print and commit
    let mut new_source = print_file(&owned_file, &source);
    normalize_blank_lines(&mut new_source);

    let mut ms = super::mutation::MutationSet::new();
    ms.insert(file.to_path_buf(), new_source);
    super::mutation::commit(&ms, project_root)?;

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
        applied: true,
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

/// Convert an `InferredType` to a type annotation string, returning `None` for
/// `Void`, `Variant`, or types that would not be useful as annotations.
fn inferred_type_to_hint(ty: &InferredType) -> Option<String> {
    match ty {
        InferredType::Void | InferredType::Variant => None,
        _ => Some(ty.display_name()),
    }
}

/// Try to infer a type for a local `variable_statement` node by checking its
/// explicit type annotation first, then falling back to expression inference on
/// its initialiser.
fn infer_var_type(node: Node, source: &str, file: &GdFile) -> Option<String> {
    // 1. Explicit annotation (`: int`, `: Vector2`, etc.)
    if let Some(type_node) = node.child_by_field_name("type")
        && type_node.kind() != "inferred_type"
        && let Ok(text) = type_node.utf8_text(source.as_bytes())
        && !text.is_empty()
    {
        return Some(text.to_string());
    }

    // 2. Infer from initialiser
    if let Some(value) = node.child_by_field_name("value")
        && let Some(ty) = infer_expression_type(&value, source, file)
    {
        return inferred_type_to_hint(&ty);
    }

    None
}

/// Find captured variables: identifiers in the range that are declared before the range
/// in the enclosing function (as parameters or local vars).
#[allow(clippy::too_many_arguments)]
fn find_captured_variables(
    func: &Node,
    body: Node,
    source: &str,
    range_idents: &HashSet<String>,
    local_decls: &HashSet<String>,
    statements: &[Node],
    extracted_range: (usize, usize),
    file: &GdFile,
) -> Vec<CapturedVar> {
    // Collect function parameters with optional type hints
    let mut pre_decls: HashMap<String, Option<String>> = HashMap::new();

    // Look up the enclosing function in the typed AST for parameter types
    let func_name = func
        .child_by_field_name("name")
        .and_then(|n| n.utf8_text(source.as_bytes()).ok())
        .unwrap_or("");
    let func_decl = file.find_func(func_name);

    if let Some(params) = func.child_by_field_name("parameters") {
        let mut cursor = params.walk();
        if cursor.goto_first_child() {
            loop {
                let child = cursor.node();
                match child.kind() {
                    "identifier" => {
                        let name = source[child.byte_range()].to_string();
                        // Try to get the type from the typed AST's GdFunc
                        let type_hint = func_decl
                            .and_then(|fd| fd.params.iter().find(|p| p.name == name))
                            .and_then(|p| p.type_ann.as_ref())
                            .filter(|ann| !ann.is_inferred && !ann.name.is_empty())
                            .map(|ann| ann.name.to_string());
                        pre_decls.insert(name, type_hint);
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
                            // No explicit type, but try inference from default value
                            let type_hint = child
                                .child_by_field_name("value")
                                .and_then(|v| infer_expression_type(&v, source, file))
                                .and_then(|ty| inferred_type_to_hint(&ty));
                            pre_decls.insert(name, type_hint);
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
            let type_hint = infer_var_type(child, source, file);
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
    file: &GdFile,
) -> Vec<CapturedVar> {
    let mut result = Vec::new();
    for var_name in local_decls {
        if !is_used_after_range(var_name, body, source, range_end) {
            continue;
        }
        // Extract type hint from the variable_statement node in the extracted range,
        // falling back to type inference on the initialiser
        let type_hint = find_var_type_hint_inferred(var_name, statements, source, file);
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

/// Find the type for a variable declaration within the given statements,
/// using explicit annotation first, then falling back to type inference.
fn find_var_type_hint_inferred(
    name: &str,
    statements: &[Node],
    source: &str,
    file: &GdFile,
) -> Option<String> {
    for stmt in statements {
        if let Some(hint) = find_var_type_hint_inferred_recursive(*stmt, name, source, file) {
            return Some(hint);
        }
    }
    None
}

fn find_var_type_hint_inferred_recursive(
    node: Node,
    name: &str,
    source: &str,
    file: &GdFile,
) -> Option<String> {
    if node.kind() == "variable_statement"
        && let Some(name_node) = node.child_by_field_name("name")
        && name_node.utf8_text(source.as_bytes()).ok() == Some(name)
    {
        return infer_var_type(node, source, file);
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if let Some(hint) = find_var_type_hint_inferred_recursive(child, name, source, file) {
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

fn has_static_keyword(func_node: &Node) -> bool {
    let mut cursor = func_node.walk();
    for child in func_node.children(&mut cursor) {
        if child.kind() == "static_keyword" {
            return true;
        }
    }
    false
}

/// Walk up from a function node to find an enclosing `class_definition` (inner class).
/// Returns `None` if the function is at the top level (parent is the root/source_file).
fn find_enclosing_class<'a>(func: &Node<'a>) -> Option<Node<'a>> {
    let mut node = func.parent()?;
    loop {
        if node.kind() == "class_definition" {
            return Some(node);
        }
        node = node.parent()?;
    }
}

// ── Typed AST helpers ──────────────────────────────────────────────────

/// Recursively clear all statement-level spans so the printer uses
/// structural output with correct indentation.
fn clear_stmt_spans(stmt: &mut OwnedStmt) {
    match stmt {
        OwnedStmt::Var(v) => v.span = None,
        OwnedStmt::If(i) => {
            i.span = None;
            for s in &mut i.body {
                clear_stmt_spans(s);
            }
            for (_, body) in &mut i.elif_branches {
                for s in body {
                    clear_stmt_spans(s);
                }
            }
            if let Some(body) = &mut i.else_body {
                for s in body {
                    clear_stmt_spans(s);
                }
            }
        }
        OwnedStmt::For { span, body, .. } | OwnedStmt::While { span, body, .. } => {
            *span = None;
            for s in body {
                clear_stmt_spans(s);
            }
        }
        OwnedStmt::Match { span, arms, .. } => {
            *span = None;
            for arm in arms {
                arm.span = None;
                for s in &mut arm.body {
                    clear_stmt_spans(s);
                }
            }
        }
        OwnedStmt::Expr { span, .. }
        | OwnedStmt::Assign { span, .. }
        | OwnedStmt::AugAssign { span, .. }
        | OwnedStmt::Return { span, .. }
        | OwnedStmt::Pass { span }
        | OwnedStmt::Break { span }
        | OwnedStmt::Continue { span }
        | OwnedStmt::Breakpoint { span }
        | OwnedStmt::Invalid { span } => *span = None,
    }
}

/// Build call-site statements from owned AST types.
fn build_call_stmts(
    call_expr: OwnedExpr,
    return_vars: &[CapturedVar],
    result_name: &str,
) -> Vec<OwnedStmt> {
    if return_vars.len() >= 2 {
        let mut stmts = vec![OwnedStmt::Var(OwnedVar {
            span: None,
            name: result_name.to_string(),
            type_ann: None,
            value: Some(call_expr),
            is_const: false,
            is_static: false,
            annotations: vec![],
            setter: None,
            getter: None,
            doc: None,
        })];
        for v in return_vars {
            let subscript = OwnedExpr::Subscript {
                span: None,
                receiver: Box::new(OwnedExpr::Ident {
                    span: None,
                    name: result_name.to_string(),
                }),
                index: Box::new(OwnedExpr::StringLiteral {
                    span: None,
                    value: format!("\"{}\"", v.name),
                }),
            };
            if v.needs_var_declaration {
                stmts.push(OwnedStmt::Var(OwnedVar {
                    span: None,
                    name: v.name.clone(),
                    type_ann: None,
                    value: Some(subscript),
                    is_const: false,
                    is_static: false,
                    annotations: vec![],
                    setter: None,
                    getter: None,
                    doc: None,
                }));
            } else {
                stmts.push(OwnedStmt::Assign {
                    span: None,
                    target: OwnedExpr::Ident {
                        span: None,
                        name: v.name.clone(),
                    },
                    value: subscript,
                });
            }
        }
        stmts
    } else if let Some(ret) = return_vars.first() {
        if ret.needs_var_declaration {
            vec![OwnedStmt::Var(OwnedVar {
                span: None,
                name: ret.name.clone(),
                type_ann: None,
                value: Some(call_expr),
                is_const: false,
                is_static: false,
                annotations: vec![],
                setter: None,
                getter: None,
                doc: None,
            })]
        } else {
            vec![OwnedStmt::Assign {
                span: None,
                target: OwnedExpr::Ident {
                    span: None,
                    name: ret.name.clone(),
                },
                value: call_expr,
            }]
        }
    } else {
        vec![OwnedStmt::Expr {
            span: None,
            expr: call_expr,
        }]
    }
}

/// Build the return statement for the extracted function body.
fn build_return_stmt(return_vars: &[CapturedVar]) -> Option<OwnedStmt> {
    if return_vars.len() >= 2 {
        let dict_pairs: Vec<(OwnedExpr, OwnedExpr)> = return_vars
            .iter()
            .map(|v| {
                (
                    OwnedExpr::StringLiteral {
                        span: None,
                        value: format!("\"{}\"", v.name),
                    },
                    OwnedExpr::Ident {
                        span: None,
                        name: v.name.clone(),
                    },
                )
            })
            .collect();
        Some(OwnedStmt::Return {
            span: None,
            value: Some(OwnedExpr::Dict {
                span: None,
                pairs: dict_pairs,
            }),
        })
    } else {
        return_vars.first().map(|ret| OwnedStmt::Return {
            span: None,
            value: Some(OwnedExpr::Ident {
                span: None,
                name: ret.name.clone(),
            }),
        })
    }
}

/// Apply the extract-method mutation to an owned AST.
#[allow(clippy::too_many_arguments)]
fn apply_extract_mutation(
    file: &mut OwnedFile,
    inner_class_name: Option<&str>,
    func_name: &str,
    first_stmt_byte: usize,
    last_stmt_byte: usize,
    new_func_name: &str,
    owned_params: Vec<OwnedParam>,
    return_type: Option<OwnedTypeRef>,
    return_stmt: Option<OwnedStmt>,
    call_stmts: Vec<OwnedStmt>,
    is_static: bool,
) -> Result<()> {
    file.span = None;
    if let Some(class_name) = inner_class_name {
        let class_idx = file
            .declarations
            .iter()
            .position(|d| matches!(d, OwnedDecl::Class(c) if c.name == class_name))
            .ok_or_else(|| miette::miette!("inner class '{class_name}' not found"))?;
        if let OwnedDecl::Class(c) = &mut file.declarations[class_idx] {
            c.span = None;
            mutate_declarations(
                &mut c.declarations,
                func_name,
                first_stmt_byte,
                last_stmt_byte,
                new_func_name,
                owned_params,
                return_type,
                return_stmt,
                call_stmts,
                is_static,
            )?;
        }
    } else {
        mutate_declarations(
            &mut file.declarations,
            func_name,
            first_stmt_byte,
            last_stmt_byte,
            new_func_name,
            owned_params,
            return_type,
            return_stmt,
            call_stmts,
            is_static,
        )?;
    }
    Ok(())
}

/// Mutate a declaration list: drain selected statements from the enclosing
/// function, insert call-site statements, and add the new extracted function.
#[allow(clippy::too_many_arguments)]
fn mutate_declarations(
    decls: &mut Vec<OwnedDecl>,
    func_name: &str,
    first_stmt_byte: usize,
    last_stmt_byte: usize,
    new_func_name: &str,
    owned_params: Vec<OwnedParam>,
    return_type: Option<OwnedTypeRef>,
    return_stmt: Option<OwnedStmt>,
    call_stmts: Vec<OwnedStmt>,
    is_static: bool,
) -> Result<()> {
    let func_idx = decls
        .iter()
        .position(|d| matches!(d, OwnedDecl::Func(f) if f.name == func_name))
        .ok_or_else(|| miette::miette!("function '{func_name}' not found in owned AST"))?;

    // Phase 1: find indices and drain selected statements
    let (first_idx, mut extracted_body) = {
        let OwnedDecl::Func(enclosing) = &mut decls[func_idx] else {
            unreachable!()
        };
        enclosing.span = None;

        let first_idx = enclosing
            .body
            .iter()
            .position(|s| s.span().is_some_and(|sp| sp.start == first_stmt_byte))
            .ok_or_else(|| miette::miette!("first selected statement not found in owned AST"))?;
        let last_idx = enclosing
            .body
            .iter()
            .rposition(|s| s.span().is_some_and(|sp| sp.end == last_stmt_byte))
            .ok_or_else(|| miette::miette!("last selected statement not found in owned AST"))?;

        let stmts: Vec<OwnedStmt> = enclosing.body.drain(first_idx..=last_idx).collect();
        // Clear spans in remaining body statements for correct re-indentation
        for stmt in &mut enclosing.body {
            clear_stmt_spans(stmt);
        }
        (first_idx, stmts)
    };

    // Phase 2: clear spans in extracted statements and build new function
    for stmt in &mut extracted_body {
        clear_stmt_spans(stmt);
    }
    if let Some(ret) = return_stmt {
        extracted_body.push(ret);
    }

    let new_func = OwnedFunc {
        span: None,
        name: new_func_name.to_string(),
        params: owned_params,
        return_type,
        body: extracted_body,
        is_static,
        is_constructor: false,
        annotations: vec![],
        doc: None,
    };

    // Phase 3: insert call-site statements at the drain position
    {
        let OwnedDecl::Func(enclosing) = &mut decls[func_idx] else {
            unreachable!()
        };
        for (i, stmt) in call_stmts.into_iter().enumerate() {
            enclosing.body.insert(first_idx + i, stmt);
        }
    }

    // Phase 4: insert new function before the enclosing function
    decls.insert(func_idx, OwnedDecl::Func(new_func));
    Ok(())
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
        let result =
            extract_method(&temp.path().join("player.gd"), 4, 5, "update", temp.path()).unwrap();
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
        let result =
            extract_method(&temp.path().join("player.gd"), 5, 6, "update", temp.path()).unwrap();
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
        let result = extract_method(&temp.path().join("player.gd"), 3, 3, "helper", temp.path());
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("return"), "should error on return: {err}");
    }

    #[test]
    fn extract_outside_function_error() {
        let temp = setup_project(&[("player.gd", "var x = 1\nvar y = 2\n")]);
        let result = extract_method(&temp.path().join("player.gd"), 1, 1, "helper", temp.path());
        assert!(result.is_err());
    }

    #[test]
    fn extract_re_indentation() {
        let temp = setup_project(&[(
            "player.gd",
            "func process():\n\t\tvar deeply = 1\n\t\tprint(deeply)\n",
        )]);
        let result =
            extract_method(&temp.path().join("player.gd"), 2, 3, "helper", temp.path()).unwrap();
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
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        assert_eq!(result.parameters.len(), 1);
        assert_eq!(result.parameters[0].name, "health");
        assert_eq!(result.parameters[0].type_hint.as_deref(), Some("int"));
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
        let result =
            extract_method(&temp.path().join("player.gd"), 2, 2, "greet", temp.path()).unwrap();
        assert!(result.warnings.is_empty(), "no await = no warning");
    }

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
        let result =
            extract_method(&temp.path().join("player.gd"), 2, 3, "helper", temp.path()).unwrap();
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
        let result =
            extract_method(&temp.path().join("player.gd"), 3, 4, "compute", temp.path()).unwrap();
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
        let result = extract_method(&temp.path().join("player.gd"), 2, 3, "helper", temp.path());
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("break"), "should error on break: {err}");
    }

    #[test]
    fn extract_continue_outside_loop_error() {
        // continue as a direct statement in function body (no enclosing loop in selection)
        let temp = setup_project(&[(
            "player.gd",
            "func process():\n\tvar x = 1\n\tcontinue\n\tprint(x)\n",
        )]);
        // Extract lines 2-3: var x = 1; continue — continue has no enclosing loop
        let result = extract_method(&temp.path().join("player.gd"), 2, 3, "helper", temp.path());
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("continue"), "should error on continue: {err}");
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
            temp.path(),
        );
        assert!(
            result.is_ok(),
            "extracting entire while-loop with continue should be allowed: {:?}",
            result.unwrap_err()
        );
    }

    // ── type inference tests ───────────────────────────────────────────

    #[test]
    fn extract_infers_param_types_from_initializers() {
        // var speed = 10.0 → float, var count = 5 → int
        let temp = setup_project(&[(
            "player.gd",
            "func process():\n\tvar speed = 10.0\n\tvar count = 5\n\tprint(speed + count)\n\tprint(\"end\")\n",
        )]);
        let result =
            extract_method(&temp.path().join("player.gd"), 4, 4, "show", temp.path()).unwrap();
        assert_eq!(result.parameters.len(), 2);
        let count_p = result
            .parameters
            .iter()
            .find(|p| p.name == "count")
            .unwrap();
        assert_eq!(
            count_p.type_hint.as_deref(),
            Some("int"),
            "count should be int"
        );
        let speed_p = result
            .parameters
            .iter()
            .find(|p| p.name == "speed")
            .unwrap();
        assert_eq!(
            speed_p.type_hint.as_deref(),
            Some("float"),
            "speed should be float"
        );
        // Verify the generated function signature includes types
        assert!(
            result.function.contains("count: int"),
            "signature should include count: int, got: {}",
            result.function
        );
        assert!(
            result.function.contains("speed: float"),
            "signature should include speed: float, got: {}",
            result.function
        );
    }

    #[test]
    fn extract_infers_return_type_from_initializer() {
        // var velocity = Vector2(1, 2) → Vector2 return type
        let temp = setup_project(&[(
            "player.gd",
            "func process():\n\tvar velocity = Vector2(1, 2)\n\tprint(velocity)\n",
        )]);
        let result = extract_method(
            &temp.path().join("player.gd"),
            2,
            2,
            "get_velocity",
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        assert_eq!(result.returns.as_deref(), Some("velocity"));
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(
            content.contains("-> Vector2:"),
            "return type should be Vector2, got:\n{content}"
        );
    }

    #[test]
    fn extract_infers_param_type_from_function_signature() {
        // Function parameter with explicit type: delta: float
        let temp = setup_project(&[(
            "player.gd",
            "func process(delta: float):\n\tvar x = delta * 2\n\tprint(\"end\")\n",
        )]);
        let result =
            extract_method(&temp.path().join("player.gd"), 2, 2, "compute", temp.path()).unwrap();
        assert_eq!(result.parameters.len(), 1);
        assert_eq!(result.parameters[0].name, "delta");
        assert_eq!(
            result.parameters[0].type_hint.as_deref(),
            Some("float"),
            "delta should have type float from function signature"
        );
        assert!(
            result.function.contains("delta: float"),
            "signature should include delta: float, got: {}",
            result.function
        );
    }

    #[test]
    fn extract_untyped_stays_untyped() {
        // var data = some_func() — can't infer type → stays untyped
        let temp = setup_project(&[(
            "player.gd",
            "func process():\n\tvar data = unknown_func()\n\tprint(data)\n\tprint(\"end\")\n",
        )]);
        let result = extract_method(
            &temp.path().join("player.gd"),
            3,
            3,
            "show_data",
            temp.path(),
        )
        .unwrap();
        assert_eq!(result.parameters.len(), 1);
        assert_eq!(result.parameters[0].name, "data");
        assert!(
            result.parameters[0].type_hint.is_none(),
            "unknown type should stay untyped, got: {:?}",
            result.parameters[0].type_hint
        );
        assert!(
            !result.function.contains("data:"),
            "signature should not have type for data, got: {}",
            result.function
        );
    }

    #[test]
    fn extract_static_with_typed_params() {
        // Static function with typed params and inferred return
        let temp = setup_project(&[(
            "player.gd",
            "static func calculate(x: int, y: int):\n\tvar sum = x + y\n\tprint(sum)\n",
        )]);
        let result =
            extract_method(&temp.path().join("player.gd"), 2, 2, "add", temp.path()).unwrap();
        assert!(result.applied);
        assert_eq!(result.parameters.len(), 2);
        let x_p = result.parameters.iter().find(|p| p.name == "x").unwrap();
        assert_eq!(x_p.type_hint.as_deref(), Some("int"));
        let y_p = result.parameters.iter().find(|p| p.name == "y").unwrap();
        assert_eq!(y_p.type_hint.as_deref(), Some("int"));
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(
            content.contains("func add(x: int, y: int)"),
            "should have typed params, got:\n{content}"
        );
    }

    #[test]
    fn extract_infers_string_type() {
        let temp = setup_project(&[(
            "player.gd",
            "func process():\n\tvar msg = \"hello\"\n\tprint(msg)\n\tprint(\"end\")\n",
        )]);
        let result = extract_method(
            &temp.path().join("player.gd"),
            3,
            3,
            "show_msg",
            temp.path(),
        )
        .unwrap();
        assert_eq!(result.parameters.len(), 1);
        assert_eq!(result.parameters[0].name, "msg");
        assert_eq!(
            result.parameters[0].type_hint.as_deref(),
            Some("String"),
            "string literal should infer as String"
        );
    }

    #[test]
    fn extract_infers_bool_type() {
        let temp = setup_project(&[(
            "player.gd",
            "func process():\n\tvar flag = true\n\tprint(flag)\n\tprint(\"end\")\n",
        )]);
        let result = extract_method(
            &temp.path().join("player.gd"),
            3,
            3,
            "show_flag",
            temp.path(),
        )
        .unwrap();
        assert_eq!(result.parameters.len(), 1);
        assert_eq!(result.parameters[0].name, "flag");
        assert_eq!(
            result.parameters[0].type_hint.as_deref(),
            Some("bool"),
            "true literal should infer as bool"
        );
    }

    #[test]
    fn extract_mixed_typed_and_untyped_params() {
        // One param has a type (from annotation), one doesn't (unknown function)
        let temp = setup_project(&[(
            "player.gd",
            "func process():\n\tvar typed: int = 5\n\tvar untyped = unknown()\n\tprint(typed + untyped)\n\tprint(\"end\")\n",
        )]);
        let result =
            extract_method(&temp.path().join("player.gd"), 4, 4, "compute", temp.path()).unwrap();
        assert_eq!(result.parameters.len(), 2);
        let typed_p = result
            .parameters
            .iter()
            .find(|p| p.name == "typed")
            .unwrap();
        assert_eq!(typed_p.type_hint.as_deref(), Some("int"));
        let untyped_p = result
            .parameters
            .iter()
            .find(|p| p.name == "untyped")
            .unwrap();
        assert!(
            untyped_p.type_hint.is_none(),
            "unknown should stay untyped, got: {:?}",
            untyped_p.type_hint
        );
        // Verify the signature has mixed: typed: int but untyped without annotation
        assert!(
            result.function.contains("typed: int"),
            "should have typed: int, got: {}",
            result.function
        );
        assert!(
            !result.function.contains("untyped:"),
            "untyped should not have annotation, got: {}",
            result.function
        );
    }

    #[test]
    fn extract_return_type_inferred_for_multi_return_is_dictionary() {
        // Multi-return always uses Dictionary regardless of individual types
        let temp = setup_project(&[(
            "player.gd",
            "func process():\n\tvar a: int = 1\n\tvar b: float = 2.0\n\ta += 1\n\tb += 1.0\n\tprint(a)\n\tprint(b)\n",
        )]);
        let result =
            extract_method(&temp.path().join("player.gd"), 4, 5, "update", temp.path()).unwrap();
        assert!(result.applied);
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(
            content.contains("-> Dictionary:"),
            "multi-return should always be Dictionary, got:\n{content}"
        );
        // But params should be typed
        assert!(
            content.contains("a: int") && content.contains("b: float"),
            "params should be typed, got:\n{content}"
        );
    }

    #[test]
    fn extract_default_param_type_inferred() {
        // default_parameter: func f(x = 5) → x should be inferred as int
        let temp = setup_project(&[(
            "player.gd",
            "func process(count = 10):\n\tprint(count)\n\tprint(\"end\")\n",
        )]);
        let result = extract_method(
            &temp.path().join("player.gd"),
            2,
            2,
            "show_count",
            temp.path(),
        )
        .unwrap();
        assert_eq!(result.parameters.len(), 1);
        assert_eq!(result.parameters[0].name, "count");
        assert_eq!(
            result.parameters[0].type_hint.as_deref(),
            Some("int"),
            "default param with integer literal should infer as int"
        );
    }

    // ── static propagation ─────────────────────────────────────────────

    #[test]
    fn extract_from_static_func_produces_static() {
        let temp = setup_project(&[(
            "utils.gd",
            "static func compute(x: int) -> int:\n\tvar doubled = x * 2\n\tprint(doubled)\n\treturn doubled\n",
        )]);
        // Extract `print(doubled)` (line 3)
        let result = extract_method(
            &temp.path().join("utils.gd"),
            3,
            3,
            "log_value",
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        let content = fs::read_to_string(temp.path().join("utils.gd")).unwrap();
        assert!(
            content.contains("static func log_value("),
            "extracted function should be static, got:\n{content}"
        );
        assert!(
            result.function.starts_with("static func"),
            "signature should start with 'static func', got: {}",
            result.function
        );
    }

    #[test]
    fn extract_from_non_static_func_stays_non_static() {
        let temp = setup_project(&[(
            "player.gd",
            "func _ready():\n\tprint(\"hello\")\n\tprint(\"world\")\n",
        )]);
        let result =
            extract_method(&temp.path().join("player.gd"), 2, 2, "greet", temp.path()).unwrap();
        assert!(result.applied);
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(
            !content.contains("static func greet"),
            "non-static function should not get static, got:\n{content}"
        );
    }

    #[test]
    fn extract_static_with_return() {
        let temp = setup_project(&[(
            "utils.gd",
            "static func compute():\n\tvar x = 1\n\tx += 10\n\tprint(x)\n",
        )]);
        // Extract `x += 10` (line 3) — x is written and used after
        let result =
            extract_method(&temp.path().join("utils.gd"), 3, 3, "bump", temp.path()).unwrap();
        assert!(result.applied);
        let content = fs::read_to_string(temp.path().join("utils.gd")).unwrap();
        assert!(
            content.contains("static func bump("),
            "extracted function should be static, got:\n{content}"
        );
        assert!(
            content.contains("return x"),
            "should return x, got:\n{content}"
        );
    }

    // ── inner class placement ──────────────────────────────────────────

    #[test]
    fn extract_in_inner_class_stays_in_class() {
        let temp = setup_project(&[(
            "player.gd",
            "\
extends Node

func _ready():
\tpass

class InnerClass:
\tfunc process():
\t\tvar x = 1
\t\tprint(x)
\t\tprint(\"done\")
",
        )]);
        // Extract `print("done")` (line 10)
        let result = extract_method(
            &temp.path().join("player.gd"),
            10,
            10,
            "do_print",
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        // The extracted function should be inside InnerClass (indented with one tab)
        assert!(
            content.contains("\tfunc do_print():"),
            "extracted function should be indented inside inner class, got:\n{content}"
        );
        // It should NOT appear at the top level (before _ready or at column 0)
        // Find the position of "func do_print" — it must come after "class InnerClass:"
        let class_pos = content.find("class InnerClass:").unwrap();
        let func_pos = content.find("func do_print").unwrap();
        assert!(
            func_pos > class_pos,
            "extracted function should be after the inner class header, got:\n{content}"
        );
    }

    #[test]
    fn extract_in_inner_class_with_params() {
        let temp = setup_project(&[(
            "player.gd",
            "\
extends Node

class Helper:
\tfunc compute():
\t\tvar a = 1
\t\tvar b = 2
\t\tprint(a + b)
\t\tprint(\"end\")
",
        )]);
        // Extract `print(a + b)` (line 7) — captures a and b
        let result = extract_method(
            &temp.path().join("player.gd"),
            7,
            7,
            "show_sum",
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        assert_eq!(result.parameters.len(), 2);
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(
            content.contains("\tfunc show_sum("),
            "extracted function should be indented inside inner class, got:\n{content}"
        );
    }

    #[test]
    fn extract_static_in_inner_class() {
        let temp = setup_project(&[(
            "player.gd",
            "\
extends Node

class Utils:
\tstatic func helper():
\t\tprint(\"a\")
\t\tprint(\"b\")
",
        )]);
        // Extract `print("b")` (line 6)
        let result =
            extract_method(&temp.path().join("player.gd"), 6, 6, "print_b", temp.path()).unwrap();
        assert!(result.applied);
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(
            content.contains("\tstatic func print_b():"),
            "should be static and indented inside inner class, got:\n{content}"
        );
    }
}
