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
    ) -> Option<PlatformFontMatch> {
        let state = self.worker_policy.as_ref()?;
        key.fontset_generation = state.policies.generation.get();
        let Some(query) = state.policies.queries.get(&key) else {
            state.missing.store(true, Ordering::Relaxed);
            return None;
        };
        if let Ok(cache) = self.char_cache.lock()
            && let Some(cached) = cache.get(&key)
        {
            return cached.clone();
        }
        let style = WorkerFontStyle::from_key(&key)?;
        let selected = self
            .resolve_frozen_query(&key, query, &style)
            .and_then(|matched| self.backend.finalize_match(matched))
            .map(|matched| self.with_native_metrics(matched));
        if let Ok(mut cache) = self.char_cache.lock() {
            cache.insert(key, selected.clone());
        }
        selected
    }

    fn resolve_frozen_query(
        &self,
        key: &CharCacheKey,
        query: &FrozenCharacterQuery,
        style: &WorkerFontStyle,
    ) -> Option<PlatformFontCandidate> {
        let prefer_monospace = self.family_prefers_monospace(&key.family);
        for entry in query.chain.iter() {
            match entry {
                FontsetRenderingEntry::ExplicitNone => return None,
                FontsetRenderingEntry::Font(spec) => {
                    let policy = CapturedCharacterPolicy::from_rendering_spec(
                        key,
                        &query.inherited,
                        style,
                        spec,
                    );
                    if let Some(matched) = self.resolve_from_policy(&policy, prefer_monospace) {
                        return Some(matched);
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
        };
        let policy =
            CapturedCharacterPolicy::from_rendering_spec(key, &query.inherited, style, &fallback);
        self.resolve_from_policy(&policy, prefer_monospace)
    }
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
