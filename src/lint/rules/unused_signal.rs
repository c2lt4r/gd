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

        // Second pass: find all referenced signals (emitted or connected)
        let mut referenced: HashSet<String> = HashSet::new();
        collect_emitted(root, src, &mut referenced);
        collect_signal_references(root, src, &mut referenced);

        // Report signals that are never referenced in this file
        for (name, (line, column)) in &signals {
            if !referenced.contains(name) {
                diags.push(LintDiagnostic {
                    rule: "unused-signal",
                    message: format!(
                        "signal `{}` is declared but never referenced in this file",
                        name
                    ),
                    severity: Severity::Warning,
                    line: *line,
                    column: *column,
                    end_column: None,
                    fix: None,
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
            {
                let name = name_node.utf8_text(src).unwrap_or("").to_string();
                let line = name_node.start_position().row;
                let col = name_node.start_position().column;
                signals.entry(name).or_insert((line, col));
            }

            // Recurse into all children
            collect_signals(child, src, signals);

            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

fn collect_emitted(node: Node, src: &[u8], emitted: &mut HashSet<String>) {
    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            let child = cursor.node();

            // Check for emit_signal("name") pattern
            if child.kind() == "call"
                && let Some(func) = child.child_by_field_name("function")
            {
                let func_text = func.utf8_text(src).unwrap_or("");

                // emit_signal("signal_name")
                if (func_text == "emit_signal" || func_text.ends_with(".emit_signal"))
                    && let Some(args) = child.child_by_field_name("arguments")
                {
                    // Get first argument (the signal name string)
                    let mut arg_cursor = args.walk();
                    if arg_cursor.goto_first_child() {
                        loop {
                            let arg = arg_cursor.node();
                            if arg.kind() == "string" {
                                let text = arg.utf8_text(src).unwrap_or("");
                                // Strip quotes
                                let stripped = text
                                    .trim_start_matches('"')
                                    .trim_end_matches('"')
                                    .trim_start_matches('\'')
                                    .trim_end_matches('\'');
                                if !stripped.is_empty() {
                                    emitted.insert(stripped.to_string());
                                }
                                break;
                            }
                            if !arg_cursor.goto_next_sibling() {
                                break;
                            }
                        }
                    }
                }

                // signal_name.emit() pattern
                if func_text.ends_with(".emit") {
                    let signal_name = func_text.trim_end_matches(".emit");
                    // Could be self.signal_name.emit or just signal_name.emit
                    let name = signal_name.rsplit('.').next().unwrap_or(signal_name);
                    if !name.is_empty() {
                        emitted.insert(name.to_string());
                    }
                }
            }

            collect_emitted(child, src, emitted);

            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

/// Find signal references via .connect(), .disconnect(), or direct signal access.
/// Catches patterns like: signal_name.connect(...), self.signal_name.connect(...)
fn collect_signal_references(node: Node, src: &[u8], referenced: &mut HashSet<String>) {
    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            let child = cursor.node();

            if child.kind() == "call"
                && let Some(func) = child.child_by_field_name("function")
            {
                let func_text = func.utf8_text(src).unwrap_or("");

                // signal_name.connect(...) / self.signal_name.connect(...)
                // signal_name.disconnect(...) / self.signal_name.disconnect(...)
                for suffix in &[".connect", ".disconnect"] {
                    if func_text.ends_with(suffix) {
                        let before = func_text.trim_end_matches(suffix);
                        let name = before.rsplit('.').next().unwrap_or(before);
                        if !name.is_empty() {
                            referenced.insert(name.to_string());
                        }
                    }
                }
            }

            collect_signal_references(child, src, referenced);

            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}
