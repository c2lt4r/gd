use crate::core::gd_ast::{self, GdDecl, GdFile, GdStmt};

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct DuplicateCode;

impl LintRule for DuplicateCode {
    fn name(&self) -> &'static str {
        "duplicate-code"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Maintenance
    }

    fn default_enabled(&self) -> bool {
        false
    }

    fn check(&self, file: &GdFile<'_>, source: &str, config: &LintConfig) -> Vec<LintDiagnostic> {
        let rule_config = config.rules.get("duplicate-code");
        let min_statements = rule_config.and_then(|r| r.min_statements).unwrap_or(5);
        let threshold = rule_config
            .and_then(|r| r.similarity_threshold)
            .unwrap_or(80);

        let functions = collect_functions(file, source);
        find_duplicates(&functions, min_statements, threshold)
    }
}

/// A function with its structural fingerprint.
struct FunctionInfo<'a> {
    name: &'a str,
    line: usize,
    fingerprint: Vec<&'static str>,
}

/// Collect all functions in the file with their normalized fingerprints.
fn collect_functions<'a>(file: &GdFile<'a>, _source: &'a str) -> Vec<FunctionInfo<'a>> {
    let mut functions = Vec::new();
    gd_ast::visit_decls(file, &mut |decl| {
        if let GdDecl::Func(func) = decl {
            let mut fingerprint = Vec::new();
            normalize_stmts(&func.body, &mut fingerprint);
            functions.push(FunctionInfo {
                name: func.name,
                line: func.node.start_position().row,
                fingerprint,
            });
        }
    });
    functions
}

/// Walk typed AST statements and produce a canonical sequence of statement kinds
/// for structural fingerprinting. Control flow nodes push their kind then recurse
/// into their bodies; leaf statements just push their kind.
fn normalize_stmts(stmts: &[GdStmt], out: &mut Vec<&'static str>) {
    for stmt in stmts {
        match stmt {
            GdStmt::If(if_stmt) => {
                out.push("if_statement");
                normalize_stmts(&if_stmt.body, out);
                for (_, branch) in &if_stmt.elif_branches {
                    out.push("elif_branch");
                    normalize_stmts(branch, out);
                }
                if let Some(else_body) = &if_stmt.else_body {
                    out.push("else_branch");
                    normalize_stmts(else_body, out);
                }
            }
            GdStmt::For { body, .. } => {
                out.push("for_statement");
                normalize_stmts(body, out);
            }
            GdStmt::While { body, .. } => {
                out.push("while_statement");
                normalize_stmts(body, out);
            }
            GdStmt::Match { arms, .. } => {
                out.push("match_statement");
                for arm in arms {
                    out.push("pattern_section");
                    normalize_stmts(&arm.body, out);
                }
            }
            GdStmt::Return { .. } => out.push("return_statement"),
            GdStmt::Var(_) => out.push("variable_statement"),
            GdStmt::Expr { .. } => out.push("expression_statement"),
            GdStmt::Assign { .. } => out.push("assignment"),
            GdStmt::Pass { .. } => out.push("pass_statement"),
            GdStmt::Break { .. } => out.push("break_statement"),
            GdStmt::Continue { .. } => out.push("continue_statement"),
            _ => {}
        }
    }
}

/// Compute Levenshtein distance between two slices.
fn edit_distance(a: &[&str], b: &[&str]) -> usize {
    let m = a.len();
    let n = b.len();

    // Use single-row optimization
    let mut prev = (0..=n).collect::<Vec<_>>();
    let mut curr = vec![0; n + 1];

    for i in 1..=m {
        curr[0] = i;
        for j in 1..=n {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }

    prev[n]
}

/// Compute similarity percentage (0-100) between two fingerprints.
fn similarity(a: &[&str], b: &[&str]) -> u8 {
    let max_len = a.len().max(b.len());
    if max_len == 0 {
        return 100;
    }
    let dist = edit_distance(a, b);
    let sim = 1.0 - (dist as f64 / max_len as f64);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let pct = (sim * 100.0).round() as u8;
    pct
}

/// Union-Find for grouping similar functions.
struct UnionFind {
    parent: Vec<usize>,
    rank: Vec<usize>,
}

impl UnionFind {
    fn new(n: usize) -> Self {
        Self {
            parent: (0..n).collect(),
            rank: vec![0; n],
        }
    }

    fn find(&mut self, x: usize) -> usize {
        if self.parent[x] != x {
            self.parent[x] = self.find(self.parent[x]);
        }
        self.parent[x]
    }

    fn union(&mut self, x: usize, y: usize) {
        let rx = self.find(x);
        let ry = self.find(y);
        if rx == ry {
            return;
        }
        match self.rank[rx].cmp(&self.rank[ry]) {
            std::cmp::Ordering::Less => self.parent[rx] = ry,
            std::cmp::Ordering::Greater => self.parent[ry] = rx,
            std::cmp::Ordering::Equal => {
                self.parent[ry] = rx;
                self.rank[rx] += 1;
            }
        }
    }
}

/// A group of structurally similar functions.
struct DuplicateGroup {
    /// Indices into the functions array.
    members: Vec<usize>,
    /// Min and max similarity within the group.
    min_similarity: u8,
    max_similarity: u8,
}

/// Find groups of duplicate/similar functions.
fn find_duplicates(
    functions: &[FunctionInfo],
    min_statements: usize,
    threshold: usize,
) -> Vec<LintDiagnostic> {
    let n = functions.len();
    if n < 2 {
        return Vec::new();
    }

    let threshold_u8 = u8::try_from(threshold.min(100)).unwrap_or(100);
    let (uf, pair_sims) = compute_pairs(functions, n, min_statements, threshold_u8);
    let groups = build_groups(functions, n, min_statements, uf, &pair_sims);
    emit_diagnostics(functions, &groups)
}

/// Pairwise comparison of function fingerprints, returning union-find and similarity pairs.
fn compute_pairs(
    functions: &[FunctionInfo],
    n: usize,
    min_statements: usize,
    threshold: u8,
) -> (UnionFind, Vec<(usize, u8)>) {
    let mut uf = UnionFind::new(n);
    let mut pair_sims: Vec<(usize, u8)> = Vec::new();

    for (i, fi) in functions.iter().enumerate() {
        if fi.fingerprint.len() < min_statements {
            continue;
        }

        for (j, fj) in functions.iter().enumerate().skip(i + 1) {
            if fj.fingerprint.len() < min_statements {
                continue;
            }

            // Quick rejection: statement count differs by more than 30%
            let max_len = fi.fingerprint.len().max(fj.fingerprint.len());
            let min_len = fi.fingerprint.len().min(fj.fingerprint.len());
            if max_len > 0 && (max_len - min_len) * 100 / max_len > 30 {
                continue;
            }

            let sim = similarity(&fi.fingerprint, &fj.fingerprint);
            if sim >= threshold {
                uf.union(i, j);
                pair_sims.push((i, sim));
            }
        }
    }

    (uf, pair_sims)
}

/// Cluster similar functions into groups using union-find results.
fn build_groups(
    functions: &[FunctionInfo],
    n: usize,
    min_statements: usize,
    mut uf: UnionFind,
    pair_sims: &[(usize, u8)],
) -> Vec<DuplicateGroup> {
    let mut groups_map: std::collections::HashMap<usize, DuplicateGroup> =
        std::collections::HashMap::new();

    for &(i, sim) in pair_sims {
        let root = uf.find(i);
        let group = groups_map.entry(root).or_insert_with(|| DuplicateGroup {
            members: Vec::new(),
            min_similarity: sim,
            max_similarity: sim,
        });
        group.min_similarity = group.min_similarity.min(sim);
        group.max_similarity = group.max_similarity.max(sim);
    }

    for (i, fi) in functions.iter().enumerate().take(n) {
        if fi.fingerprint.len() < min_statements {
            continue;
        }
        let root = uf.find(i);
        if let Some(group) = groups_map.get_mut(&root)
            && !group.members.contains(&i)
        {
            group.members.push(i);
        }
    }

    let mut groups: Vec<DuplicateGroup> = groups_map.into_values().collect();
    for group in &mut groups {
        group.members.sort_by_key(|&idx| functions[idx].line);
    }
    groups.sort_by_key(|g| functions[g.members[0]].line);
    groups
}

/// Convert duplicate groups into lint diagnostics.
fn emit_diagnostics(functions: &[FunctionInfo], groups: &[DuplicateGroup]) -> Vec<LintDiagnostic> {
    let mut diags = Vec::new();

    for group in groups {
        if group.members.len() < 2 {
            continue;
        }

        let first = &functions[group.members[0]];

        if group.members.len() == 2 {
            let second = &functions[group.members[1]];
            let sim = group.max_similarity;
            diags.push(LintDiagnostic {
                rule: "duplicate-code",
                message: format!(
                    "structurally similar functions ({sim}% similar): \
                     `{}` (line {}) and `{}` (line {}); \
                     consider extracting shared logic into a common function",
                    first.name,
                    first.line + 1,
                    second.name,
                    second.line + 1,
                ),
                severity: Severity::Warning,
                line: first.line,
                column: 0,
                end_column: None,
                fix: None,
                context_lines: None,
            });
        } else {
            let names: Vec<String> = group
                .members
                .iter()
                .map(|&idx| {
                    let f = &functions[idx];
                    format!("`{}` (line {})", f.name, f.line + 1)
                })
                .collect();
            let sim_range = if group.min_similarity == group.max_similarity {
                format!("{}%", group.max_similarity)
            } else {
                format!("{}-{}%", group.min_similarity, group.max_similarity)
            };
            diags.push(LintDiagnostic {
                rule: "duplicate-code",
                message: format!(
                    "{} structurally similar functions ({sim_range} similar): {}; \
                     consider extracting shared logic into a common function",
                    group.members.len(),
                    names.join(", "),
                ),
                severity: Severity::Warning,
                line: first.line,
                column: 0,
                end_column: None,
                fix: None,
                context_lines: None,
            });
        }
    }

    diags
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
        DuplicateCode.check(&file, source, &config)
    }

    fn check_with_config(source: &str, min_stmts: usize, threshold: usize) -> Vec<LintDiagnostic> {
        let tree = parser::parse(source).unwrap();
        let file = gd_ast::convert(&tree, source);
        let mut config = LintConfig::default();
        let rule_config = crate::core::config::RuleConfig {
            min_statements: Some(min_stmts),
            similarity_threshold: Some(threshold),
            ..Default::default()
        };
        config
            .rules
            .insert("duplicate-code".to_string(), rule_config);
        DuplicateCode.check(&file, source, &config)
    }

    #[test]
    fn exact_duplicate_detected() {
        let source = "\
func _on_attack_pressed():
\tif not can_attack:
\t\treturn
\tvar target = get_nearest_enemy()
\tif target == null:
\t\treturn
\tattack_target(target)
\tcooldown_timer.start()

func _on_special_pressed():
\tif not can_special:
\t\treturn
\tvar enemy = find_closest_foe()
\tif enemy == null:
\t\treturn
\tspecial_attack(enemy)
\tspecial_timer.start()
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule, "duplicate-code");
        assert!(diags[0].message.contains("_on_attack_pressed"));
        assert!(diags[0].message.contains("_on_special_pressed"));
        assert!(diags[0].message.contains("100%"));
    }

    #[test]
    fn similar_functions_above_threshold() {
        // Same structure but one has an extra statement — should still be above 80%
        let source = "\
func handle_attack():
\tif not can_act:
\t\treturn
\tvar target = get_target()
\tif target == null:
\t\treturn
\tdo_attack(target)
\ttimer.start()

func handle_defend():
\tif not can_act:
\t\treturn
\tvar target = get_target()
\tif target == null:
\t\treturn
\tdo_defend(target)
\ttimer.start()
\tshow_shield()
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("similar"));
    }

    #[test]
    fn dissimilar_functions_not_flagged() {
        let source = "\
func setup_player():
\tvar health = 100
\tvar armor = 50
\tvar speed = 10.0
\tvar name = \"Player\"
\tvar level = 1

func process_input(event):
\tif event.is_action_pressed(\"jump\"):
\t\tjump()
\tif event.is_action_pressed(\"attack\"):
\t\tattack()
\tif event.is_action_pressed(\"defend\"):
\t\tdefend()
\tif event.is_action_pressed(\"run\"):
\t\trun()
\tif event.is_action_pressed(\"crouch\"):
\t\tcrouch()
";
        let diags = check(source);
        assert!(diags.is_empty());
    }

    #[test]
    fn below_min_statements_skipped() {
        // 3-statement functions with default min_statements=5
        let source = "\
func foo():
\tvar a = 1
\tvar b = 2
\tvar c = 3

func bar():
\tvar x = 1
\tvar y = 2
\tvar z = 3
";
        let diags = check(source);
        assert!(diags.is_empty());
    }

    #[test]
    fn threshold_boundary() {
        // With min_statements=5 and threshold=100, only exact matches should fire
        let source = "\
func alpha():
\tvar a = 1
\tvar b = 2
\tvar c = 3
\tvar d = 4
\tvar e = 5

func beta():
\tvar x = 1
\tvar y = 2
\tvar z = 3
\tvar w = 4
\tvar v = 5
";
        // Exact structural match at threshold=100 → flagged
        let diags = check_with_config(source, 5, 100);
        assert_eq!(diags.len(), 1);

        // Now make them differ by one statement type
        let source2 = "\
func alpha():
\tvar a = 1
\tvar b = 2
\tvar c = 3
\tvar d = 4
\tvar e = 5

func beta():
\tvar x = 1
\tvar y = 2
\tvar z = 3
\tvar w = 4
\treturn
";
        // 4/5 match = 80% — at threshold=81 should NOT be flagged
        let diags = check_with_config(source2, 5, 81);
        assert!(diags.is_empty());

        // At threshold=80 should be flagged
        let diags = check_with_config(source2, 5, 80);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn three_similar_functions_grouped() {
        let source = "\
func handle_a():
\tif not ready:
\t\treturn
\tvar t = get_target()
\tif t == null:
\t\treturn
\tdo_a(t)
\ttimer.start()

func handle_b():
\tif not ready:
\t\treturn
\tvar t = get_target()
\tif t == null:
\t\treturn
\tdo_b(t)
\ttimer.start()

func handle_c():
\tif not ready:
\t\treturn
\tvar t = get_target()
\tif t == null:
\t\treturn
\tdo_c(t)
\ttimer.start()
";
        let diags = check(source);
        // Should be exactly one diagnostic grouping all three
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains('3'));
        assert!(diags[0].message.contains("handle_a"));
        assert!(diags[0].message.contains("handle_b"));
        assert!(diags[0].message.contains("handle_c"));
    }

    #[test]
    fn short_functions_ignored() {
        // 1-2 statement functions (getters/setters) should never trigger
        let source = "\
func get_health():
\treturn health

func get_armor():
\treturn armor

func get_speed():
\treturn speed

func get_name():
\treturn name

func get_level():
\treturn level
";
        let diags = check(source);
        assert!(diags.is_empty());
    }

    #[test]
    fn nested_control_flow_compared() {
        // Same nesting structure → similar
        let source = "\
func process_a(items):
\tfor item in items:
\t\tif item.valid:
\t\t\titem.process()
\t\t\titem.update()
\t\t\titem.save()
\tlog_done()
\tcleanup()

func process_b(entries):
\tfor entry in entries:
\t\tif entry.active:
\t\t\tentry.compute()
\t\t\tentry.refresh()
\t\t\tentry.persist()
\tlog_done()
\tcleanup()
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);

        // Different nesting → not similar
        let source2 = "\
func flat_process(items):
\tvar a = get_a()
\tvar b = get_b()
\tvar c = get_c()
\tvar d = get_d()
\tvar e = get_e()

func nested_process(items):
\tfor item in items:
\t\tif item.valid:
\t\t\tfor sub in item.children:
\t\t\t\tif sub.active:
\t\t\t\t\tsub.run()
";
        let diags = check_with_config(source2, 5, 80);
        assert!(diags.is_empty());
    }

    #[test]
    fn single_function_file() {
        let source = "\
func only_one():
\tvar a = 1
\tvar b = 2
\tvar c = 3
\tvar d = 4
\tvar e = 5
";
        let diags = check(source);
        assert!(diags.is_empty());
    }

    #[test]
    fn config_overrides() {
        // Two 3-statement identical functions — not flagged at default min_statements=5
        let source = "\
func foo():
\tvar a = 1
\tvar b = 2
\tvar c = 3

func bar():
\tvar x = 1
\tvar y = 2
\tvar z = 3
";
        let diags = check(source);
        assert!(diags.is_empty());

        // Lower min_statements to 3 → now flagged
        let diags = check_with_config(source, 3, 80);
        assert_eq!(diags.len(), 1);

        // Raise threshold to 100 with slightly different functions
        let source2 = "\
func foo():
\tvar a = 1
\tvar b = 2
\tvar c = 3

func bar():
\tvar x = 1
\tvar y = 2
\treturn
";
        let diags = check_with_config(source2, 3, 100);
        assert!(diags.is_empty());
    }
}
