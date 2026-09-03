//! Host-owned suspension contract for waits that admit editor input.

use std::time::Duration;

/// Failure reported by a host while suspending for editor input.
///
/// The host wait is deliberately separate from input delivery: a backend
/// wakes the evaluator, but the existing typed input channel remains the sole
/// source of editor events.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HostInputWaitError {
    message: String,
}

impl HostInputWaitError {
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for HostInputWaitError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for HostInputWaitError {}

/// Host-defined suspension point used while the evaluator waits for input.
///
/// Browser Workers implement this with JSPI (or a blocking Atomics mailbox
/// fallback). Native sessions normally leave it unset and continue through
/// the OS process/input poller. Returning does not itself claim that input
/// arrived: the evaluator always consults its typed input channel afterward.
pub trait HostInputWaitBackend {
    fn wait_for_input(&mut self, timeout: Duration) -> Result<(), HostInputWaitError>;
}
