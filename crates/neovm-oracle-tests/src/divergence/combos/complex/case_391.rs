//! Complex combo batch 391 — `error`/`condition-case`/`unwind-protect`/
//! `catch-throw` ultimate: signal/error/user-error dispatch, nested
//! unwind cleanup ordering, define-error hierarchy, ignore-errors,
//! quit handling, no-handler propagation.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx391_error_hierarchy_chain_propagation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:caught-as-a (neo-cx391-c \"detail\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (define-error 'neo-cx391-a "A error")
      (define-error 'neo-cx391-b "B error" '(neo-cx391-a))
      (define-error 'neo-cx391-c "C error" '(neo-cx391-b))
      (condition-case inner
          (signal 'neo-cx391-c '("detail"))
        (neo-cx391-a (list :caught-as-a inner))))
  (error (list :outer (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx391_signal_vs_error_vs_user_error_dispatch() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (error \"via error fn\" :caught-user-error :caught-as-error)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list
 (condition-case e (signal 'error '("plain")) (error (car e)))
 (condition-case e (error "via error fn") (error (cadr e)))
 (condition-case e (signal 'user-error '("user")) (user-error :caught-user-error))
 (condition-case e (signal 'user-error '("user")) (error :caught-as-error)))
"##,
        expect,
    )
}

#[test]
fn div_cx391_unwind_protect_cleanup_with_error_in_cleanup() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:cleanup-start (:outer . error))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (trace)
  (condition-case outer
      (unwind-protect
          (error "primary")
        (push :cleanup-start trace)
        (error "cleanup")
        (push :cleanup-never trace))
    (error (push (cons :outer (car outer)) trace)))
  (nreverse trace))
"##,
        expect,
    )
}

#[test]
fn div_cx391_catch_throw_through_nested_unwind() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (trace)
  (prog1
      (catch 'done
        (unwind-protect
            (progn
              (push :body-start trace)
              (unwind-protect
                  (progn (push :inner-start trace) (throw 'done :caught))
                (push :inner-unwind trace))
              (push :outer-never trace))
          (push :outer-unwind trace)))
    (nreverse trace))
"##,
        expect,
    )
}

#[test]
fn div_cx391_ignore_errors_all_variants() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3 nil nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (ignore-errors (+ 1 2))
      (ignore-errors (error "boom"))
      (ignore-errors (signal 'wrong-type-argument '(stringp 5)))
      (ignore-errors (car 'x))
      (ignore-errors (aref "abc" 99))
      (ignore-errors (/ 1 0)))
"##,
        expect,
    )
}

#[test]
fn div_cx391_signal_nil_empty_data_and_quit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((error \"Invalid error symbol\" neo-cx391-err) (error \"Invalid error symbol\" neo-cx391-err) (error \"Invalid error symbol\" neo-cx391-err) :caught-quit (:quit-detail (quit \"detail\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list
 (condition-case e (signal 'neo-cx391-err nil) (error e))
 (condition-case e (signal 'neo-cx391-err '()) (error e))
 (condition-case e (signal 'neo-cx391-err '("single")) (error e))
 (condition-case e (signal 'quit nil) (quit :caught-quit) (error :caught-error))
 (condition-case e (signal 'quit '("detail")) (quit (list :quit-detail e))))
"##,
        expect,
    )
}

#[test]
fn div_cx391_with_demoted_errors_conversion() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (when (fboundp 'with-demoted-errors)
      (with-demoted-errors "Error: %S"
        (error "inner induced error")))
  (error (list :outer (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx391_condition_case_no_handler_match_propagates() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (:caught-outer (wrong-type-argument integerp \"x\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case outer
    (condition-case inner
        (signal 'wrong-type-argument '(integerp "x"))
      (file-error (list :file-handler inner)))
  (wrong-type-argument (list :caught-outer outer))
  (error (list :caught-other outer)))
"##,
        expect,
    )
}

#[test]
fn div_cx391_debug_on_error_with_condition_case() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:caught wrong-type-argument number-or-marker-p)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((debug-on-error t))
  (condition-case err
      (progn (+ 1 "x") :never)
    (error (list :caught (car err) (cadr err)))))
"##,
        expect,
    )
}

#[test]
fn div_cx391_error_chain_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (trace)
  (with-temp-buffer
    (buffer-enable-undo)
    (insert "Error chain ultimate mega test buffer content")
    (put-text-property 1 6 'face 'bold)
    (let ((m (set-marker (make-marker) 8))
          (ov (make-overlay 4 14)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (narrow-to-region 2 18)
      (condition-case err
          (unwind-protect
              (error "induced mega error")
            (push :unwind trace))
        (error (push (list :caught (car err) (cadr err)) trace)))
      (let ((state (list (nreverse trace)
                         (buffer-string)
                         (marker-position m)
                         (overlay-start ov) (overlay-end ov)
                         (text-properties-at 1))))
        (undo)
        (widen()
        (list state (buffer-string) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              (text-properties-at 1)))))))
"##,
        expect,
    )
}
