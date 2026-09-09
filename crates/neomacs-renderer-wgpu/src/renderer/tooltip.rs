//! Target-local native tooltip painting. No window ownership or hover policy.
use super::super::glyph_atlas::{GlyphAtlasHandle, GlyphKey, SubpixelRequest, WgpuGlyphAtlas};
use super::super::vertex::{RectVertex, Uniforms};
use super::WgpuRenderer;
use cosmic_text::SubpixelBin;
use neomacs_display_protocol::types::{Color, FaceId};

fn glyph_key(
    atlas: &WgpuGlyphAtlas,
    ch: char,
    face: Option<&neomacs_display_protocol::Face>,
) -> GlyphKey {
    GlyphKey {
        charcode: ch as u32,
        face_id: face.map_or(FaceId::new(0), |face| face.id),
        font_size_bits: face.map_or(0.0_f32.to_bits(), |face| face.font_size.to_bits()),
        font_identity: atlas.glyph_font_identity_for_char(face, ch),
        x_bin: SubpixelBin::Zero,
        y_bin: SubpixelBin::Zero,
    }
}

impl WgpuRenderer {
    /// Native tooltip drawing uses only the supplied target's geometry.
    pub fn render_native_tooltip(
        &mut self,
        target: super::RenderTarget<'_>,
        tooltip: &crate::tooltip_layout::TooltipLayout,
        glyph_atlas: &mut WgpuGlyphAtlas,
    ) {
        let size = target.surface.logical_size();
        self.paint_tooltip(
            target.view,
            tooltip,
            glyph_atlas,
            [size.width(), size.height()],
            target.surface.device_scale().get(),
        );
    }

    fn paint_tooltip(
        &mut self,
        view: &wgpu::TextureView,
        tooltip: &crate::tooltip_layout::TooltipLayout,
        glyph_atlas: &mut WgpuGlyphAtlas,
        logical_size: [f32; 2],
        scale: f32,
    ) {
        let uniforms = Uniforms {
            screen_size: logical_size,
            time: 0.0,
            _padding: 0.0,
        };
        let draw = self.parameters(uniforms.screen_size, uniforms.time);

        let (tx, ty, tw, th) = tooltip.bounds;

        // Convert user-specified colors to linear space (surface is sRGB)
        let bg_color = Color::new(tooltip.bg.0, tooltip.bg.1, tooltip.bg.2, 1.0).srgb_to_linear();
        let border_color =
            Color::new(tooltip.border.0, tooltip.border.1, tooltip.border.2, 1.0).srgb_to_linear();
        let text_color = {
            let c = Color::new(tooltip.fg.0, tooltip.fg.1, tooltip.fg.2, 1.0).srgb_to_linear();
            [c.r, c.g, c.b, c.a]
        };

        // === Pass 1: Background and border rectangles ===
        let mut rect_vertices: Vec<RectVertex> = Vec::new();

        // Background
        self.add_rect(&mut rect_vertices, tx, ty, tw, th, &bg_color);

        // Border in native-surface logical pixels.
        let bw = tooltip.border_width;
        self.add_rect(&mut rect_vertices, tx, ty, tw, bw, &border_color); // top
        self.add_rect(&mut rect_vertices, tx, ty + th - bw, tw, bw, &border_color); // bottom
        self.add_rect(&mut rect_vertices, tx, ty, bw, th, &border_color); // left
        self.add_rect(&mut rect_vertices, tx + tw - bw, ty, bw, th, &border_color); // right

        let line_height = tooltip.line_height;
        for (row, line) in tooltip.lines.iter().enumerate() {
            for character in &line.characters {
                if let Some(run) = character.face.and_then(|index| tooltip.runs.get(index)) {
                    let x = tx + tooltip.padding + character.x;
                    let y = ty + tooltip.padding + row as f32 * line_height;
                    self.add_rect(
                        &mut rect_vertices,
                        x,
                        y,
                        character.advance,
                        line_height,
                        &run.face.background.srgb_to_linear(),
                    );
                    for rect in
                        super::content::stretch_decoration_rects(&run.face, x, y, character.advance)
                    {
                        self.add_rect(
                            &mut rect_vertices,
                            rect.x,
                            rect.y,
                            rect.width,
                            rect.height,
                            &rect.color.srgb_to_linear(),
                        );
                    }
                }
            }
        }

        if let Some(rect_buffer) =
            self.arenas
                .rect
                .upload(&self.device, &self.queue, &rect_vertices)
        {
            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Tooltip Rect Encoder"),
                });
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Tooltip Rect Pass"),
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
                pass.set_pipeline(&self.pipelines.rect);
                pass.set_bind_group(0, draw.binding(), &[]);
                pass.set_vertex_buffer(0, rect_buffer.buffer_slice());
                pass.draw(0..rect_vertices.len() as u32, 0..1);
            }
            self.queue.submit(Some(encoder.finish()));
        }

        // === Pass 2: Collect all text glyphs and render batched ===
        let padding = tooltip.padding;
        let line_height = tooltip.line_height;
        let mut overlay_glyphs: Vec<(GlyphAtlasHandle, f32, f32, [f32; 4])> = Vec::new();

        for (line_idx, line) in tooltip.lines.iter().enumerate() {
            let ly = ty + padding + line_idx as f32 * line_height;
            for character in &line.characters {
                let ch = character.value;
                let face = character
                    .face
                    .and_then(|index| tooltip.runs.get(index))
                    .map(|run| &run.face);
                let color = face.map_or(text_color, |face| {
                    let color = face.foreground.srgb_to_linear();
                    [color.r, color.g, color.b, color.a]
                });
                let key = glyph_key(glyph_atlas, ch, face);
                if let Some(handle) = glyph_atlas.get_or_create_atlas(
                    &self.device,
                    &self.queue,
                    &key,
                    face,
                    SubpixelRequest::Disabled,
                ) {
                    overlay_glyphs.push((
                        handle,
                        tx + padding + character.x,
                        ly + face.map_or(0.0, |face| {
                            face.font_ascent as f32 - glyph_atlas.default_font_ascent()
                        }),
                        color,
                    ));
                }
            }
        }

        self.render_overlay_glyphs_scaled(view, &mut overlay_glyphs, glyph_atlas, scale, &draw);
    }
}

impl crate::tooltip_layout::TooltipLayout {
    pub fn measure_with_atlas(
        request: &neomacs_display_protocol::tooltip::TooltipRequest,
        char_width: f32,
        line_height: f32,
        scale: f32,
        atlas: &mut WgpuGlyphAtlas,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Self {
        Self::measure_with(request, char_width, line_height, |ch, face| {
            let key = glyph_key(atlas, ch, face);
            atlas
                .get_or_create_atlas(device, queue, &key, face, SubpixelRequest::Disabled)
                .map_or(char_width, |handle| handle.advance_width / scale)
        })
    }
}
