//! Ordered terminal inspection, independent of semantic startup readiness.

use std::fmt;
use std::io;

use super::{ObservedTransportState, TuiSession};

/// An interactive inspection cannot certify output after transport closure.
#[derive(Debug, thiserror::Error)]
pub enum TuiObservationError {
    #[error("the PTY output stream is closed")]
    OutputClosed,
    #[error("the PTY observation owner is closed")]
    OwnerClosed,
    #[error("the ordered PTY observation failed: {0}")]
    Transport(#[source] io::Error),
}

/// A borrowed screen after all observations preceding one owner fence.
///
/// This view prevents further session operations while borrowed. It does not
/// certify startup completion, editor quiescence or future screen stability.
/// It owns no copied screen or reusable readiness certificate.
#[must_use = "inspect the ordered terminal view before advancing the session"]
#[repr(transparent)]
pub struct FencedTerminal<'session> {
    session: &'session TuiSession,
}

impl fmt::Debug for FencedTerminal<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("FencedTerminal")
            .field("session", &self.session.name)
            .field("size", &self.session.screen_size())
            .finish()
    }
}

impl FencedTerminal<'_> {
    /// Borrow the authoritative parser screen without creating another parser.
    pub fn screen(&self) -> &vt100::Screen {
        self.session.screen()
    }

    /// Read text through the existing session projection.
    pub fn text_grid(&self) -> Vec<String> {
        self.session.text_grid()
    }
}

impl TuiSession {
    /// Drain currently available PTY output through its sole owner, then borrow
    /// the resulting screen. No input or diagnostic marker is written.
    ///
    /// Every prior owner observation is applied to both the parser and recorder
    /// before this returns. This is an observation boundary, not a claim that
    /// startup or a redisplay command has finished. Output produced afterward
    /// is observed by a later fence or the normal input path.
    ///
    /// # Errors
    ///
    /// Returns a typed closure error when EOF/owner closure has been observed.
    /// Read, sequence, deadline, cancellation and recording failures propagate
    /// through the transport error; no view is returned on failure.
    pub fn observe_at_fence(&mut self) -> Result<FencedTerminal<'_>, TuiObservationError> {
        // A prior irreversible closure can refuse an inspection immediately.
        // No screen or scenario predicate is inspected before the fence.
        self.terminal.transport_state.require_interactive()?;
        let terminal = &mut self.terminal;
        self.output_pump
            .fence()
            .and_then(|ticket| ticket.finish(|event| terminal.apply_observation(event).map(|_| ())))
            .map_err(|error| TuiObservationError::Transport(io::Error::other(error)))?;
        self.terminal.transport_state.require_interactive()?;
        Ok(FencedTerminal { session: self })
    }
}

impl ObservedTransportState {
    fn require_interactive(self) -> Result<(), TuiObservationError> {
        match self {
            Self::Open => Ok(()),
            Self::OutputClosed => Err(TuiObservationError::OutputClosed),
            Self::OwnerClosed => Err(TuiObservationError::OwnerClosed),
        }
    }
}

static_assertions::assert_not_impl_any!(FencedTerminal<'static>: Clone, Copy, Send, Sync);
static_assertions::assert_eq_size!(FencedTerminal<'static>, &TuiSession);

#[cfg(test)]
#[path = "observation/tests.rs"]
mod tests;
