mod change_signature;
pub mod collision;
mod delete_symbol;
mod edit;
mod extract_method;
mod move_file;
mod move_symbol;
pub mod mutation;
pub use change_signature::*;
pub use delete_symbol::*;
pub use edit::*;
pub use extract_method::*;
pub use move_file::*;
pub use move_symbol::*;

use serde::Serialize;
use tree_sitter::Node;

use gd_core::gd_ast::{self, GdClass, GdFile};

// ── Output structs ──────────────────────────────────────────────────────────

#[derive(Serialize, Debug)]
pub struct DeleteSymbolOutput {
    pub symbol: String,
    pub kind: String,
    pub file: String,
    pub removed_lines: LineRange,
    pub references: Vec<RefLocation>,
    pub applied: bool,
}

#[derive(Serialize, Debug)]
pub struct LineRange {
    pub start: u32,
    pub end: u32,
}

#[derive(Serialize, Clone, Debug)]
pub struct RefLocation {
    pub file: String,
    pub line: u32,
    pub column: u32,
    pub end_line: u32,
    pub end_column: u32,
}

#[derive(Serialize)]
pub struct MoveSymbolOutput {
    pub symbol: String,
    pub kind: String,
    pub from: String,
    pub to: String,
    pub applied: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub preloads: Vec<PreloadRef>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub callers_updated: Vec<CallerUpdateInfo>,
}

#[derive(Serialize, Debug)]
pub struct CallerUpdateInfo {
    pub file: String,
    pub action: String,
}

#[derive(Serialize)]
pub struct PreloadRef {
    pub file: String,
    pub line: u32,
    pub path: String,
}

#[derive(Serialize, Debug)]
pub struct ExtractMethodOutput {
    pub function: String,
    pub parameters: Vec<ParameterOutput>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub returns: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub return_vars: Vec<String>,
    pub call_site: String,
    pub file: String,
    pub applied: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

#[derive(Serialize, Debug)]
pub struct ParameterOutput {
    pub name: String,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub type_hint: Option<String>,
}

#[derive(Serialize)]
pub struct MoveFileOutput {
    pub from: String,
    pub to: String,
    pub applied: bool,
    pub updated_scripts: Vec<UpdatedReference>,
    pub updated_resources: Vec<UpdatedReference>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_autoload: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

#[derive(Serialize)]
pub struct UpdatedReference {
    pub file: String,
    pub line: u32,
    pub old_path: String,
    pub new_path: String,
}

// ── Shared helpers ──────────────────────────────────────────────────────────

pub(super) const DECLARATION_KINDS: &[&str] = &[
    "function_definition",
    "constructor_definition",
    "variable_statement",
    "const_statement",
    "signal_statement",
    "enum_definition",
    "class_definition",
    "class_name_statement",
];

pub(crate) fn declaration_kind_str(kind: &str) -> &str {
    match kind {
        "function_definition" | "constructor_definition" => "function",
        "variable_statement" => "variable",
        "const_statement" => "constant",
        "signal_statement" => "signal",
        "enum_definition" => "enum",
        "class_definition" => "class",
        "class_name_statement" => "class_name",
        _ => "unknown",
    }
}

pub(super) fn get_declaration_name(node: Node, source: &str) -> Option<String> {
    if node.kind() == "constructor_definition" {
        return Some("_init".to_string());
    }
    if node.kind() == "class_name_statement" {
        let name_node = node.child(1)?;
        return Some(name_node.utf8_text(source.as_bytes()).ok()?.to_string());
    }
    let name_node = node.child_by_field_name("name")?;
    Some(name_node.utf8_text(source.as_bytes()).ok()?.to_string())
}

/// Find a top-level declaration by name.
pub(crate) fn find_declaration_by_name<'a>(file: &GdFile<'a>, name: &str) -> Option<Node<'a>> {
    if let Some(node) = file.find_decl_by_name(name).map(gd_ast::GdDecl::node) {
        return Some(node);
    }
    // class_name is stored separately in GdFile, not as a GdDecl.
    // Return the class_name_statement node (parent of the name identifier).
    if file.class_name.is_some_and(|cn| cn == name) {
        return file.class_name_node.and_then(|n| n.parent());
    }
    None
}

pub(super) fn find_declaration_by_line<'a>(file: &GdFile<'a>, line: usize) -> Option<Node<'a>> {
    file.find_decl_by_line(line).map(gd_ast::GdDecl::node)
}

/// Byte offsets of the start of each line in `source`.
pub(super) fn line_starts(source: &str) -> Vec<usize> {
    let mut starts = vec![0];
    for (i, b) in source.bytes().enumerate() {
        if b == b'\n' {
            starts.push(i + 1);
        }
    }
    starts
}

/// Returns `true` if `line` is a section divider like `# ===...`, `# ---...`, `# ~~~...`, or `# ***...`.
fn is_section_divider(line: &str) -> bool {
    let rest = line.strip_prefix("# ").unwrap_or("");
    if rest.len() < 3 {
        return false;
    }
    let ch = rest.as_bytes()[0];
    matches!(ch, b'=' | b'-' | b'~' | b'*') && rest.bytes().all(|b| b == ch)
}

/// Expand a declaration node's byte range to include immediately preceding
/// doc comments and annotations (contiguous `#`/`@` lines with no blank-line gap).
/// Bridges up to 2 blank lines when `##` doc comments exist above the gap.
/// Stops at `# ===`-style section dividers so they don't travel with the symbol.
/// Returns (start_byte, end_byte) covering annotations + comments + declaration + trailing newline.
pub(crate) fn declaration_full_range(node: Node, source: &str) -> (usize, usize) {
    let starts = line_starts(source);
    let lines: Vec<&str> = starts
        .iter()
        .enumerate()
        .map(|(i, &s)| {
            let e = starts.get(i + 1).copied().unwrap_or(source.len());
            source[s..e].trim()
        })
        .collect();
    let decl_line = node.start_position().row;

    let mut first_line = decl_line;
    let mut check = decl_line;
    while check > 0 {
        check -= 1;
        let trimmed = lines[check];
        if trimmed.starts_with('#') || trimmed.starts_with('@') {
            if trimmed.starts_with('#') && !trimmed.starts_with("##") && is_section_divider(trimmed)
            {
                break;
            }
            first_line = check;
        } else if trimmed.is_empty() {
            // Peek above blank lines: bridge if ## doc comments exist
            let mut peek = check;
            let mut blanks = 1;
            // Count consecutive blank lines
            while peek > 0 && blanks < 3 {
                peek -= 1;
                if lines[peek].is_empty() {
                    blanks += 1;
                } else {
                    break;
                }
            }
            // Too many blanks or hit top of file — stop
            if blanks >= 3 || (peek == 0 && lines[0].is_empty()) {
                break;
            }
            // Check if we landed on a ## doc comment
            if lines[peek].starts_with("##") {
                first_line = peek;
                check = peek; // continue scanning upward from doc comment
            } else {
                break;
            }
        } else {
            break;
        }
    }

    let start_byte = starts[first_line];

    let mut end_byte = node.end_byte();
    if end_byte < source.len() && source.as_bytes()[end_byte] == b'\n' {
        end_byte += 1;
    }

    (start_byte, end_byte)
}

// ── Public resolution helpers ────────────────────────────────────────────────

/// Convert `--name` (+ optional inner class) into a 1-based (line, column) position.
pub fn resolve_name_to_position(
    source: &str,
    name: &str,
    class: Option<&str>,
) -> miette::Result<(usize, usize)> {
    let tree = gd_core::parser::parse(source)?;
    let file = gd_ast::convert(&tree, source);
    let decl = if let Some(cls) = class {
        let inner = file
            .find_class(cls)
            .ok_or_else(|| miette::miette!("inner class '{cls}' not found"))?;
        inner.find_decl_by_name(name)
    } else {
        file.find_decl_by_name(name)
    }
    .ok_or_else(|| miette::miette!("declaration '{name}' not found"))?;
    let pos_node = decl.name_node().unwrap_or_else(|| decl.node());
    let pos = pos_node.start_position();
    Ok((pos.row + 1, pos.column + 1))
}

/// Convert `--line` (1-based, + optional inner class) into a symbol name.
pub fn resolve_line_to_name(
    source: &str,
    line: usize,
    class: Option<&str>,
) -> miette::Result<String> {
    let tree = gd_core::parser::parse(source)?;
    let file = gd_ast::convert(&tree, source);
    let zero_line = line.saturating_sub(1);
    let decl = if let Some(cls) = class {
        let inner = file
            .find_class(cls)
            .ok_or_else(|| miette::miette!("inner class '{cls}' not found"))?;
        inner.find_decl_by_line(zero_line)
    } else {
        file.find_decl_by_line(zero_line)
    }
    .ok_or_else(|| miette::miette!("no declaration found at line {line}"))?;
    let name = decl.name();
    if name.is_empty() {
        return Err(miette::miette!(
            "could not determine name of declaration at line {line}"
        ));
    }
    Ok(name.to_string())
}

// ── Post-refactoring validation ─────────────────────────────────────────────

/// Count ERROR/MISSING nodes in a tree-sitter tree.
pub(super) fn count_error_nodes(node: &tree_sitter::Node) -> usize {
    let mut count = usize::from(node.is_error() || node.is_missing());
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            count += count_error_nodes(&child);
        }
    }
    count
}

/// Validate that a refactoring didn't introduce new parse errors compared to the original.
pub(super) fn validate_no_new_errors(original: &str, refactored: &str) -> miette::Result<()> {
    let orig_errors = gd_core::parser::parse(original)
        .map(|t| count_error_nodes(&t.root_node()))
        .unwrap_or(0);
    let new_tree = gd_core::parser::parse(refactored)?;
    let new_errors = count_error_nodes(&new_tree.root_node());
    if new_errors > orig_errors {
        return Err(miette::miette!(
            "refactoring introduced parse errors ({orig_errors} -> {new_errors})"
        ));
    }
    Ok(())
}

/// After removing a range, collapse runs of 3+ blank lines down to 2.
pub(super) fn normalize_blank_lines(source: &mut String) {
    let mut result = String::with_capacity(source.len());
    let mut newline_count = 0;
    for ch in source.chars() {
        if ch == '\n' {
            newline_count += 1;
            if newline_count <= 3 {
                result.push(ch);
            }
        } else {
            newline_count = 0;
            result.push(ch);
        }
    }
    *source = result;
}

// ── Inner class helpers ─────────────────────────────────────────────────────

/// Find a class_definition by name.
pub(crate) fn find_class_definition<'a>(file: &GdFile<'a>, class_name: &str) -> Option<Node<'a>> {
    file.find_class(class_name).map(|c| c.node)
}

/// Find a declaration by name within an inner class.
pub(super) fn find_declaration_in_class<'a>(class: &GdClass<'a>, name: &str) -> Option<Node<'a>> {
    class.find_decl_by_name(name).map(gd_ast::GdDecl::node)
}

/// Re-indent text to a target depth (measured in tabs).
/// Strips the minimum indentation and replaces it with `target_tabs` tabs.
pub(super) fn re_indent_to_depth(text: &str, target_tabs: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();

    let min_indent = lines
        .iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let trimmed = l.trim_start();
            l.len() - trimmed.len()
        })
        .min()
        .unwrap_or(0);

    let prefix = "\t".repeat(target_tabs);
    lines
        .iter()
        .map(|line| {
            if line.trim().is_empty() {
                String::new()
            } else if line.len() >= min_indent {
                format!("{prefix}{}", &line[min_indent..])
            } else {
                format!("{prefix}{line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_file<'a>(src: &'a str, tree: &'a tree_sitter::Tree) -> GdFile<'a> {
        gd_ast::convert(tree, src)
    }

    // ── find_declaration_by_name ──────────────────────────────────────────

    #[test]
    fn find_function_by_name() {
        let src = "func foo():\n\tpass\n";
        let tree = gd_core::parser::parse(src).unwrap();
        let file = parse_file(src, &tree);
        let node = find_declaration_by_name(&file, "foo");
        assert!(node.is_some());
        assert_eq!(node.unwrap().kind(), "function_definition");
    }

    #[test]
    fn find_variable_by_name() {
        let src = "var speed = 10\n";
        let tree = gd_core::parser::parse(src).unwrap();
        let file = parse_file(src, &tree);
        let node = find_declaration_by_name(&file, "speed");
        assert!(node.is_some());
        assert_eq!(node.unwrap().kind(), "variable_statement");
    }

    #[test]
    fn find_const_by_name() {
        let src = "const MAX_HP = 200\n";
        let tree = gd_core::parser::parse(src).unwrap();
        let file = parse_file(src, &tree);
        let node = find_declaration_by_name(&file, "MAX_HP");
        assert!(node.is_some());
        assert_eq!(node.unwrap().kind(), "const_statement");
    }

    #[test]
    fn find_signal_by_name() {
        let src = "signal died\n";
        let tree = gd_core::parser::parse(src).unwrap();
        let file = parse_file(src, &tree);
        let node = find_declaration_by_name(&file, "died");
        assert!(node.is_some());
        assert_eq!(node.unwrap().kind(), "signal_statement");
    }

    #[test]
    fn find_enum_by_name() {
        let src = "enum State { IDLE, RUN }\n";
        let tree = gd_core::parser::parse(src).unwrap();
        let file = parse_file(src, &tree);
        let node = find_declaration_by_name(&file, "State");
        assert!(node.is_some());
        assert_eq!(node.unwrap().kind(), "enum_definition");
    }

    #[test]
    fn find_class_by_name() {
        let src = "class Inner:\n\tvar x = 1\n";
        let tree = gd_core::parser::parse(src).unwrap();
        let file = parse_file(src, &tree);
        let node = find_declaration_by_name(&file, "Inner");
        assert!(node.is_some());
        assert_eq!(node.unwrap().kind(), "class_definition");
    }

    #[test]
    fn find_constructor() {
        let src = "func _init():\n\tpass\n";
        let tree = gd_core::parser::parse(src).unwrap();
        let file = parse_file(src, &tree);
        let node = find_declaration_by_name(&file, "_init");
        assert!(node.is_some());
    }

    #[test]
    fn find_not_found() {
        let src = "var speed = 10\n";
        let tree = gd_core::parser::parse(src).unwrap();
        let file = parse_file(src, &tree);
        let node = find_declaration_by_name(&file, "nonexistent");
        assert!(node.is_none());
    }

    // ── find_declaration_by_line ──────────────────────────────────────────

    #[test]
    fn find_decl_by_line() {
        let src = "var a = 1\nvar b = 2\n\n\nfunc foo():\n\tpass\n";
        let tree = gd_core::parser::parse(src).unwrap();
        let file = parse_file(src, &tree);
        // Line 4 (0-based) is "func foo():"
        let node = find_declaration_by_line(&file, 4);
        assert!(node.is_some());
        assert_eq!(
            get_declaration_name(node.unwrap(), src),
            Some("foo".to_string())
        );
    }

    #[test]
    fn find_decl_by_line_body_does_not_match() {
        let src = "func foo():\n\tvar x = 1\n\treturn x\n";
        let tree = gd_core::parser::parse(src).unwrap();
        let file = parse_file(src, &tree);
        // Line 0 is "func foo():" — should match
        assert!(find_declaration_by_line(&file, 0).is_some());
        // Line 2 (0-based) is "return x" inside the body — should NOT match
        assert!(find_declaration_by_line(&file, 2).is_none());
    }

    // ── declaration_full_range ────────────────────────────────────────────

    #[test]
    fn full_range_without_comments() {
        let src = "var a = 1\n\nfunc foo():\n\tpass\n";
        let tree = gd_core::parser::parse(src).unwrap();
        let file = parse_file(src, &tree);
        let node = find_declaration_by_name(&file, "foo").unwrap();
        let (start, end) = declaration_full_range(node, src);
        assert_eq!(&src[start..end], "func foo():\n\tpass\n");
    }

    #[test]
    fn full_range_with_comments() {
        let src = "var a = 1\n\n## Documentation\n# More docs\nfunc foo():\n\tpass\n";
        let tree = gd_core::parser::parse(src).unwrap();
        let file = parse_file(src, &tree);
        let node = find_declaration_by_name(&file, "foo").unwrap();
        let (start, end) = declaration_full_range(node, src);
        assert_eq!(
            &src[start..end],
            "## Documentation\n# More docs\nfunc foo():\n\tpass\n"
        );
    }

    #[test]
    fn full_range_comment_stops_at_blank_line() {
        let src = "# Unrelated comment\n\n# Doc comment\nfunc foo():\n\tpass\n";
        let tree = gd_core::parser::parse(src).unwrap();
        let file = parse_file(src, &tree);
        let node = find_declaration_by_name(&file, "foo").unwrap();
        let (start, end) = declaration_full_range(node, src);
        assert_eq!(&src[start..end], "# Doc comment\nfunc foo():\n\tpass\n");
    }

    #[test]
    fn full_range_annotation_inline() {
        let src = "@export var speed = 10\n";
        let tree = gd_core::parser::parse(src).unwrap();
        let file = parse_file(src, &tree);
        let node = find_declaration_by_name(&file, "speed").unwrap();
        let (start, end) = declaration_full_range(node, src);
        // Annotation is part of the node, so the range covers it
        assert_eq!(&src[start..end], "@export var speed = 10\n");
    }

    #[test]
    fn full_range_annotation_separate_line() {
        let src = "@rpc(\"any_peer\")\nfunc sync_pos(pos):\n\tpass\n";
        let tree = gd_core::parser::parse(src).unwrap();
        let file = parse_file(src, &tree);
        let node = find_declaration_by_name(&file, "sync_pos").unwrap();
        let (start, end) = declaration_full_range(node, src);
        assert_eq!(
            &src[start..end],
            "@rpc(\"any_peer\")\nfunc sync_pos(pos):\n\tpass\n"
        );
    }

    #[test]
    fn full_range_doc_comment_then_annotation() {
        let src = "## Speed property\n@export\nvar speed = 10\n";
        let tree = gd_core::parser::parse(src).unwrap();
        let file = parse_file(src, &tree);
        let node = find_declaration_by_name(&file, "speed").unwrap();
        let (start, end) = declaration_full_range(node, src);
        assert_eq!(
            &src[start..end],
            "## Speed property\n@export\nvar speed = 10\n"
        );
    }

    #[test]
    fn full_range_multiple_annotations() {
        let src = "@export\n@onready\nvar timer = null\n";
        let tree = gd_core::parser::parse(src).unwrap();
        let file = parse_file(src, &tree);
        let node = find_declaration_by_name(&file, "timer").unwrap();
        let (start, end) = declaration_full_range(node, src);
        assert_eq!(&src[start..end], "@export\n@onready\nvar timer = null\n");
    }

    #[test]
    fn full_range_bridges_blank_for_doc_comment() {
        let src = "## Doc line\n\nfunc foo():\n\tpass\n";
        let tree = gd_core::parser::parse(src).unwrap();
        let file = parse_file(src, &tree);
        let node = find_declaration_by_name(&file, "foo").unwrap();
        let (start, end) = declaration_full_range(node, src);
        assert_eq!(&src[start..end], "## Doc line\n\nfunc foo():\n\tpass\n");
    }

    #[test]
    fn full_range_bridges_two_blanks_for_doc_comment() {
        let src = "## Doc line\n\n\nfunc foo():\n\tpass\n";
        let tree = gd_core::parser::parse(src).unwrap();
        let file = parse_file(src, &tree);
        let node = find_declaration_by_name(&file, "foo").unwrap();
        let (start, end) = declaration_full_range(node, src);
        assert_eq!(&src[start..end], "## Doc line\n\n\nfunc foo():\n\tpass\n");
    }

    #[test]
    fn full_range_no_bridge_three_blanks() {
        let src = "## Doc line\n\n\n\nfunc foo():\n\tpass\n";
        let tree = gd_core::parser::parse(src).unwrap();
        let file = parse_file(src, &tree);
        let node = find_declaration_by_name(&file, "foo").unwrap();
        let (start, end) = declaration_full_range(node, src);
        assert_eq!(&src[start..end], "func foo():\n\tpass\n");
    }

    #[test]
    fn full_range_bridges_multiline_doc_over_blank() {
        let src = "## First paragraph\n## more\n\n## Second paragraph\nfunc foo():\n\tpass\n";
        let tree = gd_core::parser::parse(src).unwrap();
        let file = parse_file(src, &tree);
        let node = find_declaration_by_name(&file, "foo").unwrap();
        let (start, end) = declaration_full_range(node, src);
        assert_eq!(
            &src[start..end],
            "## First paragraph\n## more\n\n## Second paragraph\nfunc foo():\n\tpass\n"
        );
    }

    #[test]
    fn full_range_stops_at_section_divider() {
        let src = "# ===\n# SECTION\n# ===\n## Doc\n\nfunc foo():\n\tpass\n";
        let tree = gd_core::parser::parse(src).unwrap();
        let file = parse_file(src, &tree);
        let node = find_declaration_by_name(&file, "foo").unwrap();
        let (start, end) = declaration_full_range(node, src);
        assert_eq!(&src[start..end], "## Doc\n\nfunc foo():\n\tpass\n");
    }

    #[test]
    fn full_range_bridges_doc_but_stops_at_divider() {
        let src = "# ===\n# SECTION\n# ===\n## Doc\n\nvar x = 1\n";
        let tree = gd_core::parser::parse(src).unwrap();
        let file = parse_file(src, &tree);
        let node = find_declaration_by_name(&file, "x").unwrap();
        let (start, end) = declaration_full_range(node, src);
        assert_eq!(&src[start..end], "## Doc\n\nvar x = 1\n");
    }

    #[test]
    fn full_range_no_bridge_regular_comment_over_blank() {
        let src = "# Regular comment\n\nfunc foo():\n\tpass\n";
        let tree = gd_core::parser::parse(src).unwrap();
        let file = parse_file(src, &tree);
        let node = find_declaration_by_name(&file, "foo").unwrap();
        let (start, end) = declaration_full_range(node, src);
        assert_eq!(&src[start..end], "func foo():\n\tpass\n");
    }

    #[test]
    fn full_range_doc_comment_one_blank_before_var() {
        let src = "## Doc.\n\nvar x = 1\n";
        let tree = gd_core::parser::parse(src).unwrap();
        let file = parse_file(src, &tree);
        let node = find_declaration_by_name(&file, "x").unwrap();
        let (start, end) = declaration_full_range(node, src);
        assert_eq!(&src[start..end], "## Doc.\n\nvar x = 1\n");
    }

    // ── is_section_divider ──────────────────────────────────────────────

    #[test]
    fn section_divider_detection() {
        assert!(is_section_divider("# ==="));
        assert!(is_section_divider(
            "# ============================================================================="
        ));
        assert!(is_section_divider("# ---"));
        assert!(is_section_divider("# ~~~"));
        assert!(is_section_divider("# ***"));
        assert!(!is_section_divider("# =="));
        assert!(!is_section_divider("# SECTION NAME"));
        assert!(!is_section_divider("## Doc comment"));
        assert!(!is_section_divider("# Regular comment"));
        assert!(!is_section_divider("#"));
    }

    // ── normalize_blank_lines ────────────────────────────────────────────

    #[test]
    fn normalize_collapses_excess() {
        let mut s = "a\n\n\n\n\nb".to_string();
        normalize_blank_lines(&mut s);
        assert_eq!(s, "a\n\n\nb");
    }

    #[test]
    fn normalize_keeps_two_blank_lines() {
        let mut s = "a\n\n\nb".to_string();
        normalize_blank_lines(&mut s);
        assert_eq!(s, "a\n\n\nb");
    }

    // ── validate_no_new_errors ──────────────────────────────────────────

    #[test]
    fn validate_accepts_valid_refactoring() {
        let original = "var x = 1\n";
        let refactored = "var y = 1\n";
        assert!(validate_no_new_errors(original, refactored).is_ok());
    }

    #[test]
    fn validate_rejects_broken_output() {
        let original = "var x = 1\n";
        let broken = "var x = \n"; // missing initializer
        assert!(validate_no_new_errors(original, broken).is_err());
    }

    #[test]
    fn validate_tolerates_preexisting_errors() {
        // If original already has errors, don't reject refactored with same count
        let original = "func ():\n\tpass\n";
        let refactored = "func ():\n\treturn 1\n";
        assert!(validate_no_new_errors(original, refactored).is_ok());
    }

    // ── resolve_name_to_position ──────────────────────────────────────────

    #[test]
    fn resolve_name_to_position_function() {
        let src = "var a = 1\n\nfunc foo():\n\tpass\n";
        let (line, col) = resolve_name_to_position(src, "foo", None).unwrap();
        assert_eq!(line, 3); // 1-based
        assert_eq!(col, 6); // "func " = 5 chars, name starts at col 6
    }

    #[test]
    fn resolve_name_to_position_variable() {
        let src = "var speed = 10\n";
        let (line, col) = resolve_name_to_position(src, "speed", None).unwrap();
        assert_eq!(line, 1);
        assert_eq!(col, 5); // "var " = 4 chars
    }

    #[test]
    fn resolve_name_to_position_not_found() {
        let src = "var speed = 10\n";
        assert!(resolve_name_to_position(src, "nonexistent", None).is_err());
    }

    #[test]
    fn resolve_name_to_position_inner_class() {
        let src = "class Inner:\n\tvar x = 1\n\tfunc bar():\n\t\tpass\n";
        let (line, col) = resolve_name_to_position(src, "bar", Some("Inner")).unwrap();
        assert_eq!(line, 3);
        assert_eq!(col, 7); // "\tfunc " = 6 chars
    }

    // ── resolve_line_to_name ──────────────────────────────────────────────

    #[test]
    fn resolve_line_to_name_function() {
        let src = "var a = 1\n\nfunc foo():\n\tpass\n";
        let name = resolve_line_to_name(src, 3, None).unwrap();
        assert_eq!(name, "foo");
    }

    #[test]
    fn resolve_line_to_name_variable() {
        let src = "var speed = 10\n";
        let name = resolve_line_to_name(src, 1, None).unwrap();
        assert_eq!(name, "speed");
    }

    #[test]
    fn resolve_line_to_name_not_found() {
        let src = "var speed = 10\n\nfunc foo():\n\tpass\n";
        assert!(resolve_line_to_name(src, 2, None).is_err()); // blank line
    }

    #[test]
    fn resolve_line_to_name_inner_class() {
        let src = "class Inner:\n\tvar x = 1\n\tfunc bar():\n\t\tpass\n";
        let name = resolve_line_to_name(src, 3, Some("Inner")).unwrap();
        assert_eq!(name, "bar");
    }
}
