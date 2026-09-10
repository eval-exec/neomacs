//! Nonblocking input-method access to the VM's ordered input stream.

use std::sync::Arc;

use crossbeam_channel::{Receiver, TryRecvError};
pub use neovm_core::ImeEditorError;
use neovm_core::{ImeRequest, keyboard::InputEvent};
use neovm_host_abi::ime::{
    ImeSelection, ImeSelectionAcknowledgement, ImeSelectionOutcome, ImeTextSnapshot,
};

use super::{FrontendInputDisconnected, FrontendInputPort};
use crate::evaluator_input::EvaluatorInputBatch;

/// Event-loop client; owns no evaluator or borrowed editor state.
#[derive(Clone)]
pub struct ImeClient {
    input: FrontendInputPort,
    notify: Arc<dyn Fn() + Send + Sync>,
}

impl FrontendInputPort {
    /// Attach an IME client to this same ordered input stream.
    /// `notify` runs on the VM thread and must only wake the frontend, without
    /// blocking, panicking, or calling back into the evaluator.
    pub fn ime_client(&self, notify: impl Fn() + Send + Sync + 'static) -> ImeClient {
        ImeClient {
            input: self.clone(),
            notify: Arc::new(notify),
        }
    }
}

impl ImeClient {
    /// Apply a snapshot-qualified selection and return the current observation
    /// in the same VM turn. A stale selection is not retried. The observation
    /// may be absent under privacy or presentation restrictions; it retires
    /// earlier observations just like `surrounding_text`.
    pub fn select_and_observe(
        &self,
        selection: ImeSelection,
    ) -> Result<
        PendingImeReply<Result<ImeSelectionAcknowledgement, ImeEditorError>>,
        FrontendInputDisconnected,
    > {
        let (request, receiver) = ImeRequest::select_and_observe(selection, self.notify.clone());
        self.input
            .submit_batch(EvaluatorInputBatch::single(InputEvent::ImeRequest(request)))?;
        Ok(PendingImeReply { receiver })
    }

    /// Apply offsets from a previously observed snapshot in input order.
    /// Stale snapshots are rejected; failures never retry against current text.
    pub fn select(
        &self,
        selection: ImeSelection,
    ) -> Result<
        PendingImeReply<Result<ImeSelectionOutcome, ImeEditorError>>,
        FrontendInputDisconnected,
    > {
        let (request, receiver) = ImeRequest::set_selection(selection, self.notify.clone());
        self.input
            .submit_batch(EvaluatorInputBatch::single(InputEvent::ImeRequest(request)))?;
        Ok(PendingImeReply { receiver })
    }

    /// Request the current insertion snapshot at the next ordered input read.
    ///
    /// Capturing a new snapshot retires the previous observation. The VM
    /// enforces privacy, field bounds, and GUI presentation freshness. Keep
    /// system keyboard export disabled until the backend also preserves the
    /// source identity of asynchronous callbacks.
    pub fn surrounding_text(
        &self,
    ) -> Result<PendingImeReply<Option<ImeTextSnapshot>>, FrontendInputDisconnected> {
        let (request, receiver) = ImeRequest::surrounding_text(self.notify.clone());
        self.input
            .submit_batch(EvaluatorInputBatch::single(InputEvent::ImeRequest(request)))?;
        Ok(PendingImeReply { receiver })
    }
}

/// One owned reply. Dropping it abandons the response, not the queued request.
/// Hosts must also observe session exit: an unanswered request dropped during
/// shutdown does not emit a reply notification.
#[must_use]
pub struct PendingImeReply<T> {
    receiver: Receiver<T>,
}

/// Result of a nonblocking reply poll.
#[derive(Debug)]
pub enum ImeReply<T> {
    /// The evaluator has not answered yet.
    Pending,
    /// The response, delivered once.
    Ready(T),
    /// All senders disappeared, or the response was already consumed.
    Disconnected,
}

impl<T> PendingImeReply<T> {
    /// Poll without ever blocking a browser or native event loop.
    pub fn try_receive(&self) -> ImeReply<T> {
        match self.receiver.try_recv() {
            Ok(value) => ImeReply::Ready(value),
            Err(TryRecvError::Empty) => ImeReply::Pending,
            Err(TryRecvError::Disconnected) => ImeReply::Disconnected,
        }
    }
}
