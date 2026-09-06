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
//! # Why the source region is anchored at the pane's destination origin
//!
//! A pane shows its destination content throughout the motion — that is what
//! makes the interaction projection a plain translation. So the region of the
//! composed picture it samples starts at its *destination* origin, extends by
//! its *current* size, and is drawn into its *current* rect at natural scale.
//! Stretching destination-sized content to fit would resample the glyphs and
//! read as a zoom.
//!
//! One consequence is worth naming, because it looks like a bug and is not.
//! A pane still larger than its destination samples past its destination edge,
//! into the region the picture gives to whatever sits there — its new
//! neighbour, or the mode line and echo area below. As the pane's quad narrows
//! toward its final size, that borrowed strip narrows with it and the real
//! neighbour is uncovered underneath. The result reads as the old window
//! shrinking to reveal the new one, which is what a split looks like, and it
//! is why the quad itself is the clip: there is nothing to draw outside it.

use crate::renderer::{GlyphVertex, WgpuRenderer};
use neomacs_display_protocol::types::Rect;

/// Which picture a pane's pixels come from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaneSource {
    /// The frame just composed. Every pane that still exists reads from here.
    Destination,
    /// The composition before it, which is the only place a pane the
    /// destination no longer contains still has pixels. A window that has been
    /// deleted is absent from the new presentation entirely, so there is
    /// nothing of it to sample there — without this it can only vanish.
    Previous,
}

/// One pane's placement for this frame, in logical pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PaneBlit {
    /// Where the pane is drawn right now.
    pub bounds: Rect,
    /// Where in its source picture its content lives.
    pub content_origin: (f32, f32),
    /// How much of the source the pane owns, at its destination size.
    ///
    /// A travelling pane is often larger than the content it will settle into.
    /// Sampling `bounds` worth from `content_origin` would then read past its
    /// own edge into whatever the destination puts beside it — and because the
    /// composed picture is the destination, that read is pixel-identical to
    /// what is already on the target, so the pane draws nothing visible and the
    /// motion is invisible. Clamping to the extent it actually owns is what
    /// makes the area it has not yet claimed show the frame underneath.
    pub content_extent: (f32, f32),
    pub source: PaneSource,
    /// How opaque to draw it, for a pane entering or leaving.
    pub opacity: f32,
}

impl WgpuRenderer {
    /// Draw `panes` from the composed picture in `source` onto `destination`.
    ///
    /// `frame_size` is the composed picture's logical size, which is what the
    /// source coordinates are relative to.
    pub fn render_pane_layout(
        &mut self,
        source: &wgpu::BindGroup,
        previous: Option<&wgpu::BindGroup>,
        destination: &wgpu::TextureView,
        frame_size: (f32, f32),
        panes: &[PaneBlit],
    ) {
        let (frame_width, frame_height) = frame_size;
        if frame_width <= 0.0 || frame_height <= 0.0 || panes.is_empty() {
            return;
        }

        let corner = |x: f32, y: f32, u: f32, v: f32, a: f32| GlyphVertex {
            position: [x, y],
            tex_coords: [u, v],
            // Premultiplied by the pipeline's blend, so the alpha alone carries
            // a departing pane's fade; there is no separate opacity uniform to
            // keep in step with the geometry.
            color: [1.0, 1.0, 1.0, a],
        };

        let mut vertices = Vec::with_capacity((panes.len() + 1) * 6);
        let mut departing = Vec::new();
        // The settled picture, underneath everything: the frame the motion is
        // travelling toward. It is a backdrop, not the animation — every pane
        // draws over it at where the motion currently puts it, and the parts of
        // the frame no pane owns (echo area, tab bar, tool bar, padding) are
        // visible because of it.
        //
        // What makes the motion visible is the `Previous`-sourced group drawn
        // on top of this and fading out. That group is the only place the
        // picture from *before* the change exists, which is why the composition
        // ring has to already be warm when the change lands: see
        // `RenderFeaturePlan::compose_offscreen`.
        vertices.extend_from_slice(&[
            corner(0.0, 0.0, 0.0, 0.0, 1.0),
            corner(frame_width, 0.0, 1.0, 0.0, 1.0),
            corner(frame_width, frame_height, 1.0, 1.0, 1.0),
            corner(0.0, 0.0, 0.0, 0.0, 1.0),
            corner(frame_width, frame_height, 1.0, 1.0, 1.0),
            corner(0.0, frame_height, 0.0, 1.0, 1.0),
        ]);
        // Quads are grouped by source, because a bind group cannot change
        // within one draw call.
        for pane in panes {
            // The pane samples its own size from the picture, not its
            // destination's: while it is still wider than it will end up, it
            // shows more of the row, and the extra is clipped by its own quad
            // rather than squeezed into it.
            // Never past what this pane owns: see `content_extent`.
            let drawn_w = pane.bounds.width.min(pane.content_extent.0);
            let drawn_h = pane.bounds.height.min(pane.content_extent.1);
            let u0 = pane.content_origin.0 / frame_width;
            let v0 = pane.content_origin.1 / frame_height;
            let u1 = (pane.content_origin.0 + drawn_w) / frame_width;
            let v1 = (pane.content_origin.1 + drawn_h) / frame_height;
            let x0 = pane.bounds.x;
            let y0 = pane.bounds.y;
            let x1 = pane.bounds.x + drawn_w;
            let y1 = pane.bounds.y + drawn_h;
            let quad = [
                corner(x0, y0, u0, v0, pane.opacity),
                corner(x1, y0, u1, v0, pane.opacity),
                corner(x1, y1, u1, v1, pane.opacity),
                corner(x0, y0, u0, v0, pane.opacity),
                corner(x1, y1, u1, v1, pane.opacity),
                corner(x0, y1, u0, v1, pane.opacity),
            ];
            match pane.source {
                PaneSource::Destination => vertices.extend_from_slice(&quad),
                // Dropped silently when there is no previous composition — the
                // first frame a window is ever drawn on has no history to fade
                // from, and inventing one would fade in from an empty texture.
                PaneSource::Previous if previous.is_some() => departing.extend_from_slice(&quad),
                PaneSource::Previous => {}
            }
        }
        // The previous-sourced group draws last, over the destination: it
        // holds the outgoing picture at `1.0 - content_mix`, so it starts
        // opaque — the frame as it was — and fades to reveal the new layout
        // underneath. Drawn first it would be hidden from the very first frame
        // and the motion would be invisible again.
        let destination_vertices = vertices.len() as u32;
        vertices.extend_from_slice(&departing);
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
                        // Cleared rather than loaded: whatever was on the
                        // target is a picture of the *previous* frame, and
                        // letting it show through would put two layouts on
                        // screen at once. The base quad above covers the target
                        // completely, so nothing is left cleared.
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
            pass.draw(0..destination_vertices, 0..1);
            if destination_vertices < vertices.len() as u32
                && let Some(previous) = previous
            {
                pass.set_bind_group(1, previous, &[]);
                pass.draw(destination_vertices..vertices.len() as u32, 0..1);
            }
        }
        self.queue.submit(std::iter::once(encoder.finish()));
    }
}

#[cfg(test)]
#[path = "layout_pass_test.rs"]
mod tests;
