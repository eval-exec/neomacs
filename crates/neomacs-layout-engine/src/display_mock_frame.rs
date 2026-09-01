use crate::display_frame_output::FrameOutputIdentity;
use crate::display_item::{
    DisplayItem, DisplayItemKind, DisplayTextRun, RenderFaceRef, SourceSpan,
};
use crate::display_row::builder::{DisplayRowPosition, DisplayTabPolicy, new_display_row};
use crate::display_row::face_state::DisplayRowMeasurementMode;
use crate::display_row::geometry::{DisplayRowGeometry, DisplayRowMaxX};
use crate::display_row::render_state::DisplayRowRenderBounds;
use crate::display_row::source_state::DisplayRowSourceState;
use crate::display_row::{DisplayRowRenderExecutor, DisplayRowSourceFragmentFrame};
use crate::display_source::{DisplayItemSource, DisplaySourceContext};
use crate::display_text_output_install::install_display_row;
use crate::font::metrics::FontMetricsService;
use crate::frame_face_arena::{FrameFaceArena, FrameFaceAttempt};
use crate::mock_frame::{MockDisplayProperty, MockFrameContent, MockStyledLine};
use crate::neovm_bridge::{FaceResolver, ResolvedFace};
use crate::output::builder::DisplayOutputBuilder;
use crate::window_output::{
    TextWindowOutputBegin, TextWindowOutputTarget, begin_text_window_output,
    close_text_window_output,
};
use neomacs_display_protocol::face::{Face, FaceAttributes};
use neomacs_display_protocol::frame_glyphs::GlyphRowRole;
use neomacs_display_protocol::glyph_matrix::{FrameDisplayState, GlyphArea, GlyphRow};
use neomacs_display_protocol::types::Color;
use neomacs_display_protocol::types::FaceId;
use neovm_core::face::FaceTable;

fn install_mock_display_row(builder: &mut DisplayOutputBuilder, row_index: usize, row: &GlyphRow) {
    install_display_row(builder, row_index, row);
}

const MOCK_DISPLAY_SOURCE_ID: u64 = 0x6d6f_636b;

pub(crate) fn protocol_color_to_pixel(color: Color) -> u32 {
    let color = color.linear_to_srgb();
    let channel = |component: f32| -> u32 { (component.clamp(0.0, 1.0) * 255.0).round() as u32 };
    (channel(color.r) << 16) | (channel(color.g) << 8) | channel(color.b)
}

pub(crate) fn resolved_mock_face(
    face: Option<&Face>,
    char_w: f32,
    char_h: f32,
    ascent: f32,
) -> ResolvedFace {
    let mut resolved = ResolvedFace::default();
    if let Some(face) = face {
        resolved.set_display_face_id(face.id);
        resolved.fg = protocol_color_to_pixel(face.foreground);
        resolved.bg = protocol_color_to_pixel(face.background);
        resolved.use_default_foreground = face.use_default_foreground;
        resolved.use_default_background = face.use_default_background;
        resolved.font_family = face.font_family.clone();
        resolved.font_weight = face.font_weight;
        resolved.italic = face.attributes.contains(FaceAttributes::ITALIC);
        let sizing = crate::font::sizing::FontSizing::native_gui();
        resolved.font_size =
            crate::font::sizing::points_to_layout_pixels(face.font_size, sizing.layout_dpi());
        resolved.underline_style = face.underline_style.gnu_code();
        resolved.underline_color = face
            .underline_color
            .map(protocol_color_to_pixel)
            .unwrap_or(0);
        resolved.strike_through = face.attributes.contains(FaceAttributes::STRIKE_THROUGH);
        resolved.strike_through_color = face
            .strike_through_color
            .map(protocol_color_to_pixel)
            .unwrap_or(0);
        resolved.overline = face.attributes.contains(FaceAttributes::OVERLINE);
        resolved.overline_color = face
            .overline_color
            .map(protocol_color_to_pixel)
            .unwrap_or(0);
        resolved.box_type = face.box_type.gnu_code();
        resolved.box_color = face.box_color.map(protocol_color_to_pixel).unwrap_or(0);
        resolved.box_line_width = face.box_line_width;
        resolved.font_ascent = face.font_ascent as f32;
        resolved.font_line_height = (face.font_ascent + face.font_descent).max(0) as f32;
    }
    resolved.set_measured_char_width_px(char_w.max(1.0));
    if resolved.font_ascent <= 0.0 {
        resolved.font_ascent = ascent.max(0.0).min(char_h.max(1.0));
    }
    if resolved.font_line_height <= 0.0 {
        resolved.font_line_height = char_h.max(1.0);
    }
    resolved
}

fn mock_display_row_geometry(
    pixel_y: f32,
    width_px: f32,
    char_w: f32,
    char_h: f32,
    ascent: f32,
) -> DisplayRowGeometry {
    DisplayRowGeometry::new(
        pixel_y,
        width_px.max(1.0),
        char_h.max(1.0),
        char_w.max(1.0),
        ascent.max(0.0).min(char_h.max(1.0)),
        DisplayTabPolicy::every(8),
    )
}

fn new_empty_mock_display_row(
    role: GlyphRowRole,
    geometry: &DisplayRowGeometry,
    base_face: &ResolvedFace,
) -> GlyphRow {
    new_display_row(&geometry.to_layout(
        role,
        geometry.char_width().max(1.0),
        geometry.ascent().max(0.0).min(geometry.height().max(1.0)),
        RenderFaceRef::FaceId(base_face.display_face_id()),
        crate::display_pixel_calc::PixelCalcContext::for_chrome_row(
            geometry.width(),
            geometry.char_width().max(1.0),
            geometry.height(),
            std::collections::HashMap::new(),
        ),
        None,
    ))
}

fn mock_display_text_item(
    text: impl Into<Box<str>>,
    face_id: FaceId,
    source_offset: usize,
) -> DisplayItem {
    let text = text.into();
    let char_len = text.chars().count();
    DisplayItem::new(
        SourceSpan::synthetic(
            MOCK_DISPLAY_SOURCE_ID,
            source_offset,
            source_offset.saturating_add(char_len),
        ),
        RenderFaceRef::FaceId(face_id),
        DisplayItemKind::TextRun(DisplayTextRun::new(text)),
    )
}

struct MockDisplayItemSource {
    items: std::vec::IntoIter<DisplayItem>,
}

impl MockDisplayItemSource {
    fn from_text(text: impl Into<Box<str>>, face_id: FaceId) -> Self {
        let mut builder = MockDisplayItemSourceBuilder::default();
        builder.push_text(text, face_id);
        builder.finish()
    }

    fn from_line(line: &MockStyledLine, fill_to_cols: Option<(usize, FaceId)>) -> Self {
        let mut builder = MockDisplayItemSourceBuilder::default();
        let mut visible_cols = 0usize;
        for glyph in &line.glyphs {
            match &glyph.display {
                Some(MockDisplayProperty::Invisible) => {
                    builder.skip_chars(1);
                }
                Some(MockDisplayProperty::Replace(text, face_id)) => {
                    let char_len = text.chars().count();
                    builder.push_text(text.clone(), *face_id);
                    visible_cols = visible_cols.saturating_add(char_len);
                }
                Some(MockDisplayProperty::Composition(composed)) => {
                    for composed_glyph in composed {
                        builder.push_text(composed_glyph.ch.to_string(), composed_glyph.face_id);
                        visible_cols = visible_cols.saturating_add(1);
                    }
                }
                None => {
                    builder.push_text(glyph.ch.to_string(), glyph.face_id);
                    visible_cols = visible_cols.saturating_add(1);
                }
            }
        }
        if let Some((target_cols, face_id)) = fill_to_cols
            && visible_cols < target_cols
        {
            builder.push_text(" ".repeat(target_cols - visible_cols), face_id);
        }
        builder.finish()
    }
}

impl DisplayItemSource for MockDisplayItemSource {
    fn next_item(&mut self, _context: &mut DisplaySourceContext<'_>) -> Option<DisplayItem> {
        self.items.next()
    }
}

#[derive(Default)]
struct MockDisplayItemSourceBuilder {
    source_offset: usize,
    items: Vec<DisplayItem>,
}

impl MockDisplayItemSourceBuilder {
    fn push_text(&mut self, text: impl Into<Box<str>>, face_id: FaceId) {
        let text = text.into();
        let char_len = text.chars().count();
        if char_len == 0 {
            return;
        }
        self.items
            .push(mock_display_text_item(text, face_id, self.source_offset));
        self.source_offset = self.source_offset.saturating_add(char_len);
    }

    fn skip_chars(&mut self, char_len: usize) {
        self.source_offset = self.source_offset.saturating_add(char_len);
    }

    fn finish(self) -> MockDisplayItemSource {
        MockDisplayItemSource {
            items: self.items.into_iter(),
        }
    }
}

struct MockDisplayAreaRenderRequest<'a> {
    role: GlyphRowRole,
    area: GlyphArea,
    source: MockDisplayItemSource,
    row: &'a mut GlyphRow,
    geometry: DisplayRowGeometry,
    base_face: &'a ResolvedFace,
    face_resolver: &'a FaceResolver,
    font_metrics: &'a mut Option<FontMetricsService>,
    face_ids: &'a mut FrameFaceAttempt,
}

fn render_mock_display_area(request: MockDisplayAreaRenderRequest<'_>) {
    let MockDisplayAreaRenderRequest {
        role,
        area,
        mut source,
        row,
        geometry,
        base_face,
        face_resolver,
        font_metrics,
        face_ids,
    } = request;
    let render_width = geometry.width();
    let mut source_state = DisplayRowSourceState::frame_local();
    let row_request =
        DisplayRowSourceFragmentFrame::new(geometry, role, base_face.display_face_id(), base_face)
            .render_request_for_area(
                DisplayRowRenderBounds::new(
                    DisplayRowPosition::new(0.0, 0),
                    DisplayRowMaxX::Bounded(render_width),
                ),
                area,
            );
    let mut executor = DisplayRowRenderExecutor::new(
        font_metrics,
        DisplayRowMeasurementMode::LogicalCells,
        face_resolver,
        None,
        face_ids,
    );
    executor.render_item_source_fragment_into_row(row_request, row, &mut source, &mut source_state);
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn mock_display_row_from_line(
    role: GlyphRowRole,
    line: &MockStyledLine,
    pixel_y: f32,
    width_px: f32,
    char_w: f32,
    char_h: f32,
    ascent: f32,
    left_margin: Option<&str>,
    fill_to_cols: Option<(usize, FaceId)>,
    base_face: &ResolvedFace,
    face_resolver: &FaceResolver,
    font_metrics: &mut Option<FontMetricsService>,
    face_ids: &mut FrameFaceAttempt,
) -> GlyphRow {
    let geometry = mock_display_row_geometry(pixel_y, width_px, char_w, char_h, ascent);
    let mut row = new_empty_mock_display_row(role, &geometry, base_face);
    if let Some(left_margin) = left_margin {
        render_mock_display_area(MockDisplayAreaRenderRequest {
            role,
            area: GlyphArea::LeftMargin,
            source: MockDisplayItemSource::from_text(left_margin.to_owned(), FaceId::new(2)),
            row: &mut row,
            geometry: geometry.clone(),
            base_face,
            face_resolver,
            font_metrics,
            face_ids,
        });
    }
    render_mock_display_area(MockDisplayAreaRenderRequest {
        role,
        area: GlyphArea::Text,
        source: MockDisplayItemSource::from_line(line, fill_to_cols),
        row: &mut row,
        geometry,
        base_face,
        face_resolver,
        font_metrics,
        face_ids,
    });
    crate::glyph_row_writer::normalize_external_row(&mut row);
    row
}

/// Convert a pixel width to a monospace column count, mirroring the `ncols`
/// estimates used by `begin_text_window_output` for mock frames.
pub(crate) fn mock_frame_pixel_width_to_columns(width_px: f32, char_w: f32) -> usize {
    (width_px / char_w.max(1.0)) as usize
}

fn set_mock_frame_identity(builder: &mut DisplayOutputBuilder, identity: FrameOutputIdentity) {
    builder.set_output_frame_identity(
        identity.frame_id,
        identity.parent_id,
        identity.parent_x,
        identity.parent_y,
        identity.z_order,
        identity.undecorated,
        identity.border_width,
        identity.border_color,
        identity.outer_border_width,
        identity.outer_border_color,
        identity.background_alpha,
        identity.no_accept_focus,
    );
}

/// Layout mock-display frame content into frame snapshots using the shared
/// typed row renderer for all mock text, mode-line, minibuffer, and child rows.
pub(crate) fn layout_mock_frame_content(
    content: &MockFrameContent,
    char_w: f32,
    char_h: f32,
    font_metrics: &mut Option<FontMetricsService>,
) -> Vec<FrameDisplayState> {
    let mut builder = DisplayOutputBuilder::new();

    set_mock_frame_identity(
        &mut builder,
        FrameOutputIdentity {
            frame_id: content.frame_id,
            parent_id: 0,
            parent_x: 0.0,
            parent_y: 0.0,
            z_order: 0,
            undecorated: false,
            border_width: 0.0,
            border_color: Color::BLACK,
            outer_border_width: 0.0,
            outer_border_color: Color::BLACK,
            background_alpha: 1.0,
            no_accept_focus: false,
        },
    );
    builder.set_output_background_color(content.background);

    for face in &content.faces {
        let mut face = face.clone();
        // Mock display faces enter in Emacs point units; frame output carries
        // physical pixels to match the measured row geometry.
        let sizing = crate::font::sizing::FontSizing::native_gui();
        face.font_size =
            crate::font::sizing::points_to_layout_pixels(face.font_size, sizing.layout_dpi());
        builder.publish_output_face(face.id, face);
    }

    let default_face = content.faces.first();
    let sizing = crate::font::sizing::FontSizing::native_gui();
    let default_size = crate::font::sizing::points_to_layout_pixels(
        default_face.map(|f| f.font_size).unwrap_or(12.0),
        sizing.layout_dpi(),
    );
    let default_family = default_face
        .map(|f| f.font_family.as_str())
        .unwrap_or("monospace");
    let default_weight = default_face.map(|f| f.font_weight).unwrap_or(400);
    let default_italic = default_face
        .map(|f| f.attributes.contains(FaceAttributes::ITALIC))
        .unwrap_or(false);

    let ascent = font_metrics
        .as_mut()
        .map(|fm| {
            fm.font_metrics(default_family, default_weight, default_italic, default_size)
                .ascent
        })
        .unwrap_or(char_h * 0.8);
    let mock_face_table = FaceTable::new();
    let mock_face_resolver = FaceResolver::new(
        &mock_face_table,
        default_face
            .map(|face| protocol_color_to_pixel(face.foreground))
            .unwrap_or(0x00ffffff),
        default_face
            .map(|face| protocol_color_to_pixel(face.background))
            .unwrap_or(0x00000000),
        default_size,
        None,
    );
    let mock_base_face = resolved_mock_face(default_face, char_w, char_h, ascent);
    let mut mock_face_ids = FrameFaceArena::default().begin_attempt();
    tracing::info!(
        "layout_mock_frame: default_size={:.1} family={} weight={} italic={} char_w={:.1} char_h={:.1}",
        default_size,
        default_family,
        default_weight,
        default_italic,
        char_w,
        char_h
    );

    for window in &content.windows {
        let nrows = window.lines.len() + 1;
        let ncols = mock_frame_pixel_width_to_columns(window.pixel_bounds.width, char_w);
        begin_text_window_output(
            TextWindowOutputTarget::from_builder(&mut builder),
            TextWindowOutputBegin {
                window_id: window.window_id,
                rows: nrows,
                cols: ncols,
                bounds: window.pixel_bounds,
                text_bounds: window.pixel_bounds,
                text_clip_bounds: window.pixel_bounds,
                selected: window.selected,
            },
        );
        for (row_idx, line) in window.lines.iter().enumerate() {
            let row_y = window.pixel_bounds.y + row_idx as f32 * char_h;
            let lnum = format!("{:>3} ", row_idx + 1);
            let row = mock_display_row_from_line(
                GlyphRowRole::Text,
                line,
                row_y,
                window.pixel_bounds.width,
                char_w,
                char_h,
                ascent,
                Some(&lnum),
                None,
                &mock_base_face,
                &mock_face_resolver,
                font_metrics,
                &mut mock_face_ids,
            );
            install_mock_display_row(&mut builder, row_idx, &row);
        }

        let mode_line_row = window.lines.len();
        let ml_ncols = mock_frame_pixel_width_to_columns(window.pixel_bounds.width, char_w);
        let row = mock_display_row_from_line(
            GlyphRowRole::ModeLine,
            &window.mode_line,
            window.pixel_bounds.y + window.pixel_bounds.height - char_h,
            window.pixel_bounds.width,
            char_w,
            char_h,
            ascent,
            None,
            Some((ml_ncols, FaceId::new(1))),
            &mock_base_face,
            &mock_face_resolver,
            font_metrics,
            &mut mock_face_ids,
        );
        install_mock_display_row(&mut builder, mode_line_row, &row);

        close_text_window_output(TextWindowOutputTarget::from_builder(&mut builder));
    }

    if let Some(ref mini) = content.minibuffer {
        let has_mode_line = !mini.mode_line.glyphs.is_empty();
        let nrows = mini.lines.len() + usize::from(has_mode_line);
        let ncols = mock_frame_pixel_width_to_columns(mini.pixel_bounds.width, char_w);
        begin_text_window_output(
            TextWindowOutputTarget::from_builder(&mut builder),
            TextWindowOutputBegin {
                window_id: mini.window_id,
                rows: nrows,
                cols: ncols,
                bounds: mini.pixel_bounds,
                text_bounds: mini.pixel_bounds,
                text_clip_bounds: mini.pixel_bounds,
                selected: mini.selected,
            },
        );

        for (row_idx, line) in mini.lines.iter().enumerate() {
            let row_y = mini.pixel_bounds.y + row_idx as f32 * char_h;
            let row = mock_display_row_from_line(
                GlyphRowRole::Minibuffer,
                line,
                row_y,
                mini.pixel_bounds.width,
                char_w,
                char_h,
                ascent,
                None,
                None,
                &mock_base_face,
                &mock_face_resolver,
                font_metrics,
                &mut mock_face_ids,
            );
            install_mock_display_row(&mut builder, row_idx, &row);
        }

        if has_mode_line {
            let mode_line_row = mini.lines.len();
            let mini_ncols = mock_frame_pixel_width_to_columns(mini.pixel_bounds.width, char_w);
            let row = mock_display_row_from_line(
                GlyphRowRole::ModeLine,
                &mini.mode_line,
                mini.pixel_bounds.y + mini.pixel_bounds.height - char_h,
                mini.pixel_bounds.width,
                char_w,
                char_h,
                ascent,
                None,
                Some((mini_ncols, FaceId::new(1))),
                &mock_base_face,
                &mock_face_resolver,
                font_metrics,
                &mut mock_face_ids,
            );
            install_mock_display_row(&mut builder, mode_line_row, &row);
        }

        close_text_window_output(TextWindowOutputTarget::from_builder(&mut builder));
    }

    let main_state = builder.finish(
        mock_frame_pixel_width_to_columns(content.frame_pixel_width, char_w),
        (content.frame_pixel_height / char_h.max(1.0)) as usize,
        char_w,
        char_h,
    );

    let mut child_frames = Vec::new();
    for cf in &content.child_frames {
        let mut cb = DisplayOutputBuilder::new();
        set_mock_frame_identity(
            &mut cb,
            FrameOutputIdentity {
                frame_id: cf.frame_id,
                parent_id: content.frame_id,
                parent_x: cf.parent_x,
                parent_y: cf.parent_y,
                z_order: cf.z_order,
                undecorated: true,
                border_width: 0.0,
                border_color: Color::BLACK,
                outer_border_width: 0.0,
                outer_border_color: Color::BLACK,
                background_alpha: 1.0,
                no_accept_focus: false,
            },
        );
        cb.set_output_background_color(Color::new(0.0, 0.0, 0.0, 0.0));
        for face in &content.faces {
            cb.publish_output_face(face.id, face.clone());
        }
        let nrows = cf.window.lines.len();
        let ncols = mock_frame_pixel_width_to_columns(cf.window.pixel_bounds.width, char_w);
        begin_text_window_output(
            TextWindowOutputTarget::from_builder(&mut cb),
            TextWindowOutputBegin {
                window_id: cf.window.window_id,
                rows: nrows,
                cols: ncols,
                bounds: cf.window.pixel_bounds,
                text_bounds: cf.window.pixel_bounds,
                text_clip_bounds: cf.window.pixel_bounds,
                selected: false,
            },
        );
        for (ri, line) in cf.window.lines.iter().enumerate() {
            let row = mock_display_row_from_line(
                GlyphRowRole::Text,
                line,
                cf.window.pixel_bounds.y + ri as f32 * char_h,
                cf.window.pixel_bounds.width,
                char_w,
                char_h,
                ascent,
                None,
                None,
                &mock_base_face,
                &mock_face_resolver,
                font_metrics,
                &mut mock_face_ids,
            );
            install_mock_display_row(&mut cb, ri, &row);
        }
        close_text_window_output(TextWindowOutputTarget::from_builder(&mut cb));
        let cs = cb.finish(
            mock_frame_pixel_width_to_columns(cf.window.pixel_bounds.width, char_w),
            cf.window.lines.len().max(1),
            char_w,
            char_h,
        );
        child_frames.push(cs);
    }

    let mut all = vec![main_state];
    all.extend(child_frames);
    all
}
