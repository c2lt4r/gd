use tree_sitter::{Node, Tree};

use super::{LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct StaticTypeInference;

impl LintRule for StaticTypeInference {
    fn name(&self) -> &'static str {
        "static-type-inference"
    }

    fn check(&self, tree: &Tree, source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        let root = tree.root_node();
        check_node(root, source, &mut diags);
        diags
    }
}

fn check_node(node: Node, source: &str, diags: &mut Vec<LintDiagnostic>) {
    if node.kind() == "variable_statement" {
        // Skip if already has a type annotation
        if node.child_by_field_name("type").is_none()
            && let Some(value_node) = node.child_by_field_name("value")
            && let Some(inferred) = infer_type(&value_node, source)
        {
            let var_name = node
                .child_by_field_name("name")
                .map(|n| &source[n.byte_range()])
                .unwrap_or("variable");
            let name_node = node.child_by_field_name("name");
            let (line, col) = if let Some(n) = name_node {
                (n.start_position().row, n.start_position().column)
            } else {
                (node.start_position().row, node.start_position().column)
            };

            diags.push(LintDiagnostic {
                rule: "static-type-inference",
                message: format!(
                    "variable `{}` could have an explicit type: `{}`",
                    var_name, inferred
                ),
                severity: Severity::Warning,
                line,
                column: col,
                end_column: None,
                fix: None,
            });
        }
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            check_node(cursor.node(), source, diags);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

/// Try to infer the GDScript type from a literal value node.
fn infer_type(node: &Node, source: &str) -> Option<&'static str> {
    match node.kind() {
        "integer" => Some("int"),
        "float" => Some("float"),
        "string" => Some("String"),
        "true" | "false" => Some("bool"),
        "array" => Some("Array"),
        "dictionary" => Some("Dictionary"),
        "null" => None, // null is ambiguous
        // Handle negative literals like -5, -3.14
        "unary_operator" => {
            let text = &source[node.byte_range()];
            if text.starts_with('-') || text.starts_with('+') {
                // Check the operand
                if let Some(operand) = node.child_by_field_name("operand") {
                    match operand.kind() {
                        "integer" => return Some("int"),
                        "float" => return Some("float"),
                        _ => {}
                    }
                }
            }
            None
        }
        // Vector2(1, 2), Vector3(1, 2, 3), Color(...) constructor calls
        "call" => {
            if let Some(func) = node.child_by_field_name("function") {
                let name = &source[func.byte_range()];
                match name {
                    "Vector2" => Some("Vector2"),
                    "Vector2i" => Some("Vector2i"),
                    "Vector3" => Some("Vector3"),
                    "Vector3i" => Some("Vector3i"),
                    "Vector4" => Some("Vector4"),
                    "Vector4i" => Some("Vector4i"),
                    "Color" => Some("Color"),
                    "Rect2" => Some("Rect2"),
                    "Rect2i" => Some("Rect2i"),
                    "Transform2D" => Some("Transform2D"),
                    "Transform3D" => Some("Transform3D"),
                    "Basis" => Some("Basis"),
                    "Quaternion" => Some("Quaternion"),
                    "AABB" => Some("AABB"),
                    "Plane" => Some("Plane"),
                    "StringName" => Some("StringName"),
                    "NodePath" => Some("NodePath"),
                    _ => None,
                }
            } else {
                None
            }
        }
        _ => None,
    }
}
