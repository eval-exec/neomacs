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
//! is confined to opt compilation, tests and cold exit-report collection.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock, Weak};

use super::{LeafObs, call_feedback};
use crate::emacs_core::jit::RuntimeState;
use crate::emacs_core::jit::opt::ir::OptCensus;

/// Report-only copies; all fields are immutable, owned compiler counters.
/// Threading: copies contain no mutator values or mutable runtime structures.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct OptStats {
    pub(crate) opt_fold: Option<Box<crate::emacs_core::jit::opt::passes::fold::FoldStats>>,
    pub(crate) opt_bool: Option<Box<crate::emacs_core::jit::opt::passes::bools::BoolStats>>,
    pub(crate) opt_reps: Option<Box<crate::emacs_core::jit::opt::ir::RepsCensus>>,
    pub(crate) opt_gvn: Option<Box<crate::emacs_core::jit::opt::passes::gvn::GvnStats>>,
}

impl OptStats {
    fn from_census(census: &OptCensus) -> Self {
        Self {
            opt_fold: census.fold.clone().map(Box::new),
            opt_bool: census.bools.clone().map(Box::new),
            opt_reps: census.reps.clone().map(Box::new),
            opt_gvn: census.gvn.clone().map(Box::new),
        }
    }

    fn is_empty(&self) -> bool {
        self.opt_fold.is_none()
            && self.opt_bool.is_none()
            && self.opt_reps.is_none()
            && self.opt_gvn.is_none()
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

/// Selected opt compiles alone reach this registration seam. Empty census
/// creates no registry, ownership token or additional existing leaf hold.
/// The token never assigns a compiled id, so source/cache numbering is exact.
#[cold]
#[inline(never)]
pub(crate) fn attach(obs: &LeafObs, census: &OptCensus) {
    let stats = OptStats::from_census(census);
    if stats.is_empty() {
        return;
    }
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

#[cfg(test)]
#[path = "tests/opt_census.rs"]
mod tests;
