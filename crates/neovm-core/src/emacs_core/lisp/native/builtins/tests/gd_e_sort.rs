//! GNU sort.c:1061-1129 captures callbacks once and reverses before keys.
//! fns.c:2432-2439 passes the vector's live contents directly to tim_sort.
use crate::emacs_core::format_eval_result;

#[test]
fn sort_key_is_captured_before_redefinition_and_collection() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::runtime_startup_context();
    let form = r#"(progn
      (defalias 'gd-e-key (lambda (x)
        (when (= x 3) (fset 'gd-e-key (lambda (y) (- y))) (garbage-collect)) x))
      (sort [3 1 2] :key #'gd-e-key))"#;
    assert_eq!(format_eval_result(&eval.eval_str(form)), "OK [1 2 3]");
    let form =
        format!("(let ((internal--compiler-function-overrides '((unused . identity)))) {form})");
    assert_eq!(format_eval_result(&eval.eval_str(&form)), "OK [1 2 3]");
}
