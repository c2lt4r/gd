use std::collections::HashMap;
use std::path::{Path, PathBuf};

use miette::Result;
use serde::Serialize;
use tree_sitter::Node;

use gd_core::gd_ast;

use super::invert_if::{get_line_indent, negate_condition, node_text};

// ── Output structs ──────────────────────────────────────────────────────────

#[derive(Serialize, Debug)]
pub struct ExtractGuardsOutput {
    pub function: String,
    pub file: String,
    pub guards: Vec<GuardClause>,
    pub applied: bool,
}

#[derive(Serialize, Debug)]
pub struct GuardClause {
    pub original_condition: String,
    pub negated_condition: String,
    pub exit_keyword: String,
}

// ── Guard chain finding ─────────────────────────────────────────────────────

struct GuardCandidate {
    /// Statements before the trailing if (text lines, at their original indent)
    prefix_lines: Vec<String>,
    /// The original condition text
    original_condition: String,
    /// The negated condition text
    negated_condition: String,
    /// "return" or "continue"
    exit_keyword: String,
}

struct GuardChainResult {
    guards: Vec<GuardCandidate>,
    /// The innermost body statements (fully flattened)
    tail_lines: Vec<String>,
}

/// Recursively find a chain of guardable trailing ifs.
fn find_guard_chain(stmts: &[Node], source: &str, exit_kw: &str) -> GuardChainResult {
    if stmts.is_empty() {
        return GuardChainResult {
            guards: vec![],
            tail_lines: vec![],
        };
    }

    let last = &stmts[stmts.len() - 1];

    // Only guardable if last stmt is an if_statement without else/elif
    if last.kind() != "if_statement" || !is_guardable_if(last) {
        // No more guards — return all statements as tail
        let tail_lines = stmts.iter().map(|s| node_text(s, source)).collect();
        return GuardChainResult {
            guards: vec![],
            tail_lines,
        };
    }

    // Collect prefix (everything before the last if)
    let prefix_lines: Vec<String> = stmts[..stmts.len() - 1]
        .iter()
        .map(|s| node_text(s, source))
        .collect();

    // Extract condition
    let Some(cond) = find_condition(last, source) else {
        let tail_lines = stmts.iter().map(|s| node_text(s, source)).collect();
        return GuardChainResult {
            guards: vec![],
            tail_lines,
        };
    };
    let original_condition = node_text(&cond, source);
    let negated_condition = negate_condition(&cond, source);

    // Get the if body's children
    let Some(if_body) = find_body_node(last) else {
        let tail_lines = stmts.iter().map(|s| node_text(s, source)).collect();
        return GuardChainResult {
            guards: vec![],
            tail_lines,
        };
    };
    let inner_stmts: Vec<Node> = collect_statements(&if_body);

    let guard = GuardCandidate {
        prefix_lines,
        original_condition,
        negated_condition,
        exit_keyword: exit_kw.to_string(),
    };

    // Recurse into the if body
    let mut inner = find_guard_chain(&inner_stmts, source, exit_kw);
    inner.guards.insert(0, guard);
    inner
}

/// An if is guardable if it has no elif or else clauses.
fn is_guardable_if(if_node: &Node) -> bool {
    let mut cursor = if_node.walk();
    for child in if_node.children(&mut cursor) {
        if matches!(child.kind(), "elif_clause" | "else_clause") {
            return false;
        }
    }
    true
}

/// Find the condition node of an if_statement.
fn find_condition<'a>(if_node: &Node<'a>, source: &str) -> Option<Node<'a>> {
    let mut cursor = if_node.walk();
    if_node.children(&mut cursor).find(|child| {
        child.is_named()
            && !matches!(
                child.kind(),
                "body" | "elif_clause" | "else_clause" | "comment"
            )
            && !node_text(child, source).is_empty()
    })
}

/// Find the body node inside an if_statement or loop.
fn find_body_node<'a>(node: &Node<'a>) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .find(|child| child.kind() == "body")
}

/// Collect named non-comment children of a body node.
fn collect_statements<'a>(body: &Node<'a>) -> Vec<Node<'a>> {
    let mut cursor = body.walk();
    body.children(&mut cursor)
        .filter(|c| c.is_named() && c.kind() != "comment")
        .collect()
}

// ── Main entry point ────────────────────────────────────────────────────────

pub fn extract_guards(
    file: &Path,
    function_name: &str,
    dry_run: bool,
    project_root: &Path,
) -> Result<ExtractGuardsOutput> {
    let source =
        std::fs::read_to_string(file).map_err(|e| miette::miette!("cannot read file: {e}"))?;
    let tree = gd_core::parser::parse(&source)?;
    let gd_file = gd_ast::convert(&tree, &source);

    let func_node = super::find_declaration_by_name(&gd_file, function_name)
        .ok_or_else(|| miette::miette!("function '{function_name}' not found"))?;

    if !matches!(
        func_node.kind(),
        "function_definition" | "constructor_definition"
    ) {
        return Err(miette::miette!("'{function_name}' is not a function"));
    }

    let func_body =
        find_body_node(&func_node).ok_or_else(|| miette::miette!("cannot find function body"))?;
    let body_stmts = collect_statements(&func_body);

    if body_stmts.is_empty() {
        return Err(miette::miette!("function body is empty"));
    }

    // Detect loop wrapper: if body is a single for/while, flatten its body with "continue"
    let (stmts_to_flatten, exit_kw, loop_wrapper) = if body_stmts.len() == 1
        && matches!(body_stmts[0].kind(), "for_statement" | "while_statement")
    {
        let loop_node = &body_stmts[0];
        let loop_body =
            find_body_node(loop_node).ok_or_else(|| miette::miette!("cannot find loop body"))?;
        let loop_stmts = collect_statements(&loop_body);
        (loop_stmts, "continue", Some(*loop_node))
    } else {
        (body_stmts, "return", None)
    };

    let chain = find_guard_chain(&stmts_to_flatten, &source, exit_kw);

    if chain.guards.is_empty() {
        return Err(miette::miette!(
            "no guard clause candidates found in '{function_name}'"
        ));
    }

    let relative_file = gd_core::fs::relative_slash(file, project_root);

    let guards: Vec<GuardClause> = chain
        .guards
        .iter()
        .map(|g| GuardClause {
            original_condition: g.original_condition.clone(),
            negated_condition: g.negated_condition.clone(),
            exit_keyword: g.exit_keyword.clone(),
        })
        .collect();

    if dry_run {
        return Ok(ExtractGuardsOutput {
            function: function_name.to_string(),
            file: relative_file,
            guards,
            applied: false,
        });
    }

    // Build the new body text
    let new_body = build_new_body(&chain.guards, &chain.tail_lines, exit_kw);

    // Determine indentation
    let body_indent = if let Some(ref _loop_node) = loop_wrapper {
        // Inside a loop, body is indented 2 levels from function
        let func_indent = get_line_indent(&source, func_node.start_position().row);
        format!("{func_indent}\t\t")
    } else {
        let func_indent = get_line_indent(&source, func_node.start_position().row);
        format!("{func_indent}\t")
    };

    let re_indented = re_indent_block(&new_body, &body_indent);

    // Splice: replace the body contents
    let new_source = if let Some(loop_node) = &loop_wrapper {
        // Replace the loop's body contents
        let loop_body = find_body_node(loop_node).unwrap();
        splice_body_contents(&source, &loop_body, &re_indented)
    } else {
        splice_body_contents(&source, &func_body, &re_indented)
    };

    super::validate_no_new_errors(&source, &new_source)?;
    std::fs::write(file, &new_source).map_err(|e| miette::miette!("cannot write file: {e}"))?;

    let mut snaps: HashMap<PathBuf, Option<Vec<u8>>> = HashMap::new();
    snaps.insert(file.to_path_buf(), Some(source.as_bytes().to_vec()));
    let stack = super::undo::UndoStack::open(project_root);
    let _ = stack.record(
        "extract-guards",
        &format!("extract guards in {function_name}"),
        &snaps,
        project_root,
    );

    Ok(ExtractGuardsOutput {
        function: function_name.to_string(),
        file: relative_file,
        guards,
        applied: true,
    })
}

// ── Body building ───────────────────────────────────────────────────────────

/// Build the new flattened body from guard chain + tail.
fn build_new_body(guards: &[GuardCandidate], tail_lines: &[String], exit_kw: &str) -> Vec<String> {
    let mut lines = Vec::new();

    for guard in guards {
        // Emit prefix statements (dedented to body level)
        for prefix in &guard.prefix_lines {
            lines.push(prefix.clone());
        }
        // Emit guard clause
        lines.push(format!("if {}:", guard.negated_condition));
        lines.push(format!("\t{exit_kw}"));
    }

    // Emit remaining tail (innermost body)
    for line in tail_lines {
        lines.push(line.clone());
    }

    lines
}

/// Re-indent a block of lines to the target indent prefix.
/// Strips the minimum indentation first, then prepends the target.
fn re_indent_block(lines: &[String], indent: &str) -> String {
    let min_indent = lines
        .iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let trimmed = l.trim_start();
            l.len() - trimmed.len()
        })
        .min()
        .unwrap_or(0);

    lines
        .iter()
        .map(|line| {
            if line.trim().is_empty() {
                String::new()
            } else if line.len() >= min_indent {
                format!("{indent}{}", &line[min_indent..])
            } else {
                format!("{indent}{line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Replace the contents of a body node (the part after the leading newline).
fn splice_body_contents(source: &str, body_node: &Node, new_contents: &str) -> String {
    // Body node starts with a newline then the indented content.
    // We want to replace from body_start+1 (skip the newline) to body_end.
    let body_start = body_node.start_byte();
    let body_end = body_node.end_byte();

    // The body text starts with \n — skip it
    let content_start = if body_start < source.len() && source.as_bytes()[body_start] == b'\n' {
        body_start + 1
    } else {
        body_start
    };

    let mut out = String::with_capacity(source.len());
    out.push_str(&source[..content_start]);
    out.push_str(new_contents);
    out.push_str(&source[body_end..]);
    out
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
    fn basic_three_level_guards() {
        let temp = setup_project(&[(
            "test.gd",
            "func process(delta):\n\tif is_alive:\n\t\tif has_target:\n\t\t\tif can_attack:\n\t\t\t\tdo_attack()\n",
        )]);
        let result =
            extract_guards(&temp.path().join("test.gd"), "process", false, temp.path()).unwrap();
        assert!(result.applied);
        assert_eq!(result.guards.len(), 3);
        assert_eq!(result.guards[0].original_condition, "is_alive");
        assert_eq!(result.guards[0].negated_condition, "not is_alive");
        assert_eq!(result.guards[0].exit_keyword, "return");
        assert_eq!(result.guards[1].original_condition, "has_target");
        assert_eq!(result.guards[2].original_condition, "can_attack");

        let content = fs::read_to_string(temp.path().join("test.gd")).unwrap();
        assert!(
            content.contains("if not is_alive:"),
            "should have guard, got:\n{content}"
        );
        assert!(
            content.contains("\treturn"),
            "should have return, got:\n{content}"
        );
        assert!(
            content.contains("do_attack()"),
            "should have flattened body, got:\n{content}"
        );
        // Should NOT have deeply nested structure anymore
        assert!(
            !content.contains("\t\t\t\tdo_attack()"),
            "should be flattened, got:\n{content}"
        );
    }

    #[test]
    fn mixed_prefix_and_guards() {
        let temp = setup_project(&[(
            "test.gd",
            "func foo():\n\tsetup()\n\tif is_alive:\n\t\tprepare()\n\t\tif has_target:\n\t\t\tdo_attack()\n",
        )]);
        let result =
            extract_guards(&temp.path().join("test.gd"), "foo", false, temp.path()).unwrap();
        assert!(result.applied);
        assert_eq!(result.guards.len(), 2);

        let content = fs::read_to_string(temp.path().join("test.gd")).unwrap();
        // setup() should stay at top
        assert!(
            content.contains("\tsetup()"),
            "setup should stay, got:\n{content}"
        );
        // prepare() should come after the first guard
        assert!(
            content.contains("\tprepare()"),
            "prepare should be at body level, got:\n{content}"
        );
        assert!(
            content.contains("\tdo_attack()"),
            "do_attack should be at body level, got:\n{content}"
        );
    }

    #[test]
    fn loop_body_uses_continue() {
        let temp = setup_project(&[(
            "test.gd",
            "func process(delta):\n\tfor enemy in enemies:\n\t\tif enemy.is_alive:\n\t\t\tif enemy.in_range:\n\t\t\t\tattack(enemy)\n",
        )]);
        let result =
            extract_guards(&temp.path().join("test.gd"), "process", false, temp.path()).unwrap();
        assert!(result.applied);
        assert_eq!(result.guards.len(), 2);
        assert_eq!(result.guards[0].exit_keyword, "continue");
        assert_eq!(result.guards[1].exit_keyword, "continue");

        let content = fs::read_to_string(temp.path().join("test.gd")).unwrap();
        assert!(
            content.contains("continue"),
            "should use continue in loop, got:\n{content}"
        );
        assert!(
            !content.contains("\treturn"),
            "should not use return in loop, got:\n{content}"
        );
    }

    #[test]
    fn dry_run_no_changes() {
        let original = "func foo():\n\tif cond:\n\t\tdo_thing()\n";
        let temp = setup_project(&[("test.gd", original)]);
        let result =
            extract_guards(&temp.path().join("test.gd"), "foo", true, temp.path()).unwrap();
        assert!(!result.applied);
        assert_eq!(result.guards.len(), 1);
        let content = fs::read_to_string(temp.path().join("test.gd")).unwrap();
        assert_eq!(content, original, "dry run should not modify file");
    }

    #[test]
    fn no_guard_candidates_errors() {
        let temp = setup_project(&[("test.gd", "func foo():\n\tdo_thing()\n\tdo_other()\n")]);
        let result = extract_guards(&temp.path().join("test.gd"), "foo", false, temp.path());
        assert!(result.is_err());
    }

    #[test]
    fn if_with_else_not_extracted() {
        let temp = setup_project(&[(
            "test.gd",
            "func foo():\n\tif cond:\n\t\tdo_a()\n\telse:\n\t\tdo_b()\n",
        )]);
        let result = extract_guards(&temp.path().join("test.gd"), "foo", false, temp.path());
        assert!(result.is_err());
    }

    #[test]
    fn single_if_wrapping_body() {
        let temp = setup_project(&[(
            "test.gd",
            "func foo():\n\tif is_ready:\n\t\tdo_thing()\n\t\tdo_other()\n",
        )]);
        let result =
            extract_guards(&temp.path().join("test.gd"), "foo", false, temp.path()).unwrap();
        assert!(result.applied);
        assert_eq!(result.guards.len(), 1);

        let content = fs::read_to_string(temp.path().join("test.gd")).unwrap();
        assert!(
            content.contains("if not is_ready:"),
            "should have guard, got:\n{content}"
        );
        assert!(
            content.contains("\tdo_thing()"),
            "body should be flattened, got:\n{content}"
        );
        assert!(
            content.contains("\tdo_other()"),
            "body should be flattened, got:\n{content}"
        );
    }

    #[test]
    fn function_not_found() {
        let temp = setup_project(&[("test.gd", "func foo():\n\tpass\n")]);
        let result = extract_guards(
            &temp.path().join("test.gd"),
            "nonexistent",
            false,
            temp.path(),
        );
        assert!(result.is_err());
    }

    #[test]
    fn not_a_function() {
        let temp = setup_project(&[("test.gd", "var speed = 10\n")]);
        let result = extract_guards(&temp.path().join("test.gd"), "speed", false, temp.path());
        assert!(result.is_err());
    }

    #[test]
    fn comparison_negation_in_guard() {
        let temp = setup_project(&[(
            "test.gd",
            "func foo(x):\n\tif x > 0:\n\t\tif x < 100:\n\t\t\tprocess(x)\n",
        )]);
        let result =
            extract_guards(&temp.path().join("test.gd"), "foo", false, temp.path()).unwrap();
        assert!(result.applied);
        assert_eq!(result.guards[0].negated_condition, "x <= 0");
        assert_eq!(result.guards[1].negated_condition, "x >= 100");
    }

    #[test]
    fn stops_at_if_with_elif() {
        let temp = setup_project(&[(
            "test.gd",
            "func foo():\n\tif is_alive:\n\t\tif state == IDLE:\n\t\t\tidle()\n\t\telif state == RUN:\n\t\t\trun()\n",
        )]);
        let result =
            extract_guards(&temp.path().join("test.gd"), "foo", false, temp.path()).unwrap();
        assert!(result.applied);
        // Should only extract the outer `if is_alive`, not the elif chain
        assert_eq!(result.guards.len(), 1);
        assert_eq!(result.guards[0].original_condition, "is_alive");
    }
}
