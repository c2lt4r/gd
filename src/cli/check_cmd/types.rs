use tree_sitter::Node;

use gd_core::gd_ast::GdFile;
use gd_core::type_inference;
use gd_core::workspace_index::ProjectIndex;

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
    file: &GdFile<'_>,
    project: &ProjectIndex,
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
                return type_inference::infer_expression_type_with_project(
                    &value, source, file, project,
                );
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
    file: &GdFile<'_>,
    project: &ProjectIndex,
    errors: &mut Vec<StructuralError>,
) {
    check_assign_type_in_node(root, source, file, project, errors);
}

fn check_assign_type_in_node(
    node: &Node,
    source: &str,
    file: &GdFile<'_>,
    project: &ProjectIndex,
    errors: &mut Vec<StructuralError>,
) {
    // Check variable declarations with explicit type and initializer
    if node.kind() == "variable_statement"
        && let Some(type_node) = node.child_by_field_name("type")
        && type_node.kind() != "inferred_type"
        && let Ok(declared_type) = type_node.utf8_text(source.as_bytes())
        && !declared_type.starts_with("Array[") // typed arrays handled separately
        && let Some(value) = node.child_by_field_name("value")
        && let Some(actual) = type_inference::infer_expression_type_with_project(&value, source, file, project)
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
        let class_var_type = file
            .vars()
            .find(|v| v.name == var_name)
            .and_then(|v| v.type_ann.as_ref())
            .filter(|t| !t.is_inferred && !t.name.is_empty())
            .map(|t| t.name.to_string());
        let local_var_type = if class_var_type.is_none() {
            infer_local_var_type(&lhs, source, file, project)
                .and_then(|ty| inferred_type_name(&ty).map(String::from))
        } else {
            None
        };
        let declared_type = class_var_type.as_deref().or(local_var_type.as_deref());
        if let Some(declared_type) = declared_type
            && let Some(actual) =
                type_inference::infer_expression_type_with_project(&rhs, source, file, project)
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
            check_assign_type_in_node(&cursor.node(), source, file, project, errors);
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
    file: &GdFile<'_>,
    project: &ProjectIndex,
    errors: &mut Vec<StructuralError>,
) {
    for func in file.funcs() {
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
        check_return_in_func(root, source, file, project, func, ret_ann.name, errors);
    }
}

fn check_return_in_func(
    root: &Node,
    source: &str,
    file: &GdFile<'_>,
    project: &ProjectIndex,
    func: &gd_core::gd_ast::GdFunc<'_>,
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
            check_return_type_in_body(&child, source, file, project, ret_type, errors);
        }
    }
}

fn check_return_type_in_body(
    node: &Node,
    source: &str,
    file: &GdFile<'_>,
    project: &ProjectIndex,
    ret_type: &str,
    errors: &mut Vec<StructuralError>,
) {
    if node.kind() == "return_statement"
        && let Some(expr) = node.named_child(0)
        && let Some(actual) =
            type_inference::infer_expression_type_with_project(&expr, source, file, project)
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
            check_return_type_in_body(&cursor.node(), source, file, project, ret_type, errors);
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
    file: &GdFile<'_>,
    project: &ProjectIndex,
    errors: &mut Vec<StructuralError>,
) {
    check_operators_in_node(root, source, file, project, errors);
}

fn check_operators_in_node(
    node: &Node,
    source: &str,
    file: &GdFile<'_>,
    project: &ProjectIndex,
    errors: &mut Vec<StructuralError>,
) {
    if node.kind() == "binary_operator"
        && let Some(left) = node.child_by_field_name("left")
        && let Some(right) = node.child_by_field_name("right")
        && let Some(op_node) = node.child_by_field_name("op")
        && let Ok(op) = op_node.utf8_text(source.as_bytes())
        && let Some(left_ty) =
            type_inference::infer_expression_type_with_project(&left, source, file, project)
        && let Some(right_ty) =
            type_inference::infer_expression_type_with_project(&right, source, file, project)
        && let Some(lt) = enum_normalized_type_name(&left_ty, file, project)
        && let Some(rt) = enum_normalized_type_name(&right_ty, file, project)
        && !operator_valid(op, lt, rt)
    {
        errors.push(StructuralError {
            line: node.start_position().row as u32 + 1,
            column: node.start_position().column as u32 + 1,
            message: format!("invalid operands \"{lt}\" and \"{rt}\" for operator \"{op}\"",),
        });
    }

    // Augmented assignment: +=, -=, *=, /=, %= imply a binary operation
    if node.kind() == "augmented_assignment"
        && let Some(lhs) = node.named_child(0)
        && let Some(rhs) = node.named_child(1)
    {
        // Extract the base operator from the augmented assignment text
        // The operator token (+=, -=, etc.) is an unnamed child between lhs and rhs
        let op = node.utf8_text(source.as_bytes()).ok().and_then(|text| {
            // Find the operator by looking for the += -= *= /= %= pattern
            for op_str in &["+=", "-=", "*=", "/=", "%="] {
                if text.contains(op_str) {
                    return Some(&op_str[..1]); // strip the '='
                }
            }
            None
        });

        if let Some(op) = op
            && let Some(left_ty) =
                type_inference::infer_expression_type_with_project(&lhs, source, file, project)
                    .or_else(|| infer_local_var_type(&lhs, source, file, project))
            && let Some(right_ty) =
                type_inference::infer_expression_type_with_project(&rhs, source, file, project)
            && let Some(lt) = enum_normalized_type_name(&left_ty, file, project)
            && let Some(rt) = enum_normalized_type_name(&right_ty, file, project)
            && !operator_valid(op, lt, rt)
        {
            errors.push(StructuralError {
                line: node.start_position().row as u32 + 1,
                column: node.start_position().column as u32 + 1,
                message: format!("invalid operands \"{lt}\" and \"{rt}\" for operator \"{op}=\"",),
            });
        }
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            check_operators_in_node(&cursor.node(), source, file, project, errors);
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
    // Equality/inequality, boolean logic, and unknown ops: always valid
    if matches!(op, "==" | "!=" | "and" | "or" | "&&" | "||") {
        return true;
    }
    // Consult the generated operator table from ClassDB
    if gd_class_db::operator_result_type(left, op, right).is_some() {
        return true;
    }
    // GDScript string formatting: "Hello %s" % value
    if op == "%" && left == "String" {
        return true;
    }
    false
}

/// B6: Invalid cast — `x as Node` where x: int.
pub(super) fn check_invalid_cast(
    root: &Node,
    source: &str,
    file: &GdFile<'_>,
    project: &ProjectIndex,
    errors: &mut Vec<StructuralError>,
) {
    check_cast_in_node(root, source, file, project, errors);
}

fn check_cast_in_node(
    node: &Node,
    source: &str,
    file: &GdFile<'_>,
    project: &ProjectIndex,
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
        && let Some(expr_ty) =
            type_inference::infer_expression_type_with_project(&expr, source, file, project)
                .or_else(|| infer_local_var_type(&expr, source, file, project))
        && let Some(actual_name) = inferred_type_name(&expr_ty)
    {
        let is_invalid = if is_primitive_type(actual_name) {
            // primitive → ClassDB class (e.g. int as Node)
            (gd_class_db::class_exists(target_type) && !is_primitive_type(target_type))
            // primitive → builtin container (e.g. int as Array)
            || is_builtin_container_type(target_type)
        } else if gd_class_db::class_exists(actual_name) || is_builtin_container_type(actual_name) {
            // class/container → primitive (e.g. RefCounted as int)
            is_primitive_type(target_type)
        } else {
            false
        };
        if is_invalid {
            errors.push(StructuralError {
                line: node.start_position().row as u32 + 1,
                column: node.start_position().column as u32 + 1,
                message: format!(
                    "invalid cast: cannot cast \"{actual_name}\" to \"{target_type}\"",
                ),
            });
        }
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            check_cast_in_node(&cursor.node(), source, file, project, errors);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

fn is_primitive_type(ty: &str) -> bool {
    matches!(ty, "int" | "float" | "bool" | "String")
}

fn is_builtin_container_type(ty: &str) -> bool {
    matches!(
        ty,
        "Array"
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
    ) || ty.starts_with("Array[")
}

/// Like `inferred_type_name`, but returns `"int"` for enum types.
/// GDScript enums are implicitly int-compatible, so arithmetic/comparison is valid.
fn enum_normalized_type_name<'a>(
    ty: &'a type_inference::InferredType,
    file: &GdFile<'_>,
    project: &ProjectIndex,
) -> Option<&'a str> {
    match ty {
        type_inference::InferredType::Enum(_) => Some("int"),
        type_inference::InferredType::Class(name) => {
            // Check if this "class" is actually a file-level or project enum
            if file.enums().any(|e| e.name == name.as_str()) || project.has_enum_type(name) {
                Some("int")
            } else {
                Some(name.as_str())
            }
        }
        _ => inferred_type_name(ty),
    }
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
