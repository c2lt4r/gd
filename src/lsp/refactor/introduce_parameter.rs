use std::collections::HashMap;
use std::path::{Path, PathBuf};

use miette::Result;
use serde::Serialize;
use tree_sitter::Node;

// ── introduce-parameter ────────────────────────────────────────────────────

#[derive(Serialize, Debug)]
pub struct IntroduceParameterOutput {
    pub parameter: String,
    pub expression: String,
    pub function: String,
    pub file: String,
    pub applied: bool,
}

/// Expression node types we accept for extraction.
const EXPRESSION_KINDS: &[&str] = &[
    "binary_operator",
    "unary_operator",
    "call",
    "attribute",
    "subscript",
    "identifier",
    "string",
    "integer",
    "float",
    "true",
    "false",
    "null",
    "ternary_expression",
    "parenthesized_expression",
    "array",
    "dictionary",
    "await_expression",
];

#[allow(clippy::too_many_arguments)]
pub fn introduce_parameter(
    file: &Path,
    line: usize,       // 1-based
    column: usize,     // 1-based
    end_column: usize, // 1-based
    name: &str,
    type_hint: Option<&str>,
    dry_run: bool,
    project_root: &Path,
) -> Result<IntroduceParameterOutput> {
    let source =
        std::fs::read_to_string(file).map_err(|e| miette::miette!("cannot read file: {e}"))?;
    let tree = crate::core::parser::parse(&source)?;
    let root = tree.root_node();

    let line_0 = line - 1;
    let col_0 = column - 1;
    let end_col_0 = end_column - 1;

    let start_point = tree_sitter::Point::new(line_0, col_0);
    let end_point = tree_sitter::Point::new(line_0, end_col_0);

    // Find expression at position
    let expr = find_expression_at(root, start_point, end_point)
        .ok_or_else(|| miette::miette!("no expression found at {line}:{column}-{end_column}"))?;

    let expr_text = expr
        .utf8_text(source.as_bytes())
        .map_err(|e| miette::miette!("cannot read expression: {e}"))?
        .to_string();

    // Find the enclosing function
    let func = crate::lsp::references::enclosing_function(root, start_point)
        .ok_or_else(|| miette::miette!("no enclosing function at line {line}"))?;

    let func_name = if func.kind() == "constructor_definition" {
        "_init".to_string()
    } else {
        func.child_by_field_name("name")
            .and_then(|n| n.utf8_text(source.as_bytes()).ok())
            .unwrap_or("unknown")
            .to_string()
    };

    // Build the parameter string: "name: Type = default" or "name = default"
    let param_text = if let Some(t) = type_hint {
        format!("{name}: {t} = {expr_text}")
    } else {
        format!("{name} = {expr_text}")
    };

    let relative_file = crate::core::fs::relative_slash(file, project_root);

    if !dry_run {
        let mut new_source = source.clone();

        // 1. Replace expression with parameter name in the body
        let expr_start = expr.start_byte();
        let expr_end = expr.end_byte();
        new_source.replace_range(expr_start..expr_end, name);

        // 2. Re-parse to get updated function node for parameter insertion
        let new_tree = crate::core::parser::parse(&new_source)?;
        let new_root = new_tree.root_node();

        // Find the function again
        let new_func = find_function_by_name(new_root, &new_source, &func_name)
            .ok_or_else(|| miette::miette!("cannot find function after edit"))?;

        // Find the parameters node
        if let Some(params_node) = new_func.child_by_field_name("parameters") {
            let params_start = params_node.start_byte();
            let params_end = params_node.end_byte();
            let old_params_text = &new_source[params_start..params_end];

            // Parse existing content between parens
            let inner = &old_params_text[1..old_params_text.len() - 1].trim();
            let new_params_text = if inner.is_empty() {
                format!("({param_text})")
            } else {
                format!("({inner}, {param_text})")
            };

            new_source.replace_range(params_start..params_end, &new_params_text);
        }

        std::fs::write(file, &new_source).map_err(|e| miette::miette!("cannot write file: {e}"))?;

        // Record undo
        let mut snaps: HashMap<PathBuf, Option<Vec<u8>>> = HashMap::new();
        snaps.insert(file.to_path_buf(), Some(source.as_bytes().to_vec()));
        let stack = super::undo::UndoStack::open(project_root);
        let _ = stack.record(
            "introduce-parameter",
            &format!("introduce param {name} in {func_name}"),
            &snaps,
            project_root,
        );
    }

    Ok(IntroduceParameterOutput {
        parameter: param_text,
        expression: expr_text,
        function: func_name,
        file: relative_file,
        applied: !dry_run,
    })
}

/// Walk up from the deepest node at the given range to find the first expression node.
fn find_expression_at(
    root: Node<'_>,
    start: tree_sitter::Point,
    end: tree_sitter::Point,
) -> Option<Node<'_>> {
    let node = root.descendant_for_point_range(start, end)?;
    let mut current = node;
    loop {
        if EXPRESSION_KINDS.contains(&current.kind()) {
            return Some(current);
        }
        current = current.parent()?;
    }
}

fn find_function_by_name<'a>(root: Node<'a>, source: &str, name: &str) -> Option<Node<'a>> {
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        if child.kind() == "constructor_definition" && name == "_init" {
            return Some(child);
        }
        if child.kind() == "function_definition"
            && let Some(n) = child.child_by_field_name("name")
            && n.utf8_text(source.as_bytes()).ok() == Some(name)
        {
            return Some(child);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn setup_project(files: &[(&str, &str)]) -> TempDir {
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

    #[test]
    fn introduce_param_literal() {
        let temp = setup_project(&[(
            "player.gd",
            "func move(delta):\n\tposition.x += 100.0 * delta\n",
        )]);
        // Select "100.0" on line 2
        // \tposition.x += 100.0 * delta
        //                 ^    ^ col 16 to 21 (1-based)
        let result = introduce_parameter(
            &temp.path().join("player.gd"),
            2,
            16,
            21,
            "speed",
            Some("float"),
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        assert_eq!(result.expression, "100.0");
        assert!(result.parameter.contains("speed: float = 100.0"));
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(
            content.contains("func move(delta, speed: float = 100.0)"),
            "should add parameter, got: {content}"
        );
        assert!(
            content.contains("position.x += speed * delta"),
            "should replace literal, got: {content}"
        );
    }

    #[test]
    fn introduce_param_no_type() {
        let temp = setup_project(&[("player.gd", "func greet():\n\tprint(\"hello\")\n")]);
        // Select "\"hello\"" on line 2
        let result = introduce_parameter(
            &temp.path().join("player.gd"),
            2,
            8,
            15,
            "msg",
            None,
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(
            content.contains("func greet(msg = \"hello\")"),
            "should add untyped param, got: {content}"
        );
        assert!(
            content.contains("print(msg)"),
            "should replace literal, got: {content}"
        );
    }

    #[test]
    fn introduce_param_dry_run() {
        let temp = setup_project(&[("player.gd", "func greet():\n\tprint(\"hello\")\n")]);
        let result = introduce_parameter(
            &temp.path().join("player.gd"),
            2,
            8,
            15,
            "msg",
            None,
            true,
            temp.path(),
        )
        .unwrap();
        assert!(!result.applied);
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(!content.contains("msg"), "dry run should not modify file");
    }
}
