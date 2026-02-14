use std::fmt::Write as _;

// ── Expression rewriting ─────────────────────────────────────────────
//
// Godot's evaluate uses the Expression class, not GDScript. It supports
// property reads, method calls, constructors, and built-in functions but
// NOT assignments, $NodePath, %UniqueName, or compound operators.
//
// We rewrite common GDScript patterns into Expression-compatible equivalents
// so users can type natural GDScript and have it "just work".

/// Rewrite a GDScript expression into one compatible with Godot's Expression class.
/// Returns `(rewritten_expr, was_rewritten)`.
pub(crate) fn rewrite_eval_expression(expr: &str) -> (String, bool) {
    let trimmed = expr.trim();

    // Already using set()/set_indexed() — pass through
    if trimmed.contains(".set(") || trimmed.contains(".set_indexed(") {
        return (trimmed.to_string(), false);
    }

    // 1. Semicolon-separated multi-expression → array trick (before assignment check,
    //    since individual parts may contain assignments that get rewritten recursively)
    if let Some(rewritten) = rewrite_multi_expression(trimmed) {
        return (rewritten, true);
    }

    // 2. $NodePath / %UniqueName rewrites (before assignment check)
    if let Some(rewritten) = rewrite_node_paths(trimmed) {
        return (rewritten, true);
    }

    // 3. Compound assignment: +=, -=, *=, /=
    if let Some(rewritten) = rewrite_compound_assignment(trimmed) {
        return (rewritten, true);
    }

    // 4. Simple assignment: lhs = rhs
    if let Some(rewritten) = rewrite_simple_assignment(trimmed) {
        return (rewritten, true);
    }

    (trimmed.to_string(), false)
}

/// Rewrite `$NodePath` → `get_node("NodePath")` and `%Unique` → `get_node("%Unique")`.
/// Handles `$Path.property`, `$"Quoted/Path"`, chained access, and method calls.
fn rewrite_node_paths(expr: &str) -> Option<String> {
    // Check if expression contains $ or % node references
    if !expr.contains('$') && !contains_unique_ref(expr) {
        return None;
    }

    let mut result = String::with_capacity(expr.len() + 16);
    let chars: Vec<char> = expr.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '$' {
            // $"quoted/path" or $'quoted/path'
            if i + 1 < chars.len() && (chars[i + 1] == '"' || chars[i + 1] == '\'') {
                let quote = chars[i + 1];
                let start = i + 2;
                let mut end = start;
                while end < chars.len() && chars[end] != quote {
                    end += 1;
                }
                let path: String = chars[start..end].iter().collect();
                let _ = write!(result, "get_node(\"{path}\")");
                i = if end < chars.len() { end + 1 } else { end };
            } else {
                // $NodePath — consume identifier chars, /, and .. for parent refs
                let start = i + 1;
                let mut end = start;
                while end < chars.len() {
                    let c = chars[end];
                    if c.is_alphanumeric() || c == '_' || c == '/' {
                        end += 1;
                    } else if c == '.' {
                        // Allow ".." for parent paths ($../Sibling), but stop at
                        // single "." which is property access ($Player.speed)
                        if end + 1 < chars.len() && chars[end + 1] == '.' {
                            end += 2; // consume both dots
                        } else {
                            break; // single dot = property access
                        }
                    } else {
                        break;
                    }
                }
                if end > start {
                    let path: String = chars[start..end].iter().collect();
                    let _ = write!(result, "get_node(\"{path}\")");
                } else {
                    result.push('$');
                }
                i = end;
            }
        } else if chars[i] == '%' && is_unique_ref_at(&chars, i) {
            // %UniqueName
            let start = i + 1;
            let mut end = start;
            while end < chars.len() && (chars[end].is_alphanumeric() || chars[end] == '_') {
                end += 1;
            }
            if end > start {
                let name: String = chars[start..end].iter().collect();
                let _ = write!(result, "get_node(\"%{name}\")");
            } else {
                result.push('%');
            }
            i = end;
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }

    if result == expr { None } else { Some(result) }
}

/// Check if expression contains a `%UniqueName` reference (not `%` in modulo context).
fn contains_unique_ref(expr: &str) -> bool {
    let chars: Vec<char> = expr.chars().collect();
    chars
        .iter()
        .enumerate()
        .any(|(i, _)| is_unique_ref_at(&chars, i))
}

/// Check if `%` at position `i` is a unique node reference (not modulo operator).
/// A `%` is a unique ref when it's at the start or preceded by whitespace/operator,
/// and followed by an identifier character.
fn is_unique_ref_at(chars: &[char], i: usize) -> bool {
    if chars[i] != '%' {
        return false;
    }
    // Must be followed by an identifier start character
    let next_is_ident =
        i + 1 < chars.len() && (chars[i + 1].is_alphabetic() || chars[i + 1] == '_');
    if !next_is_ident {
        return false;
    }
    // At start of expression — it's a unique ref
    if i == 0 {
        return true;
    }
    // After whitespace, open paren, comma, operator — it's a unique ref
    let prev = chars[i - 1];
    prev.is_whitespace() || matches!(prev, '(' | ',' | '[' | '=' | '+' | '-' | '*' | '/' | '!')
}

/// Rewrite compound assignment: `lhs += rhs` → `set("lhs", lhs + rhs)`.
fn rewrite_compound_assignment(expr: &str) -> Option<String> {
    for (op_assign, op) in [("+=", "+"), ("-=", "-"), ("*=", "*"), ("/=", "/")] {
        if let Some(pos) = expr.find(op_assign) {
            let lhs = expr[..pos].trim();
            let rhs = expr[pos + op_assign.len()..].trim();
            if lhs.is_empty() || rhs.is_empty() {
                continue;
            }
            return Some(build_set_expression(lhs, &format!("{lhs} {op} {rhs}")));
        }
    }
    None
}

/// Rewrite simple assignment: `lhs = rhs` → `set("lhs", rhs)`.
fn rewrite_simple_assignment(expr: &str) -> Option<String> {
    // Find `=` that isn't part of ==, !=, <=, >=, :=
    let bytes = expr.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if b != b'=' {
            continue;
        }
        let prev = if i > 0 { bytes[i - 1] } else { 0 };
        let next = bytes.get(i + 1).copied().unwrap_or(0);
        if next == b'=' {
            continue;
        }
        if matches!(
            prev,
            b'!' | b'<' | b'>' | b':' | b'+' | b'-' | b'*' | b'/' | b'='
        ) {
            continue;
        }
        let lhs = expr[..i].trim();
        let rhs = expr[i + 1..].trim();
        if lhs.is_empty() || rhs.is_empty() {
            return None;
        }
        return Some(build_set_expression(lhs, rhs));
    }
    None
}

/// Build a `set()` or `set_indexed()` call from an assignment target and value.
///
/// - `speed = 10` → `set("speed", 10)`
/// - `self.speed = 10` → `set("speed", 10)`
/// - `position.x = 5` → `set_indexed("position:x", 5)`
/// - `self.position.x = 5` → `set_indexed("position:x", 5)`
fn build_set_expression(lhs: &str, rhs: &str) -> String {
    let prop = lhs.strip_prefix("self.").unwrap_or(lhs);

    // Nested property: position.x → set_indexed("position:x", value)
    if let Some(dot_pos) = prop.find('.') {
        let indexed_path = format!("{}:{}", &prop[..dot_pos], &prop[dot_pos + 1..]);
        format!("set_indexed(\"{indexed_path}\", {rhs})")
    } else {
        format!("set(\"{prop}\", {rhs})")
    }
}

/// Rewrite semicolon-separated expressions into an array (each element evaluates).
/// `print("hi"); speed = 10` → `[print("hi"), set("speed", 10)]`
fn rewrite_multi_expression(expr: &str) -> Option<String> {
    if !expr.contains(';') {
        return None;
    }

    // Split on semicolons outside of strings and parens
    let parts = split_on_semicolons(expr);
    if parts.len() < 2 {
        return None;
    }

    let rewritten: Vec<String> = parts
        .iter()
        .map(|part| {
            let (r, _) = rewrite_eval_expression(part.trim());
            r
        })
        .collect();

    Some(format!("[{}]", rewritten.join(", ")))
}

/// Split expression on semicolons, respecting string literals and nested parens/brackets.
fn split_on_semicolons(expr: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let bytes = expr.as_bytes();
    let mut depth = 0u32; // paren/bracket depth
    let mut in_string = false;
    let mut string_char = b'"';
    let mut start = 0;

    for (i, &b) in bytes.iter().enumerate() {
        if in_string {
            if b == string_char && (i == 0 || bytes[i - 1] != b'\\') {
                in_string = false;
            }
            continue;
        }
        match b {
            b'"' | b'\'' => {
                in_string = true;
                string_char = b;
            }
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth = depth.saturating_sub(1),
            b';' if depth == 0 => {
                parts.push(&expr[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&expr[start..]);
    parts
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── rewrite_eval_expression ──────────────────────────────────────

    #[test]
    fn passthrough_simple_expression() {
        let (result, rewritten) = rewrite_eval_expression("position.x");
        assert_eq!(result, "position.x");
        assert!(!rewritten);
    }

    #[test]
    fn passthrough_method_call() {
        let (result, rewritten) = rewrite_eval_expression("get_node(\"Player\").get_name()");
        assert_eq!(result, "get_node(\"Player\").get_name()");
        assert!(!rewritten);
    }

    #[test]
    fn passthrough_comparison() {
        let (result, rewritten) = rewrite_eval_expression("speed == 10");
        assert_eq!(result, "speed == 10");
        assert!(!rewritten);
    }

    #[test]
    fn passthrough_set_call() {
        let (result, rewritten) = rewrite_eval_expression("self.set(\"speed\", 10)");
        assert_eq!(result, "self.set(\"speed\", 10)");
        assert!(!rewritten);
    }

    // ── Simple assignment rewrites ──────────────────────────────────

    #[test]
    fn rewrite_simple_assignment_test() {
        let (result, rewritten) = rewrite_eval_expression("speed = 10");
        assert_eq!(result, "set(\"speed\", 10)");
        assert!(rewritten);
    }

    #[test]
    fn rewrite_self_prefixed_assignment() {
        let (result, rewritten) = rewrite_eval_expression("self.speed = 10");
        assert_eq!(result, "set(\"speed\", 10)");
        assert!(rewritten);
    }

    #[test]
    fn rewrite_nested_property_assignment() {
        let (result, rewritten) = rewrite_eval_expression("position.x = 5");
        assert_eq!(result, "set_indexed(\"position:x\", 5)");
        assert!(rewritten);
    }

    #[test]
    fn rewrite_self_nested_property_assignment() {
        let (result, rewritten) = rewrite_eval_expression("self.position.x = 5.0");
        assert_eq!(result, "set_indexed(\"position:x\", 5.0)");
        assert!(rewritten);
    }

    #[test]
    fn rewrite_assignment_with_constructor() {
        let (result, rewritten) = rewrite_eval_expression("position = Vector3(1, 2, 3)");
        assert_eq!(result, "set(\"position\", Vector3(1, 2, 3))");
        assert!(rewritten);
    }

    #[test]
    fn rewrite_assignment_with_bool() {
        let (result, rewritten) = rewrite_eval_expression("visible = false");
        assert_eq!(result, "set(\"visible\", false)");
        assert!(rewritten);
    }

    // ── Compound assignment rewrites ────────────────────────────────

    #[test]
    fn rewrite_plus_equals() {
        let (result, rewritten) = rewrite_eval_expression("speed += 10");
        assert_eq!(result, "set(\"speed\", speed + 10)");
        assert!(rewritten);
    }

    #[test]
    fn rewrite_minus_equals() {
        let (result, rewritten) = rewrite_eval_expression("health -= 25");
        assert_eq!(result, "set(\"health\", health - 25)");
        assert!(rewritten);
    }

    #[test]
    fn rewrite_times_equals() {
        let (result, rewritten) = rewrite_eval_expression("score *= 2");
        assert_eq!(result, "set(\"score\", score * 2)");
        assert!(rewritten);
    }

    #[test]
    fn rewrite_divide_equals() {
        let (result, rewritten) = rewrite_eval_expression("speed /= 2.0");
        assert_eq!(result, "set(\"speed\", speed / 2.0)");
        assert!(rewritten);
    }

    #[test]
    fn rewrite_self_compound_assignment() {
        let (result, rewritten) = rewrite_eval_expression("self.speed += 5");
        assert_eq!(result, "set(\"speed\", self.speed + 5)");
        assert!(rewritten);
    }

    // ── $NodePath rewrites ──────────────────────────────────────────

    #[test]
    fn rewrite_dollar_node() {
        let (result, rewritten) = rewrite_eval_expression("$Player");
        assert_eq!(result, "get_node(\"Player\")");
        assert!(rewritten);
    }

    #[test]
    fn rewrite_dollar_nested_path() {
        let (result, rewritten) = rewrite_eval_expression("$Player/Sprite");
        assert_eq!(result, "get_node(\"Player/Sprite\")");
        assert!(rewritten);
    }

    #[test]
    fn rewrite_dollar_quoted_path() {
        let (result, rewritten) = rewrite_eval_expression("$\"Path/With Spaces\"");
        assert_eq!(result, "get_node(\"Path/With Spaces\")");
        assert!(rewritten);
    }

    #[test]
    fn rewrite_dollar_parent_path() {
        let (result, rewritten) = rewrite_eval_expression("$../Sibling");
        assert_eq!(result, "get_node(\"../Sibling\")");
        assert!(rewritten);
    }

    #[test]
    fn rewrite_dollar_property_access() {
        let (result, rewritten) = rewrite_eval_expression("$Player.speed");
        assert_eq!(result, "get_node(\"Player\").speed");
        assert!(rewritten);
    }

    #[test]
    fn rewrite_dollar_method_call() {
        let (result, rewritten) = rewrite_eval_expression("$Player.get_name()");
        assert_eq!(result, "get_node(\"Player\").get_name()");
        assert!(rewritten);
    }

    // ── %UniqueName rewrites ────────────────────────────────────────

    #[test]
    fn rewrite_unique_name() {
        let (result, rewritten) = rewrite_eval_expression("%Player");
        assert_eq!(result, "get_node(\"%Player\")");
        assert!(rewritten);
    }

    #[test]
    fn rewrite_unique_name_property() {
        let (result, rewritten) = rewrite_eval_expression("%Player.speed");
        assert_eq!(result, "get_node(\"%Player\").speed");
        assert!(rewritten);
    }

    #[test]
    fn no_rewrite_modulo_operator() {
        let (result, rewritten) = rewrite_eval_expression("10 % 3");
        assert_eq!(result, "10 % 3");
        assert!(!rewritten);
    }

    // ── Multi-expression (semicolons) ───────────────────────────────

    #[test]
    fn rewrite_semicolons() {
        let (result, rewritten) = rewrite_eval_expression("print(\"hi\"); speed = 10");
        assert_eq!(result, "[print(\"hi\"), set(\"speed\", 10)]");
        assert!(rewritten);
    }

    #[test]
    fn no_rewrite_semicolon_in_string() {
        let (result, rewritten) = rewrite_eval_expression("\"hello; world\"");
        assert_eq!(result, "\"hello; world\"");
        assert!(!rewritten);
    }

    // ── Edge cases ──────────────────────────────────────────────────

    #[test]
    fn no_rewrite_not_equal() {
        let (result, rewritten) = rewrite_eval_expression("speed != 0");
        assert_eq!(result, "speed != 0");
        assert!(!rewritten);
    }

    #[test]
    fn no_rewrite_less_equal() {
        let (result, rewritten) = rewrite_eval_expression("speed <= 100");
        assert_eq!(result, "speed <= 100");
        assert!(!rewritten);
    }

    #[test]
    fn no_rewrite_greater_equal() {
        let (result, rewritten) = rewrite_eval_expression("speed >= 0");
        assert_eq!(result, "speed >= 0");
        assert!(!rewritten);
    }

    #[test]
    fn rewrite_whitespace_handling() {
        let (result, rewritten) = rewrite_eval_expression("  speed  =  10  ");
        assert_eq!(result, "set(\"speed\", 10)");
        assert!(rewritten);
    }

    #[test]
    fn rewrite_complex_rhs() {
        let (result, rewritten) = rewrite_eval_expression("speed = clamp(speed + 10, 0, 100)");
        assert_eq!(result, "set(\"speed\", clamp(speed + 10, 0, 100))");
        assert!(rewritten);
    }

    // ── split_on_semicolons ─────────────────────────────────────────

    #[test]
    fn split_simple() {
        let parts = split_on_semicolons("a; b; c");
        assert_eq!(parts, vec!["a", " b", " c"]);
    }

    #[test]
    fn split_respects_strings() {
        let parts = split_on_semicolons("\"a;b\"; c");
        assert_eq!(parts, vec!["\"a;b\"", " c"]);
    }

    #[test]
    fn split_respects_parens() {
        let parts = split_on_semicolons("f(a; b); c");
        // semicolons inside parens don't split (even though invalid GDScript)
        assert_eq!(parts, vec!["f(a; b)", " c"]);
    }

    // ── build_set_expression ────────────────────────────────────────

    #[test]
    fn build_set_simple() {
        assert_eq!(build_set_expression("speed", "10"), "set(\"speed\", 10)");
    }

    #[test]
    fn build_set_indexed() {
        assert_eq!(
            build_set_expression("position.x", "5"),
            "set_indexed(\"position:x\", 5)"
        );
    }

    #[test]
    fn build_set_strips_self() {
        assert_eq!(
            build_set_expression("self.health", "100"),
            "set(\"health\", 100)"
        );
    }
}
