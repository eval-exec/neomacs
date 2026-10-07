//! Owned, bounded fontset policy for native work outside the evaluator.
use super::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FrozenCharacterPolicies {
    pub(super) generation: u64,
    queries: HashMap<CharCacheKey, Vec<CapturedCharacterPolicy>>,
    bytes: usize,
}

pub(super) struct WorkerPolicy {
    pub policies: Arc<FrozenCharacterPolicies>,
    missing: AtomicBool,
    pub(super) admitted_queries: HashSet<CharCacheKey>,
}

impl FrozenCharacterPolicies {
    pub(crate) fn capture(
        requests: &[(&str, char, u16, bool, FontSelectionSize)],
        max_bytes: usize,
    ) -> Option<Self> {
        let generation = fontset_generation();
        let mut snapshot = Self {
            generation,
            queries: HashMap::default(),
            bytes: std::mem::size_of::<Self>(),
        };
        for &(family, ch, weight, italic, size) in requests {
            if ch.is_ascii() {
                continue;
            }
            if family.len() > 512 || snapshot.queries.len() >= 128 {
                return None;
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
                fontset_generation: generation,
                size,
            };
            if snapshot.queries.contains_key(&key) {
                continue;
            }
            let (revision, entries) =
                neovm_core::emacs_core::fontset::bounded_entries_for_char(ch, 32, 128)?;
            if revision != generation {
                return None;
            }
            let mut alternatives = Vec::new();
            let mut fallback = true;
            for entry in entries {
                match entry {
                    FontSpecEntry::ExplicitNone => {
                        fallback = false;
                        break;
                    }
                    FontSpecEntry::Font(spec) => {
                        alternatives.push(CapturedCharacterPolicy::capture_bounded(
                            family, ch, weight, slant, size, &spec,
                        )?)
                    }
                }
            }
            if fallback {
                alternatives.push(CapturedCharacterPolicy::capture_bounded(
                    family,
                    ch,
                    weight,
                    slant,
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
                )?);
            }
            snapshot.bytes = snapshot
                .bytes
                .checked_add(std::mem::size_of::<CharCacheKey>() + family.len())?;
            for alternative in &alternatives {
                snapshot.bytes = snapshot.bytes.checked_add(alternative.owned_bytes())?;
            }
            if snapshot.bytes > max_bytes {
                return None;
            }
            snapshot.queries.insert(key, alternatives);
        }
        (fontset_generation() == generation && snapshot.bytes <= max_bytes).then_some(snapshot)
    }

    pub(crate) fn bytes(&self) -> usize {
        self.bytes
    }
    pub(crate) fn query_count(&self) -> usize {
        self.queries.len()
    }
}

impl CapturedCharacterPolicy {
    fn capture_bounded(
        family: &str,
        ch: char,
        weight: u16,
        slant: FontSlant,
        size: FontSelectionSize,
        spec: &StoredFontSpec,
    ) -> Option<Self> {
        use neovm_core::emacs_core::intern::resolve_sym;
        for symbol in [spec.family, spec.registry, spec.lang]
            .into_iter()
            .flatten()
        {
            if resolve_sym(symbol).len() > 512 {
                return None;
            }
        }
        let families = match spec.family {
            Some(symbol) => CapturedFontFamilyPolicy::Explicit(resolve_sym(symbol).to_owned()),
            None => CapturedFontFamilyPolicy::Inherited(
                neovm_core::emacs_core::font::bounded_alternative_font_families(family, 32, 2048)?,
            ),
        };
        let constraints = GnuFontPolicy::constraints_for_character(spec, ch);
        Some(Self {
            requested_family: family.to_owned(),
            ch,
            families,
            charset_ranges: constraints.coverage().ranges().to_vec(),
            languages: constraints
                .languages()
                .iter()
                .map(|lang| lang.as_str().to_owned())
                .collect(),
            weight: spec
                .weight
                .map(|weight| weight.css_weight())
                .unwrap_or(weight),
            slant: spec.slant.unwrap_or(slant),
            width: spec.width.unwrap_or(FontWidth::Normal),
            size,
        })
    }
    fn owned_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + self.requested_family.len()
            + self.charset_ranges.len() * std::mem::size_of::<(u32, u32)>()
            + self
                .languages
                .iter()
                .map(|s| std::mem::size_of::<String>() + s.len())
                .sum::<usize>()
            + match &self.families {
                CapturedFontFamilyPolicy::Explicit(name) => name.len(),
                CapturedFontFamilyPolicy::Inherited(names) => names
                    .iter()
                    .map(|s| std::mem::size_of::<String>() + s.len())
                    .sum(),
            }
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
        key.fontset_generation = state.policies.generation;
        let Some(alternatives) = state.policies.queries.get(&key) else {
            state.missing.store(true, Ordering::Relaxed);
            return None;
        };
        if let Ok(cache) = self.char_cache.lock()
            && let Some(cached) = cache.get(&key)
        {
            return cached.clone();
        }
        let prefer_monospace = self.family_prefers_monospace(&key.family);
        let selected = alternatives
            .iter()
            .find_map(|policy| self.resolve_from_policy(policy, prefer_monospace))
            .and_then(|matched| self.backend.finalize_match(matched))
            .map(|matched| self.with_native_metrics(matched));
        if let Ok(mut cache) = self.char_cache.lock() {
            cache.insert(key, selected.clone());
        }
        selected
    }
}

#[cfg(test)]
#[path = "tests/worker_policy_cache_budget_test.rs"]
mod cache_budget_tests;
