//! Cached-scene copying is a target-local draw, not an implicit fullscreen operation.
use super::super::{RenderTarget, WgpuRenderer, draw::DrawParameters};
use crate::vertex::GlyphVertex;

impl WgpuRenderer {
    pub(in crate::renderer) fn paint_blit(
        &mut self,
        target: RenderTarget<'_>,
        draw: &DrawParameters,
        src_bind_group: &wgpu::BindGroup,
    ) {
        let dst_view = target.view;
        let size = target.surface.logical_size();
        let w = size.width();
        let h = size.height();
        let vertices = [
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

            if let Some(ref upload) = upload {
                render_pass.set_pipeline(&self.pipelines.image);
                render_pass.set_bind_group(0, draw.binding(), &[]);
                render_pass.set_bind_group(1, src_bind_group, &[]);
                render_pass.set_vertex_buffer(0, upload.buffer_slice());
                render_pass.draw(0..6, 0..1);
            }
        }

        self.queue.submit(std::iter::once(encoder.finish()));
    }
}
