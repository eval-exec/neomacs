//! Immutable font catalog for products without native font discovery.

use std::sync::{Arc, OnceLock};

use neomacs_display_protocol::font::{FontBackendKind, FontMemoryAsset, ResolvedFontIdentity};
use neovm_core::face::{FontSlant, FontWidth};

use super::{
    FontBackend, FontCandidate, FontCandidateQuery, FontCandidateScope, FontFamilyName,
    PlatformFontCandidate, PlatformFontCandidateLocator, PlatformFontDesignMetrics,
    PlatformFontMatch, PlatformFontMetadata, PlatformFontSize,
};

const FAMILY: &str = "Hack";
const POST_SCRIPT_NAME: &str = "Hack-Regular";
const STABLE_KEY: &str = "packaged:hack-regular:epaint-0.36.1#0";

fn shared_bytes() -> Arc<Vec<u8>> {
    static BYTES: OnceLock<Arc<Vec<u8>>> = OnceLock::new();
    Arc::clone(BYTES.get_or_init(|| Arc::new(epaint_default_fonts::HACK_REGULAR.to_vec())))
}

fn face() -> ttf_parser::Face<'static> {
    ttf_parser::Face::parse(epaint_default_fonts::HACK_REGULAR, 0)
        .expect("the packaged Hack font must remain a valid single-face SFNT")
}

fn design_metrics(face: &ttf_parser::Face<'_>) -> PlatformFontDesignMetrics {
    let advances = (0..face.number_of_glyphs()).filter_map(|glyph| {
        face.glyph_hor_advance(ttf_parser::GlyphId(glyph))
            .map(i32::from)
    });
    let max_advance = advances.max().unwrap_or_default();
    let space_advance = face
        .glyph_index(' ')
        .and_then(|glyph| face.glyph_hor_advance(glyph))
        .map(i32::from)
        .unwrap_or(max_advance);
    let (average_total, average_count) = (' '..='~')
        .filter_map(|ch| {
            face.glyph_index(ch)
                .and_then(|glyph| face.glyph_hor_advance(glyph))
        })
        .fold((0_u64, 0_u64), |(total, count), advance| {
            (total + u64::from(advance), count + 1)
        });

    PlatformFontDesignMetrics {
        units_per_em: u32::from(face.units_per_em()),
        ascent: i32::from(face.ascender()),
        descent: -i32::from(face.descender()),
        line_gap: i32::from(face.line_gap()),
        max_advance,
        space_advance,
        average_advance: i32::try_from(average_total / average_count.max(1)).unwrap_or(max_advance),
    }
}

fn candidate() -> PlatformFontCandidate {
    let face = face();
    PlatformFontCandidate {
        identity: ResolvedFontIdentity::from_memory(
            FontBackendKind::Packaged,
            STABLE_KEY.to_owned(),
            0,
            Some(POST_SCRIPT_NAME.to_owned()),
        ),
        locator: PlatformFontCandidateLocator::Native,
        metadata: PlatformFontMetadata {
            foundry: Some("Source Foundry".to_owned()),
            family: FAMILY.to_owned(),
            weight: Some(400),
            slant: FontSlant::Normal,
            width: Some(FontWidth::Normal),
            spacing: Some(100),
            design_metrics: Some(design_metrics(&face)),
            size: PlatformFontSize::Scalable,
        },
    }
}

fn is_generic_family(family: &str) -> bool {
    matches!(
        family.to_ascii_lowercase().as_str(),
        "default" | "fixed" | "monospace" | "sans-serif" | "serif"
    )
}

/// One-face application catalog used by Android and browser products.
#[derive(Debug, Default)]
pub struct PackagedFontBackend;

impl FontBackend for PackagedFontBackend {
    fn kind(&self) -> FontBackendKind {
        FontBackendKind::Packaged
    }

    fn list_families(&self) -> Vec<FontFamilyName> {
        vec![FontFamilyName::new(FAMILY).expect("packaged family is non-empty")]
    }

    fn resolve_family(&self, family: &str) -> String {
        if family.eq_ignore_ascii_case(FAMILY) || is_generic_family(family) {
            FAMILY.to_owned()
        } else {
            family.to_owned()
        }
    }

    fn family_prefers_monospace(&self, family: &str) -> bool {
        family.eq_ignore_ascii_case(FAMILY) || is_generic_family(family)
    }

    fn list_candidates(&self, query: &FontCandidateQuery) -> Vec<FontCandidate> {
        if let FontCandidateScope::Family(family) = &query.scope
            && !family.as_str().eq_ignore_ascii_case(FAMILY)
        {
            return Vec::new();
        }
        let face = face();
        if !query.coverage_is_satisfied_by(|codepoint| {
            char::from_u32(codepoint).is_some_and(|ch| face.glyph_index(ch).is_some())
        }) {
            return Vec::new();
        }
        vec![FontCandidate {
            matched: candidate(),
        }]
    }

    fn finalize_match(&self, matched: PlatformFontCandidate) -> Option<PlatformFontMatch> {
        let asset = FontMemoryAsset::new(STABLE_KEY, shared_bytes(), 0)?;
        matched.into_memory_match(asset)
    }

    fn poll_catalog_change(&mut self) -> crate::font::catalog::FontCatalogChange {
        crate::font::catalog::FontCatalogChange::Unchanged
    }

    fn advance_catalog_generation(&mut self) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query(required: super::super::RequiredFontCoverage) -> FontCandidateQuery {
        FontCandidateQuery {
            scope: FontCandidateScope::Family(FontFamilyName::new(FAMILY).unwrap()),
            required,
            charset_ranges: Vec::new(),
            languages: Vec::new(),
            requested_weight: 400,
            requested_slant: FontSlant::Normal,
            requested_width: FontWidth::Normal,
            direction: super::super::TextDirection::LeftToRight,
        }
    }

    #[test]
    fn packaged_candidate_materializes_replayable_memory_bytes() {
        let backend = PackagedFontBackend;
        let candidate = backend
            .list_candidates(&query(super::super::RequiredFontCoverage::Character('N')))
            .pop()
            .expect("Hack covers basic Latin");
        let matched = backend
            .finalize_match(candidate.matched)
            .expect("packaged bytes match their identity");

        assert_eq!(matched.identity.backend, FontBackendKind::Packaged);
        assert_eq!(
            matched.asset.bytes(),
            Some(epaint_default_fonts::HACK_REGULAR)
        );
    }

    #[test]
    fn packaged_catalog_rejects_uncovered_text() {
        let backend = PackagedFontBackend;

        assert!(
            backend
                .list_candidates(&query(super::super::RequiredFontCoverage::Character('界')))
                .is_empty()
        );
    }
}
