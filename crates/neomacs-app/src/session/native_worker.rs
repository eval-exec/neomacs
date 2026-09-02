//! Native worker ownership for one blocking editor session.

use std::rc::Rc;
use std::thread::{JoinHandle, Result as ThreadResult};

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
}
