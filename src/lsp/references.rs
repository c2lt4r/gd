use std::collections::HashSet;
use std::path::Path;

use tower_lsp::lsp_types::{Location, Position, Range, Url};

use super::util::{FUNCTION_KINDS, node_range};
use crate::core::gd_ast::{self, GdDecl, GdFile};

// ── Static/instance disambiguation ──────────────────────────────────────

/// When the user targets a function that has a same-name counterpart (static vs
/// instance), this enum records which variant was targeted so we can filter
/// references accordingly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MethodKind {
    Static,
    Instance,
}

/// If the cursor is on the *name* of a function_definition and there is at
/// least one other function_definition with the same name (but different
/// staticness) in the same file, return the `MethodKind` of the targeted
/// declaration and the 0-based line of the *other* declaration.
fn detect_ambiguous_overload(
    file: &GdFile,
    target_name: &str,
    cursor_line: usize,
) -> Option<MethodKind> {
    let mut decls: Vec<(usize, bool)> = Vec::new(); // (line, is_static)
    for d in &file.declarations {
        if let GdDecl::Func(f) = d
            && f.name == target_name
        {
            decls.push((f.node.start_position().row, f.is_static));
        }
    }
    if decls.len() < 2 {
        return None; // No ambiguity — single or zero declarations
    }
    // Find the declaration the cursor is on (or closest to)
    decls
        .iter()
        .find(|(line, _)| *line == cursor_line)
        .map(|(_, is_static)| {
            if *is_static {
                MethodKind::Static
            } else {
                MethodKind::Instance
            }
        })
}

/// Check if a `function_definition` node has a `static_keyword` child.
fn has_static_keyword(func_node: &tree_sitter::Node) -> bool {
    let mut cursor = func_node.walk();
    for child in func_node.children(&mut cursor) {
        if child.kind() == "static_keyword" {
            return true;
        }
    }
    false
}

/// Determine the `MethodKind` of the enclosing function for a reference node.
/// Returns `None` when the node is not inside any function body (top-level code).
fn enclosing_method_kind(node: &tree_sitter::Node) -> Option<MethodKind> {
    let mut cur = *node;
    loop {
        cur = cur.parent()?;
        if FUNCTION_KINDS.contains(&cur.kind()) {
            return Some(if has_static_keyword(&cur) {
                MethodKind::Static
            } else {
                MethodKind::Instance
            });
        }
    }
}

/// Check whether an identifier node is part of a `ClassName.method()` qualified
/// call. Returns `true` when the tree-sitter AST looks like:
///
/// ```text
/// attribute {
///   identifier <ClassName>    ← first named child
///   attribute_call {
///     identifier <method>     ← this is `node`
///   }
/// }
/// ```
fn is_class_qualified_call(node: &tree_sitter::Node, source: &str, class_name: &str) -> bool {
    // node → attribute_call → attribute
    let Some(attr_call) = node.parent() else {
        return false;
    };
    if attr_call.kind() != "attribute_call" {
        return false;
    }
    let Some(attribute) = attr_call.parent() else {
        return false;
    };
    if attribute.kind() != "attribute" {
        return false;
    }
    // First named child of the attribute should be the class name
    attribute
        .named_child(0)
        .is_some_and(|obj| obj.utf8_text(source.as_bytes()).unwrap_or("") == class_name)
}

/// Check whether an identifier node is part of a `self.method()` call.
fn is_self_qualified_call(node: &tree_sitter::Node, source: &str) -> bool {
    // node → attribute_call → attribute → first child == "self"
    let Some(attr_call) = node.parent() else {
        return false;
    };
    if attr_call.kind() != "attribute_call" {
        return false;
    }
    let Some(attribute) = attr_call.parent() else {
        return false;
    };
    if attribute.kind() != "attribute" {
        return false;
    }
    attribute
        .named_child(0)
        .is_some_and(|obj| obj.utf8_text(source.as_bytes()).unwrap_or("") == "self")
}

/// Determine if a reference (non-declaration) matches the targeted `MethodKind`.
///
/// Returns `true` when we can confidently say the reference belongs to the
/// given kind, or when we cannot determine either way (ambiguous → include).
fn reference_matches_kind(
    node: &tree_sitter::Node,
    source: &str,
    kind: MethodKind,
    class_name: Option<&str>,
) -> bool {
    // Explicit `ClassName.method()` → always static
    if let Some(cls) = class_name
        && is_class_qualified_call(node, source, cls)
    {
        return kind == MethodKind::Static;
    }

    // Explicit `self.method()` → always instance
    if is_self_qualified_call(node, source) {
        return kind == MethodKind::Instance;
    }

    // Bare `method()` inside a function — infer from enclosing function staticness.
    // A static function can only call the static overload; an instance function
    // calls the instance overload.
    if let Some(enc) = enclosing_method_kind(node) {
        return enc == kind;
    }

    // Top-level code is instance context in GDScript
    kind == MethodKind::Instance
}

/// Extract the `class_name` from a typed file (if declared).
fn extract_class_name(file: &GdFile) -> Option<String> {
    file.class_name.map(String::from)
}

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

    // Check for ambiguous same-name function declarations (static vs instance)
    let file = gd_ast::convert(&tree, source);
    let overload_kind = detect_ambiguous_overload(&file, target_name, position.line as usize);
    let class_name = overload_kind.and_then(|_| extract_class_name(&file));
    let filter = overload_kind.map(|kind| OverloadFilter {
        kind,
        target_decl_line: position.line as usize,
        class_name: class_name.as_deref(),
    });
    collect_maybe_filtered(
        root,
        source,
        target_name,
        uri,
        include_declaration,
        filter.as_ref(),
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

    // Enum member declaration: `enumerator { left: (identifier) }`
    if parent.kind() == "enumerator" {
        return parent
            .child_by_field_name("left")
            .is_some_and(|left| left.id() == node.id());
    }

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

/// Filter context for collecting references when static/instance disambiguation
/// is needed.
struct OverloadFilter<'a> {
    kind: MethodKind,
    /// 0-based line of the targeted declaration. `usize::MAX` when collecting in
    /// a cross-file where no declaration is expected to match.
    target_decl_line: usize,
    /// `class_name` of the origin file (for detecting `ClassName.method()` calls).
    class_name: Option<&'a str>,
}

/// Like `collect_references`, but filters by `MethodKind` when there are
/// ambiguous same-name declarations (static vs instance).
fn collect_references_filtered(
    node: tree_sitter::Node,
    source: &str,
    target_name: &str,
    uri: &Url,
    include_declaration: bool,
    filter: &OverloadFilter<'_>,
    locations: &mut Vec<Location>,
) {
    if (node.kind() == "identifier" || node.kind() == "name")
        && node.utf8_text(source.as_bytes()).unwrap_or("") == target_name
    {
        if is_declaration(&node, source, target_name) {
            // Only include the declaration if it is the targeted one
            if include_declaration && node.start_position().row == filter.target_decl_line {
                locations.push(Location {
                    uri: uri.clone(),
                    range: node_range(&node),
                });
            }
        } else if reference_matches_kind(&node, source, filter.kind, filter.class_name) {
            locations.push(Location {
                uri: uri.clone(),
                range: node_range(&node),
            });
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_references_filtered(
            child,
            source,
            target_name,
            uri,
            include_declaration,
            filter,
            locations,
        );
    }
}

/// Dispatch to `collect_references_filtered` when a filter is present,
/// otherwise fall back to the unfiltered `collect_references`.
fn collect_maybe_filtered(
    node: tree_sitter::Node,
    source: &str,
    target_name: &str,
    uri: &Url,
    include_declaration: bool,
    filter: Option<&OverloadFilter<'_>>,
    locations: &mut Vec<Location>,
) {
    if let Some(f) = filter {
        collect_references_filtered(
            node,
            source,
            target_name,
            uri,
            include_declaration,
            f,
            locations,
        );
    } else {
        collect_references(
            node,
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

    // Check for ambiguous same-name function declarations (static vs instance)
    let file = gd_ast::convert(&tree, source);
    let overload_kind = detect_ambiguous_overload(&file, target_name, position.line as usize);
    let origin_class_name = extract_class_name(&file);

    // Build overload filter if disambiguation is needed
    let origin_filter = overload_kind.map(|kind| OverloadFilter {
        kind,
        target_decl_line: position.line as usize,
        class_name: origin_class_name.as_deref(),
    });

    // Current file
    collect_maybe_filtered(
        root,
        source,
        target_name,
        uri,
        include_declaration,
        origin_filter.as_ref(),
        &mut locations,
    );

    // Cross-file: only parse files that contain the identifier text
    let current_path = uri.to_file_path().ok();
    let cross_filter = overload_kind.map(|kind| OverloadFilter {
        kind,
        target_decl_line: usize::MAX, // no declaration match in other files
        class_name: origin_class_name.as_deref(),
    });

    for (path, content) in workspace.all_files() {
        if current_path.as_ref() == Some(&path) || !content.contains(target_name) {
            continue;
        }
        if let Ok(tree) = crate::core::parser::parse(&content)
            && let Ok(file_uri) = Url::from_file_path(&path)
        {
            collect_maybe_filtered(
                tree.root_node(),
                &content,
                target_name,
                &file_uri,
                true,
                cross_filter.as_ref(),
                &mut locations,
            );
        }
    }

    // Scene cross-references: find signal connections in .tscn files that
    // reference a handler function in the current script
    if let Some(ref path) = current_path {
        collect_scene_references(workspace, path, target_name, &mut locations);
    }

    if locations.is_empty() {
        None
    } else {
        Some(locations)
    }
}

// ── Scope-aware helpers ────────────────────────────────────────────────────

/// Find the enclosing function_definition or constructor_definition for a position.
pub(crate) fn enclosing_function(
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

// ── Scene-aware reference search ─────────────────────────────────────────

/// Collect references from .tscn files for a symbol in a script.
///
/// Adds locations for:
/// - Signal connections where `method == target_name` (handler functions)
/// - Signal connections where `signal == target_name` (signal declarations)
fn collect_scene_references(
    workspace: &super::workspace::WorkspaceIndex,
    script_path: &Path,
    target_name: &str,
    locations: &mut Vec<Location>,
) {
    // Signal handler references: handler function name appears in [connection] sections
    let handler_conns = workspace.signal_connections_for_handler(script_path, target_name);
    for conn in &handler_conns {
        if let Some(line) = find_scene_pattern(
            &conn.scene_path,
            &format!("method=\"{target_name}\""),
            &format!("method = \"{target_name}\""),
        ) && let Ok(uri) = Url::from_file_path(&conn.scene_path)
        {
            locations.push(Location {
                uri,
                range: Range::new(Position::new(line as u32, 0), Position::new(line as u32, 0)),
            });
        }
    }

    // Signal declaration references: signal name appears in [connection] sections
    let signal_conns = workspace.signal_connections_for_signal(script_path, target_name);
    for conn in &signal_conns {
        if let Some(line) = find_scene_pattern(
            &conn.scene_path,
            &format!("signal=\"{target_name}\""),
            &format!("signal = \"{target_name}\""),
        ) && let Ok(uri) = Url::from_file_path(&conn.scene_path)
        {
            // Avoid duplicates if both handler and signal match the same line
            let pos = Position::new(line as u32, 0);
            let already_added = locations
                .iter()
                .any(|loc| loc.uri == uri && loc.range.start == pos);
            if !already_added {
                locations.push(Location {
                    uri,
                    range: Range::new(pos, pos),
                });
            }
        }
    }
}

/// Find the 0-based line number of a pattern in a .tscn file.
fn find_scene_pattern(scene_path: &Path, pattern: &str, alt_pattern: &str) -> Option<usize> {
    let content = std::fs::read_to_string(scene_path).ok()?;
    for (line_num, line) in content.lines().enumerate() {
        if line.contains(pattern) || line.contains(alt_pattern) {
            return Some(line_num);
        }
    }
    None
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

        // Skip files that can't contain the identifier (avoids expensive parse)
        if !content.contains(name) {
            continue;
        }

        if let Ok(tree) = crate::core::parser::parse(&content) {
            let root = tree.root_node();
            let Ok(uri) = Url::from_file_path(&path) else {
                continue;
            };

            if let Some(class_name) = class_filter {
                let gd_file = gd_ast::convert(&tree, &content);
                if gd_file.class_name == Some(class_name) {
                    // Whole file is this class — search everything
                    collect_references(root, &content, name, &uri, true, &mut locations);
                } else if let Some(cls) = gd_file.find_class(class_name) {
                    // Inner class — only search inside it
                    collect_references(cls.node, &content, name, &uri, true, &mut locations);
                } else if let Some(enum_node) = find_enum_definition(&gd_file, class_name) {
                    // Enum definition — search members inside it and also
                    // qualified references (EnumName.MEMBER) throughout the file
                    collect_references(enum_node, &content, name, &uri, true, &mut locations);
                    collect_qualified_references(
                        root,
                        &content,
                        class_name,
                        name,
                        &uri,
                        &mut locations,
                    );
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
///
/// Also handles nested qualifications like `Types.State.IDLE` when
/// class_name="State" and name="IDLE" — the qualifier can appear at any
/// position in the attribute chain, not just the first.
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
    //
    // And `Types.State.IDLE` as a nested attribute chain:
    //   attribute {
    //     identifier "Types"
    //     identifier "State"
    //     identifier "IDLE"
    //   }
    if node.kind() == "attribute" {
        let named_children: Vec<_> = {
            let mut cursor = node.walk();
            node.named_children(&mut cursor).collect()
        };

        // Check if any identifier in the chain matches class_name, and the one
        // immediately after it matches target_name. This handles both:
        //   - `ClassName.member` (qualifier is first child)
        //   - `Outer.ClassName.member` (qualifier is a middle child)
        let mut found_qualifier = false;
        for child in &named_children {
            if found_qualifier {
                // The child right after the qualifier — check if it matches target_name
                if child.kind() == "identifier"
                    && child.utf8_text(source.as_bytes()).unwrap_or("") == target_name
                {
                    locations.push(Location {
                        uri: uri.clone(),
                        range: node_range(child),
                    });
                } else if child.kind() == "attribute_call" {
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
                }
                found_qualifier = false;
            }
            if child.kind() == "identifier"
                && child.utf8_text(source.as_bytes()).unwrap_or("") == class_name
            {
                found_qualifier = true;
            }
        }
    }

    // Recurse into children
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_qualified_references(child, source, class_name, target_name, uri, locations);
    }
}

/// Find a top-level `enum_definition` matching `target`.
fn find_enum_definition<'a>(file: &GdFile<'a>, target: &str) -> Option<tree_sitter::Node<'a>> {
    file.declarations.iter().find_map(|d| {
        if let GdDecl::Enum(e) = d
            && e.name == target
        {
            Some(e.node)
        } else {
            None
        }
    })
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

    // ── Static/instance disambiguation tests ─────────────────────────────

    #[test]
    fn static_instance_same_name_rename_instance() {
        // Two declarations with the same name: one static, one instance.
        // Renaming the instance one should only find the instance declaration
        // and calls from instance context.
        let source = "\
class_name Foo

static func do_thing() -> int:
\treturn 0

func do_thing() -> int:
\treturn 1

func caller():
\tdo_thing()

static func static_caller():
\tdo_thing()
";
        let uri = test_uri();
        // Cursor on the instance `do_thing` declaration at line 5, col 5
        let result = find_references(source, &uri, Position::new(5, 5), true);
        assert!(result.is_some());
        let locs = result.unwrap();
        // Should find: instance declaration (line 5) + instance call in caller() (line 9)
        // Should NOT include: static decl (line 2) or static call (line 12)
        let lines: Vec<u32> = locs.iter().map(|l| l.range.start.line).collect();
        assert!(
            lines.contains(&5),
            "should include instance decl at line 5, got {lines:?}"
        );
        assert!(
            lines.contains(&9),
            "should include instance call at line 9, got {lines:?}"
        );
        assert!(
            !lines.contains(&2),
            "should NOT include static decl at line 2, got {lines:?}"
        );
        assert!(
            !lines.contains(&12),
            "should NOT include static call at line 12, got {lines:?}"
        );
        assert_eq!(locs.len(), 2, "expected exactly 2 refs, got {lines:?}");
    }

    #[test]
    fn static_instance_same_name_rename_static() {
        // Renaming the static one should only find the static declaration
        // and calls from static context.
        let source = "\
class_name Foo

static func do_thing() -> int:
\treturn 0

func do_thing() -> int:
\treturn 1

func caller():
\tdo_thing()

static func static_caller():
\tdo_thing()
";
        let uri = test_uri();
        // Cursor on the static `do_thing` declaration at line 2, col 12
        let result = find_references(source, &uri, Position::new(2, 12), true);
        assert!(result.is_some());
        let locs = result.unwrap();
        let lines: Vec<u32> = locs.iter().map(|l| l.range.start.line).collect();
        // Should find: static declaration (line 2) + static call in static_caller() (line 12)
        assert!(
            lines.contains(&2),
            "should include static decl at line 2, got {lines:?}"
        );
        assert!(
            lines.contains(&12),
            "should include static call at line 12, got {lines:?}"
        );
        assert!(
            !lines.contains(&5),
            "should NOT include instance decl at line 5, got {lines:?}"
        );
        assert!(
            !lines.contains(&9),
            "should NOT include instance call at line 9, got {lines:?}"
        );
        assert_eq!(locs.len(), 2, "expected exactly 2 refs, got {lines:?}");
    }

    #[test]
    fn static_instance_same_name_self_call() {
        // `self.do_thing()` should always resolve to the instance variant.
        let source = "\
class_name Foo

static func do_thing() -> int:
\treturn 0

func do_thing() -> int:
\treturn 1

func caller():
\tself.do_thing()
";
        let uri = test_uri();
        // Cursor on instance declaration at line 5
        let result = find_references(source, &uri, Position::new(5, 5), true);
        assert!(result.is_some());
        let locs = result.unwrap();
        let lines: Vec<u32> = locs.iter().map(|l| l.range.start.line).collect();
        assert!(
            lines.contains(&5),
            "should include instance decl, got {lines:?}"
        );
        assert!(
            lines.contains(&9),
            "should include self.do_thing() call, got {lines:?}"
        );
        assert!(
            !lines.contains(&2),
            "should NOT include static decl, got {lines:?}"
        );
    }

    #[test]
    fn static_instance_same_name_class_qualified_call() {
        // `Foo.do_thing()` should always resolve to the static variant.
        let source = "\
class_name Foo

static func do_thing() -> int:
\treturn 0

func do_thing() -> int:
\treturn 1

func caller():
\tFoo.do_thing()
";
        let uri = test_uri();
        // Cursor on static declaration at line 2
        let result = find_references(source, &uri, Position::new(2, 12), true);
        assert!(result.is_some());
        let locs = result.unwrap();
        let lines: Vec<u32> = locs.iter().map(|l| l.range.start.line).collect();
        assert!(
            lines.contains(&2),
            "should include static decl, got {lines:?}"
        );
        assert!(
            lines.contains(&9),
            "should include Foo.do_thing() call, got {lines:?}"
        );
        assert!(
            !lines.contains(&5),
            "should NOT include instance decl, got {lines:?}"
        );
    }

    #[test]
    fn single_function_no_filtering() {
        // When there's only one function with the name, no filtering should apply.
        // This verifies backward compatibility.
        let source = "\
func do_thing() -> int:
\treturn 1

func caller():
\tdo_thing()

static func other_caller():
\tdo_thing()
";
        let uri = test_uri();
        let result = find_references(source, &uri, Position::new(0, 5), true);
        assert!(result.is_some());
        let locs = result.unwrap();
        // All three: declaration + two calls
        assert_eq!(locs.len(), 3, "single-function case should find all refs");
    }

    // ── Enum member tests ─────────────────────────────────────────────────

    #[test]
    fn enum_member_declaration_detected() {
        // Enum member definition should be recognized as a declaration so that
        // include_declaration=false can exclude it.
        let source = "\
enum State { IDLE, RUNNING }

func test():
\tvar s = State.IDLE
";
        let uri = test_uri();
        // Position on `IDLE` in the enum definition (line 0, col 13)
        let with_decl = find_references(source, &uri, Position::new(0, 13), true);
        let without_decl = find_references(source, &uri, Position::new(0, 13), false);
        assert!(with_decl.is_some());
        let with_locs = with_decl.unwrap();
        // Declaration (IDLE in enum) + qualified reference (State.IDLE)
        assert_eq!(with_locs.len(), 2, "include_declaration=true: decl + usage");

        assert!(without_decl.is_some());
        let without_locs = without_decl.unwrap();
        // Only the qualified reference (State.IDLE), not the declaration
        assert_eq!(
            without_locs.len(),
            1,
            "include_declaration=false: only usage"
        );
        assert_eq!(
            without_locs[0].range.start.line, 3,
            "the single ref should be on the usage line"
        );
    }

    #[test]
    fn enum_member_qualified_and_bare_refs() {
        // Both bare and qualified references to an enum member should be found.
        let source = "\
enum Direction { UP, DOWN }

func test():
\tvar x = Direction.UP
\tvar y = UP
\tmatch x:
\t\tDirection.DOWN:
\t\t\tpass
";
        let uri = test_uri();
        // Position on `UP` in the enum definition (line 0, col 17)
        let result = find_references(source, &uri, Position::new(0, 17), true);
        assert!(result.is_some());
        let locs = result.unwrap();
        // Declaration (UP in enum) + Direction.UP + bare UP = 3
        assert_eq!(
            locs.len(),
            3,
            "should find enum member decl + qualified + bare, got {:?}",
            locs.iter()
                .map(|l| (l.range.start.line, l.range.start.character))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn enum_member_with_explicit_value() {
        // Enum members with explicit integer values should also be found.
        let source = "\
enum Priority { LOW = 0, HIGH = 1 }

func test():
\tvar p = Priority.HIGH
";
        let uri = test_uri();
        // Position on `HIGH` in the enum definition
        let result = find_references(source, &uri, Position::new(0, 25), true);
        assert!(result.is_some());
        let locs = result.unwrap();
        assert_eq!(locs.len(), 2, "should find decl + qualified usage");
    }
}
