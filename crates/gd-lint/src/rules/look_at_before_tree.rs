use gd_core::gd_ast::{self, GdDecl, GdExpr, GdFile, GdStmt};
use std::collections::{HashMap, HashSet};

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use gd_core::config::LintConfig;

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

    fn check(&self, file: &GdFile<'_>, _source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        gd_ast::visit_decls(file, &mut |decl| {
            if let GdDecl::Func(func) = decl {
                let mut unattached: HashMap<&str, usize> = HashMap::new();
                let mut attached: HashSet<&str> = HashSet::new();
                scan_stmts(&func.body, &mut unattached, &mut attached, &mut diags);
            }
        });
        diags
    }
}

/// Linear scan through statements tracking `.new()` assignments and flagging
/// tree-dependent method calls / global property assignments before `add_child()`.
fn scan_stmts<'a>(
    stmts: &[GdStmt<'a>],
    unattached: &mut HashMap<&'a str, usize>,
    attached: &mut HashSet<&'a str>,
    diags: &mut Vec<LintDiagnostic>,
) {
    for stmt in stmts {
        match stmt {
            // var x = SomeClass.new()
            GdStmt::Var(var) => {
                if let Some(value) = &var.value
                    && is_new_call(value)
                {
                    unattached.insert(var.name, var.node.start_position().row);
                }
                if let Some(value) = &var.value {
                    check_expr_for_tree_calls(value, unattached, attached, diags);
                }
            }
            // x = SomeClass.new() or x.global_position = ...
            GdStmt::Assign { target, value, .. } | GdStmt::AugAssign { target, value, .. } => {
                // Check for global property assignment on unattached
                if let GdExpr::PropertyAccess {
                    receiver, property, ..
                } = target
                    && let GdExpr::Ident { name: obj, .. } = receiver.as_ref()
                    && is_global_property(property)
                    && unattached.contains_key(obj)
                    && !attached.contains(obj)
                {
                    let pos = stmt.node().start_position();
                    diags.push(LintDiagnostic {
                        rule: "look-at-before-tree",
                        message: format!(
                            "`{obj}.{property}` set before `{obj}` is added to the scene tree"
                        ),
                        severity: Severity::Warning,
                        line: pos.row,
                        column: pos.column,
                        end_column: Some(stmt.node().end_position().column),
                        fix: None,
                        context_lines: None,
                    });
                    continue;
                }
                // x = SomeClass.new() reassignment (only for regular assign)
                if matches!(stmt, GdStmt::Assign { .. })
                    && let GdExpr::Ident { name, .. } = target
                    && is_new_call(value)
                {
                    attached.remove(name);
                    unattached.insert(name, stmt.node().start_position().row);
                }
                check_expr_for_tree_calls(value, unattached, attached, diags);
            }
            // Expression statements: add_child(x), method calls, etc.
            GdStmt::Expr { expr, .. } => {
                if let Some(arg_name) = extract_add_child_arg(expr) {
                    unattached.remove(arg_name);
                    attached.insert(arg_name);
                    continue;
                }
                check_expr_for_tree_calls(expr, unattached, attached, diags);
            }
            // Return statements may contain tree-dependent calls
            GdStmt::Return { value: Some(v), .. } => {
                check_expr_for_tree_calls(v, unattached, attached, diags);
            }
            // Recurse into control flow bodies
            GdStmt::If(if_stmt) => {
                scan_stmts(&if_stmt.body, unattached, attached, diags);
                for (_, branch) in &if_stmt.elif_branches {
                    scan_stmts(branch, unattached, attached, diags);
                }
                if let Some(else_body) = &if_stmt.else_body {
                    scan_stmts(else_body, unattached, attached, diags);
                }
            }
            GdStmt::For { body, .. } | GdStmt::While { body, .. } => {
                scan_stmts(body, unattached, attached, diags);
            }
            GdStmt::Match { arms, .. } => {
                for arm in arms {
                    scan_stmts(&arm.body, unattached, attached, diags);
                }
            }
            _ => {}
        }
    }
}

/// Check if an expression is `SomeClass.new()` — a MethodCall with method "new"
/// on an identifier (the class name).
fn is_new_call(expr: &GdExpr) -> bool {
    matches!(
        expr,
        GdExpr::MethodCall { method: "new", receiver, .. }
            if matches!(receiver.as_ref(), GdExpr::Ident { .. })
    )
}

/// Recursively check expressions for tree-dependent method calls on unattached variables.
fn check_expr_for_tree_calls(
    expr: &GdExpr,
    unattached: &HashMap<&str, usize>,
    attached: &HashSet<&str>,
    diags: &mut Vec<LintDiagnostic>,
) {
    if let GdExpr::MethodCall {
        receiver,
        method,
        node,
        ..
    } = expr
        && let GdExpr::Ident { name, .. } = receiver.as_ref()
        && unattached.contains_key(name)
        && !attached.contains(name)
        && gd_class_db::is_tree_dependent_method(method)
    {
        diags.push(LintDiagnostic {
            rule: "look-at-before-tree",
            message: format!(
                "`{name}.{method}()` called before `{name}` is added to the scene tree"
            ),
            severity: Severity::Warning,
            line: node.start_position().row,
            column: node.start_position().column,
            end_column: Some(node.end_position().column),
            fix: None,
            context_lines: None,
        });
        return;
    }

    // Recurse into sub-expressions
    match expr {
        GdExpr::MethodCall { receiver, args, .. } => {
            check_expr_for_tree_calls(receiver, unattached, attached, diags);
            for a in args {
                check_expr_for_tree_calls(a, unattached, attached, diags);
            }
        }
        GdExpr::Call { callee, args, .. } => {
            check_expr_for_tree_calls(callee, unattached, attached, diags);
            for a in args {
                check_expr_for_tree_calls(a, unattached, attached, diags);
            }
        }
        GdExpr::BinOp { left, right, .. } => {
            check_expr_for_tree_calls(left, unattached, attached, diags);
            check_expr_for_tree_calls(right, unattached, attached, diags);
        }
        GdExpr::UnaryOp { operand, .. } => {
            check_expr_for_tree_calls(operand, unattached, attached, diags);
        }
        GdExpr::PropertyAccess { receiver, .. } => {
            check_expr_for_tree_calls(receiver, unattached, attached, diags);
        }
        GdExpr::Subscript {
            receiver, index, ..
        } => {
            check_expr_for_tree_calls(receiver, unattached, attached, diags);
            check_expr_for_tree_calls(index, unattached, attached, diags);
        }
        GdExpr::Ternary {
            condition,
            true_val,
            false_val,
            ..
        } => {
            check_expr_for_tree_calls(condition, unattached, attached, diags);
            check_expr_for_tree_calls(true_val, unattached, attached, diags);
            check_expr_for_tree_calls(false_val, unattached, attached, diags);
        }
        GdExpr::Array { elements, .. } => {
            for e in elements {
                check_expr_for_tree_calls(e, unattached, attached, diags);
            }
        }
        GdExpr::Dict { pairs, .. } => {
            for (k, v) in pairs {
                check_expr_for_tree_calls(k, unattached, attached, diags);
                check_expr_for_tree_calls(v, unattached, attached, diags);
            }
        }
        GdExpr::Cast { expr: inner, .. }
        | GdExpr::Is { expr: inner, .. }
        | GdExpr::Await { expr: inner, .. } => {
            check_expr_for_tree_calls(inner, unattached, attached, diags);
        }
        _ => {}
    }
}

/// Extract the first identifier argument name from `add_child(x)` / `add_sibling(x)` calls.
fn extract_add_child_arg<'a>(expr: &GdExpr<'a>) -> Option<&'a str> {
    match expr {
        // add_child(x) — direct call
        GdExpr::Call { callee, args, .. } => {
            if let GdExpr::Ident { name: func, .. } = callee.as_ref()
                && matches!(*func, "add_child" | "add_sibling")
                && let Some(GdExpr::Ident { name, .. }) = args.first()
            {
                return Some(name);
            }
            None
        }
        // self.add_child(x) or parent.add_child(x)
        GdExpr::MethodCall { method, args, .. }
            if matches!(*method, "add_child" | "add_sibling") =>
        {
            if let Some(GdExpr::Ident { name, .. }) = args.first() {
                return Some(name);
            }
            None
        }
        _ => None,
    }
}

/// Global properties that are silently wrong when set before the node is in the tree.
const GLOBAL_PROPERTIES: &[&str] = &[
    "global_position",
    "global_rotation",
    "global_rotation_degrees",
    "global_transform",
    "global_basis",
];

fn is_global_property(name: &str) -> bool {
    GLOBAL_PROPERTIES.contains(&name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use gd_core::gd_ast;
    use gd_core::parser;

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
