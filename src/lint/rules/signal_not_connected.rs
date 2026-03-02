use crate::core::gd_ast::{self, GdDecl, GdExpr, GdFile};
use std::collections::{HashMap, HashSet};

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct SignalNotConnected;

impl LintRule for SignalNotConnected {
    fn name(&self) -> &'static str {
        "signal-not-connected"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Godot
    }

    fn default_enabled(&self) -> bool {
        false
    }

    fn check(&self, file: &GdFile<'_>, _source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();

        // Collect signal declarations
        let mut signals: HashMap<&str, (usize, usize)> = HashMap::new();
        gd_ast::visit_decls(file, &mut |decl| {
            if let GdDecl::Signal(sig) = decl {
                let pos = sig.node.start_position();
                signals.entry(sig.name).or_insert((pos.row, pos.column));
            }
        });

        if signals.is_empty() {
            return diags;
        }

        // Track which signals are emitted vs connected
        let mut emitted: HashSet<&str> = HashSet::new();
        let mut connected: HashSet<&str> = HashSet::new();
        gd_ast::visit_exprs(file, &mut |expr| {
            collect_signal_usage(expr, &mut emitted, &mut connected);
        });

        // Report signals that are emitted but never connected in this file
        for (name, (line, column)) in &signals {
            if emitted.contains(name) && !connected.contains(name) {
                diags.push(LintDiagnostic {
                    rule: "signal-not-connected",
                    message: format!("signal `{name}` is emitted but never connected in this file"),
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

fn collect_signal_usage<'a>(
    expr: &GdExpr<'a>,
    emitted: &mut HashSet<&'a str>,
    connected: &mut HashSet<&'a str>,
) {
    match expr {
        // signal_name.emit/connect/disconnect()
        GdExpr::MethodCall {
            receiver, method, ..
        } => {
            if let Some(name) = signal_name_from_receiver(receiver) {
                match *method {
                    "emit" => {
                        emitted.insert(name);
                    }
                    "connect" | "disconnect" => {
                        connected.insert(name);
                    }
                    _ => {}
                }
            }
        }
        // Bare callable reference: signal_name.emit (no parentheses)
        GdExpr::PropertyAccess {
            receiver, property, ..
        } => {
            if let Some(name) = signal_name_from_receiver(receiver) {
                match *property {
                    "emit" => {
                        emitted.insert(name);
                    }
                    "connect" | "disconnect" => {
                        connected.insert(name);
                    }
                    _ => {}
                }
            }
        }
        // Legacy: emit_signal("name") / connect("name", ...)
        GdExpr::Call { callee, args, .. } => {
            let callee_name = match callee.as_ref() {
                GdExpr::Ident { name, .. } => Some(*name),
                _ => None,
            };
            match callee_name {
                Some("emit_signal") => {
                    if let Some(sig_name) = extract_string_arg(args) {
                        emitted.insert(sig_name);
                    }
                }
                Some("connect") => {
                    if let Some(sig_name) = extract_string_arg(args) {
                        connected.insert(sig_name);
                    }
                }
                _ => {}
            }
        }
        _ => {}
    }
}

fn signal_name_from_receiver<'a>(receiver: &GdExpr<'a>) -> Option<&'a str> {
    match receiver {
        GdExpr::Ident { name, .. } if *name != "self" => Some(name),
        GdExpr::PropertyAccess {
            receiver: inner,
            property,
            ..
        } if matches!(inner.as_ref(), GdExpr::Ident { name: "self", .. }) => Some(property),
        _ => None,
    }
}

fn extract_string_arg<'a>(args: &[GdExpr<'a>]) -> Option<&'a str> {
    if let Some(GdExpr::StringLiteral { value, .. }) = args.first() {
        let stripped = value.trim_matches('"').trim_matches('\'');
        if !stripped.is_empty() {
            return Some(stripped);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::gd_ast;
    use crate::core::parser;

    fn check(source: &str) -> Vec<LintDiagnostic> {
        let tree = parser::parse(source).unwrap();
        let file = gd_ast::convert(&tree, source);
        let config = LintConfig::default();
        SignalNotConnected.check(&file, source, &config)
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

    #[test]
    fn no_warning_when_emit_callable_ref_connected() {
        let source = "signal my_signal\n\nfunc _ready() -> void:\n\tmy_signal.connect(_handler)\n\tother.some_signal.connect(my_signal.emit)\n\nfunc _handler():\n\tpass\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn bare_emit_ref_counts_as_emitted() {
        // my_signal.emit used as callable ref → emitted but not connected
        let source = "signal my_signal\n\nfunc _ready() -> void:\n\tother.some_signal.connect(my_signal.emit)\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("my_signal"));
    }

    #[test]
    fn bare_connect_ref_counts_as_connected() {
        let source =
            "signal my_signal\n\nfunc _ready() -> void:\n\tmy_signal.emit()\n\tmy_signal.connect\n";
        assert!(check(source).is_empty());
    }
}
