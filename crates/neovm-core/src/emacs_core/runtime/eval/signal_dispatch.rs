//! Signal dispatch: the condition-handler stack, catch and handler lookup, signal delivery, backtrace rendering, and debugger entry (GNU eval.c signal_or_quit / find_handler_clause).
//!
//! Moved out of `eval/mod.rs` unchanged; a child module of `eval` so it keeps
//! the same view of `Context` and the parent's private items (`use super::*`).

use super::*;

impl Context {
    pub(crate) fn push_condition_frame(&mut self, frame: ConditionFrame) {
        self.condition_stack.push(frame);
    }

    pub(crate) fn pop_condition_frame(&mut self) -> Option<ConditionFrame> {
        self.condition_stack.pop()
    }

    pub(crate) fn truncate_condition_stack(&mut self, len: usize) {
        self.condition_stack.truncate(len);
    }

    /// Rebase the `stack_len` of the topmost `count` bytecode catch/condition-case
    /// handlers from FRAME-RELATIVE to ABSOLUTE `bc_buf` positions by adding
    /// `frame_base`.
    ///
    /// The JIT `push-catch`/`push-condition-case` shims record `stack_len` as the
    /// native model operand-stack DEPTH (frame-relative — a native frame keeps no
    /// operands on `bc_buf`), whereas the interpreter records the ABSOLUTE
    /// `bc_buf.len()`. When a native frame deopts and resumes via
    /// [`Vm::run_resumed_frame`], its operands are seeded at `bc_buf[frame_base..]`,
    /// so its transferred handlers must be rebased to absolute — otherwise a later
    /// throw/signal caught by such a handler would `bc_buf.truncate(relative_len)`
    /// and collapse the caller's live operand stack (the native frame's handlers
    /// are exactly the topmost `count` Vm catch/condition-case frames).
    pub(crate) fn rebase_resumed_vm_handler_stack_lens(&mut self, count: usize, frame_base: usize) {
        if count == 0 || frame_base == 0 {
            return;
        }
        let mut remaining = count;
        for frame in self.condition_stack.iter_mut().rev() {
            if remaining == 0 {
                break;
            }
            let resume = match frame {
                ConditionFrame::Catch { resume, .. }
                | ConditionFrame::ConditionCase { resume, .. } => resume,
                _ => continue,
            };
            match resume {
                ResumeTarget::VmCatch { stack_len, .. }
                | ResumeTarget::VmConditionCase { stack_len, .. } => {
                    *stack_len += frame_base;
                    remaining -= 1;
                }
                _ => continue,
            }
        }
    }

    pub(crate) fn condition_stack_len(&self) -> usize {
        self.condition_stack.len()
    }

    pub(crate) fn allocate_resume_id(&mut self) -> u64 {
        let resume_id = self.next_resume_id;
        self.next_resume_id += 1;
        resume_id
    }

    pub(crate) fn matching_catch_resume(&self, tag: &Value) -> Option<ResumeTarget> {
        if tag.is_nil() {
            return None;
        }

        self.condition_stack
            .iter()
            .rev()
            .find_map(|frame| match frame {
                ConditionFrame::Catch {
                    tag: catch_tag,
                    resume,
                } if eq_value(catch_tag, tag) => Some(resume.clone()),
                _ => None,
            })
    }

    pub(crate) fn has_active_catch(&self, tag: &Value) -> bool {
        self.matching_catch_resume(tag).is_some()
    }

    pub(crate) fn has_active_condition_handler_for_signal(&self, sig: &SignalData) -> bool {
        self.condition_stack.iter().rev().any(|frame| match frame {
            ConditionFrame::ConditionCase { conditions, .. }
            | ConditionFrame::HandlerBind { conditions, .. } => {
                crate::emacs_core::errors::signal_matches_condition_value_sym(
                    &self.obarray,
                    sig.symbol,
                    conditions,
                )
            }
            _ => false,
        })
    }

    pub(crate) fn dispatch_signal_if_needed(
        &mut self,
        sig: Box<SignalData>,
    ) -> Result<Box<SignalData>, Flow> {
        if sig.search_complete {
            return Ok(sig);
        }
        self.dispatch_signal(*sig).map(Box::new)
    }

    /// `#[inline]`: this runs on every call return; the Ok arm is a pure
    /// pass-through that should vanish into the caller instead of paying an
    /// out-of-line 24-byte Result round trip per call (measured ~2.3% flat
    /// of a call-heavy interpreter benchmark as a standalone function).
    #[inline]
    pub(crate) fn dispatch_signal_result_if_needed(&mut self, result: EvalResult) -> EvalResult {
        match result {
            Err(Flow::Signal(sig)) => match self.dispatch_signal_if_needed(sig) {
                Ok(dispatched) => Err(Flow::Signal(dispatched)),
                Err(flow) => Err(flow),
            },
            other => other,
        }
    }

    pub(super) fn dispatch_signal(&mut self, mut sig: SignalData) -> Result<SignalData, Flow> {
        self.run_signal_hook(&sig)?;
        sig = self.canonicalize_signal_symbol(sig);

        let mut idx = self.condition_stack.len();
        let mut seen_condition_entries = 0usize;

        while let Some(next_idx) = idx.checked_sub(1) {
            idx = next_idx;
            match self.condition_stack[idx].clone() {
                ConditionFrame::Catch { .. } => {}
                ConditionFrame::SkipConditions { remaining } => {
                    let mut to_skip = remaining;
                    while idx > 0 && to_skip > 0 {
                        idx -= 1;
                        if matches!(
                            self.condition_stack[idx],
                            ConditionFrame::ConditionCase { .. }
                                | ConditionFrame::HandlerBind { .. }
                        ) {
                            to_skip -= 1;
                        }
                    }
                }
                ConditionFrame::ConditionCase { conditions, resume } => {
                    seen_condition_entries += 1;
                    if crate::emacs_core::errors::signal_matches_condition_value_sym(
                        &self.obarray,
                        sig.symbol,
                        &conditions,
                    ) {
                        self.maybe_call_debugger_for_signal(&sig, Some(&conditions))?;
                        sig.selected_resume = Some(resume);
                        sig.search_complete = true;
                        return Ok(sig);
                    }
                }
                ConditionFrame::HandlerBind {
                    conditions,
                    handler,
                    mute_span,
                } => {
                    seen_condition_entries += 1;
                    if !crate::emacs_core::errors::signal_matches_condition_value_sym(
                        &self.obarray,
                        sig.symbol,
                        &conditions,
                    ) {
                        continue;
                    }

                    let specpdl_root_scope = self.save_specpdl_roots();
                    for value in &sig.data {
                        self.push_specpdl_root(*value);
                    }
                    if let Some(raw) = &sig.raw_data {
                        self.push_specpdl_root(*raw);
                    }

                    self.push_condition_frame(ConditionFrame::SkipConditions {
                        remaining: seen_condition_entries + mute_span,
                    });

                    let handler_result = self.apply(handler, vec![make_signal_binding_value(&sig)]);

                    match handler_result {
                        Ok(_) => {
                            self.pop_condition_frame();
                            self.restore_specpdl_roots(specpdl_root_scope);
                            continue;
                        }
                        Err(Flow::Signal(next_sig)) => {
                            let dispatched =
                                self.dispatch_signal_if_needed(next_sig).map(|sig| *sig);
                            self.pop_condition_frame();
                            self.restore_specpdl_roots(specpdl_root_scope);
                            return dispatched;
                        }
                        Err(flow @ Flow::Throw(_)) => {
                            self.pop_condition_frame();
                            self.restore_specpdl_roots(specpdl_root_scope);
                            return Err(flow);
                        }
                        Err(flow @ (Flow::ThreadBlocked(_) | Flow::Shutdown(_))) => {
                            self.pop_condition_frame();
                            self.restore_specpdl_roots(specpdl_root_scope);
                            return Err(flow);
                        }
                    }
                }
            }
        }

        self.maybe_call_debugger_for_signal(&sig, None)?;
        // No handler matched: this signal will propagate to the command loop.
        // Capture the live Lisp backtrace NOW, while the specpdl is still intact
        // (unwinding happens as `Ok(sig)` propagates up), so the command loop's
        // error log can show WHERE it was signaled without the reporter needing
        // `debug-on-error`. Gated on debug-level tracing — the default filter is
        // `warn`, so production only pays a cheap `enabled!` check, and only
        // truly-uncaught signals are ever rendered.
        let captured_backtrace = if tracing::enabled!(tracing::Level::DEBUG) {
            Some(self.render_uncaught_signal_backtrace(64))
        } else {
            None
        };
        self.last_uncaught_signal_backtrace = captured_backtrace;
        sig.search_complete = true;
        sig.selected_resume = None;
        Ok(sig)
    }

    /// Render a compact snapshot of the live Lisp backtrace (innermost frame
    /// first), like GNU `backtrace`, for the command-loop error log. Bounded to
    /// `max_frames`. Only invoked under a debug-tracing gate — it prints every
    /// live frame's function and arguments.
    /// Render the current Lisp backtrace for diagnostics (public alias of the
    /// uncaught-signal renderer, used by env-gated observability hooks).
    pub(crate) fn render_lisp_backtrace(&self, max_frames: usize) -> String {
        self.render_uncaught_signal_backtrace(max_frames)
    }

    pub(super) fn render_uncaught_signal_backtrace(&self, max_frames: usize) -> String {
        let mut lines: Vec<String> = Vec::new();
        for entry in self.specpdl.iter().rev() {
            let Some((function, args, _, _)) = self.backtrace_entry_values(entry) else {
                continue;
            };
            if lines.len() >= max_frames {
                lines.push("    ...".to_string());
                break;
            }
            let fn_str = crate::emacs_core::print_value_with_eval(self, &function);
            let arg_strs: Vec<String> = args
                .iter()
                .map(|v| crate::emacs_core::print_value_with_eval(self, v))
                .collect();
            lines.push(if arg_strs.is_empty() {
                format!("    ({fn_str})")
            } else {
                format!("    ({fn_str} {})", arg_strs.join(" "))
            });
        }
        lines.join("\n")
    }

    pub(super) fn run_signal_hook(&mut self, sig: &SignalData) -> Result<(), Flow> {
        if sig.suppress_signal_hook {
            return Ok(());
        }

        let hook = self
            .obarray
            .symbol_value("signal-hook-function")
            .copied()
            .unwrap_or(Value::NIL);
        if hook.is_nil() {
            return Ok(());
        }

        self.apply(
            hook,
            vec![
                Value::from_sym_id(sig.symbol),
                signal_hook_payload_value(sig),
            ],
        )
        .map(|_| ())
    }

    pub(super) fn canonicalize_signal_symbol(&self, sig: SignalData) -> SignalData {
        if sig.symbol == error_symbol() || sig.symbol == quit_symbol() {
            return sig;
        }
        // GNU `signal_or_quit` reads `error-conditions` directly from the
        // signalled symbol object (`Fget (real_error_symbol, Qerror_conditions)`,
        // src/eval.c:1959), so an *uninterned* error symbol created via
        // `make-symbol` and given conditions by `define-error` is honoured.
        // Looking the property up by name (which interns) would resolve to a
        // *different*, interned symbol with no conditions and spuriously
        // canonicalize to "Invalid error symbol".  Use identity instead.
        if self
            .obarray
            .get_property_id(sig.symbol, error_conditions_symbol())
            .is_some()
        {
            return sig;
        }

        SignalData::new(
            error_symbol(),
            vec![
                Value::string("Invalid error symbol"),
                Value::from_sym_id(sig.symbol),
            ],
            None,
            sig.suppress_signal_hook,
        )
    }

    pub(super) fn maybe_call_debugger_for_signal(
        &mut self,
        sig: &SignalData,
        matched_clause: Option<&Value>,
    ) -> Result<(), Flow> {
        if self
            .obarray
            .symbol_value("inhibit-debugger")
            .is_some_and(|value| !value.is_nil())
        {
            return Ok(());
        }

        let debug_on_signal = self
            .obarray
            .symbol_value("debug-on-signal")
            .is_some_and(|value| !value.is_nil());
        let should_consider_debugger = debug_on_signal
            || matched_clause.is_none()
            || matched_clause.is_some_and(condition_value_contains_debug);
        if !should_consider_debugger {
            return Ok(());
        }

        let conditions = self.signal_conditions_value(sig);
        let debug_setting = if crate::emacs_core::errors::signal_matches_condition_value_sym(
            &self.obarray,
            sig.symbol,
            &Value::from_sym_id(quit_symbol()),
        ) {
            self.obarray
                .symbol_value("debug-on-quit")
                .copied()
                .unwrap_or(Value::NIL)
        } else {
            self.obarray
                .symbol_value("debug-on-error")
                .copied()
                .unwrap_or(Value::NIL)
        };
        if !wants_debugger(&debug_setting, &conditions) {
            return Ok(());
        }
        if self.skip_debugger(sig, &conditions)? {
            return Ok(());
        }
        // GNU's last conjunct, and it is last there too: "See commentary on
        // definition of `internal-when-entered-debugger'" (`src/eval.c:2210-2212`).
        // A debugger that signals must not re-enter itself, so one entry per
        // non-macro input event is the budget -- which in batch, where no such
        // event ever arrives, is one entry per session.
        if !self.debugger_reentry_is_permitted() {
            return Ok(());
        }

        self.call_debugger_for_signal(sig)
    }

    pub(super) fn signal_conditions_value(&self, sig: &SignalData) -> Value {
        // Read `error-conditions' by identity so an uninterned error symbol
        // (created via `make-symbol' + `define-error') yields its real
        // condition list for `condition-case' clause matching, instead of the
        // empty/`(SYMBOL)' fallback a name-based lookup of a different interned
        // symbol would give.
        self.obarray
            .get_property_id(sig.symbol, error_conditions_symbol())
            .unwrap_or_else(|| Value::list(vec![Value::from_sym_id(sig.symbol)]))
    }

    pub(super) fn skip_debugger(
        &mut self,
        sig: &SignalData,
        conditions: &Value,
    ) -> Result<bool, Flow> {
        let ignored = self
            .obarray
            .symbol_value("debug-ignored-errors")
            .copied()
            .unwrap_or(Value::NIL);
        let Some(entries) = list_to_vec(&ignored) else {
            return Ok(false);
        };
        if entries.is_empty() {
            return Ok(false);
        }

        let mut error_message = None;
        let error_data = make_signal_binding_value(sig);
        let signal_conditions = list_to_vec(conditions).unwrap_or_else(|| vec![*conditions]);

        for entry in entries {
            if entry.is_string() {
                let message = if let Some(message) = error_message {
                    message
                } else {
                    let rendered = crate::emacs_core::errors::builtin_error_message_string(
                        self,
                        vec![error_data],
                    )?;
                    error_message = Some(rendered);
                    rendered
                };

                let current_buffer = self.buffers.current_buffer();
                let syntax_table =
                    current_buffer.map(crate::emacs_core::syntax::SyntaxTable::for_buffer);
                let category_table = Some(
                    crate::emacs_core::category::active_category_table_for_buffer(current_buffer)?,
                );
                let word_boundary = builtins::search::current_word_boundary_lookup(self);
                let syntax_properties = builtins::search::current_string_match_syntax_properties(
                    self,
                    &self.obarray,
                    &self.buffers,
                    Some(&message),
                );
                if builtins::search::builtin_string_match_p_with_case_fold(
                    false,
                    None,
                    syntax_table.as_ref(),
                    category_table,
                    word_boundary,
                    syntax_properties,
                    &[entry, message],
                )?
                .as_fixnum()
                .is_some()
                {
                    return Ok(true);
                }
                continue;
            }

            if signal_conditions.contains(&entry) {
                return Ok(true);
            }
        }

        Ok(false)
    }

    /// Whether an uncaught command signal is something GNU would show the
    /// user and forget, or something worth a diagnostic.
    ///
    /// GNU's command loop does not distinguish: `cmd_error_internal'
    /// (src/keyboard.c:1030-1047, emacs-31.0.90) hands every signal to
    /// `command-error-function' and never logs.  The one place GNU does rank
    /// signals is the debugger gate, `skip_debugger' (src/eval.c:2146-2180):
    /// a condition or message matched by `debug-ignored-errors' -- by default
    /// `beginning-of-line', `end-of-line', `end-of-buffer', `buffer-read-only',
    /// `user-error', ... (lisp/bindings.el:1038-1046), and whatever packages
    /// push there, such as evil's `end-of-line' -- is routine and must not
    /// interrupt the user.  A quit is routine by construction
    /// (`signal_quit_p', src/eval.c:2181-2190).  This port's command-loop
    /// log follows the same ranking, so `C-g` and "End of line" are not
    /// errors in the log while a void function still is.
    ///
    /// Two departures from GNU, both deliberate.  GNU consults
    /// `skip_debugger' only when the debugger is otherwise wanted
    /// (`maybe_call_debugger', src/eval.c:2206-2213); the log has no such
    /// gate, so the ignore list is evaluated on every uncaught command
    /// signal.  And this runs inside `command_loop_2', GNU's sole recovery
    /// owner (`internal_condition_case (command_loop_1, ..., cmd_error)'),
    /// where nothing may signal: a string entry in `debug-ignored-errors'
    /// that is not a valid regexp makes the matcher signal `invalid-regexp',
    /// and that must classify the original error, not unwind the command
    /// loop.  So this is infallible: a matcher failure is a `Diagnostic'.
    pub(super) fn command_error_severity(&mut self, sig: &SignalData) -> CommandErrorSeverity {
        if crate::emacs_core::errors::signal_matches_condition_value_sym(
            &self.obarray,
            sig.symbol,
            &Value::from_sym_id(quit_symbol()),
        ) {
            return CommandErrorSeverity::Routine;
        }
        let conditions = self.signal_conditions_value(sig);
        match self.skip_debugger(sig, &conditions) {
            Ok(true) => CommandErrorSeverity::Routine,
            Ok(false) => CommandErrorSeverity::Diagnostic,
            Err(flow) => {
                tracing::debug!(
                    "`debug-ignored-errors' could not be matched while ranking a command \
                     loop signal; treating it as a diagnostic: {flow:?}"
                );
                CommandErrorSeverity::Diagnostic
            }
        }
    }

    pub(super) fn call_debugger_for_signal(&mut self, sig: &SignalData) -> Result<(), Flow> {
        let rendered = super::super::error::format_signal_data_with_eval(self, sig);
        tracing::error!(
            "entering Lisp debugger for signal: symbol={} data={}",
            format_symbol_name_for_diagnostic(sig.symbol),
            rendered
        );
        // GNU `call_debugger (list2 (Qdebug, ...))` from `maybe_call_debugger`:
        // one shared entry point with the `debug-on-next-call` and
        // `debug-on-exit` sites, so the bindings it installs -- and the
        // `debug_on_next_call = 0` at `src/eval.c:298` -- cannot be one thing
        // for a signal and another thing for a call.
        self.call_debugger(vec![Value::symbol("error"), make_signal_binding_value(sig)])
            .map(|_| ())
    }
}
