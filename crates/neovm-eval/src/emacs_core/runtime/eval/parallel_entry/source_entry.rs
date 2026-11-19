//! Bootstrap source-entry observations, not an access or stop capability.
//! The result cannot certify later TLS state. It never clears a scoped
//! pointer or a root, and it returns no mutation or permission.
//!
//! The module-scope probe is one process-wide function, published once
//! through `neovm_value::hook_publication`. An unregistered probe is
//! `Unregistered` and refuses. The same function published again is
//! `AlreadyInstalled`. A different function is `HookInstallError::DifferentTable`.
//! This module does not install the system observer. Core's eval-hook boot
//! does, beside the sqlite finalizer hook.

use super::super::{Context, SCRATCH_GC_ROOTS};
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SourceEntryRefusal {
    HeapTlsNotCurrent,
    GcInhibited,
    CollectionDriver,
    CallbackAccounting,
    ModuleProbeUnregistered,
    ModuleScope,
    ModuleSlotBorrowed,
    CclVariableScope,
    ScratchRoots,
    ScratchSlotBorrowed,
    ExistingGreenRuntime,
    GreenSlotBorrowed,
    FunctionActivation,
    PoisonedHeap,
    GenerationalRequired,
}

/// Copy of the system module-scope observation. This is data, not a
/// capability: every variant is constructible and the type is `Copy`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[doc(hidden)]
pub enum SourceModuleObservation {
    Unregistered,
    Inactive,
    Active,
    Borrowed,
}

static SOURCE_MODULE_PROBE: OnceLock<fn() -> SourceModuleObservation> = OnceLock::new();
static SOURCE_MODULE_PROBE_IDENTITY: OnceLock<&'static fn() -> SourceModuleObservation> =
    OnceLock::new();

/// Publish the process-wide module-scope probe.
///
/// Identity is the static address of `probe`, the same rule as the other
/// one-time hooks. The cell holds the function itself. There is no transmute
/// and no new thread-local.
#[doc(hidden)]
#[must_use = "handle a conflicting source-module probe publication"]
pub fn install_source_module_scope_probe(
    probe: &'static fn() -> SourceModuleObservation,
) -> Result<neovm_value::HookInstallStatus, neovm_value::HookInstallError> {
    neovm_value::publish_hook_table(&SOURCE_MODULE_PROBE, &SOURCE_MODULE_PROBE_IDENTITY, probe)
}

fn observation_from_installed(
    installed: Option<&fn() -> SourceModuleObservation>,
) -> SourceModuleObservation {
    match installed {
        Some(probe) => probe(),
        None => SourceModuleObservation::Unregistered,
    }
}

#[doc(hidden)]
pub fn source_module_observation() -> SourceModuleObservation {
    observation_from_installed(SOURCE_MODULE_PROBE.get())
}

pub(crate) fn module_refusal(
    observation: SourceModuleObservation,
) -> Result<(), SourceEntryRefusal> {
    match observation {
        SourceModuleObservation::Unregistered => Err(SourceEntryRefusal::ModuleProbeUnregistered),
        SourceModuleObservation::Inactive => Ok(()),
        SourceModuleObservation::Active => Err(SourceEntryRefusal::ModuleScope),
        SourceModuleObservation::Borrowed => Err(SourceEntryRefusal::ModuleSlotBorrowed),
    }
}

/// Eval-only tests have no core boot, so they publish this Inactive probe
/// themselves. The system observer is a different function and must not be
/// installed from this crate.
#[cfg(test)]
fn inactive_source_module_probe_for_test() -> SourceModuleObservation {
    SourceModuleObservation::Inactive
}

#[cfg(test)]
pub(crate) static INACTIVE_SOURCE_MODULE_PROBE_FOR_TEST: fn() -> SourceModuleObservation =
    inactive_source_module_probe_for_test;

#[cfg(test)]
pub(crate) fn install_inactive_source_module_probe_for_test() {
    match install_source_module_scope_probe(&INACTIVE_SOURCE_MODULE_PROBE_FOR_TEST) {
        Ok(
            neovm_value::HookInstallStatus::Installed
            | neovm_value::HookInstallStatus::AlreadyInstalled,
        ) => {}
        Err(error) => panic!("inactive source-module test probe refused: {error:?}"),
    }
}

/// No mutation or permission is returned. GEN0 is a typed bootstrap refusal.
/// The future thread-start dispatcher must choose its functional Baton-serial
/// route instead. This is not a user-visible failure of an opt-in request.
#[cold]
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "E2 L3 dormant source-entry observer; no production caller enters it"
    )
)]
pub(crate) fn validate_serial_source_entry(source: &Context) -> Result<(), SourceEntryRefusal> {
    if source.gc_inhibit_depth != 0 {
        return Err(SourceEntryRefusal::GcInhibited);
    }
    if source.gc_driver_active {
        return Err(SourceEntryRefusal::CollectionDriver);
    }
    if source.user_test_gc_accounting_pointer().is_some() {
        return Err(SourceEntryRefusal::CallbackAccounting);
    }
    module_refusal(source_module_observation())?;
    match neovm_text::emacs_core::ccl::source_scope::source_ccl_scope() {
        neovm_text::emacs_core::ccl::source_scope::SourceCclScope::Inactive => {}
        neovm_text::emacs_core::ccl::source_scope::SourceCclScope::Active => {
            return Err(SourceEntryRefusal::CclVariableScope);
        }
    }
    // Refuse ANY outstanding slot loan, even a reader of an empty vector.
    // Stable scratch indices must never be truncated to force eligibility.
    SCRATCH_GC_ROOTS.with(|slot| {
        let roots = slot
            .try_borrow_mut()
            .map_err(|_| SourceEntryRefusal::ScratchSlotBorrowed)?;
        if roots.is_empty() {
            Ok(())
        } else {
            Err(SourceEntryRefusal::ScratchRoots)
        }
    })?;
    source.validate_serial_source_world_entry()?;
    if !neovm_value::tagged::gc::tagged_heap_is_current(&source.tagged_heap) {
        return Err(SourceEntryRefusal::HeapTlsNotCurrent);
    }
    if source.tagged_heap.gc_locks_poisoned() {
        return Err(SourceEntryRefusal::PoisonedHeap);
    }
    if !source.tagged_heap.tagged_heap_generational_enabled() {
        return Err(SourceEntryRefusal::GenerationalRequired);
    }
    Ok(())
}

#[cfg(test)]
#[path = "tests/source_entry_test.rs"]
mod tests;
