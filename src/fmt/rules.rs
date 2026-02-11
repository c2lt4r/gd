//! Formatting rules applied during AST traversal.

use tree_sitter::Node;

/// Check if a node has annotations as a child.
fn has_annotations(node: &Node) -> bool {
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .any(|child| child.kind() == "annotations")
}

/// Returns the number of blank lines to insert between two consecutive siblings.
///
/// `func_blank` and `class_blank` control blank lines around function and class
/// definitions at the top level. Inside class bodies, methods always get 1 blank line.
pub fn spacing_between(
    prev: &Node,
    next: &Node,
    in_class_body: bool,
    func_blank: usize,
    class_blank: usize,
) -> usize {
    if in_class_body {
        return spacing_in_class_body(prev, next);
    }

    let prev_kind = prev.kind();
    let next_kind = next.kind();

    // Standalone annotations (@tool, @icon) attach to the next statement - no blank line
    if prev_kind == "annotation" || prev_kind == "annotations" {
        return 0;
    }

    let prev_is_func = prev_kind == "function_definition";
    let next_is_func = next_kind == "function_definition";
    let prev_is_class = prev_kind == "class_definition";
    let next_is_class = next_kind == "class_definition";

    // Adjacent to both function and class: use the larger
    if (prev_is_func || next_is_func) && (prev_is_class || next_is_class) {
        return func_blank.max(class_blank);
    }

    if prev_is_func || next_is_func {
        return func_blank;
    }

    if prev_is_class || next_is_class {
        return class_blank;
    }

    // Special handling for variable statements: distinguish annotated vs non-annotated
    if prev_kind == "variable_statement" && next_kind == "variable_statement" {
        let prev_has_anno = has_annotations(prev);
        let next_has_anno = has_annotations(next);
        // If one has annotations and the other doesn't, add blank line between groups
        if prev_has_anno != next_has_anno {
            return 1;
        }
        // Both have annotations or both don't: no blank line
        return 0;
    }

    // Same kind of statement: no blank line (e.g., consecutive signals, consts)
    if prev_kind == next_kind {
        return 0;
    }

    // Different kinds: one blank line between groups
    1
}

fn spacing_in_class_body(prev: &Node, next: &Node) -> usize {
    let prev_kind = prev.kind();
    let next_kind = next.kind();

    let prev_is_func = prev_kind == "function_definition";
    let next_is_func = next_kind == "function_definition";

    // One blank line before/after methods in class body
    if prev_is_func || next_is_func {
        return 1;
    }

    // Special handling for variable statements: distinguish annotated vs non-annotated
    if prev_kind == "variable_statement" && next_kind == "variable_statement" {
        let prev_has_anno = has_annotations(prev);
        let next_has_anno = has_annotations(next);
        if prev_has_anno != next_has_anno {
            return 1;
        }
        return 0;
    }

    // Same kind: no blank line
    if prev_kind == next_kind {
        return 0;
    }

    // Different kinds: blank line
    1
}
