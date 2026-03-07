use crate::error::{InterpError, InterpResult};
use crate::value::GdValue;

fn expect_argc(method: &str, args: &[GdValue], expected: usize) -> InterpResult<()> {
    if args.len() == expected {
        Ok(())
    } else {
        Err(InterpError::argument_error(
            format!(
                "String.{method}() takes {expected} argument(s), got {}",
                args.len()
            ),
            0,
            0,
        ))
    }
}

fn get_str(receiver: &GdValue) -> &str {
    match receiver {
        GdValue::GdString(s) | GdValue::StringName(s) => s,
        _ => unreachable!("string::call_method called with non-string"),
    }
}

#[allow(clippy::too_many_lines)]
pub fn call_method(receiver: &GdValue, method: &str, args: &[GdValue]) -> InterpResult<GdValue> {
    let s = get_str(receiver);

    match method {
        "length" => {
            expect_argc(method, args, 0)?;
            Ok(GdValue::Int(s.len() as i64))
        }
        "is_empty" => {
            expect_argc(method, args, 0)?;
            Ok(GdValue::Bool(s.is_empty()))
        }
        "contains" => {
            expect_argc(method, args, 1)?;
            let sub = get_string_arg(method, &args[0])?;
            Ok(GdValue::Bool(s.contains(sub)))
        }
        "begins_with" => {
            expect_argc(method, args, 1)?;
            let prefix = get_string_arg(method, &args[0])?;
            Ok(GdValue::Bool(s.starts_with(prefix)))
        }
        "ends_with" => {
            expect_argc(method, args, 1)?;
            let suffix = get_string_arg(method, &args[0])?;
            Ok(GdValue::Bool(s.ends_with(suffix)))
        }
        "find" => {
            expect_argc(method, args, 1)?;
            let sub = get_string_arg(method, &args[0])?;
            let idx = s.find(sub).map_or(-1, |i| i as i64);
            Ok(GdValue::Int(idx))
        }
        "rfind" => {
            expect_argc(method, args, 1)?;
            let sub = get_string_arg(method, &args[0])?;
            let idx = s.rfind(sub).map_or(-1, |i| i as i64);
            Ok(GdValue::Int(idx))
        }
        "to_lower" => {
            expect_argc(method, args, 0)?;
            Ok(GdValue::GdString(s.to_lowercase()))
        }
        "to_upper" => {
            expect_argc(method, args, 0)?;
            Ok(GdValue::GdString(s.to_uppercase()))
        }
        "strip_edges" => {
            expect_argc(method, args, 0)?;
            Ok(GdValue::GdString(s.trim().to_owned()))
        }
        "replace" => {
            expect_argc(method, args, 2)?;
            let what = get_string_arg(method, &args[0])?;
            let with = get_string_arg(method, &args[1])?;
            Ok(GdValue::GdString(s.replace(what, with)))
        }
        "split" => {
            expect_argc(method, args, 1)?;
            let delim = get_string_arg(method, &args[0])?;
            let parts: Vec<GdValue> = s
                .split(delim)
                .map(|p| GdValue::GdString(p.to_owned()))
                .collect();
            Ok(GdValue::Array(parts))
        }
        "join" => {
            expect_argc(method, args, 1)?;
            match &args[0] {
                GdValue::Array(items) => {
                    let joined = items
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(s);
                    Ok(GdValue::GdString(joined))
                }
                _ => Err(InterpError::type_error(
                    "String.join() requires an Array argument".to_owned(),
                    0,
                    0,
                )),
            }
        }
        "substr" => {
            if args.is_empty() || args.len() > 2 {
                return Err(InterpError::argument_error(
                    format!("String.substr() takes 1-2 arguments, got {}", args.len()),
                    0,
                    0,
                ));
            }
            let from = match &args[0] {
                GdValue::Int(n) => *n as usize,
                _ => {
                    return Err(InterpError::type_error(
                        "substr() from must be int".to_owned(),
                        0,
                        0,
                    ));
                }
            };
            let len = if args.len() == 2 {
                match &args[1] {
                    GdValue::Int(n) => *n as usize,
                    _ => {
                        return Err(InterpError::type_error(
                            "substr() length must be int".to_owned(),
                            0,
                            0,
                        ));
                    }
                }
            } else {
                s.len().saturating_sub(from)
            };
            let end = (from + len).min(s.len());
            let from = from.min(s.len());
            Ok(GdValue::GdString(s[from..end].to_owned()))
        }
        "left" => {
            expect_argc(method, args, 1)?;
            let n = get_int_arg(method, &args[0])?;
            let n = if n < 0 {
                s.len().saturating_sub((-n) as usize)
            } else {
                (n as usize).min(s.len())
            };
            Ok(GdValue::GdString(s[..n].to_owned()))
        }
        "right" => {
            expect_argc(method, args, 1)?;
            let n = get_int_arg(method, &args[0])?;
            let n = if n < 0 {
                s.len().saturating_sub((-n) as usize)
            } else {
                (n as usize).min(s.len())
            };
            Ok(GdValue::GdString(s[s.len() - n..].to_owned()))
        }
        "repeat" => {
            expect_argc(method, args, 1)?;
            let n = get_int_arg(method, &args[0])?;
            Ok(GdValue::GdString(s.repeat(n.max(0) as usize)))
        }
        "to_int" => {
            expect_argc(method, args, 0)?;
            Ok(GdValue::Int(s.trim().parse().unwrap_or(0)))
        }
        "to_float" => {
            expect_argc(method, args, 0)?;
            Ok(GdValue::Float(s.trim().parse().unwrap_or(0.0)))
        }
        "is_valid_int" => {
            expect_argc(method, args, 0)?;
            Ok(GdValue::Bool(s.trim().parse::<i64>().is_ok()))
        }
        "is_valid_float" => {
            expect_argc(method, args, 0)?;
            Ok(GdValue::Bool(s.trim().parse::<f64>().is_ok()))
        }
        "capitalize" => {
            expect_argc(method, args, 0)?;
            let capitalized = s
                .split_whitespace()
                .map(|w| {
                    let mut chars = w.chars();
                    match chars.next() {
                        Some(c) => {
                            let upper: String = c.to_uppercase().collect();
                            format!("{upper}{}", chars.as_str().to_lowercase())
                        }
                        None => String::new(),
                    }
                })
                .collect::<Vec<_>>()
                .join(" ");
            Ok(GdValue::GdString(capitalized))
        }
        "to_snake_case" => {
            expect_argc(method, args, 0)?;
            let mut result = String::new();
            for (i, c) in s.chars().enumerate() {
                if c.is_uppercase() && i > 0 {
                    result.push('_');
                }
                result.push(c.to_ascii_lowercase());
            }
            Ok(GdValue::GdString(result))
        }
        "dedent" => {
            expect_argc(method, args, 0)?;
            let lines: Vec<&str> = s.lines().collect();
            let min_indent = lines
                .iter()
                .filter(|l| !l.trim().is_empty())
                .map(|l| l.len() - l.trim_start().len())
                .min()
                .unwrap_or(0);
            let dedented = lines
                .iter()
                .map(|l| {
                    if l.len() >= min_indent {
                        &l[min_indent..]
                    } else {
                        l.trim_start()
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");
            Ok(GdValue::GdString(dedented))
        }
        "indent" => {
            expect_argc(method, args, 1)?;
            let prefix = get_string_arg(method, &args[0])?;
            let indented = s
                .lines()
                .map(|l| format!("{prefix}{l}"))
                .collect::<Vec<_>>()
                .join("\n");
            Ok(GdValue::GdString(indented))
        }
        "pad_zeros" => {
            expect_argc(method, args, 1)?;
            let n = get_int_arg(method, &args[0])? as usize;
            let stripped = s.trim_start_matches('-');
            let is_negative = s.starts_with('-');
            let padded = if stripped.len() < n {
                let zeros = "0".repeat(n - stripped.len());
                if is_negative {
                    format!("-{zeros}{stripped}")
                } else {
                    format!("{zeros}{stripped}")
                }
            } else {
                s.to_owned()
            };
            Ok(GdValue::GdString(padded))
        }
        "lpad" => {
            if args.is_empty() || args.len() > 2 {
                return Err(InterpError::argument_error(
                    format!("String.lpad() takes 1-2 arguments, got {}", args.len()),
                    0,
                    0,
                ));
            }
            let n = get_int_arg(method, &args[0])? as usize;
            let pad_char = if args.len() == 2 {
                get_string_arg(method, &args[1])?
            } else {
                " "
            };
            let pad_char = pad_char.chars().next().unwrap_or(' ');
            if s.len() < n {
                let padding: String = std::iter::repeat_n(pad_char, n - s.len()).collect();
                Ok(GdValue::GdString(format!("{padding}{s}")))
            } else {
                Ok(GdValue::GdString(s.to_owned()))
            }
        }
        "rpad" => {
            if args.is_empty() || args.len() > 2 {
                return Err(InterpError::argument_error(
                    format!("String.rpad() takes 1-2 arguments, got {}", args.len()),
                    0,
                    0,
                ));
            }
            let n = get_int_arg(method, &args[0])? as usize;
            let pad_char = if args.len() == 2 {
                get_string_arg(method, &args[1])?
            } else {
                " "
            };
            let pad_char = pad_char.chars().next().unwrap_or(' ');
            if s.len() < n {
                let padding: String = std::iter::repeat_n(pad_char, n - s.len()).collect();
                Ok(GdValue::GdString(format!("{s}{padding}")))
            } else {
                Ok(GdValue::GdString(s.to_owned()))
            }
        }
        _ => Err(InterpError::name_error(
            format!("String has no method '{method}'"),
            0,
            0,
        )),
    }
}

fn get_string_arg<'a>(method: &str, val: &'a GdValue) -> InterpResult<&'a str> {
    match val {
        GdValue::GdString(s) | GdValue::StringName(s) => Ok(s),
        _ => Err(InterpError::type_error(
            format!(
                "String.{method}() expected String argument, got {}",
                val.type_name()
            ),
            0,
            0,
        )),
    }
}

fn get_int_arg(method: &str, val: &GdValue) -> InterpResult<i64> {
    match val {
        GdValue::Int(n) => Ok(*n),
        _ => Err(InterpError::type_error(
            format!(
                "String.{method}() expected int argument, got {}",
                val.type_name()
            ),
            0,
            0,
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(val: &str) -> GdValue {
        GdValue::GdString(val.to_owned())
    }

    #[test]
    fn length() {
        assert_eq!(
            call_method(&s("hello"), "length", &[]).unwrap(),
            GdValue::Int(5)
        );
    }

    #[test]
    fn is_empty() {
        assert_eq!(
            call_method(&s(""), "is_empty", &[]).unwrap(),
            GdValue::Bool(true)
        );
        assert_eq!(
            call_method(&s("x"), "is_empty", &[]).unwrap(),
            GdValue::Bool(false)
        );
    }

    #[test]
    fn contains() {
        assert_eq!(
            call_method(&s("hello world"), "contains", &[s("world")]).unwrap(),
            GdValue::Bool(true)
        );
        assert_eq!(
            call_method(&s("hello"), "contains", &[s("xyz")]).unwrap(),
            GdValue::Bool(false)
        );
    }

    #[test]
    fn begins_ends_with() {
        assert_eq!(
            call_method(&s("hello"), "begins_with", &[s("hel")]).unwrap(),
            GdValue::Bool(true)
        );
        assert_eq!(
            call_method(&s("hello"), "ends_with", &[s("llo")]).unwrap(),
            GdValue::Bool(true)
        );
    }

    #[test]
    fn find_rfind() {
        assert_eq!(
            call_method(&s("abcabc"), "find", &[s("bc")]).unwrap(),
            GdValue::Int(1)
        );
        assert_eq!(
            call_method(&s("abcabc"), "rfind", &[s("bc")]).unwrap(),
            GdValue::Int(4)
        );
        assert_eq!(
            call_method(&s("hello"), "find", &[s("xyz")]).unwrap(),
            GdValue::Int(-1)
        );
    }

    #[test]
    fn to_lower_upper() {
        assert_eq!(
            call_method(&s("Hello"), "to_lower", &[]).unwrap(),
            s("hello")
        );
        assert_eq!(
            call_method(&s("Hello"), "to_upper", &[]).unwrap(),
            s("HELLO")
        );
    }

    #[test]
    fn strip_edges() {
        assert_eq!(
            call_method(&s("  hello  "), "strip_edges", &[]).unwrap(),
            s("hello")
        );
    }

    #[test]
    fn replace() {
        assert_eq!(
            call_method(&s("hello world"), "replace", &[s("world"), s("rust")]).unwrap(),
            s("hello rust")
        );
    }

    #[test]
    fn split() {
        let result = call_method(&s("a,b,c"), "split", &[s(",")]).unwrap();
        assert_eq!(result, GdValue::Array(vec![s("a"), s("b"), s("c")]));
    }

    #[test]
    fn join() {
        let arr = GdValue::Array(vec![s("a"), s("b"), s("c")]);
        assert_eq!(call_method(&s(", "), "join", &[arr]).unwrap(), s("a, b, c"));
    }

    #[test]
    fn substr() {
        assert_eq!(
            call_method(&s("hello"), "substr", &[GdValue::Int(1), GdValue::Int(3)]).unwrap(),
            s("ell")
        );
    }

    #[test]
    fn left_right() {
        assert_eq!(
            call_method(&s("hello"), "left", &[GdValue::Int(3)]).unwrap(),
            s("hel")
        );
        assert_eq!(
            call_method(&s("hello"), "right", &[GdValue::Int(3)]).unwrap(),
            s("llo")
        );
    }

    #[test]
    fn repeat_method() {
        assert_eq!(
            call_method(&s("ab"), "repeat", &[GdValue::Int(3)]).unwrap(),
            s("ababab")
        );
    }

    #[test]
    fn to_int_to_float() {
        assert_eq!(
            call_method(&s("42"), "to_int", &[]).unwrap(),
            GdValue::Int(42)
        );
        assert_eq!(
            call_method(&s("3.14"), "to_float", &[]).unwrap(),
            GdValue::Float(3.14)
        );
        assert_eq!(
            call_method(&s("abc"), "to_int", &[]).unwrap(),
            GdValue::Int(0)
        );
    }

    #[test]
    fn is_valid_int_float() {
        assert_eq!(
            call_method(&s("42"), "is_valid_int", &[]).unwrap(),
            GdValue::Bool(true)
        );
        assert_eq!(
            call_method(&s("abc"), "is_valid_int", &[]).unwrap(),
            GdValue::Bool(false)
        );
        assert_eq!(
            call_method(&s("3.14"), "is_valid_float", &[]).unwrap(),
            GdValue::Bool(true)
        );
    }

    #[test]
    fn capitalize_method() {
        assert_eq!(
            call_method(&s("hello world"), "capitalize", &[]).unwrap(),
            s("Hello World")
        );
    }

    #[test]
    fn to_snake_case_method() {
        assert_eq!(
            call_method(&s("MyVariable"), "to_snake_case", &[]).unwrap(),
            s("my_variable")
        );
    }

    #[test]
    fn pad_zeros_method() {
        assert_eq!(
            call_method(&s("42"), "pad_zeros", &[GdValue::Int(5)]).unwrap(),
            s("00042")
        );
    }

    #[test]
    fn lpad_rpad() {
        assert_eq!(
            call_method(&s("hi"), "lpad", &[GdValue::Int(5), s(".")]).unwrap(),
            s("...hi")
        );
        assert_eq!(
            call_method(&s("hi"), "rpad", &[GdValue::Int(5), s(".")]).unwrap(),
            s("hi...")
        );
    }

    #[test]
    fn unknown_method_errors() {
        assert!(call_method(&s("hello"), "nonexistent", &[]).is_err());
    }
}
