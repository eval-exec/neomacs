//! Undo system -- buffer undo/redo functionality.
//!
//! Provides Emacs-compatible undo functionality:
//! - `undo-boundary` -- insert an undo boundary marker (GNU `Fundo_boundary`,
//!   src/undo.c:251, the ONLY subr GNU's `syms_of_undo` defines)
//! - undo-list truncation at GC time (GNU `compact_buffer`, src/buffer.c:1856)
//!
//! `primitive-undo` and `undo` are deliberately absent: GNU implements both
//! in Lisp (`lisp/simple.el:3645` and `:3466`) and has no C version of
//! either, so the runtime loads the .el (DIVERGENCES.md 146 and 150).

use super::error::{EvalResult, Flow, signal};
use super::value::*;
use crate::buffer::buffer::UndoBoundaryOutcome;
use crate::emacs_core::error::expect_args;

// ---------------------------------------------------------------------------
// Bootstrap variables
// ---------------------------------------------------------------------------

pub fn register_bootstrap_vars(obarray: &mut super::symbol::Obarray) {
    // GNU `syms_of_undo' (src/undo.c:437-457).
    obarray.define_int_variable("undo-limit", 160000);
    obarray.define_int_variable("undo-strong-limit", 240000);

    // `undo-outer-limit' (src/undo.c:459-474) defaults to 24000000, but
    // `--batch' replaces it with nil before anything runs
    // (src/emacs.c:1700-1707).  A bare Context is a batch evaluator, so nil is
    // the default here and the binary raises it for interactive sessions.
    obarray.set_symbol_value("undo-outer-limit", Value::NIL);
    obarray.make_special("undo-outer-limit");
    // `undo-outer-limit-function' (src/undo.c:476-485); lisp/simple.el sets it
    // to `undo-outer-limit-truncate' once that file is loaded.
    obarray.set_symbol_value("undo-outer-limit-function", Value::NIL);
    obarray.make_special("undo-outer-limit-function");
}

// ---------------------------------------------------------------------------
// Truncation at garbage collection
// ---------------------------------------------------------------------------

/// Read the truncation limits out of the bindings visible in the current
/// buffer, which is what GNU's `set_buffer_internal (b)` at the top of
/// `truncate_undo_list' (src/undo.c:296-306) arranges for.
impl crate::buffer::UndoLimitBindings for super::eval::Context {
    fn undo_limit(&self) -> Value {
        self.undo_truncation_variable("undo-limit")
    }

    fn undo_strong_limit(&self) -> Value {
        self.undo_truncation_variable("undo-strong-limit")
    }

    fn undo_outer_limit(&self) -> Value {
        self.undo_truncation_variable("undo-outer-limit")
    }

    fn undo_outer_limit_function(&self) -> Value {
        self.undo_truncation_variable("undo-outer-limit-function")
    }
}

thread_local! {
    /// Re-entrancy latch for [`compact_buffers_for_gc`].
    ///
    /// `undo-outer-limit-function' is Lisp and may call `garbage-collect'
    /// itself.  GNU cannot recurse here because its `garbage_collect' bails
    /// out on `garbage_collection_inhibited' (src/alloc.c:5789-5790) and
    /// `truncate_undo_list' holds that inhibition across the call
    /// (src/undo.c:296-298); Neomacs' explicit `garbage-collect' collects
    /// regardless, so the latch is what keeps one compaction pass from
    /// starting another.
    static COMPACTION_IN_PROGRESS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

struct CompactionLatch;

impl CompactionLatch {
    /// `None` when a compaction pass is already running on this thread.
    fn acquire() -> Option<Self> {
        COMPACTION_IN_PROGRESS.with(|flag| {
            if flag.get() {
                None
            } else {
                flag.set(true);
                Some(Self)
            }
        })
    }
}

impl Drop for CompactionLatch {
    fn drop(&mut self) {
        COMPACTION_IN_PROGRESS.with(|flag| flag.set(false));
    }
}

/// Shorten every live buffer's undo list, the way GNU's collector does.
///
/// GNU runs this as the first thing `garbage_collect' does, before any
/// marking: "Don't keep undo information around forever.  Do this early on, so
/// it is no problem if the user quits." (src/alloc.c:5796-5800).  The walk goes
/// through `compact_buffer' (src/buffer.c:1854-1885), which skips dead
/// buffers, indirect buffers, and buffers unchanged since the last compaction,
/// and refuses to hand a `t' undo list to `truncate_undo_list' because that
/// would turn undo back on.
///
/// Errors from `undo-outer-limit-function' are swallowed, as they are for the
/// finalizers and `post-gc-hook' this collector already runs: a collection
/// cannot propagate a signal to whatever the mutator was doing.
pub(crate) fn compact_buffers_for_gc(ctx: &mut super::eval::Context) {
    let Some(latch) = CompactionLatch::acquire() else {
        return;
    };
    let restore_to = ctx.buffers.current_buffer_id();
    for id in ctx.buffers.buffer_list() {
        compact_one_buffer_for_gc(ctx, id);
    }
    if let Some(id) = restore_to {
        ctx.restore_current_buffer_if_live(id);
    }
    drop(latch);
}

/// GNU `compact_buffer' (src/buffer.c:1854-1885), minus the gap shrinking that
/// has no counterpart in Neomacs' buffer text.
fn compact_one_buffer_for_gc(ctx: &mut super::eval::Context, id: crate::buffer::BufferId) {
    use crate::buffer::{UndoLimits, UndoRecording};

    let Some(buffer) = ctx.buffers.get(id) else {
        return; // killed while we walked the list
    };
    if buffer.base_buffer.is_some() {
        return; // indirect buffers share their base's text
    }
    let modified_tick = buffer.modified_tick();
    if buffer.undo_state.compacted_modified_tick() == modified_tick {
        return; // unchanged since the last compaction
    }
    // GNU stamps the buffer whatever the truncation decides, including for the
    // `t' and early-return paths (src/buffer.c:1884).
    buffer.undo_state.set_compacted_modified_tick(modified_tick);

    let undo_list = buffer.get_undo_list();
    if UndoRecording::of(&undo_list) == UndoRecording::Disabled {
        return;
    }

    // Everything from here reads the buffer's own variable bindings, so the
    // buffer has to be current -- the reason GNU calls `set_buffer_internal'.
    if ctx.set_current_buffer_unrecorded(id).is_err() {
        return;
    }
    let Some(mut limits) = UndoLimits::read(ctx) else {
        return;
    };

    let first_group_bytes = crate::buffer::undo_first_group_bytes(undo_list);
    if let Some(function) = limits.outer_limit_function_for(first_group_bytes) {
        let saved_roots = super::eval::save_scratch_gc_roots();
        super::eval::push_scratch_gc_root(function);
        let handled = ctx.with_gc_inhibited(|eval| {
            eval.funcall_general(function, vec![Value::fixnum(first_group_bytes)])
        });
        super::eval::restore_scratch_gc_roots(saved_roots);
        if handled.is_ok_and(|answer| !answer.is_nil()) {
            // "The function is responsible for making any desired changes in
            // buffer-undo-list." (src/undo.c:362-368)
            return;
        }
        // GNU reads `undo_limit' and `undo_strong_limit' during the walk that
        // follows (src/undo.c:386-389), so a function that lowers them and
        // answers nil has its new values applied.  They are C globals, so it
        // reads them from whatever buffer the function left current -- both
        // halves measured under GNU 31.0.90: a function that lowers this
        // buffer's limits and stays truncates 21 entries to 2; the same
        // function ending in `(set-buffer "H")' leaves all 21, because H's
        // values are what the globals then hold.
        let Some(reread) = UndoLimits::read(ctx) else {
            return;
        };
        limits = reread;
    }

    // Re-read the list: the function may have replaced it before answering nil.
    let Some(undo_list) = ctx.buffers.get(id).map(|buffer| buffer.get_undo_list()) else {
        return; // the function killed the buffer
    };
    if UndoRecording::of(&undo_list) == UndoRecording::Disabled {
        return;
    }
    let truncated = crate::buffer::truncate_undo_list(undo_list, &limits);
    if let Some(buffer) = ctx.buffers.get_mut(id) {
        buffer.set_undo_list(truncated);
    }
}

// ---------------------------------------------------------------------------
// Pure builtins
// ---------------------------------------------------------------------------

/// (undo-boundary) -> nil
///
/// Context-dependent variant used during normal execution: inserts an
/// undo boundary into the current buffer's undo list.
pub(crate) fn builtin_undo_boundary(
    ctx: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("undo-boundary", &args, 0)?;
    let Some(current_id) = ctx.buffers.current_buffer_id() else {
        return Err(signal("error", vec![Value::string("No current buffer")]));
    };
    // GNU sets `undo-auto--last-boundary-cause' to `explicit' inside
    // `Fundo_boundary' (src/undo.c:277), after the early return for an
    // undo-disabled buffer, so a buffer that records nothing does not claim a
    // boundary either.  The boundary itself runs below the obarray, so the
    // assignment lives here, gated on the outcome it reports.
    if ctx.buffers.add_undo_boundary(current_id) == Some(UndoBoundaryOutcome::Recorded) {
        set_last_boundary_cause_explicit(ctx)?;
    }
    Ok(Value::NIL)
}

/// GNU `Fset (Qundo_auto__last_boundary_cause, Qexplicit)` (src/undo.c:277).
///
/// This goes through our `set' builtin rather than a direct obarray write
/// because GNU goes through `Fset': the variable is a `defvar-local' in
/// lisp/simple.el, so wherever a buffer-local binding exists the assignment
/// must land on THAT and not on the default.  Writing the default instead is
/// invisible until something has made the variable local, which is the ordinary
/// case -- `undo-auto--undoably-changed-buffers' processing localizes it.
/// Delegating also picks up alias resolution, the constant check and variable
/// watchers, all of which GNU gets for free from the same call.
pub(crate) fn set_last_boundary_cause_explicit(ctx: &mut super::eval::Context) -> Result<(), Flow> {
    super::builtins::symbols::builtin_set_2(
        ctx,
        Value::symbol("undo-auto--last-boundary-cause"),
        Value::symbol("explicit"),
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Eval-dependent builtins
// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
