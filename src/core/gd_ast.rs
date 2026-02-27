//! Typed AST layer for GDScript source files.
//!
//! Converts tree-sitter's untyped `Node` tree into structured Rust types,
//! absorbing all grammar quirks (especially the `attribute` node) in one
//! conversion function.  Downstream consumers pattern-match on clean enums
//! instead of string-matching node kinds and navigating cursors.

use tree_sitter::{Node, Tree};

// ═══════════════════════════════════════════════════════════════════════
//  Types
// ═══════════════════════════════════════════════════════════════════════

/// A complete GDScript file.
#[derive(Debug)]
pub struct GdFile<'a> {
    pub node: Node<'a>,
    pub class_name: Option<&'a str>,
    pub extends: Option<GdExtends<'a>>,
    pub is_tool: bool,
    pub declarations: Vec<GdDecl<'a>>,
}

/// How a file or inner class extends a base.
#[derive(Debug, Clone, Copy)]
pub enum GdExtends<'a> {
    /// `extends Node` — a class name.
    Class(&'a str),
    /// `extends "res://path.gd"` — a file path.
    Path(&'a str),
}

/// A top-level declaration.
#[derive(Debug)]
pub enum GdDecl<'a> {
    Func(GdFunc<'a>),
    Var(GdVar<'a>),
    Signal(GdSignal<'a>),
    Enum(GdEnum<'a>),
    Class(GdClass<'a>),
}

/// A function or method.
#[derive(Debug)]
pub struct GdFunc<'a> {
    pub node: Node<'a>,
    pub name: &'a str,
    pub params: Vec<GdParam<'a>>,
    pub return_type: Option<GdTypeRef<'a>>,
    pub body: Vec<GdStmt<'a>>,
    pub is_static: bool,
    pub is_constructor: bool,
    pub annotations: Vec<GdAnnotation<'a>>,
}

/// A function parameter.
#[derive(Debug)]
pub struct GdParam<'a> {
    pub node: Node<'a>,
    pub name: &'a str,
    pub type_ann: Option<GdTypeRef<'a>>,
    pub default: Option<GdExpr<'a>>,
}

/// A variable or constant declaration.
#[derive(Debug)]
pub struct GdVar<'a> {
    pub node: Node<'a>,
    pub name: &'a str,
    pub type_ann: Option<GdTypeRef<'a>>,
    pub value: Option<GdExpr<'a>>,
    pub is_const: bool,
    pub is_static: bool,
    pub annotations: Vec<GdAnnotation<'a>>,
    pub setter: Option<&'a str>,
    pub getter: Option<&'a str>,
}

/// A type reference (annotation or return type).
#[derive(Debug)]
pub struct GdTypeRef<'a> {
    pub node: Node<'a>,
    pub name: &'a str,
    pub is_inferred: bool,
}

/// An `@annotation` with optional arguments.
#[derive(Debug)]
pub struct GdAnnotation<'a> {
    pub node: Node<'a>,
    pub name: &'a str,
    pub args: Vec<GdExpr<'a>>,
}

/// A `signal` declaration.
#[derive(Debug)]
pub struct GdSignal<'a> {
    pub node: Node<'a>,
    pub name: &'a str,
    pub params: Vec<GdParam<'a>>,
}

/// An `enum` declaration.
#[derive(Debug)]
pub struct GdEnum<'a> {
    pub node: Node<'a>,
    pub name: &'a str,
    pub members: Vec<GdEnumMember<'a>>,
}

/// A single enum member (name + optional value).
#[derive(Debug)]
pub struct GdEnumMember<'a> {
    pub node: Node<'a>,
    pub name: &'a str,
    pub value: Option<GdExpr<'a>>,
}

/// An inner `class` definition.
#[derive(Debug)]
pub struct GdClass<'a> {
    pub node: Node<'a>,
    pub name: &'a str,
    pub extends: Option<GdExtends<'a>>,
    pub declarations: Vec<GdDecl<'a>>,
}

/// An `if` statement with optional `elif`/`else` chains.
#[derive(Debug)]
pub struct GdIf<'a> {
    pub node: Node<'a>,
    pub condition: GdExpr<'a>,
    pub body: Vec<GdStmt<'a>>,
    pub elif_branches: Vec<(GdExpr<'a>, Vec<GdStmt<'a>>)>,
    pub else_body: Option<Vec<GdStmt<'a>>>,
}

/// A single `match` arm (patterns + optional guard + body).
#[derive(Debug)]
pub struct GdMatchArm<'a> {
    pub node: Node<'a>,
    pub patterns: Vec<GdExpr<'a>>,
    pub guard: Option<GdExpr<'a>>,
    pub body: Vec<GdStmt<'a>>,
}

// ── Expressions ────────────────────────────────────────────────────────

/// A GDScript expression.
#[derive(Debug)]
pub enum GdExpr<'a> {
    // Literals
    IntLiteral { node: Node<'a>, value: &'a str },
    FloatLiteral { node: Node<'a>, value: &'a str },
    StringLiteral { node: Node<'a>, value: &'a str },
    StringName { node: Node<'a>, value: &'a str },
    Bool { node: Node<'a>, value: bool },
    Null { node: Node<'a> },

    // Identifiers
    Ident { node: Node<'a>, name: &'a str },

    // Collections
    Array { node: Node<'a>, elements: Vec<GdExpr<'a>> },
    Dict { node: Node<'a>, pairs: Vec<(GdExpr<'a>, GdExpr<'a>)> },

    // Calls
    Call { node: Node<'a>, callee: Box<GdExpr<'a>>, args: Vec<GdExpr<'a>> },
    MethodCall { node: Node<'a>, receiver: Box<GdExpr<'a>>, method: &'a str, args: Vec<GdExpr<'a>> },
    SuperCall { node: Node<'a>, method: Option<&'a str>, args: Vec<GdExpr<'a>> },

    // Access
    PropertyAccess { node: Node<'a>, receiver: Box<GdExpr<'a>>, property: &'a str },
    Subscript { node: Node<'a>, receiver: Box<GdExpr<'a>>, index: Box<GdExpr<'a>> },
    GetNode { node: Node<'a>, path: &'a str },

    // Operators
    BinOp { node: Node<'a>, left: Box<GdExpr<'a>>, op: &'a str, right: Box<GdExpr<'a>> },
    UnaryOp { node: Node<'a>, op: &'a str, operand: Box<GdExpr<'a>> },
    Cast { node: Node<'a>, expr: Box<GdExpr<'a>>, target_type: &'a str },
    Is { node: Node<'a>, expr: Box<GdExpr<'a>>, type_name: &'a str },
    Ternary {
        node: Node<'a>,
        true_val: Box<GdExpr<'a>>,
        condition: Box<GdExpr<'a>>,
        false_val: Box<GdExpr<'a>>,
    },

    // Misc
    Await { node: Node<'a>, expr: Box<GdExpr<'a>> },
    Lambda { node: Node<'a>, func: Box<GdFunc<'a>> },
    Preload { node: Node<'a>, path: &'a str },

    // Error recovery
    Invalid { node: Node<'a> },
}

// ── Statements ─────────────────────────────────────────────────────────

/// A GDScript statement.
#[derive(Debug)]
pub enum GdStmt<'a> {
    Expr { node: Node<'a>, expr: GdExpr<'a> },
    Var(GdVar<'a>),
    Assign { node: Node<'a>, target: GdExpr<'a>, value: GdExpr<'a> },
    AugAssign { node: Node<'a>, target: GdExpr<'a>, op: &'a str, value: GdExpr<'a> },
    Return { node: Node<'a>, value: Option<GdExpr<'a>> },
    If(GdIf<'a>),
    For {
        node: Node<'a>,
        var: &'a str,
        var_type: Option<GdTypeRef<'a>>,
        iter: GdExpr<'a>,
        body: Vec<GdStmt<'a>>,
    },
    While { node: Node<'a>, condition: GdExpr<'a>, body: Vec<GdStmt<'a>> },
    Match { node: Node<'a>, value: GdExpr<'a>, arms: Vec<GdMatchArm<'a>> },
    Pass { node: Node<'a> },
    Break { node: Node<'a> },
    Continue { node: Node<'a> },
    Breakpoint { node: Node<'a> },
    Invalid { node: Node<'a> },
}

// ═══════════════════════════════════════════════════════════════════════
//  Convenience methods
// ═══════════════════════════════════════════════════════════════════════

impl<'a> GdExpr<'a> {
    /// The backing tree-sitter node for span / escape-hatch access.
    #[must_use]
    pub fn node(&self) -> Node<'a> {
        match self {
            GdExpr::IntLiteral { node, .. }
            | GdExpr::FloatLiteral { node, .. }
            | GdExpr::StringLiteral { node, .. }
            | GdExpr::StringName { node, .. }
            | GdExpr::Bool { node, .. }
            | GdExpr::Null { node }
            | GdExpr::Ident { node, .. }
            | GdExpr::Array { node, .. }
            | GdExpr::Dict { node, .. }
            | GdExpr::Call { node, .. }
            | GdExpr::MethodCall { node, .. }
            | GdExpr::SuperCall { node, .. }
            | GdExpr::PropertyAccess { node, .. }
            | GdExpr::Subscript { node, .. }
            | GdExpr::GetNode { node, .. }
            | GdExpr::BinOp { node, .. }
            | GdExpr::UnaryOp { node, .. }
            | GdExpr::Cast { node, .. }
            | GdExpr::Is { node, .. }
            | GdExpr::Ternary { node, .. }
            | GdExpr::Await { node, .. }
            | GdExpr::Lambda { node, .. }
            | GdExpr::Preload { node, .. }
            | GdExpr::Invalid { node } => *node,
        }
    }

    /// 0-based line number.
    #[must_use]
    pub fn line(&self) -> usize {
        self.node().start_position().row
    }

    /// 0-based column number.
    #[must_use]
    pub fn column(&self) -> usize {
        self.node().start_position().column
    }
}

impl<'a> GdStmt<'a> {
    /// The backing tree-sitter node for span / escape-hatch access.
    #[must_use]
    pub fn node(&self) -> Node<'a> {
        match self {
            GdStmt::Expr { node, .. }
            | GdStmt::Assign { node, .. }
            | GdStmt::AugAssign { node, .. }
            | GdStmt::Return { node, .. }
            | GdStmt::For { node, .. }
            | GdStmt::While { node, .. }
            | GdStmt::Match { node, .. }
            | GdStmt::Pass { node }
            | GdStmt::Break { node }
            | GdStmt::Continue { node }
            | GdStmt::Breakpoint { node }
            | GdStmt::Invalid { node } => *node,
            GdStmt::Var(v) => v.node,
            GdStmt::If(i) => i.node,
        }
    }

    /// 0-based line number.
    #[must_use]
    pub fn line(&self) -> usize {
        self.node().start_position().row
    }

    /// 0-based column number.
    #[must_use]
    pub fn column(&self) -> usize {
        self.node().start_position().column
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  Visitors — pre-order traversal helpers
// ═══════════════════════════════════════════════════════════════════════

/// Visit every expression in the file (pre-order).
pub fn visit_exprs<'a>(file: &GdFile<'a>, f: &mut impl FnMut(&GdExpr<'a>)) {
    for decl in &file.declarations {
        visit_decl_exprs(decl, f);
    }
}

/// Visit every statement in the file (pre-order).
pub fn visit_stmts<'a>(file: &GdFile<'a>, f: &mut impl FnMut(&GdStmt<'a>)) {
    for decl in &file.declarations {
        visit_decl_stmts(decl, f);
    }
}

/// Visit every declaration in the file (pre-order).
pub fn visit_decls<'a>(file: &GdFile<'a>, f: &mut impl FnMut(&GdDecl<'a>)) {
    for decl in &file.declarations {
        visit_decl(decl, f);
    }
}

// ── Declaration visitors ────────────────────────────────────────────

fn visit_decl<'a>(decl: &GdDecl<'a>, f: &mut impl FnMut(&GdDecl<'a>)) {
    f(decl);
    if let GdDecl::Class(cls) = decl {
        for inner in &cls.declarations {
            visit_decl(inner, f);
        }
    }
}

fn visit_decl_stmts<'a>(decl: &GdDecl<'a>, f: &mut impl FnMut(&GdStmt<'a>)) {
    match decl {
        GdDecl::Func(func) => {
            for stmt in &func.body {
                visit_stmt(stmt, f);
            }
        }
        GdDecl::Class(cls) => {
            for inner in &cls.declarations {
                visit_decl_stmts(inner, f);
            }
        }
        GdDecl::Var(_) | GdDecl::Signal(_) | GdDecl::Enum(_) => {}
    }
}

fn visit_decl_exprs<'a>(decl: &GdDecl<'a>, f: &mut impl FnMut(&GdExpr<'a>)) {
    match decl {
        GdDecl::Func(func) => {
            for param in &func.params {
                if let Some(default) = &param.default {
                    visit_expr(default, f);
                }
            }
            for stmt in &func.body {
                visit_stmt_exprs(stmt, f);
            }
        }
        GdDecl::Var(var) => {
            if let Some(value) = &var.value {
                visit_expr(value, f);
            }
        }
        GdDecl::Class(cls) => {
            for inner in &cls.declarations {
                visit_decl_exprs(inner, f);
            }
        }
        GdDecl::Enum(e) => {
            for member in &e.members {
                if let Some(value) = &member.value {
                    visit_expr(value, f);
                }
            }
        }
        GdDecl::Signal(_) => {}
    }
}

// ── Statement visitors ──────────────────────────────────────────────

fn visit_stmt<'a>(stmt: &GdStmt<'a>, f: &mut impl FnMut(&GdStmt<'a>)) {
    f(stmt);
    match stmt {
        GdStmt::If(if_stmt) => {
            for s in &if_stmt.body {
                visit_stmt(s, f);
            }
            for (_, branch) in &if_stmt.elif_branches {
                for s in branch {
                    visit_stmt(s, f);
                }
            }
            if let Some(else_body) = &if_stmt.else_body {
                for s in else_body {
                    visit_stmt(s, f);
                }
            }
        }
        GdStmt::For { body, .. } | GdStmt::While { body, .. } => {
            for s in body {
                visit_stmt(s, f);
            }
        }
        GdStmt::Match { arms, .. } => {
            for arm in arms {
                for s in &arm.body {
                    visit_stmt(s, f);
                }
            }
        }
        GdStmt::Expr { .. }
        | GdStmt::Var(_)
        | GdStmt::Assign { .. }
        | GdStmt::AugAssign { .. }
        | GdStmt::Return { .. }
        | GdStmt::Pass { .. }
        | GdStmt::Break { .. }
        | GdStmt::Continue { .. }
        | GdStmt::Breakpoint { .. }
        | GdStmt::Invalid { .. } => {}
    }
}

fn visit_stmt_exprs<'a>(stmt: &GdStmt<'a>, f: &mut impl FnMut(&GdExpr<'a>)) {
    match stmt {
        GdStmt::Expr { expr, .. } => visit_expr(expr, f),
        GdStmt::Var(var) => {
            if let Some(value) = &var.value {
                visit_expr(value, f);
            }
        }
        GdStmt::Assign { target, value, .. } | GdStmt::AugAssign { target, value, .. } => {
            visit_expr(target, f);
            visit_expr(value, f);
        }
        GdStmt::Return { value, .. } => {
            if let Some(v) = value {
                visit_expr(v, f);
            }
        }
        GdStmt::If(if_stmt) => {
            visit_expr(&if_stmt.condition, f);
            for s in &if_stmt.body {
                visit_stmt_exprs(s, f);
            }
            for (cond, branch) in &if_stmt.elif_branches {
                visit_expr(cond, f);
                for s in branch {
                    visit_stmt_exprs(s, f);
                }
            }
            if let Some(else_body) = &if_stmt.else_body {
                for s in else_body {
                    visit_stmt_exprs(s, f);
                }
            }
        }
        GdStmt::For { iter, body, .. } => {
            visit_expr(iter, f);
            for s in body {
                visit_stmt_exprs(s, f);
            }
        }
        GdStmt::While { condition, body, .. } => {
            visit_expr(condition, f);
            for s in body {
                visit_stmt_exprs(s, f);
            }
        }
        GdStmt::Match { value, arms, .. } => {
            visit_expr(value, f);
            for arm in arms {
                for pat in &arm.patterns {
                    visit_expr(pat, f);
                }
                if let Some(guard) = &arm.guard {
                    visit_expr(guard, f);
                }
                for s in &arm.body {
                    visit_stmt_exprs(s, f);
                }
            }
        }
        GdStmt::Pass { .. }
        | GdStmt::Break { .. }
        | GdStmt::Continue { .. }
        | GdStmt::Breakpoint { .. }
        | GdStmt::Invalid { .. } => {}
    }
}

// ── Expression visitor ──────────────────────────────────────────────

fn visit_expr<'a>(expr: &GdExpr<'a>, f: &mut impl FnMut(&GdExpr<'a>)) {
    f(expr);
    match expr {
        GdExpr::BinOp { left, right, .. } => {
            visit_expr(left, f);
            visit_expr(right, f);
        }
        GdExpr::UnaryOp { operand, .. } => visit_expr(operand, f),
        GdExpr::Call { callee, args, .. } => {
            visit_expr(callee, f);
            for arg in args {
                visit_expr(arg, f);
            }
        }
        GdExpr::MethodCall { receiver, args, .. } => {
            visit_expr(receiver, f);
            for arg in args {
                visit_expr(arg, f);
            }
        }
        GdExpr::SuperCall { args, .. } => {
            for arg in args {
                visit_expr(arg, f);
            }
        }
        GdExpr::PropertyAccess { receiver, .. } => visit_expr(receiver, f),
        GdExpr::Subscript { receiver, index, .. } => {
            visit_expr(receiver, f);
            visit_expr(index, f);
        }
        GdExpr::Cast { expr: inner, .. } | GdExpr::Is { expr: inner, .. } => {
            visit_expr(inner, f);
        }
        GdExpr::Ternary { true_val, condition, false_val, .. } => {
            visit_expr(true_val, f);
            visit_expr(condition, f);
            visit_expr(false_val, f);
        }
        GdExpr::Await { expr: inner, .. } => visit_expr(inner, f),
        GdExpr::Array { elements, .. } => {
            for e in elements {
                visit_expr(e, f);
            }
        }
        GdExpr::Dict { pairs, .. } => {
            for (k, v) in pairs {
                visit_expr(k, f);
                visit_expr(v, f);
            }
        }
        GdExpr::Lambda { func, .. } => {
            for param in &func.params {
                if let Some(default) = &param.default {
                    visit_expr(default, f);
                }
            }
        }
        GdExpr::IntLiteral { .. }
        | GdExpr::FloatLiteral { .. }
        | GdExpr::StringLiteral { .. }
        | GdExpr::StringName { .. }
        | GdExpr::Bool { .. }
        | GdExpr::Null { .. }
        | GdExpr::Ident { .. }
        | GdExpr::GetNode { .. }
        | GdExpr::Preload { .. }
        | GdExpr::Invalid { .. } => {}
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  Conversion — public entry point
// ═══════════════════════════════════════════════════════════════════════

/// Convert a parsed tree-sitter tree into a typed [`GdFile`].
///
/// Both `tree` and `source` must outlive the returned value (the AST
/// borrows from both).
#[must_use]
pub fn convert<'a>(tree: &'a Tree, source: &'a str) -> GdFile<'a> {
    let root = tree.root_node();
    let bytes = source.as_bytes();
    let mut file = GdFile {
        node: root,
        class_name: None,
        extends: None,
        is_tool: false,
        declarations: Vec::new(),
    };

    let mut cursor = root.walk();
    let children: Vec<Node> = root.children(&mut cursor).collect();

    // Pending annotations accumulate before the next declaration.
    let mut pending_annotations: Vec<GdAnnotation<'a>> = Vec::new();

    for child in &children {
        if child.is_error() || child.is_missing() {
            continue;
        }
        match child.kind() {
            "class_name_statement" => {
                file.class_name = child
                    .child_by_field_name("name")
                    .and_then(|n| n.utf8_text(bytes).ok());
            }
            "extends_statement" => {
                file.extends = convert_extends(child, source);
            }
            "annotation" => {
                let ann = convert_annotation(*child, source);
                if ann.name == "tool" {
                    file.is_tool = true;
                } else {
                    pending_annotations.push(ann);
                }
            }
            "annotations" => {
                let mut ac = child.walk();
                for a in child.named_children(&mut ac) {
                    if a.kind() == "annotation" {
                        let ann = convert_annotation(a, source);
                        if ann.name == "tool" {
                            file.is_tool = true;
                        } else {
                            pending_annotations.push(ann);
                        }
                    }
                }
            }
            _ => {
                if let Some(decl) = convert_decl(*child, source, &mut pending_annotations) {
                    file.declarations.push(decl);
                }
                // If convert_decl returned None, any accumulated annotations
                // are stale — clear them to avoid leaking onto the next decl.
                if !pending_annotations.is_empty() {
                    pending_annotations.clear();
                }
            }
        }
    }

    file
}

// ═══════════════════════════════════════════════════════════════════════
//  Conversion — declarations
// ═══════════════════════════════════════════════════════════════════════

fn convert_decl<'a>(
    node: Node<'a>,
    source: &'a str,
    pending: &mut Vec<GdAnnotation<'a>>,
) -> Option<GdDecl<'a>> {
    match node.kind() {
        "function_definition" => {
            let mut func = convert_func(node, source, false);
            func.annotations.splice(0..0, pending.drain(..));
            Some(GdDecl::Func(func))
        }
        "constructor_definition" => {
            let mut func = convert_func(node, source, true);
            func.annotations.splice(0..0, pending.drain(..));
            Some(GdDecl::Func(func))
        }
        "variable_statement" | "export_variable_statement" | "onready_variable_statement" => {
            let mut var = convert_var(node, source, false);
            var.annotations.splice(0..0, pending.drain(..));
            Some(GdDecl::Var(var))
        }
        "const_statement" => {
            let var = convert_var(node, source, true);
            // Consts don't get preceding annotations (no @export const).
            pending.clear();
            Some(GdDecl::Var(var))
        }
        "signal_statement" => {
            pending.clear();
            Some(GdDecl::Signal(convert_signal(node, source)))
        }
        "enum_definition" => {
            pending.clear();
            Some(GdDecl::Enum(convert_enum(node, source)))
        }
        "class_definition" => {
            pending.clear();
            Some(GdDecl::Class(convert_class(node, source)))
        }
        "decorated_definition" => {
            // Collect annotations from the decorator wrapper, then convert
            // the inner definition.
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                if child.kind() == "annotation" {
                    pending.push(convert_annotation(child, source));
                } else if child.kind() == "annotations" {
                    let mut ac = child.walk();
                    for a in child.named_children(&mut ac) {
                        if a.kind() == "annotation" {
                            pending.push(convert_annotation(a, source));
                        }
                    }
                } else {
                    return convert_decl(child, source, pending);
                }
            }
            None
        }
        _ => None,
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  Conversion — functions
// ═══════════════════════════════════════════════════════════════════════

fn convert_func<'a>(node: Node<'a>, source: &'a str, is_constructor: bool) -> GdFunc<'a> {
    let bytes = source.as_bytes();
    let name = if is_constructor {
        "_init"
    } else {
        node.child_by_field_name("name")
            .and_then(|n| n.utf8_text(bytes).ok())
            .unwrap_or("")
    };

    let params = node
        .child_by_field_name("parameters")
        .map(|p| convert_params(p, source))
        .unwrap_or_default();

    let return_type = convert_type_ref_field(node, "return_type", source);

    let body = node
        .child_by_field_name("body")
        .map(|b| convert_body(b, source))
        .unwrap_or_default();

    let is_static = has_child_kind(node, "static_keyword");
    let annotations = collect_inline_annotations(node, source);

    GdFunc {
        node,
        name,
        params,
        return_type,
        body,
        is_static,
        is_constructor,
        annotations,
    }
}

fn convert_params<'a>(params_node: Node<'a>, source: &'a str) -> Vec<GdParam<'a>> {
    let bytes = source.as_bytes();
    let mut params = Vec::new();
    let mut cursor = params_node.walk();
    for child in params_node.named_children(&mut cursor) {
        match child.kind() {
            "identifier" => {
                if let Ok(name) = child.utf8_text(bytes) {
                    params.push(GdParam {
                        node: child,
                        name,
                        type_ann: None,
                        default: None,
                    });
                }
            }
            "typed_parameter" => {
                params.push(convert_typed_param(child, source));
            }
            "default_parameter" => {
                params.push(convert_default_param(child, source, false));
            }
            "typed_default_parameter" => {
                params.push(convert_default_param(child, source, true));
            }
            _ => {}
        }
    }
    params
}

fn convert_typed_param<'a>(node: Node<'a>, source: &'a str) -> GdParam<'a> {
    let name = first_identifier(node, source).unwrap_or("");
    let type_ann = convert_type_ref_field(node, "type", source);
    GdParam { node, name, type_ann, default: None }
}

fn convert_default_param<'a>(node: Node<'a>, source: &'a str, typed: bool) -> GdParam<'a> {
    let name = first_identifier(node, source).unwrap_or("");
    let type_ann = if typed {
        convert_type_ref_field(node, "type", source)
    } else {
        None
    };
    let default = node
        .child_by_field_name("value")
        .map(|v| convert_expr(v, source));
    GdParam { node, name, type_ann, default }
}

// ═══════════════════════════════════════════════════════════════════════
//  Conversion — variables
// ═══════════════════════════════════════════════════════════════════════

fn convert_var<'a>(node: Node<'a>, source: &'a str, is_const: bool) -> GdVar<'a> {
    let bytes = source.as_bytes();
    let name = node
        .child_by_field_name("name")
        .and_then(|n| n.utf8_text(bytes).ok())
        .unwrap_or("");

    let type_ann = convert_type_ref_field(node, "type", source);
    let value = node
        .child_by_field_name("value")
        .map(|v| convert_expr(v, source));
    let is_static = has_child_kind(node, "static_keyword");
    let annotations = collect_inline_annotations(node, source);

    // Setter / getter (shorthand form only: `var x: set = fn_set, get = fn_get`).
    let setget = node.child_by_field_name("setget");
    let setter = setget
        .and_then(|sg| sg.child_by_field_name("set"))
        .filter(|n| n.kind() == "setter")
        .and_then(|n| n.utf8_text(bytes).ok());
    let getter = setget
        .and_then(|sg| sg.child_by_field_name("get"))
        .filter(|n| n.kind() == "getter")
        .and_then(|n| n.utf8_text(bytes).ok());

    GdVar {
        node,
        name,
        type_ann,
        value,
        is_const,
        is_static,
        annotations,
        setter,
        getter,
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  Conversion — signals, enums, classes
// ═══════════════════════════════════════════════════════════════════════

fn convert_signal<'a>(node: Node<'a>, source: &'a str) -> GdSignal<'a> {
    let bytes = source.as_bytes();
    let name = node
        .child_by_field_name("name")
        .and_then(|n| n.utf8_text(bytes).ok())
        .unwrap_or("");
    let params = node
        .child_by_field_name("parameters")
        .map(|p| convert_params(p, source))
        .unwrap_or_default();
    GdSignal { node, name, params }
}

fn convert_enum<'a>(node: Node<'a>, source: &'a str) -> GdEnum<'a> {
    let bytes = source.as_bytes();
    let name = node
        .child_by_field_name("name")
        .and_then(|n| n.utf8_text(bytes).ok())
        .unwrap_or("");
    let mut members = Vec::new();
    if let Some(body) = node.child_by_field_name("body") {
        let mut cursor = body.walk();
        for child in body.named_children(&mut cursor) {
            if child.kind() == "enumerator" {
                let member_name = child
                    .child_by_field_name("left")
                    .and_then(|n| n.utf8_text(bytes).ok())
                    .unwrap_or("");
                let value = child
                    .child_by_field_name("right")
                    .map(|v| convert_expr(v, source));
                members.push(GdEnumMember { node: child, name: member_name, value });
            }
        }
    }
    GdEnum { node, name, members }
}

fn convert_class<'a>(node: Node<'a>, source: &'a str) -> GdClass<'a> {
    let bytes = source.as_bytes();
    let name = node
        .child_by_field_name("name")
        .and_then(|n| n.utf8_text(bytes).ok())
        .unwrap_or("");

    let extends = node
        .child_by_field_name("extends")
        .and_then(|ext| convert_extends(&ext, source));

    let mut declarations = Vec::new();
    if let Some(body) = node.child_by_field_name("body") {
        let mut pending: Vec<GdAnnotation<'a>> = Vec::new();
        let mut cursor = body.walk();
        for child in body.named_children(&mut cursor) {
            match child.kind() {
                "annotation" => pending.push(convert_annotation(child, source)),
                "annotations" => {
                    let mut ac = child.walk();
                    for a in child.named_children(&mut ac) {
                        if a.kind() == "annotation" {
                            pending.push(convert_annotation(a, source));
                        }
                    }
                }
                _ => {
                    if let Some(decl) = convert_decl(child, source, &mut pending) {
                        declarations.push(decl);
                    }
                    if !pending.is_empty() {
                        pending.clear();
                    }
                }
            }
        }
    }

    GdClass { node, name, extends, declarations }
}

// ═══════════════════════════════════════════════════════════════════════
//  Conversion — statements
// ═══════════════════════════════════════════════════════════════════════

fn convert_body<'a>(body_node: Node<'a>, source: &'a str) -> Vec<GdStmt<'a>> {
    let mut stmts = Vec::new();
    let mut cursor = body_node.walk();
    for child in body_node.named_children(&mut cursor) {
        if child.kind() == "comment" {
            continue;
        }
        stmts.push(convert_stmt(child, source));
    }
    stmts
}

fn convert_stmt<'a>(node: Node<'a>, source: &'a str) -> GdStmt<'a> {
    if node.is_error() || node.is_missing() {
        return GdStmt::Invalid { node };
    }
    match node.kind() {
        "expression_statement" => convert_expression_statement(node, source),
        "return_statement" => {
            let value = node.named_child(0).map(|n| convert_expr(n, source));
            GdStmt::Return { node, value }
        }
        "if_statement" => GdStmt::If(convert_if(node, source)),
        "for_statement" => convert_for(node, source),
        "while_statement" => {
            let condition = node
                .child_by_field_name("condition")
                .map(|n| convert_expr(n, source))
                .unwrap_or(GdExpr::Invalid { node });
            let body = node
                .child_by_field_name("body")
                .map(|b| convert_body(b, source))
                .unwrap_or_default();
            GdStmt::While { node, condition, body }
        }
        "match_statement" => convert_match(node, source),
        "variable_statement" | "export_variable_statement" | "onready_variable_statement" => {
            GdStmt::Var(convert_var(node, source, false))
        }
        "const_statement" => GdStmt::Var(convert_var(node, source, true)),
        "pass_statement" => GdStmt::Pass { node },
        "break_statement" => GdStmt::Break { node },
        "continue_statement" => GdStmt::Continue { node },
        "breakpoint_statement" => GdStmt::Breakpoint { node },
        _ => GdStmt::Invalid { node },
    }
}

fn convert_expression_statement<'a>(node: Node<'a>, source: &'a str) -> GdStmt<'a> {
    let Some(inner) = node.named_child(0) else {
        return GdStmt::Invalid { node };
    };
    match inner.kind() {
        "assignment" => {
            let target = inner
                .child_by_field_name("left")
                .map(|n| convert_expr(n, source))
                .unwrap_or(GdExpr::Invalid { node: inner });
            let value = inner
                .child_by_field_name("right")
                .map(|n| convert_expr(n, source))
                .unwrap_or(GdExpr::Invalid { node: inner });
            GdStmt::Assign { node, target, value }
        }
        "augmented_assignment" => {
            let target = inner
                .child_by_field_name("left")
                .map(|n| convert_expr(n, source))
                .unwrap_or(GdExpr::Invalid { node: inner });
            let op = inner
                .child_by_field_name("op")
                .map_or("", |n| txt(n, source));
            let value = inner
                .child_by_field_name("right")
                .map(|n| convert_expr(n, source))
                .unwrap_or(GdExpr::Invalid { node: inner });
            GdStmt::AugAssign { node, target, op, value }
        }
        _ => {
            let expr = convert_expr(inner, source);
            GdStmt::Expr { node, expr }
        }
    }
}

fn convert_if<'a>(node: Node<'a>, source: &'a str) -> GdIf<'a> {
    let condition = node
        .child_by_field_name("condition")
        .map(|n| convert_expr(n, source))
        .unwrap_or(GdExpr::Invalid { node });
    let body = node
        .child_by_field_name("body")
        .map(|b| convert_body(b, source))
        .unwrap_or_default();

    let mut elif_branches = Vec::new();
    let mut else_body = None;

    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        match child.kind() {
            "elif_clause" => {
                let cond = child
                    .child_by_field_name("condition")
                    .map(|n| convert_expr(n, source))
                    .unwrap_or(GdExpr::Invalid { node: child });
                let stmts = child
                    .child_by_field_name("body")
                    .map(|b| convert_body(b, source))
                    .unwrap_or_default();
                elif_branches.push((cond, stmts));
            }
            "else_clause" => {
                else_body = child
                    .child_by_field_name("body")
                    .map(|b| convert_body(b, source));
            }
            _ => {}
        }
    }

    GdIf { node, condition, body, elif_branches, else_body }
}

fn convert_for<'a>(node: Node<'a>, source: &'a str) -> GdStmt<'a> {
    let bytes = source.as_bytes();
    let var = node
        .child_by_field_name("left")
        .and_then(|n| n.utf8_text(bytes).ok())
        .unwrap_or("");
    let var_type = convert_type_ref_field(node, "type", source);
    let iter = node
        .child_by_field_name("right")
        .map(|n| convert_expr(n, source))
        .unwrap_or(GdExpr::Invalid { node });
    let body = node
        .child_by_field_name("body")
        .map(|b| convert_body(b, source))
        .unwrap_or_default();
    GdStmt::For { node, var, var_type, iter, body }
}

fn convert_match<'a>(node: Node<'a>, source: &'a str) -> GdStmt<'a> {
    let value = node
        .child_by_field_name("value")
        .map(|n| convert_expr(n, source))
        .unwrap_or(GdExpr::Invalid { node });

    let mut arms = Vec::new();
    if let Some(body) = node.child_by_field_name("body") {
        let mut cursor = body.walk();
        for child in body.named_children(&mut cursor) {
            if child.kind() == "pattern_section" {
                arms.push(convert_match_arm(child, source));
            }
        }
    }

    GdStmt::Match { node, value, arms }
}

fn convert_match_arm<'a>(node: Node<'a>, source: &'a str) -> GdMatchArm<'a> {
    let mut patterns = Vec::new();
    let mut guard = None;

    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        match child.kind() {
            "pattern_guard" => {
                guard = child.named_child(0).map(|n| convert_expr(n, source));
            }
            "body" => {} // handled below
            _ => {
                // Everything else is a pattern expression.
                patterns.push(convert_expr(child, source));
            }
        }
    }

    let body = node
        .child_by_field_name("body")
        .map(|b| convert_body(b, source))
        .unwrap_or_default();

    GdMatchArm { node, patterns, guard, body }
}

// ═══════════════════════════════════════════════════════════════════════
//  Conversion — expressions
// ═══════════════════════════════════════════════════════════════════════

fn convert_expr<'a>(node: Node<'a>, source: &'a str) -> GdExpr<'a> {
    if node.is_error() || node.is_missing() {
        return GdExpr::Invalid { node };
    }
    match node.kind() {
        // ── Literals ──────────────────────────────────────────────
        "integer" => GdExpr::IntLiteral { node, value: txt(node, source) },
        "float" => GdExpr::FloatLiteral { node, value: txt(node, source) },
        "string" | "node_path" => GdExpr::StringLiteral { node, value: txt(node, source) },
        "string_name" => GdExpr::StringName { node, value: txt(node, source) },
        "true" => GdExpr::Bool { node, value: true },
        "false" => GdExpr::Bool { node, value: false },
        "null" => GdExpr::Null { node },
        "identifier" | "self" => GdExpr::Ident { node, name: txt(node, source) },

        // ── Collections ───────────────────────────────────────────
        "array" => convert_array(node, source),
        "dictionary" => convert_dict(node, source),

        // ── Calls ─────────────────────────────────────────────────
        "call" => convert_call(node, source),
        "base_call" => convert_base_call(node, source),

        // ── Attribute (method call / property access) ─────────────
        "attribute" => convert_attribute(node, source),

        // ── Subscript ─────────────────────────────────────────────
        "subscript" => convert_subscript(node, source),

        // ── Get-node ──────────────────────────────────────────────
        "get_node" => GdExpr::GetNode { node, path: txt(node, source) },

        // ── Binary operators (including as/is) ────────────────────
        "binary_operator" => convert_binary(node, source),
        "as_pattern" | "cast" => convert_cast_node(node, source),

        // ── Unary ─────────────────────────────────────────────────
        "unary_operator" => convert_unary(node, source),

        // ── Ternary ───────────────────────────────────────────────
        "conditional_expression" | "ternary_expression" => convert_ternary(node, source),

        // ── Await ─────────────────────────────────────────────────
        "await_expression" => {
            let expr = node
                .named_child(0)
                .map(|n| convert_expr(n, source))
                .unwrap_or(GdExpr::Invalid { node });
            GdExpr::Await { node, expr: Box::new(expr) }
        }

        // ── Lambda ────────────────────────────────────────────────
        "lambda" => {
            let func = convert_func(node, source, false);
            GdExpr::Lambda { node, func: Box::new(func) }
        }

        // ── Parenthesized — unwrap ────────────────────────────────
        "parenthesized_expression" => {
            node.named_child(0)
                .map(|n| convert_expr(n, source))
                .unwrap_or(GdExpr::Invalid { node })
        }

        // ── Fallback ─────────────────────────────────────────────
        _ => GdExpr::Invalid { node },
    }
}

fn convert_array<'a>(node: Node<'a>, source: &'a str) -> GdExpr<'a> {
    let mut elements = Vec::new();
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if !matches!(child.kind(), "pattern_open_ending" | "pattern_binding") {
            elements.push(convert_expr(child, source));
        }
    }
    GdExpr::Array { node, elements }
}

fn convert_dict<'a>(node: Node<'a>, source: &'a str) -> GdExpr<'a> {
    let mut pairs = Vec::new();
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.kind() == "pair" {
            let key = child
                .child_by_field_name("left")
                .map(|n| convert_expr(n, source))
                .unwrap_or(GdExpr::Invalid { node: child });
            let value = child
                .child_by_field_name("value")
                .map(|n| convert_expr(n, source))
                .unwrap_or(GdExpr::Invalid { node: child });
            pairs.push((key, value));
        }
    }
    GdExpr::Dict { node, pairs }
}

fn convert_call<'a>(node: Node<'a>, source: &'a str) -> GdExpr<'a> {
    let Some(callee_node) = node.named_child(0) else {
        return GdExpr::Invalid { node };
    };
    let args = node
        .child_by_field_name("arguments")
        .map(|a| convert_args(a, source))
        .unwrap_or_default();

    // Detect `preload("path")`.
    if callee_node.kind() == "identifier"
        && txt(callee_node, source) == "preload"
        && let [GdExpr::StringLiteral { value, .. }] = args.as_slice()
    {
        return GdExpr::Preload { node, path: value };
    }

    let callee = convert_expr(callee_node, source);
    GdExpr::Call { node, callee: Box::new(callee), args }
}

fn convert_base_call<'a>(node: Node<'a>, source: &'a str) -> GdExpr<'a> {
    let method = node
        .named_child(0)
        .map(|n| txt(n, source));
    let args = node
        .child_by_field_name("arguments")
        .map(|a| convert_args(a, source))
        .unwrap_or_default();
    GdExpr::SuperCall { node, method, args }
}

fn convert_attribute<'a>(node: Node<'a>, source: &'a str) -> GdExpr<'a> {
    let count = node.named_child_count();
    if count < 2 {
        return GdExpr::Invalid { node };
    }

    // tree-sitter flattens attribute chains: `a.b.c()` →
    //   attribute [identifier(a), identifier(b), attribute_call(c, args)]
    // We must chain children 0..end into nested PropertyAccess nodes.
    let build_chain = |end: usize| -> GdExpr<'a> {
        let mut result = convert_expr(node.named_child(0).unwrap(), source);
        for i in 1..end {
            if let Some(child) = node.named_child(i) {
                result = GdExpr::PropertyAccess {
                    node,
                    receiver: Box::new(result),
                    property: txt(child, source),
                };
            }
        }
        result
    };

    // Last named child determines the access type.
    let Some(last) = node.named_child(count - 1) else {
        return GdExpr::Invalid { node };
    };

    match last.kind() {
        "attribute_call" => {
            // Method call: `receiver.method(args)`
            let method = last
                .named_child(0)
                .map_or("", |n| txt(n, source));
            let args = last
                .child_by_field_name("arguments")
                .map(|a| convert_args(a, source))
                .unwrap_or_default();
            let receiver = build_chain(count - 1);
            GdExpr::MethodCall { node, receiver: Box::new(receiver), method, args }
        }
        "attribute_subscript" => {
            // Property subscript: `receiver.prop[index]`
            // Intermediate identifiers are siblings, property name is inside attribute_subscript.
            let receiver = build_chain(count - 1);
            let property = last
                .named_child(0)
                .map_or("", |n| txt(n, source));
            let prop = GdExpr::PropertyAccess {
                node,
                receiver: Box::new(receiver),
                property,
            };
            let index = last
                .child_by_field_name("arguments")
                .and_then(|a| a.named_child(0))
                .map(|n| convert_expr(n, source))
                .unwrap_or(GdExpr::Invalid { node });
            GdExpr::Subscript { node, receiver: Box::new(prop), index: Box::new(index) }
        }
        _ => {
            // Property access: `receiver.property`
            let receiver = build_chain(count - 1);
            let property = txt(last, source);
            GdExpr::PropertyAccess { node, receiver: Box::new(receiver), property }
        }
    }
}

fn convert_subscript<'a>(node: Node<'a>, source: &'a str) -> GdExpr<'a> {
    let receiver = node
        .named_child(0)
        .map(|n| convert_expr(n, source))
        .unwrap_or(GdExpr::Invalid { node });
    let index = node
        .child_by_field_name("arguments")
        .and_then(|a| a.named_child(0))
        .map(|n| convert_expr(n, source))
        .unwrap_or(GdExpr::Invalid { node });
    GdExpr::Subscript { node, receiver: Box::new(receiver), index: Box::new(index) }
}

fn convert_binary<'a>(node: Node<'a>, source: &'a str) -> GdExpr<'a> {
    let op = node
        .child_by_field_name("op")
        .map_or("", |n| txt(n, source));

    match op {
        "as" => {
            let expr = node
                .child_by_field_name("left")
                .map(|n| convert_expr(n, source))
                .unwrap_or(GdExpr::Invalid { node });
            let target_type = node
                .child_by_field_name("right")
                .map_or("", |n| txt(n, source));
            GdExpr::Cast { node, expr: Box::new(expr), target_type }
        }
        "is" => {
            let expr = node
                .child_by_field_name("left")
                .map(|n| convert_expr(n, source))
                .unwrap_or(GdExpr::Invalid { node });
            let type_name = node
                .child_by_field_name("right")
                .map_or("", |n| txt(n, source));
            GdExpr::Is { node, expr: Box::new(expr), type_name }
        }
        _ => {
            let left = node
                .child_by_field_name("left")
                .map(|n| convert_expr(n, source))
                .unwrap_or(GdExpr::Invalid { node });
            let right = node
                .child_by_field_name("right")
                .map(|n| convert_expr(n, source))
                .unwrap_or(GdExpr::Invalid { node });
            GdExpr::BinOp { node, left: Box::new(left), op, right: Box::new(right) }
        }
    }
}

fn convert_cast_node<'a>(node: Node<'a>, source: &'a str) -> GdExpr<'a> {
    let expr = node
        .named_child(0)
        .map(|n| convert_expr(n, source))
        .unwrap_or(GdExpr::Invalid { node });
    let target_type = node
        .named_child(1)
        .map_or("", |n| txt(n, source));
    GdExpr::Cast { node, expr: Box::new(expr), target_type }
}

fn convert_unary<'a>(node: Node<'a>, source: &'a str) -> GdExpr<'a> {
    // The operator is the first (unnamed) child token.
    let op = node.child(0).map_or("", |n| txt(n, source));
    let operand = node
        .named_child(0)
        .map(|n| convert_expr(n, source))
        .unwrap_or(GdExpr::Invalid { node });
    GdExpr::UnaryOp { node, op, operand: Box::new(operand) }
}

fn convert_ternary<'a>(node: Node<'a>, source: &'a str) -> GdExpr<'a> {
    let true_val = node
        .child_by_field_name("left")
        .map(|n| convert_expr(n, source))
        .unwrap_or(GdExpr::Invalid { node });
    let condition = node
        .child_by_field_name("condition")
        .map(|n| convert_expr(n, source))
        .unwrap_or(GdExpr::Invalid { node });
    let false_val = node
        .child_by_field_name("right")
        .map(|n| convert_expr(n, source))
        .unwrap_or(GdExpr::Invalid { node });
    GdExpr::Ternary {
        node,
        true_val: Box::new(true_val),
        condition: Box::new(condition),
        false_val: Box::new(false_val),
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  Conversion — helpers
// ═══════════════════════════════════════════════════════════════════════

/// Extract text from a node.
fn txt<'a>(node: Node<'a>, source: &'a str) -> &'a str {
    node.utf8_text(source.as_bytes()).unwrap_or("")
}

/// Convert an `extends_statement` node to [`GdExtends`].
fn convert_extends<'a>(node: &Node<'a>, source: &'a str) -> Option<GdExtends<'a>> {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        match child.kind() {
            "type" | "identifier" => {
                return child
                    .utf8_text(source.as_bytes())
                    .ok()
                    .map(GdExtends::Class);
            }
            "string" => {
                let raw = child.utf8_text(source.as_bytes()).ok()?;
                let path = raw.trim_matches('"').trim_matches('\'');
                return Some(GdExtends::Path(path));
            }
            _ => {}
        }
    }
    None
}

/// Convert an `annotation` node.
fn convert_annotation<'a>(node: Node<'a>, source: &'a str) -> GdAnnotation<'a> {
    let name = first_identifier(node, source).unwrap_or("");
    let args = node
        .child_by_field_name("arguments")
        .map(|a| convert_args(a, source))
        .unwrap_or_default();
    GdAnnotation { node, name, args }
}

/// Convert an `arguments` node into a list of expressions.
fn convert_args<'a>(args_node: Node<'a>, source: &'a str) -> Vec<GdExpr<'a>> {
    let mut result = Vec::new();
    let mut cursor = args_node.walk();
    for child in args_node.named_children(&mut cursor) {
        result.push(convert_expr(child, source));
    }
    result
}

/// Collect inline `annotations` from a declaration node.
fn collect_inline_annotations<'a>(node: Node<'a>, source: &'a str) -> Vec<GdAnnotation<'a>> {
    let mut annotations = Vec::new();
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.kind() == "annotations" {
            let mut ac = child.walk();
            for a in child.named_children(&mut ac) {
                if a.kind() == "annotation" {
                    annotations.push(convert_annotation(a, source));
                }
            }
        }
    }
    annotations
}

/// Get the text of the first `identifier` child.
fn first_identifier<'a>(node: Node<'a>, source: &'a str) -> Option<&'a str> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "identifier" {
            return child.utf8_text(source.as_bytes()).ok();
        }
    }
    None
}

/// Check whether `node` has a child of the given kind.
fn has_child_kind(node: Node, kind: &str) -> bool {
    let mut cursor = node.walk();
    node.children(&mut cursor).any(|c| c.kind() == kind)
}

/// Extract a type reference from a field (e.g. `"type"` or `"return_type"`).
fn convert_type_ref_field<'a>(
    node: Node<'a>,
    field: &str,
    source: &'a str,
) -> Option<GdTypeRef<'a>> {
    let type_node = node.child_by_field_name(field)?;
    if type_node.kind() == "inferred_type" {
        return Some(GdTypeRef { node: type_node, name: "", is_inferred: true });
    }
    let name = type_node.utf8_text(source.as_bytes()).ok()?;
    Some(GdTypeRef { node: type_node, name, is_inferred: false })
}

// ═══════════════════════════════════════════════════════════════════════
//  Tests
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::parser;

    // ── File structure ────────────────────────────────────────────

    #[test]
    fn extends_and_class_name() {
        let src = "class_name Player\nextends CharacterBody2D\n";
        let tree = parser::parse(src).unwrap();
        let file = convert(&tree, src);
        assert_eq!(file.class_name, Some("Player"));
        assert!(matches!(file.extends, Some(GdExtends::Class("CharacterBody2D"))));
    }

    #[test]
    fn extends_path() {
        let src = "extends \"res://base.gd\"\n";
        let tree = parser::parse(src).unwrap();
        let file = convert(&tree, src);
        assert!(matches!(file.extends, Some(GdExtends::Path("res://base.gd"))));
    }

    #[test]
    fn tool_annotation() {
        let src = "@tool\nextends Node\n";
        let tree = parser::parse(src).unwrap();
        let file = convert(&tree, src);
        assert!(file.is_tool);
    }

    // ── Functions ─────────────────────────────────────────────────

    #[test]
    fn function_with_typed_params_and_return() {
        let src = "func add(a: int, b: int) -> int:\n\treturn a + b\n";
        let tree = parser::parse(src).unwrap();
        let file = convert(&tree, src);
        assert_eq!(file.declarations.len(), 1);
        let GdDecl::Func(func) = &file.declarations[0] else { panic!("expected Func") };
        assert_eq!(func.name, "add");
        assert_eq!(func.params.len(), 2);
        assert_eq!(func.params[0].name, "a");
        assert_eq!(func.params[0].type_ann.as_ref().unwrap().name, "int");
        assert_eq!(func.params[1].name, "b");
        assert!(func.return_type.is_some());
        assert_eq!(func.return_type.as_ref().unwrap().name, "int");
        assert!(!func.body.is_empty());
    }

    #[test]
    fn constructor() {
        let src = "func _init():\n\tpass\n";
        let tree = parser::parse(src).unwrap();
        let file = convert(&tree, src);
        let GdDecl::Func(func) = &file.declarations[0] else { panic!("expected Func") };
        assert_eq!(func.name, "_init");
    }

    #[test]
    fn static_function() {
        let src = "static func helper() -> void:\n\tpass\n";
        let tree = parser::parse(src).unwrap();
        let file = convert(&tree, src);
        let GdDecl::Func(func) = &file.declarations[0] else { panic!("expected Func") };
        assert!(func.is_static);
    }

    // ── Variables ─────────────────────────────────────────────────

    #[test]
    fn typed_variable() {
        let src = "var speed: float = 10.0\n";
        let tree = parser::parse(src).unwrap();
        let file = convert(&tree, src);
        let GdDecl::Var(var) = &file.declarations[0] else { panic!("expected Var") };
        assert_eq!(var.name, "speed");
        assert!(!var.is_const);
        assert_eq!(var.type_ann.as_ref().unwrap().name, "float");
        assert!(var.value.is_some());
    }

    #[test]
    fn inferred_variable() {
        let src = "var x := 42\n";
        let tree = parser::parse(src).unwrap();
        let file = convert(&tree, src);
        let GdDecl::Var(var) = &file.declarations[0] else { panic!("expected Var") };
        assert_eq!(var.name, "x");
        assert!(var.type_ann.as_ref().unwrap().is_inferred);
    }

    #[test]
    fn const_variable() {
        let src = "const MAX = 100\n";
        let tree = parser::parse(src).unwrap();
        let file = convert(&tree, src);
        let GdDecl::Var(var) = &file.declarations[0] else { panic!("expected Var") };
        assert_eq!(var.name, "MAX");
        assert!(var.is_const);
    }

    #[test]
    fn export_variable() {
        let src = "@export var health: int = 100\n";
        let tree = parser::parse(src).unwrap();
        let file = convert(&tree, src);
        let GdDecl::Var(var) = &file.declarations[0] else { panic!("expected Var") };
        assert_eq!(var.name, "health");
        assert!(var.annotations.iter().any(|a| a.name == "export"));
    }

    // ── Expressions ───────────────────────────────────────────────

    #[test]
    fn literals() {
        let src = "func f():\n\tvar a = 42\n\tvar b = 3.14\n\tvar c = \"hello\"\n\tvar d = true\n\tvar e = null\n";
        let tree = parser::parse(src).unwrap();
        let file = convert(&tree, src);
        let GdDecl::Func(func) = &file.declarations[0] else { panic!() };
        assert!(func.body.len() >= 5);
    }

    #[test]
    fn method_call() {
        let src = "func f():\n\tget_tree().quit()\n";
        let tree = parser::parse(src).unwrap();
        let file = convert(&tree, src);
        let GdDecl::Func(func) = &file.declarations[0] else { panic!() };
        let GdStmt::Expr { expr, .. } = &func.body[0] else { panic!() };
        assert!(matches!(expr, GdExpr::MethodCall { method: "quit", .. }));
    }

    #[test]
    fn chained_method_call() {
        let src = "func f():\n\ta.b.c()\n";
        let tree = parser::parse(src).unwrap();
        let file = convert(&tree, src);
        let GdDecl::Func(func) = &file.declarations[0] else { panic!() };
        let GdStmt::Expr { expr, .. } = &func.body[0] else { panic!() };
        let GdExpr::MethodCall { receiver, method, .. } = expr else { panic!("expected MethodCall, got {expr:?}") };
        assert_eq!(*method, "c");
        assert!(matches!(receiver.as_ref(), GdExpr::PropertyAccess { property: "b", .. }));
    }

    #[test]
    fn property_access() {
        let src = "func f():\n\tvar x = obj.name\n";
        let tree = parser::parse(src).unwrap();
        let file = convert(&tree, src);
        let GdDecl::Func(func) = &file.declarations[0] else { panic!() };
        let GdStmt::Var(var) = &func.body[0] else { panic!() };
        let Some(GdExpr::PropertyAccess { property, .. }) = &var.value else { panic!() };
        assert_eq!(*property, "name");
    }

    #[test]
    fn function_call() {
        let src = "func f():\n\tprint(42)\n";
        let tree = parser::parse(src).unwrap();
        let file = convert(&tree, src);
        let GdDecl::Func(func) = &file.declarations[0] else { panic!() };
        let GdStmt::Expr { expr, .. } = &func.body[0] else { panic!() };
        let GdExpr::Call { callee, args, .. } = expr else { panic!() };
        assert!(matches!(callee.as_ref(), GdExpr::Ident { name: "print", .. }));
        assert_eq!(args.len(), 1);
    }

    #[test]
    fn preload_detection() {
        let src = "func f():\n\tvar s = preload(\"res://scene.tscn\")\n";
        let tree = parser::parse(src).unwrap();
        let file = convert(&tree, src);
        let GdDecl::Func(func) = &file.declarations[0] else { panic!() };
        let GdStmt::Var(var) = &func.body[0] else { panic!() };
        let Some(GdExpr::Preload { path, .. }) = &var.value else {
            panic!("expected Preload, got {:?}", var.value);
        };
        assert_eq!(*path, "\"res://scene.tscn\"");
    }

    #[test]
    fn subscript_access() {
        let src = "func f():\n\tvar x = arr[0]\n";
        let tree = parser::parse(src).unwrap();
        let file = convert(&tree, src);
        let GdDecl::Func(func) = &file.declarations[0] else { panic!() };
        let GdStmt::Var(var) = &func.body[0] else { panic!() };
        assert!(matches!(&var.value, Some(GdExpr::Subscript { .. })));
    }

    #[test]
    fn binary_operators() {
        let src = "func f():\n\tvar x = 1 + 2\n";
        let tree = parser::parse(src).unwrap();
        let file = convert(&tree, src);
        let GdDecl::Func(func) = &file.declarations[0] else { panic!() };
        let GdStmt::Var(var) = &func.body[0] else { panic!() };
        let Some(GdExpr::BinOp { op, .. }) = &var.value else { panic!() };
        assert_eq!(*op, "+");
    }

    #[test]
    fn cast_expression() {
        let src = "func f():\n\tvar x = node as Node2D\n";
        let tree = parser::parse(src).unwrap();
        let file = convert(&tree, src);
        let GdDecl::Func(func) = &file.declarations[0] else { panic!() };
        let GdStmt::Var(var) = &func.body[0] else { panic!() };
        assert!(matches!(&var.value, Some(GdExpr::Cast { target_type: "Node2D", .. })));
    }

    #[test]
    fn is_expression() {
        let src = "func f():\n\tvar x = node is Node2D\n";
        let tree = parser::parse(src).unwrap();
        let file = convert(&tree, src);
        let GdDecl::Func(func) = &file.declarations[0] else { panic!() };
        let GdStmt::Var(var) = &func.body[0] else { panic!() };
        assert!(matches!(&var.value, Some(GdExpr::Is { type_name: "Node2D", .. })));
    }

    #[test]
    fn ternary_expression() {
        let src = "func f():\n\tvar x = a if cond else b\n";
        let tree = parser::parse(src).unwrap();
        let file = convert(&tree, src);
        let GdDecl::Func(func) = &file.declarations[0] else { panic!() };
        let GdStmt::Var(var) = &func.body[0] else { panic!() };
        assert!(matches!(&var.value, Some(GdExpr::Ternary { .. })));
    }

    #[test]
    fn get_node_path() {
        let src = "func f():\n\tvar x = $Sprite2D\n";
        let tree = parser::parse(src).unwrap();
        let file = convert(&tree, src);
        let GdDecl::Func(func) = &file.declarations[0] else { panic!() };
        let GdStmt::Var(var) = &func.body[0] else { panic!() };
        assert!(matches!(&var.value, Some(GdExpr::GetNode { .. })));
    }

    #[test]
    fn lambda_expression() {
        let src = "func f():\n\tvar cb = func(): return 1\n";
        let tree = parser::parse(src).unwrap();
        let file = convert(&tree, src);
        let GdDecl::Func(func) = &file.declarations[0] else { panic!() };
        let GdStmt::Var(var) = &func.body[0] else { panic!() };
        assert!(matches!(&var.value, Some(GdExpr::Lambda { .. })));
    }

    #[test]
    fn unary_operator() {
        let src = "func f():\n\tvar x = -1\n";
        let tree = parser::parse(src).unwrap();
        let file = convert(&tree, src);
        let GdDecl::Func(func) = &file.declarations[0] else { panic!() };
        let GdStmt::Var(var) = &func.body[0] else { panic!() };
        assert!(matches!(&var.value, Some(GdExpr::UnaryOp { op: "-", .. })));
    }

    // ── Statements ────────────────────────────────────────────────

    #[test]
    fn if_elif_else() {
        let src = "func f():\n\tif a:\n\t\tpass\n\telif b:\n\t\tpass\n\telse:\n\t\tpass\n";
        let tree = parser::parse(src).unwrap();
        let file = convert(&tree, src);
        let GdDecl::Func(func) = &file.declarations[0] else { panic!() };
        let GdStmt::If(if_stmt) = &func.body[0] else { panic!("expected If") };
        assert_eq!(if_stmt.elif_branches.len(), 1);
        assert!(if_stmt.else_body.is_some());
    }

    #[test]
    fn for_loop() {
        let src = "func f():\n\tfor i in range(10):\n\t\tpass\n";
        let tree = parser::parse(src).unwrap();
        let file = convert(&tree, src);
        let GdDecl::Func(func) = &file.declarations[0] else { panic!() };
        let GdStmt::For { var, .. } = &func.body[0] else { panic!("expected For") };
        assert_eq!(*var, "i");
    }

    #[test]
    fn while_loop() {
        let src = "func f():\n\twhile true:\n\t\tbreak\n";
        let tree = parser::parse(src).unwrap();
        let file = convert(&tree, src);
        let GdDecl::Func(func) = &file.declarations[0] else { panic!() };
        assert!(matches!(&func.body[0], GdStmt::While { .. }));
    }

    #[test]
    fn match_statement() {
        let src = "func f():\n\tmatch x:\n\t\t1:\n\t\t\tpass\n\t\t_:\n\t\t\tpass\n";
        let tree = parser::parse(src).unwrap();
        let file = convert(&tree, src);
        let GdDecl::Func(func) = &file.declarations[0] else { panic!() };
        let GdStmt::Match { arms, .. } = &func.body[0] else { panic!("expected Match") };
        assert_eq!(arms.len(), 2);
    }

    #[test]
    fn match_with_guard() {
        let src = "func f():\n\tmatch x:\n\t\tvar v when v > 0:\n\t\t\tpass\n";
        let tree = parser::parse(src).unwrap();
        let file = convert(&tree, src);
        let GdDecl::Func(func) = &file.declarations[0] else { panic!() };
        let GdStmt::Match { arms, .. } = &func.body[0] else { panic!("expected Match") };
        assert!(arms[0].guard.is_some());
    }

    #[test]
    fn return_value() {
        let src = "func f():\n\treturn 42\n";
        let tree = parser::parse(src).unwrap();
        let file = convert(&tree, src);
        let GdDecl::Func(func) = &file.declarations[0] else { panic!() };
        let GdStmt::Return { value, .. } = &func.body[0] else { panic!("expected Return") };
        assert!(value.is_some());
    }

    #[test]
    fn assign_and_aug_assign() {
        let src = "func f():\n\tx = 1\n\tx += 2\n";
        let tree = parser::parse(src).unwrap();
        let file = convert(&tree, src);
        let GdDecl::Func(func) = &file.declarations[0] else { panic!() };
        assert!(matches!(&func.body[0], GdStmt::Assign { .. }));
        let GdStmt::AugAssign { op, .. } = &func.body[1] else { panic!("expected AugAssign") };
        assert_eq!(*op, "+=");
    }

    // ── Inner classes, signals, enums ─────────────────────────────

    #[test]
    fn inner_class() {
        let src = "class Inner:\n\tvar x = 1\n";
        let tree = parser::parse(src).unwrap();
        let file = convert(&tree, src);
        let GdDecl::Class(cls) = &file.declarations[0] else { panic!("expected Class") };
        assert_eq!(cls.name, "Inner");
        assert_eq!(cls.declarations.len(), 1);
    }

    #[test]
    fn signal_declaration() {
        let src = "signal health_changed(new_hp: int)\n";
        let tree = parser::parse(src).unwrap();
        let file = convert(&tree, src);
        let GdDecl::Signal(sig) = &file.declarations[0] else { panic!("expected Signal") };
        assert_eq!(sig.name, "health_changed");
        assert_eq!(sig.params.len(), 1);
        assert_eq!(sig.params[0].name, "new_hp");
    }

    #[test]
    fn enum_declaration() {
        let src = "enum State { IDLE, RUN, JUMP = 5 }\n";
        let tree = parser::parse(src).unwrap();
        let file = convert(&tree, src);
        let GdDecl::Enum(e) = &file.declarations[0] else { panic!("expected Enum") };
        assert_eq!(e.name, "State");
        assert_eq!(e.members.len(), 3);
        assert_eq!(e.members[0].name, "IDLE");
        assert!(e.members[0].value.is_none());
        assert_eq!(e.members[2].name, "JUMP");
        assert!(e.members[2].value.is_some());
    }

    // ── Error recovery ────────────────────────────────────────────

    #[test]
    fn error_recovery_produces_invalid() {
        let src = "func f():\n\tvar = \n";
        let tree = parser::parse(src).unwrap();
        let file = convert(&tree, src);
        assert!(!file.declarations.is_empty());
    }

    // ── Roundtrip: every node has valid span ──────────────────────

    #[test]
    fn all_expr_nodes_have_valid_span() {
        let src = "\
func f():
\tvar a = 1 + 2
\tvar b = obj.method()
\tvar c = [1, 2, 3]
\tvar d = {\"k\": \"v\"}
\tif a:\n\t\tpass
\tfor i in range(10):\n\t\tpass
\treturn a
";
        let tree = parser::parse(src).unwrap();
        let file = convert(&tree, src);
        let GdDecl::Func(func) = &file.declarations[0] else { panic!() };
        for stmt in &func.body {
            let _ = stmt.node();
        }
    }

    #[test]
    fn array_and_dict_expressions() {
        let src = "func f():\n\tvar a = [1, 2, 3]\n\tvar d = {\"a\": 1, \"b\": 2}\n";
        let tree = parser::parse(src).unwrap();
        let file = convert(&tree, src);
        let GdDecl::Func(func) = &file.declarations[0] else { panic!() };
        let GdStmt::Var(arr_var) = &func.body[0] else { panic!() };
        let Some(GdExpr::Array { elements, .. }) = &arr_var.value else { panic!() };
        assert_eq!(elements.len(), 3);

        let GdStmt::Var(dict_var) = &func.body[1] else { panic!() };
        let Some(GdExpr::Dict { pairs, .. }) = &dict_var.value else { panic!() };
        assert_eq!(pairs.len(), 2);
    }

    #[test]
    fn await_expression() {
        let src = "func f():\n\tawait get_tree().process_frame\n";
        let tree = parser::parse(src).unwrap();
        let file = convert(&tree, src);
        let GdDecl::Func(func) = &file.declarations[0] else { panic!() };
        let GdStmt::Expr { expr, .. } = &func.body[0] else { panic!() };
        assert!(matches!(expr, GdExpr::Await { .. }));
    }

    #[test]
    fn string_name_expression() {
        let src = "func f():\n\tvar x = &\"action_name\"\n";
        let tree = parser::parse(src).unwrap();
        let file = convert(&tree, src);
        let GdDecl::Func(func) = &file.declarations[0] else { panic!() };
        let GdStmt::Var(var) = &func.body[0] else { panic!() };
        assert!(matches!(&var.value, Some(GdExpr::StringName { .. })));
    }

    #[test]
    fn default_param_value() {
        let src = "func f(x: int = 5):\n\tpass\n";
        let tree = parser::parse(src).unwrap();
        let file = convert(&tree, src);
        let GdDecl::Func(func) = &file.declarations[0] else { panic!() };
        assert_eq!(func.params.len(), 1);
        assert!(func.params[0].default.is_some());
        assert_eq!(func.params[0].type_ann.as_ref().unwrap().name, "int");
    }

    #[test]
    fn pass_break_continue() {
        let src = "func f():\n\tpass\n";
        let tree = parser::parse(src).unwrap();
        let file = convert(&tree, src);
        let GdDecl::Func(func) = &file.declarations[0] else { panic!() };
        assert!(matches!(&func.body[0], GdStmt::Pass { .. }));
    }
}
