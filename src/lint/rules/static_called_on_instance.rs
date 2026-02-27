use tree_sitter::Node;
use crate::core::gd_ast::GdFile;

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;
use crate::core::symbol_table::SymbolTable;
use crate::core::workspace_index::ProjectIndex;

pub struct StaticCalledOnInstance;

impl LintRule for StaticCalledOnInstance {
    fn name(&self) -> &'static str {
        "static-called-on-instance"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Suspicious
    }

    fn check(&self, _file: &GdFile<'_>, _source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        Vec::new()
    }

    fn check_with_symbols(
        &self,
        file: &GdFile<'_>,
        source: &str,
        _config: &LintConfig,
        symbols: &SymbolTable,
    ) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        check_node(file.node, source, symbols, None, &mut diags);
        diags
    }

    fn check_with_project(
        &self,
        file: &GdFile<'_>,
        source: &str,
        _config: &LintConfig,
        symbols: &SymbolTable,
        project: &ProjectIndex,
    ) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        check_node(file.node, source, symbols, Some(project), &mut diags);
        diags
    }
}

fn check_node(
    node: Node,
    source: &str,
    symbols: &SymbolTable,
    project: Option<&ProjectIndex>,
    diags: &mut Vec<LintDiagnostic>,
) {
    // Look for `attribute` nodes with an `attribute_call` child (method calls)
    if node.kind() == "attribute" {
        let mut has_call = false;
        let mut method_name = None;
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "attribute_call" {
                has_call = true;
                if let Some(name_node) = child.named_child(0) {
                    method_name = name_node.utf8_text(source.as_bytes()).ok();
                }
            }
        }

        if has_call
            && let Some(method) = method_name
            && let Some(receiver) = node.named_child(0)
        {
            let receiver_text = receiver.utf8_text(source.as_bytes()).unwrap_or("");

            // Check if this is `self.static_method()` — same-file static
            if receiver_text == "self" {
                if symbols
                    .functions
                    .iter()
                    .any(|f| f.name == method && f.is_static)
                {
                    emit_diagnostic(method, receiver_text, diags, &node);
                }
            } else if let Some(proj) = project
                && let Some(class) = resolve_receiver_class(receiver_text, symbols)
                && proj.method_is_static(&class, method) == Some(true)
            {
                emit_diagnostic(method, receiver_text, diags, &node);
            }
        }
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            check_node(cursor.node(), source, symbols, project, diags);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

/// Try to resolve the class name of a receiver identifier from the symbol table.
fn resolve_receiver_class(receiver: &str, symbols: &SymbolTable) -> Option<String> {
    for var in &symbols.variables {
        if var.name == receiver {
            if let Some(ref type_ann) = var.type_ann
                && !type_ann.is_inferred
                && !type_ann.name.is_empty()
            {
                return Some(type_ann.name.clone());
            }
            return None;
        }
    }
    None
}

fn emit_diagnostic(method: &str, receiver: &str, diags: &mut Vec<LintDiagnostic>, node: &Node) {
    diags.push(LintDiagnostic {
        rule: "static-called-on-instance",
        message: format!(
            "static method `{method}()` called on instance `{receiver}` — call on the class instead"
        ),
        severity: Severity::Warning,
        line: node.start_position().row,
        column: node.start_position().column,
        end_column: None,
        fix: None,
        context_lines: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::workspace_index;
    use crate::core::gd_ast;
    use crate::core::{parser, symbol_table};
    use std::path::PathBuf;

    fn check_same_file(source: &str) -> Vec<LintDiagnostic> {
        let tree = parser::parse(source).unwrap();
        let file = gd_ast::convert(&tree, source);
        let symbols = symbol_table::build(&tree, source);
        let config = LintConfig::default();
        StaticCalledOnInstance.check_with_symbols(&file, source, &config, &symbols)
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
        let symbols = symbol_table::build(&tree, source);
        let config = LintConfig::default();
        StaticCalledOnInstance.check_with_project(&file, source, &config, &symbols, &project)
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
