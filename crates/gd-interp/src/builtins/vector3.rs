use crate::error::{InterpError, InterpResult};
use crate::value::GdValue;

pub fn get_property(receiver: &GdValue, property: &str) -> InterpResult<GdValue> {
    match receiver {
        GdValue::Vector3(x, y, z) => match property {
            "x" => Ok(GdValue::Float(*x)),
            "y" => Ok(GdValue::Float(*y)),
            "z" => Ok(GdValue::Float(*z)),
            _ => Err(InterpError::type_error(
                format!("Vector3 has no property '{property}'"),
                0,
                0,
            )),
        },
        GdValue::Vector3i(x, y, z) => match property {
            "x" => Ok(GdValue::Int(*x)),
            "y" => Ok(GdValue::Int(*y)),
            "z" => Ok(GdValue::Int(*z)),
            _ => Err(InterpError::type_error(
                format!("Vector3i has no property '{property}'"),
                0,
                0,
            )),
        },
        _ => Err(InterpError::type_error(
            format!("expected Vector3/Vector3i, got {}", receiver.type_name()),
            0,
            0,
        )),
    }
}

#[allow(clippy::too_many_lines)]
pub fn call_method(receiver: &GdValue, method: &str, args: &[GdValue]) -> InterpResult<GdValue> {
    match receiver {
        GdValue::Vector3(x, y, z) => call_vector3(*x, *y, *z, method, args),
        GdValue::Vector3i(x, y, z) => call_vector3i(*x, *y, *z, method, args),
        _ => Err(InterpError::type_error(
            format!("expected Vector3/Vector3i, got {}", receiver.type_name()),
            0,
            0,
        )),
    }
}

fn call_vector3(x: f64, y: f64, z: f64, method: &str, args: &[GdValue]) -> InterpResult<GdValue> {
    match method {
        "length" => Ok(GdValue::Float((x * x + y * y + z * z).sqrt())),
        "length_squared" => Ok(GdValue::Float(x * x + y * y + z * z)),
        "normalized" => {
            let len = (x * x + y * y + z * z).sqrt();
            if len == 0.0 {
                Ok(GdValue::Vector3(0.0, 0.0, 0.0))
            } else {
                Ok(GdValue::Vector3(x / len, y / len, z / len))
            }
        }
        "dot" => {
            expect_vec3_arg(method, args)?;
            if let GdValue::Vector3(x2, y2, z2) = &args[0] {
                Ok(GdValue::Float(x * x2 + y * y2 + z * z2))
            } else {
                Err(InterpError::type_error("dot() requires Vector3", 0, 0))
            }
        }
        "cross" => {
            expect_vec3_arg(method, args)?;
            if let GdValue::Vector3(x2, y2, z2) = &args[0] {
                Ok(GdValue::Vector3(
                    y * z2 - z * y2,
                    z * x2 - x * z2,
                    x * y2 - y * x2,
                ))
            } else {
                Err(InterpError::type_error("cross() requires Vector3", 0, 0))
            }
        }
        "distance_to" => {
            expect_vec3_arg(method, args)?;
            if let GdValue::Vector3(x2, y2, z2) = &args[0] {
                let dx = x - x2;
                let dy = y - y2;
                let dz = z - z2;
                Ok(GdValue::Float((dx * dx + dy * dy + dz * dz).sqrt()))
            } else {
                Err(InterpError::type_error(
                    "distance_to() requires Vector3",
                    0,
                    0,
                ))
            }
        }
        "distance_squared_to" => {
            expect_vec3_arg(method, args)?;
            if let GdValue::Vector3(x2, y2, z2) = &args[0] {
                let dx = x - x2;
                let dy = y - y2;
                let dz = z - z2;
                Ok(GdValue::Float(dx * dx + dy * dy + dz * dz))
            } else {
                Err(InterpError::type_error(
                    "distance_squared_to() requires Vector3",
                    0,
                    0,
                ))
            }
        }
        "lerp" => {
            if args.len() != 2 {
                return Err(InterpError::argument_error(
                    "lerp() takes 2 arguments",
                    0,
                    0,
                ));
            }
            if let (GdValue::Vector3(x2, y2, z2), GdValue::Float(t)) = (&args[0], &args[1]) {
                Ok(GdValue::Vector3(
                    x + (x2 - x) * t,
                    y + (y2 - y) * t,
                    z + (z2 - z) * t,
                ))
            } else {
                Err(InterpError::type_error(
                    "lerp() requires (Vector3, float)",
                    0,
                    0,
                ))
            }
        }
        "abs" => Ok(GdValue::Vector3(x.abs(), y.abs(), z.abs())),
        "sign" => Ok(GdValue::Vector3(x.signum(), y.signum(), z.signum())),
        "floor" => Ok(GdValue::Vector3(x.floor(), y.floor(), z.floor())),
        "ceil" => Ok(GdValue::Vector3(x.ceil(), y.ceil(), z.ceil())),
        "round" => Ok(GdValue::Vector3(x.round(), y.round(), z.round())),
        "clamp" => {
            if args.len() != 2 {
                return Err(InterpError::argument_error(
                    "clamp() takes 2 arguments",
                    0,
                    0,
                ));
            }
            if let (GdValue::Vector3(min_x, min_y, min_z), GdValue::Vector3(max_x, max_y, max_z)) =
                (&args[0], &args[1])
            {
                Ok(GdValue::Vector3(
                    x.clamp(*min_x, *max_x),
                    y.clamp(*min_y, *max_y),
                    z.clamp(*min_z, *max_z),
                ))
            } else {
                Err(InterpError::type_error(
                    "clamp() requires (Vector3, Vector3)",
                    0,
                    0,
                ))
            }
        }
        _ => Err(InterpError::type_error(
            format!("Vector3 has no method '{method}'"),
            0,
            0,
        )),
    }
}

fn call_vector3i(x: i64, y: i64, z: i64, method: &str, args: &[GdValue]) -> InterpResult<GdValue> {
    match method {
        "length" => Ok(GdValue::Float(((x * x + y * y + z * z) as f64).sqrt())),
        "length_squared" => Ok(GdValue::Int(x * x + y * y + z * z)),
        "abs" => Ok(GdValue::Vector3i(x.abs(), y.abs(), z.abs())),
        "sign" => Ok(GdValue::Vector3i(x.signum(), y.signum(), z.signum())),
        "clamp" => {
            if args.len() != 2 {
                return Err(InterpError::argument_error(
                    "clamp() takes 2 arguments",
                    0,
                    0,
                ));
            }
            if let (
                GdValue::Vector3i(min_x, min_y, min_z),
                GdValue::Vector3i(max_x, max_y, max_z),
            ) = (&args[0], &args[1])
            {
                Ok(GdValue::Vector3i(
                    x.clamp(*min_x, *max_x),
                    y.clamp(*min_y, *max_y),
                    z.clamp(*min_z, *max_z),
                ))
            } else {
                Err(InterpError::type_error(
                    "clamp() requires (Vector3i, Vector3i)",
                    0,
                    0,
                ))
            }
        }
        _ => Err(InterpError::type_error(
            format!("Vector3i has no method '{method}'"),
            0,
            0,
        )),
    }
}

fn expect_vec3_arg(method: &str, args: &[GdValue]) -> InterpResult<()> {
    if args.len() != 1 {
        return Err(InterpError::argument_error(
            format!("{method}() takes 1 argument, got {}", args.len()),
            0,
            0,
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vector3_length() {
        let v = GdValue::Vector3(3.0, 4.0, 0.0);
        let result = call_method(&v, "length", &[]).unwrap();
        assert_eq!(result, GdValue::Float(5.0));
    }

    #[test]
    fn test_vector3_dot() {
        let v = GdValue::Vector3(1.0, 2.0, 3.0);
        let other = GdValue::Vector3(4.0, 5.0, 6.0);
        let result = call_method(&v, "dot", &[other]).unwrap();
        assert_eq!(result, GdValue::Float(32.0));
    }

    #[test]
    fn test_vector3_cross() {
        let v = GdValue::Vector3(1.0, 0.0, 0.0);
        let other = GdValue::Vector3(0.0, 1.0, 0.0);
        let result = call_method(&v, "cross", &[other]).unwrap();
        assert_eq!(result, GdValue::Vector3(0.0, 0.0, 1.0));
    }

    #[test]
    fn test_vector3_properties() {
        let v = GdValue::Vector3(1.0, 2.0, 3.0);
        assert_eq!(get_property(&v, "x").unwrap(), GdValue::Float(1.0));
        assert_eq!(get_property(&v, "y").unwrap(), GdValue::Float(2.0));
        assert_eq!(get_property(&v, "z").unwrap(), GdValue::Float(3.0));
    }

    #[test]
    fn test_vector3i_properties() {
        let v = GdValue::Vector3i(1, 2, 3);
        assert_eq!(get_property(&v, "x").unwrap(), GdValue::Int(1));
        assert_eq!(get_property(&v, "y").unwrap(), GdValue::Int(2));
        assert_eq!(get_property(&v, "z").unwrap(), GdValue::Int(3));
    }

    #[test]
    fn test_vector3_abs() {
        let v = GdValue::Vector3(-1.0, -2.0, 3.0);
        let result = call_method(&v, "abs", &[]).unwrap();
        assert_eq!(result, GdValue::Vector3(1.0, 2.0, 3.0));
    }
}
