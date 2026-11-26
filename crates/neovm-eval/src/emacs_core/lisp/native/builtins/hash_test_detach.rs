//! Cold local receipt for the hash-test alias registry. Registration stays
//! in `collections.rs`. No production caller enters this detach.

use super::{HASH_TABLE_TEST_ALIASES, HashTableTestRegistry};
use neovm_value::heap_registry::HeapRegistryHandle;
use neovm_value::heap_registry::detachment::{
    DetachedRegistry, RegistryDetachFailure, RegistryRestoreFailure,
};

#[cold]
pub(crate) fn detach_hash_table_test_registry(
    handle: &HeapRegistryHandle<HashTableTestRegistry>,
) -> Result<DetachedRegistry<HashTableTestRegistry>, RegistryDetachFailure<HashTableTestRegistry>> {
    HASH_TABLE_TEST_ALIASES.with(|slot| {
        // SAFETY: the caller excluded every other use of this thread-local
        // slot for the receipt's lifetime. The receipt is a local effect. It
        // is not World authority and it does not resume a source.
        unsafe { slot.detach(handle, HashTableTestRegistry::default()) }
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
pub(crate) fn restore_hash_table_test_registry(
    receipt: DetachedRegistry<HashTableTestRegistry>,
) -> Result<(), RegistryRestoreFailure<HashTableTestRegistry>> {
    HASH_TABLE_TEST_ALIASES.with(|slot| receipt.restore(slot))
}

#[cfg(test)]
mod tests {
    use super::super::{HASH_TABLE_TEST_ALIASES, HashTableTestAlias, HashTableTestRegistry};
    use super::*;
    use neovm_value::heap_registry::detachment::RegistryDetachError;
    use neovm_value::heap_registry::{HeapRegistryHandle, HeapRegistrySlot};

    fn current() -> HeapRegistryHandle<HashTableTestRegistry> {
        HASH_TABLE_TEST_ALIASES.with(HeapRegistrySlot::current)
    }

    fn same(
        left: &HeapRegistryHandle<HashTableTestRegistry>,
        right: &HeapRegistryHandle<HashTableTestRegistry>,
    ) -> bool {
        HeapRegistryHandle::registry_ptr_eq_for_test(left, right)
    }

    fn seed(handle: &HeapRegistryHandle<HashTableTestRegistry>) {
        handle.borrow_mut().aliases.insert(
            "pentry-e2b".to_string(),
            HashTableTestAlias {
                standard_test: None,
                user_cmp_function: None,
                user_hash_function: None,
            },
        );
    }

    #[test]
    fn pentry_e2b_registry_adapter_hash_table_test_roundtrip() {
        let original = current();
        seed(&original);
        let receipt = detach_hash_table_test_registry(&original).expect("actual slot detach");
        let parked = current();
        assert!(!same(&original, &parked));
        assert_eq!(parked.heap_identity(), 0);
        assert!(parked.borrow().aliases.is_empty());
        assert!(original.borrow().aliases.contains_key("pentry-e2b"));
        restore_hash_table_test_registry(receipt).expect("matched original slot restore");
        assert!(same(&original, &current()));
        assert!(current().borrow().aliases.contains_key("pentry-e2b"));
    }

    #[test]
    fn pentry_e2b_registry_adapter_hash_table_test_borrow_refusal() {
        let original = current();
        let view = original.borrow_mut();
        let failure = detach_hash_table_test_registry(&original).expect_err("live original view");
        assert_eq!(failure.reason(), RegistryDetachError::RegistryBorrowed);
        drop(view);
        assert!(same(&original, &current()));
    }

    #[test]
    fn pentry_e2b_registry_adapter_hash_table_test_foreign_preserved() {
        let installed = current();
        let foreign_slot = HeapRegistrySlot::new(HashTableTestRegistry::default());
        let foreign = foreign_slot.current();
        let receipt =
            detach_hash_table_test_registry(&foreign).expect("foreign is already inactive");
        assert!(same(&installed, &current()));
        restore_hash_table_test_registry(receipt).expect("inactive receipt installs nothing");
        assert!(same(&installed, &current()));
    }

    #[test]
    fn pentry_e2b_registry_adapter_hash_table_test_displaced_restore() {
        let original = current();
        let receipt = detach_hash_table_test_registry(&original).expect("original detach");
        let newer = HeapRegistryHandle::new(HashTableTestRegistry::default());
        HASH_TABLE_TEST_ALIASES.with(|slot| slot.install(&newer));
        let failure =
            restore_hash_table_test_registry(receipt).expect_err("newer installation preserved");
        assert_eq!(failure.reason(), RegistryDetachError::Displaced);
        let (_, owned) = failure.into_parts();
        drop(owned);
        assert!(same(&newer, &current()));
    }

    #[test]
    fn pentry_e2b_registry_adapter_hash_table_test_drop_never_reinstalls() {
        let original = current();
        let receipt = detach_hash_table_test_registry(&original).expect("original detach");
        let parked = current();
        drop(receipt);
        assert!(same(&parked, &current()));
        assert!(!same(&original, &current()));
    }
}
