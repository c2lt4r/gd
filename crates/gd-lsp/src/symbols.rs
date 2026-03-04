use tower_lsp::lsp_types::{DocumentSymbol, DocumentSymbolResponse, SymbolKind};

use gd_core::gd_ast::{self, GdClass, GdDecl};

use super::util::{node_range, node_text};

/// Extract document symbols (outline) from GDScript source.
#[allow(deprecated)] // DocumentSymbol::deprecated field is deprecated in the lsp-types API
pub fn document_symbols(source: &str) -> Option<DocumentSymbolResponse> {
    let tree = gd_core::parser::parse(source).ok()?;
    let file = gd_ast::convert(&tree, source);
    let mut symbols = Vec::new();

    // class_name statement (not a GdDecl, stored separately on GdFile)
    if let Some(cn) = file.class_name
        && let Some(cn_node) = file.class_name_node
    {
        let parent = cn_node.parent().unwrap_or(cn_node);
        symbols.push(DocumentSymbol {
            name: cn.to_string(),
            detail: Some("class_name".to_string()),
            kind: SymbolKind::CLASS,
            tags: None,
            deprecated: None,
            range: node_range(&parent),
            selection_range: node_range(&cn_node),
            children: None,
        });
    }

    collect_symbols(&file.declarations, source, &mut symbols);

    if symbols.is_empty() {
        return None;
    }

    Some(DocumentSymbolResponse::Nested(symbols))
}

#[allow(deprecated)]
fn collect_symbols(decls: &[GdDecl], source: &str, symbols: &mut Vec<DocumentSymbol>) {
    for decl in decls {
        match decl {
            GdDecl::Func(f) => {
                let node = f.node;
                let detail = build_function_detail(&node, source);
                if let Some(name_node) = f.name_node {
                    symbols.push(DocumentSymbol {
                        name: f.name.to_string(),
                        detail: Some(detail),
                        kind: SymbolKind::FUNCTION,
                        tags: None,
                        deprecated: None,
                        range: node_range(&node),
                        selection_range: node_range(&name_node),
                        children: None,
                    });
                }
            }
            GdDecl::Var(v) => {
                let node = v.node;
                if let Some(name_node) = v.name_node {
                    let kind = if is_onready(&node, source) {
                        if v.is_const {
                            SymbolKind::CONSTANT
                        } else {
                            SymbolKind::FIELD
                        }
                    } else if v.is_const {
                        SymbolKind::CONSTANT
                    } else {
                        SymbolKind::VARIABLE
                    };
                    let detail = build_declaration_detail(&node, source);
                    symbols.push(DocumentSymbol {
                        name: v.name.to_string(),
                        detail: Some(detail),
                        kind,
                        tags: None,
                        deprecated: None,
                        range: node_range(&node),
                        selection_range: node_range(&name_node),
                        children: None,
                    });
                }
            }
            GdDecl::Signal(s) => {
                let node = s.node;
                if let Some(name_node) = s.name_node {
                    let detail = build_declaration_detail(&node, source);
                    symbols.push(DocumentSymbol {
                        name: s.name.to_string(),
                        detail: Some(detail),
                        kind: SymbolKind::EVENT,
                        tags: None,
                        deprecated: None,
                        range: node_range(&node),
                        selection_range: node_range(&name_node),
                        children: None,
                    });
                }
            }
            GdDecl::Enum(e) => {
                let node = e.node;
                if let Some(name_node) = e.name_node {
                    let detail = build_enum_detail(&node, source);
                    symbols.push(DocumentSymbol {
                        name: e.name.to_string(),
                        detail: Some(detail),
                        kind: SymbolKind::ENUM,
                        tags: None,
                        deprecated: None,
                        range: node_range(&node),
                        selection_range: node_range(&name_node),
                        children: None,
                    });
                }
            }
            GdDecl::Class(c) => {
                if let Some(name_node) = c.name_node {
                    let mut children = Vec::new();
                    collect_class_symbols(c, source, &mut children);
                    symbols.push(DocumentSymbol {
                        name: c.name.to_string(),
                        detail: Some("class".to_string()),
                        kind: SymbolKind::CLASS,
                        tags: None,
                        deprecated: None,
                        range: node_range(&c.node),
                        selection_range: node_range(&name_node),
                        children: if children.is_empty() {
                            None
                        } else {
                            Some(children)
                        },
                    });
                }
            }
            GdDecl::Stmt(_) => {}
        }
    }
}

#[allow(deprecated)]
fn collect_class_symbols(class: &GdClass, source: &str, symbols: &mut Vec<DocumentSymbol>) {
    collect_symbols(&class.declarations, source, symbols);
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
