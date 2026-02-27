use std::collections::{HashMap, HashSet};
use crate::core::gd_ast::{self, GdDecl, GdExpr, GdFile};

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct UnusedSignal;

impl LintRule for UnusedSignal {
    fn name(&self) -> &'static str {
        "unused-signal"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Maintenance
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

        // Event bus heuristic: if the file has no functions, it's likely an event
        // bus or signal-only file where signals are used from other files.
        if !file.declarations.iter().any(|d| matches!(d, GdDecl::Func(_))) {
            return diags;
        }

        // Collect signal references via typed AST expression visitor
        let mut referenced: HashSet<&str> = HashSet::new();
        gd_ast::visit_exprs(file, &mut |expr| {
            collect_signal_ref(expr, &mut referenced);
        });

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

/// Extract signal name from signal-related expression patterns.
fn collect_signal_ref<'a>(expr: &GdExpr<'a>, referenced: &mut HashSet<&'a str>) {
    match expr {
        // signal_name.emit() / .connect() / .disconnect()
        GdExpr::MethodCall { receiver, method, .. }
            if matches!(*method, "emit" | "connect" | "disconnect") =>
        {
            if let Some(name) = signal_name_from_receiver(receiver) {
                referenced.insert(name);
            }
        }
        // Bare callable reference: signal_name.emit (no parentheses)
        GdExpr::PropertyAccess { receiver, property, .. }
            if matches!(*property, "emit" | "connect" | "disconnect") =>
        {
            if let Some(name) = signal_name_from_receiver(receiver) {
                referenced.insert(name);
            }
        }
        // Legacy: emit_signal("signal_name")
        GdExpr::Call { callee, args, .. }
            if matches!(callee.as_ref(), GdExpr::Ident { name: "emit_signal", .. }) =>
        {
            if let Some(name) = extract_string_arg(args) {
                referenced.insert(name);
            }
        }
        _ => {}
    }
}

/// Extract signal name from the receiver of a method call or property access.
fn signal_name_from_receiver<'a>(receiver: &GdExpr<'a>) -> Option<&'a str> {
    match receiver {
        GdExpr::Ident { name, .. } if *name != "self" => Some(name),
        // self.signal_name → property is the signal
        GdExpr::PropertyAccess { receiver: inner, property, .. }
            if matches!(inner.as_ref(), GdExpr::Ident { name: "self", .. }) =>
        {
            Some(property)
        }
        _ => None,
    }
}

/// Extract the string content from the first argument of a call.
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
    use crate::core::parser;
    use crate::core::gd_ast;

    fn check(source: &str) -> Vec<LintDiagnostic> {
        let tree = parser::parse(source).unwrap();
        let file = gd_ast::convert(&tree, source);
        let config = LintConfig::default();
        UnusedSignal.check(&file, source, &config)
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
    fn file_with_functions_still_warns() {
        // Has functions → not event bus, should still warn
        let source = "signal my_signal\n\nfunc f():\n\tpass\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn no_warning_when_emit_callable_ref() {
        let source = "signal my_signal\n\nfunc _ready() -> void:\n\tother.some_signal.connect(my_signal.emit)\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_when_self_emit_callable_ref() {
        let source = "signal my_signal\n\nfunc _ready() -> void:\n\tother.some_signal.connect(self.my_signal.emit)\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_when_connect_callable_ref() {
        let source = "signal my_signal\n\nfunc _ready() -> void:\n\tmy_signal.connect\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_when_disconnect_callable_ref() {
        let source = "signal my_signal\n\nfunc _ready() -> void:\n\tmy_signal.disconnect\n";
        assert!(check(source).is_empty());
    }
}
