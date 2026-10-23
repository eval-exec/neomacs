//! One-shot preparation before the first native window exists.
//!
//! Font parsing/opening stays on the evaluator thread after its TLS is ready.
//! The native thread receives only geometry, never Lisp values or font handles.

use neomacs_display_runtime::render_thread::{
    InitialWindowLifetime, InitialWindowReply, InitialWindowSize,
};
use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

/// Evaluator-stack ownership of the native startup reply and active lifetime.
/// Display hosts receive only a weak publisher: retaining or leaking a host
/// cannot keep native startup alive after its evaluator has finished.
#[must_use = "retain on the evaluator stack until GUI evaluation ends"]
pub(super) struct InitialWindowOwner {
    state: Rc<RefCell<InitialWindowStatus>>,
}

enum InitialWindowStatus {
    Waiting(InitialWindowReply),
    Running(InitialWindowLifetime),
    Closed,
}

/// Non-owning completion capability used by the first GUI-frame realization.
#[derive(Clone)]
pub(super) struct InitialWindowPublisher {
    state: Weak<RefCell<InitialWindowStatus>>,
}

impl InitialWindowOwner {
    pub(super) fn new(reply: InitialWindowReply) -> Self {
        Self {
            state: Rc::new(RefCell::new(InitialWindowStatus::Waiting(reply))),
        }
    }

    pub(super) fn publisher(&self) -> InitialWindowPublisher {
        InitialWindowPublisher {
            state: Rc::downgrade(&self.state),
        }
    }
}

impl InitialWindowPublisher {
    pub(super) fn is_waiting(&self) -> bool {
        self.state
            .upgrade()
            .is_some_and(|state| matches!(*state.borrow(), InitialWindowStatus::Waiting(_)))
    }

    /// Publish final opened-font geometry exactly once. Later frame creation
    /// does not replace the first window or its evaluator-owned lifetime.
    pub(super) fn complete(&self, width: u32, height: u32) -> Result<(), String> {
        let state = self
            .state
            .upgrade()
            .ok_or_else(|| "GUI evaluator exited before initial frame realization".to_owned())?;
        let mut state = state.borrow_mut();
        match std::mem::replace(&mut *state, InitialWindowStatus::Closed) {
            InitialWindowStatus::Waiting(reply) => {
                let lifetime = reply
                    .ready(InitialWindowSize { width, height })
                    .ok_or_else(|| {
                        "native event loop closed before initial frame realization".to_owned()
                    })?;
                *state = InitialWindowStatus::Running(lifetime);
                Ok(())
            }
            InitialWindowStatus::Running(lifetime) => {
                *state = InitialWindowStatus::Running(lifetime);
                Ok(())
            }
            InitialWindowStatus::Closed => {
                Err("initial GUI frame preparation is closed".to_owned())
            }
        }
    }

    /// A preparation failure ends pending native startup without manufacturing
    /// a window. Failures after realization follow ordinary evaluator shutdown.
    pub(super) fn fail(&self, error: String) {
        let Some(state) = self.state.upgrade() else {
            return;
        };
        let mut state = state.borrow_mut();
        match std::mem::replace(&mut *state, InitialWindowStatus::Closed) {
            InitialWindowStatus::Waiting(reply) => reply.failed(error),
            InitialWindowStatus::Running(lifetime) => {
                *state = InitialWindowStatus::Running(lifetime);
            }
            InitialWindowStatus::Closed => {}
        }
    }
}

#[derive(Debug, strum::Display)]
pub(super) enum StartupFrameError {
    #[strum(serialize = "no usable initial GUI font was found")]
    NoUsableFont,
}
