//! The edit layer's host interface (stage-3 R0).
//!
//! Edit code must not name `eval::Context`: after the crate split the
//! edit crate cannot reach the evaluator's types at all. Edit functions
//! that need eval, display or system services take `H: EditHost`,
//! monomorphized at `H = Context`; dispatch is static, nothing is
//! installed, and there is no "uninstalled" state. The trait is never
//! `dyn`, and there is exactly one production impl, in `runtime/eval`.
//! Precedent: `emacs_core::lisp::hook_runtime::HookRuntime` and
//! `emacs_core::display_host::DisplayHost`.
//!
//! Functions that need only edit state take `&mut EditRuntime` (or
//! `&mut BufferManager`) and stay monomorphic — prefer that form.
//!
//! The method surface has two owner sections (STAGE3-ASSIGN UPDATE):
//! **eval services** (this lane) and **display/system queries** (the
//! display-seam lane). Each method is one future `EDIT-HOST` ledger row,
//! named in the READY that lands it. An installed `&'static` fn table is
//! correct only where no host is in scope (C7 substrate sites, C4b from
//! host-free code) — never here.

use crate::buffer::Buffer;
use crate::emacs_core::bookmark::BookmarkManager;
use crate::emacs_core::error::{EvalResult, Flow};
use crate::emacs_core::intern::SymId;
use crate::emacs_core::value::Value;
use crate::tagged::SymbolMode;

pub trait EditHost {
    // -------------------------------------------------------------------------
    // eval services (stage-3 gapsurvey series: C13 hooks, C2 variables,
    // C3 funcall/apply/mode/lexical + the opaque specpdl-root API)
    // -------------------------------------------------------------------------

    /// Resolve a hook variable's alias chain to the final hook symbol
    /// (GNU `FindHook`); edit reads a hook's value through the result.
    fn hook_symbol_by_id(&self, hook_sym: SymId) -> SymId;

    /// `hook_symbol_by_id` starting from the hook's name.
    fn hook_symbol_by_name(&self, hook_name: &str) -> SymId;

    /// The resolved hook symbol's visible value, `None` when unbound.
    fn hook_value_by_id(&self, hook_sym: SymId) -> Option<Value>;

    /// Run a hook value (GNU `run_hook_with_args`); `inherit_global`
    /// merges the buffer-local and global portions.
    fn run_hook_value(
        &mut self,
        hook_sym: SymId,
        hook_value: Value,
        hook_args: &[Value],
        inherit_global: bool,
    ) -> EvalResult;

    /// `run_hook_value` stopping at the first nil result
    /// (GNU `run-hook-with-args-until-failure`).
    fn run_hook_value_until_failure(
        &mut self,
        hook_sym: SymId,
        hook_value: Value,
        hook_args: &[Value],
        inherit_global: bool,
    ) -> EvalResult;

    /// The variable's visible value (GNU `find_symbol_value`), `None`
    /// when unbound. C2.
    fn variable_value(&self, id: SymId) -> Option<Value>;

    /// [`Self::variable_value`] starting from the variable's name. C2.
    fn variable_value_by_name(&self, name: &str) -> Option<Value>;

    /// The variable's visible value in `buf`: the buffer's local
    /// binding when it has one, else the default. C2.
    fn variable_value_in_buffer(&self, buf: Option<&Buffer>, id: SymId) -> Option<Value>;

    /// GNU `default_value` without a buffer. C2.
    fn default_value(&self, id: SymId) -> Option<Value>;

    /// GNU `boundp` without a buffer. C2.
    fn boundp(&self, id: SymId) -> bool;

    /// GNU `Fset` on the evaluator: alias-resolving, watcher-running
    /// store — the `set` subr body (`builtin_set_2`). C2.
    fn set_variable(&mut self, symbol: Value, value: Value) -> EvalResult;

    /// GNU `reset_var_on_error` (`insdel.c`): store `value` into the
    /// variable's runtime binding cell directly — no alias resolution
    /// (the caller resolved the symbol already) and no variable
    /// watchers, exactly as the C `*location = Qnil` store. This is the
    /// error-recovery reset and nothing else; every other store is
    /// `set_variable`. C2.
    fn reset_variable_on_error(&mut self, sym: SymId, value: Value);

    /// Whether `inhibit-modification-hooks` is non-nil. GNU `insdel.c`
    /// reads the `DEFVAR_BOOL` flag in one load; the fast paths over
    /// the forwarding live on the host, where the symbol table is. C2.
    fn inhibit_modification_hooks(&self) -> bool;

    /// The session's bookmark set (bookmark.el globals). The state is
    /// `WorldSlot`-owned on the host and does NOT move with the edit
    /// crate (STAGE3-ASSIGN WorldSlot rule, coordinated with jitcall
    /// C1); edit reaches it only through this accessor. The rectangle
    /// `WorldSlot` needs no accessor: only core code touches it.
    fn bookmarks(&mut self) -> &mut BookmarkManager;

    /// GNU `apply`: call a function value on already-evaluated
    /// arguments. C3.
    fn apply(&mut self, function: Value, args: &[Value]) -> EvalResult;

    /// GNU `funcall_general`: unified function dispatch, the tree
    /// walker's entry. C3.
    fn funcall_general(&mut self, function: Value, args: &[Value]) -> EvalResult;

    /// Symbol interpretation for this mutator's current dynamic binding
    /// (`symbols-with-pos-enabled`). C3.
    fn symbol_mode(&self) -> SymbolMode;

    /// Set whether this runtime reads symbols lexically. C3.
    fn set_lexical_binding(&mut self, enabled: bool);

    // --- specpdl: GNU-shaped, opaque, mirroring today's call shapes 1:1
    // (PEER-RULING-STAGE3 C3). SpecCount/RootMark/RootSlot are
    // unforgeable tokens: `pub struct` with private fields, no public
    // constructor. Edit never sees `SpecBinding`, `&mut specpdl`, a
    // count as `usize`, or a push of an arbitrary entry. ---

    /// Opaque specpdl depth (GNU `SPECPDL_INDEX`).
    type SpecCount;

    /// Opaque mark of the GcRoot suffix (one per rooted traversal).
    type RootMark;

    /// Opaque handle to one updatable GcRoot entry.
    type RootSlot;

    /// Current specpdl depth as a token.
    fn specpdl_index(&self) -> Self::SpecCount;

    /// Enter one dynamic binding inside an already-established scope;
    /// if binding itself exits nonlocally (a variable watcher), drain
    /// to `count` first. The fallible counterpart of GNU callers'
    /// `specbind` + `unbind_to` pairs.
    fn try_specbind_or_unwind_to(
        &mut self,
        count: Self::SpecCount,
        sym: SymId,
        value: Value,
    ) -> Result<(), Flow>;

    /// Unwind to `count`, passing `result` through (GNU `unbind_to`
    /// with a value).
    fn unbind_to_with_result(&mut self, count: Self::SpecCount, result: EvalResult) -> EvalResult;

    /// Mark the current GcRoot suffix for later removal.
    fn save_specpdl_roots(&self) -> Self::RootMark;

    /// Push a GC root keeping `value` alive across Lisp calls.
    fn push_specpdl_root(&mut self, value: Value);

    /// Push an updatable GcRoot entry and return its handle.
    fn push_specpdl_root_slot(&mut self, value: Value) -> Self::RootSlot;

    /// Re-point an updatable root slot at a new value.
    fn set_specpdl_root_slot(&mut self, slot: &Self::RootSlot, value: Value);

    /// Remove only the GcRoot entries above `mark` (error unwinds pop
    /// roots together with the frame).
    fn restore_specpdl_roots(&mut self, mark: Self::RootMark);

    // -------------------------------------------------------------------------
    // display/system queries (stage-3 display-seam lane: C4b, C5, C15)
    // -------------------------------------------------------------------------
    // Empty until that series lands; nothing here may name display or
    // system types that edit does not already own.
}
