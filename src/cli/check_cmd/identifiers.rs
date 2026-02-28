use tree_sitter::Node;

use super::StructuralError;
use super::args::{constructor_param_counts, is_builtin_convertible};
use super::classdb::is_known_type;
use crate::core::gd_ast::GdFile;
use crate::core::workspace_index::ProjectIndex;

// ---------------------------------------------------------------------------
// Round 6: A1-A4 — Name resolution
// ---------------------------------------------------------------------------

/// A4: Type not found in `as`/`is` expressions.
pub(super) fn check_type_not_found(
    root: &Node,
    source: &str,
    file: &GdFile,
    project: &ProjectIndex,
    errors: &mut Vec<StructuralError>,
) {
    check_type_not_found_in_node(root, source, file, project, errors);
}

fn check_type_not_found_in_node(
    node: &Node,
    source: &str,
    file: &GdFile,
    project: &ProjectIndex,
    errors: &mut Vec<StructuralError>,
) {
    // `binary_operator` with op "as" or "is"
    if node.kind() == "binary_operator"
        && let Some(op_node) = node.child_by_field_name("op")
        && let Ok(op) = op_node.utf8_text(source.as_bytes())
        && matches!(op, "as" | "is")
        && let Some(type_node) = node.child_by_field_name("right")
        && type_node.kind() == "identifier"
        && let Ok(type_name) = type_node.utf8_text(source.as_bytes())
        && !is_known_type(type_name, file, project)
    {
        errors.push(StructuralError {
            line: type_node.start_position().row as u32 + 1,
            column: type_node.start_position().column as u32 + 1,
            message: format!("could not find type \"{type_name}\" in the current scope",),
        });
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            check_type_not_found_in_node(&cursor.node(), source, file, project, errors);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

/// A2: Method not found — `get_chlidren()` on self, `s.nonexistent()` on typed variable.
pub(super) fn check_method_not_found(
    root: &Node,
    source: &str,
    file: &GdFile,
    project: &ProjectIndex,
    errors: &mut Vec<StructuralError>,
) {
    check_method_not_found_in_node(root, source, file, project, errors);
}

fn check_method_not_found_in_node(
    node: &Node,
    source: &str,
    file: &GdFile,
    project: &ProjectIndex,
    errors: &mut Vec<StructuralError>,
) {
    // Self method calls: `call` node with identifier callee
    if node.kind() == "call"
        && let Some(callee) = node.named_child(0)
        && callee.kind() == "identifier"
        && let Ok(func_name) = callee.utf8_text(source.as_bytes())
    {
        // Skip known identifiers: user functions, utility functions, constructors, etc.
        let is_known = file.funcs().any(|f| f.name == func_name)
            || crate::class_db::utility_function(func_name).is_some()
            || crate::class_db::class_exists(func_name)
            || crate::core::type_inference::is_builtin_type(func_name)
            || is_builtin_convertible(func_name)
            || constructor_param_counts(func_name).is_some()
            || matches!(
                func_name,
                "preload"
                    | "load"
                    | "print"
                    | "push_error"
                    | "push_warning"
                    | "range"
                    | "str"
                    | "typeof"
                    | "len"
                    | "assert"
                    | "super"
                    | "is_instance_valid"
                    | "is_instance_of"
                    | "weakref"
                    | "Color8"
                    | "print_debug"
                    | "print_stack"
                    | "get_stack"
                    | "inst_to_dict"
                    | "dict_to_inst"
                    | "type_string"
                    | "char"
                    | "ord"
                    | "convert"
            )
            || func_name.starts_with('_'); // Virtual callbacks
        if !is_known {
            // Check ProjectIndex for cross-file base class methods
            let extends = file.extends_class();
            let mut found = extends
                .is_some_and(|ext| project.method_exists(ext, func_name));
            // Resolve to ClassDB ancestor and check there
            if !found && let Some(ext) = extends {
                let classdb_ext = resolve_to_classdb_type(ext, project);
                found = crate::class_db::method_exists(&classdb_ext, func_name);
            }
            if !found {
                errors.push(StructuralError {
                    line: callee.start_position().row as u32 + 1,
                    column: callee.start_position().column as u32 + 1,
                    message: format!(
                        "function \"{func_name}()\" not found in base {}",
                        file.extends_class().unwrap_or("self"),
                    ),
                });
            }
        }
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            check_method_not_found_in_node(&cursor.node(), source, file, project, errors);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

/// A1: Undefined identifier — `nonexistent_variable` not declared.
pub(super) fn check_undefined_identifiers(
    root: &Node,
    source: &str,
    file: &GdFile,
    project: &ProjectIndex,
    errors: &mut Vec<StructuralError>,
) {
    // Build set of known names: class variables, functions, enums, inner classes, params
    let mut known: std::collections::HashSet<String> = std::collections::HashSet::new();

    // Class-level variables
    for v in file.vars() {
        known.insert(v.name.to_string());
    }
    // Functions
    for f in file.funcs() {
        known.insert(f.name.to_string());
    }
    // Enums and their members
    for e in file.enums() {
        known.insert(e.name.to_string());
        for member in &e.members {
            known.insert(member.name.to_string());
        }
    }
    // Inner classes
    for c in file.inner_classes() {
        known.insert(c.name.to_string());
    }
    // Signals
    for s in file.signals() {
        known.insert(s.name.to_string());
    }

    // Walk function bodies looking for undefined identifiers
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        if child.kind() == "function_definition"
            && let Some(body) = child.child_by_field_name("body")
        {
            // Collect function params (no `name` field — first named child is the identifier)
            let mut func_known = known.clone();
            if let Some(params) = child.child_by_field_name("parameters") {
                let mut pc = params.walk();
                for param in params.named_children(&mut pc) {
                    // Typed parameter: `name: Type` — first named child is the identifier
                    if let Some(name_node) = param.named_child(0)
                        && name_node.kind() == "identifier"
                        && let Ok(name) = name_node.utf8_text(source.as_bytes())
                    {
                        func_known.insert(name.to_string());
                    } else if param.kind() == "identifier"
                        && let Ok(name) = param.utf8_text(source.as_bytes())
                    {
                        // Untyped parameter: bare `name` — the param node IS the identifier
                        func_known.insert(name.to_string());
                    }
                }
            }
            // Also add function name itself to known
            if let Some(name_node) = child.child_by_field_name("name")
                && let Ok(fname) = name_node.utf8_text(source.as_bytes())
            {
                func_known.insert(fname.to_string());
            }
            check_undefined_in_body(&body, source, file, project, &mut func_known, errors);
        }
    }
}

fn check_undefined_in_body(
    node: &Node,
    source: &str,
    file: &GdFile,
    project: &ProjectIndex,
    known: &mut std::collections::HashSet<String>,
    errors: &mut Vec<StructuralError>,
) {
    // Track local variable declarations
    if node.kind() == "variable_statement"
        && let Some(name_node) = node.child_by_field_name("name")
        && let Ok(name) = name_node.utf8_text(source.as_bytes())
    {
        known.insert(name.to_string());
    }

    // Track for-loop iterator variable
    if node.kind() == "for_statement"
        && let Some(iter_node) = node.child_by_field_name("left")
        && let Ok(iter_name) = iter_node.utf8_text(source.as_bytes())
    {
        known.insert(iter_name.to_string());
    }

    // Check identifier usage
    if node.kind() == "identifier"
        && let Ok(name) = node.utf8_text(source.as_bytes())
        && !known.contains(name)
        && !is_identifier_context_ok(node, name, source, file, project)
    {
        errors.push(StructuralError {
            line: node.start_position().row as u32 + 1,
            column: node.start_position().column as u32 + 1,
            message: format!("identifier \"{name}\" not declared in the current scope",),
        });
    }

    // Don't recurse into nested function definitions (they have own scope)
    if matches!(node.kind(), "function_definition" | "lambda") {
        return;
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            check_undefined_in_body(&cursor.node(), source, file, project, known, errors);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

/// Check if an identifier node is the name part of a declaration, type annotation,
/// attribute member, or other non-reference context.
fn is_identifier_in_declaration_context(node: &Node, source: &str) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    // Variable/function/signal/class declaration name
    if parent
        .child_by_field_name("name")
        .is_some_and(|n| n.id() == node.id())
    {
        return true;
    }
    // Type annotation context
    if parent
        .child_by_field_name("type")
        .is_some_and(|t| t.id() == node.id())
    {
        return true;
    }
    // Return type
    if parent
        .child_by_field_name("return_type")
        .is_some_and(|t| t.id() == node.id())
    {
        return true;
    }
    // Method name inside attribute_call: always a member, not a receiver
    if parent.kind() == "attribute_call" {
        return true;
    }
    // Attribute access: non-first child is a member, not a receiver
    if parent.kind() == "attribute"
        && parent
            .named_child(0)
            .is_some_and(|first| first.id() != node.id())
    {
        return true;
    }
    // Annotation name (e.g. @warning_ignore, @export, @onready)
    if parent.kind() == "annotation" {
        return true;
    }
    // `as`/`is` type operand — already checked by A4
    if parent.kind() == "binary_operator"
        && parent
            .child_by_field_name("right")
            .is_some_and(|r| r.id() == node.id())
        && parent
            .child_by_field_name("op")
            .and_then(|op| op.utf8_text(source.as_bytes()).ok())
            .is_some_and(|op| matches!(op, "as" | "is"))
    {
        return true;
    }
    false
}

/// Check if an identifier is in a context where it doesn't need to be declared.
fn is_identifier_context_ok(
    node: &Node,
    name: &str,
    source: &str,
    file: &GdFile,
    project: &ProjectIndex,
) -> bool {
    // Skip builtins and well-known names
    if matches!(
        name,
        "self"
            | "super"
            | "true"
            | "false"
            | "null"
            | "PI"
            | "TAU"
            | "INF"
            | "NAN"
            | "OK"
            | "FAILED"
            | "ERR_UNAVAILABLE"
    ) {
        return true;
    }

    // Known type names (reuses existing comprehensive check)
    if is_known_type(name, file, project) {
        return true;
    }

    // Utility functions
    if crate::class_db::utility_function(name).is_some() {
        return true;
    }

    // Builtin convertible types used as constructors
    if is_builtin_convertible(name) || constructor_param_counts(name).is_some() {
        return true;
    }

    // Check parent context: is this identifier the NAME of a declaration?
    if is_identifier_in_declaration_context(node, source) {
        return true;
    }

    // Virtual callback names or underscore-prefixed
    if name.starts_with('_') {
        return true;
    }

    // Cross-file: methods/properties from project-defined base classes
    if let Some(ext) = file.extends_class()
        && (project.method_exists(ext, name) || project.variable_type(ext, name).is_some())
    {
        return true;
    }

    // ClassDB: extends chain for properties, methods, and constants/enums.
    // Resolve through project extends chain to find the ClassDB ancestor —
    // handles both direct ClassDB types and path-based extends.
    if let Some(ext) = file.extends_class() {
        let classdb_ext = resolve_to_classdb_type(ext, project);
        if crate::class_db::property_exists(&classdb_ext, name)
            || crate::class_db::method_exists(&classdb_ext, name)
            || crate::class_db::constant_exists(&classdb_ext, name)
        {
            return true;
        }
    }

    // Singletons used as identifiers (e.g., passing Input as argument)
    if crate::class_db::is_singleton(name) {
        return true;
    }

    // Global scope constants/enums (MOUSE_BUTTON_LEFT, KEY_ESCAPE, TYPE_INT, etc.)
    if crate::class_db::constant_exists("@GlobalScope", name) {
        return true;
    }

    // Known GDScript global functions not in utility_function registry
    // Includes @GDScript builtins (Color8, is_instance_of, etc.)
    matches!(
        name,
        "print"
            | "push_error"
            | "push_warning"
            | "printerr"
            | "prints"
            | "printraw"
            | "print_rich"
            | "str"
            | "len"
            | "range"
            | "typeof"
            | "assert"
            | "preload"
            | "load"
            | "is_instance_valid"
            | "is_instance_of"
            | "weakref"
            | "Color8"
            | "print_debug"
            | "print_stack"
            | "get_stack"
            | "inst_to_dict"
            | "dict_to_inst"
            | "type_string"
            | "char"
            | "ord"
            | "convert"
    )
}

/// Walk the project extends chain from `ext` (class name or `res://` path) until we find
/// a ClassDB-known type. Returns the ClassDB ancestor or `ext` itself if already in ClassDB.
pub(super) fn resolve_to_classdb_type<'a>(ext: &'a str, project: &'a ProjectIndex) -> String {
    if crate::class_db::class_exists(ext) {
        return ext.to_string();
    }
    let chain = project.extends_chain(ext);
    for ancestor in &chain {
        if crate::class_db::class_exists(ancestor) {
            return (*ancestor).to_string();
        }
    }
    ext.to_string()
}

/// A1 special: `super.nonexistent_parent_method()` — check method exists in parent class.
pub(super) fn check_super_method_not_found(
    root: &Node,
    source: &str,
    file: &GdFile,
    project: &ProjectIndex,
    errors: &mut Vec<StructuralError>,
) {
    check_super_method_in_node(root, source, file, project, errors);
}

fn check_super_method_in_node(
    node: &Node,
    source: &str,
    file: &GdFile,
    project: &ProjectIndex,
    errors: &mut Vec<StructuralError>,
) {
    // Pattern: `super.method()` → attribute { identifier("super"), attribute_call { identifier("method"), arguments } }
    if node.kind() == "attribute"
        && let Some(receiver) = node.named_child(0)
        && receiver.kind() == "identifier"
        && let Ok(recv_name) = receiver.utf8_text(source.as_bytes())
        && recv_name == "super"
    {
        let extends = file.extends_class();
        let mut cursor2 = node.walk();
        for child in node.children(&mut cursor2) {
            if child.kind() == "attribute_call"
                && let Some(method_node) = child.named_child(0)
                && let Ok(method_name) = method_node.utf8_text(source.as_bytes())
            {
                let mut found = extends
                    .is_some_and(|ext| project.method_exists(ext, method_name));
                if !found && let Some(ext) = extends {
                    let classdb_ext = resolve_to_classdb_type(ext, project);
                    found = crate::class_db::method_exists(&classdb_ext, method_name);
                }
                if !found {
                    errors.push(StructuralError {
                        line: method_node.start_position().row as u32 + 1,
                        column: method_node.start_position().column as u32 + 1,
                        message: format!(
                            "function \"{method_name}()\" not found in base {}",
                            file.extends_class().unwrap_or("Node"),
                        ),
                    });
                }
            }
        }
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            check_super_method_in_node(&cursor.node(), source, file, project, errors);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}
