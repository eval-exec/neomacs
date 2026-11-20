//! Cold local receipt for the file-notify registry. Watch delivery stays in
//! the parent module. No production caller enters this detach.

use super::{FILE_NOTIFY_STATE, FileNotifyState};
use neovm_value::heap_registry::HeapRegistryHandle;
use neovm_value::heap_registry::detachment::{
    DetachedRegistry, RegistryDetachFailure, RegistryRestoreFailure,
};

#[cold]
pub(crate) fn detach_file_notify_registry(
    handle: &HeapRegistryHandle<FileNotifyState>,
) -> Result<DetachedRegistry<FileNotifyState>, RegistryDetachFailure<FileNotifyState>> {
    FILE_NOTIFY_STATE.with(|slot| {
        // SAFETY: the caller excluded every other use of this thread-local
        // slot for the receipt's lifetime. The receipt is a local effect. It
        // is not World authority and it does not resume a source.
        unsafe { slot.detach(handle, FileNotifyState::default()) }
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
pub(crate) fn restore_file_notify_registry(
    receipt: DetachedRegistry<FileNotifyState>,
) -> Result<(), RegistryRestoreFailure<FileNotifyState>> {
    FILE_NOTIFY_STATE.with(|slot| receipt.restore(slot))
}

#[cfg(test)]
mod tests {
    use super::super::{FILE_NOTIFY_STATE, FileNotifyState};
    use super::*;
    use neovm_value::heap_registry::detachment::RegistryDetachError;
    use neovm_value::heap_registry::{HeapRegistryHandle, HeapRegistrySlot};

    fn current() -> HeapRegistryHandle<FileNotifyState> {
        FILE_NOTIFY_STATE.with(HeapRegistrySlot::current)
    }

    fn same(
        left: &HeapRegistryHandle<FileNotifyState>,
        right: &HeapRegistryHandle<FileNotifyState>,
    ) -> bool {
        HeapRegistryHandle::registry_ptr_eq_for_test(left, right)
    }

    #[test]
    fn pentry_e2b_registry_adapter_file_notify_roundtrip() {
        let original = current();
        let receipt = detach_file_notify_registry(&original).expect("actual slot detach");
        let parked = current();
        assert!(!same(&original, &parked));
        assert_eq!(parked.heap_identity(), 0);
        restore_file_notify_registry(receipt).expect("matched original slot restore");
        assert!(same(&original, &current()));
    }

    #[test]
    fn pentry_e2b_registry_adapter_file_notify_borrow_refusal() {
        let original = current();
        let view = original.borrow_mut();
        let failure = detach_file_notify_registry(&original).expect_err("live original view");
        assert_eq!(failure.reason(), RegistryDetachError::RegistryBorrowed);
        drop(view);
        assert!(same(&original, &current()));
    }

    #[test]
    fn pentry_e2b_registry_adapter_file_notify_foreign_preserved() {
        let installed = current();
        let foreign_slot = HeapRegistrySlot::new(FileNotifyState::default());
        let foreign = foreign_slot.current();
        let receipt = detach_file_notify_registry(&foreign).expect("foreign is already inactive");
        assert!(same(&installed, &current()));
        restore_file_notify_registry(receipt).expect("inactive receipt installs nothing");
        assert!(same(&installed, &current()));
    }

    #[test]
    fn pentry_e2b_registry_adapter_file_notify_displaced_restore() {
        let original = current();
        let receipt = detach_file_notify_registry(&original).expect("original detach");
        let newer = HeapRegistryHandle::new(FileNotifyState::default());
        FILE_NOTIFY_STATE.with(|slot| slot.install(&newer));
        let failure =
            restore_file_notify_registry(receipt).expect_err("newer installation preserved");
        assert_eq!(failure.reason(), RegistryDetachError::Displaced);
        let (_, owned) = failure.into_parts();
        drop(owned);
        assert!(same(&newer, &current()));
    }

    #[test]
    fn pentry_e2b_registry_adapter_file_notify_drop_never_reinstalls() {
        let original = current();
        let receipt = detach_file_notify_registry(&original).expect("original detach");
        let parked = current();
        drop(receipt);
        assert!(same(&parked, &current()));
        assert!(!same(&original, &current()));
    }
}
