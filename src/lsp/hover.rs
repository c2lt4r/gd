use tower_lsp::lsp_types::*;

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
            "function_definition" => return hover_function(&current, source),
            "variable_statement" => return hover_variable(&current, source),
            "const_statement" => return hover_const(&current, source),
            "signal_statement" => return hover_signal(&current, source),
            "class_name_statement" => return hover_class_name(&current, source),
            "class_definition" => return hover_class(&current, source),
            "enum_definition" => return hover_enum(&current, source),
            "identifier" | "name" => {
                // For identifiers, try to resolve them to a declaration in the same file
                let name = node_text(&current, source);
                if let Some(hover) = resolve_identifier(&root, name, source) {
                    return Some(hover);
                }
            }
            _ => {}
        }
        current = current.parent()?;
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

    Some(make_hover(&sig, node))
}

fn hover_variable(node: &tree_sitter::Node, source: &str) -> Option<Hover> {
    let text = node_text(node, source);
    let decl = text.lines().next().unwrap_or(text).trim_end();
    Some(make_hover(decl, node))
}

fn hover_const(node: &tree_sitter::Node, source: &str) -> Option<Hover> {
    let text = node_text(node, source);
    let decl = text.lines().next().unwrap_or(text).trim_end();
    Some(make_hover(decl, node))
}

fn hover_signal(node: &tree_sitter::Node, source: &str) -> Option<Hover> {
    let text = node_text(node, source);
    let decl = text.lines().next().unwrap_or(text).trim_end();
    Some(make_hover(decl, node))
}

fn hover_class_name(node: &tree_sitter::Node, source: &str) -> Option<Hover> {
    let text = node_text(node, source);
    Some(make_hover(text.trim(), node))
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
fn resolve_identifier(
    root: &tree_sitter::Node,
    name: &str,
    source: &str,
) -> Option<Hover> {
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
                    return hover_variable(&child, source);
                }
            }
            "const_statement" => {
                if matches_name(&child, name, source) {
                    return hover_const(&child, source);
                }
            }
            "signal_statement" => {
                if matches_name(&child, name, source) {
                    return hover_signal(&child, source);
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

fn matches_name(node: &tree_sitter::Node, name: &str, source: &str) -> bool {
    node.child_by_field_name("name")
        .is_some_and(|n| node_text(&n, source) == name)
}

fn node_text<'a>(node: &tree_sitter::Node, source: &'a str) -> &'a str {
    node.utf8_text(source.as_bytes()).unwrap_or("unknown")
}

fn node_range(node: &tree_sitter::Node) -> Range {
    Range::new(
        Position::new(
            node.start_position().row as u32,
            node.start_position().column as u32,
        ),
        Position::new(
            node.end_position().row as u32,
            node.end_position().column as u32,
        ),
    )
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
