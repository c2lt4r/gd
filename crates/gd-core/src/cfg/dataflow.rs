//! Generic dataflow analysis framework over a [`FunctionCfg`].
//!
//! Provides a worklist-based fixpoint solver parameterised by a lattice and
//! transfer function.  Supports both forward and backward analyses.
//!
//! # Usage
//!
//! 1. Define a type implementing [`Lattice`] (your analysis state).
//! 2. Implement [`DataflowAnalysis`] with a transfer function.
//! 3. Call [`solve`] with a CFG and your analysis to get per-block states.

use std::collections::{HashMap, HashSet, VecDeque};

use super::{BasicBlock, BlockId, FunctionCfg, Terminator};

// ═══════════════════════════════════════════════════════════════════════
//  Traits
// ═══════════════════════════════════════════════════════════════════════

/// A join-semilattice for dataflow analysis.
///
/// `bottom()` is the initial value for blocks with no analysed
/// predecessors/successors yet.  `join()` merges values at confluence
/// points — the solver never folds from `bottom()`, it starts from the
/// first real predecessor/successor state instead.
///
/// - For "may" analyses (liveness, reaching defs): `bottom` = empty set,
///   `join` = union.
/// - For "must" analyses (definite assignment): `bottom` = empty set,
///   `join` = intersection.  Works correctly because the solver joins only
///   real predecessor states (never intersects with `bottom`).
pub trait Lattice: Clone + PartialEq {
    /// Initial value for blocks that have no analysed predecessors yet.
    fn bottom() -> Self;

    /// Merge two states at a confluence point.
    #[must_use]
    fn join(&self, other: &Self) -> Self;
}

/// Direction of dataflow propagation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Forward,
    Backward,
}

/// A dataflow analysis parameterised over a lattice.
///
/// Implement this trait, then pass it to [`solve`].
pub trait DataflowAnalysis {
    /// The lattice type representing analysis state at a program point.
    type State: Lattice;

    /// Propagation direction.
    fn direction(&self) -> Direction;

    /// Initial state for the entry block (forward) or exit block (backward).
    ///
    /// All other blocks start at `State::bottom()`.
    fn initial_state(&self) -> Self::State;

    /// Compute the output state given a block and its input state.
    ///
    /// For forward analyses: input = state at block entry, output = state at
    /// block exit.  For backward: input = state at block exit, output = state
    /// at block entry.
    fn transfer(
        &self,
        block: &BasicBlock<'_>,
        terminator: Option<&Terminator<'_>>,
        state: &Self::State,
    ) -> Self::State;
}

// ═══════════════════════════════════════════════════════════════════════
//  Result
// ═══════════════════════════════════════════════════════════════════════

/// Per-block entry and exit states after fixpoint convergence.
pub struct DataflowResult<S> {
    /// State at the beginning of each block (before its first statement).
    pub entry_states: HashMap<BlockId, S>,
    /// State at the end of each block (after its terminator).
    pub exit_states: HashMap<BlockId, S>,
}

impl<S: Clone> DataflowResult<S> {
    /// State at the beginning of a block.
    pub fn entry(&self, id: BlockId) -> &S {
        &self.entry_states[&id]
    }

    /// State at the end of a block.
    pub fn exit(&self, id: BlockId) -> &S {
        &self.exit_states[&id]
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  Solver
// ═══════════════════════════════════════════════════════════════════════

/// Run a dataflow analysis to fixpoint over a CFG.
///
/// Returns per-block entry/exit states.  The solver uses a worklist
/// seeded in reverse-post-order (forward) or post-order (backward) for
/// fast convergence.
pub fn solve<A: DataflowAnalysis>(cfg: &FunctionCfg<'_>, analysis: &A) -> DataflowResult<A::State> {
    match analysis.direction() {
        Direction::Forward => solve_forward(cfg, analysis),
        Direction::Backward => solve_backward(cfg, analysis),
    }
}

fn solve_forward<A: DataflowAnalysis>(
    cfg: &FunctionCfg<'_>,
    analysis: &A,
) -> DataflowResult<A::State> {
    let reachable: HashSet<BlockId> = cfg.reachable_blocks();
    let preds = cfg.predecessors();
    let rpo = cfg.reverse_postorder();

    let mut entry_states: HashMap<BlockId, A::State> = HashMap::new();
    let mut exit_states: HashMap<BlockId, A::State> = HashMap::new();

    // Initialise: entry block gets initial_state, everything else bottom.
    for &id in &rpo {
        if id == cfg.entry {
            entry_states.insert(id, analysis.initial_state());
        } else {
            entry_states.insert(id, A::State::bottom());
        }
        exit_states.insert(id, A::State::bottom());
    }

    // Compute initial exit states via transfer.
    for &id in &rpo {
        let block = cfg.block(id);
        let entry = &entry_states[&id];
        let exit = analysis.transfer(block, block.terminator.as_ref(), entry);
        exit_states.insert(id, exit);
    }

    // Worklist iteration.
    let mut worklist: VecDeque<BlockId> = rpo.into_iter().collect();
    let mut in_worklist: HashSet<BlockId> = worklist.iter().copied().collect();

    while let Some(id) = worklist.pop_front() {
        in_worklist.remove(&id);

        if !reachable.contains(&id) {
            continue;
        }

        // Join predecessor exit states → this block's entry.
        let new_entry = if id == cfg.entry {
            analysis.initial_state()
        } else {
            let pred_ids = preds.get(&id).map_or(&[][..], Vec::as_slice);
            let mut iter = pred_ids
                .iter()
                .filter(|p| reachable.contains(p))
                .map(|p| &exit_states[p]);
            match iter.next() {
                None => A::State::bottom(),
                Some(first) => iter.fold(first.clone(), |acc, s| acc.join(s)),
            }
        };

        // Transfer.
        let block = cfg.block(id);
        let new_exit = analysis.transfer(block, block.terminator.as_ref(), &new_entry);

        // Update entry unconditionally, enqueue successors if exit changed.
        entry_states.insert(id, new_entry);
        if new_exit == exit_states[&id] {
            continue;
        }
        exit_states.insert(id, new_exit);
        for succ in cfg.successors(id) {
            if reachable.contains(&succ) && !in_worklist.contains(&succ) {
                worklist.push_back(succ);
                in_worklist.insert(succ);
            }
        }
    }

    DataflowResult {
        entry_states,
        exit_states,
    }
}

fn solve_backward<A: DataflowAnalysis>(
    cfg: &FunctionCfg<'_>,
    analysis: &A,
) -> DataflowResult<A::State> {
    let reachable: HashSet<BlockId> = cfg.reachable_blocks();
    let po = cfg.postorder();

    let mut entry_states: HashMap<BlockId, A::State> = HashMap::new();
    let mut exit_states: HashMap<BlockId, A::State> = HashMap::new();

    // Initialise: exit block gets initial_state, everything else bottom.
    // For backward analysis, "entry" = bottom of block, "exit" = top.
    for &id in &po {
        if id == cfg.exit {
            exit_states.insert(id, analysis.initial_state());
        } else {
            exit_states.insert(id, A::State::bottom());
        }
        entry_states.insert(id, A::State::bottom());
    }

    // Compute initial entry states via transfer.
    for &id in &po {
        let block = cfg.block(id);
        let exit = &exit_states[&id];
        let entry = analysis.transfer(block, block.terminator.as_ref(), exit);
        entry_states.insert(id, entry);
    }

    // Worklist iteration (post-order for backward).
    let mut worklist: VecDeque<BlockId> = po.into_iter().collect();
    let mut in_worklist: HashSet<BlockId> = worklist.iter().copied().collect();
    let preds = cfg.predecessors();

    while let Some(id) = worklist.pop_front() {
        in_worklist.remove(&id);

        if !reachable.contains(&id) {
            continue;
        }

        // Join successor entry states → this block's exit.
        let new_exit = if id == cfg.exit {
            analysis.initial_state()
        } else {
            let mut iter = cfg
                .successors(id)
                .into_iter()
                .filter(|s| reachable.contains(s))
                .map(|s| &entry_states[&s]);
            match iter.next() {
                None => A::State::bottom(),
                Some(first) => iter.fold(first.clone(), |acc, s| acc.join(s)),
            }
        };

        // Transfer.
        let block = cfg.block(id);
        let new_entry = analysis.transfer(block, block.terminator.as_ref(), &new_exit);

        // Update exit unconditionally, enqueue predecessors if entry changed.
        exit_states.insert(id, new_exit);
        if new_entry == entry_states[&id] {
            continue;
        }
        entry_states.insert(id, new_entry);
        if let Some(pred_ids) = preds.get(&id) {
            for &pred in pred_ids {
                if reachable.contains(&pred) && !in_worklist.contains(&pred) {
                    worklist.push_back(pred);
                    in_worklist.insert(pred);
                }
            }
        }
    }

    DataflowResult {
        entry_states,
        exit_states,
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  Tests
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gd_ast::{self, GdExpr, GdStmt};
    use crate::parser;

    // ── Lattice: set of variable names that are definitely assigned ───

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct AssignedVars(HashSet<String>);

    impl Lattice for AssignedVars {
        fn bottom() -> Self {
            Self(HashSet::new())
        }

        /// Must-analysis: intersection (variable must be assigned on ALL paths).
        fn join(&self, other: &Self) -> Self {
            Self(self.0.intersection(&other.0).cloned().collect())
        }
    }

    // ── Analysis: track which variables are definitely assigned ───────

    struct DefiniteAssignment;

    impl DataflowAnalysis for DefiniteAssignment {
        type State = AssignedVars;

        fn direction(&self) -> Direction {
            Direction::Forward
        }

        fn initial_state(&self) -> AssignedVars {
            // Nothing assigned at function entry.
            AssignedVars::bottom()
        }

        fn transfer(
            &self,
            block: &BasicBlock<'_>,
            _terminator: Option<&Terminator<'_>>,
            state: &AssignedVars,
        ) -> AssignedVars {
            let mut out = state.clone();
            for stmt in &block.stmts {
                // Track simple `var x = ...` and `x = ...` assignments.
                match stmt {
                    GdStmt::Var(var) if var.value.is_some() => {
                        out.0.insert(var.name.to_string());
                    }
                    GdStmt::Assign {
                        target: GdExpr::Ident { name, .. },
                        ..
                    } => {
                        out.0.insert((*name).to_string());
                    }
                    _ => {}
                }
            }
            out
        }
    }

    fn with_dataflow(
        source: &str,
        f: impl FnOnce(&FunctionCfg<'_>, &DataflowResult<AssignedVars>),
    ) {
        let tree = parser::parse(source).unwrap();
        let file = gd_ast::convert(&tree, source);
        let func = file.funcs().next().expect("no function found");
        let cfg = FunctionCfg::build(&func.body);
        let result = solve(&cfg, &DefiniteAssignment);
        f(&cfg, &result);
    }

    #[test]
    fn sequential_assignment() {
        let src = "func foo():\n\tvar x = 1\n\tvar y = 2\n\tprint(x + y)\n";
        with_dataflow(src, |cfg, result| {
            // After the real block: both x and y are assigned.
            let exit = result.exit(BlockId(2));
            assert!(exit.0.contains("x"));
            assert!(exit.0.contains("y"));
            // At entry of the real block: nothing assigned.
            let entry = result.entry(BlockId(2));
            assert!(entry.0.is_empty());
            // EXIT block should see the same state.
            let exit_block_entry = result.entry(cfg.exit);
            assert!(exit_block_entry.0.contains("x"));
        });
    }

    #[test]
    fn assignment_in_one_branch_only() {
        let src = "func foo(c):\n\tvar x = 0\n\tif c:\n\t\tx = 1\n\tprint(x)\n";
        with_dataflow(src, |_cfg, result| {
            // x is assigned before the if (var x = 0), so it's definitely
            // assigned at the merge block regardless of the branch.
            // B2=[var x=0], B3=merge=[print(x)], B4=then=[x=1]
            let merge_entry = result.entry(BlockId(3));
            assert!(
                merge_entry.0.contains("x"),
                "x should be assigned (initialized before if)"
            );
        });
    }

    #[test]
    fn assignment_in_both_branches() {
        let src = "func foo(c):\n\tif c:\n\t\tvar x = 1\n\telse:\n\t\tvar x = 2\n\tpass\n";
        with_dataflow(src, |_cfg, result| {
            // x is assigned in both branches → must-analysis intersection
            // says x is assigned at merge.
            // B2=branch, B3=merge, B4=then=[var x=1], B5=else=[var x=2]
            let merge_entry = result.entry(BlockId(3));
            assert!(merge_entry.0.contains("x"));
        });
    }

    #[test]
    fn assignment_missing_in_else() {
        let src = "func foo(c):\n\tif c:\n\t\tvar x = 1\n\telse:\n\t\tpass\n\tpass\n";
        with_dataflow(src, |_cfg, result| {
            // x assigned only in the then-branch. Must-analysis intersection:
            // then has {x}, else has {} → merge has {}.
            let merge_entry = result.entry(BlockId(3));
            assert!(
                !merge_entry.0.contains("x"),
                "x not definitely assigned (missing in else)"
            );
        });
    }

    #[test]
    fn loop_assignment_converges() {
        let src = "func foo():\n\tvar x = 0\n\tfor i in range(10):\n\t\tx = x + 1\n\tprint(x)\n";
        with_dataflow(src, |_cfg, result| {
            // x is assigned before the loop, so at loop exit it's still
            // definitely assigned.
            // B2=[var x=0, Goto(B3)], B3=header, B4=body=[x=x+1], B5=exit
            let exit_entry = result.entry(BlockId(5));
            assert!(exit_entry.0.contains("x"));
        });
    }

    // ── Backward analysis: live variables ────────────────────────────

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct LiveVars(HashSet<String>);

    impl Lattice for LiveVars {
        fn bottom() -> Self {
            Self(HashSet::new())
        }

        /// May-analysis: union (live on ANY path).
        fn join(&self, other: &Self) -> Self {
            Self(self.0.union(&other.0).cloned().collect())
        }
    }

    struct Liveness;

    impl DataflowAnalysis for Liveness {
        type State = LiveVars;

        fn direction(&self) -> Direction {
            Direction::Backward
        }

        fn initial_state(&self) -> LiveVars {
            LiveVars::bottom()
        }

        fn transfer(
            &self,
            block: &BasicBlock<'_>,
            _terminator: Option<&Terminator<'_>>,
            state: &LiveVars,
        ) -> LiveVars {
            let mut live = state.clone();
            // Walk statements in reverse: kill writes, gen reads.
            for stmt in block.stmts.iter().rev() {
                match stmt {
                    GdStmt::Assign { target, value, .. } => {
                        if let GdExpr::Ident { name, .. } = target {
                            live.0.remove(*name);
                        }
                        collect_reads(value, &mut live.0);
                    }
                    GdStmt::Var(var) => {
                        live.0.remove(var.name);
                        if let Some(val) = &var.value {
                            collect_reads(val, &mut live.0);
                        }
                    }
                    GdStmt::Expr { expr, .. } => {
                        collect_reads(expr, &mut live.0);
                    }
                    _ => {}
                }
            }
            live
        }
    }

    /// Collect identifier reads from an expression (shallow — doesn't recurse
    /// into calls/methods, just top-level idents).
    fn collect_reads(expr: &GdExpr<'_>, out: &mut HashSet<String>) {
        match expr {
            GdExpr::Ident { name, .. } => {
                out.insert((*name).to_string());
            }
            GdExpr::BinOp { left, right, .. } => {
                collect_reads(left, out);
                collect_reads(right, out);
            }
            GdExpr::Call { args, .. } => {
                for arg in args {
                    collect_reads(arg, out);
                }
            }
            _ => {}
        }
    }

    #[test]
    fn dead_store_detected() {
        let src = "func foo():\n\tvar x = 1\n\tx = 2\n\tprint(x)\n";
        let tree = parser::parse(src).unwrap();
        let file = gd_ast::convert(&tree, src);
        let func = file.funcs().next().unwrap();
        let cfg = FunctionCfg::build(&func.body);
        let result = solve(&cfg, &Liveness);

        // After "var x = 1", x is NOT live (it's immediately overwritten).
        // The entry state of the block should have x live (because of the
        // later read), but after "var x = 1" and before "x = 2", x was
        // just written and the value is dead.
        //
        // All stmts are in one block (B2). At B2 exit, x is not live
        // (no reads after). Working backward through B2:
        // - print(x): gen x → live={x}
        // - x = 2: kill x, no reads in value → live={}
        // - var x = 1: kill x → live={}
        // So B2 entry has live={}, meaning x is not live at function entry.
        let b2_entry = result.entry(BlockId(2));
        assert!(!b2_entry.0.contains("x"), "x should not be live at entry");
    }

    #[test]
    fn live_variable_across_branch() {
        let src = "func foo(c):\n\tvar x = 1\n\tif c:\n\t\tprint(x)\n\telse:\n\t\tpass\n";
        let tree = parser::parse(src).unwrap();
        let file = gd_ast::convert(&tree, src);
        let func = file.funcs().next().unwrap();
        let cfg = FunctionCfg::build(&func.body);
        let result = solve(&cfg, &Liveness);

        // x is used in the then-branch. May-analysis union: x is live
        // before the if.
        // B2=[var x=1], B3=merge, B4=then=[print(x)], B5=else=[pass]
        let b2_exit = result.exit(BlockId(2));
        assert!(
            b2_exit.0.contains("x"),
            "x should be live after assignment (used in then-branch)"
        );
    }
}
