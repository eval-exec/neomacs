//! Coverage and explicit-language admission for DirectWrite catalog entries.
//!
//! Kept independent of COM so the same native predicate is testable on every
//! host. Native language hints may include a registry-derived language; only
//! the raw explicit :lang receives GNU Windows' codepage restriction.

use super::{FontCandidateQuery, FontSignatureRequirement};

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

pub(super) struct GenericFamilyRequest {
    pub(super) family: [u16; 32],
    pub(super) weight: i32,
    pub(super) italic: u8,
}

pub(super) struct GenericFamilyRecord {
    pub(super) weight: i32,
    pub(super) italic: u8,
    pub(super) pitch_and_family: u8,
}

/// One native listing's physical-family classification boundary.
///
/// The native adapter supplies owned resources and copied observations; no
/// callback pointer or catalog-owned font escapes through this controller.
pub(super) struct GenericFamilyQuery<R> {
    resources: std::cell::OnceCell<Option<R>>,
    families: std::collections::HashMap<[u16; 32], Option<Vec<GenericFamilyRecord>>>,
}

impl<R> GenericFamilyQuery<R> {
    pub(super) fn new() -> Self {
        Self {
            resources: std::cell::OnceCell::new(),
            families: std::collections::HashMap::new(),
        }
    }

    pub(super) fn classify(
        &mut self,
        initialize: impl FnOnce() -> Option<R>,
        describe: impl FnOnce(&R) -> Option<GenericFamilyRequest>,
        enumerate: impl FnOnce(&R, &[u16; 32]) -> Option<Vec<GenericFamilyRecord>>,
    ) -> Option<&'static str> {
        let resources = self.resources.get_or_init(initialize).as_ref()?;
        let request = describe(resources)?;
        // A named-family enumeration supplies all styles and charsets. Keep
        // its complete observations, including failure/empty results, only
        // until this native query ends; a new catalog query starts fresh.
        let records = self
            .families
            .entry(request.family)
            .or_insert_with(|| enumerate(resources, &request.family))
            .as_ref()?;
        let mut probe = GenericFamilyProbe::new(request.weight, request.italic);
        for record in records {
            probe.observe(record.weight, record.italic, record.pitch_and_family);
        }
        probe.adstyle()
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
    signature_and_language_are_satisfied_by(query, os2_table)
}

/// A native fallback may cover only a prefix of the text. Its signature and
/// language still apply, independently of full-text glyph coverage.
pub(super) fn signature_and_language_are_satisfied_by(
    query: &FontCandidateQuery,
    os2_table: impl FnOnce() -> Option<Vec<u8>>,
) -> bool {
    let Some(requirement) = query.signature_requirement else {
        return explicit_language_is_satisfied_by(query.explicit_language.as_deref(), os2_table);
    };
    let Some(table) = os2_table() else {
        return false;
    };
    signature_matches(requirement, &table)
        && explicit_language_is_satisfied_by(query.explicit_language.as_deref(), || Some(table))
}

fn os2_u32(table: &[u8], offset: usize) -> Option<u32> {
    table
        .get(offset..offset + 4)?
        .try_into()
        .ok()
        .map(u32::from_be_bytes)
}

fn codepage_range(table: &[u8]) -> Option<u32> {
    let version = u16::from_be_bytes(table.get(..2)?.try_into().ok()?);
    // Version zero contains the Unicode ranges but no codepage fields.
    (version != 0).then(|| os2_u32(table, 78)).flatten()
}

fn signature_matches(requirement: FontSignatureRequirement, table: &[u8]) -> bool {
    // The four Unicode-range words are present even in OS/2 version zero.
    // A complete signature is required before interpreting any of its bits.
    let [Some(usb0), Some(usb1), Some(usb2), Some(usb3)] =
        [42, 46, 50, 54].map(|offset| os2_u32(table, offset))
    else {
        return false;
    };
    // GNU w32font.c add_font_entity_to_list tests the native signature,
    // including its specific surrogate/CJK combination for Unicode plane 2.
    match requirement {
        FontSignatureRequirement::Unicode => {
            usb3 != 0 || usb2 != 0 || usb1 != 0 || usb0 & 0x3fff_ffff != 0
        }
        FontSignatureRequirement::Bmp => {
            usb3 != 0
                || usb2 & 0xffff_ff9e != 0
                || usb1 & 0xe81f_ffff != 0
                || usb0 & 0x007f_001f != 0
        }
        FontSignatureRequirement::Sip => {
            usb1 & 0x0200_0000 != 0
                && (usb1 & 0x2800_0000 != 0
                    || codepage_range(table).is_some_and(|csb| csb & 0x003e_0000 != 0))
        }
    }
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
    // OpenType table integers are big-endian on every native platform.
    codepage_range(&table).is_some_and(|csb| csb & required_codepages != 0)
}

#[cfg(test)]
#[path = "tests/query_policy_test.rs"]
mod tests;
