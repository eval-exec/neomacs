//! Display-host protocol vocabulary owned by the evaluator layer
//! (stage-4 W2 E5, S1 state-down).
//!
//! These are validated boundary values that cross the evaluator/display-host
//! seam. They name only value/edit vocabulary (or this crate's own face data),
//! so the evaluator can hold and return them without naming a display-crate
//! type. The host traits that consume them stay display-side until their own
//! batch.

use std::fmt::{Display, Formatter};
use std::num::{NonZeroU16, NonZeroU32};

use neovm_edit::buffer::BufferId;

use super::data::face::FaceHeight;

pub mod image_catalog;

/// The display object that owns a neo-term instance.
///
/// Window terminals carry their buffer identity in the variant itself, so a
/// detached frame-wide `Window` state cannot be constructed accidentally.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalDisplayTarget {
    Window { buffer: BufferId },
    Inline,
    Floating,
}

/// A live terminal identifier. Zero is reserved as the Lisp failure sentinel,
/// so it cannot cross the typed display-host boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TerminalId(NonZeroU32);

impl TerminalId {
    pub fn new(id: u32) -> Option<Self> {
        NonZeroU32::new(id).map(Self)
    }

    pub fn get(self) -> u32 {
        self.0.get()
    }
}

impl Display for TerminalId {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

/// Non-empty terminal grid dimensions validated at the Lisp boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerminalGridSize {
    pub cols: NonZeroU16,
    pub rows: NonZeroU16,
}

impl TerminalGridSize {
    pub fn new(cols: u16, rows: u16) -> Option<Self> {
        Some(Self {
            cols: NonZeroU16::new(cols)?,
            rows: NonZeroU16::new(rows)?,
        })
    }
}

/// Validated terminal creation request owned by the evaluator/display-host
/// boundary. Renderer-specific command enums stay outside `neovm-core`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerminalCreateRequest {
    pub size: TerminalGridSize,
    pub target: TerminalDisplayTarget,
    pub shell: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TerminalFloatPlacement {
    x: f32,
    y: f32,
    opacity: f32,
}

impl TerminalFloatPlacement {
    pub fn new(x: f32, y: f32, opacity: f32) -> Option<Self> {
        (x.is_finite() && y.is_finite() && opacity.is_finite() && (0.0..=1.0).contains(&opacity))
            .then_some(Self { x, y, opacity })
    }

    pub fn x(self) -> f32 {
        self.x
    }

    pub fn y(self) -> f32 {
        self.y
    }

    pub fn opacity(self) -> f32 {
        self.opacity
    }
}

/// A finite, positive scalar used by point-size and relative-size requests.
///
/// Lisp numbers are validated once when they cross the display-host seam, so
/// native font adapters never need to recover from NaN, infinity, zero, or a
/// negative size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PositiveFontScalar(f64);

impl PositiveFontScalar {
    pub fn new(value: f64) -> Option<Self> {
        (value.is_finite() && value > 0.0).then_some(Self(value))
    }

    pub const fn get(self) -> f64 {
        self.0
    }
}

/// The semantic size attached to a frame-font request.
///
/// GNU keeps integer font-spec sizes in pixels until font realization, while
/// floating sizes are points. Keeping those units in the type prevents a
/// frame-independent conversion from silently assuming one platform DPI.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FrameFontSize {
    Default,
    Pixels(NonZeroU32),
    Points(PositiveFontScalar),
    Relative(PositiveFontScalar),
}

impl FrameFontSize {
    pub fn pixels(value: i64) -> Option<Self> {
        u32::try_from(value)
            .ok()
            .and_then(NonZeroU32::new)
            .map(Self::Pixels)
    }

    pub fn points(value: f64) -> Option<Self> {
        let value = PositiveFontScalar::new(value)?;
        ((1.0..=f64::from(i32::MAX)).contains(&(value.get() * 10.0))).then_some(Self::Points(value))
    }

    pub fn relative(value: f64) -> Option<Self> {
        PositiveFontScalar::new(value).map(Self::Relative)
    }

    pub fn from_face_height(height: FaceHeight) -> Option<Self> {
        match height {
            FaceHeight::Absolute(tenths) => Self::points(f64::from(tenths) / 10.0),
            FaceHeight::Relative(scale) => Self::relative(scale),
        }
    }
}

/// Size used to open a font for a Lisp query, not to resize a frame.
///
/// GNU opens an unsized scalable entity at the smallest usable size, whereas
/// a named font can request pixels or points. Point conversion belongs to the
/// display host; neither the selected frame's font nor backing scale supplies
/// substitute metrics. An explicit font-spec DPI overrides the display DPI.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FontOpeningSize {
    SmallestUsable,
    Pixels(NonZeroU32),
    /// An unsized name uses GNU's moderate default, except on Cocoa where
    /// the native frame's `fontsize` parameter supplies the default points.
    NamedDefault {
        frame_fontsize: Option<PositiveFontScalar>,
    },
    Points {
        size: PositiveFontScalar,
        dpi: Option<NonZeroU32>,
    },
}
