use std::path::Path;

use miette::Result;
use serde::Serialize;
use tree_sitter::Node;

use super::{
    DECLARATION_KINDS, MoveSymbolOutput, PreloadRef, declaration_full_range, declaration_kind_str,
    find_class_definition, find_declaration_by_name, find_declaration_in_class,
    get_declaration_name, normalize_blank_lines, re_indent_to_depth,
};

#[allow(clippy::too_many_lines)]
pub fn move_symbol(
    name: &str,
    from_file: &Path,
    to_file: &Path,
    dry_run: bool,
    project_root: &Path,
    class: Option<&str>,
    target_class: Option<&str>,
) -> Result<MoveSymbolOutput> {
    let source = std::fs::read_to_string(from_file)
        .map_err(|e| miette::miette!("cannot read source file: {e}"))?;
    let tree = crate::core::parser::parse(&source)?;
    let root = tree.root_node();

    // Find the declaration (possibly within a class)
    let decl = if let Some(class_name) = class {
        let class_node = find_class_definition(root, &source, class_name)
            .ok_or_else(|| miette::miette!("no inner class named '{class_name}' found"))?;
        find_declaration_in_class(class_node, &source, name).ok_or_else(|| {
            miette::miette!("no declaration named '{name}' found in class '{class_name}'")
        })?
    } else {
        find_declaration_by_name(root, &source, name)
            .ok_or_else(|| miette::miette!("no declaration named '{name}' found at top level"))?
    };

    let kind = declaration_kind_str(decl.kind()).to_string();

    let (start_byte, end_byte) = declaration_full_range(decl, &source);
    let decl_text = &source[start_byte..end_byte];
    let decl_text = if decl_text.ends_with('\n') {
        decl_text.to_string()
    } else {
        format!("{decl_text}\n")
    };

    // Re-indent if moving between scope levels
    let decl_text = if class.is_some() && target_class.is_none() {
        // Moving out of a class to top-level: strip one indent level
        let re = re_indent_to_depth(&decl_text, 0);
        if re.ends_with('\n') {
            re
        } else {
            format!("{re}\n")
        }
    } else if class.is_none() && target_class.is_some() {
        // Moving from top-level into a class: add one indent level
        let re = re_indent_to_depth(&decl_text, 1);
        if re.ends_with('\n') {
            re
        } else {
            format!("{re}\n")
        }
    } else {
        decl_text
    };

    // Check target for duplicate
    if to_file.exists() {
        let target_source = std::fs::read_to_string(to_file)
            .map_err(|e| miette::miette!("cannot read target file: {e}"))?;
        let target_tree = crate::core::parser::parse(&target_source)?;
        let target_root = target_tree.root_node();

        let dup = if let Some(tc) = target_class {
            find_class_definition(target_root, &target_source, tc)
                .and_then(|c| find_declaration_in_class(c, &target_source, name))
        } else {
            find_declaration_by_name(target_root, &target_source, name)
        };
        if dup.is_some() {
            return Err(miette::miette!(
                "target already contains a declaration named '{name}'"
            ));
        }
    }

    // Find references for warnings
    let workspace = crate::lsp::workspace::WorkspaceIndex::new(project_root.to_path_buf());
    let class_filter = class;
    let all_refs =
        crate::lsp::references::find_references_by_name(name, &workspace, None, class_filter);

    let file_uri = tower_lsp::lsp_types::Url::from_file_path(from_file).ok();
    let decl_start_line = decl.start_position().row as u32;
    let decl_end_line = decl.end_position().row as u32;

    let external_count = all_refs
        .iter()
        .filter(|loc| {
            if let Some(ref uri) = file_uri
                && &loc.uri == uri
            {
                let ref_line = loc.range.start.line;
                if ref_line >= decl_start_line && ref_line <= decl_end_line {
                    return false;
                }
            }
            true
        })
        .count();

    let mut warnings = Vec::new();
    if external_count > 0 {
        warnings.push(format!(
            "{external_count} reference{} to '{name}' may need updating",
            if external_count == 1 { "" } else { "s" }
        ));
    }

    // Self-reference warnings when moving between classes
    if target_class.is_some() || class.is_some() {
        let self_refs = collect_self_references(decl, &source);
        if !self_refs.is_empty() && to_file.exists() {
            let target_source = std::fs::read_to_string(to_file)
                .map_err(|e| miette::miette!("cannot read target file: {e}"))?;
            let target_tree = crate::core::parser::parse(&target_source)?;
            let target_root = target_tree.root_node();

            let target_scope = if let Some(tc) = target_class {
                find_class_definition(target_root, &target_source, tc)
            } else {
                Some(target_root)
            };

            if let Some(scope) = target_scope {
                for member in &self_refs {
                    if !class_has_member(scope, &target_source, member) {
                        warnings.push(format!(
                            "self.{member} referenced but '{member}' not found in target"
                        ));
                    }
                }
            }
        }
    }

    let from_relative = crate::core::fs::relative_slash(from_file, project_root);
    let to_relative = crate::core::fs::relative_slash(to_file, project_root);

    if !dry_run {
        let mut tx = super::transaction::RefactorTransaction::new();

        // Write target file
        if to_file.exists() {
            let mut target_source = std::fs::read_to_string(to_file)
                .map_err(|e| miette::miette!("cannot read target file: {e}"))?;

            if let Some(tc) = target_class {
                // Insert into target class body
                let target_tree = crate::core::parser::parse(&target_source)?;
                let target_root = target_tree.root_node();
                let tc_node =
                    find_class_definition(target_root, &target_source, tc).ok_or_else(|| {
                        miette::miette!("target class '{tc}' not found in target file")
                    })?;
                let insert_byte = tc_node.end_byte();
                // Insert before end of class with proper spacing
                let spacing = "\n";
                let insert_text = format!("{spacing}{decl_text}");
                target_source.insert_str(insert_byte, &insert_text);
            } else {
                let spacing = insertion_spacing(decl.kind(), &target_source);
                target_source.push_str(&spacing);
                target_source.push_str(&decl_text);
            }
            tx.write_file(to_file, &target_source)?;
        } else {
            tx.write_file(to_file, &decl_text)?;
        }

        // Remove from source file
        let mut new_source = String::with_capacity(source.len());
        new_source.push_str(&source[..start_byte]);
        new_source.push_str(&source[end_byte..]);
        normalize_blank_lines(&mut new_source);
        tx.write_file(from_file, &new_source)?;

        let snapshots = tx.into_snapshots();
        let stack = super::undo::UndoStack::open(project_root);
        let _ = stack.record(
            "move-symbol",
            &format!("move {name} from {from_relative} to {to_relative}"),
            &snapshots,
            project_root,
        );
    }

    // Detect preload/load references to the source file
    let from_res = format!("res://{from_relative}");
    let preloads = find_preloads_to_file(&from_res, &workspace, project_root);

    Ok(MoveSymbolOutput {
        symbol: name.to_string(),
        kind,
        from: from_relative,
        to: to_relative,
        applied: !dry_run,
        warnings,
        preloads,
    })
}

/// Determine blank-line spacing to add before inserting a declaration into an existing file.
fn insertion_spacing(decl_kind: &str, target_source: &str) -> String {
    let trimmed = target_source.trim_end();
    if trimmed.is_empty() {
        return String::new();
    }

    // Functions and classes get 2 blank lines before them
    let needs_extra = matches!(
        decl_kind,
        "function_definition" | "constructor_definition" | "class_definition"
    );

    if needs_extra {
        // Ensure the target ends with enough newlines for 2 blank lines
        let trailing_newlines = target_source.len() - trimmed.len();
        if trailing_newlines >= 3 {
            String::new()
        } else {
            "\n".repeat(3 - trailing_newlines)
        }
    } else {
        // Variables, constants, signals: 1 blank line
        let trailing_newlines = target_source.len() - trimmed.len();
        if trailing_newlines >= 2 {
            String::new()
        } else {
            "\n".repeat(2 - trailing_newlines)
        }
    }
}

// ── Self-reference analysis ─────────────────────────────────────────────────

/// Collect all `self.member` references in a node subtree.
fn collect_self_references(node: Node, source: &str) -> Vec<String> {
    let mut members = Vec::new();
    collect_self_refs_recursive(node, source, &mut members);
    members.sort();
    members.dedup();
    members
}

fn collect_self_refs_recursive(node: Node, source: &str, members: &mut Vec<String>) {
    // `self.foo` is an `attribute` node: child(0)=self, child(1)=".", child(2)=foo
    // or with attribute_call: child(0)=self, child(1)=".", child(2)=attribute_call
    if node.kind() == "attribute"
        && let Some(obj) = node.child(0)
        && obj.kind() == "identifier"
        && obj.utf8_text(source.as_bytes()).ok() == Some("self")
    {
        // The member is child(2) for property access, or named_child(1) as fallback
        if let Some(member) = node.child(2) {
            let name_text = if member.kind() == "attribute_call" {
                // self.method() → attribute_call's first named child is the name
                member
                    .named_child(0)
                    .and_then(|n| n.utf8_text(source.as_bytes()).ok())
            } else {
                member.utf8_text(source.as_bytes()).ok()
            };
            if let Some(name) = name_text {
                members.push(name.to_string());
            }
        }
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_self_refs_recursive(child, source, members);
    }
}

/// Check if a scope (class body or root) declares a member with the given name.
fn class_has_member(scope: Node, source: &str, name: &str) -> bool {
    let search_node = if scope.kind() == "class_definition" {
        scope.child_by_field_name("body").unwrap_or(scope)
    } else {
        scope
    };
    let mut cursor = search_node.walk();
    for child in search_node.children(&mut cursor) {
        if DECLARATION_KINDS.contains(&child.kind())
            && let Some(decl_name) = get_declaration_name(child, source)
            && decl_name == name
        {
            return true;
        }
    }
    false
}

// ── Preload path detection ──────────────────────────────────────────────────

/// Find all preload()/load() references to a given `res://` path across the workspace.
pub fn find_preloads_to_file(
    res_path: &str,
    workspace: &crate::lsp::workspace::WorkspaceIndex,
    project_root: &Path,
) -> Vec<PreloadRef> {
    let mut refs = Vec::new();
    for (path, content) in workspace.all_files() {
        if let Ok(tree) = crate::core::parser::parse(&content) {
            find_preloads_in_tree(
                tree.root_node(),
                &content,
                res_path,
                &crate::core::fs::relative_slash(&path, project_root),
                &mut refs,
            );
        }
    }
    refs
}

/// After moving a symbol, update preload/load paths in files that reference
/// the source file. For each file with a preload of the source, add a matching
/// preload of the destination file.
pub fn update_callers_after_move(
    source_res_path: &str,
    dest_res_path: &str,
    preloads: &[PreloadRef],
    project_root: &Path,
) -> Result<Vec<CallerUpdate>> {
    let mut updates = Vec::new();

    for preload_ref in preloads {
        let file_path = project_root.join(&preload_ref.file);
        if !file_path.exists() {
            continue;
        }
        let content = std::fs::read_to_string(&file_path)
            .map_err(|e| miette::miette!("cannot read {}: {e}", preload_ref.file))?;

        // Find the preload/load line and add a matching one for the destination
        let line_idx = (preload_ref.line - 1) as usize;
        let lines: Vec<&str> = content.lines().collect();
        if line_idx >= lines.len() {
            continue;
        }

        let preload_line = lines[line_idx];

        // Build a new preload line by replacing the source path with dest path
        let new_preload_line = preload_line.replace(source_res_path, dest_res_path);
        if new_preload_line == preload_line {
            continue; // couldn't substitute
        }

        // Derive a variable name from the dest path for the new preload
        // If the original line was `var Foo = preload(...)` or `const Foo = preload(...)`,
        // we'll add a new line with a derived name
        let mut new_content = String::with_capacity(content.len() + new_preload_line.len() + 1);
        for (i, line) in lines.iter().enumerate() {
            new_content.push_str(line);
            new_content.push('\n');
            if i == line_idx {
                new_content.push_str(&new_preload_line);
                new_content.push('\n');
            }
        }

        std::fs::write(&file_path, &new_content)
            .map_err(|e| miette::miette!("cannot write {}: {e}", preload_ref.file))?;

        updates.push(CallerUpdate {
            file: preload_ref.file.clone(),
            added_preload: new_preload_line.trim().to_string(),
        });
    }

    Ok(updates)
}

#[derive(Serialize, Debug)]
pub struct CallerUpdate {
    pub file: String,
    pub added_preload: String,
}

fn find_preloads_in_tree(
    node: Node,
    source: &str,
    target_path: &str,
    file: &str,
    refs: &mut Vec<PreloadRef>,
) {
    if node.kind() == "call" {
        let func_name = node
            .child_by_field_name("function")
            .or_else(|| node.named_child(0));
        if let Some(func) = func_name
            && let Ok(name) = func.utf8_text(source.as_bytes())
            && (name == "preload" || name == "load")
            && let Some(args) = node.child_by_field_name("arguments")
        {
            // Find string argument
            let mut arg_cursor = args.walk();
            for arg in args.children(&mut arg_cursor) {
                if arg.kind() == "string"
                    && let Ok(text) = arg.utf8_text(source.as_bytes())
                {
                    let unquoted = text.trim_matches('"').trim_matches('\'');
                    if unquoted == target_path {
                        refs.push(PreloadRef {
                            file: file.to_string(),
                            line: node.start_position().row as u32 + 1,
                            path: unquoted.to_string(),
                        });
                    }
                }
            }
        }
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        find_preloads_in_tree(child, source, target_path, file, refs);
    }
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
    fn move_to_new_file() {
        let temp = setup_project(&[("source.gd", "var keep = 1\n\n\nfunc helper():\n\tpass\n")]);
        let result = move_symbol(
            "helper",
            &temp.path().join("source.gd"),
            &temp.path().join("helpers.gd"),
            false,
            temp.path(),
            None,
            None,
        )
        .unwrap();
        assert!(result.applied);
        assert_eq!(result.kind, "function");
        assert!(
            temp.path().join("helpers.gd").exists(),
            "target file should be created"
        );
        let target = fs::read_to_string(temp.path().join("helpers.gd")).unwrap();
        assert!(target.contains("func helper()"));
        let source = fs::read_to_string(temp.path().join("source.gd")).unwrap();
        assert!(!source.contains("helper"));
        assert!(source.contains("keep"));
    }

    #[test]
    fn move_to_existing_file() {
        let temp = setup_project(&[
            (
                "source.gd",
                "func to_move():\n\tpass\n\n\nfunc stay():\n\tpass\n",
            ),
            ("target.gd", "func existing():\n\tpass\n"),
        ]);
        let result = move_symbol(
            "to_move",
            &temp.path().join("source.gd"),
            &temp.path().join("target.gd"),
            false,
            temp.path(),
            None,
            None,
        )
        .unwrap();
        assert!(result.applied);
        let target = fs::read_to_string(temp.path().join("target.gd")).unwrap();
        assert!(target.contains("func existing()"));
        assert!(target.contains("func to_move()"));
    }

    #[test]
    fn move_constant() {
        let temp = setup_project(&[("source.gd", "const A = 1\nconst B = 2\n")]);
        let result = move_symbol(
            "A",
            &temp.path().join("source.gd"),
            &temp.path().join("target.gd"),
            false,
            temp.path(),
            None,
            None,
        )
        .unwrap();
        assert!(result.applied);
        assert_eq!(result.kind, "constant");
    }

    #[test]
    fn move_signal() {
        let temp = setup_project(&[("source.gd", "signal moved\nsignal stay\n")]);
        let result = move_symbol(
            "moved",
            &temp.path().join("source.gd"),
            &temp.path().join("target.gd"),
            false,
            temp.path(),
            None,
            None,
        )
        .unwrap();
        assert!(result.applied);
        assert_eq!(result.kind, "signal");
    }

    #[test]
    fn move_class() {
        let temp = setup_project(&[("source.gd", "class Helper:\n\tvar x = 1\n\nvar keep = 2\n")]);
        let result = move_symbol(
            "Helper",
            &temp.path().join("source.gd"),
            &temp.path().join("target.gd"),
            false,
            temp.path(),
            None,
            None,
        )
        .unwrap();
        assert!(result.applied);
        assert_eq!(result.kind, "class");
    }

    #[test]
    fn move_duplicate_error() {
        let temp = setup_project(&[
            ("source.gd", "func helper():\n\tpass\n"),
            ("target.gd", "func helper():\n\treturn 1\n"),
        ]);
        let result = move_symbol(
            "helper",
            &temp.path().join("source.gd"),
            &temp.path().join("target.gd"),
            false,
            temp.path(),
            None,
            None,
        );
        assert!(result.is_err());
    }

    #[test]
    fn move_dry_run() {
        let temp = setup_project(&[(
            "source.gd",
            "func helper():\n\tpass\n\n\nfunc keep():\n\tpass\n",
        )]);
        let result = move_symbol(
            "helper",
            &temp.path().join("source.gd"),
            &temp.path().join("target.gd"),
            true, // dry run
            temp.path(),
            None,
            None,
        )
        .unwrap();
        assert!(!result.applied);
        assert!(
            !temp.path().join("target.gd").exists(),
            "dry run should not create file"
        );
        let source = fs::read_to_string(temp.path().join("source.gd")).unwrap();
        assert!(
            source.contains("helper"),
            "dry run should not modify source"
        );
    }

    #[test]
    fn move_correct_spacing() {
        let temp = setup_project(&[
            ("source.gd", "func moved():\n\tpass\n"),
            ("target.gd", "var x = 1\n"),
        ]);
        let _ = move_symbol(
            "moved",
            &temp.path().join("source.gd"),
            &temp.path().join("target.gd"),
            false,
            temp.path(),
            None,
            None,
        )
        .unwrap();
        let target = fs::read_to_string(temp.path().join("target.gd")).unwrap();
        // Functions should have 2 blank lines before them
        assert!(
            target.contains("\n\n\nfunc moved()"),
            "should have 2 blank lines before function, got: {target:?}"
        );
    }

    // ── inner class operations ──────────────────────────────────────────

    #[test]
    fn move_from_inner_class_to_top_level() {
        let temp = setup_project(&[(
            "source.gd",
            "class Inner:\n\tvar keep = 1\n\tfunc helper():\n\t\tpass\n",
        )]);
        let result = move_symbol(
            "helper",
            &temp.path().join("source.gd"),
            &temp.path().join("target.gd"),
            false,
            temp.path(),
            Some("Inner"),
            None,
        )
        .unwrap();
        assert!(result.applied);
        let target = fs::read_to_string(temp.path().join("target.gd")).unwrap();
        // Should be re-indented to top-level (no leading tab)
        assert!(
            target.contains("func helper():"),
            "should be at top-level indent, got: {target}"
        );
        assert!(
            !target.contains("\tfunc helper"),
            "should NOT have tab-indented func, got: {target}"
        );
    }

    #[test]
    fn move_top_level_into_class() {
        let temp = setup_project(&[
            ("source.gd", "func helper():\n\tpass\n"),
            ("target.gd", "class Target:\n\tvar x = 1\n"),
        ]);
        let result = move_symbol(
            "helper",
            &temp.path().join("source.gd"),
            &temp.path().join("target.gd"),
            false,
            temp.path(),
            None,
            Some("Target"),
        )
        .unwrap();
        assert!(result.applied);
        let target = fs::read_to_string(temp.path().join("target.gd")).unwrap();
        assert!(
            target.contains("\tfunc helper():"),
            "should be indented in class, got: {target}"
        );
    }

    // ── preload detection ───────────────────────────────────────────────

    #[test]
    fn move_detects_preloads_to_source_file() {
        let temp = setup_project(&[
            ("source.gd", "func helper():\n\tpass\n"),
            (
                "other.gd",
                "var x = preload(\"res://source.gd\")\nfunc _ready():\n\tpass\n",
            ),
        ]);
        let result = move_symbol(
            "helper",
            &temp.path().join("source.gd"),
            &temp.path().join("target.gd"),
            true, // dry run to just check
            temp.path(),
            None,
            None,
        )
        .unwrap();
        assert!(
            !result.preloads.is_empty(),
            "should detect preload to source file"
        );
        assert_eq!(result.preloads[0].path, "res://source.gd");
    }

    #[test]
    fn move_no_preloads_unrelated() {
        let temp = setup_project(&[
            ("source.gd", "func helper():\n\tpass\n"),
            ("other.gd", "var x = preload(\"res://other_thing.gd\")\n"),
        ]);
        let result = move_symbol(
            "helper",
            &temp.path().join("source.gd"),
            &temp.path().join("target.gd"),
            true,
            temp.path(),
            None,
            None,
        )
        .unwrap();
        assert!(
            result.preloads.is_empty(),
            "should not list unrelated preloads"
        );
    }

    // ── self-reference warnings ─────────────────────────────────────────

    #[test]
    fn move_self_ref_warning_missing_member() {
        let temp = setup_project(&[
            (
                "source.gd",
                "class Src:\n\tvar health = 100\n\tfunc take_damage():\n\t\tself.health -= 10\n",
            ),
            ("target.gd", "class Dst:\n\tvar armor = 50\n"),
        ]);
        let result = move_symbol(
            "take_damage",
            &temp.path().join("source.gd"),
            &temp.path().join("target.gd"),
            true,
            temp.path(),
            Some("Src"),
            Some("Dst"),
        )
        .unwrap();
        assert!(
            result.warnings.iter().any(|w| w.contains("self.health")),
            "should warn about missing self.health, got: {:?}",
            result.warnings
        );
    }

    #[test]
    fn move_self_ref_no_warning_when_present() {
        let temp = setup_project(&[
            (
                "source.gd",
                "class Src:\n\tvar health = 100\n\tfunc take_damage():\n\t\tself.health -= 10\n",
            ),
            ("target.gd", "class Dst:\n\tvar health = 200\n"),
        ]);
        let result = move_symbol(
            "take_damage",
            &temp.path().join("source.gd"),
            &temp.path().join("target.gd"),
            true,
            temp.path(),
            Some("Src"),
            Some("Dst"),
        )
        .unwrap();
        assert!(
            !result.warnings.iter().any(|w| w.contains("self.health")),
            "should NOT warn when member exists, got: {:?}",
            result.warnings
        );
    }

    #[test]
    fn move_no_self_refs_no_warning() {
        let temp = setup_project(&[
            (
                "source.gd",
                "class Src:\n\tfunc helper():\n\t\tprint(\"hello\")\n",
            ),
            ("target.gd", "class Dst:\n\tvar x = 1\n"),
        ]);
        let result = move_symbol(
            "helper",
            &temp.path().join("source.gd"),
            &temp.path().join("target.gd"),
            true,
            temp.path(),
            Some("Src"),
            Some("Dst"),
        )
        .unwrap();
        assert!(
            !result.warnings.iter().any(|w| w.contains("self.")),
            "no self refs means no self-ref warnings"
        );
    }
}
