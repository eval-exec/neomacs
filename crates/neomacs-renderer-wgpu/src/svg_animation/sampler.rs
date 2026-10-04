//! Grid sampling: a compiled plan becomes decoded frames.
//!
//! This is the bridge between the SVG world and the sequence world. It
//! quantizes the plan's loop with a
//! [`neomacs_display_protocol::animated_visual::SampleGrid`], evaluates and
//! patches the document once per slot, rasterizes each slot through the
//! shared vector backend, and returns one decoded sequence shaped exactly
//! like a decoded GIF: frames in slot order, each carrying the grid's exact
//! delay. The image sequence cache owns residency, budgeting, and
//! retirement from there — computed animation gets the same memory policy
//! as authored animation, which is the point.

use std::sync::Arc;

use neomacs_display_protocol::animated_visual::SampleGrid;
use neomacs_display_protocol::{
    ImageAnimationPolicy, ImageColorContext, ImageFrameDelay, ImageRealization, ImageRotation,
    ImageSizeSpec,
};

use super::eval;
use super::patch;
use super::plan;

/// One sampled animation, in the shape the sequence cache publishes.
pub(crate) struct SampledAnimation {
    pub(crate) frames: Vec<SampledFrame>,
    pub(crate) delay: ImageFrameDelay,
}

pub(crate) struct SampledFrame {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) rgba: Vec<u8>,
}

/// Sample `data`'s animation on a grid under `policy`.
///
/// `None` means "not an animated document for this policy": no plan, no
/// loop period, or a grid the policy's rate cannot quantize. The caller
/// falls back to the static single-frame decode — the fallback ladder ends
/// at GNU's behavior, never at a failed load.
///
/// Frames decode at the document's intrinsic extent — not the request's
/// size, rotation, or realization — exactly like authored raster frames:
/// the bitmap realization downstream applies those once. Baking them here
/// would have them applied twice (and make the cache entry specific to a
/// realization the sequence key does not carry). Face colors *are* baked
/// (`currentColor`, the background rect), which is why the cache entry
/// records them and a color change replaces the entry.
pub(crate) fn sample(
    data: &[u8],
    colors: ImageColorContext,
    resources: &crate::svg::SvgResourceContext,
    policy: ImageAnimationPolicy,
) -> Option<SampledAnimation> {
    if !policy.is_enabled() || !super::may_contain_animation(data) {
        return None;
    }
    let bounded = crate::svg::bounded_svg_data(data)?;
    let animation = plan::compile(bounded.as_ref())?;
    if animation.is_empty() {
        return None;
    }
    let period = animation.loop_period()?;
    let grid = SampleGrid::new(period, policy.fps().unwrap_or(SampleGrid::DEFAULT_FPS))?;
    let delay = grid.slot_delay()?;

    let mut frames: Vec<SampledFrame> = Vec::with_capacity(grid.slot_count() as usize);
    let mut total_bytes = 0_usize;
    for slot in 0..grid.slot_count() {
        let doc_time = grid.slot_start(slot)?;
        // Slot zero samples document time zero — the SMIL start state, not
        // the static base state. GNU renders the base; the difference is
        // the documented divergence the policy opts into.
        let overrides = eval::evaluate(&animation, doc_time);
        let patched = patch::apply(&animation, bounded.as_ref(), &overrides)?;
        let decoded = crate::svg::decode(
            &patched,
            ImageSizeSpec::default(),
            ImageRotation::None,
            ImageRealization::default(),
            colors,
            resources.clone(),
        )?;
        let (width, height) = decoded.geometry.raster().dimensions();
        if let Some(first) = frames.first()
            && (first.width, first.height) != (width, height)
        {
            // A document whose animated state changes its raster extent
            // cannot be a frame sequence; it stays a static image.
            return None;
        }
        // Admission during sampling, not after: the sequence budget must
        // never be learned by first materializing the bytes it rejects.
        // One slot's extent bounds every other slot's (checked above), so
        // the projection after slot zero is exact.
        total_bytes = total_bytes.checked_add(decoded.rgba.len())?;
        let projected = total_bytes.checked_add(
            decoded
                .rgba
                .len()
                .checked_mul((grid.slot_count() as usize).checked_sub(frames.len() + 1)?)?,
        )?;
        if projected > MAX_COMPUTED_SEQUENCE_BYTES {
            return None;
        }
        frames.push(SampledFrame {
            width,
            height,
            rgba: decoded.rgba,
        });
    }
    (frames.len() > 1).then_some(SampledAnimation { frames, delay })
}

/// Aggregate ceiling for one computed sequence, matching the sequence
/// cache's residency budget: a document whose loop would exceed it at the
/// requested sampling density stays a static image rather than being
/// materialized and rejected.
const MAX_COMPUTED_SEQUENCE_BYTES: usize = 64 * 1024 * 1024;

/// Sample and box frames as shared buffers, in the shape the sequence cache
/// publishes.
pub(crate) fn sample_shared(
    data: &[u8],
    colors: ImageColorContext,
    resources: &crate::svg::SvgResourceContext,
    policy: ImageAnimationPolicy,
) -> Option<Arc<crate::image_sequence::DecodedImageSequence>> {
    let animation = sample(data, colors, resources, policy)?;
    let frames = animation
        .frames
        .into_iter()
        .map(|frame| (frame.width, frame.height, frame.rgba))
        .collect();
    Some(crate::image_sequence::DecodedImageSequence::from_frames(
        frames,
        animation.delay,
    ))
}
