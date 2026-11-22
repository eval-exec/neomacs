//! Cold local receipt for the charset registry slot. The placeholder is the
//! empty registry, not [`CharsetRegistry::new`]. No production caller enters
//! this detach.

use super::{CHARSET_REGISTRY, CharsetRegistry};
use neovm_value::heap_registry::HeapRegistryHandle;
use neovm_value::heap_registry::detachment::{
    DetachedRegistry, RegistryDetachFailure, RegistryRestoreFailure,
};

#[cold]
#[doc(hidden)]
pub fn detach_charset_registry(
    handle: &HeapRegistryHandle<CharsetRegistry>,
) -> Result<DetachedRegistry<CharsetRegistry>, RegistryDetachFailure<CharsetRegistry>> {
    CHARSET_REGISTRY.with(|slot| {
        // SAFETY: the caller excluded every other use of this thread-local
        // slot for the receipt's lifetime. The receipt is a local effect. It
        // is not World authority and it does not resume a source.
        unsafe { slot.detach(handle, CharsetRegistry::empty()) }
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
pub(crate) fn restore_charset_registry(
    receipt: DetachedRegistry<CharsetRegistry>,
) -> Result<(), RegistryRestoreFailure<CharsetRegistry>> {
    CHARSET_REGISTRY.with(|slot| receipt.restore(slot))
}

#[cfg(test)]
mod tests {
    use super::super::{CHARSET_REGISTRY, CharsetRegistry};
    use super::*;
    use neovm_value::heap_registry::detachment::RegistryDetachError;
    use neovm_value::heap_registry::{HeapRegistryHandle, HeapRegistrySlot};

    fn current() -> HeapRegistryHandle<CharsetRegistry> {
        crate::test_utils::init_test_tracing();
        CHARSET_REGISTRY.with(HeapRegistrySlot::current)
    }

    fn same(
        left: &HeapRegistryHandle<CharsetRegistry>,
        right: &HeapRegistryHandle<CharsetRegistry>,
    ) -> bool {
        HeapRegistryHandle::registry_ptr_eq_for_test(left, right)
    }

    #[test]
    fn pentry_e2b_registry_adapter_charset_roundtrip() {
        let original = current();
        assert!(
            !original.borrow().charsets.is_empty(),
            "installed registry keeps the standard charsets"
        );
        let receipt = detach_charset_registry(&original).expect("actual slot detach");
        let parked = current();
        assert!(!same(&original, &parked));
        assert_eq!(parked.heap_identity(), 0);
        assert!(
            parked.borrow().charsets.is_empty(),
            "placeholder is empty, not CharsetRegistry::new"
        );
        assert!(!original.borrow().charsets.is_empty());
        restore_charset_registry(receipt).expect("matched original slot restore");
        assert!(same(&original, &current()));
        assert!(!current().borrow().charsets.is_empty());
    }

    #[test]
    fn pentry_e2b_registry_adapter_charset_borrow_refusal() {
        let original = current();
        let view = original.borrow_mut();
        let failure = detach_charset_registry(&original).expect_err("live original view");
        assert_eq!(failure.reason(), RegistryDetachError::RegistryBorrowed);
        drop(view);
        assert!(same(&original, &current()));
    }

    #[test]
    fn pentry_e2b_registry_adapter_charset_foreign_preserved() {
        let installed = current();
        let foreign_slot = HeapRegistrySlot::new(CharsetRegistry::empty());
        let foreign = foreign_slot.current();
        let receipt = detach_charset_registry(&foreign).expect("foreign is already inactive");
        assert!(same(&installed, &current()));
        restore_charset_registry(receipt).expect("inactive receipt installs nothing");
        assert!(same(&installed, &current()));
    }

    #[test]
    fn pentry_e2b_registry_adapter_charset_displaced_restore() {
        let original = current();
        let receipt = detach_charset_registry(&original).expect("original detach");
        let newer = HeapRegistryHandle::new(CharsetRegistry::new());
        CHARSET_REGISTRY.with(|slot| slot.install(&newer));
        let failure = restore_charset_registry(receipt).expect_err("newer installation preserved");
        assert_eq!(failure.reason(), RegistryDetachError::Displaced);
        let (_, owned) = failure.into_parts();
        drop(owned);
        assert!(same(&newer, &current()));
    }

    #[test]
    fn pentry_e2b_registry_adapter_charset_drop_never_reinstalls() {
        let original = current();
        let receipt = detach_charset_registry(&original).expect("original detach");
        let parked = current();
        drop(receipt);
        assert!(same(&parked, &current()));
        assert!(!same(&original, &current()));
    }
}
