//! Font identity vocabulary embedded in moved object headers.
//!
//! These are the exact definitions display-protocol re-exports at
//! `neomacs_display_protocol::font::*`; bodies are unchanged from the
//! pre-move protocol module.

use std::hash::{Hash, Hasher};

/// Which platform font catalog discovered an identity.
///
/// This is diagnostic provenance. It deliberately does not participate in
/// [`ResolvedFontIdentity`] equality or hashing: two catalogs can discover
/// the same exact file, collection face, and variable-font instance.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub enum FontBackendKind {
    /// Linux fontconfig / fontdb file identities.
    Fontconfig,
    /// macOS CoreText descriptors.
    CoreText,
    /// Windows DirectWrite face identities.
    DirectWrite,
}

/// One variation-axis coordinate of a variable font instance.
///
/// The value is stored as raw `f32` bits so the identity stays `Eq + Hash`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize)]
pub struct FontVariationCoord {
    /// OpenType axis tag (e.g. `wght` as big-endian bytes).
    tag: u32,
    /// Axis value as `f32::to_bits`.
    value_bits: u32,
}

impl FontVariationCoord {
    /// Construct one finite axis coordinate.
    ///
    /// NaN and infinities cannot be replayed consistently by native APIs,
    /// shapers, or raster caches, so they are rejected at the protocol edge.
    pub fn try_new(tag: u32, value: f32) -> Option<Self> {
        value.is_finite().then(|| Self {
            tag,
            value_bits: value.to_bits(),
        })
    }

    pub fn value(self) -> f32 {
        f32::from_bits(self.value_bits)
    }

    pub const fn tag(self) -> u32 {
        self.tag
    }

    pub const fn value_bits(self) -> u32 {
        self.value_bits
    }
}

impl<'de> serde::Deserialize<'de> for FontVariationCoord {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        struct WireCoord {
            tag: u32,
            value_bits: u32,
        }

        let wire = WireCoord::deserialize(deserializer)?;
        Self::try_new(wire.tag, f32::from_bits(wire.value_bits)).ok_or_else(|| {
            serde::de::Error::custom("font variation coordinate must contain a finite value")
        })
    }
}

/// Canonical variable-font coordinates for one exact instance.
///
/// Coordinates are always sorted by OpenType tag and contain at most one
/// value per axis. Construction uses the final value for a duplicate tag,
/// matching ordinary attribute-map semantics while keeping identity and cache
/// keys deterministic.
#[repr(transparent)]
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, serde::Serialize)]
#[serde(transparent)]
pub struct FontVariationSet(Vec<FontVariationCoord>);

impl FontVariationSet {
    pub fn new(coords: Vec<FontVariationCoord>) -> Self {
        let mut by_tag = std::collections::BTreeMap::new();
        for coord in coords {
            by_tag.insert(coord.tag(), coord);
        }
        Self(by_tag.into_values().collect())
    }

    pub fn as_slice(&self) -> &[FontVariationCoord] {
        &self.0
    }

    pub fn into_vec(self) -> Vec<FontVariationCoord> {
        self.0
    }

    /// Reserved coordinate capacity, used only by heap-accounting code.
    pub fn capacity(&self) -> usize {
        self.0.capacity()
    }
}

impl From<Vec<FontVariationCoord>> for FontVariationSet {
    fn from(coords: Vec<FontVariationCoord>) -> Self {
        Self::new(coords)
    }
}

impl std::ops::Deref for FontVariationSet {
    type Target = [FontVariationCoord];

    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

impl<'de> serde::Deserialize<'de> for FontVariationSet {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Vec::<FontVariationCoord>::deserialize(deserializer).map(Self::new)
    }
}

/// Exact, platform-openable font identity.
///
/// Not "file path only": macOS/Windows may need native descriptors, so
/// `stable_key` is the durable cross-snapshot cache key and `file_path`
/// is populated whenever a backend exposes a durable local file.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ResolvedFontIdentity {
    pub backend: FontBackendKind,
    /// Durable backend-independent key:
    /// `"{file_path}#{collection_face}@{axis}={value_bits},..."`.
    ///
    /// Platform selector encodings and diagnostic names never enter this key.
    pub stable_key: String,
    /// Absolute font file path when the backend exposes one.
    pub file_path: Option<String>,
    /// Backend-native face selector.
    ///
    /// For Fontconfig/FreeType, bits 0-15 are the face index within the font
    /// file and bits 16-30 select a named variable-font instance. Consumers
    /// which use `fontdb`/`ttf-parser` must call [`Self::file_face_index`]
    /// instead of passing this value through directly.
    face_selector: BackendFontSelector,
    pub postscript_name: Option<String>,
    /// Variable font instance coordinates, if any.
    pub variation_coords: FontVariationSet,
}

/// Opaque selector understood by the platform font backend.
///
/// This is intentionally not interchangeable with a collection face index:
/// Fontconfig/FreeType also encode a named variable-font instance in it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct BackendFontSelector(u32);

impl BackendFontSelector {
    const fn from_raw(raw: u32) -> Self {
        Self(raw)
    }

    const fn raw(self) -> u32 {
        self.0
    }
}

impl ResolvedFontIdentity {
    /// Linux fontconfig/fontdb identity from a file path + face index.
    pub fn from_file(file_path: &str, face_index: u32, postscript_name: Option<String>) -> Self {
        Self::from_file_with_variations(file_path, face_index, postscript_name, Vec::new())
    }

    /// Linux fontconfig/fontdb identity for an exact variable-font instance.
    ///
    /// Variation coordinates are sorted by OpenType tag so backend ordering
    /// cannot create distinct identities for the same instance. Their raw
    /// floating-point bits are part of the stable key; renderer caches must
    /// never alias two instances from the same file and collection index.
    pub fn from_file_with_variations(
        file_path: &str,
        face_index: u32,
        postscript_name: Option<String>,
        variation_coords: Vec<FontVariationCoord>,
    ) -> Self {
        let variation_coords = FontVariationSet::new(variation_coords);

        let collection_face = face_index & 0x0000_ffff;
        let mut stable_key = format!("{file_path}#{collection_face}");
        append_variation_key(&mut stable_key, variation_coords.as_slice());

        Self {
            backend: FontBackendKind::Fontconfig,
            stable_key,
            file_path: Some(file_path.to_string()),
            face_selector: BackendFontSelector::from_raw(face_index),
            postscript_name,
            variation_coords,
        }
    }

    /// Exact file-backed identity selected by a native platform backend.
    ///
    /// The native adapter remains available as diagnostic provenance while
    /// the stable key records only the collection face and exact variations.
    /// This makes an instance discovered through CoreText or DirectWrite
    /// identical to the same instance discovered through another catalog.
    pub fn from_platform_file_with_variations(
        backend: FontBackendKind,
        file_path: &str,
        face_selector: u32,
        postscript_name: Option<String>,
        variation_coords: Vec<FontVariationCoord>,
    ) -> Self {
        if backend == FontBackendKind::Fontconfig {
            return Self::from_file_with_variations(
                file_path,
                face_selector,
                postscript_name,
                variation_coords,
            );
        }
        let variation_coords = FontVariationSet::new(variation_coords);
        let mut stable_key = format!("{file_path}#{face_selector}");
        append_variation_key(&mut stable_key, variation_coords.as_slice());

        Self {
            backend,
            stable_key,
            file_path: Some(file_path.to_string()),
            face_selector: BackendFontSelector::from_raw(face_selector),
            postscript_name,
            variation_coords,
        }
    }

    /// Identity for a font already resident in the layout font database.
    pub fn from_memory(
        backend: FontBackendKind,
        stable_key: String,
        backend_selector: u32,
        postscript_name: Option<String>,
    ) -> Self {
        Self::from_native_with_variations(
            backend,
            stable_key,
            backend_selector,
            postscript_name,
            Vec::new(),
        )
    }

    /// Identity for a native catalog entity which may not expose a URL.
    /// The owning platform adapter later materializes immutable table bytes;
    /// the identity itself remains durable and carries the selected instance.
    pub fn from_native_with_variations(
        backend: FontBackendKind,
        mut stable_key: String,
        backend_selector: u32,
        postscript_name: Option<String>,
        variation_coords: Vec<FontVariationCoord>,
    ) -> Self {
        let variation_coords = FontVariationSet::new(variation_coords);
        append_variation_key(&mut stable_key, variation_coords.as_slice());
        Self {
            backend,
            stable_key,
            file_path: None,
            face_selector: BackendFontSelector::from_raw(backend_selector),
            postscript_name,
            variation_coords,
        }
    }

    /// The opaque selector value for diagnostics and platform-native APIs.
    pub fn backend_selector(&self) -> u32 {
        self.face_selector.raw()
    }

    /// Selector accepted by FreeType, including named-instance bits.
    pub fn freetype_selector(&self) -> Option<u32> {
        (self.backend == FontBackendKind::Fontconfig).then(|| self.face_selector.raw())
    }

    /// Face index understood by font-file parsers such as fontdb and
    /// ttf-parser.
    ///
    /// Those parsers enumerate collection faces but do not enumerate
    /// FreeType's named variable-font instances. Keeping this conversion at
    /// the identity boundary prevents layout and rendering from confusing a
    /// Fontconfig selector such as `0x0007_0000` with collection face 458752.
    pub fn file_face_index(&self) -> u32 {
        match self.backend {
            FontBackendKind::Fontconfig => self.face_selector.raw() & 0x0000_ffff,
            FontBackendKind::CoreText | FontBackendKind::DirectWrite => self.face_selector.raw(),
        }
    }

    /// FreeType named-instance index carried by a Fontconfig selector.
    pub fn named_instance_index(&self) -> Option<u32> {
        match self.backend {
            FontBackendKind::Fontconfig => {
                let index = (self.face_selector.raw() >> 16) & 0x7fff;
                (index != 0).then_some(index)
            }
            FontBackendKind::CoreText | FontBackendKind::DirectWrite => None,
        }
    }
}

impl PartialEq for ResolvedFontIdentity {
    fn eq(&self, other: &Self) -> bool {
        self.stable_key == other.stable_key
    }
}

impl Eq for ResolvedFontIdentity {}

impl Hash for ResolvedFontIdentity {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.stable_key.hash(state);
    }
}

fn append_variation_key(stable_key: &mut String, variation_coords: &[FontVariationCoord]) {
    if variation_coords.is_empty() {
        return;
    }
    stable_key.push('@');
    for (index, coord) in variation_coords.iter().enumerate() {
        if index != 0 {
            stable_key.push(',');
        }
        let tag = coord.tag().to_be_bytes();
        stable_key.extend(tag.into_iter().map(char::from));
        stable_key.push('=');
        stable_key.push_str(&format!("{:08x}", coord.value_bits()));
    }
}
