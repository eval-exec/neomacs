//! The command loop, mirroring GNU keyboard.c: recursive edit, command execution, pre/post-command hooks, and quit handling.
//!
//! Moved out of `eval/mod.rs` unchanged; a child module of `eval` so it keeps
//! the same view of `Context` and the parent's private items (`use super::*`).

use super::*;

impl Context {
    /// Enter a recursive edit level.
    ///
    /// Mirrors GNU Emacs `Frecursive_edit()` (keyboard.c:772).
    /// Increments recursive depth, enters the command loop, decrements on exit.
    /// If the command loop exits via `abort-recursive-edit` (throw 'exit t),
    /// signals quit.  If via `exit-recursive-edit` (throw 'exit nil), returns
    /// normally.
    ///
    /// In batch mode (no input_rx), returns nil immediately.
    /// Enter a recursive edit level (public API).
    ///
    /// Returns `Ok(())` on normal exit, `Err(description)` on error.
    #[tracing::instrument(skip_all)]
    pub fn recursive_edit(&mut self) -> Result<(), String> {
        match self.recursive_edit_inner() {
            Ok(_) => Ok(()),
            // kill-emacs unwinds the recursive edit; the pending shutdown
            // request carries the exit code to the caller.
            Err(Flow::Shutdown(_)) => Ok(()),
            Err(flow) => Err(super::super::error::format_flow_with_eval(self, &flow)),
        }
    }

    pub(crate) fn request_shutdown(&mut self, exit_code: i32, restart: bool) {
        self.shutdown_request = Some(ShutdownRequest { exit_code, restart });
        self.command_loop.running = false;
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
    pub(crate) fn lisp_execution(&self) -> LispExecution {
        match self.shutdown_request {
            None => LispExecution::Live,
            Some(_) => LispExecution::ExitedAlready,
        }
    }

    #[tracing::instrument(skip_all, fields(depth = self.command_loop.recursive_depth, has_input = self.input_rx.is_some()))]
    pub(crate) fn recursive_edit_inner(&mut self) -> EvalResult {
        self.run_exit_wrapped_command_loop(true)
    }

    #[tracing::instrument(skip_all, fields(depth = self.command_loop.recursive_depth, has_input = self.input_rx.is_some()))]
    pub(crate) fn minibuffer_command_loop_inner(&mut self) -> EvalResult {
        self.run_exit_wrapped_command_loop(false)
    }

    /// Classify the value carried by a `(throw 'exit VALUE)` that unwound a
    /// recursive command loop.
    ///
    /// Mirrors GNU `recursive_edit_1` (keyboard.c:749-758), which dispatches on
    /// the thrown value's *type* rather than its truthiness.
    pub(super) fn classify_command_loop_exit(
        &mut self,
        value: Value,
    ) -> Result<CommandLoopExit, Flow> {
        if value == Value::T {
            return Ok(CommandLoopExit::Quit);
        }
        if value.is_string() {
            return Ok(CommandLoopExit::Error(value));
        }
        if super::super::builtins::types::builtin_functionp_1(self, value)?.is_truthy() {
            return Ok(CommandLoopExit::Call(value));
        }
        Ok(CommandLoopExit::Normal)
    }

    pub(super) fn run_exit_wrapped_command_loop(&mut self, increment_depth: bool) -> EvalResult {
        // Interactive command loops need an input source. Batch mode is
        // different: GNU still runs `top-level`/`normal-top-level` and lets
        // `read_char` terminate the loop via noninteractive EOF, even when
        // there is no input channel at all.
        if self.input_rx.is_none() && !self.command_loop_noninteractive() {
            tracing::info!("recursive_edit_inner: no input receiver, returning immediately");
            return Ok(Value::NIL);
        }

        // Recursive edits and minibuffer readers enter the command loop even
        // when the outer loop has not been started through init_input_system().
        // GNU's recursive/minibuffer entry points do not consult an external
        // "running" gate before dispatching the first key. Preserve the
        // previous flag so explicit shutdown still unwinds correctly.
        let saved_running = self.command_loop.running;
        if !saved_running {
            self.command_loop.running = true;
        }

        if increment_depth {
            self.command_loop.recursive_depth += 1;
        }

        // GNU `recursive_edit_1` owns these bindings around the entire
        // `command_loop`, outside `command_loop_2`'s error/restart boundary.
        // Keeping them here also leaves execute-kbd-macro free to borrow its
        // caller's dynamic environment, as GNU macros.c does.
        let specpdl_count = self.specpdl.len();
        let result = (|| -> EvalResult {
            self.try_specbind_or_unwind_to(specpdl_count, intern("inhibit-redisplay"), Value::NIL)?;
            self.try_specbind_or_unwind_to(
                specpdl_count,
                intern("undo-auto--undoably-changed-buffers"),
                Value::NIL,
            )?;

            // GNU `command_loop` installs its `exit` catch only for a recursive
            // command loop or an active minibuffer (`command_loop_level > 0 ||
            // minibuf_level > 0`).  The outermost loop must leave `exit`
            // unmatched, so `(throw 'exit ...)` there signals `no-catch`.
            let catches_exit =
                self.recursive_command_loop_depth() > 0 || self.minibuffers.depth() > 0;
            if catches_exit {
                self.push_condition_frame(ConditionFrame::Catch {
                    tag: Value::symbol("exit"),
                    resume: ResumeTarget::CommandLoopExit,
                });
            }

            let result = self.command_loop_inner();

            if catches_exit {
                self.pop_condition_frame();
            }

            match result {
                Ok(val) => Ok(val),
                // exit-recursive-edit: throw 'exit nil → normal return
                Err(Flow::Throw(ref thrown))
                    if catches_exit && thrown.tag.is_symbol_named("exit") =>
                {
                    let value = thrown.value;
                    match self.classify_command_loop_exit(value)? {
                        // abort-recursive-edit: throw 'exit t → signal quit
                        CommandLoopExit::Quit => {
                            Err(super::super::error::signal(LispCondition::Quit, vec![]))
                        }
                        // read_minibuf's cross-window abort (minibuf.c:646).
                        CommandLoopExit::Error(message) => Err(super::super::error::signal(
                            LispCondition::Error,
                            vec![message],
                        )),
                        // minibuffer-quit-recursive-edit throws a thunk that
                        // signals `minibuffer-quit`; GNU calls it here.
                        CommandLoopExit::Call(function) => {
                            self.apply(function, vec![])?;
                            Ok(Value::NIL)
                        }
                        CommandLoopExit::Normal => Ok(Value::NIL),
                    }
                }
                Err(flow) => Err(flow),
            }
        })();
        if increment_depth {
            self.command_loop.recursive_depth -= 1;
        }
        if !saved_running {
            self.command_loop.running = false;
        }

        self.unbind_to_with_result(specpdl_count, result)
    }

    /// Inner command loop; only the outermost loop catches `top-level`.
    ///
    /// Mirrors GNU Emacs `command_loop()` (keyboard.c:1104).
    /// The outermost invocation wraps command_loop_2 in a catch for
    /// 'top-level.
    #[tracing::instrument(skip_all)]
    pub(super) fn command_loop_inner(&mut self) -> EvalResult {
        let outermost_command_loop =
            self.command_loop.recursive_depth == 1 && self.minibuffers.depth() == 0;
        loop {
            if outermost_command_loop {
                // Catch 'top-level throws (from (top-level) function).
                let top_level_tag = Value::symbol("top-level");
                self.push_condition_frame(ConditionFrame::Catch {
                    tag: top_level_tag,
                    resume: ResumeTarget::CommandLoopTopLevel,
                });
            }

            // GNU keyboard.c command_loop():
            //   internal_catch (Qtop_level, top_level_1, Qnil);
            //   internal_catch (Qtop_level, command_loop_2, Qerror);
            // Both top_level_1 and command_loop_2 run unconditionally per
            // outer loop iteration. The catch around top_level_1 turns any
            // 'top-level throw into a normal return so the next line — the
            // command_loop_2 catch — still runs. The previous NeoMacs
            // implementation gated command_loop_2 on
            // `self.command_loop.running`, which incorrectly skipped the
            // interactive loop entirely whenever (normal-top-level) raised
            // an error caught inside command_loop_top_level_1: the GUI
            // would create its window, hit the error, return Ok(NIL), and
            // immediately exit before the first redisplay. Match GNU and
            // always run command_loop_2 after top_level_1.
            let result = if outermost_command_loop {
                match self.command_loop_top_level_1() {
                    Ok(_) => self.command_loop_2(),
                    Err(Flow::Throw(ref thrown)) if thrown.tag.is_symbol_named("top-level") => {
                        // top-level throw inside top_level_1 — fall through
                        // to command_loop_2 just like GNU's two-catch flow.
                        self.command_loop_2()
                    }
                    Err(flow) => Err(flow),
                }
            } else {
                self.command_loop_2()
            };

            if outermost_command_loop {
                self.pop_condition_frame();
            }

            match result {
                // top-level throw → restart the loop
                Err(Flow::Throw(ref thrown))
                    if outermost_command_loop && thrown.tag.is_symbol_named("top-level") =>
                {
                    tracing::debug!("command_loop_inner: top-level throw, restarting loop");
                    continue;
                }
                Ok(value) if outermost_command_loop && self.command_loop_noninteractive() => {
                    // GNU keyboard.c:1145 — end of file in batch run
                    tracing::info!("command_loop_inner: noninteractive EOF, calling kill-emacs");
                    match super::super::builtins::symbols::builtin_kill_emacs(self, vec![Value::T])
                    {
                        Err(Flow::Shutdown(_)) | Ok(_) => {}
                        Err(flow) => return Err(flow),
                    }
                    return Ok(value);
                }
                // Any other result propagates up
                other => {
                    tracing::debug!(
                        "command_loop_inner: result={:?}, propagating",
                        other.is_ok()
                    );
                    return other;
                }
            }
        }
    }

    pub(super) fn command_loop_noninteractive(&self) -> bool {
        self.noninteractive
    }

    pub(super) fn command_loop_top_level_1(&mut self) -> EvalResult {
        let top_level = self
            .obarray
            .symbol_value("top-level")
            .copied()
            .unwrap_or(Value::NIL);

        tracing::debug!("command_loop_top_level_1: top-level={}", top_level);

        if top_level.is_nil() {
            tracing::debug!("command_loop_top_level_1: top-level is nil, skipping");
            self.log_startup_state("top-level-nil");
            return Ok(Value::NIL);
        }

        tracing::debug!("command_loop_top_level_1: evaluating top-level form");
        self.log_startup_state("top-level-before");
        match self.eval_value(&top_level) {
            Ok(_) => {
                tracing::debug!("command_loop_top_level_1: top-level completed OK");
                self.log_startup_state("top-level-after");
                Ok(Value::NIL)
            }
            Err(Flow::Signal(sig)) => {
                let rendered = super::super::error::format_signal_data_with_eval(self, &sig);
                tracing::warn!("command_loop_top_level_1: top-level SIGNALED: {}", rendered);
                let error_msg = self.command_error_message(&sig);
                let data = self.signal_error_data_value(&sig);
                self.report_command_error(data, "")?;
                if cfg!(test) {
                    let last_phase = self
                        .obarray
                        .symbol_value("neomacs--startup-last-phase")
                        .copied()
                        .map(|value| crate::emacs_core::print_value_with_eval(self, &value))
                        .unwrap_or_else(|| "nil".to_string());
                    let last_call = self
                        .obarray
                        .symbol_value("neomacs--startup-last-call")
                        .copied()
                        .map(|value| crate::emacs_core::print_value_with_eval(self, &value))
                        .unwrap_or_else(|| "nil".to_string());
                    eprintln!(
                        "top-level startup signal: {} last-phase={} last-call={}",
                        error_msg, last_phase, last_call
                    );
                }
                self.log_startup_state("top-level-signal");
                tracing::warn!("Top-level startup error: {}", error_msg);
                if self.command_loop_noninteractive() {
                    // GNU keyboard.c:cmd_error treats noninteractive
                    // startup/eval errors as fatal: it prints the error and
                    // calls (kill-emacs -1), which exits with status 255.
                    self.request_shutdown(-1, false);
                    return Err(Flow::Shutdown(ShutdownRequest {
                        exit_code: -1,
                        restart: false,
                    }));
                }
                Ok(Value::NIL)
            }
            Err(flow) => Err(flow),
        }
    }

    pub(super) fn trace_startup_state_enabled(&self) -> bool {
        std::env::var("NEOMACS_TRACE_STARTUP_STATE")
            .ok()
            .is_some_and(|value| value == "1")
    }

    pub(super) fn log_startup_state(&self, phase: &str) {
        if !self.trace_startup_state_enabled() {
            return;
        }

        let current_buffer = self
            .buffers
            .current_buffer()
            .map(|buffer| buffer.name_runtime_string_owned())
            .unwrap_or_else(|| "<none>".to_string());
        let selected_frame = self.frames.selected_frame().map(|frame| {
            let selected_window_buffer = frame
                .selected_window()
                .and_then(|window| window.buffer_id())
                .and_then(|buffer_id| self.buffers.get(buffer_id))
                .map(|buffer| buffer.name_runtime_string_owned())
                .unwrap_or_else(|| "<missing>".to_string());
            format!(
                "id=0x{:x} size={}x{} selected-window=0x{:x} selected-window-buffer={}",
                frame.id.0,
                frame.width,
                frame.height,
                frame.selected_window.0,
                selected_window_buffer
            )
        });
        let frames = self
            .frames
            .frame_list()
            .into_iter()
            .map(|fid| format!("0x{:x}", fid.0))
            .collect::<Vec<_>>();

        tracing::info!(
            "startup-state phase={} command-line-args={} command-line-args-left={} command-line-processed={} window-system={} initial-window-system={} current-buffer={} selected-frame={:?} frames={:?}",
            phase,
            format_startup_value(self.obarray.symbol_value("command-line-args")),
            format_startup_value(self.obarray.symbol_value("command-line-args-left")),
            format_startup_value(self.obarray.symbol_value("command-line-processed")),
            format_startup_value(self.obarray.symbol_value("window-system")),
            format_startup_value(self.obarray.symbol_value("initial-window-system")),
            current_buffer,
            selected_frame,
            frames
        );
    }

    /// Command loop with error recovery.
    ///
    /// Mirrors GNU Emacs `command_loop_2()` (keyboard.c:1146).
    /// Wraps command_loop_1 with condition-case error handling.
    #[tracing::instrument(skip_all)]
    pub(super) fn command_loop_2(&mut self) -> EvalResult {
        loop {
            match self.command_loop_1() {
                Ok(val) => return Ok(val),
                Err(flow @ Flow::Throw(_)) => {
                    // Throws propagate (exit, top-level, etc.) without
                    // re-entering the command loop.  Re-running command_loop_1
                    // here traps minibuffer exit throws and blocks waiting for
                    // another key instead of unwinding like GNU Emacs.
                    return Err(flow);
                }
                // A shutdown unwinds the command loop instead of restarting it:
                // GNU never returns from Fkill_emacs to command_loop_2.
                Err(flow @ (Flow::ThreadBlocked(_) | Flow::Shutdown(_))) => return Err(flow),
                Err(flow @ Flow::Signal(_))
                    if self
                        .command_loop
                        .keyboard
                        .kboard
                        .executing_kbd_macro
                        .is_some() =>
                {
                    return Err(flow);
                }
                Err(Flow::Signal(sig)) => {
                    // GNU `command_loop_2' is the sole recovery owner:
                    // `internal_condition_case (command_loop_1, ..., cmd_error)'.
                    // Keeping reporting here ensures the current buffer's
                    // buffer-local `command-error-function' decides how the
                    // error is presented (notably `minibuffer-error-function').
                    // Capture every diagnostic decision and value before
                    // arbitrary presentation Lisp can mutate editor state.
                    let diagnostic = self.capture_command_loop_diagnostic(&sig);
                    // GNU `cmd_error' clears both prefix arguments and key
                    // echoing before calling `cmd_error_internal'.
                    self.assign("prefix-arg", Value::NIL);
                    self.assign("last-prefix-arg", Value::NIL);
                    self.cancel_key_echo_state();

                    let data = self.signal_error_data_value(&sig);
                    self.report_command_error(data, "")?;

                    // GNU only ever shows the message; the log is this port's
                    // diagnostic, so it follows GNU's own ranking of signals
                    // (see `command_error_severity'): a quit or a
                    // `debug-ignored-errors' match is what the user just did,
                    // not an error.
                    diagnostic.emit();

                    // Restart the command loop.
                    continue;
                }
            }
        }
    }

    /// Render a signal for diagnostics without choosing a presentation path.
    /// Presentation belongs exclusively to `report_command_error', which
    /// dispatches through Lisp's buffer-local `command-error-function'.
    pub(super) fn command_error_message(&mut self, sig: &SignalData) -> String {
        let error_data = make_signal_binding_value(sig);
        crate::emacs_core::errors::builtin_error_message_string(self, vec![error_data])
            .ok()
            .and_then(|value| {
                value
                    .as_lisp_string()
                    .map(|ls| crate::emacs_core::emacs_char::to_utf8_lossy(ls.as_bytes()))
            })
            .unwrap_or_else(|| format_symbol_name_for_diagnostic(sig.symbol))
    }

    /// Freeze a command-loop diagnostic before `command-error-function' runs.
    ///
    /// GNU makes its debugger-ignore decision during signal dispatch, before
    /// `cmd_error_internal' calls the presentation hook.  Keeping severity and
    /// rendered values in one owned record makes that ordering explicit and
    /// prevents later Lisp state changes from altering the event in flight.
    pub(super) fn capture_command_loop_diagnostic(
        &mut self,
        sig: &SignalData,
    ) -> CommandLoopDiagnostic {
        CommandLoopDiagnostic {
            severity: self.command_error_severity(sig),
            condition: format_symbol_name_for_diagnostic(sig.symbol),
            message: self.command_error_message(sig),
            signal: super::super::error::format_signal_data_with_eval(self, sig),
            backtrace: self
                .last_uncaught_signal_backtrace
                .take()
                .unwrap_or_default(),
        }
    }

    /// Main command loop — read key sequence, look up binding, execute.
    ///
    /// Mirrors GNU Emacs `command_loop_1()` (keyboard.c:1306).
    /// This is the core interactive loop: read → dispatch → redisplay.
    #[tracing::instrument(skip_all)]
    pub(super) fn command_loop_1(&mut self) -> EvalResult {
        if !self.command_loop.running {
            return Ok(Value::NIL);
        }

        self.command_loop_1_entry_prologue()?;

        loop {
            if !self.command_loop.running {
                return Ok(Value::NIL);
            }

            self.flush_pending_safe_funcalls();
            self.sync_current_buffer_to_selected_window();

            // Save the outgoing `current-prefix-arg` into
            // `last-prefix-arg` before reading the next command.
            //
            // Do NOT also transfer `prefix-arg` here: GNU's Lisp
            // `command-execute` does that itself right before it
            // calls `call-interactively`, and prefix commands such as
            // `universal-argument` rely on `prefix-arg` surviving
            // until that point.
            let outgoing_prefix_arg = self.eval_symbol("current-prefix-arg").unwrap_or(Value::NIL);
            self.assign("last-prefix-arg", outgoing_prefix_arg);

            // Reset this-command and related variables before reading
            // the next key sequence.  GNU keyboard.c:1416-1419 clears
            // Vthis_command, Vreal_this_command, Vthis_original_command,
            // and Vthis_command_keys_shift_translated to nil so that idle
            // timer callbacks (e.g. which-key) running inside
            // read_key_sequence observe (null this-command) => t.
            self.assign("this-command", Value::NIL);
            self.assign("real-this-command", Value::NIL);
            self.assign("this-original-command", Value::NIL);

            // Read a complete key sequence (may be multi-key, e.g. C-x C-f).
            //
            // Bind `inhibit-quit` to t around the command-loop read, the way
            // GNU `command_loop_1` keeps C-g out of the quit machinery while
            // reading the next key (keyboard.c binds Qinhibit_quit around the
            // input wait, and `read_char` clears `Vquit_flag` when the
            // quit_char is returned as a key, keyboard.c:2811-2812). Without
            // this, neomacs's per-iteration `maybe_quit` in the wait loop
            // (process/wait.rs) would observe the cross-thread `quit_requested`
            // atomic an idle C-g raises and signal `quit` DIRECTLY — bypassing
            // the `keyboard-quit` command the C-g is bound to (so advice and
            // remaps never run) and leaving the C-g KeyPress queued for a
            // second quit. With `inhibit-quit` bound, `maybe_quit` returns Ok,
            // the C-g flows through as an ordinary key, and
            // `read_key_sequence` returns it bound to `keyboard-quit`.
            //
            // This binding is scoped strictly to the command-loop read.
            // `sleep-for` / `accept-process-output` run as commands (outside
            // this binding) and bind no `inhibit-quit`, so their waits stay
            // interruptible by C-g — the sleep-for quit fix is preserved.
            let read_specpdl_count = self.specpdl.len();
            self.try_specbind_or_unwind_to(read_specpdl_count, intern("inhibit-quit"), Value::T)?;
            let read_result = self.read_command_key_sequence_with_options(
                crate::keyboard::ReadKeySequenceOptions::new(Value::NIL, false, false, true),
            );

            // The read result itself is not an EvalResult, but it can carry
            // Lisp Values just like one. Keep those values in the VM root
            // window while an `inhibit-quit` unlet watcher runs arbitrary
            // Lisp during cleanup. Error payloads travel through the ordinary
            // result-carrying unwinder.
            let read_root_scope = self.save_vm_roots();
            if let Ok(crate::keyboard::CommandKeySequenceRead::Command { keys, binding }) =
                &read_result
            {
                for key in keys.iter().copied() {
                    self.push_vm_frame_root(key);
                }
                self.push_vm_frame_root(*binding);
            }
            let unwind_result = match read_result.as_ref() {
                Ok(_) => self.unbind_to_with_result(read_specpdl_count, Ok(Value::NIL)),
                Err(flow) => self.unbind_to_with_result(read_specpdl_count, Err(flow.clone())),
            };
            self.restore_vm_roots(read_root_scope);
            unwind_result?;

            let (keys, binding, input_end) = match read_result? {
                crate::keyboard::CommandKeySequenceRead::Command { keys, binding } => {
                    (keys, binding, None)
                }
                crate::keyboard::CommandKeySequenceRead::End(end) => {
                    (Vec::new(), Value::NIL, Some(end))
                }
            };

            // Reconcile a quit that became pending DURING the command-loop read.
            //
            // The input bridge raises the cross-thread `quit_requested` atomic
            // EAGERLY the instant it sees a C-g in the byte stream — even while
            // earlier keystrokes are still queued AHEAD of that C-g on the
            // ordered input channel (crates/neomacs/src/main.rs:2260; the atomic
            // is set ONLY for the quit char). With `inhibit-quit` bound around
            // the read above, a `maybe_quit` during the wait drains that eager
            // atomic into `quit-flag` while an EARLIER key is being read, so on
            // return `quit-flag` can be set even though the C-g itself has not
            // been read yet.
            //
            // GNU has no such eager cross-thread atomic: its `Vquit_flag` comes
            // from the SIGINT handler and the quit_char arrives in-stream;
            // `read_char` clears `Vquit_flag` exactly when it returns the
            // quit_char as a key under `inhibit-quit` (keyboard.c:2810-2811),
            // and the residual-quit -> `unread-command-events = (quit_char)`
            // conversion runs ONLY where the input WAIT returned no key (after
            // `sit_for` showing a minibuffer message, keyboard.c:1409-1416) —
            // never after an ordinary key read.
            //
            // We mirror that, accounting for the eager atomic:
            //
            //  * If a quit is pending, CLEAR `quit-flag` and the atomic. The
            //    pending quit corresponds to a C-g; either it was just returned
            //    as the read key (lone C-g -> `keys` is the C-g, bound to
            //    `keyboard-quit`, run below), or it is still queued IN-STREAM
            //    behind keys the bridge sent ahead of it and will be read as an
            //    ordinary key on a later iteration. Leaving `quit-flag` set
            //    would fire a spurious quit at the next `maybe_quit` (e.g. mid
            //    self-insert), aborting a minibuffer read partway and leaking
            //    the remaining keys into the buffer (the `megaalpha` bug).
            //
            //  * Re-deliver the C-g via `unread-command-events` ONLY when the
            //    read returned NO key — the genuine GNU case where a quit
            //    interrupted a wait with nothing else queued, so the C-g must
            //    become the next key exactly once. When a real key was read the
            //    in-stream C-g is still coming, so injecting a quit_char here
            //    would deliver the quit OUT OF ORDER, ahead of the queued keys.
            //
            // This keeps the single-idle-C-g fix intact: a lone C-g is read as
            // a key bound to `keyboard-quit` (run below, exactly once) and the
            // flag/atomic are cleared here; `sleep-for`/`accept-process-output`
            // run as commands outside the read's `inhibit-quit` binding and
            // stay C-g-interruptible.
            //
            // `while-no-input` is left untouched: when `quit-flag` equals
            // `throw-on-input` the pending value is while-no-input's bail-out
            // sentinel (NOT an eager C-g), so clearing it would defeat
            // while-no-input — mirror the same guard used by
            // `clear_quit_flag_after_read_key_sequence_event`.
            let throw_on_input = self
                .obarray
                .symbol_value_id_or_nil(self.throw_on_input_symbol);
            let quit_flag = self.quit_flag_value();
            let is_while_no_input =
                !throw_on_input.is_nil() && equal_value(&quit_flag, &throw_on_input, 0);
            let quit_pending = !quit_flag.is_nil()
                || self
                    .quit_requested
                    .load(std::sync::atomic::Ordering::Relaxed);
            if quit_pending && !is_while_no_input {
                self.set_quit_flag_value(Value::NIL);
                self.quit_requested
                    .store(false, std::sync::atomic::Ordering::Relaxed);
                if keys.is_empty() {
                    let quit_char = Value::fixnum(self.quit_char());
                    self.push_unread_command_event(quit_char);
                }
            }

            self.sync_current_buffer_to_selected_window();

            if input_end.is_some() {
                self.assign("this-command", Value::NIL);
                return Ok(Value::NIL);
            }

            // Start only after the complete key sequence has arrived.  This
            // excludes the user's inter-key think time while retaining the
            // command-loop work below, including command remapping.
            let command_observation_start = UserCommandObservationStart::capture_if_enabled();

            // A non-empty key sequence with a nil binding is a truly-unbound
            // key. GNU `command_loop_1` does NOT short-circuit this case: it
            // sets `Vthis_command = cmd` (= nil) at keyboard.c:1506, runs
            // `pre-command-hook` (1509), then `if (NILP (Vthis_command))
            // call0 (Qundefined);` (1512-1514) — the `undefined` command in
            // subr.el dings and echoes "<key> is undefined" — and finally
            // runs `post-command-hook` (1563) plus the deactivate-mark /
            // recent-keys bookkeeping like any other command. Routing the
            // nil-binding case through the SAME finalize tail below (rather
            // than a bare `continue`) restores those per-command hooks and
            // the user-visible "is undefined" feedback. Keyboard/command-loop
            // audit Finding 1. (The `undefined` command itself sets
            // `prefix-arg`, so we no longer reset it here.)
            if binding.is_nil() {
                let desc: Vec<String> = keys.iter().map(|v| format!("{:?}", v)).collect();
                tracing::info!("Undefined key sequence: {}", desc.join(" "));
            }

            // The unmapped command (real-this-command) is the binding
            // we read from the keymap, before any remapping is applied.
            // Do not touch `real-last-command` here: GNU updates it only after
            // the preceding command's `post-command-hook` and leaves that
            // value visible to the next `pre-command-hook` (keyboard.c's
            // `kset_real_last_command` near the command-loop finalize tail).
            self.assign("real-this-command", binding);

            // Apply command remapping per GNU
            // `keyboard.c:1340-1343`. The remapped command becomes
            // this-command for execution. Finding 4.
            let remapped = self.command_remapping_for_loop(binding);
            self.assign("this-command", remapped);
            let mut command_observation = command_observation_start.map(|start| {
                UserCommandObservation::begin(
                    start,
                    UserCommandIdentity::new(
                        self.context_instance_id(),
                        self.command_loop.recursive_depth,
                        Self::command_keys_for_log(&keys),
                        Self::command_value_for_log(binding),
                        Self::command_value_for_log(remapped),
                        self.frames.selected_frame().map(|frame| frame.id),
                        self.buffers.current_buffer_id(),
                    ),
                )
            });

            // Finding 2: this-original-command stays at the original
            // (pre-remap) command for the duration of the iteration
            // unless a pre-command-hook explicitly cleared it.
            if self
                .eval_symbol("this-original-command")
                .unwrap_or(Value::NIL)
                .is_nil()
            {
                self.assign("this-original-command", binding);
            }

            if let Some(last) = keys.last() {
                self.assign("last-command-event", *last);
            }
            tracing::debug!(
                "command_loop_1: binding={} current_buffer={:?} active_minibuffer_window={:?}",
                self.this_command_name_for_log(),
                self.buffers.current_buffer_id(),
                self.active_minibuffer_window
            );

            // GNU `command_loop_1` resets `Vdeactivate_mark = Qnil` at the top
            // of each iteration (keyboard.c:1471), before `pre-command-hook`, so
            // the flag reflects only the command about to run; the post-command
            // block then deactivates the region iff a command (re)set it.
            // Without this per-command reset a stale buffer-local `deactivate-mark`
            // (left by an earlier buffer-modifying command such as self-insert)
            // leaks forward and immediately kills a freshly `set-mark`ed region,
            // so e.g. `C-SPC M-> M-;` sees no active region.
            self.assign("deactivate-mark", Value::NIL);

            // GNU `keyboard.c:1500-1506` records the command pseudo-event
            // before `pre-command-hook`, so `recent-keys 'include-cmds` can
            // describe the command currently being run.
            self.record_recent_command(remapped);

            // Run pre-command-hook via safe-run-hooks so a broken
            // hook function is removed instead of re-firing on every
            // command. Finding 7 — GNU `keyboard.c:1510`
            // (`safe_run_hooks_maybe_narrowed (Qpre_command_hook, ...)`).
            self.safe_run_hook_if_bound("pre-command-hook")?;

            // GNU `keyboard.c:1530-1534` adds undo boundaries here, after
            // `pre-command-hook` and before command execution, so the
            // previous command's edits are grouped before the next command
            // mutates any buffer state.
            if self.obarray.fboundp("undo-auto--add-boundary") {
                let _ = self.apply(Value::symbol("undo-auto--add-boundary"), vec![]);
            }
            if let Some(current_id) = self.buffers.current_buffer_id() {
                let _ = self.buffers.record_undo_point_before_command(current_id);
            }

            // GNU `keyboard.c:1477-1486` snapshots prev-buffer/modiff and
            // `last_point_position = PT` here, then resets
            // `disable-point-adjustment` to nil so a command must opt back in
            // to suppress the post-command point adjustment.
            let apfp_prev_buffer = self.buffers.current_buffer_id();
            let apfp_last_pt = apfp_prev_buffer.map(|id| self.apfp_point(id)).unwrap_or(0);
            let apfp_prev_modiff = apfp_prev_buffer
                .and_then(|id| self.buffers.get(id))
                .map(|b| b.modified_tick())
                .unwrap_or(0);
            self.assign("disable-point-adjustment", Value::NIL);

            // Execute the remapped command, matching GNU's
            // `calln (Qcommand_execute, Vthis_command)`.
            let command_execution_start = command_observation
                .as_ref()
                .map(UserCommandObservation::begin_execution);
            let exec_result = self.dispatch_command_in_loop(remapped);
            if let (Some(observation), Some(start)) =
                (command_observation.as_mut(), command_execution_start)
            {
                let outcome = match &exec_result {
                    Ok(_) => UserCommandOutcome::Completed,
                    Err(flow) => UserCommandOutcome::from_flow(flow),
                };
                observation.finish_execution(start, outcome);
            }

            // Keep the selected window's point and current buffer/runtime view
            // aligned before post-command work and redisplay observe state.
            self.sync_current_buffer_to_selected_window();

            // GNU does not recover inside `command_loop_1'. Any non-local
            // result unwinds the unfinished command (so post-command hooks and
            // history finalization do not run) and lets `command_loop_2' make
            // the exhaustive Flow decision. In particular, a Signal must not
            // be flattened into a plain echo-area `message' here.
            if exec_result.is_err() {
                return exec_result;
            }

            // Run post-command-hook via safe-run-hooks (Finding 7).
            // GNU `command_loop_1` calls `safe_run_hooks (Qpost_command_hook)`
            // at keyboard.c:1563.
            self.safe_run_hook_if_bound("post-command-hook")?;

            // GNU `command_loop_1` (src/keyboard.c:1342-1345): "If displaying a
            // message, resize the echo area window to fit that message's size
            // exactly." It calls `resize_echo_area_exactly` whenever
            // `echo_area_buffer[0]` is non-nil; that passes
            // `exact_p = (minibuf_level == 0 ? Qt : Qnil)` (xdisp.c:13235) so
            // with NO active minibuffer the grow-only echo window shrinks to
            // fit even a shorter NON-EMPTY message (xdisp.c:13401). We can't
            // resize the mini-window here (geometry is computed lazily in the
            // layout engine), so we record the request and the next redisplay's
            // layout pass consumes it. `minibuf_level == 0` maps to "no active
            // minibuffer window".
            self.echo_area_resize_exact_pending =
                self.current_message.is_some() && self.active_minibuffer_window_id().is_none();

            // GNU runs the deactivate-mark / select-active-regions block
            // strictly AFTER post-command-hook: keyboard.c:1597-1648, with
            // `call0 (Qdeactivate_mark)` at 1611. (The earlier
            // `Vdeactivate_mark = Qnil` at keyboard.c:1471/1490 is only the
            // pre-command RESET of the flag, not the deactivation.) So a
            // command that sets `deactivate-mark` must still observe an
            // active region from inside `post-command-hook`. Finding —
            // keyboard/command-loop audit.
            let _ = self.update_active_region_selection_after_command();

            // GNU `keyboard.c:1650-1671` finalize block: adjust point out of
            // invisible/intangible text after the command.  Gated like GNU on
            // same-buffer, the selected window showing that buffer, point
            // having actually moved, and neither disable var being set.
            {
                let cur_buffer = self.buffers.current_buffer_id();
                let win_buffer = self
                    .frames
                    .selected_frame()
                    .and_then(|f| f.selected_window())
                    .and_then(|w| w.buffer_id());
                let cur_pt = cur_buffer.map(|id| self.apfp_point(id)).unwrap_or(0);
                let disabled = self
                    .eval_symbol("disable-point-adjustment")
                    .unwrap_or(Value::NIL)
                    .is_truthy()
                    || self
                        .eval_symbol("global-disable-point-adjustment")
                        .unwrap_or(Value::NIL)
                        .is_truthy();
                if cur_buffer.is_some()
                    && cur_buffer == apfp_prev_buffer
                    && cur_buffer == win_buffer
                    && apfp_last_pt != cur_pt
                    && !disabled
                {
                    let modified = cur_buffer
                        .and_then(|id| self.buffers.get(id))
                        .map(|b| b.modified_tick())
                        .unwrap_or(apfp_prev_modiff)
                        != apfp_prev_modiff;
                    self.adjust_point_for_property(apfp_last_pt, modified)?;
                    // Re-align the selected window with the adjusted point.
                    self.sync_current_buffer_to_selected_window();
                }
            }

            // GNU updates the command-history variables after
            // post-command-hook (`keyboard.c`: kset_last_command and
            // kset_real_last_command near the bottom of command_loop_1).
            // Undo uses `last-command` to decide whether a following undo
            // continues the same undo chain or starts a redo.
            if let Ok(this_cmd) = self.eval_symbol("this-command") {
                self.assign("last-command", this_cmd);
            }
            let real_this = self.eval_symbol("real-this-command").unwrap_or(Value::NIL);
            self.assign("real-last-command", real_this);

            // GNU records the real command as last-repeatable-command for
            // ordinary key events.
            let last_event = self.eval_symbol("last-command-event").unwrap_or(Value::NIL);
            if !last_event.is_cons() {
                self.assign("last-repeatable-command", real_this);
            }

            // Reset this-original-command for the next iteration so
            // a fresh command starts the cycle clean (mirroring
            // GNU's clear at the bottom of command_loop_1).
            self.assign("this-original-command", Value::NIL);

            if exec_result.is_ok()
                && self.command_loop.keyboard.kboard.defining_kbd_macro
                && self
                    .eval_symbol("prefix-arg")
                    .unwrap_or(Value::NIL)
                    .is_nil()
            {
                self.finalize_kbd_macro_runtime_chars();
            }

            // GNU `command_loop_1` calls `cancel_echoing` at the command
            // boundary: the rendered key sequence remains visible, but the
            // next ordinary input no longer treats it as keyboard-owned and
            // cannot append another command's events to it.
            self.cancel_key_echo_state();

            // Keyboard audit Finding 9: auto-save-interval check.
            // GNU `keyboard.c:1491-1506`:
            //
            //   if (INTEGERP (Vauto_save_interval)
            //       && num_nonmacro_input_events - last_auto_save
            //          > max (XFIXNUM (Vauto_save_interval), 20)
            //       && !detect_input_pending_run_timers (0))
            //     {
            //       Fdo_auto_save (Qnil, Qnil);
            //       last_auto_save = num_nonmacro_input_events;
            //       ...
            //     }
            //
            // The lower floor of 20 prevents saving too often if
            // a user sets `auto-save-interval` to a tiny value.
            // The `detect_input_pending` gate defers the save
            // when the user is typing faster than the check
            // interval — we approximate that with a "no pending
            // events in the unread queue" probe.
            self.command_loop_1_maybe_auto_save();
            if let Some(observation) = command_observation.as_mut() {
                observation.complete_finalization();
            }
        }
    }

    /// One-time entry prologue for `command_loop_1`.
    ///
    /// GNU `keyboard.c:1313-1349` runs this before the first
    /// `read_key_sequence` after entering `command_loop_1`, not after the
    /// first command. Doom relies on that ordering: it sets
    /// `inhibit-redisplay` during startup and clears it from an initial
    /// `post-command-hook` before the first input wait/redisplay.
    pub(super) fn command_loop_1_entry_prologue(&mut self) -> EvalResult {
        self.assign("prefix-arg", Value::NIL);
        self.assign("last-prefix-arg", Value::NIL);
        self.assign("deactivate-mark", Value::NIL);

        // GNU `command_loop_1` clears `this_command_key_count` and
        // `this_single_command_key_start` before its initial
        // `post-command-hook` (keyboard.c:1316-1327).  In a recursive
        // minibuffer command loop, the outer command's translated key
        // sequence is therefore hidden from that hook.  Keep the raw sequence:
        // GNU does not clear `raw_keybuf_count` until immediately before
        // `read_key_sequence` (keyboard.c:1416-1424).
        self.set_translated_command_keys(Vec::new());

        if self
            .eval_symbol("memory-full")
            .unwrap_or(Value::NIL)
            .is_nil()
        {
            self.safe_run_hook_if_bound("post-command-hook")?;

            if self
                .eval_symbol("delayed-warnings-list")
                .unwrap_or(Value::NIL)
                .is_truthy()
            {
                self.safe_run_hook_if_bound("delayed-warnings-hook")?;
            }
        }

        let this_command = self.eval_symbol("this-command").unwrap_or(Value::NIL);
        self.assign("last-command", this_command);

        let real_this_command = self.eval_symbol("real-this-command").unwrap_or(Value::NIL);
        self.assign("real-last-command", real_this_command);

        let last_command_event = self.eval_symbol("last-command-event").unwrap_or(Value::NIL);
        if !last_command_event.is_cons() {
            self.assign("last-repeatable-command", real_this_command);
        }

        Ok(Value::NIL)
    }

    /// Per-iteration `auto-save-interval` check, mirroring GNU
    /// `keyboard.c:1491-1506`. Keyboard audit Finding 9.
    pub(super) fn command_loop_1_maybe_auto_save(&mut self) {
        let interval = match self.eval_symbol("auto-save-interval").ok() {
            Some(v) => match v.as_fixnum() {
                Some(n) if n > 0 => n,
                _ => return,
            },
            None => return,
        };
        let threshold = interval.max(20);
        let current = self.num_nonmacro_input_events();
        let last = self.command_loop.last_auto_save_input_events;
        if current.saturating_sub(last) <= threshold {
            return;
        }
        // Defer if input is pending (same spirit as GNU's
        // `detect_input_pending_run_timers (0)` gate). A fast
        // typist should not be interrupted by a save.
        if self.input_pending_for_auto_save() {
            return;
        }
        self.run_command_loop_auto_save("input interval");
    }

    /// Run GNU's command-input auto-save boundary for either the event-count
    /// or idle-time trigger. Both paths must pass `auto-save-no-message`, call
    /// the same `do-auto-save` primitive, and throttle a failing attempt so a
    /// broken hook cannot spin the command loop.
    pub(crate) fn run_command_loop_auto_save(&mut self, trigger: &'static str) {
        self.command_loop.last_auto_save_input_events = self.num_nonmacro_input_events();
        let no_message = if self
            .eval_symbol("auto-save-no-message")
            .unwrap_or(Value::NIL)
            .is_truthy()
        {
            Value::T
        } else {
            Value::NIL
        };
        if let Err(flow) = self.apply(Value::symbol("do-auto-save"), vec![no_message, Value::NIL]) {
            let rendered = super::super::error::format_flow_with_eval(self, &flow);
            tracing::warn!("auto-save from {trigger} failed: {rendered}");
        }
    }

    /// Approximation of GNU `detect_input_pending_run_timers (0)`
    /// used by the command-loop auto-save gate. Returns true when
    /// there is already-queued input that should run before an
    /// expensive auto-save.
    pub(super) fn input_pending_for_auto_save(&mut self) -> bool {
        self.service_leading_internal_frontend_events();
        if self.peek_unread_command_event().is_some() {
            return true;
        }
        self.has_pending_frontend_input_with_configured_filter()
    }

    /// Apply `command-remapping` for the command-loop dispatch
    /// path. Mirrors GNU `keyboard.c:1340-1343` calling
    /// `Fcommand_remapping (cmd, Qnil, Qnil)` and substituting the
    /// result when non-nil. Keyboard audit Finding 4.
    pub(super) fn command_remapping_for_loop(&mut self, command: Value) -> Value {
        if command.is_nil() {
            return command;
        }
        match self.apply(Value::symbol("command-remapping"), vec![command]) {
            Ok(remapped) if !remapped.is_nil() => remapped,
            _ => command,
        }
    }

    /// Dispatch the current `this-command` via GNU's
    /// `command-execute` command-loop path.
    pub(super) fn dispatch_command_in_loop(&mut self, command: Value) -> EvalResult {
        // Re-resolve `this-command` from the obarray so a
        // pre-command-hook that mutated the symbol takes effect.
        let cmd = self.eval_symbol("this-command").unwrap_or(command);
        if cmd.is_nil() {
            // GNU `command_loop_1` keyboard.c:1512-1514:
            //   if (NILP (Vthis_command))
            //     /* nil means key is undefined.  */
            //     call0 (Qundefined);
            // The `undefined` command (subr.el) dings, echoes
            // "<key> is undefined", forces a mode-line update, and sets
            // `prefix-arg` for down-mouse events. Invoke it so an unbound
            // key gives the same feedback as GNU instead of silently doing
            // nothing. If `undefined` is not yet defined (minimal runtimes),
            // fall back to a bare ding so the key is still audible.
            if self.obarray.fboundp("undefined") {
                return self.apply(Value::symbol("undefined"), vec![]);
            }
            let _ = super::super::builtins::dispatch_builtin(self, "ding", vec![]);
            return Ok(Value::NIL);
        }
        self.apply(Value::symbol("command-execute"), vec![cmd])
    }

    /// Run a hook with `safe-run-hooks` semantics: each hook
    /// function is wrapped in a `condition-case` so a broken
    /// function is removed from the hook instead of re-firing on
    /// every subsequent command. Mirrors GNU
    /// `safe_run_hooks (Qhook_name)` at
    /// `src/keyboard.c:1361,1485` and `src/eval.c:2779-2830`.
    /// Keyboard audit Finding 7.
    pub(crate) fn safe_run_hook_if_bound(&mut self, hook_name: &str) -> EvalResult {
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
        let hook_sym = super::super::intern::intern(hook_name);
        // `safe_run_hook_funcall` only swallows ordinary `error`
        // signals.  Nonlocal exits like `throw`/`quit` still escape
        // the command loop, and `read-char-from-minibuffer` relies on
        // that when its local `post-command-hook` calls
        // `exit-minibuffer`.
        let specpdl_count = self.specpdl.len();
        self.try_specbind_or_unwind_to(specpdl_count, intern("inhibit-quit"), Value::T)?;
        let result = super::super::hook_runtime::safe_run_named_hook(self, hook_sym, &[]);
        self.unbind_to_with_result(specpdl_count, result)
    }

    pub(crate) fn execute_kbd_macro_iteration_via_command_loop(&mut self) -> EvalResult {
        let saved_running = self.command_loop.running;
        if !saved_running {
            self.command_loop.running = true;
        }
        self.assign("prefix-arg", Value::NIL);
        let result = self.command_loop_2();
        if !saved_running && self.command_loop.running {
            self.command_loop.running = false;
        }
        result
    }

    pub(crate) fn with_executing_kbd_macro_runtime<F>(
        &mut self,
        macro_events: Vec<Value>,
        run: F,
    ) -> EvalResult
    where
        F: FnOnce(&mut Self) -> EvalResult,
    {
        let scope = ExecutingKbdMacroRuntimeScope {
            snapshot: self.snapshot_executing_kbd_macro_runtime(),
            real_this_command: self.eval_symbol("real-this-command").unwrap_or(Value::NIL),
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

    pub(crate) fn reset_executing_kbd_macro_runtime_iteration(&mut self) {
        self.set_executing_kbd_macro_runtime_index(0);
    }

    pub(super) fn finish_executing_kbd_macro_runtime_scope(
        &mut self,
        scope: ExecutingKbdMacroRuntimeScope,
    ) -> EvalResult {
        self.restore_executing_kbd_macro_runtime(scope.snapshot);
        self.assign("real-this-command", scope.real_this_command);
        self.run_hook_if_bound("kbd-macro-termination-hook")
    }

    /// Run a named hook if it is bound and non-nil.
    pub(crate) fn run_hook_if_bound(&mut self, hook_name: &str) -> EvalResult {
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

    pub(crate) fn queue_pending_safe_funcall(&mut self, function: Value, args: Vec<Value>) {
        self.pending_safe_funcalls.push(PendingSafeFuncall {
            function,
            args: args.into_iter().collect(),
        });
    }

    pub(crate) fn queue_pending_safe_hook(&mut self, hook_name: &str, args: &[Value]) {
        self.queue_pending_safe_funcall(
            Value::symbol("run-hook-with-args"),
            std::iter::once(Value::symbol(hook_name))
                .chain(args.iter().copied())
                .collect(),
        );
    }

    pub(crate) fn flush_pending_safe_funcalls(&mut self) {
        while let Some(funcall) = self.pending_safe_funcalls.pop() {
            let _ = self.apply(funcall.function, funcall.args);
        }
    }

    pub(super) fn update_active_region_selection_after_command(&mut self) -> EvalResult {
        if self
            .eval_symbol("mark-active")
            .unwrap_or(Value::NIL)
            .is_nil()
        {
            return Ok(Value::NIL);
        }

        let transient_mark_mode = self
            .eval_symbol("transient-mark-mode")
            .unwrap_or(Value::NIL);
        if transient_mark_mode == Value::symbol("identity") {
            self.assign("transient-mark-mode", Value::NIL);
        } else if transient_mark_mode == Value::symbol("only") {
            self.assign("transient-mark-mode", Value::symbol("identity"));
        }

        if !self
            .eval_symbol("deactivate-mark")
            .unwrap_or(Value::NIL)
            .is_nil()
        {
            let _ = self.apply(Value::symbol("deactivate-mark"), vec![])?;
            self.assign("saved-region-selection", Value::NIL);
            return Ok(Value::NIL);
        }

        if self
            .apply(Value::symbol("display-selections-p"), vec![])?
            .is_nil()
        {
            self.assign("saved-region-selection", Value::NIL);
            return Ok(Value::NIL);
        }

        if self
            .eval_symbol("select-active-regions")
            .unwrap_or(Value::NIL)
            .is_nil()
        {
            self.assign("saved-region-selection", Value::NIL);
            return Ok(Value::NIL);
        }

        if self
            .apply(Value::symbol("region-active-p"), vec![])?
            .is_nil()
        {
            self.assign("saved-region-selection", Value::NIL);
            return Ok(Value::NIL);
        }

        let this_command = self.eval_symbol("this-command").unwrap_or(Value::NIL);
        let inhibited_commands = self
            .eval_symbol("selection-inhibit-update-commands")
            .unwrap_or(Value::NIL);
        if self
            .apply(
                Value::symbol("memq"),
                vec![this_command, inhibited_commands],
            )?
            .is_truthy()
        {
            self.assign("saved-region-selection", Value::NIL);
            return Ok(Value::NIL);
        }

        let region_extract = self
            .eval_symbol("region-extract-function")
            .unwrap_or(Value::symbol("buffer-substring"));
        let text = self.apply(region_extract, vec![Value::NIL])?;
        let text_len = match self.apply(Value::symbol("length"), vec![text])?.kind() {
            ValueKind::Fixnum(len) => len,
            _ => 0,
        };
        if text_len > 0 {
            let _ = self.apply(
                Value::symbol("gui-set-selection"),
                vec![Value::symbol("PRIMARY"), text],
            )?;
        }
        let _ = super::super::builtins::dispatch_builtin(
            self,
            "run-hook-with-args",
            vec![Value::symbol("post-select-region-hook"), text],
        )
        .unwrap_or(Ok(Value::NIL))?;
        self.assign("saved-region-selection", Value::NIL);
        Ok(Value::NIL)
    }

    /// Trigger redisplay — calls the layout engine and sends frame to render thread.
    ///
    /// Mirrors GNU Emacs `redisplay()` (dispnew.c:5259).
    /// In batch mode (no callback), this is a no-op.
    pub(crate) fn redisplay(&mut self) {
        self.redisplay_with_force(false);
    }

    pub(crate) fn redisplay_for_input_wait(&mut self) {
        self.redisplay_with_force(false);
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
        MenuBarRebuildGeneration(self.menu_bar_rebuild_generation)
    }

    /// See the `context_instance_id` field.
    pub fn context_instance_id(&self) -> u64 {
        self.context_instance_id
    }

    /// The chrome dirty set — which windows must re-generate their mode /
    /// header / tab line. See [`crate::emacs_core::chrome_dirty::ChromeDirty`]
    /// for the GNU flags this ports and for why nothing consults it as a skip
    /// yet.
    pub fn chrome_dirty(&self) -> &crate::emacs_core::chrome_dirty::ChromeDirty {
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
    /// [`crate::emacs_core::chrome_dirty::ChromeDirty`] for why the
    /// acknowledgement is per window rather than a blanket clear.
    pub fn note_chrome_generated(&mut self, window: WindowId) {
        self.chrome_dirty.note_chrome_generated(window);
    }

    /// Drop a deleted window's chrome acknowledgement.
    pub fn forget_chrome_window(&mut self, window: WindowId) {
        self.chrome_dirty.forget_window(window);
    }

    pub(crate) fn invalidate_redisplay(&mut self) {
        tracing::debug!(target: "neomacs::redisplay_sig", "invalidate_redisplay");
        self.redisplay_generation = self.redisplay_generation.wrapping_add(1);
        self.last_redisplay_signature = None;
    }

    /// Cross GNU `update_menu_bar`'s rebuild boundary and schedule redisplay.
    pub(crate) fn request_menu_bar_rebuild(&mut self, reason: MenuBarRebuildReason) {
        tracing::debug!(?reason, "request menu-bar rebuild");
        self.menu_bar_rebuild_generation = self.menu_bar_rebuild_generation.wrapping_add(1);
        self.invalidate_redisplay();
    }

    /// Raise GNU's global `update_mode_lines` flag.
    ///
    /// `bset_update_mode_line` does this unconditionally for buffer-owned
    /// mutations such as `rename-buffer`, even when that buffer is not yet
    /// displayed.  Keep the menu and chrome effects inseparable here: both
    /// are consumers of the same GNU flag.
    pub(crate) fn request_global_mode_line_update(&mut self) {
        self.request_menu_bar_rebuild(MenuBarRebuildReason::UpdateModeLines);
        self.mark_chrome_dirty_all();
    }

    /// Apply GNU's local/global `force-mode-line-update` boundary.
    pub(crate) fn request_mode_line_update(&mut self, target: ModeLineUpdateTarget) {
        let has_mode_line_to_update = match target {
            ModeLineUpdateTarget::CurrentBuffer(buffer) => {
                self.frames.buffer_window_count(&self.buffers, buffer) != 0
            }
            ModeLineUpdateTarget::AllBuffers => true,
        };
        if !has_mode_line_to_update {
            return;
        }

        self.request_global_mode_line_update();
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
    /// ([`crate::buffer::buffer::variable_affects_display_by_sym_id`]).
    /// This is checked at the single variable-set chokepoint so the
    /// answer is identical for every write path (tree-walk interpreter,
    /// bytecode VM, `set-default`, custom). The curated set keeps us
    /// from over-triggering redisplay on ordinary non-display variables.
    ///
    /// `sym_id` is resolved through `defvaralias` first so an alias of a
    /// display variable (e.g. an obsolete alias) still nudges redisplay.
    pub(crate) fn mark_redisplay_dirty_if_display_var(&mut self, sym_id: SymId) {
        let resolved =
            builtins::resolve_variable_alias_id_in_obarray(&self.obarray, sym_id).unwrap_or(sym_id);
        if crate::buffer::buffer::variable_affects_display_by_sym_id(resolved) {
            self.invalidate_redisplay();
            // GNU covers the three chrome formats with an
            // `add-variable-watcher` calling `set-buffer-redisplay`
            // (lisp/frame.el:3752-3779 -> xdisp.c:922-931), which raises the
            // mode-line dirty flag. The curated display-variable set here is
            // the same list by another name, so the chrome members of it get
            // the chrome flag too.
            if crate::buffer::buffer::variable_affects_chrome_by_sym_id(resolved) {
                self.chrome_dirty.mark_all();
            }
            // A display-affecting variable changed: the incremental fast paths
            // key on this counter so they re-lay instead of reusing rows shaped
            // under the old setting (the four buffer/face ticks do not move here).
            self.display_var_change_count = self.display_var_change_count.wrapping_add(1);
        }
    }

    pub(crate) fn redisplay_with_force(&mut self, force: bool) {
        // Mirrors GNU `redisplay_internal` (xdisp.c:17242-17245): bail out
        // when `inhibit-redisplay` is non-nil. `run_window_change_functions`
        // (window.c:4116) specbinds this to t so any nested redisplay
        // triggered by a window-change hook is a no-op. Without this check
        // a hook that indirectly calls `redisplay` infinitely recurses.
        let inhibit_redisplay = self.obarray.symbol_value("inhibit-redisplay");
        if !force && inhibit_redisplay.as_ref().is_some_and(|v| v.is_truthy()) {
            tracing::debug!(
                "redisplay inhibited by inhibit-redisplay={}",
                inhibit_redisplay.as_ref().unwrap()
            );
            return;
        }
        self.sync_pending_resize_events();
        // Sync window position caches from markers.  After text edits,
        // markers have auto-adjusted but the usize caches on Window::Leaf
        // may be stale.  Refresh them before redisplay reads positions.
        if let Some(buffer) = self.buffers.current_buffer() {
            let buf_id = buffer.id;
            crate::window::window_markers::sync_all_frames_for_buffer(
                &mut self.frames,
                &self.buffers,
                buf_id,
            );
        }
        // GNU's selected-window point belongs to the selected window's buffer,
        // even when Lisp has temporarily made another buffer current.  Refresh
        // only the selected window cache from its own buffer; redisplay must
        // not realign `current-buffer` with the selected window here.
        if let Some(frame_id) = self.frames.selected_frame().map(|frame| frame.id) {
            super::super::window_cmds::remember_selected_window_point_in_state(
                &mut self.frames,
                &mut self.buffers,
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
        if !force
            && !self.echo_area_resize_exact_pending
            && self.last_redisplay_signature.as_ref() == Some(&before_signature)
        {
            tracing::debug!("redisplay skipped: visible state unchanged");
            return;
        }
        // GNU `prepare_menu_bars` (xdisp.c:14230-14246) runs
        // `pre-redisplay-function` just before the window layout so hooks on
        // `pre-redisplay-functions` (e.g. `global-hl-line-mode` with sticky-flag
        // 'window, the region overlay) can refresh their overlays before
        // redisplay reads them. Placed AFTER the visible-state skip check so it
        // never runs on a skipped redisplay; `last_redisplay_signature` is
        // recomputed at the end of this function and absorbs any overlay change,
        // keeping the next unchanged redisplay skippable (no thrash).
        self.run_pre_redisplay_function();
        self.resize_minibuffer_only_frames();
        // GNU `redisplay_internal` calls `hscroll_window_tree` (src/xdisp.c)
        // before laying out windows so each window's `hscroll` follows point;
        // for a truncated line whose point has moved off the right edge (the
        // `C-e` case, issue #140) this keeps the cursor visible. Updating
        // `Window::Leaf.hscroll` here makes both the layout render and
        // `(window-hscroll)` reflect the new value (no post-layout write-back).
        crate::emacs_core::hscroll::update_auto_hscroll_before_redisplay(self);
        let has_fn = self.redisplay_fn.is_some();
        tracing::debug!("redisplay called (has_fn={})", has_fn);
        if let Some(mut f) = self.redisplay_fn.take() {
            let saved = self.buffers.reset_outermost_restrictions();
            f(self);
            // The layout pass inside `f` consumes any pending exact echo-area
            // resize (GNU `resize_echo_area_exactly`). Clear it now, once per
            // redisplay, so a later mid-command redisplay does not keep
            // shrinking a freshly grown message — GNU only resizes exactly at
            // the command boundary, not on every `redisplay_window`.
            self.echo_area_resize_exact_pending = false;
            let _ = super::super::builtins::run_redisplay_window_change_hooks(self);
            self.buffers.restore_outermost_restrictions(saved);
            self.redisplay_fn = Some(f);
        } else {
            self.echo_area_resize_exact_pending = false;
            let _ = super::super::builtins::run_redisplay_window_change_hooks(self);
        }
        self.last_redisplay_signature = Some(self.redisplay_signature());
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
    pub(super) fn run_pre_redisplay_function(&mut self) {
        let Some(function) = self.obarray.symbol_value("pre-redisplay-function").copied() else {
            return;
        };
        if function.is_nil() {
            return;
        }
        let specpdl_count = self.specpdl.len();
        if let Err(flow) = self.try_specbind_or_unwind_to(
            specpdl_count,
            crate::emacs_core::intern::intern("inhibit-redisplay"),
            Value::T,
        ) {
            tracing::debug!("pre-redisplay binding signalled (ignored): {flow:?}");
            return;
        }
        // GNU passes the list of windows being redisplayed; `t` makes
        // `redisplay--pre-redisplay-functions` iterate every live window.
        let result = self.funcall_general(function, vec![Value::T]);
        let result = self.unbind_to_with_result(specpdl_count, result);
        if let Err(flow) = result {
            tracing::debug!("pre-redisplay-function signalled (ignored): {flow:?}");
        }
    }

    pub(super) fn resize_minibuffer_only_frames(&mut self) {
        if !self
            .obarray
            .symbol_value("resize-mini-frames")
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
                        && frame.minibuffer_window == Some(frame.root_window.id()))
                    .then_some(Value::make_frame(frame_id.0))
                })
            })
            .collect();
        for frame in frames {
            let _ = self.safe_funcall(Value::symbol("window--resize-mini-frame"), vec![frame]);
        }
    }

    pub(super) fn redisplay_signature(&self) -> RedisplaySignature {
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
            selected_frame,
            selected_window,
            current_buffer: self.buffers.current_buffer_id().map(|id| id.0),
            current_message: self.current_message.clone(),
            active_minibuffer_window: self.active_minibuffer_window.map(|id| id.0),
            minibuffer_selected_window: self.minibuffer_selected_window.map(|id| id.0),
            face_change_count: self.face_change_count,
            obarray_function_epoch: self.obarray.function_epoch(),
            redisplay_generation: self.redisplay_generation,
            frame,
        }
    }

    pub(super) fn redisplay_buffer_signature(
        &self,
        id: crate::buffer::BufferId,
    ) -> Option<RedisplayBufferSignature> {
        let buffer = self.buffers.get(id)?;
        Some(RedisplayBufferSignature {
            layout: self.buffer_layout_inputs(id)?,
            save_modified_tick: buffer.save_modified_tick(),
            autosave_modified_tick: buffer.autosave_modified_tick,
            point: buffer.point_char_pos(),
            point_emacs_byte: buffer.point_emacs_byte_pos(),
            last_window_start: buffer.last_window_start,
            last_selected_window: buffer.last_selected_window.map(|id| id.0),
        })
    }

    pub(super) fn this_command_name_for_log(&self) -> String {
        self.eval_symbol("this-command")
            .map(|value| format!("{}", value))
            .unwrap_or_else(|_| "<unbound>".to_string())
    }

    pub(super) fn command_value_for_log(value: Value) -> String {
        value
            .as_symbol_name()
            .map(str::to_owned)
            .unwrap_or_else(|| crate::emacs_core::print::print_value(&value))
    }

    pub(super) fn command_keys_for_log(keys: &[Value]) -> String {
        keys.iter()
            .map(|key| {
                crate::emacs_core::keyboard::pure::describe_single_key_value(key, false)
                    .map(|description| crate::emacs_core::emacs_char::to_utf8_lossy(&description))
                    .unwrap_or_else(|_| crate::emacs_core::print::print_value(key))
            })
            .collect::<Vec<_>>()
            .join(" ")
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
    pub(super) fn gc_collect_from_current_roots(&mut self) {
        self.profiler_gc_start();
        self.gc_collect_from_current_roots_impl(self.gc_stress);
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
    pub(super) fn gc_collect_from_current_roots_impl(&mut self, force_complete: bool) {
        // A6 publication discipline: collecting while run_loop's operand-stack
        // cursor holds an unpublished length would mark a stale bc_buf prefix.
        #[cfg(debug_assertions)]
        crate::emacs_core::bytecode::vm::debug_assert_no_live_stack_cursor();
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

    pub(super) fn gc_collect_from_current_roots_body(&mut self, force_complete: bool) {
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
            || !(self.tagged_heap.mark_in_progress() || self.tagged_heap.sweep_in_progress())
        {
            crate::emacs_core::undo::compact_buffers_for_gc(self);
        }
        let start = std::time::Instant::now();
        self.lexenv_assq_cache.clear();
        self.lexenv_special_cache.clear();
        // Per-slice sweep budget in cons blocks (each ~4096 cells); the slice
        // reclaims proportionally more non-cons objects internally.
        const INCREMENTAL_SWEEP_BUDGET: usize = 8;
        let heap_ptr: *mut crate::tagged::gc::TaggedHeap = &mut *self.tagged_heap;
        // True only when a whole mark+sweep cycle finishes in this call, gating
        // the once-per-collection bookkeeping below.
        let cycle_completed;
        // Safety: GC is stop-the-world with exclusive `&mut self`. Root
        // enumeration only reads Context state while seeding the collector via
        // the raw heap pointer, which aliases `self.tagged_heap`.
        unsafe {
            if force_complete {
                // Explicit GC: drive any in-flight cycle to completion, then run
                // a fresh full stop-the-world collection of the current state
                // (GNU `garbage-collect` semantics).
                if (*heap_ptr).concurrent_mark_running() {
                    self.terminate_concurrent_mark(heap_ptr);
                }
                if (*heap_ptr).sweep_in_progress() {
                    (*heap_ptr).finish_incremental_sweep_now();
                }
                (*heap_ptr).begin_collection();
                self.seed_all_context_roots(heap_ptr);
                (*heap_ptr).complete_collection();
                cycle_completed = true;
            } else if (*heap_ptr).sweep_in_progress() {
                // Phase 3: drain the deferred sweep started at mark termination.
                if (*heap_ptr).incremental_sweep_slice(INCREMENTAL_SWEEP_BUDGET) {
                    cycle_completed = true; // sweep drained -> cycle done
                } else {
                    return; // more sweep to do; defer bookkeeping
                }
            } else if (*heap_ptr).concurrent_mark_running() {
                // Phase 5: the background GC thread is marking while we run. If it
                // has drained, run the (short) stop-the-world termination; else
                // return immediately and keep mutating — this is the pause win.
                // A hard cap forces termination if allocation outruns marking.
                let cap = (*heap_ptr).gc_threshold().saturating_mul(4);
                let must_finish = (*heap_ptr).bytes_since_gc() > cap;
                let mark_done = (*heap_ptr).concurrent_mark_done();
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
            } else if (*heap_ptr).should_run_concurrent() {
                // Concurrent start handshake: snapshot roots, hand the gray queue
                // to the GC thread, and return — marking now overlaps the mutator.
                self.start_concurrent_mark(heap_ptr);
                return; // marking concurrent; cycle not done yet
            } else if (*heap_ptr).is_partition_first_cycle() {
                // FIRST PARTITION CYCLE, CONCURRENT: the dump-blackening
                // bootstrap used to be the one big STW pause (a full trace of
                // the mapped image — ~12-50ms). Armed, `begin_collection`
                // seeds the veclike/string mapped children in the handshake
                // and stages the (bulk) cons ranges for the GC thread, whose
                // claim job DROPS span-inside children instead of deferring
                // them. Promotion + blackening run when this cycle's sweep
                // drains (`finish_first_partition_cycle` below).
                (*heap_ptr).arm_first_cycle_concurrent();
                self.start_concurrent_mark(heap_ptr);
                return; // marking concurrent; cycle not done yet
            } else {
                // Stop-the-world full collection (dump-less bootstrap): the
                // only remaining non-concurrent threshold path, sized by the
                // young heap alone.
                (*heap_ptr).begin_collection();
                self.seed_all_context_roots(heap_ptr);
                (*heap_ptr).complete_collection();
                cycle_completed = true;
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
        // `post-gc-hook`.
        self.run_doomed_finalizers();
        self.run_post_gc_hook();
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
    pub(super) unsafe fn seed_all_context_roots(
        &mut self,
        heap_ptr: *mut crate::tagged::gc::TaggedHeap,
    ) -> crate::tagged::gc::RootSeedBreakdown {
        use std::cell::{Cell, RefCell};
        let seed_t0 = std::time::Instant::now();
        // Per-group recorder shared by the two `trace_roots` closures via
        // interior mutability (both need it: the boundary closure closes the
        // running group, the visit closure counts values).
        let groups: RefCell<Vec<crate::tagged::gc::RootGroup>> =
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
        self.trace_roots(
            &mut |name| {
                close_group();
                cur_name.set(Some(name));
                cur_count.set(0);
                cur_t0.set(std::time::Instant::now());
            },
            &mut |root| {
                cur_count.set(cur_count.get() + 1);
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
        );
        close_group();
        let mut groups = groups.into_inner();
        let heap_identity = unsafe { (*heap_ptr).identity() };
        let mut thread_local_roots = Vec::new();
        collect_thread_local_gc_roots(&mut thread_local_roots, heap_identity, &mut groups);
        let tl_seed_t0 = std::time::Instant::now();
        let tl_seed_count = thread_local_roots.len();
        for (root, origin) in thread_local_roots {
            unsafe {
                (*heap_ptr).seed_root_with_origin(root, origin);
            }
        }
        groups.push((
            "tl_seed",
            tl_seed_t0.elapsed().as_micros() as u64,
            tl_seed_count,
        ));
        // Install per-buffer marker-chain head slots so `unchain_dead_markers`
        // can splice unmarked markers out of every live chain before sweep.
        // Mirrors GNU `sweep_buffer → unchain_dead_markers` (alloc.c).
        let heads_t0 = std::time::Instant::now();
        // Safety: stop-the-world GC — no concurrent borrows of the buffer
        // storage exist (the pre-refactor body relied on the enclosing
        // `unsafe fn` for this same call).
        let chain_heads = unsafe { self.buffers.collect_marker_chain_head_slots() };
        let heads_count = chain_heads.len();
        unsafe {
            (*heap_ptr).set_marker_chain_head_slots(chain_heads);
        }
        groups.push((
            "marker_heads",
            heads_t0.elapsed().as_micros() as u64,
            heads_count,
        ));
        crate::tagged::gc::RootSeedBreakdown {
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
    /// Safety: as `seed_all_context_roots`.
    pub(super) unsafe fn start_concurrent_mark(
        &mut self,
        heap_ptr: *mut crate::tagged::gc::TaggedHeap,
    ) {
        let start_t0 = std::time::Instant::now();
        let (obsnap_us, roots_breakdown, ob_slots, ob_chunks);
        unsafe {
            (*heap_ptr).concurrent_begin();
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
            let snap = self.obarray.scan_snapshot();
            obsnap_us = obsnap_t0.elapsed().as_micros() as u64;
            ob_slots = snap.n_slots();
            ob_chunks = snap.n_chunks();
            (*heap_ptr).set_pending_obarray_scan(snap);
            {
                let _skip = crate::emacs_core::symbol::ObarraySymbolCellSkipGuard::new();
                roots_breakdown = self.seed_all_context_roots(heap_ptr);
            }
            (*heap_ptr).launch_concurrent_mark();
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
        if std::env::var("NEOVM_GC_TRACE").as_deref() == Ok("1") {
            eprintln!(
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
                hs.last_start_roots.format_nonzero(),
                hs.format_probes(),
            );
        }
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
    pub(super) unsafe fn terminate_concurrent_mark(
        &mut self,
        heap_ptr: *mut crate::tagged::gc::TaggedHeap,
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
                let _skip = crate::emacs_core::symbol::ObarraySymbolCellSkipGuard::new();
                ctxroots_breakdown = self.seed_all_context_roots(heap_ptr);
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
        if std::env::var("NEOVM_GC_TRACE").as_deref() == Ok("1") {
            let stats = self.tagged_heap.sweep_stats();
            let hs = self.tagged_heap.handshake_stats();
            eprintln!(
                "NEOVM_GC concurrent_termination {}us [roots={roots_us}us drain={drain_us}us \
                 fold={}us deferred={} satb={} str_claimed={} f_claimed={} sub_dropped={} \
                 v_claimed={} bc_claimed={} kinds[{}] join={}us \
                 runtime={}us({}) remembered={}us({}) ctxroots={}us newsyms={newsyms_us}us({}) \
                 finalizer={}us weak={}us unchain={}us groups[{}] probes[{}]]",
                term_t0.elapsed().as_micros(),
                stats.last_termination_fold_us,
                stats.last_termination_deferred,
                stats.last_termination_satb,
                stats.last_concurrent_str_claimed,
                stats.last_concurrent_float_claimed,
                stats.last_concurrent_subr_dropped,
                stats.last_concurrent_vec_claimed,
                stats.last_concurrent_bc_claimed,
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
                hs.last_term_ctxroots.format_nonzero(),
                hs.format_probes(),
            );
        }
    }

    pub(crate) fn with_gc_inhibited<T>(&mut self, f: impl FnOnce(&mut Self) -> T) -> T {
        let mut guard = GcInhibitGuard::enter(self);
        f(guard.context())
    }

    pub(super) fn run_post_gc_hook(&mut self) {
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
    pub(super) fn run_doomed_finalizers(&mut self) {
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
    pub(super) fn drain_pending_surface_destroys(&mut self) {
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
    pub(super) fn drain_pending_video_destroys(&mut self) {
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
    pub(crate) fn lisp_string(&self, value: Value) -> Option<&crate::heap_types::LispString> {
        value.lisp_string_in(&self.tagged_heap)
    }

    /// [`Context::lisp_string`] with GNU's `CHECK_STRING` signal — the
    /// drop-in replacement for `builtins::expect_lisp_string(&args[i])?` at a
    /// site that must not hold the borrow across a safepoint.
    #[inline]
    pub(crate) fn expect_lisp_string(
        &self,
        value: Value,
    ) -> Result<&crate::heap_types::LispString, Flow> {
        self.lisp_string(value).ok_or_else(|| {
            crate::emacs_core::error::signal(
                crate::emacs_core::error::LispCondition::WrongTypeArgument,
                vec![Value::symbol("stringp"), value],
            )
        })
    }

    /// GC safe point used at evaluator boundaries.
    pub fn gc_safe_point(&mut self) {
        self.gc_safe_point_exact();
    }

    /// Trigger a safe-point collection using only explicit evaluator roots.
    pub(crate) fn gc_safe_point_exact(&mut self) {
        if self.gc_safe_point_exact_should_collect() {
            self.gc_collect_from_current_roots();
        }
    }

    pub(super) fn gc_safe_point_exact_should_collect(&mut self) -> bool {
        if self.gc_inhibit_depth > 0 {
            return false;
        }
        // An in-flight incremental mark or deferred sweep must keep getting
        // slices at every safe point until it finishes, regardless of the
        // allocation threshold.
        if self.tagged_heap.mark_in_progress() || self.tagged_heap.sweep_in_progress() {
            return true;
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
    pub(super) fn process_quit_flag(&mut self) -> Result<(), Flow> {
        let flag = self.quit_flag;
        self.set_quit_flag_value(Value::NIL);

        let throw_on_input = self
            .obarray
            .symbol_value_id_or_nil(self.throw_on_input_symbol);

        if flag.as_symbol_id() == Some(self.kill_emacs_symbol) {
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
            return Err(Flow::throw(throw_on_input, Value::T));
        }

        Err(signal(LispCondition::Quit, vec![]))
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
    /// The pure fast-path condition of [`Self::maybe_quit`]: true when the
    /// poll would do nothing. Loads only — no mutation, no allocation, no
    /// Lisp — so bytecode dispatch may evaluate it with its operand-stack
    /// cursor still live and only publish for the cold slow path.
    #[inline(always)]
    pub(crate) fn maybe_quit_hot_ok(&self) -> bool {
        !crate::emacs_core::profiler::profiler_sample_due()
            && !crate::emacs_core::os_signal::pending()
            && self.quit_flag.is_nil()
            && !self
                .quit_requested
                .load(std::sync::atomic::Ordering::Relaxed)
            && (self.throw_on_input.is_nil() || !self.has_throw_on_input_poll_source())
    }

    #[inline(always)]
    pub(crate) fn maybe_quit(&mut self) -> Result<(), Flow> {
        // Profiler sampling rides the quit poll (GNU samples in a SIGPROF
        // handler; SIGPROF belongs to the native profiler here, so the Lisp
        // profiler's watchdog raises a flag that this — the canonical safe
        // point every engine polls — consumes). One 'static relaxed load
        // when no profiler runs, replacing the per-call profiler_poll the
        // backtrace push/pop helpers used to pay.
        if crate::emacs_core::profiler::profiler_sample_due() {
            self.profiler_sample_tick();
        }
        // GNU's safe point is `if (!NILP (Vquit_flag) || pending_signals)`
        // (src/lisp.h:3896-3900), so an OS signal costs exactly one more
        // relaxed `'static` load here -- GNU's own hot-path shape and cost.
        if self.quit_flag.is_nil()
            && !crate::emacs_core::os_signal::pending()
            && !self
                .quit_requested
                .load(std::sync::atomic::Ordering::Relaxed)
            && (self.throw_on_input.is_nil() || !self.has_throw_on_input_poll_source())
        {
            return Ok(());
        }
        self.maybe_quit_slow()
    }

    #[cold]
    pub(super) fn maybe_quit_slow(&mut self) -> Result<(), Flow> {
        if self.has_throw_on_input_poll_source() {
            self.poll_pending_input_for_throw_on_input()?;
        }

        // Drain the cross-thread quit-request atomic into `Vquit_flag`.
        // Set by the input-bridge thread when it observes a `quit-char`
        // keystroke while the evaluator is busy (e.g. deep in bytecode
        // and not reading from `input_rx`). See
        // `Context::quit_requested` for the design rationale.
        if self
            .quit_requested
            .load(std::sync::atomic::Ordering::Relaxed)
            && self
                .quit_requested
                .swap(false, std::sync::atomic::Ordering::Relaxed)
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
            if crate::emacs_core::os_signal::pending() {
                crate::emacs_core::os_signal::drain_pending_os_signals(self);
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
    pub(crate) fn debug_on_event_signal_name(&self) -> Option<String> {
        let value = self.obarray.symbol_value("debug-on-event").copied()?;
        let name = value.as_symbol_lisp_string()?;
        Some(crate::emacs_core::emacs_char::to_utf8_lossy(
            name.as_bytes(),
        ))
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
    pub(crate) fn arm_debugger_for_debug_on_event(&mut self) {
        self.set_variable("debug-on-next-call", Value::T);
        self.set_variable("debug-on-quit", Value::T);
        self.set_variable("inhibit-quit", Value::NIL);
        self.set_quit_flag_value(Value::T);
    }

    #[inline(always)]
    pub(crate) fn quit_flag_value(&self) -> Value {
        self.quit_flag
    }

    /// GNU `QUITP`: true only when a quit is pending and `inhibit-quit` is nil.
    #[inline(always)]
    pub(crate) fn quit_pending(&self) -> bool {
        !self.quit_flag.is_nil() && self.inhibit_quit.is_nil()
    }

    #[inline(always)]
    pub(crate) fn set_quit_flag_value(&mut self, value: Value) {
        self.quit_flag = value;
        self.obarray
            .set_symbol_value_id(self.quit_flag_symbol, value);
    }

    pub(crate) fn quit_char(&self) -> i64 {
        self.quit_char
    }

    pub(crate) fn set_quit_char(&mut self, quit: i64) {
        self.quit_char = quit & 0o377;
    }

    pub(crate) fn event_is_quit_char(&self, event: &Value) -> bool {
        event.as_fixnum() == Some(self.quit_char)
    }

    pub(crate) fn request_quit_from_keyboard_input(&mut self) {
        if self.quit_flag_value().is_nil() {
            self.set_quit_flag_value(Value::T);
        }
    }

    pub(crate) fn clear_quit_flag_after_read_key_sequence_event(&mut self, event: &Value) {
        if !self.event_is_quit_char(event) {
            return;
        }

        let throw_on_input = self
            .obarray
            .symbol_value_id_or_nil(self.throw_on_input_symbol);

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
            self.quit_requested
                .store(false, std::sync::atomic::Ordering::Relaxed);
        }
    }

    pub(super) fn input_pending_filter(&self) -> crate::keyboard::InputPendingFilter {
        let configured = self
            .obarray
            .symbol_value("input-pending-p-filter-events")
            .copied()
            .unwrap_or(Value::T)
            .is_truthy();
        crate::keyboard::InputPendingFilter::from_filter_events_variable(configured)
    }

    /// GNU's `track_mouse` (`DEFVAR_LISP`, `src/keyboard.c:14134`), which every
    /// terminal back-end dereferences as a bare global -- `src/term.c:3465`,
    /// `src/androidterm.c:558`, `src/haikuterm.c:425`, `src/w32fns.c:5118`.
    ///
    /// The swap-in makes that global the *current buffer's* binding whenever a
    /// buffer has localised it (dframe, dictionary and gud all do), so the read
    /// names the buffer. Ledger 196.
    pub(crate) fn track_mouse_enabled(&self) -> bool {
        self.obarray
            .value_in_buffer(self.buffers.current_buffer(), "track-mouse")
            .unwrap_or(Value::NIL)
            .is_truthy()
    }

    pub(super) fn should_ignore_while_no_input_symbol(&self, ignore_symbol: &str) -> bool {
        let ignore_list = self
            .obarray
            .symbol_value("while-no-input-ignore-events")
            .copied()
            .unwrap_or(Value::NIL);
        super::super::value::list_to_vec(&ignore_list)
            .into_iter()
            .flatten()
            .any(|value| value.is_symbol_named(ignore_symbol))
    }

    pub(crate) fn has_pending_command_input_for_query(&self) -> bool {
        let filter = self.input_pending_filter();
        self.command_loop
            .keyboard
            .has_pending_command_input_for_query(filter, self.track_mouse_enabled(), |symbol| {
                self.should_ignore_while_no_input_symbol(symbol)
            })
    }

    pub(crate) fn has_pending_frontend_input_with_configured_filter(&self) -> bool {
        self.command_loop
            .keyboard
            .pending_input_events
            .has_pending_input(
                crate::keyboard::InputPendingFilter::ConfiguredIgnoreList,
                self.track_mouse_enabled(),
                |symbol| self.should_ignore_while_no_input_symbol(symbol),
            )
    }

    pub(crate) fn open_channel_for_module(&self, process: Value) -> Result<std::ffi::c_int, Flow> {
        self.processes.open_channel_for_module(process)
    }

    #[inline(always)]
    pub(super) fn has_throw_on_input_poll_source(&self) -> bool {
        // GNU's evaluator-side `maybe_quit` is a cheap flag/signal check; the
        // input path sets `quit-flag` when real keyboard input is available.
        // Neomacs has to poll the host channel for `throw-on-input`, but only
        // when such a channel exists or the command loop is interactive.
        self.input_rx.is_some() || !self.command_loop_noninteractive()
    }

    pub(super) fn poll_pending_input_for_throw_on_input(&mut self) -> Result<(), Flow> {
        debug_assert!(self.has_throw_on_input_poll_source());

        if self.unwind_cleanup_depth != 0 {
            return Ok(());
        }

        let throw_on_input = self
            .obarray
            .symbol_value_id_or_nil(self.throw_on_input_symbol);
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

    /// Interrupt on input for GNU-style `throw-on-input` users such as
    /// `while-no-input`, while preserving the input event for later reads.
    pub(crate) fn interrupt_for_input_event_if_requested(
        &mut self,
        event: crate::keyboard::InputEvent,
    ) -> Result<bool, Flow> {
        let throw_on_input = self
            .obarray
            .symbol_value_id_or_nil(self.throw_on_input_symbol);
        if throw_on_input.is_nil() {
            return Ok(false);
        }

        if self.inhibit_quit.is_truthy() {
            return Ok(false);
        }

        self.command_loop
            .keyboard
            .pending_input_events
            .push_front(event);
        self.set_quit_flag_value(throw_on_input);
        self.maybe_quit()?;
        Ok(true)
    }

    pub(super) fn maybe_quit_before_gc(&mut self) -> Result<(), Flow> {
        self.maybe_quit()
    }

    /// Match GNU `eval_sub` / `funcall_general`: quit check first, then GC.
    ///
    /// The remaining evaluator entry points either root their live Values
    /// explicitly or run before materializing heap-backed Values, so this path
    /// now uses exact roots rather than conservative stack scanning.
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(super) fn maybe_gc_and_quit(&mut self) -> Result<(), Flow> {
        self.maybe_quit_before_gc()?;
        if self.gc_safe_point_exact_should_collect() {
            self.gc_collect_from_current_roots();
        }
        Ok(())
    }

    /// Match GNU `bytecode.c:op_branch`: after the bytecode loop's unsigned
    /// quit counter wraps, run `maybe_gc (); maybe_quit ();`.
    pub(crate) fn bytecode_branch_maybe_gc_and_quit(&mut self) -> Result<(), Flow> {
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
        if crate::tagged::gc::concurrent_mark_active() {
            crate::tagged::gc::feed_concurrent_roots(&self.bc_buf);
            crate::tagged::gc::feed_concurrent_roots(
                &self.jit_root_stack[..self.jit_root_stack_top],
            );
        }
        self.maybe_quit()
    }
}
