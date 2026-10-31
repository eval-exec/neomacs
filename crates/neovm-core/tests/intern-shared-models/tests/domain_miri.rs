#![allow(dead_code)]
mod sync {
    pub use std::sync::atomic::{AtomicU64, Ordering};
}
#[path = "../../../src/emacs_core/runtime/intern/shared/domain.rs"]
mod domain;
#[path = "../../../src/emacs_core/runtime/intern/shared/slot.rs"]
mod slot;
use domain::*;
use slot::*;

#[test]
fn real_scope_view_and_nested_slot_borrows_keep_the_snapshot_live() {
    let domain = InternReadDomain::new();
    let owner = std::sync::Arc::new(vec![2u64, 3, 5, 7]);
    let predecessor = std::sync::Arc::clone(&owner);
    {
        let scope = domain.try_scope().unwrap();
        let installed = InstalledScope::new(&scope);
        let mut slot = ScopeSlot::new();
        {
            let mut outer = slot.install(&installed);
            {
                let _inner = outer.install(&installed);
            }
            let view = installed.borrow(&*predecessor);
            assert_eq!(view.get().as_slice(), &[2, 3, 5, 7]);
            assert!(matches!(domain.try_grace(), Err(ReadRefusal::Busy)));
        }
        slot.ensure_empty().unwrap();
    }
    drop(predecessor);
    let grace = domain.try_grace().unwrap();
    grace.verify_domain(&domain).unwrap();
    drop(owner);
}

#[test]
fn real_scope_drop_survives_panic_and_allows_grace() {
    let domain = InternReadDomain::new();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let scope = domain.try_scope().unwrap();
        let owner = Box::new([11, 13]);
        let view = scope.borrow(&owner);
        assert_eq!(view.get()[1], 13);
        panic!("view scope unwinds");
    }));
    assert!(result.is_err());
    assert!(domain.try_grace().is_ok());
}

#[test]
fn draining_retains_old_views_then_transfers_or_cancels_exclusivity() {
    let domain = InternReadDomain::new();
    let owner = Box::new([17, 19]);
    let scope = domain.try_scope().unwrap();
    let view = scope.borrow(&owner);
    let drain = domain.request_drain().unwrap();
    assert_eq!(view.get()[0], 17);
    assert!(matches!(domain.try_scope(), Err(ReadRefusal::Draining)));
    let drain = match drain.try_grace() {
        Err(pending) => pending,
        Ok(_) => panic!("old view still held"),
    };
    drop(view);
    drop(scope);
    let grace = drain
        .try_grace()
        .unwrap_or_else(|_| panic!("old view retired"));
    drop(owner);
    drop(grace);
    domain.request_drain().unwrap().cancel_drain();
    assert!(domain.try_scope().is_ok());
}
