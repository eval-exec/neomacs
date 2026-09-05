//! Child frame rendering methods for WgpuRenderer.

use super::super::glyph_atlas::WgpuGlyphAtlas;
use super::super::vertex::{RectVertex, RoundedRectVertex, Uniforms};
use super::WgpuRenderer;
use neomacs_display_protocol::frame_glyphs::FrameGlyphBuffer;
use neomacs_display_protocol::types::{AnimatedCursor, Color};
use neomacs_display_protocol::{PointerAppearanceSelection, RootSurfaceRect};

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
    /// Render a child frame as a floating overlay on top of the parent frame.
    ///
    /// Draws shadow, background fill, and rounded border, delegates all glyph
    /// rendering (text, cursors, images, etc.) to `render_frame_content()`,
    /// then draws the square outer border.
    /// Uses LoadOp::Load to composite on top of whatever was rendered before.
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
    ) {
        let logical_w = surface_width as f32 / self.scale_factor;
        let logical_h = surface_height as f32 / self.scale_factor;
        let uniforms = Uniforms {
            screen_size: [logical_w, logical_h],
            time: 0.0,
            _padding: 0.0,
        };
        self.queue
            .write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));

        let bw = child.border_width;
        let frame_w = child.width;
        let frame_h = child.height;
        let bg_alpha = child.background_alpha;
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
                        let alpha =
                            shadow_opacity * (1.0 - (layer - 1) as f32 / shadow_layers as f32);
                        let c = Color::new(0.0, 0.0, 0.0, alpha);
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
                        pass.set_bind_group(0, &self.uniform_bind_group, &[]);
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
                            pass.set_pipeline(&self.pipelines.rounded_rect);
                            pass.set_bind_group(0, &self.uniform_bind_group, &[]);
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
                        pass.set_bind_group(0, &self.uniform_bind_group, &[]);
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
                let effective_bw = if bw > 0.0 { bw } else { 1.0 };
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
                        pass.set_bind_group(0, &self.uniform_bind_group, &[]);
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
                        pass.set_bind_group(0, &self.uniform_bind_group, &[]);
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
        );

        let outer_bw = child
            .outer_border_width
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
                    pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    pass.set_vertex_buffer(0, upload.buffer_slice());
                    pass.draw(0..outer_border_verts.len() as u32, 0..1);
                }
            }
            self.queue.submit(std::iter::once(encoder.finish()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_child_clip_becomes_one_physical_scissor_for_all_render_passes() {
        let clip = RootSurfaceRect::new(10.25, 20.5, 30.25, 40.0).unwrap();
        assert_eq!(child_scissor(clip, 1.5, 200, 200), Some((15, 30, 46, 61)));
    }

    #[test]
    fn child_clip_outside_surface_has_no_renderable_scissor() {
        let clip = RootSurfaceRect::new(300.0, 300.0, 20.0, 20.0).unwrap();
        assert_eq!(child_scissor(clip, 1.0, 200, 200), None);
    }
}
