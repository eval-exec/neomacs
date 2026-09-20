//! Display-wide extents from the frontend's physical monitor snapshot.
//!
//! GNU NS exposes NSScreen coordinates (points); X exposes device pixels.
//! Keep that choice explicit, and never substitute terminal cell dimensions
//! when the graphical monitor snapshot is unavailable.

use crate::emacs_core::builtins::NeomacsMonitorInfo;

#[derive(Clone, Copy)]
enum DisplayPixelConvention {
    DevicePixels,
    MacPoints,
}

impl DisplayPixelConvention {
    fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::MacPoints
        } else {
            Self::DevicePixels
        }
    }

    fn divisor(self, monitor: &NeomacsMonitorInfo) -> Result<f64, DesktopUnavailable> {
        match self {
            Self::DevicePixels => Ok(1.0),
            Self::MacPoints if monitor.scale.is_finite() && monitor.scale > 0.0 => {
                Ok(monitor.scale)
            }
            Self::MacPoints => Err(DesktopUnavailable),
        }
    }
}

pub(super) struct DesktopExtent {
    pub width: i64,
    pub height: i64,
}

#[derive(Debug)]
pub(super) struct DesktopUnavailable;

impl DesktopExtent {
    pub fn from_monitors(monitors: &[NeomacsMonitorInfo]) -> Result<Self, DesktopUnavailable> {
        let convention = DisplayPixelConvention::current();
        let mut bounds: Option<(f64, f64, f64, f64)> = None;
        for monitor in monitors {
            if monitor.width <= 0 || monitor.height <= 0 {
                return Err(DesktopUnavailable);
            }
            let divisor = convention.divisor(monitor)?;
            let left = f64::from(monitor.x) / divisor;
            let top = f64::from(monitor.y) / divisor;
            let right = left + f64::from(monitor.width) / divisor;
            let bottom = top + f64::from(monitor.height) / divisor;
            bounds = Some(match bounds {
                None => (left, top, right, bottom),
                Some((l, t, r, b)) => (l.min(left), t.min(top), r.max(right), b.max(bottom)),
            });
        }
        let (left, top, right, bottom) = bounds.ok_or(DesktopUnavailable)?;
        Ok(Self {
            width: (right - left).round() as i64,
            height: (bottom - top).round() as i64,
        })
    }
}
