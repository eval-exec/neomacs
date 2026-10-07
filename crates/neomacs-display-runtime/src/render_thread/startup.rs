//! Native event-loop startup before font-owned window geometry is available.

use super::RenderApp;
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

/// Consuming preparation reply. Success, failure and unwind all wake the native
/// loop after publishing their outcome; an idle loop never needs a polling timer.
pub struct InitialWindowReply {
    sender: Option<Sender<Result<InitialWindowSize, String>>>,
    proxy: EventLoopProxy,
}

pub struct InitialWindowReceiver(Receiver<Result<InitialWindowSize, String>>);

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
    pub fn ready(self, size: InitialWindowSize) -> Option<InitialWindowLifetime> {
        self.sender.as_ref().unwrap().send(Ok(size)).ok()?;
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
        size: InitialWindowSize,
        evaluator: Option<Receiver<Result<InitialWindowSize, String>>>,
    },
}

enum Phase {
    Preparing {
        initial: InitialWindow,
        app: Box<RenderApp>,
    },
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

pub(super) struct StartingApp {
    phase: Phase,
    can_create_surfaces: bool,
    outcome: Rc<RefCell<Outcome>>,
    evaluator: Option<Receiver<Result<InitialWindowSize, String>>>,
    retained: bool,
}

impl StartingApp {
    pub(super) fn retained() -> Self {
        Self {
            phase: Phase::Stopped,
            can_create_surfaces: false,
            outcome: Default::default(),
            evaluator: None,
            retained: true,
        }
    }
    pub(super) fn install(&mut self, receiver: InitialWindowReceiver, app: RenderApp) {
        self.retire();
        self.phase = Phase::Preparing {
            initial: InitialWindow::Waiting(receiver),
            app: Box::new(app),
        };
    }
    pub(super) fn retire(&mut self) {
        self.evaluator = None;
        self.phase = Phase::Stopped;
    }
}

impl ApplicationHandler for StartingApp {
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
        if let Phase::Preparing { initial, app } = &mut self.phase {
            // Consume CPU-owned configuration and frame identity even while no
            // initial geometry exists. The bounded producer cannot deadlock the
            // evaluator that must eventually publish that geometry.
            if app.process_startup_commands() {
                *self.outcome.borrow_mut() = Outcome::EvaluatorExited;
                self.phase = Phase::Stopped;
                if !self.retained {
                    event_loop.exit();
                }
                return;
            }
            if let InitialWindow::Waiting(receiver) = initial {
                let result = match receiver.0.try_recv() {
                    Ok(result) => result,
                    Err(TryRecvError::Empty) => {
                        event_loop.set_control_flow(ControlFlow::Wait);
                        return;
                    }
                    Err(TryRecvError::Disconnected) => {
                        Err("GUI evaluator exited before preparing its initial frame".into())
                    }
                };
                match result {
                    Ok(size) => {
                        *initial = InitialWindow::Ready {
                            size,
                            evaluator: Some(receiver.0.clone()),
                        };
                    }
                    Err(error) => {
                        *self.outcome.borrow_mut() = Outcome::Failed(error);
                        self.phase = Phase::Stopped;
                        if !self.retained {
                            event_loop.exit();
                        }
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
                if !self.retained {
                    event_loop.exit();
                }
                return;
            }
            if !self.can_create_surfaces {
                event_loop.set_control_flow(ControlFlow::Wait);
                return;
            }
            if let Phase::Preparing {
                initial: InitialWindow::Ready { size, evaluator },
                mut app,
            } = std::mem::replace(&mut self.phase, Phase::Stopped)
            {
                app.pending_content_size = size;
                if let Some(primary) = app.frame_windows.primary_window_mut()
                    && let super::frame_windows::FrameLifecycle::Pending { width, height, .. } =
                        &mut primary.lifecycle
                {
                    *width = size.width;
                    *height = size.height;
                }
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
            if !self.retained {
                event_loop.exit();
            }
            return;
        }
        if let Phase::Running(app) = &mut self.phase {
            if app.comms.keep_alive_without_frames && self.can_create_surfaces && app.gpu.is_none()
            {
                if app.process_startup_commands() {
                    *self.outcome.borrow_mut() = Outcome::EvaluatorExited;
                    self.phase = Phase::Stopped;
                    if !self.retained {
                        event_loop.exit();
                    }
                    return;
                }
                if app.startup_error.is_none()
                    && app.gpu_startup.is_none()
                    && app.frame_windows.primary_window().is_some()
                {
                    app.can_create_surfaces(event_loop);
                }
            }
            if app.startup_error.is_none() {
                app.about_to_wait(event_loop);
            }
            if let Some(error) = app.startup_error.take() {
                if self.retained {
                    app.cancel_gpu_startup();
                    let frame = app.frame_windows.primary_event_frame_id();
                    app.frame_windows.reject_ready(frame, &error);
                    app.frame_windows.take_primary_window();
                    app.frame_windows.clear_primary_mapping();
                } else {
                    *self.outcome.borrow_mut() = Outcome::GpuFailed(error);
                    self.phase = Phase::Stopped;
                    event_loop.exit();
                }
            } else if app.gpu.is_some() {
                if !app.comms.keep_alive_without_frames {
                    self.evaluator = None;
                }
                *self.outcome.borrow_mut() = Outcome::Running;
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
    create: impl FnOnce(InitialWindowSize) -> RenderApp + 'static,
) -> Result<RenderLoopExit, RenderLoopError> {
    event_loop.set_control_flow(ControlFlow::Wait);
    let outcome = Rc::new(RefCell::new(Outcome::Preparing));
    let gpu_cancelled = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let size = match &initial {
        InitialWindow::Ready { size, .. } => *size,
        InitialWindow::Waiting(_) => InitialWindowSize {
            width: 0,
            height: 0,
        },
    };
    let mut app = Box::new(create(size));
    app.gpu_startup_cancelled = gpu_cancelled.clone();
    let result = event_loop.run_app(StartingApp {
        phase: Phase::Preparing { initial, app },
        can_create_surfaces: false,
        outcome: Rc::clone(&outcome),
        evaluator: None,
        retained: false,
    });
    let successful_exit = || {
        if gpu_cancelled.load(std::sync::atomic::Ordering::Relaxed) {
            RenderLoopExit::GpuStartupCancelled
        } else {
            RenderLoopExit::Finished
        }
    };
    match &mut *outcome.borrow_mut() {
        Outcome::Preparing => Err(RenderLoopError::StartupInterrupted(
            result
                .err()
                .map_or_else(|| "event loop exited".into(), |error| error.to_string()),
        )),
        Outcome::Failed(error) => Err(RenderLoopError::Preparation(std::mem::take(error))),
        Outcome::GpuFailed(error) => {
            Err(RenderLoopError::StartupInterrupted(std::mem::take(error)))
        }
        Outcome::EvaluatorExited => Ok(successful_exit()),
        Outcome::Running => result.map(|()| successful_exit()).map_err(|error| {
            if gpu_cancelled.load(std::sync::atomic::Ordering::Relaxed) {
                RenderLoopError::StartupInterrupted(error.to_string())
            } else {
                RenderLoopError::EventLoop(error.to_string())
            }
        }),
    }
}
