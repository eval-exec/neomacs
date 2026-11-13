#![cfg(any(test, feature = "test-support"))]
//! Signed S4-S6 error test support; normal product builds contain none of it.
//!
//! The existence gate is the signed R4 test-support gate; general and fenced
//! re-exports are installed by their respective surface owners.

use super::{
    EvalError, InFlightPinned, InFlightRegistryHandle, InFlightRoots,
    current_in_flight_registry_handle,
};
use std::collections::BTreeSet;

/// Owned copies of count facts, never fields borrowed from the table.
#[derive(Debug)]
pub struct RegistryState {
    pub live: usize,
    pub total: usize,
    pub free: usize,
}

// One canonical copy of the former flow_word_test registry_state oracle.
#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(super) fn registry_state(registry: &InFlightRegistryHandle) -> RegistryState {
    let table = registry.lock();
    let free: BTreeSet<usize> = table.free.iter().copied().collect();
    assert_eq!(free.len(), table.free.len(), "a slot was released twice");
    for &slot in &free {
        assert!(slot < table.slots.len(), "free slot outside its registry");
        assert!(table.slots[slot].is_none(), "live slot on the free list");
    }
    let live = table.slots.iter().filter(|slot| slot.is_some()).count();
    assert_eq!(
        live + free.len(),
        table.slots.len(),
        "an unowned slot leaked"
    );
    RegistryState {
        live,
        total: table.slots.len(),
        free: free.len(),
    }
}

/// A bounded retained read-only view of one existing registry.
///
/// The handle stays private; no guard, table, pin or mutable projection escapes.
pub(crate) struct InFlightRegistryObserver {
    registry: InFlightRegistryHandle,
}

impl InFlightRegistryObserver {
    pub fn registry_state_for_test(&self) -> RegistryState {
        registry_state(&self.registry)
    }

    pub fn free_list_for_test(&self) -> Vec<usize> {
        let table = self.registry.lock();
        table.free.iter().copied().collect()
    }

    pub fn same_registry(&self, other: &InFlightRegistryObserver) -> bool {
        std::sync::Arc::ptr_eq(&self.registry.table, &other.registry.table)
    }
}

/// Preserve the canonical getter's lazy installation at the caller's old point.
pub(crate) fn current_in_flight_registry_observer_for_test() -> InFlightRegistryObserver {
    InFlightRegistryObserver {
        registry: current_in_flight_registry_handle(),
    }
}

/// Identifies an existing pinned slot; the originating error stays live until
/// the worker acknowledges that it claimed its own local pin from this slot.
#[derive(Clone, Copy)]
pub struct InFlightSlotId(usize);

pub fn existing_pin_registry_slot_for_test(
    error: &EvalError,
) -> (InFlightRegistryHandle, InFlightSlotId) {
    let EvalError::Signal { pin, .. } = error else {
        panic!("expected a pinned signal");
    };
    let pin = pin
        .in_flight_roots()
        .pin
        .as_ref()
        .expect("signal has traceable payload");
    (pin.registry.clone(), InFlightSlotId(pin.slot))
}

pub fn claim_existing_registry_slot_for_test(
    registry: InFlightRegistryHandle,
    slot: InFlightSlotId,
) -> InFlightRoots {
    // The source error remains alive until the receiver claims this pin.
    // Copy only the mutex-protected root words; no Lisp object is dereferenced.
    let values = registry.lock().slots[slot.0]
        .as_ref()
        .expect("acknowledgement keeps the source slot live")
        .clone();
    InFlightRoots::claim(registry, values)
}
