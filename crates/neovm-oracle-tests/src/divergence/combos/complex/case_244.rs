//! Complex combo batch 244 — `error` / `signal` / `condition-case` /
//! `unwind-protect` / `define-error` hierarchy / `debug-on-error` /
//! `with-demoted-errors` / `ignore-errors` deep with nesting.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx244_define_error_hierarchy_multiple_parents() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (:caught-as-base (neo-cx244-leaf-error \"detail\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (define-error 'neo-cx244-base-error "Base error")
      (define-error 'neo-cx244-mid-error "Mid error" '(neo-cx244-base-error))
      (define-error 'neo-cx244-leaf-error "Leaf error" '(neo-cx244-mid-error))
      (condition-case err
          (signal 'neo-cx244-leaf-error '("detail"))
        (neo-cx244-base-error (list :caught-as-base err))
        (neo-cx244-mid-error (list :caught-as-mid err))
        (neo-cx244-leaf-error (list :caught-as-leaf err))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx244_condition_case_error_data_extraction() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((wrong-type-argument integerp \"x\") (args-out-of-range 5 0 3) (error \"simple: msg\") (file-error \"file\" \"/path\") (file-error \"file\" \"/path\") (void-variable \"undef-var\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list
 (condition-case e (signal 'wrong-type-argument '(integerp "x")) (error e))
 (condition-case e (signal 'args-out-of-range '(5 0 3)) (error e))
 (condition-case e (error "simple: %s" "msg") (error e))
 (condition-case e (signal 'file-error '("file" "/path")) (error e))
 (condition-case e (signal 'file-error '("file" "/path")) (file-error e))
 (condition-case e (signal 'void-variable '("undef-var")) (error e)))
"##,
        expect,
    );
}

#[test]
fn div_cx244_unwind_protect_nested_error_in_cleanup() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:cleanup-enter (:outer . error))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (trace)
  (condition-case outer
      (unwind-protect
          (error "primary error")
        (push :cleanup-enter trace)
        (error "cleanup error")
        (push :cleanup-after-error trace))
    (error (push (cons :outer (car outer)) trace)))
  (nreverse trace))
"##,
        expect,
    )
}

#[test]
fn div_cx244_catch_throw_through_nested_unwind() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:before-throw :unwind :end)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (trace)
  (prog1
      (catch 'tag
        (unwind-protect
            (progn
              (push :before-throw trace)
              (throw 'tag :caught)
              (push :never trace))
          (push :unwind trace))
        (push :after-unwind trace)
        :never-reached)
    (push :end trace))
  (nreverse trace))
"##,
        expect,
    )
}

#[test]
fn div_cx244_with_demoted_errors_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'with-demoted-errors)
          (when (fboundp 'with-demoted-errors)
            (with-demoted-errors "Demoted: %S"
              (error "inner error"))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx244_ignore_errors_variants() {
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
fn div_cx244_signal_nil_and_empty_data() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((error \"Invalid error symbol\" neo-cx244-err) (error \"Invalid error symbol\" neo-cx244-err) (error \"Invalid error symbol\" neo-cx244-err) (error \"Invalid error symbol\" neo-cx244-err))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list
 (condition-case e (signal 'neo-cx244-err nil) (error e))
 (condition-case e (signal 'neo-cx244-err '()) (error e))
 (condition-case e (signal 'neo-cx244-err '("single")) (error e))
 (condition-case e (signal 'neo-cx244-err '("a" "b" "c")) (error e)))
"##,
        expect,
    )
}

#[test]
fn div_cx244_debug_on_error_with_condition_case() {
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
fn div_cx244_user_error_vs_error_dispatch() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:caught-user-error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (define-error 'neo-cx244-user-error "User error" '(user-error))
      (signal 'neo-cx244-user-error '("detail")))
  (user-error (list :caught-user-error))
  (error (list :caught-error)))
"##,
        expect,
    )
}

#[test]
fn div_cx244_error_chain_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (trace)
  (with-temp-buffer
    (buffer-enable-undo)
    (insert "Error chain mega test buffer content")
    (put-text-property 1 6 'face 'bold)
    (let ((m (set-marker (make-marker) 8))
          (ov (make-overlay 4 14)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (narrow-to-region 2 18)
      (condition-case err
          (unwind-protect
              (error "induced error")
            (push :unwind trace))
        (error
         (push (list :caught (car err)) trace)))
      (let ((state (list (nreverse trace)
                         (buffer-string)
                         (marker-position m)
                         (overlay-start ov) (overlay-end ov)
                         (text-properties-at 1))))
        (undo)
        (widen)
        (list state (buffer-string) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              (text-properties-at 1))))))
"##,
        expect,
    )
}
