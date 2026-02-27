use std::collections::HashSet;

use crate::core::gd_ast::{self, GdDecl, GdExpr, GdFile, GdStmt};

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct NullableCurrentScene;

impl LintRule for NullableCurrentScene {
    fn name(&self) -> &'static str {
        "nullable-current-scene"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Suspicious
    }

    fn check(&self, file: &GdFile<'_>, source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();

        // Check direct chained access: get_tree().current_scene.xxx()
        // Use the raw tree-sitter node for text-based matching (pattern is hard
        // to express structurally due to arbitrary nesting depth).
        check_direct_access(file.node, source, &mut diags);

        // Check aliased access in each function body
        gd_ast::visit_decls(file, &mut |decl| {
            if let GdDecl::Func(func) = decl {
                check_aliased_access(&func.body, source, &mut diags);
            }
        });

        diags
    }
}

/// Recursively find `get_tree().current_scene.xxx` chains that are not inside
/// a null-guard if-block. Uses raw tree-sitter nodes for text matching.
fn check_direct_access(
    node: tree_sitter::Node,
    source: &str,
    diags: &mut Vec<LintDiagnostic>,
) {
    if node.kind() == "attribute"
        && is_current_scene_chain(&node, source)
        && !is_inside_current_scene_guard(&node, source)
    {
        diags.push(LintDiagnostic {
            rule: "nullable-current-scene",
            message:
                "`get_tree().current_scene` can be null — add a null check before accessing"
                    .to_string(),
            severity: Severity::Warning,
            line: node.start_position().row,
            column: node.start_position().column,
            end_column: None,
            fix: None,
            context_lines: None,
        });
        return;
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        check_direct_access(child, source, diags);
    }
}

/// Check if a node is an attribute chain of the form:
///   get_tree().current_scene.something
fn is_current_scene_chain(node: &tree_sitter::Node, source: &str) -> bool {
    let src = source.as_bytes();
    if node.kind() != "attribute" {
        return false;
    }
    let Ok(text) = node.utf8_text(src) else {
        return false;
    };
    if !text.contains("get_tree().current_scene.") {
        return false;
    }
    // Ensure we're at the outermost access to avoid duplication
    if let Some(parent) = node.parent()
        && parent.kind() == "attribute"
        && let Ok(parent_text) = parent.utf8_text(src)
        && parent_text.contains("get_tree().current_scene.")
    {
        return false;
    }
    true
}

/// Walk ancestors to check if this node is inside an if-block that guards
/// against `current_scene` being null.
fn is_inside_current_scene_guard(node: &tree_sitter::Node, source: &str) -> bool {
    let mut current = *node;
    while let Some(parent) = current.parent() {
        if parent.kind() == "if_statement"
            && let Some(condition) = parent.named_child(0)
        {
            let Ok(cond_text) = condition.utf8_text(source.as_bytes()) else {
                current = parent;
                continue;
            };
            if cond_text.contains("current_scene") {
                return true;
            }
        }
        current = parent;
    }
    false
}

/// Check function body for aliased access pattern:
///   var scene = get_tree().current_scene
///   scene.method()  <-- flagged if no null check between
fn check_aliased_access(body: &[GdStmt], source: &str, diags: &mut Vec<LintDiagnostic>) {
    let mut aliases: Vec<(&str, usize)> = Vec::new();

    // Pass 1: find aliases (var scene = get_tree().current_scene)
    for stmt in body {
        if let GdStmt::Var(var) = stmt
            && let Some(value) = &var.value
            && is_get_tree_current_scene(value, source)
        {
            aliases.push((var.name, stmt.node().start_position().row));
        }
    }

    // Pass 2: for each alias, find unguarded access
    for &(alias, decl_line) in &aliases {
        let guarded = collect_guarded_lines(body, alias);

        if let Some((line, col)) =
            find_alias_access(body, source, alias, decl_line, &guarded)
        {
            diags.push(LintDiagnostic {
                rule: "nullable-current-scene",
                message: format!(
                    "`{alias}` holds `get_tree().current_scene` which can be null — \
                     add a null check before accessing"
                ),
                severity: Severity::Warning,
                line,
                column: col,
                end_column: None,
                fix: None,
                context_lines: None,
            });
        }
    }
}

/// Check if an expression is `get_tree().current_scene`.
fn is_get_tree_current_scene(expr: &GdExpr, source: &str) -> bool {
    // Use text matching for simplicity (same as the original)
    let text = expr.node().utf8_text(source.as_bytes()).unwrap_or("");
    text == "get_tree().current_scene"
}

/// Collect line numbers inside if-blocks that guard the alias with a null check.
fn collect_guarded_lines(body: &[GdStmt], alias: &str) -> HashSet<usize> {
    let mut guarded = HashSet::new();
    for stmt in body {
        if let GdStmt::If(if_stmt) = stmt
            && expr_contains_ident(&if_stmt.condition, alias)
        {
            let start = stmt.node().start_position().row;
            let end = stmt.node().end_position().row;
            for line in start..=end {
                guarded.insert(line);
            }
        }
    }
    guarded
}

/// Check if an expression tree contains an identifier with the given name.
fn expr_contains_ident(expr: &GdExpr, name: &str) -> bool {
    match expr {
        GdExpr::Ident { name: n, .. } if *n == name => true,
        GdExpr::BinOp { left, right, .. } => {
            expr_contains_ident(left, name) || expr_contains_ident(right, name)
        }
        GdExpr::UnaryOp { operand, .. } => expr_contains_ident(operand, name),
        GdExpr::Call { callee, args, .. } => {
            expr_contains_ident(callee, name)
                || args.iter().any(|a| expr_contains_ident(a, name))
        }
        GdExpr::MethodCall { receiver, args, .. } => {
            expr_contains_ident(receiver, name)
                || args.iter().any(|a| expr_contains_ident(a, name))
        }
        GdExpr::PropertyAccess { receiver, .. } => expr_contains_ident(receiver, name),
        _ => false,
    }
}

/// Find the first `alias.xxx` attribute access after `decl_line` that's not guarded.
fn find_alias_access(
    body: &[GdStmt],
    source: &str,
    alias: &str,
    decl_line: usize,
    guarded_lines: &HashSet<usize>,
) -> Option<(usize, usize)> {
    // Search all expressions in the body for PropertyAccess/MethodCall on the alias
    for stmt in body {
        let row = stmt.node().start_position().row;
        if row <= decl_line {
            continue;
        }
        if let Some(pos) = find_alias_in_expr_tree(stmt, source, alias, guarded_lines) {
            return Some(pos);
        }
    }
    None
}

/// Recursively search statement expressions for `alias.xxx` access.
fn find_alias_in_expr_tree(
    stmt: &GdStmt,
    source: &str,
    alias: &str,
    guarded_lines: &HashSet<usize>,
) -> Option<(usize, usize)> {
    let mut result = None;
    // Use the raw tree-sitter node to find attribute accesses on the alias
    find_alias_in_node(stmt.node(), source, alias, guarded_lines, &mut result);
    result
}

fn find_alias_in_node(
    node: tree_sitter::Node,
    source: &str,
    alias: &str,
    guarded_lines: &HashSet<usize>,
    result: &mut Option<(usize, usize)>,
) {
    if result.is_some() {
        return;
    }

    if node.kind() == "attribute"
        && let Some(obj) = node.named_child(0)
        && obj.kind() == "identifier"
        && obj.utf8_text(source.as_bytes()).ok() == Some(alias)
        && !guarded_lines.contains(&node.start_position().row)
    {
        *result = Some((node.start_position().row, node.start_position().column));
        return;
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        find_alias_in_node(child, source, alias, guarded_lines, result);
        if result.is_some() {
            return;
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
        NullableCurrentScene.check(&file, source, &config)
    }

    #[test]
    fn detects_direct_access() {
        let source = "\
extends Node
func f():
\tget_tree().current_scene.add_child(enemy)
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("current_scene"));
        assert!(diags[0].message.contains("null"));
    }

    #[test]
    fn detects_aliased_access() {
        let source = "\
extends Node
func f():
\tvar scene = get_tree().current_scene
\tscene.add_child(enemy)
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("scene"));
    }

    #[test]
    fn no_warning_with_null_guard() {
        let source = "\
extends Node
func f():
\tif get_tree().current_scene:
\t\tget_tree().current_scene.add_child(enemy)
";
        let diags = check(source);
        assert!(diags.is_empty());
    }

    #[test]
    fn no_warning_bare_access() {
        let source = "\
extends Node
func f():
\tvar scene = get_tree().current_scene
\tprint(scene)
";
        let diags = check(source);
        assert!(diags.is_empty());
    }

    #[test]
    fn no_warning_root_access() {
        let source = "\
extends Node
func f():
\tget_tree().root.add_child(node)
";
        let diags = check(source);
        assert!(diags.is_empty());
    }

    #[test]
    fn no_warning_aliased_with_guard() {
        let source = "\
extends Node
func f():
\tvar scene = get_tree().current_scene
\tif scene:
\t\tscene.add_child(enemy)
";
        let diags = check(source);
        assert!(diags.is_empty());
    }

    #[test]
    fn default_enabled() {
        assert!(NullableCurrentScene.default_enabled());
    }
}
