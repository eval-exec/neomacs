//! Shared GNU-compatible font selection policy.
//!
//! GNU Emacs keeps fontset lookup and entity scoring in `fontset.c`/`font.c`;
//! platform drivers only list/open entities and answer coverage questions.
//! [`FontResolver`] preserves that split.  A [`FontBackend`] may use
//! Fontconfig, CoreText, or DirectWrite to discover candidates, but it never
//! decides which fontset entry or style wins.

use crate::font::policy::GnuFontPolicy;
use crate::font::selection::{CandidateSelectionScore, candidate_selection_score};
use crate::font_backend::{
    FontBackend, FontCandidate, FontCandidateQuery, FontCandidateScope, FontFamilyName,
    FontSelectionSize, PlatformFontCandidate, PlatformFontMatch, PlatformFontSize,
    RequiredFontCoverage, TextDirection,
};
use neomacs_display_protocol::font::FontBackendKind;
use neovm_core::emacs_core::font::alternative_font_families;
use neovm_core::emacs_core::fontset::{
    FontSpecEntry, StoredFontSpec, fontset_generation, matching_entries_for_char,
};
use neovm_core::emacs_core::intern::{intern, resolve_sym};
use neovm_core::face::{FontSlant, FontWeight, FontWidth};
use rustc_hash::{FxHashMap as HashMap, FxHashSet as HashSet};
use std::sync::Mutex;

/// Platform-neutral request for GNU `list-fonts` / `find-font` entity
/// discovery.  Optional fields remain optional all the way to the native
/// adapter; no platform is selected by the caller.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FontEntityQuery {
    family: Option<FontFamilyName>,
    registry: Option<String>,
    language: Option<String>,
    postscript_name: Option<String>,
    weight: Option<u16>,
    slant: Option<FontSlant>,
    width: Option<FontWidth>,
}

impl FontEntityQuery {
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

/// One exact entity returned by the active platform backend.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedFontEntity {
    pub matched: PlatformFontMatch,
    pub registry: Option<String>,
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
        if candidate == requested {
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
}

impl FontResolver {
    pub fn new(backend: Box<dyn FontBackend>) -> Self {
        Self {
            backend,
            materializer: neomacs_font_materializer::FontMaterializer::new().ok(),
            capability_cache: Mutex::new(HashMap::default()),
            primary_cache: Mutex::new(HashMap::default()),
            char_cache: Mutex::new(HashMap::default()),
        }
    }

    pub fn platform_default() -> Self {
        Self::new(crate::font_backend::default_font_backend())
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
            repertory: None,
        };
        let constraints = GnuFontPolicy::constraints_for_entity(&spec);
        let representative = constraints.representative_char();
        let charset_ranges = constraints.coverage().ranges().to_vec();
        let languages: Vec<String> = constraints
            .languages()
            .iter()
            .map(|language| language.as_str().to_owned())
            .collect();
        let scope = family.map_or(FontCandidateScope::All, FontCandidateScope::Family);
        let candidate_query = FontCandidateQuery {
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
        };
        let selected = self
            .backend
            .list_candidates(&candidate_query)
            .into_iter()
            .enumerate()
            .filter(|(_, candidate)| {
                entity_matches_query(
                    candidate,
                    query,
                    FontEntityMatchPolicy::for_backend(self.backend.kind()),
                )
            })
            .min_by_key(|(ordinal, candidate)| {
                (
                    requested_width_distance(query.width, candidate.matched.metadata.width),
                    *ordinal,
                )
            })
            .and_then(|(_, candidate)| self.backend.finalize_match(candidate.matched))?;
        Some(ResolvedFontEntity {
            matched: selected,
            registry: Some("iso10646-1".to_owned()),
        })
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
                crate::font::probe::probe_font_px_metrics(
                    file,
                    identity.freetype_selector()?,
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
        )
        .and_then(|matched| self.backend.finalize_match(matched))
        .map(|matched| self.with_native_metrics(matched));
        if let Ok(mut cache) = self.primary_cache.lock() {
            cache.insert(key, selected.clone());
        }
        selected
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
            fontset_generation: fontset_generation(),
            size,
        };
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
                    if let Some(matched) = self.resolve_from_spec(
                        family,
                        ch,
                        prefer_monospace,
                        requested_weight,
                        requested_slant,
                        requested_width,
                        size,
                        &spec,
                    ) {
                        selected = Some(matched);
                        break;
                    }
                }
            }
        }

        if selected.is_none() && allow_generic_fallback {
            selected = self.resolve_from_spec(
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
                    repertory: None,
                },
            );
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
        self.backend.advance_catalog_generation();
        self.primary_cache
            .get_mut()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clear();
        self.char_cache
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
    ) -> Option<PlatformFontCandidate> {
        let effective_weight = spec
            .weight
            .map(|weight| weight.css_weight())
            .unwrap_or(requested_weight);
        let effective_slant = spec.slant.unwrap_or(requested_slant);
        let effective_width = spec.width.unwrap_or(requested_width);
        let constraints = GnuFontPolicy::constraints_for_character(spec, ch);
        let charset_ranges = constraints.coverage().ranges().to_vec();
        let languages: Vec<String> = constraints
            .languages()
            .iter()
            .map(|language| language.as_str().to_owned())
            .collect();
        let search_order = family_search_order(self.backend.as_ref(), requested_family, spec);

        for family in search_order {
            let scope = match family.as_deref() {
                Some(family) => FontCandidateScope::Family(FontFamilyName::new(family)?),
                None => FontCandidateScope::NativeFallback {
                    base_family: FontFamilyName::new(self.resolve_family(requested_family))?,
                },
            };
            let query = FontCandidateQuery {
                scope,
                required: RequiredFontCoverage::Character(ch),
                charset_ranges: charset_ranges.clone(),
                languages: languages.clone(),
                requested_weight: effective_weight,
                requested_slant: effective_slant,
                requested_width: effective_width,
                direction: TextDirection::for_char(ch),
            };
            let request = SelectionRequest {
                weight: effective_weight,
                slant: effective_slant,
                width: spec.width,
                spacing: None,
                prefer_monospace,
                queried_family: family.as_deref(),
                size,
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
        self.capability_cache
            .get_mut()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clear();
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

fn entity_matches_query(
    candidate: &FontCandidate,
    query: &FontEntityQuery,
    policy: FontEntityMatchPolicy,
) -> bool {
    query.postscript_name.as_ref().is_none_or(|requested| {
        candidate
            .matched
            .identity
            .postscript_name
            .as_ref()
            .is_some_and(|actual| actual.eq_ignore_ascii_case(requested))
    }) && query
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

fn family_search_order(
    backend: &dyn FontBackend,
    requested_family: &str,
    spec: &StoredFontSpec,
) -> Vec<Option<String>> {
    if let Some(spec_family) = spec.family.map(resolve_sym) {
        return vec![Some(backend.resolve_family(spec_family))];
    }
    if requested_family.is_empty() {
        return vec![None];
    }

    let mut order = Vec::new();
    for family in alternative_font_families(requested_family) {
        let resolved = backend.resolve_family(&family);
        if resolved != family {
            order.push(Some(resolved));
        }
        order.push(Some(family));
    }
    // `None` asks the native backend for its ordered cascade from the base
    // family. This is discovery, not policy: it is reached only after every
    // GNU fontset/alternative-family pass has failed.
    order.push(None);
    order
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
        request.weight,
        request.slant,
        request.width,
        candidate_weight,
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
#[path = "resolver_test.rs"]
mod tests;
