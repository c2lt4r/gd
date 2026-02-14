use std::collections::HashSet;

use tower_lsp::lsp_types::{Location, Position, Url};

use super::util::{FUNCTION_KINDS, node_range};

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

    let mut locations = Vec::new();

    // Check if this is a local variable inside a function
    if let Some(func) = enclosing_function(root, point)
        && is_local_in_function(func, target_name, source)
    {
        collect_scoped_references(
            func,
            source,
            target_name,
            uri,
            include_declaration,
            &mut locations,
        );
        return if locations.is_empty() {
            None
        } else {
            Some(locations)
        };
    }

    // Global: collect all matching identifiers in the file
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

    // If local variable, only search within the enclosing function — no cross-file
    if let Some(func) = enclosing_function(root, point)
        && is_local_in_function(func, target_name, source)
    {
        collect_scoped_references(
            func,
            source,
            target_name,
            uri,
            include_declaration,
            &mut locations,
        );
        return if locations.is_empty() {
            None
        } else {
            Some(locations)
        };
    }

    // Global: current file
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
            let Ok(file_uri) = Url::from_file_path(&path) else {
                continue;
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

// ── Scope-aware helpers ────────────────────────────────────────────────────

/// Find the enclosing function_definition or constructor_definition for a position.
pub(super) fn enclosing_function(
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

/// Check if `name` is declared locally in a function (as param or var in the body).
fn is_local_in_function(func: tree_sitter::Node, name: &str, source: &str) -> bool {
    // Check parameters
    if let Some(params) = func.child_by_field_name("parameters") {
        let param_names = collect_param_names(params, source);
        if param_names.contains(name) {
            return true;
        }
    }

    // Check body for variable_statement declarations
    if let Some(body) = func.child_by_field_name("body")
        && is_declared_in_body(body, name, source)
    {
        return true;
    }

    false
}

/// Collect parameter names from a function's parameters node.
fn collect_param_names(params: tree_sitter::Node, source: &str) -> HashSet<String> {
    let mut names = HashSet::new();
    let mut cursor = params.walk();
    if !cursor.goto_first_child() {
        return names;
    }
    loop {
        let child = cursor.node();
        match child.kind() {
            "identifier" => {
                names.insert(source[child.byte_range()].to_string());
            }
            "typed_parameter" | "default_parameter" | "typed_default_parameter" => {
                if let Some(name_node) = child.child(0)
                    && name_node.kind() == "identifier"
                {
                    names.insert(source[name_node.byte_range()].to_string());
                }
            }
            _ => {}
        }
        if !cursor.goto_next_sibling() {
            break;
        }
    }
    names
}

/// Check if `name` is declared in a body node via variable_statement or for_statement.
fn is_declared_in_body(body: tree_sitter::Node, name: &str, source: &str) -> bool {
    let mut cursor = body.walk();
    if !cursor.goto_first_child() {
        return false;
    }
    loop {
        let child = cursor.node();
        if child.kind() == "variable_statement"
            && let Some(name_node) = child.child_by_field_name("name")
            && name_node.utf8_text(source.as_bytes()).unwrap_or("") == name
        {
            return true;
        }
        if child.kind() == "for_statement"
            && let Some(left) = child.child_by_field_name("left")
            && left.utf8_text(source.as_bytes()).unwrap_or("") == name
        {
            return true;
        }
        if !cursor.goto_next_sibling() {
            break;
        }
    }
    false
}

/// Collect references within a function scope, respecting inner scopes that shadow the name.
fn collect_scoped_references(
    func: tree_sitter::Node,
    source: &str,
    target_name: &str,
    uri: &Url,
    include_declaration: bool,
    locations: &mut Vec<Location>,
) {
    // Collect references from parameters
    if let Some(params) = func.child_by_field_name("parameters") {
        collect_scoped_refs_in_node(
            params,
            source,
            target_name,
            uri,
            include_declaration,
            locations,
        );
    }

    // Collect references from body, respecting shadowing
    if let Some(body) = func.child_by_field_name("body") {
        collect_scoped_refs_in_body(
            body,
            source,
            target_name,
            uri,
            include_declaration,
            locations,
        );
    }
}

/// Walk a body node collecting references, skipping inner scopes that re-declare the name.
fn collect_scoped_refs_in_body(
    body: tree_sitter::Node,
    source: &str,
    target_name: &str,
    uri: &Url,
    include_declaration: bool,
    locations: &mut Vec<Location>,
) {
    let mut cursor = body.walk();
    if !cursor.goto_first_child() {
        return;
    }
    loop {
        let child = cursor.node();

        // Skip nested functions entirely — they have their own scope
        if FUNCTION_KINDS.contains(&child.kind()) || child.kind() == "lambda" {
            if !cursor.goto_next_sibling() {
                break;
            }
            continue;
        }

        // For for_statement: if it re-declares the name as iterator, skip its body
        if child.kind() == "for_statement"
            && let Some(left) = child.child_by_field_name("left")
            && left.utf8_text(source.as_bytes()).unwrap_or("") == target_name
        {
            // The iterator variable itself is a reference/declaration
            if include_declaration {
                locations.push(Location {
                    uri: uri.clone(),
                    range: node_range(&left),
                });
            }
            // Collect in the iterable expression (right) but skip body — it's shadowed
            if let Some(right) = child.child_by_field_name("right") {
                collect_scoped_refs_in_node(
                    right,
                    source,
                    target_name,
                    uri,
                    include_declaration,
                    locations,
                );
            }
            if !cursor.goto_next_sibling() {
                break;
            }
            continue;
        }

        // For scope nodes (if/while/etc), check if their body re-declares the name
        if is_scope_node(child.kind()) {
            collect_scoped_refs_in_scope_node(
                child,
                source,
                target_name,
                uri,
                include_declaration,
                locations,
            );
            if !cursor.goto_next_sibling() {
                break;
            }
            continue;
        }

        // Regular statement — collect refs normally
        collect_scoped_refs_in_node(
            child,
            source,
            target_name,
            uri,
            include_declaration,
            locations,
        );

        if !cursor.goto_next_sibling() {
            break;
        }
    }
}

/// Process a scope node (if/while/for/match), recursing into bodies while checking for shadowing.
fn collect_scoped_refs_in_scope_node(
    node: tree_sitter::Node,
    source: &str,
    target_name: &str,
    uri: &Url,
    include_declaration: bool,
    locations: &mut Vec<Location>,
) {
    // For for_statement with iterator that shadows our target, handle specially
    if node.kind() == "for_statement"
        && let Some(left) = node.child_by_field_name("left")
        && left.utf8_text(source.as_bytes()).unwrap_or("") == target_name
    {
        // Iterator shadows our name — collect in iterable but skip body
        if include_declaration {
            locations.push(Location {
                uri: uri.clone(),
                range: node_range(&left),
            });
        }
        if let Some(right) = node.child_by_field_name("right") {
            collect_scoped_refs_in_node(
                right,
                source,
                target_name,
                uri,
                include_declaration,
                locations,
            );
        }
        return;
    }

    let mut cursor = node.walk();
    if !cursor.goto_first_child() {
        return;
    }
    loop {
        let child = cursor.node();
        if child.kind() == "body" || child.kind() == "block" {
            // Check if this body re-declares the name — if so, skip (shadowed)
            if !is_declared_in_body(child, target_name, source) {
                collect_scoped_refs_in_body(
                    child,
                    source,
                    target_name,
                    uri,
                    include_declaration,
                    locations,
                );
            }
        } else if is_scope_node(child.kind()) {
            collect_scoped_refs_in_scope_node(
                child,
                source,
                target_name,
                uri,
                include_declaration,
                locations,
            );
        } else {
            // Condition expressions, etc. — collect refs normally
            collect_scoped_refs_in_node(
                child,
                source,
                target_name,
                uri,
                include_declaration,
                locations,
            );
        }
        if !cursor.goto_next_sibling() {
            break;
        }
    }
}

/// Simple recursive reference collection within a node (no scope awareness).
fn collect_scoped_refs_in_node(
    node: tree_sitter::Node,
    source: &str,
    target_name: &str,
    uri: &Url,
    include_declaration: bool,
    locations: &mut Vec<Location>,
) {
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

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_scoped_refs_in_node(
            child,
            source,
            target_name,
            uri,
            include_declaration,
            locations,
        );
    }
}

// ── Name-based reference search ──────────────────────────────────────────

/// Find all references to a symbol by name across workspace files.
/// If `file_filter` is Some, only search that file.
/// If `class_filter` is Some, only search files whose `class_name` or inner class matches.
pub fn find_references_by_name(
    name: &str,
    workspace: &super::workspace::WorkspaceIndex,
    file_filter: Option<&std::path::Path>,
    class_filter: Option<&str>,
) -> Vec<Location> {
    let mut locations = Vec::new();

    for (path, content) in workspace.all_files() {
        if let Some(filter_path) = file_filter
            && path != filter_path
        {
            continue;
        }

        if let Ok(tree) = crate::core::parser::parse(&content) {
            let root = tree.root_node();
            let Ok(uri) = Url::from_file_path(&path) else {
                continue;
            };

            if let Some(class_name) = class_filter {
                if has_class_name_statement(root, &content, class_name) {
                    // Whole file is this class — search everything
                    collect_references(root, &content, name, &uri, true, &mut locations);
                } else if let Some(class_node) = find_inner_class(root, &content, class_name) {
                    // Inner class — only search inside it
                    collect_references(class_node, &content, name, &uri, true, &mut locations);
                } else {
                    // Search for ClassName.method() calls (autoload/singleton pattern)
                    collect_qualified_references(
                        root,
                        &content,
                        class_name,
                        name,
                        &uri,
                        &mut locations,
                    );
                }
            } else {
                collect_references(root, &content, name, &uri, true, &mut locations);
            }
        }
    }

    // Sort for deterministic output (DashMap iteration order is non-deterministic)
    locations.sort_by(|a, b| {
        a.uri
            .as_str()
            .cmp(b.uri.as_str())
            .then(a.range.start.line.cmp(&b.range.start.line))
            .then(a.range.start.character.cmp(&b.range.start.character))
    });

    locations
}

/// Find references to `name` that are qualified with `class_name`, e.g.
/// `GameManager.submit_vote()` when class_name="GameManager" and name="submit_vote".
/// This covers the autoload/singleton calling pattern in Godot.
fn collect_qualified_references(
    node: tree_sitter::Node,
    source: &str,
    class_name: &str,
    target_name: &str,
    uri: &Url,
    locations: &mut Vec<Location>,
) {
    // tree-sitter parses `GameManager.submit_vote()` as:
    //   attribute {
    //     identifier "GameManager"
    //     "."
    //     attribute_call {
    //       identifier "submit_vote"
    //       arguments { ... }
    //     }
    //   }
    //
    // And `GameManager.some_prop` (without call) as:
    //   attribute {
    //     identifier "GameManager"
    //     "."
    //     identifier "some_prop"
    //   }
    if node.kind() == "attribute" {
        // Check if the first named child is an identifier matching class_name
        if let Some(obj) = node.named_child(0)
            && obj.kind() == "identifier"
            && obj.utf8_text(source.as_bytes()).unwrap_or("") == class_name
        {
            // Look for the member name in attribute_call or as a direct identifier
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "attribute_call" {
                    // Method call: ClassName.method(args)
                    if let Some(name_node) = child.named_child(0)
                        && name_node.kind() == "identifier"
                        && name_node.utf8_text(source.as_bytes()).unwrap_or("") == target_name
                    {
                        locations.push(Location {
                            uri: uri.clone(),
                            range: node_range(&name_node),
                        });
                    }
                } else if child.kind() == "identifier"
                    && child.utf8_text(source.as_bytes()).unwrap_or("") == target_name
                    && child.start_byte() != node.start_byte()
                {
                    // Property access: ClassName.property (not the first identifier)
                    locations.push(Location {
                        uri: uri.clone(),
                        range: node_range(&child),
                    });
                }
            }
        }
    }

    // Recurse into children
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_qualified_references(child, source, class_name, target_name, uri, locations);
    }
}

/// Check if a file has a top-level `class_name` statement matching `target`.
fn has_class_name_statement(root: tree_sitter::Node, source: &str, target: &str) -> bool {
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        if child.kind() == "class_name_statement" {
            let name = child.child_by_field_name("name").or_else(|| child.child(1));
            if let Some(n) = name
                && n.utf8_text(source.as_bytes()).unwrap_or("") == target
            {
                return true;
            }
        }
    }
    false
}

/// Find a top-level inner `class_definition` matching `target`.
fn find_inner_class<'a>(
    root: tree_sitter::Node<'a>,
    source: &str,
    target: &str,
) -> Option<tree_sitter::Node<'a>> {
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        if child.kind() == "class_definition"
            && let Some(name_node) = child.child_by_field_name("name")
            && name_node.utf8_text(source.as_bytes()).unwrap_or("") == target
        {
            return Some(child);
        }
    }
    None
}

fn is_scope_node(kind: &str) -> bool {
    matches!(
        kind,
        "if_statement"
            | "for_statement"
            | "while_statement"
            | "elif_clause"
            | "else_clause"
            | "match_statement"
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

    // ── Scope-aware tests ──────────────────────────────────────────────────

    #[test]
    fn local_var_refs_stay_in_function() {
        // Two functions each with `var x` — renaming in one shouldn't affect the other
        let source = "\
func foo():
\tvar x = 1
\tprint(x)
\tx = 2

func bar():
\tvar x = 10
\tprint(x)
";
        let uri = test_uri();
        // Position on `x` in foo() at line 2, col 7 (inside print)
        let result = find_references(source, &uri, Position::new(2, 7), true);
        assert!(result.is_some());
        let locs = result.unwrap();
        // Should find: var x (decl) + print(x) + x = 2 => 3 refs, all in foo()
        assert_eq!(locs.len(), 3, "should find 3 refs in foo() only");
        for loc in &locs {
            assert!(
                loc.range.start.line <= 3,
                "all refs should be in foo(), got line {}",
                loc.range.start.line
            );
        }
    }

    #[test]
    fn parameter_refs_stay_in_function() {
        let source = "\
func foo(speed):
\tprint(speed)

func bar(speed):
\tspeed = 20
";
        let uri = test_uri();
        // Position on `speed` in foo() at line 1, col 7
        let result = find_references(source, &uri, Position::new(1, 7), true);
        assert!(result.is_some());
        let locs = result.unwrap();
        // Should find: speed param + print(speed) => 2 refs in foo() only
        assert_eq!(locs.len(), 2, "should find 2 refs in foo() only");
        for loc in &locs {
            assert!(
                loc.range.start.line <= 1,
                "all refs should be in foo(), got line {}",
                loc.range.start.line
            );
        }
    }

    #[test]
    fn global_var_refs_span_file() {
        // Top-level `var speed` should be found across all functions
        let source = "\
var speed = 10

func foo():
\tprint(speed)

func bar():
\tspeed = 20
";
        let uri = test_uri();
        // Position on `speed` usage at line 3, col 7
        let result = find_references(source, &uri, Position::new(3, 7), true);
        assert!(result.is_some());
        let locs = result.unwrap();
        // Should find: var speed (decl) + print(speed) + speed = 20 => 3 refs
        assert_eq!(locs.len(), 3, "should find 3 refs across file");
    }

    #[test]
    fn for_loop_var_scoped() {
        let source = "\
func foo():
\tvar i = 99
\tfor i in range(10):
\t\tprint(i)
\tprint(i)
";
        let uri = test_uri();
        // Position on `i` at line 1 (var i = 99)
        let result = find_references(source, &uri, Position::new(1, 5), true);
        assert!(result.is_some());
        let locs = result.unwrap();
        // var i decl (line 1) + for i iterator (line 2, collected as decl) + print(i) at line 4
        assert!(
            locs.len() >= 2,
            "should find at least var i + print(i) after loop"
        );
    }

    #[test]
    fn shadowed_var_inner_scope() {
        let source = "\
func foo():
\tvar x = 1
\tprint(x)
\tif true:
\t\tvar x = 2
\t\tprint(x)
\tprint(x)
";
        let uri = test_uri();
        // Position on outer `x` at line 2, col 7
        let result = find_references(source, &uri, Position::new(2, 7), true);
        assert!(result.is_some());
        let locs = result.unwrap();
        // Should find: var x (line 1) + print(x) (line 2) + print(x) (line 6)
        // The if block's body declares `var x`, so it's skipped
        assert_eq!(locs.len(), 3, "should skip shadowed if-block");
        // None should be on lines 4 or 5 (inside the if block)
        for loc in &locs {
            assert!(
                loc.range.start.line != 4 && loc.range.start.line != 5,
                "should not include refs from shadowed if-block, got line {}",
                loc.range.start.line
            );
        }
    }
}
