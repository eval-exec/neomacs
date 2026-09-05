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

/// Causal identity of one drag that moves a window edge.
///
/// The extent is the pointer button, not a command and not a time window: a
/// session opens when a press lands on a region whose drag moves a window edge
/// — a mode, header or tab line, or a divider — and closes when that button is
/// released. Nothing else can end it, so "caused by this drag" needs no
/// timestamp comparison and no timeout.
///
/// It is deliberately *not* the dynamic extent of a Lisp command. `mouse.el`'s
/// `mouse-drag-line` sets `track-mouse` and installs a `set-transient-map`,
/// then returns: the drag runs one command per pointer movement, and no
/// command's extent contains it. Nor is it the value of `track-mouse`, which
/// is a `setq` convention of three particular `mouse-drag-*` commands rather
/// than a fact about the pointer.
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
    /// Composed while the pointer was holding a window edge.
    ///
    /// Installed instantly, with no motion, for as long as the drag runs.
    /// Whichever command the keymap routed the press to is irrelevant: the
    /// hand is placing this geometry, so the geometry is already where it
    /// belongs.
    InteractiveResize { session: InteractionSessionId },
}

impl PresentationOrigin {
    /// Whether this presentation belongs to `session`.
    #[must_use]
    pub const fn belongs_to(self, session: InteractionSessionId) -> bool {
        match self {
            Self::InteractiveResize { session: own } => own.get() == session.get(),
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
