use tree_sitter::Node;
use crate::core::gd_ast::{self, GdDecl, GdFile};

use super::{LintCategory, LintDiagnostic, LintRule};
use crate::core::config::LintConfig;

/// Detects `add_child()` calls where the node was created with `.new()` but
/// never had `.name` set. Without an explicit name, the node appears as
/// `@ClassName@123` in the scene tree, making debugging harder.
pub struct UnnamedNode;

impl LintRule for UnnamedNode {
    fn name(&self) -> &'static str {
        "unnamed-node"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Godot
    }

    fn default_enabled(&self) -> bool {
        false
    }

    fn check(&self, file: &GdFile<'_>, source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        gd_ast::visit_decls(file, &mut |decl| {
            if let GdDecl::Func(func) = decl
                && let Some(body) = func.node.child_by_field_name("body")
            {
                check_body(&body, source, &mut diags);
            }
        });
        diags
    }
}

/// Scan a function body for `add_child(x)` where `x` was `.new()`'d without `.name` set.
fn check_body(body: &Node, source: &str, diags: &mut Vec<LintDiagnostic>) {
    // Track variables: name -> (type_name, line, was_name_set)
    let mut tracked: std::collections::HashMap<String, (String, usize, bool)> =
        std::collections::HashMap::new();
    walk_body(body, source, &mut tracked, diags);
}

/// Recursively walk statements, tracking `.new()` assignments and flagging unnamed `add_child`.
fn walk_body(
    node: &Node,
    source: &str,
    tracked: &mut std::collections::HashMap<String, (String, usize, bool)>,
    diags: &mut Vec<LintDiagnostic>,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "expression_statement" | "variable_statement" | "assignment_statement" => {
                process_statement(&child, source, tracked, diags);
            }
            "if_statement" | "for_statement" | "while_statement" | "match_statement" | "body"
            | "block" | "match_body" | "match_arm" | "pattern_guard" | "elif_branch"
            | "else_branch" => {
                walk_body(&child, source, tracked, diags);
            }
            _ => {
                if child.named_child_count() > 0
                    && !matches!(
                        child.kind(),
                        "call" | "attribute_call" | "attribute" | "string" | "binary_operator"
                    )
                {
                    walk_body(&child, source, tracked, diags);
                }
            }
        }
    }
}

/// Process a single statement node for tracking and diagnostics.
fn process_statement(
    stmt: &Node,
    source: &str,
    tracked: &mut std::collections::HashMap<String, (String, usize, bool)>,
    diags: &mut Vec<LintDiagnostic>,
) {
    // 1. Detect `var x = SomeType.new()` or `x = SomeType.new()`
    if let Some((var_name, type_name)) = extract_new_assignment(stmt, source)
        && is_node_class(&type_name)
    {
        tracked.insert(var_name, (type_name, stmt.start_position().row + 1, false));
    }

    // 2. Detect `x.name = ...`
    if let Some(var_name) = extract_name_assignment(stmt, source)
        && let Some(entry) = tracked.get_mut(&var_name)
    {
        entry.2 = true;
    }

    // 3. Detect `add_child(x)` or `something.add_child(x)`
    if let Some((var_name, row, col)) = extract_add_child_arg(stmt, source) {
        emit_unnamed_diag(&var_name, row, col, tracked, diags);
    }

    // 4. Detect `call_deferred("add_child", x)`
    if let Some((var_name, row, col)) = extract_deferred_add_child(stmt, source) {
        emit_unnamed_diag(&var_name, row, col, tracked, diags);
    }
}

/// Emit a diagnostic if the variable is tracked and was never named.
fn emit_unnamed_diag(
    var_name: &str,
    row: usize,
    col: usize,
    tracked: &std::collections::HashMap<String, (String, usize, bool)>,
    diags: &mut Vec<LintDiagnostic>,
) {
    if let Some((type_name, _, name_was_set)) = tracked.get(var_name)
        && !name_was_set
    {
        diags.push(LintDiagnostic {
            rule: "unnamed-node",
            message: format!(
                "`{var_name}` ({type_name}) is added to the scene tree without \
                 setting `.name` — it will appear as @{type_name}@ in the debugger"
            ),
            line: row + 1,
            column: col + 1,
            end_column: None,
            severity: super::Severity::Warning,
            fix: None,
            context_lines: None,
        });
    }
}

/// Extract `(var_name, type_name)` from `var x = SomeType.new()` or `var x := SomeType.new()`.
/// Also handles `x = SomeType.new()` reassignments.
fn extract_new_assignment(node: &Node, source: &str) -> Option<(String, String)> {
    match node.kind() {
        "variable_statement" => {
            // var x = Type.new() — value field holds the RHS
            let name_node = node.child_by_field_name("name")?;
            let value_node = node.child_by_field_name("value")?;
            let var_name = name_node.utf8_text(source.as_bytes()).ok()?;
            let type_name = extract_new_call_type(&value_node, source)?;
            Some((var_name.to_string(), type_name))
        }
        "expression_statement" => {
            // x = Type.new() — expression_statement > assignment
            let child = node.named_child(0)?;
            if child.kind() == "assignment" {
                let left = child.named_child(0)?;
                let right = child.named_child(1)?;
                if left.kind() == "identifier" {
                    let var_name = left.utf8_text(source.as_bytes()).ok()?;
                    let type_name = extract_new_call_type(&right, source)?;
                    return Some((var_name.to_string(), type_name));
                }
            }
            None
        }
        _ => None,
    }
}

/// Extract the class name from a `SomeType.new()` call node.
/// Returns None for `preload(...).new()`, `.instantiate()`, or non-`.new()` calls.
///
/// AST for `Button.new()`:
///   attribute
///     identifier "Button"     (named_child 0)
///     attribute_call           (named_child 1)
///       identifier "new"
///       arguments
fn extract_new_call_type(node: &Node, source: &str) -> Option<String> {
    if node.kind() != "attribute" {
        return None;
    }
    let object = node.named_child(0)?;
    let call_part = node.named_child(1)?;
    if call_part.kind() != "attribute_call" {
        return None;
    }
    let method = call_part.named_child(0)?;
    let method_name = method.utf8_text(source.as_bytes()).ok()?;
    if method_name == "new" && object.kind() == "identifier" {
        let class_name = object.utf8_text(source.as_bytes()).ok()?;
        return Some(class_name.to_string());
    }
    None
}

/// Detect `x.name = ...` pattern. Returns the variable name.
///
/// AST for `btn.name = "X"`:
///   expression_statement
///     assignment
///       attribute              (named_child 0 = left)
///         identifier "btn"
///         identifier "name"
///       string "X"             (named_child 1 = right)
fn extract_name_assignment(node: &Node, source: &str) -> Option<String> {
    if node.kind() != "expression_statement" {
        return None;
    }
    let child = node.named_child(0)?;
    if child.kind() != "assignment" {
        return None;
    }
    let left = child.named_child(0)?;
    if left.kind() == "attribute" {
        let object = left.named_child(0)?;
        let attr = left.named_child(1)?;
        if object.kind() == "identifier" && attr.kind() == "identifier" {
            let attr_name = attr.utf8_text(source.as_bytes()).ok()?;
            if attr_name == "name" {
                let var_name = object.utf8_text(source.as_bytes()).ok()?;
                return Some(var_name.to_string());
            }
        }
    }
    None
}

/// Extract method name and arguments node from a call expression.
/// Returns (method_name, arguments_node_index_in_parent, row, col).
///
/// `call { identifier, arguments }` → method name from identifier
/// `attribute { object, attribute_call { identifier, arguments } }` → method name from inner identifier
fn get_method_and_args<'a>(node: &'a Node<'a>, source: &'a str) -> Option<(&'a str, Node<'a>)> {
    match node.kind() {
        "call" => {
            let func = node.named_child(0)?;
            if func.kind() != "identifier" {
                return None;
            }
            let name = func.utf8_text(source.as_bytes()).ok()?;
            let args = node.named_child(1)?;
            if args.kind() != "arguments" {
                return None;
            }
            Some((name, args))
        }
        "attribute" => {
            let call_part = node.named_child(1)?;
            if call_part.kind() != "attribute_call" {
                return None;
            }
            let method = call_part.named_child(0)?;
            let name = method.utf8_text(source.as_bytes()).ok()?;
            let args = call_part.named_child(1)?;
            if args.kind() != "arguments" {
                return None;
            }
            Some((name, args))
        }
        _ => None,
    }
}

/// Detect `add_child(x)` or `something.add_child(x)`. Returns (arg_name, row, col).
fn extract_add_child_arg(node: &Node, source: &str) -> Option<(String, usize, usize)> {
    if node.kind() != "expression_statement" {
        return None;
    }
    let call_node = node.named_child(0)?;
    let (func_name, args_node) = get_method_and_args(&call_node, source)?;
    if func_name != "add_child" {
        return None;
    }
    let first_arg = args_node.named_child(0)?;
    if first_arg.kind() == "identifier" {
        let arg_name = first_arg.utf8_text(source.as_bytes()).ok()?;
        let row = call_node.start_position().row;
        let col = call_node.start_position().column;
        return Some((arg_name.to_string(), row, col));
    }
    None
}

/// Detect `call_deferred("add_child", x)` or `something.call_deferred("add_child", x)`.
fn extract_deferred_add_child(node: &Node, source: &str) -> Option<(String, usize, usize)> {
    if node.kind() != "expression_statement" {
        return None;
    }
    let call_node = node.named_child(0)?;
    let (func_name, args_node) = get_method_and_args(&call_node, source)?;
    if func_name != "call_deferred" {
        return None;
    }
    // First arg should be "add_child"
    let first_arg = args_node.named_child(0)?;
    let arg_text = first_arg.utf8_text(source.as_bytes()).ok()?;
    let unquoted = arg_text.trim_matches('"').trim_matches('\'');
    if unquoted != "add_child" {
        return None;
    }
    // Second arg is the node being added
    let second_arg = args_node.named_child(1)?;
    if second_arg.kind() == "identifier" {
        let arg_name = second_arg.utf8_text(source.as_bytes()).ok()?;
        let row = call_node.start_position().row;
        let col = call_node.start_position().column;
        return Some((arg_name.to_string(), row, col));
    }
    None
}

/// Check if a class name is a Node subclass (has `.name` property).
fn is_node_class(name: &str) -> bool {
    crate::class_db::inherits(name, "Node") || name == "Node"
}

#[cfg(test)]
mod tests {
    use crate::lint::rules::LintRule;

    use super::*;

    fn check(source: &str) -> Vec<LintDiagnostic> {
        let tree = crate::core::parser::parse(source).unwrap();
        let file = crate::core::gd_ast::convert(&tree, source);
        let config = LintConfig::default();
        UnnamedNode.check(&file, source, &config)
    }

    #[test]
    fn flags_add_child_without_name() {
        let diags = check("func _ready():\n\tvar btn = Button.new()\n\tadd_child(btn)\n");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("btn"));
        assert!(diags[0].message.contains("Button"));
    }

    #[test]
    fn no_flag_when_name_is_set() {
        let diags = check(
            "func _ready():\n\tvar btn = Button.new()\n\tbtn.name = \"Submit\"\n\tadd_child(btn)\n",
        );
        assert!(diags.is_empty());
    }

    #[test]
    fn no_flag_for_non_node_class() {
        let diags = check(
            "func _ready():\n\tvar style = StyleBoxFlat.new()\n\tadd_theme_stylebox_override(\"panel\", style)\n",
        );
        assert!(diags.is_empty());
    }

    #[test]
    fn flags_method_call_add_child() {
        let diags = check("func _ready():\n\tvar lbl = Label.new()\n\tparent.add_child(lbl)\n");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_deferred_add_child() {
        let diags = check(
            "func _ready():\n\tvar timer = Timer.new()\n\tget_root().call_deferred(\"add_child\", timer)\n",
        );
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn no_flag_deferred_with_name() {
        let diags = check(
            "func _ready():\n\tvar timer = Timer.new()\n\ttimer.name = \"MyTimer\"\n\tget_root().call_deferred(\"add_child\", timer)\n",
        );
        assert!(diags.is_empty());
    }

    #[test]
    fn no_flag_for_instantiate() {
        // .instantiate() returns a scene instance which already has a name
        let diags =
            check("func _ready():\n\tvar npc = npc_scene.instantiate()\n\tadd_child(npc)\n");
        assert!(diags.is_empty());
    }

    #[test]
    fn no_flag_for_factory_function() {
        // Can't trace into function returns, so skip
        let diags = check("func _ready():\n\tvar btn = _create_button(\"OK\")\n\tadd_child(btn)\n");
        assert!(diags.is_empty());
    }

    #[test]
    fn no_flag_for_preload_new() {
        // preload("...").new() — can't know the class, skip
        let diags = check(
            "func _ready():\n\tvar mgr = preload(\"res://manager.gd\").new()\n\tadd_child(mgr)\n",
        );
        assert!(diags.is_empty());
    }

    #[test]
    fn flags_multiple_unnamed_nodes() {
        let diags = check(
            "func _build():\n\tvar a = Label.new()\n\tvar b = Button.new()\n\tadd_child(a)\n\tadd_child(b)\n",
        );
        assert_eq!(diags.len(), 2);
    }

    #[test]
    fn flags_only_unnamed_in_mixed() {
        let diags = check(
            "func _build():\n\tvar a = Label.new()\n\ta.name = \"Title\"\n\tvar b = Button.new()\n\tadd_child(a)\n\tadd_child(b)\n",
        );
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("Button"));
    }

    #[test]
    fn flags_inside_if_block() {
        let diags =
            check("func _ready():\n\tvar btn = Button.new()\n\tif true:\n\t\tadd_child(btn)\n");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn no_flag_for_non_node_resource() {
        // RandomNumberGenerator extends RefCounted, not Node
        let diags = check("func _ready():\n\tvar rng = RandomNumberGenerator.new()\n");
        assert!(diags.is_empty());
    }

    #[test]
    fn flags_walrus_operator() {
        // var bg := ColorRect.new() — uses := instead of =
        let diags = check("func _build():\n\tvar bg := ColorRect.new()\n\tadd_child(bg)\n");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("ColorRect"));
    }

    #[test]
    fn flags_member_assignment() {
        // _opp_name = Label.new() — member variable, no var keyword
        let diags = check("func _build():\n\t_opp_name = Label.new()\n\tadd_child(_opp_name)\n");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("Label"));
    }

    #[test]
    fn no_flag_member_with_name() {
        let diags = check(
            "func _build():\n\t_label = Label.new()\n\t_label.name = \"Title\"\n\tadd_child(_label)\n",
        );
        assert!(diags.is_empty());
    }

    #[test]
    fn flags_parent_add_child() {
        // parent.add_child(info) — method call style
        let diags = check(
            "func _build(parent: Control):\n\tvar info := PanelContainer.new()\n\tparent.add_child(info)\n",
        );
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn no_flag_when_not_added() {
        // Node created but never add_child'd — not our problem
        let diags = check("func _ready():\n\tvar btn = Button.new()\n\tbtn.text = \"Hi\"\n");
        assert!(diags.is_empty());
    }
}
