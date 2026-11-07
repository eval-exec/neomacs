//! Context facade state, message and accessor surface.
//!
//! Moved out of `eval/mod.rs` unchanged; a child module of `eval` so it
//! keeps the same view of `Context` and the parent's private items
//! (`use super::*`). The facade keeps the `Context` struct, its types and
//! the public evaluation API; these accessors, runtime-state entry points
//! and message/echo-area methods were the bulk that pushed the facade over
//! its layout ceiling.

use super::*;

impl Context {
    #[doc(hidden)]
    pub fn set_interpreted_closure_filter_fn(&mut self, filter_fn: Option<Value>) {
        self.interpreted_closure_filter_fn = filter_fn;
    }

    /// Load a file with a typed caller policy, converting EvalError back to
    /// Flow for use in special forms.
    #[doc(hidden)]
    pub fn load_file_internal_with_options(
        &mut self,
        path: &std::path::Path,
        options: super::super::load::LoadOptions,
    ) -> EvalResult {
        super::super::load::load_file_with_options(self, path, options)
            .map_err(neovm_value::error::flow_from_eval_error)
    }

    #[doc(hidden)]
    pub fn eval_value_with_lexical_arg(
        &mut self,
        form: Value,
        lexical_arg: Option<Value>,
    ) -> EvalResult {
        let state =
            begin_eval_with_lexical_arg_in_state(&mut self.lexenv, &mut self.specpdl, lexical_arg)?;
        let eval_result = self.eval_value(&form);
        let result = self.dispatch_signal_result_if_needed(eval_result);
        finish_eval_with_lexical_arg_in_state(&mut self.lexenv, &mut self.specpdl, state);
        result
    }

    #[doc(hidden)]
    pub fn begin_lambda_call(
        &mut self,
        fun: Value,
        arglist: Value,
        env: Option<Value>,
        args: &[Value],
    ) -> Result<ActiveLambdaCallState, Flow> {
        let specpdl_count = self.specpdl.len();
        // A lexical closure never gets here: `apply_lambda` binds it in GNU's
        // shape itself.
        debug_assert!(env.is_none(), "begin_lambda_call is the dynamic arm");
        let _ = env;
        if !self.lexenv.is_nil() {
            let old_lexenv = std::mem::replace(&mut self.lexenv, Value::NIL);
            // GNU funcall_lambda computes a nil local `lexenv` for a
            // dynamically scoped lambda and saves the caller's lexical
            // environment before evaluating its body.
            self.push_specpdl_with(|| SpecBinding::LexicalEnv { old_lexenv });
        }

        if let Err(flow) = self.bind_lambda_args_from_arglist(fun, arglist, args) {
            return match self.unbind_to_with_result(specpdl_count, Err(flow)) {
                Err(flow) => Err(flow),
                Ok(_) => unreachable!("unwinding an error cannot produce a value"),
            };
        }

        // GNU never writes `lexical-binding` during lambda/closure calls.
        // The closure's captured env is installed in self.lexenv (above),
        // which is the single source of truth for "is lexical mode active?"
        // via lexical_binding() -> !self.lexenv.is_nil().
        Ok(ActiveLambdaCallState { specpdl_count })
    }

    #[doc(hidden)]
    pub fn finish_lambda_call(
        &mut self,
        state: ActiveLambdaCallState,
        result: EvalResult,
    ) -> EvalResult {
        // Dynamic arguments must unwind through the same typed specpdl path
        // as every other special binding. In particular, LetLocal records the
        // buffer whose slot was shadowed and LetDefault records a localized
        // variable's shared default.
        self.unbind_to_with_result(state.specpdl_count, result)
    }

    #[doc(hidden)]
    pub fn refresh_features_from_variable(&mut self) {
        let mode = self.symbol_mode();
        refresh_features_from_variable_in_state(
            VariableView::new(&self.obarray, &self.owner),
            &mut self.features,
            mode,
        );
    }

    #[doc(hidden)]
    pub fn has_feature(&mut self, name: &str) -> bool {
        {
            let mode = self.symbol_mode();
            feature_present_in_state(
                VariableView::new(&self.obarray, &self.owner),
                &mut self.features,
                name,
                mode,
            )
        }
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    #[doc(hidden)]
    pub fn add_feature(&mut self, name: &str) {
        let mode = self.symbol_mode();
        // SAFETY: this noncalling helper changes only the feature value/list;
        // it preserves function cells, canonical descriptors and manager identity.
        unsafe {
            self.obarray.with_legacy_value_write(|obarray| {
                let mut vars = VariableStore::new(obarray, Some(&mut self.owner));
                add_feature_in_state(&mut vars, &mut self.features, name, mode);
            });
        }
    }

    #[doc(hidden)]
    pub fn feature_present(&mut self, name: &str) -> bool {
        self.has_feature(name)
    }

    /// Remove a feature (used to undo temporary provides during bootstrap).
    #[doc(hidden)]
    pub fn remove_feature(&mut self, name: &str) {
        let mode = self.symbol_mode();
        // SAFETY: this noncalling helper changes only the feature value/list;
        // no dispatch metadata or manager authority leaves the phase.
        unsafe {
            self.obarray.with_legacy_value_write(|obarray| {
                let mut vars = VariableStore::new(obarray, Some(&mut self.owner));
                remove_feature_in_state(&mut vars, &mut self.features, name, mode);
            });
        }
    }

    /// Access the obarray (for builtins that need it).
    #[inline(always)]
    pub fn obarray(&self) -> &Obarray {
        &self.obarray
    }

    /// R-b (PC11-F f1): the Context's tagged-heap view. The field this reads
    /// narrows to `pub(crate)`; cross-crate test code uses this accessor.
    #[doc(hidden)]
    #[inline(always)]
    pub fn tagged_heap(&self) -> &WorldView<neovm_value::tagged::gc::TaggedHeap> {
        &self.tagged_heap
    }

    /// R-b (PC11-F f2): mutable tagged-heap view, for fixture setup that
    /// allocates (`alloc_cons`) or sets heap policy. Live callers are the
    /// re-homed value era fixtures.
    ///
    /// # Safety
    ///
    /// The caller must be the owning thread of this Context's World. The view
    /// must only be used for in-place heap operations through its `DerefMut`
    /// target; it must never be replaced, swapped with another Context's view,
    /// or moved across Worlds (the view carries its World's retention lease,
    /// so swapping would leave the pointer paired with the wrong lease).
    #[doc(hidden)]
    #[inline(always)]
    pub unsafe fn tagged_heap_mut(
        &mut self,
    ) -> &mut WorldView<neovm_value::tagged::gc::TaggedHeap> {
        &mut self.tagged_heap
    }

    /// Split accessor for fixtures that must hold the obarray and the mutable
    /// tagged-heap view at the same time (the obarray snapshot taken during a
    /// concurrent-mark start). The two returned borrows are DISJOINT fields,
    /// so the obarray may be read while the heap view is live; no raw pointer
    /// or whole-Context aliasing is involved.
    ///
    /// # Safety
    ///
    /// The caller must be the owning thread of this Context's World. The heap
    /// view must only be used for in-place heap operations through its
    /// `DerefMut` target; it must never be replaced, swapped with another
    /// Context's view, or moved across Worlds (the view carries its World's
    /// retention lease, so swapping would leave the pointer paired with the
    /// wrong lease).
    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    #[inline(always)]
    pub(crate) unsafe fn obarray_and_tagged_heap_mut(
        &mut self,
    ) -> (&Obarray, &mut neovm_value::tagged::gc::TaggedHeap) {
        // Disjoint field borrows: shared obarray view, mutable heap view.
        (&*self.obarray, &mut *self.tagged_heap)
    }

    /// Test-support: copied macro-expansion counter.
    #[cfg(any(test, feature = "test-support"))]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    pub(crate) fn macro_expand_calls_for_test(&self) -> u64 {
        self.macro_expand_calls
    }

    /// Test-support: copied face-change counter.
    #[cfg(any(test, feature = "test-support"))]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    pub(crate) fn face_change_count_for_test(&self) -> u64 {
        self.face_change_count
    }

    /// Test-support: copied pending-shutdown request.
    #[cfg(any(test, feature = "test-support"))]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    pub(crate) fn shutdown_request_for_test(
        &self,
    ) -> Option<neovm_value::value::flow_vocab::ShutdownRequest> {
        self.shutdown_request
    }

    /// Test-support: copied autoload query (the manager is a WorldView that
    /// holds heap Values, so N3 forbids handing out `&`/`&mut`; a copied
    /// query is the VIS route).
    #[cfg(any(test, feature = "test-support"))]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    pub(crate) fn autoload_is_autoloaded_for_test(&self, name: &str) -> bool {
        self.autoloads.is_autoloaded(name)
    }

    /// Test-support: copied autoload entry query by symbol id.
    #[cfg(any(test, feature = "test-support"))]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    pub(crate) fn autoload_entry_symbol_for_test(
        &self,
        name: neovm_value::intern::SymId,
    ) -> Option<crate::emacs_core::autoload::AutoloadEntry> {
        self.autoloads.get_entry_symbol(name).cloned()
    }

    /// Test-support: copied autoload entry query by name.
    #[cfg(any(test, feature = "test-support"))]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    pub(crate) fn autoload_entry_for_test(
        &self,
        name: &str,
    ) -> Option<crate::emacs_core::autoload::AutoloadEntry> {
        self.autoloads.get_entry(name).cloned()
    }

    /// Test-support: typed replacement of the autoload manager; the only
    /// cross-crate write route (the authority fence delegates here).
    ///
    /// # Safety
    ///
    /// Every `Value` stored in `manager` must have been allocated in this
    /// context's heap, and no collection may have run since those values were
    /// last rooted. The caller transfers sole ownership of `manager` to
    /// `self`; the replaced manager is dropped.
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) unsafe fn replace_autoloads_raw(
        &mut self,
        manager: crate::emacs_core::autoload::AutoloadManager,
    ) {
        *self.autoloads = manager;
    }

    /// R-b (PC11-F f1): the edit runtime (buffers, overlays, markers). The
    /// `edit` field predates the split; this is the named read route the
    /// re-homed value test tree uses.
    #[doc(hidden)]
    #[inline(always)]
    pub fn edit(&self) -> &neovm_edit::buffer::EditRuntime {
        &self.edit
    }

    /// R-b (PC11-F f2): mutable edit runtime for fixture setup that needs it.
    #[doc(hidden)]
    #[inline(always)]
    pub fn edit_mut(&mut self) -> &mut neovm_edit::buffer::EditRuntime {
        &mut self.edit
    }

    /// R-b (PC11-F f1): the thread-projection binding authority (read).
    #[doc(hidden)]
    #[inline(always)]
    pub fn owner(&self) -> &owner_bindings::OwnerSlot {
        &self.owner
    }

    /// R-b (PC11-F f2/f3): mutable owner authority.
    ///
    /// # Safety
    ///
    /// The caller must be the Context's owner thread holding its World Baton.
    /// The slot must not be swapped, replaced or moved to another Context;
    /// only the existing binding write paths may use it.
    #[doc(hidden)]
    #[inline(always)]
    pub unsafe fn owner_mut(&mut self) -> &mut owner_bindings::OwnerSlot {
        &mut self.owner
    }

    /// R-b (PC11-F f1): the dynamic-binding stack (read).
    #[doc(hidden)]
    #[inline(always)]
    pub fn specpdl(&self) -> &Vec<SpecBinding> {
        &self.specpdl
    }

    /// R-b (PC11-F f2): raw dynamic-binding-stack writes.
    ///
    /// # Safety
    ///
    /// The caller must be the owner thread and must keep the stack balanced
    /// (every push has its matching truncate/pop on the same path); the Vec
    /// must not be replaced or moved to another Context.
    #[doc(hidden)]
    #[inline(always)]
    pub unsafe fn specpdl_mut(&mut self) -> &mut Vec<SpecBinding> {
        &mut self.specpdl
    }

    /// R-b (PC11-F f1): the published host input receiver.
    #[doc(hidden)]
    #[inline(always)]
    pub fn input_rx(&self) -> &crate::emacs_core::eval::host_input::HostInputReceiver {
        &self.input_rx
    }

    /// R-b (PC11-F f1): owner-carried redisplay-hook ownership (read).
    #[doc(hidden)]
    #[inline(always)]
    pub fn gnu_redisplay_hooks(&self) -> &redisplay_hooks::RedisplayHookOwnership {
        &self.gnu_redisplay_hooks
    }

    /// R-b (PC11-F f2, signed): the existing core redisplay writers mutate the
    /// owner-carried hook ownership in place (dimensions, window-change set,
    /// old selected window). `owner_carried` remains the only swap site.
    #[doc(hidden)]
    #[inline(always)]
    pub fn gnu_redisplay_hooks_mut(&mut self) -> &mut redisplay_hooks::RedisplayHookOwnership {
        &mut self.gnu_redisplay_hooks
    }

    /// R-b (PC11-F f2, signed): the existing display writers bump or reset the
    /// face-change counter.
    #[doc(hidden)]
    #[inline(always)]
    pub fn face_change_count_mut(&mut self) -> &mut u64 {
        &mut self.face_change_count
    }

    /// Access the obarray's view capability (phase-scoped writes that must go
    /// through `with_legacy_value_write`).
    #[doc(hidden)]
    #[inline(always)]
    pub fn obarray_view(&self) -> &ObarrayView {
        &self.obarray
    }

    /// Non-swappable mutable view capability for phase-scoped writes.
    ///
    /// The capability forwards the audited `&mut self` operations but never
    /// yields `&mut ObarrayView`, so safe code cannot `mem::swap` two
    /// Contexts' views and cross-link their Worlds.
    #[doc(hidden)]
    #[inline(always)]
    pub fn obarray_view_mut(&mut self) -> ObarrayViewMut<'_> {
        ObarrayViewMut::new(&mut self.obarray)
    }

    /// Resolve a fringe-bitmap symbol value to its registry index and data.
    /// Used by the display pipeline to turn a `(left-fringe SYMBOL FACE)` spec
    /// into a renderable bitmap. Returns `None` for a non-symbol or a symbol
    /// with no registered user bitmap.
    pub fn fringe_bitmap_for_symbol(
        &self,
        symbol: Value,
    ) -> Option<(u32, &super::super::builtins::fringe_bitmap::FringeBitmap)> {
        let symbol_mode = self.symbol_mode();
        let sym = symbol.as_symbol_id(symbol_mode)?;
        let index = self.fringe_bitmaps.index_of(sym)?;
        let bitmap = self.fringe_bitmaps.get(sym)?;
        Some((index, bitmap))
    }

    /// Borrow the fringe-bitmap registry (read-only) for per-frame snapshots.
    pub fn fringe_bitmap_registry(
        &self,
    ) -> &super::super::builtins::fringe_bitmap::FringeBitmapRegistry {
        &self.fringe_bitmaps
    }

    /// Resolve a fringe-bitmap symbol NAME (e.g. `"empty-line"`) to its registry
    /// index, the way the display pipeline needs when it produces a synthetic
    /// indicator row (empty-line `~` markers) rather than reacting to an explicit
    /// `(left-fringe SYM)` display spec. Returns `None` if no bitmap is
    /// registered under that name.
    pub fn fringe_bitmap_index_for_name(&self, name: &str) -> Option<u32> {
        let sym = neovm_value::intern::intern(name);
        self.fringe_bitmaps.index_of(sym)
    }

    /// The buffer-default value of a variable (GNU `BVAR(&buffer_defaults, …)`).
    /// For a slot-backed per-buffer variable this reads `buffer_defaults[slot]`
    /// (the value seen by buffers that haven't set a local override); for any
    /// other variable it falls back to the obarray value cell. Used by the
    /// layout-side fringe-indicator resolver, which mirrors GNU's two-level
    /// lookup (buffer-local, then `buffer_defaults`) — the obarray value cell of
    /// a forwarded slot var is always nil, so the resolver must read the default
    /// slot directly.
    pub fn buffer_default_value(&self, name: &str) -> Option<Value> {
        // GNU default read: BVAR (&buffer_defaults, slot) for a slot-backed
        // per-buffer variable (src/buffer.c:1116, 2289); for any other
        // variable the obarray default cell is GNU's Fdefault_value.
        if let Some(info) = neovm_edit::buffer::buffer::lookup_buffer_slot(name) {
            return Some(self.edit.buffers.buffer_defaults[info.offset.index()]);
        }
        self.default_value_only_by_name(name)
    }

    /// [`Self::buffer_default_value`] for a fixed GNU spelling.
    ///
    /// Identical two-level lookup, with the variable name resolved once per
    /// registry epoch instead of at every layout probe.
    pub fn buffer_default_value_fixed(&self, symbol: &WellKnown) -> Option<Value> {
        // Same GNU default read as buffer_default_value: BVAR
        // (&buffer_defaults, slot), src/buffer.c:1116, 2289.
        if let Some(info) = neovm_edit::buffer::buffer::lookup_buffer_slot(symbol.name()) {
            return Some(self.edit.buffers.buffer_defaults[info.offset.index()]);
        }
        self.default_value_only(symbol.id())
    }

    /// Seed the 24 GNU standard built-in fringe bitmaps into the registry and
    /// set each bitmap symbol's `'fringe` plist property to its index.
    ///
    /// Mirrors GNU `syms_of_fringe` (which installs `standard_bitmaps[]` and the
    /// `'fringe` indices). This is the authoritative seed in Rust: it makes the
    /// standard bitmaps resolvable even in contexts that never load
    /// `lisp/fringe.el` (e.g. unit-test `Context::new()`), so an explicit
    /// `(left-fringe right-arrow …)` display spec resolves to a real bitmap. In a
    /// fully lisp-loaded runtime, fringe.el's `(put SYM 'fringe N)` loop runs too
    /// (we bind `fringe-bitmaps` so its `(boundp 'fringe-bitmaps)` guard passes)
    /// and re-puts the SAME indices — idempotent because the orders agree.
    /// Called from `Context::new` after the obarray is populated.
    #[doc(hidden)]
    pub fn pre_register_standard_fringe_bitmaps(&mut self) {
        let assigned = self.fringe_bitmaps.pre_register_standard_bitmaps();
        let symbols_with_pos_enabled = self.symbols_with_pos_enabled;
        let fringe_prop = Value::symbol("fringe");
        for (sym, index) in assigned {
            let sym_value = Value::from_sym_id(sym);
            // Ignore the (unreachable) plist error path: these are freshly
            // interned symbols with nil plists.
            // SAFETY: this noncalling plist operation preserves function cells,
            // canonical descriptor identity and manager ownership; no loan escapes.
            let _ = unsafe {
                self.obarray.with_legacy_value_write(|obarray| {
                    super::super::builtins::symbols::put_in_obarray_values(
                        obarray,
                        sym_value,
                        fringe_prop,
                        Value::fixnum(i64::from(index)),
                        symbols_with_pos_enabled,
                    )
                })
            };
        }
        self.pre_register_standard_fringe_indicator_alist();
    }

    /// Seed the GNU default `fringe-indicator-alist` / `fringe-cursor-alist`
    /// (`lisp/fringe.el`'s `(boundp 'fringe-bitmaps)`-guarded `setq-default`
    /// block, ~lines 65-84), so the logical-indicator resolver
    /// (`get_logical_fringe_bitmap`) finds the standard truncation / continuation
    /// / empty-line bitmaps even in a bare `Context::new()` that never loads
    /// `lisp/fringe.el`. In a fully lisp-loaded runtime fringe.el runs the same
    /// `setq-default`, which simply overwrites this identical default. Only seeds
    /// when the current default is still nil so a loaded fringe.el (or user
    /// customization that ran first) is never clobbered.
    #[doc(hidden)]
    pub fn pre_register_standard_fringe_indicator_alist(&mut self) {
        let Some(info) = neovm_edit::buffer::buffer::lookup_buffer_slot("fringe-indicator-alist")
        else {
            return;
        };
        let offset = info.offset.index();
        // Don't clobber a default already installed (loaded fringe.el / user).
        if !self.edit.buffers.buffer_defaults[offset].is_nil() {
            return;
        }
        // Build the literal GNU default alist (`lisp/fringe.el` ~65-77). Evaluate
        // the quoted form so the cons cells are heap-allocated and rooted by the
        // evaluator; `setq-default`'s per-buffer-default plumbing is too early to
        // rely on during init, so write the resolved default slot directly.
        let Ok(alist) = self.eval_str(
            "'((truncation . (left-arrow right-arrow)) \
               (continuation . (left-curly-arrow right-curly-arrow)) \
               (overlay-arrow . right-triangle) \
               (up . up-arrow) \
               (down . down-arrow) \
               (top . (top-left-angle top-right-angle)) \
               (bottom . (bottom-left-angle bottom-right-angle \
                          top-right-angle top-left-angle)) \
               (top-bottom . (left-bracket right-bracket \
                              top-right-angle top-left-angle)) \
               (empty-line . empty-line) \
               (unknown . question-mark))",
        ) else {
            return;
        };
        // Set the per-buffer default (GNU `buffer_defaults`) and propagate to
        // every existing buffer still using the default (its slot was copied
        // from `buffer_defaults` at creation, before this seed ran). GNU buffers
        // read `buffer_defaults` live; neomacs copies, so update the copies that
        // are still nil (no explicit local override).
        self.edit.buffers.buffer_defaults_mut()[offset] = alist;
        self.edit
            .buffers
            .seed_default_slot_into_unset_buffers(info.offset, alist);
    }

    /// Access the opaque manager mutation interface.
    ///
    /// Its bounded methods retire publishing writes themselves; this does not
    /// expose an unrestricted mutable Obarray or a forgettable writer guard.
    #[inline(always)]
    pub fn obarray_mut(&mut self) -> ObarrayWriteAccess<'_> {
        ObarrayWriteAccess::new(&mut self.obarray)
    }

    /// Run a bounded mutation through the closed manager interface.
    ///
    /// The higher-ranked capability cannot escape. Each publishing method
    /// retires before it returns, including permitted Rust unwinding. The
    /// capability has no pending Drop obligation that a caller could forget.
    #[inline(always)]
    pub fn with_obarray_mut<R>(
        &mut self,
        f: impl for<'phase> FnOnce(ObarrayWriteAccess<'phase>) -> R,
    ) -> R {
        f(self.obarray_mut())
    }

    /// Public read access to the buffer manager.
    pub fn buffer_manager(&self) -> &BufferManager {
        &self.edit.buffers
    }

    /// Public mutable access to the buffer manager.
    pub fn buffer_manager_mut(&mut self) -> &mut BufferManager {
        &mut self.edit.buffers
    }

    /// Public read access to the frame manager.
    pub fn frame_manager(&self) -> &FrameManager {
        &self.frames
    }

    /// Public mutable access to the frame manager.
    pub fn frame_manager_mut(&mut self) -> &mut FrameManager {
        &mut self.frames
    }

    // --- PC11-F field accessors (R-b) --------------------------------------
    //
    // The eval move made these World slots/views plain production `pub`,
    // which let any downstream crate read and write authority-bearing World
    // state by field.  They are `pub(crate)` again; cross-crate callers use
    // these generated getters.  `_mut` exists only for fields that callers
    // outside this crate actually write, and its callers are recorded in the
    // signed R-b rows.  Every `_mut` borrows the Context exclusively, so the
    // World's one-owner-at-a-time rule is unchanged.

    /// Read the layout-query adapter state (the LayoutBusy observation).
    #[doc(hidden)]
    #[inline]
    pub fn window_layout_query_adapter(&self) -> &WindowLayoutQueryAdapter {
        &self.window_layout_query_adapter
    }

    /// Install the frontend layout-query handler.
    ///
    /// Returns `Err(LayoutQueryInstallError::Running)` when a synchronous
    /// layout query is already in flight; the handler is not installed and
    /// the Running state is preserved, so `LayoutBusy` cannot be bypassed.
    #[doc(hidden)]
    #[inline]
    pub fn install_window_layout_query_handler<F>(
        &mut self,
        query: F,
    ) -> Result<(), LayoutQueryInstallError>
    where
        F: FnMut(
                &mut Context,
                FrameId,
                WindowId,
                WindowLayoutQueryScope,
            ) -> WindowLayoutQueryOutcome
            + 'static,
    {
        install_layout_query_adapter(
            &mut self.window_layout_query_adapter,
            WindowLayoutQueryAdapter::Ready(Box::new(query)),
        )
    }

    /// Enter the Running state and mint the take proof the caller must
    /// restore with [`Self::restore_window_layout_query`].
    ///
    /// A nested query may take while already running; its token carries
    /// `Running` and it reports `LayoutBusy` instead of starting a second
    /// walk. The token is `#[must_use]` and consumed by the restore, so the
    /// transition cannot be forgotten or repeated.
    #[doc(hidden)]
    #[inline]
    pub fn take_window_layout_query(&mut self) -> LayoutQueryTake {
        let owner = self.context_instance_id();
        take_layout_query_adapter(&mut self.window_layout_query_adapter, owner)
    }

    /// Consume the take proof and restore the adapter it replaced.
    ///
    /// The token is consumed; the owner and the outstanding Running state are
    /// asserted in release builds too, so misuse is never a silent no-op.
    #[doc(hidden)]
    #[inline]
    pub fn restore_window_layout_query(&mut self, take: LayoutQueryTake) {
        let owner = self.context_instance_id();
        restore_layout_query_adapter(&mut self.window_layout_query_adapter, owner, take)
    }

    #[doc(hidden)]
    #[inline]
    pub fn current_message(&self) -> &Option<neovm_value::heap_types::LispString> {
        &self.current_message
    }

    #[doc(hidden)]
    #[inline]
    pub fn threads(&self) -> &ThreadManager {
        &self.threads
    }

    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    #[inline]
    pub fn quit_flag_copied(&self) -> Value {
        self.quit_flag
    }

    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    #[inline]
    pub fn throw_on_input_copied(&self) -> Value {
        self.throw_on_input
    }

    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    #[inline]
    pub fn specpdl_len_for_test(&self) -> usize {
        self.specpdl.len()
    }

    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    #[inline]
    pub fn jit_stack_limit_for_test(&self) -> usize {
        self.jit_stack_limit
    }

    /// Typed GC-root push for a Rust-local fixture the collector cannot see.
    /// Roots are owner-thread state, so this never yields the root table.
    ///
    /// # Safety
    /// `value` must be a live heap value owned by this Context's World and must
    /// not have been collected: the push publishes it as a GC root for the
    /// rest of the Context's life.
    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    #[inline]
    pub(crate) unsafe fn push_eval_temp_root_for_test(&mut self, value: neovm_value::value::Value) {
        self.eval_temp_roots.push(value);
    }

    /// Install (or clear) the retained pdump mapping the rows-authority
    /// fixture owns. The mapping is an owner lease, never a heap view; the
    /// Context stores only the caller's `'static` reference.
    ///
    /// # Safety
    /// `image` must be a genuinely mmapped, process-retained pdump image whose
    /// rows authority the Context may adopt; the mapping must outlive the
    /// Context and must not be mutated or unmapped while installed.
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) unsafe fn install_pdump_image_for_test(
        &mut self,
        image: Option<&'static super::super::pdump::mmap_image::LoadedMmapImage>,
    ) {
        self.pdump_image = image;
    }

    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    #[inline]
    pub fn last_redisplay_signature_is_none_for_test(&self) -> bool {
        self.last_redisplay_signature.is_none()
    }

    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    #[inline]
    pub fn last_redisplay_signature_is_some_for_test(&self) -> bool {
        self.last_redisplay_signature.is_some()
    }

    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    #[inline]
    pub fn standard_syntax_table_copied(&self) -> Value {
        self.standard_syntax_table
    }

    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    #[inline]
    pub fn syntax_code_objects_copied(&self) -> Value {
        self.syntax_code_objects
    }

    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    #[inline]
    pub fn standard_category_table_copied(&self) -> Value {
        self.standard_category_table
    }

    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub fn collect_font_registry_gc_roots_for_test(&self, roots: &mut Vec<Value>) {
        self::data::face_registry::collect_font_registry_gc_roots(&self.font_registry, roots);
    }

    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub fn collect_hash_table_test_registry_gc_roots_for_test(&self, roots: &mut Vec<Value>) {
        crate::emacs_core::builtins::collect_hash_table_test_registry_gc_roots(
            &self.hash_table_test_registry,
            roots,
        );
    }
    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub fn collect_ccl_registry_gc_roots_for_test(&self, roots: &mut Vec<Value>) {
        neovm_text::emacs_core::ccl::collect_ccl_registry_gc_roots(&self.ccl_registry, roots);
    }

    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub fn collect_charset_registry_gc_roots_for_test(&self, roots: &mut Vec<Value>) {
        neovm_text::emacs_core::charset::collect_charset_registry_gc_roots(
            &self.charset_registry,
            roots,
        );
    }

    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub fn collect_in_flight_registry_gc_roots_for_test(&self, roots: &mut Vec<Value>) {
        neovm_value::error::collect_in_flight_registry_gc_roots(
            roots,
            &self.in_flight_registry,
            self.tagged_heap().tagged_heap_identity(),
        );
    }

    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    #[inline]
    pub fn features(&self) -> &[SymId] {
        &self.features
    }

    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    #[inline]
    pub fn require_stack(&self) -> &[SymId] {
        &self.require_stack
    }

    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    #[inline]
    pub fn loads_in_progress(&self) -> &[neovm_value::heap_types::LispString] {
        &self.loads_in_progress
    }

    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    #[inline]
    pub fn cached_system_name(&self) -> Value {
        *self.cached_system_name
    }

    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub(crate) fn create_thread_for_test(
        &mut self,
        function: Value,
        name: crate::emacs_core::threads::ThreadName,
    ) {
        self.threads
            .try_create_thread(function, name)
            .expect("test identity issuance");
    }

    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub(crate) fn insert_feature_for_test(&mut self, index: usize, feature: SymId) {
        self.features.insert(index, feature);
    }

    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub(crate) fn push_require_stack_for_test(&mut self, sym: SymId) {
        self.require_stack.push(sym);
    }

    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub(crate) fn push_loads_in_progress_for_test(
        &mut self,
        value: neovm_value::heap_types::LispString,
    ) {
        self.loads_in_progress.push(value);
    }

    #[doc(hidden)]
    #[inline]
    pub fn command_loop(&self) -> &crate::emacs_core::eval::data::command_loop::CommandLoop {
        &self.command_loop
    }

    #[doc(hidden)]
    #[inline]
    pub fn command_loop_mut(
        &mut self,
    ) -> &mut crate::emacs_core::eval::data::command_loop::CommandLoop {
        &mut self.command_loop
    }

    #[doc(hidden)]
    #[inline]
    pub fn interactive(&self) -> &InteractiveRegistry {
        &self.interactive
    }

    #[doc(hidden)]
    #[inline]
    pub fn interactive_mut(&mut self) -> &mut InteractiveRegistry {
        &mut self.interactive
    }

    #[doc(hidden)]
    #[inline]
    pub fn minibuffers(&self) -> &MinibufferManager {
        &self.minibuffers
    }

    #[doc(hidden)]
    #[inline]
    pub fn minibuffers_mut(&mut self) -> &mut MinibufferManager {
        &mut self.minibuffers
    }

    #[inline]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn bookmarks(&self) -> &BookmarkManager {
        &self.bookmarks
    }

    #[inline]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn bookmarks_mut(&mut self) -> &mut BookmarkManager {
        &mut self.bookmarks
    }

    #[inline]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn treesit(&self) -> &neovm_text::emacs_core::treesit::TreeSitterManager {
        &self.treesit
    }

    #[inline]
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn treesit_mut(
        &mut self,
    ) -> &mut neovm_text::emacs_core::treesit::TreeSitterManager {
        &mut self.treesit
    }

    #[inline]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn modes(&self) -> &ModeRegistry {
        &self.modes
    }

    #[inline]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn modes_mut(&mut self) -> &mut ModeRegistry {
        &mut self.modes
    }

    #[inline]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn rectangle(&self) -> &RectangleState {
        &self.rectangle
    }

    #[inline]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn rectangle_mut(&mut self) -> &mut RectangleState {
        &mut self.rectangle
    }

    #[doc(hidden)]
    #[inline]
    pub fn daemon(&self) -> &Option<DaemonState> {
        &self.daemon
    }

    #[doc(hidden)]
    #[inline]
    pub fn processes(&self) -> &ProcessManager {
        &self.processes
    }

    #[doc(hidden)]
    #[inline]
    pub fn processes_mut(&mut self) -> &mut ProcessManager {
        &mut self.processes
    }

    #[doc(hidden)]
    #[inline]
    pub fn xwidgets(&self) -> &xwidget::XwidgetState {
        &self.xwidgets
    }

    #[doc(hidden)]
    #[inline]
    pub fn xwidgets_mut(&mut self) -> &mut xwidget::XwidgetState {
        &mut self.xwidgets
    }

    #[doc(hidden)]
    #[inline]
    pub fn registers(&self) -> &RegisterManager {
        &self.registers
    }

    #[doc(hidden)]
    #[inline]
    pub fn registers_mut(&mut self) -> &mut RegisterManager {
        &mut self.registers
    }

    #[doc(hidden)]
    #[inline]
    pub fn coding_systems(&self) -> &CodingSystemManager {
        &self.coding_systems
    }

    /// Mutable access for the callers that pass the manager to a coding-system
    /// builtin (the re-homed coding tests drive the real builtins).
    #[doc(hidden)]
    #[inline]
    pub fn coding_systems_mut(&mut self) -> &mut CodingSystemManager {
        &mut self.coding_systems
    }

    /// Split borrow: the frame manager and the buffer manager, mutably.
    ///
    /// One method borrows the whole Context; callers that need both managers
    /// at once (the display layer's window/frame edits) cannot split fields
    /// through separate getters.  This returns the two disjoint borrows that
    /// field syntax used to provide.
    #[doc(hidden)]
    #[inline]
    pub fn frame_manager_and_buffers_mut(&mut self) -> (&mut FrameManager, &mut BufferManager) {
        (&mut self.frames, &mut self.edit.buffers)
    }

    /// Split borrow: the frame manager by shared reference and the buffer
    /// manager mutably (window-buffer sync reads frames, writes buffers).
    #[doc(hidden)]
    #[inline]
    pub fn frame_manager_read_and_buffers_mut(&mut self) -> (&FrameManager, &mut BufferManager) {
        (&self.frames, &mut self.edit.buffers)
    }

    /// Split borrow: the process manager and the buffer manager, mutably.
    #[doc(hidden)]
    #[inline]
    pub fn processes_and_buffers_mut(&mut self) -> (&mut ProcessManager, &mut BufferManager) {
        (&mut self.processes, &mut self.edit.buffers)
    }

    /// Split borrow: the process manager mutably and the thread manager by
    /// shared reference (status publication reads the current thread).
    #[doc(hidden)]
    #[inline]
    pub fn processes_and_threads(&mut self) -> (&mut ProcessManager, &ThreadManager) {
        (&mut self.processes, &self.threads)
    }

    /// Split borrow: the frame manager and the display host, both mutably.
    #[doc(hidden)]
    #[inline]
    pub fn frame_manager_and_display_host_mut(
        &mut self,
    ) -> (&mut FrameManager, &mut Option<Box<dyn DisplayHost>>) {
        (&mut self.frames, &mut self.display_host)
    }

    #[doc(hidden)]
    #[inline]
    pub fn minibuffer_selected_window(
        &self,
    ) -> &Option<neovm_edit::emacs_core::display_metrics::WindowId> {
        &self.minibuffer_selected_window
    }

    #[doc(hidden)]
    #[inline]
    pub fn minibuffer_selected_window_mut(
        &mut self,
    ) -> &mut Option<neovm_edit::emacs_core::display_metrics::WindowId> {
        &mut self.minibuffer_selected_window
    }

    #[doc(hidden)]
    #[inline]
    pub fn active_minibuffer_window(
        &self,
    ) -> &Option<neovm_edit::emacs_core::display_metrics::WindowId> {
        &self.active_minibuffer_window
    }

    #[doc(hidden)]
    #[inline]
    pub fn active_minibuffer_window_mut(
        &mut self,
    ) -> &mut Option<neovm_edit::emacs_core::display_metrics::WindowId> {
        &mut self.active_minibuffer_window
    }

    #[doc(hidden)]
    #[inline]
    pub fn gstring_shape_cache_mut(
        &mut self,
    ) -> &mut std::collections::HashMap<neovm_value::display_vocab::GstringShapeCacheKey, Value>
    {
        &mut self.gstring_shape_cache
    }

    /// Split borrow: the frame manager mutably and the two minibuffer
    /// selection slots mutably.
    #[doc(hidden)]
    #[inline]
    pub fn frame_manager_and_minibuffer_selection_mut(
        &mut self,
    ) -> (
        &mut FrameManager,
        &mut Option<neovm_edit::emacs_core::display_metrics::WindowId>,
        &mut Option<neovm_edit::emacs_core::display_metrics::WindowId>,
    ) {
        (
            &mut self.frames,
            &mut self.minibuffer_selected_window,
            &mut self.active_minibuffer_window,
        )
    }

    /// Split borrow: the frame manager and buffer manager mutably, and the two
    /// minibuffer selection slots mutably.
    #[doc(hidden)]
    #[inline]
    pub fn frame_manager_buffers_and_minibuffer_selection_mut(
        &mut self,
    ) -> (
        &mut FrameManager,
        &mut BufferManager,
        &mut Option<neovm_edit::emacs_core::display_metrics::WindowId>,
        &mut Option<neovm_edit::emacs_core::display_metrics::WindowId>,
    ) {
        (
            &mut self.frames,
            &mut self.edit.buffers,
            &mut self.minibuffer_selected_window,
            &mut self.active_minibuffer_window,
        )
    }

    /// Split borrow: the obarray by shared reference and the buffer manager
    /// mutably (casing property modes borrow the obarray while editing text).
    #[doc(hidden)]
    #[inline]
    pub fn obarray_and_buffers_mut(&mut self) -> (&Obarray, &mut BufferManager) {
        (&self.obarray, &mut self.edit.buffers)
    }

    /// Split borrow for a specpdl-rooted runtime binding write: the buffer
    /// manager mutably, the owner slab mutably (when bound), the specpdl by
    /// shared reference, and the obarray view mutably.
    ///
    /// # Safety
    ///
    /// The caller must hold the one-owner phase for this Context and use the
    /// parts only for the bounded `with_legacy_value_write` publication that
    /// retires before it returns.  The owner slab and specpdl are authority
    /// state: mutating them outside that publication can break the World/owner
    /// and binding-stack invariants.
    #[doc(hidden)]
    #[inline]
    pub unsafe fn runtime_binding_parts_mut(
        &mut self,
    ) -> (
        &mut BufferManager,
        Option<&mut owner_bindings::OwnerSlab>,
        &[SpecBinding],
        &mut ObarrayView,
    ) {
        (
            &mut self.edit.buffers,
            self.owner.slab_mut(),
            self.specpdl.as_slice(),
            &mut self.obarray,
        )
    }

    /// Split borrow: the buffer manager mutably and the obarray view mutably.
    ///
    /// The view half is the non-swappable [`ObarrayViewMut`] capability.
    #[doc(hidden)]
    #[inline]
    pub fn buffers_and_obarray_view_mut(&mut self) -> (&mut BufferManager, ObarrayViewMut<'_>) {
        (
            &mut self.edit.buffers,
            ObarrayViewMut::new(&mut self.obarray),
        )
    }

    /// Split borrow: the obarray and owner bindings by shared reference, and
    /// the buffer manager mutably (syntax scans read variables, edit text).
    #[doc(hidden)]
    #[inline]
    pub fn variable_view_and_buffers_mut(
        &mut self,
    ) -> (&Obarray, &owner_bindings::OwnerSlot, &mut BufferManager) {
        (&self.obarray, &self.owner, &mut self.edit.buffers)
    }

    /// Split borrow: the frame manager by shared reference and the display
    /// host mutably (font resolution reads frames, publishes through the host).
    #[doc(hidden)]
    #[inline]
    pub fn frame_manager_read_and_display_host_mut(
        &mut self,
    ) -> (&FrameManager, &mut Option<Box<dyn DisplayHost>>) {
        (&self.frames, &mut self.display_host)
    }

    /// Split borrow: the frame manager, buffer manager and display host, all
    /// mutably.
    #[doc(hidden)]
    #[inline]
    pub fn frame_manager_buffers_and_display_host_mut(
        &mut self,
    ) -> (
        &mut FrameManager,
        &mut BufferManager,
        &mut Option<Box<dyn DisplayHost>>,
    ) {
        (
            &mut self.frames,
            &mut self.edit.buffers,
            &mut self.display_host,
        )
    }

    /// Split borrow: the process manager and the coding-system manager, both
    /// mutably.
    #[doc(hidden)]
    #[inline]
    pub fn processes_and_coding_systems_mut(
        &mut self,
    ) -> (&mut ProcessManager, &mut CodingSystemManager) {
        (&mut self.processes, &mut self.coding_systems)
    }

    /// Split borrow for xwidget publication: the xwidget state by shared
    /// reference, the owner bindings mutably, and the obarray view mutably.
    ///
    /// # Safety
    ///
    /// The caller must hold the one-owner phase for this Context (sole owner,
    /// or the Green Baton holder) and must use the returned parts only for the
    /// bounded `with_legacy_value_write` publication that retires before it
    /// returns.  The owner bindings are authority state: writing them outside
    /// that publication can break the World/owner invariant.
    #[doc(hidden)]
    #[inline]
    pub unsafe fn xwidgets_owner_and_obarray_view_mut(
        &mut self,
    ) -> (
        &xwidget::XwidgetState,
        &mut owner_bindings::OwnerSlot,
        &mut ObarrayView,
    ) {
        (&self.xwidgets, &mut self.owner, &mut self.obarray)
    }

    /// Split borrow for the mode line: the obarray and owner bindings by
    /// shared reference, the frame manager mutably, and the read-only buffer,
    /// process and coding-system managers.
    #[doc(hidden)]
    #[inline]
    pub fn mode_line_inputs_mut(
        &mut self,
    ) -> (
        &Obarray,
        &owner_bindings::OwnerSlot,
        &mut FrameManager,
        &BufferManager,
        &ProcessManager,
        &CodingSystemManager,
    ) {
        (
            &self.obarray,
            &self.owner,
            &mut self.frames,
            &self.edit.buffers,
            &self.processes,
            &self.coding_systems,
        )
    }

    /// Test-support split borrow for the mode-line formatting tests: the
    /// obarray and owner bindings by shared reference, the frame and process
    /// managers by shared reference, and the buffer manager mutably.
    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    #[inline]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    pub(crate) fn mode_line_formatting_inputs_mut(
        &mut self,
    ) -> (
        &Obarray,
        &owner_bindings::OwnerSlot,
        &FrameManager,
        &mut BufferManager,
        &ProcessManager,
    ) {
        (
            &self.obarray,
            &self.owner,
            &self.frames,
            &mut self.edit.buffers,
            &self.processes,
        )
    }

    /// Split borrow for variable reads that also scroll a window: the obarray
    /// and owner bindings by shared reference, the frame and buffer managers
    /// mutably.
    #[doc(hidden)]
    #[inline]
    pub fn variable_view_and_frames_buffers_mut(
        &mut self,
    ) -> (
        &Obarray,
        &owner_bindings::OwnerSlot,
        &mut FrameManager,
        &mut BufferManager,
    ) {
        (
            &self.obarray,
            &self.owner,
            &mut self.frames,
            &mut self.edit.buffers,
        )
    }

    /// Split borrow: the frame manager and buffer manager mutably, and the
    /// minibuffer manager by shared reference.
    #[doc(hidden)]
    #[inline]
    pub fn frame_manager_buffers_and_minibuffers_mut(
        &mut self,
    ) -> (&mut FrameManager, &mut BufferManager, &MinibufferManager) {
        (&mut self.frames, &mut self.edit.buffers, &self.minibuffers)
    }

    /// Split borrow: the process manager (mutable), the thread manager
    /// (shared), and the buffer manager (mutable).  Status publication reads
    /// the current thread while mutating a process record and syncing its
    /// mark in the buffer manager.
    #[doc(hidden)]
    #[inline]
    pub fn processes_threads_and_buffers_mut(
        &mut self,
    ) -> (&mut ProcessManager, &ThreadManager, &mut BufferManager) {
        (&mut self.processes, &self.threads, &mut self.edit.buffers)
    }

    /// Split borrow: processes and buffers mutably, threads and coding
    /// systems by shared reference (process creation reads both).
    #[doc(hidden)]
    #[inline]
    pub fn processes_threads_buffers_and_coding_systems_mut(
        &mut self,
    ) -> (
        &mut ProcessManager,
        &ThreadManager,
        &mut BufferManager,
        &mut CodingSystemManager,
    ) {
        (
            &mut self.processes,
            &self.threads,
            &mut self.edit.buffers,
            &mut self.coding_systems,
        )
    }

    pub fn current_message_text(&self) -> Option<String> {
        self.current_message
            .as_ref()
            .map(|message| neovm_value::emacs_char::to_utf8_lossy(message.as_bytes()))
    }

    /// Whether an echo-area message is currently displayed.
    ///
    /// GNU's condition for installing the echo-area buffer in the inactive
    /// mini-window is `echo_area_buffer[0]` being non-nil while the message
    /// is up (xdisp.c `with_echo_area_buffer`); callers such as the layout
    /// engine's display-source decision must not render the echo buffer once
    /// the message has cleared, because the idle mini-window then shows its
    /// own buffer — `" *Minibuf-0*"` (minibuf.c:1281-1284) — whose contents
    /// packages like `minibuffer-line' own.
    pub fn has_current_message(&self) -> bool {
        self.current_message.is_some()
    }

    /// Whether redisplay should move the active cursor into an inactive echo
    /// area while displaying the current message.
    ///
    /// GNU's `get_window_cursor_type` reads the dynamically visible value of
    /// `cursor-in-echo-area`; exposing the semantic request here keeps layout
    /// from bypassing specbindings through the obarray's global value cell.
    /// An active minibuffer already owns the live selected-window cursor and
    /// therefore is not an inactive echo-area redirection.
    pub fn inactive_echo_area_cursor_requested(&self) -> bool {
        self.current_message.is_some()
            && !self.minibuffer_is_active()
            && self
                .visible_variable_value_or_nil("cursor-in-echo-area")
                .is_truthy()
    }

    pub fn current_message_value(&self) -> Option<Value> {
        self.current_message
            .as_ref()
            .map(|message| Value::heap_string(message.clone()))
    }

    /// Whether the next redisplay should resize the echo-area mini-window
    /// exactly to its content (GNU `resize_echo_area_exactly`, the post-command
    /// `exact_p = minibuf_level == 0` case in src/xdisp.c:13235). Read by the
    /// layout engine's grow-only mini-window shrink check. The flag is cleared
    /// once per redisplay in `redisplay_with_force` so a later mid-command
    /// redisplay does not keep shrinking a freshly grown message (GNU only
    /// resizes exactly at the command boundary, not on every `redisplay_window`).
    pub fn echo_area_resize_exact_pending(&self) -> bool {
        self.echo_area_resize_exact_pending
    }

    pub fn set_current_message(&mut self, message: Option<neovm_value::heap_types::LispString>) {
        // An ordinary message takes ownership of the echo area away from the
        // keyboard reader. Keyboard echo publication restores its typed state
        // only after installing its own message.
        self.cancel_key_echo_state();
        self.set_message_buf_print(false);
        if (*self.current_message) != message {
            // GNU keeps no copy: `current_message` (src/xdisp.c:13420) reads the
            // echo buffer back with `make_buffer_string (BEG, Z, true)`
            // (`current_message_1`, src/xdisp.c:13437-13446), so what Lisp sees
            // is the text as the buffer holds it -- multibyte whenever the
            // buffer is. Store what was written, not what was handed in.
            let stored = self.mirror_message_to_echo_area_buffer(message.as_ref());
            *self.current_message = stored.or(message);
            self.invalidate_redisplay();
        }
    }

    /// Mirror the current echo message into the ` *Echo Area 0*` buffer, the way
    /// GNU `set_message_1` (`src/xdisp.c`) does: clear the echo buffer and insert
    /// the message text at BEG. This keeps the echo-area buffer as the single
    /// source of truth for the message text so redisplay can render it as
    /// ordinary buffer text (the GNU `display_echo_area_1` model). The echo
    /// reroute has landed: the layout engine renders the inactive echo area
    /// through this ` *Echo Area 0*` buffer via the normal buffer walk (not from
    /// `current_message`), so keeping this buffer in sync is load-bearing.
    ///
    /// Returns the text as the echo buffer now holds it, which is what GNU
    /// `current_message` reads back out (src/xdisp.c:13437-13446); `None` when
    /// there was nothing to write or no echo buffer to write it to.
    #[doc(hidden)]
    pub fn mirror_message_to_echo_area_buffer(
        &mut self,
        message: Option<&neovm_value::heap_types::LispString>,
    ) -> Option<neovm_value::heap_types::LispString> {
        match message {
            Some(message) => {
                // GNU `set_message_1` runs inside `with_echo_area_buffer`
                // (xdisp.c:12904), which calls `ensure_echo_area_buffers ()`
                // first — so setting a message always materializes the echo
                // buffers. Creation order stays correct because `builtin_message`
                // logs *Messages* (message_dolog) BEFORE set_current_message.
                self.ensure_echo_area_buffers();
                let id = self.echo_area_display_buffer()?;
                // GNU `with_echo_area_buffer` clears the echo buffer
                // (`del_range (BEG, Z)`) BEFORE setting its multibyteness, then
                // inserts. Order matters: `set_buffer_multibyte_flag` only flips
                // the flag, so toggling it while the buffer still holds the
                // previous message — encoded in the OTHER multibyteness — makes
                // the subsequent full-range delete in
                // `replace_buffer_contents_lisp_string` miscompute its position
                // adjustment and panic ("buffer text edit position underflow").
                // Clear first (with the flag still matching the existing
                // content), then toggle on the now-empty buffer, then insert.
                let _ = self.edit.buffers.replace_buffer_contents(id, "");
                let buffer_is_multibyte = self
                    .edit
                    .buffers
                    .get(id)
                    .map(|buffer| buffer.get_multibyte())?;
                let resolved = EchoAreaMessageText::resolve(
                    message,
                    buffer_is_multibyte,
                    self.visible_variable_value_or_nil("unibyte-display-via-language-environment")
                        .is_truthy(),
                );
                let _ = self
                    .edit
                    .buffers
                    .set_buffer_multibyte_flag(id, resolved.buffer_is_multibyte);
                let _ = self
                    .edit
                    .buffers
                    .replace_buffer_contents_lisp_string(id, &resolved.text);
                Some(resolved.text)
            }
            None => {
                // Clearing the message: only touch the echo buffer if it already
                // exists; do not materialize it just to empty it.
                let id = self.echo_area_display_buffer()?;
                let _ = self.edit.buffers.replace_buffer_contents(id, "");
                None
            }
        }
    }

    /// GNU `ensure_echo_area_buffers` (src/xdisp.c:12862-12884).
    ///
    /// A slot is (re)filled only when it holds no buffer or the buffer it holds
    /// has died -- never merely because no buffer answers to the canonical
    /// name. That is what keeps the echo area attached to its buffer across a
    /// rename, and what keeps an unrelated user buffer standing at the name
    /// from being adopted and overwritten. Filling uses `Fget_buffer_create`
    /// semantics, as GNU does, so a dump-restored buffer is re-adopted once and
    /// identified by id from then on.
    pub fn ensure_echo_area_buffers(&mut self) {
        for index in 0..EchoAreaBuffers::NAMES.len() {
            let live = self.echo_area_buffers.slots[index]
                .filter(|id| self.edit.buffers.get(*id).is_some());
            let id = match live {
                Some(id) => id,
                None => {
                    let name = EchoAreaBuffers::NAMES[index];
                    let id = self
                        .edit
                        .buffers
                        .find_buffer_by_name(name)
                        .unwrap_or_else(|| {
                            let id = self.edit.buffers.create_buffer(name);
                            let _ = self.edit.buffers.set_buffer_local_property(
                                id,
                                "truncate-lines",
                                Value::NIL,
                            );
                            id
                        });
                    self.echo_area_buffers.slots[index] = Some(id);
                    id
                }
            };
            let _ = self.edit.buffers.configure_buffer_undo_list(id, Value::T);
        }
    }

    /// The buffer an inactive mini-window displays the current message from.
    ///
    /// GNU reaches it through `with_echo_area_buffer', which installs it in the
    /// window for the duration of display and restores `w->contents` on unwind
    /// (src/xdisp.c:12961, :13038). Callers that need it to exist must call
    /// [`Self::ensure_echo_area_buffers`] first, exactly as GNU does.
    pub fn echo_area_display_buffer(&self) -> Option<neovm_edit::buffer::BufferId> {
        self.echo_area_buffers
            .display_slot()
            .filter(|id| self.edit.buffers.get(*id).is_some())
    }

    #[doc(hidden)]
    pub fn append_current_message_runtime_text(&mut self, text: &str) {
        let multibyte = self
            .current_message
            .as_ref()
            .map(neovm_value::heap_types::LispString::is_multibyte)
            .unwrap_or(true);
        let piece = crate::emacs_core::builtins::plain_str_to_lisp_string(text, multibyte);
        self.append_current_message_lisp_string(&piece);
    }

    #[doc(hidden)]
    pub fn append_current_message_lisp_string(
        &mut self,
        text: &neovm_value::heap_types::LispString,
    ) {
        match self.current_message.as_mut() {
            Some(message) => *message = message.concat(text),
            None => *self.current_message = Some(text.clone()),
        }
        let current = self.current_message.clone();
        if let Some(stored) = self.mirror_message_to_echo_area_buffer(current.as_ref()) {
            *self.current_message = Some(stored);
        }
        self.invalidate_redisplay();
    }

    #[doc(hidden)]
    pub fn append_echo_area_print_runtime_text(&mut self, text: &str) {
        if !self.noninteractive() {
            self.ensure_echo_area_buffers();
        }
        if !self.message_buf_print() {
            *self.current_message = None;
            self.set_message_buf_print(true);
        }
        self.append_current_message_runtime_text(text);
    }

    /// Emacs-bytes echo-area sibling of [`Self::append_echo_area_print_runtime_text`].
    /// Used by the byte-faithful print sink (`prin1`/`print`/`write-char`) so a
    /// real Private-Use glyph in the printer output is not reinterpreted as a
    /// raw byte by the storage-string echo path (issue #131).
    #[doc(hidden)]
    pub fn append_echo_area_print_lisp_string(
        &mut self,
        text: &neovm_value::heap_types::LispString,
    ) {
        if !self.noninteractive() {
            self.ensure_echo_area_buffers();
        }
        if !self.message_buf_print() {
            *self.current_message = None;
            self.set_message_buf_print(true);
        }
        self.append_current_message_lisp_string(text);
    }

    #[doc(hidden)]
    pub fn discard_current_message_without_clear_hook(&mut self) {
        self.set_message_buf_print(false);
        if self.current_message.take().is_some() {
            let _ = self.mirror_message_to_echo_area_buffer(None);
            self.invalidate_redisplay();
        }
    }

    #[doc(hidden)]
    pub fn clear_echo_area_message_with_hook(
        &mut self,
        run_echo_area_clear_hook: bool,
    ) -> EchoMessageClearResult {
        self.set_message_buf_print(false);
        if self
            .visible_variable_value_or_nil_by_id(INHIBIT_MESSAGE.id())
            .is_truthy()
        {
            return EchoMessageClearResult::PreserveEchoArea;
        }

        let had_current_message = self.current_message.is_some();
        let mut called_clear_function = false;
        let mut clear_result = EchoMessageClearResult::ClearEchoArea;

        let clear_message_function =
            self.visible_variable_value_or_nil_by_id(CLEAR_MESSAGE_FUNCTION.id());
        if !clear_message_function.is_nil()
            && self.gc_inhibit_depth == 0
            && self.function_value_is_callable(&clear_message_function)
        {
            called_clear_function = true;
            let specpdl_count = self.specpdl.len();
            if let Err(err) = self.try_specbind_or_unwind_to(
                specpdl_count,
                neovm_value::intern::WellKnown::InhibitQuit.id(),
                Value::T,
            ) {
                tracing::warn!(
                    "inhibit-quit watcher signaled while clearing echo message: {:?}",
                    err
                );
                return EchoMessageClearResult::PreserveEchoArea;
            }
            let result = self.funcall_general(clear_message_function, vec![]);
            let result = self.unbind_to_with_result(specpdl_count, result);

            match result {
                Ok(value) if value.is_symbol_named("dont-clear-message", self.symbol_mode()) => {
                    clear_result = EchoMessageClearResult::PreserveEchoArea;
                }
                Ok(_) => {}
                Err(err) => {
                    tracing::warn!(
                        "clear-message-function signaled while clearing echo message: {:?}",
                        err
                    );
                }
            }
        }

        if clear_result == EchoMessageClearResult::PreserveEchoArea {
            if called_clear_function {
                self.invalidate_redisplay();
            }
            return clear_result;
        }

        if had_current_message && run_echo_area_clear_hook {
            let hook = crate::emacs_core::hook_runtime::hook_symbol_by_id(
                self,
                echo_area_clear_hook_symbol(),
            );
            let _ = crate::emacs_core::hook_runtime::safe_run_named_hook(self, hook, &[]);
        }

        let changed = self.current_message.take().is_some();
        if changed {
            let _ = self.mirror_message_to_echo_area_buffer(None);
        }
        if changed || called_clear_function {
            self.invalidate_redisplay();
        }
        clear_result
    }

    #[doc(hidden)]
    pub fn clear_echo_area_message(&mut self) -> EchoMessageClearResult {
        self.clear_echo_area_message_with_hook(true)
    }

    /// GNU `clear_message (current_p, last_displayed_p)` (src/xdisp.c:13620) as
    /// every non-keyboard caller issues it: `clear-message-function' is
    /// consulted, `echo-area-clear-hook' is NOT run.
    ///
    /// That hook is the keyboard reader's alone -- GNU runs it only from
    /// `src/keyboard.c:1399`, `:3235` and `:3288`, never from `clear_message` --
    /// so a caller such as `read_minibuf' (src/minibuf.c:894) must not fire it.
    ///
    /// GNU's two flags select the two slots of `echo_area_buffer[2]'
    /// (src/xdisp.c:785): the current message and the last displayed one. This
    /// port keeps no last-displayed slot (ledger 215 residual 1, still open), so
    /// there is no flag to pass and none is invented: the method is named for
    /// what it does here rather than carrying a parameter that is always
    /// ignored.
    #[doc(hidden)]
    pub fn clear_echo_area_message_without_clear_hook(&mut self) -> EchoMessageClearResult {
        self.clear_echo_area_message_with_hook(false)
    }

    /// Clear a message produced by key echoing without running
    /// `echo-area-clear-hook`. GNU's `echo_update` uses
    /// `message3_nolog(nil)` -> `clear_message`, which consults
    /// `clear-message-function` but does not run the keyboard reader's
    /// separate echo-area-clear hook.
    #[doc(hidden)]
    pub fn clear_key_echo_message(&mut self) {
        self.cancel_key_echo_state();
        let _ = self.clear_echo_area_message_with_hook(false);
    }

    pub fn clear_current_message(&mut self) {
        self.cancel_key_echo_state();
        if self.clear_echo_area_message() == EchoMessageClearResult::PreserveEchoArea {}
    }

    /// Clear stale echo-area cells while an input event is being ingested,
    /// without surrendering keyboard-echo ownership. GNU `read_char` clears
    /// the old message before `echo_add_key`, then immediately rebuilds it when
    /// `immediate_echo` is active.
    #[doc(hidden)]
    pub fn clear_current_message_for_keyboard_input(&mut self) {
        if self.clear_echo_area_message() == EchoMessageClearResult::PreserveEchoArea {}
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    #[doc(hidden)]
    pub fn current_message_slot(&mut self) -> &mut Option<neovm_value::heap_types::LispString> {
        &mut self.current_message
    }

    #[doc(hidden)]
    pub fn route_keyboard_input_to_frame(&mut self, emacs_frame_id: u64) {
        let frame_id = if emacs_frame_id == 0 {
            self.frames.selected_frame().map(|frame| frame.id)
        } else {
            Some(neovm_value::display_vocab::FrameId(emacs_frame_id))
        };
        if let Some(frame_id) = frame_id {
            self.observe_keyboard_input_frame(frame_id);
        } else {
            self.command_loop
                .keyboard
                .select_terminal(crate::emacs_core::eval::protocol::tty_host::TERMINAL_ID);
        }
    }

    #[doc(hidden)]
    pub fn route_tty_keyboard_input(
        &mut self,
        target: crate::emacs_core::eval::keyboard::TtyInputTarget,
    ) {
        use crate::emacs_core::eval::keyboard::TtyInputTarget;

        let frame_id = match target {
            TtyInputTarget::SelectedFrame => self.frames.selected_frame().map(|frame| frame.id),
            TtyInputTarget::Frame(frame_id) => self
                .frames
                .get(frame_id)
                .map(|frame| frame.id)
                .or_else(|| self.frames.selected_frame().map(|frame| frame.id)),
            TtyInputTarget::Terminal(terminal_id) => self
                .frames
                .selected_frame()
                .filter(|frame| frame.terminal_id == terminal_id)
                .map(|frame| frame.id)
                .or_else(|| self.frames.top_frame_on_terminal(terminal_id)),
        };
        if let Some(frame_id) = frame_id {
            self.observe_keyboard_input_frame(frame_id);
        }
    }

    /// Route one keyboard event to its terminal-local kboard and record the
    /// frame GNU's keyboard buffer attached to that event.  A character from a
    /// different frame must yield `switch-frame' first; otherwise a secondary
    /// TTY can execute its keystroke in whichever GUI/TTY frame was globally
    /// selected most recently.
    #[doc(hidden)]
    pub fn observe_keyboard_input_frame(&mut self, frame_id: neovm_value::display_vocab::FrameId) {
        if let Some(frame) = self.frames.get(frame_id) {
            self.command_loop
                .keyboard
                .select_terminal(frame.terminal_id);
        }

        let selected_frame = self.frames.selected_frame().map(|frame| frame.id);
        let last_event_frame = self
            .command_loop
            .keyboard
            .kboard
            .internal_last_event_frame();
        let switching = Some(frame_id) != last_event_frame && Some(frame_id) != selected_frame;
        self.command_loop
            .keyboard
            .kboard
            .set_internal_last_event_frame(frame_id);

        // GNU's `Qlast_event_frame` / `Qswitch_frame`; every keyboard event
        // observed updates the former and previously re-interned it.
        static LAST_EVENT_FRAME: WellKnown = WellKnown::LastEventFrame;
        static SWITCH_FRAME: WellKnown = WellKnown::SwitchFrame;
        let frame_value = Value::make_frame(frame_id.0);
        self.store_variable(LAST_EVENT_FRAME.id(), frame_value);
        if switching || self.command_loop.keyboard.has_unread_selection_event() {
            self.command_loop
                .keyboard
                .set_unread_selection_event(Value::list(vec![
                    Value::symbol(&SWITCH_FRAME),
                    frame_value,
                ]));
        }
    }
}
