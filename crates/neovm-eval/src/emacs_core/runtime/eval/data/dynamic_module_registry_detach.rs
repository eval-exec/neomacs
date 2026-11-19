//! Cold local receipt for the dynamic-module global-ref vector. The slot
//! followed its owner into eval. No production caller enters this detach.
//! The placeholder is an empty vector: a real [`GlobalRefEntry`] drop
//! invalidates a raw tag pointer, so tests push `None`.

use super::{GLOBAL_REFS, GlobalRefEntry};
use neovm_value::heap_registry::HeapRegistryHandle;
use neovm_value::heap_registry::detachment::{
    DetachedRegistry, RegistryDetachFailure, RegistryRestoreFailure,
};

type GlobalRefs = Vec<Option<GlobalRefEntry>>;

#[cold]
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "E2 L3 dormant cold detach; no production caller enters it"
    )
)]
pub(crate) fn detach_dynamic_module_registry(
    handle: &HeapRegistryHandle<GlobalRefs>,
) -> Result<DetachedRegistry<GlobalRefs>, RegistryDetachFailure<GlobalRefs>> {
    GLOBAL_REFS.with(|slot| {
        // SAFETY: the caller excluded every other use of this thread-local
        // slot for the receipt's lifetime. The receipt is a local effect. It
        // is not World authority and it does not resume a source.
        unsafe { slot.detach(handle, Vec::new()) }
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
pub(crate) fn restore_dynamic_module_registry(
    receipt: DetachedRegistry<GlobalRefs>,
) -> Result<(), RegistryRestoreFailure<GlobalRefs>> {
    GLOBAL_REFS.with(|slot| receipt.restore(slot))
}

#[cfg(test)]
mod tests {
    use super::super::GLOBAL_REFS;
    use super::*;
    use neovm_value::heap_registry::HeapRegistrySlot;
    use neovm_value::heap_registry::detachment::RegistryDetachError;

    fn current() -> HeapRegistryHandle<GlobalRefs> {
        GLOBAL_REFS.with(HeapRegistrySlot::current)
    }

    fn same(left: &HeapRegistryHandle<GlobalRefs>, right: &HeapRegistryHandle<GlobalRefs>) -> bool {
        HeapRegistryHandle::registry_ptr_eq_for_test(left, right)
    }

    #[test]
    fn pentry_e2b_registry_adapter_dynamic_module_roundtrip() {
        let original = current();
        original.borrow_mut().push(None);
        let receipt = detach_dynamic_module_registry(&original).expect("actual slot detach");
        let parked = current();
        assert!(!same(&original, &parked));
        assert_eq!(parked.heap_identity(), 0);
        assert!(parked.borrow().is_empty());
        assert_eq!(original.borrow().len(), 1);
        restore_dynamic_module_registry(receipt).expect("matched original slot restore");
        assert!(same(&original, &current()));
        assert_eq!(current().borrow().len(), 1);
        current().borrow_mut().clear();
    }

    #[test]
    fn pentry_e2b_registry_adapter_dynamic_module_borrow_refusal() {
        let original = current();
        let view = original.borrow_mut();
        let failure = detach_dynamic_module_registry(&original).expect_err("live original view");
        assert_eq!(failure.reason(), RegistryDetachError::RegistryBorrowed);
        drop(view);
        assert!(same(&original, &current()));
    }

    #[test]
    fn pentry_e2b_registry_adapter_dynamic_module_foreign_preserved() {
        let installed = current();
        let foreign_slot = HeapRegistrySlot::new(Vec::new());
        let foreign = foreign_slot.current();
        let receipt =
            detach_dynamic_module_registry(&foreign).expect("foreign is already inactive");
        assert!(same(&installed, &current()));
        restore_dynamic_module_registry(receipt).expect("inactive receipt installs nothing");
        assert!(same(&installed, &current()));
    }

    #[test]
    fn pentry_e2b_registry_adapter_dynamic_module_displaced_restore() {
        let original = current();
        let receipt = detach_dynamic_module_registry(&original).expect("original detach");
        let newer = HeapRegistryHandle::new(Vec::new());
        GLOBAL_REFS.with(|slot| slot.install(&newer));
        let failure =
            restore_dynamic_module_registry(receipt).expect_err("newer installation preserved");
        assert_eq!(failure.reason(), RegistryDetachError::Displaced);
        let (_, owned) = failure.into_parts();
        drop(owned);
        assert!(same(&newer, &current()));
    }

    #[test]
    fn pentry_e2b_registry_adapter_dynamic_module_drop_never_reinstalls() {
        let original = current();
        let receipt = detach_dynamic_module_registry(&original).expect("original detach");
        let parked = current();
        drop(receipt);
        assert!(same(&parked, &current()));
        assert!(!same(&original, &current()));
    }
}
