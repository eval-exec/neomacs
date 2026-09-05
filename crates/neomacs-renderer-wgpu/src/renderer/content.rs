//! Child-frame content rendering core.
//!
//! `render_frame_content()` renders ALL glyph types from a `FrameGlyphBuffer`
//! into an existing surface. Used by child frame rendering for full parity with
//! the main frame's glyph handling.
//!
//! Handles: Char (with overstrike, composed, decorations), Stretch (with stipple),
//! Background, Border, Cursor (all styles with animation), ScrollBar (with rounded
//! thumbs), Image, Video, WebKit.

use super::super::glyph_atlas::{
    AnyAtlasEntry, ComposedGlyphKey, GlyphKey, SubpixelRequest, WgpuGlyphAtlas,
};
use super::super::vertex::{GlyphVertex, RectVertex, RoundedRectVertex, SubpixelGlyphVertex};
use super::GlyphRenderStats;
use super::WgpuRenderer;
use super::cursor_presentation::{
    CursorColorPolicy, CursorShape, FilledBoxPresentation, PresentedCursorPaint,
    ResolvedCursorPaint,
};
use super::frame_pass::{BoxSpan, collect_frame_box_spans};
#[cfg(all(feature = "webview", target_os = "linux"))]
use super::layer_media::inline_webview_quad;
use super::layer_media::{MediaQuad, clipped_media_rect, textured_quad_vertices_uv};
use cosmic_text::SubpixelBin;
use neomacs_display_protocol::DeviceScale;
use neomacs_display_protocol::face::{Face, FaceAttributes, UnderlineStyle};
use neomacs_display_protocol::frame_glyphs::{
    CursorStyle, FrameGlyph, FrameGlyphBuffer, MaterializedFaceData,
};
use neomacs_display_protocol::types::FaceId;
use neomacs_display_protocol::types::Rect;
use neomacs_display_protocol::types::{AnimatedCursor, Color};
use std::collections::HashSet;

/// Snap a glyph origin's physical-pixel position to the nearest whole pixel with
/// no subpixel bin, matching GNU Emacs's whole-pixel glyph placement.
///
/// The atlas cache key includes the subpixel bins (`GlyphKey::x_bin`/`y_bin`).
/// Deriving them from the fractional origin (via `SubpixelBin::new`) made the key
/// depend on subpixel position: a monospace grid has fractional advances, so the
/// bin varied per column, and every fractional scroll offset produced a fresh
/// vertical bin -- turning each glyph into up to 16 position variants and
/// re-rasterizing ~173 glyphs (with as many GPU texture uploads) *per frame*
/// during scrolling, at ~25% of total CPU. Forcing the bins to `Zero` collapses
/// the key to `(char, face, size, font)` so each glyph rasterizes once and later
/// frames are pure cache hits. The vertex is placed at the returned integer
/// origin, so bitmap and placement stay consistent.
pub(super) fn snap_glyph_origin(phys_x: f32, phys_y: f32) -> (i32, i32, SubpixelBin, SubpixelBin) {
    (
        phys_x.round() as i32,
        phys_y.round() as i32,
        SubpixelBin::Zero,
        SubpixelBin::Zero,
    )
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct DecorationRect {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    color: Color,
}

fn stretch_decoration_rects(face: &Face, x: f32, y: f32, width: f32) -> Vec<DecorationRect> {
    let mut rects = Vec::new();
    let fg = face.foreground;
    let baseline = y + face.font_ascent as f32;
    let underline = face.underline_placement.resolve(
        y,
        (face.font_ascent + face.font_descent).max(1) as f32,
        baseline,
        face.underline_thickness as f32,
    );
    let thickness = underline.thickness;
    let underline_color = face.underline_color.unwrap_or(fg);
    let mut push = |x, y, width, height, color| {
        rects.push(DecorationRect {
            x,
            y,
            width,
            height,
            color,
        });
    };
    if face.attributes.contains(FaceAttributes::UNDERLINE) {
        let underline_y = underline.top_y;
        match face.underline_style {
            UnderlineStyle::Line => push(x, underline_y, width, thickness, underline_color),
            UnderlineStyle::Double => {
                push(x, underline_y, width, thickness, underline_color);
                push(
                    x,
                    underline_y + thickness + 1.0,
                    width,
                    thickness,
                    underline_color,
                );
            }
            UnderlineStyle::Wave => {
                let mut cx = x;
                while cx < x + width {
                    let segment = 1.0_f32.min(x + width - cx);
                    let phase = (cx - x) * std::f32::consts::TAU / 8.0;
                    push(
                        cx,
                        underline_y + phase.sin() * 2.0,
                        segment,
                        thickness,
                        underline_color,
                    );
                    cx += 1.0;
                }
            }
            UnderlineStyle::Dotted => {
                let mut cx = x;
                while cx < x + width {
                    let segment = thickness.min(x + width - cx);
                    push(cx, underline_y, segment, thickness, underline_color);
                    cx += thickness + 2.0;
                }
            }
            UnderlineStyle::Dashed => {
                let mut cx = x;
                while cx < x + width {
                    let segment = 4.0_f32.min(x + width - cx);
                    push(cx, underline_y, segment, thickness, underline_color);
                    cx += 7.0;
                }
            }
            UnderlineStyle::None => {}
        }
    }
    if face.attributes.contains(FaceAttributes::OVERLINE) {
        push(x, y, width, thickness, face.overline_color.unwrap_or(fg));
    }
    if face.attributes.contains(FaceAttributes::STRIKE_THROUGH) {
        push(
            x,
            baseline - face.font_ascent as f32 / 3.0,
            width,
            thickness,
            face.strike_through_color.unwrap_or(fg),
        );
    }
    rects
}

fn subpixel_foreground_color(bg: Color, fg: Color, blend: f32) -> [f32; 4] {
    let t = blend.clamp(0.0, 1.0);
    [
        bg.r + (fg.r - bg.r) * t,
        bg.g + (fg.g - bg.g) * t,
        bg.b + (fg.b - bg.b) * t,
        1.0,
    ]
}

fn subpixel_background_color(bg: Color) -> [f32; 4] {
    [bg.r, bg.g, bg.b, bg.a]
}

fn build_subpixel_vertices(
    glyph_x: f32,
    glyph_y: f32,
    glyph_w: f32,
    glyph_h: f32,
    tex_u_min: f32,
    tex_u_max: f32,
    tex_v_min: f32,
    tex_v_max: f32,
    fg_color: [f32; 4],
    bg_color: [f32; 4],
) -> [SubpixelGlyphVertex; 6] {
    [
        SubpixelGlyphVertex {
            position: [glyph_x, glyph_y],
            tex_coords: [tex_u_min, tex_v_min],
            fg_color,
            bg_color,
        },
        SubpixelGlyphVertex {
            position: [glyph_x + glyph_w, glyph_y],
            tex_coords: [tex_u_max, tex_v_min],
            fg_color,
            bg_color,
        },
        SubpixelGlyphVertex {
            position: [glyph_x + glyph_w, glyph_y + glyph_h],
            tex_coords: [tex_u_max, tex_v_max],
            fg_color,
            bg_color,
        },
        SubpixelGlyphVertex {
            position: [glyph_x, glyph_y],
            tex_coords: [tex_u_min, tex_v_min],
            fg_color,
            bg_color,
        },
        SubpixelGlyphVertex {
            position: [glyph_x + glyph_w, glyph_y + glyph_h],
            tex_coords: [tex_u_max, tex_v_max],
            fg_color,
            bg_color,
        },
        SubpixelGlyphVertex {
            position: [glyph_x, glyph_y + glyph_h],
            tex_coords: [tex_u_min, tex_v_max],
            fg_color,
            bg_color,
        },
    ]
}

impl WgpuRenderer {
    /// Render all glyphs from a `FrameGlyphBuffer` with coordinate offset.
    ///
    /// This is the child-frame content rendering core. It handles
    /// every glyph type with the same fidelity as the main frame renderer
    /// (minus visual effects which are main-frame-only).
    ///
    /// Uses `LoadOp::Load` to composite on top of existing content.
    /// Everything is rendered in a single encoder + single `queue.submit()`.
    pub fn render_frame_content(
        &mut self,
        view: &wgpu::TextureView,
        frame: &FrameGlyphBuffer,
        glyph_atlas: &mut WgpuGlyphAtlas,
        _surface_width: u32,
        _surface_height: u32,
        offset_x: f32,
        offset_y: f32,
        cursor_visible: bool,
        animated_cursor: Option<AnimatedCursor>,
        clip_corner_radius: f32,
        pointer_selection: Option<neomacs_display_protocol::PointerAppearanceSelection>,
        scissor: Option<(u32, u32, u32, u32)>,
    ) {
        self.arenas.glyph.begin_frame();
        self.arenas.subpixel.begin_frame();

        tracing::debug!(
            "render_frame_content: frame={}x{} offset=({:.1},{:.1}) {} glyphs",
            frame.width,
            frame.height,
            offset_x,
            offset_y,
            frame.glyphs.len(),
        );
        let pointer_override =
            super::pointer_override::PointerOverrideResolver::new(frame, pointer_selection);

        let mut stats = GlyphRenderStats::new();
        stats.total_frame_glyphs = frame.glyphs.len();
        let mut seen_single_keys: HashSet<GlyphKey> = HashSet::new();
        let mut seen_composed_keys: HashSet<ComposedGlyphKey> = HashSet::new();
        let faces = &frame.faces;
        let device_scale = DeviceScale::new(self.scale_factor)
            .expect("renderer scale factor is validated by its native-surface adapter");

        // --- Box span merging (for proper border rendering) ---
        let box_spans = self.merge_box_spans(frame, &pointer_override);

        // --- Collect vertices by category for correct z-ordering ---
        //
        // Rendering order:
        //   1. Backgrounds (window bg, stretches, char bg)
        //   2. Box borders (sharp and rounded)
        //   3. Text (mask glyphs, color glyphs, composed)
        //   4. Decorations (underline, overline, strikethrough)
        //   5. Inline media (images, videos, webkit)
        //   6. Cursors, borders, scroll bars (on top)
        let mut bg_vertices: Vec<RectVertex> = Vec::new();
        let mut cursor_bg_vertices: Vec<RectVertex> = Vec::new();
        let mut cursor_vertices: Vec<RectVertex> = Vec::new();
        let mut cursor_inverse_video = None;
        let mut fringe_vertices: Vec<RectVertex> = Vec::new();
        let mut scroll_bar_thumbs: Vec<(f32, f32, f32, f32, f32, Color)> = Vec::new();

        // --- Step 1: Collect backgrounds ---
        for (glyph_index, glyph) in frame.glyphs.iter().enumerate() {
            match glyph {
                FrameGlyph::Background { bounds, color } => {
                    self.add_rect(
                        &mut bg_vertices,
                        bounds.x + offset_x,
                        bounds.y + offset_y,
                        bounds.width,
                        bounds.height,
                        color,
                    );
                }
                FrameGlyph::Stretch {
                    x,
                    y,
                    width,
                    height,
                    bg: _,
                    face_id,
                    row_role,
                    clip_rect,
                    ..
                } => {
                    for paint in pointer_override.face_paints(
                        glyph_index,
                        *face_id,
                        Rect::new(*x, *y, *width, *height),
                        clip_rect.as_ref(),
                    ) {
                        let face_id = paint.face_id();
                        let bg = frame.resolved_face(face_id).bg;
                        let effective_clip = paint.clip();
                        if Self::paint_has_rounded_box_span(
                            *x,
                            *y,
                            *width,
                            *height,
                            face_id,
                            effective_clip.as_ref(),
                            *row_role,
                            &box_spans,
                            faces,
                        ) {
                            continue;
                        }
                        self.add_face_paint_background(
                            &mut bg_vertices,
                            frame.faces.get(&face_id),
                            &bg,
                            paint,
                            offset_x,
                            offset_y,
                        );
                        // Draw the face's own `:stipple` over the stretch
                        // background (GNU `stippled_p`), mirroring the Char arm
                        // below. A run of stipple-faced whitespace (indent-bars /
                        // highlight-indent-guides) may be a Stretch. `face.stipple`
                        // is the single source of truth for stipples.
                        if let Some(pat) =
                            frame.faces.get(&face_id).and_then(|f| f.stipple.as_deref())
                        {
                            let fg = frame.resolved_face(face_id).fg;
                            self.add_stipple_paint(
                                &mut bg_vertices,
                                &fg,
                                pat,
                                paint,
                                offset_x,
                                offset_y,
                            );
                        }
                    }
                }
                FrameGlyph::Char {
                    x,
                    y,
                    width,
                    height,
                    face_id,
                    row_role,
                    clip_rect,
                    ..
                } => {
                    // Per-glyph background was inlined as `Some(face.background)`;
                    // resolve it from the face table for the same value.
                    for paint in pointer_override.face_paints(
                        glyph_index,
                        *face_id,
                        Rect::new(*x, *y, *width, *height),
                        clip_rect.as_ref(),
                    ) {
                        let face_id = paint.face_id();
                        let bg_color = frame.resolved_face(face_id).bg;
                        let effective_clip = paint.clip();
                        if Self::paint_has_rounded_box_span(
                            *x,
                            *y,
                            *width,
                            *height,
                            face_id,
                            effective_clip.as_ref(),
                            *row_role,
                            &box_spans,
                            faces,
                        ) {
                            continue;
                        }
                        self.add_face_paint_background(
                            &mut bg_vertices,
                            frame.faces.get(&face_id),
                            &bg_color,
                            paint,
                            offset_x,
                            offset_y,
                        );
                        // Stipple pattern behind the glyph. GNU sets
                        // `s->stippled_p = face->stipple != 0` for text glyph
                        // strings (xterm.c) and fills the background rect with
                        // the tiled bitmap in the face foreground (0-bits leave
                        // the solid bg painted above). Key it off the face so a
                        // `:stipple` face (e.g. indent-bars) paints on ordinary
                        // buffer-text glyphs.
                        if let Some(pat) =
                            frame.faces.get(&face_id).and_then(|f| f.stipple.as_deref())
                        {
                            let fg = frame.resolved_face(face_id).fg;
                            self.add_stipple_paint(
                                &mut bg_vertices,
                                &fg,
                                pat,
                                paint,
                                offset_x,
                                offset_y,
                            );
                        }
                    }
                }
                _ => {}
            }
        }

        // --- Collect fringe bitmaps (own step: own fringe column, no overlap
        // with text) — magit section-heading fold arrows. ---
        for glyph in &frame.glyphs {
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
                let Some(bitmap) = frame.fringe_bitmaps.get(bitmap_index) else {
                    continue;
                };
                let face = frame.resolved_face(*face_id);
                self.render_fringe_bitmap(
                    &mut fringe_vertices,
                    *x + offset_x,
                    *y + offset_y,
                    *width,
                    *height,
                    &face.fg,
                    bitmap,
                );
            }
        }

        // --- Collect cursors, borders, scroll bars ---
        for glyph in &frame.glyphs {
            match glyph {
                FrameGlyph::Border {
                    x,
                    y,
                    width,
                    height,
                    color,
                    ..
                } => {
                    self.add_rect(
                        &mut cursor_vertices,
                        *x + offset_x,
                        *y + offset_y,
                        *width,
                        *height,
                        color,
                    );
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
                    // Track
                    self.add_rect(
                        &mut cursor_vertices,
                        *x + offset_x,
                        *y + offset_y,
                        *width,
                        *height,
                        track_color,
                    );
                    // Thumb (rounded)
                    let (tx, ty, tw, th) = if *horizontal {
                        (
                            *x + offset_x + *thumb_start,
                            *y + offset_y,
                            *thumb_size,
                            *height,
                        )
                    } else {
                        (
                            *x + offset_x,
                            *y + offset_y + *thumb_start,
                            *width,
                            *thumb_size,
                        )
                    };
                    let radius = tw.min(th) * self.effects.scroll_bar.thumb_radius;
                    scroll_bar_thumbs.push((tx, ty, tw, th, radius, *thumb_color));
                }
                _ => {}
            }
        }

        let animated_cursor_with_offset = animated_cursor.map(|animated| AnimatedCursor {
            x: animated.x + offset_x,
            y: animated.y + offset_y,
            corners: animated
                .corners
                .map(|corners| corners.map(|(x, y)| (x + offset_x, y + offset_y))),
            ..animated
        });

        // One entry per window (selected window's entry is `active`); draw each.
        for cursor in &frame.window_cursors {
            if !cursor_visible && !cursor.style.is_hollow() {
                continue;
            }

            let (target_x, target_y, target_width, target_height) = frame.cursor_draw_rect(
                cursor.slot_id,
                cursor.style,
                cursor.ascent,
                (cursor.x, cursor.y, cursor.width, cursor.height),
            );
            let destination = Rect::new(
                target_x + offset_x,
                target_y + offset_y,
                target_width,
                target_height,
            );
            let (gx, gy, gw, gh) = if !cursor.style.is_hollow() {
                if let Some(ref ac) = animated_cursor_with_offset {
                    if ac.window_id == cursor.window_id {
                        (ac.x, ac.y, ac.width, ac.height)
                    } else {
                        (
                            destination.x,
                            destination.y,
                            destination.width,
                            destination.height,
                        )
                    }
                } else {
                    (
                        destination.x,
                        destination.y,
                        destination.width,
                        destination.height,
                    )
                }
            } else {
                (
                    destination.x,
                    destination.y,
                    destination.width,
                    destination.height,
                )
            };

            match cursor.style {
                CursorStyle::FilledBox => {
                    let paint = PresentedCursorPaint::resolve(
                        ResolvedCursorPaint::new(cursor.color, cursor.cursor_fg),
                        CursorColorPolicy::Inherit,
                        self.frame_sample.presentation_time(),
                    );
                    let presentation = FilledBoxPresentation::resolve(
                        cursor.window_id,
                        cursor.slot_id,
                        destination,
                        animated_cursor_with_offset.as_ref(),
                        paint,
                    );
                    match presentation {
                        FilledBoxPresentation::Settled { rect, .. } => self.add_rect(
                            &mut cursor_bg_vertices,
                            rect.x,
                            rect.y,
                            rect.width,
                            rect.height,
                            &paint.body_background,
                        ),
                        FilledBoxPresentation::InFlight { shape, .. } => match shape {
                            CursorShape::Rect(rect) => self.add_rect(
                                &mut cursor_bg_vertices,
                                rect.x,
                                rect.y,
                                rect.width,
                                rect.height,
                                &paint.body_background,
                            ),
                            CursorShape::Quad(corners) => self.add_quad(
                                &mut cursor_bg_vertices,
                                &corners,
                                &paint.body_background,
                            ),
                        },
                    }
                    if cursor.active {
                        cursor_inverse_video = presentation.inverse_video();
                    }
                }
                CursorStyle::Bar(bar_w) => {
                    self.add_rect(&mut cursor_vertices, gx, gy, bar_w, gh, &cursor.color);
                }
                CursorStyle::Hbar(hbar_h) => {
                    self.add_rect(
                        &mut cursor_vertices,
                        gx,
                        gy + gh - hbar_h,
                        gw,
                        hbar_h,
                        &cursor.color,
                    );
                }
                CursorStyle::Hollow => {
                    self.add_rect(&mut cursor_vertices, gx, gy, gw, 1.0, &cursor.color);
                    self.add_rect(
                        &mut cursor_vertices,
                        gx,
                        gy + gh - 1.0,
                        gw,
                        1.0,
                        &cursor.color,
                    );
                    self.add_rect(&mut cursor_vertices, gx, gy, 1.0, gh, &cursor.color);
                    self.add_rect(
                        &mut cursor_vertices,
                        gx + gw - 1.0,
                        gy,
                        1.0,
                        gh,
                        &cursor.color,
                    );
                }
            }
        }

        // --- Step 2: Collect text glyphs (with overstrike and composed) ---
        let mut mask_data: Vec<(AnyAtlasEntry, [GlyphVertex; 6])> = Vec::new();
        let mut subpixel_data: Vec<(AnyAtlasEntry, [SubpixelGlyphVertex; 6])> = Vec::new();
        let mut color_data: Vec<(AnyAtlasEntry, [GlyphVertex; 6])> = Vec::new();
        let enable_subpixel = glyph_atlas.subpixel_enabled();

        let mut text_face_cache: Option<(FaceId, MaterializedFaceData)> = None;
        for (glyph_index, glyph) in frame.glyphs.iter().enumerate() {
            if let FrameGlyph::Char {
                char: ch,
                composed,
                x,
                y,
                baseline,
                width,
                height,
                ascent: _,
                face_id,
                clip_rect,
                ..
            } = glyph
            {
                // Resolve the face-derived attributes (fg/bg/font_size/overstrike)
                // that used to be inlined on the glyph. A one-entry cache reuses
                // the resolve across runs of glyphs sharing a face.
                for paint in pointer_override.face_paints(
                    glyph_index,
                    *face_id,
                    Rect::new(*x, *y, *width, *height),
                    clip_rect.as_ref(),
                ) {
                    let face_id = paint.face_id();
                    let rf = match text_face_cache {
                        Some((id, ref data)) if id == face_id => *data,
                        _ => {
                            let data = frame.resolved_face(face_id);
                            text_face_cache = Some((face_id, data));
                            data
                        }
                    };
                    let fg = &rf.fg;
                    let bg: Option<Color> = Some(rf.bg);
                    let font_size = rf.font_size;
                    let overstrike = rf.overstrike;

                    let face = faces.get(&face_id);

                    // Snap glyph origins to integer physical pixels (see
                    // `snap_glyph_origin`): the atlas cache key must be independent
                    // of subpixel position or each glyph re-rasterizes per column and
                    // per fractional scroll offset.
                    let sf = self.scale_factor;
                    let phys_x = (*x + offset_x) * sf;
                    let baseline_y = *baseline + offset_y;
                    let phys_y = baseline_y * sf;
                    let (x_int, y_int, x_bin, y_bin) = snap_glyph_origin(phys_x, phys_y);
                    let font_identity = glyph_atlas.glyph_font_identity_for_char(face, *ch);

                    let subpixel_request = if enable_subpixel {
                        SubpixelRequest::Enabled
                    } else {
                        SubpixelRequest::Disabled
                    };
                    let handles = if let Some(text) = composed {
                        stats.text_glyphs += 1;
                        stats.composed_glyphs += 1;
                        seen_composed_keys.insert(ComposedGlyphKey {
                            text: text.clone(),
                            face_id,
                            font_size_bits: font_size.to_bits(),
                            font_identity,
                            glyph_stream_identity: glyph_atlas
                                .glyph_stream_identity_for_composed(face, text),
                            x_bin,
                            y_bin,
                        });
                        glyph_atlas
                            .get_or_create_composed_atlas(
                                &self.device,
                                &self.queue,
                                text,
                                face_id,
                                font_size.to_bits(),
                                face,
                                x_bin,
                                y_bin,
                                subpixel_request,
                            )
                            .unwrap_or_default()
                    } else {
                        stats.text_glyphs += 1;
                        let key = GlyphKey {
                            charcode: *ch as u32,
                            face_id,
                            font_size_bits: font_size.to_bits(),
                            font_identity,
                            x_bin,
                            y_bin,
                        };
                        seen_single_keys.insert(key.clone());
                        glyph_atlas
                            .get_or_create_atlas(
                                &self.device,
                                &self.queue,
                                &key,
                                face,
                                subpixel_request,
                            )
                            .into_iter()
                            .collect()
                    };

                    for handle in handles {
                        let entry = handle.entry;
                        let metrics = entry.metrics();
                        let uv = entry.uv();
                        let content_rect = entry.rect();
                        let base_u_min = uv.min()[0];
                        let base_u_max = uv.max()[0];
                        let base_v_min = uv.min()[1];
                        let base_v_max = uv.max()[1];
                        let base_glyph_x = (x_int as f32 + metrics.bearing_x) / sf;
                        let base_glyph_y = (y_int as f32 - metrics.bearing_y) / sf;
                        let base_glyph_w = content_rect.width() as f32 / sf;
                        let base_glyph_h = content_rect.height() as f32 / sf;
                        let effective_clip = paint.clip().map(|clip| Rect {
                            x: clip.x + offset_x,
                            y: clip.y + offset_y,
                            width: clip.width,
                            height: clip.height,
                        });
                        let Some(clipped) = clipped_media_rect(
                            base_glyph_x,
                            base_glyph_y,
                            base_glyph_w,
                            base_glyph_h,
                            effective_clip.as_ref(),
                        ) else {
                            continue;
                        };
                        let glyph_x = clipped.draw_x;
                        let glyph_y = clipped.draw_y;
                        let glyph_w = clipped.draw_width;
                        let glyph_h = clipped.draw_height;
                        let tex_u_min = base_u_min + (base_u_max - base_u_min) * clipped.u_min;
                        let tex_u_max = base_u_min + (base_u_max - base_u_min) * clipped.u_max;
                        let tex_v_min = base_v_min + (base_v_max - base_v_min) * clipped.v_min;
                        let tex_v_max = base_v_min + (base_v_max - base_v_min) * clipped.v_max;

                        let mut effective_fg = *fg;
                        let mut effective_bg =
                            WgpuRenderer::sample_face_paint_background(face, bg, paint)
                                .unwrap_or(Color::rgb(1.0, 1.0, 1.0));
                        if cursor_visible
                            && let Some(inverse) = cursor_inverse_video
                            && glyph.slot_id().is_some_and(|slot| slot == inverse.slot_id)
                        {
                            effective_fg = inverse.paint.glyph_foreground;
                            effective_bg = inverse.paint.body_background;
                        }

                        let is_color = matches!(entry, AnyAtlasEntry::Color(_));
                        let color = if is_color {
                            [1.0, 1.0, 1.0, 1.0]
                        } else {
                            [
                                effective_fg.r,
                                effective_fg.g,
                                effective_fg.b,
                                effective_fg.a,
                            ]
                        };
                        let subpixel_fg =
                            subpixel_foreground_color(effective_bg, effective_fg, 1.0);
                        let subpixel_bg = subpixel_background_color(effective_bg);

                        let vertices = [
                            GlyphVertex {
                                position: [glyph_x, glyph_y],
                                tex_coords: [tex_u_min, tex_v_min],
                                color,
                            },
                            GlyphVertex {
                                position: [glyph_x + glyph_w, glyph_y],
                                tex_coords: [tex_u_max, tex_v_min],
                                color,
                            },
                            GlyphVertex {
                                position: [glyph_x + glyph_w, glyph_y + glyph_h],
                                tex_coords: [tex_u_max, tex_v_max],
                                color,
                            },
                            GlyphVertex {
                                position: [glyph_x, glyph_y],
                                tex_coords: [tex_u_min, tex_v_min],
                                color,
                            },
                            GlyphVertex {
                                position: [glyph_x + glyph_w, glyph_y + glyph_h],
                                tex_coords: [tex_u_max, tex_v_max],
                                color,
                            },
                            GlyphVertex {
                                position: [glyph_x, glyph_y + glyph_h],
                                tex_coords: [tex_u_min, tex_v_max],
                                color,
                            },
                        ];

                        let overstrike_vertices = if overstrike {
                            let ox = 1.0 / sf;
                            super::pointer_override::clip_glyph_quad(
                                [
                                    GlyphVertex {
                                        position: [glyph_x + ox, glyph_y],
                                        tex_coords: [tex_u_min, tex_v_min],
                                        color,
                                    },
                                    GlyphVertex {
                                        position: [glyph_x + ox + glyph_w, glyph_y],
                                        tex_coords: [tex_u_max, tex_v_min],
                                        color,
                                    },
                                    GlyphVertex {
                                        position: [glyph_x + ox + glyph_w, glyph_y + glyph_h],
                                        tex_coords: [tex_u_max, tex_v_max],
                                        color,
                                    },
                                    GlyphVertex {
                                        position: [glyph_x + ox, glyph_y],
                                        tex_coords: [tex_u_min, tex_v_min],
                                        color,
                                    },
                                    GlyphVertex {
                                        position: [glyph_x + ox + glyph_w, glyph_y + glyph_h],
                                        tex_coords: [tex_u_max, tex_v_max],
                                        color,
                                    },
                                    GlyphVertex {
                                        position: [glyph_x + ox, glyph_y + glyph_h],
                                        tex_coords: [tex_u_min, tex_v_max],
                                        color,
                                    },
                                ],
                                effective_clip.as_ref(),
                            )
                        } else {
                            None
                        };

                        let subpixel_vertices = build_subpixel_vertices(
                            glyph_x,
                            glyph_y,
                            glyph_w,
                            glyph_h,
                            tex_u_min,
                            tex_u_max,
                            tex_v_min,
                            tex_v_max,
                            subpixel_fg,
                            subpixel_bg,
                        );

                        let overstrike_subpixel_vertices = if overstrike {
                            let ox = 1.0 / sf;
                            super::pointer_override::clip_subpixel_quad(
                                build_subpixel_vertices(
                                    glyph_x + ox,
                                    glyph_y,
                                    glyph_w,
                                    glyph_h,
                                    tex_u_min,
                                    tex_u_max,
                                    tex_v_min,
                                    tex_v_max,
                                    subpixel_fg,
                                    subpixel_bg,
                                ),
                                effective_clip.as_ref(),
                            )
                        } else {
                            None
                        };

                        if is_color {
                            color_data.push((entry, vertices));
                            if let Some(ov) = overstrike_vertices {
                                color_data.push((entry, ov));
                            }
                        } else if matches!(entry, AnyAtlasEntry::Subpixel(_)) {
                            subpixel_data.push((entry, subpixel_vertices));
                            if let Some(ov) = overstrike_subpixel_vertices {
                                subpixel_data.push((entry, ov));
                            }
                        } else {
                            mask_data.push((entry, vertices));
                            if let Some(ov) = overstrike_vertices {
                                mask_data.push((entry, ov));
                            }
                        }
                    }
                }
            }
        }

        // --- Step 3: Collect decorations (underline, overline, strikethrough) ---
        let mut decoration_vertices: Vec<RectVertex> = Vec::new();
        let mut deco_face_cache: Option<(FaceId, MaterializedFaceData)> = None;
        for (glyph_index, glyph) in frame.glyphs.iter().enumerate() {
            if let FrameGlyph::Char {
                x,
                y,
                baseline,
                width,
                height,
                ascent,
                face_id,
                clip_rect,
                ..
            } = glyph
            {
                for paint in pointer_override.face_paints(
                    glyph_index,
                    *face_id,
                    Rect::new(*x, *y, *width, *height),
                    clip_rect.as_ref(),
                ) {
                    let face_id = paint.face_id();
                    let effective_clip = paint.clip().map(|clip| Rect {
                        x: clip.x + offset_x,
                        y: clip.y + offset_y,
                        width: clip.width,
                        height: clip.height,
                    });
                    let decoration_start = decoration_vertices.len();
                    let rf = match deco_face_cache {
                        Some((id, ref data)) if id == face_id => *data,
                        _ => {
                            let data = frame.resolved_face(face_id);
                            deco_face_cache = Some((face_id, data));
                            data
                        }
                    };
                    let fg = &rf.fg;
                    let underline = &rf.underline;
                    let underline_color = &rf.underline_color;
                    let strike_through = &rf.strike_through;
                    let strike_through_color = &rf.strike_through_color;
                    let overline = &rf.overline;
                    let overline_color = &rf.overline_color;

                    let gx = *x + offset_x;
                    let gy = *y + offset_y;
                    let baseline_y = *baseline + offset_y;

                    // Per-face font metrics for underline positioning
                    let (ul_position, ul_thick) = frame
                        .faces
                        .get(&face_id)
                        .map(|f| (f.underline_placement, f.underline_thickness as f32))
                        .unwrap_or_default();

                    // Underline
                    if *underline != UnderlineStyle::None {
                        let ul_color = underline_color.as_ref().unwrap_or(fg);
                        let geometry = ul_position.resolve(gy, *height, baseline_y, ul_thick);
                        let ul_y = geometry.top_y;
                        let line_thickness = geometry.thickness;

                        match *underline {
                            UnderlineStyle::Line => {
                                // Single solid line
                                self.add_rect(
                                    &mut decoration_vertices,
                                    gx,
                                    ul_y,
                                    *width,
                                    line_thickness,
                                    ul_color,
                                );
                            }
                            UnderlineStyle::Wave => {
                                // Wave underline
                                let amplitude: f32 = 2.0;
                                let wavelength: f32 = 8.0;
                                let seg_w: f32 = 1.0;
                                let mut cx = gx;
                                while cx < gx + *width {
                                    let sw = seg_w.min(gx + *width - cx);
                                    let phase = (cx - gx) * std::f32::consts::TAU / wavelength;
                                    let wave_offset = phase.sin() * amplitude;
                                    self.add_rect(
                                        &mut decoration_vertices,
                                        cx,
                                        ul_y + wave_offset,
                                        sw,
                                        line_thickness,
                                        ul_color,
                                    );
                                    cx += seg_w;
                                }
                            }
                            UnderlineStyle::Double => {
                                // Double line
                                self.add_rect(
                                    &mut decoration_vertices,
                                    gx,
                                    ul_y,
                                    *width,
                                    line_thickness,
                                    ul_color,
                                );
                                self.add_rect(
                                    &mut decoration_vertices,
                                    gx,
                                    ul_y + line_thickness + 1.0,
                                    *width,
                                    line_thickness,
                                    ul_color,
                                );
                            }
                            UnderlineStyle::Dotted => {
                                // Dotted
                                let mut cx = gx;
                                while cx < gx + *width {
                                    let dw = line_thickness.min(gx + *width - cx);
                                    self.add_rect(
                                        &mut decoration_vertices,
                                        cx,
                                        ul_y,
                                        dw,
                                        line_thickness,
                                        ul_color,
                                    );
                                    cx += line_thickness + 2.0;
                                }
                            }
                            UnderlineStyle::Dashed => {
                                // Dashed
                                let mut cx = gx;
                                while cx < gx + *width {
                                    let dw = 4.0_f32.min(gx + *width - cx);
                                    self.add_rect(
                                        &mut decoration_vertices,
                                        cx,
                                        ul_y,
                                        dw,
                                        line_thickness,
                                        ul_color,
                                    );
                                    cx += 7.0;
                                }
                            }
                            // None reaches here only for an out-of-range code (the
                            // `*underline > 0` guard excludes a real None): fall back
                            // to a single solid line.
                            UnderlineStyle::None => {
                                self.add_rect(
                                    &mut decoration_vertices,
                                    gx,
                                    ul_y,
                                    *width,
                                    line_thickness,
                                    ul_color,
                                );
                            }
                        }
                    }

                    // Overline
                    if *overline {
                        let ol_color = overline_color.as_ref().unwrap_or(fg);
                        self.add_rect(
                            &mut decoration_vertices,
                            gx,
                            gy,
                            *width,
                            ul_thick.max(1.0),
                            ol_color,
                        );
                    }

                    // Strikethrough
                    if *strike_through {
                        let st_color = strike_through_color.as_ref().unwrap_or(fg);
                        let st_y = baseline_y - *ascent / 3.0;
                        self.add_rect(
                            &mut decoration_vertices,
                            gx,
                            st_y,
                            *width,
                            ul_thick.max(1.0),
                            st_color,
                        );
                    }
                    super::pointer_override::clip_new_rect_vertices(
                        &mut decoration_vertices,
                        decoration_start,
                        effective_clip.as_ref(),
                    );
                }
            }
        }
        for (glyph_index, glyph) in frame.glyphs.iter().enumerate() {
            let FrameGlyph::Stretch {
                x,
                y,
                width,
                height,
                face_id,
                clip_rect,
                ..
            } = glyph
            else {
                continue;
            };
            for paint in pointer_override.face_paints(
                glyph_index,
                *face_id,
                Rect::new(*x, *y, *width, *height),
                clip_rect.as_ref(),
            ) {
                let face_id = paint.face_id();
                let Some(face) = frame.faces.get(&face_id) else {
                    continue;
                };
                let effective_clip = paint.clip().map(|clip| Rect {
                    x: clip.x + offset_x,
                    y: clip.y + offset_y,
                    width: clip.width,
                    height: clip.height,
                });
                let decoration_start = decoration_vertices.len();
                for rect in stretch_decoration_rects(face, *x + offset_x, *y + offset_y, *width) {
                    self.add_rect(
                        &mut decoration_vertices,
                        rect.x,
                        rect.y,
                        rect.width,
                        rect.height,
                        &rect.color,
                    );
                }
                super::pointer_override::clip_new_rect_vertices(
                    &mut decoration_vertices,
                    decoration_start,
                    effective_clip.as_ref(),
                );
            }
        }

        // --- Step 4: Box borders (sharp and rounded, from merged spans) ---
        let mut sharp_border_vertices: Vec<RectVertex> = Vec::new();
        let mut rounded_border_vertices: Vec<RoundedRectVertex> = Vec::new();
        let mut rounded_fill_vertices: Vec<RoundedRectVertex> = Vec::new();

        for span in box_spans {
            if let Some(face) = faces.get(&span.face_id) {
                self.append_required_box_background_fill_geometry(
                    &mut bg_vertices,
                    &span,
                    offset_x,
                    offset_y,
                );
                self.append_rounded_box_fill_geometry(
                    &mut rounded_fill_vertices,
                    &span,
                    face,
                    offset_x,
                    offset_y,
                );
                self.append_box_border_geometry(
                    &mut sharp_border_vertices,
                    &mut rounded_border_vertices,
                    &span,
                    face,
                    device_scale,
                    offset_x,
                    offset_y,
                );
            }
        }

        #[cfg(feature = "video")]
        let mut submitted_video_ids = Vec::new();
        #[cfg(feature = "video")]
        let gpu_timing = self.begin_video_frame_content_pass(
            frame
                .glyphs
                .iter()
                .any(|glyph| matches!(glyph, FrameGlyph::Video { .. })),
        );
        #[cfg(feature = "video")]
        let timestamp_writes = gpu_timing.as_ref().map(|timing| timing.timestamp_writes());
        #[cfg(not(feature = "video"))]
        let timestamp_writes = None;

        // === GPU submission: single encoder, single submit ===
        // Select pipelines: stencil-aware variants when clipping to rounded corners
        let use_stencil = clip_corner_radius > 0.0;
        let rect_pl = if use_stencil {
            &self.pipelines.stencil_rect
        } else {
            &self.pipelines.rect
        };
        let rounded_rect_pl = if use_stencil {
            &self.pipelines.stencil_rounded_rect
        } else {
            &self.pipelines.rounded_rect
        };
        let glyph_pl = if use_stencil {
            &self.pipelines.stencil_glyph
        } else {
            &self.pipelines.glyph
        };
        let subpixel_pl = if use_stencil {
            &self.pipelines.stencil_subpixel_glyph
        } else {
            &self.pipelines.subpixel_glyph
        };
        let image_pl = if use_stencil {
            &self.pipelines.stencil_image
        } else {
            &self.pipelines.image
        };
        #[cfg(feature = "video")]
        let bi_planar_video_pl = if use_stencil {
            &self.pipelines.stencil_bi_planar_video
        } else {
            &self.pipelines.bi_planar_video
        };
        let _opaque_image_pl = if use_stencil {
            &self.pipelines.stencil_opaque_image
        } else {
            &self.pipelines.opaque_image
        };

        let stencil_attachment = if use_stencil {
            Some(wgpu::RenderPassDepthStencilAttachment {
                view: self.stencil.texture.view(),
                depth_ops: None,
                stencil_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                }),
            })
        } else {
            None
        };

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Frame Content Encoder"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Frame Content Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: stencil_attachment,
                timestamp_writes,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            if let Some((x, y, width, height)) = scissor {
                pass.set_scissor_rect(x, y, width, height);
            }

            if use_stencil {
                pass.set_stencil_reference(1);
            }

            // --- Draw backgrounds ---
            if let Some(upload) = self
                .arenas
                .rect
                .upload(&self.device, &self.queue, &bg_vertices)
            {
                pass.set_pipeline(rect_pl);
                pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                pass.set_vertex_buffer(0, upload.buffer_slice());
                pass.draw(0..bg_vertices.len() as u32, 0..1);
            }

            // --- Draw fringe bitmaps (own column, above backgrounds, below
            // text — fringes never overlap the text area). ---
            if let Some(upload) =
                self.arenas
                    .rect
                    .upload(&self.device, &self.queue, &fringe_vertices)
            {
                pass.set_pipeline(rect_pl);
                pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                pass.set_vertex_buffer(0, upload.buffer_slice());
                pass.draw(0..fringe_vertices.len() as u32, 0..1);
            }

            // Filled-box cursor backgrounds must be below text so the covered
            // glyph can be drawn with inverse foreground on top.
            if let Some(upload) =
                self.arenas
                    .rect
                    .upload(&self.device, &self.queue, &cursor_bg_vertices)
            {
                pass.set_pipeline(rect_pl);
                pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                pass.set_vertex_buffer(0, upload.buffer_slice());
                pass.draw(0..cursor_bg_vertices.len() as u32, 0..1);
            }

            // --- Draw rounded box fills ---
            if let Some(upload) =
                self.arenas
                    .rounded
                    .upload(&self.device, &self.queue, &rounded_fill_vertices)
            {
                pass.set_pipeline(rounded_rect_pl);
                pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                pass.set_vertex_buffer(0, upload.buffer_slice());
                pass.draw(0..rounded_fill_vertices.len() as u32, 0..1);
            }

            // GNU draws character/composition box relief before the glyph so
            // thick inset borders cannot cover a narrow character cell.
            if let Some(upload) =
                self.arenas
                    .rect
                    .upload(&self.device, &self.queue, &sharp_border_vertices)
            {
                pass.set_pipeline(rect_pl);
                pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                pass.set_vertex_buffer(0, upload.buffer_slice());
                pass.draw(0..sharp_border_vertices.len() as u32, 0..1);
            }
            if let Some(upload) =
                self.arenas
                    .rounded
                    .upload(&self.device, &self.queue, &rounded_border_vertices)
            {
                pass.set_pipeline(rounded_rect_pl);
                pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                pass.set_vertex_buffer(0, upload.buffer_slice());
                pass.draw(0..rounded_border_vertices.len() as u32, 0..1);
            }

            // --- Draw mask text glyphs ---
            if !mask_data.is_empty() {
                let all_vertices: Vec<GlyphVertex> = mask_data
                    .iter()
                    .flat_map(|(_, verts)| verts.iter().copied())
                    .collect();

                let mask_upload =
                    self.arenas
                        .glyph
                        .upload(&self.device, &self.queue, &all_vertices);
                stats.glyph_vertex_buffer_creations += 1;

                pass.set_pipeline(glyph_pl);
                pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                if let Some(ref upload) = mask_upload {
                    pass.set_vertex_buffer(0, self.arenas.glyph.slice(upload));
                }

                let mut i = 0;
                while i < mask_data.len() {
                    let (entry, _) = &mask_data[i];
                    let page_id = entry.binding_id_value();
                    let batch_start = i;
                    i += 1;
                    while i < mask_data.len() && mask_data[i].0.binding_id_value() == page_id {
                        i += 1;
                    }
                    let bg = match glyph_atlas.atlas_bind_group(*entry) {
                        Ok(bg) => bg,
                        Err(err) => {
                            tracing::warn!(?err, "skipping stale content mask glyph batch");
                            continue;
                        }
                    };
                    let vert_start = (batch_start * 6) as u32;
                    let vert_end = (i * 6) as u32;
                    pass.set_bind_group(1, bg, &[]);
                    stats.glyph_bind_group_changes += 1;
                    pass.draw(vert_start..vert_end, 0..1);
                    stats.glyph_draw_calls += 1;
                }
            }

            // --- Draw subpixel LCD text glyphs ---
            if !subpixel_data.is_empty() {
                let all_vertices: Vec<SubpixelGlyphVertex> = subpixel_data
                    .iter()
                    .flat_map(|(_, verts)| verts.iter().copied())
                    .collect();

                let subpixel_upload =
                    self.arenas
                        .subpixel
                        .upload(&self.device, &self.queue, &all_vertices);
                stats.glyph_vertex_buffer_creations += 1;

                pass.set_pipeline(subpixel_pl);
                pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                if let Some(ref upload) = subpixel_upload {
                    pass.set_vertex_buffer(0, self.arenas.subpixel.slice(upload));
                }

                let mut i = 0;
                while i < subpixel_data.len() {
                    let (entry, _) = &subpixel_data[i];
                    let page_id = entry.binding_id_value();
                    let batch_start = i;
                    i += 1;
                    while i < subpixel_data.len()
                        && subpixel_data[i].0.binding_id_value() == page_id
                    {
                        i += 1;
                    }
                    let bg = match glyph_atlas.atlas_bind_group(*entry) {
                        Ok(bg) => bg,
                        Err(err) => {
                            tracing::warn!(?err, "skipping stale content subpixel glyph batch");
                            continue;
                        }
                    };
                    let vert_start = (batch_start * 6) as u32;
                    let vert_end = (i * 6) as u32;
                    pass.set_bind_group(1, bg, &[]);
                    stats.glyph_bind_group_changes += 1;
                    pass.draw(vert_start..vert_end, 0..1);
                    stats.glyph_draw_calls += 1;
                }
            }

            // --- Draw color text glyphs (emoji) ---
            if !color_data.is_empty() {
                let all_vertices: Vec<GlyphVertex> = color_data
                    .iter()
                    .flat_map(|(_, verts)| verts.iter().copied())
                    .collect();

                let color_upload =
                    self.arenas
                        .glyph
                        .upload(&self.device, &self.queue, &all_vertices);
                stats.glyph_vertex_buffer_creations += 1;

                pass.set_pipeline(image_pl);
                pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                if let Some(ref upload) = color_upload {
                    pass.set_vertex_buffer(0, self.arenas.glyph.slice(upload));
                }

                let mut i = 0;
                while i < color_data.len() {
                    let (entry, _) = &color_data[i];
                    let page_id = entry.binding_id_value();
                    let batch_start = i;
                    i += 1;
                    while i < color_data.len() && color_data[i].0.binding_id_value() == page_id {
                        i += 1;
                    }
                    let bg = match glyph_atlas.atlas_bind_group(*entry) {
                        Ok(bg) => bg,
                        Err(err) => {
                            tracing::warn!(?err, "skipping stale content color glyph batch");
                            continue;
                        }
                    };
                    let vert_start = (batch_start * 6) as u32;
                    let vert_end = (i * 6) as u32;
                    pass.set_bind_group(1, bg, &[]);
                    stats.glyph_bind_group_changes += 1;
                    pass.draw(vert_start..vert_end, 0..1);
                    stats.glyph_draw_calls += 1;
                }
            }

            // --- Draw text decorations ---
            if let Some(upload) =
                self.arenas
                    .rect
                    .upload(&self.device, &self.queue, &decoration_vertices)
            {
                pass.set_pipeline(rect_pl);
                pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                pass.set_vertex_buffer(0, upload.buffer_slice());
                pass.draw(0..decoration_vertices.len() as u32, 0..1);
            }

            // --- Draw inline images ---
            pass.set_pipeline(image_pl);
            pass.set_bind_group(0, &self.uniform_bind_group, &[]);

            let mut image_quads = Vec::new();
            let mut relief_vertices = Vec::new();
            for (glyph_index, glyph) in frame.glyphs.iter().enumerate() {
                if let FrameGlyph::Image {
                    image_id,
                    source_rect,
                    x,
                    y,
                    width,
                    height,
                    clip_rect,
                    ..
                } = glyph
                    && self.caches.image.get(*image_id).is_some()
                {
                    let effective_clip = *clip_rect;
                    let Some(clipped) =
                        clipped_media_rect(*x, *y, *width, *height, effective_clip.as_ref())
                    else {
                        continue;
                    };
                    let ix = clipped.draw_x + offset_x;
                    let iy = clipped.draw_y + offset_y;
                    let (u_min, v_min) = source_rect.map_uv(clipped.u_min, clipped.v_min);
                    let (u_max, v_max) = source_rect.map_uv(clipped.u_max, clipped.v_max);
                    tracing::debug!(
                        "render_frame_content: image {} at ({:.1},{:.1}) size {:.1}x{:.1}",
                        image_id,
                        ix,
                        iy,
                        width,
                        height,
                    );
                    image_quads.push(MediaQuad {
                        id: *image_id,
                        vertices: textured_quad_vertices_uv(
                            ix,
                            iy,
                            clipped.draw_width,
                            clipped.draw_height,
                            u_min,
                            u_max,
                            v_min,
                            v_max,
                        ),
                    });
                    if let Some(paint) = pointer_override.image_override(glyph_index)
                        && let neomacs_display_protocol::PointerDrawMode::ImageRelief(relief) =
                            paint.mode()
                    {
                        let relief_clip = pointer_override
                            .image_clip(glyph_index, clip_rect.as_ref())
                            .map(|clip| Rect {
                                x: clip.x + offset_x,
                                y: clip.y + offset_y,
                                width: clip.width,
                                height: clip.height,
                            });
                        super::pointer_override::append_clipped_relief(
                            &mut relief_vertices,
                            *x + offset_x,
                            *y + offset_y,
                            *width,
                            *height,
                            relief,
                            relief_clip.as_ref(),
                        );
                    }
                }
            }
            let image_vertices: Vec<GlyphVertex> = image_quads
                .iter()
                .flat_map(|quad| quad.vertices.iter().copied())
                .collect();
            if let Some(upload) =
                self.arenas
                    .image
                    .upload(&self.device, &self.queue, &image_vertices)
            {
                pass.set_vertex_buffer(0, upload.buffer_slice());
                for (i, quad) in image_quads.iter().enumerate() {
                    if let Some(cached) = self.caches.image.get(quad.id) {
                        pass.set_bind_group(1, &cached.bind_group, &[]);
                        pass.draw((i * 6) as u32..(i * 6 + 6) as u32, 0..1);
                    }
                }
            }
            if let Some(upload) =
                self.arenas
                    .rect
                    .upload(&self.device, &self.queue, &relief_vertices)
            {
                pass.set_pipeline(rect_pl);
                pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                pass.set_vertex_buffer(0, upload.buffer_slice());
                pass.draw(0..relief_vertices.len() as u32, 0..1);
            }
            // Inline videos below inherit the image pipeline.
            pass.set_pipeline(image_pl);
            pass.set_bind_group(0, &self.uniform_bind_group, &[]);

            // --- Draw inline videos (inherit the image pipeline set above) ---
            #[cfg(feature = "video")]
            {
                if let Some(prepared) = super::layer_media::prepare_inline_videos(
                    &self.caches.video,
                    &frame.glyphs,
                    offset_x,
                    offset_y,
                ) {
                    self.media_budget.touch(
                        crate::media_budget::MediaType::Video,
                        crate::video_cache::VIDEO_GPU_POOL_ACCOUNTING_ID,
                    );
                    if let Some(upload) =
                        prepared.upload(&mut self.arenas.image, &self.device, &self.queue)
                    {
                        submitted_video_ids.extend(prepared.quads.iter().map(|quad| quad.id));
                        prepared.draw(&mut pass, &upload, image_pl, bi_planar_video_pl);
                    }
                }
            }

            // --- Draw inline shader surfaces (inherit the image pipeline) ---
            {
                let mut surface_quads = Vec::new();
                for glyph in &frame.glyphs {
                    if let FrameGlyph::Surface {
                        surface_id,
                        x,
                        y,
                        width,
                        height,
                        ..
                    } = glyph
                        && self.caches.surface.get(surface_id.get()).is_some()
                    {
                        self.caches.surface.mark_drawn(surface_id.get());
                        surface_quads.push(MediaQuad {
                            id: surface_id.get(),
                            vertices: super::layer_media::textured_quad_vertices(
                                *x + offset_x,
                                *y + offset_y,
                                *width,
                                *height,
                                0.0,
                                1.0,
                            ),
                        });
                    }
                }
                let surface_vertices: Vec<GlyphVertex> = surface_quads
                    .iter()
                    .flat_map(|quad| quad.vertices.iter().copied())
                    .collect();
                if let Some(upload) =
                    self.arenas
                        .image
                        .upload(&self.device, &self.queue, &surface_vertices)
                {
                    pass.set_vertex_buffer(0, upload.buffer_slice());
                    for (i, quad) in surface_quads.iter().enumerate() {
                        if let Some(cached) = self.caches.surface.get(quad.id) {
                            pass.set_bind_group(1, &cached.composite_bind_group, &[]);
                            pass.draw((i * 6) as u32..(i * 6 + 6) as u32, 0..1);
                        }
                    }
                }
            }

            // --- Draw inline webkit views ---
            #[cfg(all(feature = "webview", target_os = "linux"))]
            {
                pass.set_pipeline(_opaque_image_pl);
                pass.set_bind_group(0, &self.uniform_bind_group, &[]);

                let mut webkit_quads = Vec::new();
                for glyph in &frame.glyphs {
                    if let Some(quad) = inline_webview_quad(glyph, offset_x, offset_y)
                        && self.caches.webview.get(quad.id).is_some()
                    {
                        webkit_quads.push(quad);
                    }
                }
                let webkit_vertices: Vec<GlyphVertex> = webkit_quads
                    .iter()
                    .flat_map(|quad| quad.vertices.iter().copied())
                    .collect();
                if let Some(upload) =
                    self.arenas
                        .image
                        .upload(&self.device, &self.queue, &webkit_vertices)
                {
                    pass.set_vertex_buffer(0, upload.buffer_slice());
                    for (i, quad) in webkit_quads.iter().enumerate() {
                        if let Some(cached) = self.caches.webview.get(quad.id) {
                            pass.set_bind_group(1, &cached.bind_group, &[]);
                            pass.draw((i * 6) as u32..(i * 6 + 6) as u32, 0..1);
                        }
                    }
                }
            }

            // --- Draw cursors and borders (on top of everything) ---
            if let Some(upload) =
                self.arenas
                    .rect
                    .upload(&self.device, &self.queue, &cursor_vertices)
            {
                pass.set_pipeline(rect_pl);
                pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                pass.set_vertex_buffer(0, upload.buffer_slice());
                pass.draw(0..cursor_vertices.len() as u32, 0..1);
            }

            // --- Draw scroll bar thumbs (rounded) ---
            if !scroll_bar_thumbs.is_empty() {
                let mut rounded_verts: Vec<RoundedRectVertex> = Vec::new();
                for (tx, ty, tw, th, radius, color) in &scroll_bar_thumbs {
                    self.add_rounded_rect(
                        &mut rounded_verts,
                        *tx,
                        *ty,
                        *tw,
                        *th,
                        0.0,
                        *radius,
                        color,
                    );
                }
                if let Some(upload) =
                    self.arenas
                        .rounded
                        .upload(&self.device, &self.queue, &rounded_verts)
                {
                    pass.set_pipeline(rounded_rect_pl);
                    pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    pass.set_vertex_buffer(0, upload.buffer_slice());
                    pass.draw(0..rounded_verts.len() as u32, 0..1);
                }
            }
        }

        stats.unique_single_glyph_keys = seen_single_keys.len();
        stats.unique_composed_glyph_keys = seen_composed_keys.len();
        stats.cache_hits = glyph_atlas.cache_hits_this_frame;
        stats.cache_misses = glyph_atlas.cache_misses_this_frame;
        stats.glyph_texture_uploads = glyph_atlas.cache_misses_this_frame;
        stats.log_if_enabled();

        #[cfg(feature = "video")]
        self.submit_video_frame_content_pass(encoder, gpu_timing, submitted_video_ids);
        #[cfg(not(feature = "video"))]
        self.queue.submit([encoder.finish()]);
        tracing::debug!("render_frame_content: submitted (1 encoder, 1 pass)");
    }

    /// Merge adjacent boxed glyphs into spans for proper border rendering.
    ///
    /// All box faces get span-merged. Rounded boxes (corner_radius > 0) get SDF
    /// treatment; standard boxes (corner_radius = 0) get rect borders.
    fn merge_box_spans(
        &self,
        frame: &FrameGlyphBuffer,
        pointer_override: &super::pointer_override::PointerOverrideResolver,
    ) -> Vec<BoxSpan> {
        collect_frame_box_spans(frame, &frame.faces, pointer_override)
    }
}

#[cfg(test)]
#[path = "content_test.rs"]
mod tests;
