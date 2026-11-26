//! Target-surface font policy, separate from the opened font's metrics.

#[cfg(target_os = "linux")]
use neomacs_font_materializer::fontconfig::SurfaceFontOptions;
use neomacs_renderer_wgpu::WgpuGlyphAtlas;

/// Owned backend-specific rendering preferences. Wayland consumes PGTK's
/// desktop options independently of X11 resources, even when DISPLAY exists.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum DisplayRasterOptions {
    #[cfg(target_os = "linux")]
    X11(SurfaceFontOptions),
    #[cfg(target_os = "linux")]
    Wayland(SurfaceFontOptions),
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
            Self::X11(options) | Self::Wayland(options) => options,
            Self::Other => SurfaceFontOptions::default(),
        });
        #[cfg(not(target_os = "linux"))]
        let _ = atlas;
    }

    pub(crate) fn with_desktop_options(
        self,
        options: neomacs_display_protocol::DesktopFontRenderOptions,
    ) -> Self {
        #[cfg(target_os = "linux")]
        if matches!(self, Self::Wayland(_)) {
            return Self::Wayland(SurfaceFontOptions::from_desktop_options(options));
        }
        let _ = options;
        self
    }
}
