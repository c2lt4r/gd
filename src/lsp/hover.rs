use tower_lsp::lsp_types::{Hover, HoverContents, MarkupContent, MarkupKind, Position};

use super::builtins::BuiltinMember;
use super::util::{matches_name, node_range, node_text};
use super::workspace::WorkspaceIndex;

/// Provide hover information at the given position.
pub fn hover_at(
    source: &str,
    position: Position,
    workspace: Option<&WorkspaceIndex>,
) -> Option<Hover> {
    let tree = crate::core::parser::parse(source).ok()?;
    let root = tree.root_node();

    // Find the most specific node at the cursor position
    let point = tree_sitter::Point::new(position.line as usize, position.character as usize);
    let node = root.descendant_for_point_range(point, point)?;

    // Walk up the tree to find a meaningful declaration node
    let mut current = node;
    loop {
        match current.kind() {
            "function_definition" => {
                // Show hover for the function's name or keyword, not for
                // arbitrary identifiers inside the function body.
                if is_declaration_name(&current, &node)
                    || is_on_declaration_keyword(&current, point)
                {
                    return hover_function(&current, source);
                }
                return None;
            }
            "variable_statement" => {
                if is_declaration_name(&current, &node)
                    || is_on_declaration_keyword(&current, point)
                {
                    return Some(hover_variable(&current, source));
                }
                return None;
            }
            "const_statement" => {
                if is_declaration_name(&current, &node)
                    || is_on_declaration_keyword(&current, point)
                {
                    return Some(hover_const(&current, source));
                }
                return None;
            }
            "signal_statement" => {
                if is_declaration_name(&current, &node)
                    || is_on_declaration_keyword(&current, point)
                {
                    return Some(hover_signal(&current, source));
                }
                return None;
            }
            "class_name_statement" => return Some(hover_class_name(&current, source)),
            "class_definition" => {
                if is_declaration_name(&current, &node)
                    || is_on_declaration_keyword(&current, point)
                {
                    return hover_class(&current, source);
                }
                return None;
            }
            "enum_definition" => {
                if is_declaration_name(&current, &node)
                    || is_on_declaration_keyword(&current, point)
                {
                    return hover_enum(&current, source);
                }
                return None;
            }
            "identifier" | "name" => {
                return resolve_hover_for_identifier(&current, &root, source, position, workspace);
            }
            _ => {}
        }
        current = current.parent()?;
    }
}

/// Check if `node` is (or is an ancestor of) the `name` field of `decl_node`.
fn is_declaration_name(decl_node: &tree_sitter::Node, node: &tree_sitter::Node) -> bool {
    if let Some(name_node) = decl_node.child_by_field_name("name") {
        // The cursor node is at or inside the declaration's name
        node.start_byte() >= name_node.start_byte() && node.end_byte() <= name_node.end_byte()
    } else {
        false
    }
}

/// Check if the cursor is on the keyword/spacing before the declaration's name.
/// Extends hover coverage so that `const MOVE_SPEED` is hoverable from column 1.
fn is_on_declaration_keyword(decl_node: &tree_sitter::Node, point: tree_sitter::Point) -> bool {
    if let Some(name_node) = decl_node.child_by_field_name("name") {
        point.row == name_node.start_position().row
            && point.column < name_node.start_position().column
    } else {
        false
    }
}

/// Resolve hover for an identifier/name node: declarations, builtins, enum members.
fn resolve_hover_for_identifier(
    current: &tree_sitter::Node,
    root: &tree_sitter::Node,
    source: &str,
    position: Position,
    workspace: Option<&WorkspaceIndex>,
) -> Option<Hover> {
    let name = node_text(current, source);

    // 1. FIRST: check if this is a member access (foo.bar) — must come before
    //    same-file declaration lookup to avoid name collisions (e.g. hovering on
    //    `_turn_manager.submit_action` should NOT resolve to the local submit_action).
    if let Some(hover) = try_member_hover(current, root, source, position, workspace) {
        return Some(hover);
    }
    // 2. If we're the object side of a dot (EventBus in EventBus.foo), try workspace/autoload
    if let Some(hover) = try_receiver_hover(current, name, workspace) {
        return Some(hover);
    }
    // 3. Try to resolve to a same-file declaration
    if let Some(hover) = resolve_identifier(root, name, source) {
        return Some(hover);
    }
    // 4. Try builtin type
    if let Some(doc) = super::builtins::lookup_type(name) {
        return Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: super::builtins::format_type_hover(&doc),
            }),
            range: Some(node_range(current)),
        });
    }
    // 5. Try builtin function
    if let Some(doc) = super::builtins::lookup_function(name) {
        return Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: super::builtins::format_function_hover(&doc),
            }),
            range: Some(node_range(current)),
        });
    }
    // 6. GDScript language keywords (preload, load — not in ClassDB utility functions)
    if let Some(hover) = hover_gdscript_keyword(name, current) {
        return Some(hover);
    }
    // 7. Try builtin/ClassDB member as standalone identifier (GDScript allows
    //    inherited members without self prefix: velocity, move_and_slide, etc.)
    //    Priority: class-specific builtin → ClassDB property/method → generic builtin.
    if let Some(doc) = resolve_builtin_member_for_file(root, source, name) {
        return Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: super::builtins::format_member_hover(doc),
            }),
            range: Some(node_range(current)),
        });
    }
    if let Some(hover) = resolve_classdb_member_for_file(root, source, name, current) {
        return Some(hover);
    }
    if let Some(doc) = super::builtins::lookup_member(name) {
        return Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: super::builtins::format_member_hover(doc),
            }),
            range: Some(node_range(current)),
        });
    }
    // 8. Check if this is an enum member (HOUSE inside enum { HOUSE, MART })
    if let Some(parent) = current.parent()
        && parent.kind() == "enumerator"
    {
        return hover_enum_member(&parent, source);
    }
    // 8. Check if this identifier refers to an enum member in the same file
    if let Some(hover) = resolve_enum_member(root, name, source) {
        return Some(hover);
    }
    // 9. Global enum constant used as bare identifier (OK, KEY_ESCAPE, etc.)
    if let Some(ev) = crate::class_db::enum_value_doc("@GlobalScope", name) {
        return Some(make_hover(
            &format_enum_value(ev),
            current,
            if ev.doc.is_empty() { None } else { Some(ev.doc) },
        ));
    }
    // 10. Engine class/singleton used as bare identifier (Input, OS, Engine, etc.)
    if crate::class_db::class_exists(name) {
        let code = format!("class {name}");
        let doc = crate::class_db::class_doc(name);
        return Some(make_hover(&code, current, doc));
    }
    // 10. Workspace class_name (user-defined class referenced as bare identifier,
    //     e.g. `extends KartEffect` or `VehicleData.get_vehicle()`)
    if let Some(ws) = workspace
        && let Some(path) = ws.lookup_class_name(name)
        && let Some(content) = ws.get_content(&path)
        && let Ok(tree) = crate::core::parser::parse(&content)
    {
        let extends =
            super::completion::find_extends_class(tree.root_node(), &content).unwrap_or_default();
        let code = if extends.is_empty() {
            format!("class_name {name}")
        } else {
            format!("class_name {name} extends {extends}")
        };
        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let doc = extract_doc_comment_from_root(&tree.root_node(), &content);
        let origin = format!("*{file_name}*");
        let doc_text = match doc {
            Some(d) => format!("{origin}\n\n{d}"),
            None => origin,
        };
        return Some(make_hover(&code, current, Some(&doc_text)));
    }
    // 11. Nothing found
    None
}

/// Hover for the object side of a dot-access: `EventBus` in `EventBus.foo`,
/// `MapBuilder` in `MapBuilder.create_tileset`.
fn try_receiver_hover(
    ident_node: &tree_sitter::Node,
    name: &str,
    workspace: Option<&WorkspaceIndex>,
) -> Option<Hover> {
    // Only trigger when this identifier is the left side of an attribute node
    let parent = ident_node.parent()?;
    if parent.kind() != "attribute" {
        return None;
    }
    let object_node = parent.child(0)?;
    if ident_node.id() != object_node.id() {
        return None; // We're the member side, not the object
    }

    let ws = workspace?;

    // Autoload singleton
    if let Some(info) = ws.lookup_autoload(name) {
        let extends = info.class_name.as_deref().unwrap_or("(autoload)");
        let code = format!("{name}: {extends}");
        return Some(make_hover(&code, ident_node, Some("Autoload singleton")));
    }

    // Workspace class_name
    if let Some(path) = ws.lookup_class_name(name)
        && let Some(content) = ws.get_content(&path)
        && let Ok(tree) = crate::core::parser::parse(&content)
    {
        let extends =
            super::completion::find_extends_class(tree.root_node(), &content).unwrap_or_default();
        let code = if extends.is_empty() {
            format!("class_name {name}")
        } else {
            format!("class_name {name} extends {extends}")
        };
        return Some(make_hover(&code, ident_node, None));
    }

    None
}

/// Try to resolve a member access pattern (foo.bar or self.bar).
///
/// tree-sitter structures:
/// - Property access: `attribute` > [`obj`, `.`, `member`]
/// - Method call: `attribute` > [`obj`, `attribute_call` > [`method`, `arguments`]]
fn try_member_hover(
    ident_node: &tree_sitter::Node,
    root: &tree_sitter::Node,
    source: &str,
    position: Position,
    workspace: Option<&WorkspaceIndex>,
) -> Option<Hover> {
    let parent = ident_node.parent()?;

    // Determine the attribute node and object node based on context
    let attr_node = match parent.kind() {
        // Direct property access: parent is the `attribute` node
        "attribute" => parent,
        // Method call: parent is `attribute_call`, grandparent is `attribute`
        "attribute_call" => {
            let grandparent = parent.parent()?;
            if grandparent.kind() == "attribute" {
                grandparent
            } else {
                return None;
            }
        }
        _ => return None,
    };

    let member_name = node_text(ident_node, source);

    // Check if this identifier is the member (right side), not the object (left side).
    let object_node = attr_node.child(0)?;
    if ident_node.id() == object_node.id() {
        return None;
    }

    // Handle self.member — resolve to same-file declarations first
    let object_text = node_text(&object_node, source);
    if object_text == "self"
        && let Some(hover) = resolve_identifier(root, member_name, source)
    {
        return Some(hover);
    }

    // Resolve the receiver's type using the completion resolver chain.
    // Build the full receiver text (handles chains like `self.velocity`).
    let receiver_text = build_receiver_text(&object_node, source);
    if let Some(receiver_type) =
        resolve_receiver_for_hover(&receiver_text, source, position, workspace)
        && let Some(hover) =
            hover_member_on_type(&receiver_type, member_name, ident_node, workspace)
    {
        return Some(hover);
    }

    // Last resort: bare builtin member lookup (e.g. unknown receiver but known method name)
    if let Some(doc) = super::builtins::lookup_member(member_name) {
        return Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: super::builtins::format_member_hover(doc),
            }),
            range: Some(node_range(ident_node)),
        });
    }

    None
}

/// Build the full text of a receiver node, including chained attribute access.
/// E.g. for `self.velocity`, the object_node is `self` → returns `"self"`.
/// For `self.velocity.normalized`, the object_node is an attribute → returns `"self.velocity"`.
fn build_receiver_text(node: &tree_sitter::Node, source: &str) -> String {
    if node.kind() == "attribute" {
        // Reconstruct dotted chain
        let text = node.utf8_text(source.as_bytes()).unwrap_or("unknown");
        text.to_string()
    } else {
        node_text(node, source).to_string()
    }
}

/// Resolve a receiver string to its type name using the completion module's resolver.
fn resolve_receiver_for_hover(
    receiver: &str,
    source: &str,
    position: Position,
    workspace: Option<&WorkspaceIndex>,
) -> Option<String> {
    use super::completion::{ResolvedReceiver, resolve_receiver_type};
    match resolve_receiver_type(receiver, source, position, workspace)? {
        ResolvedReceiver::ClassName(name) => Some(name),
        ResolvedReceiver::WorkspaceEnum { .. } => None, // Enum members don't have sub-members
    }
}

/// Provide hover for a member on a resolved type.
/// Checks: workspace user-defined class → ClassDB → builtin members.
fn hover_member_on_type(
    class: &str,
    member: &str,
    ident_node: &tree_sitter::Node,
    workspace: Option<&WorkspaceIndex>,
) -> Option<Hover> {
    // 1. User-defined class from workspace — find function/var/signal declaration
    if let Some(ws) = workspace {
        let content = ws
            .lookup_class_name(class)
            .and_then(|path| ws.get_content(&path))
            .or_else(|| ws.autoload_content(class));
        if let Some(content) = content
            && let Ok(tree) = crate::core::parser::parse(&content)
            && let Some(mut hover) = resolve_identifier(&tree.root_node(), member, &content)
        {
            // Annotate with origin class
            if let HoverContents::Markup(ref mut markup) = hover.contents {
                markup.value = format!("{}\n\n*{}*", markup.value, class);
            }
            return Some(hover);
        }
    }

    // Resolve the ClassDB class name — if `class` is a user-defined name
    // (autoload/class_name), find its `extends` chain for ClassDB lookup.
    let db_class = if crate::class_db::class_exists(class) {
        Some(class.to_string())
    } else if let Some(ws) = workspace {
        let content = ws
            .lookup_class_name(class)
            .and_then(|path| ws.get_content(&path))
            .or_else(|| ws.autoload_content(class));
        content.and_then(|c| {
            let tree = crate::core::parser::parse(&c).ok()?;
            super::completion::find_extends_class(tree.root_node(), &c)
        })
    } else {
        None
    };

    let db_class = db_class.as_deref().unwrap_or(class);

    // 2. ClassDB property
    for (name, prop_type, owner_class) in crate::class_db::class_properties(db_class) {
        if name == member {
            let code = format!("{prop_type} {owner_class}.{name}");
            let doc = crate::class_db::property_doc(db_class, name);
            return Some(make_hover(&code, ident_node, doc));
        }
    }

    // 3. ClassDB method
    if let Some(ret) = crate::class_db::method_return_type(db_class, member) {
        let owner = find_method_owner(db_class, member).unwrap_or(db_class);
        let code = format!("{owner}.{member}() -> {ret}");
        let doc = crate::class_db::method_doc(db_class, member);
        return Some(make_hover(&code, ident_node, doc));
    }

    // 4. Builtin members (Vector2, String, Array, etc.) — walk inheritance
    let mut cur = db_class;
    loop {
        if let Some(doc) = super::builtins::lookup_member_for(cur, member) {
            return Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: super::builtins::format_member_hover(doc),
                }),
                range: Some(node_range(ident_node)),
            });
        }
        match crate::class_db::parent_class(cur) {
            Some(parent) => cur = parent,
            None => break,
        }
    }

    // 5. ClassDB enum constant (e.g. Object.CONNECT_DEFERRED)
    if let Some(ev) = crate::class_db::enum_value_doc(db_class, member) {
        return Some(make_hover(
            &format_enum_value(ev),
            ident_node,
            if ev.doc.is_empty() { None } else { Some(ev.doc) },
        ));
    }

    // 6. ClassDB enum type (e.g. Viewport.MSAA) — show all values
    if crate::class_db::enum_type_exists(db_class, member) {
        let values = crate::class_db::enum_values(db_class, member);
        if !values.is_empty() {
            let code = format!("enum {db_class}.{member}");
            let listing = format_enum_listing(values);
            return Some(make_hover(&code, ident_node, Some(&listing)));
        }
    }

    None
}

/// Find which class in the hierarchy actually defines a method.
fn find_method_owner<'a>(class: &str, method: &str) -> Option<&'a str> {
    for (name, _, owner) in crate::class_db::class_methods(class) {
        if name == method {
            return Some(owner);
        }
    }
    None
}

/// Extract `##` doc comment lines preceding a declaration node.
/// Walks backward through named siblings, skipping annotation nodes.
fn extract_doc_comment(decl_node: &tree_sitter::Node, source: &str) -> Option<String> {
    let bytes = source.as_bytes();
    let mut lines = Vec::new();
    let mut current = decl_node.prev_named_sibling();

    while let Some(prev) = current {
        match prev.kind() {
            "comment" => {
                if let Ok(text) = prev.utf8_text(bytes) {
                    if let Some(stripped) = text.strip_prefix("##") {
                        lines.push(stripped.trim().to_string());
                    } else {
                        break; // Regular `#` comment breaks the chain
                    }
                }
            }
            "annotation" | "annotations" => {
                // Annotations can appear between doc comments and declarations
            }
            _ => break,
        }
        current = prev.prev_named_sibling();
    }

    if lines.is_empty() {
        None
    } else {
        lines.reverse();
        Some(lines.join("\n"))
    }
}

fn hover_function(node: &tree_sitter::Node, source: &str) -> Option<Hover> {
    let name = node_text(&node.child_by_field_name("name")?, source);

    let mut sig = String::from("func ");
    sig.push_str(name);

    if let Some(params) = node.child_by_field_name("parameters") {
        sig.push_str(node_text(&params, source));
    } else {
        sig.push_str("()");
    }

    if let Some(ret) = node.child_by_field_name("return_type") {
        sig.push_str(" -> ");
        sig.push_str(node_text(&ret, source));
    }

    let doc = extract_doc_comment(node, source);
    Some(make_hover(&sig, node, doc.as_deref()))
}

fn hover_variable(node: &tree_sitter::Node, source: &str) -> Hover {
    let text = node_text(node, source);
    let decl = text.lines().next().unwrap_or(text).trim_end();
    let doc = extract_doc_comment(node, source);
    make_hover(decl, node, doc.as_deref())
}

fn hover_const(node: &tree_sitter::Node, source: &str) -> Hover {
    let text = node_text(node, source);
    let decl = text.lines().next().unwrap_or(text).trim_end();
    let doc = extract_doc_comment(node, source);
    make_hover(decl, node, doc.as_deref())
}

fn hover_signal(node: &tree_sitter::Node, source: &str) -> Hover {
    let text = node_text(node, source);
    let decl = text.lines().next().unwrap_or(text).trim_end();
    let doc = extract_doc_comment(node, source);
    make_hover(decl, node, doc.as_deref())
}

fn hover_class_name(node: &tree_sitter::Node, source: &str) -> Hover {
    let text = node_text(node, source);
    let doc = extract_doc_comment(node, source);
    make_hover(text.trim(), node, doc.as_deref())
}

fn hover_class(node: &tree_sitter::Node, source: &str) -> Option<Hover> {
    let name = node_text(&node.child_by_field_name("name")?, source);
    let decl = format!("class {name}");
    let doc = extract_doc_comment(node, source);
    Some(make_hover(&decl, node, doc.as_deref()))
}

fn hover_enum(node: &tree_sitter::Node, source: &str) -> Option<Hover> {
    let name_node = node.child_by_field_name("name")?;
    let text = node_text(node, source);
    let doc = extract_doc_comment(node, source);
    Some(make_hover(text.trim(), &name_node, doc.as_deref()))
}

fn hover_enum_member(member_node: &tree_sitter::Node, source: &str) -> Option<Hover> {
    let name_node = member_node.child_by_field_name("left")?;
    let member_name = node_text(&name_node, source);

    // Walk up: enumerator → enumerator_list → enum_definition
    let enum_list = member_node.parent()?;
    let enum_def = enum_list.parent()?;
    if enum_def.kind() != "enum_definition" {
        return None;
    }

    let enum_name = enum_def
        .child_by_field_name("name")
        .map(|n| node_text(&n, source));
    let value = compute_enum_member_value(member_node, source);

    let code = match enum_name {
        Some(name) => format!("{name}.{member_name} = {value}"),
        None => format!("{member_name} = {value}"),
    };

    let doc = extract_doc_comment(member_node, source);
    Some(make_hover(&code, &name_node, doc.as_deref()))
}

/// Compute the integer value of an enum member, tracking explicit assignments.
fn compute_enum_member_value(target: &tree_sitter::Node, source: &str) -> String {
    let Some(list) = target.parent() else {
        return "0".to_string();
    };

    let mut cursor = list.walk();
    let mut next_value: i64 = 0;

    for child in list.children(&mut cursor) {
        if child.kind() != "enumerator" {
            continue;
        }
        if let Some(val_node) = child.child_by_field_name("right") {
            let val_text = node_text(&val_node, source).trim().to_string();
            if child.id() == target.id() {
                return val_text;
            }
            if let Ok(v) = val_text.parse::<i64>() {
                next_value = v + 1;
            } else {
                next_value += 1;
            }
        } else {
            if child.id() == target.id() {
                return next_value.to_string();
            }
            next_value += 1;
        }
    }

    "0".to_string()
}

/// Search enum definitions for a member matching `name`.
fn resolve_enum_member(root: &tree_sitter::Node, name: &str, source: &str) -> Option<Hover> {
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        if child.kind() == "enum_definition"
            && let Some(body) = child.child_by_field_name("body")
        {
            let mut body_cursor = body.walk();
            for member in body.children(&mut body_cursor) {
                if member.kind() == "enumerator"
                    && let Some(left) = member.child_by_field_name("left")
                    && node_text(&left, source) == name
                {
                    return hover_enum_member(&member, source);
                }
            }
        }
    }
    None
}

/// Try to resolve an identifier to a top-level declaration in the file.
fn resolve_identifier(root: &tree_sitter::Node, name: &str, source: &str) -> Option<Hover> {
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        match child.kind() {
            "function_definition" => {
                if matches_name(&child, name, source) {
                    return hover_function(&child, source);
                }
            }
            "variable_statement" => {
                if matches_name(&child, name, source) {
                    return Some(hover_variable(&child, source));
                }
            }
            "const_statement" => {
                if matches_name(&child, name, source) {
                    return Some(hover_const(&child, source));
                }
            }
            "signal_statement" => {
                if matches_name(&child, name, source) {
                    return Some(hover_signal(&child, source));
                }
            }
            "class_definition" => {
                if matches_name(&child, name, source) {
                    return hover_class(&child, source);
                }
            }
            "enum_definition" => {
                if matches_name(&child, name, source) {
                    return hover_enum(&child, source);
                }
            }
            _ => {}
        }
    }
    None
}

/// Extract the `##` doc comment at the top of a file (before class_name or first declaration).
fn extract_doc_comment_from_root(root: &tree_sitter::Node, source: &str) -> Option<String> {
    let bytes = source.as_bytes();
    let mut lines = Vec::new();
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        match child.kind() {
            "comment" => {
                if let Ok(text) = child.utf8_text(bytes) {
                    if let Some(stripped) = text.strip_prefix("##") {
                        lines.push(stripped.trim().to_string());
                    } else {
                        break;
                    }
                }
            }
            _ => break,
        }
    }
    if lines.is_empty() {
        None
    } else {
        Some(lines.join("\n"))
    }
}

/// Hover for GDScript language built-in keywords that aren't in ClassDB.
fn hover_gdscript_keyword(name: &str, node: &tree_sitter::Node) -> Option<Hover> {
    let (code, doc) = match name {
        "preload" => (
            "preload(path: String) -> Resource",
            "Loads a resource from the filesystem at compile time. The path must be a constant string literal (not a variable). Returns a `Resource` subtype matching the file: `.tscn` → `PackedScene`, `.gd` → `GDScript`, `.png` → `CompressedTexture2D`, etc.\n\nUnlike `load()`, the resource is embedded into the compiled script, so there is no filesystem access at runtime.",
        ),
        "load" => (
            "load(path: String) -> Resource",
            "Loads a resource from the filesystem at runtime. Equivalent to `ResourceLoader.load()`. Returns `null` if the resource cannot be found.\n\nPrefer `preload()` when the path is known at compile time for better performance.",
        ),
        _ => return None,
    };
    Some(make_hover(code, node, Some(doc)))
}

/// Try to resolve a bare identifier as a ClassDB property or method using the
/// file's extends class. Covers engine properties like `rotation` on Node3D.
fn resolve_classdb_member_for_file(
    root: &tree_sitter::Node,
    source: &str,
    name: &str,
    ident_node: &tree_sitter::Node,
) -> Option<Hover> {
    let extends = super::completion::find_extends_class(*root, source)?;
    // Try property
    for (prop_name, prop_type, owner_class) in crate::class_db::class_properties(&extends) {
        if prop_name == name {
            let code = format!("{prop_type} {owner_class}.{prop_name}");
            let doc = crate::class_db::property_doc(&extends, prop_name);
            return Some(make_hover(&code, ident_node, doc));
        }
    }
    // Try method
    if let Some(ret) = crate::class_db::method_return_type(&extends, name) {
        let owner = find_method_owner(&extends, name).unwrap_or(&extends);
        let code = format!("{owner}.{name}() -> {ret}");
        let doc = crate::class_db::method_doc(&extends, name);
        return Some(make_hover(&code, ident_node, doc));
    }
    None
}

/// Try to resolve a builtin member using the file's extends class, walking the
/// ClassDB inheritance chain. Returns the class-specific member doc if found.
fn resolve_builtin_member_for_file<'a>(
    root: &tree_sitter::Node,
    source: &str,
    name: &str,
) -> Option<&'a BuiltinMember> {
    let extends = super::completion::find_extends_class(*root, source)?;
    let mut current: &str = &extends;
    loop {
        if let Some(doc) = super::builtins::lookup_member_for(current, name) {
            return Some(doc);
        }
        match crate::class_db::parent_class(current) {
            Some(parent) => current = parent,
            None => return None,
        }
    }
}

/// Format a single enum value for hover display.
fn format_enum_value(ev: &crate::class_db::generated::EnumValue) -> String {
    format!("{}.{} = {}", ev.enum_name, ev.name, ev.value)
}

/// Format a listing of all enum values for hover display.
fn format_enum_listing(values: &[crate::class_db::generated::EnumValue]) -> String {
    let mut lines = Vec::new();
    for v in values {
        if v.doc.is_empty() {
            lines.push(format!("- `{}` = {}", v.name, v.value));
        } else {
            // Take first sentence of doc for the listing
            let brief = v.doc.split('\n').next().unwrap_or(v.doc);
            lines.push(format!("- `{}` = {} — {brief}", v.name, v.value));
        }
    }
    lines.join("\n")
}

fn make_hover(code: &str, node: &tree_sitter::Node, doc: Option<&str>) -> Hover {
    let value = match doc {
        Some(d) => format!("```gdscript\n{code}\n```\n\n{d}"),
        None => format!("```gdscript\n{code}\n```"),
    };
    Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value,
        }),
        range: Some(node_range(node)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hover_value(source: &str, line: u32, character: u32) -> Option<String> {
        hover_at(source, Position::new(line, character), None).map(|h| match h.contents {
            HoverContents::Markup(m) => m.value,
            _ => String::new(),
        })
    }

    #[test]
    fn hover_documented_function() {
        let source = "## Move the player.\nfunc move() -> void:\n\tpass\n";
        let val = hover_value(source, 1, 5).unwrap();
        assert!(val.contains("func move() -> void"));
        assert!(val.contains("Move the player."));
    }

    #[test]
    fn hover_undocumented_function() {
        let source = "func move() -> void:\n\tpass\n";
        let val = hover_value(source, 0, 5).unwrap();
        assert!(val.contains("func move() -> void"));
        assert!(!val.contains("\n\n"));
    }

    #[test]
    fn hover_documented_variable() {
        let source = "## The player's health.\nvar health: int = 100\n";
        let val = hover_value(source, 1, 4).unwrap();
        assert!(val.contains("var health: int = 100"));
        assert!(val.contains("The player's health."));
    }

    #[test]
    fn hover_multiline_doc() {
        let source = "## Line one.\n## Line two.\nfunc f():\n\tpass\n";
        let val = hover_value(source, 2, 5).unwrap();
        assert!(val.contains("Line one.\nLine two."));
    }

    #[test]
    fn hover_doc_with_annotation() {
        let source = "## Exported health.\n@export\nvar health: int = 100\n";
        let val = hover_value(source, 2, 4).unwrap();
        assert!(val.contains("Exported health."));
    }

    #[test]
    fn hover_resolved_identifier_with_doc() {
        let source = "## The speed value.\nvar speed: float = 5.0\n\nfunc f():\n\tprint(speed)\n";
        let val = hover_value(source, 4, 7).unwrap();
        assert!(val.contains("The speed value."));
    }

    // ── Fix 1: column sensitivity ────────────────────────────────────

    #[test]
    fn hover_const_on_keyword() {
        // Hovering on `const` keyword should show the const declaration
        let source = "const MOVE_SPEED = 100\n";
        let val = hover_value(source, 0, 0).unwrap(); // col 0 = 'c' in 'const'
        assert!(val.contains("const MOVE_SPEED = 100"));
    }

    #[test]
    fn hover_const_on_space_before_name() {
        // Hovering on space between keyword and name should work
        let source = "const MOVE_SPEED = 100\n";
        let val = hover_value(source, 0, 5).unwrap(); // col 5 = space before MOVE_SPEED
        assert!(val.contains("const MOVE_SPEED = 100"));
    }

    #[test]
    fn hover_var_on_keyword() {
        let source = "var health: int = 100\n";
        let val = hover_value(source, 0, 0).unwrap();
        assert!(val.contains("var health: int = 100"));
    }

    #[test]
    fn hover_func_on_keyword() {
        let source = "func move():\n\tpass\n";
        let val = hover_value(source, 0, 0).unwrap(); // col 0 = 'f' in 'func'
        assert!(val.contains("func move()"));
    }

    #[test]
    fn hover_signal_on_keyword() {
        let source = "signal health_changed(amount: int)\n";
        let val = hover_value(source, 0, 0).unwrap();
        assert!(val.contains("signal health_changed"));
    }

    #[test]
    fn hover_const_value_no_trigger() {
        // Hovering on the value (after =) should NOT show the const
        let source = "const MOVE_SPEED = 100\n";
        assert!(hover_value(source, 0, 20).is_none()); // col 20 = inside '100'
    }

    // ── Fix 2: enum members ─────────────────────────────────────────

    #[test]
    fn hover_enum_member_inline() {
        let source = "enum Color { RED, GREEN, BLUE }\n";
        let val = hover_value(source, 0, 13).unwrap(); // col 13 = 'R' in RED
        assert!(val.contains("Color.RED = 0"));
    }

    #[test]
    fn hover_enum_member_with_value() {
        let source = "enum Flags { A = 1, B = 5, C }\n";
        let val = hover_value(source, 0, 20).unwrap(); // col 20 = 'B'
        assert!(val.contains("Flags.B = 5"));
    }

    #[test]
    fn hover_enum_member_implicit_after_explicit() {
        let source = "enum Flags { A = 10, B, C }\n";
        let val = hover_value(source, 0, 21).unwrap(); // col 21 = 'B'
        assert!(val.contains("Flags.B = 11"));
    }

    #[test]
    fn hover_enum_member_multiline() {
        let source = "enum Dir {\n\tUP,\n\tDOWN,\n\tLEFT,\n\tRIGHT,\n}\n";
        let val = hover_value(source, 3, 1).unwrap(); // line 3, col 1 = LEFT
        assert!(val.contains("Dir.LEFT = 2"));
    }

    #[test]
    fn hover_enum_member_anonymous() {
        let source = "enum { X, Y, Z }\n";
        let val = hover_value(source, 0, 7).unwrap(); // col 7 = 'X'
        assert!(val.contains("X = 0"));
        assert!(!val.contains('.')); // no enum name prefix
    }

    #[test]
    fn hover_enum_member_reference() {
        // Using enum member name elsewhere in the file resolves to the declaration
        let source = "enum Color { RED, GREEN, BLUE }\nfunc f():\n\tvar c = RED\n";
        let val = hover_value(source, 2, 9).unwrap(); // col 9 = 'R' in RED usage
        assert!(val.contains("Color.RED = 0"));
    }

    // ── Dot-access hover with receiver resolution ────────────────────

    #[test]
    fn hover_typed_var_dot_member() {
        // s.texture should resolve s: Sprite2D, then show ClassDB property
        let source = "extends Node\nvar s: Sprite2D\nfunc run():\n\ts.texture\n";
        let val = hover_value(source, 3, 3).unwrap(); // col 3 = 't' in texture
        assert!(
            val.contains("texture"),
            "should hover Sprite2D.texture, got: {val}"
        );
    }

    #[test]
    fn hover_classdb_dot_method() {
        // self.add_child should resolve to Node.add_child()
        let source = "extends Node2D\nfunc run():\n\tself.add_child\n";
        let val = hover_value(source, 2, 6).unwrap(); // col 6 = 'a' in add_child
        assert!(
            val.contains("add_child"),
            "should hover Node.add_child, got: {val}"
        );
    }

    #[test]
    fn hover_self_velocity_classdb() {
        // self.velocity should resolve to CharacterBody2D property
        let source = "extends CharacterBody2D\nfunc run():\n\tself.velocity\n";
        let val = hover_value(source, 2, 6).unwrap(); // col 6 = 'v' in velocity
        assert!(
            val.contains("velocity"),
            "should hover CharacterBody2D.velocity, got: {val}"
        );
    }

    #[test]
    fn hover_builtin_type_dot_member() {
        // Vector2.normalized should show builtin member doc
        let source = "extends Node\nfunc run():\n\tvar v := Vector2(1, 2)\n\tv.normalized\n";
        let val = hover_value(source, 3, 3).unwrap(); // col 3 = 'n' in normalized
        assert!(
            val.contains("normalized"),
            "should hover Vector2.normalized, got: {val}"
        );
    }

    #[test]
    fn hover_member_name_collision() {
        // Hovering on `obj.foo` where `foo` also exists in the current file
        // should resolve to the receiver's type, NOT the local declaration.
        let source = "extends Node\nvar s: Sprite2D\nfunc set_texture():\n\tpass\nfunc run():\n\ts.set_texture\n";
        let val = hover_value(source, 5, 3).unwrap(); // col 3 = 's' in set_texture
        // Should show Sprite2D.set_texture from ClassDB, not our local func
        assert!(
            val.contains("Sprite2D") || val.contains("CanvasItem"),
            "should resolve to Sprite2D's set_texture, not local — got: {val}"
        );
    }

    #[test]
    fn hover_inferred_var_dot_member() {
        // rng.seed should resolve rng := RandomNumberGenerator.new() → RNG property
        let source =
            "extends Node\nfunc run():\n\tvar rng := RandomNumberGenerator.new()\n\trng.seed\n";
        let val = hover_value(source, 3, 5).unwrap(); // col 5 = 's' in seed
        assert!(
            val.contains("seed") && !val.contains("func seed"),
            "should hover RandomNumberGenerator.seed property, got: {val}"
        );
    }

    #[test]
    fn hover_method_call_on_typed_var() {
        // s.get_rect() — hovering on get_rect should resolve via attribute_call
        let source = "extends Node\nvar s: Sprite2D\nfunc run():\n\ts.get_rect()\n";
        let val = hover_value(source, 3, 3).unwrap(); // col 3 = 'g' in get_rect
        assert!(
            val.contains("get_rect"),
            "should hover Sprite2D.get_rect method call, got: {val}"
        );
    }

    #[test]
    fn hover_method_call_name_collision() {
        // obj.add_child(x) where add_child also exists locally — should resolve to Node's
        let source = "extends Node\nvar n: Node2D\nfunc add_child():\n\tpass\nfunc run():\n\tn.add_child(null)\n";
        let val = hover_value(source, 5, 3).unwrap(); // col 3 = 'a' in add_child
        assert!(
            val.contains("Node.add_child"),
            "should resolve to Node's add_child, not local — got: {val}"
        );
    }

    #[test]
    fn hover_engine_class_bare() {
        // Hovering on `Input` as a bare identifier should show class info
        let source = "extends Node\nfunc run():\n\tInput\n";
        let val = hover_value(source, 2, 1).unwrap();
        assert!(
            val.contains("class Input"),
            "should hover engine class Input, got: {val}"
        );
    }

    #[test]
    fn hover_signal_member_emit() {
        // self.my_signal.emit — hover on emit should show Signal.emit
        let source =
            "extends Node\nsignal my_signal(value: int)\nfunc run():\n\tself.my_signal.emit\n";
        let val = hover_value(source, 3, 16).unwrap(); // col 16 = 'e' in emit
        assert!(val.contains("emit"), "should hover Signal.emit, got: {val}");
    }
}
