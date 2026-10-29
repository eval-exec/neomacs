//! GNU's unhandled-error reporter (`cmd_error_internal` /
//! `command-error-default-function`) as evaluator methods.
//!
//! A report starts from a signal, but rendering it needs the printer and
//! delivering it needs the evaluator (the Lisp `command-error-function`,
//! shutdown hooks, and the minibuffer-quit exception). These methods
//! therefore live with the evaluator; the bottom `error` module keeps only
//! the typed payloads.

use crate::emacs_core::error::{Flow, SignalData};
use crate::emacs_core::intern::intern;
use crate::emacs_core::value::Value;

/// GNU's `cmd_error_internal` / `command-error-default-function` (keyboard.c:
/// 1030-1101): report an error that no Lisp handler caught, under a context
/// string naming where it happened.
///
/// This is the shared reporter. Its callers are the command loop and the
/// process filter/sentinel boundaries, which GNU routes through the very same
/// function so that "error in process filter: ..." reads like every other
/// unhandled-error report and obeys the same batch fatality.
impl super::Context {
    /// The `(SYMBOL . DATA)` value GNU hands to `command-error-function`.
    pub(crate) fn signal_error_data_value(&self, sig: &SignalData) -> Value {
        let payload = match (&sig.raw_data, sig.data.is_empty()) {
            (Some(raw), _) => *raw,
            (None, true) => Value::NIL,
            (None, false) => Value::list(sig.data.clone()),
        };
        Value::cons(Value::from_sym_id(sig.symbol), payload)
    }

    /// GNU `cmd_error_internal`: hand DATA and CONTEXT to
    /// `command-error-function`, which reports it.
    ///
    /// Returns `Flow::Shutdown` when the report is fatal, which is how a batch
    /// session dies on an unhandled error; the caller must propagate it rather
    /// than resume the work the error escaped from.
    pub(crate) fn report_command_error(&mut self, data: Value, context: &str) -> Result<(), Flow> {
        // GNU clears quit-flag and sets inhibit-quit around the report, so a
        // pending C-g cannot interrupt the reporting of an earlier error.
        self.assign("quit-flag", Value::NIL);
        self.assign("inhibit-quit", Value::T);

        // GNU `cmd_error_internal` dereferences `Vcommand_error_function`
        // (`src/keyboard.c:1041-1042`), which the swap-in keeps equal to the
        // current buffer's binding; `lisp/simple.el` localises it. Ledger 196.
        let handler = self
            .variables()
            .value_in_buffer_by_name(self.buffers.current_buffer(), "command-error-function")
            .unwrap_or(Value::NIL);
        let context_value = Value::string(context);
        // GNU's variable defaults to the C function itself, so its handler is
        // always callable. Ours is preset to help.el's wrapper at bootstrap,
        // which only becomes callable once help.el is loaded -- before that
        // (and in a bare evaluator) the default report IS the behavior the
        // variable names, so call it directly rather than signalling
        // void-function while reporting an error.
        if handler.is_truthy() && self.function_value_is_callable(&handler) {
            self.apply(handler, vec![data, context_value, Value::NIL])
                .map(|_| ())
        } else {
            self.command_error_default_report(data, context_value)
        }
    }

    /// GNU `command-error-default-function` (keyboard.c:1049-1101). Batch and
    /// pre-display sessions write the diagnostic to stderr and exit -1 (status
    /// 255); a live session messages it and carries on.
    pub(crate) fn command_error_default_report(
        &mut self,
        data: Value,
        context: Value,
    ) -> Result<(), Flow> {
        let symbol_mode = self.symbol_mode();
        let context_text = context.as_utf8_str().unwrap_or_default().to_string();
        let rendered = self.error_data_message(data);
        // GNU guards this branch with `!is_minibuffer_quit` (keyboard.c:1064):
        // aborting a minibuffer reports like any other quit but must never take
        // the session down, not even before the first frame is displayed.
        let is_minibuffer_quit = data.is_cons()
            && data
                .cons_car()
                .as_symbol_id(symbol_mode)
                .is_some_and(|symbol| {
                    crate::emacs_core::errors::signal_matches_hierarchical_sym(
                        &self.obarray,
                        symbol,
                        intern("minibuffer-quit"),
                        self.symbol_mode(),
                    )
                });
        if !is_minibuffer_quit && self.noninteractive() {
            eprintln!("{context_text}{rendered}");
            // GNU calls Fkill_emacs (-1) here, which runs kill-emacs-hook and
            // exits with status 255. Same path as the kill-emacs builtin: the
            // guarded shutdown boundary owns status 255 before its hooks and
            // records the exit code before the flow unwinds.
            return self
                .shutdown_with_hooks(super::ShutdownRequest {
                    exit_code: -1,
                    restart: false,
                })
                .map(|_| ());
        }
        let text = format!("{context_text}{rendered}");
        crate::emacs_core::builtins::misc_pure::builtin_message(self, &[Value::string(text)])?;
        Ok(())
    }

    /// GNU `print_error_message`'s message half: `error-message-string` of the
    /// `(SYMBOL . DATA)` pair.
    fn error_data_message(&mut self, data: Value) -> String {
        match crate::emacs_core::print::builtin_error_message_string(self, vec![data]) {
            Ok(text) => text
                .as_utf8_str()
                .map(|s| s.to_string())
                .unwrap_or_else(|| "peculiar error".to_string()),
            Err(_) => "peculiar error".to_string(),
        }
    }
}
