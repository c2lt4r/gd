use crate::core::gd_ast::{GdDecl, GdFile};

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct EnumVariableWithoutDefault;

impl LintRule for EnumVariableWithoutDefault {
    fn name(&self) -> &'static str {
        "enum-variable-without-default"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Godot
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
        check_decls(&file.declarations, &mut diags);
        for inner in file.inner_classes() {
            check_decls(&inner.declarations, &mut diags);
        }
        diags
    }
}

fn check_decls(decls: &[GdDecl], diags: &mut Vec<LintDiagnostic>) {
    let enum_names: Vec<&str> = decls
        .iter()
        .filter_map(GdDecl::as_enum)
        .map(|e| e.name)
        .collect();

    for var in decls.iter().filter_map(GdDecl::as_var) {
        if var.value.is_some() || var.is_const {
            continue;
        }
        if let Some(ref ann) = var.type_ann
            && !ann.is_inferred
            && enum_names.contains(&ann.name)
        {
            diags.push(LintDiagnostic {
                rule: "enum-variable-without-default",
                message: format!(
                    "`{}` is typed as enum `{}` but has no default value — it will be `0`, not the first enum member",
                    var.name, ann.name
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
        EnumVariableWithoutDefault.check_with_symbols(&file, source, &config)
    }

    #[test]
    fn detects_enum_var_without_default() {
        let source = "enum State { IDLE, RUN }\nvar state: State\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("state"));
        assert!(diags[0].message.contains("State"));
    }

    #[test]
    fn no_warning_with_default() {
        let source = "enum State { IDLE, RUN }\nvar state: State = State.IDLE\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_non_enum_type() {
        let source = "enum State { IDLE, RUN }\nvar health: int\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_constant() {
        let source = "enum State { IDLE, RUN }\nconst DEFAULT: State = State.IDLE\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_untyped_var() {
        let source = "enum State { IDLE, RUN }\nvar state\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn opt_in_rule() {
        assert!(!EnumVariableWithoutDefault.default_enabled());
    }
}
