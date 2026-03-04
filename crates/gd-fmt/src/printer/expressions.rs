//! Expression formatting: unary_op, await, parenthesized, subscript,
//! conditional_expression, call, attribute, attribute_call, print_children, lambda.

use tree_sitter::Node;

use super::Printer;

impl Printer {
    // ── Unary operator ─────────────────────────────────────────────────

    pub(crate) fn print_unary_op(&mut self, node: &Node, source: &str, indent: usize) {
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();

        for child in &children {
            if child.is_named() {
                // For word operators like "not", add space
                if children.first().is_some_and(|c| c.kind() == "not") {
                    self.push_str(" ");
                }
                self.print_node(child, source, indent);
            } else {
                self.emit(child, source);
            }
        }
    }

    // ── Await expression ──────────────────────────────────────────────

    pub(crate) fn print_await_expr(&mut self, node: &Node, source: &str, indent: usize) {
        self.push_str("await ");
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            self.print_node(&child, source, indent);
        }
    }

    // ── Parenthesized expression ───────────────────────────────────────

    pub(crate) fn print_parenthesized(&mut self, node: &Node, source: &str, indent: usize) {
        let is_multiline = node.start_position().row != node.end_position().row;
        if is_multiline {
            // Preserve multiline parenthesized expressions to avoid collapsing
            // line comments into subsequent code (e.g. `# comment + c`)
            self.emit(node, source);
        } else {
            self.push_str("(");
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                self.print_node(&child, source, indent);
            }
            self.push_str(")");
        }
    }

    // ── Subscript ──────────────────────────────────────────────────────

    pub(crate) fn print_subscript(&mut self, node: &Node, source: &str, indent: usize) {
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();

        for child in &children {
            if child.kind() == "[" || child.kind() == "]" {
                self.emit(child, source);
            } else {
                self.print_node(child, source, indent);
            }
        }
    }

    // ── Conditional expression (ternary) ───────────────────────────────

    pub(crate) fn print_conditional_expr(&mut self, node: &Node, source: &str, indent: usize) {
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();

        for child in &children {
            match child.kind() {
                "if" => self.push_str(" if "),
                "else" => self.push_str(" else "),
                _ if child.is_named() => self.print_node(child, source, indent),
                _ => {}
            }
        }
    }

    // ── Generic children printer ───────────────────────────────────────

    pub(crate) fn print_children(&mut self, node: &Node, source: &str, indent: usize) {
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            self.print_node(&child, source, indent);
        }
    }

    // ── Call / attribute access ─────────────────────────────────────────

    pub(crate) fn print_call(&mut self, node: &Node, source: &str, indent: usize) {
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();

        for child in &children {
            match child.kind() {
                "arguments" => self.print_paren_list(child, source, indent),
                _ => self.print_node(child, source, indent),
            }
        }
    }

    pub(crate) fn print_attribute(&mut self, node: &Node, source: &str, indent: usize) {
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();

        for child in &children {
            if child.kind() == "." {
                self.push_str(".");
            } else {
                self.print_node(child, source, indent);
            }
        }
    }
}
