//! Platform display rules for converting Emacs face heights to layout units.
//!
//! Font catalogs answer which font exists. They do not own frame DPI or the
//! conversion between GNU printer points, logical coordinates, and device
//! pixels. Keeping this module independent prevents X11 policy from leaking
//! into the CoreText and DirectWrite catalogs.

use neomacs_display_protocol::{DeviceScale, DisplayObservation, Dpi, XServerKind};
use neovm_core::emacs_core::display_host::FrameFontSize;
use neovm_core::face::{Face, FaceHeight};
use std::sync::OnceLock;

pub use super::frame_metrics::GraphicFontSizePx;

#[cfg(target_os = "linux")]
use std::ffi::{CStr, CString};
#[cfg(target_os = "linux")]
use std::ptr;
#[cfg(target_os = "linux")]
use x11_dl::xlib;

/// GNU uses the printer's point rather than the desktop-publishing 72 DPI
/// point for its `POINT_TO_PIXEL` conversion (`src/font.h`).
pub const GNU_POINTS_PER_INCH: f64 = 72.27;

/// The logical-coordinate rule selected by the active display frontend.
///
/// Device/backing scale is deliberately absent: it is applied later when a
/// realized logical size becomes a raster request.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum LogicalFontScale {
    /// GNU NS sets frame resolution to 72.27 so Emacs points map to Cocoa
    /// logical units before Retina backing scale is applied.
    GnuCocoaPoint,
    /// DirectWrite sizes are device-independent pixels at 96 logical DPI.
    WindowsDip,
    /// Neomacs's Wayland logical-coordinate policy, independently named so it
    /// cannot be confused with DirectWrite merely because both currently use
    /// 96 logical DPI.
    WaylandLogical,
    /// X11 uses the frame/display's effective Xft DPI.
    X11 { effective_dpi: f32 },
    /// Explicit frontend/test value.
    ExplicitDpi(f32),
}

impl LogicalFontScale {
    fn layout_dpi(self) -> f32 {
        let dpi = match self {
            Self::GnuCocoaPoint => GNU_POINTS_PER_INCH as f32,
            Self::WindowsDip | Self::WaylandLogical => 96.0,
            Self::X11 { effective_dpi } | Self::ExplicitDpi(effective_dpi) => effective_dpi,
        };
        if dpi.is_finite() && dpi > 0.0 {
            dpi
        } else {
            96.0
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FontSizing {
    scale: LogicalFontScale,
}

/// Policy for converting native display observations into frame scale.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum ScalePolicy {
    /// Preserve GNU X11 behavior except for reliably identified Xwayland,
    /// whose physical-size report is not a stable logical-DPI authority.
    Automatic,
    /// Follow GNU's X11 resource/geometry fallback for every X server.
    StrictGnu,
    /// Ignore display-reported font DPI while retaining its device scale.
    Explicit(Dpi),
}

/// Provenance of the logical font DPI in a resolved frame profile.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum FrameScaleSource {
    ExplicitPolicy,
    XftResource,
    X11Geometry,
    GnuX11Fallback,
    XwaylandLogicalFallback,
    PlatformLogical,
}

/// One atomic answer for logical font sizing and logical-to-device scale.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrameScaleProfile {
    font_sizing: FontSizing,
    device_scale: DeviceScale,
    source: FrameScaleSource,
}

impl FrameScaleProfile {
    #[must_use]
    pub const fn font_sizing(self) -> FontSizing {
        self.font_sizing
    }

    #[must_use]
    pub const fn device_scale(self) -> DeviceScale {
        self.device_scale
    }

    #[must_use]
    pub const fn source(self) -> FrameScaleSource {
        self.source
    }
}

/// Resolve native display facts without performing platform I/O.
///
/// An explicit Xft resource remains authoritative, matching GNU Emacs. In
/// automatic mode only a positively identified Xwayland server receives the
/// 96-DPI logical fallback; native and unknown X servers retain GNU's raw
/// geometry calculation, including unusual values used by remote displays.
#[must_use]
pub fn resolve_frame_scale(
    observation: DisplayObservation,
    policy: ScalePolicy,
) -> FrameScaleProfile {
    let device_scale = match observation {
        DisplayObservation::X11(observation) => observation.device_scale(),
        DisplayObservation::Wayland { device_scale }
        | DisplayObservation::Cocoa { device_scale }
        | DisplayObservation::Windows { device_scale } => device_scale,
        _ => DeviceScale::ONE,
    };

    if let ScalePolicy::Explicit(dpi) = policy {
        return FrameScaleProfile {
            font_sizing: FontSizing::for_layout_dpi(dpi.get()),
            device_scale,
            source: FrameScaleSource::ExplicitPolicy,
        };
    }

    match observation {
        DisplayObservation::X11(observation) => {
            let (dpi, source) = if let Some(dpi) = observation.xft_dpi() {
                (dpi.get(), FrameScaleSource::XftResource)
            } else if matches!(policy, ScalePolicy::Automatic)
                && matches!(observation.server(), XServerKind::Xwayland)
            {
                (96.0, FrameScaleSource::XwaylandLogicalFallback)
            } else if let Some(geometry) = observation.geometry() {
                (
                    geometry.height_px() as f32 * 25.4 / geometry.height_mm() as f32,
                    FrameScaleSource::X11Geometry,
                )
            } else {
                (100.0, FrameScaleSource::GnuX11Fallback)
            };
            FrameScaleProfile {
                font_sizing: FontSizing::for_layout_dpi(dpi),
                device_scale,
                source,
            }
        }
        DisplayObservation::Wayland { .. } => FrameScaleProfile {
            font_sizing: FontSizing::wayland(),
            device_scale,
            source: FrameScaleSource::PlatformLogical,
        },
        DisplayObservation::Cocoa { .. } => FrameScaleProfile {
            font_sizing: FontSizing::gnu_cocoa(),
            device_scale,
            source: FrameScaleSource::PlatformLogical,
        },
        DisplayObservation::Windows { .. } => FrameScaleProfile {
            font_sizing: FontSizing::windows_dip(),
            device_scale,
            source: FrameScaleSource::PlatformLogical,
        },
        _ => FrameScaleProfile {
            font_sizing: FontSizing::logical(),
            device_scale,
            source: FrameScaleSource::PlatformLogical,
        },
    }
}

impl FontSizing {
    pub const fn new(scale: LogicalFontScale) -> Self {
        Self { scale }
    }

    /// Compatibility constructor for X11 call sites. New GUI code should
    /// select a frontend-specific rule through [`Self::native_gui`].
    pub fn xft() -> Self {
        Self::new(LogicalFontScale::X11 {
            effective_dpi: xft_dpi(),
        })
    }

    /// Compatibility name for the existing 96-DPI logical rule.
    pub const fn logical() -> Self {
        Self::new(LogicalFontScale::WaylandLogical)
    }

    pub const fn gnu_cocoa() -> Self {
        Self::new(LogicalFontScale::GnuCocoaPoint)
    }

    pub const fn windows_dip() -> Self {
        Self::new(LogicalFontScale::WindowsDip)
    }

    pub const fn wayland() -> Self {
        Self::new(LogicalFontScale::WaylandLogical)
    }

    pub const fn for_layout_dpi(layout_dpi: f32) -> Self {
        Self::new(LogicalFontScale::ExplicitDpi(layout_dpi))
    }

    pub fn native_gui() -> Self {
        std::cfg_select! {
            target_os = "macos" => Self::gnu_cocoa(),
            windows => Self::windows_dip(),
            target_os = "linux" => Self::xft(),
            _ => Self::logical(),
        }
    }

    pub fn layout_dpi(self) -> f32 {
        self.scale.layout_dpi()
    }

    pub fn face_height_to_layout_pixels(self, tenths: i32) -> f32 {
        points_to_layout_pixels(tenths as f32 / 10.0, self.layout_dpi())
    }

    pub fn font_size_px_for_face(self, face: &Face) -> f32 {
        let default_font_size = self.face_height_to_layout_pixels(100);
        match &face.height {
            Some(FaceHeight::Absolute(tenths)) => self.face_height_to_layout_pixels(*tenths),
            Some(FaceHeight::Relative(scale)) => default_font_size * (*scale as f32),
            None => default_font_size,
        }
    }

    /// Convert a typed frame-font request to the logical pixel size consumed
    /// by the native font selector. Pixel requests deliberately bypass DPI;
    /// point and relative requests use this frame's frontend policy.
    pub fn font_size_px_for_request(self, request: FrameFontSize) -> Option<GraphicFontSizePx> {
        let pixels = match request {
            FrameFontSize::Default => self.face_height_to_layout_pixels(100),
            FrameFontSize::Pixels(pixels) => pixels.get() as f32,
            FrameFontSize::Points(points) => {
                points_to_layout_pixels(points.get() as f32, self.layout_dpi())
            }
            FrameFontSize::Relative(scale) => {
                self.face_height_to_layout_pixels(100) * scale.get() as f32
            }
        };
        GraphicFontSizePx::new(pixels)
    }

    /// GNU `PIXEL_TO_POINT(pixel_size * 10, FRAME_RES(frame))` for one
    /// realized logical pixel size. This is the inverse representation stored
    /// in a Lisp face's absolute `:height` slot.
    pub fn face_height_tenths_for_layout_pixels(self, pixels: u32) -> i32 {
        let tenths = f64::from(pixels) * 10.0 * GNU_POINTS_PER_INCH / f64::from(self.layout_dpi());
        tenths.round().clamp(1.0, f64::from(i32::MAX)) as i32
    }
}

pub fn points_to_layout_pixels(points: f32, dpi: f32) -> f32 {
    (f64::from(points) * f64::from(dpi) / GNU_POINTS_PER_INCH).round() as f32
}

/// Compatibility helper for GNU X11 callers.
pub fn points_to_pixels(points: f32) -> f32 {
    points_to_layout_pixels(points, xft_dpi())
}

/// Compatibility helper for a face height in tenths of a point.
pub fn face_height_to_pixels(tenths: i32) -> f32 {
    points_to_pixels(tenths as f32 / 10.0)
}

static XFT_DPI: OnceLock<f32> = OnceLock::new();
static X_DPI_PROBE_DISABLED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

pub fn disable_x_dpi_probe() {
    X_DPI_PROBE_DISABLED.store(true, std::sync::atomic::Ordering::Relaxed);
}

pub fn xft_dpi() -> f32 {
    *XFT_DPI.get_or_init(|| {
        let dpi = query_xft_dpi().unwrap_or(100.0);
        tracing::info!("Xft.dpi: {}", dpi);
        dpi
    })
}

#[cfg(target_os = "linux")]
fn query_xft_dpi() -> Option<f32> {
    if X_DPI_PROBE_DISABLED.load(std::sync::atomic::Ordering::Relaxed)
        || std::env::var("DISPLAY").unwrap_or_default().is_empty()
    {
        return None;
    }

    let (tx, rx) = std::sync::mpsc::channel();
    let _handle = std::thread::Builder::new()
        .name("xft-dpi-probe".into())
        .spawn(move || {
            let result = query_xft_dpi_inner();
            let _ = tx.send(result);
        });
    match rx.recv_timeout(std::time::Duration::from_millis(100)) {
        Ok(result) => result,
        Err(_) => {
            tracing::warn!(
                "query_xft_dpi: X11 connection timed out (broken display?), using fallback DPI"
            );
            None
        }
    }
}

#[cfg(target_os = "linux")]
fn query_xft_dpi_inner() -> Option<f32> {
    let xlib = xlib::Xlib::open().ok()?;
    let display = unsafe { (xlib.XOpenDisplay)(ptr::null()) };
    if display.is_null() {
        return None;
    }

    let class = CString::new("Xft").ok()?;
    let name = CString::new("dpi").ok()?;
    let dpi = unsafe {
        let resource = (xlib.XGetDefault)(display, class.as_ptr(), name.as_ptr());
        let parsed = if resource.is_null() {
            None
        } else {
            CStr::from_ptr(resource)
                .to_str()
                .ok()
                .and_then(|value| value.trim().parse::<f32>().ok())
        };
        match parsed {
            Some(dpi) if dpi.is_finite() && dpi > 0.0 => Some(dpi),
            _ => {
                let screen = (xlib.XDefaultScreen)(display);
                let pixels = (xlib.XDisplayHeight)(display, screen);
                let mm = (xlib.XDisplayHeightMM)(display, screen);
                Some(fallback_frame_res_y(pixels, mm))
            }
        }
    };
    unsafe { (xlib.XCloseDisplay)(display) };
    dpi
}

#[cfg(not(target_os = "linux"))]
fn query_xft_dpi() -> Option<f32> {
    None
}

pub(crate) fn fallback_frame_res_y(display_height_px: i32, display_height_mm: i32) -> f32 {
    if display_height_mm < 1 {
        100.0
    } else {
        display_height_px as f32 * 25.4 / display_height_mm as f32
    }
}

#[cfg(test)]
#[path = "sizing_test.rs"]
mod tests;
