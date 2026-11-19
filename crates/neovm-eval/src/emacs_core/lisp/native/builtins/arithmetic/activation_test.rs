use super::super::{INTEGER_WIDTH, integer_width_below};
use super::{IntegerWidthDetachError, restore_integer_width_activation};
use crate::emacs_core::eval::Context;
use neovm_value::value::Value;
use std::sync::{Arc, Weak};

fn source() -> Context {
    let mut source = Context::new_minimal_vm_harness(crate::test_utils::unit_harness());
    source.obarray.define_int_variable("integer-width", 65536);
    source.set_variable("integer-width", Value::fixnum(200));
    source.setup_thread_locals();
    assert!(integer_width_below(300), "real policy must be installed");
    source
}

fn active_is(owner: &Weak<()>) -> bool {
    INTEGER_WIDTH.with(|slot| {
        slot.borrow()
            .as_ref()
            .is_some_and(|binding| binding.owner.ptr_eq(owner))
    })
}

#[test]
fn pentry_e2b_integer_width_detach_disables_and_restores_actual_policy() {
    let source = source();
    let original = Arc::downgrade(&source.integer_width_context.lifetime);
    let receipt = source.integer_width_context.detach_activation().unwrap();
    assert!(!integer_width_below(300), "disabled policy uses default");
    assert!(active_is(&receipt.placeholder));
    assert!(receipt.placeholder.upgrade().is_none());
    restore_integer_width_activation(receipt).expect("same binding restored");
    assert!(active_is(&original));
    assert!(integer_width_below(300));
}

#[test]
fn pentry_e2b_integer_width_detach_preserves_absent_slot() {
    let owner = super::super::IntegerWidthContext::default();
    let removed = INTEGER_WIDTH.with(|slot| slot.borrow_mut().take());
    drop(removed);
    let receipt = owner.detach_activation().unwrap();
    assert!(INTEGER_WIDTH.with(|slot| slot.borrow().is_none()));
    restore_integer_width_activation(receipt).expect("inactive no-op");
    assert!(INTEGER_WIDTH.with(|slot| slot.borrow().is_none()));
}

#[test]
fn pentry_e2b_integer_width_detach_preserves_foreign_activation() {
    let source = source();
    let newer = self::source();
    let owner = Arc::downgrade(&newer.integer_width_context.lifetime);
    let receipt = source.integer_width_context.detach_activation().unwrap();
    assert!(active_is(&owner));
    restore_integer_width_activation(receipt).expect("inactive no-op");
    assert!(active_is(&owner));
    assert!(integer_width_below(300));
}

#[test]
fn pentry_e2b_integer_width_detach_refuses_slot_reader() {
    let source = source();
    INTEGER_WIDTH.with(|slot| {
        let view = slot.borrow();
        let failure = source.integer_width_context.detach_activation();
        assert!(matches!(
            failure,
            Err(IntegerWidthDetachError::SlotBorrowed)
        ));
        assert!(view.as_ref().unwrap().owner.upgrade().is_some());
    });
    assert!(integer_width_below(300));
}

#[test]
fn pentry_e2b_integer_width_detach_refuses_slot_writer() {
    let source = source();
    INTEGER_WIDTH.with(|slot| {
        let view = slot.borrow_mut();
        assert!(matches!(
            source.integer_width_context.detach_activation(),
            Err(IntegerWidthDetachError::SlotBorrowed)
        ));
        assert!(view.as_ref().unwrap().owner.upgrade().is_some());
    });
}

#[test]
fn pentry_e2b_integer_width_detach_restore_owns_receipt_on_borrow() {
    let source = source();
    let receipt = source.integer_width_context.detach_activation().unwrap();
    let failure = INTEGER_WIDTH.with(|slot| {
        let _view = slot.borrow();
        restore_integer_width_activation(receipt).expect_err("live slot view")
    });
    assert_eq!(failure.reason, IntegerWidthDetachError::SlotBorrowed);
    let (_, receipt) = failure.into_parts();
    restore_integer_width_activation(receipt).expect("same receipt retries after view ends");
    assert!(integer_width_below(300));
}

#[test]
fn pentry_e2b_integer_width_detach_refuses_displaced_restoration() {
    let source = source();
    let receipt = source.integer_width_context.detach_activation().unwrap();
    let newer = self::source();
    let owner = Arc::downgrade(&newer.integer_width_context.lifetime);
    let failure = restore_integer_width_activation(receipt).expect_err("newer activation");
    assert_eq!(failure.reason, IntegerWidthDetachError::Displaced);
    drop(failure);
    assert!(active_is(&owner));
    assert!(integer_width_below(300));
}

#[test]
fn pentry_e2b_integer_width_detach_refuses_same_owner_reactivation_aba() {
    let mut source = source();
    let receipt = source.integer_width_context.detach_activation().unwrap();
    source.setup_thread_locals();
    let failure = restore_integer_width_activation(receipt).expect_err("same owner displaced");
    assert_eq!(failure.reason, IntegerWidthDetachError::Displaced);
    assert!(integer_width_below(300));
}

#[test]
fn pentry_e2b_integer_width_detach_repeated_inactive_receipt_does_not_restore_outer() {
    let source = source();
    let outer = source.integer_width_context.detach_activation().unwrap();
    let inner = source.integer_width_context.detach_activation().unwrap();
    restore_integer_width_activation(inner).expect("inner observes inactive original");
    assert!(active_is(&outer.placeholder));
    assert!(!integer_width_below(300));
    restore_integer_width_activation(outer).expect("only actual removal reinstalls");
    assert!(integer_width_below(300));
}

#[test]
fn pentry_e2b_integer_width_detach_drop_never_reactivates() {
    let source = source();
    let receipt = source.integer_width_context.detach_activation().unwrap();
    let placeholder = receipt.placeholder.clone();
    drop(receipt);
    assert!(active_is(&placeholder));
    assert!(placeholder.upgrade().is_none());
    assert!(!integer_width_below(300));
}

#[test]
fn pentry_e2b_integer_width_detach_forget_never_reactivates() {
    let source = source();
    let receipt = source.integer_width_context.detach_activation().unwrap();
    let placeholder = receipt.placeholder.clone();
    std::mem::forget(receipt);
    assert!(active_is(&placeholder));
    assert!(placeholder.upgrade().is_none());
    assert!(!integer_width_below(300));
}

#[test]
fn pentry_e2b_integer_width_detach_unwind_never_reactivates() {
    let source = source();
    let receipt = source.integer_width_context.detach_activation().unwrap();
    let placeholder = receipt.placeholder.clone();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
        let _receipt = receipt;
        panic!("INTEGER_WIDTH_DETACH_UNWIND_FAULT");
    }));
    assert!(result.is_err());
    assert!(active_is(&placeholder));
    assert!(!integer_width_below(300));
}

#[test]
fn pentry_e2b_integer_width_detach_restore_refuses_retired_context() {
    let source = source();
    let receipt = source.integer_width_context.detach_activation().unwrap();
    let placeholder = receipt.placeholder.clone();
    drop(source);
    let failure = restore_integer_width_activation(receipt).expect_err("retired owner");
    assert_eq!(failure.reason, IntegerWidthDetachError::OwnerRetired);
    assert!(active_is(&placeholder));
    assert!(!integer_width_below(300));
}

#[test]
fn pentry_e2b_integer_width_detach_dead_sentinels_have_distinct_identity() {
    let source = source();
    let first = source.integer_width_context.detach_activation().unwrap();
    let second = source.integer_width_context.detach_activation().unwrap();
    assert!(!first.placeholder.ptr_eq(&second.placeholder));
    assert!(first.placeholder.upgrade().is_none());
    assert!(second.placeholder.upgrade().is_none());
    restore_integer_width_activation(second).unwrap();
    restore_integer_width_activation(first).unwrap();
}
