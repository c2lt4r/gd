use std::fmt::Write;

use gd_core::gd_ast::GdExpr;

use crate::builtins;
use crate::env::Environment;
use crate::error::{InterpError, InterpResult};
use crate::value::GdValue;

/// Strip surrounding quotes and process escape sequences in a GDScript string literal.
#[must_use]
pub fn eval_string_literal(raw: &str) -> String {
    // Strip surrounding quotes: "...", '...', """...""", '''...'''
    let inner = if raw.starts_with("\"\"\"") || raw.starts_with("'''") {
        &raw[3..raw.len() - 3]
    } else if raw.starts_with('"') || raw.starts_with('\'') {
        &raw[1..raw.len() - 1]
    } else {
        raw
    };

    let mut result = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => result.push('\n'),
                Some('t') => result.push('\t'),
                Some('r') => result.push('\r'),
                Some('\\') | None => result.push('\\'),
                Some('"') => result.push('"'),
                Some('\'') => result.push('\''),
                Some('0') => result.push('\0'),
                Some('a') => result.push('\x07'),
                Some('b') => result.push('\x08'),
                Some('f') => result.push('\x0C'),
                Some(other) => {
                    result.push('\\');
                    result.push(other);
                }
            }
        } else {
            result.push(c);
        }
    }
    result
}

/// Parse an integer literal (decimal, hex 0x, binary 0b, octal 0o).
fn parse_int(s: &str) -> Option<i64> {
    // GDScript allows underscores in number literals
    let s = s.replace('_', "");
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        i64::from_str_radix(hex, 16).ok()
    } else if let Some(bin) = s.strip_prefix("0b").or_else(|| s.strip_prefix("0B")) {
        i64::from_str_radix(bin, 2).ok()
    } else if let Some(oct) = s.strip_prefix("0o").or_else(|| s.strip_prefix("0O")) {
        i64::from_str_radix(oct, 8).ok()
    } else {
        s.parse().ok()
    }
}

/// Evaluate a GDScript expression AST node.
#[allow(clippy::too_many_lines)]
pub fn eval_expr(expr: &GdExpr<'_>, env: &mut Environment) -> InterpResult<GdValue> {
    let line = expr.line();
    let col = expr.column();

    match expr {
        // ── Literals ───────────────────────────────────────────────────
        GdExpr::IntLiteral { value, .. } => parse_int(value).map(GdValue::Int).ok_or_else(|| {
            InterpError::value_error(format!("invalid integer literal: {value}"), line, col)
        }),

        GdExpr::FloatLiteral { value, .. } => {
            let s = value.replace('_', "");
            s.parse::<f64>().map(GdValue::Float).map_err(|_| {
                InterpError::value_error(format!("invalid float literal: {value}"), line, col)
            })
        }

        GdExpr::StringLiteral { value, .. } => Ok(GdValue::GdString(eval_string_literal(value))),

        GdExpr::StringName { value, .. } => {
            // value includes &"..." — strip &" prefix and " suffix
            let inner = if let Some(stripped) = value.strip_prefix("&\"") {
                stripped.strip_suffix('"').unwrap_or(stripped)
            } else if let Some(stripped) = value.strip_prefix("&'") {
                stripped.strip_suffix('\'').unwrap_or(stripped)
            } else {
                value
            };
            Ok(GdValue::StringName(eval_string_literal(&format!(
                "\"{inner}\""
            ))))
        }

        GdExpr::Bool { value, .. } => Ok(GdValue::Bool(*value)),
        GdExpr::Null { .. } => Ok(GdValue::Null),

        // ── Identifiers ────────────────────────────────────────────────
        GdExpr::Ident { name, .. } => eval_ident(name, line, col, env),

        // ── Collections ────────────────────────────────────────────────
        GdExpr::Array { elements, .. } => {
            let vals: InterpResult<Vec<GdValue>> =
                elements.iter().map(|e| eval_expr(e, env)).collect();
            Ok(GdValue::Array(vals?))
        }

        GdExpr::Dict { pairs, .. } => {
            let mut entries = Vec::with_capacity(pairs.len());
            for (k, v) in pairs {
                entries.push((eval_expr(k, env)?, eval_expr(v, env)?));
            }
            Ok(GdValue::Dictionary(entries))
        }

        // ── Calls ──────────────────────────────────────────────────────
        GdExpr::Call { callee, args, .. } => eval_call(callee, args, env, line, col),

        GdExpr::MethodCall {
            receiver,
            method,
            args,
            ..
        } => {
            let recv = eval_expr(receiver, env)?;
            let evaled: InterpResult<Vec<GdValue>> =
                args.iter().map(|a| eval_expr(a, env)).collect();
            builtins::call_method(&recv, method, &evaled?)
        }

        GdExpr::SuperCall { .. } => Err(InterpError::not_implemented("super calls", line, col)),

        // ── Access ─────────────────────────────────────────────────────
        GdExpr::PropertyAccess {
            receiver, property, ..
        } => {
            // Handle Color.RED, Vector2.ZERO, etc. (static constants)
            if let GdExpr::Ident { name, .. } = receiver.as_ref()
                && let Some(val) = eval_static_property(name, property)
            {
                return Ok(val);
            }
            let recv = eval_expr(receiver, env)?;
            builtins::get_property(&recv, property)
        }

        GdExpr::Subscript {
            receiver, index, ..
        } => {
            let recv = eval_expr(receiver, env)?;
            let idx = eval_expr(index, env)?;
            eval_subscript(&recv, &idx, line, col)
        }

        GdExpr::GetNode { .. } => Err(InterpError::not_implemented("get_node ($)", line, col)),

        // ── Operators ──────────────────────────────────────────────────
        GdExpr::BinOp {
            left, op, right, ..
        } => eval_binop(left, op, right, env, line, col),

        GdExpr::UnaryOp { op, operand, .. } => {
            let val = eval_expr(operand, env)?;
            eval_unaryop(op, &val, line, col)
        }

        GdExpr::Cast {
            expr, target_type, ..
        } => {
            let val = eval_expr(expr, env)?;
            eval_cast(&val, target_type, line, col)
        }

        GdExpr::Is {
            expr, type_name, ..
        } => {
            let val = eval_expr(expr, env)?;
            Ok(GdValue::Bool(val.type_name() == *type_name))
        }

        GdExpr::Ternary {
            true_val,
            condition,
            false_val,
            ..
        } => {
            let cond = eval_expr(condition, env)?;
            if cond.is_truthy() {
                eval_expr(true_val, env)
            } else {
                eval_expr(false_val, env)
            }
        }

        // ── Not yet implemented ────────────────────────────────────────
        GdExpr::Await { .. } => Err(InterpError::not_implemented("await", line, col)),
        GdExpr::Lambda { .. } => Err(InterpError::not_implemented("lambda", line, col)),
        GdExpr::Preload { .. } => Err(InterpError::not_implemented("preload", line, col)),
        GdExpr::Invalid { .. } => Err(InterpError::value_error("invalid expression", line, col)),
    }
}

fn eval_ident(name: &str, line: usize, col: usize, env: &Environment) -> InterpResult<GdValue> {
    match name {
        "PI" => Ok(GdValue::Float(std::f64::consts::PI)),
        "TAU" => Ok(GdValue::Float(std::f64::consts::TAU)),
        "INF" => Ok(GdValue::Float(f64::INFINITY)),
        "NAN" => Ok(GdValue::Float(f64::NAN)),
        _ => env.get(name).cloned().ok_or_else(|| {
            InterpError::name_error(format!("undefined identifier: {name}"), line, col)
        }),
    }
}

#[allow(clippy::too_many_lines)]
fn eval_call(
    callee: &GdExpr<'_>,
    args: &[GdExpr<'_>],
    env: &mut Environment,
    line: usize,
    col: usize,
) -> InterpResult<GdValue> {
    // Evaluate arguments first
    let evaled: InterpResult<Vec<GdValue>> = args.iter().map(|a| eval_expr(a, env)).collect();
    let evaled = evaled?;

    // Check if callee is a simple identifier (constructor or builtin)
    if let GdExpr::Ident { name, .. } = callee {
        match *name {
            "Vector2" => {
                expect_argc("Vector2", &evaled, 2, line, col)?;
                let x = to_float(&evaled[0], line, col)?;
                let y = to_float(&evaled[1], line, col)?;
                return Ok(GdValue::Vector2(x, y));
            }
            "Vector2i" => {
                expect_argc("Vector2i", &evaled, 2, line, col)?;
                let x = to_int(&evaled[0], line, col)?;
                let y = to_int(&evaled[1], line, col)?;
                return Ok(GdValue::Vector2i(x, y));
            }
            "Vector3" => {
                expect_argc("Vector3", &evaled, 3, line, col)?;
                let x = to_float(&evaled[0], line, col)?;
                let y = to_float(&evaled[1], line, col)?;
                let z = to_float(&evaled[2], line, col)?;
                return Ok(GdValue::Vector3(x, y, z));
            }
            "Vector3i" => {
                expect_argc("Vector3i", &evaled, 3, line, col)?;
                let x = to_int(&evaled[0], line, col)?;
                let y = to_int(&evaled[1], line, col)?;
                let z = to_int(&evaled[2], line, col)?;
                return Ok(GdValue::Vector3i(x, y, z));
            }
            "Vector4" => {
                expect_argc("Vector4", &evaled, 4, line, col)?;
                let x = to_float(&evaled[0], line, col)?;
                let y = to_float(&evaled[1], line, col)?;
                let z = to_float(&evaled[2], line, col)?;
                let w = to_float(&evaled[3], line, col)?;
                return Ok(GdValue::Vector4(x, y, z, w));
            }
            "Color" => {
                return eval_color_constructor(&evaled, line, col);
            }
            "Rect2" => {
                expect_argc("Rect2", &evaled, 4, line, col)?;
                let x = to_float(&evaled[0], line, col)?;
                let y = to_float(&evaled[1], line, col)?;
                let w = to_float(&evaled[2], line, col)?;
                let h = to_float(&evaled[3], line, col)?;
                return Ok(GdValue::Rect2(x, y, w, h));
            }
            "Array" => return Ok(GdValue::Array(Vec::new())),
            "Dictionary" => return Ok(GdValue::Dictionary(Vec::new())),
            "NodePath" => {
                expect_argc("NodePath", &evaled, 1, line, col)?;
                return match &evaled[0] {
                    GdValue::GdString(s) => Ok(GdValue::NodePath(s.clone())),
                    _ => Err(InterpError::type_error(
                        "NodePath() expects a String",
                        line,
                        col,
                    )),
                };
            }
            // Try as builtin utility function
            _ => return builtins::call_builtin(name, &evaled, env),
        }
    }

    Err(InterpError::not_implemented(
        "non-identifier function calls",
        line,
        col,
    ))
}

fn eval_color_constructor(args: &[GdValue], line: usize, col: usize) -> InterpResult<GdValue> {
    match args.len() {
        3 => {
            let r = to_float(&args[0], line, col)?;
            let g = to_float(&args[1], line, col)?;
            let b = to_float(&args[2], line, col)?;
            Ok(GdValue::Color(r, g, b, 1.0))
        }
        4 => {
            let r = to_float(&args[0], line, col)?;
            let g = to_float(&args[1], line, col)?;
            let b = to_float(&args[2], line, col)?;
            let a = to_float(&args[3], line, col)?;
            Ok(GdValue::Color(r, g, b, a))
        }
        _ => Err(InterpError::argument_error(
            format!("Color() takes 3 or 4 arguments, got {}", args.len()),
            line,
            col,
        )),
    }
}

/// Resolve static class properties like `Color.RED`, `Vector2.ZERO`, etc.
#[must_use]
fn eval_static_property(class_name: &str, property: &str) -> Option<GdValue> {
    match (class_name, property) {
        // Color constants
        ("Color", "RED") => Some(GdValue::Color(1.0, 0.0, 0.0, 1.0)),
        ("Color", "GREEN") => Some(GdValue::Color(0.0, 1.0, 0.0, 1.0)),
        ("Color", "BLUE") => Some(GdValue::Color(0.0, 0.0, 1.0, 1.0)),
        ("Color", "WHITE") => Some(GdValue::Color(1.0, 1.0, 1.0, 1.0)),
        ("Color", "BLACK") => Some(GdValue::Color(0.0, 0.0, 0.0, 1.0)),
        ("Color", "TRANSPARENT") => Some(GdValue::Color(0.0, 0.0, 0.0, 0.0)),
        ("Color", "YELLOW") => Some(GdValue::Color(1.0, 1.0, 0.0, 1.0)),
        ("Color", "CYAN") => Some(GdValue::Color(0.0, 1.0, 1.0, 1.0)),
        ("Color", "MAGENTA") => Some(GdValue::Color(1.0, 0.0, 1.0, 1.0)),
        ("Color", "ORANGE") => Some(GdValue::Color(1.0, 0.647_058_8, 0.0, 1.0)),

        // Vector2 constants
        ("Vector2", "ZERO") => Some(GdValue::Vector2(0.0, 0.0)),
        ("Vector2", "ONE") => Some(GdValue::Vector2(1.0, 1.0)),
        ("Vector2", "UP") => Some(GdValue::Vector2(0.0, -1.0)),
        ("Vector2", "DOWN") => Some(GdValue::Vector2(0.0, 1.0)),
        ("Vector2", "LEFT") => Some(GdValue::Vector2(-1.0, 0.0)),
        ("Vector2", "RIGHT") => Some(GdValue::Vector2(1.0, 0.0)),
        ("Vector2", "INF") => Some(GdValue::Vector2(f64::INFINITY, f64::INFINITY)),

        // Vector2i constants
        ("Vector2i", "ZERO") => Some(GdValue::Vector2i(0, 0)),
        ("Vector2i", "ONE") => Some(GdValue::Vector2i(1, 1)),
        ("Vector2i", "UP") => Some(GdValue::Vector2i(0, -1)),
        ("Vector2i", "DOWN") => Some(GdValue::Vector2i(0, 1)),
        ("Vector2i", "LEFT") => Some(GdValue::Vector2i(-1, 0)),
        ("Vector2i", "RIGHT") => Some(GdValue::Vector2i(1, 0)),

        // Vector3 constants
        ("Vector3", "ZERO") => Some(GdValue::Vector3(0.0, 0.0, 0.0)),
        ("Vector3", "ONE") => Some(GdValue::Vector3(1.0, 1.0, 1.0)),
        ("Vector3", "UP") => Some(GdValue::Vector3(0.0, 1.0, 0.0)),
        ("Vector3", "DOWN") => Some(GdValue::Vector3(0.0, -1.0, 0.0)),
        ("Vector3", "LEFT") => Some(GdValue::Vector3(-1.0, 0.0, 0.0)),
        ("Vector3", "RIGHT") => Some(GdValue::Vector3(1.0, 0.0, 0.0)),
        ("Vector3", "FORWARD") => Some(GdValue::Vector3(0.0, 0.0, -1.0)),
        ("Vector3", "BACK") => Some(GdValue::Vector3(0.0, 0.0, 1.0)),

        // Vector3i constants
        ("Vector3i", "ZERO") => Some(GdValue::Vector3i(0, 0, 0)),
        ("Vector3i", "ONE") => Some(GdValue::Vector3i(1, 1, 1)),

        _ => None,
    }
}

#[allow(clippy::too_many_lines)]
fn eval_binop(
    left: &GdExpr<'_>,
    op: &str,
    right: &GdExpr<'_>,
    env: &mut Environment,
    line: usize,
    col: usize,
) -> InterpResult<GdValue> {
    // Short-circuit for `and` / `or`
    if op == "and" {
        let l = eval_expr(left, env)?;
        if !l.is_truthy() {
            return Ok(l);
        }
        return eval_expr(right, env);
    }
    if op == "or" {
        let l = eval_expr(left, env)?;
        if l.is_truthy() {
            return Ok(l);
        }
        return eval_expr(right, env);
    }

    let l = eval_expr(left, env)?;
    let r = eval_expr(right, env)?;

    match op {
        // ── Arithmetic ─────────────────────────────────────────────
        "+" => eval_add(&l, &r, line, col),
        "-" => eval_arith(&l, &r, line, col, |a, b| a - b, |a, b| a - b),
        "*" => eval_arith(&l, &r, line, col, |a, b| a * b, |a, b| a * b),
        "/" => eval_div(&l, &r, line, col),
        "%" => eval_mod(&l, &r, line, col),
        "**" => eval_pow(&l, &r, line, col),

        // ── Comparison ─────────────────────────────────────────────
        "==" => Ok(GdValue::Bool(l == r)),
        "!=" => Ok(GdValue::Bool(l != r)),
        "<" => eval_cmp(&l, &r, line, col, std::cmp::Ordering::is_lt),
        ">" => eval_cmp(&l, &r, line, col, std::cmp::Ordering::is_gt),
        "<=" => eval_cmp(&l, &r, line, col, std::cmp::Ordering::is_le),
        ">=" => eval_cmp(&l, &r, line, col, std::cmp::Ordering::is_ge),

        // ── Containment ────────────────────────────────────────────
        "in" => eval_in(&l, &r, line, col),

        // ── Bitwise ────────────────────────────────────────────────
        "&" => match (&l, &r) {
            (GdValue::Int(a), GdValue::Int(b)) => Ok(GdValue::Int(a & b)),
            _ => Err(InterpError::type_error(
                format!(
                    "'&' requires int operands, got {} and {}",
                    l.type_name(),
                    r.type_name()
                ),
                line,
                col,
            )),
        },
        "|" => match (&l, &r) {
            (GdValue::Int(a), GdValue::Int(b)) => Ok(GdValue::Int(a | b)),
            _ => Err(InterpError::type_error(
                format!(
                    "'|' requires int operands, got {} and {}",
                    l.type_name(),
                    r.type_name()
                ),
                line,
                col,
            )),
        },
        "^" => match (&l, &r) {
            (GdValue::Int(a), GdValue::Int(b)) => Ok(GdValue::Int(a ^ b)),
            _ => Err(InterpError::type_error(
                format!(
                    "'^' requires int operands, got {} and {}",
                    l.type_name(),
                    r.type_name()
                ),
                line,
                col,
            )),
        },
        "<<" => match (&l, &r) {
            (GdValue::Int(a), GdValue::Int(b)) => Ok(GdValue::Int(a << b)),
            _ => Err(InterpError::type_error(
                format!(
                    "'<<' requires int operands, got {} and {}",
                    l.type_name(),
                    r.type_name()
                ),
                line,
                col,
            )),
        },
        ">>" => match (&l, &r) {
            (GdValue::Int(a), GdValue::Int(b)) => Ok(GdValue::Int(a >> b)),
            _ => Err(InterpError::type_error(
                format!(
                    "'>>' requires int operands, got {} and {}",
                    l.type_name(),
                    r.type_name()
                ),
                line,
                col,
            )),
        },

        _ => Err(InterpError::not_implemented(
            &format!("operator '{op}'"),
            line,
            col,
        )),
    }
}

fn eval_add(l: &GdValue, r: &GdValue, line: usize, col: usize) -> InterpResult<GdValue> {
    match (l, r) {
        (GdValue::Int(a), GdValue::Int(b)) => Ok(GdValue::Int(a.wrapping_add(*b))),
        (GdValue::Float(a), GdValue::Float(b)) => Ok(GdValue::Float(a + b)),
        (GdValue::Int(a), GdValue::Float(b)) => Ok(GdValue::Float(*a as f64 + b)),
        (GdValue::Float(a), GdValue::Int(b)) => Ok(GdValue::Float(a + *b as f64)),
        (GdValue::GdString(a), GdValue::GdString(b)) => Ok(GdValue::GdString(format!("{a}{b}"))),
        (GdValue::Array(a), GdValue::Array(b)) => {
            let mut combined = a.clone();
            combined.extend(b.iter().cloned());
            Ok(GdValue::Array(combined))
        }
        (GdValue::Vector2(x1, y1), GdValue::Vector2(x2, y2)) => {
            Ok(GdValue::Vector2(x1 + x2, y1 + y2))
        }
        (GdValue::Vector3(x1, y1, z1), GdValue::Vector3(x2, y2, z2)) => {
            Ok(GdValue::Vector3(x1 + x2, y1 + y2, z1 + z2))
        }
        (GdValue::Vector2i(x1, y1), GdValue::Vector2i(x2, y2)) => {
            Ok(GdValue::Vector2i(x1 + x2, y1 + y2))
        }
        (GdValue::Vector3i(x1, y1, z1), GdValue::Vector3i(x2, y2, z2)) => {
            Ok(GdValue::Vector3i(x1 + x2, y1 + y2, z1 + z2))
        }
        _ => Err(InterpError::type_error(
            format!(
                "'+' not supported between {} and {}",
                l.type_name(),
                r.type_name()
            ),
            line,
            col,
        )),
    }
}

fn eval_arith(
    l: &GdValue,
    r: &GdValue,
    line: usize,
    col: usize,
    int_op: fn(i64, i64) -> i64,
    float_op: fn(f64, f64) -> f64,
) -> InterpResult<GdValue> {
    match (l, r) {
        (GdValue::Int(a), GdValue::Int(b)) => Ok(GdValue::Int(int_op(*a, *b))),
        (GdValue::Float(a), GdValue::Float(b)) => Ok(GdValue::Float(float_op(*a, *b))),
        (GdValue::Int(a), GdValue::Float(b)) => Ok(GdValue::Float(float_op(*a as f64, *b))),
        (GdValue::Float(a), GdValue::Int(b)) => Ok(GdValue::Float(float_op(*a, *b as f64))),
        (GdValue::Vector2(x1, y1), GdValue::Vector2(x2, y2)) => {
            Ok(GdValue::Vector2(float_op(*x1, *x2), float_op(*y1, *y2)))
        }
        (GdValue::Vector3(x1, y1, z1), GdValue::Vector3(x2, y2, z2)) => Ok(GdValue::Vector3(
            float_op(*x1, *x2),
            float_op(*y1, *y2),
            float_op(*z1, *z2),
        )),
        _ => Err(InterpError::type_error(
            format!(
                "arithmetic not supported between {} and {}",
                l.type_name(),
                r.type_name()
            ),
            line,
            col,
        )),
    }
}

fn eval_div(l: &GdValue, r: &GdValue, line: usize, col: usize) -> InterpResult<GdValue> {
    match (l, r) {
        (GdValue::Int(_), GdValue::Int(0)) => Err(InterpError::division_by_zero(line, col)),
        (GdValue::Int(a), GdValue::Int(b)) => Ok(GdValue::Int(a / b)),
        (GdValue::Float(a), GdValue::Float(b)) => Ok(GdValue::Float(a / b)),
        (GdValue::Int(a), GdValue::Float(b)) => Ok(GdValue::Float(*a as f64 / b)),
        (GdValue::Float(a), GdValue::Int(b)) => Ok(GdValue::Float(a / *b as f64)),
        _ => Err(InterpError::type_error(
            format!(
                "'/' not supported between {} and {}",
                l.type_name(),
                r.type_name()
            ),
            line,
            col,
        )),
    }
}

fn eval_mod(l: &GdValue, r: &GdValue, line: usize, col: usize) -> InterpResult<GdValue> {
    match (l, r) {
        (GdValue::Int(_), GdValue::Int(0)) => Err(InterpError::division_by_zero(line, col)),
        (GdValue::Int(a), GdValue::Int(b)) => Ok(GdValue::Int(a % b)),
        (GdValue::Float(a), GdValue::Float(b)) => Ok(GdValue::Float(a % b)),
        (GdValue::Int(a), GdValue::Float(b)) => Ok(GdValue::Float(*a as f64 % b)),
        (GdValue::Float(a), GdValue::Int(b)) => Ok(GdValue::Float(a % *b as f64)),
        // String % formatting
        (GdValue::GdString(fmt), val) => Ok(eval_string_format(fmt, val)),
        _ => Err(InterpError::type_error(
            format!(
                "'%' not supported between {} and {}",
                l.type_name(),
                r.type_name()
            ),
            line,
            col,
        )),
    }
}

fn eval_string_format(fmt: &str, val: &GdValue) -> GdValue {
    // Simple GDScript string formatting: "text %s more" % value
    // For arrays, each %s/%d consumes the next element
    let values: Vec<&GdValue> = match val {
        GdValue::Array(arr) => arr.iter().collect(),
        other => vec![other],
    };

    let mut result = String::new();
    let mut val_idx = 0;
    let mut chars = fmt.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '%' {
            match chars.peek() {
                Some('s') => {
                    chars.next();
                    if val_idx < values.len() {
                        result.push_str(&values[val_idx].to_string());
                        val_idx += 1;
                    }
                }
                Some('d') => {
                    chars.next();
                    if val_idx < values.len() {
                        match values[val_idx] {
                            GdValue::Int(n) => result.push_str(&n.to_string()),
                            GdValue::Float(f) => result.push_str(&(*f as i64).to_string()),
                            other => result.push_str(&other.to_string()),
                        }
                        val_idx += 1;
                    }
                }
                Some('f') => {
                    chars.next();
                    if val_idx < values.len() {
                        match values[val_idx] {
                            GdValue::Int(n) => {
                                let _ = write!(result, "{:.6}", *n as f64);
                            }
                            GdValue::Float(f) => {
                                let _ = write!(result, "{f:.6}");
                            }
                            other => result.push_str(&other.to_string()),
                        }
                        val_idx += 1;
                    }
                }
                Some('%') => {
                    chars.next();
                    result.push('%');
                }
                _ => result.push('%'),
            }
        } else {
            result.push(c);
        }
    }

    GdValue::GdString(result)
}

fn eval_pow(l: &GdValue, r: &GdValue, line: usize, col: usize) -> InterpResult<GdValue> {
    match (l, r) {
        (GdValue::Int(base), GdValue::Int(exp)) => {
            if *exp >= 0 {
                Ok(GdValue::Int(base.wrapping_pow(*exp as u32)))
            } else {
                Ok(GdValue::Float((*base as f64).powf(*exp as f64)))
            }
        }
        (GdValue::Float(a), GdValue::Float(b)) => Ok(GdValue::Float(a.powf(*b))),
        (GdValue::Int(a), GdValue::Float(b)) => Ok(GdValue::Float((*a as f64).powf(*b))),
        (GdValue::Float(a), GdValue::Int(b)) => Ok(GdValue::Float(a.powf(*b as f64))),
        _ => Err(InterpError::type_error(
            format!(
                "'**' not supported between {} and {}",
                l.type_name(),
                r.type_name()
            ),
            line,
            col,
        )),
    }
}

fn eval_cmp(
    l: &GdValue,
    r: &GdValue,
    line: usize,
    col: usize,
    check: fn(std::cmp::Ordering) -> bool,
) -> InterpResult<GdValue> {
    let ord = match (l, r) {
        (GdValue::Int(a), GdValue::Int(b)) => a.cmp(b),
        (GdValue::Float(a), GdValue::Float(b)) => {
            a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal)
        }
        (GdValue::Int(a), GdValue::Float(b)) => (*a as f64)
            .partial_cmp(b)
            .unwrap_or(std::cmp::Ordering::Equal),
        (GdValue::Float(a), GdValue::Int(b)) => a
            .partial_cmp(&(*b as f64))
            .unwrap_or(std::cmp::Ordering::Equal),
        (GdValue::GdString(a), GdValue::GdString(b)) => a.cmp(b),
        _ => {
            return Err(InterpError::type_error(
                format!(
                    "comparison not supported between {} and {}",
                    l.type_name(),
                    r.type_name()
                ),
                line,
                col,
            ));
        }
    };
    Ok(GdValue::Bool(check(ord)))
}

fn eval_in(l: &GdValue, r: &GdValue, line: usize, col: usize) -> InterpResult<GdValue> {
    match r {
        GdValue::Array(arr) => Ok(GdValue::Bool(arr.contains(l))),
        GdValue::Dictionary(pairs) => Ok(GdValue::Bool(pairs.iter().any(|(k, _)| k == l))),
        GdValue::GdString(s) => match l {
            GdValue::GdString(sub) => Ok(GdValue::Bool(s.contains(sub.as_str()))),
            _ => Err(InterpError::type_error(
                format!(
                    "'in' with String requires String operand, got {}",
                    l.type_name()
                ),
                line,
                col,
            )),
        },
        _ => Err(InterpError::type_error(
            format!("'in' not supported for {}", r.type_name()),
            line,
            col,
        )),
    }
}

fn eval_unaryop(op: &str, val: &GdValue, line: usize, col: usize) -> InterpResult<GdValue> {
    match op {
        "-" => match val {
            GdValue::Int(n) => Ok(GdValue::Int(-n)),
            GdValue::Float(f) => Ok(GdValue::Float(-f)),
            GdValue::Vector2(x, y) => Ok(GdValue::Vector2(-x, -y)),
            GdValue::Vector3(x, y, z) => Ok(GdValue::Vector3(-x, -y, -z)),
            _ => Err(InterpError::type_error(
                format!("unary '-' not supported for {}", val.type_name()),
                line,
                col,
            )),
        },
        "not" | "!" => Ok(GdValue::Bool(!val.is_truthy())),
        "~" => match val {
            GdValue::Int(n) => Ok(GdValue::Int(!n)),
            _ => Err(InterpError::type_error(
                format!("'~' requires int, got {}", val.type_name()),
                line,
                col,
            )),
        },
        "+" => match val {
            GdValue::Int(_) | GdValue::Float(_) => Ok(val.clone()),
            _ => Err(InterpError::type_error(
                format!("unary '+' not supported for {}", val.type_name()),
                line,
                col,
            )),
        },
        _ => Err(InterpError::not_implemented(
            &format!("unary operator '{op}'"),
            line,
            col,
        )),
    }
}

fn eval_cast(val: &GdValue, target: &str, line: usize, col: usize) -> InterpResult<GdValue> {
    match target {
        "int" => Ok(GdValue::Int(match val {
            GdValue::Int(n) => *n,
            GdValue::Float(f) => *f as i64,
            GdValue::Bool(b) => i64::from(*b),
            GdValue::GdString(s) => s.parse().unwrap_or(0),
            _ => {
                return Err(InterpError::type_error(
                    format!("cannot cast {} to int", val.type_name()),
                    line,
                    col,
                ));
            }
        })),
        "float" => Ok(GdValue::Float(match val {
            GdValue::Float(f) => *f,
            GdValue::Int(n) => *n as f64,
            GdValue::Bool(b) => {
                if *b {
                    1.0
                } else {
                    0.0
                }
            }
            GdValue::GdString(s) => s.parse().unwrap_or(0.0),
            _ => {
                return Err(InterpError::type_error(
                    format!("cannot cast {} to float", val.type_name()),
                    line,
                    col,
                ));
            }
        })),
        "String" => Ok(GdValue::GdString(val.to_string())),
        "bool" => Ok(GdValue::Bool(val.is_truthy())),
        _ => {
            // For unknown types, just return the value as-is (permissive for now)
            Ok(val.clone())
        }
    }
}

fn eval_subscript(recv: &GdValue, idx: &GdValue, line: usize, col: usize) -> InterpResult<GdValue> {
    match recv {
        GdValue::Array(arr) => {
            let GdValue::Int(i) = idx else {
                return Err(InterpError::type_error(
                    "array index must be int",
                    line,
                    col,
                ));
            };
            let index = if *i < 0 {
                (arr.len() as i64 + i) as usize
            } else {
                *i as usize
            };
            arr.get(index)
                .cloned()
                .ok_or_else(|| InterpError::index_out_of_bounds(*i, arr.len(), line, col))
        }
        GdValue::Dictionary(pairs) => {
            for (k, v) in pairs {
                if k == idx {
                    return Ok(v.clone());
                }
            }
            Err(InterpError::key_error(&idx.to_string(), line, col))
        }
        GdValue::GdString(s) => {
            let GdValue::Int(i) = idx else {
                return Err(InterpError::type_error(
                    "string index must be int",
                    line,
                    col,
                ));
            };
            let index = if *i < 0 {
                (s.len() as i64 + i) as usize
            } else {
                *i as usize
            };
            s.chars()
                .nth(index)
                .map(|c| GdValue::GdString(c.to_string()))
                .ok_or_else(|| InterpError::index_out_of_bounds(*i, s.len(), line, col))
        }
        _ => Err(InterpError::type_error(
            format!("'[]' not supported on {}", recv.type_name()),
            line,
            col,
        )),
    }
}

/// Convert a `GdValue` to `f64` (for constructors).
fn to_float(val: &GdValue, line: usize, col: usize) -> InterpResult<f64> {
    match val {
        GdValue::Float(f) => Ok(*f),
        GdValue::Int(n) => Ok(*n as f64),
        _ => Err(InterpError::type_error(
            format!("expected number, got {}", val.type_name()),
            line,
            col,
        )),
    }
}

/// Convert a `GdValue` to `i64` (for constructors).
fn to_int(val: &GdValue, line: usize, col: usize) -> InterpResult<i64> {
    match val {
        GdValue::Int(n) => Ok(*n),
        GdValue::Float(f) => Ok(*f as i64),
        _ => Err(InterpError::type_error(
            format!("expected int, got {}", val.type_name()),
            line,
            col,
        )),
    }
}

fn expect_argc(
    name: &str,
    args: &[GdValue],
    expected: usize,
    line: usize,
    col: usize,
) -> InterpResult<()> {
    if args.len() != expected {
        return Err(InterpError::argument_error(
            format!("{name}() takes {expected} arguments, got {}", args.len()),
            line,
            col,
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eval(source: &str) -> InterpResult<GdValue> {
        // Wrap expression in a function to parse it
        let wrapped = format!("var _x = {source}\n");
        let tree = gd_core::parser::parse(&wrapped).expect("parse failed");
        let file = gd_core::gd_ast::convert(&tree, &wrapped);

        // Find the var declaration and extract its initializer
        for decl in &file.declarations {
            if let gd_core::gd_ast::GdDecl::Var(v) = decl {
                if let Some(ref expr) = v.value {
                    let mut env = Environment::new();
                    return eval_expr(expr, &mut env);
                }
            }
        }
        panic!("could not find expression in parsed source");
    }

    #[test]
    fn test_int_literals() {
        assert_eq!(eval("42").unwrap(), GdValue::Int(42));
        assert_eq!(eval("0xFF").unwrap(), GdValue::Int(255));
        assert_eq!(eval("0b1010").unwrap(), GdValue::Int(10));
        assert_eq!(eval("1_000_000").unwrap(), GdValue::Int(1_000_000));
    }

    #[test]
    fn test_float_literals() {
        assert_eq!(eval("3.14").unwrap(), GdValue::Float(3.14));
        assert_eq!(eval("1e5").unwrap(), GdValue::Float(1e5));
    }

    #[test]
    fn test_string_literal() {
        assert_eq!(
            eval("\"hello world\"").unwrap(),
            GdValue::GdString("hello world".into())
        );
    }

    #[test]
    fn test_bool_null() {
        assert_eq!(eval("true").unwrap(), GdValue::Bool(true));
        assert_eq!(eval("false").unwrap(), GdValue::Bool(false));
        assert_eq!(eval("null").unwrap(), GdValue::Null);
    }

    #[test]
    fn test_arithmetic() {
        assert_eq!(eval("1 + 2").unwrap(), GdValue::Int(3));
        assert_eq!(eval("10 - 3").unwrap(), GdValue::Int(7));
        assert_eq!(eval("4 * 5").unwrap(), GdValue::Int(20));
        assert_eq!(eval("10 / 3").unwrap(), GdValue::Int(3));
        assert_eq!(eval("10.0 / 3.0").unwrap(), GdValue::Float(10.0 / 3.0));
        assert_eq!(eval("7 % 3").unwrap(), GdValue::Int(1));
        assert_eq!(eval("2 ** 10").unwrap(), GdValue::Int(1024));
    }

    #[test]
    fn test_string_concat() {
        assert_eq!(
            eval("\"hello\" + \" world\"").unwrap(),
            GdValue::GdString("hello world".into())
        );
    }

    #[test]
    fn test_comparison() {
        assert_eq!(eval("1 < 2").unwrap(), GdValue::Bool(true));
        assert_eq!(eval("2 > 3").unwrap(), GdValue::Bool(false));
        assert_eq!(eval("1 == 1").unwrap(), GdValue::Bool(true));
        assert_eq!(eval("1 != 2").unwrap(), GdValue::Bool(true));
    }

    #[test]
    fn test_boolean_ops() {
        assert_eq!(eval("true and false").unwrap(), GdValue::Bool(false));
        assert_eq!(eval("true or false").unwrap(), GdValue::Bool(true));
        assert_eq!(eval("not true").unwrap(), GdValue::Bool(false));
    }

    #[test]
    fn test_array() {
        assert_eq!(
            eval("[1, 2, 3]").unwrap(),
            GdValue::Array(vec![GdValue::Int(1), GdValue::Int(2), GdValue::Int(3)])
        );
    }

    #[test]
    fn test_dict() {
        let result = eval("{\"a\": 1}").unwrap();
        assert_eq!(
            result,
            GdValue::Dictionary(vec![(GdValue::GdString("a".into()), GdValue::Int(1))])
        );
    }

    #[test]
    fn test_vector2_constructor() {
        assert_eq!(
            eval("Vector2(1.0, 2.0)").unwrap(),
            GdValue::Vector2(1.0, 2.0)
        );
    }

    #[test]
    fn test_ternary() {
        assert_eq!(eval("1 if true else 2").unwrap(), GdValue::Int(1));
        assert_eq!(eval("1 if false else 2").unwrap(), GdValue::Int(2));
    }

    #[test]
    fn test_unary() {
        assert_eq!(eval("-5").unwrap(), GdValue::Int(-5));
        assert_eq!(eval("-3.14").unwrap(), GdValue::Float(-3.14));
    }

    #[test]
    fn test_in_operator() {
        assert_eq!(eval("1 in [1, 2, 3]").unwrap(), GdValue::Bool(true));
        assert_eq!(eval("4 in [1, 2, 3]").unwrap(), GdValue::Bool(false));
    }

    #[test]
    fn test_constants() {
        assert_eq!(eval("PI").unwrap(), GdValue::Float(std::f64::consts::PI));
        assert_eq!(eval("TAU").unwrap(), GdValue::Float(std::f64::consts::TAU));
    }

    #[test]
    fn test_static_properties() {
        assert_eq!(
            eval("Color.RED").unwrap(),
            GdValue::Color(1.0, 0.0, 0.0, 1.0)
        );
        assert_eq!(eval("Vector2.ZERO").unwrap(), GdValue::Vector2(0.0, 0.0));
    }

    #[test]
    fn test_bitwise() {
        assert_eq!(eval("0xFF & 0x0F").unwrap(), GdValue::Int(0x0F));
        assert_eq!(eval("0xF0 | 0x0F").unwrap(), GdValue::Int(0xFF));
        assert_eq!(eval("1 << 4").unwrap(), GdValue::Int(16));
    }

    #[test]
    fn test_division_by_zero() {
        assert!(eval("1 / 0").is_err());
    }

    #[test]
    fn test_string_format() {
        assert_eq!(
            eval("\"hello %s\" % \"world\"").unwrap(),
            GdValue::GdString("hello world".into())
        );
    }
}
