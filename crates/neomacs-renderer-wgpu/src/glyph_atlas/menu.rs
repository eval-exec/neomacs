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

impl WgpuGlyphAtlas {
    /// Replay a layout-resolved heading without selecting fonts or measuring text.
    pub fn menu_label_atlas(
        &mut self,
        label: &neomacs_display_protocol::frame_chrome::ResolvedMenuLabel,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Result<Vec<super::GlyphAtlasHandle>, super::GlyphAtlasError> {
        use super::*;
        self.install_resolved_fonts(label.fonts().iter());
        let key = ComposedGlyphKey {
            text: "".into(),
            face_id: FaceId::new(0),
            font_size_bits: label.font_size().to_bits(),
            font_identity: 0,
            glyph_stream_identity: Some(resolved_glyph_stream_identity(label.glyphs())),
            x_bin: SubpixelBin::Zero,
            y_bin: SubpixelBin::Zero,
        };
        let cache_key = CachedComposedGlyphKey {
            glyph: key.identity(),
            mode: self.render_mode_from_request(SubpixelRequest::Disabled),
        };
        if let Some(cached) = self.atlas_composed_cache.get_mut(&cache_key) {
            cached.last_accessed = self.generation;
            let handles = cached.handles.clone();
            for handle in &handles {
                self.pin_entry_page(handle.entry)?;
            }
            return Ok(handles);
        }
        if label.glyphs().is_empty() {
            return Ok(Vec::new());
        }
        let results = self
            .rasterize_resolved_cluster(
                label.glyphs(),
                label.font_size(),
                SubpixelBin::Zero,
                SubpixelBin::Zero,
                false,
            )
            .ok_or(GlyphAtlasError::RasterizeFailed)?;
        let mut handles = Vec::new();
        for result in results {
            if result.width == 0 || result.height == 0 {
                continue;
            }
            let entry = self.rasterize_result_to_atlas_entry(device, queue, &result)?;
            self.pin_entry_page(entry)?;
            handles.push(GlyphAtlasHandle {
                entry,
                advance_width: result.advance_width,
            });
        }
        self.atlas_composed_cache.insert(
            cache_key,
            CachedComposedGlyph {
                handles: handles.clone(),
                last_accessed: self.generation,
            },
        );
        Ok(handles)
    }
}
