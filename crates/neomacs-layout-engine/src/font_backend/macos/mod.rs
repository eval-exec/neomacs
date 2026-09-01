//! Safe macOS catalog adapter.
//!
//! GNU-compatible selection remains in the shared resolver. CoreText calls and
//! ownership contracts are isolated in `core_text_calls`; this module sees only
//! owned Rust data and exposes no Apple framework type.

mod catalog;
mod core_text_calls;

use super::native_asset_cache::NativeFontAssetCache;
use super::{
    FontBackend, FontCandidate, FontCandidateQuery, FontFamilyName, PlatformFontCandidate,
    PlatformFontCandidateLocator, PlatformFontDesignMetrics, PlatformFontMatch,
    PlatformFontMetadata,
};
use neomacs_display_protocol::font::{FontBackendKind, ResolvedFontIdentity};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// CoreText adapter for native macOS family matching and cascade fallback.
#[derive(Debug, Default)]
pub struct CoreTextBackend {
    catalog: catalog::CoreTextCatalogMonitor,
    native_assets: NativeFontAssetCache,
    file_faces: Mutex<HashMap<PathBuf, HashMap<String, u32>>>,
}

impl FontBackend for CoreTextBackend {
    fn kind(&self) -> FontBackendKind {
        FontBackendKind::CoreText
    }

    fn list_families(&self) -> Vec<FontFamilyName> {
        core_text_calls::available_family_names()
            .into_iter()
            .filter_map(FontFamilyName::new)
            .collect()
    }

    fn resolve_family(&self, family: &str) -> String {
        core_text_calls::resolve_generic_family(family).unwrap_or_else(|| family.to_string())
    }

    fn family_prefers_monospace(&self, family: &str) -> bool {
        core_text_calls::family_prefers_monospace(&self.resolve_family(family))
    }

    fn list_candidates(&self, query: &FontCandidateQuery) -> Vec<FontCandidate> {
        core_text_calls::candidates(query)
            .into_iter()
            .filter_map(|candidate| {
                let metadata = PlatformFontMetadata {
                    foundry: None,
                    family: candidate.family,
                    weight: Some(candidate.weight),
                    slant: candidate.slant,
                    width: Some(candidate.width),
                    spacing: Some(candidate.spacing),
                    design_metrics: None,
                    size: if candidate.path.is_some() {
                        super::PlatformFontSize::Unknown
                    } else {
                        // URL-less CoreText entities are native scalable fonts.
                        // Their table bytes are copied only if shared policy
                        // selects this candidate.
                        super::PlatformFontSize::Scalable
                    },
                };
                let matched = if let Some(path) = candidate.path {
                    let face_index = self
                        .file_face_index_for_postscript_name(&path, &candidate.postscript_name)?;
                    PlatformFontCandidate::from_platform_file(
                        FontBackendKind::CoreText,
                        &path,
                        face_index,
                        Some(candidate.postscript_name),
                        candidate.variation_coords,
                        metadata,
                    )?
                } else {
                    let stable_key = format!("coretext:{}#0", candidate.postscript_name);
                    PlatformFontCandidate {
                        identity: ResolvedFontIdentity::from_native_with_variations(
                            FontBackendKind::CoreText,
                            stable_key,
                            0,
                            Some(candidate.postscript_name),
                            candidate.variation_coords,
                        ),
                        locator: PlatformFontCandidateLocator::Native,
                        metadata,
                    }
                };
                Some(FontCandidate { matched })
            })
            .collect()
    }

    fn finalize_match(&self, matched: PlatformFontCandidate) -> Option<PlatformFontMatch> {
        if matches!(&matched.locator, PlatformFontCandidateLocator::File(_)) {
            return matched.into_file_match();
        }
        let postscript_name = matched.identity.postscript_name.as_deref()?;
        let asset = self
            .native_assets
            .get_or_materialize(&matched.identity, || {
                core_text_calls::standalone_font_bytes(
                    postscript_name,
                    &matched.identity.variation_coords,
                )
            })?;
        matched.into_memory_match(asset)
    }

    fn design_metrics(&self, matched: &PlatformFontMatch) -> Option<PlatformFontDesignMetrics> {
        core_text_calls::design_metrics(
            matched.identity.postscript_name.as_deref()?,
            &matched.identity.variation_coords,
        )
    }

    fn advance_catalog_generation(&mut self) {
        self.native_assets.clear();
        self.file_faces
            .get_mut()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clear();
    }

    fn poll_catalog_change(&mut self) -> crate::font::catalog::FontCatalogChange {
        self.catalog.poll()
    }
}

impl CoreTextBackend {
    fn file_face_index_for_postscript_name(
        &self,
        path: &Path,
        postscript_name: &str,
    ) -> Option<u32> {
        {
            let cache = self
                .file_faces
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if let Some(faces) = cache.get(path) {
                return faces.get(postscript_name).copied();
            }
        }

        let data = std::fs::read(path).ok()?;
        let face_count = ttf_parser::fonts_in_collection(&data).unwrap_or(1);
        let mut faces = HashMap::new();
        if face_count == 1 {
            ttf_parser::Face::parse(&data, 0).ok()?;
            faces.insert(postscript_name.to_string(), 0);
        } else {
            for face_index in 0..face_count {
                let name = ttf_parser::Face::parse(&data, face_index)
                    .ok()
                    .and_then(|face| {
                        face.names()
                            .into_iter()
                            .find(|name| {
                                name.name_id == ttf_parser::name_id::POST_SCRIPT_NAME
                                    && name.is_unicode()
                            })
                            .and_then(|name| name.to_string())
                    });
                if let Some(name) = name {
                    faces.insert(name, face_index);
                }
            }
        }
        let selected = faces.get(postscript_name).copied();
        self.file_faces
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(path.to_path_buf(), faces);
        selected
    }
}
