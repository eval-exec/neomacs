//! Debug-on-entry mapcar on the runtime-startup image.
//!
//! The driver returns the words this test compares. The three messages stay
//! here.

#[test]
fn mapcar_activation_debug_on_entry_preserves_backtrace_and_next_element_redefinition() {
    crate::test_utils::init_test_tracing();
    let mut ctx = crate::test_utils::runtime_startup_context();
    let (
        warmup,
        mapped,
        depth_before,
        specpdl_before,
        depth_after,
        specpdl_after,
        calls_cdr_nil,
        calls_car_debug,
        args,
    ) = neovm_eval::test_support::authority::mapcar_debug_entry_obs_for_test(&mut ctx);
    assert_eq!(warmup, Some(7));
    assert_eq!(
        mapped, "(1 102 103)",
        "advice continues its copied original; the next element sees fset"
    );
    assert_eq!((depth_before, specpdl_before), (depth_after, specpdl_after));
    assert!(
        calls_cdr_nil,
        "fset cancels entry advice for later elements"
    );
    assert!(calls_car_debug);
    assert_eq!(
        args, "(1)",
        "the debugger sees the current callback's argument"
    );
}
