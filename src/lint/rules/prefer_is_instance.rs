use tree_sitter::Node;
use crate::core::gd_ast::GdFile;

use super::{Fix, LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct PreferIsInstance;

impl LintRule for PreferIsInstance {
    fn name(&self) -> &'static str {
        "prefer-is-instance"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Style
    }

    fn default_enabled(&self) -> bool {
        false
    }

    fn check(&self, file: &GdFile<'_>, source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        check_node(file.node, source, &mut diags);
        diags
    }
}

fn check_node(node: Node, source: &str, diags: &mut Vec<LintDiagnostic>) {
    if node.kind() == "binary_operator" {
        check_typeof_comparison(node, source, diags);
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

fn check_typeof_comparison(node: Node, source: &str, diags: &mut Vec<LintDiagnostic>) {
    let Some(op_node) = node.child_by_field_name("op") else {
        return;
    };
    let op = &source[op_node.byte_range()];

    if op != "==" && op != "!=" {
        return;
    }

    let Some(left) = node.child_by_field_name("left") else {
        return;
    };
    let Some(right) = node.child_by_field_name("right") else {
        return;
    };

    // Try both orders: typeof(x) == TYPE_* and TYPE_* == typeof(x)
    let (typeof_arg, type_constant) = if let Some(arg) = extract_typeof_arg(left, source) {
        // left is typeof(x), right should be TYPE_*
        let type_text = &source[right.byte_range()];
        (arg, type_text)
    } else if let Some(arg) = extract_typeof_arg(right, source) {
        // right is typeof(x), left should be TYPE_*
        let type_text = &source[left.byte_range()];
        (arg, type_text)
    } else {
        return;
    };

    let Some(type_name) = type_constant_to_type(type_constant) else {
        return;
    };

    let replacement = if op == "==" {
        format!("{typeof_arg} is {type_name}")
    } else {
        format!("not {typeof_arg} is {type_name}")
    };

    let full_text = &source[node.byte_range()];

    diags.push(LintDiagnostic {
        rule: "prefer-is-instance",
        message: format!("use `{replacement}` instead of `{full_text}`"),
        severity: Severity::Warning,
        line: node.start_position().row,
        column: node.start_position().column,
        end_column: Some(node.end_position().column),
        fix: Some(Fix {
            byte_start: node.start_byte(),
            byte_end: node.end_byte(),
            replacement,
        }),
        context_lines: None,
    });
}

/// Extract the argument from a `typeof(x)` call.
fn extract_typeof_arg<'a>(node: Node<'a>, source: &'a str) -> Option<&'a str> {
    if node.kind() != "call" {
        return None;
    }

    let src = source.as_bytes();
    let callee = node
        .children(&mut node.walk())
        .find(|c| c.kind() == "identifier")
        .and_then(|n| n.utf8_text(src).ok())?;

    if callee != "typeof" {
        return None;
    }

    let args = node.child_by_field_name("arguments")?;
    let mut named = Vec::new();
    let mut cursor = args.walk();
    if cursor.goto_first_child() {
        loop {
            let child = cursor.node();
            if child.is_named() {
                named.push(child);
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    if named.len() != 1 {
        return None;
    }

    Some(&source[named[0].byte_range()])
}

/// Map TYPE_* constants to their GDScript type names.
fn type_constant_to_type(constant: &str) -> Option<&'static str> {
    match constant {
        "TYPE_BOOL" => Some("bool"),
        "TYPE_INT" => Some("int"),
        "TYPE_FLOAT" => Some("float"),
        "TYPE_STRING" => Some("String"),
        "TYPE_VECTOR2" => Some("Vector2"),
        "TYPE_VECTOR2I" => Some("Vector2i"),
        "TYPE_VECTOR3" => Some("Vector3"),
        "TYPE_VECTOR3I" => Some("Vector3i"),
        "TYPE_VECTOR4" => Some("Vector4"),
        "TYPE_VECTOR4I" => Some("Vector4i"),
        "TYPE_RECT2" => Some("Rect2"),
        "TYPE_RECT2I" => Some("Rect2i"),
        "TYPE_TRANSFORM2D" => Some("Transform2D"),
        "TYPE_TRANSFORM3D" => Some("Transform3D"),
        "TYPE_PLANE" => Some("Plane"),
        "TYPE_QUATERNION" => Some("Quaternion"),
        "TYPE_AABB" => Some("AABB"),
        "TYPE_BASIS" => Some("Basis"),
        "TYPE_PROJECTION" => Some("Projection"),
        "TYPE_COLOR" => Some("Color"),
        "TYPE_ARRAY" => Some("Array"),
        "TYPE_DICTIONARY" => Some("Dictionary"),
        "TYPE_NODE_PATH" => Some("NodePath"),
        "TYPE_RID" => Some("RID"),
        "TYPE_OBJECT" => Some("Object"),
        "TYPE_STRING_NAME" => Some("StringName"),
        "TYPE_PACKED_BYTE_ARRAY" => Some("PackedByteArray"),
        "TYPE_PACKED_INT32_ARRAY" => Some("PackedInt32Array"),
        "TYPE_PACKED_INT64_ARRAY" => Some("PackedInt64Array"),
        "TYPE_PACKED_FLOAT32_ARRAY" => Some("PackedFloat32Array"),
        "TYPE_PACKED_FLOAT64_ARRAY" => Some("PackedFloat64Array"),
        "TYPE_PACKED_STRING_ARRAY" => Some("PackedStringArray"),
        "TYPE_PACKED_VECTOR2_ARRAY" => Some("PackedVector2Array"),
        "TYPE_PACKED_VECTOR3_ARRAY" => Some("PackedVector3Array"),
        "TYPE_PACKED_VECTOR4_ARRAY" => Some("PackedVector4Array"),
        "TYPE_PACKED_COLOR_ARRAY" => Some("PackedColorArray"),
        "TYPE_SIGNAL" => Some("Signal"),
        "TYPE_CALLABLE" => Some("Callable"),
        _ => None,
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
        PreferIsInstance.check(&file, source, &config)
    }

    fn apply_fix(source: &str, fix: &Fix) -> String {
        format!(
            "{}{}{}",
            &source[..fix.byte_start],
            &fix.replacement,
            &source[fix.byte_end..]
        )
    }

    #[test]
    fn detects_typeof_eq_type_string() {
        let source = "func f(x):\n\tif typeof(x) == TYPE_STRING:\n\t\tpass\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("x is String"));
    }

    #[test]
    fn detects_typeof_eq_type_int() {
        let source = "func f(x):\n\tif typeof(x) == TYPE_INT:\n\t\tpass\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("x is int"));
    }

    #[test]
    fn detects_typeof_eq_type_bool() {
        let source = "func f(x):\n\tif typeof(x) == TYPE_BOOL:\n\t\tpass\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("x is bool"));
    }

    #[test]
    fn detects_reversed_operand_order() {
        let source = "func f(x):\n\tif TYPE_INT == typeof(x):\n\t\tpass\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("x is int"));
    }

    #[test]
    fn detects_not_equals() {
        let source = "func f(x):\n\tif typeof(x) != TYPE_STRING:\n\t\tpass\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("not x is String"));
    }

    #[test]
    fn detects_packed_types() {
        let source = "func f(x):\n\tif typeof(x) == TYPE_PACKED_BYTE_ARRAY:\n\t\tpass\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("x is PackedByteArray"));
    }

    #[test]
    fn detects_vector_types() {
        let source = "func f(x):\n\tif typeof(x) == TYPE_VECTOR2:\n\t\tpass\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("x is Vector2"));
    }

    #[test]
    fn no_warning_unknown_type_constant() {
        let source = "func f(x):\n\tif typeof(x) == TYPE_UNKNOWN:\n\t\tpass\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_non_typeof_call() {
        let source = "func f(x):\n\tif get_type(x) == TYPE_STRING:\n\t\tpass\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_non_comparison() {
        let source = "func f(x):\n\tvar t = typeof(x)\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn fix_typeof_eq() {
        let source = "func f(x):\n\tif typeof(x) == TYPE_STRING:\n\t\tpass\n";
        let diags = check(source);
        let fix = diags[0].fix.as_ref().unwrap();
        let fixed = apply_fix(source, fix);
        assert!(fixed.contains("x is String"));
        assert!(!fixed.contains("typeof"));
    }

    #[test]
    fn fix_typeof_neq() {
        let source = "func f(x):\n\tif typeof(x) != TYPE_STRING:\n\t\tpass\n";
        let diags = check(source);
        let fix = diags[0].fix.as_ref().unwrap();
        let fixed = apply_fix(source, fix);
        assert!(fixed.contains("not x is String"));
    }

    #[test]
    fn fix_reversed_order() {
        let source = "func f(x):\n\tif TYPE_INT == typeof(x):\n\t\tpass\n";
        let diags = check(source);
        let fix = diags[0].fix.as_ref().unwrap();
        let fixed = apply_fix(source, fix);
        assert!(fixed.contains("x is int"));
    }

    #[test]
    fn detects_complex_expression_arg() {
        let source = "func f(x):\n\tif typeof(items[0]) == TYPE_STRING:\n\t\tpass\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("items[0] is String"));
    }

    #[test]
    fn opt_in_rule() {
        assert!(!PreferIsInstance.default_enabled());
    }
}
