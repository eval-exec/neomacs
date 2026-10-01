//! Whole text-run rendering for buffer source items.

use crate::buffer_source::item_append::BufferSourceRowAppendContext;
use crate::buffer_source::item_render::BufferSourceItemRenderOutcome;
use crate::display_cursor::{
    CapturedCursorInfo, CapturedCursorPlacement, CapturedCursorSlotWidth, CursorCaptureState,
    capture_cursor_info,
};
use crate::display_item::DisplaySourcePosition;
use crate::display_row::append_context::DisplayRowAppendKind;
use crate::display_row::builder::{DisplayRowAppendProgress, DisplayRowPosition};
use crate::display_row::face_state::DisplayRowActiveFaceState;
use crate::display_row::face_state::DisplayRowExtendFace;
use crate::display_row::geometry::DisplayRowGeometryState;
use crate::display_row::source_render::TextRowSourceRenderState;
use crate::display_row::walk_state::{TrailingWhitespaceRenderState, WordWrapRenderState};
use crate::display_source::{DisplaySourceStepItem, DisplaySourceTextOrigin};
use crate::display_source_append_plan::DisplaySourceAppendRenderPolicy;
use crate::display_source_progress::DisplaySourceProgressState;
use crate::neovm_bridge::LayoutBufferView;
use crate::types::LineWrapMode;

#[derive(Clone, Copy)]
pub(crate) struct BufferSourceTextRunRenderRequest {
    text_origin: DisplaySourceTextOrigin,
    point_charpos: i64,
    right_edge_px: f32,
    position: DisplayRowPosition,
    geometry: DisplayRowGeometryState,
}

impl BufferSourceTextRunRenderRequest {
    pub(crate) fn new(
        text_start_byte: usize,
        point_charpos: i64,
        right_edge_px: f32,
        position: DisplayRowPosition,
        geometry: DisplayRowGeometryState,
    ) -> Self {
        Self {
            text_origin: DisplaySourceTextOrigin::new(text_start_byte),
            point_charpos,
            right_edge_px,
            position,
            geometry,
        }
    }

    /// The largest prefix of `source_item` that fits the row, or `None` when no
    /// proper prefix does.
    ///
    /// TRUNCATE MODE ONLY — the wrap modes reach their overflow machinery
    /// instead, so this never builds a continuation row. The unrendered tail is
    /// neither returned nor queued: the renderer reseats the producer at the
    /// prefix end with `BufferElementProducer::consume_prefix_to`, so the next
    /// element is produced from the first unfitting character.
    pub(crate) fn prefix_to_fit<B: LayoutBufferView>(
        self,
        source_item: &DisplaySourceStepItem,
        wrap_mode: LineWrapMode,
        append_context: &BufferSourceRowAppendContext<'_, '_, B>,
        source_render: &mut TextRowSourceRenderState<'_>,
    ) -> Option<DisplaySourceStepItem> {
        if wrap_mode != LineWrapMode::Truncate {
            return None;
        }
        let text = source_item.text_run()?;
        let start_charpos = source_item.source_step_char().start_charpos();
        let end_charpos = source_item.source_end_charpos()?;
        // Valid split offsets are 1..hi (exclusive): within the run's chars
        // and strictly before its end charpos, exactly the range the former
        // one-char-at-a-time loop probed.
        let hi = (text.chars().count() as i64).min(end_charpos.saturating_sub(start_charpos));
        if hi <= 1 {
            return None;
        }
        // Binary-search the largest fitting prefix instead of remeasuring
        // every growing prefix: the old loop cloned and measured the run
        // once per character, O(w^2) measurement plus O(w*r) cloning on
        // long truncated lines (logs, minified files). Prefix width is
        // monotone in prefix length for the width policies in use, so the
        // fits predicate crosses once; the search only ever returns an
        // offset whose prefix was measured to fit. A split failure counts
        // as not fitting, which reproduces the old loop's abort-to-None
        // when the very first split fails and is unreachable otherwise.
        let mut fits_at = |split_charpos: i64| -> bool {
            source_item
                .clone()
                .split_text_run_at_charpos(split_charpos, self.text_origin.buffer_byte())
                .is_some_and(|(prefix, _)| {
                    self.source_display_item_fits_text_row(&prefix, append_context, source_render)
                })
        };
        let mut lo = 0i64; // largest verified-fitting offset; 0 = none
        let mut hi_bound = hi; // smallest known (or assumed) non-fitting offset
        while lo + 1 < hi_bound {
            let mid = lo + (hi_bound - lo) / 2;
            if fits_at(start_charpos.saturating_add(mid)) {
                lo = mid;
            } else {
                hi_bound = mid;
            }
        }
        if lo == 0 {
            return None;
        }
        source_item
            .clone()
            .split_text_run_at_charpos(
                start_charpos.saturating_add(lo),
                self.text_origin.buffer_byte(),
            )
            .map(|(prefix, _tail)| prefix)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn render_and_apply<B: LayoutBufferView + ?Sized>(
        self,
        source_item: DisplaySourceStepItem,
        active_face_state: &DisplayRowActiveFaceState,
        append_context: &BufferSourceRowAppendContext<'_, '_, B>,
        cursor_info: &mut CursorCaptureState,
        trailing_whitespace: &mut TrailingWhitespaceRenderState,
        word_wrap: &mut WordWrapRenderState,
        predecessor_row_extend: Option<DisplayRowExtendFace>,
        source_render: &mut TextRowSourceRenderState<'_>,
        progress: &mut DisplaySourceProgressState<'_>,
    ) -> BufferSourceItemRenderOutcome {
        let output_display_point_start = source_render.output_emitter().display_point_len();
        let output_row_positions_start = source_render
            .output_emitter()
            .current_row_display_positions();
        // Snapshot the row's drawn-glyph counts BEFORE this run is appended. The
        // whole-run word-wrap path records candidates after the append, so each
        // candidate's boundary adds the writer's primitive offset, which
        // includes wide-character padding and accounts for composition.
        let row_glyph_checkpoint_start = source_render.capture_glyph_checkpoint();
        let source_end_charpos = source_item.source_end_charpos();
        let source_end_byte_idx = source_item.source_end_byte_idx();
        let source_text = source_item.raw_text_run().unwrap_or_default().to_owned();
        let (_source_step_char, _, _, source_item) = source_item.into_render_parts();
        let mut render_policy = DisplaySourceAppendRenderPolicy::natural();
        let Some(append_progress) = append_context.append_source_display_item_to_text_row(
            &self.geometry,
            source_render,
            source_item,
            self.position,
            DisplayRowAppendKind::SourceText,
            &mut render_policy,
        ) else {
            return BufferSourceItemRenderOutcome::Stop;
        };
        let row_glyph_checkpoint_after_append = source_render.capture_glyph_checkpoint();
        capture_whole_text_run_cursor_if_point(
            cursor_info,
            active_face_state,
            &self.geometry,
            self.point_charpos,
            &append_progress,
        );
        apply_whole_text_run_trailing_whitespace_state(
            &source_text,
            trailing_whitespace,
            &self.geometry,
            &append_progress,
        );
        crate::display_row::word_wrap::record_text_progress(
            &source_text,
            self.text_origin,
            word_wrap,
            output_display_point_start,
            output_row_positions_start,
            row_glyph_checkpoint_start,
            row_glyph_checkpoint_after_append,
            &append_progress,
            predecessor_row_extend,
            active_face_state.row_extend_fill(),
        );
        progress.apply_row_position(append_progress.end());
        if let Some(end_charpos) = source_end_charpos {
            progress.max_charpos(end_charpos);
        }
        if let Some(end_byte_idx) = source_end_byte_idx {
            progress.set_byte_idx(end_byte_idx);
        }
        BufferSourceItemRenderOutcome::Rendered
    }

    /// Inspect the run before transferring ownership to the renderer. A
    /// rejected run is still needed by the character/overflow path.
    pub(crate) fn can_render_whole_run<B: LayoutBufferView>(
        self,
        source_item: &DisplaySourceStepItem,
        append_context: &BufferSourceRowAppendContext<'_, '_, B>,
        source_render: &mut TextRowSourceRenderState<'_>,
    ) -> bool {
        if source_item.text_run().is_none() {
            return false;
        }
        if source_item.source_end_charpos().is_none() || source_item.source_end_byte_idx().is_none()
        {
            return false;
        }
        self.source_display_item_fits_text_row(source_item, append_context, source_render)
    }

    fn source_display_item_fits_text_row<B: LayoutBufferView>(
        self,
        source_item: &DisplaySourceStepItem,
        append_context: &BufferSourceRowAppendContext<'_, '_, B>,
        source_render: &mut TextRowSourceRenderState<'_>,
    ) -> bool {
        let measured_width = {
            let mut measure = source_render.measure_state();
            append_context.measure_source_display_item_width_naturally(
                &self.geometry,
                &mut measure,
                source_item.item(),
                self.position,
                DisplayRowAppendKind::SourceText,
            )
        };
        measured_width
            .map(|width| self.position.x_px() + width <= self.right_edge_px + f32::EPSILON)
            .unwrap_or(false)
    }
}

fn capture_whole_text_run_cursor_if_point(
    cursor_info: &mut CursorCaptureState,
    active_face_state: &DisplayRowActiveFaceState,
    geometry: &DisplayRowGeometryState,
    point_charpos: i64,
    append_progress: &DisplayRowAppendProgress,
) {
    let slot = append_progress.slots().iter().find(|slot| {
        let DisplaySourcePosition::Buffer { char_pos, .. } = slot.source() else {
            return false;
        };
        cursor_info.should_capture_visible_glyph_at(char_pos.get() as i64, point_charpos)
    });
    let Some(slot) = slot else {
        return;
    };
    let DisplaySourcePosition::Buffer { byte_pos, .. } = slot.source() else {
        return;
    };
    capture_cursor_info(
        cursor_info,
        CapturedCursorInfo::from_active_face_state(
            active_face_state,
            CapturedCursorPlacement::from_row_text_position(
                geometry.text_position(slot.x_px(), byte_pos.get(), slot.col()),
                CapturedCursorSlotWidth::Explicit(slot.width_px()),
                false,
            ),
        ),
    );
}

fn apply_whole_text_run_trailing_whitespace_state(
    text: &str,
    trailing_whitespace: &mut TrailingWhitespaceRenderState,
    geometry: &DisplayRowGeometryState,
    append_progress: &DisplayRowAppendProgress,
) {
    if !trailing_whitespace.is_enabled() {
        return;
    }
    for (ch, slot) in text.chars().zip(append_progress.slots()) {
        trailing_whitespace.track_rendered_char(ch, geometry.start_marker_at_x(slot.x_px()));
    }
}
