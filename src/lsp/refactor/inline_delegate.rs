use std::path::Path;

use miette::Result;
use serde::Serialize;
use tree_sitter::Node;

use super::inline_method::{extract_call_arguments, extract_function_params};
use super::{declaration_full_range, find_declaration_by_name, normalize_blank_lines};

#[derive(Serialize, Debug)]
pub struct InlineDelegateOutput {
    pub function: String,
    pub delegate_target: String,
    pub call_sites_replaced: u32,
    pub function_deleted: bool,
    pub file: String,
    pub applied: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

/// Detect that a function is a pure pass-through delegate
/// (e.g., `func foo(x): return bar.foo(x)` or `func foo(): bar.do_thing()`),
/// replace all callers with the delegate target, then delete the function.
pub fn inline_delegate(
    file: &Path,
    name: &str,
    dry_run: bool,
    project_root: &Path,
) -> Result<InlineDelegateOutput> {
    let source =
        std::fs::read_to_string(file).map_err(|e| miette::miette!("cannot read file: {e}"))?;
    let tree = crate::core::parser::parse(&source)?;
    let root = tree.root_node();
    let relative_file = crate::core::fs::relative_slash(file, project_root);

    // Find function definition
    let func_def = find_declaration_by_name(root, &source, name)
        .ok_or_else(|| miette::miette!("no function named '{name}' found"))?;
    if !matches!(
        func_def.kind(),
        "function_definition" | "constructor_definition"
    ) {
        return Err(miette::miette!("'{name}' is not a function"));
    }

    let func_body = func_def
        .child_by_field_name("body")
        .ok_or_else(|| miette::miette!("function has no body"))?;

    // Body must have exactly one non-comment statement
    let stmts: Vec<Node> = {
        let mut c = func_body.walk();
        func_body
            .children(&mut c)
            .filter(|n| n.kind() != "comment")
            .collect()
    };

    if stmts.len() != 1 {
        return Err(miette::miette!(
            "not a delegate: function has {} statement(s), expected 1",
            stmts.len()
        ));
    }

    let stmt = stmts[0];

    // Extract the delegate expression from the body statement
    let delegate_expr = match stmt.kind() {
        "expression_statement" | "return_statement" => stmt
            .named_child(0)
            .ok_or_else(|| miette::miette!("empty statement"))?,
        _ => {
            return Err(miette::miette!(
                "not a delegate: body is a {} statement",
                stmt.kind()
            ));
        }
    };

    // Extract delegate target and arguments.
    // Handles both simple calls (foo()) and method calls (weapon.fire()).
    // tree-sitter parses method calls as `attribute` > `attribute_call`, NOT `call`.
    let (delegate_target, call_args) = extract_delegate_info(delegate_expr, &source)?;

    // Verify pure pass-through: parameters are forwarded in order
    let func_params = extract_function_params(func_def, &source);

    let is_passthrough = func_params.len() == call_args.len()
        && func_params
            .iter()
            .zip(&call_args)
            .all(|(p, a)| p.name == *a);

    if !is_passthrough {
        return Err(miette::miette!(
            "not a pure delegate: arguments are not forwarded in order"
        ));
    }

    // Find all call sites of this function in the file
    let func_def_start = func_def.start_position().row;
    let func_def_end = func_def.end_position().row;
    let mut call_sites = Vec::new();
    collect_call_sites(
        root,
        name,
        &source,
        func_def_start,
        func_def_end,
        &mut call_sites,
    );

    let mut warnings = Vec::new();
    if call_sites.is_empty() {
        warnings.push("no call sites found in this file".to_string());
    }

    // Check for cross-file call sites
    let workspace = crate::lsp::workspace::WorkspaceIndex::new(project_root.to_path_buf());
    let all_refs = crate::lsp::references::find_references_by_name(name, &workspace, None, None);
    let file_uri = tower_lsp::lsp_types::Url::from_file_path(file).ok();
    let cross_file_count = all_refs
        .iter()
        .filter(|loc| {
            if let Some(ref uri) = file_uri {
                &loc.uri != uri
            } else {
                true
            }
        })
        .count();
    if cross_file_count > 0 {
        warnings.push(format!(
            "{cross_file_count} cross-file reference(s) not updated"
        ));
    }

    let replaced_count = call_sites.len() as u32;

    if !dry_run {
        let mut new_source = source.clone();

        // Replace call sites from bottom to top
        call_sites.sort_by(|a, b| b.start_byte.cmp(&a.start_byte));

        for site in &call_sites {
            // Replace the function name part of the call with the delegate target
            new_source.replace_range(site.name_start..site.name_end, &delegate_target);
        }

        // Delete the function definition (re-parse after replacements)
        // Since call sites are outside the function def (we filtered them),
        // and we process bottom-to-top, the function def range is only shifted
        // by replacements that come AFTER it (higher byte offsets).
        // Reparse to be safe.
        let new_tree = crate::core::parser::parse(&new_source)?;
        let new_root = new_tree.root_node();
        if let Some(def) = find_declaration_by_name(new_root, &new_source, name) {
            let (ds, de) = declaration_full_range(def, &new_source);
            let mut final_source = String::with_capacity(new_source.len());
            final_source.push_str(&new_source[..ds]);
            final_source.push_str(&new_source[de..]);
            normalize_blank_lines(&mut final_source);
            new_source = final_source;
        }

        normalize_blank_lines(&mut new_source);
        std::fs::write(file, &new_source).map_err(|e| miette::miette!("cannot write file: {e}"))?;
    }

    Ok(InlineDelegateOutput {
        function: name.to_string(),
        delegate_target,
        call_sites_replaced: replaced_count,
        function_deleted: !dry_run && !call_sites.is_empty(),
        file: relative_file,
        applied: !dry_run,
        warnings,
    })
}

/// Extract delegate target and arguments from the delegate expression.
/// Handles:
///   - `call` nodes: `foo(args)` → target "foo"
///   - `attribute` nodes: `weapon.fire(args)` → target "weapon.fire"
fn extract_delegate_info(expr: Node, source: &str) -> Result<(String, Vec<String>)> {
    match expr.kind() {
        "call" => {
            // Simple function call: foo(args)
            let func = expr
                .child_by_field_name("function")
                .or_else(|| expr.named_child(0))
                .ok_or_else(|| miette::miette!("not a delegate: cannot find function name"))?;
            let target = func
                .utf8_text(source.as_bytes())
                .map_err(|e| miette::miette!("cannot read function name: {e}"))?
                .to_string();
            let args = extract_call_arguments(expr, source);
            Ok((target, args))
        }
        "attribute" => {
            // Method call: weapon.fire(args)
            // Structure: attribute { identifier, ".", attribute_call { identifier, arguments } }
            let attr_call = find_attribute_call_child(expr)
                .ok_or_else(|| miette::miette!("not a delegate: not a method call"))?;

            // Extract target: everything from expr start to arguments start
            let args_node = find_arguments_child(attr_call);
            let target = if let Some(args_node) = args_node {
                source[expr.start_byte()..args_node.start_byte()].to_string()
            } else {
                expr.utf8_text(source.as_bytes())
                    .map_err(|e| miette::miette!("{e}"))?
                    .to_string()
            };

            // Extract arguments from the attribute_call's arguments child
            let args = if let Some(args_node) = find_arguments_child(attr_call) {
                let mut result = Vec::new();
                let mut cursor = args_node.walk();
                for child in args_node.children(&mut cursor) {
                    if child.is_named()
                        && child.kind() != "("
                        && child.kind() != ")"
                        && child.kind() != ","
                        && let Ok(text) = child.utf8_text(source.as_bytes())
                    {
                        result.push(text.to_string());
                    }
                }
                result
            } else {
                Vec::new()
            };

            Ok((target, args))
        }
        _ => Err(miette::miette!(
            "not a delegate: body is a {} expression, not a function call",
            expr.kind()
        )),
    }
}

fn find_attribute_call_child(node: Node) -> Option<Node> {
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .find(|child| child.kind() == "attribute_call")
}

fn find_arguments_child(node: Node) -> Option<Node> {
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .find(|child| child.kind() == "arguments" || child.kind() == "argument_list")
}

struct CallSiteInfo {
    start_byte: usize,
    name_start: usize,
    name_end: usize,
}

/// Collect call sites where the function name matches, excluding calls
/// within the function definition itself.
fn collect_call_sites(
    node: Node,
    func_name: &str,
    source: &str,
    func_def_start: usize,
    func_def_end: usize,
    out: &mut Vec<CallSiteInfo>,
) {
    if node.kind() == "call" {
        let callee = node
            .child_by_field_name("function")
            .or_else(|| node.named_child(0));
        if let Some(callee) = callee
            && let Ok(callee_text) = callee.utf8_text(source.as_bytes())
            && callee_text == func_name
            && callee.kind() != "attribute"
        {
            let row = node.start_position().row;
            if row < func_def_start || row > func_def_end {
                out.push(CallSiteInfo {
                    start_byte: node.start_byte(),
                    name_start: callee.start_byte(),
                    name_end: callee.end_byte(),
                });
            }
        }
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_call_sites(child, func_name, source, func_def_start, func_def_end, out);
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
    fn inline_void_delegate() {
        let temp = setup_project(&[(
            "player.gd",
            "var weapon = null\n\n\nfunc attack():\n\tweapon.fire()\n\n\nfunc _ready():\n\tattack()\n",
        )]);
        let result =
            inline_delegate(&temp.path().join("player.gd"), "attack", false, temp.path()).unwrap();
        assert!(result.applied);
        assert_eq!(result.delegate_target, "weapon.fire");
        assert_eq!(result.call_sites_replaced, 1);
        assert!(result.function_deleted);
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(
            content.contains("weapon.fire()"),
            "should replace with delegate target, got: {content}"
        );
        assert!(
            !content.contains("func attack()"),
            "delegate should be deleted, got: {content}"
        );
    }

    #[test]
    fn inline_return_delegate() {
        let temp = setup_project(&[(
            "player.gd",
            "var stats = null\n\n\nfunc get_health():\n\treturn stats.get_health()\n\n\nfunc _ready():\n\tvar h = get_health()\n",
        )]);
        let result = inline_delegate(
            &temp.path().join("player.gd"),
            "get_health",
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        assert_eq!(result.delegate_target, "stats.get_health");
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(
            content.contains("var h = stats.get_health()"),
            "should replace call with delegate, got: {content}"
        );
    }

    #[test]
    fn inline_delegate_with_args() {
        let temp = setup_project(&[(
            "player.gd",
            "var dmg = null\n\n\nfunc take_damage(amount, type):\n\tdmg.apply(amount, type)\n\n\nfunc _ready():\n\ttake_damage(10, \"fire\")\n",
        )]);
        let result = inline_delegate(
            &temp.path().join("player.gd"),
            "take_damage",
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        assert_eq!(result.delegate_target, "dmg.apply");
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(
            content.contains("dmg.apply(10, \"fire\")"),
            "should replace call with delegate args, got: {content}"
        );
    }

    #[test]
    fn inline_delegate_not_passthrough() {
        let temp = setup_project(&[(
            "player.gd",
            "func foo(a, b):\n\treturn bar.baz(b, a)\n\n\nfunc _ready():\n\tfoo(1, 2)\n",
        )]);
        let result = inline_delegate(&temp.path().join("player.gd"), "foo", false, temp.path());
        assert!(result.is_err(), "reordered args should not be a delegate");
    }

    #[test]
    fn inline_delegate_not_single_statement() {
        let temp = setup_project(&[(
            "player.gd",
            "func foo():\n\tprint(1)\n\tbar.baz()\n\n\nfunc _ready():\n\tfoo()\n",
        )]);
        let result = inline_delegate(&temp.path().join("player.gd"), "foo", false, temp.path());
        assert!(result.is_err(), "multi-statement should not be a delegate");
    }

    #[test]
    fn inline_delegate_dry_run() {
        let temp = setup_project(&[(
            "player.gd",
            "var w = null\n\n\nfunc attack():\n\tw.fire()\n\n\nfunc _ready():\n\tattack()\n",
        )]);
        let result =
            inline_delegate(&temp.path().join("player.gd"), "attack", true, temp.path()).unwrap();
        assert!(!result.applied);
        assert!(!result.function_deleted);
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(
            content.contains("func attack()"),
            "dry run should not modify"
        );
    }
}
