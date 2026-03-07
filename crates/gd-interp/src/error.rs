use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    TypeError,
    NameError,
    ValueError,
    DivisionByZero,
    IndexOutOfBounds,
    KeyError,
    AssertionFailed,
    NotImplemented,
    ArgumentError,
    BreakOutsideLoop,
    ContinueOutsideLoop,
}

impl fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::TypeError => "Type error",
            Self::NameError => "Name error",
            Self::ValueError => "Value error",
            Self::DivisionByZero => "Division by zero",
            Self::IndexOutOfBounds => "Index out of bounds",
            Self::KeyError => "Key error",
            Self::AssertionFailed => "Assertion failed",
            Self::NotImplemented => "Not implemented",
            Self::ArgumentError => "Argument error",
            Self::BreakOutsideLoop => "Break outside loop",
            Self::ContinueOutsideLoop => "Continue outside loop",
        };
        f.write_str(s)
    }
}

#[derive(Debug, Clone)]
pub struct InterpError {
    pub kind: ErrorKind,
    pub message: String,
    pub line: usize,
    pub column: usize,
}

impl fmt::Display for InterpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}: {} (line {}, col {})",
            self.kind, self.message, self.line, self.column
        )
    }
}

impl std::error::Error for InterpError {}

impl InterpError {
    #[must_use]
    pub fn type_error(msg: impl Into<String>, line: usize, col: usize) -> Self {
        Self {
            kind: ErrorKind::TypeError,
            message: msg.into(),
            line,
            column: col,
        }
    }

    #[must_use]
    pub fn name_error(msg: impl Into<String>, line: usize, col: usize) -> Self {
        Self {
            kind: ErrorKind::NameError,
            message: msg.into(),
            line,
            column: col,
        }
    }

    #[must_use]
    pub fn value_error(msg: impl Into<String>, line: usize, col: usize) -> Self {
        Self {
            kind: ErrorKind::ValueError,
            message: msg.into(),
            line,
            column: col,
        }
    }

    #[must_use]
    pub fn division_by_zero(line: usize, col: usize) -> Self {
        Self {
            kind: ErrorKind::DivisionByZero,
            message: "Division by zero".to_owned(),
            line,
            column: col,
        }
    }

    #[must_use]
    pub fn index_out_of_bounds(index: i64, len: usize, line: usize, col: usize) -> Self {
        Self {
            kind: ErrorKind::IndexOutOfBounds,
            message: format!("Index {index} out of bounds for length {len}"),
            line,
            column: col,
        }
    }

    #[must_use]
    pub fn key_error(key: &str, line: usize, col: usize) -> Self {
        Self {
            kind: ErrorKind::KeyError,
            message: format!("Key not found: {key}"),
            line,
            column: col,
        }
    }

    #[must_use]
    pub fn assertion_failed(msg: impl Into<String>, line: usize, col: usize) -> Self {
        Self {
            kind: ErrorKind::AssertionFailed,
            message: msg.into(),
            line,
            column: col,
        }
    }

    #[must_use]
    pub fn not_implemented(feature: &str, line: usize, col: usize) -> Self {
        Self {
            kind: ErrorKind::NotImplemented,
            message: format!("Not implemented: {feature}"),
            line,
            column: col,
        }
    }

    #[must_use]
    pub fn argument_error(msg: impl Into<String>, line: usize, col: usize) -> Self {
        Self {
            kind: ErrorKind::ArgumentError,
            message: msg.into(),
            line,
            column: col,
        }
    }
}

pub type InterpResult<T> = Result<T, InterpError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_kind_display() {
        assert_eq!(ErrorKind::TypeError.to_string(), "Type error");
        assert_eq!(ErrorKind::NameError.to_string(), "Name error");
        assert_eq!(ErrorKind::DivisionByZero.to_string(), "Division by zero");
        assert_eq!(
            ErrorKind::ContinueOutsideLoop.to_string(),
            "Continue outside loop"
        );
    }

    #[test]
    fn interp_error_display() {
        let err = InterpError::type_error("expected int", 10, 5);
        assert_eq!(err.to_string(), "Type error: expected int (line 10, col 5)");
    }

    #[test]
    fn division_by_zero_display() {
        let err = InterpError::division_by_zero(3, 12);
        assert_eq!(
            err.to_string(),
            "Division by zero: Division by zero (line 3, col 12)"
        );
    }

    #[test]
    fn index_out_of_bounds_display() {
        let err = InterpError::index_out_of_bounds(5, 3, 7, 1);
        assert_eq!(
            err.to_string(),
            "Index out of bounds: Index 5 out of bounds for length 3 (line 7, col 1)"
        );
    }

    #[test]
    fn key_error_display() {
        let err = InterpError::key_error("missing", 1, 0);
        assert_eq!(
            err.to_string(),
            "Key error: Key not found: missing (line 1, col 0)"
        );
    }

    #[test]
    fn not_implemented_display() {
        let err = InterpError::not_implemented("lambdas", 2, 4);
        assert_eq!(
            err.to_string(),
            "Not implemented: Not implemented: lambdas (line 2, col 4)"
        );
    }

    #[test]
    fn error_kind_equality() {
        assert_eq!(ErrorKind::TypeError, ErrorKind::TypeError);
        assert_ne!(ErrorKind::TypeError, ErrorKind::NameError);
    }

    #[test]
    fn interp_result_ok() {
        let result: InterpResult<i32> = Ok(42);
        assert_eq!(result.unwrap(), 42);
    }

    #[test]
    fn interp_result_err() {
        let result: InterpResult<i32> = Err(InterpError::name_error("undefined x", 1, 0));
        let err = result.unwrap_err();
        assert_eq!(err.kind, ErrorKind::NameError);
        assert_eq!(err.message, "undefined x");
        assert_eq!(err.line, 1);
        assert_eq!(err.column, 0);
    }

    #[test]
    fn error_implements_std_error() {
        let err = InterpError::value_error("bad value", 1, 1);
        let std_err: &dyn std::error::Error = &err;
        assert!(std_err.source().is_none());
    }
}
