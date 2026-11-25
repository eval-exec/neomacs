//! Media vocabulary the evaluator sends to a display: shader surfaces and
//! image sources.

/// Source language of a user surface shader.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum SurfaceShaderLanguage {
    Wgsl,
    Glsl,
}

/// What `iChannel0` samples: another shader surface's texture, a decoded
/// image, or a (playing) video's current frame. Resolved per pass so late
/// creation / decode completion / per-frame video uploads are picked up.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub enum SurfaceChannelSource {
    Surface(u32),
    Image(crate::types::ImageId),
    Video(crate::VideoId),
}

/// Initial value for one user uniform: Lisp `(name . value)` pairs arrive as
/// a name, up to four components, and the component count (1..=4) that picks
/// the accessor's WGSL type (f32/vec2/vec3/vec4).
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SurfaceUniformInit {
    pub name: String,
    pub value: [f32; 4],
    pub components: u8,
}

/// How an image source's text was stored as a Lisp string.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ImageSpellingMode {
    Unibyte,
    Multibyte,
}

/// Property-free image source text, preserving the Lisp cache-key spelling:
/// its bytes and its string mode.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ImageSourceSpelling {
    bytes: Vec<u8>,
    mode: ImageSpellingMode,
}

/// Hashes exactly as the Lisp string the spelling was taken from (the bytes,
/// then whether the string is multibyte, as `neovm_value`'s `LispString`
/// does), so a spelling and its source string agree as hash keys. Equality is
/// the same relation: the bytes and the mode.
impl std::hash::Hash for ImageSourceSpelling {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.bytes.hash(state);
        match self.mode {
            ImageSpellingMode::Unibyte => false,
            ImageSpellingMode::Multibyte => true,
        }
        .hash(state);
    }
}

impl ImageSourceSpelling {
    #[must_use]
    pub fn new(bytes: Vec<u8>, mode: ImageSpellingMode) -> Self {
        Self { bytes, mode }
    }

    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    #[must_use]
    pub const fn mode(&self) -> ImageSpellingMode {
        self.mode
    }
}

/// A GNU image `:base-uri` spelling without Lisp properties or heap pointers.
///
/// Retain its original bytes and string mode for request identity. Decoders
/// interpret those bytes as UTF-8 exactly as they did at the Lisp-string edge.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ImageBaseUri(ImageSourceSpelling);

static_assertions::assert_impl_all!(ImageBaseUri: Send, Sync, Clone, std::fmt::Debug, Eq, std::hash::Hash);

impl ImageBaseUri {
    #[must_use]
    pub fn new(spelling: ImageSourceSpelling) -> Self {
        Self(spelling)
    }

    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }

    #[must_use]
    pub fn as_utf8_str(&self) -> Option<&str> {
        std::str::from_utf8(self.as_bytes()).ok()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ImageDataSource {
    /// Encoded bytes with no authority to resolve external resources.
    Isolated(crate::image::EncodedBytes),
    /// Encoded bytes whose relative resources may be resolved against the
    /// explicitly supplied GNU image `:base-uri`.
    WithBaseUri {
        data: crate::image::EncodedBytes,
        base_uri: ImageBaseUri,
    },
}

static_assertions::assert_impl_all!(ImageDataSource: Send, Sync, Clone, std::fmt::Debug, Eq, std::hash::Hash);

impl ImageDataSource {
    /// The encoded bytes, whichever authority they carry.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        match self {
            Self::Isolated(data) => data,
            Self::WithBaseUri { data, .. } => data,
        }
    }
}

/// Intrinsic metadata of a decoded image, as the evaluator lays it out.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ResolvedImageMetadata {
    /// Logical layout size (redisplay / `frame-char-width` space).
    pub layout: crate::image::ImageLayoutExtent,
    /// GNU `Fimage_size` with PIXELS non-nil (`img->width` / `img->height` space).
    pub reported: crate::image::ImageReportedExtent,
    /// GNU's decoded four-corner background guess (0x00RRGGBB).
    pub background: u32,
    /// GNU's decoded four-corner mask classification.
    pub background_transparent: bool,
    /// Distinguishes a GNU-compatible clipping mask from continuous alpha.
    pub mask: crate::image::ImageMaskKind,
    /// Format metadata owned by the decoder, never renderer geometry.
    pub embedded: crate::image::ImageEmbeddedMetadata,
}

impl ResolvedImageMetadata {
    /// Build metadata when layout already equals GNU image-pixel size.
    #[must_use]
    pub const fn layout_is_image_pixels(
        width: u32,
        height: u32,
        background: u32,
        background_transparent: bool,
        mask: crate::image::ImageMaskKind,
    ) -> Self {
        Self {
            layout: crate::image::ImageLayoutExtent::new(width, height),
            reported: crate::image::ImageReportedExtent::new(width, height),
            background,
            background_transparent,
            mask,
            embedded: crate::image::ImageEmbeddedMetadata::EMPTY,
        }
    }

    /// Build metadata from logical layout + realization report scale.
    #[must_use]
    pub fn from_layout(
        layout: crate::image::ImageLayoutExtent,
        realization: crate::ImageRealization,
        background: u32,
        background_transparent: bool,
        mask: crate::image::ImageMaskKind,
    ) -> Self {
        Self {
            layout,
            reported: realization.report_extent(layout),
            background,
            background_transparent,
            mask,
            embedded: crate::image::ImageEmbeddedMetadata::default(),
        }
    }

    /// Attach decoder-owned metadata without making plain-image constructors
    /// fabricate animation state.
    #[must_use]
    pub fn with_embedded(mut self, embedded: crate::image::ImageEmbeddedMetadata) -> Self {
        self.embedded = embedded;
        self
    }
}

#[cfg(test)]
#[path = "media_vocab/tests/media_vocab_test.rs"]
mod tests;
