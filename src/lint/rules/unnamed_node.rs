use crate::core::gd_ast::{self, GdDecl, GdExpr, GdFile, GdStmt};
use std::collections::HashMap;

use super::{LintCategory, LintDiagnostic, LintRule};
use crate::core::config::LintConfig;

/// Detects `add_child()` calls where the node was created with `.new()` but
/// never had `.name` set. Without an explicit name, the node appears as
/// `@ClassName@123` in the scene tree, making debugging harder.
pub struct UnnamedNode;

impl LintRule for UnnamedNode {
    fn name(&self) -> &'static str {
        "unnamed-node"
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
                let mut tracked: HashMap<&str, (&str, usize, bool)> = HashMap::new();
                walk_stmts(&func.body, &mut tracked, &mut diags);
            }
        });
        diags
    }
}

/// Track: var_name -> (type_name, line_1based, was_name_set)
fn walk_stmts<'a>(
    stmts: &[GdStmt<'a>],
    tracked: &mut HashMap<&'a str, (&'a str, usize, bool)>,
    diags: &mut Vec<LintDiagnostic>,
) {
    for stmt in stmts {
        match stmt {
            // var x = SomeType.new()
            GdStmt::Var(var) => {
                if let Some(value) = &var.value
                    && let Some(type_name) = extract_new_type(value)
                    && is_node_class(type_name)
                {
                    tracked.insert(
                        var.name,
                        (type_name, var.node.start_position().row + 1, false),
                    );
                }
            }
            // x = SomeType.new() (reassignment) or x.name = "..."
            GdStmt::Assign { target, value, .. } => {
                // x = SomeType.new()
                if let GdExpr::Ident { name, .. } = target
                    && let Some(type_name) = extract_new_type(value)
                    && is_node_class(type_name)
                {
                    tracked.insert(
                        name,
                        (type_name, stmt.node().start_position().row + 1, false),
                    );
                }
                // x.name = "..."
                if let GdExpr::PropertyAccess {
                    receiver,
                    property: "name",
                    ..
                } = target
                    && let GdExpr::Ident { name, .. } = receiver.as_ref()
                    && let Some(entry) = tracked.get_mut(name)
                {
                    entry.2 = true;
                }
            }
            // Expression statements: add_child(x), parent.add_child(x), call_deferred("add_child", x)
            GdStmt::Expr { expr, .. } => {
                if let Some((var_name, row, col)) = extract_add_child_arg(expr) {
                    emit_unnamed_diag(var_name, row, col, tracked, diags);
                }
                if let Some((var_name, row, col)) = extract_deferred_add_child(expr) {
                    emit_unnamed_diag(var_name, row, col, tracked, diags);
                }
            }
            // Recurse into control flow bodies
            GdStmt::If(if_stmt) => {
                walk_stmts(&if_stmt.body, tracked, diags);
                for (_, branch) in &if_stmt.elif_branches {
                    walk_stmts(branch, tracked, diags);
                }
                if let Some(else_body) = &if_stmt.else_body {
                    walk_stmts(else_body, tracked, diags);
                }
            }
            GdStmt::For { body, .. } | GdStmt::While { body, .. } => {
                walk_stmts(body, tracked, diags);
            }
            GdStmt::Match { arms, .. } => {
                for arm in arms {
                    walk_stmts(&arm.body, tracked, diags);
                }
            }
            _ => {}
        }
    }
}

/// Emit a diagnostic if the variable is tracked and was never named.
fn emit_unnamed_diag(
    var_name: &str,
    row: usize,
    col: usize,
    tracked: &HashMap<&str, (&str, usize, bool)>,
    diags: &mut Vec<LintDiagnostic>,
) {
    if let Some(&(type_name, _, name_was_set)) = tracked.get(var_name)
        && !name_was_set
    {
        diags.push(LintDiagnostic {
            rule: "unnamed-node",
            message: format!(
                "`{var_name}` ({type_name}) is added to the scene tree without \
                 setting `.name` — it will appear as @{type_name}@ in the debugger"
            ),
            line: row + 1,
            column: col + 1,
            end_column: None,
            severity: super::Severity::Warning,
            fix: None,
            context_lines: None,
        });
    }
}

/// Extract the class name from `SomeType.new()` — returns the identifier receiver of a "new" method call.
fn extract_new_type<'a>(expr: &GdExpr<'a>) -> Option<&'a str> {
    if let GdExpr::MethodCall {
        method: "new",
        receiver,
        ..
    } = expr
        && let GdExpr::Ident { name, .. } = receiver.as_ref()
    {
        return Some(name);
    }
    None
}

/// Detect `add_child(x)` or `parent.add_child(x)`. Returns (arg_name, row, col).
fn extract_add_child_arg<'a>(expr: &GdExpr<'a>) -> Option<(&'a str, usize, usize)> {
    match expr {
        // add_child(x) — direct call
        GdExpr::Call {
            callee, args, node, ..
        } => {
            if let GdExpr::Ident { name: func, .. } = callee.as_ref()
                && *func == "add_child"
                && let Some(GdExpr::Ident { name, .. }) = args.first()
            {
                return Some((
                    name,
                    node.start_position().row,
                    node.start_position().column,
                ));
            }
            None
        }
        // parent.add_child(x) — method call
        GdExpr::MethodCall {
            method: "add_child",
            args,
            node,
            ..
        } => {
            if let Some(GdExpr::Ident { name, .. }) = args.first() {
                return Some((
                    name,
                    node.start_position().row,
                    node.start_position().column,
                ));
            }
            None
        }
        _ => None,
    }
}

/// Detect `call_deferred("add_child", x)` or `parent.call_deferred("add_child", x)`.
fn extract_deferred_add_child<'a>(expr: &GdExpr<'a>) -> Option<(&'a str, usize, usize)> {
    let (method, args, node) = match expr {
        GdExpr::Call {
            callee, args, node, ..
        } => {
            if let GdExpr::Ident { name, .. } = callee.as_ref() {
                (*name, args, node)
            } else {
                return None;
            }
        }
        GdExpr::MethodCall {
            method, args, node, ..
        } => (*method, args, node),
        _ => return None,
    };

    if method != "call_deferred" {
        return None;
    }

    // First arg should be "add_child" string
    if let Some(GdExpr::StringLiteral { value, .. }) = args.first()
        && value.trim_matches('"').trim_matches('\'') == "add_child"
        && let Some(GdExpr::Ident { name, .. }) = args.get(1)
    {
        return Some((
            name,
            node.start_position().row,
            node.start_position().column,
        ));
    }
    None
}

/// Check if a class name is a Node subclass (has `.name` property).
fn is_node_class(name: &str) -> bool {
    crate::class_db::inherits(name, "Node") || name == "Node"
}

#[cfg(test)]
mod tests {
    use crate::lint::rules::LintRule;

    use super::*;

    fn check(source: &str) -> Vec<LintDiagnostic> {
        let tree = crate::core::parser::parse(source).unwrap();
        let file = crate::core::gd_ast::convert(&tree, source);
        let config = LintConfig::default();
        UnnamedNode.check(&file, source, &config)
    }

    #[test]
    fn flags_add_child_without_name() {
        let diags = check("func _ready():\n\tvar btn = Button.new()\n\tadd_child(btn)\n");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("btn"));
        assert!(diags[0].message.contains("Button"));
    }

    #[test]
    fn no_flag_when_name_is_set() {
        let diags = check(
            "func _ready():\n\tvar btn = Button.new()\n\tbtn.name = \"Submit\"\n\tadd_child(btn)\n",
        );
        assert!(diags.is_empty());
    }

    #[test]
    fn no_flag_for_non_node_class() {
        let diags = check(
            "func _ready():\n\tvar style = StyleBoxFlat.new()\n\tadd_theme_stylebox_override(\"panel\", style)\n",
        );
        assert!(diags.is_empty());
    }

    #[test]
    fn flags_method_call_add_child() {
        let diags = check("func _ready():\n\tvar lbl = Label.new()\n\tparent.add_child(lbl)\n");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_deferred_add_child() {
        let diags = check(
            "func _ready():\n\tvar timer = Timer.new()\n\tget_root().call_deferred(\"add_child\", timer)\n",
        );
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn no_flag_deferred_with_name() {
        let diags = check(
            "func _ready():\n\tvar timer = Timer.new()\n\ttimer.name = \"MyTimer\"\n\tget_root().call_deferred(\"add_child\", timer)\n",
        );
        assert!(diags.is_empty());
    }

    #[test]
    fn no_flag_for_instantiate() {
        // .instantiate() returns a scene instance which already has a name
        let diags =
            check("func _ready():\n\tvar npc = npc_scene.instantiate()\n\tadd_child(npc)\n");
        assert!(diags.is_empty());
    }

    #[test]
    fn no_flag_for_factory_function() {
        // Can't trace into function returns, so skip
        let diags = check("func _ready():\n\tvar btn = _create_button(\"OK\")\n\tadd_child(btn)\n");
        assert!(diags.is_empty());
    }

    #[test]
    fn no_flag_for_preload_new() {
        // preload("...").new() — can't know the class, skip
        let diags = check(
            "func _ready():\n\tvar mgr = preload(\"res://manager.gd\").new()\n\tadd_child(mgr)\n",
        );
        assert!(diags.is_empty());
    }

    #[test]
    fn flags_multiple_unnamed_nodes() {
        let diags = check(
            "func _build():\n\tvar a = Label.new()\n\tvar b = Button.new()\n\tadd_child(a)\n\tadd_child(b)\n",
        );
        assert_eq!(diags.len(), 2);
    }

    #[test]
    fn flags_only_unnamed_in_mixed() {
        let diags = check(
            "func _build():\n\tvar a = Label.new()\n\ta.name = \"Title\"\n\tvar b = Button.new()\n\tadd_child(a)\n\tadd_child(b)\n",
        );
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("Button"));
    }

    #[test]
    fn flags_inside_if_block() {
        let diags =
            check("func _ready():\n\tvar btn = Button.new()\n\tif true:\n\t\tadd_child(btn)\n");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn no_flag_for_non_node_resource() {
        // RandomNumberGenerator extends RefCounted, not Node
        let diags = check("func _ready():\n\tvar rng = RandomNumberGenerator.new()\n");
        assert!(diags.is_empty());
    }

    #[test]
    fn flags_walrus_operator() {
        // var bg := ColorRect.new() — uses := instead of =
        let diags = check("func _build():\n\tvar bg := ColorRect.new()\n\tadd_child(bg)\n");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("ColorRect"));
    }

    #[test]
    fn flags_member_assignment() {
        // _opp_name = Label.new() — member variable, no var keyword
        let diags = check("func _build():\n\t_opp_name = Label.new()\n\tadd_child(_opp_name)\n");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("Label"));
    }

    #[test]
    fn no_flag_member_with_name() {
        let diags = check(
            "func _build():\n\t_label = Label.new()\n\t_label.name = \"Title\"\n\tadd_child(_label)\n",
        );
        assert!(diags.is_empty());
    }

    #[test]
    fn flags_parent_add_child() {
        // parent.add_child(info) — method call style
        let diags = check(
            "func _build(parent: Control):\n\tvar info := PanelContainer.new()\n\tparent.add_child(info)\n",
        );
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn no_flag_when_not_added() {
        // Node created but never add_child'd — not our problem
        let diags = check("func _ready():\n\tvar btn = Button.new()\n\tbtn.text = \"Hi\"\n");
        assert!(diags.is_empty());
    }
}
