use std::collections::HashSet;

use tree_sitter::{Node, Tree};

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

    fn check(&self, tree: &Tree, source: &str, config: &LintConfig) -> Vec<LintDiagnostic> {
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

        let mut diags = Vec::new();
        let root = tree.root_node();
        let ctx = CheckContext {
            allowed: &allowed,
            allowed_contexts: &allowed_contexts,
            min_value,
        };
        check_node(root, source, &mut diags, false, &ctx);
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

fn check_node(
    node: Node,
    source: &str,
    diags: &mut Vec<LintDiagnostic>,
    in_function_body: bool,
    ctx: &CheckContext,
) {
    // If we're entering a function body, mark it
    if node.kind() == "function_definition"
        && let Some(body_node) = node.child_by_field_name("body")
    {
        check_node(body_node, source, diags, true, ctx);
        return;
    }

    // Only check numeric literals inside function bodies
    if in_function_body && (node.kind() == "integer" || node.kind() == "float") {
        // Skip if inside a variable/const definition (right-hand side)
        if !is_in_variable_or_const_definition(&node) {
            let value_text = &source[node.byte_range()];

            if let Ok(value) = parse_numeric(value_text) {
                let abs_value = value.abs();

                // Check min_value threshold
                if abs_value >= ctx.min_value
                    && !ctx.allowed.contains(&OrderedF64(value))
                    && !is_in_allowed_context(&node, source, ctx)
                    && !is_in_array_index(&node)
                    && !is_modulo_operand(&node, source)
                    && !is_comparison_to_zero_or_one(&node, source)
                {
                    diags.push(LintDiagnostic {
                        rule: "magic-number",
                        message: format!(
                            "consider extracting magic number {value_text} to a named constant",
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
        }
    }

    // Recurse into children
    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            check_node(cursor.node(), source, diags, in_function_body, ctx);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
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

/// Check if a node is inside a variable_statement or const_statement.
fn is_in_variable_or_const_definition(node: &Node) -> bool {
    let mut current = node.parent();
    while let Some(parent) = current {
        let kind = parent.kind();
        if kind == "variable_statement" || kind == "const_statement" {
            return true;
        }
        if kind == "function_definition" {
            return false;
        }
        current = parent.parent();
    }
    false
}

/// Check if this number is an argument inside an allowed function/constructor call.
fn is_in_allowed_context(node: &Node, source: &str, ctx: &CheckContext) -> bool {
    let mut current = node.parent();
    while let Some(parent) = current {
        match parent.kind() {
            "call" => {
                // tree-sitter-gdscript: no `function` field on bare calls, use named_child(0)
                if let Some(func_node) = parent
                    .child_by_field_name("function")
                    .or_else(|| parent.named_child(0))
                {
                    let func_text = &source[func_node.byte_range()];
                    // Handle method calls like `obj.lerp(...)` — extract the method name
                    let func_name = func_text.rsplit('.').next().unwrap_or(func_text);
                    if ctx.allowed_contexts.contains(func_name) {
                        return true;
                    }
                }
            }
            "attribute_call" => {
                // `attribute` > `attribute_call` for method calls
                if let Some(method_node) = parent.child_by_field_name("method") {
                    let method_name = &source[method_node.byte_range()];
                    if ctx.allowed_contexts.contains(method_name) {
                        return true;
                    }
                }
            }
            // Stop searching at function boundary
            "function_definition" => return false,
            _ => {}
        }
        current = parent.parent();
    }
    false
}

/// Check if the number is used as an array/dictionary index: `arr[0]`, `arr[-1]`.
fn is_in_array_index(node: &Node) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    // Direct subscript: `arr[0]`
    if parent.kind() == "subscript" {
        return true;
    }
    // Negative index: `arr[-1]` — the `-1` is a `unary_operator` inside a `subscript`
    if parent.kind() == "unary_operator"
        && let Some(grandparent) = parent.parent()
    {
        return grandparent.kind() == "subscript";
    }
    false
}

/// Check if the number is the right-hand side of a modulo operation: `x % 2`.
fn is_modulo_operand(node: &Node, source: &str) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    if parent.kind() == "binary_operator"
        && let Some(op_node) = parent.child_by_field_name("op")
    {
        let op = &source[op_node.byte_range()];
        return op == "%";
    }
    false
}

/// Check if the number is being compared to 0 or 1 (common idioms like `size() == 0`).
fn is_comparison_to_zero_or_one(node: &Node, source: &str) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    if (parent.kind() == "comparison_operator" || parent.kind() == "binary_operator")
        && let Some(op_node) = parent.child_by_field_name("op")
    {
        let op = &source[op_node.byte_range()];
        if matches!(op, "==" | "!=" | "<" | ">" | "<=" | ">=") {
            let value_text = &source[node.byte_range()];
            return matches!(value_text, "0" | "1" | "0.0" | "1.0");
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::config::{LintConfig, RuleConfig};
    use crate::core::parser;
    use std::collections::HashMap;

    fn check(source: &str) -> Vec<LintDiagnostic> {
        let tree = parser::parse(source).unwrap();
        let config = LintConfig::default();
        MagicNumber.check(&tree, source, &config)
    }

    fn check_with_config(source: &str, config: &LintConfig) -> Vec<LintDiagnostic> {
        let tree = parser::parse(source).unwrap();
        MagicNumber.check(&tree, source, config)
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
