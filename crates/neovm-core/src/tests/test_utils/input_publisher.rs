//! Cross-thread host input for evaluator fixtures.

use crate::emacs_core::Context;
use crate::emacs_core::eval::QuitRequest;
use crate::emacs_core::process::WaitNotifier;
use crate::keyboard::InputEvent;

/// Native producers for one evaluator's unbounded input queue.
#[derive(Clone)]
pub struct InputPublisher {
    sender: crossbeam_channel::Sender<InputEvent>,
    notifier: Option<WaitNotifier>,
    quit: QuitRequest,
}

static_assertions::assert_impl_all!(InputPublisher: Send, Sync, Clone, std::fmt::Debug);

impl std::fmt::Debug for InputPublisher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InputPublisher").finish_non_exhaustive()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum InputPublishError {
    #[error("input receiver disconnected")]
    Disconnected(#[from] crossbeam_channel::SendError<InputEvent>),
    #[error("input poller notification failed")]
    Notify(#[from] std::io::Error),
}

impl InputPublisher {
    pub fn install(context: &mut Context) -> Self {
        let (sender, receiver) = crossbeam_channel::unbounded();
        context.input_rx = Some(receiver);
        Self {
            sender,
            notifier: context.wait_notifier(),
            quit: context.quit_requested.clone(),
        }
    }

    pub fn send(&self, event: InputEvent) -> Result<(), InputPublishError> {
        self.send_batch(vec![event])
    }

    pub(crate) fn send_batch(&self, events: Vec<InputEvent>) -> Result<(), InputPublishError> {
        for event in events {
            self.sender.send(event)?;
        }
        // Enqueue before wake. A failed wake never retries a committed event.
        if let Some(notifier) = &self.notifier {
            notifier.notify()?;
        }
        Ok(())
    }

    pub(crate) fn cancel_wait(&self) -> Result<(), InputPublishError> {
        // A missing witness never authorizes the normal test input. Quit is a
        // distinct failure-only cancellation of the evaluator's pending wait.
        self.quit.request();
        if let Some(notifier) = &self.notifier {
            notifier.notify()?;
        }
        Ok(())
    }
}
