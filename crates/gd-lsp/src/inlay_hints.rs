use tower_lsp::lsp_types::{InlayHint, InlayHintKind, InlayHintLabel, Position, Range};

use super::workspace::WorkspaceIndex;

/// Provide inlay hints for a GDScript source file within the given range.
///
/// Currently produces type hints for variables declared with `:=` (inferred type)
/// where the type can be statically determined.
pub fn provide_inlay_hints(
    source: &str,
    range: Range,
    _workspace: Option<&WorkspaceIndex>,
) -> Vec<InlayHint> {
    let Ok(tree) = gd_core::parser::parse(source) else {
        return Vec::new();
    };

    let file = gd_core::gd_ast::convert(&tree, source);
    let root = tree.root_node();
    let mut hints = Vec::new();

    collect_variable_type_hints(&root, source, &file, &range, &mut hints);

    hints
}

/// Walk the AST and collect type hints for `:=` variable declarations within `range`.
fn collect_variable_type_hints(
    node: &tree_sitter::Node,
    source: &str,
    file: &gd_core::gd_ast::GdFile,
    range: &Range,
    hints: &mut Vec<InlayHint>,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        let child_start_line = child.start_position().row as u32;
        let child_end_line = child.end_position().row as u32;

        // Skip nodes entirely outside the requested range.
        if child_end_line < range.start.line || child_start_line > range.end.line {
            continue;
        }

        if child.kind() == "variable_statement"
            && let Some(hint) = variable_type_hint(&child, source, file)
        {
            hints.push(hint);
        }

        // Recurse into children (function bodies, class bodies, etc.).
        collect_variable_type_hints(&child, source, file, range, hints);
    }
}

/// Try to produce a type hint for a single `variable_statement` node.
///
/// Only emits a hint when the declaration uses `:=` (inferred type marker)
/// and we can determine the type from the value expression.
fn variable_type_hint(
    node: &tree_sitter::Node,
    source: &str,
    file: &gd_core::gd_ast::GdFile,
) -> Option<InlayHint> {
    let bytes = source.as_bytes();

    // Only produce hints for `:=` declarations (inferred_type node in the "type" field).
    let type_node = node.child_by_field_name("type")?;
    if type_node.kind() != "inferred_type" {
        return None;
    }

    let name_node = node.child_by_field_name("name")?;
    let value_node = node.child_by_field_name("value")?;

    let type_name = infer_value_type(&value_node, bytes, file)?;

    // Place the hint right after the variable name.
    let name_end = name_node.end_position();
    Some(InlayHint {
        position: Position::new(name_end.row as u32, name_end.column as u32),
        label: InlayHintLabel::String(format!(": {type_name}")),
        kind: Some(InlayHintKind::TYPE),
        text_edits: None,
        tooltip: None,
        padding_left: None,
        padding_right: None,
        data: None,
    })
}

/// Infer the type name of a value expression for inlay hint display.
///
/// Returns the type as a display string, or `None` if the type cannot be determined.
fn infer_value_type(
    value: &tree_sitter::Node,
    source: &[u8],
    file: &gd_core::gd_ast::GdFile,
) -> Option<String> {
    match value.kind() {
        // Literals
        "integer" => Some("int".to_string()),
        "float" => Some("float".to_string()),
        "string" => Some("String".to_string()),
        "true" | "false" => Some("bool".to_string()),
        "array" => Some("Array".to_string()),
        "dictionary" => Some("Dictionary".to_string()),

        // Constructor / function call: `Vector2(1, 2)` or `RandomNumberGenerator.new()` via call
        "call" => infer_call_type(value, source),

        // Attribute expression: `ClassName.new()` or property access
        "attribute" => infer_attribute_type(value, source),

        // Try the full type inference engine as a fallback
        _ => {
            let source_str = std::str::from_utf8(source).ok()?;
            let inferred = gd_core::type_inference::infer_expression_type(value, source_str, file)?;
            let name = inferred.display_name();
            // Don't show hints for Variant or void — they aren't useful.
            if name == "Variant" || name == "void" {
                return None;
            }
            Some(name)
        }
    }
}

/// Infer type from a `call` node (builtin constructor like `Vector2(...)` or class name).
fn infer_call_type(node: &tree_sitter::Node, source: &[u8]) -> Option<String> {
    let func_node = node
        .child_by_field_name("function")
        .or_else(|| node.named_child(0))?;
    let func_name = func_node.utf8_text(source).ok()?;

    // PascalCase builtin constructors or known engine classes
    if gd_class_db::class_exists(func_name) {
        return Some(func_name.to_string());
    }

    // Builtin value-type constructors that aren't engine classes but are still PascalCase
    if is_builtin_constructor(func_name) {
        return Some(func_name.to_string());
    }

    None
}

/// Infer type from an `attribute` node (e.g. `ClassName.new()`).
fn infer_attribute_type(node: &tree_sitter::Node, source: &[u8]) -> Option<String> {
    // Check for `ClassName.new()` pattern:
    // attribute > [identifier (child 0), attribute_call > ["new", arguments]]
    let receiver = node.named_child(0)?;
    let receiver_name = receiver.utf8_text(source).ok()?;

    // Look for an attribute_call child with method name "new"
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "attribute_call"
            && let Some(method_node) = child.named_child(0)
        {
            let method_name = method_node.utf8_text(source).ok().unwrap_or("");
            if method_name == "new" && gd_class_db::class_exists(receiver_name) {
                return Some(receiver_name.to_string());
            }
        }
    }

    None
}

/// Check if a name is a known GDScript builtin value-type constructor.
fn is_builtin_constructor(name: &str) -> bool {
    matches!(
        name,
        "Vector2"
            | "Vector2i"
            | "Vector3"
            | "Vector3i"
            | "Vector4"
            | "Vector4i"
            | "Color"
            | "Rect2"
            | "Rect2i"
            | "Transform2D"
            | "Transform3D"
            | "Basis"
            | "Quaternion"
            | "AABB"
            | "Plane"
            | "StringName"
            | "NodePath"
            | "RID"
            | "Callable"
            | "Signal"
            | "PackedByteArray"
            | "PackedInt32Array"
            | "PackedInt64Array"
            | "PackedFloat32Array"
            | "PackedFloat64Array"
            | "PackedStringArray"
            | "PackedVector2Array"
            | "PackedVector3Array"
            | "PackedColorArray"
            | "PackedVector4Array"
            | "Array"
            | "Dictionary"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: a range covering lines 0 through 9999 (effectively the whole file).
    fn full_range() -> Range {
        Range::new(Position::new(0, 0), Position::new(9999, 0))
    }

    /// Extract the label string from an `InlayHint`.
    fn label_text(hint: &InlayHint) -> &str {
        match &hint.label {
            InlayHintLabel::String(s) => s.as_str(),
            InlayHintLabel::LabelParts(_) => "",
        }
    }

    #[test]
    fn inferred_var_from_constructor() {
        let source = "var rng := RandomNumberGenerator.new()\n";
        let hints = provide_inlay_hints(source, full_range(), None);
        assert_eq!(hints.len(), 1);
        assert_eq!(label_text(&hints[0]), ": RandomNumberGenerator");
        assert!(matches!(hints[0].kind, Some(InlayHintKind::TYPE)));
    }

    #[test]
    fn inferred_var_from_builtin_constructor() {
        let source = "var v := Vector2(1, 2)\n";
        let hints = provide_inlay_hints(source, full_range(), None);
        assert_eq!(hints.len(), 1);
        assert_eq!(label_text(&hints[0]), ": Vector2");
    }

    #[test]
    fn inferred_var_from_literal() {
        let source = "var x := 42\n";
        let hints = provide_inlay_hints(source, full_range(), None);
        assert_eq!(hints.len(), 1);
        assert_eq!(label_text(&hints[0]), ": int");
    }

    #[test]
    fn inferred_var_from_float_literal() {
        let source = "var y := 3.14\n";
        let hints = provide_inlay_hints(source, full_range(), None);
        assert_eq!(hints.len(), 1);
        assert_eq!(label_text(&hints[0]), ": float");
    }

    #[test]
    fn inferred_var_from_string_literal() {
        let source = "var s := \"hello\"\n";
        let hints = provide_inlay_hints(source, full_range(), None);
        assert_eq!(hints.len(), 1);
        assert_eq!(label_text(&hints[0]), ": String");
    }

    #[test]
    fn inferred_var_from_bool_literal() {
        let source = "var b := true\n";
        let hints = provide_inlay_hints(source, full_range(), None);
        assert_eq!(hints.len(), 1);
        assert_eq!(label_text(&hints[0]), ": bool");
    }

    #[test]
    fn explicit_type_no_hint() {
        let source = "var x: int = 42\n";
        let hints = provide_inlay_hints(source, full_range(), None);
        assert!(hints.is_empty(), "explicit type should not produce a hint");
    }

    #[test]
    fn no_type_annotation_no_hint() {
        let source = "var x = 42\n";
        let hints = provide_inlay_hints(source, full_range(), None);
        assert!(
            hints.is_empty(),
            "plain var without := should not produce a hint"
        );
    }

    #[test]
    fn no_hint_outside_range() {
        let source = "var a := 1\nvar b := 2\nvar c := 3\n";
        // Only request hints for line 1 (var b)
        let range = Range::new(Position::new(1, 0), Position::new(1, 999));
        let hints = provide_inlay_hints(source, range, None);
        assert_eq!(hints.len(), 1);
        assert_eq!(label_text(&hints[0]), ": int");
        // Verify the hint is on line 1
        assert_eq!(hints[0].position.line, 1);
    }

    #[test]
    fn hint_position_after_variable_name() {
        let source = "var my_var := 42\n";
        let hints = provide_inlay_hints(source, full_range(), None);
        assert_eq!(hints.len(), 1);
        // "var my_var" — name starts at col 4, ends at col 10
        assert_eq!(hints[0].position.line, 0);
        assert_eq!(hints[0].position.character, 10);
    }

    #[test]
    fn multiple_vars_produce_multiple_hints() {
        let source = "var a := 1\nvar b := \"hi\"\nvar c := true\n";
        let hints = provide_inlay_hints(source, full_range(), None);
        assert_eq!(hints.len(), 3);

        let labels: Vec<_> = hints.iter().map(label_text).collect();
        assert_eq!(labels, vec![": int", ": String", ": bool"]);
    }
}
