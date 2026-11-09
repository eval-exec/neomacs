//! The command loop, mirroring GNU keyboard.c: recursive edit, command execution, pre/post-command hooks, and quit handling.
//!
//! Moved out of `eval/mod.rs` unchanged; a child module of `eval` so it keeps
//! the same view of `Context` and the parent's private items (`use super::*`).

use super::*;
use neovm_value::tagged::gc::MarkPhase;

/// Labels for the structured GC diagnostics; text compatibility stays in the
/// event message for existing pause-analysis tools.
#[derive(Clone, Copy)]
enum GcTraceKind {
    SynchronousRecoveryPending,
    ConcurrentStart,
    ConcurrentTermination,
}

impl GcTraceKind {
    #[doc(hidden)]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SynchronousRecoveryPending => "synchronous_recovery_pending",
            Self::ConcurrentStart => "concurrent_start",
            Self::ConcurrentTermination => "concurrent_termination",
        }
    }
}

/// Fixed command-loop identities: GNU DEFSYM statics with compile-time ids,
/// no lock and no epoch. A caller may use `id` again after arbitrary Lisp;
/// unintern or a dump restore cannot change the identity (GNU semantics).
#[derive(Clone, Copy, Debug, strum::EnumIter)]
#[doc(hidden)]
pub enum CommandLoopSymbol {
    PrefixArg,
    LastPrefixArg,
    CurrentPrefixArg,
    ThisCommand,
    RealThisCommand,
    ThisOriginalCommand,
    LastCommandEvent,
    DeactivateMark,
    DisablePointAdjustment,
    GlobalDisablePointAdjustment,
    LastCommand,
    RealLastCommand,
    LastRepeatableCommand,
    InhibitQuit,
    UndoAutoAddBoundary,
    MemoryFull,
    DelayedWarningsList,
    AutoSaveInterval,
    Undefined,
    CommandExecute,
    InhibitRedisplay,
    PreRedisplayFunction,
    RedisplaySkipInitialFrame,
    ResizeMiniFrames,
}

impl CommandLoopSymbol {
    #[inline]
    #[doc(hidden)]
    pub fn id(self) -> SymId {
        use neovm_value::intern::WellKnown;

        match self {
            Self::PrefixArg => {
                static SYMBOL: WellKnown = WellKnown::PrefixArg;
                SYMBOL.id()
            }
            Self::LastPrefixArg => {
                static SYMBOL: WellKnown = WellKnown::LastPrefixArg;
                SYMBOL.id()
            }
            Self::CurrentPrefixArg => {
                static SYMBOL: WellKnown = WellKnown::CurrentPrefixArg;
                SYMBOL.id()
            }
            Self::ThisCommand => {
                static SYMBOL: WellKnown = WellKnown::ThisCommand;
                SYMBOL.id()
            }
            Self::RealThisCommand => {
                static SYMBOL: WellKnown = WellKnown::RealThisCommand;
                SYMBOL.id()
            }
            Self::ThisOriginalCommand => {
                static SYMBOL: WellKnown = WellKnown::ThisOriginalCommand;
                SYMBOL.id()
            }
            Self::LastCommandEvent => {
                static SYMBOL: WellKnown = WellKnown::LastCommandEvent;
                SYMBOL.id()
            }
            Self::DeactivateMark => {
                static SYMBOL: WellKnown = WellKnown::DeactivateMark;
                SYMBOL.id()
            }
            Self::DisablePointAdjustment => {
                static SYMBOL: WellKnown = WellKnown::DisablePointAdjustment;
                SYMBOL.id()
            }
            Self::GlobalDisablePointAdjustment => {
                static SYMBOL: WellKnown = WellKnown::GlobalDisablePointAdjustment;
                SYMBOL.id()
            }
            Self::LastCommand => {
                static SYMBOL: WellKnown = WellKnown::LastCommand;
                SYMBOL.id()
            }
            Self::RealLastCommand => {
                static SYMBOL: WellKnown = WellKnown::RealLastCommand;
                SYMBOL.id()
            }
            Self::LastRepeatableCommand => {
                static SYMBOL: WellKnown = WellKnown::LastRepeatableCommand;
                SYMBOL.id()
            }
            Self::InhibitQuit => {
                static SYMBOL: WellKnown = WellKnown::InhibitQuit;
                SYMBOL.id()
            }
            Self::UndoAutoAddBoundary => {
                static SYMBOL: WellKnown = WellKnown::UndoAutoAddBoundary;
                SYMBOL.id()
            }
            Self::MemoryFull => {
                static SYMBOL: WellKnown = WellKnown::MemoryFull;
                SYMBOL.id()
            }
            Self::DelayedWarningsList => {
                static SYMBOL: WellKnown = WellKnown::DelayedWarningsList;
                SYMBOL.id()
            }
            Self::AutoSaveInterval => {
                static SYMBOL: WellKnown = WellKnown::AutoSaveInterval;
                SYMBOL.id()
            }
            Self::Undefined => {
                static SYMBOL: WellKnown = WellKnown::Undefined;
                SYMBOL.id()
            }
            Self::CommandExecute => {
                static SYMBOL: WellKnown = WellKnown::CommandExecute;
                SYMBOL.id()
            }
            Self::InhibitRedisplay => {
                static SYMBOL: WellKnown = WellKnown::InhibitRedisplay;
                SYMBOL.id()
            }
            Self::PreRedisplayFunction => {
                static SYMBOL: WellKnown = WellKnown::PreRedisplayFunction;
                SYMBOL.id()
            }
            Self::RedisplaySkipInitialFrame => {
                static SYMBOL: WellKnown = WellKnown::RedisplaySkipInitialFrame;
                SYMBOL.id()
            }
            Self::ResizeMiniFrames => {
                static SYMBOL: WellKnown = WellKnown::ResizeMiniFrames;
                SYMBOL.id()
            }
        }
    }
}

/// Fixed tags and variables the command loop reads on every command. GNU holds
// them as staticpro'd or DEFVAR_LISP statics (`Qtop_level`, `Qexit`,
// `Qidentity`, `Qonly`, `Vtransient_mark_mode`, `Vinput_pending_p_filter_events`).
static INPUT_PENDING_FILTER_EVENTS: WellKnown = WellKnown::InputPendingPFilterEvents;
static DEBUG_ON_EVENT: WellKnown = WellKnown::DebugOnEvent;
static DO_AUTO_SAVE: WellKnown = WellKnown::DoAutoSave;

// Hooks the command loop runs itself, pre-resolved to fixed DEFSYM ids.

#[path = "gc_generational.rs"]
mod gc_generational;

/// The collection policy is selected before the admitted start mutates the heap.
#[derive(Clone, Copy, Debug)]
enum ConcurrentCycleKind {
    Threshold,
    FirstPartition,
}

static_assertions::assert_impl_all!(ConcurrentCycleKind: Send, Sync, Copy, Clone, std::fmt::Debug);

impl Context {
    /// Mark this evaluator as a daemon before running normal-top-level.
    /// The name is host-owned; Lisp cannot change daemonp by setting variables.
    pub fn configure_daemon(&mut self, name: Option<String>, notify: Option<DaemonNotifier>) {
        *self.daemon = Some(DaemonState {
            name,
            initialized: false,
            notify,
        });
    }

    /// Close owned network listeners and terminate children after kill-emacs
    /// hooks, without running Lisp sentinels after the shutdown boundary.
    pub fn close_processes_for_exit(&mut self) {
        for id in self.processes.list_processes() {
            crate::emacs_core::eval::process_hooks::unlink_owned_unix_listener_for_exit(self, id);
            crate::emacs_core::eval::process_hooks::delete_process(self, id);
        }
    }

    /// Run `kill-emacs-hook`, then enter Dying and publish `request`.
    ///
    /// A call that arrives while that hook is already running re-enters,
    /// as GNU `Fkill_emacs` does (`emacs.c:2954`, no recursion guard). The
    /// hook list runs again through the ordinary walker. Lisp call depth
    /// stops the recursion at `max-lisp-eval-depth`. The nested shutdown
    /// error propagates, so this frame does not publish over the inner
    /// request. A call that arrives while Dying skips the hook and publishes
    /// only when nothing is stored.
    #[doc(hidden)]
    pub fn shutdown_with_hooks(&mut self, request: ShutdownRequest) -> EvalResult {
        if let Some(request) = self.shutdown_request {
            return Err(Flow::shutdown(request));
        }
        match self.shutdown_phase {
            ShutdownPhase::Dying => self.shutdown_after_hooks_are_dead(request),
            ShutdownPhase::Running => {
                self.shutdown_phase = ShutdownPhase::InProgress;
                self.finish_kill_emacs_hook(request)
            }
            ShutdownPhase::InProgress => self.finish_kill_emacs_hook(request),
        }
    }

    /// Run the hook, then publish `request` unless a nested kill-emacs already
    /// published and returned [`Flow::shutdown`].
    fn finish_kill_emacs_hook(&mut self, request: ShutdownRequest) -> EvalResult {
        if let Err(flow) = self.run_hook_if_bound("kill-emacs-hook")
            && flow.is_shutdown()
        {
            return Err(flow);
        }
        // `shut_down_emacs` (`emacs.c:3107-3110`) nils `Vrun_hooks`
        // and sets `Vinhibit_redisplay`. Neither assignment is specbound.
        self.enter_shut_down();
        self.shutdown_after_hooks_are_dead(request)
    }

    /// GNU `shut_down_emacs`'s first two assignments (`emacs.c:3107-3110`).
    fn enter_shut_down(&mut self) {
        self.shutdown_phase = ShutdownPhase::Dying;
        self.assign_by_id(CommandLoopSymbol::InhibitRedisplay.id(), Value::T);
    }

    /// Publish the shutdown request once hook bodies are no longer eligible.
    ///
    /// A later entry while `Dying` reaches here without running hook bodies
    /// (`eval.c:3006-3007`) and publishes `request` when nothing is published
    /// yet. A request that is already published stays: a nested entry must
    /// not replace it.
    fn shutdown_after_hooks_are_dead(&mut self, request: ShutdownRequest) -> EvalResult {
        if let Some(published) = self.shutdown_request {
            return Err(Flow::shutdown(published));
        }
        // GNU calls `Fdo_auto_save (Qt, Qnil)` directly after disabling
        // hooks (`emacs.c:3107-3110`, `:3157`). Resolve the canonical subr,
        // independently of a redefined or unbound Lisp function cell.
        // A handler's nonlocal exit leaves before publishing this request,
        // including a throw or a nested shutdown with its own exit code.
        crate::emacs_core::builtins::dispatch_builtin_by_id(
            self,
            DO_AUTO_SAVE.id(),
            vec![Value::T, Value::NIL],
        )
        .ok_or_else(|| {
            signal(
                LispCondition::VoidFunction,
                vec![Value::symbol(&DO_AUTO_SAVE)],
            )
        })??;
        self.log_cconv_memo_report();
        self.log_tier_i_report();
        self.shutdown_request = Some(request);
        self.command_loop.running = false;
        Err(Flow::shutdown(request))
    }

    pub fn shutdown_request(&self) -> Option<ShutdownRequest> {
        self.shutdown_request
    }

    /// GNU `Fkill_emacs`'s `attributes: noreturn`, asked as a question.
    ///
    /// The recorded [`ShutdownRequest`] is the authority, not the propagating
    /// `Flow::Shutdown`: `module_handle_nonlocal_exit` (`dynamic_module.rs`)
    /// hands a module a signal named `kill-emacs`, and a module that clears it
    /// still exits, because the request is what the evaluator acts on.  See
    /// [`LispExecution`].
    #[doc(hidden)]
    pub fn lisp_execution(&self) -> LispExecution {
        match self.shutdown_request {
            None => LispExecution::Live,
            Some(_) => LispExecution::ExitedAlready,
        }
    }

    #[doc(hidden)]
    pub fn command_loop_noninteractive(&self) -> bool {
        self.noninteractive
    }

    /// Run GNU's command-input auto-save boundary for either the event-count
    /// or idle-time trigger. Both paths must pass `auto-save-no-message`, call
    /// the same `do-auto-save` primitive, and throttle a failing attempt so a
    /// broken hook cannot spin the command loop.
    #[doc(hidden)]
    pub fn run_command_loop_auto_save(&mut self, trigger: &'static str) {
        self.command_loop.last_auto_save_input_events = self.num_nonmacro_input_events();
        let no_message = if self
            .eval_symbol_by_id(neovm_value::intern::WellKnown::AutoSaveNoMessage.id())
            .unwrap_or(Value::NIL)
            .is_truthy()
        {
            Value::T
        } else {
            Value::NIL
        };
        if let Err(flow) = self.apply(Value::symbol(&DO_AUTO_SAVE), vec![no_message, Value::NIL]) {
            let rendered = crate::emacs_core::print::format_flow_with_eval(self, &flow);
            tracing::warn!("auto-save from {trigger} failed: {rendered}");
        }
    }

    /// Run a hook with `safe-run-hooks` semantics: each hook
    /// function is wrapped in a `condition-case` so a broken
    /// function is removed from the hook instead of re-firing on
    /// every subsequent command. Mirrors GNU
    /// `safe_run_hooks (Qhook_name)` at
    /// `src/keyboard.c:1361,1485` and `src/eval.c:2779-2830`.
    /// Keyboard audit Finding 7.
    #[doc(hidden)]
    pub fn safe_run_hook_if_bound(&mut self, hook_sym: SymId) -> EvalResult {
        // GNU `keyboard.c:1970-1978` (`safe_run_hooks`):
        //
        //   void safe_run_hooks (Lisp_Object hook) {
        //     specbind (Qinhibit_quit, Qt);
        //     run_hook_with_args (2, {hook, hook}, safe_run_hook_funcall);
        //     unbind_to (count, Qnil);
        //   }
        //
        // This is a C function — NOT the Lisp `safe-run-hooks` from
        // `subr.el`. It calls `run_hook_with_args` with a custom
        // funcall wrapper (`safe_run_hook_funcall`) that wraps each
        // hook function in `internal_condition_case_n` and removes
        // broken entries on error.
        //
        // neomacs mirrors this by calling
        // `hook_runtime::safe_run_named_hook` directly from Rust,
        // which resolves the hook value (including buffer-local
        // bindings + the `t` global marker), calls each hook
        // function, and swallows Signal errors. This never goes
        // through Lisp — matching GNU's keyboard.c which calls the
        // C function, not the Lisp wrapper.
        // `safe_run_hook_funcall` only swallows ordinary `error`
        // signals.  Nonlocal exits like `throw`/`quit` still escape
        // the command loop, and `read-char-from-minibuffer` relies on
        // that when its local `post-command-hook` calls
        // `exit-minibuffer`.
        let specpdl_count = self.specpdl.len();
        self.try_specbind_or_unwind_to(
            specpdl_count,
            CommandLoopSymbol::InhibitQuit.id(),
            Value::T,
        )?;
        let result = super::super::hook_runtime::safe_run_named_hook(self, hook_sym, &[]);
        self.unbind_to_with_result(specpdl_count, result)
    }

    #[doc(hidden)]
    pub fn with_executing_kbd_macro_runtime<F>(
        &mut self,
        macro_events: Vec<Value>,
        run: F,
    ) -> EvalResult
    where
        F: FnOnce(&mut Self) -> EvalResult,
    {
        let scope = ExecutingKbdMacroRuntimeScope {
            snapshot: self.snapshot_executing_kbd_macro_runtime(),
            real_this_command: self
                .eval_symbol_by_id(CommandLoopSymbol::RealThisCommand.id())
                .unwrap_or(Value::NIL),
        };
        self.begin_executing_kbd_macro_runtime(macro_events);
        let result = run(self);
        let cleanup = self.finish_executing_kbd_macro_runtime_scope(scope);
        match cleanup {
            Ok(v) if v.is_nil() => result,
            Ok(other) => Ok(other),
            Err(flow) => Err(flow),
        }
    }

    #[doc(hidden)]
    pub fn reset_executing_kbd_macro_runtime_iteration(&mut self) {
        self.set_executing_kbd_macro_runtime_index(0);
    }

    #[doc(hidden)]
    pub fn finish_executing_kbd_macro_runtime_scope(
        &mut self,
        scope: ExecutingKbdMacroRuntimeScope,
    ) -> EvalResult {
        self.restore_executing_kbd_macro_runtime(scope.snapshot);
        self.assign_by_id(
            CommandLoopSymbol::RealThisCommand.id(),
            scope.real_this_command,
        );
        self.run_hook_if_bound("kbd-macro-termination-hook")
    }

    /// Run a named hook if it is bound and non-nil.
    #[doc(hidden)]
    pub fn run_hook_if_bound(&mut self, hook_name: &str) -> EvalResult {
        match self.eval_symbol(hook_name) {
            Ok(hook_val) if !hook_val.is_nil() => {
                // (run-hooks 'HOOK)
                super::super::builtins::dispatch_builtin(
                    self,
                    "run-hooks",
                    vec![Value::symbol(hook_name)],
                )
                .unwrap_or(Ok(Value::NIL))
            }
            _ => Ok(Value::NIL),
        }
    }

    #[doc(hidden)]
    pub fn queue_pending_safe_funcall(&mut self, function: Value, args: Vec<Value>) {
        self.pending_safe_funcalls.push(PendingSafeFuncall {
            function,
            args: args.into_iter().collect(),
        });
    }

    #[doc(hidden)]
    pub fn queue_pending_safe_hook(&mut self, hook_name: &str, args: &[Value]) {
        self.queue_pending_safe_funcall(
            Value::symbol("run-hook-with-args"),
            std::iter::once(Value::symbol(hook_name))
                .chain(args.iter().copied())
                .collect(),
        );
    }

    #[doc(hidden)]
    pub fn flush_pending_safe_funcalls(&mut self) {
        while let Some(funcall) = self.pending_safe_funcalls.pop() {
            let _ = self.apply(funcall.function, funcall.args);
        }
    }

    /// Trigger redisplay — calls the layout engine and sends frame to render thread.
    ///
    /// Mirrors GNU Emacs `redisplay()` (dispnew.c:5259).
    /// In batch mode (no callback), this is a no-op.
    #[doc(hidden)]
    pub fn redisplay(&mut self) -> Result<(), Flow> {
        self.redisplay_with_force(false)
    }

    #[doc(hidden)]
    pub fn redisplay_for_input_wait(&mut self) -> Result<(), Flow> {
        if self.input_wait_redisplay_allowed() {
            self.redisplay_with_force(false)?;
        }
        Ok(())
    }

    /// GNU's initial batch frame skips redisplay unless Lisp explicitly
    /// enables it for testing (xdisp.c:17372-17387, 39295-39301). Keep this
    /// decision local to the Context's frame and current variable binding;
    /// independent mutators share no display state here.
    #[doc(hidden)]
    pub fn input_wait_redisplay_allowed(&self) -> bool {
        !(self
            .frames
            .selected_frame()
            .is_some_and(|frame| frame.initial)
            && self
                .builtin_var_value(CommandLoopSymbol::RedisplaySkipInitialFrame.id())
                .is_some_and(|value| value.is_truthy()))
    }

    /// Preserve the first mode-line non-local exit across a frontend callback.
    /// This slot is Context-owned: one mutator uses a Context at a time, and
    /// the Flow keeps its Lisp payload pinned until it is taken or dropped.
    #[cold]
    #[inline(never)]
    pub fn defer_mode_line_display_flow(&mut self, flow: Flow) {
        if self.mode_line_display_flow.is_none() {
            self.mode_line_display_flow = Some(flow);
        }
    }

    /// Whether layout must stop before evaluating or publishing another row.
    #[inline]
    pub fn has_mode_line_display_flow(&self) -> bool {
        self.mode_line_display_flow.is_some()
    }

    /// Return the deferred mode-line exit after restoring redisplay state.
    #[inline]
    pub fn take_mode_line_display_flow(&mut self) -> Option<Flow> {
        self.mode_line_display_flow.take()
    }

    /// Generation of asynchronously decoded media state; see
    /// [`Self::invalidate_media`].
    pub fn media_generation(&self) -> u64 {
        self.media_generation
    }

    /// Record that async media reached a terminal state (an image finished
    /// decoding), forcing the next redisplay to rebuild rather than reuse a
    /// retained matrix holding the placeholder geometry.
    pub fn invalidate_media(&mut self) {
        self.media_generation = self.media_generation.wrapping_add(1);
        self.invalidate_redisplay();
    }

    /// Monotonic redisplay-invalidation counter — the analogue of GNU's
    /// `update_mode_lines || windows_or_buffers_changed` trigger family
    /// (bumped by `force-mode-line-update`, display-variable writes, media
    /// changes). Caches of redisplay-derived data key on it.
    pub fn redisplay_generation(&self) -> u64 {
        self.redisplay_generation
    }

    /// Generation of GNU `update_menu_bar` rebuild requests.
    pub fn menu_bar_rebuild_generation(&self) -> MenuBarRebuildGeneration {
        MenuBarRebuildGeneration(*self.menu_bar_rebuild_generation)
    }

    /// See the `context_instance_id` field.
    pub fn context_instance_id(&self) -> u64 {
        self.context_instance_id
    }

    /// The chrome dirty set — which windows must re-generate their mode /
    /// header / tab line. See [`crate::emacs_core::eval::data::window_state::ChromeDirty`]
    /// for the GNU flags this ports and for why nothing consults it as a skip
    /// yet.
    pub fn chrome_dirty(&self) -> &crate::emacs_core::eval::data::window_state::ChromeDirty {
        &self.chrome_dirty
    }

    /// GNU `bset_update_mode_line`: a buffer-scoped event that invalidates
    /// chrome everywhere the buffer might be shown.
    pub fn mark_chrome_dirty_all(&mut self) {
        self.chrome_dirty.mark_all();
    }

    /// GNU `wset_update_mode_line`: a window-scoped event.
    pub fn mark_chrome_dirty_window(&mut self, window: WindowId) {
        self.chrome_dirty.mark_window(window);
        // `wset_update_mode_line` ends in GNU `wset_redisplay`.  That helper
        // raises `windows_or_buffers_changed` exactly when W is not the
        // globally selected window (xdisp.c), which also makes
        // `update_menu_bar` rebuild.  Preserve that selected/nonselected
        // distinction here instead of making every window-local chrome event
        // a broad menu invalidation.
        let selected = self
            .frames
            .selected_frame()
            .is_some_and(|frame| frame.selected_window == window);
        if !selected {
            self.request_menu_bar_rebuild(MenuBarRebuildReason::WindowsOrBuffersChanged);
        }
    }

    /// Called by redisplay for each window whose chrome it actually generated.
    /// GNU's analogue is `mark_window_display_accurate_1`. A window that
    /// SKIPPED its chrome must not be acknowledged here — see
    /// [`crate::emacs_core::eval::data::window_state::ChromeDirty`] for why the
    /// acknowledgement is per window rather than a blanket clear.
    pub fn note_chrome_generated(&mut self, window: WindowId) {
        self.chrome_dirty.note_chrome_generated(window);
    }

    /// Drop a deleted window's chrome acknowledgement.
    pub fn forget_chrome_window(&mut self, window: WindowId) {
        self.chrome_dirty.forget_window(window);
    }

    #[doc(hidden)]
    pub fn invalidate_redisplay(&mut self) {
        tracing::debug!(target: "neomacs::redisplay_sig", "invalidate_redisplay");
        self.redisplay_generation = self.redisplay_generation.wrapping_add(1);
        *self.last_redisplay_signature = None;
    }

    /// Record an explicit body redisplay request (GNU `Fforce_window_update`,
    /// `src/window.c:4492`).
    ///
    /// Unlike [`Self::invalidate_redisplay`] this only moves the scopes the
    /// request names, so a chrome/mode-line invalidation cannot relayout body
    /// text and a targeted force cannot rebuild unrelated windows:
    ///
    /// - `AllWindows` — every window (nil OBJECT).
    /// - `Window(W)` — live window W, and (mirroring GNU's
    ///   `prevent_redisplay_optimizations_p` on `w->contents`) the buffer it
    ///   displays, so every other window showing that buffer also rebuilds.
    /// - `Buffer(B)` — every window displaying B.
    pub fn force_body_redisplay(&mut self, target: ForcedBodyRedisplay) {
        match target {
            ForcedBodyRedisplay::AllWindows => {
                self.body_redisplay_all = self.body_redisplay_all.wrapping_add(1);
            }
            ForcedBodyRedisplay::Window(window) => {
                let counter = self.body_redisplay_by_window.entry(window).or_insert(0);
                *counter = counter.wrapping_add(1);
                if let Some(buffer) = self.frames.window_buffer_id(window) {
                    self.mark_buffer_body_redisplay(buffer);
                }
            }
            ForcedBodyRedisplay::Buffer(buffer) => self.mark_buffer_body_redisplay(buffer),
        }
        self.invalidate_redisplay();
    }

    #[doc(hidden)]
    pub fn mark_buffer_body_redisplay(&mut self, buffer: BufferId) {
        let counter = self.body_redisplay_by_buffer.entry(buffer).or_insert(0);
        *counter = counter.wrapping_add(1);
    }

    /// The body-redisplay revision that any retained layout of BUFFER inside
    /// WINDOW must satisfy; see [`BodyRedisplayRevision`].
    pub fn body_redisplay_revision(
        &self,
        window: WindowId,
        buffer: BufferId,
    ) -> BodyRedisplayRevision {
        BodyRedisplayRevision::new(
            self.body_redisplay_all,
            self.body_redisplay_by_window
                .get(&window)
                .copied()
                .unwrap_or(0),
            self.body_redisplay_by_buffer
                .get(&buffer)
                .copied()
                .unwrap_or(0),
        )
    }

    /// Cross GNU `update_menu_bar`'s rebuild boundary and schedule redisplay.
    #[doc(hidden)]
    pub fn request_menu_bar_rebuild(&mut self, reason: MenuBarRebuildReason) {
        tracing::debug!(?reason, "request menu-bar rebuild");
        *self.menu_bar_rebuild_generation = self.menu_bar_rebuild_generation.wrapping_add(1);
        self.invalidate_redisplay();
    }

    /// Raise GNU's global `update_mode_lines` flag.
    ///
    /// `bset_update_mode_line` does this unconditionally for buffer-owned
    /// mutations such as `rename-buffer`, even when that buffer is not yet
    /// displayed.  Keep the menu and chrome effects inseparable here: both
    /// are consumers of the same GNU flag.
    #[doc(hidden)]
    pub fn request_global_mode_line_update(&mut self) {
        self.request_menu_bar_rebuild(MenuBarRebuildReason::UpdateModeLines);
        self.mark_chrome_dirty_all();
    }

    /// Apply GNU's local/global `force-mode-line-update` boundary.
    #[doc(hidden)]
    pub fn request_mode_line_update(&mut self, target: ModeLineUpdateTarget) {
        let has_mode_line_to_update = match target {
            ModeLineUpdateTarget::CurrentBuffer(buffer) => {
                self.frames.buffer_window_count(&self.edit.buffers, buffer) != 0
            }
            ModeLineUpdateTarget::AllBuffers => true,
        };
        if !has_mode_line_to_update {
            return;
        }

        if gnu_redisplay_hooks_enabled() {
            match target {
                ModeLineUpdateTarget::CurrentBuffer(buffer) => {
                    self.gnu_mark_buffer_mode_line(buffer)
                }
                ModeLineUpdateTarget::AllBuffers => {
                    self.gnu_mark_mode_lines_all();
                    self.request_global_mode_line_update();
                }
            }
        } else {
            self.request_global_mode_line_update();
        }
    }

    /// Mark redisplay dirty when a display-affecting variable is set.
    ///
    /// GNU Emacs has no per-variable redisplay flag in the `set`/`setq`
    /// store path: `redisplay_window` re-reads every live display slot
    /// each cycle and the current-matrix diff repaints any change
    /// (`src/xdisp.c:20535-20566`). Neomacs adds an aggressive
    /// optimization GNU lacks — `redisplay_with_force` early-returns on
    /// an unchanged `RedisplaySignature`, which captures buffer/overlay/
    /// text-property ticks, point and window geometry but NOT the
    /// per-buffer display slots (`truncate-lines`, `tab-width`,
    /// `header-line-format`, `cursor-type`, …). So a bare
    /// `(setq truncate-lines t)` left the screen stale until the next
    /// keystroke bumped the signature (Finding 6 in the command-loop
    /// audit; the "Doom blank pane" class of bug).
    ///
    /// To stay faithful to GNU's *observable* behavior we mark redisplay
    /// dirty here — the analogue of GNU `bset_redisplay` /
    /// `windows_or_buffers_changed` — when the variable being set is in
    /// the curated display-affecting set
    /// ([`neovm_edit::buffer::buffer::variable_affects_display_by_sym_id`]).
    /// This is checked at the single variable-set chokepoint so the
    /// answer is identical for every write path (tree-walk interpreter,
    /// bytecode VM, `set-default`, custom). The curated set keeps us
    /// from over-triggering redisplay on ordinary non-display variables.
    ///
    /// `sym_id` is resolved through `defvaralias` first so an alias of a
    /// display variable (e.g. an obsolete alias) still nudges redisplay.
    #[doc(hidden)]
    pub fn mark_redisplay_dirty_if_display_var(&mut self, sym_id: SymId) {
        let resolved =
            builtins::resolve_variable_alias_id_in_obarray(&self.obarray, sym_id).unwrap_or(sym_id);
        self.mark_redisplay_dirty_if_display_var_resolved(resolved);
    }

    /// The same check for a caller that has already walked the alias chain.
    ///
    /// Resolving an alias is not free: it is a chunked symbol-slot load plus a
    /// redirect test, ~28 instructions, and it runs on every variable write.
    /// The store paths resolve the symbol before they decide where to put the
    /// value, then handed the *resolved* id to a function that resolved it a
    /// second time -- a walk whose answer is, by construction, its own
    /// argument.  Measured on the call microbenchmark, this happened twice per
    /// variable write: 805,733 resolutions for 400,312 writes.
    #[doc(hidden)]
    pub fn mark_redisplay_dirty_if_display_var_resolved(&mut self, resolved: SymId) {
        if neovm_edit::buffer::buffer::variable_affects_display_by_sym_id(resolved) {
            self.invalidate_redisplay();
            // GNU covers the three chrome formats with an
            // `add-variable-watcher` calling `set-buffer-redisplay`
            // (lisp/frame.el:3752-3779 -> xdisp.c:922-931), which raises the
            // mode-line dirty flag. The curated display-variable set here is
            // the same list by another name, so the chrome members of it get
            // the chrome flag too.
            if neovm_edit::buffer::buffer::variable_affects_chrome_by_sym_id(resolved) {
                if gnu_redisplay_hooks_enabled() {
                    if let Some(buffer) = self.edit.buffers.current_buffer_id() {
                        self.gnu_mark_buffer_chrome_cache(buffer);
                    }
                } else {
                    self.chrome_dirty.mark_all();
                }
            }
            // A display-affecting variable changed: the incremental fast paths
            // key on this counter so they re-lay instead of reusing rows shaped
            // under the old setting (the four buffer/face ticks do not move here).
            self.display_var_change_count = self.display_var_change_count.wrapping_add(1);
        }
    }

    #[doc(hidden)]
    pub fn redisplay_with_force(&mut self, force: bool) -> Result<(), Flow> {
        self.redisplay_with_force_flow(force).map(|_| ())
    }

    #[doc(hidden)]
    pub fn redisplay_with_force_legacy(&mut self, force: bool) -> Result<(), Flow> {
        if let Some(flow) = self.take_mode_line_display_flow() {
            return Err(flow);
        }
        // Mirrors GNU `redisplay_internal` (xdisp.c:17242-17245): bail out
        // when `inhibit-redisplay` is non-nil. `run_window_change_functions`
        // (window.c:4116) specbinds this to t so any nested redisplay
        // triggered by a window-change hook is a no-op. Without this check
        // a hook that indirectly calls `redisplay` infinitely recurses.
        let inhibit_redisplay = self.builtin_var_value(CommandLoopSymbol::InhibitRedisplay.id());
        if inhibit_redisplay.as_ref().is_some_and(|v| v.is_truthy())
            && (!force
                || display_query_hooks::mode_line_flow_enabled()?
                || display_query_hooks::face_realization_enabled()?)
        {
            tracing::debug!(
                "redisplay inhibited by inhibit-redisplay={}",
                inhibit_redisplay.as_ref().unwrap()
            );
            return Ok(());
        }
        self.sync_pending_resize_events();
        // Sync window position caches from markers.  After text edits,
        // markers have auto-adjusted but the usize caches on Window::Leaf
        // may be stale.  Refresh them before redisplay reads positions.
        if let Some(buffer) = self.edit.buffers.current_buffer() {
            let buf_id = buffer.id();
            crate::emacs_core::eval::data::window_markers::sync_all_frames_for_buffer(
                &mut self.frames,
                buf_id,
            );
        }
        // GNU's selected-window point belongs to the selected window's buffer,
        // even when Lisp has temporarily made another buffer current.  Refresh
        // only the selected window cache from its own buffer; redisplay must
        // not realign `current-buffer` with the selected window here.
        if let Some(frame_id) = self.frames.selected_frame().map(|frame| frame.id) {
            super::display_query_hooks::remember_selected_window_point(
                &mut self.frames,
                &mut self.edit.buffers,
                frame_id,
            );
        }
        let before_signature = self.redisplay_signature();
        // A pending exact echo-area resize (GNU `resize_echo_area_exactly`)
        // must still drive a redisplay even when the visible signature is
        // otherwise unchanged: the message text can be identical while the
        // mini-window is still grown from a previous longer message and needs
        // to shrink back to fit. Don't skip while the request is pending.
        if tracing::enabled!(target: "neomacs::redisplay_sig", tracing::Level::DEBUG) {
            let captured: Vec<String> = before_signature
                .frame
                .as_ref()
                .map(|frame| {
                    frame
                        .windows
                        .iter()
                        .map(|window| match &window.buffer {
                            Some(buffer) => format!(
                                "w{}:b{}:tick{}:chars{}:total{}",
                                window.layout.id.0,
                                buffer.layout.id.0,
                                buffer.layout.modified_tick,
                                buffer.layout.chars_modified_tick,
                                buffer.layout.total_chars.get()
                            ),
                            None => format!(
                                "w{}:b{}:NO-BUFFER-SIG",
                                window.layout.id.0, window.layout.buffer_id.0
                            ),
                        })
                        .collect()
                })
                .unwrap_or_default();
            tracing::debug!(
                target: "neomacs::redisplay_sig",
                "signature windows=[{}] last_is_some={}",
                captured.join(" "),
                self.last_redisplay_signature.is_some()
            );
        }
        if tracing::enabled!(target: "neomacs::redisplay_sig", tracing::Level::DEBUG)
            && let Some(last) = self.last_redisplay_signature.as_ref()
            && *last != before_signature
        {
            // P3.5 C0: which part of the visible state moved since the last
            // redisplay -- the question behind every redisplay that could
            // have been skipped and was not.
            tracing::debug!(
                target: "neomacs::redisplay_sig",
                moved = ?last.moved_fields(&before_signature),
                "signature moved"
            );
        }
        // GNU `prepare_menu_bars` (xdisp.c:14237-14262) runs
        // `pre-redisplay-function` on EVERY redisplay that is not inhibited,
        // before the window layout, so hooks on `pre-redisplay-functions`
        // (e.g. `global-hl-line-mode` with sticky-flag 'window, the region
        // overlay) can refresh their overlays before redisplay reads them.
        // It passes nil -- the selected window only -- when no window needs
        // redisplay, which is what an unchanged visible state means here, and
        // t otherwise (GNU's per-window list is not tracked). The skip check
        // then reads the state the hooks left.
        let unchanged = self.last_redisplay_signature.as_ref() == Some(&before_signature);
        self.run_pre_redisplay_function(if unchanged { Value::NIL } else { Value::T });
        let skip_when_forced = display_query_hooks::redisplay_idle_skip_enabled()?;
        if (!force || skip_when_forced)
            && !self.echo_area_resize_exact_pending
            && self.last_redisplay_signature.is_some()
            && self.last_redisplay_signature.as_ref() == Some(&self.redisplay_signature())
            && !(force && self.displayed_buffer_changes_unacknowledged())
        {
            tracing::debug!("redisplay skipped: visible state unchanged");
            return Ok(());
        }
        self.resize_minibuffer_only_frames();
        // GNU `redisplay_internal` lifts labeled restrictions first
        // (`reset_outermost_restrictions`, src/xdisp.c:17415) and runs the
        // window change functions next (`run_window_change_functions`,
        // xdisp.c:17563), BEFORE any window is hscrolled or laid out
        // (`redisplay_windows`, then `hscroll_windows` at xdisp.c:17942).
        // A `window-size-change-functions` member therefore observes the
        // window as the previous redisplay left it: a fresh buffer in a
        // `truncate-lines` window with point past the right edge reads
        // `window-hscroll' 0 there, and the ttykey `truncate' scene recorded
        // exactly that from GNU while this path ran the hooks after layout
        // and read the auto-hscroll of 85 it had just computed. Same hook
        // pass per redisplay as before; only its position moved.
        let saved = self.edit.buffers.reset_outermost_restrictions();
        let _ = crate::emacs_core::eval::lifecycle::run_redisplay_window_change_hooks(self);
        // GNU `redisplay_internal` calls `hscroll_window_tree` (src/xdisp.c)
        // before laying out windows so each window's `hscroll` follows point;
        // for a truncated line whose point has moved off the right edge (the
        // `C-e` case, issue #140) this keeps the cursor visible. Updating
        // `Window::Leaf.hscroll` here makes both the layout render and
        // `(window-hscroll)` reflect the new value (no post-layout write-back).
        super::display_query_hooks::update_auto_hscroll(self);
        let has_fn = self.redisplay_fn.is_some();
        tracing::debug!("redisplay called (has_fn={})", has_fn);
        if let Some(mut f) = self.redisplay_fn.take() {
            f(self);
            // The layout pass inside `f` consumes any pending exact echo-area
            // resize (GNU `resize_echo_area_exactly`). Clear it now, once per
            // redisplay, so a later mid-command redisplay does not keep
            // shrinking a freshly grown message — GNU only resizes exactly at
            // the command boundary, not on every `redisplay_window`.
            self.echo_area_resize_exact_pending = false;
            if let Some(flow) = self.take_mode_line_display_flow() {
                self.edit.buffers.restore_outermost_restrictions(saved);
                self.redisplay_fn = Some(f);
                return Err(flow);
            }
            // Face realization also runs with the legacy hook policy. Its
            // callback flow belongs to this restored transaction boundary,
            // otherwise one failed lookup poisons subsequent redisplays.
            if display_query_hooks::face_realization_enabled()?
                && let Some(flow) = self.take_deferred_redisplay_layout_flow()
            {
                self.edit.buffers.restore_outermost_restrictions(saved);
                self.redisplay_fn = Some(f);
                return Err(flow);
            }
            self.edit.buffers.restore_outermost_restrictions(saved);
            self.redisplay_fn = Some(f);
        } else {
            self.echo_area_resize_exact_pending = false;
            self.edit.buffers.restore_outermost_restrictions(saved);
        }
        *self.last_redisplay_signature = Some(self.redisplay_signature());
        Ok(())
    }

    /// Run `pre-redisplay-function` (the driver of the `pre-redisplay-functions`
    /// hook) just before laying out, mirroring GNU `prepare_menu_bars`
    /// (xdisp.c:14230-14246). Features such as `global-hl-line-mode`
    /// (`global-hl-line-sticky-flag` = 'window) and the region overlay register
    /// on `pre-redisplay-functions` and depend on this to refresh their overlays
    /// before redisplay reads them; without it hl-line never highlights the
    /// current line.
    ///
    /// `inhibit-redisplay` is bound to t (GNU's redisplay is already
    /// `redisplaying_p`, and `run_redisplay_window_change_hooks` does the same)
    /// so a nested redisplay triggered by a hook is a no-op; an error from the
    /// hook is demoted (GNU calls via `dsafe_calln`, and the lisp driver wraps
    /// each hook in `with-demoted-errors`).
    #[doc(hidden)]
    pub fn run_pre_redisplay_function(&mut self, windows: Value) {
        let Some(function) = self.builtin_var_value(CommandLoopSymbol::PreRedisplayFunction.id())
        else {
            return;
        };
        if function.is_nil() {
            return;
        }
        let specpdl_count = self.specpdl.len();
        if let Err(flow) = self.try_specbind_or_unwind_to(
            specpdl_count,
            CommandLoopSymbol::InhibitRedisplay.id(),
            Value::T,
        ) {
            tracing::debug!("pre-redisplay binding signalled (ignored): {flow:?}");
            return;
        }
        // GNU passes the list of windows being redisplayed: `t` makes
        // `redisplay--pre-redisplay-functions` iterate every live window, nil
        // runs the hooks for the selected window only.
        let result = self.funcall_general(function, vec![windows]);
        let result = self.unbind_to_with_result(specpdl_count, result);
        if let Err(flow) = result {
            tracing::debug!("pre-redisplay-function signalled (ignored): {flow:?}");
        }
    }

    #[doc(hidden)]
    pub fn resize_minibuffer_only_frames(&mut self) {
        if !self
            .builtin_var_value(CommandLoopSymbol::ResizeMiniFrames.id())
            .is_some_and(|value| value.is_truthy())
        {
            return;
        }
        let frames: Vec<Value> = self
            .frames
            .frame_list()
            .into_iter()
            .filter_map(|frame_id| {
                self.frames.get(frame_id).and_then(|frame| {
                    (frame.visibility.is_visible()
                        && frame.minibuffer_window == Some(frame.root_window().id()))
                    .then_some(Value::make_frame(frame_id.0))
                })
            })
            .collect();
        for frame in frames {
            // GNU's fixed `Qwindow_resize_mini_frame` command symbol.
            static WINDOW_RESIZE_MINI_FRAME: WellKnown = WellKnown::WindowResizeMiniFrame;
            let _ = self.safe_funcall(Value::symbol(&WINDOW_RESIZE_MINI_FRAME), vec![frame]);
        }
    }

    /// Whether a buffer the selected frame shows changed since redisplay last
    /// acknowledged it (the layout's `reset_unchanged_region`, GNU's
    /// `mark_window_display_accurate`).
    ///
    /// The skip signature is taken when a redisplay ENDS, after the window
    /// change hooks; a change made after the layout's acknowledgement (by
    /// those hooks, say) is then part of the signature and yet was never laid
    /// out. GNU's next redisplay sees such a buffer as modified since its
    /// display and redisplays it, so a forced redisplay must not skip it: the
    /// change would otherwise reach the next edit frame as damage outside the
    /// edited line (the mode-line `:eval` oracle's `typenofl` case, where GNU
    /// keeps the mode line and the skipped redisplay made neomacs evaluate it).
    #[doc(hidden)]
    pub fn displayed_buffer_changes_unacknowledged(&self) -> bool {
        let Some(frame) = self.frames.selected_frame() else {
            return false;
        };
        let mut window_ids = frame.window_list();
        if let Some(minibuffer_window) = frame.minibuffer_window {
            window_ids.push(minibuffer_window);
        }
        window_ids.into_iter().any(|window_id| {
            frame
                .find_window(window_id)
                .and_then(|window| window.buffer_id())
                .and_then(|buffer_id| self.edit.buffers.get(buffer_id))
                .is_some_and(|buffer| buffer.changed_char_range().is_some())
        })
    }

    /// Plain viewport facts used to attribute a diagnostic scroll response.
    /// Point/chrome changes alone do not count as scrolling.
    pub fn input_latency_viewport(
        &self,
        frame_id: u64,
    ) -> Vec<neomacs_display_protocol::input_latency::ScrollViewport> {
        let Some(frame) = self
            .frames
            .get(neovm_value::display_vocab::FrameId(frame_id))
        else {
            return Vec::new();
        };
        frame
            .window_list()
            .into_iter()
            .filter_map(|window| {
                let layout = frame.window_layout_inputs(window)?;
                Some(neomacs_display_protocol::input_latency::ScrollViewport {
                    window: window.0,
                    buffer: layout.buffer_id.0,
                    start: layout.window_start.to_one_based_usize(),
                    hscroll: layout.hscroll,
                    vscroll: layout.vscroll,
                })
            })
            .collect()
    }

    #[doc(hidden)]
    pub fn redisplay_signature(&self) -> RedisplaySignature {
        let selected_frame = self.frames.selected_frame().map(|frame| frame.id.0);
        let selected_window = self
            .frames
            .selected_frame()
            .map(|frame| frame.selected_window.0);
        let frame = self.frames.selected_frame().map(|frame| {
            let mut window_ids = frame.window_list();
            if let Some(minibuffer_window) = frame.minibuffer_window {
                window_ids.push(minibuffer_window);
            }
            let mut windows = Vec::with_capacity(window_ids.len());
            for window_id in window_ids {
                let Some(window) = frame.find_window(window_id) else {
                    continue;
                };
                let Some(state) = window.redisplay_state() else {
                    continue;
                };
                let Some(layout) = frame.window_layout_inputs(window_id) else {
                    continue;
                };
                windows.push(RedisplayWindowSignature {
                    layout,
                    window_end: state.window_end,
                    old_point: state.old_point,
                    buffer: self.redisplay_buffer_signature(state.buffer_id),
                });
            }
            RedisplayFrameSignature {
                layout: frame.layout_inputs(),
                selected_window: frame.selected_window.0,
                window_state_change: frame.window_state_change,
                windows,
            }
        });
        RedisplaySignature {
            fontset_generation: super::display_query_hooks::fontset_generation(),
            compositor_scrolling_enabled: selected_window.is_some_and(|window| {
                crate::emacs_core::eval::hooks_proof::display::compositor_scrolling_enabled(
                    self,
                    neovm_edit::emacs_core::display_metrics::WindowId(window),
                )
            }),
            // Bindings and hooks may change without moving text or point.
            // Republish permission so the renderer cannot keep a stale grant.
            compositor_pixel_scroll: selected_window.is_some_and(|window| {
                crate::emacs_core::eval::hooks_proof::display::permits_compositor_pixel_scroll(
                    self,
                    neovm_edit::emacs_core::display_metrics::WindowId(window),
                )
            }),
            input_checkpoint: self.input_progress.checkpoint(),
            selected_frame,
            selected_window,
            current_buffer: self.edit.buffers.current_buffer_id().map(|id| id.0),
            current_message: self.current_message.clone(),
            active_minibuffer_window: self.active_minibuffer_window.map(|id| id.0),
            minibuffer_selected_window: self.minibuffer_selected_window.map(|id| id.0),
            face_change_count: self.face_change_count,
            obarray_function_epoch: self.obarray.function_epoch(),
            symbol_property_revision: neovm_value::symbol::SymbolPropertyRevision::current(),
            redisplay_generation: self.redisplay_generation,
            frame,
        }
    }

    #[doc(hidden)]
    pub fn redisplay_buffer_signature(
        &self,
        id: neovm_edit::buffer::BufferId,
    ) -> Option<RedisplayBufferSignature> {
        let buffer = self.edit.buffers.get(id)?;
        Some(RedisplayBufferSignature {
            layout: crate::emacs_core::eval::hooks_proof::display::redisplay_buffer_layout_inputs(
                self, id,
            )?,
            save_modified_tick: buffer.save_modified_tick(),
            autosave_modified_tick: buffer.autosave_modified_tick(),
            point: buffer.point_char_pos(),
            point_emacs_byte: buffer.point_emacs_byte_pos(),
            last_window_start: buffer.last_window_start(),
            last_selected_window: buffer.last_selected_window().map(|id| id.0),
        })
    }

    /// Perform a full mark-and-sweep garbage collection.
    #[tracing::instrument(level = "debug", skip(self))]
    pub fn gc_collect(&mut self) {
        self.gc_collect_exact();
    }

    /// Perform a full mark-and-sweep garbage collection using only explicit
    /// roots. Always runs to completion synchronously (force-completes any
    /// in-flight incremental mark), matching GNU `garbage-collect` semantics.
    #[tracing::instrument(level = "debug", skip(self))]
    pub fn gc_collect_exact(&mut self) {
        self.profiler_gc_start();
        self.gc_collect_from_current_roots_impl(true);
        self.profiler_gc_finish();
    }

    /// Safe-point GC entry. Uses concurrent marking after the heap's
    /// bootstrap cycle (and, for dump heaps, once the pdump is blackened);
    /// the first cycle runs a stop-the-world collection.
    ///
    /// Exact-GC stress mode always collects synchronously to completion: its
    /// purpose is a deterministic missing-root shakeout at every
    /// allocation-bearing safe point, which an asynchronous concurrent cycle
    /// would both defer and de-randomize.
    #[doc(hidden)]
    pub fn gc_collect_from_current_roots(&mut self) {
        self.profiler_gc_start();
        let force_complete = self.gc_stress && !self.tagged_heap.tagged_heap_generational_enabled();
        self.gc_collect_from_current_roots_impl(force_complete);
        self.profiler_gc_finish();
    }

    /// Drive a collection from the current roots.
    ///
    /// `force_complete` (explicit `garbage-collect`) runs synchronously to a
    /// full sweep. Otherwise, on partitioned cycles with incremental marking
    /// enabled, marking is sliced across safe points: each call advances one
    /// bounded slice and only the slice that drains the gray queue runs mark
    /// termination + sweep. The first cycle and non-incremental builds take the
    /// stop-the-world path.
    #[doc(hidden)]
    pub fn gc_collect_from_current_roots_impl(&mut self, force_complete: bool) {
        #[cfg(debug_assertions)]
        neovm_value::tagged::mutate::debug_assert_no_heap_mut_closure();
        // Collection also compacts undo state, materializes thread buffer
        // objects and publishes GC statistics. Those operations can allocate
        // or consult semantic TLS, so activate the collecting Context before
        // any of them runs. Keep the usual already-active safe point cheap.
        if !neovm_value::tagged::gc::tagged_heap_is_current(&self.tagged_heap) {
            self.setup_thread_locals();
        }
        // A6 publication discipline: collecting while run_loop's operand-stack
        // cursor holds an unpublished length would mark a stale bc_buf prefix.
        #[cfg(debug_assertions)]
        crate::emacs_core::bytecode::vm::debug_assert_no_live_stack_cursor();
        crate::emacs_core::subr::leaf::debug_assert_no_leaf_active!("a collection");
        // Inline set/restore, NOT a Drop guard (see the `gc_driver_active`
        // field doc): the body is infallible, so the trailing restore runs on
        // every normal exit (including the body's early `return`s), while a
        // panic escaping the body leaves the flag set for the module-boundary
        // containment probe to see. Save/restore (not set/clear) so a nested
        // collection — e.g. `(garbage-collect)` reached from a finalizer —
        // cannot clear the OUTER driver extent's flag on its way out.
        let prev = self.gc_driver_active;
        self.gc_driver_active = true;
        self.gc_collect_from_current_roots_body(force_complete);
        self.gc_driver_active = prev;
    }

    #[inline(never)]
    #[doc(hidden)]
    pub fn gc_collect_from_current_roots_body(&mut self, force_complete: bool) {
        // GNU `garbage_collect' shortens every live buffer's undo list before
        // it marks anything: "Don't keep undo information around forever. Do
        // this early on, so it is no problem if the user quits."
        // (src/alloc.c:5796-5800). This is the only place undo lists are
        // truncated -- `undo-boundary' does not do it.
        //
        // GNU compacts once per `garbage_collect' call. This collector slices
        // one collection across several safe points, so compact only when a
        // new cycle is about to start: an in-flight mark or sweep is the
        // continuation of a collection whose compaction already ran.
        if force_complete
            || match self.tagged_heap.mark_phase() {
                MarkPhase::Idle => true,
                MarkPhase::ConcurrentRunning
                | MarkPhase::StoppedMarking
                | MarkPhase::PublishingSweep
                | MarkPhase::Sweeping => false,
            }
        {
            neovm_edit::emacs_core::undo::compact_buffers_for_gc(self);
        }
        let start = std::time::Instant::now();
        // These two are the caches keyed on a raw heap address -- the address
        // of a lexical environment's head cons -- so a swept and recycled cons
        // could otherwise be read back as a different environment.  Nothing
        // else needs clearing here: the named-call and form-head caches key on
        // a symbol id and a function epoch, and what they hold is reachable
        // from the symbol's function cell, so it cannot be swept while the
        // entry is still valid.  Do not add an epoch-keyed cache to this list.
        //
        // Gating these on a sweep having actually run was tried and measured
        // NEUTRAL (2026-09-10): mx-tab -0.06%, org-editing +0.01%,
        // magit +0.02%, and rust-lsp-typing's median -0.35% inside its own
        // 6.6% bimodal spread.  The 32 stores are not worth the invalidation
        // risk of a narrower rule.
        self.lexenv_assq_cache.clear();
        self.lexenv_special_cache.clear();
        // Per-slice sweep budget in cons blocks (each ~4096 cells); the slice
        // reclaims proportionally more non-cons objects internally.
        const INCREMENTAL_SWEEP_BUDGET: usize = 8;
        let heap_ptr: *mut neovm_value::tagged::gc::TaggedHeap =
            self.tagged_heap.heap_root().as_ptr();
        // True only when a whole mark+sweep cycle finishes in this call, gating
        // the once-per-collection bookkeeping below.
        let cycle_completed;
        // SAFETY: GC is stop-the-world with exclusive `&mut self`. Root
        // enumeration only reads Context state while seeding the collector via
        // the raw heap pointer, which aliases `self.tagged_heap`.
        unsafe {
            if force_complete {
                // Explicit GC: drive any in-flight cycle to completion, then run
                // a fresh full stop-the-world collection of the current state
                // (GNU `garbage-collect` semantics).
                match (*heap_ptr).mark_phase() {
                    MarkPhase::Idle => {}
                    MarkPhase::ConcurrentRunning | MarkPhase::StoppedMarking => {
                        // Joining is idempotent. Both running and explicitly
                        // joined workers leave a mark to terminate and promote.
                        self.terminate_concurrent_mark(heap_ptr);
                        (*heap_ptr).finish_incremental_sweep_now();
                    }
                    MarkPhase::Sweeping => (*heap_ptr).finish_incremental_sweep_now(),
                    MarkPhase::PublishingSweep => {
                        debug_assert!(
                            false,
                            "a collection driver cannot reenter sweep publication"
                        );
                        return;
                    }
                }
                // Disarms a concurrent first cycle drained above, so this
                // cycle traces the whole image before it promotes (see
                // `begin_stw_collection`).
                (*heap_ptr).begin_stw_collection();
                self.seed_registered_mutator_roots_world_stopped(heap_ptr);
                (*heap_ptr).complete_collection();
                cycle_completed = true;
            } else {
                match (*heap_ptr).mark_phase() {
                    MarkPhase::Sweeping => {
                        // Phase 3: drain the deferred sweep started at mark termination.
                        //
                        // A sweep that allocation has outrun -- the NEXT cycle is
                        // already due -- finishes now, as the concurrent mark's cap
                        // does. Slices come one per safe point, and a loop whose safe
                        // points are sparse can cons faster than they reclaim: the next
                        // cycle cannot start until this one drains, so each cycle began
                        // with more garbage, swept longer, and set a higher threshold
                        // from the consing its own sweep let through. elb pidigits'
                        // threshold climbed 18 MB -> 311 MB over 14 cycles at a flat
                        // ~10 MB live, and the benchmark peaked at 3 GB resident (GNU,
                        // which sweeps synchronously, 495 MB). The cap is the threshold
                        // itself, not a multiple: at 4x (the mark's) a threshold that
                        // counts half the consing since the last cycle could still
                        // double every cycle.
                        let overdue = (*heap_ptr).bytes_since_gc() > (*heap_ptr).gc_threshold();
                        if overdue {
                            (*heap_ptr).finish_incremental_sweep_now();
                            cycle_completed = true;
                        } else if (*heap_ptr).incremental_sweep_slice(INCREMENTAL_SWEEP_BUDGET) {
                            cycle_completed = true; // sweep drained -> cycle done
                        } else {
                            return; // more sweep to do; defer bookkeeping
                        }
                    }
                    MarkPhase::ConcurrentRunning => {
                        // Phase 5: the background GC thread is marking while we run. If it
                        // has drained, run the (short) stop-the-world termination; else
                        // return immediately and keep mutating — this is the pause win.
                        // A hard cap forces termination if allocation outruns marking.
                        let cap = (*heap_ptr).gc_threshold().saturating_mul(4);
                        let must_finish = (*heap_ptr).bytes_since_gc() > cap;
                        let mark_done = (*heap_ptr).concurrent_mark_done();
                        // A tentative done flag is sufficient to stop now. The
                        // stopped-world admission folds every owner's bounded partial;
                        // publishing it and waiting another poll only grows the window.
                        if mark_done || must_finish {
                            if must_finish && !mark_done {
                                // Cap-forced: the GC thread had NOT drained — the
                                // residual mark now runs synchronously in the
                                // termination below. Record it so the pacing
                                // instrumentation escalates its lead and the trace
                                // attributes the pause.
                                (*heap_ptr).note_must_finish();
                            }
                            self.terminate_concurrent_mark(heap_ptr);
                            return; // sweep deferred; cycle not done yet
                        }
                        return; // GC thread still marking; mutator continues
                    }
                    MarkPhase::StoppedMarking => {
                        // A joined worker or dropped unpublished capture can leave a
                        // major's residual mark. Minor admission requires quiescence;
                        // finish that cycle with the complete current root set first.
                        match (*heap_ptr).drain_to_quiescent(|heap| {
                            self.seed_registered_mutator_roots_world_stopped(heap);
                        }) {
                            Ok(quiescent) => drop(quiescent),
                            Err(error) => {
                                // Preserve the unfinished cycle on failed recovery.
                                tracing::warn!(target: "neovm::gc", %error, "residual major recovery pending");
                                return;
                            }
                        }
                        cycle_completed = true;
                    }
                    MarkPhase::PublishingSweep => {
                        debug_assert!(
                            false,
                            "a collection driver cannot reenter sweep publication"
                        );
                        return;
                    }
                    MarkPhase::Idle => {
                        if self.gc_stress && (*heap_ptr).tagged_heap_generational_enabled() {
                            // A fresh automatic stress cycle is synchronous. Choose its
                            // generation before advancing the next-cycle stride; an
                            // explicit full collection and every continuation took the
                            // earlier branches and never advance this counter here.
                            let minor = (*heap_ptr).should_run_minor(
                                self.gc_runtime_settings_cache.memory_full.is_full(),
                                true,
                            );
                            (*heap_ptr).note_generation_stress_cycle_started();
                            if minor {
                                (*heap_ptr).begin_minor_collection().expect(
                        "scheduler drained the preceding mark and sweep before minor admission",
                    );
                                self.seed_registered_mutator_roots_world_stopped(heap_ptr);
                                (*heap_ptr).complete_minor_collection();
                                (*heap_ptr).finish_incremental_sweep_now();
                            } else {
                                (*heap_ptr).begin_stw_collection();
                                self.seed_registered_mutator_roots_world_stopped(heap_ptr);
                                (*heap_ptr).complete_collection();
                            }
                            cycle_completed = true;
                        } else if (*heap_ptr).should_run_minor(
                            self.gc_runtime_settings_cache.memory_full.is_full(),
                            false,
                        ) {
                            // Minor marking stops every registered mutator and visits
                            // their roots. Promotion completes before mutating resumes;
                            // the existing sweep continuation records cycle completion.
                            (*heap_ptr).begin_minor_collection().expect(
                    "scheduler drained the preceding mark and sweep before minor admission",
                );
                            self.seed_registered_mutator_roots_world_stopped(heap_ptr);
                            (*heap_ptr).complete_minor_collection();
                            return;
                        } else {
                            let concurrent_kind = if (*heap_ptr).should_run_concurrent() {
                                Some(ConcurrentCycleKind::Threshold)
                            } else if (*heap_ptr).is_partition_first_cycle() {
                                Some(ConcurrentCycleKind::FirstPartition)
                            } else {
                                None
                            };
                            if let Some(kind) = concurrent_kind {
                                match self.start_concurrent_mark(heap_ptr, kind) {
                        Ok(()) => return,
                        Err(neovm_value::tagged::gc::ConcurrentMarkAdmissionError::Excluded(_)) => {
                            // Held facade roots choose exact synchronous GC.
                        }
                        Err(neovm_value::tagged::gc::ConcurrentMarkAdmissionError::ForeignHeap {
                            ..
                        }) => {
                            // An exchanged heap cannot use this owner's old
                            // census. Keep the incomplete cycle for explicit
                            // owner recovery rather than tracing foreign roots.
                            return;
                        }
                        Err(
                            neovm_value::tagged::gc::ConcurrentMarkAdmissionError::CollectorBusy
                            | neovm_value::tagged::gc::ConcurrentMarkAdmissionError::NotStarted,
                        ) => {
                            // A dropped or rejected capture keeps its gray work;
                            // the synchronous recovery below must finish it.
                        }
                    }
                            }
                            match (*heap_ptr).mark_phase() {
                                MarkPhase::StoppedMarking => {
                                    // SAFETY: this current owner's closed GC-driver extent
                                    // excludes heap/obarray/TLS writers and callbacks. The
                                    // complete owner census reseeds every external root before
                                    // the unlaunched cycle drains marking and sweep.
                                    let recovery_cycle =
                                        (*heap_ptr).gc_collections().saturating_add(1);
                                    let drain = (*heap_ptr).drain_to_quiescent(|heap| {
                                        self.seed_registered_mutator_roots_world_stopped(heap);
                                    });
                                    match drain {
                                        Ok(quiescent) => drop(quiescent),
                                        Err(error) => {
                                            // Preserve incomplete storage and work for explicit
                                            // retry; do not start or account another collection.
                                            tracing::debug!(
                                                target: "neovm::gc",
                                                kind = GcTraceKind::SynchronousRecoveryPending.as_str(),
                                                cycle = recovery_cycle,
                                                error = %error,
                                                "NEOVM_GC synchronous recovery pending: {error}"
                                            );
                                            return;
                                        }
                                    }
                                }
                                MarkPhase::Idle => {
                                    // No concurrent state has been armed on exclusion rejection.
                                    (*heap_ptr).begin_stw_collection();
                                    self.seed_registered_mutator_roots_world_stopped(heap_ptr);
                                    (*heap_ptr).complete_collection();
                                }
                                MarkPhase::ConcurrentRunning
                                | MarkPhase::PublishingSweep
                                | MarkPhase::Sweeping => {
                                    debug_assert!(
                                        false,
                                        "failed mark admission must leave idle or stopped marking"
                                    );
                                    return;
                                }
                            }
                            cycle_completed = true;
                        }
                    }
                }
            }
        }
        if !cycle_completed {
            return;
        }
        // First partition cycle (concurrent bootstrap): with the sweep now
        // drained, promote survivors + blacken the image + build the initial
        // remembered set. No-op for every other cycle (including the STW
        // paths, which promote inside `complete_collection`).
        unsafe {
            (*heap_ptr).finish_first_partition_cycle();
        }
        self.gc_pending = false;
        self.gc_count += 1;
        self.update_gc_runtime_stats(start.elapsed());
        self.sync_gc_threshold_from_runtime_settings();
        // Destroy the GPU objects of shader-surface handles this cycle swept.
        // Every completed collection funnels through this block (explicit
        // `garbage-collect` and the safe-point paths above), so this is the
        // single drain point for `TaggedHeap::pending_surface_destroys`.
        self.drain_pending_surface_destroys();
        self.drain_pending_video_destroys();
        // GNU `garbage_collect` runs the doomed finalizers before
        // `post-gc-hook`.  A collection that completes while the cconv memo
        // observes a Lisp run leaves both to the end of that run.
        if !self.cconv_memo_defers_gc_hooks() {
            self.run_doomed_finalizers();
            self.run_post_gc_hook();
        }
        if self.gc_stress {
            // GNU resets `consing_until_gc` before running post-gc-hook and
            // runs the hook with GC inhibited.  Keep Neomacs' exact-GC stress
            // mode from treating hook bookkeeping allocation as a fresh
            // allocation-bearing safe point.
            self.tagged_heap.reset_bytes_since_gc();
        }
    }

    /// Seed every evaluator/context root into the collector's gray queue and
    /// install the per-buffer marker-chain head slots used by sweep. Does NOT
    /// clear marks, so it is safe to call both at incremental start and again
    /// at mark termination (re-snapshotting roots).
    ///
    /// Returns the per-group cost/volume breakdown of this seed (diagnostics
    /// only — seeding order and content are unchanged). Groups are the
    /// `trace_roots` sections, the per-side-table thread-local collects, the
    /// thread-local seed loop (`tl_seed`), and the marker-chain-head install
    /// (`marker_heads`, whose count is the live-buffer count).
    ///
    /// Safety: caller holds exclusive `&mut self`; `heap_ptr` aliases
    /// `self.tagged_heap`. Root enumeration only reads Context state.
    #[doc(hidden)]
    pub unsafe fn seed_all_context_roots(
        &mut self,
        heap_ptr: *mut neovm_value::tagged::gc::TaggedHeap,
    ) -> neovm_value::tagged::gc::RootSeedBreakdown {
        use std::cell::{Cell, RefCell};
        let seed_t0 = std::time::Instant::now();
        // Per-group recorder shared by the two `trace_roots` closures via
        // interior mutability (both need it: the boundary closure closes the
        // running group, the visit closure counts values).
        let groups: RefCell<Vec<neovm_value::tagged::gc::RootGroup>> =
            RefCell::new(Vec::with_capacity(32));
        let cur_name: Cell<Option<&'static str>> = Cell::new(None);
        let cur_t0: Cell<std::time::Instant> = Cell::new(seed_t0);
        let cur_count: Cell<usize> = Cell::new(0);
        let close_group = || {
            if let Some(name) = cur_name.get() {
                groups.borrow_mut().push((
                    name,
                    cur_t0.get().elapsed().as_micros() as u64,
                    cur_count.get(),
                ));
            }
        };
        #[cfg(debug_assertions)]
        let mut root_index = 0usize;
        // Share the count with the group recorder, but capture the stable heap
        // pointer by value so each root does not reload it through a reference.
        let root_count = &cur_count;
        // SAFETY: this function runs only in the stopped seeding window. The
        // independent loan drops (including on unwind) before returning.
        let mut thread_local_roots = unsafe { (*heap_ptr).root_seed_scratch() };
        let heap_identity = unsafe { (*heap_ptr).tagged_heap_identity() };
        self.trace_roots_reusing(
            heap_identity,
            |roots| {
                // SAFETY: borrow only from the live collector loan, and end
                // this read before the root visitor writes through heap_ptr.
                neovm_value::tagged::transport::collect_shared_root_gc_roots(
                    unsafe { &*heap_ptr },
                    roots,
                );
            },
            &mut |name| {
                close_group();
                cur_name.set(Some(name));
                cur_count.set(0);
                cur_t0.set(std::time::Instant::now());
            },
            &mut move |root| {
                root_count.set(root_count.get() + 1);
                #[cfg(debug_assertions)]
                {
                    let origin = format!("context-root#{root_index}");
                    root_index += 1;
                    unsafe {
                        (*heap_ptr).seed_root_with_origin(root, &origin);
                    }
                }
                #[cfg(not(debug_assertions))]
                {
                    unsafe {
                        (*heap_ptr).seed_root(root);
                    }
                }
            },
            thread_local_roots.roots(),
        );
        close_group();
        let mut groups = groups.into_inner();
        // A JIT leaf may keep addresses inside the obarray it was compiled
        // against; drop the cache before its roots are walked if this
        // Context's obarray is not that one (P1.4 §3.6).
        #[cfg(feature = "jit")]
        crate::emacs_core::jit::cache::sync_cache_to_obarray(self.obarray.generation());
        // Exact GC can finish an older sweep after activation. Read the epoch
        // here, before any TLS cache reconstructs Values from address keys.
        let collection_epoch = unsafe { (*heap_ptr).gc_collections() };
        // STW begin does not arm mark_in_progress. This call certifies actual
        // seeding, independently of that flag; snapshots never certify it.
        debug_assert!(!unsafe { (*heap_ptr).sweep_in_progress() });
        let mut tl_seed_us = 0;
        let mut tl_seed_count = 0;
        for_each_thread_local_root_source(
            heap_identity,
            collection_epoch,
            neovm_value::tagged::gc::CacheRootScan::Collection,
            |origin, collect| {
                let roots = thread_local_roots.roots();
                debug_assert!(roots.is_empty());
                let collect_t0 = std::time::Instant::now();
                collect(roots);
                groups.push((origin, collect_t0.elapsed().as_micros() as u64, roots.len()));
                let seed_t0 = std::time::Instant::now();
                tl_seed_count += roots.len();
                for root in roots.drain(..) {
                    // SAFETY: same stopped collector loan as the Context walk.
                    unsafe {
                        (*heap_ptr).seed_root_with_origin(root, origin);
                    }
                }
                tl_seed_us += seed_t0.elapsed().as_micros() as u64;
            },
        );
        drop(thread_local_roots);
        groups.push(("tl_seed", tl_seed_us, tl_seed_count));
        // Install per-buffer marker-chain head slots so `unchain_dead_markers`
        // can splice unmarked markers out of every live chain before sweep.
        // Mirrors GNU `sweep_buffer → unchain_dead_markers` (alloc.c).
        let heads_t0 = std::time::Instant::now();
        // Safety: stop-the-world GC — no concurrent borrows of the buffer
        // storage exist (the pre-refactor body relied on the enclosing
        // `unsafe fn` for this same call).
        let chain_heads = unsafe { self.edit.buffers.collect_marker_chain_head_slots() };
        let heads_count = chain_heads.len();
        unsafe {
            (*heap_ptr).set_marker_chain_head_slots(chain_heads);
        }
        groups.push((
            "marker_heads",
            heads_t0.elapsed().as_micros() as u64,
            heads_count,
        ));
        neovm_value::tagged::gc::RootSeedBreakdown {
            total_us: seed_t0.elapsed().as_micros() as u64,
            groups,
        }
    }

    /// Start a non-blocking concurrent mark (Phase 5): clear marks + seed the
    /// complete root snapshot into the gray queue, then hand it to the GC thread.
    /// Returns immediately — the mutator runs while the GC thread marks conses.
    ///
    /// The whole handshake and each phase are timed into `HandshakeStats`
    /// (clear/runtime/remembered are recorded heap-side by `concurrent_begin`;
    /// conssnap/vecsnap/jobasm by `launch_concurrent_mark`) and printed under
    /// `NEOVM_GC_TRACE=1`. Size probes are refreshed AFTER the pause is
    /// stamped so probe collection never inflates the measured pause.
    ///
    /// # Safety
    /// As `seed_all_context_roots`: the current owner exclusively admits the
    /// matching heap, Obarray and TLS writers without Lisp or allocation
    /// safepoints through capture and launch. Rejection precedes blackening.
    #[doc(hidden)]
    pub unsafe fn start_concurrent_mark(
        &mut self,
        heap_ptr: *mut neovm_value::tagged::gc::TaggedHeap,
        kind: ConcurrentCycleKind,
    ) -> Result<(), neovm_value::tagged::gc::ConcurrentMarkAdmissionError> {
        let start_t0 = std::time::Instant::now();
        let (obsnap_us, roots_breakdown, ob_slots, ob_chunks);
        // SAFETY: heap_ptr is this Context's owned heap, exclusively admitted
        // to the start handshake. No Lisp callback or allocation safepoint
        // runs during capture, seeding or publication of the marker job.
        unsafe {
            let mut permit = (*heap_ptr).permit_concurrent_mark()?;
            match kind {
                ConcurrentCycleKind::Threshold => {}
                ConcurrentCycleKind::FirstPartition => permit.arm_first_cycle(),
            }
            let mut capture = permit.begin();
            // Renew the designator from the exclusive capture loan, rather
            // than reusing the pointer that preceded that mutable reborrow.
            let heap_ptr = capture.heap_mut() as *mut neovm_value::tagged::gc::TaggedHeap;
            // CONCURRENT OBARRAY SCAN (Stage 1b). Capture the obarray chunk snapshot
            // at THIS world-stopped point — the same instant the cons snapshot is
            // taken (inside `launch_concurrent_mark`) and the roots are seeded — so
            // `n_slots`/`n_chunks` reflect the start-of-cycle obarray. The heap can't
            // reach the Context-side obarray, so we build it here and stage it on the
            // heap for the launch to move into the job. The symbol-cell skip guard,
            // scoped to just the seed, keeps the start seed from also pushing the
            // symbol cells the GC thread now owns (the BLV pool + non-obarray roots
            // still seed normally).
            let obsnap_t0 = std::time::Instant::now();
            // SAFETY: start runs on the sole heap/obarray writer without Lisp
            // callbacks. SATB, retirement and seqlock writes govern the cycle;
            // explicit finish or abandonment retains the reader's storage.
            let world = neovm_value::tagged::gc::scan_contract::SingleMutatorWorld::from_heap(
                &mut *heap_ptr,
            );
            let snap = self.obarray.scan_snapshot(&world);
            obsnap_us = obsnap_t0.elapsed().as_micros() as u64;
            ob_slots = snap.n_slots();
            ob_chunks = snap.n_chunks();
            (*heap_ptr).set_pending_obarray_scan(snap);
            {
                let _skip = neovm_value::symbol::ObarraySymbolCellSkipGuard::new();
                roots_breakdown = self.seed_registered_mutator_roots_world_stopped(heap_ptr);
            }
            capture.launch()?;
        }
        let total_us = start_t0.elapsed().as_micros() as u64;
        // Pause is stamped; stats bookkeeping + probes below are off-pause.
        let pace_lead = self.tagged_heap.pace_probe();
        let pace_bytes = self.tagged_heap.bytes_since_gc();
        let pace_thr = self.tagged_heap.gc_threshold();
        #[cfg(feature = "jit")]
        let (jit_entries, jit_slots) = crate::emacs_core::jit::cache::compiled_cache_probe();
        #[cfg(not(feature = "jit"))]
        let (jit_entries, jit_slots) = (0usize, 0usize);
        let bc_depth = self.bc_buf.len();
        let specpdl_depth = self.specpdl.len();
        // SAFETY: the capture loan ended at launch; this current owner's
        // original heap designator is again valid for exclusive statistics.
        let cycle = self.tagged_heap.gc_collections().saturating_add(1);
        let hs = unsafe { (*heap_ptr).handshake_stats_mut() };
        hs.last_start_obsnap_us = obsnap_us;
        hs.last_start_roots = roots_breakdown;
        hs.last_start_total_us = total_us;
        hs.max_start_total_us = hs.max_start_total_us.max(total_us);
        hs.probe_obarray_slots = ob_slots;
        hs.probe_obarray_chunks = ob_chunks;
        hs.probe_bc_buf_depth = bc_depth;
        hs.probe_specpdl_depth = specpdl_depth;
        hs.probe_jit_compiled_entries = jit_entries;
        hs.probe_jit_reloc_slots = jit_slots;
        hs.probe_buffer_count = hs
            .last_start_roots
            .groups
            .iter()
            .find(|(name, _, _)| *name == "marker_heads")
            .map(|&(_, _, count)| count)
            .unwrap_or(0);
        if tracing::event_enabled!(target: "neovm::gc", tracing::Level::DEBUG, kind, cycle) {
            let groups = hs.last_start_roots.format_nonzero();
            let probes = hs.format_probes();
            tracing::debug!(
                target: "neovm::gc",
                kind = GcTraceKind::ConcurrentStart.as_str(),
                cycle,
                total_us,
                pace_bytes,
                pace_threshold = pace_thr,
                pace_lead,
                clear_us = hs.last_start_clear_us,
                clear_cons_us = hs.last_start_clear_cons_us,
                clear_noncons_us = hs.last_start_clear_noncons_us,
                clear_mapped_us = hs.last_start_clear_mapped_us,
                runtime_us = hs.last_start_runtime_us,
                runtime_roots = hs.last_start_runtime_roots,
                remembered_us = hs.last_start_remembered_us,
                remembered_roots = hs.last_start_remembered_roots,
                obsnap_us,
                roots_us = hs.last_start_roots.total_us,
                conssnap_us = hs.last_start_conssnap_us,
                vecsnap_us = hs.last_start_vecsnap_us,
                jobasm_us = hs.last_start_jobasm_us,
                groups = %groups,
                probes = %probes,
                "NEOVM_GC concurrent_start {total_us}us \
                 pace[bytes={pace_bytes} thr={pace_thr} lead={pace_lead}] \
                 [clear={}us[cons={} noncons={} \
                 mapped={}] runtime={}us({}) \
                 remembered={}us({}) obsnap={obsnap_us}us roots={}us conssnap={}us \
                 vecsnap={}us jobasm={}us groups[{}] probes[{}]]",
                hs.last_start_clear_us,
                hs.last_start_clear_cons_us,
                hs.last_start_clear_noncons_us,
                hs.last_start_clear_mapped_us,
                hs.last_start_runtime_us,
                hs.last_start_runtime_roots,
                hs.last_start_remembered_us,
                hs.last_start_remembered_roots,
                hs.last_start_roots.total_us,
                hs.last_start_conssnap_us,
                hs.last_start_vecsnap_us,
                hs.last_start_jobasm_us,
                groups,
                probes,
            );
        }
        Ok(())
    }

    /// Terminate a concurrent mark stop-the-world: stop the GC thread and reclaim
    /// the heap, re-snapshot the COMPLETE root set (covering root->white edges the
    /// barrier cannot observe), drain the residual marking (deferred non-cons +
    /// SATB + roots) to a fixpoint, then start the deferred sweep. The expensive
    /// cons-spine traversal already happened concurrently; this pause finishes the
    /// veclike/string traces and any roots that appeared during the window.
    ///
    /// Safety: as `seed_all_context_roots`.
    /// Every phase of the pre-drain `roots=` lump is timed into
    /// `HandshakeStats` (join/fold heap-side; runtime+remembered by
    /// `reseed_runtime_and_remembered_roots`; the context re-seed per group;
    /// the Stage 1b new-symbol residual here) along with the post-drain
    /// finalizer/weak/unchain passes (heap-side, in `incremental_finish`).
    /// Probes are refreshed after the pause is stamped.
    #[doc(hidden)]
    pub unsafe fn terminate_concurrent_mark(
        &mut self,
        heap_ptr: *mut neovm_value::tagged::gc::TaggedHeap,
    ) {
        let term_t0 = std::time::Instant::now();
        let (roots_us, drain_us);
        let (ctxroots_breakdown, newsyms_us);
        let mut newsyms_roots = 0usize;
        unsafe {
            // Reclaim exclusive heap ownership: stop the GC thread and fold its
            // residual SATB + deferred work back into the gray queue.
            (*heap_ptr).join_concurrent_mark();
            (*heap_ptr).reseed_runtime_and_remembered_roots();
            {
                // Stage 1a: the symbol-cell SATB barrier retained every
                // value/function/plist overwrite during the mark window, so this
                // TERMINATION re-seed skips the ~450k-symbol obarray walk (the
                // dominant root pause). The guard still seeds the BLV-pool residual
                // + every non-obarray Context root, and restores full-scan on drop
                // so the start seed + STW full-collection seeds are unaffected.
                let _skip = neovm_value::symbol::ObarraySymbolCellSkipGuard::new();
                ctxroots_breakdown = self.seed_registered_mutator_roots_world_stopped(heap_ptr);
            }
            // Stage 1b CONCURRENT OBARRAY SCAN termination residual: the GC thread's
            // scan covered only the symbol cells present at the start snapshot (slots
            // [0, start_slots)). Symbols interned MID-CYCLE live in slots
            // >= start_slots and were never scanned, and the symbol-cell SATB barrier
            // only retains OVERWRITES of pre-existing heap values (it does not seed a
            // brand-new symbol's initial val/function/plist). So at this STW point,
            // bounded-re-seed exactly the new range. Chosen over the "seed the FULL
            // obarray un-skipped" fallback: it preserves the Stage 1a win (no full
            // ~450k-symbol walk) while staying correct. `None` only if no start
            // snapshot was captured, in which case the residual is skipped.
            let newsyms_t0 = std::time::Instant::now();
            if let Some(start_slots) = (*heap_ptr).take_concurrent_obarray_start_slots() {
                self.obarray
                    .trace_new_symbol_cells(start_slots, &mut |root| {
                        newsyms_roots += 1;
                        #[cfg(debug_assertions)]
                        {
                            (*heap_ptr).seed_root_with_origin(root, "stage1b-new-symbol");
                        }
                        #[cfg(not(debug_assertions))]
                        {
                            (*heap_ptr).seed_root(root);
                        }
                    });
            }
            newsyms_us = newsyms_t0.elapsed().as_micros() as u64;
            roots_us = term_t0.elapsed().as_micros();
            let bytes_before = (*heap_ptr).live_bytes();
            let pause_t0 = std::time::Instant::now();
            (*heap_ptr).incremental_drain_all();
            drain_us = pause_t0.elapsed().as_micros();
            (*heap_ptr).incremental_finish(bytes_before, pause_t0);
        }
        // Pause work done; stats bookkeeping + probes below are off-pause.
        #[cfg(feature = "jit")]
        let (jit_entries, jit_slots) = crate::emacs_core::jit::cache::compiled_cache_probe();
        #[cfg(not(feature = "jit"))]
        let (jit_entries, jit_slots) = (0usize, 0usize);
        let bc_depth = self.bc_buf.len();
        let specpdl_depth = self.specpdl.len();
        let ob_slots = self.obarray.current_slot_len();
        let hs = unsafe { (*heap_ptr).handshake_stats_mut() };
        hs.last_term_ctxroots = ctxroots_breakdown;
        hs.last_term_newsyms_us = newsyms_us;
        hs.last_term_newsyms_roots = newsyms_roots;
        hs.last_term_roots_total_us = roots_us as u64;
        hs.max_term_roots_total_us = hs.max_term_roots_total_us.max(roots_us as u64);
        hs.probe_bc_buf_depth = bc_depth;
        hs.probe_specpdl_depth = specpdl_depth;
        hs.probe_obarray_slots = ob_slots;
        hs.probe_jit_compiled_entries = jit_entries;
        hs.probe_jit_reloc_slots = jit_slots;
        hs.probe_buffer_count = hs
            .last_term_ctxroots
            .groups
            .iter()
            .find(|(name, _, _)| *name == "marker_heads")
            .map(|&(_, _, count)| count)
            .unwrap_or(0);
        if tracing::event_enabled!(target: "neovm::gc", tracing::Level::DEBUG, kind, cycle) {
            let cycle = self.tagged_heap.gc_collections().saturating_add(1);
            let stats = self.tagged_heap.sweep_stats();
            let (leaf_claimed, hash_claimed) = self.tagged_heap.last_concurrent_claim_counts();
            let hs = self.tagged_heap.handshake_stats();
            let total_us = term_t0.elapsed().as_micros();
            let groups = hs.last_term_ctxroots.format_nonzero();
            let probes = hs.format_probes();
            tracing::debug!(
                target: "neovm::gc",
                kind = GcTraceKind::ConcurrentTermination.as_str(),
                cycle,
                total_us,
                roots_us,
                drain_us,
                fold_us = stats.last_termination_fold_us,
                deferred = stats.last_termination_deferred,
                satb = stats.last_termination_satb,
                partial_words = stats.last_termination_partial_words,
                str_claimed = stats.last_concurrent_str_claimed,
                f_claimed = stats.last_concurrent_float_claimed,
                sub_dropped = stats.last_concurrent_subr_dropped,
                v_claimed = stats.last_concurrent_vec_claimed,
                bc_claimed = stats.last_concurrent_bc_claimed,
                leaf_claimed,
                ht_claimed = hash_claimed,
                kinds = %stats.last_termination_kinds,
                join_us = hs.last_term_join_us,
                runtime_us = hs.last_term_runtime_us,
                runtime_roots = hs.last_term_runtime_roots,
                remembered_us = hs.last_term_remembered_us,
                remembered_roots = hs.last_term_remembered_roots,
                ctxroots_us = hs.last_term_ctxroots.total_us,
                newsyms_us,
                newsyms_roots,
                finalizer_us = hs.last_term_finalizer_us,
                weak_us = hs.last_term_weak_us,
                unchain_us = hs.last_term_unchain_us,
                groups = %groups,
                probes = %probes,
                "NEOVM_GC concurrent_termination {}us [roots={roots_us}us drain={drain_us}us \
                 fold={}us deferred={} satb={} partial_words={} str_claimed={} f_claimed={} sub_dropped={} \
                 v_claimed={} bc_claimed={} leaf_claimed={} ht_claimed={} kinds[{}] join={}us \
                 runtime={}us({}) remembered={}us({}) ctxroots={}us newsyms={newsyms_us}us({}) \
                 finalizer={}us weak={}us unchain={}us groups[{}] probes[{}]]",
                total_us,
                stats.last_termination_fold_us,
                stats.last_termination_deferred,
                stats.last_termination_satb,
                stats.last_termination_partial_words,
                stats.last_concurrent_str_claimed,
                stats.last_concurrent_float_claimed,
                stats.last_concurrent_subr_dropped,
                stats.last_concurrent_vec_claimed,
                stats.last_concurrent_bc_claimed,
                leaf_claimed,
                hash_claimed,
                stats.last_termination_kinds,
                hs.last_term_join_us,
                hs.last_term_runtime_us,
                hs.last_term_runtime_roots,
                hs.last_term_remembered_us,
                hs.last_term_remembered_roots,
                hs.last_term_ctxroots.total_us,
                newsyms_roots,
                hs.last_term_finalizer_us,
                hs.last_term_weak_us,
                hs.last_term_unchain_us,
                groups,
                probes,
            );
        }
    }

    #[doc(hidden)]
    pub fn with_gc_inhibited<T>(&mut self, f: impl FnOnce(&mut Self) -> T) -> T {
        let mut guard = GcInhibitGuard::enter(self);
        f(guard.context())
    }

    #[doc(hidden)]
    pub fn run_post_gc_hook(&mut self) {
        let hook = crate::emacs_core::hook_runtime::hook_symbol_by_id(self, post_gc_hook_symbol());
        let _ = self.with_gc_inhibited(|eval| {
            crate::emacs_core::hook_runtime::safe_run_named_hook(eval, hook, &[])
        });
    }

    /// Run the functions queued for finalizer objects the just-finished cycle
    /// found unreachable — GNU `run_finalizers` (alloc.c). The whole batch is
    /// taken up front, so a finalizer created (and doomed) during one of these
    /// calls lands in a later batch after a later cycle. Each function is
    /// called with zero args; errors are ignored so one failing finalizer
    /// cannot block the rest (GNU wraps each call in a catch-all
    /// `internal_condition_case`).
    #[doc(hidden)]
    pub fn run_doomed_finalizers(&mut self) {
        let functions = self.tagged_heap.take_doomed_finalizer_functions();
        if functions.is_empty() {
            return;
        }
        // The taken batch left the heap-side queue root; keep it rooted for
        // the duration — `with_gc_inhibited` blocks safe-point GCs, but an
        // explicit `garbage-collect` inside a finalizer still collects.
        let saved_roots = save_scratch_gc_roots();
        for function in functions.iter().copied() {
            push_scratch_gc_root(function);
        }
        self.with_gc_inhibited(|eval| {
            for function in functions {
                let _ = eval.funcall_general(function, Vec::<Value>::new());
            }
        });
        restore_scratch_gc_roots(saved_roots);
    }

    /// Destroy the GPU objects of every shader-surface handle the
    /// just-finished cycle swept (`SurfaceObj` — see
    /// `TaggedHeap::pending_surface_destroys`). Best-effort: errors are
    /// ignored, and without a display host the batch is simply dropped (the
    /// ids were host-allocated, so no host means no GPU objects to free — the
    /// host was torn down). A handle already freed by an explicit
    /// `neomacs-surface-destroy` re-queues its id here when the dead handle
    /// is later swept; the render-thread free of a missing id is a no-op, so
    /// the double destroy is harmless.
    #[doc(hidden)]
    pub fn drain_pending_surface_destroys(&mut self) {
        let ids = self.tagged_heap.take_pending_surface_destroys();
        if ids.is_empty() {
            return;
        }
        let Some(host) = self.display_host.as_ref() else {
            return;
        };
        for id in ids {
            if let Err(err) = host.destroy_shader_surface(id) {
                tracing::debug!("gc surface destroy {id} failed: {err}");
            }
        }
    }

    /// Close every video session whose final Lisp handle was swept by the
    /// just-finished collection. As with shader surfaces, this is best-effort:
    /// losing the GUI host already tears down the renderer that owns the
    /// corresponding sessions.
    #[doc(hidden)]
    pub fn drain_pending_video_destroys(&mut self) {
        let ids = self.tagged_heap.take_pending_video_destroys();
        if ids.is_empty() {
            return;
        }
        let Some(host) = self.display_host.as_ref() else {
            return;
        };
        for id in ids {
            if let Err(err) = host.destroy_video(id) {
                tracing::debug!(video_id = id.get(), "gc video destroy failed: {err}");
            }
        }
    }

    /// Borrow VALUE's string payload for as long as the collector provably
    /// cannot run.
    ///
    /// This is the ergonomic front door to [`Value::lisp_string_in`], and the
    /// reason it lives on `Context` rather than on `Value` is the whole
    /// mechanism: the borrow it returns holds `&self`, and EVERY safepoint in
    /// this engine is a `&mut self` method on `Context`
    /// (`gc_safe_point`/`gc_safe_point_exact` here, `eval_sub`'s collect at
    /// the interpreted-eval boundary, `apply_internal`'s at the funcall
    /// boundary, `bytecode_branch_maybe_gc_and_quit` from the VM and the JIT,
    /// and `builtin_garbage_collect`). So
    ///
    /// ```ignore
    /// let s = ctx.lisp_string(v).unwrap();
    /// ctx.apply(f, args)?;   // error[E0502]: cannot borrow `*ctx` as mutable
    /// use_bytes(s.as_bytes());
    /// ```
    ///
    /// does not compile, whereas the same code written with
    /// `v.as_lisp_string()` compiles and reads freed memory. "Is this borrow
    /// held across a safepoint" becomes a compile question.
    ///
    /// When a value genuinely must survive a safepoint, this is the wrong
    /// tool: root it (DIVERGENCES.md 161/162) or copy the bytes out.
    #[inline]
    #[doc(hidden)]
    pub fn lisp_string(&self, value: Value) -> Option<&neovm_value::heap_types::LispString> {
        value.lisp_string_in(&self.tagged_heap)
    }

    /// [`Context::lisp_string`] with GNU's `CHECK_STRING` signal — the
    /// drop-in replacement for `builtins::expect_lisp_string(&args[i])?` at a
    /// site that must not hold the borrow across a safepoint.
    #[inline]
    #[doc(hidden)]
    pub fn expect_lisp_string(
        &self,
        value: Value,
    ) -> Result<&neovm_value::heap_types::LispString, Flow> {
        self.lisp_string(value).ok_or_else(|| {
            neovm_value::error::signal(
                neovm_value::error::LispCondition::WrongTypeArgument,
                vec![Value::symbol("stringp"), value],
            )
        })
    }

    /// GC safe point used at evaluator boundaries.
    pub fn gc_safe_point(&mut self) {
        self.gc_safe_point_exact();
    }

    /// Trigger a safe-point collection using only explicit evaluator roots.
    #[doc(hidden)]
    pub fn gc_safe_point_exact(&mut self) {
        crate::emacs_core::subr::leaf::debug_assert_no_leaf_active!("a GC safe point");
        if self.gc_safe_point_exact_should_collect() {
            self.gc_collect_from_current_roots();
        }
    }

    /// The concurrent mark is running, has not drained, and has not blown its
    /// allocation cap -- so this safe point has no collector work to do.
    ///
    /// Strictly narrower than the branch it front-runs: under exactly these
    /// conditions `gc_collect_from_current_roots_body` reaches its
    /// "GC thread still marking; mutator continues" return and does nothing on
    /// the way. `compact_buffers_for_gc` is already gated on no mark being in
    /// progress (`concurrent_mark_running` implies `mark_in_progress`), and the
    /// two address-keyed lexenv caches only go stale when a sweep recycles a
    /// cons -- every sweep runs inside that body, past this point.
    #[doc(hidden)]
    pub fn concurrent_mark_is_still_the_gc_thread_s(&self) -> bool {
        debug_assert_eq!(self.tagged_heap.mark_phase(), MarkPhase::ConcurrentRunning);
        if self.tagged_heap.concurrent_mark_done() {
            return false;
        }
        let cap = self.tagged_heap.gc_threshold().saturating_mul(4);
        self.tagged_heap.bytes_since_gc() <= cap
    }

    #[inline(always)]
    #[doc(hidden)]
    pub fn gc_safe_point_exact_should_collect(&mut self) -> bool {
        #[cfg(debug_assertions)]
        neovm_value::tagged::mutate::debug_assert_no_heap_mut_closure();
        // The common answer, decided from plain fields without a call: no
        // collection is in flight, none is pending or forced, and the
        // allocation pacing has not tripped. Everything else takes the
        // exact path below. These short-circuit fact reads preserve the hot
        // guard's sweep-before-mark loads; the slow path decides what the
        // complete phase means through its exhaustive MarkPhase match.
        if self.gc_inhibit_depth == 0
            && !self.tagged_heap.sweep_in_progress()
            && !self.tagged_heap.mark_in_progress()
            && !self.gc_pending
            && !self.gc_stress
            && !self.tagged_heap.gc_threshold_is_overridden()
            && !self.tagged_heap.should_collect()
        {
            return false;
        }
        self.gc_safe_point_exact_should_collect_slow()
    }

    #[inline(never)]
    #[doc(hidden)]
    pub fn gc_safe_point_exact_should_collect_slow(&mut self) -> bool {
        if self.gc_inhibit_depth > 0 {
            return false;
        }
        match self.tagged_heap.mark_phase() {
            MarkPhase::Idle => {}
            MarkPhase::Sweeping | MarkPhase::StoppedMarking => return true,
            MarkPhase::ConcurrentRunning => {
                return !self.concurrent_mark_is_still_the_gc_thread_s();
            }
            MarkPhase::PublishingSweep => {
                debug_assert!(false, "a safe point cannot reenter sweep publication");
                return true;
            }
        }
        if self.gc_pending {
            return true;
        }
        if self.gc_stress {
            // GNU `maybe_gc` only reaches collection after the consing
            // countdown crosses zero.  Stress exact GC at every boundary that
            // follows allocation, but do not spin full-heap collections across
            // boundaries where the heap has not changed.
            return self.tagged_heap.bytes_since_gc() > 0;
        }
        if self.tagged_heap.gc_threshold_is_overridden() {
            return self.tagged_heap.should_collect();
        }

        if !self.tagged_heap.should_collect() {
            return false;
        }

        // GNU's maybe_gc hot path only checks consing_until_gc and defers
        // percentage-based threshold recalculation until the countdown crosses
        // zero.  Keep Neomacs' allocation fast path in the same shape.
        // Rare path (the byte counter already crossed the current threshold):
        // re-read the Lisp settings before deciding, so a threshold raised since
        // the last GC — including one restored by unbinding a `let` — is honored
        // without waiting for a GC the raise was meant to prevent (see
        // `sync_gc_threshold_from_runtime_settings`).
        self.refresh_gc_runtime_settings_cache();
        let threshold = self.effective_gc_threshold_bytes();
        if self.tagged_heap.gc_threshold() != threshold {
            self.tagged_heap.set_gc_threshold_from_runtime(threshold);
        }
        self.tagged_heap.should_collect()
    }

    /// GNU-style quit processing used from evaluator boundaries.
    ///
    /// Mirrors `process_quit_flag` in GNU `eval.c`: clear `quit-flag`, then
    /// honor `throw-on-input`, `kill-emacs`, or signal `quit`.
    #[doc(hidden)]
    pub fn process_quit_flag(&mut self) -> Result<(), Flow> {
        let symbol_mode = self.symbol_mode();
        let flag = self.quit_flag;
        self.set_quit_flag_value(Value::NIL);

        let throw_on_input = self.variable_value_or_nil(self.throw_on_input_symbol);

        if flag.as_symbol_id(symbol_mode) == Some(self.kill_emacs_symbol) {
            // GNU keyboard.c process_quit_flag: (setq quit-flag 'kill-emacs)
            // calls Fkill_emacs, which runs the hooks and exits. Unwinding as
            // a quit signal instead would let condition-case swallow the exit.
            return super::super::builtins::symbols::builtin_kill_emacs(self, vec![]).map(|_| ());
        }

        if !throw_on_input.is_nil() && equal_value(&flag, &throw_on_input, 0) {
            tracing::debug!(
                target: "neomacs::throw_on_input",
                ?flag,
                ?throw_on_input,
                condition_stack_len = self.condition_stack.len(),
                specpdl_len = self.specpdl.len(),
                has_matching_catch = self.has_active_catch(&throw_on_input),
                "process_quit_flag: throwing for pending input"
            );
            return Err(Flow::throw_with_symbols(
                throw_on_input,
                Value::T,
                self.symbol_mode(),
            ));
        }

        Err(signal(LispCondition::Quit, vec![]))
    }

    /// The pure fast-path condition of [`Self::maybe_quit`]: true when the
    /// poll would do nothing. Loads only — no mutation, no allocation, no
    /// Lisp — so bytecode dispatch may evaluate it with its operand-stack
    /// cursor still live and only publish for the cold slow path.
    ///
    /// GNU's test is `!NILP (Vquit_flag) || pending_signals`
    /// (src/lisp.h:3897-3901), and so is this one: the Context's attention
    /// word (`quit-flag`, and a bound `throw-on-input`, whose host-input poll
    /// the slow path runs) and [`ASYNC_ATTENTION`] (the profiler tick, a
    /// handled OS signal and every raised cross-thread quit request).
    /// `inhibit-quit` is not in the fast test, as in GNU.
    #[inline(always)]
    #[doc(hidden)]
    pub fn maybe_quit_hot_ok(&self) -> bool {
        self.attention_clear(AttentionMask::QUIT)
    }

    /// GNU `maybe_quit`: promote frontend input for `throw-on-input`, then do
    /// nothing when `quit-flag` is nil or `inhibit-quit` is non-nil;
    /// otherwise process the quit request.
    ///
    /// GNU's low-level input machinery updates `quit-flag` before evaluator
    /// safe points run.  Neomacs receives ordinary frontend events through a
    /// host channel, so this semantic safe point must perform that promotion
    /// itself.  Restrict the channel poll to an active `throw-on-input`
    /// binding; the normal `maybe_quit` hot path remains a flag/atomic check.
    #[inline(always)]
    #[doc(hidden)]
    pub fn maybe_quit(&mut self) -> Result<(), Flow> {
        if self.maybe_quit_hot_ok() {
            return Ok(());
        }
        self.maybe_quit_slow()
    }

    #[cold]
    #[inline(never)]
    #[doc(hidden)]
    pub fn maybe_quit_slow(&mut self) -> Result<(), Flow> {
        crate::emacs_core::subr::leaf::debug_assert_no_leaf_active!("a quit poll");
        // GNU fatal_error_signal calls Fkill_emacs(signal-number, nil), even
        // with inhibit-quit bound. Hooks run on the Lisp owner, and Shutdown
        // unwinds to the ordinary host terminal/process cleanup boundary.
        if let Some(sig) = crate::emacs_core::eval::input_hooks::take_termination_signal()
            && self.shutdown_request.is_none()
            && match self.shutdown_phase {
                // `shut_down_emacs` has not started.
                ShutdownPhase::Running => true,
                // GNU `terminate_due_to_signal` calls `Fkill_emacs` for
                // SIGHUP, SIGTERM, and SIGINT with the first call still on
                // the stack (`emacs.c:453-460`).
                ShutdownPhase::InProgress => true,
                // `Vrun_hooks` is already nil. A later fatal signal may enter
                // `Fkill_emacs` again.
                ShutdownPhase::Dying => true,
            }
        {
            return super::super::builtins::symbols::builtin_kill_emacs(
                self,
                vec![Value::fixnum(i64::from(sig))],
            )
            .map(|_| ());
        }
        // Profiler sampling rides the quit poll (GNU samples in a SIGPROF
        // handler; SIGPROF belongs to the native profiler here, so the Lisp
        // profiler's watchdog raises `AsyncSource::ProfilerTick`, which sends
        // the safe point here). A tick is consumed first, as the fast path
        // used to, and every step below is a no-op when its own condition is
        // false, so a tick alone changes nothing else.
        self.profiler_sample_tick();
        if self.has_throw_on_input_poll_source() {
            self.poll_pending_input_for_throw_on_input()?;
        }

        // Drain the cross-thread quit-request atomic into `Vquit_flag`.
        // Set by the input-bridge thread when it observes a `quit-char`
        // keystroke while the evaluator is busy (e.g. deep in bytecode
        // and not reading from `input_rx`). See
        // `Context::quit_requested` for the design rationale.
        if self.quit_requested.is_requested()
            && self.quit_requested.take()
            && self.quit_flag.is_nil()
        {
            self.set_quit_flag_value(Value::T);
        }
        let quit_flag = self.quit_flag;
        if quit_flag.is_nil() || self.inhibit_quit.is_truthy() {
            // GNU's `probably_quit` (src/eval.c:1868-1876):
            //
            //     if (!NILP (Vquit_flag) && NILP (Vinhibit_quit))
            //       process_quit_flag ();
            //     else if (pending_signals)
            //       process_pending_signals ();
            //
            // -- an `else if`, so a pending quit wins and a pending OS signal
            // is handled only when there is none.
            if crate::emacs_core::eval::input_hooks::os_signal_pending() {
                crate::emacs_core::eval::input_hooks::drain_pending_os_signals(self)?;
            }
            return Ok(());
        }

        self.process_quit_flag()
    }

    /// The printed name of `debug-on-event`, or `None` when it does not hold a
    /// symbol.
    ///
    /// GNU's `handle_user_signal` opens with
    /// `if (SYMBOLP (Vdebug_on_event)) special_event_name = SSDATA (SYMBOL_NAME
    /// (Vdebug_on_event));` (src/keyboard.c:8492-8493) and then `strcmp`s it
    /// against the signal's `add_user_signal` NAME, so the comparison really is
    /// on the printed name and a non-symbol really does select no arm.
    #[doc(hidden)]
    pub fn debug_on_event_signal_name(&self) -> Option<String> {
        let symbol_mode = self.symbol_mode();
        let value = self.builtin_var_value(DEBUG_ON_EVENT.id())?;
        let name = value.as_symbol_lisp_string(symbol_mode)?;
        Some(neovm_value::emacs_char::to_utf8_lossy(name.as_bytes()))
    }

    /// GNU's `handle_user_signal` debugger arm, all four writes
    /// (src/keyboard.c:8500-8506):
    ///
    /// ```c
    ///   /* Enter the debugger in many ways.  */
    ///   debug_on_next_call = true;
    ///   debug_on_quit = true;
    ///   Vquit_flag = Qt;
    ///   Vinhibit_quit = Qnil;
    /// ```
    ///
    /// They are four writes and not one because they cover the three ways the
    /// debugger can be reached: the next call, the quit that is about to be
    /// signalled, and the `inhibit-quit` binding that would otherwise swallow
    /// it.
    #[doc(hidden)]
    pub fn arm_debugger_for_debug_on_event(&mut self) {
        self.set_variable("debug-on-next-call", Value::T);
        self.set_variable("debug-on-quit", Value::T);
        self.set_variable("inhibit-quit", Value::NIL);
        self.set_quit_flag_value(Value::T);
    }

    #[inline(always)]
    #[doc(hidden)]
    pub fn quit_flag_value(&self) -> Value {
        self.quit_flag
    }

    /// GNU `QUITP`: true only when a quit is pending and `inhibit-quit` is nil.
    #[inline(always)]
    #[doc(hidden)]
    pub fn quit_pending(&self) -> bool {
        !self.quit_flag.is_nil() && self.inhibit_quit.is_nil()
    }

    #[inline(always)]
    #[doc(hidden)]
    pub fn set_quit_flag_value(&mut self, value: Value) {
        self.quit_flag = value;
        self.refresh_attention();
        self.store_variable(self.quit_flag_symbol, value);
    }

    #[doc(hidden)]
    pub fn quit_char(&self) -> i64 {
        *self.quit_char
    }

    #[doc(hidden)]
    pub fn set_quit_char(&mut self, quit: i64) {
        *self.quit_char = quit & 0o377;
    }

    #[doc(hidden)]
    pub fn event_is_quit_char(&self, event: &Value) -> bool {
        event.as_fixnum() == Some(*self.quit_char)
    }

    #[doc(hidden)]
    pub fn request_quit_from_keyboard_input(&mut self) {
        if self.quit_flag_value().is_nil() {
            self.set_quit_flag_value(Value::T);
        }
    }

    #[doc(hidden)]
    pub fn clear_quit_flag_after_read_key_sequence_event(&mut self, event: &Value) {
        if !self.event_is_quit_char(event) {
            return;
        }

        let throw_on_input = self.variable_value_or_nil(self.throw_on_input_symbol);

        let quit_flag = self.quit_flag_value();
        // while-no-input is active iff `throw-on-input` is non-nil AND the
        // pending quit equals it; in that case leave BOTH the flag and the
        // atomic alone so while-no-input can still bail out.
        let is_while_no_input =
            !throw_on_input.is_nil() && equal_value(&quit_flag, &throw_on_input, 0);

        // GNU `read_char` keyboard.c:2811-2812: when the quit_char is being
        // returned as an ordinary key event, `if (!NILP (Vinhibit_quit))
        // Vquit_flag = Qnil;` — the C-g is consumed as a key, so the pending
        // quit is dropped rather than fired a second time.
        if !quit_flag.is_nil() && !is_while_no_input {
            self.set_quit_flag_value(Value::NIL);
        }

        // The cross-thread `quit_requested` atomic is the neomacs analogue of
        // the same pending C-g (the input bridge sets it in lockstep with
        // queueing the C-g KeyPress, crates/neomacs/src/main.rs:2260/2569). When
        // that very C-g is now consumed as a key here, the atomic MUST be
        // cleared too — otherwise the next `maybe_quit` poll (e.g. inside
        // pre-command-hook or the command dispatch) drains it into `quit-flag`
        // and signals a SECOND, spurious `quit`, pre-empting the
        // `keyboard-quit` command this key is bound to (the "double-quit"
        // bug). This is the root of Finding 3 and lives at the shared
        // key-consumption helper so every read path (channel or unread queue)
        // is covered. Skipped under while-no-input so its throw still fires.
        if !is_while_no_input {
            self.quit_requested.clear();
        }
    }

    #[doc(hidden)]
    pub fn input_pending_filter(&self) -> neovm_value::system_vocab::input::InputPendingFilter {
        let configured = self
            .builtin_var_value(INPUT_PENDING_FILTER_EVENTS.id())
            .unwrap_or(Value::T)
            .is_truthy();
        neovm_value::system_vocab::input::InputPendingFilter::from_filter_events_variable(
            configured,
        )
    }

    /// GNU's `track_mouse` (`DEFVAR_LISP`, `src/keyboard.c:14134`), which every
    /// terminal back-end dereferences as a bare global -- `src/term.c:3465`,
    /// `src/androidterm.c:558`, `src/haikuterm.c:425`, `src/w32fns.c:5118`.
    ///
    /// The swap-in makes that global the *current buffer's* binding whenever a
    /// buffer has localised it (dframe, dictionary and gud all do), so the read
    /// names the buffer. Ledger 196.
    #[doc(hidden)]
    pub fn track_mouse_enabled(&self) -> bool {
        // GNU DEFVAR_LISP `track-mouse`. Every pending-input query reads it;
        // the by-name spelling re-interned it at each probe.
        static TRACK_MOUSE: WellKnown = WellKnown::TrackMouse;
        self.variables()
            .value_in_buffer(self.edit.buffers.current_buffer(), TRACK_MOUSE.id())
            .unwrap_or(Value::NIL)
            .is_truthy()
    }

    #[doc(hidden)]
    pub fn should_ignore_while_no_input_symbol(&self, ignore_symbol: &str) -> bool {
        // GNU DEFVAR_LISP `while-no-input-ignore-events`, read per queried
        // event; see `track_mouse_enabled` for the fixed-spelling rationale.
        static WHILE_NO_INPUT_IGNORE_EVENTS: WellKnown = WellKnown::WhileNoInputIgnoreEvents;
        let symbol_mode = self.symbol_mode();
        let ignore_list = self
            .builtin_var_value(WHILE_NO_INPUT_IGNORE_EVENTS.id())
            .unwrap_or(Value::NIL);
        neovm_value::value::list_to_vec(&ignore_list)
            .into_iter()
            .flatten()
            .any(|value| value.is_symbol_named(ignore_symbol, symbol_mode))
    }

    #[doc(hidden)]
    pub fn has_pending_command_input_for_query(&self) -> bool {
        let filter = self.input_pending_filter();
        self.command_loop
            .keyboard
            .has_pending_command_input_for_query(
                filter,
                self.track_mouse_enabled(),
                |symbol| self.should_ignore_while_no_input_symbol(symbol),
                self.symbol_mode(),
            )
    }

    #[doc(hidden)]
    pub fn has_pending_frontend_input_with_configured_filter(&self) -> bool {
        self.has_frontend_input(super::frontend_events::FrontendInputQuery::Pending(
            neovm_value::system_vocab::input::InputPendingFilter::ConfiguredIgnoreList,
        ))
    }

    #[doc(hidden)]
    pub fn has_frontend_input(&self, query: super::frontend_events::FrontendInputQuery) -> bool {
        self.command_loop.keyboard.pending_input_events.has_input(
            query,
            self.track_mouse_enabled(),
            |symbol| self.should_ignore_while_no_input_symbol(symbol),
        )
    }

    #[doc(hidden)]
    pub fn open_channel_for_module(&mut self, process: Value) -> Result<std::ffi::c_int, Flow> {
        crate::emacs_core::eval::process_hooks::open_channel_for_module(self, process)
    }

    #[inline(always)]
    #[doc(hidden)]
    pub fn has_throw_on_input_poll_source(&self) -> bool {
        // GNU's evaluator-side `maybe_quit` is a cheap flag/signal check; the
        // input path sets `quit-flag` when real keyboard input is available.
        // Neomacs has to poll the host channel for `throw-on-input`, but only
        // when such a channel exists or the command loop is interactive.
        self.input_rx.as_ref().is_some() || !self.command_loop_noninteractive()
    }

    #[doc(hidden)]
    pub fn poll_pending_input_for_throw_on_input(&mut self) -> Result<(), Flow> {
        debug_assert!(self.has_throw_on_input_poll_source());

        if self.unwind_cleanup_depth != 0 {
            return Ok(());
        }

        let throw_on_input = self.variable_value_or_nil(self.throw_on_input_symbol);
        if throw_on_input.is_nil() {
            return Ok(());
        }

        if !self.quit_flag.is_nil() {
            return Ok(());
        }

        while self.stage_next_host_input_event_if_available()? {}

        self.service_leading_internal_frontend_events();

        if self.has_pending_frontend_input_with_configured_filter() {
            tracing::debug!(
                target: "neomacs::throw_on_input",
                ?throw_on_input,
                condition_stack_len = self.condition_stack.len(),
                specpdl_len = self.specpdl.len(),
                has_matching_catch = self.has_active_catch(&throw_on_input),
                pending_input_events = self.command_loop.keyboard.pending_input_events.len(),
                "poll_pending_input_for_throw_on_input: setting quit-flag"
            );
            self.set_quit_flag_value(throw_on_input);
        }

        Ok(())
    }

    #[doc(hidden)]
    pub fn maybe_quit_before_gc(&mut self) -> Result<(), Flow> {
        self.maybe_quit()
    }

    /// Match GNU `eval_sub` / `funcall_general`: quit check first, then GC.
    ///
    /// The remaining evaluator entry points either root their live Values
    /// explicitly or run before materializing heap-backed Values, so this path
    /// now uses exact roots rather than conservative stack scanning.
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    #[doc(hidden)]
    pub fn maybe_gc_and_quit(&mut self) -> Result<(), Flow> {
        self.maybe_quit_before_gc()?;
        if self.gc_safe_point_exact_should_collect() {
            self.gc_collect_from_current_roots();
        }
        Ok(())
    }

    /// Match GNU `bytecode.c:op_branch`: after the bytecode loop's unsigned
    /// quit counter wraps, run `maybe_gc (); maybe_quit ();`.
    #[inline(never)]
    #[doc(hidden)]
    pub fn bytecode_branch_maybe_gc_and_quit(&mut self) -> Result<(), Flow> {
        #[cfg(test)]
        BYTECODE_BRANCH_POLL_COUNT.with(|count| count.set(count.get() + 1));
        if self.gc_safe_point_exact_should_collect() {
            self.gc_collect_from_current_roots();
        }
        // Concurrent root feeding: young data reachable only from the
        // operand stacks (a loop building a list) is otherwise invisible to
        // the concurrent marker until the STW termination fold — which then
        // pays a full young-generation mark as pause. Both tiers funnel
        // through here (the interpreter's backward branch and the native
        // loop's `neovm_jit_backedge`, whose shims have already spilled live
        // values to `bc_buf` / the root window), once per ~256 iterations.
        if neovm_value::tagged::gc::concurrent_mark_active() {
            neovm_value::tagged::gc::feed_concurrent_roots(&self.bc_buf);
            neovm_value::tagged::gc::feed_concurrent_roots(
                &self.jit_root_stack[..self.jit_root_stack_top],
            );
        }
        self.maybe_quit()
    }
}

#[cfg(test)]
#[cfg(debug_assertions)]
#[path = "tests/gc_heap_mut_closure_test.rs"]
mod gc_heap_mut_closure_tests;

#[cfg(test)]
#[path = "tests/command_symbols_test.rs"]
mod command_symbols_tests;
