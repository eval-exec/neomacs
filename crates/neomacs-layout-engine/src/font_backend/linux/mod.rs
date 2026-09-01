//! Linux native font catalog adapter.
//!
//! Fontconfig mechanics stay behind this module. Shared GNU selection policy,
//! candidate scoring, and exact identity transport live in the parent module.

mod catalog;

use super::{
    FontBackend, FontCandidate, FontCandidateQuery, FontCandidateScope, FontFamilyName,
    PlatformFontCandidate, PlatformFontCandidateLocator, PlatformFontMatch, PlatformFontMetadata,
};
use neomacs_display_protocol::font::{FontBackendKind, FontFileAsset};
use neovm_core::face::FontWidth;

impl PlatformFontCandidate {
    fn from_fontconfig(
        matched: crate::font::fontconfig::FontMatch,
        foundry: Option<String>,
        width: Option<FontWidth>,
        spacing: Option<i32>,
    ) -> Option<Self> {
        let file = matched.file.as_deref()?;
        let weight = matched
            .variation_coords
            .iter()
            .find(|coord| coord.tag() == u32::from_be_bytes(*b"wght"))
            .map(|coord| coord.value().round().clamp(1.0, 1000.0) as u16)
            .or(matched.weight);
        let identity =
            neomacs_display_protocol::font::ResolvedFontIdentity::from_file_with_variations(
                file,
                matched.face_index,
                matched.postscript_name.clone(),
                matched.variation_coords,
            );
        let asset = FontFileAsset::from_identity(&identity)?;
        Some(Self {
            identity,
            locator: PlatformFontCandidateLocator::File(asset),
            metadata: PlatformFontMetadata {
                foundry,
                family: matched.family,
                weight,
                slant: matched.slant,
                width,
                spacing,
                design_metrics: None,
                size: matched.size,
            },
        })
    }

    fn finalize_fontconfig(mut self) -> Self {
        let Some(file) = self.identity.file_path.clone() else {
            return self;
        };
        let Some(face_selector) = self.identity.freetype_selector() else {
            return self;
        };
        let variation_coords =
            if self.identity.variation_coords.is_empty() && (face_selector >> 16) & 0x7fff != 0 {
                crate::font::probe::named_instance_variation_coords(&file, face_selector)
            } else {
                self.identity.variation_coords.as_slice().to_vec()
            };
        let postscript_name = self
            .identity
            .postscript_name
            .clone()
            .or_else(|| crate::font::probe::postscript_name(&file, face_selector));
        if let Some(weight) = variation_coords
            .iter()
            .find(|coord| coord.tag() == u32::from_be_bytes(*b"wght"))
            .map(|coord| coord.value().round().clamp(1.0, 1000.0) as u16)
        {
            self.metadata.weight = Some(weight);
        }
        self.identity =
            neomacs_display_protocol::font::ResolvedFontIdentity::from_file_with_variations(
                &file,
                face_selector,
                postscript_name,
                variation_coords,
            );
        self
    }
}

/// Fontconfig adapter for native Linux font discovery.
#[derive(Debug, Default)]
pub struct FontconfigBackend {
    catalog: catalog::FontconfigCatalogMonitor,
}

impl FontBackend for FontconfigBackend {
    fn kind(&self) -> FontBackendKind {
        FontBackendKind::Fontconfig
    }

    fn list_families(&self) -> Vec<FontFamilyName> {
        crate::font::fontconfig::list_families()
    }

    fn resolve_family(&self, family: &str) -> String {
        crate::font::fontconfig::resolve_family(family)
    }

    fn family_prefers_monospace(&self, family: &str) -> bool {
        crate::font::fontconfig::family_prefers_monospace(family)
    }

    fn list_candidates(&self, query: &FontCandidateQuery) -> Vec<FontCandidate> {
        let family = match &query.scope {
            FontCandidateScope::Family(family) => Some(family.as_str()),
            FontCandidateScope::All | FontCandidateScope::NativeFallback { .. } => None,
        };
        let mut charset_ranges = query.charset_ranges.clone();
        if query.required.as_text().is_some() {
            charset_ranges.extend(
                query
                    .required
                    .chars()
                    .map(|ch| (u32::from(ch), u32::from(ch))),
            );
        }
        crate::font::fontconfig::fc_list_candidates(
            family,
            &charset_ranges,
            query.required.single_char().map(u32::from),
            &query.languages,
        )
        .into_iter()
        .filter_map(|candidate| {
            Some(FontCandidate {
                matched: PlatformFontCandidate::from_fontconfig(
                    candidate.matched,
                    candidate.foundry,
                    candidate.width,
                    candidate.spacing,
                )?,
            })
        })
        .collect()
    }

    fn finalize_match(&self, matched: PlatformFontCandidate) -> Option<PlatformFontMatch> {
        matched.finalize_fontconfig().into_file_match()
    }

    fn advance_catalog_generation(&mut self) {
        crate::font::fontconfig::invalidate_catalog_caches();
    }

    fn poll_catalog_change(&mut self) -> crate::font::catalog::FontCatalogChange {
        self.catalog.poll()
    }
}
