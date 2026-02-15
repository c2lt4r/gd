use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, Documentation, InsertTextFormat, MarkupContent, MarkupKind,
    Position,
};

use super::workspace::WorkspaceIndex;

/// GDScript keywords.
const KEYWORDS: &[&str] = &[
    "func",
    "var",
    "const",
    "signal",
    "class",
    "extends",
    "if",
    "elif",
    "else",
    "for",
    "while",
    "match",
    "return",
    "break",
    "continue",
    "pass",
    "await",
    "yield",
    "self",
    "super",
    "true",
    "false",
    "null",
    "void",
    "preload",
    "load",
    "export",
    "onready",
    "static",
    "class_name",
    "tool",
    "enum",
];

/// GDScript built-in types.
const BUILTIN_TYPES: &[&str] = &[
    "int",
    "float",
    "bool",
    "String",
    "Vector2",
    "Vector3",
    "Vector4",
    "Vector2i",
    "Vector3i",
    "Vector4i",
    "Array",
    "Dictionary",
    "NodePath",
    "StringName",
    "Color",
    "Rect2",
    "Transform2D",
    "Transform3D",
    "Basis",
    "AABB",
    "Plane",
    "Quaternion",
    "PackedByteArray",
    "PackedInt32Array",
    "PackedInt64Array",
    "PackedFloat32Array",
    "PackedFloat64Array",
    "PackedStringArray",
    "PackedVector2Array",
    "PackedVector3Array",
];

/// GDScript built-in functions.
const BUILTIN_FUNCTIONS: &[&str] = &[
    "print",
    "prints",
    "printt",
    "printerr",
    "push_error",
    "push_warning",
    "str",
    "int",
    "float",
    "bool",
    "len",
    "range",
    "typeof",
    "is_instance_of",
    "abs",
    "sign",
    "min",
    "max",
    "clamp",
    "lerp",
    "smoothstep",
    "sqrt",
    "pow",
    "sin",
    "cos",
    "tan",
    "floor",
    "ceil",
    "round",
    "randi",
    "randf",
    "randomize",
    "seed",
    "hash",
    "is_equal_approx",
    "is_zero_approx",
];

/// Godot lifecycle methods with snippet parameter templates.
const LIFECYCLE_METHODS: &[(&str, &str)] = &[
    ("_ready", "_ready():\n\t${0:pass}"),
    ("_process", "_process(${1:delta: float}):\n\t${0:pass}"),
    (
        "_physics_process",
        "_physics_process(${1:delta: float}):\n\t${0:pass}",
    ),
    ("_input", "_input(${1:event: InputEvent}):\n\t${0:pass}"),
    (
        "_unhandled_input",
        "_unhandled_input(${1:event: InputEvent}):\n\t${0:pass}",
    ),
    ("_enter_tree", "_enter_tree():\n\t${0:pass}"),
    ("_exit_tree", "_exit_tree():\n\t${0:pass}"),
    ("_init", "_init():\n\t${0:pass}"),
    (
        "_notification",
        "_notification(${1:what: int}):\n\t${0:pass}",
    ),
    ("_draw", "_draw():\n\t${0:pass}"),
    (
        "_gui_input",
        "_gui_input(${1:event: InputEvent}):\n\t${0:pass}",
    ),
];

/// Provide completion items at the given position.
pub fn provide_completions(
    source: &str,
    _position: Position,
    workspace: Option<&WorkspaceIndex>,
) -> Vec<CompletionItem> {
    let mut items = Vec::new();

    // Keywords
    for &kw in KEYWORDS {
        items.push(CompletionItem {
            label: kw.to_string(),
            kind: Some(CompletionItemKind::KEYWORD),
            ..Default::default()
        });
    }

    // Built-in types
    for &ty in BUILTIN_TYPES {
        let documentation = super::builtins::lookup_type(ty).map(|doc| {
            Documentation::MarkupContent(MarkupContent {
                kind: MarkupKind::Markdown,
                value: doc.description.to_string(),
            })
        });
        items.push(CompletionItem {
            label: ty.to_string(),
            kind: Some(CompletionItemKind::CLASS),
            documentation,
            ..Default::default()
        });
    }

    // Built-in functions
    for &func in BUILTIN_FUNCTIONS {
        let documentation = super::builtins::lookup_function(func).map(|doc| {
            Documentation::MarkupContent(MarkupContent {
                kind: MarkupKind::Markdown,
                value: doc.description.to_string(),
            })
        });
        items.push(CompletionItem {
            label: func.to_string(),
            kind: Some(CompletionItemKind::FUNCTION),
            insert_text: Some(format!("{func}($0)")),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            documentation,
            ..Default::default()
        });
    }

    // Lifecycle methods (snippets)
    for &(label, snippet) in LIFECYCLE_METHODS {
        items.push(CompletionItem {
            label: label.to_string(),
            kind: Some(CompletionItemKind::METHOD),
            detail: Some("Godot lifecycle".to_string()),
            insert_text: Some(snippet.to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            ..Default::default()
        });
    }

    // Symbols from the current file
    if let Ok(tree) = crate::core::parser::parse(source) {
        collect_file_symbols(tree.root_node(), source, &mut items);
    }

    // Symbols from workspace (other files)
    if let Some(ws) = workspace {
        for (path, content) in ws.all_files() {
            if let Ok(tree) = crate::core::parser::parse(&content) {
                let file_name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown");
                collect_workspace_symbols(tree.root_node(), &content, file_name, &mut items);
            }
        }
    }

    // Engine methods from class_db based on extends clause
    if let Ok(tree) = crate::core::parser::parse(source)
        && let Some(extends_class) = find_extends_class(tree.root_node(), source)
    {
        collect_class_db_methods(&extends_class, &mut items);
    }

    items
}

/// Find the class name from the `extends` statement at the top of the file.
fn find_extends_class(root: tree_sitter::Node, source: &str) -> Option<String> {
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        if child.kind() == "extends_statement" {
            let mut inner = child.walk();
            for c in child.children(&mut inner) {
                if c.kind() != "extends" {
                    let text = c.utf8_text(source.as_bytes()).ok()?;
                    if crate::class_db::class_exists(text) {
                        return Some(text.to_string());
                    }
                }
            }
        }
    }
    None
}

/// Add engine methods from the class_db for the given class and its ancestors.
fn collect_class_db_methods(class: &str, items: &mut Vec<CompletionItem>) {
    for (method_name, ret_type, owner_class) in crate::class_db::class_methods(class) {
        items.push(CompletionItem {
            label: method_name.to_string(),
            kind: Some(CompletionItemKind::METHOD),
            detail: Some(format!("{owner_class}.{method_name}() -> {ret_type}")),
            ..Default::default()
        });
    }
}

/// Extract `##` doc comment lines preceding a declaration node.
fn extract_doc_comment(node: &tree_sitter::Node, source: &str) -> Option<Documentation> {
    let bytes = source.as_bytes();
    let mut lines = Vec::new();
    let mut current = node.prev_named_sibling();

    while let Some(prev) = current {
        match prev.kind() {
            "comment" => {
                if let Ok(text) = prev.utf8_text(bytes) {
                    if let Some(stripped) = text.strip_prefix("##") {
                        lines.push(stripped.trim().to_string());
                    } else {
                        break;
                    }
                }
            }
            "annotation" | "annotations" => {}
            _ => break,
        }
        current = prev.prev_named_sibling();
    }

    if lines.is_empty() {
        None
    } else {
        lines.reverse();
        Some(Documentation::MarkupContent(MarkupContent {
            kind: MarkupKind::Markdown,
            value: lines.join("\n"),
        }))
    }
}

/// Collect symbols from a file's AST as completion items.
fn collect_file_symbols(node: tree_sitter::Node, source: &str, items: &mut Vec<CompletionItem>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "function_definition" => {
                if let Some(name) = child_name(&child, source) {
                    items.push(CompletionItem {
                        label: name.to_string(),
                        kind: Some(CompletionItemKind::FUNCTION),
                        detail: Some(build_function_detail(&child, source)),
                        documentation: extract_doc_comment(&child, source),
                        ..Default::default()
                    });
                }
            }
            "variable_statement" => {
                if let Some(name) = child_name(&child, source) {
                    items.push(CompletionItem {
                        label: name.to_string(),
                        kind: Some(CompletionItemKind::VARIABLE),
                        documentation: extract_doc_comment(&child, source),
                        ..Default::default()
                    });
                }
            }
            "const_statement" => {
                if let Some(name) = child_name(&child, source) {
                    items.push(CompletionItem {
                        label: name.to_string(),
                        kind: Some(CompletionItemKind::CONSTANT),
                        documentation: extract_doc_comment(&child, source),
                        ..Default::default()
                    });
                }
            }
            "signal_statement" => {
                if let Some(name) = child_name(&child, source) {
                    items.push(CompletionItem {
                        label: name.to_string(),
                        kind: Some(CompletionItemKind::EVENT),
                        documentation: extract_doc_comment(&child, source),
                        ..Default::default()
                    });
                }
            }
            "class_definition" => {
                if let Some(name) = child_name(&child, source) {
                    items.push(CompletionItem {
                        label: name.to_string(),
                        kind: Some(CompletionItemKind::CLASS),
                        documentation: extract_doc_comment(&child, source),
                        ..Default::default()
                    });
                }
            }
            "enum_definition" => {
                if let Some(name) = child_name(&child, source) {
                    items.push(CompletionItem {
                        label: name.to_string(),
                        kind: Some(CompletionItemKind::ENUM),
                        documentation: extract_doc_comment(&child, source),
                        ..Default::default()
                    });
                }
            }
            _ => {}
        }
    }
}

/// Collect top-level symbols from a workspace file.
fn collect_workspace_symbols(
    node: tree_sitter::Node,
    source: &str,
    file_name: &str,
    items: &mut Vec<CompletionItem>,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "function_definition" => {
                if let Some(name) = child_name(&child, source) {
                    items.push(CompletionItem {
                        label: name.to_string(),
                        kind: Some(CompletionItemKind::FUNCTION),
                        detail: Some(file_name.to_string()),
                        documentation: extract_doc_comment(&child, source),
                        ..Default::default()
                    });
                }
            }
            "class_definition" | "class_name_statement" => {
                if let Some(name) = child_name(&child, source) {
                    items.push(CompletionItem {
                        label: name.to_string(),
                        kind: Some(CompletionItemKind::CLASS),
                        detail: Some(file_name.to_string()),
                        documentation: extract_doc_comment(&child, source),
                        ..Default::default()
                    });
                }
            }
            "signal_statement" => {
                if let Some(name) = child_name(&child, source) {
                    items.push(CompletionItem {
                        label: name.to_string(),
                        kind: Some(CompletionItemKind::EVENT),
                        detail: Some(file_name.to_string()),
                        documentation: extract_doc_comment(&child, source),
                        ..Default::default()
                    });
                }
            }
            _ => {}
        }
    }
}

fn child_name<'a>(node: &tree_sitter::Node, source: &'a str) -> Option<&'a str> {
    let name_node = node.child_by_field_name("name")?;
    name_node.utf8_text(source.as_bytes()).ok()
}

fn build_function_detail(node: &tree_sitter::Node, source: &str) -> String {
    let mut detail = "func(".to_string();
    if let Some(params) = node.child_by_field_name("parameters") {
        let params_text = params.utf8_text(source.as_bytes()).unwrap_or("()");
        let inner = params_text
            .strip_prefix('(')
            .and_then(|s| s.strip_suffix(')'))
            .unwrap_or(params_text);
        detail.push_str(inner);
    }
    detail.push(')');
    if let Some(ret) = node.child_by_field_name("return_type") {
        detail.push_str(" -> ");
        detail.push_str(ret.utf8_text(source.as_bytes()).unwrap_or("unknown"));
    }
    detail
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keywords_and_builtins_present() {
        let items = provide_completions("", Position::new(0, 0), None);
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(labels.contains(&"func"));
        assert!(labels.contains(&"var"));
        assert!(labels.contains(&"Vector2"));
        assert!(labels.contains(&"print"));
        assert!(labels.contains(&"_ready"));
    }

    #[test]
    fn lifecycle_methods_are_snippets() {
        let items = provide_completions("", Position::new(0, 0), None);
        let ready = items.iter().find(|i| i.label == "_ready").unwrap();
        assert_eq!(ready.insert_text_format, Some(InsertTextFormat::SNIPPET));
        assert!(ready.insert_text.as_ref().unwrap().contains("pass"));
    }

    #[test]
    fn builtin_functions_are_snippets() {
        let items = provide_completions("", Position::new(0, 0), None);
        let print_item = items.iter().find(|i| i.label == "print").unwrap();
        assert_eq!(
            print_item.insert_text_format,
            Some(InsertTextFormat::SNIPPET)
        );
        assert_eq!(print_item.insert_text.as_deref(), Some("print($0)"));
    }

    #[test]
    fn collects_symbols_from_source() {
        let source = r"
var health := 100
const MAX_SPEED = 200
signal damage_taken
enum State { IDLE, RUN }

func _ready():
    pass

func attack(target):
    pass
";
        let items = provide_completions(source, Position::new(0, 0), None);
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(labels.contains(&"health"));
        assert!(labels.contains(&"MAX_SPEED"));
        assert!(labels.contains(&"damage_taken"));
        assert!(labels.contains(&"State"));
        assert!(labels.contains(&"attack"));
    }

    #[test]
    fn function_detail_includes_params() {
        let source = "func move(speed: float, dir: Vector2) -> bool:\n\tpass\n";
        let items = provide_completions(source, Position::new(0, 0), None);
        let move_item = items
            .iter()
            .find(|i| i.label == "move" && i.kind == Some(CompletionItemKind::FUNCTION))
            .unwrap();
        let detail = move_item.detail.as_deref().unwrap();
        assert!(detail.contains("speed: float"));
        assert!(detail.contains("-> bool"));
    }

    #[test]
    fn extends_adds_class_db_methods() {
        let source = "extends Node2D\n\nfunc _ready():\n\tpass\n";
        let items = provide_completions(source, Position::new(0, 0), None);
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        // Node2D own method
        assert!(labels.contains(&"apply_scale"));
        // Inherited from Node
        assert!(labels.contains(&"add_child"));
    }

    #[test]
    fn extends_method_detail_shows_class() {
        let source = "extends Node2D\n";
        let items = provide_completions(source, Position::new(0, 0), None);
        let add_child = items
            .iter()
            .find(|i| i.label == "add_child" && i.kind == Some(CompletionItemKind::METHOD))
            .unwrap();
        let detail = add_child.detail.as_deref().unwrap();
        assert!(detail.contains("Node.add_child()"));
    }

    #[test]
    fn no_class_db_methods_without_extends() {
        let source = "func _ready():\n\tpass\n";
        let items = provide_completions(source, Position::new(0, 0), None);
        // add_child should not appear (only lifecycle snippets and file symbols)
        let engine_methods: Vec<&CompletionItem> = items
            .iter()
            .filter(|i| {
                i.kind == Some(CompletionItemKind::METHOD)
                    && i.detail.as_deref().is_some_and(|d| d.contains("Node."))
            })
            .collect();
        assert!(engine_methods.is_empty());
    }

    #[test]
    fn completion_includes_doc_comment() {
        let source = "## Move the player forward.\nfunc move():\n\tpass\n";
        let items = provide_completions(source, Position::new(0, 0), None);
        let move_item = items
            .iter()
            .find(|i| i.label == "move" && i.kind == Some(CompletionItemKind::FUNCTION))
            .unwrap();
        match &move_item.documentation {
            Some(Documentation::MarkupContent(mc)) => {
                assert_eq!(mc.value, "Move the player forward.");
            }
            _ => panic!("Expected MarkupContent documentation"),
        }
    }

    #[test]
    fn completion_no_doc_comment() {
        let source = "func idle():\n\tpass\n";
        let items = provide_completions(source, Position::new(0, 0), None);
        let idle_item = items
            .iter()
            .find(|i| i.label == "idle" && i.kind == Some(CompletionItemKind::FUNCTION))
            .unwrap();
        assert!(idle_item.documentation.is_none());
    }

    #[test]
    fn completion_var_doc_comment() {
        let source = "## The player's health.\nvar health: int = 100\n";
        let items = provide_completions(source, Position::new(0, 0), None);
        let health_item = items
            .iter()
            .find(|i| i.label == "health" && i.kind == Some(CompletionItemKind::VARIABLE))
            .unwrap();
        assert!(health_item.documentation.is_some());
    }
}
