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
    Context, LispArgVec, SpecpdlCount, SpecpdlRootScopeState, SpecpdlRootSlot, VariableView,
};
use crate::buffer::{Buffer, BufferId, EditRuntime, LispCharPos1};
use crate::emacs_core::bookmark::BookmarkManager;
use crate::emacs_core::builtins::symbols::builtin_set_2;
use crate::emacs_core::display_metrics::{
    InvisibleRunContext, LineWrap, MeasuredMotion, MotionEngine, WindowGeometry,
};
use crate::emacs_core::edit_host::EditHost;
use crate::emacs_core::error::{EvalResult, Flow};
use crate::emacs_core::hook_runtime;
use crate::emacs_core::intern::SymId;
use crate::emacs_core::value::Value;
use crate::heap_types::LispString;

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

    fn variable_value(&self, id: SymId) -> Option<Value> {
        Context::variable_value(self, id)
    }

    fn variable_value_by_name(&self, name: &str) -> Option<Value> {
        Context::variable_value_by_name(self, name)
    }

    fn variable_value_in_buffer(&self, buf: Option<&Buffer>, id: SymId) -> Option<Value> {
        Context::variable_value_in_buffer(self, buf, id)
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

    fn bookmarks(&mut self) -> &mut BookmarkManager {
        &mut *self.bookmarks
    }

    fn inhibit_modification_hooks(&self) -> bool {
        use crate::emacs_core::symbol::SymbolRedirect;
        let sym = crate::emacs_core::editfns::inhibit_modification_hooks_symbol();
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
                    return Context::variable_value(self, sym)
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
                            buf.id,
                            buf.local_var_alist_value(),
                        )
                    {
                        return !value.is_unbound() && value.is_truthy();
                    }
                    return Context::variable_value(self, sym)
                        .is_some_and(|v| !v.is_unbound() && v.is_truthy());
                }
                SymbolRedirect::Varalias => {}
            }
        }
        let sym = EditHost::hook_symbol_by_id(self, sym);
        hook_runtime::hook_value_by_id(self, sym).is_some_and(|v| v.is_truthy())
    }

    fn apply(&mut self, function: Value, args: &[Value]) -> EvalResult {
        Context::apply(self, function, LispArgVec::from_slice(args))
    }

    fn funcall_general(&mut self, function: Value, args: &[Value]) -> EvalResult {
        Context::funcall_general(self, function, LispArgVec::from_slice(args))
    }

    fn symbol_mode(&self) -> crate::tagged::SymbolMode {
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
        crate::emacs_core::fileio::resolve_filename_lisp_for_eval(self, filename)
    }

    fn lock_file(&mut self, filename: &LispString) -> Result<(), Flow> {
        let _ = crate::emacs_core::filelock::lock_file(self, filename)?;
        Ok(())
    }

    fn unlock_file(&mut self, filename: &LispString) -> Result<(), Flow> {
        let _ = crate::emacs_core::filelock::unlock_file(self, filename)?;
        Ok(())
    }

    fn mark_buffer_redisplay(&mut self, buffer: BufferId) {
        Context::gnu_mark_buffer_redisplay(self, buffer)
    }

    fn invisible_source_run_end(
        &mut self,
        buffer: BufferId,
        byte_pos: usize,
        context: InvisibleRunContext,
    ) -> Result<Option<usize>, Flow> {
        crate::emacs_core::xdisp::invisible_source_run_end_byte(self, buffer, byte_pos, context)
    }

    fn invisible_value_hides_source(&mut self, value: Value) -> Result<bool, Flow> {
        crate::emacs_core::xdisp::invisible_status_for_value(self, value)
            .map(|status| status.hides_source())
    }

    fn motion_engine(&self) -> MotionEngine {
        crate::emacs_core::xdisp::motion::engine_for_context(self)
    }

    fn resolve_motion(
        &mut self,
        buffer: BufferId,
        window: Option<Value>,
        origin: LispCharPos1,
        rows: i64,
        goal_column: Option<i64>,
    ) -> Result<Option<MeasuredMotion>, Flow> {
        crate::emacs_core::xdisp::motion::resolve(
            self,
            crate::emacs_core::xdisp::motion::MotionRequest {
                buffer,
                window: window
                    .and_then(|value| value.as_window_id())
                    .map(crate::window::WindowId),
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
        crate::emacs_core::window_cmds::window_line_wrap(self, window, buffer, engine)
    }

    fn window_geometry(&mut self, window: Option<Value>) -> WindowGeometry {
        let window = window.unwrap_or(Value::NIL);
        let body_width =
            crate::emacs_core::window_cmds::builtin_window_body_width(self, &[window, Value::NIL])
                .ok()
                .and_then(|value| value.as_fixnum())
                .unwrap_or(0);
        let hscroll = crate::emacs_core::window_cmds::builtin_window_hscroll(self, &[window])
            .ok()
            .and_then(|value| value.as_fixnum())
            .unwrap_or(0);
        WindowGeometry {
            body_width,
            hscroll,
        }
    }

    fn char_width_table(&self) -> Option<Value> {
        crate::encoding::char_width_table_value(self)
    }

    fn character_width(&self, table: Option<Value>, code: u32) -> usize {
        crate::encoding::char_width_in_table(table, code)
    }

    fn word_wrap_policy(
        &self,
        buffer: &Buffer,
        mode: crate::emacs_core::word_wrap::WordWrapMode,
    ) -> crate::emacs_core::word_wrap::WordWrapPolicy {
        crate::window::word_wrap_policy_for_buffer(buffer, mode)
    }

    fn word_wrap_mode(&self, buffer: &Buffer) -> crate::emacs_core::word_wrap::WordWrapMode {
        // GNU `init_iterator` reaches WORD_WRAP from the buffer's `word-wrap`
        // only when `word-wrap-by-category` is also non-nil
        // (src/xdisp.c:3425-3426); the scan's fallback reads that flag.
        let categories = self
            .variable_value_in_buffer(
                Some(buffer),
                crate::window::WindowLayoutVariable::WordWrapByCategory.sym_id(),
            )
            .is_some_and(|value| value.is_truthy());
        if categories {
            crate::emacs_core::word_wrap::WordWrapMode::Category
        } else {
            crate::emacs_core::word_wrap::WordWrapMode::Whitespace
        }
    }
}
