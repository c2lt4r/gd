use std::path::Path;

use miette::Result;
use tree_sitter::Node;

use gd_core::gd_ast;

use super::{
    CallerUpdateInfo, DECLARATION_KINDS, MoveSymbolOutput, PreloadRef, declaration_full_range,
    declaration_kind_str, find_class_definition, find_declaration_by_name,
    find_declaration_in_class, get_declaration_name, normalize_blank_lines, re_indent_to_depth,
};

#[allow(clippy::too_many_lines, clippy::too_many_arguments)]
pub fn move_symbol(
    name: &str,
    from_file: &Path,
    to_file: &Path,
    dry_run: bool,
    project_root: &Path,
    class: Option<&str>,
    target_class: Option<&str>,
    update_callers: bool,
) -> Result<MoveSymbolOutput> {
    let source = std::fs::read_to_string(from_file)
        .map_err(|e| miette::miette!("cannot read source file: {e}"))?;
    let tree = gd_core::parser::parse(&source)?;
    let file = gd_ast::convert(&tree, &source);

    // Find the declaration (possibly within a class)
    let decl = if let Some(class_name) = class {
        let inner = file
            .find_class(class_name)
            .ok_or_else(|| miette::miette!("no inner class named '{class_name}' found"))?;
        find_declaration_in_class(inner, name).ok_or_else(|| {
            miette::miette!("no declaration named '{name}' found in class '{class_name}'")
        })?
    } else {
        find_declaration_by_name(&file, name)
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
        let target_tree = gd_core::parser::parse(&target_source)?;
        let target_file = gd_ast::convert(&target_tree, &target_source);

        let dup = if let Some(tc) = target_class {
            target_file
                .find_class(tc)
                .and_then(|c| find_declaration_in_class(c, name))
        } else {
            find_declaration_by_name(&target_file, name)
        };
        if dup.is_some() {
            return Err(miette::miette!(
                "target already contains a declaration named '{name}'"
            ));
        }
    }

    // Find references for warnings
    let workspace = crate::workspace::WorkspaceIndex::new(project_root.to_path_buf());
    let class_filter = class;
    let all_refs = crate::references::find_references_by_name(name, &workspace, None, class_filter);

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
            let target_tree = gd_core::parser::parse(&target_source)?;
            let target_file = gd_ast::convert(&target_tree, &target_source);

            let target_scope = if let Some(tc) = target_class {
                find_class_definition(&target_file, tc)
            } else {
                Some(target_tree.root_node())
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

    let from_relative = gd_core::fs::relative_slash(from_file, project_root);
    let to_relative = gd_core::fs::relative_slash(to_file, project_root);

    // Detect preload/load references to the source file
    let from_res = format!("res://{from_relative}");
    let to_res = format!("res://{to_relative}");
    let preloads = find_preloads_to_file(&from_res, &workspace, project_root);

    // Collect all top-level symbol names in the source file (for caller analysis)
    let source_symbols: Vec<String> = file
        .declarations
        .iter()
        .filter(|d| d.is_declaration())
        .map(|d| d.name().to_string())
        .filter(|n| !n.is_empty())
        .collect();

    let mut callers_updated = Vec::new();

    if !dry_run {
        // Write target file
        if to_file.exists() {
            let mut target_source = std::fs::read_to_string(to_file)
                .map_err(|e| miette::miette!("cannot read target file: {e}"))?;

            if let Some(tc) = target_class {
                // Insert into target class body
                let target_tree = gd_core::parser::parse(&target_source)?;
                let target_file = gd_ast::convert(&target_tree, &target_source);
                let tc_node = find_class_definition(&target_file, tc).ok_or_else(|| {
                    miette::miette!("target class '{tc}' not found in target file")
                })?;
                let insert_byte = tc_node.end_byte();
                let spacing = "\n";
                let insert_text = format!("{spacing}{decl_text}");
                target_source.insert_str(insert_byte, &insert_text);
            } else {
                let spacing = insertion_spacing(decl.kind(), &target_source);
                target_source.push_str(&spacing);
                target_source.push_str(&decl_text);
            }
            super::validate_no_new_errors("", &target_source)?;
            std::fs::write(to_file, &target_source)
                .map_err(|e| miette::miette!("cannot write target file: {e}"))?;
        } else {
            super::validate_no_new_errors("", &decl_text)?;
            std::fs::write(to_file, &decl_text)
                .map_err(|e| miette::miette!("cannot write target file: {e}"))?;
        }

        // Remove from source file
        let mut new_source = String::with_capacity(source.len());
        new_source.push_str(&source[..start_byte]);
        new_source.push_str(&source[end_byte..]);
        normalize_blank_lines(&mut new_source);
        super::validate_no_new_errors(&source, &new_source)?;
        std::fs::write(from_file, &new_source)
            .map_err(|e| miette::miette!("cannot write source file: {e}"))?;

        // Update caller files that reference the source file via preload/load
        if update_callers {
            for preload_ref in &preloads {
                let caller_path = project_root.join(&preload_ref.file);
                if caller_path == from_file || caller_path == to_file {
                    continue;
                }
                if !caller_path.exists() {
                    continue;
                }

                match update_caller_file(
                    &caller_path,
                    &from_res,
                    &to_res,
                    name,
                    &source_symbols,
                    preload_ref,
                ) {
                    Ok(Some(update)) => {
                        if let Err(e) = std::fs::write(&caller_path, &update.new_content) {
                            warnings.push(format!("could not write {}: {e}", preload_ref.file));
                        } else {
                            callers_updated.push(CallerUpdateInfo {
                                file: preload_ref.file.clone(),
                                action: update.action,
                            });
                        }
                    }
                    Ok(None) => {}
                    Err(e) => {
                        warnings.push(format!("could not update {}: {e}", preload_ref.file));
                    }
                }
            }
        }
    }

    Ok(MoveSymbolOutput {
        symbol: name.to_string(),
        kind,
        from: from_relative,
        to: to_relative,
        applied: !dry_run,
        warnings,
        preloads,
        callers_updated,
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
    workspace: &crate::workspace::WorkspaceIndex,
    project_root: &Path,
) -> Vec<PreloadRef> {
    let mut refs = Vec::new();
    for (path, content) in workspace.all_files() {
        if let Ok(tree) = gd_core::parser::parse(&content) {
            find_preloads_in_tree(
                tree.root_node(),
                &content,
                res_path,
                &gd_core::fs::relative_slash(&path, project_root),
                &mut refs,
            );
        }
    }
    refs
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

// ── Caller update logic ─────────────────────────────────────────────────────

/// Result of updating a single caller file.
struct CallerFileUpdate {
    new_content: String,
    action: String,
}

/// Update a caller file's preload path after a symbol move.
///
/// Strategy:
/// - If the caller has `preload("res://source.gd")` and only uses the moved symbol
///   from source.gd, replace the preload path with `"res://dest.gd"`.
/// - If the caller has `preload("res://source.gd")` but also uses other symbols from
///   source.gd, keep the existing preload and add a new one for dest.gd. Rewrite
///   call sites of the moved symbol to use the new preload variable.
fn update_caller_file(
    caller_path: &Path,
    source_res: &str,
    dest_res: &str,
    moved_symbol: &str,
    source_symbols: &[String],
    _preload_ref: &PreloadRef,
) -> Result<Option<CallerFileUpdate>> {
    let content = std::fs::read_to_string(caller_path)
        .map_err(|e| miette::miette!("cannot read caller: {e}"))?;

    let tree = gd_core::parser::parse(&content)?;
    let root = tree.root_node();

    // Find the preload/load assignment for the source file
    let preload_info = find_preload_assignment(root, &content, source_res);

    let Some(info) = preload_info else {
        // No variable assignment for the preload — inline preload() or load() call.
        // Replace the path directly in the source text.
        let new_content = content.replace(source_res, dest_res);
        if new_content == content {
            return Ok(None);
        }
        return Ok(Some(CallerFileUpdate {
            new_content,
            action: "replaced preload path".to_string(),
        }));
    };

    // Check if the caller uses other symbols from the source file besides the moved one.
    // "Other symbols" means top-level declarations in source.gd that are NOT the moved symbol.
    let other_symbols: Vec<&str> = source_symbols
        .iter()
        .filter(|s| s.as_str() != moved_symbol)
        .map(String::as_str)
        .collect();

    let uses_other = caller_uses_other_symbols(root, &content, &info.var_name, &other_symbols);

    if uses_other {
        // Caller uses other symbols from source.gd too — keep the existing preload,
        // add a new preload for the destination, and rewrite references to the moved symbol.
        Ok(Some(add_dest_preload_and_rewrite(
            &content,
            &info,
            dest_res,
            moved_symbol,
        )))
    } else {
        // Only the moved symbol was used from source.gd — replace the path
        let new_content = content.replace(source_res, dest_res);
        if new_content == content {
            return Ok(None);
        }
        Ok(Some(CallerFileUpdate {
            new_content,
            action: "replaced preload path".to_string(),
        }))
    }
}

/// Info about a `const/var Foo = preload("res://...")` assignment.
struct PreloadAssignment {
    /// The variable name (e.g., "Source" from `const Source = preload(...)`)
    var_name: String,
    /// The full line text of the assignment
    line_text: String,
    /// 0-based line index
    line_idx: usize,
}

/// Find a `const X = preload("res://...")` or `var X = preload("res://...")`
/// assignment for the given `res://` path.
fn find_preload_assignment(root: Node, source: &str, res_path: &str) -> Option<PreloadAssignment> {
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        let kind = child.kind();
        if kind != "variable_statement" && kind != "const_statement" {
            continue;
        }
        // Check if the initializer is a preload/load call with matching path
        let text = child.utf8_text(source.as_bytes()).unwrap_or("");
        if !text.contains(res_path) {
            continue;
        }
        // Get the variable name
        if let Some(decl_name) = get_declaration_name(child, source) {
            let line_idx = child.start_position().row;
            let lines: Vec<&str> = source.lines().collect();
            let line_text = lines.get(line_idx).unwrap_or(&"").to_string();
            return Some(PreloadAssignment {
                var_name: decl_name,
                line_text,
                line_idx,
            });
        }
    }
    None
}

/// Check if the caller file uses any symbols from source.gd besides the moved one.
///
/// We look for `VarName.symbol` patterns where `VarName` is the preload variable
/// and `symbol` is one of the other source file symbols.
fn caller_uses_other_symbols(
    root: Node,
    source: &str,
    preload_var: &str,
    other_symbols: &[&str],
) -> bool {
    if other_symbols.is_empty() {
        return false;
    }
    has_qualified_usage(root, source, preload_var, other_symbols)
}

/// Recursively check if the AST contains `preload_var.symbol` for any symbol in the list.
fn has_qualified_usage(node: Node, source: &str, preload_var: &str, symbols: &[&str]) -> bool {
    // Check `attribute` nodes: preload_var.something
    if node.kind() == "attribute"
        && let Some(obj) = node.child(0)
        && obj.kind() == "identifier"
        && obj.utf8_text(source.as_bytes()).ok() == Some(preload_var)
        && let Some(member) = node.child(2)
    {
        let member_name = if member.kind() == "attribute_call" {
            member
                .named_child(0)
                .and_then(|n| n.utf8_text(source.as_bytes()).ok())
        } else {
            member.utf8_text(source.as_bytes()).ok()
        };
        if let Some(name) = member_name
            && symbols.contains(&name)
        {
            return true;
        }
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if has_qualified_usage(child, source, preload_var, symbols) {
            return true;
        }
    }
    false
}

/// When the caller uses both the moved symbol and other symbols from source.gd:
/// 1. Keep the existing preload for source.gd
/// 2. Add a new `const _Dest = preload("res://dest.gd")` line after the existing preload
/// 3. Rewrite `Source.moved_symbol` to `_Dest.moved_symbol` throughout the file
fn add_dest_preload_and_rewrite(
    content: &str,
    preload_info: &PreloadAssignment,
    dest_res: &str,
    moved_symbol: &str,
) -> CallerFileUpdate {
    let lines: Vec<&str> = content.lines().collect();

    // Derive a variable name for the new preload from the destination file name
    let dest_var = derive_preload_var_name(dest_res);

    // Determine the keyword (const or var) from the existing preload line
    let keyword = if preload_info.line_text.trim_start().starts_with("const") {
        "const"
    } else {
        "var"
    };

    let new_preload_line = format!("{keyword} {dest_var} = preload(\"{dest_res}\")");

    // Build new content:
    // 1. Insert the new preload line after the existing one
    // 2. Replace `SourceVar.moved_symbol` with `DestVar.moved_symbol`
    let mut result = String::with_capacity(content.len() + new_preload_line.len() + 10);
    for (i, line) in lines.iter().enumerate() {
        result.push_str(line);
        result.push('\n');
        if i == preload_info.line_idx {
            result.push_str(&new_preload_line);
            result.push('\n');
        }
    }
    // Handle content that doesn't end with newline
    if !content.ends_with('\n') && !result.ends_with('\n') {
        result.push('\n');
    }

    // Rewrite qualified references: `Source.moved_symbol` → `_Dest.moved_symbol`
    let old_qualified = format!("{}.{moved_symbol}", preload_info.var_name);
    let new_qualified = format!("{dest_var}.{moved_symbol}");
    let result = result.replace(&old_qualified, &new_qualified);

    CallerFileUpdate {
        new_content: result,
        action: format!("added preload for {dest_res}, rewrote {old_qualified} → {new_qualified}"),
    }
}

/// Derive a PascalCase variable name from a `res://path/to/file.gd` path.
///
/// Examples:
/// - `res://helpers.gd` → `_Helpers`
/// - `res://utils/math_helpers.gd` → `_MathHelpers`
fn derive_preload_var_name(res_path: &str) -> String {
    let path = res_path.strip_prefix("res://").unwrap_or(res_path);
    let stem = Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Dest");

    // Convert snake_case to PascalCase
    let pascal: String = stem
        .split('_')
        .filter(|s| !s.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(c) => {
                    let upper: String = c.to_uppercase().collect();
                    format!("{upper}{}", chars.as_str())
                }
                None => String::new(),
            }
        })
        .collect();

    format!("_{pascal}")
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
            false,
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
            false,
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
            false,
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
            false,
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
            false,
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
            false,
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
            true,
            temp.path(),
            None,
            None,
            false,
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
            false,
        )
        .unwrap();
        let target = fs::read_to_string(temp.path().join("target.gd")).unwrap();
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
            false,
        )
        .unwrap();
        assert!(result.applied);
        let target = fs::read_to_string(temp.path().join("target.gd")).unwrap();
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
            false,
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
            true,
            temp.path(),
            None,
            None,
            false,
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
            false,
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
            false,
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
            false,
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
            false,
        )
        .unwrap();
        assert!(
            !result.warnings.iter().any(|w| w.contains("self.")),
            "no self refs means no self-ref warnings"
        );
    }

    // ── caller update tests ─────────────────────────────────────────────

    #[test]
    fn move_updates_caller_preload_single_symbol() {
        // Caller only uses the moved symbol from source.gd => preload path updated
        let temp = setup_project(&[
            (
                "source.gd",
                "func helper():\n\treturn 42\n\n\nfunc other():\n\tpass\n",
            ),
            (
                "caller.gd",
                "const Source = preload(\"res://source.gd\")\n\nfunc _ready():\n\tSource.helper()\n",
            ),
        ]);
        let result = move_symbol(
            "helper",
            &temp.path().join("source.gd"),
            &temp.path().join("dest.gd"),
            false,
            temp.path(),
            None,
            None,
            true,
        )
        .unwrap();
        assert!(result.applied);
        assert!(
            !result.callers_updated.is_empty(),
            "should have updated callers, got: {:?}",
            result.callers_updated
        );
        let caller = fs::read_to_string(temp.path().join("caller.gd")).unwrap();
        // Caller does NOT use Source.other(), so the preload should be redirected to dest.gd
        assert!(
            caller.contains("res://dest.gd"),
            "preload should point to dest.gd, got: {caller}"
        );
        assert!(
            !caller.contains("res://source.gd"),
            "old preload should be gone, got: {caller}"
        );
    }

    #[test]
    fn move_updates_caller_adds_preload_when_other_symbols_used() {
        // Caller uses both the moved symbol and another symbol from source.gd
        let temp = setup_project(&[
            (
                "source.gd",
                "func helper():\n\treturn 42\n\n\nfunc other():\n\tpass\n",
            ),
            (
                "caller.gd",
                "const Source = preload(\"res://source.gd\")\n\nfunc _ready():\n\tSource.helper()\n\tSource.other()\n",
            ),
        ]);
        let result = move_symbol(
            "helper",
            &temp.path().join("source.gd"),
            &temp.path().join("dest.gd"),
            false,
            temp.path(),
            None,
            None,
            true,
        )
        .unwrap();
        assert!(result.applied);
        assert!(
            !result.callers_updated.is_empty(),
            "should have updated callers"
        );
        let caller = fs::read_to_string(temp.path().join("caller.gd")).unwrap();
        // Should keep source.gd preload AND add dest.gd preload
        assert!(
            caller.contains("res://source.gd"),
            "should keep original preload, got: {caller}"
        );
        assert!(
            caller.contains("res://dest.gd"),
            "should add dest preload, got: {caller}"
        );
        // The moved symbol reference should be rewritten
        assert!(
            caller.contains("_Dest.helper()"),
            "should rewrite Source.helper() to _Dest.helper(), got: {caller}"
        );
        // The other symbol should still use Source
        assert!(
            caller.contains("Source.other()"),
            "should keep Source.other(), got: {caller}"
        );
    }

    #[test]
    fn move_dry_run_does_not_update_callers() {
        let temp = setup_project(&[
            ("source.gd", "func helper():\n\treturn 42\n"),
            (
                "caller.gd",
                "const Source = preload(\"res://source.gd\")\n\nfunc _ready():\n\tSource.helper()\n",
            ),
        ]);
        let result = move_symbol(
            "helper",
            &temp.path().join("source.gd"),
            &temp.path().join("dest.gd"),
            true,
            temp.path(),
            None,
            None,
            true,
        )
        .unwrap();
        assert!(!result.applied);
        assert!(
            result.callers_updated.is_empty(),
            "dry run should not update callers"
        );
        let caller = fs::read_to_string(temp.path().join("caller.gd")).unwrap();
        assert!(
            caller.contains("res://source.gd"),
            "caller should be unchanged in dry run"
        );
    }

    #[test]
    fn move_updates_inline_preload() {
        // Caller uses preload inline (not assigned to a variable)
        let temp = setup_project(&[
            ("source.gd", "func helper():\n\treturn 42\n"),
            (
                "caller.gd",
                "func _ready():\n\tvar h = preload(\"res://source.gd\").helper()\n",
            ),
        ]);
        let result = move_symbol(
            "helper",
            &temp.path().join("source.gd"),
            &temp.path().join("dest.gd"),
            false,
            temp.path(),
            None,
            None,
            true,
        )
        .unwrap();
        assert!(result.applied);
        assert!(
            !result.callers_updated.is_empty(),
            "should have updated callers"
        );
        let caller = fs::read_to_string(temp.path().join("caller.gd")).unwrap();
        assert!(
            caller.contains("res://dest.gd"),
            "inline preload should be updated, got: {caller}"
        );
    }

    #[test]
    fn derive_preload_var_name_basic() {
        assert_eq!(derive_preload_var_name("res://helpers.gd"), "_Helpers");
        assert_eq!(
            derive_preload_var_name("res://math_helpers.gd"),
            "_MathHelpers"
        );
        assert_eq!(
            derive_preload_var_name("res://utils/string_util.gd"),
            "_StringUtil"
        );
    }

    #[test]
    fn move_multiple_callers_updated() {
        let temp = setup_project(&[
            ("source.gd", "func helper():\n\treturn 42\n"),
            (
                "caller_a.gd",
                "const Source = preload(\"res://source.gd\")\n\nfunc _ready():\n\tSource.helper()\n",
            ),
            (
                "caller_b.gd",
                "const Source = preload(\"res://source.gd\")\n\nfunc _ready():\n\tSource.helper()\n",
            ),
        ]);
        let result = move_symbol(
            "helper",
            &temp.path().join("source.gd"),
            &temp.path().join("dest.gd"),
            false,
            temp.path(),
            None,
            None,
            true,
        )
        .unwrap();
        assert!(result.applied);
        assert_eq!(
            result.callers_updated.len(),
            2,
            "should update both callers, got: {:?}",
            result.callers_updated
        );
    }

    #[test]
    fn move_no_caller_update_when_flag_false() {
        let temp = setup_project(&[
            ("source.gd", "func helper():\n\treturn 42\n"),
            (
                "caller.gd",
                "const Source = preload(\"res://source.gd\")\n\nfunc _ready():\n\tSource.helper()\n",
            ),
        ]);
        let result = move_symbol(
            "helper",
            &temp.path().join("source.gd"),
            &temp.path().join("dest.gd"),
            false,
            temp.path(),
            None,
            None,
            false, // update_callers = false
        )
        .unwrap();
        assert!(result.applied);
        assert!(
            result.callers_updated.is_empty(),
            "should not update callers when flag is false"
        );
        let caller = fs::read_to_string(temp.path().join("caller.gd")).unwrap();
        assert!(
            caller.contains("res://source.gd"),
            "caller should be unchanged when update_callers is false"
        );
    }
}
