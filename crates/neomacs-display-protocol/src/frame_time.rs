//! Injected time for the display engine.
//!
//! Temporal code receives time; it never obtains it. Two types express that:
//!
//! - [`EventTime`] — a moment already observed to have happened. This is the
//!   only *stored* point type: every timestamp held in long-lived state is one
//!   of these, and every one of them was minted by an adapter that watched
//!   something occur.
//! - [`FrameSample`] — the moment one frame's visuals are dated to. A
//!   parameter, never a field. Holding one means "I am sampling for a frame
//!   being produced right now"; storing one would mean sampling a later frame
//!   against a stale identity.
//!
//! The render thread has a third domain, `FutureDeadline`, for a moment the
//! loop may block until. It stays private to the scheduler because nothing
//! outside it may arm a wakeup.
//!
//! # Why these live in the protocol crate
//!
//! `neomacs-renderer-wgpu` is a *dependency* of `neomacs-display-runtime`, so
//! it can never name a type private to the runtime. Both crates depend on this
//! one, so a time type shared between the frame loop and the renderer's effects
//! has to live here.
//!
//! # Reading the clock
//!
//! [`observe_platform_now`] is the only sanctioned way to read the wall clock,
//! and it exists so that reads are *greppable*. `Instant::now()` scattered
//! through effect code is invisible; a call to `observe_platform_now` announces
//! that a platform adapter is minting an observation. There is deliberately no
//! `EventTime::now()` and no `EventTime::elapsed()` — `elapsed()` is exactly
//! the hidden clock read this module exists to remove.

use std::time::{Duration, Instant};

/// Read the platform clock and mint an observation.
///
/// Sanctioned callers are platform adapters only: the winit event handler
/// stamping a delivered input event, a renderer constructing its initial
/// sample, and explicitly-marked CPU stopwatches that measure work rather than
/// date visuals. Everything else must be handed an [`EventTime`] or a
/// [`FrameSample`].
#[must_use]
pub fn observe_platform_now() -> EventTime {
    EventTime(Instant::now())
}

/// A moment that has already been observed to occur.
///
/// Deliberately supports no arithmetic operators and no comparison against a
/// bare [`Instant`]: the only ways to relate two points in time are the named
/// methods here and on [`FrameSample`], which makes every cross-domain
/// comparison in the codebase greppable.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct EventTime(Instant);

impl EventTime {
    /// Duration from `earlier` to `self`, saturating at zero.
    ///
    /// Saturating rather than panicking because nothing in the type system
    /// orders two observations: an input event stamped by the winit handler and
    /// a frame stamped by the loop are minted independently, and a caller must
    /// not have to reason about which came first to avoid a panic.
    #[must_use]
    pub fn saturating_since(self, earlier: EventTime) -> Duration {
        self.0.saturating_duration_since(earlier.0)
    }

    /// The moment `after` later than this one.
    ///
    /// Used to build scheduler targets: a blink deadline, a retry backoff, a
    /// max-rate phase anchor.
    #[must_use]
    pub fn plus(self, after: Duration) -> EventTime {
        EventTime(self.0 + after)
    }

    /// As [`EventTime::plus`], but `None` rather than panicking on overflow.
    #[must_use]
    pub fn checked_plus(self, after: Duration) -> Option<EventTime> {
        self.0.checked_add(after).map(EventTime)
    }

    /// Unwrap to a raw [`Instant`].
    ///
    /// ADAPTER BOUNDARY ONLY. This exists for the handful of places that must
    /// hand a point in time to a foreign API — winit's `ControlFlow::WaitUntil`
    /// is the motivating case. Using it to compute an elapsed time re-opens the
    /// hole this module closes; use [`EventTime::saturating_since`] or a
    /// [`FrameSample`] method instead.
    #[must_use]
    pub fn into_instant(self) -> Instant {
        self.0
    }

    /// Wrap a raw [`Instant`] that some other clock already observed.
    ///
    /// ADAPTER BOUNDARY ONLY, for foreign APIs that hand back an `Instant`
    /// (a platform presentation timestamp, a condvar deadline). Never call this
    /// with `Instant::now()` — that is [`observe_platform_now`], which says so.
    #[must_use]
    pub fn from_observed_instant(instant: Instant) -> Self {
        EventTime(instant)
    }
}

/// The moment one frame's visuals are dated to.
///
/// Carries two points on purpose:
///
/// - [`frame_time`](FrameSample::frame_time) is when the loop decided to draw.
///   It is the right basis for ageing observations and for scheduler
///   bookkeeping.
/// - [`presentation_time`](FrameSample::presentation_time) is when the pixels
///   are expected to be on screen. It is the right basis for anything whose
///   *phase* a viewer perceives, because that is when they will perceive it.
///
/// Mixing the two within one frame — ageing a transition against one and a
/// cursor against the other — is precisely the class of drift this type exists
/// to prevent, so both are reachable but each is named.
///
/// Never store a `FrameSample` in a struct field. It identifies one frame; a
/// stored one is a stale frame's identity waiting to be sampled against.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FrameSample {
    frame_time: EventTime,
    interval: Duration,
}

impl FrameSample {
    /// Build a sample for a frame drawn at `frame_time` and expected on screen
    /// `interval` later.
    #[must_use]
    pub const fn new(frame_time: EventTime, interval: Duration) -> Self {
        Self {
            frame_time,
            interval,
        }
    }

    /// When the loop decided to draw this frame.
    #[must_use]
    pub const fn frame_time(self) -> EventTime {
        self.frame_time
    }

    /// When this frame's pixels are expected to be on screen.
    #[must_use]
    pub fn presentation_time(self) -> EventTime {
        self.frame_time.plus(self.interval)
    }

    /// How far ahead of `frame_time` the presentation is predicted to land.
    #[must_use]
    pub const fn interval(self) -> Duration {
        self.interval
    }

    /// Age of an observation as of this frame's draw time.
    #[must_use]
    pub fn since(self, earlier: EventTime) -> Duration {
        self.frame_time.saturating_since(earlier)
    }

    /// Age of an observation as of this frame's expected presentation.
    ///
    /// Use this for anything whose phase the viewer perceives — a transition's
    /// progress, an effect's animation phase — so that the phase is correct at
    /// the moment the pixels appear rather than at the moment they were built.
    #[must_use]
    pub fn since_at_presentation(self, earlier: EventTime) -> Duration {
        self.presentation_time().saturating_since(earlier)
    }
}

#[cfg(test)]
#[path = "frame_time_test.rs"]
mod tests;
