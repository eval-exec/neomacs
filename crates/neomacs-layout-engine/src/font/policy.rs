//! GNU-compatible font query policy shared by every platform catalog.
//!
//! Registry, repertory, and language semantics are Emacs behavior, not
//! Fontconfig behavior. This module translates a stored Lisp font spec into a
//! typed native-catalog query without naming any operating-system backend.

use crate::font::probe::{OtfCapability, OtfScript};
use neovm_core::emacs_core::font::FrozenFontFamilies;
use neovm_core::emacs_core::fontset::{
    FontsetRenderingSpec, StoredFontSpec, StoredOtfFeatures, StoredOtfLangsys, StoredOtfSpec,
    native_font_name,
};
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
    pub(super) registry: Option<Cow<'a, str>>,
    language: Option<Cow<'a, str>>,
    pub(super) weight: Option<neovm_core::face::FontWeight>,
    pub(super) slant: Option<neovm_core::face::FontSlant>,
    pub(super) width: Option<neovm_core::face::FontWidth>,
    /// GNU `:otf`: a candidate must advertise these OpenType features.
    pub(super) otf: Option<&'a StoredOtfSpec>,
}
impl<'a> From<&'a StoredFontSpec> for FontPolicySpec<'a> {
    fn from(spec: &'a StoredFontSpec) -> Self {
        Self {
            registry: spec.registry.map(native_font_name),
            language: spec.lang.map(native_font_name),
            weight: spec.weight,
            slant: spec.slant,
            width: spec.width,
            otf: spec.otf.as_ref(),
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
            otf: spec.otf.as_ref(),
        }
    }
}

/// GNU `font_check_otf_features` (`src/font.c:2358-2395`).
///
/// The named script must exist. With LANGSYS, that language system must exist;
/// without one, the default (`nil`) language system is preferred and the
/// first table entry is the fallback. Required features must all be present
/// and forbidden features must all be absent; an empty feature table still
/// satisfies a forbidden-only list, exactly as GNU's loop does.
/// OpenType script tag -> the characters GNU's `script-representative-chars`
/// lists for the script that `otf-script-alist` maps the tag to.
///
/// Generated from the effective GNU 31.1 `lisp/international/fontset.el`
/// tables (a360712c) including the startup-added entries. `ftfont_spec_pattern`
/// puts every listed character into the Fontconfig charset it lists for a
/// `:otf` spec (`src/ftfont.c:785-798`), so a font must cover all of them.
const OTF_SCRIPT_REPRESENTATIVE_CHARS: &[(&str, &[u32])] = &[
    ("adlm", &[0x1e900, 0x1e943]),
    ("aghb", &[0x10530]),
    ("arab", &[0x628, 0x6c1]),
    ("armn", &[0x531]),
    ("avst", &[0x10b00]),
    ("bali", &[0x1b13, 0x1b35, 0x1b5e]),
    ("bass", &[0x16ad0]),
    ("batk", &[0x1bc2, 0x1be7, 0x1bff]),
    ("beng", &[0x995]),
    ("berf", &[0x16ea0]),
    ("bhks", &[0x11c00]),
    ("bng2", &[0x995]),
    ("bopo", &[0x3105]),
    ("brah", &[0x11013, 0x11045, 0x11052, 0x11065]),
    ("brai", &[0x2800]),
    ("bugi", &[0x1a00, 0x1a1e]),
    ("buhd", &[0x1740]),
    ("byzm", &[0x1d000]),
    ("cakm", &[0x11103, 0x11127]),
    ("cans", &[0x14c0]),
    ("cari", &[0x102a0]),
    ("cham", &[0xaa00]),
    ("cher", &[0x13b6]),
    ("chrs", &[0x10fb0]),
    ("copt", &[0x3e2, 0x2c80, 0x2cae]),
    ("cpmn", &[0x12f90]),
    ("cyrl", &[0x42f]),
    ("dev2", &[0x915]),
    ("deva", &[0x915]),
    ("diak", &[0x11900]),
    ("dogr", &[0x11800]),
    ("dsrt", &[0x10400]),
    ("dupl", &[0x1bc20]),
    ("egyp", &[0x13000]),
    ("elba", &[0x10500]),
    ("elym", &[0x10fe0]),
    ("ethi", &[0x1208]),
    ("gara", &[0x10d50, 0x10d70, 0x10d4a, 0x10d41]),
    ("geor", &[0x10d3]),
    ("gjr2", &[0xa95]),
    ("gong", &[0x11d60]),
    ("gonm", &[0x11d00]),
    ("goth", &[0x10330, 0x10348]),
    ("gran", &[0x11315, 0x1133e, 0x11374]),
    ("grek", &[0x3a9]),
    ("gujr", &[0xa95]),
    ("gukh", &[0x16100, 0x1611e, 0x16131]),
    ("gur2", &[0xa15]),
    ("guru", &[0xa15]),
    ("hang", &[0xac00]),
    ("hani", &[0x3410, 0x4e10, 0x5b57, 0xfe30, 0xf900]),
    ("hano", &[0x1720]),
    ("hebr", &[0x5d0]),
    ("hmng", &[0x16b11]),
    ("hmnp", &[0x1e100]),
    ("ital", &[0x10300]),
    ("jamo", &[0xac00]),
    ("java", &[0xa98f, 0xa9b4, 0xa9ca]),
    ("kana", &[0x304b]),
    ("kawi", &[0x11f04, 0x11f41, 0x11f4f]),
    ("khar", &[0x10a00]),
    ("khmr", &[0x1780]),
    ("khoj", &[0x11200]),
    ("kits", &[0x18b00]),
    ("knd2", &[0xc95]),
    ("knda", &[0xc95]),
    ("krai", &[0x16d43, 0x16d63, 0x16d71]),
    ("kthi", &[0x1108d, 0x110b0, 0x110bd]),
    ("lana", &[0x1a20, 0x1a55, 0x1a61, 0x1a80]),
    ("lao ", &[0xea5]),
    (
        "latn",
        &[0x41, 0x5a, 0x61, 0x7a, 0xc0, 0x100, 0x180, 0x1e00],
    ),
    ("lepc", &[0x1c00, 0x1c24, 0x1c40]),
    ("limb", &[0x1901, 0x1920, 0x1936]),
    ("lina", &[0x10600]),
    ("linb", &[0x10000]),
    ("lyci", &[0x10280]),
    ("lydi", &[0x10920]),
    ("mahj", &[0x11150]),
    ("maka", &[0x11ee0, 0x11ef7]),
    ("mani", &[0x10ac0]),
    ("marc", &[0x11c72]),
    ("medf", &[0x16e40]),
    ("mend", &[0x1e810, 0x1e8a6]),
    ("mlm2", &[0xd15]),
    ("mlym", &[0xd15]),
    ("modi", &[0x1160e, 0x11630, 0x11655]),
    ("mong", &[0x1826]),
    ("mroo", &[0x16a40]),
    ("mtei", &[0xabc0, 0xabe3, 0xaae0, 0xaaf6]),
    ("musc", &[0x1d100]),
    ("mym2", &[0x1000]),
    ("mymr", &[0x1000]),
    ("nagm", &[0x1e4d0, 0x1e4eb, 0x1e4f0]),
    ("nand", &[0x119a0]),
    ("nbat", &[0x10880]),
    ("newa", &[0x11400]),
    ("nshu", &[0x1b170]),
    ("ogam", &[0x168f]),
    ("omao", &[0x1e5d0, 0x1e5f2]),
    ("orkh", &[0x10c00, 0x10c01]),
    ("ory2", &[0xb15]),
    ("orya", &[0xb15]),
    ("osge", &[0x104b0]),
    ("osma", &[0x10480]),
    ("ougr", &[0x10f70]),
    ("palm", &[0x10860]),
    ("pauc", &[0x11ac0]),
    ("perm", &[0x10350]),
    ("phnx", &[0x10900]),
    ("rjng", &[0xa930, 0xa947, 0xa95f]),
    ("rohg", &[0x10d00, 0x10d24, 0x10d39]),
    ("runr", &[0x16a0]),
    ("shaw", &[0x10450]),
    ("shrd", &[0x11191, 0x111b3, 0x111cd]),
    ("sidd", &[0x1158e, 0x115af, 0x115d4]),
    ("sidt", &[0x10940]),
    ("sind", &[0x112b0]),
    ("sinh", &[0xd95]),
    ("sogd", &[0x10f30]),
    ("sogo", &[0x10f00]),
    ("soyo", &[0x11a50]),
    ("sund", &[0x1b8a, 0x1bab, 0x1cc4]),
    ("sunu", &[0x11bc0, 0x11bf1]),
    ("sylo", &[0xa807, 0xa823, 0xa82c]),
    ("syrc", &[0x710]),
    ("tagb", &[0x1760]),
    ("takr", &[0x11680]),
    ("tale", &[0x1950]),
    ("talu", &[0x1980]),
    ("taml", &[0xb95]),
    ("tang", &[0x17000]),
    ("tavt", &[0xaa80]),
    ("tayo", &[0x1e6e0]),
    ("tel2", &[0xc15]),
    ("telu", &[0xc15]),
    ("tfng", &[0x2d30, 0x2d60]),
    ("tglg", &[0x1700]),
    ("thaa", &[0x78c]),
    ("thai", &[0xe17]),
    ("tibt", &[0xf40]),
    ("tirh", &[0x11481, 0x1148f, 0x114d0]),
    ("tml2", &[0xb95]),
    ("tnsa", &[0x16a70, 0x16ac0]),
    ("todr", &[0x105c0, 0x105ed]),
    ("tols", &[0x11db0]),
    ("toto", &[0x1e290, 0x1e295, 0x1e2ad]),
    ("tutg", &[0x11380, 0x113b8]),
    ("ugar", &[0x10380]),
    ("vai ", &[0xa500]),
    ("vith", &[0x10570]),
    ("wara", &[0x118a1]),
    ("wcho", &[0x1e2c0, 0x1e2e8, 0x1e2ef]),
    ("xpeo", &[0x103a0]),
    ("xsux", &[0x12000]),
    ("yezi", &[0x10e80]),
    ("yi 	", &[0xa288]),
    ("zanb", &[0x11a00]),
];

/// The representative characters GNU requires for a `:otf` spec's script.
fn otf_script_representative_chars(script: &str) -> Option<&'static [u32]> {
    OTF_SCRIPT_REPRESENTATIVE_CHARS
        .binary_search_by(|(tag, _)| (*tag).cmp(script))
        .ok()
        .map(|index| OTF_SCRIPT_REPRESENTATIVE_CHARS[index].1)
}

/// Whether GNU's ftfont backend resolves this registry to a Fontconfig charset
/// of its own (`ftfont_get_charset`). When it does, the spec's charset comes
/// from the registry and the `:otf` representative characters are not added
/// (`src/ftfont.c:710-720, 785-798`).
fn registry_supplies_backend_charset(registry: Option<&str>) -> bool {
    registry.is_some_and(|registry| {
        !matches!(
            registry.trim().to_ascii_lowercase().as_str(),
            "ascii-0" | "iso10646-1" | "unicode-bmp"
        )
    })
}

/// The extra required coverage a `:otf` spec adds on GNU's list path: the
/// `script-representative-chars` of the script `otf-script-alist` maps the tag
/// to. Every listed character must be present in the font's charset.
pub(super) fn otf_representative_ranges(
    otf: &StoredOtfSpec,
    registry: Option<&str>,
) -> Vec<(u32, u32)> {
    if registry_supplies_backend_charset(registry) {
        return Vec::new();
    }
    otf_script_representative_chars(&otf.script)
        .map(|chars| {
            chars
                .iter()
                .map(|codepoint| (*codepoint, *codepoint))
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
pub(super) fn otf_features_match(
    script: &str,
    langsys: &StoredOtfLangsys,
    features: &StoredOtfFeatures,
    table: &[OtfScript],
) -> bool {
    let Some(script_entry) = table.iter().find(|entry| entry.tag == script) else {
        return false;
    };
    let lang = match langsys {
        StoredOtfLangsys::Named(langsys) => script_entry
            .lang_syses
            .iter()
            .find(|entry| entry.tag.as_deref() == Some(langsys.as_str())),
        StoredOtfLangsys::Default => script_entry
            .lang_syses
            .iter()
            .find(|entry| entry.tag.is_none())
            .or_else(|| script_entry.lang_syses.first()),
        // GNU's validator accepts a non-symbol LANGSYS, but `assq_no_quit`
        // can never match it, so a feature requirement rejects every font.
        StoredOtfLangsys::Unmatchable => return false,
    };
    let feature_tags: &[String] = match lang {
        // A named language system GNU cannot find rejects the font.
        None if matches!(langsys, StoredOtfLangsys::Named(_)) => return false,
        Some(lang) => &lang.features,
        None => &[],
    };
    if features
        .required
        .iter()
        .any(|feature| !feature_tags.iter().any(|tag| tag == feature))
    {
        return false;
    }
    if features
        .forbidden
        .iter()
        .any(|feature| feature_tags.iter().any(|tag| tag == feature))
    {
        return false;
    }
    true
}

/// GNU `font_check_otf` (`src/font.c:2399-2425`): GSUB and GPOS are checked
/// independently against the font's capability.
///
/// This is the rule `Ffont_match_p` applies to an opened font object. The
/// listing/selection path uses [`otf_spec_matches_for_listing`] instead, so
/// this matcher is exercised by tests until the Lisp entry point is ported.
#[cfg(test)]
pub(super) fn otf_spec_matches(spec: &StoredOtfSpec, capability: &OtfCapability) -> bool {
    if let Some(gsub) = &spec.gsub
        && !otf_features_match(&spec.script, &spec.langsys, gsub, &capability.gsub)
    {
        return false;
    }
    if let Some(gpos) = &spec.gpos
        && !otf_features_match(&spec.script, &spec.langsys, gpos, &capability.gpos)
    {
        return false;
    }
    true
}

/// GNU's backend-level `:otf` filter (`src/ftfont.c:1026-1041`,
/// `hbotf_check_features`): both GSUB and GPOS must contain the script, a
/// named language system must exist in each, and a specified feature list is
/// checked against that table's language system. An omitted feature list
/// still requires the script (and named language system) to exist. This is
/// the rule `find-font`/`list-fonts` and fontset selection apply, and it
/// differs from `font_check_otf` in two ways: a nil LANGSYS selects only the
/// default language system (HarfBuzz does not fall back to the first), and a
/// side without features is still checked for the script.
pub(super) fn otf_listing_side_matches(
    script: &str,
    langsys: &StoredOtfLangsys,
    features: Option<&StoredOtfFeatures>,
    table: &[OtfScript],
) -> bool {
    let Some(script_entry) = table.iter().find(|entry| entry.tag == script) else {
        return false;
    };
    let feature_tags: &[String] = match langsys {
        StoredOtfLangsys::Unmatchable => return false,
        StoredOtfLangsys::Named(langsys) => {
            let Some(lang) = script_entry
                .lang_syses
                .iter()
                .find(|entry| entry.tag.as_deref() == Some(langsys.as_str()))
            else {
                return false;
            };
            &lang.features
        }
        StoredOtfLangsys::Default => script_entry
            .lang_syses
            .iter()
            .find(|entry| entry.tag.is_none())
            .map_or(&[][..], |lang| lang.features.as_slice()),
    };
    let Some(features) = features else {
        return true;
    };
    features
        .required
        .iter()
        .all(|feature| feature_tags.iter().any(|tag| tag == feature))
        && !features
            .forbidden
            .iter()
            .any(|feature| feature_tags.iter().any(|tag| tag == feature))
}

pub(super) fn otf_spec_matches_for_listing(
    spec: &StoredOtfSpec,
    capability: &OtfCapability,
) -> bool {
    otf_listing_side_matches(
        &spec.script,
        &spec.langsys,
        spec.gsub.as_ref(),
        &capability.gsub,
    ) && otf_listing_side_matches(
        &spec.script,
        &spec.langsys,
        spec.gpos.as_ref(),
        &capability.gpos,
    )
}

pub struct GnuFontPolicy;

impl GnuFontPolicy {
    pub fn constraints_for_entity(spec: &StoredFontSpec) -> NativeQueryConstraints {
        let representative = representative_char_for_spec(spec);
        Self::constraints_for_spec(spec, representative)
    }

    pub fn constraints_for_character(
        spec: &StoredFontSpec,
        required_char: char,
    ) -> NativeQueryConstraints {
        Self::constraints_for_spec(spec, required_char)
    }

    pub fn constraints_for_spec(
        spec: &StoredFontSpec,
        required_char: char,
    ) -> NativeQueryConstraints {
        Self::constraints_for_names(&FontPolicySpec::from(spec), required_char)
    }

    pub(super) fn constraints_for_names(
        names: &FontPolicySpec<'_>,
        required_char: char,
    ) -> NativeQueryConstraints {
        let representative_char = representative_char_for_registry(names.registry.as_deref());
        let coverage_ranges =
            query_charset_ranges_for_registry(names.registry.as_deref(), required_char);
        let registry_language = names.registry.as_deref().and_then(registry_language);
        let languages = combined_query_langs(registry_language, names.language.as_deref())
            .into_iter()
            .filter_map(LanguageTag::new)
            .collect();
        NativeQueryConstraints {
            representative_char,
            coverage: if coverage_ranges.is_empty() {
                CoverageRequirement::Any
            } else {
                CoverageRequirement::Ranges(coverage_ranges)
            },
            languages,
        }
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

pub(crate) fn representative_char_for_spec(spec: &StoredFontSpec) -> char {
    representative_char_for_registry(spec.registry.map(native_font_name).as_deref())
}

fn representative_char_for_registry(registry: Option<&str>) -> char {
    registry
        .and_then(|registry| registry_query_chars(Some(registry), 'a').into_iter().next())
        .and_then(char::from_u32)
        .unwrap_or('a')
}

pub(crate) fn query_charset_ranges(spec: &StoredFontSpec, ch: char) -> Vec<(u32, u32)> {
    query_charset_ranges_for_registry(spec.registry.map(native_font_name).as_deref(), ch)
}

fn query_charset_ranges_for_registry(registry: Option<&str>, ch: char) -> Vec<(u32, u32)> {
    if let Some(registry) = registry {
        if ftfont_registry_uses_unconstrained_charset(registry) {
            return Vec::new();
        }
        let mut codepoints = registry_query_chars(Some(registry), ch);
        if !codepoints.contains(&(ch as u32)) {
            codepoints.push(ch as u32);
        }
        return coalesce_ranges(
            codepoints
                .into_iter()
                .map(|codepoint| (codepoint, codepoint))
                .collect(),
        );
    }
    Vec::new()
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
    matches!(
        registry.trim().to_ascii_lowercase().as_str(),
        "ascii-0" | "iso10646-1" | "unicode-bmp"
    )
}

pub(crate) fn registry_query_chars(registry: Option<&str>, ch: char) -> Vec<u32> {
    registry
        .and_then(registry_hint)
        .map(|hint| hint.uniquifiers.to_vec())
        .filter(|chars| !chars.is_empty())
        .unwrap_or_else(|| vec![ch as u32])
}

pub(crate) fn registry_hint(registry: &str) -> Option<&'static RegistryHint> {
    let registry = registry.trim().to_ascii_lowercase();
    REGISTRY_HINTS
        .iter()
        .find(|hint| wildcard_casefold_match(&registry, hint.name))
}

/// ftfont_spec_pattern uses exact symbol shortcuts for Unicode registries;
/// the legacy charset table separately accepts case-insensitive patterns.
pub(crate) fn registry_supported_for_listing(registry: &str) -> bool {
    matches!(registry, "ascii-0" | "iso10646-1" | "unicode-bmp")
        || registry_hint(registry).is_some()
}

pub(crate) fn registry_language(registry: &str) -> Option<&'static str> {
    registry_hint(registry).and_then(|hint| hint.lang)
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
