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

use tree_sitter::Node;

use super::rules::{self, Spacing};

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

pub struct Printer {
    output: String,
    use_tabs: bool,
    indent_size: usize,
}

impl Printer {
    pub fn new(use_tabs: bool, indent_size: usize) -> Self {
        Self {
            output: String::new(),
            use_tabs,
            indent_size,
        }
    }

    pub fn finish(mut self) -> String {
        // Ensure single trailing newline
        let trimmed = self.output.trim_end().len();
        self.output.truncate(trimmed);
        self.output.push('\n');
        self.output
    }

    pub fn format(&mut self, root: &Node, source: &str) {
        self.print_node(root, source, 0);
    }

    fn print_node(&mut self, node: &Node, source: &str, indent: usize) {
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
            "variable_statement" => self.print_var_or_const(node, source, indent),
            "const_statement" => self.print_var_or_const(node, source, indent),
            "expression_statement" => self.print_children(node, source, indent),
            "return_statement" => self.print_return(node, source, indent),
            "assignment" => self.print_assignment(node, source, indent),
            "augmented_assignment" => self.print_assignment(node, source, indent),
            "binary_operator" => self.print_binary_op(node, source, indent),
            "signal_statement" => self.print_signal(node, source, indent),
            "enum_definition" => self.print_enum_def(node, source, indent),
            "extends_statement" => self.print_extends(node, source),
            "class_name_statement" => self.print_class_name(node, source),
            "annotation" => self.emit(node, source),
            "annotations" => self.print_annotations_standalone(node, source, indent),
            "dictionary" => self.print_dictionary(node, source, indent),
            "array" => self.print_array(node, source, indent),
            "call" => self.print_call(node, source, indent),
            "attribute_call" => self.print_call(node, source, indent),
            "arguments" | "parameters" => self.print_paren_list(node, source, indent),
            "pair" => self.print_dict_pair(node, source, indent),
            "typed_parameter" => self.print_typed_param(node, source),
            "attribute" => self.print_attribute(node, source, indent),
            "enumerator_list" => self.print_enumerator_list(node, source),
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
            "lambda" => self.emit(node, source),
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

        for (i, child) in children.iter().enumerate() {
            if i > 0 {
                let prev = &children[i - 1];
                let spacing = rules::spacing_between(prev, child, false);
                match spacing {
                    Spacing::TwoBlankLines => self.push_str("\n\n\n"),
                    Spacing::BlankLine => self.push_str("\n\n"),
                    Spacing::None => self.push_str("\n"),
                }
            }
            self.write_indent(indent);
            self.print_node(child, source, indent);
        }
    }

    // ── Body blocks (function body, class body) ────────────────────────

    fn print_body_block(
        &mut self,
        node: &Node,
        source: &str,
        indent: usize,
        is_class_body: bool,
    ) {
        let inner_indent = indent + 1;
        let mut cursor = node.walk();
        let children: Vec<Node> = node.named_children(&mut cursor).collect();

        for (i, child) in children.iter().enumerate() {
            if i > 0 && is_class_body {
                let prev = &children[i - 1];
                let spacing = rules::spacing_between(prev, child, true);
                match spacing {
                    Spacing::TwoBlankLines => self.push_str("\n\n\n"),
                    Spacing::BlankLine => self.push_str("\n\n"),
                    Spacing::None => self.push_str("\n"),
                }
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
                "static" => self.push_str("static "),
                "func" => self.push_str("func"),
                "name" => {
                    self.push_str(" ");
                    self.emit(child, source);
                }
                "parameters" => self.print_paren_list(child, source, indent),
                "->" => self.push_str(" -> "),
                "type" => self.print_type(child, source),
                ":" => self.push_str(":"),
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
                "class_body" => self.print_body_block(child, source, indent, true),
                _ => {}
            }
        }
    }

    // ── If/elif/else ───────────────────────────────────────────────────

    fn print_if_statement(&mut self, node: &Node, source: &str, indent: usize) {
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
                _ if child.is_named() => self.print_node(child, source, indent),
                _ => {}
            }
        }
    }

    fn print_elif_clause(&mut self, node: &Node, source: &str, indent: usize) {
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();

        for child in &children {
            match child.kind() {
                "elif" => self.push_str("elif "),
                ":" => self.push_str(":"),
                "body" => self.print_body_block(child, source, indent, false),
                _ if child.is_named() => self.print_node(child, source, indent),
                _ => {}
            }
        }
    }

    fn print_else_clause(&mut self, node: &Node, source: &str, indent: usize) {
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();

        for child in &children {
            match child.kind() {
                "else" => self.push_str("else"),
                ":" => self.push_str(":"),
                "body" => self.print_body_block(child, source, indent, false),
                _ => {}
            }
        }
    }

    // ── For/while loops ────────────────────────────────────────────────

    fn print_for_statement(&mut self, node: &Node, source: &str, indent: usize) {
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();

        for child in &children {
            match child.kind() {
                "for" => self.push_str("for "),
                "in" => self.push_str(" in "),
                ":" => self.push_str(":"),
                "body" => self.print_body_block(child, source, indent, false),
                _ if child.is_named() => self.print_node(child, source, indent),
                _ => {}
            }
        }
    }

    fn print_while_statement(&mut self, node: &Node, source: &str, indent: usize) {
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();

        for child in &children {
            match child.kind() {
                "while" => self.push_str("while "),
                ":" => self.push_str(":"),
                "body" => self.print_body_block(child, source, indent, false),
                _ if child.is_named() => self.print_node(child, source, indent),
                _ => {}
            }
        }
    }

    // ── Match statement ────────────────────────────────────────────────

    fn print_match_statement(&mut self, node: &Node, source: &str, indent: usize) {
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();

        for child in &children {
            match child.kind() {
                "match" => self.push_str("match "),
                ":" => self.push_str(":"),
                "match_body" => self.print_match_body(child, source, indent),
                _ if child.is_named() => self.print_node(child, source, indent),
                _ => {}
            }
        }
    }

    fn print_match_body(&mut self, node: &Node, source: &str, indent: usize) {
        let inner_indent = indent + 1;
        let mut cursor = node.walk();
        let children: Vec<Node> = node.named_children(&mut cursor).collect();

        for child in &children {
            self.push_str("\n");
            self.write_indent(inner_indent);
            self.print_node(child, source, inner_indent);
        }
    }

    fn print_pattern_section(&mut self, node: &Node, source: &str, indent: usize) {
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();

        for child in &children {
            match child.kind() {
                ":" => self.push_str(":"),
                "body" => self.print_body_block(child, source, indent, false),
                "," => self.push_str(", "),
                _ if child.is_named() => self.print_node(child, source, indent),
                _ => {}
            }
        }
    }

    // ── Variable / Const statement ─────────────────────────────────────
    // Children: [annotations]? var/const name [:type] [= value]

    fn print_var_or_const(&mut self, node: &Node, source: &str, indent: usize) {
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();

        for child in &children {
            match child.kind() {
                "annotations" => {
                    self.print_annotations_block(child, source, indent);
                }
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
                _ if child.is_named() => self.print_node(child, source, indent),
                _ => {}
            }
        }
    }

    // ── Signal statement ───────────────────────────────────────────────

    fn print_signal(&mut self, node: &Node, source: &str, indent: usize) {
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
                _ => {}
            }
        }
    }

    // ── Extends / class_name ───────────────────────────────────────────

    fn print_extends(&mut self, node: &Node, source: &str) {
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();

        for child in &children {
            match child.kind() {
                "extends" => self.push_str("extends"),
                "type" => {
                    self.push_str(" ");
                    self.print_type(child, source);
                }
                _ => {}
            }
        }
    }

    fn print_class_name(&mut self, node: &Node, source: &str) {
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();

        for child in &children {
            match child.kind() {
                "class_name" => self.push_str("class_name"),
                "name" => {
                    self.push_str(" ");
                    self.emit(child, source);
                }
                _ => {}
            }
        }
    }

    // ── Return statement ───────────────────────────────────────────────

    fn print_return(&mut self, node: &Node, source: &str, indent: usize) {
        self.push_str("return");
        let mut cursor = node.walk();
        let children: Vec<Node> = node.named_children(&mut cursor).collect();
        if !children.is_empty() {
            self.push_str(" ");
            self.print_node(&children[0], source, indent);
        }
    }

    // ── Assignment ─────────────────────────────────────────────────────

    fn print_assignment(&mut self, node: &Node, source: &str, indent: usize) {
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();

        for (i, child) in children.iter().enumerate() {
            if child.is_named() {
                if i > 0 {
                    self.push_str(" ");
                }
                self.print_node(child, source, indent);
            } else {
                // Operator token (=, +=, -=, etc.)
                self.push_str(" ");
                self.emit(child, source);
            }
        }
    }

    // ── Binary operator ────────────────────────────────────────────────

    fn print_binary_op(&mut self, node: &Node, source: &str, indent: usize) {
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();

        for (i, child) in children.iter().enumerate() {
            if i > 0 {
                self.push_str(" ");
            }
            if child.is_named() {
                self.print_node(child, source, indent);
            } else {
                self.emit(child, source);
            }
        }
    }

    // ── Unary operator ─────────────────────────────────────────────────

    fn print_unary_op(&mut self, node: &Node, source: &str, indent: usize) {
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

    fn print_await_expr(&mut self, node: &Node, source: &str, indent: usize) {
        self.push_str("await ");
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            self.print_node(&child, source, indent);
        }
    }

    // ── Parenthesized expression ───────────────────────────────────────

    fn print_parenthesized(&mut self, node: &Node, source: &str, indent: usize) {
        self.push_str("(");
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            self.print_node(&child, source, indent);
        }
        self.push_str(")");
    }

    // ── Subscript ──────────────────────────────────────────────────────

    fn print_subscript(&mut self, node: &Node, source: &str, indent: usize) {
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

    fn print_conditional_expr(&mut self, node: &Node, source: &str, indent: usize) {
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

    fn print_children(&mut self, node: &Node, source: &str, indent: usize) {
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            self.print_node(&child, source, indent);
        }
    }

    // ── Annotations ────────────────────────────────────────────────────

    /// Print annotations inline before a declaration (var/func/etc).
    /// Multiple annotations: each on its own line, last one on the same line as the declaration.
    fn print_annotations_block(&mut self, node: &Node, source: &str, indent: usize) {
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
    fn print_annotations_standalone(&mut self, node: &Node, source: &str, _indent: usize) {
        let mut cursor = node.walk();
        let children: Vec<Node> = node.named_children(&mut cursor).collect();

        for child in &children {
            self.emit(child, source);
        }
    }

    // ── Enum definition ────────────────────────────────────────────────

    fn print_enum_def(&mut self, node: &Node, source: &str, _indent: usize) {
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

    fn print_enumerator_list(&mut self, node: &Node, source: &str) {
        self.push_str("{ ");
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();

        let mut first = true;
        for child in &children {
            match child.kind() {
                "{" | "}" => {}
                "," => self.push_str(", "),
                "enumerator" => {
                    if !first {
                        // comma already added
                    }
                    first = false;
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

    fn print_dictionary(&mut self, node: &Node, source: &str, indent: usize) {
        self.push_str("{");
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();

        let mut first = true;
        for child in &children {
            match child.kind() {
                "{" | "}" => {}
                "," => self.push_str(", "),
                _ if child.is_named() => {
                    if !first {
                        // comma already handled
                    }
                    first = false;
                    self.print_node(child, source, indent);
                }
                _ => {}
            }
        }
        self.push_str("}");
    }

    fn print_dict_pair(&mut self, node: &Node, source: &str, indent: usize) {
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

    fn print_array(&mut self, node: &Node, source: &str, indent: usize) {
        self.push_str("[");
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();

        let mut first = true;
        for child in &children {
            match child.kind() {
                "[" | "]" => {}
                "," => self.push_str(", "),
                _ => {
                    if !first {
                        // comma already handled
                    }
                    first = false;
                    self.print_node(child, source, indent);
                }
            }
        }
        self.push_str("]");
    }

    // ── Call / attribute access ─────────────────────────────────────────

    fn print_call(&mut self, node: &Node, source: &str, indent: usize) {
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();

        for child in &children {
            match child.kind() {
                "arguments" => self.print_paren_list(child, source, indent),
                _ => self.print_node(child, source, indent),
            }
        }
    }

    fn print_attribute(&mut self, node: &Node, source: &str, indent: usize) {
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

    // ── Paren lists (arguments, parameters) ────────────────────────────

    fn print_paren_list(&mut self, node: &Node, source: &str, indent: usize) {
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();

        // Check if the list was originally multiline
        let is_multiline = node.start_position().row != node.end_position().row;

        if is_multiline {
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
                "(" | ")" => {}
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
                "(" | ")" => {}
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

    fn print_typed_param(&mut self, node: &Node, source: &str) {
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

    fn print_type(&mut self, node: &Node, source: &str) {
        // Type nodes contain identifiers, possibly with generics
        // Emit the full text verbatim since type syntax shouldn't be reformatted
        self.emit(node, source);
    }

    // ── Primitive helpers ──────────────────────────────────────────────

    /// Emit the full source text of a node.
    fn emit(&mut self, node: &Node, source: &str) {
        if let Ok(text) = node.utf8_text(source.as_bytes()) {
            self.output.push_str(text);
        }
    }

    fn push_str(&mut self, s: &str) {
        self.output.push_str(s);
    }

    fn write_indent(&mut self, level: usize) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::parser;

    fn format_source(source: &str) -> String {
        let tree = parser::parse(source).unwrap();
        let mut printer = Printer::new(true, 4);
        printer.format(&tree.root_node(), source);
        printer.finish()
    }

    #[test]
    fn test_basic_function() {
        let input = "func hello() -> void:\n\tpass\n";
        let output = format_source(input);
        assert_eq!(output, "func hello() -> void:\n\tpass\n");
    }

    #[test]
    fn test_string_preserved() {
        let input = "func f():\n\tprint(\"hello world\")\n";
        let output = format_source(input);
        assert!(output.contains("\"hello world\""), "got: {output}");
    }

    #[test]
    fn test_variable_with_annotation() {
        let input = "@export var health: int = 100\n";
        let output = format_source(input);
        assert_eq!(output, "@export var health: int = 100\n");
    }

    #[test]
    fn test_dictionary() {
        let input = "func f():\n\tvar d = {\"key\": \"value\", \"k2\": \"v2\"}\n";
        let output = format_source(input);
        assert!(output.contains("{\"key\": \"value\", \"k2\": \"v2\"}"), "got: {output}");
    }

    #[test]
    fn test_binary_operator_spacing() {
        let input = "func f():\n\tvar x = 1 + 2 * 3\n";
        let output = format_source(input);
        assert!(output.contains("1 + 2 * 3"), "got: {output}");
    }

    #[test]
    fn test_trailing_whitespace_removed() {
        let input = "func hello() -> void:\n\tpass\n";
        let output = format_source(input);
        assert!(!output.lines().any(|line| line.ends_with(' ')));
    }

    #[test]
    fn test_single_trailing_newline() {
        let input = "func hello() -> void:\n\tpass\n\n\n";
        let output = format_source(input);
        assert!(output.ends_with('\n'));
        assert!(!output.ends_with("\n\n"));
    }

    #[test]
    fn test_two_blank_lines_between_functions() {
        let input = "func a():\n\tpass\nfunc b():\n\tpass\n";
        let output = format_source(input);
        assert!(output.contains("pass\n\n\nfunc b"), "got: {output}");
    }

    #[test]
    fn test_if_elif_else() {
        let input = "func f():\n\tif x > 0:\n\t\tpass\n\telif x < 0:\n\t\tpass\n\telse:\n\t\tpass\n";
        let output = format_source(input);
        assert!(output.contains("if x > 0:"), "got: {output}");
        assert!(output.contains("\telif x < 0:"), "got: {output}");
        assert!(output.contains("\telse:"), "got: {output}");
    }

    #[test]
    fn test_enum() {
        let input = "enum State { IDLE, RUNNING, JUMPING }\n";
        let output = format_source(input);
        assert_eq!(output, "enum State { IDLE, RUNNING, JUMPING }\n");
    }

    #[test]
    fn test_for_loop() {
        let input = "func f():\n\tfor i in range(10):\n\t\tprint(i)\n";
        let output = format_source(input);
        assert!(output.contains("for i in range(10):"), "got: {output}");
    }

    #[test]
    fn test_get_node_preserved() {
        let input = "@onready var sprite: Sprite2D = $Sprite2D\n";
        let output = format_source(input);
        assert!(output.contains("$Sprite2D"), "got: {output}");
    }

    // ── Edge case tests ───────────────────────────────────────────────

    #[test]
    fn test_annotation_on_same_line_as_var() {
        let input = "@export var health: int = 100\n";
        let output = format_source(input);
        assert_eq!(output, "@export var health: int = 100\n");
    }

    #[test]
    fn test_onready_annotation_on_same_line() {
        let input = "@onready var sprite: Sprite2D = $Sprite2D\n";
        let output = format_source(input);
        assert_eq!(output, "@onready var sprite: Sprite2D = $Sprite2D\n");
    }

    #[test]
    fn test_multi_annotation_var() {
        let input = "@export @onready var sprite: Sprite2D = $Sprite2D\n";
        let output = format_source(input);
        assert!(
            output.contains("@export\n@onready var sprite"),
            "got: {output}"
        );
    }

    #[test]
    fn test_annotation_on_function() {
        let input = "@rpc(\"any_peer\") func sync():\n\tpass\n";
        let output = format_source(input);
        assert!(
            output.contains("@rpc(\"any_peer\") func sync()"),
            "got: {output}"
        );
    }

    #[test]
    fn test_tool_annotation_no_blank_line() {
        let input = "@tool\nextends Node2D\n";
        let output = format_source(input);
        assert_eq!(output, "@tool\nextends Node2D\n");
    }

    #[test]
    fn test_await_expression() {
        let input = "func f():\n\tawait get_tree().create_timer(1.0).timeout\n";
        let output = format_source(input);
        assert!(
            output.contains("await get_tree().create_timer(1.0).timeout"),
            "got: {output}"
        );
    }

    #[test]
    fn test_as_cast() {
        let input = "func f():\n\tvar node = get_node(\"path\") as Node2D\n";
        let output = format_source(input);
        assert!(
            output.contains("get_node(\"path\") as Node2D"),
            "got: {output}"
        );
    }

    #[test]
    fn test_is_type_check() {
        let input = "func f():\n\tif enemy is Boss:\n\t\tpass\n";
        let output = format_source(input);
        assert!(output.contains("if enemy is Boss:"), "got: {output}");
    }

    #[test]
    fn test_not_keyword() {
        let input = "func f():\n\tif not ready:\n\t\tpass\n";
        let output = format_source(input);
        assert!(output.contains("if not ready:"), "got: {output}");
    }

    #[test]
    fn test_preload_call() {
        let input = "func f():\n\tvar s = preload(\"res://scene.tscn\")\n";
        let output = format_source(input);
        assert!(
            output.contains("preload(\"res://scene.tscn\")"),
            "got: {output}"
        );
    }

    #[test]
    fn test_typed_array() {
        let input = "var arr: Array[int] = []\n";
        let output = format_source(input);
        assert!(output.contains("arr: Array[int]"), "got: {output}");
    }

    #[test]
    fn test_inferred_type() {
        let input = "func f():\n\tvar x := 42\n";
        let output = format_source(input);
        assert!(output.contains("var x := 42"), "got: {output}");
    }

    #[test]
    fn test_idempotency_basic() {
        let input = "extends Node2D\n\nvar health: int = 100\n\n\nfunc _ready():\n\tpass\n";
        let first = format_source(input);
        let second = format_source(&first);
        assert_eq!(first, second, "Format is not idempotent!\nFirst:\n{first}\nSecond:\n{second}");
    }

    #[test]
    fn test_idempotency_annotations() {
        let input = "@export var health: int = 100\n@onready var sprite: Sprite2D = $Sprite2D\n";
        let first = format_source(input);
        let second = format_source(&first);
        assert_eq!(first, second, "Format is not idempotent!\nFirst:\n{first}\nSecond:\n{second}");
    }

    #[test]
    fn test_idempotency_tool() {
        let input = "@tool\nextends Node2D\n\nvar x: int = 0\n";
        let first = format_source(input);
        let second = format_source(&first);
        assert_eq!(first, second, "Format is not idempotent!\nFirst:\n{first}\nSecond:\n{second}");
    }

    #[test]
    fn test_annotation_var_grouping() {
        let input = "@export var health: int = 100\n@export var mana: int = 50\n@onready var sprite = $Sprite2D\n\nvar speed = 200\n";
        let output = format_source(input);
        // No blank lines between annotated vars
        assert!(!output.contains("100\n\n@export"), "got: {output}");
        // No blank line between regular vars and previous group
        assert!(!output.contains("$Sprite2D\n\n\nvar speed"), "got: {output}");
        // One blank line between different groups
        assert!(output.contains("$Sprite2D\n\nvar speed"), "got: {output}");
    }

    #[test]
    fn test_class_body_formatting() {
        let input = "class_name Player\n\nextends Node2D\n\nsignal died\n\nvar health: int = 100\nvar mana: int = 50\n\n\nfunc _ready() -> void:\n\tpass\n\n\nfunc _process(delta: float) -> void:\n\tpass\n";
        let output = format_source(input);
        // One blank line between different declaration groups
        assert!(output.contains("signal died\n\nvar health"), "got: {output}");
        // No blank line between consecutive vars
        assert!(output.contains("health: int = 100\nvar mana"), "got: {output}");
        // Two blank lines before first function
        assert!(output.contains("mana: int = 50\n\n\nfunc _ready"), "got: {output}");
        // Two blank lines between functions
        assert!(output.contains("pass\n\n\nfunc _process"), "got: {output}");
    }

    #[test]
    fn test_trailing_comma_preserved() {
        let input = "func f():\n\tvar items = [\n\t\t\"a\",\n\t\t\"b\",\n\t]\n";
        let output = format_source(input);
        // Trailing comma should be preserved (though spacing may be normalized)
        assert!(output.contains("\"b\","), "Trailing comma should be preserved, got: {output}");
    }
}
