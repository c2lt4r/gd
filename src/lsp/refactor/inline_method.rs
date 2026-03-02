use std::collections::HashMap;
use std::fmt::Write;
use std::path::{Path, PathBuf};

use miette::Result;
use serde::Serialize;
use tree_sitter::Node;

use super::{declaration_full_range, find_declaration_by_name, line_starts, normalize_blank_lines};
use crate::core::gd_ast;
use crate::core::workspace_index::ProjectIndex;

// ── inline-method ───────────────────────────────────────────────────────────

#[derive(Serialize, Debug)]
pub struct InlineMethodOutput {
    pub function: String,
    pub call_site_file: String,
    pub call_site_line: u32,
    pub inlined_lines: u32,
    pub function_deleted: bool,
    pub applied: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_file: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

#[allow(clippy::too_many_lines)]
pub fn inline_method(
    file: &Path,
    line: usize,   // 1-based
    column: usize, // 1-based
    dry_run: bool,
    project_root: &Path,
) -> Result<InlineMethodOutput> {
    let source =
        std::fs::read_to_string(file).map_err(|e| miette::miette!("cannot read file: {e}"))?;
    let tree = crate::core::parser::parse(&source)?;
    let root = tree.root_node();
    let file_ast = gd_ast::convert(&tree, &source);

    let point = tree_sitter::Point::new(line - 1, column - 1);

    // Find call node at cursor — handles both `call` and `attribute` (method call) nodes
    let call_node = find_call_at(root, point)
        .ok_or_else(|| miette::miette!("no function call found at {line}:{column}"))?;

    // Resolve the call: determine the method name, receiver object (if any),
    // and the source file + parsed tree containing the definition.
    let call_info = resolve_call_target(call_node, &source, root, file, project_root)?;

    let def_source;
    let def_tree;
    let func_def;

    // Track cross-file source info for the output
    let mut cross_file_relative: Option<String> = None;

    // If the method is in another file (typed var or cross-file bare call), parse that file.
    // If it's in the same file, use the already-parsed tree.
    if let Some(ref target_file) = call_info.target_file {
        def_source = std::fs::read_to_string(target_file)
            .map_err(|e| miette::miette!("cannot read target file: {e}"))?;
        def_tree = crate::core::parser::parse(&def_source)?;
        let def_file = gd_ast::convert(&def_tree, &def_source);
        func_def =
            find_declaration_by_name(&def_file, &call_info.method_name).ok_or_else(|| {
                miette::miette!(
                    "cannot find definition of '{}' in {}",
                    call_info.method_name,
                    target_file.display()
                )
            })?;
        cross_file_relative = Some(crate::core::fs::relative_slash(target_file, project_root));
    } else {
        // Same-file: check if the definition actually exists here
        let same_file_def = find_declaration_by_name(&file_ast, &call_info.method_name);
        if let Some(local_def) = same_file_def {
            def_source = source.clone();
            def_tree = crate::core::parser::parse(&def_source)?;
            let def_file = gd_ast::convert(&def_tree, &def_source);
            func_def =
                find_declaration_by_name(&def_file, &call_info.method_name).unwrap_or(local_def);
        } else {
            // Bare call not found in same file — try cross-file resolution
            let cross = resolve_cross_file_function(&call_info.method_name, file, project_root)?;
            cross_file_relative = Some(crate::core::fs::relative_slash(&cross.path, project_root));
            def_source = cross.source;
            def_tree = crate::core::parser::parse(&def_source)?;
            let def_file = gd_ast::convert(&def_tree, &def_source);
            func_def =
                find_declaration_by_name(&def_file, &call_info.method_name).ok_or_else(|| {
                    miette::miette!(
                        "cannot find definition of '{}' in resolved file",
                        call_info.method_name
                    )
                })?;
        }
    }

    let func_name = &call_info.method_name;
    let is_cross_file = cross_file_relative.is_some();

    if !matches!(
        func_def.kind(),
        "function_definition" | "constructor_definition"
    ) {
        return Err(miette::miette!("'{func_name}' is not a function"));
    }

    let func_body = func_def
        .child_by_field_name("body")
        .ok_or_else(|| miette::miette!("function has no body"))?;

    // Collect body statements (skip comments)
    let body_stmts: Vec<Node> = {
        let mut c = func_body.walk();
        func_body
            .children(&mut c)
            .filter(|n| n.kind() != "comment")
            .collect()
    };

    if body_stmts.is_empty() {
        return Err(miette::miette!("function body is empty"));
    }

    // Check for multiple return statements recursively (only allow single trailing return)
    let mut return_count = 0;
    for stmt in &body_stmts {
        count_return_statements(*stmt, &mut return_count);
    }
    if return_count > 1 {
        return Err(miette::miette!(
            "cannot inline function with multiple return statements"
        ));
    }
    // If there's a return, it must be the last top-level statement
    if return_count == 1
        && body_stmts.last().map(tree_sitter::Node::kind) != Some("return_statement")
    {
        return Err(miette::miette!(
            "cannot inline function with non-trailing return statement"
        ));
    }

    // Check for recursion
    for stmt in &body_stmts {
        if contains_call_to(*stmt, func_name, &def_source) {
            return Err(miette::miette!(
                "cannot inline recursive function '{func_name}'"
            ));
        }
    }

    // Parse call arguments (from the call site, which is in `source`)
    let call_args = extract_call_arguments(call_node, &source);

    // Parse function parameters (from the definition, which is in `def_source`)
    let func_params = extract_function_params(func_def, &def_source);

    // Build parameter -> argument mapping
    let mut param_map: HashMap<String, String> = HashMap::new();
    for (i, param) in func_params.iter().enumerate() {
        let arg = call_args
            .get(i)
            .map(std::string::String::as_str)
            .or(param.default.as_deref())
            .unwrap_or(&param.name);
        param_map.insert(param.name.clone(), arg.to_string());
    }

    // Extract body text and do parameter substitution
    let body_start = body_stmts[0].start_byte();
    let body_end = body_stmts.last().unwrap().end_byte();
    let body_text = &def_source[body_start..body_end];

    let mut substituted =
        substitute_params(body_text, &param_map, &body_stmts, body_start, &def_source);

    // For attribute calls, replace `self.` references in the inlined body
    // with the receiver expression (e.g., `self.health` -> `enemy.health`)
    if let Some(ref receiver) = call_info.receiver_expr
        && receiver != "self"
    {
        substituted = replace_self_references(&substituted, receiver);
    }

    // Handle return value
    // Note: we extract the return expression from `substituted` by text matching,
    // NOT by AST byte offsets, because parameter substitution may have changed string lengths.
    let has_return = return_count == 1;
    let (inlined_text, return_expr) = if has_return {
        // Find the last line starting with "return" in the substituted text
        let lines: Vec<&str> = substituted.lines().collect();
        let ret_expr_text = lines
            .iter()
            .rev()
            .find(|l| l.trim_start().starts_with("return"))
            .map(|l| {
                let trimmed = l.trim_start();
                if trimmed.len() > 6 {
                    trimmed[6..].trim_start().to_string()
                } else {
                    String::new()
                }
            })
            .unwrap_or_default();

        if body_stmts.len() == 1 {
            // Single return statement — just use the expression
            (String::new(), Some(ret_expr_text))
        } else {
            // Multiple statements + trailing return — everything except the last return line
            let last_return_idx = lines
                .iter()
                .rposition(|l| l.trim_start().starts_with("return"))
                .unwrap_or(lines.len());
            let prefix = lines[..last_return_idx].join("\n");
            (prefix, Some(ret_expr_text))
        }
    } else {
        (substituted.clone(), None)
    };

    // Get call site context
    let call_line = call_node.start_position().row;
    let call_indent = get_indent(&source, call_line);

    // Build the inlined code
    let mut inlined_lines_text = String::new();

    // Check if call is part of an assignment (var x = func() or x = func())
    let call_parent = call_node.parent();
    let is_assignment = call_parent.is_some_and(|p| {
        matches!(
            p.kind(),
            "assignment" | "augmented_assignment" | "variable_statement"
        )
    });

    if !inlined_text.is_empty() {
        // Re-indent non-return body statements
        let re_indented = re_indent_to_depth_with_indent(&inlined_text, &call_indent);
        inlined_lines_text.push_str(&re_indented);
        if !inlined_lines_text.ends_with('\n') {
            inlined_lines_text.push('\n');
        }
    }

    if let Some(ref ret_expr) = return_expr {
        if is_assignment {
            // Replace call in assignment with return expression
            if let Some(parent) = call_parent {
                if parent.kind() == "variable_statement" {
                    // var x = func() -> var x = expr + body before
                    let var_name = parent
                        .child_by_field_name("name")
                        .and_then(|n| n.utf8_text(source.as_bytes()).ok())
                        .unwrap_or("x");
                    let _ = writeln!(
                        inlined_lines_text,
                        "{call_indent}var {var_name} = {ret_expr}"
                    );
                } else {
                    // x = func() -> body + x = expr
                    let left = parent
                        .child_by_field_name("left")
                        .and_then(|n| n.utf8_text(source.as_bytes()).ok())
                        .unwrap_or("x");
                    let _ = writeln!(inlined_lines_text, "{call_indent}{left} = {ret_expr}");
                }
            }
        } else {
            // Standalone call with return -> just add the expression (discard return value)
            if inlined_text.is_empty() {
                let _ = writeln!(inlined_lines_text, "{call_indent}{ret_expr}");
            }
            // else: Body already added above; the return value is discarded
        }
    } else if inlined_text.is_empty() {
        // Void function, single `pass` -> remove the call line entirely
        let _ = writeln!(inlined_lines_text, "{call_indent}pass");
    }

    let total_inlined = inlined_lines_text.lines().count() as u32;

    // For cross-file inlining, never delete the function (it belongs to another class)
    let can_delete = if is_cross_file {
        false
    } else {
        let workspace = crate::lsp::workspace::WorkspaceIndex::new(project_root.to_path_buf());
        let all_refs =
            crate::lsp::references::find_references_by_name(func_name, &workspace, None, None);
        let func_def_start = func_def.start_position().row as u32;
        let func_def_end = func_def.end_position().row as u32;
        let file_uri = tower_lsp::lsp_types::Url::from_file_path(file).ok();
        let call_count = all_refs
            .iter()
            .filter(|loc| {
                if let Some(ref uri) = file_uri
                    && &loc.uri == uri
                {
                    let ref_line = loc.range.start.line;
                    // Don't count references within the function definition itself
                    if ref_line >= func_def_start && ref_line <= func_def_end {
                        return false;
                    }
                }
                true
            })
            .count();
        call_count <= 1
    };

    let mut warnings = Vec::new();
    if !can_delete && !is_cross_file {
        warnings.push(format!(
            "function '{func_name}' has multiple call sites \u{2014} not deleted"
        ));
    }
    if is_cross_file {
        if let Some(ref cross_rel) = cross_file_relative {
            warnings.push(format!(
                "function '{func_name}' inlined from '{cross_rel}' (not deleted from source)"
            ));
        } else {
            warnings.push(format!(
                "function '{func_name}' is defined in another file \u{2014} not deleted"
            ));
        }
    }

    // Check for self. usage — only warn if we did NOT already substitute self refs
    if call_info.receiver_expr.is_none() {
        let self_refs = collect_self_references(func_def, &def_source);
        if !self_refs.is_empty() {
            if is_cross_file {
                warnings.push(
                    "inlined code contains 'self.' references \u{2014} may not work in this context"
                        .to_string(),
                );
            } else {
                warnings.push("inlined code contains 'self.' references".to_string());
            }
        }
    }

    let relative_file = crate::core::fs::relative_slash(file, project_root);

    if !dry_run {
        let starts = line_starts(&source);
        let mut new_source = source.clone();

        // 1. Replace the call site line(s)
        let call_stmt_node = if is_assignment {
            call_parent.unwrap_or(call_node)
        } else {
            call_node
        };
        let stmt_start_line = call_stmt_node.start_position().row;
        let stmt_end_line = call_stmt_node.end_position().row;

        let replace_start = starts[stmt_start_line];
        let replace_end = if stmt_end_line + 1 < starts.len() {
            starts[stmt_end_line + 1]
        } else {
            source.len()
        };

        new_source.replace_range(replace_start..replace_end, &inlined_lines_text);

        // 2. Delete function definition if single callsite (same-file only)
        if can_delete {
            let new_tree = crate::core::parser::parse(&new_source)?;
            let new_file = gd_ast::convert(&new_tree, &new_source);
            if let Some(def) = find_declaration_by_name(&new_file, func_name) {
                let (def_start, def_end) = declaration_full_range(def, &new_source);
                let mut final_source = String::with_capacity(new_source.len());
                final_source.push_str(&new_source[..def_start]);
                final_source.push_str(&new_source[def_end..]);
                normalize_blank_lines(&mut final_source);
                new_source = final_source;
            }
        }

        normalize_blank_lines(&mut new_source);
        std::fs::write(file, &new_source).map_err(|e| miette::miette!("cannot write file: {e}"))?;

        let undo_desc = if let Some(ref cross_rel) = cross_file_relative {
            format!("inline {func_name} from {cross_rel}")
        } else {
            format!("inline {func_name}")
        };
        let mut snaps: HashMap<PathBuf, Option<Vec<u8>>> = HashMap::new();
        snaps.insert(file.to_path_buf(), Some(source.as_bytes().to_vec()));
        let stack = super::undo::UndoStack::open(project_root);
        let _ = stack.record("inline-method", &undo_desc, &snaps, project_root);
    }

    Ok(InlineMethodOutput {
        function: func_name.clone(),
        call_site_file: relative_file,
        call_site_line: line as u32,
        inlined_lines: total_inlined,
        function_deleted: can_delete && !dry_run,
        applied: !dry_run,
        source_file: cross_file_relative,
        warnings,
    })
}

// ── inline-method by name ────────────────────────────────────────────────────

#[derive(Serialize, Debug)]
pub struct InlineMethodByNameOutput {
    pub function: String,
    pub file: String,
    pub call_sites_inlined: u32,
    pub function_deleted: bool,
    pub applied: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

/// Inline all (or list) call sites of a function by name within a file.
/// With `all=true`, inlines every call site and deletes the function.
/// With `all=false`, reports call sites in dry-run style.
///
/// If the function definition is not found in the file, searches the project
/// cross-file and inlines from the external definition.
pub fn inline_method_by_name(
    file: &Path,
    name: &str,
    all: bool,
    dry_run: bool,
    project_root: &Path,
) -> Result<InlineMethodByNameOutput> {
    let source =
        std::fs::read_to_string(file).map_err(|e| miette::miette!("cannot read file: {e}"))?;
    let tree = crate::core::parser::parse(&source)?;
    let root = tree.root_node();
    let file_ast = gd_ast::convert(&tree, &source);

    // Try same-file definition first
    let same_file_def = find_declaration_by_name(&file_ast, name);

    if let Some(func_def) = same_file_def {
        inline_by_name_same_file(
            func_def,
            root,
            name,
            &source,
            file,
            all,
            dry_run,
            project_root,
        )
    } else {
        inline_by_name_cross_file(root, name, &source, file, all, dry_run, project_root)
    }
}

#[allow(clippy::too_many_arguments)]
fn inline_by_name_same_file(
    func_def: Node<'_>,
    root: Node<'_>,
    name: &str,
    source: &str,
    file: &Path,
    all: bool,
    dry_run: bool,
    project_root: &Path,
) -> Result<InlineMethodByNameOutput> {
    if !matches!(
        func_def.kind(),
        "function_definition" | "constructor_definition"
    ) {
        return Err(miette::miette!("'{name}' is not a function"));
    }

    let func_def_start = func_def.start_position().row;
    let func_def_end = func_def.end_position().row;

    // Find all call sites of this function in the file
    let mut call_sites: Vec<(usize, usize)> = Vec::new();
    collect_calls_to(
        root,
        name,
        source,
        func_def_start,
        func_def_end,
        &mut call_sites,
    );

    if call_sites.is_empty() {
        return Err(miette::miette!(
            "no call sites for '{name}' found in this file"
        ));
    }

    let relative_file = crate::core::fs::relative_slash(file, project_root);
    let mut warnings = Vec::new();

    if !all && call_sites.len() > 1 {
        warnings.push(format!(
            "function '{name}' has {} call sites \u{2014} use --all to inline all",
            call_sites.len()
        ));
    }

    let sites_to_inline = if all {
        call_sites.clone()
    } else {
        // Just inline the first call site
        vec![call_sites[0]]
    };

    let mut inlined_count = 0u32;

    if dry_run {
        inlined_count = sites_to_inline.len() as u32;
    } else {
        // Inline call sites from bottom to top to preserve line numbers
        let mut sorted_sites = sites_to_inline.clone();
        sorted_sites.sort_by(|a, b| b.0.cmp(&a.0));

        for (line, column) in &sorted_sites {
            // Re-read and re-parse after each inline (source changes)
            match inline_method(file, *line, *column, false, project_root) {
                Ok(_) => inlined_count += 1,
                Err(e) => warnings.push(format!("failed to inline at {line}:{column}: {e}")),
            }
        }

        // If we inlined all sites and the function still exists, delete it
        if all && inlined_count == sites_to_inline.len() as u32 {
            let current_source = std::fs::read_to_string(file)
                .map_err(|e| miette::miette!("cannot read file: {e}"))?;
            let current_tree = crate::core::parser::parse(&current_source)?;
            let current_file = gd_ast::convert(&current_tree, &current_source);
            if let Some(def) = find_declaration_by_name(&current_file, name) {
                let (def_start, def_end) = declaration_full_range(def, &current_source);
                let mut final_source = String::with_capacity(current_source.len());
                final_source.push_str(&current_source[..def_start]);
                final_source.push_str(&current_source[def_end..]);
                normalize_blank_lines(&mut final_source);
                std::fs::write(file, &final_source)
                    .map_err(|e| miette::miette!("cannot write file: {e}"))?;

                let mut snaps: HashMap<PathBuf, Option<Vec<u8>>> = HashMap::new();
                snaps.insert(file.to_path_buf(), Some(current_source.into_bytes()));
                let stack = super::undo::UndoStack::open(project_root);
                let _ = stack.record(
                    "inline-method",
                    &format!("delete {name} after inline-all"),
                    &snaps,
                    project_root,
                );
            }
        }
    }

    // Check if function was deleted (either by inline_method for single callsite,
    // or by our explicit deletion above for --all)
    let function_deleted = if !dry_run && inlined_count > 0 {
        let current_source =
            std::fs::read_to_string(file).map_err(|e| miette::miette!("cannot read file: {e}"))?;
        let current_tree = crate::core::parser::parse(&current_source)?;
        let current_file = gd_ast::convert(&current_tree, &current_source);
        find_declaration_by_name(&current_file, name).is_none()
    } else {
        false
    };

    Ok(InlineMethodByNameOutput {
        function: name.to_string(),
        file: relative_file,
        call_sites_inlined: inlined_count,
        function_deleted,
        applied: !dry_run,
        warnings,
    })
}

/// Cross-file variant: the function is defined in another file, but called from this file.
fn inline_by_name_cross_file(
    root: Node<'_>,
    name: &str,
    source: &str,
    file: &Path,
    all: bool,
    dry_run: bool,
    project_root: &Path,
) -> Result<InlineMethodByNameOutput> {
    // Verify the function exists somewhere in the project
    let _cross = resolve_cross_file_function(name, file, project_root)?;

    // Find all call sites of this function in the current file.
    // Since the definition is cross-file, use usize::MAX range to skip no calls.
    let mut call_sites: Vec<(usize, usize)> = Vec::new();
    collect_calls_to(root, name, source, usize::MAX, usize::MAX, &mut call_sites);

    if call_sites.is_empty() {
        return Err(miette::miette!(
            "no call sites for '{name}' found in this file"
        ));
    }

    let relative_file = crate::core::fs::relative_slash(file, project_root);
    let mut warnings = Vec::new();

    if !all && call_sites.len() > 1 {
        warnings.push(format!(
            "function '{name}' has {} call sites \u{2014} use --all to inline all",
            call_sites.len()
        ));
    }

    let sites_to_inline = if all {
        call_sites.clone()
    } else {
        vec![call_sites[0]]
    };

    let mut inlined_count = 0u32;

    if dry_run {
        inlined_count = sites_to_inline.len() as u32;
    } else {
        // Inline call sites from bottom to top to preserve line numbers
        let mut sorted_sites = sites_to_inline.clone();
        sorted_sites.sort_by(|a, b| b.0.cmp(&a.0));

        for (line, column) in &sorted_sites {
            match inline_method(file, *line, *column, false, project_root) {
                Ok(_) => inlined_count += 1,
                Err(e) => warnings.push(format!("failed to inline at {line}:{column}: {e}")),
            }
        }
    }

    Ok(InlineMethodByNameOutput {
        function: name.to_string(),
        file: relative_file,
        call_sites_inlined: inlined_count,
        function_deleted: false, // Never delete from other files
        applied: !dry_run,
        warnings,
    })
}

// ── Call resolution ─────────────────────────────────────────────────────────

/// Information about a resolved call target.
struct CallInfo {
    /// The method name to look up (e.g., `helper` for `self.helper()`).
    method_name: String,
    /// The receiver expression text (e.g., `"self"`, `"enemy"`), or `None` for bare calls.
    receiver_expr: Option<String>,
    /// If the definition is in another file, the path to that file.
    /// `None` means the definition is in the same file as the call site.
    target_file: Option<PathBuf>,
}

/// Resolve a call node to determine where to find the method definition.
///
/// Handles three cases:
/// - Bare `call` node: `helper()` -> look up `helper` in the same file
/// - `attribute` node with `self`: `self.helper()` -> look up `helper` in the same file
/// - `attribute` node with typed var: `enemy.take_damage()` -> resolve type to find target file
///
/// Tree-sitter-gdscript AST shapes:
/// - Bare call: `call { identifier("helper") arguments }`
/// - Attribute call: `attribute { identifier("self") "." attribute_call { identifier("helper") arguments } }`
fn resolve_call_target(
    call_node: Node,
    source: &str,
    root: Node,
    call_site_file: &Path,
    project_root: &Path,
) -> Result<CallInfo> {
    if call_node.kind() == "call" {
        // Bare call: `func_name()`
        let func_name_node = call_node
            .child_by_field_name("function")
            .or_else(|| call_node.named_child(0))
            .ok_or_else(|| miette::miette!("cannot determine function name from call"))?;

        // Reject nested attribute calls that somehow got here
        if func_name_node.kind() == "attribute" {
            return Err(miette::miette!(
                "cannot inline method calls via call node with attribute function"
            ));
        }

        let func_name = func_name_node
            .utf8_text(source.as_bytes())
            .map_err(|e| miette::miette!("cannot read function name: {e}"))?;
        return Ok(CallInfo {
            method_name: func_name.to_string(),
            receiver_expr: None,
            target_file: None,
        });
    }

    // Attribute call: `obj.method()`
    // AST: attribute { identifier(obj) "." attribute_call { identifier(method) arguments } }
    if call_node.kind() == "attribute" {
        let obj_node = call_node
            .named_child(0)
            .ok_or_else(|| miette::miette!("cannot determine receiver object"))?;

        // Find the attribute_call child which contains the method name
        let attr_call = {
            let mut cursor = call_node.walk();
            call_node
                .children(&mut cursor)
                .find(|c| c.kind() == "attribute_call")
                .ok_or_else(|| miette::miette!("cannot find attribute_call in attribute node"))?
        };

        // The method name is the first named child of attribute_call
        let method_name_node = attr_call
            .named_child(0)
            .ok_or_else(|| miette::miette!("cannot determine method name from attribute call"))?;

        let obj_text = obj_node
            .utf8_text(source.as_bytes())
            .map_err(|e| miette::miette!("cannot read receiver: {e}"))?;
        let method_name = method_name_node
            .utf8_text(source.as_bytes())
            .map_err(|e| miette::miette!("cannot read method name: {e}"))?;

        if obj_text == "self" {
            // `self.method()` -> same file
            return Ok(CallInfo {
                method_name: method_name.to_string(),
                receiver_expr: Some("self".to_string()),
                target_file: None,
            });
        }

        // `obj.method()` -> resolve the type of `obj`
        let type_name =
            resolve_receiver_type(obj_text, root, source, call_site_file, project_root)?;

        // Look up the type in the project index to find the file
        let index = ProjectIndex::build(project_root);
        let file_symbols = index.lookup_class(&type_name).ok_or_else(|| {
            miette::miette!(
                "cannot resolve type '{type_name}' to a file \u{2014} class_name not found in project"
            )
        })?;

        return Ok(CallInfo {
            method_name: method_name.to_string(),
            receiver_expr: Some(obj_text.to_string()),
            target_file: Some(file_symbols.path.clone()),
        });
    }

    Err(miette::miette!(
        "unexpected node kind '{}' \u{2014} expected call or attribute",
        call_node.kind()
    ))
}

/// Resolve the type of a receiver variable by checking:
/// 1. Type annotations on variable declarations (`var enemy: Enemy`)
/// 2. Typed function parameters (`func attack(enemy: Enemy)`)
fn resolve_receiver_type(
    var_name: &str,
    root: Node,
    source: &str,
    _call_site_file: &Path,
    _project_root: &Path,
) -> Result<String> {
    // Walk the AST to find the variable declaration
    if let Some(type_name) = find_var_type_annotation(root, var_name, source) {
        return Ok(type_name);
    }

    Err(miette::miette!(
        "cannot determine type of '{var_name}' \u{2014} add a type annotation (var {var_name}: TypeName)"
    ))
}

/// Find a variable's explicit type annotation in the AST.
/// Searches for `var name: Type` or `var name: Type = ...` patterns.
fn find_var_type_annotation(root: Node, var_name: &str, source: &str) -> Option<String> {
    find_var_type_recursive(root, var_name, source)
}

fn find_var_type_recursive(node: Node, var_name: &str, source: &str) -> Option<String> {
    // Check variable_statement or const_statement nodes for type annotations
    if (node.kind() == "variable_statement" || node.kind() == "const_statement")
        && let Some(name_node) = node.child_by_field_name("name")
        && name_node.utf8_text(source.as_bytes()).ok() == Some(var_name)
    {
        return node
            .child_by_field_name("type")
            .and_then(|t| t.utf8_text(source.as_bytes()).ok())
            .map(std::string::ToString::to_string);
    }

    // Check function parameters (typed_parameter)
    if (node.kind() == "typed_parameter" || node.kind() == "typed_default_parameter")
        && let Some(name_node) = node.child(0)
        && name_node.utf8_text(source.as_bytes()).ok() == Some(var_name)
    {
        return node
            .child_by_field_name("type")
            .and_then(|t| t.utf8_text(source.as_bytes()).ok())
            .map(std::string::ToString::to_string);
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if let Some(t) = find_var_type_recursive(child, var_name, source) {
            return Some(t);
        }
    }
    None
}

// ── Cross-file resolution ───────────────────────────────────────────────────

/// Information about a function resolved from another file.
struct CrossFileFunc {
    /// Path to the file containing the function.
    path: PathBuf,
    /// Full source of the file containing the function.
    source: String,
}

/// Search the project for a function definition by name, excluding the current file.
fn resolve_cross_file_function(
    func_name: &str,
    current_file: &Path,
    project_root: &Path,
) -> Result<CrossFileFunc> {
    let index = ProjectIndex::build(project_root);

    let mut matches: Vec<PathBuf> = Vec::new();

    for fs in index.files() {
        if fs.path == current_file {
            continue;
        }
        if fs.functions.iter().any(|f| f.name == func_name) {
            matches.push(fs.path.clone());
        }
    }

    match matches.len() {
        0 => Err(miette::miette!(
            "cannot find definition of '{func_name}' in this file or any project file"
        )),
        1 => {
            let path = matches.remove(0);
            let source = std::fs::read_to_string(&path)
                .map_err(|e| miette::miette!("cannot read {}: {e}", path.display()))?;
            Ok(CrossFileFunc { path, source })
        }
        n => {
            let file_list: Vec<String> = matches
                .iter()
                .map(|p| crate::core::fs::relative_slash(p, project_root))
                .collect();
            Err(miette::miette!(
                "'{func_name}' found in {n} files (ambiguous): {}",
                file_list.join(", ")
            ))
        }
    }
}

// ── Self-reference substitution ─────────────────────────────────────────────

/// Replace `self.` references in inlined text with the receiver expression.
/// For example, if receiver is `enemy`, then `self.health` -> `enemy.health`.
fn replace_self_references(text: &str, receiver: &str) -> String {
    // Parse the text to find `self.` patterns and replace them properly.
    // We use a simple text-based approach since the body text has already been
    // extracted and is not a full valid GDScript file.
    let mut result = String::with_capacity(text.len());
    let bytes = text.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i..].starts_with(b"self.") {
            // Check that 'self' is at a word boundary (not part of a larger identifier)
            let at_word_boundary = i == 0 || !is_ident_char(bytes[i - 1]);
            if at_word_boundary {
                result.push_str(receiver);
                result.push('.');
                i += 5; // skip "self."
                continue;
            }
        }
        // Also handle bare `self` not followed by `.` (e.g., passing self as argument)
        if bytes[i..].starts_with(b"self")
            && (i == 0 || !is_ident_char(bytes[i - 1]))
            && (i + 4 >= bytes.len() || !is_ident_char(bytes[i + 4]))
        {
            result.push_str(receiver);
            i += 4;
            continue;
        }
        result.push(bytes[i] as char);
        i += 1;
    }

    result
}

/// Check if a byte is a valid GDScript identifier character.
fn is_ident_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

// ── Private helpers ─────────────────────────────────────────────────────────

/// Collect all call sites of `func_name` in the AST, excluding those within
/// the function definition itself.
fn collect_calls_to(
    node: Node,
    func_name: &str,
    source: &str,
    func_def_start: usize,
    func_def_end: usize,
    out: &mut Vec<(usize, usize)>,
) {
    if node.kind() == "call" {
        let callee = node
            .child_by_field_name("function")
            .or_else(|| node.named_child(0));
        if let Some(callee) = callee
            && let Ok(name) = callee.utf8_text(source.as_bytes())
            && name == func_name
            && callee.kind() != "attribute"
        {
            let row = node.start_position().row;
            // Skip calls inside the function definition itself
            if row < func_def_start || row > func_def_end {
                out.push((row + 1, node.start_position().column + 1));
            }
        }
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_calls_to(child, func_name, source, func_def_start, func_def_end, out);
    }
}

/// Count return statements recursively in a node subtree.
fn count_return_statements(node: Node, count: &mut usize) {
    if node.kind() == "return_statement" {
        *count += 1;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        count_return_statements(child, count);
    }
}

/// Find a `call` or attribute method call node at the given point.
///
/// Handles both bare calls (`helper()` -> `call` node) and attribute calls
/// (`self.helper()` or `obj.method()` -> `attribute` node containing `attribute_call`).
pub(super) fn find_call_at(root: Node<'_>, point: tree_sitter::Point) -> Option<Node<'_>> {
    let leaf = root.descendant_for_point_range(point, point)?;
    let mut node = leaf;
    loop {
        if node.kind() == "call" {
            return Some(node);
        }
        // For attribute method calls like `self.helper()` or `obj.method()`,
        // the tree-sitter-gdscript AST is:
        //   attribute { identifier("self") "." attribute_call { identifier("helper") arguments } }
        // We treat the outer `attribute` as the call node.
        if node.kind() == "attribute" && has_attribute_call_child(node) {
            return Some(node);
        }
        node = node.parent()?;
    }
}

/// Check if a node has a direct `attribute_call` child.
fn has_attribute_call_child(node: Node) -> bool {
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .any(|c| c.kind() == "attribute_call")
}

/// Extract argument text strings from a call node.
///
/// Handles both `call` nodes (bare calls) and `attribute` nodes (method calls).
/// For `attribute` nodes, arguments are inside the `attribute_call` child.
pub(super) fn extract_call_arguments(call: Node, source: &str) -> Vec<String> {
    // For attribute nodes, find the attribute_call child which holds the arguments
    let arg_source = if call.kind() == "attribute" {
        let mut cursor = call.walk();
        call.children(&mut cursor)
            .find(|c| c.kind() == "attribute_call")
    } else {
        Some(call)
    };

    let mut args = Vec::new();
    if let Some(source_node) = arg_source
        && let Some(arg_node) = source_node.child_by_field_name("arguments")
    {
        let mut cursor = arg_node.walk();
        for child in arg_node.children(&mut cursor) {
            if child.is_named()
                && child.kind() != "("
                && child.kind() != ")"
                && child.kind() != ","
                && let Ok(text) = child.utf8_text(source.as_bytes())
            {
                args.push(text.to_string());
            }
        }
    }
    args
}

#[derive(Debug)]
pub(super) struct ParamInfo {
    pub(super) name: String,
    #[allow(dead_code)]
    pub(super) type_hint: Option<String>,
    pub(super) default: Option<String>,
}

/// Extract function parameter info from a function definition.
pub(super) fn extract_function_params(func: Node, source: &str) -> Vec<ParamInfo> {
    let mut params = Vec::new();
    let Some(params_node) = func.child_by_field_name("parameters") else {
        return params;
    };
    let mut cursor = params_node.walk();
    if cursor.goto_first_child() {
        loop {
            let child = cursor.node();
            match child.kind() {
                "identifier" => {
                    params.push(ParamInfo {
                        name: source[child.byte_range()].to_string(),
                        type_hint: None,
                        default: None,
                    });
                }
                "typed_parameter" => {
                    if let Some(name_node) = child.child(0) {
                        let type_hint = child
                            .child_by_field_name("type")
                            .and_then(|t| t.utf8_text(source.as_bytes()).ok())
                            .map(std::string::ToString::to_string);
                        params.push(ParamInfo {
                            name: source[name_node.byte_range()].to_string(),
                            type_hint,
                            default: None,
                        });
                    }
                }
                "default_parameter" => {
                    if let Some(name_node) = child.child(0) {
                        let default = child
                            .child_by_field_name("value")
                            .and_then(|v| v.utf8_text(source.as_bytes()).ok())
                            .map(std::string::ToString::to_string);
                        params.push(ParamInfo {
                            name: source[name_node.byte_range()].to_string(),
                            type_hint: None,
                            default,
                        });
                    }
                }
                "typed_default_parameter" => {
                    if let Some(name_node) = child.child(0) {
                        let type_hint = child
                            .child_by_field_name("type")
                            .and_then(|t| t.utf8_text(source.as_bytes()).ok())
                            .map(std::string::ToString::to_string);
                        let default = child
                            .child_by_field_name("value")
                            .and_then(|v| v.utf8_text(source.as_bytes()).ok())
                            .map(std::string::ToString::to_string);
                        params.push(ParamInfo {
                            name: source[name_node.byte_range()].to_string(),
                            type_hint,
                            default,
                        });
                    }
                }
                _ => {}
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
    params
}

/// Substitute parameter identifiers with argument expressions in body text.
/// Uses byte-level replacement to avoid substring false matches.
fn substitute_params(
    body_text: &str,
    param_map: &HashMap<String, String>,
    stmts: &[Node],
    body_offset: usize,
    source: &str,
) -> String {
    if param_map.is_empty() {
        return body_text.to_string();
    }

    // Collect all identifier positions within the body that match param names
    let mut replacements: Vec<(usize, usize, String)> = Vec::new();
    for stmt in stmts {
        collect_param_replacements(*stmt, source, param_map, body_offset, &mut replacements);
    }

    // Sort replacements by position (reverse order for safe in-place replacement)
    replacements.sort_by(|a, b| b.0.cmp(&a.0));

    let mut result = body_text.to_string();
    for (start, end, replacement) in replacements {
        if start <= result.len() && end <= result.len() {
            result.replace_range(start..end, &replacement);
        }
    }
    result
}

fn collect_param_replacements(
    node: Node,
    source: &str,
    param_map: &HashMap<String, String>,
    body_offset: usize,
    replacements: &mut Vec<(usize, usize, String)>,
) {
    if (node.kind() == "identifier" || node.kind() == "name")
        && let Ok(text) = node.utf8_text(source.as_bytes())
        && let Some(replacement) = param_map.get(text)
    {
        let rel_start = node.start_byte() - body_offset;
        let rel_end = node.end_byte() - body_offset;
        replacements.push((rel_start, rel_end, replacement.clone()));
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_param_replacements(child, source, param_map, body_offset, replacements);
    }
}

/// Check if a node contains a call to a function with the given name.
fn contains_call_to(node: Node, name: &str, source: &str) -> bool {
    if node.kind() == "call" {
        let func_name = node
            .child_by_field_name("function")
            .or_else(|| node.named_child(0));
        if let Some(f) = func_name
            && f.utf8_text(source.as_bytes()).ok() == Some(name)
        {
            return true;
        }
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if contains_call_to(child, name, source) {
            return true;
        }
    }
    false
}

/// Re-indent text to match a target indent string (preserves relative indentation).
fn re_indent_to_depth_with_indent(text: &str, target_indent: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();

    let min_indent = lines
        .iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.len() - l.trim_start().len())
        .min()
        .unwrap_or(0);

    lines
        .iter()
        .map(|line| {
            if line.trim().is_empty() {
                String::new()
            } else if line.len() >= min_indent {
                format!("{target_indent}{}", &line[min_indent..])
            } else {
                format!("{target_indent}{line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
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
                // self.method() -> attribute_call's first named child is the name
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

// ── Indent helper ───────────────────────────────────────────────────────────

/// Get the indentation string (tabs/spaces) of a line (0-based).
fn get_indent(source: &str, line: usize) -> String {
    let line_text = source.lines().nth(line).unwrap_or("");
    let indent_len = line_text.len() - line_text.trim_start().len();
    line_text[..indent_len].to_string()
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

    // ── inline-method ────────────────────────────────────────────────────

    #[test]
    fn inline_void_function() {
        let temp = setup_project(&[(
            "player.gd",
            "func helper():\n\tprint(\"hello\")\n\n\nfunc _ready():\n\thelper()\n",
        )]);
        let result =
            inline_method(&temp.path().join("player.gd"), 6, 2, false, temp.path()).unwrap();
        assert!(result.applied);
        assert_eq!(result.function, "helper");
        assert!(result.function_deleted, "single callsite should delete");
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(
            content.contains("print(\"hello\")"),
            "should inline body, got: {content}"
        );
        assert!(
            !content.contains("func helper()"),
            "function should be deleted, got: {content}"
        );
    }

    #[test]
    fn inline_with_return() {
        let temp = setup_project(&[(
            "player.gd",
            "func double(x):\n\treturn x * 2\n\n\nfunc _ready():\n\tvar result = double(5)\n\tprint(result)\n",
        )]);
        let result = inline_method(
            &temp.path().join("player.gd"),
            6,
            16, // column of 'double' in '\tvar result = double(5)'
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(
            content.contains("var result = 5 * 2"),
            "should substitute params and inline return, got: {content}"
        );
    }

    #[test]
    fn inline_with_params() {
        let temp = setup_project(&[(
            "player.gd",
            "func greet(name):\n\tprint(name)\n\n\nfunc _ready():\n\tgreet(\"world\")\n",
        )]);
        let result =
            inline_method(&temp.path().join("player.gd"), 6, 2, false, temp.path()).unwrap();
        assert!(result.applied);
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(
            content.contains("print(\"world\")"),
            "should substitute param, got: {content}"
        );
    }

    #[test]
    fn inline_multiple_returns_error() {
        let temp = setup_project(&[(
            "player.gd",
            "func helper(x):\n\tif x > 0:\n\t\treturn 1\n\treturn 0\n\n\nfunc _ready():\n\thelper(1)\n",
        )]);
        let result = inline_method(&temp.path().join("player.gd"), 8, 2, false, temp.path());
        assert!(result.is_err());
    }

    #[test]
    fn inline_dry_run() {
        let temp = setup_project(&[(
            "player.gd",
            "func helper():\n\tprint(42)\n\n\nfunc _ready():\n\thelper()\n",
        )]);
        let result =
            inline_method(&temp.path().join("player.gd"), 6, 2, true, temp.path()).unwrap();
        assert!(!result.applied);
        assert!(!result.function_deleted);
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(
            content.contains("func helper()"),
            "dry run should not modify file"
        );
    }

    #[test]
    fn inline_multiple_callsites_keeps_function() {
        let temp = setup_project(&[(
            "player.gd",
            "func helper():\n\tprint(42)\n\n\nfunc _ready():\n\thelper()\n\thelper()\n",
        )]);
        let result =
            inline_method(&temp.path().join("player.gd"), 6, 2, false, temp.path()).unwrap();
        assert!(result.applied);
        assert!(
            !result.function_deleted,
            "multiple callsites should keep function"
        );
        assert!(
            !result.warnings.is_empty(),
            "should warn about remaining callsites"
        );
    }

    // ── inline-method by name ────────────────────────────────────────────

    #[test]
    fn inline_by_name_single_site() {
        let temp = setup_project(&[(
            "player.gd",
            "func helper():\n\tprint(42)\n\n\nfunc _ready():\n\thelper()\n",
        )]);
        let result = inline_method_by_name(
            &temp.path().join("player.gd"),
            "helper",
            false,
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        assert_eq!(result.call_sites_inlined, 1);
        assert!(result.function_deleted);
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(
            content.contains("print(42)"),
            "should inline body, got: {content}"
        );
        assert!(
            !content.contains("func helper()"),
            "function should be deleted, got: {content}"
        );
    }

    #[test]
    fn inline_by_name_all_sites() {
        let temp = setup_project(&[(
            "player.gd",
            "func helper():\n\tprint(42)\n\n\nfunc _ready():\n\thelper()\n\thelper()\n",
        )]);
        let result = inline_method_by_name(
            &temp.path().join("player.gd"),
            "helper",
            true,
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        assert_eq!(result.call_sites_inlined, 2);
        assert!(result.function_deleted, "all=true should delete function");
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(
            !content.contains("func helper()"),
            "function should be deleted, got: {content}"
        );
    }

    #[test]
    fn inline_by_name_dry_run() {
        let temp = setup_project(&[(
            "player.gd",
            "func helper():\n\tprint(42)\n\n\nfunc _ready():\n\thelper()\n",
        )]);
        let result = inline_method_by_name(
            &temp.path().join("player.gd"),
            "helper",
            true,
            true,
            temp.path(),
        )
        .unwrap();
        assert!(!result.applied);
        assert!(!result.function_deleted);
        assert_eq!(result.call_sites_inlined, 1);
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(
            content.contains("func helper()"),
            "dry run should not modify file"
        );
    }

    #[test]
    fn inline_by_name_not_found() {
        let temp = setup_project(&[("player.gd", "func helper():\n\tpass\n")]);
        let result = inline_method_by_name(
            &temp.path().join("player.gd"),
            "nonexistent",
            false,
            false,
            temp.path(),
        );
        assert!(result.is_err());
    }

    // ── cross-file inline ─────────────────────────────────────────────────

    #[test]
    fn cross_file_inline_void_function() {
        let temp = setup_project(&[
            ("utils.gd", "func helper():\n\tprint(\"from utils\")\n"),
            ("player.gd", "func _ready():\n\thelper()\n"),
        ]);
        let result =
            inline_method(&temp.path().join("player.gd"), 2, 2, false, temp.path()).unwrap();
        assert!(result.applied);
        assert_eq!(result.function, "helper");
        assert!(!result.function_deleted, "cross-file should not delete");
        assert!(result.source_file.is_some(), "should report source file");
        assert!(
            result.source_file.as_deref().unwrap().contains("utils.gd"),
            "source_file should be utils.gd, got: {:?}",
            result.source_file
        );
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(
            content.contains("print(\"from utils\")"),
            "should inline body from cross file, got: {content}"
        );
        // Verify the source file is untouched
        let utils = fs::read_to_string(temp.path().join("utils.gd")).unwrap();
        assert!(
            utils.contains("func helper()"),
            "source file should not be modified, got: {utils}"
        );
    }

    #[test]
    fn cross_file_inline_with_return_and_params() {
        let temp = setup_project(&[
            ("math.gd", "func add(a, b):\n\treturn a + b\n"),
            (
                "player.gd",
                "func _ready():\n\tvar result = add(1, 2)\n\tprint(result)\n",
            ),
        ]);
        let result = inline_method(
            &temp.path().join("player.gd"),
            2,
            16, // column of 'add' in '\tvar result = add(1, 2)'
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        assert!(result.source_file.is_some());
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(
            content.contains("var result = 1 + 2"),
            "should substitute params and inline return from cross file, got: {content}"
        );
    }

    #[test]
    fn cross_file_inline_dry_run() {
        let temp = setup_project(&[
            ("utils.gd", "func helper():\n\tprint(42)\n"),
            ("player.gd", "func _ready():\n\thelper()\n"),
        ]);
        let result =
            inline_method(&temp.path().join("player.gd"), 2, 2, true, temp.path()).unwrap();
        assert!(!result.applied);
        assert!(!result.function_deleted);
        assert!(result.source_file.is_some());
        // File should not be modified
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(
            content.contains("helper()"),
            "dry run should not modify file, got: {content}"
        );
    }

    #[test]
    fn cross_file_inline_not_found_anywhere() {
        let temp = setup_project(&[("player.gd", "func _ready():\n\tnonexistent()\n")]);
        let result = inline_method(&temp.path().join("player.gd"), 2, 2, false, temp.path());
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("cannot find definition"),
            "should report not found, got: {err}"
        );
    }

    #[test]
    fn cross_file_inline_ambiguous() {
        let temp = setup_project(&[
            ("a.gd", "func helper():\n\tprint(\"a\")\n"),
            ("b.gd", "func helper():\n\tprint(\"b\")\n"),
            ("player.gd", "func _ready():\n\thelper()\n"),
        ]);
        let result = inline_method(&temp.path().join("player.gd"), 2, 2, false, temp.path());
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("ambiguous"),
            "should report ambiguity, got: {err}"
        );
    }

    #[test]
    fn cross_file_inline_self_warning() {
        let temp = setup_project(&[
            ("utils.gd", "func get_hp():\n\treturn self.hp\n"),
            ("player.gd", "func _ready():\n\tvar hp = get_hp()\n"),
        ]);
        let result =
            inline_method(&temp.path().join("player.gd"), 2, 12, true, temp.path()).unwrap();
        assert!(
            result.warnings.iter().any(|w| w.contains("self.")),
            "should warn about self references, got: {:?}",
            result.warnings
        );
    }

    // ── cross-file inline by name ─────────────────────────────────────────

    #[test]
    fn cross_file_inline_by_name() {
        let temp = setup_project(&[
            ("utils.gd", "func helper():\n\tprint(\"from utils\")\n"),
            ("player.gd", "func _ready():\n\thelper()\n"),
        ]);
        let result = inline_method_by_name(
            &temp.path().join("player.gd"),
            "helper",
            false,
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        assert_eq!(result.call_sites_inlined, 1);
        assert!(!result.function_deleted);
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(
            content.contains("print(\"from utils\")"),
            "should inline body from cross file, got: {content}"
        );
    }

    #[test]
    fn cross_file_inline_by_name_not_found_anywhere() {
        let temp = setup_project(&[("player.gd", "func _ready():\n\tpass\n")]);
        let result = inline_method_by_name(
            &temp.path().join("player.gd"),
            "nonexistent",
            false,
            false,
            temp.path(),
        );
        assert!(result.is_err());
    }

    #[test]
    fn cross_file_inline_by_name_multiple_call_sites() {
        let temp = setup_project(&[
            ("utils.gd", "func helper():\n\tprint(\"util\")\n"),
            ("player.gd", "func _ready():\n\thelper()\n\thelper()\n"),
        ]);
        let result = inline_method_by_name(
            &temp.path().join("player.gd"),
            "helper",
            true,
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        assert_eq!(result.call_sites_inlined, 2);
        assert!(!result.function_deleted);
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(
            !content.contains("helper()"),
            "all call sites should be inlined, got: {content}"
        );
    }

    // ── self.method() inlining ────────────────────────────────────────────

    #[test]
    fn inline_self_void_method() {
        let temp = setup_project(&[(
            "player.gd",
            "func helper():\n\tprint(\"hello\")\n\n\nfunc _ready():\n\tself.helper()\n",
        )]);
        let result =
            inline_method(&temp.path().join("player.gd"), 6, 2, false, temp.path()).unwrap();
        assert!(result.applied);
        assert_eq!(result.function, "helper");
        assert!(result.function_deleted, "single callsite should delete");
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(
            content.contains("print(\"hello\")"),
            "should inline body, got: {content}"
        );
        assert!(
            !content.contains("func helper()"),
            "function should be deleted, got: {content}"
        );
    }

    #[test]
    fn inline_self_method_with_params() {
        let temp = setup_project(&[(
            "player.gd",
            "func greet(name):\n\tprint(name)\n\n\nfunc _ready():\n\tself.greet(\"world\")\n",
        )]);
        let result =
            inline_method(&temp.path().join("player.gd"), 6, 2, false, temp.path()).unwrap();
        assert!(result.applied);
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(
            content.contains("print(\"world\")"),
            "should substitute param, got: {content}"
        );
    }

    #[test]
    fn inline_self_method_with_return() {
        let temp = setup_project(&[(
            "player.gd",
            "func double(x):\n\treturn x * 2\n\n\nfunc _ready():\n\tvar result = self.double(5)\n\tprint(result)\n",
        )]);
        let result = inline_method(
            &temp.path().join("player.gd"),
            6,
            21, // column of 'double' in '\tvar result = self.double(5)'
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(
            content.contains("var result = 5 * 2"),
            "should substitute params and inline return, got: {content}"
        );
    }

    #[test]
    fn inline_self_method_dry_run() {
        let temp = setup_project(&[(
            "player.gd",
            "func helper():\n\tprint(42)\n\n\nfunc _ready():\n\tself.helper()\n",
        )]);
        let result =
            inline_method(&temp.path().join("player.gd"), 6, 2, true, temp.path()).unwrap();
        assert!(!result.applied);
        assert!(!result.function_deleted);
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(
            content.contains("func helper()"),
            "dry run should not modify file"
        );
    }

    // ── obj.method() cross-file inlining ──────────────────────────────────

    #[test]
    fn inline_typed_var_method() {
        let temp = setup_project(&[
            (
                "enemy.gd",
                "class_name Enemy\nextends Node\n\nfunc take_damage(amount):\n\tprint(amount)\n",
            ),
            (
                "player.gd",
                "var enemy: Enemy\n\n\nfunc _ready():\n\tenemy.take_damage(10)\n",
            ),
        ]);
        let result =
            inline_method(&temp.path().join("player.gd"), 5, 2, false, temp.path()).unwrap();
        assert!(result.applied);
        assert_eq!(result.function, "take_damage");
        assert!(
            !result.function_deleted,
            "cross-file method should not be deleted"
        );
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(
            content.contains("print(10)"),
            "should inline body with param substitution, got: {content}"
        );
        // Original method should still exist in enemy.gd
        let enemy_content = fs::read_to_string(temp.path().join("enemy.gd")).unwrap();
        assert!(
            enemy_content.contains("func take_damage"),
            "cross-file function should not be deleted, got: {enemy_content}"
        );
    }

    #[test]
    fn inline_typed_var_method_with_return() {
        let temp = setup_project(&[
            (
                "utils.gd",
                "class_name Utils\nextends RefCounted\n\nfunc compute(x):\n\treturn x * 2\n",
            ),
            (
                "main.gd",
                "var utils: Utils\n\n\nfunc _ready():\n\tvar result = utils.compute(5)\n",
            ),
        ]);
        let result = inline_method(
            &temp.path().join("main.gd"),
            5,
            16, // column of 'compute' in '\tvar result = utils.compute(5)'
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        let content = fs::read_to_string(temp.path().join("main.gd")).unwrap();
        assert!(
            content.contains("var result = 5 * 2"),
            "should inline return with param substitution, got: {content}"
        );
    }

    #[test]
    fn inline_typed_var_self_to_obj_substitution() {
        let temp = setup_project(&[
            (
                "enemy.gd",
                "class_name Enemy\nextends Node\nvar health: int = 100\n\nfunc take_damage(amount):\n\tself.health -= amount\n",
            ),
            (
                "player.gd",
                "var target: Enemy\n\n\nfunc _ready():\n\ttarget.take_damage(10)\n",
            ),
        ]);
        let result =
            inline_method(&temp.path().join("player.gd"), 5, 2, false, temp.path()).unwrap();
        assert!(result.applied);
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(
            content.contains("target.health -= 10"),
            "should replace self.health with target.health, got: {content}"
        );
    }

    #[test]
    fn inline_typed_var_unresolvable_type_error() {
        let temp = setup_project(&[(
            "player.gd",
            "var enemy\n\n\nfunc _ready():\n\tenemy.take_damage(10)\n",
        )]);
        let result = inline_method(&temp.path().join("player.gd"), 5, 2, false, temp.path());
        assert!(result.is_err(), "should error when type cannot be resolved");
    }

    #[test]
    fn inline_typed_var_dry_run() {
        let temp = setup_project(&[
            (
                "enemy.gd",
                "class_name Enemy\nextends Node\n\nfunc take_damage(amount):\n\tprint(amount)\n",
            ),
            (
                "player.gd",
                "var enemy: Enemy\n\n\nfunc _ready():\n\tenemy.take_damage(10)\n",
            ),
        ]);
        let result =
            inline_method(&temp.path().join("player.gd"), 5, 2, true, temp.path()).unwrap();
        assert!(!result.applied);
        assert!(!result.function_deleted);
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(
            content.contains("enemy.take_damage(10)"),
            "dry run should not modify file"
        );
    }

    // ── replace_self_references helper ─────────────────────────────────────

    #[test]
    fn replace_self_dot_references() {
        let text = "self.health -= amount\nself.emit_signal(\"damaged\")";
        let result = replace_self_references(text, "enemy");
        assert_eq!(
            result,
            "enemy.health -= amount\nenemy.emit_signal(\"damaged\")"
        );
    }

    #[test]
    fn replace_self_bare_reference() {
        let text = "some_func(self)";
        let result = replace_self_references(text, "target");
        assert_eq!(result, "some_func(target)");
    }

    #[test]
    fn replace_self_no_false_positives() {
        let text = "myself.health\nnotself.x\nself_damage = 10";
        let result = replace_self_references(text, "enemy");
        // Should not replace partial matches
        assert_eq!(result, "myself.health\nnotself.x\nself_damage = 10");
    }
}
