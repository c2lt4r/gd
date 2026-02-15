use tower_lsp::lsp_types::{Hover, HoverContents, MarkupContent, MarkupKind, Position};

use super::util::{matches_name, node_range, node_text};

/// Provide hover information at the given position.
pub fn hover_at(source: &str, position: Position) -> Option<Hover> {
    let tree = crate::core::parser::parse(source).ok()?;
    let root = tree.root_node();

    // Find the most specific node at the cursor position
    let point = tree_sitter::Point::new(position.line as usize, position.character as usize);
    let node = root.descendant_for_point_range(point, point)?;

    // Walk up the tree to find a meaningful declaration node
    let mut current = node;
    loop {
        match current.kind() {
            "function_definition" => {
                // Show hover for the function's name or keyword, not for
                // arbitrary identifiers inside the function body.
                if is_declaration_name(&current, &node)
                    || is_on_declaration_keyword(&current, point)
                {
                    return hover_function(&current, source);
                }
                return None;
            }
            "variable_statement" => {
                if is_declaration_name(&current, &node)
                    || is_on_declaration_keyword(&current, point)
                {
                    return Some(hover_variable(&current, source));
                }
                return None;
            }
            "const_statement" => {
                if is_declaration_name(&current, &node)
                    || is_on_declaration_keyword(&current, point)
                {
                    return Some(hover_const(&current, source));
                }
                return None;
            }
            "signal_statement" => {
                if is_declaration_name(&current, &node)
                    || is_on_declaration_keyword(&current, point)
                {
                    return Some(hover_signal(&current, source));
                }
                return None;
            }
            "class_name_statement" => return Some(hover_class_name(&current, source)),
            "class_definition" => {
                if is_declaration_name(&current, &node)
                    || is_on_declaration_keyword(&current, point)
                {
                    return hover_class(&current, source);
                }
                return None;
            }
            "enum_definition" => {
                if is_declaration_name(&current, &node)
                    || is_on_declaration_keyword(&current, point)
                {
                    return hover_enum(&current, source);
                }
                return None;
            }
            "identifier" | "name" => {
                return resolve_hover_for_identifier(&current, &root, source);
            }
            _ => {}
        }
        current = current.parent()?;
    }
}

/// Check if `node` is (or is an ancestor of) the `name` field of `decl_node`.
fn is_declaration_name(decl_node: &tree_sitter::Node, node: &tree_sitter::Node) -> bool {
    if let Some(name_node) = decl_node.child_by_field_name("name") {
        // The cursor node is at or inside the declaration's name
        node.start_byte() >= name_node.start_byte() && node.end_byte() <= name_node.end_byte()
    } else {
        false
    }
}

/// Check if the cursor is on the keyword/spacing before the declaration's name.
/// Extends hover coverage so that `const MOVE_SPEED` is hoverable from column 1.
fn is_on_declaration_keyword(decl_node: &tree_sitter::Node, point: tree_sitter::Point) -> bool {
    if let Some(name_node) = decl_node.child_by_field_name("name") {
        point.row == name_node.start_position().row
            && point.column < name_node.start_position().column
    } else {
        false
    }
}

/// Resolve hover for an identifier/name node: declarations, builtins, enum members.
fn resolve_hover_for_identifier(
    current: &tree_sitter::Node,
    root: &tree_sitter::Node,
    source: &str,
) -> Option<Hover> {
    let name = node_text(current, source);

    // 1. Try to resolve to a same-file declaration
    if let Some(hover) = resolve_identifier(root, name, source) {
        return Some(hover);
    }
    // 2. Try builtin type
    if let Some(doc) = super::builtins::lookup_type(name) {
        return Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: super::builtins::format_type_hover(doc),
            }),
            range: Some(node_range(current)),
        });
    }
    // 3. Try builtin function
    if let Some(doc) = super::builtins::lookup_function(name) {
        return Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: super::builtins::format_function_hover(doc),
            }),
            range: Some(node_range(current)),
        });
    }
    // 4. Check if this is a member access (foo.bar) — try builtin member
    //    or self.member — resolve to same-file declaration
    if let Some(hover) = try_member_hover(current, root, source) {
        return Some(hover);
    }
    // 5. Try builtin member as standalone identifier (GDScript allows
    //    inherited members without self prefix: velocity, move_and_slide, etc.)
    if let Some(doc) = super::builtins::lookup_member(name) {
        return Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: super::builtins::format_member_hover(doc),
            }),
            range: Some(node_range(current)),
        });
    }
    // 6. Check if this is an enum member (HOUSE inside enum { HOUSE, MART })
    if let Some(parent) = current.parent()
        && parent.kind() == "enumerator"
    {
        return hover_enum_member(&parent, source);
    }
    // 7. Check if this identifier refers to an enum member in the same file
    if let Some(hover) = resolve_enum_member(root, name, source) {
        return Some(hover);
    }
    // 8. Nothing found
    None
}

/// Try to resolve a member access pattern (foo.bar or self.bar).
fn try_member_hover(
    ident_node: &tree_sitter::Node,
    root: &tree_sitter::Node,
    source: &str,
) -> Option<Hover> {
    let parent = ident_node.parent()?;
    if parent.kind() != "attribute" {
        return None;
    }
    let name = node_text(ident_node, source);

    // Check if this identifier is the member (right side), not the object (left side).
    // In tree-sitter-gdscript, `attribute` has children: object, ".", member
    // The object is child(0), the member is the `attribute` field or last named child.
    let object_node = parent.child(0)?;
    if ident_node.id() == object_node.id() {
        // This is the object side (foo in foo.bar), not the member
        return None;
    }

    // Handle self.member — resolve to same-file declarations
    let object_text = node_text(&object_node, source);
    if object_text == "self"
        && let Some(hover) = resolve_identifier(root, name, source)
    {
        return Some(hover);
    }

    // Try builtin member lookup
    if let Some(doc) = super::builtins::lookup_member(name) {
        return Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: super::builtins::format_member_hover(doc),
            }),
            range: Some(node_range(ident_node)),
        });
    }

    None
}

/// Extract `##` doc comment lines preceding a declaration node.
/// Walks backward through named siblings, skipping annotation nodes.
fn extract_doc_comment(decl_node: &tree_sitter::Node, source: &str) -> Option<String> {
    let bytes = source.as_bytes();
    let mut lines = Vec::new();
    let mut current = decl_node.prev_named_sibling();

    while let Some(prev) = current {
        match prev.kind() {
            "comment" => {
                if let Ok(text) = prev.utf8_text(bytes) {
                    if let Some(stripped) = text.strip_prefix("##") {
                        lines.push(stripped.trim().to_string());
                    } else {
                        break; // Regular `#` comment breaks the chain
                    }
                }
            }
            "annotation" | "annotations" => {
                // Annotations can appear between doc comments and declarations
            }
            _ => break,
        }
        current = prev.prev_named_sibling();
    }

    if lines.is_empty() {
        None
    } else {
        lines.reverse();
        Some(lines.join("\n"))
    }
}

fn hover_function(node: &tree_sitter::Node, source: &str) -> Option<Hover> {
    let name = node_text(&node.child_by_field_name("name")?, source);

    let mut sig = String::from("func ");
    sig.push_str(name);

    if let Some(params) = node.child_by_field_name("parameters") {
        sig.push_str(node_text(&params, source));
    } else {
        sig.push_str("()");
    }

    if let Some(ret) = node.child_by_field_name("return_type") {
        sig.push_str(" -> ");
        sig.push_str(node_text(&ret, source));
    }

    let doc = extract_doc_comment(node, source);
    Some(make_hover(&sig, node, doc.as_deref()))
}

fn hover_variable(node: &tree_sitter::Node, source: &str) -> Hover {
    let text = node_text(node, source);
    let decl = text.lines().next().unwrap_or(text).trim_end();
    let doc = extract_doc_comment(node, source);
    make_hover(decl, node, doc.as_deref())
}

fn hover_const(node: &tree_sitter::Node, source: &str) -> Hover {
    let text = node_text(node, source);
    let decl = text.lines().next().unwrap_or(text).trim_end();
    let doc = extract_doc_comment(node, source);
    make_hover(decl, node, doc.as_deref())
}

fn hover_signal(node: &tree_sitter::Node, source: &str) -> Hover {
    let text = node_text(node, source);
    let decl = text.lines().next().unwrap_or(text).trim_end();
    let doc = extract_doc_comment(node, source);
    make_hover(decl, node, doc.as_deref())
}

fn hover_class_name(node: &tree_sitter::Node, source: &str) -> Hover {
    let text = node_text(node, source);
    let doc = extract_doc_comment(node, source);
    make_hover(text.trim(), node, doc.as_deref())
}

fn hover_class(node: &tree_sitter::Node, source: &str) -> Option<Hover> {
    let name = node_text(&node.child_by_field_name("name")?, source);
    let decl = format!("class {name}");
    let doc = extract_doc_comment(node, source);
    Some(make_hover(&decl, node, doc.as_deref()))
}

fn hover_enum(node: &tree_sitter::Node, source: &str) -> Option<Hover> {
    let name_node = node.child_by_field_name("name")?;
    let text = node_text(node, source);
    let doc = extract_doc_comment(node, source);
    Some(make_hover(text.trim(), &name_node, doc.as_deref()))
}

fn hover_enum_member(member_node: &tree_sitter::Node, source: &str) -> Option<Hover> {
    let name_node = member_node.child_by_field_name("left")?;
    let member_name = node_text(&name_node, source);

    // Walk up: enumerator → enumerator_list → enum_definition
    let enum_list = member_node.parent()?;
    let enum_def = enum_list.parent()?;
    if enum_def.kind() != "enum_definition" {
        return None;
    }

    let enum_name = enum_def
        .child_by_field_name("name")
        .map(|n| node_text(&n, source));
    let value = compute_enum_member_value(member_node, source);

    let code = match enum_name {
        Some(name) => format!("{name}.{member_name} = {value}"),
        None => format!("{member_name} = {value}"),
    };

    let doc = extract_doc_comment(member_node, source);
    Some(make_hover(&code, &name_node, doc.as_deref()))
}

/// Compute the integer value of an enum member, tracking explicit assignments.
fn compute_enum_member_value(target: &tree_sitter::Node, source: &str) -> String {
    let Some(list) = target.parent() else {
        return "0".to_string();
    };

    let mut cursor = list.walk();
    let mut next_value: i64 = 0;

    for child in list.children(&mut cursor) {
        if child.kind() != "enumerator" {
            continue;
        }
        if let Some(val_node) = child.child_by_field_name("right") {
            let val_text = node_text(&val_node, source).trim().to_string();
            if child.id() == target.id() {
                return val_text;
            }
            if let Ok(v) = val_text.parse::<i64>() {
                next_value = v + 1;
            } else {
                next_value += 1;
            }
        } else {
            if child.id() == target.id() {
                return next_value.to_string();
            }
            next_value += 1;
        }
    }

    "0".to_string()
}

/// Search enum definitions for a member matching `name`.
fn resolve_enum_member(root: &tree_sitter::Node, name: &str, source: &str) -> Option<Hover> {
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        if child.kind() == "enum_definition"
            && let Some(body) = child.child_by_field_name("body")
        {
            let mut body_cursor = body.walk();
            for member in body.children(&mut body_cursor) {
                if member.kind() == "enumerator"
                    && let Some(left) = member.child_by_field_name("left")
                    && node_text(&left, source) == name
                {
                    return hover_enum_member(&member, source);
                }
            }
        }
    }
    None
}

/// Try to resolve an identifier to a top-level declaration in the file.
fn resolve_identifier(root: &tree_sitter::Node, name: &str, source: &str) -> Option<Hover> {
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        match child.kind() {
            "function_definition" => {
                if matches_name(&child, name, source) {
                    return hover_function(&child, source);
                }
            }
            "variable_statement" => {
                if matches_name(&child, name, source) {
                    return Some(hover_variable(&child, source));
                }
            }
            "const_statement" => {
                if matches_name(&child, name, source) {
                    return Some(hover_const(&child, source));
                }
            }
            "signal_statement" => {
                if matches_name(&child, name, source) {
                    return Some(hover_signal(&child, source));
                }
            }
            "class_definition" => {
                if matches_name(&child, name, source) {
                    return hover_class(&child, source);
                }
            }
            "enum_definition" => {
                if matches_name(&child, name, source) {
                    return hover_enum(&child, source);
                }
            }
            _ => {}
        }
    }
    None
}

fn make_hover(code: &str, node: &tree_sitter::Node, doc: Option<&str>) -> Hover {
    let value = match doc {
        Some(d) => format!("```gdscript\n{code}\n```\n\n{d}"),
        None => format!("```gdscript\n{code}\n```"),
    };
    Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value,
        }),
        range: Some(node_range(node)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hover_value(source: &str, line: u32, character: u32) -> Option<String> {
        hover_at(source, Position::new(line, character)).map(|h| match h.contents {
            HoverContents::Markup(m) => m.value,
            _ => String::new(),
        })
    }

    #[test]
    fn hover_documented_function() {
        let source = "## Move the player.\nfunc move() -> void:\n\tpass\n";
        let val = hover_value(source, 1, 5).unwrap();
        assert!(val.contains("func move() -> void"));
        assert!(val.contains("Move the player."));
    }

    #[test]
    fn hover_undocumented_function() {
        let source = "func move() -> void:\n\tpass\n";
        let val = hover_value(source, 0, 5).unwrap();
        assert!(val.contains("func move() -> void"));
        assert!(!val.contains("\n\n"));
    }

    #[test]
    fn hover_documented_variable() {
        let source = "## The player's health.\nvar health: int = 100\n";
        let val = hover_value(source, 1, 4).unwrap();
        assert!(val.contains("var health: int = 100"));
        assert!(val.contains("The player's health."));
    }

    #[test]
    fn hover_multiline_doc() {
        let source = "## Line one.\n## Line two.\nfunc f():\n\tpass\n";
        let val = hover_value(source, 2, 5).unwrap();
        assert!(val.contains("Line one.\nLine two."));
    }

    #[test]
    fn hover_doc_with_annotation() {
        let source = "## Exported health.\n@export\nvar health: int = 100\n";
        let val = hover_value(source, 2, 4).unwrap();
        assert!(val.contains("Exported health."));
    }

    #[test]
    fn hover_resolved_identifier_with_doc() {
        let source = "## The speed value.\nvar speed: float = 5.0\n\nfunc f():\n\tprint(speed)\n";
        let val = hover_value(source, 4, 7).unwrap();
        assert!(val.contains("The speed value."));
    }

    // ── Fix 1: column sensitivity ────────────────────────────────────

    #[test]
    fn hover_const_on_keyword() {
        // Hovering on `const` keyword should show the const declaration
        let source = "const MOVE_SPEED = 100\n";
        let val = hover_value(source, 0, 0).unwrap(); // col 0 = 'c' in 'const'
        assert!(val.contains("const MOVE_SPEED = 100"));
    }

    #[test]
    fn hover_const_on_space_before_name() {
        // Hovering on space between keyword and name should work
        let source = "const MOVE_SPEED = 100\n";
        let val = hover_value(source, 0, 5).unwrap(); // col 5 = space before MOVE_SPEED
        assert!(val.contains("const MOVE_SPEED = 100"));
    }

    #[test]
    fn hover_var_on_keyword() {
        let source = "var health: int = 100\n";
        let val = hover_value(source, 0, 0).unwrap();
        assert!(val.contains("var health: int = 100"));
    }

    #[test]
    fn hover_func_on_keyword() {
        let source = "func move():\n\tpass\n";
        let val = hover_value(source, 0, 0).unwrap(); // col 0 = 'f' in 'func'
        assert!(val.contains("func move()"));
    }

    #[test]
    fn hover_signal_on_keyword() {
        let source = "signal health_changed(amount: int)\n";
        let val = hover_value(source, 0, 0).unwrap();
        assert!(val.contains("signal health_changed"));
    }

    #[test]
    fn hover_const_value_no_trigger() {
        // Hovering on the value (after =) should NOT show the const
        let source = "const MOVE_SPEED = 100\n";
        assert!(hover_value(source, 0, 20).is_none()); // col 20 = inside '100'
    }

    // ── Fix 2: enum members ─────────────────────────────────────────

    #[test]
    fn hover_enum_member_inline() {
        let source = "enum Color { RED, GREEN, BLUE }\n";
        let val = hover_value(source, 0, 13).unwrap(); // col 13 = 'R' in RED
        assert!(val.contains("Color.RED = 0"));
    }

    #[test]
    fn hover_enum_member_with_value() {
        let source = "enum Flags { A = 1, B = 5, C }\n";
        let val = hover_value(source, 0, 20).unwrap(); // col 20 = 'B'
        assert!(val.contains("Flags.B = 5"));
    }

    #[test]
    fn hover_enum_member_implicit_after_explicit() {
        let source = "enum Flags { A = 10, B, C }\n";
        let val = hover_value(source, 0, 21).unwrap(); // col 21 = 'B'
        assert!(val.contains("Flags.B = 11"));
    }

    #[test]
    fn hover_enum_member_multiline() {
        let source = "enum Dir {\n\tUP,\n\tDOWN,\n\tLEFT,\n\tRIGHT,\n}\n";
        let val = hover_value(source, 3, 1).unwrap(); // line 3, col 1 = LEFT
        assert!(val.contains("Dir.LEFT = 2"));
    }

    #[test]
    fn hover_enum_member_anonymous() {
        let source = "enum { X, Y, Z }\n";
        let val = hover_value(source, 0, 7).unwrap(); // col 7 = 'X'
        assert!(val.contains("X = 0"));
        assert!(!val.contains('.')); // no enum name prefix
    }

    #[test]
    fn hover_enum_member_reference() {
        // Using enum member name elsewhere in the file resolves to the declaration
        let source = "enum Color { RED, GREEN, BLUE }\nfunc f():\n\tvar c = RED\n";
        let val = hover_value(source, 2, 9).unwrap(); // col 9 = 'R' in RED usage
        assert!(val.contains("Color.RED = 0"));
    }
}
