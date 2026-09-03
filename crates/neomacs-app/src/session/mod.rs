//! One evaluator session attached to a typed frontend transport.
//!
//! Platform adapters own their event loops and renderers. This module owns the
//! inverse side of that boundary: evaluator input, retained presentation
//! state, and renderer acknowledgements. Native hosts additionally expose a
//! blocking GNU command loop; browser WASM requires an async suspension
//! adapter which preserves the recursive Lisp stack.

mod blocking;
mod transport;
pub use blocking::{EditorSessionExit, StoppedEditorSession};
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
            move |frame| frame_tx.try_send(frame).is_ok(),
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
