//! Platform font backend trait (render-boundary design §7).
//!
//! [`crate::font::resolver::FontResolver`] owns GNU-compatible fontset policy
//! and entity scoring. This module is the deliberately smaller platform half:
//! resolve native generic aliases, enumerate concrete candidates, report
//! coverage/spacing/metrics, and preserve an exact identity.

use neomacs_display_protocol::font::{
    FontBackendKind, FontFileAsset, FontMemoryAsset, FontOutlineAsset, ResolvedFontIdentity,
};
use neomacs_display_protocol::geometry::DeviceScale;
use neovm_core::face::{FontSlant, FontWidth};
use std::fmt::{Display, Formatter};
#[cfg(any(target_os = "macos", windows))]
use std::path::Path;

#[cfg(all(unix, not(target_os = "macos")))]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(any(target_os = "macos", windows, test))]
mod native_asset_cache;
#[cfg(windows)]
mod windows;

#[cfg(all(unix, not(target_os = "macos")))]
pub use linux::FontconfigBackend;
#[cfg(target_os = "macos")]
pub use macos::CoreTextBackend;
#[cfg(windows)]
pub use windows::DirectWriteBackend;

/// A non-empty platform font-family name.
///
/// Native APIs are allowed to return missing or empty metadata.  Keeping that
/// state out of the shared resolver means every family that crosses the
/// platform boundary is immediately usable by Lisp and by candidate lookup.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct FontFamilyName(String);

impl FontFamilyName {
    pub fn new(name: impl Into<String>) -> Option<Self> {
        let name = name.into();
        let name = name.trim();
        (!name.is_empty()).then(|| Self(name.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }
}

impl AsRef<str> for FontFamilyName {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl Display for FontFamilyName {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

/// Native design-unit metrics transported with an exact candidate.
///
/// The platform backend attaches these only after shared policy selects the
/// winner, avoiding per-candidate metric work and avoiding a FreeType reopen of
/// a CoreText/DirectWrite selection. Values are scaled to the layout pixel size
/// at the layout boundary.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PlatformFontDesignMetrics {
    pub units_per_em: u32,
    pub ascent: i32,
    pub descent: i32,
    pub line_gap: i32,
    pub max_advance: i32,
    pub space_advance: i32,
    pub average_advance: i32,
}

impl PlatformFontDesignMetrics {
    pub fn at_pixel_size(self, pixel_size: f32) -> Option<crate::font::probe::FontPxMetrics> {
        if self.units_per_em == 0 || !pixel_size.is_finite() || pixel_size <= 0.0 {
            return None;
        }
        let scale = pixel_size / self.units_per_em as f32;
        let scaled = |value: i32| (value as f32 * scale).round() as i32;
        let ascent = scaled(self.ascent).max(0);
        let descent = scaled(self.descent).max(0);
        let line_gap = scaled(self.line_gap).max(0);
        Some(crate::font::probe::FontPxMetrics {
            pixel_size: pixel_size.round().max(1.0) as u32,
            height: (ascent + descent + line_gap).max(1),
            ascent,
            descent,
            max_width: scaled(self.max_advance).max(0),
            space_width: scaled(self.space_advance).max(0),
            average_width: scaled(self.average_advance).max(0),
        })
    }
}

/// Selector metadata for one exact native font.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlatformFontMetadata {
    pub foundry: Option<String>,
    pub family: String,
    pub weight: Option<u16>,
    pub slant: FontSlant,
    pub width: Option<FontWidth>,
    /// GNU/Fontconfig spacing code: proportional=0, dual=90, mono=100,
    /// charcell=110.
    pub spacing: Option<i32>,
    pub design_metrics: Option<PlatformFontDesignMetrics>,
    pub size: PlatformFontSize,
}

/// Size capability of one platform-discovered font entity.
///
/// `Unknown` is intentionally not treated as scalable. Backends such as
/// CoreText and DirectWrite do not always expose fixed strikes during native
/// enumeration, so the shared resolver asks the materializer to classify the
/// exact file before GNU size scoring.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PlatformFontSize {
    Scalable,
    Fixed {
        device_ppem_26_6: u32,
    },
    #[default]
    Unknown,
}

impl PlatformFontSize {
    pub const fn selected_device_ppem_26_6(self) -> Option<u32> {
        match self {
            Self::Fixed { device_ppem_26_6 } => Some(device_ppem_26_6),
            Self::Scalable | Self::Unknown => None,
        }
    }

    pub const fn is_fixed(self) -> bool {
        matches!(self, Self::Fixed { .. })
    }
}

impl PlatformFontMetadata {
    pub fn fixed_spacing_policy(&self) -> neomacs_display_protocol::font::FixedFontSpacing {
        match self.spacing {
            Some(100..) => {
                neomacs_display_protocol::font::FixedFontSpacing::MonospaceOrCharacterCell
            }
            _ => neomacs_display_protocol::font::FixedFontSpacing::ProportionalOrDual,
        }
    }

    pub fn width_class(&self) -> u16 {
        match self.width.unwrap_or(FontWidth::Normal) {
            FontWidth::UltraCondensed => 1,
            FontWidth::ExtraCondensed => 2,
            FontWidth::Condensed => 3,
            FontWidth::SemiCondensed => 4,
            FontWidth::Normal => 5,
            FontWidth::SemiExpanded => 6,
            FontWidth::Expanded => 7,
            FontWidth::ExtraExpanded => 8,
            FontWidth::UltraExpanded => 9,
        }
    }
}

/// How a catalog candidate can be materialized after shared policy chooses it.
///
/// `Native` is deliberately not a renderer asset. The owning backend must
/// turn it into immutable bytes during `finalize_match`; only
/// [`PlatformFontMatch`] can leave the resolver.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlatformFontCandidateLocator {
    File(FontFileAsset),
    Native,
}

impl PlatformFontCandidateLocator {
    pub fn file(&self) -> Option<&FontFileAsset> {
        match self {
            Self::File(asset) => Some(asset),
            Self::Native => None,
        }
    }
}

/// One exact candidate discovered by a platform backend, before expensive
/// native asset materialization.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlatformFontCandidate {
    pub identity: ResolvedFontIdentity,
    pub locator: PlatformFontCandidateLocator,
    pub metadata: PlatformFontMetadata,
}

impl PlatformFontCandidate {
    pub fn file_path(&self) -> Option<&str> {
        match &self.locator {
            PlatformFontCandidateLocator::File(asset) => Some(asset.path()),
            PlatformFontCandidateLocator::Native => None,
        }
    }

    pub fn family(&self) -> &str {
        &self.metadata.family
    }

    pub fn weight(&self) -> Option<u16> {
        self.metadata.weight
    }

    pub fn slant(&self) -> FontSlant {
        self.metadata.slant
    }

    pub fn into_file_match(self) -> Option<PlatformFontMatch> {
        let PlatformFontCandidateLocator::File(asset) = self.locator else {
            return None;
        };
        if FontFileAsset::from_identity(&self.identity).as_ref() != Some(&asset) {
            return None;
        }
        Some(PlatformFontMatch {
            identity: self.identity,
            asset: FontOutlineAsset::File(asset),
            metadata: self.metadata,
        })
    }

    /// Finalize a native-only catalog candidate with immutable outline bytes.
    /// The stable key is checked here so a backend cannot accidentally attach
    /// bytes from one native face to another candidate's identity.
    pub fn into_memory_match(self, asset: FontMemoryAsset) -> Option<PlatformFontMatch> {
        if !matches!(self.locator, PlatformFontCandidateLocator::Native)
            || self.identity.file_path.is_some()
            || asset.key() != self.identity.stable_key
            || asset.face_index() != self.identity.file_face_index()
        {
            return None;
        }
        Some(PlatformFontMatch {
            identity: self.identity,
            asset: FontOutlineAsset::Memory(asset),
            metadata: self.metadata,
        })
    }

    #[cfg(any(target_os = "macos", windows))]
    fn from_platform_file(
        backend: FontBackendKind,
        file: &Path,
        face_index: u32,
        postscript_name: Option<String>,
        variation_coords: Vec<neomacs_display_protocol::font::FontVariationCoord>,
        metadata: PlatformFontMetadata,
    ) -> Option<Self> {
        let file = file.to_str()?;
        Some(Self {
            identity: ResolvedFontIdentity::from_platform_file_with_variations(
                backend,
                file,
                face_index,
                postscript_name,
                variation_coords,
            ),
            locator: PlatformFontCandidateLocator::File(FontFileAsset::new(file, face_index)?),
            metadata,
        })
    }
}

/// One exact, replayable match produced after platform finalization.
///
/// This is deliberately deeper than a file path: collection and variable-font
/// named instances can share a file while representing different drawable
/// fonts. Layout consumes this complete answer and transports its identity to
/// the renderer; neither layer reconstructs selection from family attributes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlatformFontMatch {
    pub identity: ResolvedFontIdentity,
    pub asset: FontOutlineAsset,
    pub metadata: PlatformFontMetadata,
}

impl PlatformFontMatch {
    pub fn file_path(&self) -> Option<&str> {
        self.asset.file().map(FontFileAsset::path)
    }

    pub fn family(&self) -> &str {
        &self.metadata.family
    }

    pub fn weight(&self) -> Option<u16> {
        self.metadata.weight
    }

    pub fn slant(&self) -> FontSlant {
        self.metadata.slant
    }

    pub fn pixel_metrics(&self, pixel_size: f32) -> Option<crate::font::probe::FontPxMetrics> {
        self.metadata
            .design_metrics
            .and_then(|metrics| metrics.at_pixel_size(pixel_size))
    }
}

/// Text direction needed by native fallback APIs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextDirection {
    LeftToRight,
    RightToLeft,
}

/// Requested realization size carried through native candidate selection.
/// The bit-preserving fields make it safe in resolver cache keys; the derived
/// device ppem is the domain GNU scores fixed-size entities in.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct FontSelectionSize {
    layout_px_bits: u32,
    device_scale_bits: u32,
    device_px_26_6: u32,
}

impl FontSelectionSize {
    pub fn new(layout_px: f32, device_scale: DeviceScale) -> Self {
        let layout_px = if layout_px.is_finite() && layout_px > 0.0 {
            layout_px
        } else {
            1.0
        };
        let device_px = layout_px * device_scale.get();
        Self {
            layout_px_bits: layout_px.to_bits(),
            device_scale_bits: device_scale.get().to_bits(),
            device_px_26_6: (device_px * 64.0).round().clamp(1.0, u32::MAX as f32) as u32,
        }
    }

    pub const fn device_px_26_6(self) -> u32 {
        self.device_px_26_6
    }

    /// Nearest whole device-pixel size used by GNU's Cairo/FreeType font
    /// realization.  Keep this conversion on the typed selection size so a
    /// logical font size can never accidentally enter a device-metric probe.
    pub const fn rounded_device_px(self) -> u32 {
        let rounded = self.device_px_26_6.saturating_add(32) / 64;
        if rounded == 0 { 1 } else { rounded }
    }
}

impl TextDirection {
    pub fn for_char(ch: char) -> Self {
        use crate::bidi::BidiClass;
        match crate::bidi::bidi_class(ch) {
            BidiClass::R | BidiClass::AL | BidiClass::RLE | BidiClass::RLO | BidiClass::RLI => {
                Self::RightToLeft
            }
            _ => Self::LeftToRight,
        }
    }
}

/// Discovery request passed from shared policy to one native backend.
#[derive(Clone, Debug)]
pub struct FontCandidateQuery {
    pub scope: FontCandidateScope,
    pub required: RequiredFontCoverage,
    pub charset_ranges: Vec<(u32, u32)>,
    pub languages: Vec<String>,
    pub requested_weight: u16,
    pub requested_slant: FontSlant,
    pub requested_width: FontWidth,
    pub direction: TextDirection,
}

impl FontCandidateQuery {
    /// Whether one native candidate satisfies every GNU coverage constraint.
    ///
    /// Keeping this traversal in shared code prevents platform adapters from
    /// silently interpreting a registry/repertory range as "any character".
    /// Invalid Unicode scalar values are rejected instead of being passed to
    /// a native API with platform-dependent behavior.
    pub fn coverage_is_satisfied_by(&self, mut supports: impl FnMut(u32) -> bool) -> bool {
        if !self.required.chars().all(|ch| supports(u32::from(ch))) {
            return false;
        }
        self.charset_ranges.iter().all(|&(first, second)| {
            let from = first.min(second);
            let to = first.max(second);
            (from..=to).all(|codepoint| char::from_u32(codepoint).is_some() && supports(codepoint))
        })
    }

    /// Whether a catalog's supported-language list satisfies this query.
    /// Multiple GNU language hints are alternatives, matching the union used
    /// by the Fontconfig adapter.
    pub fn languages_are_satisfied_by(&self, mut supports: impl FnMut(&str) -> bool) -> bool {
        self.languages.is_empty() || self.languages.iter().any(|language| supports(language))
    }
}

/// Required text coverage for native catalog enumeration.
///
/// `Text` is non-empty by construction. Keeping it distinct from `Character`
/// lets native fallback APIs receive a whole grapheme/composition range while
/// Linux can retain its optimized one-character Fontconfig query.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum RequiredFontCoverage {
    #[default]
    Any,
    Character(char),
    Text(RequiredFontText),
}

impl RequiredFontCoverage {
    pub fn for_text(text: impl Into<String>) -> Option<Self> {
        RequiredFontText::new(text).map(Self::Text)
    }

    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Any => None,
            Self::Character(_) => None,
            Self::Text(text) => Some(text.as_str()),
        }
    }

    pub fn single_char(&self) -> Option<char> {
        match self {
            Self::Character(ch) => Some(*ch),
            Self::Any | Self::Text(_) => None,
        }
    }

    pub fn chars(&self) -> impl Iterator<Item = char> + '_ {
        let text = match self {
            Self::Any => None,
            Self::Character(ch) => Some(RequiredChars::Character(Some(*ch))),
            Self::Text(text) => Some(RequiredChars::Text(text.as_str().chars())),
        };
        text.into_iter().flatten()
    }
}

enum RequiredChars<'a> {
    Character(Option<char>),
    Text(std::str::Chars<'a>),
}

impl Iterator for RequiredChars<'_> {
    type Item = char;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Character(ch) => ch.take(),
            Self::Text(chars) => chars.next(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RequiredFontText(String);

impl RequiredFontText {
    pub fn new(text: impl Into<String>) -> Option<Self> {
        let text = text.into();
        (!text.is_empty()).then_some(Self(text))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Which native candidate population one query addresses.
///
/// `All` and `NativeFallback` were previously both encoded as `family: None`.
/// Keeping them distinct makes every backend implement GNU `list-fonts`
/// enumeration separately from per-character platform fallback.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FontCandidateScope {
    Family(FontFamilyName),
    All,
    NativeFallback { base_family: FontFamilyName },
}

impl FontCandidateScope {
    pub fn queried_family(&self) -> Option<&str> {
        match self {
            Self::Family(family) => Some(family.as_str()),
            Self::All | Self::NativeFallback { .. } => None,
        }
    }
}

/// One raw candidate plus attributes used exclusively by shared scoring.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FontCandidate {
    pub matched: PlatformFontCandidate,
}

pub trait FontBackend: Send {
    /// Native platform implementation represented by this adapter.
    fn kind(&self) -> FontBackendKind;

    /// Enumerate the font families visible to this platform adapter.
    ///
    /// This is GNU's `font_driver::list_family` operation.  It is deliberately
    /// required rather than defaulted: a new platform backend must implement
    /// the public `font-family-list` contract before it can compile.
    fn list_families(&self) -> Vec<FontFamilyName>;

    /// Resolve a generic family alias ("monospace", "sans-serif", …) to the
    /// concrete family the platform would pick. Concrete names pass through
    /// unchanged.
    fn resolve_family(&self, family: &str) -> String;

    /// Whether the platform considers this family monospace-preferring
    /// (drives fallback ordering for per-char matches).
    fn family_prefers_monospace(&self, family: &str) -> bool;

    /// Enumerate candidates for one shared-policy pass.
    ///
    /// Ordering must preserve the native discovery/cascade order. The backend
    /// may filter by coverage but must not score weight/slant/spacing; that is
    /// [`crate::font::resolver::FontResolver`]'s responsibility.
    fn list_candidates(&self, query: &FontCandidateQuery) -> Vec<FontCandidate>;

    /// Enrich the selected candidate into an exact renderer identity.
    ///
    /// Discovery deliberately asks only for policy-relevant metadata because
    /// Fontconfig changes enumeration order when renderer metadata is included
    /// in `FcFontList`.  This hook runs once after shared policy chooses a
    /// candidate, never while candidates are being scored.
    fn finalize_match(&self, matched: PlatformFontCandidate) -> Option<PlatformFontMatch> {
        matched.into_file_match()
    }

    /// Native metrics for the already selected exact candidate.
    ///
    /// Called once on a resolver cache miss, never for every enumerated
    /// candidate. Backends whose ordinary metric probe already consumes the
    /// exact identity (Fontconfig/FreeType) may return `None`.
    fn design_metrics(&self, _matched: &PlatformFontMatch) -> Option<PlatformFontDesignMetrics> {
        None
    }

    /// Observe whether the native catalog changed since the previous safe
    /// point. Implementations must coalesce OS callbacks or rate-limit native
    /// polling; this method is called from the evaluator redisplay path.
    fn poll_catalog_change(&mut self) -> crate::font::catalog::FontCatalogChange;

    /// Advance to a fresh view of the platform font catalog.
    ///
    /// Native font objects can disappear or be replaced when the process font
    /// catalog changes. Stateful backends must discard lookup metadata here;
    /// already-published immutable assets remain owned by their frame users.
    /// This is required even for stateless backends so adding backend-local
    /// caching always requires an explicit generation-lifecycle decision.
    fn advance_catalog_generation(&mut self);
}

/// The platform's default backend.
pub fn default_font_backend() -> Box<dyn FontBackend> {
    std::cfg_select! {
        target_os = "macos" => {
            Box::new(CoreTextBackend::default())
        }
        windows => {
            Box::new(DirectWriteBackend::default())
        }
        all(unix, not(target_os = "macos")) => {
            Box::new(FontconfigBackend::default())
        }
        _ => compile_error!("Neomacs has no native font catalog for this target"),
    }
}

#[cfg(test)]
mod tests {
    use neomacs_display_protocol::font::FontBackendKind;

    #[test]
    fn default_backend_matches_the_build_target() {
        let backend = super::default_font_backend();
        #[cfg(target_os = "linux")]
        assert_eq!(backend.kind(), FontBackendKind::Fontconfig);
        #[cfg(target_os = "macos")]
        assert_eq!(backend.kind(), FontBackendKind::CoreText);
        #[cfg(windows)]
        assert_eq!(backend.kind(), FontBackendKind::DirectWrite);
    }

    #[test]
    fn native_design_metrics_scale_at_the_layout_boundary() {
        let metrics = super::PlatformFontDesignMetrics {
            units_per_em: 1_000,
            ascent: 800,
            descent: 200,
            line_gap: 100,
            max_advance: 700,
            space_advance: 500,
            average_advance: 600,
        }
        .at_pixel_size(20.0)
        .expect("valid design metrics");

        assert_eq!(metrics.ascent, 16);
        assert_eq!(metrics.descent, 4);
        assert_eq!(metrics.height, 22);
        assert_eq!(metrics.max_width, 14);
        assert_eq!(metrics.space_width, 10);
        assert_eq!(metrics.average_width, 12);
    }

    #[test]
    fn native_fallback_direction_uses_unicode_bidi_class() {
        assert_eq!(
            super::TextDirection::for_char('א'),
            super::TextDirection::RightToLeft
        );
        assert_eq!(
            super::TextDirection::for_char('م'),
            super::TextDirection::RightToLeft
        );
        assert_eq!(
            super::TextDirection::for_char('好'),
            super::TextDirection::LeftToRight
        );
    }

    #[test]
    fn catalog_coverage_requires_the_character_and_every_normalized_range() {
        let query = super::FontCandidateQuery {
            scope: super::FontCandidateScope::All,
            required: super::RequiredFontCoverage::Character('λ'),
            charset_ranges: vec![(0x43, 0x41)],
            languages: Vec::new(),
            requested_weight: 400,
            requested_slant: neovm_core::face::FontSlant::Normal,
            requested_width: neovm_core::face::FontWidth::Normal,
            direction: super::TextDirection::LeftToRight,
        };

        assert!(
            query
                .coverage_is_satisfied_by(|codepoint| { matches!(codepoint, 0x41..=0x43 | 0x3bb) })
        );
        assert!(
            !query
                .coverage_is_satisfied_by(|codepoint| { matches!(codepoint, 0x41 | 0x43 | 0x3bb) })
        );
        assert!(!query.coverage_is_satisfied_by(|codepoint| { matches!(codepoint, 0x41..=0x43) }));
    }

    #[test]
    fn native_fallback_text_is_non_empty_and_requires_the_whole_cluster() {
        assert!(super::RequiredFontCoverage::for_text("").is_none());
        let required = super::RequiredFontCoverage::for_text("👩‍💻").unwrap();
        let query = super::FontCandidateQuery {
            scope: super::FontCandidateScope::All,
            required,
            charset_ranges: Vec::new(),
            languages: Vec::new(),
            requested_weight: 400,
            requested_slant: neovm_core::face::FontSlant::Normal,
            requested_width: neovm_core::face::FontWidth::Normal,
            direction: super::TextDirection::LeftToRight,
        };

        assert!(query.coverage_is_satisfied_by(|codepoint| {
            matches!(codepoint, 0x1f469 | 0x200d | 0x1f4bb)
        }));
        assert!(
            !query.coverage_is_satisfied_by(|codepoint| { matches!(codepoint, 0x1f469 | 0x1f4bb) })
        );
    }

    #[test]
    fn catalog_language_hints_are_alternatives() {
        let query = super::FontCandidateQuery {
            scope: super::FontCandidateScope::All,
            required: super::RequiredFontCoverage::Any,
            charset_ranges: Vec::new(),
            languages: vec!["ja".into(), "zh-hans".into()],
            requested_weight: 400,
            requested_slant: neovm_core::face::FontSlant::Normal,
            requested_width: neovm_core::face::FontWidth::Normal,
            direction: super::TextDirection::LeftToRight,
        };

        assert!(query.languages_are_satisfied_by(|language| language == "zh-hans"));
        assert!(!query.languages_are_satisfied_by(|language| language == "ko"));
    }
}
