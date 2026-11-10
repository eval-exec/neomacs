//! The deferred between-jobs reset must diagnose a leaked native guard.

#[test]
fn deferred_carrier_reset_rejects_a_leaked_native_depth_before_zeroing() {
    let observation = neovm_eval::test_support::authority::native_depth_reset_leak_probe_for_test();
    #[cfg(debug_assertions)]
    {
        assert!(
            observation.panicked(),
            "a leaked native guard must reject carrier reuse"
        );
        assert_eq!(
            observation.retained_depth(),
            1,
            "the failed deferred reset must preserve the leaked depth before fixture cleanup"
        );
    }
    #[cfg(not(debug_assertions))]
    {
        assert!(
            !observation.panicked(),
            "release has no native-depth assertion or leaked counter"
        );
        assert_eq!(observation.retained_depth(), 0);
    }
    assert_eq!(observation.normal_reset_depth(), 0);
}
