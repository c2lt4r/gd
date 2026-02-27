mod args;
mod builtins;
mod classdb;
mod identifiers;
mod structural;
mod types;

#[cfg(test)]
mod tests;

pub use classdb::check_classdb_errors;

use std::env;
use std::path::Path;

use clap::Args;
use miette::Result;
use owo_colors::OwoColorize;
use serde::Serialize;

use crate::core::symbol_table::SymbolTable;
use crate::core::workspace_index::ProjectIndex;
use crate::core::{
    config::Config, config::find_project_root, fs::collect_gdscript_files,
    fs::collect_resource_files, parser, resource_parser, scene, symbol_table,
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

use structural::validate_structure;

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

#[derive(Debug)]
pub struct StructuralError {
    pub line: u32,
    pub column: u32,
    pub message: String,
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
        let root_project = find_project_root(root).unwrap_or_else(|| ignore_base.clone());
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
                        validate_structure(&root_node, &source, &symbols, Some(&root_project));
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
    let file = crate::core::gd_ast::convert(tree, source);
    let lint_config = crate::core::config::LintConfig::default();
    let rules: [&dyn LintRule; 3] = [&DuplicateFunction, &DuplicateSignal, &DuplicateVariable];
    let mut diags = Vec::new();
    for rule in rules {
        diags.extend(rule.check(&file, source, &lint_config));
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
    let file = crate::core::gd_ast::convert(tree, source);
    let lint_config = crate::core::config::LintConfig::default();
    OverrideSignatureMismatch.check_with_project(&file, source, &lint_config, symbols, project)
}

// ---------------------------------------------------------------------------
// Promoted lint rules — errors that Godot's compiler also rejects
// ---------------------------------------------------------------------------

fn check_promoted_rules(
    tree: &tree_sitter::Tree,
    source: &str,
    symbols: &SymbolTable,
) -> Vec<crate::lint::rules::LintDiagnostic> {
    let file = crate::core::gd_ast::convert(tree, source);
    let lint_config = crate::core::config::LintConfig::default();
    let mut diags = Vec::new();

    // duplicate-key: duplicate dictionary keys are a compile error
    diags.extend(DuplicateKey.check(&file, source, &lint_config));

    // onready-with-export: @onready + @export is a compile error
    diags.extend(OnreadyWithExport.check_with_symbols(&file, source, &lint_config, symbols));

    // get-node-default-without-onready: $Path default without @onready is a compile error
    diags.extend(GetNodeDefaultWithoutOnready.check_with_symbols(
        &file,
        source,
        &lint_config,
        symbols,
    ));

    // native-method-override: overriding a native non-virtual method is a compile error
    diags.extend(NativeMethodOverride.check_with_symbols(&file, source, &lint_config, symbols));

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
