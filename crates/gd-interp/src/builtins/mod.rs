mod array;
mod color;
mod dictionary;
mod math;
mod string;
mod vector2;
mod vector3;

use crate::env::Environment;
use crate::error::{InterpError, InterpResult};
use crate::value::GdValue;

#[allow(clippy::too_many_lines)]
pub fn call_builtin(name: &str, args: &[GdValue], env: &mut Environment) -> InterpResult<GdValue> {
    match name {
        "print" | "prints" => {
            let msg = args
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(" ");
            env.capture_output(msg);
            Ok(GdValue::Null)
        }
        "print_rich" => {
            let msg = args
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(" ");
            let stripped = strip_bbcode(&msg);
            env.capture_output(stripped);
            Ok(GdValue::Null)
        }
        "printt" => {
            let msg = args
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("\t");
            env.capture_output(msg);
            Ok(GdValue::Null)
        }
        "push_error" => {
            let msg = args
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(" ");
            env.capture_output(format!("ERROR: {msg}"));
            Ok(GdValue::Null)
        }
        "push_warning" => {
            let msg = args
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(" ");
            env.capture_output(format!("WARNING: {msg}"));
            Ok(GdValue::Null)
        }
        "printerr" => {
            let msg = args
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(" ");
            env.capture_output(format!("Error: {msg}"));
            Ok(GdValue::Null)
        }
        // Assertion builtins for native test runner
        "assert_true" => {
            if args.is_empty() {
                return Err(InterpError::argument_error(
                    "assert_true() requires at least 1 argument",
                    0,
                    0,
                ));
            }
            if !args[0].is_truthy() {
                let msg = args
                    .get(1)
                    .map_or("assert_true failed".to_string(), ToString::to_string);
                return Err(InterpError::assertion_failed(msg, 0, 0));
            }
            Ok(GdValue::Null)
        }
        "assert_false" => {
            if args.is_empty() {
                return Err(InterpError::argument_error(
                    "assert_false() requires at least 1 argument",
                    0,
                    0,
                ));
            }
            if args[0].is_truthy() {
                let msg = args
                    .get(1)
                    .map_or("assert_false failed".to_string(), ToString::to_string);
                return Err(InterpError::assertion_failed(msg, 0, 0));
            }
            Ok(GdValue::Null)
        }
        "assert_eq" => {
            if args.len() < 2 {
                return Err(InterpError::argument_error(
                    "assert_eq() requires at least 2 arguments",
                    0,
                    0,
                ));
            }
            if args[0] != args[1] {
                let msg = args.get(2).map_or_else(
                    || {
                        format!(
                            "assert_eq failed\n  left:  {}\n  right: {}",
                            args[0], args[1]
                        )
                    },
                    ToString::to_string,
                );
                return Err(InterpError::assertion_failed(msg, 0, 0));
            }
            Ok(GdValue::Null)
        }
        "assert_ne" => {
            if args.len() < 2 {
                return Err(InterpError::argument_error(
                    "assert_ne() requires at least 2 arguments",
                    0,
                    0,
                ));
            }
            if args[0] == args[1] {
                let msg = args.get(2).map_or_else(
                    || format!("assert_ne failed: both sides equal {}", args[0]),
                    ToString::to_string,
                );
                return Err(InterpError::assertion_failed(msg, 0, 0));
            }
            Ok(GdValue::Null)
        }
        "assert_gt" => {
            assert_cmp("assert_gt", args, |a, b| a > b, ">")?;
            Ok(GdValue::Null)
        }
        "assert_lt" => {
            assert_cmp("assert_lt", args, |a, b| a < b, "<")?;
            Ok(GdValue::Null)
        }
        "assert_null" => {
            if args.is_empty() {
                return Err(InterpError::argument_error(
                    "assert_null() requires at least 1 argument",
                    0,
                    0,
                ));
            }
            if args[0] != GdValue::Null {
                return Err(InterpError::assertion_failed(
                    format!("assert_null failed: got {}", args[0]),
                    0,
                    0,
                ));
            }
            Ok(GdValue::Null)
        }
        "assert_not_null" => {
            if args.is_empty() {
                return Err(InterpError::argument_error(
                    "assert_not_null() requires at least 1 argument",
                    0,
                    0,
                ));
            }
            if args[0] == GdValue::Null {
                return Err(InterpError::assertion_failed(
                    "assert_not_null failed: got null",
                    0,
                    0,
                ));
            }
            Ok(GdValue::Null)
        }
        _ => math::call(name, args),
    }
}

fn assert_cmp(
    name: &str,
    args: &[GdValue],
    check: fn(f64, f64) -> bool,
    op: &str,
) -> InterpResult<()> {
    if args.len() < 2 {
        return Err(InterpError::argument_error(
            format!("{name}() requires at least 2 arguments"),
            0,
            0,
        ));
    }
    let a = match &args[0] {
        GdValue::Int(n) => *n as f64,
        GdValue::Float(f) => *f,
        _ => {
            return Err(InterpError::type_error(
                format!("{name}() requires numeric arguments"),
                0,
                0,
            ));
        }
    };
    let b = match &args[1] {
        GdValue::Int(n) => *n as f64,
        GdValue::Float(f) => *f,
        _ => {
            return Err(InterpError::type_error(
                format!("{name}() requires numeric arguments"),
                0,
                0,
            ));
        }
    };
    if !check(a, b) {
        return Err(InterpError::assertion_failed(
            format!("{name} failed: {} {op} {} is false", args[0], args[1]),
            0,
            0,
        ));
    }
    Ok(())
}

/// Check if a method is mutating for the given receiver type.
#[must_use]
pub fn is_mutating_method(receiver: &GdValue, method: &str) -> bool {
    match receiver {
        GdValue::Array(_) => array::is_mutating(method),
        GdValue::Dictionary(_) => dictionary::is_mutating(method),
        _ => false,
    }
}

/// Call a mutating method on a value, modifying it in place.
pub fn call_method_mut(
    receiver: &mut GdValue,
    method: &str,
    args: &[GdValue],
) -> InterpResult<GdValue> {
    match receiver {
        GdValue::Array(_) => array::call_method_mut(receiver, method, args),
        GdValue::Dictionary(_) => dictionary::call_method_mut(receiver, method, args),
        _ => call_method(receiver, method, args),
    }
}

pub fn call_method(receiver: &GdValue, method: &str, args: &[GdValue]) -> InterpResult<GdValue> {
    match receiver {
        GdValue::GdString(_) | GdValue::StringName(_) => {
            string::call_method(receiver, method, args)
        }
        GdValue::Array(_) => array::call_method(receiver, method, args),
        GdValue::Dictionary(_) => dictionary::call_method(receiver, method, args),
        GdValue::Vector2(..) | GdValue::Vector2i(..) => {
            vector2::call_method(receiver, method, args)
        }
        GdValue::Vector3(..) | GdValue::Vector3i(..) => {
            vector3::call_method(receiver, method, args)
        }
        GdValue::Color(..) => color::call_method(receiver, method, args),
        _ => Err(InterpError::type_error(
            format!("no method '{method}' on {}", receiver.type_name()),
            0,
            0,
        )),
    }
}

pub fn get_property(receiver: &GdValue, property: &str) -> InterpResult<GdValue> {
    match receiver {
        GdValue::Vector2(..) | GdValue::Vector2i(..) => vector2::get_property(receiver, property),
        GdValue::Vector3(..) | GdValue::Vector3i(..) => vector3::get_property(receiver, property),
        GdValue::Color(..) => color::get_property(receiver, property),
        GdValue::Rect2(x, y, w, h) => match property {
            "position" => Ok(GdValue::Vector2(*x, *y)),
            "size" => Ok(GdValue::Vector2(*w, *h)),
            "end" => Ok(GdValue::Vector2(*x + *w, *y + *h)),
            _ => Err(InterpError::type_error(
                format!("no property '{property}' on Rect2"),
                0,
                0,
            )),
        },
        GdValue::GdString(s) | GdValue::StringName(s) => match property {
            "length" => Ok(GdValue::Int(s.len() as i64)),
            _ => Err(InterpError::type_error(
                format!("no property '{property}' on {}", receiver.type_name()),
                0,
                0,
            )),
        },
        GdValue::Array(items) => match property {
            "size" => Ok(GdValue::Int(items.len() as i64)),
            _ => Err(InterpError::type_error(
                format!("no property '{property}' on Array"),
                0,
                0,
            )),
        },
        GdValue::Dictionary(entries) => match property {
            "size" => Ok(GdValue::Int(entries.len() as i64)),
            _ => Err(InterpError::type_error(
                format!("no property '{property}' on Dictionary"),
                0,
                0,
            )),
        },
        _ => Err(InterpError::type_error(
            format!("no property '{property}' on {}", receiver.type_name()),
            0,
            0,
        )),
    }
}

fn strip_bbcode(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        if c == '[' {
            in_tag = true;
        } else if c == ']' && in_tag {
            in_tag = false;
        } else if !in_tag {
            result.push(c);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn print_joins_args() {
        let mut env = Environment::new();
        let args = vec![GdValue::GdString("hello".into()), GdValue::Int(42)];
        let result = call_builtin("print", &args, &mut env).unwrap();
        assert_eq!(result, GdValue::Null);
        assert_eq!(env.output(), &["hello 42"]);
    }

    #[test]
    fn push_error_prefix() {
        let mut env = Environment::new();
        call_builtin("push_error", &[GdValue::GdString("fail".into())], &mut env).unwrap();
        assert_eq!(env.output(), &["ERROR: fail"]);
    }

    #[test]
    fn push_warning_prefix() {
        let mut env = Environment::new();
        call_builtin(
            "push_warning",
            &[GdValue::GdString("warn".into())],
            &mut env,
        )
        .unwrap();
        assert_eq!(env.output(), &["WARNING: warn"]);
    }

    #[test]
    fn printerr_prefix() {
        let mut env = Environment::new();
        call_builtin("printerr", &[GdValue::GdString("oops".into())], &mut env).unwrap();
        assert_eq!(env.output(), &["Error: oops"]);
    }

    #[test]
    fn call_method_unknown_type() {
        let result = call_method(&GdValue::Null, "foo", &[]);
        assert!(result.is_err());
    }

    #[test]
    fn get_property_rect2() {
        let r = GdValue::Rect2(1.0, 2.0, 3.0, 4.0);
        assert_eq!(
            get_property(&r, "position").unwrap(),
            GdValue::Vector2(1.0, 2.0)
        );
        assert_eq!(
            get_property(&r, "size").unwrap(),
            GdValue::Vector2(3.0, 4.0)
        );
        assert_eq!(get_property(&r, "end").unwrap(), GdValue::Vector2(4.0, 6.0));
    }

    #[test]
    fn strip_bbcode_works() {
        assert_eq!(strip_bbcode("[b]bold[/b]"), "bold");
        assert_eq!(strip_bbcode("no tags"), "no tags");
    }

    #[test]
    fn print_rich_strips_bbcode() {
        let mut env = Environment::new();
        call_builtin(
            "print_rich",
            &[GdValue::GdString("[b]bold[/b] text".into())],
            &mut env,
        )
        .unwrap();
        assert_eq!(env.output(), &["bold text"]);
    }

    #[test]
    fn unknown_builtin_errors() {
        let mut env = Environment::new();
        let result = call_builtin("nonexistent_func", &[], &mut env);
        assert!(result.is_err());
    }
}
