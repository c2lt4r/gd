use tree_sitter::Node;
use crate::core::gd_ast::GdFile;

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use crate::class_db;
use crate::core::config::LintConfig;
use crate::core::symbol_table::SymbolTable;

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
        symbols: &SymbolTable,
    ) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        check_node(file.node, source, symbols, &mut diags);
        diags
    }
}

fn check_node(node: Node, source: &str, symbols: &SymbolTable, diags: &mut Vec<LintDiagnostic>) {
    if node.kind() == "variable_statement" {
        check_variable(node, source, symbols, diags);
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            check_node(cursor.node(), source, symbols, diags);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

fn check_variable(
    node: Node,
    source: &str,
    symbols: &SymbolTable,
    diags: &mut Vec<LintDiagnostic>,
) {
    // Only check := (inferred type)
    let is_inferred = node
        .child_by_field_name("type")
        .is_some_and(|t| t.kind() == "inferred_type");
    if !is_inferred {
        return;
    }

    let Some(value) = node.child_by_field_name("value") else {
        return;
    };

    // RHS must be a member access: obj.member
    if value.kind() != "attribute" {
        return;
    }

    let Some(object_node) = value.named_child(0) else {
        return;
    };

    // Find the member name — second identifier child (after the object)
    let mut member_name = None;
    let mut cursor = value.walk();
    for child in value.children(&mut cursor) {
        if child.kind() == "identifier" && child != object_node {
            member_name = child.utf8_text(source.as_bytes()).ok();
            break;
        }
    }
    let Some(member_name) = member_name else {
        return;
    };

    // Resolve the type of the object
    let Some(obj_type) = resolve_object_type(&object_node, source, symbols) else {
        return;
    };

    // Only check engine classes — user classes may have properties we don't track here
    if !class_db::class_exists(&obj_type) {
        return;
    }

    // Check if member exists as a property, method, or signal on the resolved type
    if class_db::property_exists(&obj_type, member_name)
        || class_db::method_exists(&obj_type, member_name)
        || class_db::signal_exists(&obj_type, member_name)
    {
        return;
    }

    let var_name = node
        .child_by_field_name("name")
        .and_then(|n| n.utf8_text(source.as_bytes()).ok())
        .unwrap_or("?");

    diags.push(LintDiagnostic {
        rule: "infer-unknown-member",
        message: format!(
            "`:=` cannot infer type — `{member_name}` is not a known member of `{obj_type}`; \
             use an explicit type annotation for `{var_name}`"
        ),
        severity: Severity::Warning,
        line: node.start_position().row,
        column: node.start_position().column,
        end_column: None,
        fix: None,
        context_lines: None,
    });
}

/// Resolve the type of an expression node from the symbol table.
/// Returns the class name if it can be determined.
fn resolve_object_type(node: &Node, source: &str, symbols: &SymbolTable) -> Option<String> {
    match node.kind() {
        "identifier" => {
            let name = node.utf8_text(source.as_bytes()).ok()?;

            // Check class-level variable declarations
            for var in &symbols.variables {
                if var.name == name {
                    return var
                        .type_ann
                        .as_ref()
                        .filter(|t| !t.is_inferred)
                        .map(|t| t.name.clone());
                }
            }

            // Check function parameters in the enclosing function
            let func_node = find_enclosing_function(node)?;
            let name_node = func_node.child_by_field_name("name")?;
            let func_name = name_node.utf8_text(source.as_bytes()).ok()?;

            for func in &symbols.functions {
                if func.name == func_name {
                    for param in &func.params {
                        if param.name == name {
                            return param
                                .type_ann
                                .as_ref()
                                .filter(|t| !t.is_inferred)
                                .map(|t| t.name.clone());
                        }
                    }
                }
            }

            // Check local variable declarations before this line
            resolve_local_var_type(node, name, source)
        }
        _ => None,
    }
}

/// Walk up the tree to find the enclosing function definition.
fn find_enclosing_function<'a>(node: &'a Node<'a>) -> Option<Node<'a>> {
    let mut current = node.parent()?;
    loop {
        if current.kind() == "function_definition" || current.kind() == "constructor_definition" {
            return Some(current);
        }
        current = current.parent()?;
    }
}

/// Look for a local variable declaration with an explicit type annotation
/// in the same function body, before the current node's line.
fn resolve_local_var_type(node: &Node, name: &str, source: &str) -> Option<String> {
    let func_node = find_enclosing_function(node)?;
    let body = func_node.child_by_field_name("body")?;
    let target_line = node.start_position().row;

    find_typed_var_in_body(body, name, source, target_line)
}

fn find_typed_var_in_body(
    body: Node,
    name: &str,
    source: &str,
    before_line: usize,
) -> Option<String> {
    let mut cursor = body.walk();
    if !cursor.goto_first_child() {
        return None;
    }

    loop {
        let child = cursor.node();
        if child.start_position().row >= before_line {
            break;
        }

        if child.kind() == "variable_statement"
            && let Some(var_name) = child.child_by_field_name("name")
            && let Ok(vn) = var_name.utf8_text(source.as_bytes())
            && vn == name
            && let Some(type_node) = child.child_by_field_name("type")
            && type_node.kind() != "inferred_type"
        {
            return type_node
                .utf8_text(source.as_bytes())
                .ok()
                .map(String::from);
        }

        if !cursor.goto_next_sibling() {
            break;
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::gd_ast;
    use crate::core::{parser, symbol_table};

    fn check(source: &str) -> Vec<LintDiagnostic> {
        let tree = parser::parse(source).unwrap();
        let file = gd_ast::convert(&tree, source);
        let symbols = symbol_table::build(&tree, source);
        let config = LintConfig::default();
        InferUnknownMember.check_with_symbols(&file, source, &config, &symbols)
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
