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
        _ => math::call(name, args),
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
