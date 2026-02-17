use std::path::PathBuf;

use tower_lsp::lsp_types::{GotoDefinitionResponse, Location, Position, Range, Url};

use super::util::{FUNCTION_KINDS, matches_name, node_range, node_text};

/// Resolve go-to-definition at the given position within a single file.
pub fn goto_definition(
    source: &str,
    uri: &Url,
    position: Position,
) -> Option<GotoDefinitionResponse> {
    let tree = crate::core::parser::parse(source).ok()?;
    let root = tree.root_node();

    let point = tree_sitter::Point::new(position.line as usize, position.character as usize);
    let node = root.descendant_for_point_range(point, point)?;

    // Handle `extends "res://path.gd"` string literals
    if node.kind() == "string" {
        let text = node.utf8_text(source.as_bytes()).ok()?;
        return resolve_extends_path(text, uri);
    }

    // Get the identifier text at the cursor
    let ident = node.utf8_text(source.as_bytes()).ok()?;

    // If inside a function, check local declarations first
    if let Some(result) = find_local_definition(root, point, ident, source, uri) {
        return Some(result);
    }

    // Search top-level nodes for a matching definition
    find_definition(&root, ident, source, uri)
}

/// Find a top-level definition that matches the given name.
fn find_definition(
    root: &tree_sitter::Node,
    name: &str,
    source: &str,
    uri: &Url,
) -> Option<GotoDefinitionResponse> {
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        let matched = match child.kind() {
            "function_definition"
            | "variable_statement"
            | "const_statement"
            | "signal_statement"
            | "class_definition"
            | "enum_definition" => matches_name(&child, name, source),
            "class_name_statement" => {
                // class_name MyClass  — the name is the second child
                child
                    .child_by_field_name("name")
                    .or_else(|| child.child(1))
                    .is_some_and(|n| node_text(&n, source) == name)
            }
            _ => false,
        };

        if matched {
            let name_node = child.child_by_field_name("name").unwrap_or(child);
            return Some(GotoDefinitionResponse::Scalar(Location {
                uri: uri.clone(),
                range: node_range(&name_node),
            }));
        }
    }
    None
}

/// For `extends "res://path.gd"` strings, resolve the path relative to the project root.
fn resolve_extends_path(text: &str, uri: &Url) -> Option<GotoDefinitionResponse> {
    // Strip surrounding quotes
    let inner = text.trim_matches('"').trim_matches('\'');
    if !inner.starts_with("res://") {
        return None;
    }

    let rel_path = &inner["res://".len()..];

    // Derive project root from the current file's directory, walking up to find project.godot
    let file_path = uri.to_file_path().ok()?;
    let project_root = find_project_root(&file_path)?;
    let target = project_root.join(rel_path);

    if target.exists() {
        let target_uri = Url::from_file_path(&target).ok()?;
        Some(GotoDefinitionResponse::Scalar(Location {
            uri: target_uri,
            range: Range::new(Position::new(0, 0), Position::new(0, 0)),
        }))
    } else {
        None
    }
}

/// Walk up directories to find the Godot project root (containing project.godot).
fn find_project_root(from: &std::path::Path) -> Option<PathBuf> {
    let mut dir = from.parent()?;
    loop {
        if dir.join("project.godot").exists() {
            return Some(dir.to_path_buf());
        }
        dir = dir.parent()?;
    }
}

/// Find a local definition (parameter or var) inside the enclosing function.
fn find_local_definition(
    root: tree_sitter::Node,
    point: tree_sitter::Point,
    name: &str,
    source: &str,
    uri: &Url,
) -> Option<GotoDefinitionResponse> {
    // Walk from leaf up to find enclosing function
    let leaf = root.descendant_for_point_range(point, point)?;
    let mut node = leaf;
    let func = loop {
        if FUNCTION_KINDS.contains(&node.kind()) {
            break node;
        }
        node = node.parent()?;
    };

    // Check parameters
    if let Some(params) = func.child_by_field_name("parameters") {
        let mut cursor = params.walk();
        if cursor.goto_first_child() {
            loop {
                let child = cursor.node();
                match child.kind() {
                    "identifier" if node_text(&child, source) == name => {
                        return Some(GotoDefinitionResponse::Scalar(Location {
                            uri: uri.clone(),
                            range: node_range(&child),
                        }));
                    }
                    "typed_parameter" | "default_parameter" | "typed_default_parameter" => {
                        if let Some(name_node) = child.child(0)
                            && name_node.kind() == "identifier"
                            && node_text(&name_node, source) == name
                        {
                            return Some(GotoDefinitionResponse::Scalar(Location {
                                uri: uri.clone(),
                                range: node_range(&name_node),
                            }));
                        }
                    }
                    _ => {}
                }
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
        }
    }

    // Check body for variable_statement and for_statement declarations
    if let Some(body) = func.child_by_field_name("body")
        && let Some(loc) = find_var_in_body(body, name, source, uri)
    {
        return Some(GotoDefinitionResponse::Scalar(loc));
    }

    None
}

/// Search a body node for a variable_statement or for_statement declaring `name`.
fn find_var_in_body(
    body: tree_sitter::Node,
    name: &str,
    source: &str,
    uri: &Url,
) -> Option<Location> {
    let mut cursor = body.walk();
    if !cursor.goto_first_child() {
        return None;
    }
    loop {
        let child = cursor.node();
        if child.kind() == "variable_statement"
            && let Some(name_node) = child.child_by_field_name("name")
            && node_text(&name_node, source) == name
        {
            return Some(Location {
                uri: uri.clone(),
                range: node_range(&name_node),
            });
        }
        if child.kind() == "for_statement"
            && let Some(left) = child.child_by_field_name("left")
            && node_text(&left, source) == name
        {
            return Some(Location {
                uri: uri.clone(),
                range: node_range(&left),
            });
        }
        if !cursor.goto_next_sibling() {
            break;
        }
    }
    None
}

/// Resolve go-to-definition using workspace for cross-file support.
pub fn goto_definition_cross_file(
    source: &str,
    uri: &Url,
    position: Position,
    workspace: &super::workspace::WorkspaceIndex,
) -> Option<GotoDefinitionResponse> {
    let tree = crate::core::parser::parse(source).ok()?;
    let root = tree.root_node();

    let point = tree_sitter::Point::new(position.line as usize, position.character as usize);
    let node = root.descendant_for_point_range(point, point)?;

    // Handle string nodes (preload/load/extends paths) using workspace resolution
    if node.kind() == "string" {
        let text = node.utf8_text(source.as_bytes()).ok()?;
        let inner = text.trim_matches('"').trim_matches('\'');
        if inner.starts_with("res://")
            && let Some(path) = workspace.resolve_res_path(inner)
        {
            let target_uri = Url::from_file_path(&path).ok()?;
            return Some(GotoDefinitionResponse::Scalar(Location {
                uri: target_uri,
                range: Range::new(Position::new(0, 0), Position::new(0, 0)),
            }));
        }
        return None;
    }

    let ident = node.utf8_text(source.as_bytes()).ok()?;

    // If inside a function, check local declarations first
    if let Some(result) = find_local_definition(root, point, ident, source, uri) {
        return Some(result);
    }

    // Try single-file definition first
    if let Some(result) = find_definition(&root, ident, source, uri) {
        return Some(result);
    }

    // Search declaration index for candidate files (O(K) instead of O(N))
    let current_path = uri.to_file_path().ok();
    for path in workspace.lookup_declaration(ident) {
        if current_path.as_ref() == Some(&path) {
            continue;
        }
        if let Some(content) = workspace.get_content(&path)
            && let Ok(tree) = crate::core::parser::parse(&content)
        {
            let Ok(file_uri) = Url::from_file_path(&path) else {
                continue;
            };
            if let Some(result) = find_definition(&tree.root_node(), ident, &content, &file_uri) {
                return Some(result);
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_uri() -> Url {
        Url::parse("file:///test.gd").unwrap()
    }

    #[test]
    fn goto_function_definition() {
        let source = "func greet():\n\tpass\n\nfunc main():\n\tgreet()\n";
        let uri = test_uri();
        // Position on `greet()` call at line 4, col 1
        let result = goto_definition(source, &uri, Position::new(4, 1));
        assert!(result.is_some());
        if let Some(GotoDefinitionResponse::Scalar(loc)) = result {
            assert_eq!(loc.range.start.line, 0);
            assert_eq!(loc.range.start.character, 5); // "greet" starts after "func "
        } else {
            panic!("Expected Scalar response");
        }
    }

    #[test]
    fn goto_variable_definition() {
        let source = "var speed = 10\n\nfunc run():\n\tprint(speed)\n";
        let uri = test_uri();
        // Position on `speed` at line 3, col 7
        let result = goto_definition(source, &uri, Position::new(3, 7));
        assert!(result.is_some());
        if let Some(GotoDefinitionResponse::Scalar(loc)) = result {
            assert_eq!(loc.range.start.line, 0);
            assert_eq!(loc.range.start.character, 4); // "speed" starts after "var "
        } else {
            panic!("Expected Scalar response");
        }
    }

    #[test]
    fn goto_signal_definition() {
        let source = "signal health_changed\n\nfunc hit():\n\thealth_changed.emit()\n";
        let uri = test_uri();
        // Position on `health_changed` at line 3
        let result = goto_definition(source, &uri, Position::new(3, 5));
        assert!(result.is_some());
        if let Some(GotoDefinitionResponse::Scalar(loc)) = result {
            assert_eq!(loc.range.start.line, 0);
        } else {
            panic!("Expected Scalar response");
        }
    }

    #[test]
    fn unknown_identifier_returns_none() {
        let source = "func main():\n\tunknown_thing()\n";
        let uri = test_uri();
        let result = goto_definition(source, &uri, Position::new(1, 5));
        assert!(result.is_none());
    }

    #[test]
    fn empty_source_returns_none() {
        let uri = test_uri();
        let result = goto_definition("", &uri, Position::new(0, 0));
        assert!(result.is_none());
    }

    #[test]
    fn goto_local_variable_definition() {
        let source = "func foo():\n\tvar x = 10\n\tprint(x)\n";
        let uri = test_uri();
        // Position on `x` usage at line 2, col 7
        let result = goto_definition(source, &uri, Position::new(2, 7));
        assert!(result.is_some());
        if let Some(GotoDefinitionResponse::Scalar(loc)) = result {
            assert_eq!(loc.range.start.line, 1, "should jump to local var x");
            assert_eq!(loc.range.start.character, 5);
        } else {
            panic!("Expected Scalar response");
        }
    }

    #[test]
    fn goto_parameter_definition() {
        let source = "func foo(speed):\n\tprint(speed)\n";
        let uri = test_uri();
        // Position on `speed` usage at line 1, col 7
        let result = goto_definition(source, &uri, Position::new(1, 7));
        assert!(result.is_some());
        if let Some(GotoDefinitionResponse::Scalar(loc)) = result {
            assert_eq!(loc.range.start.line, 0, "should jump to parameter");
            assert_eq!(loc.range.start.character, 9);
        } else {
            panic!("Expected Scalar response");
        }
    }

    #[test]
    fn local_var_preferred_over_global() {
        // Local `var speed` should take priority over global `var speed`
        let source = "var speed = 10\n\nfunc foo():\n\tvar speed = 20\n\tprint(speed)\n";
        let uri = test_uri();
        // Position on `speed` at line 4, col 7 (inside print)
        let result = goto_definition(source, &uri, Position::new(4, 7));
        assert!(result.is_some());
        if let Some(GotoDefinitionResponse::Scalar(loc)) = result {
            assert_eq!(
                loc.range.start.line, 3,
                "should jump to local var, not global"
            );
        } else {
            panic!("Expected Scalar response");
        }
    }
}
