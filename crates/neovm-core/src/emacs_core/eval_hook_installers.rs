//! The one core process-level eval boot entry (stage-4 E3a).
//!
//! Core owns exactly one boot entry, [`install_eval_hooks`]. It delegates to
//! the eval-side publication entry with the single canonical [`EvalHookSet`],
//! which publishes every eval PC2 slot in a fixed order and mints the
//! [`EvalHooksInstalled`] proof only after the last slot succeeds. Eval
//! `Context` constructors take that proof by value, so the GC-root-enumerating
//! and pdump-reconstruct slots provably exist before the first possible
//! allocation (peer E3a, refining E3).
//!
//! The command/display modules keep the canonical descriptor statics and the
//! bodies; this module only assembles the set and is the exact caller recorded
//! by the eval publication entry's PC2 row.
//!
//! Frequency: construction / dump restore, once per process. Not a hot path.

use crate::emacs_core::eval::Context;
use crate::emacs_core::eval::hooks_proof::{EvalHookSet, EvalHooksInstalled};
use std::sync::OnceLock;

static CANONICAL_EVAL_HOOK_SET: OnceLock<EvalHookSet> = OnceLock::new();

/// Publish every eval cold table and return the boot proof.
///
/// Idempotent: a second call re-publishes the same set as `AlreadyInstalled`
/// and returns the proof again. A conflicting set is the eval entry's typed
/// refusal, which this process-level entry treats as a fatal invariant
/// violation (two canonical sets can never exist).
pub fn install_eval_hooks() -> EvalHooksInstalled {
    let set = CANONICAL_EVAL_HOOK_SET.get_or_init(|| {
        EvalHookSet::new(
            crate::emacs_core::lifecycle_hooks::canonical_lifecycle_hooks(),
            crate::emacs_core::process_hooks::canonical_process_hooks(),
            crate::emacs_core::input_hooks::canonical_input_hooks(),
            crate::emacs_core::interactive::canonical_call_interactively_hooks(),
            crate::emacs_core::casetab_entries::canonical_case_table_hooks(),
            crate::emacs_core::buffer_ops::canonical_buffer_kill_hooks(),
            crate::emacs_core::kmacro::canonical_kmacro_hooks(),
            crate::emacs_core::xdisp::canonical_display_query_hooks(),
            crate::emacs_core::perf_trace::canonical_perf_enabled(),
            crate::emacs_core::perf_trace::canonical_perf_hooks(),
        )
    });
    match crate::emacs_core::eval::hooks_proof::install_eval_hooks(set) {
        Ok(proof) => proof,
        Err(_) => panic!("a different eval hook set is already installed"),
    }
}

/// Core facade constructor: boot the eval hooks, then build a full Context.
///
/// This is the only way for a core-side caller (neomacs, the worker, tests)
/// to construct the evaluator without naming the individual tables.
pub fn new_context() -> Context {
    let proof = install_eval_hooks();
    Context::new(proof)
}
