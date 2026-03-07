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
        | "assign" => Err(InterpError::not_implemented(
            "mutable array methods (call on a variable, not a literal)",
            0,
            0,
        )),
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

/// Returns true if this method mutates the array.
pub fn is_mutating(method: &str) -> bool {
    matches!(
        method,
        "append"
            | "push_back"
            | "push_front"
            | "pop_back"
            | "pop_front"
            | "insert"
            | "remove_at"
            | "erase"
            | "sort"
            | "reverse"
            | "clear"
            | "resize"
            | "shuffle"
    )
}

/// Call a mutating method on an array, modifying it in place.
#[allow(clippy::too_many_lines)]
pub fn call_method_mut(
    receiver: &mut GdValue,
    method: &str,
    args: &[GdValue],
) -> InterpResult<GdValue> {
    let GdValue::Array(items) = receiver else {
        unreachable!("array::call_method_mut called with non-array");
    };

    match method {
        "append" | "push_back" => {
            expect_argc(method, args, 1)?;
            items.push(args[0].clone());
            Ok(GdValue::Null)
        }
        "push_front" => {
            expect_argc(method, args, 1)?;
            items.insert(0, args[0].clone());
            Ok(GdValue::Null)
        }
        "pop_back" => {
            expect_argc(method, args, 0)?;
            Ok(items.pop().unwrap_or(GdValue::Null))
        }
        "pop_front" => {
            expect_argc(method, args, 0)?;
            if items.is_empty() {
                Ok(GdValue::Null)
            } else {
                Ok(items.remove(0))
            }
        }
        "insert" => {
            expect_argc(method, args, 2)?;
            let GdValue::Int(idx) = &args[0] else {
                return Err(InterpError::type_error("insert() index must be int", 0, 0));
            };
            let pos = if *idx < 0 {
                (items.len() as i64 + idx).max(0) as usize
            } else {
                (*idx as usize).min(items.len())
            };
            items.insert(pos, args[1].clone());
            Ok(GdValue::Null)
        }
        "remove_at" => {
            expect_argc(method, args, 1)?;
            let GdValue::Int(idx) = &args[0] else {
                return Err(InterpError::type_error(
                    "remove_at() index must be int",
                    0,
                    0,
                ));
            };
            let pos = if *idx < 0 {
                (items.len() as i64 + idx) as usize
            } else {
                *idx as usize
            };
            if pos >= items.len() {
                return Err(InterpError::index_out_of_bounds(*idx, items.len(), 0, 0));
            }
            Ok(items.remove(pos))
        }
        "erase" => {
            expect_argc(method, args, 1)?;
            if let Some(pos) = items.iter().position(|v| v == &args[0]) {
                items.remove(pos);
            }
            Ok(GdValue::Null)
        }
        "sort" => {
            expect_argc(method, args, 0)?;
            items.sort_by(|a, b| match (a, b) {
                (GdValue::Int(x), GdValue::Int(y)) => x.cmp(y),
                (GdValue::Float(x), GdValue::Float(y)) => {
                    x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal)
                }
                (GdValue::GdString(x), GdValue::GdString(y)) => x.cmp(y),
                _ => std::cmp::Ordering::Equal,
            });
            Ok(GdValue::Null)
        }
        "reverse" => {
            expect_argc(method, args, 0)?;
            items.reverse();
            Ok(GdValue::Null)
        }
        "clear" => {
            expect_argc(method, args, 0)?;
            items.clear();
            Ok(GdValue::Null)
        }
        "resize" => {
            expect_argc(method, args, 1)?;
            let GdValue::Int(n) = &args[0] else {
                return Err(InterpError::type_error("resize() size must be int", 0, 0));
            };
            let new_len = (*n).max(0) as usize;
            items.resize(new_len, GdValue::Null);
            Ok(GdValue::Null)
        }
        "shuffle" => {
            expect_argc(method, args, 0)?;
            // Deterministic shuffle using a simple swap pattern
            let len = items.len();
            if len > 1 {
                for i in (1..len).rev() {
                    items.swap(i, i / 2);
                }
            }
            Ok(GdValue::Null)
        }
        _ => {
            // Delegate to immutable call_method for read-only methods
            call_method(receiver, method, args)
        }
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
    fn mutating_on_immutable_errors() {
        let a = arr(&[1]);
        assert!(call_method(&a, "append", &[GdValue::Int(2)]).is_err());
        assert!(call_method(&a, "sort", &[]).is_err());
    }

    #[test]
    fn append_mut() {
        let mut a = arr(&[1, 2]);
        call_method_mut(&mut a, "append", &[GdValue::Int(3)]).unwrap();
        assert_eq!(a, arr(&[1, 2, 3]));
    }

    #[test]
    fn pop_back_mut() {
        let mut a = arr(&[10, 20, 30]);
        let val = call_method_mut(&mut a, "pop_back", &[]).unwrap();
        assert_eq!(val, GdValue::Int(30));
        assert_eq!(a, arr(&[10, 20]));
    }

    #[test]
    fn pop_front_mut() {
        let mut a = arr(&[10, 20, 30]);
        let val = call_method_mut(&mut a, "pop_front", &[]).unwrap();
        assert_eq!(val, GdValue::Int(10));
        assert_eq!(a, arr(&[20, 30]));
    }

    #[test]
    fn sort_mut() {
        let mut a = arr(&[3, 1, 4, 1, 5]);
        call_method_mut(&mut a, "sort", &[]).unwrap();
        assert_eq!(a, arr(&[1, 1, 3, 4, 5]));
    }

    #[test]
    fn reverse_mut() {
        let mut a = arr(&[1, 2, 3]);
        call_method_mut(&mut a, "reverse", &[]).unwrap();
        assert_eq!(a, arr(&[3, 2, 1]));
    }

    #[test]
    fn erase_mut() {
        let mut a = arr(&[1, 2, 3, 2]);
        call_method_mut(&mut a, "erase", &[GdValue::Int(2)]).unwrap();
        assert_eq!(a, arr(&[1, 3, 2])); // only first occurrence
    }

    #[test]
    fn clear_mut() {
        let mut a = arr(&[1, 2, 3]);
        call_method_mut(&mut a, "clear", &[]).unwrap();
        assert_eq!(a, arr(&[]));
    }

    #[test]
    fn insert_mut() {
        let mut a = arr(&[1, 3]);
        call_method_mut(&mut a, "insert", &[GdValue::Int(1), GdValue::Int(2)]).unwrap();
        assert_eq!(a, arr(&[1, 2, 3]));
    }

    #[test]
    fn remove_at_mut() {
        let mut a = arr(&[10, 20, 30]);
        let val = call_method_mut(&mut a, "remove_at", &[GdValue::Int(1)]).unwrap();
        assert_eq!(val, GdValue::Int(20));
        assert_eq!(a, arr(&[10, 30]));
    }

    #[test]
    fn unknown_method() {
        assert!(call_method(&arr(&[]), "nonexistent", &[]).is_err());
    }
}
