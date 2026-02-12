use tower_lsp::lsp_types::*;

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
                // Only show hover for the function's name identifier, not for
                // arbitrary identifiers inside the function body.
                if is_declaration_name(&current, &node) {
                    return hover_function(&current, source);
                }
                return None;
            }
            "variable_statement" => {
                if is_declaration_name(&current, &node) {
                    return Some(hover_variable(&current, source));
                }
                return None;
            }
            "const_statement" => {
                if is_declaration_name(&current, &node) {
                    return Some(hover_const(&current, source));
                }
                return None;
            }
            "signal_statement" => {
                if is_declaration_name(&current, &node) {
                    return Some(hover_signal(&current, source));
                }
                return None;
            }
            "class_name_statement" => return Some(hover_class_name(&current, source)),
            "class_definition" => {
                if is_declaration_name(&current, &node) {
                    return hover_class(&current, source);
                }
                return None;
            }
            "enum_definition" => {
                if is_declaration_name(&current, &node) {
                    return hover_enum(&current, source);
                }
                return None;
            }
            "identifier" | "name" => {
                let name = node_text(&current, source);

                // 1. Try to resolve to a same-file declaration
                if let Some(hover) = resolve_identifier(&root, name, source) {
                    return Some(hover);
                }
                // 2. Try builtin type
                if let Some(doc) = super::builtins::lookup_type(name) {
                    return Some(Hover {
                        contents: HoverContents::Markup(MarkupContent {
                            kind: MarkupKind::Markdown,
                            value: super::builtins::format_type_hover(doc),
                        }),
                        range: Some(node_range(&current)),
                    });
                }
                // 3. Try builtin function
                if let Some(doc) = super::builtins::lookup_function(name) {
                    return Some(Hover {
                        contents: HoverContents::Markup(MarkupContent {
                            kind: MarkupKind::Markdown,
                            value: super::builtins::format_function_hover(doc),
                        }),
                        range: Some(node_range(&current)),
                    });
                }
                // 4. Check if this is a member access (foo.bar) — try builtin member
                if let Some(hover) = try_member_hover(&current, &root, source) {
                    return Some(hover);
                }
                // 5. Nothing found — return None instead of walking up to
                //    function_definition and showing the enclosing function.
                return None;
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

    Some(make_hover(&sig, node))
}

fn hover_variable(node: &tree_sitter::Node, source: &str) -> Hover {
    let text = node_text(node, source);
    let decl = text.lines().next().unwrap_or(text).trim_end();
    make_hover(decl, node)
}

fn hover_const(node: &tree_sitter::Node, source: &str) -> Hover {
    let text = node_text(node, source);
    let decl = text.lines().next().unwrap_or(text).trim_end();
    make_hover(decl, node)
}

fn hover_signal(node: &tree_sitter::Node, source: &str) -> Hover {
    let text = node_text(node, source);
    let decl = text.lines().next().unwrap_or(text).trim_end();
    make_hover(decl, node)
}

fn hover_class_name(node: &tree_sitter::Node, source: &str) -> Hover {
    let text = node_text(node, source);
    make_hover(text.trim(), node)
}

fn hover_class(node: &tree_sitter::Node, source: &str) -> Option<Hover> {
    let name = node_text(&node.child_by_field_name("name")?, source);
    let decl = format!("class {name}");
    Some(make_hover(&decl, node))
}

fn hover_enum(node: &tree_sitter::Node, source: &str) -> Option<Hover> {
    let name_node = node.child_by_field_name("name")?;
    let text = node_text(node, source);
    Some(make_hover(text.trim(), &name_node))
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

fn make_hover(code: &str, node: &tree_sitter::Node) -> Hover {
    Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: format!("```gdscript\n{code}\n```"),
        }),
        range: Some(node_range(node)),
    }
}
