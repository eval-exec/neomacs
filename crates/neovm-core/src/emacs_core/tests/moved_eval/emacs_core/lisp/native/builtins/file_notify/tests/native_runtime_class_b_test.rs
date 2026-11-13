#[test]
#[cfg(target_os = "linux")]
fn inotify_rename_delivers_an_ordered_cookie_pair() {
    neovm_eval::test_support::authority::run_inotify_rename_delivers_an_ordered_cookie_pair_for_test(
        crate::test_utils::new_context,
    );
}

#[test]
fn native_callback_can_remove_its_own_watch() {
    neovm_eval::test_support::authority::run_native_callback_can_remove_its_own_watch_for_test(
        crate::test_utils::new_context,
    );
}
