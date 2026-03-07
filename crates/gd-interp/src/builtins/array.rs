use crate::error::{InterpError, InterpResult};
use crate::value::GdValue;

fn expect_argc(method: &str, args: &[GdValue], expected: usize) -> InterpResult<()> {
    if args.len() == expected {
        Ok(())
    } else {
        Err(InterpError::argument_error(
            format!(
                "Array.{method}() takes {expected} argument(s), got {}",
                args.len()
            ),
            0,
            0,
        ))
    }
}

fn get_items(receiver: &GdValue) -> &[GdValue] {
    match receiver {
        GdValue::Array(items) => items,
        _ => unreachable!("array::call_method called with non-array"),
    }
}

#[allow(clippy::too_many_lines)]
pub fn call_method(receiver: &GdValue, method: &str, args: &[GdValue]) -> InterpResult<GdValue> {
    let items = get_items(receiver);

    match method {
        "size" => {
            expect_argc(method, args, 0)?;
            Ok(GdValue::Int(items.len() as i64))
        }
        "is_empty" => {
            expect_argc(method, args, 0)?;
            Ok(GdValue::Bool(items.is_empty()))
        }
        "has" => {
            expect_argc(method, args, 1)?;
            Ok(GdValue::Bool(items.contains(&args[0])))
        }
        "find" => {
            expect_argc(method, args, 1)?;
            let idx = items
                .iter()
                .position(|v| v == &args[0])
                .map_or(-1, |i| i as i64);
            Ok(GdValue::Int(idx))
        }
        "rfind" => {
            expect_argc(method, args, 1)?;
            let idx = items
                .iter()
                .rposition(|v| v == &args[0])
                .map_or(-1, |i| i as i64);
            Ok(GdValue::Int(idx))
        }
        "count" => {
            expect_argc(method, args, 1)?;
            let count = items.iter().filter(|v| *v == &args[0]).count();
            Ok(GdValue::Int(count as i64))
        }
        "front" => {
            expect_argc(method, args, 0)?;
            Ok(items.first().cloned().unwrap_or(GdValue::Null))
        }
        "back" => {
            expect_argc(method, args, 0)?;
            Ok(items.last().cloned().unwrap_or(GdValue::Null))
        }
        "duplicate" => {
            expect_argc(method, args, 0)?;
            Ok(GdValue::Array(items.to_vec()))
        }
        "slice" => {
            if args.is_empty() || args.len() > 2 {
                return Err(InterpError::argument_error(
                    format!("Array.slice() takes 1-2 arguments, got {}", args.len()),
                    0,
                    0,
                ));
            }
            let begin = match &args[0] {
                GdValue::Int(n) => {
                    let n = *n;
                    if n < 0 {
                        (items.len() as i64 + n).max(0) as usize
                    } else {
                        (n as usize).min(items.len())
                    }
                }
                _ => {
                    return Err(InterpError::type_error(
                        "slice() begin must be int".to_owned(),
                        0,
                        0,
                    ));
                }
            };
            let end = if args.len() == 2 {
                match &args[1] {
                    GdValue::Int(n) => {
                        let n = *n;
                        if n < 0 {
                            (items.len() as i64 + n).max(0) as usize
                        } else {
                            (n as usize).min(items.len())
                        }
                    }
                    _ => {
                        return Err(InterpError::type_error(
                            "slice() end must be int".to_owned(),
                            0,
                            0,
                        ));
                    }
                }
            } else {
                items.len()
            };
            if begin >= end {
                Ok(GdValue::Array(Vec::new()))
            } else {
                Ok(GdValue::Array(items[begin..end].to_vec()))
            }
        }
        "min" => {
            expect_argc(method, args, 0)?;
            if items.is_empty() {
                return Ok(GdValue::Null);
            }
            let mut min_val = &items[0];
            for item in &items[1..] {
                match (min_val, item) {
                    (GdValue::Int(a), GdValue::Int(b)) => {
                        if b < a {
                            min_val = item;
                        }
                    }
                    (GdValue::Float(a), GdValue::Float(b)) => {
                        if b < a {
                            min_val = item;
                        }
                    }
                    _ => return Ok(GdValue::Null),
                }
            }
            Ok(min_val.clone())
        }
        "max" => {
            expect_argc(method, args, 0)?;
            if items.is_empty() {
                return Ok(GdValue::Null);
            }
            let mut max_val = &items[0];
            for item in &items[1..] {
                match (max_val, item) {
                    (GdValue::Int(a), GdValue::Int(b)) => {
                        if b > a {
                            max_val = item;
                        }
                    }
                    (GdValue::Float(a), GdValue::Float(b)) => {
                        if b > a {
                            max_val = item;
                        }
                    }
                    _ => return Ok(GdValue::Null),
                }
            }
            Ok(max_val.clone())
        }
        "hash" => {
            expect_argc(method, args, 0)?;
            Ok(GdValue::Int(0))
        }
        "append" | "push_back" | "push_front" | "pop_back" | "pop_front" | "insert"
        | "remove_at" | "erase" | "sort" | "reverse" | "clear" | "resize" | "shuffle"
        | "assign" => Err(InterpError::not_implemented("mutable array methods", 0, 0)),
        "map" | "filter" | "reduce" | "any" | "all" => {
            Err(InterpError::not_implemented("array callable methods", 0, 0))
        }
        _ => Err(InterpError::name_error(
            format!("Array has no method '{method}'"),
            0,
            0,
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arr(vals: &[i64]) -> GdValue {
        GdValue::Array(vals.iter().map(|n| GdValue::Int(*n)).collect())
    }

    #[test]
    fn size() {
        assert_eq!(
            call_method(&arr(&[1, 2, 3]), "size", &[]).unwrap(),
            GdValue::Int(3)
        );
    }

    #[test]
    fn is_empty() {
        assert_eq!(
            call_method(&arr(&[]), "is_empty", &[]).unwrap(),
            GdValue::Bool(true)
        );
        assert_eq!(
            call_method(&arr(&[1]), "is_empty", &[]).unwrap(),
            GdValue::Bool(false)
        );
    }

    #[test]
    fn has() {
        assert_eq!(
            call_method(&arr(&[1, 2, 3]), "has", &[GdValue::Int(2)]).unwrap(),
            GdValue::Bool(true)
        );
        assert_eq!(
            call_method(&arr(&[1, 2, 3]), "has", &[GdValue::Int(9)]).unwrap(),
            GdValue::Bool(false)
        );
    }

    #[test]
    fn find_rfind() {
        let a = arr(&[1, 2, 3, 2]);
        assert_eq!(
            call_method(&a, "find", &[GdValue::Int(2)]).unwrap(),
            GdValue::Int(1)
        );
        assert_eq!(
            call_method(&a, "rfind", &[GdValue::Int(2)]).unwrap(),
            GdValue::Int(3)
        );
        assert_eq!(
            call_method(&a, "find", &[GdValue::Int(9)]).unwrap(),
            GdValue::Int(-1)
        );
    }

    #[test]
    fn count() {
        assert_eq!(
            call_method(&arr(&[1, 2, 2, 3, 2]), "count", &[GdValue::Int(2)]).unwrap(),
            GdValue::Int(3)
        );
    }

    #[test]
    fn front_back() {
        assert_eq!(
            call_method(&arr(&[10, 20, 30]), "front", &[]).unwrap(),
            GdValue::Int(10)
        );
        assert_eq!(
            call_method(&arr(&[10, 20, 30]), "back", &[]).unwrap(),
            GdValue::Int(30)
        );
        assert_eq!(call_method(&arr(&[]), "front", &[]).unwrap(), GdValue::Null);
    }

    #[test]
    fn duplicate() {
        let a = arr(&[1, 2]);
        let d = call_method(&a, "duplicate", &[]).unwrap();
        assert_eq!(a, d);
    }

    #[test]
    fn slice() {
        let a = arr(&[10, 20, 30, 40, 50]);
        assert_eq!(
            call_method(&a, "slice", &[GdValue::Int(1), GdValue::Int(3)]).unwrap(),
            arr(&[20, 30])
        );
        assert_eq!(
            call_method(&a, "slice", &[GdValue::Int(2)]).unwrap(),
            arr(&[30, 40, 50])
        );
    }

    #[test]
    fn min_max() {
        assert_eq!(
            call_method(&arr(&[3, 1, 2]), "min", &[]).unwrap(),
            GdValue::Int(1)
        );
        assert_eq!(
            call_method(&arr(&[3, 1, 2]), "max", &[]).unwrap(),
            GdValue::Int(3)
        );
        assert_eq!(call_method(&arr(&[]), "min", &[]).unwrap(), GdValue::Null);
    }

    #[test]
    fn mutating_methods_not_implemented() {
        let a = arr(&[1]);
        assert!(call_method(&a, "append", &[GdValue::Int(2)]).is_err());
        assert!(call_method(&a, "sort", &[]).is_err());
    }

    #[test]
    fn unknown_method() {
        assert!(call_method(&arr(&[]), "nonexistent", &[]).is_err());
    }
}
