use std::collections::HashMap;

use crate::value::GdValue;

pub struct Environment {
    frames: Vec<HashMap<String, GdValue>>,
    output: Vec<String>,
}

impl Environment {
    #[must_use]
    pub fn new() -> Self {
        Self {
            frames: vec![HashMap::new()],
            output: Vec::new(),
        }
    }

    pub fn push_frame(&mut self) {
        self.frames.push(HashMap::new());
    }

    pub fn pop_frame(&mut self) {
        assert!(self.frames.len() > 1, "cannot pop the global frame");
        self.frames.pop();
    }

    #[must_use]
    pub fn get(&self, name: &str) -> Option<&GdValue> {
        for frame in self.frames.iter().rev() {
            if let Some(val) = frame.get(name) {
                return Some(val);
            }
        }
        None
    }

    pub fn get_mut(&mut self, name: &str) -> Option<&mut GdValue> {
        for frame in self.frames.iter_mut().rev() {
            if let Some(val) = frame.get_mut(name) {
                return Some(val);
            }
        }
        None
    }

    pub fn set(&mut self, name: &str, val: GdValue) -> bool {
        for frame in self.frames.iter_mut().rev() {
            if frame.contains_key(name) {
                frame.insert(name.to_owned(), val);
                return true;
            }
        }
        false
    }

    pub fn define(&mut self, name: &str, val: GdValue) {
        if let Some(frame) = self.frames.last_mut() {
            frame.insert(name.to_owned(), val);
        }
    }

    #[must_use]
    pub fn has(&self, name: &str) -> bool {
        self.frames.iter().rev().any(|f| f.contains_key(name))
    }

    pub fn capture_output(&mut self, msg: String) {
        self.output.push(msg);
    }

    pub fn take_output(&mut self) -> Vec<String> {
        std::mem::take(&mut self.output)
    }

    #[must_use]
    pub fn output(&self) -> &[String] {
        &self.output
    }
}

impl Default for Environment {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn define_and_get() {
        let mut env = Environment::new();
        env.define("x", GdValue::Int(42));
        assert_eq!(env.get("x"), Some(&GdValue::Int(42)));
        assert!(env.has("x"));
        assert!(!env.has("y"));
    }

    #[test]
    fn scoping() {
        let mut env = Environment::new();
        env.define("outer", GdValue::Int(1));

        env.push_frame();
        env.define("inner", GdValue::Int(2));
        assert_eq!(env.get("inner"), Some(&GdValue::Int(2)));
        assert_eq!(env.get("outer"), Some(&GdValue::Int(1)));

        env.pop_frame();
        assert!(env.get("inner").is_none());
        assert_eq!(env.get("outer"), Some(&GdValue::Int(1)));
    }

    #[test]
    fn set_updates_nearest_frame() {
        let mut env = Environment::new();
        env.define("x", GdValue::Int(1));

        env.push_frame();
        env.define("x", GdValue::Int(2));

        env.push_frame();
        assert!(env.set("x", GdValue::Int(99)));
        // Should update the middle frame (nearest), not the global one
        assert_eq!(env.get("x"), Some(&GdValue::Int(99)));

        env.pop_frame();
        assert_eq!(env.get("x"), Some(&GdValue::Int(99)));

        env.pop_frame();
        // Global frame still has original value
        assert_eq!(env.get("x"), Some(&GdValue::Int(1)));
    }

    #[test]
    fn set_returns_false_when_not_found() {
        let mut env = Environment::new();
        assert!(!env.set("missing", GdValue::Int(0)));
    }

    #[test]
    fn output_capture_and_take() {
        let mut env = Environment::new();
        env.capture_output("hello".to_owned());
        env.capture_output("world".to_owned());
        assert_eq!(env.output(), &["hello", "world"]);

        let taken = env.take_output();
        assert_eq!(taken, vec!["hello", "world"]);
        assert!(env.output().is_empty());
    }

    #[test]
    #[should_panic(expected = "cannot pop the global frame")]
    fn pop_global_frame_panics() {
        let mut env = Environment::new();
        env.pop_frame();
    }
}
