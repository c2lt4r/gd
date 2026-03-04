//! Statement-level formatting: if/elif/else, for, while, match, var/const,
//! signal, extends, class_name, return, assignment, binary_op, pass, break, continue.

use tree_sitter::Node;

use super::Printer;

impl Printer {
    // ── If/elif/else ───────────────────────────────────────────────────

    pub(crate) fn print_if_statement(&mut self, node: &Node, source: &str, indent: usize) {
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();

        for child in &children {
            match child.kind() {
                "if" => self.push_str("if "),
                ":" => self.push_str(":"),
                "body" => self.print_body_block(child, source, indent, false),
                "elif_clause" => {
                    self.push_str("\n");
                    self.write_indent(indent);
                    self.print_elif_clause(child, source, indent);
                }
                "else_clause" => {
                    self.push_str("\n");
                    self.write_indent(indent);
                    self.print_else_clause(child, source, indent);
                }
                "comment" => self.print_inline_comment(child, source, indent + 1),
                _ if child.is_named() => self.print_node(child, source, indent),
                _ => {}
            }
        }
    }

    pub(crate) fn print_elif_clause(&mut self, node: &Node, source: &str, indent: usize) {
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();

        for child in &children {
            match child.kind() {
                "elif" => self.push_str("elif "),
                ":" => self.push_str(":"),
                "comment" => self.print_inline_comment(child, source, indent + 1),
                "body" => self.print_body_block(child, source, indent, false),
                _ if child.is_named() => self.print_node(child, source, indent),
                _ => {}
            }
        }
    }

    pub(crate) fn print_else_clause(&mut self, node: &Node, source: &str, indent: usize) {
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();

        for child in &children {
            match child.kind() {
                "else" => self.push_str("else"),
                ":" => self.push_str(":"),
                "comment" => self.print_inline_comment(child, source, indent + 1),
                "body" => self.print_body_block(child, source, indent, false),
                _ => {}
            }
        }
    }

    // ── For/while loops ────────────────────────────────────────────────

    pub(crate) fn print_for_statement(&mut self, node: &Node, source: &str, indent: usize) {
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();

        for child in &children {
            match child.kind() {
                "for" => self.push_str("for "),
                "in" => self.push_str(" in "),
                ":" => self.push_str(":"),
                "comment" => self.print_inline_comment(child, source, indent + 1),
                "body" => self.print_body_block(child, source, indent, false),
                _ if child.is_named() => self.print_node(child, source, indent),
                _ => {}
            }
        }
    }

    pub(crate) fn print_while_statement(&mut self, node: &Node, source: &str, indent: usize) {
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();

        for child in &children {
            match child.kind() {
                "while" => self.push_str("while "),
                ":" => self.push_str(":"),
                "comment" => self.print_inline_comment(child, source, indent + 1),
                "body" => self.print_body_block(child, source, indent, false),
                _ if child.is_named() => self.print_node(child, source, indent),
                _ => {}
            }
        }
    }

    // ── Match statement ────────────────────────────────────────────────

    pub(crate) fn print_match_statement(&mut self, node: &Node, source: &str, indent: usize) {
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();

        for child in &children {
            match child.kind() {
                "match" => self.push_str("match "),
                ":" => self.push_str(":"),
                "comment" => self.print_inline_comment(child, source, indent + 1),
                "match_body" => self.print_match_body(child, source, indent),
                _ if child.is_named() => self.print_node(child, source, indent),
                _ => {}
            }
        }
    }

    pub(crate) fn print_match_body(&mut self, node: &Node, source: &str, indent: usize) {
        let inner_indent = indent + 1;
        let mut cursor = node.walk();
        let children: Vec<Node> = node.named_children(&mut cursor).collect();

        for child in &children {
            self.push_str("\n");
            self.write_indent(inner_indent);
            self.print_node(child, source, inner_indent);
        }
    }

    pub(crate) fn print_pattern_section(&mut self, node: &Node, source: &str, indent: usize) {
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();

        for child in &children {
            match child.kind() {
                ":" => self.push_str(":"),
                "pattern_guard" => {
                    self.push_str(" ");
                    self.emit(child, source);
                }
                "comment" => self.print_inline_comment(child, source, indent + 1),
                "body" => self.print_body_block(child, source, indent, false),
                "," => self.push_str(", "),
                _ if child.is_named() => self.print_node(child, source, indent),
                _ => {}
            }
        }
    }

    // ── Variable / Const statement ─────────────────────────────────────
    // Children: [annotations]? var/const name [:type] [= value]

    pub(crate) fn print_var_or_const(&mut self, node: &Node, source: &str, indent: usize) {
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();

        for child in &children {
            match child.kind() {
                "annotations" => {
                    self.print_annotations_block(child, source, indent);
                }
                "static_keyword" => self.push_str("static "),
                "var" => self.push_str("var"),
                "const" => self.push_str("const"),
                "name" => {
                    self.push_str(" ");
                    self.emit(child, source);
                }
                ":" => self.push_str(":"),
                "type" => {
                    self.push_str(" ");
                    self.print_type(child, source);
                }
                "inferred_type" => {
                    self.push_str(" := ");
                }
                "=" => self.push_str(" = "),
                "comment" => self.print_trailing_comment(child, source, node),
                _ if child.is_named() => self.print_node(child, source, indent),
                _ => {}
            }
        }
    }

    // ── Signal statement ───────────────────────────────────────────────

    pub(crate) fn print_signal(&mut self, node: &Node, source: &str, indent: usize) {
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();

        for child in &children {
            match child.kind() {
                "signal" => self.push_str("signal"),
                "name" => {
                    self.push_str(" ");
                    self.emit(child, source);
                }
                "parameters" => self.print_paren_list(child, source, indent),
                "comment" => self.print_trailing_comment(child, source, node),
                _ => {}
            }
        }
    }

    // ── Extends / class_name ───────────────────────────────────────────

    pub(crate) fn print_extends(&mut self, node: &Node, source: &str) {
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();

        for child in &children {
            match child.kind() {
                "extends" => self.push_str("extends"),
                "comment" => self.print_trailing_comment(child, source, node),
                // type (extends Node2D) or string (extends "res://path.gd")
                _ if child.is_named() => {
                    self.push_str(" ");
                    self.emit(child, source);
                }
                _ => {}
            }
        }
    }

    pub(crate) fn print_class_name(&mut self, node: &Node, source: &str) {
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();

        for child in &children {
            match child.kind() {
                "class_name" => self.push_str("class_name"),
                "name" => {
                    self.push_str(" ");
                    self.emit(child, source);
                }
                "comment" => self.print_trailing_comment(child, source, node),
                _ => {}
            }
        }
    }

    // ── Return statement ───────────────────────────────────────────────

    pub(crate) fn print_return(&mut self, node: &Node, source: &str, indent: usize) {
        self.push_str("return");
        let mut cursor = node.walk();
        let children: Vec<Node> = node.named_children(&mut cursor).collect();
        for child in &children {
            if child.kind() == "comment" {
                self.print_trailing_comment(child, source, node);
            } else {
                self.push_str(" ");
                self.print_node(child, source, indent);
            }
        }
    }

    // ── Assignment ─────────────────────────────────────────────────────

    pub(crate) fn print_assignment(&mut self, node: &Node, source: &str, indent: usize) {
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();

        let mut first = true;
        for child in &children {
            if child.kind() == "line_continuation" {
                continue;
            }
            if child.kind() == "comment" {
                self.print_trailing_comment(child, source, node);
                continue;
            }
            if child.is_named() {
                if !first {
                    self.push_str(" ");
                }
                self.print_node(child, source, indent);
            } else {
                // Operator token (=, +=, -=, etc.)
                self.push_str(" ");
                self.emit(child, source);
            }
            first = false;
        }
    }

    // ── Binary operator ────────────────────────────────────────────────

    pub(crate) fn print_binary_op(&mut self, node: &Node, source: &str, indent: usize) {
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();

        let mut first = true;
        for child in &children {
            if child.kind() == "line_continuation" {
                continue;
            }
            if !first {
                self.push_str(" ");
            }
            first = false;
            if child.is_named() {
                self.print_node(child, source, indent);
            } else {
                self.emit(child, source);
            }
        }
    }
}
