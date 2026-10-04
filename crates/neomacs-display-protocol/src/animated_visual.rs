//! The contract animated media declares to the frame scheduler.
//!
//! The scheduler asks exactly three questions of anything that moves:
//! when does it next change on its own, does it interpolate between those
//! moments, and does it loop. A pre-authored frame list (GIF, APNG) answers
//! from its delay table; a computed timeline (animated SVG) answers from its
//! compiled plan; a native video decoder answers from its presentation
//! timestamps. The answers drive cadence demands, so a source that can say
//! "discrete steps every 100ms" never forces the compositor to 60Hz
//! sampling, and a source that interpolates smoothly never steps.
//!
//! This module owns no clocks and no pixels: document time goes in, scheduling
//! facts come out. [`crate::media_clock`] is the type that produced the
//! document time in the first place.

use crate::image::ImageFrameDelay;
use std::time::Duration;

/// Scheduling facts about one animated source, in document time.
pub trait AnimatedVisual {
    /// The next document time at which the visual changes discontinuously —
    /// a discrete keyframe, a step between values, or the start of a segment
    /// that interpolates.
    ///
    /// `None` means "no further change" (a finished finite animation).
    /// Continuous sources still report segment boundaries so the scheduler
    /// can pick between exact-deadline and max-rate pacing.
    fn next_event(&self, doc_time: Duration) -> Option<Duration>;

    /// Whether values interpolate between events, demanding samples at the
    /// display rate rather than only at event deadlines.
    fn is_continuous(&self) -> bool;

    /// The fixed loop period, if the visual repeats exactly.
    ///
    /// A period lets the scheduler phase-anchor the cadence so an editor
    /// commit between loops cannot re-anchor an ambient animation, and lets
    /// a sample grid reuse identical rasterized frames on every cycle.
    fn period(&self) -> Option<Duration>;
}

/// Nanosecond magnitude of a duration, widened for the products below.
fn nanos(duration: Duration) -> u128 {
    // `as_nanos` is exact — a Duration is internally u64-bounded — so the
    // u128 widening exists for the multiply-then-divide products below,
    // not because time can exceed it.
    duration.as_nanos()
}

/// One sampling decision: the ceiling on distinct samples per loop.
///
/// A grid is the memory bound for computed animation as much as a throttle:
/// a source sampled on a stable grid produces at most `slot_count` distinct
/// rasterizations, which a cache keyed by slot can then serve forever. For
/// that reason the grid — not the decoder — decides what "a frame" is, and
/// the slot count is hard-capped so a pathological document cannot turn a
/// long period and a high rate into an unbounded frame list.
///
/// The delay a grid reports is exact per construction — `period / slots` —
/// and stays a rational number of milliseconds, because
/// `image-multi-frame-p` reads it and GNU's timer arithmetic consumes it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SampleGrid {
    period: Duration,
    slot_count: u32,
}

impl SampleGrid {
    /// Upper bound on distinct slots, shared by every computed-animation
    /// source. `period × fps` above this collapses to this many samples.
    pub const MAX_SLOTS: u32 = 256;

    /// Default sampling ceiling when the source specifies no rate.
    pub const DEFAULT_FPS: u32 = 30;

    /// A grid over `period` sampling at most `fps` times per second.
    ///
    /// `fps` is a ceiling, not a target: the slot count is
    /// `min(period × fps, MAX_SLOTS)` so the grid degrades density, never
    /// correctness, when the product would explode.
    ///
    /// Returns `None` for a zero period or zero fps — there is nothing to
    /// sample then, and callers treat that as "not animated here".
    #[must_use]
    pub fn new(period: Duration, fps: u32) -> Option<Self> {
        if period.is_zero() || fps == 0 {
            return None;
        }
        let wanted = nanos(period) * u128::from(fps);
        let slots = (wanted / 1_000_000_000).clamp(1, u128::from(Self::MAX_SLOTS));
        Some(Self {
            period,
            slot_count: u32::try_from(slots).expect("clamped to MAX_SLOTS"),
        })
    }

    /// The loop period the grid samples over.
    #[must_use]
    pub const fn period(&self) -> Duration {
        self.period
    }

    /// How many distinct samples one loop produces.
    #[must_use]
    pub const fn slot_count(&self) -> u32 {
        self.slot_count
    }

    /// Fold an arbitrary document time into `[0, period)`.
    ///
    /// Looping sources are sampled modulo their period; the wrap is part of
    /// the grid so every consumer folds time the same way.
    #[must_use]
    pub fn wrap(&self, doc_time: Duration) -> Duration {
        Duration::from_nanos(u64::try_from(nanos(doc_time) % nanos(self.period)).unwrap_or(0))
    }

    /// The slot a wrapped document time falls in.
    ///
    /// The last slot includes its endpoint: a wrapped time exactly at the
    /// period belongs to the final sample rather than wrapping to slot zero,
    /// which would alias the loop's end onto its start.
    #[must_use]
    pub fn slot_for(&self, wrapped: Duration) -> u32 {
        if wrapped >= self.period {
            return self.slot_count.saturating_sub(1);
        }
        let slot = nanos(wrapped) * u128::from(self.slot_count) / nanos(self.period);
        slot.clamp(0, u128::from(self.slot_count.saturating_sub(1))) as u32
    }

    /// The document time slot `slot` is sampled at.
    #[must_use]
    pub fn slot_start(&self, slot: u32) -> Option<Duration> {
        if slot >= self.slot_count {
            return None;
        }
        let start = nanos(self.period) * u128::from(slot) / u128::from(self.slot_count);
        Some(Duration::from_nanos(u64::try_from(start).unwrap_or(0)))
    }

    /// The exact rational delay each slot's sample is displayed for.
    ///
    /// `period / slots` is computed as one reduced millisecond fraction, so
    /// a 2s/60-slot grid reports 100/3 ms rather than a third of a
    /// millisecond short of it. Sources whose reduced fraction cannot fit
    /// the u32 numerator domain fall back to whole milliseconds — a
    /// multi-century period, at which point the frame delay is academic.
    #[must_use]
    pub fn slot_delay(&self) -> Option<ImageFrameDelay> {
        let numerator = nanos(self.period);
        if numerator == 0 {
            return None;
        }
        // nanoseconds / (slots × 1e6) is the delay in milliseconds.
        let denominator = u128::from(self.slot_count) * 1_000_000;
        let common = gcd(numerator, denominator);
        let (numerator, denominator) = (numerator / common, denominator / common);
        match (u32::try_from(numerator), u32::try_from(denominator)) {
            (Ok(numerator), Ok(denominator)) => {
                ImageFrameDelay::milliseconds(numerator, denominator)
            }
            _ => {
                // The reduced fraction is already in milliseconds; only its
                // magnitude outgrew u32. Keep the whole milliseconds.
                let whole_ms = numerator / denominator;
                let whole_ms = u32::try_from(whole_ms).ok()?;
                ImageFrameDelay::milliseconds(whole_ms, 1)
            }
        }
    }
}

/// Euclid's algorithm; the inputs are period nanoseconds and slot-count
/// millionths, both nonzero.
fn gcd(mut a: u128, mut b: u128) -> u128 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

#[cfg(test)]
#[path = "animated_visual/tests/mod.rs"]
mod tests;
