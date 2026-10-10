//! Immutable image geometry shared by layout, async decoding, and rendering.

use crate::types::{ImageId, ImageLoadToken};
use std::collections::HashSet;
use std::marker::PhantomData;
use std::sync::Arc;

/// One renderer-side image lifecycle fact.
///
/// A decode completion belongs to one particular load attempt, while eviction
/// belongs to the stable image identity. Encoding that distinction
/// in the variants makes stale completion handling mandatory and prevents an
/// impossible `DecodeCompleted` event without an attempt token.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageStateEvent {
    DecodeCompleted(ImageLoadToken),
    Evicted(ImageId),
}

impl ImageStateEvent {
    #[must_use]
    pub const fn image(self) -> ImageId {
        match self {
            Self::DecodeCompleted(load) => load.image(),
            Self::Evicted(image) => image,
        }
    }
}

/// Image identities retained by accepted or queued render presentations.
///
/// The set is a lifetime fence, not a cache-lookup result: a logically
/// invalidated image remains here until every frame that can draw it has been
/// replaced or discarded. Renderer eviction and physical release must exclude
/// every member.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RetainedImageSet(HashSet<ImageId>);

impl RetainedImageSet {
    #[must_use]
    pub fn contains(&self, image: ImageId) -> bool {
        self.0.contains(&image)
    }

    pub fn insert(&mut self, image: ImageId) {
        self.0.insert(image);
    }

    pub fn extend(&mut self, images: impl IntoIterator<Item = ImageId>) {
        self.0.extend(images);
    }

    pub fn iter(&self) -> impl Iterator<Item = ImageId> + '_ {
        self.0.iter().copied()
    }
}

impl FromIterator<ImageId> for RetainedImageSet {
    fn from_iter<T: IntoIterator<Item = ImageId>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}

/// The encoded bytes of one image source, handed around by handle.
///
/// One buffer, named by the Lisp side and read by the renderer, so the two are
/// the same bytes rather than two copies of them: a request's identity holds a
/// handle to what the decode job reads, and every hop between them — the load
/// command, the decode queue, the row-wise decoders, the whole-image fallback
/// an abandoned banded attempt takes — clones the handle instead of the image.
///
/// A handle to the [`Vec`] rather than an `Arc<[u8]>`, which is the tidier type
/// and is not reachable from an owned buffer without the copy this exists to
/// avoid: a slice's refcount header has to sit in front of its data, so
/// `Arc::from(vec)` reallocates and copies the whole buffer, while moving a
/// `Vec` into an `Arc` moves three words and leaves the bytes where they are.
///
/// Equality and hashing are the bytes': two handles to the same contents are
/// one source, which is what a request keyed by them means.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct EncodedBytes(Arc<Vec<u8>>);

impl EncodedBytes {
    /// Take ownership of the bytes, without copying them.
    #[must_use]
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(Arc::new(bytes))
    }

    /// A handle to a copy of `bytes`.
    ///
    /// For a caller that has a slice and no buffer to hand over: it is a full
    /// copy of the source, so a path that owns its bytes uses [`Self::new`].
    #[must_use]
    pub fn copy_of(bytes: &[u8]) -> Self {
        Self::new(bytes.to_vec())
    }

    /// How many bytes the source is.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether the source is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The bytes.
    #[must_use]
    pub fn as_slice(&self) -> &[u8] {
        self.0.as_slice()
    }
}

impl AsRef<[u8]> for EncodedBytes {
    /// The bytes, for a reader that is given a buffer to read from.
    fn as_ref(&self) -> &[u8] {
        self.0.as_slice()
    }
}

impl std::ops::Deref for EncodedBytes {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        self.0.as_slice()
    }
}

/// A normalized, non-empty rectangle sampled from an image texture.
///
/// Layout resolves GNU's pixel/fraction `(slice …)` operands once it knows the
/// realized image size. Rendering receives only this closed normalized form,
/// so it cannot reinterpret Lisp values or confuse crop geometry with window
/// clipping geometry.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ImageSourceRect {
    x_start: u16,
    y_start: u16,
    x_end: u16,
    y_end: u16,
}

impl ImageSourceRect {
    const NORMALIZED_MAX: f32 = u16::MAX as f32;

    pub const FULL: Self = Self {
        x_start: 0,
        y_start: 0,
        x_end: u16::MAX,
        y_end: u16::MAX,
    };

    #[must_use]
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Option<Self> {
        let values = [x, y, width, height];
        if values.iter().any(|value| !value.is_finite())
            || x < 0.0
            || y < 0.0
            || width <= 0.0
            || height <= 0.0
            || x + width > 1.0 + f32::EPSILON
            || y + height > 1.0 + f32::EPSILON
        {
            return None;
        }
        let encode = |value: f32| (value.clamp(0.0, 1.0) * Self::NORMALIZED_MAX).round() as u16;
        let x_start = encode(x);
        let y_start = encode(y);
        let x_end = encode((x + width).min(1.0));
        let y_end = encode((y + height).min(1.0));
        (x_end > x_start && y_end > y_start).then_some(Self {
            x_start,
            y_start,
            x_end,
            y_end,
        })
    }

    #[must_use]
    pub fn x(self) -> f32 {
        f32::from(self.x_start) / Self::NORMALIZED_MAX
    }

    #[must_use]
    pub fn y(self) -> f32 {
        f32::from(self.y_start) / Self::NORMALIZED_MAX
    }

    #[must_use]
    pub fn width(self) -> f32 {
        f32::from(self.x_end - self.x_start) / Self::NORMALIZED_MAX
    }

    #[must_use]
    pub fn height(self) -> f32 {
        f32::from(self.y_end - self.y_start) / Self::NORMALIZED_MAX
    }

    /// Map UV coordinates produced by frame/window clipping into this image
    /// sample. Keeping this composition beside the crop type prevents the GPU
    /// path from accidentally treating a clipped slice as a full texture.
    #[must_use]
    pub fn map_uv(self, u: f32, v: f32) -> (f32, f32) {
        (self.x() + u * self.width(), self.y() + v * self.height())
    }

    /// The sub-rect left when the right `1 - fraction` of this rect's x extent
    /// is cropped away.
    ///
    /// GNU crops an inline image glyph that would run past the row's right
    /// edge by removing the same pixels from its layout advance and from its
    /// source slice (`it->pixel_width -= crop; slice.width -= crop;`,
    /// `produce_image_glyph`, src/xdisp.c:32506-32507), so the glyph shows the
    /// left part of the image instead of squeezing all of it into the narrower
    /// box.  This is that crop in the slice's own normalized coordinates.
    ///
    /// `None` when `fraction` leaves nothing to sample — including the case
    /// where the remaining extent rounds to zero width in the `u16` encoding,
    /// which is also how GNU reaches `slice.width == 0`.
    #[must_use]
    pub fn crop_right_to_fraction(self, fraction: f32) -> Option<Self> {
        if !fraction.is_finite() || fraction <= 0.0 {
            return None;
        }
        if fraction >= 1.0 {
            return Some(self);
        }
        Self::new(self.x(), self.y(), self.width() * fraction, self.height())
    }
}

/// The horizontal row advance an inline image glyph keeps after GNU's
/// right-edge crop.
///
/// The cropped advance is deliberately not an `ImageSourceRect` or a raw
/// pixel count: it is the one number `produce_image_glyph` subtracts the crop
/// from, and pairing it with the *uncropped* source slice would stretch the
/// whole image into the narrower box.  [`ImageSourceRect::crop_right_to_fraction`]
/// is the other half of the same edit.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ImageLayoutAdvance(crate::types::Px);

impl ImageLayoutAdvance {
    /// A finite, strictly positive glyph advance.  A crop that leaves nothing
    /// produces no glyph in this port, matching the row writer's rule for
    /// ordinary glyphs.
    #[must_use]
    pub fn new(px: crate::types::Px) -> Option<Self> {
        (px.get().is_finite() && px.get() > 0.0).then_some(Self(px))
    }

    #[must_use]
    pub const fn px(self) -> crate::types::Px {
        self.0
    }
}

/// GNU image margins are non-negative integer pixels. Store them in that
/// domain instead of two `f32`s so adding crop state does not enlarge every
/// glyph in every text row.
#[derive(
    Clone, Copy, Debug, Default, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize,
)]
pub struct ImageMargins {
    left: u16,
    right: u16,
    top: u16,
    bottom: u16,
}

impl ImageMargins {
    #[must_use]
    pub fn new(horizontal: f32, vertical: f32) -> Self {
        Self::asymmetric(horizontal, horizontal, vertical, vertical)
    }

    #[must_use]
    pub fn asymmetric(left: f32, right: f32, top: f32, bottom: f32) -> Self {
        let encode = |value: f32| {
            if value.is_finite() {
                value.round().clamp(0.0, f32::from(u16::MAX)) as u16
            } else {
                0
            }
        };
        Self {
            left: encode(left),
            right: encode(right),
            top: encode(top),
            bottom: encode(bottom),
        }
    }

    #[must_use]
    pub const fn left(self) -> f32 {
        self.left as f32
    }

    #[must_use]
    pub const fn right(self) -> f32 {
        self.right as f32
    }

    #[must_use]
    pub const fn top(self) -> f32 {
        self.top as f32
    }

    #[must_use]
    pub const fn bottom(self) -> f32 {
        self.bottom as f32
    }

    #[must_use]
    pub const fn packed(self) -> u64 {
        (self.left as u64)
            | ((self.right as u64) << 16)
            | ((self.top as u64) << 32)
            | ((self.bottom as u64) << 48)
    }
}

/// Optional opaque image background packed into one word. Decoded image
/// backgrounds are RGB24, leaving `0x01000000` as an impossible sentinel.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ImageOpaqueBackground(u32);

impl ImageOpaqueBackground {
    const NONE: u32 = 0x0100_0000;

    #[must_use]
    pub const fn new(color: Option<u32>) -> Self {
        match color {
            Some(color) => Self(color & 0x00ff_ffff),
            None => Self(Self::NONE),
        }
    }

    #[must_use]
    pub const fn get(self) -> Option<u32> {
        if self.0 == Self::NONE {
            None
        } else {
            Some(self.0)
        }
    }

    #[must_use]
    pub const fn packed(self) -> u32 {
        self.0
    }
}

impl Default for ImageOpaqueBackground {
    fn default() -> Self {
        Self::new(None)
    }
}

impl Default for ImageSourceRect {
    fn default() -> Self {
        Self::FULL
    }
}

/// One opaque sRGB color used while materializing a face-sensitive image.
///
/// GNU image colors are frame pixel values, but only their RGB24 payload is
/// meaningful to the cross-platform decoders.  Keeping that invariant in a
/// type makes black (`0x000000`) a real color instead of an accidental
/// "unspecified" sentinel.
#[derive(
    Clone, Copy, Debug, Default, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize,
)]
pub struct ImageRgb(u32);

impl ImageRgb {
    #[must_use]
    pub const fn from_pixel(pixel: u32) -> Self {
        Self(pixel & 0x00ff_ffff)
    }

    #[must_use]
    pub const fn rgb24(self) -> u32 {
        self.0
    }

    /// The three channels, for the consumers GNU treats as opaque -- XPM
    /// paints an unresolvable color key with the frame foreground pixel and no
    /// alpha of its own (`src/image.c:6518-6538`), so no alpha travels here.
    #[must_use]
    pub const fn rgb8(self) -> [u8; 3] {
        [
            ((self.0 >> 16) & 0xff) as u8,
            ((self.0 >> 8) & 0xff) as u8,
            (self.0 & 0xff) as u8,
        ]
    }

    #[must_use]
    pub const fn rgba8(self) -> [u8; 4] {
        let [red, green, blue] = self.rgb8();
        [red, green, blue, 0xff]
    }
}

/// Resolved face colors that participate in image identity and decoding.
///
/// This is one value throughout layout, the evaluator-owned image catalog,
/// the render command, and the decoder.  Consequently a decoder cannot accept
/// an unlabelled `(u32, u32)` pair or mistake valid black for a missing color.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(from = "ImageColorContextWire")]
pub struct ImageColorContext {
    foreground: ImageRgb,
    background: ImageRgb,
    /// What a decoder paints a pixel whose key resolves to no color with: the
    /// frame's foreground, which GNU keeps equal to the `default` face's
    /// foreground (read as `FRAME_FOREGROUND_PIXEL` at src/image.c:6518 and
    /// used at :6537-6538; the frame-parameter sync is src/xfaces.c:4394-4404).
    ///
    /// Deliberately apart from `foreground`: an image is *displayed under* a
    /// face and may carry a `:foreground` of its own, and GNU reads neither for
    /// this (issue #550).
    frame_foreground: ImageRgb,
    background_policy: ImageBackgroundPolicy,
}

/// Deserialization mirror of [`ImageColorContext`].
///
/// An absent `frame_foreground` reads as the face foreground -- the default
/// [`ImageColorContext::from_pixels`] documents -- never as black, which is a
/// real color here and not an "unset" (issue #556).
#[derive(serde::Deserialize)]
struct ImageColorContextWire {
    foreground: ImageRgb,
    background: ImageRgb,
    #[serde(default)]
    frame_foreground: Option<ImageRgb>,
    #[serde(default)]
    background_policy: ImageBackgroundPolicy,
}

impl From<ImageColorContextWire> for ImageColorContext {
    fn from(wire: ImageColorContextWire) -> Self {
        Self {
            foreground: wire.foreground,
            background: wire.background,
            frame_foreground: wire.frame_foreground.unwrap_or(wire.foreground),
            background_policy: wire.background_policy,
        }
    }
}

/// Editor images use GNU's face-colored wrapper; chrome icons preserve alpha
/// so the toolbar and hover/pressed backgrounds remain visible behind them.
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    Eq,
    Hash,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    strum::EnumIter,
)]
pub enum ImageBackgroundPolicy {
    #[default]
    FaceColor,
    Transparent,
}

impl ImageColorContext {
    /// The face's colors, with the frame foreground defaulted to the face's own:
    /// callers that know the frame replace it with
    /// [`Self::with_frame_foreground`].
    #[must_use]
    pub const fn from_pixels(foreground: u32, background: u32) -> Self {
        Self {
            foreground: ImageRgb::from_pixel(foreground),
            background: ImageRgb::from_pixel(background),
            frame_foreground: ImageRgb::from_pixel(foreground),
            background_policy: ImageBackgroundPolicy::FaceColor,
        }
    }

    #[must_use]
    pub const fn foreground(self) -> ImageRgb {
        self.foreground
    }

    #[must_use]
    pub const fn background(self) -> ImageRgb {
        self.background
    }

    /// GNU's `FRAME_FOREGROUND_PIXEL` for the frame this image belongs to.
    #[must_use]
    pub const fn frame_foreground(self) -> ImageRgb {
        self.frame_foreground
    }

    #[must_use]
    pub const fn with_frame_foreground(mut self, frame_foreground: u32) -> Self {
        self.frame_foreground = ImageRgb::from_pixel(frame_foreground);
        self
    }

    #[must_use]
    pub const fn with_background_policy(mut self, policy: ImageBackgroundPolicy) -> Self {
        self.background_policy = policy;
        self
    }

    pub const fn background_policy(self) -> ImageBackgroundPolicy {
        self.background_policy
    }

    pub const fn background_rgba8(self) -> [u8; 4] {
        match self.background_policy {
            ImageBackgroundPolicy::FaceColor => self.background.rgba8(),
            ImageBackgroundPolicy::Transparent => [0; 4],
        }
    }
}

impl Default for ImageColorContext {
    /// Preserve the renderer's historical visible fallback for callers that
    /// have no Emacs face, such as raw pixels. Redisplay and toolbar image
    /// requests carry their resolved face colors explicitly.
    fn default() -> Self {
        Self::from_pixels(0x00ff_ffff, 0x0000_0000)
    }
}

/// How GNU image postprocessing should derive or remove a clipping mask.
///
/// This is part of image realization identity and travels with the load. The
/// decoder therefore cannot accidentally render `:mask nil` or a heuristic
/// mask as if the property had been absent.
#[derive(
    Clone, Copy, Debug, Default, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize,
)]
pub enum ImageMaskPolicy {
    /// Preserve transparency supplied by the image format.
    #[default]
    Preserve,
    /// GNU `:mask nil`: remove format-supplied clipping transparency.
    Suppress,
    /// GNU `:mask heuristic` / `:heuristic-mask`: chroma-key a background.
    Heuristic(ImageHeuristicMask),
}

/// Background selection for GNU's heuristic clipping-mask construction.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ImageHeuristicMask {
    /// Select the most frequent RGB value among the four corners.
    FourCorners,
    /// Explicit GNU 16-bit RGB components.
    Rgb16([u16; 3]),
}

/// Transparency representation produced by image decoding and postprocessing.
///
/// A clipping mask and continuous alpha can render through the same RGBA GPU
/// texture, but they are observably different to GNU's `image-mask-p` API.
#[derive(
    Clone, Copy, Debug, Default, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize,
)]
pub enum ImageMaskKind {
    #[default]
    None,
    Clipping,
    AlphaChannel,
}

impl ImageMaskKind {
    #[must_use]
    pub const fn has_clipping_mask(self) -> bool {
        matches!(self, Self::Clipping)
    }
}

/// Zero-based frame selected from a multi-image source.
///
/// Keeping this distinct from dimensions and renderer IDs makes it impossible
/// to accidentally route GNU's `:index` into an unrelated integer field.
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
    serde::Serialize,
    serde::Deserialize,
)]
pub struct ImageFrameIndex(u64);

impl ImageFrameIndex {
    #[must_use]
    pub const fn new(index: u64) -> Self {
        Self(index)
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }

    #[must_use]
    pub const fn is_first(self) -> bool {
        self.0 == 0
    }
}

/// Stable identity of one encoded multi-frame source.
///
/// This is intentionally independent of [`ImageId`]: each animation frame has
/// its own texture identity, while all frames share one decoder/compositor
/// cache entry.
#[derive(
    Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize,
)]
pub struct ImageSequenceId(std::num::NonZeroU64);

impl ImageSequenceId {
    #[must_use]
    pub const fn new(id: u64) -> Option<Self> {
        match std::num::NonZeroU64::new(id) {
            Some(id) => Some(Self(id)),
            None => None,
        }
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

/// Generation-qualified invalidation for the decoder-side sequence cache.
///
/// `AllocatedThrough` is a high-water fence: decoder jobs already in flight
/// for any older identity may finish for their caller, but cannot repopulate
/// the cache after a global clear.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ImageSequenceRetirement {
    One(ImageSequenceId),
    AllocatedThrough(ImageSequenceId),
}

/// Exact renderer-owned storage currently retained by the image subsystem.
///
/// Texture and decoded-sequence bytes have different lifetimes, so they stay
/// separate until the Lisp compatibility boundary asks for GNU's one-number
/// `image-cache-size` result.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ImageCacheUsage {
    texture_bytes: u64,
    decoded_sequence_bytes: u64,
}

impl ImageCacheUsage {
    #[must_use]
    pub const fn new(texture_bytes: u64, decoded_sequence_bytes: u64) -> Self {
        Self {
            texture_bytes,
            decoded_sequence_bytes,
        }
    }

    #[must_use]
    pub const fn texture_bytes(self) -> u64 {
        self.texture_bytes
    }

    #[must_use]
    pub const fn decoded_sequence_bytes(self) -> u64 {
        self.decoded_sequence_bytes
    }

    #[must_use]
    pub const fn total_bytes(self) -> u64 {
        self.texture_bytes
            .saturating_add(self.decoded_sequence_bytes)
    }
}

/// Whether the renderer may materialize animation a source computes itself.
///
/// GNU rasterizes an SVG through librsvg, which renders one static frame and
/// has no document clock, so GNU reports no animation for SVG at all. A
/// port that synthesizes frames from an SMIL timeline therefore changes
/// observable behavior (`image-multi-frame-p`, `:index` walking) and must be
/// opt-in: the default stays the GNU-compatible static frame, and enabling
/// the policy is the documented divergence point.
///
/// The optional `fps` is a sampling ceiling that doubles as the memory
/// bound — it caps the distinct frames one loop can produce
/// ([`crate::animated_visual::SampleGrid`]).
#[derive(
    Clone, Copy, Debug, Default, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize,
)]
pub struct ImageAnimationPolicy {
    enabled: bool,
    fps: Option<u32>,
}

impl ImageAnimationPolicy {
    /// The GNU-compatible default: animation is not materialized.
    #[must_use]
    pub const fn disabled() -> Self {
        Self {
            enabled: false,
            fps: None,
        }
    }

    /// Enable computed animation, optionally capping the sampling rate.
    ///
    /// A zero or negative rate cap is meaningless; it is dropped rather than
    /// carried, so the renderer's default ceiling applies.
    #[must_use]
    pub const fn enabled(fps: Option<u32>) -> Self {
        match fps {
            Some(fps) if fps > 0 => Self {
                enabled: true,
                fps: Some(fps),
            },
            _ => Self {
                enabled: true,
                fps: None,
            },
        }
    }

    /// Whether computed animation may run for this source.
    #[must_use]
    pub const fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// The sampling ceiling, when one was stated.
    #[must_use]
    pub const fn fps(&self) -> Option<u32> {
        self.fps
    }
}

/// GNU-compatible delay for the currently decoded animation frame.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ImageFrameDelay {
    /// The format supplied no positive duration. GNU publishes `t`, which
    /// `image-multi-frame-p` resolves through `image-default-frame-delay`.
    UseDefault,
    /// An exact rational number of milliseconds.
    Milliseconds {
        numerator: u32,
        denominator: std::num::NonZeroU32,
    },
}

impl ImageFrameDelay {
    #[must_use]
    pub const fn milliseconds(numerator: u32, denominator: u32) -> Option<Self> {
        let Some(denominator) = std::num::NonZeroU32::new(denominator) else {
            return None;
        };
        Some(if numerator == 0 {
            Self::UseDefault
        } else {
            Self::Milliseconds {
                numerator,
                denominator,
            }
        })
    }

    #[must_use]
    pub fn seconds(self) -> Option<f64> {
        match self {
            Self::UseDefault => None,
            Self::Milliseconds {
                numerator,
                denominator,
            } => Some(f64::from(numerator) / f64::from(denominator.get()) / 1_000.0),
        }
    }
}

/// Decoder-owned metadata exposed by GNU's `image-metadata`.
///
/// Geometry and renderer bookkeeping deliberately do not belong here. The
/// evaluator converts this closed, thread-safe representation to Lisp only at
/// the API boundary.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ImageEmbeddedMetadata {
    frame_count: Option<std::num::NonZeroU32>,
    frame_delay: Option<ImageFrameDelay>,
    /// Neomacs computed sources can have a prefix played only once.
    #[serde(default)]
    introduction: Option<ImageSequenceIntroduction>,
}

/// A prefix and its repeating tail have independently quantized delays.
/// Keeping these fields together prevents partial playback descriptions.
#[derive(Clone, Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
struct ImageSequenceIntroduction {
    loop_start: std::num::NonZeroU32,
    prefix_delay: ImageFrameDelay,
    loop_delay: ImageFrameDelay,
}

impl ImageEmbeddedMetadata {
    pub const EMPTY: Self = Self {
        frame_count: None,
        frame_delay: None,
        introduction: None,
    };

    #[must_use]
    pub fn animation(frame_count: u32, frame_delay: ImageFrameDelay) -> Self {
        Self {
            frame_count: std::num::NonZeroU32::new(frame_count).filter(|count| count.get() > 1),
            frame_delay: Some(frame_delay),
            introduction: None,
        }
    }

    /// Set the first repeatable frame, validating it against this sequence.
    #[must_use]
    pub fn with_introduction(
        mut self,
        start: ImageFrameIndex,
        prefix_delay: ImageFrameDelay,
        loop_delay: ImageFrameDelay,
    ) -> Option<Self> {
        let start = u32::try_from(start.get()).ok()?;
        if start >= self.frame_count()? {
            return None;
        }
        self.introduction =
            std::num::NonZeroU32::new(start).map(|loop_start| ImageSequenceIntroduction {
                loop_start,
                prefix_delay,
                loop_delay,
            });
        Some(self)
    }

    #[must_use]
    pub const fn loop_start(&self) -> Option<u32> {
        match &self.introduction {
            Some(intro) => Some(intro.loop_start.get()),
            None => None,
        }
    }

    #[must_use]
    pub const fn intro_delay(&self) -> Option<ImageFrameDelay> {
        match &self.introduction {
            Some(intro) => Some(intro.prefix_delay),
            None => None,
        }
    }

    #[must_use]
    pub const fn loop_delay(&self) -> Option<ImageFrameDelay> {
        match &self.introduction {
            Some(intro) => Some(intro.loop_delay),
            None => None,
        }
    }

    #[must_use]
    pub const fn frame_count(&self) -> Option<u32> {
        match self.frame_count {
            Some(count) => Some(count.get()),
            None => None,
        }
    }

    #[must_use]
    pub const fn frame_delay(&self) -> Option<ImageFrameDelay> {
        self.frame_delay
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.frame_count.is_none() && self.frame_delay.is_none()
    }
}

#[cfg(test)]
#[path = "image/tests/image_source_rect_test.rs"]
mod image_source_rect_tests;

/// Coordinate space carried by an [`ImageExtent`].
///
/// The marker never exists at runtime. It makes native decoder pixels,
/// logical layout pixels, GPU raster pixels, and GNU-reported image pixels
/// incompatible at compile time even though all four use the same integer
/// representation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ImageNativeSpace;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ImageLayoutSpace;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ImageRasterSpace;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ImageReportedSpace;

/// A two-dimensional image extent whose coordinate space is part of its type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ImageExtent<Space> {
    width: u32,
    height: u32,
    space: PhantomData<fn() -> Space>,
}

impl<Space> ImageExtent<Space> {
    #[must_use]
    pub const fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            space: PhantomData,
        }
    }

    #[must_use]
    pub const fn width(&self) -> u32 {
        self.width
    }

    #[must_use]
    pub const fn height(&self) -> u32 {
        self.height
    }

    #[must_use]
    pub const fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }
}

pub type ImageNativeExtent = ImageExtent<ImageNativeSpace>;
pub type ImageLayoutExtent = ImageExtent<ImageLayoutSpace>;
pub type ImageRasterExtent = ImageExtent<ImageRasterSpace>;
pub type ImageReportedExtent = ImageExtent<ImageReportedSpace>;

/// Finite, positive source dimensions before conversion to integer pixels.
///
/// Vector documents may have fractional dimensions. Keep these distinct from
/// pixel extents: GNU truncates native dimensions before scaling, but uses
/// their full precision when a requested axis determines the aspect ratio.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ImageIntrinsicExtent {
    width: f64,
    height: f64,
}

impl ImageIntrinsicExtent {
    #[must_use]
    pub fn new(width: f64, height: f64) -> Option<Self> {
        (width.is_finite() && height.is_finite() && width > 0.0 && height > 0.0)
            .then_some(Self { width, height })
    }

    #[must_use]
    pub const fn dimensions(self) -> (f64, f64) {
        (self.width, self.height)
    }

    /// The integer extent this source occupies once it is rasterized.
    ///
    /// GNU compares integer decoded dimensions, so a fractional extent is
    /// compared with its ceiling: a vector document that is over a limit after
    /// rounding is over the limit.
    #[must_use]
    pub fn native_ceiling(self) -> ImageNativeExtent {
        ImageNativeExtent::new(intrinsic_bound(self.width), intrinsic_bound(self.height))
    }
}

impl From<ImageNativeExtent> for ImageIntrinsicExtent {
    fn from(native: ImageNativeExtent) -> Self {
        Self {
            width: f64::from(native.width().max(1)),
            height: f64::from(native.height().max(1)),
        }
    }
}

/// GNU `max-image-size` resolved into the native-pixel bound it implies.
///
/// GNU refuses an image whose *native* extent — before `:width`, `:max-width`
/// and `:rotation` — exceeds the limit, and it does so before the loader has
/// allocated a single pixel (`check_image_size`, `src/image.c:1811-1836`):
///
/// - an integer limits both axes to that many pixels;
/// - a float limits each axis to that fraction of the frame's own pixel extent,
///   so the two axes carry *different* bounds;
/// - anything non-numeric is no limit at all.
///
/// Resolving a Lisp value plus a frame into this one value, on the frame the
/// image is being looked up on, is what lets the refusal happen at the load
/// rather than after it: the comparison at the decoder cannot disagree with the
/// comparison at the caller, and a loader cannot be in the state of "the limit
/// was never resolved".
///
/// The bound is stored as a native-pixel extent rather than as the raw float
/// because [`ImageNativeExtent`] saturates at `u32::MAX`: a ratio larger than
/// any representable image degenerates to [`Self::UNLIMITED`] instead of
/// becoming an "infinite but not really" special case every caller must
/// remember to handle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ImageSizeLimit {
    maximum: ImageNativeExtent,
}

impl ImageSizeLimit {
    /// No limit: GNU's non-numeric `max-image-size`, and the saturation point
    /// of every bound wider than the extent type can represent.
    pub const UNLIMITED: Self = Self {
        maximum: ImageNativeExtent::new(u32::MAX, u32::MAX),
    };

    /// GNU's `FIXNUMP (Vmax_image_size)` arm: one absolute pixel bound for both
    /// axes. A non-positive value permits nothing, which is the same verdict
    /// `width <= XFIXNUM (Vmax_image_size)` reaches for every real extent.
    #[must_use]
    pub fn from_axis_pixels(pixels: i64) -> Self {
        let bound = match u32::try_from(pixels) {
            Ok(bound) => bound,
            Err(_) if pixels < 0 => 0,
            Err(_) => u32::MAX,
        };
        Self {
            maximum: ImageNativeExtent::new(bound, bound),
        }
    }

    /// GNU's `FLOATP (Vmax_image_size)` arm: a fraction of this frame's pixel
    /// extent, per axis.
    ///
    /// `frame` is `None` for GNU's null-frame case, which compares against a
    /// fixed 1024x1024 (`src/image.c:1830`).
    #[must_use]
    pub fn from_frame_ratio(ratio: f64, frame: Option<ImageNativeExtent>) -> Self {
        let (frame_width, frame_height) = match frame {
            Some(frame) => (f64::from(frame.width()), f64::from(frame.height())),
            None => (1024.0, 1024.0),
        };
        Self {
            maximum: ImageNativeExtent::new(
                ratio_bound(ratio * frame_width),
                ratio_bound(ratio * frame_height),
            ),
        }
    }

    /// The largest native extent this limit admits.
    #[must_use]
    pub const fn maximum(self) -> ImageNativeExtent {
        self.maximum
    }

    /// GNU `check_image_size` for a decoded, integer-valued native extent.
    #[must_use]
    pub const fn permits(self, native: ImageNativeExtent) -> bool {
        // GNU's first arm: a non-positive extent is not a size at all
        // (`src/image.c:1816`). Header-derived extents are always positive, so
        // this only ever fires for a caller that invented one.
        if native.width() == 0 || native.height() == 0 {
            return false;
        }
        native.width() <= self.maximum.width() && native.height() <= self.maximum.height()
    }

    /// [`Self::permits`] for a source extent that has not been rounded to
    /// pixels yet; see [`ImageIntrinsicExtent::native_ceiling`].
    #[must_use]
    pub fn permits_intrinsic(self, intrinsic: ImageIntrinsicExtent) -> bool {
        self.permits(intrinsic.native_ceiling())
    }
}

impl Default for ImageSizeLimit {
    /// GNU's documented default: `MAX_IMAGE_SIZE 10.0` (`src/image.c:1740`)
    /// measured against GNU's own unknown-frame size.
    fn default() -> Self {
        Self::from_frame_ratio(10.0, None)
    }
}

/// `width <= X` for an integer width is `width <= floor (X)`.
///
/// A bound that is not a number, or not positive, admits nothing — which is
/// the verdict the comparison itself reaches, since every comparison against a
/// NaN is false. An infinite ratio is GNU's "no limit at all" and saturates to
/// the widest extent the type can hold.
fn ratio_bound(bound: f64) -> u32 {
    if bound.is_nan() || bound <= 0.0 {
        return 0;
    }
    if bound >= f64::from(u32::MAX) {
        return u32::MAX;
    }
    bound.floor() as u32
}

/// The integer extent a fractional source extent is compared as.
fn intrinsic_bound(dimension: f64) -> u32 {
    if dimension.is_nan() || dimension <= 0.0 {
        return 0;
    }
    if dimension >= f64::from(u32::MAX) {
        return u32::MAX;
    }
    dimension.ceil() as u32
}

/// An image the loader refuses because of [`ImageSizeLimit`].
///
/// GNU reports this through `image_size_error` (`src/image.c:1432`), which logs
/// [`Self::MESSAGE`] rather than signalling: redisplay must not be interrupted
/// by an image it cannot show, and the failed image keeps its placeholder slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OversizedImage {
    extent: ImageNativeExtent,
    limit: ImageSizeLimit,
}

impl OversizedImage {
    /// GNU's `image_size_error` text, verbatim.
    pub const MESSAGE: &'static str = "Invalid image size (see `max-image-size')";

    #[must_use]
    pub const fn new(extent: ImageNativeExtent, limit: ImageSizeLimit) -> Self {
        Self { extent, limit }
    }

    /// The native extent that was refused.
    #[must_use]
    pub const fn extent(self) -> ImageNativeExtent {
        self.extent
    }

    /// The limit that refused it.
    #[must_use]
    pub const fn limit(self) -> ImageSizeLimit {
        self.limit
    }
}

#[cfg(test)]
#[path = "image/tests/image_size_limit_test.rs"]
mod image_size_limit_tests;

/// All extents derived for one decoded image realization.
///
/// Keeping the spaces in one value gives bitmap and SVG decoders one sizing
/// operation and one rotation operation. Callers can no longer independently
/// recompute one of the three output extents with subtly different rounding.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ResolvedImageGeometry {
    layout: ImageLayoutExtent,
    reported: ImageReportedExtent,
    raster: ImageRasterExtent,
}

impl ResolvedImageGeometry {
    #[must_use]
    pub const fn new(
        layout: ImageLayoutExtent,
        reported: ImageReportedExtent,
        raster: ImageRasterExtent,
    ) -> Self {
        Self {
            layout,
            reported,
            raster,
        }
    }

    #[must_use]
    pub const fn layout(self) -> ImageLayoutExtent {
        self.layout
    }

    #[must_use]
    pub const fn reported(self) -> ImageReportedExtent {
        self.reported
    }

    #[must_use]
    pub const fn raster(self) -> ImageRasterExtent {
        self.raster
    }

    #[must_use]
    pub const fn with_raster(self, raster: ImageRasterExtent) -> Self {
        Self { raster, ..self }
    }

    #[must_use]
    pub fn oriented(self, rotation: ImageRotation) -> Self {
        Self {
            layout: rotation.orient_extent(self.layout),
            reported: rotation.orient_extent(self.reported),
            raster: rotation.orient_extent(self.raster),
        }
    }
}

/// One resolved image realization for one frame presentation.
///
/// - `layout_scale` maps native/GNU image pixels into **logical** layout pixels
///   (same space as `frame-char-width`).
/// - `device_scale` maps those logical pixels to physical texture pixels.
/// - `report_scale` maps layout pixels back to **GNU `Fimage_size` / image-pixel**
///   space (`PIXELS` non-nil). For `:scale default` this is the frame device
///   scale (layout was divided by it); otherwise it is 1.
///
/// The three factors travel together so pending layout, decoded metadata, and
/// GPU upload cannot consult different scale-factor snapshots.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ImageRealization {
    layout_scale_bits: u32,
    device_scale_bits: u32,
    report_scale_bits: u32,
}

impl ImageRealization {
    #[must_use]
    pub fn new(layout_scale: f32, device_scale: f32, report_scale: f32) -> Self {
        let layout_scale = if layout_scale.is_finite() && layout_scale >= 0.0 {
            layout_scale
        } else {
            1.0
        };
        let device_scale = if device_scale.is_finite() && device_scale > 0.0 {
            device_scale
        } else {
            1.0
        };
        let report_scale = if report_scale.is_finite() && report_scale > 0.0 {
            report_scale
        } else {
            1.0
        };
        Self {
            // Normalize signed zero so equal numeric realizations have one
            // cache identity.
            layout_scale_bits: if layout_scale == 0.0 {
                0.0_f32.to_bits()
            } else {
                layout_scale.to_bits()
            },
            device_scale_bits: device_scale.to_bits(),
            report_scale_bits: report_scale.to_bits(),
        }
    }

    /// Convenience when layout already lives in GNU image-pixel space
    /// (`report_scale = 1`).
    #[must_use]
    pub fn with_device_scale(layout_scale: f32, device_scale: f32) -> Self {
        Self::new(layout_scale, device_scale, 1.0)
    }

    #[must_use]
    pub fn layout_scale(self) -> f32 {
        f32::from_bits(self.layout_scale_bits)
    }

    #[must_use]
    pub fn device_scale(self) -> f32 {
        f32::from_bits(self.device_scale_bits)
    }

    #[must_use]
    pub fn report_scale(self) -> f32 {
        f32::from_bits(self.report_scale_bits)
    }

    /// Convert a GNU image dimension to integer logical layout pixels.
    #[must_use]
    pub fn layout_dimension(self, dimension: u32) -> u32 {
        ((f64::from(dimension) * f64::from(self.layout_scale()))
            .round()
            .max(1.0)) as u32
    }

    /// Convert an integer logical extent to physical texture pixels.
    #[must_use]
    pub fn raster_dimension(self, layout_dimension: u32) -> u32 {
        ((f64::from(layout_dimension) * f64::from(self.device_scale()))
            .ceil()
            .max(1.0)) as u32
    }

    /// Convert a logical layout extent to GNU `Fimage_size` pixel extent.
    ///
    /// Prefer re-running `ImageSizeSpec::desired` at [`Self::image_pixel_scale`]
    /// when the native size is still known — `ceil` is not invertible.
    #[must_use]
    pub fn image_pixel_dimension(self, layout_dimension: u32) -> u32 {
        ((f64::from(layout_dimension) * f64::from(self.report_scale()))
            .ceil()
            .max(1.0)) as u32
    }

    /// Scale factor for GNU `compute_image_size` / Fimage_size pixel space.
    ///
    /// Equal to `layout_scale × report_scale`, with a near-1.0 snap so that
    /// `1/1.25 × 1.25` does not become `1.0000001` and ceil native extents
    /// by an extra pixel.
    #[must_use]
    pub fn image_pixel_scale(self) -> f64 {
        if (self.report_scale() - 1.0).abs() <= f32::EPSILON {
            return f64::from(self.layout_scale());
        }
        let product = f64::from(self.layout_scale()) * f64::from(self.report_scale());
        if (product - 1.0).abs() < 1e-4 {
            1.0
        } else {
            product
        }
    }

    /// Resolve all image geometry once, then apply GNU's post-sizing rotation
    /// to every coordinate space together.
    #[must_use]
    pub fn resolve_geometry(
        self,
        size: ImageSizeSpec,
        native: impl Into<ImageIntrinsicExtent>,
        rotation: ImageRotation,
    ) -> ResolvedImageGeometry {
        let native = native.into();
        let (layout_width, layout_height) =
            size.desired_intrinsic(native, f64::from(self.layout_scale()));
        let (reported_width, reported_height) =
            size.desired_intrinsic(native, self.image_pixel_scale());
        let raster_width = self.raster_dimension(layout_width);
        let raster_height = self.raster_dimension(layout_height);

        ResolvedImageGeometry::new(
            ImageLayoutExtent::new(layout_width, layout_height),
            ImageReportedExtent::new(reported_width, reported_height),
            ImageRasterExtent::new(raster_width, raster_height),
        )
        .oriented(rotation)
    }
}

impl Default for ImageRealization {
    fn default() -> Self {
        Self::new(1.0, 1.0, 1.0)
    }
}

/// What a spec asks for along one axis.
///
/// GNU resolves `:width` vs `:max-width` by precedence — ":width overrides
/// :max-width" (src/image.c:2767) — which means the two can never both apply.
/// Making that a sum type retires the bug this replaces: the old code kept one
/// `max_width` field that BOTH keys wrote into, so a target silently became a
/// clamp and the aspect ratio was computed against the wrong number.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum AxisSize {
    /// Nothing requested: the native extent, scaled.
    #[default]
    Native,
    /// `:width` / `:height` — an exact target, itself multiplied by the scale.
    Exact(u32),
    /// `:max-width` / `:max-height` — an upper bound; the other axis follows to
    /// preserve the aspect ratio.
    AtMost(u32),
}

impl AxisSize {
    /// Apply GNU's precedence once, at construction: a target wins over a clamp.
    #[must_use]
    pub fn resolve(target: Option<u32>, at_most: Option<u32>) -> Self {
        match (target, at_most) {
            (Some(target), _) => Self::Exact(target),
            (None, Some(at_most)) => Self::AtMost(at_most),
            (None, None) => Self::Native,
        }
    }

    fn target(self, scale: f64) -> Option<u32> {
        match self {
            // GNU scales the target too (src/image.c:2766).
            Self::Exact(size) => Some(scale_size(size, 1.0, scale)),
            _ => None,
        }
    }

    /// The extent this axis pins, if any. `Native` pins nothing — the answer
    /// is not knowable until the image is decoded.
    #[must_use]
    pub fn pinned(self) -> Option<u32> {
        match self {
            Self::Exact(size) | Self::AtMost(size) => Some(size),
            Self::Native => None,
        }
    }

    fn at_most(self) -> Option<u32> {
        match self {
            Self::AtMost(size) => Some(size),
            _ => None,
        }
    }
}

/// A quarter-turn rotation, the only kind native transforms perform.
///
/// GNU accepts any number for `:rotation` but only multiplies of 90 actually
/// turn the image; everything else is a no-op (src/image.c:2928-2958).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ImageRotation {
    #[default]
    None,
    Quarter,
    Half,
    ThreeQuarter,
}

impl ImageRotation {
    /// GNU's `image_compute_rotation` followed by its 90-degree dispatch.
    #[must_use]
    pub fn from_degrees(degrees: f64) -> Self {
        if !degrees.is_finite() {
            return Self::None;
        }
        // Emacs `mod` keeps the divisor's sign, so -90 → 270, not 90.
        let mut reduced = degrees % 360.0;
        if reduced < 0.0 {
            reduced += 360.0;
        }
        // Snap near-integers; non-multiples of 90 stay upright.
        let nearest = reduced.round();
        if (reduced - nearest).abs() > 1e-6 {
            return Self::None;
        }
        match nearest as i32 {
            0 | 360 => Self::None,
            90 => Self::Quarter,
            180 => Self::Half,
            270 => Self::ThreeQuarter,
            _ => Self::None,
        }
    }

    /// Swap width/height for 90° and 270° turns (GNU after sizing).
    #[must_use]
    pub fn orient(self, width: u32, height: u32) -> (u32, u32) {
        match self {
            Self::None | Self::Half => (width, height),
            Self::Quarter | Self::ThreeQuarter => (height, width),
        }
    }

    /// Apply the turn without erasing the extent's coordinate-space marker.
    #[must_use]
    pub fn orient_extent<Space>(self, extent: ImageExtent<Space>) -> ImageExtent<Space> {
        let (width, height) = self.orient(extent.width(), extent.height());
        ImageExtent::new(width, height)
    }
}

/// This is GNU's `compute_image_size` input set (src/image.c:2750). It travels
/// to the decoder because the native size it is resolved against is the
/// decoder's — but for every raster format that size is already in the encoded
/// header, so layout can resolve the same geometry from the header while the
/// decode is still running (see `neomacs_renderer_wgpu::image_probe`). Both
/// resolutions must produce the identical extent: they are the same
/// [`ImageRealization::resolve_geometry`] call over the same native size.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct ImageSizeSpec {
    width: AxisSize,
    height: AxisSize,
}

impl ImageSizeSpec {
    #[must_use]
    pub fn new(width: AxisSize, height: AxisSize) -> Self {
        Self { width, height }
    }

    /// Size to reserve for an image that has not been decoded yet.
    ///
    /// `None` when neither axis is pinned: the native size is the answer then,
    /// and it is not known until decoding finishes.
    #[must_use]
    pub fn placeholder_extent(self) -> Option<(u32, u32)> {
        match (self.width.pinned(), self.height.pinned()) {
            (Some(width), Some(height)) => Some((width, height)),
            // One axis pinned: the other follows the native aspect ratio, which
            // is still unknown, so fall back to a square of the known extent.
            (Some(width), None) => Some((width, width)),
            (None, Some(height)) => Some((height, height)),
            (None, None) => None,
        }
    }

    /// The size a `native_width` x `native_height` image should be drawn at.
    ///
    /// Mirrors GNU `compute_image_size` (src/image.c:2750) step for step.
    #[must_use]
    pub fn desired(self, native_width: u32, native_height: u32, scale: f64) -> (u32, u32) {
        self.desired_intrinsic(
            ImageNativeExtent::new(native_width, native_height).into(),
            scale,
        )
    }

    /// Resolve source dimensions without prematurely rounding vector geometry.
    #[must_use]
    pub fn desired_intrinsic(self, native: ImageIntrinsicExtent, scale: f64) -> (u32, u32) {
        let (native_width, native_height) = native.dimensions();

        let (mut width, mut height) = match (self.width.target(scale), self.height.target(scale)) {
            // Both given: GNU skips the aspect-preserving work entirely.
            (Some(width), Some(height)) => return (width.max(1), height.max(1)),
            (Some(width), None) => (width, ratio(width, native_width, native_height)),
            (None, Some(height)) => (ratio(height, native_height, native_width), height),
            (None, None) => (
                // GNU compute_image_size passes double dimensions to the
                // integer SIZE parameter of scale_image_size here only.
                scale_size(native_width as u32, 1.0, scale),
                scale_size(native_height as u32, 1.0, scale),
            ),
        };

        // Clamps, each preserving the aspect ratio (src/image.c:2798-2810).
        if let Some(max) = self.width.at_most().filter(|max| *max < width) {
            width = max;
            height = ratio(width, native_width, native_height);
        }
        if let Some(max) = self.height.at_most().filter(|max| *max < height) {
            height = max;
            width = ratio(height, native_height, native_width);
        }

        (width.max(1), height.max(1))
    }
}

/// GNU `scale_image_size` (src/image.c:2700): `size * multiplier / divisor`.
///
/// Uses `ceil` like GNU so fractional SVG/device pixels are never discarded.
fn scale_size(size: u32, divisor: f64, multiplier: f64) -> u32 {
    let scaled = f64::from(size) * multiplier / divisor;
    if scaled.is_finite() && scaled >= 1.0 {
        scaled.ceil() as u32
    } else {
        1
    }
}

/// Keep the aspect ratio: `size * to / from`.
fn ratio(size: u32, from: f64, to: f64) -> u32 {
    scale_size(size, from, to)
}

#[cfg(test)]
#[path = "image/tests/image_inline_test.rs"]
mod tests;
