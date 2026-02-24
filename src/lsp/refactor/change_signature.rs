use std::collections::HashMap;
use std::path::Path;

use miette::Result;
use serde::Serialize;

use std::fmt::Write;

use super::inline_method::{
    ParamInfo, extract_call_arguments, extract_function_params, find_call_at,
};
use super::{find_class_definition, find_declaration_by_name, find_declaration_in_class};

// ── change-signature ────────────────────────────────────────────────────────

#[derive(Serialize, Debug)]
pub struct ChangeSignatureOutput {
    pub function: String,
    pub file: String,
    pub old_signature: String,
    pub new_signature: String,
    pub call_sites_updated: u32,
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
    let root = tree.root_node();

    // Find function definition
    let func_def = if let Some(class_name) = class {
        let class_node = find_class_definition(root, &source, class_name)
            .ok_or_else(|| miette::miette!("no inner class named '{class_name}' found"))?;
        find_declaration_in_class(class_node, &source, name)
            .ok_or_else(|| miette::miette!("no function '{name}' in class '{class_name}'"))?
    } else {
        find_declaration_by_name(root, &source, name)
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

    let relative_file = crate::core::fs::relative_slash(file, project_root);
    let mut warnings = Vec::new();

    // Find call sites to update
    let workspace = crate::lsp::workspace::WorkspaceIndex::new(project_root.to_path_buf());
    let all_refs = crate::lsp::references::find_references_by_name(name, &workspace, None, None);
    let file_uri = tower_lsp::lsp_types::Url::from_file_path(file).ok();
    let func_def_start = func_def.start_position().row as u32;
    let func_def_end = func_def.end_position().row as u32;

    // Collect call site info for updating
    let mut call_sites: Vec<(std::path::PathBuf, u32, u32)> = Vec::new();
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

    let mut call_sites_updated = 0u32;

    if dry_run {
        call_sites_updated = call_sites.len() as u32;
    } else {
        // 1. Update function definition (signature + body renames)
        let mut new_source = source.clone();
        if let Some(pn) = params_node {
            // Replace content between parens
            let params_start = pn.start_byte();
            let params_end = pn.end_byte();
            let new_params_text = format!("({new_param_str})");
            new_source.replace_range(params_start..params_end, &new_params_text);
        }

        // Rename param usages in function body
        if !rename_map.is_empty() {
            // Re-parse after signature change to get correct byte offsets
            let new_tree = crate::core::parser::parse(&new_source)?;
            let new_root = new_tree.root_node();
            if let Some(new_func) = find_declaration_by_name(new_root, &new_source, name)
                && let Some(body) = new_func.child_by_field_name("body")
            {
                let body_start = body.start_byte();
                let body_end = body.end_byte();
                let mut body_text = new_source[body_start..body_end].to_string();
                for (old_name, new_name) in &rename_map {
                    body_text = rename_identifier_in_text(&body_text, old_name, new_name);
                }
                new_source.replace_range(body_start..body_end, &body_text);
            }
        }

        std::fs::write(file, &new_source).map_err(|e| miette::miette!("cannot write file: {e}"))?;

        // 2. Update call sites
        // Group call sites by file
        let mut sites_by_file: HashMap<std::path::PathBuf, Vec<(u32, u32)>> = HashMap::new();
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
                        &extract_function_params(func_def, &source),
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

/// Rename an identifier in text, only matching whole words (not substrings).
fn rename_identifier_in_text(text: &str, old_name: &str, new_name: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let old_bytes = old_name.as_bytes();
    let old_len = old_bytes.len();
    let text_bytes = text.as_bytes();
    let text_len = text_bytes.len();
    let mut i = 0;

    while i < text_len {
        if i + old_len <= text_len && &text_bytes[i..i + old_len] == old_bytes {
            // Check word boundary before
            let before_ok = i == 0 || !is_ident_char(text_bytes[i - 1]);
            // Check word boundary after
            let after_ok = i + old_len >= text_len || !is_ident_char(text_bytes[i + old_len]);
            if before_ok && after_ok {
                result.push_str(new_name);
                i += old_len;
                continue;
            }
        }
        result.push(text_bytes[i] as char);
        i += 1;
    }
    result
}

fn is_ident_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
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
}
