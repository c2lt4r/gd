//! Formatting rules applied during AST traversal.

use tree_sitter::Node;

/// Represents the kind of whitespace to insert between top-level statements.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Spacing {
    /// No extra blank lines - consecutive lines.
    None,
    /// One blank line between items.
    BlankLine,
    /// Two blank lines between items.
    TwoBlankLines,
}

/// Top-level node kinds that get two blank lines around them.
const TWO_BLANK_LINE_KINDS: &[&str] = &[
    "function_definition",
    "class_definition",
];

/// Check if a node has annotations as a child
fn has_annotations(node: &Node) -> bool {
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .any(|child| child.kind() == "annotations")
}

/// Determine spacing between two consecutive siblings in a body.
pub fn spacing_between(prev: &Node, next: &Node, in_class_body: bool) -> Spacing {
    let prev_kind = prev.kind();
    let next_kind = next.kind();

    if in_class_body {
        return spacing_in_class_body(prev, next);
    }

    // Standalone annotations (@tool, @icon) attach to the next statement - no blank line
    if prev_kind == "annotation" || prev_kind == "annotations" {
        return Spacing::None;
    }

    // Two blank lines before/after functions and classes
    let prev_is_big = TWO_BLANK_LINE_KINDS.contains(&prev_kind);
    let next_is_big = TWO_BLANK_LINE_KINDS.contains(&next_kind);
    if prev_is_big || next_is_big {
        return Spacing::TwoBlankLines;
    }

    // Special handling for variable statements: distinguish annotated vs non-annotated
    if prev_kind == "variable_statement" && next_kind == "variable_statement" {
        let prev_has_anno = has_annotations(prev);
        let next_has_anno = has_annotations(next);
        // If one has annotations and the other doesn't, add blank line between groups
        if prev_has_anno != next_has_anno {
            return Spacing::BlankLine;
        }
        // Both have annotations or both don't: no blank line
        return Spacing::None;
    }

    // Same kind of statement: no blank line (e.g., consecutive signals, consts)
    if prev_kind == next_kind {
        return Spacing::None;
    }

    // Different kinds: one blank line between groups
    Spacing::BlankLine
}

fn spacing_in_class_body(prev: &Node, next: &Node) -> Spacing {
    let prev_kind = prev.kind();
    let next_kind = next.kind();

    let prev_is_func = prev_kind == "function_definition";
    let next_is_func = next_kind == "function_definition";

    // One blank line before/after methods
    if prev_is_func || next_is_func {
        return Spacing::BlankLine;
    }

    // Special handling for variable statements: distinguish annotated vs non-annotated
    if prev_kind == "variable_statement" && next_kind == "variable_statement" {
        let prev_has_anno = has_annotations(prev);
        let next_has_anno = has_annotations(next);
        // If one has annotations and the other doesn't, add blank line between groups
        if prev_has_anno != next_has_anno {
            return Spacing::BlankLine;
        }
        // Both have annotations or both don't: no blank line
        return Spacing::None;
    }

    // Same kind: no blank line
    if prev_kind == next_kind {
        return Spacing::None;
    }

    // Different kinds: blank line
    Spacing::BlankLine
}
