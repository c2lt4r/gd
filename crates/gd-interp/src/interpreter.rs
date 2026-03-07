use std::collections::HashMap;

use gd_core::gd_ast::{GdClass, GdDecl, GdFile, GdFunc, GdVar};

use crate::env::Environment;
use crate::error::InterpResult;
use crate::eval::eval_expr;
use crate::value::GdValue;

/// A registered class definition (either file-level or inner class).
pub struct ClassDef<'a> {
    pub name: String,
    pub extends: Option<String>,
    pub vars: Vec<&'a GdVar<'a>>,
    pub funcs: HashMap<&'a str, &'a GdFunc<'a>>,
}

/// Interpreter context: environment + function registry + class registry.
///
/// Holds the variable scope stack, a lookup table of user-defined
/// functions from the current script file, and registered class definitions.
pub struct Interpreter<'a> {
    pub env: Environment,
    funcs: HashMap<&'a str, &'a GdFunc<'a>>,
    classes: HashMap<String, ClassDef<'a>>,
    /// The class name of the file-level script (if any).
    file_class: Option<String>,
}

impl<'a> Interpreter<'a> {
    #[must_use]
    pub fn new() -> Self {
        Self {
            env: Environment::new(),
            funcs: HashMap::new(),
            classes: HashMap::new(),
            file_class: None,
        }
    }

    /// Build an interpreter from a parsed GDScript file.
    ///
    /// Registers all top-level functions, evaluates top-level var/const
    /// initializers, defines enum values as constants, and registers
    /// the file-level class and any inner classes.
    pub fn from_file(file: &'a GdFile<'a>) -> InterpResult<Self> {
        let mut interp = Self::new();

        // Determine the file-level class name (defaults to script filename convention)
        let class_name = file
            .class_name
            .map_or_else(|| "__FileClass__".to_owned(), String::from);
        interp.file_class = Some(class_name.clone());

        // Collect file-level vars and funcs for the class definition
        let mut class_vars: Vec<&'a GdVar<'a>> = Vec::new();
        let mut class_funcs: HashMap<&'a str, &'a GdFunc<'a>> = HashMap::new();

        for decl in &file.declarations {
            match decl {
                GdDecl::Func(f) => {
                    interp.register_func(f);
                    class_funcs.insert(f.name, f);
                }
                GdDecl::Var(v) => {
                    let val = match &v.value {
                        Some(expr) => eval_expr(expr, &mut interp)?,
                        None => GdValue::Null,
                    };
                    interp.env.define(v.name, val);
                    class_vars.push(v);
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
                GdDecl::Class(inner) => {
                    interp.register_class(inner);
                }
                GdDecl::Signal(_) | GdDecl::Stmt(_) => {}
            }
        }

        // Register the file-level class
        let extends = file.extends.as_ref().map(|e| match e {
            gd_core::gd_ast::GdExtends::Class(name) => (*name).to_owned(),
            gd_core::gd_ast::GdExtends::Path(path) => (*path).to_owned(),
        });
        interp.classes.insert(
            class_name,
            ClassDef {
                name: interp.file_class.clone().unwrap_or_default(),
                extends,
                vars: class_vars,
                funcs: class_funcs,
            },
        );

        Ok(interp)
    }

    /// Register an inner class from the AST.
    fn register_class(&mut self, class: &'a GdClass<'a>) {
        let mut vars = Vec::new();
        let mut funcs = HashMap::new();

        for decl in &class.declarations {
            match decl {
                GdDecl::Func(f) => {
                    funcs.insert(f.name, f);
                }
                GdDecl::Var(v) => {
                    vars.push(v);
                }
                // Inner classes can have enums, signals, etc. — skip for now
                _ => {}
            }
        }

        let extends = class.extends.as_ref().map(|e| match e {
            gd_core::gd_ast::GdExtends::Class(name) => (*name).to_owned(),
            gd_core::gd_ast::GdExtends::Path(path) => (*path).to_owned(),
        });

        self.classes.insert(
            class.name.to_owned(),
            ClassDef {
                name: class.name.to_owned(),
                extends,
                vars,
                funcs,
            },
        );
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

    /// Look up a method on a class by class name.
    #[must_use]
    pub fn lookup_method(&self, class_name: &str, method_name: &str) -> Option<&'a GdFunc<'a>> {
        let class = self.classes.get(class_name)?;
        if let Some(func) = class.funcs.get(method_name) {
            return Some(*func);
        }
        // Walk the extends chain
        if let Some(ref parent) = class.extends {
            return self.lookup_method(parent, method_name);
        }
        None
    }

    /// Look up a registered class definition.
    #[must_use]
    pub fn lookup_class(&self, name: &str) -> Option<&ClassDef<'a>> {
        self.classes.get(name)
    }

    /// Check if a name is a registered function.
    #[must_use]
    pub fn has_func(&self, name: &str) -> bool {
        self.funcs.contains_key(name)
    }

    /// Check if a name is a registered class.
    #[must_use]
    pub fn has_class(&self, name: &str) -> bool {
        self.classes.contains_key(name)
    }

    /// Get the file-level class name.
    #[must_use]
    pub fn file_class(&self) -> Option<&str> {
        self.file_class.as_deref()
    }
}

impl Default for Interpreter<'_> {
    fn default() -> Self {
        Self::new()
    }
}
