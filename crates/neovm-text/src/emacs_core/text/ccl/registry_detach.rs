//! Cold local receipt for the CCL registry slot. No production caller enters
//! this detach. Native registration stays in the parent module.

use super::{CCL_REGISTRY, CclRegistry};
use neovm_value::heap_registry::HeapRegistryHandle;
use neovm_value::heap_registry::detachment::{
    DetachedRegistry, RegistryDetachFailure, RegistryRestoreFailure,
};

#[cold]
#[doc(hidden)]
pub fn detach_ccl_registry(
    handle: &HeapRegistryHandle<CclRegistry>,
) -> Result<DetachedRegistry<CclRegistry>, RegistryDetachFailure<CclRegistry>> {
    CCL_REGISTRY.with(|slot| {
        // SAFETY: the caller excluded every other use of this thread-local
        // slot for the receipt's lifetime. The receipt is a local effect. It
        // is not World authority and it does not resume a source.
        unsafe { slot.detach(handle, CclRegistry::with_defaults()) }
    })
}

#[cold]
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "E2 L3 dormant cold detach; no production caller enters it"
    )
)]
pub(crate) fn restore_ccl_registry(
    receipt: DetachedRegistry<CclRegistry>,
) -> Result<(), RegistryRestoreFailure<CclRegistry>> {
    CCL_REGISTRY.with(|slot| receipt.restore(slot))
}

#[cfg(test)]
mod tests {
    use super::super::{CCL_REGISTRY, CclRegistry};
    use super::*;
    use neovm_value::heap_registry::detachment::RegistryDetachError;
    use neovm_value::heap_registry::{HeapRegistryHandle, HeapRegistrySlot};

    fn current() -> HeapRegistryHandle<CclRegistry> {
        CCL_REGISTRY.with(HeapRegistrySlot::current)
    }

    fn same(
        left: &HeapRegistryHandle<CclRegistry>,
        right: &HeapRegistryHandle<CclRegistry>,
    ) -> bool {
        HeapRegistryHandle::registry_ptr_eq_for_test(left, right)
    }

    #[test]
    fn pentry_e2b_registry_adapter_ccl_roundtrip() {
        let original = current();
        original.borrow_mut().next_program_id = 99;
        let receipt = detach_ccl_registry(&original).expect("actual slot detach");
        let parked = current();
        assert!(!same(&original, &parked));
        assert_eq!(parked.heap_identity(), 0);
        assert_eq!(parked.borrow().next_program_id, 1);
        assert_eq!(original.borrow().next_program_id, 99);
        restore_ccl_registry(receipt).expect("matched original slot restore");
        assert!(same(&original, &current()));
        assert_eq!(current().borrow().next_program_id, 99);
    }

    #[test]
    fn pentry_e2b_registry_adapter_ccl_borrow_refusal() {
        let original = current();
        let view = original.borrow_mut();
        let failure = detach_ccl_registry(&original).expect_err("live original view");
        assert_eq!(failure.reason(), RegistryDetachError::RegistryBorrowed);
        drop(view);
        assert!(same(&original, &current()));
    }

    #[test]
    fn pentry_e2b_registry_adapter_ccl_foreign_preserved() {
        let installed = current();
        let foreign_slot = HeapRegistrySlot::new(CclRegistry::with_defaults());
        let foreign = foreign_slot.current();
        let receipt = detach_ccl_registry(&foreign).expect("foreign is already inactive");
        assert!(same(&installed, &current()));
        restore_ccl_registry(receipt).expect("inactive receipt installs nothing");
        assert!(same(&installed, &current()));
    }

    #[test]
    fn pentry_e2b_registry_adapter_ccl_displaced_restore() {
        let original = current();
        let receipt = detach_ccl_registry(&original).expect("original detach");
        let newer = HeapRegistryHandle::new(CclRegistry::with_defaults());
        CCL_REGISTRY.with(|slot| slot.install(&newer));
        let failure = restore_ccl_registry(receipt).expect_err("newer installation preserved");
        assert_eq!(failure.reason(), RegistryDetachError::Displaced);
        let (_, owned) = failure.into_parts();
        drop(owned);
        assert!(same(&newer, &current()));
    }

    #[test]
    fn pentry_e2b_registry_adapter_ccl_drop_never_reinstalls() {
        let original = current();
        let receipt = detach_ccl_registry(&original).expect("original detach");
        let parked = current();
        drop(receipt);
        assert!(same(&parked, &current()));
        assert!(!same(&original, &current()));
    }
}
