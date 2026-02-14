use tree_sitter::{Node, Tree};

use super::{Fix, LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;
use crate::core::symbol_table::SymbolTable;
use crate::core::type_inference::{
    InferredType, infer_expression_type, infer_expression_type_with_project,
};
use crate::core::workspace_index::ProjectIndex;

pub struct UnsafeVoidReturn;

impl LintRule for UnsafeVoidReturn {
    fn name(&self) -> &'static str {
        "unsafe-void-return"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Suspicious
    }

    fn default_enabled(&self) -> bool {
        false
    }

    fn check(&self, _tree: &Tree, _source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        Vec::new()
    }

    fn check_with_symbols(
        &self,
        tree: &Tree,
        source: &str,
        _config: &LintConfig,
        symbols: &SymbolTable,
    ) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        check_node(tree.root_node(), source, symbols, None, &mut diags);
        diags
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
        check_node(tree.root_node(), source, symbols, Some(project), &mut diags);
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
    // Look for `return <expr>` where `<expr>` is a void function call
    if node.kind() == "return_statement"
        && let Some(expr) = node.named_child(0)
        && is_void_call(&expr, source, symbols, project)
    {
        let call_text = expr.utf8_text(source.as_bytes()).ok().unwrap_or("?");
        let display = if call_text.len() > 40 {
            format!("{}...", &call_text[..37])
        } else {
            call_text.to_string()
        };
        // Fix: `return void_call()` → `void_call()\n<indent>return`
        let indent = "\t".repeat(node.start_position().column / 4 + 1);
        let indent = if node.start_position().column > 0 {
            &source[node.start_byte() - node.start_position().column..node.start_byte()]
        } else {
            &indent
        };
        let fix = Some(Fix {
            byte_start: node.start_byte(),
            byte_end: node.end_byte(),
            replacement: format!("{call_text}\n{indent}return"),
        });
        diags.push(LintDiagnostic {
            rule: "unsafe-void-return",
            message: format!("returning void call `{display}` as a value"),
            severity: Severity::Warning,
            line: node.start_position().row,
            column: node.start_position().column,
            end_column: None,
            fix,
            context_lines: None,
        });
    }

    // Also check variable assignments: `var x = void_func()`
    if node.kind() == "variable_statement"
        && let Some(value) = node.child_by_field_name("value")
        && is_void_call(&value, source, symbols, project)
    {
        let var_name = node
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(source.as_bytes()).ok())
            .unwrap_or("?");
        // Fix: `var x = void_call()` → `void_call()`
        let call_text = value.utf8_text(source.as_bytes()).unwrap_or("");
        let fix = if call_text.is_empty() {
            None
        } else {
            Some(Fix {
                byte_start: node.start_byte(),
                byte_end: node.end_byte(),
                replacement: call_text.to_string(),
            })
        };
        diags.push(LintDiagnostic {
            rule: "unsafe-void-return",
            message: format!("assigning void call result to `{var_name}`"),
            severity: Severity::Warning,
            line: node.start_position().row,
            column: node.start_position().column,
            end_column: None,
            fix,
            context_lines: None,
        });
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

fn is_void_call(
    node: &Node,
    source: &str,
    symbols: &SymbolTable,
    project: Option<&ProjectIndex>,
) -> bool {
    // Only flag actual call expressions (not identifiers or other exprs)
    let is_call = node.kind() == "call"
        || (node.kind() == "attribute" && {
            let mut cursor = node.walk();
            node.children(&mut cursor)
                .any(|c| c.kind() == "attribute_call")
        });
    if !is_call {
        return false;
    }
    let inferred = if let Some(proj) = project {
        infer_expression_type_with_project(node, source, symbols, proj)
    } else {
        infer_expression_type(node, source, symbols)
    };
    matches!(inferred, Some(InferredType::Void))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{parser, symbol_table};

    fn check(source: &str) -> Vec<LintDiagnostic> {
        let tree = parser::parse(source).unwrap();
        let symbols = symbol_table::build(&tree, source);
        let config = LintConfig::default();
        UnsafeVoidReturn.check_with_symbols(&tree, source, &config, &symbols)
    }

    #[test]
    fn return_void_call() {
        let source = "\
extends Node
func f():
\treturn add_child(null)
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("void"));
    }

    #[test]
    fn return_non_void_call() {
        let source = "\
extends Node
func f():
\treturn get_child(0)
";
        let diags = check(source);
        assert!(diags.is_empty());
    }

    #[test]
    fn assign_void_call() {
        let source = "\
extends Node
func f():
\tvar x = add_child(null)
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("void"));
    }

    #[test]
    fn return_literal() {
        let source = "\
func f():
\treturn 42
";
        let diags = check(source);
        assert!(diags.is_empty());
    }

    #[test]
    fn return_self_void() {
        let source = "\
func do_thing() -> void:
\tpass
func f():
\treturn do_thing()
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn autofix_return_splits() {
        let source = "\
extends Node
func f():
\treturn add_child(null)
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        let fix = diags[0].fix.as_ref().expect("should have auto-fix");
        let fixed = format!(
            "{}{}{}",
            &source[..fix.byte_start],
            fix.replacement,
            &source[fix.byte_end..]
        );
        assert!(
            fixed.contains("add_child(null)\n\treturn"),
            "fixed was: {fixed}"
        );
    }

    #[test]
    fn autofix_assign_removes_var() {
        let source = "\
extends Node
func f():
\tvar x = add_child(null)
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        let fix = diags[0].fix.as_ref().expect("should have auto-fix");
        let fixed = format!(
            "{}{}{}",
            &source[..fix.byte_start],
            fix.replacement,
            &source[fix.byte_end..]
        );
        assert!(fixed.contains("\tadd_child(null)"), "fixed was: {fixed}");
        assert!(!fixed.contains("var x"), "should remove var: {fixed}");
    }

    #[test]
    fn opt_in_rule() {
        assert!(!UnsafeVoidReturn.default_enabled());
    }
}
