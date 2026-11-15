//! VM rows for the absent-layer misc cluster of
//! `vm_base64_json_ccl_and_runtime_clusters_use_direct_dispatch`.
//!
//! `init-image-library` and `dump-emacs-portable` are core-owned placements
//! and `clear-image-cache` is display-owned, so eval's unit runtime holds
//! panicking stand-ins for all three and the rows moved here run against
//! core's canonical install. The rows and expectations are unchanged; in
//! this runtime `clear-image-cache` takes GNU's unopenable-window-system
//! error branch (image.c:2451 `decode_window_system_frame` on the selected
//! frame), which the row's `condition-case` folds to t exactly as GNU's
//! batch run does.

use neovm_value::error::EvalError;
use neovm_value::value::Value;

fn vm_eval_str(src: &str) -> String {
    let mut eval = crate::test_utils::new_context();
    eval.set_lexical_binding(false);
    let result: Result<Value, EvalError> = eval.eval_str(src);
    crate::emacs_core::format_eval_result(&result)
}

#[test]
fn vm_init_image_library_answers_nil_for_no_library() {
    crate::test_utils::init_test_tracing();
    assert_eq!(vm_eval_str("(null (init-image-library nil))"), "OK t");
}

#[test]
fn vm_clear_image_cache_folds_the_window_system_error_to_t_like_gnu() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str("(condition-case nil (clear-image-cache nil) (error t))"),
        "OK t"
    );
}

#[test]
fn vm_dump_emacs_portable_answers_nil_without_dumping() {
    crate::test_utils::init_test_tracing();
    assert_eq!(vm_eval_str(r#"(null (dump-emacs-portable "x"))"#), "OK t");
}
