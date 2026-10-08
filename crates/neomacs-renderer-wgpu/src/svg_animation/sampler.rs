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
    ImageAnimationPolicy, ImageColorContext, ImageFrameDelay, ImageFrameIndex, ImageRealization,
    ImageRotation, ImageSizeSpec,
};

use super::eval;
use super::patch;
use super::plan;

/// One sampled animation, in the shape the sequence cache publishes.
pub(crate) struct SampledAnimation {
    pub(crate) frames: Vec<SampledFrame>,
    pub(crate) loop_start: Option<ImageFrameIndex>,
}

pub(crate) struct SampledFrame {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) rgba: Vec<u8>,
    pub(crate) delay: ImageFrameDelay,
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
    if !policy.is_enabled() {
        return None;
    }
    let bounded = crate::svg::bounded_svg_data(data)?;
    if !super::may_contain_animation(bounded.as_ref()) {
        return None;
    }
    let animation = plan::compile(bounded.as_ref())?;
    if animation.is_empty() {
        return None;
    }
    let schedule = SampleSchedule::new(
        animation.sample_timeline()?,
        policy.fps().unwrap_or(SampleGrid::DEFAULT_FPS),
    )?;
    let frame_count = schedule.samples.len();
    let mut frames: Vec<SampledFrame> = Vec::with_capacity(frame_count);
    let mut total_bytes = 0_usize;
    for sample in schedule.samples {
        let doc_time = sample.document_time;
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
                .checked_mul(frame_count.checked_sub(frames.len() + 1)?)?,
        )?;
        if projected > MAX_COMPUTED_SEQUENCE_BYTES {
            return None;
        }
        frames.push(SampledFrame {
            width,
            height,
            rgba: decoded.rgba,
            delay: sample.delay,
        });
    }
    (frames.len() > 1).then_some(SampledAnimation {
        frames,
        loop_start: schedule.loop_start,
    })
}

/// Sample times remain separate from pixels, so a bounded prefix and repeating
/// tail can retain their own exact delays without coupling decoder geometry.
struct TimedSample {
    document_time: std::time::Duration,
    delay: ImageFrameDelay,
}

struct SampleSchedule {
    samples: Vec<TimedSample>,
    loop_start: Option<ImageFrameIndex>,
}

impl SampleSchedule {
    fn new(timeline: plan::SampleTimeline, fps: u32) -> Option<Self> {
        use std::num::NonZeroU32;
        use std::time::Duration;
        let mut schedule = Self {
            samples: Vec::new(),
            loop_start: None,
        };
        match timeline {
            plan::SampleTimeline::Finite { end } => {
                u64::try_from(end.as_nanos()).ok()?;
                let grid = SampleGrid::for_finite_span(end, fps)?;
                schedule.append_grid(grid, Duration::ZERO)?;
                schedule.samples.push(TimedSample {
                    document_time: end,
                    delay: grid.slot_delay()?,
                });
            }
            plan::SampleTimeline::Repeating { origin, period } => {
                u64::try_from(origin.as_nanos()).ok()?;
                u64::try_from(period.as_nanos()).ok()?;
                let loop_grid = SampleGrid::new(period, fps)?;
                if origin.is_zero() {
                    schedule.append_grid(loop_grid, origin)?;
                } else {
                    let prefix_grid = SampleGrid::new(origin, fps)?;
                    let combined = prefix_grid.slot_count() + loop_grid.slot_count();
                    let (prefix_grid, loop_grid) = if combined > SampleGrid::MAX_SLOTS {
                        // Every segment receives at least one sample. Allocate
                        // the cap proportionally, preserving each span's exact
                        // duration rather than replaying the introduction.
                        let prefix_slots = (SampleGrid::MAX_SLOTS * prefix_grid.slot_count()
                            / combined)
                            .clamp(1, SampleGrid::MAX_SLOTS - 1);
                        (
                            SampleGrid::with_max_slots(
                                origin,
                                fps,
                                NonZeroU32::new(prefix_slots)?,
                            )?,
                            SampleGrid::with_max_slots(
                                period,
                                fps,
                                NonZeroU32::new(SampleGrid::MAX_SLOTS - prefix_slots)?,
                            )?,
                        )
                    } else {
                        (prefix_grid, loop_grid)
                    };
                    schedule.append_grid(prefix_grid, Duration::ZERO)?;
                    schedule.loop_start = Some(ImageFrameIndex::new(schedule.samples.len() as u64));
                    schedule.append_grid(loop_grid, origin)?;
                }
            }
        }
        Some(schedule)
    }

    fn append_grid(&mut self, grid: SampleGrid, origin: std::time::Duration) -> Option<()> {
        let delay = grid.slot_delay()?;
        for slot in 0..grid.slot_count() {
            self.samples.push(TimedSample {
                document_time: origin.checked_add(grid.slot_start(slot)?)?,
                delay,
            });
        }
        Some(())
    }
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
        .map(|frame| (frame.width, frame.height, frame.rgba, frame.delay))
        .collect();
    crate::image_sequence::DecodedImageSequence::from_timed_frames(frames, animation.loop_start)
}
