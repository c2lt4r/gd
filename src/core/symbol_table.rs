//! Per-file symbol table built from a tree-sitter AST.
//!
//! Layer 1: declaration-level type tracking. Records what types were **declared**
//! on variables, functions, signals, enums, and constants, plus resolves `extends`
//! chains against the engine class database.

use tree_sitter::{Node, Tree};

/// A type annotation on a declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeAnnotation {
    /// The type name, e.g. `"int"`, `"Array[int]"`, `"MyEnum"`.
    pub name: String,
    /// True if the type was inferred via `:=` rather than explicitly declared.
    pub is_inferred: bool,
}

/// A variable or constant declaration.
#[derive(Debug, Clone)]
pub struct VarDecl {
    pub name: String,
    pub type_ann: Option<TypeAnnotation>,
    pub is_constant: bool,
    pub is_static: bool,
    pub annotations: Vec<String>,
    pub has_default: bool,
    pub line: usize,
}

/// A function/method parameter.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ParamDecl {
    pub name: String,
    pub type_ann: Option<TypeAnnotation>,
    pub has_default: bool,
}

/// A function or method declaration.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct FuncDecl {
    pub name: String,
    pub params: Vec<ParamDecl>,
    pub return_type: Option<TypeAnnotation>,
    pub is_static: bool,
    pub annotations: Vec<String>,
    pub line: usize,
}

/// A signal declaration.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct SignalDecl {
    pub name: String,
    pub params: Vec<ParamDecl>,
    pub line: usize,
}

/// An enum declaration.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct EnumDecl {
    pub name: String,
    pub members: Vec<String>,
    pub line: usize,
}

/// Per-file symbol table with all top-level declarations.
#[derive(Debug, Clone)]
pub struct SymbolTable {
    pub class_name: Option<String>,
    pub extends: Option<String>,
    pub has_tool: bool,
    pub has_static_unload: bool,
    pub variables: Vec<VarDecl>,
    pub functions: Vec<FuncDecl>,
    pub signals: Vec<SignalDecl>,
    pub enums: Vec<EnumDecl>,
    pub inner_classes: Vec<(String, SymbolTable)>,
}

/// Build a symbol table from a parsed tree-sitter tree and source text.
pub fn build(tree: &Tree, source: &str) -> SymbolTable {
    build_from_node(tree.root_node(), source)
}

fn build_from_node(root: Node, source: &str) -> SymbolTable {
    let bytes = source.as_bytes();
    let mut table = SymbolTable {
        class_name: None,
        extends: None,
        has_tool: false,
        has_static_unload: false,
        variables: Vec::new(),
        functions: Vec::new(),
        signals: Vec::new(),
        enums: Vec::new(),
        inner_classes: Vec::new(),
    };

    // Collect children into a Vec so we can look back at preceding annotations.
    let mut cursor = root.walk();
    let children: Vec<Node> = root.children(&mut cursor).collect();

    for (idx, child) in children.iter().enumerate() {
        match child.kind() {
            "class_name_statement" => {
                if let Some(name_node) = child.child_by_field_name("name") {
                    table.class_name = name_node.utf8_text(bytes).ok().map(String::from);
                }
            }
            "extends_statement" => {
                table.extends = extract_extends(child, bytes);
            }
            // Standalone @tool / @static_unload at top level
            "annotations" | "annotation" => {
                check_top_level_annotations(child, bytes, &mut table);
            }
            "variable_statement" => {
                if let Some(mut var) = extract_var_decl(child, bytes) {
                    // Collect preceding standalone annotation nodes
                    collect_preceding_annotations(&children, idx, bytes, &mut var.annotations);
                    table.variables.push(var);
                }
            }
            "const_statement" => {
                if let Some(var) = extract_const_decl(child, bytes) {
                    table.variables.push(var);
                }
            }
            "function_definition" => {
                if let Some(mut func) = extract_func_decl(child, bytes) {
                    collect_preceding_annotations(&children, idx, bytes, &mut func.annotations);
                    table.functions.push(func);
                }
            }
            "constructor_definition" => {
                if let Some(func) = extract_constructor_decl(child, bytes) {
                    table.functions.push(func);
                }
            }
            "signal_statement" => {
                if let Some(sig) = extract_signal_decl(child, bytes) {
                    table.signals.push(sig);
                }
            }
            "enum_definition" => {
                if let Some(e) = extract_enum_decl(child, bytes) {
                    table.enums.push(e);
                }
            }
            "class_definition" => {
                if let Some(name_node) = child.child_by_field_name("name")
                    && let Ok(name) = name_node.utf8_text(bytes)
                    && let Some(body) = child.child_by_field_name("body")
                {
                    let inner = build_from_node(body, source);
                    table.inner_classes.push((name.to_string(), inner));
                }
            }
            _ => {}
        }
    }

    table
}

/// Walk backward from `idx` collecting standalone `annotation` nodes that
/// immediately precede a declaration (e.g. `@export` on the line before a var).
/// These are NOT wrapped in the declaration's own `annotations` child.
fn collect_preceding_annotations(
    children: &[Node],
    idx: usize,
    source: &[u8],
    annotations: &mut Vec<String>,
) {
    let mut i = idx;
    while i > 0 {
        i -= 1;
        let prev = &children[i];
        if prev.kind() == "annotation" {
            if let Some(name) = find_annotation_identifier(prev, source) {
                // Skip @tool / @static_unload — those are file-level, not per-decl
                if name != "tool" && name != "static_unload" {
                    annotations.push(name);
                }
            }
        } else if prev.kind() == "comment" {
            // Doc comments can appear between annotations and declarations
        } else {
            break;
        }
    }
}

fn extract_extends(node: &Node, source: &[u8]) -> Option<String> {
    for i in 0..node.named_child_count() {
        if let Some(type_node) = node.named_child(i)
            && (type_node.kind() == "type" || type_node.kind() == "identifier")
        {
            return type_node.utf8_text(source).ok().map(String::from);
        }
    }
    None
}

fn check_top_level_annotations(node: &Node, source: &[u8], table: &mut SymbolTable) {
    if node.kind() == "annotation" {
        if let Some(ident) = find_annotation_identifier(node, source) {
            match ident.as_str() {
                "tool" => table.has_tool = true,
                "static_unload" => table.has_static_unload = true,
                _ => {}
            }
        }
    } else if node.kind() == "annotations" {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "annotation" {
                check_top_level_annotations(&child, source, table);
            }
        }
    }
}

fn find_annotation_identifier(node: &Node, source: &[u8]) -> Option<String> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "identifier" {
            return child.utf8_text(source).ok().map(String::from);
        }
    }
    None
}

/// Collect all annotation identifiers from a declaration node's own
/// `annotations` child (the inline form, e.g. `@onready var x`).
fn collect_annotations(node: &Node, source: &[u8]) -> Vec<String> {
    let mut annotations = Vec::new();
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "annotations" {
            let mut annot_cursor = child.walk();
            for annot in child.children(&mut annot_cursor) {
                if annot.kind() == "annotation"
                    && let Some(name) = find_annotation_identifier(&annot, source)
                {
                    annotations.push(name);
                }
            }
        }
    }
    annotations
}

fn extract_var_decl(node: &Node, source: &[u8]) -> Option<VarDecl> {
    let name = node
        .child_by_field_name("name")?
        .utf8_text(source)
        .ok()?
        .to_string();

    let type_ann = extract_type_annotation(node, source);
    let annotations = collect_annotations(node, source);
    let has_default = node.child_by_field_name("value").is_some();
    let is_static = has_keyword(node, "static_keyword");

    Some(VarDecl {
        name,
        type_ann,
        is_constant: false,
        is_static,
        annotations,
        has_default,
        line: node.start_position().row,
    })
}

fn extract_const_decl(node: &Node, source: &[u8]) -> Option<VarDecl> {
    let name = node
        .child_by_field_name("name")?
        .utf8_text(source)
        .ok()?
        .to_string();

    let type_ann = extract_type_annotation(node, source);
    let has_default = node.child_by_field_name("value").is_some();

    Some(VarDecl {
        name,
        type_ann,
        is_constant: true,
        is_static: false,
        annotations: Vec::new(),
        has_default,
        line: node.start_position().row,
    })
}

fn extract_type_annotation(node: &Node, source: &[u8]) -> Option<TypeAnnotation> {
    let type_node = node.child_by_field_name("type")?;
    if type_node.kind() == "inferred_type" {
        return Some(TypeAnnotation {
            name: String::new(),
            is_inferred: true,
        });
    }
    let name = type_node.utf8_text(source).ok()?.to_string();
    Some(TypeAnnotation {
        name,
        is_inferred: false,
    })
}

fn extract_func_decl(node: &Node, source: &[u8]) -> Option<FuncDecl> {
    let name = node
        .child_by_field_name("name")?
        .utf8_text(source)
        .ok()?
        .to_string();

    let params = extract_params(node, source);
    let return_type = extract_return_type(node, source);
    let is_static = has_keyword(node, "static_keyword");
    let annotations = collect_annotations(node, source);

    Some(FuncDecl {
        name,
        params,
        return_type,
        is_static,
        annotations,
        line: node.start_position().row,
    })
}

#[allow(clippy::unnecessary_wraps)]
fn extract_constructor_decl(node: &Node, source: &[u8]) -> Option<FuncDecl> {
    let params = extract_params(node, source);
    let return_type = extract_return_type(node, source);

    Some(FuncDecl {
        name: "_init".to_string(),
        params,
        return_type,
        is_static: false,
        annotations: Vec::new(),
        line: node.start_position().row,
    })
}

fn extract_return_type(node: &Node, source: &[u8]) -> Option<TypeAnnotation> {
    let ret_node = node.child_by_field_name("return_type")?;
    // return_type is a `type` node wrapping an identifier
    let text = ret_node.utf8_text(source).ok()?;
    Some(TypeAnnotation {
        name: text.to_string(),
        is_inferred: false,
    })
}

fn extract_params(node: &Node, source: &[u8]) -> Vec<ParamDecl> {
    let Some(params_node) = node.child_by_field_name("parameters") else {
        return Vec::new();
    };
    let mut params = Vec::new();
    let mut cursor = params_node.walk();
    for child in params_node.children(&mut cursor) {
        match child.kind() {
            "identifier" => {
                // Untyped parameter without default
                if let Ok(name) = child.utf8_text(source) {
                    params.push(ParamDecl {
                        name: name.to_string(),
                        type_ann: None,
                        has_default: false,
                    });
                }
            }
            "typed_parameter" => {
                if let Some(p) = extract_typed_param(&child, source) {
                    params.push(p);
                }
            }
            "default_parameter" => {
                if let Some(p) = extract_untyped_default_param(&child, source) {
                    params.push(p);
                }
            }
            "typed_default_parameter" => {
                if let Some(p) = extract_typed_default_param(&child, source) {
                    params.push(p);
                }
            }
            _ => {}
        }
    }
    params
}

/// `typed_parameter`: `identifier : type`
/// No `name` field — the identifier is the first named child.
fn extract_typed_param(node: &Node, source: &[u8]) -> Option<ParamDecl> {
    let name = first_identifier_text(node, source)?;

    let type_ann = node.child_by_field_name("type").and_then(|t| {
        if t.kind() == "inferred_type" {
            Some(TypeAnnotation {
                name: String::new(),
                is_inferred: true,
            })
        } else {
            t.utf8_text(source).ok().map(|s| TypeAnnotation {
                name: s.to_string(),
                is_inferred: false,
            })
        }
    });

    Some(ParamDecl {
        name,
        type_ann,
        has_default: false,
    })
}

/// `default_parameter`: `identifier = value` (untyped with default)
fn extract_untyped_default_param(node: &Node, source: &[u8]) -> Option<ParamDecl> {
    let name = first_identifier_text(node, source)?;
    Some(ParamDecl {
        name,
        type_ann: None,
        has_default: true,
    })
}

/// `typed_default_parameter`: `identifier : type = value` or `identifier := value`
fn extract_typed_default_param(node: &Node, source: &[u8]) -> Option<ParamDecl> {
    let name = first_identifier_text(node, source)?;

    let type_ann = node.child_by_field_name("type").and_then(|t| {
        if t.kind() == "inferred_type" {
            Some(TypeAnnotation {
                name: String::new(),
                is_inferred: true,
            })
        } else {
            t.utf8_text(source).ok().map(|s| TypeAnnotation {
                name: s.to_string(),
                is_inferred: false,
            })
        }
    });

    Some(ParamDecl {
        name,
        type_ann,
        has_default: true,
    })
}

/// Get the text of the first `identifier` child of a node.
fn first_identifier_text(node: &Node, source: &[u8]) -> Option<String> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "identifier" {
            return child.utf8_text(source).ok().map(String::from);
        }
    }
    None
}

fn extract_signal_decl(node: &Node, source: &[u8]) -> Option<SignalDecl> {
    let name = node
        .child_by_field_name("name")?
        .utf8_text(source)
        .ok()?
        .to_string();

    let params = extract_params(node, source);

    Some(SignalDecl {
        name,
        params,
        line: node.start_position().row,
    })
}

#[allow(clippy::unnecessary_wraps)]
fn extract_enum_decl(node: &Node, source: &[u8]) -> Option<EnumDecl> {
    let name = node
        .child_by_field_name("name")
        .and_then(|n| n.utf8_text(source).ok())
        .unwrap_or("")
        .to_string();

    let mut members = Vec::new();
    if let Some(body) = node.child_by_field_name("body") {
        let mut cursor = body.walk();
        for child in body.children(&mut cursor) {
            if child.kind() == "enumerator" {
                // Enumerator name is in the `left` field, not `name`
                if let Some(name_node) = child.child_by_field_name("left")
                    && let Ok(member_name) = name_node.utf8_text(source)
                {
                    members.push(member_name.to_string());
                }
            }
        }
    }

    Some(EnumDecl {
        name,
        members,
        line: node.start_position().row,
    })
}

/// Check if a node has a named keyword child (e.g. `"static_keyword"`).
fn has_keyword(node: &Node, keyword_kind: &str) -> bool {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == keyword_kind {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::parser;

    fn build_table(source: &str) -> SymbolTable {
        let tree = parser::parse(source).unwrap();
        build(&tree, source)
    }

    #[test]
    fn class_name_and_extends() {
        let source = "class_name Player\nextends CharacterBody2D\n";
        let table = build_table(source);
        assert_eq!(table.class_name.as_deref(), Some("Player"));
        assert_eq!(table.extends.as_deref(), Some("CharacterBody2D"));
    }

    #[test]
    fn no_class_name() {
        let source = "extends Node\n";
        let table = build_table(source);
        assert!(table.class_name.is_none());
        assert_eq!(table.extends.as_deref(), Some("Node"));
    }

    #[test]
    fn tool_annotation() {
        let source = "@tool\nextends Node\n";
        let table = build_table(source);
        assert!(table.has_tool);
        assert!(!table.has_static_unload);
    }

    #[test]
    fn static_unload_annotation() {
        let source = "@static_unload\nextends Node\n";
        let table = build_table(source);
        assert!(table.has_static_unload);
        assert!(!table.has_tool);
    }

    #[test]
    fn tool_and_static_unload() {
        let source = "@tool\n@static_unload\nextends Node\n";
        let table = build_table(source);
        assert!(table.has_tool);
        assert!(table.has_static_unload);
    }

    #[test]
    fn variable_with_type() {
        let source = "var health: int = 100\n";
        let table = build_table(source);
        assert_eq!(table.variables.len(), 1);
        let var = &table.variables[0];
        assert_eq!(var.name, "health");
        assert!(!var.is_constant);
        assert!(!var.is_static);
        assert!(var.has_default);
        let ann = var.type_ann.as_ref().unwrap();
        assert_eq!(ann.name, "int");
        assert!(!ann.is_inferred);
    }

    #[test]
    fn variable_inferred_type() {
        let source = "var speed := 5.0\n";
        let table = build_table(source);
        assert_eq!(table.variables.len(), 1);
        let var = &table.variables[0];
        assert_eq!(var.name, "speed");
        assert!(var.has_default);
        let ann = var.type_ann.as_ref().unwrap();
        assert!(ann.is_inferred);
    }

    #[test]
    fn variable_no_type() {
        let source = "var data = null\n";
        let table = build_table(source);
        assert_eq!(table.variables.len(), 1);
        assert!(table.variables[0].type_ann.is_none());
        assert!(table.variables[0].has_default);
    }

    #[test]
    fn variable_no_default() {
        let source = "var health: int\n";
        let table = build_table(source);
        assert_eq!(table.variables.len(), 1);
        assert!(!table.variables[0].has_default);
    }

    #[test]
    fn static_variable() {
        let source = "static var instance: Node\n";
        let table = build_table(source);
        assert_eq!(table.variables.len(), 1);
        assert!(table.variables[0].is_static);
    }

    #[test]
    fn constant() {
        let source = "const MAX_SPEED: float = 10.0\n";
        let table = build_table(source);
        assert_eq!(table.variables.len(), 1);
        let var = &table.variables[0];
        assert_eq!(var.name, "MAX_SPEED");
        assert!(var.is_constant);
        assert!(var.has_default);
        let ann = var.type_ann.as_ref().unwrap();
        assert_eq!(ann.name, "float");
    }

    #[test]
    fn export_annotation() {
        let source = "@export var health: int = 100\n";
        let table = build_table(source);
        assert_eq!(table.variables.len(), 1);
        assert_eq!(table.variables[0].annotations, vec!["export"]);
    }

    #[test]
    fn onready_annotation() {
        let source = "@onready var sprite = $Sprite2D\n";
        let table = build_table(source);
        assert_eq!(table.variables.len(), 1);
        assert_eq!(table.variables[0].annotations, vec!["onready"]);
    }

    #[test]
    fn multiple_annotations() {
        let source = "@export\n@onready var sprite = $Sprite2D\n";
        let table = build_table(source);
        assert_eq!(table.variables.len(), 1);
        let annots = &table.variables[0].annotations;
        assert!(annots.contains(&"export".to_string()));
        assert!(annots.contains(&"onready".to_string()));
    }

    #[test]
    fn function_basic() {
        let source = "func move(dir: Vector2) -> void:\n\tpass\n";
        let table = build_table(source);
        assert_eq!(table.functions.len(), 1);
        let func = &table.functions[0];
        assert_eq!(func.name, "move");
        assert!(!func.is_static);
        assert_eq!(func.params.len(), 1);
        assert_eq!(func.params[0].name, "dir");
        assert_eq!(func.params[0].type_ann.as_ref().unwrap().name, "Vector2");
        assert_eq!(func.return_type.as_ref().unwrap().name, "void");
    }

    #[test]
    fn function_no_params_no_return() {
        let source = "func idle():\n\tpass\n";
        let table = build_table(source);
        assert_eq!(table.functions.len(), 1);
        let func = &table.functions[0];
        assert_eq!(func.name, "idle");
        assert!(func.params.is_empty());
        assert!(func.return_type.is_none());
    }

    #[test]
    fn static_function() {
        let source = "static func create() -> Node:\n\treturn Node.new()\n";
        let table = build_table(source);
        assert_eq!(table.functions.len(), 1);
        assert!(table.functions[0].is_static);
    }

    #[test]
    fn constructor() {
        let source = "func _init(x: int):\n\tpass\n";
        let table = build_table(source);
        assert_eq!(table.functions.len(), 1);
        let func = &table.functions[0];
        assert_eq!(func.name, "_init");
        assert_eq!(func.params.len(), 1);
    }

    #[test]
    fn signal_basic() {
        let source = "signal health_changed(new_health: int)\n";
        let table = build_table(source);
        assert_eq!(table.signals.len(), 1);
        let sig = &table.signals[0];
        assert_eq!(sig.name, "health_changed");
        assert_eq!(sig.params.len(), 1);
        assert_eq!(sig.params[0].name, "new_health");
    }

    #[test]
    fn signal_no_params() {
        let source = "signal died\n";
        let table = build_table(source);
        assert_eq!(table.signals.len(), 1);
        assert_eq!(table.signals[0].name, "died");
        assert!(table.signals[0].params.is_empty());
    }

    #[test]
    fn enum_basic() {
        let source = "enum State { IDLE, RUNNING, JUMPING }\n";
        let table = build_table(source);
        assert_eq!(table.enums.len(), 1);
        let e = &table.enums[0];
        assert_eq!(e.name, "State");
        assert_eq!(e.members, vec!["IDLE", "RUNNING", "JUMPING"]);
    }

    #[test]
    fn anonymous_enum() {
        let source = "enum { A, B, C }\n";
        let table = build_table(source);
        assert_eq!(table.enums.len(), 1);
        assert_eq!(table.enums[0].name, "");
        assert_eq!(table.enums[0].members, vec!["A", "B", "C"]);
    }

    #[test]
    fn inner_class() {
        let source = "\
class InnerThing:
\tvar x: int
\tfunc do_it() -> void:
\t\tpass
";
        let table = build_table(source);
        assert_eq!(table.inner_classes.len(), 1);
        let (name, inner) = &table.inner_classes[0];
        assert_eq!(name, "InnerThing");
        assert_eq!(inner.variables.len(), 1);
        assert_eq!(inner.functions.len(), 1);
    }

    #[test]
    fn default_parameter() {
        let source = "func f(x: int = 5, y := 10):\n\tpass\n";
        let table = build_table(source);
        let func = &table.functions[0];
        assert_eq!(func.params.len(), 2);
        assert!(func.params[0].has_default);
        assert_eq!(func.params[0].type_ann.as_ref().unwrap().name, "int");
        assert!(func.params[1].has_default);
        assert!(func.params[1].type_ann.as_ref().unwrap().is_inferred);
    }

    #[test]
    fn full_script() {
        let source = "\
@tool
class_name Player
extends CharacterBody2D

signal died
signal health_changed(new_value: int)

enum State { IDLE, RUN, JUMP }

const MAX_SPEED: float = 300.0

@export var health: int = 100
@onready var sprite := $Sprite2D
var _internal: String
static var count: int = 0

func _ready() -> void:
\tpass

func move(direction: Vector2, speed: float = MAX_SPEED) -> void:
\tpass

static func get_count() -> int:
\treturn count
";
        let table = build_table(source);
        assert!(table.has_tool);
        assert_eq!(table.class_name.as_deref(), Some("Player"));
        assert_eq!(table.extends.as_deref(), Some("CharacterBody2D"));
        assert_eq!(table.signals.len(), 2);
        assert_eq!(table.enums.len(), 1);
        // MAX_SPEED + health + sprite + _internal + count = 5
        assert_eq!(table.variables.len(), 5);
        // _ready + move + get_count = 3
        assert_eq!(table.functions.len(), 3);

        let health = table.variables.iter().find(|v| v.name == "health").unwrap();
        assert!(health.annotations.contains(&"export".to_string()));

        let sprite = table.variables.iter().find(|v| v.name == "sprite").unwrap();
        assert!(sprite.annotations.contains(&"onready".to_string()));

        let internal = table
            .variables
            .iter()
            .find(|v| v.name == "_internal")
            .unwrap();
        assert!(internal.annotations.is_empty());

        let count = table.variables.iter().find(|v| v.name == "count").unwrap();
        assert!(count.is_static);

        let get_count = table
            .functions
            .iter()
            .find(|f| f.name == "get_count")
            .unwrap();
        assert!(get_count.is_static);
    }

    #[test]
    fn function_with_annotation() {
        let source = "@rpc(\"any_peer\")\nfunc sync_state() -> void:\n\tpass\n";
        let table = build_table(source);
        assert_eq!(table.functions.len(), 1);
        assert!(table.functions[0].annotations.contains(&"rpc".to_string()));
    }
}
