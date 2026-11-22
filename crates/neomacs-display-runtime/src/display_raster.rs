//! Target-surface font policy, separate from the opened font's metrics.

#[cfg(target_os = "linux")]
use neomacs_font_materializer::fontconfig::SurfaceFontOptions;
use neomacs_renderer_wgpu::WgpuGlyphAtlas;

/// One owned display-lifetime snapshot. Wayland intentionally retains pattern
/// policy: GNU PGTK/GSettings rendering precedence is a separate, UNATTESTED
/// follow-up and must not inherit X11 resources merely because DISPLAY exists.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum DisplayRasterOptions {
    #[cfg(target_os = "linux")]
    X11(SurfaceFontOptions),
    #[cfg(target_os = "linux")]
    Wayland,
    #[default]
    Other,
}

impl DisplayRasterOptions {
    /// All native atlas lifetimes (including device recreation and popups)
    /// apply the same snapshot before measuring or rasterizing any glyph.
    pub(crate) fn glyph_atlas(self, device: &wgpu::Device, scale: f32) -> WgpuGlyphAtlas {
        let mut atlas = WgpuGlyphAtlas::new_with_scale(device, scale);
        self.apply(&mut atlas);
        atlas
    }

    pub(crate) fn apply(self, atlas: &mut WgpuGlyphAtlas) {
        #[cfg(target_os = "linux")]
        atlas.set_font_surface_options(match self {
            Self::X11(options) => options,
            Self::Wayland | Self::Other => SurfaceFontOptions::default(),
        });
        #[cfg(not(target_os = "linux"))]
        let _ = atlas;
    }
}
