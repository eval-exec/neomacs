//! Cold local receipt for the window-configuration snapshot map. The snapshot
//! type has no empty constructor beyond the map itself. No production caller
//! enters this detach.

use super::WINDOW_CONFIGURATION_SNAPSHOTS;
use neovm_value::heap_registry::HeapRegistryHandle;
use neovm_value::heap_registry::detachment::{
    DetachedRegistry, RegistryDetachFailure, RegistryRestoreFailure,
};
use std::collections::HashMap;

use super::WindowConfigurationSnapshot;

#[cold]
pub(crate) fn detach_window_configuration_registry(
    handle: &HeapRegistryHandle<HashMap<i64, WindowConfigurationSnapshot>>,
) -> Result<
    DetachedRegistry<HashMap<i64, WindowConfigurationSnapshot>>,
    RegistryDetachFailure<HashMap<i64, WindowConfigurationSnapshot>>,
> {
    WINDOW_CONFIGURATION_SNAPSHOTS.with(|slot| {
        // SAFETY: the caller excluded every other use of this thread-local
        // slot for the receipt's lifetime. The receipt is a local effect. It
        // is not World authority and it does not resume a source.
        unsafe { slot.detach(handle, HashMap::new()) }
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
pub(crate) fn restore_window_configuration_registry(
    receipt: DetachedRegistry<HashMap<i64, WindowConfigurationSnapshot>>,
) -> Result<(), RegistryRestoreFailure<HashMap<i64, WindowConfigurationSnapshot>>> {
    WINDOW_CONFIGURATION_SNAPSHOTS.with(|slot| receipt.restore(slot))
}

#[cfg(test)]
mod tests {
    use super::super::WINDOW_CONFIGURATION_SNAPSHOTS;
    use super::*;
    use neovm_value::heap_registry::HeapRegistrySlot;
    use neovm_value::heap_registry::detachment::RegistryDetachError;

    type Handle = HeapRegistryHandle<HashMap<i64, WindowConfigurationSnapshot>>;

    fn current() -> Handle {
        WINDOW_CONFIGURATION_SNAPSHOTS.with(HeapRegistrySlot::current)
    }

    fn same(left: &Handle, right: &Handle) -> bool {
        HeapRegistryHandle::registry_ptr_eq_for_test(left, right)
    }

    #[test]
    fn pentry_e2b_registry_adapter_window_configuration_roundtrip() {
        let original = current();
        let receipt = detach_window_configuration_registry(&original).expect("actual slot detach");
        let parked = current();
        assert!(!same(&original, &parked));
        assert_eq!(parked.heap_identity(), 0);
        assert!(parked.borrow().is_empty());
        restore_window_configuration_registry(receipt).expect("matched original slot restore");
        assert!(same(&original, &current()));
    }

    #[test]
    fn pentry_e2b_registry_adapter_window_configuration_borrow_refusal() {
        let original = current();
        let view = original.borrow_mut();
        let failure =
            detach_window_configuration_registry(&original).expect_err("live original view");
        assert_eq!(failure.reason(), RegistryDetachError::RegistryBorrowed);
        drop(view);
        assert!(same(&original, &current()));
    }

    #[test]
    fn pentry_e2b_registry_adapter_window_configuration_foreign_preserved() {
        let installed = current();
        let foreign_slot = HeapRegistrySlot::new(HashMap::new());
        let foreign = foreign_slot.current();
        let receipt =
            detach_window_configuration_registry(&foreign).expect("foreign is already inactive");
        assert!(same(&installed, &current()));
        restore_window_configuration_registry(receipt).expect("inactive receipt installs nothing");
        assert!(same(&installed, &current()));
    }

    #[test]
    fn pentry_e2b_registry_adapter_window_configuration_displaced_restore() {
        let original = current();
        let receipt = detach_window_configuration_registry(&original).expect("original detach");
        let newer = HeapRegistryHandle::new(HashMap::new());
        WINDOW_CONFIGURATION_SNAPSHOTS.with(|slot| slot.install(&newer));
        let failure = restore_window_configuration_registry(receipt)
            .expect_err("newer installation preserved");
        assert_eq!(failure.reason(), RegistryDetachError::Displaced);
        let (_, owned) = failure.into_parts();
        drop(owned);
        assert!(same(&newer, &current()));
    }

    #[test]
    fn pentry_e2b_registry_adapter_window_configuration_drop_never_reinstalls() {
        let original = current();
        let receipt = detach_window_configuration_registry(&original).expect("original detach");
        let parked = current();
        drop(receipt);
        assert!(same(&parked, &current()));
        assert!(!same(&original, &current()));
    }
}
