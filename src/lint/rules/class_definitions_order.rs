use tree_sitter::{Node, Tree};

use super::{LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct ClassDefinitionsOrder;

impl LintRule for ClassDefinitionsOrder {
    fn name(&self) -> &'static str {
        "class-definitions-order"
    }

    fn default_enabled(&self) -> bool {
        false
    }

    fn check(&self, tree: &Tree, source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        let root = tree.root_node();

        // Check top-level ordering
        check_member_order(root, source, &mut diags);

        // Check inner class bodies
        check_inner_classes(root, source, &mut diags);

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
const CAT_HEADER: u8 = 0; // class_name, extends, @tool
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
        CAT_HEADER => "header (class_name/extends/@tool)",
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

/// Check ordering of members in a given parent node (source_file or class body).
fn check_member_order(parent: Node, source: &str, diags: &mut Vec<LintDiagnostic>) {
    let mut highest_cat: u8 = 0;
    let mut highest_cat_label = "";

    let mut cursor = parent.walk();
    if !cursor.goto_first_child() {
        return;
    }

    loop {
        let child = cursor.node();
        if let Some(cat) = categorize_member(&child, source) {
            if cat < highest_cat {
                let name = member_name(&child, source).unwrap_or_default();
                let label = if name.is_empty() {
                    category_label(cat).to_string()
                } else {
                    format!("`{name}`")
                };
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
                    line: child.start_position().row,
                    column: child.start_position().column,
                    end_column: None,
                    fix: None,
                    context_lines: None,
                });
            } else if cat > highest_cat {
                highest_cat = cat;
                highest_cat_label = member_name_static(&child, source);
            }
        }
        if !cursor.goto_next_sibling() {
            break;
        }
    }
}

/// Recursively find inner class_definition nodes and check their body ordering.
fn check_inner_classes(node: Node, source: &str, diags: &mut Vec<LintDiagnostic>) {
    let mut cursor = node.walk();
    if !cursor.goto_first_child() {
        return;
    }
    loop {
        let child = cursor.node();
        if child.kind() == "class_definition"
            && let Some(body) = child.child_by_field_name("body")
        {
            check_member_order(body, source, diags);
            // Recurse for nested inner classes
            check_inner_classes(body, source, diags);
        }
        if !cursor.goto_next_sibling() {
            break;
        }
    }
}

/// Determine the category of a class member node. Returns None for
/// non-categorizable nodes (comments, blank lines, etc.).
fn categorize_member(node: &Node, source: &str) -> Option<u8> {
    match node.kind() {
        // Standalone annotations at top level (e.g. @tool, @icon)
        "class_name_statement" | "extends_statement" | "annotations" | "annotation" => {
            Some(CAT_HEADER)
        }
        "signal_statement" => Some(CAT_SIGNAL),
        "enum_definition" => Some(CAT_ENUM),
        "const_statement" => Some(CAT_CONST),
        "variable_statement" => {
            if has_annotation(node, source, "onready") {
                Some(CAT_ONREADY_VAR)
            } else if has_annotation(node, source, "export")
                || has_export_group_annotation(node, source)
            {
                Some(CAT_EXPORT_VAR)
            } else {
                let name = member_name(node, source).unwrap_or_default();
                if name.starts_with('_') {
                    Some(CAT_PRIVATE_VAR)
                } else {
                    Some(CAT_PUBLIC_VAR)
                }
            }
        }
        "function_definition" => {
            let name = member_name(node, source).unwrap_or_default();
            if LIFECYCLE_METHODS.contains(&name.as_str()) {
                Some(CAT_LIFECYCLE_METHOD)
            } else if name.starts_with('_') {
                Some(CAT_PRIVATE_METHOD)
            } else {
                Some(CAT_PUBLIC_METHOD)
            }
        }
        "class_definition" => Some(CAT_INNER_CLASS),
        _ => None,
    }
}

/// Check if a node has a specific annotation (e.g. "export", "onready").
fn has_annotation(node: &Node, source: &str, annotation_name: &str) -> bool {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "annotations" {
            let mut annot_cursor = child.walk();
            for annot in child.children(&mut annot_cursor) {
                if annot.kind() == "annotation" {
                    let mut ident_cursor = annot.walk();
                    for ident_child in annot.children(&mut ident_cursor) {
                        if ident_child.kind() == "identifier" {
                            let name = ident_child.utf8_text(source.as_bytes()).unwrap_or("");
                            if name == annotation_name {
                                return true;
                            }
                        }
                    }
                }
            }
        }
    }
    false
}

/// Check if a node has any @export_* annotation (export_category, export_group, etc.).
fn has_export_group_annotation(node: &Node, source: &str) -> bool {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "annotations" {
            let mut annot_cursor = child.walk();
            for annot in child.children(&mut annot_cursor) {
                if annot.kind() == "annotation" {
                    let mut ident_cursor = annot.walk();
                    for ident_child in annot.children(&mut ident_cursor) {
                        if ident_child.kind() == "identifier" {
                            let name = ident_child.utf8_text(source.as_bytes()).unwrap_or("");
                            if name.starts_with("export") {
                                return true;
                            }
                        }
                    }
                }
            }
        }
    }
    false
}

/// Extract the name of a member node (function, variable, signal, etc.).
fn member_name(node: &Node, source: &str) -> Option<String> {
    node.child_by_field_name("name")
        .map(|n| source[n.byte_range()].to_string())
}

/// Get a static string reference for the highest-seen member.
/// Returns a leaked &str for use in error messages. Since this is only
/// called for a small number of category transitions, leaking is acceptable.
fn member_name_static<'a>(node: &Node, source: &'a str) -> &'a str {
    if let Some(name_node) = node.child_by_field_name("name") {
        &source[name_node.byte_range()]
    } else {
        ""
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::parser;

    fn check(source: &str) -> Vec<LintDiagnostic> {
        let tree = parser::parse(source).unwrap();
        let config = LintConfig::default();
        ClassDefinitionsOrder.check(&tree, source, &config)
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
        // Both are category 0, no violation
        assert!(check(source).is_empty());
    }
}
