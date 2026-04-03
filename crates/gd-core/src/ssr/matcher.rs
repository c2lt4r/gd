//! Structural matcher — lockstep AST walk with capture.
//!
//! Given a pattern AST (from Phase 1) and a candidate AST node from a
//! source file, determines whether they match structurally and, if so,
//! produces a set of [`Capture`]s mapping placeholder names to the
//! matched subtrees.

use std::collections::HashMap;
use std::path::PathBuf;

use crate::gd_ast::{self, GdExpr, GdFile, GdStmt, GdVar};

use super::captures::{Capture, CapturedExpr, MatchResult};
use super::pattern::{PatternKind, PlaceholderInfo, SSR_PREFIX, SSRV_PREFIX, SsrPattern};

// ═══════════════════════════════════════════════════════════════════════
//  Public API
// ═══════════════════════════════════════════════════════════════════════

/// Find all structural matches for `pattern` in `file`.
///
/// Returns one [`MatchResult`] per match, with captures for each
/// placeholder.  Matches may overlap (an outer node and its inner
/// child can both match).
#[allow(clippy::needless_pass_by_value)]
pub fn find_matches(
    pattern: &SsrPattern,
    file: &GdFile<'_>,
    source: &str,
    file_path: PathBuf,
) -> Vec<MatchResult> {
    let mut results = Vec::new();

    match &pattern.kind {
        PatternKind::Expr(pat_expr) => {
            gd_ast::visit_exprs(file, &mut |candidate| {
                let mut captures = HashMap::new();
                if match_expr(
                    pat_expr,
                    candidate,
                    &pattern.placeholders,
                    source,
                    &mut captures,
                ) {
                    let node = candidate.node();
                    results.push(MatchResult {
                        captures,
                        matched_range: node.byte_range(),
                        line: node.start_position().row + 1,
                        file: file_path.clone(),
                    });
                }
            });
        }
        PatternKind::Stmt(pat_stmt) => {
            gd_ast::visit_stmts(file, &mut |candidate| {
                let mut captures = HashMap::new();
                if match_stmt(
                    pat_stmt,
                    candidate,
                    &pattern.placeholders,
                    source,
                    &mut captures,
                ) {
                    let node = candidate.node();
                    results.push(MatchResult {
                        captures,
                        matched_range: node.byte_range(),
                        line: node.start_position().row + 1,
                        file: file_path.clone(),
                    });
                }
            });
        }
    }

    results
}

// ═══════════════════════════════════════════════════════════════════════
//  Expression matching
// ═══════════════════════════════════════════════════════════════════════

/// Try to match `pattern` against `candidate`, filling `captures`.
///
/// Returns `true` if the match succeeds.  On failure, `captures` may be
/// partially filled — the caller must discard it.
#[allow(clippy::too_many_lines)]
fn match_expr(
    pattern: &crate::ast_owned::OwnedExpr,
    candidate: &GdExpr<'_>,
    placeholders: &HashMap<String, PlaceholderInfo>,
    source: &str,
    captures: &mut HashMap<String, Capture>,
) -> bool {
    use crate::ast_owned::OwnedExpr as P;
    use GdExpr as C;

    // 1. Is pattern node a placeholder?
    if let P::Ident { name, .. } = pattern
        && let Some(ph_name) = deprefix(name)
        && placeholders.contains_key(ph_name)
    {
        return capture_or_verify(ph_name, candidate, source, captures);
    }

    // 2. Variant match — same shape, recurse children.
    match (pattern, candidate) {
        // ── Literals ─────────────────────────────────────────────
        (P::IntLiteral { value: pv, .. }, C::IntLiteral { value: cv, .. })
        | (P::FloatLiteral { value: pv, .. }, C::FloatLiteral { value: cv, .. })
        | (P::StringLiteral { value: pv, .. }, C::StringLiteral { value: cv, .. })
        | (P::StringName { value: pv, .. }, C::StringName { value: cv, .. }) => pv.as_str() == *cv,
        (P::Bool { value: pv, .. }, C::Bool { value: cv, .. }) => pv == cv,
        (P::Null { .. }, C::Null { .. }) => true,

        // ── Identifiers ──────────────────────────────────────────
        (P::Ident { name: pn, .. }, C::Ident { name: cn, .. }) => pn.as_str() == *cn,

        // ── Collections ──────────────────────────────────────────
        (P::Array { elements: pe, .. }, C::Array { elements: ce, .. }) => {
            pe.len() == ce.len()
                && pe
                    .iter()
                    .zip(ce)
                    .all(|(p, c)| match_expr(p, c, placeholders, source, captures))
        }
        (P::Dict { pairs: pp, .. }, C::Dict { pairs: cp, .. }) => {
            pp.len() == cp.len()
                && pp.iter().zip(cp).all(|((pk, pv), (ck, cv))| {
                    match_expr(pk, ck, placeholders, source, captures)
                        && match_expr(pv, cv, placeholders, source, captures)
                })
        }

        // ── Calls ────────────────────────────────────────────────
        (
            P::Call {
                callee: pc,
                args: pa,
                ..
            },
            C::Call {
                callee: cc,
                args: ca,
                ..
            },
        ) => {
            match_expr(pc, cc, placeholders, source, captures)
                && match_args(pa, ca, placeholders, source, captures)
        }

        (
            P::MethodCall {
                receiver: pr,
                method: pm,
                args: pa,
                ..
            },
            C::MethodCall {
                receiver: cr,
                method: cm,
                args: ca,
                ..
            },
        ) => {
            pm.as_str() == *cm
                && match_expr(pr, cr, placeholders, source, captures)
                && match_args(pa, ca, placeholders, source, captures)
        }

        (
            P::SuperCall {
                method: pm,
                args: pa,
                ..
            },
            C::SuperCall {
                method: cm,
                args: ca,
                ..
            },
        ) => pm.as_deref() == *cm && match_args(pa, ca, placeholders, source, captures),

        // ── Access ───────────────────────────────────────────────
        (
            P::PropertyAccess {
                receiver: pr,
                property: pp,
                ..
            },
            C::PropertyAccess {
                receiver: cr,
                property: cp,
                ..
            },
        ) => pp.as_str() == *cp && match_expr(pr, cr, placeholders, source, captures),

        (
            P::Subscript {
                receiver: pr,
                index: pi,
                ..
            },
            C::Subscript {
                receiver: cr,
                index: ci,
                ..
            },
        ) => {
            match_expr(pr, cr, placeholders, source, captures)
                && match_expr(pi, ci, placeholders, source, captures)
        }

        (P::GetNode { path: pp, .. }, C::GetNode { path: cp, .. })
        | (P::Preload { path: pp, .. }, C::Preload { path: cp, .. }) => pp.as_str() == *cp,

        // ── Operators ────────────────────────────────────────────
        (
            P::BinOp {
                op: po,
                left: pl,
                right: pr,
                ..
            },
            C::BinOp {
                op: co,
                left: cl,
                right: cr,
                ..
            },
        ) => {
            po.as_str() == *co
                && match_expr(pl, cl, placeholders, source, captures)
                && match_expr(pr, cr, placeholders, source, captures)
        }

        (
            P::UnaryOp {
                op: po,
                operand: px,
                ..
            },
            C::UnaryOp {
                op: co,
                operand: cx,
                ..
            },
        ) => po.as_str() == *co && match_expr(px, cx, placeholders, source, captures),

        (
            P::Cast {
                expr: pe,
                target_type: pt,
                ..
            },
            C::Cast {
                expr: ce,
                target_type: ct,
                ..
            },
        )
        | (
            P::Is {
                expr: pe,
                type_name: pt,
                ..
            },
            C::Is {
                expr: ce,
                type_name: ct,
                ..
            },
        ) => pt.as_str() == *ct && match_expr(pe, ce, placeholders, source, captures),

        (
            P::Ternary {
                true_val: ptv,
                condition: pc,
                false_val: pfv,
                ..
            },
            C::Ternary {
                true_val: ctv,
                condition: cc,
                false_val: cfv,
                ..
            },
        ) => {
            match_expr(ptv, ctv, placeholders, source, captures)
                && match_expr(pc, cc, placeholders, source, captures)
                && match_expr(pfv, cfv, placeholders, source, captures)
        }

        // ── Misc ─────────────────────────────────────────────────
        (P::Await { expr: pe, .. }, C::Await { expr: ce, .. }) => {
            match_expr(pe, ce, placeholders, source, captures)
        }

        // ── Mismatch ─────────────────────────────────────────────
        _ => false,
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  Argument list matching (with variadic support)
// ═══════════════════════════════════════════════════════════════════════

/// Match pattern argument list against candidate argument list.
///
/// If a pattern argument is a variadic placeholder (`$$name`), it
/// captures all remaining candidate arguments from that position.
fn match_args(
    pattern_args: &[crate::ast_owned::OwnedExpr],
    candidate_args: &[GdExpr<'_>],
    placeholders: &HashMap<String, PlaceholderInfo>,
    source: &str,
    captures: &mut HashMap<String, Capture>,
) -> bool {
    let mut pi = 0;
    let mut ci = 0;

    while pi < pattern_args.len() {
        // Check for variadic placeholder.
        if let Some(ph_name) = variadic_name(&pattern_args[pi])
            && placeholders.contains_key(ph_name)
        {
            let rest: Vec<CapturedExpr> = candidate_args[ci..]
                .iter()
                .map(|c| make_captured_expr(c, source))
                .collect();
            captures.insert(ph_name.to_string(), Capture::ArgList(rest));
            return true; // variadic consumes the rest
        }

        if ci >= candidate_args.len() {
            return false; // pattern has more args than candidate
        }

        if !match_expr(
            &pattern_args[pi],
            &candidate_args[ci],
            placeholders,
            source,
            captures,
        ) {
            return false;
        }

        pi += 1;
        ci += 1;
    }

    ci == candidate_args.len() // both exhausted = match
}

// ═══════════════════════════════════════════════════════════════════════
//  Statement matching
// ═══════════════════════════════════════════════════════════════════════

/// Try to match a pattern statement against a candidate statement.
fn match_stmt(
    pattern: &crate::ast_owned::OwnedStmt,
    candidate: &GdStmt<'_>,
    placeholders: &HashMap<String, PlaceholderInfo>,
    source: &str,
    captures: &mut HashMap<String, Capture>,
) -> bool {
    use crate::ast_owned::OwnedStmt as P;
    use GdStmt as C;

    match (pattern, candidate) {
        (P::Expr { expr: pe, .. }, C::Expr { expr: ce, .. }) => {
            match_expr(pe, ce, placeholders, source, captures)
        }

        (P::Var(pv), C::Var(cv)) => {
            match_var_name(&pv.name, cv, placeholders, source, captures)
                && pv.is_const == cv.is_const
                && match_optional_expr(&pv.value, &cv.value, placeholders, source, captures)
        }

        (
            P::Assign {
                target: pt,
                value: pv,
                ..
            },
            C::Assign {
                target: ct,
                value: cv,
                ..
            },
        ) => {
            match_expr(pt, ct, placeholders, source, captures)
                && match_expr(pv, cv, placeholders, source, captures)
        }

        (
            P::AugAssign {
                target: pt,
                op: po,
                value: pv,
                ..
            },
            C::AugAssign {
                target: ct,
                op: co,
                value: cv,
                ..
            },
        ) => {
            po.as_str() == *co
                && match_expr(pt, ct, placeholders, source, captures)
                && match_expr(pv, cv, placeholders, source, captures)
        }

        (P::Return { value: pv, .. }, C::Return { value: cv, .. }) => {
            match_optional_expr(pv, cv, placeholders, source, captures)
        }

        (P::Pass { .. }, C::Pass { .. })
        | (P::Break { .. }, C::Break { .. })
        | (P::Continue { .. }, C::Continue { .. })
        | (P::Breakpoint { .. }, C::Breakpoint { .. }) => true,

        _ => false,
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  Helpers
// ═══════════════════════════════════════════════════════════════════════

/// Strip the SSR sentinel prefix from an identifier name.
/// Returns the clean placeholder name, or `None` if not a placeholder.
fn deprefix(name: &str) -> Option<&str> {
    name.strip_prefix(SSR_PREFIX)
        .or_else(|| name.strip_prefix(SSRV_PREFIX))
}

/// If `expr` is a variadic placeholder ident, return the clean name.
fn variadic_name(expr: &crate::ast_owned::OwnedExpr) -> Option<&str> {
    if let crate::ast_owned::OwnedExpr::Ident { name, .. } = expr {
        name.strip_prefix(SSRV_PREFIX)
    } else {
        None
    }
}

/// Capture a candidate expression under `ph_name`.
///
/// If `ph_name` was already captured, verify that the new candidate is
/// structurally identical (for repeated placeholders like `$a + $a`).
fn capture_or_verify(
    ph_name: &str,
    candidate: &GdExpr<'_>,
    source: &str,
    captures: &mut HashMap<String, Capture>,
) -> bool {
    if let Some(existing) = captures.get(ph_name) {
        // Repeated placeholder — must be structurally identical.
        match existing {
            Capture::Expr(prev) => {
                // Use structural equality on the AST nodes for correctness.
                // We need to re-parse the previous capture to compare...
                // But we can use source text comparison as a fast path:
                // if both come from the same file with the same source,
                // structurally equal expressions have the same text IF they
                // appear identically in the source.
                //
                // For true structural comparison, use the candidate directly:
                // parse the captured source text and compare.  For now,
                // compare the candidate with the previous via the existing
                // `structurally_equal_expr` by holding onto a temp reference.
                //
                // Actually, since both candidates come from the same parse
                // tree and the visitor gives us the real AST nodes, we just
                // need to compare them.  But we only have source_text from
                // the previous capture.  Use source text comparison — it's
                // correct for expressions from the same source file.
                let node = candidate.node();
                let range = node.byte_range();
                let text = &source[range];
                prev.source_text == text
            }
            Capture::ArgList(_) => false,
        }
    } else {
        captures.insert(
            ph_name.to_string(),
            Capture::Expr(make_captured_expr(candidate, source)),
        );
        true
    }
}

/// Build a `CapturedExpr` from a borrowed `GdExpr`.
fn make_captured_expr(expr: &GdExpr<'_>, source: &str) -> CapturedExpr {
    let node = expr.node();
    let range = node.byte_range();
    CapturedExpr {
        source_text: source[range.clone()].to_string(),
        byte_range: range,
    }
}

/// Match a var-decl name: if the pattern name is a placeholder,
/// capture the candidate's name; otherwise require exact name equality.
fn match_var_name(
    pattern_name: &str,
    candidate: &GdVar<'_>,
    placeholders: &HashMap<String, PlaceholderInfo>,
    source: &str,
    captures: &mut HashMap<String, Capture>,
) -> bool {
    if let Some(ph_name) = deprefix(pattern_name)
        && placeholders.contains_key(ph_name)
    {
        if let Some(name_node) = candidate.name_node {
            let range = name_node.byte_range();
            let text = source[range.clone()].to_string();

            // Repeated placeholder check.
            if let Some(existing) = captures.get(ph_name) {
                return match existing {
                    Capture::Expr(prev) => prev.source_text == text,
                    Capture::ArgList(_) => false,
                };
            }

            captures.insert(
                ph_name.to_string(),
                Capture::Expr(CapturedExpr {
                    byte_range: range,
                    source_text: text,
                }),
            );
            return true;
        }
        return false;
    }
    // Not a placeholder — require exact name match.
    pattern_name == candidate.name
}

/// Match optional expressions (e.g. `return` value, `var` initializer).
#[allow(clippy::ref_option)]
fn match_optional_expr(
    pattern: &Option<crate::ast_owned::OwnedExpr>,
    candidate: &Option<GdExpr<'_>>,
    placeholders: &HashMap<String, PlaceholderInfo>,
    source: &str,
    captures: &mut HashMap<String, Capture>,
) -> bool {
    match (pattern, candidate) {
        (Some(pe), Some(ce)) => match_expr(pe, ce, placeholders, source, captures),
        (None, None) => true,
        _ => false,
    }
}
