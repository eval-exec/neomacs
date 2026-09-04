//! specbind / unbind_to: GNU specpdl-style dynamic binding, variable watchers, and the unwind that restores them.
//!
//! Moved out of `eval/mod.rs` unchanged; a child module of `eval` so it keeps
//! the same view of `Context` and the parent's private items (`use super::*`).

use super::*;

impl Context {
    // Shared runtime write path for symbol-cell mutation. This mirrors GNU
    // `set_internal` after lexical handling has already been decided.

    // -----------------------------------------------------------------------
    // specbind / unbind_to — GNU Emacs specpdl-style dynamic variable binding
    // -----------------------------------------------------------------------

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

    /// Save the current value of a special variable and set a new value.
    /// Matches GNU Emacs's `specbind` in eval.c:
    /// - Follows SYMBOL_VARALIAS to the final target
    /// - For buffer-local variables with a local binding: SPECPDL_LET_LOCAL
    /// - For buffer-local variables without local binding: SPECPDL_LET_DEFAULT
    /// - For plain variables: SPECPDL_LET
    pub(super) fn specbind_resolved(&mut self, sym_id: SymId, value: Value) -> Result<(), Flow> {
        // GNU `specbind` switches on the redirect first: a plain value cell is
        // a specpdl push and one store (`SET_SYMBOL_VAL`).  Every other shape
        // — alias, forwarded, buffer-local, the undo list — takes the full
        // path below.  The plain tail of that path is reproduced exactly: the
        // constant check already ran in `sf_let`, and watchers still fire.
        if sym_id != buffer_undo_list_symbol()
            && let Some(sym) = self.obarray.get_by_id(sym_id)
            && sym.redirect() == crate::emacs_core::symbol::SymbolRedirect::Plainval
        {
            let old_value = SavedBindingValue::from_plain(sym.plain());
            self.specpdl.push(SpecBinding::Let { sym_id, old_value });
            self.run_specbind_watcher(sym_id, value, "let")?;
            self.obarray.store_plain_value_id(sym_id, value);
            self.sync_cached_runtime_binding_by_id(sym_id, value);
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
            self.specpdl.push(SpecBinding::LetLocal {
                sym_id: resolved,
                old_value,
                buffer_id: buf_id,
            });
            self.run_specbind_watcher(resolved, value, "let")?;
            let _ = self
                .buffers
                .set_buffer_local_property_by_sym_id(buf_id, resolved, value);
            self.sync_cached_runtime_binding_by_id(resolved, value);
            return Ok(());
        }

        // ONE symbol fetch decides the arm, like GNU `specbind`'s redirect
        // switch over an in-hand `Lisp_Symbol *` (eval.c:3642). The POD facts
        // are captured so the obarray borrow ends before any mutation; every
        // arm below reuses them instead of re-fetching the symbol.
        use crate::emacs_core::symbol::SymbolRedirect;
        let (redirect, forwarded) = match self.obarray.get_by_id(resolved) {
            Some(sym) => {
                let redirect = sym.redirect();
                let fwd = (redirect == SymbolRedirect::Forwarded).then(|| unsafe { sym.val.fwd });
                (redirect, fwd)
            }
            None => (SymbolRedirect::Plainval, None),
        };

        // FORWARDED BUFFER_OBJFWD specbind, separate from the legacy
        // LOCALIZED path. Mirrors GNU `specbind` SYMBOL_FORWARDED arm at
        // `eval.c:3641-3677`.
        {
            use crate::emacs_core::forward::{LispBufferObjFwd, LispFwdType};
            if let Some(fwd_ptr) = forwarded {
                let fwd = unsafe { &*fwd_ptr };
                if matches!(fwd.ty, LispFwdType::BufferObj) {
                    let buf_fwd = unsafe { &*(fwd as *const _ as *const LispBufferObjFwd) };
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
                        self.specpdl.push(SpecBinding::LetLocal {
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
                        self.specpdl.push(SpecBinding::LetDefault {
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
                self.specpdl.push(SpecBinding::LetLocal {
                    sym_id: resolved,
                    old_value: old_val,
                    buffer_id: buf_id,
                });
            } else {
                self.specpdl.push(SpecBinding::LetDefault {
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
            return Ok(());
        }

        // Plain value path (GNU: SYMBOL_PLAINVAL). A PLAINVAL symbol has no
        // forward descriptor (`assignment_forwarder` is None by redirect), so
        // the typed-store probe is pure overhead for it; non-buffer forwarded
        // symbols (Int/Bool/Obj/Kboard) still take it so `(let
        // ((gc-cons-threshold "x")) ...)` keeps signaling before the body.
        let old_value = self.obarray.symbol_value_id(resolved).copied();
        self.specpdl.push(SpecBinding::Let {
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

    /// Check if a `let` is currently shadowing a buffer-local
    /// variable's binding. Matches GNU
    /// `eval.c:3559-3577 (let_shadows_buffer_binding_p)`.
    ///
    /// When true, `setq` inside the let should modify the existing
    /// binding (whichever specpdl record is on top) rather than
    /// auto-creating a brand-new per-buffer binding.
    ///
    /// GNU walks the specpdl looking for SPECPDL_LET_DEFAULT records
    /// keyed to the symbol in the current buffer. SPECPDL_LET_LOCAL is
    /// explicitly excluded (GNU bug#62419), because a let over an
    /// existing buffer-local binding must keep writes in that local
    /// binding instead of treating the default as shadowed.
    pub(crate) fn let_shadows_buffer_binding_p(&self, sym_id: SymId) -> bool {
        let current = self.buffers.current_buffer_id();
        self.specpdl.iter().rev().any(|entry| match entry {
            SpecBinding::LetDefault {
                sym_id: s,
                buffer_id,
                ..
            } => *s == sym_id && buffer_id.get() == current,
            SpecBinding::LetLocal { .. } => false,
            SpecBinding::Let { .. }
            | SpecBinding::LexicalEnv { .. }
            | SpecBinding::GcRoot { .. }
            | SpecBinding::Backtrace { .. }
            | SpecBinding::Backtrace1 { .. }
            | SpecBinding::Backtrace2 { .. }
            | SpecBinding::BacktraceNative { .. }
            | SpecBinding::Nop
            | SpecBinding::UnwindProtect { .. }
            | SpecBinding::SaveExcursion { .. }
            | SpecBinding::SaveCurrentBuffer { .. }
            | SpecBinding::SaveRestriction { .. }
            | SpecBinding::LoadsInProgress { .. }
            | SpecBinding::NativeUnwind { .. }
            | SpecBinding::RequireStack { .. } => false,
        })
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
        use crate::emacs_core::symbol::{SetInternalBind, SymbolRedirect};

        let is_localized = self
            .obarray
            .get_by_id(sym_id)
            .map(|s| s.redirect() == SymbolRedirect::Localized)
            .unwrap_or(false);
        if is_localized {
            let buf_val = Value::make_buffer(buffer_id);
            let alist = self
                .buffers
                .get(buffer_id)
                .map(|buf| buf.local_var_alist_value())
                .unwrap_or(Value::NIL);
            let new_alist = self.obarray.set_internal_localized(
                sym_id,
                value,
                buf_val,
                alist,
                SetInternalBind::ThreadSwitch,
                false,
            );
            if let Some(buf) = self.buffers.get_mut(buffer_id) {
                buf.replace_local_var_alist(new_alist);
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
            self.obarray.symbol_value_id(sym_id).copied()
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
                }
                None => {
                    self.obarray.makunbound_id(sym_id);
                    self.sync_cached_runtime_binding_by_id(sym_id, Value::NIL);
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
        // Mirrors GNU `unbind_to` in `eval.c:3907-3930`: suppress a
        // pending quit during cleanup so `unwind-protect` cleanup forms
        // run to completion, then restore the pending state on exit if
        // no inner form replaced it. Without this an interactive `C-g`
        // arriving during a long-running protected form would abort the
        // CLEANUP clause mid-way, leaving resources in a bad state.
        let quitf = self.quit_flag_value();
        if !quitf.is_nil() {
            self.set_quit_flag_value(Value::NIL);
        }
        let result = (|| -> Result<(), Flow> {
            while self.specpdl.len() > count {
                let binding = self.specpdl.pop().unwrap();
                match binding {
                    SpecBinding::Let { sym_id, old_value } => {
                        let old_value = old_value.get();
                        let still_plain = self.obarray.get_by_id(sym_id).is_none_or(|s| {
                            s.redirect() == crate::emacs_core::symbol::SymbolRedirect::Plainval
                        });
                        if still_plain {
                            if self.watchers.has_watchers(sym_id) {
                                let restore_val = old_value.unwrap_or(Value::NIL);
                                self.run_variable_watchers_by_id(
                                    sym_id,
                                    &restore_val,
                                    &Value::NIL,
                                    "unlet",
                                )?;
                            }
                            match old_value {
                                Some(val) => {
                                    self.obarray.set_symbol_value_id(sym_id, val);
                                    self.sync_cached_runtime_binding_by_id(sym_id, val);
                                }
                                None => {
                                    self.obarray.makunbound_id(sym_id);
                                    self.sync_cached_runtime_binding_by_id(sym_id, Value::NIL);
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
                            if self.watchers.has_watchers(sym_id) {
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
                                let _ = self.buffers.set_buffer_local_property_by_sym_id(
                                    buffer_id, sym_id, old_value,
                                );
                            }
                            self.sync_cached_runtime_binding_by_id(sym_id, old_value);
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
                        buffer_id,
                        marker_id,
                        marker: _,
                    } => {
                        self.restore_current_buffer_if_live(buffer_id);
                        if let Some(saved_pt) =
                            self.buffers.marker_emacs_byte_pos(buffer_id, marker_id)
                        {
                            let _ = self.buffers.goto_buffer_emacs_byte_pos(buffer_id, saved_pt);
                        }
                        self.buffers.remove_marker(marker_id);
                    }
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
            }
            Ok(())
        })();
        // If cleanup forms didn't set their own quit, reinstate the
        // pending state. Matches `eval.c:3927-3928`.
        if !quitf.is_nil() && self.quit_flag_value().is_nil() {
            self.set_quit_flag_value(quitf);
        }
        result
    }
}
