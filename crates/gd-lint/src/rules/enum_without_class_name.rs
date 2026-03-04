use gd_core::gd_ast::{self, GdDecl, GdFile, GdStmt, GdTypeRef};

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use gd_core::config::LintConfig;

pub struct EnumWithoutClassName;

impl LintRule for EnumWithoutClassName {
    fn name(&self) -> &'static str {
        "enum-without-class-name"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Godot
    }

    fn default_enabled(&self) -> bool {
        false
    }

    fn check(&self, file: &GdFile<'_>, _source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        // If the file has a class_name, enum types resolve fine
        if file.class_name.is_some() {
            return Vec::new();
        }

        // Collect enum names defined at file scope
        let enum_names: Vec<&str> = file
            .declarations
            .iter()
            .filter_map(|d| {
                if let GdDecl::Enum(e) = d {
                    if e.name.is_empty() {
                        None
                    } else {
                        Some(e.name)
                    }
                } else {
                    None
                }
            })
            .collect();

        if enum_names.is_empty() {
            return Vec::new();
        }

        // Scan all type annotations for references to these enum names
        let mut diags = Vec::new();

        // Check declaration-level type annotations (vars, func return types, params)
        gd_ast::visit_decls(file, &mut |decl| match decl {
            GdDecl::Var(var) => {
                check_type_ref(var.type_ann.as_ref(), &enum_names, &mut diags);
            }
            GdDecl::Func(func) => {
                check_type_ref(func.return_type.as_ref(), &enum_names, &mut diags);
                for param in &func.params {
                    check_type_ref(param.type_ann.as_ref(), &enum_names, &mut diags);
                }
            }
            _ => {}
        });

        // Check local variable type annotations in function bodies
        gd_ast::visit_stmts(file, &mut |stmt| {
            if let GdStmt::Var(var) = stmt {
                check_type_ref(var.type_ann.as_ref(), &enum_names, &mut diags);
            }
        });

        diags
    }
}

fn check_type_ref(
    type_ann: Option<&GdTypeRef<'_>>,
    enum_names: &[&str],
    diags: &mut Vec<LintDiagnostic>,
) {
    if let Some(type_ref) = type_ann
        && !type_ref.is_inferred
        && enum_names.contains(&type_ref.name)
    {
        diags.push(LintDiagnostic {
            rule: "enum-without-class-name",
            message: format!(
                "type annotation `{}` won't resolve — script defines enum `{}` but has no `class_name`; add `class_name` to fix",
                type_ref.name, type_ref.name,
            ),
            severity: Severity::Warning,
            line: type_ref.node.start_position().row,
            column: type_ref.node.start_position().column,
            end_column: Some(type_ref.node.end_position().column),
            fix: None,
            context_lines: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gd_core::gd_ast;
    use gd_core::parser;

    fn check(source: &str) -> Vec<LintDiagnostic> {
        let tree = parser::parse(source).unwrap();
        let file = gd_ast::convert(&tree, source);
        let config = LintConfig::default();
        EnumWithoutClassName.check(&file, source, &config)
    }

    #[test]
    fn detects_enum_type_annotation_without_class_name() {
        let source = "\
enum LobbyState { WAITING, PLAYING }
var lobby_state: LobbyState
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("LobbyState"));
        assert!(diags[0].message.contains("class_name"));
    }

    #[test]
    fn no_warning_with_class_name() {
        let source = "\
class_name Lobby
enum LobbyState { WAITING, PLAYING }
var lobby_state: LobbyState
";
        let diags = check(source);
        assert!(diags.is_empty());
    }

    #[test]
    fn no_warning_enum_not_used_in_annotation() {
        let source = "\
enum LobbyState { WAITING, PLAYING }
var x := 42
";
        let diags = check(source);
        assert!(diags.is_empty());
    }

    #[test]
    fn detects_in_function_parameter() {
        let source = "\
enum LobbyState { WAITING, PLAYING }
func set_state(state: LobbyState) -> void:
\tpass
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("LobbyState"));
    }

    #[test]
    fn detects_in_return_type() {
        let source = "\
enum LobbyState { WAITING, PLAYING }
func get_state() -> LobbyState:
\treturn LobbyState.WAITING
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("LobbyState"));
    }

    #[test]
    fn opt_in_rule() {
        assert!(!EnumWithoutClassName.default_enabled());
    }
}
