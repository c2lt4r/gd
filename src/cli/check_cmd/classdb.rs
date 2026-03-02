use tree_sitter::Node;

use crate::core::gd_ast::{GdClass, GdDecl, GdFile};
use crate::core::type_inference;
use crate::core::workspace_index::ProjectIndex;

use super::StructuralError;
use super::identifiers::resolve_to_classdb_type;
use super::structural::find_annotation_name;

fn check_onready_non_node(
    file: &GdFile,
    project: &ProjectIndex,
    errors: &mut Vec<StructuralError>,
) {
    let has_onready = file
        .vars()
        .any(|v| v.annotations.iter().any(|a| a.name == "onready"));
    if !has_onready {
        return;
    }

    // Check if extends chain reaches Node — resolve through project types
    let extends = file.extends_class().unwrap_or("RefCounted");
    let classdb_type = resolve_to_classdb_type(extends, project);
    if classdb_type == "Node" || crate::class_db::inherits(&classdb_type, "Node") {
        return;
    }

    // @onready is used but class doesn't extend Node
    for var in file.vars() {
        if var.annotations.iter().any(|a| a.name == "onready") {
            errors.push(StructuralError {
                line: var.node.start_position().row as u32 + 1,
                column: 1,
                message: format!(
                    "`@onready` can only be used in classes that extend `Node` (class extends `{extends}`)",
                ),
            });
        }
    }
}

// ---------------------------------------------------------------------------
// Batch 5: ClassDB / signature lookup checks
// ---------------------------------------------------------------------------

/// Batch 5: Check type annotations, class_name shadowing, enum shadowing.
pub fn check_classdb_errors(
    file: &GdFile,
    source: &str,
    project: &ProjectIndex,
) -> Vec<StructuralError> {
    let root = &file.node;
    let mut errors = Vec::new();
    check_class_name_shadows_native(file, &mut errors);
    check_enum_shadows_builtin(file, &mut errors);
    check_type_annotations_resolve(root, source, file, project, &mut errors);
    check_use_void_return(root, source, file, &mut errors);
    check_instance_method_on_class(root, source, &mut errors);
    check_virtual_override_signature(file, &mut errors);
    check_cyclic_inner_class(file, &mut errors);
    check_export_invalid_type(file, &mut errors);
    check_rpc_args(root, source, &mut errors);
    check_export_node_path_type(root, source, &mut errors);
    check_lambda_super(root, source, &mut errors);
    check_typed_array_wrong_element(root, source, file, project, &mut errors);
    check_callable_direct_call(root, source, file, &mut errors);
    check_for_on_non_iterable(root, source, file, project, &mut errors);
    super::args::check_arg_count(root, source, file, project, &mut errors);
    super::args::check_arg_type_mismatch(root, source, file, project, &mut errors);
    super::types::check_assign_type_mismatch(root, source, file, project, &mut errors);
    super::types::check_return_type_mismatch(root, source, file, project, &mut errors);
    super::types::check_invalid_operators(root, source, file, project, &mut errors);
    super::types::check_invalid_cast(root, source, file, project, &mut errors);
    super::identifiers::check_type_not_found(root, source, file, project, &mut errors);
    super::identifiers::check_method_not_found(root, source, file, project, &mut errors);
    super::identifiers::check_super_method_not_found(root, source, file, project, &mut errors);
    super::identifiers::check_undefined_identifiers(root, source, file, project, &mut errors);
    super::builtins::check_builtin_method_not_found(root, source, file, project, &mut errors);
    super::builtins::check_builtin_property_not_found(root, source, file, project, &mut errors);
    check_onready_non_node(file, project, &mut errors);
    errors
}

/// H5: `class_name` shadows a native Godot class.
fn check_class_name_shadows_native(file: &GdFile, errors: &mut Vec<StructuralError>) {
    if let Some(name) = file.class_name
        && crate::class_db::class_exists(name)
    {
        errors.push(StructuralError {
            line: 1,
            column: 1,
            message: format!("`class_name {name}` shadows the native Godot class `{name}`",),
        });
    }
    for inner in file.inner_classes() {
        if crate::class_db::class_exists(inner.name) {
            errors.push(StructuralError {
                line: 1,
                column: 1,
                message: format!(
                    "inner class `{}` shadows the native Godot class `{}`",
                    inner.name, inner.name,
                ),
            });
        }
        check_class_name_shadows_native_inner(inner, errors);
    }
}

fn check_class_name_shadows_native_inner(class: &GdClass, errors: &mut Vec<StructuralError>) {
    for inner in class.declarations.iter().filter_map(GdDecl::as_class) {
        if crate::class_db::class_exists(inner.name) {
            errors.push(StructuralError {
                line: 1,
                column: 1,
                message: format!(
                    "inner class `{}` shadows the native Godot class `{}`",
                    inner.name, inner.name,
                ),
            });
        }
        check_class_name_shadows_native_inner(inner, errors);
    }
}

/// G5: Enum name or member name shadows a builtin type.
fn check_enum_shadows_builtin(file: &GdFile, errors: &mut Vec<StructuralError>) {
    let builtin_types = [
        "bool",
        "int",
        "float",
        "String",
        "Vector2",
        "Vector2i",
        "Vector3",
        "Vector3i",
        "Vector4",
        "Vector4i",
        "Rect2",
        "Rect2i",
        "Transform2D",
        "Transform3D",
        "Plane",
        "Quaternion",
        "AABB",
        "Basis",
        "Projection",
        "Color",
        "NodePath",
        "StringName",
        "RID",
        "Callable",
        "Signal",
        "Dictionary",
        "Array",
        "PackedByteArray",
        "PackedInt32Array",
        "PackedInt64Array",
        "PackedFloat32Array",
        "PackedFloat64Array",
        "PackedStringArray",
        "PackedVector2Array",
        "PackedVector3Array",
        "PackedColorArray",
        "PackedVector4Array",
        "Nil",
        "Object",
    ];
    for e in file.enums() {
        if !e.name.is_empty() && builtin_types.contains(&e.name) {
            errors.push(StructuralError {
                line: e.node.start_position().row as u32 + 1,
                column: 1,
                message: format!(
                    "enum `{name}` shadows the built-in type `{name}`",
                    name = e.name,
                ),
            });
        }
    }
    for inner in file.inner_classes() {
        check_enum_shadows_builtin_inner(inner, &builtin_types, errors);
    }
}

fn check_enum_shadows_builtin_inner(
    class: &GdClass,
    builtin_types: &[&str],
    errors: &mut Vec<StructuralError>,
) {
    for e in class.declarations.iter().filter_map(GdDecl::as_enum) {
        if !e.name.is_empty() && builtin_types.contains(&e.name) {
            errors.push(StructuralError {
                line: e.node.start_position().row as u32 + 1,
                column: 1,
                message: format!(
                    "enum `{name}` shadows the built-in type `{name}`",
                    name = e.name,
                ),
            });
        }
    }
    for inner in class.declarations.iter().filter_map(GdDecl::as_class) {
        check_enum_shadows_builtin_inner(inner, builtin_types, errors);
    }
}

/// A4: Type annotation doesn't resolve to a known type.
fn check_type_annotations_resolve(
    root: &Node,
    source: &str,
    file: &GdFile,
    project: &ProjectIndex,
    errors: &mut Vec<StructuralError>,
) {
    check_type_annotations_in_node(*root, source, file, project, errors);
}

fn check_type_annotations_in_node(
    node: Node,
    source: &str,
    file: &GdFile,
    project: &ProjectIndex,
    errors: &mut Vec<StructuralError>,
) {
    let bytes = source.as_bytes();

    // Check typed parameters, typed variables, return types
    let type_node = match node.kind() {
        "typed_parameter"
        | "typed_default_parameter"
        | "variable_statement"
        | "const_statement" => node.child_by_field_name("type"),
        "function_definition" | "constructor_definition" => node.child_by_field_name("return_type"),
        _ => None,
    };

    if let Some(type_node) = type_node
        && type_node.kind() != "inferred_type"
        && let Ok(type_name) = type_node.utf8_text(bytes)
    {
        // Strip Array[T] / Dictionary[K, V] wrappers — check each element type
        if let Some(inner) = type_name
            .strip_prefix("Array[")
            .and_then(|s| s.strip_suffix(']'))
        {
            if !inner.is_empty() && !is_known_type(inner, file, project) {
                let pos = type_node.start_position();
                errors.push(StructuralError {
                    line: pos.row as u32 + 1,
                    column: pos.column as u32 + 1,
                    message: format!("unknown type `{inner}` in type annotation"),
                });
            }
            // Walk children for further nested annotations
            let mut cursor = node.walk();
            if cursor.goto_first_child() {
                loop {
                    check_type_annotations_in_node(cursor.node(), source, file, project, errors);
                    if !cursor.goto_next_sibling() {
                        break;
                    }
                }
            }
            return;
        }
        if let Some(inner) = type_name
            .strip_prefix("Dictionary[")
            .and_then(|s| s.strip_suffix(']'))
        {
            // Check each type in "K, V"
            for part in inner.split(',') {
                let part = part.trim();
                if !part.is_empty() && !is_known_type(part, file, project) {
                    let pos = type_node.start_position();
                    errors.push(StructuralError {
                        line: pos.row as u32 + 1,
                        column: pos.column as u32 + 1,
                        message: format!("unknown type `{part}` in type annotation"),
                    });
                }
            }
            let mut cursor = node.walk();
            if cursor.goto_first_child() {
                loop {
                    check_type_annotations_in_node(cursor.node(), source, file, project, errors);
                    if !cursor.goto_next_sibling() {
                        break;
                    }
                }
            }
            return;
        }
        let base_type = type_name;

        if !base_type.is_empty() && !is_known_type(base_type, file, project) {
            // Handle inner class types: `ClassName.InnerClass` — check if the
            // base part before the dot is a known class.
            if base_type.contains('.') {
                let root_type = base_type.split('.').next().unwrap_or("");
                if is_known_type(root_type, file, project) {
                    // Base class exists; assume the inner class is valid
                } else {
                    let pos = type_node.start_position();
                    errors.push(StructuralError {
                        line: pos.row as u32 + 1,
                        column: pos.column as u32 + 1,
                        message: format!("unknown type `{base_type}` in type annotation"),
                    });
                }
            } else {
                let pos = type_node.start_position();
                errors.push(StructuralError {
                    line: pos.row as u32 + 1,
                    column: pos.column as u32 + 1,
                    message: format!("unknown type `{base_type}` in type annotation"),
                });
            }
        }
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            check_type_annotations_in_node(cursor.node(), source, file, project, errors);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

/// Check if a type name is known (builtin, ClassDB, user class, enum, or inner class).
pub(super) fn is_known_type(name: &str, file: &GdFile, project: &ProjectIndex) -> bool {
    // GDScript built-in types
    let builtins = [
        "void",
        "bool",
        "int",
        "float",
        "String",
        "Variant",
        "Vector2",
        "Vector2i",
        "Vector3",
        "Vector3i",
        "Vector4",
        "Vector4i",
        "Rect2",
        "Rect2i",
        "Transform2D",
        "Transform3D",
        "Plane",
        "Quaternion",
        "AABB",
        "Basis",
        "Projection",
        "Color",
        "NodePath",
        "StringName",
        "RID",
        "Callable",
        "Signal",
        "Dictionary",
        "Array",
        "PackedByteArray",
        "PackedInt32Array",
        "PackedInt64Array",
        "PackedFloat32Array",
        "PackedFloat64Array",
        "PackedStringArray",
        "PackedVector2Array",
        "PackedVector3Array",
        "PackedColorArray",
        "PackedVector4Array",
        "Object",
    ];
    if builtins.contains(&name) {
        return true;
    }

    // ClassDB class
    if crate::class_db::class_exists(name) {
        return true;
    }

    // User-defined class in project
    if project.lookup_class(name).is_some() {
        return true;
    }

    // Autoload
    if project.is_autoload(name) {
        return true;
    }

    // Same-file enums
    if file.enums().any(|e| e.name == name) {
        return true;
    }

    // Inner classes
    if file.inner_classes().any(|c| c.name == name) {
        return true;
    }

    // Enum types defined in other project files (cross-file enum namespaces)
    if project.has_enum_type(name) {
        return true;
    }

    // @GlobalScope enum types (Error, Corner, EulerOrder, PropertyHint, etc.)
    if crate::class_db::enum_type_exists("@GlobalScope", name) {
        return true;
    }

    // Dotted type: Class.EnumType or Class.InnerClass (e.g., BaseMaterial3D.BillboardMode)
    if let Some((class, member)) = name.split_once('.') {
        // ClassDB class with enum type
        if crate::class_db::class_exists(class) && crate::class_db::enum_type_exists(class, member)
        {
            return true;
        }
        // Project class with enum
        if let Some(file_syms) = project.lookup_class(class)
            && file_syms.enums.iter().any(|e| e == member)
        {
            return true;
        }
    }

    false
}

// ---------------------------------------------------------------------------
// Round 2: New ClassDB / semantic checks
// ---------------------------------------------------------------------------

/// C3 (extended): Using the return value of a void function.
fn check_use_void_return(
    root: &Node,
    source: &str,
    file: &GdFile,
    errors: &mut Vec<StructuralError>,
) {
    check_use_void_in_node(*root, source, file, errors);
}

fn check_use_void_in_node(
    node: Node,
    source: &str,
    file: &GdFile,
    errors: &mut Vec<StructuralError>,
) {
    let bytes = source.as_bytes();
    let extends = file.extends_class().unwrap_or("RefCounted");

    // Helper: check if a call node is a void call (bare identifier calls)
    let is_void_call = |call_node: &Node| -> Option<String> {
        let func = call_node
            .child_by_field_name("function")
            .or_else(|| call_node.named_child(0))?;
        if func.kind() == "identifier" {
            let func_name = func.utf8_text(bytes).ok()?;
            // User-defined void functions
            if file.funcs().any(|f| {
                f.name == func_name && f.return_type.as_ref().is_some_and(|r| r.name == "void")
            }) {
                return Some(func_name.to_string());
            }
            // ClassDB methods (bare call = self method)
            if crate::class_db::method_return_type(extends, func_name) == Some("void") {
                return Some(func_name.to_string());
            }
            // Void utility functions
            if is_void_utility(func_name) {
                return Some(func_name.to_string());
            }
        }
        None
    };

    // Check: var x = void_func()
    if node.kind() == "variable_statement"
        && let Some(value) = node.child_by_field_name("value")
        && value.kind() == "call"
        && let Some(func_name) = is_void_call(&value)
    {
        let pos = value.start_position();
        errors.push(StructuralError {
            line: pos.row as u32 + 1,
            column: pos.column as u32 + 1,
            message: format!("cannot use return value of `{func_name}()` because it returns void",),
        });
    }

    // Check: void_func() used as argument inside another call
    // e.g. print(void_func()) or some_func(a, void_func(), b)
    if node.kind() == "call"
        && let Some(args) = node.child_by_field_name("arguments")
    {
        let mut arg_cursor = args.walk();
        for arg in args.children(&mut arg_cursor) {
            if arg.kind() == "call"
                && let Some(func_name) = is_void_call(&arg)
            {
                let pos = arg.start_position();
                errors.push(StructuralError {
                    line: pos.row as u32 + 1,
                    column: pos.column as u32 + 1,
                    message: format!(
                        "cannot use return value of `{func_name}()` as argument because it returns void",
                    ),
                });
            }
        }
    }

    // Don't recurse into function definitions
    if node.kind() == "function_definition"
        || node.kind() == "constructor_definition"
        || node.kind() == "lambda"
    {
        // Still need to recurse into body
        if let Some(body) = node.child_by_field_name("body") {
            let mut cursor = body.walk();
            if cursor.goto_first_child() {
                loop {
                    check_use_void_in_node(cursor.node(), source, file, errors);
                    if !cursor.goto_next_sibling() {
                        break;
                    }
                }
            }
        }
        return;
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            check_use_void_in_node(cursor.node(), source, file, errors);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

/// Utility functions that return void and should not be used as values.
fn is_void_utility(name: &str) -> bool {
    matches!(
        name,
        "print"
            | "push_error"
            | "push_warning"
            | "printerr"
            | "prints"
            | "printraw"
            | "print_rich"
            | "print_debug"
            | "print_stack"
            | "seed"
    )
}

/// C7: Calling a non-static method on a class name (e.g., `Node.get_children()`).
fn check_instance_method_on_class(root: &Node, source: &str, errors: &mut Vec<StructuralError>) {
    check_instance_method_in_node(*root, source, errors);
}

fn check_instance_method_in_node(node: Node, source: &str, errors: &mut Vec<StructuralError>) {
    let bytes = source.as_bytes();

    // Pattern: attribute > identifier(ClassName) + attribute_call > identifier(method_name)
    if node.kind() == "attribute"
        && let Some(lhs) = node.named_child(0)
        && lhs.kind() == "identifier"
        && let Ok(class_name) = lhs.utf8_text(bytes)
        && crate::class_db::class_exists(class_name)
        && !crate::class_db::is_singleton(class_name)
    {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "attribute_call"
                && let Some(name_node) = child.named_child(0)
                && let Ok(method_name) = name_node.utf8_text(bytes)
                && method_name != "new"
                && crate::class_db::method_exists(class_name, method_name)
                && !crate::class_db::method_is_static(class_name, method_name)
            {
                let pos = node.start_position();
                errors.push(StructuralError {
                    line: pos.row as u32 + 1,
                    column: pos.column as u32 + 1,
                    message: format!(
                        "cannot call non-static method `{method_name}()` on class `{class_name}` — use an instance instead",
                    ),
                });
            }
        }
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            check_instance_method_in_node(cursor.node(), source, errors);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

/// Well-known virtual method signatures not in ClassDB (they are part of the
/// Godot core but not in extension_api.json).
fn known_virtual_signature(name: &str) -> Option<(&'static str, u8)> {
    // (return_type, param_count)
    match name {
        "_to_string" => Some(("String", 0)),
        "_init" => Some(("void", 0)), // already checked by G4, but included for completeness
        "_notification" => Some(("void", 1)), // what: int
        "_get" | "_property_get_revert" => Some(("Variant", 1)),
        "_set" => Some(("bool", 2)),
        "_get_property_list" => Some(("Array", 0)),
        "_property_can_revert" => Some(("bool", 1)),
        _ => None,
    }
}

/// D1/D2: Virtual override signature checks — wrong return type or wrong param count.
fn check_virtual_override_signature(file: &GdFile, errors: &mut Vec<StructuralError>) {
    let extends = file.extends_class().unwrap_or("RefCounted");
    for func in file.funcs() {
        // Only check virtual overrides (functions starting with _)
        if !func.name.starts_with('_') {
            continue;
        }

        // Try ClassDB first, fall back to well-known virtuals
        let (ret_type, total) =
            if let Some(sig) = crate::class_db::method_signature(extends, func.name) {
                (sig.return_type, sig.total_params as usize)
            } else if let Some((ret, params)) = known_virtual_signature(func.name) {
                (ret, params as usize)
            } else {
                continue;
            };

        // D1: Wrong return type
        // Normalize enum:: prefix from ClassDB (e.g. "enum::Error" → "Error")
        let normalized_ret = ret_type.strip_prefix("enum::").unwrap_or(ret_type);
        if let Some(ref ret) = func.return_type
            && !ret.name.is_empty()
            && ret.name != "void"
            && normalized_ret != "Variant"
            && !types_assignable(normalized_ret, ret.name)
        {
            errors.push(StructuralError {
                line: func.node.start_position().row as u32 + 1,
                column: 1,
                message: format!(
                    "override `{}()` has return type `{}` but parent expects `{}`",
                    func.name, ret.name, ret_type,
                ),
            });
        }

        // D2: Wrong param count (skip _init — constructors can have their own signatures)
        let user_count = func.params.len();
        if func.name != "_init" && user_count != total {
            errors.push(StructuralError {
                line: func.node.start_position().row as u32 + 1,
                column: 1,
                message: format!(
                    "override `{}()` has {} parameter(s) but parent expects {}",
                    func.name, user_count, total,
                ),
            });
        }
    }
    for inner in file.inner_classes() {
        check_virtual_override_signature_inner(inner, errors);
    }
}

fn check_virtual_override_signature_inner(class: &GdClass, errors: &mut Vec<StructuralError>) {
    let extends = class
        .extends
        .as_ref()
        .and_then(|e| match e {
            crate::core::gd_ast::GdExtends::Class(c) => Some(*c),
            crate::core::gd_ast::GdExtends::Path(_) => None,
        })
        .unwrap_or("RefCounted");
    for func in class.declarations.iter().filter_map(GdDecl::as_func) {
        if !func.name.starts_with('_') {
            continue;
        }
        let (ret_type, total) =
            if let Some(sig) = crate::class_db::method_signature(extends, func.name) {
                (sig.return_type, sig.total_params as usize)
            } else if let Some((ret, params)) = known_virtual_signature(func.name) {
                (ret, params as usize)
            } else {
                continue;
            };
        let normalized_ret = ret_type.strip_prefix("enum::").unwrap_or(ret_type);
        if let Some(ref ret) = func.return_type
            && !ret.name.is_empty()
            && ret.name != "void"
            && normalized_ret != "Variant"
            && !types_assignable(normalized_ret, ret.name)
        {
            errors.push(StructuralError {
                line: func.node.start_position().row as u32 + 1,
                column: 1,
                message: format!(
                    "override `{}()` has return type `{}` but parent expects `{}`",
                    func.name, ret.name, ret_type,
                ),
            });
        }
        let user_count = func.params.len();
        if func.name != "_init" && user_count != total {
            errors.push(StructuralError {
                line: func.node.start_position().row as u32 + 1,
                column: 1,
                message: format!(
                    "override `{}()` has {} parameter(s) but parent expects {}",
                    func.name, user_count, total,
                ),
            });
        }
    }
    for inner in class.declarations.iter().filter_map(GdDecl::as_class) {
        check_virtual_override_signature_inner(inner, errors);
    }
}

/// D3: Cyclic inner class inheritance.
fn check_cyclic_inner_class(file: &GdFile, errors: &mut Vec<StructuralError>) {
    // Build a map of inner class name -> extends
    let extends_map: std::collections::HashMap<&str, &str> = file
        .inner_classes()
        .filter_map(|c| {
            c.extends.as_ref().and_then(|e| match e {
                crate::core::gd_ast::GdExtends::Class(ext) => Some((c.name, *ext)),
                crate::core::gd_ast::GdExtends::Path(_) => None,
            })
        })
        .collect();

    // Check for cycles: walk the extends chain, detect if we revisit a class
    let mut reported = std::collections::HashSet::new();
    for inner in file.inner_classes() {
        let name = inner.name;
        let mut visited = std::collections::HashSet::new();
        let mut current = name;
        while let Some(&parent) = extends_map.get(current) {
            if !visited.insert(parent) || parent == name {
                // Cycle detected — report only once
                if reported.insert(name) {
                    errors.push(StructuralError {
                        line: 1,
                        column: 1,
                        message: format!(
                            "cyclic inheritance: inner class `{name}` is involved in an inheritance cycle",
                        ),
                    });
                }
                break;
            }
            current = parent;
        }
    }
}

/// E2: `@export` with an invalid type (Object is not exportable).
fn check_export_invalid_type(file: &GdFile, errors: &mut Vec<StructuralError>) {
    for var in file.vars() {
        let has_export = var.annotations.iter().any(|a| a.name == "export");
        if !has_export {
            continue;
        }
        if let Some(ref type_ann) = var.type_ann
            && type_ann.name == "Object"
        {
            errors.push(StructuralError {
                line: var.node.start_position().row as u32 + 1,
                column: 1,
                message: format!(
                    "`@export` type `Object` is not a valid export type for variable `{}`",
                    var.name,
                ),
            });
        }
    }
    for inner in file.inner_classes() {
        check_export_invalid_type_inner(inner, errors);
    }
}

fn check_export_invalid_type_inner(class: &GdClass, errors: &mut Vec<StructuralError>) {
    for var in class.declarations.iter().filter_map(GdDecl::as_var) {
        let has_export = var.annotations.iter().any(|a| a.name == "export");
        if !has_export {
            continue;
        }
        if let Some(ref type_ann) = var.type_ann
            && type_ann.name == "Object"
        {
            errors.push(StructuralError {
                line: var.node.start_position().row as u32 + 1,
                column: 1,
                message: format!(
                    "`@export` type `Object` is not a valid export type for variable `{}`",
                    var.name,
                ),
            });
        }
    }
    for inner in class.declarations.iter().filter_map(GdDecl::as_class) {
        check_export_invalid_type_inner(inner, errors);
    }
}

/// E9: `@rpc` annotation with invalid arguments.
fn check_rpc_args(root: &Node, source: &str, errors: &mut Vec<StructuralError>) {
    check_rpc_in_node(*root, source, errors);
}

fn check_rpc_in_node(node: Node, source: &str, errors: &mut Vec<StructuralError>) {
    let bytes = source.as_bytes();
    let valid_rpc_args = [
        "call_local",
        "call_remote",
        "any_peer",
        "authority",
        "reliable",
        "unreliable",
        "unreliable_ordered",
    ];

    if node.kind() == "annotation"
        && let Some(id) = find_annotation_name(&node, source)
        && id == "rpc"
    {
        // Check all string arguments
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "arguments" {
                let mut ac = child.walk();
                for arg in child.named_children(&mut ac) {
                    if arg.kind() == "string"
                        && let Ok(raw) = arg.utf8_text(bytes)
                    {
                        let val = raw.trim_matches('"').trim_matches('\'');
                        if !valid_rpc_args.contains(&val) {
                            let pos = arg.start_position();
                            errors.push(StructuralError {
                                line: pos.row as u32 + 1,
                                column: pos.column as u32 + 1,
                                message: format!(
                                    "invalid `@rpc` argument `\"{val}\"` — expected one of: {}",
                                    valid_rpc_args.join(", "),
                                ),
                            });
                        }
                    }
                }
            }
        }
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            check_rpc_in_node(cursor.node(), source, errors);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

/// E10: `@export_node_path` with a type that doesn't extend Node.
fn check_export_node_path_type(root: &Node, source: &str, errors: &mut Vec<StructuralError>) {
    check_export_node_path_in_node(*root, source, errors);
}

fn check_export_node_path_in_node(node: Node, source: &str, errors: &mut Vec<StructuralError>) {
    let bytes = source.as_bytes();

    if node.kind() == "annotation"
        && let Some(id) = find_annotation_name(&node, source)
        && id == "export_node_path"
    {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "arguments" {
                let mut ac = child.walk();
                for arg in child.named_children(&mut ac) {
                    if arg.kind() == "string"
                        && let Ok(raw) = arg.utf8_text(bytes)
                    {
                        let type_name = raw.trim_matches('"').trim_matches('\'');
                        if !type_name.is_empty()
                            && !crate::class_db::inherits(type_name, "Node")
                            && type_name != "Node"
                        {
                            let pos = arg.start_position();
                            errors.push(StructuralError {
                                line: pos.row as u32 + 1,
                                column: pos.column as u32 + 1,
                                message: format!(
                                    "`@export_node_path` type `{type_name}` does not extend Node",
                                ),
                            });
                        }
                    }
                }
            }
        }
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            check_export_node_path_in_node(cursor.node(), source, errors);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Round 3: Medium checks
// ---------------------------------------------------------------------------

/// H3: `super` is not allowed inside lambda bodies.
fn check_lambda_super(root: &Node, source: &str, errors: &mut Vec<StructuralError>) {
    check_lambda_super_in_node(root, source, errors, false);
}

fn check_lambda_super_in_node(
    node: &Node,
    source: &str,
    errors: &mut Vec<StructuralError>,
    in_lambda: bool,
) {
    if node.kind() == "lambda" {
        // Recurse into the lambda body with in_lambda=true
        let mut cursor = node.walk();
        if cursor.goto_first_child() {
            loop {
                check_lambda_super_in_node(&cursor.node(), source, errors, true);
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
        }
        return;
    }

    if in_lambda
        && node.kind() == "identifier"
        && let Ok(name) = node.utf8_text(source.as_bytes())
        && name == "super"
    {
        errors.push(StructuralError {
            line: node.start_position().row as u32 + 1,
            column: node.start_position().column as u32 + 1,
            message: "cannot use `super` inside a lambda".to_string(),
        });
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            check_lambda_super_in_node(&cursor.node(), source, errors, in_lambda);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

/// H6: Typed array literal with wrong element types.
/// e.g., `var arr: Array[int] = ["string"]`
fn check_typed_array_wrong_element(
    root: &Node,
    source: &str,
    file: &GdFile,
    project: &ProjectIndex,
    errors: &mut Vec<StructuralError>,
) {
    check_typed_array_in_node(root, source, file, project, errors);
}

fn check_typed_array_in_node(
    node: &Node,
    source: &str,
    file: &GdFile,
    project: &ProjectIndex,
    errors: &mut Vec<StructuralError>,
) {
    // Look for variable/const declarations with typed array annotation and array literal initializer
    if matches!(node.kind(), "variable_statement" | "const_statement")
        && let Some(type_node) = node.child_by_field_name("type")
        && let Ok(type_text) = type_node.utf8_text(source.as_bytes())
        && let Some(element_type) = type_text
            .strip_prefix("Array[")
            .and_then(|s| s.strip_suffix(']'))
        && let Some(value_node) = node.child_by_field_name("value")
        && value_node.kind() == "array"
    {
        check_array_elements(&value_node, source, file, project, element_type, errors);
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            check_typed_array_in_node(&cursor.node(), source, file, project, errors);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

fn check_array_elements(
    array_node: &Node,
    source: &str,
    file: &GdFile,
    project: &ProjectIndex,
    expected_type: &str,
    errors: &mut Vec<StructuralError>,
) {
    let mut cursor = array_node.walk();
    for child in array_node.children(&mut cursor) {
        if !child.is_named() {
            continue;
        }
        let Some(actual) =
            type_inference::infer_expression_type_with_project(&child, source, file, project)
        else {
            continue;
        };
        let actual_name = match &actual {
            type_inference::InferredType::Builtin(b) => *b,
            type_inference::InferredType::Class(c) => c.as_str(),
            _ => continue,
        };
        if !types_assignable(expected_type, actual_name) {
            errors.push(StructuralError {
                line: child.start_position().row as u32 + 1,
                column: child.start_position().column as u32 + 1,
                message: format!(
                    "cannot include a value of type \"{actual_name}\" in Array[{expected_type}]",
                ),
            });
        }
    }
}

/// Check if a value type is assignable to a declared type.
pub(super) fn types_assignable(declared: &str, actual: &str) -> bool {
    if declared == actual || declared == "Variant" || actual == "Variant" {
        return true;
    }
    // Numeric coercions: int ↔ float (Godot allows both directions)
    if is_numeric_coercible(declared) && is_numeric_coercible(actual) {
        return true;
    }
    // int/float → bool coercion (valid in GDScript)
    if declared == "bool" && matches!(actual, "int" | "float") {
        return true;
    }
    // Enum ↔ int implicit conversion (enum values are int constants in GDScript)
    // An "enum name" is something that is not a builtin value type, not a ClassDB class,
    // and not an array/dict family — this covers user-defined enum names.
    if is_possible_enum_type(declared) && matches!(actual, "int" | "float") {
        return true;
    }
    if is_possible_enum_type(actual) && matches!(declared, "int" | "float") {
        return true;
    }
    // Godot implicit conversions: String ↔ StringName, String ↔ NodePath
    if matches!(
        (declared, actual),
        ("StringName" | "NodePath", "String") | ("String", "StringName" | "NodePath")
    ) {
        return true;
    }
    // Vector widening: Vector2i → Vector2, Vector3i → Vector3, etc.
    if is_vector_widening(declared, actual) {
        return true;
    }
    // Array/Dictionary family coercion (Godot converts implicitly)
    if is_array_family(declared) && is_array_family(actual) {
        return true;
    }
    if is_dict_family(declared) && is_dict_family(actual) {
        return true;
    }
    // Class inheritance: allow both upcast and downcast (Godot defers to runtime)
    if crate::class_db::class_exists(declared) && crate::class_db::class_exists(actual) {
        return crate::class_db::inherits(actual, declared)
            || crate::class_db::inherits(declared, actual);
    }
    // If one type is a ClassDB class and the other is a user class (not a primitive),
    // allow it — we can't verify user class hierarchies without project context.
    let is_decl_classdb = crate::class_db::class_exists(declared);
    let is_actual_classdb = crate::class_db::class_exists(actual);
    if is_decl_classdb != is_actual_classdb {
        let non_classdb = if is_decl_classdb { actual } else { declared };
        if !is_builtin_value_type(non_classdb) {
            return true;
        }
    }
    // If both types are user classes (neither in ClassDB nor builtin value types),
    // allow it — we can't verify subclass relationships without project context.
    if !is_decl_classdb
        && !is_actual_classdb
        && !is_builtin_value_type(declared)
        && !is_builtin_value_type(actual)
    {
        return true;
    }
    false
}

fn is_numeric_coercible(ty: &str) -> bool {
    matches!(ty, "int" | "float")
}

fn is_array_family(ty: &str) -> bool {
    ty == "Array" || ty.starts_with("Array[") || ty.starts_with("Packed")
}

fn is_dict_family(ty: &str) -> bool {
    ty == "Dictionary" || ty.starts_with("Dictionary[")
}

fn is_vector_widening(declared: &str, actual: &str) -> bool {
    matches!(
        (declared, actual),
        ("Vector2", "Vector2i")
            | ("Vector3", "Vector3i")
            | ("Vector4", "Vector4i")
            | ("Vector2i", "Vector2")
            | ("Vector3i", "Vector3")
            | ("Vector4i", "Vector4")
            | ("Rect2", "Rect2i")
            | ("Rect2i", "Rect2")
    )
}

fn is_builtin_value_type(ty: &str) -> bool {
    matches!(
        ty,
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
            | "Projection"
            | "RID"
            | "Callable"
            | "Signal"
            | "Dictionary"
            | "void"
    )
}

/// A type name that could be a user-defined enum: not a builtin value type,
/// not a ClassDB class, and not an array/dict family type.
fn is_possible_enum_type(ty: &str) -> bool {
    !is_builtin_value_type(ty)
        && !crate::class_db::class_exists(ty)
        && !is_array_family(ty)
        && !is_dict_family(ty)
        && !ty.is_empty()
        && ty != "Variant"
}

/// H16: Cannot call a variable directly — e.g. `f()` where `f: Callable`.
/// Godot requires `.call()` syntax for Callable-typed variables.
fn check_callable_direct_call(
    root: &Node,
    source: &str,
    file: &GdFile,
    errors: &mut Vec<StructuralError>,
) {
    // Collect class-level Callable variables
    let mut callable_names: Vec<String> = file
        .vars()
        .filter(|v| v.type_ann.as_ref().is_some_and(|t| t.name == "Callable"))
        .map(|v| v.name.to_string())
        .collect();
    check_callable_in_node(root, source, file, &mut callable_names, errors);
}

fn check_callable_in_node(
    node: &Node,
    source: &str,
    file: &GdFile,
    callable_names: &mut Vec<String>,
    errors: &mut Vec<StructuralError>,
) {
    // Track local variable declarations with type Callable
    if node.kind() == "variable_statement"
        && let Some(name_node) = node.child_by_field_name("name")
        && let Ok(var_name) = name_node.utf8_text(source.as_bytes())
        && let Some(type_node) = node.child_by_field_name("type")
        && let Ok(type_text) = type_node.utf8_text(source.as_bytes())
        && type_text == "Callable"
    {
        callable_names.push(var_name.to_string());
    }

    // Check call expressions — the callee is the first named child (no field name)
    if node.kind() == "call"
        && let Some(func_node) = node.named_child(0)
        && func_node.kind() == "identifier"
        && let Ok(name) = func_node.utf8_text(source.as_bytes())
        && !file.funcs().any(|f| f.name == name)
        && callable_names.iter().any(|cn| cn == name)
    {
        errors.push(StructuralError {
            line: node.start_position().row as u32 + 1,
            column: node.start_position().column as u32 + 1,
            message: format!(
                "function \"{name}()\" not found in base self — use `{name}.call()` for Callable variables",
            ),
        });
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            check_callable_in_node(&cursor.node(), source, file, callable_names, errors);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

/// B7: For-loop on a non-iterable type (e.g., `for i in true:`).
fn check_for_on_non_iterable(
    root: &Node,
    source: &str,
    file: &GdFile,
    project: &ProjectIndex,
    errors: &mut Vec<StructuralError>,
) {
    check_for_iterable_in_node(root, source, file, project, errors);
}

fn check_for_iterable_in_node(
    node: &Node,
    source: &str,
    file: &GdFile,
    project: &ProjectIndex,
    errors: &mut Vec<StructuralError>,
) {
    if node.kind() == "for_statement"
        && let Some(iter_node) = node.child_by_field_name("right")
        && let Some(ty) =
            type_inference::infer_expression_type_with_project(&iter_node, source, file, project)
        && !is_iterable_type(&ty)
    {
        let ty_name = match &ty {
            type_inference::InferredType::Builtin(b) => (*b).to_string(),
            type_inference::InferredType::Class(c) | type_inference::InferredType::Enum(c) => {
                c.clone()
            }
            type_inference::InferredType::Void => "void".to_string(),
            _ => return,
        };
        errors.push(StructuralError {
            line: iter_node.start_position().row as u32 + 1,
            column: iter_node.start_position().column as u32 + 1,
            message: format!("unable to iterate on value of type \"{ty_name}\""),
        });
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            check_for_iterable_in_node(&cursor.node(), source, file, project, errors);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

/// Check if a type is iterable in GDScript.
fn is_iterable_type(ty: &type_inference::InferredType) -> bool {
    match ty {
        type_inference::InferredType::Builtin(b) => matches!(
            *b,
            "int"
                | "float"
                | "String"
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
                | "Vector2"
                | "Vector2i"
                | "Vector3"
                | "Vector3i"
                | "Vector4"
                | "Vector4i"
        ),
        type_inference::InferredType::TypedArray(_) | type_inference::InferredType::Variant => true,
        // Typed dictionaries/arrays appear as Class("Dictionary[K, V]") or Class("Array[T]")
        type_inference::InferredType::Class(c) => {
            c.starts_with("Dictionary") || c.starts_with("Array")
        }
        _ => false,
    }
}
