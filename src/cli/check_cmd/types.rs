use tree_sitter::Node;

use crate::core::symbol_table::SymbolTable;
use crate::core::{symbol_table, type_inference};

use super::StructuralError;
use super::classdb::types_assignable;

// ---------------------------------------------------------------------------
// Round 4: B4 — Argument count mismatch
// ---------------------------------------------------------------------------

/// Try to infer a local variable's type by finding its declaration in the enclosing scope.
/// This handles `var v := Vector2()` patterns where `v` has an inferred type.
pub(super) fn infer_local_var_type(
    ident_node: &Node,
    source: &str,
    symbols: &SymbolTable,
) -> Option<type_inference::InferredType> {
    if ident_node.kind() != "identifier" {
        return None;
    }
    let name = ident_node.utf8_text(source.as_bytes()).ok()?;

    // Walk up to find the enclosing block/body, then scan its children for a var decl
    let mut current = ident_node.parent()?;
    loop {
        if matches!(current.kind(), "body" | "class_body" | "source") {
            break;
        }
        current = current.parent()?;
    }

    let mut cursor = current.walk();
    for child in current.children(&mut cursor) {
        if child.kind() == "variable_statement"
            && child.start_position().row < ident_node.start_position().row
            && let Some(name_node) = child.child_by_field_name("name")
            && let Ok(var_name) = name_node.utf8_text(source.as_bytes())
            && var_name == name
        {
            // Try explicit type annotation first
            if let Some(type_node) = child.child_by_field_name("type")
                && type_node.kind() != "inferred_type"
                && let Ok(type_text) = type_node.utf8_text(source.as_bytes())
            {
                return Some(type_inference::classify_type_name(type_text));
            }
            // Then infer from the initializer value
            if let Some(value) = child.child_by_field_name("value") {
                return type_inference::infer_expression_type(&value, source, symbols);
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Round 5: B1/B2/B5/B6 — Type mismatch checks
// ---------------------------------------------------------------------------

/// B1: Assignment type mismatch — `var x: int = "hello"` or `x = "hello"` where x is typed.
pub(super) fn check_assign_type_mismatch(
    root: &Node,
    source: &str,
    symbols: &SymbolTable,
    errors: &mut Vec<StructuralError>,
) {
    check_assign_type_in_node(root, source, symbols, errors);
}

fn check_assign_type_in_node(
    node: &Node,
    source: &str,
    symbols: &SymbolTable,
    errors: &mut Vec<StructuralError>,
) {
    // Check variable declarations with explicit type and initializer
    if node.kind() == "variable_statement"
        && let Some(type_node) = node.child_by_field_name("type")
        && type_node.kind() != "inferred_type"
        && let Ok(declared_type) = type_node.utf8_text(source.as_bytes())
        && !declared_type.starts_with("Array[") // typed arrays handled separately
        && let Some(value) = node.child_by_field_name("value")
        && let Some(actual) = type_inference::infer_expression_type(&value, source, symbols)
        && let Some(actual_name) = inferred_type_name(&actual)
        && !types_assignable(declared_type, actual_name)
    {
        errors.push(StructuralError {
            line: value.start_position().row as u32 + 1,
            column: value.start_position().column as u32 + 1,
            message: format!(
                "cannot assign a value of type \"{actual_name}\" to variable of type \"{declared_type}\"",
            ),
        });
    }

    // Check reassignment: x = "string" where x is typed as int
    if node.kind() == "assignment"
        && let Some(lhs) = node.child_by_field_name("left")
        && lhs.kind() == "identifier"
        && let Ok(var_name) = lhs.utf8_text(source.as_bytes())
        && let Some(rhs) = node.child_by_field_name("right")
    {
        // Check class-level variables first, then local variables
        let class_var_type = symbols
            .variables
            .iter()
            .find(|v| v.name == var_name)
            .and_then(|v| v.type_ann.as_ref())
            .filter(|t| !t.is_inferred && !t.name.is_empty())
            .map(|t| t.name.clone());
        let local_var_type = if class_var_type.is_none() {
            infer_local_var_type(&lhs, source, symbols)
                .and_then(|ty| inferred_type_name(&ty).map(String::from))
        } else {
            None
        };
        let declared_type = class_var_type.as_deref().or(local_var_type.as_deref());
        if let Some(declared_type) = declared_type
            && let Some(actual) = type_inference::infer_expression_type(&rhs, source, symbols)
            && let Some(actual_name) = inferred_type_name(&actual)
            && !types_assignable(declared_type, actual_name)
        {
            errors.push(StructuralError {
                line: rhs.start_position().row as u32 + 1,
                column: rhs.start_position().column as u32 + 1,
                message: format!(
                    "cannot assign a value of type \"{actual_name}\" to variable of type \"{declared_type}\"",
                ),
            });
        }
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            check_assign_type_in_node(&cursor.node(), source, symbols, errors);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

/// B2: Return type mismatch — `func f() -> int: return "hello"`.
pub(super) fn check_return_type_mismatch(
    root: &Node,
    source: &str,
    symbols: &SymbolTable,
    errors: &mut Vec<StructuralError>,
) {
    for func in &symbols.functions {
        let Some(ref ret_ann) = func.return_type else {
            continue;
        };
        if ret_ann.is_inferred
            || ret_ann.name.is_empty()
            || ret_ann.name == "Variant"
            || ret_ann.name == "void"
        {
            continue;
        }
        // Find the function definition node and check return statements
        check_return_in_func(root, source, symbols, func, &ret_ann.name, errors);
    }
}

fn check_return_in_func(
    root: &Node,
    source: &str,
    symbols: &SymbolTable,
    func: &symbol_table::FuncDecl,
    ret_type: &str,
    errors: &mut Vec<StructuralError>,
) {
    // Find the function_definition node for this function
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        if child.kind() == "function_definition"
            && let Some(name_node) = child.child_by_field_name("name")
            && let Ok(name) = name_node.utf8_text(source.as_bytes())
            && name == func.name
        {
            check_return_type_in_body(&child, source, symbols, ret_type, errors);
        }
    }
}

fn check_return_type_in_body(
    node: &Node,
    source: &str,
    symbols: &SymbolTable,
    ret_type: &str,
    errors: &mut Vec<StructuralError>,
) {
    if node.kind() == "return_statement"
        && let Some(expr) = node.named_child(0)
        && let Some(actual) = type_inference::infer_expression_type(&expr, source, symbols)
        && let Some(actual_name) = inferred_type_name(&actual)
        && !types_assignable(ret_type, actual_name)
    {
        errors.push(StructuralError {
            line: expr.start_position().row as u32 + 1,
            column: expr.start_position().column as u32 + 1,
            message: format!(
                "cannot return a value of type \"{actual_name}\" from function with return type \"{ret_type}\"",
            ),
        });
    }

    // Don't recurse into nested function definitions or lambdas
    if matches!(node.kind(), "function_definition" | "lambda")
        && node.parent().is_some_and(|p| p.kind() != "source")
    {
        return;
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            check_return_type_in_body(&cursor.node(), source, symbols, ret_type, errors);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

/// B5: Invalid binary operators — `"hello" + 5`, `true * false`, etc.
pub(super) fn check_invalid_operators(
    root: &Node,
    source: &str,
    symbols: &SymbolTable,
    errors: &mut Vec<StructuralError>,
) {
    check_operators_in_node(root, source, symbols, errors);
}

fn check_operators_in_node(
    node: &Node,
    source: &str,
    symbols: &SymbolTable,
    errors: &mut Vec<StructuralError>,
) {
    if node.kind() == "binary_operator"
        && let Some(left) = node.child_by_field_name("left")
        && let Some(right) = node.child_by_field_name("right")
        && let Some(op_node) = node.child_by_field_name("op")
        && let Ok(op) = op_node.utf8_text(source.as_bytes())
        && let Some(left_ty) = type_inference::infer_expression_type(&left, source, symbols)
        && let Some(right_ty) = type_inference::infer_expression_type(&right, source, symbols)
        && let Some(lt) = inferred_type_name(&left_ty)
        && let Some(rt) = inferred_type_name(&right_ty)
        && !operator_valid(op, lt, rt)
    {
        errors.push(StructuralError {
            line: node.start_position().row as u32 + 1,
            column: node.start_position().column as u32 + 1,
            message: format!("invalid operands \"{lt}\" and \"{rt}\" for operator \"{op}\"",),
        });
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            check_operators_in_node(&cursor.node(), source, symbols, errors);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

/// Check if a binary operator is valid for the given operand types.
fn operator_valid(op: &str, left: &str, right: &str) -> bool {
    // Variant is always compatible
    if left == "Variant" || right == "Variant" {
        return true;
    }
    match op {
        "+" | "-" => {
            // Numeric: int+int, int+float, float+float, float+int
            if is_numeric_type(left) && is_numeric_type(right) {
                return true;
            }
            // String + String
            if left == "String" && right == "String" {
                return true;
            }
            // Vector arithmetic (same type)
            if left == right && is_vector_type(left) {
                return true;
            }
            // Array + Array
            if left == "Array" && right == "Array" {
                return true;
            }
            // PackedByteArray + PackedByteArray (and other packed arrays)
            if left == right && left.starts_with("Packed") {
                return true;
            }
            false
        }
        "*" | "/" => {
            if is_numeric_type(left) && is_numeric_type(right) {
                return true;
            }
            // Vector * scalar, scalar * Vector
            if is_vector_type(left) && is_numeric_type(right) {
                return true;
            }
            if is_numeric_type(left) && is_vector_type(right) {
                return true;
            }
            // Vector * Vector (element-wise)
            if left == right && is_vector_type(left) {
                return true;
            }
            // Transform multiplication and composition
            if is_transform_type(left) && is_transform_type(right) {
                return true;
            }
            // Transform * Vector (apply transform)
            if is_transform_type(left) && is_vector_type(right) {
                return true;
            }
            if is_vector_type(left) && is_transform_type(right) {
                return true;
            }
            // Basis * Basis, Basis * Vector3
            if left == "Basis" || right == "Basis" {
                return true;
            }
            // String * int (repeat)
            if op == "*" && left == "String" && right == "int" {
                return true;
            }
            false
        }
        "%" => {
            // Numeric modulo
            if is_numeric_type(left) && is_numeric_type(right) {
                return true;
            }
            // GDScript string formatting: "Hello %s" % value
            if left == "String" {
                return true;
            }
            // Vector element-wise modulo
            left == right && is_vector_type(left)
        }
        "<" | ">" | "<=" | ">=" => {
            if is_numeric_type(left) && is_numeric_type(right) {
                return true;
            }
            if left == "String" && right == "String" {
                return true;
            }
            false
        }
        // ==, !=, and/or/&&/||, and unknown ops: always valid
        _ => true,
    }
}

fn is_numeric_type(ty: &str) -> bool {
    matches!(ty, "int" | "float")
}

fn is_vector_type(ty: &str) -> bool {
    matches!(
        ty,
        "Vector2" | "Vector2i" | "Vector3" | "Vector3i" | "Vector4" | "Vector4i" | "Color"
    )
}

fn is_transform_type(ty: &str) -> bool {
    matches!(ty, "Transform2D" | "Transform3D" | "Projection")
}

/// B6: Invalid cast — `x as Node` where x: int.
pub(super) fn check_invalid_cast(
    root: &Node,
    source: &str,
    symbols: &SymbolTable,
    errors: &mut Vec<StructuralError>,
) {
    check_cast_in_node(root, source, symbols, errors);
}

fn check_cast_in_node(
    node: &Node,
    source: &str,
    symbols: &SymbolTable,
    errors: &mut Vec<StructuralError>,
) {
    // `as` cast: can appear as `as_pattern`, `cast`, or `binary_operator` with op "as"
    let cast_parts: Option<(tree_sitter::Node, tree_sitter::Node)> =
        if matches!(node.kind(), "as_pattern" | "cast") {
            node.named_child(0)
                .and_then(|expr| node.named_child(1).map(|ty| (expr, ty)))
        } else if node.kind() == "binary_operator"
            && node
                .child_by_field_name("op")
                .and_then(|op| op.utf8_text(source.as_bytes()).ok())
                .is_some_and(|op| op == "as")
        {
            node.child_by_field_name("left")
                .and_then(|l| node.child_by_field_name("right").map(|r| (l, r)))
        } else {
            None
        };

    if let Some((expr, type_node)) = cast_parts
        && let Ok(target_type) = type_node.utf8_text(source.as_bytes())
        && let Some(expr_ty) = type_inference::infer_expression_type(&expr, source, symbols)
            .or_else(|| infer_local_var_type(&expr, source, symbols))
        && let Some(actual_name) = inferred_type_name(&expr_ty)
        && is_primitive_type(actual_name)
        && crate::class_db::class_exists(target_type)
        && !is_primitive_type(target_type)
    {
        errors.push(StructuralError {
            line: node.start_position().row as u32 + 1,
            column: node.start_position().column as u32 + 1,
            message: format!("invalid cast: cannot cast \"{actual_name}\" to \"{target_type}\"",),
        });
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            check_cast_in_node(&cursor.node(), source, symbols, errors);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

fn is_primitive_type(ty: &str) -> bool {
    matches!(ty, "int" | "float" | "bool" | "String")
}

/// Extract a human-readable type name from an `InferredType`.
pub(super) fn inferred_type_name(ty: &type_inference::InferredType) -> Option<&str> {
    match ty {
        type_inference::InferredType::Builtin(b) => Some(b),
        type_inference::InferredType::Class(c) => Some(c.as_str()),
        type_inference::InferredType::Enum(e) => Some(e.as_str()),
        type_inference::InferredType::Void => Some("void"),
        _ => None,
    }
}
