//! Owned AST types for GDScript — no lifetimes, no tree-sitter `Node` references.
//!
//! Mirror of the borrowed types in [`gd_ast`] with `String` instead of `&str`
//! and `Option<Span>` instead of `Node<'a>`.  These types can be freely
//! constructed, transformed, and returned from rewrite rules without being
//! tied to the original source text or parse tree.
//!
//! Conversion: `OwnedFile::from_borrowed(&GdFile)` (and likewise for every
//! sub-type) preserves the original byte spans so the printer can emit
//! original source text for unmodified nodes.

use crate::gd_ast;

// ═══════════════════════════════════════════════════════════════════════
//  Span
// ═══════════════════════════════════════════════════════════════════════

/// Byte range in the original source text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    /// Extract the corresponding slice from `source`.
    #[must_use]
    pub fn text<'a>(&self, source: &'a str) -> &'a str {
        &source[self.start..self.end]
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  Owned types
// ═══════════════════════════════════════════════════════════════════════

/// Owned equivalent of [`gd_ast::GdFile`].
#[derive(Debug, Clone)]
pub struct OwnedFile {
    pub span: Option<Span>,
    pub class_name: Option<String>,
    pub extends: Option<OwnedExtends>,
    pub is_tool: bool,
    pub has_static_unload: bool,
    pub declarations: Vec<OwnedDecl>,
}

/// Owned equivalent of [`gd_ast::GdExtends`].
#[derive(Debug, Clone)]
pub enum OwnedExtends {
    Class(String),
    Path(String),
}

/// Owned equivalent of [`gd_ast::GdDecl`].
#[derive(Debug, Clone)]
pub enum OwnedDecl {
    Func(OwnedFunc),
    Var(OwnedVar),
    Signal(OwnedSignal),
    Enum(OwnedEnum),
    Class(OwnedClass),
    Stmt(OwnedStmt),
}

/// Owned equivalent of [`gd_ast::GdFunc`].
#[derive(Debug, Clone)]
pub struct OwnedFunc {
    pub span: Option<Span>,
    pub name: String,
    pub params: Vec<OwnedParam>,
    pub return_type: Option<OwnedTypeRef>,
    pub body: Vec<OwnedStmt>,
    pub is_static: bool,
    pub is_constructor: bool,
    pub annotations: Vec<OwnedAnnotation>,
    pub doc: Option<String>,
}

/// Owned equivalent of [`gd_ast::GdParam`].
#[derive(Debug, Clone)]
pub struct OwnedParam {
    pub span: Option<Span>,
    pub name: String,
    pub type_ann: Option<OwnedTypeRef>,
    pub default: Option<OwnedExpr>,
}

/// Owned equivalent of [`gd_ast::GdVar`].
#[derive(Debug, Clone)]
pub struct OwnedVar {
    pub span: Option<Span>,
    pub name: String,
    pub type_ann: Option<OwnedTypeRef>,
    pub value: Option<OwnedExpr>,
    pub is_const: bool,
    pub is_static: bool,
    pub annotations: Vec<OwnedAnnotation>,
    pub setter: Option<String>,
    pub getter: Option<String>,
    pub doc: Option<String>,
}

/// Owned equivalent of [`gd_ast::GdTypeRef`].
#[derive(Debug, Clone)]
pub struct OwnedTypeRef {
    pub span: Option<Span>,
    pub name: String,
    pub is_inferred: bool,
}

/// Owned equivalent of [`gd_ast::GdAnnotation`].
#[derive(Debug, Clone)]
pub struct OwnedAnnotation {
    pub span: Option<Span>,
    pub name: String,
    pub args: Vec<OwnedExpr>,
}

/// Owned equivalent of [`gd_ast::GdSignal`].
#[derive(Debug, Clone)]
pub struct OwnedSignal {
    pub span: Option<Span>,
    pub name: String,
    pub params: Vec<OwnedParam>,
    pub doc: Option<String>,
}

/// Owned equivalent of [`gd_ast::GdEnum`].
#[derive(Debug, Clone)]
pub struct OwnedEnum {
    pub span: Option<Span>,
    pub name: String,
    pub members: Vec<OwnedEnumMember>,
    pub doc: Option<String>,
}

/// Owned equivalent of [`gd_ast::GdEnumMember`].
#[derive(Debug, Clone)]
pub struct OwnedEnumMember {
    pub span: Option<Span>,
    pub name: String,
    pub value: Option<OwnedExpr>,
}

/// Owned equivalent of [`gd_ast::GdClass`].
#[derive(Debug, Clone)]
pub struct OwnedClass {
    pub span: Option<Span>,
    pub name: String,
    pub extends: Option<OwnedExtends>,
    pub declarations: Vec<OwnedDecl>,
    pub doc: Option<String>,
}

/// Owned equivalent of [`gd_ast::GdIf`].
#[derive(Debug, Clone)]
pub struct OwnedIf {
    pub span: Option<Span>,
    pub condition: OwnedExpr,
    pub body: Vec<OwnedStmt>,
    pub elif_branches: Vec<(OwnedExpr, Vec<OwnedStmt>)>,
    pub else_body: Option<Vec<OwnedStmt>>,
}

/// Owned equivalent of [`gd_ast::GdMatchArm`].
#[derive(Debug, Clone)]
pub struct OwnedMatchArm {
    pub span: Option<Span>,
    pub patterns: Vec<OwnedExpr>,
    pub guard: Option<OwnedExpr>,
    pub body: Vec<OwnedStmt>,
}

// ── Expressions ───────────────────────────────────────────────────────

/// Owned equivalent of [`gd_ast::GdExpr`].
#[derive(Debug, Clone)]
pub enum OwnedExpr {
    // Literals
    IntLiteral {
        span: Option<Span>,
        value: String,
    },
    FloatLiteral {
        span: Option<Span>,
        value: String,
    },
    StringLiteral {
        span: Option<Span>,
        value: String,
    },
    StringName {
        span: Option<Span>,
        value: String,
    },
    Bool {
        span: Option<Span>,
        value: bool,
    },
    Null {
        span: Option<Span>,
    },

    // Identifiers
    Ident {
        span: Option<Span>,
        name: String,
    },

    // Collections
    Array {
        span: Option<Span>,
        elements: Vec<OwnedExpr>,
    },
    Dict {
        span: Option<Span>,
        pairs: Vec<(OwnedExpr, OwnedExpr)>,
    },

    // Calls
    Call {
        span: Option<Span>,
        callee: Box<OwnedExpr>,
        args: Vec<OwnedExpr>,
    },
    MethodCall {
        span: Option<Span>,
        receiver: Box<OwnedExpr>,
        method: String,
        args: Vec<OwnedExpr>,
    },
    SuperCall {
        span: Option<Span>,
        method: Option<String>,
        args: Vec<OwnedExpr>,
    },

    // Access
    PropertyAccess {
        span: Option<Span>,
        receiver: Box<OwnedExpr>,
        property: String,
    },
    Subscript {
        span: Option<Span>,
        receiver: Box<OwnedExpr>,
        index: Box<OwnedExpr>,
    },
    GetNode {
        span: Option<Span>,
        path: String,
    },

    // Operators
    BinOp {
        span: Option<Span>,
        left: Box<OwnedExpr>,
        op: String,
        right: Box<OwnedExpr>,
    },
    UnaryOp {
        span: Option<Span>,
        op: String,
        operand: Box<OwnedExpr>,
    },
    Cast {
        span: Option<Span>,
        expr: Box<OwnedExpr>,
        target_type: String,
    },
    Is {
        span: Option<Span>,
        expr: Box<OwnedExpr>,
        type_name: String,
    },
    Ternary {
        span: Option<Span>,
        true_val: Box<OwnedExpr>,
        condition: Box<OwnedExpr>,
        false_val: Box<OwnedExpr>,
    },

    // Misc
    Await {
        span: Option<Span>,
        expr: Box<OwnedExpr>,
    },
    Lambda {
        span: Option<Span>,
        func: Box<OwnedFunc>,
    },
    Preload {
        span: Option<Span>,
        path: String,
    },

    // Error recovery
    Invalid {
        span: Option<Span>,
    },
}

// ── Statements ────────────────────────────────────────────────────────

/// Owned equivalent of [`gd_ast::GdStmt`].
#[derive(Debug, Clone)]
pub enum OwnedStmt {
    Expr {
        span: Option<Span>,
        expr: OwnedExpr,
    },
    Var(OwnedVar),
    Assign {
        span: Option<Span>,
        target: OwnedExpr,
        value: OwnedExpr,
    },
    AugAssign {
        span: Option<Span>,
        target: OwnedExpr,
        op: String,
        value: OwnedExpr,
    },
    Return {
        span: Option<Span>,
        value: Option<OwnedExpr>,
    },
    If(OwnedIf),
    For {
        span: Option<Span>,
        var: String,
        var_type: Option<OwnedTypeRef>,
        iter: OwnedExpr,
        body: Vec<OwnedStmt>,
    },
    While {
        span: Option<Span>,
        condition: OwnedExpr,
        body: Vec<OwnedStmt>,
    },
    Match {
        span: Option<Span>,
        value: OwnedExpr,
        arms: Vec<OwnedMatchArm>,
    },
    Pass {
        span: Option<Span>,
    },
    Break {
        span: Option<Span>,
    },
    Continue {
        span: Option<Span>,
    },
    Breakpoint {
        span: Option<Span>,
    },
    Invalid {
        span: Option<Span>,
    },
}

// ═══════════════════════════════════════════════════════════════════════
//  Span accessors
// ═══════════════════════════════════════════════════════════════════════

impl OwnedExpr {
    #[must_use]
    pub fn span(&self) -> Option<Span> {
        match self {
            Self::IntLiteral { span, .. }
            | Self::FloatLiteral { span, .. }
            | Self::StringLiteral { span, .. }
            | Self::StringName { span, .. }
            | Self::Bool { span, .. }
            | Self::Null { span }
            | Self::Ident { span, .. }
            | Self::Array { span, .. }
            | Self::Dict { span, .. }
            | Self::Call { span, .. }
            | Self::MethodCall { span, .. }
            | Self::SuperCall { span, .. }
            | Self::PropertyAccess { span, .. }
            | Self::Subscript { span, .. }
            | Self::GetNode { span, .. }
            | Self::BinOp { span, .. }
            | Self::UnaryOp { span, .. }
            | Self::Cast { span, .. }
            | Self::Is { span, .. }
            | Self::Ternary { span, .. }
            | Self::Await { span, .. }
            | Self::Lambda { span, .. }
            | Self::Preload { span, .. }
            | Self::Invalid { span } => *span,
        }
    }

    /// Recursively clear all source spans so the printer regenerates
    /// this expression entirely from AST fields.
    pub fn clear_spans(&mut self) {
        match self {
            Self::IntLiteral { span, .. }
            | Self::FloatLiteral { span, .. }
            | Self::StringLiteral { span, .. }
            | Self::StringName { span, .. }
            | Self::Bool { span, .. }
            | Self::Null { span }
            | Self::Ident { span, .. }
            | Self::GetNode { span, .. }
            | Self::Preload { span, .. }
            | Self::Invalid { span } => *span = None,
            Self::Array { span, elements } => {
                *span = None;
                for e in elements {
                    e.clear_spans();
                }
            }
            Self::Dict { span, pairs } => {
                *span = None;
                for (k, v) in pairs {
                    k.clear_spans();
                    v.clear_spans();
                }
            }
            Self::Call { span, callee, args } => {
                *span = None;
                callee.clear_spans();
                for a in args {
                    a.clear_spans();
                }
            }
            Self::MethodCall {
                span,
                receiver,
                args,
                ..
            } => {
                *span = None;
                receiver.clear_spans();
                for a in args {
                    a.clear_spans();
                }
            }
            Self::SuperCall { span, args, .. } => {
                *span = None;
                for a in args {
                    a.clear_spans();
                }
            }
            Self::PropertyAccess { span, receiver, .. } => {
                *span = None;
                receiver.clear_spans();
            }
            Self::Subscript {
                span,
                receiver,
                index,
            } => {
                *span = None;
                receiver.clear_spans();
                index.clear_spans();
            }
            Self::BinOp {
                span, left, right, ..
            } => {
                *span = None;
                left.clear_spans();
                right.clear_spans();
            }
            Self::UnaryOp { span, operand, .. } => {
                *span = None;
                operand.clear_spans();
            }
            Self::Cast { span, expr, .. }
            | Self::Is { span, expr, .. }
            | Self::Await { span, expr } => {
                *span = None;
                expr.clear_spans();
            }
            Self::Ternary {
                span,
                true_val,
                condition,
                false_val,
            } => {
                *span = None;
                true_val.clear_spans();
                condition.clear_spans();
                false_val.clear_spans();
            }
            Self::Lambda { span, func } => {
                *span = None;
                func.clear_spans();
            }
        }
    }
}

impl OwnedStmt {
    #[must_use]
    pub fn span(&self) -> Option<Span> {
        match self {
            Self::Expr { span, .. }
            | Self::Assign { span, .. }
            | Self::AugAssign { span, .. }
            | Self::Return { span, .. }
            | Self::For { span, .. }
            | Self::While { span, .. }
            | Self::Match { span, .. }
            | Self::Pass { span }
            | Self::Break { span }
            | Self::Continue { span }
            | Self::Breakpoint { span }
            | Self::Invalid { span } => *span,
            Self::Var(v) => v.span,
            Self::If(i) => i.span,
        }
    }

    /// Recursively clear all source spans so the printer regenerates
    /// this statement entirely from AST fields.
    pub fn clear_spans(&mut self) {
        match self {
            Self::Expr { span, expr } => {
                *span = None;
                expr.clear_spans();
            }
            Self::Var(v) => v.clear_spans(),
            Self::Assign {
                span,
                target,
                value,
            }
            | Self::AugAssign {
                span,
                target,
                value,
                ..
            } => {
                *span = None;
                target.clear_spans();
                value.clear_spans();
            }
            Self::Return { span, value } => {
                *span = None;
                if let Some(v) = value {
                    v.clear_spans();
                }
            }
            Self::If(i) => {
                i.span = None;
                i.condition.clear_spans();
                for s in &mut i.body {
                    s.clear_spans();
                }
                for (cond, body) in &mut i.elif_branches {
                    cond.clear_spans();
                    for s in body {
                        s.clear_spans();
                    }
                }
                if let Some(eb) = &mut i.else_body {
                    for s in eb {
                        s.clear_spans();
                    }
                }
            }
            Self::For {
                span, iter, body, ..
            } => {
                *span = None;
                iter.clear_spans();
                for s in body {
                    s.clear_spans();
                }
            }
            Self::While {
                span,
                condition,
                body,
            } => {
                *span = None;
                condition.clear_spans();
                for s in body {
                    s.clear_spans();
                }
            }
            Self::Match { span, value, arms } => {
                *span = None;
                value.clear_spans();
                for arm in arms {
                    arm.span = None;
                    for p in &mut arm.patterns {
                        p.clear_spans();
                    }
                    if let Some(g) = &mut arm.guard {
                        g.clear_spans();
                    }
                    for s in &mut arm.body {
                        s.clear_spans();
                    }
                }
            }
            Self::Pass { span }
            | Self::Break { span }
            | Self::Continue { span }
            | Self::Breakpoint { span }
            | Self::Invalid { span } => *span = None,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  Conversion from borrowed types
// ═══════════════════════════════════════════════════════════════════════

fn node_span(node: tree_sitter::Node) -> Span {
    Span {
        start: node.start_byte(),
        end: node.end_byte(),
    }
}

impl OwnedFile {
    #[must_use]
    pub fn from_borrowed(file: &gd_ast::GdFile) -> Self {
        Self {
            span: Some(node_span(file.node)),
            class_name: file.class_name.map(String::from),
            extends: file.extends.map(|e| OwnedExtends::from_borrowed(&e)),
            is_tool: file.is_tool,
            has_static_unload: file.has_static_unload,
            declarations: file
                .declarations
                .iter()
                .map(OwnedDecl::from_borrowed)
                .collect(),
        }
    }

    /// Recursively clear all source spans in this file.
    pub fn clear_spans(&mut self) {
        self.span = None;
        for d in &mut self.declarations {
            d.clear_spans();
        }
    }
}

impl OwnedExtends {
    #[must_use]
    pub fn from_borrowed(ext: &gd_ast::GdExtends) -> Self {
        match ext {
            gd_ast::GdExtends::Class(c) => Self::Class((*c).to_string()),
            gd_ast::GdExtends::Path(p) => Self::Path((*p).to_string()),
        }
    }
}

impl OwnedDecl {
    #[must_use]
    pub fn from_borrowed(decl: &gd_ast::GdDecl) -> Self {
        match decl {
            gd_ast::GdDecl::Func(f) => Self::Func(OwnedFunc::from_borrowed(f)),
            gd_ast::GdDecl::Var(v) => Self::Var(OwnedVar::from_borrowed(v)),
            gd_ast::GdDecl::Signal(s) => Self::Signal(OwnedSignal::from_borrowed(s)),
            gd_ast::GdDecl::Enum(e) => Self::Enum(OwnedEnum::from_borrowed(e)),
            gd_ast::GdDecl::Class(c) => Self::Class(OwnedClass::from_borrowed(c)),
            gd_ast::GdDecl::Stmt(s) => Self::Stmt(OwnedStmt::from_borrowed(s)),
        }
    }

    /// Return the source span of this declaration, if it has one.
    #[must_use]
    pub fn span(&self) -> Option<Span> {
        match self {
            Self::Func(f) => f.span,
            Self::Var(v) => v.span,
            Self::Signal(s) => s.span,
            Self::Enum(e) => e.span,
            Self::Class(c) => c.span,
            Self::Stmt(s) => s.span(),
        }
    }

    /// Recursively clear all source spans in this declaration.
    pub fn clear_spans(&mut self) {
        match self {
            Self::Func(f) => f.clear_spans(),
            Self::Var(v) => v.clear_spans(),
            Self::Signal(s) => {
                s.span = None;
                for p in &mut s.params {
                    p.span = None;
                }
            }
            Self::Enum(e) => {
                e.span = None;
                for m in &mut e.members {
                    m.span = None;
                    if let Some(v) = &mut m.value {
                        v.clear_spans();
                    }
                }
            }
            Self::Class(c) => {
                c.span = None;
                for d in &mut c.declarations {
                    d.clear_spans();
                }
            }
            Self::Stmt(s) => s.clear_spans(),
        }
    }

    /// Return the name of this declaration, or `""` for bare statements.
    #[must_use]
    pub fn name(&self) -> &str {
        match self {
            Self::Func(f) => &f.name,
            Self::Var(v) => &v.name,
            Self::Signal(s) => &s.name,
            Self::Enum(e) => &e.name,
            Self::Class(c) => &c.name,
            Self::Stmt(_) => "",
        }
    }

    /// Find the index of a declaration by name in a slice (skips bare statements).
    #[must_use]
    pub fn find_by_name(decls: &[Self], name: &str) -> Option<usize> {
        decls
            .iter()
            .position(|d| !matches!(d, Self::Stmt(_)) && d.name() == name)
    }

    /// Find the index of the declaration whose span contains `byte`.
    #[must_use]
    pub fn find_at_byte(decls: &[Self], byte: usize) -> Option<usize> {
        decls
            .iter()
            .position(|d| d.span().is_some_and(|s| s.start <= byte && byte < s.end))
    }
}

impl OwnedFunc {
    #[must_use]
    pub fn from_borrowed(f: &gd_ast::GdFunc) -> Self {
        Self {
            span: Some(node_span(f.node)),
            name: f.name.to_string(),
            params: f.params.iter().map(OwnedParam::from_borrowed).collect(),
            return_type: f.return_type.as_ref().map(OwnedTypeRef::from_borrowed),
            body: f.body.iter().map(OwnedStmt::from_borrowed).collect(),
            is_static: f.is_static,
            is_constructor: f.is_constructor,
            annotations: f
                .annotations
                .iter()
                .map(OwnedAnnotation::from_borrowed)
                .collect(),
            doc: f.doc.map(String::from),
        }
    }

    /// Recursively clear all source spans.
    pub fn clear_spans(&mut self) {
        self.span = None;
        for a in &mut self.annotations {
            a.span = None;
            for e in &mut a.args {
                e.clear_spans();
            }
        }
        for p in &mut self.params {
            p.span = None;
        }
        for s in &mut self.body {
            s.clear_spans();
        }
    }
}

impl OwnedParam {
    #[must_use]
    pub fn from_borrowed(p: &gd_ast::GdParam) -> Self {
        Self {
            span: Some(node_span(p.node)),
            name: p.name.to_string(),
            type_ann: p.type_ann.as_ref().map(OwnedTypeRef::from_borrowed),
            default: p.default.as_ref().map(OwnedExpr::from_borrowed),
        }
    }
}

impl OwnedVar {
    #[must_use]
    pub fn from_borrowed(v: &gd_ast::GdVar) -> Self {
        Self {
            span: Some(node_span(v.node)),
            name: v.name.to_string(),
            type_ann: v.type_ann.as_ref().map(OwnedTypeRef::from_borrowed),
            value: v.value.as_ref().map(OwnedExpr::from_borrowed),
            is_const: v.is_const,
            is_static: v.is_static,
            annotations: v
                .annotations
                .iter()
                .map(OwnedAnnotation::from_borrowed)
                .collect(),
            setter: v.setter.map(String::from),
            getter: v.getter.map(String::from),
            doc: v.doc.map(String::from),
        }
    }

    /// Recursively clear all source spans.
    pub fn clear_spans(&mut self) {
        self.span = None;
        for a in &mut self.annotations {
            a.span = None;
            for e in &mut a.args {
                e.clear_spans();
            }
        }
        if let Some(e) = &mut self.value {
            e.clear_spans();
        }
    }
}

impl OwnedTypeRef {
    #[must_use]
    pub fn from_borrowed(t: &gd_ast::GdTypeRef) -> Self {
        Self {
            span: Some(node_span(t.node)),
            name: t.name.to_string(),
            is_inferred: t.is_inferred,
        }
    }
}

impl OwnedAnnotation {
    #[must_use]
    pub fn from_borrowed(a: &gd_ast::GdAnnotation) -> Self {
        Self {
            span: Some(node_span(a.node)),
            name: a.name.to_string(),
            args: a.args.iter().map(OwnedExpr::from_borrowed).collect(),
        }
    }
}

impl OwnedSignal {
    #[must_use]
    pub fn from_borrowed(s: &gd_ast::GdSignal) -> Self {
        Self {
            span: Some(node_span(s.node)),
            name: s.name.to_string(),
            params: s.params.iter().map(OwnedParam::from_borrowed).collect(),
            doc: s.doc.map(String::from),
        }
    }
}

impl OwnedEnum {
    #[must_use]
    pub fn from_borrowed(e: &gd_ast::GdEnum) -> Self {
        Self {
            span: Some(node_span(e.node)),
            name: e.name.to_string(),
            members: e
                .members
                .iter()
                .map(OwnedEnumMember::from_borrowed)
                .collect(),
            doc: e.doc.map(String::from),
        }
    }
}

impl OwnedEnumMember {
    #[must_use]
    pub fn from_borrowed(m: &gd_ast::GdEnumMember) -> Self {
        Self {
            span: Some(node_span(m.node)),
            name: m.name.to_string(),
            value: m.value.as_ref().map(OwnedExpr::from_borrowed),
        }
    }
}

impl OwnedClass {
    #[must_use]
    pub fn from_borrowed(c: &gd_ast::GdClass) -> Self {
        Self {
            span: Some(node_span(c.node)),
            name: c.name.to_string(),
            extends: c.extends.map(|e| OwnedExtends::from_borrowed(&e)),
            declarations: c
                .declarations
                .iter()
                .map(OwnedDecl::from_borrowed)
                .collect(),
            doc: c.doc.map(String::from),
        }
    }
}

impl OwnedIf {
    #[must_use]
    pub fn from_borrowed(i: &gd_ast::GdIf) -> Self {
        Self {
            span: Some(node_span(i.node)),
            condition: OwnedExpr::from_borrowed(&i.condition),
            body: i.body.iter().map(OwnedStmt::from_borrowed).collect(),
            elif_branches: i
                .elif_branches
                .iter()
                .map(|(cond, body)| {
                    (
                        OwnedExpr::from_borrowed(cond),
                        body.iter().map(OwnedStmt::from_borrowed).collect(),
                    )
                })
                .collect(),
            else_body: i
                .else_body
                .as_ref()
                .map(|body| body.iter().map(OwnedStmt::from_borrowed).collect()),
        }
    }
}

impl OwnedMatchArm {
    #[must_use]
    pub fn from_borrowed(arm: &gd_ast::GdMatchArm) -> Self {
        Self {
            span: Some(node_span(arm.node)),
            patterns: arm.patterns.iter().map(OwnedExpr::from_borrowed).collect(),
            guard: arm.guard.as_ref().map(OwnedExpr::from_borrowed),
            body: arm.body.iter().map(OwnedStmt::from_borrowed).collect(),
        }
    }
}

impl OwnedExpr {
    #[must_use]
    #[allow(clippy::too_many_lines)]
    pub fn from_borrowed(expr: &gd_ast::GdExpr) -> Self {
        let span = Some(node_span(expr.node()));
        match expr {
            gd_ast::GdExpr::IntLiteral { value, .. } => Self::IntLiteral {
                span,
                value: (*value).to_string(),
            },
            gd_ast::GdExpr::FloatLiteral { value, .. } => Self::FloatLiteral {
                span,
                value: (*value).to_string(),
            },
            gd_ast::GdExpr::StringLiteral { value, .. } => Self::StringLiteral {
                span,
                value: (*value).to_string(),
            },
            gd_ast::GdExpr::StringName { value, .. } => Self::StringName {
                span,
                value: (*value).to_string(),
            },
            gd_ast::GdExpr::Bool { value, .. } => Self::Bool {
                span,
                value: *value,
            },
            gd_ast::GdExpr::Null { .. } => Self::Null { span },
            gd_ast::GdExpr::Ident { name, .. } => Self::Ident {
                span,
                name: (*name).to_string(),
            },
            gd_ast::GdExpr::Array { elements, .. } => Self::Array {
                span,
                elements: elements.iter().map(Self::from_borrowed).collect(),
            },
            gd_ast::GdExpr::Dict { pairs, .. } => Self::Dict {
                span,
                pairs: pairs
                    .iter()
                    .map(|(k, v)| (Self::from_borrowed(k), Self::from_borrowed(v)))
                    .collect(),
            },
            gd_ast::GdExpr::Call { callee, args, .. } => Self::Call {
                span,
                callee: Box::new(Self::from_borrowed(callee)),
                args: args.iter().map(Self::from_borrowed).collect(),
            },
            gd_ast::GdExpr::MethodCall {
                receiver,
                method,
                args,
                ..
            } => Self::MethodCall {
                span,
                receiver: Box::new(Self::from_borrowed(receiver)),
                method: (*method).to_string(),
                args: args.iter().map(Self::from_borrowed).collect(),
            },
            gd_ast::GdExpr::SuperCall { method, args, .. } => Self::SuperCall {
                span,
                method: method.map(String::from),
                args: args.iter().map(Self::from_borrowed).collect(),
            },
            gd_ast::GdExpr::PropertyAccess {
                receiver, property, ..
            } => Self::PropertyAccess {
                span,
                receiver: Box::new(Self::from_borrowed(receiver)),
                property: (*property).to_string(),
            },
            gd_ast::GdExpr::Subscript {
                receiver, index, ..
            } => Self::Subscript {
                span,
                receiver: Box::new(Self::from_borrowed(receiver)),
                index: Box::new(Self::from_borrowed(index)),
            },
            gd_ast::GdExpr::GetNode { path, .. } => Self::GetNode {
                span,
                path: (*path).to_string(),
            },
            gd_ast::GdExpr::BinOp {
                left, op, right, ..
            } => Self::BinOp {
                span,
                left: Box::new(Self::from_borrowed(left)),
                op: (*op).to_string(),
                right: Box::new(Self::from_borrowed(right)),
            },
            gd_ast::GdExpr::UnaryOp { op, operand, .. } => Self::UnaryOp {
                span,
                op: (*op).to_string(),
                operand: Box::new(Self::from_borrowed(operand)),
            },
            gd_ast::GdExpr::Cast {
                expr, target_type, ..
            } => Self::Cast {
                span,
                expr: Box::new(Self::from_borrowed(expr)),
                target_type: (*target_type).to_string(),
            },
            gd_ast::GdExpr::Is {
                expr, type_name, ..
            } => Self::Is {
                span,
                expr: Box::new(Self::from_borrowed(expr)),
                type_name: (*type_name).to_string(),
            },
            gd_ast::GdExpr::Ternary {
                true_val,
                condition,
                false_val,
                ..
            } => Self::Ternary {
                span,
                true_val: Box::new(Self::from_borrowed(true_val)),
                condition: Box::new(Self::from_borrowed(condition)),
                false_val: Box::new(Self::from_borrowed(false_val)),
            },
            gd_ast::GdExpr::Await { expr, .. } => Self::Await {
                span,
                expr: Box::new(Self::from_borrowed(expr)),
            },
            gd_ast::GdExpr::Lambda { func, .. } => Self::Lambda {
                span,
                func: Box::new(OwnedFunc::from_borrowed(func)),
            },
            gd_ast::GdExpr::Preload { path, .. } => Self::Preload {
                span,
                path: (*path).to_string(),
            },
            gd_ast::GdExpr::Invalid { .. } => Self::Invalid { span },
        }
    }
}

impl OwnedStmt {
    #[must_use]
    pub fn from_borrowed(stmt: &gd_ast::GdStmt) -> Self {
        match stmt {
            gd_ast::GdStmt::Expr { node, expr } => Self::Expr {
                span: Some(node_span(*node)),
                expr: OwnedExpr::from_borrowed(expr),
            },
            gd_ast::GdStmt::Var(v) => Self::Var(OwnedVar::from_borrowed(v)),
            gd_ast::GdStmt::Assign {
                node,
                target,
                value,
            } => Self::Assign {
                span: Some(node_span(*node)),
                target: OwnedExpr::from_borrowed(target),
                value: OwnedExpr::from_borrowed(value),
            },
            gd_ast::GdStmt::AugAssign {
                node,
                target,
                op,
                value,
            } => Self::AugAssign {
                span: Some(node_span(*node)),
                target: OwnedExpr::from_borrowed(target),
                op: (*op).to_string(),
                value: OwnedExpr::from_borrowed(value),
            },
            gd_ast::GdStmt::Return { node, value } => Self::Return {
                span: Some(node_span(*node)),
                value: value.as_ref().map(OwnedExpr::from_borrowed),
            },
            gd_ast::GdStmt::If(i) => Self::If(OwnedIf::from_borrowed(i)),
            gd_ast::GdStmt::For {
                node,
                var,
                var_type,
                iter,
                body,
                ..
            } => Self::For {
                span: Some(node_span(*node)),
                var: (*var).to_string(),
                var_type: var_type.as_ref().map(OwnedTypeRef::from_borrowed),
                iter: OwnedExpr::from_borrowed(iter),
                body: body.iter().map(OwnedStmt::from_borrowed).collect(),
            },
            gd_ast::GdStmt::While {
                node,
                condition,
                body,
            } => Self::While {
                span: Some(node_span(*node)),
                condition: OwnedExpr::from_borrowed(condition),
                body: body.iter().map(OwnedStmt::from_borrowed).collect(),
            },
            gd_ast::GdStmt::Match { node, value, arms } => Self::Match {
                span: Some(node_span(*node)),
                value: OwnedExpr::from_borrowed(value),
                arms: arms.iter().map(OwnedMatchArm::from_borrowed).collect(),
            },
            gd_ast::GdStmt::Pass { node } => Self::Pass {
                span: Some(node_span(*node)),
            },
            gd_ast::GdStmt::Break { node } => Self::Break {
                span: Some(node_span(*node)),
            },
            gd_ast::GdStmt::Continue { node } => Self::Continue {
                span: Some(node_span(*node)),
            },
            gd_ast::GdStmt::Breakpoint { node } => Self::Breakpoint {
                span: Some(node_span(*node)),
            },
            gd_ast::GdStmt::Invalid { node } => Self::Invalid {
                span: Some(node_span(*node)),
            },
        }
    }
}
