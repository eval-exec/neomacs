//! Safe macOS catalog adapter.
//!
//! GNU-compatible selection remains in the shared resolver. CoreText calls and
//! ownership contracts are isolated in `core_text_calls`; this module sees only
//! owned Rust data and exposes no Apple framework type.

mod catalog;
mod core_text_calls;
#[cfg(test)]
mod test_registration;
#[cfg(test)]
pub(crate) use test_registration::ProcessFontRegistration;

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
        self.candidates_for_identity(query, None)
    }

    fn list_candidates_for_identity(
        &self,
        query: &FontCandidateQuery,
        identity: &ResolvedFontIdentity,
    ) -> Vec<FontCandidate> {
        self.candidates_for_identity(query, Some(identity))
            .into_iter()
            .filter(|candidate| candidate.matched.identity == *identity)
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
    /// Enumerate candidates that can produce the requested stable key before
    /// opening font files. PostScript metadata on the caller's identity is not
    /// authoritative: fontdb and CoreText can name the same file differently.
    fn candidates_for_identity(
        &self,
        query: &FontCandidateQuery,
        identity: Option<&ResolvedFontIdentity>,
    ) -> Vec<FontCandidate> {
        core_text_calls::candidates(query)
            .into_iter()
            .filter(|candidate| {
                identity
                    .is_none_or(|identity| Self::candidate_may_match_identity(candidate, identity))
            })
            .filter_map(|candidate| {
                let metadata = PlatformFontMetadata {
                    foundry: None,
                    family: candidate.family,
                    weight: Some(candidate.weight),
                    slant: candidate.slant,
                    width: Some(candidate.width),
                    spacing: Some(candidate.spacing),
                    design_metrics: None,
                    // CoreText is the authority for the macOS catalog. Every
                    // candidate it exposes is replayed as an outline/color
                    // font through its file or copied SFNT tables; unsupported
                    // legacy bitmap files never enter the candidate set.
                    size: super::PlatformFontSize::Scalable,
                };
                let file_face = candidate.path.as_ref().and_then(|path| {
                    self.file_face_index_for_postscript_name(path, &candidate.postscript_name)
                        .map(|face_index| (path.clone(), face_index))
                });
                let matched = if let Some((path, face_index)) = file_face {
                    PlatformFontCandidate::from_platform_file(
                        FontBackendKind::CoreText,
                        &path,
                        face_index,
                        Some(candidate.postscript_name),
                        candidate.variation_coords,
                        metadata,
                    )?
                } else {
                    // A CoreText URL is only a usable cross-thread identity
                    // when its exact collection face can be proven. If the
                    // container is opaque to ttf-parser, retain the native
                    // identity and copy the selected face's SFNT tables after
                    // shared policy chooses it.
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

    /// Conservatively retain every candidate that could have the same stable
    /// key. File keys begin with the exact path followed by '#'; the face and
    /// variations are checked after reading that file. Even a URL-backed
    /// candidate may fall back to a native key if its container is opaque.
    fn candidate_may_match_identity(
        candidate: &core_text_calls::Candidate,
        identity: &ResolvedFontIdentity,
    ) -> bool {
        if candidate
            .path
            .as_deref()
            .and_then(Path::to_str)
            .is_some_and(|path| {
                identity
                    .stable_key
                    .strip_prefix(path)
                    .is_some_and(|suffix| suffix.starts_with('#'))
            })
        {
            return true;
        }
        identity.stable_key.starts_with("coretext:")
            && ResolvedFontIdentity::from_native_with_variations(
                FontBackendKind::CoreText,
                format!("coretext:{}#0", candidate.postscript_name),
                0,
                Some(candidate.postscript_name.clone()),
                candidate.variation_coords.clone(),
            ) == *identity
    }

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

#[cfg(test)]
#[path = "tests/coretext_fixture_test.rs"]
mod tests;
