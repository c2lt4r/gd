use tower_lsp::lsp_types::{DocumentSymbol, DocumentSymbolResponse, SymbolKind};

use super::util::{node_range, node_text};

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

#[allow(deprecated, clippy::too_many_lines)]
fn collect_symbols(node: tree_sitter::Node, source: &str, symbols: &mut Vec<DocumentSymbol>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "class_name_statement" => {
                if let Some(name_node) = child.child_by_field_name("name") {
                    let name = node_text(&name_node, source).to_string();
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
                    let name = node_text(&name_node, source).to_string();
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
                    let name = node_text(&name_node, source).to_string();
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
                    let name = node_text(&name_node, source).to_string();
                    let kind = if is_onready(&child, source) {
                        SymbolKind::FIELD
                    } else {
                        SymbolKind::VARIABLE
                    };
                    let detail = build_declaration_detail(&child, source);
                    symbols.push(DocumentSymbol {
                        name,
                        detail: Some(detail),
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
                    let name = node_text(&name_node, source).to_string();
                    let detail = build_declaration_detail(&child, source);
                    symbols.push(DocumentSymbol {
                        name,
                        detail: Some(detail),
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
                    let name = node_text(&name_node, source).to_string();
                    let detail = build_declaration_detail(&child, source);
                    symbols.push(DocumentSymbol {
                        name,
                        detail: Some(detail),
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
                    let name = node_text(&name_node, source).to_string();
                    let detail = build_enum_detail(&child, source);
                    symbols.push(DocumentSymbol {
                        name,
                        detail: Some(detail),
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

fn build_function_detail(node: &tree_sitter::Node, source: &str) -> String {
    let mut detail = "func(".to_string();

    if let Some(params) = node.child_by_field_name("parameters") {
        let params_text = node_text(&params, source);
        // Strip outer parens if present
        let inner = params_text
            .strip_prefix('(')
            .and_then(|s| s.strip_suffix(')'))
            .unwrap_or(params_text);
        detail.push_str(inner);
    }

    detail.push(')');

    if let Some(return_type) = node.child_by_field_name("return_type") {
        detail.push_str(" -> ");
        detail.push_str(node_text(&return_type, source));
    }

    detail
}

fn is_onready(node: &tree_sitter::Node, source: &str) -> bool {
    let text = &source[node.byte_range()];
    text.starts_with("@onready")
}

/// Build detail string from the first line of a declaration node.
/// Used for var, const, and signal statements.
fn build_declaration_detail(node: &tree_sitter::Node, source: &str) -> String {
    let text = node_text(node, source);
    text.lines().next().unwrap_or(text).trim().to_string()
}

/// Build detail string for an enum showing its members.
fn build_enum_detail(node: &tree_sitter::Node, source: &str) -> String {
    let text = node_text(node, source);
    // For single-line enums, show the whole thing
    let first_line = text.lines().next().unwrap_or(text).trim();
    if text.lines().count() <= 1 {
        return first_line.to_string();
    }
    // For multi-line enums, collect member names
    if let Some(body) = node.child_by_field_name("body") {
        let mut members = Vec::new();
        let mut cursor = body.walk();
        for member in body.children(&mut cursor) {
            if member.kind() == "enumerator"
                && let Some(left) = member.child_by_field_name("left")
            {
                members.push(node_text(&left, source).to_string());
            }
        }
        if !members.is_empty() {
            return format!("{{ {} }}", members.join(", "));
        }
    }
    first_line.to_string()
}
