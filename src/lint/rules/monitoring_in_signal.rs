use std::collections::HashSet;
use tree_sitter::{Node, Tree};

use super::{LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct MonitoringInSignal;

/// Signals that trigger during physics overlap resolution where modifying
/// `monitoring` or `monitorable` causes a Godot runtime error.
const AREA_SIGNALS: &[&str] = &[
    "body_entered",
    "body_exited",
    "area_entered",
    "area_exited",
    "body_shape_entered",
    "body_shape_exited",
    "area_shape_entered",
    "area_shape_exited",
];

/// Properties that must not be directly assigned inside these callbacks.
const DANGEROUS_PROPS: &[&str] = &["monitoring", "monitorable"];

impl LintRule for MonitoringInSignal {
    fn name(&self) -> &'static str {
        "monitoring-in-signal"
    }

    fn check(&self, tree: &Tree, source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();

        // 1. Collect callback function names connected to area signals
        let mut signal_callbacks: HashSet<String> = HashSet::new();
        collect_signal_callbacks(tree.root_node(), source, &mut signal_callbacks);

        // 2. Also check functions whose name matches the Godot auto-connect pattern:
        //    _on_<NodeName>_<signal> e.g. _on_Area2D_body_entered
        // We don't add these to signal_callbacks because we check them below.

        // 3. Check each matching function body for direct monitoring/monitorable assignment
        check_functions(tree.root_node(), source, &signal_callbacks, &mut diags);

        diags
    }
}

/// Find `signal.connect(callback)` patterns and collect callback names.
fn collect_signal_callbacks(node: Node, source: &str, callbacks: &mut HashSet<String>) {
    // Walk all expression_statement nodes looking for signal.connect(func_name)
    if (node.kind() == "expression_statement" || node.kind() == "attribute")
        && let Some(cb) = extract_signal_connect(&node, source)
    {
        callbacks.insert(cb);
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            collect_signal_callbacks(cursor.node(), source, callbacks);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

/// Try to extract a callback name from `signal.connect(callback)`.
/// Pattern: attribute > [identifier(signal_name), ".", attribute_call > [identifier("connect"), arguments > [identifier(callback)]]]
fn extract_signal_connect(node: &Node, source: &str) -> Option<String> {
    let attr = if node.kind() == "attribute" {
        *node
    } else {
        // expression_statement may wrap an attribute
        let mut cursor = node.walk();
        let mut found = None;
        for child in node.children(&mut cursor) {
            if child.kind() == "attribute" {
                found = Some(child);
                break;
            }
        }
        found?
    };

    // First named child should be the signal name
    let signal_id = attr.named_child(0)?;
    if signal_id.kind() != "identifier" {
        return None;
    }
    let signal_name = signal_id.utf8_text(source.as_bytes()).ok()?;

    if !AREA_SIGNALS.contains(&signal_name) {
        return None;
    }

    // Find attribute_call with "connect"
    let mut cursor = attr.walk();
    for child in attr.children(&mut cursor) {
        if child.kind() == "attribute_call" {
            let method_name = child.named_child(0)?;
            if method_name.utf8_text(source.as_bytes()).ok()? != "connect" {
                return None;
            }
            // Get first argument (the callback)
            let mut inner = child.walk();
            for inner_child in child.children(&mut inner) {
                if inner_child.kind() == "arguments" {
                    let arg = inner_child.named_child(0)?;
                    if arg.kind() == "identifier" {
                        return arg.utf8_text(source.as_bytes()).ok().map(String::from);
                    }
                }
            }
        }
    }

    None
}

/// Check function bodies for direct monitoring/monitorable assignments.
fn check_functions(
    node: Node,
    source: &str,
    signal_callbacks: &HashSet<String>,
    diags: &mut Vec<LintDiagnostic>,
) {
    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            let child = cursor.node();
            match child.kind() {
                "function_definition" => {
                    if let Some(name_node) = child.child_by_field_name("name")
                        && let Ok(func_name) = name_node.utf8_text(source.as_bytes())
                    {
                        let is_connected = signal_callbacks.contains(func_name);
                        let is_auto = is_auto_connected_signal_handler(func_name);

                        if (is_connected || is_auto)
                            && let Some(body) = child.child_by_field_name("body")
                        {
                            check_body_for_monitoring(body, source, func_name, diags);
                        }
                    }
                }
                "class_definition" => {
                    if let Some(body) = child.child_by_field_name("body") {
                        check_functions(body, source, signal_callbacks, diags);
                    }
                }
                _ => {}
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

/// Check if function name matches Godot auto-connect pattern: _on_*_<signal>
fn is_auto_connected_signal_handler(name: &str) -> bool {
    if !name.starts_with("_on_") {
        return false;
    }
    let suffix = &name[4..]; // strip "_on_"
    // Check if it ends with an area signal name after a separator
    for signal in AREA_SIGNALS {
        // _on_Area2D_body_entered or _on_body_entered
        if suffix.ends_with(signal)
            && (suffix.len() == signal.len()
                || suffix.as_bytes()[suffix.len() - signal.len() - 1] == b'_')
        {
            return true;
        }
    }
    false
}

/// Scan a function body for `monitoring = ...` or `monitorable = ...` direct assignments.
fn check_body_for_monitoring(
    body: Node,
    source: &str,
    func_name: &str,
    diags: &mut Vec<LintDiagnostic>,
) {
    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        check_statement_for_monitoring(&child, source, func_name, diags);
    }
}

fn check_statement_for_monitoring(
    node: &Node,
    source: &str,
    func_name: &str,
    diags: &mut Vec<LintDiagnostic>,
) {
    // expression_statement wraps assignment: expression_statement > assignment > [lhs, =, rhs]
    if node.kind() == "expression_statement" {
        let mut inner = node.walk();
        for child in node.children(&mut inner) {
            if child.kind() == "assignment" {
                emit_if_monitoring(&child, source, func_name, diags);
            }
        }
    }

    // Recurse into control flow
    match node.kind() {
        "if_statement" | "for_statement" | "while_statement" | "match_statement" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "body" || child.kind() == "match_body" {
                    check_body_for_monitoring(child, source, func_name, diags);
                }
                if (child.kind() == "elif_branch" || child.kind() == "else_branch")
                    && let Some(body) = child.child_by_field_name("body")
                {
                    check_body_for_monitoring(body, source, func_name, diags);
                }
            }
        }
        _ => {}
    }
}

fn emit_if_monitoring(
    assignment: &Node,
    source: &str,
    func_name: &str,
    diags: &mut Vec<LintDiagnostic>,
) {
    if let Some((prop, prop_node)) = extract_monitoring_assignment(assignment, source) {
        diags.push(LintDiagnostic {
            rule: "monitoring-in-signal",
            message: format!(
                "direct assignment to `{prop}` in signal callback `{func_name}()`; \
                 use `set_deferred(\"{prop}\", value)` instead"
            ),
            severity: Severity::Warning,
            line: prop_node.start_position().row,
            column: prop_node.start_position().column,
            end_column: Some(prop_node.end_position().column),
            fix: None,
            context_lines: None,
        });
    }
}

/// Extract property name from `monitoring = X`, `self.monitoring = X`, etc.
/// Returns (property_name, assignment_node) if it's a dangerous prop.
fn extract_monitoring_assignment<'a>(
    node: &'a Node<'a>,
    source: &str,
) -> Option<(String, Node<'a>)> {
    // assignment: LHS = RHS
    if node.kind() != "assignment" {
        return None;
    }

    let lhs = node.named_child(0)?;
    let prop_name = match lhs.kind() {
        "identifier" => lhs.utf8_text(source.as_bytes()).ok()?,
        // self.monitoring
        "attribute" => {
            let first = lhs.named_child(0)?;
            if first.kind() != "identifier" {
                return None;
            }
            let obj = first.utf8_text(source.as_bytes()).ok()?;
            if obj != "self" {
                return None;
            }
            // Get the property name after the dot
            let mut cursor = lhs.walk();
            let mut prop = None;
            for child in lhs.children(&mut cursor) {
                // The property is the second identifier (after self and .)
                if child.kind() == "identifier" && child != first {
                    prop = child.utf8_text(source.as_bytes()).ok();
                }
            }
            prop?
        }
        _ => return None,
    };

    if DANGEROUS_PROPS.contains(&prop_name) {
        Some((prop_name.to_string(), *node))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::parser;

    fn check(source: &str) -> Vec<LintDiagnostic> {
        let tree = parser::parse(source).unwrap();
        let config = LintConfig::default();
        MonitoringInSignal.check(&tree, source, &config)
    }

    #[test]
    fn detects_monitoring_in_connected_callback() {
        let source = "\
func _ready():
\tbody_entered.connect(_on_body_entered)

func _on_body_entered(body):
\tmonitoring = false
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("monitoring"));
        assert!(diags[0].message.contains("set_deferred"));
    }

    #[test]
    fn detects_self_monitoring_assignment() {
        let source = "\
func _ready():
\tbody_exited.connect(_on_exit)

func _on_exit(body):
\tself.monitoring = false
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("monitoring"));
    }

    #[test]
    fn detects_monitorable_assignment() {
        let source = "\
func _ready():
\tarea_entered.connect(_on_area)

func _on_area(area):
\tmonitorable = false
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("monitorable"));
    }

    #[test]
    fn no_warning_with_set_deferred() {
        let source = "\
func _ready():
\tbody_entered.connect(_on_body_entered)

func _on_body_entered(body):
\tset_deferred(\"monitoring\", false)
";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_unrelated_function() {
        let source = "\
func _ready():
\tbody_entered.connect(_on_body_entered)

func _on_body_entered(body):
\tpass

func some_other_func():
\tmonitoring = false
";
        assert!(check(source).is_empty());
    }

    #[test]
    fn detects_auto_connected_handler() {
        let source = "\
func _on_Area2D_body_entered(body):
\tmonitoring = false
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("monitoring"));
    }

    #[test]
    fn detects_in_conditional() {
        let source = "\
func _ready():
\tbody_entered.connect(_on_hit)

func _on_hit(body):
\tif body.is_in_group(\"enemy\"):
\t\tmonitoring = false
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn no_warning_non_area_signal() {
        let source = "\
func _ready():
\ttimeout.connect(_on_timeout)

func _on_timeout():
\tmonitoring = false
";
        assert!(check(source).is_empty());
    }

    #[test]
    fn detects_area_exited_signal() {
        let source = "\
func _ready():
\tarea_exited.connect(_cleanup)

func _cleanup(area):
\tmonitoring = false
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn default_enabled() {
        assert!(MonitoringInSignal.default_enabled());
    }
}
