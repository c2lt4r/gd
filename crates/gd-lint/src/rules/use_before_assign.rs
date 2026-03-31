use gd_core::cfg::dataflow::{self, DataflowAnalysis, Direction, Lattice};
use gd_core::cfg::{BasicBlock, FunctionCfg, Terminator};
use gd_core::gd_ast::{GdDecl, GdExpr, GdExtends, GdFile, GdStmt};
use std::collections::{HashMap, HashSet, VecDeque};

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use gd_core::config::LintConfig;

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

        // For Node subclasses, members assigned in _ready() or _init()
        // (directly or transitively) are guaranteed initialized before any
        // other user method runs.
        let ready_assigned = file
            .extends
            .and_then(|ext| match ext {
                GdExtends::Class(cls) if gd_class_db::inherits(cls, "Node") || cls == "Node" => {
                    Some(cls)
                }
                _ => None,
            })
            .map(|_| {
                let mut assigned = transitive_assigns("_ready", &func_info);
                assigned.extend(transitive_assigns("_init", &func_info));
                assigned
            })
            .unwrap_or_default();

        let mut diags = Vec::new();
        for decl in &file.declarations {
            if let GdDecl::Func(func) = decl {
                let initial = if func.name == "_ready" || func.name == "_init" {
                    HashSet::new()
                } else {
                    ready_assigned.clone()
                };
                check_call_sites(func, &members, &func_info, &initial, &mut diags);
            }
        }
        diags
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  Member variable collection
// ═══════════════════════════════════════════════════════════════════════

/// Collect class-level member variable names with no initializer or `= null`.
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

// ═══════════════════════════════════════════════════════════════════════
//  Dataflow: definitely-assigned members
// ═══════════════════════════════════════════════════════════════════════

#[derive(Clone, Debug, PartialEq, Eq)]
struct AssignedMembers(HashSet<String>);

impl Lattice for AssignedMembers {
    fn bottom() -> Self {
        Self(HashSet::new())
    }

    /// Must-analysis: a member is definitely assigned only if assigned on
    /// ALL incoming paths (intersection).
    fn join(&self, other: &Self) -> Self {
        Self(self.0.intersection(&other.0).cloned().collect())
    }
}

struct MemberAssignAnalysis<'a> {
    members: &'a HashSet<String>,
    initial: HashSet<String>,
}

impl DataflowAnalysis for MemberAssignAnalysis<'_> {
    type State = AssignedMembers;

    fn direction(&self) -> Direction {
        Direction::Forward
    }

    fn initial_state(&self) -> AssignedMembers {
        AssignedMembers(self.initial.clone())
    }

    fn transfer(
        &self,
        block: &BasicBlock<'_>,
        _terminator: Option<&Terminator<'_>>,
        state: &AssignedMembers,
    ) -> AssignedMembers {
        let mut out = state.clone();
        for stmt in &block.stmts {
            if let Some(member) = extract_member_assign(stmt, self.members) {
                out.0.insert(member);
            }
        }
        out
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  Per-function info collection (Level 1)
// ═══════════════════════════════════════════════════════════════════════

struct FuncInfo {
    /// Members dereferenced before being definitely assigned in this function.
    reads_before_assign: HashSet<String>,
    /// All members assigned anywhere in this function (flat — for transitive tracking).
    assigns: HashSet<String>,
    /// All local function names called in this function.
    calls: HashSet<String>,
}

fn collect_function_info(file: &GdFile, members: &HashSet<String>) -> HashMap<String, FuncInfo> {
    let mut info = HashMap::new();
    for decl in &file.declarations {
        if let GdDecl::Func(func) = decl {
            let cfg = FunctionCfg::build(&func.body);
            let result = dataflow::solve(
                &cfg,
                &MemberAssignAnalysis {
                    members,
                    initial: HashSet::new(),
                },
            );

            let mut reads = HashSet::new();
            let mut assigns = HashSet::new();
            let mut null_checked = HashSet::new();
            let mut calls = HashSet::new();

            let reachable = cfg.reachable_blocks();
            for block in &cfg.blocks {
                if !reachable.contains(&block.id) {
                    continue;
                }
                let mut state = result.entry(block.id).clone();
                for stmt in &block.stmts {
                    analyze_member_usage(stmt, members, &state.0, &mut reads, &mut null_checked);
                    collect_calls(stmt, &mut calls);
                    if let Some(member) = extract_member_assign(stmt, members) {
                        state.0.insert(member.clone());
                        assigns.insert(member);
                    }
                }
            }

            // Members that are null-checked (bare identifier reads) within the
            // function are assumed to be properly guarded before dereference.
            for m in &null_checked {
                reads.remove(m);
            }

            info.insert(
                func.name.to_string(),
                FuncInfo {
                    reads_before_assign: reads,
                    assigns,
                    calls,
                },
            );
        }
    }
    info
}

// ═══════════════════════════════════════════════════════════════════════
//  Call-site checking (Level 2)
// ═══════════════════════════════════════════════════════════════════════

fn check_call_sites(
    func: &gd_core::gd_ast::GdFunc<'_>,
    members: &HashSet<String>,
    func_info: &HashMap<String, FuncInfo>,
    initial_assigned: &HashSet<String>,
    diags: &mut Vec<LintDiagnostic>,
) {
    let cfg = FunctionCfg::build(&func.body);
    let result = dataflow::solve(
        &cfg,
        &MemberAssignAnalysis {
            members,
            initial: initial_assigned.clone(),
        },
    );

    let reachable = cfg.reachable_blocks();
    for block in &cfg.blocks {
        if !reachable.contains(&block.id) {
            continue;
        }
        let mut state = result.entry(block.id).clone();
        for stmt in &block.stmts {
            check_calls_in_stmt(stmt, &state.0, func_info, func.name, diags);
            if let Some(member) = extract_member_assign(stmt, members) {
                state.0.insert(member);
            }
        }
    }
}

/// Check all calls in a statement against the current assigned state.
fn check_calls_in_stmt(
    stmt: &GdStmt<'_>,
    assigned: &HashSet<String>,
    func_info: &HashMap<String, FuncInfo>,
    caller_name: &str,
    diags: &mut Vec<LintDiagnostic>,
) {
    visit_exprs_in_stmt(stmt, &mut |expr| {
        let (callee_name, call_node) = match expr {
            GdExpr::Call { callee, node, .. } => {
                if let GdExpr::Ident { name, .. } = callee.as_ref() {
                    (Some(*name), Some(node))
                } else {
                    (None, None)
                }
            }
            GdExpr::MethodCall {
                receiver,
                method,
                node,
                ..
            } => {
                if let GdExpr::Ident { name: "self", .. } = receiver.as_ref() {
                    (Some(*method), Some(node))
                } else {
                    (None, None)
                }
            }
            _ => (None, None),
        };

        if let Some(callee) = callee_name
            && let Some(node) = call_node
            && callee != caller_name
            && let Some(info) = func_info.get(callee)
        {
            for member in &info.reads_before_assign {
                if !assigned.contains(member) {
                    diags.push(LintDiagnostic {
                        rule: "use-before-assign",
                        message: format!(
                            "`{callee}()` accesses member `{member}` which may not be assigned yet at this call site"
                        ),
                        severity: Severity::Warning,
                        line: node.start_position().row,
                        column: node.start_position().column,
                        end_column: None,
                        fix: None,
                        context_lines: None,
                    });
                }
            }
        }
    });
}

// ═══════════════════════════════════════════════════════════════════════
//  Transitive assignment tracking
// ═══════════════════════════════════════════════════════════════════════

fn transitive_assigns(start: &str, func_info: &HashMap<String, FuncInfo>) -> HashSet<String> {
    let mut result = HashSet::new();
    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();

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

// ═══════════════════════════════════════════════════════════════════════
//  Expression helpers
// ═══════════════════════════════════════════════════════════════════════

/// Extract member name if this statement assigns to a tracked member.
fn extract_member_assign(stmt: &GdStmt, members: &HashSet<String>) -> Option<String> {
    if let GdStmt::Assign { target, .. } = stmt {
        match target {
            GdExpr::Ident { name, .. } if members.contains(*name) => {
                return Some((*name).to_string());
            }
            GdExpr::PropertyAccess {
                receiver, property, ..
            } if matches!(receiver.as_ref(), GdExpr::Ident { name: "self", .. })
                && members.contains(*property) =>
            {
                return Some((*property).to_string());
            }
            _ => {}
        }
    }
    None
}

/// Analyse member accesses in a statement, distinguishing dereferences
/// (member.x, member.method(), member[idx]) from bare identifier reads
/// (if member:, print(member)).
///
/// Context-aware: an identifier that is the receiver of a property access /
/// method call / subscript is a dereference, NOT a null-check.
fn analyze_member_usage(
    stmt: &GdStmt<'_>,
    members: &HashSet<String>,
    assigned: &HashSet<String>,
    reads: &mut HashSet<String>,
    null_checked: &mut HashSet<String>,
) {
    // Extract top-level expressions from the statement. analyze_expr handles
    // all recursion — do NOT also use visit_exprs_in_stmt, which would
    // double-recurse and lose the deref-vs-bare-ident context.
    match stmt {
        GdStmt::Expr { expr, .. } => {
            analyze_expr(expr, members, assigned, reads, null_checked);
        }
        GdStmt::Var(var) => {
            if let Some(value) = &var.value {
                analyze_expr(value, members, assigned, reads, null_checked);
            }
        }
        GdStmt::Assign { target, value, .. } | GdStmt::AugAssign { target, value, .. } => {
            analyze_expr(target, members, assigned, reads, null_checked);
            analyze_expr(value, members, assigned, reads, null_checked);
        }
        GdStmt::Return { value: Some(v), .. } => {
            analyze_expr(v, members, assigned, reads, null_checked);
        }
        _ => {}
    }
}

fn analyze_expr(
    expr: &GdExpr<'_>,
    members: &HashSet<String>,
    assigned: &HashSet<String>,
    reads: &mut HashSet<String>,
    null_checked: &mut HashSet<String>,
) {
    match expr {
        // Dereference: member.property, self.member
        GdExpr::PropertyAccess {
            receiver, property, ..
        } => {
            if let GdExpr::Ident { name: "self", .. } = receiver.as_ref() {
                // self.member — flag as dereference of member
                if members.contains(*property) && !assigned.contains(*property) {
                    reads.insert((*property).to_string());
                }
            } else if let GdExpr::Ident { name, .. } = receiver.as_ref() {
                // member.property — flag as dereference of member
                if members.contains(*name) && !assigned.contains(*name) {
                    reads.insert((*name).to_string());
                }
            } else {
                // Recurse into complex receiver (don't treat sub-idents as bare)
                analyze_expr(receiver, members, assigned, reads, null_checked);
            }
        }

        // Dereference: member.method()
        GdExpr::MethodCall { receiver, args, .. } => {
            if let GdExpr::Ident { name, .. } = receiver.as_ref() {
                if members.contains(*name) && !assigned.contains(*name) {
                    reads.insert((*name).to_string());
                }
            } else {
                analyze_expr(receiver, members, assigned, reads, null_checked);
            }
            for a in args {
                analyze_expr(a, members, assigned, reads, null_checked);
            }
        }

        // Dereference: member[index]
        GdExpr::Subscript {
            receiver, index, ..
        } => {
            if let GdExpr::Ident { name, .. } = receiver.as_ref() {
                if members.contains(*name) && !assigned.contains(*name) {
                    reads.insert((*name).to_string());
                }
            } else {
                analyze_expr(receiver, members, assigned, reads, null_checked);
            }
            analyze_expr(index, members, assigned, reads, null_checked);
        }

        // Bare identifier: null-check / safe use (not a dereference)
        GdExpr::Ident { name, .. } if members.contains(*name) && !assigned.contains(*name) => {
            null_checked.insert((*name).to_string());
        }

        // Recurse into sub-expressions for all other types
        GdExpr::Call { callee, args, .. } => {
            analyze_expr(callee, members, assigned, reads, null_checked);
            for a in args {
                analyze_expr(a, members, assigned, reads, null_checked);
            }
        }
        GdExpr::BinOp { left, right, .. } => {
            analyze_expr(left, members, assigned, reads, null_checked);
            analyze_expr(right, members, assigned, reads, null_checked);
        }
        GdExpr::UnaryOp { operand, .. } => {
            analyze_expr(operand, members, assigned, reads, null_checked);
        }
        GdExpr::Ternary {
            condition,
            true_val,
            false_val,
            ..
        } => {
            analyze_expr(condition, members, assigned, reads, null_checked);
            analyze_expr(true_val, members, assigned, reads, null_checked);
            analyze_expr(false_val, members, assigned, reads, null_checked);
        }
        GdExpr::Array { elements, .. } => {
            for e in elements {
                analyze_expr(e, members, assigned, reads, null_checked);
            }
        }
        GdExpr::Dict { pairs, .. } => {
            for (k, v) in pairs {
                analyze_expr(k, members, assigned, reads, null_checked);
                analyze_expr(v, members, assigned, reads, null_checked);
            }
        }
        GdExpr::Cast { expr: inner, .. }
        | GdExpr::Is { expr: inner, .. }
        | GdExpr::Await { expr: inner, .. } => {
            analyze_expr(inner, members, assigned, reads, null_checked);
        }
        GdExpr::SuperCall { args, .. } => {
            for a in args {
                analyze_expr(a, members, assigned, reads, null_checked);
            }
        }
        _ => {}
    }
}

/// Collect all local function calls (plain or self.method).
fn collect_calls(stmt: &GdStmt<'_>, calls: &mut HashSet<String>) {
    visit_exprs_in_stmt(stmt, &mut |expr| match expr {
        GdExpr::Call { callee, .. } => {
            if let GdExpr::Ident { name, .. } = callee.as_ref() {
                calls.insert((*name).to_string());
            }
        }
        GdExpr::MethodCall {
            receiver, method, ..
        } => {
            if let GdExpr::Ident { name: "self", .. } = receiver.as_ref() {
                calls.insert((*method).to_string());
            }
        }
        _ => {}
    });
}

// ═══════════════════════════════════════════════════════════════════════
//  Generic expression visitor (per-statement)
// ═══════════════════════════════════════════════════════════════════════

fn visit_exprs_in_stmt<'a>(stmt: &'a GdStmt<'a>, f: &mut impl FnMut(&'a GdExpr<'a>)) {
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
        GdStmt::Return { value: Some(v), .. } => visit_expr(v, f),
        _ => {}
    }
}

fn visit_expr<'a>(expr: &'a GdExpr<'a>, f: &mut impl FnMut(&'a GdExpr<'a>)) {
    f(expr);
    match expr {
        GdExpr::Call { callee, args, .. } => {
            visit_expr(callee, f);
            for a in args {
                visit_expr(a, f);
            }
        }
        GdExpr::MethodCall { receiver, args, .. } => {
            visit_expr(receiver, f);
            for a in args {
                visit_expr(a, f);
            }
        }
        GdExpr::PropertyAccess { receiver, .. } => visit_expr(receiver, f),
        GdExpr::Subscript {
            receiver, index, ..
        } => {
            visit_expr(receiver, f);
            visit_expr(index, f);
        }
        GdExpr::BinOp { left, right, .. } => {
            visit_expr(left, f);
            visit_expr(right, f);
        }
        GdExpr::UnaryOp { operand, .. } => visit_expr(operand, f),
        GdExpr::Ternary {
            condition,
            true_val,
            false_val,
            ..
        } => {
            visit_expr(condition, f);
            visit_expr(true_val, f);
            visit_expr(false_val, f);
        }
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
        GdExpr::Cast { expr: inner, .. }
        | GdExpr::Is { expr: inner, .. }
        | GdExpr::Await { expr: inner, .. } => visit_expr(inner, f),
        GdExpr::SuperCall { args, .. } => {
            for a in args {
                visit_expr(a, f);
            }
        }
        _ => {}
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  Tests
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use gd_core::gd_ast;
    use gd_core::parser;

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
    fn assignment_in_both_branches_counts() {
        // Member assigned on ALL paths → definitely assigned at merge.
        let source = "\
var target: Node2D

func _ready():
\tif true:
\t\ttarget = get_node(\"T\")
\telse:
\t\ttarget = get_node(\"U\")
\tsetup()

func setup():
\ttarget.visible = true
";
        assert!(check(source).is_empty());
    }

    #[test]
    fn assignment_in_one_branch_warns() {
        // Member assigned on only ONE path → not definitely assigned at merge.
        let source = "\
var target: Node2D

func _ready():
\tif true:
\t\ttarget = get_node(\"T\")
\tsetup()

func setup():
\ttarget.visible = true
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
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
        let diags = check(source);
        assert!(!diags.is_empty());
    }

    #[test]
    fn panelcontainer_ready_suppresses() {
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
