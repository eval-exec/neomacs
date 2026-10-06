//! Conservative loop policy and exact baseline CFG backedge classification.
//!
//! Threading: read-only borrowed compiler metadata, with no runtime Lisp state
//! or shared mutable cache. Static switch edges remain owned by the validated CFG.

use super::knobs::{SwitchLoopPolicy, jit_switch_loop_policy};
use super::{Cfg, Op};
use crate::emacs_core::bytecode::opcode::BranchTargets;

/// Production policy is chosen before compilation. The default examines only
/// direct edges, preserving original emission; the optional conservative policy
/// also includes Switch tables whose targets are not available from Op alone.
pub(crate) fn has_back_edge(ops: &[Op]) -> bool {
    has_back_edge_with_policy(ops, jit_switch_loop_policy())
}

/// Pure ops-only classification. Handler resumes are separate from ordinary
/// branches. Exact baseline GC/quit polls remain determined by the validated CFG.
#[deny(clippy::wildcard_enum_match_arm)]
pub(crate) fn has_back_edge_with_policy(ops: &[Op], policy: SwitchLoopPolicy) -> bool {
    ops.iter()
        .enumerate()
        .any(|(pc, op)| match op.branch_targets() {
            BranchTargets::Direct(target) => target.get() as usize <= pc,
            BranchTargets::SwitchTable => match policy {
                SwitchLoopPolicy::DirectOnly => false,
                SwitchLoopPolicy::Conservative => true,
            },
            BranchTargets::None | BranchTargets::Handler(_) => false,
        })
}

/// Baseline GC/quit polls use actual resolved switch instruction indices.
#[deny(clippy::wildcard_enum_match_arm)]
pub(crate) fn baseline_has_backedge(ops: &[Op], cfg: &Cfg) -> bool {
    ops.iter()
        .enumerate()
        .any(|(pc, op)| match op.branch_targets() {
            BranchTargets::Direct(target) => target.get() as usize <= pc,
            BranchTargets::None | BranchTargets::Handler(_) | BranchTargets::SwitchTable => false,
        })
        || cfg
            .switch_targets
            .iter()
            .any(|(pc, targets)| targets.iter().any(|&(_, target)| target <= *pc))
}

#[cfg(test)]
#[path = "tests/switch_loop_policy.rs"]
mod policy_tests;
