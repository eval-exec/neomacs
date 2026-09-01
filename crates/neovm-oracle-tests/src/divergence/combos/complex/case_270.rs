//! Complex combo batch 270 — `condition-case` with `debug` condition,
//! `signal` vs `error` vs `user-error` dispatch matrix, `quit` handling,
//! `with-demoted-errors` actual error conversion.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx270_condition_case_debug_condition() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (:caught-error (wrong-type-argument integerp \"x\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (signal 'wrong-type-argument '(integerp "x"))
  (debug (list :caught-debug e))
  (error (list :caught-error e)))
"##,
        expect,
    )
}

#[test]
fn div_cx270_signal_vs_error_vs_user_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (error \"via error fn\" :caught-user-error :caught-as-error)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list
 (condition-case e (signal 'error '("plain error")) (error (car e)))
 (condition-case e (error "via error fn") (error (cadr e)))
 (condition-case e (signal 'user-error '("user error")) (user-error :caught-user-error))
 (condition-case e (signal 'user-error '("user error")) (error :caught-as-error)))
"##,
        expect,
    )
}

#[test]
fn div_cx270_quit_signal_handling() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:caught-quit (:quit-detail (quit \"detail\")))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list
 (condition-case e (signal 'quit nil) (quit :caught-quit) (error :caught-error))
 (condition-case e (signal 'quit '("detail")) (quit (list :quit-detail e))))
"##,
        expect,
    )
}

#[test]
fn div_cx270_with_demoted_errors_conversion() {
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
fn div_cx270_error_hierarchy_chain_propagation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:caught-as-a (neo-cx270-c \"detail\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (define-error 'neo-cx270-a "A error")
      (define-error 'neo-cx270-b "B error" '(neo-cx270-a))
      (define-error 'neo-cx270-c "C error" '(neo-cx270-b))
      (condition-case inner
          (signal 'neo-cx270-c '("detail"))
        (neo-cx270-a (list :caught-as-a inner))))
  (error (list :outer (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx270_nested_unwind_protect_catch_throw_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (:body-start :inner-start :inner-unwind :outer-unwind)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (trace)
  (catch 'done
    (unwind-protect
        (progn
          (push :body-start trace)
          (unwind-protect
              (progn
                (push :inner-start trace)
                (throw 'done :caught)
                (push :inner-never trace))
            (push :inner-unwind trace))
          (push :outer-never trace))
      (push :outer-unwind trace))
    (push :after-never trace))
  (nreverse trace))
"##,
        expect,
    )
}

#[test]
fn div_cx270_error_in_unwind_cleanup_caught_by_outer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:cleanup-start (:outer-caught . error))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (trace)
  (condition-case outer
      (unwind-protect
          (error "primary")
        (push :cleanup-start trace)
        (error "cleanup")
        (push :cleanup-never trace))
    (error
     (push (cons :outer-caught (car outer)) trace)))
  (nreverse trace))
"##,
        expect,
    )
}

#[test]
fn div_cx270_condition_case_no_handler_match() {
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
fn div_cx270_signal_with_complex_error_data() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((file-error \"file\" \"/path\" \"detail\") (file-error \"file\") (file-error \"file\" nil nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list
 (condition-case e (signal 'file-error '("file" "/path" "detail")) (error e))
 (condition-case e (signal 'file-error '("file")) (error e))
 (condition-case e (signal 'file-error '("file" nil nil)) (error e)))
"##,
        expect,
    )
}

#[test]
fn div_cx270_error_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (trace)
  (with-temp-buffer
    (buffer-enable-undo)
    (insert "Error/signal mega test buffer content")
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
        (widen)
        (list state (buffer-string) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              (text-properties-at 1)))))))
"##,
        expect,
    )
}
