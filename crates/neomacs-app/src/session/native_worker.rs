//! Native worker ownership for one blocking editor session.

use std::rc::Rc;
use std::thread::{JoinHandle, Result as ThreadResult};
use std::time::{Duration, Instant};

use neovm_core::emacs_core::eval::Context;

use crate::presentation::PresentationMetrics;

use super::{EditorFrontend, EditorSession, EditorSessionExit};

/// Typed observation emitted from a native evaluator worker.
pub enum NativeEditorWorkerEvent {
    /// Evaluator construction and transport attachment completed.
    Started(EditorFrontend),
    /// At least one evaluator presentation may be waiting in the frontend inbox.
    FramesReady,
    /// Evaluator construction failed before a session was attached.
    StartupFailed(String),
    /// The outer GNU command loop unwound.
    Exited(EditorSessionExit),
}

/// Longest wait for an evaluator worker once its host has been destroyed.
///
/// This has to be bounded rather than a plain `join`. Android's Java main
/// thread is blocked for the whole span between the destroy notification and
/// the native entry point returning, so the wait is charged against the
/// watchdog that raises an ANR. `MainEvent::Destroy` is documented as
/// "Command from main thread: the app's activity is being destroyed, and
/// waiting for the app thread to clean up and exit before proceeding"
/// (android-activity 0.6.1 `src/lib.rs`), and game-activity's glue implements
/// exactly that: `android_app_free`, reached from `onDestroy`, writes
/// `APP_CMD_DESTROY` and then `pthread_cond_wait`s until `destroyed`, which is
/// only set by `android_app_destroy` "due to `android_main` returning"
/// (`android_native_app_glue.c:376-396`).
///
/// The evaluator is normally parked in its input wait and observes the lost
/// frontend in microseconds, so this budget only ever covers one that is busy
/// inside Lisp -- where surrendering the thread beats hanging the system's
/// teardown.
pub const HOST_DESTROY_JOIN_TIMEOUT: Duration = Duration::from_millis(2000);

/// How the worker thread came to rest during a bounded shutdown.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkerShutdown {
    /// The command loop unwound and the thread was joined.
    Joined,
    /// The worker panicked. It was joined and the panic was absorbed.
    ///
    /// The panic message has already reached the process panic hook, and the
    /// hosts that wait for a worker do so while being torn down -- resuming
    /// the unwind there would abort instead of reporting.
    Panicked,
    /// The worker was still running when the deadline passed.
    ///
    /// Its thread is left detached: the caller has surrendered every frontend
    /// handle by this point, so it has no way to interrupt Lisp, and a host
    /// being destroyed cannot afford to block without end.
    TimedOut,
}

/// Join handle for an evaluator constructed and run entirely off the UI thread.
#[must_use = "retain or join the native editor worker"]
pub struct NativeEditorWorker {
    thread: JoinHandle<()>,
}

impl NativeEditorWorker {
    /// Spawn one evaluator worker and report its lifecycle asynchronously.
    ///
    /// `create_evaluator` executes on the new worker. Native runtime-image
    /// loading and all other thread-affine evaluator initialization therefore
    /// stay off the platform UI thread. `emit` may forward events through a
    /// winit `EventLoopProxy`; it is subsequently invoked only by this worker.
    pub fn spawn(
        name: impl Into<String>,
        create_evaluator: impl FnOnce() -> Result<Context, String> + Send + 'static,
        metrics: PresentationMetrics,
        emit: impl Fn(NativeEditorWorkerEvent) + Send + 'static,
    ) -> std::io::Result<Self> {
        let thread = std::thread::Builder::new()
            .name(name.into())
            .spawn(move || {
                let emit: Rc<dyn Fn(NativeEditorWorkerEvent)> = Rc::new(emit);
                let evaluator = match create_evaluator() {
                    Ok(evaluator) => evaluator,
                    Err(error) => {
                        emit(NativeEditorWorkerEvent::StartupFailed(error));
                        return;
                    }
                };

                let frame_emitter = Rc::clone(&emit);
                let (session, frontend) = EditorSession::attach(evaluator, metrics, move || {
                    frame_emitter(NativeEditorWorkerEvent::FramesReady);
                });
                emit(NativeEditorWorkerEvent::Started(frontend));
                emit(NativeEditorWorkerEvent::Exited(session.run()));
            })?;
        Ok(Self { thread })
    }

    /// Wait for the evaluator worker and surface a thread panic to the owner.
    pub fn join(self) -> ThreadResult<()> {
        self.thread.join()
    }

    /// Wait up to `timeout` for the worker, abandoning it if it overruns.
    ///
    /// The caller is responsible for having already told the evaluator to
    /// stop. The supported way to do that without running Lisp is to drop
    /// every [`crate::session::FrontendInputPort`] clone: the evaluator reads
    /// the disconnected input channel as a lost display terminal and requests
    /// a non-interactive shutdown. Asking through a frontend event instead
    /// would let `delete-frame` prompt, and a destroyed host has nobody left
    /// to answer.
    #[must_use]
    pub fn shut_down_before(self, timeout: Duration) -> WorkerShutdown {
        /// Small enough that a prompt exit is not perceptibly delayed, large
        /// enough that the wait is not a spin.
        const POLL_INTERVAL: Duration = Duration::from_millis(5);

        let deadline = Instant::now() + timeout;
        while !self.thread.is_finished() {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return WorkerShutdown::TimedOut;
            }
            std::thread::sleep(POLL_INTERVAL.min(remaining));
        }
        // The thread has finished, so this join returns without blocking.
        if self.thread.join().is_err() {
            return WorkerShutdown::Panicked;
        }
        WorkerShutdown::Joined
    }
}
