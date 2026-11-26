//! Owned desktop font-opening preferences. DEFAULT retains the font pattern's
//! policy; no native settings or font handles cross this boundary.

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum DesktopFontAntialias {
    #[default]
    Default,
    None,
    Grayscale,
    Subpixel,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum DesktopFontHinting {
    #[default]
    Default,
    None,
    Slight,
    Medium,
    Full,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum DesktopFontSubpixelOrder {
    #[default]
    Default,
    Rgb,
    Bgr,
    VRgb,
    VBgr,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DesktopFontRenderOptions {
    pub antialias: DesktopFontAntialias,
    pub hinting: DesktopFontHinting,
    pub subpixel_order: DesktopFontSubpixelOrder,
}
