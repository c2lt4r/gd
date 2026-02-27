use std::collections::HashMap;
use crate::core::gd_ast::{GdDecl, GdFile};

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct DuplicateFunction;

impl LintRule for DuplicateFunction {
    fn name(&self) -> &'static str {
        "duplicate-function"
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

fn check_scope(decls: &[GdDecl<'_>], diags: &mut Vec<LintDiagnostic>) {
    let mut functions: HashMap<&str, usize> = HashMap::new();

    for decl in decls {
        if let GdDecl::Func(func) = decl {
            let line = func.node.start_position().row;
            let name_node = func.node.child_by_field_name("name");
            let col = name_node.map_or(func.node.start_position().column, |n| n.start_position().column);

            if let Some(&first_line) = functions.get(func.name) {
                diags.push(LintDiagnostic {
                    rule: "duplicate-function",
                    message: format!(
                        "function `{}` already defined on line {}",
                        func.name,
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
                functions.insert(func.name, line);
            }
        }

        // Recurse into inner classes (separate scope)
        if let GdDecl::Class(class) = decl {
            check_scope(&class.declarations, diags);
        }
    }
}
