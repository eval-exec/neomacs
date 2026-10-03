//! Cached-scene copying is a target-local draw, not an implicit fullscreen operation.
use super::super::{RenderTarget, WgpuRenderer, draw::DrawParameters};
use crate::vertex::GlyphVertex;

pub(in crate::renderer) enum BlitPlacement {
    Retained,
    NativeContent(neomacs_display_protocol::Color),
    Region {
        uv: neomacs_display_protocol::Rect,
        destination: neomacs_display_protocol::Rect,
    },
}

impl WgpuRenderer {
    pub(in crate::renderer) fn paint_blit(
        &mut self,
        target: RenderTarget<'_>,
        draw: &DrawParameters,
        src_bind_group: &wgpu::BindGroup,
        placement: BlitPlacement,
    ) {
        let dst_view = target.view;
        let size = target.surface.logical_size();
        let (uv, load) = match &placement {
            BlitPlacement::Region { uv, .. } => (*uv, true),
            _ => (
                neomacs_display_protocol::Rect::new(0.0, 0.0, 1.0, 1.0),
                false,
            ),
        };
        let (x, y, w, h, clear, pipeline) = match placement {
            BlitPlacement::Retained => (
                0.0,
                0.0,
                size.width(),
                size.height(),
                wgpu::Color::TRANSPARENT,
                &self.pipelines.surface_copy,
            ),
            BlitPlacement::Region { destination, .. } => (
                destination.x,
                destination.y,
                destination.width,
                destination.height,
                wgpu::Color::TRANSPARENT,
                &self.pipelines.surface_copy,
            ),
            BlitPlacement::NativeContent(bg) => {
                let insets = target.surface.content_insets();
                let scale = target.surface.device_scale().get();
                let (w, h) = insets.content_size(
                    target.surface.device_width().get(),
                    target.surface.device_height().get(),
                );
                // Native sRGB content stores encoded-premultiplied RGB.
                // Clears are supplied in linear space before attachment encoding,
                // so perform the same encode -> multiply -> decode as fs_native.
                let straight = if self.surface_format.is_srgb() {
                    bg.linear_to_srgb()
                } else {
                    bg
                };
                let premultiplied = neomacs_display_protocol::Color::new(
                    straight.r * bg.a,
                    straight.g * bg.a,
                    straight.b * bg.a,
                    bg.a,
                );
                let clear = if self.surface_format.is_srgb() {
                    premultiplied.srgb_to_linear()
                } else {
                    premultiplied
                };
                (
                    insets.left as f32 / scale,
                    insets.top as f32 / scale,
                    w as f32 / scale,
                    h as f32 / scale,
                    wgpu::Color {
                        r: clear.r as f64,
                        g: clear.g as f64,
                        b: clear.b as f64,
                        a: clear.a as f64,
                    },
                    &self.pipelines.native_copy,
                )
            }
        };
        let mut vertices = [
            GlyphVertex {
                position: [0.0, 0.0],
                tex_coords: [0.0, 0.0],
                color: [1.0, 1.0, 1.0, 1.0],
            },
            GlyphVertex {
                position: [w, 0.0],
                tex_coords: [1.0, 0.0],
                color: [1.0, 1.0, 1.0, 1.0],
            },
            GlyphVertex {
                position: [w, h],
                tex_coords: [1.0, 1.0],
                color: [1.0, 1.0, 1.0, 1.0],
            },
            GlyphVertex {
                position: [0.0, 0.0],
                tex_coords: [0.0, 0.0],
                color: [1.0, 1.0, 1.0, 1.0],
            },
            GlyphVertex {
                position: [w, h],
                tex_coords: [1.0, 1.0],
                color: [1.0, 1.0, 1.0, 1.0],
            },
            GlyphVertex {
                position: [0.0, h],
                tex_coords: [0.0, 1.0],
                color: [1.0, 1.0, 1.0, 1.0],
            },
        ];
        for vertex in &mut vertices {
            vertex.position[0] += x;
            vertex.position[1] += y;
            vertex.tex_coords[0] = uv.x + vertex.tex_coords[0] * uv.width;
            vertex.tex_coords[1] = uv.y + vertex.tex_coords[1] * uv.height;
        }

        let upload = self
            .arenas
            .image
            .upload(&self.device, &self.queue, &vertices);

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Blit Encoder"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Blit Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: dst_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: if load {
                            wgpu::LoadOp::Load
                        } else {
                            wgpu::LoadOp::Clear(clear)
                        },
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            if let Some(ref upload) = upload {
                render_pass.set_pipeline(pipeline);
                render_pass.set_bind_group(0, draw.binding(), &[]);
                render_pass.set_bind_group(1, src_bind_group, &[]);
                render_pass.set_vertex_buffer(0, upload.buffer_slice());
                render_pass.draw(0..6, 0..1);
            }
        }

        self.queue.submit(std::iter::once(encoder.finish()));
    }
}
