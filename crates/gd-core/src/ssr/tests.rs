//! Behaviour tests for SSR pattern parsing (Phase 1).

use super::*;

// ═══════════════════════════════════════════════════════════════════════
//  Expression patterns — successful parsing
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn method_call_pattern() {
    let pat = parse_pattern("$recv.method($a, $b)").unwrap();
    assert!(matches!(pat.kind, PatternKind::Expr(_)));
    assert_eq!(pat.placeholders.len(), 3);
    assert!(pat.placeholders.contains_key("recv"));
    assert!(pat.placeholders.contains_key("a"));
    assert!(pat.placeholders.contains_key("b"));
    assert!(pat.placeholders.values().all(|p| !p.variadic));
}

#[test]
fn property_access_pattern() {
    let pat = parse_pattern("$obj.property").unwrap();
    assert!(matches!(pat.kind, PatternKind::Expr(_)));
    assert_eq!(pat.placeholders.len(), 1);
    assert!(pat.placeholders.contains_key("obj"));
}

#[test]
fn binary_op_pattern() {
    let pat = parse_pattern("$left + $right").unwrap();
    assert!(matches!(pat.kind, PatternKind::Expr(_)));
    assert_eq!(pat.placeholders.len(), 2);
    assert!(pat.placeholders.contains_key("left"));
    assert!(pat.placeholders.contains_key("right"));
}

#[test]
fn function_call_pattern() {
    let pat = parse_pattern("SomeClass.new($a)").unwrap();
    assert!(matches!(pat.kind, PatternKind::Expr(_)));
    assert_eq!(pat.placeholders.len(), 1);
    assert!(pat.placeholders.contains_key("a"));
}

#[test]
fn chained_method_call_pattern() {
    let pat = parse_pattern("$a.foo().bar()").unwrap();
    assert!(matches!(pat.kind, PatternKind::Expr(_)));
    assert_eq!(pat.placeholders.len(), 1);
    assert!(pat.placeholders.contains_key("a"));
}

#[test]
fn subscript_pattern() {
    let pat = parse_pattern("$arr[$idx]").unwrap();
    assert!(matches!(pat.kind, PatternKind::Expr(_)));
    assert_eq!(pat.placeholders.len(), 2);
    assert!(pat.placeholders.contains_key("arr"));
    assert!(pat.placeholders.contains_key("idx"));
}

#[test]
fn ternary_pattern() {
    let pat = parse_pattern("$a if $cond else $b").unwrap();
    assert!(matches!(pat.kind, PatternKind::Expr(_)));
    assert_eq!(pat.placeholders.len(), 3);
    assert!(pat.placeholders.contains_key("a"));
    assert!(pat.placeholders.contains_key("cond"));
    assert!(pat.placeholders.contains_key("b"));
}

#[test]
fn literal_in_structural_position() {
    let pat = parse_pattern("$a.connect(\"ready\", $b)").unwrap();
    assert!(matches!(pat.kind, PatternKind::Expr(_)));
    assert_eq!(pat.placeholders.len(), 2);
    assert!(pat.placeholders.contains_key("a"));
    assert!(pat.placeholders.contains_key("b"));
}

#[test]
fn unary_op_pattern() {
    let pat = parse_pattern("-$x").unwrap();
    assert!(matches!(pat.kind, PatternKind::Expr(_)));
    assert_eq!(pat.placeholders.len(), 1);
    assert!(pat.placeholders.contains_key("x"));
}

#[test]
fn no_placeholders() {
    let pat = parse_pattern("foo.bar()").unwrap();
    assert!(matches!(pat.kind, PatternKind::Expr(_)));
    assert!(pat.placeholders.is_empty());
}

// ═══════════════════════════════════════════════════════════════════════
//  Statement patterns
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn var_declaration_pattern() {
    let pat = parse_pattern("var $name = $value").unwrap();
    assert!(matches!(pat.kind, PatternKind::Stmt(_)));
    assert_eq!(pat.placeholders.len(), 2);
    assert!(pat.placeholders.contains_key("name"));
    assert!(pat.placeholders.contains_key("value"));
}

#[test]
fn assignment_pattern() {
    let pat = parse_pattern("$target = $value").unwrap();
    assert!(matches!(pat.kind, PatternKind::Stmt(_)));
    assert_eq!(pat.placeholders.len(), 2);
    assert!(pat.placeholders.contains_key("target"));
    assert!(pat.placeholders.contains_key("value"));
}

#[test]
fn aug_assignment_pattern() {
    let pat = parse_pattern("$target += $value").unwrap();
    assert!(matches!(pat.kind, PatternKind::Stmt(_)));
    assert_eq!(pat.placeholders.len(), 2);
}

#[test]
fn return_pattern() {
    let pat = parse_pattern("return $expr").unwrap();
    assert!(matches!(pat.kind, PatternKind::Stmt(_)));
    assert_eq!(pat.placeholders.len(), 1);
    assert!(pat.placeholders.contains_key("expr"));
}

#[test]
fn stmt_prefix_forces_statement() {
    let pat = parse_pattern("stmt:$target = $value").unwrap();
    assert!(matches!(pat.kind, PatternKind::Stmt(_)));
    assert_eq!(pat.placeholders.len(), 2);
}

// ═══════════════════════════════════════════════════════════════════════
//  Placeholder features
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn repeated_placeholder_single_entry() {
    let pat = parse_pattern("$a + $a").unwrap();
    assert_eq!(pat.placeholders.len(), 1);
    assert!(pat.placeholders.contains_key("a"));
}

#[test]
fn variadic_in_call_position() {
    let pat = parse_pattern("print($$args)").unwrap();
    assert!(matches!(pat.kind, PatternKind::Expr(_)));
    assert_eq!(pat.placeholders.len(), 1);
    let info = &pat.placeholders["args"];
    assert!(info.variadic);
}

#[test]
fn variadic_with_other_args() {
    let pat = parse_pattern("$obj.method($a, $$rest)").unwrap();
    assert_eq!(pat.placeholders.len(), 3);
    assert!(!pat.placeholders["obj"].variadic);
    assert!(!pat.placeholders["a"].variadic);
    assert!(pat.placeholders["rest"].variadic);
}

#[test]
fn type_constraint_on_placeholder() {
    let pat = parse_pattern("$x:Node.foo()").unwrap();
    assert!(matches!(pat.kind, PatternKind::Expr(_)));
    assert_eq!(pat.placeholders.len(), 1);
    let info = &pat.placeholders["x"];
    assert!(!info.variadic);
    assert_eq!(info.type_constraint.as_deref(), Some("Node"));
}

#[test]
fn type_constraint_multiple() {
    let pat = parse_pattern("$a:Node.add_child($b:Control)").unwrap();
    assert_eq!(pat.placeholders.len(), 2);
    assert_eq!(
        pat.placeholders["a"].type_constraint.as_deref(),
        Some("Node")
    );
    assert_eq!(
        pat.placeholders["b"].type_constraint.as_deref(),
        Some("Control")
    );
}

#[test]
fn original_source_preserved() {
    let input = "$recv.method($a)";
    let pat = parse_pattern(input).unwrap();
    assert_eq!(pat.source, input);
}

// ═══════════════════════════════════════════════════════════════════════
//  Validation — errors
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn syntax_error_rejected() {
    let result = parse_pattern("$a.");
    assert!(result.is_err());
}

#[test]
fn variadic_outside_call_rejected() {
    let result = parse_pattern("$a + $$b");
    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("variadic"),
        "error should mention variadic: {msg}"
    );
}

#[test]
fn variadic_in_binary_op_rejected() {
    let result = parse_pattern("$$x * 2");
    assert!(result.is_err());
}

#[test]
fn bare_dollar_rejected() {
    let result = parse_pattern("$ + 1");
    assert!(result.is_err());
}

#[test]
fn bare_double_dollar_rejected() {
    let result = parse_pattern("$$");
    assert!(result.is_err());
}

// ═══════════════════════════════════════════════════════════════════════
//  Template parsing
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn template_with_subset_of_pattern_placeholders() {
    let pat = parse_pattern("$a + $b").unwrap();
    let tmpl = parse_template("$a", &pat).unwrap();
    assert_eq!(tmpl.placeholders.len(), 1);
    assert!(tmpl.placeholders.contains("a"));
}

#[test]
fn template_with_all_pattern_placeholders() {
    let pat = parse_pattern("$recv.old_method($arg)").unwrap();
    let tmpl = parse_template("$recv.new_method($arg)", &pat).unwrap();
    assert_eq!(tmpl.placeholders.len(), 2);
    assert!(tmpl.placeholders.contains("recv"));
    assert!(tmpl.placeholders.contains("arg"));
}

#[test]
fn template_unbound_placeholder_rejected() {
    let pat = parse_pattern("$a + $b").unwrap();
    let result = parse_template("$a + $c", &pat);
    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("$c"),
        "error should mention unbound placeholder: {msg}"
    );
}

#[test]
fn template_original_source_preserved() {
    let pat = parse_pattern("$a.foo()").unwrap();
    let input = "$a.bar()";
    let tmpl = parse_template(input, &pat).unwrap();
    assert_eq!(tmpl.source, input);
}

// ═══════════════════════════════════════════════════════════════════════
//  Pattern AST structure verification
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn expr_pattern_is_method_call_ast() {
    let pat = parse_pattern("$recv.method($a)").unwrap();
    match &pat.kind {
        PatternKind::Expr(expr) => match expr {
            crate::ast_owned::OwnedExpr::MethodCall { method, args, .. } => {
                assert_eq!(method, "method");
                assert_eq!(args.len(), 1);
            }
            other => panic!("expected MethodCall, got {other:?}"),
        },
        PatternKind::Stmt(_) => panic!("expected Expr pattern"),
    }
}

#[test]
fn expr_pattern_binop_structure() {
    let pat = parse_pattern("$x + $y").unwrap();
    match &pat.kind {
        PatternKind::Expr(crate::ast_owned::OwnedExpr::BinOp { op, .. }) => {
            assert_eq!(op, "+");
        }
        other => panic!("expected BinOp, got {other:?}"),
    }
}

#[test]
fn stmt_pattern_is_assign_ast() {
    let pat = parse_pattern("$target = $value").unwrap();
    match pat.kind {
        PatternKind::Stmt(ref s) => {
            assert!(matches!(
                s.as_ref(),
                crate::ast_owned::OwnedStmt::Assign { .. }
            ));
        }
        PatternKind::Expr(ref e) => panic!("expected Assign statement, got Expr({e:?})"),
    }
}

#[test]
fn stmt_pattern_is_return_ast() {
    let pat = parse_pattern("return $x").unwrap();
    match pat.kind {
        PatternKind::Stmt(ref s) => match s.as_ref() {
            crate::ast_owned::OwnedStmt::Return { value, .. } => {
                assert!(value.is_some());
            }
            other => panic!("expected Return statement, got {other:?}"),
        },
        PatternKind::Expr(ref e) => panic!("expected Stmt pattern, got Expr({e:?})"),
    }
}

#[test]
fn stmt_pattern_var_decl_ast() {
    let pat = parse_pattern("var $name = $value").unwrap();
    match pat.kind {
        PatternKind::Stmt(ref s) => match s.as_ref() {
            crate::ast_owned::OwnedStmt::Var(var) => {
                assert!(var.value.is_some());
            }
            other => panic!("expected Var statement, got {other:?}"),
        },
        PatternKind::Expr(ref e) => panic!("expected Stmt pattern, got Expr({e:?})"),
    }
}
