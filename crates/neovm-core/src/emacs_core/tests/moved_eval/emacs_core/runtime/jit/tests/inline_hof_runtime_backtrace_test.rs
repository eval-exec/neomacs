//! Mid-callback mapcar backtrace on the runtime-startup image.
//!
//! Mounted twice, once for each nextest id the eval file used to produce.
//! The HOF frame stays in eval. This test compares the returned words.

#[test]
fn inline_hof_runtime_mid_callback_backtrace_retains_original_argument() {
    let mut ctx = crate::test_utils::runtime_startup_context();
    let (callback_bits, backtrace_function_bits, argument, depth, specpdl_empty) =
        neovm_eval::test_support::authority::inline_hof_mapcar_backtrace_obs_for_test(&mut ctx);
    assert_eq!(callback_bits, backtrace_function_bits);
    assert_eq!(argument, Some(1));
    assert_eq!(depth, 0);
    assert!(specpdl_empty);
}
