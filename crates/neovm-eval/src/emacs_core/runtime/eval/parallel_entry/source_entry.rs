//! Bootstrap source-entry observations, not an access or stop capability.
//! The result cannot certify later TLS state. It never clears a scoped
//! pointer or a root, and it returns no mutation or permission.
//!
//! The module-scope probe is a process-wide function pointer. An unregistered
//! probe reads Inactive, so an eval-only test matches a binary that has not
//! installed the system observer. A future live caller in a binary that links
//! neovm-system must install that probe; this module does not.

use super::super::{Context, SCRATCH_GC_ROOTS};
use std::sync::atomic::{AtomicPtr, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SourceEntryRefusal {
    HeapTlsNotCurrent,
    GcInhibited,
    CollectionDriver,
    CallbackAccounting,
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
    Inactive,
    Active,
    Borrowed,
}

static SOURCE_MODULE_PROBE: AtomicPtr<()> = AtomicPtr::new(std::ptr::null_mut());

/// Install the process-wide module-scope probe.
///
/// # Safety
///
/// `probe` only reads the existing module thread-locals. It remains valid for
/// the process. The store is not a TLS slot and it names no World. Tests in
/// one process must not install differing probes.
#[doc(hidden)]
pub unsafe fn install_source_module_scope_probe(probe: fn() -> SourceModuleObservation) {
    SOURCE_MODULE_PROBE.store(probe as *mut (), Ordering::Release);
}

#[doc(hidden)]
pub fn source_module_observation() -> SourceModuleObservation {
    let pointer = SOURCE_MODULE_PROBE.load(Ordering::Acquire);
    if pointer.is_null() {
        return SourceModuleObservation::Inactive;
    }
    // SAFETY: `install_source_module_scope_probe` stored a function pointer of
    // this exact signature, and that function remains valid for the process.
    // The transmute runs only on the load side. No TLS slot is created.
    let probe = unsafe { std::mem::transmute::<*mut (), fn() -> SourceModuleObservation>(pointer) };
    probe()
}

pub(crate) fn module_refusal(
    observation: SourceModuleObservation,
) -> Result<(), SourceEntryRefusal> {
    match observation {
        SourceModuleObservation::Inactive => Ok(()),
        SourceModuleObservation::Active => Err(SourceEntryRefusal::ModuleScope),
        SourceModuleObservation::Borrowed => Err(SourceEntryRefusal::ModuleSlotBorrowed),
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
