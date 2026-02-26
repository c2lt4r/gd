use std::env;
use std::path::Path;

use clap::Args;
use miette::Result;
use owo_colors::OwoColorize;
use serde::Serialize;
use tree_sitter::Node;

use crate::core::symbol_table::SymbolTable;
use crate::core::workspace_index::ProjectIndex;
use crate::core::{
    config::Config, config::find_project_root, fs::collect_gdscript_files,
    fs::collect_resource_files, parser, resource_parser, scene, symbol_table, type_inference,
};
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

    let roots: Vec<std::path::PathBuf> = if args.paths.is_empty() {
        vec![cwd.clone()]
    } else {
        args.paths.iter().map(std::path::PathBuf::from).collect()
    };

    // Build project-wide index for cross-file override checking.
    // When explicit paths are given, use the first path's project root so
    // autoloads, class_names, etc. are resolved from the target project.
    let index_root = if args.paths.is_empty() {
        ignore_base.clone()
    } else {
        let first = &roots[0];
        let start = if first.is_file() {
            first.parent().unwrap_or(first).to_path_buf()
        } else {
            first.clone()
        };
        find_project_root(&start).unwrap_or(start)
    };
    let project_index = ProjectIndex::build(&index_root);

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
                    let structural =
                        validate_structure(&root_node, &source, &symbols, Some(&ignore_base));
                    let classdb =
                        check_classdb_errors(&root_node, &source, &symbols, &project_index);
                    let duplicates = check_duplicates(&tree, &source);
                    let promoted = check_promoted_rules(&tree, &source, &symbols);
                    let overrides = check_overrides(&tree, &source, &symbols, &project_index);

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

#[derive(Debug)]
pub struct StructuralError {
    pub line: u32,
    pub column: u32,
    pub message: String,
}

/// Run structural checks that go beyond tree-sitter error nodes.
fn validate_structure(
    root: &Node,
    source: &str,
    symbols: &SymbolTable,
    project_root: Option<&Path>,
) -> Vec<StructuralError> {
    let mut errors = Vec::new();
    check_top_level_statements(root, &mut errors);
    check_indentation_consistency(root, &mut errors);
    check_class_constants(root, source, &mut errors);
    check_variant_inference(root, source, &mut errors);
    check_declaration_constraints(root, source, symbols, &mut errors);
    check_semantic_errors(root, source, symbols, &mut errors);
    check_preload_and_misc(root, source, project_root, &mut errors);
    check_advanced_semantic(root, source, symbols, &mut errors);
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
    check_onready_non_node(symbols, errors);
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
        let is_static = symbols.functions.iter().any(|f| {
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
        } else if name == "self" || name == "super" {
            let pos = node.start_position();
            errors.push(StructuralError {
                line: pos.row as u32 + 1,
                column: pos.column as u32 + 1,
                message: format!("cannot use `{name}` in static function `{func_name}()`",),
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

fn check_returns_in_body(node: &Node, func_name: &str, errors: &mut Vec<StructuralError>) {
    if node.kind() == "return_statement" {
        // Check if there's a value after `return`
        if node.named_child_count() > 0 {
            let pos = node.start_position();
            errors.push(StructuralError {
                line: pos.row as u32 + 1,
                column: pos.column as u32 + 1,
                message: format!("void function `{func_name}()` cannot return a value",),
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
fn check_export_constraints(symbols: &SymbolTable, errors: &mut Vec<StructuralError>) {
    for var in &symbols.variables {
        let export_count = var
            .annotations
            .iter()
            .filter(|a| a.as_str() == "export")
            .count();
        let has_export = export_count > 0;

        if has_export {
            // E4: Duplicate @export
            if export_count > 1 {
                errors.push(StructuralError {
                    line: var.line as u32 + 1,
                    column: 1,
                    message: format!("duplicate `@export` annotation on `{}`", var.name,),
                });
            }

            // E3: @export on static
            if var.is_static {
                errors.push(StructuralError {
                    line: var.line as u32 + 1,
                    column: 1,
                    message: format!("`@export` cannot be used on static variable `{}`", var.name,),
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

/// E5: `@onready` on a class that doesn't extend Node.
fn check_onready_non_node(symbols: &SymbolTable, errors: &mut Vec<StructuralError>) {
    let has_onready = symbols
        .variables
        .iter()
        .any(|v| v.annotations.iter().any(|a| a == "onready"));
    if !has_onready {
        return;
    }

    // Check if extends chain reaches Node
    let extends = symbols.extends.as_deref().unwrap_or("RefCounted");
    if extends == "Node" || crate::class_db::inherits(extends, "Node") {
        return;
    }

    // @onready is used but class doesn't extend Node
    for var in &symbols.variables {
        if var.annotations.iter().any(|a| a == "onready") {
            errors.push(StructuralError {
                line: var.line as u32 + 1,
                column: 1,
                message: format!(
                    "`@onready` can only be used in classes that extend `Node` (class extends `{extends}`)",
                ),
            });
        }
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
        && msg_arg.kind() != "string"
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
// Batch 5: ClassDB / signature lookup checks
// ---------------------------------------------------------------------------

/// Batch 5: Check type annotations, class_name shadowing, enum shadowing.
pub fn check_classdb_errors(
    root: &Node,
    source: &str,
    symbols: &SymbolTable,
    project: &ProjectIndex,
) -> Vec<StructuralError> {
    let mut errors = Vec::new();
    check_class_name_shadows_native(symbols, &mut errors);
    check_enum_shadows_builtin(symbols, &mut errors);
    check_type_annotations_resolve(root, source, symbols, project, &mut errors);
    check_use_void_return(root, source, symbols, &mut errors);
    check_instance_method_on_class(root, source, &mut errors);
    check_virtual_override_signature(symbols, &mut errors);
    check_cyclic_inner_class(symbols, &mut errors);
    check_export_invalid_type(symbols, &mut errors);
    check_rpc_args(root, source, &mut errors);
    check_export_node_path_type(root, source, &mut errors);
    check_lambda_super(root, source, &mut errors);
    check_typed_array_wrong_element(root, source, symbols, &mut errors);
    check_callable_direct_call(root, source, symbols, &mut errors);
    check_for_on_non_iterable(root, source, symbols, &mut errors);
    check_arg_count(root, source, symbols, &mut errors);
    check_arg_type_mismatch(root, source, symbols, &mut errors);
    check_assign_type_mismatch(root, source, symbols, &mut errors);
    check_return_type_mismatch(root, source, symbols, &mut errors);
    check_invalid_operators(root, source, symbols, &mut errors);
    check_invalid_cast(root, source, symbols, &mut errors);
    check_type_not_found(root, source, symbols, project, &mut errors);
    check_method_not_found(root, source, symbols, project, &mut errors);
    check_super_method_not_found(root, source, symbols, project, &mut errors);
    check_undefined_identifiers(root, source, symbols, project, &mut errors);
    check_builtin_method_not_found(root, source, symbols, &mut errors);
    check_builtin_property_not_found(root, source, symbols, &mut errors);
    errors
}

/// H5: `class_name` shadows a native Godot class.
fn check_class_name_shadows_native(symbols: &SymbolTable, errors: &mut Vec<StructuralError>) {
    if let Some(ref name) = symbols.class_name
        && crate::class_db::class_exists(name)
    {
        errors.push(StructuralError {
            line: 1,
            column: 1,
            message: format!("`class_name {name}` shadows the native Godot class `{name}`",),
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
    if symbols.enums.iter().any(|e| e.name == name) {
        return true;
    }

    // Inner classes
    if symbols.inner_classes.iter().any(|(n, _)| n == name) {
        return true;
    }

    false
}

// ---------------------------------------------------------------------------
// Batch 6: Advanced semantic checks
// ---------------------------------------------------------------------------

fn check_advanced_semantic(
    root: &Node,
    source: &str,
    symbols: &SymbolTable,
    errors: &mut Vec<StructuralError>,
) {
    check_missing_return(root, source, symbols, errors);
    check_const_expression_required(root, source, errors);
    check_getter_setter_signature(root, source, symbols, errors);
}

/// C4: Not all code paths return a value.
/// Functions with a typed non-void return type must return a value on every path.
fn check_missing_return(
    root: &Node,
    source: &str,
    symbols: &SymbolTable,
    errors: &mut Vec<StructuralError>,
) {
    let bytes = source.as_bytes();
    for func in &symbols.functions {
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
                    line: func.line as u32 + 1,
                    column: 1,
                    message: format!(
                        "not all code paths return a value in `{name}()` (declared -> {})",
                        ret.name,
                    ),
                });
            }
        }
    }
    for (_, inner) in &symbols.inner_classes {
        check_missing_return(root, source, inner, errors);
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
fn is_const_expression(node: &Node, source: &[u8]) -> bool {
    match node.kind() {
        // Literals are always constant
        "integer" | "float" | "string" | "true" | "false" | "null" | "string_name" => true,
        // Negative literal: unary_operator with -
        "unary_operator" => node
            .named_child(0)
            .is_some_and(|c| is_const_expression(&c, source)),
        // Constant identifier references (UPPER_CASE or known builtins)
        "identifier" => {
            let text = node.utf8_text(source).unwrap_or("");
            // Allow references to other constants (UPPER_CASE) or preload/INF/NAN/PI/TAU
            is_upper_snake_case(text)
                || matches!(text, "INF" | "NAN" | "PI" | "TAU" | "INFINITY" | "preload")
        }
        // Array/dictionary literals with all-constant elements
        "array" => {
            let mut cursor = node.walk();
            node.named_children(&mut cursor)
                .all(|c| is_const_expression(&c, source))
        }
        "dictionary" => {
            let mut cursor = node.walk();
            node.named_children(&mut cursor).all(|c| {
                if c.kind() == "pair" {
                    c.named_child(0)
                        .is_some_and(|k| is_const_expression(&k, source))
                        && c.named_child(1)
                            .is_some_and(|v| is_const_expression(&v, source))
                } else {
                    is_const_expression(&c, source)
                }
            })
        }
        // Binary operations on constants
        "binary_operator" => {
            node.named_child(0)
                .is_some_and(|c| is_const_expression(&c, source))
                && node
                    .named_child(1)
                    .is_some_and(|c| is_const_expression(&c, source))
        }
        // Parenthesized expression
        "parenthesized_expression" => node
            .named_child(0)
            .is_some_and(|c| is_const_expression(&c, source)),
        // Class.CONSTANT or enum access
        "attribute" => {
            // Check for a call suffix — if it has one, it's not constant (except preload)
            let mut cursor = node.walk();
            let has_call = node
                .children(&mut cursor)
                .any(|c| c.kind() == "attribute_call");
            !has_call
        }
        // preload() is a constant expression
        "call" => {
            if let Some(func) = node
                .child_by_field_name("function")
                .or_else(|| node.named_child(0))
            {
                func.utf8_text(source).ok() == Some("preload")
            } else {
                false
            }
        }
        // Type constructors with constant args (Vector2(1, 2), Color(1, 0, 0))
        // are constant in Godot — but detecting this precisely is hard.
        // For now, don't flag these.
        _ => false,
    }
}

/// H1: Getter/setter signature mismatch.
/// A property's `set(value)` function must have exactly 1 parameter.
/// A property's `get()` function must have 0 parameters and match the property type.
fn check_getter_setter_signature(
    root: &Node,
    source: &str,
    symbols: &SymbolTable,
    errors: &mut Vec<StructuralError>,
) {
    check_getset_in_node(*root, source.as_bytes(), symbols, errors);
}

fn check_getset_in_node(
    node: Node,
    source: &[u8],
    symbols: &SymbolTable,
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
                && let Some(func) = symbols.functions.iter().find(|f| f.name == func_name)
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
                && let Some(func) = symbols.functions.iter().find(|f| f.name == func_name)
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
            check_getset_in_node(cursor.node(), source, symbols, errors);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Round 2: New ClassDB / semantic checks
// ---------------------------------------------------------------------------

/// C3 (extended): Using the return value of a void function.
fn check_use_void_return(
    root: &Node,
    source: &str,
    symbols: &SymbolTable,
    errors: &mut Vec<StructuralError>,
) {
    check_use_void_in_node(*root, source, symbols, errors);
}

fn check_use_void_in_node(
    node: Node,
    source: &str,
    symbols: &SymbolTable,
    errors: &mut Vec<StructuralError>,
) {
    let bytes = source.as_bytes();

    // Check: var x = void_func()
    if node.kind() == "variable_statement"
        && let Some(value) = node.child_by_field_name("value")
        && value.kind() == "call"
        && let Some(func) = value
            .child_by_field_name("function")
            .or_else(|| value.named_child(0))
        && func.kind() == "identifier"
        && let Ok(func_name) = func.utf8_text(bytes)
    {
        // Check user-defined functions
        let is_void = symbols.functions.iter().any(|f| {
            f.name == func_name && f.return_type.as_ref().is_some_and(|r| r.name == "void")
        });
        // Check ClassDB methods (bare call = self method)
        let extends = symbols.extends.as_deref().unwrap_or("RefCounted");
        let is_classdb_void =
            !is_void && crate::class_db::method_return_type(extends, func_name) == Some("void");

        if is_void || is_classdb_void {
            let pos = value.start_position();
            errors.push(StructuralError {
                line: pos.row as u32 + 1,
                column: pos.column as u32 + 1,
                message: format!(
                    "cannot use return value of `{func_name}()` because it returns void",
                ),
            });
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
                    check_use_void_in_node(cursor.node(), source, symbols, errors);
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
            check_use_void_in_node(cursor.node(), source, symbols, errors);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
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
fn check_virtual_override_signature(symbols: &SymbolTable, errors: &mut Vec<StructuralError>) {
    let extends = symbols.extends.as_deref().unwrap_or("RefCounted");
    for func in &symbols.functions {
        // Only check virtual overrides (functions starting with _)
        if !func.name.starts_with('_') {
            continue;
        }

        // Try ClassDB first, fall back to well-known virtuals
        let (ret_type, total) =
            if let Some(sig) = crate::class_db::method_signature(extends, &func.name) {
                (sig.return_type, sig.total_params as usize)
            } else if let Some((ret, params)) = known_virtual_signature(&func.name) {
                (ret, params as usize)
            } else {
                continue;
            };

        // D1: Wrong return type
        if let Some(ref ret) = func.return_type
            && !ret.name.is_empty()
            && ret.name != "void"
            && ret_type != "Variant"
            && ret.name != ret_type
        {
            errors.push(StructuralError {
                line: func.line as u32 + 1,
                column: 1,
                message: format!(
                    "override `{}()` has return type `{}` but parent expects `{}`",
                    func.name, ret.name, ret_type,
                ),
            });
        }

        // D2: Wrong param count
        let user_count = func.params.len();
        if user_count != total {
            errors.push(StructuralError {
                line: func.line as u32 + 1,
                column: 1,
                message: format!(
                    "override `{}()` has {} parameter(s) but parent expects {}",
                    func.name, user_count, total,
                ),
            });
        }
    }
    for (_, inner) in &symbols.inner_classes {
        check_virtual_override_signature(inner, errors);
    }
}

/// D3: Cyclic inner class inheritance.
fn check_cyclic_inner_class(symbols: &SymbolTable, errors: &mut Vec<StructuralError>) {
    // Build a map of inner class name -> extends
    let extends_map: std::collections::HashMap<&str, &str> = symbols
        .inner_classes
        .iter()
        .filter_map(|(n, s)| s.extends.as_deref().map(|e| (n.as_str(), e)))
        .collect();

    // Check for cycles: walk the extends chain, detect if we revisit a class
    let mut reported = std::collections::HashSet::new();
    for (name, _) in &symbols.inner_classes {
        let mut visited = std::collections::HashSet::new();
        let mut current = name.as_str();
        while let Some(&parent) = extends_map.get(current) {
            if !visited.insert(parent) || parent == name {
                // Cycle detected — report only once
                if reported.insert(name.as_str()) {
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
fn check_export_invalid_type(symbols: &SymbolTable, errors: &mut Vec<StructuralError>) {
    for var in &symbols.variables {
        let has_export = var.annotations.iter().any(|a| a == "export");
        if !has_export {
            continue;
        }
        if let Some(ref type_ann) = var.type_ann
            && type_ann.name == "Object"
        {
            errors.push(StructuralError {
                line: var.line as u32 + 1,
                column: 1,
                message: format!(
                    "`@export` type `Object` is not a valid export type for variable `{}`",
                    var.name,
                ),
            });
        }
    }
    for (_, inner) in &symbols.inner_classes {
        check_export_invalid_type(inner, errors);
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
    symbols: &SymbolTable,
    errors: &mut Vec<StructuralError>,
) {
    check_typed_array_in_node(root, source, symbols, errors);
}

fn check_typed_array_in_node(
    node: &Node,
    source: &str,
    symbols: &SymbolTable,
    errors: &mut Vec<StructuralError>,
) {
    // Look for variable declarations with typed array annotation and array literal initializer
    if node.kind() == "variable_statement"
        && let Some(type_node) = node.child_by_field_name("type")
        && let Ok(type_text) = type_node.utf8_text(source.as_bytes())
        && let Some(element_type) = type_text
            .strip_prefix("Array[")
            .and_then(|s| s.strip_suffix(']'))
        && let Some(value_node) = node.child_by_field_name("value")
        && value_node.kind() == "array"
    {
        check_array_elements(&value_node, source, symbols, element_type, errors);
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            check_typed_array_in_node(&cursor.node(), source, symbols, errors);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

fn check_array_elements(
    array_node: &Node,
    source: &str,
    symbols: &SymbolTable,
    expected_type: &str,
    errors: &mut Vec<StructuralError>,
) {
    let mut cursor = array_node.walk();
    for child in array_node.children(&mut cursor) {
        if !child.is_named() {
            continue;
        }
        let Some(actual) = type_inference::infer_expression_type(&child, source, symbols) else {
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
fn types_assignable(declared: &str, actual: &str) -> bool {
    if declared == actual || declared == "Variant" || actual == "Variant" {
        return true;
    }
    // Numeric widening: int → float
    if declared == "float" && actual == "int" {
        return true;
    }
    // Godot implicit conversions: String → StringName, String → NodePath
    if (declared == "StringName" || declared == "NodePath") && actual == "String" {
        return true;
    }
    // Class inheritance: allow both upcast and downcast (Godot defers to runtime)
    if crate::class_db::class_exists(declared) && crate::class_db::class_exists(actual) {
        return crate::class_db::inherits(actual, declared)
            || crate::class_db::inherits(declared, actual);
    }
    false
}

/// H16: Cannot call a variable directly — e.g. `f()` where `f: Callable`.
/// Godot requires `.call()` syntax for Callable-typed variables.
fn check_callable_direct_call(
    root: &Node,
    source: &str,
    symbols: &SymbolTable,
    errors: &mut Vec<StructuralError>,
) {
    // Collect class-level Callable variables
    let mut callable_names: Vec<String> = symbols
        .variables
        .iter()
        .filter(|v| v.type_ann.as_ref().is_some_and(|t| t.name == "Callable"))
        .map(|v| v.name.clone())
        .collect();
    check_callable_in_node(root, source, symbols, &mut callable_names, errors);
}

fn check_callable_in_node(
    node: &Node,
    source: &str,
    symbols: &SymbolTable,
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
        && !symbols.functions.iter().any(|f| f.name == name)
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
            check_callable_in_node(&cursor.node(), source, symbols, callable_names, errors);
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
    symbols: &SymbolTable,
    errors: &mut Vec<StructuralError>,
) {
    check_for_iterable_in_node(root, source, symbols, errors);
}

fn check_for_iterable_in_node(
    node: &Node,
    source: &str,
    symbols: &SymbolTable,
    errors: &mut Vec<StructuralError>,
) {
    if node.kind() == "for_statement"
        && let Some(iter_node) = node.child_by_field_name("right")
        && let Some(ty) = type_inference::infer_expression_type(&iter_node, source, symbols)
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
            check_for_iterable_in_node(&cursor.node(), source, symbols, errors);
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
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Round 4: B4 — Argument count mismatch
// ---------------------------------------------------------------------------

/// Try to infer a local variable's type by finding its declaration in the enclosing scope.
/// This handles `var v := Vector2()` patterns where `v` has an inferred type.
fn infer_local_var_type(
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
fn check_assign_type_mismatch(
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
fn check_return_type_mismatch(
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
fn check_invalid_operators(
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
            // Vector arithmetic
            if left == right && is_vector_type(left) {
                return true;
            }
            // Array + Array
            if left == "Array" && right == "Array" {
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

/// B6: Invalid cast — `x as Node` where x: int.
fn check_invalid_cast(
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
fn inferred_type_name(ty: &type_inference::InferredType) -> Option<&str> {
    match ty {
        type_inference::InferredType::Builtin(b) => Some(b),
        type_inference::InferredType::Class(c) => Some(c.as_str()),
        type_inference::InferredType::Enum(e) => Some(e.as_str()),
        type_inference::InferredType::Void => Some("void"),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Round 5 continued: B3 — Argument type mismatch
// ---------------------------------------------------------------------------

/// B3: Argument type mismatch — wrong types passed to functions/methods/constructors.
fn check_arg_type_mismatch(
    root: &Node,
    source: &str,
    symbols: &SymbolTable,
    errors: &mut Vec<StructuralError>,
) {
    check_arg_types_in_node(root, source, symbols, errors);
}

fn check_arg_types_in_node(
    node: &Node,
    source: &str,
    symbols: &SymbolTable,
    errors: &mut Vec<StructuralError>,
) {
    // Identifier calls: user functions, self methods, constructors
    if node.kind() == "call"
        && let Some(callee) = node.named_child(0)
        && let Ok(func_name) = callee.utf8_text(source.as_bytes())
        && let Some(args_node) = node.child_by_field_name("arguments")
    {
        // User-defined functions — check param types from SymbolTable
        if let Some(func) = symbols.functions.iter().find(|f| f.name == func_name) {
            check_call_arg_types_user(func_name, &func.params, &args_node, source, symbols, errors);
        }
        // Self methods via ClassDB (extends chain)
        else if let Some(extends) = &symbols.extends {
            check_call_arg_types_classdb(func_name, extends, &args_node, source, symbols, errors);
        }
        // Constructor: Vector2("bad", "args") or builtin conversion: int([])
        if callee.kind() == "identifier"
            && (crate::class_db::class_exists(func_name)
                || is_builtin_convertible(func_name)
                || constructor_param_counts(func_name).is_some())
        {
            check_constructor_arg_types(func_name, &args_node, source, symbols, errors);
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
                        type_inference::infer_expression_type(&receiver, source, symbols);
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
                            symbols,
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
            check_arg_types_in_node(&cursor.node(), source, symbols, errors);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

/// Check user-defined function argument types.
fn check_call_arg_types_user(
    func_name: &str,
    params: &[symbol_table::ParamDecl],
    args_node: &Node,
    source: &str,
    symbols: &SymbolTable,
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
            && let Some(actual) = type_inference::infer_expression_type(arg, source, symbols)
            && let Some(actual_name) = inferred_type_name(&actual)
            && !types_assignable(&type_ann.name, actual_name)
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
    symbols: &SymbolTable,
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
            && let Some(actual) = type_inference::infer_expression_type(arg, source, symbols)
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

fn is_builtin_convertible(name: &str) -> bool {
    matches!(name, "int" | "float" | "bool" | "String" | "str")
}

/// Check constructor argument types (e.g., `Vector2("bad", "args")` or `int([])`).
fn check_constructor_arg_types(
    type_name: &str,
    args_node: &Node,
    source: &str,
    symbols: &SymbolTable,
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
                    type_inference::infer_expression_type(&args[0], source, symbols)
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
                    type_inference::infer_expression_type(&args[0], source, symbols)
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
            && let Some(actual) = type_inference::infer_expression_type(arg, source, symbols)
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
fn constructor_param_counts(type_name: &str) -> Option<&'static [u8]> {
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
fn check_arg_count(
    root: &Node,
    source: &str,
    symbols: &SymbolTable,
    errors: &mut Vec<StructuralError>,
) {
    check_arg_count_in_node(root, source, symbols, errors);
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
    symbols: &SymbolTable,
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
            if let Some(func) = symbols.functions.iter().find(|f| f.name == name) {
                let required = func.params.iter().filter(|p| !p.has_default).count();
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
            let ty = type_inference::infer_expression_type(&receiver, source, symbols)
                .or_else(|| infer_local_var_type(&receiver, source, symbols));
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
        check_attribute_call_args(node, source, symbols, errors);
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            check_arg_count_in_node(&cursor.node(), source, symbols, errors);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

fn check_attribute_call_args(
    node: &Node,
    source: &str,
    symbols: &SymbolTable,
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
        let ty = type_inference::infer_expression_type(&receiver, source, symbols)
            .or_else(|| infer_local_var_type(&receiver, source, symbols));
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
fn check_builtin_method_not_found(
    root: &Node,
    source: &str,
    symbols: &SymbolTable,
    errors: &mut Vec<StructuralError>,
) {
    check_builtin_method_in_node(root, source, symbols, errors);
}

fn check_builtin_method_in_node(
    node: &Node,
    source: &str,
    symbols: &SymbolTable,
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
        let ty = type_inference::infer_expression_type(&receiver, source, symbols)
            .or_else(|| infer_local_var_type(&receiver, source, symbols));
        if let Some(ref ty) = ty
            && let Some(type_name) = resolve_builtin_type_name(ty)
            && type_inference::is_builtin_type(type_name)
            && !method_name.starts_with('_')
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
            check_builtin_method_in_node(&cursor.node(), source, symbols, errors);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

/// A3: Property not found on builtin type — `v.zz` where v: Vector2.
fn check_builtin_property_not_found(
    root: &Node,
    source: &str,
    symbols: &SymbolTable,
    errors: &mut Vec<StructuralError>,
) {
    check_builtin_property_in_node(root, source, symbols, errors);
}

fn check_builtin_property_in_node(
    node: &Node,
    source: &str,
    symbols: &SymbolTable,
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
        let ty = type_inference::infer_expression_type(&receiver, source, symbols)
            .or_else(|| infer_local_var_type(&receiver, source, symbols));
        if let Some(ref ty) = ty
            && let Some(type_name) = resolve_builtin_type_name(ty)
            && type_inference::is_builtin_type(type_name)
            && !member_name.starts_with('_')
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
            check_builtin_property_in_node(&cursor.node(), source, symbols, errors);
            if !cursor.goto_next_sibling() {
                break;
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

// ---------------------------------------------------------------------------
// Round 6: A1-A4 — Name resolution
// ---------------------------------------------------------------------------

/// A4: Type not found in `as`/`is` expressions.
fn check_type_not_found(
    root: &Node,
    source: &str,
    symbols: &SymbolTable,
    project: &ProjectIndex,
    errors: &mut Vec<StructuralError>,
) {
    check_type_not_found_in_node(root, source, symbols, project, errors);
}

fn check_type_not_found_in_node(
    node: &Node,
    source: &str,
    symbols: &SymbolTable,
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
        && !is_known_type(type_name, symbols, project)
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
            check_type_not_found_in_node(&cursor.node(), source, symbols, project, errors);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

/// A2: Method not found — `get_chlidren()` on self, `s.nonexistent()` on typed variable.
fn check_method_not_found(
    root: &Node,
    source: &str,
    symbols: &SymbolTable,
    project: &ProjectIndex,
    errors: &mut Vec<StructuralError>,
) {
    check_method_not_found_in_node(root, source, symbols, project, errors);
}

fn check_method_not_found_in_node(
    node: &Node,
    source: &str,
    symbols: &SymbolTable,
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
        let is_known = symbols.functions.iter().any(|f| f.name == func_name)
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
            )
            || func_name.starts_with('_'); // Virtual callbacks
        if !is_known {
            // Check ClassDB via extends chain
            let mut found = symbols
                .extends
                .as_ref()
                .is_some_and(|ext| crate::class_db::method_exists(ext, func_name));
            // Check ProjectIndex for cross-file base class methods
            if !found && let Some(ext) = &symbols.extends {
                found = project.method_exists(ext, func_name);
            }
            if !found {
                errors.push(StructuralError {
                    line: callee.start_position().row as u32 + 1,
                    column: callee.start_position().column as u32 + 1,
                    message: format!(
                        "function \"{func_name}()\" not found in base {}",
                        symbols.extends.as_deref().unwrap_or("self"),
                    ),
                });
            }
        }
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            check_method_not_found_in_node(&cursor.node(), source, symbols, project, errors);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

/// A1: Undefined identifier — `nonexistent_variable` not declared.
fn check_undefined_identifiers(
    root: &Node,
    source: &str,
    symbols: &SymbolTable,
    project: &ProjectIndex,
    errors: &mut Vec<StructuralError>,
) {
    // Build set of known names: class variables, functions, enums, inner classes, params
    let mut known: std::collections::HashSet<String> = std::collections::HashSet::new();

    // Class-level variables
    for v in &symbols.variables {
        known.insert(v.name.clone());
    }
    // Functions
    for f in &symbols.functions {
        known.insert(f.name.clone());
    }
    // Enums and their members
    for e in &symbols.enums {
        known.insert(e.name.clone());
        for member in &e.members {
            known.insert(member.clone());
        }
    }
    // Inner classes
    for (name, _) in &symbols.inner_classes {
        known.insert(name.clone());
    }
    // Signals
    for s in &symbols.signals {
        known.insert(s.name.clone());
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
                    if let Some(name_node) = param.named_child(0)
                        && name_node.kind() == "identifier"
                        && let Ok(name) = name_node.utf8_text(source.as_bytes())
                    {
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
            check_undefined_in_body(&body, source, symbols, project, &mut func_known, errors);
        }
    }
}

fn check_undefined_in_body(
    node: &Node,
    source: &str,
    symbols: &SymbolTable,
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
        && !is_identifier_context_ok(node, name, source, symbols, project)
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
            check_undefined_in_body(&cursor.node(), source, symbols, project, known, errors);
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
    symbols: &SymbolTable,
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
    if is_known_type(name, symbols, project) {
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

    // ClassDB: extends chain for properties, methods, and constants/enums
    if let Some(ext) = &symbols.extends
        && (crate::class_db::property_exists(ext, name)
            || crate::class_db::method_exists(ext, name)
            || crate::class_db::constant_exists(ext, name))
    {
        return true;
    }

    // Cross-file: methods/properties from project-defined base classes
    if let Some(ext) = &symbols.extends
        && (project.method_exists(ext, name) || project.variable_type(ext, name).is_some())
    {
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
            | "weakref"
    )
}

/// A1 special: `super.nonexistent_parent_method()` — check method exists in parent class.
fn check_super_method_not_found(
    root: &Node,
    source: &str,
    symbols: &SymbolTable,
    project: &ProjectIndex,
    errors: &mut Vec<StructuralError>,
) {
    check_super_method_in_node(root, source, symbols, project, errors);
}

fn check_super_method_in_node(
    node: &Node,
    source: &str,
    symbols: &SymbolTable,
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
        let mut cursor2 = node.walk();
        for child in node.children(&mut cursor2) {
            if child.kind() == "attribute_call"
                && let Some(method_node) = child.named_child(0)
                && let Ok(method_name) = method_node.utf8_text(source.as_bytes())
            {
                let mut found = symbols
                    .extends
                    .as_ref()
                    .is_some_and(|ext| crate::class_db::method_exists(ext, method_name));
                if !found && let Some(ext) = &symbols.extends {
                    found = project.method_exists(ext, method_name);
                }
                if !found {
                    errors.push(StructuralError {
                        line: method_node.start_position().row as u32 + 1,
                        column: method_node.start_position().column as u32 + 1,
                        message: format!(
                            "function \"{method_name}()\" not found in base {}",
                            symbols.extends.as_deref().unwrap_or("Node"),
                        ),
                    });
                }
            }
        }
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            check_super_method_in_node(&cursor.node(), source, symbols, project, errors);
            if !cursor.goto_next_sibling() {
                break;
            }
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
        validate_structure(&tree.root_node(), source, &symbols, None)
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
        assert!(
            errs.iter()
                .any(|e| e.message.contains("required parameter"))
        );
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
        assert!(
            errs.iter()
                .any(|e| e.message.contains("duplicate `class_name`"))
        );
    }

    #[test]
    fn duplicate_extends() {
        let source = "extends Node\nextends Node2D\n";
        let errs = structural_errors(source);
        assert!(
            errs.iter()
                .any(|e| e.message.contains("duplicate `extends`"))
        );
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
        assert!(
            errs.iter()
                .any(|e| e.message.contains("duplicate parameter"))
        );
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
        assert!(
            errs.iter()
                .any(|e| e.message.contains("self") && e.message.contains("static"))
        );
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
        assert!(
            errs.iter()
                .any(|e| e.message.contains("health") && e.message.contains("static"))
        );
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
        assert!(
            errs.iter()
                .any(|e| e.message.contains("bar") && e.message.contains("static"))
        );
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
        assert!(
            errs.iter()
                .any(|e| e.message.contains("constant") && e.message.contains("MAX"))
        );
    }

    #[test]
    fn assign_to_enum_member() {
        let source = "\
enum State { IDLE, RUNNING }
func f():
\tIDLE = 5
";
        let errs = structural_errors(source);
        assert!(
            errs.iter()
                .any(|e| e.message.contains("enum value") && e.message.contains("IDLE"))
        );
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
        assert!(
            errs.iter()
                .any(|e| e.message.contains("void") && e.message.contains("return"))
        );
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
        assert!(
            errs.iter()
                .any(|e| e.message.contains("get_node") && e.message.contains("static"))
        );
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
        assert!(
            errs.iter()
                .any(|e| e.message.contains("export") && e.message.contains("no type"))
        );
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
        assert!(
            errs.iter()
                .any(|e| e.message.contains("export") && e.message.contains("static"))
        );
    }

    // -- E4: Duplicate @export --

    #[test]
    fn duplicate_export() {
        let source = "@export\n@export\nvar x: int = 0\n";
        let errs = structural_errors(source);
        assert!(
            errs.iter()
                .any(|e| e.message.contains("duplicate") && e.message.contains("export"))
        );
    }

    // -- H17: Object() constructor --

    #[test]
    fn object_direct_constructor() {
        let source = "func f():\n\tvar o = Object()\n";
        let errs = structural_errors(source);
        assert!(
            errs.iter()
                .any(|e| e.message.contains("Object()") && e.message.contains("Object.new()"))
        );
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
        assert!(
            errs.iter()
                .any(|e| e.message.contains("preload") && e.message.contains("constant string"))
        );
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
        assert!(
            errs.iter()
                .any(|e| e.message.contains("range") && e.message.contains("at most 3"))
        );
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
        assert!(
            errs.iter()
                .any(|e| e.message.contains("shadows") && e.message.contains("int"))
        );
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

    #[test]
    fn h16_callable_direct_call() {
        let source = "extends Node\nfunc _ready():\n\tvar f: Callable = func(): pass\n\tf()\n";
        let errs = classdb_errors(source);
        assert!(
            !errs.is_empty(),
            "expected callable direct call error, got none"
        );
        assert!(
            errs[0].message.contains("not found"),
            "msg: {}",
            errs[0].message
        );
    }

    #[test]
    fn b4_too_few_user_func() {
        let source = "extends Node\nfunc my_func(a: int, b: int, c: int) -> int:\n\treturn a + b + c\nfunc _ready():\n\tmy_func(1)\n";
        let errs = classdb_errors(source);
        assert!(errs.iter().any(|e| e.message.contains("too few")));
    }

    #[test]
    fn b4_too_many_user_func() {
        let source = "extends Node\nfunc my_func(a: int) -> int:\n\treturn a\nfunc _ready():\n\tmy_func(1, 2, 3)\n";
        let errs = classdb_errors(source);
        assert!(errs.iter().any(|e| e.message.contains("too many")));
    }

    #[test]
    fn b4_too_few_builtin() {
        let source = "extends Node\nfunc _ready():\n\tlerp(1.0, 2.0)\n";
        let errs = classdb_errors(source);
        assert!(errs.iter().any(|e| e.message.contains("too few")));
    }

    #[test]
    fn b5_bool_multiply() {
        let source = "extends Node\nfunc _ready():\n\tvar x = true * false\n";
        let errs = classdb_errors(source);
        assert!(
            errs.iter().any(|e| e.message.contains("invalid operands")),
            "expected operator error, got: {errs:?}"
        );
    }

    #[test]
    fn b5_array_minus_int() {
        let source = "extends Node\nfunc _ready():\n\tvar x = [] - 5\n";
        let errs = classdb_errors(source);
        assert!(
            errs.iter().any(|e| e.message.contains("invalid operands")),
            "expected operator error, got: {errs:?}"
        );
    }

    #[test]
    fn b1_assign_type_mismatch() {
        let source = "extends Node\nvar health: int = \"hello\"\n";
        let errs = classdb_errors(source);
        assert!(
            errs.iter().any(|e| e.message.contains("cannot assign")),
            "expected type mismatch, got: {errs:?}"
        );
    }

    #[test]
    fn b2_return_type_mismatch() {
        let source = "extends Node\nfunc f() -> int:\n\treturn \"hello\"\n";
        let errs = classdb_errors(source);
        assert!(
            errs.iter().any(|e| e.message.contains("cannot return")),
            "expected return type error, got: {errs:?}"
        );
    }

    #[test]
    fn b6_invalid_cast_int_to_node() {
        let source = "extends Node\nfunc _ready():\n\tvar x: int = 42\n\tvar n := x as Node\n";
        let errs = classdb_errors(source);
        assert!(
            errs.iter().any(|e| e.message.contains("invalid cast")),
            "expected cast error, got: {errs:?}"
        );
    }

    #[test]
    fn b1_reassign_local_wrong_type() {
        let source = "extends Node\nfunc _ready():\n\tvar x: int = 10\n\tx = \"now a string\"\n";
        let errs = classdb_errors(source);
        assert!(
            errs.iter().any(|e| e.message.contains("cannot assign")),
            "expected type mismatch, got: {errs:?}"
        );
    }

    #[test]
    fn b2_return_node_as_string() {
        let source = "extends Node\nfunc get_name_str() -> String:\n\treturn Node.new()\n";
        let errs = classdb_errors(source);
        assert!(
            errs.iter().any(|e| e.message.contains("cannot return")),
            "expected return type error, got: {errs:?}"
        );
    }
}
