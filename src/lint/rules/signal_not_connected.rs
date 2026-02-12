use std::collections::{HashMap, HashSet};
use tree_sitter::{Node, Tree};

use super::{LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct SignalNotConnected;

impl LintRule for SignalNotConnected {
    fn name(&self) -> &'static str {
        "signal-not-connected"
    }

    fn default_enabled(&self) -> bool {
        false
    }

    fn check(&self, tree: &Tree, source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        let root = tree.root_node();
        let src = source.as_bytes();

        // Collect signal declarations: name → (line, column)
        let mut signals: HashMap<String, (usize, usize)> = HashMap::new();
        collect_signals(root, src, &mut signals);

        if signals.is_empty() {
            return diags;
        }

        // Collect signals that are emitted and connected
        let mut emitted: HashSet<String> = HashSet::new();
        let mut connected: HashSet<String> = HashSet::new();
        collect_usage(root, src, &mut emitted, &mut connected);

        // Report signals that are emitted but never connected in this file
        for (name, (line, column)) in &signals {
            if emitted.contains(name) && !connected.contains(name) {
                diags.push(LintDiagnostic {
                    rule: "signal-not-connected",
                    message: format!(
                        "signal `{}` is emitted but never connected in this file",
                        name
                    ),
                    severity: Severity::Info,
                    line: *line,
                    column: *column,
                    end_column: None,
                    fix: None,
                    context_lines: None,
                });
            }
        }

        diags
    }
}

fn collect_signals(node: Node, src: &[u8], signals: &mut HashMap<String, (usize, usize)>) {
    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            let child = cursor.node();
            if child.kind() == "signal_statement"
                && let Some(name_node) = child.child_by_field_name("name")
                && let Ok(name) = name_node.utf8_text(src)
            {
                signals.entry(name.to_string()).or_insert((
                    name_node.start_position().row,
                    name_node.start_position().column,
                ));
            }
            collect_signals(child, src, signals);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

fn collect_usage(
    node: Node,
    src: &[u8],
    emitted: &mut HashSet<String>,
    connected: &mut HashSet<String>,
) {
    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            let child = cursor.node();

            // Modern GDScript: signal_name.emit() / .connect() / .disconnect()
            if child.kind() == "attribute" {
                check_signal_method(child, src, emitted, connected);
            }

            // Legacy: emit_signal("name") / connect("name", ...)
            if child.kind() == "call" {
                check_legacy_call(child, src, emitted, connected);
            }

            collect_usage(child, src, emitted, connected);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

fn check_signal_method(
    node: Node,
    src: &[u8],
    emitted: &mut HashSet<String>,
    connected: &mut HashSet<String>,
) {
    let mut identifiers: Vec<String> = Vec::new();
    let mut method_name = None;

    let mut cursor = node.walk();
    if !cursor.goto_first_child() {
        return;
    }

    loop {
        let child = cursor.node();

        if child.kind() == "identifier"
            && let Ok(text) = child.utf8_text(src)
        {
            identifiers.push(text.to_string());
        }

        if child.kind() == "attribute_call"
            && let Some(method_node) = child
                .children(&mut child.walk())
                .find(|c| c.kind() == "identifier")
            && let Ok(method) = method_node.utf8_text(src)
        {
            method_name = Some(method.to_string());
        }

        if !cursor.goto_next_sibling() {
            break;
        }
    }

    if let Some(method) = method_name
        && let Some(signal_name) = identifiers.last()
        && signal_name != "self"
    {
        match method.as_str() {
            "emit" => {
                emitted.insert(signal_name.clone());
            }
            "connect" | "disconnect" => {
                connected.insert(signal_name.clone());
            }
            _ => {}
        }
    }
}

fn check_legacy_call(
    node: Node,
    src: &[u8],
    emitted: &mut HashSet<String>,
    connected: &mut HashSet<String>,
) {
    let func_name = node
        .children(&mut node.walk())
        .find(|c| c.kind() == "identifier")
        .and_then(|n| n.utf8_text(src).ok())
        .unwrap_or("");

    let is_emit = func_name == "emit_signal";
    let is_connect = func_name == "connect";

    if !is_emit && !is_connect {
        return;
    }

    let args = match node
        .children(&mut node.walk())
        .find(|c| c.kind() == "arguments")
    {
        Some(a) => a,
        None => return,
    };

    // First string argument is the signal name
    for child in args.children(&mut args.walk()) {
        if child.kind() == "string" {
            let text = child.utf8_text(src).unwrap_or("");
            let stripped = text
                .trim_start_matches('"')
                .trim_end_matches('"')
                .trim_start_matches('\'')
                .trim_end_matches('\'');
            if !stripped.is_empty() {
                if is_emit {
                    emitted.insert(stripped.to_string());
                } else {
                    connected.insert(stripped.to_string());
                }
            }
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::parser;

    fn check(source: &str) -> Vec<LintDiagnostic> {
        let tree = parser::parse(source).unwrap();
        let config = LintConfig::default();
        SignalNotConnected.check(&tree, source, &config)
    }

    #[test]
    fn detects_emit_without_connect() {
        let source = "signal health_changed(value: int)\n\nfunc take_damage(amount: int) -> void:\n\thealth_changed.emit(health)\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule, "signal-not-connected");
        assert!(diags[0].message.contains("health_changed"));
    }

    #[test]
    fn no_warning_when_connected() {
        let source = "signal health_changed(value: int)\n\nfunc _ready() -> void:\n\thealth_changed.connect(_on_health)\n\nfunc take_damage(amount: int) -> void:\n\thealth_changed.emit(health)\n\nfunc _on_health(v):\n\tpass\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_when_not_emitted() {
        let source = "signal my_signal\n\nfunc f() -> void:\n\tpass\n";
        // Not emitted, so not flagged by this rule (unused-signal handles that)
        assert!(check(source).is_empty());
    }

    #[test]
    fn legacy_emit_without_connect() {
        let source = "signal my_signal\n\nfunc f() -> void:\n\temit_signal(\"my_signal\")\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn legacy_both_emit_and_connect() {
        let source = "signal my_signal\n\nfunc _ready() -> void:\n\tconnect(\"my_signal\", _handler)\n\nfunc f() -> void:\n\temit_signal(\"my_signal\")\n\nfunc _handler():\n\tpass\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn opt_in_rule() {
        assert!(!SignalNotConnected.default_enabled());
    }
}
