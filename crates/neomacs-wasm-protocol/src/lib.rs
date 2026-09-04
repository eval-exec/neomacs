//! Version-independent browser-to-editor input batches.
//!
//! Browser callbacks and Worker messages are not editor semantics. This module
//! validates their wire-shaped values as one atomic batch and only then
//! exposes the shared [`neomacs_app::session`] input vocabulary.

use std::fmt::{Display, Formatter};
use std::num::NonZeroU64;

use neomacs_app::frontend_event::{
    FrontendEvent, FrontendFrameId, FrontendKeyEvent, FrontendKeyState, FrontendKeySymbol,
    FrontendLogicalExtent, FrontendModifiers, FrontendPresentationId, FrontendViewport,
};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Wire contract understood by this browser frontend and editor Worker.
///
/// Version 3 keeps browser-observed viewport geometry separate from
/// editor-owned font-cell measurement. Device scale remains an independent
/// observation applied by the renderer.
pub const WORKER_PROTOCOL_VERSION: u16 = 3;

/// Browser color preference sampled for the initial editor frame.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BrowserColorScheme {
    Light,
    Dark,
}

/// Complete initial-surface facts delivered before the Worker restores Lisp.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BrowserEditorStartup {
    protocol_version: u16,
    width: u32,
    height: u32,
    scale_factor: f64,
    font_pixel_size: f32,
    color_scheme: BrowserColorScheme,
}

impl BrowserEditorStartup {
    /// Construct and validate the browser's opening logical geometry.
    pub fn new(
        logical_extent: FrontendLogicalExtent,
        scale_factor: f64,
        font_pixel_size: f32,
        color_scheme: BrowserColorScheme,
    ) -> Result<Self, InvalidBrowserEditorStartup> {
        let width = logical_extent.width();
        let height = logical_extent.height();
        let startup = Self {
            protocol_version: WORKER_PROTOCOL_VERSION,
            width,
            height,
            scale_factor,
            font_pixel_size,
            color_scheme,
        };
        startup.validate()?;
        Ok(startup)
    }

    /// Reject stale wire versions and geometry that cannot form a frame.
    pub fn validate(&self) -> Result<(), InvalidBrowserEditorStartup> {
        if self.protocol_version != WORKER_PROTOCOL_VERSION {
            return Err(InvalidBrowserEditorStartup::UnsupportedProtocol {
                found: self.protocol_version,
            });
        }
        if self.width == 0 || self.height == 0 {
            return Err(InvalidBrowserEditorStartup::EmptyExtent);
        }
        if !self.scale_factor.is_finite() || self.scale_factor <= 0.0 {
            return Err(InvalidBrowserEditorStartup::ScaleFactor);
        }
        if !positive_finite(self.font_pixel_size) {
            return Err(InvalidBrowserEditorStartup::FontPixelSize);
        }
        Ok(())
    }

    #[must_use]
    pub const fn protocol_version(&self) -> u16 {
        self.protocol_version
    }

    #[must_use]
    /// Logical editor extent, before applying device scale.
    pub const fn logical_extent(&self) -> FrontendLogicalExtent {
        FrontendLogicalExtent::new(self.width, self.height)
    }

    #[must_use]
    pub const fn scale_factor(&self) -> f64 {
        self.scale_factor
    }

    #[must_use]
    pub const fn font_pixel_size(&self) -> f32 {
        self.font_pixel_size
    }

    #[must_use]
    pub const fn color_scheme(&self) -> BrowserColorScheme {
        self.color_scheme
    }
}

const fn positive_finite(value: f32) -> bool {
    value.is_finite() && value > 0.0
}

/// Invalid browser facts rejected before restoring the runtime image.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InvalidBrowserEditorStartup {
    UnsupportedProtocol { found: u16 },
    EmptyExtent,
    ScaleFactor,
    FontPixelSize,
}

impl Display for InvalidBrowserEditorStartup {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedProtocol { found } => write!(
                formatter,
                "unsupported browser Worker protocol {found}; expected {WORKER_PROTOCOL_VERSION}"
            ),
            Self::EmptyExtent => formatter.write_str("browser frame extent must be nonzero"),
            Self::ScaleFactor => {
                formatter.write_str("browser scale factor must be finite and positive")
            }
            Self::FontPixelSize => {
                formatter.write_str("browser font size must be finite and positive")
            }
        }
    }
}

impl std::error::Error for InvalidBrowserEditorStartup {}

/// Monotonic identity echoed by the Worker after accepting an input batch.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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

impl Serialize for InputBatchSequence {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.get().to_string())
    }
}

impl<'de> Deserialize<'de> for InputBatchSequence {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = String::deserialize(deserializer)?;
        let value = wire.parse::<u64>().map_err(serde::de::Error::custom)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

mod decimal_u64 {
    use serde::{Deserialize, Deserializer, Serializer};

    pub(super) fn serialize<S>(value: &u64, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&value.to_string())
    }

    pub(super) fn deserialize<'de, D>(deserializer: D) -> Result<u64, D::Error>
    where
        D: Deserializer<'de>,
    {
        String::deserialize(deserializer)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}

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
        #[serde(with = "decimal_u64")]
        target: u64,
    },
    /// Unicode text committed by the browser's input method.
    TextCommitted {
        text: String,
        #[serde(with = "decimal_u64")]
        target: u64,
    },
    /// Logical browser viewport extent paired with its logical-to-device scale.
    ViewportChanged {
        width: u32,
        height: u32,
        scale_factor: f64,
        #[serde(with = "decimal_u64")]
        target: u64,
    },
    /// Browser focus changed.
    FocusChanged {
        focused: bool,
        #[serde(with = "decimal_u64")]
        target: u64,
    },
    /// The page requested editor shutdown.
    CloseRequested {
        #[serde(with = "decimal_u64")]
        target: u64,
    },
    /// The renderer installed an immutable presentation.
    PresentationActivated {
        #[serde(with = "decimal_u64")]
        presentation: u64,
        #[serde(with = "decimal_u64")]
        target: u64,
    },
    /// The renderer rejected a presentation before installation.
    PresentationDiscarded {
        #[serde(with = "decimal_u64")]
        presentation: u64,
        #[serde(with = "decimal_u64")]
        target: u64,
    },
    /// A formerly visible presentation can no longer produce input hits.
    PresentationRetired {
        #[serde(with = "decimal_u64")]
        presentation: u64,
    },
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
                FrontendViewport::new(
                    FrontendLogicalExtent::new(width, height),
                    scale_factor,
                    FrontendFrameId::new(target),
                )
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
