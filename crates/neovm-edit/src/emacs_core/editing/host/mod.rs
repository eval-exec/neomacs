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

use crate::buffer::{Buffer, BufferId, EditRuntime, EmacsBytePos, EmacsByteRange, LispCharPos1};
use crate::emacs_core::bookmark::BookmarkManager;
use crate::emacs_core::display_metrics::{
    InvisibleRunContext, LineNumberDisplayTarget, LineWrap, MeasuredMotion, MotionEngine,
    WindowGeometry,
};
use crate::emacs_core::word_wrap::{WordWrapMode, WordWrapPolicy};
use neovm_value::error::{EvalResult, Flow};
use neovm_value::heap_types::LispString;
use neovm_value::intern::SymId;
use neovm_value::symbol::Obarray;
use neovm_value::tagged::SymbolMode;
use neovm_value::value::Value;

/// The variable reads one owner's edit phase may perform: the read
/// surface of the evaluator's variable view by pure `#[inline(always)]`
/// delegation, with no semantic change (PEER-RULING-STAGE3 addendum
/// 22:1xZ V.2). Only methods edit call sites use exist; writes stay on
/// the host (`set_variable` / `reset_variable_on_error`).
///
/// `Copy`, and `!Send`/`!Sync` (the evaluator's view asserts this), so a
/// view is re-created per edit phase and never outlives it: a stale view
/// cannot exist.
pub trait EditVariableRead<'a>: Copy {
    /// The symbol table the view reads. Edit needs it for symbol
    /// queries that are reads, not variable reads (`is_localized`,
    /// localization-era symbol inspection on the read-only paths).
    fn obarray(self) -> &'a Obarray;

    /// GNU `Vfoo`: the variable's value without a buffer, `None` when
    /// void.
    fn value(self, id: SymId) -> Option<Value>;

    /// [`Self::value`] by name.
    fn value_by_name(self, name: &str) -> Option<Value>;

    /// The variable's visible value in `buf`: the buffer's local binding
    /// when it has one, else the default, `None` when void.
    fn value_in_buffer(self, buf: Option<&Buffer>, id: SymId) -> Option<Value>;

    /// [`Self::value`], nil when void.
    fn value_or_nil(self, id: SymId) -> Value;
}

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

    /// The variable's DEFAULT-ONLY value, without a buffer: for a
    /// buffer-local variable the default cell, never the current buffer's
    /// binding; `None` when void. GNU `Fdefault_value` /
    /// `BVAR (&buffer_defaults, slot)`. Use
    /// [`Self::forwarded_variable_value`] for GNU `find_symbol_value` (the
    /// visible read); callers are counted by the default-value-only ratchet.
    /// C2.
    fn default_value_only(&self, id: SymId) -> Option<Value>;

    /// [`Self::default_value_only`] starting from the variable's name.
    /// C2.
    fn default_value_only_by_name(&self, name: &str) -> Option<Value>;

    /// The variable's visible value in `buf`: the buffer's local
    /// binding when it has one, else the default. C2.
    fn variable_value_in_buffer(&self, buf: Option<&Buffer>, id: SymId) -> Option<Value>;

    /// [`Self::variable_value_in_buffer`] for the current buffer (GNU's
    /// `BVAR (current_buffer, ...)` read). Added for the text layer's
    /// character-width policy (stage-3b T6 thin addition).
    fn variable_value_in_current_buffer(&self, id: SymId) -> Option<Value>;

    /// GNU `find_symbol_value` for a C-forwarded variable: the current
    /// buffer's forwarded/local binding (swapped into the C global by
    /// `set_buffer_internal_2`, src/buffer.c:2342ff) and the live dynamic
    /// binding, `None` when void. This is the value a `DEFVAR_*` C global
    /// holds. Callers must use this, never `default_value_only`, for the
    /// scanner/scan/search variables whose GNU C reads see buffer locals.
    /// Pure read. EDIT-HOST row (stage-3b forwarded-locals fix).
    fn forwarded_variable_value(&self, id: SymId) -> Option<Value>;

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

    /// GNU `Fset` by NAME: the full `set` subr semantics —
    /// buffer-forwarded slot write, runtime-binding cache sync, GC refresh
    /// and display-variable mark (`Context::set_variable`), NOT the
    /// canonical-default `store_variable_by_name`. Added for the text
    /// layer's `last-coding-system-used` writes (stage-3b T6 thin addition).
    fn set_variable_by_name(&mut self, name: &str, value: Value);

    /// GNU `Vfoo = VALUE` from C, by name: the canonical-default store
    /// with no alias resolution and no variable watchers — the obarray's
    /// `set_canonical_value` write. Every alias-resolving, watcher-running
    /// store is `set_variable`. C2.
    fn store_variable_by_name(&mut self, name: &str, value: Value);

    /// Read one of the undo-truncation variables the way GNU's C code
    /// sees it: a `DEFVAR_INT`/`DEFVAR_LISP` global, so no lexical
    /// binding shadows it, only the dynamic value visible in the current
    /// buffer counts, and void reads back nil. Feeds the edit layer's
    /// `UndoLimitBindings` adapter. C2.
    fn undo_truncation_variable(&self, name: &str) -> Value;

    /// Whether `inhibit-modification-hooks` is non-nil. GNU `insdel.c`
    /// reads the `DEFVAR_BOOL` flag in one load; the fast paths over
    /// the forwarding live on the host, where the symbol table is. C2.
    fn inhibit_modification_hooks(&self) -> bool;

    /// GNU's visible variable read by id in the lexical-then-visible
    /// order (`find_symbol_value` through the lexenv cache): the read
    /// the buffer-access property gate uses for its policy variables.
    /// Pure delegation, semantics unchanged. M3a.
    fn visible_variable_value_or_nil(&self, id: SymId) -> Value;

    /// The void-signalling visible read of `id` (GNU `symbol-value`
    /// with its void-variable error path folded into one call). Also
    /// the policy-symbol read casing, rectangle and field-constraint
    /// code use. Pure delegation. M3a.
    fn eval_symbol_by_id(&self, id: SymId) -> EvalResult;

    /// GNU `reset_buffer_local_variables` (buffer.c) for one buffer:
    /// the dormant branch resets directly; the projected-owner branch
    /// captures the owner bindings, reveals the projections, resets,
    /// and clears them. Includes the one audited legacy obarray write.
    /// The `kill-all-local-variables` Lisp entry drives this through
    /// the host after `change-major-mode-hook` runs. M3a.
    fn reset_buffer_locals(&mut self, buffer: BufferId, kill_permanent: bool);

    /// The epoch-checked string borrow (the `as_lisp_string` safety
    /// discipline, DIVERGENCES.md 161): `Some(&LispString)` only while the
    /// value is a live string in the current GC epoch. The borrow must not
    /// cross a Lisp call; copy the bytes out when it must survive one. C3.
    fn lisp_string<'a>(&'a self, value: Value) -> Option<&'a LispString>;

    /// [`Self::lisp_string`] with GNU's `CHECK_STRING` signal
    /// (`wrong-type-argument stringp`): the drop-in replacement for the
    /// argument-check helper at a site that must not hold the borrow
    /// across a safepoint. Like [`Self::lisp_string`], holding the
    /// returned borrow across a `&mut self` host call is a compile
    /// error — that is the safety property the API exists for. M3b.
    fn expect_lisp_string<'a>(&'a self, value: Value) -> Result<&'a LispString, Flow>;

    /// GNU's native error template path: doprnt translation with
    /// `text-quoting-style` applied (eval.c:2285-2287 through
    /// doprnt.c:493-499), returning the signalled `Flow`. The
    /// quoting-style resolution stays in the coding layer; edit hands
    /// over the template only. M3c.
    fn signal_c_error(&self, message: neovm_value::errors::CErrorMessage<'_>) -> Flow;

    /// The session's bookmark set (bookmark.el globals). The state is
    /// `WorldSlot`-owned on the host and does NOT move with the edit
    /// crate (STAGE3-ASSIGN WorldSlot rule, coordinated with jitcall
    /// C1); edit reaches it only through this accessor. The rectangle
    /// `WorldSlot` needs no accessor: only core code touches it.
    fn bookmarks(&mut self) -> &mut BookmarkManager;

    // --- current-buffer switches and GC inhibition: runtime state the
    // edit layer drives at GC-time compaction (GNU `compact_buffer`). ---

    /// Make `id` current without recording a buffer-change window point
    /// (GNU `set_buffer_internal` for internal switches); `Err` when
    /// `id` is not live.
    fn set_current_buffer_unrecorded(&mut self, id: BufferId) -> Result<(), Flow>;

    /// GNU `set_buffer_if_live` for an unwind: re-select `id` unless it
    /// was killed meanwhile or is already current.
    fn restore_current_buffer_if_live(&mut self, id: BufferId);

    /// GNU `record_unwind_current_buffer` around `f` with `buffer`
    /// current (textprop.c:1175-1184): no save entry when `buffer` is
    /// already current (bug#36190), `SaveCurrentBuffer` as the only
    /// specpdl entry pushed, the caller's buffer current again on every
    /// exit, and the scope's Drop discarding the suffix on a Rust
    /// unwind. The scope lives in the host's frame, so unwinding
    /// through `f` still drops it there; edit never sees the scope
    /// type. Inside `f`, edit state is reached through
    /// [`Self::edit_and_vars`] under the V phasing rules.
    fn with_current_buffer_scope(
        &mut self,
        buffer: BufferId,
        f: impl FnOnce(&mut Self) -> EvalResult,
    ) -> EvalResult;

    /// Run `f` with garbage collection inhibited (GNU holds
    /// `garbage_collection_inhibited` across `truncate_undo_list`'s
    /// outer-limit call, src/undo.c:296-298).
    fn with_gc_inhibited<T>(&mut self, f: impl FnOnce(&mut Self) -> T) -> T;

    // --- Tree-sitter edit notification (the parsers live above the edit
    // layer, so edit reaches them only through the host). Shapes mirror
    // the manager's methods 1:1; every type is value- or edit-owned. ---

    /// Whether any live parser still holds an editable tree for BUFFER.
    fn treesit_has_editable_tree(&self, buffer: BufferId) -> bool;

    /// Whether a parser edit began for BUFFER and has not finished.
    fn treesit_has_pending_edit(&self, buffer: BufferId) -> bool;

    /// Begin the parser edit for BUFFER covering BYTE_RANGE (GNU
    /// `treesit_notify_before_change`). No-op when BUFFER is not live:
    /// the manager reads the buffer's tick and restriction, which the
    /// host supplies from its own edit state.
    fn treesit_begin_buffer_edit(&mut self, buffer: BufferId, byte_range: EmacsByteRange);

    /// Fold START_BYTE into the parsers' line/column caches (GNU
    /// `treesit_notify_change`).
    fn treesit_note_buffer_change(&mut self, buffer: BufferId, start_byte: EmacsBytePos);

    /// Apply the pending parser edit and mark its trees for reparse (GNU
    /// `treesit_notify_after_change`). No-op when BUFFER is not live,
    /// exactly like the caller-side liveness guard it replaces.
    fn treesit_finish_buffer_edit(&mut self, buffer: BufferId, new_end_byte: EmacsBytePos);

    /// GNU `apply`: call a function value on already-evaluated
    /// arguments. C3.
    fn apply(&mut self, function: Value, args: &[Value]) -> EvalResult;

    /// GNU `funcall_general`: unified function dispatch, the tree
    /// walker's entry. C3.
    fn funcall_general(&mut self, function: Value, args: &[Value]) -> EvalResult;

    /// GNU `safe_funcall` (`eval.c`): call FUNCTION while suppressing
    /// signaled conditions, so a broken callback cannot recursively replace
    /// the operation already handling an error. `throw` and suspended-thread
    /// flow remain nonlocal exits, as in GNU. Added for the text layer's
    /// syntax-propertize hook (stage-3b T6.0 thin addition).
    fn safe_funcall(&mut self, function: Value, args: &[Value]) -> EvalResult;

    /// Whether VALUE is a callable non-special-form function (GNU
    /// `SUBRP`/`Ffuncall` dispatch class): the subr registry decides, so
    /// this is an eval service. Stage-3b C5 row, homed here per T6.0.
    fn subr_is_callable_function_value(&self, value: &Value) -> bool;

    /// Render the live Lisp backtrace for diagnostics (GNU `backtrace`
    /// shape, bounded to MAX_FRAMES). Added for the text layer's
    /// parse-partial-sexp observability hook (stage-3b T6.0 thin addition).
    fn lisp_backtrace(&mut self, max_frames: usize) -> String;

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

    /// Opaque specpdl depth (GNU `SPECPDL_INDEX`). `Copy`: a depth snapshot
    /// is an integer reading of a stack top, and callers pass one count to
    /// both the bind and the matching unwind.
    type SpecCount: Copy;

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

    // --- edit phases: the split borrow (stage-3 V ruling). A `&mut H`
    // cannot yield `(&mut H, &mut EditRuntime)` at once, so an edit
    // phase opens this pair instead; while either return lives, NO host
    // method is callable. Mixed bodies PHASE: host phase (calls above),
    // edit phase (this), alternating, with the original statement order
    // kept exactly. ---

    /// The owner's variable-read view for one edit phase
    /// (see [`EditVariableRead`]).
    type Vars<'a>: EditVariableRead<'a> + Copy
    where
        Self: 'a;

    /// The edit phase: the edit runtime and its variable-read view, from
    /// DISJOINT field borrows. Open in the narrowest scope the body
    /// allows.
    fn edit_and_vars(&mut self) -> (&mut EditRuntime, Self::Vars<'_>);

    // -------------------------------------------------------------------------
    // display/system queries (stage-3 display-seam lane: C4b, C5, C15)
    // -------------------------------------------------------------------------
    // Every type here is value- or edit-owned; no display or system type
    // crosses the trait.

    /// Resolve FILENAME against the host's default directory
    /// (GNU `expand-file-name`'s default-directory walk). C15.
    fn resolve_filename(&self, filename: &LispString) -> LispString;

    /// Take the file lock for FILENAME through file-name handler
    /// dispatch and prompting (GNU `Flock_file`). C15.
    fn lock_file(&mut self, filename: &LispString) -> Result<(), Flow>;

    /// Release the file lock for FILENAME through file-name handler
    /// dispatch (GNU `Funlock_file`). C15.
    fn unlock_file(&mut self, filename: &LispString) -> Result<(), Flow>;

    /// Publish GNU `bset_redisplay` for BUFFER at mutation time: mark the
    /// shared text root and raise the windows scope when a second window
    /// shows it or it is not the selected buffer. C4b.
    fn mark_buffer_redisplay(&mut self, buffer: BufferId);

    /// Refresh the typed window position caches derived from the markers
    /// of BUFFER after a character change (GNU keeps the caches inside
    /// its markers; Neomacs derives them, at the common post-edit
    /// boundary `signal_after_change` is). C4b.
    fn sync_window_positions(&mut self, buffer: BufferId);

    /// The first visible byte of the invisible run at BYTE_POS, or `None`
    /// when the position is visible (GNU `skip_invisible`). C5.
    fn invisible_source_run_end(
        &mut self,
        buffer: BufferId,
        byte_pos: usize,
        context: InvisibleRunContext,
    ) -> Result<Option<usize>, Flow>;

    /// Whether a Lisp `invisible` property value hides its text under the
    /// display walk's ellipsis rules. C5.
    fn invisible_value_hides_source(&mut self, value: Value) -> Result<bool, Flow>;

    /// The motion engine this session's `vertical-motion` uses
    /// (GNU `if (noninteractive)`, src/indent.c:2280). C5.
    fn motion_engine(&self) -> MotionEngine;

    /// Resolve motion from retained display rows or the canonical row
    /// producer; `None` when the batch engine answers without display rows.
    /// C5.
    fn resolve_motion(
        &mut self,
        buffer: BufferId,
        window: Option<Value>,
        origin: LispCharPos1,
        rows: i64,
        goal_column: Option<i64>,
    ) -> Result<Option<MeasuredMotion>, Flow>;

    /// The line-wrap method of BUFFER in WINDOW under ENGINE (GNU
    /// `init_iterator`'s TRUNCATE/WINDOW_WRAP/WORD_WRAP choice). C5.
    fn window_line_wrap(
        &mut self,
        buffer: BufferId,
        window: Option<Value>,
        engine: MotionEngine,
    ) -> LineWrap;

    /// Body width and horizontal scroll of WINDOW (the selected window when
    /// `None`), as raw fixnums; a failed lookup yields the pre-metric
    /// defaults the edit callers already apply. C5.
    fn window_geometry(&mut self, window: Option<Value>) -> WindowGeometry;

    /// The live `char-width-table` value (GNU `Vchar_width_table`), `None`
    /// when the current table is not a char table. C5.
    fn char_width_table(&self) -> Option<Value>;

    /// Display width of character CODE under the char-width TABLE, falling
    /// back to GNU's built-in width for the code. C5.
    fn character_width(&self, table: Option<Value>, code: u32) -> usize;

    /// Project BUFFER's word-wrap policy for MODE (GNU's category
    /// `char_can_wrap_*` ranges), observing the per-epoch cache; `None`
    /// when BUFFER is not live. The buffer comes by id, not reference: a
    /// generic host caller cannot hold a `&Buffer` borrowed from the host
    /// it is about to call. C5.
    fn word_wrap_policy(&self, buffer: BufferId, mode: WordWrapMode) -> Option<WordWrapPolicy>;

    /// The wrap mode of the edit layer's screen-line scan for BUFFER: GNU
    /// `init_iterator` consults the buffer's `word-wrap-by-category` flag
    /// before it can produce `WORD_WRAP` rows; `None` when BUFFER is not
    /// live. C5.
    fn word_wrap_mode(&self, buffer: BufferId) -> Option<WordWrapMode>;

    /// The selected window's line-number measurement: its buffer, the frame's
    /// character cell width, and its visible row count, or `None` when no live
    /// window has a buffer. C5.
    fn line_number_display_target(&self) -> Option<LineNumberDisplayTarget>;

    /// GNU `force-mode-line-update` with a nil argument: mark the current
    /// buffer's mode line for the next redisplay. C4b.
    fn force_mode_line_update(&mut self) -> EvalResult;

    /// GNU `wset_update_mode_line` broadcast: mark every window's chrome
    /// (mode line, header line, menu bar) for redisplay. C4b.
    fn mark_chrome_dirty_all(&mut self);

    /// Raise GNU's global `update_mode_lines`: mark every window's chrome
    /// and request the menu-bar rebuild. C4b.
    fn request_global_mode_line_update(&mut self);

    /// GNU `bset_update_mode_line` for BUFFER: mark shared text and the
    /// windows showing it, and request the menu rebuild. C4b.
    fn gnu_mark_buffer_mode_line(&mut self, buffer: BufferId);
}
