use std::env;
use std::path::Path;

use clap::Args;
use miette::Result;
use owo_colors::OwoColorize;
use serde::Serialize;
use tree_sitter::Node;

use crate::core::workspace_index::ProjectIndex;
use crate::core::{
    config::Config, config::find_project_root, fs::collect_gdscript_files,
    fs::collect_resource_files, parser, resource_parser, scene, symbol_table, type_inference,
};
use crate::core::symbol_table::SymbolTable;
use crate::lint::matches_ignore_pattern;
use crate::lint::rules::LintRule;
use crate::lint::rules::duplicate_function::DuplicateFunction;
use crate::lint::rules::duplicate_key::DuplicateKey;
use crate::lint::rules::duplicate_signal::DuplicateSignal;
use crate::lint::rules::duplicate_variable::DuplicateVariable;
use crate::lint::rules::get_node_default_without_onready::GetNodeDefaultWithoutOnready;
use crate::lint::rules::native_method_override::NativeMethodOverride;
use crate::lint::rules::onready_with_export::OnreadyWithExport;
use crate::lint::rules::override_signature_mismatch::OverrideSignatureMismatch;
use crate::{ceprintln, cprintln};

#[derive(Args)]
pub struct CheckArgs {
    /// Files or directories to check (defaults to current directory)
    pub paths: Vec<String>,
    /// Output format (text or json)
    #[arg(long, default_value = "text")]
    pub format: String,
}

#[derive(Serialize)]
struct CheckOutput {
    files_checked: u32,
    files_with_errors: u32,
    errors: Vec<ParseError>,
    ok: bool,
}

#[derive(Serialize)]
struct ParseError {
    file: String,
    line: u32,
    column: u32,
    message: String,
}

#[allow(clippy::too_many_lines)]
pub fn exec(args: &CheckArgs) -> Result<()> {
    let cwd = env::current_dir().unwrap_or_default();
    let config = Config::load(&cwd)?;
    let ignore_base = find_project_root(&cwd).unwrap_or_else(|| cwd.clone());

    let roots = if args.paths.is_empty() {
        vec![cwd.clone()]
    } else {
        args.paths.iter().map(std::path::PathBuf::from).collect()
    };

    // Build project-wide index for cross-file override checking
    let project_index = ProjectIndex::build(&ignore_base);

    let json_mode = args.format == "json";
    let mut error_count = 0u32;
    let mut checked = 0u32;
    let mut parse_errors = Vec::new();

    for root in &roots {
        let files = collect_gdscript_files(root)?;
        for file in &files {
            if matches_ignore_pattern(file, &ignore_base, &config.lint.ignore_patterns) {
                continue;
            }
            checked += 1;
            match parser::parse_file(file) {
                Ok((source, tree)) => {
                    let root_node = tree.root_node();
                    let has_parse_errors = root_node.has_error();
                    let symbols = symbol_table::build(&tree, &source);
                    let structural = validate_structure(&root_node, &source, &symbols);
                    let classdb = check_classdb_errors(
                        &root_node, &source, &symbols, &project_index,
                    );
                    let duplicates = check_duplicates(&tree, &source);
                    let promoted = check_promoted_rules(&tree, &source, &symbols);
                    let overrides =
                        check_overrides(&tree, &source, &symbols, &project_index);

                    let has_errors = has_parse_errors
                        || !structural.is_empty()
                        || !classdb.is_empty()
                        || !duplicates.is_empty()
                        || !promoted.is_empty()
                        || !overrides.is_empty();
                    if has_errors {
                        error_count += 1;
                        if json_mode {
                            let rel = crate::core::fs::relative_slash(file, &cwd);
                            if has_parse_errors {
                                let mut cursor = root_node.walk();
                                collect_errors(&mut cursor, file, &cwd, &mut parse_errors);
                            }
                            for err in structural.iter().chain(classdb.iter()) {
                                parse_errors.push(ParseError {
                                    file: rel.clone(),
                                    line: err.line,
                                    column: err.column,
                                    message: err.message.clone(),
                                });
                            }
                            for diag in duplicates
                                .iter()
                                .chain(promoted.iter())
                                .chain(overrides.iter())
                            {
                                parse_errors.push(ParseError {
                                    file: rel.clone(),
                                    line: diag.line as u32 + 1,
                                    column: diag.column as u32 + 1,
                                    message: diag.message.clone(),
                                });
                            }
                        } else {
                            if has_parse_errors {
                                let mut cursor = root_node.walk();
                                report_errors(&mut cursor, &source, file);
                            }
                            report_structural(&structural, &source, file);
                            report_structural(&classdb, &source, file);
                            report_duplicates(&duplicates, &source, file);
                            report_duplicates(&promoted, &source, file);
                            report_duplicates(&overrides, &source, file);
                        }
                    }
                }
                Err(e) => {
                    error_count += 1;
                    if json_mode {
                        let rel = crate::core::fs::relative_slash(file, &cwd);
                        parse_errors.push(ParseError {
                            file: rel,
                            line: 0,
                            column: 0,
                            message: format!("{e}"),
                        });
                    } else {
                        ceprintln!("{e}");
                    }
                }
            }
        }
    }

    // Check resource files (.tscn/.tres)
    for root in &roots {
        let project_root = find_project_root(root).or_else(|| find_project_root(&cwd));
        let resource_files = collect_resource_files(root)?;
        for file in &resource_files {
            if matches_ignore_pattern(file, &ignore_base, &config.lint.ignore_patterns) {
                continue;
            }
            checked += 1;
            match resource_parser::parse_resource_file(file) {
                Ok((source, tree)) => {
                    let root_node = tree.root_node();
                    if root_node.has_error() {
                        error_count += 1;
                        if json_mode {
                            let mut cursor = root_node.walk();
                            collect_errors(&mut cursor, file, &cwd, &mut parse_errors);
                        } else {
                            let mut cursor = root_node.walk();
                            report_errors(&mut cursor, &source, file);
                        }
                    }

                    // Validate resource paths and references in .tscn files
                    if let Some(ext) = file.extension()
                        && ext == "tscn"
                        && let Some(ref proj_root) = project_root
                        && let Ok(scene_data) = scene::parse_scene(&source)
                    {
                        let scene_errors = validate_scene(&scene_data, proj_root, file, &cwd);
                        if !scene_errors.is_empty() {
                            error_count += 1;
                        }
                        if json_mode {
                            parse_errors.extend(scene_errors);
                        } else {
                            report_scene_errors(&scene_errors, file);
                        }
                    }
                }
                Err(e) => {
                    error_count += 1;
                    if json_mode {
                        let rel = crate::core::fs::relative_slash(file, &cwd);
                        parse_errors.push(ParseError {
                            file: rel,
                            line: 0,
                            column: 0,
                            message: format!("{e}"),
                        });
                    } else {
                        ceprintln!("{e}");
                    }
                }
            }
        }
    }

    if json_mode {
        let output = CheckOutput {
            files_checked: checked,
            files_with_errors: error_count,
            errors: parse_errors,
            ok: error_count == 0,
        };
        let json = serde_json::to_string_pretty(&output).map_err(|e| miette::miette!("{e}"))?;
        cprintln!("{json}");
        if !output.ok {
            std::process::exit(1);
        }
        return Ok(());
    }

    if error_count > 0 {
        ceprintln!("\n{checked} files checked, {error_count} with parse errors");
        std::process::exit(1);
    }

    cprintln!("{} {} files checked", "✓".green(), checked);
    Ok(())
}

// ---------------------------------------------------------------------------
// Structural validation — catches patterns tree-sitter accepts but Godot rejects
// ---------------------------------------------------------------------------

struct StructuralError {
    line: u32,
    column: u32,
    message: String,
}

/// Run structural checks that go beyond tree-sitter error nodes.
fn validate_structure(root: &Node, source: &str, symbols: &SymbolTable) -> Vec<StructuralError> {
    let mut errors = Vec::new();
    check_top_level_statements(root, &mut errors);
    check_indentation_consistency(root, &mut errors);
    check_class_constants(root, source, &mut errors);
    check_variant_inference(root, source, &mut errors);
    check_declaration_constraints(root, source, symbols, &mut errors);
    check_semantic_errors(root, source, symbols, &mut errors);
    check_preload_and_misc(root, source, &mut errors);
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
    let mut cursor = body.walk();

    for child in body.children(&mut cursor) {
        if !child.is_named() || child.kind() == "comment" {
            continue;
        }
        let col = child.start_position().column;
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
fn check_variant_inference(root: &Node, source: &str, errors: &mut Vec<StructuralError>) {
    check_variant_node(*root, source, errors);
}

fn check_variant_node(node: Node, source: &str, errors: &mut Vec<StructuralError>) {
    if node.kind() == "variable_statement" {
        // Check for := (tree-sitter stores this as type field with "inferred_type" kind)
        let is_inferred = node
            .child_by_field_name("type")
            .is_some_and(|t| t.kind() == "inferred_type");
        if is_inferred && let Some(value) = node.child_by_field_name("value") {
            let should_flag = if is_variant_producing_expr(&value, source) {
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
            check_variant_node(cursor.node(), source, errors);
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
fn is_variant_producing_expr(node: &Node, source: &str) -> bool {
    match node.kind() {
        // dict["key"], arr[idx]
        "subscript" => true,
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
            // `in` / `not in` return Variant in Godot's static type system
            if is_in_operator(node, source) {
                return true;
            }
            node.named_child(0)
                .is_some_and(|c| is_variant_producing_expr(&c, source))
                || node
                    .named_child(1)
                    .is_some_and(|c| is_variant_producing_expr(&c, source))
        }
        // Parenthesized: unwrap and check inner expression
        "parenthesized_expression" => node
            .named_child(0)
            .is_some_and(|c| is_variant_producing_expr(&c, source)),
        // Unary operators: `not dict["key"]`
        "unary_operator" => node
            .child_by_field_name("operand")
            .is_some_and(|c| is_variant_producing_expr(&c, source)),
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
    symbols: &SymbolTable,
    errors: &mut Vec<StructuralError>,
) {
    check_init_return_type(symbols, errors);
    check_mandatory_after_optional(symbols, errors);
    check_signal_default_values(root, source, errors);
    check_duplicate_class_name_extends(root, source, errors);
    check_duplicate_param_names(symbols, errors);
    check_yield_keyword(root, source, errors);
    check_static_init_params(symbols, errors);
    check_duplicate_tool(root, source, errors);
}

/// G4: Constructor `_init` cannot have a return type.
fn check_init_return_type(symbols: &SymbolTable, errors: &mut Vec<StructuralError>) {
    for func in &symbols.functions {
        if func.name == "_init" && func.return_type.is_some() {
            errors.push(StructuralError {
                line: func.line as u32 + 1,
                column: 1,
                message: "constructor `_init()` cannot have a return type".to_string(),
            });
        }
    }
    for (_, inner) in &symbols.inner_classes {
        check_init_return_type(inner, errors);
    }
}

/// G3: Mandatory parameter after optional parameter.
fn check_mandatory_after_optional(symbols: &SymbolTable, errors: &mut Vec<StructuralError>) {
    for func in &symbols.functions {
        let mut seen_optional = false;
        for param in &func.params {
            if param.has_default {
                seen_optional = true;
            } else if seen_optional {
                errors.push(StructuralError {
                    line: func.line as u32 + 1,
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
    for (_, inner) in &symbols.inner_classes {
        check_mandatory_after_optional(inner, errors);
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
            if param.kind() == "default_parameter"
                || param.kind() == "typed_default_parameter"
            {
                let param_name =
                    first_identifier_text(&param, source.as_bytes()).unwrap_or("?");
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
fn check_duplicate_param_names(symbols: &SymbolTable, errors: &mut Vec<StructuralError>) {
    for func in &symbols.functions {
        let mut seen = std::collections::HashSet::new();
        for param in &func.params {
            if !seen.insert(&param.name) {
                errors.push(StructuralError {
                    line: func.line as u32 + 1,
                    column: 1,
                    message: format!(
                        "duplicate parameter name `{}` in `{}()`",
                        param.name, func.name,
                    ),
                });
            }
        }
    }
    for (_, inner) in &symbols.inner_classes {
        check_duplicate_param_names(inner, errors);
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
fn check_static_init_params(symbols: &SymbolTable, errors: &mut Vec<StructuralError>) {
    for func in &symbols.functions {
        if func.name == "_static_init" && !func.params.is_empty() {
            errors.push(StructuralError {
                line: func.line as u32 + 1,
                column: 1,
                message: "`_static_init()` cannot have parameters".to_string(),
            });
        }
    }
    for (_, inner) in &symbols.inner_classes {
        check_static_init_params(inner, errors);
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
fn find_annotation_name<'a>(node: &Node, source: &'a str) -> Option<&'a str> {
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
    symbols: &SymbolTable,
    errors: &mut Vec<StructuralError>,
) {
    check_static_context_violations(root, source, symbols, errors);
    check_assign_to_constant(root, source, symbols, errors);
    check_void_return_value(root, source, symbols, errors);
    check_get_node_in_static(root, source, errors);
    check_export_constraints(symbols, errors);
    check_object_constructor(root, source, errors);
}

/// C1: Static context violations — using instance vars, `self`, or instance methods
/// from a static function.
fn check_static_context_violations(
    root: &Node,
    source: &str,
    symbols: &SymbolTable,
    errors: &mut Vec<StructuralError>,
) {
    let bytes = source.as_bytes();
    // Collect instance (non-static) member names for reference
    let instance_vars: std::collections::HashSet<&str> = symbols
        .variables
        .iter()
        .filter(|v| !v.is_static && !v.is_constant)
        .map(|v| v.name.as_str())
        .collect();
    let instance_funcs: std::collections::HashSet<&str> = symbols
        .functions
        .iter()
        .filter(|f| !f.is_static)
        .map(|f| f.name.as_str())
        .collect();

    // Walk functions that are static
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

        // Check if this function is static
        let is_static = symbols
            .functions
            .iter()
            .any(|f| {
                f.is_static
                    && func_node
                        .child_by_field_name("name")
                        .and_then(|n| n.utf8_text(bytes).ok())
                        == Some(&f.name)
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
            check_static_body(
                &body,
                bytes,
                func_name,
                &instance_vars,
                &instance_funcs,
                errors,
            );
        }
    }
}

fn check_static_body(
    node: &Node,
    source: &[u8],
    func_name: &str,
    instance_vars: &std::collections::HashSet<&str>,
    instance_funcs: &std::collections::HashSet<&str>,
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
        } else if name == "self" {
            let pos = node.start_position();
            errors.push(StructuralError {
                line: pos.row as u32 + 1,
                column: pos.column as u32 + 1,
                message: format!(
                    "cannot use `self` in static function `{func_name}()`",
                ),
            });
            return;
        } else if instance_vars.contains(name) {
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
        && let Some(func_node) =
            node.child_by_field_name("function").or_else(|| node.named_child(0))
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
    symbols: &SymbolTable,
    errors: &mut Vec<StructuralError>,
) {
    let bytes = source.as_bytes();
    let constants: std::collections::HashSet<&str> = symbols
        .variables
        .iter()
        .filter(|v| v.is_constant)
        .map(|v| v.name.as_str())
        .collect();
    let enum_members: std::collections::HashSet<&str> = symbols
        .enums
        .iter()
        .flat_map(|e| e.members.iter().map(String::as_str))
        .collect();

    check_assign_to_const_in_node(*root, bytes, &constants, &enum_members, errors);
}

fn check_assign_to_const_in_node(
    node: Node,
    source: &[u8],
    constants: &std::collections::HashSet<&str>,
    enum_members: &std::collections::HashSet<&str>,
    errors: &mut Vec<StructuralError>,
) {
    // Assignments: assignment_statement/augmented_assignment_statement at top level,
    // or expression_statement > assignment/augmented_assignment inside function bodies
    let assign_node = match node.kind() {
        "assignment_statement" | "augmented_assignment_statement" | "assignment"
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
        && lhs.kind() == "identifier"
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
        }
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            check_assign_to_const_in_node(cursor.node(), source, constants, enum_members, errors);
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
    symbols: &SymbolTable,
    errors: &mut Vec<StructuralError>,
) {
    let bytes = source.as_bytes();
    for func in &symbols.functions {
        if let Some(ref ret) = func.return_type
            && ret.name == "void"
        {
            // Find the AST node for this function and check for `return <value>`
            check_void_func_returns(*root, bytes, &func.name, errors);
        }
    }
    for (_, inner) in &symbols.inner_classes {
        check_void_return_value(root, source, inner, errors);
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

fn check_returns_in_body(
    node: &Node,
    func_name: &str,
    errors: &mut Vec<StructuralError>,
) {
    if node.kind() == "return_statement" {
        // Check if there's a value after `return`
        if node.named_child_count() > 0 {
            let pos = node.start_position();
            errors.push(StructuralError {
                line: pos.row as u32 + 1,
                column: pos.column as u32 + 1,
                message: format!(
                    "void function `{func_name}()` cannot return a value",
                ),
            });
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
fn check_get_node_in_static(
    root: &Node,
    source: &str,
    errors: &mut Vec<StructuralError>,
) {
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

fn check_get_node_in_body(
    node: &Node,
    func_name: &str,
    errors: &mut Vec<StructuralError>,
) {
    if node.kind() == "get_node" {
        let pos = node.start_position();
        errors.push(StructuralError {
            line: pos.row as u32 + 1,
            column: pos.column as u32 + 1,
            message: format!(
                "cannot use `$`/`%` get_node in static function `{func_name}()`",
            ),
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
fn check_export_constraints(symbols: &SymbolTable, errors: &mut Vec<StructuralError>) {
    for var in &symbols.variables {
        let export_count = var.annotations.iter().filter(|a| a.as_str() == "export").count();
        let has_export = export_count > 0;

        if has_export {
            // E4: Duplicate @export
            if export_count > 1 {
                errors.push(StructuralError {
                    line: var.line as u32 + 1,
                    column: 1,
                    message: format!(
                        "duplicate `@export` annotation on `{}`",
                        var.name,
                    ),
                });
            }

            // E3: @export on static
            if var.is_static {
                errors.push(StructuralError {
                    line: var.line as u32 + 1,
                    column: 1,
                    message: format!(
                        "`@export` cannot be used on static variable `{}`",
                        var.name,
                    ),
                });
            }

            // E1: @export without type or initializer
            // Only check plain @export, not @export_* variants
            let has_type = var.type_ann.as_ref().is_some_and(|t| !t.name.is_empty());
            if !has_type && !var.has_default {
                errors.push(StructuralError {
                    line: var.line as u32 + 1,
                    column: 1,
                    message: format!(
                        "`@export` variable `{}` has no type annotation or initializer",
                        var.name,
                    ),
                });
            }
        }
    }
    for (_, inner) in &symbols.inner_classes {
        check_export_constraints(inner, errors);
    }
}

/// H17: `Object()` constructor must use `Object.new()` instead.
fn check_object_constructor(root: &Node, source: &str, errors: &mut Vec<StructuralError>) {
    check_object_constructor_in_node(*root, source, errors);
}

fn check_object_constructor_in_node(
    node: Node,
    source: &str,
    errors: &mut Vec<StructuralError>,
) {
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
    errors: &mut Vec<StructuralError>,
) {
    check_preload_path(root, source, errors);
    check_range_args(root, source, errors);
}

/// F1: `preload()` path does not exist on disk.
/// F2: `preload()` argument is not a constant string.
fn check_preload_path(root: &Node, source: &str, errors: &mut Vec<StructuralError>) {
    check_preload_in_node(*root, source, errors);
}

fn check_preload_in_node(node: Node, source: &str, errors: &mut Vec<StructuralError>) {
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
                message: "`preload()` argument must be a constant string literal"
                    .to_string(),
            });
        }
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            check_preload_in_node(cursor.node(), source, errors);
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
                message: format!(
                    "`range()` accepts at most 3 arguments (got {arg_count})",
                ),
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

// ---------------------------------------------------------------------------
// Batch 5: ClassDB / signature lookup checks
// ---------------------------------------------------------------------------

/// Batch 5: Check type annotations, class_name shadowing, enum shadowing.
fn check_classdb_errors(
    root: &Node,
    source: &str,
    symbols: &SymbolTable,
    project: &ProjectIndex,
) -> Vec<StructuralError> {
    let mut errors = Vec::new();
    check_class_name_shadows_native(symbols, &mut errors);
    check_enum_shadows_builtin(symbols, &mut errors);
    check_type_annotations_resolve(root, source, symbols, project, &mut errors);
    errors
}

/// H5: `class_name` shadows a native Godot class.
fn check_class_name_shadows_native(
    symbols: &SymbolTable,
    errors: &mut Vec<StructuralError>,
) {
    if let Some(ref name) = symbols.class_name
        && crate::class_db::class_exists(name)
    {
        errors.push(StructuralError {
            line: 1,
            column: 1,
            message: format!(
                "`class_name {name}` shadows the native Godot class `{name}`",
            ),
        });
    }
    for (inner_name, inner) in &symbols.inner_classes {
        if crate::class_db::class_exists(inner_name) {
            errors.push(StructuralError {
                line: 1,
                column: 1,
                message: format!(
                    "inner class `{inner_name}` shadows the native Godot class `{inner_name}`",
                ),
            });
        }
        check_class_name_shadows_native(inner, errors);
    }
}

/// G5: Enum name or member name shadows a builtin type.
fn check_enum_shadows_builtin(symbols: &SymbolTable, errors: &mut Vec<StructuralError>) {
    let builtin_types = [
        "bool", "int", "float", "String", "Vector2", "Vector2i", "Vector3", "Vector3i",
        "Vector4", "Vector4i", "Rect2", "Rect2i", "Transform2D", "Transform3D",
        "Plane", "Quaternion", "AABB", "Basis", "Projection", "Color",
        "NodePath", "StringName", "RID", "Callable", "Signal",
        "Dictionary", "Array", "PackedByteArray", "PackedInt32Array",
        "PackedInt64Array", "PackedFloat32Array", "PackedFloat64Array",
        "PackedStringArray", "PackedVector2Array", "PackedVector3Array",
        "PackedColorArray", "PackedVector4Array", "Nil", "Object",
    ];
    for e in &symbols.enums {
        if !e.name.is_empty() && builtin_types.contains(&e.name.as_str()) {
            errors.push(StructuralError {
                line: e.line as u32 + 1,
                column: 1,
                message: format!(
                    "enum `{name}` shadows the built-in type `{name}`",
                    name = e.name,
                ),
            });
        }
    }
    for (_, inner) in &symbols.inner_classes {
        check_enum_shadows_builtin(inner, errors);
    }
}

/// A4: Type annotation doesn't resolve to a known type.
fn check_type_annotations_resolve(
    root: &Node,
    source: &str,
    symbols: &SymbolTable,
    project: &ProjectIndex,
    errors: &mut Vec<StructuralError>,
) {
    check_type_annotations_in_node(*root, source, symbols, project, errors);
}

fn check_type_annotations_in_node(
    node: Node,
    source: &str,
    symbols: &SymbolTable,
    project: &ProjectIndex,
    errors: &mut Vec<StructuralError>,
) {
    let bytes = source.as_bytes();

    // Check typed parameters, typed variables, return types
    let type_node = match node.kind() {
        "typed_parameter" | "typed_default_parameter" | "variable_statement"
        | "const_statement" => node.child_by_field_name("type"),
        "function_definition" | "constructor_definition" => {
            node.child_by_field_name("return_type")
        }
        _ => None,
    };

    if let Some(type_node) = type_node
        && type_node.kind() != "inferred_type"
        && let Ok(type_name) = type_node.utf8_text(bytes)
    {
        // Strip Array[...] wrapper
        let base_type = if let Some(inner) = type_name.strip_prefix("Array[") {
            inner.strip_suffix(']').unwrap_or(type_name)
        } else {
            type_name
        };

        if !base_type.is_empty() && !is_known_type(base_type, symbols, project) {
            let pos = type_node.start_position();
            errors.push(StructuralError {
                line: pos.row as u32 + 1,
                column: pos.column as u32 + 1,
                message: format!("unknown type `{base_type}` in type annotation"),
            });
        }
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            check_type_annotations_in_node(cursor.node(), source, symbols, project, errors);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

/// Check if a type name is known (builtin, ClassDB, user class, enum, or inner class).
fn is_known_type(name: &str, symbols: &SymbolTable, project: &ProjectIndex) -> bool {
    // GDScript built-in types
    let builtins = [
        "void", "bool", "int", "float", "String", "Variant",
        "Vector2", "Vector2i", "Vector3", "Vector3i", "Vector4", "Vector4i",
        "Rect2", "Rect2i", "Transform2D", "Transform3D",
        "Plane", "Quaternion", "AABB", "Basis", "Projection", "Color",
        "NodePath", "StringName", "RID", "Callable", "Signal",
        "Dictionary", "Array", "PackedByteArray", "PackedInt32Array",
        "PackedInt64Array", "PackedFloat32Array", "PackedFloat64Array",
        "PackedStringArray", "PackedVector2Array", "PackedVector3Array",
        "PackedColorArray", "PackedVector4Array", "Object",
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
    if symbols
        .enums
        .iter()
        .any(|e| e.name == name)
    {
        return true;
    }

    // Inner classes
    if symbols.inner_classes.iter().any(|(n, _)| n == name) {
        return true;
    }

    false
}

// ---------------------------------------------------------------------------
// Tree-sitter error reporting (existing)
// ---------------------------------------------------------------------------

fn report_errors(cursor: &mut tree_sitter::TreeCursor, source: &str, file: &Path) {
    use owo_colors::OwoColorize;
    loop {
        let node = cursor.node();
        if node.is_error() || node.is_missing() {
            let start = node.start_position();
            let line = source.lines().nth(start.row).unwrap_or("");
            ceprintln!(
                "{}:{}:{} {} parse error",
                file.display(),
                start.row + 1,
                start.column + 1,
                "error:".red().bold(),
            );
            ceprintln!("  {line}");
        }
        if cursor.goto_first_child() {
            report_errors(cursor, source, file);
            cursor.goto_parent();
        }
        if !cursor.goto_next_sibling() {
            break;
        }
    }
}

fn report_structural(errors: &[StructuralError], source: &str, file: &Path) {
    use owo_colors::OwoColorize;
    for err in errors {
        let line = source.lines().nth(err.line as usize - 1).unwrap_or("");
        ceprintln!(
            "{}:{}:{} {} {}",
            file.display(),
            err.line,
            err.column,
            "error:".red().bold(),
            err.message,
        );
        ceprintln!("  {line}");
    }
}

// ---------------------------------------------------------------------------
// Duplicate declaration checks (compile errors in Godot)
// ---------------------------------------------------------------------------

fn check_duplicates(
    tree: &tree_sitter::Tree,
    source: &str,
) -> Vec<crate::lint::rules::LintDiagnostic> {
    let lint_config = crate::core::config::LintConfig::default();
    let rules: [&dyn LintRule; 3] = [&DuplicateFunction, &DuplicateSignal, &DuplicateVariable];
    let mut diags = Vec::new();
    for rule in rules {
        diags.extend(rule.check(tree, source, &lint_config));
    }
    diags
}

// ---------------------------------------------------------------------------
// Override signature mismatch checks (compile errors in Godot)
// ---------------------------------------------------------------------------

fn check_overrides(
    tree: &tree_sitter::Tree,
    source: &str,
    symbols: &SymbolTable,
    project: &ProjectIndex,
) -> Vec<crate::lint::rules::LintDiagnostic> {
    let lint_config = crate::core::config::LintConfig::default();
    OverrideSignatureMismatch.check_with_project(tree, source, &lint_config, symbols, project)
}

// ---------------------------------------------------------------------------
// Promoted lint rules — errors that Godot's compiler also rejects
// ---------------------------------------------------------------------------

fn check_promoted_rules(
    tree: &tree_sitter::Tree,
    source: &str,
    symbols: &SymbolTable,
) -> Vec<crate::lint::rules::LintDiagnostic> {
    let lint_config = crate::core::config::LintConfig::default();
    let mut diags = Vec::new();

    // duplicate-key: duplicate dictionary keys are a compile error
    diags.extend(DuplicateKey.check(tree, source, &lint_config));

    // onready-with-export: @onready + @export is a compile error
    diags.extend(OnreadyWithExport.check_with_symbols(tree, source, &lint_config, symbols));

    // get-node-default-without-onready: $Path default without @onready is a compile error
    diags.extend(GetNodeDefaultWithoutOnready.check_with_symbols(
        tree,
        source,
        &lint_config,
        symbols,
    ));

    // native-method-override: overriding a native non-virtual method is a compile error
    diags.extend(NativeMethodOverride.check_with_symbols(tree, source, &lint_config, symbols));

    diags
}

fn report_duplicates(diags: &[crate::lint::rules::LintDiagnostic], source: &str, file: &Path) {
    for diag in diags {
        let line = source.lines().nth(diag.line).unwrap_or("");
        ceprintln!(
            "{}:{}:{} {} {}",
            file.display(),
            diag.line + 1,
            diag.column + 1,
            "error:".red().bold(),
            diag.message,
        );
        ceprintln!("  {line}");
    }
}

// ---------------------------------------------------------------------------
// Scene (.tscn) validation
// ---------------------------------------------------------------------------

fn validate_scene(
    data: &scene::SceneData,
    project_root: &Path,
    file: &Path,
    cwd: &Path,
) -> Vec<ParseError> {
    let rel = crate::core::fs::relative_slash(file, cwd);
    let mut errors = Vec::new();

    // Check ext_resource paths exist on disk
    for ext in &data.ext_resources {
        if !ext.path.is_empty()
            && let Some(resolved) = scene::resolve_res_path(&ext.path, project_root)
            && !resolved.exists()
        {
            errors.push(ParseError {
                file: rel.clone(),
                line: 0,
                column: 0,
                message: format!("broken resource path: {} (file not found)", ext.path),
            });
        }
    }

    // Check for orphaned ext_resources (declared but never referenced)
    for ext in &data.ext_resources {
        if !scene::is_ext_resource_referenced(&ext.id, data) {
            errors.push(ParseError {
                file: rel.clone(),
                line: 0,
                column: 0,
                message: format!(
                    "orphaned ext_resource: {} ({}) is declared but never referenced",
                    ext.id, ext.path
                ),
            });
        }
    }

    // Check script references — script ExtResource must point to an existing .gd file
    for node in &data.nodes {
        if let Some(ref script_val) = node.script
            && let Some(ext_id) = extract_ext_resource_id(script_val)
            && let Some(ext) = data.ext_resources.iter().find(|e| e.id == ext_id)
            && let Some(resolved) = scene::resolve_res_path(&ext.path, project_root)
            && !resolved.exists()
        {
            errors.push(ParseError {
                file: rel.clone(),
                line: 0,
                column: 0,
                message: format!(
                    "missing script: node \"{}\" references {} which doesn't exist",
                    node.name, ext.path
                ),
            });
        }
    }

    errors
}

/// Extract the id from `ExtResource("some_id")`.
fn extract_ext_resource_id(value: &str) -> Option<&str> {
    let trimmed = value.trim();
    let inner = trimmed.strip_prefix("ExtResource(")?.strip_suffix(')')?;
    let inner = inner.trim().trim_matches('"');
    Some(inner)
}

fn report_scene_errors(errors: &[ParseError], _file: &Path) {
    use owo_colors::OwoColorize;
    for err in errors {
        ceprintln!(
            "{} {} {}",
            format!("{}:", err.file).dimmed(),
            "warning:".yellow().bold(),
            err.message,
        );
    }
}

fn collect_errors(
    cursor: &mut tree_sitter::TreeCursor,
    file: &Path,
    base: &Path,
    out: &mut Vec<ParseError>,
) {
    loop {
        let node = cursor.node();
        if node.is_error() || node.is_missing() {
            let start = node.start_position();
            let rel = crate::core::fs::relative_slash(file, base);
            out.push(ParseError {
                file: rel,
                line: start.row as u32 + 1,
                column: start.column as u32 + 1,
                message: "parse error".to_string(),
            });
        }
        if cursor.goto_first_child() {
            collect_errors(cursor, file, base, out);
            cursor.goto_parent();
        }
        if !cursor.goto_next_sibling() {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::core::parser;

    use super::*;

    fn structural_errors(source: &str) -> Vec<StructuralError> {
        let tree = parser::parse(source).unwrap();
        let symbols = symbol_table::build(&tree, source);
        validate_structure(&tree.root_node(), source, &symbols)
    }

    // -- Top-level statement checks --

    #[test]
    fn valid_top_level_no_errors() {
        let source = "extends Node\n\nvar x := 1\nconst Y = 2\n\nfunc _ready():\n\tpass\n";
        assert!(structural_errors(source).is_empty());
    }

    #[test]
    fn top_level_for_loop_is_error() {
        let source = "extends Node\n\nfor i in range(10):\n\tprint(i)\n";
        let errs = structural_errors(source);
        assert_eq!(errs.len(), 1);
        assert!(errs[0].message.contains("top level"));
    }

    #[test]
    fn top_level_expression_is_error() {
        let source = "extends Node\n\nprint(\"hello\")\n";
        let errs = structural_errors(source);
        assert_eq!(errs.len(), 1);
        assert!(errs[0].message.contains("top level"));
    }

    #[test]
    fn top_level_if_is_error() {
        let source = "extends Node\n\nif true:\n\tpass\n";
        let errs = structural_errors(source);
        assert_eq!(errs.len(), 1);
        assert!(errs[0].message.contains("top level"));
    }

    #[test]
    fn top_level_return_is_error() {
        let source = "return 42\n";
        let errs = structural_errors(source);
        assert_eq!(errs.len(), 1);
        assert!(errs[0].message.contains("top level"));
    }

    // -- Indentation consistency checks --

    #[test]
    fn consistent_indentation_no_errors() {
        let source = "func f(x: int) -> int:\n\tif x > 0:\n\t\treturn x\n\telse:\n\t\treturn -x\n";
        assert!(structural_errors(source).is_empty());
    }

    #[test]
    fn orphaned_block_after_return_detected() {
        // Simulates removing else: but leaving body indented too deep
        let source = "func f(m: int) -> int:\n\tmatch m:\n\t\t0:\n\t\t\tif m == 1:\n\t\t\t\treturn 1\n\t\t\t# comment\n\t\t\t\tvar q := 2\n\t\t\t\treturn q\n\t\t_:\n\t\t\treturn 0\n";
        let errs = structural_errors(source);
        assert!(!errs.is_empty(), "should detect orphaned indented block");
        assert!(errs[0].message.contains("indentation"));
    }

    #[test]
    fn dedented_body_code_at_top_level_detected() {
        // Function body code accidentally at column 0
        let source = "extends Node\n\nvar items: Array = []\n\nfor i in range(10):\n\titems.append(i)\n\nfunc _ready():\n\tpass\n";
        let errs = structural_errors(source);
        assert!(!errs.is_empty());
    }

    #[test]
    fn multiline_expression_not_false_positive() {
        // Continuation lines inside a single statement node are fine
        let source = "func f() -> Quaternion:\n\tvar result := Quaternion(\n\t\t1.0,\n\t\t2.0,\n\t\t3.0,\n\t\t4.0\n\t).normalized()\n\treturn result\n";
        assert!(structural_errors(source).is_empty());
    }

    #[test]
    fn multiline_function_call_not_false_positive() {
        let source = "func f() -> void:\n\tsome_function(\n\t\targ1,\n\t\targ2,\n\t\targ3\n\t)\n\tprint(\"done\")\n";
        assert!(structural_errors(source).is_empty());
    }

    #[test]
    fn multiline_array_not_false_positive() {
        let source =
            "func f() -> Array:\n\tvar arr := [\n\t\t1,\n\t\t2,\n\t\t3,\n\t]\n\treturn arr\n";
        assert!(structural_errors(source).is_empty());
    }

    #[test]
    fn multiline_dict_not_false_positive() {
        let source = "func f() -> Dictionary:\n\tvar d := {\n\t\t\"a\": 1,\n\t\t\"b\": 2,\n\t}\n\treturn d\n";
        assert!(structural_errors(source).is_empty());
    }

    // -- Class constant validation checks --

    #[test]
    fn valid_class_constant_no_error() {
        let source = "func f():\n\tvar mode := Environment.TONE_MAPPER_LINEAR\n";
        let errs = structural_errors(source);
        assert!(
            errs.is_empty(),
            "valid constant should not produce errors, got: {:?}",
            errs.iter().map(|e| &e.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn invalid_class_constant_detected() {
        let source = "func f():\n\tvar mode := Environment.TONE_MAP_ACES\n";
        let errs = structural_errors(source);
        assert_eq!(errs.len(), 1);
        assert!(errs[0].message.contains("unknown constant"));
    }

    #[test]
    fn user_class_not_validated() {
        // Only Godot built-in classes should be validated
        let source = "func f():\n\tvar x := MyClass.SOME_CONST\n";
        let errs = structural_errors(source);
        assert!(errs.is_empty());
    }

    // -- Variant inference checks --

    #[test]
    fn variant_infer_from_subscript() {
        let source = "var dict := {}\nfunc f():\n\tvar x := dict[\"key\"]\n";
        let errs = structural_errors(source);
        assert_eq!(errs.len(), 1);
        assert!(errs[0].message.contains("Variant"));
    }

    #[test]
    fn variant_infer_from_dict_get() {
        let source = "var dict := {}\nfunc f():\n\tvar x := dict.get(\"key\")\n";
        let errs = structural_errors(source);
        assert_eq!(errs.len(), 1);
        assert!(errs[0].message.contains("Variant"));
    }

    #[test]
    fn no_variant_warning_with_explicit_type() {
        let source = "var dict := {}\nfunc f():\n\tvar x: String = dict[\"key\"]\n";
        assert!(structural_errors(source).is_empty());
    }

    #[test]
    fn no_variant_warning_simple_infer() {
        let source = "func f():\n\tvar x := 42\n";
        assert!(structural_errors(source).is_empty());
    }

    #[test]
    fn variant_infer_from_max() {
        let source = "func f():\n\tvar x := max(1, 2)\n";
        let errs = structural_errors(source);
        assert_eq!(errs.len(), 1);
        assert!(errs[0].message.contains("Variant"));
    }

    #[test]
    fn variant_infer_from_min() {
        let source = "func f():\n\tvar x := min(1, 2)\n";
        let errs = structural_errors(source);
        assert_eq!(errs.len(), 1);
        assert!(errs[0].message.contains("Variant"));
    }

    #[test]
    fn variant_infer_from_clamp() {
        let source = "func f():\n\tvar x := clamp(5, 1, 10)\n";
        let errs = structural_errors(source);
        assert_eq!(errs.len(), 1);
        assert!(errs[0].message.contains("Variant"));
    }

    #[test]
    fn no_variant_from_maxi() {
        let source = "func f():\n\tvar x := maxi(1, 2)\n";
        assert!(structural_errors(source).is_empty());
    }

    #[test]
    fn no_variant_from_maxf() {
        let source = "func f():\n\tvar x := maxf(1.0, 2.0)\n";
        assert!(structural_errors(source).is_empty());
    }

    #[test]
    fn enum_type_as_cast_not_flagged() {
        let source = "func f(index: int):\n\tvar msaa := index as Viewport.MSAA\n";
        let errs = structural_errors(source);
        assert!(
            errs.iter().all(|e| !e.message.contains("unknown constant")),
            "enum type name used for casting should not be flagged: {:?}",
            errs.iter().map(|e| &e.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn region_markers_valid_at_top_level() {
        let source =
            "extends Node\n\n#region Signals\nsignal foo\n#endregion\n\nfunc _ready():\n\tpass\n";
        assert!(structural_errors(source).is_empty());
    }

    // -- `in` / `not in` variant inference --

    #[test]
    fn variant_infer_from_in_operator() {
        let source = "\
var ACTIONS := [\"move_left\", \"move_right\"]
func f(action: String):
\tvar is_move := action in ACTIONS
";
        let errs = structural_errors(source);
        assert_eq!(errs.len(), 1);
        assert!(errs[0].message.contains("Variant"));
        assert!(errs[0].message.contains("is_move"));
    }

    #[test]
    fn variant_infer_from_not_in() {
        let source = "\
var ACTIONS := [\"move_left\", \"move_right\"]
func f(action: String):
\tvar missing := action not in ACTIONS
";
        let errs = structural_errors(source);
        assert_eq!(errs.len(), 1);
        assert!(errs[0].message.contains("Variant"));
        assert!(errs[0].message.contains("missing"));
    }

    #[test]
    fn no_variant_from_in_with_explicit_type() {
        let source = "\
var ACTIONS := [\"move_left\", \"move_right\"]
func f(action: String):
\tvar is_move: bool = action in ACTIONS
";
        assert!(structural_errors(source).is_empty());
    }

    // -- Unresolvable property access variant inference --

    #[test]
    fn variant_infer_from_base_class_property() {
        let source = "\
func handle(event: InputEvent):
\tvar keycode := event.physical_keycode
";
        let errs = structural_errors(source);
        assert_eq!(errs.len(), 1);
        assert!(errs[0].message.contains("Variant"));
        assert!(errs[0].message.contains("keycode"));
    }

    #[test]
    fn no_variant_self_property() {
        let source = "\
var speed := 10.0
func f():
\tvar s := self.speed
";
        assert!(structural_errors(source).is_empty());
    }

    #[test]
    fn no_variant_explicit_type_on_property() {
        let source = "\
func handle(event: InputEvent):
\tvar keycode: int = event.physical_keycode
";
        assert!(structural_errors(source).is_empty());
    }

    #[test]
    fn no_variant_from_known_type_property() {
        // Vector2.x is a known float — should not be flagged
        let source = "\
func f(pos: Vector2):
\tvar x := pos.x
";
        assert!(structural_errors(source).is_empty());
    }

    #[test]
    fn no_variant_from_method_call() {
        // Method calls should not trigger the property access check
        let source = "\
func f(node: Node):
\tvar name := node.get_name()
";
        assert!(structural_errors(source).is_empty());
    }

    #[test]
    fn no_variant_from_class_constant() {
        let source = "\
func f():
\tvar zero := Vector2.ZERO
";
        assert!(structural_errors(source).is_empty());
    }

    // -- load().instantiate() variant inference --

    #[test]
    fn variant_infer_from_load_instantiate() {
        let source = "\
func f():
\tvar popup := load(\"res://popup.tscn\").instantiate()
";
        let errs = structural_errors(source);
        assert_eq!(errs.len(), 1);
        assert!(errs[0].message.contains("Variant"));
        assert!(errs[0].message.contains("popup"));
    }

    #[test]
    fn no_variant_from_preload_instantiate() {
        let source = "\
func f():
\tvar popup := preload(\"res://popup.tscn\").instantiate()
";
        assert!(structural_errors(source).is_empty());
    }

    // -- ClassDB Variant-return method inference --

    #[test]
    fn variant_infer_from_classdb_variant_method() {
        let source = "\
func f(node: Node):
\tvar meta := node.get_meta(\"key\")
";
        let errs = structural_errors(source);
        assert_eq!(errs.len(), 1);
        assert!(errs[0].message.contains("Variant"));
        assert!(errs[0].message.contains("meta"));
    }

    #[test]
    fn no_variant_from_classdb_concrete_method() {
        let source = "\
func f(node: Node):
\tvar child := node.get_child(0)
";
        assert!(structural_errors(source).is_empty());
    }

    #[test]
    fn no_variant_from_untyped_receiver_method() {
        let source = "\
func f(node):
\tvar meta := node.get_meta(\"key\")
";
        assert!(structural_errors(source).is_empty());
    }

    // -- Type narrowing after `is` checks --

    #[test]
    fn no_variant_after_direct_is_guard() {
        let source = "\
func f(event: InputEvent):
\tif event is InputEventKey:
\t\tvar k := event.keycode
";
        assert!(structural_errors(source).is_empty());
    }

    #[test]
    fn no_variant_after_early_exit_is_guard() {
        let source = "\
func f(event: InputEvent):
\tif not event is InputEventKey:
\t\treturn
\tvar k := event.keycode
";
        assert!(structural_errors(source).is_empty());
    }

    #[test]
    fn no_variant_after_early_exit_continue() {
        let source = "\
func f(events: Array):
\tfor event in events:
\t\tif not event is InputEventKey:
\t\t\tcontinue
\t\tvar k := event.keycode
";
        assert!(structural_errors(source).is_empty());
    }

    #[test]
    fn variant_still_flagged_without_is_guard() {
        let source = "\
func f(event: InputEvent):
\tvar k := event.keycode
";
        let errs = structural_errors(source);
        assert_eq!(errs.len(), 1);
        assert!(errs[0].message.contains("Variant"));
    }

    // -- := initializer type inference --

    #[test]
    fn infer_constructor_new() {
        let source = "\
func f():
\tvar target := Node3D.new()
\tvar d := target.position
";
        assert!(structural_errors(source).is_empty());
    }

    #[test]
    fn infer_constructor_call() {
        let source = "\
func f():
\tvar v := Vector2(1, 2)
\tvar x := v.x
";
        assert!(structural_errors(source).is_empty());
    }

    #[test]
    fn infer_same_file_function_return() {
        let source = "\
func _find_node() -> Node3D:
\treturn null
func f():
\tvar target := _find_node()
\tvar d := target.position
";
        assert!(structural_errors(source).is_empty());
    }

    #[test]
    fn infer_cast_as_type() {
        let source = "\
func f(obj):
\tvar node := obj as Node3D
\tvar d := node.position
";
        assert!(structural_errors(source).is_empty());
    }

    // -- Scene validation --

    #[test]
    fn extract_ext_resource_id_basic() {
        assert_eq!(
            super::extract_ext_resource_id(r#"ExtResource("1_abc")"#),
            Some("1_abc")
        );
    }

    #[test]
    fn extract_ext_resource_id_none() {
        assert_eq!(super::extract_ext_resource_id("not_a_reference"), None);
    }

    #[test]
    fn validate_scene_orphaned_ext_resource() {
        let source = r#"[gd_scene format=3]

[ext_resource type="Texture2D" path="res://icon.png" id="unused_1"]

[node name="Root" type="Node2D"]
"#;
        let data = crate::core::scene::parse_scene(source).unwrap();
        let root = std::path::Path::new("/nonexistent");
        let cwd = std::path::Path::new("/cwd");
        let file = std::path::Path::new("/cwd/test.tscn");
        let errors = super::validate_scene(&data, root, file, cwd);
        assert!(
            errors.iter().any(|e| e.message.contains("orphaned")),
            "should detect orphaned ext_resource"
        );
    }

    // ====================================================================
    // Batch 2: Declaration constraint checks
    // ====================================================================

    // -- G4: _init cannot have return type --

    #[test]
    fn init_with_return_type() {
        let source = "func _init() -> void:\n\tpass\n";
        let errs = structural_errors(source);
        assert!(errs.iter().any(|e| e.message.contains("_init")));
    }

    #[test]
    fn init_without_return_type_ok() {
        let source = "func _init():\n\tpass\n";
        assert!(structural_errors(source).is_empty());
    }

    // -- G3: Mandatory parameter after optional --

    #[test]
    fn mandatory_after_optional() {
        let source = "func f(a: int = 1, b: int):\n\tpass\n";
        let errs = structural_errors(source);
        assert!(errs.iter().any(|e| e.message.contains("required parameter")));
    }

    #[test]
    fn all_optional_ok() {
        let source = "func f(a: int = 1, b: int = 2):\n\tpass\n";
        assert!(structural_errors(source).is_empty());
    }

    #[test]
    fn all_mandatory_ok() {
        let source = "func f(a: int, b: int):\n\tpass\n";
        assert!(structural_errors(source).is_empty());
    }

    // -- G2: Signal params cannot have defaults --

    #[test]
    fn signal_with_default_param() {
        let source = "signal my_signal(a: int = 5)\n";
        let errs = structural_errors(source);
        assert!(errs.iter().any(|e| e.message.contains("signal parameter")));
    }

    #[test]
    fn signal_without_default_ok() {
        let source = "signal my_signal(a: int)\n";
        assert!(structural_errors(source).is_empty());
    }

    // -- G6: Duplicate class_name / extends --

    #[test]
    fn duplicate_class_name() {
        let source = "class_name Foo\nclass_name Bar\n";
        let errs = structural_errors(source);
        assert!(errs.iter().any(|e| e.message.contains("duplicate `class_name`")));
    }

    #[test]
    fn duplicate_extends() {
        let source = "extends Node\nextends Node2D\n";
        let errs = structural_errors(source);
        assert!(errs.iter().any(|e| e.message.contains("duplicate `extends`")));
    }

    #[test]
    fn single_class_name_ok() {
        let source = "class_name Foo\nextends Node\n";
        assert!(structural_errors(source).is_empty());
    }

    // -- G7: Duplicate parameter names --

    #[test]
    fn duplicate_param_name() {
        let source = "func f(a: int, a: int):\n\tpass\n";
        let errs = structural_errors(source);
        assert!(errs.iter().any(|e| e.message.contains("duplicate parameter")));
    }

    #[test]
    fn unique_params_ok() {
        let source = "func f(a: int, b: int):\n\tpass\n";
        assert!(structural_errors(source).is_empty());
    }

    // -- G1: yield keyword --

    #[test]
    fn yield_keyword_detected() {
        let source = "func f():\n\tyield(get_tree(), \"idle_frame\")\n";
        let errs = structural_errors(source);
        assert!(errs.iter().any(|e| e.message.contains("yield")));
    }

    // -- H7: _static_init cannot have params --

    #[test]
    fn static_init_with_params() {
        let source = "static func _static_init(x: int):\n\tpass\n";
        let errs = structural_errors(source);
        assert!(errs.iter().any(|e| e.message.contains("_static_init")));
    }

    #[test]
    fn static_init_no_params_ok() {
        let source = "static func _static_init():\n\tpass\n";
        assert!(structural_errors(source).is_empty());
    }

    // -- E8: Duplicate @tool --

    #[test]
    fn duplicate_tool_annotation() {
        let source = "@tool\n@tool\nextends Node\n";
        let errs = structural_errors(source);
        assert!(errs.iter().any(|e| e.message.contains("duplicate `@tool`")));
    }

    #[test]
    fn single_tool_ok() {
        let source = "@tool\nextends Node\n";
        assert!(structural_errors(source).is_empty());
    }

    // ====================================================================
    // Batch 3: Semantic checks
    // ====================================================================

    // -- C1: Static context violations --

    #[test]
    fn static_func_uses_self() {
        let source = "\
extends Node
static func foo():
\tprint(self)
";
        let errs = structural_errors(source);
        assert!(errs.iter().any(|e| e.message.contains("self") && e.message.contains("static")));
    }

    #[test]
    fn static_func_accesses_instance_var() {
        let source = "\
extends Node
var health := 100
static func foo():
\tprint(health)
";
        let errs = structural_errors(source);
        assert!(errs.iter().any(|e| e.message.contains("health") && e.message.contains("static")));
    }

    #[test]
    fn static_func_calls_instance_method() {
        let source = "\
extends Node
func bar():
\tpass
static func foo():
\tbar()
";
        let errs = structural_errors(source);
        assert!(errs.iter().any(|e| e.message.contains("bar") && e.message.contains("static")));
    }

    #[test]
    fn non_static_func_uses_self_ok() {
        let source = "\
extends Node
func foo():
\tprint(self)
";
        assert!(structural_errors(source).is_empty());
    }

    // -- C2: Assign to constant --

    #[test]
    fn assign_to_constant() {
        let source = "\
const MAX := 100
func f():
\tMAX = 200
";
        let errs = structural_errors(source);
        assert!(errs.iter().any(|e| e.message.contains("constant") && e.message.contains("MAX")));
    }

    #[test]
    fn assign_to_enum_member() {
        let source = "\
enum State { IDLE, RUNNING }
func f():
\tIDLE = 5
";
        let errs = structural_errors(source);
        assert!(errs.iter().any(|e| e.message.contains("enum value") && e.message.contains("IDLE")));
    }

    #[test]
    fn assign_to_var_ok() {
        let source = "\
var x := 100
func f():
\tx = 200
";
        assert!(structural_errors(source).is_empty());
    }

    // -- C3: Void function returns value --

    #[test]
    fn void_func_returns_value() {
        let source = "func f() -> void:\n\treturn 42\n";
        let errs = structural_errors(source);
        assert!(errs.iter().any(|e| e.message.contains("void") && e.message.contains("return")));
    }

    #[test]
    fn void_func_bare_return_ok() {
        let source = "func f() -> void:\n\treturn\n";
        assert!(structural_errors(source).is_empty());
    }

    #[test]
    fn typed_func_returns_value_ok() {
        let source = "func f() -> int:\n\treturn 42\n";
        assert!(structural_errors(source).is_empty());
    }

    // -- H14: get_node in static --

    #[test]
    fn get_node_in_static_func() {
        let source = "static func f():\n\tvar x = $Sprite2D\n";
        let errs = structural_errors(source);
        assert!(errs.iter().any(|e| e.message.contains("get_node") && e.message.contains("static")));
    }

    #[test]
    fn get_node_in_non_static_ok() {
        let source = "func f():\n\tvar x = $Sprite2D\n";
        assert!(structural_errors(source).is_empty());
    }

    // -- E1: @export without type or initializer --

    #[test]
    fn export_without_type_or_default() {
        let source = "@export var x\n";
        let errs = structural_errors(source);
        assert!(errs.iter().any(|e| e.message.contains("export") && e.message.contains("no type")));
    }

    #[test]
    fn export_with_type_ok() {
        let source = "@export var x: int\n";
        assert!(structural_errors(source).is_empty());
    }

    #[test]
    fn export_with_default_ok() {
        let source = "@export var x = 10\n";
        assert!(structural_errors(source).is_empty());
    }

    // -- E3: @export on static --

    #[test]
    fn export_on_static() {
        let source = "@export\nstatic var x: int = 0\n";
        let errs = structural_errors(source);
        assert!(errs.iter().any(|e| e.message.contains("export") && e.message.contains("static")));
    }

    // -- E4: Duplicate @export --

    #[test]
    fn duplicate_export() {
        let source = "@export\n@export\nvar x: int = 0\n";
        let errs = structural_errors(source);
        assert!(errs.iter().any(|e| e.message.contains("duplicate") && e.message.contains("export")));
    }

    // -- H17: Object() constructor --

    #[test]
    fn object_direct_constructor() {
        let source = "func f():\n\tvar o = Object()\n";
        let errs = structural_errors(source);
        assert!(errs.iter().any(|e| e.message.contains("Object()") && e.message.contains("Object.new()")));
    }

    #[test]
    fn object_new_ok() {
        let source = "func f():\n\tvar o = Object.new()\n";
        assert!(structural_errors(source).is_empty());
    }

    // ====================================================================
    // Batch 4: Preload & misc checks
    // ====================================================================

    // -- F2: preload() argument not a constant string --

    #[test]
    fn preload_non_string_arg() {
        let source = "func f():\n\tvar path = \"res://foo.gd\"\n\tvar x = preload(path)\n";
        let errs = structural_errors(source);
        assert!(errs.iter().any(|e| e.message.contains("preload") && e.message.contains("constant string")));
    }

    #[test]
    fn preload_string_arg_ok() {
        let source = "func f():\n\tvar x = preload(\"res://foo.gd\")\n";
        assert!(structural_errors(source).is_empty());
    }

    // -- H15: range() too many arguments --

    #[test]
    fn range_too_many_args() {
        let source = "func f():\n\tfor i in range(1, 2, 3, 4):\n\t\tpass\n";
        let errs = structural_errors(source);
        assert!(errs.iter().any(|e| e.message.contains("range") && e.message.contains("at most 3")));
    }

    #[test]
    fn range_three_args_ok() {
        let source = "func f():\n\tfor i in range(0, 10, 2):\n\t\tpass\n";
        assert!(structural_errors(source).is_empty());
    }

    // ====================================================================
    // Batch 5: ClassDB checks
    // ====================================================================

    fn classdb_errors(source: &str) -> Vec<StructuralError> {
        let tree = parser::parse(source).unwrap();
        let symbols = symbol_table::build(&tree, source);
        let project = ProjectIndex::build(std::path::Path::new("/nonexistent"));
        check_classdb_errors(&tree.root_node(), source, &symbols, &project)
    }

    // -- H5: class_name shadows native class --

    #[test]
    fn class_name_shadows_native() {
        let source = "class_name Node\n";
        let errs = classdb_errors(source);
        assert!(errs.iter().any(|e| e.message.contains("shadows")));
    }

    #[test]
    fn class_name_custom_ok() {
        let source = "class_name MyPlayer\n";
        assert!(classdb_errors(source).is_empty());
    }

    // -- G5: Enum shadows builtin type --

    #[test]
    fn enum_shadows_builtin() {
        let source = "enum int { A, B, C }\n";
        let errs = classdb_errors(source);
        assert!(errs.iter().any(|e| e.message.contains("shadows") && e.message.contains("int")));
    }

    #[test]
    fn enum_custom_name_ok() {
        let source = "enum MyState { A, B, C }\n";
        assert!(classdb_errors(source).is_empty());
    }

    // -- A4: Unknown type in annotation --

    #[test]
    fn unknown_type_annotation() {
        let source = "var x: NonExistentType\n";
        let errs = classdb_errors(source);
        assert!(errs.iter().any(|e| e.message.contains("unknown type")));
    }

    #[test]
    fn known_type_annotation_ok() {
        let source = "var x: int\n";
        assert!(classdb_errors(source).is_empty());
    }

    #[test]
    fn classdb_type_annotation_ok() {
        let source = "var x: Node2D\n";
        assert!(classdb_errors(source).is_empty());
    }

    #[test]
    fn same_file_enum_type_ok() {
        let source = "enum State { A, B }\nvar x: State\n";
        assert!(classdb_errors(source).is_empty());
    }
}
