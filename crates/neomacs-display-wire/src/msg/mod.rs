//! NDP/1 message families (DESIGN §6.8), revision 1.0.
//!
//! Vocabulary types travel through their display-protocol serde derives;
//! this module adds only what exists per session on the wire: channel,
//! request and conversion ids, update sequence numbers, the lane each message
//! uses, and the shapes of the messages themselves.

pub mod down;
pub mod up;

use std::num::NonZeroU64;

use serde::{Deserialize, Serialize};

pub use down::Downlink;
pub use up::Uplink;

/// The display service's lane assignment (DESIGN §6.4). The numbering is the
/// session record's lane byte; the session crate names the same lanes, and
/// the endpoint that binds the two maps them one to one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum Lane {
    Control = 0,
    Input = 1,
    Present = 2,
    Facts = 3,
    Hint = 4,
    Bulk = 5,
}

/// One Lisp terminal of type `neo` on a session (DESIGN §6.7).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ChannelId(NonZeroU64);

impl ChannelId {
    #[must_use]
    pub const fn new(raw: NonZeroU64) -> Self {
        Self(raw)
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

/// The evaluator's terminal number for a channel (`next_terminal_id`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct TerminalNumber(u32);

impl TerminalNumber {
    #[must_use]
    pub const fn new(raw: u32) -> Self {
        Self(raw)
    }

    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// The exact display name Lisp used to open a channel (`neo:LABEL`, C5).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DisplayName(pub String);

/// A correlated evaluator request (DESIGN §6.13).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct RequestId(NonZeroU64);

impl RequestId {
    #[must_use]
    pub const fn new(raw: NonZeroU64) -> Self {
        Self(raw)
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

/// A frontend's request to convert an Emacs-owned selection (D15).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ConvertId(NonZeroU64);

impl ConvertId {
    #[must_use]
    pub const fn new(raw: NonZeroU64) -> Self {
        Self(raw)
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

/// A tooltip the evaluator presented on one session.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct TooltipTicket(NonZeroU64);

impl TooltipTicket {
    #[must_use]
    pub const fn new(raw: NonZeroU64) -> Self {
        Self(raw)
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

/// A full-frame shader request, answered by its acknowledgement.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct FrameShaderRequestId(NonZeroU64);

impl FrameShaderRequestId {
    #[must_use]
    pub const fn new(raw: NonZeroU64) -> Self {
        Self(raw)
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

/// Monotonic version of a level fact (monitors, desktop, capabilities).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct FactVersion(pub u64);

/// A selection's ownership generation, bumped by every `Own` (D15).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SelectionGeneration(pub u64);

/// The X selections Emacs names.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum SelectionName {
    Primary,
    Secondary,
    Clipboard,
}

/// A conversion target atom, such as `UTF8_STRING` or `TARGETS`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SelectionTarget(pub String);

/// Whether a chunked byte transfer continues.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ChunkEnd {
    More,
    Last,
}

/// A size in logical pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WindowSize {
    pub width: u32,
    pub height: u32,
}

/// A point in a frame's layout space, bit-exact.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct LayoutPosition {
    pub x: f32,
    pub y: f32,
}

/// A position in the desktop's coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ScreenPosition {
    pub x: i32,
    pub y: i32,
}

/// A boolean setting carried as a state, never as a bare flag.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Toggle {
    Off,
    On,
}
