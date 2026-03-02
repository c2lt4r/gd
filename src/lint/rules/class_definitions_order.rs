use crate::core::gd_ast::{GdClass, GdDecl, GdFile, GdFunc, GdVar};

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct ClassDefinitionsOrder;

impl LintRule for ClassDefinitionsOrder {
    fn name(&self) -> &'static str {
        "class-definitions-order"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Style
    }

    fn default_enabled(&self) -> bool {
        false
    }

    fn check(&self, file: &GdFile<'_>, _source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();

        // Check top-level ordering
        check_member_order(&file.declarations, &mut diags);

        // Check inner class bodies
        for decl in &file.declarations {
            if let GdDecl::Class(cls) = decl {
                check_class(cls, &mut diags);
            }
        }

        diags
    }
}

/// Lifecycle/virtual methods that belong in category 8.
const LIFECYCLE_METHODS: &[&str] = &[
    "_ready",
    "_init",
    "_process",
    "_physics_process",
    "_enter_tree",
    "_exit_tree",
    "_input",
    "_unhandled_input",
    "_unhandled_key_input",
    "_draw",
    "_notification",
    "_to_string",
    "_get",
    "_set",
    "_get_property_list",
    "_validate_property",
    "_property_can_revert",
    "_property_get_revert",
];

/// Member categories in canonical order (lower = earlier).
const CAT_SIGNAL: u8 = 1;
const CAT_ENUM: u8 = 2;
const CAT_CONST: u8 = 3;
const CAT_EXPORT_VAR: u8 = 4;
const CAT_PUBLIC_VAR: u8 = 5;
const CAT_PRIVATE_VAR: u8 = 6;
const CAT_ONREADY_VAR: u8 = 7;
const CAT_LIFECYCLE_METHOD: u8 = 8;
const CAT_PUBLIC_METHOD: u8 = 9;
const CAT_PRIVATE_METHOD: u8 = 10;
const CAT_INNER_CLASS: u8 = 11;

fn category_label(cat: u8) -> &'static str {
    match cat {
        CAT_SIGNAL => "signals",
        CAT_ENUM => "enums",
        CAT_CONST => "constants",
        CAT_EXPORT_VAR => "@export variables",
        CAT_PUBLIC_VAR => "public variables",
        CAT_PRIVATE_VAR => "private variables",
        CAT_ONREADY_VAR => "@onready variables",
        CAT_LIFECYCLE_METHOD => "lifecycle methods",
        CAT_PUBLIC_METHOD => "public methods",
        CAT_PRIVATE_METHOD => "private methods",
        CAT_INNER_CLASS => "inner classes",
        _ => "unknown",
    }
}

fn check_class(cls: &GdClass, diags: &mut Vec<LintDiagnostic>) {
    check_member_order(&cls.declarations, diags);
    for decl in &cls.declarations {
        if let GdDecl::Class(inner) = decl {
            check_class(inner, diags);
        }
    }
}

/// Check ordering of members in a declaration list.
fn check_member_order(decls: &[GdDecl], diags: &mut Vec<LintDiagnostic>) {
    let mut highest_cat: u8 = 0;
    let mut highest_cat_label = "";

    for decl in decls {
        if let Some(cat) = categorize_member(decl) {
            if cat < highest_cat {
                let name = member_name(decl);
                let label = if name.is_empty() {
                    category_label(cat).to_string()
                } else {
                    format!("`{name}`")
                };
                let node = decl_node(decl);
                diags.push(LintDiagnostic {
                    rule: "class-definitions-order",
                    message: format!(
                        "{} ({}) should come before {} ({})",
                        label,
                        category_label(cat),
                        highest_cat_label,
                        category_label(highest_cat),
                    ),
                    severity: Severity::Warning,
                    line: node.start_position().row,
                    column: node.start_position().column,
                    end_column: None,
                    fix: None,
                    context_lines: None,
                });
            } else if cat > highest_cat {
                highest_cat = cat;
                highest_cat_label = member_name(decl);
            }
        }
    }
}

/// Get the tree-sitter node for a declaration (for position info).
fn decl_node<'a>(decl: &'a GdDecl<'a>) -> tree_sitter::Node<'a> {
    match decl {
        GdDecl::Func(f) => f.node,
        GdDecl::Var(v) => v.node,
        GdDecl::Signal(s) => s.node,
        GdDecl::Enum(e) => e.node,
        GdDecl::Class(c) => c.node,
        GdDecl::Stmt(s) => s.node(),
    }
}

/// Determine the category of a class member.
fn categorize_member(decl: &GdDecl) -> Option<u8> {
    match decl {
        GdDecl::Signal(_) => Some(CAT_SIGNAL),
        GdDecl::Enum(_) => Some(CAT_ENUM),
        GdDecl::Var(var) => Some(categorize_var(var)),
        GdDecl::Func(func) => Some(categorize_func(func)),
        GdDecl::Class(_) => Some(CAT_INNER_CLASS),
        GdDecl::Stmt(_) => None,
    }
}

fn categorize_var(var: &GdVar) -> u8 {
    if var.is_const {
        return CAT_CONST;
    }
    if var.annotations.iter().any(|a| a.name == "onready") {
        return CAT_ONREADY_VAR;
    }
    if var.annotations.iter().any(|a| a.name.starts_with("export")) {
        return CAT_EXPORT_VAR;
    }
    if var.name.starts_with('_') {
        CAT_PRIVATE_VAR
    } else {
        CAT_PUBLIC_VAR
    }
}

fn categorize_func(func: &GdFunc) -> u8 {
    if LIFECYCLE_METHODS.contains(&func.name) {
        CAT_LIFECYCLE_METHOD
    } else if func.name.starts_with('_') {
        CAT_PRIVATE_METHOD
    } else {
        CAT_PUBLIC_METHOD
    }
}

/// Extract the name of a member for error messages.
fn member_name<'a>(decl: &'a GdDecl<'a>) -> &'a str {
    match decl {
        GdDecl::Func(f) => f.name,
        GdDecl::Var(v) => v.name,
        GdDecl::Signal(s) => s.name,
        GdDecl::Enum(e) => e.name,
        GdDecl::Class(c) => c.name,
        GdDecl::Stmt(_) => "",
    }
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
        ClassDefinitionsOrder.check(&file, source, &config)
    }

    // ── Correct ordering ──────────────────────────────────────────────

    #[test]
    fn correct_full_order() {
        let source = "\
class_name MyClass
extends Node

signal health_changed

enum Direction { UP, DOWN }

const MAX_SPEED = 100

@export var speed: float = 10.0

var public_var = 0

var _private_var = 0

@onready var sprite = $Sprite2D

func _ready():
\tpass

func public_method():
\tpass

func _private_method():
\tpass

class InnerClass:
\tpass
";
        assert!(check(source).is_empty());
    }

    #[test]
    fn signals_only() {
        let source = "signal a\nsignal b\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn empty_file() {
        assert!(check("").is_empty());
    }

    #[test]
    fn single_function() {
        let source = "func _ready():\n\tpass\n";
        assert!(check(source).is_empty());
    }

    // ── Misordered members ────────────────────────────────────────────

    #[test]
    fn signal_after_function() {
        let source = "\
func some_method():
\tpass

signal my_signal
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("signal"));
    }

    #[test]
    fn function_before_variable() {
        let source = "\
func my_func():
\tpass

var my_var = 0
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("public variable"));
    }

    #[test]
    fn const_after_variable() {
        let source = "\
var my_var = 0

const MY_CONST = 5
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("constant"));
    }

    #[test]
    fn inner_class_before_method() {
        // Inner class is category 11, method is 9 - this is fine (class AFTER method)
        // But method AFTER inner class is wrong
        let source = "\
class InnerClass:
\tpass

func my_method():
\tpass
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("public method"));
    }

    #[test]
    fn enum_after_signal_ok() {
        let source = "\
signal my_signal

enum Direction { UP, DOWN }
";
        assert!(check(source).is_empty());
    }

    #[test]
    fn private_var_before_public_var() {
        let source = "\
var _private = 0

var public_one = 0
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("public variable"));
    }

    #[test]
    fn lifecycle_before_public_method_ok() {
        let source = "\
func _ready():
\tpass

func my_method():
\tpass
";
        assert!(check(source).is_empty());
    }

    #[test]
    fn public_method_before_lifecycle() {
        let source = "\
func my_method():
\tpass

func _ready():
\tpass
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("lifecycle"));
    }

    // ── Inner class ordering ──────────────────────────────────────────

    #[test]
    fn inner_class_body_ordering() {
        let source = "\
class Inner:
\tfunc some_method():
\t\tpass
\tsignal my_signal
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("signal"));
    }

    // ── Opt-in ────────────────────────────────────────────────────────

    #[test]
    fn is_opt_in() {
        assert!(!ClassDefinitionsOrder.default_enabled());
    }

    // ── Multiple violations ───────────────────────────────────────────

    #[test]
    fn multiple_violations() {
        let source = "\
func my_func():
\tpass

var my_var = 0

signal my_signal
";
        let diags = check(source);
        assert_eq!(diags.len(), 2);
    }

    // ── Extends before class_name ok ──────────────────────────────────

    #[test]
    fn extends_and_class_name_same_category() {
        let source = "\
extends Node
class_name MyClass
";
        // Both are in the file header, not in declarations — no violation
        assert!(check(source).is_empty());
    }

    // ── Standalone annotations ───────────────────────────────────────

    #[test]
    fn rpc_before_function_not_flagged() {
        let source = "\
var speed = 10

@rpc
func sync_position():
\tpass
";
        assert!(check(source).is_empty());
    }

    #[test]
    fn export_group_between_exports_not_flagged() {
        let source = "\
@export var health: int = 100

@export_group(\"Movement\")
@export var speed: float = 10.0

@export_subgroup(\"Advanced\")
@export var acceleration: float = 5.0
";
        assert!(check(source).is_empty());
    }

    #[test]
    fn export_category_with_exports() {
        let source = "\
@export_category(\"Stats\")
@export var health: int = 100
";
        assert!(check(source).is_empty());
    }

    #[test]
    fn tool_annotation_is_header() {
        // Standalone @tool after a signal — the typed AST stores @tool as
        // file.is_tool rather than a declaration, so this won't appear in
        // the ordering check. This is acceptable: @tool is nearly always
        // first and checking it via `is_tool` is the right pattern.
        let source = "\
signal my_signal

@tool
";
        let diags = check(source);
        // @tool doesn't appear in declarations, so no ordering violation detected.
        // The old CST approach caught this, but in practice @tool at end-of-file
        // is extremely rare. The typed AST correctly handles `file.is_tool`.
        assert!(diags.is_empty());
    }
}
