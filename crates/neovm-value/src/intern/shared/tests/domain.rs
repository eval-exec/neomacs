use super::domain::*;
use super::slot::*;

#[test]
fn overlapping_scopes_exclude_grace_until_the_last_release() {
    let domain = InternReadDomain::new();
    let first = domain.try_scope().unwrap();
    let second = domain.try_scope().unwrap();
    assert!(matches!(domain.try_grace(), Err(ReadRefusal::Busy)));
    drop(first);
    assert!(matches!(domain.try_grace(), Err(ReadRefusal::Busy)));
    let snapshot = vec![3, 5, 7];
    let view = second.borrow(&snapshot);
    assert_eq!(view.get()[1], 5);
    drop(view);
    drop(second);
    let grace = domain.try_grace().unwrap();
    assert_eq!(grace.verify_domain(&domain), Ok(()));
    assert!(matches!(domain.try_scope(), Err(ReadRefusal::Busy)));
    assert!(matches!(domain.try_grace(), Err(ReadRefusal::Busy)));
    drop(grace);
    assert!(domain.try_scope().is_ok());
}

#[test]
fn a_world_or_foreign_domain_cannot_certify_process_grace() {
    let first = InternReadDomain::new();
    let second = InternReadDomain::new();
    let scope = first.try_scope().unwrap();
    let foreign_grace = second.try_grace().unwrap();
    assert_eq!(foreign_grace.verify_domain(&first), Err(ForeignDomain));
    assert!(matches!(first.try_grace(), Err(ReadRefusal::Busy)));
    drop(scope);
    assert!(first.try_grace().is_ok());
}

#[test]
fn saturation_and_reserved_bits_refuse_without_a_loan() {
    let saturated = InternReadDomain::seeded_for_test((1 << 47) - 1);
    assert!(matches!(saturated.try_scope(), Err(ReadRefusal::Saturated)));
    assert!(matches!(saturated.try_grace(), Err(ReadRefusal::Busy)));
    let reserved = InternReadDomain::seeded_for_test(1 << 50);
    assert!(matches!(
        reserved.try_scope(),
        Err(ReadRefusal::ReservedState)
    ));
    assert!(matches!(reserved.try_grace(), Err(ReadRefusal::Busy)));
}

#[test]
fn scoped_slot_restores_nested_installations_and_is_empty_at_park() {
    let domain = InternReadDomain::new();
    let scope = domain.try_scope().unwrap();
    let first = InstalledScope::new(&scope);
    let second = InstalledScope::new(&scope);
    let mut slot = ScopeSlot::new();
    assert_eq!(slot.ensure_empty(), Ok(()));
    {
        let mut outer = slot.install(&first);
        assert_eq!(outer.slot().ensure_empty(), Err(ScopeStillInstalled));
        {
            let _inner = outer.install(&second);
        }
        assert_eq!(outer.slot().ensure_empty(), Err(ScopeStillInstalled));
        let payload = [11, 13];
        assert_eq!(first.borrow(&payload).get()[0], 11);
    }
    assert_eq!(slot.ensure_empty(), Ok(()));
}

#[test]
fn unwind_drops_scope_and_installation_before_grace() {
    let domain = InternReadDomain::new();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let scope = domain.try_scope().unwrap();
        let installed = InstalledScope::new(&scope);
        let mut slot = ScopeSlot::new();
        let _installation = slot.install(&installed);
        panic!("scoped unwind");
    }));
    assert!(result.is_err());
    assert!(domain.try_grace().is_ok());
}

#[test]
fn drain_excludes_new_scopes_and_transfers_only_after_existing_readers_exit() {
    let domain = InternReadDomain::new();
    let old = domain.try_scope().unwrap();
    let drain = domain.request_drain().unwrap();
    assert!(matches!(domain.try_scope(), Err(ReadRefusal::Draining)));
    assert!(matches!(
        domain.request_drain(),
        Err(DrainRefusal::AlreadyDraining)
    ));
    assert!(matches!(domain.try_grace(), Err(ReadRefusal::Busy)));
    let drain = match drain.try_grace() {
        Err(pending) => pending,
        Ok(_) => panic!("old scope still held"),
    };
    drop(old);
    let grace = drain
        .try_grace()
        .unwrap_or_else(|_| panic!("drain must now be quiescent"));
    assert!(matches!(domain.try_scope(), Err(ReadRefusal::Busy)));
    drop(grace);
    assert!(domain.try_scope().is_ok());
}

#[test]
fn cancelling_or_unwinding_a_drain_restores_admission_with_old_readers() {
    let domain = InternReadDomain::new();
    let old = domain.try_scope().unwrap();
    domain.request_drain().unwrap().cancel_drain();
    assert!(domain.try_scope().is_ok());
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _drain = domain.request_drain().unwrap();
        panic!("drain owner unwinds");
    }));
    assert!(result.is_err());
    assert!(domain.try_scope().is_ok());
    drop(old);
    assert!(domain.try_grace().is_ok());
}

#[test]
fn grace_exit_preserves_a_concurrent_drain_request_and_cancel_preserves_grace() {
    let domain = InternReadDomain::new();
    let grace = domain.try_grace().unwrap();
    let drain = domain.request_drain().unwrap();
    drop(grace);
    assert!(matches!(domain.try_scope(), Err(ReadRefusal::Draining)));
    let grace = drain
        .try_grace()
        .unwrap_or_else(|_| panic!("old grace retired"));
    domain.request_drain().unwrap().cancel_drain();
    assert!(matches!(domain.try_scope(), Err(ReadRefusal::Busy)));
    drop(grace);
    assert!(domain.try_scope().is_ok());
}
