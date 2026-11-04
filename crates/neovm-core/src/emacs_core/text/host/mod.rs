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
use neovm_edit::emacs_core::edit_host::{EditHost, EditVariableRead};
use neovm_value::error::{EvalResult, Flow};
use neovm_value::heap_types::{LispString, ProcessId};
use neovm_value::intern::SymId;
use neovm_value::symbol::Obarray;
use neovm_value::value::Value;

use super::coding::{CodingSystemManager, EolConversion};
use crate::code_conversion_workspace::{CodeConversionBufferLease, ConversionBufferEncoding};
use crate::encoding::{CodingEncoderState, RuntimeCodingSystem};

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

    // -------------------------------------------------------------------------
    // Text-specific services (cired2's 09:52Z GO batch; one TEXT-HOST row
    // each, or EDIT-HOST when it is a generic eval service)
    // -------------------------------------------------------------------------

    /// GNU `eval_sub`: evaluate FORM in this evaluator (T6.1; the encoding
    /// pre-write/post-read hook bodies keep it exactly as today).
    /// TEXT-HOST row.
    fn eval_sub(&mut self, form: Value) -> EvalResult;

    /// GNU's `inhibit-eol-conversion` read at the point of conversion
    /// (`src/coding.c`). TEXT-HOST row; pure read (T5.3).
    fn eol_conversion(&self) -> EolConversion;

    /// The coding-system manager (coding-owned state; read-only through
    /// this row). TEXT-HOST row; pure read (T5.3).
    fn coding_systems(&self) -> &CodingSystemManager;

    /// The live encoder state of PROCESS, borrowed only after callbacks
    /// ended. TEXT-HOST row.
    fn process_encoder_state(
        &mut self,
        process: ProcessId,
    ) -> Result<&mut CodingEncoderState, Flow>;

    /// The process's current encoding coding system, re-read after a
    /// pre-write hook callback may have replaced it (GNU
    /// `send_process`'s recalibration). `None`-coding is an error with
    /// GNU's "Process output coding is not initialized" message.
    /// TEXT-HOST row.
    fn process_encoding_coding(&mut self, process: ProcessId) -> Result<RuntimeCodingSystem, Flow>;

    /// Whether PROCESS's encoding path is bypassed: the descriptor does not
    /// require encoding (GNU's raw/unibyte sink). `true` when no process is
    /// found, mirroring the existing `is_some_and` read. TEXT-HOST row.
    fn process_encoding_bypasses(&mut self, process: ProcessId) -> bool;

    /// Whether the current buffer is shown in the selected frame's window
    /// tree. Scalar only; no FrameManager/WindowId crosses. TEXT-HOST row;
    /// pure read (T5.3).
    fn current_buffer_is_displayed(&self) -> bool;

    /// Acquire the reusable code-conversion buffer (GNU
    /// `code_conversion_save`), through the evaluator so the localized
    /// forwarding metadata is updated. TEXT-HOST row.
    fn acquire_code_conversion_buffer(
        &mut self,
        encoding: ConversionBufferEncoding,
    ) -> Result<CodeConversionBufferLease, Flow>;

    /// Replace the leased conversion buffer's contents. TEXT-HOST row.
    fn set_code_conversion_buffer_contents(
        &mut self,
        lease: &CodeConversionBufferLease,
        text: &LispString,
    );

    /// Release the lease. TEXT-HOST row.
    fn release_code_conversion_buffer(&mut self, lease: CodeConversionBufferLease);
}

/// The write surface a text phase may use over one owner's variables
/// (PEER-RULING-STAGE3B T6.2/T6.3).
///
/// Text never names `VariableStore` or `OwnerSlot`: core constructs every
/// store (including the bootstrap `VariableStore::new(obarray, None)`),
/// because the owner concept lives in eval. The read view reuses
/// [`EditVariableRead`]; writes stay on [`TextVariableStore::store`], whose
/// core impl keeps today's `set_canonical_value_routed` +
/// `store_projected`-when-Projected semantics unchanged.
pub trait TextVariableStore {
    /// The read view of the same variables.
    type View<'a>: EditVariableRead<'a> + Copy
    where
        Self: 'a;

    /// The obarray, for what a store does not cover (seed/make-special/
    /// property writes).
    fn obarray(&mut self) -> &mut Obarray;

    /// GNU `Vfoo = VALUE` from C (see `Context::store_variable`).
    fn store(&mut self, id: SymId, value: Value);

    /// The read view of the same variables.
    fn view(&self) -> Self::View<'_>;
}

/// The CCL same-thread raw publication (PEER-RULING-STAGE3B T2; own signed
/// unsafe row, never folded into the read surface).
///
/// `into_raw` returns a `Copy`, stack-owned raw form: the caller keeps it
/// alive for the publication's body (a TLS pointer set/restore with a null
/// default) and reconstructs through [`Self::view_from_raw`]. The raw form
/// is opaque to text — only the projection `<V as
/// RawTextVariableView>::Raw` is nameable — and MUST NOT appear in any
/// TextHost receiver or return.
///
/// # Safety
/// Reconstructing may run only while the source view's borrows outlive `'a`,
/// on the same thread, inside the publication body; callers copy `Value`s
/// out and never keep the rebuilt view past that body.
pub trait RawTextVariableView<'a>: EditVariableRead<'a> + Copy {
    /// The opaque, `Copy` raw form (core: the two raw parts).
    type Raw: Copy;

    /// Publish this view's raw parts. The result is valid only while the
    /// source view's borrows outlive `'a`, on this thread.
    fn into_raw(self) -> Self::Raw;

    /// Rebuild the view from its raw parts.
    ///
    /// # Safety
    /// `raw` must come from [`Self::into_raw`] on this thread, and the
    /// source view's borrows must still outlive `'a`.
    unsafe fn view_from_raw(raw: Self::Raw) -> Self;
}
