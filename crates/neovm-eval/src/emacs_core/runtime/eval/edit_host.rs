//! The one production `EditHost` implementation (stage-3 R0).
//!
//! The edit layer declares `EditHost` (`emacs_core::edit_host`) and
//! calls its methods generically; the evaluator implements it for
//! `Context` right here, so the trait never needs a second impl and the
//! edit side never names `Context`. The C13 slice delegates to
//! `hook_runtime`, which stays `pub(crate)` in core (PEER-RULING-STAGE3
//! C13: nothing is installed, so there is no PC2 row). The C2 slice is
//! the variable surface: reads over `VariableView`, `set_variable` the
//! `set` subr body, `reset_variable_on_error` GNU `reset_var_on_error`
//! (`insdel.c`), and the `inhibit-modification-hooks` flag read whose
//! forwarding fast paths live with the symbol table. The C3 slice is
//! the funcall family (`apply`, `funcall_general`), symbol mode,
//! lexical binding, and the GNU-shaped opaque specpdl API whose
//! `SpecCount`/`RootMark`/`RootSlot` tokens are minted only here
//! (`pub struct`, private fields, no public constructor).

use super::{
    Context, CurrentBufferScope, LispArgVec, SpecpdlCount, SpecpdlRootScopeState, SpecpdlRootSlot,
    VariableView,
};
#[doc(hidden)]
pub use crate::emacs_core::builtins::symbols::builtin_set_2;
use crate::emacs_core::hook_runtime;
use neovm_edit::buffer::{
    Buffer, BufferId, EditRuntime, EmacsBytePos, EmacsByteRange, LispCharPos1,
};
use neovm_edit::emacs_core::bookmark::BookmarkManager;
use neovm_edit::emacs_core::display_metrics::{
    InvisibleRunContext, LineNumberDisplayTarget, LineWrap, MeasuredMotion, MotionEngine,
    MotionRequest, WindowGeometry,
};
use neovm_edit::emacs_core::edit_host::EditHost;
use neovm_value::error::{EvalResult, Flow};
use neovm_value::heap_types::LispString;
use neovm_value::intern::SymId;
use neovm_value::value::Value;

impl EditHost for Context {
    fn hook_symbol_by_id(&self, hook_sym: SymId) -> SymId {
        hook_runtime::hook_symbol_by_id(self, hook_sym)
    }

    fn hook_symbol_by_name(&self, hook_name: &str) -> SymId {
        hook_runtime::hook_symbol_by_name(self, hook_name)
    }

    fn hook_value_by_id(&self, hook_sym: SymId) -> Option<Value> {
        hook_runtime::hook_value_by_id(self, hook_sym)
    }

    fn run_hook_value(
        &mut self,
        hook_sym: SymId,
        hook_value: Value,
        hook_args: &[Value],
        inherit_global: bool,
    ) -> EvalResult {
        hook_runtime::run_hook_value(self, hook_sym, hook_value, hook_args, inherit_global)
    }

    fn run_hook_value_until_failure(
        &mut self,
        hook_sym: SymId,
        hook_value: Value,
        hook_args: &[Value],
        inherit_global: bool,
    ) -> EvalResult {
        hook_runtime::run_hook_value_until_failure(
            self,
            hook_sym,
            hook_value,
            hook_args,
            inherit_global,
        )
    }

    fn default_value_only(&self, id: SymId) -> Option<Value> {
        Context::default_value_only(self, id)
    }

    fn default_value_only_by_name(&self, name: &str) -> Option<Value> {
        Context::default_value_only_by_name(self, name)
    }

    fn variable_value_in_buffer(&self, buf: Option<&Buffer>, id: SymId) -> Option<Value> {
        Context::variable_value_in_buffer(self, buf, id)
    }

    fn variable_value_in_current_buffer(&self, id: SymId) -> Option<Value> {
        Context::variable_value_in_current_buffer(self, id)
    }

    fn forwarded_variable_value(&self, id: SymId) -> Option<Value> {
        Context::builtin_var_value(self, id)
    }

    fn default_value(&self, id: SymId) -> Option<Value> {
        self.variables().default_value(id)
    }

    fn boundp(&self, id: SymId) -> bool {
        self.variables().boundp(id)
    }

    fn set_variable(&mut self, symbol: Value, value: Value) -> EvalResult {
        builtin_set_2(self, symbol, value)
    }

    fn reset_variable_on_error(&mut self, sym: SymId, value: Value) {
        // A failed store simply leaves the old binding: GNU's C store
        // cannot fail, and the reset is best-effort error recovery.
        let _ = Context::try_set_runtime_binding_by_id(self, sym, value);
    }

    fn store_variable_by_name(&mut self, name: &str, value: Value) {
        Context::store_variable_by_name(self, name, value)
    }

    fn set_variable_by_name(&mut self, name: &str, value: Value) {
        // The `set` subr semantics (Context::set_variable), not the
        // canonical-default store above.
        Context::set_variable(self, name, value)
    }

    fn undo_truncation_variable(&self, name: &str) -> Value {
        Context::undo_truncation_variable(self, name)
    }

    fn bookmarks(&mut self) -> &mut BookmarkManager {
        &mut *self.bookmarks
    }

    fn set_current_buffer_unrecorded(&mut self, id: BufferId) -> Result<(), Flow> {
        Context::set_current_buffer_unrecorded(self, id)
    }

    fn restore_current_buffer_if_live(&mut self, id: BufferId) {
        Context::restore_current_buffer_if_live(self, id)
    }

    fn with_current_buffer_scope(
        &mut self,
        buffer: BufferId,
        f: impl FnOnce(&mut Self) -> EvalResult,
    ) -> EvalResult {
        // The scope is the existing `record_unwind_current_buffer` guard,
        // verbatim (PEER-RULING-STAGE3 addendum 00 item 2): `for_buffer`
        // skips the save entry when no switch happens (bug#36190), and
        // `finish`/Drop unbind exactly the suffix the scope pushed.
        let mut scope = CurrentBufferScope::for_buffer(self, buffer)?;
        let r = f(scope.context());
        scope.finish(r)
    }

    fn with_gc_inhibited<T>(&mut self, f: impl FnOnce(&mut Self) -> T) -> T {
        Context::with_gc_inhibited(self, f)
    }

    fn treesit_has_editable_tree(&self, buffer: BufferId) -> bool {
        self.treesit.has_editable_tree(buffer)
    }

    fn treesit_has_pending_edit(&self, buffer: BufferId) -> bool {
        self.treesit.has_pending_edit(buffer)
    }

    fn treesit_begin_buffer_edit(&mut self, buffer: BufferId, byte_range: EmacsByteRange) {
        // The buffer reference and the parser set are disjoint field borrows
        // of the same Context, exactly as the old direct call site read them.
        if let Some(buffer_data) = self.edit.buffers.get(buffer) {
            self.treesit
                .begin_buffer_edit(buffer, buffer_data, byte_range)
        }
    }

    fn treesit_note_buffer_change(&mut self, buffer: BufferId, start_byte: EmacsBytePos) {
        self.treesit.note_buffer_change(buffer, start_byte)
    }

    fn treesit_finish_buffer_edit(&mut self, buffer: BufferId, new_end_byte: EmacsBytePos) {
        if let Some(buffer_data) = self.edit.buffers.get(buffer) {
            self.treesit
                .finish_buffer_edit(buffer, buffer_data, new_end_byte)
        }
    }

    fn visible_variable_value_or_nil(&self, id: SymId) -> Value {
        Context::visible_variable_value_or_nil_by_id(self, id)
    }

    fn eval_symbol_by_id(&self, id: SymId) -> EvalResult {
        Context::eval_symbol_by_id(self, id)
    }

    fn reset_buffer_locals(&mut self, buffer: BufferId, kill_permanent: bool) {
        // GNU buffer.c reset_buffer_local_variables:
        // - preserves most always-local slots
        // - resets only a small fixed reset-on-kill-all subset
        // - clears conditional slot locals unless they are permanent-local
        // - walks local_var_alist for LOCALIZED entries (Phase 10E)
        if self.owner.is_dormant() {
            reset_buffer_local_variables(self, buffer, kill_permanent);
        } else {
            // Projected locals reset as the variables they project, and this
            // owner's bindings of the locals that go capture what they held.
            let before = self.owner_locals_before_reset(buffer);
            let projected = self.projected_locals_of(buffer);
            self.with_revealed_projections(&projected, |ctx| {
                reset_buffer_local_variables(ctx, buffer, kill_permanent)
            });
            self.owner_locals_reset(buffer, before);
        }
    }

    fn inhibit_modification_hooks(&self) -> bool {
        use neovm_value::symbol::SymbolRedirect;
        let sym = neovm_edit::emacs_core::editfns::inhibit_modification_hooks_symbol();
        // `inhibit-modification-hooks` is GNU `DEFVAR_BOOL` (`insdel.c:2575`):
        // the symbol forwards to a Boolean cell, `let` stores through it in place
        // (`store_symval_forwarding`), and reading it is the one flag load of
        // `do_symval_forwarding`'s `Lisp_Fwd_Bool` arm. Read that cell directly.
        // The Plainval arm covers a runtime that registered it as a plain
        // global. Anything else -- an alias, a buffer-local -- takes the
        // alias-resolving hook reader below, which walks the symbol table
        // several times and ran twice per text-property write when the
        // forwarded case fell through to it.
        if let Some(s) = self.obarray.get_by_id(sym) {
            match s.redirect() {
                SymbolRedirect::Forwarded => {
                    // Whichever forwarder this runtime installed for it -- the
                    // `DEFVAR_BOOL` flag, or an object cell when the symbol was
                    // adopted as a global object variable -- `load` is the
                    // one-load read of `do_symval_forwarding`; only a buffer
                    // object forwarder (never this symbol) answers `None`.
                    if let Some(value) = s.forwarded_descriptor().and_then(|fwd| fwd.load()) {
                        return value.is_truthy();
                    }
                }
                SymbolRedirect::Plainval => {
                    return Context::builtin_var_value(self, sym)
                        .is_some_and(|v| !v.is_unbound() && v.is_truthy());
                }
                // The arm a booted session takes: the code-conversion work buffer
                // makes the variable buffer-local in itself, exactly as GNU's
                // `make_conversion_work_buffer` does (`Fmake_local_variable`,
                // coding.c), so after the first conversion the symbol is a BLV
                // for good. GNU still reads it as one C load because the BLV
                // swaps the current buffer's value into the forwarded cell on
                // every buffer switch; here it is the current buffer's binding
                // if it has one, else the BLV's default -- `find_symbol_value`'s
                // order, at the cost of one gated buffer-local probe.
                SymbolRedirect::Localized => {
                    // The BLV's where-buffer cache is GNU's swapped-in cell: on a
                    // hit this is an epoch compare and a cons cdr, where the
                    // per-buffer binding map cost ~120 instructions a read.
                    if let Some(buf) = self.edit.buffers.current_buffer()
                        && let Some(value) = self.obarray.read_localized_symbol_for_buffer(
                            sym,
                            s,
                            buf.id(),
                            buf.local_var_alist_value(),
                        )
                    {
                        return !value.is_unbound() && value.is_truthy();
                    }
                    return Context::builtin_var_value(self, sym)
                        .is_some_and(|v| !v.is_unbound() && v.is_truthy());
                }
                SymbolRedirect::Varalias => {}
            }
        }
        let sym = EditHost::hook_symbol_by_id(self, sym);
        hook_runtime::hook_value_by_id(self, sym).is_some_and(|v| v.is_truthy())
    }

    fn lisp_string<'a>(&'a self, value: Value) -> Option<&'a LispString> {
        Context::lisp_string(self, value)
    }

    fn expect_lisp_string<'a>(&'a self, value: Value) -> Result<&'a LispString, Flow> {
        Context::expect_lisp_string(self, value)
    }

    /// GNU eval.c:2285-2287 routes native error templates through doprnt;
    /// doprnt.c:493-499 translates quotes according to text-quoting-style.
    /// Body moved verbatim from text/coding's inherent `impl Context`
    /// (stage-3b: coding then names no Context at all).
    #[cold]
    #[inline(never)]
    fn signal_c_error(&self, message: neovm_value::errors::CErrorMessage<'_>) -> Flow {
        let style = neovm_text::emacs_core::coding::effective_text_quoting_style(
            self.variables(),
            self.symbol_mode(),
        );
        neovm_value::error::signal(
            neovm_value::error::LispCondition::Error,
            vec![Value::string(
                neovm_text::emacs_core::coding::requote_c_error_message(message.template(), style),
            )],
        )
    }

    fn apply(&mut self, function: Value, args: &[Value]) -> EvalResult {
        Context::apply(self, function, LispArgVec::from_slice(args))
    }

    fn funcall_general(&mut self, function: Value, args: &[Value]) -> EvalResult {
        Context::funcall_general(self, function, LispArgVec::from_slice(args))
    }

    fn safe_funcall(&mut self, function: Value, args: &[Value]) -> EvalResult {
        Context::safe_funcall(self, function, LispArgVec::from_slice(args))
    }

    fn subr_is_callable_function_value(&self, value: &Value) -> bool {
        crate::emacs_core::subr_info::subr_is_callable_function_value(value)
    }

    fn lisp_backtrace(&mut self, max_frames: usize) -> String {
        Context::render_lisp_backtrace(self, max_frames)
    }

    fn symbol_mode(&self) -> neovm_value::tagged::SymbolMode {
        Context::symbol_mode(self)
    }

    fn set_lexical_binding(&mut self, enabled: bool) {
        Context::set_lexical_binding(self, enabled);
    }

    type SpecCount = SpecpdlCount;

    type RootMark = SpecpdlRootScopeState;

    type RootSlot = SpecpdlRootSlot;

    fn specpdl_index(&self) -> Self::SpecCount {
        SpecpdlCount {
            count: self.specpdl.len(),
        }
    }

    fn try_specbind_or_unwind_to(
        &mut self,
        count: Self::SpecCount,
        sym: SymId,
        value: Value,
    ) -> Result<(), Flow> {
        Context::try_specbind_or_unwind_to(self, count.count, sym, value)
    }

    fn unbind_to_with_result(&mut self, count: Self::SpecCount, result: EvalResult) -> EvalResult {
        Context::unbind_to_with_result(self, count.count, result)
    }

    fn save_specpdl_roots(&self) -> Self::RootMark {
        Context::save_specpdl_roots(self)
    }

    fn push_specpdl_root(&mut self, value: Value) {
        Context::push_specpdl_root(self, value)
    }

    fn push_specpdl_root_slot(&mut self, value: Value) -> Self::RootSlot {
        Context::push_specpdl_root_slot(self, value)
    }

    fn set_specpdl_root_slot(&mut self, slot: &Self::RootSlot, value: Value) {
        Context::set_specpdl_root_slot(self, slot, value)
    }

    fn restore_specpdl_roots(&mut self, mark: Self::RootMark) {
        Context::restore_specpdl_roots(self, mark)
    }

    type Vars<'a> = VariableView<'a>;

    fn edit_and_vars(&mut self) -> (&mut EditRuntime, Self::Vars<'_>) {
        // DISJOINT field borrows (ruling V.2): the edit runtime mutably,
        // the view's obarray and owner slot shared. Routed through
        // `Self::variables()` this would borrow all of `self` and the
        // pair could never coexist.
        (
            &mut self.edit,
            VariableView::new(&self.obarray, &self.owner),
        )
    }

    // --- display/system queries (C4b, C5, C15) ---

    fn resolve_filename(&self, filename: &LispString) -> LispString {
        crate::emacs_core::eval::lifecycle::resolve_filename(self, filename)
    }

    fn lock_file(&mut self, filename: &LispString) -> Result<(), Flow> {
        let _ = crate::emacs_core::eval::lifecycle::lock_file(self, filename)?;
        Ok(())
    }

    fn unlock_file(&mut self, filename: &LispString) -> Result<(), Flow> {
        let _ = crate::emacs_core::eval::lifecycle::unlock_file(self, filename)?;
        Ok(())
    }

    fn mark_buffer_redisplay(&mut self, buffer: BufferId) {
        Context::gnu_mark_buffer_redisplay(self, buffer)
    }

    fn sync_window_positions(&mut self, buffer: BufferId) {
        // Pre-split inherent body (core window/display.rs, moved by the eval
        // split): walk every frame and refresh the leaf window caches from
        // their markers.  The split carried only the free functions, so the
        // old `Context::sync_window_positions` path now names this trait
        // method itself and must not be kept.
        crate::emacs_core::eval::data::window_markers::sync_all_frames_for_buffer(
            self.frame_manager_mut(),
            buffer,
        )
    }

    fn invisible_source_run_end(
        &mut self,
        buffer: BufferId,
        byte_pos: usize,
        context: InvisibleRunContext,
    ) -> Result<Option<usize>, Flow> {
        super::display_query_hooks::invisible_source_run_end(self, buffer, byte_pos, context)
    }

    fn invisible_value_hides_source(&mut self, value: Value) -> Result<bool, Flow> {
        super::display_query_hooks::invisible_status(self, value)
            .map(|status| status.hides_source())
    }

    fn motion_engine(&self) -> MotionEngine {
        if self.noninteractive() {
            MotionEngine::ComputeMotion
        } else {
            MotionEngine::DisplayIterator
        }
    }

    fn resolve_motion(
        &mut self,
        buffer: BufferId,
        window: Option<Value>,
        origin: LispCharPos1,
        rows: i64,
        goal_column: Option<i64>,
    ) -> Result<Option<MeasuredMotion>, Flow> {
        super::display_query_hooks::resolve_motion(
            self,
            MotionRequest {
                buffer,
                window: window
                    .and_then(|value| value.as_window_id())
                    .map(neovm_edit::emacs_core::display_metrics::WindowId),
                origin,
                rows,
                goal_column,
            },
        )
    }

    fn window_line_wrap(
        &mut self,
        buffer: BufferId,
        window: Option<Value>,
        engine: MotionEngine,
    ) -> LineWrap {
        super::display_query_hooks::window_line_wrap(self, window, buffer, engine)
    }

    fn window_geometry(&mut self, window: Option<Value>) -> WindowGeometry {
        let window = window.unwrap_or(Value::NIL);
        let body_width = super::display_query_hooks::window_body_width(self, &[window, Value::NIL])
            .ok()
            .and_then(|value| value.as_fixnum())
            .unwrap_or(0);
        let hscroll = super::display_query_hooks::window_hscroll(self, &[window])
            .ok()
            .and_then(|value| value.as_fixnum())
            .unwrap_or(0);
        WindowGeometry {
            body_width,
            hscroll,
        }
    }

    fn char_width_table(&self) -> Option<Value> {
        neovm_text::encoding::char_width_table_value(self)
    }

    fn character_width(&self, table: Option<Value>, code: u32) -> usize {
        neovm_text::encoding::char_width_in_table(table, code)
    }

    fn word_wrap_policy(
        &self,
        buffer: BufferId,
        mode: neovm_edit::emacs_core::word_wrap::WordWrapMode,
    ) -> Option<neovm_edit::emacs_core::word_wrap::WordWrapPolicy> {
        let buffer = self.edit.buffers.get(buffer)?;
        Some(super::data::word_wrap::word_wrap_policy_for_buffer(
            buffer, mode,
        ))
    }

    fn word_wrap_mode(
        &self,
        buffer: BufferId,
    ) -> Option<neovm_edit::emacs_core::word_wrap::WordWrapMode> {
        // GNU `init_iterator` reaches WORD_WRAP from the buffer's `word-wrap`
        // only when `word-wrap-by-category` is also non-nil
        // (src/xdisp.c:3425-3426); the scan's fallback reads that flag.
        let buffer = self.edit.buffers.get(buffer)?;
        let categories = self
            .variable_value_in_buffer(
                Some(buffer),
                neovm_value::display_vocab::WindowLayoutVariable::WordWrapByCategory.sym_id(),
            )
            .is_some_and(|value| value.is_truthy());
        if categories {
            Some(neovm_edit::emacs_core::word_wrap::WordWrapMode::Category)
        } else {
            Some(neovm_edit::emacs_core::word_wrap::WordWrapMode::Whitespace)
        }
    }

    fn line_number_display_target(&self) -> Option<LineNumberDisplayTarget> {
        let frame = self.frames.selected_frame()?;
        let window = frame.find_window(frame.selected_window)?;
        let buffer = window.buffer_id()?;
        let char_width = frame.char_width.max(1.0).round() as i64;
        let char_height = frame.char_height.max(1.0).round() as i64;
        let visible_lines = match window {
            crate::emacs_core::eval::data::window_state::Window::Leaf { bounds, .. } => {
                ((bounds.height / char_height.max(1) as f32).floor() as i64).max(1)
            }
            crate::emacs_core::eval::data::window_state::Window::Internal { .. } => 1,
        };
        Some(LineNumberDisplayTarget {
            buffer,
            char_width,
            visible_lines,
        })
    }
    fn force_mode_line_update(&mut self) -> EvalResult {
        crate::emacs_core::builtins::misc_pure::builtin_force_mode_line_update(
            self,
            vec![Value::NIL],
        )
    }
    fn mark_chrome_dirty_all(&mut self) {
        Context::mark_chrome_dirty_all(self)
    }

    fn request_global_mode_line_update(&mut self) {
        Context::request_global_mode_line_update(self)
    }

    fn gnu_mark_buffer_mode_line(&mut self, buffer: BufferId) {
        Context::gnu_mark_buffer_mode_line(self, buffer)
    }
}

/// `kill-all-local-variables`' reset of BUFFER's locals. The
/// `EditHost::reset_buffer_locals` implementation below owns the
/// dormant/projected-owner dispatch around it; the one audited legacy
/// obarray write stays here in core.
#[inline(always)]
pub(crate) fn reset_buffer_local_variables(
    eval: &mut Context,
    buffer: BufferId,
    kill_permanent: bool,
) {
    let buffers = &mut eval.edit.buffers;
    // SAFETY: the major-mode hook has returned. This local-value/cache reset
    // invokes no Lisp and preserves canonical dispatch metadata and storage.
    let _ = unsafe {
        eval.obarray.with_legacy_value_write(|obarray| {
            buffers.clear_buffer_local_properties(buffer, obarray, kill_permanent)
        })
    };
}
