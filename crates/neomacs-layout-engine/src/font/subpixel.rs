//! Display subpixel geometry independent of font discovery.

/// Physical color-component order used by LCD mask rasterization.
///
/// The historical name is retained for protocol compatibility, but the type
/// belongs to the display surface and is not a Fontconfig result on macOS or
/// Windows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontconfigSubpixelOrder {
    Unknown,
    None,
    Rgb,
    Bgr,
    VRgb,
    VBgr,
}

impl FontconfigSubpixelOrder {
    pub fn allows_horizontal_subpixel(self) -> bool {
        matches!(self, Self::Rgb | Self::Bgr | Self::Unknown)
    }
}

pub fn default_subpixel_order() -> FontconfigSubpixelOrder {
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        crate::font::fontconfig::default_subpixel_order()
    }
    #[cfg(not(all(unix, not(target_os = "macos"))))]
    {
        FontconfigSubpixelOrder::Unknown
    }
}
