use crate::error::{InterpError, InterpResult};
use crate::value::GdValue;

pub fn get_property(receiver: &GdValue, property: &str) -> InterpResult<GdValue> {
    let GdValue::Color(r, g, b, a) = receiver else {
        return Err(InterpError::type_error(
            format!("expected Color, got {}", receiver.type_name()),
            0,
            0,
        ));
    };

    match property {
        "r" => Ok(GdValue::Float(*r)),
        "g" => Ok(GdValue::Float(*g)),
        "b" => Ok(GdValue::Float(*b)),
        "a" => Ok(GdValue::Float(*a)),
        "r8" => Ok(GdValue::Int((r * 255.0).round() as i64)),
        "g8" => Ok(GdValue::Int((g * 255.0).round() as i64)),
        "b8" => Ok(GdValue::Int((b * 255.0).round() as i64)),
        "a8" => Ok(GdValue::Int((a * 255.0).round() as i64)),
        "h" => {
            let (h, _, _) = rgb_to_hsv(*r, *g, *b);
            Ok(GdValue::Float(h))
        }
        "s" => {
            let (_, s, _) = rgb_to_hsv(*r, *g, *b);
            Ok(GdValue::Float(s))
        }
        "v" => {
            let (_, _, v) = rgb_to_hsv(*r, *g, *b);
            Ok(GdValue::Float(v))
        }
        _ => Err(InterpError::type_error(
            format!("Color has no property '{property}'"),
            0,
            0,
        )),
    }
}

pub fn call_method(receiver: &GdValue, method: &str, args: &[GdValue]) -> InterpResult<GdValue> {
    let GdValue::Color(r, g, b, a) = receiver else {
        return Err(InterpError::type_error(
            format!("expected Color, got {}", receiver.type_name()),
            0,
            0,
        ));
    };

    match method {
        "lerp" => {
            if args.len() != 2 {
                return Err(InterpError::argument_error(
                    "lerp() takes 2 arguments",
                    0,
                    0,
                ));
            }
            if let (GdValue::Color(r2, g2, b2, a2), GdValue::Float(t)) = (&args[0], &args[1]) {
                Ok(GdValue::Color(
                    r + (r2 - r) * t,
                    g + (g2 - g) * t,
                    b + (b2 - b) * t,
                    a + (a2 - a) * t,
                ))
            } else {
                Err(InterpError::type_error(
                    "lerp() requires (Color, float)",
                    0,
                    0,
                ))
            }
        }
        "lightened" => {
            if args.len() != 1 {
                return Err(InterpError::argument_error(
                    "lightened() takes 1 argument",
                    0,
                    0,
                ));
            }
            let GdValue::Float(amount) = &args[0] else {
                return Err(InterpError::type_error("lightened() requires float", 0, 0));
            };
            Ok(GdValue::Color(
                r + (1.0 - r) * amount,
                g + (1.0 - g) * amount,
                b + (1.0 - b) * amount,
                *a,
            ))
        }
        "darkened" => {
            if args.len() != 1 {
                return Err(InterpError::argument_error(
                    "darkened() takes 1 argument",
                    0,
                    0,
                ));
            }
            let GdValue::Float(amount) = &args[0] else {
                return Err(InterpError::type_error("darkened() requires float", 0, 0));
            };
            Ok(GdValue::Color(
                r * (1.0 - amount),
                g * (1.0 - amount),
                b * (1.0 - amount),
                *a,
            ))
        }
        "inverted" => Ok(GdValue::Color(1.0 - r, 1.0 - g, 1.0 - b, *a)),
        "luminance" => Ok(GdValue::Float(0.2126 * r + 0.7152 * g + 0.0722 * b)),
        "to_html" => {
            let with_alpha = match args.first() {
                Some(GdValue::Bool(b)) => *b,
                _ => true,
            };
            let ri = (r * 255.0).round() as u8;
            let gi = (g * 255.0).round() as u8;
            let bi = (b * 255.0).round() as u8;
            if with_alpha {
                let ai = (a * 255.0).round() as u8;
                Ok(GdValue::GdString(format!(
                    "{ri:02x}{gi:02x}{bi:02x}{ai:02x}"
                )))
            } else {
                Ok(GdValue::GdString(format!("{ri:02x}{gi:02x}{bi:02x}")))
            }
        }
        _ => Err(InterpError::type_error(
            format!("Color has no method '{method}'"),
            0,
            0,
        )),
    }
}

fn rgb_to_hsv(r: f64, g: f64, b: f64) -> (f64, f64, f64) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;

    let v = max;
    let s = if max == 0.0 { 0.0 } else { delta / max };
    let h = if delta == 0.0 {
        0.0
    } else if (max - r).abs() < f64::EPSILON {
        ((g - b) / delta).rem_euclid(6.0) / 6.0
    } else if (max - g).abs() < f64::EPSILON {
        ((b - r) / delta + 2.0) / 6.0
    } else {
        ((r - g) / delta + 4.0) / 6.0
    };

    (h, s, v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_color_properties() {
        let c = GdValue::Color(1.0, 0.5, 0.25, 0.8);
        assert_eq!(get_property(&c, "r").unwrap(), GdValue::Float(1.0));
        assert_eq!(get_property(&c, "g").unwrap(), GdValue::Float(0.5));
        assert_eq!(get_property(&c, "b").unwrap(), GdValue::Float(0.25));
        assert_eq!(get_property(&c, "a").unwrap(), GdValue::Float(0.8));
    }

    #[test]
    fn test_color_r8() {
        let c = GdValue::Color(1.0, 0.5, 0.0, 1.0);
        assert_eq!(get_property(&c, "r8").unwrap(), GdValue::Int(255));
        assert_eq!(get_property(&c, "g8").unwrap(), GdValue::Int(128));
        assert_eq!(get_property(&c, "b8").unwrap(), GdValue::Int(0));
    }

    #[test]
    fn test_color_inverted() {
        let c = GdValue::Color(1.0, 0.0, 0.5, 1.0);
        let result = call_method(&c, "inverted", &[]).unwrap();
        assert_eq!(result, GdValue::Color(0.0, 1.0, 0.5, 1.0));
    }

    #[test]
    fn test_color_to_html() {
        let c = GdValue::Color(1.0, 0.0, 0.0, 1.0);
        let result = call_method(&c, "to_html", &[GdValue::Bool(false)]).unwrap();
        assert_eq!(result, GdValue::GdString("ff0000".into()));
    }

    #[test]
    fn test_color_luminance() {
        let white = GdValue::Color(1.0, 1.0, 1.0, 1.0);
        let lum = call_method(&white, "luminance", &[]).unwrap();
        if let GdValue::Float(l) = lum {
            assert!((l - 1.0).abs() < 0.001);
        } else {
            panic!("expected float");
        }
    }
}
