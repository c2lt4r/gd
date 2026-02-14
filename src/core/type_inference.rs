//! Expression-level type inference for GDScript AST nodes.
//!
//! Layer 2: given any expression node, determine its inferred type.
//! Builds on the per-file symbol table (Layer 1) and the engine ClassDB.

use tree_sitter::Node;

use super::symbol_table::SymbolTable;
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
pub fn infer_expression_type(
    node: &Node,
    source: &str,
    symbols: &SymbolTable,
) -> Option<InferredType> {
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
        "unary_operator" => infer_unary(node, source, symbols),

        // ── Binary operators ────────────────────────────────────────
        "binary_operator" => infer_binary(node, source, symbols),

        // ── Cast: `x as Node` ───────────────────────────────────────
        "as_pattern" | "cast" => infer_cast(node, source),

        // ── Ternary: `a if cond else b` ─────────────────────────────
        "conditional_expression" | "ternary_expression" => infer_ternary(node, source, symbols),

        // ── Parenthesized: `(expr)` → recurse ──────────────────────
        "parenthesized_expression" => node
            .named_child(0)
            .and_then(|inner| infer_expression_type(&inner, source, symbols)),

        // ── $Node / get_node ────────────────────────────────────────
        "get_node" => Some(InferredType::Class("Node".to_string())),

        // ── Identifiers → symbol table lookup ───────────────────────
        "identifier" => infer_identifier(node, source, symbols),

        // ── Function/constructor calls ──────────────────────────────
        "call" => infer_call(node, source, symbols),

        // ── Method calls: `obj.method()` ────────────────────────────
        "attribute" => infer_attribute(node, source, symbols),

        // ── Await: result type of the awaited expression ────────────
        "await_expression" => node
            .named_child(0)
            .and_then(|inner| infer_expression_type(&inner, source, symbols)),

        _ => None,
    }
}

/// Infer type of a unary operator expression.
fn infer_unary(node: &Node, source: &str, symbols: &SymbolTable) -> Option<InferredType> {
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
        .and_then(|operand| infer_expression_type(&operand, source, symbols))
}

/// Infer type of a binary operator expression.
fn infer_binary(node: &Node, source: &str, symbols: &SymbolTable) -> Option<InferredType> {
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

    // String concatenation
    if op_text == "+" {
        let left = node
            .child_by_field_name("left")
            .or_else(|| node.named_child(0));
        let right = node
            .child_by_field_name("right")
            .or_else(|| node.named_child(1));

        if let (Some(l), Some(r)) = (left, right) {
            let lt = infer_expression_type(&l, source, symbols);
            let rt = infer_expression_type(&r, source, symbols);

            match (&lt, &rt) {
                (Some(InferredType::Builtin("String")), Some(InferredType::Builtin("String"))) => {
                    return Some(InferredType::Builtin("String"));
                }
                // Arithmetic promotion: int+float or float+float → float
                (
                    Some(InferredType::Builtin("float")),
                    Some(InferredType::Builtin("int" | "float")),
                )
                | (Some(InferredType::Builtin("int")), Some(InferredType::Builtin("float"))) => {
                    return Some(InferredType::Builtin("float"));
                }
                // Same type arithmetic
                (Some(InferredType::Builtin("int")), Some(InferredType::Builtin("int"))) => {
                    return Some(InferredType::Builtin("int"));
                }
                _ => return lt.or(rt),
            }
        }
    }

    // Division always returns float in GDScript (except integer // which is not standard)
    if op_text == "/" {
        return Some(InferredType::Builtin("float"));
    }

    // Modulo, bit ops → int
    if matches!(op_text, "%" | "<<" | ">>" | "&" | "|" | "^") {
        return Some(InferredType::Builtin("int"));
    }

    // General arithmetic: infer from operands
    if matches!(op_text, "-" | "*" | "**") {
        let left = node
            .child_by_field_name("left")
            .or_else(|| node.named_child(0));
        let right = node
            .child_by_field_name("right")
            .or_else(|| node.named_child(1));

        if let (Some(l), Some(r)) = (left, right) {
            let lt = infer_expression_type(&l, source, symbols);
            let rt = infer_expression_type(&r, source, symbols);

            match (&lt, &rt) {
                (Some(InferredType::Builtin("float")), _)
                | (_, Some(InferredType::Builtin("float"))) => {
                    return Some(InferredType::Builtin("float"));
                }
                (Some(InferredType::Builtin("int")), Some(InferredType::Builtin("int"))) => {
                    return Some(InferredType::Builtin("int"));
                }
                _ => return lt.or(rt),
            }
        }
    }

    None
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
fn infer_ternary(node: &Node, source: &str, symbols: &SymbolTable) -> Option<InferredType> {
    // tree-sitter-gdscript: conditional_expression has 3 named children:
    // [0] = true branch, [1] = condition, [2] = false branch
    let true_branch = node.named_child(0)?;
    let false_branch = node.named_child(2).or_else(|| node.named_child(1))?;

    let true_type = infer_expression_type(&true_branch, source, symbols);
    let false_type = infer_expression_type(&false_branch, source, symbols);

    match (&true_type, &false_type) {
        (Some(a), Some(b)) if a == b => true_type,
        (None, Some(_)) => false_type,
        // (Some(_), None), mismatched, or both None: return true branch type
        _ => true_type,
    }
}

/// Infer type from an identifier by looking it up in the symbol table.
fn infer_identifier(node: &Node, source: &str, symbols: &SymbolTable) -> Option<InferredType> {
    let name = node.utf8_text(source.as_bytes()).ok()?;

    // Check class-level variables
    for var in &symbols.variables {
        if var.name == name {
            if let Some(ref type_ann) = var.type_ann
                && !type_ann.is_inferred
                && !type_ann.name.is_empty()
            {
                return Some(classify_type_name(&type_ann.name));
            }
            return None;
        }
    }

    // Check enum names (the enum itself is a type)
    for e in &symbols.enums {
        if e.name == name {
            return Some(InferredType::Enum(name.to_string()));
        }
    }

    None
}

/// Infer type from a function/constructor call.
fn infer_call(node: &Node, source: &str, symbols: &SymbolTable) -> Option<InferredType> {
    // tree-sitter-gdscript: call has `function` field, or first named child is identifier
    let func_node = node
        .child_by_field_name("function")
        .or_else(|| node.named_child(0))?;
    let func_name = func_node.utf8_text(source.as_bytes()).ok()?;

    // 1. Constructor calls (PascalCase): Vector2(...), Color(...), etc.
    if let Some(typ) = constructor_return_type(func_name) {
        return Some(typ);
    }

    // 2. GDScript builtin functions
    if let Some(typ) = builtin_function_return_type(func_name) {
        return Some(typ);
    }

    // 3. Self method calls — look up in symbol table, then ClassDB via extends chain
    for func in &symbols.functions {
        if func.name == func_name {
            return func.return_type.as_ref().map_or_else(
                || Some(InferredType::Variant),
                |ret| {
                    if ret.name == "void" {
                        Some(InferredType::Void)
                    } else {
                        Some(classify_type_name(&ret.name))
                    }
                },
            );
        }
    }

    // 4. ClassDB lookup via extends chain
    if let Some(extends) = &symbols.extends
        && let Some(ret_type) = class_db::method_return_type(extends, func_name)
    {
        return Some(parse_class_db_type(ret_type));
    }

    None
}

/// Infer type from an attribute expression (property access or method call).
///
/// tree-sitter pattern: `obj.method()` → `attribute` > [`identifier`, `attribute_call`]
fn infer_attribute(node: &Node, source: &str, symbols: &SymbolTable) -> Option<InferredType> {
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

    if !has_call {
        // Property access — we don't resolve properties yet (Layer 3)
        return None;
    }

    let method = method_name?;

    // Infer the receiver type
    let receiver = node.named_child(0)?;
    let receiver_type = infer_expression_type(&receiver, source, symbols)?;

    // Resolve the class name for ClassDB lookup
    let class_name = match &receiver_type {
        InferredType::Builtin(b) => *b,
        InferredType::Class(c) => c.as_str(),
        _ => return None,
    };

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
fn classify_type_name(name: &str) -> InferredType {
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
        // Resource-returning
        "preload" | "load" => Some(InferredType::Class("Resource".to_string())),
        // Collection-returning
        "range" => Some(InferredType::Builtin("Array")),
        _ => None,
    }
}

/// Check if a type name is a GDScript builtin type.
fn is_builtin_type(name: &str) -> bool {
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
    use crate::core::parser;
    use crate::core::symbol_table;

    /// Parse source, build symbol table, find the value node of the first variable
    /// statement, and infer its type.
    fn infer_var_value(source: &str) -> Option<InferredType> {
        let tree = parser::parse(source).unwrap();
        let symbols = symbol_table::build(&tree, source);
        let root = tree.root_node();
        find_first_var_value(&root, source, &symbols)
    }

    fn find_first_var_value(
        node: &Node,
        source: &str,
        symbols: &SymbolTable,
    ) -> Option<InferredType> {
        if node.kind() == "variable_statement"
            && let Some(value) = node.child_by_field_name("value")
        {
            return infer_expression_type(&value, source, symbols);
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if let Some(result) = find_first_var_value(&child, source, symbols) {
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
    fn builtin_preload() {
        assert_eq!(
            infer_var_value("func f():\n\tvar x = preload(\"res://scene.tscn\")\n"),
            Some(InferredType::Class("Resource".to_string()))
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
}
