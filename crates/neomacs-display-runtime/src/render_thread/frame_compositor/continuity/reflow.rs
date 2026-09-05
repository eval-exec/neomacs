//! How far an edit pushed the rows beneath it.
//!
//! An edit does not move a viewport; it moves the rows *below itself*. Text
//! inserted above pushes everything after it down by the height of what was
//! added, a deletion pulls it up, and everything above the edit stays exactly
//! where it was. That vertical push is a different fact from a scroll, and the
//! two cannot be measured the same way.
//!
//! The producer currently declares this rather than measuring it. Its
//! `WindowEffectHint::LineAnimation` carries an `edit_y` taken from the cursor
//! (plus one line height) and an `offset` that is a constant `±char_height`
//! whose sign comes from whether `buffer_size` grew. Neither number was
//! observed: the cursor is not where rows moved, an edit that wraps or unwraps
//! a line moves them by more than one line height, and an image or a
//! variable-height row moves them by something else again. This module
//! measures both from the rows themselves.
//!
//! # Content identity, not character positions
//!
//! [`super::scroll`] matches rows by their character range, which it may do
//! precisely because the text is unchanged: the same range is the same text.
//! An edit destroys that. Inserting one character renumbers every position
//! after it, so the row that moved down is the *same row* under a different
//! range — matching by range would find nothing, or worse, match it to whatever
//! row inherited its old numbers. So rows are matched here by
//! [`GlyphRow::hash`], the fingerprint of what the row renders. Two rows with
//! the same fingerprint show the same thing, whatever the edit did to the
//! positions around them.
//!
//! # The modification tick is inverted relative to scroll
//!
//! Scroll displacement is a fact only while the tick is *unchanged*, because
//! that is what makes a character range trustworthy. A reflow is the opposite:
//! it requires the tick to have *changed*. An unchanged tick means no edit
//! happened, and rows that moved without an edit moved because the viewport
//! did — that is a scroll, and this module must not claim it as its own.
//! The buffer identity must match either way; rows of different buffers
//! correspond to nothing.

use neomacs_display_protocol::frame_glyphs::WindowInfo;
use neomacs_display_protocol::glyph_matrix::{GlyphRow, NO_BUFFER_POSITION_CHARPOS};

/// Why a reflow could not be measured.
#[derive(Clone, Copy, Debug, Eq, PartialEq, strum::IntoStaticStr)]
#[strum(serialize_all = "snake_case")]
pub(in crate::render_thread) enum ReflowAmbiguity {
    /// The modification tick did not move, so no edit happened. Whatever the
    /// rows did, an edit did not do it, and attributing their motion to one
    /// would claim a scroll as a reflow.
    SameBufferTick,
    /// A different buffer is on screen, so no row can correspond.
    BufferChanged,
    /// Nothing survived the edit that can be recognized on both sides — a
    /// wholesale rewrite, or a screenful whose rows are all ambiguous.
    NoCommonRows,
    /// Rows moved, and disagreed about how far. An edit shifts everything
    /// below it by one distance; several distances mean something else also
    /// changed, and there is no single reflow to report.
    InconsistentShift,
    /// A row that did not move sits *below* rows that did. An edit cannot do
    /// that: its shift extends to the bottom of the window. Either the match
    /// is wrong or the layout changed for some other reason, and both are
    /// reasons to report nothing.
    StationaryRowBelowShift,
}

/// How far an edit pushed the rows beneath it, and how well that is known.
///
/// Only [`RowShift::Shifted`] carries numbers. A caller that wants to animate
/// an unmeasurable reflow has to say so by matching the variant, rather than
/// receiving a plausible-looking value it did not earn — which is exactly what
/// the producer hint this replaces hands out.
#[derive(Clone, Copy, Debug, PartialEq, strum::IntoStaticStr)]
#[strum(serialize_all = "snake_case")]
pub(in crate::render_thread) enum RowShift {
    /// Rows below the edit moved, by a distance every one of them agrees on.
    Shifted {
        /// The common vertical delta, signed: positive means the rows moved
        /// down the screen, as an insertion above them does.
        pixels: f32,
        /// Where the moved run begins, in the *new* presentation.
        ///
        /// This is the honest `edit_y`: the top of the topmost row that
        /// actually moved, not the top of the window and not the cursor. Note
        /// what it is not — for an insertion, the inserted rows themselves sit
        /// above this y, because new content matches nothing on the other side
        /// and so cannot be measured. This is where the *displaced* content
        /// starts, which is the region an animation has to move.
        first_moved_y: f32,
        /// How many rows moved by that delta. Diagnostics, and the weight of
        /// evidence behind the measurement; one row is already enough.
        rows: std::num::NonZeroUsize,
    },
    /// The measurement could not be made, and no number is offered.
    Ambiguous { reason: ReflowAmbiguity },
    /// Rows were recognized on both sides and every one of them held its
    /// position. The edit changed text without displacing anything — typing
    /// inside a line that did not rewrap.
    Unchanged,
}

impl RowShift {
    /// The vertical delta, when it was measured rather than assumed.
    pub(in crate::render_thread) const fn shifted_pixels(self) -> Option<f32> {
        match self {
            Self::Shifted { pixels, .. } => Some(pixels),
            Self::Ambiguous { .. } | Self::Unchanged => None,
        }
    }
}

/// One row's contribution to measuring a reflow: what it rendered, and where.
///
/// Extracted when a presentation is ingested and retained instead of the rows
/// themselves. The compositor keeps materialized glyph buffers, which carry no
/// rows, so by the time two presentations can be compared the rows are gone;
/// retaining whole matrices to answer one question would cost far more than
/// the question is worth.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::render_thread) struct RowImprint {
    content: u64,
    pixel_y: f32,
}

impl RowImprint {
    /// The imprint a row leaves, or `None` if it may not be matched.
    pub(in crate::render_thread) fn of(row: &GlyphRow) -> Option<Self> {
        row_content(row).map(|content| Self {
            content,
            pixel_y: row.pixel_y,
        })
    }
}

/// A row's content fingerprint, or `None` if the row cannot be matched.
fn row_content(row: &GlyphRow) -> Option<u64> {
    // Only real buffer text reflows: a mode line stays put while the text
    // under it moves, a disabled row shows nothing, and synthetic glyphs — the
    // face-extension stretch past end of line — cover no buffer character, so
    // no edit displaces them as content.
    if !row.enabled || !row.displays_text || row.mode_line {
        return None;
    }
    if row.start_charpos == NO_BUFFER_POSITION_CHARPOS
        || row.end_charpos == NO_BUFFER_POSITION_CHARPOS
    {
        return None;
    }
    // Zero is the protocol's "no hash" sentinel: an empty row, or one whose
    // hash was never computed. Either way it identifies nothing, and treating
    // it as content would match every such row to every other.
    (row.hash != 0).then_some(row.hash)
}

/// Rows whose content occurs exactly once, keyed by that content.
///
/// Repeated fingerprints are dropped rather than disambiguated. Identical
/// lines are ordinary in code — a run of closing braces, blank lines, repeated
/// imports — and if a fingerprint is not unique within one presentation then
/// matching it across two says nothing about which row went where.
fn unique_imprints(imprints: &[RowImprint]) -> std::collections::HashMap<u64, f32> {
    use std::collections::HashMap;
    let mut seen: HashMap<u64, Option<f32>> = HashMap::new();
    for imprint in imprints {
        seen.entry(imprint.content)
            .and_modify(|slot| *slot = None)
            .or_insert(Some(imprint.pixel_y));
    }
    seen.into_iter()
        .filter_map(|(content, y)| y.map(|y| (content, y)))
        .collect()
}

/// Distances within this many pixels of each other are treated as one value.
///
/// Used twice: to decide whether a row moved at all, and whether two rows that
/// moved agree. Rows are laid out at whole-pixel positions but travel as
/// `f32`, so exact comparison would split a single shift into two, or read a
/// representation artifact as motion.
const PIXEL_AGREEMENT_EPSILON: f32 = 0.5;

/// How far the edit between `prev` and `curr` pushed the rows beneath it.
///
/// Returns [`RowShift::Shifted`] only when all of these hold: the same buffer
/// is shown, its modification tick moved (so an edit is what happened), at
/// least one row is uniquely recognized on both sides, every row that moved
/// agrees on the distance, and no stationary row sits below the moved run.
///
/// Rows above the edit and rows below it are *both* expected. A row found at
/// the same y in both presentations is evidence that it sits above the edit —
/// not a contradiction — so it is excluded from the delta rather than allowed
/// to average it away toward zero. The measurement is the delta of the moved
/// group; the stationary group only constrains where that group may begin.
pub(in crate::render_thread) fn shift(
    prev: &WindowInfo,
    curr: &WindowInfo,
    prev_imprints: &[RowImprint],
    curr_imprints: &[RowImprint],
) -> RowShift {
    if prev.buffer_id != curr.buffer_id {
        return RowShift::Ambiguous {
            reason: ReflowAmbiguity::BufferChanged,
        };
    }
    if prev.buffer_modiff == curr.buffer_modiff {
        return RowShift::Ambiguous {
            reason: ReflowAmbiguity::SameBufferTick,
        };
    }

    let previous = unique_imprints(prev_imprints);
    let current = unique_imprints(curr_imprints);

    let mut agreed: Option<f32> = None;
    let mut moved_rows = 0usize;
    let mut matched_rows = 0usize;
    let mut first_moved_y = f32::INFINITY;
    let mut last_stationary_y = f32::NEG_INFINITY;

    for (content, curr_y) in &current {
        let Some(prev_y) = previous.get(content) else {
            continue;
        };
        matched_rows += 1;
        let delta = curr_y - prev_y;
        if delta.abs() <= PIXEL_AGREEMENT_EPSILON {
            // Above the edit. Its position constrains where the moved run may
            // start, and contributes nothing to the distance.
            last_stationary_y = last_stationary_y.max(*curr_y);
            continue;
        }
        match agreed {
            None => agreed = Some(delta),
            Some(distance) if (distance - delta).abs() <= PIXEL_AGREEMENT_EPSILON => {}
            Some(_) => {
                return RowShift::Ambiguous {
                    reason: ReflowAmbiguity::InconsistentShift,
                };
            }
        }
        moved_rows += 1;
        first_moved_y = first_moved_y.min(*curr_y);
    }

    if matched_rows == 0 {
        return RowShift::Ambiguous {
            reason: ReflowAmbiguity::NoCommonRows,
        };
    }
    let (Some(pixels), Some(rows)) = (agreed, std::num::NonZeroUsize::new(moved_rows)) else {
        return RowShift::Unchanged;
    };
    if last_stationary_y > first_moved_y {
        return RowShift::Ambiguous {
            reason: ReflowAmbiguity::StationaryRowBelowShift,
        };
    }
    RowShift::Shifted {
        pixels,
        first_moved_y,
        rows,
    }
}

/// The imprints every window in a presentation offers, keyed by window.
///
/// Taken at ingest, while the presentation still has its glyph matrices. The
/// materialized buffer the compositor retains has no rows, so measuring a
/// reflow later means keeping this much and no more.
pub(in crate::render_thread) fn imprints_by_window(
    state: &neomacs_display_protocol::glyph_matrix::FrameDisplayState,
) -> std::collections::HashMap<neomacs_display_protocol::types::DisplayWindowId, Vec<RowImprint>> {
    state
        .window_matrices
        .iter()
        .map(|entry| {
            let imprints = entry
                .matrix
                .rows
                .iter()
                .filter_map(|row| RowImprint::of(row))
                .collect();
            (entry.window_id, imprints)
        })
        .collect()
}

#[cfg(test)]
#[path = "reflow_test.rs"]
mod tests;
