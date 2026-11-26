//! Cold local receipt for the terminal Lisp registry. Native hosts and
//! tombstones stay in the parent module. No production caller enters this
//! detach, and tests read the slot directly rather than the activating getter.

use super::TERMINAL_LISP_STATE;
use neovm_eval::emacs_core::eval::data::terminal_state::{
    TerminalLispRegistry, TerminalRegistryHandle,
};
use neovm_value::heap_registry::detachment::{
    DetachedRegistry, RegistryDetachFailure, RegistryRestoreFailure,
};

#[cold]
#[doc(hidden)]
pub fn detach_terminal_registry(
    handle: &TerminalRegistryHandle,
) -> Result<DetachedRegistry<TerminalLispRegistry>, RegistryDetachFailure<TerminalLispRegistry>> {
    TERMINAL_LISP_STATE.with(|slot| {
        // SAFETY: the caller excluded every other use of this thread-local
        // slot for the receipt's lifetime. The receipt is a local effect. It
        // is not World authority and it does not resume a source.
        unsafe { slot.detach(handle, TerminalLispRegistry::default()) }
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
pub(crate) fn restore_terminal_registry(
    receipt: DetachedRegistry<TerminalLispRegistry>,
) -> Result<(), RegistryRestoreFailure<TerminalLispRegistry>> {
    TERMINAL_LISP_STATE.with(|slot| receipt.restore(slot))
}

#[cfg(test)]
mod tests {
    use super::super::TERMINAL_LISP_STATE;
    use super::*;
    use neovm_value::heap_registry::detachment::RegistryDetachError;
    use neovm_value::heap_registry::{HeapRegistryHandle, HeapRegistrySlot};

    fn current() -> TerminalRegistryHandle {
        TERMINAL_LISP_STATE.with(HeapRegistrySlot::current)
    }

    fn same(left: &TerminalRegistryHandle, right: &TerminalRegistryHandle) -> bool {
        HeapRegistryHandle::registry_ptr_eq_for_test(left, right)
    }

    #[test]
    fn pentry_e2b_registry_adapter_terminal_roundtrip() {
        let original = current();
        original.borrow_mut().creation_order.push(41);
        let receipt = detach_terminal_registry(&original).expect("actual slot detach");
        let parked = current();
        assert!(!same(&original, &parked));
        assert_eq!(parked.heap_identity(), 0);
        assert!(parked.borrow().creation_order.is_empty());
        assert_eq!(original.borrow().creation_order, vec![41]);
        restore_terminal_registry(receipt).expect("matched original slot restore");
        assert!(same(&original, &current()));
        assert_eq!(current().borrow().creation_order, vec![41]);
    }

    #[test]
    fn pentry_e2b_registry_adapter_terminal_borrow_refusal() {
        let original = current();
        let view = original.borrow_mut();
        let failure = detach_terminal_registry(&original).expect_err("live original view");
        assert_eq!(failure.reason(), RegistryDetachError::RegistryBorrowed);
        drop(view);
        assert!(same(&original, &current()));
    }

    #[test]
    fn pentry_e2b_registry_adapter_terminal_foreign_preserved() {
        let installed = current();
        let foreign_slot = HeapRegistrySlot::new(TerminalLispRegistry::default());
        let foreign = foreign_slot.current();
        let receipt = detach_terminal_registry(&foreign).expect("foreign is already inactive");
        assert!(same(&installed, &current()));
        restore_terminal_registry(receipt).expect("inactive receipt installs nothing");
        assert!(same(&installed, &current()));
    }

    #[test]
    fn pentry_e2b_registry_adapter_terminal_displaced_restore() {
        let original = current();
        let receipt = detach_terminal_registry(&original).expect("original detach");
        let newer = HeapRegistryHandle::new(TerminalLispRegistry::default());
        TERMINAL_LISP_STATE.with(|slot| slot.install(&newer));
        let failure = restore_terminal_registry(receipt).expect_err("newer installation preserved");
        assert_eq!(failure.reason(), RegistryDetachError::Displaced);
        let (_, owned) = failure.into_parts();
        drop(owned);
        assert!(same(&newer, &current()));
    }

    #[test]
    fn pentry_e2b_registry_adapter_terminal_drop_never_reinstalls() {
        let original = current();
        let receipt = detach_terminal_registry(&original).expect("original detach");
        let parked = current();
        drop(receipt);
        assert!(same(&parked, &current()));
        assert!(!same(&original, &current()));
    }
}
