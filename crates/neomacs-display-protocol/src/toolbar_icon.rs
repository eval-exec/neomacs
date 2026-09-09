//! Exact raster inputs for toolbar icons, shared by asset loading and drawing.

use std::num::NonZeroU32;

use crate::{
    AxisSize, Color, DeviceScale, ImageBackgroundPolicy, ImageColorContext, ImageRealization,
    ImageSizeSpec, ToolBarImageSource,
};

/// Appearance published by the toolbar face. Hover/pressed/disabled state is
/// composited at draw time, never baked into the decoded icon.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ToolBarIconStyle {
    size: NonZeroU32,
    colors: ImageColorContext,
}

impl ToolBarIconStyle {
    /// Chrome face colors are opaque sRGB, unlike linear scene colors.
    pub fn from_chrome_face(size: u32, foreground: Color, background: Color) -> Self {
        fn pixel(color: Color) -> u32 {
            let channel = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u32;
            (channel(color.r) << 16) | (channel(color.g) << 8) | channel(color.b)
        }
        Self {
            size: NonZeroU32::new(size).unwrap_or(NonZeroU32::MIN),
            colors: ImageColorContext::from_pixels(pixel(foreground), pixel(background))
                .with_background_policy(ImageBackgroundPolicy::Transparent),
        }
    }

    pub fn realize(self, source: ToolBarImageSource, scale: DeviceScale) -> ToolBarIconKey {
        ToolBarIconKey {
            source,
            style: self,
            realization: ImageRealization::with_device_scale(1.0, scale.get()),
        }
    }
}

/// A texture lookup cannot omit face colors, logical size, or device scale.
/// Font changes matter through the resulting logical icon size. Source changes
/// (including light/dark theme asset selection) are also distinct realizations.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ToolBarIconKey {
    source: ToolBarImageSource,
    style: ToolBarIconStyle,
    realization: ImageRealization,
}

impl ToolBarIconKey {
    pub fn source(&self) -> &ToolBarImageSource {
        &self.source
    }
    pub fn colors(&self) -> ImageColorContext {
        self.style.colors
    }
    pub fn size_spec(&self) -> ImageSizeSpec {
        let limit = AxisSize::AtMost(self.style.size.get());
        ImageSizeSpec::new(limit, limit)
    }
    pub fn realization(&self) -> ImageRealization {
        self.realization
    }
}

#[cfg(test)]
#[path = "toolbar_icon_test.rs"]
mod tests;
