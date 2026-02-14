//! Collection formatting: array, dictionary (inline/multiline), enum, enumerator.

use tree_sitter::Node;

use super::{Printer, collect_multiline_entries};

impl Printer {
    // ── Enum definition ────────────────────────────────────────────────

    pub(crate) fn print_enum_def(&mut self, node: &Node, source: &str, _indent: usize) {
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();

        for child in &children {
            match child.kind() {
                "enum" => self.push_str("enum"),
                "name" => {
                    self.push_str(" ");
                    self.emit(child, source);
                }
                "enumerator_list" => {
                    self.push_str(" ");
                    self.print_enumerator_list(child, source);
                }
                _ => {}
            }
        }
    }

    pub(crate) fn print_enumerator_list(&mut self, node: &Node, source: &str) {
        self.push_str("{ ");
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();

        for child in &children {
            match child.kind() {
                "," => self.push_str(", "),
                "enumerator" => {
                    self.print_enumerator(child, source);
                }
                _ => {}
            }
        }
        self.push_str(" }");
    }

    fn print_enumerator(&mut self, node: &Node, source: &str) {
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();

        for (i, child) in children.iter().enumerate() {
            if i > 0 && child.kind() == "=" {
                self.push_str(" = ");
            } else if i > 0 && children.get(i - 1).is_some_and(|c| c.kind() == "=") {
                // value after = already got space from above
                self.emit(child, source);
            } else {
                self.emit(child, source);
            }
        }
    }

    // ── Dictionary ─────────────────────────────────────────────────────

    pub(crate) fn print_dictionary(&mut self, node: &Node, source: &str, indent: usize) {
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();

        let is_multiline = node.start_position().row != node.end_position().row;
        let has_comment = children.iter().any(|c| c.kind() == "comment");
        let has_line_continuation = children.iter().any(|c| c.kind() == "line_continuation");

        if has_line_continuation {
            self.print_dict_inline(&children, source, indent);
        } else if is_multiline || has_comment {
            self.print_dict_multiline(&children, source, indent);
        } else {
            self.print_dict_inline(&children, source, indent);
        }
    }

    fn print_dict_inline(&mut self, children: &[Node], source: &str, indent: usize) {
        self.push_str("{");
        for child in children {
            match child.kind() {
                "{" | "}" => {}
                "," => self.push_str(", "),
                _ if child.is_named() => {
                    self.print_node(child, source, indent);
                }
                _ => {}
            }
        }
        self.push_str("}");
    }

    fn print_dict_multiline(&mut self, children: &[Node], source: &str, indent: usize) {
        let inner = indent + 1;
        self.push_str("{");
        let entries = collect_multiline_entries(children, |k| matches!(k, "{" | "}"));
        for entry in &entries {
            self.push_str("\n");
            self.write_indent(inner);
            if entry.node.kind() == "comment" {
                self.emit(&entry.node, source);
                continue;
            }
            self.print_node(&entry.node, source, inner);
            self.push_str(",");
            if let Some(ref c) = entry.trailing_comment {
                self.push_str("  ");
                self.emit(c, source);
            }
        }
        if !entries.is_empty() {
            self.push_str("\n");
            self.write_indent(indent);
        }
        self.push_str("}");
    }

    pub(crate) fn print_dict_pair(&mut self, node: &Node, source: &str, indent: usize) {
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();

        for child in &children {
            match child.kind() {
                ":" => self.push_str(": "),
                _ => self.print_node(child, source, indent),
            }
        }
    }

    // ── Array ──────────────────────────────────────────────────────────

    pub(crate) fn print_array(&mut self, node: &Node, source: &str, indent: usize) {
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();

        let is_multiline = node.start_position().row != node.end_position().row;
        let has_comment = children.iter().any(|c| c.kind() == "comment");
        let has_line_continuation = children.iter().any(|c| c.kind() == "line_continuation");

        if has_line_continuation {
            // Line continuation (\) makes the array span multiple rows but
            // it's logically a single line — format as inline
            self.print_array_inline(&children, source, indent);
        } else if is_multiline || has_comment {
            self.print_array_multiline(&children, source, indent);
        } else {
            self.print_array_inline(&children, source, indent);
        }
    }

    fn print_array_inline(&mut self, children: &[Node], source: &str, indent: usize) {
        self.push_str("[");
        for child in children {
            match child.kind() {
                "[" | "]" | "line_continuation" => {}
                "," => self.push_str(", "),
                _ => {
                    self.print_node(child, source, indent);
                }
            }
        }
        self.push_str("]");
    }

    fn print_array_multiline(&mut self, children: &[Node], source: &str, indent: usize) {
        let inner = indent + 1;
        self.push_str("[");
        let entries = collect_multiline_entries(children, |k| matches!(k, "[" | "]"));
        for entry in &entries {
            self.push_str("\n");
            self.write_indent(inner);
            if entry.node.kind() == "comment" {
                self.emit(&entry.node, source);
                continue;
            }
            self.print_node(&entry.node, source, inner);
            self.push_str(",");
            if let Some(ref c) = entry.trailing_comment {
                self.push_str("  ");
                self.emit(c, source);
            }
        }
        if !entries.is_empty() {
            self.push_str("\n");
            self.write_indent(indent);
        }
        self.push_str("]");
    }
}
