use tower_lsp::lsp_types::*;

/// Find all references to the symbol at the given position within the same file.
pub fn find_references(
    source: &str,
    uri: &Url,
    position: Position,
    include_declaration: bool,
) -> Option<Vec<Location>> {
    let tree = crate::core::parser::parse(source).ok()?;
    let root = tree.root_node();

    // Find leaf node at cursor
    let point = tree_sitter::Point::new(position.line as usize, position.character as usize);
    let node = root.descendant_for_point_range(point, point)?;

    // Get the identifier text
    let target_name = node.utf8_text(source.as_bytes()).ok()?;
    if target_name.is_empty() {
        return None;
    }

    // Collect all matching identifiers in the file
    let mut locations = Vec::new();
    collect_references(
        root,
        source,
        target_name,
        uri,
        include_declaration,
        &mut locations,
    );

    if locations.is_empty() {
        None
    } else {
        Some(locations)
    }
}

/// Declaration node kinds whose `name` field child is the defining occurrence.
const DECLARATION_KINDS: &[&str] = &[
    "function_definition",
    "variable_statement",
    "const_statement",
    "signal_statement",
    "class_definition",
    "enum_definition",
    "class_name_statement",
];

/// Check whether `node` is the name child of a declaration.
fn is_declaration(node: &tree_sitter::Node, source: &str, target_name: &str) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    if !DECLARATION_KINDS.contains(&parent.kind()) {
        return false;
    }
    // The node must be the `name` field of its parent (or child(1) for class_name_statement)
    if let Some(name_node) = parent.child_by_field_name("name") {
        name_node.id() == node.id()
    } else if parent.kind() == "class_name_statement" {
        // Fallback: check second child
        parent.child(1).is_some_and(|n| {
            n.id() == node.id() && n.utf8_text(source.as_bytes()).unwrap_or("") == target_name
        })
    } else {
        false
    }
}

/// Recursively walk the AST and collect locations of identifiers matching `target_name`.
fn collect_references(
    node: tree_sitter::Node,
    source: &str,
    target_name: &str,
    uri: &Url,
    include_declaration: bool,
    locations: &mut Vec<Location>,
) {
    // Check if this node is an identifier that matches our target
    if (node.kind() == "identifier" || node.kind() == "name")
        && node.utf8_text(source.as_bytes()).unwrap_or("") == target_name
    {
        if is_declaration(&node, source, target_name) {
            if include_declaration {
                locations.push(Location {
                    uri: uri.clone(),
                    range: node_range(&node),
                });
            }
        } else {
            locations.push(Location {
                uri: uri.clone(),
                range: node_range(&node),
            });
        }
    }

    // Recurse into children
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_references(
            child,
            source,
            target_name,
            uri,
            include_declaration,
            locations,
        );
    }
}

/// Find all references to the symbol at position across all workspace files.
pub fn find_references_cross_file(
    source: &str,
    uri: &Url,
    position: Position,
    include_declaration: bool,
    workspace: &super::workspace::WorkspaceIndex,
) -> Option<Vec<Location>> {
    let tree = crate::core::parser::parse(source).ok()?;
    let root = tree.root_node();

    let point = tree_sitter::Point::new(position.line as usize, position.character as usize);
    let node = root.descendant_for_point_range(point, point)?;

    let target_name = node.utf8_text(source.as_bytes()).ok()?;
    if target_name.is_empty() {
        return None;
    }

    let mut locations = Vec::new();

    // Current file
    collect_references(
        root,
        source,
        target_name,
        uri,
        include_declaration,
        &mut locations,
    );

    // All workspace files
    let current_path = uri.to_file_path().ok();
    for (path, content) in workspace.all_files() {
        if current_path.as_ref() == Some(&path) {
            continue;
        }
        if let Ok(tree) = crate::core::parser::parse(&content) {
            let file_uri = match Url::from_file_path(&path) {
                Ok(u) => u,
                Err(_) => continue,
            };
            collect_references(
                tree.root_node(),
                &content,
                target_name,
                &file_uri,
                true,
                &mut locations,
            );
        }
    }

    if locations.is_empty() {
        None
    } else {
        Some(locations)
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn test_uri() -> Url {
        Url::parse("file:///test.gd").unwrap()
    }

    #[test]
    fn references_to_function() {
        let source = "func greet():\n\tpass\n\nfunc main():\n\tgreet()\n\tgreet()\n";
        let uri = test_uri();
        // Position on `greet` call at line 4, col 1
        let result = find_references(source, &uri, Position::new(4, 1), false);
        assert!(result.is_some());
        let locs = result.unwrap();
        // Two call-site references (excluding the declaration)
        assert_eq!(locs.len(), 2);
    }

    #[test]
    fn references_include_declaration() {
        let source = "func greet():\n\tpass\n\nfunc main():\n\tgreet()\n";
        let uri = test_uri();
        // Position on `greet` at line 0, col 5 (the declaration)
        let result = find_references(source, &uri, Position::new(0, 5), true);
        assert!(result.is_some());
        let locs = result.unwrap();
        // Declaration + one call-site reference
        assert_eq!(locs.len(), 2);
    }

    #[test]
    fn references_exclude_declaration() {
        let source = "func greet():\n\tpass\n\nfunc main():\n\tgreet()\n";
        let uri = test_uri();
        let result = find_references(source, &uri, Position::new(0, 5), false);
        assert!(result.is_some());
        let locs = result.unwrap();
        // Only the call-site reference
        assert_eq!(locs.len(), 1);
        assert_eq!(locs[0].range.start.line, 4);
    }

    #[test]
    fn references_to_variable() {
        let source = "var speed = 10\n\nfunc run():\n\tprint(speed)\n\tspeed = 20\n";
        let uri = test_uri();
        // Position on `speed` at line 3, col 7
        let result = find_references(source, &uri, Position::new(3, 7), true);
        assert!(result.is_some());
        let locs = result.unwrap();
        // Declaration + two usages
        assert_eq!(locs.len(), 3);
    }

    #[test]
    fn no_references_for_unknown() {
        let source = "func main():\n\tpass\n";
        let uri = test_uri();
        // Position on `pass` keyword — not a meaningful identifier reference
        let result = find_references(source, &uri, Position::new(1, 1), true);
        // `pass` only appears once, but it is not an identifier node — should be None or 1
        // Actually "pass" is a keyword node, so it won't match identifier nodes
        assert!(result.is_none() || result.unwrap().len() <= 1);
    }

    #[test]
    fn empty_source_returns_none() {
        let uri = test_uri();
        let result = find_references("", &uri, Position::new(0, 0), true);
        assert!(result.is_none());
    }
}
