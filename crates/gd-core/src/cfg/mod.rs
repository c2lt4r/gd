//! Control flow graph construction and analysis for GDScript functions.
//!
//! Builds a per-function CFG from the typed AST (`GdFunc.body`), enabling
//! flow-sensitive analysis: reachability, definite assignment, liveness.
//! Each function gets its own CFG with virtual ENTRY/EXIT blocks.
//!
//! Two build modes:
//! - [`FunctionCfg::build`] — for complete function bodies.
//! - [`FunctionCfg::build_body`] — for sub-bodies (if-body, loop body, etc.)
//!   where `break`/`continue` without an enclosing loop transfer control
//!   **out of** the analyzed scope rather than being malformed.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::gd_ast::{GdExpr, GdIf, GdMatchArm, GdStmt};

// ═══════════════════════════════════════════════════════════════════════
//  Types
// ═══════════════════════════════════════════════════════════════════════

/// Opaque index into [`FunctionCfg::blocks`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BlockId(u32);

impl BlockId {
    /// Numeric index for direct indexing into the blocks vec.
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// A basic block: straight-line statements ending with a control transfer.
#[derive(Debug)]
pub struct BasicBlock<'a> {
    pub id: BlockId,
    pub stmts: Vec<&'a GdStmt<'a>>,
    /// How control leaves this block. `None` only for the virtual EXIT block.
    pub terminator: Option<Terminator<'a>>,
}

/// How control leaves a basic block.
#[derive(Debug)]
pub enum Terminator<'a> {
    /// Unconditional jump (fallthrough, back-edge).
    Goto(BlockId),

    /// Conditional branch (`if`, `elif`, loop header).
    Branch {
        condition: &'a GdExpr<'a>,
        then_block: BlockId,
        else_block: BlockId,
    },

    /// Return from function (edge to EXIT).
    Return { value: Option<&'a GdExpr<'a>> },

    /// Multi-way dispatch (`match`).
    Match {
        value: &'a GdExpr<'a>,
        /// One block per match arm, in source order.
        arms: Vec<BlockId>,
        /// Reached when no arm matches. `None` when a wildcard `_` pattern
        /// makes the match exhaustive.
        fallthrough: Option<BlockId>,
    },

    /// Control transfers out of the analyzed body scope.
    ///
    /// Produced by `break`/`continue` when there is no enclosing loop in the
    /// CFG being built (i.e. in [`FunctionCfg::build_body`] mode where the
    /// enclosing loop lives in a parent scope).  Edges to EXIT — not
    /// considered fallthrough by [`FunctionCfg::can_fall_through`].
    BodyExit,
}

/// Control flow graph for a single function body or sub-body.
#[derive(Debug)]
pub struct FunctionCfg<'a> {
    /// Virtual entry block (always `BlockId(0)`, empty, Goto to first real
    /// block).
    pub entry: BlockId,
    /// Virtual exit block (always `BlockId(1)`, empty, no terminator).
    pub exit: BlockId,
    /// All blocks, indexable by [`BlockId`].
    pub blocks: Vec<BasicBlock<'a>>,
}

// ═══════════════════════════════════════════════════════════════════════
//  Builder
// ═══════════════════════════════════════════════════════════════════════

struct CfgBuilder<'a> {
    blocks: Vec<BasicBlock<'a>>,
    current: BlockId,
    loop_stack: Vec<LoopCtx>,
    /// When true, `break`/`continue` without an enclosing loop produce
    /// [`Terminator::BodyExit`] instead of being silently ignored.
    sub_body_mode: bool,
}

struct LoopCtx {
    header: BlockId,
    exit: BlockId,
}

impl<'a> CfgBuilder<'a> {
    fn new(sub_body_mode: bool) -> Self {
        let entry = BasicBlock {
            id: BlockId(0),
            stmts: vec![],
            terminator: None,
        };
        let exit = BasicBlock {
            id: BlockId(1),
            stmts: vec![],
            terminator: None,
        };
        let first = BasicBlock {
            id: BlockId(2),
            stmts: vec![],
            terminator: None,
        };
        Self {
            blocks: vec![entry, exit, first],
            current: BlockId(2),
            loop_stack: vec![],
            sub_body_mode,
        }
    }

    fn new_block(&mut self) -> BlockId {
        let id = BlockId(self.blocks.len() as u32);
        self.blocks.push(BasicBlock {
            id,
            stmts: vec![],
            terminator: None,
        });
        id
    }

    fn is_terminated(&self) -> bool {
        self.blocks[self.current.index()].terminator.is_some()
    }

    fn terminate(&mut self, term: Terminator<'a>) {
        self.blocks[self.current.index()].terminator = Some(term);
    }

    fn push_stmt(&mut self, stmt: &'a GdStmt<'a>) {
        self.blocks[self.current.index()].stmts.push(stmt);
    }

    // ── Body / statement lowering ────────────────────────────────────

    fn lower_body(&mut self, stmts: &'a [GdStmt<'a>]) {
        for stmt in stmts {
            if self.is_terminated() {
                self.current = self.new_block();
            }
            self.lower_stmt(stmt);
        }
    }

    fn lower_stmt(&mut self, stmt: &'a GdStmt<'a>) {
        match stmt {
            GdStmt::Expr { .. }
            | GdStmt::Var(_)
            | GdStmt::Assign { .. }
            | GdStmt::AugAssign { .. }
            | GdStmt::Pass { .. }
            | GdStmt::Breakpoint { .. }
            | GdStmt::Invalid { .. } => {
                self.push_stmt(stmt);
            }

            GdStmt::Return { value, .. } => {
                self.terminate(Terminator::Return {
                    value: value.as_ref(),
                });
            }

            GdStmt::Break { .. } => {
                if let Some(ctx) = self.loop_stack.last() {
                    self.terminate(Terminator::Goto(ctx.exit));
                } else if self.sub_body_mode {
                    self.terminate(Terminator::BodyExit);
                }
            }

            GdStmt::Continue { .. } => {
                if let Some(ctx) = self.loop_stack.last() {
                    self.terminate(Terminator::Goto(ctx.header));
                } else if self.sub_body_mode {
                    self.terminate(Terminator::BodyExit);
                }
            }

            GdStmt::If(gif) => self.lower_if(gif),

            GdStmt::While {
                condition, body, ..
            } => {
                self.lower_loop(condition, body);
            }

            GdStmt::For { iter, body, .. } => {
                self.lower_loop(iter, body);
            }

            GdStmt::Match { value, arms, .. } => {
                self.lower_match(value, arms);
            }
        }
    }

    // ── If / elif / else ─────────────────────────────────────────────

    fn lower_if(&mut self, gif: &'a GdIf<'a>) {
        let merge = self.new_block();

        let then_block = self.new_block();
        let else_target = if gif.elif_branches.is_empty() && gif.else_body.is_none() {
            merge
        } else {
            self.new_block()
        };

        self.terminate(Terminator::Branch {
            condition: &gif.condition,
            then_block,
            else_block: else_target,
        });

        self.current = then_block;
        self.lower_body(&gif.body);
        if !self.is_terminated() {
            self.terminate(Terminator::Goto(merge));
        }

        self.current = else_target;
        for (i, (cond, body)) in gif.elif_branches.iter().enumerate() {
            let elif_then = self.new_block();
            let elif_else = if i + 1 == gif.elif_branches.len() && gif.else_body.is_none() {
                merge
            } else {
                self.new_block()
            };

            self.terminate(Terminator::Branch {
                condition: cond,
                then_block: elif_then,
                else_block: elif_else,
            });

            self.current = elif_then;
            self.lower_body(body);
            if !self.is_terminated() {
                self.terminate(Terminator::Goto(merge));
            }

            self.current = elif_else;
        }

        if let Some(else_body) = &gif.else_body {
            self.lower_body(else_body);
            if !self.is_terminated() {
                self.terminate(Terminator::Goto(merge));
            }
        }

        self.current = merge;
    }

    // ── While / for loops ────────────────────────────────────────────

    fn lower_loop(&mut self, condition: &'a GdExpr<'a>, body: &'a [GdStmt<'a>]) {
        let header = self.new_block();
        let body_block = self.new_block();
        let exit = self.new_block();

        self.terminate(Terminator::Goto(header));

        self.current = header;
        self.terminate(Terminator::Branch {
            condition,
            then_block: body_block,
            else_block: exit,
        });

        self.current = body_block;
        self.loop_stack.push(LoopCtx { header, exit });
        self.lower_body(body);
        self.loop_stack.pop();
        if !self.is_terminated() {
            self.terminate(Terminator::Goto(header));
        }

        self.current = exit;
    }

    // ── Match ────────────────────────────────────────────────────────

    fn lower_match(&mut self, value: &'a GdExpr<'a>, arms: &'a [GdMatchArm<'a>]) {
        let merge = self.new_block();
        let match_block = self.current;

        let arm_ids: Vec<BlockId> = arms.iter().map(|_| self.new_block()).collect();

        let has_wildcard = arms.iter().any(|arm| {
            arm.patterns
                .iter()
                .any(|p| matches!(p, GdExpr::Ident { name: "_", .. }))
        });

        self.blocks[match_block.index()].terminator = Some(Terminator::Match {
            value,
            arms: arm_ids.clone(),
            fallthrough: if has_wildcard { None } else { Some(merge) },
        });

        for (i, arm) in arms.iter().enumerate() {
            self.current = arm_ids[i];
            self.lower_body(&arm.body);
            if !self.is_terminated() {
                self.terminate(Terminator::Goto(merge));
            }
        }

        self.current = merge;
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  Public API — construction
// ═══════════════════════════════════════════════════════════════════════

impl<'a> FunctionCfg<'a> {
    /// Build a CFG from a complete function body.
    ///
    /// `break`/`continue` without an enclosing loop are left unterminated
    /// (malformed code — GDScript compiler would reject this).
    #[allow(clippy::cast_possible_truncation)]
    pub fn build(body: &'a [GdStmt<'a>]) -> Self {
        Self::build_inner(body, false)
    }

    /// Build a CFG from a sub-body (if-body, match arm, etc.).
    ///
    /// `break`/`continue` without an enclosing loop produce
    /// [`Terminator::BodyExit`] — meaning control transfers out of this
    /// scope. This allows `can_fall_through()` to correctly report that a
    /// body ending in `break` does not fall through.
    #[allow(clippy::cast_possible_truncation)]
    pub fn build_body(body: &'a [GdStmt<'a>]) -> Self {
        Self::build_inner(body, true)
    }

    fn build_inner(body: &'a [GdStmt<'a>], sub_body_mode: bool) -> Self {
        let mut b = CfgBuilder::new(sub_body_mode);
        b.blocks[0].terminator = Some(Terminator::Goto(BlockId(2)));
        b.lower_body(body);
        if !b.is_terminated() {
            b.terminate(Terminator::Goto(BlockId(1)));
        }
        Self {
            entry: BlockId(0),
            exit: BlockId(1),
            blocks: b.blocks,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  Public API — queries
// ═══════════════════════════════════════════════════════════════════════

impl<'a> FunctionCfg<'a> {
    /// Look up a block by id.
    pub fn block(&self, id: BlockId) -> &BasicBlock<'a> {
        &self.blocks[id.index()]
    }

    /// Number of blocks (including ENTRY and EXIT).
    pub fn len(&self) -> usize {
        self.blocks.len()
    }

    /// Whether the CFG has no real blocks beyond ENTRY/EXIT.
    pub fn is_empty(&self) -> bool {
        self.blocks.len() <= 2
    }

    /// Successor block ids of the given block.
    pub fn successors(&self, id: BlockId) -> Vec<BlockId> {
        match &self.blocks[id.index()].terminator {
            None => vec![],
            Some(Terminator::Goto(target)) => vec![*target],
            Some(Terminator::Branch {
                then_block,
                else_block,
                ..
            }) => vec![*then_block, *else_block],
            Some(Terminator::Return { .. } | Terminator::BodyExit) => vec![self.exit],
            Some(Terminator::Match {
                arms, fallthrough, ..
            }) => {
                let mut succs = arms.clone();
                if let Some(ft) = fallthrough {
                    succs.push(*ft);
                }
                succs
            }
        }
    }

    /// Predecessor map: for each block, which blocks jump to it.
    pub fn predecessors(&self) -> HashMap<BlockId, Vec<BlockId>> {
        let mut preds: HashMap<BlockId, Vec<BlockId>> = HashMap::new();
        for block in &self.blocks {
            for succ in self.successors(block.id) {
                preds.entry(succ).or_default().push(block.id);
            }
        }
        preds
    }

    /// Set of blocks reachable from ENTRY via forward BFS.
    pub fn reachable_blocks(&self) -> HashSet<BlockId> {
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        queue.push_back(self.entry);
        visited.insert(self.entry);
        while let Some(id) = queue.pop_front() {
            for succ in self.successors(id) {
                if visited.insert(succ) {
                    queue.push_back(succ);
                }
            }
        }
        visited
    }

    /// True if some reachable path reaches the end of the body without an
    /// explicit `return`, `break`, `continue`, or `BodyExit`.
    ///
    /// Only `Goto(EXIT)` — the implicit fall-off-the-end — counts as
    /// fallthrough.  `Return` and `BodyExit` terminators do not.
    pub fn can_fall_through(&self) -> bool {
        let reachable = self.reachable_blocks();
        self.blocks.iter().any(|b| {
            reachable.contains(&b.id)
                && matches!(&b.terminator, Some(Terminator::Goto(t)) if *t == self.exit)
        })
    }

    /// True if every reachable `Return` terminator carries a value.
    pub fn all_paths_return_value(&self) -> bool {
        let reachable = self.reachable_blocks();
        self.blocks.iter().all(|b| {
            !reachable.contains(&b.id)
                || !matches!(&b.terminator, Some(Terminator::Return { value: None }))
        })
    }

    /// Blocks that contain statements but are unreachable from ENTRY.
    pub fn unreachable_blocks(&self) -> Vec<BlockId> {
        let reachable = self.reachable_blocks();
        self.blocks
            .iter()
            .filter(|b| !reachable.contains(&b.id) && !b.stmts.is_empty())
            .map(|b| b.id)
            .collect()
    }

    /// Reverse post-order of reachable blocks (ENTRY first).
    ///
    /// Optimal iteration order for forward dataflow — every block is visited
    /// after all its non-back-edge predecessors, minimising solver iterations.
    pub fn reverse_postorder(&self) -> Vec<BlockId> {
        let mut visited = HashSet::new();
        let mut postorder = Vec::new();
        self.dfs_postorder(self.entry, &mut visited, &mut postorder);
        postorder.reverse();
        postorder
    }

    /// Post-order of reachable blocks (EXIT-ward leaves first).
    ///
    /// Optimal iteration order for backward dataflow.
    pub fn postorder(&self) -> Vec<BlockId> {
        let mut visited = HashSet::new();
        let mut order = Vec::new();
        self.dfs_postorder(self.entry, &mut visited, &mut order);
        order
    }

    fn dfs_postorder(&self, id: BlockId, visited: &mut HashSet<BlockId>, order: &mut Vec<BlockId>) {
        if !visited.insert(id) {
            return;
        }
        for succ in self.successors(id) {
            self.dfs_postorder(succ, visited, order);
        }
        order.push(id);
    }
}

pub mod dataflow;

// ═══════════════════════════════════════════════════════════════════════
//  Display
// ═══════════════════════════════════════════════════════════════════════

impl std::fmt::Display for BlockId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "B{}", self.0)
    }
}

impl std::fmt::Display for FunctionCfg<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for block in &self.blocks {
            if block.id == self.entry {
                write!(f, "ENTRY")?;
            } else if block.id == self.exit {
                write!(f, "EXIT")?;
            } else {
                write!(f, "{}", block.id)?;
            }

            if block.stmts.is_empty() {
                writeln!(f, ": (empty)")?;
            } else {
                writeln!(f, ":")?;
                for stmt in &block.stmts {
                    writeln!(f, "  [{}]", stmt_kind_label(stmt))?;
                }
            }

            for succ in self.successors(block.id) {
                let label = if succ == self.exit {
                    "EXIT".to_string()
                } else if succ == self.entry {
                    "ENTRY".to_string()
                } else {
                    format!("{succ}")
                };
                writeln!(f, "  -> {label}")?;
            }
        }
        Ok(())
    }
}

fn stmt_kind_label(stmt: &GdStmt<'_>) -> &'static str {
    match stmt {
        GdStmt::Expr { .. } => "Expr",
        GdStmt::Var(_) => "Var",
        GdStmt::Assign { .. } => "Assign",
        GdStmt::AugAssign { .. } => "AugAssign",
        GdStmt::Return { .. } => "Return",
        GdStmt::If(_) => "If",
        GdStmt::For { .. } => "For",
        GdStmt::While { .. } => "While",
        GdStmt::Match { .. } => "Match",
        GdStmt::Pass { .. } => "Pass",
        GdStmt::Break { .. } => "Break",
        GdStmt::Continue { .. } => "Continue",
        GdStmt::Breakpoint { .. } => "Breakpoint",
        GdStmt::Invalid { .. } => "Invalid",
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  Tests
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gd_ast;
    use crate::parser;

    fn with_cfg(source: &str, f: impl FnOnce(&FunctionCfg<'_>)) {
        let tree = parser::parse(source).unwrap();
        let file = gd_ast::convert(&tree, source);
        let func = file.funcs().next().expect("no function found");
        let cfg = FunctionCfg::build(&func.body);
        f(&cfg);
    }

    /// Build a sub-body CFG from a single function's body statements.
    fn with_body_cfg(source: &str, f: impl FnOnce(&FunctionCfg<'_>)) {
        let tree = parser::parse(source).unwrap();
        let file = gd_ast::convert(&tree, source);
        let func = file.funcs().next().expect("no function found");
        let cfg = FunctionCfg::build_body(&func.body);
        f(&cfg);
    }

    // ── Sequential ───────────────────────────────────────────────────

    #[test]
    fn sequential_pass() {
        with_cfg("func foo():\n\tpass\n", |cfg| {
            assert_eq!(cfg.len(), 3);
            assert!(cfg.can_fall_through());
            assert!(cfg.unreachable_blocks().is_empty());
        });
    }

    #[test]
    fn sequential_statements() {
        with_cfg(
            "func foo():\n\tvar x = 1\n\tx = x + 1\n\tprint(x)\n",
            |cfg| {
                assert_eq!(cfg.len(), 3);
                assert_eq!(cfg.block(BlockId(2)).stmts.len(), 3);
                assert!(cfg.can_fall_through());
            },
        );
    }

    // ── Return ───────────────────────────────────────────────────────

    #[test]
    fn explicit_return() {
        with_cfg("func foo():\n\treturn 42\n", |cfg| {
            assert!(!cfg.can_fall_through());
            assert!(cfg.all_paths_return_value());
        });
    }

    #[test]
    fn return_no_value() {
        with_cfg("func foo():\n\treturn\n", |cfg| {
            assert!(!cfg.can_fall_through());
            assert!(!cfg.all_paths_return_value());
        });
    }

    #[test]
    fn dead_code_after_return() {
        with_cfg("func foo():\n\treturn 1\n\tprint(\"dead\")\n", |cfg| {
            assert!(!cfg.can_fall_through());
            let dead = cfg.unreachable_blocks();
            assert_eq!(dead.len(), 1);
            assert_eq!(cfg.block(dead[0]).stmts.len(), 1);
        });
    }

    // ── If / elif / else ─────────────────────────────────────────────

    #[test]
    fn if_without_else_can_fall_through() {
        with_cfg("func foo(x):\n\tif x:\n\t\treturn 1\n", |cfg| {
            assert!(cfg.can_fall_through());
            assert!(cfg.unreachable_blocks().is_empty());
        });
    }

    #[test]
    fn if_else_both_return() {
        let src = "func foo(x) -> int:\n\tif x:\n\t\treturn 1\n\telse:\n\t\treturn 2\n\tprint(\"dead\")\n";
        with_cfg(src, |cfg| {
            assert!(!cfg.can_fall_through());
            assert_eq!(cfg.unreachable_blocks().len(), 1);
        });
    }

    #[test]
    fn if_elif_else_all_return() {
        let src = "func foo(x: int) -> int:\n\tif x > 0:\n\t\treturn x\n\telif x == 0:\n\t\treturn 0\n\telse:\n\t\treturn -x\n\tprint(\"dead\")\n";
        with_cfg(src, |cfg| {
            assert_eq!(cfg.len(), 8);
            assert!(!cfg.can_fall_through());
            assert!(cfg.all_paths_return_value());
            assert_eq!(cfg.unreachable_blocks().len(), 1);
        });
    }

    #[test]
    fn if_elif_no_else() {
        let src = "func foo(x: int):\n\tif x > 0:\n\t\tprint(\"pos\")\n\telif x == 0:\n\t\tprint(\"zero\")\n\tprint(\"done\")\n";
        with_cfg(src, |cfg| {
            assert!(cfg.can_fall_through());
            assert!(cfg.unreachable_blocks().is_empty());
        });
    }

    // ── While loops ──────────────────────────────────────────────────

    #[test]
    fn while_loop_basic() {
        let src = "func foo():\n\tvar i = 0\n\twhile i < 10:\n\t\ti += 1\n";
        with_cfg(src, |cfg| {
            assert!(cfg.can_fall_through());
            assert!(cfg.unreachable_blocks().is_empty());
            let preds = cfg.predecessors();
            let header = BlockId(3);
            let header_preds = preds.get(&header).unwrap();
            assert_eq!(header_preds.len(), 2);
        });
    }

    #[test]
    fn while_true_with_break() {
        let src = "func foo():\n\twhile true:\n\t\tvar x = get_input()\n\t\tif x:\n\t\t\tbreak\n";
        with_cfg(src, |cfg| {
            assert!(cfg.can_fall_through());
        });
    }

    #[test]
    fn while_true_with_return() {
        let src = "func foo() -> int:\n\twhile true:\n\t\tvar x = get_input()\n\t\tif x:\n\t\t\treturn x\n";
        with_cfg(src, |cfg| {
            assert!(cfg.can_fall_through());
        });
    }

    // ── For loops ────────────────────────────────────────────────────

    #[test]
    fn for_loop_basic() {
        let src =
            "func foo():\n\tvar total = 0\n\tfor i in range(10):\n\t\ttotal += i\n\treturn total\n";
        with_cfg(src, |cfg| {
            assert_eq!(cfg.len(), 6);
            assert!(!cfg.can_fall_through());
        });
    }

    #[test]
    fn for_loop_with_break() {
        let src = "func bar(condition: bool):\n\tvar result = 0\n\tfor i in range(10):\n\t\tif condition:\n\t\t\tbreak\n\t\tresult += i\n\treturn result\n";
        with_cfg(src, |cfg| {
            assert!(!cfg.can_fall_through());
            assert!(cfg.unreachable_blocks().is_empty());
        });
    }

    #[test]
    fn for_loop_with_continue() {
        let src =
            "func foo():\n\tfor i in range(10):\n\t\tif i == 5:\n\t\t\tcontinue\n\t\tprint(i)\n";
        with_cfg(src, |cfg| {
            assert!(cfg.can_fall_through());
            assert!(cfg.unreachable_blocks().is_empty());
        });
    }

    // ── Nested loops ─────────────────────────────────────────────────

    #[test]
    fn nested_loops_break_targets_inner() {
        let src = "func foo():\n\tfor i in range(10):\n\t\tfor j in range(10):\n\t\t\tif i == j:\n\t\t\t\tcontinue\n\t\t\tif i + j > 15:\n\t\t\t\tbreak\n\t\t\tprint(i)\n";
        with_cfg(src, |cfg| {
            assert_eq!(cfg.len(), 13);
            assert!(cfg.can_fall_through());
            assert!(cfg.unreachable_blocks().is_empty());
        });
    }

    // ── Match ────────────────────────────────────────────────────────

    #[test]
    fn match_all_arms_return_with_wildcard() {
        let src = "func foo(val) -> String:\n\tmatch val:\n\t\t1:\n\t\t\treturn \"one\"\n\t\t2:\n\t\t\treturn \"two\"\n\t\t_:\n\t\t\treturn \"other\"\n\tprint(\"dead\")\n";
        with_cfg(src, |cfg| {
            assert!(!cfg.can_fall_through());
            assert!(cfg.all_paths_return_value());
            assert_eq!(cfg.unreachable_blocks().len(), 1);
        });
    }

    #[test]
    fn match_no_wildcard_can_fall_through() {
        let src = "func foo(val: int):\n\tmatch val:\n\t\t1:\n\t\t\tprint(\"one\")\n\t\t2:\n\t\t\tprint(\"two\")\n\tprint(\"after\")\n";
        with_cfg(src, |cfg| {
            assert!(cfg.can_fall_through());
            assert!(cfg.unreachable_blocks().is_empty());
        });
    }

    #[test]
    fn match_arms_merge() {
        let src = "func foo(val):\n\tmatch val:\n\t\t1:\n\t\t\tprint(\"one\")\n\t\t_:\n\t\t\tprint(\"other\")\n\tprint(\"after\")\n";
        with_cfg(src, |cfg| {
            assert!(cfg.can_fall_through());
            let preds = cfg.predecessors();
            let merge_preds = preds.get(&BlockId(3)).unwrap();
            assert!(merge_preds.len() >= 2);
        });
    }

    // ── Early return in branches ─────────────────────────────────────

    #[test]
    fn early_return_branches() {
        let src = "func foo(x, y) -> int:\n\tif x:\n\t\tif y:\n\t\t\treturn 1\n\t\treturn 2\n\treturn 3\n";
        with_cfg(src, |cfg| {
            assert!(!cfg.can_fall_through());
            assert!(cfg.all_paths_return_value());
            assert!(cfg.unreachable_blocks().is_empty());
        });
    }

    #[test]
    fn guard_early_return() {
        let src = "func foo():\n\tvar resource = open()\n\tif not resource:\n\t\treturn\n\tresource.use_it()\n\tresource.close()\n";
        with_cfg(src, |cfg| {
            assert!(cfg.can_fall_through());
            assert!(cfg.unreachable_blocks().is_empty());
        });
    }

    // ── Sub-body mode (build_body) ───────────────────────────────────

    #[test]
    fn body_with_return_does_not_fall_through() {
        with_body_cfg("func foo():\n\treturn 1\n", |cfg| {
            assert!(!cfg.can_fall_through());
        });
    }

    #[test]
    fn body_with_break_does_not_fall_through() {
        // break without enclosing loop → BodyExit in sub-body mode
        with_body_cfg("func foo():\n\tbreak\n", |cfg| {
            assert!(!cfg.can_fall_through());
        });
    }

    #[test]
    fn body_with_continue_does_not_fall_through() {
        with_body_cfg("func foo():\n\tcontinue\n", |cfg| {
            assert!(!cfg.can_fall_through());
        });
    }

    #[test]
    fn body_if_else_both_break_does_not_fall_through() {
        let src = "func foo(x):\n\tif x:\n\t\tbreak\n\telse:\n\t\tcontinue\n";
        with_body_cfg(src, |cfg| {
            assert!(!cfg.can_fall_through());
        });
    }

    #[test]
    fn body_if_without_else_falls_through() {
        let src = "func foo(x):\n\tif x:\n\t\tbreak\n\tprint(\"still here\")\n";
        with_body_cfg(src, |cfg| {
            assert!(cfg.can_fall_through());
        });
    }

    #[test]
    fn body_break_inside_loop_is_not_body_exit() {
        // break inside a for loop targets the loop exit — NOT BodyExit.
        // The sub-body can still fall through after the loop.
        let src = "func foo():\n\tfor i in range(10):\n\t\tbreak\n";
        with_body_cfg(src, |cfg| {
            assert!(cfg.can_fall_through());
        });
    }

    // ── Display / misc ───────────────────────────────────────────────

    #[test]
    fn display_simple() {
        with_cfg("func foo():\n\tpass\n", |cfg| {
            let output = cfg.to_string();
            assert!(output.contains("ENTRY"));
            assert!(output.contains("EXIT"));
            assert!(output.contains("[Pass]"));
        });
    }

    #[test]
    fn entry_has_no_predecessors() {
        with_cfg("func foo():\n\tpass\n", |cfg| {
            let preds = cfg.predecessors();
            assert!(!preds.contains_key(&cfg.entry));
        });
    }

    #[test]
    fn all_blocks_reachable_in_simple_function() {
        with_cfg("func foo():\n\tvar x = 1\n\treturn x\n", |cfg| {
            let reachable = cfg.reachable_blocks();
            assert_eq!(reachable.len(), cfg.len());
        });
    }
}
