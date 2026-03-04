use std::path::Path;

use tree_sitter::Node;

use crate::core::gd_ast::{GdClass, GdFile, GdFunc};
use crate::core::type_inference;
use crate::core::workspace_index::ProjectIndex;

use super::StructuralError;
use super::types::infer_local_var_type;

/// Run structural checks that go beyond tree-sitter error nodes.
pub(super) fn validate_structure(
    root: &Node,
    source: &str,
    file: &GdFile<'_>,
    project_root: Option<&Path>,
    project: &ProjectIndex,
) -> Vec<StructuralError> {
    let mut errors = Vec::new();
    check_top_level_statements(root, &mut errors);
    check_indentation_consistency(root, &mut errors);
    check_class_constants(root, source, &mut errors);
    check_variant_inference(root, source, file, project, &mut errors);
    check_declaration_constraints(root, source, file, &mut errors);
    check_semantic_errors(root, source, file, &mut errors);
    check_preload_and_misc(root, source, project_root, &mut errors);
    check_advanced_semantic(root, source, file, &mut errors);
    errors
}

/// Check 1: Only declarations are valid at the top level of a GDScript file.
/// Bare expressions, loops, if-statements etc. at root level are rejected by Godot.
fn check_top_level_statements(root: &Node, errors: &mut Vec<StructuralError>) {
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        if !child.is_named() || child.kind() == "comment" {
            continue;
        }
        if !is_valid_top_level(child.kind()) {
            // Standalone string expressions are valid at top level in GDScript
            // (used as docstring-style comments, e.g. triple-quoted strings).
            if child.kind() == "expression_statement"
                && child.named_child(0).is_some_and(|c| c.kind() == "string")
            {
                continue;
            }
            // Skip indented statements — likely inside a function body that tree-sitter
            // misparsed due to comments at column 0 breaking indentation tracking.
            if child.start_position().column > 0 {
                continue;
            }
            let pos = child.start_position();
            errors.push(StructuralError {
                line: pos.row as u32 + 1,
                column: pos.column as u32 + 1,
                message: format!(
                    "unexpected `{}` at top level — only declarations are allowed here",
                    friendly_kind(child.kind()),
                ),
            });
        }
    }
}

fn is_valid_top_level(kind: &str) -> bool {
    matches!(
        kind,
        "extends_statement"
            | "class_name_statement"
            | "variable_statement"
            | "const_statement"
            | "function_definition"
            | "constructor_definition"
            | "signal_statement"
            | "enum_definition"
            | "class_definition"
            | "annotation"
            | "decorated_definition"
            | "region_start"
            | "region_end"
    )
}

/// Check 2: Within any `body` node, all non-comment children should be at the
/// same indentation column. A child indented deeper than its siblings indicates
/// an orphaned block (e.g. code left over after removing an `else:`).
/// Godot rejects these but tree-sitter silently accepts them.
fn check_indentation_consistency(node: &Node, errors: &mut Vec<StructuralError>) {
    if node.kind() == "body" {
        check_body_indentation(node, errors);
    }

    // Recurse into children
    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            check_indentation_consistency(&cursor.node(), errors);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

fn check_body_indentation(body: &Node, errors: &mut Vec<StructuralError>) {
    // Find the expected indentation from the first non-comment named child.
    let mut expected_col: Option<usize> = None;
    let mut prev_row: Option<usize> = None;
    let mut cursor = body.walk();

    for child in body.children(&mut cursor) {
        if !child.is_named() || child.kind() == "comment" {
            continue;
        }
        // Skip children that are part of control flow structures — their
        // indentation is managed by the parent statement (if/elif/else/match).
        // Tree-sitter nests these deeper inside the body node, but they are
        // not orphaned blocks.
        if is_control_flow_child(child.kind()) {
            continue;
        }
        let row = child.start_position().row;
        let col = child.start_position().column;
        // Skip indentation check for children on the same line as a previous
        // sibling — these are semicolon-separated one-liners like
        // `func foo(): var x := 59; return x`
        let same_line = prev_row.is_some_and(|r| r == row);
        prev_row = Some(row);
        if same_line {
            continue;
        }
        match expected_col {
            None => expected_col = Some(col),
            Some(exp) if col > exp => {
                let pos = child.start_position();
                errors.push(StructuralError {
                    line: pos.row as u32 + 1,
                    column: pos.column as u32 + 1,
                    message: format!(
                        "unexpected indentation — `{}` is indented deeper than surrounding code (expected column {})",
                        friendly_kind(child.kind()),
                        exp + 1,
                    ),
                });
            }
            _ => {}
        }
    }
}

/// Children of control flow parents that tree-sitter nests inside `body` nodes
/// at deeper indentation levels — these are legitimate and should not be flagged.
fn is_control_flow_child(kind: &str) -> bool {
    matches!(
        kind,
        "elif_clause" | "else_clause" | "match_arm" | "pattern_guard" | "match_body"
    )
}

fn friendly_kind(kind: &str) -> &str {
    match kind {
        "expression_statement" => "expression",
        "variable_statement" => "var statement",
        "const_statement" => "const statement",
        "function_definition" => "function",
        "constructor_definition" => "constructor",
        "for_statement" => "for loop",
        "while_statement" => "while loop",
        "if_statement" => "if statement",
        "match_statement" => "match statement",
        "return_statement" => "return statement",
        "break_statement" => "break statement",
        "continue_statement" => "continue statement",
        "pass_statement" => "pass statement",
        "assignment_statement" | "augmented_assignment_statement" => "assignment",
        other => other,
    }
}

/// Check 3: Validate `ClassName.CONSTANT` references against the Godot class DB.
/// Catches typos like `Environment.TONE_MAP_ACES` (should be `TONE_MAPPER_ACES`).
fn check_class_constants(root: &Node, source: &str, errors: &mut Vec<StructuralError>) {
    check_constants_in_node(*root, source, errors);
}

fn check_constants_in_node(node: Node, source: &str, errors: &mut Vec<StructuralError>) {
    // Look for `attribute` nodes like `Environment.TONE_MAPPER_LINEAR`
    if node.kind() == "attribute"
        && let Some(lhs) = node.named_child(0)
        && let Some(rhs) = node.named_child(1)
        && let Ok(class_name) = lhs.utf8_text(source.as_bytes())
        && let Ok(const_name) = rhs.utf8_text(source.as_bytes())
    {
        // Only check if LHS looks like a Godot class and RHS is UPPER_CASE
        if crate::class_db::class_exists(class_name)
            && is_upper_snake_case(const_name)
            && !crate::class_db::constant_exists(class_name, const_name)
            && !crate::class_db::enum_member_exists(class_name, const_name)
            && !crate::class_db::enum_type_exists(class_name, const_name)
        {
            let suggestions = crate::class_db::suggest_constant(class_name, const_name, 3);
            let hint = if suggestions.is_empty() {
                String::new()
            } else {
                format!(" — did you mean `{}`?", suggestions[0])
            };
            let pos = rhs.start_position();
            errors.push(StructuralError {
                line: pos.row as u32 + 1,
                column: pos.column as u32 + 1,
                message: format!("unknown constant `{class_name}.{const_name}`{hint}",),
            });
        }
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            check_constants_in_node(cursor.node(), source, errors);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

fn is_upper_snake_case(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

/// Check 4: Detect `:=` that resolves to Variant (common source of runtime errors).
fn check_variant_inference(
    root: &Node,
    source: &str,
    file: &GdFile<'_>,
    project: &ProjectIndex,
    errors: &mut Vec<StructuralError>,
) {
    check_variant_node(*root, source, file, project, errors);
}

fn check_variant_node(
    node: Node,
    source: &str,
    file: &GdFile<'_>,
    project: &ProjectIndex,
    errors: &mut Vec<StructuralError>,
) {
    if node.kind() == "variable_statement" {
        // Check for := (tree-sitter stores this as type field with "inferred_type" kind)
        let is_inferred = node
            .child_by_field_name("type")
            .is_some_and(|t| t.kind() == "inferred_type");
        if is_inferred && let Some(value) = node.child_by_field_name("value") {
            let should_flag = if is_variant_producing_expr(&value, source, file, project) {
                true
            } else {
                is_unresolvable_property_access(&value, source)
            };
            if should_flag {
                let var_name = node
                    .child_by_field_name("name")
                    .and_then(|n| n.utf8_text(source.as_bytes()).ok())
                    .unwrap_or("?");
                let pos = node.start_position();
                errors.push(StructuralError {
                    line: pos.row as u32 + 1,
                    column: pos.column as u32 + 1,
                    message: format!(
                        "`:=` infers Variant for `{var_name}` — use an explicit type annotation",
                    ),
                });
            }
        }
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            check_variant_node(cursor.node(), source, file, project, errors);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

/// Check if a value expression is a property access on a variable typed as a Godot
/// Object-derived class — e.g. `event.physical_keycode` where `event: InputEvent`.
/// Property access on base classes resolves to Variant in Godot's type system
/// unless the property is declared on the specific class.
fn is_unresolvable_property_access(value: &Node, source: &str) -> bool {
    // Only check `attribute` nodes (property access), not method calls
    if value.kind() != "attribute" {
        return false;
    }

    // If this attribute has an `attribute_call` child, it's a method call — skip
    let mut cursor = value.walk();
    for child in value.children(&mut cursor) {
        if child.kind() == "attribute_call" {
            return false;
        }
    }

    // Get the object part (first named child)
    let Some(obj) = value.named_child(0) else {
        return false;
    };
    if obj.kind() != "identifier" {
        return false;
    }
    let Ok(obj_name) = obj.utf8_text(source.as_bytes()) else {
        return false;
    };

    // Skip `self.property`
    if obj_name == "self" {
        return false;
    }

    // Find the receiver's declared type — only flag if it's a ClassDB class
    let Some(receiver_type) = find_receiver_type(value, obj_name, source) else {
        return false;
    };
    if !crate::class_db::class_exists(&receiver_type) {
        return false;
    }

    // Get the property name
    let Some(prop_node) = value.named_child(1) else {
        return false;
    };
    let Ok(prop_name) = prop_node.utf8_text(source.as_bytes()) else {
        return false;
    };

    // If the property exists on the receiver's class, it's resolvable — not Variant
    if crate::class_db::property_exists(&receiver_type, prop_name) {
        return false;
    }

    true
}

/// Walk up the AST from `node` to find the enclosing function, then look up
/// the type annotation for a parameter or local variable named `name`.
fn find_receiver_type(node: &Node, name: &str, source: &str) -> Option<String> {
    let bytes = source.as_bytes();

    // Walk up to find the enclosing function
    let mut current = *node;
    let func = loop {
        let parent = current.parent()?;
        if parent.kind() == "function_definition" || parent.kind() == "constructor_definition" {
            break parent;
        }
        current = parent;
    };

    // Check function parameters — typed_parameter / typed_default_parameter
    // These don't have a `name` field; the identifier is the first named child.
    if let Some(params) = func.child_by_field_name("parameters") {
        let mut cursor = params.walk();
        for param in params.named_children(&mut cursor) {
            let param_name = match param.kind() {
                "typed_parameter" | "typed_default_parameter" => {
                    first_identifier_text(&param, bytes)
                }
                _ => None,
            };
            if let Some(pname) = param_name
                && pname == name
                && let Some(type_node) = param.child_by_field_name("type")
                && type_node.kind() != "inferred_type"
                && let Ok(type_text) = type_node.utf8_text(bytes)
            {
                // Prefer narrowed type from `is` guard over declared type
                if let Some(narrowed) = type_inference::find_narrowed_type(node, name, source) {
                    return Some(narrowed);
                }
                return Some(type_text.to_string());
            }
        }
    }

    // Check local variable declarations in the function body before this node
    if let Some(body) = func.child_by_field_name("body") {
        let target_row = node.start_position().row;
        let mut cursor = body.walk();
        for child in body.children(&mut cursor) {
            if child.start_position().row >= target_row {
                break;
            }
            if child.kind() == "variable_statement"
                && let Some(var_name) = child.child_by_field_name("name")
                && let Ok(vname) = var_name.utf8_text(bytes)
                && vname == name
            {
                // Explicit type annotation (not inferred)
                if let Some(type_node) = child.child_by_field_name("type")
                    && type_node.kind() != "inferred_type"
                    && let Ok(type_text) = type_node.utf8_text(bytes)
                {
                    if let Some(narrowed) = type_inference::find_narrowed_type(node, name, source) {
                        return Some(narrowed);
                    }
                    return Some(type_text.to_string());
                }

                // Inferred type (:=) — try to resolve from initializer
                if let Some(value) = child.child_by_field_name("value")
                    && let Some(typ) = infer_type_from_initializer(&value, bytes, &func)
                {
                    return Some(typ);
                }
            }
        }
    }

    // Fallback: check for type narrowing on params/locals without explicit type
    if let Some(narrowed) = type_inference::find_narrowed_type(node, name, source) {
        return Some(narrowed);
    }

    None
}

/// Lightweight type inference from a variable initializer expression.
/// Handles constructors (`Node3D.new()`), cast (`as Type`), and same-file function return types.
fn infer_type_from_initializer(
    value: &Node,
    source: &[u8],
    enclosing_func: &Node,
) -> Option<String> {
    match value.kind() {
        // Cast: `expr as Type`
        "as_pattern" | "cast" => {
            let type_node = value.child_by_field_name("type").or_else(|| {
                let count = value.named_child_count();
                if count >= 2 {
                    value.named_child(count - 1)
                } else {
                    None
                }
            })?;
            Some(type_node.utf8_text(source).ok()?.to_string())
        }
        // Method call: `Type.new()` — attribute with attribute_call
        "attribute" => {
            let mut has_call = false;
            let mut method = None;
            let mut cursor = value.walk();
            for child in value.children(&mut cursor) {
                if child.kind() == "attribute_call" {
                    has_call = true;
                    if let Some(name_node) = child.named_child(0) {
                        method = name_node.utf8_text(source).ok();
                    }
                }
            }
            if has_call && method == Some("new") {
                let receiver = value.named_child(0)?;
                let type_name = receiver.utf8_text(source).ok()?;
                if type_name.chars().next()?.is_ascii_uppercase() {
                    return Some(type_name.to_string());
                }
            }
            None
        }
        // Function call: constructor or same-file function
        "call" => {
            let func_node = value
                .child_by_field_name("function")
                .or_else(|| value.named_child(0))?;
            let func_name = func_node.utf8_text(source).ok()?;

            // Constructor call (PascalCase)
            if func_name.chars().next()?.is_ascii_uppercase() {
                return Some(func_name.to_string());
            }

            // Same-file function — walk siblings of the enclosing function to find it
            let parent = enclosing_func.parent()?;
            let mut cursor = parent.walk();
            for sibling in parent.children(&mut cursor) {
                if sibling.kind() == "function_definition"
                    && let Some(sib_name) = sibling.child_by_field_name("name")
                    && sib_name.utf8_text(source).ok() == Some(func_name)
                    && let Some(ret_type) = sibling.child_by_field_name("return_type")
                    && let Ok(ret_text) = ret_type.utf8_text(source)
                    && ret_text != "void"
                {
                    return Some(ret_text.to_string());
                }
            }
            None
        }
        _ => None,
    }
}

/// Extract the first `identifier` child's text from a node.
fn first_identifier_text<'a>(node: &Node, source: &'a [u8]) -> Option<&'a str> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "identifier" {
            return child.utf8_text(source).ok();
        }
    }
    None
}

/// Check if an expression is known to produce Variant (losing type information).
fn is_variant_producing_expr(
    node: &Node,
    source: &str,
    file: &GdFile<'_>,
    project: &ProjectIndex,
) -> bool {
    match node.kind() {
        // dict["key"], arr[idx] — only flag when we can confirm the receiver is
        // Dictionary or untyped Array. Packed arrays, typed arrays, and strings
        // have known element types. If we can't determine the receiver type at all,
        // don't flag (user likely knows the type from context).
        "subscript" => {
            if let Some(receiver) = node.named_child(0) {
                let ty = type_inference::infer_expression_type_with_project(
                    &receiver, source, file, project,
                )
                .or_else(|| infer_local_var_type(&receiver, source, file, project));
                matches!(
                    &ty,
                    Some(type_inference::InferredType::Builtin(
                        "Dictionary" | "Array"
                    ))
                )
            } else {
                true
            }
        }
        // method calls: attribute > attribute_call (tree-sitter pattern)
        // e.g. dict.get("key"), dict.values(), dict.keys()
        "attribute" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "attribute_call"
                    && let Some(name_node) = child.named_child(0)
                    && let Ok(method_name) = name_node.utf8_text(source.as_bytes())
                {
                    // Dict methods that always return Variant
                    if matches!(method_name, "get" | "get_or_add" | "values" | "keys") {
                        return true;
                    }

                    // load(...).instantiate() — load() returns Resource which has
                    // no instantiate(); Godot rejects this. preload() is fine.
                    if let Some(obj) = node.named_child(0)
                        && obj.kind() == "call"
                        && let Some(func) = obj
                            .child_by_field_name("function")
                            .or_else(|| obj.named_child(0))
                        && let Ok(func_name) = func.utf8_text(source.as_bytes())
                        && func_name == "load"
                    {
                        return true;
                    }

                    // ClassDB method returning Variant on a typed receiver
                    if let Some(obj) = node.named_child(0)
                        && obj.kind() == "identifier"
                        && let Ok(obj_name) = obj.utf8_text(source.as_bytes())
                        && obj_name != "self"
                        && let Some(receiver_type) = find_receiver_type(node, obj_name, source)
                        && crate::class_db::method_return_type(&receiver_type, method_name)
                            == Some("Variant")
                    {
                        return true;
                    }

                    return false;
                }
            }
            false
        }
        // Binary/comparison operators with a Variant operand produce Variant
        // e.g., dict["key"] == "switch", dict["key"] + 1
        "binary_operator" | "comparison_operator" => {
            // `as` cast produces a typed result, not Variant
            if node
                .child_by_field_name("op")
                .and_then(|op| op.utf8_text(source.as_bytes()).ok())
                .is_some_and(|op| op == "as")
            {
                return false;
            }
            // `in` / `not in` return Variant in Godot's static type system
            if is_in_operator(node, source) {
                return true;
            }
            node.named_child(0)
                .is_some_and(|c| is_variant_producing_expr(&c, source, file, project))
                || node
                    .named_child(1)
                    .is_some_and(|c| is_variant_producing_expr(&c, source, file, project))
        }
        // Parenthesized: unwrap and check inner expression
        "parenthesized_expression" => node
            .named_child(0)
            .is_some_and(|c| is_variant_producing_expr(&c, source, file, project)),
        // Unary operators: `not dict["key"]`
        "unary_operator" => node
            .child_by_field_name("operand")
            .is_some_and(|c| is_variant_producing_expr(&c, source, file, project)),
        // Builtin function calls that return Variant (polymorphic builtins)
        "call" => {
            let func_node = node
                .child_by_field_name("function")
                .or_else(|| node.named_child(0));
            if let Some(func) = func_node
                && let Ok(name) = func.utf8_text(source.as_bytes())
            {
                matches!(name, "max" | "min" | "clamp" | "snapped" | "wrap")
            } else {
                false
            }
        }
        _ => false,
    }
}

/// Check if a binary/comparison operator uses `in` or `not in`.
/// These return Variant in Godot's static type system.
fn is_in_operator(node: &Node, source: &str) -> bool {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if !child.is_named()
            && let Ok(text) = child.utf8_text(source.as_bytes())
            && (text == "in" || text == "not")
        {
            return true;
        }
    }
    false
}

// ---------------------------------------------------------------------------
// Batch 2: Declaration constraint checks
// ---------------------------------------------------------------------------

fn check_declaration_constraints(
    root: &Node,
    source: &str,
    file: &GdFile<'_>,
    errors: &mut Vec<StructuralError>,
) {
    check_init_return_type(file, errors);
    check_mandatory_after_optional(file, errors);
    check_signal_default_values(root, source, errors);
    check_duplicate_class_name_extends(root, source, errors);
    check_duplicate_param_names(file, errors);
    check_yield_keyword(root, source, errors);
    check_static_init_params(file, errors);
    check_duplicate_tool(root, source, errors);
}

/// G4: Constructor `_init` cannot have a non-void return type.
fn check_init_return_type(file: &GdFile<'_>, errors: &mut Vec<StructuralError>) {
    for func in file.funcs() {
        if func.name == "_init" && has_non_void_return(func) {
            errors.push(StructuralError {
                line: func.node.start_position().row as u32 + 1,
                column: 1,
                message: "constructor `_init()` cannot have a return type".to_string(),
            });
        }
    }
    for inner in file.inner_classes() {
        check_init_return_type_inner(inner, errors);
    }
}

fn has_non_void_return(func: &GdFunc<'_>) -> bool {
    func.return_type
        .as_ref()
        .is_some_and(|rt| rt.name != "void")
}

fn check_init_return_type_inner(class: &GdClass<'_>, errors: &mut Vec<StructuralError>) {
    for func in class.declarations.iter().filter_map(|d| d.as_func()) {
        if func.name == "_init" && has_non_void_return(func) {
            errors.push(StructuralError {
                line: func.node.start_position().row as u32 + 1,
                column: 1,
                message: "constructor `_init()` cannot have a return type".to_string(),
            });
        }
    }
    for inner in class.declarations.iter().filter_map(|d| d.as_class()) {
        check_init_return_type_inner(inner, errors);
    }
}

/// G3: Mandatory parameter after optional parameter.
fn check_mandatory_after_optional(file: &GdFile<'_>, errors: &mut Vec<StructuralError>) {
    for func in file.funcs() {
        let mut seen_optional = false;
        for param in &func.params {
            if param.default.is_some() {
                seen_optional = true;
            } else if seen_optional {
                errors.push(StructuralError {
                    line: func.node.start_position().row as u32 + 1,
                    column: 1,
                    message: format!(
                        "required parameter `{}` follows optional parameter in `{}()`",
                        param.name, func.name,
                    ),
                });
                break;
            }
        }
    }
    for inner in file.inner_classes() {
        check_mandatory_after_optional_inner(inner, errors);
    }
}

fn check_mandatory_after_optional_inner(class: &GdClass<'_>, errors: &mut Vec<StructuralError>) {
    for func in class.declarations.iter().filter_map(|d| d.as_func()) {
        let mut seen_optional = false;
        for param in &func.params {
            if param.default.is_some() {
                seen_optional = true;
            } else if seen_optional {
                errors.push(StructuralError {
                    line: func.node.start_position().row as u32 + 1,
                    column: 1,
                    message: format!(
                        "required parameter `{}` follows optional parameter in `{}()`",
                        param.name, func.name,
                    ),
                });
                break;
            }
        }
    }
    for inner in class.declarations.iter().filter_map(|d| d.as_class()) {
        check_mandatory_after_optional_inner(inner, errors);
    }
}

/// G2: Signal parameters cannot have default values.
fn check_signal_default_values(root: &Node, source: &str, errors: &mut Vec<StructuralError>) {
    check_signal_defaults_in_node(*root, source, errors);
}

fn check_signal_defaults_in_node(node: Node, source: &str, errors: &mut Vec<StructuralError>) {
    if node.kind() == "signal_statement"
        && let Some(params) = node.child_by_field_name("parameters")
    {
        let mut cursor = params.walk();
        for param in params.named_children(&mut cursor) {
            if param.kind() == "default_parameter" || param.kind() == "typed_default_parameter" {
                let param_name = first_identifier_text(&param, source.as_bytes()).unwrap_or("?");
                let pos = param.start_position();
                errors.push(StructuralError {
                    line: pos.row as u32 + 1,
                    column: pos.column as u32 + 1,
                    message: format!(
                        "signal parameter `{param_name}` cannot have a default value",
                    ),
                });
            }
        }
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            check_signal_defaults_in_node(cursor.node(), source, errors);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

/// G6: Duplicate `class_name` or duplicate `extends` statements.
fn check_duplicate_class_name_extends(
    root: &Node,
    source: &str,
    errors: &mut Vec<StructuralError>,
) {
    let mut cursor = root.walk();
    let mut class_name_count = 0u32;
    let mut extends_count = 0u32;

    for child in root.children(&mut cursor) {
        match child.kind() {
            "class_name_statement" => {
                class_name_count += 1;
                if class_name_count > 1 {
                    let pos = child.start_position();
                    errors.push(StructuralError {
                        line: pos.row as u32 + 1,
                        column: pos.column as u32 + 1,
                        message: "duplicate `class_name` declaration".to_string(),
                    });
                }
            }
            "extends_statement" => {
                extends_count += 1;
                if extends_count > 1 {
                    let pos = child.start_position();
                    errors.push(StructuralError {
                        line: pos.row as u32 + 1,
                        column: pos.column as u32 + 1,
                        message: "duplicate `extends` declaration".to_string(),
                    });
                }
            }
            _ => {}
        }
    }
    // Also check inner classes — but only the decorated_definition children
    let _ = source; // used in other checks
}

/// G7: Duplicate parameter names in the same function.
fn check_duplicate_param_names(file: &GdFile<'_>, errors: &mut Vec<StructuralError>) {
    for func in file.funcs() {
        let mut seen = std::collections::HashSet::new();
        for param in &func.params {
            if !seen.insert(param.name) {
                errors.push(StructuralError {
                    line: func.node.start_position().row as u32 + 1,
                    column: 1,
                    message: format!(
                        "duplicate parameter name `{}` in `{}()`",
                        param.name, func.name,
                    ),
                });
            }
        }
    }
    for inner in file.inner_classes() {
        check_duplicate_param_names_inner(inner, errors);
    }
}

fn check_duplicate_param_names_inner(class: &GdClass<'_>, errors: &mut Vec<StructuralError>) {
    for func in class.declarations.iter().filter_map(|d| d.as_func()) {
        let mut seen = std::collections::HashSet::new();
        for param in &func.params {
            if !seen.insert(param.name) {
                errors.push(StructuralError {
                    line: func.node.start_position().row as u32 + 1,
                    column: 1,
                    message: format!(
                        "duplicate parameter name `{}` in `{}()`",
                        param.name, func.name,
                    ),
                });
            }
        }
    }
    for inner in class.declarations.iter().filter_map(|d| d.as_class()) {
        check_duplicate_param_names_inner(inner, errors);
    }
}

/// G1: `yield` keyword was removed in Godot 4 (replaced by `await`).
fn check_yield_keyword(root: &Node, source: &str, errors: &mut Vec<StructuralError>) {
    check_yield_in_node(*root, source, errors);
}

fn check_yield_in_node(node: Node, source: &str, errors: &mut Vec<StructuralError>) {
    // yield() appears as a call node with function name "yield"
    if node.kind() == "call"
        && let Some(func) = node
            .child_by_field_name("function")
            .or_else(|| node.named_child(0))
        && func.utf8_text(source.as_bytes()).ok() == Some("yield")
    {
        let pos = node.start_position();
        errors.push(StructuralError {
            line: pos.row as u32 + 1,
            column: pos.column as u32 + 1,
            message: "`yield` was removed in Godot 4 — use `await` instead".to_string(),
        });
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            check_yield_in_node(cursor.node(), source, errors);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

/// H7: `_static_init` cannot have parameters.
fn check_static_init_params(file: &GdFile<'_>, errors: &mut Vec<StructuralError>) {
    for func in file.funcs() {
        if func.name == "_static_init" && !func.params.is_empty() {
            errors.push(StructuralError {
                line: func.node.start_position().row as u32 + 1,
                column: 1,
                message: "`_static_init()` cannot have parameters".to_string(),
            });
        }
    }
    for inner in file.inner_classes() {
        check_static_init_params_inner(inner, errors);
    }
}

fn check_static_init_params_inner(class: &GdClass<'_>, errors: &mut Vec<StructuralError>) {
    for func in class.declarations.iter().filter_map(|d| d.as_func()) {
        if func.name == "_static_init" && !func.params.is_empty() {
            errors.push(StructuralError {
                line: func.node.start_position().row as u32 + 1,
                column: 1,
                message: "`_static_init()` cannot have parameters".to_string(),
            });
        }
    }
    for inner in class.declarations.iter().filter_map(|d| d.as_class()) {
        check_static_init_params_inner(inner, errors);
    }
}

/// E8: Duplicate `@tool` annotation.
fn check_duplicate_tool(root: &Node, source: &str, errors: &mut Vec<StructuralError>) {
    let mut cursor = root.walk();
    let mut tool_count = 0u32;

    for child in root.children(&mut cursor) {
        if child.kind() == "annotation"
            && let Some(id) = find_annotation_name(&child, source)
            && id == "tool"
        {
            tool_count += 1;
            if tool_count > 1 {
                let pos = child.start_position();
                errors.push(StructuralError {
                    line: pos.row as u32 + 1,
                    column: pos.column as u32 + 1,
                    message: "duplicate `@tool` annotation".to_string(),
                });
            }
        }
    }
}

/// Extract the annotation name (e.g. "tool", "export", "onready") from an annotation node.
pub(super) fn find_annotation_name<'a>(node: &Node, source: &'a str) -> Option<&'a str> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "identifier" {
            return child.utf8_text(source.as_bytes()).ok();
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Batch 3: Semantic checks
// ---------------------------------------------------------------------------

fn check_semantic_errors(
    root: &Node,
    source: &str,
    file: &GdFile<'_>,
    errors: &mut Vec<StructuralError>,
) {
    check_static_context_violations(root, source, file, errors);
    check_assign_to_constant(root, source, file, errors);
    check_void_return_value(root, source, file, errors);
    check_get_node_in_static(root, source, errors);
    check_export_constraints(file, errors);
    check_object_constructor(root, source, errors);
}

/// C1: Static context violations — using instance vars, `self`, or instance methods
/// from a static function.
fn check_static_context_violations(
    root: &Node,
    source: &str,
    file: &GdFile<'_>,
    errors: &mut Vec<StructuralError>,
) {
    let bytes = source.as_bytes();
    // Collect instance (non-static) member names for reference
    let instance_vars: std::collections::HashSet<&str> = file
        .vars()
        .filter(|v| !v.is_static && !v.is_const)
        .map(|v| v.name)
        .collect();
    let instance_funcs: std::collections::HashSet<&str> = file
        .funcs()
        .filter(|f| !f.is_static)
        .map(|f| f.name)
        .collect();

    // Walk functions that are static
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        // Check static variable initializers
        check_static_var_initializer(&child, bytes, file, &instance_vars, &instance_funcs, errors);

        let func_node = match child.kind() {
            "function_definition" => child,
            "decorated_definition" => {
                let mut inner_cursor = child.walk();
                let mut found = None;
                for inner in child.children(&mut inner_cursor) {
                    if inner.kind() == "function_definition" {
                        found = Some(inner);
                        break;
                    }
                }
                if let Some(f) = found {
                    f
                } else {
                    continue;
                }
            }
            _ => continue,
        };

        // Check if this function is static
        let is_static = file.funcs().any(|f| {
            f.is_static
                && func_node
                    .child_by_field_name("name")
                    .and_then(|n| n.utf8_text(bytes).ok())
                    == Some(f.name)
        });
        if !is_static {
            continue;
        }

        let func_name = func_node
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(bytes).ok())
            .unwrap_or("?");

        // Walk body looking for `self`, instance var refs, instance method calls
        if let Some(body) = func_node.child_by_field_name("body") {
            let mut local_vars = std::collections::HashSet::new();
            collect_local_vars(&body, bytes, &mut local_vars);
            // Also add function parameters
            for param in func_node.children_by_field_name("parameters", &mut func_node.walk()) {
                if let Ok(n) = param.utf8_text(bytes) {
                    local_vars.insert(n);
                }
            }
            check_static_body(
                &body,
                bytes,
                func_name,
                &instance_vars,
                &instance_funcs,
                &local_vars,
                errors,
            );
        }
    }
}

fn check_static_var_initializer(
    child: &Node,
    bytes: &[u8],
    file: &GdFile<'_>,
    instance_vars: &std::collections::HashSet<&str>,
    instance_funcs: &std::collections::HashSet<&str>,
    errors: &mut Vec<StructuralError>,
) {
    let var_node = match child.kind() {
        "variable_statement" => Some(*child),
        "decorated_definition" => {
            let mut ic = child.walk();
            child
                .children(&mut ic)
                .find(|c| c.kind() == "variable_statement")
        }
        _ => return,
    };
    if let Some(var_node) = var_node
        && file.vars().any(|v| {
            v.is_static
                && !v.is_const
                && var_node
                    .child_by_field_name("name")
                    .and_then(|n| n.utf8_text(bytes).ok())
                    == Some(v.name)
        })
        && let Some(value) = var_node.child_by_field_name("value")
    {
        let var_name = var_node
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(bytes).ok())
            .unwrap_or("?");
        let label = format!("static var `{var_name}` initializer");
        let empty = std::collections::HashSet::new();
        check_static_body(
            &value,
            bytes,
            &label,
            instance_vars,
            instance_funcs,
            &empty,
            errors,
        );
    }
}

fn collect_local_vars<'a>(
    node: &Node<'a>,
    source: &'a [u8],
    locals: &mut std::collections::HashSet<&'a str>,
) {
    match node.kind() {
        "variable_statement" | "const_statement" => {
            if let Some(name) = node.child_by_field_name("name")
                && let Ok(n) = name.utf8_text(source)
            {
                locals.insert(n);
            }
        }
        "for_statement" => {
            if let Some(name) = node.child_by_field_name("left")
                && let Ok(n) = name.utf8_text(source)
            {
                locals.insert(n);
            }
        }
        _ => {}
    }
    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            collect_local_vars(&cursor.node(), source, locals);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

fn check_static_body(
    node: &Node,
    source: &[u8],
    func_name: &str,
    instance_vars: &std::collections::HashSet<&str>,
    instance_funcs: &std::collections::HashSet<&str>,
    local_vars: &std::collections::HashSet<&str>,
    errors: &mut Vec<StructuralError>,
) {
    // Check for direct identifier references to instance members, self, or instance methods
    // Only check bare identifiers (not the RHS of attribute access)
    if node.kind() == "identifier"
        && let Ok(name) = node.utf8_text(source)
        && let Some(parent) = node.parent()
    {
        if parent.kind() == "attribute" && parent.named_child(1) == Some(*node) {
            // This is obj.name — don't flag
        } else if name == "self" || name == "super" {
            let pos = node.start_position();
            errors.push(StructuralError {
                line: pos.row as u32 + 1,
                column: pos.column as u32 + 1,
                message: format!("cannot use `{name}` in static function `{func_name}()`",),
            });
            return;
        } else if instance_vars.contains(name) && !local_vars.contains(name) {
            let pos = node.start_position();
            errors.push(StructuralError {
                line: pos.row as u32 + 1,
                column: pos.column as u32 + 1,
                message: format!(
                    "cannot access instance variable `{name}` from static function `{func_name}()`",
                ),
            });
            return;
        }
    }

    // Check for bare function calls to instance methods
    if node.kind() == "call"
        && let Some(func_node) = node
            .child_by_field_name("function")
            .or_else(|| node.named_child(0))
        && func_node.kind() == "identifier"
        && let Ok(callee) = func_node.utf8_text(source)
        && instance_funcs.contains(callee)
    {
        let pos = node.start_position();
        errors.push(StructuralError {
            line: pos.row as u32 + 1,
            column: pos.column as u32 + 1,
            message: format!(
                "cannot call instance method `{callee}()` from static function `{func_name}()`",
            ),
        });
        return;
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            check_static_body(
                &cursor.node(),
                source,
                func_name,
                instance_vars,
                instance_funcs,
                local_vars,
                errors,
            );
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

/// C2: Assignment to a constant or enum value.
fn check_assign_to_constant(
    root: &Node,
    source: &str,
    file: &GdFile<'_>,
    errors: &mut Vec<StructuralError>,
) {
    let bytes = source.as_bytes();
    let constants: std::collections::HashSet<&str> =
        file.vars().filter(|v| v.is_const).map(|v| v.name).collect();
    let enum_members: std::collections::HashSet<&str> = file
        .enums()
        .flat_map(|e| e.members.iter().map(|m| m.name))
        .collect();
    let signals: std::collections::HashSet<&str> = file.signals().map(|s| s.name).collect();

    check_assign_to_const_in_node(*root, bytes, &constants, &enum_members, &signals, errors);
}

fn check_assign_to_const_in_node(
    node: Node,
    source: &[u8],
    constants: &std::collections::HashSet<&str>,
    enum_members: &std::collections::HashSet<&str>,
    signals: &std::collections::HashSet<&str>,
    errors: &mut Vec<StructuralError>,
) {
    // Assignments: assignment_statement/augmented_assignment_statement at top level,
    // or expression_statement > assignment/augmented_assignment inside function bodies
    let assign_node = match node.kind() {
        "assignment_statement"
        | "augmented_assignment_statement"
        | "assignment"
        | "augmented_assignment" => Some(node),
        "expression_statement" => {
            let mut c = node.walk();
            node.children(&mut c).find(|child| {
                child.kind() == "assignment" || child.kind() == "augmented_assignment"
            })
        }
        _ => None,
    };

    if let Some(assign) = assign_node
        && let Some(lhs) = assign.named_child(0)
    {
        if lhs.kind() == "identifier"
            && let Ok(name) = lhs.utf8_text(source)
        {
            if constants.contains(name) {
                let pos = lhs.start_position();
                errors.push(StructuralError {
                    line: pos.row as u32 + 1,
                    column: pos.column as u32 + 1,
                    message: format!("cannot assign to constant `{name}`"),
                });
            } else if enum_members.contains(name) {
                let pos = lhs.start_position();
                errors.push(StructuralError {
                    line: pos.row as u32 + 1,
                    column: pos.column as u32 + 1,
                    message: format!("cannot assign to enum value `{name}`"),
                });
            } else if signals.contains(name) {
                let pos = lhs.start_position();
                errors.push(StructuralError {
                    line: pos.row as u32 + 1,
                    column: pos.column as u32 + 1,
                    message: format!("cannot assign to signal `{name}`"),
                });
            }
        } else if lhs.kind() == "subscript" {
            // const_arr[idx] = val — extract the base identifier
            if let Some(base) = lhs.named_child(0)
                && base.kind() == "identifier"
                && let Ok(base_name) = base.utf8_text(source)
                && constants.contains(base_name)
            {
                let pos = lhs.start_position();
                errors.push(StructuralError {
                    line: pos.row as u32 + 1,
                    column: pos.column as u32 + 1,
                    message: format!("cannot assign to element of constant `{base_name}`"),
                });
            }
        } else if lhs.kind() == "attribute"
            && let Some(rhs) = lhs.named_child(1)
            && rhs.kind() == "identifier"
            && let Ok(member) = rhs.utf8_text(source)
            && enum_members.contains(member)
        {
            let pos = lhs.start_position();
            errors.push(StructuralError {
                line: pos.row as u32 + 1,
                column: pos.column as u32 + 1,
                message: format!("cannot assign to enum value `{member}`"),
            });
        }
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            check_assign_to_const_in_node(
                cursor.node(),
                source,
                constants,
                enum_members,
                signals,
                errors,
            );
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

/// C3: Void function returning a value / returning void from typed function.
fn check_void_return_value(
    root: &Node,
    source: &str,
    file: &GdFile<'_>,
    errors: &mut Vec<StructuralError>,
) {
    let bytes = source.as_bytes();
    for func in file.funcs() {
        if let Some(ref ret) = func.return_type
            && ret.name == "void"
        {
            // Find the AST node for this function and check for `return <value>`
            check_void_func_returns(*root, bytes, func.name, errors);
        }
    }
    for inner in file.inner_classes() {
        check_void_return_value_inner(root, source, inner, errors);
    }
}

fn check_void_return_value_inner(
    root: &Node,
    source: &str,
    class: &GdClass<'_>,
    errors: &mut Vec<StructuralError>,
) {
    let bytes = source.as_bytes();
    for func in class.declarations.iter().filter_map(|d| d.as_func()) {
        if let Some(ref ret) = func.return_type
            && ret.name == "void"
        {
            check_void_func_returns(*root, bytes, func.name, errors);
        }
    }
    for inner in class.declarations.iter().filter_map(|d| d.as_class()) {
        check_void_return_value_inner(root, source, inner, errors);
    }
}

fn check_void_func_returns(
    root: Node,
    source: &[u8],
    func_name: &str,
    errors: &mut Vec<StructuralError>,
) {
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        let func_node = match child.kind() {
            "function_definition" | "constructor_definition" => child,
            "decorated_definition" => {
                let mut inner_cursor = child.walk();
                let mut found = None;
                for inner in child.children(&mut inner_cursor) {
                    if inner.kind() == "function_definition" {
                        found = Some(inner);
                        break;
                    }
                }
                if let Some(f) = found {
                    f
                } else {
                    continue;
                }
            }
            _ => continue,
        };

        let name = func_node
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(source).ok())
            .unwrap_or("");
        if name != func_name {
            continue;
        }

        if let Some(body) = func_node.child_by_field_name("body") {
            check_returns_in_body(&body, func_name, errors);
        }
    }
}

fn check_returns_in_body(node: &Node, func_name: &str, errors: &mut Vec<StructuralError>) {
    if node.kind() == "return_statement" {
        // Check if there's a value after `return`
        if let Some(child) = node.named_child(0) {
            // Allow `return <call>()` in void functions — Godot permits this pattern
            // for side-effect calls (e.g. `return print("hello")`, `return emit()`)
            let is_call_return = child.kind() == "call" || child.kind() == "attribute";

            if !is_call_return {
                let pos = node.start_position();
                errors.push(StructuralError {
                    line: pos.row as u32 + 1,
                    column: pos.column as u32 + 1,
                    message: format!("void function `{func_name}()` cannot return a value",),
                });
            }
        }
        return;
    }

    // Don't recurse into nested function definitions (lambdas / inner functions)
    if node.kind() == "function_definition"
        || node.kind() == "constructor_definition"
        || node.kind() == "lambda"
    {
        return;
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            check_returns_in_body(&cursor.node(), func_name, errors);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

/// H14: `$` / `%` get_node syntax in static function.
fn check_get_node_in_static(root: &Node, source: &str, errors: &mut Vec<StructuralError>) {
    let bytes = source.as_bytes();
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        let func_node = match child.kind() {
            "function_definition" => child,
            "decorated_definition" => {
                let mut inner_cursor = child.walk();
                let mut found = None;
                for inner in child.children(&mut inner_cursor) {
                    if inner.kind() == "function_definition" {
                        found = Some(inner);
                        break;
                    }
                }
                if let Some(f) = found {
                    f
                } else {
                    continue;
                }
            }
            _ => continue,
        };

        // Check if static (has static_keyword child)
        let mut is_static = false;
        {
            let mut c = func_node.walk();
            for fc in func_node.children(&mut c) {
                if fc.kind() == "static_keyword" {
                    is_static = true;
                    break;
                }
            }
        }
        if !is_static {
            continue;
        }

        let func_name = func_node
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(bytes).ok())
            .unwrap_or("?");

        if let Some(body) = func_node.child_by_field_name("body") {
            check_get_node_in_body(&body, func_name, errors);
        }
    }
}

fn check_get_node_in_body(node: &Node, func_name: &str, errors: &mut Vec<StructuralError>) {
    if node.kind() == "get_node" {
        let pos = node.start_position();
        errors.push(StructuralError {
            line: pos.row as u32 + 1,
            column: pos.column as u32 + 1,
            message: format!("cannot use `$`/`%` get_node in static function `{func_name}()`",),
        });
        return;
    }

    // Don't recurse into nested function definitions
    if node.kind() == "function_definition"
        || node.kind() == "constructor_definition"
        || node.kind() == "lambda"
    {
        return;
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            check_get_node_in_body(&cursor.node(), func_name, errors);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

/// E1: `@export` without type or initializer.
/// E3: `@export` on a static variable.
/// E4: Duplicate `@export` annotation on same variable.
fn check_export_constraints(file: &GdFile<'_>, errors: &mut Vec<StructuralError>) {
    for var in file.vars() {
        let export_count = var
            .annotations
            .iter()
            .filter(|a| a.name == "export")
            .count();
        let has_export = export_count > 0;

        if has_export {
            // E4: Duplicate @export
            if export_count > 1 {
                errors.push(StructuralError {
                    line: var.node.start_position().row as u32 + 1,
                    column: 1,
                    message: format!("duplicate `@export` annotation on `{}`", var.name,),
                });
            }

            // E3: @export on static
            if var.is_static {
                errors.push(StructuralError {
                    line: var.node.start_position().row as u32 + 1,
                    column: 1,
                    message: format!("`@export` cannot be used on static variable `{}`", var.name,),
                });
            }

            // E1: @export without type or initializer
            // Only check plain @export, not @export_* variants
            let has_type = var.type_ann.as_ref().is_some_and(|t| !t.name.is_empty());
            if !has_type && var.value.is_none() {
                errors.push(StructuralError {
                    line: var.node.start_position().row as u32 + 1,
                    column: 1,
                    message: format!(
                        "`@export` variable `{}` has no type annotation or initializer",
                        var.name,
                    ),
                });
            }
        }
    }
    for inner in file.inner_classes() {
        check_export_constraints_inner(inner, errors);
    }
}

fn check_export_constraints_inner(class: &GdClass<'_>, errors: &mut Vec<StructuralError>) {
    for var in class.declarations.iter().filter_map(|d| d.as_var()) {
        let export_count = var
            .annotations
            .iter()
            .filter(|a| a.name == "export")
            .count();
        let has_export = export_count > 0;

        if has_export {
            if export_count > 1 {
                errors.push(StructuralError {
                    line: var.node.start_position().row as u32 + 1,
                    column: 1,
                    message: format!("duplicate `@export` annotation on `{}`", var.name,),
                });
            }
            if var.is_static {
                errors.push(StructuralError {
                    line: var.node.start_position().row as u32 + 1,
                    column: 1,
                    message: format!("`@export` cannot be used on static variable `{}`", var.name,),
                });
            }
            let has_type = var.type_ann.as_ref().is_some_and(|t| !t.name.is_empty());
            if !has_type && var.value.is_none() {
                errors.push(StructuralError {
                    line: var.node.start_position().row as u32 + 1,
                    column: 1,
                    message: format!(
                        "`@export` variable `{}` has no type annotation or initializer",
                        var.name,
                    ),
                });
            }
        }
    }
    for inner in class.declarations.iter().filter_map(|d| d.as_class()) {
        check_export_constraints_inner(inner, errors);
    }
}

/// H17: `Object()` constructor must use `Object.new()` instead.
fn check_object_constructor(root: &Node, source: &str, errors: &mut Vec<StructuralError>) {
    check_object_constructor_in_node(*root, source, errors);
}

fn check_object_constructor_in_node(node: Node, source: &str, errors: &mut Vec<StructuralError>) {
    if node.kind() == "call"
        && let Some(func) = node
            .child_by_field_name("function")
            .or_else(|| node.named_child(0))
        && func.kind() == "identifier"
        && func.utf8_text(source.as_bytes()).ok() == Some("Object")
    {
        let pos = node.start_position();
        errors.push(StructuralError {
            line: pos.row as u32 + 1,
            column: pos.column as u32 + 1,
            message: "`Object()` cannot be constructed directly — use `Object.new()` instead"
                .to_string(),
        });
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            check_object_constructor_in_node(cursor.node(), source, errors);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Batch 4: Preload & misc checks
// ---------------------------------------------------------------------------

fn check_preload_and_misc(
    root: &Node,
    source: &str,
    project_root: Option<&Path>,
    errors: &mut Vec<StructuralError>,
) {
    check_preload_path(root, source, project_root, errors);
    check_range_args(root, source, errors);
    check_assert_message(root, source, errors);
}

/// F1: `preload()` path does not exist on disk.
/// F2: `preload()` argument is not a constant string.
fn check_preload_path(
    root: &Node,
    source: &str,
    project_root: Option<&Path>,
    errors: &mut Vec<StructuralError>,
) {
    check_preload_in_node(*root, source, project_root, errors);
}

fn check_preload_in_node(
    node: Node,
    source: &str,
    project_root: Option<&Path>,
    errors: &mut Vec<StructuralError>,
) {
    if node.kind() == "call"
        && let Some(func) = node
            .child_by_field_name("function")
            .or_else(|| node.named_child(0))
        && func.kind() == "identifier"
        && func.utf8_text(source.as_bytes()).ok() == Some("preload")
        && let Some(args) = node.child_by_field_name("arguments")
    {
        let arg_count = args.named_child_count();
        if arg_count == 0 {
            let pos = node.start_position();
            errors.push(StructuralError {
                line: pos.row as u32 + 1,
                column: pos.column as u32 + 1,
                message: "`preload()` requires a path argument".to_string(),
            });
        } else if let Some(arg) = args.named_child(0)
            && arg.kind() != "string"
        {
            let pos = arg.start_position();
            errors.push(StructuralError {
                line: pos.row as u32 + 1,
                column: pos.column as u32 + 1,
                message: "`preload()` argument must be a constant string literal".to_string(),
            });
        } else if let Some(arg) = args.named_child(0)
            && arg.kind() == "string"
            && let Ok(raw) = arg.utf8_text(source.as_bytes())
            && let Some(project_root) = project_root
        {
            // Strip quotes from string literal
            let path_str = raw.trim_matches('"').trim_matches('\'');
            if let Some(rel) = path_str.strip_prefix("res://") {
                let resolved = project_root.join(rel);
                if !resolved.exists() {
                    let pos = arg.start_position();
                    errors.push(StructuralError {
                        line: pos.row as u32 + 1,
                        column: pos.column as u32 + 1,
                        message: format!("preload file \"{path_str}\" does not exist",),
                    });
                }
            }
        }
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            check_preload_in_node(cursor.node(), source, project_root, errors);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

/// H15: `range()` accepts at most 3 arguments (start, end, step).
fn check_range_args(root: &Node, source: &str, errors: &mut Vec<StructuralError>) {
    check_range_in_node(*root, source, errors);
}

fn check_range_in_node(node: Node, source: &str, errors: &mut Vec<StructuralError>) {
    if node.kind() == "call"
        && let Some(func) = node
            .child_by_field_name("function")
            .or_else(|| node.named_child(0))
        && func.kind() == "identifier"
        && func.utf8_text(source.as_bytes()).ok() == Some("range")
        && let Some(args) = node.child_by_field_name("arguments")
    {
        let arg_count = args.named_child_count();
        if arg_count > 3 {
            let pos = node.start_position();
            errors.push(StructuralError {
                line: pos.row as u32 + 1,
                column: pos.column as u32 + 1,
                message: format!("`range()` accepts at most 3 arguments (got {arg_count})",),
            });
        }
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            check_range_in_node(cursor.node(), source, errors);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

/// Check if an expression is string-like: literal string, string format (`"..." % val`),
/// or string concatenation.
fn is_string_like_expr(node: &Node, source: &str) -> bool {
    match node.kind() {
        "string" | "string_name" => true,
        // "format %s" % value — string formatting
        "binary_operator" => {
            let op = node
                .child_by_field_name("op")
                .and_then(|o| o.utf8_text(source.as_bytes()).ok())
                .unwrap_or("");
            if op == "%" {
                // Left side should be a string
                return node
                    .named_child(0)
                    .is_some_and(|c| is_string_like_expr(&c, source));
            }
            // "a" + "b" — concatenation
            if op == "+" {
                return node
                    .named_child(0)
                    .is_some_and(|c| is_string_like_expr(&c, source));
            }
            false
        }
        // str(), String() calls
        "call" => {
            let func = node
                .child_by_field_name("function")
                .or_else(|| node.named_child(0))
                .and_then(|f| f.utf8_text(source.as_bytes()).ok())
                .unwrap_or("");
            matches!(func, "str" | "String")
        }
        _ => false,
    }
}

/// H9: `assert()` second argument (message) must be a string literal.
fn check_assert_message(root: &Node, source: &str, errors: &mut Vec<StructuralError>) {
    check_assert_in_node(*root, source, errors);
}

fn check_assert_in_node(node: Node, source: &str, errors: &mut Vec<StructuralError>) {
    if node.kind() == "call"
        && let Some(func) = node
            .child_by_field_name("function")
            .or_else(|| node.named_child(0))
        && func.kind() == "identifier"
        && func.utf8_text(source.as_bytes()).ok() == Some("assert")
        && let Some(args) = node.child_by_field_name("arguments")
        && args.named_child_count() >= 2
        && let Some(msg_arg) = args.named_child(1)
        && !is_string_like_expr(&msg_arg, source)
    {
        let pos = msg_arg.start_position();
        errors.push(StructuralError {
            line: pos.row as u32 + 1,
            column: pos.column as u32 + 1,
            message: "expected string for assert error message".to_string(),
        });
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            check_assert_in_node(cursor.node(), source, errors);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Batch 6: Advanced semantic checks
// ---------------------------------------------------------------------------

fn check_advanced_semantic(
    root: &Node,
    source: &str,
    file: &GdFile<'_>,
    errors: &mut Vec<StructuralError>,
) {
    check_missing_return(root, source, file, errors);
    check_const_expression_required(root, source, errors);
    check_getter_setter_signature(root, source, file, errors);
}

/// C4: Not all code paths return a value.
/// Functions with a typed non-void return type must return a value on every path.
fn check_missing_return(
    root: &Node,
    source: &str,
    file: &GdFile<'_>,
    errors: &mut Vec<StructuralError>,
) {
    let bytes = source.as_bytes();
    for func in file.funcs() {
        // Only check functions with explicit non-void return type
        let Some(ref ret) = func.return_type else {
            continue;
        };
        if ret.name == "void" || ret.name.is_empty() || ret.is_inferred {
            continue;
        }

        // Find the AST node for this function
        let mut cursor = root.walk();
        for child in root.children(&mut cursor) {
            let func_node = match child.kind() {
                "function_definition" => child,
                "decorated_definition" => {
                    let mut inner_cursor = child.walk();
                    let mut found = None;
                    for inner in child.children(&mut inner_cursor) {
                        if inner.kind() == "function_definition" {
                            found = Some(inner);
                            break;
                        }
                    }
                    if let Some(f) = found { f } else { continue }
                }
                _ => continue,
            };

            let name = func_node
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(bytes).ok())
                .unwrap_or("");
            if name != func.name {
                continue;
            }

            if let Some(body) = func_node.child_by_field_name("body")
                && !body_always_returns(&body, bytes)
            {
                errors.push(StructuralError {
                    line: func.node.start_position().row as u32 + 1,
                    column: 1,
                    message: format!(
                        "not all code paths return a value in `{name}()` (declared -> {})",
                        ret.name,
                    ),
                });
            }
        }
    }
    for inner in file.inner_classes() {
        check_missing_return_inner(root, source, inner, errors);
    }
}

fn check_missing_return_inner(
    _root: &Node,
    source: &str,
    class: &GdClass<'_>,
    errors: &mut Vec<StructuralError>,
) {
    let bytes = source.as_bytes();
    // Search within the class's own class_body, not the file root.
    let class_body = class.node.child_by_field_name("body");
    let Some(class_body) = class_body else { return };

    for func in class.declarations.iter().filter_map(|d| d.as_func()) {
        let Some(ref ret) = func.return_type else {
            continue;
        };
        if ret.name == "void" || ret.name.is_empty() || ret.is_inferred {
            continue;
        }

        let mut cursor = class_body.walk();
        for child in class_body.children(&mut cursor) {
            let func_node = match child.kind() {
                "function_definition" => child,
                "decorated_definition" => {
                    let mut inner_cursor = child.walk();
                    let mut found = None;
                    for inner in child.children(&mut inner_cursor) {
                        if inner.kind() == "function_definition" {
                            found = Some(inner);
                            break;
                        }
                    }
                    if let Some(f) = found { f } else { continue }
                }
                _ => continue,
            };

            let name = func_node
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(bytes).ok())
                .unwrap_or("");
            if name != func.name {
                continue;
            }

            if let Some(body) = func_node.child_by_field_name("body")
                && !body_always_returns(&body, bytes)
            {
                errors.push(StructuralError {
                    line: func.node.start_position().row as u32 + 1,
                    column: 1,
                    message: format!(
                        "not all code paths return a value in `{name}()` (declared -> {})",
                        ret.name,
                    ),
                });
            }
        }
    }
    for inner in class.declarations.iter().filter_map(|d| d.as_class()) {
        check_missing_return_inner(&class.node, source, inner, errors);
    }
}

/// Check if a body node always returns a value (all code paths end in return).
fn body_always_returns(body: &Node, source: &[u8]) -> bool {
    let mut cursor = body.walk();
    let children: Vec<_> = body
        .children(&mut cursor)
        .filter(tree_sitter::Node::is_named)
        .collect();

    // An empty body doesn't return
    if children.is_empty() {
        return false;
    }

    let last = children.last().unwrap();
    statement_always_returns(last, source)
}

/// Check if a statement always returns a value.
fn statement_always_returns(node: &Node, source: &[u8]) -> bool {
    match node.kind() {
        "return_statement" => true,
        "if_statement" => {
            // Must have an else branch, and all branches must return
            let mut has_else = false;
            let mut all_branches_return = true;
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                match child.kind() {
                    "body" => {
                        if !body_always_returns(&child, source) {
                            all_branches_return = false;
                        }
                    }
                    "elif_clause" => {
                        if let Some(body) = child.child_by_field_name("body") {
                            if !body_always_returns(&body, source) {
                                all_branches_return = false;
                            }
                        } else {
                            all_branches_return = false;
                        }
                    }
                    "else_clause" => {
                        has_else = true;
                        if let Some(body) = child.child_by_field_name("body") {
                            if !body_always_returns(&body, source) {
                                all_branches_return = false;
                            }
                        } else {
                            // Walk children to find body
                            let mut ec = child.walk();
                            let else_body_returns = child
                                .children(&mut ec)
                                .any(|c| c.kind() == "body" && body_always_returns(&c, source));
                            if !else_body_returns {
                                all_branches_return = false;
                            }
                        }
                    }
                    _ => {}
                }
            }
            has_else && all_branches_return
        }
        "match_statement" => {
            // All match arms must return. Check for a catch-all pattern.
            let mut cursor = node.walk();
            let mut has_catchall = false;
            let mut all_arms_return = true;
            for child in node.children(&mut cursor) {
                if child.kind() == "match_body" {
                    let mut mc = child.walk();
                    for arm in child.children(&mut mc) {
                        if arm.kind() == "pattern_section" {
                            let mut pc = arm.walk();
                            for p in arm.children(&mut pc) {
                                // Wildcard `_` may be a direct child or inside a `pattern` wrapper
                                if p.kind() == "identifier" && p.utf8_text(source).ok() == Some("_")
                                {
                                    has_catchall = true;
                                }
                                if p.kind() == "pattern" {
                                    let mut inner = p.walk();
                                    for pat_child in p.children(&mut inner) {
                                        if pat_child.kind() == "identifier"
                                            && pat_child.utf8_text(source).ok() == Some("_")
                                        {
                                            has_catchall = true;
                                        }
                                    }
                                }
                                if p.kind() == "body" && !body_always_returns(&p, source) {
                                    all_arms_return = false;
                                }
                            }
                        }
                    }
                }
            }
            has_catchall && all_arms_return
        }
        _ => false,
    }
}

/// F3: Constant expression required — const and enum values must be compile-time constants.
fn check_const_expression_required(root: &Node, source: &str, errors: &mut Vec<StructuralError>) {
    check_const_expr_in_node(*root, source, errors);
}

fn check_const_expr_in_node(node: Node, source: &str, errors: &mut Vec<StructuralError>) {
    let bytes = source.as_bytes();

    // Check const declarations: value must be a constant expression
    if node.kind() == "const_statement"
        && let Some(value) = node.child_by_field_name("value")
        && !is_const_expression(&value, bytes)
    {
        let name = node
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(bytes).ok())
            .unwrap_or("?");
        let pos = value.start_position();
        errors.push(StructuralError {
            line: pos.row as u32 + 1,
            column: pos.column as u32 + 1,
            message: format!("constant `{name}` requires a compile-time constant expression",),
        });
    }

    // Check enum member values: must also be constant expressions
    if node.kind() == "enum_definition"
        && let Some(body) = node.child_by_field_name("body")
    {
        let mut cursor2 = body.walk();
        for child in body.children(&mut cursor2) {
            if child.kind() == "enumerator"
                && let Some(value) = child.child_by_field_name("right")
                && !is_const_expression(&value, bytes)
            {
                let pos = value.start_position();
                errors.push(StructuralError {
                    line: pos.row as u32 + 1,
                    column: pos.column as u32 + 1,
                    message: "enum values must be constant expressions".to_string(),
                });
            }
        }
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            check_const_expr_in_node(cursor.node(), source, errors);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

/// Check if an expression is a compile-time constant.
#[allow(clippy::too_many_lines)]
fn is_const_expression(node: &Node, source: &[u8]) -> bool {
    match node.kind() {
        // Literals are always constant
        "integer" | "float" | "string" | "true" | "false" | "null" | "string_name" => true,
        // Negative literal: unary_operator with -
        "unary_operator" => node
            .named_child(0)
            .is_some_and(|c| is_const_expression(&c, source)),
        // Constant identifier references (UPPER_CASE, PascalCase, or known builtins)
        "identifier" => {
            let text = node.utf8_text(source).unwrap_or("");
            // Allow references to other constants (UPPER_CASE), class/enum names (PascalCase),
            // or preload/INF/NAN/PI/TAU
            is_upper_snake_case(text)
                || matches!(text, "INF" | "NAN" | "PI" | "TAU" | "INFINITY" | "preload")
                || text.starts_with(|c: char| c.is_ascii_uppercase())
                // Bare function references (e.g. `absf`, `sqrt`) are valid const Callable values
                || crate::class_db::utility_function(text).is_some()
                // Check if identifier matches a const declaration in the enclosing scope
                || is_local_const_name(node, text, source)
        }
        // Array/dictionary literals with all-constant elements
        "array" => {
            let mut cursor = node.walk();
            node.named_children(&mut cursor)
                .all(|c| c.kind() == "comment" || is_const_expression(&c, source))
        }
        "dictionary" => {
            let mut cursor = node.walk();
            node.named_children(&mut cursor).all(|c| {
                if c.kind() == "pair" {
                    c.named_child(0)
                        .is_some_and(|k| is_const_expression(&k, source))
                        && c.named_child(1)
                            .is_some_and(|v| is_const_expression(&v, source))
                } else if c.kind() == "comment" {
                    true
                } else {
                    is_const_expression(&c, source)
                }
            })
        }
        // Binary operations on constants
        "binary_operator" => {
            // `as` cast: only the left operand needs to be const (right is a type name)
            let is_as_cast = node
                .child_by_field_name("op")
                .and_then(|op| op.utf8_text(source).ok())
                .is_some_and(|op| op == "as");
            if is_as_cast {
                node.named_child(0)
                    .is_some_and(|c| is_const_expression(&c, source))
            } else {
                node.named_child(0)
                    .is_some_and(|c| is_const_expression(&c, source))
                    && node
                        .named_child(1)
                        .is_some_and(|c| is_const_expression(&c, source))
            }
        }
        // Parenthesized expression
        "parenthesized_expression" => node
            .named_child(0)
            .is_some_and(|c| is_const_expression(&c, source)),
        // Class.CONSTANT or enum access
        "attribute" => {
            // Check for a call suffix — if it has one, check if it's a .new() constructor
            let mut cursor = node.walk();
            let call_child = node
                .children(&mut cursor)
                .find(|c| c.kind() == "attribute_call");
            match call_child {
                None => true, // No call — Class.CONSTANT is constant
                Some(call) => {
                    // Allow Type.new() as a constant expression
                    let method = call
                        .named_child(0)
                        .and_then(|n| n.utf8_text(source).ok())
                        .unwrap_or("");
                    method == "new"
                }
            }
        }
        // preload() and type constructors with constant args are constant expressions
        "call" => {
            let func_name = node
                .child_by_field_name("function")
                .or_else(|| node.named_child(0))
                .and_then(|f| f.utf8_text(source).ok())
                .unwrap_or("");
            if func_name == "preload" {
                return true;
            }
            // Type constructors: Color(...), Vector2(...), etc. with all-constant args
            // Also @GDScript utility functions: Color8(), etc.
            if is_builtin_constructor(func_name) || is_gdscript_utility_const(func_name) {
                let args = node.child_by_field_name("arguments");
                return args.is_none_or(|a| {
                    let mut cursor = a.walk();
                    a.named_children(&mut cursor)
                        .all(|c| is_const_expression(&c, source))
                });
            }
            false
        }
        // Subscript on const base + const index: [1, 2, 3][0]
        "subscript" => {
            node.named_child(0)
                .is_some_and(|base| is_const_expression(&base, source))
                && node
                    .named_child(1)
                    .and_then(|idx| {
                        // tree-sitter wraps the index in `subscript_arguments` — unwrap it
                        if idx.kind() == "subscript_arguments" {
                            idx.named_child(0)
                        } else {
                            Some(idx)
                        }
                    })
                    .is_some_and(|idx| is_const_expression(&idx, source))
        }
        _ => false,
    }
}

/// Check if an identifier matches a `const` declaration in the enclosing scope.
/// Walks up the AST to find the nearest `body`/`class_body`/root and searches
/// sibling `const_statement` nodes for a matching name.
fn is_local_const_name(node: &Node, name: &str, source: &[u8]) -> bool {
    // Walk up to find the enclosing scope (body, class_body, or root)
    let mut current = *node;
    while let Some(parent) = current.parent() {
        match parent.kind() {
            "body" | "class_body" | "source" => {
                // Search sibling const_statement nodes for matching name
                let mut cursor = parent.walk();
                for child in parent.children(&mut cursor) {
                    if child.kind() == "const_statement"
                        && let Some(name_node) = child.child_by_field_name("name")
                        && name_node.utf8_text(source).ok() == Some(name)
                    {
                        return true;
                    }
                }
                break;
            }
            _ => {}
        }
        current = parent;
    }
    false
}

fn is_builtin_constructor(name: &str) -> bool {
    matches!(
        name,
        "Color"
            | "Vector2"
            | "Vector2i"
            | "Vector3"
            | "Vector3i"
            | "Vector4"
            | "Vector4i"
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
            | "NodePath"
            | "StringName"
            | "Array"
            | "Dictionary"
            | "int"
            | "float"
            | "bool"
            | "String"
    )
}

/// @GDScript utility functions that produce constant values when given constant args.
fn is_gdscript_utility_const(name: &str) -> bool {
    matches!(
        name,
        "Color8"
            | "deg_to_rad"
            | "rad_to_deg"
            | "sin"
            | "cos"
            | "tan"
            | "asin"
            | "acos"
            | "atan"
            | "atan2"
            | "sqrt"
            | "pow"
            | "abs"
            | "absf"
            | "absi"
            | "ceil"
            | "ceilf"
            | "ceili"
            | "floor"
            | "floorf"
            | "floori"
            | "round"
            | "roundf"
            | "roundi"
            | "clamp"
            | "clampf"
            | "clampi"
            | "min"
            | "max"
            | "minf"
            | "maxf"
            | "mini"
            | "maxi"
            | "log"
            | "exp"
            | "lerp"
            | "lerpf"
            | "sign"
            | "signf"
            | "signi"
            | "fmod"
            | "fposmod"
            | "posmod"
    )
}

/// H1: Getter/setter signature mismatch.
/// A property's `set(value)` function must have exactly 1 parameter.
/// A property's `get()` function must have 0 parameters and match the property type.
fn check_getter_setter_signature(
    root: &Node,
    source: &str,
    file: &GdFile<'_>,
    errors: &mut Vec<StructuralError>,
) {
    check_getset_in_node(*root, source.as_bytes(), file, errors);
}

fn check_getset_in_node(
    node: Node,
    source: &[u8],
    file: &GdFile<'_>,
    errors: &mut Vec<StructuralError>,
) {
    // Handle inline getter/setter nodes (direct set(v):/get(): syntax)
    if node.kind() == "setter"
        && let Some(params) = node.child_by_field_name("parameters")
    {
        let param_count = params.named_child_count();
        if param_count != 1 {
            let pos = node.start_position();
            errors.push(StructuralError {
                line: pos.row as u32 + 1,
                column: pos.column as u32 + 1,
                message: format!(
                    "property setter must have exactly 1 parameter (got {param_count})",
                ),
            });
        }
    }
    if node.kind() == "getter"
        && let Some(params) = node.child_by_field_name("parameters")
        && params.named_child_count() > 0
    {
        let pos = node.start_position();
        errors.push(StructuralError {
            line: pos.row as u32 + 1,
            column: pos.column as u32 + 1,
            message: "property getter cannot have parameters".to_string(),
        });
    }

    // Handle named getter/setter in setget node: `get = _func_name` / `set = _func_name`
    // The tree-sitter AST has: setget > [get "=" getter="_func_name"] or [set "=" setter="_func_name"]
    if node.kind() == "setget" {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            // Unnamed "getter" node contains the function name for `get = func`
            if child.kind() == "getter"
                && let Ok(func_name) = child.utf8_text(source)
                && !func_name.is_empty()
                && let Some(func) = file.funcs().find(|f| f.name == func_name)
                && !func.params.is_empty()
            {
                let pos = child.start_position();
                errors.push(StructuralError {
                    line: pos.row as u32 + 1,
                    column: pos.column as u32 + 1,
                    message: format!(
                        "function `{func_name}` cannot be used as getter because of its signature",
                    ),
                });
            }
            // Unnamed "setter" node contains the function name for `set = func`
            if child.kind() == "setter"
                && let Ok(func_name) = child.utf8_text(source)
                && !func_name.is_empty()
                && let Some(func) = file.funcs().find(|f| f.name == func_name)
                && func.params.len() != 1
            {
                let pos = child.start_position();
                errors.push(StructuralError {
                    line: pos.row as u32 + 1,
                    column: pos.column as u32 + 1,
                    message: format!(
                        "function `{func_name}` cannot be used as setter because of its signature",
                    ),
                });
            }
        }
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            check_getset_in_node(cursor.node(), source, file, errors);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}
