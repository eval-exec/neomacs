//! Explicit, immutable font catalogs for host-independent layout tests.
//! The catalog is owned by the existing boxed backend and cloned into jobs;
//! no global fontconfig state or additional TLS storage is involved.

use super::{FontMetricsService, FontSystem, font_slant_from_fontdb};
use crate::font_backend::{
    FontBackend, FontCandidate, FontCandidateQuery, FontCandidateScope, FontFamilyName,
    PlatformFontCandidate, PlatformFontCandidateLocator, PlatformFontMetadata, PlatformFontSize,
};
use neomacs_display_protocol::font::{FontBackendKind, FontFileAsset, ResolvedFontIdentity};
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Debug, thiserror::Error)]
pub(crate) enum TestFontCatalogError {
    #[error("font fixture I/O at {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("fixture has no named font face: {0}")]
    MissingFace(PathBuf),
    #[error("font fixture path is not UTF-8: {0}")]
    NonUtf8Path(PathBuf),
    #[error("a fixture font catalog needs at least one primary face")]
    Empty,
    #[error("requested fixture serif family is absent: {0}")]
    MissingSerif(String),
}

struct CatalogData {
    database: fontdb::Database,
    primary_family: String,
    serif_family: String,
    candidates: Vec<(fontdb::ID, PlatformFontCandidate)>,
}

/// A validated catalog owner. Equality means the same immutable owner, so a
/// worker cannot reuse a service from another independently built catalog.
#[derive(Clone)]
pub struct TestFontCatalog(Arc<CatalogData>);

impl std::fmt::Debug for TestFontCatalog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TestFontCatalog")
            .field("primary_family", &self.0.primary_family)
            .field("faces", &self.0.candidates.len())
            .finish()
    }
}

impl PartialEq for TestFontCatalog {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for TestFontCatalog {}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum TestFontCatalogSource {
    #[default]
    System,
    Fixture(TestFontCatalog),
}

static_assertions::assert_impl_all!(TestFontCatalogSource: Send, Sync);
static_assertions::assert_eq_size!(TestFontCatalogSource, usize);

enum SerifFamily<'a> {
    Primary,
    Named(&'a str),
}

impl TestFontCatalog {
    /// The first file is the primary family. Every fallback is another
    /// explicit file; none of the services consult the host's font catalog.
    pub(crate) fn from_files(files: &[&Path]) -> Result<Self, TestFontCatalogError> {
        Self::with_files(files, SerifFamily::Primary)
    }

    pub(crate) fn from_files_with_serif(
        files: &[&Path],
        serif_family: &str,
    ) -> Result<Self, TestFontCatalogError> {
        Self::with_files(files, SerifFamily::Named(serif_family))
    }

    fn with_files(files: &[&Path], serif: SerifFamily<'_>) -> Result<Self, TestFontCatalogError> {
        let mut database = fontdb::Database::new();
        let mut candidates = Vec::new();
        for file in files {
            let path = file
                .canonicalize()
                .map_err(|source| TestFontCatalogError::Io {
                    path: file.to_path_buf(),
                    source,
                })?;
            let name = path
                .to_str()
                .ok_or_else(|| TestFontCatalogError::NonUtf8Path(path.clone()))?;
            let bytes = std::fs::read(&path).map_err(|source| TestFontCatalogError::Io {
                path: path.clone(),
                source,
            })?;
            let ids = database
                .load_font_source(fontdb::Source::SharedFile(path.clone(), Arc::new(bytes)));
            if ids.is_empty() {
                return Err(TestFontCatalogError::MissingFace(path));
            }
            for id in ids {
                let face = database
                    .face(id)
                    .ok_or_else(|| TestFontCatalogError::MissingFace(path.clone()))?;
                let family = FontFamilyName::new(
                    face.families
                        .first()
                        .ok_or_else(|| TestFontCatalogError::MissingFace(path.clone()))?
                        .0
                        .clone(),
                )
                .ok_or_else(|| TestFontCatalogError::MissingFace(path.clone()))?
                .into_string();
                let identity = ResolvedFontIdentity::from_file(
                    name,
                    face.index,
                    Some(face.post_script_name.clone()),
                );
                let asset = FontFileAsset::from_identity(&identity)
                    .ok_or_else(|| TestFontCatalogError::MissingFace(path.clone()))?;
                candidates.push((
                    id,
                    PlatformFontCandidate {
                        identity,
                        locator: PlatformFontCandidateLocator::File(asset),
                        metadata: PlatformFontMetadata {
                            foundry: None,
                            family,
                            weight: Some(face.weight.0),
                            slant: font_slant_from_fontdb(face.style),
                            width: None,
                            spacing: Some(if face.monospaced { 100 } else { 0 }),
                            design_metrics: None,
                            size: PlatformFontSize::Scalable,
                        },
                    },
                ));
            }
        }
        let primary_family = candidates
            .first()
            .ok_or(TestFontCatalogError::Empty)?
            .1
            .metadata
            .family
            .clone();
        let serif_family = match serif {
            SerifFamily::Primary => primary_family.clone(),
            SerifFamily::Named(name) => candidates
                .iter()
                .find(|(_, candidate)| candidate.metadata.family.eq_ignore_ascii_case(name))
                .map(|(_, candidate)| candidate.metadata.family.clone())
                .ok_or_else(|| TestFontCatalogError::MissingSerif(name.to_owned()))?,
        };
        database.set_monospace_family(primary_family.clone());
        database.set_sans_serif_family(primary_family.clone());
        database.set_serif_family(serif_family.clone());
        database.set_cursive_family(primary_family.clone());
        database.set_fantasy_family(primary_family.clone());
        Ok(Self(Arc::new(CatalogData {
            database,
            primary_family,
            serif_family,
            candidates,
        })))
    }

    pub(crate) fn metrics_service(&self) -> FontMetricsService {
        FontMetricsService::from_font_system_and_resolver(
            FontSystem::new_with_locale_and_db("en-US".to_owned(), self.0.database.clone()),
            crate::font::resolver::FontResolver::new(Box::new(self.clone())),
        )
    }
}

impl FontMetricsService {
    pub(crate) fn test_catalog(&self) -> TestFontCatalogSource {
        self.font_resolver.test_catalog()
    }
}

impl TestFontCatalogSource {
    pub(crate) fn metrics_service(&self) -> FontMetricsService {
        match self {
            Self::System => FontMetricsService::new(),
            Self::Fixture(catalog) => catalog.metrics_service(),
        }
    }
}

impl FontBackend for TestFontCatalog {
    fn test_catalog(&self) -> TestFontCatalogSource {
        TestFontCatalogSource::Fixture(self.clone())
    }

    fn kind(&self) -> FontBackendKind {
        // These are explicit portable file identities, opened through Swash.
        FontBackendKind::Fontconfig
    }

    fn list_families(&self) -> Vec<FontFamilyName> {
        self.0
            .candidates
            .iter()
            .filter_map(|(_, candidate)| FontFamilyName::new(&candidate.metadata.family))
            .collect()
    }

    fn resolve_family(&self, family: &str) -> String {
        match family.to_ascii_lowercase().as_str() {
            "serif" => self.0.serif_family.clone(),
            "monospace" | "sans-serif" | "cursive" | "fantasy" => self.0.primary_family.clone(),
            _ => family.to_owned(),
        }
    }

    fn family_prefers_monospace(&self, family: &str) -> bool {
        let family = self.resolve_family(family);
        self.0.candidates.iter().any(|(_, candidate)| {
            candidate.metadata.family.eq_ignore_ascii_case(&family)
                && candidate.metadata.spacing == Some(100)
        })
    }

    fn list_candidates(&self, query: &FontCandidateQuery) -> Vec<FontCandidate> {
        self.0
            .candidates
            .iter()
            .filter(|(id, candidate)| {
                let family_matches = match &query.scope {
                    FontCandidateScope::Family(family) => candidate
                        .metadata
                        .family
                        .eq_ignore_ascii_case(&self.resolve_family(family.as_str())),
                    FontCandidateScope::All | FontCandidateScope::NativeFallback { .. } => true,
                };
                family_matches
                    && self
                        .0
                        .database
                        .with_face_data(*id, |data, index| {
                            ttf_parser::Face::parse(data, index).is_ok_and(|face| {
                                query.coverage_is_satisfied_by(|codepoint| {
                                    char::from_u32(codepoint)
                                        .is_some_and(|ch| face.glyph_index(ch).is_some())
                                })
                            })
                        })
                        .unwrap_or(false)
            })
            .map(|(_, candidate)| FontCandidate {
                matched: candidate.clone(),
            })
            .collect()
    }

    fn poll_catalog_change(&mut self) -> crate::font::catalog::FontCatalogChange {
        crate::font::catalog::FontCatalogChange::Unchanged
    }

    fn advance_catalog_generation(&mut self) {}
}
