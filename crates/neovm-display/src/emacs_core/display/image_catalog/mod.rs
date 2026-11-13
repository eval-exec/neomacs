//! Nonblocking image lookup used by redisplay.
//!
//! Image decoding and renderer upload happen outside the evaluator thread.
//! Callers receive a complete state immediately and never infer readiness
//! from optional metadata.

use crate::window::Frame;
pub use neomacs_display_protocol::ImageAnimationPolicy;
pub use neomacs_display_protocol::ImageRealization as ResolvedImageRealization;
pub use neomacs_display_protocol::image::EncodedBytes;
pub use neomacs_display_protocol::image_diagnostic::ImageLoadIdentity;
pub use neomacs_display_protocol::image_diagnostic::{ImageDiagnosticSubject, ImageFormatName};
pub use neomacs_display_protocol::{
    AxisSize, ImageColorContext, ImageEmbeddedMetadata, ImageFrameDelay, ImageFrameIndex,
    ImageHeuristicMask, ImageId, ImageLayoutExtent, ImageLoadAttempt, ImageLoadToken,
    ImageMaskKind, ImageMaskPolicy, ImageNativeExtent, ImageReportedExtent, ImageRotation,
    ImageSizeLimit, ImageSizeSpec, ImageStateEvent, OversizedImage,
};
use neovm_value::intern::{SymId, WellKnown};
use neovm_value::value::Value;

/// Fixed image variable identities; values are read anew at every use.
/// Shared cells contain scalar IDs only and follow the registry epoch across
/// all mutators. Their guards are released before any owner or Lisp read.
#[derive(Clone, Copy)]
enum ImageVariable {
    ScalingFactor,
    SizeLimit,
}

impl ImageVariable {
    #[inline]
    fn sym_id(self) -> SymId {
        match self {
            Self::ScalingFactor => {
                static SYMBOL: WellKnown = WellKnown::ImageScalingFactor;
                SYMBOL.id()
            }
            Self::SizeLimit => {
                static SYMBOL: WellKnown = WellKnown::MaxImageSize;
                SYMBOL.id()
            }
        }
    }
}

/// A finite, non-negative image scale stored by bits so image requests remain
/// exact cache keys.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ImageScaleFactor(u32);

impl ImageScaleFactor {
    #[must_use]
    pub fn get(self) -> f32 {
        f32::from_bits(self.0)
    }
}

impl TryFrom<f32> for ImageScaleFactor {
    type Error = &'static str;

    fn try_from(value: f32) -> Result<Self, Self::Error> {
        if value.is_finite() && value >= 0.0 {
            Ok(Self(if value == 0.0 {
                0.0_f32.to_bits()
            } else {
                value.to_bits()
            }))
        } else {
            Err("image scale must be finite and non-negative")
        }
    }
}

/// Meaning of an image spec's `:scale` property before frame realization.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ImageScalePolicy {
    /// No `:scale` key in the spec at all. GNU leaves the scale at 1 here and
    /// never consults `image-scaling-factor` (`double scale = 1` with no
    /// matching branch, src/image.c:2697-2736) — only an explicit
    /// `:scale default` opts into the variable.
    Unspecified,
    /// `:scale default` — resolve through GNU's `image-scaling-factor`.
    Default,
    /// A numeric scale written directly in the image spec.
    Explicit(ImageScaleFactor),
}

/// Parsed value of GNU's global `image-scaling-factor` variable.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ImageDefaultScale {
    Auto,
    Explicit(ImageScaleFactor),
}

/// Frame facts a GNU image lookup resolves against before it can load
/// anything: the semantic scaling inputs, and the `max-image-size` bound that
/// decides whether the image may be decoded at all.
///
/// Both come from the same two sources — the frame and the obarray — and are
/// read at the same moment, because GNU resolves scaling in
/// `compute_image_size` and the size limit in `check_image_size`
/// (`src/image.c:1811`) while handling the very same image. Carrying them in
/// one value is what lets a lookup that has a frame publish the scaling *and*
/// the limit to the loader, instead of only the scaling.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ImageScaleEnvironment {
    frame_column_width: ImageScaleFactor,
    device_scale: ImageScaleFactor,
    default_scale: ImageDefaultScale,
    /// GNU's `max-image-size`, measured against this frame. Starts at GNU's
    /// own default value so an environment nobody resolved a limit for still
    /// bounds what it loads.
    size_limit: ImageSizeLimit,
}

impl ImageScaleEnvironment {
    #[must_use]
    pub fn new(
        frame_column_width: f32,
        device_scale: f32,
        default_scale: ImageDefaultScale,
    ) -> Self {
        let frame_column_width = if frame_column_width.is_finite() && frame_column_width > 0.0 {
            frame_column_width
        } else {
            10.0
        };
        let device_scale = if device_scale.is_finite() && device_scale > 0.0 {
            device_scale
        } else {
            1.0
        };
        Self {
            frame_column_width: ImageScaleFactor::try_from(frame_column_width)
                .expect("sanitized frame column width is valid"),
            device_scale: ImageScaleFactor::try_from(device_scale)
                .expect("sanitized device scale is valid"),
            default_scale,
            size_limit: ImageSizeLimit::default(),
        }
    }

    /// Attach the frame's resolved `max-image-size` bound.
    #[must_use]
    pub const fn with_size_limit(mut self, size_limit: ImageSizeLimit) -> Self {
        self.size_limit = size_limit;
        self
    }

    /// The largest native extent this frame will load an image at.
    #[must_use]
    pub const fn size_limit(self) -> ImageSizeLimit {
        self.size_limit
    }

    /// The validated logical-to-device scale carried by this frame snapshot.
    ///
    /// Return the display protocol's domain type so layout geometry cannot
    /// accidentally consume the raw image-scaling scalar as logical pixels.
    #[must_use]
    pub fn device_scale(self) -> neomacs_display_protocol::DeviceScale {
        neomacs_display_protocol::DeviceScale::new(self.device_scale.get())
            .expect("ImageScaleEnvironment stores a validated positive device scale")
    }

    #[must_use]
    pub fn resolve(self, policy: ImageScalePolicy) -> ResolvedImageRealization {
        let device_scale = self.device_scale.get();
        // `report_scale` maps logical layout pixels → GNU Fimage_size pixels.
        // Only `:scale default` divides layout by device_scale; report multiplies
        // it back. Other policies leave layout already in image-pixel space.
        let (layout_scale, report_scale) = match policy {
            ImageScalePolicy::Unspecified => (1.0, 1.0),
            ImageScalePolicy::Explicit(scale) => (scale.get(), 1.0),
            ImageScalePolicy::Default => {
                let device_factor = match self.default_scale {
                    ImageDefaultScale::Auto => {
                        // GNU's FRAME_COLUMN_WIDTH is an integer device-pixel
                        // font metric.  Neomacs stores frame geometry in
                        // logical pixels, so recover the enclosing device
                        // column instead of rounding to the nearest pixel:
                        // the latter turns a 7px cell at 1.75 scale into 12px
                        // and loses the 13th pixel occupied by the font.
                        let device_column_width =
                            (self.frame_column_width.get() * device_scale).ceil();
                        if device_column_width > 10.0 {
                            device_column_width / 10.0
                        } else {
                            1.0
                        }
                    }
                    ImageDefaultScale::Explicit(scale) => scale.get(),
                };
                (device_factor / device_scale, device_scale)
            }
        };
        ResolvedImageRealization::new(layout_scale, device_scale, report_scale)
    }
}

impl Default for ImageScaleEnvironment {
    fn default() -> Self {
        Self::new(10.0, 1.0, ImageDefaultScale::Auto)
    }
}

/// Resolve GNU's dynamically bound `image-scaling-factor` together with the
/// selected frame facts.  Redisplay and synchronous image builtins share this
/// entry point so the same image spec cannot acquire two geometries.
#[must_use]
pub fn image_scale_environment(
    frame: &Frame,
    vars: neovm_eval::emacs_core::eval::VariableView<'_>,
) -> ImageScaleEnvironment {
    let default_scale = match vars.value(ImageVariable::ScalingFactor.sym_id()) {
        Some(value) if value.is_bare_symbol_named("auto") => ImageDefaultScale::Auto,
        Some(value) => numeric_image_scale(value)
            .map(ImageDefaultScale::Explicit)
            .unwrap_or(ImageDefaultScale::Auto),
        None => ImageDefaultScale::Auto,
    };
    ImageScaleEnvironment::new(
        frame.char_width,
        frame.device_scale_factor as f32,
        default_scale,
    )
    .with_size_limit(image_size_limit(frame, vars))
}

/// GNU's `max-image-size` (`src/image.c:13037`), resolved against FRAME.
///
/// `Vmax_image_size` is an ordinary special variable, so what counts is the
/// binding in force when the lookup runs — a `let` around the call that makes
/// the image visible is honoured, and a binding that has already been unwound
/// is not, which is exactly GNU's behaviour for a load that happens during
/// redisplay. `DEFVAR_LISP` keeps it out of every buffer, so there is no
/// buffer-local case to consider.
#[must_use]
pub fn image_size_limit(
    frame: &Frame,
    vars: neovm_eval::emacs_core::eval::VariableView<'_>,
) -> ImageSizeLimit {
    // GNU tests `FIXNUMP` before `FLOATP`; `Value` splits the same way, and
    // anything else (nil, a string, a symbol) is GNU's "no explicit limit".
    match vars.value(ImageVariable::SizeLimit.sym_id()) {
        Some(value) if value.as_int().is_some() => {
            ImageSizeLimit::from_axis_pixels(value.as_int().expect("tested fixnum"))
        }
        Some(value) if value.as_float().is_some() => ImageSizeLimit::from_frame_ratio(
            value.as_float().expect("tested float"),
            Some(frame_pixel_extent(frame)),
        ),
        _ => ImageSizeLimit::UNLIMITED,
    }
}

/// GNU's `FRAME_PIXEL_WIDTH` / `FRAME_PIXEL_HEIGHT` (`src/frame.h`).
///
/// Neomacs stores a frame's geometry in logical pixels and publishes the
/// physical scale beside it, while GNU's macros are already in device pixels:
/// recover the physical extent so a fractional `max-image-size` means the same
/// fraction of the same frame on both sides.
fn frame_pixel_extent(frame: &Frame) -> ImageNativeExtent {
    let scale = if frame.device_scale_factor.is_finite() && frame.device_scale_factor > 0.0 {
        frame.device_scale_factor
    } else {
        1.0
    };
    ImageNativeExtent::new(
        physical_dimension(frame.width, scale),
        physical_dimension(frame.height, scale),
    )
}

fn physical_dimension(logical: u32, scale: f64) -> u32 {
    let physical = f64::from(logical) * scale;
    if physical >= f64::from(u32::MAX) {
        u32::MAX
    } else {
        physical.round().max(0.0) as u32
    }
}

#[must_use]
pub fn numeric_image_scale(value: Value) -> Option<ImageScaleFactor> {
    let scale = value
        .as_float()
        .or_else(|| value.as_int().map(|value| value as f64))?;
    (scale.is_finite() && scale >= 0.0)
        .then(|| ImageScaleFactor::try_from(scale as f32).ok())
        .flatten()
}

pub use neovm_eval::emacs_core::eval::protocol::image_catalog::{
    FailedImage, ImageAnimationInvalidation, ImageBaseUri, ImageCatalog, ImageDataSource,
    ImageFileName, ImageInvalidation, ImageInvalidationResult, ImageLookup, ImagePlacement,
    ImageResolveRequest, ImageResolveSource, PendingImage, ReadyImage, ResolvedImageMetadata,
};
pub use neovm_value::image_identity::ImageSpecIdentity;

#[cfg(test)]
#[path = "tests/image_catalog_test.rs"]
mod tests;

#[cfg(test)]
#[path = "tests/image_size_limit_test.rs"]
mod size_limit_tests;

#[cfg(test)]
#[path = "tests/fixed_symbols_test.rs"]
mod fixed_symbols_tests;
