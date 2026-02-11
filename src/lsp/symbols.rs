use tower_lsp::lsp_types::*;

/// Extract document symbols (outline) from GDScript source.
#[allow(deprecated)] // DocumentSymbol::deprecated field is deprecated in the lsp-types API
pub fn document_symbols(source: &str) -> Option<DocumentSymbolResponse> {
    let tree = crate::core::parser::parse(source).ok()?;
    let root = tree.root_node();
    let mut symbols = Vec::new();

    collect_symbols(root, source, &mut symbols);

    if symbols.is_empty() {
        return None;
    }

    Some(DocumentSymbolResponse::Nested(symbols))
}

#[allow(deprecated)]
fn collect_symbols(node: tree_sitter::Node, source: &str, symbols: &mut Vec<DocumentSymbol>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "class_name_statement" => {
                if let Some(name_node) = child.child_by_field_name("name") {
                    let name = node_text(&name_node, source);
                    symbols.push(DocumentSymbol {
                        name,
                        detail: Some("class_name".to_string()),
                        kind: SymbolKind::CLASS,
                        tags: None,
                        deprecated: None,
                        range: node_range(&child),
                        selection_range: node_range(&name_node),
                        children: None,
                    });
                }
            }
            "class_definition" => {
                if let Some(name_node) = child.child_by_field_name("name") {
                    let name = node_text(&name_node, source);
                    let mut children = Vec::new();
                    if let Some(body) = child.child_by_field_name("body") {
                        collect_symbols(body, source, &mut children);
                    }
                    symbols.push(DocumentSymbol {
                        name,
                        detail: Some("class".to_string()),
                        kind: SymbolKind::CLASS,
                        tags: None,
                        deprecated: None,
                        range: node_range(&child),
                        selection_range: node_range(&name_node),
                        children: if children.is_empty() {
                            None
                        } else {
                            Some(children)
                        },
                    });
                }
            }
            "function_definition" => {
                if let Some(name_node) = child.child_by_field_name("name") {
                    let name = node_text(&name_node, source);
                    let detail = build_function_detail(&child, source);
                    symbols.push(DocumentSymbol {
                        name,
                        detail: Some(detail),
                        kind: SymbolKind::FUNCTION,
                        tags: None,
                        deprecated: None,
                        range: node_range(&child),
                        selection_range: node_range(&name_node),
                        children: None,
                    });
                }
            }
            "variable_statement" => {
                if let Some(name_node) = child.child_by_field_name("name") {
                    let name = node_text(&name_node, source);
                    let kind = if is_onready(&child, source) {
                        SymbolKind::FIELD
                    } else {
                        SymbolKind::VARIABLE
                    };
                    symbols.push(DocumentSymbol {
                        name,
                        detail: None,
                        kind,
                        tags: None,
                        deprecated: None,
                        range: node_range(&child),
                        selection_range: node_range(&name_node),
                        children: None,
                    });
                }
            }
            "const_statement" => {
                if let Some(name_node) = child.child_by_field_name("name") {
                    let name = node_text(&name_node, source);
                    symbols.push(DocumentSymbol {
                        name,
                        detail: Some("const".to_string()),
                        kind: SymbolKind::CONSTANT,
                        tags: None,
                        deprecated: None,
                        range: node_range(&child),
                        selection_range: node_range(&name_node),
                        children: None,
                    });
                }
            }
            "signal_statement" => {
                if let Some(name_node) = child.child_by_field_name("name") {
                    let name = node_text(&name_node, source);
                    symbols.push(DocumentSymbol {
                        name,
                        detail: Some("signal".to_string()),
                        kind: SymbolKind::EVENT,
                        tags: None,
                        deprecated: None,
                        range: node_range(&child),
                        selection_range: node_range(&name_node),
                        children: None,
                    });
                }
            }
            "enum_definition" => {
                if let Some(name_node) = child.child_by_field_name("name") {
                    let name = node_text(&name_node, source);
                    symbols.push(DocumentSymbol {
                        name,
                        detail: Some("enum".to_string()),
                        kind: SymbolKind::ENUM,
                        tags: None,
                        deprecated: None,
                        range: node_range(&child),
                        selection_range: node_range(&name_node),
                        children: None,
                    });
                }
            }
            _ => {}
        }
    }
}

fn node_text(node: &tree_sitter::Node, source: &str) -> String {
    node.utf8_text(source.as_bytes())
        .unwrap_or("unknown")
        .to_string()
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

fn build_function_detail(node: &tree_sitter::Node, source: &str) -> String {
    let mut detail = "func(".to_string();

    if let Some(params) = node.child_by_field_name("parameters") {
        let params_text = node_text(&params, source);
        // Strip outer parens if present
        let inner = params_text
            .strip_prefix('(')
            .and_then(|s| s.strip_suffix(')'))
            .unwrap_or(&params_text);
        detail.push_str(inner);
    }

    detail.push(')');

    if let Some(return_type) = node.child_by_field_name("return_type") {
        detail.push_str(" -> ");
        detail.push_str(&node_text(&return_type, source));
    }

    detail
}

fn is_onready(node: &tree_sitter::Node, source: &str) -> bool {
    let text = &source[node.byte_range()];
    text.starts_with("@onready")
}
