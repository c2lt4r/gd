//! SSR pattern parser — preprocess, parse, extract, validate.
//!
//! Converts a user-written pattern string (e.g. `$recv.method($a, $b)`)
//! into a structured [`SsrPattern`] by:
//!
//! 1. **Preprocessing** — replacing `$name` placeholders with `__ssr_name`
//!    sentinel identifiers so tree-sitter can parse the string as valid
//!    GDScript.
//! 2. **Wrapping** — embedding the munged pattern in a minimal GDScript
//!    file (var initializer or function body) so tree-sitter has a
//!    complete compilation unit.
//! 3. **Parsing** — running tree-sitter + `gd_ast::convert()` to get a
//!    typed AST, then converting to owned form.
//! 4. **Extracting** — pulling the relevant expression or statement out
//!    of the wrapper and building the placeholder map.
//! 5. **Validating** — checking variadic placement, template bindings,
//!    and syntax errors.

use std::collections::{HashMap, HashSet};

use miette::{Result, miette};

use crate::ast_owned::{OwnedDecl, OwnedExpr, OwnedFile, OwnedStmt};
use crate::gd_ast;
use crate::parser;

use super::pattern::{
    PatternKind, PlaceholderInfo, SSR_PREFIX, SSRV_PREFIX, SsrPattern, SsrTemplate,
};

/// Name of the wrapper variable used to parse expression patterns.
const WRAPPER_VAR: &str = "__ssr_result__";

/// Name of the wrapper function used to parse statement patterns.
const WRAPPER_FUNC: &str = "__ssr_body__";

// ═══════════════════════════════════════════════════════════════════════
//  Public API
// ═══════════════════════════════════════════════════════════════════════

/// Parse a pattern string into an [`SsrPattern`].
///
/// The pattern is GDScript with `$`-prefixed placeholders:
/// - `$name` — matches any single expression
/// - `$$name` — matches zero or more arguments (variadic, call-position only)
/// - `$name:Type` — type-constrained placeholder (Phase 4)
///
/// Prefix the pattern with `stmt:` to force statement parsing.
pub fn parse_pattern(input: &str) -> Result<SsrPattern> {
    let (force_stmt, raw) = strip_stmt_prefix(input);
    let (munged, constraints) = preprocess(raw)?;

    let kind = if force_stmt {
        parse_as_stmt(&munged)?
    } else {
        parse_as_expr(&munged).or_else(|_| parse_as_stmt(&munged))?
    };

    let placeholders = extract_placeholders(&kind, &constraints);
    validate_variadics(&kind)?;

    Ok(SsrPattern {
        kind,
        placeholders,
        source: input.to_string(),
    })
}

/// Parse a replacement template string into an [`SsrTemplate`].
///
/// Every placeholder in the template must also appear in the search
/// `pattern` (otherwise it would be unbound at replacement time).
pub fn parse_template(input: &str, pattern: &SsrPattern) -> Result<SsrTemplate> {
    let (force_stmt, raw) = strip_stmt_prefix(input);
    let (munged, _constraints) = preprocess(raw)?;

    let kind = if force_stmt {
        parse_as_stmt(&munged)?
    } else {
        match &pattern.kind {
            PatternKind::Expr(_) => parse_as_expr(&munged).or_else(|_| parse_as_stmt(&munged))?,
            PatternKind::Stmt(_) => parse_as_stmt(&munged).or_else(|_| parse_as_expr(&munged))?,
        }
    };

    let placeholders = collect_placeholder_names(&kind);
    validate_template_placeholders(&placeholders, pattern)?;

    Ok(SsrTemplate {
        kind,
        placeholders,
        source: input.to_string(),
    })
}

// ═══════════════════════════════════════════════════════════════════════
//  Preprocessing — $name → __ssr_name
// ═══════════════════════════════════════════════════════════════════════

/// Strip the `stmt:` prefix if present, returning `(is_stmt, rest)`.
fn strip_stmt_prefix(input: &str) -> (bool, &str) {
    input
        .strip_prefix("stmt:")
        .map_or((false, input), |rest| (true, rest))
}

/// Replace `$` placeholders with sentinel identifiers, extract type
/// constraints into a side table.
///
/// - `$name`       → `__ssr_name`
/// - `$$name`      → `__ssrv_name`
/// - `$name:Type`  → `__ssr_name` + constraint `("name", "Type")`
fn preprocess(input: &str) -> Result<(String, HashMap<String, String>)> {
    let mut out = String::with_capacity(input.len() + 32);
    let mut constraints: HashMap<String, String> = HashMap::new();
    let bytes = input.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        if bytes[i] != b'$' {
            out.push(bytes[i] as char);
            i += 1;
            continue;
        }

        // We hit a '$'.  Check for variadic '$$'.
        let variadic = i + 1 < len && bytes[i + 1] == b'$';
        i += if variadic { 2 } else { 1 };

        // Read the identifier name.
        let name_start = i;
        if i >= len || !is_ident_start(bytes[i]) {
            return Err(miette!(
                "expected identifier after '{}'",
                if variadic { "$$" } else { "$" }
            ));
        }
        while i < len && is_ident_continue(bytes[i]) {
            i += 1;
        }
        let name = &input[name_start..i];

        // Check for `:Type` constraint (not valid on variadics).
        if !variadic && i < len && bytes[i] == b':' {
            i += 1; // skip ':'
            let type_start = i;
            if i >= len || !is_ident_start(bytes[i]) {
                return Err(miette!("expected type name after ':' in '${name}:'"));
            }
            while i < len && is_ident_continue(bytes[i]) {
                i += 1;
            }
            let type_name = &input[type_start..i];
            constraints.insert(name.to_string(), type_name.to_string());
        }

        // Emit the sentinel identifier.
        if variadic {
            out.push_str(SSRV_PREFIX);
        } else {
            out.push_str(SSR_PREFIX);
        }
        out.push_str(name);
    }

    Ok((out, constraints))
}

fn is_ident_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_'
}

fn is_ident_continue(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

// ═══════════════════════════════════════════════════════════════════════
//  Wrapping + parsing
// ═══════════════════════════════════════════════════════════════════════

/// Try to parse the munged pattern as an expression by wrapping it in
/// `var __ssr_result__ = <pattern>`.
fn parse_as_expr(munged: &str) -> Result<PatternKind> {
    let wrapped = format!("var {WRAPPER_VAR} = {munged}\n");
    let tree = parser::parse(&wrapped)?;

    if tree.root_node().has_error() {
        return Err(miette!("pattern has syntax errors (tried as expression)"));
    }

    let file = gd_ast::convert(&tree, &wrapped);
    let owned = OwnedFile::from_borrowed(&file);
    let expr = extract_expr_from_var(&owned)?;
    Ok(PatternKind::Expr(expr))
}

/// Try to parse the munged pattern as a statement by wrapping it in
/// `func __ssr_body__():\n\t<pattern>`.
fn parse_as_stmt(munged: &str) -> Result<PatternKind> {
    let wrapped = format!("func {WRAPPER_FUNC}():\n\t{munged}\n");
    let tree = parser::parse(&wrapped)?;

    if tree.root_node().has_error() {
        return Err(miette!("pattern has syntax errors"));
    }

    let file = gd_ast::convert(&tree, &wrapped);
    let owned = OwnedFile::from_borrowed(&file);
    let stmt = extract_stmt_from_func(&owned)?;
    Ok(PatternKind::Stmt(Box::new(stmt)))
}

/// Pull the initializer expression out of `var __ssr_result__ = <expr>`.
fn extract_expr_from_var(file: &OwnedFile) -> Result<OwnedExpr> {
    for decl in &file.declarations {
        if let OwnedDecl::Var(var) = decl
            && var.name == WRAPPER_VAR
        {
            return var
                .value
                .clone()
                .ok_or_else(|| miette!("pattern wrapper var has no initializer"));
        }
    }
    Err(miette!("could not extract expression from pattern wrapper"))
}

/// Pull the first statement out of `func __ssr_body__(): <stmts>`.
fn extract_stmt_from_func(file: &OwnedFile) -> Result<OwnedStmt> {
    for decl in &file.declarations {
        if let OwnedDecl::Func(func) = decl
            && func.name == WRAPPER_FUNC
        {
            return func
                .body
                .first()
                .cloned()
                .ok_or_else(|| miette!("pattern wrapper function has empty body"));
        }
    }
    Err(miette!("could not extract statement from pattern wrapper"))
}

// ═══════════════════════════════════════════════════════════════════════
//  Placeholder extraction
// ═══════════════════════════════════════════════════════════════════════

/// Walk the pattern AST to collect all placeholder identifiers.
fn extract_placeholders(
    kind: &PatternKind,
    constraints: &HashMap<String, String>,
) -> HashMap<String, PlaceholderInfo> {
    let mut out = HashMap::new();
    match kind {
        PatternKind::Expr(expr) => walk_expr_idents(expr, &mut |name| {
            insert_placeholder(name, constraints, &mut out);
        }),
        PatternKind::Stmt(stmt) => walk_stmt_idents(stmt, &mut |name| {
            insert_placeholder(name, constraints, &mut out);
        }),
    }
    out
}

/// If `name` is a sentinel identifier, record the placeholder.
fn insert_placeholder(
    name: &str,
    constraints: &HashMap<String, String>,
    out: &mut HashMap<String, PlaceholderInfo>,
) {
    if let Some(ph) = name.strip_prefix(SSRV_PREFIX) {
        out.entry(ph.to_string()).or_insert(PlaceholderInfo {
            variadic: true,
            type_constraint: None,
        });
    } else if let Some(ph) = name.strip_prefix(SSR_PREFIX) {
        out.entry(ph.to_string()).or_insert(PlaceholderInfo {
            variadic: false,
            type_constraint: constraints.get(ph).cloned(),
        });
    }
}

/// Collect placeholder names from a template AST (names only, no info).
fn collect_placeholder_names(kind: &PatternKind) -> HashSet<String> {
    let mut names = HashSet::new();
    let mut visitor = |ident_name: &str| {
        if let Some(ph) = ident_name
            .strip_prefix(SSRV_PREFIX)
            .or_else(|| ident_name.strip_prefix(SSR_PREFIX))
        {
            names.insert(ph.to_string());
        }
    };
    match kind {
        PatternKind::Expr(expr) => walk_expr_idents(expr, &mut visitor),
        PatternKind::Stmt(stmt) => walk_stmt_idents(stmt, &mut visitor),
    }
    names
}

// ═══════════════════════════════════════════════════════════════════════
//  Validation
// ═══════════════════════════════════════════════════════════════════════

/// Ensure variadic placeholders (`$$args`) appear only as direct
/// arguments in `Call`, `MethodCall`, or `SuperCall` nodes.
fn validate_variadics(kind: &PatternKind) -> Result<()> {
    match kind {
        PatternKind::Expr(expr) => check_variadics_expr(expr),
        PatternKind::Stmt(stmt) => check_variadics_stmt(stmt),
    }
}

/// Check that every placeholder in the template is bound in the pattern.
fn validate_template_placeholders(
    template_phs: &HashSet<String>,
    pattern: &SsrPattern,
) -> Result<()> {
    for name in template_phs {
        if !pattern.placeholders.contains_key(name) {
            return Err(miette!(
                "template placeholder '${name}' does not appear in the search pattern"
            ));
        }
    }
    Ok(())
}

// ── Variadic validation (expressions) ────────────────────────────────

/// Reject variadic idents that are not direct children of a call's arg
/// list.  Call/MethodCall/SuperCall args are checked separately — direct
/// variadic args are allowed, everything else is recursed normally.
fn check_variadics_expr(expr: &OwnedExpr) -> Result<()> {
    match expr {
        OwnedExpr::Ident { name, .. } => {
            if let Some(ph) = name.strip_prefix(SSRV_PREFIX) {
                return Err(miette!(
                    "variadic placeholder '$${}' can only appear as a \
                     direct argument in a function or method call",
                    ph
                ));
            }
            Ok(())
        }

        // Calls — allow variadic direct args, recurse into the rest.
        OwnedExpr::Call { callee, args, .. } => {
            check_variadics_expr(callee)?;
            check_variadics_args(args)
        }
        OwnedExpr::MethodCall { receiver, args, .. } => {
            check_variadics_expr(receiver)?;
            check_variadics_args(args)
        }
        OwnedExpr::SuperCall { args, .. } => check_variadics_args(args),

        // Binary / unary
        OwnedExpr::BinOp { left, right, .. } => {
            check_variadics_expr(left)?;
            check_variadics_expr(right)
        }
        OwnedExpr::UnaryOp { operand, .. } => check_variadics_expr(operand),

        // Access
        OwnedExpr::PropertyAccess { receiver, .. } => check_variadics_expr(receiver),
        OwnedExpr::Subscript {
            receiver, index, ..
        } => {
            check_variadics_expr(receiver)?;
            check_variadics_expr(index)
        }

        // Cast / Is / Await
        OwnedExpr::Cast { expr, .. }
        | OwnedExpr::Is { expr, .. }
        | OwnedExpr::Await { expr, .. } => check_variadics_expr(expr),

        // Ternary
        OwnedExpr::Ternary {
            true_val,
            condition,
            false_val,
            ..
        } => {
            check_variadics_expr(true_val)?;
            check_variadics_expr(condition)?;
            check_variadics_expr(false_val)
        }

        // Collections
        OwnedExpr::Array { elements, .. } => {
            for el in elements {
                check_variadics_expr(el)?;
            }
            Ok(())
        }
        OwnedExpr::Dict { pairs, .. } => {
            for (k, v) in pairs {
                check_variadics_expr(k)?;
                check_variadics_expr(v)?;
            }
            Ok(())
        }

        // Lambda — recurse into body statements
        OwnedExpr::Lambda { func, .. } => {
            for stmt in &func.body {
                check_variadics_stmt(stmt)?;
            }
            Ok(())
        }

        // Leaves — no children to check.
        OwnedExpr::IntLiteral { .. }
        | OwnedExpr::FloatLiteral { .. }
        | OwnedExpr::StringLiteral { .. }
        | OwnedExpr::StringName { .. }
        | OwnedExpr::Bool { .. }
        | OwnedExpr::Null { .. }
        | OwnedExpr::GetNode { .. }
        | OwnedExpr::Preload { .. }
        | OwnedExpr::Invalid { .. } => Ok(()),
    }
}

/// For a call's argument list, skip direct variadic args (allowed) and
/// recurse into all other arguments.
fn check_variadics_args(args: &[OwnedExpr]) -> Result<()> {
    for arg in args {
        if matches!(arg, OwnedExpr::Ident { name, .. } if name.starts_with(SSRV_PREFIX)) {
            continue; // variadic in arg position — allowed
        }
        check_variadics_expr(arg)?;
    }
    Ok(())
}

// ── Variadic validation (statements) ─────────────────────────────────

fn check_variadics_stmt(stmt: &OwnedStmt) -> Result<()> {
    match stmt {
        OwnedStmt::Expr { expr, .. } => check_variadics_expr(expr),
        OwnedStmt::Var(var) => {
            if let Some(val) = &var.value {
                check_variadics_expr(val)?;
            }
            Ok(())
        }
        OwnedStmt::Assign { target, value, .. } | OwnedStmt::AugAssign { target, value, .. } => {
            check_variadics_expr(target)?;
            check_variadics_expr(value)
        }
        OwnedStmt::Return { value, .. } => {
            if let Some(v) = value {
                check_variadics_expr(v)?;
            }
            Ok(())
        }
        OwnedStmt::If(if_stmt) => {
            check_variadics_expr(&if_stmt.condition)?;
            for s in &if_stmt.body {
                check_variadics_stmt(s)?;
            }
            for (cond, body) in &if_stmt.elif_branches {
                check_variadics_expr(cond)?;
                for s in body {
                    check_variadics_stmt(s)?;
                }
            }
            if let Some(els) = &if_stmt.else_body {
                for s in els {
                    check_variadics_stmt(s)?;
                }
            }
            Ok(())
        }
        OwnedStmt::For { iter, body, .. } => {
            check_variadics_expr(iter)?;
            for s in body {
                check_variadics_stmt(s)?;
            }
            Ok(())
        }
        OwnedStmt::While {
            condition, body, ..
        } => {
            check_variadics_expr(condition)?;
            for s in body {
                check_variadics_stmt(s)?;
            }
            Ok(())
        }
        OwnedStmt::Match { value, arms, .. } => {
            check_variadics_expr(value)?;
            for arm in arms {
                for pat in &arm.patterns {
                    check_variadics_expr(pat)?;
                }
                if let Some(g) = &arm.guard {
                    check_variadics_expr(g)?;
                }
                for s in &arm.body {
                    check_variadics_stmt(s)?;
                }
            }
            Ok(())
        }
        OwnedStmt::Pass { .. }
        | OwnedStmt::Break { .. }
        | OwnedStmt::Continue { .. }
        | OwnedStmt::Breakpoint { .. }
        | OwnedStmt::Invalid { .. } => Ok(()),
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  AST walkers — visit every Ident in an owned tree
// ═══════════════════════════════════════════════════════════════════════

/// Call `f` with the `name` of every `Ident` node found in `expr`.
fn walk_expr_idents(expr: &OwnedExpr, f: &mut impl FnMut(&str)) {
    match expr {
        OwnedExpr::Ident { name, .. } => f(name),

        // Calls
        OwnedExpr::Call { callee, args, .. } => {
            walk_expr_idents(callee, f);
            for a in args {
                walk_expr_idents(a, f);
            }
        }
        OwnedExpr::MethodCall { receiver, args, .. } => {
            walk_expr_idents(receiver, f);
            for a in args {
                walk_expr_idents(a, f);
            }
        }
        OwnedExpr::SuperCall { args, .. } => {
            for a in args {
                walk_expr_idents(a, f);
            }
        }

        // Binary / unary
        OwnedExpr::BinOp { left, right, .. } => {
            walk_expr_idents(left, f);
            walk_expr_idents(right, f);
        }
        OwnedExpr::UnaryOp { operand, .. } => walk_expr_idents(operand, f),

        // Access
        OwnedExpr::PropertyAccess { receiver, .. } => {
            walk_expr_idents(receiver, f);
        }
        OwnedExpr::Subscript {
            receiver, index, ..
        } => {
            walk_expr_idents(receiver, f);
            walk_expr_idents(index, f);
        }

        // Cast / Is / Await
        OwnedExpr::Cast { expr, .. }
        | OwnedExpr::Is { expr, .. }
        | OwnedExpr::Await { expr, .. } => walk_expr_idents(expr, f),

        // Ternary
        OwnedExpr::Ternary {
            true_val,
            condition,
            false_val,
            ..
        } => {
            walk_expr_idents(true_val, f);
            walk_expr_idents(condition, f);
            walk_expr_idents(false_val, f);
        }

        // Collections
        OwnedExpr::Array { elements, .. } => {
            for el in elements {
                walk_expr_idents(el, f);
            }
        }
        OwnedExpr::Dict { pairs, .. } => {
            for (k, v) in pairs {
                walk_expr_idents(k, f);
                walk_expr_idents(v, f);
            }
        }

        // Lambda
        OwnedExpr::Lambda { func, .. } => {
            for stmt in &func.body {
                walk_stmt_idents(stmt, f);
            }
        }

        // Leaves
        OwnedExpr::IntLiteral { .. }
        | OwnedExpr::FloatLiteral { .. }
        | OwnedExpr::StringLiteral { .. }
        | OwnedExpr::StringName { .. }
        | OwnedExpr::Bool { .. }
        | OwnedExpr::Null { .. }
        | OwnedExpr::GetNode { .. }
        | OwnedExpr::Preload { .. }
        | OwnedExpr::Invalid { .. } => {}
    }
}

/// Call `f` with the `name` of every `Ident` node found in `stmt`.
fn walk_stmt_idents(stmt: &OwnedStmt, f: &mut impl FnMut(&str)) {
    match stmt {
        OwnedStmt::Expr { expr, .. } => walk_expr_idents(expr, f),
        OwnedStmt::Var(var) => {
            // The var name itself may be a placeholder (e.g. `var $name = ...`).
            f(&var.name);
            if let Some(val) = &var.value {
                walk_expr_idents(val, f);
            }
        }
        OwnedStmt::Assign { target, value, .. } | OwnedStmt::AugAssign { target, value, .. } => {
            walk_expr_idents(target, f);
            walk_expr_idents(value, f);
        }
        OwnedStmt::Return { value, .. } => {
            if let Some(v) = value {
                walk_expr_idents(v, f);
            }
        }
        OwnedStmt::If(if_stmt) => {
            walk_expr_idents(&if_stmt.condition, f);
            for s in &if_stmt.body {
                walk_stmt_idents(s, f);
            }
            for (cond, body) in &if_stmt.elif_branches {
                walk_expr_idents(cond, f);
                for s in body {
                    walk_stmt_idents(s, f);
                }
            }
            if let Some(els) = &if_stmt.else_body {
                for s in els {
                    walk_stmt_idents(s, f);
                }
            }
        }
        OwnedStmt::For {
            var, iter, body, ..
        } => {
            f(var);
            walk_expr_idents(iter, f);
            for s in body {
                walk_stmt_idents(s, f);
            }
        }
        OwnedStmt::While {
            condition, body, ..
        } => {
            walk_expr_idents(condition, f);
            for s in body {
                walk_stmt_idents(s, f);
            }
        }
        OwnedStmt::Match { value, arms, .. } => {
            walk_expr_idents(value, f);
            for arm in arms {
                for pat in &arm.patterns {
                    walk_expr_idents(pat, f);
                }
                if let Some(g) = &arm.guard {
                    walk_expr_idents(g, f);
                }
                for s in &arm.body {
                    walk_stmt_idents(s, f);
                }
            }
        }
        OwnedStmt::Pass { .. }
        | OwnedStmt::Break { .. }
        | OwnedStmt::Continue { .. }
        | OwnedStmt::Breakpoint { .. }
        | OwnedStmt::Invalid { .. } => {}
    }
}
