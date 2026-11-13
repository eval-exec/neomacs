//! Evaluator-bound input reception (stage-6 S6-2 split).
//!
//! `HostInputReceiver` is a `Context` field, so the receiving half and the
//! Context-owned test helper stay in eval.  The publisher service lives in
//! `emacs_core::host_input` (system) and re-exports this type for the
//! `crate::host_input` facade; the publisher reaches the receiver downward.

#[cfg(any(test, feature = "redisplay-test-policy", feature = "test-support"))]
use super::Context;
use super::keyboard::InputEvent;
use crossbeam_channel::Receiver;

/// The receiving half is constructed only with its notifying publisher.
/// Context owns this optional host connection; absence still denotes batch input.
#[derive(Default, Debug)]
#[repr(transparent)]
#[doc(hidden)]
pub struct HostInputReceiver(Option<Receiver<InputEvent>>);

impl HostInputReceiver {
    /// Pair a fresh receiver with the Context (the publisher service owns the
    /// sending half and installs through this constructor).
    #[inline]
    #[doc(hidden)]
    pub fn from_receiver(receiver: Receiver<InputEvent>) -> Self {
        Self(Some(receiver))
    }

    #[inline]
    #[doc(hidden)]
    pub fn as_ref(&self) -> Option<&Receiver<InputEvent>> {
        self.0.as_ref()
    }

    #[doc(hidden)]
    pub fn clear(&mut self) {
        self.0 = None;
    }
}

static_assertions::assert_eq_size!(HostInputReceiver, Option<Receiver<InputEvent>>);
static_assertions::assert_eq_align!(HostInputReceiver, Option<Receiver<InputEvent>>);

#[cfg(any(test, feature = "redisplay-test-policy", feature = "test-support"))]
impl Context {
    /// Observe host input in dependent frontend fixtures without a raw receiver.
    #[doc(hidden)]
    pub fn receive_host_input_for_test(
        &self,
        timeout: std::time::Duration,
    ) -> Result<InputEvent, crossbeam_channel::RecvTimeoutError> {
        self.input_rx
            .as_ref()
            .ok_or(crossbeam_channel::RecvTimeoutError::Disconnected)?
            .recv_timeout(timeout)
    }
}
