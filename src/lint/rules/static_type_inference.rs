use crate::core::gd_ast::{self, GdDecl, GdFile, GdStmt, GdVar};

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;
use crate::core::symbol_table::SymbolTable;
use crate::core::type_inference::{InferredType, infer_expression_type};

pub struct StaticTypeInference;

impl LintRule for StaticTypeInference {
    fn name(&self) -> &'static str {
        "static-type-inference"
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
        gd_ast::visit_decls(file, &mut |decl| {
            if let GdDecl::Var(var) = decl {
                check_var(var, source, symbols, &mut diags);
            }
        });
        gd_ast::visit_stmts(file, &mut |stmt| {
            if let GdStmt::Var(var) = stmt {
                check_var(var, source, symbols, &mut diags);
            }
        });
        diags
    }
}

fn check_var(
    var: &GdVar<'_>,
    source: &str,
    symbols: &SymbolTable,
    diags: &mut Vec<LintDiagnostic>,
) {
    // Skip if already has any type annotation (explicit or inferred via :=)
    if var.type_ann.is_some() {
        return;
    }

    let Some(value) = &var.value else { return };
    let value_node = value.node();
    let Some(inferred) = infer_expression_type(&value_node, source, symbols) else {
        return;
    };

    // Only suggest for concrete builtin types (not Void, Variant, or Class)
    if !matches!(inferred, InferredType::Builtin(_)) {
        return;
    }

    let (line, col) = if let Some(name_node) = var.node.child_by_field_name("name") {
        (
            name_node.start_position().row,
            name_node.start_position().column,
        )
    } else {
        (
            var.node.start_position().row,
            var.node.start_position().column,
        )
    };

    diags.push(LintDiagnostic {
        rule: "static-type-inference",
        message: format!(
            "variable `{}` could have an explicit type: `{}`",
            var.name,
            inferred.display_name()
        ),
        severity: Severity::Warning,
        line,
        column: col,
        end_column: None,
        fix: None,
        context_lines: None,
    });
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
        StaticTypeInference.check_with_symbols(&file, source, &config, &symbols)
    }

    #[test]
    fn suggests_int_type() {
        let diags = check("var x = 42\n");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("`int`"));
    }

    #[test]
    fn suggests_float_type() {
        let diags = check("var x = 3.14\n");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("`float`"));
    }

    #[test]
    fn suggests_string_type() {
        let diags = check("var x = \"hello\"\n");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("`String`"));
    }

    #[test]
    fn suggests_bool_type() {
        let diags = check("var x = true\n");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("`bool`"));
    }

    #[test]
    fn suggests_array_type() {
        let diags = check("var x = [1, 2, 3]\n");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("`Array`"));
    }

    #[test]
    fn suggests_dictionary_type() {
        let diags = check("var x = {\"a\": 1}\n");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("`Dictionary`"));
    }

    #[test]
    fn suggests_vector2_type() {
        let diags = check("var x = Vector2(1, 2)\n");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("`Vector2`"));
    }

    #[test]
    fn suggests_color_type() {
        let diags = check("var x = Color(1, 0, 0)\n");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("`Color`"));
    }

    #[test]
    fn no_warning_typed_var() {
        let diags = check("var x: int = 42\n");
        assert!(diags.is_empty());
    }

    #[test]
    fn no_warning_inferred_var() {
        let diags = check("var x := 42\n");
        assert!(diags.is_empty());
    }

    #[test]
    fn suggests_negative_int() {
        let diags = check("var x = -5\n");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("`int`"));
    }

    #[test]
    fn suggests_negative_float() {
        let diags = check("var x = -3.14\n");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("`float`"));
    }

    #[test]
    fn no_warning_unresolvable() {
        // Variable assigned from a function call that doesn't return a builtin
        let diags = check("var x = get_node(\"path\")\n");
        assert!(diags.is_empty());
    }
}
