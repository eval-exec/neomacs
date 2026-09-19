//! Native menu text measurement through the same font/raster source as paint.
use super::{GlyphKey, SubpixelRequest, WgpuGlyphAtlas, glyph_font_identity};
use cosmic_text::SubpixelBin;
use neomacs_display_protocol::FaceId;

impl WgpuGlyphAtlas {
    /// Freeze menu text advances before panel allocation. The painter uses
    /// these positions even when raster-cache contents change on a redraw.
    pub fn measure_menu(
        &mut self,
        items: Vec<neomacs_display_protocol::PopupMenuItem>,
        title: Option<String>,
        space_advance: f32,
        face: Option<&neomacs_display_protocol::face::Face>,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> neomacs_display_protocol::menu::MeasuredMenu {
        neomacs_display_protocol::menu::MeasuredMenu::measure(items, title, space_advance, |ch| {
            let key = GlyphKey {
                charcode: ch as u32,
                face_id: FaceId::new(0),
                font_size_bits: 0.0_f32.to_bits(),
                font_identity: glyph_font_identity(face),
                x_bin: SubpixelBin::Zero,
                y_bin: SubpixelBin::Zero,
            };
            self.get_or_create_atlas(device, queue, &key, face, SubpixelRequest::Disabled)
                .map_or(space_advance, |glyph| {
                    glyph.advance_width / self.scale_factor
                })
        })
    }
}
