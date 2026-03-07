use std::collections::HashMap;

use gd_core::gd_ast::{GdDecl, GdFile, GdFunc};

use crate::env::Environment;
use crate::error::InterpResult;
use crate::eval::eval_expr;
use crate::value::GdValue;

/// Interpreter context: environment + function registry.
///
/// Holds both the variable scope stack and a lookup table of user-defined
/// functions from the current script file.
pub struct Interpreter<'a> {
    pub env: Environment,
    funcs: HashMap<&'a str, &'a GdFunc<'a>>,
}

impl<'a> Interpreter<'a> {
    #[must_use]
    pub fn new() -> Self {
        Self {
            env: Environment::new(),
            funcs: HashMap::new(),
        }
    }

    /// Build an interpreter from a parsed GDScript file.
    ///
    /// Registers all top-level functions, evaluates top-level var/const
    /// initializers, and defines enum values as constants.
    pub fn from_file(file: &'a GdFile<'a>) -> InterpResult<Self> {
        let mut interp = Self::new();
        for decl in &file.declarations {
            match decl {
                GdDecl::Func(f) => {
                    interp.register_func(f);
                }
                GdDecl::Var(v) => {
                    let val = match &v.value {
                        Some(expr) => eval_expr(expr, &mut interp)?,
                        None => GdValue::Null,
                    };
                    interp.env.define(v.name, val);
                }
                GdDecl::Enum(e) => {
                    // Define each enum member as an integer constant
                    let mut next_value: i64 = 0;
                    for member in &e.members {
                        let val = if let Some(expr) = &member.value {
                            let v = eval_expr(expr, &mut interp)?;
                            if let GdValue::Int(n) = v {
                                next_value = n + 1;
                                n
                            } else {
                                next_value += 1;
                                next_value - 1
                            }
                        } else {
                            let v = next_value;
                            next_value += 1;
                            v
                        };
                        interp.env.define(member.name, GdValue::Int(val));
                    }
                    // If the enum has a name, define it as a dictionary too
                    if !e.name.is_empty() {
                        let mut pairs = Vec::new();
                        let mut val: i64 = 0;
                        for member in &e.members {
                            if let Some(expr) = &member.value
                                && let Ok(GdValue::Int(n)) = eval_expr(expr, &mut interp)
                            {
                                val = n;
                            }
                            pairs.push((
                                GdValue::GdString(member.name.to_owned()),
                                GdValue::Int(val),
                            ));
                            val += 1;
                        }
                        interp.env.define(e.name, GdValue::Dictionary(pairs));
                    }
                }
                GdDecl::Signal(_) | GdDecl::Class(_) | GdDecl::Stmt(_) => {}
            }
        }
        Ok(interp)
    }

    /// Register a function so it can be called by name.
    pub fn register_func(&mut self, func: &'a GdFunc<'a>) {
        self.funcs.insert(func.name, func);
    }

    /// Look up a user-defined function by name.
    #[must_use]
    pub fn lookup_func(&self, name: &str) -> Option<&'a GdFunc<'a>> {
        self.funcs.get(name).copied()
    }

    /// Check if a name is a registered function.
    #[must_use]
    pub fn has_func(&self, name: &str) -> bool {
        self.funcs.contains_key(name)
    }
}

impl Default for Interpreter<'_> {
    fn default() -> Self {
        Self::new()
    }
}
