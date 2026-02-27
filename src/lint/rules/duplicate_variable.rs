use std::collections::HashMap;
use crate::core::gd_ast::{GdDecl, GdFile};

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct DuplicateVariable;

impl LintRule for DuplicateVariable {
    fn name(&self) -> &'static str {
        "duplicate-variable"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Correctness
    }

    fn check(&self, file: &GdFile<'_>, _source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        check_scope(&file.declarations, &mut diags);
        diags
    }
}

/// Check a single scope for duplicate variable names.
fn check_scope(decls: &[GdDecl<'_>], diags: &mut Vec<LintDiagnostic>) {
    let mut variables: HashMap<&str, usize> = HashMap::new();

    for decl in decls {
        if let GdDecl::Var(var) = decl {
            let line = var.node.start_position().row;
            let name_node = var.node.child_by_field_name("name");
            let col = name_node.map_or(var.node.start_position().column, |n| n.start_position().column);

            if let Some(&first_line) = variables.get(var.name) {
                diags.push(LintDiagnostic {
                    rule: "duplicate-variable",
                    message: format!(
                        "variable `{}` already declared on line {}",
                        var.name,
                        first_line + 1,
                    ),
                    severity: Severity::Error,
                    line,
                    column: col,
                    end_column: None,
                    fix: None,
                    context_lines: None,
                });
            } else {
                variables.insert(var.name, line);
            }
        }

        // Recurse into inner classes (separate scope)
        if let GdDecl::Class(class) = decl {
            check_scope(&class.declarations, diags);
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
        DuplicateVariable.check(&file, source, &config)
    }

    #[test]
    fn detects_duplicate_class_variable() {
        let source = "extends Control\nvar _ship_index: int = 0\nvar _name: Label\nvar _ship_index: int = 0\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("_ship_index"));
        assert!(diags[0].message.contains("line 2"));
        assert_eq!(diags[0].severity, Severity::Error);
    }

    #[test]
    fn detects_multiple_duplicates() {
        let source = "var a: int\nvar b: int\nvar a: int\nvar b: int\n";
        let diags = check(source);
        assert_eq!(diags.len(), 2);
    }

    #[test]
    fn no_warning_unique_variables() {
        let source = "extends Node\nvar health: int\nvar speed: float\nvar name: String\n";
        let diags = check(source);
        assert!(diags.is_empty());
    }

    #[test]
    fn no_warning_different_scopes() {
        let source = "var x: int\nclass Inner:\n\tvar x: int\n";
        let diags = check(source);
        assert!(diags.is_empty());
    }

    #[test]
    fn detects_duplicate_in_inner_class() {
        let source = "class Inner:\n\tvar x: int\n\tvar x: int\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn default_enabled() {
        assert!(DuplicateVariable.default_enabled());
    }
}
