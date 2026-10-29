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
    #[cfg(target_os = "linux")]
    #[error(transparent)]
    Native(#[from] crate::font::fontconfig::test_config::ConfigError),
    #[error("requested fixture serif family is absent: {0}")]
    MissingSerif(String),
    #[error("requested fixture sans family is absent: {0}")]
    MissingSans(String),
}

#[derive(Clone)]
struct CatalogData {
    database: fontdb::Database,
    #[cfg(target_os = "linux")]
    native_config: crate::font::fontconfig::test_config::TestFontconfig,
    #[cfg(target_os = "linux")]
    backend: CatalogBackend,
    primary_family: FontFamilyName,
    serif_family: FontFamilyName,
    sans_serif_family: FontFamilyName,
    candidates: Vec<(fontdb::ID, PlatformFontCandidate)>,
}

#[cfg(target_os = "linux")]
#[derive(Clone, Copy, Debug)]
enum CatalogBackend {
    Portable,
    NativeFontconfig,
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

#[derive(Debug)]
enum GenericFamilyPolicy<'a> {
    Primary,
    SansAndSerif {
        sans_serif: &'a FontFamilyName,
        serif: &'a FontFamilyName,
    },
}

impl TestFontCatalog {
    /// The first file is the primary family. Every fallback is another
    /// explicit file; none of the services consult the host's font catalog.
    pub(crate) fn from_files(files: &[&Path]) -> Result<Self, TestFontCatalogError> {
        Self::with_files(files, GenericFamilyPolicy::Primary)
    }

    pub(crate) fn from_files_with_sans_and_serif(
        files: &[&Path],
        sans_serif: &FontFamilyName,
        serif: &FontFamilyName,
    ) -> Result<Self, TestFontCatalogError> {
        Self::with_files(
            files,
            GenericFamilyPolicy::SansAndSerif { sans_serif, serif },
        )
    }

    fn with_files(
        files: &[&Path],
        generics: GenericFamilyPolicy<'_>,
    ) -> Result<Self, TestFontCatalogError> {
        let mut database = fontdb::Database::new();
        let mut candidates = Vec::new();
        let mut font_files = Vec::with_capacity(files.len());
        for file in files {
            let path = file
                .canonicalize()
                .map_err(|source| TestFontCatalogError::Io {
                    path: file.to_path_buf(),
                    source,
                })?;
            font_files.push(path.clone());
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
        let primary_family = FontFamilyName::new(
            &candidates
                .first()
                .ok_or(TestFontCatalogError::Empty)?
                .1
                .metadata
                .family,
        )
        .ok_or(TestFontCatalogError::Empty)?;
        let loaded_family = |name: &FontFamilyName| {
            candidates
                .iter()
                .find(|(_, candidate)| {
                    candidate
                        .metadata
                        .family
                        .eq_ignore_ascii_case(name.as_str())
                })
                .and_then(|(_, candidate)| FontFamilyName::new(&candidate.metadata.family))
        };
        let (sans_serif_family, serif_family) = match generics {
            GenericFamilyPolicy::Primary => (primary_family.clone(), primary_family.clone()),
            GenericFamilyPolicy::SansAndSerif { sans_serif, serif } => (
                loaded_family(sans_serif).ok_or_else(|| {
                    TestFontCatalogError::MissingSans(sans_serif.as_str().to_owned())
                })?,
                loaded_family(serif)
                    .ok_or_else(|| TestFontCatalogError::MissingSerif(serif.as_str().to_owned()))?,
            ),
        };
        database.set_monospace_family(primary_family.as_str());
        database.set_sans_serif_family(sans_serif_family.as_str());
        database.set_serif_family(serif_family.as_str());
        database.set_cursive_family(primary_family.as_str());
        database.set_fantasy_family(primary_family.as_str());
        #[cfg(target_os = "linux")]
        let native_config = crate::font::fontconfig::test_config::TestFontconfig::new(
            &font_files,
            &primary_family,
            &sans_serif_family,
            &serif_family,
        )?;
        Ok(Self(Arc::new(CatalogData {
            database,
            #[cfg(target_os = "linux")]
            native_config,
            #[cfg(target_os = "linux")]
            backend: CatalogBackend::Portable,
            primary_family,
            serif_family,
            sans_serif_family,
            candidates,
        })))
    }

    fn backend(&self) -> Box<dyn FontBackend> {
        #[cfg(target_os = "linux")]
        return match self.0.backend {
            CatalogBackend::Portable => Box::new(self.clone()),
            CatalogBackend::NativeFontconfig => Box::new(
                crate::font_backend::FontconfigBackend::for_test_catalog(NativeCatalogAuthority {
                    catalog: self.clone(),
                }),
            ),
        };
        #[cfg(not(target_os = "linux"))]
        Box::new(self.clone())
    }

    #[cfg(target_os = "linux")]
    pub(crate) fn to_native_fontconfig(&self) -> Self {
        match self.0.backend {
            CatalogBackend::Portable => Self(Arc::new(CatalogData {
                backend: CatalogBackend::NativeFontconfig,
                native_config: self.0.native_config.fresh_cache_view(),
                ..self.0.as_ref().clone()
            })),
            CatalogBackend::NativeFontconfig => self.clone(),
        }
    }

    #[cfg(target_os = "linux")]
    pub(crate) fn native_fontconfig(
        &self,
    ) -> &crate::font::fontconfig::test_config::TestFontconfig {
        &self.0.native_config
    }

    pub(crate) fn foundry_for_file(&self, file: &str) -> Option<String> {
        #[cfg(target_os = "linux")]
        return self.0.native_config.foundry_for_file(file);
        #[cfg(not(target_os = "linux"))]
        {
            let _ = file;
            None
        }
    }

    pub(crate) fn font_system(&self) -> FontSystem {
        FontSystem::new_with_locale_and_db("en-US".to_owned(), self.0.database.clone())
    }

    pub(crate) fn metrics_service(&self) -> FontMetricsService {
        FontMetricsService::from_font_system_and_resolver(
            self.font_system(),
            crate::font::resolver::FontResolver::new(self.backend()),
        )
    }
}

impl FontMetricsService {
    pub(crate) fn test_catalog(&self) -> TestFontCatalogSource {
        self.font_resolver.test_catalog()
    }
}

impl TestFontCatalogSource {
    pub(crate) fn font_system(&self) -> FontSystem {
        match self {
            Self::System => FontSystem::new(),
            Self::Fixture(catalog) => catalog.font_system(),
        }
    }

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
            "serif" => self.0.serif_family.as_str().to_owned(),
            "sans-serif" => self.0.sans_serif_family.as_str().to_owned(),
            "monospace" | "cursive" | "fantasy" => self.0.primary_family.as_str().to_owned(),
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

/// The standard layout-test catalog has one immutable owner and real distinct
/// primary, proportional and serif faces. All fresh services/worker captures
/// derive from this owner; there is no ambient system-font fallback.
pub(crate) fn standard_catalog() -> &'static TestFontCatalog {
    static CATALOG: std::sync::OnceLock<TestFontCatalog> = std::sync::OnceLock::new();
    CATALOG.get_or_init(|| {
        let dejavu = neomacs_test_fonts::dejavu_2_37();
        let ubuntu = neomacs_test_fonts::ubuntu_0_83();
        let sans = FontFamilyName::new("Noto Sans").expect("nonempty fixture family");
        let serif = FontFamilyName::new("Noto Serif").expect("nonempty fixture family");
        TestFontCatalog::from_files_with_sans_and_serif(
            &[
                neomacs_test_fonts::noto_sans_mono_variable(),
                neomacs_test_fonts::mplus_1_code_thin(),
                neomacs_test_fonts::noto_sans_arabic_variable(),
                neomacs_test_fonts::noto_sans_devanagari_variable(),
                neomacs_test_fonts::noto_sans_thai_variable(),
                neomacs_test_fonts::noto_sans_hebrew_variable(),
                neomacs_test_fonts::noto_color_emoji_2_051(),
                neomacs_test_fonts::noto_serif_variable(),
                neomacs_test_fonts::noto_sans_variable(),
                neomacs_test_fonts::noto_sans_italic_variable(),
                neomacs_test_fonts::noto_sans_cjk_variable(),
                dejavu.mono_regular(),
                dejavu.mono_bold(),
                dejavu.mono_oblique(),
                dejavu.mono_bold_oblique(),
                dejavu.sans_regular(),
                dejavu.sans_bold(),
                dejavu.sans_oblique(),
                dejavu.sans_bold_oblique(),
                dejavu.serif_regular(),
                dejavu.serif_bold(),
                dejavu.serif_italic(),
                dejavu.serif_bold_italic(),
                ubuntu.mono_regular(),
                ubuntu.mono_bold(),
                ubuntu.mono_italic(),
                ubuntu.mono_bold_italic(),
                ubuntu.regular(),
                ubuntu.bold(),
                ubuntu.italic(),
                ubuntu.bold_italic(),
                neomacs_test_fonts::jetbrains_mono_regular_2_304(),
                neomacs_test_fonts::jetbrains_mono_bold_2_304(),
                neomacs_test_fonts::jetbrains_mono_italic_2_304(),
                neomacs_test_fonts::jetbrains_mono_nerd_regular_3_4_0(),
                neomacs_test_fonts::jetbrains_mono_nerd_bold_3_4_0(),
                neomacs_test_fonts::jetbrains_mono_nerd_italic_3_4_0(),
                neomacs_test_fonts::symbols_nerd_mono_3_4_0(),
                neomacs_test_fonts::hack_regular_3_003(),
                neomacs_test_fonts::hack_bold_3_003(),
                neomacs_test_fonts::hack_italic_3_003(),
            ],
            &sans,
            &serif,
        )
        .expect("all immutable standard layout-test font families are present")
    })
}

pub(crate) fn metrics_service() -> FontMetricsService {
    standard_catalog().metrics_service()
}

pub(crate) fn font_system() -> FontSystem {
    standard_catalog().font_system()
}

/// Preserve the production enable/clear transition without discovering host fonts.
pub(crate) fn enable_metrics(engine: &mut crate::LayoutEngine) {
    if engine.font_metrics.is_none() {
        engine.font_metrics = Some(metrics_service());
    }
    engine.enable_cosmic_metrics();
}

pub(crate) fn layout_engine() -> crate::LayoutEngine {
    let mut engine = crate::LayoutEngine::new_without_font_metrics();
    engine.font_metrics = Some(metrics_service());
    engine
}

pub(crate) fn query_engine() -> crate::engine::WindowLayoutQueryEngine {
    crate::engine::WindowLayoutQueryEngine::for_test_catalog()
}

#[cfg(test)]
#[path = "tests/test_catalog_test.rs"]
mod standard_fixture_tests;

#[cfg(target_os = "linux")]
#[derive(Clone, Debug)]
pub(crate) struct NativeCatalogAuthority {
    catalog: TestFontCatalog,
}

#[cfg(target_os = "linux")]
impl NativeCatalogAuthority {
    pub(crate) fn catalog(&self) -> &TestFontCatalog {
        &self.catalog
    }
    pub(crate) fn config(&self) -> &crate::font::fontconfig::test_config::TestFontconfig {
        &self.catalog.0.native_config
    }
}
