mod bulk_delete;
mod bulk_rename;
mod change_signature;
mod delete_symbol;
mod edit;
mod extract_class;
mod extract_method;
mod inline_delegate;
mod inline_method;
mod introduce_parameter;
mod introduce_variable;
mod move_symbol;

pub use bulk_delete::*;
pub use bulk_rename::*;
pub use change_signature::*;
pub use delete_symbol::*;
pub use edit::*;
pub use extract_class::*;
pub use extract_method::*;
pub use inline_delegate::*;
pub use inline_method::*;
pub use introduce_parameter::*;
pub use introduce_variable::*;
pub use move_symbol::*;

use serde::Serialize;
use tree_sitter::Node;

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
pub struct PreloadRef {
    pub file: String,
    pub line: u32,
    pub path: String,
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

pub(super) fn declaration_kind_str(kind: &str) -> &str {
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
pub(super) fn find_declaration_by_name<'a>(
    root: Node<'a>,
    source: &str,
    name: &str,
) -> Option<Node<'a>> {
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        if !DECLARATION_KINDS.contains(&child.kind()) {
            continue;
        }
        if let Some(decl_name) = get_declaration_name(child, source)
            && decl_name == name
        {
            return Some(child);
        }
    }
    None
}

/// Find a top-level declaration at the given line (0-based).
/// Only matches the declaration's start line — pointing to a line inside
/// a function body does NOT match the enclosing function.
pub(super) fn find_declaration_by_line(root: Node, line: usize) -> Option<Node> {
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        if !DECLARATION_KINDS.contains(&child.kind()) {
            continue;
        }
        if child.start_position().row == line {
            return Some(child);
        }
    }
    None
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

/// Expand a declaration node's byte range to include immediately preceding
/// doc comments and annotations (contiguous `#`/`@` lines with no blank-line gap).
/// Returns (start_byte, end_byte) covering annotations + comments + declaration + trailing newline.
pub(super) fn declaration_full_range(node: Node, source: &str) -> (usize, usize) {
    let starts = line_starts(source);
    let decl_line = node.start_position().row;

    let mut first_line = decl_line;
    let mut check = decl_line;
    while check > 0 {
        check -= 1;
        let line_start = starts[check];
        let line_end = starts.get(check + 1).copied().unwrap_or(source.len());
        let line_text = &source[line_start..line_end];
        let trimmed = line_text.trim();
        if trimmed.starts_with('#') || trimmed.starts_with('@') {
            first_line = check;
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

/// Find a class_definition by name among direct children of `parent`.
pub(super) fn find_class_definition<'a>(
    parent: Node<'a>,
    source: &str,
    class_name: &str,
) -> Option<Node<'a>> {
    let mut cursor = parent.walk();
    for child in parent.children(&mut cursor) {
        if child.kind() == "class_definition"
            && let Some(name_node) = child.child_by_field_name("name")
            && name_node.utf8_text(source.as_bytes()).ok() == Some(class_name)
        {
            return Some(child);
        }
    }
    None
}

/// Find the body node of a class_definition.
pub(super) fn class_body(class_node: Node) -> Option<Node> {
    class_node.child_by_field_name("body")
}

/// Find a declaration by name within a class's body.
pub(super) fn find_declaration_in_class<'a>(
    class_node: Node<'a>,
    source: &str,
    name: &str,
) -> Option<Node<'a>> {
    let body = class_body(class_node)?;
    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        if !DECLARATION_KINDS.contains(&child.kind()) {
            continue;
        }
        if let Some(decl_name) = get_declaration_name(child, source)
            && decl_name == name
        {
            return Some(child);
        }
    }
    None
}

/// Find a declaration by line within a class's body.
pub(super) fn find_declaration_in_class_by_line(class_node: Node, line: usize) -> Option<Node> {
    let body = class_body(class_node)?;
    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        if !DECLARATION_KINDS.contains(&child.kind()) {
            continue;
        }
        if child.start_position().row == line {
            return Some(child);
        }
    }
    None
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

    // ── find_declaration_by_name ──────────────────────────────────────────

    #[test]
    fn find_function_by_name() {
        let src = "func foo():\n\tpass\n";
        let tree = crate::core::parser::parse(src).unwrap();
        let node = find_declaration_by_name(tree.root_node(), src, "foo");
        assert!(node.is_some());
        assert_eq!(node.unwrap().kind(), "function_definition");
    }

    #[test]
    fn find_variable_by_name() {
        let src = "var speed = 10\n";
        let tree = crate::core::parser::parse(src).unwrap();
        let node = find_declaration_by_name(tree.root_node(), src, "speed");
        assert!(node.is_some());
        assert_eq!(node.unwrap().kind(), "variable_statement");
    }

    #[test]
    fn find_const_by_name() {
        let src = "const MAX_HP = 200\n";
        let tree = crate::core::parser::parse(src).unwrap();
        let node = find_declaration_by_name(tree.root_node(), src, "MAX_HP");
        assert!(node.is_some());
        assert_eq!(node.unwrap().kind(), "const_statement");
    }

    #[test]
    fn find_signal_by_name() {
        let src = "signal died\n";
        let tree = crate::core::parser::parse(src).unwrap();
        let node = find_declaration_by_name(tree.root_node(), src, "died");
        assert!(node.is_some());
        assert_eq!(node.unwrap().kind(), "signal_statement");
    }

    #[test]
    fn find_enum_by_name() {
        let src = "enum State { IDLE, RUN }\n";
        let tree = crate::core::parser::parse(src).unwrap();
        let node = find_declaration_by_name(tree.root_node(), src, "State");
        assert!(node.is_some());
        assert_eq!(node.unwrap().kind(), "enum_definition");
    }

    #[test]
    fn find_class_by_name() {
        let src = "class Inner:\n\tvar x = 1\n";
        let tree = crate::core::parser::parse(src).unwrap();
        let node = find_declaration_by_name(tree.root_node(), src, "Inner");
        assert!(node.is_some());
        assert_eq!(node.unwrap().kind(), "class_definition");
    }

    #[test]
    fn find_constructor() {
        let src = "func _init():\n\tpass\n";
        let tree = crate::core::parser::parse(src).unwrap();
        let node = find_declaration_by_name(tree.root_node(), src, "_init");
        assert!(node.is_some());
    }

    #[test]
    fn find_not_found() {
        let src = "var speed = 10\n";
        let tree = crate::core::parser::parse(src).unwrap();
        let node = find_declaration_by_name(tree.root_node(), src, "nonexistent");
        assert!(node.is_none());
    }

    // ── find_declaration_by_line ──────────────────────────────────────────

    #[test]
    fn find_decl_by_line() {
        let src = "var a = 1\nvar b = 2\n\n\nfunc foo():\n\tpass\n";
        let tree = crate::core::parser::parse(src).unwrap();
        // Line 4 (0-based) is "func foo():"
        let node = find_declaration_by_line(tree.root_node(), 4);
        assert!(node.is_some());
        assert_eq!(
            get_declaration_name(node.unwrap(), src),
            Some("foo".to_string())
        );
    }

    #[test]
    fn find_decl_by_line_body_does_not_match() {
        let src = "func foo():\n\tvar x = 1\n\treturn x\n";
        let tree = crate::core::parser::parse(src).unwrap();
        // Line 0 is "func foo():" — should match
        assert!(find_declaration_by_line(tree.root_node(), 0).is_some());
        // Line 2 (0-based) is "return x" inside the body — should NOT match
        assert!(find_declaration_by_line(tree.root_node(), 2).is_none());
    }

    // ── declaration_full_range ────────────────────────────────────────────

    #[test]
    fn full_range_without_comments() {
        let src = "var a = 1\n\nfunc foo():\n\tpass\n";
        let tree = crate::core::parser::parse(src).unwrap();
        let node = find_declaration_by_name(tree.root_node(), src, "foo").unwrap();
        let (start, end) = declaration_full_range(node, src);
        assert_eq!(&src[start..end], "func foo():\n\tpass\n");
    }

    #[test]
    fn full_range_with_comments() {
        let src = "var a = 1\n\n## Documentation\n# More docs\nfunc foo():\n\tpass\n";
        let tree = crate::core::parser::parse(src).unwrap();
        let node = find_declaration_by_name(tree.root_node(), src, "foo").unwrap();
        let (start, end) = declaration_full_range(node, src);
        assert_eq!(
            &src[start..end],
            "## Documentation\n# More docs\nfunc foo():\n\tpass\n"
        );
    }

    #[test]
    fn full_range_comment_stops_at_blank_line() {
        let src = "# Unrelated comment\n\n# Doc comment\nfunc foo():\n\tpass\n";
        let tree = crate::core::parser::parse(src).unwrap();
        let node = find_declaration_by_name(tree.root_node(), src, "foo").unwrap();
        let (start, end) = declaration_full_range(node, src);
        assert_eq!(&src[start..end], "# Doc comment\nfunc foo():\n\tpass\n");
    }

    #[test]
    fn full_range_annotation_inline() {
        let src = "@export var speed = 10\n";
        let tree = crate::core::parser::parse(src).unwrap();
        let node = find_declaration_by_name(tree.root_node(), src, "speed").unwrap();
        let (start, end) = declaration_full_range(node, src);
        // Annotation is part of the node, so the range covers it
        assert_eq!(&src[start..end], "@export var speed = 10\n");
    }

    #[test]
    fn full_range_annotation_separate_line() {
        let src = "@rpc(\"any_peer\")\nfunc sync_pos(pos):\n\tpass\n";
        let tree = crate::core::parser::parse(src).unwrap();
        let node = find_declaration_by_name(tree.root_node(), src, "sync_pos").unwrap();
        let (start, end) = declaration_full_range(node, src);
        assert_eq!(
            &src[start..end],
            "@rpc(\"any_peer\")\nfunc sync_pos(pos):\n\tpass\n"
        );
    }

    #[test]
    fn full_range_doc_comment_then_annotation() {
        let src = "## Speed property\n@export\nvar speed = 10\n";
        let tree = crate::core::parser::parse(src).unwrap();
        let node = find_declaration_by_name(tree.root_node(), src, "speed").unwrap();
        let (start, end) = declaration_full_range(node, src);
        assert_eq!(
            &src[start..end],
            "## Speed property\n@export\nvar speed = 10\n"
        );
    }

    #[test]
    fn full_range_multiple_annotations() {
        let src = "@export\n@onready\nvar timer = null\n";
        let tree = crate::core::parser::parse(src).unwrap();
        let node = find_declaration_by_name(tree.root_node(), src, "timer").unwrap();
        let (start, end) = declaration_full_range(node, src);
        assert_eq!(&src[start..end], "@export\n@onready\nvar timer = null\n");
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
}
