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

#[test]
fn sort_reverse_visits_keys_in_reversed_order() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::runtime_startup_context();
    let form = r#"(let ((seen nil))
      (list (sort [3 1 2] :reverse t :key (lambda (x) (push x seen) x))
            (nreverse seen)))"#;
    assert_eq!(
        format_eval_result(&eval.eval_str(form)),
        "OK ([3 2 1] (2 1 3))"
    );
}

#[test]
fn sort_vector_signal_keeps_completed_reordering() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::runtime_startup_context();
    let form = r#"(let ((v (vector 3 2 1 5 4)) (n 0))
      (condition-case err
        (sort v :lessp (lambda (a b) (setq n (1+ n))
          (when (= n 6) (error "stop")) (< a b)) :in-place t)
        (error nil))
      (list v n))"#;
    assert_eq!(
        format_eval_result(&eval.eval_str(form)),
        "OK ([1 2 3 5 4] 6)"
    );
}

#[test]
fn sort_vector_keys_read_later_mutated_slots() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::runtime_startup_context();
    let form = r#"(let ((v (vector 3 2 1)))
      (list (sort v :in-place t :key (lambda (x) (when (= x 3) (aset v 1 10)) x)) v))"#;
    assert_eq!(
        format_eval_result(&eval.eval_str(form)),
        "OK ([1 3 10] [1 3 10])"
    );
}

#[test]
fn sort_vector_small_merge_signal_keeps_gnu_unrestored_slots() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::runtime_startup_context();
    let form = r#"(let ((v (vconcat (number-sequence 32 127) (number-sequence 0 31))) (n 0))
      (condition-case nil (sort v :in-place t :lessp (lambda (a b)
        (setq n (1+ n)) (when (= n 145) (error "stop")) (< a b))) (error nil))
      (list n (equal (sort v) (vconcat (number-sequence 0 127)))
            (equal v (vconcat (number-sequence 32 127) (number-sequence 0 31)))))"#;
    assert_eq!(format_eval_result(&eval.eval_str(form)), "OK (145 nil nil)");
    let form = r#"(let (out)
      (dolist (size '(128 2048))
        (dolist (keyed '(nil t))
          (let ((v (vconcat (number-sequence (/ size 4) (1- size))
                           (number-sequence 0 (1- (/ size 4)))))
                (n 0) (stop (+ size (if (= size 128) 17 25))))
            (condition-case nil
                (sort v :in-place t :key (and keyed (lambda (x) x))
                      :lessp (lambda (a b) (setq n (1+ n))
                               (when (= n stop) (error "stop")) (< a b)))
              (error nil))
            (push (list size keyed n
                        (equal (sort v) (vconcat (number-sequence 0 (1- size))))) out))))
      (nreverse out))"#;
    assert_eq!(
        format_eval_result(&eval.eval_str(form)),
        "OK ((128 nil 145 nil) (128 t 145 t) (2048 nil 2073 t) (2048 t 2073 t))"
    );
}
