//! Cold exit blocks (P2.2 O0.2, knob `NEOVM_JIT_COLD_EXITS`).
//!
//! Cranelift lowers a function's blocks in a depth-first order over the
//! CFG, not in layout order, and moves only blocks marked cold to the end
//! (`machinst/vcode.rs`, "cold blocks at the end"; an edge block split in
//! front of a cold block is cold too). Without the mark an exit reached by a
//! guard's `brif` lands between hot blocks: the hot path then jumps over it,
//! and it shares the hot code's cache lines.
//!
//! The exits, in both tiers (the baseline and MIR share every emitter):
//!
//! | exit | where it is filled |
//! |---|---|
//! | precise deopt (`STATUS_DEOPT_AT`) | `emit_pending_deopts` |
//! | rerun deopt (`STATUS_DEOPT`, pure MIR) | `build_mir_leaf_fn` |
//! | the shared signal exit (`STATUS_SIGNAL`) | `build_leaf_fn`, `build_mir_leaf_fn` |
//! | a handler dispatch (a signal inside a protected extent) | `emit_pending_dispatches` |
//! | the back-edge poll's slow path | `emit_backedge_jump_with_args` |
//! | a root-window grow call | `emit_root_window_stores`, `emit_hoisted_root_window_prologue` |
//!
//! Every exit is already shared where its state is: one signal exit and one
//! rerun block per function, one deopt block per guarded op (all its guards
//! branch there), one dispatch per protected signal site.
//!
//! An exit that branches further (the poll's rooting and signal check, a
//! dispatch's handler compare chain) is marked with every block created
//! while it was emitted ([`ColdSpan`]): those are reachable only through it.
//! The mark changes where code goes, never what it does; off, nothing is
//! marked that was not marked before, so the CLIF is identical.

use cranelift_codegen::ir::Block;
use cranelift_frontend::FunctionBuilder;

use super::jit_cold_exits_on;

/// Which exit a cold mark is for: the compile census
/// (`stats::CompileStats::cold_exits`, the `cold_exits[...]` summary group
/// under `NEOVM_JIT_COMPILE_STATS=1`) counts marks by kind, so a run shows
/// the knob engaged.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, strum::EnumCount, strum::EnumIter, strum::IntoStaticStr,
)]
#[strum(serialize_all = "snake_case")]
pub(crate) enum ColdExit {
    /// A precise-deopt block (`STATUS_DEOPT_AT`).
    Deopt,
    /// The pure MIR body's shared rerun block (`STATUS_DEOPT`).
    Rerun,
    /// The shared signal exit (`STATUS_SIGNAL`).
    Signal,
    /// A handler dispatch at a signal site inside a protected extent.
    Dispatch,
    /// A back-edge poll's slow path (rooting, the service call, the
    /// signal check).
    Poll,
    /// A root-window grow call (per site, or the hoisted entry check).
    Grow,
}

/// Mark exit block `block` (a `kind` exit) cold, when the knob is on.
#[inline]
pub(crate) fn mark_exit_cold(fb: &mut FunctionBuilder, block: Block, kind: ColdExit) {
    if jit_cold_exits_on() {
        fb.set_cold_block(block);
        super::super::stats::record_cold_exit(kind);
    }
}

/// The blocks an exit's emission creates: [`Self::begin`] before the first,
/// [`Self::end`] after the last marks each of them cold (knob on).
/// Cranelift numbers blocks in creation order, so the span is a range.
#[must_use]
pub(crate) struct ColdSpan {
    first: Option<u32>,
}

impl ColdSpan {
    /// Start a span at the next block `fb` creates.
    pub(crate) fn begin(fb: &FunctionBuilder) -> Self {
        ColdSpan {
            first: jit_cold_exits_on().then(|| fb.func.dfg.num_blocks() as u32),
        }
    }

    /// Mark every block created since [`Self::begin`] cold. `exit` names
    /// the exit the span is, when its blocks are one exit of their own
    /// (the poll); `None` when the span only collects the blocks of exits
    /// already counted by [`mark_exit_cold`].
    pub(crate) fn end(self, fb: &mut FunctionBuilder, exit: Option<ColdExit>) {
        let Some(first) = self.first else {
            return;
        };
        let last = fb.func.dfg.num_blocks() as u32;
        for index in first..last {
            fb.set_cold_block(Block::from_u32(index));
        }
        if let Some(kind) = exit {
            super::super::stats::record_cold_exit(kind);
        }
    }
}

#[cfg(test)]
#[path = "../tests/cold_exits.rs"]
mod tests;
