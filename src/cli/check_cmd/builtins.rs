use tree_sitter::Node;

use crate::core::gd_ast::GdFile;
use crate::core::type_inference;
use crate::core::workspace_index::ProjectIndex;

use super::StructuralError;
use super::types::infer_local_var_type;

/// Resolve the builtin type name from an InferredType, treating TypedArray as "Array".
fn resolve_builtin_type_name(ty: &type_inference::InferredType) -> Option<&str> {
    match ty {
        type_inference::InferredType::Builtin(b) => Some(b),
        type_inference::InferredType::Class(c) => Some(c.as_str()),
        type_inference::InferredType::TypedArray(_) => Some("Array"),
        _ => None,
    }
}

/// A2: Method not found on builtin type — `v.nonexistent()` where v: Vector2.
pub(super) fn check_builtin_method_not_found(
    root: &Node,
    source: &str,
    file: &GdFile<'_>,
    project: &ProjectIndex,
    errors: &mut Vec<StructuralError>,
) {
    check_builtin_method_in_node(root, source, file, project, errors);
}

fn check_builtin_method_in_node(
    node: &Node,
    source: &str,
    file: &GdFile<'_>,
    project: &ProjectIndex,
    errors: &mut Vec<StructuralError>,
) {
    // Pattern: attribute { identifier(receiver), [identifier(prop)...], attribute_call { identifier(method), arguments } }
    // tree-sitter-gdscript flattens property chains: `a.b.c.method()` → attribute(a, b, c, attribute_call(method, args))
    if node.kind() == "attribute"
        && let Some(receiver) = node.named_child(0)
        && receiver.kind() == "identifier"
        && let Some(attr_call) = (0..node.named_child_count())
            .filter_map(|i| node.named_child(i))
            .find(|c| c.kind() == "attribute_call")
        && let Some(method_ident) = attr_call.named_child(0)
        && method_ident.kind() == "identifier"
        && let Ok(method_name) = method_ident.utf8_text(source.as_bytes())
    {
        // Resolve receiver type, then walk intermediate property accesses
        let mut ty =
            type_inference::infer_expression_type_with_project(&receiver, source, file, project)
                .or_else(|| infer_local_var_type(&receiver, source, file, project));
        // Walk intermediate identifiers (property accesses) before the attribute_call
        let named_count = node.named_child_count();
        for i in 1..named_count {
            let Some(child) = node.named_child(i) else {
                break;
            };
            if child.kind() == "attribute_call" {
                break;
            }
            if child.kind() == "identifier"
                && let Ok(prop_name) = child.utf8_text(source.as_bytes())
            {
                ty = ty.and_then(|t| {
                    let type_name = resolve_builtin_type_name(&t)?;
                    type_inference::builtin_member_type(type_name, prop_name).or_else(|| {
                        crate::class_db::property_type(type_name, prop_name)
                            .map(type_inference::parse_class_db_type)
                    })
                });
            }
        }
        if let Some(ref ty) = ty
            && let Some(type_name) = resolve_builtin_type_name(ty)
            && type_inference::is_builtin_type(type_name)
            && !method_name.starts_with('_')
            && type_name != "Dictionary"
        {
            let exists = crate::lsp::builtins::lookup_member_for(type_name, method_name)
                .is_some_and(|m| m.kind == crate::lsp::builtins::MemberKind::Method);
            if !exists {
                errors.push(StructuralError {
                    line: method_ident.start_position().row as u32 + 1,
                    column: method_ident.start_position().column as u32 + 1,
                    message: format!(
                        "method \"{method_name}()\" not found on type \"{type_name}\"",
                    ),
                });
            }
        }
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            check_builtin_method_in_node(&cursor.node(), source, file, project, errors);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

/// A3: Property not found on builtin type — `v.zz` where v: Vector2.
pub(super) fn check_builtin_property_not_found(
    root: &Node,
    source: &str,
    file: &GdFile<'_>,
    project: &ProjectIndex,
    errors: &mut Vec<StructuralError>,
) {
    check_builtin_property_in_node(root, source, file, project, errors);
}

fn check_builtin_property_in_node(
    node: &Node,
    source: &str,
    file: &GdFile<'_>,
    project: &ProjectIndex,
    errors: &mut Vec<StructuralError>,
) {
    // Pattern: attribute { identifier(receiver), identifier(member) } — no attribute_call
    if node.kind() == "attribute"
        && let Some(receiver) = node.named_child(0)
        && receiver.kind() == "identifier"
        && let Some(member) = node.named_child(1)
        && member.kind() == "identifier"
        && !(0..node.named_child_count())
            .filter_map(|i| node.named_child(i))
            .any(|c| c.kind() == "attribute_call")
        && let Ok(member_name) = member.utf8_text(source.as_bytes())
    {
        let ty =
            type_inference::infer_expression_type_with_project(&receiver, source, file, project)
                .or_else(|| infer_local_var_type(&receiver, source, file, project));
        if let Some(ref ty) = ty
            && let Some(type_name) = resolve_builtin_type_name(ty)
            && type_inference::is_builtin_type(type_name)
            && !member_name.starts_with('_')
            && type_name != "Dictionary"
        {
            let exists = crate::lsp::builtins::lookup_member_for(type_name, member_name).is_some();
            let hardcoded = type_inference::builtin_member_type(type_name, member_name).is_some();
            if !exists && !hardcoded {
                errors.push(StructuralError {
                    line: member.start_position().row as u32 + 1,
                    column: member.start_position().column as u32 + 1,
                    message: format!("member \"{member_name}\" not found on type \"{type_name}\"",),
                });
            }
        }
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            check_builtin_property_in_node(&cursor.node(), source, file, project, errors);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}
