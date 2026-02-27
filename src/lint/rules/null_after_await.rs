use std::collections::HashSet;

use tree_sitter::Node;
use crate::core::gd_ast::{GdDecl, GdExpr, GdFile};

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

        // Step 1: Collect nullable member vars (no init or init to null)
        let mut nullable_vars = HashSet::new();
        for decl in &file.declarations {
            if let GdDecl::Var(var) = decl
                && matches!(&var.value, None | Some(GdExpr::Null { .. }))
            {
                nullable_vars.insert(var.name.to_string());
            }
        }
        if nullable_vars.is_empty() {
            return diags;
        }

        // Step 2: Find vars assigned after await in any function
        let mut vars_assigned_after_await = HashSet::new();
        for decl in &file.declarations {
            if let GdDecl::Func(func) = decl
                && let Some(body) = func.node.child_by_field_name("body")
                && contains_await(body)
            {
                collect_assignments_after_await(body, source, &nullable_vars, &mut vars_assigned_after_await);
            }
        }
        if vars_assigned_after_await.is_empty() {
            return diags;
        }

        // Step 3: Check _process/_physics_process for unguarded access
        for decl in &file.declarations {
            if let GdDecl::Func(func) = decl
                && matches!(func.name, "_process" | "_physics_process")
                && let Some(body) = func.node.child_by_field_name("body")
            {
                check_unguarded_access(body, source, &vars_assigned_after_await, &mut diags);
            }
        }

        diags
    }
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
