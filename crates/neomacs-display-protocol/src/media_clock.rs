//! Document-time mapping for animated media sources.
//!
//! A multi-frame source lives on its own timeline: GIF frame delays, an SVG
//! document clock, a video presentation timestamp. The compositor lives on
//! presentation time. [`MediaClock`] is the only place those two domains
//! meet, so every animated source answers scheduling questions in document
//! time and every scheduling decision is made in presentation time.
//!
//! The mapping is deliberately minimal: one epoch (first presentation), one
//! pause domain (nothing is presenting the source), unit rate. Resampling
//! and per-window offsets are compositor policies that compose on top of a
//! clock rather than being baked into it, because two windows presenting the
//! same source must share one document timeline or their samples would
//! disagree.

use crate::frame_time::EventTime;
use std::time::Duration;

/// Maps presentation time onto one media document's own timeline.
///
/// Document time starts at zero when the clock is started, accumulates while
/// running, and freezes across pauses. Pausing while already paused and
/// resuming while already running are no-ops, so a presenter that polls
/// visibility cannot corrupt the timeline.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MediaClock {
    /// Document time accumulated before the current run segment.
    elapsed: Duration,
    /// Presentation time the current run segment started from; `None` while
    /// paused. `None` at construction means "started paused" so that a clock
    /// created ahead of its first presentation reads as zero until then.
    running_since: Option<EventTime>,
}

impl MediaClock {
    /// A clock that begins paused at document time zero.
    ///
    /// Construction cannot know when presentation begins — the first
    /// [`Self::resume`] supplies the epoch — so a fresh clock reports zero
    /// for any presentation time rather than a negative document time.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            elapsed: Duration::ZERO,
            running_since: None,
        }
    }

    /// Whether the clock is currently accumulating.
    #[must_use]
    pub const fn is_paused(&self) -> bool {
        self.running_since.is_none()
    }

    /// Freeze accumulation at `presentation`.
    ///
    /// Idempotent: pausing an already-paused clock keeps the earlier freeze
    /// point, so repeated visibility polls cannot leak time.
    pub fn pause(&mut self, presentation: EventTime) {
        if let Some(started) = self.running_since {
            self.elapsed = self.accumulated(presentation, started);
            self.running_since = None;
        }
    }

    /// Continue (or begin) accumulating from `presentation`.
    ///
    /// The first resume on a fresh clock fixes the epoch: document time zero
    /// is that presentation instant. Idempotent while running.
    pub fn resume(&mut self, presentation: EventTime) {
        if self.running_since.is_none() {
            self.running_since = Some(presentation);
        }
    }

    /// Document time reached at `presentation`.
    ///
    /// A paused clock reports its frozen value; a clock that has never run
    /// reports zero. Asking about a presentation time *before* the epoch
    /// saturates at zero — the document has no negative time.
    #[must_use]
    pub fn document_time(&self, presentation: EventTime) -> Duration {
        match self.running_since {
            Some(started) => self.accumulated(presentation, started),
            None => self.elapsed,
        }
    }

    fn accumulated(&self, presentation: EventTime, started: EventTime) -> Duration {
        self.elapsed + presentation.saturating_since(started)
    }
}

impl Default for MediaClock {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "media_clock/tests/mod.rs"]
mod tests;
