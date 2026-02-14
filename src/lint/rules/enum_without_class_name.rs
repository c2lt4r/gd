use tree_sitter::{Node, Tree};

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

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

    fn check(&self, tree: &Tree, source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let root = tree.root_node();
        let bytes = source.as_bytes();

        // Pass 1: scan top-level for class_name and enum definitions
        let mut has_class_name = false;
        let mut enum_names: Vec<String> = Vec::new();

        let mut cursor = root.walk();
        for child in root.children(&mut cursor) {
            match child.kind() {
                "class_name_statement" => has_class_name = true,
                "enum_definition" => {
                    if let Some(name_node) = child.child_by_field_name("name")
                        && let Ok(name) = name_node.utf8_text(bytes)
                    {
                        enum_names.push(name.to_string());
                    }
                }
                _ => {}
            }
        }

        if has_class_name || enum_names.is_empty() {
            return Vec::new();
        }

        // Pass 2: find type annotations that reference the enum names
        let mut diags = Vec::new();
        find_enum_annotations(root, bytes, &enum_names, &mut diags);
        diags
    }
}

fn find_enum_annotations(
    node: Node,
    source: &[u8],
    enum_names: &[String],
    diags: &mut Vec<LintDiagnostic>,
) {
    match node.kind() {
        "variable_statement" => {
            if let Some(type_node) = node.child_by_field_name("type")
                && type_node.kind() != "inferred_type"
                && let Ok(type_text) = type_node.utf8_text(source)
                && enum_names.iter().any(|e| e == type_text)
            {
                emit(diags, type_text, &type_node);
            }
        }
        "typed_parameter" => {
            if let Some(type_node) = node.child_by_field_name("type")
                && let Ok(type_text) = type_node.utf8_text(source)
                && enum_names.iter().any(|e| e == type_text)
            {
                emit(diags, type_text, &type_node);
            }
        }
        "function_definition" => {
            if let Some(ret_node) = node.child_by_field_name("return_type")
                && let Ok(type_text) = ret_node.utf8_text(source)
                && enum_names.iter().any(|e| e == type_text)
            {
                emit(diags, type_text, &ret_node);
            }
        }
        _ => {}
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            find_enum_annotations(cursor.node(), source, enum_names, diags);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

fn emit(diags: &mut Vec<LintDiagnostic>, type_text: &str, node: &Node) {
    diags.push(LintDiagnostic {
        rule: "enum-without-class-name",
        message: format!(
            "type annotation `{type_text}` won't resolve — script defines enum `{type_text}` but has no `class_name`; add `class_name` to fix"
        ),
        severity: Severity::Warning,
        line: node.start_position().row,
        column: node.start_position().column,
        end_column: Some(node.end_position().column),
        fix: None,
        context_lines: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::parser;

    fn check(source: &str) -> Vec<LintDiagnostic> {
        let tree = parser::parse(source).unwrap();
        let config = LintConfig::default();
        EnumWithoutClassName.check(&tree, source, &config)
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
