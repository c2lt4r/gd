use std::collections::HashMap;
use std::path::{Path, PathBuf};

use miette::Result;
use serde::Serialize;

use crate::core::gd_ast;

use super::{
    declaration_full_range, find_declaration_by_name, find_declaration_in_class, line_starts,
    re_indent_to_depth,
};

// ── Output ──────────────────────────────────────────────────────────────────

#[derive(Serialize, Debug)]
pub struct EditOutput {
    pub file: String,
    pub operation: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    pub applied: bool,
    pub lines_changed: u32,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

// ── Shared helpers ──────────────────────────────────────────────────────────

/// Validate that an edit didn't introduce new parse errors compared to the original.
fn validate_no_new_errors(original: &str, edited: &str) -> Result<()> {
    super::validate_no_new_errors(original, edited)
}

/// Format GDScript source using the project's formatter config.
fn format_source(source: &str, project_root: &Path) -> Result<String> {
    let config = crate::core::config::Config::load(project_root)?;
    let tree = crate::core::parser::parse(source)?;
    let mut printer = crate::fmt::printer::Printer::from_config(&config.fmt);
    printer.format(&tree.root_node(), source);
    let formatted = printer.finish();
    if let Some(err) = crate::fmt::verify_format(source, &formatted, &config.fmt) {
        return Err(miette::miette!("format safety check failed: {err}"));
    }
    Ok(formatted)
}

// ── replace-body ────────────────────────────────────────────────────────────

pub fn replace_body(
    file: &Path,
    name: &str,
    class: Option<&str>,
    new_body: &str,
    no_format: bool,
    dry_run: bool,
    project_root: &Path,
) -> Result<EditOutput> {
    let source =
        std::fs::read_to_string(file).map_err(|e| miette::miette!("cannot read file: {e}"))?;
    let tree = crate::core::parser::parse(&source)?;
    let file_ast = gd_ast::convert(&tree, &source);
    let rel = crate::core::fs::relative_slash(file, project_root);

    // Find the target function/constructor
    let decl = if let Some(cls) = class {
        let inner = file_ast
            .find_class(cls)
            .ok_or_else(|| miette::miette!("class '{cls}' not found in {rel}"))?;
        find_declaration_in_class(inner, name)
            .ok_or_else(|| miette::miette!("symbol '{name}' not found in class '{cls}'"))?
    } else {
        find_declaration_by_name(&file_ast, name)
            .ok_or_else(|| miette::miette!("symbol '{name}' not found in {rel}"))?
    };

    let kind = decl.kind();
    if kind != "function_definition" && kind != "constructor_definition" {
        return Err(miette::miette!(
            "'{name}' is a {}, not a function — replace-body only works on functions",
            super::declaration_kind_str(kind)
        ));
    }

    // Guard: reject input that looks like it contains a function signature.
    // The body of a function cannot legitimately start with `func ` or `static func `.
    let first_content_line = new_body
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
        .trim();
    if first_content_line.starts_with("func ") || first_content_line.starts_with("static func ") {
        return Err(miette::miette!(
            "input appears to contain a function signature (`{}`); \
             replace-body expects only the body (indented statements), not the signature",
            first_content_line.chars().take(60).collect::<String>()
        ));
    }

    // Get the body node
    let body = decl
        .child_by_field_name("body")
        .ok_or_else(|| miette::miette!("function '{name}' has no body"))?;

    // Find the first named child (actual statement) to get indentation.
    // The body node itself starts right after `:`, including the newline.
    let first_stmt = body
        .named_child(0)
        .ok_or_else(|| miette::miette!("function '{name}' has an empty body"))?;

    let body_end = body.end_byte();

    // Determine target indentation from the first statement's line
    let stmt_line = first_stmt.start_position().row;
    let starts = line_starts(&source);
    let line_start = starts[stmt_line];
    let line_end = starts.get(stmt_line + 1).copied().unwrap_or(source.len());
    let first_line = &source[line_start..line_end].trim_end_matches('\n');
    let target_indent_count = first_line.len() - first_line.trim_start().len();

    // Determine indent unit: if tabs, count tabs; if spaces, count spaces
    let target_tabs = if first_line.starts_with('\t') {
        target_indent_count
    } else {
        // For space indentation, approximate tab depth
        target_indent_count / 4
    };

    // Re-indent the new body to match
    let reindented = re_indent_to_depth(new_body.trim_end(), target_tabs);

    // Build the new source — replace from first statement to body end
    // Keep everything up to (and including) the newline after `:` on the signature line
    let mut result = String::with_capacity(source.len());
    result.push_str(&source[..line_start]);
    result.push_str(&reindented);
    if !reindented.ends_with('\n') {
        result.push('\n');
    }
    result.push_str(&source[body_end..]);

    // Validate
    validate_no_new_errors(&source, &result)?;

    // Format
    let final_source = if no_format {
        result
    } else {
        format_source(&result, project_root)?
    };

    let lines_changed = diff_line_count(&source, &final_source);

    if !dry_run {
        std::fs::write(file, &final_source)
            .map_err(|e| miette::miette!("cannot write file: {e}"))?;

        // Record undo
        let mut snaps: HashMap<PathBuf, Option<Vec<u8>>> = HashMap::new();
        snaps.insert(file.to_path_buf(), Some(source.as_bytes().to_vec()));
        let stack = super::undo::UndoStack::open(project_root);
        let _ = stack.record(
            "replace-body",
            &format!("replace body of {name}"),
            &snaps,
            project_root,
        );
    }

    Ok(EditOutput {
        file: rel,
        operation: "replace-body",
        symbol: Some(name.to_string()),
        applied: !dry_run,
        lines_changed,
        warnings: vec![],
    })
}

// ── insert ──────────────────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
pub fn insert(
    file: &Path,
    anchor_name: &str,
    after: bool, // true = --after, false = --before
    class: Option<&str>,
    content: &str,
    no_format: bool,
    dry_run: bool,
    project_root: &Path,
) -> Result<EditOutput> {
    let source =
        std::fs::read_to_string(file).map_err(|e| miette::miette!("cannot read file: {e}"))?;
    let tree = crate::core::parser::parse(&source)?;
    let file_ast = gd_ast::convert(&tree, &source);
    let rel = crate::core::fs::relative_slash(file, project_root);

    let decl = if let Some(cls) = class {
        let inner = file_ast
            .find_class(cls)
            .ok_or_else(|| miette::miette!("class '{cls}' not found in {rel}"))?;
        find_declaration_in_class(inner, anchor_name)
            .ok_or_else(|| miette::miette!("symbol '{anchor_name}' not found in class '{cls}'"))?
    } else {
        find_declaration_by_name(&file_ast, anchor_name)
            .ok_or_else(|| miette::miette!("symbol '{anchor_name}' not found in {rel}"))?
    };

    let (full_start, full_end) = declaration_full_range(decl, &source);

    let insert_point = if after { full_end } else { full_start };

    // Build new source
    let mut result = String::with_capacity(source.len() + content.len());
    result.push_str(&source[..insert_point]);

    // Ensure proper newline separation
    if after {
        if !result.ends_with('\n') {
            result.push('\n');
        }
        result.push_str(content.trim_end());
        result.push('\n');
    } else {
        let trimmed = content.trim_end();
        result.push_str(trimmed);
        if !trimmed.ends_with('\n') {
            result.push('\n');
        }
    }

    result.push_str(&source[insert_point..]);

    // Validate
    validate_no_new_errors(&source, &result)?;

    // Format
    let final_source = if no_format {
        result
    } else {
        format_source(&result, project_root)?
    };

    let lines_changed = diff_line_count(&source, &final_source);

    if !dry_run {
        std::fs::write(file, &final_source)
            .map_err(|e| miette::miette!("cannot write file: {e}"))?;

        // Record undo
        let mut snaps: HashMap<PathBuf, Option<Vec<u8>>> = HashMap::new();
        snaps.insert(file.to_path_buf(), Some(source.as_bytes().to_vec()));
        let stack = super::undo::UndoStack::open(project_root);
        let _ = stack.record(
            "insert",
            &format!("insert near {anchor_name}"),
            &snaps,
            project_root,
        );
    }

    Ok(EditOutput {
        file: rel,
        operation: "insert",
        symbol: Some(anchor_name.to_string()),
        applied: !dry_run,
        lines_changed,
        warnings: vec![],
    })
}

// ── replace-symbol ──────────────────────────────────────────────────────────

pub fn replace_symbol(
    file: &Path,
    name: &str,
    class: Option<&str>,
    new_content: &str,
    no_format: bool,
    dry_run: bool,
    project_root: &Path,
) -> Result<EditOutput> {
    let source =
        std::fs::read_to_string(file).map_err(|e| miette::miette!("cannot read file: {e}"))?;
    let tree = crate::core::parser::parse(&source)?;
    let file_ast = gd_ast::convert(&tree, &source);
    let rel = crate::core::fs::relative_slash(file, project_root);

    let decl = if let Some(cls) = class {
        let inner = file_ast
            .find_class(cls)
            .ok_or_else(|| miette::miette!("class '{cls}' not found in {rel}"))?;
        find_declaration_in_class(inner, name)
            .ok_or_else(|| miette::miette!("symbol '{name}' not found in class '{cls}'"))?
    } else {
        find_declaration_by_name(&file_ast, name)
            .ok_or_else(|| miette::miette!("symbol '{name}' not found in {rel}"))?
    };

    // When the target is a class_name_statement the whole file IS that class,
    // so replace the entire file content rather than just the one-line statement.
    let (full_start, full_end) = if decl.kind() == "class_name_statement" {
        (0, source.len())
    } else {
        declaration_full_range(decl, &source)
    };

    // Determine the indentation depth of the original declaration so we can
    // re-indent the replacement content to match (critical for inner classes).
    let starts = line_starts(&source);
    let decl_line = decl.start_position().row;
    let line_start = starts[decl_line];
    let line_text = &source[line_start..decl.start_byte()];
    let target_tabs = line_text.chars().filter(|&c| c == '\t').count();

    // Re-indent new content to match the original declaration depth
    let reindented = re_indent_to_depth(new_content.trim_end(), target_tabs);

    // Build new source
    let mut result = String::with_capacity(source.len());
    result.push_str(&source[..full_start]);
    result.push_str(&reindented);
    if !reindented.ends_with('\n') {
        result.push('\n');
    }
    result.push_str(&source[full_end..]);

    // Validate
    validate_no_new_errors(&source, &result)?;

    // Format
    let final_source = if no_format {
        result
    } else {
        format_source(&result, project_root)?
    };

    let lines_changed = diff_line_count(&source, &final_source);

    if !dry_run {
        std::fs::write(file, &final_source)
            .map_err(|e| miette::miette!("cannot write file: {e}"))?;

        // Record undo
        let mut snaps: HashMap<PathBuf, Option<Vec<u8>>> = HashMap::new();
        snaps.insert(file.to_path_buf(), Some(source.as_bytes().to_vec()));
        let stack = super::undo::UndoStack::open(project_root);
        let _ = stack.record(
            "replace-symbol",
            &format!("replace symbol {name}"),
            &snaps,
            project_root,
        );
    }

    Ok(EditOutput {
        file: rel,
        operation: "replace-symbol",
        symbol: Some(name.to_string()),
        applied: !dry_run,
        lines_changed,
        warnings: vec![],
    })
}

// ── edit-range ──────────────────────────────────────────────────────────────

pub fn edit_range(
    file: &Path,
    start_line: usize, // 1-based inclusive
    end_line: usize,   // 1-based inclusive
    new_content: &str,
    no_format: bool,
    dry_run: bool,
    project_root: &Path,
) -> Result<EditOutput> {
    let source =
        std::fs::read_to_string(file).map_err(|e| miette::miette!("cannot read file: {e}"))?;
    let rel = crate::core::fs::relative_slash(file, project_root);

    if start_line == 0 || end_line == 0 {
        return Err(miette::miette!("line numbers are 1-based"));
    }
    if start_line > end_line {
        return Err(miette::miette!(
            "start-line ({start_line}) must be <= end-line ({end_line})"
        ));
    }

    // Handle empty or newline-only files: build the result directly
    let effectively_empty = source.is_empty() || source.chars().all(|c| c == '\n' || c == '\r');
    if effectively_empty && start_line == 1 && end_line == 1 {
        let trimmed = new_content.trim_end();
        let mut result = String::from(trimmed);
        if !result.ends_with('\n') {
            result.push('\n');
        }

        validate_no_new_errors("", &result)?;

        let final_source = if no_format {
            result
        } else {
            format_source(&result, project_root)?
        };

        let lines_changed = diff_line_count(&source, &final_source);

        if !dry_run {
            std::fs::write(file, &final_source)
                .map_err(|e| miette::miette!("cannot write file: {e}"))?;

            // Record undo
            let mut snaps: HashMap<PathBuf, Option<Vec<u8>>> = HashMap::new();
            snaps.insert(file.to_path_buf(), Some(source.as_bytes().to_vec()));
            let stack = super::undo::UndoStack::open(project_root);
            let _ = stack.record(
                "edit-range",
                &format!("edit range {start_line}-{end_line}"),
                &snaps,
                project_root,
            );
        }

        return Ok(EditOutput {
            file: rel,
            operation: "edit-range",
            symbol: None,
            applied: !dry_run,
            lines_changed,
            warnings: vec![],
        });
    }

    let starts = line_starts(&source);
    let total_lines = starts.len();

    if start_line > total_lines {
        return Err(miette::miette!(
            "start-line {start_line} exceeds file length ({total_lines} lines)"
        ));
    }

    let start_byte = starts[start_line - 1];
    let end_byte = if end_line >= total_lines {
        source.len()
    } else {
        starts[end_line] // start of the line *after* end_line
    };

    // Build new source
    let mut result = String::with_capacity(source.len());
    result.push_str(&source[..start_byte]);
    let trimmed = new_content.trim_end();
    result.push_str(trimmed);
    if !trimmed.ends_with('\n') {
        result.push('\n');
    }
    result.push_str(&source[end_byte..]);

    // Validate
    validate_no_new_errors(&source, &result)?;

    // Format
    let final_source = if no_format {
        result
    } else {
        format_source(&result, project_root)?
    };

    let lines_changed = diff_line_count(&source, &final_source);

    if !dry_run {
        std::fs::write(file, &final_source)
            .map_err(|e| miette::miette!("cannot write file: {e}"))?;

        // Record undo
        let mut snaps: HashMap<PathBuf, Option<Vec<u8>>> = HashMap::new();
        snaps.insert(file.to_path_buf(), Some(source.as_bytes().to_vec()));
        let stack = super::undo::UndoStack::open(project_root);
        let _ = stack.record(
            "edit-range",
            &format!("edit range {start_line}-{end_line}"),
            &snaps,
            project_root,
        );
    }

    Ok(EditOutput {
        file: rel,
        operation: "edit-range",
        symbol: None,
        applied: !dry_run,
        lines_changed,
        warnings: vec![],
    })
}

// ── Helpers ─────────────────────────────────────────────────────────────────

/// Count the number of lines that differ between two strings.
fn diff_line_count(old: &str, new: &str) -> u32 {
    let old_lines: Vec<&str> = old.lines().collect();
    let new_lines: Vec<&str> = new.lines().collect();
    let max = old_lines.len().max(new_lines.len());
    let mut changed = 0u32;
    for i in 0..max {
        let a = old_lines.get(i).copied().unwrap_or("");
        let b = new_lines.get(i).copied().unwrap_or("");
        if a != b {
            changed += 1;
        }
    }
    // Also count extra lines in longer file
    changed
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn setup(files: &[(&str, &str)]) -> TempDir {
        let temp = tempfile::Builder::new()
            .prefix("gdtest")
            .tempdir()
            .expect("create temp dir");
        fs::write(
            temp.path().join("project.godot"),
            "[application]\nconfig/name=\"test\"\n",
        )
        .expect("write project.godot");
        for (name, content) in files {
            fs::write(temp.path().join(name), content).expect("write file");
        }
        temp
    }

    // ── replace-body ────────────────────────────────────────────────────

    #[test]
    fn replace_body_basic() {
        let temp = setup(&[("player.gd", "extends Node\n\n\nfunc _ready():\n\tpass\n")]);
        let file = temp.path().join("player.gd");
        let result = replace_body(
            &file,
            "_ready",
            None,
            "\tprint(\"hello\")\n",
            true, // no_format to keep it simple
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        assert_eq!(result.operation, "replace-body");
        assert_eq!(result.symbol, Some("_ready".to_string()));

        let content = fs::read_to_string(&file).unwrap();
        assert!(content.contains("print(\"hello\")"));
        assert!(!content.contains("\tpass"));
        assert!(content.contains("func _ready():"));
    }

    #[test]
    fn replace_body_dry_run() {
        let temp = setup(&[("player.gd", "extends Node\n\n\nfunc _ready():\n\tpass\n")]);
        let file = temp.path().join("player.gd");
        let result = replace_body(
            &file,
            "_ready",
            None,
            "\tprint(\"hello\")\n",
            true,
            true, // dry_run
            temp.path(),
        )
        .unwrap();
        assert!(!result.applied);

        let content = fs::read_to_string(&file).unwrap();
        assert!(content.contains("\tpass"), "dry-run should not modify file");
    }

    #[test]
    fn replace_body_multiline() {
        let temp = setup(&[("player.gd", "extends Node\n\n\nfunc move(delta):\n\tpass\n")]);
        let file = temp.path().join("player.gd");
        let result = replace_body(
            &file,
            "move",
            None,
            "\tvar speed = 10\n\tposition += speed * delta\n",
            true,
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);

        let content = fs::read_to_string(&file).unwrap();
        assert!(content.contains("\tvar speed = 10"));
        assert!(content.contains("\tposition += speed * delta"));
    }

    #[test]
    fn replace_body_in_class() {
        let temp = setup(&[(
            "player.gd",
            "extends Node\n\n\nclass Inner:\n\tfunc foo():\n\t\tpass\n",
        )]);
        let file = temp.path().join("player.gd");
        let result = replace_body(
            &file,
            "foo",
            Some("Inner"),
            "\t\tprint(1)\n",
            true,
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);

        let content = fs::read_to_string(&file).unwrap();
        assert!(content.contains("\t\tprint(1)"));
    }

    #[test]
    fn replace_body_non_function_rejected() {
        let temp = setup(&[("player.gd", "extends Node\nvar speed = 10\n")]);
        let file = temp.path().join("player.gd");
        let result = replace_body(&file, "speed", None, "\t42\n", true, false, temp.path());
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("variable"));
    }

    #[test]
    fn replace_body_constructor() {
        let temp = setup(&[("player.gd", "extends Node\n\n\nfunc _init():\n\tpass\n")]);
        let file = temp.path().join("player.gd");
        let result = replace_body(
            &file,
            "_init",
            None,
            "\tprint(\"init\")\n",
            true,
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);

        let content = fs::read_to_string(&file).unwrap();
        assert!(content.contains("print(\"init\")"));
    }

    #[test]
    fn replace_body_reindents_from_zero() {
        let temp = setup(&[("player.gd", "extends Node\n\n\nfunc _ready():\n\tpass\n")]);
        let file = temp.path().join("player.gd");
        // Content with no indentation — should be reindented to 1 tab
        let result = replace_body(
            &file,
            "_ready",
            None,
            "print(\"hello\")\nprint(\"world\")\n",
            true,
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);

        let content = fs::read_to_string(&file).unwrap();
        assert!(content.contains("\tprint(\"hello\")"));
        assert!(content.contains("\tprint(\"world\")"));
    }

    #[test]
    fn replace_body_rejects_signature_in_input() {
        let temp = setup(&[("player.gd", "extends Node\n\n\nfunc _ready():\n\tpass\n")]);
        let file = temp.path().join("player.gd");
        let result = replace_body(
            &file,
            "_ready",
            None,
            "func _ready():\n\tprint(\"hello\")\n",
            true,
            false,
            temp.path(),
        );
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("function signature"),
            "expected signature error, got: {msg}"
        );
    }

    #[test]
    fn replace_body_rejects_static_func_signature() {
        let temp = setup(&[(
            "player.gd",
            "extends Node\n\n\nstatic func helper():\n\tpass\n",
        )]);
        let file = temp.path().join("player.gd");
        let result = replace_body(
            &file,
            "helper",
            None,
            "static func helper():\n\tprint(1)\n",
            true,
            false,
            temp.path(),
        );
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("function signature"),
            "expected signature error, got: {msg}"
        );
    }

    // ── insert ──────────────────────────────────────────────────────────

    #[test]
    fn insert_after() {
        let temp = setup(&[("player.gd", "extends Node\n\n\nfunc _ready():\n\tpass\n")]);
        let file = temp.path().join("player.gd");
        let result = insert(
            &file,
            "_ready",
            true, // after
            None,
            "\nfunc _process(delta):\n\tpass\n",
            true,
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        assert_eq!(result.operation, "insert");

        let content = fs::read_to_string(&file).unwrap();
        assert!(content.contains("func _process(delta):"));
        // _process should come after _ready
        let ready_pos = content.find("func _ready()").unwrap();
        let process_pos = content.find("func _process(delta)").unwrap();
        assert!(process_pos > ready_pos);
    }

    #[test]
    fn insert_before() {
        let temp = setup(&[("player.gd", "extends Node\n\n\nfunc _ready():\n\tpass\n")]);
        let file = temp.path().join("player.gd");
        let result = insert(
            &file,
            "_ready",
            false, // before
            None,
            "var speed = 10\n",
            true,
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);

        let content = fs::read_to_string(&file).unwrap();
        assert!(content.contains("var speed = 10"));
        let var_pos = content.find("var speed").unwrap();
        let ready_pos = content.find("func _ready()").unwrap();
        assert!(var_pos < ready_pos);
    }

    #[test]
    fn insert_dry_run() {
        let temp = setup(&[("player.gd", "extends Node\n\n\nfunc _ready():\n\tpass\n")]);
        let file = temp.path().join("player.gd");
        let result = insert(
            &file,
            "_ready",
            true,
            None,
            "\nfunc foo():\n\tpass\n",
            true,
            true, // dry_run
            temp.path(),
        )
        .unwrap();
        assert!(!result.applied);

        let content = fs::read_to_string(&file).unwrap();
        assert!(!content.contains("func foo()"));
    }

    // ── replace-symbol ──────────────────────────────────────────────────

    #[test]
    fn replace_symbol_var() {
        let temp = setup(&[(
            "player.gd",
            "extends Node\nvar speed = 10\n\n\nfunc _ready():\n\tpass\n",
        )]);
        let file = temp.path().join("player.gd");
        let result = replace_symbol(
            &file,
            "speed",
            None,
            "var speed: float = 42.0\n",
            true,
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        assert_eq!(result.operation, "replace-symbol");

        let content = fs::read_to_string(&file).unwrap();
        assert!(content.contains("var speed: float = 42.0"));
        assert!(!content.contains("var speed = 10"));
    }

    #[test]
    fn replace_symbol_function() {
        let temp = setup(&[(
            "player.gd",
            "extends Node\n\n\nfunc old_func():\n\tvar x = 1\n\tprint(x)\n",
        )]);
        let file = temp.path().join("player.gd");
        let result = replace_symbol(
            &file,
            "old_func",
            None,
            "func new_func():\n\tprint(\"replaced\")\n",
            true,
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);

        let content = fs::read_to_string(&file).unwrap();
        assert!(content.contains("func new_func():"));
        assert!(content.contains("print(\"replaced\")"));
        assert!(!content.contains("old_func"));
    }

    #[test]
    fn replace_symbol_class_name_replaces_whole_file() {
        let temp = setup(&[(
            "npc.gd",
            "class_name Npc\nextends Node\n\n\nvar speed = 100\n\n\nfunc _ready():\n\tpass\n",
        )]);
        let file = temp.path().join("npc.gd");
        let result = replace_symbol(
            &file,
            "Npc",
            None,
            "class_name Npc\nextends Node\n\n\nvar speed = 200\n\n\nfunc _ready():\n\tprint(1)\n",
            true,
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);

        let content = fs::read_to_string(&file).unwrap();
        assert!(content.contains("var speed = 200"));
        assert!(content.contains("print(1)"));
        // Old content must be gone
        assert!(!content.contains("var speed = 100"));
        assert!(!content.contains("\tpass"));
    }

    #[test]
    fn replace_symbol_inner_class_preserves_indent() {
        // Exact reproduction from false-positives report: replace-symbol on an
        // inner class function should keep the replacement at inner-class indent
        // level, not drop it to column 0.
        let temp = setup(&[(
            "tmp_script.gd",
            "extends RefCounted\n\n\nclass Inner:\n\tvar name: String = \"\"\n\tvar value: int = 0\n\n\tfunc get_name() -> String:\n\t\treturn name\n\n\tfunc get_value() -> int:\n\t\treturn value\n\n\tfunc set_value(v: int) -> void:\n\t\tvalue = v\n\n\nfunc outer_func() -> void:\n\tpass\n",
        )]);
        let file = temp.path().join("tmp_script.gd");
        let result = replace_symbol(
            &file,
            "get_name",
            Some("Inner"),
            "func get_name() -> Variant:\n\treturn name\n",
            true, // no_format — test raw re-indentation
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);

        let content = fs::read_to_string(&file).unwrap();
        // Must be indented inside Inner (1 tab for func, 2 tabs for body)
        assert!(
            content.contains("\tfunc get_name() -> Variant:"),
            "function should be at 1-tab indent inside Inner class, got:\n{content}"
        );
        assert!(
            content.contains("\t\treturn name"),
            "body should be at 2-tab indent inside Inner class, got:\n{content}"
        );
        // Sibling methods must remain at their original indent
        assert!(
            content.contains("\tfunc get_value() -> int:"),
            "sibling get_value should stay at 1-tab indent, got:\n{content}"
        );
        assert!(
            content.contains("\tfunc set_value(v: int) -> void:"),
            "sibling set_value should stay at 1-tab indent, got:\n{content}"
        );
        // Outer function must remain top-level
        assert!(
            content.contains("\nfunc outer_func() -> void:"),
            "outer_func should remain at indent 0, got:\n{content}"
        );
    }

    #[test]
    fn replace_symbol_dry_run() {
        let temp = setup(&[("player.gd", "extends Node\nvar speed = 10\n")]);
        let file = temp.path().join("player.gd");
        let result = replace_symbol(
            &file,
            "speed",
            None,
            "var speed = 99\n",
            true,
            true,
            temp.path(),
        )
        .unwrap();
        assert!(!result.applied);

        let content = fs::read_to_string(&file).unwrap();
        assert!(content.contains("var speed = 10"));
    }

    // ── edit-range ──────────────────────────────────────────────────────

    #[test]
    fn edit_range_basic() {
        let temp = setup(&[(
            "player.gd",
            "extends Node\nvar a = 1\nvar b = 2\nvar c = 3\n",
        )]);
        let file = temp.path().join("player.gd");
        let result = edit_range(
            &file,
            2,
            3, // replace lines 2-3
            "var x = 10\nvar y = 20\n",
            true,
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        assert_eq!(result.operation, "edit-range");
        assert!(result.symbol.is_none());

        let content = fs::read_to_string(&file).unwrap();
        assert!(content.contains("var x = 10"));
        assert!(content.contains("var y = 20"));
        assert!(!content.contains("var a = 1"));
        assert!(!content.contains("var b = 2"));
        assert!(content.contains("var c = 3"));
    }

    #[test]
    fn edit_range_dry_run() {
        let temp = setup(&[("player.gd", "extends Node\nvar a = 1\nvar b = 2\n")]);
        let file = temp.path().join("player.gd");
        let result =
            edit_range(&file, 2, 2, "var replaced = 99\n", true, true, temp.path()).unwrap();
        assert!(!result.applied);

        let content = fs::read_to_string(&file).unwrap();
        assert!(content.contains("var a = 1"));
    }

    #[test]
    fn edit_range_invalid_lines() {
        let temp = setup(&[("player.gd", "extends Node\nvar a = 1\n")]);
        let file = temp.path().join("player.gd");
        let result = edit_range(&file, 3, 1, "x\n", true, false, temp.path());
        assert!(result.is_err());
    }

    #[test]
    fn edit_range_zero_line() {
        let temp = setup(&[("player.gd", "extends Node\nvar a = 1\n")]);
        let file = temp.path().join("player.gd");
        let result = edit_range(&file, 0, 1, "x\n", true, false, temp.path());
        assert!(result.is_err());
    }

    #[test]
    fn edit_range_empty_file() {
        let temp = setup(&[("empty.gd", "")]);
        let file = temp.path().join("empty.gd");
        let result = edit_range(
            &file,
            1,
            1,
            "extends Node\n",
            true, // no_format
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        let content = fs::read_to_string(&file).unwrap();
        assert!(content.contains("extends Node"));
    }

    #[test]
    fn edit_range_newline_only_file() {
        let temp = setup(&[("nl.gd", "\n")]);
        let file = temp.path().join("nl.gd");
        let result = edit_range(
            &file,
            1,
            1,
            "extends Node\nvar x = 1\n",
            true,
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        let content = fs::read_to_string(&file).unwrap();
        assert!(content.contains("extends Node"));
        assert!(content.contains("var x = 1"));
    }

    #[test]
    fn diff_line_count_empty_to_content() {
        assert_eq!(diff_line_count("", "extends Node\nvar x = 1\n"), 2);
    }
}
