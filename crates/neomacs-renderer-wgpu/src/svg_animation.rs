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

/// Cheap pre-filter for animation elements in normalized XML bytes.
///
/// The common static document must not pay for another XML parse just to
/// learn it has nothing to animate. Inspect the local element name so XML
/// namespace prefixes cannot hide animation. This remains an over-broad
/// optimization; plan compilation decides whether the document animates.
pub(crate) fn may_contain_animation(data: &[u8]) -> bool {
    data.split(|byte| *byte == b'<').skip(1).any(|element| {
        let qualified = element
            .split(|byte| byte.is_ascii_whitespace() || matches!(*byte, b'/' | b'>'))
            .next()
            .unwrap_or_default();
        let local = qualified
            .rsplit(|byte| *byte == b':')
            .next()
            .unwrap_or_default();
        local.starts_with(b"animate") || local.starts_with(b"set")
    })
}

#[cfg(test)]
mod tests;
