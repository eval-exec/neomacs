//! Error policy for asynchronous Lisp callbacks (filters, sentinels, timers).
//!
//! Shared by both process backends: timers run on every host, and the GNU
//! reporting rules below are properties of the callback class, not of whether
//! this build can spawn processes.

use crate::emacs_core::error::{EvalResult, Flow};
use crate::emacs_core::eval::Context;

/// Which asynchronous callback an escaped error came from.
///
/// GNU treats the classes DIFFERENTLY, so the kind decides the reporting, not
/// a log string: a filter or sentinel error goes through `cmd_error_internal`
/// (process.c:6208, :7791) and is therefore FATAL in batch, while a timer error
/// is caught by timer.el's own `condition-case-unless-debug` and merely
/// messaged (timer.el:332-338), so batch survives it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AsyncCallbackKind {
    /// GNU `read_process_output_error_handler` (process.c:6208).
    ProcessFilter,
    /// GNU `exec_sentinel_error_handler` (process.c:7791).
    ProcessSentinel,
    /// GNU runs these through timer.el `timer-event-handler`, never through
    /// the command-error reporter.
    Timer,
    /// A network server's log function. GNU installs NO handler around it
    /// (a bare `calln`, process.c:5176), so an error there propagates to the
    /// command loop instead of being reported here. Keeping the catch is a
    /// known, deliberate divergence rather than part of this fix.
    ServerLog,
}

impl AsyncCallbackKind {
    /// Diagnostic name for the trace log.
    pub(crate) fn label(self) -> &'static str {
        match self {
            AsyncCallbackKind::ProcessFilter => "process filter",
            AsyncCallbackKind::ProcessSentinel => "process sentinel",
            AsyncCallbackKind::Timer => "GNU Lisp timer",
            AsyncCallbackKind::ServerLog => "server log",
        }
    }

    /// GNU's `cmd_error_internal` context string for this class, or `None`
    /// when GNU does not route the class through the command-error reporter.
    pub(crate) fn command_error_context(self) -> Option<&'static str> {
        match self {
            AsyncCallbackKind::ProcessFilter => Some("error in process filter: "),
            AsyncCallbackKind::ProcessSentinel => Some("error in process sentinel: "),
            AsyncCallbackKind::Timer | AsyncCallbackKind::ServerLog => None,
        }
    }
}

impl Context {
    /// Resolve the control flow that escaped a timer/process callback after the
    /// callback's own state (buffer/deactivate-mark/specpdl/gc-roots) has been
    /// restored.
    ///
    /// GNU runs timer callbacks through `lisp/emacs-lisp/timer.el`
    /// `timer-event-handler`, which wraps the call in
    /// `condition-case-unless-debug err … (error …)`; process filters/sentinels
    /// in `src/process.c` (`read_process_output`/`exec_sentinel`) run with no
    /// surrounding handler at all.  In both cases an `error`-class *signal* is
    /// caught (and logged), but a non-local `throw` is NOT an error, so it
    /// propagates past the callback boundary to the matching outer `catch`.
    ///
    /// Mirroring that, a `Flow::Signal` is caught and logged here, while
    /// non-local control flow is propagated to the caller so it can reach the
    /// matching wait/catch boundary.  A throw to a tag with no live catch still
    /// becomes a `no-catch` error at the eval/thread boundary, as in GNU.
    ///
    /// `Flow::Shutdown` propagates for the same reason: GNU's `Fkill_emacs`
    /// never returns, so a callback that kills cannot be resumed and its exit
    /// code must not be swallowed here.
    pub(crate) fn finish_callback_flow(
        &mut self,
        result: EvalResult,
        kind: AsyncCallbackKind,
    ) -> Result<(), Flow> {
        match result {
            Ok(_) => Ok(()),
            Err(err @ (Flow::Throw(_) | Flow::ThreadBlocked(_) | Flow::Shutdown(_))) => Err(err),
            Err(err @ Flow::Signal(_)) => {
                let rendered = crate::emacs_core::error::format_flow_with_eval(self, &err);
                tracing::warn!("{} callback error: {}", kind.label(), rendered);
                let Flow::Signal(sig) = &err else {
                    unreachable!("matched Flow::Signal above")
                };
                match kind.command_error_context() {
                    // GNU reports these through cmd_error_internal, whose
                    // default reporter writes to stderr and kills a batch
                    // session -- so the shutdown propagates and the work the
                    // error escaped from is not resumed.
                    Some(context) => {
                        let data = self.signal_error_data_value(sig);
                        self.report_command_error(data, context)
                    }
                    // Reported by the callback's own Lisp handler in GNU; the
                    // trace above is all this boundary owes.
                    None => Ok(()),
                }
            }
        }
    }
}
