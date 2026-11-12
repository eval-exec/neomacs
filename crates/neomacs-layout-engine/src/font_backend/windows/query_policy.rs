//! Coverage and explicit-language admission for DirectWrite catalog entries.
//!
//! Kept independent of COM so the same native predicate is testable on every
//! host. Native language hints may include a registry-derived language; only
//! the raw explicit :lang receives GNU Windows' codepage restriction.

use super::FontCandidateQuery;

pub(super) fn generic_adstyle(pitch_and_family: u8) -> Option<&'static str> {
    // GNU w32_enumfont_pattern_entity uses the native generic family, not
    // the pitch bit or a guess based on the font's localized family name.
    match pitch_and_family & 0xf0 {
        0x10 => Some("serif"),
        0x20 => Some("sans"),
        0x30 => Some("mono"),
        0x40 => Some("script"),
        0x50 => Some("decorative"),
        _ => None,
    }
}

enum GenericFamilyObservation {
    Missing,
    Known(u8),
    Ambiguous,
}

pub(super) struct GenericFamilyProbe {
    weight: i32,
    italic: u8,
    observed: GenericFamilyObservation,
}

impl GenericFamilyProbe {
    pub(super) fn new(weight: i32, italic: u8) -> Self {
        Self {
            weight,
            italic,
            observed: GenericFamilyObservation::Missing,
        }
    }

    pub(super) fn observe(&mut self, weight: i32, italic: u8, pitch_and_family: u8) {
        if weight != self.weight || italic != self.italic {
            return;
        }
        let family = pitch_and_family & 0xf0;
        self.observed = match self.observed {
            GenericFamilyObservation::Missing => GenericFamilyObservation::Known(family),
            GenericFamilyObservation::Known(previous) if previous == family => {
                GenericFamilyObservation::Known(previous)
            }
            GenericFamilyObservation::Known(_) | GenericFamilyObservation::Ambiguous => {
                GenericFamilyObservation::Ambiguous
            }
        };
    }

    pub(super) fn adstyle(&self) -> Option<&'static str> {
        match self.observed {
            GenericFamilyObservation::Known(family) => generic_adstyle(family),
            GenericFamilyObservation::Missing | GenericFamilyObservation::Ambiguous => None,
        }
    }
}

pub(super) fn query_is_satisfied_by(
    query: &FontCandidateQuery,
    supports: impl FnOnce(&[u32]) -> bool,
    os2_table: impl FnOnce() -> Option<Vec<u8>>,
) -> bool {
    let mut codepoints = Vec::new();
    if !query.coverage_is_satisfied_by(|codepoint| {
        codepoints.push(codepoint);
        true
    }) {
        return false;
    }
    codepoints.sort_unstable();
    codepoints.dedup();
    // DirectWrite accepts a complete codepoint slice. Validate first, then
    // make one native call instead of crossing COM once per scalar/range.
    if !codepoints.is_empty() && !supports(&codepoints) {
        return false;
    }
    explicit_language_is_satisfied_by(query.explicit_language.as_deref(), os2_table)
}

/// A mapped fallback may cover only a prefix of the requested text. Admit
/// its explicit language without imposing full-text or registry coverage.
pub(super) fn explicit_language_is_satisfied_by(
    language: Option<&str>,
    os2_table: impl FnOnce() -> Option<Vec<u8>>,
) -> bool {
    // GNU w32font.c font_matches_spec tests only these exact explicit
    // language symbols against FONTSIGNATURE.fsCsb[0]. Registry-derived
    // Fontconfig hints do not represent an explicit :lang restriction.
    let required_codepages = match language {
        None => return true,
        Some("ja") => 1 << 17,
        Some("ko") => (1 << 19) | (1 << 21),
        Some("zh") => (1 << 18) | (1 << 20),
        Some(_) => return false,
    };
    let Some(table) = os2_table() else {
        return false;
    };
    // ulCodePageRange1 was introduced in OS/2 version 1. OpenType table
    // integers are big-endian, independently of the native platform.
    let Some(version) = table.get(..2).and_then(|bytes| bytes.try_into().ok()) else {
        return false;
    };
    if u16::from_be_bytes(version) == 0 {
        return false;
    }
    table
        .get(78..82)
        .and_then(|bytes| bytes.try_into().ok())
        .is_some_and(|bytes| u32::from_be_bytes(bytes) & required_codepages != 0)
}

#[cfg(test)]
#[path = "tests/query_policy_test.rs"]
mod tests;
