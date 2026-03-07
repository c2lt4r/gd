use std::fmt::Write;

use gd_core::gd_ast::GdExpr;

use crate::error::{InterpError, InterpResult};
use crate::eval::eval_expr;
use crate::interpreter::Interpreter;
use crate::value::GdValue;

/// A subexpression with its source text, column offset, and evaluated value.
struct SubExpr {
    /// Column offset relative to the assertion line start.
    col: usize,
    /// Source text of the subexpression.
    text: String,
    /// Evaluated value.
    value: GdValue,
}

/// Check if a function name is an assertion builtin that should use power assertions.
#[must_use]
pub fn is_assertion(name: &str) -> bool {
    matches!(
        name,
        "assert_true"
            | "assert_false"
            | "assert_eq"
            | "assert_ne"
            | "assert_gt"
            | "assert_lt"
            | "assert_null"
            | "assert_not_null"
    )
}

/// Execute a power assertion: evaluate args with AST decomposition,
/// and on failure produce a Groovy/Spock-style diagnostic.
#[allow(clippy::too_many_lines)]
pub fn exec_power_assert(
    name: &str,
    arg_exprs: &[GdExpr<'_>],
    interp: &mut Interpreter<'_>,
    line: usize,
    col: usize,
) -> InterpResult<GdValue> {
    // Evaluate all arguments
    let args: InterpResult<Vec<GdValue>> = arg_exprs.iter().map(|a| eval_expr(a, interp)).collect();
    let args = args?;

    // Check the assertion condition
    let failed = match name {
        "assert_true" => {
            if args.is_empty() {
                return Err(InterpError::argument_error(
                    "assert_true() requires at least 1 argument",
                    line,
                    col,
                ));
            }
            !args[0].is_truthy()
        }
        "assert_false" => {
            if args.is_empty() {
                return Err(InterpError::argument_error(
                    "assert_false() requires at least 1 argument",
                    line,
                    col,
                ));
            }
            args[0].is_truthy()
        }
        "assert_eq" => {
            if args.len() < 2 {
                return Err(InterpError::argument_error(
                    "assert_eq() requires at least 2 arguments",
                    line,
                    col,
                ));
            }
            args[0] != args[1]
        }
        "assert_ne" => {
            if args.len() < 2 {
                return Err(InterpError::argument_error(
                    "assert_ne() requires at least 2 arguments",
                    line,
                    col,
                ));
            }
            args[0] == args[1]
        }
        "assert_gt" => {
            if args.len() < 2 {
                return Err(InterpError::argument_error(
                    "assert_gt() requires at least 2 arguments",
                    line,
                    col,
                ));
            }
            !gt_check(&args[0], &args[1])
        }
        "assert_lt" => {
            if args.len() < 2 {
                return Err(InterpError::argument_error(
                    "assert_lt() requires at least 2 arguments",
                    line,
                    col,
                ));
            }
            !lt_check(&args[0], &args[1])
        }
        "assert_null" => {
            if args.is_empty() {
                return Err(InterpError::argument_error(
                    "assert_null() requires at least 1 argument",
                    line,
                    col,
                ));
            }
            args[0] != GdValue::Null
        }
        "assert_not_null" => {
            if args.is_empty() {
                return Err(InterpError::argument_error(
                    "assert_not_null() requires at least 1 argument",
                    line,
                    col,
                ));
            }
            args[0] == GdValue::Null
        }
        _ => {
            return Err(InterpError::name_error(
                format!("unknown assertion: {name}"),
                line,
                col,
            ));
        }
    };

    if !failed {
        return Ok(GdValue::Null);
    }

    // Check if the user provided a custom message as the last arg
    let has_custom_msg = match name {
        "assert_true" | "assert_false" | "assert_null" | "assert_not_null" => args.len() > 1,
        _ => args.len() > 2,
    };

    // Build the power assertion message
    let msg = if let Some(source) = interp.source {
        build_power_assert_message(name, arg_exprs, &args, source, interp, has_custom_msg)
    } else {
        build_fallback_message(name, &args)
    };

    Err(InterpError::assertion_failed(msg, line, col))
}

fn gt_check(a: &GdValue, b: &GdValue) -> bool {
    match (a, b) {
        (GdValue::Int(a), GdValue::Int(b)) => a > b,
        (GdValue::Float(a), GdValue::Float(b)) => a > b,
        (GdValue::Int(a), GdValue::Float(b)) => (*a as f64) > *b,
        (GdValue::Float(a), GdValue::Int(b)) => *a > (*b as f64),
        _ => false,
    }
}

fn lt_check(a: &GdValue, b: &GdValue) -> bool {
    match (a, b) {
        (GdValue::Int(a), GdValue::Int(b)) => a < b,
        (GdValue::Float(a), GdValue::Float(b)) => a < b,
        (GdValue::Int(a), GdValue::Float(b)) => (*a as f64) < *b,
        (GdValue::Float(a), GdValue::Int(b)) => *a < (*b as f64),
        _ => false,
    }
}

/// Build a Groovy/Spock-style power assertion message.
///
/// Example output:
/// ```text
/// assert_eq(a + b, expected)
///           |   |  |
///           7   4  10
///           |
///           3
/// ```
#[allow(clippy::too_many_lines)]
fn build_power_assert_message(
    name: &str,
    arg_exprs: &[GdExpr<'_>],
    args: &[GdValue],
    source: &str,
    interp: &mut Interpreter<'_>,
    has_custom_msg: bool,
) -> String {
    // Reconstruct the assertion call source text from the first arg's node
    // We use the call node's start/end to get the full assertion text
    let assertion_text = if let Some(first_arg) = arg_exprs.first() {
        // Find the assertion line in source
        let arg_line = first_arg.node().start_position().row;
        source.lines().nth(arg_line).unwrap_or("")
    } else {
        ""
    };

    // Trim leading whitespace to get the assertion text
    let trimmed = assertion_text.trim();

    // Collect subexpressions from each argument
    let mut subs = Vec::new();

    // Determine which args are assertion arguments (not the custom message)
    let assertion_arg_count = if has_custom_msg {
        match name {
            "assert_true" | "assert_false" | "assert_null" | "assert_not_null" => 1,
            _ => 2,
        }
    } else {
        arg_exprs.len()
    };

    // The base column is the start of the trimmed assertion line
    let line_start_col = assertion_text.len() - assertion_text.trim_start().len();

    for (i, arg_expr) in arg_exprs.iter().take(assertion_arg_count).enumerate() {
        collect_subexprs(
            arg_expr,
            &args[i],
            source,
            line_start_col,
            interp,
            &mut subs,
        );
    }

    if subs.is_empty() {
        return build_fallback_message(name, args);
    }

    // Sort by column position
    subs.sort_by_key(|s| s.col);

    // Deduplicate: remove entries whose text is a simple literal (same as value display)
    subs.retain(|s| {
        let val_str = s.value.to_string();
        s.text != val_str && s.text != format!("\"{val_str}\"")
    });

    if subs.is_empty() {
        return build_fallback_message(name, args);
    }

    // Build the output
    let mut out = String::new();

    // Custom message first if present
    if has_custom_msg && let Some(msg_val) = args.last() {
        let _ = writeln!(out, "{msg_val}");
    }

    // Line 1: the assertion source text
    let _ = writeln!(out, "{trimmed}");

    // Build pointer lines: each subexpression gets a | at its column,
    // and its value printed on the line where no lower subexpression needs a |
    // Strategy: render from right to left, bottom to top
    // Each sub gets its own value line, with | pipes for all subs to its left

    for i in (0..subs.len()).rev() {
        // Pipe line: show | at every sub's column position
        let mut pipe_line = String::new();
        let mut last_col = 0;
        for sub in subs.iter().take(i + 1) {
            let offset = sub.col.saturating_sub(last_col);
            for _ in 0..offset {
                pipe_line.push(' ');
            }
            pipe_line.push('|');
            last_col = sub.col + 1;
        }
        let _ = writeln!(out, "{pipe_line}");

        // Value line: show | at columns before this one, then the value at this column
        let mut val_line = String::new();
        let mut last_col = 0;
        for (j, sub) in subs.iter().enumerate() {
            if j < i {
                let offset = sub.col.saturating_sub(last_col);
                for _ in 0..offset {
                    val_line.push(' ');
                }
                val_line.push('|');
                last_col = sub.col + 1;
            } else if j == i {
                let offset = sub.col.saturating_sub(last_col);
                for _ in 0..offset {
                    val_line.push(' ');
                }
                let _ = write!(val_line, "{}", sub.value);
                break;
            }
        }
        let _ = writeln!(out, "{val_line}");
    }

    // Trim trailing newline
    while out.ends_with('\n') {
        out.pop();
    }

    out
}

/// Find the column position of a binary operator between its left and right operands.
/// Falls back to the expression start column if the operator can't be found.
fn find_op_col(left: &GdExpr<'_>, op: &str, source: &str, line_start_col: usize) -> usize {
    let left_end = left.node().end_byte();
    // Search for the operator token in the source between left end and a reasonable range
    let search_end = (left_end + op.len() + 4).min(source.len());
    if left_end < search_end {
        let between = &source[left_end..search_end];
        if let Some(offset) = between.find(op) {
            let op_byte = left_end + offset;
            let line_start = source[..op_byte].rfind('\n').map_or(0, |p| p + 1);
            return (op_byte - line_start).saturating_sub(line_start_col);
        }
    }
    left.node()
        .start_position()
        .column
        .saturating_sub(line_start_col)
}

/// Recursively collect subexpressions that are "interesting" (not literals).
fn collect_subexprs(
    expr: &GdExpr<'_>,
    value: &GdValue,
    source: &str,
    line_start_col: usize,
    interp: &mut Interpreter<'_>,
    subs: &mut Vec<SubExpr>,
) {
    let node = expr.node();
    let start_col = node.start_position().column;

    // Get source text for this expression
    let start = node.start_byte();
    let end = node.end_byte();
    let text = if end <= source.len() {
        source[start..end].to_owned()
    } else {
        return;
    };

    // Skip if this is on a different line than the start of the expression
    if node.start_position().row != node.end_position().row {
        let col = start_col.saturating_sub(line_start_col);
        subs.push(SubExpr {
            col,
            text,
            value: value.clone(),
        });
        return;
    }

    // For BinOp, place the value under the operator (Spock-style).
    // For everything else, use the expression start column.
    let col = if let GdExpr::BinOp { left, op, .. } = expr {
        find_op_col(left, op, source, line_start_col)
    } else {
        start_col.saturating_sub(line_start_col)
    };

    subs.push(SubExpr {
        col,
        text,
        value: value.clone(),
    });

    // Recurse into interesting child subexpressions
    match expr {
        GdExpr::BinOp { left, right, .. } => {
            if let Ok(lv) = eval_expr(left, interp) {
                collect_subexprs(left, &lv, source, line_start_col, interp, subs);
            }
            if let Ok(rv) = eval_expr(right, interp) {
                collect_subexprs(right, &rv, source, line_start_col, interp, subs);
            }
        }
        GdExpr::UnaryOp { operand, .. } => {
            if let Ok(v) = eval_expr(operand, interp) {
                collect_subexprs(operand, &v, source, line_start_col, interp, subs);
            }
        }
        GdExpr::Call { callee, args, .. } => {
            if !matches!(callee.as_ref(), GdExpr::Ident { .. })
                && let Ok(v) = eval_expr(callee, interp)
            {
                collect_subexprs(callee, &v, source, line_start_col, interp, subs);
            }
            for arg in args {
                if let Ok(v) = eval_expr(arg, interp) {
                    collect_subexprs(arg, &v, source, line_start_col, interp, subs);
                }
            }
        }
        GdExpr::MethodCall { receiver, args, .. } => {
            if let Ok(v) = eval_expr(receiver, interp) {
                collect_subexprs(receiver, &v, source, line_start_col, interp, subs);
            }
            for arg in args {
                if let Ok(v) = eval_expr(arg, interp) {
                    collect_subexprs(arg, &v, source, line_start_col, interp, subs);
                }
            }
        }
        GdExpr::PropertyAccess { receiver, .. } => {
            if let Ok(v) = eval_expr(receiver, interp) {
                collect_subexprs(receiver, &v, source, line_start_col, interp, subs);
            }
        }
        GdExpr::Subscript {
            receiver, index, ..
        } => {
            if let Ok(v) = eval_expr(receiver, interp) {
                collect_subexprs(receiver, &v, source, line_start_col, interp, subs);
            }
            if let Ok(v) = eval_expr(index, interp) {
                collect_subexprs(index, &v, source, line_start_col, interp, subs);
            }
        }
        GdExpr::Ternary {
            condition,
            true_val,
            false_val,
            ..
        } => {
            if let Ok(v) = eval_expr(condition, interp) {
                collect_subexprs(condition, &v, source, line_start_col, interp, subs);
            }
            if let Ok(v) = eval_expr(true_val, interp) {
                collect_subexprs(true_val, &v, source, line_start_col, interp, subs);
            }
            if let Ok(v) = eval_expr(false_val, interp) {
                collect_subexprs(false_val, &v, source, line_start_col, interp, subs);
            }
        }
        _ => {}
    }
}

/// Fallback message when source text is unavailable.
fn build_fallback_message(name: &str, args: &[GdValue]) -> String {
    match name {
        "assert_true" => format!("assert_true failed: got {}", args[0]),
        "assert_false" => format!("assert_false failed: got {}", args[0]),
        "assert_eq" => format!(
            "assert_eq failed\n  left:  {}\n  right: {}",
            args[0], args[1]
        ),
        "assert_ne" => format!("assert_ne failed: both sides equal {}", args[0]),
        "assert_gt" => format!("assert_gt failed: {} > {} is false", args[0], args[1]),
        "assert_lt" => format!("assert_lt failed: {} < {} is false", args[0], args[1]),
        "assert_null" => format!("assert_null failed: got {}", args[0]),
        "assert_not_null" => "assert_not_null failed: got null".to_owned(),
        _ => format!("{name} failed"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_assertion(code: &str) -> Result<GdValue, InterpError> {
        let source = format!("func test():\n\t{code}\n");
        let tree = gd_core::parser::parse(&source).expect("parse failed");
        let file = gd_core::gd_ast::convert(&tree, &source);
        let mut interp =
            Interpreter::from_file_with_source(&file, &source).expect("interpreter init failed");

        let func = interp.lookup_func("test").expect("test func not found");
        crate::exec::exec_func(func, &[], &mut interp)
    }

    #[test]
    fn power_assert_eq_shows_subexpressions() {
        let result = run_assertion("var a = 3\n\tvar b = 4\n\tassert_eq(a + b, 10)");
        let err = result.unwrap_err();
        assert_eq!(err.kind, crate::error::ErrorKind::AssertionFailed);
        // Should contain the assertion line and subexpression values
        assert!(
            err.message.contains("assert_eq"),
            "message should contain assertion name: {}",
            err.message
        );
        assert!(
            err.message.contains('7') || err.message.contains("a + b"),
            "message should show decomposed values: {}",
            err.message
        );
    }

    #[test]
    fn power_assert_true_shows_expression() {
        let result = run_assertion("var x = 5\n\tassert_true(x > 10)");
        let err = result.unwrap_err();
        assert_eq!(err.kind, crate::error::ErrorKind::AssertionFailed);
        assert!(
            err.message.contains("assert_true"),
            "message: {}",
            err.message
        );
    }

    #[test]
    fn power_assert_passes_on_success() {
        let result = run_assertion("assert_eq(2 + 2, 4)");
        assert!(result.is_ok());
    }

    #[test]
    fn power_assert_with_custom_message() {
        let result = run_assertion("var x = 1\n\tassert_eq(x, 2, \"x should be 2\")");
        let err = result.unwrap_err();
        assert!(
            err.message.contains("x should be 2"),
            "should include custom message: {}",
            err.message
        );
    }

    #[test]
    fn power_assert_ne_failure() {
        let result = run_assertion("var a = 5\n\tassert_ne(a, 5)");
        let err = result.unwrap_err();
        assert_eq!(err.kind, crate::error::ErrorKind::AssertionFailed);
        assert!(
            err.message.contains("assert_ne"),
            "message: {}",
            err.message
        );
    }

    #[test]
    fn power_assert_null_failure() {
        let result = run_assertion("var x = 42\n\tassert_null(x)");
        let err = result.unwrap_err();
        assert_eq!(err.kind, crate::error::ErrorKind::AssertionFailed);
    }

    #[test]
    fn power_assert_not_null_failure() {
        let result = run_assertion("var x = null\n\tassert_not_null(x)");
        let err = result.unwrap_err();
        assert_eq!(err.kind, crate::error::ErrorKind::AssertionFailed);
    }

    #[test]
    fn fallback_when_no_source() {
        let msg = build_fallback_message("assert_eq", &[GdValue::Int(1), GdValue::Int(2)]);
        assert!(msg.contains("left:  1"));
        assert!(msg.contains("right: 2"));
    }
}
