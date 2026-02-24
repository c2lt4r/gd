use std::collections::HashMap;
use std::path::{Path, PathBuf};

use miette::Result;
use serde::Serialize;
use tree_sitter::Node;

use super::invert_if::node_text;

// ── Output ──────────────────────────────────────────────────────────────────

#[derive(Serialize, Debug)]
pub struct ConvertNodePathOutput {
    pub original: String,
    pub converted: String,
    pub direction: String,
    pub file: String,
    pub line: u32,
    pub applied: bool,
}

// ── Direction ───────────────────────────────────────────────────────────────

enum Direction {
    /// `$Path` → `get_node("Path")`
    ToCall,
    /// `get_node("Path")` → `$Path`
    ToDollar,
}

// ── Public entry point ──────────────────────────────────────────────────────

pub fn convert_node_path(
    file: &Path,
    line: usize,   // 1-based
    column: usize,  // 1-based
    dry_run: bool,
    project_root: &Path,
) -> Result<ConvertNodePathOutput> {
    let source =
        std::fs::read_to_string(file).map_err(|e| miette::miette!("cannot read file: {e}"))?;
    let tree = crate::core::parser::parse(&source)?;
    let root = tree.root_node();

    let point = tree_sitter::Point::new(line - 1, column - 1);
    let (target_node, direction) = find_get_node_or_call(root, point, &source)
        .ok_or_else(|| miette::miette!("no $Path or get_node() call found at {line}:{column}"))?;

    let original_text = node_text(&target_node, &source);

    let (raw_path, converted) = match direction {
        Direction::ToCall => {
            let path = extract_dollar_path(&original_text);
            let call = format!("get_node(\"{path}\")");
            (path, call)
        }
        Direction::ToDollar => {
            let path = extract_get_node_arg(&target_node, &source)?;
            let dollar = if needs_quoting(&path) {
                format!("$\"{path}\"")
            } else {
                format!("${path}")
            };
            (path, dollar)
        }
    };

    let _ = raw_path; // used only for the conversion above

    let relative_file = crate::core::fs::relative_slash(file, project_root);
    let direction_str = match direction {
        Direction::ToCall => "to-call",
        Direction::ToDollar => "to-dollar",
    };

    if dry_run {
        return Ok(ConvertNodePathOutput {
            original: original_text,
            converted,
            direction: direction_str.to_string(),
            file: relative_file,
            line: line as u32,
            applied: false,
        });
    }

    // Apply the replacement
    let mut new_source = String::with_capacity(source.len());
    new_source.push_str(&source[..target_node.start_byte()]);
    new_source.push_str(&converted);
    new_source.push_str(&source[target_node.end_byte()..]);

    super::validate_no_new_errors(&source, &new_source)?;
    std::fs::write(file, &new_source).map_err(|e| miette::miette!("cannot write file: {e}"))?;

    // Record undo
    let mut snaps: HashMap<PathBuf, Option<Vec<u8>>> = HashMap::new();
    snaps.insert(file.to_path_buf(), Some(source.as_bytes().to_vec()));
    let stack = super::undo::UndoStack::open(project_root);
    let _ = stack.record(
        "convert-node-path",
        &format!("convert {original_text} → {converted}"),
        &snaps,
        project_root,
    );

    Ok(ConvertNodePathOutput {
        original: original_text,
        converted,
        direction: direction_str.to_string(),
        file: relative_file,
        line: line as u32,
        applied: true,
    })
}

// ── Node finder ─────────────────────────────────────────────────────────────

/// Walk from the deepest node at `point` upward to find either:
/// - A `"get_node"` node (the `$` shorthand) → `Direction::ToCall`
/// - A `"call"` node whose callee is `get_node` with a single string arg → `Direction::ToDollar`
fn find_get_node_or_call<'a>(
    root: Node<'a>,
    point: tree_sitter::Point,
    source: &str,
) -> Option<(Node<'a>, Direction)> {
    let mut node = root.descendant_for_point_range(point, point)?;

    loop {
        if node.kind() == "get_node" {
            return Some((node, Direction::ToCall));
        }
        if node.kind() == "call" && is_get_node_call(&node, source) {
            return Some((node, Direction::ToDollar));
        }
        node = node.parent()?;
    }
}

/// Check if a `call` node is `get_node("some_string_literal")`.
fn is_get_node_call(call_node: &Node, source: &str) -> bool {
    let mut cursor = call_node.walk();
    let children: Vec<Node> = call_node.children(&mut cursor).collect();

    // First child should be identifier "get_node"
    let Some(callee) = children.first() else {
        return false;
    };
    if callee.kind() != "identifier" || node_text(callee, source) != "get_node" {
        return false;
    }

    // Should have an arguments node with exactly one string argument
    let Some(args) = children.iter().find(|c| c.kind() == "arguments") else {
        return false;
    };
    let mut arg_cursor = args.walk();
    let arg_children: Vec<Node> = args
        .children(&mut arg_cursor)
        .filter(Node::is_named)
        .collect();
    arg_children.len() == 1 && arg_children[0].kind() == "string"
}

// ── Path extraction helpers ─────────────────────────────────────────────────

/// Extract the raw path from `$Sprite2D` or `$"Player/Sprite2D"`.
fn extract_dollar_path(text: &str) -> String {
    let without_dollar = text.strip_prefix('$').unwrap_or(text);
    // Strip surrounding quotes if present
    without_dollar
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .unwrap_or(without_dollar)
        .to_string()
}

/// Extract the string argument from a `get_node("path")` call node.
fn extract_get_node_arg(call_node: &Node, source: &str) -> Result<String> {
    let mut cursor = call_node.walk();
    let args_node = call_node
        .children(&mut cursor)
        .find(|c| c.kind() == "arguments")
        .ok_or_else(|| miette::miette!("get_node() call has no arguments"))?;

    let mut arg_cursor = args_node.walk();
    let string_node = args_node
        .children(&mut arg_cursor)
        .find(|c| c.is_named() && c.kind() == "string")
        .ok_or_else(|| miette::miette!("get_node() argument is not a string literal"))?;

    let raw = node_text(&string_node, source);
    // Strip surrounding quotes: "path" → path
    Ok(raw
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .unwrap_or(&raw)
        .to_string())
}

/// Whether a node path needs quoting in `$"..."` form.
/// Simple identifiers like `Sprite2D` don't need quotes.
/// Paths with `/`, `..`, `-`, `%`, `@`, or spaces do.
fn needs_quoting(path: &str) -> bool {
    path.contains('/')
        || path.contains('.')
        || path.contains('-')
        || path.contains(' ')
        || path.contains('%')
        || path.contains('@')
}

// ── Tests ───────────────────────────────────────────────────────────────────

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
    fn dollar_to_get_node_simple() {
        let temp = setup_project(&[(
            "test.gd",
            "func foo():\n\tvar node = $Sprite2D\n",
        )]);
        let result = convert_node_path(&temp.path().join("test.gd"), 2, 13, false, temp.path()).unwrap();
        assert!(result.applied);
        assert_eq!(result.direction, "to-call");
        assert_eq!(result.converted, "get_node(\"Sprite2D\")");
        let content = fs::read_to_string(temp.path().join("test.gd")).unwrap();
        assert!(
            content.contains("get_node(\"Sprite2D\")"),
            "should convert $ to get_node(), got:\n{content}"
        );
    }

    #[test]
    fn dollar_to_get_node_quoted() {
        let temp = setup_project(&[(
            "test.gd",
            "func foo():\n\tvar node = $\"Player/Sprite2D\"\n",
        )]);
        let result = convert_node_path(&temp.path().join("test.gd"), 2, 13, false, temp.path()).unwrap();
        assert!(result.applied);
        assert_eq!(result.direction, "to-call");
        assert_eq!(result.converted, "get_node(\"Player/Sprite2D\")");
        let content = fs::read_to_string(temp.path().join("test.gd")).unwrap();
        assert!(
            content.contains("get_node(\"Player/Sprite2D\")"),
            "should convert $\"...\" to get_node(), got:\n{content}"
        );
    }

    #[test]
    fn get_node_to_dollar_simple() {
        let temp = setup_project(&[(
            "test.gd",
            "func foo():\n\tvar node = get_node(\"Sprite2D\")\n",
        )]);
        let result = convert_node_path(&temp.path().join("test.gd"), 2, 13, false, temp.path()).unwrap();
        assert!(result.applied);
        assert_eq!(result.direction, "to-dollar");
        assert_eq!(result.converted, "$Sprite2D");
        let content = fs::read_to_string(temp.path().join("test.gd")).unwrap();
        assert!(
            content.contains("$Sprite2D"),
            "should convert get_node() to $, got:\n{content}"
        );
    }

    #[test]
    fn get_node_to_dollar_path() {
        let temp = setup_project(&[(
            "test.gd",
            "func foo():\n\tvar node = get_node(\"Player/Sprite2D\")\n",
        )]);
        let result = convert_node_path(&temp.path().join("test.gd"), 2, 13, false, temp.path()).unwrap();
        assert!(result.applied);
        assert_eq!(result.direction, "to-dollar");
        assert_eq!(result.converted, "$\"Player/Sprite2D\"");
        let content = fs::read_to_string(temp.path().join("test.gd")).unwrap();
        assert!(
            content.contains("$\"Player/Sprite2D\""),
            "should convert get_node() to $\"...\" for paths, got:\n{content}"
        );
    }

    #[test]
    fn get_node_to_dollar_relative() {
        let temp = setup_project(&[(
            "test.gd",
            "func foo():\n\tvar node = get_node(\"../Sibling\")\n",
        )]);
        let result = convert_node_path(&temp.path().join("test.gd"), 2, 13, false, temp.path()).unwrap();
        assert!(result.applied);
        assert_eq!(result.direction, "to-dollar");
        assert_eq!(result.converted, "$\"../Sibling\"");
        let content = fs::read_to_string(temp.path().join("test.gd")).unwrap();
        assert!(
            content.contains("$\"../Sibling\""),
            "should convert get_node() to $\"...\" for relative paths, got:\n{content}"
        );
    }

    #[test]
    fn dry_run_no_modify() {
        let original = "func foo():\n\tvar node = $Sprite2D\n";
        let temp = setup_project(&[("test.gd", original)]);
        let result = convert_node_path(&temp.path().join("test.gd"), 2, 13, true, temp.path()).unwrap();
        assert!(!result.applied);
        assert_eq!(result.direction, "to-call");
        assert_eq!(result.converted, "get_node(\"Sprite2D\")");
        let content = fs::read_to_string(temp.path().join("test.gd")).unwrap();
        assert_eq!(content, original, "dry run should not modify file");
    }
}
