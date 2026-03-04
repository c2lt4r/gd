//! Annotation/type formatting: annotations, paren_list (inline/multiline),
//! typed_parameter, print_type, print_inline_comment, decorators, export.

use tree_sitter::Node;

use super::Printer;

impl Printer {
    // ── Annotations ────────────────────────────────────────────────────

    /// Print annotations inline before a declaration (var/func/etc).
    /// Multiple annotations: each on its own line, last one on the same line as the declaration.
    pub(crate) fn print_annotations_block(&mut self, node: &Node, source: &str, indent: usize) {
        let mut cursor = node.walk();
        let children: Vec<Node> = node.named_children(&mut cursor).collect();

        for (i, child) in children.iter().enumerate() {
            if i > 0 {
                self.push_str("\n");
                self.write_indent(indent);
            }
            self.emit(child, source);
        }
        // Space before the declaration keyword (var/func) - keeps annotation on same line
        self.push_str(" ");
    }

    /// Print standalone annotations (at module level, not attached to var/func).
    pub(crate) fn print_annotations_standalone(
        &mut self,
        node: &Node,
        source: &str,
        _indent: usize,
    ) {
        let mut cursor = node.walk();
        let children: Vec<Node> = node.named_children(&mut cursor).collect();

        for child in &children {
            self.emit(child, source);
        }
    }

    // ── Paren lists (arguments, parameters) ────────────────────────────

    pub(crate) fn print_paren_list(&mut self, node: &Node, source: &str, indent: usize) {
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();

        let is_multiline = node.start_position().row != node.end_position().row;
        let has_line_continuation = children.iter().any(|c| c.kind() == "line_continuation");

        if has_line_continuation {
            // Line continuation makes it span rows but it's logically one line
            self.print_paren_list_inline(&children, source, indent);
        } else if is_multiline {
            self.print_paren_list_multiline(&children, source, indent);
        } else {
            self.print_paren_list_inline(&children, source, indent);
        }
    }

    fn print_paren_list_inline(&mut self, children: &[Node], source: &str, indent: usize) {
        self.push_str("(");
        let mut first = true;
        for child in children {
            match child.kind() {
                "(" | ")" | "line_continuation" => {}
                "," => self.push_str(", "),
                _ => {
                    if first {
                        first = false;
                    }
                    self.print_node(child, source, indent);
                }
            }
        }
        self.push_str(")");
    }

    fn print_paren_list_multiline(&mut self, children: &[Node], source: &str, indent: usize) {
        let inner_indent = indent + 1;
        self.push_str("(");
        let mut first = true;
        for child in children {
            match child.kind() {
                "(" | ")" | "line_continuation" => {}
                "," => self.push_str(","),
                _ => {
                    self.push_str("\n");
                    self.write_indent(inner_indent);
                    self.print_node(child, source, inner_indent);
                    first = false;
                }
            }
        }
        if !first {
            self.push_str("\n");
            self.write_indent(indent);
        }
        self.push_str(")");
    }

    // ── Typed parameter (name: type) ───────────────────────────────────

    pub(crate) fn print_typed_param(&mut self, node: &Node, source: &str) {
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();

        for child in &children {
            match child.kind() {
                ":" => self.push_str(": "),
                "type" => self.print_type(child, source),
                _ => self.emit(child, source),
            }
        }
    }

    // ── Type node ──────────────────────────────────────────────────────

    pub(crate) fn print_type(&mut self, node: &Node, source: &str) {
        // Type nodes contain identifiers, possibly with generics
        // Emit the full text verbatim since type syntax shouldn't be reformatted
        self.emit(node, source);
    }

    // ── Inline comment helper ──────────────────────────────────────────

    /// Emit a comment on its own line at the given indent level.
    pub(crate) fn print_inline_comment(&mut self, node: &Node, source: &str, indent: usize) {
        self.push_str("\n");
        self.write_indent(indent);
        self.emit(node, source);
    }

    /// Emit a trailing comment on the same line as the parent statement.
    /// Used for `# gd:ignore[rule]` and other inline comments on signals, vars, etc.
    pub(crate) fn print_trailing_comment(&mut self, comment: &Node, source: &str, parent: &Node) {
        if comment.start_position().row == parent.start_position().row {
            self.push_str("  ");
            self.emit(comment, source);
        }
    }
}
