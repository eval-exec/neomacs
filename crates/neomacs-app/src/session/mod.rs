//! One evaluator session attached to a typed frontend transport.
//!
//! Platform adapters own their event loops and renderers. This module owns the
//! inverse side of that boundary: evaluator input, retained presentation
//! state, and renderer acknowledgements. Native hosts additionally expose a
//! blocking GNU command loop; browser WASM requires an async suspension
//! adapter which preserves the recursive Lisp stack.

mod transport;
std::cfg_select! {
    target_family = "wasm" => {}
    _ => {
        mod blocking;
        mod native_worker;
        pub use blocking::EditorSessionExit;
        pub use native_worker::{NativeEditorWorker, NativeEditorWorkerEvent};
    }
}

use std::rc::Rc;

use crossbeam_channel::{Sender, unbounded};
use neomacs_display_protocol::SealedFramePresentation;
use neovm_core::emacs_core::eval::Context;
use neovm_core::keyboard::HostInputWaitBackend;

use crate::presentation::{EditorPresentationRuntime, FramePublishResult, PresentationMetrics};

pub use transport::{
    ActiveFrontendPresentation, EditorFrontend, FrontendFrameInbox, FrontendFrameReceive,
    FrontendInputDisconnected, FrontendInputPort, FrontendInputSubmission, FrontendWake,
    PendingFrontendFrame,
};

/// Evaluator state wired to one frontend's input and presentation streams.
pub struct EditorSession {
    evaluator: Context,
    presentation: EditorPresentationRuntime,
    frame_tx: Sender<SealedFramePresentation>,
    notify_frontend: Rc<dyn Fn()>,
}

impl EditorSession {
    /// Attach an already initialized evaluator to a frontend.
    ///
    /// The caller must construct this value on the thread that will run Lisp.
    /// Android therefore calls it inside its evaluator worker, never on the
    /// Activity thread. Browser WASM will use the same attachment inside its
    /// Worker once its asynchronous suspension adapter is available.
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

        let presentation = EditorPresentationRuntime::new(metrics);
        presentation.install_evaluator_query_hooks(&mut evaluator);

        let (frame_tx, frame_rx) = unbounded();
        let notify_frontend: Rc<dyn Fn()> = Rc::new(notify_frontend);
        let redisplay_runtime = presentation.clone();
        let redisplay_tx = frame_tx.clone();
        let redisplay_notify = Rc::clone(&notify_frontend);
        evaluator.redisplay_fn = Some(Box::new(move |evaluator| {
            publish_presentations(
                &redisplay_runtime,
                evaluator,
                &redisplay_tx,
                redisplay_notify.as_ref(),
            );
        }));

        let frontend = EditorFrontend::new(input, frame_rx);
        (
            Self {
                evaluator,
                presentation,
                frame_tx,
                notify_frontend,
            },
            frontend,
        )
    }

    /// Publish the currently visible frame forest immediately.
    pub fn publish_now(&mut self) -> FramePublishResult {
        publish_presentations(
            &self.presentation,
            &mut self.evaluator,
            &self.frame_tx,
            self.notify_frontend.as_ref(),
        )
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

fn publish_presentations(
    presentation: &EditorPresentationRuntime,
    evaluator: &mut Context,
    frame_tx: &Sender<SealedFramePresentation>,
    notify_frontend: &dyn Fn(),
) -> FramePublishResult {
    let result =
        presentation.publish_visible_frames(evaluator, |frame| frame_tx.try_send(frame).is_ok());
    if result.published() > 0 {
        notify_frontend();
    }
    result
}
