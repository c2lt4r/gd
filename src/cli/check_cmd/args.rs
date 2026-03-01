use tree_sitter::Node;

use crate::core::gd_ast::GdFile;
use crate::core::type_inference;
use crate::core::workspace_index::ProjectIndex;

use super::StructuralError;
use super::classdb::types_assignable;
use super::types::{infer_local_var_type, inferred_type_name};

// ---------------------------------------------------------------------------
// Round 5 continued: B3 — Argument type mismatch
// ---------------------------------------------------------------------------

/// B3: Argument type mismatch — wrong types passed to functions/methods/constructors.
pub(super) fn check_arg_type_mismatch(
    root: &Node,
    source: &str,
    file: &GdFile<'_>,
    project: &ProjectIndex,
    errors: &mut Vec<StructuralError>,
) {
    check_arg_types_in_node(root, source, file, project, errors);
}

fn check_arg_types_in_node(
    node: &Node,
    source: &str,
    file: &GdFile<'_>,
    project: &ProjectIndex,
    errors: &mut Vec<StructuralError>,
) {
    // Identifier calls: user functions, self methods, constructors
    if node.kind() == "call"
        && let Some(callee) = node.named_child(0)
        && let Ok(func_name) = callee.utf8_text(source.as_bytes())
        && let Some(args_node) = node.child_by_field_name("arguments")
    {
        // User-defined functions — check param types from GdFile
        if let Some(func) = file.funcs().find(|f| f.name == func_name) {
            check_call_arg_types_user(func_name, &func.params, &args_node, source, file, project, errors);
        }
        // Self methods via ClassDB (extends chain)
        else if let Some(extends) = file.extends_class() {
            check_call_arg_types_classdb(func_name, extends, &args_node, source, file, project, errors);
        }
        // Constructor: Vector2("bad", "args") or builtin conversion: int([])
        if callee.kind() == "identifier"
            && (crate::class_db::class_exists(func_name)
                || is_builtin_convertible(func_name)
                || constructor_param_counts(func_name).is_some())
        {
            check_constructor_arg_types(func_name, &args_node, source, file, project, errors);
        }
    }

    // attribute_call: obj.method(args)
    if node.kind() == "attribute" {
        let mut cursor2 = node.walk();
        for child in node.children(&mut cursor2) {
            if child.kind() == "attribute_call"
                && let Some(method_node) = child.named_child(0)
                && let Ok(method_name) = method_node.utf8_text(source.as_bytes())
                && let Some(args_node) = child.child_by_field_name("arguments")
            {
                // Infer receiver type
                if let Some(receiver) = node.named_child(0) {
                    let receiver_type =
                        type_inference::infer_expression_type_with_project(&receiver, source, file, project);
                    let class = receiver_type
                        .as_ref()
                        .and_then(|t| match t {
                            type_inference::InferredType::Class(c) => Some(c.as_str()),
                            type_inference::InferredType::Builtin(b) => Some(*b),
                            _ => None,
                        })
                        .or_else(|| {
                            let name = receiver.utf8_text(source.as_bytes()).ok()?;
                            if receiver.kind() == "identifier"
                                && crate::class_db::class_exists(name)
                            {
                                Some(name)
                            } else {
                                None
                            }
                        });
                    if let Some(class_name) = class {
                        check_call_arg_types_classdb(
                            method_name,
                            class_name,
                            &args_node,
                            source,
                            file,
                            project,
                            errors,
                        );
                    }
                }
            }
        }
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            check_arg_types_in_node(&cursor.node(), source, file, project, errors);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

/// Check user-defined function argument types.
fn check_call_arg_types_user(
    func_name: &str,
    params: &[crate::core::gd_ast::GdParam<'_>],
    args_node: &Node,
    source: &str,
    file: &GdFile<'_>,
    project: &ProjectIndex,
    errors: &mut Vec<StructuralError>,
) {
    let mut cursor = args_node.walk();
    let args: Vec<_> = args_node.named_children(&mut cursor).collect();
    for (i, arg) in args.iter().enumerate() {
        if let Some(param) = params.get(i)
            && let Some(ref type_ann) = param.type_ann
            && !type_ann.is_inferred
            && !type_ann.name.is_empty()
            && type_ann.name != "Variant"
            && let Some(actual) = type_inference::infer_expression_type_with_project(arg, source, file, project)
            && let Some(actual_name) = inferred_type_name(&actual)
            && !types_assignable(type_ann.name, actual_name)
        {
            errors.push(StructuralError {
                line: arg.start_position().row as u32 + 1,
                column: arg.start_position().column as u32 + 1,
                message: format!(
                    "invalid argument for \"{func_name}()\": argument {} should be \"{}\" but is \"{actual_name}\"",
                    i + 1,
                    type_ann.name,
                ),
            });
        }
    }
}

/// Check ClassDB method argument types.
fn check_call_arg_types_classdb(
    method_name: &str,
    class_name: &str,
    args_node: &Node,
    source: &str,
    file: &GdFile<'_>,
    project: &ProjectIndex,
    errors: &mut Vec<StructuralError>,
) {
    let Some(sig) = crate::class_db::method_signature(class_name, method_name) else {
        return;
    };
    if sig.param_types.is_empty() {
        return;
    }
    let param_types: Vec<&str> = sig.param_types.split(',').map(str::trim).collect();
    let mut cursor = args_node.walk();
    let args: Vec<_> = args_node.named_children(&mut cursor).collect();
    for (i, arg) in args.iter().enumerate() {
        if let Some(&expected) = param_types.get(i)
            && !expected.is_empty()
            && expected != "Variant"
            && let Some(actual) = type_inference::infer_expression_type_with_project(arg, source, file, project)
            && let Some(actual_name) = inferred_type_name(&actual)
            && !types_assignable_classdb(expected, actual_name)
        {
            errors.push(StructuralError {
                line: arg.start_position().row as u32 + 1,
                column: arg.start_position().column as u32 + 1,
                message: format!(
                    "invalid argument for \"{method_name}()\": argument {} should be \"{expected}\" but is \"{actual_name}\"",
                    i + 1,
                ),
            });
        }
    }
}

pub(super) fn is_builtin_convertible(name: &str) -> bool {
    matches!(name, "int" | "float" | "bool" | "String" | "str")
}

/// Check constructor argument types (e.g., `Vector2("bad", "args")` or `int([])`).
fn check_constructor_arg_types(
    type_name: &str,
    args_node: &Node,
    source: &str,
    file: &GdFile<'_>,
    project: &ProjectIndex,
    errors: &mut Vec<StructuralError>,
) {
    let expected_types: Option<&[&str]> = match type_name {
        "Vector2" | "Vector2i" => Some(&["float", "float"]),
        "Vector3" | "Vector3i" => Some(&["float", "float", "float"]),
        "Vector4" | "Vector4i" => Some(&["float", "float", "float", "float"]),
        // Builtin conversions: int(x), float(x), bool(x) — x must be numeric, String, or bool
        "int" | "float" | "bool" => {
            let mut cursor = args_node.walk();
            let args: Vec<_> = args_node.named_children(&mut cursor).collect();
            if args.len() == 1
                && let Some(actual) =
                    type_inference::infer_expression_type_with_project(&args[0], source, file, project)
                && let Some(actual_name) = inferred_type_name(&actual)
                && !matches!(actual_name, "int" | "float" | "bool" | "String" | "Variant")
            {
                errors.push(StructuralError {
                    line: args[0].start_position().row as u32 + 1,
                    column: args[0].start_position().column as u32 + 1,
                    message: format!(
                        "no constructor of \"{type_name}\" matches the signature \"{type_name}({actual_name})\"",
                    ),
                });
            }
            return;
        }
        // Color(x) with 1 arg: only Color(String) and Color(Color) are valid
        "Color" => {
            let mut cursor = args_node.walk();
            let args: Vec<_> = args_node.named_children(&mut cursor).collect();
            if args.len() == 1
                && let Some(actual) =
                    type_inference::infer_expression_type_with_project(&args[0], source, file, project)
                && let Some(actual_name) = inferred_type_name(&actual)
                && !matches!(actual_name, "String" | "Color" | "Variant")
            {
                errors.push(StructuralError {
                    line: args[0].start_position().row as u32 + 1,
                    column: args[0].start_position().column as u32 + 1,
                    message: format!(
                        "no constructor of \"Color\" matches the signature \"Color({actual_name})\"",
                    ),
                });
            }
            return;
        }
        _ => None,
    };

    let Some(expected) = expected_types else {
        return;
    };

    let mut cursor = args_node.walk();
    let args: Vec<_> = args_node.named_children(&mut cursor).collect();

    // Only check when arg count matches this constructor variant
    if args.len() != expected.len() {
        return;
    }

    for (i, arg) in args.iter().enumerate() {
        if let Some(&expected_type) = expected.get(i)
            && let Some(actual) = type_inference::infer_expression_type_with_project(arg, source, file, project)
            && let Some(actual_name) = inferred_type_name(&actual)
            && !types_assignable(expected_type, actual_name)
        {
            errors.push(StructuralError {
                line: arg.start_position().row as u32 + 1,
                column: arg.start_position().column as u32 + 1,
                message: format!(
                    "no constructor of \"{type_name}\" matches: argument {} should be \"{expected_type}\" but is \"{actual_name}\"",
                    i + 1,
                ),
            });
            return; // Report once per constructor
        }
    }
}

/// ClassDB-aware type assignability check.
fn types_assignable_classdb(expected: &str, actual: &str) -> bool {
    let expected_clean = expected.strip_prefix("enum::").unwrap_or(expected);
    types_assignable(expected_clean, actual)
}

// ---------------------------------------------------------------------------
// Round 4: B4 — Argument count mismatch
// ---------------------------------------------------------------------------

/// Parse a utility function signature to extract param count.
/// E.g., "lerp(from: Variant, to: Variant, weight: Variant) -> Variant" → (3, 3)
fn parse_utility_param_count(sig: &str) -> (u8, u8) {
    let Some(paren_start) = sig.find('(') else {
        return (0, 0);
    };
    let Some(paren_end) = sig.find(')') else {
        return (0, 0);
    };
    let params = &sig[paren_start + 1..paren_end];
    if params.trim().is_empty() {
        return (0, 0);
    }
    // Handle vararg: `...` at end
    if params.contains("...") {
        return (0, 255);
    }
    let total = params.split(',').count() as u8;
    // Count params with default values
    let with_defaults = params.split(',').filter(|p| p.contains('=')).count() as u8;
    let required = total - with_defaults;
    (required, total)
}

/// Known constructor variants for builtin types. Returns list of valid param counts.
pub(super) fn constructor_param_counts(type_name: &str) -> Option<&'static [u8]> {
    match type_name {
        "Color" | "Plane" => Some(&[0, 1, 2, 3, 4]),
        "Vector2" | "Vector2i" => Some(&[0, 1, 2]),
        "Vector3" | "Vector3i" => Some(&[0, 1, 3]),
        "Vector4" | "Vector4i" | "Projection" => Some(&[0, 1, 4]),
        "Transform3D" | "AABB" => Some(&[0, 2]),
        "Basis" => Some(&[0, 3]),
        "Rect2" | "Rect2i" | "Quaternion" => Some(&[0, 2, 4]),
        "Transform2D" => Some(&[0, 2, 3]),
        _ => None,
    }
}

/// B4: Argument count mismatch for function/method calls.
pub(super) fn check_arg_count(
    root: &Node,
    source: &str,
    file: &GdFile<'_>,
    project: &ProjectIndex,
    errors: &mut Vec<StructuralError>,
) {
    check_arg_count_in_node(root, source, file, project, errors);
}

fn count_call_args(node: &Node) -> usize {
    // Find the arguments node — try field name first, then search named children
    let args_node = node.child_by_field_name("arguments").or_else(|| {
        (0..node.named_child_count())
            .filter_map(|i| node.named_child(i))
            .find(|c| c.kind() == "arguments")
    });
    let Some(args) = args_node else { return 0 };
    let mut count = 0;
    let mut cursor = args.walk();
    for child in args.children(&mut cursor) {
        if child.is_named() {
            count += 1;
        }
    }
    count
}

fn check_arg_count_in_node(
    node: &Node,
    source: &str,
    file: &GdFile<'_>,
    project: &ProjectIndex,
    errors: &mut Vec<StructuralError>,
) {
    if node.kind() == "call"
        && let Some(func_node) = node.named_child(0)
    {
        let arg_count = count_call_args(node);

        if func_node.kind() == "identifier"
            && let Ok(name) = func_node.utf8_text(source.as_bytes())
        {
            // 1. Check user-defined functions
            if let Some(func) = file.funcs().find(|f| f.name == name) {
                let required = func.params.iter().filter(|p| p.default.is_none()).count();
                let total = func.params.len();
                check_param_bounds(name, arg_count, required, total, node, errors);
            }
            // 2. Check utility/builtin functions
            else if let Some(uf) = crate::class_db::utility_function(name) {
                let (required, total) = parse_utility_param_count(uf.signature);
                if total < 255 {
                    check_param_bounds(
                        name,
                        arg_count,
                        required as usize,
                        total as usize,
                        node,
                        errors,
                    );
                }
            }
            // 3. Check builtin constructors (e.g., Color(0.5))
            else if let Some(valid_counts) = constructor_param_counts(name)
                && !valid_counts.contains(&(arg_count as u8))
            {
                errors.push(StructuralError {
                    line: node.start_position().row as u32 + 1,
                    column: node.start_position().column as u32 + 1,
                    message: format!(
                        "no constructor of \"{name}\" matches the given arguments ({arg_count} arguments)",
                    ),
                });
            }
        } else if func_node.kind() == "attribute"
            && let Some(receiver) = func_node.named_child(0)
            && let Some(method_node) = func_node.named_child(1)
            && method_node.kind() == "identifier"
            && let Ok(method_name) = method_node.utf8_text(source.as_bytes())
        {
            // Try standard type inference first, then fall back to local var lookup
            let ty = type_inference::infer_expression_type_with_project(&receiver, source, file, project)
                .or_else(|| infer_local_var_type(&receiver, source, file, project));
            let class_name = ty.as_ref().and_then(|t| match t {
                type_inference::InferredType::Builtin(b) => Some(*b),
                type_inference::InferredType::Class(c) => Some(c.as_str()),
                _ => None,
            });
            if let Some(class_name) = class_name {
                if let Some(sig) = crate::class_db::method_signature(class_name, method_name) {
                    check_param_bounds(
                        method_name,
                        arg_count,
                        sig.required_params as usize,
                        sig.total_params as usize,
                        node,
                        errors,
                    );
                } else if let Some(member) =
                    crate::lsp::builtins::lookup_member_for(class_name, method_name)
                    && member.kind == crate::lsp::builtins::MemberKind::Method
                {
                    let (required, total) = parse_utility_param_count(member.brief);
                    if total < 255 {
                        check_param_bounds(
                            method_name,
                            arg_count,
                            required as usize,
                            total as usize,
                            node,
                            errors,
                        );
                    }
                }
            }
        }
    }

    // Handle method calls via `attribute` + `attribute_call` pattern:
    // `v.lerp(args)` is parsed as: attribute { identifier("v"), attribute_call { ... } }
    if node.kind() == "attribute" {
        check_attribute_call_args(node, source, file, project, errors);
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            check_arg_count_in_node(&cursor.node(), source, file, project, errors);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

fn check_attribute_call_args(
    node: &Node,
    source: &str,
    file: &GdFile<'_>,
    project: &ProjectIndex,
    errors: &mut Vec<StructuralError>,
) {
    let Some(attr_call) = (0..node.named_child_count())
        .filter_map(|i| node.named_child(i))
        .find(|c| c.kind() == "attribute_call")
    else {
        return;
    };
    if let Some(receiver) = node.named_child(0)
        && receiver.kind() == "identifier"
        && let Some(method_ident) = attr_call.named_child(0)
        && method_ident.kind() == "identifier"
        && let Ok(method_name) = method_ident.utf8_text(source.as_bytes())
    {
        let arg_count = count_call_args(&attr_call);
        let ty = type_inference::infer_expression_type_with_project(&receiver, source, file, project)
            .or_else(|| infer_local_var_type(&receiver, source, file, project));
        let class_name = ty.as_ref().and_then(|t| match t {
            type_inference::InferredType::Builtin(b) => Some(*b),
            type_inference::InferredType::Class(c) => Some(c.as_str()),
            _ => None,
        });

        // Resolve through intermediate property accesses (e.g., `_timer.timeout.connect()`).
        // Collect identifiers between receiver and attribute_call — the last one is the
        // property whose type determines the actual receiver of the method call.
        let resolved_class = if let Some(base) = class_name {
            let mut intermediates = Vec::new();
            for i in 1..node.named_child_count() {
                if let Some(c) = node.named_child(i)
                    && c.kind() == "identifier"
                {
                    intermediates.push(c);
                }
            }
            if intermediates.is_empty() {
                // No intermediate properties — method called directly on receiver
                Some(base.to_string())
            } else {
                // Walk the property chain: resolve each intermediate on the current type
                let mut current_type = base.to_string();
                for prop_node in &intermediates {
                    let Ok(prop_name) = prop_node.utf8_text(source.as_bytes()) else {
                        break;
                    };
                    // Check if the property is a signal on the current type
                    if crate::class_db::signal_exists(&current_type, prop_name) {
                        current_type = "Signal".to_string();
                        continue;
                    }
                    // Check if it's a known property with a type in ClassDB
                    if let Some(prop_type) =
                        crate::class_db::property_type(&current_type, prop_name)
                    {
                        current_type = prop_type.to_string();
                        continue;
                    }
                    // Can't resolve further — bail out
                    return;
                }
                Some(current_type)
            }
        } else {
            None
        };

        if let Some(class_name) = resolved_class.as_deref() {
            if let Some(sig) = crate::class_db::method_signature(class_name, method_name) {
                check_param_bounds(
                    method_name,
                    arg_count,
                    sig.required_params as usize,
                    sig.total_params as usize,
                    node,
                    errors,
                );
            } else if let Some(member) =
                crate::lsp::builtins::lookup_member_for(class_name, method_name)
                && member.kind == crate::lsp::builtins::MemberKind::Method
            {
                let (required, total) = parse_utility_param_count(member.brief);
                if total < 255 {
                    check_param_bounds(
                        method_name,
                        arg_count,
                        required as usize,
                        total as usize,
                        node,
                        errors,
                    );
                }
            }
        }
    }
}

fn check_param_bounds(
    name: &str,
    arg_count: usize,
    required: usize,
    total: usize,
    node: &Node,
    errors: &mut Vec<StructuralError>,
) {
    if arg_count < required {
        errors.push(StructuralError {
            line: node.start_position().row as u32 + 1,
            column: node.start_position().column as u32 + 1,
            message: format!(
                "too few arguments for \"{name}()\" call — expected at least {required} but received {arg_count}",
            ),
        });
    } else if arg_count > total {
        errors.push(StructuralError {
            line: node.start_position().row as u32 + 1,
            column: node.start_position().column as u32 + 1,
            message: format!(
                "too many arguments for \"{name}()\" call — expected at most {total} but received {arg_count}",
            ),
        });
    }
}
