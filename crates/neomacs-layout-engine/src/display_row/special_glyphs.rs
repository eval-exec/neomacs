//! Right-edge truncation/continuation markers and the right window border — GNU's special glyphs (`produce_special_glyphs`, xdisp.c; `IT_TRUNCATION`/`IT_CONTINUATION`). Relocated out of display_row_append.rs (pure move, no behavior change).

use crate::display_face_policy::EffectiveWindowDefaultFace;
use crate::display_item::{
    DisplayItem, DisplayItemKind, DisplayTextRun, RenderFaceRef, SourceSpan,
};
use crate::display_row::builder::{
    DisplayRowColumnCount, pop_display_row_trailing_text_char,
    trim_display_row_text_to_total_columns,
};
use crate::display_row::geometry::{DisplayRowFlagKind, DisplayRowFlags};
use crate::display_row::source_state::DisplayRowSourceState;
use crate::display_source::{DisplayItemSource, DisplaySourceContext, SyntheticTextItemSource};
use crate::display_status_line::ChromeRowRenderServices;
use crate::display_text_output_install::install_output_resolved_face;
use crate::neovm_bridge::ResolvedFace;
use crate::output::builder::DisplayOutputBuilder;
use crate::output::row_request::{DisplayWindowRowMutation, DisplayWindowRowsMutation};
use neomacs_display_protocol::glyph_matrix::{GlyphArea, GlyphRow};
use neomacs_display_protocol::types::FaceId;

const RIGHT_EDGE_MARKER_SOURCE_ID: u64 = 0x7265_6467;
const RIGHT_BORDER_SOURCE_ID: u64 = 0x7262_6f72;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextWindowRightEdgeMarkerColumn {
    LastColumn,
    BeforeRightBorder,
}

pub(crate) struct TextWindowRightEdgeMarkers<'a> {
    pub(crate) display_text_row_base: usize,
    pub(crate) output_cols: usize,
    pub(crate) column: TextWindowRightEdgeMarkerColumn,
    pub(crate) row_flags: &'a DisplayRowFlags,
    pub(crate) face_id: FaceId,
    pub(crate) face: &'a ResolvedFace,
    pub(crate) char_width: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TextWindowRightBorder {
    pub(crate) ch: char,
    pub(crate) face_id: FaceId,
    pub(crate) char_width: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TextWindowTerminalRightBorder {
    pub(crate) ch: char,
    pub(crate) face_name: &'static str,
    pub(crate) char_width: f32,
}

impl TextWindowRightEdgeMarkerColumn {
    pub(crate) fn target_col(self, output_cols: usize) -> usize {
        match self {
            Self::LastColumn => output_cols.saturating_sub(1),
            Self::BeforeRightBorder => output_cols.saturating_sub(2),
        }
    }
}

impl<'a> TextWindowRightEdgeMarkers<'a> {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn for_reserved_special_column(
        reserve_right_special_col: bool,
        reserve_right_border_col: bool,
        display_text_row_base: usize,
        output_cols: usize,
        row_flags: &'a DisplayRowFlags,
        face_id: FaceId,
        face: &'a ResolvedFace,
        char_width: f32,
    ) -> Option<Self> {
        reserve_right_special_col.then_some(Self {
            display_text_row_base,
            output_cols,
            column: if reserve_right_border_col {
                TextWindowRightEdgeMarkerColumn::BeforeRightBorder
            } else {
                TextWindowRightEdgeMarkerColumn::LastColumn
            },
            row_flags,
            face_id,
            face,
            char_width,
        })
    }
}

fn prepare_special_glyph_row(
    row: &mut GlyphRow,
    matrix_cols: usize,
    target_col: usize,
) -> Option<usize> {
    if matrix_cols == 0 {
        return None;
    }
    row.enabled = true;
    Some(target_col.min(matrix_cols - 1))
}

fn right_border_text_source(
    text: impl Into<Box<str>>,
    face_id: FaceId,
    start_offset: usize,
) -> SyntheticTextItemSource {
    SyntheticTextItemSource::new(
        RIGHT_BORDER_SOURCE_ID,
        text,
        RenderFaceRef::FaceId(face_id),
        start_offset,
    )
}

struct RightEdgeMarkerItemSource {
    items: std::vec::IntoIter<DisplayItem>,
}

impl RightEdgeMarkerItemSource {
    fn new(padding_cols: usize, marker: char, face_id: FaceId) -> Self {
        let mut source_offset = 0usize;
        let mut items = Vec::with_capacity(usize::from(padding_cols > 0) + 1);
        if padding_cols > 0 {
            items.push(synthetic_special_glyph_text_item(
                RIGHT_EDGE_MARKER_SOURCE_ID,
                " ".repeat(padding_cols),
                face_id,
                source_offset,
            ));
            source_offset = source_offset.saturating_add(padding_cols);
        }
        items.push(synthetic_special_glyph_text_item(
            RIGHT_EDGE_MARKER_SOURCE_ID,
            marker.to_string(),
            face_id,
            source_offset,
        ));
        Self {
            items: items.into_iter(),
        }
    }
}

impl DisplayItemSource for RightEdgeMarkerItemSource {
    fn next_item(&mut self, _context: &mut DisplaySourceContext<'_>) -> Option<DisplayItem> {
        self.items.next()
    }
}

fn synthetic_special_glyph_text_item(
    source_id: u64,
    text: impl Into<Box<str>>,
    face_id: FaceId,
    start_offset: usize,
) -> DisplayItem {
    let text = text.into();
    let end_offset = start_offset.saturating_add(text.chars().count());
    DisplayItem::new(
        SourceSpan::synthetic(source_id, start_offset, end_offset),
        RenderFaceRef::FaceId(face_id),
        DisplayItemKind::TextRun(DisplayTextRun::new(text)),
    )
}

fn render_right_edge_marker_source(
    row: &mut GlyphRow,
    render_services: &mut ChromeRowRenderServices<'_, '_>,
    source: &mut RightEdgeMarkerItemSource,
    face_id: FaceId,
    base_face: &ResolvedFace,
    char_width: f32,
    matrix_cols: usize,
) {
    let start_col = DisplayRowColumnCount::from_row(row, char_width).get();
    let mut source_state = DisplayRowSourceState::frame_local();
    render_services.render_item_source_fragment_from_glyph_row_columns(
        row,
        source,
        &mut source_state,
        matrix_cols,
        char_width,
        face_id,
        base_face,
        start_col,
        matrix_cols,
        GlyphArea::Text,
    );
}

#[allow(clippy::too_many_arguments)]
fn install_right_edge_marker_from_source_request(
    row: &mut GlyphRow,
    target_col: usize,
    marker: char,
    face_id: FaceId,
    base_face: &ResolvedFace,
    char_width: f32,
    matrix_cols: usize,
    render_services: &mut ChromeRowRenderServices<'_, '_>,
) {
    let Some(clamped_col) = prepare_special_glyph_row(row, matrix_cols, target_col) else {
        return;
    };
    trim_display_row_text_to_total_columns(row, clamped_col, char_width);

    let padding_cols =
        clamped_col.saturating_sub(DisplayRowColumnCount::from_row(row, char_width).get());
    let mut source = RightEdgeMarkerItemSource::new(padding_cols, marker, face_id);
    render_right_edge_marker_source(
        row,
        render_services,
        &mut source,
        face_id,
        base_face,
        char_width,
        matrix_cols,
    );
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TextWindowRightEdgeMarkerDecoration {
    pub(crate) display_row_index: usize,
    pub(crate) target_col: usize,
    pub(crate) marker: char,
}

pub(crate) fn text_window_right_edge_marker_decorations(
    request: &TextWindowRightEdgeMarkers<'_>,
) -> Vec<TextWindowRightEdgeMarkerDecoration> {
    let target_col = request.column.target_col(request.output_cols);
    let mut decorations = Vec::new();
    for row_idx in 0..request.row_flags.len() {
        let display_row_index = request.display_text_row_base + row_idx;
        let marker = if request
            .row_flags
            .is_set(row_idx, DisplayRowFlagKind::Truncated)
        {
            Some('$')
        } else if request
            .row_flags
            .is_set(row_idx, DisplayRowFlagKind::ContinuedMidElement)
        {
            Some('\\')
        } else {
            None
        };
        let Some(marker) = marker else {
            continue;
        };
        decorations.push(TextWindowRightEdgeMarkerDecoration {
            display_row_index,
            target_col,
            marker,
        });
    }
    decorations
}

struct TextWindowRightEdgeMarkerMutation<'base, 'services, 'emit, 'face> {
    decoration: TextWindowRightEdgeMarkerDecoration,
    face_id: FaceId,
    char_width: f32,
    base_face: &'base ResolvedFace,
    render_services: &'services mut ChromeRowRenderServices<'emit, 'face>,
}

impl DisplayWindowRowMutation for TextWindowRightEdgeMarkerMutation<'_, '_, '_, '_> {
    type Output = ();

    fn apply(self, row: &mut GlyphRow, matrix_cols: usize) -> Self::Output {
        install_right_edge_marker_from_source_request(
            row,
            self.decoration.target_col,
            self.decoration.marker,
            self.face_id,
            self.base_face,
            self.char_width,
            matrix_cols,
            self.render_services,
        );
    }
}

struct RightBorderTextRenderRequest<'face> {
    text: String,
    area: GlyphArea,
    face_id: FaceId,
    base_face: &'face ResolvedFace,
    char_width: f32,
    matrix_cols: usize,
    source_offset: usize,
    start_col: usize,
}

fn render_right_border_text(
    row: &mut GlyphRow,
    render_services: &mut ChromeRowRenderServices<'_, '_>,
    request: RightBorderTextRenderRequest<'_>,
) {
    if request.text.is_empty() {
        return;
    }
    let mut source = right_border_text_source(request.text, request.face_id, request.source_offset);
    let mut source_state = DisplayRowSourceState::frame_local();
    render_services.render_item_source_fragment_from_glyph_row_columns(
        row,
        &mut source,
        &mut source_state,
        request.matrix_cols,
        request.char_width,
        request.face_id,
        request.base_face,
        request.start_col,
        request.matrix_cols,
        request.area,
    );
}

fn install_right_border_from_source_request(
    row: &mut GlyphRow,
    target_col: usize,
    request: TextWindowRightBorder,
    border_face: &ResolvedFace,
    padding_face: &EffectiveWindowDefaultFace,
    matrix_cols: usize,
    render_services: &mut ChromeRowRenderServices<'_, '_>,
) {
    let Some(target_col) = prepare_special_glyph_row(row, matrix_cols, target_col) else {
        return;
    };
    // Clear any border glyph a previous redisplay pass left in the right margin
    // before re-installing. This install is only reached for the TTY terminal
    // right border (`reserve_terminal_right_border_col`), whose sole right-margin
    // glyph is that border. Without this, a row reused by the cursor-only /
    // scroll / edit fast paths already carries a `|` here; since
    // The row column extent counts right-margin glyphs, so re-running the
    // install adds ANOTHER `|` and mis-sizes the trim/padding, so the border band
    // accretes leftward across passes (the timing-dependent multi-column band).
    row.glyphs[GlyphArea::RightMargin.index()].clear();
    let prior_displays_text = row.displays_text;
    let preserved_trailing = pop_display_row_trailing_text_char(row, '$');
    let preserved_cols = usize::from(preserved_trailing.is_some());
    let before_final_cols = target_col.saturating_sub(preserved_cols);
    trim_display_row_text_to_total_columns(row, before_final_cols, request.char_width);

    let mut source_offset = 0usize;
    let padding_face_id = padding_face.face_id();
    let leading_padding = before_final_cols
        .saturating_sub(DisplayRowColumnCount::from_row(row, request.char_width).get());
    if leading_padding > 0 {
        render_right_border_text(
            row,
            render_services,
            RightBorderTextRenderRequest {
                text: " ".repeat(leading_padding),
                area: GlyphArea::Text,
                face_id: padding_face_id,
                base_face: padding_face.face(),
                char_width: request.char_width,
                matrix_cols,
                source_offset,
                start_col: DisplayRowColumnCount::from_row(row, request.char_width).get(),
            },
        );
        source_offset = source_offset.saturating_add(leading_padding);
    }

    if let Some(glyph) = preserved_trailing {
        render_right_border_text(
            row,
            render_services,
            RightBorderTextRenderRequest {
                text: "$".into(),
                area: GlyphArea::Text,
                face_id: glyph.face_id,
                base_face: padding_face.face(),
                char_width: request.char_width,
                matrix_cols,
                source_offset,
                start_col: DisplayRowColumnCount::from_row(row, request.char_width).get(),
            },
        );
        source_offset = source_offset.saturating_add(preserved_cols);
    }

    let trailing_padding =
        target_col.saturating_sub(DisplayRowColumnCount::from_row(row, request.char_width).get());
    if trailing_padding > 0 {
        render_right_border_text(
            row,
            render_services,
            RightBorderTextRenderRequest {
                text: " ".repeat(trailing_padding),
                area: GlyphArea::Text,
                face_id: padding_face_id,
                base_face: padding_face.face(),
                char_width: request.char_width,
                matrix_cols,
                source_offset,
                start_col: DisplayRowColumnCount::from_row(row, request.char_width).get(),
            },
        );
        source_offset = source_offset.saturating_add(trailing_padding);
    }

    render_right_border_text(
        row,
        render_services,
        RightBorderTextRenderRequest {
            text: request.ch.to_string(),
            area: GlyphArea::RightMargin,
            face_id: request.face_id,
            base_face: border_face,
            char_width: request.char_width,
            matrix_cols,
            source_offset,
            start_col: target_col,
        },
    );
    row.displays_text = prior_displays_text;
}

struct TextWindowRightBorderRowsMutation<'base, 'emit, 'face> {
    render_services: ChromeRowRenderServices<'emit, 'face>,
    request: TextWindowRightBorder,
    border_face: &'base ResolvedFace,
    padding_face: &'base EffectiveWindowDefaultFace,
}

impl DisplayWindowRowsMutation for TextWindowRightBorderRowsMutation<'_, '_, '_> {
    fn apply(&mut self, row: &mut GlyphRow, matrix_cols: usize) {
        install_right_border_from_source_request(
            row,
            matrix_cols.saturating_sub(1),
            self.request,
            self.border_face,
            self.padding_face,
            matrix_cols,
            &mut self.render_services,
        );
    }
}

pub(crate) fn install_text_window_right_edge_markers(
    output_builder: &mut DisplayOutputBuilder,
    mut render_services: ChromeRowRenderServices<'_, '_>,
    request: TextWindowRightEdgeMarkers<'_>,
) {
    for decoration in text_window_right_edge_marker_decorations(&request) {
        let _ = output_builder.apply_current_window_row_mutation(
            decoration.display_row_index,
            TextWindowRightEdgeMarkerMutation {
                decoration,
                face_id: request.face_id,
                char_width: request.char_width,
                base_face: request.face,
                render_services: &mut render_services,
            },
        );
    }
}

pub(crate) fn install_text_window_right_border_rows(
    output_builder: &mut DisplayOutputBuilder,
    render_services: ChromeRowRenderServices<'_, '_>,
    request: TextWindowRightBorder,
    border_face: &ResolvedFace,
    padding_face: &EffectiveWindowDefaultFace,
) {
    output_builder.apply_last_window_rows_mutation(TextWindowRightBorderRowsMutation {
        render_services,
        request,
        border_face,
        padding_face,
    });
}

pub(crate) fn install_text_window_terminal_right_border(
    output_builder: &mut DisplayOutputBuilder,
    request: TextWindowTerminalRightBorder,
    mut render_services: ChromeRowRenderServices<'_, '_>,
    effective_default_face: &EffectiveWindowDefaultFace,
) -> FaceId {
    let border_face = render_services.resolve_frame_named_face(request.face_name);
    // GNU draws every realized face id from the single per-frame face cache
    // counter (`face_cache->used`, xfaces.c `lookup_face`). Allocate the
    // border's id from the frame-scoped allocator rather than a separate
    // `FaceResolver` counter that could collide with it.
    let border_face_id = crate::display_row::face_state::stable_face_id_for_resolved(
        render_services.face_ids(),
        &border_face,
    );
    install_output_resolved_face(output_builder, border_face_id, &border_face, None);
    install_text_window_right_border_rows(
        output_builder,
        render_services.reborrow(),
        TextWindowRightBorder {
            ch: request.ch,
            face_id: border_face_id,
            char_width: request.char_width,
        },
        &border_face,
        effective_default_face,
    );
    border_face_id
}
