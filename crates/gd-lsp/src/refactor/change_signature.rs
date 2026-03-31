use std::collections::HashMap;
use std::path::{Path, PathBuf};

use miette::Result;
use serde::Serialize;

use std::fmt::Write;

use super::{find_declaration_by_name, find_declaration_in_class};

use gd_core::ast_owned::{OwnedDecl, OwnedExpr, OwnedFile};
use gd_core::printer;
use gd_core::rewriter;

// ── Helpers ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct ParamInfo {
    name: String,
    type_hint: Option<String>,
    default: Option<String>,
}

fn extract_function_params(func: tree_sitter::Node, source: &str) -> Vec<ParamInfo> {
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
use gd_core::gd_ast;

// ── Rewriter helpers ───────────────────────────────────────────────────────────

/// Parse a default value string into an `OwnedExpr`.
///
/// Handles common literals directly; anything else is emitted verbatim via the
/// `Ident` printer path (which simply pushes the name string).
fn parse_default_expr(text: &str) -> OwnedExpr {
    let trimmed = text.trim();
    match trimmed {
        "null" => OwnedExpr::Null { span: None },
        "true" => OwnedExpr::Bool {
            span: None,
            value: true,
        },
        "false" => OwnedExpr::Bool {
            span: None,
            value: false,
        },
        _ => OwnedExpr::Ident {
            span: None,
            name: trimmed.to_string(),
        },
    }
}

/// Rename identifiers in a function's body using the rewriter for
/// transformation, but applying results via targeted byte-splice so that
/// comments and other non-AST content between statements are preserved.
fn rename_body_via_rewriter(
    source: &str,
    func_name: &str,
    class_name: Option<&str>,
    rename_map: &HashMap<String, String>,
) -> Result<String> {
    let tree = gd_core::parser::parse(source)?;
    let file_ast = gd_ast::convert(&tree, source);
    let owned = OwnedFile::from_borrowed(&file_ast);

    let func = find_owned_func(&owned.declarations, func_name, class_name);
    let Some(func) = func else {
        return Ok(source.to_string());
    };

    let rename_rule = |expr: OwnedExpr| -> OwnedExpr {
        if let OwnedExpr::Ident { ref name, .. } = expr
            && let Some(new_name) = rename_map.get(name.as_str())
        {
            return OwnedExpr::Ident {
                span: None,
                name: new_name.clone(),
            };
        }
        expr
    };

    // Collect byte-level replacements for statements that changed
    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    for stmt in &func.body {
        let orig_span = stmt.span();
        let rewritten = rewriter::rewrite_stmt(stmt.clone(), &rename_rule);
        if rewritten.span().is_none()
            && let Some(sp) = orig_span
        {
            let printed = printer::print_stmt(&rewritten, source);
            edits.push((sp.start, sp.end, printed));
        }
    }

    let mut result = source.to_string();
    edits.sort_by(|a, b| b.0.cmp(&a.0));
    for (start, end, replacement) in edits {
        result.replace_range(start..end, &replacement);
    }
    Ok(result)
}

/// Look up an `OwnedFunc` by name (optionally inside an inner class).
fn find_owned_func<'a>(
    decls: &'a [OwnedDecl],
    func_name: &str,
    class_name: Option<&str>,
) -> Option<&'a gd_core::ast_owned::OwnedFunc> {
    if let Some(cls) = class_name {
        for decl in decls {
            if let OwnedDecl::Class(c) = decl
                && c.name == cls
            {
                return find_owned_func(&c.declarations, func_name, None);
            }
        }
        return None;
    }
    for decl in decls {
        if let OwnedDecl::Func(func) = decl
            && func.name == func_name
        {
            return Some(func);
        }
    }
    None
}

// ── change-signature ────────────────────────────────────────────────────────

#[derive(Serialize, Debug)]
pub struct ChangeSignatureOutput {
    pub function: String,
    pub file: String,
    pub old_signature: String,
    pub new_signature: String,
    pub call_sites_updated: u32,
    pub overrides_updated: u32,
    pub applied: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub fn change_signature(
    file: &Path,
    name: &str,
    add_params: &[String],
    remove_params: &[String],
    rename_params: &[String],
    reorder: Option<&str>,
    class: Option<&str>,
    project_root: &Path,
) -> Result<ChangeSignatureOutput> {
    let source =
        std::fs::read_to_string(file).map_err(|e| miette::miette!("cannot read file: {e}"))?;
    let tree = gd_core::parser::parse(&source)?;
    let file_ast = gd_ast::convert(&tree, &source);

    // Find function definition
    let func_def = if let Some(class_name) = class {
        let inner = file_ast
            .find_class(class_name)
            .ok_or_else(|| miette::miette!("no inner class named '{class_name}' found"))?;
        find_declaration_in_class(inner, name)
            .ok_or_else(|| miette::miette!("no function '{name}' in class '{class_name}'"))?
    } else {
        find_declaration_by_name(&file_ast, name)
            .ok_or_else(|| miette::miette!("no declaration named '{name}' found"))?
    };

    if !matches!(
        func_def.kind(),
        "function_definition" | "constructor_definition"
    ) {
        return Err(miette::miette!("'{name}' is not a function"));
    }

    // Parse existing parameters
    let mut existing_params = extract_function_params(func_def, &source);
    let old_param_str = existing_params
        .iter()
        .map(|p| {
            let mut s = p.name.clone();
            if let Some(ref t) = p.type_hint {
                let _ = write!(s, ": {t}");
            }
            if let Some(ref d) = p.default {
                let _ = write!(s, " = {d}");
            }
            s
        })
        .collect::<Vec<_>>()
        .join(", ");
    let old_signature = format!("func {name}({old_param_str})");

    // Apply removals
    for remove in remove_params {
        let idx = existing_params.iter().position(|p| p.name == *remove);
        if let Some(i) = idx {
            existing_params.remove(i);
        } else {
            return Err(miette::miette!("parameter '{remove}' not found"));
        }
    }

    // Apply renames
    let mut rename_map: HashMap<String, String> = HashMap::new();
    for rename in rename_params {
        let Some((old_name, new_name)) = rename.split_once('=') else {
            return Err(miette::miette!(
                "invalid --rename-param format: '{rename}' (expected 'old_name=new_name')"
            ));
        };
        let old_name = old_name.trim();
        let new_name = new_name.trim();
        if old_name.is_empty() || new_name.is_empty() {
            return Err(miette::miette!(
                "invalid --rename-param: both old and new names must be non-empty"
            ));
        }
        let idx = existing_params.iter().position(|p| p.name == old_name);
        if let Some(i) = idx {
            existing_params[i].name = new_name.to_string();
            rename_map.insert(old_name.to_string(), new_name.to_string());
        } else {
            return Err(miette::miette!(
                "parameter '{old_name}' not found for rename"
            ));
        }
    }

    // Apply additions
    for add in add_params {
        let parsed = parse_param_spec(add)?;
        if existing_params.iter().any(|p| p.name == parsed.name) {
            return Err(miette::miette!(
                "parameter '{}' already exists",
                parsed.name
            ));
        }
        existing_params.push(parsed);
    }

    // Apply reorder
    if let Some(order_str) = reorder {
        let order: Vec<&str> = order_str.split(',').map(str::trim).collect();
        let mut reordered = Vec::new();
        for name_ref in &order {
            let idx = existing_params.iter().position(|p| p.name == *name_ref);
            if let Some(i) = idx {
                reordered.push(existing_params.remove(i));
            } else {
                return Err(miette::miette!(
                    "parameter '{name_ref}' not found for reorder"
                ));
            }
        }
        // Append any remaining params not in the reorder list
        reordered.append(&mut existing_params);
        existing_params = reordered;
    }

    // Build new parameter string
    let new_param_str = existing_params
        .iter()
        .map(|p| {
            let mut s = p.name.clone();
            if let Some(ref t) = p.type_hint {
                let _ = write!(s, ": {t}");
            }
            if let Some(ref d) = p.default {
                let _ = write!(s, " = {d}");
            }
            s
        })
        .collect::<Vec<_>>()
        .join(", ");
    let new_signature = format!("func {name}({new_param_str})");

    let original_params = extract_function_params(func_def, &source);

    let relative_file = gd_core::fs::relative_slash(file, project_root);
    let mut warnings = Vec::new();

    // Build project index for cross-file resolution (overrides + call sites)
    let project_index = gd_core::workspace_index::ProjectIndex::build(project_root);

    // Find call sites to update
    let workspace = crate::workspace::WorkspaceIndex::new(project_root.to_path_buf());
    let all_refs = crate::references::find_references_by_name(name, &workspace, None, None);
    let file_uri = tower_lsp::lsp_types::Url::from_file_path(file).ok();
    let func_def_start = func_def.start_position().row as u32;
    let func_def_end = func_def.end_position().row as u32;

    // Collect call site info for updating
    let mut call_sites: Vec<(PathBuf, u32, u32)> = Vec::new();
    for loc in &all_refs {
        if let Some(ref uri) = file_uri
            && &loc.uri == uri
        {
            let ref_line = loc.range.start.line;
            if ref_line >= func_def_start && ref_line <= func_def_end {
                continue; // Skip references within the function itself
            }
        }
        if let Ok(path) = loc.uri.to_file_path() {
            call_sites.push((path, loc.range.start.line, loc.range.start.character));
        }
    }

    // ── Find overriding methods in subclasses ────────────────────────────
    let override_files = find_override_files(file, name, class, &project_index);

    let mut ms = super::mutation::MutationSet::new();
    let mut call_sites_updated = 0u32;
    let mut overrides_updated = 0u32;

    // 1. Update function definition (signature + body renames) via typed AST rewriter
    let mut new_source = source.clone();
    // Splice new params into the source (preserves comments / non-AST content)
    if let Some(pn) = func_def.child_by_field_name("parameters") {
        let new_params_text = format!("({new_param_str})");
        new_source.replace_range(pn.start_byte()..pn.end_byte(), &new_params_text);
    }
    // Rename param usages in function body via rewriter + targeted byte-splice
    if !rename_map.is_empty() {
        new_source = rename_body_via_rewriter(&new_source, name, class, &rename_map)?;
    }
    ms.insert(file.to_path_buf(), new_source);

    // 2. Update override methods in subclasses
    for ovr_path in &override_files {
        match apply_signature_to_override(
            ovr_path,
            name,
            &new_param_str,
            &existing_params,
            &rename_map,
            &mut ms,
        ) {
            Ok(()) => overrides_updated += 1,
            Err(e) => {
                let rel = gd_core::fs::relative_slash(ovr_path, project_root);
                warnings.push(format!("failed to update override in {rel}: {e}"));
            }
        }
    }

    // 3. Update call sites via typed AST rewriter
    // Deduplicate call-site files
    let mut call_files: Vec<PathBuf> = Vec::new();
    for (path, _line, _col) in &call_sites {
        if !call_files.contains(path) {
            call_files.push(path.clone());
        }
    }

    for call_file in &call_files {
        let cs = if let Some(mutated) = ms.get(call_file) {
            mutated.clone()
        } else {
            std::fs::read_to_string(call_file)
                .map_err(|e| miette::miette!("cannot read {}: {e}", call_file.display()))?
        };
        let cs_tree = gd_core::parser::parse(&cs)?;
        let cs_file_ast = gd_ast::convert(&cs_tree, &cs);
        let cs_owned = OwnedFile::from_borrowed(&cs_file_ast);

        let local_count = std::cell::Cell::new(0u32);
        let local_phs = std::cell::RefCell::new(Vec::<String>::new());
        let call_rule = |expr: OwnedExpr| -> OwnedExpr {
            match expr {
                OwnedExpr::Call { span, callee, args } => {
                    if matches!(&*callee, OwnedExpr::Ident { name: n, .. } if n == name) {
                        let (new_args, phs) = rewrite_call_args_owned(
                            &args,
                            &original_params,
                            &existing_params,
                            remove_params,
                            &rename_map,
                            reorder,
                        );
                        local_phs.borrow_mut().extend(phs);
                        local_count.set(local_count.get() + 1);
                        OwnedExpr::Call {
                            span: None,
                            callee,
                            args: new_args,
                        }
                    } else {
                        OwnedExpr::Call { span, callee, args }
                    }
                }
                OwnedExpr::MethodCall {
                    span,
                    receiver,
                    method,
                    args,
                } if method == name => {
                    let (new_args, phs) = rewrite_call_args_owned(
                        &args,
                        &original_params,
                        &existing_params,
                        remove_params,
                        &rename_map,
                        reorder,
                    );
                    local_phs.borrow_mut().extend(phs);
                    local_count.set(local_count.get() + 1);
                    OwnedExpr::MethodCall {
                        span: None,
                        receiver,
                        method,
                        args: new_args,
                    }
                }
                other => other,
            }
        };

        let rewritten = rewriter::rewrite_file(cs_owned, &call_rule);
        if local_count.get() > 0 {
            let cs_new = printer::print_file(&rewritten, &cs);
            ms.insert(call_file.clone(), cs_new);
            call_sites_updated += local_count.get();
        }
        for p in local_phs.into_inner() {
            warnings.push(format!(
                "inserted `null` placeholder for parameter '{p}' (no default value)"
            ));
        }
    }

    // 4. Check .tscn signal connections that reference this handler
    check_tscn_connections(name, project_root, &mut warnings);

    // 5. Commit all mutations atomically
    super::mutation::commit(&ms, project_root)?;

    // Check for non-call references (variable references to the function name)
    let non_call_count = call_sites.len() as u32 - call_sites_updated;
    if non_call_count > 0 {
        warnings.push(format!(
            "{non_call_count} non-call reference{} to '{name}' may need manual updating",
            if non_call_count == 1 { "" } else { "s" }
        ));
    }

    Ok(ChangeSignatureOutput {
        function: name.to_string(),
        file: relative_file,
        old_signature,
        new_signature,
        call_sites_updated,
        overrides_updated,
        applied: true,
        warnings,
    })
}

/// Parse a parameter spec string like "name: Type = default" or just "name".
fn parse_param_spec(spec: &str) -> Result<ParamInfo> {
    let spec = spec.trim();

    // Check for default value
    let (before_default, default) = if let Some(eq_pos) = spec.find('=') {
        let name_type = spec[..eq_pos].trim();
        let default_val = spec[eq_pos + 1..].trim().to_string();
        (name_type, Some(default_val))
    } else {
        (spec, None)
    };

    // Check for type hint
    let (name, type_hint) = if let Some(colon_pos) = before_default.find(':') {
        let name = before_default[..colon_pos].trim().to_string();
        let type_h = before_default[colon_pos + 1..].trim().to_string();
        (name, Some(type_h))
    } else {
        (before_default.to_string(), None)
    };

    if name.is_empty() {
        return Err(miette::miette!("empty parameter name in '{spec}'"));
    }

    Ok(ParamInfo {
        name,
        type_hint,
        default,
    })
}

// ── Override chain helpers ──────────────────────────────────────────────────

/// Find all files that contain an override of `func_name` for classes that
/// extend the class defined in `file`.
fn find_override_files(
    file: &Path,
    func_name: &str,
    inner_class: Option<&str>,
    index: &gd_core::workspace_index::ProjectIndex,
) -> Vec<PathBuf> {
    // Don't propagate for inner classes (not supported for overrides)
    if inner_class.is_some() {
        return Vec::new();
    }

    // Determine the class_name of the file we're modifying
    let target_class = index
        .files()
        .iter()
        .find(|fs| fs.path == file)
        .and_then(|fs| fs.class_name.as_deref());

    let Some(target_class) = target_class else {
        return Vec::new();
    };

    // Find all files whose extends chain includes target_class and that
    // define a function with the same name (i.e., an override)
    let mut results = Vec::new();
    for fs in index.files() {
        if fs.path == file {
            continue; // Skip the file we're already modifying
        }

        // Check if this file extends the target class (directly or transitively)
        let extends_target = if fs.extends.as_deref() == Some(target_class) {
            true
        } else if let Some(ref cn) = fs.class_name {
            index.extends_chain(cn).contains(&target_class)
        } else {
            // File without class_name — check its extends field
            fs.extends.as_deref().is_some_and(|ext| {
                ext == target_class || index.extends_chain(ext).contains(&target_class)
            })
        };

        if extends_target && fs.functions.iter().any(|f| f.name == func_name) {
            results.push(fs.path.clone());
        }
    }

    results
}

/// Apply the same signature change to an override method in a subclass file.
fn apply_signature_to_override(
    file: &Path,
    func_name: &str,
    new_param_str: &str,
    _new_params: &[ParamInfo],
    rename_map: &HashMap<String, String>,
    ms: &mut super::mutation::MutationSet,
) -> Result<()> {
    let source =
        std::fs::read_to_string(file).map_err(|e| miette::miette!("cannot read file: {e}"))?;
    let tree = gd_core::parser::parse(&source)?;
    let file_ast = gd_ast::convert(&tree, &source);

    let func_def = find_declaration_by_name(&file_ast, func_name)
        .ok_or_else(|| miette::miette!("override '{func_name}' not found"))?;

    if !matches!(
        func_def.kind(),
        "function_definition" | "constructor_definition"
    ) {
        return Err(miette::miette!("'{func_name}' is not a function"));
    }

    // Splice new params into source (preserves comments / non-AST content)
    let mut new_source = source.clone();
    if let Some(pn) = func_def.child_by_field_name("parameters") {
        let new_params_text = format!("({new_param_str})");
        new_source.replace_range(pn.start_byte()..pn.end_byte(), &new_params_text);
    }

    // Rename param usages in body via rewriter + targeted byte-splice
    if !rename_map.is_empty() {
        new_source = rename_body_via_rewriter(&new_source, func_name, None, rename_map)?;
    }

    ms.insert(file.to_path_buf(), new_source);
    Ok(())
}

// ── .tscn signal connection warnings ────────────────────────────────────────

/// Scan `.tscn` files for `[connection ... method="name" ...]` and warn if
/// the handler function's signature is being changed.
fn check_tscn_connections(func_name: &str, project_root: &Path, warnings: &mut Vec<String>) {
    let Ok(resource_files) = gd_core::fs::collect_resource_files(project_root) else {
        return;
    };

    let pattern = format!("method=\"{func_name}\"");
    let pattern_spaced = format!("method = \"{func_name}\"");

    for res_path in &resource_files {
        if !res_path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("tscn"))
        {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(res_path) else {
            continue;
        };
        if !content.contains(&pattern) && !content.contains(&pattern_spaced) {
            continue;
        }
        // Count matching connections
        let count = content
            .lines()
            .filter(|line| {
                line.starts_with("[connection")
                    && (line.contains(&pattern) || line.contains(&pattern_spaced))
            })
            .count();
        if count > 0 {
            let rel = gd_core::fs::relative_slash(res_path, project_root);
            warnings.push(format!(
                "{count} signal connection{} in {rel} reference{} handler '{func_name}' \
                 — verify binds and parameters match the new signature",
                if count == 1 { "" } else { "s" },
                if count == 1 { "s" } else { "" },
            ));
        }
    }
}

/// Rewrite call arguments using owned AST expressions.
/// Returns (new_args, placeholder_param_names).
fn rewrite_call_args_owned(
    old_args: &[OwnedExpr],
    old_params: &[ParamInfo],
    new_params: &[ParamInfo],
    remove_params: &[String],
    rename_map: &HashMap<String, String>,
    reorder: Option<&str>,
) -> (Vec<OwnedExpr>, Vec<String>) {
    // Build old param name -> arg expression mapping
    let mut arg_map: HashMap<String, OwnedExpr> = HashMap::new();
    for (i, param) in old_params.iter().enumerate() {
        if let Some(arg) = old_args.get(i) {
            arg_map.insert(param.name.clone(), arg.clone());
        }
    }

    // Map renamed param entries: insert under new name too
    for (old_name, new_name) in rename_map {
        if let Some(arg) = arg_map.get(old_name).cloned() {
            arg_map.insert(new_name.clone(), arg);
        }
    }

    // Remove entries for removed params
    for rm in remove_params {
        arg_map.remove(rm.as_str());
    }

    // Build new argument list in new param order
    let mut new_args = Vec::new();
    let mut placeholders = Vec::new();
    let reorder_names: Option<Vec<&str>> = reorder.map(|r| r.split(',').map(str::trim).collect());

    for param in new_params {
        if let Some(arg) = arg_map.get(&param.name) {
            new_args.push(arg.clone());
        } else if let Some(ref default) = param.default {
            new_args.push(parse_default_expr(default));
        } else {
            new_args.push(OwnedExpr::Null { span: None });
            placeholders.push(param.name.clone());
        }
    }

    // If reordering without add/remove, use the reorder directly
    if let Some(names) = reorder_names
        && remove_params.is_empty()
    {
        let mut reordered = Vec::new();
        for rn in &names {
            if let Some(arg) = arg_map.get(*rn) {
                reordered.push(arg.clone());
            }
        }
        for param in new_params {
            if !names.contains(&param.name.as_str())
                && let Some(arg) = arg_map.get(&param.name)
            {
                reordered.push(arg.clone());
            }
        }
        if reordered.len() >= new_args.len() {
            return (reordered, placeholders);
        }
    }

    (new_args, placeholders)
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

    // ── change-signature ─────────────────────────────────────────────────

    #[test]
    fn change_sig_add_param_with_default() {
        let temp = setup_project(&[(
            "player.gd",
            "func greet(name):\n\tprint(name)\n\n\nfunc _ready():\n\tgreet(\"world\")\n",
        )]);
        let result = change_signature(
            &temp.path().join("player.gd"),
            "greet",
            &["greeting: String = \"hello\"".to_string()],
            &[],
            &[],
            None,
            None,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        assert!(
            result
                .new_signature
                .contains("greeting: String = \"hello\"")
        );
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(
            content.contains("greeting: String = \"hello\""),
            "should update definition, got: {content}"
        );
    }

    #[test]
    fn change_sig_remove_param() {
        let temp = setup_project(&[(
            "player.gd",
            "func greet(name, title):\n\tprint(name)\n\n\nfunc _ready():\n\tgreet(\"world\", \"mr\")\n",
        )]);
        let result = change_signature(
            &temp.path().join("player.gd"),
            "greet",
            &[],
            &["title".to_string()],
            &[],
            None,
            None,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        assert!(!result.new_signature.contains("title"));
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(
            !content.contains("title"),
            "title should be removed, got: {content}"
        );
    }

    #[test]
    fn change_sig_reorder() {
        let temp = setup_project(&[(
            "player.gd",
            "func greet(a, b, c):\n\tprint(a)\n\n\nfunc _ready():\n\tgreet(1, 2, 3)\n",
        )]);
        let result = change_signature(
            &temp.path().join("player.gd"),
            "greet",
            &[],
            &[],
            &[],
            Some("c, a, b"),
            None,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        assert_eq!(result.new_signature, "func greet(c, a, b)");
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(
            content.contains("func greet(c, a, b)"),
            "should reorder params, got: {content}"
        );
    }

    #[test]
    fn change_sig_remove_nonexistent_error() {
        let temp = setup_project(&[("player.gd", "func greet(name):\n\tpass\n")]);
        let result = change_signature(
            &temp.path().join("player.gd"),
            "greet",
            &[],
            &["nonexistent".to_string()],
            &[],
            None,
            None,
            temp.path(),
        );
        assert!(result.is_err());
    }

    #[test]
    fn change_sig_add_duplicate_error() {
        let temp = setup_project(&[("player.gd", "func greet(name):\n\tpass\n")]);
        let result = change_signature(
            &temp.path().join("player.gd"),
            "greet",
            &["name".to_string()],
            &[],
            &[],
            None,
            None,
            temp.path(),
        );
        assert!(result.is_err());
    }

    #[test]
    fn change_sig_rename_param() {
        let temp = setup_project(&[(
            "player.gd",
            "func attack(victim_id):\n\tprint(victim_id)\n\n\nfunc _ready():\n\tattack(42)\n",
        )]);
        let result = change_signature(
            &temp.path().join("player.gd"),
            "attack",
            &[],
            &[],
            &["victim_id=target_id".to_string()],
            None,
            None,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        assert!(
            result.new_signature.contains("target_id"),
            "signature should have new name, got: {}",
            result.new_signature
        );
        assert!(
            !result.new_signature.contains("victim_id"),
            "signature should not have old name"
        );
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(
            content.contains("func attack(target_id)"),
            "definition should be updated, got: {content}"
        );
        assert!(
            content.contains("print(target_id)"),
            "body usage should be renamed, got: {content}"
        );
        assert!(
            !content.contains("victim_id"),
            "old name should not appear, got: {content}"
        );
    }

    #[test]
    fn change_sig_rename_nonexistent_error() {
        let temp = setup_project(&[("player.gd", "func greet(name):\n\tpass\n")]);
        let result = change_signature(
            &temp.path().join("player.gd"),
            "greet",
            &[],
            &[],
            &["nonexistent=new_name".to_string()],
            None,
            None,
            temp.path(),
        );
        assert!(result.is_err());
    }

    // ── Gap 1: Override chain propagation ────────────────────────────────

    #[test]
    fn change_sig_propagates_to_child_override() {
        let temp = setup_project(&[
            (
                "parent.gd",
                "class_name Parent\nextends Node\nfunc attack(damage):\n\tprint(damage)\n",
            ),
            (
                "child.gd",
                "class_name Child\nextends Parent\nfunc attack(damage):\n\tprint(damage * 2)\n",
            ),
        ]);
        let result = change_signature(
            &temp.path().join("parent.gd"),
            "attack",
            &["multiplier: float = 1.0".to_string()],
            &[],
            &[],
            None,
            None,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        assert_eq!(result.overrides_updated, 1);
        let child = fs::read_to_string(temp.path().join("child.gd")).unwrap();
        assert!(
            child.contains("func attack(damage, multiplier: float = 1.0)"),
            "child override should get new param, got: {child}"
        );
    }

    #[test]
    fn change_sig_propagates_remove_to_child() {
        let temp = setup_project(&[
            (
                "parent.gd",
                "class_name Parent\nextends Node\nfunc attack(damage, crit):\n\tprint(damage)\n",
            ),
            (
                "child.gd",
                "class_name Child\nextends Parent\nfunc attack(damage, crit):\n\tprint(crit)\n",
            ),
        ]);
        let result = change_signature(
            &temp.path().join("parent.gd"),
            "attack",
            &[],
            &["crit".to_string()],
            &[],
            None,
            None,
            temp.path(),
        )
        .unwrap();
        assert_eq!(result.overrides_updated, 1);
        let child = fs::read_to_string(temp.path().join("child.gd")).unwrap();
        assert!(
            child.contains("func attack(damage)"),
            "child override should lose param, got: {child}"
        );
    }

    #[test]
    fn change_sig_propagates_rename_to_child_body() {
        let temp = setup_project(&[
            (
                "parent.gd",
                "class_name Parent\nextends Node\nfunc attack(victim_id):\n\tprint(victim_id)\n",
            ),
            (
                "child.gd",
                "class_name Child\nextends Parent\nfunc attack(victim_id):\n\tprint(victim_id)\n",
            ),
        ]);
        let result = change_signature(
            &temp.path().join("parent.gd"),
            "attack",
            &[],
            &[],
            &["victim_id=target_id".to_string()],
            None,
            None,
            temp.path(),
        )
        .unwrap();
        assert_eq!(result.overrides_updated, 1);
        let child = fs::read_to_string(temp.path().join("child.gd")).unwrap();
        assert!(
            child.contains("func attack(target_id)"),
            "child signature should be renamed, got: {child}"
        );
        assert!(
            child.contains("print(target_id)"),
            "child body should be renamed, got: {child}"
        );
        assert!(
            !child.contains("victim_id"),
            "old param name should not remain in child, got: {child}"
        );
    }

    #[test]
    fn change_sig_propagates_to_transitive_child() {
        let temp = setup_project(&[
            (
                "base.gd",
                "class_name Base\nextends Node\nfunc process(delta):\n\tpass\n",
            ),
            (
                "middle.gd",
                "class_name Middle\nextends Base\nfunc process(delta):\n\tpass\n",
            ),
            (
                "leaf.gd",
                "class_name Leaf\nextends Middle\nfunc process(delta):\n\tpass\n",
            ),
        ]);
        let result = change_signature(
            &temp.path().join("base.gd"),
            "process",
            &["weight: float = 1.0".to_string()],
            &[],
            &[],
            None,
            None,
            temp.path(),
        )
        .unwrap();
        assert_eq!(result.overrides_updated, 2);
        let middle = fs::read_to_string(temp.path().join("middle.gd")).unwrap();
        let leaf = fs::read_to_string(temp.path().join("leaf.gd")).unwrap();
        assert!(
            middle.contains("func process(delta, weight: float = 1.0)"),
            "middle should be updated, got: {middle}"
        );
        assert!(
            leaf.contains("func process(delta, weight: float = 1.0)"),
            "leaf should be updated, got: {leaf}"
        );
    }

    #[test]
    fn change_sig_no_override_for_non_class_file() {
        // File without class_name should not propagate overrides
        let temp = setup_project(&[(
            "script.gd",
            "extends Node\nfunc attack(damage):\n\tprint(damage)\n",
        )]);
        let result = change_signature(
            &temp.path().join("script.gd"),
            "attack",
            &["extra".to_string()],
            &[],
            &[],
            None,
            None,
            temp.path(),
        )
        .unwrap();
        assert_eq!(result.overrides_updated, 0);
    }

    // ── Gap 2: AST-aware body rename ────────────────────────────────────

    #[test]
    fn change_sig_rename_skips_string_literals() {
        let temp = setup_project(&[(
            "player.gd",
            "func attack(victim_id):\n\tprint(\"victim_id is: \" + str(victim_id))\n",
        )]);
        let result = change_signature(
            &temp.path().join("player.gd"),
            "attack",
            &[],
            &[],
            &["victim_id=target_id".to_string()],
            None,
            None,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(
            content.contains("\"victim_id is: \""),
            "string literal should NOT be renamed, got: {content}"
        );
        assert!(
            content.contains("str(target_id)"),
            "identifier usage should be renamed, got: {content}"
        );
    }

    #[test]
    fn change_sig_rename_skips_comments() {
        let temp = setup_project(&[(
            "player.gd",
            "func attack(victim_id):\n\t# victim_id is the target\n\tprint(victim_id)\n",
        )]);
        let result = change_signature(
            &temp.path().join("player.gd"),
            "attack",
            &[],
            &[],
            &["victim_id=target_id".to_string()],
            None,
            None,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(
            content.contains("# victim_id is the target"),
            "comment should NOT be renamed, got: {content}"
        );
        assert!(
            content.contains("print(target_id)"),
            "identifier should be renamed, got: {content}"
        );
    }

    // ── Gap 3: .tscn signal connection warnings ─────────────────────────

    #[test]
    fn change_sig_warns_about_tscn_connections() {
        let temp = setup_project(&[
            ("player.gd", "func _on_hit(damage):\n\tprint(damage)\n"),
            (
                "level.tscn",
                "[gd_scene format=3]\n\n\
                 [node name=\"Root\" type=\"Node2D\"]\n\n\
                 [connection signal=\"body_entered\" from=\"Area\" to=\".\" method=\"_on_hit\"]\n",
            ),
        ]);
        let result = change_signature(
            &temp.path().join("player.gd"),
            "_on_hit",
            &["extra".to_string()],
            &[],
            &[],
            None,
            None,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        let has_tscn_warning = result.warnings.iter().any(|w| {
            w.contains("signal connection") && w.contains("_on_hit") && w.contains("level.tscn")
        });
        assert!(
            has_tscn_warning,
            "should warn about .tscn connection, got warnings: {:?}",
            result.warnings
        );
    }

    #[test]
    fn change_sig_no_tscn_warning_when_no_connections() {
        let temp = setup_project(&[
            ("player.gd", "func attack(damage):\n\tprint(damage)\n"),
            (
                "level.tscn",
                "[gd_scene format=3]\n\n[node name=\"Root\" type=\"Node2D\"]\n",
            ),
        ]);
        let result = change_signature(
            &temp.path().join("player.gd"),
            "attack",
            &["extra".to_string()],
            &[],
            &[],
            None,
            None,
            temp.path(),
        )
        .unwrap();
        let has_tscn_warning = result
            .warnings
            .iter()
            .any(|w| w.contains("signal connection"));
        assert!(
            !has_tscn_warning,
            "should NOT warn about .tscn when no connections match"
        );
    }
}
