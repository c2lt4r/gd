use std::collections::{HashMap, HashSet};
use tree_sitter::Node;
use crate::core::gd_ast::{self, GdDecl, GdFile};

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct LookAtBeforeTree;

impl LintRule for LookAtBeforeTree {
    fn name(&self) -> &'static str {
        "look-at-before-tree"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Godot
    }

    fn default_enabled(&self) -> bool {
        false
    }

    fn check(&self, file: &GdFile<'_>, source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        gd_ast::visit_decls(file, &mut |decl| {
            if let GdDecl::Func(func) = decl
                && let Some(body) = func.node.child_by_field_name("body")
            {
                check_function_body(body, source, &mut diags);
            }
        });
        diags
    }
}

/// Linear scan through a function body.
/// Track variables assigned via `X.new()` and flag tree-dependent method calls
/// on those variables before `add_child(var)` is called.
fn check_function_body(body: Node, source: &str, diags: &mut Vec<LintDiagnostic>) {
    // Variables created via X.new() that haven't been added to the tree yet
    let mut unattached: HashMap<String, usize> = HashMap::new();
    // Variables that have been added to the tree
    let mut attached: HashSet<String> = HashSet::new();

    scan_statements(body, source, &mut unattached, &mut attached, diags);
}

fn scan_statements(
    node: Node,
    source: &str,
    unattached: &mut HashMap<String, usize>,
    attached: &mut HashSet<String>,
    diags: &mut Vec<LintDiagnostic>,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if !child.is_named() || child.kind() == "comment" {
            continue;
        }

        // var x = SomeClass.new() or var x := SomeClass.new()
        if child.kind() == "variable_statement" {
            if let Some((var_name, _line)) = extract_new_assignment(&child, source) {
                unattached.insert(var_name, child.start_position().row);
            }
            // Also check the RHS for tree-dependent calls
            if let Some(value) = child.child_by_field_name("value") {
                check_expr_for_tree_calls(&value, source, unattached, attached, diags);
            }
            continue;
        }

        // Check for global_* property assignment on unattached variables
        // Tree-sitter: `expression_statement > assignment` or `assignment_statement`
        if let Some((obj, prop)) = extract_global_property_assignment(&child, source)
            && unattached.contains_key(&obj)
            && !attached.contains(&obj)
        {
            diags.push(LintDiagnostic {
                rule: "look-at-before-tree",
                message: format!("`{obj}.{prop}` set before `{obj}` is added to the scene tree"),
                severity: Severity::Warning,
                line: child.start_position().row,
                column: child.start_position().column,
                end_column: Some(child.end_position().column),
                fix: None,
                context_lines: None,
            });
            continue;
        }

        // x = SomeClass.new() (reassignment)
        if child.kind() == "assignment_statement" {
            if let Some((var_name, _line)) = extract_new_reassignment(&child, source) {
                attached.remove(&var_name);
                unattached.insert(var_name, child.start_position().row);
            }
            check_expr_for_tree_calls(&child, source, unattached, attached, diags);
            continue;
        }

        // add_child(x) / add_sibling(x) — mark x as attached
        if is_add_child_call(&child, source) {
            if let Some(arg_name) = extract_first_arg(&child, source) {
                unattached.remove(&arg_name);
                attached.insert(arg_name);
            }
            continue;
        }

        // Check for tree-dependent method calls on unattached variables
        check_expr_for_tree_calls(&child, source, unattached, attached, diags);

        // Recurse into control flow blocks (if/match/for/while)
        recurse_into_blocks(&child, source, unattached, attached, diags);
    }
}

fn recurse_into_blocks(
    node: &Node,
    source: &str,
    unattached: &mut HashMap<String, usize>,
    attached: &mut HashSet<String>,
    diags: &mut Vec<LintDiagnostic>,
) {
    match node.kind() {
        "if_statement" | "for_statement" | "while_statement" | "match_statement" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "body" || child.kind() == "match_body" {
                    scan_statements(child, source, unattached, attached, diags);
                }
                // else/elif branches
                if (child.kind() == "elif_branch" || child.kind() == "else_branch")
                    && let Some(body) = child.child_by_field_name("body")
                {
                    scan_statements(body, source, unattached, attached, diags);
                }
            }
        }
        _ => {}
    }
}

/// Check if this expression contains a method call on an unattached variable.
fn check_expr_for_tree_calls(
    node: &Node,
    source: &str,
    unattached: &mut HashMap<String, usize>,
    attached: &HashSet<String>,
    diags: &mut Vec<LintDiagnostic>,
) {
    // node.method() pattern: attribute > [identifier, attribute_call > [identifier, arguments]]
    if node.kind() == "attribute"
        && let Some((obj_name, method_name)) = extract_method_call(node, source)
        && unattached.contains_key(&obj_name)
        && !attached.contains(&obj_name)
        && crate::class_db::is_tree_dependent_method(&method_name)
    {
        diags.push(LintDiagnostic {
            rule: "look-at-before-tree",
            message: format!(
                "`{obj_name}.{method_name}()` called before `{obj_name}` is added to the scene tree"
            ),
            severity: Severity::Warning,
            line: node.start_position().row,
            column: node.start_position().column,
            end_column: Some(node.end_position().column),
            fix: None,
            context_lines: None,
        });
    }

    // Recurse into sub-expressions
    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            let child = cursor.node();
            if child.kind() != "function_definition" && child.kind() != "lambda" {
                check_expr_for_tree_calls(&child, source, unattached, attached, diags);
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

/// Extract `(var_name, line)` from `var x = Foo.new()` or `var x := Foo.new()`.
fn extract_new_assignment(node: &Node, source: &str) -> Option<(String, usize)> {
    let name = node
        .child_by_field_name("name")?
        .utf8_text(source.as_bytes())
        .ok()?;
    let value = node.child_by_field_name("value")?;
    if is_new_call(&value, source) {
        Some((name.to_string(), node.start_position().row))
    } else {
        None
    }
}

/// Extract from `x = Foo.new()` reassignment.
fn extract_new_reassignment(node: &Node, source: &str) -> Option<(String, usize)> {
    let lhs = node.named_child(0)?;
    if lhs.kind() != "identifier" {
        return None;
    }
    let name = lhs.utf8_text(source.as_bytes()).ok()?;
    // RHS is the last named child
    let rhs = node.named_child(node.named_child_count() - 1)?;
    if is_new_call(&rhs, source) {
        Some((name.to_string(), node.start_position().row))
    } else {
        None
    }
}

/// Check if a node is a `.new()` call: attribute > [identifier, attribute_call > [identifier("new")]]
fn is_new_call(node: &Node, source: &str) -> bool {
    if node.kind() != "attribute" {
        return false;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "attribute_call"
            && let Some(name_node) = child.named_child(0)
            && name_node.utf8_text(source.as_bytes()).ok() == Some("new")
        {
            return true;
        }
    }
    false
}

/// Check if a node is `add_child(...)` or `add_sibling(...)`.
fn is_add_child_call(node: &Node, source: &str) -> bool {
    // Direct call: add_child(x)
    if node.kind() == "call"
        && let Some(id) = node.named_child(0)
        && id.kind() == "identifier"
        && let Ok(name) = id.utf8_text(source.as_bytes())
    {
        return matches!(name, "add_child" | "add_sibling");
    }
    // Also expression_statement wrapping a call or attribute call
    if node.kind() == "expression_statement" {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if is_add_child_call(&child, source) {
                return true;
            }
            // self.add_child(x) → attribute > [identifier("self"), attribute_call > [identifier("add_child"), arguments]]
            if child.kind() == "attribute"
                && let Some((_, method)) = extract_method_call(&child, source)
                && matches!(method.as_str(), "add_child" | "add_sibling")
            {
                return true;
            }
        }
    }
    false
}

/// Extract first positional argument name from a call or attribute_call.
fn extract_first_arg(node: &Node, source: &str) -> Option<String> {
    // For expression_statement, look inside
    if node.kind() == "expression_statement" {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if let Some(arg) = extract_first_arg(&child, source) {
                return Some(arg);
            }
        }
        return None;
    }

    // Direct call: call > [identifier, arguments > [identifier]]
    if node.kind() == "call" {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "arguments"
                && let Some(first) = child.named_child(0)
                && first.kind() == "identifier"
            {
                return first.utf8_text(source.as_bytes()).ok().map(String::from);
            }
        }
    }

    // Attribute call: attribute > [..., attribute_call > [identifier, arguments]]
    if node.kind() == "attribute" {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "attribute_call" {
                let mut inner_cursor = child.walk();
                for inner in child.children(&mut inner_cursor) {
                    if inner.kind() == "arguments"
                        && let Some(first) = inner.named_child(0)
                        && first.kind() == "identifier"
                    {
                        return first.utf8_text(source.as_bytes()).ok().map(String::from);
                    }
                }
            }
        }
    }

    None
}

/// Global properties that are silently wrong when set before the node is in the tree.
const GLOBAL_PROPERTIES: &[&str] = &[
    "global_position",
    "global_rotation",
    "global_rotation_degrees",
    "global_transform",
    "global_basis",
];

/// Extract `(object_name, property_name)` from assignments like `obj.global_position = ...`.
/// Handles both `assignment_statement` and `expression_statement > assignment`.
fn extract_global_property_assignment(node: &Node, source: &str) -> Option<(String, String)> {
    let assign = if node.kind() == "expression_statement" {
        // Look for `assignment` or `augmented_assignment` inside
        let mut cursor = node.walk();
        let mut found = None;
        for child in node.children(&mut cursor) {
            if child.kind() == "assignment" || child.kind() == "augmented_assignment" {
                found = Some(child);
                break;
            }
        }
        found?
    } else if node.kind() == "assignment_statement"
        || node.kind() == "augmented_assignment_statement"
    {
        *node
    } else {
        return None;
    };

    // LHS is the first named child
    let lhs = assign.named_child(0)?;
    if lhs.kind() != "attribute" {
        return None;
    }
    let obj = lhs.named_child(0)?;
    if obj.kind() != "identifier" {
        return None;
    }
    let obj_name = obj.utf8_text(source.as_bytes()).ok()?;
    let prop = lhs.named_child(1)?;
    let prop_name = prop.utf8_text(source.as_bytes()).ok()?;
    if GLOBAL_PROPERTIES.contains(&prop_name) {
        Some((obj_name.to_string(), prop_name.to_string()))
    } else {
        None
    }
}

/// Extract (object_name, method_name) from `obj.method(...)` call.
fn extract_method_call(node: &Node, source: &str) -> Option<(String, String)> {
    if node.kind() != "attribute" {
        return None;
    }
    let obj = node.named_child(0)?;
    if obj.kind() != "identifier" {
        return None;
    }
    let obj_name = obj.utf8_text(source.as_bytes()).ok()?;

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "attribute_call"
            && let Some(name_node) = child.named_child(0)
            && let Ok(method_name) = name_node.utf8_text(source.as_bytes())
        {
            return Some((obj_name.to_string(), method_name.to_string()));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::parser;
    use crate::core::gd_ast;

    fn check(source: &str) -> Vec<LintDiagnostic> {
        let tree = parser::parse(source).unwrap();
        let file = gd_ast::convert(&tree, source);
        let config = LintConfig::default();
        LookAtBeforeTree.check(&file, source, &config)
    }

    #[test]
    fn detects_look_at_before_add_child() {
        let source = "\
func setup():
\tvar node := Node3D.new()
\tnode.look_at(Vector3.ZERO)
\tadd_child(node)
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("look_at"));
        assert!(diags[0].message.contains("before"));
    }

    #[test]
    fn detects_to_global_before_add_child() {
        let source = "\
func setup():
\tvar sprite := Node2D.new()
\tvar pos := sprite.to_global(Vector2.ZERO)
\tadd_child(sprite)
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("to_global"));
    }

    #[test]
    fn no_warning_after_add_child() {
        let source = "\
func setup():
\tvar node := Node3D.new()
\tadd_child(node)
\tnode.look_at(Vector3.ZERO)
";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_non_new_variable() {
        let source = "\
func setup():
\tvar node := get_node(\"Existing\")
\tnode.look_at(Vector3.ZERO)
";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_non_tree_dependent_method() {
        let source = "\
func setup():
\tvar node := Node3D.new()
\tnode.set_position(Vector3.ZERO)
\tadd_child(node)
";
        assert!(check(source).is_empty());
    }

    #[test]
    fn detects_get_parent_before_add_child() {
        let source = "\
func setup():
\tvar child := Node.new()
\tvar p := child.get_parent()
\tadd_child(child)
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("get_parent"));
    }

    #[test]
    fn self_add_child_also_works() {
        let source = "\
func setup():
\tvar node := Node3D.new()
\tself.add_child(node)
\tnode.look_at(Vector3.ZERO)
";
        assert!(check(source).is_empty());
    }

    #[test]
    fn opt_in_rule() {
        assert!(!LookAtBeforeTree.default_enabled());
    }

    #[test]
    fn no_warning_without_new() {
        let source = "\
func setup():
\tvar x := 42
\tvar y := \"hello\"
";
        assert!(check(source).is_empty());
    }

    #[test]
    fn detects_global_position_before_add_child() {
        let source = "\
func setup():
\tvar zone := Node3D.new()
\tzone.global_position = Vector3(100, 5, 0)
\tadd_child(zone)
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("global_position"));
        assert!(diags[0].message.contains("before"));
    }

    #[test]
    fn no_warning_local_position() {
        let source = "\
func setup():
\tvar zone := Node3D.new()
\tzone.position = Vector3(100, 5, 0)
\tadd_child(zone)
";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_global_after_add_child() {
        let source = "\
func setup():
\tvar zone := Node3D.new()
\tadd_child(zone)
\tzone.global_position = Vector3(100, 5, 0)
";
        assert!(check(source).is_empty());
    }

    #[test]
    fn detects_global_rotation() {
        let source = "\
func setup():
\tvar node := Node3D.new()
\tnode.global_rotation = Vector3(0, 1.5, 0)
\tadd_child(node)
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("global_rotation"));
    }

    #[test]
    fn detects_global_transform() {
        let source = "\
func setup():
\tvar node := Node3D.new()
\tnode.global_transform = Transform3D.IDENTITY
\tadd_child(node)
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("global_transform"));
    }

    #[test]
    fn multiple_variables_tracked() {
        let source = "\
func setup():
\tvar a := Node3D.new()
\tvar b := Node2D.new()
\tadd_child(a)
\ta.look_at(Vector3.ZERO)
\tb.look_at(Vector2.ZERO)
";
        let diags = check(source);
        // b.look_at should be flagged (b not yet added), a.look_at should not (a was added)
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("b.look_at"));
    }
}
