use crate::core::gd_ast::{self, GdDecl, GdExpr, GdFile, GdFunc, GdStmt};

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;
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

    fn check(&self, _file: &GdFile<'_>, _source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        Vec::new()
    }

    fn check_with_project(
        &self,
        file: &GdFile<'_>,
        source: &str,
        _config: &LintConfig,
        project: &ProjectIndex,
    ) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        gd_ast::visit_decls(file, &mut |decl| {
            if let GdDecl::Func(func) = decl {
                check_stmts(&func.body, func, source, file, project, &mut diags);
            }
        });
        diags
    }
}

/// Recursively search statements for function calls with array arguments.
fn check_stmts(
    stmts: &[GdStmt],
    func: &GdFunc,
    source: &str,
    file: &GdFile,
    project: &ProjectIndex,
    diags: &mut Vec<LintDiagnostic>,
) {
    for stmt in stmts {
        match stmt {
            GdStmt::Expr { expr, .. } => {
                check_call_expr(expr, func, source, file, project, diags);
            }
            GdStmt::Var(var) => {
                if let Some(value) = &var.value {
                    check_call_expr(value, func, source, file, project, diags);
                }
            }
            GdStmt::Assign { value, .. } | GdStmt::AugAssign { value, .. } => {
                check_call_expr(value, func, source, file, project, diags);
            }
            GdStmt::Return { value: Some(v), .. } => {
                check_call_expr(v, func, source, file, project, diags);
            }
            GdStmt::If(if_stmt) => {
                check_call_expr(&if_stmt.condition, func, source, file, project, diags);
                check_stmts(&if_stmt.body, func, source, file, project, diags);
                for (cond, branch) in &if_stmt.elif_branches {
                    check_call_expr(cond, func, source, file, project, diags);
                    check_stmts(branch, func, source, file, project, diags);
                }
                if let Some(else_body) = &if_stmt.else_body {
                    check_stmts(else_body, func, source, file, project, diags);
                }
            }
            GdStmt::For { iter, body, .. } => {
                check_call_expr(iter, func, source, file, project, diags);
                check_stmts(body, func, source, file, project, diags);
            }
            GdStmt::While { condition, body, .. } => {
                check_call_expr(condition, func, source, file, project, diags);
                check_stmts(body, func, source, file, project, diags);
            }
            GdStmt::Match { value, arms, .. } => {
                check_call_expr(value, func, source, file, project, diags);
                for arm in arms {
                    check_stmts(&arm.body, func, source, file, project, diags);
                }
            }
            _ => {}
        }
    }
}

/// Recursively check expressions for function calls with array type mismatches.
fn check_call_expr(
    expr: &GdExpr,
    func: &GdFunc,
    source: &str,
    file: &GdFile,
    project: &ProjectIndex,
    diags: &mut Vec<LintDiagnostic>,
) {
    // Check if this is a plain call: func_name(args)
    if let GdExpr::Call { callee, args, .. } = expr
        && let GdExpr::Ident { name: func_name, .. } = callee.as_ref()
    {
        check_call_args(func_name, args, func, source, file, project, diags);
    }

    // Recurse into sub-expressions
    match expr {
        GdExpr::Call { callee, args, .. } => {
            check_call_expr(callee, func, source, file, project, diags);
            for a in args {
                check_call_expr(a, func, source, file, project, diags);
            }
        }
        GdExpr::MethodCall { receiver, args, .. } => {
            check_call_expr(receiver, func, source, file, project, diags);
            for a in args {
                check_call_expr(a, func, source, file, project, diags);
            }
        }
        GdExpr::BinOp { left, right, .. } => {
            check_call_expr(left, func, source, file, project, diags);
            check_call_expr(right, func, source, file, project, diags);
        }
        GdExpr::UnaryOp { operand, .. } => {
            check_call_expr(operand, func, source, file, project, diags);
        }
        GdExpr::Ternary { condition, true_val, false_val, .. } => {
            check_call_expr(condition, func, source, file, project, diags);
            check_call_expr(true_val, func, source, file, project, diags);
            check_call_expr(false_val, func, source, file, project, diags);
        }
        GdExpr::Array { elements, .. } => {
            for e in elements {
                check_call_expr(e, func, source, file, project, diags);
            }
        }
        GdExpr::Subscript { receiver, index, .. } => {
            check_call_expr(receiver, func, source, file, project, diags);
            check_call_expr(index, func, source, file, project, diags);
        }
        GdExpr::PropertyAccess { receiver, .. } => {
            check_call_expr(receiver, func, source, file, project, diags);
        }
        GdExpr::Cast { expr: inner, .. }
        | GdExpr::Is { expr: inner, .. }
        | GdExpr::Await { expr: inner, .. } => {
            check_call_expr(inner, func, source, file, project, diags);
        }
        _ => {}
    }
}

/// Check arguments of a plain function call against expected parameter types.
fn check_call_args(
    func_name: &str,
    args: &[GdExpr],
    func: &GdFunc,
    source: &str,
    file: &GdFile,
    project: &ProjectIndex,
    diags: &mut Vec<LintDiagnostic>,
) {
    let param_types = resolve_param_types(func_name, file, project);
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

        // Infer argument type — use raw node escape hatch for type inference API,
        // then fall back to local variable lookup via typed AST
        let arg_node = arg.node();
        let arg_type = infer_expression_type_with_project(&arg_node, source, file, project)
            .or_else(|| resolve_local_type(arg, &func.body));

        let Some(arg_type) = arg_type else {
            continue;
        };

        match &arg_type {
            // Untyped Array passed to typed Array[T]
            InferredType::Builtin("Array") => {
                // Skip empty array literals — Godot handles these fine
                if matches!(arg, GdExpr::Array { elements, .. } if elements.is_empty()) {
                    continue;
                }
                let node = arg.node();
                diags.push(LintDiagnostic {
                    rule: "untyped-array-argument",
                    message: format!(
                        "passing untyped `Array` to parameter expecting `Array[{expected_element}]`"
                    ),
                    severity: Severity::Warning,
                    line: node.start_position().row,
                    column: node.start_position().column,
                    end_column: Some(node.end_position().column),
                    fix: None,
                    context_lines: None,
                });
            }
            // Typed Array[X] passed to Array[T] where X != T
            InferredType::TypedArray(inner) => {
                let actual_element = inner.display_name();
                if actual_element != expected_element {
                    let node = arg.node();
                    diags.push(LintDiagnostic {
                        rule: "untyped-array-argument",
                        message: format!(
                            "passing `Array[{actual_element}]` to parameter expecting `Array[{expected_element}]`"
                        ),
                        severity: Severity::Warning,
                        line: node.start_position().row,
                        column: node.start_position().column,
                        end_column: Some(node.end_position().column),
                        fix: None,
                        context_lines: None,
                    });
                }
            }
            _ => {}
        }
    }
}

/// Resolve the type of an identifier argument by looking up its local variable declaration.
fn resolve_local_type(arg: &GdExpr, func_body: &[GdStmt]) -> Option<InferredType> {
    let GdExpr::Ident { name, node, .. } = arg else {
        return None;
    };

    let target_line = node.start_position().row;
    for stmt in func_body {
        if stmt.node().start_position().row >= target_line {
            break;
        }
        if let GdStmt::Var(var) = stmt
            && var.name == *name
            && let Some(type_ann) = &var.type_ann
            && !type_ann.is_inferred
        {
            return Some(classify_array_type(type_ann.name));
        }
    }
    None
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
    file: &GdFile,
    project: &ProjectIndex,
) -> Vec<Option<String>> {
    // Same-file functions first
    for func in file.funcs() {
        if func.name == func_name {
            return func
                .params
                .iter()
                .map(|p| {
                    p.type_ann
                        .as_ref()
                        .filter(|t| !t.is_inferred && !t.name.is_empty())
                        .map(|t| t.name.to_string())
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
    use crate::core::gd_ast;
    use crate::core::parser;
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
        let file = gd_ast::convert(&tree, source);
        let config = LintConfig::default();
        UntypedArrayArgument.check_with_project(&file, source, &config, &project)
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
