//! Shared GNU-compatible font selection policy.
//!
//! GNU Emacs keeps fontset lookup and entity scoring in `fontset.c`/`font.c`;
//! platform drivers list/open entities and answer coverage questions.
//! [`FontResolver`] owns fontset and enumerated-candidate selection across
//! Fontconfig, CoreText, and DirectWrite. GNU's separate driver `match`
//! operation is explicit: its native winner bypasses enumeration's style
//! filters without changing shared fontset selection.

mod worker_policy;
pub(crate) use worker_policy::{
    FontPolicyCaptureError, FontPolicyMergeError, FrozenCharacterPolicies,
};

use crate::font::policy::{
    CapturedFontFamilyPolicy, FontFamilyCandidate, FontFamilySource, GnuFontPolicy,
};
use crate::font::selection::{
    CandidateSelectionScore, candidate_listing_score, candidate_selection_score,
    candidate_style_selection_score,
};
use crate::font_backend::{
    FontBackend, FontCandidate, FontCandidateQuery, FontCandidateScope, FontFamilyName,
    FontSelectionSize, PlatformFontCandidate, PlatformFontMatch, PlatformFontSize,
    RequiredFontCoverage, TextDirection,
};
use neomacs_display_protocol::font::{FontBackendKind, ResolvedFontIdentity};
use neovm_core::emacs_core::eval::FontSpecSelection;
use neovm_core::emacs_core::fontset::{
    FontSpecEntry, StoredFontSpec, fontset_generation, matching_entries_for_char,
};
use neovm_core::emacs_core::intern::{SymId, intern};
use neovm_core::emacs_core::regex::EmacsRegexpError;
use neovm_core::face::{FontSlant, FontWeight, FontWidth};
use rustc_hash::{FxHashMap as HashMap, FxHashSet as HashSet};
use std::sync::Mutex;

/// Platform-neutral request for GNU entity enumeration or driver matching.
/// Optional fields remain optional all the way to the native adapter; no
/// platform is selected by the caller.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FontEntityQuery {
    selection: FontSpecSelection,
    family: Option<FontFamilyName>,
    registry: Option<String>,
    language: Option<String>,
    postscript_name: Option<String>,
    weight: Option<u16>,
    slant: Option<FontSlant>,
    width: Option<FontWidth>,
}

impl FontEntityQuery {
    pub fn with_selection(mut self, selection: FontSpecSelection) -> Self {
        self.selection = selection;
        self
    }

    pub fn new(family: Option<FontFamilyName>) -> Self {
        Self {
            family,
            ..Self::default()
        }
    }

    pub fn with_registry(mut self, registry: impl Into<String>) -> Self {
        self.registry = Some(registry.into());
        self
    }

    pub fn with_language(mut self, language: impl Into<String>) -> Self {
        self.language = non_empty_query_text(language.into());
        self
    }

    pub fn with_postscript_name(mut self, postscript_name: impl Into<String>) -> Self {
        self.postscript_name = non_empty_query_text(postscript_name.into());
        self
    }

    pub fn with_weight(mut self, weight: u16) -> Self {
        self.weight = Some(weight);
        self
    }

    pub fn with_slant(mut self, slant: FontSlant) -> Self {
        self.slant = Some(slant);
        self
    }

    pub fn with_width(mut self, width: FontWidth) -> Self {
        self.width = Some(width);
        self
    }
}

fn non_empty_query_text(value: String) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

fn result_registry(backend: FontBackendKind, requested: Option<&str>) -> String {
    // w32font's add_font_entity_to_list preserves any explicit registry on
    // an admitted entity. FT instead publishes its native Unicode registry.
    if backend == FontBackendKind::DirectWrite {
        requested.unwrap_or("iso10646-1").to_owned()
    } else {
        "iso10646-1".to_owned()
    }
}

// GNU uses these driver symbols for its HarfBuzz/Cairo and Cocoa adapters.
// DirectWrite is a Neomacs driver, not GNU's GDI or Uniscribe implementation.
fn font_driver_symbol(kind: FontBackendKind) -> Option<neovm_core::emacs_core::intern::SymId> {
    match kind {
        FontBackendKind::Fontconfig => Some(intern("ftcrhb")),
        FontBackendKind::CoreText => Some(intern("ns")),
        FontBackendKind::DirectWrite => Some(intern("directwrite")),
        _ => None,
    }
}

/// One exact entity returned by the active platform backend.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedFontEntity {
    pub matched: PlatformFontMatch,
    pub registry: Option<String>,
    pub driver: Option<neovm_core::emacs_core::intern::SymId>,
}

/// Catalog metadata before a font has been selected for coverage or opening.
/// A native locator carries no copied font bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListedFontCandidate {
    pub candidate: PlatformFontCandidate,
    pub registry: Option<String>,
    pub driver: Option<neovm_core::emacs_core::intern::SymId>,
}

/// Constraints for catalog listing, separate from driver matching and opening.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FontEntityListConstraints {
    pub otf: Option<crate::font_backend::NativeFontOtfRequirement>,
    pub scalable: Option<bool>,
    pub script_coverage: Option<crate::font_backend::NativeFontScriptCoverage>,
    pub driver: Option<neovm_core::emacs_core::intern::SymId>,
    pub foundry: Option<String>,
    pub adstyle: Option<String>,
    pub pixel_size: Option<u32>,
    pub dpi: Option<u32>,
    pub spacing: Option<i32>,
    pub average_width: Option<i32>,
}

/// One font entity opened through its owning platform backend.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenedFontEntity {
    pub entity: ResolvedFontEntity,
    pub metrics: crate::font::probe::FontPxMetrics,
}

/// GNU's post-enumeration entity filtering policy.  The NT GUI deliberately
/// accepts a nearby listed weight because the native family enumeration only
/// exposes a few standard weights; all other platform drivers require exact
/// style attributes (`src/font.c:2695-2760`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FontEntityMatchPolicy {
    Exact,
    WindowsNtGui,
}

impl FontEntityMatchPolicy {
    const fn for_backend(backend: FontBackendKind) -> Self {
        match backend {
            FontBackendKind::DirectWrite => Self::WindowsNtGui,
            _ => Self::Exact,
        }
    }

    fn weight_matches(self, requested: u16, candidate: Option<u16>) -> bool {
        let Some(candidate) = candidate else {
            return false;
        };
        // GNU compares FONT_WEIGHT_INDEX >> 8, the table's category,
        // not the platform's raw CSS coordinate (e.g. Thin 100 and 150).
        if FontWeight::from_css_weight(candidate).gnu_numeric()
            == FontWeight::from_css_weight(requested).gnu_numeric()
        {
            return true;
        }
        match self {
            Self::Exact => false,
            Self::WindowsNtGui => {
                FontWeight::from_css_weight(candidate)
                    .gnu_numeric()
                    .abs_diff(FontWeight::from_css_weight(requested).gnu_numeric())
                    <= 100
            }
        }
    }
}

/// Platform-neutral owner of fontset policy and candidate scoring.
pub struct FontResolver {
    worker_policy: Option<worker_policy::WorkerPolicy>,
    backend: Box<dyn FontBackend>,
    materializer: Option<neomacs_font_materializer::FontMaterializer>,
    capability_cache: Mutex<
        HashMap<
            neomacs_display_protocol::font::FontFileAsset,
            Result<
                neomacs_font_materializer::FontCapability,
                neomacs_font_materializer::FontMaterializationError,
            >,
        >,
    >,
    primary_cache: Mutex<HashMap<PrimaryCacheKey, Option<PlatformFontMatch>>>,
    char_cache: Mutex<HashMap<CharCacheKey, Option<PlatformFontMatch>>>,
    exact_cache: Mutex<HashMap<ResolvedFontIdentity, Option<PlatformFontMatch>>>,
}

impl FontResolver {
    pub fn new(backend: Box<dyn FontBackend>) -> Self {
        Self {
            backend,
            worker_policy: None,
            materializer: neomacs_font_materializer::FontMaterializer::new().ok(),
            capability_cache: Mutex::new(HashMap::default()),
            primary_cache: Mutex::new(HashMap::default()),
            char_cache: Mutex::new(HashMap::default()),
            exact_cache: Mutex::new(HashMap::default()),
        }
    }

    pub fn platform_default() -> Self {
        Self::new(crate::font_backend::default_font_backend())
    }

    #[cfg(test)]
    pub(crate) fn test_catalog(&self) -> crate::font::metrics::test_catalog::TestFontCatalogSource {
        self.backend.test_catalog()
    }

    #[cfg(test)]
    pub(crate) fn test_foundry_for_file(&self, file: &str) -> Option<String> {
        self.backend.test_foundry_for_file(file)
    }

    pub fn backend_kind(&self) -> FontBackendKind {
        self.backend.kind()
    }

    pub(crate) fn poll_catalog_change(&mut self) -> crate::font::catalog::FontCatalogChange {
        self.backend.poll_catalog_change()
    }

    /// List platform families in native discovery order, removing duplicate
    /// names just as GNU's frame-level font-driver dispatcher does.
    pub fn list_families(&self) -> Vec<FontFamilyName> {
        let mut seen = HashSet::default();
        self.backend
            .list_families()
            .into_iter()
            .filter(|family| seen.insert(family.clone()))
            .collect()
    }

    pub fn resolve_family(&self, family: &str) -> String {
        self.backend.resolve_family(family)
    }

    pub fn family_prefers_monospace(&self, family: &str) -> bool {
        self.backend.family_prefers_monospace(family)
    }

    /// Resolve the first entity matching a GNU font spec through the active
    /// platform adapter.  Unlike the former binary-side helper this never
    /// assumes Fontconfig on CoreText or DirectWrite builds.
    pub fn resolve_entity(&self, query: &FontEntityQuery) -> Option<ResolvedFontEntity> {
        if !self.native_registry_query_allowed(query.registry.as_deref(), None) {
            return None;
        }
        let candidate_query = match self.entity_candidate_query(query) {
            Ok(query) => query,
            Err(error) => {
                tracing::warn!(target: "font_registry", %error, "invalid native font policy");
                return None;
            }
        };
        let selected =
            match query.selection {
                FontSpecSelection::Enumerate => self.select_entity_candidates(
                    self.backend.list_candidates(&candidate_query),
                    query,
                    &candidate_query,
                    FontSpecSelection::Enumerate,
                ),
                FontSpecSelection::DriverMatch => self.match_entity_driver(&candidate_query, query),
                FontSpecSelection::OpenBySpec => self
                    .select_entity_candidates(
                        self.backend.list_candidates(&candidate_query),
                        query,
                        &candidate_query,
                        FontSpecSelection::OpenBySpec,
                    )
                    .or_else(|| {
                        // GNU font_load_for_lface first honors explicit listing
                        // constraints; on a miss, font_matching_entity replaces
                        // the style slots with font_open_by_spec's normal attrs.
                        let mut preferences = candidate_query.clone();
                        preferences.requested_weight = 400;
                        preferences.requested_slant = FontSlant::Normal;
                        preferences.requested_width = FontWidth::Normal;
                        self.match_entity_driver(&preferences, query)
                            .filter(|matched| {
                                // Native finalization can supply a previously
                                // unknown name; style fallback must preserve the
                                // requested identity once that name is available.
                                query.postscript_name.as_ref().is_none_or(|requested| {
                                    matched.identity.postscript_name.as_ref().is_some_and(
                                        |actual| actual.eq_ignore_ascii_case(requested),
                                    )
                                })
                            })
                    }),
            }?;
        Some(ResolvedFontEntity {
            matched: selected,
            registry: Some(result_registry(
                self.backend.kind(),
                query.registry.as_deref(),
            )),
            driver: font_driver_symbol(self.backend.kind()),
        })
    }

    /// Enumerate catalog metadata in native discovery order. Native assets
    /// stay deferred until an exact listed candidate is selected for use.
    pub fn list_entities(
        &self,
        query: &FontEntityQuery,
        constraints: &FontEntityListConstraints,
        prefer: Option<(&FontEntityQuery, &FontEntityListConstraints)>,
        maximum: Option<usize>,
    ) -> Vec<ListedFontCandidate> {
        let mut listed = Vec::new();
        if maximum == Some(0) {
            return listed;
        }
        self.visit_entities(query, constraints, prefer, |entity| {
            listed.push(entity);
            if maximum.is_some_and(|maximum| listed.len() >= maximum) {
                std::ops::ControlFlow::Break(())
            } else {
                std::ops::ControlFlow::Continue(())
            }
        });
        listed
    }

    /// Visit metadata until the consumer has enough accepted results. Without
    /// PREFER, a break also stops exact file-capability probes. Preference
    /// sorting must first classify all matching candidates to rank their sizes.
    pub fn visit_entities(
        &self,
        query: &FontEntityQuery,
        constraints: &FontEntityListConstraints,
        prefer: Option<(&FontEntityQuery, &FontEntityListConstraints)>,
        mut visitor: impl FnMut(ListedFontCandidate) -> std::ops::ControlFlow<()>,
    ) {
        let driver = font_driver_symbol(self.backend.kind());
        if !self.native_registry_query_allowed(query.registry.as_deref(), constraints.driver) {
            return;
        }
        let mut candidate_query = match self.entity_candidate_query(query) {
            Ok(query) => query,
            Err(error) => {
                tracing::warn!(target: "font_registry", %error, "invalid native font policy");
                return;
            }
        };
        if self.backend.kind() == FontBackendKind::Fontconfig {
            candidate_query.otf = constraints.otf.clone();
            candidate_query.script_coverage = constraints.script_coverage.clone();
            candidate_query.scalable = constraints.scalable;
        }
        let policy = FontEntityMatchPolicy::for_backend(self.backend.kind());
        // ftfont_list checks the registry's language independently of an
        // explicit ADSTYLE. Extra :lang remains a native query constraint.
        let registry_adstyle_language = if self.backend.kind() == FontBackendKind::Fontconfig {
            match query
                .registry
                .as_deref()
                .map(crate::font::policy::registry_language)
                .transpose()
            {
                Ok(language) => language.flatten(),
                Err(error) => {
                    tracing::warn!(target: "font_registry", %error, "invalid native font policy");
                    return;
                }
            }
        } else {
            None
        };
        // Native discovery owns family membership. Other cheap selector
        // filters precede any exact file inspection.
        let candidates = self
            .backend
            .list_candidates(&candidate_query)
            .into_iter()
            .filter(|candidate| {
                let metadata = &candidate.matched.metadata;
                entity_matches_query(candidate, query, policy)
                    && constraints.foundry.as_ref().is_none_or(|foundry| {
                        metadata
                            .foundry
                            .as_ref()
                            .is_some_and(|actual| actual.eq_ignore_ascii_case(foundry))
                    })
                    && constraints
                        .adstyle
                        .as_deref()
                        .filter(|style| !style.is_empty())
                        .is_none_or(|requested| {
                            let requested = if self.backend.kind() == FontBackendKind::DirectWrite {
                                // w32_generic_family compares exact symbols;
                                // unrecognized names impose no family class.
                                match requested {
                                    "mono" | "monospace" => "mono",
                                    "sans" | "sans-serif" | "sansserif" => "sans",
                                    "serif" | "script" | "decorative" => requested,
                                    _ => return true,
                                }
                            } else {
                                requested
                            };
                            metadata
                                .adstyle
                                .as_deref()
                                .is_some_and(|actual| actual.eq_ignore_ascii_case(requested))
                        })
                    && registry_adstyle_language.is_none_or(|language| {
                        metadata
                            .adstyle
                            .as_deref()
                            .is_none_or(|actual| actual.eq_ignore_ascii_case(language))
                    })
                    && constraints.spacing.is_none_or(|spacing| {
                        metadata.spacing.is_none_or(|actual| actual == spacing)
                    })
            });
        let size_matches = |candidate: &FontCandidate| {
            let metadata = &candidate.matched.metadata;
            // GNU filters only known, nonzero average widths. A requested
            // zero still excludes known nonzero widths; unknown/zero remains
            // unconstrained, independently of the font's strike size.
            constraints.average_width.is_none_or(|requested| {
                metadata
                    .average_width
                    .is_none_or(|actual| actual == 0 || actual == requested)
            }) && constraints
                .pixel_size
                // GNU's primary filter tests `if (size)`. Numeric zero is
                // still meaningful in the separate PREFER scoring path.
                .filter(|size| *size != 0)
                .is_none_or(|size| match metadata.size {
                    PlatformFontSize::Fixed { device_ppem_26_6 } => {
                        (device_ppem_26_6 >> 6).abs_diff(size) <= 1
                    }
                    PlatformFontSize::Scalable => true,
                    PlatformFontSize::Unknown => false,
                })
        };
        let listed = |candidate: FontCandidate| ListedFontCandidate {
            candidate: candidate.matched,
            registry: Some(result_registry(
                self.backend.kind(),
                query.registry.as_deref(),
            )),
            driver,
        };
        if let Some((prefer, constraints)) = prefer {
            let mut candidates: Vec<_> = self
                .classify_unknown_candidate_sizes(candidates.collect())
                .into_iter()
                .filter(size_matches)
                .collect();
            candidates.sort_by_key(|candidate| {
                let metadata = &candidate.matched.metadata;
                let score = candidate_listing_score(
                    prefer.weight.map(FontWeight::from_css_weight),
                    prefer.slant,
                    prefer.width,
                    metadata.weight.map(FontWeight::from_css_weight),
                    metadata.slant,
                    metadata.width,
                    constraints
                        .pixel_size
                        .map(|size| (size.saturating_mul(64), metadata.size)),
                )
                .map(|score| {
                    if constraints.pixel_size.is_some()
                        && matches!(metadata.size, PlatformFontSize::Fixed { .. })
                        && (constraints.dpi.is_some()
                            || constraints
                                .average_width
                                .is_some_and(|requested| metadata.average_width != Some(requested)))
                    {
                        score.with_size_metadata_mismatch()
                    } else {
                        score
                    }
                });
                (score.is_none(), score)
            });
            for candidate in candidates {
                if visitor(listed(candidate)).is_break() {
                    return;
                }
            }
        } else {
            let mut emit = |candidate| {
                if size_matches(&candidate) {
                    visitor(listed(candidate))
                } else {
                    std::ops::ControlFlow::Continue(())
                }
            };
            for candidate in candidates {
                if candidate.matched.metadata.size == PlatformFontSize::Unknown {
                    // One unknown file can expand to several fixed strikes.
                    // Inspect it only when native discovery reaches it.
                    for classified in self.classify_unknown_candidate_sizes(vec![candidate]) {
                        if emit(classified).is_break() {
                            return;
                        }
                    }
                } else if emit(candidate).is_break() {
                    // Known native metadata needs no per-candidate allocation.
                    return;
                }
            }
        }
    }

    /// Finalize this exact catalog candidate without another selection pass.
    pub(crate) fn materialize_font_candidate(
        &self,
        candidate: PlatformFontCandidate,
    ) -> Option<PlatformFontMatch> {
        self.backend.finalize_match(candidate)
    }

    /// Public callers use the fallible result before enumeration. Native
    /// Option-returning selection paths record and skip invalid policies.
    pub(crate) fn validate_font_entity_listing(
        &self,
        registry: Option<&str>,
        driver: Option<SymId>,
    ) -> Result<bool, EmacsRegexpError> {
        let backend = self.backend.kind();
        if driver.is_some_and(|requested| Some(requested) != font_driver_symbol(backend)) {
            return Ok(false);
        }
        let Some(registry) = registry else {
            return Ok(true);
        };
        match backend {
            FontBackendKind::Fontconfig => super::policy::registry_supported_for_listing(registry),
            FontBackendKind::DirectWrite => Ok(super::policy::windows_registry_shortcut(registry)
                || super::policy::legacy_backend_registry_hint(registry).is_some()),
            FontBackendKind::CoreText => Ok(true),
            // Do not guess registry support for an unimplemented adapter.
            _ => Ok(false),
        }
    }

    fn native_registry_query_allowed(&self, registry: Option<&str>, driver: Option<SymId>) -> bool {
        match self.validate_font_entity_listing(registry, driver) {
            Ok(allowed) => allowed,
            Err(error) => {
                tracing::warn!(target: "font_registry", %error, "invalid native font policy");
                false
            }
        }
    }

    fn entity_candidate_query(
        &self,
        query: &FontEntityQuery,
    ) -> Result<FontCandidateQuery, EmacsRegexpError> {
        let family = query
            .family
            .as_ref()
            .and_then(|family| FontFamilyName::new(self.resolve_family(family.as_str())));
        let spec = StoredFontSpec {
            family: family.as_ref().map(|family| intern(family.as_str())),
            registry: query
                .registry
                .as_deref()
                .filter(|registry| {
                    super::policy::registry_needs_native_constraints(self.backend.kind(), registry)
                })
                .map(intern),
            lang: query
                .language
                .as_deref()
                .map(|language| intern(&language.to_ascii_lowercase())),
            weight: query
                .weight
                .map(neovm_core::face::FontWeight::from_css_weight),
            slant: query.slant,
            width: query.width,
            definition: None,
        };
        let constraints = GnuFontPolicy::constraints_for_entity(&spec)?;
        let representative = constraints.representative_char();
        let charset_ranges = constraints.coverage().ranges().to_vec();
        let languages: Vec<String> = constraints
            .languages()
            .iter()
            .map(|language| language.as_str().to_owned())
            .collect();
        let scope = family.map_or(FontCandidateScope::All, FontCandidateScope::Family);
        Ok(FontCandidateQuery {
            signature_requirement: super::policy::registry_signature_requirement(
                self.backend.kind(),
                query.registry.as_deref(),
            ),
            scalable: None,
            otf: None,
            script_coverage: None,
            scope,
            // Entity enumeration is not character fallback.  Registry
            // constraints are already represented by `charset_ranges`; GNU
            // does not invent an additional required character for generic
            // registries such as iso10646-1.
            required: RequiredFontCoverage::Any,
            charset_ranges,
            languages,
            explicit_language: query.language.clone(),
            requested_weight: query.weight.unwrap_or(400),
            requested_slant: query.slant.unwrap_or(FontSlant::Normal),
            requested_width: query.width.unwrap_or(FontWidth::Normal),
            direction: TextDirection::for_char(representative),
        })
    }

    fn match_entity_driver(
        &self,
        preferences: &FontCandidateQuery,
        query: &FontEntityQuery,
    ) -> Option<PlatformFontMatch> {
        match self.backend.match_font_spec(preferences) {
            crate::font_backend::FontDriverMatch::Native(candidate) => {
                self.backend.finalize_match(candidate?.matched)
            }
            crate::font_backend::FontDriverMatch::Enumerated(candidates) => self
                .select_entity_candidates(
                    candidates,
                    query,
                    preferences,
                    FontSpecSelection::DriverMatch,
                ),
        }
    }

    fn select_entity_candidates(
        &self,
        candidates: Vec<FontCandidate>,
        query: &FontEntityQuery,
        preferences: &FontCandidateQuery,
        selection: FontSpecSelection,
    ) -> Option<PlatformFontMatch> {
        candidates
            .into_iter()
            .enumerate()
            .filter(|(_, candidate)| match selection {
                FontSpecSelection::Enumerate | FontSpecSelection::OpenBySpec => {
                    entity_matches_query(
                        candidate,
                        query,
                        FontEntityMatchPolicy::for_backend(self.backend.kind()),
                    )
                }
                FontSpecSelection::DriverMatch => entity_matches_identity(candidate, query),
            })
            .min_by_key(|(ordinal, candidate)| {
                let score = match selection {
                    FontSpecSelection::Enumerate => EntityCandidateScore::Enumeration(
                        requested_width_distance(query.width, candidate.matched.metadata.width),
                    ),
                    FontSpecSelection::OpenBySpec | FontSpecSelection::DriverMatch => {
                        EntityCandidateScore::Preferred(candidate_style_selection_score(
                            FontWeight::from_css_weight(preferences.requested_weight),
                            preferences.requested_slant,
                            Some(preferences.requested_width),
                            FontWeight::from_css_weight(candidate.matched.weight().unwrap_or(400)),
                            candidate.matched.slant(),
                            candidate.matched.metadata.width,
                        ))
                    }
                };
                (score, *ordinal)
            })
            .and_then(|(_, candidate)| self.backend.finalize_match(candidate.matched))
    }

    /// Open one selected entity through its owning backend for `font-info`.
    /// Native design metrics win; Linux falls back to probing the exact
    /// Fontconfig/FreeType identity selected above.
    pub fn open_entity(
        &self,
        query: &FontEntityQuery,
        pixel_size: u32,
    ) -> Option<OpenedFontEntity> {
        let entity = self.resolve_entity(query)?;
        let (matched, metrics) = self.open_match(entity.matched, pixel_size, query.weight)?;
        Some(OpenedFontEntity {
            entity: ResolvedFontEntity { matched, ..entity },
            metrics,
        })
    }

    /// Obtain metrics from an already selected identity without family reselection.
    pub(crate) fn open_match(
        &self,
        matched: PlatformFontMatch,
        pixel_size: u32,
        requested_weight: Option<u16>,
    ) -> Option<(PlatformFontMatch, crate::font::probe::FontPxMetrics)> {
        let matched = self.with_native_metrics(matched);
        let pixel_size = pixel_size.max(1);
        let metrics = matched.pixel_metrics(pixel_size as f32).or_else(|| {
            let identity = &matched.identity;
            let file = identity.file_path.as_deref()?;
            let explicit_weight = identity
                .variation_coords
                .iter()
                .find(|coord| coord.tag() == u32::from_be_bytes(*b"wght"))
                .map(|coord| coord.value())
                .or_else(|| requested_weight.map(f32::from));
            crate::font::probe::probe_font_px_metrics(
                file,
                identity.freetype_selector()?,
                pixel_size,
                explicit_weight,
            )
        })?;
        Some((matched, metrics))
    }

    /// Resolve a primary face from candidates in one concrete family.
    pub fn resolve_primary(
        &self,
        family: &str,
        requested_weight: u16,
        requested_slant: FontSlant,
        requested_width: FontWidth,
        size: FontSelectionSize,
    ) -> Option<PlatformFontMatch> {
        let key = PrimaryCacheKey {
            family: family.to_string(),
            weight: requested_weight,
            slant: requested_slant.gnu_numeric(),
            width: requested_width.gnu_numeric(),
            size,
        };
        if let Ok(cache) = self.primary_cache.lock()
            && let Some(cached) = cache.get(&key)
        {
            return cached.clone();
        }
        let family = self.resolve_family(family);
        let family_name = FontFamilyName::new(family.clone())?;
        let query = FontCandidateQuery {
            signature_requirement: None,
            scalable: None,
            otf: None,
            script_coverage: None,
            scope: FontCandidateScope::Family(family_name),
            required: RequiredFontCoverage::Any,
            charset_ranges: Vec::new(),
            languages: Vec::new(),
            explicit_language: None,
            requested_weight,
            requested_slant,
            requested_width,
            direction: TextDirection::LeftToRight,
        };
        let selected = select_best_candidate(
            self.classify_unknown_candidate_sizes(self.backend.list_candidates(&query)),
            &SelectionRequest {
                weight: requested_weight,
                slant: requested_slant,
                width: Some(requested_width),
                spacing: None,
                prefer_monospace: self.family_prefers_monospace(&family),
                queried_family: Some(&family),
                size,
            },
        )
        .and_then(|matched| self.backend.finalize_match(matched))
        .map(|matched| self.with_native_metrics(matched));
        if let Ok(mut cache) = self.primary_cache.lock() {
            cache.insert(key, selected.clone());
        }
        selected
    }

    /// Observe an already selected face without running style selection again.
    ///
    /// GNU opens the selected entity in `font_open_entity`; it does not choose
    /// another family member to obtain its metrics. FAMILY is only a discovery
    /// hint: pinned fontdb faces can carry synthetic family names. The full
    /// identity alone authorizes native finalization/observation.
    /// Cache misses too, since shaping can select faces absent from the native
    /// catalog. Both outcomes expire with the catalog's other observations.
    pub(crate) fn observe_exact_font(
        &self,
        identity: &ResolvedFontIdentity,
        family: &str,
    ) -> Option<PlatformFontMatch> {
        if let Ok(cache) = self.exact_cache.lock()
            && let Some(cached) = cache.get(identity)
        {
            return cached.clone();
        }
        let mut query = FontCandidateQuery {
            signature_requirement: None,
            scalable: None,
            otf: None,
            script_coverage: None,
            scope: FontCandidateScope::All,
            required: RequiredFontCoverage::Any,
            charset_ranges: Vec::new(),
            languages: Vec::new(),
            explicit_language: None,
            requested_weight: 400,
            requested_slant: FontSlant::Normal,
            requested_width: FontWidth::Normal,
            direction: TextDirection::LeftToRight,
        };
        let mut scopes = FontFamilyName::new(family)
            .map(FontCandidateScope::Family)
            .into_iter()
            .chain(std::iter::once(FontCandidateScope::All));
        let matched = scopes
            .find_map(|scope| {
                query.scope = scope;
                self.backend
                    .list_candidates_for_identity(&query, identity)
                    .into_iter()
                    .filter(|candidate| candidate.matched.identity == *identity)
                    .find_map(|candidate| {
                        self.backend
                            .finalize_match(candidate.matched)
                            .filter(|matched| matched.identity == *identity)
                    })
            })
            .map(|matched| self.with_native_metrics(matched));
        if let Ok(mut cache) = self.exact_cache.lock() {
            cache.insert(identity.clone(), matched.clone());
        }
        matched
    }

    /// Resolve the first usable fontset entry for a non-ASCII character.
    pub fn resolve_for_char(
        &self,
        family: &str,
        ch: char,
        requested_weight: u16,
        requested_slant: FontSlant,
        requested_width: FontWidth,
        size: FontSelectionSize,
    ) -> Option<PlatformFontMatch> {
        if ch.is_ascii() {
            return None;
        }
        let key = CharCacheKey {
            family: family.to_string(),
            ch,
            weight: requested_weight,
            slant: requested_slant.gnu_numeric(),
            width: requested_width.gnu_numeric(),
            fontset_generation: self
                .worker_policy
                .as_ref()
                .map_or_else(fontset_generation, |policy| {
                    policy.policies.generation.get()
                }),
            size,
        };
        if self.worker_policy.is_some() {
            return self.resolve_worker_character(key);
        }
        if let Ok(cache) = self.char_cache.lock()
            && let Some(cached) = cache.get(&key)
        {
            return cached.clone();
        }

        let prefer_monospace = self.family_prefers_monospace(family);
        let mut selected = None;
        let mut allow_generic_fallback = true;
        for entry in matching_entries_for_char(ch) {
            match entry {
                FontSpecEntry::ExplicitNone => {
                    allow_generic_fallback = false;
                    break;
                }
                FontSpecEntry::Font(spec) => {
                    let attempt = match self.resolve_from_spec(
                        family,
                        ch,
                        prefer_monospace,
                        requested_weight,
                        requested_slant,
                        requested_width,
                        size,
                        &spec,
                    ) {
                        Ok(attempt) => attempt,
                        Err(error) => {
                            tracing::warn!(target: "font_registry", %error, "invalid native font policy");
                            return None;
                        }
                    };
                    if let Some(matched) = attempt {
                        selected = Some(matched);
                        break;
                    }
                }
            }
        }

        if selected.is_none() && allow_generic_fallback {
            selected = match self.resolve_from_spec(
                family,
                ch,
                prefer_monospace,
                requested_weight,
                requested_slant,
                requested_width,
                size,
                &StoredFontSpec {
                    family: None,
                    registry: None,
                    lang: None,
                    weight: None,
                    slant: None,
                    width: None,
                    definition: None,
                },
            ) {
                Ok(attempt) => attempt,
                Err(error) => {
                    tracing::warn!(target: "font_registry", %error, "invalid native font policy");
                    return None;
                }
            };
        }
        let selected = selected
            .and_then(|matched| self.backend.finalize_match(matched))
            .map(|matched| self.with_native_metrics(matched));
        if let Ok(mut cache) = self.char_cache.lock() {
            cache.insert(key, selected.clone());
        }
        selected
    }

    fn with_native_metrics(&self, mut matched: PlatformFontMatch) -> PlatformFontMatch {
        if matched.metadata.design_metrics.is_none() {
            matched.metadata.design_metrics = self.backend.design_metrics(&matched);
        }
        matched
    }

    /// Complete partial native size metadata before GNU entity scoring.
    /// `Unknown` is classified by opening the exact file face; it never
    /// silently receives the score of a scalable entity.
    fn classify_unknown_candidate_sizes(
        &self,
        candidates: Vec<FontCandidate>,
    ) -> Vec<FontCandidate> {
        let mut classified = Vec::with_capacity(candidates.len());
        for candidate in candidates {
            if candidate.matched.metadata.size != PlatformFontSize::Unknown {
                classified.push(candidate);
                continue;
            }
            let identity_key = candidate.matched.identity.stable_key.clone();
            let Some(asset) = candidate.matched.locator.file() else {
                tracing::warn!(
                    target: "font_boundary",
                    identity = %identity_key,
                    "native font candidate reported unknown size without a file asset"
                );
                continue;
            };
            match self.inspect_capability(asset) {
                Err(neomacs_font_materializer::FontMaterializationError::NotBitmapFace) => {
                    let mut scalable = candidate;
                    scalable.matched.metadata.size = PlatformFontSize::Scalable;
                    classified.push(scalable);
                }
                Ok(neomacs_font_materializer::FontCapability::FreeTypeBitmap { strikes }) => {
                    classified.extend(strikes.into_iter().filter_map(|strike| {
                        let ppem = u32::try_from(strike.y_ppem_26_6).ok()?;
                        let mut fixed = candidate.clone();
                        fixed.matched.metadata.size = PlatformFontSize::Fixed {
                            device_ppem_26_6: ppem,
                        };
                        Some(fixed)
                    }));
                }
                Err(error) => {
                    tracing::warn!(
                        target: "font_boundary",
                        identity = %identity_key,
                        %error,
                        "could not classify platform font size before GNU scoring"
                    );
                }
            }
        }
        classified
    }

    fn inspect_capability(
        &self,
        asset: &neomacs_display_protocol::font::FontFileAsset,
    ) -> Result<
        neomacs_font_materializer::FontCapability,
        neomacs_font_materializer::FontMaterializationError,
    > {
        if let Ok(cache) = self.capability_cache.lock()
            && let Some(cached) = cache.get(asset)
        {
            return cached.clone();
        }
        let result = self
            .materializer
            .as_ref()
            .ok_or(neomacs_font_materializer::FontMaterializationError::BackendUnavailable)
            .and_then(|materializer| materializer.inspect(asset));
        if let Ok(mut cache) = self.capability_cache.lock() {
            cache.insert(asset.clone(), result.clone());
        }
        result
    }

    pub(crate) fn clear_caches(&mut self) {
        if let Some(policy) = &mut self.worker_policy {
            policy.admitted_queries.clear();
        }
        self.backend.advance_catalog_generation();
        self.primary_cache
            .get_mut()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clear();
        self.char_cache
            .get_mut()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clear();
        self.exact_cache
            .get_mut()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clear();
        self.capability_cache
            .get_mut()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clear();
    }

    #[allow(clippy::too_many_arguments)]
    fn resolve_from_spec(
        &self,
        requested_family: &str,
        ch: char,
        prefer_monospace: bool,
        requested_weight: u16,
        requested_slant: FontSlant,
        requested_width: FontWidth,
        size: FontSelectionSize,
        spec: &StoredFontSpec,
    ) -> Result<Option<PlatformFontCandidate>, EmacsRegexpError> {
        if self.backend.kind() != FontBackendKind::Fontconfig
            && !self.native_registry_query_allowed(
                spec.registry
                    .map(neovm_core::emacs_core::fontset::native_font_name)
                    .as_deref(),
                None,
            )
        {
            return Ok(None);
        }
        let policy = CapturedCharacterPolicy::capture(
            requested_family,
            ch,
            requested_weight,
            requested_slant,
            requested_width,
            size,
            spec,
            self.backend.kind(),
        )?;
        Ok(self.resolve_from_policy(&policy, prefer_monospace))
    }

    /// Native discovery and scoring use only owned policy, never Lisp symbols
    /// or the current fontset. The caller owns generation validation.
    fn resolve_from_policy(
        &self,
        policy: &CapturedCharacterPolicy,
        prefer_monospace: bool,
    ) -> Option<PlatformFontCandidate> {
        let search_order = policy
            .families
            .search_order(|family| self.backend.resolve_family(family));
        for family in search_order {
            let scope = match &family {
                FontFamilyCandidate::Family(family) => {
                    FontCandidateScope::Family(FontFamilyName::new(family.as_ref())?)
                }
                FontFamilyCandidate::NativeFallback => FontCandidateScope::NativeFallback {
                    base_family: FontFamilyName::new(
                        self.resolve_family(&policy.requested_family),
                    )?,
                },
            };
            let query = FontCandidateQuery {
                signature_requirement: policy.signature_requirement,
                scalable: None,
                otf: None,
                script_coverage: None,
                scope,
                required: RequiredFontCoverage::Character(policy.ch),
                charset_ranges: policy.charset_ranges.clone(),
                languages: policy.languages.clone(),
                explicit_language: policy.explicit_language.clone(),
                requested_weight: policy.weight,
                requested_slant: policy.slant,
                requested_width: policy.width,
                direction: TextDirection::for_char(policy.ch),
            };
            let request = SelectionRequest {
                weight: policy.weight,
                slant: policy.slant,
                // GNU font_select_entity prefers the face's width when the
                // fontset spec leaves it unset, as for weight and slant.
                width: Some(policy.width),
                spacing: None,
                prefer_monospace,
                queried_family: family.as_family(),
                size: policy.size,
            };
            if let Some(matched) = select_best_candidate(
                self.classify_unknown_candidate_sizes(self.backend.list_candidates(&query)),
                &request,
            ) {
                return Some(matched);
            }
        }
        None
    }

    #[cfg(test)]
    pub(crate) fn replace_backend(&mut self, backend: Box<dyn FontBackend>) {
        self.backend = backend;
        self.primary_cache
            .get_mut()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clear();
        self.char_cache
            .get_mut()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clear();
        self.exact_cache
            .get_mut()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clear();
        self.capability_cache
            .get_mut()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clear();
    }
}

/// One fontset alternative, detached from the evaluator. Keep capture per
/// alternative so the live resolver does not inspect rules after its winner.
#[derive(Clone, Debug, PartialEq, Eq)]
struct CapturedCharacterPolicy {
    requested_family: String,
    ch: char,
    families: CapturedFontFamilyPolicy,
    charset_ranges: Vec<(u32, u32)>,
    languages: Vec<String>,
    explicit_language: Option<String>,
    signature_requirement: Option<crate::font_backend::FontSignatureRequirement>,
    weight: u16,
    slant: FontSlant,
    width: FontWidth,
    size: FontSelectionSize,
}

impl CapturedCharacterPolicy {
    #[allow(clippy::too_many_arguments)] // The captured font request also needs its native registry namespace.
    fn capture(
        family: &str,
        ch: char,
        weight: u16,
        slant: FontSlant,
        width: FontWidth,
        size: FontSelectionSize,
        spec: &StoredFontSpec,
        backend: FontBackendKind,
    ) -> Result<Self, EmacsRegexpError> {
        Self {
            requested_family: family.to_owned(),
            ch,
            families: FontFamilySource::for_spec(family, spec).capture(),
            charset_ranges: Vec::new(),
            languages: Vec::new(),
            explicit_language: None,
            signature_requirement: None,
            weight,
            slant,
            width,
            size,
        }
        .with_spec(spec.into(), backend)
    }

    /// The single registry/language and style-override rule for both live and
    /// detached alternatives. Only their owned naming authority differs.
    fn with_spec(
        mut self,
        spec: super::policy::FontPolicySpec<'_>,
        backend: FontBackendKind,
    ) -> Result<Self, EmacsRegexpError> {
        self.signature_requirement = spec.signature_requirement(backend);
        let spec = spec.for_backend(backend);
        let constraints = GnuFontPolicy::constraints_for_names(&spec, self.ch)?;
        self.charset_ranges = constraints.coverage().ranges().to_vec();
        self.explicit_language = spec.language.as_deref().map(str::to_owned);
        self.languages = constraints
            .languages()
            .iter()
            .map(|language| language.as_str().to_owned())
            .collect();
        self.weight = spec
            .weight
            .map(|weight| weight.css_weight())
            .unwrap_or(self.weight);
        self.slant = spec.slant.unwrap_or(self.slant);
        self.width = spec.width.unwrap_or(self.width);
        Ok(self)
    }
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct PrimaryCacheKey {
    family: String,
    weight: u16,
    slant: u16,
    width: u16,
    size: FontSelectionSize,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct CharCacheKey {
    family: String,
    ch: char,
    weight: u16,
    slant: u16,
    width: u16,
    fontset_generation: u64,
    size: FontSelectionSize,
}

struct SelectionRequest<'a> {
    weight: u16,
    slant: FontSlant,
    width: Option<FontWidth>,
    spacing: Option<i32>,
    prefer_monospace: bool,
    queried_family: Option<&'a str>,
    size: FontSelectionSize,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum EntityCandidateScore {
    Enumeration(u16),
    Preferred(CandidateSelectionScore),
}

fn entity_matches_identity(candidate: &FontCandidate, query: &FontEntityQuery) -> bool {
    query.postscript_name.as_ref().is_none_or(|requested| {
        candidate
            .matched
            .identity
            .postscript_name
            .as_ref()
            .is_some_and(|actual| actual.eq_ignore_ascii_case(requested))
    })
}

fn entity_matches_query(
    candidate: &FontCandidate,
    query: &FontEntityQuery,
    policy: FontEntityMatchPolicy,
) -> bool {
    entity_matches_identity(candidate, query)
        && query
            .weight
            .is_none_or(|weight| policy.weight_matches(weight, candidate.matched.weight()))
        && query
            .slant
            .is_none_or(|slant| candidate.matched.slant() == slant)
        && query
            .width
            .is_none_or(|width| candidate.matched.metadata.width == Some(width))
}

fn requested_width_distance(requested: Option<FontWidth>, candidate: Option<FontWidth>) -> u16 {
    requested.map_or(0, |requested| {
        candidate
            .unwrap_or(FontWidth::Normal)
            .gnu_numeric()
            .abs_diff(requested.gnu_numeric())
    })
}

fn select_best_candidate(
    candidates: Vec<FontCandidate>,
    request: &SelectionRequest<'_>,
) -> Option<PlatformFontCandidate> {
    // GNU scores every entity independently. Equal scores retain the entity's
    // own Fontconfig discovery order; a different named instance in the same
    // variable file must not donate an earlier ordinal to the winner.
    let selected = candidates
        .into_iter()
        .enumerate()
        .filter_map(|(ordinal, candidate)| {
            let score = candidate_score(&candidate, request)?;
            Some((ordinal, score, candidate.matched))
        })
        .min_by_key(|(ordinal, score, _)| (*score, *ordinal));
    if let Some((ordinal, score, matched)) = selected.as_ref() {
        tracing::trace!(
            target: "font_boundary",
            family = matched.family(),
            identity = %matched.identity.stable_key,
            weight = matched.weight(),
            slant = ?matched.slant(),
            ordinal,
            ?score,
            "shared font resolver selected platform candidate"
        );
    }
    selected.map(|(_, _, matched)| matched)
}

fn candidate_score(
    candidate: &FontCandidate,
    request: &SelectionRequest<'_>,
) -> Option<CandidateSelectionScore> {
    let candidate_weight = candidate.matched.weight().unwrap_or(400);
    let compatibility =
        spacing_score(
            request.spacing,
            candidate.matched.metadata.spacing,
            request.prefer_monospace,
        ) + family_affinity_score(request.queried_family, candidate.matched.family());
    candidate_selection_score(
        compatibility,
        request.size.device_px_26_6(),
        FontWeight::from_css_weight(request.weight),
        request.slant,
        request.width,
        FontWeight::from_css_weight(candidate_weight),
        candidate.matched.slant(),
        candidate.matched.metadata.width,
        candidate.matched.metadata.size,
    )
}

fn spacing_score(
    requested_spacing: Option<i32>,
    candidate_spacing: Option<i32>,
    prefer_monospace: bool,
) -> u32 {
    let requested = requested_spacing.and_then(normalize_spacing);
    let candidate = candidate_spacing.and_then(normalize_spacing);
    match (requested, candidate) {
        (Some(requested), Some(candidate)) if requested == candidate => 0,
        (Some(SpacingClass::Mono | SpacingClass::Charcell), Some(SpacingClass::Dual))
            if prefer_monospace =>
        {
            25
        }
        (Some(SpacingClass::Dual), Some(SpacingClass::Mono | SpacingClass::Charcell))
            if prefer_monospace =>
        {
            25
        }
        (Some(_), None) if prefer_monospace => 800,
        (Some(requested), Some(candidate)) => spacing_distance(requested, candidate),
        // GNU ftfont does not turn a monospace family into an implicit exact
        // FC_SPACING request. `prefer_monospace` only relaxes an explicit
        // mono/dual request above.
        _ => 0,
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum SpacingClass {
    Proportional,
    Dual,
    Mono,
    Charcell,
}

fn normalize_spacing(spacing: i32) -> Option<SpacingClass> {
    match spacing {
        i32::MIN..=-1 => None,
        0..=89 => Some(SpacingClass::Proportional),
        90..=99 => Some(SpacingClass::Dual),
        100..=109 => Some(SpacingClass::Mono),
        _ => Some(SpacingClass::Charcell),
    }
}

fn spacing_distance(requested: SpacingClass, candidate: SpacingClass) -> u32 {
    use SpacingClass::{Charcell, Dual, Mono, Proportional};
    match (requested, candidate) {
        (Proportional, Dual) | (Dual, Proportional) => 500,
        (Proportional, Mono) | (Mono, Proportional) => 800,
        (Proportional, Charcell) | (Charcell, Proportional) => 1_000,
        (Dual, Mono) | (Mono, Dual) => 200,
        (Dual, Charcell) | (Charcell, Dual) => 250,
        (Mono, Charcell) | (Charcell, Mono) => 100,
        _ => 0,
    }
}

fn family_affinity_score(queried_family: Option<&str>, candidate_family: &str) -> u32 {
    let Some(queried_family) = queried_family.filter(|family| !family.is_empty()) else {
        return 0;
    };
    let queried = queried_family.to_ascii_lowercase();
    let candidate = candidate_family.to_ascii_lowercase();
    if candidate == queried {
        0
    } else if candidate.starts_with(&queried) || queried.starts_with(&candidate) {
        5
    } else if candidate.contains(&queried) || queried.contains(&candidate) {
        15
    } else {
        80
    }
}

#[cfg(test)]
#[path = "resolver/tests/resolver_test.rs"]
mod tests;
