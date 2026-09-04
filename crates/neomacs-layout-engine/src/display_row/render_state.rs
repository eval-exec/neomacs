use crate::display_item::{DisplayLineHeightPolicy, DisplayRowBreak};
#[cfg(test)]
use crate::display_row::builder::DisplayRowVerticalMetrics;
use crate::display_row::builder::{
    DisplayRowAppendProgress, DisplayRowAppendStatus, DisplayRowGlyphSlot, DisplayRowPosition,
    apply_display_row_source_slot_bounds, merge_display_row_source_slot_bounds,
};
use crate::display_row::finalizer::{RowTrailingFaceFill, RowTrailingFaceFillResult};
use crate::display_row::geometry::{
    DisplayRowGeometryState, DisplayRowMaxX, DisplayRowTextAreaOrigin,
};
use crate::glyph_row_writer;
use neomacs_display_protocol::face::Face;
#[cfg(test)]
use neomacs_display_protocol::frame_glyphs::GlyphRowRole;
use neomacs_display_protocol::glyph_matrix::{
    GlyphArea, GlyphProvenance, GlyphRow, GlyphStringId, GlyphStringSource, GlyphStringSourceId,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct DisplayRowOutputProgress {
    end_x: f32,
    end_col: i64,
    y: f32,
    height: f32,
}

impl DisplayRowOutputProgress {
    pub(crate) fn new(end_x: f32, end_col: i64, y: f32, height: f32) -> Self {
        Self {
            end_x,
            end_col,
            y,
            height,
        }
    }

    pub(crate) fn end_x(self) -> f32 {
        self.end_x
    }

    pub(crate) fn end_col(self) -> i64 {
        self.end_col
    }

    pub(crate) fn y(self) -> f32 {
        self.y
    }

    pub(crate) fn height(self) -> f32 {
        self.height
    }

    pub(crate) fn with_y(self, y: f32) -> Self {
        Self { y, ..self }
    }

    pub(crate) fn with_height(self, height: f32) -> Self {
        Self { height, ..self }
    }
}

pub(crate) struct RenderedDisplayRow {
    row: GlyphRow,
    progress: DisplayRowOutputProgress,
    source_slots: Vec<DisplayRowGlyphSlot>,
    faces: Vec<Face>,
}

impl RenderedDisplayRow {
    pub(crate) fn new(
        row: GlyphRow,
        progress: DisplayRowOutputProgress,
        source_slots: Vec<DisplayRowGlyphSlot>,
        faces: Vec<Face>,
    ) -> Self {
        Self {
            row,
            progress,
            source_slots,
            faces,
        }
    }

    pub(crate) fn row(&self) -> &GlyphRow {
        &self.row
    }

    /// Restore GNU's `(glyph->object, glyph->charpos)` identity after chrome
    /// formatting flattened several Lisp strings into one layout source.
    pub(crate) fn remap_root_string_provenance(
        &mut self,
        root_string: GlyphStringId,
        mut resolve: impl FnMut(usize) -> Option<(GlyphStringId, usize)>,
    ) {
        let mappings: [Vec<Option<(GlyphStringId, usize, GlyphStringSourceId)>>; GlyphArea::COUNT] =
            std::array::from_fn(|area| {
                self.row.glyphs[area]
                    .iter()
                    .map(|glyph| {
                        let GlyphProvenance::Str { source, index } = glyph.provenance else {
                            return None;
                        };
                        self.row
                            .string_source(source)
                            .filter(|entry| entry.string() == root_string)?;
                        let (string, source_index) = resolve(index)?;
                        Some((string, source_index, source))
                    })
                    .collect()
            });

        let mut remapped_tokens = std::collections::HashMap::new();
        for (string, _, _) in mappings.iter().flatten().flatten().copied() {
            if string == root_string || remapped_tokens.contains_key(&string) {
                continue;
            }
            let Some(token) = self.row.push_string_source(GlyphStringSource::new(string)) else {
                continue;
            };
            remapped_tokens.insert(string, token);
        }

        for area in GlyphArea::ALL {
            for (glyph, mapping) in self.row.glyphs[area.index()]
                .iter_mut()
                .zip(&mappings[area.index()])
            {
                let Some((string, source_index, original_token)) = *mapping else {
                    continue;
                };
                let token = if string == root_string {
                    original_token
                } else if let Some(token) = remapped_tokens.get(&string).copied() {
                    token
                } else {
                    continue;
                };
                glyph.provenance = GlyphProvenance::string(token, source_index);
            }
        }
    }

    pub(crate) fn progress(&self) -> DisplayRowOutputProgress {
        self.progress
    }

    pub(crate) fn source_slots(&self) -> &[DisplayRowGlyphSlot] {
        &self.source_slots
    }

    pub(crate) fn apply_source_slot_bounds_to(&self, row: &mut GlyphRow) {
        apply_display_row_source_slot_bounds(row, &self.source_slots);
    }

    pub(crate) fn materialize_output_row(
        &self,
        pixel_y: f32,
        height_px: f32,
        ascent_px: f32,
    ) -> GlyphRow {
        let mut row = self.row.clone();
        self.apply_source_slot_bounds_to(&mut row);
        // Normalize only; the bidi reorder happens once at row install
        // (window chrome via the `Complete` lifecycle, frame chrome via the
        // finalizer in `frame_chrome_display_row`).
        glyph_row_writer::normalize_external_row(&mut row);
        row.pixel_y = pixel_y;
        row.height_px = height_px;
        row.ascent_px = ascent_px;
        row
    }

    #[cfg(test)]
    pub(crate) fn append_fragment_to_current_row(&self, row: &mut GlyphRow) -> DisplayRowPosition {
        let rendered_row = self.row();
        row.enabled = true;
        row.role = rendered_row.role;
        row.mode_line = matches!(rendered_row.role, GlyphRowRole::ModeLine);
        row.displays_text |=
            rendered_row.displays_text || !rendered_row.glyphs[GlyphArea::Text.index()].is_empty();
        row.glyphs[GlyphArea::Text.index()]
            .extend(rendered_row.glyphs[GlyphArea::Text.index()].iter().cloned());
        // Carry a fringe bitmap recorded on the fragment (e.g. an overlay
        // before-string with a `(left-fringe …)` display spec) onto the output
        // row. The first fragment to set a given side wins (GNU shows one bitmap
        // per fringe per row).
        if row.left_fringe_bitmap.is_none() {
            row.left_fringe_bitmap = rendered_row.left_fringe_bitmap;
        }
        if row.right_fringe_bitmap.is_none() {
            row.right_fringe_bitmap = rendered_row.right_fringe_bitmap;
        }
        DisplayRowVerticalMetrics::from_row(rendered_row).include_in_row(row);
        merge_display_row_source_slot_bounds(row, &self.source_slots);
        display_row_output_end_position(self.progress())
    }

    pub(crate) fn faces(&self) -> &[Face] {
        &self.faces
    }

    /// Complete a rendered row with an exact synthetic trailing fill and keep
    /// its output progress consistent with the new glyph.  Source slots are
    /// deliberately unchanged: the fill has no Lisp-string or buffer source.
    pub(crate) fn apply_trailing_face_fill(
        &mut self,
        fill: RowTrailingFaceFill,
    ) -> RowTrailingFaceFillResult {
        let result = fill.apply_to(&mut self.row);
        if result == RowTrailingFaceFillResult::Appended {
            self.progress.end_x += fill.width_px();
            self.progress.end_col = self
                .progress
                .end_col
                .saturating_add(i64::from(fill.width_cols()));
        }
        result
    }

    #[cfg(test)]
    pub(crate) fn into_row(self) -> GlyphRow {
        self.row
    }

    pub(crate) fn normalize_external_row(&mut self) {
        // Normalize the freshly built logical-order row; the single bidi
        // reorder happens at install (see `RenderedDisplayRow::materialize_output_row`).
        glyph_row_writer::normalize_external_row(&mut self.row);
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum DisplayRowRenderStop {
    SourceExhausted,
    Clipped,
    RowBreak(DisplayRowBreak),
}

pub(crate) struct DisplayRowRenderIntoRowResult {
    progress: DisplayRowOutputProgress,
    source_slots: Vec<DisplayRowGlyphSlot>,
    faces: Vec<Face>,
    stop: DisplayRowRenderStop,
}

impl DisplayRowRenderIntoRowResult {
    pub(crate) fn new(
        progress: DisplayRowOutputProgress,
        source_slots: Vec<DisplayRowGlyphSlot>,
        faces: Vec<Face>,
        stop: DisplayRowRenderStop,
    ) -> Self {
        Self {
            progress,
            source_slots,
            faces,
            stop,
        }
    }

    pub(crate) fn faces(&self) -> &[Face] {
        &self.faces
    }

    #[cfg(test)]
    pub(crate) fn progress(&self) -> DisplayRowOutputProgress {
        self.progress
    }

    #[cfg(test)]
    pub(crate) fn stop(&self) -> DisplayRowRenderStop {
        self.stop
    }

    fn merge_source_slot_bounds_into(&self, row: &mut GlyphRow) {
        merge_display_row_source_slot_bounds(row, &self.source_slots);
    }

    pub(crate) fn apply_current_row_effects_to(&self, row: &mut GlyphRow) {
        self.merge_source_slot_bounds_into(row);
    }

    pub(crate) fn into_current_row_parts(
        self,
    ) -> (
        DisplayRowOutputProgress,
        Vec<DisplayRowGlyphSlot>,
        Vec<Face>,
        DisplayRowRenderStop,
    ) {
        (self.progress, self.source_slots, self.faces, self.stop)
    }

    pub(crate) fn with_row(self, row: GlyphRow) -> RenderedDisplayRow {
        RenderedDisplayRow::new(row, self.progress, self.source_slots, self.faces)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DisplayRowRenderBounds {
    start: DisplayRowPosition,
    max_x: DisplayRowMaxX,
    text_area_origin: DisplayRowTextAreaOrigin,
}

impl DisplayRowRenderBounds {
    /// Bounds for a row laid out in its own coordinates (the text area
    /// starts at 0): chrome rows, mock frames and unit tests.
    pub(crate) fn new(start: DisplayRowPosition, max_x: DisplayRowMaxX) -> Self {
        Self {
            start,
            max_x,
            text_area_origin: DisplayRowTextAreaOrigin::row_local(),
        }
    }

    /// Bounds for a row of a window whose text area starts at a
    /// frame-absolute x; the buffer text path uses this so window-local GNU
    /// rules see window-local coordinates.
    pub(crate) fn in_window_text_area(
        start: DisplayRowPosition,
        max_x: DisplayRowMaxX,
        text_area_origin: DisplayRowTextAreaOrigin,
    ) -> Self {
        Self {
            start,
            max_x,
            text_area_origin,
        }
    }

    pub(crate) fn whole_row(width_px: f32) -> Self {
        Self::new(
            DisplayRowPosition::new(0.0, 0),
            DisplayRowMaxX::Bounded(width_px.max(0.0)),
        )
    }

    pub(crate) fn unbounded_from(start: DisplayRowPosition) -> Self {
        Self::new(start, DisplayRowMaxX::Unbounded)
    }

    pub(crate) fn start(self) -> DisplayRowPosition {
        self.start
    }

    pub(crate) fn max_x(self) -> DisplayRowMaxX {
        self.max_x
    }

    pub(crate) fn text_area_origin(self) -> DisplayRowTextAreaOrigin {
        self.text_area_origin
    }
}

pub(crate) struct CurrentTextRowRenderOutcome {
    stop: DisplayRowRenderStop,
    source_slots: Vec<DisplayRowGlyphSlot>,
    end: DisplayRowPosition,
    row_height_px: f32,
    row_ascent_px: f32,
}

impl CurrentTextRowRenderOutcome {
    pub(crate) fn new(
        stop: DisplayRowRenderStop,
        source_slots: Vec<DisplayRowGlyphSlot>,
        end: DisplayRowPosition,
        row_height_px: f32,
        row_ascent_px: f32,
    ) -> Self {
        Self {
            stop,
            source_slots,
            end,
            row_height_px,
            row_ascent_px,
        }
    }

    pub(crate) fn stop(&self) -> DisplayRowRenderStop {
        self.stop
    }

    pub(crate) fn source_slots(&self) -> &[DisplayRowGlyphSlot] {
        &self.source_slots
    }

    pub(crate) fn end_position(&self) -> DisplayRowPosition {
        self.end
    }

    pub(crate) fn include_vertical_metrics(&self, geometry: &mut DisplayRowGeometryState) {
        if matches!(
            self.stop,
            DisplayRowRenderStop::RowBreak(row_break)
                if row_break.line_height == DisplayLineHeightPolicy::ContentOnly
        ) {
            geometry.replace_current_row_metrics(self.row_height_px, self.row_ascent_px);
        } else {
            geometry.include_glyph_vertical_metrics(self.row_height_px, self.row_ascent_px);
        }
    }

    pub(crate) fn into_append_progress(
        self,
        start: DisplayRowPosition,
    ) -> DisplayRowAppendProgress {
        display_row_append_progress_from_render_result(
            start,
            self.end,
            self.stop,
            self.source_slots,
        )
    }
}

fn display_row_append_progress_from_render_result(
    start: DisplayRowPosition,
    end: DisplayRowPosition,
    stop: DisplayRowRenderStop,
    slots: Vec<DisplayRowGlyphSlot>,
) -> DisplayRowAppendProgress {
    DisplayRowAppendProgress::from_positions(
        start,
        end,
        match stop {
            DisplayRowRenderStop::SourceExhausted => DisplayRowAppendStatus::Complete,
            DisplayRowRenderStop::Clipped => DisplayRowAppendStatus::Clipped,
            DisplayRowRenderStop::RowBreak(_) => DisplayRowAppendStatus::RowBreak,
        },
        slots,
    )
}

pub(crate) fn display_row_output_end_position(
    progress: DisplayRowOutputProgress,
) -> DisplayRowPosition {
    DisplayRowPosition::new(
        progress.end_x().max(0.0),
        usize::try_from(progress.end_col().max(0)).unwrap_or(usize::MAX),
    )
}

pub(crate) fn display_row_progress(
    end: DisplayRowPosition,
    y: f32,
    height: f32,
) -> DisplayRowOutputProgress {
    DisplayRowOutputProgress::new(
        end.x_px().max(0.0),
        end.col().min(i64::MAX as usize) as i64,
        y,
        height,
    )
}
