use crate::core::gd_ast::{self, GdExpr, GdFile};

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
        gd_ast::visit_exprs(file, &mut |expr| {
            if let GdExpr::BinOp {
                node,
                op,
                left,
                right,
            } = expr
                && (*op == "==" || *op == "!=")
            {
                // Try both orders: typeof(x) == TYPE_* and TYPE_* == typeof(x)
                let result = extract_typeof_and_type(left, right, source)
                    .or_else(|| extract_typeof_and_type(right, left, source));

                if let Some((typeof_arg, type_constant)) = result
                    && let Some(type_name) = type_constant_to_type(type_constant)
                {
                    let replacement = if *op == "==" {
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
            }
        });
        diags
    }
}

/// Extract (typeof_argument_text, type_constant_text) if `call_side` is `typeof(x)`.
fn extract_typeof_and_type<'a>(
    call_side: &GdExpr<'a>,
    type_side: &GdExpr<'a>,
    source: &'a str,
) -> Option<(&'a str, &'a str)> {
    if let GdExpr::Call { callee, args, .. } = call_side
        && let GdExpr::Ident { name: "typeof", .. } = callee.as_ref()
        && args.len() == 1
    {
        let arg_text = &source[args[0].node().byte_range()];
        let type_text = &source[type_side.node().byte_range()];
        Some((arg_text, type_text))
    } else {
        None
    }
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
    use crate::core::gd_ast;
    use crate::core::parser;

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
