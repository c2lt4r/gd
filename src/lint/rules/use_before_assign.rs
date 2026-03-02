use crate::core::gd_ast::{GdDecl, GdExpr, GdExtends, GdFile, GdStmt};
use std::collections::{HashMap, HashSet};

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct UseBeforeAssign;

impl LintRule for UseBeforeAssign {
    fn name(&self) -> &'static str {
        "use-before-assign"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Correctness
    }

    fn default_enabled(&self) -> bool {
        false // opt-in — cross-function analysis, can have false positives
    }

    fn check(&self, file: &GdFile<'_>, _source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let members = collect_member_vars(file);
        if members.is_empty() {
            return Vec::new();
        }

        let func_info = collect_function_info(file, &members);

        // For Node subclasses, members assigned in _ready() or _init() (directly
        // or transitively through called methods) are guaranteed initialized
        // before any other user method runs.
        let extends_class = match file.extends {
            Some(GdExtends::Class(cls)) => Some(cls),
            _ => None,
        };
        let ready_assigned = extends_class
            .filter(|cls| crate::class_db::inherits(cls, "Node") || *cls == "Node")
            .map(|_| {
                let mut assigned = transitive_assigns("_ready", &func_info);
                assigned.extend(transitive_assigns("_init", &func_info));
                assigned
            })
            .unwrap_or_default();

        let mut diags = Vec::new();
        check_functions(file, &members, &func_info, &ready_assigned, &mut diags);
        diags
    }
}

/// Collect class-level member variable names that have no initializer or `= null`.
fn collect_member_vars(file: &GdFile) -> HashSet<String> {
    let mut members = HashSet::new();
    for decl in &file.declarations {
        if let GdDecl::Var(var) = decl
            && !var.is_const
        {
            let has_non_null_init = var
                .value
                .as_ref()
                .is_some_and(|v| !matches!(v, GdExpr::Null { .. }));
            if !has_non_null_init {
                members.insert(var.name.to_string());
            }
        }
    }
    members
}

struct FuncInfo {
    reads_before_assign: HashSet<String>,
    assigns: HashSet<String>,
    calls: HashSet<String>,
}

fn collect_function_info(file: &GdFile, members: &HashSet<String>) -> HashMap<String, FuncInfo> {
    let mut info = HashMap::new();
    for decl in &file.declarations {
        if let GdDecl::Func(func) = decl {
            let mut assigned = HashSet::new();
            let mut reads = HashSet::new();
            let mut null_checked = HashSet::new();
            let mut calls = HashSet::new();
            scan_stmts_for_member_access(
                &func.body,
                members,
                &mut assigned,
                &mut reads,
                &mut null_checked,
            );
            collect_calls_in_stmts(&func.body, &mut calls);
            // Members that are null-checked (bare identifier reads) within the
            // function are assumed to be properly guarded before dereference.
            for m in &null_checked {
                reads.remove(m);
            }
            info.insert(
                func.name.to_string(),
                FuncInfo {
                    reads_before_assign: reads,
                    assigns: assigned,
                    calls,
                },
            );
        }
    }
    info
}

/// Collect all function names called within statements (for transitive assignment tracking).
fn collect_calls_in_stmts(stmts: &[GdStmt], calls: &mut HashSet<String>) {
    for stmt in stmts {
        collect_calls_in_expr_tree(stmt, calls);
        // Recurse into control flow bodies
        match stmt {
            GdStmt::If(if_stmt) => {
                collect_calls_in_stmts(&if_stmt.body, calls);
                for (_, branch) in &if_stmt.elif_branches {
                    collect_calls_in_stmts(branch, calls);
                }
                if let Some(else_body) = &if_stmt.else_body {
                    collect_calls_in_stmts(else_body, calls);
                }
            }
            GdStmt::For { body, .. } | GdStmt::While { body, .. } => {
                collect_calls_in_stmts(body, calls);
            }
            GdStmt::Match { arms, .. } => {
                for arm in arms {
                    collect_calls_in_stmts(&arm.body, calls);
                }
            }
            _ => {}
        }
    }
}

/// Extract call names from all expressions in a statement.
fn collect_calls_in_expr_tree(stmt: &GdStmt, calls: &mut HashSet<String>) {
    match stmt {
        GdStmt::Expr { expr, .. } => collect_calls_in_expr(expr, calls),
        GdStmt::Var(var) => {
            if let Some(value) = &var.value {
                collect_calls_in_expr(value, calls);
            }
        }
        GdStmt::Assign { value, .. } | GdStmt::AugAssign { value, .. } => {
            collect_calls_in_expr(value, calls);
        }
        GdStmt::Return { value: Some(v), .. } => collect_calls_in_expr(v, calls),
        _ => {}
    }
}

fn collect_calls_in_expr(expr: &GdExpr, calls: &mut HashSet<String>) {
    match expr {
        // Plain call: func_name()
        GdExpr::Call { callee, args, .. } => {
            if let GdExpr::Ident { name, .. } = callee.as_ref() {
                calls.insert((*name).to_string());
            }
            collect_calls_in_expr(callee, calls);
            for a in args {
                collect_calls_in_expr(a, calls);
            }
        }
        // self.method()
        GdExpr::MethodCall {
            receiver,
            method,
            args,
            ..
        } => {
            if let GdExpr::Ident { name: "self", .. } = receiver.as_ref() {
                calls.insert((*method).to_string());
            }
            collect_calls_in_expr(receiver, calls);
            for a in args {
                collect_calls_in_expr(a, calls);
            }
        }
        GdExpr::BinOp { left, right, .. } => {
            collect_calls_in_expr(left, calls);
            collect_calls_in_expr(right, calls);
        }
        GdExpr::UnaryOp { operand, .. } => collect_calls_in_expr(operand, calls),
        GdExpr::PropertyAccess { receiver, .. } => collect_calls_in_expr(receiver, calls),
        GdExpr::Subscript {
            receiver, index, ..
        } => {
            collect_calls_in_expr(receiver, calls);
            collect_calls_in_expr(index, calls);
        }
        GdExpr::Ternary {
            condition,
            true_val,
            false_val,
            ..
        } => {
            collect_calls_in_expr(condition, calls);
            collect_calls_in_expr(true_val, calls);
            collect_calls_in_expr(false_val, calls);
        }
        GdExpr::Array { elements, .. } => {
            for e in elements {
                collect_calls_in_expr(e, calls);
            }
        }
        GdExpr::Dict { pairs, .. } => {
            for (k, v) in pairs {
                collect_calls_in_expr(k, calls);
                collect_calls_in_expr(v, calls);
            }
        }
        GdExpr::Cast { expr: inner, .. }
        | GdExpr::Is { expr: inner, .. }
        | GdExpr::Await { expr: inner, .. } => {
            collect_calls_in_expr(inner, calls);
        }
        _ => {}
    }
}

/// Compute the transitive set of member assignments reachable from a function
/// by following its call graph (BFS). E.g. `_ready -> _build_ui -> _build_move_panel`
/// will collect assigns from all three functions.
fn transitive_assigns(start: &str, func_info: &HashMap<String, FuncInfo>) -> HashSet<String> {
    let mut result = HashSet::new();
    let mut visited = HashSet::new();
    let mut queue = std::collections::VecDeque::new();

    if func_info.contains_key(start) {
        queue.push_back(start.to_string());
    }

    while let Some(func_name) = queue.pop_front() {
        if !visited.insert(func_name.clone()) {
            continue;
        }
        if let Some(info) = func_info.get(&func_name) {
            result.extend(info.assigns.iter().cloned());
            for callee in &info.calls {
                if !visited.contains(callee) {
                    queue.push_back(callee.clone());
                }
            }
        }
    }
    result
}

fn scan_stmts_for_member_access(
    stmts: &[GdStmt],
    members: &HashSet<String>,
    assigned: &mut HashSet<String>,
    reads_before_assign: &mut HashSet<String>,
    null_checked: &mut HashSet<String>,
) {
    for stmt in stmts {
        scan_statement(stmt, members, assigned, reads_before_assign, null_checked);
    }
}

fn scan_statement(
    stmt: &GdStmt,
    members: &HashSet<String>,
    assigned: &mut HashSet<String>,
    reads_before_assign: &mut HashSet<String>,
    null_checked: &mut HashSet<String>,
) {
    // Check for member assignment: member = ... or self.member = ...
    if let GdStmt::Assign { target, value, .. } = stmt
        && let Some(member) = extract_member_assign(target, members)
    {
        collect_member_reads_expr(value, members, assigned, reads_before_assign, null_checked);
        assigned.insert(member);
        return;
    }

    // Collect member reads from expressions in this statement
    match stmt {
        GdStmt::Expr { expr, .. } => {
            collect_member_reads_expr(expr, members, assigned, reads_before_assign, null_checked);
        }
        GdStmt::Var(var) => {
            if let Some(value) = &var.value {
                collect_member_reads_expr(
                    value,
                    members,
                    assigned,
                    reads_before_assign,
                    null_checked,
                );
            }
        }
        GdStmt::Assign { target, value, .. } | GdStmt::AugAssign { target, value, .. } => {
            collect_member_reads_expr(target, members, assigned, reads_before_assign, null_checked);
            collect_member_reads_expr(value, members, assigned, reads_before_assign, null_checked);
        }
        GdStmt::Return { value: Some(v), .. } => {
            collect_member_reads_expr(v, members, assigned, reads_before_assign, null_checked);
        }
        _ => {}
    }

    // Recurse into control flow bodies
    match stmt {
        GdStmt::If(if_stmt) => {
            collect_member_reads_expr(
                &if_stmt.condition,
                members,
                assigned,
                reads_before_assign,
                null_checked,
            );
            scan_stmts_for_member_access(
                &if_stmt.body,
                members,
                assigned,
                reads_before_assign,
                null_checked,
            );
            for (cond, branch) in &if_stmt.elif_branches {
                collect_member_reads_expr(
                    cond,
                    members,
                    assigned,
                    reads_before_assign,
                    null_checked,
                );
                scan_stmts_for_member_access(
                    branch,
                    members,
                    assigned,
                    reads_before_assign,
                    null_checked,
                );
            }
            if let Some(else_body) = &if_stmt.else_body {
                scan_stmts_for_member_access(
                    else_body,
                    members,
                    assigned,
                    reads_before_assign,
                    null_checked,
                );
            }
        }
        GdStmt::For { body, .. } | GdStmt::While { body, .. } => {
            scan_stmts_for_member_access(
                body,
                members,
                assigned,
                reads_before_assign,
                null_checked,
            );
        }
        GdStmt::Match { arms, .. } => {
            for arm in arms {
                scan_stmts_for_member_access(
                    &arm.body,
                    members,
                    assigned,
                    reads_before_assign,
                    null_checked,
                );
            }
        }
        _ => {}
    }
}

/// Check if an assignment target is a member variable. Returns the member name.
fn extract_member_assign(target: &GdExpr, members: &HashSet<String>) -> Option<String> {
    match target {
        // member = value
        GdExpr::Ident { name, .. } if members.contains(*name) => Some((*name).to_string()),
        // self.member = value
        GdExpr::PropertyAccess {
            receiver, property, ..
        } if matches!(receiver.as_ref(), GdExpr::Ident { name: "self", .. })
            && members.contains(*property) =>
        {
            Some((*property).to_string())
        }
        _ => None,
    }
}

/// Recursively collect member reads from an expression tree.
fn collect_member_reads_expr(
    expr: &GdExpr,
    members: &HashSet<String>,
    assigned: &HashSet<String>,
    reads_before_assign: &mut HashSet<String>,
    null_checked: &mut HashSet<String>,
) {
    match expr {
        // Bare identifier reads are safe (null checks, comparisons, args)
        // but record them as evidence the function is null-aware about this member.
        GdExpr::Ident { name, .. } if members.contains(*name) && !assigned.contains(*name) => {
            null_checked.insert((*name).to_string());
        }

        // self.member — flag member reads via self
        GdExpr::PropertyAccess {
            receiver, property, ..
        } if matches!(receiver.as_ref(), GdExpr::Ident { name: "self", .. })
            && members.contains(*property)
            && !assigned.contains(*property) =>
        {
            reads_before_assign.insert((*property).to_string());
        }

        // member.something / member.method() / member[index] — dereferencing a possibly-unassigned member
        GdExpr::PropertyAccess { receiver, .. }
        | GdExpr::MethodCall { receiver, .. }
        | GdExpr::Subscript { receiver, .. } => {
            if let GdExpr::Ident { name, .. } = receiver.as_ref()
                && members.contains(*name)
                && !assigned.contains(*name)
            {
                reads_before_assign.insert((*name).to_string());
            } else {
                recurse_member_reads(expr, members, assigned, reads_before_assign, null_checked);
            }
        }

        // Recurse into sub-expressions
        _ => {
            recurse_member_reads(expr, members, assigned, reads_before_assign, null_checked);
        }
    }
}

fn recurse_member_reads(
    expr: &GdExpr,
    members: &HashSet<String>,
    assigned: &HashSet<String>,
    reads_before_assign: &mut HashSet<String>,
    null_checked: &mut HashSet<String>,
) {
    match expr {
        GdExpr::Call { callee, args, .. } => {
            collect_member_reads_expr(callee, members, assigned, reads_before_assign, null_checked);
            for a in args {
                collect_member_reads_expr(a, members, assigned, reads_before_assign, null_checked);
            }
        }
        GdExpr::MethodCall { receiver, args, .. } => {
            collect_member_reads_expr(
                receiver,
                members,
                assigned,
                reads_before_assign,
                null_checked,
            );
            for a in args {
                collect_member_reads_expr(a, members, assigned, reads_before_assign, null_checked);
            }
        }
        GdExpr::PropertyAccess { receiver, .. } => {
            collect_member_reads_expr(
                receiver,
                members,
                assigned,
                reads_before_assign,
                null_checked,
            );
        }
        GdExpr::BinOp { left, right, .. } => {
            collect_member_reads_expr(left, members, assigned, reads_before_assign, null_checked);
            collect_member_reads_expr(right, members, assigned, reads_before_assign, null_checked);
        }
        GdExpr::UnaryOp { operand, .. } => {
            collect_member_reads_expr(
                operand,
                members,
                assigned,
                reads_before_assign,
                null_checked,
            );
        }
        GdExpr::Subscript {
            receiver, index, ..
        } => {
            collect_member_reads_expr(
                receiver,
                members,
                assigned,
                reads_before_assign,
                null_checked,
            );
            collect_member_reads_expr(index, members, assigned, reads_before_assign, null_checked);
        }
        GdExpr::Ternary {
            condition,
            true_val,
            false_val,
            ..
        } => {
            collect_member_reads_expr(
                condition,
                members,
                assigned,
                reads_before_assign,
                null_checked,
            );
            collect_member_reads_expr(
                true_val,
                members,
                assigned,
                reads_before_assign,
                null_checked,
            );
            collect_member_reads_expr(
                false_val,
                members,
                assigned,
                reads_before_assign,
                null_checked,
            );
        }
        GdExpr::Array { elements, .. } => {
            for e in elements {
                collect_member_reads_expr(e, members, assigned, reads_before_assign, null_checked);
            }
        }
        GdExpr::Dict { pairs, .. } => {
            for (k, v) in pairs {
                collect_member_reads_expr(k, members, assigned, reads_before_assign, null_checked);
                collect_member_reads_expr(v, members, assigned, reads_before_assign, null_checked);
            }
        }
        GdExpr::Cast { expr: inner, .. }
        | GdExpr::Is { expr: inner, .. }
        | GdExpr::Await { expr: inner, .. } => {
            collect_member_reads_expr(inner, members, assigned, reads_before_assign, null_checked);
        }
        _ => {}
    }
}

fn check_functions(
    file: &GdFile,
    members: &HashSet<String>,
    func_info: &HashMap<String, FuncInfo>,
    ready_assigned: &HashSet<String>,
    diags: &mut Vec<LintDiagnostic>,
) {
    for decl in &file.declarations {
        if let GdDecl::Func(func) = decl {
            // For non-_ready/_init functions in Node subclasses, pre-populate
            // with members that _ready() and _init() guarantee are assigned.
            let mut assigned_so_far = if func.name == "_ready" || func.name == "_init" {
                HashSet::new()
            } else {
                ready_assigned.clone()
            };
            check_stmts_for_calls(
                &func.body,
                members,
                func_info,
                func.name,
                &mut assigned_so_far,
                diags,
            );
        }
    }
}

fn check_stmts_for_calls(
    stmts: &[GdStmt],
    members: &HashSet<String>,
    func_info: &HashMap<String, FuncInfo>,
    caller_name: &str,
    assigned_so_far: &mut HashSet<String>,
    diags: &mut Vec<LintDiagnostic>,
) {
    for stmt in stmts {
        // Track member assignments
        if let GdStmt::Assign { target, .. } = stmt
            && let Some(member) = extract_member_assign(target, members)
        {
            assigned_so_far.insert(member);
        }

        // Check for calls
        find_calls_in_stmt(stmt, func_info, caller_name, assigned_so_far, diags);

        // Recurse into control flow bodies
        match stmt {
            GdStmt::If(if_stmt) => {
                check_stmts_for_calls(
                    &if_stmt.body,
                    members,
                    func_info,
                    caller_name,
                    assigned_so_far,
                    diags,
                );
                for (_, branch) in &if_stmt.elif_branches {
                    check_stmts_for_calls(
                        branch,
                        members,
                        func_info,
                        caller_name,
                        assigned_so_far,
                        diags,
                    );
                }
                if let Some(else_body) = &if_stmt.else_body {
                    check_stmts_for_calls(
                        else_body,
                        members,
                        func_info,
                        caller_name,
                        assigned_so_far,
                        diags,
                    );
                }
            }
            GdStmt::For { body, .. } | GdStmt::While { body, .. } => {
                check_stmts_for_calls(
                    body,
                    members,
                    func_info,
                    caller_name,
                    assigned_so_far,
                    diags,
                );
            }
            GdStmt::Match { arms, .. } => {
                for arm in arms {
                    check_stmts_for_calls(
                        &arm.body,
                        members,
                        func_info,
                        caller_name,
                        assigned_so_far,
                        diags,
                    );
                }
            }
            _ => {}
        }
    }
}

/// Check all expressions in a statement for calls to functions that read unassigned members.
fn find_calls_in_stmt(
    stmt: &GdStmt,
    func_info: &HashMap<String, FuncInfo>,
    caller_name: &str,
    assigned_so_far: &HashSet<String>,
    diags: &mut Vec<LintDiagnostic>,
) {
    match stmt {
        GdStmt::Expr { expr, .. } => {
            find_calls_in_expr(expr, func_info, caller_name, assigned_so_far, diags);
        }
        GdStmt::Var(var) => {
            if let Some(value) = &var.value {
                find_calls_in_expr(value, func_info, caller_name, assigned_so_far, diags);
            }
        }
        GdStmt::Assign { value, .. } | GdStmt::AugAssign { value, .. } => {
            find_calls_in_expr(value, func_info, caller_name, assigned_so_far, diags);
        }
        GdStmt::Return { value: Some(v), .. } => {
            find_calls_in_expr(v, func_info, caller_name, assigned_so_far, diags);
        }
        GdStmt::If(if_stmt) => {
            find_calls_in_expr(
                &if_stmt.condition,
                func_info,
                caller_name,
                assigned_so_far,
                diags,
            );
        }
        _ => {}
    }
}

fn find_calls_in_expr(
    expr: &GdExpr,
    func_info: &HashMap<String, FuncInfo>,
    caller_name: &str,
    assigned_so_far: &HashSet<String>,
    diags: &mut Vec<LintDiagnostic>,
) {
    match expr {
        // Plain call: func_name()
        GdExpr::Call {
            callee, args, node, ..
        } => {
            if let GdExpr::Ident { name, .. } = callee.as_ref() {
                check_callee(name, node, func_info, caller_name, assigned_so_far, diags);
            }
            find_calls_in_expr(callee, func_info, caller_name, assigned_so_far, diags);
            for a in args {
                find_calls_in_expr(a, func_info, caller_name, assigned_so_far, diags);
            }
        }
        // self.method()
        GdExpr::MethodCall {
            receiver,
            method,
            args,
            node,
            ..
        } => {
            if let GdExpr::Ident { name: "self", .. } = receiver.as_ref() {
                check_callee(method, node, func_info, caller_name, assigned_so_far, diags);
            }
            find_calls_in_expr(receiver, func_info, caller_name, assigned_so_far, diags);
            for a in args {
                find_calls_in_expr(a, func_info, caller_name, assigned_so_far, diags);
            }
        }
        GdExpr::BinOp { left, right, .. } => {
            find_calls_in_expr(left, func_info, caller_name, assigned_so_far, diags);
            find_calls_in_expr(right, func_info, caller_name, assigned_so_far, diags);
        }
        GdExpr::UnaryOp { operand, .. } => {
            find_calls_in_expr(operand, func_info, caller_name, assigned_so_far, diags);
        }
        GdExpr::PropertyAccess { receiver, .. } => {
            find_calls_in_expr(receiver, func_info, caller_name, assigned_so_far, diags);
        }
        GdExpr::Subscript {
            receiver, index, ..
        } => {
            find_calls_in_expr(receiver, func_info, caller_name, assigned_so_far, diags);
            find_calls_in_expr(index, func_info, caller_name, assigned_so_far, diags);
        }
        GdExpr::Ternary {
            condition,
            true_val,
            false_val,
            ..
        } => {
            find_calls_in_expr(condition, func_info, caller_name, assigned_so_far, diags);
            find_calls_in_expr(true_val, func_info, caller_name, assigned_so_far, diags);
            find_calls_in_expr(false_val, func_info, caller_name, assigned_so_far, diags);
        }
        GdExpr::Array { elements, .. } => {
            for e in elements {
                find_calls_in_expr(e, func_info, caller_name, assigned_so_far, diags);
            }
        }
        GdExpr::Dict { pairs, .. } => {
            for (k, v) in pairs {
                find_calls_in_expr(k, func_info, caller_name, assigned_so_far, diags);
                find_calls_in_expr(v, func_info, caller_name, assigned_so_far, diags);
            }
        }
        GdExpr::Cast { expr: inner, .. }
        | GdExpr::Is { expr: inner, .. }
        | GdExpr::Await { expr: inner, .. } => {
            find_calls_in_expr(inner, func_info, caller_name, assigned_so_far, diags);
        }
        _ => {}
    }
}

fn check_callee(
    callee: &str,
    call_node: &tree_sitter::Node,
    func_info: &HashMap<String, FuncInfo>,
    caller_name: &str,
    assigned_so_far: &HashSet<String>,
    diags: &mut Vec<LintDiagnostic>,
) {
    if callee == caller_name {
        return;
    }
    if let Some(info) = func_info.get(callee) {
        for member in &info.reads_before_assign {
            if !assigned_so_far.contains(member) {
                diags.push(LintDiagnostic {
                    rule: "use-before-assign",
                    message: format!(
                        "`{callee}()` accesses member `{member}` which may not be assigned yet at this call site"
                    ),
                    severity: Severity::Warning,
                    line: call_node.start_position().row,
                    column: call_node.start_position().column,
                    end_column: None,
                    fix: None,
                    context_lines: None,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::gd_ast;
    use crate::core::parser;

    fn check(source: &str) -> Vec<LintDiagnostic> {
        let tree = parser::parse(source).unwrap();
        let file = gd_ast::convert(&tree, source);
        let config = LintConfig::default();
        UseBeforeAssign.check(&file, source, &config)
    }

    #[test]
    fn detects_call_before_assignment() {
        let source = "\
var target: Node2D

func _ready():
\tsetup_visuals()
\ttarget = get_node(\"Target\")

func setup_visuals():
\ttarget.modulate = Color.RED
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("setup_visuals()"));
        assert!(diags[0].message.contains("target"));
    }

    #[test]
    fn no_warning_after_assignment() {
        let source = "\
var target: Node2D

func _ready():
\ttarget = get_node(\"Target\")
\tsetup_visuals()

func setup_visuals():
\ttarget.modulate = Color.RED
";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_with_initializer() {
        let source = "\
var target: Node2D = Node2D.new()

func _ready():
\tsetup_visuals()

func setup_visuals():
\ttarget.modulate = Color.RED
";
        assert!(check(source).is_empty());
    }

    #[test]
    fn detects_self_access() {
        let source = "\
var hp: int

func _ready():
\tapply_damage()

func apply_damage():
\tself.hp -= 10
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("hp"));
    }

    #[test]
    fn self_method_call() {
        let source = "\
var target: Node2D

func _ready():
\tself.setup()

func setup():
\ttarget.visible = true
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("setup()"));
    }

    #[test]
    fn assignment_in_branch_counts() {
        let source = "\
var target: Node2D

func _ready():
\tif true:
\t\ttarget = get_node(\"T\")
\tsetup()

func setup():
\ttarget.visible = true
";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_unrelated_method() {
        let source = "\
var target: Node2D

func _ready():
\tother_func()
\ttarget = get_node(\"T\")

func other_func():
\tprint(\"hello\")
";
        assert!(check(source).is_empty());
    }

    #[test]
    fn null_initializer_treated_as_uninitialized() {
        let source = "\
var target = null

func _ready():
\tsetup()
\ttarget = get_node(\"T\")

func setup():
\ttarget.visible = true
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn const_member_ignored() {
        let source = "\
const SPEED := 10

func _ready():
\tsetup()

func setup():
\tprint(SPEED)
";
        assert!(check(source).is_empty());
    }

    #[test]
    fn opt_in() {
        assert!(!UseBeforeAssign.default_enabled());
    }

    // --- Node subclass _ready() suppression tests ---

    #[test]
    fn control_ready_assigns_suppresses_other_methods() {
        // Procedural UI: _ready calls _build_ui which assigns _label,
        // then _update reads _label — should not warn.
        let source = "\
extends Control

var _label: Label

func _ready():
\t_build_ui()

func _build_ui():
\t_label = Label.new()
\tadd_child(_label)

func _update():
\t_label.text = \"hello\"
";
        assert!(check(source).is_empty());
    }

    #[test]
    fn control_ready_direct_assign_suppresses() {
        // Direct assignment in _ready, read in another method.
        let source = "\
extends Control

var _btn: Button

func _ready():
\t_btn = Button.new()
\tadd_child(_btn)

func _on_pressed():
\t_btn.text = \"clicked\"
";
        assert!(check(source).is_empty());
    }

    #[test]
    fn control_still_warns_in_ready_itself() {
        // _ready calls setup before assigning — should still warn.
        let source = "\
extends Control

var _label: Label

func _ready():
\t_update()
\t_label = Label.new()

func _update():
\t_label.text = \"hello\"
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("_update()"));
    }

    #[test]
    fn non_node_class_no_suppression() {
        // RefCounted subclass — no _ready suppression.
        // Even though _ready assigns _data via _build, RefCounted is not a Node
        // so other functions calling _use_data should still warn.
        let source = "\
extends RefCounted

var _data: Dictionary

func _ready():
\t_build()

func _build():
\t_data = {}

func process():
\t_use_data()

func _use_data():
\t_data.clear()
";
        // RefCounted is not a Node, so no suppression for process() calling _use_data()
        let diags = check(source);
        assert!(!diags.is_empty());
    }

    #[test]
    fn panelcontainer_ready_suppresses() {
        // PanelContainer extends Node transitively — _ready suppression should work.
        let source = "\
extends PanelContainer

var _label: Label
var _arrow: Label

func _ready() -> void:
\t_build_ui()

func _build_ui() -> void:
\t_label = Label.new()
\t_arrow = Label.new()

func _process(delta: float) -> void:
\t_advance_queue()

func _advance_queue() -> void:
\t_label.text = \"hello\"
\t_arrow.visible = true
";
        assert!(check(source).is_empty());
    }

    #[test]
    fn node_subclass_suppresses() {
        // Node2D extends Node — should suppress.
        let source = "\
extends Node2D

var _sprite: Sprite2D

func _ready():
\t_sprite = Sprite2D.new()
\tadd_child(_sprite)

func _process(_delta):
\t_sprite.rotation += 0.1
";
        assert!(check(source).is_empty());
    }

    #[test]
    fn init_assigns_suppresses() {
        // _init() is the constructor — assignments there are guaranteed.
        let source = "\
extends Node

var _processor: Node

func _init() -> void:
\t_processor = Node.new()

func _do_work():
\t_use_processor()

func _use_processor():
\t_processor.queue_free()
";
        assert!(check(source).is_empty());
    }

    #[test]
    fn bare_identifier_null_check_not_flagged() {
        // Bare identifier reads (null checks, comparisons, passing as args)
        // are safe — only dereferences (member.something) are dangerous.
        let source = "\
extends Node

var _target: Node2D

func _ready():
\t_target = get_node(\"Target\")

func _check():
\tif _target:
\t\t_target.visible = true

func _compare(other):
\tif other == _target:
\t\tpass

func _pass_arg():
\tprint(_target)
";
        assert!(check(source).is_empty());
    }

    #[test]
    fn bare_identifier_guard_with_return_not_flagged() {
        // `if not member: return` is a common guard pattern — safe.
        let source = "\
extends Node

var _active: Node

func _ready():
\t_active = Node.new()

func _process(_delta):
\tif not _active:
\t\treturn
\t_active.queue_free()
";
        assert!(check(source).is_empty());
    }

    #[test]
    fn null_guard_suppresses_dereference() {
        // If a function null-checks a member before dereferencing it,
        // the function is null-aware — don't flag the dereference.
        let source = "\
extends Node

var _active: Node2D

func _ready():
\t_active = get_node(\"Active\")

func _show():
\tif _active:
\t\t_active.visible = true

func _hide():
\tif not _active:
\t\treturn
\t_active.visible = false

func _process(_delta):
\t_show()
\t_hide()
";
        assert!(check(source).is_empty());
    }

    #[test]
    fn dereference_without_guard_still_flagged() {
        // member.property without prior assignment should still warn.
        let source = "\
var target: Node2D

func _ready():
\tsetup()
\ttarget = get_node(\"T\")

func setup():
\ttarget.visible = true
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("target"));
    }

    #[test]
    fn subscript_dereference_flagged() {
        // member[index] is a dereference — should be flagged.
        let source = "\
var _items: Array

func _ready():
\tuse_items()
\t_items = []

func use_items():
\t_items[0] = 1
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("_items"));
    }

    #[test]
    fn deep_transitive_chain() {
        // _ready → _build_ui → _build_panels → assigns _panel
        let source = "\
extends Control

var _panel: HBoxContainer

func _ready():
\t_build_ui()

func _build_ui():
\t_build_panels()

func _build_panels():
\t_panel = HBoxContainer.new()

func _update():
\t_panel.visible = true
";
        assert!(check(source).is_empty());
    }
}
