use gd_core::gd_ast::{GdDecl, GdFile, GdFunc, GdStmt};

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use gd_core::config::LintConfig;

pub struct EmptyFunction;

impl LintRule for EmptyFunction {
    fn name(&self) -> &'static str {
        "empty-function"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Style
    }

    fn check(&self, file: &GdFile<'_>, _source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        check_scope(&file.declarations, &mut diags);
        diags
    }
}

/// Check all functions within a scope (file top-level or class body).
/// Two-pass: first detect if the scope contains virtual stubs (empty functions
/// with all-`_`-prefixed params), then emit warnings skipping zero-param
/// empty functions that are siblings of virtual stubs.
fn check_scope(decls: &[GdDecl<'_>], diags: &mut Vec<LintDiagnostic>) {
    // Pass 1: find empty functions and check for virtual stubs
    let mut empty_funcs: Vec<&GdFunc<'_>> = Vec::new();
    let mut has_virtual_stubs = false;

    for decl in decls {
        if let GdDecl::Func(func) = decl
            && is_empty_body(func)
            && !func.annotations.iter().any(|a| a.name == "abstract")
        {
            if is_virtual_stub(func) {
                has_virtual_stubs = true;
            }
            empty_funcs.push(func);
        }

        // Recurse into class bodies (separate scope)
        if let GdDecl::Class(class) = decl {
            check_scope(&class.declarations, diags);
        }
    }

    // Pass 2: emit warnings, skipping virtual stubs and zero-param siblings
    for func in empty_funcs {
        if is_virtual_stub(func) {
            continue;
        }

        // Zero-param private function alongside virtual stubs → likely a virtual stub too
        if has_virtual_stubs && func.params.is_empty() && func.name.starts_with('_') {
            continue;
        }

        diags.push(LintDiagnostic {
            rule: "empty-function",
            message: format!("function `{}` has an empty body (only `pass`)", func.name),
            severity: Severity::Warning,
            line: func.node.start_position().row,
            column: func.node.start_position().column,
            fix: None,
            end_column: None,
            context_lines: None,
        });
    }
}

/// Check if a function has only `pass` in its body.
fn is_empty_body(func: &GdFunc<'_>) -> bool {
    func.body.len() == 1 && matches!(func.body[0], GdStmt::Pass { .. })
}

/// A function is a virtual stub if it has at least one parameter and every
/// parameter name starts with `_`.
fn is_virtual_stub(func: &GdFunc<'_>) -> bool {
    !func.params.is_empty() && func.params.iter().all(|p| p.name.starts_with('_'))
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
        EmptyFunction.check(&file, source, &config)
    }

    #[test]
    fn warns_on_empty_function() {
        let source = "func do_nothing():\n\tpass\n";
        assert_eq!(check(source).len(), 1);
    }

    #[test]
    fn no_warning_on_virtual_stub() {
        let source = "func _on_enter(_msg: Dictionary) -> void:\n\tpass\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_on_multi_param_virtual_stub() {
        let source = "func _on_update(_delta: float, _state: int) -> void:\n\tpass\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn warns_when_not_all_params_prefixed() {
        let source = "func process(delta: float) -> void:\n\tpass\n";
        assert_eq!(check(source).len(), 1);
    }

    #[test]
    fn no_warning_on_zero_param_sibling_of_virtual_stub() {
        // _on_exit has no params but is alongside _on_enter which is a virtual stub
        let source = "func _on_enter(_msg: Dictionary) -> void:\n\tpass\n\nfunc _on_exit() -> void:\n\tpass\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn warns_on_isolated_zero_param_empty() {
        // No virtual stub siblings → should warn
        let source = "func _on_exit() -> void:\n\tpass\n";
        assert_eq!(check(source).len(), 1);
    }

    #[test]
    fn warns_on_standalone_empty_handler() {
        // Empty signal handler with no virtual stub context
        let source = "func _on_button_pressed() -> void:\n\tpass\n\nfunc do_stuff() -> void:\n\tprint(\"hi\")\n";
        assert_eq!(check(source).len(), 1);
    }

    #[test]
    fn no_warning_on_abstract_function() {
        // @abstract on same line as func
        let source = "@abstract func draw():\n\tpass\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_on_abstract_function_separate_line() {
        // @abstract on separate line from func
        let source = "@abstract\nfunc draw():\n\tpass\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn warns_on_non_abstract_empty() {
        // @abstract on the class, not the function — function should still warn
        let source = "func draw():\n\tpass\n";
        assert_eq!(check(source).len(), 1);
    }

    #[test]
    fn warns_on_public_empty_alongside_stubs() {
        // do_nothing is public (no _ prefix) so should still warn even with virtual stubs nearby
        let source = "func _on_enter(_msg: Dictionary) -> void:\n\tpass\n\nfunc do_nothing() -> void:\n\tpass\n";
        assert_eq!(check(source).len(), 1);
        assert_eq!(
            check(source)[0].message,
            "function `do_nothing` has an empty body (only `pass`)"
        );
    }
}
