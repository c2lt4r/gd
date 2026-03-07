use crate::error::{InterpError, InterpResult};
use crate::value::GdValue;

pub fn get_property(receiver: &GdValue, property: &str) -> InterpResult<GdValue> {
    match receiver {
        GdValue::Vector2(x, y) => match property {
            "x" => Ok(GdValue::Float(*x)),
            "y" => Ok(GdValue::Float(*y)),
            "ZERO" => Ok(GdValue::Vector2(0.0, 0.0)),
            "ONE" => Ok(GdValue::Vector2(1.0, 1.0)),
            "UP" => Ok(GdValue::Vector2(0.0, -1.0)),
            "DOWN" => Ok(GdValue::Vector2(0.0, 1.0)),
            "LEFT" => Ok(GdValue::Vector2(-1.0, 0.0)),
            "RIGHT" => Ok(GdValue::Vector2(1.0, 0.0)),
            "INF" => Ok(GdValue::Vector2(f64::INFINITY, f64::INFINITY)),
            _ => Err(InterpError::type_error(
                format!("no property '{property}' on Vector2"),
                0,
                0,
            )),
        },
        GdValue::Vector2i(x, y) => match property {
            "x" => Ok(GdValue::Int(*x)),
            "y" => Ok(GdValue::Int(*y)),
            "ZERO" => Ok(GdValue::Vector2i(0, 0)),
            "ONE" => Ok(GdValue::Vector2i(1, 1)),
            "UP" => Ok(GdValue::Vector2i(0, -1)),
            "DOWN" => Ok(GdValue::Vector2i(0, 1)),
            "LEFT" => Ok(GdValue::Vector2i(-1, 0)),
            "RIGHT" => Ok(GdValue::Vector2i(1, 0)),
            _ => Err(InterpError::type_error(
                format!("no property '{property}' on Vector2i"),
                0,
                0,
            )),
        },
        _ => unreachable!("vector2::get_property called with non-vector2"),
    }
}

fn extract_v2(val: &GdValue, method: &str) -> InterpResult<(f64, f64)> {
    match val {
        GdValue::Vector2(x, y) => Ok((*x, *y)),
        _ => Err(InterpError::type_error(
            format!(
                "Vector2.{method}() expected Vector2 argument, got {}",
                val.type_name()
            ),
            0,
            0,
        )),
    }
}

fn extract_v2i(val: &GdValue, method: &str) -> InterpResult<(i64, i64)> {
    match val {
        GdValue::Vector2i(x, y) => Ok((*x, *y)),
        _ => Err(InterpError::type_error(
            format!(
                "Vector2i.{method}() expected Vector2i argument, got {}",
                val.type_name()
            ),
            0,
            0,
        )),
    }
}

fn expect_argc(method: &str, args: &[GdValue], expected: usize) -> InterpResult<()> {
    if args.len() == expected {
        Ok(())
    } else {
        Err(InterpError::argument_error(
            format!(
                "Vector2.{method}() takes {expected} argument(s), got {}",
                args.len()
            ),
            0,
            0,
        ))
    }
}

#[allow(clippy::too_many_lines)]
pub fn call_method(receiver: &GdValue, method: &str, args: &[GdValue]) -> InterpResult<GdValue> {
    match receiver {
        GdValue::Vector2(x, y) => call_method_v2(*x, *y, method, args),
        GdValue::Vector2i(x, y) => call_method_v2i(*x, *y, method, args),
        _ => unreachable!("vector2::call_method called with non-vector2"),
    }
}

#[allow(clippy::too_many_lines)]
fn call_method_v2(x: f64, y: f64, method: &str, args: &[GdValue]) -> InterpResult<GdValue> {
    match method {
        "length" => {
            expect_argc(method, args, 0)?;
            Ok(GdValue::Float((x * x + y * y).sqrt()))
        }
        "length_squared" => {
            expect_argc(method, args, 0)?;
            Ok(GdValue::Float(x * x + y * y))
        }
        "normalized" => {
            expect_argc(method, args, 0)?;
            let len = (x * x + y * y).sqrt();
            if len == 0.0 {
                Ok(GdValue::Vector2(0.0, 0.0))
            } else {
                Ok(GdValue::Vector2(x / len, y / len))
            }
        }
        "dot" => {
            expect_argc(method, args, 1)?;
            let (ox, oy) = extract_v2(&args[0], method)?;
            Ok(GdValue::Float(x * ox + y * oy))
        }
        "cross" => {
            expect_argc(method, args, 1)?;
            let (ox, oy) = extract_v2(&args[0], method)?;
            Ok(GdValue::Float(x * oy - y * ox))
        }
        "distance_to" => {
            expect_argc(method, args, 1)?;
            let (ox, oy) = extract_v2(&args[0], method)?;
            let dx = x - ox;
            let dy = y - oy;
            Ok(GdValue::Float((dx * dx + dy * dy).sqrt()))
        }
        "distance_squared_to" => {
            expect_argc(method, args, 1)?;
            let (ox, oy) = extract_v2(&args[0], method)?;
            let dx = x - ox;
            let dy = y - oy;
            Ok(GdValue::Float(dx * dx + dy * dy))
        }
        "angle" => {
            expect_argc(method, args, 0)?;
            Ok(GdValue::Float(y.atan2(x)))
        }
        "angle_to" => {
            expect_argc(method, args, 1)?;
            let (ox, oy) = extract_v2(&args[0], method)?;
            Ok(GdValue::Float((x * oy - y * ox).atan2(x * ox + y * oy)))
        }
        "lerp" => {
            expect_argc(method, args, 2)?;
            let (tx, ty) = extract_v2(&args[0], method)?;
            let weight = match &args[1] {
                GdValue::Float(w) => *w,
                GdValue::Int(w) => *w as f64,
                _ => {
                    return Err(InterpError::type_error(
                        "lerp() weight must be numeric".to_owned(),
                        0,
                        0,
                    ));
                }
            };
            Ok(GdValue::Vector2(
                x + (tx - x) * weight,
                y + (ty - y) * weight,
            ))
        }
        "abs" => {
            expect_argc(method, args, 0)?;
            Ok(GdValue::Vector2(x.abs(), y.abs()))
        }
        "sign" => {
            expect_argc(method, args, 0)?;
            Ok(GdValue::Vector2(x.signum(), y.signum()))
        }
        "floor" => {
            expect_argc(method, args, 0)?;
            Ok(GdValue::Vector2(x.floor(), y.floor()))
        }
        "ceil" => {
            expect_argc(method, args, 0)?;
            Ok(GdValue::Vector2(x.ceil(), y.ceil()))
        }
        "round" => {
            expect_argc(method, args, 0)?;
            Ok(GdValue::Vector2(x.round(), y.round()))
        }
        "clamp" => {
            expect_argc(method, args, 2)?;
            let (min_x, min_y) = extract_v2(&args[0], method)?;
            let (max_x, max_y) = extract_v2(&args[1], method)?;
            Ok(GdValue::Vector2(
                x.max(min_x).min(max_x),
                y.max(min_y).min(max_y),
            ))
        }
        _ => Err(InterpError::name_error(
            format!("Vector2 has no method '{method}'"),
            0,
            0,
        )),
    }
}

fn call_method_v2i(x: i64, y: i64, method: &str, args: &[GdValue]) -> InterpResult<GdValue> {
    match method {
        "length" => {
            expect_argc(method, args, 0)?;
            let fx = x as f64;
            let fy = y as f64;
            Ok(GdValue::Float((fx * fx + fy * fy).sqrt()))
        }
        "length_squared" => {
            expect_argc(method, args, 0)?;
            Ok(GdValue::Int(x * x + y * y))
        }
        "dot" => {
            expect_argc(method, args, 1)?;
            let (ox, oy) = extract_v2i(&args[0], method)?;
            Ok(GdValue::Int(x * ox + y * oy))
        }
        "cross" => {
            expect_argc(method, args, 1)?;
            let (ox, oy) = extract_v2i(&args[0], method)?;
            Ok(GdValue::Int(x * oy - y * ox))
        }
        "distance_to" => {
            expect_argc(method, args, 1)?;
            let (ox, oy) = extract_v2i(&args[0], method)?;
            let dx = (x - ox) as f64;
            let dy = (y - oy) as f64;
            Ok(GdValue::Float((dx * dx + dy * dy).sqrt()))
        }
        "distance_squared_to" => {
            expect_argc(method, args, 1)?;
            let (ox, oy) = extract_v2i(&args[0], method)?;
            let dx = x - ox;
            let dy = y - oy;
            Ok(GdValue::Int(dx * dx + dy * dy))
        }
        "abs" => {
            expect_argc(method, args, 0)?;
            Ok(GdValue::Vector2i(x.abs(), y.abs()))
        }
        "sign" => {
            expect_argc(method, args, 0)?;
            Ok(GdValue::Vector2i(x.signum(), y.signum()))
        }
        "clamp" => {
            expect_argc(method, args, 2)?;
            let (min_x, min_y) = extract_v2i(&args[0], method)?;
            let (max_x, max_y) = extract_v2i(&args[1], method)?;
            Ok(GdValue::Vector2i(
                x.max(min_x).min(max_x),
                y.max(min_y).min(max_y),
            ))
        }
        _ => Err(InterpError::name_error(
            format!("Vector2i has no method '{method}'"),
            0,
            0,
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vector2_properties() {
        let v = GdValue::Vector2(3.0, 4.0);
        assert_eq!(get_property(&v, "x").unwrap(), GdValue::Float(3.0));
        assert_eq!(get_property(&v, "y").unwrap(), GdValue::Float(4.0));
    }

    #[test]
    fn vector2i_properties() {
        let v = GdValue::Vector2i(3, 4);
        assert_eq!(get_property(&v, "x").unwrap(), GdValue::Int(3));
        assert_eq!(get_property(&v, "y").unwrap(), GdValue::Int(4));
    }

    #[test]
    fn vector2_length() {
        let v = GdValue::Vector2(3.0, 4.0);
        assert_eq!(call_method(&v, "length", &[]).unwrap(), GdValue::Float(5.0));
    }

    #[test]
    fn vector2_length_squared() {
        let v = GdValue::Vector2(3.0, 4.0);
        assert_eq!(
            call_method(&v, "length_squared", &[]).unwrap(),
            GdValue::Float(25.0)
        );
    }

    #[test]
    fn vector2_normalized() {
        let v = GdValue::Vector2(3.0, 4.0);
        let result = call_method(&v, "normalized", &[]).unwrap();
        if let GdValue::Vector2(nx, ny) = result {
            assert!((nx - 0.6).abs() < 1e-10);
            assert!((ny - 0.8).abs() < 1e-10);
        } else {
            panic!("expected Vector2");
        }
    }

    #[test]
    fn vector2_normalized_zero() {
        let v = GdValue::Vector2(0.0, 0.0);
        assert_eq!(
            call_method(&v, "normalized", &[]).unwrap(),
            GdValue::Vector2(0.0, 0.0)
        );
    }

    #[test]
    fn vector2_dot() {
        let v = GdValue::Vector2(1.0, 2.0);
        let o = GdValue::Vector2(3.0, 4.0);
        assert_eq!(call_method(&v, "dot", &[o]).unwrap(), GdValue::Float(11.0));
    }

    #[test]
    fn vector2_cross() {
        let v = GdValue::Vector2(1.0, 2.0);
        let o = GdValue::Vector2(3.0, 4.0);
        assert_eq!(
            call_method(&v, "cross", &[o]).unwrap(),
            GdValue::Float(-2.0)
        );
    }

    #[test]
    fn vector2_distance_to() {
        let v = GdValue::Vector2(0.0, 0.0);
        let o = GdValue::Vector2(3.0, 4.0);
        assert_eq!(
            call_method(&v, "distance_to", &[o]).unwrap(),
            GdValue::Float(5.0)
        );
    }

    #[test]
    fn vector2_angle() {
        let v = GdValue::Vector2(1.0, 0.0);
        assert_eq!(call_method(&v, "angle", &[]).unwrap(), GdValue::Float(0.0));
    }

    #[test]
    fn vector2_lerp() {
        let v = GdValue::Vector2(0.0, 0.0);
        let to = GdValue::Vector2(10.0, 20.0);
        let result = call_method(&v, "lerp", &[to, GdValue::Float(0.5)]).unwrap();
        assert_eq!(result, GdValue::Vector2(5.0, 10.0));
    }

    #[test]
    fn vector2_abs() {
        let v = GdValue::Vector2(-3.0, -4.0);
        assert_eq!(
            call_method(&v, "abs", &[]).unwrap(),
            GdValue::Vector2(3.0, 4.0)
        );
    }

    #[test]
    fn vector2_floor_ceil_round() {
        let v = GdValue::Vector2(1.7, 2.3);
        assert_eq!(
            call_method(&v, "floor", &[]).unwrap(),
            GdValue::Vector2(1.0, 2.0)
        );
        assert_eq!(
            call_method(&v, "ceil", &[]).unwrap(),
            GdValue::Vector2(2.0, 3.0)
        );
        assert_eq!(
            call_method(&v, "round", &[]).unwrap(),
            GdValue::Vector2(2.0, 2.0)
        );
    }

    #[test]
    fn vector2_clamp() {
        let v = GdValue::Vector2(5.0, -1.0);
        let min = GdValue::Vector2(0.0, 0.0);
        let max = GdValue::Vector2(3.0, 3.0);
        assert_eq!(
            call_method(&v, "clamp", &[min, max]).unwrap(),
            GdValue::Vector2(3.0, 0.0)
        );
    }

    #[test]
    fn vector2i_length() {
        let v = GdValue::Vector2i(3, 4);
        assert_eq!(call_method(&v, "length", &[]).unwrap(), GdValue::Float(5.0));
    }

    #[test]
    fn vector2i_length_squared() {
        let v = GdValue::Vector2i(3, 4);
        assert_eq!(
            call_method(&v, "length_squared", &[]).unwrap(),
            GdValue::Int(25)
        );
    }

    #[test]
    fn vector2i_dot() {
        let v = GdValue::Vector2i(1, 2);
        let o = GdValue::Vector2i(3, 4);
        assert_eq!(call_method(&v, "dot", &[o]).unwrap(), GdValue::Int(11));
    }

    #[test]
    fn vector2i_abs() {
        let v = GdValue::Vector2i(-3, -4);
        assert_eq!(
            call_method(&v, "abs", &[]).unwrap(),
            GdValue::Vector2i(3, 4)
        );
    }

    #[test]
    fn vector2_constant_properties() {
        let v = GdValue::Vector2(0.0, 0.0);
        assert_eq!(
            get_property(&v, "ZERO").unwrap(),
            GdValue::Vector2(0.0, 0.0)
        );
        assert_eq!(get_property(&v, "ONE").unwrap(), GdValue::Vector2(1.0, 1.0));
        assert_eq!(get_property(&v, "UP").unwrap(), GdValue::Vector2(0.0, -1.0));
    }

    #[test]
    fn unknown_property() {
        let v = GdValue::Vector2(0.0, 0.0);
        assert!(get_property(&v, "z").is_err());
    }

    #[test]
    fn unknown_method() {
        let v = GdValue::Vector2(0.0, 0.0);
        assert!(call_method(&v, "nonexistent", &[]).is_err());
    }
}
