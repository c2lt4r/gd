use tree_sitter::Node;
use crate::core::gd_ast::GdFile;

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct PhysicsInProcess;

const PHYSICS_METHODS: &[&str] = &[
    "move_and_slide",
    "move_and_collide",
    "apply_force",
    "apply_impulse",
    "apply_central_force",
    "apply_central_impulse",
    "apply_torque",
    "apply_torque_impulse",
    "set_velocity",
];

impl LintRule for PhysicsInProcess {
    fn name(&self) -> &'static str {
        "physics-in-process"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Performance
    }

    fn check(&self, file: &GdFile<'_>, source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        let root = file.node;
        find_process_functions(root, source, &mut diags);
        diags
    }
}

fn find_process_functions(node: Node, source: &str, diags: &mut Vec<LintDiagnostic>) {
    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            let child = cursor.node();
            if child.kind() == "function_definition"
                && let Some(name_node) = child.child_by_field_name("name")
            {
                let name = name_node.utf8_text(source.as_bytes()).unwrap_or("");
                if name == "_process"
                    && let Some(body) = child.child_by_field_name("body")
                {
                    find_physics_calls(body, source, diags);
                }
            }

            // Recurse into class bodies
            if child.kind() == "class_definition"
                && let Some(body) = child.child_by_field_name("body")
            {
                find_process_functions(body, source, diags);
            }

            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

fn find_physics_calls(node: Node, source: &str, diags: &mut Vec<LintDiagnostic>) {
    let src = source.as_bytes();

    // Bare call: move_and_slide() (implicit self)
    if node.kind() == "call" {
        let callee = node
            .children(&mut node.walk())
            .find(|c| c.kind() == "identifier")
            .and_then(|n| n.utf8_text(src).ok())
            .unwrap_or("");
        if PHYSICS_METHODS.contains(&callee) {
            diags.push(make_diagnostic(callee, node));
        }
    }

    // Method call: self.move_and_slide() or body.apply_force(...)
    // Parsed as: attribute > [identifier, attribute_call > [identifier, arguments]]
    if node.kind() == "attribute" {
        check_attribute_physics_call(node, source, diags);
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            let child = cursor.node();
            // Don't recurse into nested function definitions or lambdas
            if child.kind() != "function_definition" && child.kind() != "lambda" {
                find_physics_calls(child, source, diags);
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

fn check_attribute_physics_call(node: Node, source: &str, diags: &mut Vec<LintDiagnostic>) {
    let src = source.as_bytes();

    for child in node.children(&mut node.walk()) {
        if child.kind() == "attribute_call" {
            let method = child
                .children(&mut child.walk())
                .find(|c| c.kind() == "identifier")
                .and_then(|n| n.utf8_text(src).ok())
                .unwrap_or("");
            if PHYSICS_METHODS.contains(&method) {
                diags.push(make_diagnostic(method, node));
            }
        }
    }
}

fn make_diagnostic(method: &str, node: Node) -> LintDiagnostic {
    LintDiagnostic {
        rule: "physics-in-process",
        message: format!("`{method}()` should be called in _physics_process(), not _process()"),
        severity: Severity::Warning,
        line: node.start_position().row,
        column: node.start_position().column,
        end_column: None,
        fix: None,
        context_lines: None,
    }
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
        PhysicsInProcess.check(&file, source, &config)
    }

    #[test]
    fn detects_move_and_slide_in_process() {
        let source = "func _process(delta: float) -> void:\n\tmove_and_slide()\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule, "physics-in-process");
        assert!(diags[0].message.contains("move_and_slide()"));
    }

    #[test]
    fn detects_apply_force_in_process() {
        let source = "func _process(delta: float) -> void:\n\tapply_force(Vector2(0, 10))\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("apply_force()"));
    }

    #[test]
    fn detects_self_move_and_slide() {
        let source = "func _process(delta: float) -> void:\n\tself.move_and_slide()\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("move_and_slide()"));
    }

    #[test]
    fn detects_apply_impulse_on_object() {
        let source = "func _process(delta: float) -> void:\n\tbody.apply_impulse(Vector2.UP)\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("apply_impulse()"));
    }

    #[test]
    fn no_warning_in_physics_process() {
        let source = "func _physics_process(delta: float) -> void:\n\tmove_and_slide()\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_in_regular_function() {
        let source = "func helper() -> void:\n\tmove_and_slide()\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn detects_multiple_physics_calls() {
        let source = "func _process(delta: float) -> void:\n\tmove_and_slide()\n\tapply_force(Vector2.ZERO)\n";
        let diags = check(source);
        assert_eq!(diags.len(), 2);
    }

    #[test]
    fn detects_move_and_collide() {
        let source = "func _process(delta: float) -> void:\n\tvar col := move_and_collide(velocity * delta)\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("move_and_collide()"));
    }

    #[test]
    fn detects_set_velocity() {
        let source = "func _process(delta: float) -> void:\n\tset_velocity(Vector2(100, 0))\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("set_velocity()"));
    }

    #[test]
    fn detects_in_inner_class() {
        let source = "class Inner:\n\tfunc _process(delta: float) -> void:\n\t\tmove_and_slide()\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn no_warning_for_nested_function() {
        let source = "func _process(delta: float) -> void:\n\tpass\n\nfunc helper() -> void:\n\tmove_and_slide()\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn detects_in_conditional() {
        let source = "func _process(delta: float) -> void:\n\tif is_on_floor():\n\t\tapply_force(Vector2.UP * 100)\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
    }
}
