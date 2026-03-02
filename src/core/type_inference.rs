//! Expression-level type inference for GDScript AST nodes.
//!
//! Layer 2: given any expression node, determine its inferred type.
//! Builds on the typed AST (`GdFile`) and the engine ClassDB.

use tree_sitter::Node;

use super::gd_ast::GdFile;
use super::workspace_index::ProjectIndex;
use crate::class_db;

/// An inferred type for a GDScript expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InferredType {
    /// A built-in type: `"int"`, `"float"`, `"String"`, `"bool"`, `"Vector2"`, etc.
    Builtin(&'static str),
    /// A class type: `"Node"`, `"CharacterBody2D"`, or a user `class_name`.
    Class(String),
    /// A user-defined enum name.
    Enum(String),
    /// `Array[T]` with a known element type.
    TypedArray(Box<InferredType>),
    /// The method/function returns nothing.
    Void,
    /// Dynamic/unknown type.
    Variant,
}

impl InferredType {
    /// Returns the type name for display purposes.
    pub fn display_name(&self) -> String {
        match self {
            InferredType::Builtin(s) => (*s).to_string(),
            InferredType::Class(s) | InferredType::Enum(s) => s.clone(),
            InferredType::TypedArray(inner) => format!("Array[{}]", inner.display_name()),
            InferredType::Void => "void".to_string(),
            InferredType::Variant => "Variant".to_string(),
        }
    }

    /// Returns true if this type is numeric (`int` or `float`).
    pub fn is_numeric(&self) -> bool {
        matches!(self, InferredType::Builtin("int" | "float"))
    }
}

/// Try to infer the type of an expression AST node.
///
/// Returns `None` when the type cannot be determined (not the same as `Variant` —
/// `None` means "we don't know", `Some(Variant)` means "we know it's dynamic").
pub fn infer_expression_type(node: &Node, source: &str, file: &GdFile) -> Option<InferredType> {
    match node.kind() {
        // ── Literals ────────────────────────────────────────────────
        "integer" => Some(InferredType::Builtin("int")),
        "float" => Some(InferredType::Builtin("float")),
        "string" => Some(InferredType::Builtin("String")),
        "true" | "false" => Some(InferredType::Builtin("bool")),
        "null" | "subscript" => Some(InferredType::Variant),
        "array" => Some(InferredType::Builtin("Array")),
        "dictionary" => Some(InferredType::Builtin("Dictionary")),

        // ── Unary operators ─────────────────────────────────────────
        "unary_operator" => infer_unary(node, source, file),

        // ── Binary operators ────────────────────────────────────────
        "binary_operator" => infer_binary(node, source, file),

        // ── Cast: `x as Node` ───────────────────────────────────────
        "as_pattern" | "cast" => infer_cast(node, source),

        // ── Ternary: `a if cond else b` ─────────────────────────────
        "conditional_expression" | "ternary_expression" => infer_ternary(node, source, file),

        // ── Parenthesized: `(expr)` → recurse ──────────────────────
        "parenthesized_expression" => node
            .named_child(0)
            .and_then(|inner| infer_expression_type(&inner, source, file)),

        // ── $Node / get_node ────────────────────────────────────────
        "get_node" => Some(InferredType::Class("Node".to_string())),

        // ── Identifiers → typed AST lookup ──────────────────────────
        "identifier" => infer_identifier(node, source, file),

        // ── Function/constructor calls ──────────────────────────────
        "call" => infer_call(node, source, file),

        // ── Method calls: `obj.method()` ────────────────────────────
        "attribute" => infer_attribute(node, source, file),

        // ── Await: result type of the awaited expression ────────────
        "await_expression" => node
            .named_child(0)
            .and_then(|inner| infer_expression_type(&inner, source, file)),

        _ => None,
    }
}

/// Try to infer the type of an expression AST node, with access to the project-wide index.
///
/// This extends `infer_expression_type` with cross-file resolution: user-defined base class
/// methods, autoload types, and preloaded script types.
pub fn infer_expression_type_with_project(
    node: &Node,
    source: &str,
    file: &GdFile,
    project: &ProjectIndex,
) -> Option<InferredType> {
    match node.kind() {
        "call" => infer_call_with_project(node, source, file, project),
        "attribute" => infer_attribute_with_project(node, source, file, project),
        // For all other node kinds, delegate to the per-file inference
        _ => infer_expression_type(node, source, file),
    }
}

/// Infer type from a function/constructor call, with project-wide resolution.
fn infer_call_with_project(
    node: &Node,
    source: &str,
    file: &GdFile,
    project: &ProjectIndex,
) -> Option<InferredType> {
    let func_node = node
        .child_by_field_name("function")
        .or_else(|| node.named_child(0))?;
    let func_name = func_node.utf8_text(source.as_bytes()).ok()?;

    // 1. Constructor calls
    if let Some(typ) = constructor_return_type(func_name) {
        return Some(typ);
    }

    // 2. preload/load — resolve with project index for class_name lookup
    if matches!(func_name, "preload" | "load") {
        if let Some(path) = extract_string_arg(node, source) {
            return resolve_load_type_with_project(&path, project);
        }
        return Some(InferredType::Class("Resource".to_string()));
    }

    // 3. GDScript builtin functions
    if let Some(typ) = builtin_function_return_type(func_name) {
        return Some(typ);
    }

    // 4. Self method calls — typed AST first
    for func in file.funcs() {
        if func.name == func_name {
            return func.return_type.as_ref().map_or_else(
                || Some(InferredType::Variant),
                |ret| {
                    if ret.name == "void" {
                        Some(InferredType::Void)
                    } else {
                        Some(classify_type_name(ret.name))
                    }
                },
            );
        }
    }

    // 5. Project index: check user-defined base classes via extends chain
    if let Some(extends) = file.extends_class()
        && let Some(ret) = project.method_return_type(extends, func_name)
    {
        return Some(classify_type_str(&ret));
    }

    // 6. ClassDB lookup via extends chain
    if let Some(extends) = file.extends_class()
        && let Some(ret_type) = class_db::method_return_type(extends, func_name)
    {
        return Some(parse_class_db_type(ret_type));
    }

    None
}

/// Infer type from an attribute expression, with project-wide resolution.
fn infer_attribute_with_project(
    node: &Node,
    source: &str,
    file: &GdFile,
    project: &ProjectIndex,
) -> Option<InferredType> {
    let mut has_call = false;
    let mut method_name = None;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "attribute_call" {
            has_call = true;
            if let Some(name_node) = child.named_child(0) {
                method_name = name_node.utf8_text(source.as_bytes()).ok();
            }
        }
    }

    let receiver = node.named_child(0)?;
    let receiver_type = infer_expression_type_with_project(&receiver, source, file, project);

    // If receiver doesn't resolve, check if it's a class name (e.g., Node.new())
    let receiver_class_name: Option<String>;
    let class_name = if let Some(ref rt) = receiver_type {
        match rt {
            InferredType::Builtin(b) => *b,
            InferredType::Class(c) => c.as_str(),
            _ => return None,
        }
    } else {
        let name = receiver.utf8_text(source.as_bytes()).ok()?;
        if receiver.kind() == "identifier"
            && (class_db::class_exists(name) || project.lookup_class(name).is_some())
        {
            receiver_class_name = Some(name.to_string());
            receiver_class_name.as_deref()?
        } else {
            return None;
        }
    };

    if !has_call {
        // Property access — resolve via builtin members, project index, then ClassDB
        let prop = node.named_child(1)?;
        let prop_name = prop.utf8_text(source.as_bytes()).ok()?;

        if let Some(t) = builtin_member_type(class_name, prop_name) {
            return Some(t);
        }
        if let Some(t) = project.variable_type(class_name, prop_name) {
            return Some(classify_type_str(&t));
        }
        if let Some(t) = class_db::property_type(class_name, prop_name) {
            return Some(parse_class_db_type(t));
        }
        return None;
    }

    let method = method_name?;

    // ClassName.new() → returns ClassName
    if method == "new"
        && (class_db::class_exists(class_name) || project.lookup_class(class_name).is_some())
    {
        return Some(InferredType::Class(class_name.to_string()));
    }

    // Try project index first (user-defined types)
    if let Some(ret) = project.method_return_type(class_name, method) {
        return Some(classify_type_str(&ret));
    }

    // Fall back to ClassDB
    if let Some(ret_type) = class_db::method_return_type(class_name, method) {
        return Some(parse_class_db_type(ret_type));
    }

    None
}

/// Classify a type string from the project index (may be "void", "int", etc.).
fn classify_type_str(name: &str) -> InferredType {
    if name == "void" {
        return InferredType::Void;
    }
    if name == "Variant" || name.is_empty() {
        return InferredType::Variant;
    }
    classify_type_name(name)
}

/// Infer type of a unary operator expression.
fn infer_unary(node: &Node, source: &str, file: &GdFile) -> Option<InferredType> {
    // Check if it's a boolean negation
    let mut cursor = node.walk();
    let children: Vec<Node> = node.children(&mut cursor).collect();
    if children
        .first()
        .is_some_and(|c| c.kind() == "not" || c.utf8_text(source.as_bytes()).ok() == Some("!"))
    {
        return Some(InferredType::Builtin("bool"));
    }
    // `-x`, `+x`, `~x` → propagate operand type (first named child is the operand)
    node.named_child(0)
        .and_then(|operand| infer_expression_type(&operand, source, file))
}

/// Infer type of a binary operator expression.
fn infer_binary(node: &Node, source: &str, file: &GdFile) -> Option<InferredType> {
    let op = node.child_by_field_name("op").or_else(|| {
        // Some tree-sitter versions use positional children for the operator
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if !child.is_named() {
                let text = child.utf8_text(source.as_bytes()).unwrap_or("");
                if matches!(
                    text,
                    "+" | "-"
                        | "*"
                        | "/"
                        | "%"
                        | "**"
                        | "<<"
                        | ">>"
                        | "&"
                        | "|"
                        | "^"
                        | "and"
                        | "or"
                ) {
                    return Some(child);
                }
            }
        }
        None
    });

    let op_text = op?.utf8_text(source.as_bytes()).ok()?;

    // Comparison operators and type tests → always bool
    if matches!(
        op_text,
        "==" | "!=" | "<" | ">" | "<=" | ">=" | "in" | "not in" | "is" | "is not"
    ) {
        return Some(InferredType::Builtin("bool"));
    }

    // Boolean operators
    if matches!(op_text, "and" | "or") {
        return Some(InferredType::Builtin("bool"));
    }

    // String concatenation or arithmetic
    if matches!(op_text, "+" | "-" | "*" | "**") {
        return infer_arithmetic(node, source, file, op_text);
    }

    // Division always returns float in GDScript
    if op_text == "/" {
        return Some(InferredType::Builtin("float"));
    }

    // String format operator: "text %s" % value → String
    if op_text == "%" {
        let left = node
            .child_by_field_name("left")
            .or_else(|| node.named_child(0));
        if let Some(l) = left
            && (l.kind() == "string"
                || infer_expression_type(&l, source, file) == Some(InferredType::Builtin("String")))
        {
            return Some(InferredType::Builtin("String"));
        }
        return Some(InferredType::Builtin("int"));
    }

    // Bit ops → int
    if matches!(op_text, "<<" | ">>" | "&" | "|" | "^") {
        return Some(InferredType::Builtin("int"));
    }

    None
}

/// Infer type from arithmetic operators (+, -, *, **) with promotion rules.
fn infer_arithmetic(
    node: &Node,
    source: &str,
    file: &GdFile,
    op_text: &str,
) -> Option<InferredType> {
    let left = node
        .child_by_field_name("left")
        .or_else(|| node.named_child(0));
    let right = node
        .child_by_field_name("right")
        .or_else(|| node.named_child(1));

    let (Some(l), Some(r)) = (left, right) else {
        return None;
    };
    let lt = infer_expression_type(&l, source, file);
    let rt = infer_expression_type(&r, source, file);

    // String + String → String
    if op_text == "+"
        && matches!(
            (&lt, &rt),
            (
                Some(InferredType::Builtin("String")),
                Some(InferredType::Builtin("String"))
            )
        )
    {
        return Some(InferredType::Builtin("String"));
    }

    match (&lt, &rt) {
        (Some(InferredType::Builtin("float")), _) | (_, Some(InferredType::Builtin("float"))) => {
            Some(InferredType::Builtin("float"))
        }
        (Some(InferredType::Builtin("int")), Some(InferredType::Builtin("int"))) => {
            Some(InferredType::Builtin("int"))
        }
        _ => lt.or(rt),
    }
}

/// Infer type from a cast expression (`x as Node`).
fn infer_cast(node: &Node, source: &str) -> Option<InferredType> {
    // The type is the last named child (or child_by_field_name("type"))
    let type_node = node.child_by_field_name("type").or_else(|| {
        let count = node.named_child_count();
        if count >= 2 {
            node.named_child(count - 1)
        } else {
            None
        }
    })?;
    let type_name = type_node.utf8_text(source.as_bytes()).ok()?;
    Some(classify_type_name(type_name))
}

/// Infer type of a ternary expression — common type of both branches.
fn infer_ternary(node: &Node, source: &str, file: &GdFile) -> Option<InferredType> {
    // tree-sitter-gdscript: conditional_expression has 3 named children:
    // [0] = true branch, [1] = condition, [2] = false branch
    let true_branch = node.named_child(0)?;
    let false_branch = node.named_child(2).or_else(|| node.named_child(1))?;

    let true_type = infer_expression_type(&true_branch, source, file);
    let false_type = infer_expression_type(&false_branch, source, file);

    match (&true_type, &false_type) {
        (Some(a), Some(b)) if a == b => true_type,
        (None, Some(_)) => false_type,
        // (Some(_), None), mismatched, or both None: return true branch type
        _ => true_type,
    }
}

/// Infer type from an identifier by looking it up in the typed AST.
fn infer_identifier(node: &Node, source: &str, file: &GdFile) -> Option<InferredType> {
    let name = node.utf8_text(source.as_bytes()).ok()?;

    // Check class-level variables
    for var in file.vars() {
        if var.name == name {
            if let Some(ref type_ann) = var.type_ann
                && !type_ann.is_inferred
                && !type_ann.name.is_empty()
            {
                // Check if there's a narrowed type from an `is` guard
                if let Some(narrowed) = find_narrowed_type(node, name, source) {
                    return Some(classify_type_name(&narrowed));
                }
                return Some(classify_type_name(type_ann.name));
            }
            return None;
        }
    }

    // Check enum names (the enum itself is a type)
    for e in file.enums() {
        if e.name == name {
            return Some(InferredType::Enum(name.to_string()));
        }
    }

    // Fallback: check for type narrowing (covers function params, for-loop iterators, etc.)
    if let Some(narrowed) = find_narrowed_type(node, name, source) {
        return Some(classify_type_name(&narrowed));
    }

    None
}

/// Infer type from a function/constructor call.
fn infer_call(node: &Node, source: &str, file: &GdFile) -> Option<InferredType> {
    // tree-sitter-gdscript: call has `function` field, or first named child is identifier
    let func_node = node
        .child_by_field_name("function")
        .or_else(|| node.named_child(0))?;
    let func_name = func_node.utf8_text(source.as_bytes()).ok()?;

    // 1. Constructor calls (PascalCase): Vector2(...), Color(...), etc.
    if let Some(typ) = constructor_return_type(func_name) {
        return Some(typ);
    }

    // 2. preload/load — resolve by file extension
    if matches!(func_name, "preload" | "load") {
        if let Some(path) = extract_string_arg(node, source) {
            return resolve_load_type_by_extension(&path);
        }
        return Some(InferredType::Class("Resource".to_string()));
    }

    // 3. GDScript builtin functions
    if let Some(typ) = builtin_function_return_type(func_name) {
        return Some(typ);
    }

    // 4. Self method calls — look up in file, then ClassDB via extends chain
    for func in file.funcs() {
        if func.name == func_name {
            return func.return_type.as_ref().map_or_else(
                || Some(InferredType::Variant),
                |ret| {
                    if ret.name == "void" {
                        Some(InferredType::Void)
                    } else {
                        Some(classify_type_name(ret.name))
                    }
                },
            );
        }
    }

    // 5. ClassDB lookup via extends chain
    if let Some(extends) = file.extends_class()
        && let Some(ret_type) = class_db::method_return_type(extends, func_name)
    {
        return Some(parse_class_db_type(ret_type));
    }

    None
}

/// Infer type from an attribute expression (property access or method call).
///
/// tree-sitter pattern: `obj.method()` → `attribute` > [`identifier`, `attribute_call`]
fn infer_attribute(node: &Node, source: &str, file: &GdFile) -> Option<InferredType> {
    // Check if this is a method call (has attribute_call child)
    let mut has_call = false;
    let mut method_name = None;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "attribute_call" {
            has_call = true;
            if let Some(name_node) = child.named_child(0) {
                method_name = name_node.utf8_text(source.as_bytes()).ok();
            }
        }
    }

    // Infer the receiver type
    let receiver = node.named_child(0)?;
    let receiver_type = infer_expression_type(&receiver, source, file);

    // If receiver doesn't resolve, check if it's a class name (e.g., Node.new())
    let receiver_class_name: Option<String>;
    let class_name = if let Some(ref rt) = receiver_type {
        match rt {
            InferredType::Builtin(b) => *b,
            InferredType::Class(c) => c.as_str(),
            _ => return None,
        }
    } else {
        // Check if receiver is a known class identifier
        let name = receiver.utf8_text(source.as_bytes()).ok()?;
        if receiver.kind() == "identifier" && class_db::class_exists(name) {
            receiver_class_name = Some(name.to_string());
            receiver_class_name.as_deref()?
        } else {
            return None;
        }
    };

    if !has_call {
        // Property access — resolve via builtin members, then ClassDB
        let prop = node.named_child(1)?;
        let prop_name = prop.utf8_text(source.as_bytes()).ok()?;

        if let Some(t) = builtin_member_type(class_name, prop_name) {
            return Some(t);
        }
        if let Some(t) = class_db::property_type(class_name, prop_name) {
            return Some(parse_class_db_type(t));
        }
        return None;
    }

    let method = method_name?;

    // ClassName.new() → returns ClassName
    if method == "new" && class_db::class_exists(class_name) {
        return Some(InferredType::Class(class_name.to_string()));
    }

    // Look up the method in ClassDB
    if let Some(ret_type) = class_db::method_return_type(class_name, method) {
        return Some(parse_class_db_type(ret_type));
    }

    None
}

/// Convert a ClassDB return type string to an `InferredType`.
///
/// ClassDB uses formats like: `"void"`, `"int"`, `"Node"`, `"typedarray::Node"`,
/// `"enum::Error"`, `"enum::Viewport.MSAA"`, `"Variant"`.
pub fn parse_class_db_type(raw: &str) -> InferredType {
    if raw == "void" {
        return InferredType::Void;
    }
    if raw == "Variant" {
        return InferredType::Variant;
    }
    if let Some(inner) = raw.strip_prefix("typedarray::") {
        let element = parse_class_db_type(inner);
        return InferredType::TypedArray(Box::new(element));
    }
    if let Some(enum_name) = raw.strip_prefix("enum::") {
        return InferredType::Enum(enum_name.to_string());
    }
    // Check if it's a known builtin type
    if is_builtin_type(raw) {
        return InferredType::Builtin(leak_str(raw));
    }
    // Otherwise it's a class
    InferredType::Class(raw.to_string())
}

/// Classify a type name from source code into an `InferredType`.
pub fn classify_type_name(name: &str) -> InferredType {
    // Handle ClassDB `typedarray::X` format (e.g. from method return types)
    if let Some(inner) = name.strip_prefix("typedarray::") {
        return InferredType::TypedArray(Box::new(classify_type_name(inner)));
    }
    // Handle ClassDB `enum::X` format
    if let Some(enum_name) = name.strip_prefix("enum::") {
        return InferredType::Enum(enum_name.to_string());
    }
    // Handle Array[T] syntax
    if let Some(inner) = name
        .strip_prefix("Array[")
        .and_then(|s| s.strip_suffix(']'))
    {
        let element = classify_type_name(inner);
        return InferredType::TypedArray(Box::new(element));
    }

    if name == "void" {
        return InferredType::Void;
    }
    if name == "Variant" {
        return InferredType::Variant;
    }
    if is_builtin_type(name) {
        return InferredType::Builtin(leak_str(name));
    }
    InferredType::Class(name.to_string())
}

/// Return type for constructor calls (PascalCase names that construct values).
fn constructor_return_type(name: &str) -> Option<InferredType> {
    let typ = match name {
        "Vector2" => "Vector2",
        "Vector2i" => "Vector2i",
        "Vector3" => "Vector3",
        "Vector3i" => "Vector3i",
        "Vector4" => "Vector4",
        "Vector4i" => "Vector4i",
        "Color" => "Color",
        "Rect2" => "Rect2",
        "Rect2i" => "Rect2i",
        "Transform2D" => "Transform2D",
        "Transform3D" => "Transform3D",
        "Basis" => "Basis",
        "Quaternion" => "Quaternion",
        "AABB" => "AABB",
        "Plane" => "Plane",
        "StringName" => "StringName",
        "NodePath" => "NodePath",
        "RID" => "RID",
        "Callable" => "Callable",
        "Signal" => "Signal",
        "PackedByteArray" => "PackedByteArray",
        "PackedInt32Array" => "PackedInt32Array",
        "PackedInt64Array" => "PackedInt64Array",
        "PackedFloat32Array" => "PackedFloat32Array",
        "PackedFloat64Array" => "PackedFloat64Array",
        "PackedStringArray" => "PackedStringArray",
        "PackedVector2Array" => "PackedVector2Array",
        "PackedVector3Array" => "PackedVector3Array",
        "PackedColorArray" => "PackedColorArray",
        "PackedVector4Array" => "PackedVector4Array",
        "Array" => "Array",
        "Dictionary" => "Dictionary",
        "Projection" => "Projection",
        _ => return None,
    };
    Some(InferredType::Builtin(typ))
}

/// Return types for GDScript builtin functions.
pub fn builtin_function_return_type(name: &str) -> Option<InferredType> {
    match name {
        // Integer-returning
        "int" | "floor" | "ceil" | "round" | "roundi" | "floori" | "ceili" | "absi" | "clampi"
        | "wrapi" | "mini" | "maxi" | "posmod" | "snappedi" | "hash" | "len" | "randi"
        | "typeof" => Some(InferredType::Builtin("int")),
        // Float-returning
        "float"
        | "sqrt"
        | "sin"
        | "cos"
        | "tan"
        | "asin"
        | "acos"
        | "atan"
        | "atan2"
        | "abs"
        | "lerp"
        | "inverselerp"
        | "inverse_lerp"
        | "randf"
        | "randf_range"
        | "randi_range"
        | "deg_to_rad"
        | "rad_to_deg"
        | "exp"
        | "log"
        | "pow"
        | "fmod"
        | "fposmod"
        | "ease"
        | "smoothstep"
        | "move_toward"
        | "lerp_angle"
        | "pingpong"
        | "sign"
        | "signf"
        | "snappedf"
        | "wrapf"
        | "minf"
        | "maxf"
        | "bezier_derivative"
        | "bezier_interpolate"
        | "cubic_interpolate"
        | "cubic_interpolate_angle"
        | "cubic_interpolate_in_time"
        | "cubic_interpolate_angle_in_time"
        | "lerpf"
        | "db_to_linear"
        | "linear_to_db"
        | "is_inf"
        | "is_nan"
        | "stepify"
        | "nearest_po2" => Some(InferredType::Builtin("float")),
        // String-returning
        "str" | "String" | "char" | "JSON.stringify" => Some(InferredType::Builtin("String")),
        // Bool-returning
        "bool" | "is_equal_approx" | "is_zero_approx" | "is_finite" | "is_instance_valid"
        | "is_instance_of" | "is_same" => Some(InferredType::Builtin("bool")),
        // Void-returning
        "print" | "print_rich" | "prints" | "printt" | "printerr" | "print_verbose"
        | "push_error" | "push_warning" | "assert" | "breakpoint" | "seed" | "randomize"
        | "queue_free" | "free" => Some(InferredType::Void),
        // Polymorphic builtins — accept Variant, return Variant
        // (typed variants like maxi/maxf/mini/minf/clampi/clampf are above)
        "max" | "min" | "clamp" | "snapped" | "wrap" => Some(InferredType::Variant),
        // Collection-returning
        "range" => Some(InferredType::Builtin("Array")),
        _ => None,
    }
}

/// Try to find a narrowed type for `var_name` at the given AST node position.
///
/// Detects two patterns:
/// - **Direct guard**: `if event is InputEventKey:` → within body, `event` is `InputEventKey`
/// - **Early exit**: `if not event is InputEventKey: return/continue/break` → after that if,
///   `event` is `InputEventKey`
pub fn find_narrowed_type(node: &Node, var_name: &str, source: &str) -> Option<String> {
    let bytes = source.as_bytes();
    let mut current = *node;

    while let Some(parent) = current.parent() {
        if parent.kind() == "body" {
            // Pattern A: Direct guard — body's parent is `if_statement` with `var is Type`
            if let Some(if_stmt) = parent.parent()
                && if_stmt.kind() == "if_statement"
                && let Some(cond) = if_stmt.named_child(0)
                && cond.kind() != "body"
                && let Some(ty) = extract_is_check(&cond, var_name, bytes)
            {
                return Some(ty);
            }

            // Pattern B: Early exit — preceding sibling `if not var is Type: return/continue/break`
            let target_row = node.start_position().row;
            let mut cursor = parent.walk();
            for sibling in parent.children(&mut cursor) {
                if sibling.start_position().row >= target_row {
                    break;
                }
                if sibling.kind() == "if_statement"
                    && let Some(ty) = extract_negated_is_early_exit(&sibling, var_name, bytes)
                {
                    return Some(ty);
                }
            }
        }
        current = parent;
    }
    None
}

/// Extract `Type` from a condition like `var_name is Type`.
fn extract_is_check(cond: &Node, var_name: &str, source: &[u8]) -> Option<String> {
    if cond.kind() != "binary_operator" {
        return None;
    }
    // Look for unnamed child "is"
    let mut has_is = false;
    let mut cursor = cond.walk();
    for child in cond.children(&mut cursor) {
        if !child.is_named() && child.utf8_text(source).ok() == Some("is") {
            has_is = true;
        }
    }
    if !has_is {
        return None;
    }
    // Left operand = identifier matching var_name
    let left = cond.named_child(0)?;
    if left.utf8_text(source).ok()? != var_name {
        return None;
    }
    // Right operand = the type name
    let right = cond.named_child(1)?;
    Some(right.utf8_text(source).ok()?.to_string())
}

/// Extract `Type` from `if not var_name is Type: return/continue/break` (early exit pattern).
fn extract_negated_is_early_exit(if_stmt: &Node, var_name: &str, source: &[u8]) -> Option<String> {
    let cond = if_stmt.named_child(0)?;
    if cond.kind() == "body" {
        return None;
    }

    // Condition should be `not (var is Type)` — unary_operator wrapping binary_operator
    let inner = if cond.kind() == "unary_operator" {
        // Check it's a `not` operator
        let mut is_not = false;
        let mut binary = None;
        let mut cursor = cond.walk();
        for child in cond.children(&mut cursor) {
            if !child.is_named() && child.utf8_text(source).ok() == Some("not") {
                is_not = true;
            }
            if child.kind() == "binary_operator" {
                binary = Some(child);
            }
        }
        if !is_not {
            return None;
        }
        binary?
    } else {
        return None;
    };

    // Check inner is `var_name is Type`
    let type_name = extract_is_check(&inner, var_name, source)?;

    // Check the body contains only early-exit statements
    let mut cursor2 = if_stmt.walk();
    for child in if_stmt.children(&mut cursor2) {
        if child.kind() == "body" {
            let mut body_cursor = child.walk();
            for stmt in child.named_children(&mut body_cursor) {
                if !matches!(
                    stmt.kind(),
                    "return_statement" | "continue_statement" | "break_statement"
                ) {
                    return None;
                }
            }
            return Some(type_name);
        }
    }
    None
}

/// Resolve property types for value-type builtins not covered by ClassDB.
/// ClassDB tracks engine Object-derived classes; value types like Vector2, Color,
/// etc. have members that only exist in the GDScript binding layer.
pub fn builtin_member_type(class: &str, member: &str) -> Option<InferredType> {
    match (class, member) {
        // Scalar float members
        ("Vector2" | "Vector2i", "x" | "y")
        | ("Vector3" | "Vector3i", "x" | "y" | "z")
        | ("Vector4" | "Vector4i" | "Quaternion", "x" | "y" | "z" | "w")
        | ("Color", "r" | "g" | "b" | "a" | "r8" | "g8" | "b8" | "a8" | "h" | "s" | "v")
        | ("Plane", "d" | "x" | "y" | "z") => Some(InferredType::Builtin("float")),
        // → Vector2
        ("Rect2" | "Transform2D", "position" | "end" | "size" | "origin" | "x" | "y") => {
            Some(InferredType::Builtin("Vector2"))
        }
        // → Vector2i
        ("Rect2i", "position" | "end" | "size") => Some(InferredType::Builtin("Vector2i")),
        // → Vector3
        ("Transform3D" | "AABB", "origin" | "position" | "end" | "size")
        | ("Basis", "x" | "y" | "z")
        | ("Plane", "normal") => Some(InferredType::Builtin("Vector3")),
        // → Basis
        ("Transform3D", "basis") => Some(InferredType::Builtin("Basis")),
        // → Vector4
        ("Projection", "x" | "y" | "z" | "w") => Some(InferredType::Builtin("Vector4")),
        _ => None,
    }
}

/// Resolve the type of a `preload()`/`load()` call based on the file extension.
/// Without project index, only extension-based resolution is possible.
fn resolve_load_type_by_extension(path: &str) -> Option<InferredType> {
    let ext = path.rsplit('.').next()?;
    match ext {
        "tscn" | "scn" => Some(InferredType::Class("PackedScene".to_string())),
        "gd" => Some(InferredType::Class("GDScript".to_string())),
        "png" | "jpg" | "jpeg" | "webp" | "svg" | "bmp" | "tga" | "hdr" | "exr" => {
            Some(InferredType::Class("Texture2D".to_string()))
        }
        "ogg" | "wav" | "mp3" => Some(InferredType::Class("AudioStream".to_string())),
        "shader" | "gdshader" => Some(InferredType::Class("Shader".to_string())),
        // .tres, .res, and everything else → Resource
        _ => Some(InferredType::Class("Resource".to_string())),
    }
}

/// Resolve the type of a `preload()`/`load()` call with project index access.
/// Can resolve `.gd` files to their `class_name` if available.
fn resolve_load_type_with_project(path: &str, project: &ProjectIndex) -> Option<InferredType> {
    let ext = path.rsplit('.').next()?;
    if ext == "gd" {
        // Try to resolve to the script's class_name
        if let Some(fs) = project.resolve_preload(path)
            && let Some(ref class_name) = fs.class_name
        {
            return Some(InferredType::Class(class_name.clone()));
        }
        return Some(InferredType::Class("GDScript".to_string()));
    }
    resolve_load_type_by_extension(path)
}

/// Extract a string literal from the first argument of a call node.
fn extract_string_arg(node: &Node, source: &str) -> Option<String> {
    let args = node.child_by_field_name("arguments")?;
    let first_arg = args.named_child(0)?;
    if first_arg.kind() == "string" {
        let text = first_arg.utf8_text(source.as_bytes()).ok()?;
        // Strip quotes
        Some(text.trim_matches('"').to_string())
    } else {
        None
    }
}

/// Check if a type name is a GDScript builtin type.
pub fn is_builtin_type(name: &str) -> bool {
    matches!(
        name,
        "int"
            | "float"
            | "bool"
            | "String"
            | "StringName"
            | "NodePath"
            | "Vector2"
            | "Vector2i"
            | "Vector3"
            | "Vector3i"
            | "Vector4"
            | "Vector4i"
            | "Color"
            | "Rect2"
            | "Rect2i"
            | "Transform2D"
            | "Transform3D"
            | "Basis"
            | "Quaternion"
            | "AABB"
            | "Plane"
            | "RID"
            | "Callable"
            | "Signal"
            | "Array"
            | "Dictionary"
            | "PackedByteArray"
            | "PackedInt32Array"
            | "PackedInt64Array"
            | "PackedFloat32Array"
            | "PackedFloat64Array"
            | "PackedStringArray"
            | "PackedVector2Array"
            | "PackedVector3Array"
            | "PackedColorArray"
            | "PackedVector4Array"
            | "Projection"
    )
}

/// Leak a string to get a `&'static str`. Used for type names that come from
/// source code and need to live as long as a `Builtin` variant.
fn leak_str(s: &str) -> &'static str {
    // For known builtins, return the static literal directly
    match s {
        "int" => "int",
        "float" => "float",
        "bool" => "bool",
        "String" => "String",
        "StringName" => "StringName",
        "NodePath" => "NodePath",
        "Vector2" => "Vector2",
        "Vector2i" => "Vector2i",
        "Vector3" => "Vector3",
        "Vector3i" => "Vector3i",
        "Vector4" => "Vector4",
        "Vector4i" => "Vector4i",
        "Color" => "Color",
        "Rect2" => "Rect2",
        "Rect2i" => "Rect2i",
        "Transform2D" => "Transform2D",
        "Transform3D" => "Transform3D",
        "Basis" => "Basis",
        "Quaternion" => "Quaternion",
        "AABB" => "AABB",
        "Plane" => "Plane",
        "RID" => "RID",
        "Callable" => "Callable",
        "Signal" => "Signal",
        "Array" => "Array",
        "Dictionary" => "Dictionary",
        "PackedByteArray" => "PackedByteArray",
        "PackedInt32Array" => "PackedInt32Array",
        "PackedInt64Array" => "PackedInt64Array",
        "PackedFloat32Array" => "PackedFloat32Array",
        "PackedFloat64Array" => "PackedFloat64Array",
        "PackedStringArray" => "PackedStringArray",
        "PackedVector2Array" => "PackedVector2Array",
        "PackedVector3Array" => "PackedVector3Array",
        "PackedColorArray" => "PackedColorArray",
        "PackedVector4Array" => "PackedVector4Array",
        "Projection" => "Projection",
        "void" => "void",
        "Variant" => "Variant",
        _ => Box::leak(s.to_string().into_boxed_str()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::gd_ast;
    use crate::core::parser;

    /// Parse source, build typed AST, find the value node of the first variable
    /// statement, and infer its type.
    fn infer_var_value(source: &str) -> Option<InferredType> {
        let tree = parser::parse(source).unwrap();
        let file = gd_ast::convert(&tree, source);
        let root = tree.root_node();
        find_first_var_value(&root, source, &file)
    }

    fn find_first_var_value(node: &Node, source: &str, file: &GdFile) -> Option<InferredType> {
        if node.kind() == "variable_statement"
            && let Some(value) = node.child_by_field_name("value")
        {
            return infer_expression_type(&value, source, file);
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if let Some(result) = find_first_var_value(&child, source, file) {
                return Some(result);
            }
        }
        None
    }

    // ── Literals ────────────────────────────────────────────────

    #[test]
    fn literal_int() {
        assert_eq!(
            infer_var_value("func f():\n\tvar x = 42\n"),
            Some(InferredType::Builtin("int"))
        );
    }

    #[test]
    fn literal_float() {
        assert_eq!(
            infer_var_value("func f():\n\tvar x = 3.14\n"),
            Some(InferredType::Builtin("float"))
        );
    }

    #[test]
    fn literal_string() {
        assert_eq!(
            infer_var_value("func f():\n\tvar x = \"hello\"\n"),
            Some(InferredType::Builtin("String"))
        );
    }

    #[test]
    fn literal_bool_true() {
        assert_eq!(
            infer_var_value("func f():\n\tvar x = true\n"),
            Some(InferredType::Builtin("bool"))
        );
    }

    #[test]
    fn literal_bool_false() {
        assert_eq!(
            infer_var_value("func f():\n\tvar x = false\n"),
            Some(InferredType::Builtin("bool"))
        );
    }

    #[test]
    fn literal_array() {
        assert_eq!(
            infer_var_value("func f():\n\tvar x = [1, 2]\n"),
            Some(InferredType::Builtin("Array"))
        );
    }

    #[test]
    fn literal_dictionary() {
        assert_eq!(
            infer_var_value("func f():\n\tvar x = {\"a\": 1}\n"),
            Some(InferredType::Builtin("Dictionary"))
        );
    }

    #[test]
    fn literal_null() {
        assert_eq!(
            infer_var_value("func f():\n\tvar x = null\n"),
            Some(InferredType::Variant)
        );
    }

    // ── Constructors ────────────────────────────────────────────

    #[test]
    fn constructor_vector2() {
        assert_eq!(
            infer_var_value("func f():\n\tvar x = Vector2(1, 2)\n"),
            Some(InferredType::Builtin("Vector2"))
        );
    }

    #[test]
    fn constructor_color() {
        assert_eq!(
            infer_var_value("func f():\n\tvar x = Color(1, 0, 0)\n"),
            Some(InferredType::Builtin("Color"))
        );
    }

    #[test]
    fn constructor_transform3d() {
        assert_eq!(
            infer_var_value("func f():\n\tvar x = Transform3D()\n"),
            Some(InferredType::Builtin("Transform3D"))
        );
    }

    // ── Builtin functions ───────────────────────────────────────

    #[test]
    fn builtin_str() {
        assert_eq!(
            infer_var_value("func f():\n\tvar x = str(42)\n"),
            Some(InferredType::Builtin("String"))
        );
    }

    #[test]
    fn builtin_len() {
        assert_eq!(
            infer_var_value("func f():\n\tvar x = len([1, 2])\n"),
            Some(InferredType::Builtin("int"))
        );
    }

    #[test]
    fn builtin_sqrt() {
        assert_eq!(
            infer_var_value("func f():\n\tvar x = sqrt(4.0)\n"),
            Some(InferredType::Builtin("float"))
        );
    }

    #[test]
    fn builtin_typeof() {
        assert_eq!(
            infer_var_value("func f():\n\tvar x = typeof(42)\n"),
            Some(InferredType::Builtin("int"))
        );
    }

    #[test]
    fn builtin_range() {
        assert_eq!(
            infer_var_value("func f():\n\tvar x = range(10)\n"),
            Some(InferredType::Builtin("Array"))
        );
    }

    #[test]
    fn builtin_max_returns_variant() {
        assert_eq!(
            infer_var_value("func f():\n\tvar x = max(1, 2)\n"),
            Some(InferredType::Variant)
        );
    }

    #[test]
    fn builtin_min_returns_variant() {
        assert_eq!(
            infer_var_value("func f():\n\tvar x = min(1, 2)\n"),
            Some(InferredType::Variant)
        );
    }

    #[test]
    fn builtin_maxi_returns_int() {
        assert_eq!(
            infer_var_value("func f():\n\tvar x = maxi(1, 2)\n"),
            Some(InferredType::Builtin("int"))
        );
    }

    #[test]
    fn builtin_clamp_returns_variant() {
        assert_eq!(
            infer_var_value("func f():\n\tvar x = clamp(5, 1, 10)\n"),
            Some(InferredType::Variant)
        );
    }

    #[test]
    fn builtin_preload() {
        assert_eq!(
            infer_var_value("func f():\n\tvar x = preload(\"res://scene.tscn\")\n"),
            Some(InferredType::Class("PackedScene".to_string()))
        );
    }

    // ── Unary operators ─────────────────────────────────────────

    #[test]
    fn unary_negate_int() {
        assert_eq!(
            infer_var_value("func f():\n\tvar x = -42\n"),
            Some(InferredType::Builtin("int"))
        );
    }

    #[test]
    fn unary_negate_float() {
        assert_eq!(
            infer_var_value("func f():\n\tvar x = -3.14\n"),
            Some(InferredType::Builtin("float"))
        );
    }

    #[test]
    fn unary_not() {
        assert_eq!(
            infer_var_value("func f():\n\tvar x = not true\n"),
            Some(InferredType::Builtin("bool"))
        );
    }

    // ── Binary operators ────────────────────────────────────────

    #[test]
    fn binary_int_add() {
        assert_eq!(
            infer_var_value("func f():\n\tvar x = 1 + 2\n"),
            Some(InferredType::Builtin("int"))
        );
    }

    #[test]
    fn binary_float_add() {
        assert_eq!(
            infer_var_value("func f():\n\tvar x = 1.0 + 2.0\n"),
            Some(InferredType::Builtin("float"))
        );
    }

    #[test]
    fn binary_promotion() {
        assert_eq!(
            infer_var_value("func f():\n\tvar x = 1 + 2.0\n"),
            Some(InferredType::Builtin("float"))
        );
    }

    #[test]
    fn binary_string_concat() {
        assert_eq!(
            infer_var_value("func f():\n\tvar x = \"a\" + \"b\"\n"),
            Some(InferredType::Builtin("String"))
        );
    }

    #[test]
    fn binary_modulo() {
        assert_eq!(
            infer_var_value("func f():\n\tvar x = 10 % 3\n"),
            Some(InferredType::Builtin("int"))
        );
    }

    // ── Comparisons ─────────────────────────────────────────────

    #[test]
    fn comparison_eq() {
        assert_eq!(
            infer_var_value("func f():\n\tvar x = 1 == 2\n"),
            Some(InferredType::Builtin("bool"))
        );
    }

    #[test]
    fn comparison_lt() {
        assert_eq!(
            infer_var_value("func f():\n\tvar x = 1 < 2\n"),
            Some(InferredType::Builtin("bool"))
        );
    }

    // ── Self method calls ───────────────────────────────────────

    #[test]
    fn self_method_with_return_type() {
        let source = "\
extends Node
func get_value() -> int:
\treturn 42
func f():
\tvar x = get_value()
";
        assert_eq!(infer_var_value(source), Some(InferredType::Builtin("int")));
    }

    #[test]
    fn self_method_void() {
        let source = "\
extends Node
func do_thing() -> void:
\tpass
func f():
\tvar x = do_thing()
";
        assert_eq!(infer_var_value(source), Some(InferredType::Void));
    }

    #[test]
    fn self_method_no_return_type() {
        let source = "\
extends Node
func unknown():
\tpass
func f():
\tvar x = unknown()
";
        assert_eq!(infer_var_value(source), Some(InferredType::Variant));
    }

    // ── ClassDB method calls (via extends) ──────────────────────

    #[test]
    fn classdb_self_call() {
        let source = "\
extends Node
func f():
\tvar x = get_child(0)
";
        assert_eq!(
            infer_var_value(source),
            Some(InferredType::Class("Node".to_string()))
        );
    }

    #[test]
    fn classdb_void_call() {
        let source = "\
extends Node
func f():
\tvar x = add_child(null)
";
        assert_eq!(infer_var_value(source), Some(InferredType::Void));
    }

    // ── Chained method calls ────────────────────────────────────

    #[test]
    fn chained_method_call() {
        let source = "\
extends Node
var node: Node2D
func f():
\tvar x = node.get_child(0)
";
        // node is Node2D, get_child returns Node (inherited from Node)
        assert_eq!(
            infer_var_value(source),
            Some(InferredType::Class("Node".to_string()))
        );
    }

    // ── Identifiers ─────────────────────────────────────────────

    #[test]
    fn identifier_typed_var() {
        let source = "\
var health: int
func f():
\tvar x = health
";
        assert_eq!(infer_var_value(source), Some(InferredType::Builtin("int")));
    }

    #[test]
    fn identifier_untyped_var() {
        let source = "\
var data
func f():
\tvar x = data
";
        assert_eq!(infer_var_value(source), None);
    }

    // ── get_node / $ ────────────────────────────────────────────

    #[test]
    fn get_node_dollar() {
        assert_eq!(
            infer_var_value("func f():\n\tvar x = $Sprite2D\n"),
            Some(InferredType::Class("Node".to_string()))
        );
    }

    // ── Parenthesized ───────────────────────────────────────────

    #[test]
    fn parenthesized_expression() {
        assert_eq!(
            infer_var_value("func f():\n\tvar x = (42)\n"),
            Some(InferredType::Builtin("int"))
        );
    }

    // ── parse_class_db_type ─────────────────────────────────────

    #[test]
    fn parse_void() {
        assert_eq!(parse_class_db_type("void"), InferredType::Void);
    }

    #[test]
    fn parse_variant() {
        assert_eq!(parse_class_db_type("Variant"), InferredType::Variant);
    }

    #[test]
    fn parse_builtin() {
        assert_eq!(parse_class_db_type("int"), InferredType::Builtin("int"));
        assert_eq!(parse_class_db_type("float"), InferredType::Builtin("float"));
        assert_eq!(parse_class_db_type("bool"), InferredType::Builtin("bool"));
    }

    #[test]
    fn parse_class() {
        assert_eq!(
            parse_class_db_type("CharacterBody2D"),
            InferredType::Class("CharacterBody2D".to_string())
        );
    }

    #[test]
    fn parse_typed_array() {
        assert_eq!(
            parse_class_db_type("typedarray::Node"),
            InferredType::TypedArray(Box::new(InferredType::Class("Node".to_string())))
        );
    }

    #[test]
    fn parse_enum() {
        assert_eq!(
            parse_class_db_type("enum::Error"),
            InferredType::Enum("Error".to_string())
        );
    }

    // ── display_name ────────────────────────────────────────────

    #[test]
    fn display_name_builtin() {
        assert_eq!(InferredType::Builtin("int").display_name(), "int");
    }

    #[test]
    fn display_name_typed_array() {
        let t = InferredType::TypedArray(Box::new(InferredType::Builtin("int")));
        assert_eq!(t.display_name(), "Array[int]");
    }

    #[test]
    fn display_name_void() {
        assert_eq!(InferredType::Void.display_name(), "void");
    }

    // ── is_numeric ──────────────────────────────────────────────

    #[test]
    fn is_numeric_int() {
        assert!(InferredType::Builtin("int").is_numeric());
    }

    #[test]
    fn is_numeric_float() {
        assert!(InferredType::Builtin("float").is_numeric());
    }

    #[test]
    fn is_numeric_string() {
        assert!(!InferredType::Builtin("String").is_numeric());
    }

    // ── Property access (builtin members) ─────────────────────────

    #[test]
    fn property_vector2_x() {
        let source = "\
var pos: Vector2
func f():
\tvar x = pos.x
";
        assert_eq!(
            infer_var_value(source),
            Some(InferredType::Builtin("float"))
        );
    }

    #[test]
    fn property_color_r() {
        let source = "\
var c: Color
func f():
\tvar x = c.r
";
        assert_eq!(
            infer_var_value(source),
            Some(InferredType::Builtin("float"))
        );
    }

    #[test]
    fn property_rect2_position() {
        let source = "\
var r: Rect2
func f():
\tvar x = r.position
";
        assert_eq!(
            infer_var_value(source),
            Some(InferredType::Builtin("Vector2"))
        );
    }

    #[test]
    fn property_transform3d_origin() {
        let source = "\
var t: Transform3D
func f():
\tvar x = t.origin
";
        assert_eq!(
            infer_var_value(source),
            Some(InferredType::Builtin("Vector3"))
        );
    }

    #[test]
    fn property_unknown_returns_none() {
        let source = "\
var v: Vector2
func f():
\tvar x = v.nonexistent
";
        assert_eq!(infer_var_value(source), None);
    }

    #[test]
    fn property_node2d_position_classdb() {
        let source = "\
var node: Node2D
func f():
\tvar x = node.position
";
        assert_eq!(
            infer_var_value(source),
            Some(InferredType::Builtin("Vector2"))
        );
    }

    // ── Type narrowing after `is` checks ──────────────────────────

    #[test]
    fn narrowing_direct_is_guard() {
        let source = "\
var event: InputEvent
func f():
\tif event is InputEventKey:
\t\tvar x = event
";
        // Inside `is` guard, event should be narrowed to InputEventKey
        assert_eq!(
            infer_var_value(source),
            Some(InferredType::Class("InputEventKey".to_string()))
        );
    }

    #[test]
    fn no_narrowing_without_guard() {
        let source = "\
var event: InputEvent
func f():
\tvar x = event
";
        assert_eq!(
            infer_var_value(source),
            Some(InferredType::Class("InputEvent".to_string()))
        );
    }

    // ── Preload/load type resolution ──────────────────────────────

    #[test]
    fn preload_tscn_returns_packed_scene() {
        assert_eq!(
            infer_var_value("func f():\n\tvar x = preload(\"res://scene.tscn\")\n"),
            Some(InferredType::Class("PackedScene".to_string()))
        );
    }

    #[test]
    fn load_tscn_returns_packed_scene() {
        assert_eq!(
            infer_var_value("func f():\n\tvar x = load(\"res://scene.tscn\")\n"),
            Some(InferredType::Class("PackedScene".to_string()))
        );
    }

    #[test]
    fn preload_png_returns_texture() {
        assert_eq!(
            infer_var_value("func f():\n\tvar x = preload(\"res://icon.png\")\n"),
            Some(InferredType::Class("Texture2D".to_string()))
        );
    }

    #[test]
    fn preload_gd_returns_gdscript() {
        assert_eq!(
            infer_var_value("func f():\n\tvar x = preload(\"res://script.gd\")\n"),
            Some(InferredType::Class("GDScript".to_string()))
        );
    }

    #[test]
    fn preload_shader_returns_shader() {
        assert_eq!(
            infer_var_value("func f():\n\tvar x = preload(\"res://effect.gdshader\")\n"),
            Some(InferredType::Class("Shader".to_string()))
        );
    }

    #[test]
    fn load_non_string_returns_resource() {
        assert_eq!(
            infer_var_value("func f():\n\tvar x = load(some_var)\n"),
            Some(InferredType::Class("Resource".to_string()))
        );
    }

    // ── Project-aware inference ────────────────────────────────────

    mod project_tests {
        use super::*;
        use crate::core::gd_ast;
        use crate::core::workspace_index;
        use std::path::PathBuf;

        fn infer_var_value_with_project(
            source: &str,
            project_files: &[(&str, &str)],
        ) -> Option<InferredType> {
            let root = PathBuf::from("/test_project");
            let file_entries: Vec<(PathBuf, &str)> = project_files
                .iter()
                .map(|(name, src)| (root.join(name), *src))
                .collect();
            let project = workspace_index::build_from_sources(&root, &file_entries, &[]);

            let tree = parser::parse(source).unwrap();
            let file = gd_ast::convert(&tree, source);
            let root_node = tree.root_node();
            find_first_var_value_project(&root_node, source, &file, &project)
        }

        fn find_first_var_value_project(
            node: &tree_sitter::Node,
            source: &str,
            file: &GdFile,
            project: &workspace_index::ProjectIndex,
        ) -> Option<InferredType> {
            if node.kind() == "variable_statement"
                && let Some(value) = node.child_by_field_name("value")
            {
                return infer_expression_type_with_project(&value, source, file, project);
            }
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if let Some(result) = find_first_var_value_project(&child, source, file, project) {
                    return Some(result);
                }
            }
            None
        }

        #[test]
        fn cross_file_base_class_method() {
            let source = "\
extends BaseEnemy
func f():
\tvar x = get_health()
";
            let result = infer_var_value_with_project(
                source,
                &[(
                    "base.gd",
                    "class_name BaseEnemy\nextends CharacterBody2D\nfunc get_health() -> int:\n\treturn 100\n",
                )],
            );
            assert_eq!(result, Some(InferredType::Builtin("int")));
        }

        #[test]
        fn cross_file_void_method() {
            let source = "\
extends BaseEnemy
func f():
\tvar x = take_damage()
";
            let result = infer_var_value_with_project(
                source,
                &[(
                    "base.gd",
                    "class_name BaseEnemy\nextends Node\nfunc take_damage() -> void:\n\tpass\n",
                )],
            );
            assert_eq!(result, Some(InferredType::Void));
        }

        #[test]
        fn cross_file_classdb_fallback() {
            let source = "\
extends MyNode
func f():
\tvar x = get_child(0)
";
            let result = infer_var_value_with_project(
                source,
                &[("mynode.gd", "class_name MyNode\nextends Node\n")],
            );
            // get_child is from ClassDB Node
            assert_eq!(result, Some(InferredType::Class("Node".to_string())));
        }

        #[test]
        fn cross_file_no_return_annotation() {
            let source = "\
extends Utils
func f():
\tvar x = compute()
";
            let result = infer_var_value_with_project(
                source,
                &[(
                    "utils.gd",
                    "class_name Utils\nextends Node\nfunc compute():\n\treturn 42\n",
                )],
            );
            assert_eq!(result, Some(InferredType::Variant));
        }

        #[test]
        fn preload_gd_without_class_name_is_gdscript() {
            let source = "\
extends Node
func f():
\tvar x = preload(\"res://enemy.gd\")
";
            // resolve_preload requires the file to exist on disk, so in-memory
            // tests fall back to extension-based resolution → GDScript
            let result = infer_var_value_with_project(
                source,
                &[(
                    "enemy.gd",
                    "class_name BaseEnemy\nextends CharacterBody2D\n",
                )],
            );
            assert_eq!(result, Some(InferredType::Class("GDScript".to_string())));
        }

        #[test]
        fn cross_file_property_access() {
            let source = "\
extends Node
var enemy: BaseEnemy
func f():
\tvar x = enemy.health
";
            let result = infer_var_value_with_project(
                source,
                &[(
                    "base.gd",
                    "class_name BaseEnemy\nextends CharacterBody2D\nvar health: int\n",
                )],
            );
            assert_eq!(result, Some(InferredType::Builtin("int")));
        }
    }
}
