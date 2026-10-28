//! Cross-thread host input for evaluator fixtures.

use crate::emacs_core::Context;
use crate::emacs_core::process::WaitNotifier;
use crate::keyboard::InputEvent;

/// Test input producers share a receiver with one evaluator. Clones may publish
/// from other threads; queued input must also wake that evaluator's poller.
///
/// This is the frontend input bridge's contract (`Context::init_input_system`):
/// a channel send alone does not wake the unified process/input wait, so a
/// fixture that sends without notifying is observed only when some unrelated
/// timer or GC wake happens to follow it. Every fixture that delivers input
/// while the evaluator may already be waiting publishes through this type.
#[derive(Clone)]
pub struct InputPublisher {
    sender: crossbeam_channel::Sender<InputEvent>,
    notifier: Option<WaitNotifier>,
}

impl std::fmt::Debug for InputPublisher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InputPublisher")
            .field("has_notifier", &self.notifier.is_some())
            .finish_non_exhaustive()
    }
}

static_assertions::assert_impl_all!(InputPublisher: Send, Sync, Clone, std::fmt::Debug);

#[derive(Debug, thiserror::Error)]
pub enum InputPublishError {
    #[error("input receiver disconnected")]
    Disconnected(#[from] crossbeam_channel::SendError<InputEvent>),
    #[error("input poller notification failed")]
    Notify(#[from] std::io::Error),
}

impl InputPublisher {
    /// Connect a fresh input channel to `context` and return its publisher.
    pub fn install(context: &mut Context) -> Self {
        let (sender, receiver) = crossbeam_channel::unbounded();
        context.input_rx = Some(receiver);
        Self {
            sender,
            notifier: context.wait_notifier(),
        }
    }

    pub fn send(&self, event: InputEvent) -> Result<(), InputPublishError> {
        self.sender.send(event)?;
        // Publish first: the wake must never race ahead of its queued payload.
        // Poller notifications are remembered even before kernel wait begins.
        if let Some(notifier) = &self.notifier {
            notifier.notify()?;
        }
        Ok(())
    }
}
