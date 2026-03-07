use crate::error::{InterpError, InterpResult};
use crate::value::GdValue;

fn expect_argc(method: &str, args: &[GdValue], expected: usize) -> InterpResult<()> {
    if args.len() == expected {
        Ok(())
    } else {
        Err(InterpError::argument_error(
            format!(
                "Dictionary.{method}() takes {expected} argument(s), got {}",
                args.len()
            ),
            0,
            0,
        ))
    }
}

fn get_entries(receiver: &GdValue) -> &[(GdValue, GdValue)] {
    match receiver {
        GdValue::Dictionary(entries) => entries,
        _ => unreachable!("dictionary::call_method called with non-dictionary"),
    }
}

pub fn call_method(receiver: &GdValue, method: &str, args: &[GdValue]) -> InterpResult<GdValue> {
    let entries = get_entries(receiver);

    match method {
        "size" => {
            expect_argc(method, args, 0)?;
            Ok(GdValue::Int(entries.len() as i64))
        }
        "is_empty" => {
            expect_argc(method, args, 0)?;
            Ok(GdValue::Bool(entries.is_empty()))
        }
        "has" => {
            expect_argc(method, args, 1)?;
            Ok(GdValue::Bool(entries.iter().any(|(k, _)| k == &args[0])))
        }
        "has_all" => {
            expect_argc(method, args, 1)?;
            match &args[0] {
                GdValue::Array(keys) => {
                    let has_all = keys.iter().all(|key| entries.iter().any(|(k, _)| k == key));
                    Ok(GdValue::Bool(has_all))
                }
                _ => Err(InterpError::type_error(
                    "Dictionary.has_all() requires an Array argument".to_owned(),
                    0,
                    0,
                )),
            }
        }
        "keys" => {
            expect_argc(method, args, 0)?;
            let keys: Vec<GdValue> = entries.iter().map(|(k, _)| k.clone()).collect();
            Ok(GdValue::Array(keys))
        }
        "values" => {
            expect_argc(method, args, 0)?;
            let values: Vec<GdValue> = entries.iter().map(|(_, v)| v.clone()).collect();
            Ok(GdValue::Array(values))
        }
        "get" => {
            if args.is_empty() || args.len() > 2 {
                return Err(InterpError::argument_error(
                    format!("Dictionary.get() takes 1-2 arguments, got {}", args.len()),
                    0,
                    0,
                ));
            }
            let key = &args[0];
            let default = if args.len() == 2 {
                &args[1]
            } else {
                &GdValue::Null
            };
            let value = entries
                .iter()
                .find(|(k, _)| k == key)
                .map_or(default, |(_, v)| v);
            Ok(value.clone())
        }
        "duplicate" => {
            expect_argc(method, args, 0)?;
            Ok(GdValue::Dictionary(entries.to_vec()))
        }
        "hash" => {
            expect_argc(method, args, 0)?;
            Ok(GdValue::Int(0))
        }
        "erase" | "merge" | "clear" => Err(InterpError::not_implemented(
            "mutable dictionary methods",
            0,
            0,
        )),
        _ => Err(InterpError::name_error(
            format!("Dictionary has no method '{method}'"),
            0,
            0,
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dict(pairs: &[(&str, i64)]) -> GdValue {
        GdValue::Dictionary(
            pairs
                .iter()
                .map(|(k, v)| (GdValue::GdString((*k).to_owned()), GdValue::Int(*v)))
                .collect(),
        )
    }

    fn sk(s: &str) -> GdValue {
        GdValue::GdString(s.to_owned())
    }

    #[test]
    fn size() {
        assert_eq!(
            call_method(&dict(&[("a", 1), ("b", 2)]), "size", &[]).unwrap(),
            GdValue::Int(2)
        );
    }

    #[test]
    fn is_empty() {
        assert_eq!(
            call_method(&dict(&[]), "is_empty", &[]).unwrap(),
            GdValue::Bool(true)
        );
        assert_eq!(
            call_method(&dict(&[("a", 1)]), "is_empty", &[]).unwrap(),
            GdValue::Bool(false)
        );
    }

    #[test]
    fn has() {
        let d = dict(&[("x", 10), ("y", 20)]);
        assert_eq!(
            call_method(&d, "has", &[sk("x")]).unwrap(),
            GdValue::Bool(true)
        );
        assert_eq!(
            call_method(&d, "has", &[sk("z")]).unwrap(),
            GdValue::Bool(false)
        );
    }

    #[test]
    fn has_all() {
        let d = dict(&[("a", 1), ("b", 2), ("c", 3)]);
        let keys = GdValue::Array(vec![sk("a"), sk("c")]);
        assert_eq!(
            call_method(&d, "has_all", &[keys]).unwrap(),
            GdValue::Bool(true)
        );
        let keys = GdValue::Array(vec![sk("a"), sk("z")]);
        assert_eq!(
            call_method(&d, "has_all", &[keys]).unwrap(),
            GdValue::Bool(false)
        );
    }

    #[test]
    fn keys_values() {
        let d = dict(&[("a", 1), ("b", 2)]);
        assert_eq!(
            call_method(&d, "keys", &[]).unwrap(),
            GdValue::Array(vec![sk("a"), sk("b")])
        );
        assert_eq!(
            call_method(&d, "values", &[]).unwrap(),
            GdValue::Array(vec![GdValue::Int(1), GdValue::Int(2)])
        );
    }

    #[test]
    fn get_with_default() {
        let d = dict(&[("a", 1)]);
        assert_eq!(call_method(&d, "get", &[sk("a")]).unwrap(), GdValue::Int(1));
        assert_eq!(
            call_method(&d, "get", &[sk("z"), GdValue::Int(99)]).unwrap(),
            GdValue::Int(99)
        );
        assert_eq!(call_method(&d, "get", &[sk("z")]).unwrap(), GdValue::Null);
    }

    #[test]
    fn duplicate() {
        let d = dict(&[("a", 1)]);
        let dup = call_method(&d, "duplicate", &[]).unwrap();
        assert_eq!(d, dup);
    }

    #[test]
    fn mutating_not_implemented() {
        let d = dict(&[("a", 1)]);
        assert!(call_method(&d, "erase", &[sk("a")]).is_err());
        assert!(call_method(&d, "clear", &[]).is_err());
    }

    #[test]
    fn unknown_method() {
        assert!(call_method(&dict(&[]), "nonexistent", &[]).is_err());
    }
}
