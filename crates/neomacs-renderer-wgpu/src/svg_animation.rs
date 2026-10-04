//! Computed animation for SVG documents: an SMIL subset, sampled on a grid.
//!
//! GNU renders SVG through librsvg, which has no document clock, so an
//! animated SVG displays as one static frame there. This module is the
//! opt-in divergence: it compiles the animation elements a document carries
//! into a pure timeline plan, evaluates that plan at quantized document
//! times, patches the computed values back into the source text, and
//! rasterizes each slot through the same vector backend as every other SVG
//! (`crate::svg`).
//!
//! The split mirrors the display engine's separation of concerns:
//!
//! - [`plan`] turns XML into data (once, at load);
//! - [`eval`] turns `(plan, document time)` into attribute values (pure);
//! - [`patch`] turns values into renderable source text (byte splicing);
//! - [`sampler`] turns slots into decoded frames on a
//!   [`neomacs_display_protocol::animated_visual::SampleGrid`] and hands
//!   them to the image sequence cache.
//!
//! Nothing here knows about clocks, scheduling, or the evaluator thread;
//! the frame scheduler asks its questions through
//! [`neomacs_display_protocol::animated_visual::AnimatedVisual`].

mod eval;
mod patch;
mod plan;
mod sampler;

pub(crate) use sampler::sample_shared as sample_svg_sequence;

/// Cheap pre-filter for animation elements, operating on raw bytes.
///
/// Layout measures every SVG it places, so the common static document must
/// not pay for a second document parse just to learn it has nothing to
/// animate. The scan is deliberately over-broad (`<set` also prefixes
/// nothing in the SVG vocabulary that matters); the plan compile remains
/// the authority on what animates.
pub(crate) fn may_contain_animation(data: &[u8]) -> bool {
    // `animate`, `animateTransform`, `animateMotion`, `animateColor` all
    // share the `<animate` byte prefix.
    data.windows(b"<animate".len())
        .any(|window| window == b"<animate")
        || data.windows(b"<set".len()).any(|window| window == b"<set")
}

#[cfg(test)]
mod tests;
