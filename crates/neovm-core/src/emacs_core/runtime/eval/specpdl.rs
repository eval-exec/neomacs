//! specbind / unbind_to: GNU specpdl-style dynamic binding, variable watchers, and the unwind that restores them.
//!
//! Moved out of `eval/mod.rs` unchanged; a child module of `eval` so it keeps
//! the same view of `Context` and the parent's private items (`use super::*`).

use super::*;

/// Push the entry MAKE builds onto SPECPDL, constructed in its final slot.
///
/// `Vec::push(entry)` evaluates the entry before its capacity check, and
/// because growing may unwind, LLVM parks the 32-byte `SpecBinding` in a stack
/// temporary written with narrow stores, then copies it into the slot with
/// 16-byte loads -- one store-forwarding block per push (1.02 per dynamic
/// `let`, one per `mapc` callback frame). Growing first and then writing
/// MAKE's value straight into the spare slot removes the temporary.
///
/// MAKE must build exactly one variant: branching between variants inside it
/// joins the aggregates and brings the temporary back. Branch outside and call
/// this once per arm.
#[inline(always)]
pub(crate) fn push_specpdl_entry_with(
    specpdl: &mut Vec<SpecBinding>,
    make: impl FnOnce() -> SpecBinding,
) {
    let len = specpdl.len();
    if len == specpdl.capacity() {
        grow_specpdl_for_push(specpdl);
    }
    // SAFETY: `len < capacity` was just ensured, so the slot at `len` is spare
    // capacity; it is fully written before the length grows over it. MAKE
    // cannot reach SPECPDL (it is exclusively borrowed here), and if MAKE
    // panics nothing has been published.
    unsafe {
        specpdl.as_mut_ptr().add(len).write(make());
        specpdl.set_len(len + 1);
    }
}

#[cold]
#[inline(never)]
fn grow_specpdl_for_push(specpdl: &mut Vec<SpecBinding>) {
    // Amortized doubling, as `Vec::push` grows.
    specpdl.reserve(1);
}

/// Stack-local root ownership with no work in its inactive state.
#[derive(Clone, Copy, Debug)]
enum UnwindVmRootsOwnership {
    Inactive,
    Owned {
        saved: VmRootScopeState,
        frame_count: usize,
    },
}

/// Stack-local ownership of VM roots through arbitrary Lisp or Rust unwind.
/// No Context/pdump layout changes; the exclusive mutator borrow stays local.
#[must_use = "finish the temporary root scope"]
pub(super) struct UnwindVmRootsScope<'a> {
    context: &'a mut Context,
    ownership: UnwindVmRootsOwnership,
    thread_confined: std::marker::PhantomData<*const ()>,
}
impl std::fmt::Debug for UnwindVmRootsScope<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UnwindVmRootsScope")
            .field("ownership", &self.ownership)
            .finish_non_exhaustive()
    }
}
impl<'a> UnwindVmRootsScope<'a> {
    #[inline]
    fn idle(context: &'a mut Context) -> Self {
        Self {
            context,
            ownership: UnwindVmRootsOwnership::Inactive,
            thread_confined: std::marker::PhantomData,
        }
    }
    #[inline]
    pub(super) fn enter(context: &'a mut Context) -> Self {
        let frame_count = context.vm_root_frames.len();
        let saved = context.save_vm_roots();
        Self {
            context,
            ownership: UnwindVmRootsOwnership::Owned { saved, frame_count },
            thread_confined: std::marker::PhantomData,
        }
    }
    #[inline]
    pub(super) fn context(&mut self) -> &mut Context {
        self.context
    }
    #[inline]
    fn restore(&mut self) {
        match std::mem::replace(&mut self.ownership, UnwindVmRootsOwnership::Inactive) {
            UnwindVmRootsOwnership::Inactive => {}
            UnwindVmRootsOwnership::Owned { saved, frame_count } => {
                // Captured frames are LIFO-owned: nested containment snapshots
                // retain this guard's frame; older-boundary recovery follows its
                // Drop. Surviving depth therefore still names the captured frame.
                // Recovery may already have removed it; never recreate it.
                self.context.vm_root_frames.truncate(frame_count);
                // A removed captured frame does not transfer this guard's
                // root span to a different surviving caller frame.
                if frame_count != 0
                    && self.context.vm_root_frames.len() == frame_count
                    && let Some(frame) = self.context.vm_root_frames.last_mut()
                    && let Some(len) = saved.saved_vm_root_frame_len
                {
                    frame.roots.truncate(len);
                }
            }
        }
    }
    #[inline]
    pub(super) fn finish(mut self) {
        self.restore();
    }
}
impl Drop for UnwindVmRootsScope<'_> {
    #[inline]
    fn drop(&mut self) {
        self.restore();
    }
}
static_assertions::assert_not_impl_any!(UnwindVmRootsScope<'static>: Send, Sync);

/// GNU's pending-quit bracket, including Rust panic exits from a watcher.
#[derive(Debug)]
#[must_use = "finish the pending-quit bracket"]
pub(super) struct UnwindQuitScope<'a> {
    roots: UnwindVmRootsScope<'a>,
    pending: Option<Value>,
}
impl<'a> UnwindQuitScope<'a> {
    #[inline]
    pub(super) fn enter(context: &'a mut Context) -> Self {
        let pending = Some(context.quit_flag_value());
        let roots = if pending.is_some_and(|value| !value.is_nil()) {
            UnwindVmRootsScope::enter(context)
        } else {
            UnwindVmRootsScope::idle(context)
        };
        let mut guard = Self { roots, pending };
        if let Some(quitf) = pending.filter(|value| !value.is_nil()) {
            guard.roots.context().push_vm_frame_root(quitf);
            guard.roots.context().set_quit_flag_value(Value::NIL);
        }
        guard
    }
    #[inline]
    pub(super) fn context(&mut self) -> &mut Context {
        self.roots.context()
    }
    #[inline]
    fn restore(&mut self) {
        if let Some(quitf) = self.pending.take()
            && !quitf.is_nil()
            && self.roots.context().quit_flag_value().is_nil()
        {
            self.roots.context().set_quit_flag_value(quitf);
        }
    }
    #[inline]
    pub(super) fn finish(mut self, result: EvalResult) -> EvalResult {
        self.restore();
        result
    }
    #[inline]
    fn finish_unbind(mut self, result: Result<(), Flow>) -> Result<(), Flow> {
        self.restore();
        result
    }
}
impl Drop for UnwindQuitScope<'_> {
    #[inline]
    fn drop(&mut self) {
        self.restore();
    }
}
static_assertions::assert_not_impl_any!(UnwindQuitScope<'static>: Send, Sync);

/// Keeps one popped entry's native storage recovery armed during Lisp cleanup.
/// Normal Flow exits preserve GNU's existing popped-entry semantics; only a
/// Rust panic replays the owned entry through the storage-only discarder.
#[derive(Debug)]
#[must_use = "finish the popped binding after normal or signaled cleanup"]
struct PoppedBindingScope<'a> {
    roots: UnwindVmRootsScope<'a>,
    recovery: Option<SpecBinding>,
    count: usize,
    lexical_environment: Option<Value>,
    watcher_owner: Option<SymId>,
}
impl<'a> PoppedBindingScope<'a> {
    #[deny(clippy::wildcard_enum_match_arm)]
    fn enter(context: &'a mut Context, binding: SpecBinding) -> Self {
        let count = context.specpdl.len().saturating_sub(1);
        let lexical_environment =
            (!matches!(&binding, SpecBinding::LexicalEnv { .. })).then_some(context.lexenv);
        let watcher_owner = binding
            .let_bound_symbol()
            .filter(|sym| !context.active_variable_watchers.contains(sym));
        let mut guard = Self {
            roots: UnwindVmRootsScope::enter(context),
            recovery: Some(binding),
            count,
            lexical_environment,
            watcher_owner,
        };
        if let Some(environment) = lexical_environment {
            guard.roots.context().push_vm_frame_root(environment);
        }
        // This root copy cannot run Lisp. The original entry is still on the
        // specpdl until construction succeeds, so every capture is traced.
        let (roots, recovery) = (&mut guard.roots, &guard.recovery);
        if let Some(binding) = recovery {
            let context = roots.context();
            match binding {
                SpecBinding::Let { old_value, .. } | SpecBinding::LetDefault { old_value, .. } => {
                    if let Some(value) = old_value.get() {
                        context.push_vm_frame_root(value);
                    }
                }
                SpecBinding::LetLocal { old_value, .. } => context.push_vm_frame_root(*old_value),
                SpecBinding::LexicalEnv { old_lexenv } => context.push_vm_frame_root(*old_lexenv),
                SpecBinding::GcRoot { value } => context.push_vm_frame_root(*value),
                SpecBinding::UnwindProtect { forms, lexenv } => {
                    context.push_vm_frame_root(*forms);
                    context.push_vm_frame_root(*lexenv);
                }
                SpecBinding::SaveExcursion { marker, .. } => context.push_vm_frame_root(*marker),
                SpecBinding::NativeUnwind { action } => {
                    action.trace_roots(&mut |value| context.push_vm_frame_root(value))
                }
                SpecBinding::Backtrace { .. }
                | SpecBinding::Backtrace1 { .. }
                | SpecBinding::Backtrace2 { .. }
                | SpecBinding::BacktraceNative { .. }
                | SpecBinding::SaveCurrentBuffer { .. }
                | SpecBinding::SaveRestriction { .. }
                | SpecBinding::LoadsInProgress { .. }
                | SpecBinding::RequireStack { .. }
                | SpecBinding::Nop => {}
            }
        }
        guard
    }
    #[inline]
    fn context(&mut self) -> &mut Context {
        self.roots.context()
    }
    #[inline]
    fn finish(mut self) {
        self.recovery = None;
        self.roots.restore();
    }
}
impl Drop for PoppedBindingScope<'_> {
    #[inline]
    fn drop(&mut self) {
        if let Some(binding) = self.recovery.take() {
            let context = self.roots.context();
            if context.specpdl.len() >= self.count {
                context.discard_specpdl_to(self.count);
                // The entry occupied this slot before being popped; draining
                // the child suffix makes the retained capacity sufficient.
                context.specpdl.push(binding);
                context.discard_specpdl_to(self.count);
                if let Some(environment) = self.lexical_environment {
                    context.lexenv = environment;
                }
                context.lexenv_assq_cache.clear();
                context.lexenv_special_cache.clear();
            }
            if let Some(symbol) = self.watcher_owner {
                context.active_variable_watchers.remove(&symbol);
            }
        }
    }
}
static_assertions::assert_not_impl_any!(PoppedBindingScope<'static>: Send, Sync);

/// Recovery is needed only while the popped entry can evaluate Lisp.
/// Pure storage variants do not clone entries or grow a root frame.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PoppedBindingPolicy {
    PureStorage,
    LispCleanup,
}

/// Normal Lisp unwind synchronizes runtime tables; Rust panic recovery only
/// restores existing storage. One mutator owns either restoration policy.
#[derive(Clone, Copy, Debug)]
enum SavedBufferRestore {
    Runtime,
    StorageOnly,
}

/// Whether the current mutator completed an excursion without a buffer switch
/// or window-point write. Only the completed case can retire its root entry.
#[derive(Clone, Copy, Debug)]
pub(super) enum ExcursionStorageRestore {
    Completed,
    NeedsRuntime,
}

impl Context {
    // Shared runtime write path for symbol-cell mutation. This mirrors GNU
    // `set_internal` after lexical handling has already been decided.

    // -----------------------------------------------------------------------
    // specbind / unbind_to — GNU Emacs specpdl-style dynamic variable binding
    // -----------------------------------------------------------------------

    /// [`push_specpdl_entry_with`] on this context's specpdl.
    #[inline(always)]
    pub(crate) fn push_specpdl_with(&mut self, make: impl FnOnce() -> SpecBinding) {
        crate::emacs_core::subr::leaf::debug_assert_no_leaf_active!("a specpdl push");
        push_specpdl_entry_with(&mut self.specpdl, make);
    }

    /// Abandon a scope without evaluating Lisp during Rust panic recovery.
    ///
    /// Normal and signaled Lisp exits must use `unbind_to_with_result`, which
    /// executes watchers and cleanup forms and propagates their nonlocal exits.
    /// This fallback restores storage and native saved state only. Abandoned
    /// Lisp/native cleanup payloads are dropped without invoking callbacks.
    /// A saved value rejected by a changed forwarding descriptor leaves that
    /// descriptor's last valid value installed; recovery must not panic or
    /// silently write an invalid value through a typed forwarding slot.
    ///
    /// This operation exclusively borrows one mutator's Context and specpdl;
    /// independent mutators own separate restoration stacks and projections.
    #[cold]
    #[inline(never)]
    #[deny(clippy::wildcard_enum_match_arm)]
    pub(crate) fn discard_specpdl_to(&mut self, count: usize) {
        while self.specpdl.len() > count {
            let Some(binding) = self.specpdl.pop() else {
                break;
            };
            match binding {
                SpecBinding::Let { sym_id, old_value }
                | SpecBinding::LetDefault {
                    sym_id, old_value, ..
                } => {
                    self.discard_restore_default_binding(sym_id, old_value.get());
                }
                SpecBinding::LetLocal {
                    sym_id,
                    old_value,
                    buffer_id,
                } => {
                    // A removed local binding stays removed, just as in GNU's
                    // do_one_unbind. ThreadSwitch stores suppress watchers.
                    if self
                        .local_binding_value_for_thread_switch(sym_id, buffer_id)
                        .is_some()
                    {
                        self.set_local_binding_for_thread_switch(sym_id, buffer_id, old_value);
                        if self.runtime_binding_has_projection(sym_id) {
                            // Restoring another buffer's local value must not
                            // replace this mutator's current-buffer projections.
                            let visible = self
                                .visible_runtime_variable_value_by_id_resolved(sym_id)
                                .unwrap_or(Value::NIL);
                            self.publish_runtime_binding_write_by_resolved_id(sym_id, visible);
                        }
                    }
                }
                SpecBinding::LexicalEnv { old_lexenv } => {
                    self.lexenv = old_lexenv;
                }
                SpecBinding::Backtrace { args, .. } => {
                    if let Some(index) = args.owned_index() {
                        // Unlike the normal LIFO pop this tolerates side-stack
                        // residue already healed by a panic-containment boundary.
                        self.backtrace_args_stack.truncate(index);
                    }
                }
                SpecBinding::SaveExcursion {
                    marker,
                    saved_window,
                    ..
                } => self.restore_save_excursion_with_policy(
                    marker,
                    saved_window,
                    SavedBufferRestore::StorageOnly,
                ),
                SpecBinding::SaveCurrentBuffer { buffer_id } => {
                    self.restore_current_buffer_storage_if_live(buffer_id);
                }
                SpecBinding::SaveRestriction { state } => {
                    self.buffers
                        .restore_saved_restriction_state(state.into_state());
                }
                SpecBinding::LoadsInProgress { len } => self.loads_in_progress.truncate(len),
                SpecBinding::RequireStack { len } => self.require_stack.truncate(len),
                SpecBinding::GcRoot { .. }
                | SpecBinding::Backtrace1 { .. }
                | SpecBinding::Backtrace2 { .. }
                | SpecBinding::BacktraceNative { .. }
                | SpecBinding::UnwindProtect { .. }
                | SpecBinding::NativeUnwind { .. }
                | SpecBinding::Nop => {}
            }
        }
        self.lexenv_assq_cache.clear();
        self.lexenv_special_cache.clear();
    }

    /// Storage-only restoration below watcher and constant-check policy.
    fn discard_restore_default_binding(&mut self, sym_id: SymId, saved: Option<Value>) {
        use crate::emacs_core::forward::ForwardSlot;

        // Follow the storage writer's bounded alias walk without constructing
        // a Lisp error during Rust recovery. A watcher may have left an alias
        // pointing at a built-in after this binding was recorded.
        let mut resolved = sym_id;
        for _ in 0..50 {
            let Some(target) = self
                .obarray
                .get_by_id(resolved)
                .and_then(|symbol| symbol.alias_target())
            else {
                break;
            };
            resolved = target;
        }
        let value = saved.unwrap_or(Value::UNBOUND);
        let forwarder = self.obarray.forwarder(resolved);
        // GNU set_internal refuses Qunbound for both built-in arms before
        // storing (data.c:1725-1728,1805-1808). Descriptor type checking alone
        // is insufficient: a Bool would accept UNBOUND as true, and an Obj
        // would store the sentinel. Keep valid storage, then publish its value.
        let builtin_unbind = value.is_unbound()
            && (forwarder.is_some()
                || self
                    .obarray
                    .blv(resolved)
                    .is_some_and(|blv| blv.fwd.is_some()));
        if !builtin_unbind {
            if let Some(forwarder) = forwarder {
                let Ok(store) = forwarder.store(value) else {
                    // A Lisp set-default-toplevel-value can change the saved
                    // value to one this descriptor cannot hold. Retain storage.
                    return;
                };
                match forwarder.slot() {
                    ForwardSlot::BufferObj(_) => {
                        if let Some(info) =
                            crate::buffer::buffer::lookup_buffer_slot_by_sym_id(resolved)
                        {
                            self.buffers
                                .set_buffer_default_slot(info, store.canonical_value());
                        }
                    }
                    ForwardSlot::Int(_)
                    | ForwardSlot::Bool(_)
                    | ForwardSlot::Obj(_)
                    | ForwardSlot::KboardObj(_) => {
                        forwarder.commit(store);
                    }
                }
            } else {
                // This preserves LOCALIZED defcell/valcell identity. These
                // storage setters do not evaluate watchers or Lisp forms.
                self.obarray.set_symbol_value_id(sym_id, value);
            }
        }
        if self.runtime_binding_has_projection(resolved) {
            // Host runtime fields mirror dynamic storage. A lexical binding
            // left active during panic recovery cannot shadow a C global.
            let visible = if let Some(blv) = self.obarray.blv(resolved) {
                // The ordinary localized reader refreshes its cache through
                // a buffer Value wrapper. Recovery reads the same canonical
                // current binding without allocating in an ambient TLS heap
                // or changing the retained BLV's defcell/valcell identity.
                self.buffers
                    .current_buffer()
                    .and_then(|buffer| buffer.local_variable_binding_cell(resolved))
                    .unwrap_or(blv.defcell)
                    .cons_cdr()
            } else {
                self.visible_runtime_variable_value_by_id_resolved(resolved)
                    .unwrap_or(Value::NIL)
            };
            let visible = if visible.is_unbound() {
                Value::NIL
            } else {
                visible
            };
            self.publish_runtime_binding_write_by_resolved_id(resolved, visible);
        }
        self.sync_user_test_gc_binding_by_id(resolved);
    }

    pub(super) fn run_specbind_watcher(
        &mut self,
        sym_id: SymId,
        value: Value,
        operation: &'static str,
    ) -> Result<(), Flow> {
        if !self.watchers.has_watchers(sym_id) {
            return Ok(());
        }
        let where_value = self.variable_watcher_where_for_set_by_id(sym_id);
        self.run_variable_watchers_by_id_with_where(
            sym_id,
            &value,
            &Value::NIL,
            operation,
            &where_value,
        )
    }

    /// GNU `set_internal` from its redirect switch on (`src/data.c:1712-1830`):
    /// the store a watched write makes once its watchers have run.
    ///
    /// GNU notifies before it looks at the redirect, so the value lands on
    /// whatever arm the watchers left: a `make-local-variable` in a `let`
    /// watcher sends the binding to the new buffer-local cell, and a
    /// `defvaralias` in an `unlet` watcher sends the restored value to the
    /// alias target. A caller that read the arm before running the watchers
    /// stores through here instead of on the arm it read.
    ///
    /// BINDFLAG is GNU's `Set_Internal_Bind`: only `Set` may create a binding
    /// for an automatically buffer-local variable, and VALUE `UNBOUND` (GNU
    /// `unbinding_p`) is refused for a built-in variable.
    #[cold]
    #[inline(never)]
    pub(crate) fn set_internal_after_watchers(
        &mut self,
        sym_id: SymId,
        value: Value,
        bindflag: crate::emacs_core::symbol::SetInternalBind,
    ) -> Result<(), Flow> {
        use crate::emacs_core::forward::ForwardSlot;
        use crate::emacs_core::symbol::{SetInternalBind, ValueCell};

        // `case SYMBOL_VARALIAS: sym = SYMBOL_ALIAS (sym); goto start;`
        let resolved = builtins::resolve_variable_alias_id_in_obarray(&self.obarray, sym_id)?;
        let unbinding = value.is_unbound();
        if unbinding {
            // "Built-in variable may not be unbound", named by the symbol the
            // caller wrote (`src/data.c:1723-1727`, `:1802-1806`).
            check_forwarded_unbind(&self.obarray, resolved, Value::from_sym_id(sym_id))?;
        }
        let cell = self.obarray.get_by_id(resolved).map(|sym| sym.value_cell());
        let stored = match cell {
            // Ordinary assignment storage is this switch for `SET_INTERNAL_SET`:
            // the local-if-set binding, the `let`-shadowed default, the
            // per-buffer slot flag.
            _ if bindflag == SetInternalBind::Set => {
                let checked = check_forwarded_store(
                    &self.obarray,
                    &self.buffers,
                    &self.specpdl,
                    resolved,
                    value,
                )?;
                let stored = checked.value();
                store_runtime_binding(
                    &mut self.obarray,
                    &mut self.buffers,
                    &self.custom,
                    &self.specpdl,
                    resolved,
                    checked,
                );
                stored
            }
            Some(ValueCell::Localized(_)) => {
                let stored = check_forwarded_store_at(
                    &self.obarray,
                    &self.buffers,
                    &self.specpdl,
                    resolved,
                    value,
                    ForwardStoreSite::Bind,
                )?
                .value();
                match self.buffers.current_buffer_id() {
                    Some(buf_id) => {
                        let (cur_val, alist) = match self.buffers.get(buf_id) {
                            Some(buf) => (Value::make_buffer(buf.id), buf.local_var_alist_value()),
                            None => (Value::NIL, Value::NIL),
                        };
                        // A `let` or its unwind never creates a binding: no
                        // binding here means the default cell.
                        let new_alist = self.obarray.set_internal_localized(
                            resolved, stored, cur_val, alist, bindflag, false,
                        );
                        if let Some(buf) = self.buffers.get_mut(buf_id) {
                            buf.replace_local_var_alist(new_alist);
                        }
                    }
                    None => self.obarray.set_symbol_value_id(resolved, stored),
                }
                stored
            }
            Some(ValueCell::Forwarded(fwd)) => match fwd.slot() {
                // `store_symval_forwarding` writes the current buffer's slot;
                // only `SET_INTERNAL_SET` touches the slot's local flag.
                ForwardSlot::BufferObj(buf_fwd) => {
                    let stored = match fwd.store(value) {
                        Ok(store) => store.canonical_value(),
                        Err(error) => return Err(forward_store_signal(error, value)),
                    };
                    if let Some(slot) = crate::buffer::buffer::BufferSlot::from_u16(buf_fwd.offset)
                        && let Some(buf_id) = self.buffers.current_buffer_id()
                        && let Some(buf) = self.buffers.get_mut(buf_id)
                    {
                        buf.slots[slot.index()] = stored;
                    }
                    stored
                }
                ForwardSlot::Int(_)
                | ForwardSlot::Bool(_)
                | ForwardSlot::Obj(_)
                | ForwardSlot::KboardObj(_) => {
                    let stored = check_forwarded_store_at(
                        &self.obarray,
                        &self.buffers,
                        &self.specpdl,
                        resolved,
                        value,
                        ForwardStoreSite::Bind,
                    )?
                    .value();
                    self.obarray.set_symbol_value_id(resolved, stored);
                    stored
                }
            },
            // The alias walk above ends on a non-alias cell.
            None | Some(ValueCell::Plain(_) | ValueCell::Alias(_)) => {
                self.obarray.set_symbol_value_id(resolved, value);
                value
            }
        };
        let visible = if unbinding { Value::NIL } else { stored };
        self.publish_runtime_binding_write_by_resolved_id(resolved, visible);
        self.sync_user_test_gc_binding_by_id(resolved);
        Ok(())
    }

    /// Save the current value of a special variable and set a new value.
    /// Matches GNU Emacs's `specbind` in eval.c:
    /// - Follows SYMBOL_VARALIAS to the final target
    /// - For buffer-local variables with a local binding: SPECPDL_LET_LOCAL
    /// - For buffer-local variables without local binding: SPECPDL_LET_DEFAULT
    /// - For plain variables: SPECPDL_LET
    /// GNU `specbind`'s PLAINVAL arm with `do_specbind`'s untrapped store, as
    /// one obarray visit: the swap refuses a watched, constant, aliased,
    /// buffer-local, forwarded, uninterned or host-projected symbol (`false`,
    /// nothing stored), and binds every other one with a specpdl push and
    /// one store. No Lisp runs and no safe point is reached, so the caller
    /// needs no root for `value`: the cell holds it from the swap on and the
    /// specpdl entry holds the old value. The JIT's `varbind` shim takes
    /// this before it roots anything, which is most of a `let` on a source
    /// load.
    #[inline]
    pub(crate) fn specbind_plain_untrapped_fast(&mut self, sym_id: SymId, value: Value) -> bool {
        let Some(old) = self.obarray.swap_plain_untrapped_value_id(sym_id, value) else {
            return false;
        };
        self.push_specpdl_with(|| SpecBinding::Let {
            sym_id,
            old_value: SavedBindingValue::from_plain(old),
        });
        true
    }

    pub(super) fn specbind_resolved(&mut self, sym_id: SymId, value: Value) -> Result<(), Flow> {
        // GNU `specbind` switches on the redirect first: a plain value cell is
        // a specpdl push and one store (`SET_SYMBOL_VAL`).  Every other shape
        // — alias, forwarded, buffer-local, the undo list — takes the full
        // path below.  The plain tail of that path is reproduced exactly: the
        // constant check already ran in `sf_let`, and watchers still fire.
        // GNU `specbind`'s PLAINVAL arm with `do_specbind`'s untrapped store,
        // as one obarray visit: the swap refuses a watched, constant,
        // aliased, buffer-local, forwarded, uninterned or host-projected
        // symbol, and each of those keeps the tiers below unchanged.
        if self.specbind_plain_untrapped_fast(sym_id, value) {
            return Ok(());
        }
        // A buffer-local variable whose BLV cache is loaded for this buffer,
        // or a forwarder holding its own value (P1.4 A3).
        if self.specbind_cached(sym_id, value) {
            return Ok(());
        }
        self.specbind_uncached(sym_id, value)
    }

    /// [`Self::specbind_resolved`] after both cached tiers have refused: the
    /// general `specbind`. The JIT's `varbind` shim, which tries the tiers
    /// itself before it roots VALUE, enters here directly.
    #[inline(never)]
    pub(crate) fn specbind_uncached(&mut self, sym_id: SymId, value: Value) -> Result<(), Flow> {
        if sym_id != buffer_undo_list_symbol()
            && let Some(sym) = self.obarray.get_by_id(sym_id)
            && let Some(old_plain) = sym.plain_value()
        {
            let old_value = SavedBindingValue::from_plain(old_plain);
            // GNU `specbind` on a plain cell: `SET_SYMBOL_VAL` when the
            // symbol is untrapped, `set_internal` (watchers) when it is
            // `SYMBOL_TRAPPED_WRITE`.  The flag sits on the slot in hand.
            let trapped =
                sym.trapped_write() == crate::emacs_core::symbol::SymbolTrappedWrite::Trapped;
            debug_assert_eq!(trapped, self.watchers.has_watchers(sym_id));
            self.push_specpdl_with(|| SpecBinding::Let { sym_id, old_value });
            if trapped {
                self.run_specbind_watcher(sym_id, value, "let")?;
                // `do_specbind` hands a trapped plain cell to `set_internal`,
                // which stores on the arm the watcher left
                // (`src/eval.c:3618-3622`).
                return self.set_internal_after_watchers(
                    sym_id,
                    value,
                    crate::emacs_core::symbol::SetInternalBind::Bind,
                );
            }
            let stored = self.obarray.store_plain_value_id(sym_id, value);
            debug_assert!(
                stored.is_ok(),
                "an untrapped cell left the plain arm unseen"
            );
            self.sync_cached_runtime_binding_by_id(sym_id, value);
            self.sync_user_test_gc_binding_by_id(sym_id);
            return Ok(());
        }
        let resolved =
            builtins::resolve_variable_alias_id_in_obarray(&self.obarray, sym_id).unwrap_or(sym_id);

        // `buffer-undo-list` is a per-buffer variable in GNU.  Neomacs stores
        // it in SharedUndoState instead of the generic buffer-local alist, so
        // dynamic binding must update that shared state directly.  This is
        // required for GNU's `with-silent-modifications`, which binds
        // `buffer-undo-list` to t so font-lock/jit-lock text-property changes
        // do not enter the user's undo history.
        if resolved == buffer_undo_list_symbol()
            && let Some(buf_id) = self.buffers.current_buffer_id()
        {
            let old_value = self
                .buffers
                .get(buf_id)
                .map(|buf| buf.get_undo_list())
                .unwrap_or(Value::NIL);
            self.push_specpdl_with(|| SpecBinding::LetLocal {
                sym_id: resolved,
                old_value,
                buffer_id: buf_id,
            });
            self.run_specbind_watcher(resolved, value, "let")?;
            let _ = self
                .buffers
                .set_buffer_local_property_by_sym_id(buf_id, resolved, value);
            self.sync_cached_runtime_binding_by_id(resolved, value);
            self.sync_user_test_gc_binding_by_id(resolved);
            return Ok(());
        }

        // ONE symbol fetch decides the arm, like GNU `specbind`'s redirect
        // switch over an in-hand `Lisp_Symbol *` (eval.c:3642). The POD facts
        // are captured so the obarray borrow ends before any mutation; every
        // arm below reuses them instead of re-fetching the symbol.
        use crate::emacs_core::symbol::SymbolRedirect;
        let (redirect, forwarded) = match self.obarray.get_by_id(resolved) {
            Some(sym) => (sym.redirect(), sym.forwarded_descriptor()),
            None => (SymbolRedirect::Plainval, None),
        };

        // FORWARDED BUFFER_OBJFWD specbind, separate from the legacy
        // LOCALIZED path. Mirrors GNU `specbind` SYMBOL_FORWARDED arm at
        // `eval.c:3641-3677`.
        {
            if let Some(fwd) = forwarded {
                if let Some(buf_fwd) = fwd.as_buffer_obj_fwd() {
                    let Some(slot) = crate::buffer::buffer::BufferSlot::from_u16(buf_fwd.offset)
                    else {
                        return Ok(());
                    };
                    let off = slot.index();
                    let flags_idx = buf_fwd.local_flags_idx;
                    let buf_id_opt = self.buffers.current_buffer_id();
                    let has_local = match buf_id_opt {
                        Some(id) => self
                            .buffers
                            .get(id)
                            .map(|buf| flags_idx < 0 || buf.slot_local_flag(slot))
                            .unwrap_or(false),
                        None => false,
                    };
                    if has_local {
                        // SPECPDL_LET_LOCAL — save the current
                        // per-buffer slot value, then overwrite. On
                        // unbind we restore via set_buffer_local
                        // which writes back to the slot.
                        let buf_id = buf_id_opt.expect("has_local implies current buffer");
                        let old_val = self
                            .buffers
                            .get(buf_id)
                            .map(|b| b.slots[off])
                            .unwrap_or(Value::NIL);
                        self.push_specpdl_with(|| SpecBinding::LetLocal {
                            sym_id: resolved,
                            old_value: old_val,
                            buffer_id: buf_id,
                        });
                        self.run_specbind_watcher(resolved, value, "let")?;
                        // `check_forwarded_store_at` at site=Bind with a local
                        // binding reduces to the descriptor's own typed store
                        // (the predicate check GNU does in
                        // `store_symval_forwarding`); everything else it
                        // derives — fwd, slot, has_local — is already in hand.
                        let stored = match fwd.store(value) {
                            Ok(store) => store.canonical_value(),
                            Err(error) => return Err(forward_store_signal(error, value)),
                        };
                        if let Some(buf) = self.buffers.get_mut(buf_id) {
                            buf.slots[off] = stored;
                            // Always-local slots need no flag
                            // change; conditional slots already
                            // have the bit set (has_local check).
                        }
                        return Ok(());
                    } else {
                        // SPECPDL_LET_DEFAULT — save old default,
                        // propagate the new value via
                        // set_buffer_default_slot. On unbind we
                        // propagate the saved default back.
                        let old_default = if off < self.buffers.buffer_defaults.len() {
                            Some(self.buffers.buffer_defaults[off])
                        } else {
                            Some(buf_fwd.default)
                        };
                        self.push_specpdl_with(|| SpecBinding::LetDefault {
                            sym_id: resolved,
                            old_value: SavedBindingValue::from_option(old_default),
                            buffer_id: SavedBufferId::from_option(buf_id_opt),
                        });
                        // GNU routes a BUFFER_OBJFWD default binding through
                        // data.c's `set_default_internal`; its watcher
                        // operation is `set` even though the write was caused
                        // by a `let`.
                        super::super::data::set_default_internal_resolved(
                            self,
                            resolved,
                            value,
                            crate::emacs_core::symbol::SetInternalBind::Bind,
                        )?;
                        return Ok(());
                    }
                }
            }
        }

        // Phase 10E: SYMBOL_LOCALIZED specbind. Mirrors GNU `specbind`
        // SYMBOL_LOCALIZED arm at `eval.c:3641-3677`:
        //
        //   1. Read the current value (forces BLV swap-in to current
        //      buffer).
        //   2. Tentatively record SPECPDL_LET_LOCAL with the captured
        //      value and buffer.
        //   3. If !blv_found(blv) (the swap-in landed on defcell, not
        //      a per-buffer alist entry), demote to SPECPDL_LET_DEFAULT.
        //   4. Call set_internal_localized(BIND) to write the new
        //      value into wherever the BLV cache currently points.
        if redirect == SymbolRedirect::Localized
            && let Some(buf_id) = self.buffers.current_buffer_id()
        {
            let (cur_val, alist) = match self.buffers.get(buf_id) {
                Some(buf) => (Value::make_buffer(buf.id), buf.local_var_alist_value()),
                None => (Value::NIL, Value::NIL),
            };
            // Force a swap so blv.found / blv.valcell match the
            // current buffer state. After this, blv.where_buf =
            // cur_val.
            let old_val = self
                .obarray
                .find_symbol_value_in_buffer(
                    resolved,
                    Some(buf_id),
                    cur_val,
                    alist,
                    None,
                    0u64,
                    None,
                )
                .unwrap_or(Value::NIL);
            let has_local_binding = self
                .obarray
                .has_per_buffer_binding(resolved, cur_val, alist);
            if has_local_binding {
                self.push_specpdl_with(|| SpecBinding::LetLocal {
                    sym_id: resolved,
                    old_value: old_val,
                    buffer_id: buf_id,
                });
            } else {
                self.push_specpdl_with(|| SpecBinding::LetDefault {
                    sym_id: resolved,
                    old_value: SavedBindingValue::from_option(Some(old_val)),
                    buffer_id: SavedBufferId::from_option(Some(buf_id)),
                });
            }
            self.run_specbind_watcher(resolved, value, "let")?;
            let stored = check_forwarded_store_at(
                &self.obarray,
                &self.buffers,
                &self.specpdl,
                resolved,
                value,
                ForwardStoreSite::Bind,
            )?
            .value();
            // Write the new value via set_internal_localized
            // with bindflag=Bind. Bind never auto-creates a new
            // alist entry, so a let on a non-buffer-local
            // LOCALIZED symbol writes to defcell.cdr (the
            // global default), matching GNU.
            let new_alist = self.obarray.set_internal_localized(
                resolved,
                stored,
                cur_val,
                alist,
                crate::emacs_core::symbol::SetInternalBind::Bind,
                false,
            );
            if let Some(buf) = self.buffers.get_mut(buf_id) {
                buf.replace_local_var_alist(new_alist);
            }
            self.sync_cached_runtime_binding_by_id(resolved, stored);
            self.sync_user_test_gc_binding_by_id(resolved);
            return Ok(());
        }

        // Plain value path (GNU: SYMBOL_PLAINVAL). A PLAINVAL symbol has no
        // forward descriptor (`assignment_forwarder` is None by redirect), so
        // the typed-store probe is pure overhead for it; non-buffer forwarded
        // symbols (Int/Bool/Obj/Kboard) still take it so `(let
        // ((gc-cons-threshold "x")) ...)` keeps signaling before the body.
        let old_value = self.obarray.symbol_value_id_copied(resolved);
        self.push_specpdl_with(|| SpecBinding::Let {
            sym_id: resolved,
            old_value: SavedBindingValue::from_option(old_value),
        });
        self.run_specbind_watcher(resolved, value, "let")?;
        let stored = if redirect == SymbolRedirect::Plainval {
            value
        } else {
            check_forwarded_store_at(
                &self.obarray,
                &self.buffers,
                &self.specpdl,
                resolved,
                value,
                ForwardStoreSite::Bind,
            )?
            .value()
        };
        self.obarray.set_symbol_value_id(resolved, stored);
        self.sync_cached_runtime_binding_by_id(resolved, stored);
        self.sync_user_test_gc_binding_by_id(resolved);
        Ok(())
    }

    /// GNU-compatible checked entry point for dynamic binding.
    ///
    /// GNU's `specbind` reaches the same `store_symval_forwarding` an ordinary
    /// `setq` does -- it calls `set_internal (..., SET_INTERNAL_BIND)` for
    /// every forwarded symbol (`src/eval.c:3641-3677`) -- which is why
    /// `(let ((undo-limit "x")) ...)` signals before the body ever runs.
    pub(crate) fn try_specbind(&mut self, sym_id: SymId, value: Value) -> Result<(), Flow> {
        self.specbind_resolved(sym_id, value)
    }

    /// Enter one dynamic binding inside an already-established specpdl scope.
    /// If binding itself exits nonlocally (for example from a variable
    /// watcher), drain this binding and every earlier entry in the scope before
    /// returning that flow. This is the fallible counterpart of GNU callers'
    /// `specbind` + surrounding `unbind_to` pattern.
    pub(crate) fn try_specbind_or_unwind_to(
        &mut self,
        scope_count: usize,
        sym_id: SymId,
        value: Value,
    ) -> Result<(), Flow> {
        match self.try_specbind(sym_id, value) {
            Ok(()) => Ok(()),
            Err(flow) => match self.unbind_to_with_result(scope_count, Err(flow)) {
                Err(flow) => Err(flow),
                Ok(_) => unreachable!("unwinding an error cannot produce a value"),
            },
        }
    }

    pub(super) fn restore_default_binding_by_id(
        &mut self,
        sym_id: SymId,
        old_value: Option<Value>,
        bindflag: crate::emacs_core::symbol::SetInternalBind,
    ) -> Result<(), Flow> {
        // GNU's do_one_unbind and thread switching both call data.c's
        // `set_default_internal`, with the bind flag carrying the policy
        // difference. The shared storage seam also republishes the now-visible
        // runtime value and invalidates retained redisplay state.
        let value = old_value.unwrap_or(Value::UNBOUND);
        super::super::data::set_default_internal_resolved(self, sym_id, value, bindflag)?;
        // The evaluator caches a few of these cells in its own fields (the
        // quit flags, `throw-on-input`, the eval-depth limit) so its hot
        // paths do not consult the obarray. The plain-cell restore in
        // `unbind_to_result` republishes them; this one restores a
        // LOCALIZED/FORWARDED cell and must do the same, or a cache keeps a
        // value the binding it mirrors has already given up.
        self.sync_cached_runtime_binding_by_id(sym_id, old_value.unwrap_or(Value::NIL));
        self.sync_user_test_gc_binding_by_id(sym_id);
        Ok(())
    }

    /// Restore all specpdl bindings back to `count`.
    /// Matches GNU Emacs's unbind_to() in eval.c.
    pub(crate) fn unbind_to(&mut self, count: usize) {
        // Recovery/invariant-only callers have no Lisp result channel, but
        // they must still drain the whole suffix if cleanup signals. Normal
        // evaluator/VM paths use `unbind_to_with_result` and propagate it.
        let _ = self.drain_unwind_to(count, Ok(Value::NIL));
    }

    pub(super) fn local_binding_value_for_thread_switch(
        &self,
        sym_id: SymId,
        buffer_id: crate::buffer::BufferId,
    ) -> Option<Value> {
        self.buffers
            .get(buffer_id)
            .and_then(|buf| buf.get_buffer_local_binding_by_sym_id(sym_id))
            .map(|binding| binding.as_value().unwrap_or(Value::UNBOUND))
    }

    pub(super) fn set_local_binding_for_thread_switch(
        &mut self,
        sym_id: SymId,
        buffer_id: crate::buffer::BufferId,
        value: Value,
    ) {
        use crate::emacs_core::symbol::SymbolRedirect;

        let is_localized = self
            .obarray
            .get_by_id(sym_id)
            .map(|s| s.redirect() == SymbolRedirect::Localized)
            .unwrap_or(false);
        if is_localized {
            // GNU do_one_unbind restores only an existing local binding
            // (eval.c:3871-3885), and set_internal writes that cell's cdr
            // (data.c:1790-1791). The buffer owns the canonical cons; a
            // valid BLV cache already points at it, and another buffer's
            // loaded cache stays valid. Avoid constructing a Buffer wrapper
            // through an ambient Context's TLS heap during panic recovery.
            if let Some(cell) = self
                .buffers
                .get(buffer_id)
                .and_then(|buffer| buffer.local_variable_binding_cell(sym_id))
            {
                cell.set_cdr(value);
            }
        } else if value.is_unbound() {
            let _ = self
                .buffers
                .set_buffer_local_void_property_by_sym_id(buffer_id, sym_id);
        } else {
            let _ = self
                .buffers
                .set_buffer_local_property_by_sym_id(buffer_id, sym_id, value);
        }
        self.sync_cached_runtime_binding_by_id(sym_id, value);
        self.sync_user_test_gc_binding_by_id(sym_id);
    }

    pub(super) fn swap_let_binding_for_thread_switch(&mut self, index: usize) -> Result<(), Flow> {
        let (sym_id, old_value, originally_default_binding) = match self.specpdl.get(index) {
            Some(SpecBinding::Let { sym_id, old_value }) => (*sym_id, old_value.get(), false),
            Some(SpecBinding::LetDefault {
                sym_id, old_value, ..
            }) => (*sym_id, old_value.get(), true),
            _ => return Ok(()),
        };
        // GNU rechecks the redirect on every thread switch. A plain binding
        // can become LOCALIZED/FORWARDED inside its dynamic extent; from that
        // point it must fall through to the default-value path instead of
        // swapping the current buffer's local value into the saved default.
        let still_plain = self.obarray.get_by_id(sym_id).is_none_or(|symbol| {
            symbol.redirect() == crate::emacs_core::symbol::SymbolRedirect::Plainval
        });
        let use_default_storage = originally_default_binding || !still_plain;
        let current_value = if use_default_storage {
            super::super::data::default_value_by_id(self, sym_id)
        } else {
            self.obarray.symbol_value_id_copied(sym_id)
        };
        if use_default_storage {
            self.restore_default_binding_by_id(
                sym_id,
                old_value,
                crate::emacs_core::symbol::SetInternalBind::ThreadSwitch,
            )?;
        } else {
            match old_value {
                Some(value) => {
                    self.obarray.set_symbol_value_id(sym_id, value);
                    self.sync_cached_runtime_binding_by_id(sym_id, value);
                    self.sync_user_test_gc_binding_by_id(sym_id);
                }
                None => {
                    self.obarray.makunbound_id(sym_id);
                    self.sync_cached_runtime_binding_by_id(sym_id, Value::NIL);
                    self.sync_user_test_gc_binding_by_id(sym_id);
                }
            }
        }
        // Commit the exchange only after the fallible forwarded/default store
        // succeeds.  GNU permits thread-switch unrewind to signal; retaining
        // the requested saved value lets the caller handle the error without
        // silently losing the binding it failed to install.
        match self.specpdl.get_mut(index) {
            Some(SpecBinding::Let {
                old_value: saved_value,
                ..
            })
            | Some(SpecBinding::LetDefault {
                old_value: saved_value,
                ..
            }) => {
                saved_value.set(current_value);
            }
            _ => {}
        }
        Ok(())
    }

    pub(super) fn swap_local_let_binding_for_thread_switch(&mut self, index: usize) {
        let (sym_id, old_value, buffer_id) = match self.specpdl.get(index) {
            Some(SpecBinding::LetLocal {
                sym_id,
                old_value,
                buffer_id,
            }) => (*sym_id, *old_value, *buffer_id),
            _ => return,
        };
        let Some(current_value) = self.local_binding_value_for_thread_switch(sym_id, buffer_id)
        else {
            if let Some(binding) = self.specpdl.get_mut(index) {
                *binding = SpecBinding::Nop;
            }
            return;
        };
        if let Some(SpecBinding::LetLocal { old_value, .. }) = self.specpdl.get_mut(index) {
            *old_value = current_value;
        }
        self.set_local_binding_for_thread_switch(sym_id, buffer_id, old_value);
    }

    pub(super) fn specpdl_unrewind_vars_for_thread_switch(
        &mut self,
        rewind: bool,
    ) -> Result<(), Flow> {
        let indices: Vec<usize> = if rewind {
            (0..self.specpdl.len()).collect()
        } else {
            (0..self.specpdl.len()).rev().collect()
        };
        let mut swapped = Vec::with_capacity(indices.len());
        for index in indices {
            if let Err(flow) = self.swap_let_binding_for_thread_switch(index) {
                // Thread switching is an exchange, so replaying every
                // completed exchange in reverse order restores both live
                // storage and the saved specpdl cells.  This matters when an
                // outer forwarded binding rejects its saved value after an
                // inner binding has already been exchanged: the old thread is
                // still current and must remain observably unchanged when the
                // signal is caught.
                for swapped_index in swapped.into_iter().rev() {
                    let rollback = self.swap_let_binding_for_thread_switch(swapped_index);
                    debug_assert!(
                        rollback.is_ok(),
                        "a successful thread-binding exchange must be reversible"
                    );
                    self.swap_local_let_binding_for_thread_switch(swapped_index);
                }
                self.lexenv_assq_cache.clear();
                self.lexenv_special_cache.clear();
                return Err(flow);
            }
            self.swap_local_let_binding_for_thread_switch(index);
            swapped.push(index);
        }
        self.lexenv_assq_cache.clear();
        self.lexenv_special_cache.clear();
        Ok(())
    }

    /// GNU `specpdl_unrewind` (eval.c:4134-4234): swap each entry's saved
    /// state with the live state it protects, in place — the pointer-reversal
    /// trick `backtrace-eval` uses to evaluate a form under an earlier frame's
    /// bindings, and the same exchange thread switching performs on a whole
    /// stack. UNWIND walks the suffix top-down, the rewind walks it bottom-up;
    /// every arm is its own inverse, so the two passes restore exactly.
    ///
    /// Arms follow GNU: an excursion entry re-captures the current state into
    /// itself and then restores the saved one (`save_excursion_save` +
    /// `save_excursion_restore`, eval.c:4167-4175); save-current-buffer swaps
    /// like the `set_buffer_if_live` special case (eval.c:4152-4159); let
    /// entries use the thread-switch exchange. GNU's FIXME stands: Lisp
    /// unwind-protect cleanups and restrictions have no rewind, so they stay
    /// untouched in both directions.
    pub(crate) fn specpdl_swap_suffix_for_backtrace_eval(
        &mut self,
        distance: usize,
        unwind: bool,
    ) -> Result<(), Flow> {
        let top = self.specpdl.len();
        let Some(base) = top.checked_sub(distance) else {
            return Ok(());
        };
        let indices: Vec<usize> = (base..top).collect();
        let indices = if unwind {
            indices.into_iter().rev().collect::<Vec<_>>()
        } else {
            indices
        };
        let mut completed = Vec::new();
        for index in indices {
            if let Err(flow) = self.swap_backtrace_eval_entry(index) {
                // Each completed swap is its own inverse; replaying them in
                // reverse completion order restores both stacks, exactly as
                // the thread-switch exchange does when a forwarded store
                // rejects its saved value mid-exchange.
                for swapped_index in completed.into_iter().rev() {
                    let rollback = self.swap_backtrace_eval_entry(swapped_index);
                    debug_assert!(
                        rollback.is_ok(),
                        "a completed backtrace-eval swap must be reversible"
                    );
                }
                self.lexenv_assq_cache.clear();
                self.lexenv_special_cache.clear();
                return Err(flow);
            }
            completed.push(index);
        }
        self.lexenv_assq_cache.clear();
        self.lexenv_special_cache.clear();
        Ok(())
    }

    fn swap_backtrace_eval_entry(&mut self, index: usize) -> Result<(), Flow> {
        // Copy the payload out first: the match's borrow of `specpdl` must end
        // before the restore calls below take `&mut self`.
        enum Swap {
            Excursion(Value, ExcursionWindow),
            CurrentBuffer(crate::buffer::BufferId),
            LexicalEnv,
            Let,
            LetLocal,
        }
        let swap = match self.specpdl.get(index) {
            Some(SpecBinding::SaveExcursion {
                marker,
                saved_window,
                ..
            }) => Swap::Excursion(*marker, *saved_window),
            Some(SpecBinding::SaveCurrentBuffer { buffer_id }) => Swap::CurrentBuffer(*buffer_id),
            Some(SpecBinding::LexicalEnv { .. }) => Swap::LexicalEnv,
            Some(SpecBinding::Let { .. } | SpecBinding::LetDefault { .. }) => Swap::Let,
            Some(SpecBinding::LetLocal { .. }) => Swap::LetLocal,
            _ => return Ok(()),
        };
        match swap {
            Swap::Excursion(old_marker, old_window) => {
                // GNU re-captures into the entry first (`save_excursion_save`):
                // a fresh marker at the current buffer's point, plus the
                // selected window when it displays that buffer. The old
                // marker stays traceable in its slot until the write lands.
                let Some(buffer_id) = self.buffers.current_buffer_id() else {
                    return Ok(());
                };
                let (new_marker, _) = super::super::marker::make_registered_point_marker(
                    &mut self.buffers,
                    buffer_id,
                )
                .expect("the current buffer is live, so its point marker registers");
                let new_window = super::ExcursionWindow::capture(&self.frames, buffer_id);
                if let Some(entry) = self.specpdl.get_mut(index) {
                    *entry = SpecBinding::SaveExcursion {
                        _saved_buffer_id: buffer_id,
                        saved_window: new_window,
                        marker: new_marker,
                    };
                }
                // The popped pair is only on the stack now; root it across the
                // restore, which may allocate window markers.
                let root_scope = self.save_vm_roots();
                self.push_vm_frame_root(old_marker);
                self.restore_save_excursion(old_marker, old_window);
                self.restore_vm_roots(root_scope);
            }
            Swap::CurrentBuffer(old_buffer_id) => {
                // eval.c:4152-4159: record the current buffer into the entry
                // and restore the saved one if it is still live.
                let current = self.buffers.current_buffer_id();
                if let (Some(entry), Some(current_buffer_id)) =
                    (self.specpdl.get_mut(index), current)
                {
                    *entry = SpecBinding::SaveCurrentBuffer {
                        buffer_id: current_buffer_id,
                    };
                }
                self.restore_current_buffer_if_live(old_buffer_id);
            }
            Swap::LexicalEnv => {
                // GNU binds internal-interpreter-environment with `specbind`,
                // so its swap is the plain LET exchange; this port keeps a
                // dedicated kind for the same live/saved exchange.
                if let Some(SpecBinding::LexicalEnv { old_lexenv }) = self.specpdl.get_mut(index) {
                    let saved = *old_lexenv;
                    *old_lexenv = self.lexenv;
                    self.lexenv = saved;
                }
            }
            Swap::Let => self.swap_let_binding_for_thread_switch(index)?,
            Swap::LetLocal => {
                self.swap_let_binding_for_thread_switch(index)?;
                self.swap_local_let_binding_for_thread_switch(index);
            }
        }
        Ok(())
    }

    pub(crate) fn suspend_dynamic_bindings_for_thread_switch(
        &mut self,
    ) -> Result<ThreadDynamicBindingToken, Flow> {
        let lexenv = std::mem::replace(&mut self.lexenv, Value::NIL);
        if let Err(flow) = self.specpdl_unrewind_vars_for_thread_switch(false) {
            self.lexenv = lexenv;
            self.lexenv_assq_cache.clear();
            self.lexenv_special_cache.clear();
            return Err(flow);
        }
        let suspended_depth = self.suspended_thread_bindings.len();
        self.suspended_thread_bindings
            .push(ThreadDynamicBindingState {
                lexenv,
                specpdl: std::mem::take(&mut self.specpdl),
                condition_stack: std::mem::take(&mut self.condition_stack),
            });
        Ok(ThreadDynamicBindingToken { suspended_depth })
    }

    pub(crate) fn resume_dynamic_bindings_for_thread_switch(
        &mut self,
        token: ThreadDynamicBindingToken,
    ) -> Result<(), Flow> {
        assert!(
            self.specpdl.is_empty(),
            "a simulated thread must unwind its active specpdl before switching out"
        );
        assert!(
            self.condition_stack.is_empty(),
            "a simulated thread must unwind its active handlers before switching out"
        );
        assert_eq!(
            self.suspended_thread_bindings.len(),
            token.suspended_depth + 1,
            "simulated thread binding stacks must resume in LIFO order"
        );
        let state = self
            .suspended_thread_bindings
            .pop()
            .expect("validated suspended thread binding depth");
        self.specpdl = state.specpdl;
        self.condition_stack = state.condition_stack;
        let result = self.specpdl_unrewind_vars_for_thread_switch(true);
        // The thread is current by the time its bindings are resumed.  Its
        // lexical environment therefore belongs to the error handler too if
        // a forwarded dynamic value rejects the exchange.
        self.lexenv = state.lexenv;
        self.lexenv_assq_cache.clear();
        self.lexenv_special_cache.clear();
        result
    }

    pub(crate) fn unbind_to_result(&mut self, count: usize) -> Result<(), Flow> {
        // GNU `unbind_to` (eval.c:3907-3930) suspends a pending quit while
        // cleanups run. With no quit pending its bracket saves and puts back
        // nil, so there is nothing to suspend, root or restore.
        if !self.quit_flag_value().is_nil() {
            return self.unbind_entries_suspending_quit(count);
        }
        self.unbind_entries_to(count)
    }

    #[cold]
    #[inline(never)]
    fn unbind_entries_suspending_quit(&mut self, count: usize) -> Result<(), Flow> {
        let mut quit_scope = UnwindQuitScope::enter(self);
        let result = quit_scope.context().unbind_entries_to(count);
        quit_scope.finish_unbind(result)
    }

    #[inline]
    fn unbind_entries_to(&mut self, count: usize) -> Result<(), Flow> {
        while self.specpdl.len() > count {
            match self.next_popped_binding_policy() {
                PoppedBindingPolicy::PureStorage => {
                    let Some(binding) = self.specpdl.pop() else {
                        break;
                    };
                    self.unbind_popped_binding(binding, PoppedBindingPolicy::PureStorage)?;
                }
                PoppedBindingPolicy::LispCleanup => self.unbind_popped_with_recovery()?,
            }
        }
        Ok(())
    }

    #[inline]
    #[deny(clippy::wildcard_enum_match_arm)]
    fn next_popped_binding_policy(&self) -> PoppedBindingPolicy {
        match self.specpdl.last() {
            Some(
                SpecBinding::Let { sym_id, .. }
                | SpecBinding::LetDefault { sym_id, .. }
                | SpecBinding::LetLocal { sym_id, .. },
            ) => {
                if self.watchers.has_watchers(*sym_id) {
                    PoppedBindingPolicy::LispCleanup
                } else {
                    PoppedBindingPolicy::PureStorage
                }
            }
            Some(SpecBinding::UnwindProtect { .. }) => match self.lisp_execution() {
                LispExecution::Live => PoppedBindingPolicy::LispCleanup,
                LispExecution::ExitedAlready => PoppedBindingPolicy::PureStorage,
            },
            Some(SpecBinding::NativeUnwind { .. }) => PoppedBindingPolicy::LispCleanup,
            Some(
                SpecBinding::LexicalEnv { .. }
                | SpecBinding::GcRoot { .. }
                | SpecBinding::Backtrace { .. }
                | SpecBinding::Backtrace1 { .. }
                | SpecBinding::Backtrace2 { .. }
                | SpecBinding::BacktraceNative { .. }
                | SpecBinding::SaveExcursion { .. }
                | SpecBinding::SaveCurrentBuffer { .. }
                | SpecBinding::SaveRestriction { .. }
                | SpecBinding::LoadsInProgress { .. }
                | SpecBinding::RequireStack { .. }
                | SpecBinding::Nop,
            )
            | None => PoppedBindingPolicy::PureStorage,
        }
    }

    /// Callback-only cold ownership; ordinary pure unbind entries bypass it.
    #[cold]
    #[inline(never)]
    fn unbind_popped_with_recovery(&mut self) -> Result<(), Flow> {
        let Some(recovery) = self.specpdl.last().cloned() else {
            return Ok(());
        };
        let mut in_flight = PoppedBindingScope::enter(self, recovery);
        let Some(binding) = in_flight.context().specpdl.pop() else {
            in_flight.finish();
            return Ok(());
        };
        let result = in_flight
            .context()
            .unbind_popped_binding(binding, PoppedBindingPolicy::LispCleanup);
        // Normal signals preserve GNU's popped-entry semantics. Only Rust
        // panic recovery replays native storage without evaluating Lisp.
        in_flight.finish();
        result
    }

    // Inlined into the unbind loop, which is where base `unbind_to_result`
    // kept this match: an out-of-line call per popped entry cost about 0.4%
    // of the org-editing board row.
    // Classification precedes the pop, without a Lisp call or mutation in
    // between. PureStorage therefore also proves this Context's watcher set
    // is empty for a let entry; do not look it up a second time on that path.
    #[inline(always)]
    fn unbind_popped_binding(
        &mut self,
        binding: SpecBinding,
        policy: PoppedBindingPolicy,
    ) -> Result<(), Flow> {
        match binding {
            SpecBinding::Let { sym_id, old_value } => {
                let old_value = old_value.get();
                let still_plain = self.obarray.get_by_id(sym_id).is_none_or(|s| {
                    s.redirect() == crate::emacs_core::symbol::SymbolRedirect::Plainval
                });
                if still_plain
                    && policy == PoppedBindingPolicy::LispCleanup
                    && self.watchers.has_watchers(sym_id)
                {
                    let restore_val = old_value.unwrap_or(Value::NIL);
                    self.run_variable_watchers_by_id(sym_id, &restore_val, &Value::NIL, "unlet")?;
                    // A watcher can change the redirect arm. Restore through
                    // the arm it left, as GNU do_one_unbind does.
                    self.set_internal_after_watchers(
                        sym_id,
                        old_value.unwrap_or(Value::UNBOUND),
                        crate::emacs_core::symbol::SetInternalBind::Unbind,
                    )?;
                } else if still_plain {
                    match old_value {
                        Some(val) => {
                            self.obarray.set_symbol_value_id(sym_id, val);
                            self.sync_cached_runtime_binding_by_id(sym_id, val);
                            self.sync_user_test_gc_binding_by_id(sym_id);
                        }
                        None => {
                            self.obarray.makunbound_id(sym_id);
                            self.sync_cached_runtime_binding_by_id(sym_id, Value::NIL);
                            self.sync_user_test_gc_binding_by_id(sym_id);
                        }
                    }
                } else {
                    self.restore_default_binding_by_id(
                        sym_id,
                        old_value,
                        crate::emacs_core::symbol::SetInternalBind::Unbind,
                    )?;
                }
            }
            SpecBinding::LetLocal {
                sym_id,
                old_value,
                buffer_id,
            } => {
                // Restore only if the buffer is still live AND the
                // variable is *still* buffer-local in that buffer.
                // Mirrors GNU `do_one_unbind` SPECPDL_LET_LOCAL
                // arm at `eval.c:3852-3863`:
                //     /* If this was a local binding, reset the value in
                //        the appropriate buffer, but only if that buffer's
                //        binding still exists.  */
                //     if (!NILP (Flocal_variable_p (symbol, where)))
                //       set_internal (symbol, old_value, where, UNBIND);
                //
                // The `Flocal_variable_p` guard is load-bearing: if the
                // local binding was eliminated *inside* the `let` body
                // (e.g. `kill-all-local-variables` killed a non-permanent
                // local), GNU does NOT restore the old value — the kill
                // wins. Without this guard neomacs resurrected the old
                // local value, leaking stale buffer-local state across a
                // major-mode switch (the org/derived-mode hook-loss path:
                // `delay-mode-hooks`/`delayed-mode-hooks` machinery relies
                // on KALV's reset surviving the surrounding `let`).
                use crate::emacs_core::symbol::{SetInternalBind, SymbolRedirect};
                let is_localized = self
                    .obarray
                    .get_by_id(sym_id)
                    .map(|s| s.redirect() == SymbolRedirect::Localized)
                    .unwrap_or(false);
                let still_local = match self.buffers.get(buffer_id) {
                    None => false,
                    Some(buf) => {
                        if is_localized {
                            let buf_val = Value::make_buffer(buffer_id);
                            self.obarray.has_per_buffer_binding(
                                sym_id,
                                buf_val,
                                buf.local_var_alist_value(),
                            )
                        } else {
                            // `is_localized` is false here, so a non-slot,
                            // non-undo symbol is never in the alist: gate the
                            // scan away (slot/undo still resolve).
                            buf.has_buffer_local_by_sym_id_gated(sym_id, false)
                        }
                    }
                };
                if still_local {
                    if policy == PoppedBindingPolicy::LispCleanup
                        && self.watchers.has_watchers(sym_id)
                    {
                        self.run_variable_watchers_by_id_with_where(
                            sym_id,
                            &old_value,
                            &Value::NIL,
                            "unlet",
                            &Value::make_buffer(buffer_id),
                        )?;
                    }
                    // Phase 10E: for LOCALIZED symbols, restore via
                    // set_internal_localized(UNBIND) targeting the
                    // saved buffer. This walks the buffer's alist
                    // and rewrites the cell's cdr in place,
                    // matching GNU's set_internal LOCALIZED arm
                    // and bypassing the legacy lisp_bindings path.
                    if is_localized {
                        let buf_val = Value::make_buffer(buffer_id);
                        let alist = self
                            .buffers
                            .get(buffer_id)
                            .map(|buf| buf.local_var_alist_value())
                            .unwrap_or(Value::NIL);
                        let new_alist = self.obarray.set_internal_localized(
                            sym_id,
                            old_value,
                            buf_val,
                            alist,
                            SetInternalBind::Unbind,
                            false,
                        );
                        if let Some(buf) = self.buffers.get_mut(buffer_id) {
                            buf.replace_local_var_alist(new_alist);
                        }
                    } else {
                        let _ = self
                            .buffers
                            .set_buffer_local_property_by_sym_id(buffer_id, sym_id, old_value);
                    }
                    self.sync_cached_runtime_binding_by_id(sym_id, old_value);
                    self.sync_user_test_gc_binding_by_id(sym_id);
                }
            }
            SpecBinding::LetDefault {
                sym_id, old_value, ..
            } => {
                let old_value = old_value.get();
                self.restore_default_binding_by_id(
                    sym_id,
                    old_value,
                    crate::emacs_core::symbol::SetInternalBind::Unbind,
                )?;
            }
            SpecBinding::LexicalEnv { old_lexenv } => {
                // Mirrors GNU unbind_to for
                // specbind(Qinternal_interpreter_environment, ...).
                self.lexenv = old_lexenv;
            }
            SpecBinding::GcRoot { .. } => {}
            SpecBinding::Backtrace { args, .. } => {
                self.release_backtrace_args(&args);
                // No-op, matches GNU SPECPDL_BACKTRACE
            }
            SpecBinding::Backtrace1 { .. }
            | SpecBinding::Backtrace2 { .. }
            | SpecBinding::BacktraceNative { .. } => {
                // Inline evaluated backtraces own no side-stack payload.
            }
            SpecBinding::Nop => {
                // No-op, matches GNU SPECPDL_NOP
            }
            SpecBinding::UnwindProtect {
                forms: cleanup,
                lexenv,
            } => match self.lisp_execution() {
                // GNU's `Fkill_emacs` is `attributes: noreturn`
                // (src/emacs.c:2974) and ends in `exit (exit_code)` (:3088)
                // without ever reaching `unbind_to`, so a cleanup form
                // still on the specpdl when `kill-emacs` is called
                // never runs.  This port has to drain the specpdl to
                // walk back out to `main`; the drain must not evaluate
                // what GNU has already exited past.  The binding
                // restorations below/above still run -- see
                // [`LispExecution`] for why that is invisible.
                LispExecution::ExitedAlready => {}
                LispExecution::Live => {
                    // Entry already popped — re-entrant errors won't re-unwind.
                    let saved_lexenv = self.lexenv;
                    self.lexenv = lexenv;
                    let cleanup_result = {
                        let mut guard = UnwindCleanupGuard::enter(self);
                        if cleanup.is_cons() || cleanup.is_nil() {
                            // Interpreter path: list of forms
                            guard.context().sf_progn_value(cleanup)
                        } else {
                            // VM path: callable (bytecode function)
                            guard.context().apply(cleanup, vec![])
                        }
                    };
                    self.lexenv = saved_lexenv;
                    cleanup_result?;
                }
            },
            SpecBinding::SaveExcursion {
                marker,
                saved_window,
                ..
            } => self.restore_save_excursion(marker, saved_window),
            SpecBinding::SaveCurrentBuffer { buffer_id } => {
                self.restore_current_buffer_if_live(buffer_id);
            }
            SpecBinding::SaveRestriction { state } => {
                self.buffers
                    .restore_saved_restriction_state(state.into_state());
            }
            SpecBinding::LoadsInProgress { len } => {
                self.loads_in_progress.truncate(len);
            }
            SpecBinding::RequireStack { len } => {
                self.require_stack.truncate(len);
            }
            SpecBinding::NativeUnwind { action } => {
                action.run(self)?;
            }
        }
        Ok(())
    }

    /// GNU's common excursion restore: the marker's live buffer is still
    /// current and the captured window needs no point update. The caller keeps
    /// the original specpdl entry rooting MARKER until this returns Completed;
    /// no Lisp object allocation, callback or GC point occurs during restore.
    #[inline]
    pub(super) fn restore_excursion_in_current_buffer(
        &mut self,
        marker: Value,
        saved_window: ExcursionWindow,
    ) -> ExcursionStorageRestore {
        let Some(location) = super::super::marker::marker_location(&self.buffers, marker) else {
            return ExcursionStorageRestore::NeedsRuntime;
        };
        if self.buffers.current_buffer_id() != Some(location.buffer()) {
            return ExcursionStorageRestore::NeedsRuntime;
        }
        if let Some(window) = saved_window.window()
            && Some(window)
                != self
                    .frames
                    .selected_frame()
                    .map(|frame| frame.selected_window)
        {
            return ExcursionStorageRestore::NeedsRuntime;
        }
        // Use the live marker and normal point setter: a changed narrowing
        // still clamps point just as GNU Fgoto_char does (editfns.c:802).
        let _ = self
            .buffers
            .goto_buffer_emacs_byte_pos(location.buffer(), location.byte_pos());
        super::super::marker::unchain_marker(&mut self.buffers, &marker);
        ExcursionStorageRestore::Completed
    }

    /// GNU `save_excursion_restore` (editfns.c:791-810): follow the saved
    /// marker's live buffer — buffer-swap-text may have moved it — restore
    /// that buffer and point, unchain the marker, and sync the capture-time
    /// window's point when a different window is selected now and it still
    /// displays the restored buffer. Panic recovery selects the storage-only
    /// policy so switching buffers cannot seed lazy runtime tables.
    #[inline]
    fn restore_save_excursion(&mut self, marker: Value, saved_window: ExcursionWindow) {
        self.restore_save_excursion_with_policy(marker, saved_window, SavedBufferRestore::Runtime);
    }

    #[inline(always)]
    fn restore_save_excursion_with_policy(
        &mut self,
        marker: Value,
        saved_window: ExcursionWindow,
        restore: SavedBufferRestore,
    ) {
        if let Some(location) = super::super::marker::marker_location(&self.buffers, marker) {
            let restored_buffer = location.buffer();
            match restore {
                SavedBufferRestore::Runtime => {
                    self.restore_current_buffer_if_live(restored_buffer);
                }
                SavedBufferRestore::StorageOnly => {
                    self.restore_current_buffer_storage_if_live(restored_buffer);
                }
            }
            let _ = self
                .buffers
                .goto_buffer_emacs_byte_pos(restored_buffer, location.byte_pos());
            // GNU editfns.c:804-810: when the recorded window is not the
            // selected window and still shows the restored buffer,
            // `Fset_window_point (window, PT)` — the nonselected branch is
            // one marker store plus the redisplay flag (window.c:1928-1933).
            if let Some(window_id) = saved_window.window()
                && Some(window_id)
                    != self
                        .frames
                        .selected_frame()
                        .map(|frame| frame.selected_window)
                && let Some(point) = self
                    .buffers
                    .get(restored_buffer)
                    .map(crate::buffer::Buffer::point_lisp_char_pos)
                && let Some(window) = self.frames.lookup_window_mut(window_id)
                && window.buffer_id() == Some(restored_buffer)
            {
                crate::window::window_markers::set_window_point_with_marker(
                    &mut self.buffers,
                    window,
                    point,
                );
                self.gnu_mark_window_redisplay(window_id);
            }
        }
        super::super::marker::unchain_marker(&mut self.buffers, &marker);
    }
}

/// A mutator-local specpdl scope with a non-Lisp panic fallback.
///
/// The exclusive borrow keeps the Context alive and prevents migration while
/// saved state is active. Normal/signaled exits call `finish`; Drop abandons
/// Lisp cleanup payloads and only restores native storage.
#[must_use = "finish the scope to propagate Lisp cleanup signals"]
struct SavedStateScope<'a> {
    context: &'a mut Context,
    count: Option<usize>,
    thread_confined: std::marker::PhantomData<*const ()>,
}

static_assertions::assert_not_impl_any!(SavedStateScope<'static>: Send, Sync);

impl std::fmt::Debug for SavedStateScope<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SavedStateScope")
            .field("count", &self.count)
            .finish_non_exhaustive()
    }
}

impl SavedStateScope<'_> {
    #[inline]
    fn finish(mut self, result: EvalResult) -> EvalResult {
        let result = match self.count {
            Some(count) if self.context.specpdl.len() != count => {
                self.context.unbind_to_with_result(count, result)
            }
            Some(_) | None => result,
        };
        self.count = None;
        result
    }
}

impl Drop for SavedStateScope<'_> {
    #[inline]
    fn drop(&mut self) {
        if let Some(count) = self.count.take() {
            self.context.discard_specpdl_to(count);
        }
    }
}

/// GNU `record_unwind_current_buffer`, owned by one mutator.
///
/// Holds an exclusive Context borrow and is deliberately neither Send nor
/// Sync. Its specpdl entry roots the saved state across arbitrary Lisp and GC.
#[derive(Debug)]
#[must_use = "finish the current-buffer scope to propagate cleanup signals"]
pub(crate) struct CurrentBufferScope<'a>(SavedStateScope<'a>);
static_assertions::assert_not_impl_any!(CurrentBufferScope<'static>: Send, Sync);

impl<'a> CurrentBufferScope<'a> {
    #[inline]
    pub(crate) fn enter(context: &'a mut Context) -> Self {
        let count = Some(context.specpdl.len());
        if let Some(buffer_id) = context.buffers.current_buffer_id() {
            context.push_specpdl_with(|| SpecBinding::SaveCurrentBuffer { buffer_id });
        }
        Self(SavedStateScope {
            context,
            count,
            thread_confined: std::marker::PhantomData,
        })
    }

    /// Avoid a save entry when GNU would not switch buffers at all.
    #[inline]
    pub(crate) fn for_buffer(
        context: &'a mut Context,
        buffer: crate::buffer::BufferId,
    ) -> Result<Self, Flow> {
        if context.buffers.current_buffer_id() == Some(buffer) {
            // GNU does not record a buffer restore for this case. Still own
            // the child cleanup boundary if Rust unwinds unexpectedly.
            let count = Some(context.specpdl.len());
            return Ok(Self(SavedStateScope {
                context,
                count,
                thread_confined: std::marker::PhantomData,
            }));
        }
        let mut scope = Self::enter(context);
        scope.context().set_current_buffer_unrecorded(buffer)?;
        Ok(scope)
    }

    #[inline]
    pub(crate) fn context(&mut self) -> &mut Context {
        self.0.context
    }
    #[inline]
    pub(crate) fn finish(self, result: EvalResult) -> EvalResult {
        self.0.finish(result)
    }
}

/// GNU save-excursion: current buffer and marker-backed point, for one mutator.
/// No state is shared between independent Contexts, and the guard cannot migrate.
#[derive(Debug)]
#[must_use = "finish the excursion scope to propagate cleanup signals"]
pub(crate) struct ExcursionScope<'a>(SavedStateScope<'a>);
static_assertions::assert_not_impl_any!(ExcursionScope<'static>: Send, Sync);

impl<'a> ExcursionScope<'a> {
    #[inline]
    pub(crate) fn enter(context: &'a mut Context) -> Self {
        let count = Some(context.specpdl.len());
        let _ = context.record_save_excursion();
        Self(SavedStateScope {
            context,
            count,
            thread_confined: std::marker::PhantomData,
        })
    }
    #[inline]
    pub(crate) fn context(&mut self) -> &mut Context {
        self.0.context
    }
    #[inline]
    pub(crate) fn finish(self, result: EvalResult) -> EvalResult {
        self.0.finish(result)
    }
}

/// GNU save-restriction: marker-backed bounds without restoring current buffer.
/// Exclusively borrows one mutator's Context; neither Send nor Sync.
#[derive(Debug)]
#[must_use = "finish the restriction scope to propagate cleanup signals"]
pub(crate) struct RestrictionScope<'a>(SavedStateScope<'a>);
static_assertions::assert_not_impl_any!(RestrictionScope<'static>: Send, Sync);

impl<'a> RestrictionScope<'a> {
    #[inline]
    pub(crate) fn enter(context: &'a mut Context) -> Self {
        let count = Some(context.specpdl.len());
        if let Some(state) = context.buffers.save_current_restriction_state() {
            context.push_specpdl_with(|| SpecBinding::save_restriction(state));
        }
        Self(SavedStateScope {
            context,
            count,
            thread_confined: std::marker::PhantomData,
        })
    }
    #[inline]
    pub(crate) fn context(&mut self) -> &mut Context {
        self.0.context
    }
    #[inline]
    pub(crate) fn finish(self, result: EvalResult) -> EvalResult {
        self.0.finish(result)
    }
}
