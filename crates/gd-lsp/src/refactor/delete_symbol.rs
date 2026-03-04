use std::collections::HashMap;
use std::path::{Path, PathBuf};

use miette::Result;

use gd_core::gd_ast;

use super::{
    DeleteSymbolOutput, LineRange, RefLocation, declaration_full_range, declaration_kind_str,
    find_declaration_by_line, find_declaration_by_name, find_declaration_in_class,
    get_declaration_name, line_starts, normalize_blank_lines,
};

#[allow(clippy::too_many_lines)]
pub fn delete_symbol(
    file: &Path,
    name: Option<&str>,
    line: Option<usize>,
    force: bool,
    dry_run: bool,
    project_root: &Path,
    class: Option<&str>,
) -> Result<DeleteSymbolOutput> {
    // Check for enum member syntax: "EnumName.MEMBER"
    if let Some(name) = name
        && let Some((enum_name, member_name)) = name.split_once('.')
    {
        return delete_enum_member(file, enum_name, member_name, force, dry_run, project_root);
    }

    let source =
        std::fs::read_to_string(file).map_err(|e| miette::miette!("cannot read file: {e}"))?;
    let tree = gd_core::parser::parse(&source)?;
    let gd_file = gd_ast::convert(&tree, &source);

    let decl = if let Some(class_name) = class {
        // Look inside an inner class
        let gd_class = gd_file
            .find_class(class_name)
            .ok_or_else(|| miette::miette!("no inner class named '{class_name}' found"))?;
        if let Some(name) = name {
            find_declaration_in_class(gd_class, name).ok_or_else(|| {
                miette::miette!("no declaration named '{name}' found in class '{class_name}'")
            })?
        } else if let Some(line) = line {
            gd_class
                .find_decl_by_line(line - 1)
                .map(gd_ast::GdDecl::node)
                .ok_or_else(|| {
                    miette::miette!("no declaration found at line {line} in class '{class_name}'")
                })?
        } else {
            return Err(miette::miette!("either --name or --line is required"));
        }
    } else if let Some(name) = name {
        find_declaration_by_name(&gd_file, name)
            .ok_or_else(|| miette::miette!("no declaration named '{name}' found at top level"))?
    } else if let Some(line) = line {
        find_declaration_by_line(&gd_file, line - 1)
            .ok_or_else(|| miette::miette!("no declaration found at line {line}"))?
    } else {
        return Err(miette::miette!("either --name or --line is required"));
    };

    let symbol_name = get_declaration_name(decl, &source).unwrap_or_else(|| "unknown".to_string());
    let kind = declaration_kind_str(decl.kind()).to_string();

    let (start_byte, end_byte) = declaration_full_range(decl, &source);

    // Compute 1-based line range for the removed section
    let starts = line_starts(&source);
    let start_line_1 = starts
        .iter()
        .position(|&s| s > start_byte)
        .unwrap_or(starts.len());
    let end_line_1 = starts
        .iter()
        .position(|&s| s >= end_byte)
        .unwrap_or(starts.len());

    // Check for references across the workspace.
    // Scope the search: if the file declares a class_name, only look for references
    // in files that reference that class (avoids false positives from same-name methods
    // on unrelated classes).
    let workspace = crate::workspace::WorkspaceIndex::new(project_root.to_path_buf());
    let all_refs = crate::references::find_references_by_name(
        &symbol_name,
        &workspace,
        if gd_file.class_name.is_none() {
            Some(file)
        } else {
            None
        },
        gd_file.class_name,
    );

    // Filter out references within the declaration's own range
    let file_uri = tower_lsp::lsp_types::Url::from_file_path(file).ok();
    let decl_start_line = decl.start_position().row as u32;
    let decl_end_line = decl.end_position().row as u32;

    let external_refs: Vec<_> = all_refs
        .into_iter()
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
        .collect();

    let relative_file = gd_core::fs::relative_slash(file, project_root);

    let ref_outputs: Vec<RefLocation> = external_refs
        .iter()
        .map(|loc| {
            let loc_file = crate::query::url_to_relative(&loc.uri, project_root);
            RefLocation {
                file: loc_file,
                line: loc.range.start.line + 1,
                column: loc.range.start.character + 1,
                end_line: loc.range.end.line + 1,
                end_column: loc.range.end.character + 1,
            }
        })
        .collect();

    if !external_refs.is_empty() && !force {
        return Ok(DeleteSymbolOutput {
            symbol: symbol_name,
            kind,
            file: relative_file,
            removed_lines: LineRange {
                start: start_line_1 as u32,
                end: end_line_1 as u32,
            },
            references: ref_outputs,
            applied: false,
        });
    }

    if !dry_run {
        let mut new_source = String::with_capacity(source.len());
        new_source.push_str(&source[..start_byte]);
        new_source.push_str(&source[end_byte..]);
        normalize_blank_lines(&mut new_source);
        super::validate_no_new_errors(&source, &new_source)?;
        std::fs::write(file, &new_source).map_err(|e| miette::miette!("cannot write file: {e}"))?;

        let mut snaps: HashMap<PathBuf, Option<Vec<u8>>> = HashMap::new();
        snaps.insert(file.to_path_buf(), Some(source.as_bytes().to_vec()));
        let stack = super::undo::UndoStack::open(project_root);
        let _ = stack.record(
            "delete-symbol",
            &format!("delete {symbol_name}"),
            &snaps,
            project_root,
        );
    }

    Ok(DeleteSymbolOutput {
        symbol: symbol_name,
        kind,
        file: relative_file,
        removed_lines: LineRange {
            start: start_line_1 as u32,
            end: end_line_1 as u32,
        },
        references: ref_outputs,
        applied: !dry_run,
    })
}

// ── delete-enum-member ──────────────────────────────────────────────────────

#[allow(clippy::too_many_lines)]
fn delete_enum_member(
    file: &Path,
    enum_name: &str,
    member_name: &str,
    force: bool,
    dry_run: bool,
    project_root: &Path,
) -> Result<DeleteSymbolOutput> {
    let source =
        std::fs::read_to_string(file).map_err(|e| miette::miette!("cannot read file: {e}"))?;
    let tree = gd_core::parser::parse(&source)?;
    let gd_file = gd_ast::convert(&tree, &source);

    let enum_node = find_declaration_by_name(&gd_file, enum_name)
        .ok_or_else(|| miette::miette!("no enum named '{enum_name}' found"))?;
    if enum_node.kind() != "enum_definition" {
        return Err(miette::miette!("'{enum_name}' is not an enum"));
    }

    // Find enumerator_list (the { ... } body)
    let body = enum_node
        .child_by_field_name("body")
        .ok_or_else(|| miette::miette!("enum has no body"))?;

    // Collect all enumerator children
    let mut enumerators: Vec<tree_sitter::Node> = Vec::new();
    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        if child.kind() == "enumerator" {
            enumerators.push(child);
        }
    }

    // Find the target member
    let (member_idx, _member_node) = enumerators
        .iter()
        .enumerate()
        .find(|(_, e)| {
            if let Some(name_node) = e.child_by_field_name("name") {
                name_node.utf8_text(source.as_bytes()).ok() == Some(member_name)
            } else {
                // Fallback: first named child is the name identifier
                e.named_child(0)
                    .and_then(|n| n.utf8_text(source.as_bytes()).ok())
                    == Some(member_name)
            }
        })
        .ok_or_else(|| miette::miette!("no member '{member_name}' in enum '{enum_name}'"))?;

    if enumerators.len() == 1 {
        return Err(miette::miette!(
            "cannot delete the last member of enum '{enum_name}'"
        ));
    }

    // Compute byte range to remove including comma
    let member_node = enumerators[member_idx];
    let (remove_start, remove_end) =
        compute_enum_member_removal_range(&source, &enumerators, member_idx);

    let relative_file = gd_core::fs::relative_slash(file, project_root);
    let starts = line_starts(&source);
    let start_line_1 = starts
        .iter()
        .position(|&s| s > member_node.start_byte())
        .unwrap_or(starts.len());
    let end_line_1 = start_line_1; // Single member is usually one line

    // Check references — scope to this class if class_name is declared
    let workspace = crate::workspace::WorkspaceIndex::new(project_root.to_path_buf());
    let all_refs = crate::references::find_references_by_name(
        member_name,
        &workspace,
        if gd_file.class_name.is_none() {
            Some(file)
        } else {
            None
        },
        gd_file.class_name,
    );

    let file_uri = tower_lsp::lsp_types::Url::from_file_path(file).ok();
    let enum_start = enum_node.start_position().row as u32;
    let enum_end = enum_node.end_position().row as u32;

    let external_refs: Vec<_> = all_refs
        .into_iter()
        .filter(|loc| {
            if let Some(ref uri) = file_uri
                && &loc.uri == uri
            {
                let ref_line = loc.range.start.line;
                if ref_line >= enum_start && ref_line <= enum_end {
                    return false;
                }
            }
            true
        })
        .collect();

    let ref_outputs: Vec<RefLocation> = external_refs
        .iter()
        .map(|loc| {
            let loc_file = crate::query::url_to_relative(&loc.uri, project_root);
            RefLocation {
                file: loc_file,
                line: loc.range.start.line + 1,
                column: loc.range.start.character + 1,
                end_line: loc.range.end.line + 1,
                end_column: loc.range.end.character + 1,
            }
        })
        .collect();

    if !external_refs.is_empty() && !force {
        return Ok(DeleteSymbolOutput {
            symbol: format!("{enum_name}.{member_name}"),
            kind: "enum_member".to_string(),
            file: relative_file,
            removed_lines: LineRange {
                start: start_line_1 as u32,
                end: end_line_1 as u32,
            },
            references: ref_outputs,
            applied: false,
        });
    }

    if !dry_run {
        let mut new_source = String::with_capacity(source.len());
        new_source.push_str(&source[..remove_start]);
        new_source.push_str(&source[remove_end..]);
        super::validate_no_new_errors(&source, &new_source)?;
        std::fs::write(file, &new_source).map_err(|e| miette::miette!("cannot write file: {e}"))?;

        let mut snaps: HashMap<PathBuf, Option<Vec<u8>>> = HashMap::new();
        snaps.insert(file.to_path_buf(), Some(source.as_bytes().to_vec()));
        let stack = super::undo::UndoStack::open(project_root);
        let _ = stack.record(
            "delete-symbol",
            &format!("delete {enum_name}.{member_name}"),
            &snaps,
            project_root,
        );
    }

    Ok(DeleteSymbolOutput {
        symbol: format!("{enum_name}.{member_name}"),
        kind: "enum_member".to_string(),
        file: relative_file,
        removed_lines: LineRange {
            start: start_line_1 as u32,
            end: end_line_1 as u32,
        },
        references: ref_outputs,
        applied: !dry_run,
    })
}

/// Compute byte range to remove for an enum member, including adjacent comma/whitespace.
fn compute_enum_member_removal_range(
    source: &str,
    enumerators: &[tree_sitter::Node],
    idx: usize,
) -> (usize, usize) {
    let member = enumerators[idx];

    if enumerators.len() == 1 {
        // Should not happen (checked above), but be safe
        return (member.start_byte(), member.end_byte());
    }

    if idx == 0 {
        // First member: remove from member start to next member start
        let next = enumerators[1];
        (member.start_byte(), next.start_byte())
    } else {
        // Middle or last: remove from comma after previous member to this member end
        let prev = enumerators[idx - 1];
        let between = &source[prev.end_byte()..member.end_byte()];
        let comma_offset = between.find(',').unwrap_or(0);
        (prev.end_byte() + comma_offset, member.end_byte())
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
    fn delete_function() {
        let temp = setup_project(&[(
            "player.gd",
            "var health = 100\n\n\nfunc unused():\n\tpass\n\n\nfunc _ready():\n\tpass\n",
        )]);
        let result = delete_symbol(
            &temp.path().join("player.gd"),
            Some("unused"),
            None,
            false,
            false,
            temp.path(),
            None,
        )
        .unwrap();
        assert!(result.applied);
        assert_eq!(result.kind, "function");
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(!content.contains("unused"));
        assert!(content.contains("health"));
        assert!(content.contains("_ready"));
    }

    #[test]
    fn delete_variable() {
        let temp = setup_project(&[("player.gd", "var unused_var = 1\nvar keep = 2\n")]);
        let result = delete_symbol(
            &temp.path().join("player.gd"),
            Some("unused_var"),
            None,
            false,
            false,
            temp.path(),
            None,
        )
        .unwrap();
        assert!(result.applied);
        assert_eq!(result.kind, "variable");
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(!content.contains("unused_var"));
        assert!(content.contains("keep"));
    }

    #[test]
    fn delete_constant() {
        let temp = setup_project(&[("player.gd", "const OLD = 1\nconst KEEP = 2\n")]);
        let result = delete_symbol(
            &temp.path().join("player.gd"),
            Some("OLD"),
            None,
            false,
            false,
            temp.path(),
            None,
        )
        .unwrap();
        assert!(result.applied);
        assert_eq!(result.kind, "constant");
    }

    #[test]
    fn delete_signal() {
        let temp = setup_project(&[("player.gd", "signal unused\nsignal keep\n")]);
        let result = delete_symbol(
            &temp.path().join("player.gd"),
            Some("unused"),
            None,
            false,
            false,
            temp.path(),
            None,
        )
        .unwrap();
        assert!(result.applied);
        assert_eq!(result.kind, "signal");
    }

    #[test]
    fn delete_enum() {
        let temp = setup_project(&[("player.gd", "enum OldState { A, B }\nenum State { C, D }\n")]);
        let result = delete_symbol(
            &temp.path().join("player.gd"),
            Some("OldState"),
            None,
            false,
            false,
            temp.path(),
            None,
        )
        .unwrap();
        assert!(result.applied);
        assert_eq!(result.kind, "enum");
    }

    #[test]
    fn delete_class() {
        let temp = setup_project(&[(
            "player.gd",
            "class Unused:\n\tvar x = 1\n\nclass Keep:\n\tvar y = 2\n",
        )]);
        let result = delete_symbol(
            &temp.path().join("player.gd"),
            Some("Unused"),
            None,
            false,
            false,
            temp.path(),
            None,
        )
        .unwrap();
        assert!(result.applied);
        assert_eq!(result.kind, "class");
    }

    #[test]
    fn delete_with_doc_comments() {
        let temp = setup_project(&[(
            "player.gd",
            "## This is documented\n## More docs\nfunc documented():\n\tpass\n",
        )]);
        let result = delete_symbol(
            &temp.path().join("player.gd"),
            Some("documented"),
            None,
            false,
            false,
            temp.path(),
            None,
        )
        .unwrap();
        assert!(result.applied);
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(
            !content.contains("documented"),
            "function should be removed"
        );
        assert!(
            !content.contains("## This is documented"),
            "doc comments should be removed"
        );
    }

    #[test]
    fn delete_by_line() {
        let temp = setup_project(&[(
            "player.gd",
            "var a = 1\n\n\nfunc target():\n\tpass\n\n\nfunc keep():\n\tpass\n",
        )]);
        let result = delete_symbol(
            &temp.path().join("player.gd"),
            None,
            Some(4), // line 4 is "func target():"
            false,
            false,
            temp.path(),
            None,
        )
        .unwrap();
        assert!(result.applied);
        assert_eq!(result.symbol, "target");
    }

    #[test]
    fn delete_not_found() {
        let temp = setup_project(&[("player.gd", "var x = 1\n")]);
        let result = delete_symbol(
            &temp.path().join("player.gd"),
            Some("nonexistent"),
            None,
            false,
            false,
            temp.path(),
            None,
        );
        assert!(result.is_err());
    }

    #[test]
    fn delete_blocked_by_references() {
        let temp = setup_project(&[
            (
                "player.gd",
                "var speed = 10\n\n\nfunc run():\n\tprint(speed)\n",
            ),
            ("enemy.gd", "var speed = 5\n"),
        ]);
        let result = delete_symbol(
            &temp.path().join("player.gd"),
            Some("speed"),
            None,
            false,
            false,
            temp.path(),
            None,
        )
        .unwrap();
        assert!(!result.applied, "should not delete when references exist");
        assert!(
            !result.references.is_empty(),
            "should list external references"
        );
    }

    #[test]
    fn delete_force_with_references() {
        let temp = setup_project(&[(
            "player.gd",
            "var speed = 10\n\n\nfunc run():\n\tprint(speed)\n",
        )]);
        let result = delete_symbol(
            &temp.path().join("player.gd"),
            Some("speed"),
            None,
            true,  // force
            false, // not dry run
            temp.path(),
            None,
        )
        .unwrap();
        assert!(result.applied, "force should override reference check");
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(!content.contains("var speed"), "should be deleted");
    }

    #[test]
    fn delete_dry_run() {
        let temp = setup_project(&[(
            "player.gd",
            "func unused():\n\tpass\n\n\nfunc keep():\n\tpass\n",
        )]);
        let result = delete_symbol(
            &temp.path().join("player.gd"),
            Some("unused"),
            None,
            false,
            true, // dry run
            temp.path(),
            None,
        )
        .unwrap();
        assert!(!result.applied);
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(content.contains("unused"), "dry run should not modify file");
    }

    // ── inner class operations ──────────────────────────────────────────

    #[test]
    fn delete_from_inner_class() {
        let temp = setup_project(&[(
            "player.gd",
            "class Inner:\n\tvar keep = 1\n\tfunc remove_me():\n\t\tpass\n",
        )]);
        let result = delete_symbol(
            &temp.path().join("player.gd"),
            Some("remove_me"),
            None,
            false,
            false,
            temp.path(),
            Some("Inner"),
        )
        .unwrap();
        assert!(result.applied);
        assert_eq!(result.kind, "function");
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(!content.contains("remove_me"));
        assert!(content.contains("keep"));
    }

    #[test]
    fn delete_var_from_inner_class() {
        let temp = setup_project(&[("player.gd", "class Inner:\n\tvar old = 1\n\tvar keep = 2\n")]);
        let result = delete_symbol(
            &temp.path().join("player.gd"),
            Some("old"),
            None,
            false,
            false,
            temp.path(),
            Some("Inner"),
        )
        .unwrap();
        assert!(result.applied);
        assert_eq!(result.kind, "variable");
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(!content.contains("old"));
        assert!(content.contains("keep"));
    }

    #[test]
    fn delete_class_not_found() {
        let temp = setup_project(&[("player.gd", "var x = 1\n")]);
        let result = delete_symbol(
            &temp.path().join("player.gd"),
            Some("foo"),
            None,
            false,
            false,
            temp.path(),
            Some("NonExistent"),
        );
        assert!(result.is_err());
    }

    // ── enum member operations ──────────────────────────────────────────

    #[test]
    fn delete_enum_member_first() {
        let temp = setup_project(&[("player.gd", "enum State { IDLE, RUN, JUMP }\n")]);
        let result = delete_symbol(
            &temp.path().join("player.gd"),
            Some("State.IDLE"),
            None,
            false,
            false,
            temp.path(),
            None,
        )
        .unwrap();
        assert!(result.applied);
        assert_eq!(result.kind, "enum_member");
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(!content.contains("IDLE"), "IDLE should be removed");
        assert!(content.contains("RUN"), "RUN should remain");
        assert!(content.contains("JUMP"), "JUMP should remain");
    }

    #[test]
    fn delete_enum_member_last() {
        let temp = setup_project(&[("player.gd", "enum State { IDLE, RUN, JUMP }\n")]);
        let result = delete_symbol(
            &temp.path().join("player.gd"),
            Some("State.JUMP"),
            None,
            false,
            false,
            temp.path(),
            None,
        )
        .unwrap();
        assert!(result.applied);
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(!content.contains("JUMP"), "JUMP should be removed");
        assert!(content.contains("IDLE"), "IDLE should remain");
        assert!(content.contains("RUN"), "RUN should remain");
    }

    #[test]
    fn delete_enum_member_middle() {
        let temp = setup_project(&[("player.gd", "enum State { IDLE, RUN, JUMP }\n")]);
        let result = delete_symbol(
            &temp.path().join("player.gd"),
            Some("State.RUN"),
            None,
            false,
            false,
            temp.path(),
            None,
        )
        .unwrap();
        assert!(result.applied);
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(!content.contains("RUN"), "RUN should be removed");
        assert!(content.contains("IDLE"), "IDLE should remain");
        assert!(content.contains("JUMP"), "JUMP should remain");
    }

    #[test]
    fn delete_enum_member_with_value() {
        let temp = setup_project(&[("player.gd", "enum State { IDLE = 0, RUN = 1, JUMP = 2 }\n")]);
        let result = delete_symbol(
            &temp.path().join("player.gd"),
            Some("State.RUN"),
            None,
            false,
            false,
            temp.path(),
            None,
        )
        .unwrap();
        assert!(result.applied);
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(!content.contains("RUN"), "RUN should be removed");
    }

    #[test]
    fn delete_enum_member_last_one_error() {
        let temp = setup_project(&[("player.gd", "enum State { ONLY }\n")]);
        let result = delete_symbol(
            &temp.path().join("player.gd"),
            Some("State.ONLY"),
            None,
            false,
            false,
            temp.path(),
            None,
        );
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("last member"), "should error: {err}");
    }

    #[test]
    fn delete_enum_member_not_found() {
        let temp = setup_project(&[("player.gd", "enum State { IDLE, RUN }\n")]);
        let result = delete_symbol(
            &temp.path().join("player.gd"),
            Some("State.JUMP"),
            None,
            false,
            false,
            temp.path(),
            None,
        );
        assert!(result.is_err());
    }
}
