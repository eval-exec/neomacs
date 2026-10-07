//! Row-wise decode, for sources whose decoder can hand back part of a frame.
//!
//! A large image takes long enough to decode that the honest thing to do with
//! it is to show it arriving: rows become available from the top while the
//! bottom is still being read. [`BandSource`] is the route decision — a
//! row-wise decoder, or the all-or-nothing decode this renderer did before —
//! and [`BandedSource::next_band`] is the progress itself.
//!
//! Four properties this module is built to keep:
//!
//! - **Bands tile the source.** A source derives every band from the cursor it
//!   advances itself, and nothing can ask a source for a named range, so its
//!   bands cannot overlap, skip or arrive out of order.
//! - **A banded decode agrees with the whole one.** Both decode through the
//!   same transforms and both land in the same RGBA, which is what lets a
//!   decode move between the two paths without the picture changing.
//! - **Failure is not truncation.** A row-wise decode that errors mid-stream
//!   stops at [`BandStep::Failed`]; its bands are abandoned and the caller
//!   decodes the whole image again. Only [`BandedSource::into_raster`] on a
//!   completed source yields pixels.
//! - **A band knows where it goes, and its pixels are already there.** The
//!   decode writes into the raster its texture holds
//!   ([`crate::image_scale::RasterTarget`]) rather than into a native-size image
//!   it would later have to resample, so a band is a range of the raster with
//!   the pixels for it, and a source that reaches the end hands back that same
//!   raster — the preview during a decode and the finished image are one
//!   buffer, not two that have to agree.

use std::io::Cursor;
use std::num::NonZeroU32;
use std::sync::Arc;

use neomacs_display_protocol::image::EncodedBytes;
use neomacs_display_protocol::{
    ImageIntrinsicExtent, ImageMaskKind, ImageMaskPolicy, ImageNativeExtent, ImageRasterExtent,
    ImageRealization, ImageRotation, ImageSizeSpec,
};
use zune_jpeg::JpegDecoder as ZuneDecoder;
// Reached through `zune_jpeg` rather than depended on directly: the decoder
// was built against this exact `zune_core`, and naming it any other way would
// be naming a second copy whose `ZCursor` is a different type.
use zune_jpeg::zune_core::bytestream::ZCursor as ZuneCursor;
use zune_jpeg::zune_core::colorspace::ColorSpace as ZuneColorSpace;
use zune_jpeg::zune_core::options::DecoderOptions as ZuneOptions;

use crate::image_scale::RasterTarget;

/// Smallest source, in pixels, that banding engages for.
///
/// Below this one decode is not visibly slow — the measured 36-megapixel PNG
/// takes 1.4 s here, so a four-megapixel source is tens of milliseconds — and
/// the whole-image path is both simpler and no slower. The threshold only has
/// to be the point where the first band visibly beats the last one.
pub(crate) const BANDING_MIN_PIXELS: u64 = 4_000_000;

/// Largest run of source rows one band reads, in RGBA.
///
/// This is what bounds the number of bands from below: with no width to worry
/// about, a source arrives in at most `total_bytes / BAND_MAX_BYTES` bands, so
/// even a hundred-megapixel image bands into about a hundred pieces instead of
/// following its height into thousands. It bounds the *reading*, not what a
/// band hands over — a band's payload is the raster rows those source rows
/// filled, which is smaller by the reduction.
pub(crate) const BAND_MAX_BYTES: usize = 4 * 1024 * 1024;

/// Most bands one source is planned to produce.
///
/// One band per scanline would be progressive in name only: the first band
/// would arrive almost as late as the last, because a decoder still has to
/// produce the rows above it.
pub(crate) const BAND_TARGET_COUNT: u32 = 32;

/// Whether one decode's bands have somewhere to go.
///
/// Two things about a realization stop a band from having a destination, and
/// neither is about the band:
///
/// - A quarter turn is applied to the *scaled* image (GNU turns after sizing),
///   so a band of source rows becomes a column range of the stored raster, and
///   "how far down has it filled" stops being one number.
/// - A mask policy that rewrites pixels needs the whole image before it can say
///   what any pixel is — `ImageHeuristicMask::FourCorners` is literally the
///   four corners.
///
/// Both keep the band, which is still progress, and decline the placement,
/// which is why this is named after the side that is missing.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BandFilling {
    /// A band's rows go straight into the texture, below the last one written.
    TopDown,
    /// The texture is filled once, when the decode completes.
    Deferred,
}

impl BandFilling {
    pub(crate) const fn of(rotation: ImageRotation, mask: ImageMaskPolicy) -> Self {
        if matches!(rotation, ImageRotation::None) && matches!(mask, ImageMaskPolicy::Preserve) {
            Self::TopDown
        } else {
            Self::Deferred
        }
    }
}

/// A run of whole source rows, in decode order.
///
/// `len` is non-zero, so an empty run cannot be built. The invariant that one
/// decode's runs tile its source — contiguous, in order, no overlap — belongs
/// to the source that hands them out, which derives every one from the cursor
/// it advances; the constructor is open so a run can also be published,
/// carried or asserted on without a source in hand.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RowRange {
    start: u32,
    len: NonZeroU32,
}

impl RowRange {
    /// A run of `len` rows from `start`.
    #[must_use]
    pub const fn new(start: u32, len: NonZeroU32) -> Self {
        Self { start, len }
    }

    /// First source row this range covers.
    #[must_use]
    pub const fn start(self) -> u32 {
        self.start
    }

    /// How many source rows it covers; never zero.
    #[must_use]
    pub const fn len(self) -> NonZeroU32 {
        self.len
    }

    /// One past the last source row it covers.
    #[must_use]
    pub const fn end(self) -> u32 {
        self.start + self.len.get()
    }
}

/// A run of rows of the texture that will hold an image.
///
/// Built by whatever filled them — [`RasterTarget`], which writes the raster
/// from row zero down — so a destination cannot name rows outside it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TextureRows {
    start: u32,
    len: NonZeroU32,
}

impl TextureRows {
    /// A run of `len` raster rows from `start`; `len` is non-zero, so a run
    /// that fills nothing cannot be built.
    pub(crate) const fn new(start: u32, len: NonZeroU32) -> Self {
        Self { start, len }
    }

    /// First texture row this range covers.
    #[must_use]
    pub const fn start(self) -> u32 {
        self.start
    }

    /// How many texture rows it covers; never zero.
    #[must_use]
    pub const fn len(self) -> NonZeroU32 {
        self.len
    }

    /// One past the last texture row it covers.
    #[must_use]
    pub const fn end(self) -> u32 {
        self.start + self.len.get()
    }
}

/// One band's destination: the raster it belongs to and the rows it fills.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BandPlacement {
    raster: ImageRasterExtent,
    rows: TextureRows,
}

impl BandPlacement {
    /// Where a run of the raster's rows belongs.
    ///
    /// Built by whatever filled those rows — [`RasterTarget`] and nothing else
    /// — so a placement cannot name rows outside the raster it was filled in.
    pub(crate) const fn new(raster: ImageRasterExtent, rows: TextureRows) -> Self {
        Self { raster, rows }
    }

    /// The raster of the texture this band belongs in.
    #[must_use]
    pub const fn raster(self) -> ImageRasterExtent {
        self.raster
    }

    /// The texture rows it fills.
    #[must_use]
    pub const fn rows(self) -> TextureRows {
        self.rows
    }
}

/// A band's pixels in the texture's own coordinates, ready to be written.
///
/// `pixels` covers exactly [`BandPlacement::rows`] of a
/// [`BandPlacement::raster`]-sized texture, so the write that carries them
/// writes the rows it claims and no others — and the constructor is where that
/// is checked, so a band cannot claim a rectangle it does not carry.
///
/// The scaling that produces them happens on the thread that just decoded the
/// rows, rather than on the render thread: a frame that is about to be drawn
/// must not spend itself on it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RasterBand {
    placement: BandPlacement,
    pixels: Arc<[u8]>,
}

impl RasterBand {
    /// The pixels one band wrote for the rows of `placement`.
    pub(crate) fn new(placement: BandPlacement, pixels: Arc<[u8]>) -> Self {
        let raster = placement.raster();
        let rows = placement.rows();
        assert_eq!(
            pixels.len(),
            raster.width() as usize * rows.len().get() as usize * 4,
            "a band carries exactly the rows it fills"
        );
        Self { placement, pixels }
    }

    /// Where these pixels go.
    #[must_use]
    pub const fn placement(&self) -> BandPlacement {
        self.placement
    }

    /// RGBA pixels, `raster.width() * rows.len() * 4` bytes, row-major from
    /// `rows.start()`.
    #[must_use]
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }
}

/// One band of a decode: the source rows it consumed, and the texture rows it
/// filled.
///
/// Two views of the same work, because two consumers want different things.
/// [`Self::source`] is the decode's own progress — which rows of the source
/// exist now, which is what the display side reports — and [`Self::placed`] is
/// where those rows went, which is what the renderer writes. Neither is
/// optional: a band exists because rows were decoded, and rows are only decoded
/// where there is a target to write them into ([`BandFilling`]).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DecodedBand {
    source: RowRange,
    placed: RasterBand,
}

impl DecodedBand {
    #[must_use]
    pub fn new(source: RowRange, placed: RasterBand) -> Self {
        Self { source, placed }
    }

    /// The source rows these pixels came from.
    #[must_use]
    pub const fn source(&self) -> RowRange {
        self.source
    }

    /// Where they went, and the pixels that went there.
    #[must_use]
    pub const fn placed(&self) -> &RasterBand {
        &self.placed
    }

    /// Both halves, for a consumer that reports one and writes the other.
    #[must_use]
    pub fn into_parts(self) -> (RowRange, RasterBand) {
        (self.source, self.placed)
    }
}

/// What one turn of a banded source produced.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum BandStep {
    /// The next band, in order, once.
    Band(DecodedBand),
    /// Every source row has been handed out.
    Done,
    /// The decoder failed part-way through. The bands already handed out are
    /// still what they said they were, but this source can never finish: a
    /// caller that wants a whole image must abandon it and decode the source
    /// again on the whole-image path.
    Failed,
}

/// Where an encoded source's rows come from.
///
/// Matched exhaustively wherever a source is consumed: a third route has to be
/// a deliberate addition rather than something a wildcard swallows.
pub(crate) enum BandSource {
    /// Whole source rows, in order, one band at a time.
    Banded(BandedSource),
    /// One all-or-nothing decode of the whole image — this renderer's
    /// behaviour before banding existed. Every format whose decoder has no
    /// row-wise entry point lands here, and so does every source too small for
    /// the decode to be visibly slow.
    Whole,
}

/// The row-wise decoders, one variant per format that has one.
///
/// This variant set is the compile-time answer to "which formats can band?":
/// a format added here fails to build in every match over it until its
/// decoder is written. That property cannot be had from `image::ImageFormat`,
/// which is `#[non_exhaustive]` and so needs a wildcard arm; the format match
/// in [`BandSource::open`] names every format `image` reports and falls back
/// for formats `image` might add, and this enum is where a *banded* addition
/// is caught.
pub(crate) enum BandedSource {
    /// PNG, whose reader yields one transformed row at a time
    /// (`png-0.18.1` `src/decoder/mod.rs:513` `next_row`).
    Png(PngRows),
    /// Baseline JPEG, whose decoder yields one MCU row at a time
    /// (`zune-jpeg` `McuRowReader::next_row` — the fork `Cargo.toml`'s
    /// `[patch.crates-io]` block pins, since `zune-jpeg` 0.5.15's public
    /// surface is `decode`/`decode_into` and nothing else).
    Jpeg(JpegRows),
}

impl BandedSource {
    /// Take the next band, or learn that there are no more.
    pub(crate) fn next_band(&mut self) -> BandStep {
        match self {
            Self::Png(rows) => rows.next_band(),
            Self::Jpeg(rows) => rows.next_band(),
        }
    }

    /// The raster the decode wrote, once every row has been handed out.
    ///
    /// `None` before then, so a caller cannot publish a prefix by accident.
    #[must_use]
    pub(crate) fn into_raster(self) -> Option<RasterPixels> {
        match self {
            Self::Png(rows) => rows.into_raster(),
            Self::Jpeg(rows) => rows.into_raster(),
        }
    }
}

/// A completed banded decode: the source's extent, and the raster it became.
///
/// The pixels are the target's own buffer, so they are at the raster's size and
/// in its coordinates — there is no native-size image anywhere in this
/// structure, which is the point. Which mask the source has is not the raster's
/// answer: that is classified from the source's own alphas as its rows are read
/// (`classify_alpha`), because a filtered raster has alphas the source never
/// had: intermediate ones, and — a kernel with negative lobes being what it
/// is — ones the source's own range cannot hold.
pub(crate) struct RasterPixels {
    native: ImageNativeExtent,
    raster: ImageRasterExtent,
    rgba: Vec<u8>,
    mask: ImageMaskKind,
}

impl RasterPixels {
    /// The extent the source's header named.
    #[must_use]
    pub(crate) const fn native(&self) -> ImageNativeExtent {
        self.native
    }

    /// The raster the decode realized it onto.
    #[must_use]
    pub(crate) const fn raster(&self) -> ImageRasterExtent {
        self.raster
    }

    /// The mask identity of the source's own pixels.
    #[must_use]
    pub(crate) const fn mask(&self) -> ImageMaskKind {
        self.mask
    }

    /// The pixels, at the raster's size, row-major.
    ///
    /// Taking them consumes the value, so the extent and the mask must be read
    /// first — which is the order a caller that is about to upload them wants
    /// anyway.
    #[must_use]
    pub(crate) fn into_rgba(self) -> Vec<u8> {
        self.rgba
    }
}

/// The mask identity of an RGBA buffer, as GNU's `:mask` reads it.
///
/// Continuous alpha is a different fact from a clipping mask, and the
/// distinction is made from the decoded source pixels rather than from
/// interpolated ones: the renderer may store both as RGBA, but storage must not
/// collapse them into one semantic state.
pub(crate) fn classify_alpha(rgba: &[u8]) -> ImageMaskKind {
    let mut has_transparent = false;
    for alpha in rgba.iter().skip(3).step_by(4).copied() {
        match alpha {
            255 => {}
            0 => has_transparent = true,
            _ => return ImageMaskKind::AlphaChannel,
        }
    }
    if has_transparent {
        ImageMaskKind::Clipping
    } else {
        ImageMaskKind::None
    }
}

/// The mask identity of two runs of pixels together.
///
/// The classification is a property of every pixel of the source, and a row-wise
/// decode only ever has one row at a time: this is what it accumulates into the
/// answer the whole-image path reaches in one pass.
#[must_use]
pub(crate) fn merge_mask(left: ImageMaskKind, right: ImageMaskKind) -> ImageMaskKind {
    match (left, right) {
        (ImageMaskKind::AlphaChannel, _) | (_, ImageMaskKind::AlphaChannel) => {
            ImageMaskKind::AlphaChannel
        }
        (ImageMaskKind::Clipping, _) | (_, ImageMaskKind::Clipping) => ImageMaskKind::Clipping,
        (ImageMaskKind::None, ImageMaskKind::None) => ImageMaskKind::None,
    }
}

impl BandSource {
    /// Classify an encoded source and, when banding applies, open its row-wise
    /// decoder.
    ///
    /// `size` and `realization` are the realization the image will be drawn
    /// through, and they only size the bands: how many source rows one display
    /// row is worth. Rotation is deliberately not an input — GNU turns the
    /// image after sizing (`src/image.c:3169-3201`), so the unrotated geometry
    /// is the one whose vertical scale maps source rows onto display rows.
    pub(crate) fn open(
        data: EncodedBytes,
        size: ImageSizeSpec,
        realization: ImageRealization,
    ) -> Self {
        Self::open_at_least(data, size, realization, BANDING_MIN_PIXELS)
    }

    /// The same, with the size threshold forced to zero.
    ///
    /// The threshold is a policy about when banding is *worth it*, not about
    /// what banding does, so the tests that are about what it does read a
    /// small source rather than encode a four-megapixel one.
    #[cfg(test)]
    pub(crate) fn open_forced(
        data: EncodedBytes,
        size: ImageSizeSpec,
        realization: ImageRealization,
    ) -> Self {
        Self::open_at_least(data, size, realization, 0)
    }

    fn open_at_least(
        data: EncodedBytes,
        size: ImageSizeSpec,
        realization: ImageRealization,
        min_pixels: u64,
    ) -> Self {
        match image::guess_format(&data) {
            Ok(image::ImageFormat::Png) => {
                PngRows::open(data, BandPlan::new(size, realization), min_pixels)
                    .map_or(Self::Whole, |rows| Self::Banded(BandedSource::Png(rows)))
            }
            Ok(image::ImageFormat::Jpeg) => {
                JpegRows::open(data, BandPlan::new(size, realization), min_pixels)
                    .map_or(Self::Whole, |rows| Self::Banded(BandedSource::Jpeg(rows)))
            }
            // Every other format `image` reports, named so the route per format
            // is a line of code rather than a default. None of them can band.
            // GIF, WebP, TIFF, BMP, ICO, TGA, DDS, PNM, HDR, OpenEXR,
            // farbfeld, AVIF and QOI are whole-image decoders in `image`, with
            // no row-wise entry point to reach.
            Ok(
                image::ImageFormat::Gif
                | image::ImageFormat::WebP
                | image::ImageFormat::Pnm
                | image::ImageFormat::Tiff
                | image::ImageFormat::Tga
                | image::ImageFormat::Dds
                | image::ImageFormat::Bmp
                | image::ImageFormat::Ico
                | image::ImageFormat::Hdr
                | image::ImageFormat::OpenExr
                | image::ImageFormat::Farbfeld
                | image::ImageFormat::Avif
                | image::ImageFormat::Qoi,
            ) => Self::Whole,
            // `ImageFormat` is `#[non_exhaustive]`, so this arm is mandatory
            // whether or not a format is left to name. Both the formats hid
            // behind it and the sources `image` cannot identify at all (XPM,
            // XBM and SVG, which the caller's own fallbacks decode) route
            // whole, which is the right answer for every one of them.
            Ok(_) | Err(_) => Self::Whole,
        }
    }
}

/// How the bands of one source are sized.
///
/// The realized geometry, kept as the spec it was resolved from: the source's
/// native extent is only known once its header has been read, and the display
/// row a band has to cover is derived from both.
#[derive(Clone, Copy, Debug)]
struct BandPlan {
    size: ImageSizeSpec,
    realization: ImageRealization,
}

impl BandPlan {
    const fn new(size: ImageSizeSpec, realization: ImageRealization) -> Self {
        Self { size, realization }
    }

    /// Rows one band of a `width` x `height` source aims to read.
    ///
    /// Three bounds meet, and the largest floor under the smallest ceiling
    /// wins:
    ///
    /// - A display row's worth of source pixels, because a band is drawn as a
    ///   sub-rect of the image and one that does not cover a display row
    ///   cannot advance it.
    /// - A [`BAND_TARGET_COUNT`]th of the height, because the first band
    ///   should arrive early rather than after most of the decode.
    /// - [`BAND_MAX_BYTES`] of RGBA, because a very wide source would
    ///   otherwise band into tens of megabytes at a time.
    ///
    /// The height floor and the byte ceiling govern in practice: a source
    /// large enough to band is usually shown at or below its own size, where
    /// one display row is a single source row. This is a budget rather than a
    /// rule — a band reads past it rather than fill no raster row at all
    /// (`PngRows::next_band`) — and how many rows it hands over is whichever
    /// raster rows those source rows completed.
    fn band_rows(self, width: u32, height: u32) -> NonZeroU32 {
        let display = self.rows_per_display_row(width, height).get();
        let counted = height.div_ceil(BAND_TARGET_COUNT).max(1);
        let by_bytes = (BAND_MAX_BYTES / (width.max(1) as usize * 4)).max(1);
        let by_bytes = u32::try_from(by_bytes).unwrap_or(u32::MAX);
        let rows = display.max(counted).min(by_bytes).max(1).min(height.max(1));
        NonZeroU32::new(rows).unwrap_or(NonZeroU32::MIN)
    }

    /// The raster a `native`-sized source decodes into: the realization's
    /// raster, clamped to the texture limit.
    ///
    /// Resolved here, from the *header's* extent, through the one function the
    /// finished upload also resolves from — step 1 pins those two extents
    /// equal, and a texture written under one raster and finished under another
    /// would hold rows from two scalings that nothing downstream could tell
    /// apart.
    fn target(self, native: ImageNativeExtent) -> Option<RasterTarget> {
        let geometry = crate::image_cache::realized_geometry(native, self.size, self.realization);
        RasterTarget::new(native, geometry.raster())
    }

    /// Source rows behind one display row at this realization: one at native
    /// size or magnified, more when the source is minified onto fewer rows.
    fn rows_per_display_row(self, width: u32, height: u32) -> NonZeroU32 {
        let layout = self
            .realization
            .resolve_geometry(
                self.size,
                ImageIntrinsicExtent::from(ImageNativeExtent::new(width, height)),
                ImageRotation::None,
            )
            .layout();
        NonZeroU32::new(height.div_ceil(layout.height().max(1))).unwrap_or(NonZeroU32::MIN)
    }
}

#[cfg(test)]
#[path = "image_bands/tests/image_bands_test.rs"]
mod tests;

/// A baseline JPEG's rows, as the decoder hands them over.
///
/// The unit a JPEG decoder can stop at is the MCU row — eight or sixteen
/// *output* rows, depending on the sampling factors — which is coarser than the
/// row budget a band is planned to, and finer than nothing. So a band here is
/// whole MCU rows: the budget decides how many of them one band is, the way it
/// decides how many rows one band of a PNG is, and neither can be finer than
/// its own decoder's unit of work.
type JpegReader = zune_jpeg::McuRowReader<ZuneCursor<EncodedBytes>>;

/// A baseline JPEG being read one MCU row at a time, straight into the raster
/// it will be drawn from.
///
/// Same shape as [`PngRows`] and for the same reason: the struct holds no
/// image. Each source row is expanded to RGBA, resampled into the target's
/// coordinates and filtered into whichever output rows cover it, and what is
/// left at the end is the raster the texture takes.
pub(crate) struct JpegRows {
    /// The row-wise decoder. It carries its own cursor: the bands it hands back
    /// are the next MCU rows and nothing else can be asked of it, which is
    /// where "bands cannot overlap, skip or arrive out of order" comes from
    /// here.
    reader: JpegReader,
    /// The colours the decoder was configured to output, and how a row of them
    /// widens to RGBA.
    format: RowFormat,
    native: ImageNativeExtent,
    /// Rows one band aims to read, before the target's own floor of one raster
    /// row is applied and before MCU rows round it up. The last band of a
    /// source is shorter.
    band: NonZeroU32,
    /// The raster being filled, and the source rows still being filtered.
    target: RasterTarget,
    /// One source row, expanded to RGBA.
    row: Vec<u8>,
    /// The mask identity of the rows read so far.
    mask: ImageMaskKind,
    /// Rows read so far. Every band is derived from this cursor.
    decoded: u32,
    /// Set once the reader has failed, so later calls cannot report progress a
    /// caller might read as a successful continuation.
    failed: bool,
}

impl JpegRows {
    /// Open `data` for row-wise reading, or decline.
    ///
    /// `None` hands the source back to the whole-image path, and means one of:
    /// the headers would not parse, the image is progressive, its components
    /// are split across scans so that no MCU row is final until a later one has
    /// landed, the output is not one of the colour types below, the source is
    /// too small for banding to pay ([`BANDING_MIN_PIXELS`]), or the
    /// realization asks for it *larger* than it is.
    fn open(data: EncodedBytes, plan: BandPlan, min_pixels: u64) -> Option<Self> {
        // The options `image`'s own JPEG decoder configures
        // (`image-0.25.10` `src/codecs/jpeg/decoder.rs:34-38`): non-strict, and
        // no dimension limits. Both paths decoding through them is what makes a
        // banded decode and a whole decode of the same file agree pixel for
        // pixel.
        let options = ZuneOptions::default()
            .set_strict_mode(false)
            .set_max_width(usize::MAX)
            .set_max_height(usize::MAX);
        let mut decoder = ZuneDecoder::new_with_options(ZuneCursor::new(data), options);
        decoder.decode_headers().ok()?;
        let (width, height) = decoder.dimensions()?;
        let (width, height) = (width as u32, height as u32);
        // A progressive image is decoded in several scans, each carrying part
        // of every block, so no prefix of it is final until the last scan has
        // landed: those bands would be wrong pixels rather than early ones.
        // `mcu_rows_available` is the same question plus the split-scan
        // baseline shape, which `is_progressive` alone does not catch.
        if decoder.mcu_rows_available().is_err() {
            return None;
        }
        // The output colour `image` asks this decoder for, which is the input
        // colour where it is one of the four it can pass through and RGB
        // otherwise (`src/codecs/jpeg/decoder.rs:56-69`).
        let requested = match decoder.input_colorspace()? {
            space @ (ZuneColorSpace::RGB
            | ZuneColorSpace::RGBA
            | ZuneColorSpace::Luma
            | ZuneColorSpace::LumaA) => space,
            _ => ZuneColorSpace::RGB,
        };
        decoder.set_options(decoder.options().jpeg_set_out_colorspace(requested));
        let reader = zune_jpeg::McuRowReader::new(decoder).ok()?;
        // What the decoder actually produced, which is what a row's bytes mean.
        let format = RowFormat::of_jpeg(reader.color_space())?;
        if u64::from(width) * u64::from(height) < min_pixels {
            return None;
        }
        let native = ImageNativeExtent::new(width, height);
        Some(Self {
            target: plan.target(native)?,
            row: vec![0; width as usize * 4],
            native,
            band: plan.band_rows(width, height),
            format,
            mask: ImageMaskKind::None,
            reader,
            decoded: 0,
            failed: false,
        })
    }

    /// Read the next band's worth of rows into the raster.
    ///
    /// A band is at least one MCU row and at least enough rows to fill a raster
    /// row, so it always has a placement: stopping while the target had nowhere
    /// to write would publish a band with no destination, which is exactly the
    /// state [`DecodedBand`] exists to make unrepresentable. The last band of a
    /// source is whatever is left of it.
    fn next_band(&mut self) -> BandStep {
        if self.failed {
            return BandStep::Failed;
        }
        let height = self.native.height();
        if self.decoded >= height {
            return BandStep::Done;
        }
        let start = self.decoded;
        let built = self.target.built();
        let budget = self.band.get().min(height - start);
        loop {
            if self.decoded >= height {
                break;
            }
            if self.decoded > start && self.decoded - start >= budget && self.target.built() > built
            {
                break;
            }
            if self.read_mcu_row().is_err() {
                // The decoder ran out before the header's height did, or would
                // not read a stream the headers promised it could. Either way
                // this source can never finish, and the caller must decode the
                // image whole rather than draw a prefix of it.
                self.failed = true;
                return BandStep::Failed;
            }
        }
        let source = RowRange::new(
            start,
            NonZeroU32::new(self.decoded - start).unwrap_or(NonZeroU32::MIN),
        );
        match self.target.band_since(built) {
            Some(placed) => BandStep::Band(DecodedBand::new(source, placed)),
            // Unreachable for the same reason it is in `PngRows::next_band`:
            // the loop stops either with a raster row built or at the end of
            // the source, and the source's last row completes the raster's last
            // row. Handled as a failure rather than asserted because the
            // alternative — a band that fills nothing — is a state the caller
            // cannot represent.
            None => {
                self.failed = true;
                BandStep::Failed
            }
        }
    }

    /// Read one MCU row — several source rows — into the raster.
    fn read_mcu_row(&mut self) -> Result<(), ()> {
        // A borrow of one field, so the rest of `self` stays usable inside the
        // loop below. The band borrows the reader, which is what stops a later
        // MCU row from being asked for before this one has been written out.
        let reader = &mut self.reader;
        let Some(band) = reader.next_row().map_err(|_| ())? else {
            return Err(());
        };
        let stride = band.row_bytes();
        for source in band.pixels().chunks_exact(stride) {
            self.format.expand_row(source, &mut self.row).ok_or(())?;
            // The mask is a property of the source's own pixels, so it is read
            // from them rather than from the raster they are filtered into.
            if self.format.may_be_transparent() {
                self.mask = merge_mask(self.mask, classify_alpha(&self.row));
            }
            self.target.push_row(&self.row);
            self.decoded += 1;
        }
        Ok(())
    }

    fn into_raster(self) -> Option<RasterPixels> {
        if self.failed || self.decoded != self.native.height() {
            return None;
        }
        let raster = self.target.raster();
        Some(RasterPixels {
            native: self.native,
            raster,
            rgba: self.target.into_pixels()?,
            mask: self.mask,
        })
    }
}

/// A PNG being read one row at a time, straight into the raster it will be
/// drawn from.
///
/// The struct holds no image: one row of RGBA is expanded at a time, it is
/// resampled into the target's coordinates, and it is filtered into whichever
/// output rows cover it. What is left of the source afterwards is the raster,
/// which is what the texture takes — so the native-size buffer this path used
/// to build, and the resample of it that followed, are both gone.
pub(crate) struct PngRows {
    reader: png::Reader<Cursor<EncodedBytes>>,
    format: RowFormat,
    native: ImageNativeExtent,
    /// Rows one band carries, before the target's own floor of one raster row
    /// is applied. The last band of a source is shorter.
    band: NonZeroU32,
    /// The raster being filled, and the source rows still being filtered.
    target: RasterTarget,
    /// One source row, expanded to RGBA.
    row: Vec<u8>,
    /// The mask identity of the rows read so far.
    mask: ImageMaskKind,
    /// Rows read so far. Every band is derived from this cursor.
    decoded: u32,
    /// Set once the reader has failed, so later calls cannot report progress a
    /// caller might read as a successful continuation.
    failed: bool,
}

impl PngRows {
    /// Open `data` for row-wise reading, or decline.
    ///
    /// `None` hands the source back to the whole-image path, and means one of:
    /// the header would not parse, the output is not one of the colour types the
    /// format table names, the source is interlaced, it is too small for banding
    /// to pay ([`BANDING_MIN_PIXELS`]), or the realization asks for it *larger*
    /// than it is, which is the one shape a filter that only ever reduces cannot
    /// take.
    fn open(data: EncodedBytes, plan: BandPlan, min_pixels: u64) -> Option<Self> {
        let mut decoder = png::Decoder::new(Cursor::new(data));
        // The transform `image`'s own PNG decoder sets before reading
        // (`image-0.25.10` `src/codecs/png.rs:60`). Both paths decoding through
        // it is what makes a banded decode and a whole decode of the same file
        // agree pixel for pixel. `EXPAND` is also what leaves a 16-bit row
        // 16-bit: `STRIP_16` would truncate it to 8, which is a different
        // picture from the one the whole-image path decodes.
        decoder.set_transformations(png::Transformations::EXPAND);
        let reader = decoder.read_info().ok()?;
        let (width, height) = (reader.info().width, reader.info().height);
        // Adam7 rows are partial rows, so an interlaced source is read whole
        // rather than guessed at — see
        // `an_interlaced_png_declines_banding` for why that is the ordering
        // and not the privacy of the pass geometry.
        if reader.info().interlaced {
            return None;
        }
        let format = RowFormat::of_png(reader.output_color_type())?;
        // What the header says a row is, against what the format says a row
        // is. The two agreeing is what lets the row checks below be a length
        // check rather than a parse, and asking here is what keeps a
        // disagreement a decline rather than a mid-stream failure after bands
        // have already been published.
        if reader.output_line_size(width) != Some(format.row_bytes(width)) {
            return None;
        }
        if u64::from(width) * u64::from(height) < min_pixels {
            return None;
        }
        let native = ImageNativeExtent::new(width, height);
        Some(Self {
            target: plan.target(native)?,
            row: vec![0; width as usize * 4],
            native,
            band: plan.band_rows(width, height),
            format,
            mask: ImageMaskKind::None,
            reader,
            decoded: 0,
            failed: false,
        })
    }

    /// Read the next band's worth of rows into the raster.
    ///
    /// A band is a run of source rows that fills at least one raster row: the
    /// row budget is what says how often a decode should report progress, but
    /// stopping while the target had nowhere to write would publish a band with
    /// no placement, which is exactly the state [`DecodedBand`] exists to make
    /// unrepresentable. A source minified hard enough for one raster row to
    /// span many source rows therefore reads that many rows into one band,
    /// which is also the band the display side wants: it covers a row of the
    /// picture.
    fn next_band(&mut self) -> BandStep {
        if self.failed {
            return BandStep::Failed;
        }
        let height = self.native.height();
        if self.decoded >= height {
            return BandStep::Done;
        }
        let start = self.decoded;
        let built = self.target.built();
        let budget = self.band.get().min(height - start);
        loop {
            if self.read_row().is_err() {
                self.failed = true;
                return BandStep::Failed;
            }
            self.decoded += 1;
            if self.decoded >= height {
                break;
            }
            if self.decoded - start >= budget && self.target.built() > built {
                break;
            }
        }
        let source = RowRange::new(
            start,
            NonZeroU32::new(self.decoded - start).unwrap_or(NonZeroU32::MIN),
        );
        match self.target.band_since(built) {
            Some(placed) => BandStep::Band(DecodedBand::new(source, placed)),
            // Unreachable, and handled as a failure rather than asserted
            // because the alternative — a band that fills nothing — is a state
            // the caller cannot represent. The loop above stops either with a
            // raster row built or at the end of the source, and the source's
            // last row completes the raster's last row.
            None => {
                self.failed = true;
                BandStep::Failed
            }
        }
    }

    /// Read one source row, resample it, and average it into the raster.
    fn read_row(&mut self) -> Result<(), ()> {
        let Self {
            reader,
            format,
            target,
            row,
            mask,
            ..
        } = self;
        let format = *format;
        let Some(source) = reader.next_row().map_err(|_| ())? else {
            // The reader ended before the header's height did: a truncated
            // stream, which is exactly the mid-stream failure the caller has to
            // be able to recover from.
            return Err(());
        };
        format.expand_row(source.data(), row).ok_or(())?;
        // The mask is a property of the source's own pixels, so it is read from
        // them rather than from the raster they are filtered into: the filter
        // gives a source with only opaque and clear pixels alphas in between,
        // and asking the raster which mask it has would be asking the filter.
        if format.may_be_transparent() {
            *mask = merge_mask(*mask, classify_alpha(row));
        }
        target.push_row(row);
        Ok(())
    }

    fn into_raster(self) -> Option<RasterPixels> {
        if self.failed || self.decoded != self.native.height() {
            return None;
        }
        let raster = self.target.raster();
        Some(RasterPixels {
            native: self.native,
            raster,
            rgba: self.target.into_pixels()?,
            mask: self.mask,
        })
    }
}

/// The colour types a source row can arrive in, as two orthogonal facts: which
/// samples a pixel has, and how wide each sample is.
///
/// Two tables rather than one variant per combination, because a variant per
/// combination is a table a combination can fall off the end of without
/// anything failing to build — which is how 16-bit PNG output went unbanded.
/// The old table named only the `BitDepth::Eight` pairs and swallowed the rest
/// with a wildcard, so the whole depth quietly took the whole-image path and no
/// test said why. Both tables below are matched without a wildcard, so a colour
/// type or a depth `png` grows is a build error here.
///
/// Both row-wise decoders arrive at these, which is not a coincidence: each is
/// what `image`'s own decoder for that format hands to `to_rgba8`, and
/// widening to RGBA is where a banded decode and a whole one meet. For PNG,
/// `Transformations::EXPAND` lifts grayscale below 8 bits and palettes (with or
/// without `tRNS`) into 8-bit rows and leaves 16-bit output 16-bit. For JPEG
/// these are the colours `image` asks `zune-jpeg` for, one per input colour
/// space it can map, and always one byte wide.
#[derive(Clone, Copy, Debug)]
struct RowFormat {
    layout: Layout,
    sample: SampleWidth,
}

/// Which samples a pixel has, in the order the row stores them.
#[derive(Clone, Copy, Debug)]
enum Layout {
    Gray,
    GrayAlpha,
    Rgb,
    Rgba,
}

/// How wide one sample of a row is.
#[derive(Clone, Copy, Debug)]
enum SampleWidth {
    /// One byte, which is already the 8-bit value `image`'s `to_rgba8` wants.
    One,
    /// Two bytes, most significant first. PNG stores its samples big-endian
    /// and `EXPAND` does not reorder them — `SWAP_ENDIAN` would, and
    /// `png-0.18.1` never reads that flag, which is why a probe row of
    /// `0x1234` comes back as `[0x12, 0x34]`. `image`'s own decoder reorders
    /// each pair to native order as it reads the frame
    /// (`image-0.25.10` `src/codecs/png.rs:251-263`), and this reads the pair
    /// big-endian, which is that same reorder without the copy.
    Two,
}

impl Layout {
    /// How many samples one pixel of this layout has.
    const fn channels(self) -> usize {
        match self {
            Self::Gray => 1,
            Self::GrayAlpha => 2,
            Self::Rgb => 3,
            Self::Rgba => 4,
        }
    }

    /// Whether this layout carries an alpha sample of its own.
    const fn has_alpha(self) -> bool {
        matches!(self, Self::GrayAlpha | Self::Rgba)
    }
}

impl SampleWidth {
    /// How many bytes one sample of this width occupies.
    const fn bytes(self) -> usize {
        match self {
            Self::One => 1,
            Self::Two => 2,
        }
    }

    /// Sample `channel` of one pixel, as the byte `image`'s `to_rgba8` reduces
    /// it to.
    ///
    /// A 16-bit sample is *not* its high byte. `image` scales it onto the
    /// 8-bit range by rounding — `(v + 128) / 257`, which is what its
    /// `FromPrimitive<u16> for u8` computes and what its `f32` path computes
    /// to the same values — so `0x00ff` is 1 and `0xff00` is 254, where taking
    /// the high byte would give 0 and 255. Every value of every channel of all
    /// four 16-bit colour types was checked against the whole-image path when
    /// this went in; `0x1234`, the value a probe row naturally reaches for, is
    /// one of the values that cannot tell the two apart.
    fn sample(self, pixel: &[u8], channel: usize) -> u8 {
        match self {
            Self::One => pixel[channel],
            Self::Two => {
                let bytes = [pixel[2 * channel], pixel[2 * channel + 1]];
                ((u32::from(u16::from_be_bytes(bytes)) + 128) / 257) as u8
            }
        }
    }
}

impl RowFormat {
    fn of_png((color, depth): (png::ColorType, png::BitDepth)) -> Option<Self> {
        let layout = match color {
            png::ColorType::Grayscale => Layout::Gray,
            png::ColorType::GrayscaleAlpha => Layout::GrayAlpha,
            png::ColorType::Rgb => Layout::Rgb,
            png::ColorType::Rgba => Layout::Rgba,
            // `EXPAND` resolves a palette — with or without `tRNS` — into one
            // of the four above before `output_color_type` reports it, so an
            // indexed row here is one this path has no row for.
            png::ColorType::Indexed => return None,
        };
        let sample = match depth {
            // `EXPAND` lifts every depth below eight to eight, so these are
            // depths it did not lift rather than depths a row arrives in.
            png::BitDepth::One | png::BitDepth::Two | png::BitDepth::Four => return None,
            png::BitDepth::Eight => SampleWidth::One,
            png::BitDepth::Sixteen => SampleWidth::Two,
        };
        Some(Self { layout, sample })
    }

    /// The colour a JPEG decoder was configured to output.
    ///
    /// The decoder's own answer and not the request: a colour space it cannot
    /// produce for this image is one this path declines rather than reads as
    /// the wrong one. Unlike the PNG table this one keeps a wildcard, and can:
    /// what it matches is what this crate asked the decoder for, so anything
    /// else is a decline rather than a case no one thought of.
    fn of_jpeg(space: ZuneColorSpace) -> Option<Self> {
        let layout = match space {
            ZuneColorSpace::Luma => Layout::Gray,
            ZuneColorSpace::LumaA => Layout::GrayAlpha,
            ZuneColorSpace::RGB => Layout::Rgb,
            ZuneColorSpace::RGBA => Layout::Rgba,
            _ => return None,
        };
        Some(Self {
            layout,
            sample: SampleWidth::One,
        })
    }

    /// How long a row of this format is, in bytes, for `width` pixels.
    fn row_bytes(self, width: u32) -> usize {
        width as usize * self.layout.channels() * self.sample.bytes()
    }

    /// Whether this colour type carries an alpha channel at all.
    ///
    /// The layouts without one expand to alpha 255 by construction, so their
    /// rows cannot change the mask and are not scanned for it.
    fn may_be_transparent(self) -> bool {
        self.layout.has_alpha()
    }

    /// Widen one row to RGBA, the conversion `image`'s `to_rgba8` applies to
    /// the same colour type. `None` means the row was not the length the
    /// colour type promised.
    fn expand_row(self, source: &[u8], target: &mut [u8]) -> Option<()> {
        let channels = self.layout.channels();
        let pixel_bytes = channels * self.sample.bytes();
        if source.len() != target.len() / 4 * pixel_bytes {
            return None;
        }
        // The one widening that is a copy rather than a widening, and the shape
        // most sources arrive in.
        if matches!((self.layout, self.sample), (Layout::Rgba, SampleWidth::One)) {
            target.copy_from_slice(source);
            return Some(());
        }
        // Otherwise one loop rather than one per colour type: the layout says
        // which samples of a pixel land in which channels, and the sample width
        // says only how wide each one is on the wire.
        for (texel, pixel) in target
            .chunks_exact_mut(4)
            .zip(source.chunks_exact(pixel_bytes))
        {
            let sample = |channel: usize| self.sample.sample(pixel, channel);
            match self.layout {
                Layout::Rgba => {
                    texel.copy_from_slice(&[sample(0), sample(1), sample(2), sample(3)])
                }
                Layout::Rgb => {
                    texel.copy_from_slice(&[sample(0), sample(1), sample(2), 0xff]);
                }
                Layout::GrayAlpha => {
                    let luma = sample(0);
                    texel.copy_from_slice(&[luma, luma, luma, sample(1)]);
                }
                Layout::Gray => {
                    let luma = sample(0);
                    texel.copy_from_slice(&[luma, luma, luma, 0xff]);
                }
            }
        }
        Some(())
    }
}
