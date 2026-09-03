//! Version-independent browser-to-editor input batches.
//!
//! Browser callbacks and Worker messages are not editor semantics. This module
//! validates their wire-shaped values as one atomic batch and only then
//! exposes the shared [`neomacs_app::session`] input vocabulary.

use std::fmt::{Display, Formatter};
use std::num::NonZeroU64;

use neomacs_app::frontend_event::{
    FrontendEvent, FrontendFrameId, FrontendKeyEvent, FrontendKeyState, FrontendKeySymbol,
    FrontendModifiers, FrontendPresentationId, FrontendViewport,
};
use serde::{Deserialize, Serialize};

/// Monotonic identity echoed by the Worker after accepting an input batch.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct InputBatchSequence(NonZeroU64);

impl InputBatchSequence {
    /// Construct a sequence identity. Zero is reserved for "no batch".
    pub const fn new(value: u64) -> Result<Self, InvalidInputBatchSequence> {
        match NonZeroU64::new(value) {
            Some(value) => Ok(Self(value)),
            None => Err(InvalidInputBatchSequence),
        }
    }

    /// Return the wire value.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

/// Zero cannot identify an input batch.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InvalidInputBatchSequence;

impl Display for InvalidInputBatchSequence {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("browser input batch sequence must be nonzero")
    }
}

impl std::error::Error for InvalidInputBatchSequence {}

/// Modifier state sampled with one browser key event.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct BrowserModifiers {
    shift: bool,
    control: bool,
    meta: bool,
    super_: bool,
}

impl BrowserModifiers {
    /// Construct one atomic modifier sample.
    #[must_use]
    pub const fn new(shift: bool, control: bool, meta: bool, super_: bool) -> Self {
        Self {
            shift,
            control,
            meta,
            super_,
        }
    }

    const fn into_frontend(self) -> FrontendModifiers {
        FrontendModifiers::new(self.shift, self.control, self.meta, self.super_)
    }
}

/// Browser key activation state.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BrowserKeyState {
    /// The key became active.
    Pressed,
    /// The key became inactive.
    Released,
}

impl BrowserKeyState {
    const fn into_frontend(self) -> FrontendKeyState {
        match self {
            Self::Pressed => FrontendKeyState::Pressed,
            Self::Released => FrontendKeyState::Released,
        }
    }
}

/// One structured-clone-safe observation sent by the browser frontend.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum BrowserInputEvent {
    /// Logical keyboard input after browser-key translation.
    Key {
        symbol: u32,
        modifiers: BrowserModifiers,
        state: BrowserKeyState,
        target: u64,
    },
    /// Unicode text committed by the browser's input method.
    TextCommitted { text: String, target: u64 },
    /// Physical canvas extent paired with its logical/device scale.
    ViewportChanged {
        width: u32,
        height: u32,
        scale_factor: f64,
        target: u64,
    },
    /// Browser focus changed.
    FocusChanged { focused: bool, target: u64 },
    /// The page requested editor shutdown.
    CloseRequested { target: u64 },
    /// The renderer installed an immutable presentation.
    PresentationActivated { presentation: u64, target: u64 },
    /// The renderer rejected a presentation before installation.
    PresentationDiscarded { presentation: u64, target: u64 },
    /// A formerly visible presentation can no longer produce input hits.
    PresentationRetired { presentation: u64 },
}

impl BrowserInputEvent {
    /// Construct logical keyboard input.
    #[must_use]
    pub const fn key(
        symbol: u32,
        modifiers: BrowserModifiers,
        state: BrowserKeyState,
        target: u64,
    ) -> Self {
        Self::Key {
            symbol,
            modifiers,
            state,
            target,
        }
    }

    /// Construct committed Unicode input.
    #[must_use]
    pub fn text_committed(text: impl Into<String>, target: u64) -> Self {
        Self::TextCommitted {
            text: text.into(),
            target,
        }
    }

    /// Construct presentation retirement feedback.
    #[must_use]
    pub const fn presentation_retired(presentation: u64) -> Self {
        Self::PresentationRetired { presentation }
    }

    fn try_into_frontend(self) -> Result<FrontendEvent, InvalidFrontendObservation> {
        Ok(match self {
            Self::Key {
                symbol,
                modifiers,
                state,
                target,
            } => FrontendEvent::Key(FrontendKeyEvent::new(
                FrontendKeySymbol::new(symbol),
                modifiers.into_frontend(),
                state.into_frontend(),
                FrontendFrameId::new(target),
            )),
            Self::TextCommitted { text, target } => {
                FrontendEvent::text_committed(text, FrontendFrameId::new(target))
            }
            Self::ViewportChanged {
                width,
                height,
                scale_factor,
                target,
            } => FrontendEvent::ViewportChanged(
                FrontendViewport::new(width, height, scale_factor, FrontendFrameId::new(target))
                    .map_err(|_| InvalidFrontendObservation::InvalidScaleFactor)?,
            ),
            Self::FocusChanged { focused, target } => FrontendEvent::FocusChanged {
                focused,
                target: FrontendFrameId::new(target),
            },
            Self::CloseRequested { target } => FrontendEvent::CloseRequested {
                target: FrontendFrameId::new(target),
            },
            Self::PresentationActivated {
                presentation,
                target,
            } => FrontendEvent::PresentationActivated {
                presentation: FrontendPresentationId::new(presentation),
                target: FrontendFrameId::new(target),
            },
            Self::PresentationDiscarded {
                presentation,
                target,
            } => FrontendEvent::PresentationDiscarded {
                presentation: FrontendPresentationId::new(presentation),
                target: FrontendFrameId::new(target),
            },
            Self::PresentationRetired { presentation } => FrontendEvent::PresentationRetired {
                presentation: FrontendPresentationId::new(presentation),
            },
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum InvalidFrontendObservation {
    InvalidScaleFactor,
}

/// A browser callback batch before editor-boundary validation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BrowserInputBatch {
    sequence: InputBatchSequence,
    events: Vec<BrowserInputEvent>,
}

impl BrowserInputBatch {
    /// Construct a nonempty browser batch.
    pub fn new(
        sequence: InputBatchSequence,
        events: Vec<BrowserInputEvent>,
    ) -> Result<Self, InvalidBrowserInputBatch> {
        if events.is_empty() {
            return Err(InvalidBrowserInputBatch::Empty);
        }
        Ok(Self { sequence, events })
    }

    /// Validate every observation before exposing any editor input.
    pub fn try_into_frontend_batch(
        self,
    ) -> Result<ValidatedFrontendInputBatch, InvalidBrowserInputBatch> {
        if self.events.is_empty() {
            return Err(InvalidBrowserInputBatch::Empty);
        }
        let mut events = Vec::with_capacity(self.events.len());
        for (event_index, event) in self.events.into_iter().enumerate() {
            match event.try_into_frontend() {
                Ok(event) => events.push(event),
                Err(InvalidFrontendObservation::InvalidScaleFactor) => {
                    return Err(InvalidBrowserInputBatch::InvalidScaleFactor { event_index });
                }
            }
        }
        Ok(ValidatedFrontendInputBatch {
            sequence: self.sequence,
            events,
        })
    }
}

/// Browser batch rejected before reaching the editor session.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InvalidBrowserInputBatch {
    /// A message with no observations is not an input batch.
    Empty,
    /// A viewport observation carried a non-finite or non-positive scale.
    InvalidScaleFactor { event_index: usize },
}

impl Display for InvalidBrowserInputBatch {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => formatter.write_str("browser input batch must not be empty"),
            Self::InvalidScaleFactor { event_index } => write!(
                formatter,
                "browser input event {event_index} has an invalid scale factor"
            ),
        }
    }
}

impl std::error::Error for InvalidBrowserInputBatch {}

/// One fully validated, ordered batch accepted by the shared editor session.
#[derive(Clone, Debug, PartialEq)]
pub struct ValidatedFrontendInputBatch {
    sequence: InputBatchSequence,
    events: Vec<FrontendEvent>,
}

impl ValidatedFrontendInputBatch {
    /// Identity to acknowledge after all events have been submitted.
    #[must_use]
    pub const fn sequence(&self) -> InputBatchSequence {
        self.sequence
    }

    /// Editor-session observations in browser delivery order.
    #[must_use]
    pub fn events(&self) -> &[FrontendEvent] {
        &self.events
    }

    /// Consume the batch for submission to an editor input port.
    pub fn into_events(self) -> impl ExactSizeIterator<Item = FrontendEvent> {
        self.events.into_iter()
    }
}
