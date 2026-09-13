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

pub(super) enum InitialWindow {
    Waiting(InitialWindowReceiver),
    Ready {
        size: InitialWindowSize,
        evaluator: Option<Receiver<Result<InitialWindowSize, String>>>,
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
}

struct StartingApp<F> {
    phase: Phase<F>,
    can_create_surfaces: bool,
    outcome: Rc<RefCell<Outcome>>,
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
            app.window_event(event_loop, id, event);
        }
    }

    fn about_to_wait(&mut self, event_loop: &dyn ActiveEventLoop) {
        if let Phase::Preparing { initial, .. } = &mut self.phase {
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
                initial: InitialWindow::Ready { size, .. },
                create,
            } = std::mem::replace(&mut self.phase, Phase::Stopped)
            {
                let mut app = Box::new(create(size));
                app.can_create_surfaces(event_loop);
                self.phase = Phase::Running(app);
                *self.outcome.borrow_mut() = Outcome::Running;
            }
        }
        if let Phase::Running(app) = &mut self.phase {
            app.about_to_wait(event_loop);
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
) -> Result<(), RenderLoopError> {
    event_loop.set_control_flow(ControlFlow::Wait);
    let outcome = Rc::new(RefCell::new(Outcome::Preparing));
    let result = event_loop.run_app(StartingApp {
        phase: Phase::Preparing { initial, create },
        can_create_surfaces: false,
        outcome: Rc::clone(&outcome),
    });
    match &mut *outcome.borrow_mut() {
        Outcome::Preparing => Err(RenderLoopError::StartupInterrupted(
            result
                .err()
                .map_or_else(|| "event loop exited".into(), |error| error.to_string()),
        )),
        Outcome::Failed(error) => Err(RenderLoopError::Preparation(std::mem::take(error))),
        Outcome::EvaluatorExited => Ok(()),
        Outcome::Running => result.map_err(|error| RenderLoopError::EventLoop(error.to_string())),
    }
}
