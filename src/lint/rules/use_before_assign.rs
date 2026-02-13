use std::collections::{HashMap, HashSet};
use tree_sitter::{Node, Tree};

use super::{LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct UseBeforeAssign;

impl LintRule for UseBeforeAssign {
    fn name(&self) -> &'static str {
        "use-before-assign"
    }

    fn default_enabled(&self) -> bool {
        false // opt-in — cross-function analysis, can have false positives
    }

    fn check(&self, tree: &Tree, source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let root = tree.root_node();

        let members = collect_member_vars(root, source);
        if members.is_empty() {
            return Vec::new();
        }

        let func_info = collect_function_info(root, source, &members);

        let mut diags = Vec::new();
        check_functions(root, source, &members, &func_info, &mut diags);
        diags
    }
}

/// Collect class-level member variable names that have no initializer or `= null`.
fn collect_member_vars(root: Node, source: &str) -> HashSet<String> {
    let mut members = HashSet::new();
    let mut cursor = root.walk();
    if !cursor.goto_first_child() {
        return members;
    }
    loop {
        let node = cursor.node();
        if node.kind() == "variable_statement" {
            let text = &source[node.start_byte()..].trim_start();
            if text.starts_with("const") {
                if !cursor.goto_next_sibling() {
                    break;
                }
                continue;
            }

            if let Some(name_node) = node.child_by_field_name("name")
                && let Ok(name) = name_node.utf8_text(source.as_bytes())
            {
                let has_non_null_init = node
                    .child_by_field_name("value")
                    .is_some_and(|v| v.kind() != "null");
                if !has_non_null_init {
                    members.insert(name.to_string());
                }
            }
        }
        if !cursor.goto_next_sibling() {
            break;
        }
    }
    members
}

struct FuncInfo {
    reads_before_assign: HashSet<String>,
}

fn collect_function_info(
    root: Node,
    source: &str,
    members: &HashSet<String>,
) -> HashMap<String, FuncInfo> {
    let mut info = HashMap::new();
    let mut cursor = root.walk();
    if !cursor.goto_first_child() {
        return info;
    }
    loop {
        let node = cursor.node();
        if node.kind() == "function_definition"
            && let Some(name_node) = node.child_by_field_name("name")
            && let Ok(func_name) = name_node.utf8_text(source.as_bytes())
            && let Some(body) = node.child_by_field_name("body")
        {
            let mut assigned = HashSet::new();
            let mut reads = HashSet::new();
            scan_body_for_member_access(body, source, members, &mut assigned, &mut reads);
            info.insert(
                func_name.to_string(),
                FuncInfo {
                    reads_before_assign: reads,
                },
            );
        }
        if !cursor.goto_next_sibling() {
            break;
        }
    }
    info
}

fn scan_body_for_member_access(
    body: Node,
    source: &str,
    members: &HashSet<String>,
    assigned: &mut HashSet<String>,
    reads_before_assign: &mut HashSet<String>,
) {
    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        scan_statement(child, source, members, assigned, reads_before_assign);
    }
}

fn scan_statement(
    node: Node,
    source: &str,
    members: &HashSet<String>,
    assigned: &mut HashSet<String>,
    reads_before_assign: &mut HashSet<String>,
) {
    if node.kind() == "expression_statement" {
        let mut c = node.walk();
        for child in node.children(&mut c) {
            if child.kind() == "assignment"
                && let Some(member) = extract_member_assign(&child, source, members)
            {
                if let Some(rhs) = child.named_child(1) {
                    collect_member_reads(rhs, source, members, assigned, reads_before_assign);
                }
                assigned.insert(member);
                return;
            }
        }
    }

    collect_member_reads(node, source, members, assigned, reads_before_assign);

    match node.kind() {
        "if_statement" | "for_statement" | "while_statement" | "match_statement" => {
            let mut c = node.walk();
            for child in node.children(&mut c) {
                if child.kind() == "body" || child.kind() == "match_body" {
                    scan_body_for_member_access(
                        child,
                        source,
                        members,
                        assigned,
                        reads_before_assign,
                    );
                }
                if (child.kind() == "elif_branch" || child.kind() == "else_branch")
                    && let Some(b) = child.child_by_field_name("body")
                {
                    scan_body_for_member_access(b, source, members, assigned, reads_before_assign);
                }
            }
        }
        _ => {}
    }
}

fn extract_member_assign(node: &Node, source: &str, members: &HashSet<String>) -> Option<String> {
    let lhs = node.named_child(0)?;
    match lhs.kind() {
        "identifier" => {
            let name = lhs.utf8_text(source.as_bytes()).ok()?;
            if members.contains(name) {
                Some(name.to_string())
            } else {
                None
            }
        }
        "attribute" => {
            let first = lhs.named_child(0)?;
            if first.kind() != "identifier" || first.utf8_text(source.as_bytes()).ok()? != "self" {
                return None;
            }
            let mut c = lhs.walk();
            for child in lhs.children(&mut c) {
                if child.kind() == "identifier" && child != first {
                    let name = child.utf8_text(source.as_bytes()).ok()?;
                    if members.contains(name) {
                        return Some(name.to_string());
                    }
                }
            }
            None
        }
        _ => None,
    }
}

fn collect_member_reads(
    node: Node,
    source: &str,
    members: &HashSet<String>,
    assigned: &HashSet<String>,
    reads_before_assign: &mut HashSet<String>,
) {
    match node.kind() {
        "identifier" => {
            if let Ok(name) = node.utf8_text(source.as_bytes())
                && members.contains(name)
                && !assigned.contains(name)
            {
                reads_before_assign.insert(name.to_string());
            }
        }
        "attribute" => {
            if let Some(first) = node.named_child(0)
                && first.kind() == "identifier"
                && first.utf8_text(source.as_bytes()).ok() == Some("self")
            {
                let mut c = node.walk();
                for child in node.children(&mut c) {
                    if child.kind() == "identifier"
                        && child != first
                        && let Ok(name) = child.utf8_text(source.as_bytes())
                        && members.contains(name)
                        && !assigned.contains(name)
                    {
                        reads_before_assign.insert(name.to_string());
                    }
                }
                return;
            }
        }
        _ => {}
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            collect_member_reads(
                cursor.node(),
                source,
                members,
                assigned,
                reads_before_assign,
            );
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

fn check_functions(
    root: Node,
    source: &str,
    members: &HashSet<String>,
    func_info: &HashMap<String, FuncInfo>,
    diags: &mut Vec<LintDiagnostic>,
) {
    let mut cursor = root.walk();
    if !cursor.goto_first_child() {
        return;
    }
    loop {
        let node = cursor.node();
        if node.kind() == "function_definition"
            && let Some(name_node) = node.child_by_field_name("name")
            && let Ok(func_name) = name_node.utf8_text(source.as_bytes())
            && let Some(body) = node.child_by_field_name("body")
        {
            let mut assigned_so_far = HashSet::new();
            check_body_calls(
                body,
                source,
                members,
                func_info,
                func_name,
                &mut assigned_so_far,
                diags,
            );
        }
        if !cursor.goto_next_sibling() {
            break;
        }
    }
}

fn check_body_calls(
    body: Node,
    source: &str,
    members: &HashSet<String>,
    func_info: &HashMap<String, FuncInfo>,
    caller_name: &str,
    assigned_so_far: &mut HashSet<String>,
    diags: &mut Vec<LintDiagnostic>,
) {
    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        process_statement_for_calls(
            child,
            source,
            members,
            func_info,
            caller_name,
            assigned_so_far,
            diags,
        );
    }
}

fn process_statement_for_calls(
    node: Node,
    source: &str,
    members: &HashSet<String>,
    func_info: &HashMap<String, FuncInfo>,
    caller_name: &str,
    assigned_so_far: &mut HashSet<String>,
    diags: &mut Vec<LintDiagnostic>,
) {
    if node.kind() == "expression_statement" {
        let mut c = node.walk();
        for child in node.children(&mut c) {
            if child.kind() == "assignment"
                && let Some(member) = extract_member_assign(&child, source, members)
            {
                assigned_so_far.insert(member);
            }
        }
    }

    find_calls_in_node(node, source, func_info, caller_name, assigned_so_far, diags);

    match node.kind() {
        "if_statement" | "for_statement" | "while_statement" | "match_statement" => {
            let mut c = node.walk();
            for child in node.children(&mut c) {
                if child.kind() == "body" || child.kind() == "match_body" {
                    check_body_calls(
                        child,
                        source,
                        members,
                        func_info,
                        caller_name,
                        assigned_so_far,
                        diags,
                    );
                }
                if (child.kind() == "elif_branch" || child.kind() == "else_branch")
                    && let Some(b) = child.child_by_field_name("body")
                {
                    check_body_calls(
                        b,
                        source,
                        members,
                        func_info,
                        caller_name,
                        assigned_so_far,
                        diags,
                    );
                }
            }
        }
        _ => {}
    }
}

fn find_calls_in_node(
    node: Node,
    source: &str,
    func_info: &HashMap<String, FuncInfo>,
    caller_name: &str,
    assigned_so_far: &HashSet<String>,
    diags: &mut Vec<LintDiagnostic>,
) {
    if node.kind() == "call"
        && let Some(func_id) = node.named_child(0)
        && func_id.kind() == "identifier"
        && let Ok(callee) = func_id.utf8_text(source.as_bytes())
    {
        check_callee(
            callee,
            &node,
            func_info,
            caller_name,
            assigned_so_far,
            diags,
        );
    }

    if node.kind() == "attribute"
        && let Some(first) = node.named_child(0)
        && first.kind() == "identifier"
        && first.utf8_text(source.as_bytes()).ok() == Some("self")
    {
        let mut c = node.walk();
        for child in node.children(&mut c) {
            if child.kind() == "attribute_call"
                && let Some(method_id) = child.named_child(0)
                && method_id.kind() == "identifier"
                && let Ok(callee) = method_id.utf8_text(source.as_bytes())
            {
                check_callee(
                    callee,
                    &node,
                    func_info,
                    caller_name,
                    assigned_so_far,
                    diags,
                );
            }
        }
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            find_calls_in_node(
                cursor.node(),
                source,
                func_info,
                caller_name,
                assigned_so_far,
                diags,
            );
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

fn check_callee(
    callee: &str,
    call_node: &Node,
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
    use crate::core::parser;

    fn check(source: &str) -> Vec<LintDiagnostic> {
        let tree = parser::parse(source).unwrap();
        let config = LintConfig::default();
        UseBeforeAssign.check(&tree, source, &config)
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
}
