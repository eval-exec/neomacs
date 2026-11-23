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
    otf_representative_ranges, otf_spec_matches_for_listing,
};
use crate::font::probe::OtfCapability;
use crate::font::selection::{
    CandidateSelectionScore, candidate_selection_score, candidate_style_selection_score,
};
use crate::font_backend::{
    FontBackend, FontCandidate, FontCandidateQuery, FontCandidateScope, FontFamilyName,
    FontSelectionSize, PlatformFontCandidate, PlatformFontMatch, PlatformFontSize,
    RequiredFontCoverage, TextDirection,
};
use neomacs_display_protocol::font::{FontBackendKind, FontFileAsset, ResolvedFontIdentity};
use neovm_core::emacs_core::eval::FontSpecSelection;
use neovm_core::emacs_core::fontset::{
    FontSpecEntry, StoredFontSpec, StoredOtfSpec, fontset_generation, rendering_groups_for_char,
};
use neovm_core::emacs_core::intern::intern;
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
    /// GNU `:otf`: enumerated entities must satisfy this OpenType requirement.
    otf: Option<StoredOtfSpec>,
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
        self.registry = non_empty_query_text(registry.into());
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

    pub fn with_otf(mut self, otf: StoredOtfSpec) -> Self {
        self.otf = Some(otf);
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

/// Catalog metadata before acceptance and native asset finalization.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListedFontCandidate {
    pub candidate: PlatformFontCandidate,
    pub registry: Option<String>,
    pub driver: Option<neovm_core::emacs_core::intern::SymId>,
}

/// Constraints for catalog listing, separate from driver matching and opening.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FontEntityListConstraints {
    pub driver: Option<neovm_core::emacs_core::intern::SymId>,
    pub foundry: Option<String>,
    pub adstyle: Option<String>,
    pub pixel_size: Option<u32>,
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
    /// GNU `:otf` matching reads each candidate's GSUB/GPOS capability; cache
    /// one parsed capability per (file, face) so a fontset lookup pays it once.
    /// The cache validates file identity, so a replaced font file is re-probed.
    otf_capability_cache: Mutex<crate::font::probe::OtfCapabilityCache>,
    #[cfg(target_os = "linux")]
    render_options_cache: Mutex<
        HashMap<
            (String, u32, u32, Option<u32>),
            Option<neomacs_font_materializer::fontconfig::FontRenderOptions>,
        >,
    >,
    primary_cache: Mutex<HashMap<PrimaryCacheKey, Option<PlatformFontMatch>>>,
    char_cache: Mutex<HashMap<CharCacheKey, Option<PlatformFontMatch>>>,
    exact_cache: Mutex<HashMap<ResolvedFontIdentity, Option<PlatformFontMatch>>>,
    /// Fonts already opened for one face and font-def, in open order.
    ///
    /// GNU `fontset_find_font` checks these with `font_has_char` before
    /// `font_find_for_lface` (`src/fontset.c`). The key is the face plus the
    /// font spec, not the character, so a later character covered by an
    /// opened fallback does not get a new equal-score pick.
    opened_fallbacks: Mutex<HashMap<OpenedFallbackKey, Vec<PlatformFontMatch>>>,
    /// Fallback specs in the order the next character tries them.
    ///
    /// GNU moves the found fallback object to the front of the fallback
    /// group (`src/fontset.c`, the `found:` label) and leaves the
    /// character-specific group in charset order. Empty means "no move yet".
    fallback_order: Mutex<Vec<FontSpecEntry>>,
}

impl FontResolver {
    pub fn new(backend: Box<dyn FontBackend>) -> Self {
        Self {
            backend,
            worker_policy: None,
            materializer: neomacs_font_materializer::FontMaterializer::new().ok(),
            capability_cache: Mutex::new(HashMap::default()),
            otf_capability_cache: Mutex::new(crate::font::probe::OtfCapabilityCache::default()),
            #[cfg(target_os = "linux")]
            render_options_cache: Mutex::new(HashMap::default()),
            primary_cache: Mutex::new(HashMap::default()),
            char_cache: Mutex::new(HashMap::default()),
            exact_cache: Mutex::new(HashMap::default()),
            opened_fallbacks: Mutex::new(HashMap::default()),
            fallback_order: Mutex::new(Vec::new()),
        }
    }

    /// Cache policy under this resolver's catalog owner and exact physical size.
    #[cfg(target_os = "linux")]
    pub(crate) fn render_options(
        &self,
        file: &str,
        selector: u32,
        size: f32,
        weight: Option<f32>,
    ) -> Option<neomacs_font_materializer::fontconfig::FontRenderOptions> {
        let key = (
            file.to_owned(),
            selector,
            size.to_bits(),
            weight.map(f32::to_bits),
        );
        if let Ok(cache) = self.render_options_cache.lock()
            && let Some(options) = cache.get(&key)
        {
            return *options;
        }
        let options = self.backend.render_options(file, selector, size, weight);
        if let Ok(mut cache) = self.render_options_cache.lock() {
            cache.insert(key, options);
        }
        options
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
        let candidate_query = self.entity_candidate_query(query);
        let selected = match query.selection {
            FontSpecSelection::Enumerate => self.select_entity_candidates(
                self.filter_entity_candidates(
                    query,
                    self.backend.list_candidates(&candidate_query),
                ),
                query,
                &candidate_query,
                FontSpecSelection::Enumerate,
            ),
            FontSpecSelection::DriverMatch => self
                .match_entity_driver(&candidate_query, query)
                .filter(|matched| self.driver_match_satisfies_otf(query, matched)),
            FontSpecSelection::OpenBySpec => self
                .select_entity_candidates(
                    self.filter_entity_candidates(
                        query,
                        self.backend.list_candidates(&candidate_query),
                    ),
                    query,
                    &candidate_query,
                    FontSpecSelection::OpenBySpec,
                )
                .filter(|matched| self.driver_match_satisfies_otf(query, matched))
                .or_else(|| {
                    // GNU font_load_for_lface first honors explicit listing
                    // constraints; on a miss, font_matching_entity replaces
                    // the style slots with font_open_by_spec's normal attrs.
                    let mut preferences = candidate_query.clone();
                    preferences.requested_weight = 400;
                    preferences.requested_slant = FontSlant::Normal;
                    preferences.requested_width = FontWidth::Normal;
                    self.match_entity_driver(&preferences, query)
                        .filter(|matched| self.driver_match_satisfies_otf(query, matched))
                }),
        }?;
        Some(ResolvedFontEntity {
            matched: selected,
            registry: Some("iso10646-1".to_owned()),
            driver: font_driver_symbol(self.backend.kind()),
        })
    }

    /// Enumerate every matching entity in native discovery order. Preference
    /// sorting is stable and the limit is applied before native asset loading.
    pub fn list_entities(
        &self,
        query: &FontEntityQuery,
        constraints: &FontEntityListConstraints,
        prefer: Option<(&FontEntityQuery, &FontEntityListConstraints)>,
        maximum: Option<usize>,
    ) -> Vec<ResolvedFontEntity> {
        let mut entities = Vec::new();
        match self.try_visit_entities::<std::convert::Infallible>(
            query,
            constraints,
            prefer,
            maximum,
            None,
            &mut |entity| {
                entities.push(entity);
                std::ops::ControlFlow::Continue(())
            },
        ) {
            Ok(()) => entities,
            Err(never) => match never {},
        }
    }

    /// Apply the caller's metadata policy before GNU's numeric postfilters.
    /// Only accepted, selected entities are finalized. The limit counts
    /// successful finalizations; rejected and unavailable assets do not use it.
    #[allow(clippy::type_complexity)]
    pub fn try_visit_entities<E>(
        &self,
        query: &FontEntityQuery,
        constraints: &FontEntityListConstraints,
        prefer: Option<(&FontEntityQuery, &FontEntityListConstraints)>,
        maximum: Option<usize>,
        mut filter: Option<&mut dyn FnMut(&ListedFontCandidate) -> Result<bool, E>>,
        visitor: &mut dyn FnMut(ResolvedFontEntity) -> std::ops::ControlFlow<()>,
    ) -> Result<(), E> {
        if maximum == Some(0) {
            return Ok(());
        }
        if constraints
            .driver
            .is_some_and(|driver| Some(driver) != font_driver_symbol(self.backend.kind()))
        {
            return Ok(());
        }
        if query
            .registry
            .as_deref()
            .is_some_and(|registry| !crate::font::policy::registry_supported_for_listing(registry))
        {
            return Ok(());
        }
        let candidate_query = self.entity_candidate_query(query);
        let policy = FontEntityMatchPolicy::for_backend(self.backend.kind());
        let candidates = self.filter_entity_candidates(query, self.backend.list_candidates(&candidate_query))
            .into_iter()
            .filter(|candidate| {
                let metadata = &candidate.matched.metadata;
                entity_matches_identity(candidate, query)
                    && constraints.foundry.as_ref().is_none_or(|foundry| {
                        metadata.foundry.as_ref().is_some_and(|actual| actual.eq_ignore_ascii_case(foundry))
                    })
                    // A non-empty additional style requires catalog metadata;
                    // it must never become an unconstrained family fallback.
                    && constraints.adstyle.as_ref().is_none_or(String::is_empty)
                    && constraints.spacing.is_none_or(|spacing| metadata.spacing.is_none_or(|actual| actual == spacing))
            });
        let driver = font_driver_symbol(self.backend.kind());
        let mut admit = |candidate: FontCandidate| -> Result<Option<ListedFontCandidate>, E> {
            let listed = ListedFontCandidate {
                candidate: candidate.matched,
                registry: Some("iso10646-1".to_owned()),
                driver,
            };
            if let Some(filter) = filter.as_mut()
                && !filter(&listed)?
            {
                return Ok(None);
            }
            let metadata = &listed.candidate.metadata;
            // This catalog does not publish measured nonzero average widths.
            // GNU leaves unknown/zero widths unconstrained; strike size is
            // not evidence of a nonzero width (font.c:font_delete_unmatched).
            let matches = entity_styles_match(metadata, query, policy)
                && constraints.pixel_size.filter(|size| *size != 0).is_none_or(
                    |size| match metadata.size {
                        PlatformFontSize::Fixed { device_ppem_26_6 } => {
                            (device_ppem_26_6 >> 6).abs_diff(size) <= 1
                        }
                        PlatformFontSize::Scalable | PlatformFontSize::Unknown => true,
                    },
                );
            Ok(matches.then_some(listed))
        };
        let mut count = 0;
        let mut emit = |listed: ListedFontCandidate| {
            let metadata = listed.candidate.metadata.clone();
            let Some(mut matched) = self.backend.finalize_match(listed.candidate) else {
                return std::ops::ControlFlow::Continue(());
            };
            // Listing names and filters describe the native catalog entry.
            // Finalization may enrich the exact asset/variation identity, but
            // must not replace metadata that already passed acceptance/MAX.
            matched.metadata = metadata;
            count += 1;
            let outcome = visitor(ResolvedFontEntity {
                matched,
                registry: listed.registry,
                driver: listed.driver,
            });
            if outcome.is_break() || maximum.is_some_and(|maximum| count >= maximum) {
                std::ops::ControlFlow::Break(())
            } else {
                std::ops::ControlFlow::Continue(())
            }
        };
        if let Some((prefer, constraints)) = prefer {
            let mut accepted = Vec::new();
            for candidate in candidates {
                if let Some(listed) = admit(candidate)? {
                    accepted.push(listed);
                }
            }
            accepted.sort_by_key(|listed| {
                let metadata = &listed.candidate.metadata;
                let weight = FontWeight::from_css_weight(metadata.weight.unwrap_or(400));
                let requested_weight = prefer
                    .weight
                    .map(FontWeight::from_css_weight)
                    .unwrap_or(weight);
                let requested_slant = prefer.slant.unwrap_or(metadata.slant);
                let score = if let Some(size) = constraints.pixel_size {
                    candidate_selection_score(
                        0,
                        size.saturating_mul(64),
                        requested_weight,
                        requested_slant,
                        prefer.width,
                        weight,
                        metadata.slant,
                        metadata.width,
                        metadata.size,
                    )
                } else {
                    Some(candidate_style_selection_score(
                        requested_weight,
                        requested_slant,
                        prefer.width,
                        weight,
                        metadata.slant,
                        metadata.width,
                    ))
                };
                (score.is_none(), score)
            });
            for listed in accepted {
                if emit(listed).is_break() {
                    break;
                }
            }
        } else {
            for candidate in candidates {
                if let Some(listed) = admit(candidate)?
                    && emit(listed).is_break()
                {
                    break;
                }
            }
        }
        Ok(())
    }

    fn entity_candidate_query(&self, query: &FontEntityQuery) -> FontCandidateQuery {
        let family = query
            .family
            .as_ref()
            .and_then(|family| FontFamilyName::new(self.resolve_family(family.as_str())));
        let spec = StoredFontSpec {
            family: family.as_ref().map(|family| intern(family.as_str())),
            registry: query
                .registry
                .as_deref()
                .map(|registry| intern(&registry.to_ascii_lowercase())),
            lang: query
                .language
                .as_deref()
                .map(|language| intern(&language.to_ascii_lowercase())),
            weight: query
                .weight
                .map(neovm_core::face::FontWeight::from_css_weight),
            slant: query.slant,
            width: query.width,
            otf: None,
            definition: None,
        };
        let constraints = GnuFontPolicy::constraints_for_entity(&spec);
        let representative = constraints.representative_char();
        let mut charset_ranges = constraints.coverage().ranges().to_vec();
        if let Some(otf) = query.otf.as_ref() {
            charset_ranges.extend(otf_representative_ranges(otf, query.registry.as_deref()));
        }
        let languages: Vec<String> = constraints
            .languages()
            .iter()
            .map(|language| language.as_str().to_owned())
            .collect();
        let scope = family.map_or(FontCandidateScope::All, FontCandidateScope::Family);
        FontCandidateQuery {
            scope,
            // Entity enumeration is not character fallback.  Registry
            // constraints are already represented by `charset_ranges`; GNU
            // does not invent an additional required character for generic
            // registries such as iso10646-1.
            required: RequiredFontCoverage::Any,
            charset_ranges,
            languages,
            requested_weight: query.weight.unwrap_or(400),
            requested_slant: query.slant.unwrap_or(FontSlant::Normal),
            requested_width: query.width.unwrap_or(FontWidth::Normal),
            direction: TextDirection::for_char(representative),
        }
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
        let mut entity = self.resolve_entity(query)?;
        entity.matched = self.with_native_metrics(entity.matched);
        let pixel_size = pixel_size.max(1);
        let metrics = entity
            .matched
            .pixel_metrics(pixel_size as f32)
            .or_else(|| {
                let identity = &entity.matched.identity;
                let file = identity.file_path.as_deref()?;
                let explicit_weight = identity
                    .variation_coords
                    .iter()
                    .find(|coord| coord.tag() == u32::from_be_bytes(*b"wght"))
                    .map(|coord| coord.value())
                    .or_else(|| query.weight.map(f32::from));
                let selector = identity.freetype_selector()?;
                #[cfg(target_os = "linux")]
                return crate::font::probe::probe_font_px_metrics_with_policy(
                    file,
                    selector,
                    pixel_size,
                    explicit_weight,
                    |size| self.render_options(file, selector, size as f32, explicit_weight),
                );
                #[cfg(not(target_os = "linux"))]
                crate::font::probe::probe_font_px_metrics(
                    file,
                    selector,
                    pixel_size,
                    explicit_weight,
                )
            })?;
        Some(OpenedFontEntity { entity, metrics })
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
            scope: FontCandidateScope::Family(family_name),
            required: RequiredFontCoverage::Any,
            charset_ranges: Vec::new(),
            languages: Vec::new(),
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
            EqualScoreTie::Discovery,
            &[],
        )
        .and_then(|matched| self.backend.finalize_match(matched))
        .map(|matched| self.with_native_metrics(matched));
        if let Ok(mut cache) = self.primary_cache.lock() {
            cache.insert(key, selected.clone());
        }
        selected
    }

    /// GNU font_find_for_lface treats the face family and foundry as
    /// preferences, independently relaxing each before scoring native entities.
    pub fn resolve_face_font(
        &self,
        family: &str,
        foundry: Option<&str>,
        weight: u16,
        slant: FontSlant,
        width: FontWidth,
        size: FontSelectionSize,
    ) -> Option<PlatformFontMatch> {
        let families = FontFamilySource::Face(family).capture();
        self.resolve_face_policy(&families, foundry, weight, slant, width, size)
    }

    fn resolve_face_policy(
        &self,
        families: &CapturedFontFamilyPolicy,
        foundry: Option<&str>,
        weight: u16,
        slant: FontSlant,
        width: FontWidth,
        size: FontSelectionSize,
    ) -> Option<PlatformFontMatch> {
        for family in families.search_order(|family| self.backend.resolve_family(family)) {
            let scope = match family {
                FontFamilyCandidate::Family(family) => {
                    FontCandidateScope::Family(FontFamilyName::new(family.as_ref())?)
                }
                // This is GNU's family=nil enumeration, not a character's
                // native fallback cascade or a generic monospace request.
                FontFamilyCandidate::NativeFallback => FontCandidateScope::All,
            };
            let query = FontCandidateQuery {
                scope,
                required: RequiredFontCoverage::Any,
                charset_ranges: Vec::new(),
                languages: Vec::new(),
                requested_weight: weight,
                requested_slant: slant,
                requested_width: width,
                direction: TextDirection::LeftToRight,
            };
            let candidates = self.backend.list_candidates(&query);
            for foundry in foundry.into_iter().map(Some).chain(std::iter::once(None)) {
                let matching = candidates
                    .iter()
                    .filter(|candidate| {
                        foundry.is_none_or(|foundry| {
                            candidate
                                .matched
                                .metadata
                                .foundry
                                .as_ref()
                                .is_some_and(|actual| actual.eq_ignore_ascii_case(foundry))
                        })
                    })
                    .cloned()
                    .collect();
                let request = SelectionRequest {
                    weight,
                    slant,
                    width: Some(width),
                    spacing: None,
                    prefer_monospace: false,
                    queried_family: None,
                    size,
                };
                if let Some(matched) = select_best_candidate(
                    self.classify_unknown_candidate_sizes(matching),
                    &request,
                    EqualScoreTie::Discovery,
                    &[],
                )
                .and_then(|selected| self.backend.finalize_match(selected))
                {
                    return Some(self.with_native_metrics(matched));
                }
            }
        }
        None
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
            scope: FontCandidateScope::All,
            required: RequiredFontCoverage::Any,
            charset_ranges: Vec::new(),
            languages: Vec::new(),
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

        let generation = key.fontset_generation;
        let prefer_monospace = self.family_prefers_monospace(family);
        let mut resolution = None;
        let mut allow_generic_fallback = true;
        let (specific, fallback) = rendering_groups_for_char(ch);
        let fallback = self.ordered_fallback_group(fallback);
        for entry in specific {
            let keep_going = self.consider_fontset_entry(
                &entry,
                family,
                ch,
                requested_weight,
                requested_slant,
                requested_width,
                size,
                prefer_monospace,
                generation,
                &mut allow_generic_fallback,
                &mut resolution,
            );
            if !keep_going || resolution.is_some() {
                break;
            }
        }
        if resolution.is_none() && allow_generic_fallback {
            for entry in &fallback {
                if !self.consider_fontset_entry(
                    entry,
                    family,
                    ch,
                    requested_weight,
                    requested_slant,
                    requested_width,
                    size,
                    prefer_monospace,
                    generation,
                    &mut allow_generic_fallback,
                    &mut resolution,
                ) {
                    break;
                }
                if resolution.is_some() {
                    self.promote_fallback(entry);
                    break;
                }
            }
        }

        if resolution.is_none() && allow_generic_fallback {
            let policy = CapturedCharacterPolicy::capture(
                family,
                ch,
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
                    otf: None,
                    definition: None,
                },
            );
            resolution = self.resolve_spec(policy, prefer_monospace, generation);
        }
        let selected = self.finish_character_resolution(resolution);
        if let Ok(mut cache) = self.char_cache.lock() {
            cache.insert(key, selected.clone());
        }
        selected
    }

    /// Reuse an opened font for this face and font-def when it covers `ch`.
    /// A miss lists candidates and records the newly opened font.
    fn resolve_spec(
        &self,
        policy: CapturedCharacterPolicy,
        prefer_monospace: bool,
        generation: u64,
    ) -> Option<SpecResolution> {
        let group = OpenedFallbackKey::from_policy(&policy, generation);
        // A character the requested family covers must not inherit a fallback
        // opened for a different character. That reuse changed é from 16px on
        // a fresh engine to 17px after offscreen preparation.
        if let Some(matched) =
            self.resolve_from_policy(&policy, prefer_monospace, CatalogPass::NamedFamilies)
        {
            return Some(SpecResolution::Fresh(group, matched));
        }
        if let Some(reused) = self.reuse_opened(&group, policy.ch) {
            return Some(SpecResolution::Reused(reused));
        }
        self.resolve_from_policy(
            &policy,
            prefer_monospace,
            CatalogPass::UnconstrainedFallback,
        )
        .map(|matched| SpecResolution::Fresh(group, matched))
    }

    fn finish_character_resolution(
        &self,
        resolution: Option<SpecResolution>,
    ) -> Option<PlatformFontMatch> {
        match resolution {
            Some(SpecResolution::Reused(matched)) => Some(matched),
            Some(SpecResolution::Fresh(group, matched)) => {
                let selected = self
                    .backend
                    .finalize_match(matched)
                    .map(|matched| self.with_native_metrics(matched));
                if let Some(selected) = &selected {
                    self.remember_opened(group, selected.clone());
                }
                selected
            }
            None => None,
        }
    }

    /// Charset order, with any fallback already found moved to the front.
    fn ordered_fallback_group(&self, base: Vec<FontSpecEntry>) -> Vec<FontSpecEntry> {
        let Ok(mut stored) = self.fallback_order.lock() else {
            return base;
        };
        if stored.is_empty() {
            *stored = base.clone();
            return base;
        }
        let mut ordered = Vec::with_capacity(base.len());
        for entry in stored.iter() {
            if base.contains(entry) && !ordered.contains(entry) {
                ordered.push(entry.clone());
            }
        }
        for entry in &base {
            if !ordered.contains(entry) {
                ordered.push(entry.clone());
            }
        }
        *stored = ordered.clone();
        ordered
    }

    /// GNU's `found:` label: the fallback that covered this character is tried
    /// first for the next one. The character-specific group is not reordered.
    fn promote_fallback(&self, found: &FontSpecEntry) {
        let Ok(mut stored) = self.fallback_order.lock() else {
            return;
        };
        if let Some(index) = stored.iter().position(|entry| entry == found) {
            let entry = stored.remove(index);
            stored.insert(0, entry);
        } else {
            stored.insert(0, found.clone());
        }
    }

    /// Try one fontset entry. Returns false when the walk must stop.
    #[allow(clippy::too_many_arguments)]
    fn consider_fontset_entry(
        &self,
        entry: &FontSpecEntry,
        family: &str,
        ch: char,
        requested_weight: u16,
        requested_slant: FontSlant,
        requested_width: FontWidth,
        size: FontSelectionSize,
        prefer_monospace: bool,
        generation: u64,
        allow_generic_fallback: &mut bool,
        resolution: &mut Option<SpecResolution>,
    ) -> bool {
        match entry {
            FontSpecEntry::ExplicitNone => {
                *allow_generic_fallback = false;
                false
            }
            FontSpecEntry::Font(spec) => {
                let policy = CapturedCharacterPolicy::capture(
                    family,
                    ch,
                    requested_weight,
                    requested_slant,
                    requested_width,
                    size,
                    spec,
                );
                if let Some(found) = self.resolve_spec(policy, prefer_monospace, generation) {
                    *resolution = Some(found);
                }
                true
            }
        }
    }

    fn reuse_opened(&self, key: &OpenedFallbackKey, ch: char) -> Option<PlatformFontMatch> {
        let opened = {
            let guard = self.opened_fallbacks.lock().ok()?;
            guard.get(key).cloned().unwrap_or_default()
        };
        opened
            .into_iter()
            .find(|matched| self.opened_font_covers(matched, ch))
    }

    /// GNU `font_has_char` on an already opened font object. This asks the
    /// backend about that identity only; it does not score a new entity list.
    fn opened_font_covers(&self, matched: &PlatformFontMatch, ch: char) -> bool {
        let query = FontCandidateQuery {
            scope: FontCandidateScope::All,
            required: RequiredFontCoverage::Character(ch),
            charset_ranges: Vec::new(),
            languages: Vec::new(),
            requested_weight: matched.weight().unwrap_or(400),
            requested_slant: matched.slant(),
            requested_width: FontWidth::Normal,
            direction: TextDirection::for_char(ch),
        };
        self.backend
            .list_candidates_for_identity(&query, &matched.identity)
            .iter()
            .any(|candidate| candidate.matched.identity == matched.identity)
    }

    fn remember_opened(&self, key: OpenedFallbackKey, matched: PlatformFontMatch) {
        let Ok(mut opened) = self.opened_fallbacks.lock() else {
            return;
        };
        let fonts = opened.entry(key).or_default();
        if fonts
            .iter()
            .any(|existing| existing.identity == matched.identity)
        {
            return;
        }
        fonts.push(matched);
    }

    pub(super) fn with_native_metrics(&self, mut matched: PlatformFontMatch) -> PlatformFontMatch {
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

    /// Drop opened fallbacks and the per-character answers.
    ///
    /// A redisplay does not call this. The entries live until the catalog
    /// changes, or until `fontset_generation` in the key misses. The worker
    /// holds its own resolver and uses the same boundary. Reuse applies only
    /// after the requested family misses, so a character that family covers
    /// is not drawn with an older fallback.
    pub(crate) fn clear_caches(&mut self) {
        if let Some(policy) = &mut self.worker_policy {
            policy.admitted_queries.clear();
        }
        self.backend.advance_catalog_generation();
        #[cfg(target_os = "linux")]
        self.render_options_cache
            .get_mut()
            .unwrap_or_else(|p| p.into_inner())
            .clear();
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
        self.opened_fallbacks
            .get_mut()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clear();
        self.fallback_order
            .get_mut()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clear();
        self.capability_cache
            .get_mut()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clear();
        self.otf_capability_cache
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
    ) -> Option<PlatformFontCandidate> {
        let policy = CapturedCharacterPolicy::capture(
            requested_family,
            ch,
            requested_weight,
            requested_slant,
            requested_width,
            size,
            spec,
        );
        self.resolve_from_policy(&policy, prefer_monospace, CatalogPass::All)
    }

    /// Native discovery and scoring use only owned policy, never Lisp symbols
    /// or the current fontset. The caller owns generation validation.
    fn resolve_from_policy(
        &self,
        policy: &CapturedCharacterPolicy,
        prefer_monospace: bool,
        pass: CatalogPass,
    ) -> Option<PlatformFontCandidate> {
        let search_order = policy
            .families
            .search_order(|family| self.backend.resolve_family(family));
        for family in search_order {
            let unconstrained = family.as_family().is_none();
            if !pass.allows(unconstrained) {
                continue;
            }
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
                scope,
                required: RequiredFontCoverage::Character(policy.ch),
                charset_ranges: policy.charset_ranges.clone(),
                languages: policy.languages.clone(),
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
            let candidates = self.backend.list_candidates(&query);
            // GNU `font_spec_match` rejects a font whose GSUB/GPOS capability
            // does not satisfy the spec's `:otf` requirement before scoring
            // (`src/font.c:2521-2533`). A font the driver cannot inspect has
            // no capability and therefore does not match.
            let candidates = match &policy.otf {
                Some(otf) => candidates
                    .into_iter()
                    .filter(|candidate| self.candidate_satisfies_otf(otf, candidate))
                    .collect(),
                None => candidates,
            };
            let candidates = self.classify_unknown_candidate_sizes(candidates);
            // FcFontSort reorders only the unconstrained fallback. An explicit
            // fontset family, and the primary face, keep GNU list order when
            // scores tie. Scoring itself is unchanged.
            let (tie, sort_ranks) = if family.as_family().is_none() {
                (
                    EqualScoreTie::Fallback,
                    self.fallback_sort_ranks(policy, &candidates),
                )
            } else {
                (EqualScoreTie::Discovery, Vec::new())
            };
            if let Some(matched) = select_best_candidate(candidates, &request, tie, &sort_ranks) {
                return Some(matched);
            }
        }
        None
    }

    /// `FcFontSort` order for this face and the character's language.
    /// A fontset language wins; otherwise kana, Hangul, and Bopomofo name one.
    fn fallback_sort_ranks(
        &self,
        policy: &CapturedCharacterPolicy,
        candidates: &[FontCandidate],
    ) -> Vec<Option<u32>> {
        let Some(lang) = policy
            .languages
            .first()
            .map(String::as_str)
            .filter(|lang| !lang.is_empty())
            .or_else(|| tie_break_language(policy.ch))
        else {
            return Vec::new();
        };
        if policy.requested_family.is_empty() {
            return Vec::new();
        }
        self.backend
            .equal_score_sort_ranks(&policy.requested_family, lang, candidates)
    }

    /// GNU `font_spec_match`'s `:otf` arm: the candidate must advertise the
    /// required script/language-system features. A candidate without a file
    /// asset cannot be inspected and therefore fails the requirement.
    fn candidate_satisfies_otf(&self, otf: &StoredOtfSpec, candidate: &FontCandidate) -> bool {
        self.asset_satisfies_otf(otf, candidate.matched.locator.file())
    }

    /// Filter enumerated entities by the query's `:otf` requirement, the way
    /// `font_spec_match` drops a font that cannot satisfy the spec.
    fn filter_entity_candidates(
        &self,
        query: &FontEntityQuery,
        candidates: Vec<FontCandidate>,
    ) -> Vec<FontCandidate> {
        let Some(otf) = query.otf.as_ref() else {
            return candidates;
        };
        candidates
            .into_iter()
            .filter(|candidate| self.candidate_satisfies_otf(otf, candidate))
            .collect()
    }

    fn driver_match_satisfies_otf(
        &self,
        query: &FontEntityQuery,
        matched: &PlatformFontMatch,
    ) -> bool {
        query
            .otf
            .as_ref()
            .is_none_or(|otf| self.asset_satisfies_otf(otf, matched.asset.file()))
    }

    fn asset_satisfies_otf(&self, otf: &StoredOtfSpec, asset: Option<&FontFileAsset>) -> bool {
        let Some(asset) = asset else {
            return false;
        };
        let Some(capability) = self.otf_capability(asset) else {
            return false;
        };
        otf_spec_matches_for_listing(otf, &capability)
    }

    fn otf_capability(&self, asset: &FontFileAsset) -> Option<OtfCapability> {
        self.otf_capability_for_file(asset.path(), asset.face_index())
    }

    /// The single production route for GSUB/GPOS capability probing. The
    /// parsed tables live next to the resolver's other native observations so
    /// `clear_caches` retires them with the font backend state.
    pub(crate) fn otf_capability_for_file(
        &self,
        file: &str,
        face_index: u32,
    ) -> Option<OtfCapability> {
        self.otf_capability_cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(file, face_index)
    }

    #[cfg(test)]
    pub(crate) fn replace_backend(&mut self, backend: Box<dyn FontBackend>) {
        self.backend = backend;
        #[cfg(target_os = "linux")]
        self.render_options_cache
            .get_mut()
            .unwrap_or_else(|p| p.into_inner())
            .clear();
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
        self.opened_fallbacks
            .get_mut()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clear();
        self.fallback_order
            .get_mut()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clear();
        self.capability_cache
            .get_mut()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clear();
        self.otf_capability_cache
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
    weight: u16,
    slant: FontSlant,
    width: FontWidth,
    size: FontSelectionSize,
    /// GNU `:otf`: candidates must satisfy this requirement to be selected.
    otf: Option<StoredOtfSpec>,
}

impl CapturedCharacterPolicy {
    fn capture(
        family: &str,
        ch: char,
        weight: u16,
        slant: FontSlant,
        width: FontWidth,
        size: FontSelectionSize,
        spec: &StoredFontSpec,
    ) -> Self {
        Self {
            requested_family: family.to_owned(),
            ch,
            families: FontFamilySource::for_spec(family, spec).capture(),
            charset_ranges: Vec::new(),
            languages: Vec::new(),
            weight,
            slant,
            width,
            size,
            otf: None,
        }
        .with_spec(spec.into())
    }

    /// The single registry/language and style-override rule for both live and
    /// detached alternatives. Only their owned naming authority differs.
    fn with_spec(mut self, spec: super::policy::FontPolicySpec<'_>) -> Self {
        let constraints = GnuFontPolicy::constraints_for_names(&spec, self.ch);
        self.charset_ranges = constraints.coverage().ranges().to_vec();
        if let Some(otf) = spec.otf {
            self.charset_ranges
                .extend(otf_representative_ranges(otf, spec.registry.as_deref()));
        }
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
        self.otf = spec.otf.cloned();
        self
    }
}

enum SpecResolution {
    Reused(PlatformFontMatch),
    Fresh(OpenedFallbackKey, PlatformFontCandidate),
}

/// Identity of one realized font-def on one face. The character is absent:
/// coverage is asked of the opened font, the way `RFONT_DEF_OBJECT` is.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
struct OpenedFallbackKey {
    requested_family: String,
    families: OpenedFamilyKey,
    languages: Vec<String>,
    weight: u16,
    slant: u16,
    width: u16,
    size: FontSelectionSize,
    otf: Option<StoredOtfSpec>,
    fontset_generation: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
enum OpenedFamilyKey {
    Explicit(String),
    Inherited(Vec<String>),
}

impl OpenedFallbackKey {
    fn from_policy(policy: &CapturedCharacterPolicy, fontset_generation: u64) -> Self {
        let families = match &policy.families {
            CapturedFontFamilyPolicy::Explicit(family) => OpenedFamilyKey::Explicit(family.clone()),
            CapturedFontFamilyPolicy::Inherited(families) => {
                OpenedFamilyKey::Inherited(families.iter().map(str::to_owned).collect())
            }
        };
        Self {
            requested_family: policy.requested_family.clone(),
            families,
            languages: policy.languages.clone(),
            weight: policy.weight,
            slant: policy.slant.gnu_numeric(),
            width: policy.width.gnu_numeric(),
            size: policy.size,
            otf: policy.otf.clone(),
            fontset_generation,
        }
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
        && entity_styles_match(&candidate.matched.metadata, query, policy)
}

fn entity_styles_match(
    metadata: &crate::font_backend::PlatformFontMetadata,
    query: &FontEntityQuery,
    policy: FontEntityMatchPolicy,
) -> bool {
    query
        .weight
        .is_none_or(|weight| policy.weight_matches(weight, metadata.weight))
        && query.slant.is_none_or(|slant| metadata.slant == slant)
        && query
            .width
            .is_none_or(|width| metadata.width == Some(width))
}

fn requested_width_distance(requested: Option<FontWidth>, candidate: Option<FontWidth>) -> u16 {
    requested.map_or(0, |requested| {
        candidate
            .unwrap_or(FontWidth::Normal)
            .gnu_numeric()
            .abs_diff(requested.gnu_numeric())
    })
}

/// Language for the fallback `FcFontSort` pattern when the font spec has none.
///
/// Kana, Hangul, and Bopomofo name a language. A bare Han ideograph does
/// not: GNU takes that language from the fontset registry (`fontset.el`
/// orders GB2312/`zh-cn` ahead of JISX0208/`ja` for `han`).
fn tie_break_language(ch: char) -> Option<&'static str> {
    match ch {
        '\u{3040}'..='\u{30FF}' | '\u{31F0}'..='\u{31FF}' | '\u{FF66}'..='\u{FF9D}' => Some("ja"),
        '\u{1100}'..='\u{11FF}' | '\u{3130}'..='\u{318F}' | '\u{AC00}'..='\u{D7AF}' => Some("ko"),
        '\u{3100}'..='\u{312F}' => Some("zh"),
        _ => None,
    }
}

/// Which part of the family search to run.
///
/// Named families are tried before an opened fallback. The unconstrained
/// catalog pass runs only when those families miss, so a character the
/// requested family covers does not inherit an older fallback.
#[derive(Clone, Copy)]
enum CatalogPass {
    NamedFamilies,
    UnconstrainedFallback,
    All,
}

impl CatalogPass {
    fn allows(self, unconstrained: bool) -> bool {
        match self {
            Self::NamedFamilies => !unconstrained,
            Self::UnconstrainedFallback => unconstrained,
            Self::All => true,
        }
    }
}

/// How an equal [`candidate_score`] chooses among entities.
#[derive(Clone, Copy, Eq, PartialEq)]
enum EqualScoreTie {
    /// GNU `font_sort_entities` does not reorder. The driver's list order wins.
    Discovery,
    /// Unconstrained fallback. `FcFontSort`, then family name, then file path.
    Fallback,
}

fn select_best_candidate(
    candidates: Vec<FontCandidate>,
    request: &SelectionRequest<'_>,
    tie: EqualScoreTie,
    sort_ranks: &[Option<u32>],
) -> Option<PlatformFontCandidate> {
    // GNU scores every entity independently and keeps `FcFontList` order on a
    // tie. That order follows the font directory, so only the unconstrained
    // fallback replaces it: `FcFontSort` rank, then family name, then path.
    // The discovery ordinal stays last, so two instances of one file do not
    // borrow each other's rank. Primary and explicit-family searches stay on
    // the GNU list order. An empty rank list is that list order too: macOS
    // and Windows have no `FcFontSort`, and a missing language does not
    // invent an alphabetical order.
    let tie = if tie == EqualScoreTie::Fallback && !sort_ranks.iter().any(|rank| rank.is_some()) {
        EqualScoreTie::Discovery
    } else {
        tie
    };
    let mut ranked = Vec::new();
    for (ordinal, candidate) in candidates.into_iter().enumerate() {
        let Some(score) = candidate_score(&candidate, request) else {
            continue;
        };
        let (sort_rank, family, path) = if tie == EqualScoreTie::Fallback {
            (
                sort_ranks
                    .get(ordinal)
                    .copied()
                    .flatten()
                    .unwrap_or(u32::MAX),
                candidate.matched.family().to_ascii_lowercase(),
                candidate.matched.file_path().unwrap_or("").to_owned(),
            )
        } else {
            (0, String::new(), String::new())
        };
        ranked.push((score, sort_rank, family, path, ordinal, candidate.matched));
    }
    ranked.sort_by(|left, right| {
        let by_score = left.0.cmp(&right.0);
        if tie == EqualScoreTie::Discovery {
            return by_score.then(left.4.cmp(&right.4));
        }
        by_score
            .then(left.1.cmp(&right.1))
            .then(left.2.cmp(&right.2))
            .then(left.3.cmp(&right.3))
            .then(left.4.cmp(&right.4))
    });
    for (rank, (score, sort_rank, _, _, ordinal, matched)) in ranked.iter().enumerate() {
        tracing::trace!(
            target: "font_boundary",
            rank,
            family = matched.family(),
            identity = %matched.identity.stable_key,
            weight = matched.weight(),
            slant = ?matched.slant(),
            ordinal,
            sort_rank,
            ?score,
            "shared font resolver ranked platform candidate"
        );
    }
    let (score, _, _, _, ordinal, matched) = ranked.into_iter().next()?;
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
    Some(matched)
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

#[cfg(test)]
#[path = "resolver/tests/otf_fixture_test.rs"]
mod otf_fixture_tests;
