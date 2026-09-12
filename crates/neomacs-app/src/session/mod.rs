//! One evaluator session attached to a typed frontend transport.
//!
//! Platform adapters own their event loops and renderers. This module owns the
//! inverse side of that boundary: evaluator input, retained presentation
//! state, and renderer acknowledgements. Native hosts additionally expose a
//! blocking GNU command loop; browser WASM requires an async suspension
//! adapter which preserves the recursive Lisp stack.

mod blocking;
mod ime;
mod transport;
pub use blocking::{EditorSessionExit, StoppedEditorSession};
pub use ime::{ImeClient, ImeEditorError, ImeReply, PendingImeReply};
std::cfg_select! {
    target_family = "wasm" => {}
    _ => {
        mod native_worker;
        pub use native_worker::{NativeEditorWorker, NativeEditorWorkerEvent};
    }
}

use std::rc::Rc;

use crossbeam_channel::unbounded;
use neomacs_display_protocol::SealedFramePresentation;
use neovm_core::emacs_core::eval::Context;
use neovm_core::emacs_core::wait::HostInputWaitBackend;

use crate::presentation::{EditorPresentationRuntime, FramePublishResult, PresentationMetrics};

pub use transport::{
    ActiveFrontendPresentation, EditorFrontend, FrontendFrameInbox, FrontendFrameReceive,
    FrontendInputDisconnected, FrontendInputPort, FrontendInputSubmission, FrontendWake,
    PendingFrontendFrame,
};

/// Evaluator state wired to one frontend's input and presentation streams.
pub struct EditorSession {
    evaluator: Context,
    presentation: SessionPresentationTransport,
}

/// Decision made by a host-specific redisplay route before GUI publication.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionRedisplayAction {
    /// Continue through the session's shared presentation runtime.
    Publish,
    /// A different host renderer, such as a selected secondary TTY, handled it.
    Handled,
}

#[derive(Clone)]
struct SessionPresentationTransport {
    runtime: EditorPresentationRuntime,
    route: Rc<dyn Fn(&mut Context) -> SessionRedisplayAction>,
    try_publish: Rc<dyn Fn(SealedFramePresentation) -> bool>,
    notify_frontend: Rc<dyn Fn()>,
}

impl SessionPresentationTransport {
    fn publish(&self, evaluator: &mut Context) -> FramePublishResult {
        evaluator.setup_thread_locals();
        if (self.route)(evaluator) == SessionRedisplayAction::Handled {
            return FramePublishResult::default();
        }
        let result = self
            .runtime
            .publish_visible_frames(evaluator, |frame| (self.try_publish)(frame));
        if result.published() > 0 {
            (self.notify_frontend)();
        }
        result
    }
}

impl EditorSession {
    /// Attach an already initialized evaluator to a frontend.
    ///
    /// The caller must construct this value on the thread that will run Lisp.
    /// Android therefore calls it inside its evaluator worker, never on the
    /// Activity thread. Browser WASM uses the same attachment inside its
    /// dedicated Worker.
    pub fn attach(
        mut evaluator: Context,
        metrics: PresentationMetrics,
        notify_frontend: impl Fn() + 'static,
    ) -> (Self, EditorFrontend) {
        evaluator.setup_thread_locals();

        let (input_tx, input_rx) = unbounded();
        let input = FrontendInputPort::new(
            input_tx,
            evaluator.wait_notifier(),
            evaluator.quit_requested.clone(),
        );
        evaluator.init_input_system(input_rx);

        let (frame_tx, frame_rx) = unbounded();
        let session = Self::attach_presentation_transport(
            evaluator,
            EditorPresentationRuntime::new(metrics),
            |_| SessionRedisplayAction::Publish,
            move |frame| frame_tx.try_send(Box::new(frame)).is_ok(),
            notify_frontend,
        );

        let frontend = EditorFrontend::new(input, frame_rx);
        (session, frontend)
    }

    /// Attach an evaluator whose host has already installed its input channel.
    ///
    /// Native GUI adapters use this form because their input vocabulary also
    /// contains platform observations beyond [`crate::frontend_event::FrontendEvent`].
    /// Presentation ownership, query hooks, initial publication, and command
    /// loop lifecycle still remain inside this session.
    pub fn attach_host_transport(
        evaluator: Context,
        presentation: EditorPresentationRuntime,
        route: impl Fn(&mut Context) -> SessionRedisplayAction + 'static,
        try_publish: impl Fn(SealedFramePresentation) -> bool + 'static,
        notify_frontend: impl Fn() + 'static,
    ) -> Self {
        Self::attach_presentation_transport(
            evaluator,
            presentation,
            route,
            try_publish,
            notify_frontend,
        )
    }

    fn attach_presentation_transport(
        mut evaluator: Context,
        presentation: EditorPresentationRuntime,
        route: impl Fn(&mut Context) -> SessionRedisplayAction + 'static,
        try_publish: impl Fn(SealedFramePresentation) -> bool + 'static,
        notify_frontend: impl Fn() + 'static,
    ) -> Self {
        evaluator.setup_thread_locals();
        presentation.install_evaluator_query_hooks(&mut evaluator);
        let transport = SessionPresentationTransport {
            runtime: presentation,
            route: Rc::new(route),
            try_publish: Rc::new(try_publish),
            notify_frontend: Rc::new(notify_frontend),
        };
        let redisplay_transport = transport.clone();
        evaluator.redisplay_fn = Some(Box::new(move |evaluator| {
            redisplay_transport.publish(evaluator);
        }));
        Self {
            evaluator,
            presentation: transport,
        }
    }

    /// Publish the currently visible frame forest immediately.
    pub fn publish_now(&mut self) -> FramePublishResult {
        self.presentation.publish(&mut self.evaluator)
    }

    /// Install the host suspension boundary used while this session waits for
    /// frontend input.
    ///
    /// Browser Workers use this to bridge the blocking GNU command loop to
    /// JSPI or an Atomics mailbox. Events still enter through the
    /// [`FrontendInputPort`], so host-specific wake mechanics cannot bypass
    /// the shared event validation and translation path.
    pub fn install_host_input_wait_backend(
        &mut self,
        backend: impl HostInputWaitBackend + 'static,
    ) {
        self.evaluator.install_host_input_wait_backend(backend);
    }
}

/// Bounded wait for a host that is being stopped.
///
/// Android runs `onStop` before the OS is free to kill the process, and the
/// kill arrives with no further notice, so the flush has to complete inline.
/// It also must not hang: an evaluator stuck in Lisp would turn a graceful
/// stop into an ANR, which is worse than a missed autosave. Autosave is
/// best-effort in GNU too.
pub const HOST_STOP_FLUSH_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(1500);

/// Outcome of [`FrontendInputPort::flush_for_host_stop`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostStopFlush {
    /// `do-auto-save` ran; buffers have recovery files.
    Flushed,
    /// It ran and signalled. Recorded, not retried.
    Failed,
    /// The evaluator did not answer inside [`HOST_STOP_FLUSH_TIMEOUT`].
    TimedOut,
    /// The evaluator was already gone.
    Disconnected,
}

impl FrontendInputPort {
    /// Ask the evaluator to autosave, and wait briefly for it to finish.
    ///
    /// Call this from the host's stop notification. It is deliberately
    /// blocking and deliberately bounded — see [`HOST_STOP_FLUSH_TIMEOUT`].
    pub fn flush_for_host_stop(&self) -> HostStopFlush {
        let (request, receiver) = neovm_core::PersistRequest::auto_save(std::sync::Arc::new(|| {}));
        if self
            .submit_batch(crate::evaluator_input::EvaluatorInputBatch::single(
                neovm_core::keyboard::InputEvent::PersistRequest(request),
            ))
            .is_err()
        {
            return HostStopFlush::Disconnected;
        }
        match receiver.recv_timeout(HOST_STOP_FLUSH_TIMEOUT) {
            Ok(neovm_core::PersistOutcome::Flushed) => HostStopFlush::Flushed,
            Ok(neovm_core::PersistOutcome::Failed) => HostStopFlush::Failed,
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => HostStopFlush::TimedOut,
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => HostStopFlush::Disconnected,
        }
    }
}
