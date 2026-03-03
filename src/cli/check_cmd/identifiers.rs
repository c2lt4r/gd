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
    let mut local_types = std::collections::HashSet::new();
    check_type_not_found_in_node(root, source, file, project, &mut local_types, errors);
}

fn check_type_not_found_in_node(
    node: &Node,
    source: &str,
    file: &GdFile,
    project: &ProjectIndex,
    local_types: &mut std::collections::HashSet<String>,
    errors: &mut Vec<StructuralError>,
) {
    let bytes = source.as_bytes();

    // Track inner class extends — add consts/inner classes from base to local scope
    if node.kind() == "class_definition"
        && let Some(ext_node) = node.child_by_field_name("extends")
    {
        let mut cursor_ext = ext_node.walk();
        for child in ext_node.named_children(&mut cursor_ext) {
            if matches!(child.kind(), "type" | "identifier")
                && let Ok(ext_name) = child.utf8_text(bytes)
            {
                super::classdb::add_names_from_inner_class_extends(ext_name, project, local_types);
                break;
            }
        }
    }

    // `binary_operator` with op "as" or "is"
    if node.kind() == "binary_operator"
        && let Some(op_node) = node.child_by_field_name("op")
        && let Ok(op) = op_node.utf8_text(bytes)
        && matches!(op, "as" | "is")
        && let Some(type_node) = node.child_by_field_name("right")
        && type_node.kind() == "identifier"
        && let Ok(type_name) = type_node.utf8_text(bytes)
        && !is_known_type(type_name, file, project)
        && !local_types.contains(type_name)
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
            check_type_not_found_in_node(&cursor.node(), source, file, project, local_types, errors);
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
                    | "new"
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
                    | "type_exists"
                    | "char"
                    | "ord"
                    | "convert"
            )
            || func_name.starts_with('_'); // Virtual callbacks
        if !is_known {
            // Check ProjectIndex for cross-file base class methods
            let extends = file.extends_str();
            let mut found = extends.is_some_and(|ext| project.method_exists(ext, func_name));
            // Resolve to ClassDB ancestor and check there
            // Default to RefCounted when no extends (Godot's implicit base)
            if !found {
                let classdb_ext = match extends {
                    Some(ext) => resolve_to_classdb_type(ext, project),
                    None => "RefCounted".to_string(),
                };
                found = crate::class_db::method_exists(&classdb_ext, func_name);
            }
            // Also check inner class functions (if inside an inner class body)
            if !found {
                found = file.inner_classes().any(|c| {
                    c.declarations
                        .iter()
                        .any(|d| d.as_func().is_some_and(|f| f.name == func_name))
                });
            }
            if !found {
                errors.push(StructuralError {
                    line: callee.start_position().row as u32 + 1,
                    column: callee.start_position().column as u32 + 1,
                    message: format!(
                        "function \"{func_name}()\" not found in base {}",
                        file.extends_str().unwrap_or("self"),
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
    // Inner classes and their enum members
    for c in file.inner_classes() {
        known.insert(c.name.to_string());
        for decl in &c.declarations {
            if let Some(e) = decl.as_enum() {
                known.insert(e.name.to_string());
                for member in &e.members {
                    known.insert(member.name.to_string());
                }
            }
        }
    }
    // Signals
    for s in file.signals() {
        known.insert(s.name.to_string());
    }
    // Own class_name
    if let Some(cn) = file.class_name {
        known.insert(cn.to_string());
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
                    // Unwrap variadic_parameter (`...name: Type`) to get inner param
                    let effective = if param.kind() == "variadic_parameter" {
                        param.named_child(0).unwrap_or(param)
                    } else {
                        param
                    };
                    // Typed parameter: `name: Type` — first named child is the identifier
                    if let Some(name_node) = effective.named_child(0)
                        && name_node.kind() == "identifier"
                        && let Ok(name) = name_node.utf8_text(source.as_bytes())
                    {
                        func_known.insert(name.to_string());
                    } else if effective.kind() == "identifier"
                        && let Ok(name) = effective.utf8_text(source.as_bytes())
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
    // Track local variable and const declarations
    if matches!(node.kind(), "variable_statement" | "const_statement")
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
    if node.kind() == "function_definition" {
        return;
    }
    // Match pattern sections: `var value` bindings in patterns introduce names
    // scoped to the arm body and pattern guard.
    if node.kind() == "pattern_section" {
        let mut arm_known = known.clone();
        // Collect pattern_binding identifiers from pattern children
        collect_pattern_bindings(node, source, &mut arm_known);
        // Check pattern_guard and body with the extended set
        let mut cursor2 = node.walk();
        for child in node.children(&mut cursor2) {
            if child.kind() == "pattern_guard" || child.kind() == "body" {
                check_undefined_in_body(&child, source, file, project, &mut arm_known, errors);
            }
        }
        return;
    }
    // Lambdas inherit enclosing scope — clone known set and add lambda params
    if node.kind() == "lambda" {
        let mut lambda_known = known.clone();
        if let Some(params) = node.child_by_field_name("parameters") {
            let mut pc = params.walk();
            for param in params.named_children(&mut pc) {
                let effective = if param.kind() == "variadic_parameter" {
                    param.named_child(0).unwrap_or(param)
                } else {
                    param
                };
                if let Some(name_node) = effective.named_child(0)
                    && name_node.kind() == "identifier"
                    && let Ok(name) = name_node.utf8_text(source.as_bytes())
                {
                    lambda_known.insert(name.to_string());
                } else if effective.kind() == "identifier"
                    && let Ok(name) = effective.utf8_text(source.as_bytes())
                {
                    lambda_known.insert(name.to_string());
                }
            }
        }
        if let Some(body) = node.child_by_field_name("body") {
            check_undefined_in_body(&body, source, file, project, &mut lambda_known, errors);
        }
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

/// Recursively collect `pattern_binding` identifiers from match pattern nodes.
/// `pattern_binding` has a single `identifier` child — the bound variable name.
fn collect_pattern_bindings(
    node: &Node,
    source: &str,
    known: &mut std::collections::HashSet<String>,
) {
    if node.kind() == "pattern_binding" {
        if let Some(ident) = node.named_child(0)
            && ident.kind() == "identifier"
            && let Ok(name) = ident.utf8_text(source.as_bytes())
        {
            known.insert(name.to_string());
        }
        return;
    }
    // Don't recurse into body or pattern_guard — those are checked separately
    if matches!(node.kind(), "body" | "pattern_guard") {
        return;
    }
    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            collect_pattern_bindings(&cursor.node(), source, known);
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
    // Attribute subscript: `obj.member[index]` — the member name is inside attribute_subscript
    if parent.kind() == "attribute_subscript" {
        return true;
    }
    // Annotation name (e.g. @warning_ignore, @export, @onready)
    if parent.kind() == "annotation" {
        return true;
    }
    // Match pattern binding: `var value` in match arms
    if parent.kind() == "pattern_binding" {
        return true;
    }
    // Dictionary literal key: { key = value }
    if parent.kind() == "pair"
        && parent
            .named_child(0)
            .is_some_and(|first| first.id() == node.id())
    {
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

/// GDScript keywords and builtins that should never be flagged as undeclared.
fn is_builtin_or_keyword(name: &str) -> bool {
    matches!(
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
            | "if"
            | "elif"
            | "else"
            | "for"
            | "while"
            | "match"
            | "break"
            | "continue"
            | "pass"
            | "return"
            | "var"
            | "const"
            | "func"
            | "class"
            | "signal"
            | "enum"
            | "await"
            | "yield"
            | "not"
            | "and"
            | "or"
            | "in"
            | "is"
            | "as"
    )
}

/// Check if an identifier is in a context where it doesn't need to be declared.
fn is_identifier_context_ok(
    node: &Node,
    name: &str,
    source: &str,
    file: &GdFile,
    project: &ProjectIndex,
) -> bool {
    if is_builtin_or_keyword(name) {
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

    // Cross-file: methods/properties from project-defined base classes.
    // Use extends_str() to handle both class names AND path-based extends.
    if let Some(ext) = file.extends_str() {
        if project.method_exists(ext, name) || project.variable_exists(ext, name) {
            return true;
        }
        // Also check signals and enum members from base classes
        if project_has_signal(ext, name, project) || project_has_enum_member(ext, name, project) {
            return true;
        }
    }

    // ClassDB: extends chain for properties, methods, constants, and signals.
    // Resolve through project extends chain to find the ClassDB ancestor.
    // If no extends statement, default to RefCounted (Godot's implicit base class).
    let classdb_ext = match file.extends_str() {
        Some(ext) => resolve_to_classdb_type(ext, project),
        None => "RefCounted".to_string(),
    };
    if crate::class_db::property_exists(&classdb_ext, name)
        || crate::class_db::method_exists(&classdb_ext, name)
        || crate::class_db::constant_exists(&classdb_ext, name)
        || crate::class_db::signal_exists(&classdb_ext, name)
    {
        return true;
    }

    // Autoloads: check if identifier is a known autoload name
    if project.is_autoload(name) {
        return true;
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
            | "type_exists"
            | "char"
            | "ord"
            | "convert"
    )
}

/// Check if a signal exists on a user class or its extends chain.
fn project_has_signal(class: &str, signal_name: &str, project: &ProjectIndex) -> bool {
    let mut current = class;
    for _ in 0..64 {
        if let Some(fs) = project.lookup_class(current) {
            if fs.signals.iter().any(|s| s == signal_name) {
                return true;
            }
            match fs.extends.as_deref() {
                Some(parent) => current = parent,
                None => return false,
            }
        } else {
            return false;
        }
    }
    false
}

/// Check if an enum member exists on a user class or its extends chain.
fn project_has_enum_member(class: &str, member_name: &str, project: &ProjectIndex) -> bool {
    let mut current = class;
    for _ in 0..64 {
        if let Some(fs) = project.lookup_class(current) {
            if fs.enum_members.iter().any(|m| m == member_name) {
                return true;
            }
            match fs.extends.as_deref() {
                Some(parent) => current = parent,
                None => return false,
            }
        } else {
            return false;
        }
    }
    false
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

/// Find the enclosing `class_definition` node (inner class) and return its extends name.
fn enclosing_inner_class_extends<'a>(node: &Node<'a>, source: &'a str) -> Option<&'a str> {
    let mut current = node.parent()?;
    loop {
        if current.kind() == "class_definition" {
            // The `extends` field is an `extends_statement` node; extract the class
            // name from its `type` or `identifier` child.
            let ext_stmt = current.child_by_field_name("extends")?;
            let mut cursor = ext_stmt.walk();
            for child in ext_stmt.named_children(&mut cursor) {
                if matches!(child.kind(), "type" | "identifier") {
                    return child.utf8_text(source.as_bytes()).ok();
                }
            }
            return None;
        }
        // Stop at the source root (top-level)
        if current.kind() == "source" {
            return None;
        }
        current = current.parent()?;
    }
}

/// Recursively search all inner classes (including nested) for a class with the given
/// name that has a method with the given name.
fn inner_class_has_method(file: &GdFile, class_name: &str, method_name: &str) -> bool {
    fn search_in_class(c: &crate::core::gd_ast::GdClass, name: &str, method: &str) -> bool {
        if c.name == name
            && c.declarations
                .iter()
                .any(|d| d.as_func().is_some_and(|f| f.name == method))
        {
            return true;
        }
        // Recurse into nested inner classes
        for d in &c.declarations {
            if let Some(nested) = d.as_class()
                && search_in_class(nested, name, method)
            {
                return true;
            }
        }
        false
    }
    file.inner_classes()
        .any(|c| search_in_class(c, class_name, method_name))
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
        // Determine the correct extends: if inside an inner class, use its extends;
        // otherwise use the file's top-level extends.
        let inner_ext = enclosing_inner_class_extends(node, source);
        let extends = inner_ext.or_else(|| file.extends_str());
        let mut cursor2 = node.walk();
        for child in node.children(&mut cursor2) {
            if child.kind() == "attribute_call"
                && let Some(method_node) = child.named_child(0)
                && let Ok(method_name) = method_node.utf8_text(source.as_bytes())
            {
                let mut found = extends.is_some_and(|ext| project.method_exists(ext, method_name));
                // Check inner class definitions (recursively) for methods on the parent class
                if !found
                    && let Some(ext) = extends
                {
                    found = inner_class_has_method(file, ext, method_name);
                }
                if !found {
                    let classdb_ext = match extends {
                        Some(ext) => resolve_to_classdb_type(ext, project),
                        None => "RefCounted".to_string(),
                    };
                    found = crate::class_db::method_exists(&classdb_ext, method_name);
                }
                if !found {
                    errors.push(StructuralError {
                        line: method_node.start_position().row as u32 + 1,
                        column: method_node.start_position().column as u32 + 1,
                        message: format!(
                            "function \"{method_name}()\" not found in base {}",
                            extends.unwrap_or("Node"),
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
