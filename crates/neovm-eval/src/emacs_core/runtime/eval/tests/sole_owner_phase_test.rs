use super::*;

#[test]
fn sole_owner_phase_is_pointer_free_and_needs_no_retirement() {
    assert_eq!(std::mem::size_of::<SoleOwnerRegistration>(), 0);
    assert_eq!(std::mem::size_of::<SoleOwnerVm>(), 0);
    assert_eq!(std::mem::size_of::<SoleOwnerDriver>(), 0);
    assert_eq!(std::mem::size_of::<PublishedSoleOwnerDriver>(), 0);
    assert_eq!(std::mem::align_of::<SoleOwnerRegistration>(), 1);
    assert_eq!(std::mem::align_of::<SoleOwnerVm>(), 1);
    assert_eq!(std::mem::align_of::<SoleOwnerDriver>(), 1);
    assert_eq!(std::mem::align_of::<PublishedSoleOwnerDriver>(), 1);
    assert!(!std::mem::needs_drop::<SoleOwnerRegistration>());
    assert!(!std::mem::needs_drop::<SoleOwnerVm>());
    assert!(!std::mem::needs_drop::<SoleOwnerDriver>());
    assert!(!std::mem::needs_drop::<PublishedSoleOwnerDriver>());
}

#[test]
fn sole_owner_factory_registration_adds_no_allocation_lease() {
    let context = Context::new_minimal_vm_harness();
    let before = Arc::strong_count(&context.obarray.lease.0);
    let registration = context.obarray.sole_owner_registration();
    assert_eq!(Arc::strong_count(&context.obarray.lease.0), before);
    let vm = registration.into_vm();
    let driver = vm.driver().into_published().into_running();
    let description = format!("{driver:?}");
    assert!(description.contains("SoleOwnerDriver"));
    assert_eq!(Arc::strong_count(&context.obarray.lease.0), before);
}

#[test]
fn sole_owner_policy_keeps_original_context_reborrowable() {
    let mut context = Context::new_minimal_vm_harness();
    let epoch = context.dispatch_projection().function_epoch();
    let vm = context.obarray.sole_owner_registration().into_vm();
    let driver = vm.driver();
    context
        .obarray
        .invalidate_all_function_bindings(FunctionEpochBump::CompilerOverrides);
    let published = driver.into_published();
    let after = context.dispatch_projection().function_epoch();
    let _rejoined = published.into_running();
    assert!(after > epoch);
    assert!(context.obarray.admission_matches_canonical());
}

#[test]
fn sole_owner_plain_tokens_allow_ordinary_activation_transport() {
    fn transport(driver: SoleOwnerDriver) -> SoleOwnerDriver {
        driver
    }

    let context = Context::new_minimal_vm_harness();
    let vm = context.obarray.sole_owner_registration().into_vm();
    let outer = transport(vm.driver());
    let inner = transport(vm.driver());
    // The policy token deliberately does not claim activation identity or a
    // unique live-driver count. The actual VM borrow supplies access authority.
    assert!(format!("{outer:?}").contains("SoleOwnerDriver"));
    assert!(format!("{inner:?}").contains("SoleOwnerDriver"));
}
