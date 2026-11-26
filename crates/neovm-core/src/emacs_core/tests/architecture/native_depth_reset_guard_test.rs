//! The deferred between-jobs reset must refuse a leaked native guard.

#[test]
fn deferred_carrier_reset_rejects_a_leaked_native_depth_before_zeroing() {
    let observation = neovm_eval::test_support::authority::native_depth_reset_leak_probe_for_test();
    assert!(
        !observation.panicked(),
        "a leaked native guard refuses the serial receipt without panicking"
    );
    assert_eq!(
        observation.retained_depth(),
        1,
        "the refused reset preserves the leaked depth"
    );
    assert_eq!(observation.normal_reset_depth(), 0);
}
