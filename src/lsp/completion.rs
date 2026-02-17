use std::sync::Arc;

use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, Documentation, InsertTextFormat, MarkupContent, MarkupKind,
    Position,
};

use super::workspace::WorkspaceIndex;

/// GDScript keywords.
const KEYWORDS: &[&str] = &[
    "func",
    "var",
    "const",
    "signal",
    "class",
    "extends",
    "if",
    "elif",
    "else",
    "for",
    "while",
    "match",
    "return",
    "break",
    "continue",
    "pass",
    "await",
    "yield",
    "self",
    "super",
    "true",
    "false",
    "null",
    "void",
    "preload",
    "load",
    "export",
    "onready",
    "static",
    "class_name",
    "tool",
    "enum",
];

/// GDScript built-in types.
const BUILTIN_TYPES: &[&str] = &[
    "int",
    "float",
    "bool",
    "String",
    "Vector2",
    "Vector3",
    "Vector4",
    "Vector2i",
    "Vector3i",
    "Vector4i",
    "Array",
    "Dictionary",
    "NodePath",
    "StringName",
    "Color",
    "Rect2",
    "Rect2i",
    "Transform2D",
    "Transform3D",
    "Basis",
    "AABB",
    "Plane",
    "Quaternion",
    "Projection",
    "RID",
    "Callable",
    "Signal",
    "PackedByteArray",
    "PackedInt32Array",
    "PackedInt64Array",
    "PackedFloat32Array",
    "PackedFloat64Array",
    "PackedStringArray",
    "PackedColorArray",
    "PackedVector2Array",
    "PackedVector3Array",
    "PackedVector4Array",
];

/// GDScript built-in functions.
const BUILTIN_FUNCTIONS: &[&str] = &[
    "print",
    "prints",
    "printt",
    "printerr",
    "push_error",
    "push_warning",
    "str",
    "int",
    "float",
    "bool",
    "len",
    "range",
    "typeof",
    "is_instance_of",
    "abs",
    "sign",
    "min",
    "max",
    "clamp",
    "lerp",
    "smoothstep",
    "sqrt",
    "pow",
    "sin",
    "cos",
    "tan",
    "floor",
    "ceil",
    "round",
    "randi",
    "randf",
    "randomize",
    "seed",
    "hash",
    "is_equal_approx",
    "is_zero_approx",
];

/// Godot lifecycle methods with snippet parameter templates.
const LIFECYCLE_METHODS: &[(&str, &str)] = &[
    ("_ready", "_ready():\n\t${0:pass}"),
    ("_process", "_process(${1:delta: float}):\n\t${0:pass}"),
    (
        "_physics_process",
        "_physics_process(${1:delta: float}):\n\t${0:pass}",
    ),
    ("_input", "_input(${1:event: InputEvent}):\n\t${0:pass}"),
    (
        "_unhandled_input",
        "_unhandled_input(${1:event: InputEvent}):\n\t${0:pass}",
    ),
    ("_enter_tree", "_enter_tree():\n\t${0:pass}"),
    ("_exit_tree", "_exit_tree():\n\t${0:pass}"),
    ("_init", "_init():\n\t${0:pass}"),
    (
        "_notification",
        "_notification(${1:what: int}):\n\t${0:pass}",
    ),
    ("_draw", "_draw():\n\t${0:pass}"),
    (
        "_gui_input",
        "_gui_input(${1:event: InputEvent}):\n\t${0:pass}",
    ),
];

// ── Dot-context detection ───────────────────────────────────────────

/// Parsed dot-completion context: `receiver.prefix` where cursor is after the dot.
struct DotContext {
    /// The identifier before the dot (e.g. `"self"`, `"sprite"`, `"Vector2"`).
    receiver: String,
    /// Partial text typed after the dot (for prefix filtering).
    prefix: String,
}

/// Detect if the cursor is in a dot-completion context by examining the text
/// before the cursor on the current line.
fn detect_dot_context(source: &str, position: Position) -> Option<DotContext> {
    let line = source.lines().nth(position.line as usize)?;
    let col = position.character as usize;
    let before = if col <= line.len() {
        &line[..col]
    } else {
        line
    };

    // Find the last '.' in the text before cursor
    let dot_pos = before.rfind('.')?;
    let after_dot = &before[dot_pos + 1..];
    let prefix = after_dot.trim_start().to_string();

    // If there are non-identifier characters between the dot and cursor,
    // this isn't a dot-completion context (e.g. `tilemap.set_cell(x, y)` with
    // cursor inside the argument list)
    if prefix.contains(|c: char| !c.is_alphanumeric() && c != '_') {
        return None;
    }

    // Extract the receiver identifier before the dot
    let before_dot = before[..dot_pos].trim_end();
    if before_dot.is_empty() {
        return None;
    }

    // Walk backwards to find the receiver chain (letters, digits, underscore, dots for chains)
    let receiver_start = before_dot
        .rfind(|c: char| !c.is_alphanumeric() && c != '_' && c != '.')
        .map_or(0, |i| i + 1);
    let receiver = before_dot[receiver_start..].trim_matches('.');
    if receiver.is_empty() {
        return None;
    }

    Some(DotContext {
        receiver: receiver.to_string(),
        prefix,
    })
}

// ── Receiver type resolution ────────────────────────────────────────

/// Resolved receiver info for dot-completions.
pub(super) enum ResolvedReceiver {
    /// A Godot/builtin/workspace class name (e.g. `"Node2D"`, `"CharacterBody2D"`).
    ClassName(String),
    /// An enum from a workspace file — provide its members instead of class members.
    WorkspaceEnum {
        file_content: Arc<String>,
        enum_name: String,
    },
}

/// Resolve the type of a dot-completion receiver.
pub(super) fn resolve_receiver_type(
    receiver: &str,
    source: &str,
    position: Position,
    workspace: Option<&WorkspaceIndex>,
) -> Option<ResolvedReceiver> {
    // Handle dotted chains: "MapBuilder.Tile" → resolve "MapBuilder", then "Tile" within it
    if let Some(dot_pos) = receiver.find('.') {
        let head = &receiver[..dot_pos];
        let tail = &receiver[dot_pos + 1..];
        return resolve_chain(head, tail, source, position, workspace);
    }

    resolve_simple_receiver(receiver, source, position, workspace).map(ResolvedReceiver::ClassName)
}

/// Resolve a simple (non-dotted) receiver to a class name.
pub(super) fn resolve_simple_receiver(
    receiver: &str,
    source: &str,
    position: Position,
    workspace: Option<&WorkspaceIndex>,
) -> Option<String> {
    // 1. self / super → file's extends class
    if receiver == "self" || receiver == "super" {
        if let Ok(tree) = crate::core::parser::parse(source) {
            return find_extends_class(tree.root_node(), source);
        }
        return None;
    }

    // 2. Engine class name (for static-style access like `Node2D.`, `CharacterBody2D.`)
    if crate::class_db::class_exists(receiver) {
        return Some(receiver.to_string());
    }

    // 2b. Built-in types (Vector2, String, Array, etc.) — not in ClassDB but have members
    if BUILTIN_TYPES.contains(&receiver) || !super::builtins::members_for_class(receiver).is_empty()
    {
        return Some(receiver.to_string());
    }

    // 3. Workspace: check class_name index
    if let Some(ws) = workspace {
        if ws.lookup_class_name(receiver).is_some() {
            return Some(receiver.to_string());
        }

        // 3b. Autoload singletons (EventBus., PokemonDB., etc.)
        // Return class_name if declared, otherwise the autoload name itself
        // so that file symbols can be found via autoload_content().
        if let Some(info) = ws.lookup_autoload(receiver) {
            if let Some(cn) = &info.class_name {
                return Some(cn.clone());
            }
            return Some(receiver.to_string());
        }
    }

    // 4. Top-level typed vars from the current file's symbol table
    if let Ok(tree) = crate::core::parser::parse(source) {
        if let Some(ty) = find_variable_type(tree.root_node(), source, receiver) {
            return Some(ty);
        }

        // 5. Local vars/params in enclosing function
        if let Some(ty) = find_local_variable_type(tree.root_node(), source, position, receiver) {
            return Some(ty);
        }

        // 6. Inherited member from the file's extends class (e.g. position → Vector2)
        if let Some(extends_class) = find_extends_class(tree.root_node(), source)
            && let Some(ty) = resolve_member_type(&extends_class, receiver)
        {
            return Some(ty);
        }
    }

    None
}

/// Resolve a dotted chain like `MapBuilder.Tile` or `self.velocity`.
fn resolve_chain(
    head: &str,
    tail: &str,
    source: &str,
    position: Position,
    workspace: Option<&WorkspaceIndex>,
) -> Option<ResolvedReceiver> {
    // Resolve the head first
    let head_type = resolve_simple_receiver(head, source, position, workspace)?;

    // Split remaining tail on dots for multi-level chains
    let parts: Vec<&str> = tail.split('.').collect();
    let member = parts[0];

    // Try to resolve `member` within head_type:

    // 1. If head is a workspace class_name, look up member in that file
    if let Some(ws) = workspace {
        let content = ws
            .lookup_class_name(&head_type)
            .and_then(|path| ws.get_content(&path))
            .or_else(|| ws.autoload_content(&head_type));
        if let Some(content) = content
            && let Ok(tree) = crate::core::parser::parse(&content)
        {
            // Check if member is an enum in that file
            if find_enum_in_source(tree.root_node(), &content, member) {
                return Some(ResolvedReceiver::WorkspaceEnum {
                    file_content: content,
                    enum_name: member.to_string(),
                });
            }
            // Check if member is an inner class — return it as a class name
            if find_inner_class(tree.root_node(), &content, member) {
                return Some(ResolvedReceiver::ClassName(member.to_string()));
            }
            // Check if member is a signal — signals have type "Signal"
            if find_signal_in_source(tree.root_node(), &content, member) {
                let signal_type = "Signal".to_string();
                if parts.len() > 1 {
                    let remaining = parts[1..].join(".");
                    return resolve_chain(&signal_type, &remaining, source, position, workspace);
                }
                return Some(ResolvedReceiver::ClassName(signal_type));
            }
        }
    }

    // 1b. For self/super, check current file's signals
    if (head == "self" || head == "super")
        && let Ok(tree) = crate::core::parser::parse(source)
        && find_signal_in_source(tree.root_node(), source, member)
    {
        let signal_type = "Signal".to_string();
        if parts.len() > 1 {
            let remaining = parts[1..].join(".");
            return resolve_chain(&signal_type, &remaining, source, position, workspace);
        }
        return Some(ResolvedReceiver::ClassName(signal_type));
    }

    // 2. If head resolved to self/extends or a typed var, look up the member's type
    //    e.g. self.velocity → CharacterBody2D.velocity → type Vector2
    if let Some(prop_type) = resolve_member_type(&head_type, member) {
        if parts.len() > 1 {
            // Multi-level: e.g. self.velocity.x → resolve recursively
            let remaining = parts[1..].join(".");
            return resolve_chain(&prop_type, &remaining, source, position, workspace);
        }
        return Some(ResolvedReceiver::ClassName(prop_type));
    }

    None
}

/// Resolve the type of a member (property/method return) on a class.
pub(super) fn resolve_member_type(class: &str, member: &str) -> Option<String> {
    // Check ClassDB properties
    for (name, prop_type, _) in crate::class_db::class_properties(class) {
        if name == member {
            return Some(normalize_type(prop_type));
        }
    }
    // Check ClassDB method return types
    if let Some(ret) = crate::class_db::method_return_type(class, member)
        && ret != "void"
    {
        return Some(normalize_type(ret));
    }
    // Check builtin type members (Vector2.x, String.length(), etc.)
    if let Some(doc) = super::builtins::lookup_member_for(class, member) {
        return extract_type_from_brief(doc.brief, doc.kind);
    }
    None
}

/// Extract a type from a builtin member's brief string.
/// Methods: `"normalized() -> Vector2"` → `"Vector2"`
/// Properties: `"x: float"` → `"float"`
fn extract_type_from_brief(brief: &str, kind: super::builtins::MemberKind) -> Option<String> {
    match kind {
        super::builtins::MemberKind::Method => {
            let ret = brief.rsplit(" -> ").next()?;
            if ret == "void" {
                return None;
            }
            Some(ret.to_string())
        }
        super::builtins::MemberKind::Property => {
            let ty = brief.rsplit(": ").next()?;
            Some(ty.to_string())
        }
    }
}

/// Normalize ClassDB type strings to simple class names.
fn normalize_type(raw: &str) -> String {
    // Strip enum:: prefix, typedarray:: prefix, etc.
    if let Some(rest) = raw.strip_prefix("enum::") {
        // "enum::Error" → "Error"
        rest.to_string()
    } else if let Some(rest) = raw.strip_prefix("typedarray::") {
        // "typedarray::Node" → "Array"
        let _ = rest;
        "Array".to_string()
    } else if let Some(rest) = raw.strip_prefix("bitfield::") {
        let _ = rest;
        "int".to_string()
    } else {
        raw.to_string()
    }
}

/// Check if an enum with the given name exists at the top level of a source file.
fn find_enum_in_source(root: tree_sitter::Node, source: &str, enum_name: &str) -> bool {
    let bytes = source.as_bytes();
    let mut cursor = root.walk();
    root.children(&mut cursor).any(|child| {
        child.kind() == "enum_definition"
            && child
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(bytes).ok())
                == Some(enum_name)
    })
}

/// Check if a signal with the given name exists at the top level of a source file.
fn find_signal_in_source(root: tree_sitter::Node, source: &str, signal_name: &str) -> bool {
    let bytes = source.as_bytes();
    let mut cursor = root.walk();
    root.children(&mut cursor).any(|child| {
        child.kind() == "signal_statement"
            && child
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(bytes).ok())
                == Some(signal_name)
    })
}

/// Check if an inner class with the given name exists at the top level of a source file.
fn find_inner_class(root: tree_sitter::Node, source: &str, class_name: &str) -> bool {
    let bytes = source.as_bytes();
    let mut cursor = root.walk();
    root.children(&mut cursor).any(|child| {
        child.kind() == "class_definition"
            && child
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(bytes).ok())
                == Some(class_name)
    })
}

/// Find the type annotation of a top-level variable by name.
fn find_variable_type(root: tree_sitter::Node, source: &str, var_name: &str) -> Option<String> {
    let bytes = source.as_bytes();
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        if child.kind() == "variable_statement"
            && let Some(name_node) = child.child_by_field_name("name")
            && name_node.utf8_text(bytes).ok() == Some(var_name)
        {
            if let Some(ty) = extract_type_from_variable(&child, source) {
                return Some(ty);
            }
            // Infer from initializer value (var x := Constructor.new())
            if let Some(value_node) = child.child_by_field_name("value")
                && let Some(ty) = infer_type_from_value(
                    &value_node,
                    source,
                    Some(root),
                    &std::collections::HashMap::new(),
                )
            {
                return Some(ty);
            }
        }
    }
    None
}

/// Extract a type annotation from a variable_statement or typed_parameter.
/// Returns `None` for `:=` inferred types (tree-sitter `inferred_type` node).
fn extract_type_from_variable(node: &tree_sitter::Node, source: &str) -> Option<String> {
    let type_node = node.child_by_field_name("type")?;
    // `:=` produces an `inferred_type` marker node — not a real type annotation
    if type_node.kind() == "inferred_type" {
        return None;
    }
    type_node
        .utf8_text(source.as_bytes())
        .ok()
        .map(std::string::ToString::to_string)
}

/// Find the type of a local variable or parameter within the function enclosing `position`.
fn find_local_variable_type(
    root: tree_sitter::Node,
    source: &str,
    position: Position,
    var_name: &str,
) -> Option<String> {
    let bytes = source.as_bytes();
    let point = tree_sitter::Point::new(position.line as usize, position.character as usize);

    let func_node = find_enclosing_function(root, point)?;

    // Search the function body for variable_statement and for_statement
    if let Some(body) = func_node.child_by_field_name("body")
        && let Some(ty) = find_var_type_in_body(body, root, source, var_name, point)
    {
        return Some(ty);
    }

    // Check function parameters (typed_parameter has no `name` field —
    // the identifier is the first named child)
    if let Some(params) = func_node.child_by_field_name("parameters") {
        let mut pcursor = params.walk();
        for param in params.children(&mut pcursor) {
            if param.kind() == "typed_parameter"
                && let Some(first) = param.named_child(0)
                && first.utf8_text(bytes).ok() == Some(var_name)
            {
                return param
                    .child_by_field_name("type")?
                    .utf8_text(bytes)
                    .ok()
                    .map(std::string::ToString::to_string);
            }
        }
    }

    None
}

/// Search a body node recursively for variable/for-loop declarations matching `var_name`.
/// Builds a progressive local type map so that later variables can use earlier types.
fn find_var_type_in_body(
    body: tree_sitter::Node,
    root: tree_sitter::Node,
    source: &str,
    var_name: &str,
    point: tree_sitter::Point,
) -> Option<String> {
    let mut local_types = std::collections::HashMap::new();
    find_var_type_in_body_with_locals(body, root, source, var_name, point, &mut local_types)
}

fn find_var_type_in_body_with_locals(
    body: tree_sitter::Node,
    root: tree_sitter::Node,
    source: &str,
    var_name: &str,
    point: tree_sitter::Point,
    local_types: &mut std::collections::HashMap<String, String>,
) -> Option<String> {
    let bytes = source.as_bytes();
    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        // var x: Type or var x := expr
        if child.kind() == "variable_statement"
            && let Some(name_node) = child.child_by_field_name("name")
            && let Ok(name) = name_node.utf8_text(bytes)
        {
            let resolved = extract_type_from_variable(&child, source).or_else(|| {
                child
                    .child_by_field_name("value")
                    .and_then(|v| infer_type_from_value(&v, source, Some(root), local_types))
            });
            if name == var_name {
                return resolved;
            }
            // Accumulate for later lookups
            if let Some(ty) = resolved {
                local_types.insert(name.to_string(), ty);
            }
        }
        // for npc: Type in expr — typed for-loop iterator
        if child.kind() == "for_statement"
            && let Some(left) = child.child_by_field_name("left")
            && left.utf8_text(bytes).ok() == Some(var_name)
        {
            // Explicit type annotation: for npc: Node2D in ...
            if let Some(type_node) = child.child_by_field_name("type")
                && type_node.kind() != "inferred_type"
                && let Ok(ty) = type_node.utf8_text(bytes)
            {
                return Some(ty.to_string());
            }
        }
        // Recurse into for/if/while/match bodies that contain the cursor
        if is_scope_with_body(&child)
            && child.start_position().row <= point.row
            && child.end_position().row >= point.row
            && let Some(inner_body) = child.child_by_field_name("body")
            && let Some(ty) = find_var_type_in_body_with_locals(
                inner_body,
                root,
                source,
                var_name,
                point,
                local_types,
            )
        {
            return Some(ty);
        }
    }
    None
}

/// Check if a node is a scope-creating statement with a body field.
fn is_scope_with_body(node: &tree_sitter::Node) -> bool {
    matches!(
        node.kind(),
        "for_statement" | "while_statement" | "if_statement" | "elif_clause" | "else_clause"
    )
}

/// Find the function definition node that encloses the given point.
fn find_enclosing_function(
    root: tree_sitter::Node,
    point: tree_sitter::Point,
) -> Option<tree_sitter::Node> {
    let mut cursor = root.walk();
    root.children(&mut cursor).find(|child| {
        (child.kind() == "function_definition" || child.kind() == "constructor_definition")
            && child.start_position().row <= point.row
            && child.end_position().row >= point.row
    })
}

/// Infer a type from an initializer value expression.
///
/// Handles: `ClassName.new()` → `ClassName`, `Vector2(...)` → `Vector2`,
/// `func_call()` → return type from same-file function definition,
/// `expr.method()` → method return type, `a + b` → propagated type,
/// literals → `int`/`float`/`String`/`bool`.
#[allow(clippy::too_many_lines)]
fn infer_type_from_value(
    node: &tree_sitter::Node,
    source: &str,
    root: Option<tree_sitter::Node>,
    local_types: &std::collections::HashMap<String, String>,
) -> Option<String> {
    let bytes = source.as_bytes();
    match node.kind() {
        // ClassName.new(), ClassName.CONSTANT, or receiver.method() / receiver.property
        "attribute" => {
            let first = node.child(0)?;
            let first_text = first.utf8_text(bytes).ok()?;

            // ClassName.new() / ClassName.CONSTANT — first child is a known type
            if crate::class_db::class_exists(first_text)
                || BUILTIN_TYPES.contains(&first_text)
                || !super::builtins::members_for_class(first_text).is_empty()
            {
                return Some(first_text.to_string());
            }

            // receiver.method() / receiver.property — resolve receiver type, then member
            if let Some(receiver_type) = infer_type_from_value(&first, source, root, local_types) {
                let mut c = node.walk();
                for child in node.children(&mut c) {
                    // method call: receiver.method()
                    if child.kind() == "attribute_call"
                        && let Some(method_id) = child.named_child(0)
                        && method_id.kind() == "identifier"
                        && let Ok(method_name) = method_id.utf8_text(bytes)
                    {
                        return resolve_member_type(&receiver_type, method_name)
                            .or(Some(receiver_type));
                    }
                    // property access: receiver.prop
                    if child.kind() == "identifier"
                        && child.id() != first.id()
                        && let Ok(prop_name) = child.utf8_text(bytes)
                    {
                        return resolve_member_type(&receiver_type, prop_name)
                            .or(Some(receiver_type));
                    }
                }
            }
            None
        }
        // Constructor calls (Vector2(...)) or regular function calls
        "call" => {
            let callee = node.named_child(0)?;
            let name = callee.utf8_text(bytes).ok()?;
            // Constructor: class name used as function call
            if crate::class_db::class_exists(name)
                || BUILTIN_TYPES.contains(&name)
                || !super::builtins::members_for_class(name).is_empty()
            {
                return Some(name.to_string());
            }
            // Regular function call: look up return type in same file
            if let Some(root) = root {
                return find_function_return_type(root, name, source);
            }
            None
        }
        // Binary operators: propagate non-primitive type from either operand.
        // e.g. Vector2(...) * TILE_SIZE → Vector2, position - target → Vector2
        "binary_operator" => {
            let op = node
                .child_by_field_name("operator")
                .and_then(|n| n.utf8_text(bytes).ok())
                .unwrap_or("");
            // Comparison operators always return bool
            if matches!(
                op,
                "==" | "!=" | "<" | ">" | "<=" | ">=" | "and" | "or" | "not"
            ) {
                return Some("bool".to_string());
            }
            // For arithmetic, try both sides — prefer non-primitive types
            let left = node
                .child_by_field_name("left")
                .and_then(|n| infer_type_from_value(&n, source, root, local_types));
            let right = node
                .child_by_field_name("right")
                .and_then(|n| infer_type_from_value(&n, source, root, local_types));
            match (&left, &right) {
                (Some(l), _) if !is_primitive_type(l) => left,
                (_, Some(r)) if !is_primitive_type(r) => right,
                (Some(_), _) => left,
                _ => right,
            }
        }
        // Parenthesized expression: unwrap
        "parenthesized_expression" => {
            let inner = node.named_child(0)?;
            infer_type_from_value(&inner, source, root, local_types)
        }
        // Identifier: resolve via local types, file-level vars, and inherited members
        "identifier" => {
            let name = node.utf8_text(bytes).ok()?;
            // Class/type name used as value
            if crate::class_db::class_exists(name) || BUILTIN_TYPES.contains(&name) {
                return Some(name.to_string());
            }
            // Local variable resolved earlier in the same function
            if let Some(ty) = local_types.get(name) {
                return Some(ty.clone());
            }
            if let Some(root) = root {
                // Top-level typed vars in the file
                if let Some(ty) = find_variable_type(root, source, name) {
                    return Some(ty);
                }
                // Inherited member from extends class (e.g. `position` → Vector2)
                if let Some(extends_class) = find_extends_class(root, source)
                    && let Some(ty) = resolve_member_type(&extends_class, name)
                {
                    return Some(ty);
                }
            }
            None
        }
        "integer" => Some("int".to_string()),
        "float" => Some("float".to_string()),
        "string" => Some("String".to_string()),
        "true" | "false" => Some("bool".to_string()),
        _ => None,
    }
}

/// Check if a type name is a primitive GDScript type.
fn is_primitive_type(ty: &str) -> bool {
    matches!(ty, "int" | "float" | "bool" | "String")
}

/// Find the return type of a function definition in the same file.
fn find_function_return_type(
    root: tree_sitter::Node,
    func_name: &str,
    source: &str,
) -> Option<String> {
    let bytes = source.as_bytes();
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        if child.kind() == "function_definition"
            && let Some(name_node) = child.child_by_field_name("name")
            && name_node.utf8_text(bytes).ok() == Some(func_name)
            && let Some(ret) = child.child_by_field_name("return_type")
        {
            return ret
                .utf8_text(bytes)
                .ok()
                .map(std::string::ToString::to_string);
        }
    }
    None
}

// ── Dot-completion member collection ────────────────────────────────

/// Provide dot-completions for a resolved receiver.
fn provide_dot_completions(
    source: &str,
    position: Position,
    dot_ctx: &DotContext,
    workspace: Option<&WorkspaceIndex>,
) -> Vec<CompletionItem> {
    let Some(resolved) = resolve_receiver_type(&dot_ctx.receiver, source, position, workspace)
    else {
        return Vec::new();
    };

    match resolved {
        ResolvedReceiver::ClassName(class_name) => {
            provide_class_dot_completions(source, dot_ctx, workspace, &class_name)
        }
        ResolvedReceiver::WorkspaceEnum {
            file_content,
            enum_name,
        } => collect_enum_dot_completions(&file_content, &enum_name, &dot_ctx.prefix),
    }
}

/// Provide dot-completions for a resolved class name.
fn provide_class_dot_completions(
    source: &str,
    dot_ctx: &DotContext,
    workspace: Option<&WorkspaceIndex>,
    class_name: &str,
) -> Vec<CompletionItem> {
    let prefix = &dot_ctx.prefix;
    let mut items = Vec::new();
    let mut seen = std::collections::HashSet::new();

    // For `self.`, include current file's declarations
    if dot_ctx.receiver == "self"
        && let Ok(tree) = crate::core::parser::parse(source)
    {
        collect_filtered_file_symbols(tree.root_node(), source, prefix, &mut seen, &mut items);
    }

    // For user-defined class names, include that class's file declarations
    if dot_ctx.receiver != "self"
        && dot_ctx.receiver != "super"
        && let Some(ws) = workspace
    {
        collect_user_class_symbols(ws, class_name, prefix, &mut seen, &mut items);
    }

    // Resolve the class for ClassDB/builtin lookup.
    // If class_name is a user-defined name (autoload/class_name), find its extends chain.
    let db_class = if crate::class_db::class_exists(class_name)
        || !super::builtins::members_for_class(class_name).is_empty()
    {
        Some(class_name.to_string())
    } else if let Some(ws) = workspace {
        let content = ws
            .lookup_class_name(class_name)
            .and_then(|path| ws.get_content(&path))
            .or_else(|| ws.autoload_content(class_name));
        content.and_then(|c| {
            let tree = crate::core::parser::parse(&c).ok()?;
            find_extends_class(tree.root_node(), &c)
        })
    } else {
        None
    };

    if let Some(ref cls) = db_class {
        collect_class_db_dot_items(cls, prefix, &mut seen, &mut items);
        collect_builtin_dot_items(cls, prefix, &mut seen, &mut items);
    }

    items
}

/// Provide dot-completions for enum members (e.g. `MapBuilder.Tile.`).
fn collect_enum_dot_completions(
    file_content: &str,
    enum_name: &str,
    prefix: &str,
) -> Vec<CompletionItem> {
    let Ok(tree) = crate::core::parser::parse(file_content) else {
        return Vec::new();
    };
    let root = tree.root_node();
    let bytes = file_content.as_bytes();
    let mut cursor = root.walk();

    for child in root.children(&mut cursor) {
        if child.kind() == "enum_definition"
            && child
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(bytes).ok())
                == Some(enum_name)
        {
            let mut items = Vec::new();
            collect_enum_members(&child, file_content, &mut items);
            if !prefix.is_empty() {
                items.retain(|item| item.label.starts_with(prefix));
            }
            return items;
        }
    }
    Vec::new()
}

/// Collect file symbols filtered by prefix into items, tracking seen labels.
fn collect_filtered_file_symbols(
    root: tree_sitter::Node,
    source: &str,
    prefix: &str,
    seen: &mut std::collections::HashSet<String>,
    items: &mut Vec<CompletionItem>,
) {
    let mut file_items = Vec::new();
    collect_file_symbols(root, source, &mut file_items);
    for item in file_items {
        if (prefix.is_empty() || item.label.starts_with(prefix)) && seen.insert(item.label.clone())
        {
            items.push(item);
        }
    }
}

/// Find and collect symbols from a workspace file matching a class_name.
fn collect_user_class_symbols(
    ws: &WorkspaceIndex,
    class_name: &str,
    prefix: &str,
    seen: &mut std::collections::HashSet<String>,
    items: &mut Vec<CompletionItem>,
) {
    // Fast path: lookup by class_name index
    if let Some(path) = ws.lookup_class_name(class_name)
        && let Some(content) = ws.get_content(&path)
        && let Ok(tree) = crate::core::parser::parse(&content)
    {
        collect_filtered_file_symbols(tree.root_node(), &content, prefix, seen, items);
        return;
    }

    // Fallback: check autoload scripts whose extends matches
    if let Some(content) = ws.autoload_content(class_name)
        && let Ok(tree) = crate::core::parser::parse(&content)
    {
        collect_filtered_file_symbols(tree.root_node(), &content, prefix, seen, items);
    }
}

/// Collect ClassDB methods and properties for dot-completion.
fn collect_class_db_dot_items(
    class_name: &str,
    prefix: &str,
    seen: &mut std::collections::HashSet<String>,
    items: &mut Vec<CompletionItem>,
) {
    for (method_name, ret_type, owner_class) in crate::class_db::class_methods(class_name) {
        if (!prefix.is_empty() && !method_name.starts_with(prefix))
            || !seen.insert(method_name.to_string())
        {
            continue;
        }
        let documentation = member_doc(owner_class, method_name);
        items.push(CompletionItem {
            label: method_name.to_string(),
            kind: Some(CompletionItemKind::METHOD),
            detail: Some(format!("{owner_class}.{method_name}() -> {ret_type}")),
            documentation,
            ..Default::default()
        });
    }

    for (prop_name, prop_type, owner_class) in crate::class_db::class_properties(class_name) {
        if (!prefix.is_empty() && !prop_name.starts_with(prefix))
            || !seen.insert(prop_name.to_string())
        {
            continue;
        }
        let documentation = member_doc(owner_class, prop_name);
        items.push(CompletionItem {
            label: prop_name.to_string(),
            kind: Some(CompletionItemKind::PROPERTY),
            detail: Some(format!("{prop_type} {owner_class}.{prop_name}")),
            documentation,
            ..Default::default()
        });
    }
}

/// Collect builtin member docs not already in ClassDB (e.g. Vector2/String/Array methods).
fn collect_builtin_dot_items(
    class_name: &str,
    prefix: &str,
    seen: &mut std::collections::HashSet<String>,
    items: &mut Vec<CompletionItem>,
) {
    let mut cur = class_name;
    loop {
        for member in super::builtins::members_for_class(cur) {
            if (!prefix.is_empty() && !member.name.starts_with(prefix))
                || !seen.insert(member.name.to_string())
            {
                continue;
            }
            let kind = match member.kind {
                super::builtins::MemberKind::Property => CompletionItemKind::PROPERTY,
                super::builtins::MemberKind::Method => CompletionItemKind::METHOD,
            };
            items.push(CompletionItem {
                label: member.name.to_string(),
                kind: Some(kind),
                detail: Some(member.brief.to_string()),
                documentation: Some(Documentation::MarkupContent(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: member.description.to_string(),
                })),
                ..Default::default()
            });
        }
        match crate::class_db::parent_class(cur) {
            Some(parent) => cur = parent,
            None => break,
        }
    }
}

/// Build documentation from builtin member lookup.
fn member_doc(class: &str, name: &str) -> Option<Documentation> {
    super::builtins::lookup_member_for(class, name).map(|doc| {
        Documentation::MarkupContent(MarkupContent {
            kind: MarkupKind::Markdown,
            value: doc.description.to_string(),
        })
    })
}

/// Try dot-completions only. Returns `Some(items)` if in a dot context with a
/// resolved receiver. Returns `None` if not in a dot context or receiver is unknown.
/// Used to prioritize our dot-completions over Godot proxy results.
pub fn try_dot_completions(
    source: &str,
    position: Position,
    workspace: Option<&WorkspaceIndex>,
) -> Option<Vec<CompletionItem>> {
    let dot_ctx = detect_dot_context(source, position)?;
    let items = provide_dot_completions(source, position, &dot_ctx, workspace);
    if items.is_empty() { None } else { Some(items) }
}

/// Provide completion items at the given position.
pub fn provide_completions(
    source: &str,
    position: Position,
    workspace: Option<&WorkspaceIndex>,
) -> Vec<CompletionItem> {
    // Dot-completion: return only members of the receiver type
    if let Some(dot_ctx) = detect_dot_context(source, position) {
        return provide_dot_completions(source, position, &dot_ctx, workspace);
    }

    let mut items = Vec::new();

    // Keywords
    for &kw in KEYWORDS {
        items.push(CompletionItem {
            label: kw.to_string(),
            kind: Some(CompletionItemKind::KEYWORD),
            ..Default::default()
        });
    }

    // Built-in types
    for &ty in BUILTIN_TYPES {
        let documentation = super::builtins::lookup_type(ty).map(|doc| {
            Documentation::MarkupContent(MarkupContent {
                kind: MarkupKind::Markdown,
                value: doc.description.to_string(),
            })
        });
        items.push(CompletionItem {
            label: ty.to_string(),
            kind: Some(CompletionItemKind::CLASS),
            documentation,
            ..Default::default()
        });
    }

    // Built-in functions
    for &func in BUILTIN_FUNCTIONS {
        let documentation = super::builtins::lookup_function(func).map(|doc| {
            Documentation::MarkupContent(MarkupContent {
                kind: MarkupKind::Markdown,
                value: doc.description.to_string(),
            })
        });
        items.push(CompletionItem {
            label: func.to_string(),
            kind: Some(CompletionItemKind::FUNCTION),
            insert_text: Some(format!("{func}($0)")),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            documentation,
            ..Default::default()
        });
    }

    // Lifecycle methods (snippets)
    for &(label, snippet) in LIFECYCLE_METHODS {
        items.push(CompletionItem {
            label: label.to_string(),
            kind: Some(CompletionItemKind::METHOD),
            detail: Some("Godot lifecycle".to_string()),
            insert_text: Some(snippet.to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            ..Default::default()
        });
    }

    // Symbols from the current file
    if let Ok(tree) = crate::core::parser::parse(source) {
        collect_file_symbols(tree.root_node(), source, &mut items);
    }

    // Symbols from workspace (other files) — deduplicate by skipping labels
    // already added from the current file
    if let Some(ws) = workspace {
        let current_labels: std::collections::HashSet<String> =
            items.iter().map(|i| i.label.clone()).collect();
        for (path, content) in ws.all_files() {
            // Skip if this file's content matches the current source (same file)
            if content.as_ref() == source {
                continue;
            }
            if let Ok(tree) = crate::core::parser::parse(&content) {
                let file_name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown");
                let mut ws_items = Vec::new();
                collect_workspace_symbols(tree.root_node(), &content, file_name, &mut ws_items);
                // Only add workspace symbols that don't duplicate current file symbols
                items.extend(
                    ws_items
                        .into_iter()
                        .filter(|item| !current_labels.contains(&item.label)),
                );
            }
        }
    }

    // Engine methods from class_db based on extends clause
    if let Ok(tree) = crate::core::parser::parse(source)
        && let Some(extends_class) = find_extends_class(tree.root_node(), source)
    {
        collect_class_db_methods(&extends_class, &mut items);
    }

    items
}

/// Find the class name from the `extends` statement at the top of the file.
pub(super) fn find_extends_class(root: tree_sitter::Node, source: &str) -> Option<String> {
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        if child.kind() == "extends_statement" {
            let mut inner = child.walk();
            for c in child.children(&mut inner) {
                if c.kind() != "extends" {
                    let text = c.utf8_text(source.as_bytes()).ok()?;
                    if crate::class_db::class_exists(text) {
                        return Some(text.to_string());
                    }
                }
            }
        }
    }
    None
}

/// Add engine methods from the class_db for the given class and its ancestors.
fn collect_class_db_methods(class: &str, items: &mut Vec<CompletionItem>) {
    for (method_name, ret_type, owner_class) in crate::class_db::class_methods(class) {
        items.push(CompletionItem {
            label: method_name.to_string(),
            kind: Some(CompletionItemKind::METHOD),
            detail: Some(format!("{owner_class}.{method_name}() -> {ret_type}")),
            ..Default::default()
        });
    }
}

/// Extract `##` doc comment lines preceding a declaration node.
fn extract_doc_comment(node: &tree_sitter::Node, source: &str) -> Option<Documentation> {
    let bytes = source.as_bytes();
    let mut lines = Vec::new();
    let mut current = node.prev_named_sibling();

    while let Some(prev) = current {
        match prev.kind() {
            "comment" => {
                if let Ok(text) = prev.utf8_text(bytes) {
                    if let Some(stripped) = text.strip_prefix("##") {
                        lines.push(stripped.trim().to_string());
                    } else {
                        break;
                    }
                }
            }
            "annotation" | "annotations" => {}
            _ => break,
        }
        current = prev.prev_named_sibling();
    }

    if lines.is_empty() {
        None
    } else {
        lines.reverse();
        Some(Documentation::MarkupContent(MarkupContent {
            kind: MarkupKind::Markdown,
            value: lines.join("\n"),
        }))
    }
}

/// Collect symbols from a file's AST as completion items.
fn collect_file_symbols(node: tree_sitter::Node, source: &str, items: &mut Vec<CompletionItem>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "function_definition" => {
                if let Some(name) = child_name(&child, source) {
                    items.push(CompletionItem {
                        label: name.to_string(),
                        kind: Some(CompletionItemKind::FUNCTION),
                        detail: Some(build_function_detail(&child, source)),
                        documentation: extract_doc_comment(&child, source),
                        ..Default::default()
                    });
                }
            }
            "variable_statement" => {
                if let Some(name) = child_name(&child, source) {
                    // Top-level vars are properties; vars inside functions are variables
                    let kind = if child.parent().is_some_and(|p| p.kind() == "source") {
                        CompletionItemKind::PROPERTY
                    } else {
                        CompletionItemKind::VARIABLE
                    };
                    items.push(CompletionItem {
                        label: name.to_string(),
                        kind: Some(kind),
                        detail: Some(build_variable_detail(&child, source)),
                        documentation: extract_doc_comment(&child, source),
                        ..Default::default()
                    });
                }
            }
            "const_statement" => {
                if let Some(name) = child_name(&child, source) {
                    items.push(CompletionItem {
                        label: name.to_string(),
                        kind: Some(CompletionItemKind::CONSTANT),
                        detail: Some(build_const_detail(&child, source)),
                        documentation: extract_doc_comment(&child, source),
                        ..Default::default()
                    });
                }
            }
            "signal_statement" => {
                if let Some(name) = child_name(&child, source) {
                    items.push(CompletionItem {
                        label: name.to_string(),
                        kind: Some(CompletionItemKind::EVENT),
                        detail: Some(build_signal_detail(&child, source)),
                        documentation: extract_doc_comment(&child, source),
                        ..Default::default()
                    });
                }
            }
            "class_definition" => {
                if let Some(name) = child_name(&child, source) {
                    items.push(CompletionItem {
                        label: name.to_string(),
                        kind: Some(CompletionItemKind::CLASS),
                        documentation: extract_doc_comment(&child, source),
                        ..Default::default()
                    });
                }
            }
            "enum_definition" => {
                if let Some(name) = child_name(&child, source) {
                    items.push(CompletionItem {
                        label: name.to_string(),
                        kind: Some(CompletionItemKind::ENUM),
                        documentation: extract_doc_comment(&child, source),
                        ..Default::default()
                    });
                }
                // Also add individual enum members
                collect_enum_members(&child, source, items);
            }
            _ => {}
        }
    }
}

/// Collect top-level symbols from a workspace file.
fn collect_workspace_symbols(
    node: tree_sitter::Node,
    source: &str,
    file_name: &str,
    items: &mut Vec<CompletionItem>,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "function_definition" => {
                if let Some(name) = child_name(&child, source) {
                    let sig = build_function_detail(&child, source);
                    items.push(CompletionItem {
                        label: name.to_string(),
                        kind: Some(CompletionItemKind::FUNCTION),
                        detail: Some(format!("{sig}  ({file_name})")),
                        documentation: extract_doc_comment(&child, source),
                        ..Default::default()
                    });
                }
            }
            "class_definition" | "class_name_statement" => {
                if let Some(name) = child_name(&child, source) {
                    items.push(CompletionItem {
                        label: name.to_string(),
                        kind: Some(CompletionItemKind::CLASS),
                        detail: Some(file_name.to_string()),
                        documentation: extract_doc_comment(&child, source),
                        ..Default::default()
                    });
                }
            }
            "variable_statement" => {
                if let Some(name) = child_name(&child, source) {
                    let var_detail = build_variable_detail(&child, source);
                    items.push(CompletionItem {
                        label: name.to_string(),
                        kind: Some(CompletionItemKind::PROPERTY),
                        detail: Some(format!("{var_detail}  ({file_name})")),
                        documentation: extract_doc_comment(&child, source),
                        ..Default::default()
                    });
                }
            }
            "const_statement" => {
                if let Some(name) = child_name(&child, source) {
                    let const_detail = build_const_detail(&child, source);
                    items.push(CompletionItem {
                        label: name.to_string(),
                        kind: Some(CompletionItemKind::CONSTANT),
                        detail: Some(format!("{const_detail}  ({file_name})")),
                        documentation: extract_doc_comment(&child, source),
                        ..Default::default()
                    });
                }
            }
            "signal_statement" => {
                if let Some(name) = child_name(&child, source) {
                    let sig_detail = build_signal_detail(&child, source);
                    items.push(CompletionItem {
                        label: name.to_string(),
                        kind: Some(CompletionItemKind::EVENT),
                        detail: Some(format!("{sig_detail}  ({file_name})")),
                        documentation: extract_doc_comment(&child, source),
                        ..Default::default()
                    });
                }
            }
            "enum_definition" => {
                if let Some(name) = child_name(&child, source) {
                    items.push(CompletionItem {
                        label: name.to_string(),
                        kind: Some(CompletionItemKind::ENUM),
                        detail: Some(file_name.to_string()),
                        documentation: extract_doc_comment(&child, source),
                        ..Default::default()
                    });
                }
                collect_enum_members(&child, source, items);
            }
            _ => {}
        }
    }
}

fn child_name<'a>(node: &tree_sitter::Node, source: &'a str) -> Option<&'a str> {
    let name_node = node.child_by_field_name("name")?;
    name_node.utf8_text(source.as_bytes()).ok()
}

/// Build detail string for a variable: `var name: Type` or `var name := value`.
fn build_variable_detail(node: &tree_sitter::Node, source: &str) -> String {
    let text = node.utf8_text(source.as_bytes()).unwrap_or("");
    let first_line = text.lines().next().unwrap_or(text).trim();
    first_line.to_string()
}

/// Build detail string for a constant: `const NAME: Type = value`.
fn build_const_detail(node: &tree_sitter::Node, source: &str) -> String {
    let text = node.utf8_text(source.as_bytes()).unwrap_or("");
    let first_line = text.lines().next().unwrap_or(text).trim();
    first_line.to_string()
}

/// Build detail string for a signal: `signal name(params)`.
fn build_signal_detail(node: &tree_sitter::Node, source: &str) -> String {
    let text = node.utf8_text(source.as_bytes()).unwrap_or("");
    let first_line = text.lines().next().unwrap_or(text).trim();
    first_line.to_string()
}

/// Collect individual enum members as `ENUM_MEMBER` completion items.
fn collect_enum_members(
    enum_node: &tree_sitter::Node,
    source: &str,
    items: &mut Vec<CompletionItem>,
) {
    let enum_name = enum_node
        .child_by_field_name("name")
        .and_then(|n| n.utf8_text(source.as_bytes()).ok());

    let Some(body) = enum_node.child_by_field_name("body") else {
        return;
    };

    let mut body_cursor = body.walk();
    let mut index: i64 = 0;

    for member in body.children(&mut body_cursor) {
        if member.kind() != "enumerator" {
            continue;
        }
        let Some(left) = member.child_by_field_name("left") else {
            continue;
        };
        let Ok(member_name) = left.utf8_text(source.as_bytes()) else {
            continue;
        };

        let value_str = if let Some(val) = member.child_by_field_name("right") {
            let text = val.utf8_text(source.as_bytes()).unwrap_or("?");
            if let Ok(v) = text.parse::<i64>() {
                index = v + 1;
            } else {
                index += 1;
            }
            text.trim().to_string()
        } else {
            let v = index;
            index += 1;
            v.to_string()
        };

        let detail = match enum_name {
            Some(name) => format!("{name}.{member_name} = {value_str}"),
            None => format!("{member_name} = {value_str}"),
        };

        items.push(CompletionItem {
            label: member_name.to_string(),
            kind: Some(CompletionItemKind::ENUM_MEMBER),
            detail: Some(detail),
            documentation: extract_doc_comment(&member, source),
            ..Default::default()
        });
    }
}

fn build_function_detail(node: &tree_sitter::Node, source: &str) -> String {
    let mut detail = "func(".to_string();
    if let Some(params) = node.child_by_field_name("parameters") {
        let params_text = params.utf8_text(source.as_bytes()).unwrap_or("()");
        let inner = params_text
            .strip_prefix('(')
            .and_then(|s| s.strip_suffix(')'))
            .unwrap_or(params_text);
        detail.push_str(inner);
    }
    detail.push(')');
    if let Some(ret) = node.child_by_field_name("return_type") {
        detail.push_str(" -> ");
        detail.push_str(ret.utf8_text(source.as_bytes()).unwrap_or("unknown"));
    }
    detail
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keywords_and_builtins_present() {
        let items = provide_completions("", Position::new(0, 0), None);
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(labels.contains(&"func"));
        assert!(labels.contains(&"var"));
        assert!(labels.contains(&"Vector2"));
        assert!(labels.contains(&"print"));
        assert!(labels.contains(&"_ready"));
    }

    #[test]
    fn lifecycle_methods_are_snippets() {
        let items = provide_completions("", Position::new(0, 0), None);
        let ready = items.iter().find(|i| i.label == "_ready").unwrap();
        assert_eq!(ready.insert_text_format, Some(InsertTextFormat::SNIPPET));
        assert!(ready.insert_text.as_ref().unwrap().contains("pass"));
    }

    #[test]
    fn builtin_functions_are_snippets() {
        let items = provide_completions("", Position::new(0, 0), None);
        let print_item = items.iter().find(|i| i.label == "print").unwrap();
        assert_eq!(
            print_item.insert_text_format,
            Some(InsertTextFormat::SNIPPET)
        );
        assert_eq!(print_item.insert_text.as_deref(), Some("print($0)"));
    }

    #[test]
    fn collects_symbols_from_source() {
        let source = r"
var health := 100
const MAX_SPEED = 200
signal damage_taken
enum State { IDLE, RUN }

func _ready():
    pass

func attack(target):
    pass
";
        let items = provide_completions(source, Position::new(0, 0), None);
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(labels.contains(&"health"));
        assert!(labels.contains(&"MAX_SPEED"));
        assert!(labels.contains(&"damage_taken"));
        assert!(labels.contains(&"State"));
        assert!(labels.contains(&"attack"));
    }

    #[test]
    fn function_detail_includes_params() {
        let source = "func move(speed: float, dir: Vector2) -> bool:\n\tpass\n";
        let items = provide_completions(source, Position::new(0, 0), None);
        let move_item = items
            .iter()
            .find(|i| i.label == "move" && i.kind == Some(CompletionItemKind::FUNCTION))
            .unwrap();
        let detail = move_item.detail.as_deref().unwrap();
        assert!(detail.contains("speed: float"));
        assert!(detail.contains("-> bool"));
    }

    #[test]
    fn extends_adds_class_db_methods() {
        let source = "extends Node2D\n\nfunc _ready():\n\tpass\n";
        let items = provide_completions(source, Position::new(0, 0), None);
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        // Node2D own method
        assert!(labels.contains(&"apply_scale"));
        // Inherited from Node
        assert!(labels.contains(&"add_child"));
    }

    #[test]
    fn extends_method_detail_shows_class() {
        let source = "extends Node2D\n";
        let items = provide_completions(source, Position::new(0, 0), None);
        let add_child = items
            .iter()
            .find(|i| i.label == "add_child" && i.kind == Some(CompletionItemKind::METHOD))
            .unwrap();
        let detail = add_child.detail.as_deref().unwrap();
        assert!(detail.contains("Node.add_child()"));
    }

    #[test]
    fn no_class_db_methods_without_extends() {
        let source = "func _ready():\n\tpass\n";
        let items = provide_completions(source, Position::new(0, 0), None);
        // add_child should not appear (only lifecycle snippets and file symbols)
        let engine_methods: Vec<&CompletionItem> = items
            .iter()
            .filter(|i| {
                i.kind == Some(CompletionItemKind::METHOD)
                    && i.detail.as_deref().is_some_and(|d| d.contains("Node."))
            })
            .collect();
        assert!(engine_methods.is_empty());
    }

    #[test]
    fn completion_includes_doc_comment() {
        let source = "## Move the player forward.\nfunc move():\n\tpass\n";
        let items = provide_completions(source, Position::new(0, 0), None);
        let move_item = items
            .iter()
            .find(|i| i.label == "move" && i.kind == Some(CompletionItemKind::FUNCTION))
            .unwrap();
        match &move_item.documentation {
            Some(Documentation::MarkupContent(mc)) => {
                assert_eq!(mc.value, "Move the player forward.");
            }
            _ => panic!("Expected MarkupContent documentation"),
        }
    }

    #[test]
    fn completion_no_doc_comment() {
        let source = "func idle():\n\tpass\n";
        let items = provide_completions(source, Position::new(0, 0), None);
        let idle_item = items
            .iter()
            .find(|i| i.label == "idle" && i.kind == Some(CompletionItemKind::FUNCTION))
            .unwrap();
        assert!(idle_item.documentation.is_none());
    }

    #[test]
    fn completion_var_doc_comment() {
        let source = "## The player's health.\nvar health: int = 100\n";
        let items = provide_completions(source, Position::new(0, 0), None);
        let health_item = items
            .iter()
            .find(|i| i.label == "health" && i.kind == Some(CompletionItemKind::PROPERTY))
            .unwrap();
        assert!(health_item.documentation.is_some());
    }

    #[test]
    fn top_level_var_is_property_kind() {
        let source = "var health: int = 100\n";
        let items = provide_completions(source, Position::new(0, 0), None);
        let health = items.iter().find(|i| i.label == "health").unwrap();
        assert_eq!(health.kind, Some(CompletionItemKind::PROPERTY));
    }

    #[test]
    fn variable_detail_shows_declaration() {
        let source = "var speed: float = 5.0\n";
        let items = provide_completions(source, Position::new(0, 0), None);
        let speed = items.iter().find(|i| i.label == "speed").unwrap();
        assert_eq!(speed.detail.as_deref(), Some("var speed: float = 5.0"));
    }

    #[test]
    fn const_detail_shows_declaration() {
        let source = "const MAX_HP: int = 100\n";
        let items = provide_completions(source, Position::new(0, 0), None);
        let item = items.iter().find(|i| i.label == "MAX_HP").unwrap();
        assert_eq!(item.detail.as_deref(), Some("const MAX_HP: int = 100"));
    }

    #[test]
    fn signal_detail_shows_params() {
        let source = "signal health_changed(old: int, new_val: int)\n";
        let items = provide_completions(source, Position::new(0, 0), None);
        let item = items.iter().find(|i| i.label == "health_changed").unwrap();
        let detail = item.detail.as_deref().unwrap();
        assert!(detail.contains("health_changed"));
        assert!(detail.contains("old: int"));
    }

    #[test]
    fn enum_members_are_enum_member_kind() {
        let source = "enum Color { RED, GREEN, BLUE }\n";
        let items = provide_completions(source, Position::new(0, 0), None);
        let red = items.iter().find(|i| i.label == "RED").unwrap();
        assert_eq!(red.kind, Some(CompletionItemKind::ENUM_MEMBER));
        assert_eq!(red.detail.as_deref(), Some("Color.RED = 0"));
    }

    #[test]
    fn enum_member_with_explicit_value() {
        let source = "enum Flags { A = 10, B, C }\n";
        let items = provide_completions(source, Position::new(0, 0), None);
        let a = items.iter().find(|i| i.label == "A").unwrap();
        assert_eq!(a.detail.as_deref(), Some("Flags.A = 10"));
        let b = items.iter().find(|i| i.label == "B").unwrap();
        assert_eq!(b.detail.as_deref(), Some("Flags.B = 11"));
    }

    // ── Dot-completion tests ────────────────────────────────────────

    #[test]
    fn dot_context_self() {
        let ctx = detect_dot_context("\tself.", Position::new(0, 6)).unwrap();
        assert_eq!(ctx.receiver, "self");
        assert_eq!(ctx.prefix, "");
    }

    #[test]
    fn dot_context_self_with_prefix() {
        let ctx = detect_dot_context("\tself.pos", Position::new(0, 10)).unwrap();
        assert_eq!(ctx.receiver, "self");
        assert_eq!(ctx.prefix, "pos");
    }

    #[test]
    fn dot_context_variable() {
        let ctx = detect_dot_context("\tplayer.move", Position::new(0, 13)).unwrap();
        assert_eq!(ctx.receiver, "player");
        assert_eq!(ctx.prefix, "move");
    }

    #[test]
    fn dot_context_engine_class() {
        let ctx = detect_dot_context("\tVector2.", Position::new(0, 10)).unwrap();
        assert_eq!(ctx.receiver, "Vector2");
        assert_eq!(ctx.prefix, "");
    }

    #[test]
    fn dot_context_none_without_dot() {
        assert!(detect_dot_context("\tvar x = 1", Position::new(0, 11)).is_none());
    }

    #[test]
    fn self_dot_includes_file_symbols_excludes_keywords() {
        let source =
            "extends CharacterBody2D\nvar health: int = 100\nfunc run():\n\tself.\n\tpass\n";
        // Cursor at end of `self.` on line 3
        let items = provide_completions(source, Position::new(3, 6), None);
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        // File symbols should be present
        assert!(
            labels.contains(&"health"),
            "should contain file var 'health'"
        );
        assert!(labels.contains(&"run"), "should contain file func 'run'");
        // Engine methods from CharacterBody2D should be present
        assert!(
            labels.contains(&"move_and_slide"),
            "should contain engine method 'move_and_slide'"
        );
        // Keywords/builtins should NOT be present
        assert!(
            !labels.contains(&"func"),
            "should not contain keyword 'func'"
        );
        assert!(!labels.contains(&"var"), "should not contain keyword 'var'");
        assert!(
            !labels.contains(&"print"),
            "should not contain builtin 'print'"
        );
        assert!(
            !labels.contains(&"Vector2"),
            "should not contain type 'Vector2'"
        );
    }

    #[test]
    fn self_dot_prefix_filters() {
        let source =
            "extends CharacterBody2D\nvar position_offset := 0\nfunc run():\n\tself.pos\n\tpass\n";
        let items = provide_completions(source, Position::new(3, 9), None);
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        // position_offset matches prefix "pos"
        assert!(labels.contains(&"position_offset"));
        // add_child does NOT match prefix "pos"
        assert!(!labels.contains(&"add_child"));
    }

    #[test]
    fn typed_var_dot_completions() {
        let source = "extends Node\nvar s: Sprite2D\nfunc run():\n\ts.\n\tpass\n";
        let items = provide_completions(source, Position::new(3, 3), None);
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        // Sprite2D properties from ClassDB
        assert!(
            labels.contains(&"texture"),
            "should contain Sprite2D property 'texture'"
        );
        assert!(
            labels.contains(&"flip_h"),
            "should contain Sprite2D property 'flip_h'"
        );
    }

    #[test]
    fn local_typed_var_in_function() {
        let source = "extends Node\nfunc run():\n\tvar s: Sprite2D\n\ts.\n\tpass\n";
        let items = provide_completions(source, Position::new(3, 3), None);
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            labels.contains(&"texture"),
            "should resolve local typed var"
        );
    }

    #[test]
    fn typed_param_in_function() {
        let source = "extends Node\nfunc run(s: Sprite2D):\n\ts.\n\tpass\n";
        let items = provide_completions(source, Position::new(2, 3), None);
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            labels.contains(&"texture"),
            "should resolve typed parameter"
        );
    }

    #[test]
    fn engine_class_dot_completions() {
        let source = "extends Node\nfunc run():\n\tVector2.\n\tpass\n";
        let items = provide_completions(source, Position::new(2, 9), None);
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        // Vector2 methods from builtins
        assert!(
            labels.contains(&"normalized"),
            "should contain Vector2 method 'normalized'"
        );
    }

    #[test]
    fn color_dot_completions() {
        let source = "extends Node\nfunc run():\n\tColor.\n\tpass\n";
        let items = provide_completions(source, Position::new(2, 7), None);
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            labels.contains(&"lightened"),
            "should contain Color method 'lightened', got: {labels:?}"
        );
        assert!(
            labels.contains(&"r"),
            "should contain Color property 'r', got: {labels:?}"
        );
    }

    #[test]
    fn string_dot_completions_generated() {
        let source = "extends Node\nfunc run():\n\tString.\n\tpass\n";
        let items = provide_completions(source, Position::new(2, 8), None);
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        // Generated methods that weren't in the manual list
        assert!(
            labels.contains(&"replace"),
            "should contain String method 'replace', got: {labels:?}"
        );
        assert!(
            labels.contains(&"is_empty"),
            "should contain String method 'is_empty', got: {labels:?}"
        );
    }

    #[test]
    fn non_dot_context_unchanged() {
        let source = "extends Node2D\nfunc run():\n\tvar x = 1\n";
        let items = provide_completions(source, Position::new(2, 10), None);
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        // Should have keywords and builtins
        assert!(labels.contains(&"func"));
        assert!(labels.contains(&"print"));
        assert!(labels.contains(&"Vector2"));
    }

    #[test]
    fn unknown_receiver_returns_empty() {
        let source = "extends Node\nfunc run():\n\tunknown_thing.\n";
        let items = provide_completions(source, Position::new(2, 15), None);
        assert!(
            items.is_empty(),
            "unknown receiver should return empty list"
        );
    }

    #[test]
    fn self_dot_includes_class_db_properties() {
        let source = "extends CharacterBody2D\nfunc run():\n\tself.\n\tpass\n";
        let items = provide_completions(source, Position::new(2, 6), None);
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        // CharacterBody2D own property
        assert!(labels.contains(&"velocity"), "should contain 'velocity'");
        // Inherited from Node2D
        assert!(
            labels.contains(&"position"),
            "should contain inherited 'position'"
        );
    }

    // ── Chain resolution tests ───────────────────────────────────────

    #[test]
    fn dot_context_chained_receiver() {
        let ctx = detect_dot_context("\tMapBuilder.Tile.", Position::new(0, 18)).unwrap();
        assert_eq!(ctx.receiver, "MapBuilder.Tile");
        assert_eq!(ctx.prefix, "");
    }

    #[test]
    fn dot_context_chained_with_prefix() {
        let ctx = detect_dot_context("\tMapBuilder.Tile.GR", Position::new(0, 20)).unwrap();
        assert_eq!(ctx.receiver, "MapBuilder.Tile");
        assert_eq!(ctx.prefix, "GR");
    }

    #[test]
    fn enum_dot_completions_direct() {
        let source = "class_name MapBuilder\nenum Tile { GRASS, WATER, SAND }\n";
        let items = collect_enum_dot_completions(source, "Tile", "");
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(labels.contains(&"GRASS"));
        assert!(labels.contains(&"WATER"));
        assert!(labels.contains(&"SAND"));
        assert_eq!(items.len(), 3);
    }

    #[test]
    fn enum_dot_completions_with_prefix() {
        let source = "enum Tile { GRASS, WATER, SAND }\n";
        let items = collect_enum_dot_completions(source, "Tile", "GR");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].label, "GRASS");
    }

    #[test]
    fn chain_self_velocity_dot() {
        // self.velocity. should resolve to Vector2 members
        let source = "extends CharacterBody2D\nfunc run():\n\tself.velocity.\n\tpass\n";
        let items = provide_completions(source, Position::new(2, 15), None);
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        // Vector2 methods via builtins
        assert!(
            labels.contains(&"normalized"),
            "self.velocity. should show Vector2 method 'normalized', got: {labels:?}"
        );
        // Should NOT have keywords
        assert!(!labels.contains(&"func"));
    }

    #[test]
    fn chain_self_velocity_prefix() {
        let source = "extends CharacterBody2D\nfunc run():\n\tself.velocity.nor\n\tpass\n";
        let items = provide_completions(source, Position::new(2, 18), None);
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(labels.contains(&"normalized"));
        assert!(!labels.contains(&"length"));
    }

    // ── Inferred type tests ──────────────────────────────────────────

    #[test]
    fn local_var_inferred_from_constructor_new() {
        // var rng := RandomNumberGenerator.new() → rng. should show RNG members
        let source =
            "extends Node\nfunc run():\n\tvar rng := RandomNumberGenerator.new()\n\trng.\n\tpass\n";
        let items = provide_completions(source, Position::new(3, 5), None);
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            labels.contains(&"seed"),
            "rng. should show RandomNumberGenerator.seed, got: {labels:?}"
        );
    }

    #[test]
    fn local_var_inferred_from_builtin_constructor() {
        // var v := Vector2(1, 2) → v. should show Vector2 members
        let source = "extends Node\nfunc run():\n\tvar v := Vector2(1, 2)\n\tv.\n\tpass\n";
        let items = provide_completions(source, Position::new(3, 3), None);
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            labels.contains(&"normalized"),
            "v. should show Vector2 method 'normalized', got: {labels:?}"
        );
    }

    #[test]
    fn top_level_var_inferred_from_constructor() {
        // Top-level var with := should also infer type
        let source =
            "extends Node\nvar rng := RandomNumberGenerator.new()\nfunc run():\n\trng.\n\tpass\n";
        let items = provide_completions(source, Position::new(3, 5), None);
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            labels.contains(&"seed"),
            "top-level rng. should show RNG members, got: {labels:?}"
        );
    }

    // ── Function return type inference ────────────────────────────────

    #[test]
    fn local_var_inferred_from_function_return_type() {
        // var input_dir := _get_input_direction() where func returns -> Vector2
        let source = "extends Node\nfunc run():\n\tvar input_dir := _get_input_direction()\n\tinput_dir.\n\tpass\n\nfunc _get_input_direction() -> Vector2:\n\treturn Vector2.ZERO\n";
        let items = provide_completions(source, Position::new(3, 11), None);
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            labels.contains(&"normalized"),
            "input_dir. should show Vector2 method 'normalized', got: {labels:?}"
        );
    }

    #[test]
    fn local_var_inferred_from_class_constant() {
        // var dir := Vector2.ZERO → dir. should show Vector2 members
        let source = "extends Node\nfunc run():\n\tvar dir := Vector2.ZERO\n\tdir.\n\tpass\n";
        let items = provide_completions(source, Position::new(3, 5), None);
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            labels.contains(&"normalized"),
            "dir. should show Vector2 method 'normalized', got: {labels:?}"
        );
    }

    // ── For-loop typed iterator ──────────────────────────────────────

    #[test]
    fn for_loop_typed_iterator_completions() {
        // for npc: Node2D in get_children() → npc. should show Node2D members
        let source =
            "extends Node\nfunc run():\n\tfor npc: Node2D in get_children():\n\t\tnpc.\n\t\tpass\n";
        // \t\tnpc. → col 6 is after the dot
        let items = provide_completions(source, Position::new(3, 6), None);
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            labels.contains(&"position"),
            "npc. in typed for-loop should show Node2D.position, got: {labels:?}"
        );
    }

    // ── Signal member completions ────────────────────────────────────

    #[test]
    fn signal_dot_completions() {
        // self.my_signal. should show Signal members (emit, connect, etc.)
        let source =
            "extends Node\nsignal my_signal(value: int)\nfunc run():\n\tself.my_signal.\n\tpass\n";
        // \tself.my_signal. → col 16 is after the second dot
        let items = provide_completions(source, Position::new(3, 16), None);
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            labels.contains(&"emit"),
            "self.my_signal. should show Signal.emit, got: {labels:?}"
        );
        assert!(
            labels.contains(&"connect"),
            "self.my_signal. should show Signal.connect, got: {labels:?}"
        );
    }
}
