use tree_sitter::{Node, Tree};

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;
use crate::core::symbol_table::SymbolTable;
use crate::core::type_inference::{InferredType, infer_expression_type_with_project};
use crate::core::workspace_index::ProjectIndex;

pub struct UntypedArrayArgument;

impl LintRule for UntypedArrayArgument {
    fn name(&self) -> &'static str {
        "untyped-array-argument"
    }

    fn category(&self) -> LintCategory {
        LintCategory::TypeSafety
    }

    fn check(&self, _tree: &Tree, _source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        Vec::new()
    }

    fn check_with_project(
        &self,
        tree: &Tree,
        source: &str,
        _config: &LintConfig,
        symbols: &SymbolTable,
        project: &ProjectIndex,
    ) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        check_node(tree.root_node(), source, symbols, project, &mut diags);
        diags
    }
}

fn check_node(
    node: Node,
    source: &str,
    symbols: &SymbolTable,
    project: &ProjectIndex,
    diags: &mut Vec<LintDiagnostic>,
) {
    if node.kind() == "call" {
        check_call(&node, source, symbols, project, diags);
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        check_node(child, source, symbols, project, diags);
    }
}

fn check_call(
    node: &Node,
    source: &str,
    symbols: &SymbolTable,
    project: &ProjectIndex,
    diags: &mut Vec<LintDiagnostic>,
) {
    let src = source.as_bytes();

    // Get function name (plain calls only — `func_name(args)`)
    let Some(func_name_node) = node
        .children(&mut node.walk())
        .find(|c| c.kind() == "identifier")
    else {
        return;
    };
    let Ok(func_name) = func_name_node.utf8_text(src) else {
        return;
    };

    // Get arguments node
    let Some(args_node) = node
        .children(&mut node.walk())
        .find(|c| c.kind() == "arguments")
    else {
        return;
    };

    // Collect argument expression nodes
    let args: Vec<Node> = args_node
        .children(&mut args_node.walk())
        .filter(|c| c.is_named() && c.kind() != "comment")
        .collect();

    // Resolve expected parameter types
    let param_types = resolve_param_types(func_name, symbols, project);
    if param_types.is_empty() {
        return;
    }

    for (i, arg) in args.iter().enumerate() {
        let Some(expected) = param_types.get(i) else {
            break;
        };
        let Some(expected_type) = expected else {
            continue;
        };

        // Only check Array[T] parameters
        let Some(expected_element) = parse_array_element_type(expected_type) else {
            continue;
        };

        // Infer argument type — try type inference first, then local variable lookup
        let arg_type = infer_expression_type_with_project(arg, source, symbols, project)
            .or_else(|| resolve_local_type(arg, source));

        let Some(arg_type) = arg_type else {
            continue;
        };

        match &arg_type {
            // Untyped Array passed to typed Array[T]
            InferredType::Builtin("Array") => {
                // Skip empty array literals — Godot handles these fine
                let Ok(arg_text) = arg.utf8_text(src) else {
                    continue;
                };
                if arg_text.trim() == "[]" {
                    continue;
                }
                diags.push(LintDiagnostic {
                    rule: "untyped-array-argument",
                    message: format!(
                        "passing untyped `Array` to parameter expecting `Array[{expected_element}]`"
                    ),
                    severity: Severity::Warning,
                    line: arg.start_position().row,
                    column: arg.start_position().column,
                    end_column: Some(arg.end_position().column),
                    fix: None,
                    context_lines: None,
                });
            }
            // Typed Array[X] passed to Array[T] where X != T
            InferredType::TypedArray(inner) => {
                let actual_element = inner.display_name();
                if actual_element != expected_element {
                    diags.push(LintDiagnostic {
                        rule: "untyped-array-argument",
                        message: format!(
                            "passing `Array[{actual_element}]` to parameter expecting `Array[{expected_element}]`"
                        ),
                        severity: Severity::Warning,
                        line: arg.start_position().row,
                        column: arg.start_position().column,
                        end_column: Some(arg.end_position().column),
                        fix: None,
                        context_lines: None,
                    });
                }
            }
            _ => {}
        }
    }
}

/// Resolve the type of an identifier by looking up its local variable declaration
/// in the enclosing function body. Returns `InferredType` based on the type annotation.
fn resolve_local_type(node: &Node, source: &str) -> Option<InferredType> {
    if node.kind() != "identifier" {
        return None;
    }
    let src = source.as_bytes();
    let name = node.utf8_text(src).ok()?;

    // Find enclosing function
    let func = find_enclosing_function(node)?;
    let body = func.child_by_field_name("body")?;
    let target_line = node.start_position().row;

    // Search for `var <name>: <type>` before the target line
    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        if child.start_position().row >= target_line {
            break;
        }
        if child.kind() == "variable_statement"
            && let Some(name_node) = child.child_by_field_name("name")
            && let Ok(var_name) = name_node.utf8_text(src)
            && var_name == name
            && let Some(type_node) = child.child_by_field_name("type")
            && type_node.kind() != "inferred_type"
            && let Ok(type_text) = type_node.utf8_text(src)
        {
            return Some(classify_array_type(type_text));
        }
    }
    None
}

/// Walk up the tree to find the enclosing function definition.
fn find_enclosing_function<'a>(node: &'a Node<'a>) -> Option<Node<'a>> {
    let mut current = node.parent()?;
    loop {
        if current.kind() == "function_definition" || current.kind() == "constructor_definition" {
            return Some(current);
        }
        current = current.parent()?;
    }
}

/// Classify a type annotation string into an `InferredType`.
fn classify_array_type(type_name: &str) -> InferredType {
    if let Some(element) = parse_array_element_type(type_name) {
        InferredType::TypedArray(Box::new(classify_array_type(element)))
    } else if type_name == "Array" {
        InferredType::Builtin("Array")
    } else {
        // For non-array types, use Class as a generic bucket
        InferredType::Class(type_name.to_string())
    }
}

/// Resolve parameter types for a function by name.
/// Returns Vec of `Option<String>` where `None` means untyped parameter.
fn resolve_param_types(
    func_name: &str,
    symbols: &SymbolTable,
    project: &ProjectIndex,
) -> Vec<Option<String>> {
    // Same-file functions first
    for func in &symbols.functions {
        if func.name == func_name {
            return func
                .params
                .iter()
                .map(|p| {
                    p.type_ann
                        .as_ref()
                        .filter(|t| !t.is_inferred && !t.name.is_empty())
                        .map(|t| t.name.clone())
                })
                .collect();
        }
    }

    // Cross-file via ProjectIndex
    for file in project.files() {
        for func in &file.functions {
            if func.name == func_name {
                return func.params.iter().map(|p| p.type_name.clone()).collect();
            }
        }
    }

    Vec::new()
}

/// Parse `Array[ElementType]` and return the element type string.
fn parse_array_element_type(type_name: &str) -> Option<&str> {
    let rest = type_name.strip_prefix("Array[")?;
    let element = rest.strip_suffix(']')?;
    if element.is_empty() {
        None
    } else {
        Some(element)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::workspace_index;
    use crate::core::{parser, symbol_table};
    use std::path::PathBuf;

    fn check(source: &str) -> Vec<LintDiagnostic> {
        check_with_files(source, &[])
    }

    fn check_with_files(source: &str, project_files: &[(&str, &str)]) -> Vec<LintDiagnostic> {
        let root = PathBuf::from("/test_project");
        let file_entries: Vec<(PathBuf, &str)> = project_files
            .iter()
            .map(|(name, src)| (root.join(name), *src))
            .collect();
        let project = workspace_index::build_from_sources(&root, &file_entries, &[]);

        let tree = parser::parse(source).unwrap();
        let symbols = symbol_table::build(&tree, source);
        let config = LintConfig::default();
        UntypedArrayArgument.check_with_project(&tree, source, &config, &symbols, &project)
    }

    #[test]
    fn detects_untyped_array_to_typed_param() {
        let source = "\
extends Node
func process_items(items: Array[Dictionary]) -> void:
\tpass
func f():
\tvar data: Array = []
\tprocess_items(data)
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("Array[Dictionary]"));
        assert!(diags[0].message.contains("untyped"));
    }

    #[test]
    fn detects_element_type_mismatch() {
        let source = "\
extends Node
func process_items(items: Array[String]) -> void:
\tpass
func f():
\tvar data: Array[int] = [1, 2, 3]
\tprocess_items(data)
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("Array[int]"));
        assert!(diags[0].message.contains("Array[String]"));
    }

    #[test]
    fn no_warning_matching_typed_array() {
        let source = "\
extends Node
func process_items(items: Array[Dictionary]) -> void:
\tpass
func f():
\tvar data: Array[Dictionary] = []
\tprocess_items(data)
";
        let diags = check(source);
        assert!(diags.is_empty());
    }

    #[test]
    fn no_warning_untyped_param() {
        let source = "\
extends Node
func process_items(items) -> void:
\tpass
func f():
\tvar data = [1, 2]
\tprocess_items(data)
";
        let diags = check(source);
        assert!(diags.is_empty());
    }

    #[test]
    fn no_warning_non_array_argument() {
        let source = "\
extends Node
func process_items(items: Array[Dictionary]) -> void:
\tpass
func f():
\tvar count: int = 5
\tprocess_items(count)
";
        let diags = check(source);
        assert!(diags.is_empty());
    }

    #[test]
    fn no_warning_empty_array_literal() {
        let source = "\
extends Node
func process_items(items: Array[Dictionary]) -> void:
\tpass
func f():
\tprocess_items([])
";
        let diags = check(source);
        assert!(diags.is_empty());
    }

    #[test]
    fn detects_class_var_untyped_array() {
        let source = "\
extends Node
var data: Array = []
func process_items(items: Array[Dictionary]) -> void:
\tpass
func f():
\tprocess_items(data)
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("untyped"));
    }

    #[test]
    fn default_enabled() {
        assert!(UntypedArrayArgument.default_enabled());
    }
}
