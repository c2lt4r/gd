use std::collections::HashMap;

use tower_lsp::lsp_types::*;

/// Prepare rename: verify the symbol at position is renameable and return its range.
pub fn prepare_rename(source: &str, position: Position) -> Option<PrepareRenameResponse> {
    let tree = crate::core::parser::parse(source).ok()?;
    let root = tree.root_node();
    let point = tree_sitter::Point::new(position.line as usize, position.character as usize);
    let node = root.descendant_for_point_range(point, point)?;

    // Only allow renaming identifiers
    let text = node.utf8_text(source.as_bytes()).ok()?;
    if text.is_empty() {
        return None;
    }

    let range = Range::new(
        Position::new(
            node.start_position().row as u32,
            node.start_position().column as u32,
        ),
        Position::new(
            node.end_position().row as u32,
            node.end_position().column as u32,
        ),
    );

    Some(PrepareRenameResponse::Range(range))
}

/// Rename all occurrences of the symbol at position to new_name.
pub fn rename_symbol(
    source: &str,
    uri: &Url,
    position: Position,
    new_name: &str,
) -> Option<WorkspaceEdit> {
    // Reuse references logic to find all occurrences (including declaration)
    let locations = super::references::find_references(source, uri, position, true)?;

    let edits: Vec<TextEdit> = locations
        .into_iter()
        .map(|loc| TextEdit {
            range: loc.range,
            new_text: new_name.to_string(),
        })
        .collect();

    if edits.is_empty() {
        return None;
    }

    let changes = [(uri.clone(), edits)].into_iter().collect();
    Some(WorkspaceEdit {
        changes: Some(changes),
        ..Default::default()
    })
}

/// Rename a symbol across all workspace files.
pub fn rename_cross_file(
    source: &str,
    uri: &Url,
    position: Position,
    new_name: &str,
    workspace: &super::workspace::WorkspaceIndex,
) -> Option<WorkspaceEdit> {
    let locations = super::references::find_references_cross_file(
        source, uri, position, true, workspace,
    )?;

    let mut changes: HashMap<Url, Vec<TextEdit>> = HashMap::new();
    for loc in locations {
        changes.entry(loc.uri).or_default().push(TextEdit {
            range: loc.range,
            new_text: new_name.to_string(),
        });
    }

    if changes.is_empty() {
        return None;
    }

    Some(WorkspaceEdit {
        changes: Some(changes),
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_uri() -> Url {
        Url::parse("file:///test.gd").unwrap()
    }

    #[test]
    fn prepare_rename_on_identifier() {
        let source = "var speed = 10\n";
        let result = prepare_rename(source, Position::new(0, 4));
        assert!(result.is_some());
        if let Some(PrepareRenameResponse::Range(range)) = result {
            assert_eq!(range.start, Position::new(0, 4));
            assert_eq!(range.end, Position::new(0, 9));
        } else {
            panic!("expected Range variant");
        }
    }

    #[test]
    fn rename_variable() {
        let source = "var speed = 10\n\nfunc run():\n\tprint(speed)\n\tspeed = 20\n";
        let uri = test_uri();
        let result = rename_symbol(source, &uri, Position::new(0, 4), "velocity");
        assert!(result.is_some());
        let edit = result.unwrap();
        let changes = edit.changes.unwrap();
        let edits = changes.get(&uri).unwrap();
        // Declaration + two usages
        assert_eq!(edits.len(), 3);
        for e in edits {
            assert_eq!(e.new_text, "velocity");
        }
    }

    #[test]
    fn rename_function() {
        let source = "func greet():\n\tpass\n\nfunc main():\n\tgreet()\n\tgreet()\n";
        let uri = test_uri();
        let result = rename_symbol(source, &uri, Position::new(0, 5), "hello");
        assert!(result.is_some());
        let edit = result.unwrap();
        let changes = edit.changes.unwrap();
        let edits = changes.get(&uri).unwrap();
        // Declaration + two call sites
        assert_eq!(edits.len(), 3);
    }

    #[test]
    fn rename_empty_source() {
        let uri = test_uri();
        let result = rename_symbol("", &uri, Position::new(0, 0), "foo");
        assert!(result.is_none());
    }
}
