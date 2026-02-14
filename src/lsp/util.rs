use tower_lsp::lsp_types::{Position, Range};

/// Convert a tree-sitter node's position range to an LSP `Range`.
pub fn node_range(node: &tree_sitter::Node) -> Range {
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

/// Extract the text of a tree-sitter node from source.
pub fn node_text<'a>(node: &tree_sitter::Node, source: &'a str) -> &'a str {
    node.utf8_text(source.as_bytes()).unwrap_or("unknown")
}

/// Check if a declaration node's `name` field matches the given name.
pub fn matches_name(node: &tree_sitter::Node, name: &str, source: &str) -> bool {
    node.child_by_field_name("name")
        .is_some_and(|n| node_text(&n, source) == name)
}

/// Node kinds that represent function definitions (including constructors).
pub const FUNCTION_KINDS: &[&str] = &["function_definition", "constructor_definition"];
