//! Truncation / continuation fringe arrows — GNU's `draw_window_fringes`
//! (`src/fringe.c` ~1220-1308).
//!
//! When a buffer-text row is truncated or continued, GNU draws a small arrow
//! bitmap in the corresponding fringe:
//!
//! | row state                       | fringe | logical indicator | bitmap            |
//! |---------------------------------|--------|-------------------|-------------------|
//! | truncated on the LEFT (hscroll) | left   | `truncation` L    | `left-arrow`      |
//! | truncated on the RIGHT          | right  | `truncation` R    | `right-arrow`     |
//! | continued (wraps to next line)  | right  | `continuation` R  | `right-curly-arrow` |
//! | continuation (of previous line) | left   | `continuation` L  | `left-curly-arrow`  |
//!
//! Concrete bitmaps are resolved through `fringe-indicator-alist`
//! (`resolve_fringe_indicator_bitmap_index`, the Stage-6 resolver) so a buffer
//! can rebind them, exactly like GNU's `get_logical_fringe_bitmap`.
//!
//! neomacs records row state two ways: the hscroll left-truncation lives on
//! `GlyphRow::truncated_left` (set by the hscroll marker), while right-edge
//! truncation / continuation / continuation-line live in [`DisplayRowFlags`]
//! (`Truncated` / `Continued` / `Continuation`). This installer reads both and
//! sets `GlyphRow::left_fringe_bitmap` / `right_fringe_bitmap` — but only when
//! that slot is still empty, so an explicit `(left-fringe …)` display spec and
//! the empty-line filler (which run first / on separate rows) keep precedence,
//! matching GNU's `row->left_user_fringe_bitmap` short-circuit.

use crate::display_row::geometry::{DisplayRowFlagKind, DisplayRowFlags};
use crate::neovm_bridge::{LayoutBufferView, resolve_fringe_indicator_bitmap_index};
use crate::output::builder::DisplayOutputBuilder;
use crate::output::row_request::DisplayWindowRowMutation;
use crate::types::WindowParams;
use neomacs_display_protocol::glyph_matrix::{FringeBitmapInfo, GlyphRow};
use neomacs_display_protocol::types::FaceId;
use neovm_core::emacs_core::intern::intern;
use neovm_core::emacs_core::{Context, Value};

/// The four logical arrow bitmaps resolved once per window for the body rows.
/// `None` means the indicator resolves to no bitmap (GNU `NO_FRINGE_BITMAP`).
#[derive(Clone, Copy, Debug, Default)]
struct FringeArrowBitmaps {
    /// `truncation` left element — `left-arrow`.
    truncation_left: Option<u16>,
    /// `truncation` right element — `right-arrow`.
    truncation_right: Option<u16>,
    /// `continuation` left element — `left-curly-arrow`.
    continuation_left: Option<u16>,
    /// `continuation` right element — `right-curly-arrow`.
    continuation_right: Option<u16>,
}

impl FringeArrowBitmaps {
    fn resolve<B: LayoutBufferView>(buffer: &B, ctx: &Context) -> Self {
        let truncation = Value::from_sym_id(intern("truncation"));
        let continuation = Value::from_sym_id(intern("continuation"));
        let resolve = |sym: Value, right_p: bool| {
            resolve_fringe_indicator_bitmap_index(
                buffer, ctx, sym, right_p, /* partial */ false,
            )
        };
        Self {
            truncation_left: resolve(truncation, false),
            truncation_right: resolve(truncation, true),
            continuation_left: resolve(continuation, false),
            continuation_right: resolve(continuation, true),
        }
    }

    fn any(&self) -> bool {
        self.truncation_left.is_some()
            || self.truncation_right.is_some()
            || self.continuation_left.is_some()
            || self.continuation_right.is_some()
    }
}

/// Per-row decoration request for the truncation/continuation fringe arrows.
/// Built for every installed body row from [`DisplayRowFlags`] and applied to
/// the live `GlyphRow` (which also carries `truncated_left` / `reversed_p`).
pub(crate) struct TruncationContinuationFringeRequest {
    /// First text-area display-row index (`display_text_row_base`).
    display_text_row_base: usize,
    /// Whether the left fringe has any width (GNU `WINDOW_LEFT_FRINGE_WIDTH`).
    has_left_fringe: bool,
    /// Whether the right fringe has any width.
    has_right_fringe: bool,
    /// Resolved bitmap indices for the four logical arrow cases.
    bitmaps: FringeArrowBitmaps,
    /// Face id for the fringe bitmap quads (the `fringe` face).
    face_id: FaceId,
}

impl TruncationContinuationFringeRequest {
    /// Resolve everything needed up front. Returns `None` when neither fringe
    /// has width or no arrow bitmaps resolve (nothing to draw), so the caller
    /// can skip the row walk entirely.
    pub(crate) fn new<B: LayoutBufferView>(
        buffer: &B,
        ctx: &Context,
        params: &WindowParams,
        display_text_row_base: usize,
        face_id: FaceId,
    ) -> Option<Self> {
        let has_left_fringe = params.left_fringe_width > 0.0;
        let has_right_fringe = params.right_fringe_width > 0.0;
        if !has_left_fringe && !has_right_fringe {
            return None;
        }
        let bitmaps = FringeArrowBitmaps::resolve(buffer, ctx);
        if !bitmaps.any() {
            return None;
        }
        Some(Self {
            display_text_row_base,
            has_left_fringe,
            has_right_fringe,
            bitmaps,
            face_id,
        })
    }

    /// Walk every text-area row and install the arrow bitmaps that its state
    /// calls for. Body rows live at `display_text_row_base + row_idx`, parallel
    /// to `row_flags` (and to the empty-line filler's row range, which is
    /// disjoint — those rows have no truncation/continuation flags).
    pub(crate) fn install(
        self,
        output_builder: &mut DisplayOutputBuilder,
        row_flags: &DisplayRowFlags,
    ) {
        for row_idx in 0..row_flags.len() {
            let mutation = FringeArrowRowMutation {
                continued: row_flags.is_set(row_idx, DisplayRowFlagKind::Continued),
                continuation: row_flags.is_set(row_idx, DisplayRowFlagKind::Continuation),
                truncated_right: row_flags.is_set(row_idx, DisplayRowFlagKind::Truncated),
                has_left_fringe: self.has_left_fringe,
                has_right_fringe: self.has_right_fringe,
                bitmaps: self.bitmaps,
                face_id: self.face_id,
            };
            // `truncated_left` lives on the GlyphRow (not in `row_flags`), so the
            // mutation must run on every row to read it; the flag-only fields are
            // passed through.
            let _ = output_builder
                .apply_current_window_row_mutation(self.display_text_row_base + row_idx, mutation);
        }
    }
}

/// The per-row state that decides which arrow bitmaps apply, gathered from
/// both [`DisplayRowFlags`] and the `GlyphRow` itself.
#[derive(Clone, Copy, Debug, Default)]
struct FringeArrowRowState {
    /// `DisplayRowFlags::Continued` — this row continues onto the next visual
    /// line (right curly arrow on the right fringe).
    continued: bool,
    /// `DisplayRowFlags::Continuation` — this row is a continuation of the
    /// previous line (left curly arrow on the left fringe).
    continuation: bool,
    /// `DisplayRowFlags::Truncated` — line truncated on the right edge
    /// (right-arrow on the right fringe).
    truncated_right: bool,
    /// `GlyphRow::truncated_left` — hscroll-truncated on the left edge
    /// (left-arrow on the left fringe).
    truncated_left: bool,
    /// `GlyphRow::reversed_p` — R2L paragraph; mirrors the left/right cases.
    reversed: bool,
}

/// Pure GNU-`draw_window_fringes` selection: pick the (left, right) fringe
/// bitmap indices for a row's state. Either may be `None` (no bitmap). This is
/// side-effect-free so it can be unit-tested without a full output grid.
fn select_fringe_bitmaps(
    state: FringeArrowRowState,
    has_left_fringe: bool,
    has_right_fringe: bool,
    bitmaps: &FringeArrowBitmaps,
) -> (Option<u16>, Option<u16>) {
    let reversed = state.reversed;

    // LEFT fringe: truncation takes precedence over continuation (GNU order).
    let left = if has_left_fringe {
        let left_truncated =
            (!reversed && state.truncated_left) || (reversed && state.truncated_right);
        let left_continuation = (!reversed && state.continuation) || (reversed && state.continued);
        if left_truncated {
            bitmaps.truncation_left
        } else if left_continuation {
            bitmaps.continuation_left
        } else {
            None
        }
    } else {
        None
    };

    // RIGHT fringe: truncation then continuation.
    let right = if has_right_fringe {
        let right_truncated =
            (!reversed && state.truncated_right) || (reversed && state.truncated_left);
        let right_continued = (!reversed && state.continued) || (reversed && state.continuation);
        if right_truncated {
            bitmaps.truncation_right
        } else if right_continued {
            bitmaps.continuation_right
        } else {
            None
        }
    } else {
        None
    };

    (left, right)
}

struct FringeArrowRowMutation {
    continued: bool,
    continuation: bool,
    truncated_right: bool,
    has_left_fringe: bool,
    has_right_fringe: bool,
    bitmaps: FringeArrowBitmaps,
    face_id: FaceId,
}

impl DisplayWindowRowMutation for FringeArrowRowMutation {
    type Output = ();

    fn apply(self, row: &mut GlyphRow, _matrix_cols: usize) -> Self::Output {
        if !row.enabled {
            return;
        }
        let state = FringeArrowRowState {
            continued: self.continued,
            continuation: self.continuation,
            truncated_right: self.truncated_right,
            truncated_left: row.truncated_left,
            reversed: row.reversed_p,
        };
        let (left, right) = select_fringe_bitmaps(
            state,
            self.has_left_fringe,
            self.has_right_fringe,
            &self.bitmaps,
        );
        // Precedence: don't clobber an explicit `(left-fringe …)` spec or the
        // empty-line filler already occupying the slot (GNU's
        // `row->left_user_fringe_bitmap` short-circuit).
        if row.left_fringe_bitmap.is_none()
            && let Some(bitmap_index) = left
        {
            row.left_fringe_bitmap = Some(FringeBitmapInfo {
                bitmap_index,
                face_id: self.face_id,
            });
        }
        if row.right_fringe_bitmap.is_none()
            && let Some(bitmap_index) = right
        {
            row.right_fringe_bitmap = Some(FringeBitmapInfo {
                bitmap_index,
                face_id: self.face_id,
            });
        }
    }
}

#[cfg(test)]
#[path = "fringe_arrows_test.rs"]
mod tests;
