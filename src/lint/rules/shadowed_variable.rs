use std::collections::HashSet;
use tree_sitter::{Node, Tree};

use super::{LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct ShadowedVariable;

impl LintRule for ShadowedVariable {
    fn name(&self) -> &'static str {
        "shadowed-variable"
    }

    fn check(&self, tree: &Tree, source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        let root = tree.root_node();
        collect_functions(root, source, &mut diags);
        diags
    }
}

fn collect_functions(node: Node, source: &str, diags: &mut Vec<LintDiagnostic>) {
    if node.kind() == "function_definition" {
        let mut outer_vars: HashSet<String> = HashSet::new();

        // Collect parameter names as outer scope variables
        if let Some(params) = node.child_by_field_name("parameters") {
            collect_param_names(params, source, &mut outer_vars);
        }

        if let Some(body) = node.child_by_field_name("body") {
            check_body(body, source, &outer_vars, diags);
        }
        return;
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            collect_functions(cursor.node(), source, diags);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

fn collect_param_names(params: Node, source: &str, names: &mut HashSet<String>) {
    let mut cursor = params.walk();
    if !cursor.goto_first_child() {
        return;
    }
    loop {
        let child = cursor.node();
        match child.kind() {
            "identifier" => {
                names.insert(source[child.byte_range()].to_string());
            }
            "typed_parameter" | "default_parameter" | "typed_default_parameter" => {
                if let Some(name_node) = child.child(0)
                    && name_node.kind() == "identifier"
                {
                    names.insert(source[name_node.byte_range()].to_string());
                }
            }
            _ => {}
        }
        if !cursor.goto_next_sibling() {
            break;
        }
    }
}

/// Check a body for variable declarations, tracking outer scope names.
fn check_body(body: Node, source: &str, outer: &HashSet<String>, diags: &mut Vec<LintDiagnostic>) {
    let mut current_scope = outer.clone();

    let mut cursor = body.walk();
    if !cursor.goto_first_child() {
        return;
    }

    loop {
        let child = cursor.node();

        if child.kind() == "variable_statement"
            && let Some(name_node) = child.child_by_field_name("name")
        {
            let name = source[name_node.byte_range()].to_string();
            if outer.contains(&name) {
                diags.push(LintDiagnostic {
                    rule: "shadowed-variable",
                    message: format!("variable `{name}` shadows a variable from an outer scope"),
                    severity: Severity::Warning,
                    line: name_node.start_position().row,
                    column: name_node.start_position().column,
                    fix: None,
                    end_column: None,
                    context_lines: None,
                });
            }
            current_scope.insert(name);
        }

        // Recurse into inner scopes (if/for/while bodies)
        if is_scope_node(child.kind()) {
            check_inner_scopes(child, source, &current_scope, diags);
        }

        if !cursor.goto_next_sibling() {
            break;
        }
    }
}

fn is_scope_node(kind: &str) -> bool {
    matches!(
        kind,
        "if_statement"
            | "for_statement"
            | "while_statement"
            | "elif_clause"
            | "else_clause"
            | "match_statement"
    )
}

fn check_inner_scopes(
    node: Node,
    source: &str,
    outer: &HashSet<String>,
    diags: &mut Vec<LintDiagnostic>,
) {
    // For for_statement, the iterator variable is also an outer name for the body
    if node.kind() == "for_statement" {
        let mut for_outer = outer.clone();
        if let Some(iter_node) = node.child_by_field_name("left")
            && iter_node.kind() == "identifier"
        {
            for_outer.insert(source[iter_node.byte_range()].to_string());
        }
        if let Some(body) = node.child_by_field_name("body") {
            check_body(body, source, &for_outer, diags);
        }
        return;
    }

    let mut cursor = node.walk();
    if !cursor.goto_first_child() {
        return;
    }
    loop {
        let child = cursor.node();
        if child.kind() == "body" || child.kind() == "block" {
            check_body(child, source, outer, diags);
        } else if is_scope_node(child.kind()) {
            check_inner_scopes(child, source, outer, diags);
        }
        if !cursor.goto_next_sibling() {
            break;
        }
    }
}
