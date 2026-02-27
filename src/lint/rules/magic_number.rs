use std::collections::HashSet;

use crate::core::gd_ast::{self, GdDecl, GdExpr, GdFile, GdStmt};

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct MagicNumber;

/// Default allowed values: ubiquitous in math/game code.
const DEFAULT_ALLOWED: &[f64] = &[
    0.0, 1.0, -1.0, 2.0, 0.5, //
    4.0, 8.0, 16.0, 32.0, 64.0, 128.0, 256.0, // powers of 2
    10.0, 100.0, 255.0, 360.0, // common game/math constants
];

/// Default function/constructor contexts where numbers are fine as literals.
const DEFAULT_ALLOWED_CONTEXTS: &[&str] = &[
    "Vector2",
    "Vector3",
    "Vector4",
    "Vector2i",
    "Vector3i",
    "Vector4i",
    "Color",
    "Rect2",
    "Rect2i",
    "clamp",
    "clampf",
    "clampi",
    "lerp",
    "lerpf",
    "lerp_angle",
    "inverse_lerp",
    "smoothstep",
    "move_toward",
    "min",
    "max",
    "minf",
    "maxf",
    "mini",
    "maxi",
    "abs",
    "absf",
    "absi",
    "sign",
    "signf",
    "signi",
    "pow",
    "sqrt",
    "ceil",
    "floor",
    "round",
    "snapped",
    "snappedf",
    "snappedi",
    "wrap",
    "wrapf",
    "wrapi",
    "range",
    "randi_range",
    "randf_range",
    "randfn",
    "deg_to_rad",
    "rad_to_deg",
    "linear_to_db",
    "db_to_linear",
];

impl LintRule for MagicNumber {
    fn name(&self) -> &'static str {
        "magic-number"
    }

    fn category(&self) -> LintCategory {
        LintCategory::TypeSafety
    }

    fn default_enabled(&self) -> bool {
        false
    }

    fn check(&self, file: &GdFile<'_>, source: &str, config: &LintConfig) -> Vec<LintDiagnostic> {
        let rule_config = config.rules.get("magic-number");

        // Build allowed set from config or defaults
        let allowed: HashSet<OrderedF64> = if let Some(rc) = rule_config
            && !rc.allowed.is_empty()
        {
            rc.allowed.iter().map(|&v| OrderedF64(v)).collect()
        } else {
            DEFAULT_ALLOWED.iter().map(|&v| OrderedF64(v)).collect()
        };

        // Build allowed contexts set
        let allowed_contexts: HashSet<&str> = if let Some(rc) = rule_config
            && !rc.allowed_contexts.is_empty()
        {
            rc.allowed_contexts
                .iter()
                .map(std::string::String::as_str)
                .collect()
        } else {
            DEFAULT_ALLOWED_CONTEXTS.iter().copied().collect()
        };

        let min_value = rule_config.and_then(|rc| rc.min_value).unwrap_or(0.0);

        let ctx = CheckContext {
            allowed: &allowed,
            allowed_contexts: &allowed_contexts,
            min_value,
        };
        let mut diags = Vec::new();
        gd_ast::visit_decls(file, &mut |decl| {
            if let GdDecl::Func(func) = decl {
                check_stmts(&func.body, source, &ctx, &mut diags);
            }
        });
        diags
    }
}

struct CheckContext<'a> {
    allowed: &'a HashSet<OrderedF64>,
    allowed_contexts: &'a HashSet<&'a str>,
    min_value: f64,
}

/// Wrapper for f64 that implements Hash + Eq via total ordering.
#[derive(Debug, Clone, Copy)]
struct OrderedF64(f64);

impl PartialEq for OrderedF64 {
    fn eq(&self, other: &Self) -> bool {
        self.0.to_bits() == other.0.to_bits()
    }
}

impl Eq for OrderedF64 {}

impl std::hash::Hash for OrderedF64 {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.to_bits().hash(state);
    }
}

/// Walk function body statements checking for magic numbers.
/// Variable/const definitions are skipped entirely (numbers in definitions are fine).
fn check_stmts(stmts: &[GdStmt], source: &str, ctx: &CheckContext, diags: &mut Vec<LintDiagnostic>) {
    for stmt in stmts {
        match stmt {
            // Variable/const definitions — numbers are fine here.
            // Listed explicitly so the intent is clear (not just a wildcard fallthrough).
            #[allow(clippy::match_same_arms)]
            GdStmt::Var(_) => {}

            GdStmt::Expr { expr, .. } => check_expr(expr, source, ctx, false, false, diags),
            GdStmt::Assign { value, .. } | GdStmt::AugAssign { value, .. } => {
                check_expr(value, source, ctx, false, false, diags);
            }
            GdStmt::Return { value: Some(v), .. } => check_expr(v, source, ctx, false, false, diags),

            GdStmt::If(if_stmt) => {
                check_expr(&if_stmt.condition, source, ctx, false, false, diags);
                check_stmts(&if_stmt.body, source, ctx, diags);
                for (cond, branch) in &if_stmt.elif_branches {
                    check_expr(cond, source, ctx, false, false, diags);
                    check_stmts(branch, source, ctx, diags);
                }
                if let Some(else_body) = &if_stmt.else_body {
                    check_stmts(else_body, source, ctx, diags);
                }
            }
            GdStmt::For { iter, body, .. } => {
                check_expr(iter, source, ctx, false, false, diags);
                check_stmts(body, source, ctx, diags);
            }
            GdStmt::While { condition, body, .. } => {
                check_expr(condition, source, ctx, false, false, diags);
                check_stmts(body, source, ctx, diags);
            }
            GdStmt::Match { value, arms, .. } => {
                check_expr(value, source, ctx, false, false, diags);
                for arm in arms {
                    for pat in &arm.patterns {
                        check_expr(pat, source, ctx, false, false, diags);
                    }
                    if let Some(guard) = &arm.guard {
                        check_expr(guard, source, ctx, false, false, diags);
                    }
                    check_stmts(&arm.body, source, ctx, diags);
                }
            }
            _ => {}
        }
    }
}

/// Check an expression for magic numbers. Context flags are pushed down through
/// the tree instead of walking up parents:
/// - `in_allowed_ctx`: inside an allowed function/constructor call's arguments
/// - `in_subscript`: inside a subscript index (array access)
fn check_expr(
    expr: &GdExpr,
    source: &str,
    ctx: &CheckContext,
    in_allowed_ctx: bool,
    in_subscript: bool,
    diags: &mut Vec<LintDiagnostic>,
) {
    match expr {
        GdExpr::IntLiteral { node, value } | GdExpr::FloatLiteral { node, value } => {
            if in_allowed_ctx || in_subscript {
                return;
            }
            if let Ok(parsed) = parse_numeric(value)
                && parsed.abs() >= ctx.min_value
                && !ctx.allowed.contains(&OrderedF64(parsed))
            {
                diags.push(LintDiagnostic {
                    rule: "magic-number",
                    message: format!(
                        "consider extracting magic number {value} to a named constant",
                    ),
                    severity: Severity::Warning,
                    line: node.start_position().row,
                    column: node.start_position().column,
                    fix: None,
                    end_column: None,
                    context_lines: None,
                });
            }
        }

        GdExpr::Call { callee, args, .. } => {
            let func_name = callee_name(callee);
            let args_allowed =
                in_allowed_ctx || func_name.is_some_and(|n| ctx.allowed_contexts.contains(n));
            check_expr(callee, source, ctx, in_allowed_ctx, in_subscript, diags);
            for arg in args {
                check_expr(arg, source, ctx, args_allowed, false, diags);
            }
        }
        GdExpr::MethodCall { receiver, method, args, .. } => {
            let args_allowed = in_allowed_ctx || ctx.allowed_contexts.contains(method);
            check_expr(receiver, source, ctx, in_allowed_ctx, in_subscript, diags);
            for arg in args {
                check_expr(arg, source, ctx, args_allowed, false, diags);
            }
        }

        GdExpr::BinOp { op, left, right, .. } => {
            if *op == "%" {
                // Left side checked normally; right side of modulo is always allowed
                check_expr(left, source, ctx, in_allowed_ctx, in_subscript, diags);
            } else if is_comparison_op(op) {
                // In comparisons, skip 0/1 literals (common idioms like `size() == 0`)
                check_comparison_side(left, source, ctx, in_allowed_ctx, in_subscript, diags);
                check_comparison_side(right, source, ctx, in_allowed_ctx, in_subscript, diags);
            } else {
                check_expr(left, source, ctx, in_allowed_ctx, in_subscript, diags);
                check_expr(right, source, ctx, in_allowed_ctx, in_subscript, diags);
            }
        }

        GdExpr::Subscript { receiver, index, .. } => {
            check_expr(receiver, source, ctx, in_allowed_ctx, false, diags);
            check_expr(index, source, ctx, in_allowed_ctx, true, diags);
        }

        GdExpr::UnaryOp { operand, .. } => {
            check_expr(operand, source, ctx, in_allowed_ctx, in_subscript, diags);
        }
        GdExpr::PropertyAccess { receiver, .. } => {
            check_expr(receiver, source, ctx, in_allowed_ctx, in_subscript, diags);
        }
        GdExpr::Ternary { condition, true_val, false_val, .. } => {
            check_expr(condition, source, ctx, in_allowed_ctx, in_subscript, diags);
            check_expr(true_val, source, ctx, in_allowed_ctx, in_subscript, diags);
            check_expr(false_val, source, ctx, in_allowed_ctx, in_subscript, diags);
        }
        GdExpr::Array { elements, .. } => {
            for e in elements {
                check_expr(e, source, ctx, in_allowed_ctx, in_subscript, diags);
            }
        }
        GdExpr::Dict { pairs, .. } => {
            for (k, v) in pairs {
                check_expr(k, source, ctx, in_allowed_ctx, in_subscript, diags);
                check_expr(v, source, ctx, in_allowed_ctx, in_subscript, diags);
            }
        }
        GdExpr::Cast { expr: inner, .. }
        | GdExpr::Is { expr: inner, .. }
        | GdExpr::Await { expr: inner, .. } => {
            check_expr(inner, source, ctx, in_allowed_ctx, in_subscript, diags);
        }

        _ => {} // Ident, StringLiteral, Bool, Null, Self_, Lambda, etc.
    }
}

/// Extract the function/constructor name from a call callee expression.
fn callee_name<'a>(callee: &GdExpr<'a>) -> Option<&'a str> {
    match callee {
        GdExpr::Ident { name, .. } => Some(name),
        GdExpr::PropertyAccess { property, .. } => Some(property),
        _ => None,
    }
}

fn is_comparison_op(op: &str) -> bool {
    matches!(op, "==" | "!=" | "<" | ">" | "<=" | ">=")
}

/// Check one side of a comparison expression, skipping 0/1 literals.
fn check_comparison_side(
    expr: &GdExpr,
    source: &str,
    ctx: &CheckContext,
    in_allowed_ctx: bool,
    in_subscript: bool,
    diags: &mut Vec<LintDiagnostic>,
) {
    match expr {
        GdExpr::IntLiteral { value, .. } | GdExpr::FloatLiteral { value, .. }
            if matches!(*value, "0" | "1" | "0.0" | "1.0") => {}
        _ => check_expr(expr, source, ctx, in_allowed_ctx, in_subscript, diags),
    }
}

/// Parse a GDScript numeric literal to f64.
fn parse_numeric(text: &str) -> Result<f64, ()> {
    // Handle hex, binary, octal prefixes
    if let Some(hex) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
        let hex = hex.replace('_', "");
        return u64::from_str_radix(&hex, 16)
            .map(|v| v as f64)
            .map_err(|_| ());
    }
    if let Some(bin) = text.strip_prefix("0b").or_else(|| text.strip_prefix("0B")) {
        let bin = bin.replace('_', "");
        return u64::from_str_radix(&bin, 2)
            .map(|v| v as f64)
            .map_err(|_| ());
    }
    // Standard integer or float (may contain underscores)
    let clean = text.replace('_', "");
    clean.parse::<f64>().map_err(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::config::{LintConfig, RuleConfig};
    use crate::core::parser;
    use crate::core::gd_ast;
    use std::collections::HashMap;

    fn check(source: &str) -> Vec<LintDiagnostic> {
        let tree = parser::parse(source).unwrap();
        let file = gd_ast::convert(&tree, source);
        let config = LintConfig::default();
        MagicNumber.check(&file, source, &config)
    }

    fn check_with_config(source: &str, config: &LintConfig) -> Vec<LintDiagnostic> {
        let tree = parser::parse(source).unwrap();
        let file = gd_ast::convert(&tree, source);
        MagicNumber.check(&file, source, config)
    }

    #[test]
    fn flags_magic_number_in_function() {
        let source = "func foo():\n\tvar x = 42\n\ttake_damage(50)\n";
        let diags = check(source);
        // 42 is in a variable_statement, so only 50 is flagged
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("50"));
    }

    #[test]
    fn allows_default_values() {
        let source = "\
func foo():
\tvar a = health + 0
\tvar b = health + 1
\tvar c = health + 0.5
\tvar d = health + 2.0
\tvar e = health + 256
";
        assert!(check(source).is_empty());
    }

    #[test]
    fn skips_const_and_var_definitions() {
        let source = "\
func foo():
\tvar max_health = 9999
\tconst SPEED = 300
";
        assert!(check(source).is_empty());
    }

    #[test]
    fn skips_top_level_numbers() {
        let source = "\
var MAX_HEALTH = 100
const SPEED = 300.0
";
        assert!(check(source).is_empty());
    }

    #[test]
    fn skips_allowed_contexts_vector() {
        let source = "\
func foo():
\tvar pos = Vector2(150, 300)
\tvar color = Color(0.8, 0.2, 0.5, 1.0)
\tvar v3 = Vector3(10, 20, 30)
";
        assert!(check(source).is_empty());
    }

    #[test]
    fn skips_allowed_contexts_math() {
        let source = "\
func foo():
\tvar x = clamp(val, 5, 95)
\tvar y = lerp(a, b, 0.75)
\tvar z = min(health, 9999)
\tvar w = max(0, damage - 50)
";
        // 50 in `damage - 50` is NOT inside a min/max call argument, it's in a binary_operator
        // Actually `max(0, damage - 50)` — the 50 is inside the max() call's arguments
        let diags = check(source);
        assert!(diags.is_empty());
    }

    #[test]
    fn skips_array_index() {
        let source = "\
func foo(arr):
\tvar first = arr[0]
\tvar last = arr[-1]
\tvar third = arr[2]
";
        assert!(check(source).is_empty());
    }

    #[test]
    fn skips_modulo_operand() {
        let source = "\
func foo(x):
\tif x % 2 == 0:
\t\tprint(\"even\")
\tvar frame = x % 60
";
        assert!(check(source).is_empty());
    }

    #[test]
    fn min_value_filter() {
        let mut config = LintConfig::default();
        let mut rules = HashMap::new();
        rules.insert(
            "magic-number".to_string(),
            RuleConfig {
                min_value: Some(10.0),
                ..Default::default()
            },
        );
        config.rules = rules;

        let source = "\
func foo():
\tset_val(3)
\tset_val(0.85)
\tset_val(50)
";
        let diags = check_with_config(source, &config);
        // 3 and 0.85 are below min_value=10, only 50 is flagged
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("50"));
    }

    #[test]
    fn custom_allowed_list() {
        let mut config = LintConfig::default();
        let mut rules = HashMap::new();
        rules.insert(
            "magic-number".to_string(),
            RuleConfig {
                allowed: vec![0.0, 1.0, 42.0, 100.0],
                ..Default::default()
            },
        );
        config.rules = rules;

        let source = "\
func foo():
\tset_val(42)
\tset_val(100)
\tset_val(50)
";
        let diags = check_with_config(source, &config);
        // 42 and 100 are in custom allowed list, only 50 is flagged
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("50"));
    }

    #[test]
    fn custom_allowed_contexts() {
        let mut config = LintConfig::default();
        let mut rules = HashMap::new();
        rules.insert(
            "magic-number".to_string(),
            RuleConfig {
                allowed_contexts: vec!["my_custom_func".to_string()],
                ..Default::default()
            },
        );
        config.rules = rules;

        let source = "\
func foo():
\tmy_custom_func(999)
\tother_func(999)
";
        let diags = check_with_config(source, &config);
        // 999 in my_custom_func is allowed, 999 in other_func is flagged
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn powers_of_two_allowed_by_default() {
        let source = "\
func foo():
\tvar x = health + 4
\tvar y = health + 8
\tvar z = health + 16
\tvar w = health + 32
\tvar a = health + 64
\tvar b = health + 128
";
        assert!(check(source).is_empty());
    }

    #[test]
    fn hex_and_binary_literals() {
        let source = "\
func foo():
\tvar x = health + 0xFF
\tvar y = health + 0b1010
";
        let diags = check(source);
        // 0xFF = 255 (allowed), 0b1010 = 10 (allowed)
        assert!(diags.is_empty());
    }

    #[test]
    fn flags_gameplay_tuning_numbers() {
        let source = "\
func foo():
\ttake_damage(50)
\tset_speed(300.0)
\thealth -= 25
";
        let diags = check(source);
        assert_eq!(diags.len(), 3);
    }
}
