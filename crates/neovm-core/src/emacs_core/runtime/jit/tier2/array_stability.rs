//! Selected array stability outside main's Version/PolicyState/T2Cells layout.
//!
//! Threading: a leaf and its policy belong to one mutator, as on main. Its
//! stable boxed LeafObs address is an opaque registry key, never dereferenced.
//! Different mutators/leaves use disjoint rows under a process RwLock. A fresh
//! empty RuntimeState token is held by the leaf's existing feedback_holds;
//! only its Weak enters this registry, so address reuse cannot expose a dead
//! leaf's version. Attach sweeps dead rows. No new destructor or hot field is
//! added, and no Lisp state, source id or object pointer is retained.
//!
//! Sources already publish their atomic scalar kind tables separately. This
//! module copies masks at cold policy boundaries. Relaxed atomic snapshots
//! are monotone hints; actual Opt array shape/bounds proof guards still run.
//! No registry access is emitted in generated code or performed by workers.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock, Weak};

use super::super::compile::{LeafObs, call_feedback};
use crate::emacs_core::jit::RuntimeState;
use crate::emacs_core::jit::feedback::arrays::ArrayKindMask;

struct Entry {
    owner: Weak<RuntimeState>,
    masks: Option<Vec<ArrayKindMask>>,
}
type Registry = HashMap<usize, Entry>;
static REGISTRY: OnceLock<RwLock<Registry>> = OnceLock::new();

/// One cold request's copied version. Threading: invocation-owned scalar
/// masks, with no runtime object ownership or pointer. Main's single attempts
/// budget controls combined source and array instability.
pub(super) struct ArrayVersion {
    pub(super) changed: bool,
    masks: Vec<ArrayKindMask>,
}

fn key(obs: &LeafObs) -> usize {
    std::ptr::from_ref(obs) as usize
}

fn selected() -> bool {
    super::super::compile::array_snapshot::selected()
}

/// Called only during a selected profiling T1 front build with an Aref. The
/// ordinary census registry cannot supply ownership: profiling T1 has no Opt
/// census, yet its array stability must remain live through fallback/retry.
/// The token never assigns an id and contains no Lisp roots or emitted sites.
#[cold]
#[inline(never)]
pub(crate) fn attach(obs: &LeafObs) {
    if !selected() || !obs.t2.profiling() {
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
        Entry {
            owner: Arc::downgrade(&owner),
            masks: None,
        },
    );
}

/// Compare without publishing: an exhausted instability-attempt budget must
/// retain the old version, just like main's numeric/call Version. OFF returns
/// before source/registry reads and initializes no state.
pub(super) fn read(obs: &LeafObs, source: &RuntimeState, ops_len: usize) -> Option<ArrayVersion> {
    if !selected() {
        return None;
    }
    let registry = REGISTRY.get()?;
    let masks = source.array_kind_snapshot(ops_len);
    let entries = registry
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let entry = entries
        .get(&key(obs))
        .filter(|entry| entry.owner.strong_count() != 0)?;
    Some(ArrayVersion {
        changed: entry.masks.as_ref() != Some(&masks),
        masks,
    })
}

pub(super) fn publish(obs: &LeafObs, version: ArrayVersion) {
    let Some(registry) = REGISTRY.get() else {
        return;
    };
    let mut entries = registry
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(entry) = entries
        .get_mut(&key(obs))
        .filter(|entry| entry.owner.strong_count() != 0)
    {
        entry.masks = Some(version.masks);
    }
}

/// Rearm/fresh BudgetWait reopen records the current masks at exactly the
/// same cut as main's retained numeric/call version; no stale pause snapshot
/// can authorize an upgrade before a full newly sampled stable window.
pub(super) fn reset(obs: &LeafObs, source: &RuntimeState, ops_len: usize) {
    if let Some(version) = read(obs, source, ops_len) {
        publish(obs, version);
    }
}

#[cfg(test)]
#[path = "tests/array_stability_test.rs"]
mod tests;
