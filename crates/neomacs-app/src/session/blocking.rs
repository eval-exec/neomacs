//! Outer GNU command loop entered on the evaluator's owning thread.

use neovm_core::emacs_core::eval::{Context, ShutdownRequest};

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
    pub fn run(self) -> EditorSessionExit {
        self.run_until_stopped(|_| {}).into_exit()
    }

    /// Run the outer loop and return evaluator ownership to a native host.
    ///
    /// The callback runs after `after-pdump-load-hook` and immediately before
    /// the first outer-loop dispatch. Native hosts use it for thread-affine
    /// one-time preparation such as AOT preload installation.
    pub fn run_until_stopped(
        mut self,
        before_command_loop: impl FnOnce(&Context),
    ) -> StoppedEditorSession {
        self.prepare_to_run();
        before_command_loop(&self.evaluator);
        let command_loop_error = self.evaluator.recursive_edit().err();
        let exit = EditorSessionExit {
            command_loop_error,
            shutdown: self.evaluator.shutdown_request(),
        };
        StoppedEditorSession {
            evaluator: self.evaluator,
            exit,
        }
    }
}

/// A session whose command loop has unwound but whose evaluator is still owned.
pub struct StoppedEditorSession {
    evaluator: Context,
    exit: EditorSessionExit,
}

impl StoppedEditorSession {
    /// Discard evaluator ownership and retain only the terminal status.
    #[must_use]
    pub fn into_exit(self) -> EditorSessionExit {
        self.exit
    }

    /// Return terminal status and evaluator state to their native owner.
    #[must_use]
    pub fn into_parts(self) -> (EditorSessionExit, Context) {
        (self.exit, self.evaluator)
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
