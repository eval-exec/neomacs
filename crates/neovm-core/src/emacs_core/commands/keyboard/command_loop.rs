//! GNU keyboard.c command-loop body (STAGE4 design §5, R0).
//!
//! The loop spine, command dispatch, startup logging and the interactive
//! input/quit boundary bind keyboard behaviour, so they live in the core
//! command layer.  They operate on the evaluator's `&mut Context` and call
//! its GC, safepoint and generic quit substrate downward.  The trait is
//! declared here and implemented for the evaluator's Context.

use crate::emacs_core::command_observation::{
    UserCommandIdentity, UserCommandObservation, UserCommandObservationStart, UserCommandOutcome,
};
use crate::emacs_core::error::{
    EvalResult, Flow, FlowKind, FlowResultExt, LispCondition, SignalData, make_signal_binding_value,
};
use crate::emacs_core::eval::command_loop::CommandLoopSymbol;
use crate::emacs_core::eval::{
    CommandLoopDiagnostic, CommandLoopExit, ConditionFrame, Context, ModeLineUpdateTarget,
    ResumeTarget, ShutdownRequest, format_startup_value,
};
use crate::emacs_core::intern::{SymId, WellKnown, format_symbol_name_for_diagnostic};
use crate::emacs_core::value::{Value, ValueKind, eq_value_swp, equal_value};
use crate::window::DisplayFrameExt;
use crate::window::DisplayFrameManagerExt;
use neovm_edit::emacs_core::edit_host::EditHost;
use neovm_eval::emacs_core::reader::KeyboardInputRuntime;

static TOP_LEVEL_SYMBOL: WellKnown = WellKnown::TopLevel;
static EXIT_TAG: WellKnown = WellKnown::Exit;
static IDENTITY_SYMBOL: WellKnown = WellKnown::Identity;
static ONLY_SYMBOL: WellKnown = WellKnown::Only;
static TRANSIENT_MARK_MODE: WellKnown = WellKnown::TransientMarkMode;
static UNDO_AUTO_UNDOABLY_CHANGED_BUFFERS: WellKnown = WellKnown::UndoAutoUndoablyChangedBuffers;
static NEOVM_STARTUP_LAST_PHASE: WellKnown = WellKnown::NeomacsStartupLastPhase;
static NEOVM_STARTUP_LAST_CALL: WellKnown = WellKnown::NeomacsStartupLastCall;
///
/// GNU names them with DEFSYM statics (`Qpre_command_hook`,
/// `Qpost_command_hook`, `Qdelayed_warnings_hook`); the loop previously
/// re-interned each spelling on every command.
#[derive(Clone, Copy, Debug)]
enum CommandLoopHook {
    PreCommand,
    PostCommand,
    DelayedWarnings,
}

impl CommandLoopHook {
    #[inline]
    fn id(self) -> SymId {
        use crate::emacs_core::intern::WellKnown;

        match self {
            Self::PreCommand => {
                static SYMBOL: WellKnown = WellKnown::PreCommandHook;
                SYMBOL.id()
            }
            Self::PostCommand => {
                static SYMBOL: WellKnown = WellKnown::PostCommandHook;
                SYMBOL.id()
            }
            Self::DelayedWarnings => {
                static SYMBOL: WellKnown = WellKnown::DelayedWarningsHook;
                SYMBOL.id()
            }
        }
    }
}

/// Evaluator-driver methods of the GNU keyboard.c command loop.
///
/// The loop body and the interactive input/quit boundary bind keyboard
/// behaviour, so they live in the core command layer (STAGE4 design §5,
/// R0).  Core declares the trait and implements it for the evaluator's
/// Context; the evaluator keeps the GC, safepoint and generic quit
/// substrate that these methods call downward.
pub(crate) trait CommandLoopHost {
    fn classify_command_loop_exit(&mut self, value: Value) -> Result<CommandLoopExit, Flow>;
    fn run_exit_wrapped_command_loop(&mut self, increment_depth: bool) -> EvalResult;
    fn command_loop_inner(&mut self) -> EvalResult;
    fn command_loop_top_level_1(&mut self) -> EvalResult;
    fn trace_startup_state_enabled(&self) -> bool;
    fn log_startup_state(&self, phase: &str);
    fn command_loop_2(&mut self) -> EvalResult;
    fn command_error_message(&mut self, sig: &SignalData) -> String;
    fn capture_command_loop_diagnostic(&mut self, sig: &SignalData) -> CommandLoopDiagnostic;
    fn command_loop_1(&mut self) -> EvalResult;
    fn command_loop_1_entry_prologue(&mut self) -> EvalResult;
    fn command_loop_1_maybe_auto_save(&mut self);
    fn input_pending_for_auto_save(&mut self) -> bool;
    fn dispatch_command_in_loop(&mut self, command: Value) -> EvalResult;
    fn execute_kbd_macro_iteration_via_command_loop(&mut self) -> EvalResult;
    fn update_active_region_selection_after_command(&mut self) -> EvalResult;
    fn this_command_name_for_log(&self) -> String;
    fn command_value_for_log(value: Value) -> String;
    fn command_keys_for_log(keys: &[Value]) -> String;
    fn recursive_edit_inner(&mut self) -> EvalResult;
    fn minibuffer_command_loop_inner(&mut self) -> EvalResult;
    fn interrupt_for_input_event_if_requested(
        &mut self,
        event: crate::keyboard::InputEvent,
    ) -> Result<bool, Flow>;
}

impl CommandLoopHost for Context {
    /// Classify the value carried by a `(throw 'exit VALUE)` that unwound a
    /// recursive command loop.
    ///
    /// Mirrors GNU `recursive_edit_1` (keyboard.c:749-758), which dispatches on
    /// the thrown value's *type* rather than its truthiness.
    fn classify_command_loop_exit(&mut self, value: Value) -> Result<CommandLoopExit, Flow> {
        if eq_value_swp(&value, &Value::T, self.symbols_with_pos_enabled) {
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

    fn run_exit_wrapped_command_loop(&mut self, increment_depth: bool) -> EvalResult {
        // Interactive command loops need an input source. Batch mode is
        // different: GNU still runs `top-level`/`normal-top-level` and lets
        // `read_char` terminate the loop via noninteractive EOF, even when
        // there is no input channel at all.
        if self.input_rx().as_ref().is_none() && !self.command_loop_noninteractive() {
            tracing::info!("recursive_edit_inner: no input receiver, returning immediately");
            return Ok(Value::NIL);
        }

        // Recursive edits and minibuffer readers enter the command loop even
        // when the outer loop has not been started through init_input_system().
        // GNU's recursive/minibuffer entry points do not consult an external
        // "running" gate before dispatching the first key. Preserve the
        // previous flag so explicit shutdown still unwinds correctly.
        let saved_running = self.command_loop().running;
        if !saved_running {
            self.command_loop_mut().running = true;
        }

        if increment_depth {
            self.command_loop_mut().recursive_depth += 1;
            // GNU Frecursive_edit raises update_mode_lines after entering.
            self.request_mode_line_update(ModeLineUpdateTarget::AllBuffers);
        }

        // GNU `recursive_edit_1` owns these bindings around the entire
        // `command_loop`, outside `command_loop_2`'s error/restart boundary.
        // Keeping them here also leaves execute-kbd-macro free to borrow its
        // caller's dynamic environment, as GNU macros.c does.
        let specpdl_count = self.specpdl().len();
        let result = (|| -> EvalResult {
            self.try_specbind_or_unwind_to(
                specpdl_count,
                CommandLoopSymbol::InhibitRedisplay.id(),
                Value::NIL,
            )?;
            self.try_specbind_or_unwind_to(
                specpdl_count,
                UNDO_AUTO_UNDOABLY_CHANGED_BUFFERS.id(),
                Value::NIL,
            )?;

            // GNU `command_loop` installs its `exit` catch only for a recursive
            // command loop or an active minibuffer (`command_loop_level > 0 ||
            // minibuf_level > 0`).  The outermost loop must leave `exit`
            // unmatched, so `(throw 'exit ...)` there signals `no-catch`.
            let catches_exit =
                self.recursive_command_loop_depth() > 0 || self.minibuffers().depth() > 0;
            if catches_exit {
                self.push_condition_frame(ConditionFrame::Catch {
                    tag: Value::symbol(&EXIT_TAG),
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
                Err(ref flow)
                    if let Some(thrown) = flow.as_throw()
                        && catches_exit
                        && thrown.matches_tag(Value::symbol(&EXIT_TAG)) =>
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
            self.command_loop_mut().recursive_depth -= 1;
            // The unwind boundary removes %[ / %] even after a non-local exit.
            self.request_mode_line_update(ModeLineUpdateTarget::AllBuffers);
        }
        if !saved_running {
            self.command_loop_mut().running = false;
        }

        self.unbind_to_with_result(specpdl_count, result)
    }

    /// Inner command loop; only the outermost loop catches `top-level`.
    ///
    /// Mirrors GNU Emacs `command_loop()` (keyboard.c:1104).
    /// The outermost invocation wraps command_loop_2 in a catch for
    /// 'top-level.
    #[tracing::instrument(skip_all)]
    fn command_loop_inner(&mut self) -> EvalResult {
        let outermost_command_loop =
            self.command_loop().recursive_depth == 1 && self.minibuffers().depth() == 0;
        loop {
            if outermost_command_loop {
                // Catch 'top-level throws (from (top-level) function).
                let top_level_tag = Value::symbol(&TOP_LEVEL_SYMBOL);
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
                    Err(ref flow)
                        if let Some(thrown) = flow.as_throw()
                            && thrown.matches_tag(Value::symbol(&TOP_LEVEL_SYMBOL)) =>
                    {
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
                Err(ref flow)
                    if let Some(thrown) = flow.as_throw()
                        && outermost_command_loop
                        && thrown.matches_tag(Value::symbol(&TOP_LEVEL_SYMBOL)) =>
                {
                    tracing::debug!("command_loop_inner: top-level throw, restarting loop");
                    continue;
                }
                Ok(value) if outermost_command_loop && self.command_loop_noninteractive() => {
                    // GNU keyboard.c:1145 — end of file in batch run
                    tracing::info!("command_loop_inner: noninteractive EOF, calling kill-emacs");
                    match super::super::builtins::symbols::builtin_kill_emacs(self, vec![Value::T])
                        .kinded()
                    {
                        Err(FlowKind::Shutdown(_)) | Ok(_) => {}
                        Err(flow) => return Err(Flow::from_kind(flow)),
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

    fn command_loop_top_level_1(&mut self) -> EvalResult {
        let top_level = self
            .builtin_var_value(TOP_LEVEL_SYMBOL.id())
            .unwrap_or(Value::NIL);

        tracing::debug!("command_loop_top_level_1: top-level={}", top_level);

        if top_level.is_nil() {
            tracing::debug!("command_loop_top_level_1: top-level is nil, skipping");
            self.log_startup_state("top-level-nil");
            return Ok(Value::NIL);
        }

        tracing::debug!("command_loop_top_level_1: evaluating top-level form");
        self.log_startup_state("top-level-before");
        match self.eval_value(&top_level).kinded() {
            Ok(_) => {
                tracing::debug!("command_loop_top_level_1: top-level completed OK");
                self.log_startup_state("top-level-after");
                Ok(Value::NIL)
            }
            Err(FlowKind::Signal(sig)) => {
                let rendered = crate::emacs_core::print::format_signal_data_with_eval(self, &sig);
                tracing::warn!("command_loop_top_level_1: top-level SIGNALED: {}", rendered);
                if self
                    .display_host
                    .as_mut()
                    .is_some_and(|host| host.fail_pending_gui_startup(rendered.clone()))
                {
                    eprintln!("Error during GUI startup: {rendered}");
                    return self.shutdown_with_hooks(ShutdownRequest {
                        exit_code: 1,
                        restart: false,
                    });
                }
                if self
                    .daemon()
                    .as_ref()
                    .is_some_and(|daemon| !daemon.initialized)
                {
                    // A display-free startup cannot enter an interactive
                    // debugger to report a failed init/server/action. Retire
                    // it instead, so the readiness parent sees EOF/failure.
                    eprintln!("Error during daemon startup: {rendered}");
                    return self.shutdown_with_hooks(ShutdownRequest {
                        exit_code: 1,
                        restart: false,
                    });
                }
                let error_msg = self.command_error_message(&sig);
                let data = self.signal_error_data_value(&sig);
                self.report_command_error(data, "")?;
                if cfg!(test) {
                    let last_phase = self
                        .builtin_var_value(NEOVM_STARTUP_LAST_PHASE.id())
                        .map(|value| crate::emacs_core::print_value_with_eval(self, &value))
                        .unwrap_or_else(|| "nil".to_string());
                    let last_call = self
                        .builtin_var_value(NEOVM_STARTUP_LAST_CALL.id())
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
                    return self.shutdown_with_hooks(ShutdownRequest {
                        exit_code: -1,
                        restart: false,
                    });
                }
                Ok(Value::NIL)
            }
            Err(flow) => Err(Flow::from_kind(flow)),
        }
    }

    fn trace_startup_state_enabled(&self) -> bool {
        std::env::var("NEOMACS_TRACE_STARTUP_STATE")
            .ok()
            .is_some_and(|value| value == "1")
    }

    fn log_startup_state(&self, phase: &str) {
        if !self.trace_startup_state_enabled() {
            return;
        }

        let current_buffer = self
            .edit
            .buffers
            .current_buffer()
            .map(|buffer| buffer.name_runtime_string_owned())
            .unwrap_or_else(|| "<none>".to_string());
        let selected_frame = self.frame_manager().selected_frame().map(|frame| {
            let selected_window_buffer = frame
                .selected_window()
                .and_then(|window| window.buffer_id())
                .and_then(|buffer_id| self.edit.buffers.get(buffer_id))
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
            .frame_manager()
            .frame_list()
            .into_iter()
            .map(|fid| format!("0x{:x}", fid.0))
            .collect::<Vec<_>>();

        tracing::info!(
            "startup-state phase={} command-line-args={} command-line-args-left={} command-line-processed={} window-system={} initial-window-system={} current-buffer={} selected-frame={:?} frames={:?}",
            phase,
            format_startup_value(self.builtin_var_value_by_name("command-line-args").as_ref()),
            format_startup_value(
                self.builtin_var_value_by_name("command-line-args-left")
                    .as_ref()
            ),
            format_startup_value(
                self.builtin_var_value_by_name("command-line-processed")
                    .as_ref()
            ),
            format_startup_value(self.builtin_var_value_by_name("window-system").as_ref()),
            format_startup_value(
                self.builtin_var_value_by_name("initial-window-system")
                    .as_ref()
            ),
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
    fn command_loop_2(&mut self) -> EvalResult {
        loop {
            match self.command_loop_1().kinded() {
                Ok(val) => return Ok(val),
                Err(flow @ FlowKind::Throw(_)) => {
                    // Throws propagate (exit, top-level, etc.) without
                    // re-entering the command loop.  Re-running command_loop_1
                    // here traps minibuffer exit throws and blocks waiting for
                    // another key instead of unwinding like GNU Emacs.
                    return Err(Flow::from_kind(flow));
                }
                // A shutdown unwinds the command loop instead of restarting it:
                // GNU never returns from Fkill_emacs to command_loop_2.
                Err(flow @ (FlowKind::ThreadBlocked(_) | FlowKind::Shutdown(_))) => {
                    return Err(Flow::from_kind(flow));
                }
                Err(flow @ FlowKind::Signal(_))
                    if self
                        .command_loop()
                        .keyboard
                        .kboard
                        .executing_kbd_macro
                        .is_some() =>
                {
                    return Err(Flow::from_kind(flow));
                }
                Err(FlowKind::Signal(sig)) => {
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
                    self.assign_by_id(CommandLoopSymbol::PrefixArg.id(), Value::NIL);
                    self.assign_by_id(CommandLoopSymbol::LastPrefixArg.id(), Value::NIL);
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
    fn command_error_message(&mut self, sig: &SignalData) -> String {
        let error_data = make_signal_binding_value(sig);
        crate::emacs_core::print::builtin_error_message_string(self, vec![error_data])
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
    fn capture_command_loop_diagnostic(&mut self, sig: &SignalData) -> CommandLoopDiagnostic {
        CommandLoopDiagnostic::new(
            self.command_error_severity(sig),
            format_symbol_name_for_diagnostic(sig.symbol),
            self.command_error_message(sig),
            crate::emacs_core::print::format_signal_data_with_eval(self, sig),
            self.last_uncaught_signal_backtrace
                .take()
                .unwrap_or_default(),
        )
    }

    /// Main command loop — read key sequence, look up binding, execute.
    ///
    /// Mirrors GNU Emacs `command_loop_1()` (keyboard.c:1306).
    /// This is the core interactive loop: read → dispatch → redisplay.
    #[tracing::instrument(skip_all)]
    fn command_loop_1(&mut self) -> EvalResult {
        if !self.command_loop().running {
            return Ok(Value::NIL);
        }

        self.command_loop_1_entry_prologue()?;

        loop {
            if !self.command_loop().running {
                return Ok(Value::NIL);
            }

            self.service_gui_command_boundary()?;

            // Retained through hooks, point adjustment and finalization. A
            // mid-command redisplay must not acknowledge unfinished input.
            let _completed_inputs = self.input_progress.begin_command();

            self.flush_pending_safe_funcalls();
            sync_current_buffer_to_selected_window(self);

            // Save the outgoing `current-prefix-arg` into
            // `last-prefix-arg` before reading the next command.
            //
            // Do NOT also transfer `prefix-arg` here: GNU's Lisp
            // `command-execute` does that itself right before it
            // calls `call-interactively`, and prefix commands such as
            // `universal-argument` rely on `prefix-arg` surviving
            // until that point.
            let outgoing_prefix_arg = self
                .eval_symbol_by_id(CommandLoopSymbol::CurrentPrefixArg.id())
                .unwrap_or(Value::NIL);
            self.assign_by_id(CommandLoopSymbol::LastPrefixArg.id(), outgoing_prefix_arg);

            // Reset this-command and related variables before reading
            // the next key sequence.  GNU keyboard.c:1416-1419 clears
            // Vthis_command, Vreal_this_command, Vthis_original_command,
            // and Vthis_command_keys_shift_translated to nil so that idle
            // timer callbacks (e.g. which-key) running inside
            // read_key_sequence observe (null this-command) => t.
            self.assign_by_id(CommandLoopSymbol::ThisCommand.id(), Value::NIL);
            self.assign_by_id(CommandLoopSymbol::RealThisCommand.id(), Value::NIL);
            self.assign_by_id(CommandLoopSymbol::ThisOriginalCommand.id(), Value::NIL);

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
            let read_specpdl_count = self.specpdl().len();
            self.try_specbind_or_unwind_to(
                read_specpdl_count,
                CommandLoopSymbol::InhibitQuit.id(),
                Value::T,
            )?;
            let read_result = self.read_command_key_sequence_with_options(
                neovm_value::system_vocab::input::ReadKeySequenceOptions::for_command_loop(),
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
                self.push_vm_frame_root(binding.original());
                self.push_vm_frame_root(binding.executed());
            }
            let unwind_result = match read_result.as_ref() {
                Ok(_) => self.unbind_to_with_result(read_specpdl_count, Ok(Value::NIL)),
                Err(flow) => self.unbind_to_with_result(read_specpdl_count, Err(flow.clone())),
            };
            self.restore_vm_roots(read_root_scope);
            unwind_result?;

            let (keys, binding) = match read_result? {
                crate::keyboard::CommandKeySequenceRead::Command { keys, binding } => {
                    (keys, Some(binding))
                }
                crate::keyboard::CommandKeySequenceRead::End(_) => (Vec::new(), None),
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
            let throw_on_input = self.variable_value_or_nil(self.throw_on_input_symbol);
            let quit_flag = self.quit_flag_value();
            let is_while_no_input =
                !throw_on_input.is_nil() && equal_value(&quit_flag, &throw_on_input, 0);
            let quit_pending = !quit_flag.is_nil() || self.quit_requested.is_requested();
            if quit_pending && !is_while_no_input {
                self.set_quit_flag_value(Value::NIL);
                self.quit_requested.clear();
                if keys.is_empty() {
                    let quit_char = Value::fixnum(self.quit_char());
                    self.push_unread_command_event(quit_char);
                }
            }

            sync_current_buffer_to_selected_window(self);

            let Some(binding) = binding else {
                // The read reached a typed input boundary: GNU's
                // `i == 0` return from `read_key_sequence`.
                self.assign_by_id(CommandLoopSymbol::ThisCommand.id(), Value::NIL);
                return Ok(Value::NIL);
            };

            // Start only after the complete key sequence has arrived.  This
            // excludes the user's inter-key think time while retaining the
            // command-loop work below.  Command remapping already happened
            // inside read_key_sequence, where GNU does it.
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
            if binding.executed().is_nil() {
                let desc: Vec<String> = keys.iter().map(|v| format!("{:?}", v)).collect();
                tracing::info!("Undefined key sequence: {}", desc.join(" "));
            }

            // GNU `keyboard.c:1464-1507`: `read_key_sequence` already applied
            // the one level of command remapping, so `this-command` and
            // `real-this-command` are the command it produced; asking again
            // would remap a remapped command. Do not touch
            // `real-last-command` here: GNU updates it only after the
            // preceding command's `post-command-hook` and leaves that value
            // visible to the next `pre-command-hook` (keyboard.c's
            // `kset_real_last_command` near the command-loop finalize tail).
            let remapped = binding.executed();
            self.assign_by_id(CommandLoopSymbol::RealThisCommand.id(), remapped);
            self.assign_by_id(CommandLoopSymbol::ThisCommand.id(), remapped);
            let mut command_observation = command_observation_start.map(|start| {
                UserCommandObservation::begin(
                    start,
                    UserCommandIdentity::new(
                        self.context_instance_id(),
                        self.command_loop().recursive_depth,
                        Self::command_keys_for_log(&keys),
                        Self::command_value_for_log(binding.original()),
                        Self::command_value_for_log(remapped),
                        self.frame_manager().selected_frame().map(|frame| frame.id),
                        self.edit.buffers.current_buffer_id(),
                    ),
                )
            });

            let input_measurement = neomacs_display_protocol::input_latency::CommandInputs::begin();

            // Finding 2: this-original-command stays at the original
            // (pre-remap) command for the duration of the iteration
            // unless a pre-command-hook explicitly cleared it.
            if self
                .eval_symbol_by_id(CommandLoopSymbol::ThisOriginalCommand.id())
                .unwrap_or(Value::NIL)
                .is_nil()
            {
                self.assign_by_id(
                    CommandLoopSymbol::ThisOriginalCommand.id(),
                    binding.original(),
                );
            }

            if let Some(last) = keys.last() {
                self.assign_by_id(CommandLoopSymbol::LastCommandEvent.id(), *last);
            }
            tracing::debug!(
                "command_loop_1: binding={} current_buffer={:?} active_minibuffer_window={:?}",
                self.this_command_name_for_log(),
                self.edit.buffers.current_buffer_id(),
                self.active_minibuffer_window()
            );

            // GNU `command_loop_1` resets `Vdeactivate_mark = Qnil` at the top
            // of each iteration (keyboard.c:1471), before `pre-command-hook`, so
            // the flag reflects only the command about to run; the post-command
            // block then deactivates the region iff a command (re)set it.
            // Without this per-command reset a stale buffer-local `deactivate-mark`
            // (left by an earlier buffer-modifying command such as self-insert)
            // leaks forward and immediately kills a freshly `set-mark`ed region,
            // so e.g. `C-SPC M-> M-;` sees no active region.
            self.assign_by_id(CommandLoopSymbol::DeactivateMark.id(), Value::NIL);

            // GNU `keyboard.c:1500-1506` records the command pseudo-event
            // before `pre-command-hook`, so `recent-keys 'include-cmds` can
            // describe the command currently being run.
            self.record_recent_command(remapped);

            // Run pre-command-hook via safe-run-hooks so a broken
            // hook function is removed instead of re-firing on every
            // command. Finding 7 — GNU `keyboard.c:1510`
            // (`safe_run_hooks_maybe_narrowed (Qpre_command_hook, ...)`).
            self.safe_run_hook_if_bound(CommandLoopHook::PreCommand.id())?;

            // GNU `keyboard.c:1530-1534` adds undo boundaries here, after
            // `pre-command-hook` and before command execution, so the
            // previous command's edits are grouped before the next command
            // mutates any buffer state.
            if self
                .obarray()
                .fboundp_id(CommandLoopSymbol::UndoAutoAddBoundary.id())
            {
                let _ = self.apply(
                    Value::from_sym_id(CommandLoopSymbol::UndoAutoAddBoundary.id()),
                    vec![],
                );
            }
            if let Some(current_id) = self.edit.buffers.current_buffer_id() {
                let _ = self
                    .edit
                    .buffers
                    .record_undo_point_before_command(current_id);
            }

            // GNU `keyboard.c:1477-1486` snapshots prev-buffer/modiff and
            // `last_point_position = PT` here, then resets
            // `disable-point-adjustment` to nil so a command must opt back in
            // to suppress the post-command point adjustment.
            let apfp_prev_buffer = self.edit.buffers.current_buffer_id();
            let apfp_last_pt = apfp_prev_buffer.map(|id| self.apfp_point(id)).unwrap_or(0);
            let apfp_prev_modiff = apfp_prev_buffer
                .and_then(|id| self.edit.buffers.get(id))
                .map(|b| b.modified_tick())
                .unwrap_or(0);
            self.assign_by_id(CommandLoopSymbol::DisablePointAdjustment.id(), Value::NIL);

            // Execute the remapped command, matching GNU's
            // `calln (Qcommand_execute, Vthis_command)`.
            let command_execution_start = command_observation
                .as_ref()
                .map(UserCommandObservation::begin_execution);
            input_measurement.start(|frame| self.input_latency_viewport(frame));
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
            sync_current_buffer_to_selected_window(self);

            // GNU does not recover inside `command_loop_1'. Any non-local
            // result unwinds the unfinished command (so post-command hooks and
            // history finalization do not run) and lets `command_loop_2' make
            // the exhaustive Flow decision. In particular, a Signal must not
            // be flattened into a plain echo-area `message' here.
            exec_result?;

            // Run post-command-hook via safe-run-hooks (Finding 7).
            // GNU `command_loop_1` calls `safe_run_hooks (Qpost_command_hook)`
            // at keyboard.c:1563.
            self.safe_run_hook_if_bound(CommandLoopHook::PostCommand.id())?;
            input_measurement.complete(|frame| self.input_latency_viewport(frame));

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
                self.current_message().is_some() && self.active_minibuffer_window_id().is_none();

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
                let cur_buffer = self.edit.buffers.current_buffer_id();
                let win_buffer = self
                    .frame_manager()
                    .selected_frame()
                    .and_then(|f| f.selected_window())
                    .and_then(|w| w.buffer_id());
                let cur_pt = cur_buffer.map(|id| self.apfp_point(id)).unwrap_or(0);
                let disabled = self
                    .eval_symbol_by_id(CommandLoopSymbol::DisablePointAdjustment.id())
                    .unwrap_or(Value::NIL)
                    .is_truthy()
                    || self
                        .eval_symbol_by_id(CommandLoopSymbol::GlobalDisablePointAdjustment.id())
                        .unwrap_or(Value::NIL)
                        .is_truthy();
                if cur_buffer.is_some()
                    && cur_buffer == apfp_prev_buffer
                    && cur_buffer == win_buffer
                    && apfp_last_pt != cur_pt
                    && !disabled
                {
                    let modified = cur_buffer
                        .and_then(|id| self.edit.buffers.get(id))
                        .map(|b| b.modified_tick())
                        .unwrap_or(apfp_prev_modiff)
                        != apfp_prev_modiff;
                    self.adjust_point_for_property(apfp_last_pt, modified)?;
                    // Re-align the selected window with the adjusted point.
                    sync_current_buffer_to_selected_window(self);
                }
            }

            // GNU updates the command-history variables after
            // post-command-hook (`keyboard.c`: kset_last_command and
            // kset_real_last_command near the bottom of command_loop_1).
            // Undo uses `last-command` to decide whether a following undo
            // continues the same undo chain or starts a redo.
            if let Ok(this_cmd) = self.eval_symbol_by_id(CommandLoopSymbol::ThisCommand.id()) {
                self.assign_by_id(CommandLoopSymbol::LastCommand.id(), this_cmd);
            }
            let real_this = self
                .eval_symbol_by_id(CommandLoopSymbol::RealThisCommand.id())
                .unwrap_or(Value::NIL);
            self.assign_by_id(CommandLoopSymbol::RealLastCommand.id(), real_this);

            // GNU records the real command as last-repeatable-command for
            // ordinary key events.
            let last_event = self
                .eval_symbol_by_id(CommandLoopSymbol::LastCommandEvent.id())
                .unwrap_or(Value::NIL);
            if !last_event.is_cons() {
                self.assign_by_id(CommandLoopSymbol::LastRepeatableCommand.id(), real_this);
            }

            // Reset this-original-command for the next iteration so
            // a fresh command starts the cycle clean (mirroring
            // GNU's clear at the bottom of command_loop_1).
            self.assign_by_id(CommandLoopSymbol::ThisOriginalCommand.id(), Value::NIL);

            if self.command_loop().keyboard.kboard.defining_kbd_macro
                && self
                    .eval_symbol_by_id(CommandLoopSymbol::PrefixArg.id())
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
    fn command_loop_1_entry_prologue(&mut self) -> EvalResult {
        self.assign_by_id(CommandLoopSymbol::PrefixArg.id(), Value::NIL);
        self.assign_by_id(CommandLoopSymbol::LastPrefixArg.id(), Value::NIL);
        self.assign_by_id(CommandLoopSymbol::DeactivateMark.id(), Value::NIL);

        // GNU `command_loop_1` clears `this_command_key_count` and
        // `this_single_command_key_start` before its initial
        // `post-command-hook` (keyboard.c:1316-1327).  In a recursive
        // minibuffer command loop, the outer command's translated key
        // sequence is therefore hidden from that hook.  Keep the raw sequence:
        // GNU does not clear `raw_keybuf_count` until immediately before
        // `read_key_sequence` (keyboard.c:1416-1424).
        self.set_translated_command_keys(Vec::new());

        if self
            .eval_symbol_by_id(CommandLoopSymbol::MemoryFull.id())
            .unwrap_or(Value::NIL)
            .is_nil()
        {
            self.safe_run_hook_if_bound(CommandLoopHook::PostCommand.id())?;

            if self
                .eval_symbol_by_id(CommandLoopSymbol::DelayedWarningsList.id())
                .unwrap_or(Value::NIL)
                .is_truthy()
            {
                self.safe_run_hook_if_bound(CommandLoopHook::DelayedWarnings.id())?;
            }
        }

        let this_command = self
            .eval_symbol_by_id(CommandLoopSymbol::ThisCommand.id())
            .unwrap_or(Value::NIL);
        self.assign_by_id(CommandLoopSymbol::LastCommand.id(), this_command);

        let real_this_command = self
            .eval_symbol_by_id(CommandLoopSymbol::RealThisCommand.id())
            .unwrap_or(Value::NIL);
        self.assign_by_id(CommandLoopSymbol::RealLastCommand.id(), real_this_command);

        let last_command_event = self
            .eval_symbol_by_id(CommandLoopSymbol::LastCommandEvent.id())
            .unwrap_or(Value::NIL);
        if !last_command_event.is_cons() {
            self.assign_by_id(
                CommandLoopSymbol::LastRepeatableCommand.id(),
                real_this_command,
            );
        }

        Ok(Value::NIL)
    }

    /// Per-iteration `auto-save-interval` check, mirroring GNU
    /// `keyboard.c:1491-1506`. Keyboard audit Finding 9.
    fn command_loop_1_maybe_auto_save(&mut self) {
        let interval = match self
            .eval_symbol_by_id(CommandLoopSymbol::AutoSaveInterval.id())
            .ok()
        {
            Some(v) => match v.as_fixnum() {
                Some(n) if n > 0 => n,
                _ => return,
            },
            None => return,
        };
        let threshold = interval.max(20);
        let current = self.num_nonmacro_input_events();
        let last = self.command_loop().last_auto_save_input_events;
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

    /// Approximation of GNU `detect_input_pending_run_timers (0)`
    /// used by the command-loop auto-save gate. Returns true when
    /// there is already-queued input that should run before an
    /// expensive auto-save.
    fn input_pending_for_auto_save(&mut self) -> bool {
        self.service_leading_internal_frontend_events();
        if self.peek_unread_command_event().is_some() {
            return true;
        }
        self.has_pending_frontend_input_with_configured_filter()
    }

    /// Dispatch the current `this-command` via GNU's
    /// `command-execute` command-loop path.
    fn dispatch_command_in_loop(&mut self, command: Value) -> EvalResult {
        // Re-resolve `this-command` from the obarray so a
        // pre-command-hook that mutated the symbol takes effect.
        let cmd = self
            .eval_symbol_by_id(CommandLoopSymbol::ThisCommand.id())
            .unwrap_or(command);
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
            if self.obarray().fboundp_id(CommandLoopSymbol::Undefined.id()) {
                return self.apply(
                    Value::from_sym_id(CommandLoopSymbol::Undefined.id()),
                    vec![],
                );
            }
            let _ = super::super::builtins::dispatch_builtin(self, "ding", vec![]);
            return Ok(Value::NIL);
        }
        self.apply(
            Value::from_sym_id(CommandLoopSymbol::CommandExecute.id()),
            vec![cmd],
        )
    }

    fn execute_kbd_macro_iteration_via_command_loop(&mut self) -> EvalResult {
        let saved_running = self.command_loop().running;
        if !saved_running {
            self.command_loop_mut().running = true;
        }
        self.assign_by_id(CommandLoopSymbol::PrefixArg.id(), Value::NIL);
        let result = self.command_loop_2();
        if !saved_running && self.command_loop().running {
            self.command_loop_mut().running = false;
        }
        result
    }

    fn update_active_region_selection_after_command(&mut self) -> EvalResult {
        if self
            .eval_symbol_by_id(crate::emacs_core::intern::WellKnown::MarkActive.id())
            .unwrap_or(Value::NIL)
            .is_nil()
        {
            return Ok(Value::NIL);
        }

        let transient_mark_mode = self
            .eval_symbol_by_id(crate::emacs_core::intern::WellKnown::TransientMarkMode.id())
            .unwrap_or(Value::NIL);
        if transient_mark_mode == Value::symbol(&IDENTITY_SYMBOL) {
            self.assign("transient-mark-mode", Value::NIL);
        } else if transient_mark_mode == Value::symbol(&ONLY_SYMBOL) {
            self.assign_by_id(TRANSIENT_MARK_MODE.id(), Value::symbol(&IDENTITY_SYMBOL));
        }

        if !self
            .eval_symbol_by_id(CommandLoopSymbol::DeactivateMark.id())
            .unwrap_or(Value::NIL)
            .is_nil()
        {
            let _ = self.apply(
                Value::from_sym_id(CommandLoopSymbol::DeactivateMark.id()),
                vec![],
            )?;
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
            .eval_symbol_by_id(crate::emacs_core::intern::WellKnown::SelectActiveRegions.id())
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

        let this_command = self
            .eval_symbol_by_id(CommandLoopSymbol::ThisCommand.id())
            .unwrap_or(Value::NIL);
        let inhibited_commands = self
            .eval_symbol_by_id(
                crate::emacs_core::intern::WellKnown::SelectionInhibitUpdateCommands.id(),
            )
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
            .eval_symbol_by_id(crate::emacs_core::intern::WellKnown::RegionExtractFunction.id())
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

    fn this_command_name_for_log(&self) -> String {
        self.eval_symbol_by_id(CommandLoopSymbol::ThisCommand.id())
            .map(|value| format!("{}", value))
            .unwrap_or_else(|_| "<unbound>".to_string())
    }

    fn command_value_for_log(value: Value) -> String {
        value
            .as_bare_symbol_name()
            .map(str::to_owned)
            .unwrap_or_else(|| crate::emacs_core::print::print_value(&value))
    }

    fn command_keys_for_log(keys: &[Value]) -> String {
        keys.iter()
            .map(|key| {
                crate::emacs_core::keyboard::pure::describe_single_key_value(
                    key,
                    false,
                    crate::tagged::SymbolMode::Bare,
                )
                .map(|description| crate::emacs_core::emacs_char::to_utf8_lossy(&description))
                .unwrap_or_else(|_| crate::emacs_core::print::print_value(key))
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    #[tracing::instrument(skip_all, fields(depth = self.command_loop().recursive_depth, has_input = self.input_rx().as_ref().is_some()))]
    fn recursive_edit_inner(&mut self) -> EvalResult {
        self.run_exit_wrapped_command_loop(true)
    }

    #[tracing::instrument(skip_all, fields(depth = self.command_loop().recursive_depth, has_input = self.input_rx().as_ref().is_some()))]
    fn minibuffer_command_loop_inner(&mut self) -> EvalResult {
        self.run_exit_wrapped_command_loop(false)
    }

    /// Interrupt on input for GNU-style `throw-on-input` users such as
    /// `while-no-input`, while preserving the input event for later reads.
    fn interrupt_for_input_event_if_requested(
        &mut self,
        event: crate::keyboard::InputEvent,
    ) -> Result<bool, Flow> {
        let throw_on_input = self.variable_value_or_nil(self.throw_on_input_symbol);
        if throw_on_input.is_nil() {
            return Ok(false);
        }

        if self.inhibit_quit.is_truthy() {
            return Ok(false);
        }

        self.command_loop_mut()
            .keyboard
            .pending_input_events
            .push_front(event);
        self.set_quit_flag_value(throw_on_input);
        self.maybe_quit()?;
        Ok(true)
    }
}

/// Initialize host termination policy after command-line mode selection.
///
/// GNU installs its fatal-signal handlers before the command loop starts;
/// the OS-signal service lives above eval, so this host initializer is a
/// core-side free function (STAGE4 design §5, R0).
pub fn initialize_termination_signals(noninteractive: bool) {
    crate::emacs_core::os_signal::install_termination(noninteractive);
}

/// Enter a recursive edit level from the host boundary.
///
/// Mirrors GNU `Frecursive_edit` (keyboard.c:772): increments recursive
/// depth, enters the core command loop, decrements on exit.  The loop body
/// lives above eval, so this entry is a core-side free function; the
/// binary calls it with the evaluator context.
pub fn recursive_edit(ctx: &mut Context) -> Result<(), String> {
    match ctx.recursive_edit_inner() {
        Ok(_) => Ok(()),
        // kill-emacs unwinds the recursive edit; the pending shutdown
        // request carries the exit code to the caller.
        Err(flow) if flow.is_shutdown() => Ok(()),
        Err(flow) => Err(crate::emacs_core::print::format_flow_with_eval(ctx, &flow)),
    }
}

/// Re-align the evaluator's current buffer and the selected window's point
/// (the GNU `command_loop_1` `set_buffer_internal` step).  The state is
/// eval-owned; the behaviour stays in the core command layer (stage-4 N17).
fn sync_current_buffer_to_selected_window(eval: &mut crate::emacs_core::eval::Context) {
    let Some(frame_id) = eval.frame_manager().selected_frame().map(|frame| frame.id) else {
        return;
    };
    let (frames, buffers) = eval.frame_manager_and_buffers_mut();
    crate::emacs_core::window_cmds::remember_selected_window_point_in_state(
        frames, buffers, frame_id,
    );
    crate::emacs_core::window_cmds::sync_selected_window_buffer_in_state(frames, buffers, frame_id);
    let _ = eval.sync_current_buffer_runtime_state();
}
