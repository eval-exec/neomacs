//! GNU-compatible font query policy shared by every platform catalog.
//!
//! Registry, repertory, and language semantics are Emacs behavior, not
//! Fontconfig behavior. This module translates a stored Lisp font spec into a
//! typed native-catalog query without naming any operating-system backend.

use neovm_core::emacs_core::fontset::{StoredFontSpec, repertory_target_ranges};
use neovm_core::emacs_core::intern::resolve_sym;

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
        let representative_char = representative_char_for_spec(spec);
        let coverage_ranges = query_charset_ranges(spec, required_char);
        let registry_language = spec.registry.map(resolve_sym).and_then(registry_language);
        let languages = combined_query_langs(registry_language, spec.lang.map(resolve_sym))
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
    spec.registry
        .map(resolve_sym)
        .and_then(|registry| registry_query_chars(Some(registry), 'a').into_iter().next())
        .and_then(char::from_u32)
        .unwrap_or('a')
}

pub(crate) fn query_charset_ranges(spec: &StoredFontSpec, ch: char) -> Vec<(u32, u32)> {
    if let Some(registry) = spec.registry.map(resolve_sym) {
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

    if let Some(mut ranges) = spec
        .repertory
        .as_ref()
        .and_then(repertory_target_ranges)
        .filter(|ranges| !ranges.is_empty())
    {
        ranges.push((ch as u32, ch as u32));
        return coalesce_ranges(ranges);
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
#[path = "policy_test.rs"]
mod tests;
