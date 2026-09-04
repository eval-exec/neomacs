//! How far a viewport moved between two presentations.
//!
//! Scroll *direction* is always derivable: `window_start` moving forward or
//! backward says which way the viewport went. The pixel *distance* is a harder
//! question, because it is only a fact when the two presentations show the same
//! text — and the character positions alone cannot establish that.
//!
//! The previous implementation estimated it from a character count:
//!
//! ```text
//! cols       = bounds.width / char_height      // char HEIGHT as a column width
//! est_lines  = char_delta / cols
//! scroll_px  = est_lines * char_height
//! ```
//!
//! That is right only for a monospaced buffer with no wrapping, no images, no
//! variable-height rows, and a square cell. This computes the displacement
//! instead: find a row that appears in both presentations and take the
//! difference of its pixel positions.

// Interface defined by the temporal-presentation plan and exercised by this
// module's tests; the production consumer arrives when the producer-side
// classifier is replaced. Remove this attribute in that change.
#![allow(dead_code)]

use neomacs_display_protocol::frame_glyphs::WindowInfo;
use neomacs_display_protocol::glyph_matrix::{GlyphRow, NO_BUFFER_POSITION_CHARPOS};
use neomacs_display_protocol::transition_policy::TransitionDirection;

/// Which way a viewport moved through its buffer.
#[derive(Clone, Copy, Debug, Eq, PartialEq, strum::IntoStaticStr)]
#[strum(serialize_all = "snake_case")]
pub(in crate::render_thread) enum ScrollDirection {
    TowardBufferStart,
    TowardBufferEnd,
}

impl ScrollDirection {
    pub(in crate::render_thread) const fn transition_direction(self) -> TransitionDirection {
        match self {
            Self::TowardBufferStart => TransitionDirection::Backward,
            Self::TowardBufferEnd => TransitionDirection::Forward,
        }
    }
}

/// Why an exact displacement could not be established.
#[derive(Clone, Copy, Debug, Eq, PartialEq, strum::IntoStaticStr)]
#[strum(serialize_all = "snake_case")]
pub(in crate::render_thread) enum AmbiguityReason {
    /// A different buffer is on screen, so no row can correspond.
    BufferChanged,
    /// The text changed under the viewport. An equal-length edit leaves every
    /// character range plausible while the text beneath differs, so a row match
    /// here would be wrong rather than merely uncertain.
    ModiffChanged,
    /// Rows matched, but not one-to-one, so no single row is *the* anchor.
    NoUniqueRow,
    /// Several rows matched and disagreed about how far the viewport moved,
    /// which means the layout changed as well as scrolled.
    InconsistentDisplacement,
}

/// How far a viewport moved, and how well that is known.
///
/// Only [`ScrollDisplacement::Exact`] carries pixels. A caller that wants to
/// animate an ambiguous scroll has to say so by matching the variant, rather
/// than receiving a number that merely looks authoritative.
#[derive(Clone, Copy, Debug, PartialEq, strum::IntoStaticStr)]
#[strum(serialize_all = "snake_case")]
pub(in crate::render_thread) enum ScrollDisplacement {
    /// Measured from rows present in both presentations.
    Exact {
        pixels: f32,
        direction: ScrollDirection,
        /// How many rows agreed. Diagnostics only; one is enough to be exact.
        anchors: std::num::NonZeroUsize,
    },
    /// The viewport moved, but by how much cannot be established.
    Ambiguous {
        reason: AmbiguityReason,
        direction: ScrollDirection,
    },
    /// The viewport moved far enough that no row survived on screen.
    NoOverlap { direction: ScrollDirection },
}

impl ScrollDisplacement {
    /// The pixel distance, when it is a fact rather than a guess.
    pub(in crate::render_thread) const fn exact_pixels(self) -> Option<f32> {
        match self {
            Self::Exact { pixels, .. } => Some(pixels),
            Self::Ambiguous { .. } | Self::NoOverlap { .. } => None,
        }
    }

    pub(in crate::render_thread) const fn direction(self) -> ScrollDirection {
        match self {
            Self::Exact { direction, .. }
            | Self::Ambiguous { direction, .. }
            | Self::NoOverlap { direction } => direction,
        }
    }
}

/// A row's identity for matching purposes.
///
/// The character range says which text the row shows; the flags distinguish
/// rows that share a range but are not the same row — a continued row and the
/// continuation that follows it, or a left-truncated row and its untruncated
/// twin after an hscroll.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(in crate::render_thread) struct RowKey {
    start_charpos: usize,
    end_charpos: usize,
    continued: bool,
    truncated_left: bool,
    reversed: bool,
}

/// One row's contribution to measuring how far a viewport moved.
///
/// Extracted when a presentation is ingested and retained instead of the rows
/// themselves: the compositor keeps materialized glyph buffers, which do not
/// carry rows, and retaining whole matrices to answer one question would be
/// far more than the question is worth.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::render_thread) struct RowAnchor {
    key: RowKey,
    pixel_y: f32,
}

impl RowAnchor {
    /// The anchor a row offers, or `None` if it may not anchor a measurement.
    pub(in crate::render_thread) fn of(row: &GlyphRow) -> Option<Self> {
        anchor_key(row).map(|key| Self {
            key,
            pixel_y: row.pixel_y,
        })
    }
}

/// A row eligible to anchor a measurement, or `None` if it cannot.
fn anchor_key(row: &GlyphRow) -> Option<RowKey> {
    // Only real buffer text can anchor: a mode line does not move with the
    // viewport, a disabled row shows nothing, and synthetic glyphs map to no
    // buffer position at all.
    if !row.enabled || !row.displays_text || row.mode_line {
        return None;
    }
    if row.start_charpos == NO_BUFFER_POSITION_CHARPOS
        || row.end_charpos == NO_BUFFER_POSITION_CHARPOS
    {
        return None;
    }
    Some(RowKey {
        start_charpos: row.start_charpos,
        end_charpos: row.end_charpos,
        continued: row.continued,
        truncated_left: row.truncated_left,
        reversed: row.reversed_p,
    })
}

/// Rows that occur exactly once, keyed for matching.
///
/// Rows appearing more than once are dropped rather than disambiguated: if a
/// range is not unique within one presentation, matching it across two says
/// nothing.
fn unique_anchors(anchors: &[RowAnchor]) -> std::collections::HashMap<RowKey, f32> {
    use std::collections::HashMap;
    let mut seen: HashMap<RowKey, Option<f32>> = HashMap::new();
    for anchor in anchors {
        seen.entry(anchor.key)
            .and_modify(|slot| *slot = None)
            .or_insert(Some(anchor.pixel_y));
    }
    seen.into_iter()
        .filter_map(|(key, y)| y.map(|y| (key, y)))
        .collect()
}

/// Displacements agreeing within this many pixels are treated as one value.
///
/// Rows are laid out at whole-pixel positions but arrive as `f32`, so exact
/// equality would reject matches that differ only by representation.
const DISPLACEMENT_AGREEMENT_EPSILON: f32 = 0.5;

/// How far `curr`'s viewport moved relative to `prev`.
///
/// Returns [`ScrollDisplacement::Exact`] only when all of these hold:
/// the same buffer is shown, its modification tick is unchanged, at least one
/// row is uniquely matched on both sides, and every matched row agrees on the
/// distance. Any failure yields the direction plus a reason, and the caller
/// decides what to do without a fabricated number.
pub(in crate::render_thread) fn displacement(
    prev: &WindowInfo,
    curr: &WindowInfo,
    prev_anchors: &[RowAnchor],
    curr_anchors: &[RowAnchor],
) -> ScrollDisplacement {
    let direction = if curr.window_start > prev.window_start {
        ScrollDirection::TowardBufferEnd
    } else {
        ScrollDirection::TowardBufferStart
    };

    if prev.buffer_id != curr.buffer_id {
        return ScrollDisplacement::Ambiguous {
            reason: AmbiguityReason::BufferChanged,
            direction,
        };
    }
    if prev.buffer_modiff != curr.buffer_modiff {
        return ScrollDisplacement::Ambiguous {
            reason: AmbiguityReason::ModiffChanged,
            direction,
        };
    }

    let previous = unique_anchors(prev_anchors);
    if previous.is_empty() {
        return ScrollDisplacement::Ambiguous {
            reason: AmbiguityReason::NoUniqueRow,
            direction,
        };
    }
    let current = unique_anchors(curr_anchors);

    let mut measured: Option<f32> = None;
    let mut anchors = 0usize;
    for (key, curr_y) in &current {
        let Some(prev_y) = previous.get(key) else {
            continue;
        };
        let delta = prev_y - curr_y;
        anchors += 1;
        match measured {
            None => measured = Some(delta),
            Some(agreed) if (agreed - delta).abs() <= DISPLACEMENT_AGREEMENT_EPSILON => {}
            Some(_) => {
                // Rows moved by different amounts, so the layout changed as
                // well as scrolling. There is no single displacement.
                return ScrollDisplacement::Ambiguous {
                    reason: AmbiguityReason::InconsistentDisplacement,
                    direction,
                };
            }
        }
    }

    match (measured, std::num::NonZeroUsize::new(anchors)) {
        (Some(pixels), Some(anchors)) => ScrollDisplacement::Exact {
            pixels: pixels.abs(),
            direction,
            anchors,
        },
        _ => ScrollDisplacement::NoOverlap { direction },
    }
}

/// The anchors every window in a presentation offers, keyed by window.
///
/// Taken at ingest, while the presentation still has its glyph matrices. The
/// materialized buffer the compositor retains has no rows, so measuring a
/// scroll later means keeping this much and no more.
pub(in crate::render_thread) fn anchors_by_window(
    state: &neomacs_display_protocol::glyph_matrix::FrameDisplayState,
) -> std::collections::HashMap<neomacs_display_protocol::types::DisplayWindowId, Vec<RowAnchor>> {
    state
        .window_matrices
        .iter()
        .map(|entry| {
            let anchors = entry
                .matrix
                .rows
                .iter()
                .filter_map(|row| RowAnchor::of(row))
                .collect();
            (entry.window_id, anchors)
        })
        .collect()
}

#[cfg(test)]
#[path = "scroll_test.rs"]
mod tests;
