//! Global color control — respects `NO_COLOR` env and `--no-color` flag.
//!
//! Call [`init`] early in `main()`. After that, use [`cprintln!`] / [`ceprintln!`]
//! instead of `println!` / `eprintln!` for any output that may contain ANSI color codes.

use std::sync::atomic::{AtomicBool, Ordering};

/// When `true`, ANSI escape sequences are stripped from output.
static NO_COLOR: AtomicBool = AtomicBool::new(false);

/// Initialize color state from `--no-color` flag and `NO_COLOR` environment variable.
/// Call this once at startup before any output.
pub fn init(flag: bool) {
    let disabled = flag || std::env::var_os("NO_COLOR").is_some();
    NO_COLOR.store(disabled, Ordering::Relaxed);
}

/// Returns `true` if color output is disabled.
pub fn is_disabled() -> bool {
    NO_COLOR.load(Ordering::Relaxed)
}

/// Strip ANSI SGR escape sequences (`\x1b[...m`) from a string.
/// Only touches styling escapes — safe for all text content.
pub fn strip_ansi(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == 0x1b && i + 1 < bytes.len() && bytes[i + 1] == b'[' {
            // Skip past the 'm' terminator
            i += 2;
            while i < bytes.len() && bytes[i] != b'm' {
                i += 1;
            }
            if i < bytes.len() {
                i += 1; // skip 'm'
            }
        } else {
            out.push(bytes[i] as char);
            i += 1;
        }
    }
    out
}

/// Like `println!`, but strips ANSI color codes when `NO_COLOR` is active.
#[macro_export]
macro_rules! cprintln {
    () => { println!() };
    ($($arg:tt)*) => {{
        if $crate::core::color::is_disabled() {
            println!("{}", $crate::core::color::strip_ansi(&format!($($arg)*)));
        } else {
            println!($($arg)*);
        }
    }};
}

/// Like `eprintln!`, but strips ANSI color codes when `NO_COLOR` is active.
#[macro_export]
macro_rules! ceprintln {
    () => { eprintln!() };
    ($($arg:tt)*) => {{
        if $crate::core::color::is_disabled() {
            eprintln!("{}", $crate::core::color::strip_ansi(&format!($($arg)*)));
        } else {
            eprintln!($($arg)*);
        }
    }};
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_ansi_removes_colors() {
        assert_eq!(strip_ansi("\x1b[32mhello\x1b[39m"), "hello");
        assert_eq!(strip_ansi("\x1b[1;32mbold green\x1b[0m"), "bold green");
        assert_eq!(strip_ansi("no colors here"), "no colors here");
        assert_eq!(strip_ansi(""), "");
    }

    #[test]
    fn strip_ansi_preserves_content() {
        // File paths, JSON, etc. are never affected
        assert_eq!(strip_ansi("res://player.gd:42"), "res://player.gd:42");
        assert_eq!(strip_ansi("{\"key\": \"value\"}"), "{\"key\": \"value\"}");
    }

    #[test]
    fn strip_ansi_handles_nested() {
        // Bold + color
        assert_eq!(strip_ansi("\x1b[1m\x1b[36mtext\x1b[39m\x1b[22m"), "text");
    }
}
