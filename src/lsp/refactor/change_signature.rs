use std::collections::HashMap;
use std::path::{Path, PathBuf};

use miette::Result;
use serde::Serialize;

use std::fmt::Write;

use super::inline_method::{
    ParamInfo, extract_call_arguments, extract_function_params, find_call_at,
};
use super::{find_class_definition, find_declaration_by_name, find_declaration_in_class};
use crate::core::gd_ast;

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
    dry_run: bool,
    project_root: &Path,
) -> Result<ChangeSignatureOutput> {
    let source =
        std::fs::read_to_string(file).map_err(|e| miette::miette!("cannot read file: {e}"))?;
    let tree = crate::core::parser::parse(&source)?;
    let file_ast = gd_ast::convert(&tree, &source);

    // Find function definition
    let func_def = if let Some(class_name) = class {
        let _class_node = find_class_definition(&file_ast, class_name)
            .ok_or_else(|| miette::miette!("no inner class named '{class_name}' found"))?;
        let inner = file_ast.find_class(class_name).unwrap(); // safe: find_class_definition succeeded
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

    // Find the parameters node to replace
    let params_node = func_def.child_by_field_name("parameters");
    let original_params = extract_function_params(func_def, &source);

    let relative_file = crate::core::fs::relative_slash(file, project_root);
    let mut warnings = Vec::new();

    // Build project index for cross-file resolution (overrides + call sites)
    let project_index = crate::core::workspace_index::ProjectIndex::build(project_root);

    // Find call sites to update
    let workspace = crate::lsp::workspace::WorkspaceIndex::new(project_root.to_path_buf());
    let all_refs = crate::lsp::references::find_references_by_name(name, &workspace, None, None);
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

    let mut call_sites_updated = 0u32;
    let mut overrides_updated = 0u32;

    if dry_run {
        call_sites_updated = call_sites.len() as u32;
        overrides_updated = override_files.len() as u32;
    } else {
        // Snapshot all affected files for undo before any writes
        let mut snaps: HashMap<PathBuf, Option<Vec<u8>>> = HashMap::new();
        snaps.insert(file.to_path_buf(), Some(source.as_bytes().to_vec()));
        // Pre-read call-site files for snapshot
        {
            let mut seen: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();
            seen.insert(file.to_path_buf());
            for (path, _, _) in &call_sites {
                if seen.insert(path.clone())
                    && let Ok(content) = std::fs::read(path)
                {
                    snaps.insert(path.clone(), Some(content));
                }
            }
            // Also snapshot override files
            for ovr_path in &override_files {
                if seen.insert(ovr_path.clone())
                    && let Ok(content) = std::fs::read(ovr_path)
                {
                    snaps.insert(ovr_path.clone(), Some(content));
                }
            }
        }

        // 1. Update function definition (signature + body renames)
        let mut new_source = source.clone();
        if let Some(pn) = params_node {
            // Replace content between parens
            let params_start = pn.start_byte();
            let params_end = pn.end_byte();
            let new_params_text = format!("({new_param_str})");
            new_source.replace_range(params_start..params_end, &new_params_text);
        }

        // Rename param usages in function body (AST-aware)
        if !rename_map.is_empty() {
            // Re-parse after signature change to get correct byte offsets
            let new_tree = crate::core::parser::parse(&new_source)?;
            let new_file = gd_ast::convert(&new_tree, &new_source);
            if let Some(new_func) = find_declaration_by_name(&new_file, name)
                && let Some(body) = new_func.child_by_field_name("body")
            {
                let body_start = body.start_byte();
                let body_end = body.end_byte();
                let body_text = new_source[body_start..body_end].to_string();
                let renamed = rename_identifiers_ast(&body_text, &rename_map);
                new_source.replace_range(body_start..body_end, &renamed);
            }
        }

        std::fs::write(file, &new_source).map_err(|e| miette::miette!("cannot write file: {e}"))?;

        // 2. Update override methods in subclasses
        for ovr_path in &override_files {
            match apply_signature_to_override(ovr_path, name, &new_param_str, &rename_map) {
                Ok(()) => overrides_updated += 1,
                Err(e) => {
                    let rel = crate::core::fs::relative_slash(ovr_path, project_root);
                    warnings.push(format!("failed to update override in {rel}: {e}"));
                }
            }
        }

        // 3. Update call sites
        // Group call sites by file
        let mut sites_by_file: HashMap<PathBuf, Vec<(u32, u32)>> = HashMap::new();
        for (path, line, col) in &call_sites {
            sites_by_file
                .entry(path.clone())
                .or_default()
                .push((*line, *col));
        }

        for (call_file, positions) in &sites_by_file {
            let cs = std::fs::read_to_string(call_file)
                .map_err(|e| miette::miette!("cannot read {}: {e}", call_file.display()))?;
            let cs_tree = crate::core::parser::parse(&cs)?;
            let cs_root = cs_tree.root_node();

            let mut edits: Vec<(usize, usize, String)> = Vec::new();
            for &(ref_line, ref_col) in positions {
                let pt = tree_sitter::Point::new(ref_line as usize, ref_col as usize);
                if let Some(call) = find_call_at(cs_root, pt)
                    && let Some(args_node) = call.child_by_field_name("arguments")
                {
                    let old_args = extract_call_arguments(call, &cs);
                    let (new_args, placeholder_params) = rewrite_call_arguments(
                        &old_args,
                        &original_params,
                        &existing_params,
                        remove_params,
                        add_params,
                        &rename_map,
                        reorder,
                    );
                    for p in &placeholder_params {
                        warnings.push(format!(
                            "inserted `null` placeholder for parameter '{p}' (no default value)"
                        ));
                    }
                    let new_args_text = format!("({})", new_args.join(", "));
                    edits.push((args_node.start_byte(), args_node.end_byte(), new_args_text));
                    call_sites_updated += 1;
                }
            }

            if !edits.is_empty() {
                edits.sort_by(|a, b| b.0.cmp(&a.0));
                let mut cs_new = cs.clone();
                for (start, end, replacement) in edits {
                    cs_new.replace_range(start..end, &replacement);
                }
                super::validate_no_new_errors(&cs, &cs_new)?;
                std::fs::write(call_file, &cs_new)
                    .map_err(|e| miette::miette!("cannot write {}: {e}", call_file.display()))?;
            }
        }

        // 4. Check .tscn signal connections that reference this handler
        check_tscn_connections(name, project_root, &mut warnings);

        // Record undo
        let stack = super::undo::UndoStack::open(project_root);
        let _ = stack.record(
            "change-signature",
            &format!("change signature of {name}"),
            &snaps,
            project_root,
        );
    }

    // Check for non-call references (variable references to the function name)
    let non_call_count = call_sites.len() as u32 - call_sites_updated;
    if non_call_count > 0 && !dry_run {
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
        applied: !dry_run,
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

/// Rename identifiers in text using tree-sitter AST to avoid renaming
/// occurrences inside strings or comments.
fn rename_identifiers_ast(text: &str, rename_map: &HashMap<String, String>) -> String {
    let Ok(tree) = crate::core::parser::parse(text) else {
        return text.to_string();
    };

    // Collect all identifier nodes that match any old name in the rename map,
    // skipping those inside string or comment nodes.
    let mut replacements: Vec<(usize, usize, &str)> = Vec::new();
    collect_identifier_replacements(tree.root_node(), text, rename_map, &mut replacements);

    // Apply replacements in reverse order to preserve byte offsets
    replacements.sort_by(|a, b| b.0.cmp(&a.0));
    let mut result = text.to_string();
    for (start, end, new_name) in replacements {
        result.replace_range(start..end, new_name);
    }
    result
}

/// Recursively collect identifier nodes that should be renamed, skipping
/// nodes inside `string`, `string_content`, or `comment` parents.
fn collect_identifier_replacements<'a>(
    node: tree_sitter::Node,
    source: &str,
    rename_map: &'a HashMap<String, String>,
    out: &mut Vec<(usize, usize, &'a str)>,
) {
    // Skip entire subtrees that are strings or comments
    if matches!(node.kind(), "string" | "comment") {
        return;
    }

    if node.kind() == "identifier"
        && let Ok(text) = node.utf8_text(source.as_bytes())
        && let Some(new_name) = rename_map.get(text)
    {
        out.push((node.start_byte(), node.end_byte(), new_name.as_str()));
        return;
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_identifier_replacements(child, source, rename_map, out);
    }
}

// ── Override chain helpers ──────────────────────────────────────────────────

/// Find all files that contain an override of `func_name` for classes that
/// extend the class defined in `file`.
fn find_override_files(
    file: &Path,
    func_name: &str,
    inner_class: Option<&str>,
    index: &crate::core::workspace_index::ProjectIndex,
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
    rename_map: &HashMap<String, String>,
) -> Result<()> {
    let source =
        std::fs::read_to_string(file).map_err(|e| miette::miette!("cannot read file: {e}"))?;
    let tree = crate::core::parser::parse(&source)?;
    let file_ast = gd_ast::convert(&tree, &source);

    let func_def = find_declaration_by_name(&file_ast, func_name)
        .ok_or_else(|| miette::miette!("override '{func_name}' not found"))?;

    if !matches!(
        func_def.kind(),
        "function_definition" | "constructor_definition"
    ) {
        return Err(miette::miette!("'{func_name}' is not a function"));
    }

    let mut new_source = source.clone();

    // Update signature
    if let Some(pn) = func_def.child_by_field_name("parameters") {
        let new_params_text = format!("({new_param_str})");
        new_source.replace_range(pn.start_byte()..pn.end_byte(), &new_params_text);
    }

    // Rename param usages in body (AST-aware)
    if !rename_map.is_empty() {
        let new_tree = crate::core::parser::parse(&new_source)?;
        let new_file = gd_ast::convert(&new_tree, &new_source);
        if let Some(new_func) = find_declaration_by_name(&new_file, func_name)
            && let Some(body) = new_func.child_by_field_name("body")
        {
            let body_start = body.start_byte();
            let body_end = body.end_byte();
            let body_text = new_source[body_start..body_end].to_string();
            let renamed = rename_identifiers_ast(&body_text, rename_map);
            new_source.replace_range(body_start..body_end, &renamed);
        }
    }

    super::validate_no_new_errors(&source, &new_source)?;
    std::fs::write(file, &new_source).map_err(|e| miette::miette!("cannot write file: {e}"))?;
    Ok(())
}

// ── .tscn signal connection warnings ────────────────────────────────────────

/// Scan `.tscn` files for `[connection ... method="name" ...]` and warn if
/// the handler function's signature is being changed.
fn check_tscn_connections(func_name: &str, project_root: &Path, warnings: &mut Vec<String>) {
    let Ok(resource_files) = crate::core::fs::collect_resource_files(project_root) else {
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
            let rel = crate::core::fs::relative_slash(res_path, project_root);
            warnings.push(format!(
                "{count} signal connection{} in {rel} reference{} handler '{func_name}' \
                 — verify binds and parameters match the new signature",
                if count == 1 { "" } else { "s" },
                if count == 1 { "s" } else { "" },
            ));
        }
    }
}

/// Rewrite call arguments based on parameter changes.
/// Returns (new_args, placeholder_param_names).
fn rewrite_call_arguments(
    old_args: &[String],
    old_params: &[ParamInfo],
    new_params: &[ParamInfo],
    remove_params: &[String],
    _add_params: &[String],
    rename_map: &HashMap<String, String>,
    reorder: Option<&str>,
) -> (Vec<String>, Vec<String>) {
    // Build old param name -> arg value mapping
    let mut arg_map: HashMap<String, String> = HashMap::new();
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
    for name in remove_params {
        arg_map.remove(name.as_str());
    }

    // Build new argument list in new param order
    let mut new_args = Vec::new();
    let mut placeholders = Vec::new();
    let reorder_names: Option<Vec<&str>> = reorder.map(|r| r.split(',').map(str::trim).collect());

    for param in new_params {
        if let Some(arg) = arg_map.get(&param.name) {
            new_args.push(arg.clone());
        } else if let Some(ref default) = param.default {
            new_args.push(default.clone());
        } else {
            // Added param without default — use null as a valid GDScript placeholder
            new_args.push("null".to_string());
            placeholders.push(param.name.clone());
        }
    }

    // If reordering without add/remove, use the reorder directly
    if let Some(names) = reorder_names
        && remove_params.is_empty()
    {
        let mut reordered = Vec::new();
        for name in &names {
            if let Some(arg) = arg_map.get(*name) {
                reordered.push(arg.clone());
            }
        }
        // Add remaining args not in reorder list
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
            false,
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
            false,
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
            false,
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
            false,
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
            false,
            temp.path(),
        );
        assert!(result.is_err());
    }

    #[test]
    fn change_sig_dry_run() {
        let temp = setup_project(&[("player.gd", "func greet(name):\n\tpass\n")]);
        let result = change_signature(
            &temp.path().join("player.gd"),
            "greet",
            &["title".to_string()],
            &[],
            &[],
            None,
            None,
            true,
            temp.path(),
        )
        .unwrap();
        assert!(!result.applied);
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(!content.contains("title"), "dry run should not modify file");
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
            false,
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
            false,
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
            false,
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
            false,
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
            false,
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
            false,
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
            false,
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
            false,
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
            false,
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
            false,
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
            false,
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
