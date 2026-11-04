//! The text layer's host interface (stage-3b R0).
//!
//! Text code must not name `eval::Context`: after the crate split the text
//! crate cannot reach the evaluator's types at all. Text functions that need
//! evaluator, system or display services take `H: TextHost`, monomorphized at
//! `H = Context`; dispatch is static, nothing is installed, and there is no
//! "uninstalled" state. The trait is never `dyn`, and there is exactly one
//! production impl, in `runtime/eval` (mirror of `edit_host`).
//!
//! `TextHost: EditHost` (PEER-RULING-STAGE3B T6.0): text sits above edit and
//! its bodies already call edit host-generic code, so every TextHost
//! implementor IS an EditHost, and there is ONE home per service. TextHost
//! declares only text-specific methods; it never redeclares an EditHost
//! service (`symbol_mode`, `lisp_string`, `eval_symbol_by_id`,
//! `visible_variable_value_or_nil`, `funcall_general` / `apply`, the
//! specpdl/root API, the hook methods, `set_variable`, …), because a
//! redeclared name is E0034-ambiguous for `H: TextHost`. Generic eval
//! services that EditHost lacks are added there as thin rows, not here.
//!
//! Functions that need only edit state take `&mut BufferManager` (or
//! `&mut EditRuntime`) and stay monomorphic — prefer that form (jitcall
//! RECEIVER-SHAPE 00:26Z). A body that also reads variables opens the
//! `variables()` phase in the narrowest scope; while the view lives only
//! `&self` host methods run — every `&mut self` method is borrowck-blocked
//! — and only `Copy` values cross a host phase. Buffers arrive through
//! [`TextHost::buffers`] (shared, composable with a view) and
//! [`TextHost::buffers_mut`] (write phase, no view live).
//!
//! Each TextHost method is one TEXT-HOST ledger row, named in the READY that
//! lands it. An installed `&'static` fn table is correct only where no host
//! is in scope (the C2/C7/C8a seams) — never here.

use neovm_edit::buffer::BufferManager;
use neovm_edit::emacs_core::edit_host::EditHost;

pub trait TextHost: EditHost {
    /// The text phase opener: the owner's variable-read view, from DISJOINT
    /// field borrows. It is [`EditHost::Vars`] — there is no second view type
    /// (T6.0). Open it in the narrowest scope the body allows; while it
    /// lives only `&self` host methods may run, and every `&mut self` method
    /// is borrowck-blocked, so no mutation can race a live view. Read-only
    /// host calls and buffer reads interleave exactly as the pre-inversion
    /// code did. TEXT-HOST row; pure read (T5.3).
    fn variables(&self) -> <Self as EditHost>::Vars<'_>;

    /// The edit layer's buffer manager for read-only access: the
    /// shared-buffer argument of every `_in_buffers`/`_in_manager` callee
    /// and the `current_buffer()`/`get(id)` source in text bodies.
    /// Read-only, so it composes with a live view. TEXT-HOST row; pure read
    /// (T5.3).
    fn buffers(&self) -> &BufferManager;

    /// Mutable buffer access for a write phase. Cannot coexist with a live
    /// variable view (borrowck): close the view first. TEXT-HOST row.
    fn buffers_mut(&mut self) -> &mut BufferManager;
}
