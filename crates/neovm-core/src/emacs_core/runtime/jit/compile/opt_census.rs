//! Immutable opt compiler census outside hot observation and runtime layouts.
//!
//! Threading: a process RwLock publishes fully initialized scalar metadata.
//! Keys are existing stable LeafObs allocation addresses used as identities;
//! pointers are never dereferenced. A fresh, empty RuntimeState ownership token
//! is retained through the leaf's existing feedback_holds allocation. It has
//! no Lisp values, source ids, cached Lisp state or emitted site pointers. The
//! registry owns only a Weak token, so leaf destruction needs no new hook.
//! Snapshot readers reject dead owners, and selected compiles sweep dead rows;
//! address reuse cannot expose the previous leaf's counters. Registry access
//! is confined to opt compilation, tests, cold selected policy and reports.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock, Weak};

use super::{CompiledLeaf, LeafObs, LeafTier, call_feedback};
use crate::emacs_core::jit::RuntimeState;
use crate::emacs_core::jit::opt::ir::OptCensus;

/// Report-only copies; all fields are immutable, owned compiler counters.
/// Threading: copies contain no mutator values or mutable runtime structures.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct OptStats {
    /// True even when this Opt plan has no enabled pass counters.
    /// Report-owned scalar only; never stored in CompiledLeaf or LeafObs.
    pub(crate) is_opt: bool,
    pub(crate) opt_fold: Option<Box<crate::emacs_core::jit::opt::passes::fold::FoldStats>>,
    pub(crate) opt_bool: Option<Box<crate::emacs_core::jit::opt::passes::bools::BoolStats>>,
    pub(crate) opt_reps: Option<Box<crate::emacs_core::jit::opt::ir::RepsCensus>>,
    pub(crate) opt_gvn: Option<Box<crate::emacs_core::jit::opt::passes::gvn::GvnStats>>,
    pub(crate) opt_range: Option<Box<crate::emacs_core::jit::opt::passes::range::RangeStats>>,
    pub(crate) opt_licm: Option<Box<crate::emacs_core::jit::opt::passes::licm::LicmStats>>,
    pub(crate) opt_sink: Option<Box<crate::emacs_core::jit::opt::sink_recipes::SinkStats>>,
    pub(crate) opt_arrays:
        Option<Box<crate::emacs_core::jit::opt::passes::array_reads::ArrayLiftStats>>,
}

impl OptStats {
    /// Exit rows retain main's physical LeafTier field and read this copied
    /// selected identity without another registry access.
    pub(crate) fn tier_name(&self, tier: LeafTier) -> &'static str {
        if self.is_opt { "opt" } else { tier.name() }
    }

    fn from_census(census: &OptCensus) -> Self {
        Self {
            is_opt: true,
            opt_fold: census.fold.clone().map(Box::new),
            opt_bool: census.bools.clone().map(Box::new),
            opt_reps: census.reps.clone().map(Box::new),
            opt_gvn: census.gvn.clone().map(Box::new),
            opt_range: census.range.clone().map(Box::new),
            opt_licm: census.licm.clone().map(Box::new),
            opt_sink: census.sink.clone().map(Box::new),
            opt_arrays: census.arrays.clone().map(Box::new),
        }
    }

    #[cfg(test)]
    fn is_empty(&self) -> bool {
        self.opt_fold.is_none()
            && self.opt_bool.is_none()
            && self.opt_reps.is_none()
            && self.opt_gvn.is_none()
            && self.opt_range.is_none()
            && self.opt_licm.is_none()
            && self.opt_sink.is_none()
            && self.opt_arrays.is_none()
    }
}

/// The registry never owns the leaf or its token. Concurrent compilers publish
/// under its write lock; reports clone immutable counts under the read lock.
struct RegistryEntry {
    owner: Weak<RuntimeState>,
    stats: OptStats,
}

type Registry = HashMap<usize, RegistryEntry>;
static REGISTRY: OnceLock<RwLock<Registry>> = OnceLock::new();

fn key(obs: &LeafObs) -> usize {
    std::ptr::from_ref(obs) as usize
}

/// Selected opt compiles alone reach this registration seam. Every Opt
/// plan, including one with empty pass counters, keeps its cold identity here.
/// The token never assigns a compiled id, so source/cache numbering is exact.
#[cold]
#[inline(never)]
pub(crate) fn attach(obs: &LeafObs, census: &OptCensus) {
    let stats = OptStats::from_census(census);
    let owner = Arc::new(RuntimeState::new());
    call_feedback::hold_for_leaf(&owner);
    let registry = REGISTRY.get_or_init(|| RwLock::new(HashMap::new()));
    let mut entries = registry
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    entries.retain(|_, entry| entry.owner.strong_count() != 0);
    entries.insert(
        key(obs),
        RegistryEntry {
            owner: Arc::downgrade(&owner),
            stats,
        },
    );
}

/// A report/test snapshot takes no runtime ownership and initializes nothing.
/// The containing live leaf keeps its unique token alive; a dead registration
/// is invisible even when the allocator has recycled the old observation key.
#[cfg(test)]
#[cold]
#[inline(never)]
pub(crate) fn snapshot(obs: &LeafObs) -> OptStats {
    let Some(registry) = REGISTRY.get() else {
        return OptStats::default();
    };
    let entries = registry
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    entries
        .get(&key(obs))
        .filter(|entry| entry.owner.strong_count() != 0)
        .map_or_else(OptStats::default, |entry| entry.stats.clone())
}

/// The selected code producer, copied only at cold compiler/report seams.
/// Threading: immutable diagnostic values; never stored in the main leaf,
/// observation, ABI, TLS, worker-payload or generated-code layouts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SelectedTier {
    Baseline,
    Mir,
    Aot,
    Opt,
}

impl SelectedTier {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Baseline => "baseline",
            Self::Mir => "mir",
            Self::Aot => "aot",
            Self::Opt => "opt",
        }
    }
}

/// Verify the exact token held by the actual leaf, rather than merely whether
/// the old registry token is alive. LeafObs can be freed before feedback_holds
/// drops; another mutator can then reuse its key while the old token is live.
/// Weak pointer identity remains unique until that Weak is dropped. This
/// helper takes no strong ownership and never dereferences an opaque key.
fn owns_marker(entry: &RegistryEntry, holds: &[Arc<RuntimeState>]) -> bool {
    holds
        .iter()
        .any(|held| Weak::ptr_eq(&entry.owner, &Arc::downgrade(held)))
}

/// Cold identity lookup verifies the actual leaf's ownership token. It
/// initializes nothing, clones no counters and takes no strong ownership.
#[cold]
#[inline(never)]
pub(crate) fn is_opt_leaf(leaf: &CompiledLeaf) -> bool {
    let Some(registry) = REGISTRY.get() else {
        return false;
    };
    let entries = registry
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    entries
        .get(&key(&leaf.obs))
        .is_some_and(|entry| owns_marker(entry, &leaf.feedback_holds))
}

/// Report-only copy authenticated against the same actual leaf. An old
/// address-matching registry row cannot leak identity or counters during drop.
#[cold]
#[inline(never)]
pub(crate) fn snapshot_leaf(leaf: &CompiledLeaf) -> OptStats {
    let Some(registry) = REGISTRY.get() else {
        return OptStats::default();
    };
    let entries = registry
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    entries
        .get(&key(&leaf.obs))
        .filter(|entry| owns_marker(entry, &leaf.feedback_holds))
        .map_or_else(OptStats::default, |entry| entry.stats.clone())
}

/// Observation-only fixture helper. Production callers must authenticate
/// through is_opt_leaf/snapshot_leaf with the actual leaf's existing holds.
#[cfg(test)]
fn is_opt(obs: &LeafObs) -> bool {
    let Some(registry) = REGISTRY.get() else {
        return false;
    };
    let entries = registry
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    entries
        .get(&key(obs))
        .is_some_and(|entry| entry.owner.strong_count() != 0)
}

impl CompiledLeaf {
    /// Cold compiler/report identity. Main's tier() accessor and its enum
    /// remain exact; Opt code uses the baseline physical handle and ABI.
    #[cold]
    #[inline(never)]
    pub(crate) fn selected_tier(&self) -> SelectedTier {
        match self.tier() {
            LeafTier::Baseline if is_opt_leaf(self) => SelectedTier::Opt,
            LeafTier::Baseline => SelectedTier::Baseline,
            LeafTier::Mir => SelectedTier::Mir,
            LeafTier::Aot => SelectedTier::Aot,
        }
    }
}

#[cfg(test)]
#[path = "tests/opt_census.rs"]
mod tests;
