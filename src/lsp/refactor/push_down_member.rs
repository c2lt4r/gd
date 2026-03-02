use std::path::Path;

use miette::Result;
use serde::Serialize;

use super::{
    DECLARATION_KINDS, declaration_full_range, declaration_kind_str, find_declaration_by_name,
    normalize_blank_lines,
};
use crate::core::gd_ast;
use crate::core::workspace_index::ProjectIndex;

// ── Output structs ──────────────────────────────────────────────────────────

#[derive(Serialize, Debug)]
pub struct PushDownMemberOutput {
    pub symbol: String,
    pub kind: String,
    pub from: String,
    pub targets: Vec<PushDownTarget>,
    pub applied: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

#[derive(Serialize, Debug)]
pub struct PushDownTarget {
    pub file: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skipped: Option<String>,
}

// ── Core implementation ─────────────────────────────────────────────────────

#[allow(clippy::too_many_lines)]
pub fn push_down_member(
    file: &Path,
    name: &str,
    to_files: &[String],
    force: bool,
    dry_run: bool,
    project_root: &Path,
) -> Result<PushDownMemberOutput> {
    let source =
        std::fs::read_to_string(file).map_err(|e| miette::miette!("cannot read file: {e}"))?;
    let tree = crate::core::parser::parse(&source)?;
    let root = tree.root_node();
    let gd_file = gd_ast::convert(&tree, &source);

    // Find the declaration
    let decl = find_declaration_by_name(&gd_file, name)
        .ok_or_else(|| miette::miette!("no declaration named '{name}' found"))?;

    let kind = declaration_kind_str(decl.kind()).to_string();
    let (start_byte, end_byte) = declaration_full_range(decl, &source);
    let decl_text = &source[start_byte..end_byte];
    let decl_text = if decl_text.ends_with('\n') {
        decl_text.to_string()
    } else {
        format!("{decl_text}\n")
    };

    let from_relative = crate::core::fs::relative_slash(file, project_root);

    // Resolve target files: either explicit --to or auto-discover children
    let resolved_targets = if to_files.is_empty() {
        discover_child_files(file, &source, project_root)?
    } else {
        to_files
            .iter()
            .map(|t| project_root.join(t))
            .collect::<Vec<_>>()
    };

    if resolved_targets.is_empty() {
        return Err(miette::miette!(
            "no child classes found that extend the parent in '{from_relative}'"
        ));
    }

    // Check for parent-internal references to this symbol
    let mut warnings = Vec::new();
    let internal_refs = count_internal_references(root, &source, name, decl);
    if internal_refs > 0 {
        warnings.push(format!(
            "{internal_refs} reference{} to '{name}' in parent file may break",
            if internal_refs == 1 { "" } else { "s" }
        ));
    }

    // Check each target for conflicts and build the target list
    let mut targets = Vec::new();
    for target_path in &resolved_targets {
        let target_relative = crate::core::fs::relative_slash(target_path, project_root);
        if !target_path.exists() {
            targets.push(PushDownTarget {
                file: target_relative,
                skipped: Some("file does not exist".to_string()),
            });
            continue;
        }
        let target_source = std::fs::read_to_string(target_path)
            .map_err(|e| miette::miette!("cannot read {target_relative}: {e}"))?;
        let target_tree = crate::core::parser::parse(&target_source)?;
        let target_file = gd_ast::convert(&target_tree, &target_source);

        if let Some(existing) = find_declaration_by_name(&target_file, name) {
            if force {
                targets.push(PushDownTarget {
                    file: target_relative,
                    skipped: Some(format!(
                        "already has '{}' {} (skipped, --force)",
                        name,
                        declaration_kind_str(existing.kind())
                    )),
                });
            } else {
                targets.push(PushDownTarget {
                    file: target_relative,
                    skipped: Some(format!(
                        "already has '{}' {}",
                        name,
                        declaration_kind_str(existing.kind())
                    )),
                });
            }
            continue;
        }

        targets.push(PushDownTarget {
            file: target_relative,
            skipped: None,
        });
    }

    // If all targets are skipped, error
    let actionable: Vec<_> = targets.iter().filter(|t| t.skipped.is_none()).collect();
    if actionable.is_empty() {
        return Err(miette::miette!(
            "all target files already contain '{name}' — nothing to push down"
        ));
    }

    if !dry_run {
        let mut tx = super::transaction::RefactorTransaction::new();

        // Insert the symbol into each non-skipped target
        for target in &targets {
            if target.skipped.is_some() {
                continue;
            }
            let target_path = project_root.join(&target.file);
            let mut target_source = std::fs::read_to_string(&target_path)
                .map_err(|e| miette::miette!("cannot read {}: {e}", target.file))?;

            let spacing = insertion_spacing(decl.kind(), &target_source);
            target_source.push_str(&spacing);
            target_source.push_str(&decl_text);

            super::validate_no_new_errors("", &target_source)?;
            tx.write_file(&target_path, &target_source)?;
        }

        // Remove from parent
        let mut new_source = String::with_capacity(source.len());
        new_source.push_str(&source[..start_byte]);
        new_source.push_str(&source[end_byte..]);
        normalize_blank_lines(&mut new_source);
        super::validate_no_new_errors(&source, &new_source)?;
        tx.write_file(file, &new_source)?;

        let target_names: Vec<&str> = actionable.iter().map(|t| t.file.as_str()).collect();
        let snapshots = tx.into_snapshots();
        let stack = super::undo::UndoStack::open(project_root);
        let _ = stack.record(
            "push-down-member",
            &format!(
                "push down {name} from {from_relative} to {}",
                target_names.join(", ")
            ),
            &snapshots,
            project_root,
        );
    }

    Ok(PushDownMemberOutput {
        symbol: name.to_string(),
        kind,
        from: from_relative,
        targets,
        applied: !dry_run,
        warnings,
    })
}

// ── Helpers ─────────────────────────────────────────────────────────────────

/// Discover all `.gd` files that directly extend the parent class.
///
/// A child file is one whose `extends` clause matches either:
/// - The parent's `class_name` (if declared), or
/// - The `res://` path to the parent file.
fn discover_child_files(
    parent_file: &Path,
    parent_source: &str,
    project_root: &Path,
) -> Result<Vec<std::path::PathBuf>> {
    let parent_tree = crate::core::parser::parse(parent_source)?;
    let parent_gd = gd_ast::convert(&parent_tree, parent_source);

    let index = ProjectIndex::build(project_root);
    let parent_relative = crate::core::fs::relative_slash(parent_file, project_root);
    let parent_res = format!("res://{parent_relative}");

    let mut children = Vec::new();
    for fs in index.files() {
        // Don't match the parent itself
        if fs.path == parent_file {
            continue;
        }

        let is_child = if let Some(ref extends) = fs.extends {
            // Match by class_name
            let by_name = parent_gd.class_name.is_some_and(|cn| extends == cn);
            // Also match by res:// path in extends (e.g., extends "res://base.gd")
            by_name || extends == &parent_res || extends == &parent_relative
        } else {
            false
        };

        if is_child {
            children.push(fs.path.clone());
        }
    }

    Ok(children)
}

/// Count references to `name` in the parent file outside the declaration itself.
fn count_internal_references(
    root: tree_sitter::Node,
    source: &str,
    name: &str,
    decl: tree_sitter::Node,
) -> usize {
    let decl_start = decl.start_byte();
    let decl_end = decl.end_byte();
    let mut count = 0;
    count_refs_recursive(root, source, name, decl_start, decl_end, &mut count);
    count
}

fn count_refs_recursive(
    node: tree_sitter::Node,
    source: &str,
    name: &str,
    decl_start: usize,
    decl_end: usize,
    count: &mut usize,
) {
    // Skip the entire declaration subtree
    if node.start_byte() >= decl_start && node.end_byte() <= decl_end {
        return;
    }

    // Count identifier references outside the declaration
    if node.kind() == "identifier"
        && (node.start_byte() < decl_start || node.start_byte() >= decl_end)
        && let Ok(text) = node.utf8_text(source.as_bytes())
        && text == name
        && !DECLARATION_KINDS.contains(&node.parent().map_or("", |p| p.kind()))
    {
        *count += 1;
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        count_refs_recursive(child, source, name, decl_start, decl_end, count);
    }
}

/// Determine blank-line spacing to add before inserting a declaration.
fn insertion_spacing(decl_kind: &str, target_source: &str) -> String {
    let trimmed = target_source.trim_end();
    if trimmed.is_empty() {
        return String::new();
    }

    let needs_extra = matches!(
        decl_kind,
        "function_definition" | "constructor_definition" | "class_definition"
    );

    let trailing_newlines = target_source.len() - trimmed.len();
    if needs_extra {
        if trailing_newlines >= 3 {
            String::new()
        } else {
            "\n".repeat(3 - trailing_newlines)
        }
    } else if trailing_newlines >= 2 {
        String::new()
    } else {
        "\n".repeat(2 - trailing_newlines)
    }
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
    fn push_down_function_to_explicit_targets() {
        let temp = setup_project(&[
            (
                "base.gd",
                "class_name Base\nextends Node\nvar keep = 1\n\n\nfunc helper():\n\tpass\n",
            ),
            ("child_a.gd", "extends Base\nvar x = 1\n"),
            ("child_b.gd", "extends Base\nvar y = 2\n"),
        ]);
        let result = push_down_member(
            &temp.path().join("base.gd"),
            "helper",
            &["child_a.gd".to_string(), "child_b.gd".to_string()],
            false,
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        assert_eq!(result.kind, "function");
        assert_eq!(result.targets.len(), 2);
        assert!(result.targets.iter().all(|t| t.skipped.is_none()));

        // Parent should no longer have the function
        let parent = fs::read_to_string(temp.path().join("base.gd")).unwrap();
        assert!(!parent.contains("func helper"));
        assert!(parent.contains("keep"));

        // Both children should have it
        let child_a = fs::read_to_string(temp.path().join("child_a.gd")).unwrap();
        assert!(child_a.contains("func helper()"));
        let child_b = fs::read_to_string(temp.path().join("child_b.gd")).unwrap();
        assert!(child_b.contains("func helper()"));
    }

    #[test]
    fn push_down_auto_discover_children() {
        let temp = setup_project(&[
            (
                "base.gd",
                "class_name Base\nextends Node\nvar keep = 1\n\n\nfunc helper():\n\tpass\n",
            ),
            ("child.gd", "class_name Child\nextends Base\nvar x = 1\n"),
            ("unrelated.gd", "extends Node\nvar z = 3\n"),
        ]);
        let result = push_down_member(
            &temp.path().join("base.gd"),
            "helper",
            &[],
            false,
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        assert_eq!(result.targets.len(), 1);
        assert_eq!(result.targets[0].file, "child.gd");

        let parent = fs::read_to_string(temp.path().join("base.gd")).unwrap();
        assert!(!parent.contains("func helper"));
        let child = fs::read_to_string(temp.path().join("child.gd")).unwrap();
        assert!(child.contains("func helper()"));
        // Unrelated file should be unchanged
        let unrelated = fs::read_to_string(temp.path().join("unrelated.gd")).unwrap();
        assert!(!unrelated.contains("helper"));
    }

    #[test]
    fn push_down_variable() {
        let temp = setup_project(&[
            (
                "base.gd",
                "class_name Base\nextends Node\nvar pushed = 10\nvar keep = 20\n",
            ),
            ("child.gd", "extends Base\nvar x = 1\n"),
        ]);
        let result = push_down_member(
            &temp.path().join("base.gd"),
            "pushed",
            &["child.gd".to_string()],
            false,
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        assert_eq!(result.kind, "variable");

        let parent = fs::read_to_string(temp.path().join("base.gd")).unwrap();
        assert!(!parent.contains("pushed"));
        assert!(parent.contains("keep"));
        let child = fs::read_to_string(temp.path().join("child.gd")).unwrap();
        assert!(child.contains("var pushed"));
    }

    #[test]
    fn push_down_signal() {
        let temp = setup_project(&[
            (
                "base.gd",
                "class_name Base\nextends Node\nsignal pushed\nsignal keep\n",
            ),
            ("child.gd", "extends Base\nvar x = 1\n"),
        ]);
        let result = push_down_member(
            &temp.path().join("base.gd"),
            "pushed",
            &["child.gd".to_string()],
            false,
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        assert_eq!(result.kind, "signal");

        let parent = fs::read_to_string(temp.path().join("base.gd")).unwrap();
        assert!(!parent.contains("signal pushed"));
        assert!(parent.contains("signal keep"));
    }

    #[test]
    fn push_down_with_doc_comments() {
        let temp = setup_project(&[
            (
                "base.gd",
                "class_name Base\nextends Node\n\n## Helper function\n## Does things\nfunc helper():\n\tpass\n",
            ),
            ("child.gd", "extends Base\nvar x = 1\n"),
        ]);
        let result = push_down_member(
            &temp.path().join("base.gd"),
            "helper",
            &["child.gd".to_string()],
            false,
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);

        let child = fs::read_to_string(temp.path().join("child.gd")).unwrap();
        assert!(child.contains("## Helper function"));
        assert!(child.contains("func helper()"));
    }

    #[test]
    fn push_down_skips_existing_symbol() {
        let temp = setup_project(&[
            (
                "base.gd",
                "class_name Base\nextends Node\nfunc helper():\n\tpass\n",
            ),
            ("child.gd", "extends Base\nfunc helper():\n\treturn 1\n"),
        ]);
        let result = push_down_member(
            &temp.path().join("base.gd"),
            "helper",
            &["child.gd".to_string()],
            false,
            false,
            temp.path(),
        );
        // Should error since the only target already has the symbol
        assert!(result.is_err());
    }

    #[test]
    fn push_down_force_skips_existing() {
        let temp = setup_project(&[
            (
                "base.gd",
                "class_name Base\nextends Node\nfunc helper():\n\tpass\n",
            ),
            ("child_a.gd", "extends Base\nfunc helper():\n\treturn 1\n"),
            ("child_b.gd", "extends Base\nvar x = 1\n"),
        ]);
        let result = push_down_member(
            &temp.path().join("base.gd"),
            "helper",
            &["child_a.gd".to_string(), "child_b.gd".to_string()],
            true,
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        // child_a should be skipped, child_b should get the function
        assert!(result.targets[0].skipped.is_some());
        assert!(result.targets[1].skipped.is_none());

        let child_b = fs::read_to_string(temp.path().join("child_b.gd")).unwrap();
        assert!(child_b.contains("func helper()"));
    }

    #[test]
    fn push_down_dry_run() {
        let temp = setup_project(&[
            (
                "base.gd",
                "class_name Base\nextends Node\nfunc helper():\n\tpass\n",
            ),
            ("child.gd", "extends Base\nvar x = 1\n"),
        ]);
        let result = push_down_member(
            &temp.path().join("base.gd"),
            "helper",
            &["child.gd".to_string()],
            false,
            true,
            temp.path(),
        )
        .unwrap();
        assert!(!result.applied);

        // Files should be unchanged
        let parent = fs::read_to_string(temp.path().join("base.gd")).unwrap();
        assert!(parent.contains("func helper"));
        let child = fs::read_to_string(temp.path().join("child.gd")).unwrap();
        assert!(!child.contains("helper"));
    }

    #[test]
    fn push_down_not_found() {
        let temp = setup_project(&[
            ("base.gd", "class_name Base\nextends Node\nvar x = 1\n"),
            ("child.gd", "extends Base\nvar y = 2\n"),
        ]);
        let result = push_down_member(
            &temp.path().join("base.gd"),
            "nonexistent",
            &["child.gd".to_string()],
            false,
            false,
            temp.path(),
        );
        assert!(result.is_err());
    }

    #[test]
    fn push_down_no_children_found() {
        let temp = setup_project(&[
            (
                "base.gd",
                "class_name Base\nextends Node\nfunc helper():\n\tpass\n",
            ),
            ("unrelated.gd", "extends Node\nvar z = 3\n"),
        ]);
        let result = push_down_member(
            &temp.path().join("base.gd"),
            "helper",
            &[],
            false,
            false,
            temp.path(),
        );
        assert!(result.is_err());
    }

    #[test]
    fn push_down_warns_on_internal_refs() {
        let temp = setup_project(&[
            (
                "base.gd",
                "class_name Base\nextends Node\nvar speed = 10\n\n\nfunc run():\n\tprint(speed)\n",
            ),
            ("child.gd", "extends Base\nvar x = 1\n"),
        ]);
        let result = push_down_member(
            &temp.path().join("base.gd"),
            "speed",
            &["child.gd".to_string()],
            false,
            true,
            temp.path(),
        )
        .unwrap();
        assert!(
            result.warnings.iter().any(|w| w.contains("reference")),
            "should warn about references, got: {:?}",
            result.warnings
        );
    }

    #[test]
    fn push_down_correct_spacing_function() {
        let temp = setup_project(&[
            (
                "base.gd",
                "class_name Base\nextends Node\nfunc helper():\n\tpass\n",
            ),
            ("child.gd", "extends Base\nvar x = 1\n"),
        ]);
        let _ = push_down_member(
            &temp.path().join("base.gd"),
            "helper",
            &["child.gd".to_string()],
            false,
            false,
            temp.path(),
        )
        .unwrap();
        let child = fs::read_to_string(temp.path().join("child.gd")).unwrap();
        // Functions should have 2 blank lines before them
        assert!(
            child.contains("\n\n\nfunc helper()"),
            "should have 2 blank lines before function, got: {child:?}"
        );
    }
}
