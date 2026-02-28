use std::collections::HashSet;

use tower_lsp::lsp_types::{GotoDefinitionResponse, Location, Position, Range, Url};

use crate::core::gd_ast;

use super::util::{FUNCTION_KINDS, node_range, node_text};
use super::workspace::WorkspaceIndex;

/// Find implementations (subtypes or method overrides) for the symbol at cursor.
pub fn find_implementations(
    source: &str,
    uri: &Url,
    position: Position,
    workspace: &WorkspaceIndex,
) -> Option<GotoDefinitionResponse> {
    let tree = crate::core::parser::parse(source).ok()?;
    let root = tree.root_node();

    let point = tree_sitter::Point::new(position.line as usize, position.character as usize);
    let node = root.descendant_for_point_range(point, point)?;

    // Walk up to determine what kind of symbol we're on
    let mut current = node;
    loop {
        match current.kind() {
            // Cursor on a class_name declaration → find all subtypes
            "class_name_statement" => {
                let name_node = current.child_by_field_name("name")?;
                let class_name = node_text(&name_node, source);
                return find_subtype_locations(class_name, workspace);
            }

            // Cursor on an extends statement → find subtypes of the extended class
            "extends_statement" => {
                let name_node = current
                    .child_by_field_name("path")
                    .or_else(|| current.child(1))?;
                let class_name = node_text(&name_node, source);
                return find_subtype_locations(class_name, workspace);
            }

            // Cursor on a function definition → find overrides in subclasses
            kind if FUNCTION_KINDS.contains(&kind) => {
                let name_node = current.child_by_field_name("name")?;
                let method_name = node_text(&name_node, source);
                return find_method_overrides(method_name, source, uri, workspace);
            }

            _ => {}
        }

        // If we haven't matched yet, try the parent node
        let Some(parent) = current.parent() else {
            break;
        };
        // Stop walking up once we've passed the immediate context
        if current.id() != node.id() {
            break;
        }
        current = parent;
    }

    // Fallback: if the identifier matches a known class_name, find its subtypes
    let ident = node.utf8_text(source.as_bytes()).ok()?;
    if workspace.lookup_class_name(ident).is_some() {
        return find_subtype_locations(ident, workspace);
    }

    None
}

/// Find all files that are (transitively) subtypes of `class_name` and return
/// locations pointing to each subtype's class definition.
fn find_subtype_locations(
    class_name: &str,
    workspace: &WorkspaceIndex,
) -> Option<GotoDefinitionResponse> {
    let subtypes = collect_all_subtypes(class_name, workspace);
    if subtypes.is_empty() {
        return None;
    }

    let mut locations = Vec::new();
    for subtype in &subtypes {
        let Some(path) = workspace.lookup_class_name(subtype) else {
            continue;
        };
        let Ok(file_uri) = Url::from_file_path(&path) else {
            continue;
        };

        // Find the class_name_statement line for a precise location
        let range = workspace
            .get_content(&path)
            .and_then(|content| find_class_name_range(&content, subtype))
            .unwrap_or_else(|| Range::new(Position::new(0, 0), Position::new(0, 0)));

        locations.push(Location {
            uri: file_uri,
            range,
        });
    }

    if locations.is_empty() {
        return None;
    }

    if locations.len() == 1 {
        Some(GotoDefinitionResponse::Scalar(locations.remove(0)))
    } else {
        Some(GotoDefinitionResponse::Array(locations))
    }
}

/// Find all overrides of `method_name` in subclasses of the current file's class.
fn find_method_overrides(
    method_name: &str,
    source: &str,
    uri: &Url,
    workspace: &WorkspaceIndex,
) -> Option<GotoDefinitionResponse> {
    // Determine the class_name of the current file
    let class_name = find_file_class_name(source, uri, workspace)?;

    let subtypes = collect_all_subtypes(&class_name, workspace);
    if subtypes.is_empty() {
        return None;
    }

    let mut locations = Vec::new();
    for subtype in &subtypes {
        let Some(path) = workspace.lookup_class_name(subtype) else {
            continue;
        };
        let Some(symbols) = workspace.get_symbols(&path) else {
            continue;
        };

        // Check if this subtype has a function with the same name
        let Some(func) = symbols.functions.iter().find(|f| f.name == method_name) else {
            continue;
        };

        let Ok(file_uri) = Url::from_file_path(&path) else {
            continue;
        };

        // Use the function's line from the symbol table for precise location;
        // column 0 is fine since we don't store column info in FuncDecl.
        let line = func.line as u32;
        locations.push(Location {
            uri: file_uri,
            range: Range::new(Position::new(line, 0), Position::new(line, 0)),
        });
    }

    if locations.is_empty() {
        return None;
    }

    if locations.len() == 1 {
        Some(GotoDefinitionResponse::Scalar(locations.remove(0)))
    } else {
        Some(GotoDefinitionResponse::Array(locations))
    }
}

/// Determine the `class_name` of a file, either from the source or workspace symbols.
fn find_file_class_name(source: &str, uri: &Url, workspace: &WorkspaceIndex) -> Option<String> {
    // Try workspace symbols first (covers unsaved edits)
    if let Ok(path) = uri.to_file_path()
        && let Some(symbols) = workspace.get_symbols(&path)
        && let Some(cn) = &symbols.class_name
    {
        return Some(cn.clone());
    }

    // Parse source directly as fallback
    let tree = crate::core::parser::parse(source).ok()?;
    let file = gd_ast::convert(&tree, source);
    file.class_name.map(String::from)
}

/// Parse a source to find the range of its `class_name` identifier.
fn find_class_name_range(source: &str, name: &str) -> Option<Range> {
    let tree = crate::core::parser::parse(source).ok()?;
    let file = gd_ast::convert(&tree, source);
    if file.class_name == Some(name) {
        file.class_name_node.map(|n| node_range(&n))
    } else {
        None
    }
}

/// Collect all transitive subtypes of a class via BFS.
fn collect_all_subtypes(class: &str, workspace: &WorkspaceIndex) -> Vec<String> {
    let mut result = Vec::new();
    let mut queue = vec![class.to_string()];
    let mut seen = HashSet::new();
    while let Some(current) = queue.pop() {
        if !seen.insert(current.clone()) {
            continue;
        }
        for subtype in workspace.subtypes(&current) {
            result.push(subtype.clone());
            queue.push(subtype);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collect_all_subtypes_empty_workspace() {
        let workspace = WorkspaceIndex::new_empty();
        let result = collect_all_subtypes("BaseClass", &workspace);
        assert!(result.is_empty());
    }

    #[test]
    fn find_implementations_no_class_name() {
        let source = "func foo():\n\tpass\n";
        let uri = Url::parse("file:///test.gd").unwrap();
        let workspace = WorkspaceIndex::new_empty();
        // No class_name in file, no workspace classes — should return None
        let result = find_implementations(source, &uri, Position::new(0, 5), &workspace);
        assert!(result.is_none());
    }

    #[test]
    fn find_implementations_on_class_name_no_subtypes() {
        let source = "class_name MyClass\n\nfunc foo():\n\tpass\n";
        let uri = Url::parse("file:///test.gd").unwrap();
        let workspace = WorkspaceIndex::new_empty();
        // Cursor on "MyClass" at line 0, col 11
        let result = find_implementations(source, &uri, Position::new(0, 11), &workspace);
        // No subtypes in empty workspace
        assert!(result.is_none());
    }

    #[test]
    fn find_class_name_range_parses_correctly() {
        let source = "class_name Foo\n\nfunc bar():\n\tpass\n";
        let range = find_class_name_range(source, "Foo");
        assert!(range.is_some());
        let range = range.unwrap();
        assert_eq!(range.start.line, 0);
        assert_eq!(range.start.character, 11);
    }

    #[test]
    fn find_file_class_name_from_source() {
        let source = "class_name MyNode\nextends Node2D\n";
        let uri = Url::parse("file:///test.gd").unwrap();
        let workspace = WorkspaceIndex::new_empty();
        let name = find_file_class_name(source, &uri, &workspace);
        assert_eq!(name.as_deref(), Some("MyNode"));
    }
}
