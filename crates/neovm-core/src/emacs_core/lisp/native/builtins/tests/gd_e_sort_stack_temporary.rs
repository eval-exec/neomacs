//! Consumed values in GNU's small merge array remain conservative GC roots.
use crate::emacs_core::format_eval_result;

#[test]
fn sort_small_merge_keeps_consumed_stack_values_until_return() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::runtime_startup_context();
    let form = r#"(let* ((v (vconcat
                     (mapcar (lambda (x) (cons x nil)) (number-sequence 1 127 2))
                     (mapcar (lambda (x) (cons x nil)) (number-sequence 0 126 2))))
                  (h (make-hash-table :test 'eq :weakness 'key))
                  (n 0) (seen nil))
      (dotimes (i (length v)) (puthash (aref v i) t h))
      (condition-case nil
          (sort v :in-place t :lessp (lambda (a b)
            (setq n (1+ n))
            (when (and (= (car a) 2) (= (car b) 3))
              (dotimes (i (length v))
                (when (eq (car-safe (aref v i)) 1) (aset v i nil)))
              (garbage-collect)
              (push (hash-table-count h) seen)
              (error "stop"))
            (< (car a) (car b))))
        (error nil))
      (list n seen (aref v 0) (aref v 1)))"#;
    assert_eq!(
        format_eval_result(&eval.eval_str(form)),
        "OK (131 (128) (0) nil)"
    );
}
