use gd_core::gd_ast::{GdExpr, GdFunc, GdStmt};

use crate::error::{InterpError, InterpResult};
use crate::eval::eval_expr;
use crate::interpreter::Interpreter;
use crate::value::GdValue;

/// Control flow signal from statement execution.
#[derive(Debug)]
pub enum ControlFlow {
    /// Normal execution continues.
    None,
    /// A `return` statement was hit.
    Return(GdValue),
    /// A `break` statement was hit.
    Break,
    /// A `continue` statement was hit.
    Continue,
}

/// Execute a sequence of statements, propagating control flow.
pub fn exec_body(stmts: &[GdStmt<'_>], interp: &mut Interpreter<'_>) -> InterpResult<ControlFlow> {
    for stmt in stmts {
        let flow = exec_stmt(stmt, interp)?;
        match flow {
            ControlFlow::None => {}
            other => return Ok(other),
        }
    }
    Ok(ControlFlow::None)
}

/// Execute a single statement.
#[allow(clippy::too_many_lines)]
pub fn exec_stmt(stmt: &GdStmt<'_>, interp: &mut Interpreter<'_>) -> InterpResult<ControlFlow> {
    match stmt {
        GdStmt::Expr { expr, .. } => {
            eval_expr(expr, interp)?;
            Ok(ControlFlow::None)
        }

        GdStmt::Var(v) => {
            let val = match &v.value {
                Some(expr) => eval_expr(expr, interp)?,
                None => GdValue::Null,
            };
            interp.env.define(v.name, val);
            Ok(ControlFlow::None)
        }

        GdStmt::Assign { target, value, .. } => {
            let val = eval_expr(value, interp)?;
            exec_assign(target, val, interp)?;
            Ok(ControlFlow::None)
        }

        GdStmt::AugAssign {
            target, op, value, ..
        } => {
            let current = eval_expr(target, interp)?;
            let rhs = eval_expr(value, interp)?;
            let new_val = apply_aug_op(&current, op, &rhs, target)?;
            exec_assign(target, new_val, interp)?;
            Ok(ControlFlow::None)
        }

        GdStmt::Return { value, .. } => {
            let val = match value {
                Some(expr) => eval_expr(expr, interp)?,
                None => GdValue::Null,
            };
            Ok(ControlFlow::Return(val))
        }

        GdStmt::If(if_stmt) => {
            let cond = eval_expr(&if_stmt.condition, interp)?;
            if cond.is_truthy() {
                return exec_body(&if_stmt.body, interp);
            }

            for (elif_cond, elif_body) in &if_stmt.elif_branches {
                let cond = eval_expr(elif_cond, interp)?;
                if cond.is_truthy() {
                    return exec_body(elif_body, interp);
                }
            }

            if let Some(else_body) = &if_stmt.else_body {
                return exec_body(else_body, interp);
            }

            Ok(ControlFlow::None)
        }

        GdStmt::For {
            var, iter, body, ..
        } => exec_for(var, iter, body, interp),

        GdStmt::While {
            condition, body, ..
        } => exec_while(condition, body, interp),

        GdStmt::Match { value, arms, .. } => {
            let val = eval_expr(value, interp)?;
            for arm in arms {
                if arm_matches(&val, &arm.patterns, interp)? {
                    if let Some(guard) = &arm.guard {
                        let guard_val = eval_expr(guard, interp)?;
                        if !guard_val.is_truthy() {
                            continue;
                        }
                    }
                    return exec_body(&arm.body, interp);
                }
            }
            Ok(ControlFlow::None)
        }

        GdStmt::Pass { .. } | GdStmt::Breakpoint { .. } | GdStmt::Invalid { .. } => {
            Ok(ControlFlow::None)
        }

        GdStmt::Break { .. } => Ok(ControlFlow::Break),
        GdStmt::Continue { .. } => Ok(ControlFlow::Continue),
    }
}

/// Execute a function: push frame, bind params, run body, pop frame.
pub fn exec_func(
    func: &GdFunc<'_>,
    args: &[GdValue],
    interp: &mut Interpreter<'_>,
) -> InterpResult<GdValue> {
    interp.env.push_frame();

    // Bind parameters
    for (i, param) in func.params.iter().enumerate() {
        let val = if i < args.len() {
            args[i].clone()
        } else if let Some(default) = &param.default {
            eval_expr(default, interp)?
        } else {
            GdValue::Null
        };
        interp.env.define(param.name, val);
    }

    let flow = exec_body(&func.body, interp);
    interp.env.pop_frame();

    match flow? {
        ControlFlow::Return(val) => Ok(val),
        _ => Ok(GdValue::Null),
    }
}

fn exec_assign(
    target: &GdExpr<'_>,
    val: GdValue,
    interp: &mut Interpreter<'_>,
) -> InterpResult<()> {
    match target {
        GdExpr::Ident { name, .. } => {
            if !interp.env.set(name, val.clone()) {
                interp.env.define(name, val);
            }
            Ok(())
        }
        // self.property = val (or obj.property = val)
        GdExpr::PropertyAccess {
            receiver, property, ..
        } => {
            if let GdExpr::Ident { name, .. } = receiver.as_ref()
                && let Some(GdValue::Object(_)) = interp.env.get(name)
            {
                let recv = interp.env.get_mut(name).expect("just checked");
                if let GdValue::Object(obj) = recv {
                    obj.properties.insert((*property).to_owned(), val);
                    return Ok(());
                }
            }
            Err(InterpError::not_implemented(
                "property assignment on non-object",
                target.line(),
                target.column(),
            ))
        }
        GdExpr::Subscript {
            receiver, index, ..
        } => {
            let idx = eval_expr(index, interp)?;
            // Get the receiver name for reassignment
            if let GdExpr::Ident { name, .. } = receiver.as_ref() {
                let mut recv = interp.env.get(name).cloned().ok_or_else(|| {
                    InterpError::name_error(
                        format!("undefined: {name}"),
                        target.line(),
                        target.column(),
                    )
                })?;
                match &mut recv {
                    GdValue::Array(arr) => {
                        let GdValue::Int(i) = idx else {
                            return Err(InterpError::type_error(
                                "array index must be int",
                                target.line(),
                                target.column(),
                            ));
                        };
                        let index = if i < 0 {
                            (arr.len() as i64 + i) as usize
                        } else {
                            i as usize
                        };
                        if index >= arr.len() {
                            return Err(InterpError::index_out_of_bounds(
                                i,
                                arr.len(),
                                target.line(),
                                target.column(),
                            ));
                        }
                        arr[index] = val;
                    }
                    GdValue::Dictionary(pairs) => {
                        for (k, v) in pairs.iter_mut() {
                            if *k == idx {
                                *v = val;
                                interp.env.set(name, recv);
                                return Ok(());
                            }
                        }
                        pairs.push((idx, val));
                    }
                    _ => {
                        return Err(InterpError::type_error(
                            "subscript assignment requires array or dict",
                            target.line(),
                            target.column(),
                        ));
                    }
                }
                interp.env.set(name, recv);
                Ok(())
            } else {
                Err(InterpError::not_implemented(
                    "complex subscript assignment",
                    target.line(),
                    target.column(),
                ))
            }
        }
        _ => Err(InterpError::not_implemented(
            "complex assignment target",
            target.line(),
            target.column(),
        )),
    }
}

fn apply_aug_op(
    current: &GdValue,
    op: &str,
    rhs: &GdValue,
    target: &GdExpr<'_>,
) -> InterpResult<GdValue> {
    let line = target.line();
    let col = target.column();
    match op {
        "+=" => eval_add_values(current, rhs, line, col),
        "-=" => eval_arith_values(current, rhs, line, col, |a, b| a - b, |a, b| a - b),
        "*=" => eval_arith_values(current, rhs, line, col, |a, b| a * b, |a, b| a * b),
        "/=" => eval_div_values(current, rhs, line, col),
        "%=" => eval_mod_values(current, rhs, line, col),
        "**=" => eval_pow_values(current, rhs, line, col),
        "&=" => match (current, rhs) {
            (GdValue::Int(a), GdValue::Int(b)) => Ok(GdValue::Int(a & b)),
            _ => Err(InterpError::type_error(
                "'&=' requires int operands",
                line,
                col,
            )),
        },
        "|=" => match (current, rhs) {
            (GdValue::Int(a), GdValue::Int(b)) => Ok(GdValue::Int(a | b)),
            _ => Err(InterpError::type_error(
                "'|=' requires int operands",
                line,
                col,
            )),
        },
        "^=" => match (current, rhs) {
            (GdValue::Int(a), GdValue::Int(b)) => Ok(GdValue::Int(a ^ b)),
            _ => Err(InterpError::type_error(
                "'^=' requires int operands",
                line,
                col,
            )),
        },
        "<<=" => match (current, rhs) {
            (GdValue::Int(a), GdValue::Int(b)) => Ok(GdValue::Int(a << b)),
            _ => Err(InterpError::type_error(
                "'<<=' requires int operands",
                line,
                col,
            )),
        },
        ">>=" => match (current, rhs) {
            (GdValue::Int(a), GdValue::Int(b)) => Ok(GdValue::Int(a >> b)),
            _ => Err(InterpError::type_error(
                "'>>=' requires int operands",
                line,
                col,
            )),
        },
        _ => Err(InterpError::not_implemented(
            &format!("augmented assignment '{op}'"),
            line,
            col,
        )),
    }
}

fn eval_add_values(l: &GdValue, r: &GdValue, line: usize, col: usize) -> InterpResult<GdValue> {
    match (l, r) {
        (GdValue::Int(a), GdValue::Int(b)) => Ok(GdValue::Int(a.wrapping_add(*b))),
        (GdValue::Float(a), GdValue::Float(b)) => Ok(GdValue::Float(a + b)),
        (GdValue::Int(a), GdValue::Float(b)) => Ok(GdValue::Float(*a as f64 + b)),
        (GdValue::Float(a), GdValue::Int(b)) => Ok(GdValue::Float(a + *b as f64)),
        (GdValue::GdString(a), GdValue::GdString(b)) => Ok(GdValue::GdString(format!("{a}{b}"))),
        _ => Err(InterpError::type_error(
            format!(
                "'+' not supported between {} and {}",
                l.type_name(),
                r.type_name()
            ),
            line,
            col,
        )),
    }
}

fn eval_arith_values(
    l: &GdValue,
    r: &GdValue,
    line: usize,
    col: usize,
    int_op: fn(i64, i64) -> i64,
    float_op: fn(f64, f64) -> f64,
) -> InterpResult<GdValue> {
    match (l, r) {
        (GdValue::Int(a), GdValue::Int(b)) => Ok(GdValue::Int(int_op(*a, *b))),
        (GdValue::Float(a), GdValue::Float(b)) => Ok(GdValue::Float(float_op(*a, *b))),
        (GdValue::Int(a), GdValue::Float(b)) => Ok(GdValue::Float(float_op(*a as f64, *b))),
        (GdValue::Float(a), GdValue::Int(b)) => Ok(GdValue::Float(float_op(*a, *b as f64))),
        _ => Err(InterpError::type_error(
            format!(
                "arithmetic not supported between {} and {}",
                l.type_name(),
                r.type_name()
            ),
            line,
            col,
        )),
    }
}

fn eval_div_values(l: &GdValue, r: &GdValue, line: usize, col: usize) -> InterpResult<GdValue> {
    match (l, r) {
        (GdValue::Int(_), GdValue::Int(0)) => Err(InterpError::division_by_zero(line, col)),
        (GdValue::Int(a), GdValue::Int(b)) => Ok(GdValue::Int(a / b)),
        (GdValue::Float(a), GdValue::Float(b)) => Ok(GdValue::Float(a / b)),
        (GdValue::Int(a), GdValue::Float(b)) => Ok(GdValue::Float(*a as f64 / b)),
        (GdValue::Float(a), GdValue::Int(b)) => Ok(GdValue::Float(a / *b as f64)),
        _ => Err(InterpError::type_error(
            format!(
                "'/' not supported between {} and {}",
                l.type_name(),
                r.type_name()
            ),
            line,
            col,
        )),
    }
}

fn eval_mod_values(l: &GdValue, r: &GdValue, line: usize, col: usize) -> InterpResult<GdValue> {
    match (l, r) {
        (GdValue::Int(_), GdValue::Int(0)) => Err(InterpError::division_by_zero(line, col)),
        (GdValue::Int(a), GdValue::Int(b)) => Ok(GdValue::Int(a % b)),
        (GdValue::Float(a), GdValue::Float(b)) => Ok(GdValue::Float(a % b)),
        (GdValue::Int(a), GdValue::Float(b)) => Ok(GdValue::Float(*a as f64 % b)),
        (GdValue::Float(a), GdValue::Int(b)) => Ok(GdValue::Float(a % *b as f64)),
        _ => Err(InterpError::type_error(
            format!(
                "'%' not supported between {} and {}",
                l.type_name(),
                r.type_name()
            ),
            line,
            col,
        )),
    }
}

fn eval_pow_values(l: &GdValue, r: &GdValue, line: usize, col: usize) -> InterpResult<GdValue> {
    match (l, r) {
        (GdValue::Int(base), GdValue::Int(exp)) => {
            if *exp >= 0 {
                Ok(GdValue::Int(base.wrapping_pow(*exp as u32)))
            } else {
                Ok(GdValue::Float((*base as f64).powf(*exp as f64)))
            }
        }
        (GdValue::Float(a), GdValue::Float(b)) => Ok(GdValue::Float(a.powf(*b))),
        (GdValue::Int(a), GdValue::Float(b)) => Ok(GdValue::Float((*a as f64).powf(*b))),
        (GdValue::Float(a), GdValue::Int(b)) => Ok(GdValue::Float(a.powf(*b as f64))),
        _ => Err(InterpError::type_error(
            format!(
                "'**' not supported between {} and {}",
                l.type_name(),
                r.type_name()
            ),
            line,
            col,
        )),
    }
}

fn exec_for(
    var: &str,
    iter: &GdExpr<'_>,
    body: &[GdStmt<'_>],
    interp: &mut Interpreter<'_>,
) -> InterpResult<ControlFlow> {
    let iter_val = eval_expr(iter, interp)?;

    let items: Vec<GdValue> = match iter_val {
        GdValue::Array(arr) => arr,
        GdValue::Int(n) => {
            // range sugar: `for i in 10` means `for i in range(10)`
            (0..n).map(GdValue::Int).collect()
        }
        _ => {
            return Err(InterpError::type_error(
                format!("cannot iterate over {}", iter_val.type_name()),
                iter.line(),
                iter.column(),
            ));
        }
    };

    for item in items {
        interp.env.define(var, item);
        match exec_body(body, interp)? {
            ControlFlow::Break => break,
            ControlFlow::Continue | ControlFlow::None => {}
            ControlFlow::Return(v) => return Ok(ControlFlow::Return(v)),
        }
    }

    Ok(ControlFlow::None)
}

fn exec_while(
    condition: &GdExpr<'_>,
    body: &[GdStmt<'_>],
    interp: &mut Interpreter<'_>,
) -> InterpResult<ControlFlow> {
    let max_iterations = 1_000_000;
    let mut count = 0;

    loop {
        let cond = eval_expr(condition, interp)?;
        if !cond.is_truthy() {
            break;
        }

        match exec_body(body, interp)? {
            ControlFlow::Break => break,
            ControlFlow::Continue | ControlFlow::None => {}
            ControlFlow::Return(v) => return Ok(ControlFlow::Return(v)),
        }

        count += 1;
        if count >= max_iterations {
            return Err(InterpError::value_error(
                "while loop exceeded 1,000,000 iterations (possible infinite loop)",
                condition.line(),
                condition.column(),
            ));
        }
    }

    Ok(ControlFlow::None)
}

fn arm_matches(
    val: &GdValue,
    patterns: &[GdExpr<'_>],
    interp: &mut Interpreter<'_>,
) -> InterpResult<bool> {
    for pattern in patterns {
        match pattern {
            // Wildcard `_`
            GdExpr::Ident { name: "_", .. } => return Ok(true),
            // Binding pattern — any other identifier binds the value
            GdExpr::Ident { name, .. } => {
                interp.env.define(name, val.clone());
                return Ok(true);
            }
            // Literal comparison
            _ => {
                let pattern_val = eval_expr(pattern, interp)?;
                if *val == pattern_val {
                    return Ok(true);
                }
            }
        }
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: parse a complete GDScript snippet, find and execute a function.
    fn run_script(source: &str) -> (InterpResult<GdValue>, Vec<String>) {
        let tree = gd_core::parser::parse(source).expect("parse failed");
        let file = gd_core::gd_ast::convert(&tree, source);
        let mut interp = Interpreter::from_file(&file).unwrap();

        let result = if let Some(func) = interp.lookup_func("test_main") {
            exec_func(func, &[], &mut interp)
        } else {
            // Execute top-level statements
            let mut last = GdValue::Null;
            for decl in &file.declarations {
                if let gd_core::gd_ast::GdDecl::Stmt(s) = decl {
                    match exec_stmt(s, &mut interp) {
                        Ok(ControlFlow::Return(v)) => {
                            last = v;
                            break;
                        }
                        Ok(_) => {}
                        Err(e) => return (Err(e), interp.env.take_output()),
                    }
                }
            }
            Ok(last)
        };

        let output = interp.env.take_output();
        (result, output)
    }

    #[test]
    fn test_var_and_assign() {
        let (result, _) = run_script("func test_main():\n\tvar x = 10\n\tx = 20\n\treturn x\n");
        assert_eq!(result.unwrap(), GdValue::Int(20));
    }

    #[test]
    fn test_if_else() {
        let (result, _) = run_script(
            "func test_main():\n\tvar x = 5\n\tif x > 3:\n\t\treturn \"big\"\n\telse:\n\t\treturn \"small\"\n",
        );
        assert_eq!(result.unwrap(), GdValue::GdString("big".into()));
    }

    #[test]
    fn test_for_loop() {
        let (result, _) = run_script(
            "func test_main():\n\tvar sum = 0\n\tfor i in [1, 2, 3, 4, 5]:\n\t\tsum += i\n\treturn sum\n",
        );
        assert_eq!(result.unwrap(), GdValue::Int(15));
    }

    #[test]
    fn test_while_loop() {
        let (result, _) = run_script(
            "func test_main():\n\tvar i = 0\n\tvar sum = 0\n\twhile i < 5:\n\t\tsum += i\n\t\ti += 1\n\treturn sum\n",
        );
        assert_eq!(result.unwrap(), GdValue::Int(10));
    }

    #[test]
    fn test_for_with_break() {
        let (result, _) = run_script(
            "func test_main():\n\tvar sum = 0\n\tfor i in [1, 2, 3, 4, 5]:\n\t\tif i == 4:\n\t\t\tbreak\n\t\tsum += i\n\treturn sum\n",
        );
        assert_eq!(result.unwrap(), GdValue::Int(6));
    }

    #[test]
    fn test_match() {
        let (result, _) = run_script(
            "func test_main():\n\tvar x = 2\n\tmatch x:\n\t\t1:\n\t\t\treturn \"one\"\n\t\t2:\n\t\t\treturn \"two\"\n\t\t_:\n\t\t\treturn \"other\"\n",
        );
        assert_eq!(result.unwrap(), GdValue::GdString("two".into()));
    }

    #[test]
    fn test_aug_assign() {
        let (result, _) = run_script(
            "func test_main():\n\tvar x = 10\n\tx += 5\n\tx -= 3\n\tx *= 2\n\treturn x\n",
        );
        assert_eq!(result.unwrap(), GdValue::Int(24));
    }

    #[test]
    fn test_print_capture() {
        let (_, output) = run_script("func test_main():\n\tprint(\"hello\")\n\tprint(\"world\")\n");
        assert_eq!(output, vec!["hello", "world"]);
    }

    #[test]
    fn test_return_null() {
        let (result, _) = run_script("func test_main():\n\tpass\n");
        assert_eq!(result.unwrap(), GdValue::Null);
    }

    #[test]
    fn test_elif() {
        let (result, _) = run_script(
            "func test_main():\n\tvar x = 5\n\tif x > 10:\n\t\treturn \"big\"\n\telif x > 3:\n\t\treturn \"medium\"\n\telse:\n\t\treturn \"small\"\n",
        );
        assert_eq!(result.unwrap(), GdValue::GdString("medium".into()));
    }

    #[test]
    fn test_for_range_sugar() {
        let (result, _) = run_script(
            "func test_main():\n\tvar sum = 0\n\tfor i in 5:\n\t\tsum += i\n\treturn sum\n",
        );
        assert_eq!(result.unwrap(), GdValue::Int(10)); // 0+1+2+3+4
    }

    #[test]
    fn test_nested_if() {
        let (result, _) = run_script(
            "func test_main():\n\tvar x = 5\n\tvar y = 3\n\tif x > 3:\n\t\tif y > 2:\n\t\t\treturn \"both\"\n\treturn \"nope\"\n",
        );
        assert_eq!(result.unwrap(), GdValue::GdString("both".into()));
    }

    #[test]
    fn test_subscript_assign() {
        let (result, _) = run_script(
            "func test_main():\n\tvar arr = [1, 2, 3]\n\tarr[1] = 99\n\treturn arr[1]\n",
        );
        assert_eq!(result.unwrap(), GdValue::Int(99));
    }

    #[test]
    fn test_user_function_call() {
        let (result, _) = run_script(
            "func add(a, b):\n\treturn a + b\n\nfunc test_main():\n\treturn add(3, 4)\n",
        );
        assert_eq!(result.unwrap(), GdValue::Int(7));
    }

    #[test]
    fn test_recursive_function() {
        let (result, _) = run_script(
            "func factorial(n):\n\tif n <= 1:\n\t\treturn 1\n\treturn n * factorial(n - 1)\n\nfunc test_main():\n\treturn factorial(5)\n",
        );
        assert_eq!(result.unwrap(), GdValue::Int(120));
    }

    #[test]
    fn test_function_default_params() {
        let (result, _) = run_script(
            "func greet(name = \"world\"):\n\treturn \"hello \" + name\n\nfunc test_main():\n\treturn greet()\n",
        );
        assert_eq!(result.unwrap(), GdValue::GdString("hello world".into()));
    }

    #[test]
    fn test_array_append() {
        let (result, _) = run_script(
            "func test_main():\n\tvar arr = [1, 2]\n\tarr.append(3)\n\treturn arr.size()\n",
        );
        assert_eq!(result.unwrap(), GdValue::Int(3));
    }

    #[test]
    fn test_array_pop() {
        let (result, _) = run_script(
            "func test_main():\n\tvar arr = [10, 20, 30]\n\tvar last = arr.pop_back()\n\treturn last\n",
        );
        assert_eq!(result.unwrap(), GdValue::Int(30));
    }

    #[test]
    fn test_array_sort() {
        let (result, _) =
            run_script("func test_main():\n\tvar arr = [3, 1, 2]\n\tarr.sort()\n\treturn arr[0]\n");
        assert_eq!(result.unwrap(), GdValue::Int(1));
    }

    #[test]
    fn test_array_erase() {
        let (result, _) = run_script(
            "func test_main():\n\tvar arr = [1, 2, 3]\n\tarr.erase(2)\n\treturn arr.size()\n",
        );
        assert_eq!(result.unwrap(), GdValue::Int(2));
    }

    #[test]
    fn test_dict_erase() {
        let (result, _) = run_script(
            "func test_main():\n\tvar d = {\"a\": 1, \"b\": 2}\n\td.erase(\"a\")\n\treturn d.size()\n",
        );
        assert_eq!(result.unwrap(), GdValue::Int(1));
    }

    #[test]
    fn test_dict_clear() {
        let (result, _) = run_script(
            "func test_main():\n\tvar d = {\"a\": 1}\n\td.clear()\n\treturn d.is_empty()\n",
        );
        assert_eq!(result.unwrap(), GdValue::Bool(true));
    }

    #[test]
    fn test_enum_values() {
        let (result, _) = run_script(
            "enum State { IDLE, RUNNING, JUMPING }\n\nfunc test_main():\n\treturn JUMPING\n",
        );
        assert_eq!(result.unwrap(), GdValue::Int(2));
    }

    #[test]
    fn test_named_enum_dict() {
        let (result, _) =
            run_script("enum Dir { LEFT, RIGHT, UP }\n\nfunc test_main():\n\treturn Dir.size()\n");
        assert_eq!(result.unwrap(), GdValue::Int(3));
    }

    #[test]
    fn test_top_level_var() {
        let (result, _) = run_script("var SPEED = 100\n\nfunc test_main():\n\treturn SPEED * 2\n");
        assert_eq!(result.unwrap(), GdValue::Int(200));
    }

    #[test]
    fn test_multiple_function_calls() {
        let (result, _) = run_script(
            "func double(x):\n\treturn x * 2\n\nfunc add_one(x):\n\treturn x + 1\n\nfunc test_main():\n\treturn add_one(double(5))\n",
        );
        assert_eq!(result.unwrap(), GdValue::Int(11));
    }

    // ── Phase 4: Class system tests ──────────────────────────────────

    #[test]
    fn test_inner_class_new() {
        let (result, _) = run_script(
            "\
class Counter:
\tvar count = 0

func test_main():
\tvar c = Counter.new()
\treturn c.count
",
        );
        assert_eq!(result.unwrap(), GdValue::Int(0));
    }

    #[test]
    fn test_inner_class_init() {
        let (result, _) = run_script(
            "\
class Counter:
\tvar count = 0
\tfunc _init(start):
\t\tself.count = start

func test_main():
\tvar c = Counter.new(10)
\treturn c.count
",
        );
        assert_eq!(result.unwrap(), GdValue::Int(10));
    }

    #[test]
    fn test_inner_class_method() {
        let (result, _) = run_script(
            "\
class Counter:
\tvar count = 0
\tfunc increment():
\t\tself.count += 1
\tfunc get_count():
\t\treturn self.count

func test_main():
\tvar c = Counter.new()
\tc.increment()
\tc.increment()
\treturn c.get_count()
",
        );
        assert_eq!(result.unwrap(), GdValue::Int(2));
    }

    #[test]
    fn test_inner_class_method_with_args() {
        let (result, _) = run_script(
            "\
class Adder:
\tvar total = 0
\tfunc add(n):
\t\tself.total += n
\t\treturn self.total

func test_main():
\tvar a = Adder.new()
\ta.add(5)
\ta.add(3)
\treturn a.total
",
        );
        assert_eq!(result.unwrap(), GdValue::Int(8));
    }

    #[test]
    fn test_inner_class_multiple_instances() {
        let (result, _) = run_script(
            "\
class Box:
\tvar value = 0
\tfunc _init(v):
\t\tself.value = v

func test_main():
\tvar a = Box.new(10)
\tvar b = Box.new(20)
\treturn a.value + b.value
",
        );
        assert_eq!(result.unwrap(), GdValue::Int(30));
    }

    #[test]
    fn test_inner_class_is_check() {
        let (result, _) = run_script(
            "\
class MyNode:
\tpass

func test_main():
\tvar n = MyNode.new()
\treturn n is MyNode
",
        );
        assert_eq!(result.unwrap(), GdValue::Bool(true));
    }

    #[test]
    fn test_inner_class_inheritance() {
        let (result, _) = run_script(
            "\
class Base:
\tvar x = 10
\tfunc get_x():
\t\treturn self.x

class Child extends Base:
\tvar y = 20
\tfunc get_sum():
\t\treturn self.x + self.y

func test_main():
\tvar c = Child.new()
\treturn c.get_sum()
",
        );
        assert_eq!(result.unwrap(), GdValue::Int(30));
    }

    #[test]
    fn test_inner_class_inherited_method() {
        let (result, _) = run_script(
            "\
class Base:
\tvar x = 5
\tfunc get_x():
\t\treturn self.x

class Child extends Base:
\tvar y = 10

func test_main():
\tvar c = Child.new()
\treturn c.get_x()
",
        );
        assert_eq!(result.unwrap(), GdValue::Int(5));
    }

    #[test]
    fn test_class_constructor_shorthand() {
        // ClassName() as shorthand for ClassName.new()
        let (result, _) = run_script(
            "\
class Point:
\tvar x = 0
\tvar y = 0
\tfunc _init(px, py):
\t\tself.x = px
\t\tself.y = py

func test_main():
\tvar p = Point.new(3, 4)
\treturn p.x * p.x + p.y * p.y
",
        );
        assert_eq!(result.unwrap(), GdValue::Int(25));
    }

    #[test]
    fn test_class_static_method() {
        let (result, _) = run_script(
            "\
class MathUtil:
\tstatic func add(a, b):
\t\treturn a + b

func test_main():
\treturn MathUtil.add(3, 4)
",
        );
        assert_eq!(result.unwrap(), GdValue::Int(7));
    }

    #[test]
    fn test_class_method_returns_value() {
        let (result, _) = run_script(
            "\
class Calc:
\tvar value = 0
\tfunc _init(v):
\t\tself.value = v
\tfunc doubled():
\t\treturn self.value * 2

func test_main():
\tvar c = Calc.new(21)
\treturn c.doubled()
",
        );
        assert_eq!(result.unwrap(), GdValue::Int(42));
    }
}
