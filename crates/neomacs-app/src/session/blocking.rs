//! Native worker entry point for GNU's blocking outer command loop.

use neovm_core::emacs_core::eval::ShutdownRequest;

use super::EditorSession;

impl EditorSession {
    /// Enter GNU's blocking outer command loop on the current worker thread.
    ///
    /// This API is absent on WASM targets. Browser Workers require a separate
    /// JSPI or Asyncify adapter which can suspend without unwinding the
    /// recursive Lisp stack.
    pub fn run(mut self) -> EditorSessionExit {
        self.publish_now();
        let command_loop_error = self.evaluator.recursive_edit().err();
        EditorSessionExit {
            command_loop_error,
            shutdown: self.evaluator.shutdown_request(),
        }
    }
}

/// Terminal state returned after the outer command loop unwinds.
#[derive(Debug)]
pub struct EditorSessionExit {
    command_loop_error: Option<String>,
    shutdown: Option<ShutdownRequest>,
}

impl EditorSessionExit {
    /// Whether the command loop unwound without an evaluator error.
    #[must_use]
    pub const fn is_success(&self) -> bool {
        self.command_loop_error.is_none()
    }

    /// Formatted evaluator error, if the command loop failed.
    #[must_use]
    pub fn command_loop_error(&self) -> Option<&str> {
        self.command_loop_error.as_deref()
    }

    /// Explicit `kill-emacs` request that ended this session, if any.
    #[must_use]
    pub const fn shutdown_request(&self) -> Option<ShutdownRequest> {
        self.shutdown
    }
}
