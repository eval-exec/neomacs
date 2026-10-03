//! Child frame rendering methods for WgpuRenderer.

use super::super::glyph_atlas::WgpuGlyphAtlas;
use super::super::vertex::{GlyphVertex, RectVertex, RoundedRectVertex};
use super::WgpuRenderer;
use neomacs_display_protocol::frame_glyphs::FrameGlyphBuffer;
use neomacs_display_protocol::types::{AnimatedCursor, Color};
use neomacs_display_protocol::{PointerAppearanceSelection, RootSurfaceRect};

/// Unscaled old picture and preflighted scratch for a child resize mix.
pub struct ChildResizePicture<'a> {
    pub old: &'a super::SnapshotLease,
    pub old_width: f32,
    pub old_height: f32,
    pub mix: f32,
    pub composition: &'a super::SnapshotLease,
}

impl WgpuRenderer {
    /// The scissor rect a child frame's clip resolves to on this surface.
    ///
    /// The crossfade quad needs the same clip the frame's own chrome
    /// computes internally, so the fading old picture cannot paint outside
    /// the popup's placed area.
    pub fn child_frame_scissor(
        &self,
        clip: RootSurfaceRect,
        surface_width: u32,
        surface_height: u32,
    ) -> Option<(u32, u32, u32, u32)> {
        child_scissor(clip, self.scale_factor, surface_width, surface_height)
    }
}

fn child_scissor(
    clip: RootSurfaceRect,
    scale_factor: f32,
    surface_width: u32,
    surface_height: u32,
) -> Option<(u32, u32, u32, u32)> {
    let left = (clip.x() * scale_factor).floor().max(0.0) as u32;
    let top = (clip.y() * scale_factor).floor().max(0.0) as u32;
    let right = ((clip.x() + clip.width()) * scale_factor)
        .ceil()
        .clamp(0.0, surface_width as f32) as u32;
    let bottom = ((clip.y() + clip.height()) * scale_factor)
        .ceil()
        .clamp(0.0, surface_height as f32) as u32;
    if right > left && bottom > top {
        Some((left, top, right - left, bottom - top))
    } else {
        None
    }
}

impl WgpuRenderer {
    /// Draw one snapshot's picture as an alpha-blended quad.
    ///
    /// The resize content crossfade's old-content layer: the previous
    /// presentation's picture, leased from the snapshot pool, fading out
    /// beneath the freshly installed frame. The image pipeline's blended
    /// variant composites `tex_color * vertex_color`, so the alpha rides on
    /// the vertex color and the draw-parameters snapshot stays the shared
    /// identity one.
    pub fn draw_child_crossfade_quad(
        &mut self,
        view: &wgpu::TextureView,
        snapshot_bind_group: &wgpu::BindGroup,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        surface_width: u32,
        surface_height: u32,
        alpha: f32,
        scissor: Option<(u32, u32, u32, u32)>,
    ) {
        self.draw_child_picture_quad(
            view,
            snapshot_bind_group,
            x,
            y,
            width,
            height,
            surface_width,
            surface_height,
            alpha,
            scissor,
            false,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_child_picture_quad(
        &mut self,
        view: &wgpu::TextureView,
        snapshot_bind_group: &wgpu::BindGroup,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        surface_width: u32,
        surface_height: u32,
        alpha: f32,
        scissor: Option<(u32, u32, u32, u32)>,
        additive: bool,
    ) {
        let logical_w = surface_width as f32 / self.scale_factor;
        let logical_h = surface_height as f32 / self.scale_factor;
        let draw = self.parameters([logical_w, logical_h], 0.0);

        let (r, g, b, a) = (1.0_f32, 1.0_f32, 1.0_f32, alpha.clamp(0.0, 1.0));
        let color = [r, g, b, a];
        let mut vertices = Vec::with_capacity(6);
        let quad = |vertices: &mut Vec<GlyphVertex>, x0: f32, y0: f32, x1: f32, y1: f32| {
            for (position, tex_coords) in [
                ([x0, y0], [0.0, 0.0]),
                ([x1, y0], [1.0, 0.0]),
                ([x1, y1], [1.0, 1.0]),
                ([x0, y0], [0.0, 0.0]),
                ([x1, y1], [1.0, 1.0]),
                ([x0, y1], [0.0, 1.0]),
            ] {
                vertices.push(GlyphVertex {
                    position,
                    tex_coords,
                    color,
                });
            }
        };
        quad(&mut vertices, x, y, x + width, y + height);
        if let Some(upload) = self
            .arenas
            .glyph
            .upload(&self.device, &self.queue, &vertices)
        {
            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Child Frame Crossfade Quad Encoder"),
                });
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Child Frame Crossfade Quad Pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                        depth_slice: None,
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                if let Some((sx, sy, sw, sh)) = scissor {
                    pass.set_scissor_rect(sx, sy, sw, sh);
                }
                pass.set_pipeline(if additive {
                    &self.pipelines.crossfade_add
                } else {
                    &self.pipelines.composition
                });
                pass.set_bind_group(0, draw.binding(), &[]);
                pass.set_bind_group(1, snapshot_bind_group, &[]);
                pass.set_vertex_buffer(0, upload.buffer_slice());
                pass.draw(0..vertices.len() as u32, 0..1);
            }
            self.queue.submit(std::iter::once(encoder.finish()));
        }
    }

    /// Render a child frame as a floating overlay on top of the parent frame.
    ///
    /// Draws shadow, background fill, and rounded border, delegates all glyph
    /// rendering (text, cursors, images, etc.) to `render_frame_content()`,
    /// then draws the square outer border.
    /// Uses LoadOp::Load to composite on top of whatever was rendered before.
    ///
    /// `alpha` scales the frame's whole picture — background, border, shadow
    /// and glyphs alike — toward transparent. The composition path passes the
    /// interpolated value of a lifecycle animation combined with legacy
    /// frame opacity. Non-opaque pictures are isolated before composition,
    /// so overlapping background fills never multiply opacity twice.
    #[allow(clippy::too_many_arguments)]
    pub fn render_child_frame(
        &mut self,
        view: &wgpu::TextureView,
        child: &FrameGlyphBuffer,
        offset_x: f32,
        offset_y: f32,
        clip_in_root: RootSurfaceRect,
        glyph_atlas: &mut WgpuGlyphAtlas,
        surface_width: u32,
        surface_height: u32,
        cursor_visible: bool,
        animated_cursor: Option<AnimatedCursor>,
        corner_radius: f32,
        shadow_enabled: bool,
        shadow_layers: u32,
        shadow_offset: f32,
        shadow_opacity: f32,
        pointer_selection: Option<PointerAppearanceSelection>,
        alpha: f32,
        scale: f32,
        pivot: [f32; 2],
    ) -> Result<(), super::BudgetExceeded> {
        let size = super::SnapshotSize::new(surface_width, surface_height)
            .expect("child target dimensions are nonzero");
        let picture = if child.background_alpha != 1.0 || alpha != 1.0 {
            Some(self.acquire_snapshot(size)?)
        } else {
            None
        };
        self.render_child_frame_prepared(
            view,
            child,
            offset_x,
            offset_y,
            clip_in_root,
            glyph_atlas,
            surface_width,
            surface_height,
            cursor_visible,
            animated_cursor,
            corner_radius,
            shadow_enabled,
            shadow_layers,
            shadow_offset,
            shadow_opacity,
            pointer_selection,
            alpha,
            scale,
            pivot,
            picture.as_ref(),
            None,
        );
        Ok(())
    }

    /// Draw using a mandatory composition lease reserved before acquisition
    /// and presentation sampling. All children in one pass may reuse it.
    #[allow(clippy::too_many_arguments)]
    pub fn render_child_frame_prepared(
        &mut self,
        view: &wgpu::TextureView,
        child: &FrameGlyphBuffer,
        offset_x: f32,
        offset_y: f32,
        clip_in_root: RootSurfaceRect,
        glyph_atlas: &mut WgpuGlyphAtlas,
        surface_width: u32,
        surface_height: u32,
        cursor_visible: bool,
        animated_cursor: Option<AnimatedCursor>,
        corner_radius: f32,
        shadow_enabled: bool,
        shadow_layers: u32,
        shadow_offset: f32,
        shadow_opacity: f32,
        pointer_selection: Option<PointerAppearanceSelection>,
        alpha: f32,
        scale: f32,
        pivot: [f32; 2],
        picture: Option<&super::SnapshotLease>,
        resize: Option<ChildResizePicture<'_>>,
    ) {
        let alpha = if alpha.is_finite() {
            alpha.clamp(0.0, 1.0)
        } else {
            1.0
        };
        if child.background_alpha != 1.0 || alpha != 1.0 || resize.is_some() {
            let picture = picture.expect("non-opaque child composition was preflighted");
            self.clear_child_picture(picture.view());
            // Backgrounds replace one another inside the child's own picture;
            // text is source-over coverage. Apply lifecycle opacity only once,
            // to the completed premultiplied picture, not overlapping fills.
            self.render_child_frame_picture(
                picture.view(),
                child,
                offset_x,
                offset_y,
                clip_in_root,
                glyph_atlas,
                surface_width,
                surface_height,
                cursor_visible,
                animated_cursor,
                corner_radius,
                shadow_enabled,
                shadow_layers,
                shadow_offset,
                shadow_opacity,
                pointer_selection,
                1.0,
                scale,
                pivot,
            );
            let composed = if let Some(resize) = resize {
                let mixed = resize.composition;
                self.clear_child_picture(mixed.view());
                let scissor = self.child_frame_scissor(clip_in_root, surface_width, surface_height);
                // Both pictures are already premultiplied. Sum their weighted
                // RGBA, then source-over the complete result once below.
                self.draw_child_picture_quad(
                    mixed.view(),
                    resize.old.bind_group(),
                    offset_x,
                    offset_y,
                    resize.old_width * scale,
                    resize.old_height * scale,
                    surface_width,
                    surface_height,
                    1.0 - resize.mix,
                    scissor,
                    true,
                );
                self.draw_child_picture_quad(
                    mixed.view(),
                    picture.bind_group(),
                    0.0,
                    0.0,
                    surface_width as f32 / self.scale_factor,
                    surface_height as f32 / self.scale_factor,
                    surface_width,
                    surface_height,
                    resize.mix,
                    None,
                    true,
                );
                mixed
            } else {
                picture
            };
            self.draw_child_crossfade_quad(
                view,
                composed.bind_group(),
                0.0,
                0.0,
                surface_width as f32 / self.scale_factor,
                surface_height as f32 / self.scale_factor,
                surface_width,
                surface_height,
                alpha,
                None,
            );
            return;
        }
        self.render_child_frame_picture(
            view,
            child,
            offset_x,
            offset_y,
            clip_in_root,
            glyph_atlas,
            surface_width,
            surface_height,
            cursor_visible,
            animated_cursor,
            corner_radius,
            shadow_enabled,
            shadow_layers,
            shadow_offset,
            shadow_opacity,
            pointer_selection,
            alpha,
            scale,
            pivot,
        );
    }

    /// Capture the complete unscaled child picture in an already budgeted lease.
    /// The caller includes shadow extent in its dimensions; opacity is not baked.
    #[allow(clippy::too_many_arguments)]
    pub fn capture_child_frame_picture(
        &mut self,
        picture: &super::SnapshotLease,
        child: &FrameGlyphBuffer,
        atlas: &mut WgpuGlyphAtlas,
        corner_radius: f32,
        shadow_enabled: bool,
        shadow_layers: u32,
        shadow_offset: f32,
        shadow_opacity: f32,
    ) {
        self.clear_child_picture(picture.view());
        atlas.set_current_frame_fonts(child.font_bindings());
        let w = picture.size().width();
        let h = picture.size().height();
        // Only the target-local clip attachment changes. Do not resize or
        // reconfigure an installed native surface to the capture dimensions.
        let previous_size = (
            self.stencil.texture.view().texture().width(),
            self.stencil.texture.view().texture().height(),
        );
        if previous_size != (w, h) {
            self.install_stencil_targets(w, h);
        }
        self.render_child_frame_picture(
            picture.view(),
            child,
            0.0,
            0.0,
            RootSurfaceRect::new(
                0.0,
                0.0,
                w as f32 / self.scale_factor,
                h as f32 / self.scale_factor,
            )
            .unwrap(),
            atlas,
            w,
            h,
            false,
            None,
            corner_radius,
            shadow_enabled,
            shadow_layers,
            shadow_offset,
            shadow_opacity,
            None,
            1.0,
            1.0,
            [0.0; 2],
        );
        if previous_size != (w, h) {
            self.install_stencil_targets(previous_size.0, previous_size.1);
        }
    }

    fn clear_child_picture(&self, view: &wgpu::TextureView) {
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Child Picture Clear"),
            });
        {
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Child Picture Clear"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
        }
        self.queue.submit([encoder.finish()]);
    }

    #[allow(clippy::too_many_arguments)]
    fn render_child_frame_picture(
        &mut self,
        view: &wgpu::TextureView,
        child: &FrameGlyphBuffer,
        offset_x: f32,
        offset_y: f32,
        clip_in_root: RootSurfaceRect,
        glyph_atlas: &mut WgpuGlyphAtlas,
        surface_width: u32,
        surface_height: u32,
        cursor_visible: bool,
        animated_cursor: Option<AnimatedCursor>,
        corner_radius: f32,
        shadow_enabled: bool,
        shadow_layers: u32,
        shadow_offset: f32,
        shadow_opacity: f32,
        pointer_selection: Option<PointerAppearanceSelection>,
        alpha: f32,
        scale: f32,
        pivot: [f32; 2],
    ) {
        // An identity scale normalizes to (1.0, origin) so the draw-parameters
        // cache key -- and therefore the immutable uniform snapshot -- is
        // byte-identical to the pre-scale pipeline for every settled frame.
        let (scale, pivot) = if !scale.is_finite() || (scale - 1.0).abs() < f32::EPSILON {
            (1.0, [0.0_f32; 2])
        } else {
            (scale, pivot)
        };
        let logical_w = surface_width as f32 / self.scale_factor;
        let logical_h = surface_height as f32 / self.scale_factor;
        // Same cache key as `parameters()` when alpha is 1.0 and the scale is
        // the identity, so a settled child frame reuses the identical
        // immutable snapshot.
        let draw = self.parameters_for([logical_w, logical_h], 0.0, alpha, scale, pivot);

        let bw = child.border_width;
        // The CPU-built chrome geometry scales by the same factor the vertex
        // shader applies to glyph positions: the frame grows out of its
        // top-left anchor, so the chrome's width and height shrink toward it
        // and its radii and border widths thin proportionally.
        let frame_w = child.width * scale;
        let frame_h = child.height * scale;
        let corner_radius = corner_radius * scale;
        let bg_alpha = child.background_alpha * alpha;
        let Some(scissor) = child_scissor(
            clip_in_root,
            self.scale_factor,
            surface_width,
            surface_height,
        ) else {
            return;
        };

        tracing::debug!(
            "render_child_frame: size={:.0}x{:.0} offset=({:.1},{:.1}) border={:.1} glyphs={}",
            frame_w,
            frame_h,
            offset_x,
            offset_y,
            bw,
            child.glyphs.len(),
        );

        // Child-frame-specific rendering: shadow + background + border.
        {
            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Child Frame Chrome Encoder"),
                });
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Child Frame Chrome Pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                        depth_slice: None,
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                pass.set_scissor_rect(scissor.0, scissor.1, scissor.2, scissor.3);

                if shadow_enabled && shadow_layers > 0 {
                    let mut shadow_verts: Vec<RectVertex> = Vec::new();
                    let total_w = frame_w;
                    let total_h = frame_h;
                    let sx = offset_x;
                    let sy = offset_y;
                    for layer in (1..=shadow_layers).rev() {
                        let off = layer as f32 * shadow_offset;
                        let layer_alpha = shadow_opacity
                            * (1.0 - (layer - 1) as f32 / shadow_layers as f32)
                            * alpha;
                        let c = Color::new(0.0, 0.0, 0.0, layer_alpha);
                        self.add_rect(&mut shadow_verts, sx + off, sy + total_h, total_w, off, &c);
                        self.add_rect(&mut shadow_verts, sx + total_w, sy + off, off, total_h, &c);
                        self.add_rect(&mut shadow_verts, sx + total_w, sy + total_h, off, off, &c);
                    }
                    if let Some(upload) =
                        self.arenas
                            .rect
                            .upload(&self.device, &self.queue, &shadow_verts)
                    {
                        pass.set_pipeline(&self.pipelines.rect);
                        pass.set_bind_group(0, draw.binding(), &[]);
                        pass.set_vertex_buffer(0, upload.buffer_slice());
                        pass.draw(0..shadow_verts.len() as u32, 0..1);
                    }
                }
            }

            {
                let bg = Color::new(
                    child.background.r,
                    child.background.g,
                    child.background.b,
                    bg_alpha,
                );
                if corner_radius > 0.0 {
                    let mut bg_verts: Vec<RoundedRectVertex> = Vec::new();
                    self.add_rounded_rect(
                        &mut bg_verts,
                        offset_x,
                        offset_y,
                        frame_w,
                        frame_h,
                        0.0,
                        corner_radius,
                        &bg,
                    );
                    if !bg_verts.is_empty() {
                        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                            label: Some("Child Frame BG Pass"),
                            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                view,
                                resolve_target: None,
                                ops: wgpu::Operations {
                                    load: wgpu::LoadOp::Load,
                                    store: wgpu::StoreOp::Store,
                                },
                                depth_slice: None,
                            })],
                            depth_stencil_attachment: None,
                            timestamp_writes: None,
                            occlusion_query_set: None,
                            multiview_mask: None,
                        });
                        pass.set_scissor_rect(scissor.0, scissor.1, scissor.2, scissor.3);
                        if let Some(upload) =
                            self.arenas
                                .rounded
                                .upload(&self.device, &self.queue, &bg_verts)
                        {
                            pass.set_pipeline(&self.pipelines.rounded_fill);
                            pass.set_bind_group(0, draw.binding(), &[]);
                            pass.set_vertex_buffer(0, upload.buffer_slice());
                            pass.draw(0..bg_verts.len() as u32, 0..1);
                        }
                    }
                } else {
                    let mut bg_verts: Vec<RectVertex> = Vec::new();
                    self.add_rect(&mut bg_verts, offset_x, offset_y, frame_w, frame_h, &bg);
                    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("Child Frame BG Pass"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Load,
                                store: wgpu::StoreOp::Store,
                            },
                            depth_slice: None,
                        })],
                        depth_stencil_attachment: None,
                        timestamp_writes: None,
                        occlusion_query_set: None,
                        multiview_mask: None,
                    });
                    pass.set_scissor_rect(scissor.0, scissor.1, scissor.2, scissor.3);
                    if let Some(upload) =
                        self.arenas
                            .rect
                            .upload(&self.device, &self.queue, &bg_verts)
                    {
                        pass.set_pipeline(&self.pipelines.rect);
                        pass.set_bind_group(0, draw.binding(), &[]);
                        pass.set_vertex_buffer(0, upload.buffer_slice());
                        pass.draw(0..bg_verts.len() as u32, 0..1);
                    }
                }
            }

            if bw > 0.0 || corner_radius > 0.0 {
                let mut border_verts: Vec<RoundedRectVertex> = Vec::new();
                let bc = if bw > 0.0 {
                    child.border_color
                } else {
                    Color::new(0.5, 0.5, 0.5, 0.3).srgb_to_linear()
                };
                let effective_bw = (if bw > 0.0 { bw } else { 1.0 }) * scale;
                self.add_rounded_rect(
                    &mut border_verts,
                    offset_x,
                    offset_y,
                    frame_w,
                    frame_h,
                    effective_bw,
                    corner_radius,
                    &bc,
                );
                if !border_verts.is_empty() {
                    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("Child Frame Border Pass"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Load,
                                store: wgpu::StoreOp::Store,
                            },
                            depth_slice: None,
                        })],
                        depth_stencil_attachment: None,
                        timestamp_writes: None,
                        occlusion_query_set: None,
                        multiview_mask: None,
                    });
                    pass.set_scissor_rect(scissor.0, scissor.1, scissor.2, scissor.3);
                    if let Some(upload) =
                        self.arenas
                            .rounded
                            .upload(&self.device, &self.queue, &border_verts)
                    {
                        pass.set_pipeline(&self.pipelines.rounded_rect);
                        pass.set_bind_group(0, draw.binding(), &[]);
                        pass.set_vertex_buffer(0, upload.buffer_slice());
                        pass.draw(0..border_verts.len() as u32, 0..1);
                    }
                }
            }

            self.queue.submit(std::iter::once(encoder.finish()));
        }

        // Stencil-write pass: write rounded rect shape into stencil buffer
        // so content rendering clips to the rounded corners.
        if corner_radius > 0.0 {
            let mut stencil_verts: Vec<RoundedRectVertex> = Vec::new();
            self.add_rounded_rect(
                &mut stencil_verts,
                offset_x,
                offset_y,
                frame_w,
                frame_h,
                0.0, // filled (no border)
                corner_radius,
                &Color::new(1.0, 1.0, 1.0, 1.0), // color irrelevant, writes disabled
            );
            if !stencil_verts.is_empty() {
                let mut encoder =
                    self.device
                        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                            label: Some("Stencil Write Encoder"),
                        });
                {
                    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("Stencil Write Pass"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Load,
                                store: wgpu::StoreOp::Store,
                            },
                            depth_slice: None,
                        })],
                        depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                            view: self.stencil.texture.view(),
                            depth_ops: None,
                            stencil_ops: Some(wgpu::Operations {
                                load: wgpu::LoadOp::Clear(0),
                                store: wgpu::StoreOp::Store,
                            }),
                        }),
                        timestamp_writes: None,
                        occlusion_query_set: None,
                        multiview_mask: None,
                    });
                    pass.set_scissor_rect(scissor.0, scissor.1, scissor.2, scissor.3);
                    if let Some(upload) =
                        self.arenas
                            .rounded
                            .upload(&self.device, &self.queue, &stencil_verts)
                    {
                        pass.set_pipeline(&self.pipelines.stencil_write);
                        pass.set_bind_group(0, draw.binding(), &[]);
                        pass.set_vertex_buffer(0, upload.buffer_slice());
                        pass.set_stencil_reference(1);
                        pass.draw(0..stencil_verts.len() as u32, 0..1);
                    }
                }
                self.queue.submit(std::iter::once(encoder.finish()));
            }
        }

        self.render_frame_content(
            view,
            child,
            glyph_atlas,
            surface_width,
            surface_height,
            offset_x,
            offset_y,
            cursor_visible,
            animated_cursor,
            corner_radius,
            pointer_selection,
            Some(scissor),
            alpha,
            scale,
            pivot,
        );

        let outer_bw = (child.outer_border_width * scale)
            .max(0.0)
            .min(frame_w.max(0.0) / 2.0)
            .min(frame_h.max(0.0) / 2.0);
        if outer_bw > 0.0 {
            let mut outer_border_verts: Vec<RectVertex> = Vec::new();
            self.add_rect(
                &mut outer_border_verts,
                offset_x,
                offset_y,
                frame_w,
                outer_bw,
                &child.outer_border_color,
            );
            self.add_rect(
                &mut outer_border_verts,
                offset_x,
                offset_y + frame_h - outer_bw,
                frame_w,
                outer_bw,
                &child.outer_border_color,
            );
            self.add_rect(
                &mut outer_border_verts,
                offset_x,
                offset_y,
                outer_bw,
                frame_h,
                &child.outer_border_color,
            );
            self.add_rect(
                &mut outer_border_verts,
                offset_x + frame_w - outer_bw,
                offset_y,
                outer_bw,
                frame_h,
                &child.outer_border_color,
            );
            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Child Frame Outer Border Encoder"),
                });
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Child Frame Outer Border Pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                        depth_slice: None,
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                pass.set_scissor_rect(scissor.0, scissor.1, scissor.2, scissor.3);
                if let Some(upload) =
                    self.arenas
                        .rect
                        .upload(&self.device, &self.queue, &outer_border_verts)
                {
                    pass.set_pipeline(&self.pipelines.rect);
                    pass.set_bind_group(0, draw.binding(), &[]);
                    pass.set_vertex_buffer(0, upload.buffer_slice());
                    pass.draw(0..outer_border_verts.len() as u32, 0..1);
                }
            }
            self.queue.submit(std::iter::once(encoder.finish()));
        }
    }
}

#[cfg(test)]
#[path = "child_frames/tests/child_frames_test.rs"]
mod tests;
