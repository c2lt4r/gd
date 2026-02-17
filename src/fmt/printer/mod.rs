//! AST-aware formatter - walks the tree-sitter tree and emits formatted source.
//!
//! Strategy: We walk the AST recursively. For leaf nodes (tokens), we emit the
//! original source text. For interior nodes, we control whitespace between children
//! based on formatting rules. Indentation is tracked via the AST's `body` and
//! `class_body` nodes.
//!
//! Important: Some tree-sitter-gdscript nodes (string, get_node, comment) have
//! internal content not represented as children. For these "opaque" nodes we emit
//! the full source text directly rather than recursing into children.

mod annotations;
mod collections;
mod expressions;
mod statements;
#[cfg(test)]
mod tests;

use tree_sitter::Node;

use super::rules;
use crate::core::config::FmtConfig;

/// Node kinds whose full source text should be emitted verbatim.
/// These nodes have internal content that is NOT exposed as child nodes.
fn is_opaque(kind: &str) -> bool {
    matches!(
        kind,
        "string"
            | "get_node"
            | "comment"
            | "integer"
            | "float"
            | "true"
            | "false"
            | "null"
            | "identifier"
            | "name"
            | "self"
            | "string_name"
            | "node_path"
    )
}

/// An entry in a multiline collection: either an item (with optional trailing
/// comment on the same line) or a standalone comment.
struct MultilineEntry<'a> {
    node: Node<'a>,
    trailing_comment: Option<Node<'a>>,
}

/// Collect children of a multiline collection into entries, pairing items with
/// their trailing comments. Brackets and commas are stripped.
fn collect_multiline_entries<'a>(
    children: &[Node<'a>],
    is_bracket: impl Fn(&str) -> bool,
) -> Vec<MultilineEntry<'a>> {
    let mut entries = Vec::new();
    let mut i = 0;
    while i < children.len() {
        let child = &children[i];
        let kind = child.kind();
        if is_bracket(kind) || kind == "," || kind == "line_continuation" {
            i += 1;
            continue;
        }
        if kind == "comment" {
            // Check if this comment is on the same line as the previous item
            if let Some(last) = entries.last_mut() {
                let last_entry: &mut MultilineEntry = last;
                if last_entry.trailing_comment.is_none()
                    && last_entry.node.end_position().row == child.start_position().row
                {
                    last_entry.trailing_comment = Some(*child);
                    i += 1;
                    continue;
                }
            }
            // Standalone comment (e.g. section header)
            entries.push(MultilineEntry {
                node: *child,
                trailing_comment: None,
            });
            i += 1;
            continue;
        }
        entries.push(MultilineEntry {
            node: *child,
            trailing_comment: None,
        });
        i += 1;
    }
    entries
}

pub struct Printer {
    output: String,
    use_tabs: bool,
    indent_size: usize,
    blank_lines_around_functions: usize,
    blank_lines_around_classes: usize,
    trailing_newline: bool,
}

impl Printer {
    #[cfg(test)]
    pub fn new(use_tabs: bool, indent_size: usize) -> Self {
        Self {
            output: String::new(),
            use_tabs,
            indent_size,
            blank_lines_around_functions: 2,
            blank_lines_around_classes: 2,
            trailing_newline: true,
        }
    }

    pub fn from_config(config: &FmtConfig) -> Self {
        Self {
            output: String::new(),
            use_tabs: config.use_tabs,
            indent_size: config.indent_size,
            blank_lines_around_functions: config.blank_lines_around_functions,
            blank_lines_around_classes: config.blank_lines_around_classes,
            trailing_newline: config.trailing_newline,
        }
    }

    pub fn finish(mut self) -> String {
        let trimmed = self.output.trim_end().len();
        self.output.truncate(trimmed);
        if self.trailing_newline {
            self.output.push('\n');
        }
        self.output
    }

    pub fn format(&mut self, root: &Node, source: &str) {
        self.print_node(root, source, 0);
    }

    // ── Node dispatch ─────────────────────────────────────────────────

    pub(crate) fn print_node(&mut self, node: &Node, source: &str, indent: usize) {
        // Opaque nodes: emit full source text
        if is_opaque(node.kind()) {
            self.emit(node, source);
            return;
        }

        match node.kind() {
            "source" => self.print_source(node, source, indent),
            "class_body" => self.print_body_block(node, source, indent, true),
            "body" => self.print_body_block(node, source, indent, false),
            "function_definition" => self.print_function_def(node, source, indent),
            "class_definition" => self.print_class_def(node, source, indent),
            "if_statement" => self.print_if_statement(node, source, indent),
            "elif_clause" => self.print_elif_clause(node, source, indent),
            "else_clause" => self.print_else_clause(node, source, indent),
            "for_statement" => self.print_for_statement(node, source, indent),
            "while_statement" => self.print_while_statement(node, source, indent),
            "match_statement" => self.print_match_statement(node, source, indent),
            "match_body" => self.print_match_body(node, source, indent),
            "pattern_section" => self.print_pattern_section(node, source, indent),
            "variable_statement" | "const_statement" => {
                self.print_var_or_const(node, source, indent);
            }
            "expression_statement" => self.print_children(node, source, indent),
            "return_statement" => self.print_return(node, source, indent),
            "assignment" | "augmented_assignment" => {
                self.print_assignment(node, source, indent);
            }
            "binary_operator" => self.print_binary_op(node, source, indent),
            "signal_statement" => self.print_signal(node, source, indent),
            "enum_definition" => self.print_enum_def(node, source, indent),
            "extends_statement" => self.print_extends(node, source),
            "class_name_statement" => self.print_class_name(node, source),
            "annotation" | "lambda" => self.emit(node, source),
            "annotations" => self.print_annotations_standalone(node, source, indent),
            "dictionary" => self.print_dictionary(node, source, indent),
            "array" => self.print_array(node, source, indent),
            "call" | "attribute_call" => self.print_call(node, source, indent),
            "arguments" | "parameters" => self.print_paren_list(node, source, indent),
            "pair" => self.print_dict_pair(node, source, indent),
            "typed_parameter" => self.print_typed_param(node, source),
            "attribute" => self.print_attribute(node, source, indent),
            "enumerator_list" => self.print_enumerator_list(node, source, indent),
            "inferred_type" => self.push_str(":="),
            "type" => self.print_type(node, source),
            "pass_statement" => self.push_str("pass"),
            "break_statement" => self.push_str("break"),
            "continue_statement" => self.push_str("continue"),
            "unary_operator" => self.print_unary_op(node, source, indent),
            "await_expression" => self.print_await_expr(node, source, indent),
            "parenthesized_expression" => self.print_parenthesized(node, source, indent),
            "subscript" => self.print_subscript(node, source, indent),
            "conditional_expression" => self.print_conditional_expr(node, source, indent),
            "line_continuation" => {} // Skip — formatter controls line breaks
            _ => {
                // Fallback: if leaf, emit text; otherwise emit full source text
                // (safe default - preserves content)
                self.emit(node, source);
            }
        }
    }

    // ── Source (top-level) ──────────────────────────────────────────────

    fn print_source(&mut self, node: &Node, source: &str, indent: usize) {
        let mut cursor = node.walk();
        let children: Vec<Node> = node.named_children(&mut cursor).collect();
        let children: Vec<&Node> = children
            .iter()
            .filter(|c| c.kind() != "line_continuation")
            .collect();

        for (i, child) in children.iter().enumerate() {
            // Trailing comment on same line as previous statement
            if child.kind() == "comment"
                && i > 0
                && child.start_position().row == children[i - 1].start_position().row
            {
                self.push_str("  ");
                self.emit(child, source);
                continue;
            }
            if i > 0 {
                let prev = children[i - 1];
                let blank_lines = rules::spacing_between(
                    prev,
                    child,
                    false,
                    self.blank_lines_around_functions,
                    self.blank_lines_around_classes,
                    source,
                );
                self.write_blank_lines(blank_lines);
            }
            self.write_indent(indent);
            self.print_node(child, source, indent);
        }
    }

    // ── Body blocks (function body, class body) ────────────────────────

    pub(crate) fn print_body_block(
        &mut self,
        node: &Node,
        source: &str,
        indent: usize,
        is_class_body: bool,
    ) {
        let inner_indent = indent + 1;
        let mut cursor = node.walk();
        let children: Vec<Node> = node.named_children(&mut cursor).collect();
        // Filter out line_continuation nodes — they have no output and
        // would otherwise produce blank lines in the body
        let children: Vec<&Node> = children
            .iter()
            .filter(|c| c.kind() != "line_continuation")
            .collect();

        for (i, child) in children.iter().enumerate() {
            // Trailing comment on same line as previous statement
            if child.kind() == "comment"
                && i > 0
                && child.start_position().row == children[i - 1].start_position().row
            {
                self.push_str("  ");
                self.emit(child, source);
                continue;
            }
            if i > 0 && is_class_body {
                let prev = children[i - 1];
                let blank_lines = rules::spacing_between(
                    prev,
                    child,
                    true,
                    self.blank_lines_around_functions,
                    self.blank_lines_around_classes,
                    source,
                );
                self.write_blank_lines(blank_lines);
            } else {
                self.push_str("\n");
            }
            self.write_indent(inner_indent);
            self.print_node(child, source, inner_indent);
        }
    }

    // ── Function definition ────────────────────────────────────────────

    fn print_function_def(&mut self, node: &Node, source: &str, indent: usize) {
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();

        for child in &children {
            match child.kind() {
                "annotations" => {
                    self.print_annotations_block(child, source, indent);
                }
                "static_keyword" => self.push_str("static "),
                "func" => self.push_str("func"),
                "name" => {
                    self.push_str(" ");
                    self.emit(child, source);
                }
                "parameters" => self.print_paren_list(child, source, indent),
                "->" => self.push_str(" -> "),
                "type" => self.print_type(child, source),
                ":" => self.push_str(":"),
                "comment" => self.print_inline_comment(child, source, indent + 1),
                "body" => self.print_body_block(child, source, indent, false),
                _ => {}
            }
        }
    }

    // ── Class definition ───────────────────────────────────────────────

    fn print_class_def(&mut self, node: &Node, source: &str, indent: usize) {
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();

        for child in &children {
            match child.kind() {
                "class" => self.push_str("class"),
                "name" => {
                    self.push_str(" ");
                    self.emit(child, source);
                }
                "extends" => self.push_str(" extends "),
                "type" => self.print_type(child, source),
                ":" => self.push_str(":"),
                "comment" => self.print_inline_comment(child, source, indent + 1),
                "class_body" => self.print_body_block(child, source, indent, true),
                _ => {}
            }
        }
    }

    // ── Primitive helpers ──────────────────────────────────────────────

    /// Emit the full source text of a node.
    pub(crate) fn emit(&mut self, node: &Node, source: &str) {
        if let Ok(text) = node.utf8_text(source.as_bytes()) {
            self.output.push_str(text);
        }
    }

    pub(crate) fn push_str(&mut self, s: &str) {
        self.output.push_str(s);
    }

    /// Write a newline plus `count` blank lines.
    pub(crate) fn write_blank_lines(&mut self, count: usize) {
        for _ in 0..=count {
            self.output.push('\n');
        }
    }

    pub(crate) fn write_indent(&mut self, level: usize) {
        if self.use_tabs {
            for _ in 0..level {
                self.output.push('\t');
            }
        } else {
            let spaces = level * self.indent_size;
            for _ in 0..spaces {
                self.output.push(' ');
            }
        }
    }
}
