use std::env;
use std::path::Path;

use clap::Args;
use miette::Result;
use owo_colors::OwoColorize;
use serde::Serialize;
use tree_sitter::Node;

use crate::core::{
    config::Config, config::find_project_root, fs::collect_gdscript_files,
    fs::collect_resource_files, parser, resource_parser, scene,
};
use crate::lint::matches_ignore_pattern;

#[derive(Args)]
pub struct CheckArgs {
    /// Files or directories to check (defaults to current directory)
    pub paths: Vec<String>,
    /// Output format (human or json)
    #[arg(long, default_value = "human")]
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
                    let structural = validate_structure(&root_node, &source);

                    if has_parse_errors || !structural.is_empty() {
                        error_count += 1;
                        if json_mode {
                            let rel = crate::core::fs::relative_slash(file, &cwd);
                            if has_parse_errors {
                                let mut cursor = root_node.walk();
                                collect_errors(&mut cursor, file, &cwd, &mut parse_errors);
                            }
                            for err in &structural {
                                parse_errors.push(ParseError {
                                    file: rel.clone(),
                                    line: err.line,
                                    column: err.column,
                                    message: err.message.clone(),
                                });
                            }
                        } else {
                            if has_parse_errors {
                                let mut cursor = root_node.walk();
                                report_errors(&mut cursor, &source, file);
                            }
                            report_structural(&structural, &source, file);
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
                        eprintln!("{e}");
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
                        eprintln!("{e}");
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
        println!("{json}");
        if !output.ok {
            std::process::exit(1);
        }
        return Ok(());
    }

    if error_count > 0 {
        eprintln!("\n{checked} files checked, {error_count} with parse errors");
        std::process::exit(1);
    }

    println!("{} {} files checked", "✓".green(), checked);
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
fn validate_structure(root: &Node, source: &str) -> Vec<StructuralError> {
    let mut errors = Vec::new();
    check_top_level_statements(root, &mut errors);
    check_indentation_consistency(root, &mut errors);
    check_class_constants(root, source, &mut errors);
    check_variant_inference(root, source, &mut errors);
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
        if is_inferred
            && let Some(value) = node.child_by_field_name("value")
            && is_variant_producing_expr(&value, source)
        {
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
                    return matches!(method_name, "get" | "get_or_add" | "values" | "keys");
                }
            }
            false
        }
        _ => false,
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
            eprintln!(
                "{}:{}:{} {} parse error",
                file.display(),
                start.row + 1,
                start.column + 1,
                "error:".red().bold(),
            );
            eprintln!("  {line}");
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
        eprintln!(
            "{}:{}:{} {} {}",
            file.display(),
            err.line,
            err.column,
            "error:".red().bold(),
            err.message,
        );
        eprintln!("  {line}");
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
        eprintln!(
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
        validate_structure(&tree.root_node(), source)
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
}
