//! Span-preserving printer for owned AST types.
//!
//! Serializes an [`OwnedFile`] (or sub-node) back to GDScript source text.
//!
//! **Span preservation:** nodes that still carry an original [`Span`] are
//! emitted verbatim from the original source — zero diff noise for untouched
//! code.  Nodes whose span was cleared by the rewriter (i.e. `None`) are
//! printed structurally from their fields.
//!
//! The printer does not impose formatting opinions.  Run `gd fmt` as a
//! post-pass if formatting matters.

use crate::ast_owned::{
    OwnedAnnotation, OwnedClass, OwnedDecl, OwnedExpr, OwnedExtends, OwnedFile, OwnedFunc,
    OwnedIf, OwnedMatchArm, OwnedParam, OwnedStmt, OwnedVar, Span,
};

// ═══════════════════════════════════════════════════════════════════════
//  Public API
// ═══════════════════════════════════════════════════════════════════════

/// Print an entire file back to source text.
///
/// `source` is the original source text (needed for span-preserved nodes).
#[must_use]
pub fn print_file(file: &OwnedFile, source: &str) -> String {
    if let Some(span) = file.span {
        return verbatim(span, source);
    }
    let mut out = String::new();

    if file.is_tool {
        out.push_str("@tool\n");
    }
    if let Some(cn) = &file.class_name {
        out.push_str("class_name ");
        out.push_str(cn);
        out.push('\n');
    }
    if let Some(ext) = &file.extends {
        out.push_str("extends ");
        out.push_str(&print_extends(ext));
        out.push('\n');
    }
    if file.is_tool || file.class_name.is_some() || file.extends.is_some() {
        out.push('\n');
    }

    for (i, decl) in file.declarations.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        print_decl(decl, source, &mut out, "");
    }

    if !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

/// Print a single expression.
#[must_use]
pub fn print_expr(expr: &OwnedExpr, source: &str) -> String {
    let mut out = String::new();
    write_expr(expr, source, &mut out);
    out
}

/// Print a single statement.
#[must_use]
pub fn print_stmt(stmt: &OwnedStmt, source: &str) -> String {
    let mut out = String::new();
    write_stmt(stmt, source, &mut out, "");
    out
}

// ═══════════════════════════════════════════════════════════════════════
//  Internal — expressions
// ═══════════════════════════════════════════════════════════════════════

#[allow(clippy::too_many_lines)]
fn write_expr(expr: &OwnedExpr, source: &str, out: &mut String) {
    if let Some(span) = expr.span() {
        out.push_str(&verbatim(span, source));
        return;
    }
    match expr {
        OwnedExpr::IntLiteral { value, .. }
        | OwnedExpr::FloatLiteral { value, .. }
        | OwnedExpr::StringLiteral { value, .. }
        | OwnedExpr::StringName { value, .. } => out.push_str(value),
        OwnedExpr::Bool { value, .. } => out.push_str(if *value { "true" } else { "false" }),
        OwnedExpr::Null { .. } => out.push_str("null"),

        OwnedExpr::Ident { name, .. } => out.push_str(name),

        OwnedExpr::Array { elements, .. } => {
            out.push('[');
            for (i, elem) in elements.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                write_expr(elem, source, out);
            }
            out.push(']');
        }

        OwnedExpr::Dict { pairs, .. } => {
            out.push('{');
            for (i, (k, v)) in pairs.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                write_expr(k, source, out);
                out.push_str(": ");
                write_expr(v, source, out);
            }
            out.push('}');
        }

        OwnedExpr::Call { callee, args, .. } => {
            write_expr(callee, source, out);
            out.push('(');
            write_args(args, source, out);
            out.push(')');
        }

        OwnedExpr::MethodCall { receiver, method, args, .. } => {
            write_expr(receiver, source, out);
            out.push('.');
            out.push_str(method);
            out.push('(');
            write_args(args, source, out);
            out.push(')');
        }

        OwnedExpr::SuperCall { method, args, .. } => {
            out.push_str("super");
            if let Some(m) = method {
                out.push('.');
                out.push_str(m);
            }
            out.push('(');
            write_args(args, source, out);
            out.push(')');
        }

        OwnedExpr::PropertyAccess { receiver, property, .. } => {
            write_expr(receiver, source, out);
            out.push('.');
            out.push_str(property);
        }

        OwnedExpr::Subscript { receiver, index, .. } => {
            write_expr(receiver, source, out);
            out.push('[');
            write_expr(index, source, out);
            out.push(']');
        }

        OwnedExpr::GetNode { path, .. } => {
            out.push('$');
            out.push_str(path);
        }

        OwnedExpr::BinOp { left, op, right, .. } => {
            write_expr(left, source, out);
            out.push(' ');
            out.push_str(op);
            out.push(' ');
            write_expr(right, source, out);
        }

        OwnedExpr::UnaryOp { op, operand, .. } => {
            out.push_str(op);
            // `not` needs a space; `-`, `~` do not.
            if (*op == "not" || *op == "not ") && !op.ends_with(' ') {
                out.push(' ');
            }
            write_expr(operand, source, out);
        }

        OwnedExpr::Cast { expr, target_type, .. } => {
            write_expr(expr, source, out);
            out.push_str(" as ");
            out.push_str(target_type);
        }

        OwnedExpr::Is { expr, type_name, .. } => {
            write_expr(expr, source, out);
            out.push_str(" is ");
            out.push_str(type_name);
        }

        OwnedExpr::Ternary { true_val, condition, false_val, .. } => {
            write_expr(true_val, source, out);
            out.push_str(" if ");
            write_expr(condition, source, out);
            out.push_str(" else ");
            write_expr(false_val, source, out);
        }

        OwnedExpr::Await { expr, .. } => {
            out.push_str("await ");
            write_expr(expr, source, out);
        }

        OwnedExpr::Lambda { func, .. } => {
            out.push_str("func");
            out.push('(');
            write_params(&func.params, source, out);
            out.push(')');
            if let Some(ret) = &func.return_type {
                out.push_str(" -> ");
                out.push_str(&ret.name);
            }
            out.push_str(": ");
            // Lambda bodies are typically single expressions or short.
            for (i, stmt) in func.body.iter().enumerate() {
                if i > 0 {
                    out.push_str("; ");
                }
                write_stmt(stmt, source, out, "");
            }
        }

        OwnedExpr::Preload { path, .. } => {
            out.push_str("preload(\"");
            out.push_str(path);
            out.push_str("\")");
        }

        OwnedExpr::Invalid { .. } => out.push_str("# <invalid>"),
    }
}

fn write_args(args: &[OwnedExpr], source: &str, out: &mut String) {
    for (i, arg) in args.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        write_expr(arg, source, out);
    }
}

fn write_params(params: &[OwnedParam], source: &str, out: &mut String) {
    for (i, p) in params.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        if let Some(span) = p.span {
            out.push_str(&verbatim(span, source));
        } else {
            out.push_str(&p.name);
            if let Some(t) = &p.type_ann {
                out.push_str(": ");
                out.push_str(&t.name);
            }
            if let Some(d) = &p.default {
                out.push_str(" = ");
                write_expr(d, source, out);
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  Internal — statements
// ═══════════════════════════════════════════════════════════════════════

fn write_stmt(stmt: &OwnedStmt, source: &str, out: &mut String, indent: &str) {
    if let Some(span) = stmt.span() {
        out.push_str(&verbatim(span, source));
        return;
    }
    match stmt {
        OwnedStmt::Expr { expr, .. } => {
            out.push_str(indent);
            write_expr(expr, source, out);
            out.push('\n');
        }

        OwnedStmt::Var(v) => write_var(v, source, out, indent),

        OwnedStmt::Assign { target, value, .. } => {
            out.push_str(indent);
            write_expr(target, source, out);
            out.push_str(" = ");
            write_expr(value, source, out);
            out.push('\n');
        }

        OwnedStmt::AugAssign { target, op, value, .. } => {
            out.push_str(indent);
            write_expr(target, source, out);
            out.push(' ');
            out.push_str(op);
            out.push(' ');
            write_expr(value, source, out);
            out.push('\n');
        }

        OwnedStmt::Return { value, .. } => {
            out.push_str(indent);
            out.push_str("return");
            if let Some(v) = value {
                out.push(' ');
                write_expr(v, source, out);
            }
            out.push('\n');
        }

        OwnedStmt::If(i) => write_if(i, source, out, indent),

        OwnedStmt::For { var, var_type, iter, body, .. } => {
            out.push_str(indent);
            out.push_str("for ");
            out.push_str(var);
            if let Some(t) = var_type {
                out.push_str(": ");
                out.push_str(&t.name);
            }
            out.push_str(" in ");
            write_expr(iter, source, out);
            out.push_str(":\n");
            write_body(body, source, out, indent);
        }

        OwnedStmt::While { condition, body, .. } => {
            out.push_str(indent);
            out.push_str("while ");
            write_expr(condition, source, out);
            out.push_str(":\n");
            write_body(body, source, out, indent);
        }

        OwnedStmt::Match { value, arms, .. } => {
            out.push_str(indent);
            out.push_str("match ");
            write_expr(value, source, out);
            out.push_str(":\n");
            let inner = format!("{indent}\t");
            for arm in arms {
                write_match_arm(arm, source, out, &inner);
            }
        }

        OwnedStmt::Pass { .. } => {
            out.push_str(indent);
            out.push_str("pass\n");
        }
        OwnedStmt::Break { .. } => {
            out.push_str(indent);
            out.push_str("break\n");
        }
        OwnedStmt::Continue { .. } => {
            out.push_str(indent);
            out.push_str("continue\n");
        }
        OwnedStmt::Breakpoint { .. } => {
            out.push_str(indent);
            out.push_str("breakpoint\n");
        }
        OwnedStmt::Invalid { .. } => {
            out.push_str(indent);
            out.push_str("# <invalid>\n");
        }
    }
}

fn write_body(body: &[OwnedStmt], source: &str, out: &mut String, parent_indent: &str) {
    let inner = format!("{parent_indent}\t");
    if body.is_empty() {
        out.push_str(&inner);
        out.push_str("pass\n");
    } else {
        for stmt in body {
            write_stmt(stmt, source, out, &inner);
        }
    }
}

fn write_if(i: &OwnedIf, source: &str, out: &mut String, indent: &str) {
    out.push_str(indent);
    out.push_str("if ");
    write_expr(&i.condition, source, out);
    out.push_str(":\n");
    write_body(&i.body, source, out, indent);

    for (cond, body) in &i.elif_branches {
        out.push_str(indent);
        out.push_str("elif ");
        write_expr(cond, source, out);
        out.push_str(":\n");
        write_body(body, source, out, indent);
    }

    if let Some(else_body) = &i.else_body {
        out.push_str(indent);
        out.push_str("else:\n");
        write_body(else_body, source, out, indent);
    }
}

fn write_match_arm(arm: &OwnedMatchArm, source: &str, out: &mut String, indent: &str) {
    if let Some(span) = arm.span {
        out.push_str(&verbatim(span, source));
        return;
    }
    out.push_str(indent);
    for (i, pat) in arm.patterns.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        write_expr(pat, source, out);
    }
    if let Some(guard) = &arm.guard {
        out.push_str(" when ");
        write_expr(guard, source, out);
    }
    out.push_str(":\n");
    write_body(&arm.body, source, out, indent);
}

// ═══════════════════════════════════════════════════════════════════════
//  Internal — declarations
// ═══════════════════════════════════════════════════════════════════════

fn print_decl(decl: &OwnedDecl, source: &str, out: &mut String, indent: &str) {
    match decl {
        OwnedDecl::Func(f) => write_func(f, source, out, indent),
        OwnedDecl::Var(v) => write_var(v, source, out, indent),
        OwnedDecl::Signal(s) => write_signal(s, source, out, indent),
        OwnedDecl::Enum(e) => write_enum(e, source, out, indent),
        OwnedDecl::Class(c) => write_class(c, source, out, indent),
        OwnedDecl::Stmt(s) => write_stmt(s, source, out, indent),
    }
}

fn write_func(f: &OwnedFunc, source: &str, out: &mut String, indent: &str) {
    if let Some(span) = f.span {
        out.push_str(&verbatim(span, source));
        return;
    }

    for ann in &f.annotations {
        write_annotation(ann, source, out, indent);
    }

    out.push_str(indent);
    if f.is_static {
        out.push_str("static ");
    }
    out.push_str("func ");
    out.push_str(&f.name);
    out.push('(');
    write_params(&f.params, source, out);
    out.push(')');
    if let Some(ret) = &f.return_type {
        out.push_str(" -> ");
        out.push_str(&ret.name);
    }
    out.push_str(":\n");
    write_body(&f.body, source, out, indent);
}

fn write_var(v: &OwnedVar, source: &str, out: &mut String, indent: &str) {
    if let Some(span) = v.span {
        out.push_str(&verbatim(span, source));
        return;
    }

    for ann in &v.annotations {
        write_annotation(ann, source, out, indent);
    }

    out.push_str(indent);
    if v.is_static {
        out.push_str("static ");
    }
    out.push_str(if v.is_const { "const " } else { "var " });
    out.push_str(&v.name);
    if let Some(t) = &v.type_ann {
        out.push_str(": ");
        out.push_str(&t.name);
    }
    if let Some(val) = &v.value {
        out.push_str(" = ");
        write_expr(val, source, out);
    }
    out.push('\n');
}

fn write_signal(s: &crate::ast_owned::OwnedSignal, source: &str, out: &mut String, indent: &str) {
    if let Some(span) = s.span {
        out.push_str(&verbatim(span, source));
        return;
    }
    out.push_str(indent);
    out.push_str("signal ");
    out.push_str(&s.name);
    if !s.params.is_empty() {
        out.push('(');
        write_params(&s.params, source, out);
        out.push(')');
    }
    out.push('\n');
}

fn write_enum(e: &crate::ast_owned::OwnedEnum, source: &str, out: &mut String, indent: &str) {
    if let Some(span) = e.span {
        out.push_str(&verbatim(span, source));
        return;
    }
    out.push_str(indent);
    out.push_str("enum ");
    out.push_str(&e.name);
    out.push_str(" {\n");
    let inner = format!("{indent}\t");
    for (i, m) in e.members.iter().enumerate() {
        if let Some(span) = m.span {
            out.push_str(&verbatim(span, source));
        } else {
            out.push_str(&inner);
            out.push_str(&m.name);
            if let Some(val) = &m.value {
                out.push_str(" = ");
                write_expr(val, source, out);
            }
        }
        if i + 1 < e.members.len() {
            out.push(',');
        }
        out.push('\n');
    }
    out.push_str(indent);
    out.push_str("}\n");
}

fn write_class(c: &OwnedClass, source: &str, out: &mut String, indent: &str) {
    if let Some(span) = c.span {
        out.push_str(&verbatim(span, source));
        return;
    }
    out.push_str(indent);
    out.push_str("class ");
    out.push_str(&c.name);
    if let Some(ext) = &c.extends {
        out.push_str(" extends ");
        out.push_str(&print_extends(ext));
    }
    out.push_str(":\n");
    let inner = format!("{indent}\t");
    for decl in &c.declarations {
        print_decl(decl, source, out, &inner);
    }
}

fn write_annotation(a: &OwnedAnnotation, source: &str, out: &mut String, indent: &str) {
    if let Some(span) = a.span {
        out.push_str(&verbatim(span, source));
        return;
    }
    out.push_str(indent);
    out.push('@');
    out.push_str(&a.name);
    if !a.args.is_empty() {
        out.push('(');
        write_args(&a.args, source, out);
        out.push(')');
    }
    out.push('\n');
}

// ═══════════════════════════════════════════════════════════════════════
//  Helpers
// ═══════════════════════════════════════════════════════════════════════

fn verbatim(span: Span, source: &str) -> String {
    source[span.start..span.end].to_string()
}

fn print_extends(ext: &OwnedExtends) -> String {
    match ext {
        OwnedExtends::Class(c) => c.clone(),
        OwnedExtends::Path(p) => format!("\"{p}\""),
    }
}
