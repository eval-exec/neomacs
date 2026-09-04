//! What caused a presentation, and how far the buffer behind it has moved.
//!
//! The compositor has to answer two questions about a sealed presentation that
//! its geometry alone cannot settle:
//!
//! - *Did the text under this viewport change?* Scroll displacement is only
//!   exact when source and destination show the same text, and comparing
//!   character positions cannot tell an edit from a scroll. [`BufferModiff`]
//!   is the tick that can.
//! - *Was this presentation caused by an interaction that is still going on?*
//!   A window being dragged by its divider commits a new presentation on every
//!   pointer movement, and animating between them would fight the drag.
//!   [`PresentationOrigin`] says which commits belong to that drag.
//!
//! Both are facts recorded at seal time by the producer that knows them. The
//! compositor reads them; it never infers them from pixels.

use std::num::NonZeroU64;

/// A buffer's modification tick, mirroring GNU's `MODIFF`.
///
/// Two presentations showing the same buffer at the same tick are showing the
/// same text. That is what makes anchor-row matching safe: without it, an edit
/// that happens to preserve length leaves every character position plausible
/// while the text under them has changed, and a row match is then *wrong*
/// rather than merely ambiguous. The buffer's size is not a substitute for the
/// same reason.
#[derive(
    Clone,
    Copy,
    Debug,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
    Default,
    serde::Serialize,
    serde::Deserialize,
)]
#[repr(transparent)]
pub struct BufferModiff(u64);

impl BufferModiff {
    #[must_use]
    pub const fn new(tick: u64) -> Self {
        Self(tick)
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Ordering of pointer events as the render thread emitted them.
///
/// Useful for latency diagnostics and for saying which input a presentation
/// had consumed, but deliberately *not* sufficient to decide what caused one:
/// a watermark proves what had been consumed before a redisplay, not what
/// provoked it. A timer-driven redisplay after a drag release carries the
/// release-era watermark and would be misclassified. That is what
/// [`InteractionSessionId`] is for.
#[derive(
    Clone,
    Copy,
    Debug,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
    Default,
    serde::Serialize,
    serde::Deserialize,
)]
#[repr(transparent)]
pub struct InputSerial(u64);

impl InputSerial {
    pub const FIRST: Self = Self(0);

    #[must_use]
    pub const fn new(serial: u64) -> Self {
        Self(serial)
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }

    /// The next serial in emission order.
    #[must_use]
    pub const fn next(self) -> Self {
        Self(self.0.wrapping_add(1))
    }
}

/// Causal identity of one interactive command started by a pointer press.
///
/// A divider drag runs entirely inside a single Lisp command: GNU's
/// `mouse-drag-line` sets `track-mouse`, loops on `read-event` calling
/// `adjust-window-trailing-edge`, and restores it in an unwind before
/// returning. Every redisplay the drag causes therefore happens inside that
/// command's dynamic extent, which gives "caused by this drag" a precise
/// meaning that no timestamp comparison can.
#[derive(
    Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize,
)]
#[repr(transparent)]
pub struct InteractionSessionId(NonZeroU64);

impl InteractionSessionId {
    /// The first session id. Sessions are numbered from one so that a
    /// `NonZeroU64` niche makes `Option<InteractionSessionId>` free, and so
    /// that there is no "session zero" to confuse with no session at all.
    pub const FIRST: Self = Self(NonZeroU64::MIN);

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0.get()
    }

    /// The next session id, saturating rather than wrapping.
    ///
    /// Saturation is safe here in a way wrapping would not be: reaching
    /// `u64::MAX` sessions is unreachable in practice, and reusing id 1 would
    /// silently make an ancient drag's commits look current.
    #[must_use]
    pub const fn next(self) -> Self {
        match NonZeroU64::new(self.0.get().saturating_add(1)) {
            Some(next) => Self(next),
            None => self,
        }
    }
}

/// Why a presentation was produced.
///
/// The compositor suppresses layout motion for presentations belonging to an
/// active interactive resize, because during a drag GNU's Lisp owns the divider
/// position: `adjust-window-trailing-edge` applies minimum sizes,
/// `window-size-fixed`, and charwise rounding, so the compositor cannot predict
/// where the divider will actually land. Animating toward a predicted position
/// would mean either visible snap-back or reimplementing window.el in Rust.
#[derive(
    Clone,
    Copy,
    Debug,
    Eq,
    PartialEq,
    Default,
    serde::Serialize,
    serde::Deserialize,
    strum::IntoStaticStr,
)]
#[strum(serialize_all = "kebab-case")]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum PresentationOrigin {
    /// Ordinary redisplay. Normal transition policy applies.
    #[default]
    Ordinary,
    /// Produced while an interactive resize command was executing.
    ///
    /// Installed instantly, with no motion, until the session ends.
    InteractiveResize {
        session: InteractionSessionId,
        through: InputSerial,
    },
}

impl PresentationOrigin {
    /// Whether this presentation belongs to `session`.
    #[must_use]
    pub const fn belongs_to(self, session: InteractionSessionId) -> bool {
        match self {
            Self::InteractiveResize { session: own, .. } => own.get() == session.get(),
            Self::Ordinary => false,
        }
    }

    /// Whether layout motion must be suppressed for this presentation.
    #[must_use]
    pub const fn suppresses_layout_motion(self) -> bool {
        matches!(self, Self::InteractiveResize { .. })
    }
}

#[cfg(test)]
#[path = "presentation_origin_test.rs"]
mod tests;
