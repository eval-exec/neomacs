//! Cold local receipt for the face-attribute registry. Realization stays in
//! the parent module. No production caller enters this detach.

use super::FACE_ATTR_STATE;
use neovm_eval::emacs_core::eval::data::face_registry::{FaceAttrState, FontRegistryHandle};
use neovm_value::heap_registry::detachment::{
    DetachedRegistry, RegistryDetachFailure, RegistryRestoreFailure,
};

#[cold]
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "E2 L3 dormant cold detach; no production caller enters it"
    )
)]
pub(crate) fn detach_font_registry(
    handle: &FontRegistryHandle,
) -> Result<DetachedRegistry<FaceAttrState>, RegistryDetachFailure<FaceAttrState>> {
    FACE_ATTR_STATE.with(|slot| {
        // SAFETY: the caller excluded every other use of this thread-local
        // slot for the receipt's lifetime. The receipt is a local effect. It
        // is not World authority and it does not resume a source.
        unsafe { slot.detach(handle, FaceAttrState::default()) }
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
pub(crate) fn restore_font_registry(
    receipt: DetachedRegistry<FaceAttrState>,
) -> Result<(), RegistryRestoreFailure<FaceAttrState>> {
    FACE_ATTR_STATE.with(|slot| receipt.restore(slot))
}

#[cfg(test)]
mod tests {
    use super::super::FACE_ATTR_STATE;
    use super::*;
    use neovm_value::heap_registry::detachment::RegistryDetachError;
    use neovm_value::heap_registry::{HeapRegistryHandle, HeapRegistrySlot};

    fn current() -> FontRegistryHandle {
        FACE_ATTR_STATE.with(HeapRegistrySlot::current)
    }

    fn same(left: &FontRegistryHandle, right: &FontRegistryHandle) -> bool {
        HeapRegistryHandle::registry_ptr_eq_for_test(left, right)
    }

    #[test]
    fn pentry_e2b_registry_adapter_font_roundtrip() {
        let original = current();
        let seeded = original.borrow().next_created_face_id;
        original.borrow_mut().next_created_face_id = seeded.wrapping_add(7);
        let receipt = detach_font_registry(&original).expect("actual slot detach");
        let parked = current();
        assert!(!same(&original, &parked));
        assert_eq!(parked.heap_identity(), 0);
        assert_eq!(
            parked.borrow().next_created_face_id,
            FaceAttrState::default().next_created_face_id
        );
        assert_eq!(
            original.borrow().next_created_face_id,
            seeded.wrapping_add(7)
        );
        restore_font_registry(receipt).expect("matched original slot restore");
        assert!(same(&original, &current()));
        assert_eq!(
            current().borrow().next_created_face_id,
            seeded.wrapping_add(7)
        );
    }

    #[test]
    fn pentry_e2b_registry_adapter_font_borrow_refusal() {
        let original = current();
        let view = original.borrow_mut();
        let failure = detach_font_registry(&original).expect_err("live original view");
        assert_eq!(failure.reason(), RegistryDetachError::RegistryBorrowed);
        drop(view);
        assert!(same(&original, &current()));
    }

    #[test]
    fn pentry_e2b_registry_adapter_font_foreign_preserved() {
        let installed = current();
        let foreign_slot = HeapRegistrySlot::new(FaceAttrState::default());
        let foreign = foreign_slot.current();
        let receipt = detach_font_registry(&foreign).expect("foreign is already inactive");
        assert!(same(&installed, &current()));
        restore_font_registry(receipt).expect("inactive receipt installs nothing");
        assert!(same(&installed, &current()));
    }

    #[test]
    fn pentry_e2b_registry_adapter_font_displaced_restore() {
        let original = current();
        let receipt = detach_font_registry(&original).expect("original detach");
        let newer = HeapRegistryHandle::new(FaceAttrState::default());
        FACE_ATTR_STATE.with(|slot| slot.install(&newer));
        let failure = restore_font_registry(receipt).expect_err("newer installation preserved");
        assert_eq!(failure.reason(), RegistryDetachError::Displaced);
        let (_, owned) = failure.into_parts();
        drop(owned);
        assert!(same(&newer, &current()));
    }

    #[test]
    fn pentry_e2b_registry_adapter_font_drop_never_reinstalls() {
        let original = current();
        let receipt = detach_font_registry(&original).expect("original detach");
        let parked = current();
        drop(receipt);
        assert!(same(&parked, &current()));
        assert!(!same(&original, &current()));
    }
}
