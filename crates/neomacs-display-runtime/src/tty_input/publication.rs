//! Owned facts delivered by the primary Unix reader.

#[cfg(unix)]
use super::{TerminalCellSize, raw_tty_input_event};

/// The publication has either committed or its sole receiver is closed.
/// Notification failure after commit must still return `Delivered`: retrying
/// would duplicate the input already owned by the core queue.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[must_use]
pub enum Publication {
    Delivered,
    ReceiverClosed,
}

/// Primary-terminal input before the shared display-to-core conversion.
#[cfg(unix)]
#[derive(Debug)]
pub enum PrimaryTtyInput {
    Bytes(Vec<u8>),
    Resize(TerminalCellSize),
}

#[cfg(unix)]
impl From<PrimaryTtyInput> for crate::thread_comm::InputEvent {
    fn from(input: PrimaryTtyInput) -> Self {
        match input {
            PrimaryTtyInput::Bytes(bytes) => raw_tty_input_event(bytes),
            PrimaryTtyInput::Resize(size) => Self::WindowResize {
                width: u32::from(size.columns()),
                height: u32::from(size.rows()),
                scale_factor: 1.0,
                emacs_frame_id: 0,
            },
        }
    }
}

/// A native-only publication owner moved into the primary reader thread.
/// Production implementations own queue/quit/wakeup handles, never evaluator
/// or Lisp references. The unbounded queue admits each consumed input once.
#[cfg(unix)]
pub trait PrimaryTtyPublisher: Send + std::fmt::Debug + 'static {
    fn publish(&mut self, input: PrimaryTtyInput) -> Publication;
}

static_assertions::assert_impl_all!(Publication: Send, Copy, std::fmt::Debug);
#[cfg(unix)]
static_assertions::assert_impl_all!(PrimaryTtyInput: Send, std::fmt::Debug);
