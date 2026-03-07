use crate::error::{InterpError, InterpResult};
use crate::value::GdValue;

fn expect_argc(name: &str, args: &[GdValue], expected: usize) -> InterpResult<()> {
    if args.len() == expected {
        Ok(())
    } else {
        Err(InterpError::argument_error(
            format!("{name}() takes {expected} argument(s), got {}", args.len()),
            0,
            0,
        ))
    }
}

fn to_float(val: &GdValue, name: &str) -> InterpResult<f64> {
    match val {
        GdValue::Float(v) => Ok(*v),
        GdValue::Int(n) => Ok(*n as f64),
        _ => Err(InterpError::type_error(
            format!(
                "{name}() expected numeric argument, got {}",
                val.type_name()
            ),
            0,
            0,
        )),
    }
}

#[allow(clippy::too_many_lines)]
pub fn call(name: &str, args: &[GdValue]) -> InterpResult<GdValue> {
    match name {
        "str" => {
            expect_argc(name, args, 1)?;
            Ok(GdValue::GdString(args[0].to_string()))
        }
        "int" => {
            expect_argc(name, args, 1)?;
            let val = match &args[0] {
                GdValue::Int(n) => *n,
                GdValue::Float(v) => *v as i64,
                GdValue::Bool(b) => i64::from(*b),
                GdValue::GdString(s) | GdValue::StringName(s) => s.parse().unwrap_or(0),
                _ => {
                    return Err(InterpError::type_error(
                        format!("int() cannot convert {}", args[0].type_name()),
                        0,
                        0,
                    ));
                }
            };
            Ok(GdValue::Int(val))
        }
        "float" => {
            expect_argc(name, args, 1)?;
            let val = match &args[0] {
                GdValue::Float(v) => *v,
                GdValue::Int(n) => *n as f64,
                GdValue::Bool(b) => {
                    if *b {
                        1.0
                    } else {
                        0.0
                    }
                }
                GdValue::GdString(s) | GdValue::StringName(s) => s.parse().unwrap_or(0.0),
                _ => {
                    return Err(InterpError::type_error(
                        format!("float() cannot convert {}", args[0].type_name()),
                        0,
                        0,
                    ));
                }
            };
            Ok(GdValue::Float(val))
        }
        "bool" => {
            expect_argc(name, args, 1)?;
            Ok(GdValue::Bool(args[0].is_truthy()))
        }
        "typeof" => {
            expect_argc(name, args, 1)?;
            Ok(GdValue::Int(args[0].type_id()))
        }
        "len" => {
            expect_argc(name, args, 1)?;
            match &args[0] {
                GdValue::Array(items) => Ok(GdValue::Int(items.len() as i64)),
                GdValue::GdString(s) | GdValue::StringName(s) => Ok(GdValue::Int(s.len() as i64)),
                GdValue::Dictionary(entries) => Ok(GdValue::Int(entries.len() as i64)),
                _ => Err(InterpError::type_error(
                    format!("len() not supported for {}", args[0].type_name()),
                    0,
                    0,
                )),
            }
        }
        "range" => call_range(args),
        "abs" => {
            expect_argc(name, args, 1)?;
            match &args[0] {
                GdValue::Int(n) => Ok(GdValue::Int(n.abs())),
                GdValue::Float(v) => Ok(GdValue::Float(v.abs())),
                _ => Err(InterpError::type_error(
                    format!("abs() expected numeric, got {}", args[0].type_name()),
                    0,
                    0,
                )),
            }
        }
        "sign" => {
            expect_argc(name, args, 1)?;
            match &args[0] {
                GdValue::Int(n) => Ok(GdValue::Int(n.signum())),
                GdValue::Float(v) => Ok(GdValue::Float(v.signum())),
                _ => Err(InterpError::type_error(
                    format!("sign() expected numeric, got {}", args[0].type_name()),
                    0,
                    0,
                )),
            }
        }
        "min" => {
            expect_argc(name, args, 2)?;
            match (&args[0], &args[1]) {
                (GdValue::Int(a), GdValue::Int(b)) => Ok(GdValue::Int(*a.min(b))),
                (GdValue::Float(a), GdValue::Float(b)) => Ok(GdValue::Float(a.min(*b))),
                (GdValue::Int(a), GdValue::Float(b)) => Ok(GdValue::Float((*a as f64).min(*b))),
                (GdValue::Float(a), GdValue::Int(b)) => Ok(GdValue::Float(a.min(*b as f64))),
                _ => Err(InterpError::type_error(
                    "min() requires numeric arguments".to_owned(),
                    0,
                    0,
                )),
            }
        }
        "max" => {
            expect_argc(name, args, 2)?;
            match (&args[0], &args[1]) {
                (GdValue::Int(a), GdValue::Int(b)) => Ok(GdValue::Int(*a.max(b))),
                (GdValue::Float(a), GdValue::Float(b)) => Ok(GdValue::Float(a.max(*b))),
                (GdValue::Int(a), GdValue::Float(b)) => Ok(GdValue::Float((*a as f64).max(*b))),
                (GdValue::Float(a), GdValue::Int(b)) => Ok(GdValue::Float(a.max(*b as f64))),
                _ => Err(InterpError::type_error(
                    "max() requires numeric arguments".to_owned(),
                    0,
                    0,
                )),
            }
        }
        "clamp" => {
            expect_argc(name, args, 3)?;
            if let (GdValue::Int(v), GdValue::Int(lo), GdValue::Int(hi)) =
                (&args[0], &args[1], &args[2])
            {
                Ok(GdValue::Int(*v.max(lo).min(hi)))
            } else {
                let v = to_float(&args[0], name)?;
                let lo = to_float(&args[1], name)?;
                let hi = to_float(&args[2], name)?;
                Ok(GdValue::Float(v.max(lo).min(hi)))
            }
        }
        "sin" => {
            expect_argc(name, args, 1)?;
            Ok(GdValue::Float(to_float(&args[0], name)?.sin()))
        }
        "cos" => {
            expect_argc(name, args, 1)?;
            Ok(GdValue::Float(to_float(&args[0], name)?.cos()))
        }
        "tan" => {
            expect_argc(name, args, 1)?;
            Ok(GdValue::Float(to_float(&args[0], name)?.tan()))
        }
        "asin" => {
            expect_argc(name, args, 1)?;
            Ok(GdValue::Float(to_float(&args[0], name)?.asin()))
        }
        "acos" => {
            expect_argc(name, args, 1)?;
            Ok(GdValue::Float(to_float(&args[0], name)?.acos()))
        }
        "atan" => {
            expect_argc(name, args, 1)?;
            Ok(GdValue::Float(to_float(&args[0], name)?.atan()))
        }
        "atan2" => {
            expect_argc(name, args, 2)?;
            let y = to_float(&args[0], name)?;
            let x = to_float(&args[1], name)?;
            Ok(GdValue::Float(y.atan2(x)))
        }
        "sqrt" => {
            expect_argc(name, args, 1)?;
            Ok(GdValue::Float(to_float(&args[0], name)?.sqrt()))
        }
        "pow" => {
            expect_argc(name, args, 2)?;
            let base = to_float(&args[0], name)?;
            let exp = to_float(&args[1], name)?;
            Ok(GdValue::Float(base.powf(exp)))
        }
        "floor" => {
            expect_argc(name, args, 1)?;
            Ok(GdValue::Float(to_float(&args[0], name)?.floor()))
        }
        "ceil" => {
            expect_argc(name, args, 1)?;
            Ok(GdValue::Float(to_float(&args[0], name)?.ceil()))
        }
        "round" => {
            expect_argc(name, args, 1)?;
            Ok(GdValue::Float(to_float(&args[0], name)?.round()))
        }
        "fmod" => {
            expect_argc(name, args, 2)?;
            let a = to_float(&args[0], name)?;
            let b = to_float(&args[1], name)?;
            Ok(GdValue::Float(a % b))
        }
        "fposmod" => {
            expect_argc(name, args, 2)?;
            let a = to_float(&args[0], name)?;
            let b = to_float(&args[1], name)?;
            let r = a % b;
            Ok(GdValue::Float(if r < 0.0 { r + b.abs() } else { r }))
        }
        "lerp" | "lerpf" => {
            expect_argc(name, args, 3)?;
            let from = to_float(&args[0], name)?;
            let to = to_float(&args[1], name)?;
            let weight = to_float(&args[2], name)?;
            Ok(GdValue::Float(from + (to - from) * weight))
        }
        "is_nan" => {
            expect_argc(name, args, 1)?;
            Ok(GdValue::Bool(to_float(&args[0], name)?.is_nan()))
        }
        "is_inf" => {
            expect_argc(name, args, 1)?;
            Ok(GdValue::Bool(to_float(&args[0], name)?.is_infinite()))
        }
        "is_zero_approx" => {
            expect_argc(name, args, 1)?;
            let v = to_float(&args[0], name)?;
            Ok(GdValue::Bool(v.abs() < 1e-6))
        }
        "is_equal_approx" => {
            expect_argc(name, args, 2)?;
            let a = to_float(&args[0], name)?;
            let b = to_float(&args[1], name)?;
            Ok(GdValue::Bool((a - b).abs() < 1e-6))
        }
        "randi" => {
            expect_argc(name, args, 0)?;
            Ok(GdValue::Int(0))
        }
        "randf" => {
            expect_argc(name, args, 0)?;
            Ok(GdValue::Float(0.0))
        }
        "snapped" => {
            expect_argc(name, args, 2)?;
            let v = to_float(&args[0], name)?;
            let step = to_float(&args[1], name)?;
            if step == 0.0 {
                Ok(GdValue::Float(v))
            } else {
                Ok(GdValue::Float((v / step).round() * step))
            }
        }
        "wrapi" => {
            expect_argc(name, args, 3)?;
            match (&args[0], &args[1], &args[2]) {
                (GdValue::Int(v), GdValue::Int(lo), GdValue::Int(hi)) => {
                    if lo == hi {
                        Ok(GdValue::Int(*lo))
                    } else {
                        let range = hi - lo;
                        Ok(GdValue::Int(lo + ((v - lo) % range + range) % range))
                    }
                }
                _ => Err(InterpError::type_error(
                    "wrapi() requires int arguments".to_owned(),
                    0,
                    0,
                )),
            }
        }
        "wrapf" => {
            expect_argc(name, args, 3)?;
            let v = to_float(&args[0], name)?;
            let lo = to_float(&args[1], name)?;
            let hi = to_float(&args[2], name)?;
            if (hi - lo).abs() < f64::EPSILON {
                Ok(GdValue::Float(lo))
            } else {
                let range = hi - lo;
                Ok(GdValue::Float(lo + ((v - lo) % range + range) % range))
            }
        }
        _ => Err(InterpError::name_error(
            format!("unknown function: {name}"),
            0,
            0,
        )),
    }
}

fn call_range(args: &[GdValue]) -> InterpResult<GdValue> {
    let (start, end, step) = match args.len() {
        1 => {
            let n = match &args[0] {
                GdValue::Int(n) => *n,
                _ => {
                    return Err(InterpError::type_error(
                        "range() requires int arguments".to_owned(),
                        0,
                        0,
                    ));
                }
            };
            (0, n, 1)
        }
        2 => {
            let start = match &args[0] {
                GdValue::Int(n) => *n,
                _ => {
                    return Err(InterpError::type_error(
                        "range() requires int arguments".to_owned(),
                        0,
                        0,
                    ));
                }
            };
            let end = match &args[1] {
                GdValue::Int(n) => *n,
                _ => {
                    return Err(InterpError::type_error(
                        "range() requires int arguments".to_owned(),
                        0,
                        0,
                    ));
                }
            };
            (start, end, 1)
        }
        3 => {
            let start = match &args[0] {
                GdValue::Int(n) => *n,
                _ => {
                    return Err(InterpError::type_error(
                        "range() requires int arguments".to_owned(),
                        0,
                        0,
                    ));
                }
            };
            let end = match &args[1] {
                GdValue::Int(n) => *n,
                _ => {
                    return Err(InterpError::type_error(
                        "range() requires int arguments".to_owned(),
                        0,
                        0,
                    ));
                }
            };
            let step = match &args[2] {
                GdValue::Int(n) => *n,
                _ => {
                    return Err(InterpError::type_error(
                        "range() requires int arguments".to_owned(),
                        0,
                        0,
                    ));
                }
            };
            if step == 0 {
                return Err(InterpError::value_error(
                    "range() step cannot be zero".to_owned(),
                    0,
                    0,
                ));
            }
            (start, end, step)
        }
        _ => {
            return Err(InterpError::argument_error(
                format!("range() takes 1-3 arguments, got {}", args.len()),
                0,
                0,
            ));
        }
    };

    let mut result = Vec::new();
    if step > 0 {
        let mut i = start;
        while i < end {
            result.push(GdValue::Int(i));
            i += step;
        }
    } else {
        let mut i = start;
        while i > end {
            result.push(GdValue::Int(i));
            i += step;
        }
    }
    Ok(GdValue::Array(result))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn str_converts() {
        assert_eq!(
            call("str", &[GdValue::Int(42)]).unwrap(),
            GdValue::GdString("42".into())
        );
    }

    #[test]
    fn int_from_float() {
        assert_eq!(
            call("int", &[GdValue::Float(3.7)]).unwrap(),
            GdValue::Int(3)
        );
    }

    #[test]
    fn int_from_bool() {
        assert_eq!(
            call("int", &[GdValue::Bool(true)]).unwrap(),
            GdValue::Int(1)
        );
    }

    #[test]
    fn int_from_string() {
        assert_eq!(
            call("int", &[GdValue::GdString("123".into())]).unwrap(),
            GdValue::Int(123)
        );
        assert_eq!(
            call("int", &[GdValue::GdString("abc".into())]).unwrap(),
            GdValue::Int(0)
        );
    }

    #[test]
    fn float_from_int() {
        assert_eq!(
            call("float", &[GdValue::Int(5)]).unwrap(),
            GdValue::Float(5.0)
        );
    }

    #[test]
    fn bool_conversion() {
        assert_eq!(
            call("bool", &[GdValue::Int(0)]).unwrap(),
            GdValue::Bool(false)
        );
        assert_eq!(
            call("bool", &[GdValue::Int(1)]).unwrap(),
            GdValue::Bool(true)
        );
    }

    #[test]
    fn typeof_returns_type_id() {
        assert_eq!(call("typeof", &[GdValue::Null]).unwrap(), GdValue::Int(0));
        assert_eq!(call("typeof", &[GdValue::Int(1)]).unwrap(), GdValue::Int(2));
    }

    #[test]
    fn len_array() {
        let arr = GdValue::Array(vec![GdValue::Int(1), GdValue::Int(2)]);
        assert_eq!(call("len", &[arr]).unwrap(), GdValue::Int(2));
    }

    #[test]
    fn len_string() {
        assert_eq!(
            call("len", &[GdValue::GdString("hello".into())]).unwrap(),
            GdValue::Int(5)
        );
    }

    #[test]
    fn range_one_arg() {
        let result = call("range", &[GdValue::Int(3)]).unwrap();
        assert_eq!(
            result,
            GdValue::Array(vec![GdValue::Int(0), GdValue::Int(1), GdValue::Int(2)])
        );
    }

    #[test]
    fn range_two_args() {
        let result = call("range", &[GdValue::Int(2), GdValue::Int(5)]).unwrap();
        assert_eq!(
            result,
            GdValue::Array(vec![GdValue::Int(2), GdValue::Int(3), GdValue::Int(4)])
        );
    }

    #[test]
    fn range_three_args() {
        let result = call(
            "range",
            &[GdValue::Int(0), GdValue::Int(10), GdValue::Int(3)],
        )
        .unwrap();
        assert_eq!(
            result,
            GdValue::Array(vec![
                GdValue::Int(0),
                GdValue::Int(3),
                GdValue::Int(6),
                GdValue::Int(9)
            ])
        );
    }

    #[test]
    fn range_negative_step() {
        let result = call(
            "range",
            &[GdValue::Int(5), GdValue::Int(0), GdValue::Int(-2)],
        )
        .unwrap();
        assert_eq!(
            result,
            GdValue::Array(vec![GdValue::Int(5), GdValue::Int(3), GdValue::Int(1)])
        );
    }

    #[test]
    fn range_zero_step_errors() {
        let result = call(
            "range",
            &[GdValue::Int(0), GdValue::Int(5), GdValue::Int(0)],
        );
        assert!(result.is_err());
    }

    #[test]
    fn abs_int() {
        assert_eq!(call("abs", &[GdValue::Int(-5)]).unwrap(), GdValue::Int(5));
    }

    #[test]
    fn abs_float() {
        assert_eq!(
            call("abs", &[GdValue::Float(-3.125)]).unwrap(),
            GdValue::Float(3.125)
        );
    }

    #[test]
    fn sign_int() {
        assert_eq!(
            call("sign", &[GdValue::Int(-10)]).unwrap(),
            GdValue::Int(-1)
        );
        assert_eq!(call("sign", &[GdValue::Int(0)]).unwrap(), GdValue::Int(0));
        assert_eq!(call("sign", &[GdValue::Int(10)]).unwrap(), GdValue::Int(1));
    }

    #[test]
    fn min_max() {
        assert_eq!(
            call("min", &[GdValue::Int(3), GdValue::Int(7)]).unwrap(),
            GdValue::Int(3)
        );
        assert_eq!(
            call("max", &[GdValue::Int(3), GdValue::Int(7)]).unwrap(),
            GdValue::Int(7)
        );
    }

    #[test]
    fn clamp_int() {
        assert_eq!(
            call(
                "clamp",
                &[GdValue::Int(15), GdValue::Int(0), GdValue::Int(10)]
            )
            .unwrap(),
            GdValue::Int(10)
        );
        assert_eq!(
            call(
                "clamp",
                &[GdValue::Int(-5), GdValue::Int(0), GdValue::Int(10)]
            )
            .unwrap(),
            GdValue::Int(0)
        );
    }

    #[test]
    fn trig_functions() {
        assert_eq!(
            call("sin", &[GdValue::Float(0.0)]).unwrap(),
            GdValue::Float(0.0)
        );
        assert_eq!(
            call("cos", &[GdValue::Float(0.0)]).unwrap(),
            GdValue::Float(1.0)
        );
    }

    #[test]
    fn sqrt_pow() {
        assert_eq!(
            call("sqrt", &[GdValue::Float(9.0)]).unwrap(),
            GdValue::Float(3.0)
        );
        assert_eq!(
            call("pow", &[GdValue::Float(2.0), GdValue::Float(3.0)]).unwrap(),
            GdValue::Float(8.0)
        );
    }

    #[test]
    fn floor_ceil_round() {
        assert_eq!(
            call("floor", &[GdValue::Float(3.7)]).unwrap(),
            GdValue::Float(3.0)
        );
        assert_eq!(
            call("ceil", &[GdValue::Float(3.2)]).unwrap(),
            GdValue::Float(4.0)
        );
        assert_eq!(
            call("round", &[GdValue::Float(3.5)]).unwrap(),
            GdValue::Float(4.0)
        );
    }

    #[test]
    fn lerp_works() {
        assert_eq!(
            call(
                "lerp",
                &[
                    GdValue::Float(0.0),
                    GdValue::Float(10.0),
                    GdValue::Float(0.5)
                ]
            )
            .unwrap(),
            GdValue::Float(5.0)
        );
    }

    #[test]
    fn is_nan_inf() {
        assert_eq!(
            call("is_nan", &[GdValue::Float(f64::NAN)]).unwrap(),
            GdValue::Bool(true)
        );
        assert_eq!(
            call("is_inf", &[GdValue::Float(f64::INFINITY)]).unwrap(),
            GdValue::Bool(true)
        );
        assert_eq!(
            call("is_nan", &[GdValue::Float(1.0)]).unwrap(),
            GdValue::Bool(false)
        );
    }

    #[test]
    fn is_zero_approx() {
        assert_eq!(
            call("is_zero_approx", &[GdValue::Float(0.000_000_1)]).unwrap(),
            GdValue::Bool(true)
        );
        assert_eq!(
            call("is_zero_approx", &[GdValue::Float(1.0)]).unwrap(),
            GdValue::Bool(false)
        );
    }

    #[test]
    fn is_equal_approx() {
        assert_eq!(
            call(
                "is_equal_approx",
                &[GdValue::Float(1.0), GdValue::Float(1.000_000_1)]
            )
            .unwrap(),
            GdValue::Bool(true)
        );
    }

    #[test]
    fn randi_randf_deterministic() {
        assert_eq!(call("randi", &[]).unwrap(), GdValue::Int(0));
        assert_eq!(call("randf", &[]).unwrap(), GdValue::Float(0.0));
    }

    #[test]
    fn snapped_works() {
        assert_eq!(
            call("snapped", &[GdValue::Float(7.3), GdValue::Float(2.5)]).unwrap(),
            GdValue::Float(7.5)
        );
    }

    #[test]
    fn wrapi_works() {
        assert_eq!(
            call(
                "wrapi",
                &[GdValue::Int(7), GdValue::Int(0), GdValue::Int(5)]
            )
            .unwrap(),
            GdValue::Int(2)
        );
    }

    #[test]
    fn wrapf_works() {
        let result = call(
            "wrapf",
            &[
                GdValue::Float(7.0),
                GdValue::Float(0.0),
                GdValue::Float(5.0),
            ],
        )
        .unwrap();
        if let GdValue::Float(v) = result {
            assert!((v - 2.0).abs() < 1e-10);
        } else {
            panic!("expected Float");
        }
    }

    #[test]
    fn unknown_function_errors() {
        assert!(call("nonexistent", &[]).is_err());
    }

    #[test]
    fn wrong_argc_errors() {
        assert!(call("abs", &[]).is_err());
        assert!(call("abs", &[GdValue::Int(1), GdValue::Int(2)]).is_err());
    }

    #[test]
    fn fmod_works() {
        assert_eq!(
            call("fmod", &[GdValue::Float(7.0), GdValue::Float(3.0)]).unwrap(),
            GdValue::Float(1.0)
        );
    }

    #[test]
    fn fposmod_positive() {
        let result = call("fposmod", &[GdValue::Float(-1.0), GdValue::Float(3.0)]).unwrap();
        if let GdValue::Float(v) = result {
            assert!((v - 2.0).abs() < 1e-10);
        } else {
            panic!("expected Float");
        }
    }
}
