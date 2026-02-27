use std::collections::HashSet;

use crate::core::gd_ast::{GdDecl, GdFile};

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct UnusedParameter;

impl LintRule for UnusedParameter {
    fn name(&self) -> &'static str {
        "unused-parameter"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Maintenance
    }

    fn default_enabled(&self) -> bool {
        false
    }

    fn check(&self, file: &GdFile<'_>, source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        check_decls(&file.declarations, source.as_bytes(), &mut diags);
        diags
    }
}

fn check_decls(decls: &[GdDecl<'_>], src: &[u8], diags: &mut Vec<LintDiagnostic>) {
    for decl in decls {
        if let GdDecl::Func(func) = decl {
            if func.params.is_empty() {
                continue;
            }

            // Collect all identifier references in the function body (raw tree-sitter)
            let Some(body_node) = func.node.child_by_field_name("body") else {
                continue;
            };
            let mut references: HashSet<String> = HashSet::new();
            collect_references(body_node, src, &mut references);

            // Report unused parameters
            let mut unused: Vec<_> = func
                .params
                .iter()
                .filter(|p| !p.name.starts_with('_') && !references.contains(p.name))
                .collect();
            unused.sort_by_key(|p| (p.node.start_position().row, p.node.start_position().column));

            for param in unused {
                diags.push(LintDiagnostic {
                    rule: "unused-parameter",
                    message: format!(
                        "parameter `{}` is never used; prefix with `_` if intentional",
                        param.name
                    ),
                    severity: Severity::Warning,
                    line: param.node.start_position().row,
                    column: param.node.start_position().column,
                    end_column: Some(param.node.start_position().column + param.name.len()),
                    fix: None,
                    context_lines: None,
                });
            }
        }
        if let GdDecl::Class(class) = decl {
            check_decls(&class.declarations, src, diags);
        }
    }
}

/// Collect all identifier references using raw tree-sitter traversal.
/// Skips nested function definitions and lambdas (separate scope).
fn collect_references(node: tree_sitter::Node<'_>, src: &[u8], references: &mut HashSet<String>) {
    if node.kind() == "identifier"
        && let Ok(text) = node.utf8_text(src)
        && !text.is_empty()
    {
        references.insert(text.to_string());
    }

    // Don't recurse into nested function definitions or lambdas (separate scope)
    if node.kind() == "function_definition" || node.kind() == "lambda" {
        return;
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            collect_references(cursor.node(), src, references);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
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
        UnusedParameter.check(&file, source, &config)
    }

    #[test]
    fn detects_unused_parameter() {
        let source = "func f(x: int, y: int) -> int:\n\treturn x\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule, "unused-parameter");
        assert!(diags[0].message.contains("`y`"));
    }

    #[test]
    fn no_warning_when_all_used() {
        let source = "func add(x: int, y: int) -> int:\n\treturn x + y\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn skips_underscore_prefixed() {
        let source = "func f(_unused: int) -> void:\n\tpass\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn detects_multiple_unused() {
        let source = "func f(a: int, b: int, c: int) -> void:\n\tpass\n";
        let diags = check(source);
        assert_eq!(diags.len(), 3);
    }

    #[test]
    fn no_warning_for_no_params() {
        let source = "func f() -> void:\n\tpass\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn parameter_used_in_nested_expression() {
        let source = "func f(x: int) -> int:\n\tvar result := x * 2 + 1\n\treturn result\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn parameter_used_in_method_call() {
        let source = "func f(msg: String) -> void:\n\tprint(msg)\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn is_opt_in_rule() {
        assert!(!UnusedParameter.default_enabled());
    }

    #[test]
    fn lambda_capture_flagged_as_unused() {
        let source = "func f(x: int) -> void:\n\tvar fn := func(): return x\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn detects_unused_delta_in_process() {
        let source = "func _process(delta: float) -> void:\n\tprint(\"hello\")\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("`delta`"));
    }

    #[test]
    fn no_warning_delta_used() {
        let source = "func _process(delta: float) -> void:\n\tposition.x += 100 * delta\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn parameter_used_in_conditional() {
        let source = "func f(x: int) -> String:\n\tif x > 0:\n\t\treturn \"positive\"\n\treturn \"non-positive\"\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn end_column_set_correctly() {
        let source = "func f(x: int, y: int) -> int:\n\treturn x\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].end_column, Some(diags[0].column + 1)); // "y" is 1 char
    }
}
