//! Resolved font identity types.
//!
//! Semantic font selection (family alias resolution, fontset fallback,
//! weight/slant substitution, per-char coverage) happens on the
//! evaluator/layout side. The render thread receives an exact, already
//! resolved font identity and only rasterizes. See
//! `docs/plans/2026-07-05-font-realization-render-boundary-design.md`.

use crate::{face::Face, types::FaceId};
use std::collections::HashMap;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::num::NonZeroU64;
use std::sync::Arc;

/// Version of the native system-font catalog used to realize one frame.
///
/// Zero is deliberately unrepresentable and missing legacy wire fields default
/// to the first live generation. Renderer caches compare generations for
/// equality rather than ordering, so wrapping a process-lifetime counter
/// remains safe.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct FontCatalogGeneration(NonZeroU64);

impl FontCatalogGeneration {
    pub const INITIAL: Self = Self(NonZeroU64::MIN);

    /// Construct from a native/wire counter, mapping the reserved zero value
    /// to the initial live generation.
    pub const fn from_raw(raw: u64) -> Self {
        match NonZeroU64::new(raw) {
            Some(value) => Self(value),
            None => Self::INITIAL,
        }
    }

    pub const fn get(self) -> u64 {
        self.0.get()
    }

    pub const fn next(self) -> Self {
        match self.get().checked_add(1) {
            Some(next) => Self::from_raw(next),
            None => Self::INITIAL,
        }
    }
}

impl Default for FontCatalogGeneration {
    fn default() -> Self {
        Self::INITIAL
    }
}

/// Snapshot-local id referencing an entry in a frame state's resolved
/// font table (`FrameDisplayState::fonts`).
///
/// Ids are allocated from the complete realized instance (durable identity,
/// replay method/strike, and size) and are stable for the lifetime of that
/// resolver, so consecutive frame snapshots reuse ids.
/// Renderer caches must still key on [`ResolvedFontIdentity`] (or a hash
/// of it), never on the raw id, so id renumbering after a font-database
/// change can never alias a cached glyph to the wrong font.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub struct ResolvedFontId(pub u32);

/// Which platform font catalog discovered an identity.
///
/// This is diagnostic provenance. It deliberately does not participate in
/// [`ResolvedFontIdentity`] equality or hashing: two catalogs can discover
/// the same exact file, collection face, and variable-font instance.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub enum FontBackendKind {
    /// Immutable application-packaged font catalog used without host discovery.
    Packaged,
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
            FontBackendKind::Packaged
            | FontBackendKind::CoreText
            | FontBackendKind::DirectWrite => self.face_selector.raw(),
        }
    }

    /// FreeType named-instance index carried by a Fontconfig selector.
    pub fn named_instance_index(&self) -> Option<u32> {
        match self.backend {
            FontBackendKind::Fontconfig => {
                let index = (self.face_selector.raw() >> 16) & 0x7fff;
                (index != 0).then_some(index)
            }
            FontBackendKind::Packaged
            | FontBackendKind::CoreText
            | FontBackendKind::DirectWrite => None,
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

/// How a resolved font was chosen. Distinguishing fallback tiers keeps
/// traces and oracle runs able to flag unexpected selection paths.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub enum FontResolutionSource {
    /// The realized face's primary font.
    FacePrimary,
    /// Chosen via fontset / per-character coverage fallback.
    FontsetFallback,
    /// Chosen via emoji presentation fallback.
    EmojiFallback,
    /// Chosen by the platform's last-resort matching.
    PlatformFallback,
    /// Renderer-side emergency fallback: text reached the render thread
    /// without a resolved identity. Must be zero for normal GUI text.
    EmergencyFallback,
}

/// Font slant as carried across the display protocol.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
pub enum FontSlantKind {
    #[default]
    Normal,
    Italic,
    Oblique,
}

/// Stable identity of the fixed bitmap strike selected during realization.
/// The ppem values use FreeType's 26.6 representation and let the renderer
/// reject a stale or mismatched face instead of silently selecting another
/// strike at replay time.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct BitmapStrikeKey {
    pub index: u32,
    pub x_ppem_26_6: i64,
    pub y_ppem_26_6: i64,
}

/// Sampling policy attached to a realized glyph source.
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    serde::Serialize,
    serde::Deserialize,
)]
pub enum GlyphSampling {
    #[default]
    Linear,
    Nearest,
}

/// GNU `ftfont_open`'s horizontal-metric policy for a fixed font.
///
/// This is part of replay identity: the render thread must reopen the exact
/// instance with the same spacing semantics selected by layout.
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    serde::Serialize,
    serde::Deserialize,
)]
pub enum FixedFontSpacing {
    /// GNU proportional and dual-width entities measure printable ASCII and
    /// retain the actual space-glyph advance.
    #[default]
    ProportionalOrDual,
    /// GNU mono and charcell entities use the face maximum advance for both
    /// average and space width.
    MonospaceOrCharacterCell,
}

/// Durable instructions for reopening one exact resolved font on the render
/// thread. Process-local font handles never cross the display protocol.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize)]
pub struct FontFileAsset {
    path: String,
    face_index: u32,
}

impl<'de> serde::Deserialize<'de> for FontFileAsset {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        struct WireAsset {
            path: String,
            face_index: u32,
        }

        let wire = WireAsset::deserialize(deserializer)?;
        Self::new(wire.path, wire.face_index)
            .ok_or_else(|| serde::de::Error::custom("font file asset path must not be empty"))
    }
}

impl FontFileAsset {
    /// Describe one exact collection face in a non-empty local font file.
    pub fn new(path: impl Into<String>, face_index: u32) -> Option<Self> {
        let path = path.into();
        (!path.is_empty()).then_some(Self { path, face_index })
    }

    /// Build the parser-facing asset for a file-backed platform identity.
    ///
    /// This deliberately applies [`ResolvedFontIdentity::file_face_index`]:
    /// Fontconfig's selector may also contain FreeType named-instance bits,
    /// which are not part of a font-file collection index.
    pub fn from_identity(identity: &ResolvedFontIdentity) -> Option<Self> {
        Self::new(identity.file_path.clone()?, identity.file_face_index())
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub const fn face_index(&self) -> u32 {
        self.face_index
    }
}

/// Immutable font bytes shared by layout, frame snapshots, and rendering.
///
/// The key names the native catalog entity and participates in renderer-cache
/// identity. Bytes are reference-counted so publishing a frame never copies a
/// system font. Native platform handles remain confined to their adapter.
#[derive(Clone, serde::Serialize)]
pub struct FontMemoryAsset {
    key: String,
    bytes: Arc<Vec<u8>>,
    face_index: u32,
}

impl<'de> serde::Deserialize<'de> for FontMemoryAsset {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        struct WireAsset {
            key: String,
            bytes: Arc<Vec<u8>>,
            face_index: u32,
        }

        let wire = WireAsset::deserialize(deserializer)?;
        Self::new(wire.key, wire.bytes, wire.face_index).ok_or_else(|| {
            serde::de::Error::custom("font memory asset key and bytes must not be empty")
        })
    }
}

impl FontMemoryAsset {
    pub fn new(key: impl Into<String>, bytes: Arc<Vec<u8>>, face_index: u32) -> Option<Self> {
        let key = key.into();
        (!key.is_empty() && !bytes.is_empty()).then_some(Self {
            key,
            bytes,
            face_index,
        })
    }

    pub fn key(&self) -> &str {
        &self.key
    }

    pub fn bytes(&self) -> &[u8] {
        self.bytes.as_slice()
    }

    pub fn shared_bytes(&self) -> Arc<Vec<u8>> {
        Arc::clone(&self.bytes)
    }

    pub const fn face_index(&self) -> u32 {
        self.face_index
    }
}

impl fmt::Debug for FontMemoryAsset {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("FontMemoryAsset")
            .field("key", &self.key)
            .field("byte_len", &self.bytes.len())
            .field("face_index", &self.face_index)
            .finish()
    }
}

impl PartialEq for FontMemoryAsset {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
            && self.face_index == other.face_index
            && (Arc::ptr_eq(&self.bytes, &other.bytes) || self.bytes == other.bytes)
    }
}

impl Eq for FontMemoryAsset {}

impl Hash for FontMemoryAsset {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Native keys are unique within one font-catalog generation. Avoid
        // hashing megabytes of immutable data for every glyph/cache lookup;
        // equality still compares bytes if two keys ever collide.
        self.key.hash(state);
        self.face_index.hash(state);
    }
}

/// Exact byte source accepted by the shared fontdb/Swash adapter.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum FontOutlineAsset {
    File(FontFileAsset),
    Memory(FontMemoryAsset),
}

impl FontOutlineAsset {
    pub const fn face_index(&self) -> u32 {
        match self {
            Self::File(asset) => asset.face_index(),
            Self::Memory(asset) => asset.face_index(),
        }
    }

    pub fn file(&self) -> Option<&FontFileAsset> {
        match self {
            Self::File(asset) => Some(asset),
            Self::Memory(_) => None,
        }
    }

    pub fn bytes(&self) -> Option<&[u8]> {
        match self {
            Self::File(_) => None,
            Self::Memory(asset) => Some(asset.bytes()),
        }
    }
}

/// Durable, valid-by-construction instructions for reopening one exact
/// resolved font on the render thread.
///
/// Each variant owns the only kind of source its adapter accepts. This makes
/// an outline replay without bytes, or a FreeType bitmap replay without a
/// file, unrepresentable.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum FontReplay {
    Swash {
        asset: FontOutlineAsset,
    },
    FreeTypeBitmap {
        asset: FontFileAsset,
        strike: BitmapStrikeKey,
        sampling: GlyphSampling,
        #[serde(default)]
        spacing: FixedFontSpacing,
    },
}

impl FontReplay {
    pub const fn sampling(&self) -> GlyphSampling {
        match self {
            Self::Swash { .. } => GlyphSampling::Linear,
            Self::FreeTypeBitmap { sampling, .. } => *sampling,
        }
    }

    pub const fn outline_asset(&self) -> Option<&FontOutlineAsset> {
        match self {
            Self::Swash { asset } => Some(asset),
            Self::FreeTypeBitmap { .. } => None,
        }
    }

    pub const fn file_asset(&self) -> Option<&FontFileAsset> {
        match self {
            Self::Swash {
                asset: FontOutlineAsset::File(asset),
            }
            | Self::FreeTypeBitmap { asset, .. } => Some(asset),
            Self::Swash {
                asset: FontOutlineAsset::Memory(_),
            } => None,
        }
    }
}

/// Positive finite logical-pixel advance for one realized fixed font cell.
///
/// Store the IEEE bits so this protocol value remains `Eq + Hash` while its
/// only constructor and deserializer reject NaN, infinity, zero, and negative
/// geometry.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FontAdvancePx(u32);

impl FontAdvancePx {
    pub fn new(value: f32) -> Option<Self> {
        (value.is_finite() && value > 0.0).then(|| Self(value.to_bits()))
    }

    #[must_use]
    pub fn get(self) -> f32 {
        f32::from_bits(self.0)
    }
}

impl serde::Serialize for FontAdvancePx {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_f32(self.get())
    }
}

impl<'de> serde::Deserialize<'de> for FontAdvancePx {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = <f32 as serde::Deserialize>::deserialize(deserializer)?;
        Self::new(value).ok_or_else(|| {
            serde::de::Error::custom("font advance must be finite and greater than zero")
        })
    }
}

/// Horizontal-advance contract for one exact realized font.
///
/// Proportional and dual-width fonts retain each glyph's own advance. GNU's
/// mono and charcell fonts instead position every covered glyph in the
/// realized font's maximum-width cell. Publishing that decision prevents
/// layout and rendering from independently choosing incompatible metrics.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
pub enum ResolvedFontAdvance {
    #[default]
    PerGlyph,
    FixedCell(FontAdvancePx),
}

impl ResolvedFontAdvance {
    #[must_use]
    pub fn fixed_cell(advance_px: f32) -> Self {
        FontAdvancePx::new(advance_px)
            .map(Self::FixedCell)
            .unwrap_or(Self::PerGlyph)
    }

    #[must_use]
    pub fn resolve(self, measured_advance_px: f32) -> f32 {
        match self {
            Self::PerGlyph => measured_advance_px,
            Self::FixedCell(advance_px) => advance_px.get(),
        }
    }

    #[must_use]
    pub fn fixed_cell_advance_px(self) -> Option<f32> {
        match self {
            Self::PerGlyph => None,
            Self::FixedCell(advance_px) => Some(advance_px.get()),
        }
    }
}

/// The resolver's canonical answer for one concrete font instance.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ResolvedFont {
    pub id: ResolvedFontId,
    pub identity: ResolvedFontIdentity,
    pub replay: FontReplay,
    /// Family name as realized (selector semantics, not file metadata).
    pub family: String,
    pub full_name: Option<String>,
    pub postscript_name: Option<String>,
    /// CSS weight (400 = normal, 700 = bold).
    pub weight: u16,
    pub slant: FontSlantKind,
    /// OS/2 usWidthClass-style stretch number (5 = normal).
    pub width: u16,
    pub pixel_size: f32,
    pub ascent_px: f32,
    pub descent_px: f32,
    /// GNU `font->space_width`: also the advance used when an ASCII glyph is
    /// unavailable in this primary font.
    #[serde(default)]
    pub space_advance_px: f32,
    /// Whether covered glyphs retain their outline advance or occupy one
    /// canonical fixed-pitch cell.
    #[serde(default)]
    pub glyph_advance: ResolvedFontAdvance,
    pub source: FontResolutionSource,
}

/// Resolved font table carried by frame state, keyed by [`ResolvedFontId`].
pub type ResolvedFontTable = HashMap<ResolvedFontId, ResolvedFont>;

/// Backend-neutral glyph index in one exact [`ResolvedFont`].
///
/// FreeType exposes the full unsigned 32-bit glyph-index domain. Keeping that
/// domain in the display protocol prevents fixed bitmap fonts from being
/// truncated merely because Swash currently uses 16-bit indices.
#[repr(transparent)]
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(transparent)]
pub struct ResolvedGlyphId(u32);

impl ResolvedGlyphId {
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u32 {
        self.0
    }

    pub const fn as_u16(self) -> Option<u16> {
        if self.0 <= u16::MAX as u32 {
            Some(self.0 as u16)
        } else {
            None
        }
    }
}

impl From<u16> for ResolvedGlyphId {
    fn from(value: u16) -> Self {
        Self(u32::from(value))
    }
}

/// One shaped glyph past semantic selection and shaping: the renderable
/// unit. Positions/advances are logical (scale 1.0) pixels; the renderer
/// applies its own scale factor.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ResolvedGlyph {
    /// Font this glyph id belongs to, in the frame's font table.
    pub resolved_font_id: ResolvedFontId,
    /// Glyph index within that font.
    pub glyph_id: ResolvedGlyphId,
    /// Pen x offset within the cluster/run.
    pub x: f32,
    /// Pen y offset (baseline-relative).
    pub y: f32,
    /// Horizontal advance.
    pub x_advance: f32,
    /// Source-text byte range (cluster) this glyph covers.
    pub cluster_start: u32,
    pub cluster_end: u32,
}

/// Per-frame shaped composed-cluster table: `face_id → cluster text →
/// shaped glyphs`.
///
/// For grapheme clusters the layout side shapes (emoji ZWJ sequences,
/// combining marks, contextual scripts emitted as `GlyphType::Composite`),
/// this publishes the exact shaped output — glyph ids in exact fonts — so
/// the render thread rasterizes those glyphs instead of re-shaping the
/// cluster text and risking a different font or cluster segmentation.
pub type ShapedClusterTable = HashMap<FaceId, HashMap<Box<str>, Vec<ResolvedGlyph>>>;

/// Exact layout answer for one visible scalar under one face.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ResolvedCharGlyph {
    pub resolved_font_id: ResolvedFontId,
    pub glyph_id: ResolvedGlyphId,
    /// Logical horizontal advance measured from the same opened font.
    pub advance_px: f32,
}

/// Per-frame character glyph table: `face_id → scalar → exact font/glyph`.
///
/// This is the layout side's projection of GNU's realized face/fontset lookup
/// for characters actually on screen. Both primary and fallback characters
/// carry the exact glyph index, so rendering performs neither font selection
/// nor a second charmap lookup.
pub type CharFontTable = HashMap<FaceId, HashMap<char, ResolvedCharGlyph>>;

/// One coherent, borrowed projection of every font binding needed to draw a
/// frame.
///
/// Keeping the catalog generation and all four lookup tables behind one type
/// makes the render boundary compile-time visible: callers cannot accidentally
/// install a new face table while retaining an old generation or fallback
/// table. The wire representation remains the individual fields on
/// `FrameGlyphBuffer`; this is the typed view used once that snapshot is in
/// memory.
#[derive(Clone, Copy, Debug)]
pub struct FrameFontBindings<'a> {
    pub catalog_generation: FontCatalogGeneration,
    pub faces: &'a HashMap<FaceId, Face>,
    pub fonts: &'a ResolvedFontTable,
    pub char_fonts: &'a CharFontTable,
    pub shaped_clusters: &'a ShapedClusterTable,
}

#[cfg(test)]
#[path = "font_test.rs"]
mod tests;
