//! neo-term vocabulary: the terminal emulator instances the evaluator asks a
//! display to host. M15 moves neo-term to the evaluator and removes these.
//!
//! Not behind this crate's `neo-term` feature: the evaluator names them in its
//! display-host interface whether or not a frontend renders terminals, and the
//! evaluator has no such feature to forward.

use std::num::{NonZeroU16, NonZeroU32};

/// The display object that owns a neo-term instance.
///
/// Window terminals carry their buffer identity in the variant itself, so a
/// detached frame-wide `Window` state cannot be constructed accidentally.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum TerminalDisplayTarget {
    /// `buffer` is the evaluator buffer id that `WindowInfo::buffer_id` names.
    Window {
        buffer: u64,
    },
    Inline,
    Floating,
}

/// A live terminal identifier. Zero is reserved as the Lisp failure sentinel,
/// so it cannot cross the typed display-host boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct TerminalId(NonZeroU32);

impl TerminalId {
    #[must_use]
    pub fn new(id: u32) -> Option<Self> {
        NonZeroU32::new(id).map(Self)
    }

    #[must_use]
    pub fn get(self) -> u32 {
        self.0.get()
    }
}

impl std::fmt::Display for TerminalId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

/// Non-empty terminal grid dimensions validated at the Lisp boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TerminalGridSize {
    pub cols: NonZeroU16,
    pub rows: NonZeroU16,
}

impl TerminalGridSize {
    #[must_use]
    pub fn new(cols: u16, rows: u16) -> Option<Self> {
        Some(Self {
            cols: NonZeroU16::new(cols)?,
            rows: NonZeroU16::new(rows)?,
        })
    }
}

/// Position and opacity of a floating terminal. Decoding validates it.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(
    try_from = "RawTerminalFloatPlacement",
    into = "RawTerminalFloatPlacement"
)]
pub struct TerminalFloatPlacement {
    x: f32,
    y: f32,
    opacity: f32,
}

#[derive(Clone, Copy, serde::Serialize, serde::Deserialize)]
struct RawTerminalFloatPlacement {
    x: f32,
    y: f32,
    opacity: f32,
}

/// Coordinates that are not finite, or an opacity outside `0.0..=1.0`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidTerminalFloatPlacement;

impl std::fmt::Display for InvalidTerminalFloatPlacement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("terminal placement needs finite coordinates and an opacity in 0..=1")
    }
}

impl TryFrom<RawTerminalFloatPlacement> for TerminalFloatPlacement {
    type Error = InvalidTerminalFloatPlacement;

    fn try_from(raw: RawTerminalFloatPlacement) -> Result<Self, Self::Error> {
        Self::new(raw.x, raw.y, raw.opacity).ok_or(InvalidTerminalFloatPlacement)
    }
}

impl From<TerminalFloatPlacement> for RawTerminalFloatPlacement {
    fn from(placement: TerminalFloatPlacement) -> Self {
        Self {
            x: placement.x,
            y: placement.y,
            opacity: placement.opacity,
        }
    }
}

impl TerminalFloatPlacement {
    #[must_use]
    pub fn new(x: f32, y: f32, opacity: f32) -> Option<Self> {
        (x.is_finite() && y.is_finite() && opacity.is_finite() && (0.0..=1.0).contains(&opacity))
            .then_some(Self { x, y, opacity })
    }

    #[must_use]
    pub fn x(self) -> f32 {
        self.x
    }

    #[must_use]
    pub fn y(self) -> f32 {
        self.y
    }

    #[must_use]
    pub fn opacity(self) -> f32 {
        self.opacity
    }
}
