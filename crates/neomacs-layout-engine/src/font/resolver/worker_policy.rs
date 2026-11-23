//! Bounded admitted queries over owner-shared, immutable native font policy.
use super::*;
use neovm_core::emacs_core::font::{
    FontFamilyCaptureError, FrozenFontFamilies, capture_alternative_font_families,
};
use neovm_core::emacs_core::fontset::{
    CapturedFontsetChain, FontSelectionRevision, FontsetCaptureError, FontsetRenderingEntry,
    FontsetRenderingSpec, capture_default_fontset_chain,
};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FrozenCharacterPolicies {
    pub(super) generation: FontSelectionRevision,
    queries: HashMap<CharCacheKey, FrozenCharacterQuery>,
    bytes: usize,
}

/// Chain and inherited-family storage belong to the mutation authority. A page
/// keeps references, and native resolution visits only the necessary prefix.
#[derive(Clone, Debug, PartialEq, Eq)]
struct FrozenCharacterQuery {
    chain: CapturedFontsetChain,
    inherited: FrozenFontFamilies,
}

pub(super) struct WorkerPolicy {
    pub policies: Arc<FrozenCharacterPolicies>,
    missing: AtomicBool,
    pub(super) admitted_queries: HashSet<CharCacheKey>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FontPolicyCaptureError {
    Unsupported,
    Budget,
    Changed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FontPolicyMergeError {
    Changed,
    Budget,
}

impl FrozenCharacterPolicies {
    /// Join only policies captured under the same authority. Duplicate queries
    /// must carry identical owner references; the worker never consults Lisp.
    pub(crate) fn merged(
        &self,
        other: &Self,
        query_limit: usize,
    ) -> Result<Self, FontPolicyMergeError> {
        if self.generation != other.generation {
            return Err(FontPolicyMergeError::Changed);
        }
        let mut queries = self.queries.clone();
        for (key, query) in &other.queries {
            match queries.entry(key.clone()) {
                std::collections::hash_map::Entry::Occupied(entry) => {
                    if entry.get() != query {
                        return Err(FontPolicyMergeError::Changed);
                    }
                }
                std::collections::hash_map::Entry::Vacant(entry) => {
                    entry.insert(query.clone());
                }
            }
            if queries.len() > query_limit {
                return Err(FontPolicyMergeError::Budget);
            }
        }
        if queries.len() > query_limit {
            return Err(FontPolicyMergeError::Budget);
        }
        let bytes = queries
            .keys()
            .try_fold(std::mem::size_of::<Self>(), |bytes, key| {
                bytes.checked_add(Self::query_bytes(&key.family)?)
            })
            .ok_or(FontPolicyMergeError::Budget)?;
        Ok(Self {
            generation: self.generation,
            queries,
            bytes,
        })
    }

    pub(crate) fn capture(
        requests: &[(&str, char, u16, bool, FontSelectionSize)],
        max_bytes: usize,
    ) -> Result<Self, FontPolicyCaptureError> {
        let generation = FontSelectionRevision::try_from(fontset_generation())
            .map_err(map_fontset_capture_error)?;
        let mut snapshot = Self {
            generation,
            queries: HashMap::default(),
            bytes: std::mem::size_of::<Self>(),
        };
        for &(family, ch, weight, italic, size) in requests {
            if ch.is_ascii() {
                continue;
            }
            let query_bytes = Self::query_bytes(family).ok_or(FontPolicyCaptureError::Budget)?;
            if std::mem::size_of::<Self>()
                .checked_add(query_bytes)
                .ok_or(FontPolicyCaptureError::Budget)?
                > max_bytes
            {
                return Err(FontPolicyCaptureError::Budget);
            }
            let slant = if italic {
                FontSlant::Italic
            } else {
                FontSlant::Normal
            };
            let key = CharCacheKey {
                family: family.to_owned(),
                ch,
                weight,
                slant: slant.gnu_numeric(),
                width: FontWidth::Normal.gnu_numeric(),
                fontset_generation: generation.get(),
                size,
            };
            if snapshot.queries.contains_key(&key) {
                continue;
            }
            if snapshot.queries.len() >= 128 {
                return Err(FontPolicyCaptureError::Budget);
            }
            let bytes = snapshot
                .bytes
                .checked_add(query_bytes)
                .ok_or(FontPolicyCaptureError::Budget)?;
            if bytes > max_bytes {
                return Err(FontPolicyCaptureError::Budget);
            }
            let chain = capture_default_fontset_chain(ch).map_err(map_fontset_capture_error)?;
            if chain.revision() != generation {
                return Err(FontPolicyCaptureError::Changed);
            }
            let inherited =
                capture_alternative_font_families(family).map_err(|error| match error {
                    FontFamilyCaptureError::Unavailable => FontPolicyCaptureError::Unsupported,
                })?;
            snapshot.bytes = bytes;
            snapshot
                .queries
                .insert(key, FrozenCharacterQuery { chain, inherited });
        }
        if fontset_generation() != generation.get() {
            return Err(FontPolicyCaptureError::Changed);
        }
        if snapshot.bytes > max_bytes {
            return Err(FontPolicyCaptureError::Budget);
        }
        Ok(snapshot)
    }

    /// Charge page-owned query headers and spelling storage. Shared fontset and
    /// family-alternative chains are allocated/accounted at their owning policy,
    /// rather than copied and charged again for each character/page admission.
    fn query_bytes(family: &str) -> Option<usize> {
        std::mem::size_of::<CharCacheKey>()
            .checked_add(std::mem::size_of::<FrozenCharacterQuery>())?
            .checked_add(std::mem::size_of::<Arc<str>>())?
            // Conservatively reserve a second request-family spelling for its
            // transient native-query materialization at resolution time.
            .checked_add(family.len().checked_mul(2)?)
    }

    pub(crate) fn bytes(&self) -> usize {
        self.bytes
    }
    pub(crate) fn query_count(&self) -> usize {
        self.queries.len()
    }
}

fn map_fontset_capture_error(error: FontsetCaptureError) -> FontPolicyCaptureError {
    match error {
        FontsetCaptureError::Unavailable | FontsetCaptureError::MissingDefault => {
            FontPolicyCaptureError::Unsupported
        }
        FontsetCaptureError::InvalidRevision => FontPolicyCaptureError::Changed,
    }
}

/// Validate the legacy numeric cache-key boundary once per native resolution.
struct WorkerFontStyle {
    slant: FontSlant,
    width: FontWidth,
}

impl WorkerFontStyle {
    fn from_key(key: &CharCacheKey) -> Option<Self> {
        Some(Self {
            slant: FontSlant::from_gnu_numeric(key.slant)?,
            width: FontWidth::from_gnu_numeric(key.width)?,
        })
    }
}

impl CapturedCharacterPolicy {
    /// Canonical per-visited-entry materialization, also used for the final
    /// unconstrained native fallback. No symbol/evaluator lookup is permitted.
    fn from_rendering_spec(
        key: &CharCacheKey,
        inherited: &FrozenFontFamilies,
        style: &WorkerFontStyle,
        spec: &FontsetRenderingSpec,
    ) -> Self {
        let families = match &spec.family {
            Some(family) => CapturedFontFamilyPolicy::Explicit(family.to_string()),
            None => CapturedFontFamilyPolicy::Inherited(inherited.clone()),
        };
        Self {
            requested_family: key.family.clone(),
            ch: key.ch,
            families,
            charset_ranges: Vec::new(),
            languages: Vec::new(),
            weight: key.weight,
            slant: style.slant,
            width: style.width,
            size: key.size,
            otf: None,
        }
        .with_spec(spec.into())
    }
}

impl FontResolver {
    /// Bound distinct admitted queries, including primary-font selections
    /// that populate measurement caches without entering native fallback.
    /// Repeated requests consume no additional working-set capacity.
    pub(crate) fn worker_policy_fits_cache(
        &self,
        policy: &FrozenCharacterPolicies,
        limit: usize,
    ) -> bool {
        let Some(current) = &self.worker_policy else {
            return policy.queries.len() <= limit;
        };
        current.admitted_queries.len().saturating_add(
            policy
                .queries
                .keys()
                .filter(|key| !current.admitted_queries.contains(*key))
                .count(),
        ) <= limit
    }

    pub(crate) fn install_worker_policy(&mut self, policies: Arc<FrozenCharacterPolicies>) {
        let mut admitted_queries = self
            .worker_policy
            .take()
            .map(|previous| previous.admitted_queries)
            .unwrap_or_default();
        admitted_queries.extend(policies.queries.keys().cloned());
        self.worker_policy = Some(WorkerPolicy {
            policies,
            missing: AtomicBool::new(false),
            admitted_queries,
        });
    }
    pub(crate) fn worker_policy_missing(&self) -> bool {
        self.worker_policy
            .as_ref()
            .is_some_and(|policy| policy.missing.load(Ordering::Relaxed))
    }
    pub(super) fn resolve_worker_character(
        &self,
        mut key: CharCacheKey,
    ) -> super::CharacterResolution {
        let Some(state) = self.worker_policy.as_ref() else {
            return super::CharacterResolution::none();
        };
        key.fontset_generation = state.policies.generation.get();
        let Some(query) = state.policies.queries.get(&key) else {
            state.missing.store(true, Ordering::Relaxed);
            return super::CharacterResolution::none();
        };
        let Some(style) = WorkerFontStyle::from_key(&key) else {
            return super::CharacterResolution::none();
        };
        let order_key = crate::font::fallback_order::FallbackOrderKey {
            family: key.family.clone(),
            weight: key.weight,
            slant: key.slant,
            width: key.width,
            size: Some(key.size),
            fontset_generation: key.fontset_generation,
        };
        if let Some(cached) = self.cached_character(&key, &order_key) {
            return cached;
        }
        let (selected, fallback_winner) =
            self.resolve_frozen_query(&key, query, &style, &order_key);
        let fallback = fallback_winner
            .map(|winner| crate::font::fallback_order::FallbackAnswerStamp { order_key, winner });
        self.store_character(key, fallback.clone(), selected.clone());
        super::CharacterResolution {
            font: selected,
            fallback,
        }
    }

    /// Specific group, then the shared fallback order. A failed open tries the
    /// next entry and does not move. Only a fallback that finishes moves.
    fn resolve_frozen_query(
        &self,
        key: &CharCacheKey,
        query: &FrozenCharacterQuery,
        style: &WorkerFontStyle,
        order_key: &crate::font::fallback_order::FallbackOrderKey,
    ) -> (Option<PlatformFontMatch>, Option<FontSpecEntry>) {
        let prefer_monospace = self.family_prefers_monospace(&key.family);
        let (specific, fallback) = query.chain.rendering_groups();
        for (_, entry) in specific {
            match entry {
                FontsetRenderingEntry::ExplicitNone => return (None, None),
                FontsetRenderingEntry::Font(spec) => {
                    if let Some(font) =
                        self.open_rendering_entry(key, query, style, &spec, prefer_monospace)
                    {
                        return (Some(font), None);
                    }
                }
            }
        }
        let ordered = order_fallback_pairs(order_key, fallback);
        for (spec, entry) in ordered {
            match entry {
                FontsetRenderingEntry::ExplicitNone => return (None, None),
                FontsetRenderingEntry::Font(rendering) => {
                    if let Some(font) =
                        self.open_rendering_entry(key, query, style, &rendering, prefer_monospace)
                    {
                        crate::font::fallback_order::promote_fallback(order_key, &spec);
                        return (Some(font), Some(spec));
                    }
                }
            }
        }
        let fallback = FontsetRenderingSpec {
            family: None,
            registry: None,
            lang: None,
            weight: None,
            slant: None,
            width: None,
            otf: None,
        };
        let font = self.open_rendering_entry(key, query, style, &fallback, prefer_monospace);
        (font, None)
    }

    fn open_rendering_entry(
        &self,
        key: &CharCacheKey,
        query: &FrozenCharacterQuery,
        style: &WorkerFontStyle,
        spec: &FontsetRenderingSpec,
        prefer_monospace: bool,
    ) -> Option<PlatformFontMatch> {
        let policy =
            CapturedCharacterPolicy::from_rendering_spec(key, &query.inherited, style, spec);
        self.resolve_spec(policy, prefer_monospace, key.fontset_generation)
            .and_then(|resolution| self.finish_character_resolution(Some(resolution)))
    }
}

fn order_fallback_pairs(
    key: &crate::font::fallback_order::FallbackOrderKey,
    pairs: Vec<(FontSpecEntry, FontsetRenderingEntry)>,
) -> Vec<(FontSpecEntry, FontsetRenderingEntry)> {
    let specs = pairs.iter().map(|(spec, _)| spec.clone()).collect();
    let ordered = crate::font::fallback_order::order_fallback_group(key, specs);
    let mut remaining = pairs;
    let mut out = Vec::with_capacity(ordered.len());
    for spec in ordered {
        if let Some(index) = remaining.iter().position(|(entry, _)| entry == &spec) {
            out.push(remaining.remove(index));
        }
    }
    out
}

#[cfg(test)]
#[path = "tests/worker_policy_cache_budget_test.rs"]
mod cache_budget_tests;

#[cfg(test)]
#[path = "tests/worker_policy_shared_chain_test.rs"]
mod shared_chain_tests;

#[cfg(test)]
#[path = "tests/worker_policy_chain_test.rs"]
mod chain_tests;
