use super::super::{ASYNC_ATTENTION, QUIT_REQUESTED_TLS, QuitRequest, quit_pending};
use super::{QuitDetachError, detach_quit_activation, restore_quit_activation, same_request};

fn active_is(expected: &QuitRequest) -> bool {
    QUIT_REQUESTED_TLS.with(|slot| {
        slot.borrow()
            .as_ref()
            .is_some_and(|current| same_request(current, expected))
    })
}

fn install(flag: &QuitRequest) {
    QUIT_REQUESTED_TLS.with(|slot| *slot.borrow_mut() = Some(flag.clone()));
}

#[test]
fn pentry_e2b_quit_detach_restores_exact_flag() {
    let original = QuitRequest::new();
    install(&original);
    let receipt = unsafe { detach_quit_activation(&original) }.unwrap();
    assert!(active_is(&receipt.placeholder));
    assert!(!active_is(&original));
    assert!(!quit_pending());
    restore_quit_activation(receipt).expect("restore original poll source");
    assert!(active_is(&original));
}

#[test]
fn pentry_e2b_quit_detach_preserves_already_raised_request() {
    let original = QuitRequest::new();
    original.request();
    install(&original);
    assert!(quit_pending());
    let before = ASYNC_ATTENTION.load();
    let receipt = unsafe { detach_quit_activation(&original) }.unwrap();
    assert!(original.is_requested());
    assert!(
        !quit_pending(),
        "leaf poll observes only the lowered sentinel"
    );
    assert_eq!(
        ASYNC_ATTENTION.load(),
        before,
        "original raise remains counted"
    );
    restore_quit_activation(receipt).unwrap();
    assert!(quit_pending());
    assert!(original.take());
    assert!(!quit_pending());
}

#[test]
fn pentry_e2b_quit_detach_preserves_absent_slot() {
    let expected = QuitRequest::new();
    let removed = QUIT_REQUESTED_TLS.with(|slot| slot.borrow_mut().take());
    drop(removed);
    let receipt = unsafe { detach_quit_activation(&expected) }.unwrap();
    assert!(QUIT_REQUESTED_TLS.with(|slot| slot.borrow().is_none()));
    restore_quit_activation(receipt).unwrap();
    assert!(QUIT_REQUESTED_TLS.with(|slot| slot.borrow().is_none()));
}

#[test]
fn pentry_e2b_quit_detach_preserves_foreign_lowered_flag() {
    let original = QuitRequest::new();
    let foreign = QuitRequest::new();
    install(&foreign);
    let receipt = unsafe { detach_quit_activation(&original) }.unwrap();
    assert!(active_is(&foreign));
    restore_quit_activation(receipt).unwrap();
    assert!(active_is(&foreign));
}

#[test]
fn pentry_e2b_quit_detach_accepts_clone_of_exact_request() {
    let original = QuitRequest::new();
    install(&original);
    let same = original.clone();
    let receipt = unsafe { detach_quit_activation(&same) }.unwrap();
    assert!(!active_is(&original));
    restore_quit_activation(receipt).unwrap();
    assert!(active_is(&original));
}

#[test]
fn pentry_e2b_quit_detach_refuses_slot_reader() {
    let original = QuitRequest::new();
    install(&original);
    QUIT_REQUESTED_TLS.with(|slot| {
        let view = slot.borrow();
        assert!(matches!(
            unsafe { detach_quit_activation(&original) },
            Err(QuitDetachError::SlotBorrowed)
        ));
        assert!(same_request(view.as_ref().unwrap(), &original));
    });
}

#[test]
fn pentry_e2b_quit_detach_refuses_slot_writer() {
    let original = QuitRequest::new();
    install(&original);
    QUIT_REQUESTED_TLS.with(|slot| {
        let view = slot.borrow_mut();
        assert!(matches!(
            unsafe { detach_quit_activation(&original) },
            Err(QuitDetachError::SlotBorrowed)
        ));
        assert!(same_request(view.as_ref().unwrap(), &original));
    });
}

#[test]
fn pentry_e2b_quit_detach_restore_owns_receipt_on_borrow() {
    let original = QuitRequest::new();
    install(&original);
    let receipt = unsafe { detach_quit_activation(&original) }.unwrap();
    let failure = QUIT_REQUESTED_TLS.with(|slot| {
        let _view = slot.borrow();
        restore_quit_activation(receipt).expect_err("live slot view")
    });
    assert_eq!(failure.reason, QuitDetachError::SlotBorrowed);
    let (_, receipt) = failure.into_parts();
    restore_quit_activation(receipt).expect("same receipt retries after view ends");
    assert!(active_is(&original));
}

#[test]
fn pentry_e2b_quit_detach_restore_refuses_newer_activation() {
    let original = QuitRequest::new();
    install(&original);
    let receipt = unsafe { detach_quit_activation(&original) }.unwrap();
    let newer = QuitRequest::new();
    install(&newer);
    let failure = restore_quit_activation(receipt).expect_err("newer activation");
    assert_eq!(failure.reason, QuitDetachError::Displaced);
    drop(failure);
    assert!(active_is(&newer));
}

#[test]
fn pentry_e2b_quit_detach_unique_sentinel_refuses_nested_aba() {
    let original = QuitRequest::new();
    install(&original);
    let outer = unsafe { detach_quit_activation(&original) }.unwrap();
    let inner = unsafe { detach_quit_activation(&outer.placeholder) }.unwrap();
    let failure = restore_quit_activation(outer).expect_err("QUIT_DETACH_ABA_REFUSED");
    assert_eq!(failure.reason, QuitDetachError::Displaced);
    assert!(active_is(&inner.placeholder));
    restore_quit_activation(inner).unwrap();
    let (_, outer) = failure.into_parts();
    restore_quit_activation(outer).unwrap();
    assert!(active_is(&original));
}

#[test]
fn pentry_e2b_quit_detach_drop_never_restores_or_clears_original() {
    let original = QuitRequest::new();
    install(&original);
    let receipt = unsafe { detach_quit_activation(&original) }.unwrap();
    let placeholder = receipt.placeholder.clone();
    original.request();
    drop(receipt);
    assert!(active_is(&placeholder));
    assert!(original.is_requested());
    assert!(!quit_pending());
    assert!(original.take());
}
