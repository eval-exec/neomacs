//! Native event-loop startup before font-owned window geometry is available.

use super::{RenderApp, SharedMonitorInfo};
use crossbeam_channel::{Receiver, Sender, TryRecvError};
use std::{cell::RefCell, rc::Rc};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy},
    window::WindowId,
};

/// Evaluator-selected dimensions in Emacs pixels. No font handle crosses threads.
#[derive(Clone, Copy, Debug)]
pub struct InitialWindowSize {
    pub width: u32,
    pub height: u32,
}

impl InitialWindowSize {
    pub(super) fn observe_content(&mut self, width: u32, height: u32) {
        // A suspended or fully covered surface cannot replace the dimensions
        // needed to recreate the editor when native surfaces become available.
        if width != 0 && height != 0 {
            *self = Self { width, height };
        }
    }
}

/// Final logical intent for a native window that has not been allocated yet.
/// Geometry observations may change size without resetting visibility or chrome.
#[derive(Clone, Copy, Debug)]
pub struct InitialWindowPreparation {
    pub size: InitialWindowSize,
    pub visibility: crate::thread_comm::WindowVisibility,
    pub decorated: bool,
}

/// Consuming preparation reply. Success, failure and unwind all wake the native
/// loop after publishing their outcome; an idle loop never needs a polling timer.
pub struct InitialWindowReply {
    sender: Option<Sender<Result<InitialWindowPreparation, String>>>,
    proxy: EventLoopProxy,
}

pub struct InitialWindowReceiver(Receiver<Result<InitialWindowPreparation, String>>);

/// Retain on the evaluator stack until it exits. While surfaces are unavailable,
/// the native loop must distinguish prepared geometry from a departed evaluator.
#[must_use = "retain on the evaluator stack until GUI evaluation ends"]
pub struct InitialWindowLifetime {
    _reply: InitialWindowReply,
}

impl InitialWindowReply {
    pub fn channel(proxy: EventLoopProxy) -> (Self, InitialWindowReceiver) {
        let (sender, receiver) = crossbeam_channel::bounded(1);
        (
            Self {
                sender: Some(sender),
                proxy,
            },
            InitialWindowReceiver(receiver),
        )
    }

    /// None means the native loop has already closed; abandon GUI installation.
    pub fn ready(self, preparation: InitialWindowPreparation) -> Option<InitialWindowLifetime> {
        self.sender.as_ref().unwrap().send(Ok(preparation)).ok()?;
        self.proxy.wake_up();
        Some(InitialWindowLifetime { _reply: self })
    }

    pub fn failed(mut self, error: String) {
        let _ = self.sender.take().unwrap().send(Err(error));
    }
}

impl Drop for InitialWindowReply {
    fn drop(&mut self) {
        // Disconnect before waking on unwind, or try_recv could still see Empty.
        drop(self.sender.take());
        self.proxy.wake_up();
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RenderLoopError {
    #[error("{0}")]
    Preparation(String),
    /// The evaluator may still be in external I/O. The caller must not join it
    /// before ending the process in response to this native startup failure.
    #[error("native event loop closed during GUI preparation: {0}")]
    StartupInterrupted(String),
    #[error("event loop error: {0}")]
    EventLoop(String),
}

pub enum RenderLoopExit {
    Finished,
    /// Native resources have been released, but a driver worker may still be
    /// inside foreign code. Process exit must bypass library exit callbacks.
    GpuStartupCancelled,
}

pub(super) enum InitialWindow {
    Waiting(InitialWindowReceiver),
    Ready {
        preparation: InitialWindowPreparation,
        evaluator: Option<Receiver<Result<InitialWindowPreparation, String>>>,
    },
}

enum Phase<F> {
    Preparing { initial: InitialWindow, create: F },
    Running(Box<RenderApp>),
    Stopped,
}

#[derive(Default)]
enum Outcome {
    #[default]
    Preparing,
    Running,
    EvaluatorExited,
    Failed(String),
    GpuFailed(String),
}

impl Outcome {
    fn observe_evaluator_shutdown(
        &mut self,
        gpu_was_ready: bool,
        reason: Option<super::state::RenderShutdownReason>,
    ) -> bool {
        // Evaluator departure is a successful startup cancellation only while
        // preparing. Running records history: recovery or teardown may remove
        // the GPU without turning an established session back into startup.
        match self {
            Self::Preparing
                if !gpu_was_ready
                    && matches!(
                        reason,
                        Some(super::state::RenderShutdownReason::EvaluatorShutdown)
                    ) =>
            {
                *self = Self::EvaluatorExited;
                true
            }
            Self::Preparing
            | Self::Running
            | Self::EvaluatorExited
            | Self::Failed(_)
            | Self::GpuFailed(_) => false,
        }
    }

    fn finish(
        self,
        result: Result<(), impl ToString>,
        gpu_cancelled: bool,
    ) -> Result<RenderLoopExit, RenderLoopError> {
        let successful_exit = || {
            if gpu_cancelled {
                RenderLoopExit::GpuStartupCancelled
            } else {
                RenderLoopExit::Finished
            }
        };
        match self {
            Self::Preparing => Err(RenderLoopError::StartupInterrupted(
                result
                    .err()
                    .map_or_else(|| "event loop exited".into(), |error| error.to_string()),
            )),
            Self::Failed(error) => Err(RenderLoopError::Preparation(error)),
            Self::GpuFailed(error) => Err(RenderLoopError::StartupInterrupted(error)),
            Self::EvaluatorExited => Ok(successful_exit()),
            Self::Running => result.map(|()| successful_exit()).map_err(|error| {
                if gpu_cancelled {
                    RenderLoopError::StartupInterrupted(error.to_string())
                } else {
                    RenderLoopError::EventLoop(error.to_string())
                }
            }),
        }
    }
}

struct StartingApp<F> {
    phase: Phase<F>,
    shared_monitors: SharedMonitorInfo,
    can_create_surfaces: bool,
    outcome: Rc<RefCell<Outcome>>,
    evaluator: Option<Receiver<Result<InitialWindowPreparation, String>>>,
    gpu_cancelled: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl<F: FnOnce(InitialWindowSize) -> RenderApp> ApplicationHandler for StartingApp<F> {
    fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        self.can_create_surfaces = true;
        if let Phase::Running(app) = &mut self.phase {
            app.can_create_surfaces(event_loop);
        }
    }

    fn destroy_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        self.can_create_surfaces = false;
        if let Phase::Running(app) = &mut self.phase {
            app.destroy_surfaces(event_loop);
        }
    }

    fn window_event(&mut self, event_loop: &dyn ActiveEventLoop, id: WindowId, event: WindowEvent) {
        if let Phase::Running(app) = &mut self.phase {
            if app
                .gpu_startup
                .as_ref()
                .is_some_and(|pending| pending.window_id() == id)
                && matches!(event, WindowEvent::CloseRequested | WindowEvent::Destroyed)
            {
                // A user close follows the ordinary WindowClose/evaluator exit
                // path. It is not an unexpected native-display interruption.
                *self.outcome.borrow_mut() = Outcome::Running;
            }
            app.window_event(event_loop, id, event);
        }
    }

    fn about_to_wait(&mut self, event_loop: &dyn ActiveEventLoop) {
        if let Phase::Preparing { initial, .. } = &mut self.phase {
            // Lisp can query display geometry during initialization before
            // requesting its first GUI frame. Publish that connection metadata
            // independently of first-window readiness and surface permission.
            if self
                .shared_monitors
                .0
                .lock()
                .is_ok_and(|monitors| monitors.is_empty())
            {
                let snapshot = RenderApp::collect_monitor_snapshot(event_loop);
                super::lifecycle::publish_monitor_snapshot(&self.shared_monitors, &snapshot);
            }
            if let InitialWindow::Waiting(receiver) = initial {
                let result = match receiver.0.try_recv() {
                    Ok(result) => result,
                    Err(TryRecvError::Empty) => {
                        event_loop.set_control_flow(ControlFlow::Wait);
                        return;
                    }
                    Err(TryRecvError::Disconnected) => {
                        // early-init may intentionally exit before creating
                        // any GUI frame. Join the worker for its authoritative
                        // status or panic, just as after readiness below.
                        *self.outcome.borrow_mut() = Outcome::EvaluatorExited;
                        self.phase = Phase::Stopped;
                        event_loop.exit();
                        return;
                    }
                };
                match result {
                    Ok(preparation) => {
                        *initial = InitialWindow::Ready {
                            preparation,
                            evaluator: Some(receiver.0.clone()),
                        };
                    }
                    Err(error) => {
                        *self.outcome.borrow_mut() = Outcome::Failed(error);
                        self.phase = Phase::Stopped;
                        event_loop.exit();
                        return;
                    }
                }
            }
            if let InitialWindow::Ready {
                evaluator: Some(evaluator),
                ..
            } = initial
                && matches!(evaluator.try_recv(), Err(TryRecvError::Disconnected))
            {
                // The caller joins to distinguish kill-emacs (including status
                // 0) from panic. Completion after readiness is not itself an error.
                *self.outcome.borrow_mut() = Outcome::EvaluatorExited;
                self.phase = Phase::Stopped;
                event_loop.exit();
                return;
            }
            if !self.can_create_surfaces {
                event_loop.set_control_flow(ControlFlow::Wait);
                return;
            }
            if let Phase::Preparing {
                initial:
                    InitialWindow::Ready {
                        preparation,
                        evaluator,
                    },
                create,
            } = std::mem::replace(&mut self.phase, Phase::Stopped)
            {
                let mut app = Box::new(create(preparation.size));
                let primary = app
                    .frame_windows
                    .primary_window_mut()
                    .expect("initial primary window state");
                primary.set_visibility(preparation.visibility);
                primary.set_decorations(preparation.decorated);
                app.gpu_startup_cancelled = self.gpu_cancelled.clone();
                app.can_create_surfaces(event_loop);
                self.evaluator = evaluator;
                self.phase = Phase::Running(app);
            }
        }
        if self.evaluator.as_ref().is_some_and(|evaluator| {
            matches!(evaluator.try_recv(), Err(TryRecvError::Disconnected))
        }) {
            *self.outcome.borrow_mut() = Outcome::EvaluatorExited;
            self.phase = Phase::Stopped;
            event_loop.exit();
            return;
        }
        if let Phase::Running(app) = &mut self.phase {
            let gpu_was_ready = app.gpu.is_some();
            app.about_to_wait(event_loop);
            let evaluator_exited = self
                .outcome
                .borrow_mut()
                .observe_evaluator_shutdown(gpu_was_ready, app.lifecycle_flags.shutdown_reason());
            if !evaluator_exited {
                if let Some(error) = app.startup_error.take() {
                    *self.outcome.borrow_mut() = Outcome::GpuFailed(error);
                    self.phase = Phase::Stopped;
                    event_loop.exit();
                } else if app.gpu.is_some() {
                    self.evaluator = None;
                    *self.outcome.borrow_mut() = Outcome::Running;
                }
            }
        }
    }

    // Both startup replies and render commands are drained at about_to_wait,
    // after native dispatch, using the same wake-only contract as RenderApp.
    fn proxy_wake_up(&mut self, _event_loop: &dyn ActiveEventLoop) {}
}

pub(super) fn run(
    event_loop: EventLoop,
    initial: InitialWindow,
    shared_monitors: SharedMonitorInfo,
    create: impl FnOnce(InitialWindowSize) -> RenderApp + 'static,
) -> Result<RenderLoopExit, RenderLoopError> {
    event_loop.set_control_flow(ControlFlow::Wait);
    let outcome = Rc::new(RefCell::new(Outcome::Preparing));
    let gpu_cancelled = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let result = event_loop.run_app(StartingApp {
        phase: Phase::Preparing { initial, create },
        shared_monitors,
        can_create_surfaces: false,
        outcome: Rc::clone(&outcome),
        evaluator: None,
        gpu_cancelled: gpu_cancelled.clone(),
    });
    std::mem::take(&mut *outcome.borrow_mut()).finish(
        result,
        gpu_cancelled.load(std::sync::atomic::Ordering::Relaxed),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render_thread::state::RenderShutdownReason;

    #[test]
    fn live_gpu_shutdown_preserves_native_event_loop_failure() {
        let mut outcome = Outcome::Running;
        outcome.observe_evaluator_shutdown(true, Some(RenderShutdownReason::EvaluatorShutdown));

        let result = outcome.finish(Err("native dispatch failed"), false);
        assert!(
            matches!(result, Err(RenderLoopError::EventLoop(error)) if error == "native dispatch failed")
        );
    }

    #[test]
    fn gpu_recovery_shutdown_preserves_native_event_loop_failure() {
        // A previously running GPU may be absent after failed device-loss
        // recovery. Shutdown in that state is no longer initial preparation.
        let mut outcome = Outcome::Running;
        outcome.observe_evaluator_shutdown(false, Some(RenderShutdownReason::EvaluatorShutdown));

        let result = outcome.finish(Err("native dispatch failed during device recovery"), false);
        assert!(matches!(result, Err(RenderLoopError::EventLoop(error))
                if error == "native dispatch failed during device recovery"));
    }

    #[test]
    fn pending_gpu_evaluator_shutdown_finishes_gracefully() {
        let mut outcome = Outcome::Preparing;
        outcome.observe_evaluator_shutdown(false, Some(RenderShutdownReason::EvaluatorShutdown));

        assert!(matches!(
            outcome.finish(Err("native loop ended during evaluator exit"), false),
            Ok(RenderLoopExit::Finished)
        ));
    }
}
