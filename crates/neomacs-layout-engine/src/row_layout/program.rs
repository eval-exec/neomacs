//! Complete, bounded row programs. Capture resolves source semantics. Rows
//! with bounded font policy defer measurement to a worker-local font service;
//! other admitted rows retain captured measurements. Execution uses the canonical writer
//! and never consults evaluator TLS.

use super::{ResolvedMappedTextInput, ResolvedSpacingInput, ResolvedTextInput};
use crate::display_face_ref::render_face_ref_id;
use crate::display_item::*;
use crate::display_pixel_calc::PixelCalcContext;
use crate::display_row::append_context::DisplayRowLineWrap;
use crate::display_row::builder::*;
use crate::display_row::face_state::{DisplayRowFace, DisplayRowMeasurementMode};
use crate::display_row::finalizer::DisplayRowLineEndFinalizer;
use crate::display_row::geometry::DisplayRowTextAreaOrigin;
use crate::display_row::metrics::DisplayRowFallbackMetrics;
use crate::display_text_run_measurement::DisplayTextRunMeasurement;
use crate::types::LineWrapMode;
use neomacs_display_protocol::frame_glyphs::GlyphRowRole;
use neomacs_display_protocol::glyph_matrix::{GlyphArea, GlyphRow};
use neomacs_display_protocol::types::{Color, FaceId};
use rustc_hash::FxHashMap;

/// Limits apply to capture as well as execution. A partial row is never a result.
#[derive(Clone, Copy, Debug)]
pub(crate) struct RowProgramLimits {
    pub items: usize,
    pub text_bytes: usize,
    pub glyphs: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RowProgramError {
    Unsupported,
    Budget,
    Incomplete,
    Overflow,
    MissingMeasurement,
    Cancelled,
}

/// No pixel-calculation expressions, image catalogs, or Lisp values. Literal
/// spacing is the only supported spacing operation at this boundary.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RowProgramGeometry {
    pub inherited_line_spacing: f32,
    pub character_wrap: bool,
    pub word_wrap: bool,
    pub fringe: Option<crate::buffer_source::fringe_arrows::TruncationContinuationFringeRequest>,
    pub width: f32,
    pub metrics: DisplayRowFallbackMetrics,
    pub tabs: DisplayTabPolicy,
    pub base_face: FaceId,
    pub background: Color,
}

impl RowProgramGeometry {
    /// GNU `it->line_wrap` for the row this program produces.
    /// `character_wrap` is the window's non-truncating mode and `word_wrap`
    /// its `word-wrap`, which is exactly the pair `init_iterator` folds into
    /// WORD_WRAP vs WINDOW_WRAP (src/xdisp.c:3416-3426).
    fn line_wrap(&self) -> DisplayRowLineWrap {
        DisplayRowLineWrap::for_window(
            if self.character_wrap {
                LineWrapMode::Wrap
            } else {
                LineWrapMode::Truncate
            },
            self.word_wrap,
        )
    }

    fn layout(&self) -> DisplayRowLayout {
        DisplayRowLayout {
            role: GlyphRowRole::Text,
            y_px: 0.0,
            height_px: self.metrics.row_height(),
            ascent_px: self.metrics.ascent(),
            char_width_px: self.metrics.char_width(),
            tab_policy: self.tabs.clone(),
            line_number_width_px: 0.0,
            base_face: RenderFaceRef::FaceId(self.base_face),
            pixel_calc: PixelCalcContext::for_chrome_row(
                self.width,
                self.metrics.char_width(),
                self.metrics.row_height(),
                Default::default(),
            ),
            space_image_params: None,
            line_wrap: self.line_wrap(),
        }
    }
}

#[derive(Clone, Debug)]
enum Operation {
    Text(ResolvedTextInput),
    Mapped(ResolvedMappedTextInput),
    Space(ResolvedSpacingInput),
    Break {
        span: SourceSpan,
        face: FaceId,
        line_height: DisplayLineHeightPolicy,
        line_spacing: f32,
        layout: DisplayItemLayout,
        pointer_appearance: Option<DisplayPointerAppearance>,
        edges: neomacs_display_protocol::face::BoxVerticalEdges,
        membership: neomacs_display_protocol::face::BoxRunMembership,
    },
}

fn preserves_buffer_anchor(source: &DisplaySourcePosition) -> bool {
    matches!(source, DisplaySourcePosition::LispString { .. })
        || matches!(source, DisplaySourcePosition::Synthetic { source_id, .. }
            if source_id.get() == crate::display_row::source_append::SyntheticTextMarker::InvisibleEllipsis.source_id())
}

impl Operation {
    fn capture(
        item: DisplayItem,
        base: FaceId,
        default_height: f32,
        inherited_line_spacing: f32,
    ) -> Result<Self, RowProgramError> {
        match &item.kind {
            DisplayItemKind::TextRun(_) => ResolvedTextInput::capture(item, base)
                .map(Self::Text)
                .map_err(|_| RowProgramError::Unsupported),
            DisplayItemKind::SourceMappedText(_) => ResolvedMappedTextInput::capture(item, base)
                .map(Self::Mapped)
                .map_err(|_| RowProgramError::Unsupported),
            DisplayItemKind::Stretch(_) => ResolvedSpacingInput::capture(item, base)
                .map(Self::Space)
                .map_err(|_| RowProgramError::Unsupported),
            DisplayItemKind::RowBreak(value)
                if !matches!(
                    value.line_spacing,
                    DisplayLineSpacingPolicy::Scale {
                        reference: DisplayLineSpacingReference::Named(_),
                        ..
                    }
                ) =>
            {
                Ok(Self::Break {
                    span: item.span,
                    face: render_face_ref_id(item.face, base),
                    line_height: value.line_height,
                    line_spacing: value
                        .line_spacing
                        .resolve(default_height, inherited_line_spacing),
                    layout: item.layout,
                    pointer_appearance: item.pointer_appearance,
                    edges: item.box_vertical_edges,
                    membership: item.box_run_membership,
                })
            }
            _ => Err(RowProgramError::Unsupported),
        }
    }

    fn into_item(self) -> DisplayItem {
        match self {
            Self::Text(input) => DisplayItem {
                span: input.span,
                face: RenderFaceRef::FaceId(input.face),
                kind: DisplayItemKind::TextRun(input.run),
                layout: input.layout,
                pointer_appearance: input.pointer_appearance,
                box_vertical_edges: input.box_vertical_edges,
                box_run_membership: input.box_run_membership,
            },
            Self::Mapped(input) => DisplayItem {
                span: input.span,
                face: RenderFaceRef::FaceId(input.face),
                kind: DisplayItemKind::SourceMappedText(
                    DisplaySourceMappedText::face_segment(input.text, input.glyph_string_start)
                        .with_measurement_face(input.measurement_face),
                ),
                layout: input.layout,
                pointer_appearance: input.pointer_appearance,
                box_vertical_edges: input.box_vertical_edges,
                box_run_membership: input.box_run_membership,
            },
            Self::Space(input) => input.into_display_item(),
            Self::Break {
                span,
                face,
                line_height,
                line_spacing,
                layout,
                pointer_appearance,
                edges,
                membership,
            } => {
                let mut item = DisplayItem::new(
                    span,
                    RenderFaceRef::FaceId(face),
                    DisplayItemKind::RowBreak(DisplayRowBreak {
                        line_height,
                        line_spacing: DisplayLineSpacingPolicy::Pixels(line_spacing),
                    }),
                );
                item.box_vertical_edges = edges;
                item.box_run_membership = membership;
                item.layout = layout;
                item.pointer_appearance = pointer_appearance;
                item
            }
        }
    }
}

/// Measurements come from the capturing frame's actual selected fonts. A
/// missing query is an admission failure, never permission to guess metrics.
#[derive(Clone, Debug)]
struct Measurements {
    backend: DisplayGeometryBackend,
    char_width: f32,
    advances: FxHashMap<(char, FaceId, u8), Option<f32>>,
    vertical: FxHashMap<(char, FaceId), Option<DisplayRowVerticalMetrics>>,
    faces: FxHashMap<FaceId, (Option<f32>, Option<DisplayRowVerticalMetrics>)>,
    missing: bool,
}

impl Measurements {
    fn capture_item(
        &mut self,
        item: &DisplayItem,
        measurer: &mut dyn DisplayGlyphMeasurer,
        base: FaceId,
    ) {
        let face = render_face_ref_id(item.face, base);
        self.faces.entry(face).or_insert_with(|| {
            (
                measurer.face_space_width_px(face),
                measurer.face_vertical_metrics_px(face),
            )
        });
        let text = match &item.kind {
            DisplayItemKind::TextRun(run) => run.text.as_ref(),
            DisplayItemKind::SourceMappedText(run) => run.text.as_ref(),
            _ => "",
        };
        // Include the primary-space fallback used by tabs and literal spaces.
        for ch in text
            .chars()
            .chain(std::iter::once(' '))
            .chain(match &item.kind {
                DisplayItemKind::Stretch(DisplayStretch {
                    width: DisplayStretchWidth::RelativeToSource { source, .. },
                    ..
                }) => source.as_rust_char(),
                _ => None,
            })
        {
            self.vertical
                .entry((ch, face))
                .or_insert_with(|| measurer.glyph_vertical_metrics_px(ch, face));
            // Natural Unicode characters occupy zero, one or two columns.
            // Unsupported future width domains are rejected during execution.
            for columns in 0..=2 {
                let fallback = self.char_width.max(1.0) * f32::from(columns.max(1));
                self.advances
                    .entry((ch, face, columns))
                    .or_insert_with(|| measurer.glyph_advance_px(ch, face, columns, fallback));
            }
        }
    }
}

impl DisplayGlyphMeasurer for Measurements {
    fn glyph_advance_px(
        &mut self,
        ch: char,
        face: FaceId,
        columns: u8,
        fallback: f32,
    ) -> Option<f32> {
        if fallback != self.char_width.max(1.0) * f32::from(columns.max(1)) {
            self.missing = true;
        }
        self.advances
            .get(&(ch, face, columns))
            .copied()
            .unwrap_or_else(|| {
                self.missing = true;
                None
            })
    }
    fn glyph_vertical_metrics_px(
        &mut self,
        ch: char,
        face: FaceId,
    ) -> Option<DisplayRowVerticalMetrics> {
        self.vertical.get(&(ch, face)).copied().unwrap_or_else(|| {
            self.missing = true;
            None
        })
    }
    fn face_vertical_metrics_px(&mut self, face: FaceId) -> Option<DisplayRowVerticalMetrics> {
        self.faces
            .get(&face)
            .map(|entry| entry.1)
            .unwrap_or_else(|| {
                self.missing = true;
                None
            })
    }
    fn face_space_width_px(&mut self, face: FaceId) -> Option<f32> {
        self.faces
            .get(&face)
            .map(|entry| entry.0)
            .unwrap_or_else(|| {
                self.missing = true;
                None
            })
    }
    fn display_geometry_backend(&self) -> DisplayGeometryBackend {
        self.backend
    }
    fn text_run_advances_px(&mut self, _: &str, _: FaceId, _: f32) -> DisplayTextRunMeasurement {
        // Every admitted text operation carries its whole-run plan separately.
        self.missing = true;
        DisplayTextRunMeasurement::PerChar
    }
}

#[derive(Clone, Debug)]
pub(crate) struct RowProgram {
    trailing_text_continues: bool,
    buffer_source_start: Option<DisplaySourcePosition>,
    geometry: RowProgramGeometry,
    deferred_fonts: Option<std::sync::Arc<super::font_measurement::FontMeasurementSnapshot>>,
    operations: Vec<(Operation, DisplayTextRunMeasurement)>,
    measurements: Measurements,
    faces: Vec<DisplayRowFace>,
    limits: RowProgramLimits,
}

/// Source consumption at a visual-row boundary. A continuation consumes no
/// newline and therefore has neither a newline hit cell nor an end-minus-one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ComputedRowEnd {
    Newline,
    Continuation,
}

pub(crate) struct ComputedRow {
    pub row: GlyphRow,
    pub slots: Vec<DisplayRowGlyphSlot>,
    pub slot_heights: Vec<f32>,
    pub source: SourceSpan,
    pub end: DisplayRowPosition,
    pub end_kind: ComputedRowEnd,
    pub terminator_width: f32,
    pub terminator_height: f32,
    pub fringe: Option<crate::buffer_source::fringe_arrows::TruncationContinuationFringeRequest>,
    pub continuation: bool,
}

pub(crate) enum RowMeasurements<'a> {
    Captured(&'a mut dyn DisplayGlyphMeasurer),
    Deferred(std::sync::Arc<super::font_measurement::FontMeasurementSnapshot>),
}

mod assembly;
mod continuation;

impl RowProgram {
    /// The iterator must be bounded at acquisition, before it allocates an
    /// item. These limits additionally bound everything retained in the job.
    pub(crate) fn capture(
        geometry: RowProgramGeometry,
        items: impl IntoIterator<Item = DisplayItem>,
        faces: Vec<DisplayRowFace>,
        measurer: &mut dyn DisplayGlyphMeasurer,
        limits: RowProgramLimits,
    ) -> Result<Self, RowProgramError> {
        Self::capture_inner(geometry, items, faces, Some(measurer), None, limits, false)
    }

    pub(crate) fn supports_deferred_text(items: &[DisplayItem]) -> bool {
        items.iter().all(|item| match &item.kind {
            DisplayItemKind::TextRun(_) => true,
            DisplayItemKind::SourceMappedText(_) => true,
            DisplayItemKind::Stretch(DisplayStretch {
                width: DisplayStretchWidth::RelativeToSource { source, .. },
                ..
            }) => source.as_rust_char().is_some(),
            DisplayItemKind::Stretch(_) | DisplayItemKind::RowBreak(_) => true,
            _ => false,
        })
    }

    #[cfg(test)]
    pub(crate) fn capture_deferred(
        geometry: RowProgramGeometry,
        items: Vec<DisplayItem>,
        faces: Vec<DisplayRowFace>,
        fonts: std::sync::Arc<super::font_measurement::FontMeasurementSnapshot>,
        limits: RowProgramLimits,
    ) -> Result<Self, RowProgramError> {
        if !Self::supports_deferred_text(&items) {
            return Err(RowProgramError::Unsupported);
        }
        Self::capture_inner(geometry, items, faces, None, Some(fonts), limits, false)
    }

    pub(crate) fn capture_fragment(
        geometry: RowProgramGeometry,
        items: Vec<DisplayItem>,
        faces: Vec<DisplayRowFace>,
        measurements: RowMeasurements<'_>,
        limits: RowProgramLimits,
    ) -> Result<Self, RowProgramError> {
        let (measurer, fonts) = match measurements {
            RowMeasurements::Captured(measurer) => (Some(measurer), None),
            RowMeasurements::Deferred(fonts) => {
                if !Self::supports_deferred_text(&items) {
                    return Err(RowProgramError::Unsupported);
                }
                (None, Some(fonts))
            }
        };
        Self::capture_inner(geometry, items, faces, measurer, fonts, limits, true)
    }

    fn capture_inner(
        geometry: RowProgramGeometry,
        items: impl IntoIterator<Item = DisplayItem>,
        faces: Vec<DisplayRowFace>,
        mut measurer: Option<&mut dyn DisplayGlyphMeasurer>,
        deferred_fonts: Option<std::sync::Arc<super::font_measurement::FontMeasurementSnapshot>>,
        limits: RowProgramLimits,
        permit_fragment: bool,
    ) -> Result<Self, RowProgramError> {
        if faces.len() > limits.items || !geometry.width.is_finite() || geometry.width <= 0.0 {
            return Err(RowProgramError::Budget);
        }
        let mut metadata_bytes = geometry
            .tabs
            .stop_cols
            .len()
            .saturating_mul(std::mem::size_of::<usize>());
        for face in &faces {
            if face.stipple.is_some() {
                return Err(RowProgramError::Unsupported);
            }
            metadata_bytes = metadata_bytes
                .saturating_add(face.font_family.len())
                .saturating_add(face.font_file_path.as_ref().map_or(0, String::len))
                .saturating_add(face.lisp_name.as_ref().map_or(0, String::len));
        }
        if metadata_bytes > limits.text_bytes {
            return Err(RowProgramError::Budget);
        }
        let mut measurements = Measurements {
            backend: measurer
                .as_ref()
                .map_or(DisplayGeometryBackend::WindowSystemPixels, |m| {
                    m.display_geometry_backend()
                }),
            char_width: geometry.metrics.char_width(),
            advances: Default::default(),
            vertical: Default::default(),
            faces: Default::default(),
            missing: false,
        };
        let mut operations = Vec::new();
        let mut bytes = metadata_bytes;
        let mut glyphs = 0usize;
        let mut complete = false;
        for item in items {
            if complete {
                return Err(RowProgramError::Unsupported);
            }
            if operations.len() == limits.items {
                return Err(RowProgramError::Budget);
            }
            if item.layout.break_after_row {
                return Err(RowProgramError::Unsupported);
            }
            let text = match &item.kind {
                DisplayItemKind::TextRun(run) => run.text.as_ref(),
                DisplayItemKind::SourceMappedText(run) => run.text.as_ref(),
                _ => "",
            };
            bytes = bytes.saturating_add(text.len());
            // Wide glyph padding and the newline fill can each add slots.
            glyphs = glyphs.saturating_add(text.chars().count().saturating_mul(2).max(1));
            if bytes > limits.text_bytes || glyphs > limits.glyphs {
                return Err(RowProgramError::Budget);
            }
            let face = render_face_ref_id(item.face, geometry.base_face);
            if !faces.iter().any(|candidate| candidate.face_id == face)
                || !faces
                    .iter()
                    .any(|candidate| candidate.face_id == item_measurement_face(&item, face))
            {
                return Err(RowProgramError::Unsupported);
            }
            let plan = if text.is_empty() || measurer.is_none() {
                DisplayTextRunMeasurement::PerChar
            } else {
                measurer.as_mut().unwrap().text_run_advances_px(
                    text,
                    item_measurement_face(&item, face),
                    geometry.metrics.char_width().max(1.0),
                )
            };
            if let Some(measurer) = measurer.as_mut() {
                measurements.capture_item(&item, *measurer, geometry.base_face);
            }
            complete = matches!(item.kind, DisplayItemKind::RowBreak(_));
            operations.push((
                Operation::capture(
                    item,
                    geometry.base_face,
                    geometry.metrics.row_height(),
                    geometry.inherited_line_spacing,
                )?,
                plan,
            ));
        }
        if operations.is_empty() || (!complete && !permit_fragment) {
            return Err(RowProgramError::Incomplete);
        }
        Ok(Self {
            trailing_text_continues: false,
            buffer_source_start: None,
            geometry,
            deferred_fonts,
            operations,
            measurements,
            faces,
            limits,
        })
    }

    pub(crate) fn with_buffer_source_start(mut self, start: DisplaySourcePosition) -> Self {
        self.buffer_source_start = Some(start);
        self
    }

    pub(crate) fn with_trailing_text_continuation(mut self, continues: bool) -> Self {
        self.trailing_text_continues = continues;
        self
    }

    pub(crate) fn font_snapshot_identity(&self) -> Option<usize> {
        self.deferred_fonts
            .as_ref()
            .map(|snapshot| std::sync::Arc::as_ptr(snapshot) as usize)
    }

    pub(crate) fn font_bytes(&self) -> usize {
        self.deferred_fonts
            .as_ref()
            .map_or(0, |fonts| fonts.bytes())
    }

    pub(crate) fn measure_on_worker(
        &mut self,
        worker: &mut super::font_measurement::WorkerFontMeasurements,
        cancelled: &impl Fn() -> bool,
    ) -> Result<(), RowProgramError> {
        let Some(snapshot) = self.deferred_fonts.take() else {
            return Ok(());
        };
        let fonts = worker.prepare(&snapshot, &self.faces, cancelled)?;
        let mut measurer = crate::display_row::face_state::DisplayRowGlyphMeasurer::with_mode(
            &self.faces,
            Some(&mut *fonts),
            self.geometry.metrics.char_width(),
            crate::glyph_advance::GlyphAdvanceQuantization::PreserveLogicalPixels,
            DisplayRowMeasurementMode::ConcreteFont,
        );
        for (operation, plan) in &mut self.operations {
            if cancelled() {
                return Err(RowProgramError::Cancelled);
            }
            let item = operation.clone().into_item();
            let face = render_face_ref_id(item.face, self.geometry.base_face);
            let text = match &item.kind {
                DisplayItemKind::TextRun(run) => run.text.as_ref(),
                DisplayItemKind::SourceMappedText(run) => run.text.as_ref(),
                _ => "",
            };
            if !text.is_empty() {
                *plan = measurer.text_run_advances_px(
                    text,
                    item_measurement_face(&item, face),
                    self.geometry.metrics.char_width().max(1.0),
                );
            }
            self.measurements
                .capture_item(&item, &mut measurer, self.geometry.base_face);
        }
        if fonts.worker_font_policy_missing() {
            return Err(RowProgramError::MissingMeasurement);
        }
        Ok(())
    }

    pub(crate) fn limits(&self) -> RowProgramLimits {
        self.limits
    }

    /// Single-row test adapter. A continuation requires the visual-row API;
    /// callers of this adapter only accept a complete physical line.
    #[cfg(test)]
    pub(crate) fn compute(
        self,
        cancelled: impl Fn() -> bool,
    ) -> Result<ComputedRow, RowProgramError> {
        let mut rows = self.compute_visual_rows(1, cancelled)?;
        let row = rows.pop().ok_or(RowProgramError::Incomplete)?;
        if row.end_kind == ComputedRowEnd::Continuation {
            return Err(RowProgramError::Overflow);
        }
        Ok(row)
    }

    pub(crate) fn compute_visual_rows(
        mut self,
        row_limit: usize,
        cancelled: impl Fn() -> bool,
    ) -> Result<Vec<ComputedRow>, RowProgramError> {
        if self.deferred_fonts.is_some() {
            return Err(RowProgramError::MissingMeasurement);
        }
        if row_limit == 0 {
            return Err(RowProgramError::Budget);
        }
        // Buffer text may wrap around owned strings. A string that itself
        // overflows still needs a resumable string-stack row entry point.
        let can_wrap = self.geometry.character_wrap
            && self
                .operations
                .iter()
                .all(|(operation, _)| match operation {
                    Operation::Text(_) | Operation::Mapped(_) | Operation::Break { .. } => true,
                    _ => false,
                });
        let physical_complete = self.is_complete();
        let layout = self.geometry.layout();
        let mut row = new_display_row(&layout);
        let mut position = DisplayRowPosition::new(0.0, 0);
        let mut slots = Vec::new();
        let mut slot_heights = Vec::new();
        let mut source_start = self.buffer_source_start;
        let mut source_end = None;
        let mut terminator_width = self.geometry.metrics.char_width();
        let mut terminator_height = self.geometry.metrics.row_height();
        let mut rows = Vec::new();
        let mut completed_glyphs = 0usize;
        let mut predecessor_extend = None;
        let mut physical_line_tabs =
            crate::display_row::builder::DisplayPhysicalLineTabState::default();
        let operations: Vec<_> = self
            .operations
            .into_iter()
            .map(|(operation, plan)| (operation.into_item(), plan))
            .collect();
        // Insertions preserve the buffer iterator at their attachment. Keep
        // that origin separately from each string's glyph provenance so a
        // word-wrap rewind replays the insertion before its anchor character.
        let mut buffer_position = source_start.clone();
        let buffer_anchors: Vec<_> = operations
            .iter()
            .map(|(item, _)| {
                if preserves_buffer_anchor(&item.span.start) {
                    buffer_position
                        .clone()
                        .unwrap_or_else(|| item.span.start.clone())
                } else {
                    buffer_position = Some(item.span.end.clone());
                    item.span.start.clone()
                }
            })
            .collect();
        let mut operation_index = 0;
        let mut operation_offset = 0;
        let mut scalar_operations = rustc_hash::FxHashSet::default();
        let mut word_wrap = crate::display_row::walk_state::WordWrapRenderState::new(
            can_wrap && self.geometry.word_wrap,
        );
        while let Some((original, original_plan)) = operations.get(operation_index) {
            let item = if operation_offset == 0 {
                original.clone()
            } else {
                // A program resumes an operation it already placed part of, so
                // only a resumable tail can express the cut; an operation the
                // line refused whole (or exhausted) has no program form.
                match crate::display_row::render_item::clipped_display_item_remainder_after_chars(
                    original.clone(),
                    operation_offset,
                ) {
                    crate::display_row::render_item::DisplayRowClippedRemainder::Resume(item) => {
                        item
                    }
                    crate::display_row::render_item::DisplayRowClippedRemainder::DeferWhole(_)
                    | crate::display_row::render_item::DisplayRowClippedRemainder::Nothing => {
                        return Err(RowProgramError::Unsupported);
                    }
                }
            };
            let scalar_plan;
            let original_plan = if scalar_operations.contains(&operation_index) {
                let DisplayItemKind::TextRun(run) = &original.kind else {
                    unreachable!()
                };
                scalar_plan = original_plan.scalar_fallback(&run.text);
                &scalar_plan
            } else {
                original_plan
            };
            let plan = measurement_suffix(
                original_plan,
                operation_offset,
                text_byte_offset(original, operation_offset)?,
            );
            if cancelled() {
                return Err(RowProgramError::Cancelled);
            }
            source_start.get_or_insert_with(|| buffer_anchors[operation_index].clone());
            source_end = Some(item.span.end.clone());
            let newline = if let DisplayItemKind::RowBreak(value) = item.kind {
                Some((
                    value,
                    render_face_ref_id(item.face, self.geometry.base_face),
                    item.box_vertical_edges,
                    item.box_run_membership,
                ))
            } else {
                None
            };
            // Source-slot height describes the active face at this item,
            // not the eventual maximum height of the complete row.
            let face = render_face_ref_id(item.face, self.geometry.base_face);
            let face_metrics = self
                .faces
                .iter()
                .find(|candidate| candidate.face_id == face)
                .map(|face| face.metrics)
                .ok_or(RowProgramError::Unsupported)?;
            let face_height = face_metrics.line_height_px();
            let active_extend = self
                .faces
                .iter()
                .find(|value| value.face_id == face && value.extend)
                .map(|value| {
                    crate::display_row::face_state::DisplayRowExtendFace::new(
                        value.background,
                        face,
                        crate::display_row::metrics::DisplayRowMeasuredFaceMetrics::new(
                            face_metrics.char_width_px(self.geometry.metrics.char_width()),
                            face_height,
                            face_metrics.ascent_px(),
                            0.0,
                        ),
                    )
                });
            // Visible buffer layout installs the active face's minimum
            // extents before emitting an item. Preserve that descent even
            // when every glyph in the item is raised above the baseline.
            let minimum = self.measurements.face_vertical_metrics_px(face).unwrap_or(
                DisplayRowVerticalMetrics::new(face_height, face_metrics.ascent_px()),
            );
            let mut item_layout = layout.clone();
            item_layout.height_px = row.height_px.max(minimum.height_px());
            item_layout.ascent_px = row.ascent_px.max(minimum.ascent_px());
            let render_item =
                crate::display_row::render_item::DisplayRowRenderItem::from_source_item(item);
            let can_split = can_wrap
                && matches!(render_item.source_item().kind, DisplayItemKind::TextRun(_))
                && !preserves_buffer_anchor(&render_item.source_item().span.start);
            let checkpoint = DisplayRowGlyphCheckpoint::capture(&row);
            let previous_slots = slots.len();
            let insertion = matches!(
                original.span.start,
                DisplaySourcePosition::LispString { .. }
            );
            if insertion
                && operation_index.checked_sub(1).is_none_or(|previous| {
                    !matches!(
                        operations[previous].0.span.start,
                        DisplaySourcePosition::LispString { .. }
                    )
                })
                && let DisplayItemKind::TextRun(run) = &original.kind
                && let Some(first) = run.text.chars().next()
                && let DisplaySourcePosition::Buffer {
                    char_pos, byte_pos, ..
                } = buffer_anchors[operation_index]
            {
                word_wrap.record_candidate_at(
                    first,
                    crate::display_source::DisplaySourceTextPosition::new(
                        byte_pos.get(),
                        char_pos.get() as i64,
                    ),
                    previous_slots,
                    (None, None),
                    checkpoint,
                    position,
                    predecessor_extend,
                );
            }
            let progress = DisplayRowProgressWriter::with_text_run_measurement_and_glyph_measurer_for_area_and_start_policy(
                &item_layout, &mut row, plan, &mut self.measurements, position, self.geometry.width,
                DisplayRowTextAreaOrigin::row_local(), GlyphArea::Text, DisplayRowAppendStartPolicy::ReconcileWithRowTail,
            ).with_buffer_tab_admission().push_item(render_item.row_item_for_write());
            if self.measurements.missing {
                return Err(RowProgramError::MissingMeasurement);
            }
            if progress.status() == DisplayRowAppendStatus::Clipped
                && !scalar_operations.contains(&operation_index)
                && matches!(&render_item.source_item().kind, DisplayItemKind::TextRun(run)
                    if !run.text.is_ascii() && run.composition == DisplayTextComposition::Independent)
            {
                // Canonical buffer production falls back to scalar items for
                // the rest of an overflowing run on this visual row.
                checkpoint.restore(&mut row);
                scalar_operations.insert(operation_index);
                continue;
            }
            if word_wrap.is_enabled()
                && !insertion
                && let DisplayItemKind::TextRun(run) = &render_item.source_item().kind
            {
                crate::display_row::word_wrap::record_text_progress(
                    &run.text,
                    crate::display_source::DisplaySourceTextOrigin::new(0),
                    &mut word_wrap,
                    previous_slots,
                    (None, None),
                    checkpoint,
                    DisplayRowGlyphCheckpoint::capture(&row),
                    &progress,
                    predecessor_extend,
                    active_extend,
                );
                // The canonical scalar overflow path records the next word
                // boundary even when its first glyph did not fit this row.
                if progress.status() == DisplayRowAppendStatus::Clipped
                    && let Some(ch) = run.text.chars().nth(progress.slots().len())
                    && let DisplaySourcePosition::Buffer {
                        char_pos, byte_pos, ..
                    } = render_item.source_item().span.start.advanced_by(
                        progress.slots().len(),
                        text_byte_offset(render_item.source_item(), progress.slots().len())?,
                    )
                {
                    word_wrap.record_candidate_at(
                        ch,
                        crate::display_source::DisplaySourceTextPosition::new(
                            byte_pos.get(),
                            char_pos.get() as i64,
                        ),
                        previous_slots + progress.slots().len(),
                        (None, None),
                        DisplayRowGlyphCheckpoint::capture(&row),
                        progress.end(),
                        if progress.slots().is_empty() {
                            predecessor_extend
                        } else {
                            active_extend
                        },
                    );
                }
            }
            if insertion
                && operations.get(operation_index + 1).is_none_or(|(next, _)| {
                    !matches!(next.span.start, DisplaySourcePosition::LispString { .. })
                })
                && let DisplayItemKind::TextRun(run) = &original.kind
                && let Some(last) = run.text.chars().last()
            {
                word_wrap.allow_after_current_char(last);
            }
            position = progress.end();
            if let DisplayItemKind::SourceMappedText(mapped) = &render_item.source_item().kind
                && let Some(measurement_face) = mapped.measurement_face
            {
                // A pushed display string emits one buffer hit span, even
                // when its own face changes across several measured items.
                let source = render_item.source_item().span.start.clone();
                let start = if slots
                    .last()
                    .is_some_and(|slot: &DisplayRowGlyphSlot| slot.source() == source)
                {
                    slot_heights.pop();
                    slots.pop().unwrap().start_position()
                } else {
                    progress.start()
                };
                slots.push(DisplayRowGlyphSlot::with_pointer_appearance(
                    source,
                    start.x_px(),
                    start.col(),
                    progress.end().x_px() - start.x_px(),
                    progress.end().col().saturating_sub(start.col()),
                    None,
                ));
                slot_heights.push(
                    self.faces
                        .iter()
                        .find(|face| face.face_id == measurement_face)
                        .ok_or(RowProgramError::Unsupported)?
                        .metrics
                        .line_height_px(),
                );
            } else {
                slot_heights.extend(
                    progress.slots().iter().map(|slot| {
                        slot.cell_height(face_height, self.geometry.metrics.row_height())
                    }),
                );
                slots.extend(progress.slots().iter().cloned());
            }
            let row_glyphs = row.glyphs.iter().map(Vec::len).sum::<usize>();
            if row_glyphs > 256 || completed_glyphs.saturating_add(row_glyphs) > self.limits.glyphs
            {
                return Err(RowProgramError::Budget);
            }
            if progress.status() == DisplayRowAppendStatus::Clipped {
                if !can_split || slots.is_empty() {
                    return Err(RowProgramError::Overflow);
                }
                let mut wrap_extend = active_extend;
                let (next_index, next_offset) = if word_wrap.has_candidate() {
                    let candidate = word_wrap.candidate();
                    wrap_extend = candidate.row_extend();
                    let boundary = candidate.source_position().charpos();
                    let (index, offset) = operations
                        .iter()
                        .enumerate()
                        .find_map(|(index, (item, _))| {
                            if matches!(item.span.start, DisplaySourcePosition::LispString { .. })
                                && let DisplaySourcePosition::Buffer { char_pos, .. } =
                                    buffer_anchors[index]
                                && char_pos.get() as i64 == boundary
                            {
                                return Some((index, 0));
                            }
                            let (
                                DisplaySourcePosition::Buffer {
                                    char_pos: start, ..
                                },
                                DisplaySourcePosition::Buffer { char_pos: end, .. },
                            ) = (&item.span.start, &item.span.end)
                            else {
                                return None;
                            };
                            ((start.get() as i64) <= boundary && boundary < end.get() as i64)
                                .then_some((index, (boundary - start.get() as i64) as usize))
                        })
                        .ok_or(RowProgramError::Unsupported)?;
                    if candidate.display_point_count() == 0 {
                        return Err(RowProgramError::Overflow);
                    }
                    candidate.glyph_checkpoint().restore(&mut row);
                    slots.truncate(candidate.display_point_count());
                    slot_heights.truncate(candidate.display_point_count());
                    position = candidate.row_position();
                    (index, offset)
                } else {
                    (operation_index, operation_offset + progress.slots().len())
                };
                let next_source = buffer_anchors[next_index].advanced_by(
                    next_offset,
                    text_byte_offset(&operations[next_index].0, next_offset)?,
                );
                row.continued = true;
                continuation::extend_background(
                    &mut row,
                    wrap_extend,
                    position,
                    &self.geometry,
                    self.measurements.backend,
                );
                let row_glyphs = row.glyphs.iter().map(Vec::len).sum::<usize>();
                if row_glyphs > 256
                    || completed_glyphs.saturating_add(row_glyphs) > self.limits.glyphs
                {
                    return Err(RowProgramError::Budget);
                }
                crate::glyph_row_writer::normalize_external_row(&mut row);
                completed_glyphs += row.glyphs.iter().map(Vec::len).sum::<usize>();
                rows.push(ComputedRow {
                    row,
                    slots: std::mem::take(&mut slots),
                    slot_heights: std::mem::take(&mut slot_heights),
                    source: SourceSpan::new(
                        source_start.take().ok_or(RowProgramError::Incomplete)?,
                        next_source.clone(),
                    ),
                    end: position,
                    end_kind: ComputedRowEnd::Continuation,
                    fringe: self.geometry.fringe.clone(),
                    continuation: !rows.is_empty(),
                    terminator_width,
                    terminator_height,
                });
                source_start = Some(next_source);
                if rows.len() == row_limit {
                    return Ok(rows);
                }
                physical_line_tabs.continue_after_visual_row(position.x_px());
                row = new_display_row(&layout);
                position = DisplayRowPosition::new(0.0, 0)
                    .with_tab_coordinates(physical_line_tabs.coordinates());
                scalar_operations.clear();
                operation_index = next_index;
                operation_offset = next_offset;
                word_wrap.reset_after_row_transition();
                predecessor_extend = None;
                continue;
            }
            predecessor_extend = active_extend;
            operation_index += 1;
            operation_offset = 0;
            if let Some((value, face, edges, membership)) = newline {
                if let Some(realized) = self
                    .faces
                    .iter()
                    .find(|candidate| candidate.face_id == face)
                {
                    terminator_width = realized
                        .metrics
                        .char_width_px(self.geometry.metrics.char_width());
                    terminator_height = realized.metrics.line_height_px();
                }
                let mode = match self.measurements.backend {
                    DisplayGeometryBackend::WindowSystemPixels => {
                        DisplayRowMeasurementMode::ConcreteFont
                    }
                    DisplayGeometryBackend::TerminalCells => {
                        DisplayRowMeasurementMode::LogicalCells
                    }
                };
                DisplayRowLineEndFinalizer::new(
                    value,
                    face,
                    self.geometry.width - position.x_px(),
                    self.geometry.metrics,
                    self.geometry.background,
                    mode,
                    edges,
                    membership,
                )
                .finalize(&mut row, &self.faces);
                if value.line_height != DisplayLineHeightPolicy::Default {
                    let resolved = crate::display_row::metrics::resolve_line_height(
                        value.line_height,
                        (row.height_px, row.ascent_px),
                        (face_metrics.line_height_px(), face_metrics.ascent_px()),
                        (
                            self.geometry.metrics.row_height(),
                            self.geometry.metrics.ascent(),
                        ),
                    );
                    terminator_height = resolved.newline_height;
                }
                // As in the canonical row transition, line spacing contributes
                // to logical descent after newline fills and hit cells are set.
                if mode.uses_concrete_font_geometry()
                    && value.line_height != DisplayLineHeightPolicy::ContentOnly
                {
                    row.line_spacing_px =
                        crate::display_row::spacing::ResolvedLineSpacing::from_pixels(
                            value
                                .line_spacing
                                .resolve(self.geometry.metrics.row_height(), 0.0),
                        )
                        .pixels();
                    row.height_px += row.line_spacing_px;
                }
            }
            let row_glyphs = row.glyphs.iter().map(Vec::len).sum::<usize>();
            if row_glyphs > 256 || completed_glyphs.saturating_add(row_glyphs) > self.limits.glyphs
            {
                return Err(RowProgramError::Budget);
            }
        }
        if !physical_complete {
            return if rows.is_empty() {
                Err(RowProgramError::Incomplete)
            } else {
                Ok(rows)
            };
        }
        crate::glyph_row_writer::normalize_external_row(&mut row);
        rows.push(ComputedRow {
            row,
            slots,
            slot_heights,
            source: SourceSpan::new(
                source_start.ok_or(RowProgramError::Incomplete)?,
                source_end.ok_or(RowProgramError::Incomplete)?,
            ),
            end: position,
            end_kind: ComputedRowEnd::Newline,
            fringe: self.geometry.fringe.clone(),
            continuation: !rows.is_empty(),
            terminator_width,
            terminator_height,
        });
        Ok(rows)
    }
}

fn item_measurement_face(item: &DisplayItem, paint_face: FaceId) -> FaceId {
    match &item.kind {
        DisplayItemKind::SourceMappedText(text) => text.measurement_face.unwrap_or(paint_face),
        _ => paint_face,
    }
}

fn text_byte_offset(item: &DisplayItem, chars: usize) -> Result<usize, RowProgramError> {
    if chars == 0 {
        return Ok(0);
    }
    let DisplayItemKind::TextRun(run) = &item.kind else {
        return Err(RowProgramError::Unsupported);
    };
    run.text
        .char_indices()
        .map(|(byte, _)| byte)
        .chain(std::iter::once(run.text.len()))
        .nth(chars)
        .ok_or(RowProgramError::Unsupported)
}

fn measurement_suffix(
    plan: &DisplayTextRunMeasurement,
    consumed: usize,
    consumed_bytes: usize,
) -> DisplayTextRunMeasurement {
    match plan {
        DisplayTextRunMeasurement::PerChar => DisplayTextRunMeasurement::PerChar,
        DisplayTextRunMeasurement::Measured(advances) => DisplayTextRunMeasurement::Measured(
            advances
                .iter()
                .filter_map(|advance| {
                    let mut advance = advance.clone();
                    advance.char_offset = advance.char_offset.checked_sub(consumed)?;
                    advance.byte_offset = advance.byte_offset.checked_sub(consumed_bytes)?;
                    Some(advance)
                })
                .collect(),
        ),
    }
}

#[cfg(test)]
#[path = "tests/program_test.rs"]
mod tests;
