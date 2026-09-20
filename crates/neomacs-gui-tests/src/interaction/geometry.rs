use super::{DriverError, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct DesktopPoint {
    pub x: f64,
    pub y: f64,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct DesktopRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}
impl DesktopRect {
    pub fn contains(self, p: DesktopPoint) -> bool {
        p.x >= self.x && p.y >= self.y && p.x < self.x + self.width && p.y < self.y + self.height
    }
}

/// Explicit mapping from global desktop points (possibly negative) to pixels
/// in one capture. Never assume a global Retina scale or a primary-screen origin.
#[derive(Clone, Copy, Debug)]
pub struct CaptureMapping {
    bounds: DesktopRect,
    width: u32,
    height: u32,
}
impl CaptureMapping {
    pub fn new(bounds: DesktopRect, width: u32, height: u32) -> Result<Self> {
        if ![bounds.x, bounds.y, bounds.width, bounds.height]
            .iter()
            .all(|v| v.is_finite())
            || bounds.width <= 0.0
            || bounds.height <= 0.0
            || width == 0
            || height == 0
        {
            return Err(DriverError::InvalidGeometry);
        }
        Ok(Self {
            bounds,
            width,
            height,
        })
    }
    pub fn pixel(self, point: DesktopPoint) -> Result<(u32, u32)> {
        if !self.bounds.contains(point) {
            return Err(DriverError::InvalidGeometry);
        }
        Ok((
            ((point.x - self.bounds.x) * self.width as f64 / self.bounds.width).floor() as u32,
            ((point.y - self.bounds.y) * self.height as f64 / self.bounds.height).floor() as u32,
        ))
    }
}
