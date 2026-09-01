//! Validated facts reported by a native display frontend for frame scaling.
//!
//! This module deliberately records observations, not policy conclusions.
//! In particular, Xwayland is a server identity; whether its physical-size
//! report is useful belongs to the layout policy that consumes the report.

use std::num::NonZeroU32;

use crate::DeviceScale;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InvalidDpi;

/// Finite, positive dots per inch.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Dpi(f32);

impl Dpi {
    pub fn new(value: f32) -> Result<Self, InvalidDpi> {
        if value.is_finite() && value > 0.0 {
            Ok(Self(value))
        } else {
            Err(InvalidDpi)
        }
    }

    #[must_use]
    pub const fn get(self) -> f32 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InvalidDisplayGeometry;

/// One display's vertical pixel and physical extents.
///
/// GNU Emacs derives its fallback X11 DPI from these two values. Zero or
/// missing millimetres are represented by the absence of this type, so every
/// value that reaches the resolver is safe to divide.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DisplayGeometry {
    height_px: NonZeroU32,
    height_mm: NonZeroU32,
}

impl DisplayGeometry {
    pub fn new(height_px: u32, height_mm: u32) -> Result<Self, InvalidDisplayGeometry> {
        let height_px = NonZeroU32::new(height_px).ok_or(InvalidDisplayGeometry)?;
        let height_mm = NonZeroU32::new(height_mm).ok_or(InvalidDisplayGeometry)?;
        Ok(Self {
            height_px,
            height_mm,
        })
    }

    #[must_use]
    pub const fn height_px(self) -> u32 {
        self.height_px.get()
    }

    #[must_use]
    pub const fn height_mm(self) -> u32 {
        self.height_mm.get()
    }
}

/// Identity of the X server, observed independently of its DPI values.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum XServerKind {
    Xorg,
    Xwayland,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct X11DisplayObservation {
    server: XServerKind,
    xft_dpi: Option<Dpi>,
    geometry: Option<DisplayGeometry>,
    device_scale: DeviceScale,
}

impl X11DisplayObservation {
    #[must_use]
    pub const fn new(
        server: XServerKind,
        xft_dpi: Option<Dpi>,
        geometry: Option<DisplayGeometry>,
        device_scale: DeviceScale,
    ) -> Self {
        Self {
            server,
            xft_dpi,
            geometry,
            device_scale,
        }
    }

    #[must_use]
    pub const fn server(self) -> XServerKind {
        self.server
    }

    #[must_use]
    pub const fn xft_dpi(self) -> Option<Dpi> {
        self.xft_dpi
    }

    #[must_use]
    pub const fn geometry(self) -> Option<DisplayGeometry> {
        self.geometry
    }

    #[must_use]
    pub const fn device_scale(self) -> DeviceScale {
        self.device_scale
    }
}

/// Backend facts used to resolve one frame's font and device scale.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum DisplayObservation {
    X11(X11DisplayObservation),
    Wayland { device_scale: DeviceScale },
    Cocoa { device_scale: DeviceScale },
    Windows { device_scale: DeviceScale },
}
