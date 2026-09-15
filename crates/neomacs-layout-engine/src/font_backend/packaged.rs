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

#[derive(Clone, Copy)]
enum PackagedFace {
    Hack,
    Ubuntu,
    NotoSerif,
}

impl PackagedFace {
    const ALL: [Self; 3] = [Self::Hack, Self::Ubuntu, Self::NotoSerif];

    fn family(self) -> &'static str {
        match self {
            Self::Hack => FAMILY,
            Self::Ubuntu => "Ubuntu",
            Self::NotoSerif => "Noto Serif",
        }
    }

    fn key(self) -> &'static str {
        match self {
            Self::Hack => STABLE_KEY,
            Self::Ubuntu => "packaged:ubuntu-light:epaint-0.36.1#0",
            Self::NotoSerif => "packaged:noto-serif-regular:oxifont-0.2.2#0",
        }
    }

    fn postscript_name(self) -> &'static str {
        match self {
            Self::Hack => POST_SCRIPT_NAME,
            Self::Ubuntu => "Ubuntu-Light",
            Self::NotoSerif => "NotoSerif-Regular",
        }
    }

    fn bytes(self) -> &'static [u8] {
        match self {
            Self::Hack => epaint_default_fonts::HACK_REGULAR,
            Self::Ubuntu => epaint_default_fonts::UBUNTU_LIGHT,
            Self::NotoSerif => oxifont_bundled::NOTO_SERIF_REGULAR,
        }
    }

    fn shared_bytes(self) -> Arc<Vec<u8>> {
        static HACK: OnceLock<Arc<Vec<u8>>> = OnceLock::new();
        static UBUNTU: OnceLock<Arc<Vec<u8>>> = OnceLock::new();
        static NOTO_SERIF: OnceLock<Arc<Vec<u8>>> = OnceLock::new();
        let storage = match self {
            Self::Hack => &HACK,
            Self::Ubuntu => &UBUNTU,
            Self::NotoSerif => &NOTO_SERIF,
        };
        Arc::clone(storage.get_or_init(|| Arc::new(self.bytes().to_vec())))
    }

    fn face(self) -> ttf_parser::Face<'static> {
        ttf_parser::Face::parse(self.bytes(), 0)
            .expect("packaged fonts must remain valid single-face SFNTs")
    }

    fn from_family(family: &str) -> Option<Self> {
        match family.to_ascii_lowercase().as_str() {
            "default" | "fixed" | "monospace" | "hack" => Some(Self::Hack),
            "sans" | "sans-serif" | "ubuntu" => Some(Self::Ubuntu),
            "serif" | "noto serif" => Some(Self::NotoSerif),
            _ => None,
        }
    }
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

fn candidate(font: PackagedFace) -> PlatformFontCandidate {
    let face = font.face();
    PlatformFontCandidate {
        identity: ResolvedFontIdentity::from_memory(
            FontBackendKind::Packaged,
            font.key().to_owned(),
            0,
            Some(font.postscript_name().to_owned()),
        ),
        locator: PlatformFontCandidateLocator::Native,
        metadata: PlatformFontMetadata {
            foundry: None,
            family: font.family().to_owned(),
            weight: Some(face.weight().to_number()),
            slant: FontSlant::Normal,
            width: Some(FontWidth::Normal),
            spacing: Some(if face.is_monospaced() { 100 } else { 0 }),
            design_metrics: Some(design_metrics(&face)),
            size: PlatformFontSize::Scalable,
        },
    }
}

/// Immutable application catalog used by Android and browser products.
#[derive(Debug, Default)]
pub struct PackagedFontBackend;

impl FontBackend for PackagedFontBackend {
    fn kind(&self) -> FontBackendKind {
        FontBackendKind::Packaged
    }

    fn list_families(&self) -> Vec<FontFamilyName> {
        PackagedFace::ALL
            .into_iter()
            .map(|font| FontFamilyName::new(font.family()).expect("packaged family is non-empty"))
            .collect()
    }

    fn resolve_family(&self, family: &str) -> String {
        PackagedFace::from_family(family)
            .map_or_else(|| family.to_owned(), |font| font.family().to_owned())
    }

    fn family_prefers_monospace(&self, family: &str) -> bool {
        matches!(PackagedFace::from_family(family), Some(PackagedFace::Hack))
    }

    fn list_candidates(&self, query: &FontCandidateQuery) -> Vec<FontCandidate> {
        PackagedFace::ALL
            .into_iter()
            .filter(|font| {
                if let FontCandidateScope::Family(family) = &query.scope
                    && !self
                        .resolve_family(family.as_str())
                        .eq_ignore_ascii_case(font.family())
                {
                    return false;
                }
                let face = font.face();
                query.coverage_is_satisfied_by(|codepoint| {
                    char::from_u32(codepoint).is_some_and(|ch| face.glyph_index(ch).is_some())
                })
            })
            .map(|font| FontCandidate {
                matched: candidate(font),
            })
            .collect()
    }

    fn finalize_match(&self, matched: PlatformFontCandidate) -> Option<PlatformFontMatch> {
        let font = PackagedFace::ALL
            .into_iter()
            .find(|font| font.key() == matched.identity.stable_key)?;
        let asset = FontMemoryAsset::new(font.key(), font.shared_bytes(), 0)?;
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
    fn packaged_sans_serif_is_proportional_and_distinct_from_monospace() {
        let backend = PackagedFontBackend;
        let family = backend.resolve_family("sans-serif");
        assert_ne!(family, backend.resolve_family("monospace"));
        assert!(!backend.family_prefers_monospace(&family));
        let mut query = query(super::super::RequiredFontCoverage::Character('N'));
        query.scope = FontCandidateScope::Family(FontFamilyName::new(family).unwrap());
        let candidate = backend.list_candidates(&query).pop().unwrap();
        assert_eq!(candidate.matched.metadata.spacing, Some(0));
        let matched = backend.finalize_match(candidate.matched).unwrap();
        let face = ttf_parser::Face::parse(matched.asset.bytes().unwrap(), 0).unwrap();
        assert_ne!(
            face.glyph_hor_advance(face.glyph_index('i').unwrap()),
            face.glyph_hor_advance(face.glyph_index('W').unwrap())
        );
    }

    #[test]
    fn packaged_generic_families_materialize_distinct_faces() {
        let backend = PackagedFontBackend;
        let mut identities = std::collections::HashSet::new();
        for (alias, family, monospace) in [
            ("monospace", "Hack", true),
            ("sans-serif", "Ubuntu", false),
            ("serif", "Noto Serif", false),
        ] {
            assert_eq!(backend.resolve_family(alias), family);
            assert_eq!(backend.family_prefers_monospace(alias), monospace);
            let mut query = query(super::super::RequiredFontCoverage::Character('N'));
            query.scope = FontCandidateScope::Family(FontFamilyName::new(alias).unwrap());
            let candidates = backend.list_candidates(&query);
            assert_eq!(candidates.len(), 1);
            let matched = backend
                .finalize_match(candidates.into_iter().next().unwrap().matched)
                .unwrap();
            assert!(identities.insert(matched.identity.stable_key));
            let face = ttf_parser::Face::parse(matched.asset.bytes().unwrap(), 0).unwrap();
            assert_eq!(face.is_monospaced(), monospace);
        }
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
