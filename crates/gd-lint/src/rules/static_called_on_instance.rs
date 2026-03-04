use gd_core::gd_ast::{self, GdExpr, GdFile};

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use gd_core::config::LintConfig;
use gd_core::workspace_index::ProjectIndex;

pub struct StaticCalledOnInstance;

impl LintRule for StaticCalledOnInstance {
    fn name(&self) -> &'static str {
        "static-called-on-instance"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Suspicious
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
        check_calls(file, None, &mut diags);
        diags
    }

    fn check_with_project(
        &self,
        file: &GdFile<'_>,
        _source: &str,
        _config: &LintConfig,
        project: &ProjectIndex,
    ) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        check_calls(file, Some(project), &mut diags);
        diags
    }
}

fn check_calls(file: &GdFile, project: Option<&ProjectIndex>, diags: &mut Vec<LintDiagnostic>) {
    gd_ast::visit_exprs(file, &mut |expr| {
        if let GdExpr::MethodCall {
            receiver, method, ..
        } = expr
        {
            let receiver_name = match receiver.as_ref() {
                GdExpr::Ident { name, .. } => *name,
                _ => return,
            };

            // Check `self.static_method()` — same-file static
            if receiver_name == "self" {
                if file.funcs().any(|f| f.name == *method && f.is_static) {
                    emit_diagnostic(method, receiver_name, diags, expr);
                }
            } else if let Some(proj) = project
                && let Some(class) = resolve_receiver_class(receiver_name, file)
                && proj.method_is_static(&class, method) == Some(true)
            {
                emit_diagnostic(method, receiver_name, diags, expr);
            }
        }
    });
}

/// Try to resolve the class name of a receiver identifier from the typed AST.
fn resolve_receiver_class(receiver: &str, file: &GdFile) -> Option<String> {
    for var in file.vars() {
        if var.name == receiver {
            if let Some(ref type_ann) = var.type_ann
                && !type_ann.is_inferred
                && !type_ann.name.is_empty()
            {
                return Some(type_ann.name.to_string());
            }
            return None;
        }
    }
    None
}

fn emit_diagnostic(method: &str, receiver: &str, diags: &mut Vec<LintDiagnostic>, expr: &GdExpr) {
    diags.push(LintDiagnostic {
        rule: "static-called-on-instance",
        message: format!(
            "static method `{method}()` called on instance `{receiver}` — call on the class instead"
        ),
        severity: Severity::Warning,
        line: expr.line(),
        column: expr.column(),
        end_column: None,
        fix: None,
        context_lines: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use gd_core::gd_ast;
    use gd_core::parser;
    use gd_core::workspace_index;
    use std::path::PathBuf;

    fn check_same_file(source: &str) -> Vec<LintDiagnostic> {
        let tree = parser::parse(source).unwrap();
        let file = gd_ast::convert(&tree, source);
        let config = LintConfig::default();
        StaticCalledOnInstance.check_with_symbols(&file, source, &config)
    }

    fn check_with_project(source: &str, project_files: &[(&str, &str)]) -> Vec<LintDiagnostic> {
        let root = PathBuf::from("/test_project");
        let file_entries: Vec<(PathBuf, &str)> = project_files
            .iter()
            .map(|(name, src)| (root.join(name), *src))
            .collect();
        let project = workspace_index::build_from_sources(&root, &file_entries, &[]);

        let tree = parser::parse(source).unwrap();
        let file = gd_ast::convert(&tree, source);
        let config = LintConfig::default();
        StaticCalledOnInstance.check_with_project(&file, source, &config, &project)
    }

    #[test]
    fn detects_self_static_call() {
        let source = "\
extends Node
static func create() -> Node:
\treturn Node.new()
func f():
\tself.create()
";
        let diags = check_same_file(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("create"));
        assert!(diags[0].message.contains("self"));
    }

    #[test]
    fn no_warning_for_non_static_self() {
        let source = "\
extends Node
func do_thing() -> void:
\tpass
func f():
\tself.do_thing()
";
        let diags = check_same_file(source);
        assert!(diags.is_empty());
    }

    #[test]
    fn detects_cross_file_static_on_instance() {
        let source = "\
extends Node
var factory: Factory
func f():
\tfactory.create()
";
        let diags = check_with_project(
            source,
            &[(
                "factory.gd",
                "class_name Factory\nextends Node\nstatic func create() -> Node:\n\treturn Node.new()\n",
            )],
        );
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("create"));
    }

    #[test]
    fn no_warning_for_non_static_cross_file() {
        let source = "\
extends Node
var factory: Factory
func f():
\tfactory.build()
";
        let diags = check_with_project(
            source,
            &[(
                "factory.gd",
                "class_name Factory\nextends Node\nfunc build() -> void:\n\tpass\n",
            )],
        );
        assert!(diags.is_empty());
    }

    #[test]
    fn default_enabled() {
        assert!(StaticCalledOnInstance.default_enabled());
    }
}
