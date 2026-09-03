//! Outer GNU command loop entered on the evaluator's owning thread.

use neovm_core::emacs_core::eval::ShutdownRequest;

use super::EditorSession;

impl EditorSession {
    /// Complete the transition from an attached image to a running native
    /// outer command loop.
    ///
    /// Runtime-image restoration deliberately defers
    /// `after-pdump-load-hook` until the live frontend has been attached.
    fn prepare_to_run(&mut self) {
        self.publish_now();
        neovm_core::emacs_core::load::maybe_run_after_pdump_load_hook(&mut self.evaluator);
    }

    /// Enter GNU's outer command loop on the current evaluator worker.
    ///
    /// Native hosts block in their OS poller. Browser Workers install a host
    /// wait backend first; JSPI suspends this call without unwinding its Rust
    /// or Lisp stack, while the compatibility path blocks in `Atomics.wait`.
    pub fn run(mut self) -> EditorSessionExit {
        self.prepare_to_run();
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
