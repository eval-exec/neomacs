//! Basic types used throughout the display engine.

use std::num::NonZeroU64;
use std::ops::{Add, Sub};

macro_rules! display_id_type {
    ($name:ident, $raw:ty) => {
        #[repr(transparent)]
        #[derive(
            Debug,
            Clone,
            Copy,
            PartialEq,
            Eq,
            PartialOrd,
            Ord,
            Hash,
            Default,
            serde::Serialize,
            serde::Deserialize,
        )]
        pub struct $name($raw);

        impl $name {
            #[must_use]
            pub const fn new(raw: $raw) -> Self {
                Self(raw)
            }

            #[must_use]
            pub const fn get(self) -> $raw {
                self.0
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                self.0.fmt(f)
            }
        }
    };
}

display_id_type!(DisplayFrameId, u64);
display_id_type!(DisplayWindowId, i64);
display_id_type!(ImageId, u32);
display_id_type!(VideoId, u32);
display_id_type!(WebViewId, u32);
display_id_type!(XwidgetId, u32);
// Shader surface: a compositor-rendered texture created from Elisp
// (`doc/display-engine/SHADER_SURFACES.md`).
display_id_type!(SurfaceId, u32);
// Realized face id, scoped to the owning frame's face table. Basic faces
// occupy 0..BasicFaceId::SENTINEL; dynamic faces are allocated above that.
// Raw u32 face ids remain only at the FFI edge (`FaceDataFFI`) and the
// neovm bridge boundary, which wrap into `FaceId` immediately.
display_id_type!(FaceId, u32);

/// Monotonic attempt number for asynchronous work targeting one image.
///
/// An [`ImageId`] names the stable logical asset referenced by presentations;
/// this value names one replaceable decode/upload attempt for that asset.
/// Keeping the domains distinct prevents a late worker completion from being
/// accepted as the current contents of an image.
#[repr(transparent)]
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct ImageLoadAttempt(NonZeroU64);

impl ImageLoadAttempt {
    #[must_use]
    pub const fn new(raw: u64) -> Option<Self> {
        match NonZeroU64::new(raw) {
            Some(raw) => Some(Self(raw)),
            None => None,
        }
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

impl std::fmt::Display for ImageLoadAttempt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.get().fmt(f)
    }
}

/// Capability for one particular asynchronous realization of an image.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct ImageLoadToken {
    image: ImageId,
    attempt: ImageLoadAttempt,
}

impl ImageLoadToken {
    #[must_use]
    pub const fn new(image: ImageId, attempt: ImageLoadAttempt) -> Self {
        Self { image, attempt }
    }

    #[must_use]
    pub const fn image(self) -> ImageId {
        self.image
    }

    #[must_use]
    pub const fn attempt(self) -> ImageLoadAttempt {
        self.attempt
    }
}

impl std::fmt::Display for ImageLoadToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}@{}", self.image, self.attempt)
    }
}

/// A deterministic logical-pixel quantity used while laying out a frame.
///
/// Layout uses 26.6-style fixed-point arithmetic (1/64 logical pixel).  Font
/// backends and protocol adapters may speak `f32`, but values entering layout
/// are quantized once so equality, tab stops, clipping, and accumulated
/// advances cannot depend on floating-point noise.  Conversion to device
/// pixels belongs at the sealed-presentation/render boundary.
#[repr(transparent)]
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Default,
    serde::Serialize,
    serde::Deserialize,
)]
pub struct LayoutUnit(i64);

impl LayoutUnit {
    pub const FRACTION_BITS: u32 = 6;
    pub const UNITS_PER_PIXEL: i64 = 1_i64 << Self::FRACTION_BITS;
    pub const ZERO: Self = Self(0);

    #[must_use]
    pub const fn from_raw(raw: i64) -> Self {
        Self(raw)
    }

    /// Quantize a logical-pixel measurement at the boundary where it enters
    /// deterministic layout. Rust's float-to-integer cast gives defined
    /// saturation for infinities and maps NaN to zero.
    #[must_use]
    pub fn from_px(px: f32) -> Self {
        Self((px * Self::UNITS_PER_PIXEL as f32).round() as i64)
    }

    #[must_use]
    pub const fn raw(self) -> i64 {
        self.0
    }

    #[must_use]
    pub fn to_px(self) -> f32 {
        self.0 as f32 / Self::UNITS_PER_PIXEL as f32
    }

    #[must_use]
    pub const fn max(self, other: Self) -> Self {
        if self.0 >= other.0 { self } else { other }
    }

    #[must_use]
    pub const fn min(self, other: Self) -> Self {
        if self.0 <= other.0 { self } else { other }
    }

    #[must_use]
    pub const fn saturating_sub(self, other: Self) -> Self {
        Self(self.0.saturating_sub(other.0))
    }
}

impl Add for LayoutUnit {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self(self.0.saturating_add(other.0))
    }
}

impl Sub for LayoutUnit {
    type Output = Self;

    fn sub(self, other: Self) -> Self {
        Self(self.0.saturating_sub(other.0))
    }
}

impl std::ops::Mul<i64> for LayoutUnit {
    type Output = Self;

    fn mul(self, factor: i64) -> Self {
        Self(self.0.saturating_mul(factor))
    }
}

impl std::fmt::Display for LayoutUnit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}px", self.to_px())
    }
}

/// A physical-pixel quantity.
///
/// Introduced at protocol conversion seams (e.g.
/// [`crate::frame_glyphs::DisplaySlotId::from_pixels`] and the row-damage
/// `dvpos` shift) so pixel-space and cell-grid-space values cannot be mixed
/// silently at the boundaries where they convert. Scoped on purpose: the
/// bulk of raw `f32` pixel fields across layout/render structs migrates
/// separately (see the 4d follow-up note).
///
/// Serializes as a plain `f32` (serde newtype semantics), so adopting it on
/// serialized fields does not change snapshot shape.
#[repr(transparent)]
#[derive(
    Debug, Clone, Copy, PartialEq, PartialOrd, Default, serde::Serialize, serde::Deserialize,
)]
pub struct Px(pub f32);

impl Px {
    #[must_use]
    pub const fn get(self) -> f32 {
        self.0
    }

    /// Whole-cell index covered by this pixel offset when one cell spans
    /// `cell` pixels: round to nearest, clamp below zero. This is the
    /// rounding contract of [`crate::frame_glyphs::DisplaySlotId::from_pixels`].
    /// A non-positive cell size yields cell 0 (degenerate geometry guard).
    #[must_use]
    pub fn cells_rounded(self, cell: Px) -> Cell {
        if cell.0 > 0.0 {
            Cell((self.0 / cell.0).round().max(0.0) as u32)
        } else {
            Cell(0)
        }
    }
}

impl Add for Px {
    type Output = Self;
    fn add(self, other: Self) -> Self {
        Self(self.0 + other.0)
    }
}

impl Sub for Px {
    type Output = Self;
    fn sub(self, other: Self) -> Self {
        Self(self.0 - other.0)
    }
}

impl std::ops::Mul<f32> for Px {
    type Output = Self;
    fn mul(self, factor: f32) -> Self {
        Self(self.0 * factor)
    }
}

impl std::fmt::Display for Px {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}px", self.0)
    }
}

/// An integral cell-grid coordinate (visual row or column index).
///
/// The typed result of a pixel→grid conversion; see [`Px::cells_rounded`].
#[repr(transparent)]
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Default,
    serde::Serialize,
    serde::Deserialize,
)]
pub struct Cell(pub u32);

impl Cell {
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

impl Add for Cell {
    type Output = Self;
    fn add(self, other: Self) -> Self {
        Self(self.0 + other.0)
    }
}

impl Sub for Cell {
    type Output = Self;
    fn sub(self, other: Self) -> Self {
        Self(self.0.saturating_sub(other.0))
    }
}

impl std::fmt::Display for Cell {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// sRGB u8 component → linear f32, precomputed with the exact
/// `srgb_component_to_linear` formula so table lookups are
/// bit-identical to the direct computation.
static SRGB_U8_TO_LINEAR: std::sync::LazyLock<[f32; 256]> = std::sync::LazyLock::new(|| {
    std::array::from_fn(|i| Color::srgb_component_to_linear(i as f32 / 255.0))
});

/// Linear-space decision boundaries for quantizing a linear component
/// to 8-bit sRGB: `BOUNDARIES[j] = srgb_to_linear((j + 0.5) / 255)`.
/// Because the transfer function is monotonic, the number of
/// boundaries at or below a linear value x equals
/// `round(255 * linear_to_srgb(x))`, so an 8-step binary search
/// replaces a `powf` per component.
static LINEAR_TO_SRGB_U8_BOUNDARIES: std::sync::LazyLock<[f32; 255]> =
    std::sync::LazyLock::new(|| {
        std::array::from_fn(|j| Color::srgb_component_to_linear((j as f32 + 0.5) / 255.0))
    });

/// RGBA color with f32 components (0.0 - 1.0)
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    pub const fn rgb(r: f32, g: f32, b: f32) -> Self {
        Self::new(r, g, b, 1.0)
    }

    pub fn from_u8(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self {
            r: r as f32 / 255.0,
            g: g as f32 / 255.0,
            b: b as f32 / 255.0,
            a: a as f32 / 255.0,
        }
    }

    /// Convert from Emacs pixel value (0xAARRGGBB or 0x00RRGGBB).
    /// Performs sRGB→linear conversion since Emacs colors are sRGB
    /// and the GPU surface uses an sRGB format (expects linear values).
    pub fn from_pixel(pixel: u32) -> Self {
        let a = ((pixel >> 24) & 0xFF) as u8;
        let r = ((pixel >> 16) & 0xFF) as usize;
        let g = ((pixel >> 8) & 0xFF) as usize;
        let b = (pixel & 0xFF) as usize;
        // If alpha is 0, assume fully opaque
        let a = if a == 0 { 255 } else { a };
        let lut = &*SRGB_U8_TO_LINEAR;
        Self {
            r: lut[r],
            g: lut[g],
            b: lut[b],
            a: a as f32 / 255.0,
        }
    }

    /// Convert a single sRGB component (0.0-1.0) to linear space.
    fn srgb_component_to_linear(c: f32) -> f32 {
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    }

    /// Convert this color from sRGB to linear space.
    /// Use when colors come from Emacs (sRGB) and need to be used with
    /// an sRGB surface format where the GPU expects linear values.
    pub fn srgb_to_linear(self) -> Self {
        Self {
            r: Self::srgb_component_to_linear(self.r),
            g: Self::srgb_component_to_linear(self.g),
            b: Self::srgb_component_to_linear(self.b),
            a: self.a, // alpha is always linear
        }
    }

    /// Convert a single linear component (0.0-1.0) back to sRGB.
    /// Inverse of `srgb_component_to_linear`.
    fn linear_component_to_srgb(c: f32) -> f32 {
        if c <= 0.0031308 {
            c * 12.92
        } else {
            1.055 * c.powf(1.0 / 2.4) - 0.055
        }
    }

    /// Convert this color from linear space back to sRGB.
    ///
    /// Use at output boundaries that expect sRGB values, such as
    /// the TTY rasterizer which emits 24-bit ANSI color codes
    /// interpreted as sRGB by terminals. The GPU surface
    /// (`Bgra8UnormSrgb`) handles this conversion automatically,
    /// so only non-GPU outputs need it.
    ///
    /// Mirrors the architectural boundary in GNU Emacs where
    /// face pixel values (sRGB) are emitted directly to the TTY
    /// via terminfo `setab`/`setaf` with no conversion
    /// (`src/term.c::tty_defined_color`), while the X11/Cairo
    /// backend lets the server apply gamma at draw time.
    pub fn linear_to_srgb(self) -> Self {
        Self {
            r: Self::linear_component_to_srgb(self.r),
            g: Self::linear_component_to_srgb(self.g),
            b: Self::linear_component_to_srgb(self.b),
            a: self.a,
        }
    }

    /// Quantize a linear component directly to its 8-bit sRGB value:
    /// equivalent to `(linear_component_to_srgb(c).clamp(0.0, 1.0) * 255.0)
    /// .round() as u8` but via a binary search over precomputed linear-space
    /// midpoint boundaries instead of a `powf`. Hot on the TTY emit path,
    /// which converts cell colors every rasterized frame.
    pub fn linear_component_to_srgb_u8(c: f32) -> u8 {
        let bounds = &*LINEAR_TO_SRGB_U8_BOUNDARIES;
        // A missing ordering routes NaN to 0, matching the saturating `as u8`
        // cast of the arithmetic form.
        if c.partial_cmp(&bounds[0]) != Some(std::cmp::Ordering::Greater) {
            return 0;
        }
        if c >= bounds[254] {
            return 255;
        }
        bounds.partition_point(|&bound| bound <= c) as u8
    }

    // Common colors
    pub const BLACK: Self = Self::rgb(0.0, 0.0, 0.0);
    pub const WHITE: Self = Self::rgb(1.0, 1.0, 1.0);
    pub const RED: Self = Self::rgb(1.0, 0.0, 0.0);
    pub const GREEN: Self = Self::rgb(0.0, 1.0, 0.0);
    pub const BLUE: Self = Self::rgb(0.0, 0.0, 1.0);
    pub const TRANSPARENT: Self = Self::new(0.0, 0.0, 0.0, 0.0);
}

impl Default for Color {
    fn default() -> Self {
        Self::BLACK
    }
}

/// 2D point with f32 coordinates
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    #[must_use]
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    pub const ZERO: Self = Self::new(0.0, 0.0);
}

impl Add for Point {
    type Output = Self;
    fn add(self, other: Self) -> Self {
        Self::new(self.x + other.x, self.y + other.y)
    }
}

impl Sub for Point {
    type Output = Self;
    fn sub(self, other: Self) -> Self {
        Self::new(self.x - other.x, self.y - other.y)
    }
}

/// 2D size with f32 dimensions
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct Size {
    pub width: f32,
    pub height: f32,
}

impl Size {
    #[must_use]
    pub const fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }

    pub const ZERO: Self = Self::new(0.0, 0.0);
}

/// Rectangle with position and size
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    #[must_use]
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    #[must_use]
    pub fn from_point_size(point: Point, size: Size) -> Self {
        Self::new(point.x, point.y, size.width, size.height)
    }

    #[must_use]
    pub fn origin(&self) -> Point {
        Point::new(self.x, self.y)
    }

    #[must_use]
    pub fn size(&self) -> Size {
        Size::new(self.width, self.height)
    }

    #[must_use]
    pub fn right(&self) -> f32 {
        self.x + self.width
    }

    #[must_use]
    pub fn bottom(&self) -> f32 {
        self.y + self.height
    }

    #[must_use]
    pub fn contains(&self, point: Point) -> bool {
        point.x >= self.x && point.x < self.right() && point.y >= self.y && point.y < self.bottom()
    }

    #[must_use]
    pub fn intersects(&self, other: &Rect) -> bool {
        self.x < other.right()
            && self.right() > other.x
            && self.y < other.bottom()
            && self.bottom() > other.y
    }

    pub const ZERO: Self = Self::new(0.0, 0.0, 0.0, 0.0);
}

/// Animated cursor position override for smooth cursor motion.
///
/// When cursor animation is enabled, the render thread interpolates the cursor
/// position and passes this struct to the renderer instead of using the raw
/// frame glyph coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AnimatedCursor {
    pub window_id: DisplayWindowId,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    /// When Some, draw cursor as a quad from these 4 corner positions (spring trail).
    /// Order: [top-left, top-right, bottom-right, bottom-left].
    pub corners: Option<[(f32, f32); 4]>,
    /// Which frame owns this cursor (0 = root frame, non-zero = child frame_id)
    pub frame_id: DisplayFrameId,
}

/// Cursor animation style.
///
/// Controls how the smooth cursor interpolates between positions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
#[repr(u8)]
#[non_exhaustive]
pub enum CursorAnimStyle {
    /// Exponential decay (current default). No fixed duration; `speed` controls rate.
    Exponential = 0,
    /// Critically-damped spring (Neovide-style). Physics-based, natural feel.
    CriticallyDampedSpring = 1,
    /// Ease-out quadratic: gentle deceleration.
    EaseOutQuad = 2,
    /// Ease-out cubic: stronger deceleration.
    EaseOutCubic = 3,
    /// Ease-out exponential: sharp initial speed, rapid deceleration.
    EaseOutExpo = 4,
    /// Ease-in-out cubic: smooth S-curve acceleration + deceleration.
    EaseInOutCubic = 5,
    /// Linear: constant speed.
    Linear = 6,
}

impl CursorAnimStyle {
    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::CriticallyDampedSpring,
            2 => Self::EaseOutQuad,
            3 => Self::EaseOutCubic,
            4 => Self::EaseOutExpo,
            5 => Self::EaseInOutCubic,
            6 => Self::Linear,
            _ => Self::Exponential,
        }
    }
}

// ---------------------------------------------------------------------------
// Easing functions (t in 0.0..=1.0, returns 0.0..=1.0)
// ---------------------------------------------------------------------------

/// Ease-out quadratic: `−t(t−2)` — gentle deceleration.
pub fn ease_out_quad(t: f32) -> f32 {
    -t * (t - 2.0)
}

/// Ease-out cubic: `(t−1)³ + 1` — stronger deceleration.
pub fn ease_out_cubic(t: f32) -> f32 {
    let n = t - 1.0;
    n * n * n + 1.0
}

/// Ease-out exponential: `1 − 2^(−10t)` — sharp deceleration.
pub fn ease_out_expo(t: f32) -> f32 {
    if t >= 1.0 {
        1.0
    } else {
        1.0 - 2f32.powf(-10.0 * t)
    }
}

/// Ease-in-out cubic: smooth S-curve.
pub fn ease_in_out_cubic(t: f32) -> f32 {
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        let n = -2.0 * t + 2.0;
        1.0 - n * n * n / 2.0
    }
}

/// Linear easing (identity).
pub fn ease_linear(t: f32) -> f32 {
    t
}

/// 2D transform matrix
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform {
    /// 2D affine transform: [a, b, c, d, tx, ty]
    /// | a  b  0 |
    /// | c  d  0 |
    /// | tx ty 1 |
    pub matrix: [f32; 6],
}

impl Transform {
    pub const IDENTITY: Self = Self {
        matrix: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
    };

    pub fn translate(tx: f32, ty: f32) -> Self {
        Self {
            matrix: [1.0, 0.0, 0.0, 1.0, tx, ty],
        }
    }

    pub fn scale(sx: f32, sy: f32) -> Self {
        Self {
            matrix: [sx, 0.0, 0.0, sy, 0.0, 0.0],
        }
    }
}

impl Default for Transform {
    fn default() -> Self {
        Self::IDENTITY
    }
}

#[cfg(test)]
#[path = "types_test.rs"]
mod tests;
