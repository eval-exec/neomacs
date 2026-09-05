//! Drawing one composed presentation with its panes somewhere other than
//! where the presentation puts them.
//!
//! Everything up to here produces a picture of the *destination*: the frame as
//! it will look once the layout change has settled. A pane morph does not
//! change that picture — it changes where each piece of it is drawn. So the
//! composed frame is rendered once into a texture and then blitted back one
//! pane at a time, each from the region of the picture the pane owns to the
//! rect the pane currently occupies.
//!
//! # Why not re-render per pane
//!
//! Re-running layout for each pane at each intermediate size would be both far
//! more expensive and *wrong*: it would reflow the text as the pane narrowed,
//! so the user would watch their lines rewrap dozens of times on the way to a
//! width they never asked to read at. Sampling one finished picture keeps the
//! text exactly as it will be when the motion ends; only its position moves.
//!
//! # Why the source region is the pane's destination rect
//!
//! A pane shows its destination content throughout the motion — that is what
//! makes the interaction projection a plain translation. The region of the
//! composed picture it samples is therefore its *destination* rect, and the
//! quad it draws into is its *current* rect. When a pane is still larger than
//! its destination the two differ in size, and the content is drawn at its
//! natural scale from the top-left rather than stretched: stretching would
//! resample the glyphs and read as a zoom.

use crate::renderer::{GlyphVertex, WgpuRenderer};
use neomacs_display_protocol::types::Rect;

/// One pane's placement for this frame, in logical pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PaneBlit {
    /// Where the pane is drawn right now.
    pub bounds: Rect,
    /// Where in the composed picture its content lives.
    pub content_origin: (f32, f32),
}

impl WgpuRenderer {
    /// Draw `panes` from the composed picture in `source` onto `destination`.
    ///
    /// `frame_size` is the composed picture's logical size, which is what the
    /// source coordinates are relative to.
    pub fn render_pane_layout(
        &mut self,
        source: &wgpu::BindGroup,
        destination: &wgpu::TextureView,
        frame_size: (f32, f32),
        panes: &[PaneBlit],
    ) {
        let (frame_width, frame_height) = frame_size;
        if frame_width <= 0.0 || frame_height <= 0.0 || panes.is_empty() {
            return;
        }

        let mut vertices = Vec::with_capacity(panes.len() * 6);
        for pane in panes {
            // The pane samples its own size from the picture, not its
            // destination's: while it is still wider than it will end up, it
            // shows more of the row, and the extra is clipped by its own quad
            // rather than squeezed into it.
            let u0 = pane.content_origin.0 / frame_width;
            let v0 = pane.content_origin.1 / frame_height;
            let u1 = (pane.content_origin.0 + pane.bounds.width) / frame_width;
            let v1 = (pane.content_origin.1 + pane.bounds.height) / frame_height;
            let x0 = pane.bounds.x;
            let y0 = pane.bounds.y;
            let x1 = pane.bounds.x + pane.bounds.width;
            let y1 = pane.bounds.y + pane.bounds.height;
            let corner = |x: f32, y: f32, u: f32, v: f32| GlyphVertex {
                position: [x, y],
                tex_coords: [u, v],
                color: [1.0, 1.0, 1.0, 1.0],
            };
            vertices.extend_from_slice(&[
                corner(x0, y0, u0, v0),
                corner(x1, y0, u1, v0),
                corner(x1, y1, u1, v1),
                corner(x0, y0, u0, v0),
                corner(x1, y1, u1, v1),
                corner(x0, y1, u0, v1),
            ]);
        }

        let upload = self
            .arenas
            .image
            .upload(&self.device, &self.queue, &vertices);
        let Some(upload) = upload else {
            return;
        };

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Pane Layout Encoder"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Pane Layout Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: destination,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        // Cleared, not loaded: mid-morph the panes do not tile
                        // the frame, and whatever was on screen underneath is a
                        // picture of the *previous* layout. Leaving it visible
                        // through the gaps would show two layouts at once.
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
            pass.set_pipeline(&self.pipelines.image);
            pass.set_bind_group(0, &self.uniform_bind_group, &[]);
            pass.set_bind_group(1, source, &[]);
            pass.set_vertex_buffer(0, upload.buffer_slice());
            pass.draw(0..vertices.len() as u32, 0..1);
        }
        self.queue.submit(std::iter::once(encoder.finish()));
    }
}

#[cfg(test)]
#[path = "layout_pass_test.rs"]
mod tests;
