//! Behaviour tests for SSR (Phase 1 + Phase 2).

use super::*;

// ═══════════════════════════════════════════════════════════════════════
//  Phase 1: Pattern parsing — expression patterns
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
}

#[test]
fn function_call_pattern() {
    let pat = parse_pattern("SomeClass.new($a)").unwrap();
    assert!(matches!(pat.kind, PatternKind::Expr(_)));
    assert_eq!(pat.placeholders.len(), 1);
}

#[test]
fn chained_method_call_pattern() {
    let pat = parse_pattern("$a.foo().bar()").unwrap();
    assert!(matches!(pat.kind, PatternKind::Expr(_)));
    assert_eq!(pat.placeholders.len(), 1);
}

#[test]
fn subscript_pattern() {
    let pat = parse_pattern("$arr[$idx]").unwrap();
    assert!(matches!(pat.kind, PatternKind::Expr(_)));
    assert_eq!(pat.placeholders.len(), 2);
}

#[test]
fn ternary_pattern() {
    let pat = parse_pattern("$a if $cond else $b").unwrap();
    assert!(matches!(pat.kind, PatternKind::Expr(_)));
    assert_eq!(pat.placeholders.len(), 3);
}

#[test]
fn literal_in_structural_position() {
    let pat = parse_pattern("$a.connect(\"ready\", $b)").unwrap();
    assert!(matches!(pat.kind, PatternKind::Expr(_)));
    assert_eq!(pat.placeholders.len(), 2);
}

#[test]
fn unary_op_pattern() {
    let pat = parse_pattern("-$x").unwrap();
    assert!(matches!(pat.kind, PatternKind::Expr(_)));
    assert_eq!(pat.placeholders.len(), 1);
}

#[test]
fn no_placeholders() {
    let pat = parse_pattern("foo.bar()").unwrap();
    assert!(matches!(pat.kind, PatternKind::Expr(_)));
    assert!(pat.placeholders.is_empty());
}

// ═══════════════════════════════════════════════════════════════════════
//  Phase 1: Statement patterns
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn var_declaration_pattern() {
    let pat = parse_pattern("var $name = $value").unwrap();
    assert!(matches!(pat.kind, PatternKind::Stmt(_)));
    assert_eq!(pat.placeholders.len(), 2);
}

#[test]
fn assignment_pattern() {
    let pat = parse_pattern("$target = $value").unwrap();
    assert!(matches!(pat.kind, PatternKind::Stmt(_)));
    assert_eq!(pat.placeholders.len(), 2);
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
}

#[test]
fn stmt_prefix_forces_statement() {
    let pat = parse_pattern("stmt:$target = $value").unwrap();
    assert!(matches!(pat.kind, PatternKind::Stmt(_)));
    assert_eq!(pat.placeholders.len(), 2);
}

// ═══════════════════════════════════════════════════════════════════════
//  Phase 1: Placeholder features
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn repeated_placeholder_single_entry() {
    let pat = parse_pattern("$a + $a").unwrap();
    assert_eq!(pat.placeholders.len(), 1);
}

#[test]
fn variadic_in_call_position() {
    let pat = parse_pattern("print($$args)").unwrap();
    assert!(matches!(pat.kind, PatternKind::Expr(_)));
    assert!(pat.placeholders["args"].variadic);
}

#[test]
fn variadic_with_other_args() {
    let pat = parse_pattern("$obj.method($a, $$rest)").unwrap();
    assert_eq!(pat.placeholders.len(), 3);
    assert!(pat.placeholders["rest"].variadic);
}

#[test]
fn type_constraint_on_placeholder() {
    let pat = parse_pattern("$x:Node.foo()").unwrap();
    assert_eq!(
        pat.placeholders["x"].type_constraint.as_deref(),
        Some("Node")
    );
}

#[test]
fn type_constraint_multiple() {
    let pat = parse_pattern("$a:Node.add_child($b:Control)").unwrap();
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
//  Phase 1: Validation errors
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn syntax_error_rejected() {
    assert!(parse_pattern("$a.").is_err());
}

#[test]
fn variadic_outside_call_rejected() {
    let err = parse_pattern("$a + $$b").unwrap_err().to_string();
    assert!(err.contains("variadic"));
}

#[test]
fn variadic_in_binary_op_rejected() {
    assert!(parse_pattern("$$x * 2").is_err());
}

#[test]
fn bare_dollar_rejected() {
    assert!(parse_pattern("$ + 1").is_err());
}

#[test]
fn bare_double_dollar_rejected() {
    assert!(parse_pattern("$$").is_err());
}

// ═══════════════════════════════════════════════════════════════════════
//  Phase 1: Template parsing
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn template_with_subset_of_pattern_placeholders() {
    let pat = parse_pattern("$a + $b").unwrap();
    let tmpl = parse_template("$a", &pat).unwrap();
    assert_eq!(tmpl.placeholders.len(), 1);
}

#[test]
fn template_with_all_pattern_placeholders() {
    let pat = parse_pattern("$recv.old_method($arg)").unwrap();
    let tmpl = parse_template("$recv.new_method($arg)", &pat).unwrap();
    assert_eq!(tmpl.placeholders.len(), 2);
}

#[test]
fn template_unbound_placeholder_rejected() {
    let pat = parse_pattern("$a + $b").unwrap();
    let err = parse_template("$a + $c", &pat).unwrap_err().to_string();
    assert!(err.contains("$c"));
}

#[test]
fn template_original_source_preserved() {
    let pat = parse_pattern("$a.foo()").unwrap();
    let tmpl = parse_template("$a.bar()", &pat).unwrap();
    assert_eq!(tmpl.source, "$a.bar()");
}

// ═══════════════════════════════════════════════════════════════════════
//  Phase 1: Pattern AST structure
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn expr_pattern_is_method_call_ast() {
    let pat = parse_pattern("$recv.method($a)").unwrap();
    match &pat.kind {
        PatternKind::Expr(crate::ast_owned::OwnedExpr::MethodCall { method, args, .. }) => {
            assert_eq!(method, "method");
            assert_eq!(args.len(), 1);
        }
        other => panic!("expected MethodCall, got {other:?}"),
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
    assert!(
        matches!(pat.kind, PatternKind::Stmt(ref s) if matches!(s.as_ref(), crate::ast_owned::OwnedStmt::Assign { .. }))
    );
}

#[test]
fn stmt_pattern_is_return_ast() {
    let pat = parse_pattern("return $x").unwrap();
    assert!(
        matches!(pat.kind, PatternKind::Stmt(ref s) if matches!(s.as_ref(), crate::ast_owned::OwnedStmt::Return { value: Some(_), .. }))
    );
}

#[test]
fn stmt_pattern_var_decl_ast() {
    let pat = parse_pattern("var $name = $value").unwrap();
    assert!(
        matches!(pat.kind, PatternKind::Stmt(ref s) if matches!(s.as_ref(), crate::ast_owned::OwnedStmt::Var(v) if v.value.is_some()))
    );
}

// ═══════════════════════════════════════════════════════════════════════
//  Phase 2: Structural matcher — helpers
// ═══════════════════════════════════════════════════════════════════════

fn find(pattern_str: &str, source: &str) -> Vec<MatchResult> {
    let pat = parse_pattern(pattern_str).unwrap();
    let tree = crate::parser::parse(source).unwrap();
    let file = crate::gd_ast::convert(&tree, source);
    find_matches(&pat, &file, source, std::path::PathBuf::new())
}

fn cap(m: &MatchResult, name: &str) -> String {
    match m.captures.get(name).unwrap() {
        Capture::Expr(c) => c.source_text.clone(),
        Capture::ArgList(_) => panic!("expected Expr capture for '{name}'"),
    }
}

fn cap_args(m: &MatchResult, name: &str) -> Vec<String> {
    match m.captures.get(name).unwrap() {
        Capture::ArgList(a) => a.iter().map(|c| c.source_text.clone()).collect(),
        Capture::Expr(_) => panic!("expected ArgList capture for '{name}'"),
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  Phase 2: Expression matching
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn match_binop_captures() {
    let m = find("$a + $b", "func f():\n\tvar x = 1 + 2\n");
    assert_eq!(m.len(), 1);
    assert_eq!(cap(&m[0], "a"), "1");
    assert_eq!(cap(&m[0], "b"), "2");
}

#[test]
fn match_binop_op_mismatch() {
    assert!(find("$a + $b", "func f():\n\tvar x = 1 * 2\n").is_empty());
}

#[test]
fn match_method_call_captures() {
    let m = find("$recv.foo($a)", "func f():\n\tnode.foo(42)\n");
    assert_eq!(m.len(), 1);
    assert_eq!(cap(&m[0], "recv"), "node");
    assert_eq!(cap(&m[0], "a"), "42");
}

#[test]
fn match_method_name_mismatch() {
    assert!(find("$recv.foo($a)", "func f():\n\tnode.bar(42)\n").is_empty());
}

#[test]
fn match_no_receiver() {
    assert!(find("$a.foo()", "func f():\n\tbar()\n").is_empty());
}

#[test]
fn match_property_access() {
    let m = find("$a.health", "func f():\n\tvar x = obj.health\n");
    assert_eq!(m.len(), 1);
    assert_eq!(cap(&m[0], "a"), "obj");
}

#[test]
fn match_subscript() {
    let m = find("$a[$b]", "func f():\n\tvar x = arr[0]\n");
    assert_eq!(m.len(), 1);
    assert_eq!(cap(&m[0], "a"), "arr");
    assert_eq!(cap(&m[0], "b"), "0");
}

#[test]
fn match_wildcard_matches_many() {
    let m = find("$a", "func f():\n\tvar x = foo.bar(1, 2).baz()\n");
    assert!(!m.is_empty());
}

// ── Repeated placeholders ────────────────────────────────────────────

#[test]
fn match_repeated_same() {
    let m = find("$a + $a", "func f():\n\tvar z = x + x\n");
    assert_eq!(m.len(), 1);
    assert_eq!(cap(&m[0], "a"), "x");
}

#[test]
fn match_repeated_different() {
    assert!(find("$a + $a", "func f():\n\tvar z = x + y\n").is_empty());
}

// ── Variadic arguments ──────────────────────────────────────────────

#[test]
fn match_variadic_zero() {
    let m = find("print($$args)", "func f():\n\tprint()\n");
    assert_eq!(m.len(), 1);
    assert!(cap_args(&m[0], "args").is_empty());
}

#[test]
fn match_variadic_one() {
    let m = find("print($$args)", "func f():\n\tprint(1)\n");
    assert_eq!(m.len(), 1);
    assert_eq!(cap_args(&m[0], "args"), vec!["1"]);
}

#[test]
fn match_variadic_many() {
    let m = find("print($$args)", "func f():\n\tprint(1, 2, 3)\n");
    assert_eq!(m.len(), 1);
    assert_eq!(cap_args(&m[0], "args"), vec!["1", "2", "3"]);
}

#[test]
fn match_variadic_with_fixed() {
    let m = find(
        "$recv.method(\"tag\", $$rest)",
        "func f():\n\tobj.method(\"tag\", 1, 2)\n",
    );
    assert_eq!(m.len(), 1);
    assert_eq!(cap(&m[0], "recv"), "obj");
    assert_eq!(cap_args(&m[0], "rest"), vec!["1", "2"]);
}

// ── Overlapping matches ─────────────────────────────────────────────

#[test]
fn match_overlapping() {
    let m = find("$a + $b", "func f():\n\tvar z = (x + y) + w\n");
    assert_eq!(m.len(), 2);
}

// ── Literal matching ────────────────────────────────────────────────

#[test]
fn match_literal_exact() {
    let m = find(
        "$a.connect(\"ready\", $b)",
        "func f():\n\tobj.connect(\"ready\", cb)\n",
    );
    assert_eq!(m.len(), 1);
    assert_eq!(cap(&m[0], "b"), "cb");
}

#[test]
fn match_literal_mismatch() {
    assert!(
        find(
            "$a.connect(\"ready\", $b)",
            "func f():\n\tobj.connect(\"process\", cb)\n",
        )
        .is_empty()
    );
}

// ── Statement matching ──────────────────────────────────────────────

#[test]
fn match_var_decl() {
    let m = find("var $name = $value", "func f():\n\tvar x = 42\n");
    assert_eq!(m.len(), 1);
    assert_eq!(cap(&m[0], "name"), "x");
    assert_eq!(cap(&m[0], "value"), "42");
}

#[test]
fn match_return_stmt() {
    let m = find("return $x", "func f():\n\treturn foo()\n");
    assert_eq!(m.len(), 1);
    assert_eq!(cap(&m[0], "x"), "foo()");
}

#[test]
fn match_assign_stmt() {
    let m = find("$target = $value", "func f():\n\thealth = 100\n");
    assert_eq!(m.len(), 1);
    assert_eq!(cap(&m[0], "target"), "health");
    assert_eq!(cap(&m[0], "value"), "100");
}

#[test]
fn match_aug_assign_stmt() {
    let m = find("$target -= $value", "func f():\n\thealth -= 10\n");
    assert_eq!(m.len(), 1);
    assert_eq!(cap(&m[0], "value"), "10");
}

// ── Misc ────────────────────────────────────────────────────────────

#[test]
fn match_line_one_based() {
    let m = find("$a + $b", "func f():\n\tvar x = 1 + 2\n");
    assert_eq!(m[0].line, 2);
}

#[test]
fn match_multiple_in_file() {
    let m = find("$x + $y", "func f():\n\tvar a = 1 + 2\n\tvar b = 3 + 4\n");
    assert_eq!(m.len(), 2);
}

#[test]
fn match_chained() {
    let m = find("$x.bar()", "func f():\n\ta.foo().bar()\n");
    assert_eq!(m.len(), 1);
    // $x captures the receiver; its source text includes the full
    // attribute chain because tree-sitter flattens nested attribute
    // nodes and the intermediate receiver shares the outer node's span.
    assert!(m[0].captures.contains_key("x"));
}
