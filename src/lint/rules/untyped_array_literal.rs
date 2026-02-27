use crate::core::gd_ast::{self, GdDecl, GdExpr, GdFile, GdStmt, GdVar};

use super::{Fix, LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;
use crate::core::symbol_table::SymbolTable;
use crate::core::type_inference::{InferredType, infer_expression_type};

pub struct UntypedArrayLiteral;

impl LintRule for UntypedArrayLiteral {
    fn name(&self) -> &'static str {
        "untyped-array-literal"
    }

    fn category(&self) -> LintCategory {
        LintCategory::TypeSafety
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
        // Check class-level var declarations
        gd_ast::visit_decls(file, &mut |decl| {
            if let GdDecl::Var(var) = decl {
                check_var(var, source, symbols, &mut diags);
            }
        });
        // Check function-local var declarations
        gd_ast::visit_stmts(file, &mut |stmt| {
            if let GdStmt::Var(var) = stmt {
                check_var(var, source, symbols, &mut diags);
            }
        });
        diags
    }
}

fn check_var(
    var: &GdVar,
    source: &str,
    symbols: &SymbolTable,
    diags: &mut Vec<LintDiagnostic>,
) {
    // Only check := (inferred type)
    let Some(ref type_ann) = var.type_ann else { return };
    if !type_ann.is_inferred {
        return;
    }

    // Skip const declarations
    if var.is_const {
        return;
    }

    // Value must be a non-empty array literal
    let Some(GdExpr::Array { elements, .. }) = &var.value else { return };
    if elements.is_empty() {
        return;
    }

    // Try to infer element type using the centralized engine
    let suggested_type = infer_array_element_type(elements, source, symbols);

    // Build auto-fix when type is inferable: replace `:=` region with `: Array[T] =`
    let fix = suggested_type.as_ref().map(|elem_type| {
        let mut start = type_ann.node.start_byte();
        // Consume preceding whitespace so we get `var x: Array[T]` not `var x : Array[T]`
        while start > 0 && source.as_bytes()[start - 1] == b' ' {
            start -= 1;
        }
        Fix {
            byte_start: start,
            byte_end: type_ann.node.end_byte(),
            replacement: format!(": Array[{}] =", elem_type.display_name()),
        }
    });

    let message = if let Some(ref elem_type) = suggested_type {
        format!(
            "array literal infers `Variant` with `:=`; consider `var {}: Array[{}] = [...]`",
            var.name,
            elem_type.display_name()
        )
    } else {
        "array literal infers `Variant` with `:=`; consider adding an explicit `Array[T]` type"
            .to_string()
    };

    diags.push(LintDiagnostic {
        rule: "untyped-array-literal",
        message,
        severity: Severity::Warning,
        line: var.node.start_position().row,
        column: var.node.start_position().column,
        end_column: None,
        fix,
        context_lines: None,
    });
}

/// Infer the common element type of an array literal using the centralized engine.
fn infer_array_element_type(
    elements: &[GdExpr],
    source: &str,
    symbols: &SymbolTable,
) -> Option<InferredType> {
    if elements.is_empty() {
        return None;
    }

    let first_node = elements[0].node();
    let first_type = infer_expression_type(&first_node, source, symbols)?;

    // Skip Variant — can't determine a concrete element type
    if matches!(first_type, InferredType::Variant | InferredType::Void) {
        return None;
    }

    // Check that all elements have the same type
    for elem in &elements[1..] {
        let elem_node = elem.node();
        let elem_type = infer_expression_type(&elem_node, source, symbols)?;
        if elem_type != first_type {
            return None;
        }
    }

    Some(first_type)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::gd_ast;
    use crate::core::{parser, symbol_table};

    fn check(source: &str) -> Vec<LintDiagnostic> {
        let tree = parser::parse(source).unwrap();
        let file = gd_ast::convert(&tree, source);
        let symbols = symbol_table::build(&tree, source);
        let config = LintConfig::default();
        UntypedArrayLiteral.check_with_symbols(&file, source, &config, &symbols)
    }

    #[test]
    fn detects_string_array() {
        let source = "func f():\n\tvar x := [\"a\", \"b\"]\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("Array[String]"));
    }

    #[test]
    fn detects_int_array() {
        let source = "func f():\n\tvar x := [1, 2, 3]\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("Array[int]"));
    }

    #[test]
    fn detects_float_array() {
        let source = "func f():\n\tvar x := [1.0, 2.5]\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("Array[float]"));
    }

    #[test]
    fn detects_bool_array() {
        let source = "func f():\n\tvar x := [true, false]\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("Array[bool]"));
    }

    #[test]
    fn mixed_array_no_type_suggestion() {
        let source = "func f():\n\tvar x := [1, \"a\"]\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("Array[T]"));
    }

    #[test]
    fn no_warning_empty_array() {
        let source = "func f():\n\tvar x := []\n";
        let diags = check(source);
        assert!(diags.is_empty());
    }

    #[test]
    fn no_warning_explicit_type() {
        let source = "func f():\n\tvar x: Array[String] = [\"a\", \"b\"]\n";
        let diags = check(source);
        assert!(diags.is_empty());
    }

    #[test]
    fn no_warning_regular_equals() {
        let source = "func f():\n\tvar x = [\"a\", \"b\"]\n";
        let diags = check(source);
        assert!(diags.is_empty());
    }

    #[test]
    fn no_warning_const() {
        let source = "const ITEMS := [\"a\", \"b\"]\n";
        let diags = check(source);
        assert!(diags.is_empty());
    }

    #[test]
    fn autofix_string_array() {
        let source = "func f():\n\tvar x := [\"a\", \"b\"]\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        let fix = diags[0].fix.as_ref().expect("should have auto-fix");
        let fixed = format!(
            "{}{}{}",
            &source[..fix.byte_start],
            fix.replacement,
            &source[fix.byte_end..]
        );
        // `:=` replaced with `: Array[String] =`
        assert!(
            fixed.contains("var x: Array[String] ="),
            "fixed was: {fixed}"
        );
    }

    #[test]
    fn autofix_int_array() {
        let source = "func f():\n\tvar nums := [1, 2, 3]\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        let fix = diags[0].fix.as_ref().expect("should have auto-fix");
        assert_eq!(fix.replacement, ": Array[int] =");
    }

    #[test]
    fn no_autofix_mixed_array() {
        let source = "func f():\n\tvar x := [1, \"a\"]\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].fix.is_none());
    }

    #[test]
    fn detects_constructor_array() {
        let source = "func f():\n\tvar pts := [Vector2(0, 0), Vector2(1, 1)]\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("Array[Vector2]"));
        assert!(diags[0].fix.is_some());
    }

    #[test]
    fn default_enabled() {
        assert!(UntypedArrayLiteral.default_enabled());
    }
}
