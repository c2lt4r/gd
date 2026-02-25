use std::path::Path;

use miette::Result;
use serde::Serialize;

use super::{
    DECLARATION_KINDS, declaration_full_range, declaration_kind_str, find_declaration_by_name,
    get_declaration_name, normalize_blank_lines,
};

// ── Output ──────────────────────────────────────────────────────────────────

#[derive(Serialize, Debug)]
pub struct PullUpMemberOutput {
    pub symbol: String,
    pub kind: String,
    pub child_file: String,
    pub parent_file: String,
    pub parent_class: String,
    pub applied: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

// ── Core logic ──────────────────────────────────────────────────────────────

#[allow(clippy::too_many_lines)]
pub fn pull_up_member(
    name: &str,
    child_file: &Path,
    dry_run: bool,
    project_root: &Path,
) -> Result<PullUpMemberOutput> {
    let child_source = std::fs::read_to_string(child_file)
        .map_err(|e| miette::miette!("cannot read child file: {e}"))?;
    let child_tree = crate::core::parser::parse(&child_source)?;
    let child_root = child_tree.root_node();

    // Find the declaration in the child file
    let decl = find_declaration_by_name(child_root, &child_source, name)
        .ok_or_else(|| miette::miette!("no declaration named '{name}' found in child file"))?;

    let kind = declaration_kind_str(decl.kind()).to_string();

    // Read extends from child file
    let child_symbols = crate::core::symbol_table::build(&child_tree, &child_source);
    let extends = child_symbols
        .extends
        .as_deref()
        .ok_or_else(|| miette::miette!("child file has no 'extends' declaration"))?;

    // Resolve parent class via workspace index
    let index = crate::core::workspace_index::ProjectIndex::build(project_root);
    let parent_fs = if extends.starts_with('"') {
        // Path-based extends: `extends "res://path/to/file.gd"`
        let path = extends.trim_matches('"');
        index.resolve_preload(path)
    } else {
        index.lookup_class(extends)
    }
    .ok_or_else(|| miette::miette!("parent class '{extends}' not found in project"))?;

    let parent_file = &parent_fs.path;
    if !parent_file.exists() {
        return Err(miette::miette!(
            "parent file does not exist: {}",
            parent_file.display()
        ));
    }

    // Check for duplicate in parent
    let parent_source = std::fs::read_to_string(parent_file)
        .map_err(|e| miette::miette!("cannot read parent file: {e}"))?;
    let parent_tree = crate::core::parser::parse(&parent_source)?;
    let parent_root = parent_tree.root_node();

    if find_declaration_by_name(parent_root, &parent_source, name).is_some() {
        return Err(miette::miette!(
            "parent class '{extends}' already contains a declaration named '{name}'"
        ));
    }

    // Extract declaration text (with doc comments and annotations)
    let (start_byte, end_byte) = declaration_full_range(decl, &child_source);
    let decl_text = &child_source[start_byte..end_byte];
    let decl_text = if decl_text.ends_with('\n') {
        decl_text.to_string()
    } else {
        format!("{decl_text}\n")
    };

    // Check for self.member references that don't exist in the parent
    let mut warnings = Vec::new();
    let self_refs = collect_self_references(decl, &child_source);
    for member in &self_refs {
        if !parent_has_member(parent_root, &parent_source, member) {
            warnings.push(format!(
                "self.{member} referenced but '{member}' not found in parent class '{extends}'"
            ));
        }
    }

    // Check for references to child-only members (non-self references)
    let child_only_refs =
        collect_child_only_references(decl, &child_source, parent_root, &parent_source);
    for member in &child_only_refs {
        warnings.push(format!(
            "'{member}' referenced in moved symbol but not found in parent class '{extends}'"
        ));
    }

    let child_relative = crate::core::fs::relative_slash(child_file, project_root);
    let parent_relative = crate::core::fs::relative_slash(parent_file, project_root);

    if !dry_run {
        let mut tx = super::transaction::RefactorTransaction::new();

        // Insert into parent file
        let mut new_parent = parent_source.clone();
        let spacing = insertion_spacing(decl.kind(), &new_parent);
        new_parent.push_str(&spacing);
        new_parent.push_str(&decl_text);
        super::validate_no_new_errors(&parent_source, &new_parent)?;
        tx.write_file(parent_file, &new_parent)?;

        // Remove from child file
        let mut new_child = String::with_capacity(child_source.len());
        new_child.push_str(&child_source[..start_byte]);
        new_child.push_str(&child_source[end_byte..]);
        normalize_blank_lines(&mut new_child);
        super::validate_no_new_errors(&child_source, &new_child)?;
        tx.write_file(child_file, &new_child)?;

        let snapshots = tx.into_snapshots();
        let stack = super::undo::UndoStack::open(project_root);
        let _ = stack.record(
            "pull-up-member",
            &format!("pull up {name} from {child_relative} to {parent_relative}"),
            &snapshots,
            project_root,
        );
    }

    Ok(PullUpMemberOutput {
        symbol: name.to_string(),
        kind,
        child_file: child_relative,
        parent_file: parent_relative,
        parent_class: extends.to_string(),
        applied: !dry_run,
        warnings,
    })
}

// ── Helpers ─────────────────────────────────────────────────────────────────

/// Determine blank-line spacing before inserting a declaration into an existing file.
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
    let required = if needs_extra { 3 } else { 2 };

    if trailing_newlines >= required {
        String::new()
    } else {
        "\n".repeat(required - trailing_newlines)
    }
}

/// Collect all `self.member` references in a node subtree.
fn collect_self_references(node: tree_sitter::Node, source: &str) -> Vec<String> {
    let mut members = Vec::new();
    collect_self_refs_recursive(node, source, &mut members);
    members.sort();
    members.dedup();
    members
}

fn collect_self_refs_recursive(node: tree_sitter::Node, source: &str, members: &mut Vec<String>) {
    if node.kind() == "attribute"
        && let Some(obj) = node.child(0)
        && obj.kind() == "identifier"
        && obj.utf8_text(source.as_bytes()).ok() == Some("self")
        && let Some(member) = node.child(2)
    {
        let name_text = if member.kind() == "attribute_call" {
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
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_self_refs_recursive(child, source, members);
    }
}

/// Check if the parent's root scope has a member with the given name.
fn parent_has_member(parent_root: tree_sitter::Node, source: &str, name: &str) -> bool {
    let mut cursor = parent_root.walk();
    for child in parent_root.children(&mut cursor) {
        if DECLARATION_KINDS.contains(&child.kind())
            && let Some(decl_name) = get_declaration_name(child, source)
            && decl_name == name
        {
            return true;
        }
    }
    false
}

/// Collect identifiers used in the declaration body that reference top-level
/// members in the child file but don't exist in the parent file.
/// Excludes `self.x` (handled separately) and local variables/parameters.
fn collect_child_only_references(
    decl: tree_sitter::Node,
    child_source: &str,
    parent_root: tree_sitter::Node,
    parent_source: &str,
) -> Vec<String> {
    // Gather top-level member names from the child (excluding the declaration itself)
    let child_root = decl.parent().unwrap_or(decl);
    let mut child_members = std::collections::HashSet::new();
    let mut cursor = child_root.walk();
    let decl_name = get_declaration_name(decl, child_source);
    for child in child_root.children(&mut cursor) {
        if DECLARATION_KINDS.contains(&child.kind())
            && let Some(name) = get_declaration_name(child, child_source)
            && decl_name.as_deref() != Some(name.as_str())
        {
            child_members.insert(name);
        }
    }

    // Gather parent member names
    let mut parent_members = std::collections::HashSet::new();
    let mut pcursor = parent_root.walk();
    for child in parent_root.children(&mut pcursor) {
        if DECLARATION_KINDS.contains(&child.kind())
            && let Some(name) = get_declaration_name(child, parent_source)
        {
            parent_members.insert(name);
        }
    }

    // Collect local names (parameters + local vars) to exclude
    let locals = collect_local_names(decl, child_source);

    // Find identifiers in decl body that exist in child but not parent
    let mut refs = Vec::new();
    collect_identifier_refs(
        decl,
        child_source,
        &child_members,
        &parent_members,
        &locals,
        &mut refs,
    );
    refs.sort();
    refs.dedup();
    refs
}

fn collect_local_names(
    func_node: tree_sitter::Node,
    source: &str,
) -> std::collections::HashSet<String> {
    let mut locals = std::collections::HashSet::new();
    // Collect parameter names
    if let Some(params) = func_node.child_by_field_name("parameters") {
        let mut cursor = params.walk();
        for child in params.children(&mut cursor) {
            if child.kind() == "identifier"
                && let Ok(name) = child.utf8_text(source.as_bytes())
            {
                locals.insert(name.to_string());
            } else if let Some(name_node) = child.child_by_field_name("name")
                && let Ok(name) = name_node.utf8_text(source.as_bytes())
            {
                locals.insert(name.to_string());
            }
        }
    }
    // Collect local variable declarations in the body
    if let Some(body) = func_node.child_by_field_name("body") {
        collect_local_vars_in_body(body, source, &mut locals);
    }
    locals
}

fn collect_local_vars_in_body(
    node: tree_sitter::Node,
    source: &str,
    locals: &mut std::collections::HashSet<String>,
) {
    if node.kind() == "variable_statement"
        && let Some(name) = get_declaration_name(node, source)
    {
        locals.insert(name);
    }
    // `for` loop iterator variable
    if node.kind() == "for_statement"
        && let Some(iter_node) = node.child_by_field_name("left")
        && let Ok(name) = iter_node.utf8_text(source.as_bytes())
    {
        locals.insert(name.to_string());
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_local_vars_in_body(child, source, locals);
    }
}

fn collect_identifier_refs(
    node: tree_sitter::Node,
    source: &str,
    child_members: &std::collections::HashSet<String>,
    parent_members: &std::collections::HashSet<String>,
    locals: &std::collections::HashSet<String>,
    refs: &mut Vec<String>,
) {
    // Skip `self.x` — those are handled by collect_self_references
    if node.kind() == "attribute"
        && let Some(obj) = node.child(0)
        && obj.kind() == "identifier"
        && obj.utf8_text(source.as_bytes()).ok() == Some("self")
    {
        return;
    }

    if node.kind() == "identifier"
        && let Ok(name) = node.utf8_text(source.as_bytes())
        && child_members.contains(name)
        && !parent_members.contains(name)
        && !locals.contains(name)
    {
        // Check this isn't the function being called (i.e., part of a call expression's function name)
        // or the declaration name itself
        refs.push(name.to_string());
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_identifier_refs(child, source, child_members, parent_members, locals, refs);
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
    fn pull_up_function() {
        let temp = setup_project(&[
            (
                "base.gd",
                "class_name Base\nextends Node\n\n\nfunc existing():\n\tpass\n",
            ),
            (
                "child.gd",
                "extends Base\n\n\nfunc helper():\n\tpass\n\n\nfunc stay():\n\tpass\n",
            ),
        ]);
        let result =
            pull_up_member("helper", &temp.path().join("child.gd"), false, temp.path()).unwrap();
        assert!(result.applied);
        assert_eq!(result.kind, "function");
        assert_eq!(result.parent_class, "Base");

        let parent = fs::read_to_string(temp.path().join("base.gd")).unwrap();
        assert!(
            parent.contains("func helper()"),
            "parent should contain helper"
        );
        assert!(
            parent.contains("func existing()"),
            "parent should keep existing"
        );

        let child = fs::read_to_string(temp.path().join("child.gd")).unwrap();
        assert!(!child.contains("helper"), "child should not contain helper");
        assert!(child.contains("stay"), "child should keep stay");
    }

    #[test]
    fn pull_up_variable() {
        let temp = setup_project(&[
            ("base.gd", "class_name Base\nextends Node\nvar keep = 1\n"),
            ("child.gd", "extends Base\nvar speed = 10\nvar local = 5\n"),
        ]);
        let result =
            pull_up_member("speed", &temp.path().join("child.gd"), false, temp.path()).unwrap();
        assert!(result.applied);
        assert_eq!(result.kind, "variable");

        let parent = fs::read_to_string(temp.path().join("base.gd")).unwrap();
        assert!(
            parent.contains("var speed = 10"),
            "parent should have speed"
        );

        let child = fs::read_to_string(temp.path().join("child.gd")).unwrap();
        assert!(!child.contains("speed"), "child should not have speed");
        assert!(child.contains("local"), "child should keep local");
    }

    #[test]
    fn pull_up_signal() {
        let temp = setup_project(&[
            ("base.gd", "class_name Base\nextends Node\n"),
            ("child.gd", "extends Base\nsignal health_changed\n"),
        ]);
        let result = pull_up_member(
            "health_changed",
            &temp.path().join("child.gd"),
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        assert_eq!(result.kind, "signal");

        let parent = fs::read_to_string(temp.path().join("base.gd")).unwrap();
        assert!(parent.contains("signal health_changed"));
    }

    #[test]
    fn pull_up_constant() {
        let temp = setup_project(&[
            ("base.gd", "class_name Base\nextends Node\n"),
            ("child.gd", "extends Base\nconst MAX_HP = 100\n"),
        ]);
        let result =
            pull_up_member("MAX_HP", &temp.path().join("child.gd"), false, temp.path()).unwrap();
        assert!(result.applied);
        assert_eq!(result.kind, "constant");

        let parent = fs::read_to_string(temp.path().join("base.gd")).unwrap();
        assert!(parent.contains("const MAX_HP = 100"));
    }

    #[test]
    fn pull_up_with_doc_comments() {
        let temp = setup_project(&[
            ("base.gd", "class_name Base\nextends Node\n"),
            (
                "child.gd",
                "extends Base\n\n## Documentation for helper\nfunc helper():\n\tpass\n",
            ),
        ]);
        let result =
            pull_up_member("helper", &temp.path().join("child.gd"), false, temp.path()).unwrap();
        assert!(result.applied);

        let parent = fs::read_to_string(temp.path().join("base.gd")).unwrap();
        assert!(
            parent.contains("## Documentation for helper"),
            "doc comments should be preserved"
        );
        assert!(parent.contains("func helper()"));
    }

    #[test]
    fn pull_up_duplicate_error() {
        let temp = setup_project(&[
            (
                "base.gd",
                "class_name Base\nextends Node\n\n\nfunc helper():\n\treturn 1\n",
            ),
            ("child.gd", "extends Base\n\n\nfunc helper():\n\tpass\n"),
        ]);
        let result = pull_up_member("helper", &temp.path().join("child.gd"), false, temp.path());
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("already contains"), "error: {err}");
    }

    #[test]
    fn pull_up_no_extends_error() {
        let temp = setup_project(&[("child.gd", "func helper():\n\tpass\n")]);
        let result = pull_up_member("helper", &temp.path().join("child.gd"), false, temp.path());
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("no 'extends'"), "error: {err}");
    }

    #[test]
    fn pull_up_engine_class_error() {
        let temp = setup_project(&[("child.gd", "extends Node\n\n\nfunc helper():\n\tpass\n")]);
        let result = pull_up_member("helper", &temp.path().join("child.gd"), false, temp.path());
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("not found in project"), "error: {err}");
    }

    #[test]
    fn pull_up_dry_run() {
        let temp = setup_project(&[
            (
                "base.gd",
                "class_name Base\nextends Node\n\n\nfunc existing():\n\tpass\n",
            ),
            ("child.gd", "extends Base\n\n\nfunc helper():\n\tpass\n"),
        ]);
        let result =
            pull_up_member("helper", &temp.path().join("child.gd"), true, temp.path()).unwrap();
        assert!(!result.applied);

        // Files should be unchanged
        let parent = fs::read_to_string(temp.path().join("base.gd")).unwrap();
        assert!(
            !parent.contains("helper"),
            "dry run should not modify parent"
        );
        let child = fs::read_to_string(temp.path().join("child.gd")).unwrap();
        assert!(child.contains("helper"), "dry run should not modify child");
    }

    #[test]
    fn pull_up_warns_missing_self_ref() {
        let temp = setup_project(&[
            ("base.gd", "class_name Base\nextends Node\n"),
            (
                "child.gd",
                "extends Base\nvar health = 100\n\n\nfunc take_damage():\n\tself.health -= 10\n",
            ),
        ]);
        let result = pull_up_member(
            "take_damage",
            &temp.path().join("child.gd"),
            true,
            temp.path(),
        )
        .unwrap();
        assert!(
            result.warnings.iter().any(|w| w.contains("self.health")),
            "should warn about missing self.health: {:?}",
            result.warnings
        );
    }

    #[test]
    fn pull_up_no_warn_when_parent_has_member() {
        let temp = setup_project(&[
            (
                "base.gd",
                "class_name Base\nextends Node\nvar health = 200\n",
            ),
            (
                "child.gd",
                "extends Base\n\n\nfunc take_damage():\n\tself.health -= 10\n",
            ),
        ]);
        let result = pull_up_member(
            "take_damage",
            &temp.path().join("child.gd"),
            true,
            temp.path(),
        )
        .unwrap();
        assert!(
            !result.warnings.iter().any(|w| w.contains("self.health")),
            "should NOT warn when parent has member: {:?}",
            result.warnings
        );
    }

    #[test]
    fn pull_up_not_found_in_child() {
        let temp = setup_project(&[
            ("base.gd", "class_name Base\nextends Node\n"),
            ("child.gd", "extends Base\nvar x = 1\n"),
        ]);
        let result = pull_up_member(
            "nonexistent",
            &temp.path().join("child.gd"),
            false,
            temp.path(),
        );
        assert!(result.is_err());
    }

    #[test]
    fn pull_up_function_proper_spacing() {
        let temp = setup_project(&[
            ("base.gd", "class_name Base\nextends Node\nvar x = 1\n"),
            ("child.gd", "extends Base\n\n\nfunc helper():\n\tpass\n"),
        ]);
        let _ =
            pull_up_member("helper", &temp.path().join("child.gd"), false, temp.path()).unwrap();
        let parent = fs::read_to_string(temp.path().join("base.gd")).unwrap();
        assert!(
            parent.contains("\n\n\nfunc helper()"),
            "should have 2 blank lines before function, got: {parent:?}"
        );
    }
}
