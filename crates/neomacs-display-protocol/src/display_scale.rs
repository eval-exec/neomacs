//! Validated facts reported by a native display frontend for frame scaling.
//!
//! This module deliberately records observations, not policy conclusions.
//! In particular, Xwayland is a server identity; whether its physical-size
//! report is useful belongs to the layout policy that consumes the report.

use std::num::NonZeroU32;

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
pub struct InvalidDisplayHeightGeometry;

/// One display's vertical pixel and physical extents.
///
/// GNU Emacs derives its fallback X11 DPI from these two values. Zero or
/// missing millimetres are represented by the absence of this type, so every
/// value that reaches the resolver is safe to divide.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DisplayHeightGeometry {
    height_px: NonZeroU32,
    height_mm: NonZeroU32,
}

impl DisplayHeightGeometry {
    pub fn new(height_px: u32, height_mm: u32) -> Result<Self, InvalidDisplayHeightGeometry> {
        let height_px = NonZeroU32::new(height_px).ok_or(InvalidDisplayHeightGeometry)?;
        let height_mm = NonZeroU32::new(height_mm).ok_or(InvalidDisplayHeightGeometry)?;
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
    geometry: Option<DisplayHeightGeometry>,
}

impl X11DisplayObservation {
    #[must_use]
    pub const fn new(
        server: XServerKind,
        xft_dpi: Option<Dpi>,
        geometry: Option<DisplayHeightGeometry>,
    ) -> Self {
        Self {
            server,
            xft_dpi,
            geometry,
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
    pub const fn geometry(self) -> Option<DisplayHeightGeometry> {
        self.geometry
    }
}

/// Backend facts available before a native window and its device scale exist.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum DisplayObservation {
    X11(X11DisplayObservation),
    Wayland,
    Cocoa,
    Windows,
}
