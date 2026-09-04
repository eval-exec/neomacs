//! Background collection and overlay-background draw phases of
//! `render_frame_glyphs` (z-order steps 1 and 5).

// `bg` is an `Option<Color>` consumed by an `is_some()` guard above; by the
// `unwrap_or` it is provably `Some`, but the fallback is retained alongside the
// Option typing, so the literal-unwrap lint is allowed module-wide.
#![allow(clippy::unnecessary_literal_unwrap)]

use neomacs_display_protocol::frame_glyphs::{FrameGlyph, MaterializedFaceData};
use neomacs_display_protocol::types::{Color, FaceId};

use super::super::vertex::{RectVertex, RoundedRectVertex};
use super::WgpuRenderer;
use super::frame_pass::{BoxSpanSet, FrameParams, FramePassCtx, collect_frame_box_spans};

impl WgpuRenderer {
    /// Merge adjacent boxed glyphs into spans.
    pub(super) fn collect_box_spans(&self, params: &FrameParams<'_>) -> BoxSpanSet {
        BoxSpanSet {
            spans: collect_frame_box_spans(
                params.frame_glyphs,
                params.faces,
                &params.pointer_override,
            ),
        }
    }

    /// Collect the non-overlay background layer: background gradient, window
    /// backgrounds, stretches, char backgrounds, fringe bitmaps, line
    /// highlight, indent guides, and whitespace dots.
    pub(super) fn collect_non_overlay_backgrounds(
        &self,
        params: &FrameParams<'_>,
        spans: &BoxSpanSet,
    ) -> Vec<RectVertex> {
        let frame_glyphs = params.frame_glyphs;
        let faces = params.faces;
        let box_spans = &spans.spans;
        let has_line_anims = params.has_line_anims;
        let background_gradient = params.background_gradient;
        let logical_w = params.logical_w;
        let logical_h = params.logical_h;
        // --- Collect non-overlay backgrounds ---
        let mut non_overlay_rect_vertices: Vec<RectVertex> = Vec::new();

        // Background gradient (rendered behind everything)
        if let Some((top, bottom)) = background_gradient {
            let top_color = Color::new(top.0, top.1, top.2, 1.0).srgb_to_linear();
            let bot_color = Color::new(bottom.0, bottom.1, bottom.2, 1.0).srgb_to_linear();
            let tc = [top_color.r, top_color.g, top_color.b, top_color.a];
            let bc = [bot_color.r, bot_color.g, bot_color.b, bot_color.a];
            // Two triangles forming a fullscreen quad with gradient
            // Top-left, top-right, bottom-left (triangle 1)
            non_overlay_rect_vertices.push(RectVertex {
                position: [0.0, 0.0],
                color: tc,
            });
            non_overlay_rect_vertices.push(RectVertex {
                position: [logical_w, 0.0],
                color: tc,
            });
            non_overlay_rect_vertices.push(RectVertex {
                position: [0.0, logical_h],
                color: bc,
            });
            // Top-right, bottom-right, bottom-left (triangle 2)
            non_overlay_rect_vertices.push(RectVertex {
                position: [logical_w, 0.0],
                color: tc,
            });
            non_overlay_rect_vertices.push(RectVertex {
                position: [logical_w, logical_h],
                color: bc,
            });
            non_overlay_rect_vertices.push(RectVertex {
                position: [0.0, logical_h],
                color: bc,
            });
        }

        // Window backgrounds
        for glyph in &frame_glyphs.glyphs {
            if let FrameGlyph::Background { bounds, color } = glyph {
                self.add_rect(
                    &mut non_overlay_rect_vertices,
                    bounds.x,
                    bounds.y,
                    bounds.width,
                    bounds.height,
                    color,
                );
            }
        }
        // Non-overlay stretches (skip those inside a box span)
        for (glyph_index, glyph) in frame_glyphs.glyphs.iter().enumerate() {
            if let FrameGlyph::Stretch {
                x,
                y,
                width,
                height,
                bg,
                face_id,
                row_role,
                clip_rect,
                ..
            } = glyph
                && !row_role.is_chrome()
            {
                for paint in params.pointer_override.face_paints(
                    glyph_index,
                    *face_id,
                    neomacs_display_protocol::Rect::new(*x, *y, *width, *height),
                    clip_rect.as_ref(),
                ) {
                    let face_id = paint.face_id();
                    let effective_clip = paint.clip();
                    let paint_offset_y = if has_line_anims {
                        self.line_y_offset(*x, *y)
                    } else {
                        0.0
                    };
                    if Self::paint_has_rounded_box_span(
                        *x,
                        *y,
                        *width,
                        *height,
                        face_id,
                        effective_clip.as_ref(),
                        *row_role,
                        box_spans,
                        faces,
                    ) {
                        continue;
                    }
                    let face = faces.get(&face_id);
                    self.add_face_paint_background(
                        &mut non_overlay_rect_vertices,
                        face,
                        bg,
                        paint,
                        0.0,
                        paint_offset_y,
                    );
                    // Draw the face's own `:stipple` over the stretch background,
                    // 1-bits in the face foreground (GNU `stippled_p`). A run of
                    // indentation whitespace carrying a stipple face (indent-bars /
                    // highlight-indent-guides) may be emitted as a Stretch, so —
                    // like the Char background path below — the stretch honors
                    // `face.stipple` (the single source of truth for stipples).
                    if let Some(pat) = face.and_then(|f| f.stipple.as_deref()) {
                        let rf = frame_glyphs.resolved_face(face_id);
                        self.add_stipple_paint(
                            &mut non_overlay_rect_vertices,
                            &rf.fg,
                            pat,
                            paint,
                            0.0,
                            paint_offset_y,
                        );
                    }
                }
            }
        }
        // Non-overlay char backgrounds (skip boxed chars — they get rounded bg instead)
        let mut bg_face_cache: Option<(FaceId, MaterializedFaceData)> = None;
        for (glyph_index, glyph) in frame_glyphs.glyphs.iter().enumerate() {
            if let FrameGlyph::Char {
                x,
                y,
                width,
                height,
                face_id,
                row_role,
                clip_rect,
                ..
            } = glyph
                && !row_role.is_chrome()
            {
                for paint in params.pointer_override.face_paints(
                    glyph_index,
                    *face_id,
                    neomacs_display_protocol::Rect::new(*x, *y, *width, *height),
                    clip_rect.as_ref(),
                ) {
                    let face_id = paint.face_id();
                    let effective_clip = paint.clip();
                    let paint_offset_y = if has_line_anims {
                        self.line_y_offset(*x, *y)
                    } else {
                        0.0
                    };
                    // The per-glyph background used to be inlined as
                    // `Some(face.background)` by materialization, so resolving
                    // it from the face reproduces that value exactly.
                    let rf = match bg_face_cache {
                        Some((id, ref data)) if id == face_id => *data,
                        _ => {
                            let data = frame_glyphs.resolved_face(face_id);
                            bg_face_cache = Some((face_id, data));
                            data
                        }
                    };
                    let bg: Option<Color> = Some(rf.bg);
                    let face = faces.get(&face_id);
                    let has_gradient = face
                        .and_then(|resolved| resolved.background_gradient.as_deref())
                        .is_some();
                    let has_solid_bg = face.map(|f| f.background.a > f32::EPSILON).unwrap_or(false);
                    if bg.is_some() || has_gradient || has_solid_bg {
                        if Self::paint_has_rounded_box_span(
                            *x,
                            *y,
                            *width,
                            *height,
                            face_id,
                            effective_clip.as_ref(),
                            *row_role,
                            box_spans,
                            faces,
                        ) {
                            continue;
                        }
                        let fallback = bg.unwrap_or(
                            face.map(|resolved| resolved.background)
                                .unwrap_or(Color::TRANSPARENT),
                        );
                        self.add_face_paint_background(
                            &mut non_overlay_rect_vertices,
                            face,
                            &fallback,
                            paint,
                            0.0,
                            paint_offset_y,
                        );
                        // Stipple: tile the face's `:stipple` bitmap over the
                        // glyph background, 1-bits in the face foreground (GNU
                        // `stippled_p`). This is how `indent-bars` /
                        // `highlight-indent-guides` draw their vertical bars.
                        if let Some(pat) = face.and_then(|f| f.stipple.as_deref()) {
                            self.add_stipple_paint(
                                &mut non_overlay_rect_vertices,
                                &rf.fg,
                                pat,
                                paint,
                                0.0,
                                paint_offset_y,
                            );
                        }
                    }
                }
            }
        }

        // Non-overlay image backgrounds: the image texture covers only its
        // margin-inset content rect, so the face background for the full GNU
        // box extent must be painted here, exactly like the char path above.
        // Without it a masked/transparent image (telega's reply icon) shows
        // the window background through its slot instead of the face
        // background GNU paints behind the glyph.
        for (glyph_index, glyph) in frame_glyphs.glyphs.iter().enumerate() {
            if let FrameGlyph::Image {
                face_id,
                row_role,
                clip_rect,
                box_rect,
                ..
            } = glyph
                && !row_role.is_chrome()
            {
                let (x, y, width, height) =
                    (box_rect.x, box_rect.y, box_rect.width, box_rect.height);
                for paint in params.pointer_override.face_paints(
                    glyph_index,
                    *face_id,
                    *box_rect,
                    clip_rect.as_ref(),
                ) {
                    let face_id = paint.face_id();
                    let effective_clip = paint.clip();
                    let paint_offset_y = if has_line_anims {
                        self.line_y_offset(x, y)
                    } else {
                        0.0
                    };
                    let rf = match bg_face_cache {
                        Some((id, ref data)) if id == face_id => *data,
                        _ => {
                            let data = frame_glyphs.resolved_face(face_id);
                            bg_face_cache = Some((face_id, data));
                            data
                        }
                    };
                    let bg: Option<Color> = Some(rf.bg);
                    let face = faces.get(&face_id);
                    let has_gradient = face
                        .and_then(|resolved| resolved.background_gradient.as_deref())
                        .is_some();
                    let has_solid_bg = face.map(|f| f.background.a > f32::EPSILON).unwrap_or(false);
                    if bg.is_some() || has_gradient || has_solid_bg {
                        if Self::paint_has_rounded_box_span(
                            x,
                            y,
                            width,
                            height,
                            face_id,
                            effective_clip.as_ref(),
                            *row_role,
                            box_spans,
                            faces,
                        ) {
                            continue;
                        }
                        let fallback = bg.unwrap_or(
                            face.map(|resolved| resolved.background)
                                .unwrap_or(Color::TRANSPARENT),
                        );
                        self.add_face_paint_background(
                            &mut non_overlay_rect_vertices,
                            face,
                            &fallback,
                            paint,
                            0.0,
                            paint_offset_y,
                        );
                        if let Some(pat) = face.and_then(|f| f.stipple.as_deref()) {
                            self.add_stipple_paint(
                                &mut non_overlay_rect_vertices,
                                &rf.fg,
                                pat,
                                paint,
                                0.0,
                                paint_offset_y,
                            );
                        }
                    }
                }
            }
        }

        // --- Fringe bitmaps (own fringe column, drawn with the non-overlay
        // backgrounds so they sit below text — magit section fold arrows). ---
        for glyph in &frame_glyphs.glyphs {
            if let FrameGlyph::FringeBitmap {
                x,
                y,
                width,
                height,
                bitmap_index,
                face_id,
                ..
            } = glyph
            {
                let Some(bitmap) = frame_glyphs.fringe_bitmaps.get(bitmap_index) else {
                    continue;
                };
                let face = frame_glyphs.resolved_face(*face_id);
                self.render_fringe_bitmap(
                    &mut non_overlay_rect_vertices,
                    *x,
                    *y,
                    *width,
                    *height,
                    &face.fg,
                    bitmap,
                );
            }
        }

        // --- Current line highlight ---
        if self.effects.line_highlight.enabled {
            let (lr, lg, lb, la) = self.effects.line_highlight.color;
            let hl_color = Color::new(lr, lg, lb, la);

            if let Some(cursor) = frame_glyphs.active_cursor() {
                for info in &frame_glyphs.window_infos {
                    if info.selected {
                        self.add_rect(
                            &mut non_overlay_rect_vertices,
                            info.bounds.x,
                            cursor.y,
                            info.bounds.width,
                            cursor.height,
                            &hl_color,
                        );
                        break;
                    }
                }
            }
        }

        // --- Indent guides ---
        if self.effects.indent_guides.enabled {
            let (ig_r, ig_g, ig_b, ig_a) = self.effects.indent_guides.color;
            let guide_color = Color::new(ig_r, ig_g, ig_b, ig_a);
            let guide_width = 1.0_f32;

            // Detect char_width from frame
            let char_w = frame_glyphs.char_width.max(1.0);
            let tab_w = 4; // default tab width; we infer from the glyph spacing

            // Collect row info: group chars by Y coordinate to find rows,
            // then detect indent (leading space/tab) per row.
            struct RowInfo {
                y: f32,
                height: f32,
                first_non_space_x: f32,
                text_start_x: f32, // leftmost char X in the row
            }
            let mut rows: Vec<RowInfo> = Vec::new();
            let mut current_row_y: f32 = -1.0;
            let mut current_row_h: f32 = 0.0;
            let mut first_non_space_x: f32 = f32::MAX;
            let mut text_start_x: f32 = f32::MAX;
            let mut has_chars = false;

            for glyph in &frame_glyphs.glyphs {
                if let FrameGlyph::Char {
                    x,
                    y,
                    width: _,
                    height,
                    char: ch,
                    row_role,
                    ..
                } = glyph
                {
                    if row_role.is_chrome() {
                        continue;
                    }
                    let gy = *y;
                    if (gy - current_row_y).abs() > 0.5 {
                        // New row — save previous
                        if has_chars && first_non_space_x > text_start_x + char_w {
                            rows.push(RowInfo {
                                y: current_row_y,
                                height: current_row_h,
                                first_non_space_x,
                                text_start_x,
                            });
                        }
                        current_row_y = gy;
                        current_row_h = *height;
                        first_non_space_x = f32::MAX;
                        text_start_x = f32::MAX;
                    }
                    has_chars = true;
                    if *x < text_start_x {
                        text_start_x = *x;
                    }
                    if *ch != ' ' && *ch != '\t' && *x < first_non_space_x {
                        first_non_space_x = *x;
                    }
                }
            }
            // Save last row
            if has_chars && first_non_space_x > text_start_x + char_w {
                rows.push(RowInfo {
                    y: current_row_y,
                    height: current_row_h,
                    first_non_space_x,
                    text_start_x,
                });
            }

            // Draw guides at each tab-stop column within the indent region
            let tab_px = char_w * tab_w as f32;
            let use_rainbow = self.effects.indent_guides.rainbow_enabled
                && !self.effects.indent_guides.rainbow_colors.is_empty();
            for row in &rows {
                let mut col_x = row.text_start_x + tab_px;
                let mut depth: usize = 0;
                while col_x < row.first_non_space_x - 1.0 {
                    let color = if use_rainbow {
                        let (r, g, b, a) = self.effects.indent_guides.rainbow_colors
                            [depth % self.effects.indent_guides.rainbow_colors.len()];
                        Color::new(r, g, b, a)
                    } else {
                        guide_color
                    };
                    self.add_rect(
                        &mut non_overlay_rect_vertices,
                        col_x,
                        row.y,
                        guide_width,
                        row.height,
                        &color,
                    );
                    col_x += tab_px;
                    depth += 1;
                }
            }
        }

        // --- Visible whitespace dots ---
        if self.effects.show_whitespace.enabled {
            let (wr, wg, wb, wa) = self.effects.show_whitespace.color;
            let ws_color = Color::new(wr, wg, wb, wa);
            let dot_size = 1.5_f32;

            for glyph in &frame_glyphs.glyphs {
                if let FrameGlyph::Char {
                    char: ch,
                    x,
                    y,
                    width,
                    height: _,
                    ascent,
                    row_role,
                    ..
                } = glyph
                {
                    if row_role.is_chrome() {
                        continue;
                    }
                    if *ch == ' ' {
                        // Centered dot for space
                        let dot_x = *x + (*width - dot_size) / 2.0;
                        let dot_y = *y + (*ascent - dot_size / 2.0);
                        self.add_rect(
                            &mut non_overlay_rect_vertices,
                            dot_x,
                            dot_y,
                            dot_size,
                            dot_size,
                            &ws_color,
                        );
                    } else if *ch == '\t' {
                        // Small horizontal arrow for tab
                        let arrow_h = 1.5_f32;
                        let arrow_y = *y + (*ascent - arrow_h / 2.0);
                        let arrow_w = (*width - 4.0).max(4.0);
                        let arrow_x = *x + 2.0;
                        // Shaft
                        self.add_rect(
                            &mut non_overlay_rect_vertices,
                            arrow_x,
                            arrow_y,
                            arrow_w,
                            arrow_h,
                            &ws_color,
                        );
                        // Arrowhead (small triangle approximated as 2 rects)
                        let tip_x = arrow_x + arrow_w;
                        self.add_rect(
                            &mut non_overlay_rect_vertices,
                            tip_x - 3.0,
                            arrow_y - 1.5,
                            3.0,
                            arrow_h + 3.0,
                            &ws_color,
                        );
                    }
                }
            }
        }

        non_overlay_rect_vertices
    }

    /// Collect the overlay (mode-line/echo area) background layer.
    pub(super) fn collect_overlay_backgrounds(
        &self,
        params: &FrameParams<'_>,
        spans: &BoxSpanSet,
    ) -> Vec<RectVertex> {
        let frame_glyphs = params.frame_glyphs;
        let faces = params.faces;
        let box_spans = &spans.spans;
        // --- Collect overlay backgrounds ---
        let mut overlay_rect_vertices: Vec<RectVertex> = Vec::new();

        // Overlay stretches (skip those inside a box span)
        for (glyph_index, glyph) in frame_glyphs.glyphs.iter().enumerate() {
            if let FrameGlyph::Stretch {
                x,
                y,
                width,
                height,
                bg,
                face_id,
                row_role,
                clip_rect,
                ..
            } = glyph
                && row_role.is_chrome()
            {
                for paint in params.pointer_override.face_paints(
                    glyph_index,
                    *face_id,
                    neomacs_display_protocol::Rect::new(*x, *y, *width, *height),
                    clip_rect.as_ref(),
                ) {
                    let face_id = paint.face_id();
                    let effective_clip = paint.clip();
                    if Self::paint_has_rounded_box_span(
                        *x,
                        *y,
                        *width,
                        *height,
                        face_id,
                        effective_clip.as_ref(),
                        *row_role,
                        box_spans,
                        faces,
                    ) {
                        continue;
                    }
                    let face = faces.get(&face_id);
                    self.add_face_paint_background(
                        &mut overlay_rect_vertices,
                        face,
                        bg,
                        paint,
                        0.0,
                        0.0,
                    );
                }
            }
        }
        // Overlay char backgrounds (skip those inside a box span)
        let mut overlay_bg_face_cache: Option<(FaceId, MaterializedFaceData)> = None;
        for (glyph_index, glyph) in frame_glyphs.glyphs.iter().enumerate() {
            if let FrameGlyph::Char {
                x,
                y,
                width,
                height,
                face_id,
                row_role,
                clip_rect,
                ..
            } = glyph
                && row_role.is_chrome()
            {
                for paint in params.pointer_override.face_paints(
                    glyph_index,
                    *face_id,
                    neomacs_display_protocol::Rect::new(*x, *y, *width, *height),
                    clip_rect.as_ref(),
                ) {
                    let face_id = paint.face_id();
                    let effective_clip = paint.clip();
                    // Background was inlined as `Some(face.background)`.
                    let rf = match overlay_bg_face_cache {
                        Some((id, ref data)) if id == face_id => *data,
                        _ => {
                            let data = frame_glyphs.resolved_face(face_id);
                            overlay_bg_face_cache = Some((face_id, data));
                            data
                        }
                    };
                    let bg: Option<Color> = Some(rf.bg);
                    let face = faces.get(&face_id);
                    let has_gradient = face
                        .and_then(|resolved| resolved.background_gradient.as_deref())
                        .is_some();
                    let has_solid_bg = face.map(|f| f.background.a > f32::EPSILON).unwrap_or(false);
                    if bg.is_some() || has_gradient || has_solid_bg {
                        if Self::paint_has_rounded_box_span(
                            *x,
                            *y,
                            *width,
                            *height,
                            face_id,
                            effective_clip.as_ref(),
                            *row_role,
                            box_spans,
                            faces,
                        ) {
                            continue;
                        }
                        let fallback = bg.unwrap_or(
                            face.map(|resolved| resolved.background)
                                .unwrap_or(Color::TRANSPARENT),
                        );
                        self.add_face_paint_background(
                            &mut overlay_rect_vertices,
                            face,
                            &fallback,
                            paint,
                            0.0,
                            0.0,
                        );
                    }
                }
            }
        }

        overlay_rect_vertices
    }

    pub(super) fn draw_non_overlay_backgrounds(
        &mut self,
        render_pass: &mut wgpu::RenderPass<'_>,
        non_overlay_rect_vertices: &[RectVertex],
    ) {
        // === Step 1: Draw non-overlay backgrounds ===
        self.draw_rect_vertex_layer(render_pass, non_overlay_rect_vertices);
    }

    /// Draw the overlay background rects and rounded box fills. Runs at the
    /// start of the overlay text pass so overlay text lands on top.
    pub(super) fn draw_overlay_background_layer(
        &mut self,
        ctx: &mut FramePassCtx<'_, '_>,
        spans: &BoxSpanSet,
        overlay_rect_vertices: &[RectVertex],
    ) {
        let render_pass = &mut ctx.pass;
        let faces = ctx.params.faces;
        let box_spans = &spans.spans;
        if let Some(upload) =
            self.arenas
                .rect
                .upload(&self.device, &self.queue, overlay_rect_vertices)
        {
            render_pass.set_pipeline(&self.pipelines.rect);
            render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
            render_pass.set_vertex_buffer(0, upload.buffer_slice());
            render_pass.draw(0..overlay_rect_vertices.len() as u32, 0..1);
        }

        // Image textures leave their GNU margin/row-sized box slot uncovered.
        let mut required_box_fill: Vec<RectVertex> = Vec::new();
        for span in box_spans {
            if span.row_role.is_chrome() {
                self.append_required_box_background_fill_geometry(
                    &mut required_box_fill,
                    span,
                    0.0,
                    0.0,
                );
            }
        }
        self.draw_rect_vertex_layer(render_pass, &required_box_fill);

        // Draw filled rounded rect backgrounds for overlay ROUNDED boxed spans.
        {
            let mut overlay_box_fill: Vec<RoundedRectVertex> = Vec::new();
            for span in box_spans {
                if !span.row_role.is_chrome() {
                    continue;
                }
                if let Some(face) = faces.get(&span.face_id) {
                    self.append_rounded_box_fill_geometry(
                        &mut overlay_box_fill,
                        span,
                        face,
                        0.0,
                        0.0,
                    );
                }
            }
            if let Some(upload) =
                self.arenas
                    .rounded
                    .upload(&self.device, &self.queue, &overlay_box_fill)
            {
                render_pass.set_pipeline(&self.pipelines.rounded_rect);
                render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                render_pass.set_vertex_buffer(0, upload.buffer_slice());
                render_pass.draw(0..overlay_box_fill.len() as u32, 0..1);
            }
        }
    }
}
