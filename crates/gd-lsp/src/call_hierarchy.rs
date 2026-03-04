use std::collections::HashMap;

use tower_lsp::lsp_types::{
    CallHierarchyIncomingCall, CallHierarchyItem, CallHierarchyOutgoingCall, Position, Range,
    SymbolKind, Url,
};

use gd_core::gd_ast::{self, GdDecl};

use super::util::{FUNCTION_KINDS, node_range, node_text};
use super::workspace::WorkspaceIndex;

/// Prepare a call hierarchy item for the function at the given cursor position.
///
/// If the cursor is on a function definition name, returns that function.
/// If the cursor is on a call identifier, resolves to the enclosing function instead.
pub fn prepare(source: &str, uri: &Url, position: Position) -> Option<Vec<CallHierarchyItem>> {
    let tree = gd_core::parser::parse(source).ok()?;
    let root = tree.root_node();
    let file = gd_ast::convert(&tree, source);

    let point = tree_sitter::Point::new(position.line as usize, position.character as usize);
    let node = root.descendant_for_point_range(point, point)?;

    // Check if the cursor is on a call identifier — if so, try to resolve to the
    // called function's declaration in this file.
    if let Some(parent) = node.parent()
        && (parent.kind() == "call" || parent.kind() == "attribute_call")
    {
        let callee_name = node.utf8_text(source.as_bytes()).ok()?;
        if let Some(f) = file.find_func(callee_name)
            && let Some(name_node) = f.name_node
        {
            return Some(vec![make_item(callee_name, &f.node, &name_node, uri)]);
        }
    }

    // Walk up to find the enclosing function definition.
    let func_node = find_enclosing_function(root, point)?;
    let name_node = func_node.child_by_field_name("name")?;
    let func_name = node_text(&name_node, source);

    Some(vec![make_item(func_name, &func_node, &name_node, uri)])
}

/// Find all functions that call the given `item` across the workspace.
pub fn incoming_calls(
    item: &CallHierarchyItem,
    workspace: &WorkspaceIndex,
) -> Vec<CallHierarchyIncomingCall> {
    let target_name = &item.name;
    let mut results = Vec::new();

    for (path, content) in workspace.all_files() {
        let Ok(tree) = gd_core::parser::parse(&content) else {
            continue;
        };
        let Ok(file_uri) = Url::from_file_path(&path) else {
            continue;
        };

        let file = gd_ast::convert(&tree, &content);

        // Walk all top-level function definitions in this file.
        for decl in &file.declarations {
            let GdDecl::Func(f) = decl else {
                continue;
            };
            let Some(name_node) = f.name_node else {
                continue;
            };

            // Collect calls inside this function's body via CST.
            let Some(body) = f.node.child_by_field_name("body") else {
                continue;
            };
            let calls = collect_calls_in_node(body, &content);

            // Filter to calls matching the target function name.
            let matching_ranges: Vec<Range> = calls
                .into_iter()
                .filter(|(name, _)| name == target_name)
                .map(|(_, range)| range)
                .collect();

            if matching_ranges.is_empty() {
                continue;
            }

            let from_item = make_item(f.name, &f.node, &name_node, &file_uri);
            results.push(CallHierarchyIncomingCall {
                from: from_item,
                from_ranges: matching_ranges,
            });
        }
    }

    // Sort for deterministic output (DashMap iteration order is non-deterministic).
    results.sort_by(|a, b| {
        a.from
            .uri
            .as_str()
            .cmp(b.from.uri.as_str())
            .then(a.from.range.start.line.cmp(&b.from.range.start.line))
    });

    results
}

/// Find all functions called from the given `item`'s body.
pub fn outgoing_calls(item: &CallHierarchyItem, source: &str) -> Vec<CallHierarchyOutgoingCall> {
    let Ok(tree) = gd_core::parser::parse(source) else {
        return Vec::new();
    };
    let file = gd_ast::convert(&tree, source);

    let Some(f) = file.find_func(&item.name) else {
        return Vec::new();
    };
    let Some(body) = f.node.child_by_field_name("body") else {
        return Vec::new();
    };

    let calls = collect_calls_in_node(body, source);

    // Group by called function name.
    let mut grouped: HashMap<String, Vec<Range>> = HashMap::new();
    for (name, range) in calls {
        grouped.entry(name).or_default().push(range);
    }

    let mut results: Vec<CallHierarchyOutgoingCall> = grouped
        .into_iter()
        .map(|(name, from_ranges)| {
            // Try to resolve the called function to a declaration in this file.
            let to = if let Some(target) = file.find_func(&name)
                && let Some(target_name_node) = target.name_node
            {
                make_item(&name, &target.node, &target_name_node, &item.uri)
            } else {
                // External or builtin — create a synthetic item using the first call range.
                let call_range = from_ranges.first().copied().unwrap_or_default();
                CallHierarchyItem {
                    name: name.clone(),
                    kind: SymbolKind::FUNCTION,
                    tags: None,
                    detail: None,
                    uri: item.uri.clone(),
                    range: call_range,
                    selection_range: call_range,
                    data: None,
                }
            };
            CallHierarchyOutgoingCall { to, from_ranges }
        })
        .collect();

    // Sort for deterministic output.
    results.sort_by(|a, b| a.to.name.cmp(&b.to.name));

    results
}

// ── Helpers ────────────────────────────────────────────────────────────────

/// Construct a `CallHierarchyItem` from a function node.
fn make_item(
    name: &str,
    func_node: &tree_sitter::Node,
    name_node: &tree_sitter::Node,
    uri: &Url,
) -> CallHierarchyItem {
    CallHierarchyItem {
        name: name.to_string(),
        kind: SymbolKind::FUNCTION,
        tags: None,
        detail: None,
        uri: uri.clone(),
        range: node_range(func_node),
        selection_range: node_range(name_node),
        data: None,
    }
}

/// Find the enclosing function definition for a given source position.
fn find_enclosing_function(
    root: tree_sitter::Node,
    point: tree_sitter::Point,
) -> Option<tree_sitter::Node> {
    let leaf = root.descendant_for_point_range(point, point)?;
    let mut node = leaf;
    loop {
        if FUNCTION_KINDS.contains(&node.kind()) {
            return Some(node);
        }
        node = node.parent()?;
    }
}

/// Recursively collect all `(function_name, call_range)` pairs from call and
/// `attribute_call` nodes within a subtree.
fn collect_calls_in_node(node: tree_sitter::Node, source: &str) -> Vec<(String, Range)> {
    let mut results = Vec::new();
    collect_calls_recursive(node, source, &mut results);
    results
}

fn collect_calls_recursive(
    node: tree_sitter::Node,
    source: &str,
    results: &mut Vec<(String, Range)>,
) {
    match node.kind() {
        "call" => {
            // `call` node: `named_child(0)` is the callee identifier.
            if let Some(callee) = node.named_child(0)
                && callee.kind() == "identifier"
            {
                let name = node_text(&callee, source).to_string();
                results.push((name, node_range(&callee)));
            }
        }
        "attribute_call" => {
            // `attribute_call` node: first named child is the method name.
            if let Some(method_name) = node.named_child(0)
                && method_name.kind() == "identifier"
            {
                let name = node_text(&method_name, source).to_string();
                results.push((name, node_range(&method_name)));
            }
        }
        _ => {}
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_calls_recursive(child, source, results);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_uri() -> Url {
        Url::parse("file:///test.gd").unwrap()
    }

    #[test]
    fn prepare_on_function_def() {
        let source = "func greet():\n\tpass\n";
        let result = prepare(source, &test_uri(), Position::new(0, 5));
        assert!(result.is_some());
        let items = result.unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name, "greet");
    }

    #[test]
    fn prepare_inside_function_body() {
        let source = "func greet():\n\tvar x = 1\n\tprint(x)\n";
        let result = prepare(source, &test_uri(), Position::new(1, 5));
        assert!(result.is_some());
        let items = result.unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name, "greet");
    }

    #[test]
    fn prepare_outside_function_returns_none() {
        let source = "var x = 1\n";
        let result = prepare(source, &test_uri(), Position::new(0, 4));
        assert!(result.is_none());
    }

    #[test]
    fn outgoing_calls_found() {
        let source = "func helper():\n\tpass\n\nfunc main():\n\thelper()\n\tprint(1)\n";
        let item = CallHierarchyItem {
            name: "main".to_string(),
            kind: SymbolKind::FUNCTION,
            tags: None,
            detail: None,
            uri: test_uri(),
            range: Range::new(Position::new(3, 0), Position::new(5, 0)),
            selection_range: Range::new(Position::new(3, 5), Position::new(3, 9)),
            data: None,
        };
        let calls = outgoing_calls(&item, source);
        let names: Vec<&str> = calls.iter().map(|c| c.to.name.as_str()).collect();
        assert!(names.contains(&"helper"));
        assert!(names.contains(&"print"));
    }

    #[test]
    fn outgoing_calls_empty_body() {
        let source = "func main():\n\tpass\n";
        let item = CallHierarchyItem {
            name: "main".to_string(),
            kind: SymbolKind::FUNCTION,
            tags: None,
            detail: None,
            uri: test_uri(),
            range: Range::new(Position::new(0, 0), Position::new(1, 5)),
            selection_range: Range::new(Position::new(0, 5), Position::new(0, 9)),
            data: None,
        };
        let calls = outgoing_calls(&item, source);
        assert!(calls.is_empty());
    }

    #[test]
    fn outgoing_calls_grouped_by_name() {
        let source = "func main():\n\tprint(1)\n\tprint(2)\n";
        let item = CallHierarchyItem {
            name: "main".to_string(),
            kind: SymbolKind::FUNCTION,
            tags: None,
            detail: None,
            uri: test_uri(),
            range: Range::new(Position::new(0, 0), Position::new(2, 9)),
            selection_range: Range::new(Position::new(0, 5), Position::new(0, 9)),
            data: None,
        };
        let calls = outgoing_calls(&item, source);
        // `print` should appear once, with two from_ranges.
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].to.name, "print");
        assert_eq!(calls[0].from_ranges.len(), 2);
    }
}
