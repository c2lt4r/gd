use crate::core::gd_ast::{self, GdExpr, GdFile};

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct UnusedPrivateClassVariable;

impl LintRule for UnusedPrivateClassVariable {
    fn name(&self) -> &'static str {
        "unused-private-class-variable"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Maintenance
    }

    fn default_enabled(&self) -> bool {
        false
    }

    fn check(&self, _file: &GdFile<'_>, _source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        Vec::new()
    }

    fn check_with_symbols(
        &self,
        file: &GdFile<'_>,
        _source: &str,
        _config: &LintConfig,
    ) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();

        // Collect all identifier references in expression context
        let mut referenced = std::collections::HashSet::new();
        gd_ast::visit_exprs(file, &mut |expr| {
            if let GdExpr::Ident { name, .. } = expr {
                referenced.insert(*name);
            }
        });

        for var in file.vars() {
            if !var.name.starts_with('_') || var.is_const {
                continue;
            }
            if !referenced.contains(var.name) {
                diags.push(LintDiagnostic {
                    rule: "unused-private-class-variable",
                    message: format!(
                        "private variable `{}` is declared but never used in this file",
                        var.name
                    ),
                    severity: Severity::Warning,
                    line: var.node.start_position().row,
                    column: 0,
                    end_column: None,
                    fix: None,
                    context_lines: None,
                });
            }
        }

        diags
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
        UnusedPrivateClassVariable.check_with_symbols(&file, source, &config)
    }

    #[test]
    fn detects_unused_private_var() {
        let source = "var _unused: int = 0\nfunc f():\n\tpass\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("_unused"));
    }

    #[test]
    fn no_warning_when_used() {
        let source = "var _count: int = 0\nfunc f():\n\t_count += 1\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_public_var() {
        let source = "var unused_public: int = 0\nfunc f():\n\tpass\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_constant() {
        let source = "const _CONST: int = 42\nfunc f():\n\tpass\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_used_in_different_function() {
        let source = "var _hp: int = 100\nfunc take_damage(n: int):\n\t_hp -= n\nfunc get_hp() -> int:\n\treturn _hp\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn opt_in_rule() {
        assert!(!UnusedPrivateClassVariable.default_enabled());
    }
}
