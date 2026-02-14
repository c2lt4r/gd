use std::collections::{HashMap, HashSet};
use tree_sitter::{Node, Tree};

use super::{LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct UnusedSignal;

impl LintRule for UnusedSignal {
    fn name(&self) -> &'static str {
        "unused-signal"
    }

    fn check(&self, tree: &Tree, source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        let root = tree.root_node();
        let src = source.as_bytes();

        // First pass: collect all signal declarations
        let mut signals: HashMap<String, (usize, usize)> = HashMap::new();
        collect_signals(root, src, &mut signals);

        if signals.is_empty() {
            return diags;
        }

        // Event bus heuristic: if the file has no functions, it's likely an event
        // bus or signal-only file where signals are used from other files.
        // Suppress all unused-signal warnings in this case.
        if !has_functions(root) {
            return diags;
        }

        // Second pass: find all referenced signals
        let mut referenced: HashSet<String> = HashSet::new();
        collect_references(root, src, &mut referenced);

        // Report signals that are never referenced in this file
        for (name, (line, column)) in &signals {
            if !referenced.contains(name) {
                diags.push(LintDiagnostic {
                    rule: "unused-signal",
                    message: format!(
                        "signal `{name}` is declared but never referenced in this file"
                    ),
                    severity: Severity::Warning,
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

/// Check if the root scope has any function definitions.
fn has_functions(root: Node) -> bool {
    let mut cursor = root.walk();
    if cursor.goto_first_child() {
        loop {
            let kind = cursor.node().kind();
            if kind == "function_definition" || kind == "constructor_definition" {
                return true;
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
    false
}

fn collect_signals(node: Node, src: &[u8], signals: &mut HashMap<String, (usize, usize)>) {
    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            let child = cursor.node();
            if child.kind() == "signal_statement"
                && let Some(name_node) = child.child_by_field_name("name")
            {
                let name = name_node.utf8_text(src).unwrap_or("").to_string();
                let line = name_node.start_position().row;
                let col = name_node.start_position().column;
                signals.entry(name).or_insert((line, col));
            }

            collect_signals(child, src, signals);

            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

/// Find signal references in all forms:
/// - `signal_name.emit(...)` / `signal_name.connect(...)` / `signal_name.disconnect(...)`
///   Parsed as: attribute > attribute_call (tree-sitter-gdscript method call syntax)
/// - `emit_signal("signal_name")` (legacy Godot 3 API)
///   Parsed as: call > [identifier "emit_signal", arguments > [string]]
fn collect_references(node: Node, src: &[u8], referenced: &mut HashSet<String>) {
    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            let child = cursor.node();

            // Modern GDScript: signal_name.emit() / .connect() / .disconnect()
            if child.kind() == "attribute" {
                check_attribute_call(child, src, referenced);
            }

            // Legacy: emit_signal("signal_name")
            if child.kind() == "call" {
                check_legacy_emit(child, src, referenced);
            }

            collect_references(child, src, referenced);

            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

/// Check `attribute` nodes for signal method calls like:
///   signal_name.emit(...)
///   signal_name.connect(...)
///   self.signal_name.emit(...)
fn check_attribute_call(node: Node, src: &[u8], referenced: &mut HashSet<String>) {
    let mut cursor = node.walk();
    if !cursor.goto_first_child() {
        return;
    }

    let mut identifiers: Vec<String> = Vec::new();
    let mut found_signal_method = false;

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
        {
            let method = method_node.utf8_text(src).unwrap_or("");
            if matches!(method, "emit" | "connect" | "disconnect") {
                found_signal_method = true;
            }
        }

        if !cursor.goto_next_sibling() {
            break;
        }
    }

    if found_signal_method {
        // signal_name.emit() → identifiers = ["signal_name"]
        // self.signal_name.emit() → identifiers = ["self", "signal_name"]
        if let Some(name) = identifiers.last()
            && name != "self"
        {
            referenced.insert(name.clone());
        }
    }
}

/// Check legacy `emit_signal("signal_name")` calls.
/// Tree structure: call > [identifier "emit_signal", arguments > [string "signal_name"]]
fn check_legacy_emit(node: Node, src: &[u8], referenced: &mut HashSet<String>) {
    // Find the function name: first named child identifier
    let func_name = node
        .children(&mut node.walk())
        .find(|c| c.kind() == "identifier")
        .and_then(|n| n.utf8_text(src).ok())
        .unwrap_or("");

    if func_name != "emit_signal" {
        return;
    }

    // Find the arguments node
    let Some(args) = node
        .children(&mut node.walk())
        .find(|c| c.kind() == "arguments")
    else {
        return;
    };

    // Get first string argument (the signal name)
    for child in args.children(&mut args.walk()) {
        if child.kind() == "string" {
            let text = child.utf8_text(src).unwrap_or("");
            let stripped = text
                .trim_start_matches('"')
                .trim_end_matches('"')
                .trim_start_matches('\'')
                .trim_end_matches('\'');
            if !stripped.is_empty() {
                referenced.insert(stripped.to_string());
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
        UnusedSignal.check(&tree, source, &config)
    }

    #[test]
    fn detects_unused_signal() {
        let source = "signal my_signal\n\nfunc f():\n\tpass\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule, "unused-signal");
    }

    #[test]
    fn no_warning_when_emitted() {
        let source = "signal state_changed(old: String, new: String)\n\nfunc f() -> void:\n\tstate_changed.emit(\"a\", \"b\")\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_when_connected() {
        let source = "signal my_signal\n\nfunc f() -> void:\n\tmy_signal.connect(_on_signal)\n\nfunc _on_signal():\n\tpass\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_when_disconnected() {
        let source = "signal my_signal\n\nfunc f() -> void:\n\tmy_signal.disconnect(_on_signal)\n\nfunc _on_signal():\n\tpass\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_legacy_emit_signal() {
        let source = "signal my_signal\n\nfunc f() -> void:\n\temit_signal(\"my_signal\")\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn event_bus_heuristic_suppresses_warnings() {
        // File with only signals and no functions → event bus pattern
        let source = "signal player_ready\nsignal player_died\nsignal score_changed\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn event_bus_with_vars_still_suppressed() {
        // Signals + vars but no functions → still event bus
        let source = "signal game_started\nsignal game_over\nvar description = \"Events\"\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn file_with_functions_still_warns() {
        // Has functions → not event bus, should still warn
        let source = "signal my_signal\n\nfunc f():\n\tpass\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
    }
}
