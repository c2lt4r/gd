use crate::core::gd_ast::{GdClass, GdDecl, GdFile, GdFunc};

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct NativeMethodOverride;

impl LintRule for NativeMethodOverride {
    fn name(&self) -> &'static str {
        "native-method-override"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Suspicious
    }

    fn default_enabled(&self) -> bool {
        false
    }

    fn check(
        &self,
        _file: &GdFile<'_>,
        _source: &str,
        _config: &LintConfig,
    ) -> Vec<LintDiagnostic> {
        Vec::new()
    }

    fn check_with_symbols(
        &self,
        file: &GdFile<'_>,
        _source: &str,
        _config: &LintConfig,
    ) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        if let Some(extends) = file.extends_class() {
            check_funcs(file.funcs(), extends, &mut diags);
        }
        for inner in file.inner_classes() {
            check_inner_class(inner, &mut diags);
        }
        diags
    }
}

/// Normalize a ClassDB type for comparison with a GDScript annotation.
/// Strips `enum::` prefix and class-qualified enum names.
fn normalize_type(db_type: &str) -> &str {
    if let Some(rest) = db_type.strip_prefix("enum::") {
        // "enum::Node.InternalMode" → "InternalMode"
        rest.rsplit('.').next().unwrap_or(rest)
    } else {
        db_type
    }
}

fn check_inner_class(class: &GdClass, diags: &mut Vec<LintDiagnostic>) {
    let extends = match &class.extends {
        Some(crate::core::gd_ast::GdExtends::Class(c)) => *c,
        _ => return,
    };
    let funcs = class.declarations.iter().filter_map(GdDecl::as_func);
    check_funcs(funcs, extends, diags);
}

fn check_funcs<'a>(
    funcs: impl Iterator<Item = &'a GdFunc<'a>>,
    extends: &str,
    diags: &mut Vec<LintDiagnostic>,
) {
    for func in funcs {
        // Skip virtual methods (prefixed with _) — these are meant to be overridden
        if func.name.starts_with('_') {
            continue;
        }

        let Some(native_sig) = crate::class_db::method_signature(extends, func.name) else {
            continue;
        };

        // Check for signature mismatches
        let mut mismatches = Vec::new();

        // Parameter count: user must provide at least required_params
        let user_params = func.params.len();
        let native_required = native_sig.required_params as usize;
        let native_total = native_sig.total_params as usize;
        if user_params < native_required || user_params > native_total {
            mismatches.push(format!(
                "expects {} parameter{} (got {})",
                if native_required == native_total {
                    format!("{native_total}")
                } else {
                    format!("{native_required}..{native_total}")
                },
                if native_total == 1 { "" } else { "s" },
                user_params,
            ));
        }

        // Parameter types: compare typed params against native signature
        let native_types: Vec<&str> = if native_sig.param_types.is_empty() {
            Vec::new()
        } else {
            native_sig.param_types.split(',').collect()
        };

        for (i, param) in func.params.iter().enumerate() {
            if let Some(ref ann) = param.type_ann
                && !ann.is_inferred
                && let Some(&native_type) = native_types.get(i)
            {
                let normalized = normalize_type(native_type);
                if !ann.name.eq_ignore_ascii_case(normalized) && ann.name != native_type {
                    mismatches.push(format!(
                        "parameter `{}` should be `{}` (got `{}`)",
                        param.name, normalized, ann.name,
                    ));
                }
            }
        }

        // Return type: compare if user specified one
        if let Some(ref user_ret) = func.return_type {
            let native_ret = normalize_type(native_sig.return_type);
            if !user_ret.name.eq_ignore_ascii_case(native_ret)
                && user_ret.name != native_sig.return_type
            {
                mismatches.push(format!(
                    "should return `{}` (got `{}`)",
                    native_ret, user_ret.name,
                ));
            }
        }

        if mismatches.is_empty() {
            // Name matches but signature is compatible — still warn about shadowing
            diags.push(LintDiagnostic {
                rule: "native-method-override",
                message: format!(
                    "`{}()` overrides a native method from `{extends}` — this may cause unexpected behavior",
                    func.name
                ),
                severity: Severity::Error,
                line: func.node.start_position().row,
                column: 0,
                end_column: None,
                fix: None,
                context_lines: None,
            });
        } else {
            // Signature mismatch — higher severity, more helpful message
            let details = mismatches.join("; ");
            diags.push(LintDiagnostic {
                rule: "native-method-override",
                message: format!(
                    "`{}()` overrides native `{extends}.{}()` with incompatible signature: {details}",
                    func.name, func.name,
                ),
                severity: Severity::Error,
                line: func.node.start_position().row,
                column: 0,
                end_column: None,
                fix: None,
                context_lines: None,
            });
        }
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
        NativeMethodOverride.check_with_symbols(&file, source, &config)
    }

    #[test]
    fn detects_native_method_override() {
        // add_child is a method on Node
        let source = "extends Node\nfunc add_child(node):\n\tpass\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("add_child"));
        assert!(diags[0].message.contains("Node"));
        assert_eq!(diags[0].severity, Severity::Error);
    }

    #[test]
    fn detects_inherited_method_override() {
        // add_child is on Node, Node2D extends CanvasItem extends Node
        let source = "extends Node2D\nfunc add_child(node):\n\tpass\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("add_child"));
    }

    #[test]
    fn no_warning_for_virtual_methods() {
        // _ready is a virtual method meant to be overridden
        let source = "extends Node\nfunc _ready():\n\tpass\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_for_custom_methods() {
        let source = "extends Node\nfunc my_custom_method():\n\tpass\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_without_extends() {
        let source = "func add_child(node):\n\tpass\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_for_non_engine_class() {
        let source = "extends MyCustomClass\nfunc add_child(node):\n\tpass\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn opt_in_rule() {
        assert!(!NativeMethodOverride.default_enabled());
    }

    #[test]
    fn detects_param_type_mismatch() {
        // get_node(NodePath) but user declares get_node(String)
        let source = "extends Node\nfunc get_node(pool_name: String) -> Node:\n\treturn null\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("incompatible"));
        assert!(diags[0].message.contains("NodePath"));
        assert!(diags[0].message.contains("String"));
    }

    #[test]
    fn detects_return_type_mismatch() {
        // get_name() -> StringName but user declares -> String
        let source = "extends Node\nfunc get_name() -> String:\n\treturn \"test\"\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("incompatible"));
        assert!(diags[0].message.contains("StringName"));
        assert!(diags[0].message.contains("String"));
    }

    #[test]
    fn detects_param_count_mismatch() {
        // get_node takes 1 param, user provides 0
        let source = "extends Node\nfunc get_node() -> Node:\n\treturn null\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("incompatible"));
        assert!(diags[0].message.contains("parameter"));
    }

    #[test]
    fn compatible_signature_still_warns() {
        // add_child with matching required param — still warns about shadowing
        let source = "extends Node\nfunc add_child(node: Node):\n\tpass\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("overrides a native method"));
    }
}
