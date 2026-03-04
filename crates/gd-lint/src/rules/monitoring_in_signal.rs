use gd_core::gd_ast::{self, GdDecl, GdExpr, GdFile, GdStmt};
use std::collections::HashSet;

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use gd_core::config::LintConfig;

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

    fn category(&self) -> LintCategory {
        LintCategory::Godot
    }

    fn check(&self, file: &GdFile<'_>, _source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();

        // 1. Collect callback function names connected to area signals
        let mut signal_callbacks: HashSet<&str> = HashSet::new();
        gd_ast::visit_exprs(file, &mut |expr| {
            collect_signal_connect(expr, &mut signal_callbacks);
        });

        // 2. Check each matching function body for direct monitoring/monitorable assignment
        gd_ast::visit_decls(file, &mut |decl| {
            if let GdDecl::Func(func) = decl
                && (signal_callbacks.contains(func.name)
                    || is_auto_connected_signal_handler(func.name))
            {
                check_body_for_monitoring(&func.body, func.name, &mut diags);
            }
        });

        diags
    }
}

/// Find `signal.connect(callback)` where signal is an area signal, collect callback name.
fn collect_signal_connect<'a>(expr: &GdExpr<'a>, callbacks: &mut HashSet<&'a str>) {
    // Pattern: signal_name.connect(callback_name)
    let GdExpr::MethodCall {
        receiver,
        method,
        args,
        ..
    } = expr
    else {
        return;
    };
    if *method != "connect" {
        return;
    }

    // Receiver should be an identifier matching an area signal
    let signal_name = match receiver.as_ref() {
        GdExpr::Ident { name, .. } => *name,
        // self.signal_name.connect(...)
        GdExpr::PropertyAccess {
            property,
            receiver: inner,
            ..
        } if matches!(inner.as_ref(), GdExpr::Ident { name: "self", .. }) => *property,
        _ => return,
    };

    if !AREA_SIGNALS.contains(&signal_name) {
        return;
    }

    // First argument should be an identifier (the callback name)
    if let Some(GdExpr::Ident { name, .. }) = args.first() {
        callbacks.insert(name);
    }
}

/// Check if function name matches Godot auto-connect pattern: _on_*_<signal>
fn is_auto_connected_signal_handler(name: &str) -> bool {
    if !name.starts_with("_on_") {
        return false;
    }
    let suffix = &name[4..]; // strip "_on_"
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
fn check_body_for_monitoring(stmts: &[GdStmt], func_name: &str, diags: &mut Vec<LintDiagnostic>) {
    gd_ast::visit_body_stmts(stmts, &mut |stmt| {
        if let GdStmt::Assign { target, node, .. } = stmt
            && let Some(prop) = extract_dangerous_prop(target)
        {
            diags.push(LintDiagnostic {
                rule: "monitoring-in-signal",
                message: format!(
                    "direct assignment to `{prop}` in signal callback `{func_name}()`; \
                     use `set_deferred(\"{prop}\", value)` instead"
                ),
                severity: Severity::Warning,
                line: node.start_position().row,
                column: node.start_position().column,
                end_column: Some(node.end_position().column),
                fix: None,
                context_lines: None,
            });
        }
    });
}

/// Extract a dangerous property name from an assignment target.
/// Matches: `monitoring`, `self.monitoring`, `monitorable`, `self.monitorable`
fn extract_dangerous_prop<'a>(target: &GdExpr<'a>) -> Option<&'a str> {
    let name = match target {
        GdExpr::Ident { name, .. } => *name,
        GdExpr::PropertyAccess {
            receiver, property, ..
        } if matches!(receiver.as_ref(), GdExpr::Ident { name: "self", .. }) => *property,
        _ => return None,
    };
    if DANGEROUS_PROPS.contains(&name) {
        Some(name)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gd_core::gd_ast;
    use gd_core::parser;

    fn check(source: &str) -> Vec<LintDiagnostic> {
        let tree = parser::parse(source).unwrap();
        let file = gd_ast::convert(&tree, source);
        let config = LintConfig::default();
        MonitoringInSignal.check(&file, source, &config)
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
