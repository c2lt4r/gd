use std::collections::HashSet;

use tree_sitter::Node;
use crate::core::gd_ast::GdFile;

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct NullAfterAwait;

impl LintRule for NullAfterAwait {
    fn name(&self) -> &'static str {
        "null-after-await"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Suspicious
    }

    fn default_enabled(&self) -> bool {
        false
    }

    fn check(&self, file: &GdFile<'_>, source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        let root = file.node;

        // Step 1: Collect nullable member vars
        let nullable_vars = collect_nullable_vars(root, source);
        if nullable_vars.is_empty() {
            return diags;
        }

        // Step 2: Find vars assigned after await in any function
        let vars_assigned_after_await =
            find_vars_assigned_after_await(root, source, &nullable_vars);
        if vars_assigned_after_await.is_empty() {
            return diags;
        }

        // Step 3: Check _process/_physics_process for unguarded access
        check_process_functions(root, source, &vars_assigned_after_await, &mut diags);

        diags
    }
}

fn collect_nullable_vars(root: Node, source: &str) -> HashSet<String> {
    let mut vars = HashSet::new();
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        if child.kind() != "variable_statement" {
            continue;
        }
        let Some(name_node) = child.child_by_field_name("name") else {
            continue;
        };
        let Ok(name) = name_node.utf8_text(source.as_bytes()) else {
            continue;
        };

        // No initializer or initialized to null
        match child.child_by_field_name("value") {
            None => {
                vars.insert(name.to_string());
            }
            Some(val) if val.kind() == "null" => {
                vars.insert(name.to_string());
            }
            _ => {}
        }
    }
    vars
}

fn find_vars_assigned_after_await(
    root: Node,
    source: &str,
    nullable_vars: &HashSet<String>,
) -> HashSet<String> {
    let mut result = HashSet::new();
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        if child.kind() != "function_definition" && child.kind() != "constructor_definition" {
            continue;
        }
        let Some(body) = child.child_by_field_name("body") else {
            continue;
        };

        // Check if function contains await
        if !contains_await(body) {
            continue;
        }

        // Find assignments to nullable vars after an await
        collect_assignments_after_await(body, source, nullable_vars, &mut result);
    }
    result
}

fn contains_await(node: Node) -> bool {
    if node.kind() == "await" {
        return true;
    }
    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            if contains_await(cursor.node()) {
                return true;
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
    false
}

fn collect_assignments_after_await(
    body: Node,
    source: &str,
    nullable_vars: &HashSet<String>,
    result: &mut HashSet<String>,
) {
    let mut seen_await = false;
    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        if !seen_await {
            if contains_await(child) {
                seen_await = true;
            }
            continue;
        }
        // After await: check for assignment to nullable var
        check_assignment(child, source, nullable_vars, result);
    }
}

fn check_assignment(
    node: Node,
    source: &str,
    nullable_vars: &HashSet<String>,
    result: &mut HashSet<String>,
) {
    if (node.kind() == "assignment" || node.kind() == "augmented_assignment")
        && let Some(lhs) = node.named_child(0)
        && let Ok(name) = lhs.utf8_text(source.as_bytes())
        && nullable_vars.contains(name)
    {
        result.insert(name.to_string());
    }
    // Recurse into children (if/match blocks)
    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            check_assignment(cursor.node(), source, nullable_vars, result);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

fn check_process_functions(
    root: Node,
    source: &str,
    risky_vars: &HashSet<String>,
    diags: &mut Vec<LintDiagnostic>,
) {
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        if child.kind() != "function_definition" {
            continue;
        }
        let Some(name_node) = child.child_by_field_name("name") else {
            continue;
        };
        let Ok(func_name) = name_node.utf8_text(source.as_bytes()) else {
            continue;
        };

        if func_name != "_process" && func_name != "_physics_process" {
            continue;
        }

        let Some(body) = child.child_by_field_name("body") else {
            continue;
        };

        // Find usages of risky vars that aren't inside a null guard
        check_unguarded_access(body, source, risky_vars, diags);
    }
}

fn check_unguarded_access(
    body: Node,
    source: &str,
    risky_vars: &HashSet<String>,
    diags: &mut Vec<LintDiagnostic>,
) {
    let guarded = collect_guarded_vars(body, source);
    find_unguarded(body, source, risky_vars, &guarded, diags);
}

fn collect_guarded_vars(body: Node, source: &str) -> HashSet<String> {
    let mut guarded = HashSet::new();
    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        if child.kind() == "if_statement" {
            // Check condition for `if var:` or `if var != null:` or `if is_instance_valid(var):`
            if let Some(condition) = child.named_child(0) {
                for var_name in collect_identifiers_in(condition, source) {
                    guarded.insert(var_name);
                }
            }
        }
    }
    guarded
}

fn collect_identifiers_in(node: Node, source: &str) -> Vec<String> {
    let mut ids = Vec::new();
    if node.kind() == "identifier"
        && let Ok(text) = node.utf8_text(source.as_bytes())
    {
        ids.push(text.to_string());
    }
    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            ids.extend(collect_identifiers_in(cursor.node(), source));
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
    ids
}

fn find_unguarded(
    node: Node,
    source: &str,
    risky_vars: &HashSet<String>,
    guarded: &HashSet<String>,
    diags: &mut Vec<LintDiagnostic>,
) {
    // Skip if-statement bodies where the condition guards the var
    if node.kind() == "if_statement" {
        return;
    }

    if node.kind() == "identifier" {
        if let Ok(name) = node.utf8_text(source.as_bytes())
            && risky_vars.contains(name)
            && !guarded.contains(name)
        {
            diags.push(LintDiagnostic {
                rule: "null-after-await",
                message: format!(
                    "`{name}` may be null after `await` — add a null check before using"
                ),
                severity: Severity::Warning,
                line: node.start_position().row,
                column: node.start_position().column,
                end_column: Some(node.end_position().column),
                fix: None,
                context_lines: None,
            });
        }
        return;
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            find_unguarded(cursor.node(), source, risky_vars, guarded, diags);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::parser;
    use crate::core::gd_ast;

    fn check(source: &str) -> Vec<LintDiagnostic> {
        let tree = parser::parse(source).unwrap();
        let file = gd_ast::convert(&tree, source);
        let config = LintConfig::default();
        NullAfterAwait.check(&file, source, &config)
    }

    #[test]
    fn detects_unguarded_access_after_await() {
        let source = "\
var enemy = null

func load_enemy():
\tawait get_tree().create_timer(1.0).timeout
\tenemy = preload(\"res://enemy.tscn\").instantiate()

func _process(delta):
\tenemy.move()
";
        let diags = check(source);
        assert!(!diags.is_empty());
        assert!(diags[0].message.contains("enemy"));
        assert!(diags[0].message.contains("null"));
    }

    #[test]
    fn no_warning_with_null_guard() {
        let source = "\
var enemy = null

func load_enemy():
\tawait get_tree().create_timer(1.0).timeout
\tenemy = preload(\"res://enemy.tscn\").instantiate()

func _process(delta):
\tif enemy:
\t\tenemy.move()
";
        let diags = check(source);
        assert!(diags.is_empty());
    }

    #[test]
    fn no_warning_var_with_initializer() {
        let source = "\
var enemy = Node2D.new()

func load_enemy():
\tawait get_tree().create_timer(1.0).timeout
\tenemy = preload(\"res://enemy.tscn\").instantiate()

func _process(delta):
\tenemy.move()
";
        let diags = check(source);
        assert!(diags.is_empty());
    }

    #[test]
    fn no_warning_without_await() {
        let source = "\
var enemy = null

func load_enemy():
\tenemy = preload(\"res://enemy.tscn\").instantiate()

func _process(delta):
\tenemy.move()
";
        let diags = check(source);
        assert!(diags.is_empty());
    }

    #[test]
    fn no_warning_no_process_function() {
        let source = "\
var enemy = null

func load_enemy():
\tawait get_tree().create_timer(1.0).timeout
\tenemy = preload(\"res://enemy.tscn\").instantiate()

func update():
\tenemy.move()
";
        let diags = check(source);
        assert!(diags.is_empty());
    }

    #[test]
    fn detects_in_physics_process() {
        let source = "\
var enemy = null

func load_enemy():
\tawait get_tree().create_timer(1.0).timeout
\tenemy = preload(\"res://enemy.tscn\").instantiate()

func _physics_process(delta):
\tenemy.move()
";
        let diags = check(source);
        assert!(!diags.is_empty());
    }

    #[test]
    fn opt_in_rule() {
        assert!(!NullAfterAwait.default_enabled());
    }
}
