//! GC pacing and safe points: when the evaluator collects, how it seeds roots, and the concurrent-mark handshakes it drives (GNU `maybe_gc` / `garbage_collect` shape).
//!
//! Moved out of `eval/mod.rs` unchanged; a child module of `eval` so it keeps
//! the same view of `Context` and the parent's private items (`use super::*`).

use super::*;

impl Context {
    /// Enumerate every live `Value` reference in the evaluator and all
    /// sub-managers without materializing a single temporary root vector.
    /// Enumerate every evaluator/context root into `visit`, announcing each
    /// root GROUP boundary via `group(name)` immediately before that group's
    /// values are visited. The group seam is diagnostics-only: the GC
    /// handshake instrumentation brackets per-group timings around the
    /// boundaries; enumeration order and content are unchanged.
    pub(super) fn trace_roots(
        &self,
        group: &mut dyn FnMut(&'static str),
        visit: &mut dyn FnMut(Value),
    ) {
        group("vm_frames");
        for frame in &self.vm_root_frames {
            for root in frame.roots.iter().copied() {
                visit(root);
            }
        }
        group("eval_temp");
        for root in self.eval_temp_roots.iter().copied() {
            visit(root);
        }
        group("treesit");
        for root in self.treesit.roots() {
            visit(root);
        }
        group("bc");
        for root in self.bc_buf.iter().copied() {
            visit(root);
        }
        group("jit_window");
        for root in self.jit_root_stack[..self.jit_root_stack_top]
            .iter()
            .copied()
        {
            visit(root);
        }
        for frame in &self.bc_frames {
            if frame.fun.is_heap_object() {
                visit(frame.fun);
            }
        }
        group("handlers");
        for frame in self.condition_stack.iter().chain(
            self.suspended_thread_bindings
                .iter()
                .flat_map(|state| state.condition_stack.iter()),
        ) {
            match frame {
                ConditionFrame::Catch { tag, .. } => visit(*tag),
                ConditionFrame::ConditionCase { conditions, .. } => visit(*conditions),
                ConditionFrame::HandlerBind {
                    conditions,
                    handler,
                    ..
                } => {
                    visit(*conditions);
                    visit(*handler);
                }
                ConditionFrame::SkipConditions { .. } => {}
            }
        }
        group("specpdl");
        for state in &self.suspended_thread_bindings {
            visit(state.lexenv);
        }
        for entry in self.specpdl.iter().chain(
            self.suspended_thread_bindings
                .iter()
                .flat_map(|state| state.specpdl.iter()),
        ) {
            match entry {
                SpecBinding::Let { old_value, .. } => {
                    if let Some(value) = old_value.get() {
                        visit(value);
                    }
                }
                SpecBinding::LetLocal { old_value, .. } => visit(*old_value),
                SpecBinding::LetDefault { old_value, .. } => {
                    if let Some(value) = old_value.get() {
                        visit(value);
                    }
                }
                SpecBinding::LexicalEnv { old_lexenv } => visit(*old_lexenv),
                SpecBinding::GcRoot { value } => visit(*value),
                SpecBinding::Backtrace { function, args, .. } => {
                    visit(*function);
                    self.trace_backtrace_args(args, visit);
                }
                SpecBinding::Backtrace1 { function, arg, .. } => {
                    visit(*function);
                    visit(*arg);
                }
                SpecBinding::Backtrace2 {
                    function,
                    arg0,
                    arg1,
                } => {
                    visit(*function);
                    visit(*arg0);
                    visit(*arg1);
                }
                SpecBinding::BacktraceNative {
                    function,
                    args_ptr,
                    nargs,
                } => {
                    visit(*function);
                    // SAFETY: the variant's contract — the caller's
                    // call-args slot stays alive (and unmutated) while
                    // this entry exists, and root seeding runs with the
                    // mutator stopped.
                    for i in 0..*nargs as usize {
                        visit(Value::from_bits(unsafe { *args_ptr.add(i) } as usize));
                    }
                }
                SpecBinding::UnwindProtect { forms, lexenv } => {
                    visit(*forms);
                    visit(*lexenv);
                }
                SpecBinding::SaveRestriction { state } => {
                    let mut roots = Vec::new();
                    state.state().trace_roots(&mut roots);
                    // The saved bounds live as marker ids only; root the
                    // marker objects so restore still finds them (see
                    // SavedRestrictionState::trace_marker_roots).
                    state.state().trace_marker_roots(&self.buffers, &mut roots);
                    for root in roots {
                        visit(root);
                    }
                }
                SpecBinding::SaveExcursion { marker, .. } => visit(*marker),
                SpecBinding::NativeUnwind { action } => action.trace_roots(visit),
                // EXHAUSTIVE ON PURPOSE — no catch-all arm. These four carry
                // no Lisp value (a buffer id, two lengths, nothing), and a new
                // `SpecBinding` variant must state which group it belongs to
                // instead of being absorbed by a `_ => {}`. A root walk is the
                // one match where "the compiler did not complain" and "the
                // value is marked" must be the same sentence
                // (DIVERGENCES.md 161's residual, closed by 162).
                SpecBinding::SaveCurrentBuffer { .. }
                | SpecBinding::LoadsInProgress { .. }
                | SpecBinding::RequireStack { .. }
                | SpecBinding::Nop => {}
            }
        }
        group("profiler");
        self.trace_profiler_roots(visit);
        group("misc");
        visit(self.lexenv);
        visit(self.quit_flag);
        visit(self.inhibit_quit);
        if self.cached_system_name.is_heap_object() {
            visit(self.cached_system_name);
        }
        if let Some(filter_fn) = self.interpreted_closure_filter_fn {
            visit(filter_fn);
        }
        for entry in self.named_call_cache.values() {
            if let NamedCallTarget::Obarray(val) = &entry.target {
                visit(*val);
            }
        }
        for funcall in &self.pending_safe_funcalls {
            visit(funcall.function);
            for arg in funcall.args.iter().copied() {
                visit(arg);
            }
        }
        for hook in &self.last_overlay_modification_hooks {
            visit(hook.hook_list);
            visit(hook.overlay);
        }
        if !self.interval_insert_behind_hooks.is_nil() {
            visit(self.interval_insert_behind_hooks);
        }
        if !self.interval_insert_in_front_hooks.is_nil() {
            visit(self.interval_insert_in_front_hooks);
        }
        if !self.current_local_map.is_nil() {
            visit(self.current_local_map);
        }
        let selected_global_map = self.selected_global_map.value();
        if !selected_global_map.is_nil() {
            visit(selected_global_map);
        }
        if self.standard_syntax_table.is_heap_object() {
            visit(self.standard_syntax_table);
        }
        if self.syntax_code_objects.is_heap_object() {
            visit(self.syntax_code_objects);
        }
        if self.standard_category_table.is_heap_object() {
            visit(self.standard_category_table);
        }
        // Full ~all-interned-symbols walk on STW collections; only the
        // BLV-pool residual under `ObarraySymbolCellSkipGuard` (both
        // concurrent handshakes).
        group("obarray");
        self.obarray.trace_roots_with(visit);
        group("proc_timer");
        self.processes.trace_roots_with(visit);
        self.watchers.trace_roots_with(visit);
        group("reg_custom");
        self.registers.trace_roots_with(visit);
        self.custom.trace_roots_with(visit);
        self.autoloads.trace_roots_with(visit);
        self.interactive.trace_roots_with(visit);
        group("buffers");
        self.buffers.trace_roots_with(visit);
        group("ui_misc");
        self.xwidgets.trace_roots_with(visit);
        self.face_table.trace_roots_with(visit);
        self.threads.trace_roots_with(visit);
        self.kmacro.trace_roots_with(visit);
        crate::gc_trace::GcTrace::trace_roots_with(&self.command_loop, visit);
        self.modes.trace_roots_with(visit);
        self.frames.trace_roots_with(visit);
        self.coding_systems.trace_roots_with(visit);
        group("match_data");
        if let Some(ref md) = self.match_data
            && let Some(crate::emacs_core::regex::SearchedString::Heap(val)) = md.searched_string()
        {
            visit(*val);
        }
    }

    /// Get the current GC threshold.
    pub fn gc_threshold(&self) -> usize {
        self.tagged_heap.gc_threshold()
    }

    /// Whether `sym_id` is one of the GC-setting variables (compared against
    /// the live-resolved ids; `false` until the first settings refresh has
    /// resolved them — the GC-end/decision-point refresh covers that window).
    pub(super) fn is_gc_runtime_setting_symbol(&self, sym_id: SymId) -> bool {
        self.gc_runtime_settings_cache
            .syms
            .is_some_and(|syms| syms.contains(sym_id))
    }

    pub(crate) fn refresh_gc_runtime_settings_after_change_by_id(&mut self, sym_id: SymId) {
        if self.is_gc_runtime_setting_symbol(sym_id) {
            self.refresh_gc_runtime_settings_cache();
            self.sync_gc_threshold_from_runtime_settings();
        }
    }

    pub(super) fn refresh_gc_runtime_settings_cache(&mut self) {
        // Re-resolve the variable names against the LIVE interner every time
        // (four hash lookups on a rare path): see `GcRuntimeSettingsCache::syms`.
        let syms = GcSettingSyms::resolve();
        self.gc_runtime_settings_cache.syms = Some(syms);
        self.gc_runtime_settings_cache.gc_cons_threshold_bytes = self
            .obarray
            .symbol_value_id(syms.threshold)
            .copied()
            .and_then(|value| value.as_fixnum())
            .and_then(|n| usize::try_from(n).ok())
            .unwrap_or(GC_DEFAULT_THRESHOLD_BYTES);
        self.gc_runtime_settings_cache.gc_cons_percentage_scaled = self
            .obarray
            .symbol_value_id_or_nil(syms.percentage)
            .as_number_f64()
            .filter(|float| float.is_finite() && *float > 0.0)
            .map(|float| ((float * GC_PERCENT_SCALE as f64).ceil() as u64).clamp(1, u64::MAX));
        self.gc_runtime_settings_cache.memory_full = !self
            .obarray
            .symbol_value_id_or_nil(syms.memory_full)
            .is_nil();
    }

    pub(super) fn effective_gc_threshold_bytes(&mut self) -> usize {
        if self.gc_runtime_settings_cache.memory_full {
            return self.tagged_heap.gc_threshold();
        }

        let mut threshold = self
            .gc_runtime_settings_cache
            .gc_cons_threshold_bytes
            .max(GC_THRESHOLD_FLOOR_BYTES);
        if let Some(percentage_scaled) = self.gc_runtime_settings_cache.gc_cons_percentage_scaled {
            let live_estimate = self
                .tagged_heap
                .live_bytes()
                .saturating_add(self.tagged_heap.bytes_since_gc() / 2);
            let pct_threshold = ((live_estimate as u128)
                .saturating_mul(percentage_scaled as u128)
                .saturating_add((GC_PERCENT_SCALE - 1) as u128)
                / GC_PERCENT_SCALE as u128)
                .min(GC_HI_THRESHOLD_BYTES as u128) as usize;
            threshold = threshold.max(pct_threshold);
        }
        // Internal live-proportional growth term: trigger only once at least
        // GC_LIVE_GROWTH_NUM/GC_LIVE_GROWTH_DEN of the live heap has been
        // allocated since the last cycle, so the full-mark cost (O(live))
        // amortizes as the heap grows. Invariants: strict max — the
        // elisp-derived value above stays a floor this term never lowers (user
        // settings and the defaults keep their meaning as minimum budgets);
        // overridden thresholds (`set_gc_threshold`) are unaffected because
        // this value only flows through `set_gc_threshold_from_runtime`; the
        // GC_HI clamp below still bounds the result. `live_bytes` only grows
        // between sweeps (it is recomputed exactly at each sweep), which is
        // safe for a max term.
        let live_growth = ((self.tagged_heap.live_bytes() as u128)
            .saturating_mul(GC_LIVE_GROWTH_NUM)
            / GC_LIVE_GROWTH_DEN)
            .min(GC_HI_THRESHOLD_BYTES as u128) as usize;
        threshold = threshold.max(live_growth);
        let mut threshold = threshold.clamp(1, GC_HI_THRESHOLD_BYTES);
        if self.gc_runtime_settings_cache.syms.is_some_and(|syms| {
            !self
                .obarray
                .symbol_value_id_or_nil(syms.startup_ceiling)
                .is_nil()
        }) {
            threshold = threshold.min(GC_STARTUP_THRESHOLD_CEILING_BYTES);
        }
        gc_threshold_cap_from_env().map_or(threshold, |cap| threshold.min(cap))
    }

    pub(super) fn sync_gc_threshold_from_runtime_settings(&mut self) {
        // Read the Lisp variables LIVE here, like GNU's `garbage_collect` end
        // (`consing_until_gc = consing_threshold (gc_cons_threshold,
        // Vgc_cons_percentage, 0)`), instead of trusting a cache that only the
        // setter paths routed through `refresh_gc_runtime_settings_after_
        // change_by_id` keep current: any write that bypasses them (a direct
        // forwarder store, a future setter) is then honored at the next GC,
        // which is exactly GNU's contract for a changed threshold.
        self.refresh_gc_runtime_settings_cache();
        let threshold = self.effective_gc_threshold_bytes();
        if self.tagged_heap.gc_threshold() != threshold {
            self.tagged_heap.set_gc_threshold_from_runtime(threshold);
        }
    }

    pub(super) fn update_gc_runtime_stats(&mut self, elapsed: std::time::Duration) {
        self.obarray
            .set_symbol_value_id(gcs_done_symbol(), Value::fixnum(self.gc_count as i64));

        let old_elapsed = self
            .obarray
            .symbol_value_id(gc_elapsed_symbol())
            .copied()
            .and_then(|value| value.as_number_f64())
            .unwrap_or(0.0);
        self.obarray.set_symbol_value_id(
            gc_elapsed_symbol(),
            Value::make_float(old_elapsed + elapsed.as_secs_f64()),
        );

        // Publish a cross-thread snapshot for the diagnostics server. Sampled
        // here, once per GC cycle, so the diagnostics thread never touches the
        // heap; values between collections are the last post-sweep reading.
        let counts = self.tagged_heap.memory_use_counts_snapshot();
        crate::emacs_core::gc_stats::publish(crate::emacs_core::gc_stats::GcStatsSnapshot {
            collections: self.gc_count,
            live_bytes: self.tagged_heap.live_bytes() as u64,
            total_allocated_bytes: self.tagged_heap.total_allocated_bytes(),
            cons_cells: counts[0],
            vector_cells: counts[2],
            strings: counts[6],
        });
    }

    /// Set the GC threshold. Use usize::MAX to effectively disable GC.
    pub fn set_gc_threshold(&mut self, threshold: usize) {
        self.tagged_heap.set_gc_threshold(threshold);
    }

    /// Set the maximum eval recursion depth.
    pub fn set_max_depth(&mut self, depth: usize) {
        self.max_depth = depth;
    }

    /// Set the thread-local heap pointers for the current thread.
    ///
    /// Must be called when using an Context from a thread other than the one
    /// that created it (e.g., in worker thread pools).
    pub fn setup_thread_locals(&mut self) {
        crate::tagged::gc::set_tagged_heap(&mut self.tagged_heap);
        self.sync_charset_runtime_resources();
        super::super::syntax::restore_standard_syntax_table_object(self.standard_syntax_table);
        super::super::syntax::restore_syntax_code_objects(self.syntax_code_objects);
        super::super::category::restore_standard_category_table_object(
            self.standard_category_table,
        );
        super::super::casetab::restore_standard_case_table_object(&self.obarray);
        // Install this Context's quit-request flag so leaf functions
        // (regex matcher, other long-running scans) can poll it
        // without `&mut Context` access.
        QUIT_REQUESTED_TLS.with(|cell| {
            *cell.borrow_mut() = Some(std::sync::Arc::clone(&self.quit_requested));
        });
    }

    pub(super) fn finish_runtime_activation(&mut self, sync_keyboard: bool) {
        self.setup_thread_locals();
        self.refresh_gc_runtime_settings_cache();
        self.sync_gc_threshold_from_runtime_settings();
        if sync_keyboard {
            self.sync_keyboard_runtime_from_obarray();
        }
        self.sync_thread_runtime_bindings();
        self.sync_current_thread_buffer_state();
        // Every name GNU's C declares with `DEFVAR_LISP' or `DEFVAR_KBOARD'
        // gets GNU's redirect tag here, at the last point before the evaluator
        // is live: the same boundary GNU's `main' crosses when the last
        // `syms_of_*'/`init_*' returns, reached from the other side.  GNU
        // declares first and assigns after; this port assigns from several
        // hundred scattered sites -- including `runtime_identity::install' and
        // `sync_thread_runtime_bindings' just above, which is why this cannot
        // sit with the `register_bootstrap_vars' calls -- and declares once,
        // here.  Idempotent, so the pdump-restored path (whose image already
        // carries the descriptors) finds every row settled and the six names
        // an image cannot carry get theirs.  See `defvar_object' for what the
        // tag buys and why the store rule -- the thing `DEFVAR_BOOL' and
        // `DEFVAR_INT' are declared for -- is not it.
        super::super::defvar_object::adopt(&mut self.obarray);
    }

    pub(crate) fn sync_current_thread_buffer_state(&mut self) {
        let current_thread_id = self.threads.current_thread_id();
        let current_buffer_id = self.buffers.current_buffer_id();
        self.threads
            .set_thread_current_buffer(current_thread_id, current_buffer_id);
    }

    pub(super) fn sync_current_buffer_runtime_state(&mut self) -> Result<(), Flow> {
        self.sync_current_thread_buffer_state();
        super::super::casetab::sync_current_buffer_case_table_state(self)?;
        super::super::syntax::sync_current_buffer_syntax_table_state(self)?;
        Ok(())
    }

    pub(crate) fn switch_current_buffer(
        &mut self,
        id: crate::buffer::BufferId,
    ) -> Result<(), Flow> {
        if !self.buffers.switch_current(id) {
            return Err(signal(
                "error",
                vec![Value::string("Selecting deleted buffer")],
            ));
        }
        self.sync_current_buffer_runtime_state()
    }

    pub(crate) fn set_current_buffer_unrecorded(
        &mut self,
        id: crate::buffer::BufferId,
    ) -> Result<(), Flow> {
        if !self.buffers.switch_current_unrecorded(id) {
            return Err(signal(
                "error",
                vec![Value::string("Selecting deleted buffer")],
            ));
        }
        self.sync_current_buffer_runtime_state()
    }

    /// GNU `set_buffer_if_live` for an unwind: re-select `id` unless it was
    /// killed meanwhile.
    ///
    /// Mirrors `set_buffer_internal_1`'s first line, `if (current_buffer ==
    /// b) return;`: the overwhelmingly common unwind (every
    /// `save-current-buffer` body that never switched, every
    /// `put-text-property` on the current buffer) restores the buffer that
    /// is still current.  Its runtime state was synced when it became
    /// current, and the sync below is a thread-slot write plus two
    /// seed-if-nil table reads, so redoing it bought nothing — at ~1,500
    /// instructions a call it was 7% of a whole-buffer Org fontify (44,265
    /// restores per five fontifies, all of them same-buffer).
    pub fn restore_current_buffer_if_live(&mut self, id: crate::buffer::BufferId) {
        if self.buffers.current_buffer_id() == Some(id) {
            return;
        }
        if self.buffers.get(id).is_none() {
            return;
        }
        let _ = self.buffers.switch_current_unrecorded(id);
        let _ = self.sync_current_buffer_runtime_state();
    }

    /// Connect the input system for interactive mode.
    ///
    /// This mirrors GNU Emacs's `init_keyboard()` — it connects the evaluator
    /// to the render thread's input channel so that `read_char()` can block
    /// waiting for user input instead of returning immediately (batch mode).
    ///
    /// # Arguments
    /// * `input_rx` — Receiver end of the crossbeam channel from the render thread
    pub fn init_input_system(
        &mut self,
        input_rx: crossbeam_channel::Receiver<crate::keyboard::InputEvent>,
    ) {
        self.input_rx = Some(input_rx);
        self.command_loop.running = true;
    }

    /// Replace the blocking primitive used while waiting for host input.
    ///
    /// The backend is only a suspension boundary. It must arrange for input
    /// to be sent through the receiver installed by [`Self::init_input_system`];
    /// the evaluator never accepts untyped input directly from this callback.
    pub fn install_host_input_wait_backend(
        &mut self,
        backend: impl crate::emacs_core::wait::HostInputWaitBackend + 'static,
    ) {
        self.host_input_wait_backend = Some(Box::new(backend));
    }

    /// Mount product-owned runtime resources for ordinary Lisp loading.
    ///
    /// This mount is read-only and scoped to this evaluator. Browser and
    /// sandboxed application adapters use it for packaged editor libraries;
    /// user files remain governed by the host's separate storage capability.
    pub fn install_runtime_resource_store(
        &mut self,
        store: Box<dyn crate::emacs_core::fileio::RuntimeResourceStore>,
    ) {
        let store = std::rc::Rc::<dyn crate::emacs_core::fileio::RuntimeResourceStore>::from(store);
        crate::emacs_core::charset::install_runtime_resource_store(Some(std::rc::Rc::clone(
            &store,
        )));
        self.editor_file_system.install_runtime_resources(store);
    }

    pub(crate) fn sync_charset_runtime_resources(&self) {
        crate::emacs_core::charset::install_runtime_resource_store(
            self.editor_file_system.runtime_resources(),
        );
    }

    /// Replace the target-default mutable filesystem before editor startup.
    pub fn install_editor_file_system(
        &mut self,
        filesystem: Box<dyn crate::emacs_core::fileio::EditorFileSystem>,
    ) {
        self.editor_file_system.replace_host(filesystem);
    }

    pub(crate) fn editor_file_system(&self) -> &dyn crate::emacs_core::fileio::EditorFileSystem {
        &self.editor_file_system
    }

    pub(crate) fn has_host_input_wait_backend(&self) -> bool {
        self.host_input_wait_backend.is_some()
    }

    /// Install the receiver for cross-thread [`EvalThreadTask`]s (e.g. from the
    /// diagnostics server). The sender side wakes the Lisp thread via
    /// [`Context::wait_notifier`]; queued tasks run at the next safe point.
    pub fn init_eval_task_system(&mut self, rx: crossbeam_channel::Receiver<EvalThreadTask>) {
        self.eval_task_rx = Some(rx);
    }

    /// Run any queued cross-thread tasks synchronously. Called at a Lisp-safe
    /// point (the `read_char` loop); a no-op when no channel is installed.
    pub(crate) fn drain_eval_tasks(&mut self) {
        // Clone the Receiver handle so we don't borrow `self.eval_task_rx`
        // across the `&mut self` task call.
        if let Some(rx) = self.eval_task_rx.clone() {
            while let Ok(task) = rx.try_recv() {
                task(self);
            }
        }
    }

    /// Cross-platform handle producers use to wake the wait loop after
    /// publishing work (see [`WaitNotifier`]). Returns `None` only if the
    /// platform poller could not be created. Frontend input, diagnostics, and
    /// asynchronous process work share this mechanism.
    pub fn wait_notifier(&self) -> Option<crate::emacs_core::process::WaitNotifier> {
        self.processes.wait_notifier()
    }

    pub fn set_display_host(&mut self, mut host: Box<dyn DisplayHost>) {
        let _ = host.set_visual_config(self.visual_config.clone());
        self.display_host = Some(host);
    }

    pub fn set_tty_frame_host_factory(&mut self, factory: Box<dyn TtyFrameHostFactory>) {
        self.tty_frame_host_factory = Some(factory);
    }
}
