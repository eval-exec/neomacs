//! Header-only image geometry, for images whose pixels do not exist yet.
//!
//! Layout has to place a large image before its decode finishes: the encoded
//! header already carries the native extent that `:width`/`:max-*`, the frame
//! realization and `:rotation` are resolved against, and reading it costs a
//! bounded prefix of the file instead of the whole decode.
//!
//! The extent reported here is the extent the same source's full decode
//! reports, because both are the decoder's own dimensions (raster frames of an
//! animated source are composited to the same canvas the header names, and the
//! byte-oriented formats below read the same header fields their decoders do).
//! A pending layout built from it therefore cannot move when the pixels land —
//! which is the whole point: a layout that resolves late is survivable, a
//! layout that resolves *wrongly* is worse than waiting.
//!
//! Vector documents are deliberately absent. An SVG's intrinsic extent comes
//! from a document parse whose bounding-box fallback depends on resolved
//! resources and colors, so it is not a header and cannot be guaranteed to
//! agree with the rasterized result.

use std::fs::File;
use std::io::BufReader;
use std::panic::{AssertUnwindSafe, catch_unwind};

use neomacs_display_protocol::{
    ImageIntrinsicExtent, ImageLayoutExtent, ImageNativeExtent, ImageRealization, ImageRotation,
    ImageSizeLimit, ImageSizeSpec, OversizedImage,
};

/// Encoded image to measure without decoding it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImageProbeSource<'a> {
    /// A resolved filesystem path.
    File(&'a str),
    /// Encoded bytes, as held by an image `:data` spec.
    Data(&'a [u8]),
}

/// GNU `check_image_size` (`src/image.c:1811-1836`) at the point this port can
/// ask it having allocated nothing.
///
/// The header read that resolves geometry carries the dimensions the bound is
/// compared against, so a source over `limit` is refused here — before the
/// decoder is asked for a pixel, and before it could allocate one. That is what
/// makes this the seam for the rule: GNU refuses inside each loader, against the
/// size it has just read from the file and before it creates an image, and this
/// is the same moment in this port's lifecycle.
///
/// `Ok(())` also covers a source this crate cannot measure — an SVG, whose
/// extent comes from a document parse whose result depends on resolved
/// resources. The only honest answer for an unknown extent is "not known to be
/// over the limit", so such a source is admitted here and left to the decoder.
/// GNU checks those after parsing them; this port has no post-parse check of its
/// own to make.
///
/// # Errors
///
/// Returns the measurement that exceeded the limit, as [`OversizedImage`].
pub fn admit(source: ImageProbeSource<'_>, limit: ImageSizeLimit) -> Result<(), OversizedImage> {
    let Some(intrinsic) = probe_intrinsic_extent(source) else {
        return Ok(());
    };
    let native = intrinsic.native_ceiling();
    if limit.permits(native) {
        return Ok(());
    }
    Err(OversizedImage::new(native, limit))
}

/// Intrinsic (pre-realization) extent read from the encoded header alone.
///
/// `None` means the source carries no extent this crate can read without
/// decoding it; the caller keeps whatever placeholder it would have used.
pub fn probe_intrinsic_extent(source: ImageProbeSource<'_>) -> Option<ImageIntrinsicExtent> {
    // A decoder that panics on malformed input must not take the prober thread
    // down with it: one thread serves every image a session ever displays, and
    // a dead prober would silently leave every later image on its placeholder.
    catch_unwind(AssertUnwindSafe(|| match source {
        ImageProbeSource::File(path) => file_intrinsic_extent(path),
        ImageProbeSource::Data(data) => data_intrinsic_extent(data),
    }))
    .ok()
    .flatten()
}

/// Layout extent for `source` resolved from its encoded header alone.
///
/// The caller may place an image with this before any pixel exists. It must
/// equal the `layout` of the metadata the same request's full decode
/// publishes: both are [`ImageRealization::resolve_geometry`] over the same
/// native extent, so agreement reduces to the two extents being equal.
#[must_use]
pub fn probe_image_layout(
    source: ImageProbeSource<'_>,
    size: ImageSizeSpec,
    rotation: ImageRotation,
    realization: ImageRealization,
) -> Option<ImageLayoutExtent> {
    let intrinsic = probe_intrinsic_extent(source)?;
    Some(
        realization
            .resolve_geometry(size, intrinsic, rotation)
            .layout(),
    )
}

/// Raster formats read only their header. Animated sources composite to the
/// canvas the header names, so frame selection cannot change the extent.
fn file_intrinsic_extent(path: &str) -> Option<ImageIntrinsicExtent> {
    let file = File::open(path).ok()?;
    let reader = BufReader::new(file);
    let dimensions = image::ImageReader::new(reader)
        .with_guessed_format()
        .ok()?
        .into_dimensions()
        .ok()?;
    Some(ImageNativeExtent::new(dimensions.0, dimensions.1).into())
}

/// Raster headers, then the byte-oriented formats `image` cannot read at all.
fn data_intrinsic_extent(data: &[u8]) -> Option<ImageIntrinsicExtent> {
    let cursor = std::io::Cursor::new(data);
    if let Ok(dimensions) = image::ImageReader::new(BufReader::new(cursor))
        .with_guessed_format()
        .ok()?
        .into_dimensions()
    {
        return Some(ImageNativeExtent::new(dimensions.0, dimensions.1).into());
    }

    // Fallback: try XPM header
    if let Some((width, height)) = crate::xpm::query_xpm_dimensions(data) {
        return Some(ImageNativeExtent::new(width, height).into());
    }

    // Fallback: try XBM header
    if let Some((width, height)) = crate::xbm::query_xbm_dimensions(data) {
        return Some(ImageNativeExtent::new(width, height).into());
    }

    None
}
