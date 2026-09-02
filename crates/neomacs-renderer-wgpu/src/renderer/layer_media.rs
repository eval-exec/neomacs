//! Inline media draw phases of `render_frame_glyphs` (z-order step 7):
//! images, videos, and WebKit xwidget views.

use neomacs_display_protocol::frame_glyphs::FrameGlyph;
use neomacs_display_protocol::types::Rect;
#[cfg(feature = "video")]
use neomacs_display_protocol::types::VideoId;
#[cfg(all(feature = "webview", target_os = "linux"))]
use neomacs_display_protocol::types::WebViewId;

use super::super::vertex::{GlyphVertex, RectVertex};
use super::WgpuRenderer;
use super::frame_pass::FramePassCtx;

/// A textured quad gathered for a batched arena draw: the cache id to bind
/// plus its six vertices. All quads of a phase upload as one arena region;
/// each draws its own range so the draw sequence matches the gather order.
pub(super) struct MediaQuad<Id> {
    pub(super) id: Id,
    pub(super) vertices: [GlyphVertex; 6],
}

#[cfg(all(feature = "webview", target_os = "linux"))]
pub(super) const fn inline_webview_id(glyph: &FrameGlyph) -> Option<WebViewId> {
    match glyph {
        FrameGlyph::Xwidget { webview_id, .. } => Some(*webview_id),
        _ => None,
    }
}

#[cfg(feature = "video")]
pub(super) struct PreparedInlineVideos<'a> {
    pub(super) draws: crate::video_cache::PreparedVideoDraws<'a>,
    pub(super) quads: Vec<MediaQuad<VideoId>>,
}

#[cfg(feature = "video")]
impl PreparedInlineVideos<'_> {
    pub(super) fn upload(
        &self,
        arena: &mut super::dynamic_buffer::FrameVertexArena<GlyphVertex>,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Option<super::dynamic_buffer::VertexUpload> {
        let vertices: Vec<_> = self
            .quads
            .iter()
            .flat_map(|quad| quad.vertices.iter().copied())
            .collect();
        arena.upload(device, queue, &vertices)
    }

    pub(super) fn draw(
        &self,
        render_pass: &mut wgpu::RenderPass<'_>,
        upload: &super::dynamic_buffer::VertexUpload,
        packed_pipeline: &wgpu::RenderPipeline,
        bi_planar_pipeline: &wgpu::RenderPipeline,
    ) {
        render_pass.set_vertex_buffer(0, upload.buffer_slice());
        for (index, quad) in self.quads.iter().enumerate() {
            if let Some(frame) = self.draws.get(quad.id) {
                render_pass.set_pipeline(match frame.sample_kind() {
                    neomacs_video::VideoSampleKind::Packed => packed_pipeline,
                    neomacs_video::VideoSampleKind::BiPlanar => bi_planar_pipeline,
                });
                render_pass.set_bind_group(1, frame.bind_group(), &[]);
                render_pass.draw((index * 6) as u32..(index * 6 + 6) as u32, 0..1);
            }
        }
        // Later media phases intentionally inherit the canonical image path.
        render_pass.set_pipeline(packed_pipeline);
    }
}

pub(super) struct ClippedMediaRect {
    pub(super) draw_x: f32,
    pub(super) draw_y: f32,
    pub(super) draw_width: f32,
    pub(super) draw_height: f32,
    pub(super) u_min: f32,
    pub(super) u_max: f32,
    pub(super) v_min: f32,
    pub(super) v_max: f32,
}

pub(super) fn clipped_media_rect(
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    clip: Option<&Rect>,
) -> Option<ClippedMediaRect> {
    let Some(clip) = clip else {
        return Some(ClippedMediaRect {
            draw_x: x,
            draw_y: y,
            draw_width: width,
            draw_height: height,
            u_min: 0.0,
            u_max: 1.0,
            v_min: 0.0,
            v_max: 1.0,
        });
    };
    let left = x.max(clip.x);
    let top = y.max(clip.y);
    let right = (x + width).min(clip.x + clip.width);
    let bottom = (y + height).min(clip.y + clip.height);
    let draw_width = right - left;
    let draw_height = bottom - top;
    if draw_width <= 0.0 || draw_height <= 0.0 || width <= 0.0 || height <= 0.0 {
        return None;
    }
    Some(ClippedMediaRect {
        draw_x: left,
        draw_y: top,
        draw_width,
        draw_height,
        u_min: (left - x) / width,
        u_max: (right - x) / width,
        v_min: (top - y) / height,
        v_max: (bottom - y) / height,
    })
}

/// Untinted (white) textured quad spanning the full u range and the given
/// (possibly clip-trimmed) v range.
// Default features use this only from test and feature-gated video/WebKit paths.
#[allow(dead_code)]
pub(super) fn textured_quad_vertices(
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    tex_v_min: f32,
    tex_v_max: f32,
) -> [GlyphVertex; 6] {
    textured_quad_vertices_uv(x, y, width, height, 0.0, 1.0, tex_v_min, tex_v_max)
}

pub(super) fn textured_quad_vertices_uv(
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    tex_u_min: f32,
    tex_u_max: f32,
    tex_v_min: f32,
    tex_v_max: f32,
) -> [GlyphVertex; 6] {
    let white = [1.0, 1.0, 1.0, 1.0];
    [
        GlyphVertex {
            position: [x, y],
            tex_coords: [tex_u_min, tex_v_min],
            color: white,
        },
        GlyphVertex {
            position: [x + width, y],
            tex_coords: [tex_u_max, tex_v_min],
            color: white,
        },
        GlyphVertex {
            position: [x + width, y + height],
            tex_coords: [tex_u_max, tex_v_max],
            color: white,
        },
        GlyphVertex {
            position: [x, y],
            tex_coords: [tex_u_min, tex_v_min],
            color: white,
        },
        GlyphVertex {
            position: [x + width, y + height],
            tex_coords: [tex_u_max, tex_v_max],
            color: white,
        },
        GlyphVertex {
            position: [x, y + height],
            tex_coords: [tex_u_min, tex_v_max],
            color: white,
        },
    ]
}

#[cfg(feature = "video")]
pub(super) fn video_quad_vertices(
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    texture: neomacs_video::VideoTextureCoordinates,
    opacity: f32,
) -> [GlyphVertex; 6] {
    let white = [1.0, 1.0, 1.0, opacity.clamp(0.0, 1.0)];
    let texture = texture.triangle_list();
    let positions = [
        [x, y],
        [x + width, y],
        [x + width, y + height],
        [x, y],
        [x + width, y + height],
        [x, y + height],
    ];
    std::array::from_fn(|index| GlyphVertex {
        position: positions[index],
        tex_coords: texture[index],
        color: white,
    })
}

/// Resolve scene geometry and native sampling metadata together from one
/// immutable preparation. Root and child-frame paths share this operation so
/// neither re-queries video state between vertex generation and binding.
#[cfg(feature = "video")]
pub(super) fn collect_prepared_video_quads(
    glyphs: &[FrameGlyph],
    offset_x: f32,
    offset_y: f32,
    draws: &crate::video_cache::PreparedVideoDraws<'_>,
) -> Vec<MediaQuad<VideoId>> {
    glyphs
        .iter()
        .filter_map(|glyph| {
            let FrameGlyph::Video {
                video_id,
                x,
                y,
                width,
                height,
                clip_rect,
                opacity,
                ..
            } = glyph
            else {
                return None;
            };
            let frame = draws.get(*video_id)?;
            let x = *x + offset_x;
            let y = *y + offset_y;
            let translated_clip = clip_rect.map(|clip| Rect {
                x: clip.x + offset_x,
                y: clip.y + offset_y,
                ..clip
            });
            let clipped = clipped_media_rect(x, y, *width, *height, translated_clip.as_ref())?;
            Some(MediaQuad {
                id: *video_id,
                vertices: video_quad_vertices(
                    clipped.draw_x,
                    clipped.draw_y,
                    clipped.draw_width,
                    clipped.draw_height,
                    frame.sampling_transform().coordinates_for_destination_rect(
                        clipped.u_min,
                        clipped.u_max,
                        clipped.v_min,
                        clipped.v_max,
                    ),
                    *opacity,
                ),
            })
        })
        .collect()
}

/// One canonical preparation for root and child inline-video passes. It owns
/// ID discovery, generation-checked frame preparation, clipping, and native
/// sampling transforms so the two render targets cannot drift.
#[cfg(feature = "video")]
pub(super) fn prepare_inline_videos<'a>(
    cache: &'a crate::video_cache::VideoCache,
    glyphs: &[FrameGlyph],
    offset_x: f32,
    offset_y: f32,
) -> Option<PreparedInlineVideos<'a>> {
    let ids = glyphs.iter().filter_map(|glyph| match glyph {
        FrameGlyph::Video { video_id, .. } => Some(*video_id),
        _ => None,
    });
    let draws = cache.prepare_draws(ids)?;
    let quads = collect_prepared_video_quads(glyphs, offset_x, offset_y, &draws);
    (!quads.is_empty()).then_some(PreparedInlineVideos { draws, quads })
}

impl WgpuRenderer {
    /// Draw inline images on top of text.
    pub(super) fn draw_inline_images(&mut self, ctx: &mut FramePassCtx<'_, '_>) {
        let frame_glyphs = ctx.params.frame_glyphs;
        // Gather quads for images with a ready texture (same skip logic the
        // per-quad draw used), then upload once and draw per-image ranges.
        let mut quads = Vec::new();
        let mut relief_vertices: Vec<RectVertex> = Vec::new();

        for (glyph_index, glyph) in frame_glyphs.glyphs.iter().enumerate() {
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
            {
                let effective_clip = *clip_rect;
                let (
                    draw_x,
                    draw_y,
                    clipped_width,
                    clipped_height,
                    tex_u_min,
                    tex_u_max,
                    tex_v_min,
                    tex_v_max,
                ) = if let Some(clip) = &effective_clip {
                    let mut x0 = *x;
                    let mut y0 = *y;
                    let mut w0 = *width;
                    let mut h0 = *height;
                    let mut u0 = 0.0_f32;
                    let mut u1 = 1.0_f32;
                    let mut v0 = 0.0_f32;
                    let mut v1 = 1.0_f32;
                    let left = clip.x;
                    let right = clip.x + clip.width;
                    let top = clip.y;
                    let bottom = clip.y + clip.height;
                    if x0 < left {
                        let cut = left - x0;
                        if cut >= w0 {
                            continue;
                        }
                        x0 = left;
                        w0 -= cut;
                        if *width > 0.0 {
                            u0 += cut / *width;
                        }
                    }
                    if x0 + w0 > right {
                        let cut = (x0 + w0) - right;
                        if cut >= w0 {
                            continue;
                        }
                        w0 -= cut;
                        if *width > 0.0 {
                            u1 -= cut / *width;
                        }
                    }
                    if y0 < top {
                        let cut = top - y0;
                        if cut >= h0 {
                            continue;
                        }
                        y0 = top;
                        h0 -= cut;
                        if *height > 0.0 {
                            v0 += cut / *height;
                        }
                    }
                    if y0 + h0 > bottom {
                        let cut = (y0 + h0) - bottom;
                        if cut >= h0 {
                            continue;
                        }
                        h0 -= cut;
                        if *height > 0.0 {
                            v1 -= cut / *height;
                        }
                    }
                    (x0, y0, w0, h0, u0, u1, v0, v1)
                } else {
                    (*x, *y, *width, *height, 0.0, 1.0, 0.0, 1.0)
                };

                // Skip if fully clipped
                if clipped_width <= 0.0 || clipped_height <= 0.0 {
                    continue;
                }
                let (tex_u_min, tex_v_min) = source_rect.map_uv(tex_u_min, tex_v_min);
                let (tex_u_max, tex_v_max) = source_rect.map_uv(tex_u_max, tex_v_max);

                tracing::debug!(
                    "Rendering image {} at ({}, {}) size {}x{} (clipped to {})",
                    image_id,
                    x,
                    y,
                    width,
                    height,
                    clipped_height
                );
                // Check if image texture is ready
                if self.caches.image.get(*image_id).is_some() {
                    self.media_budget
                        .touch(crate::media_budget::MediaType::Image, image_id.get());
                    // Create vertices for image quad (white color = no tinting)
                    quads.push(MediaQuad {
                        id: *image_id,
                        vertices: textured_quad_vertices_uv(
                            draw_x,
                            draw_y,
                            clipped_width,
                            clipped_height,
                            tex_u_min,
                            tex_u_max,
                            tex_v_min,
                            tex_v_max,
                        ),
                    });
                    if let Some(override_paint) =
                        ctx.params.pointer_override.image_override(glyph_index)
                        && let neomacs_display_protocol::PointerDrawMode::ImageRelief(relief) =
                            override_paint.mode()
                    {
                        let relief_clip = ctx
                            .params
                            .pointer_override
                            .image_clip(glyph_index, clip_rect.as_ref());
                        super::pointer_override::append_clipped_relief(
                            &mut relief_vertices,
                            *x,
                            *y,
                            *width,
                            *height,
                            relief,
                            relief_clip.as_ref(),
                        );
                    }
                }
            }
        }

        let all_vertices: Vec<GlyphVertex> = quads
            .iter()
            .flat_map(|quad| quad.vertices.iter().copied())
            .collect();
        let upload = self
            .arenas
            .image
            .upload(&self.device, &self.queue, &all_vertices);

        // Pipeline + uniforms are set even with zero images: the inline-video
        // phase that follows inherits this pipeline state.
        let render_pass = &mut ctx.pass;
        render_pass.set_pipeline(&self.pipelines.image);
        render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
        let Some(upload) = upload else {
            return;
        };
        render_pass.set_vertex_buffer(0, upload.buffer_slice());
        for (i, quad) in quads.iter().enumerate() {
            if let Some(cached) = self.caches.image.get(quad.id) {
                render_pass.set_bind_group(1, &cached.bind_group, &[]);
                render_pass.draw((i * 6) as u32..(i * 6 + 6) as u32, 0..1);
            }
        }
        self.draw_rect_vertex_layer(&mut ctx.pass, &relief_vertices);
        // Feature-gated video rendering intentionally inherits the image
        // pipeline from this phase, so restore it after relief edges.
        ctx.pass.set_pipeline(&self.pipelines.image);
        ctx.pass.set_bind_group(0, &self.uniform_bind_group, &[]);
    }

    /// Draw inline videos (inherits the image pipeline set by the inline
    /// image phase).
    #[cfg(feature = "video")]
    pub(super) fn draw_inline_videos(&mut self, ctx: &mut FramePassCtx<'_, '_>) -> Vec<VideoId> {
        let frame_glyphs = ctx.params.frame_glyphs;
        let Some(prepared) =
            prepare_inline_videos(&self.caches.video, &frame_glyphs.glyphs, 0.0, 0.0)
        else {
            return Vec::new();
        };
        self.media_budget.touch(
            crate::media_budget::MediaType::Video,
            crate::video_cache::VIDEO_GPU_POOL_ACCOUNTING_ID,
        );
        let Some(upload) = prepared.upload(&mut self.arenas.image, &self.device, &self.queue)
        else {
            return Vec::new();
        };
        let submitted = prepared.quads.iter().map(|quad| quad.id).collect();
        prepared.draw(
            &mut ctx.pass,
            &upload,
            &self.pipelines.image,
            &self.pipelines.bi_planar_video,
        );
        submitted
    }

    /// Draw inline WebKit views (opaque pipeline: DMA-BUF XRGB has alpha=0).
    #[cfg(all(feature = "webview", target_os = "linux"))]
    pub(super) fn draw_inline_webkit_views(&mut self, ctx: &mut FramePassCtx<'_, '_>) {
        let frame_glyphs = ctx.params.frame_glyphs;
        // Draw inline webkit views (use opaque pipeline — DMA-BUF XRGB has alpha=0)
        {
            let mut quads = Vec::new();
            for glyph in &frame_glyphs.glyphs {
                if let FrameGlyph::Xwidget {
                    webview_id,
                    x,
                    y,
                    content,
                    clip_rect,
                    ..
                } = glyph
                {
                    // The texture is the widget at its own size (GNU
                    // `xww->width` x `xww->height`); the glyph slot may have
                    // been cropped at the right edge, so draw the content
                    // extent and cut it to the text-area clip on all four
                    // sides through the helper the image path uses.
                    let width = content.width_px();
                    let height = content.height_px();
                    let Some(clipped) =
                        clipped_media_rect(*x, *y, width, height, clip_rect.as_ref())
                    else {
                        continue;
                    };

                    let view_id = inline_webview_id(glyph)
                        .expect("the glyph was exhaustively matched as an xwidget");
                    // Check if webkit texture is ready
                    if self.caches.webview.get(view_id).is_some() {
                        self.media_budget
                            .touch(crate::media_budget::MediaType::WebKit, view_id.get());
                        tracing::debug!(
                            "Rendering webkit {} at ({}, {}) size {}x{} (clipped to {}x{})",
                            webview_id,
                            x,
                            y,
                            width,
                            height,
                            clipped.draw_width,
                            clipped.draw_height
                        );
                        // Create vertices for webkit quad (white color = no tinting)
                        quads.push(MediaQuad {
                            id: view_id,
                            vertices: textured_quad_vertices_uv(
                                clipped.draw_x,
                                clipped.draw_y,
                                clipped.draw_width,
                                clipped.draw_height,
                                clipped.u_min,
                                clipped.u_max,
                                clipped.v_min,
                                clipped.v_max,
                            ),
                        });
                    } else {
                        tracing::debug!("WebView {} not found in cache", webview_id);
                    }
                }
            }

            let all_vertices: Vec<GlyphVertex> = quads
                .iter()
                .flat_map(|quad| quad.vertices.iter().copied())
                .collect();
            let upload = self
                .arenas
                .image
                .upload(&self.device, &self.queue, &all_vertices);

            let render_pass = &mut ctx.pass;
            render_pass.set_pipeline(&self.pipelines.opaque_image);
            render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
            let Some(upload) = upload else {
                return;
            };
            render_pass.set_vertex_buffer(0, upload.buffer_slice());
            for (i, quad) in quads.iter().enumerate() {
                if let Some(cached) = self.caches.webview.get(quad.id) {
                    render_pass.set_bind_group(1, &cached.bind_group, &[]);
                    render_pass.draw((i * 6) as u32..(i * 6 + 6) as u32, 0..1);
                }
            }
        }
    }

    /// Draw inline shader surfaces (image pipeline, alpha blended). Also
    /// stamps each composited surface as recently drawn so its animation
    /// demand and iTime clock stay live only while visible, and routes the
    /// pointer's hover position into the `iMouse` uniform of the surface
    /// under it.
    pub(super) fn draw_inline_surfaces(&mut self, ctx: &mut FramePassCtx<'_, '_>) {
        let frame_glyphs = ctx.params.frame_glyphs;
        let mut quads = Vec::new();
        for glyph in &frame_glyphs.glyphs {
            if let FrameGlyph::Surface {
                surface_id,
                x,
                y,
                width,
                height,
                clip_rect,
                ..
            } = glyph
            {
                let (draw_x, draw_y, clipped_width, clipped_height, u0, u1, v0, v1) =
                    if let Some(clip) = clip_rect {
                        let mut x0 = *x;
                        let mut y0 = *y;
                        let mut w0 = *width;
                        let mut h0 = *height;
                        let mut u0 = 0.0_f32;
                        let mut u1 = 1.0_f32;
                        let mut v0 = 0.0_f32;
                        let mut v1 = 1.0_f32;
                        let left = clip.x;
                        let right = clip.x + clip.width;
                        let top = clip.y;
                        let bottom = clip.y + clip.height;
                        if x0 < left {
                            let cut = left - x0;
                            if cut >= w0 {
                                continue;
                            }
                            x0 = left;
                            w0 -= cut;
                            if *width > 0.0 {
                                u0 += cut / *width;
                            }
                        }
                        if x0 + w0 > right {
                            let cut = (x0 + w0) - right;
                            if cut >= w0 {
                                continue;
                            }
                            w0 -= cut;
                            if *width > 0.0 {
                                u1 -= cut / *width;
                            }
                        }
                        if y0 < top {
                            let cut = top - y0;
                            if cut >= h0 {
                                continue;
                            }
                            y0 = top;
                            h0 -= cut;
                            if *height > 0.0 {
                                v0 += cut / *height;
                            }
                        }
                        if y0 + h0 > bottom {
                            let cut = (y0 + h0) - bottom;
                            if cut >= h0 {
                                continue;
                            }
                            h0 -= cut;
                            if *height > 0.0 {
                                v1 -= cut / *height;
                            }
                        }
                        (x0, y0, w0, h0, u0, u1, v0, v1)
                    } else {
                        (*x, *y, *width, *height, 0.0, 1.0, 0.0, 1.0)
                    };

                if clipped_width <= 0.0 || clipped_height <= 0.0 {
                    continue;
                }

                if self.caches.surface.get(surface_id.get()).is_some() {
                    self.caches.surface.mark_drawn(surface_id.get());
                    self.media_budget
                        .touch(crate::media_budget::MediaType::Surface, surface_id.get());
                    // Hover-only iMouse: while the pointer is inside the
                    // glyph rect (logical px), stream its normalized position
                    // into the surface's uniforms (picked up by the next
                    // offscreen pass). Outside the rect nothing is written,
                    // so iMouse persists at the last hover position.
                    let (mx, my) = ctx.params.mouse_pos;
                    if mx >= *x && mx < *x + *width && my >= *y && my < *y + *height {
                        self.caches.surface.set_mouse_uv(
                            surface_id.get(),
                            (mx - *x) / *width,
                            (my - *y) / *height,
                        );
                    }
                    quads.push(MediaQuad {
                        id: surface_id.get(),
                        vertices: textured_quad_vertices_uv(
                            draw_x,
                            draw_y,
                            clipped_width,
                            clipped_height,
                            u0,
                            u1,
                            v0,
                            v1,
                        ),
                    });
                } else {
                    tracing::warn!("shader surface {} not found in cache", surface_id);
                }
            }
        }

        let all_vertices: Vec<GlyphVertex> = quads
            .iter()
            .flat_map(|quad| quad.vertices.iter().copied())
            .collect();
        let Some(upload) = self
            .arenas
            .image
            .upload(&self.device, &self.queue, &all_vertices)
        else {
            return;
        };

        let render_pass = &mut ctx.pass;
        render_pass.set_pipeline(&self.pipelines.image);
        render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
        render_pass.set_vertex_buffer(0, upload.buffer_slice());
        for (i, quad) in quads.iter().enumerate() {
            if let Some(cached) = self.caches.surface.get(quad.id) {
                render_pass.set_bind_group(1, &cached.composite_bind_group, &[]);
                render_pass.draw((i * 6) as u32..(i * 6 + 6) as u32, 0..1);
            }
        }
    }
}

#[cfg(all(
    test,
    any(feature = "video", all(feature = "webview", target_os = "linux"))
))]
#[path = "layer_media_test.rs"]
mod tests;
