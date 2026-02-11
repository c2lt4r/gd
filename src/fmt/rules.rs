//! Formatting rules applied during AST traversal.

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

/// Determine spacing between two consecutive siblings in a body.
pub fn spacing_between(prev_kind: &str, next_kind: &str, in_class_body: bool) -> Spacing {
    if in_class_body {
        return spacing_in_class_body(prev_kind, next_kind);
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

    // Same kind of statement: no blank line (e.g., consecutive vars, signals, consts)
    if prev_kind == next_kind {
        return Spacing::None;
    }

    // Different kinds: one blank line between groups
    Spacing::BlankLine
}

fn spacing_in_class_body(prev_kind: &str, next_kind: &str) -> Spacing {
    let prev_is_func = prev_kind == "function_definition";
    let next_is_func = next_kind == "function_definition";

    // One blank line before/after methods
    if prev_is_func || next_is_func {
        return Spacing::BlankLine;
    }

    // Same kind: no blank line
    if prev_kind == next_kind {
        return Spacing::None;
    }

    // Different kinds: blank line
    Spacing::BlankLine
}
