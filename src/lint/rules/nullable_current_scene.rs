use std::collections::HashSet;

use tree_sitter::Node;
use crate::core::gd_ast::GdFile;

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
        let root = file.node;

        // Check direct chained access: get_tree().current_scene.xxx()
        check_direct_access(root, source, &mut diags);

        // Check aliased access in each function body
        let mut cursor = root.walk();
        for child in root.children(&mut cursor) {
            if (child.kind() == "function_definition" || child.kind() == "constructor_definition")
                && let Some(body) = child.child_by_field_name("body")
            {
                check_aliased_access(body, source, &mut diags);
            }
        }

        diags
    }
}

/// Recursively find `get_tree().current_scene.xxx` chains that are not inside
/// a null-guard if-block.
fn check_direct_access(node: Node, source: &str, diags: &mut Vec<LintDiagnostic>) {
    if node.kind() == "attribute" && is_current_scene_chain(&node, source) {
        // Check this isn't inside a guarded block
        if !is_inside_current_scene_guard(&node, source) {
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
            return; // Don't recurse into children to avoid duplicate reports
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        check_direct_access(child, source, diags);
    }
}

/// Check if a node is an attribute chain of the form:
///   get_tree().current_scene.something
/// i.e., there is further access AFTER current_scene.
fn is_current_scene_chain(node: &Node, source: &str) -> bool {
    let src = source.as_bytes();

    // Must be an attribute node
    if node.kind() != "attribute" {
        return false;
    }

    let Ok(text) = node.utf8_text(src) else {
        return false;
    };

    // Quick check: must contain the pattern with a trailing dot
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
fn is_inside_current_scene_guard(node: &Node, source: &str) -> bool {
    let mut current = *node;
    while let Some(parent) = current.parent() {
        if parent.kind() == "if_statement" {
            // Check if the condition references current_scene
            if let Some(condition) = parent.named_child(0) {
                let Ok(cond_text) = condition.utf8_text(source.as_bytes()) else {
                    current = parent;
                    continue;
                };
                if cond_text.contains("current_scene") {
                    return true;
                }
            }
        }
        current = parent;
    }
    false
}

/// Check function body for aliased access pattern:
///   var scene = get_tree().current_scene
///   scene.method()  <-- flagged if no null check between
fn check_aliased_access(body: Node, source: &str, diags: &mut Vec<LintDiagnostic>) {
    let src = source.as_bytes();
    let mut aliases: Vec<(String, usize)> = Vec::new(); // (var_name, decl_line)

    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        if child.kind() == "variable_statement"
            && let Some(name_node) = child.child_by_field_name("name")
            && let Ok(var_name) = name_node.utf8_text(src)
            && let Some(value) = child.child_by_field_name("value")
            && let Ok(val_text) = value.utf8_text(src)
            && val_text == "get_tree().current_scene"
        {
            aliases.push((var_name.to_string(), child.start_position().row));
        }
    }

    for (alias, decl_line) in &aliases {
        // Collect lines guarded by null checks on the alias
        let guarded = collect_guarded_lines(body, source, alias);

        // Find the first unguarded alias.xxx access
        if let Some((line, col)) = find_alias_access(body, src, alias, *decl_line, &guarded) {
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

/// Collect line numbers inside if-blocks that guard the alias with a null check.
fn collect_guarded_lines(body: Node, source: &str, alias: &str) -> HashSet<usize> {
    let mut guarded = HashSet::new();
    let src = source.as_bytes();

    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        if child.kind() != "if_statement" {
            continue;
        }
        let Some(condition) = child.named_child(0) else {
            continue;
        };
        if !node_contains_identifier(condition, src, alias) {
            continue;
        }
        let start = child.start_position().row;
        let end = child.end_position().row;
        for line in start..=end {
            guarded.insert(line);
        }
    }

    guarded
}

fn node_contains_identifier(node: Node, source: &[u8], name: &str) -> bool {
    if node.kind() == "identifier" && node.utf8_text(source).ok() == Some(name) {
        return true;
    }
    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            if node_contains_identifier(cursor.node(), source, name) {
                return true;
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
    false
}

/// Find the first `alias.xxx` attribute access after `decl_line` that's not in a guarded block.
fn find_alias_access(
    node: Node,
    source: &[u8],
    alias: &str,
    decl_line: usize,
    guarded_lines: &HashSet<usize>,
) -> Option<(usize, usize)> {
    if node.start_position().row > decl_line
        && node.kind() == "attribute"
        && let Some(obj) = node.named_child(0)
        && obj.kind() == "identifier"
        && obj.utf8_text(source).ok() == Some(alias)
        && !guarded_lines.contains(&node.start_position().row)
    {
        return Some((node.start_position().row, node.start_position().column));
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            if let Some(pos) =
                find_alias_access(cursor.node(), source, alias, decl_line, guarded_lines)
            {
                return Some(pos);
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
    None
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
