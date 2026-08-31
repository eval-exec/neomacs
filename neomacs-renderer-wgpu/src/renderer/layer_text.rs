//! Text phases of `render_frame_glyphs` (z-order steps 4-6): box borders,
//! glyph batches, and text decorations, run once for buffer text and once for
//! overlay (mode-line/echo) text.

use neomacs_display_protocol::types::{FaceId, Rect};
use std::collections::HashSet;

use neomacs_display_protocol::face::{FaceAttributes, UnderlineStyle};
use neomacs_display_protocol::frame_glyphs::{FrameGlyph, MaterializedFaceData};
use neomacs_display_protocol::types::Color;

use super::super::glyph_atlas::{
    AnyAtlasEntry, ComposedGlyphKey, GlyphKey, SubpixelRequest, WgpuGlyphAtlas,
};
use super::super::vertex::{GlyphVertex, RectVertex, RoundedRectVertex, SubpixelGlyphVertex};
use super::frame_pass::{BoxSpanSet, FrameParams, FramePassCtx};
use super::glyphs::{
    CHAR_OVERLAP_MIN_AXIS, RenderedCharBounds, build_subpixel_vertices, color_is_grayscale,
    log_cursor_glyph_alignment, log_rendered_char_overlaps, subpixel_background_color,
    subpixel_foreground_color, trace_face_debug_enabled,
};
use super::row_reuse;
use super::{GlyphRenderStats, WgpuRenderer};

/// Per-pass glyph vertex batches keyed by atlas entry, split by pipeline.
pub(super) struct TextGlyphBatches {
    mask_data: Vec<(AnyAtlasEntry, [GlyphVertex; 6])>,
    subpixel_data: Vec<(AnyAtlasEntry, [SubpixelGlyphVertex; 6])>,
    color_data: Vec<(AnyAtlasEntry, [GlyphVertex; 6])>,
    rendered_char_bounds: Vec<RenderedCharBounds>,
}

impl WgpuRenderer {
    /// Draw text and overlay in correct z-order.
    ///
    /// For each overlay pass:
    ///   Pass 0 (non-overlay): draw buffer text (with cursor fg swap for inverse video)
    ///   Pass 1 (overlay): draw overlay backgrounds first, then overlay text
    ///
    /// This ensures: non-overlay bg -> cursor bg -> trail -> text -> overlay bg -> overlay text
    #[allow(clippy::too_many_arguments)]
    pub(super) fn draw_text_and_overlay_passes(
        &mut self,
        ctx: &mut FramePassCtx<'_, '_>,
        spans: &BoxSpanSet,
        overlay_rect_vertices: &[RectVertex],
        glyph_atlas: &mut WgpuGlyphAtlas,
        stats: &mut GlyphRenderStats,
        seen_single_keys: &mut HashSet<GlyphKey>,
        seen_composed_keys: &mut HashSet<ComposedGlyphKey>,
    ) {
        for overlay_pass in 0..2 {
            let want_overlay = overlay_pass == 1;

            // === Step 3: Draw overlay backgrounds before overlay text ===
            if want_overlay {
                self.draw_overlay_background_layer(ctx, spans, overlay_rect_vertices);
            }

            let batches = self.build_text_glyph_batches(
                ctx.params,
                want_overlay,
                glyph_atlas,
                seen_single_keys,
                seen_composed_keys,
                stats,
            );

            log_rendered_char_overlaps(
                ctx.params.frame_glyphs.frame_placement.frame().get(),
                if want_overlay { "overlay" } else { "text" },
                &batches.rendered_char_bounds,
            );
            log_cursor_glyph_alignment(
                ctx.params.frame_glyphs.frame_placement.frame().get(),
                if want_overlay { "overlay" } else { "text" },
                ctx.params.frame_glyphs,
                &batches.rendered_char_bounds,
            );

            self.draw_box_borders(ctx, want_overlay, spans);
            self.draw_text_glyph_batches(ctx, want_overlay, glyph_atlas, &batches, stats);
            self.draw_text_decorations(ctx, want_overlay);
        }

        // Both passes ran: promote this frame's captured rows to the cache.
        self.row_reuse.commit_frame();
    }

    /// Build the glyph vertex batches for one text pass.
    ///
    /// Rows whose layout damage says `Reused`/`ReusedShifted` and whose
    /// defensive keys all match splice last frame's cached vertex streams;
    /// everything else tessellates through the atlas as before. Assembly is
    /// per-row in glyph order, so the output is byte-identical to a full
    /// tessellation of the same frame.
    fn build_text_glyph_batches(
        &mut self,
        params: &FrameParams<'_>,
        want_overlay: bool,
        glyph_atlas: &mut WgpuGlyphAtlas,
        seen_single_keys: &mut HashSet<GlyphKey>,
        seen_composed_keys: &mut HashSet<ComposedGlyphKey>,
        stats: &mut GlyphRenderStats,
    ) -> TextGlyphBatches {
        let frame_glyphs = params.frame_glyphs;
        let face_debug_call_id = params.face_debug_call_id;
        if trace_face_debug_enabled() {
            tracing::info!(
                "face-debug call={} milestone=before_glyph_loop overlay={}",
                face_debug_call_id,
                want_overlay
            );
        }

        let frame_id = frame_glyphs.frame_placement.frame().get();
        let chunks = row_reuse::chunk_text_rows(&frame_glyphs.glyphs, want_overlay, frame_id);
        let window_origins = row_reuse::window_origin_bits(frame_glyphs);
        let cursor_row = frame_glyphs
            .active_cursor()
            .map(|cursor| (cursor.window_id.get(), cursor.slot_id.row));
        // These effects mutate vertex colors/positions per frame; cached rows
        // would bake stale values in, so reuse and capture shut off entirely.
        let global_effects_active = params.has_line_anims
            || !self.fx.text_fade.active.is_empty()
            || !self.fx.mode_line_fade.active.is_empty();
        let pointer_invalidated_rows = chunks
            .iter()
            .filter(|chunk| {
                params
                    .pointer_override
                    .affects_glyph_range(&frame_glyphs.glyphs, chunk.glyphs.clone())
            })
            .map(|chunk| chunk.key)
            .collect::<HashSet<_>>();
        let enable_subpixel = glyph_atlas.subpixel_enabled();
        let ctx = row_reuse::ReusePassCtx {
            damage: params.row_damage,
            scale_bits: self.scale_factor.to_bits(),
            scale_pow2: row_reuse::scale_is_power_of_two(self.scale_factor),
            scale_factor: self.scale_factor,
            atlas_generation: glyph_atlas.eviction_generation(),
            font_bindings_identity: glyph_atlas.frame_font_bindings_identity(),
            cursor_row,
            global_effects_active,
            invalidated_rows: Some(&pointer_invalidated_rows),
            window_origins: &window_origins,
            allow_store: !want_overlay && params.row_damage.is_some(),
        };

        let (out, captures, reuse_stats) = {
            let renderer: &WgpuRenderer = self;
            let cache = &renderer.row_reuse;
            let mut tessellator = LiveRowTessellator {
                renderer,
                atlas: glyph_atlas,
                params,
                want_overlay,
                enable_subpixel,
                seen_single_keys,
                seen_composed_keys,
                glyph_face_cache: None,
            };
            row_reuse::assemble_rows_with_reuse(&chunks, &ctx, cache, &mut tessellator)
        };

        self.row_reuse.stage(captures);
        stats.rows_tessellated += reuse_stats.rows_tessellated;
        stats.rows_reused_verbatim += reuse_stats.rows_reused_verbatim;
        stats.rows_reused_shifted += reuse_stats.rows_reused_shifted;
        stats.row_reuse_bails += reuse_stats.reuse_bails;

        TextGlyphBatches {
            mask_data: out.mask,
            subpixel_data: out.subpixel,
            color_data: out.color,
            rendered_char_bounds: out.bounds,
        }
    }
}

/// The production [`row_reuse::RowTessellator`]: the pre-existing per-glyph
/// atlas tessellation loop, scoped to one row chunk per call.
struct LiveRowTessellator<'r, 'p> {
    renderer: &'r WgpuRenderer,
    atlas: &'r mut WgpuGlyphAtlas,
    params: &'r FrameParams<'p>,
    want_overlay: bool,
    enable_subpixel: bool,
    seen_single_keys: &'r mut HashSet<GlyphKey>,
    seen_composed_keys: &'r mut HashSet<ComposedGlyphKey>,
    glyph_face_cache: Option<(FaceId, MaterializedFaceData)>,
}

impl row_reuse::RowTessellator for LiveRowTessellator<'_, '_> {
    fn revalidate_and_pin(&mut self, entry: AnyAtlasEntry) -> bool {
        self.atlas.revalidate_and_pin(entry)
    }

    fn tessellate(
        &mut self,
        chunk: &row_reuse::RowChunk,
        out: &mut row_reuse::RowStreams,
    ) -> row_reuse::RowTessellation {
        let frame_glyphs = self.params.frame_glyphs;
        let faces = self.params.faces;
        let face_debug_call_id = self.params.face_debug_call_id;
        let cursor_visible = self.params.cursor_visible;
        let has_line_anims = self.params.has_line_anims;
        let want_overlay = self.want_overlay;
        let enable_subpixel = self.enable_subpixel;
        let mut tessellation = row_reuse::RowTessellation::default();

        for glyph_index in chunk.glyphs.clone() {
            let glyph = &frame_glyphs.glyphs[glyph_index];
            if let FrameGlyph::Char {
                window_id,
                slot_id,
                char,
                composed,
                x,
                y,
                baseline,
                width,
                height,
                ascent,
                face_id,
                row_role,
                clip_rect,
                ..
            } = glyph
            {
                if row_role.is_chrome() != want_overlay {
                    continue;
                }

                // Resolve the face-derived attributes that used to be
                // inlined on the glyph. Glyphs arrive in face-runs, so a
                // one-entry cache avoids re-resolving per glyph.
                for paint in self.params.pointer_override.face_paints(
                    glyph_index,
                    *face_id,
                    Rect::new(*x, *y, *width, *height),
                    clip_rect.as_ref(),
                ) {
                    let face_id = paint.face_id();
                    let effective_clip = paint.clip();
                    let rf = match self.glyph_face_cache {
                        Some((id, ref data)) if id == face_id => *data,
                        _ => {
                            let data = frame_glyphs.resolved_face(face_id);
                            self.glyph_face_cache = Some((face_id, data));
                            data
                        }
                    };
                    let fg = &rf.fg;
                    let bg: Option<Color> = Some(rf.bg);
                    let font_size = rf.font_size;
                    let overstrike = rf.overstrike;

                    let face = faces.get(&face_id);
                    if face.is_some_and(|face| face.background_gradient.is_some()) {
                        // sample_face_background samples the gradient at this
                        // glyph's y; the color lands in vertex bytes, so the row
                        // may never be spliced at a different y.
                        tessellation.verbatim_only = true;
                    }

                    // Snap glyph origins to whole physical pixels via the SAME
                    // shared helper the child-frame path uses (see
                    // `content::snap_glyph_origin`), so both text surfaces keep one
                    // subpixel policy -- the atlas key must be position-invariant or
                    // scrolling re-rasterizes every glyph. Keeping this call (not a
                    // local `SubpixelBin::new`) is what makes content.rs's
                    // "full parity with the main frame" claim structurally true.
                    let sf = self.renderer.scale_factor;
                    let y_offset = if has_line_anims {
                        self.renderer.line_y_offset(*x, *y)
                    } else {
                        0.0
                    };
                    let phys_x = (*x) * sf;
                    let baseline_y = *baseline + y_offset;
                    let phys_y = baseline_y * sf;
                    let (x_int, y_int, x_bin, y_bin) =
                        super::content::snap_glyph_origin(phys_x, phys_y);
                    let font_identity = self.atlas.glyph_font_identity_for_char(face, *char);

                    let subpixel_request = if enable_subpixel {
                        SubpixelRequest::Enabled
                    } else {
                        SubpixelRequest::Disabled
                    };
                    let handles = if let Some(text) = composed {
                        self.seen_composed_keys.insert(ComposedGlyphKey {
                            text: text.clone(),
                            face_id,
                            font_size_bits: font_size.to_bits(),
                            font_identity,
                            glyph_stream_identity: self
                                .atlas
                                .glyph_stream_identity_for_composed(face, text),
                            x_bin,
                            y_bin,
                        });
                        self.atlas
                            .get_or_create_composed_atlas(
                                &self.renderer.device,
                                &self.renderer.queue,
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
                        let key = GlyphKey {
                            charcode: *char as u32,
                            face_id,
                            font_size_bits: font_size.to_bits(),
                            font_identity,
                            x_bin,
                            y_bin,
                        };
                        self.seen_single_keys.insert(key.clone());
                        if trace_face_debug_enabled() && !want_overlay && !color_is_grayscale(*fg) {
                            tracing::info!(
                                "face-debug call={} milestone=before_get_or_create char={:?} face={} pos=({:.1},{:.1}) fg=({:.3},{:.3},{:.3},{:.3})",
                                face_debug_call_id,
                                char,
                                face_id,
                                x,
                                y,
                                fg.r,
                                fg.g,
                                fg.b,
                                fg.a
                            );
                        }
                        self.atlas
                            .get_or_create_atlas(
                                &self.renderer.device,
                                &self.renderer.queue,
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
                        let glyph_x = (x_int as f32 + metrics.bearing_x) / sf;
                        let glyph_y = (y_int as f32 - metrics.bearing_y) / sf;
                        let glyph_w = content_rect.width() as f32 / sf;
                        let glyph_h = content_rect.height() as f32 / sf;

                        let tex_u_min = uv.min()[0];
                        let tex_u_max = uv.max()[0];
                        let tex_v_min_base = uv.min()[1];
                        let tex_v_max_base = uv.max()[1];

                        let (glyph_x, glyph_w, tex_u_min, tex_u_max) =
                            if let Some(clip) = &effective_clip {
                                let full_w = glyph_w;
                                let u_range = tex_u_max - tex_u_min;
                                let mut x0 = glyph_x;
                                let mut w0 = glyph_w;
                                let mut u0 = tex_u_min;
                                let mut u1 = tex_u_max;
                                let left = clip.x;
                                let right = clip.x + clip.width;
                                if x0 < left {
                                    let cut = left - x0;
                                    if cut >= w0 {
                                        continue;
                                    }
                                    x0 = left;
                                    w0 -= cut;
                                    u0 += (cut / full_w) * u_range;
                                }
                                if x0 + w0 > right {
                                    let cut = (x0 + w0) - right;
                                    if cut >= w0 {
                                        continue;
                                    }
                                    w0 -= cut;
                                    u1 -= (cut / full_w) * u_range;
                                }
                                (x0, w0, u0, u1)
                            } else {
                                (glyph_x, glyph_w, tex_u_min, tex_u_max)
                            };

                        let (glyph_y, glyph_h, tex_v_min, tex_v_max) =
                            if let Some(clip) = &effective_clip {
                                let full_h = glyph_h;
                                let v_range = tex_v_max_base - tex_v_min_base;
                                let mut y0 = glyph_y;
                                let mut h0 = glyph_h;
                                let mut v0 = tex_v_min_base;
                                let mut v1 = tex_v_max_base;
                                let top = clip.y;
                                let bottom = clip.y + clip.height;
                                if row_reuse::glyph_extent_touches_band(y0, h0, top, bottom) {
                                    // Trimmed (or trim-prone under any shift): the
                                    // baked v-range depends on absolute y vs the band.
                                    tessellation.verbatim_only = true;
                                }
                                if y0 < top {
                                    let cut = top - y0;
                                    if cut >= h0 {
                                        continue;
                                    }
                                    y0 = top;
                                    h0 -= cut;
                                    v0 += (cut / full_h) * v_range;
                                }
                                if y0 + h0 > bottom {
                                    let cut = (y0 + h0) - bottom;
                                    if cut >= h0 {
                                        continue;
                                    }
                                    h0 -= cut;
                                    v1 -= (cut / full_h) * v_range;
                                }
                                (y0, h0, v0, v1)
                            } else {
                                (glyph_y, glyph_h, tex_v_min_base, tex_v_max_base)
                            };

                        if glyph_w > CHAR_OVERLAP_MIN_AXIS && glyph_h > CHAR_OVERLAP_MIN_AXIS {
                            let cell_right = *x + *width;
                            let glyph_right = glyph_x + glyph_w;
                            out.bounds.push(RenderedCharBounds {
                                glyph_index,
                                window_id: window_id.get(),
                                row_role: *row_role,
                                slot_id: *slot_id,
                                label: composed
                                    .as_deref()
                                    .map(str::to_owned)
                                    .unwrap_or_else(|| char.to_string()),
                                face_id,
                                font_size,
                                cell_x: *x,
                                cell_y: *y,
                                cell_w: *width,
                                cell_h: *height,
                                glyph_x,
                                glyph_y,
                                glyph_w,
                                glyph_h,
                                left_overhang: (*x - glyph_x).max(0.0),
                                right_overhang: (glyph_right - cell_right).max(0.0),
                                top_overhang: (*y - glyph_y).max(0.0),
                                bottom_overhang: (glyph_y + glyph_h - (*y + *height)).max(0.0),
                            });
                        }

                        // Determine effective foreground color.
                        // For the character under a filled box cursor, swap to
                        // cursor_fg (inverse video) when cursor is visible.
                        let mut effective_fg = *fg;
                        let mut effective_bg =
                            WgpuRenderer::sample_face_paint_background(face, bg, paint)
                                .unwrap_or(Color::rgb(1.0, 1.0, 1.0));
                        if cursor_visible
                            && let Some(inverse) = self.params.cursor_inverse_video
                            && glyph.slot_id().is_some_and(|slot| slot == inverse.slot_id)
                        {
                            effective_fg = inverse.paint.glyph_foreground;
                            effective_bg = inverse.paint.body_background;
                        }

                        // Color glyphs use white vertex color (no tinting),
                        // mask glyphs use foreground color for tinting
                        let fade_alpha = self.renderer.text_fade_alpha(*x, *y)
                            * self.renderer.mode_line_fade_alpha(*x, *y);
                        let is_color = matches!(entry, AnyAtlasEntry::Color(_));
                        let color = if is_color {
                            [1.0, 1.0, 1.0, fade_alpha]
                        } else {
                            [
                                effective_fg.r,
                                effective_fg.g,
                                effective_fg.b,
                                effective_fg.a * fade_alpha,
                            ]
                        };
                        let subpixel_fg = subpixel_foreground_color(
                            effective_bg,
                            effective_fg,
                            effective_fg.a * fade_alpha,
                        );
                        let subpixel_bg = subpixel_background_color(effective_bg);

                        // Debug: log glyphs near y≈27 (where gray line appears in screenshot)
                        // and first few header glyphs (y < 5) to see row start
                        if !want_overlay && (glyph_y + glyph_h > 24.0 && glyph_y < 32.0) {
                            tracing::trace!(
                                "glyph_near_y27: char='{}' face={} pos=({:.1},{:.1}) size=({:.1},{:.1}) ascent={:.1} bottom={:.1} fg=({:.3},{:.3},{:.3},{:.3}) is_color={} cell=({:.1},{:.1},{:.1})",
                                if let Some(text) = composed {
                                    text.to_string()
                                } else {
                                    format!("{}", *char as u8 as char)
                                },
                                face_id,
                                glyph_x,
                                glyph_y,
                                glyph_w,
                                glyph_h,
                                *ascent,
                                glyph_y + glyph_h,
                                color[0],
                                color[1],
                                color[2],
                                color[3],
                                is_color,
                                *x,
                                *y,
                                *width,
                            );
                        }
                        if !want_overlay && *y < 1.0 {
                            tracing::trace!(
                                "first_row_glyph: char='{}' face={} cell=({:.1},{:.1},{:.1}) glyph_pos=({:.1},{:.1}) glyph_size=({:.1},{:.1}) ascent={:.1} fg=({:.3},{:.3},{:.3})",
                                if let Some(text) = composed {
                                    text.to_string()
                                } else {
                                    format!("{}", *char as u8 as char)
                                },
                                face_id,
                                *x,
                                *y,
                                *width,
                                glyph_x,
                                glyph_y,
                                glyph_w,
                                glyph_h,
                                *ascent,
                                color[0],
                                color[1],
                                color[2],
                            );
                        }

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

                        // Overstrike: simulate bold by drawing the
                        // glyph a second time shifted 1px right.
                        // This matches official Emacs behavior when
                        // a bold font variant is unavailable.
                        let overstrike_vertices = if overstrike {
                            let ox = 1.0 / self.renderer.scale_factor;
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

                        let subpixel_vertices = super::pointer_override::clip_subpixel_quad(
                            build_subpixel_vertices(
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
                            ),
                            effective_clip.as_ref(),
                        );

                        let overstrike_subpixel_vertices = if overstrike {
                            let ox = 1.0 / self.renderer.scale_factor;
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
                            out.color.push((entry, vertices));
                            if let Some(ov) = overstrike_vertices {
                                out.color.push((entry, ov));
                            }
                        } else if matches!(entry, AnyAtlasEntry::Subpixel(_)) {
                            if let Some(vertices) = subpixel_vertices {
                                out.subpixel.push((entry, vertices));
                            }
                            if let Some(ov) = overstrike_subpixel_vertices {
                                out.subpixel.push((entry, ov));
                            }
                        } else {
                            out.mask.push((entry, vertices));
                            if let Some(ov) = overstrike_vertices {
                                out.mask.push((entry, ov));
                            }
                        }
                    }
                }
            }
        }
        tessellation
    }
}

impl WgpuRenderer {
    /// Draw one pass's glyph batches: mask glyphs, subpixel glyphs, then
    /// color glyphs, batched by atlas page.
    fn draw_text_glyph_batches(
        &mut self,
        ctx: &mut FramePassCtx<'_, '_>,
        want_overlay: bool,
        glyph_atlas: &mut WgpuGlyphAtlas,
        batches: &TextGlyphBatches,
        stats: &mut GlyphRenderStats,
    ) {
        let render_pass = &mut ctx.pass;
        let logical_w = ctx.params.logical_w;
        let face_debug_call_id = ctx.params.face_debug_call_id;
        let TextGlyphBatches {
            mask_data,
            subpixel_data,
            color_data,
            ..
        } = batches;
        tracing::trace!(
            "render_frame_glyphs: role={:?} {} mask glyphs, {} color glyphs",
            want_overlay,
            mask_data.len(),
            color_data.len()
        );
        if trace_face_debug_enabled() {
            tracing::info!(
                "face-debug call={} milestone=after_glyph_loop overlay={} mask={} subpixel={} color={}",
                face_debug_call_id,
                want_overlay,
                mask_data.len(),
                subpixel_data.len(),
                color_data.len()
            );
        }
        // Debug: dump first few glyph positions
        if !mask_data.is_empty() && !want_overlay {
            for (i, (entry, verts)) in mask_data.iter().take(3).enumerate() {
                let p0 = verts[0].position;
                let c0 = verts[0].color;
                tracing::trace!(
                    "  glyph[{}]: page={:?} pos=({:.1},{:.1}) color=({:.3},{:.3},{:.3},{:.3}) logical_w={:.1}",
                    i,
                    entry.binding_id_value(),
                    p0[0],
                    p0[1],
                    c0[0],
                    c0[1],
                    c0[2],
                    c0[3],
                    logical_w
                );
            }
        }

        // Draw mask glyphs with glyph pipeline (alpha tinted with foreground)
        // Batch consecutive glyphs sharing the same atlas page.
        if !mask_data.is_empty() {
            render_pass.set_pipeline(&self.pipelines.glyph);
            render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);

            let all_vertices: Vec<GlyphVertex> = mask_data
                .iter()
                .flat_map(|(_, verts)| verts.iter().copied())
                .collect();

            if trace_face_debug_enabled() {
                for (idx, vertex) in all_vertices.iter().take(6).enumerate() {
                    let raw = bytemuck::bytes_of(vertex);
                    tracing::info!(
                        "face-debug call={} mask-vertex idx={} pos=({:.1},{:.1}) uv=({:.3},{:.3}) color=({:.3},{:.3},{:.3},{:.3}) raw={:02x?}",
                        face_debug_call_id,
                        idx,
                        vertex.position[0],
                        vertex.position[1],
                        vertex.tex_coords[0],
                        vertex.tex_coords[1],
                        vertex.color[0],
                        vertex.color[1],
                        vertex.color[2],
                        vertex.color[3],
                        raw,
                    );
                }
                if let Some((idx, vertex)) = all_vertices.iter().enumerate().find(|(_, v)| {
                    let [r, g, b, _] = v.color;
                    (r - g).abs() > 0.001 || (g - b).abs() > 0.001
                }) {
                    let raw = bytemuck::bytes_of(vertex);
                    tracing::info!(
                        "face-debug call={} mask-vertex-colored idx={} pos=({:.1},{:.1}) uv=({:.3},{:.3}) color=({:.3},{:.3},{:.3},{:.3}) raw={:02x?}",
                        face_debug_call_id,
                        idx,
                        vertex.position[0],
                        vertex.position[1],
                        vertex.tex_coords[0],
                        vertex.tex_coords[1],
                        vertex.color[0],
                        vertex.color[1],
                        vertex.color[2],
                        vertex.color[3],
                        raw,
                    );
                } else {
                    tracing::info!(
                        "face-debug call={} mask-vertex-colored none",
                        face_debug_call_id
                    );
                }
            }

            let mask_upload = self
                .arenas
                .glyph
                .upload(&self.device, &self.queue, &all_vertices);
            stats.glyph_vertex_buffer_creations += 1;

            if let Some(ref upload) = mask_upload {
                render_pass.set_vertex_buffer(0, self.arenas.glyph.slice(upload));
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
                        tracing::warn!(?err, "skipping stale mask glyph batch");
                        continue;
                    }
                };
                let vert_start = (batch_start * 6) as u32;
                let vert_end = (i * 6) as u32;
                render_pass.set_bind_group(1, bg, &[]);
                stats.glyph_bind_group_changes += 1;
                render_pass.draw(vert_start..vert_end, 0..1);
                stats.glyph_draw_calls += 1;
            }
        }

        if !subpixel_data.is_empty() {
            render_pass.set_pipeline(&self.pipelines.subpixel_glyph);
            render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);

            let all_vertices: Vec<SubpixelGlyphVertex> = subpixel_data
                .iter()
                .flat_map(|(_, verts)| verts.iter().copied())
                .collect();

            let subpixel_upload =
                self.arenas
                    .subpixel
                    .upload(&self.device, &self.queue, &all_vertices);
            stats.glyph_vertex_buffer_creations += 1;

            if let Some(ref upload) = subpixel_upload {
                render_pass.set_vertex_buffer(0, self.arenas.subpixel.slice(upload));
            }

            let mut i = 0;
            while i < subpixel_data.len() {
                let (entry, _) = &subpixel_data[i];
                let page_id = entry.binding_id_value();
                let batch_start = i;
                i += 1;
                while i < subpixel_data.len() && subpixel_data[i].0.binding_id_value() == page_id {
                    i += 1;
                }
                let bg = match glyph_atlas.atlas_bind_group(*entry) {
                    Ok(bg) => bg,
                    Err(err) => {
                        tracing::warn!(?err, "skipping stale subpixel glyph batch");
                        continue;
                    }
                };
                let vert_start = (batch_start * 6) as u32;
                let vert_end = (i * 6) as u32;
                render_pass.set_bind_group(1, bg, &[]);
                stats.glyph_bind_group_changes += 1;
                render_pass.draw(vert_start..vert_end, 0..1);
                stats.glyph_draw_calls += 1;
            }
        }

        // Draw color glyphs with image pipeline (direct RGBA, e.g. color emoji)
        if !color_data.is_empty() {
            render_pass.set_pipeline(&self.pipelines.image);
            render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);

            let all_vertices: Vec<GlyphVertex> = color_data
                .iter()
                .flat_map(|(_, verts)| verts.iter().copied())
                .collect();

            let color_upload = self
                .arenas
                .glyph
                .upload(&self.device, &self.queue, &all_vertices);
            stats.glyph_vertex_buffer_creations += 1;

            if let Some(ref upload) = color_upload {
                render_pass.set_vertex_buffer(0, self.arenas.glyph.slice(upload));
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
                        tracing::warn!(?err, "skipping stale color glyph batch");
                        continue;
                    }
                };
                let vert_start = (batch_start * 6) as u32;
                let vert_end = (i * 6) as u32;
                render_pass.set_bind_group(1, bg, &[]);
                stats.glyph_bind_group_changes += 1;
                render_pass.draw(vert_start..vert_end, 0..1);
                stats.glyph_draw_calls += 1;
            }
        }
    }

    /// Draw underline/overline/strike-through decorations for one text pass.
    fn draw_text_decorations(&mut self, ctx: &mut FramePassCtx<'_, '_>, want_overlay: bool) {
        let render_pass = &mut ctx.pass;
        let frame_glyphs = ctx.params.frame_glyphs;
        let has_line_anims = ctx.params.has_line_anims;
        // === Draw text decorations (underline, overline, strike-through) ===
        // Rendered after text so decorations appear on top of glyphs.
        // Box borders are handled separately via merged box_spans below.
        {
            let mut decoration_vertices: Vec<RectVertex> = Vec::new();

            let mut deco_face_cache: Option<(FaceId, MaterializedFaceData)> = None;
            for (glyph_index, glyph) in frame_glyphs.glyphs.iter().enumerate() {
                if let FrameGlyph::Char {
                    x,
                    y,
                    baseline,
                    width,
                    height,
                    ascent,
                    face_id,
                    row_role,
                    clip_rect,
                    ..
                } = glyph
                {
                    if row_role.is_chrome() != want_overlay {
                        continue;
                    }

                    for paint in ctx.params.pointer_override.face_paints(
                        glyph_index,
                        *face_id,
                        Rect::new(*x, *y, *width, *height),
                        clip_rect.as_ref(),
                    ) {
                        let face_id = paint.face_id();
                        let effective_clip = paint.clip();
                        let decoration_start = decoration_vertices.len();

                        let rf = match deco_face_cache {
                            Some((id, ref data)) if id == face_id => *data,
                            _ => {
                                let data = frame_glyphs.resolved_face(face_id);
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

                        let y_offset = if has_line_anims {
                            self.line_y_offset(*x, *y)
                        } else {
                            0.0
                        };
                        let ya = *y + y_offset;
                        let baseline_y = *baseline + y_offset;

                        // Get per-face font metrics for proper decoration positioning
                        let (ul_position, ul_thick) = frame_glyphs
                            .faces
                            .get(&face_id)
                            .map(|f| (f.underline_placement, f.underline_thickness as f32))
                            .unwrap_or_default();

                        // --- Underline ---
                        if *underline != UnderlineStyle::None {
                            let ul_color = underline_color.as_ref().unwrap_or(fg);
                            let geometry = ul_position.resolve(ya, *height, baseline_y, ul_thick);
                            let ul_y = geometry.top_y;
                            let line_thickness = geometry.thickness;

                            match *underline {
                                UnderlineStyle::Line => {
                                    // Single solid line
                                    self.add_rect(
                                        &mut decoration_vertices,
                                        *x,
                                        ul_y,
                                        *width,
                                        line_thickness,
                                        ul_color,
                                    );
                                }
                                UnderlineStyle::Wave => {
                                    // Wave: smooth sine wave underline
                                    let amplitude: f32 = 2.0;
                                    let wavelength: f32 = 8.0;
                                    let seg_w: f32 = 1.0;
                                    let mut cx = *x;
                                    while cx < *x + *width {
                                        let sw = seg_w.min(*x + *width - cx);
                                        let phase = (cx - *x) * std::f32::consts::TAU / wavelength;
                                        let offset = phase.sin() * amplitude;
                                        self.add_rect(
                                            &mut decoration_vertices,
                                            cx,
                                            ul_y + offset,
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
                                        *x,
                                        ul_y,
                                        *width,
                                        line_thickness,
                                        ul_color,
                                    );
                                    self.add_rect(
                                        &mut decoration_vertices,
                                        *x,
                                        ul_y + line_thickness + 1.0,
                                        *width,
                                        line_thickness,
                                        ul_color,
                                    );
                                }
                                UnderlineStyle::Dotted => {
                                    // Dots (dot size = thickness, gap = 2px)
                                    let mut cx = *x;
                                    while cx < *x + *width {
                                        let dw = line_thickness.min(*x + *width - cx);
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
                                    // Dashes (4px with 3px gap)
                                    let mut cx = *x;
                                    while cx < *x + *width {
                                        let dw = 4.0_f32.min(*x + *width - cx);
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
                                // None reaches here only for an out-of-range
                                // code (the `*underline > 0` guard excludes a
                                // real None): fall back to a single line.
                                UnderlineStyle::None => {
                                    self.add_rect(
                                        &mut decoration_vertices,
                                        *x,
                                        ul_y,
                                        *width,
                                        line_thickness,
                                        ul_color,
                                    );
                                }
                            }
                        }

                        // --- Overline ---
                        if *overline {
                            let ol_color = overline_color.as_ref().unwrap_or(fg);
                            self.add_rect(
                                &mut decoration_vertices,
                                *x,
                                ya,
                                *width,
                                ul_thick.max(1.0),
                                ol_color,
                            );
                        }

                        // --- Strike-through ---
                        if *strike_through {
                            let st_color = strike_through_color.as_ref().unwrap_or(fg);
                            // Position at ~1/3 of ascent above baseline (standard typographic position)
                            let st_y = baseline_y - *ascent / 3.0;
                            self.add_rect(
                                &mut decoration_vertices,
                                *x,
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

            // Also draw decorations for Stretch glyphs (e.g. align-to
            // gaps in mode-line).  Look up the face by face_id to get
            // underline/overline/strike-through attributes.
            for (glyph_index, glyph) in frame_glyphs.glyphs.iter().enumerate() {
                if let FrameGlyph::Stretch {
                    x,
                    y,
                    width,
                    height,
                    face_id,
                    row_role,
                    clip_rect,
                    ..
                } = glyph
                {
                    if row_role.is_chrome() != want_overlay {
                        continue;
                    }
                    for paint in ctx.params.pointer_override.face_paints(
                        glyph_index,
                        *face_id,
                        Rect::new(*x, *y, *width, *height),
                        clip_rect.as_ref(),
                    ) {
                        let face_id = paint.face_id();
                        let effective_clip = paint.clip();
                        let decoration_start = decoration_vertices.len();
                        let face = match frame_glyphs.faces.get(&face_id) {
                            Some(f) => f,
                            None => continue,
                        };
                        let has_underline = face.attributes.contains(FaceAttributes::UNDERLINE);
                        let has_overline = face.attributes.contains(FaceAttributes::OVERLINE);
                        let has_strike = face.attributes.contains(FaceAttributes::STRIKE_THROUGH);
                        if !has_underline && !has_overline && !has_strike {
                            continue;
                        }

                        let y_offset = if has_line_anims {
                            self.line_y_offset(*x, *y)
                        } else {
                            0.0
                        };
                        let ya = *y + y_offset;
                        let font_ascent = face.font_ascent as f32;
                        let baseline_y = ya + font_ascent;
                        let ul_thick = face.underline_thickness as f32;
                        let fg = &face.foreground;

                        // --- Underline ---
                        if has_underline {
                            let ul_color = face.underline_color.as_ref().unwrap_or(fg);
                            let geometry = face
                                .underline_placement
                                .resolve(ya, *height, baseline_y, ul_thick);
                            let ul_y = geometry.top_y;
                            let line_thickness = geometry.thickness;

                            match face.underline_style {
                                UnderlineStyle::Line => {
                                    self.add_rect(
                                        &mut decoration_vertices,
                                        *x,
                                        ul_y,
                                        *width,
                                        line_thickness,
                                        ul_color,
                                    );
                                }
                                UnderlineStyle::Wave => {
                                    let amplitude: f32 = 2.0;
                                    let wavelength: f32 = 8.0;
                                    let seg_w: f32 = 1.0;
                                    let mut cx = *x;
                                    while cx < *x + *width {
                                        let sw = seg_w.min(*x + *width - cx);
                                        let phase = (cx - *x) * std::f32::consts::TAU / wavelength;
                                        let offset = phase.sin() * amplitude;
                                        self.add_rect(
                                            &mut decoration_vertices,
                                            cx,
                                            ul_y + offset,
                                            sw,
                                            line_thickness,
                                            ul_color,
                                        );
                                        cx += seg_w;
                                    }
                                }
                                UnderlineStyle::Double => {
                                    self.add_rect(
                                        &mut decoration_vertices,
                                        *x,
                                        ul_y,
                                        *width,
                                        line_thickness,
                                        ul_color,
                                    );
                                    self.add_rect(
                                        &mut decoration_vertices,
                                        *x,
                                        ul_y + line_thickness + 1.0,
                                        *width,
                                        line_thickness,
                                        ul_color,
                                    );
                                }
                                UnderlineStyle::Dotted => {
                                    let mut cx = *x;
                                    while cx < *x + *width {
                                        let dw = line_thickness.min(*x + *width - cx);
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
                                    let mut cx = *x;
                                    while cx < *x + *width {
                                        let dw = 4.0_f32.min(*x + *width - cx);
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
                                UnderlineStyle::None => {}
                            }
                        }

                        // --- Overline ---
                        if has_overline {
                            let ol_color = face.overline_color.as_ref().unwrap_or(fg);
                            self.add_rect(
                                &mut decoration_vertices,
                                *x,
                                ya,
                                *width,
                                ul_thick.max(1.0),
                                ol_color,
                            );
                        }

                        // --- Strike-through ---
                        if has_strike {
                            let st_color = face.strike_through_color.as_ref().unwrap_or(fg);
                            let st_y = baseline_y - font_ascent / 3.0;
                            self.add_rect(
                                &mut decoration_vertices,
                                *x,
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

            if let Some(upload) =
                self.arenas
                    .rect
                    .upload(&self.device, &self.queue, &decoration_vertices)
            {
                render_pass.set_pipeline(&self.pipelines.rect);
                render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                render_pass.set_vertex_buffer(0, upload.buffer_slice());
                render_pass.draw(0..decoration_vertices.len() as u32, 0..1);
            }
        }
    }

    /// Draw merged box-span borders (sharp rects and rounded SDF rings) for
    /// one text pass.
    fn draw_box_borders(
        &mut self,
        ctx: &mut FramePassCtx<'_, '_>,
        want_overlay: bool,
        spans: &BoxSpanSet,
    ) {
        let render_pass = &mut ctx.pass;
        let faces = ctx.params.faces;
        let box_spans = &spans.spans;
        // === Draw box borders (merged spans) ===
        // Standard boxes (corner_radius=0): merged rect borders (top/bottom/left/right).
        // Rounded boxes (corner_radius>0): SDF border ring.
        {
            // Sharp box borders as merged rect spans
            let mut sharp_border_vertices: Vec<RectVertex> = Vec::new();
            // Rounded box borders via SDF
            let mut rounded_border_vertices: Vec<RoundedRectVertex> = Vec::new();

            // Filter spans for this overlay pass
            let pass_spans: Vec<usize> = box_spans
                .iter()
                .enumerate()
                .filter(|(_, s)| s.row_role.is_chrome() == want_overlay)
                .map(|(i, _)| i)
                .collect();

            for &span_idx in &pass_spans {
                let span = &box_spans[span_idx];
                if let Some(face) = faces.get(&span.face_id)
                    && self.append_box_border_geometry(
                        &mut sharp_border_vertices,
                        &mut rounded_border_vertices,
                        span,
                        face,
                        ctx.params.device_scale,
                        0.0,
                        0.0,
                    )
                {
                    self.fx.has_animated_borders = true;
                }
            }

            // Draw sharp box borders
            if let Some(upload) =
                self.arenas
                    .rect
                    .upload(&self.device, &self.queue, &sharp_border_vertices)
            {
                render_pass.set_pipeline(&self.pipelines.rect);
                render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                render_pass.set_vertex_buffer(0, upload.buffer_slice());
                render_pass.draw(0..sharp_border_vertices.len() as u32, 0..1);
            }

            // Draw rounded box borders
            if let Some(upload) =
                self.arenas
                    .rounded
                    .upload(&self.device, &self.queue, &rounded_border_vertices)
            {
                render_pass.set_pipeline(&self.pipelines.rounded_rect);
                render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                render_pass.set_vertex_buffer(0, upload.buffer_slice());
                render_pass.draw(0..rounded_border_vertices.len() as u32, 0..1);
            }
        }
    }
}
