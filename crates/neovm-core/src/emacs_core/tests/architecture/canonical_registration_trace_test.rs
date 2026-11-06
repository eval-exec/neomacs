//! T6 literal equality (1/3): the canonical PRODUCTION registration trace.
//!
//! Peer T6 structure ruling, option (a) (LEAD 04:5xZ): three literal
//! equalities against the one shared, drift-checked builder
//! [`neovm_eval::test_utils::canonical_host_active_trace_for_test`]:
//! (1) core production (this test), (2) eval's `cfg(test)` unit runtime
//! (eval's own pin — the unit runtime cannot be constructed from another
//! crate, and a test-support hook into it is explicitly refused), and
//! (3) display's `cfg(test)` unit runtime (display's own test). Equality
//! is transitive through the builder; all three run in the same P2
//! invocation, alongside the HOST_CFG agreement test.
//!
//! The trace records the single registration choke point
//! (`Context::register_subr`) plus the `EventSymbolProperties` marker at
//! its canonical step, so this comparison pins names, min/max arity and
//! order against the drift-checked plan.

#[test]
fn canonical_production_registration_trace_matches_the_plan() {
    // The T6 structure ruling also requires each test to assert its
    // crate's host-cfg view equals the shared classification. Core emits
    // both host cfgs itself through the shared build-script probe.
    use neovm_eval::host_cfg::HostCfgClass;
    assert_eq!(
        cfg!(neomacs_have_dbus),
        HostCfgClass::Dbus.is_active(),
        "core's neomacs_have_dbus view diverges from HostCfgClass"
    );
    assert_eq!(
        cfg!(neomacs_have_lcms2),
        HostCfgClass::Lcms2.is_active(),
        "core's neomacs_have_lcms2 view diverges from HostCfgClass"
    );
    let expected = neovm_eval::test_utils::canonical_host_active_trace_for_test();
    neovm_eval::emacs_core::subr::reset_registered_subr_trace_for_test();
    let context = crate::test_utils::new_context();
    let trace = neovm_eval::emacs_core::subr::take_registered_subr_trace_for_test();
    drop(context);
    assert_eq!(
        trace, expected,
        "the canonical production registration trace (name/min/max/order plus the \
         event-properties marker) diverges from the shared host-active plan sequence"
    );
}
