//! GNU-compatible font query policy shared by every platform catalog.
//!
//! Registry, repertory, and language semantics are Emacs behavior, not
//! Fontconfig behavior. This module translates a stored Lisp font spec into a
//! typed native-catalog query without naming any operating-system backend.

use crate::font_backend::FontSignatureRequirement;
use neomacs_display_protocol::font::FontBackendKind;
use neovm_core::emacs_core::font::FrozenFontFamilies;
use neovm_core::emacs_core::fontset::{FontsetRenderingSpec, StoredFontSpec, native_font_name};
use neovm_core::emacs_core::regex::{EmacsRegexpError, predicate_match_ignore_case};
use neovm_core::heap_types::LispString;
use std::borrow::Cow;

/// A fontset's explicit family constrains the search. Without one, GNU
/// `font_find_for_lface` inherits the fully merged face family and may fall
/// back outside it. Keeping these cases distinct prevents a frame-default
/// family from masquerading as a fontset rule.
pub(super) enum FontFamilySource<'a> {
    Fontset(Cow<'a, str>),
    Face(&'a str),
}

impl<'a> FontFamilySource<'a> {
    pub(super) fn for_spec(face_family: &'a str, spec: &StoredFontSpec) -> Self {
        match spec.family {
            Some(family) => Self::Fontset(native_font_name(family)),
            None => Self::Face(face_family),
        }
    }

    /// Read evaluator policy before handing native font work to another thread.
    pub(super) fn capture(self) -> CapturedFontFamilyPolicy {
        match self {
            Self::Fontset(family) => CapturedFontFamilyPolicy::Explicit(family.into_owned()),
            Self::Face("") => CapturedFontFamilyPolicy::Inherited(std::iter::empty().collect()),
            Self::Face(family) => CapturedFontFamilyPolicy::Inherited(
                neovm_core::emacs_core::font::capture_alternative_font_families(family)
                    .unwrap_or_else(|_| std::iter::once(family.to_owned()).collect()),
            ),
        }
    }
}

/// Owned family policy. Native alias resolution is intentionally deferred.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum CapturedFontFamilyPolicy {
    Explicit(String),
    Inherited(FrozenFontFamilies),
}

impl CapturedFontFamilyPolicy {
    /// Resolve only the next visited family. A first winner must not allocate
    /// or resolve the unused suffix of its owner-shared alternative list.
    pub(super) fn search_order(
        &self,
        resolve: impl Fn(&str) -> String,
    ) -> impl Iterator<Item = FontFamilyCandidate<'_>> {
        let state = match self {
            Self::Explicit(family) => FamilySearchState::Explicit(family),
            Self::Inherited(families) => FamilySearchState::Inherited(families.iter()),
        };
        FamilySearch { state, resolve }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum FontFamilyCandidate<'a> {
    Family(Cow<'a, str>),
    NativeFallback,
}

impl FontFamilyCandidate<'_> {
    /// Native catalog APIs use an absent family for the unconstrained pass.
    pub(super) fn as_family(&self) -> Option<&str> {
        match self {
            Self::Family(family) => Some(family),
            Self::NativeFallback => None,
        }
    }
}

#[cfg(test)]
impl From<FontFamilyCandidate<'_>> for Option<String> {
    fn from(candidate: FontFamilyCandidate<'_>) -> Self {
        match candidate {
            FontFamilyCandidate::Family(family) => Some(family.into_owned()),
            FontFamilyCandidate::NativeFallback => None,
        }
    }
}

/// An alias is tried before its raw spelling; explicit fontset families do not
/// permit the inherited policy's final unconstrained pass. The variants encode
/// the outstanding continuation without a fallback flag or an owned list.
#[derive(Debug)]
enum FamilySearchState<'a, I> {
    Explicit(&'a str),
    Inherited(I),
    Original { family: &'a str, remaining: I },
    Exhausted,
}

#[derive(Debug)]
struct FamilySearch<'a, I, F> {
    state: FamilySearchState<'a, I>,
    resolve: F,
}

impl<'a, I, F> Iterator for FamilySearch<'a, I, F>
where
    I: Iterator<Item = &'a str>,
    F: Fn(&str) -> String,
{
    type Item = FontFamilyCandidate<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        match std::mem::replace(&mut self.state, FamilySearchState::Exhausted) {
            FamilySearchState::Explicit(family) => Some(FontFamilyCandidate::Family(Cow::Owned(
                (self.resolve)(family),
            ))),
            FamilySearchState::Inherited(mut remaining) => match remaining.next() {
                Some(family) => {
                    let resolved = (self.resolve)(family);
                    if resolved == family {
                        self.state = FamilySearchState::Inherited(remaining);
                        Some(FontFamilyCandidate::Family(Cow::Borrowed(family)))
                    } else {
                        self.state = FamilySearchState::Original { family, remaining };
                        Some(FontFamilyCandidate::Family(Cow::Owned(resolved)))
                    }
                }
                None => Some(FontFamilyCandidate::NativeFallback),
            },
            FamilySearchState::Original { family, remaining } => {
                self.state = FamilySearchState::Inherited(remaining);
                Some(FontFamilyCandidate::Family(Cow::Borrowed(family)))
            }
            FamilySearchState::Exhausted => None,
        }
    }
}

impl<'a, I, F> std::iter::FusedIterator for FamilySearch<'a, I, F>
where
    I: Iterator<Item = &'a str>,
    F: Fn(&str) -> String,
{
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct LanguageTag(String);

impl LanguageTag {
    pub fn new(value: impl AsRef<str>) -> Option<Self> {
        let value = value.as_ref().trim();
        (!value.is_empty()).then(|| Self(value.to_ascii_lowercase()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Coverage required during native entity discovery.
///
/// `Any` is distinct from an empty range collection, so adapters cannot
/// accidentally treat a deliberately unconstrained GNU registry as a broken
/// query.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CoverageRequirement {
    Any,
    Ranges(Vec<(u32, u32)>),
}

impl CoverageRequirement {
    pub fn ranges(&self) -> &[(u32, u32)] {
        match self {
            Self::Any => &[],
            Self::Ranges(ranges) => ranges,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeQueryConstraints {
    representative_char: char,
    coverage: CoverageRequirement,
    languages: Vec<LanguageTag>,
}

impl NativeQueryConstraints {
    pub fn representative_char(&self) -> char {
        self.representative_char
    }

    pub fn coverage(&self) -> &CoverageRequirement {
        &self.coverage
    }

    pub fn languages(&self) -> &[LanguageTag] {
        &self.languages
    }
}

/// One borrowed naming view feeds both live and detached registry semantics.
pub(super) struct FontPolicySpec<'a> {
    registry: Option<Cow<'a, str>>,
    pub(super) language: Option<Cow<'a, str>>,
    pub(super) weight: Option<neovm_core::face::FontWeight>,
    pub(super) slant: Option<neovm_core::face::FontSlant>,
    pub(super) width: Option<neovm_core::face::FontWidth>,
}

impl FontPolicySpec<'_> {
    pub(super) fn signature_requirement(
        &self,
        backend: FontBackendKind,
    ) -> Option<FontSignatureRequirement> {
        registry_signature_requirement(backend, self.registry.as_deref())
    }

    pub(super) fn for_backend(mut self, backend: FontBackendKind) -> Self {
        self.registry = self
            .registry
            .filter(|registry| registry_needs_native_constraints(backend, registry));
        self
    }
}
impl<'a> From<&'a StoredFontSpec> for FontPolicySpec<'a> {
    fn from(spec: &'a StoredFontSpec) -> Self {
        Self {
            registry: spec.registry.map(native_font_name),
            language: spec.lang.map(native_font_name),
            weight: spec.weight,
            slant: spec.slant,
            width: spec.width,
        }
    }
}
impl<'a> From<&'a FontsetRenderingSpec> for FontPolicySpec<'a> {
    fn from(spec: &'a FontsetRenderingSpec) -> Self {
        Self {
            registry: spec.registry.as_deref().map(Cow::Borrowed),
            language: spec.lang.as_deref().map(Cow::Borrowed),
            weight: spec.weight,
            slant: spec.slant,
            width: spec.width,
        }
    }
}

pub struct GnuFontPolicy;

/// A valid but unsupported registry is a fontset-rule miss, not an
/// unconstrained character query and not a regexp compilation failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NativeFontPolicy {
    UnsupportedRegistry,
    Supported(NativeQueryConstraints),
}

impl GnuFontPolicy {
    pub fn constraints_for_entity(
        spec: &StoredFontSpec,
    ) -> Result<NativeFontPolicy, EmacsRegexpError> {
        let names = FontPolicySpec::from(spec);
        let registry = parse_registry(names.registry.as_deref())?;
        Ok(Self::constraints_for_parsed(
            &names,
            registry.representative(),
            registry,
        ))
    }

    pub fn constraints_for_character(
        spec: &StoredFontSpec,
        required_char: char,
    ) -> Result<NativeFontPolicy, EmacsRegexpError> {
        Self::constraints_for_spec(spec, required_char)
    }

    pub fn constraints_for_spec(
        spec: &StoredFontSpec,
        required_char: char,
    ) -> Result<NativeFontPolicy, EmacsRegexpError> {
        Self::constraints_for_names(&FontPolicySpec::from(spec), required_char)
    }

    pub(super) fn constraints_for_names(
        names: &FontPolicySpec<'_>,
        required_char: char,
    ) -> Result<NativeFontPolicy, EmacsRegexpError> {
        let registry = parse_registry(names.registry.as_deref())?;
        Ok(Self::constraints_for_parsed(names, required_char, registry))
    }

    fn constraints_for_parsed(
        names: &FontPolicySpec<'_>,
        required_char: char,
        registry: ParsedRegistry,
    ) -> NativeFontPolicy {
        if matches!(registry, ParsedRegistry::Unmatched) {
            return NativeFontPolicy::UnsupportedRegistry;
        }
        let representative_char = registry.representative();
        let coverage_ranges = registry.ranges(required_char);
        let languages = combined_query_langs(registry.language(), names.language.as_deref())
            .into_iter()
            .filter_map(LanguageTag::new)
            .collect();
        NativeFontPolicy::Supported(NativeQueryConstraints {
            representative_char,
            coverage: if coverage_ranges.is_empty() {
                CoverageRequirement::Any
            } else {
                CoverageRequirement::Ranges(coverage_ranges)
            },
            languages,
        })
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct RegistryHint {
    pub(crate) name: &'static str,
    pub(crate) uniquifiers: &'static [u32],
    pub(crate) lang: Option<&'static str>,
}

// Mirrors the registry hints GNU Emacs uses to translate XLFD registries into
// Unicode coverage constraints. The hints belong to shared Emacs policy even
// though GNU's current table lives in ftfont.c.
const REGISTRY_HINTS: &[RegistryHint] = &[
    RegistryHint {
        name: "iso8859-1",
        uniquifiers: &[0x00A0, 0x00A1, 0x00B4, 0x00BC, 0x00D0],
        lang: None,
    },
    RegistryHint {
        name: "iso8859-2",
        uniquifiers: &[0x00A0, 0x010E],
        lang: None,
    },
    RegistryHint {
        name: "iso8859-3",
        uniquifiers: &[0x00A0, 0x0108],
        lang: None,
    },
    RegistryHint {
        name: "iso8859-4",
        uniquifiers: &[0x00A0, 0x00AF, 0x0128, 0x0156, 0x02C7],
        lang: None,
    },
    RegistryHint {
        name: "iso8859-5",
        uniquifiers: &[0x00A0, 0x0401],
        lang: None,
    },
    RegistryHint {
        name: "iso8859-6",
        uniquifiers: &[0x00A0, 0x060C],
        lang: None,
    },
    RegistryHint {
        name: "iso8859-7",
        uniquifiers: &[0x00A0, 0x0384],
        lang: None,
    },
    RegistryHint {
        name: "iso8859-8",
        uniquifiers: &[0x00A0, 0x05D0],
        lang: None,
    },
    RegistryHint {
        name: "iso8859-9",
        uniquifiers: &[0x00A0, 0x00A1, 0x00BC, 0x011E],
        lang: None,
    },
    RegistryHint {
        name: "iso8859-10",
        uniquifiers: &[0x00A0, 0x00D0, 0x0128, 0x2015],
        lang: None,
    },
    RegistryHint {
        name: "iso8859-11",
        uniquifiers: &[0x00A0, 0x0E01],
        lang: None,
    },
    RegistryHint {
        name: "iso8859-13",
        uniquifiers: &[0x00A0, 0x201C],
        lang: None,
    },
    RegistryHint {
        name: "iso8859-14",
        uniquifiers: &[0x00A0, 0x0174],
        lang: None,
    },
    RegistryHint {
        name: "iso8859-15",
        uniquifiers: &[0x00A0, 0x00A1, 0x00D0, 0x0152],
        lang: None,
    },
    RegistryHint {
        name: "iso8859-16",
        uniquifiers: &[0x00A0, 0x0218],
        lang: None,
    },
    RegistryHint {
        name: "gb2312.1980-0",
        uniquifiers: &[0x4E13],
        lang: Some("zh-cn"),
    },
    RegistryHint {
        name: "big5-0",
        uniquifiers: &[0x9C21],
        lang: Some("zh-tw"),
    },
    RegistryHint {
        name: "jisx0208.1983-0",
        uniquifiers: &[0x4E55],
        lang: Some("ja"),
    },
    RegistryHint {
        name: "ksc5601.1985-0",
        uniquifiers: &[0xAC00],
        lang: Some("ko"),
    },
    RegistryHint {
        name: "cns11643.1992-1",
        uniquifiers: &[0xFE32],
        lang: Some("zh-tw"),
    },
    RegistryHint {
        name: "cns11643.1992-2",
        uniquifiers: &[0x4E33, 0x7934],
        lang: None,
    },
    RegistryHint {
        name: "cns11643.1992-3",
        uniquifiers: &[0x201A9],
        lang: None,
    },
    RegistryHint {
        name: "cns11643.1992-4",
        uniquifiers: &[0x20057],
        lang: None,
    },
    RegistryHint {
        name: "cns11643.1992-5",
        uniquifiers: &[0x20000],
        lang: None,
    },
    RegistryHint {
        name: "cns11643.1992-6",
        uniquifiers: &[0x20003],
        lang: None,
    },
    RegistryHint {
        name: "cns11643.1992-7",
        uniquifiers: &[0x20055],
        lang: None,
    },
    RegistryHint {
        name: "gbk-0",
        uniquifiers: &[0x4E06],
        lang: Some("zh-cn"),
    },
    RegistryHint {
        name: "jisx0212.1990-0",
        uniquifiers: &[0x4E44],
        lang: None,
    },
    RegistryHint {
        name: "jisx0213.2000-1",
        uniquifiers: &[0xFA10],
        lang: Some("ja"),
    },
    RegistryHint {
        name: "jisx0213.2000-2",
        uniquifiers: &[0xFA49],
        lang: None,
    },
    RegistryHint {
        name: "jisx0213.2004-1",
        uniquifiers: &[0x20B9F],
        lang: None,
    },
    RegistryHint {
        name: "viscii1.1-1",
        uniquifiers: &[0x1EA0, 0x1EAE, 0x1ED2],
        lang: Some("vi"),
    },
    RegistryHint {
        name: "tis620.2529-1",
        uniquifiers: &[0x0E01],
        lang: Some("th"),
    },
    RegistryHint {
        name: "microsoft-cp1251",
        uniquifiers: &[0x0401, 0x0490],
        lang: Some("ru"),
    },
    RegistryHint {
        name: "koi8-r",
        uniquifiers: &[0x0401, 0x2219],
        lang: Some("ru"),
    },
    RegistryHint {
        name: "mulelao-1",
        uniquifiers: &[0x0E81],
        lang: Some("lo"),
    },
    RegistryHint {
        name: "unicode-sip",
        uniquifiers: &[0x20000],
        lang: None,
    },
];

#[derive(Clone, Copy)]
enum ParsedRegistry {
    Unconstrained,
    Legacy(&'static RegistryHint),
    Unmatched,
}

impl ParsedRegistry {
    fn representative(self) -> char {
        match self {
            Self::Legacy(hint) => hint
                .uniquifiers
                .first()
                .copied()
                .and_then(char::from_u32)
                .unwrap_or('a'),
            Self::Unconstrained | Self::Unmatched => 'a',
        }
    }

    fn language(self) -> Option<&'static str> {
        match self {
            Self::Legacy(hint) => hint.lang,
            Self::Unconstrained | Self::Unmatched => None,
        }
    }

    fn characters(self, ch: char) -> Vec<u32> {
        match self {
            Self::Legacy(hint) if !hint.uniquifiers.is_empty() => hint.uniquifiers.to_vec(),
            Self::Legacy(_) | Self::Unconstrained | Self::Unmatched => vec![ch as u32],
        }
    }

    fn ranges(self, ch: char) -> Vec<(u32, u32)> {
        if matches!(self, Self::Unconstrained) {
            return Vec::new();
        }
        let mut codepoints = self.characters(ch);
        if !codepoints.contains(&(ch as u32)) {
            codepoints.push(ch as u32);
        }
        coalesce_ranges(
            codepoints
                .into_iter()
                .map(|codepoint| (codepoint, codepoint))
                .collect(),
        )
    }
}

fn parse_registry(registry: Option<&str>) -> Result<ParsedRegistry, EmacsRegexpError> {
    match registry {
        None => Ok(ParsedRegistry::Unconstrained),
        // ftfont_spec_pattern compares these registry symbols exactly before
        // applying the case-insensitive legacy charset regexp lookup.
        Some(registry) if ftfont_registry_uses_unconstrained_charset(registry) => {
            Ok(ParsedRegistry::Unconstrained)
        }
        Some(registry) => Ok(match registry_hint(registry)? {
            Some(hint) => ParsedRegistry::Legacy(hint),
            None => ParsedRegistry::Unmatched,
        }),
    }
}

#[cfg(test)]
pub(crate) fn representative_char_for_spec(
    spec: &StoredFontSpec,
) -> Result<char, EmacsRegexpError> {
    Ok(parse_registry(spec.registry.map(native_font_name).as_deref())?.representative())
}

#[cfg(test)]
pub(crate) fn query_charset_ranges(
    spec: &StoredFontSpec,
    ch: char,
) -> Result<Vec<(u32, u32)>, EmacsRegexpError> {
    Ok(parse_registry(spec.registry.map(native_font_name).as_deref())?.ranges(ch))
}

fn coalesce_ranges(mut ranges: Vec<(u32, u32)>) -> Vec<(u32, u32)> {
    ranges.sort_unstable_by_key(|(from, to)| (*from, *to));
    let mut merged: Vec<(u32, u32)> = Vec::with_capacity(ranges.len());
    for (first, second) in ranges {
        let from = first.min(second);
        let to = first.max(second);
        match merged.last_mut() {
            Some((current_from, current_to)) if from <= current_to.saturating_add(1) => {
                *current_from = (*current_from).min(from);
                *current_to = (*current_to).max(to);
            }
            _ => merged.push((from, to)),
        }
    }
    merged
}

pub(crate) fn combined_query_langs(
    registry_lang: Option<&str>,
    spec_lang: Option<&str>,
) -> Vec<String> {
    let mut langs = Vec::new();
    for lang in [registry_lang, spec_lang] {
        let Some(lang) = lang.map(str::trim).filter(|lang| !lang.is_empty()) else {
            continue;
        };
        let lang = lang.to_ascii_lowercase();
        if !langs.contains(&lang) {
            langs.push(lang);
        }
    }
    langs
}

fn ftfont_registry_uses_unconstrained_charset(registry: &str) -> bool {
    matches!(registry, "ascii-0" | "iso10646-1" | "unicode-bmp")
}

#[cfg(test)]
pub(crate) fn registry_query_chars(
    registry: Option<&str>,
    ch: char,
) -> Result<Vec<u32>, EmacsRegexpError> {
    Ok(parse_registry(registry)?.characters(ch))
}

pub(crate) fn registry_hint(
    registry: &str,
) -> Result<Option<&'static RegistryHint>, EmacsRegexpError> {
    // ftfont_get_charset copies the registry bytes, escaping only periods,
    // expanding stars, and replacing question marks. Other Emacs regexp
    // operators and unanchored matching remain meaningful.
    let mut regexp = Vec::with_capacity(registry.len().saturating_mul(2));
    for &byte in registry.as_bytes() {
        match byte {
            b'.' => regexp.extend_from_slice(b"\\."),
            b'*' => regexp.extend_from_slice(b".*"),
            b'?' => regexp.push(b'.'),
            _ => regexp.push(byte),
        }
    }
    // GNU's fast_c_string_match forces the pattern unibyte and uses its ASCII
    // canon table. Registry table targets are all ASCII; retaining raw pattern
    // bytes also avoids folding a wide character into an ASCII registry name.
    let regexp = LispString::from_unibyte(regexp);
    for hint in REGISTRY_HINTS {
        if predicate_match_ignore_case(&regexp, hint.name)? {
            return Ok(Some(hint));
        }
    }
    Ok(None)
}

/// ftfont_spec_pattern uses exact symbol shortcuts for Unicode registries;
/// the legacy charset table separately accepts case-insensitive patterns.
#[cfg(test)]
pub(crate) fn registry_supported_for_listing(registry: &str) -> Result<bool, EmacsRegexpError> {
    Ok(registry_listing_preparation(Some(registry))?.is_some())
}

pub(super) enum RegistryListingPreparation {
    WithCharset,
    WithoutCharset,
}

pub(super) fn registry_listing_preparation(
    registry: Option<&str>,
) -> Result<Option<RegistryListingPreparation>, EmacsRegexpError> {
    Ok(match parse_registry(registry)? {
        ParsedRegistry::Unconstrained => Some(RegistryListingPreparation::WithoutCharset),
        ParsedRegistry::Legacy(_) => Some(RegistryListingPreparation::WithCharset),
        ParsedRegistry::Unmatched => None,
    })
}

pub(crate) fn registry_language(registry: &str) -> Result<Option<&'static str>, EmacsRegexpError> {
    Ok(registry_hint(registry)?.and_then(|hint| hint.lang))
}

pub(super) fn windows_registry_shortcut(registry: &str) -> bool {
    matches!(
        registry,
        "unknown" | "ascii-0" | "iso10646-1" | "unicode-bmp" | "unicode-sip"
    )
}

/// w32font's Unicode registry symbols select native signature masks, not
/// Fontconfig representative characters or a requirement to cover all glyphs.
pub(super) fn registry_signature_requirement(
    backend: FontBackendKind,
    registry: Option<&str>,
) -> Option<FontSignatureRequirement> {
    if backend != FontBackendKind::DirectWrite {
        return None;
    }
    match registry {
        Some("iso10646-1") => Some(FontSignatureRequirement::Unicode),
        Some("unicode-bmp") => Some(FontSignatureRequirement::Bmp),
        Some("unicode-sip") => Some(FontSignatureRequirement::Sip),
        _ => None,
    }
}

/// Retain the existing recognized-legacy approximation on non-Fontconfig
/// adapters. Their registry names are not Fontconfig regular expressions.
pub(super) fn legacy_backend_registry_hint(registry: &str) -> Option<&'static RegistryHint> {
    REGISTRY_HINTS
        .iter()
        .find(|hint| wildcard_casefold_match(registry, hint.name))
}

pub(super) fn registry_needs_native_constraints(backend: FontBackendKind, registry: &str) -> bool {
    match backend {
        FontBackendKind::Fontconfig => true,
        FontBackendKind::DirectWrite => !windows_registry_shortcut(registry),
        // NS maps unknown prefixes to an empty script and admits all families.
        // Exact ns-reg-to-script/90% script coverage is not captured here;
        // preserve only the native adapter's existing recognized legacy hints.
        FontBackendKind::CoreText => legacy_backend_registry_hint(registry).is_some(),
        // Future adapters need their own registry contract, not FT constraints.
        _ => false,
    }
}

pub(crate) fn wildcard_casefold_match(pattern: &str, text: &str) -> bool {
    let pattern = pattern.to_ascii_lowercase().into_bytes();
    let text = text.to_ascii_lowercase().into_bytes();
    let (mut p, mut t) = (0usize, 0usize);
    let (mut star, mut star_t) = (None, 0usize);
    while t < text.len() {
        if p < pattern.len() && (pattern[p] == b'?' || pattern[p] == text[t]) {
            p += 1;
            t += 1;
            continue;
        }
        if p < pattern.len() && pattern[p] == b'*' {
            star = Some(p);
            p += 1;
            star_t = t;
            continue;
        }
        if let Some(star_pos) = star {
            p = star_pos + 1;
            star_t += 1;
            t = star_t;
            continue;
        }
        return false;
    }
    while p < pattern.len() && pattern[p] == b'*' {
        p += 1;
    }
    p == pattern.len()
}

#[cfg(test)]
#[path = "policy/tests/policy_test.rs"]
mod tests;

#[cfg(test)]
#[path = "policy/tests/legacy_registry_pattern_test.rs"]
mod legacy_registry_pattern_test;
