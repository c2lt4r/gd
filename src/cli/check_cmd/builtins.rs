use tree_sitter::Node;

use crate::core::gd_ast::GdFile;
use crate::core::type_inference;

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
    errors: &mut Vec<StructuralError>,
) {
    check_builtin_method_in_node(root, source, file, errors);
}

fn check_builtin_method_in_node(
    node: &Node,
    source: &str,
    file: &GdFile<'_>,
    errors: &mut Vec<StructuralError>,
) {
    // Pattern: attribute { identifier(receiver), attribute_call { identifier(method), arguments } }
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
        let ty = type_inference::infer_expression_type(&receiver, source, file)
            .or_else(|| infer_local_var_type(&receiver, source, file));
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
            check_builtin_method_in_node(&cursor.node(), source, file, errors);
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
    errors: &mut Vec<StructuralError>,
) {
    check_builtin_property_in_node(root, source, file, errors);
}

fn check_builtin_property_in_node(
    node: &Node,
    source: &str,
    file: &GdFile<'_>,
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
        let ty = type_inference::infer_expression_type(&receiver, source, file)
            .or_else(|| infer_local_var_type(&receiver, source, file));
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
            check_builtin_property_in_node(&cursor.node(), source, file, errors);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}
