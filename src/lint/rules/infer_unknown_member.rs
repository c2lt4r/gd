use crate::core::gd_ast::{self, GdDecl, GdExpr, GdFile, GdFunc, GdStmt, GdVar};

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use crate::class_db;
use crate::core::config::LintConfig;

pub struct InferUnknownMember;

impl LintRule for InferUnknownMember {
    fn name(&self) -> &'static str {
        "infer-unknown-member"
    }

    fn category(&self) -> LintCategory {
        LintCategory::TypeSafety
    }

    fn default_enabled(&self) -> bool {
        true
    }

    fn check(&self, _file: &GdFile<'_>, _source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        Vec::new()
    }

    fn check_with_symbols(
        &self,
        file: &GdFile<'_>,
        source: &str,
        _config: &LintConfig,
    ) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        gd_ast::visit_decls(file, &mut |decl| {
            match decl {
                GdDecl::Func(func) => {
                    check_stmts_for_inferred(&func.body, Some(func), source, file, &mut diags);
                }
                GdDecl::Var(var) => {
                    check_inferred_var(var, None, &[], source, file, &mut diags);
                }
                _ => {}
            }
        });
        diags
    }
}

/// Recursively search statements for `:=` variable declarations with member access.
fn check_stmts_for_inferred<'a>(
    stmts: &[GdStmt<'a>],
    func: Option<&GdFunc<'a>>,
    source: &str,
    file: &GdFile<'a>,
    diags: &mut Vec<LintDiagnostic>,
) {
    for stmt in stmts {
        if let GdStmt::Var(var) = stmt {
            let func_body = func.map_or(&[] as &[GdStmt], |f| f.body.as_slice());
            check_inferred_var(var, func, func_body, source, file, diags);
        }
        // Recurse into control flow
        match stmt {
            GdStmt::If(if_stmt) => {
                check_stmts_for_inferred(&if_stmt.body, func, source, file, diags);
                for (_, branch) in &if_stmt.elif_branches {
                    check_stmts_for_inferred(branch, func, source, file, diags);
                }
                if let Some(else_body) = &if_stmt.else_body {
                    check_stmts_for_inferred(else_body, func, source, file, diags);
                }
            }
            GdStmt::For { body, .. } | GdStmt::While { body, .. } => {
                check_stmts_for_inferred(body, func, source, file, diags);
            }
            GdStmt::Match { arms, .. } => {
                for arm in arms {
                    check_stmts_for_inferred(&arm.body, func, source, file, diags);
                }
            }
            _ => {}
        }
    }
}

/// Check a single variable declaration for the `:= obj.unknown_member` pattern.
fn check_inferred_var<'a>(
    var: &GdVar<'a>,
    func: Option<&GdFunc<'a>>,
    func_body: &[GdStmt<'a>],
    source: &str,
    file: &GdFile<'a>,
    diags: &mut Vec<LintDiagnostic>,
) {
    // Only check := (inferred type)
    let Some(type_ann) = &var.type_ann else {
        return;
    };
    if !type_ann.is_inferred {
        return;
    }

    // RHS must be a member access: obj.member
    let Some(GdExpr::PropertyAccess { receiver, property, .. }) = &var.value else {
        return;
    };

    // Resolve the type of the receiver object
    let Some(obj_type) = resolve_object_type(receiver, func, func_body, source, file) else {
        return;
    };

    // Only check engine classes — user classes may have properties we don't track here
    if !class_db::class_exists(&obj_type) {
        return;
    }

    // Check if member exists as a property, method, or signal on the resolved type
    if class_db::property_exists(&obj_type, property)
        || class_db::method_exists(&obj_type, property)
        || class_db::signal_exists(&obj_type, property)
    {
        return;
    }

    diags.push(LintDiagnostic {
        rule: "infer-unknown-member",
        message: format!(
            "`:=` cannot infer type — `{property}` is not a known member of `{obj_type}`; \
             use an explicit type annotation for `{}`",
            var.name
        ),
        severity: Severity::Warning,
        line: var.node.start_position().row,
        column: var.node.start_position().column,
        end_column: None,
        fix: None,
        context_lines: None,
    });
}

/// Resolve the type of a receiver expression from the typed AST.
/// Handles identifiers by checking class-level vars, function params, and local vars.
fn resolve_object_type<'a>(
    receiver: &GdExpr<'a>,
    func: Option<&GdFunc<'a>>,
    func_body: &[GdStmt<'a>],
    source: &str,
    file: &GdFile<'a>,
) -> Option<String> {
    let GdExpr::Ident { name, node, .. } = receiver else {
        return None;
    };

    // Check class-level variable declarations
    for var in file.vars() {
        if var.name == *name {
            return var
                .type_ann
                .as_ref()
                .filter(|t| !t.is_inferred)
                .map(|t| t.name.to_string());
        }
    }

    // Check function parameters (no parent walking needed — we have the func directly)
    if let Some(func) = func {
        for param in &func.params {
            if param.name == *name {
                return param
                    .type_ann
                    .as_ref()
                    .filter(|t| !t.is_inferred)
                    .map(|t| t.name.to_string());
            }
        }
    }

    // Check local variable declarations before this line
    let _ = source; // used for consistency with original API
    let target_line = node.start_position().row;
    for stmt in func_body {
        if stmt.node().start_position().row >= target_line {
            break;
        }
        if let GdStmt::Var(var) = stmt
            && var.name == *name
            && let Some(type_ann) = &var.type_ann
            && !type_ann.is_inferred
        {
            return Some(type_ann.name.to_string());
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
        InferUnknownMember.check_with_symbols(&file, source, &config)
    }

    #[test]
    fn detects_unknown_member_on_base_class() {
        let source = "\
extends Node
var _player: CharacterBody2D
func test():
\tvar grid := _player.grid_position
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("grid_position"));
        assert!(diags[0].message.contains("CharacterBody2D"));
    }

    #[test]
    fn no_warning_known_property() {
        let source = "\
extends Node
var _player: CharacterBody2D
func test():
\tvar vel := _player.velocity
";
        let diags = check(source);
        assert!(
            diags.is_empty(),
            "velocity is a known CharacterBody2D property"
        );
    }

    #[test]
    fn no_warning_known_method() {
        let source = "\
extends Node
var _player: CharacterBody2D
func test():
\tvar child := _player.get_child(0)
";
        // get_child(0) is an attribute_call, not attribute — should not trigger
        let diags = check(source);
        assert!(diags.is_empty());
    }

    #[test]
    fn no_warning_explicit_type() {
        let source = "\
extends Node
var _player: CharacterBody2D
func test():
\tvar grid: Vector2i = _player.grid_position
";
        let diags = check(source);
        assert!(
            diags.is_empty(),
            "explicit type annotation should not trigger"
        );
    }

    #[test]
    fn no_warning_untyped_var() {
        let source = "\
extends Node
var _player
func test():
\tvar grid := _player.grid_position
";
        let diags = check(source);
        assert!(
            diags.is_empty(),
            "untyped variable has no known class to check"
        );
    }

    #[test]
    fn no_warning_user_class() {
        let source = "\
extends Node
var _player: PlayerCharacter
func test():
\tvar grid := _player.grid_position
";
        let diags = check(source);
        assert!(diags.is_empty(), "user classes are not in ClassDB — skip");
    }

    #[test]
    fn detects_via_function_parameter() {
        let source = "\
extends Node
func test(body: CharacterBody2D):
\tvar grid := body.grid_position
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("grid_position"));
    }

    #[test]
    fn no_warning_known_inherited_property() {
        let source = "\
extends Node
var _sprite: Sprite2D
func test():
\tvar pos := _sprite.position
";
        let diags = check(source);
        assert!(diags.is_empty(), "position is inherited from Node2D");
    }

    #[test]
    fn no_warning_plain_equals() {
        let source = "\
extends Node
var _player: CharacterBody2D
func test():
\tvar grid = _player.grid_position
";
        let diags = check(source);
        assert!(diags.is_empty(), "plain = does not infer type");
    }

    #[test]
    fn detects_via_local_typed_var() {
        let source = "\
extends Node
func test():
\tvar body: CharacterBody2D = get_node(\"body\")
\tvar grid := body.grid_position
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("grid_position"));
    }

    #[test]
    fn no_warning_signal_access() {
        let source = "\
extends Node
var _slider: HSlider
func test():
\tvar _err := _slider.value_changed.connect(func(val: float) -> void: pass)
";
        let diags = check(source);
        assert!(
            diags.is_empty(),
            "value_changed is a signal on Range (parent of HSlider)"
        );
    }

    #[test]
    fn default_enabled() {
        assert!(InferUnknownMember.default_enabled());
    }
}
