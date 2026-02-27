use std::collections::HashSet;

use crate::core::gd_ast::{GdDecl, GdFile};

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct ParameterShadowsField;

impl LintRule for ParameterShadowsField {
    fn name(&self) -> &'static str {
        "parameter-shadows-field"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Style
    }

    fn check(&self, file: &GdFile<'_>, source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        check_scope(&file.declarations, source, &mut diags);
        diags
    }
}

fn check_scope(decls: &[GdDecl<'_>], source: &str, diags: &mut Vec<LintDiagnostic>) {
    // Collect field names at this scope level
    let fields: HashSet<&str> = decls
        .iter()
        .filter_map(|d| {
            if let GdDecl::Var(var) = d {
                Some(var.name)
            } else {
                None
            }
        })
        .collect();

    if !fields.is_empty() {
        // Check functions at this scope level
        for decl in decls {
            if let GdDecl::Func(func) = decl {
                // Skip static functions (no instance context)
                if func.is_static {
                    continue;
                }
                for param in &func.params {
                    if fields.contains(param.name) {
                        // Check if body uses self.field (intentional DI pattern)
                        let uses_self = body_uses_self_field(func.node, source.as_bytes(), param.name);
                        if !uses_self {
                            diags.push(LintDiagnostic {
                                rule: "parameter-shadows-field",
                                message: format!(
                                    "parameter `{}` shadows an instance variable",
                                    param.name
                                ),
                                severity: Severity::Warning,
                                line: param.node.start_position().row,
                                column: param.node.start_position().column,
                                end_column: Some(param.node.end_position().column),
                                fix: None,
                                context_lines: None,
                            });
                        }
                    }
                }
            }
        }
    }

    // Recurse into inner classes (separate scope)
    for decl in decls {
        if let GdDecl::Class(class) = decl {
            check_scope(&class.declarations, source, diags);
        }
    }
}

/// Check if the function body contains `self.<field_name>`.
/// Uses raw tree-sitter traversal since attribute access chains aren't fully in typed AST.
fn body_uses_self_field(func_node: tree_sitter::Node<'_>, src: &[u8], field_name: &str) -> bool {
    let Some(body) = func_node.child_by_field_name("body") else {
        return false;
    };
    scan_for_self_field(body, src, field_name)
}

fn scan_for_self_field(node: tree_sitter::Node<'_>, src: &[u8], field_name: &str) -> bool {
    if node.kind() == "attribute" {
        let mut cursor = node.walk();
        let children: Vec<_> = node.children(&mut cursor).collect();
        if let Some(first) = children.first()
            && first.kind() == "identifier"
            && first.utf8_text(src).ok() == Some("self")
        {
            for child in &children[1..] {
                if child.kind() == "identifier" && child.utf8_text(src).ok() == Some(field_name) {
                    return true;
                }
                if child.kind() == "attribute_call"
                    && let Some(id) = child
                        .children(&mut child.walk())
                        .find(|c| c.kind() == "identifier")
                    && id.utf8_text(src).ok() == Some(field_name)
                {
                    return true;
                }
            }
        }
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            if scan_for_self_field(cursor.node(), src, field_name) {
                return true;
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    false
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
        ParameterShadowsField.check(&file, source, &config)
    }

    #[test]
    fn detects_shadowing() {
        let source =
            "var speed: float = 10.0\n\nfunc set_speed(speed: float) -> void:\n\tspeed = speed\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule, "parameter-shadows-field");
        assert!(diags[0].message.contains("speed"));
    }

    #[test]
    fn no_warning_different_names() {
        let source = "var speed: float = 10.0\n\nfunc set_speed(new_speed: float) -> void:\n\tspeed = new_speed\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_when_self_used_in_constructor() {
        let source =
            "var health: int\n\nfunc _init(health: int) -> void:\n\tself.health = health\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn detects_in_constructor_without_self() {
        let source = "var health: int\n\nfunc _init(health: int) -> void:\n\thealth = health\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("health"));
    }

    #[test]
    fn no_warning_without_fields() {
        let source = "func f(x: int) -> void:\n\tprint(x)\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn detects_multiple_shadows() {
        let source = "var x: int\nvar y: int\n\nfunc f(x: int, y: int) -> void:\n\tpass\n";
        let diags = check(source);
        assert_eq!(diags.len(), 2);
    }

    #[test]
    fn inner_class_no_warning_with_self() {
        let source = "class Inner:\n\tvar value: int\n\n\tfunc set_value(value: int) -> void:\n\t\tself.value = value\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn inner_class_warns_without_self() {
        let source = "class Inner:\n\tvar value: int\n\n\tfunc set_value(value: int) -> void:\n\t\tvalue = value\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("value"));
    }

    #[test]
    fn no_cross_class_warning() {
        let source =
            "var speed: float\n\nclass Inner:\n\tfunc f(speed: float) -> void:\n\t\tpass\n";
        let diags = check(source);
        assert!(diags.is_empty());
    }

    #[test]
    fn no_warning_static_factory() {
        let source = "var blocker_id: int\nvar tick: int\n\nstatic func from_box(blocker_id: int, tick: int) -> void:\n\tvar record = DynamicBlockerRecord.new()\n\trecord.blocker_id = blocker_id\n\trecord.tick = tick\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn default_enabled() {
        assert!(ParameterShadowsField.default_enabled());
    }
}
