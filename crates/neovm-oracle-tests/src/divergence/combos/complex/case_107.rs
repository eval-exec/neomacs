//! Complex combo batch 107 — generator / coroutine / iterator patterns
//! with `iter-yield`, `iter-next`, lazy sequence generation, and infinite
//! sequences.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx107_generator_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'generator)
      (list (fboundp 'iter-yield)
            (fboundp 'iter-next)
            (fboundp 'iter-close)
            (fboundp 'lambda-iter)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx107_generator_basic_iteration() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'generator)
      (let ((iter (let ((lexical-binding t))
                    (lambda-iter
                      (dotimes (i 5)
                        (iter-yield i))))))
        (list (iter-next iter)
              (iter-next iter)
              (iter-next iter)
              (iter-next iter)
              (iter-next iter))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx107_generator_returns_stop() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'generator)
      (let ((iter (let ((lexical-binding t))
                    (lambda-iter
                      (iter-yield :a)
                      (iter-yield :b)))))
        (iter-next iter)
        (iter-next iter)
        (iter-next iter)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx107_generator_stateful_accumulator() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'generator)
      (let ((iter (let ((lexical-binding t))
                    (lambda-iter
                      (let ((acc 0))
                        (while t
                          (cl-incf acc)
                          (iter-yield acc)))))))
        (list (iter-next iter)
              (iter-next iter)
              (iter-next iter))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx107_generator_collect_to_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'generator)
      (let ((iter (let ((lexical-binding t))
                    (lambda-iter
                      (dotimes (i 5)
                        (iter-yield (* i i)))))))
        (cl-loop for x = (iter-next iter)
                 until (eq x 'iter-stop)
                 collect x)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx107_lazy_filter_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'generator)
      (let* ((numbers (let ((lexical-binding t))
                        (lambda-iter
                          (dotimes (i 20)
                            (iter-yield i))))))
        (let ((results nil))
          (cl-loop for x = (iter-next numbers)
                   until (eq x 'iter-stop)
                   when (and (cl-evenp x) (> x 5))
                   do (push x results))
          (nreverse results))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx107_generator_with_closure_capture() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'generator)
      (let ((lexical-binding t)
            (multiplier 10))
        (let ((iter (lambda-iter
                       (dolist (x '(1 2 3))
                         (iter-yield (* x multiplier))))))
          (list (iter-next iter)
                (iter-next iter)
                (iter-next iter)))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx107_iter_close_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function lambda-iter)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'generator)
      (let ((iter (let ((lexical-binding t))
                    (lambda-iter
                      (iter-yield :first)
                      (iter-yield :second)
                      (iter-yield :third))))))
        (iter-next iter)
        (iter-close iter)
        (iter-next iter)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx107_generator_recurse_via_yield_from() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function lambda-iter)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'generator)
      (let ((lexical-binding t))
        (let ((iter (lambda-iter
                       (dolist (lst '((a b) (c d) (e f g)))
                         (dolist (x lst)
                           (iter-yield x)))))))
          (list (iter-next iter)
                (iter-next iter)
                (iter-next iter)
                (iter-next iter)
                (iter-next iter)
                (iter-next iter)))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx107_iter_defun_named_generator() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 10 20)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'generator)
      (iter-defun neo-cx107-gen (n)
        (dotimes (i n)
          (iter-yield (* i 10))))
      (let ((iter (neo-cx107-gen 3)))
        (list (iter-next iter)
              (iter-next iter)
              (iter-next iter))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx107_generator_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'generator)
      (let ((iter (let ((lexical-binding t))
                    (lambda-iter
                      (dotimes (i 5)
                        (iter-yield i))))))
        (with-temp-buffer
          (buffer-enable-undo)
          (insert "Generator test buffer content")
          (put-text-property 1 9 'face 'bold)
          (let ((m (set-marker (make-marker) 8))
                (ov (make-overlay 4 14)))
            (overlay-put ov 'face 'italic)
            (overlay-put ov 'evaporate t)
            (narrow-to-region 2 18)
            (let ((v1 (iter-next iter))
                  (v2 (iter-next iter))
                  (v3 (iter-next iter)))
              (let ((state (list v1 v2 v3
                                 (buffer-string)
                                 (marker-position m)
                                 (overlay-start ov) (overlay-end ov)
                                 (text-properties-at 1))))
                (undo)
                (widen)
                (list state (buffer-string) (marker-position m)
                      (overlay-start ov) (overlay-end ov)
                      (text-properties-at 1))))))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx107_iter_lambda_idiomatic_pipeline() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'generator)
      (let ((lexical-binding t))
        (let ((infinite-counter
               (lambda-iter
                 (let ((n 0))
                   (while t
                     (cl-incf n)
                     (iter-yield n))))))
          (cl-loop for i from 0 below 5
                   collect (iter-next infinite-counter)))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}
