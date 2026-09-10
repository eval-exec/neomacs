//! Ordered input-method requests. Replies carry owned data, never Lisp values.

use std::sync::Arc;

use crossbeam_channel::{Receiver, Sender, bounded};
use neovm_host_abi::ime::{ImeSelection, ImeSelectionOutcome, ImeTextSnapshot};

/// A request failed inside Lisp. The original nonlocal exit stays on the VM
/// thread and follows normal editor error/quit handling.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImeEditorError;

/// Completion endpoint carried through the evaluator's input queue.
#[derive(Clone)]
pub struct ImeReplySender<T> {
    sender: Sender<T>,
    notify: Arc<dyn Fn() + Send + Sync>,
}

impl<T> std::fmt::Debug for ImeReplySender<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ImeReplySender").finish_non_exhaustive()
    }
}

impl<T> ImeReplySender<T> {
    fn channel(notify: Arc<dyn Fn() + Send + Sync>) -> (Self, Receiver<T>) {
        let (sender, receiver) = bounded(1);
        (Self { sender, notify }, receiver)
    }

    fn complete(self, value: T) {
        // The frontend may have disappeared or abandoned its request. Never
        // block the VM waiting for a receiver, including during shutdown.
        if self.sender.try_send(value).is_ok() {
            (self.notify)();
        }
    }
}

/// An observation serviced in input order at the evaluator's next input read.
///
/// This is not an urgent interrupt or an arbitrary-Lisp execution barrier:
/// Lisp can itself read input, including inside a recursive minibuffer.
#[derive(Clone, Debug)]
pub enum ImeRequest {
    /// Obtain a bounded snapshot of the current insertion context.
    SurroundingText(ImeReplySender<Option<ImeTextSnapshot>>),
    /// Apply a selection only to the exact captured editor context.
    SetSelection {
        selection: ImeSelection,
        reply: ImeReplySender<Result<ImeSelectionOutcome, ImeEditorError>>,
    },
}

impl ImeRequest {
    /// Create a query and its nonblocking frontend reply channel.
    /// The notification must be a short, non-panicking event-loop wake.
    pub fn surrounding_text(
        notify: Arc<dyn Fn() + Send + Sync>,
    ) -> (Self, Receiver<Option<ImeTextSnapshot>>) {
        let (reply, receiver) = ImeReplySender::channel(notify);
        (Self::SurroundingText(reply), receiver)
    }

    /// Create a snapshot-qualified selection request and its reply channel.
    /// The notification follows the same contract as `surrounding_text`.
    pub fn set_selection(
        selection: ImeSelection,
        notify: Arc<dyn Fn() + Send + Sync>,
    ) -> (Self, Receiver<Result<ImeSelectionOutcome, ImeEditorError>>) {
        let (reply, receiver) = ImeReplySender::channel(notify);
        (Self::SetSelection { selection, reply }, receiver)
    }

    pub(crate) fn dispatch(self, context: &mut crate::Context) -> Result<(), crate::Flow> {
        match self {
            Self::SurroundingText(reply) => reply.complete(context.ime_surrounding_text()),
            Self::SetSelection { selection, reply } => {
                let result = context.ime_set_selection(selection);
                reply.complete(result.as_ref().copied().map_err(|_| ImeEditorError));
                result?;
            }
        }
        Ok(())
    }
}
