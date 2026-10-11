//! Winit window icon utilities.
//!
//! Uses the project SVG icon (`assets/window-icon.svg`) and rasterizes it at
//! runtime for platform window APIs that require RGBA pixel buffers.

use winit::icon::Icon;
use winit::window::Window;

const WINDOW_ICON_SVG: &[u8] = include_bytes!("../assets/window-icon.svg");
const WINDOW_ICON_SIZE: u32 = 256;

pub(crate) struct RasterizedWindowIcon {
    width: u32,
    height: u32,
    /// Premultiplied RGBA, matching tiny-skia's native output.
    premultiplied_rgba: Vec<u8>,
}

impl RasterizedWindowIcon {
    pub(crate) const fn width(&self) -> u32 {
        self.width
    }

    pub(crate) const fn height(&self) -> u32 {
        self.height
    }

    fn to_winit_icon(&self) -> Option<Icon> {
        // Winit's cross-platform icon API expects straight-alpha RGBA.
        let mut rgba = self.premultiplied_rgba.clone();
        for px in rgba.chunks_exact_mut(4) {
            let a = px[3] as f32 / 255.0;
            if a > 0.0 && a < 1.0 {
                px[0] = (px[0] as f32 / a).min(255.0) as u8;
                px[1] = (px[1] as f32 / a).min(255.0) as u8;
                px[2] = (px[2] as f32 / a).min(255.0) as u8;
            }
        }
        winit::icon::RgbaIcon::new(rgba, self.width, self.height)
            .ok()
            .map(Into::into)
    }

    #[cfg(target_os = "linux")]
    pub(crate) fn to_wayland_argb8888(&self) -> Vec<u8> {
        self.premultiplied_rgba
            .chunks_exact(4)
            .map(|pixel| {
                let [red, green, blue, alpha] = pixel else {
                    unreachable!("chunks_exact always yields four bytes")
                };
                u32::from(*alpha) << 24
                    | u32::from(*red) << 16
                    | u32::from(*green) << 8
                    | u32::from(*blue)
            })
            .flat_map(u32::to_ne_bytes)
            .collect()
    }
}

fn decode_svg_icon(data: &[u8], size: u32) -> Option<RasterizedWindowIcon> {
    let opts = resvg::usvg::Options::default();
    let tree = resvg::usvg::Tree::from_data(data, &opts).ok()?;
    let svg_size = tree.size();
    let svg_w = svg_size.width();
    let svg_h = svg_size.height();
    if svg_w <= 0.0 || svg_h <= 0.0 || size == 0 {
        return None;
    }

    let mut pixmap = resvg::tiny_skia::Pixmap::new(size, size)?;
    let scale_x = size as f32 / svg_w;
    let scale_y = size as f32 / svg_h;
    let transform = resvg::tiny_skia::Transform::from_scale(scale_x, scale_y);
    resvg::render(&tree, transform, &mut pixmap.as_mut());

    Some(RasterizedWindowIcon {
        width: size,
        height: size,
        premultiplied_rgba: pixmap.take(),
    })
}

pub(crate) fn load_window_icon() -> Option<RasterizedWindowIcon> {
    decode_svg_icon(WINDOW_ICON_SVG, WINDOW_ICON_SIZE)
}

/// Owns decoded icon pixels and any display-scoped native icon protocol.
///
/// Keeping this beside the render application's display lifetime avoids
/// reconnecting for child frames and makes teardown order explicit.
pub(crate) struct WindowIconService {
    icon: Option<RasterizedWindowIcon>,
    #[cfg(target_os = "linux")]
    wayland: crate::wayland_toplevel_icon::WaylandToplevelIconService,
}

impl WindowIconService {
    pub(crate) fn new() -> Self {
        let icon = load_window_icon();
        if icon.is_none() {
            tracing::warn!("Failed to decode window icon SVG");
        }
        Self {
            icon,
            #[cfg(target_os = "linux")]
            wayland: crate::wayland_toplevel_icon::WaylandToplevelIconService::new(),
        }
    }

    /// Apply the Neomacs window icon to a winit window.
    pub(crate) fn apply(&mut self, window: &dyn Window) {
        let Some(icon) = self.icon.as_ref() else {
            return;
        };

        if let Some(winit_icon) = icon.to_winit_icon() {
            window.set_window_icon(Some(winit_icon));
        }

        #[cfg(target_os = "linux")]
        if let Err(error) = self.wayland.apply(window, icon) {
            tracing::warn!(%error, "failed to set native Wayland toplevel icon");
        }
    }

    pub(crate) fn shutdown(&mut self) {
        #[cfg(target_os = "linux")]
        self.wayland.shutdown();
    }
}

#[cfg(all(test, target_os = "linux"))]
#[path = "tests/window_icon_test.rs"]
mod tests;
