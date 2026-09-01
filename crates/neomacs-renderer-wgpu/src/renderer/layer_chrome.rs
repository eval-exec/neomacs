//! Cursor, window border, and scroll bar phases of `render_frame_glyphs`
//! (z-order steps 2-3 collection and step 8 draws).

use neomacs_display_protocol::frame_glyphs::FrameGlyph;
use neomacs_display_protocol::types::Color;

use super::super::vertex::RectVertex;
use super::WgpuRenderer;
use super::frame_pass::{ChromeLayerVertices, FrameParams, FramePassCtx};

impl WgpuRenderer {
    /// Collect cursor rects (inverse-video bg, behind-text trail, front
    /// cursors), window borders, and scroll bar tracks/thumbs.
    pub(super) fn collect_chrome_layers(
        &mut self,
        params: &FrameParams<'_>,
    ) -> ChromeLayerVertices {
        let frame_glyphs = params.frame_glyphs;
        let cursor_visible = params.cursor_visible;
        let animated_cursor = params.animated_cursor;
        let mouse_pos = params.mouse_pos;
        // === Collect cursor bg rect for inverse video (drawn before text) ===
        // For filled box cursor (style 0), we draw the cursor background BEFORE text
        // so the character under the cursor can be re-drawn with inverse colors on top.
        let mut cursor_bg_vertices: Vec<RectVertex> = Vec::new();

        // === Collect behind-text cursor shapes (animated trail for filled box) ===
        let mut behind_text_cursor_vertices: Vec<RectVertex> = Vec::new();

        // === Collect front cursors and borders (drawn after text) ===
        // Bar (1), hbar (2), hollow (3), borders — all drawn on top of text.
        // Filled box (0) is EXCLUDED here — handled by bg rect + trail + fg swap.
        // Scroll bar tracks AND thumbs are collected into cursor_vertices too,
        // so both draw through the reliable rect pipeline in draw_cursor_layer.
        let mut cursor_vertices: Vec<RectVertex> = Vec::new();

        for glyph in &frame_glyphs.glyphs {
            match glyph {
                FrameGlyph::Border {
                    x,
                    y,
                    width,
                    height,
                    color,
                    clip_rect,
                    ..
                } => {
                    let mut draw_y = *y;
                    let mut draw_h = *height;
                    if let Some(clip) = clip_rect {
                        let top = clip.y;
                        let bottom = clip.y + clip.height;
                        if draw_y < top {
                            let cut = top - draw_y;
                            if cut >= draw_h {
                                continue;
                            }
                            draw_y = top;
                            draw_h -= cut;
                        }
                        if draw_y + draw_h > bottom {
                            let cut = (draw_y + draw_h) - bottom;
                            if cut >= draw_h {
                                continue;
                            }
                            draw_h -= cut;
                        }
                    }
                    if draw_h > 0.0 {
                        self.add_rect(&mut cursor_vertices, *x, draw_y, *width, draw_h, color);
                    }
                }
                FrameGlyph::ScrollBar {
                    window_id: _,
                    row_role: _,
                    clip_rect: _,
                    horizontal,
                    x,
                    y,
                    width,
                    height,
                    position: _,
                    portion: _,
                    whole: _,
                    thumb_start,
                    thumb_size,
                    track_color,
                    thumb_color,
                } => {
                    // Draw scroll bar track (subtle, configurable opacity)
                    let subtle_track = Color::new(
                        track_color.r,
                        track_color.g,
                        track_color.b,
                        track_color.a * self.effects.scroll_bar.track_opacity,
                    );
                    self.add_rect(&mut cursor_vertices, *x, *y, *width, *height, &subtle_track);

                    // Compute thumb bounds
                    let (tx, ty, tw, th) = if *horizontal {
                        (*x + *thumb_start, *y, *thumb_size, *height)
                    } else {
                        (*x, *y + *thumb_start, *width, *thumb_size)
                    };

                    // Check hover: brighten thumb if mouse is over the scroll bar area
                    let (mx, my) = mouse_pos;
                    let hovered = mx >= *x && mx <= *x + *width && my >= *y && my <= *y + *height;
                    let bright = self.effects.scroll_bar.hover_brightness;
                    let effective_thumb = if hovered {
                        Color::new(
                            (thumb_color.r * bright).min(1.0),
                            (thumb_color.g * bright).min(1.0),
                            (thumb_color.b * bright).min(1.0),
                            thumb_color.a.min(1.0),
                        )
                    } else {
                        *thumb_color
                    };

                    // Draw the thumb as a solid rect through the proven rect
                    // pipeline (into cursor_vertices, after the track, so it lands
                    // on top of it). The rounded_rect "filled" path (border_width
                    // = 0) renders nothing — that pipeline paints no fragments
                    // here — so this is the reliable way to make the thumb visible.
                    // It still tracks GNU's scroll position and portion, just with
                    // square corners instead of GTK's pill.
                    self.add_rect(&mut cursor_vertices, tx, ty, tw, th, &effective_thumb);
                }
                _ => {}
            }
        }

        // One entry per window (selected window's entry is `active`); every
        // cursor draws through the shared cursor_draw_rect, so a non-selected
        // window's box lands on its glyph cell just like the selected one,
        // under line numbers or scaled fonts.
        for cursor in &frame_glyphs.window_cursors {
            let cursor_effects = frame_glyphs
                .effective_window_cursor_effects(cursor.window_id, &self.effects)
                .clone();
            self.emit_cursor_visual(
                cursor.window_id,
                frame_glyphs.cursor_draw_rect(
                    cursor.slot_id,
                    cursor.style,
                    cursor.ascent,
                    (cursor.x, cursor.y, cursor.width, cursor.height),
                ),
                cursor.slot_id,
                cursor.style,
                &cursor.color,
                cursor.cursor_fg,
                &cursor_effects,
                cursor_visible,
                animated_cursor,
                &mut cursor_bg_vertices,
                &mut behind_text_cursor_vertices,
                &mut cursor_vertices,
            );
        }

        ChromeLayerVertices {
            cursor_bg: cursor_bg_vertices,
            behind_text_cursor: behind_text_cursor_vertices,
            cursors: cursor_vertices,
        }
    }

    pub(super) fn draw_pre_text_cursor_layers(
        &mut self,
        render_pass: &mut wgpu::RenderPass<'_>,
        cursor_bg_vertices: &[RectVertex],
        behind_text_cursor_vertices: &[RectVertex],
    ) {
        // === Step 2: Draw cursor bg rect (inverse video background) ===
        // Drawn after window/char backgrounds but before text, so the cursor
        // background color is visible behind the inverse-video character.
        self.draw_rect_vertex_layer(render_pass, cursor_bg_vertices);

        // === Step 3: Draw animated cursor trail behind text ===
        // The spring trail or animated rect for filled box cursor appears
        // behind text so characters remain readable during cursor motion.
        self.draw_rect_vertex_layer(render_pass, behind_text_cursor_vertices);
    }

    /// Draw front cursors and window borders (after text).
    pub(super) fn draw_cursor_layer(
        &mut self,
        ctx: &mut FramePassCtx<'_, '_>,
        chrome: &ChromeLayerVertices,
    ) {
        let render_pass = &mut ctx.pass;
        let cursor_vertices = &chrome.cursors;
        // Draw cursors and borders (after text)
        if let Some(upload) = self
            .arenas
            .rect
            .upload(&self.device, &self.queue, &chrome.cursors)
        {
            render_pass.set_pipeline(&self.pipelines.rect);
            render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
            render_pass.set_vertex_buffer(0, upload.buffer_slice());
            render_pass.draw(0..cursor_vertices.len() as u32, 0..1);
        }
    }
}
