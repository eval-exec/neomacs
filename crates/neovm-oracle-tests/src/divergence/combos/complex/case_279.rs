//! Complex combo batch 279 — `format` / `format-spec` / `prin1` /
//! `read` with deeply shared/circular structures and dynamic field widths.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx279_format_dynamic_field_width() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (error \"Invalid format operation %*\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (format "%*d" 5 42)
      (format "%-*d|" 5 42)
      (format "%*s" 10 "hi")
      (format "%-*s|" 10 "hi")
      (format "%*.*f" 10 3 3.14159))
"##,
        expect,
    )
}

#[test]
fn div_cx279_format_spec_missing_and_extra() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function format-spec-make)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((spec (format-spec-make ?a "alpha" ?b "beta")))
  (list (format-spec "%a-%b" spec)
        (format-spec "%%literal" spec)
        (condition-case e (format-spec "%z-missing" spec) (error (car e)))
        (format-spec "%a %a %a" spec)
        (condition-case e (format-spec "%a-%b-%c" spec) (error (car e)))))
"##,
        expect,
    )
}

#[test]
fn div_cx279_prin1_print_circle_deeply_shared() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (\"(#1=(1 2 3) #1# #1#)\" \"((1 2 3) (1 2 3) (1 2 3))\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((inner (list 1 2 3))
       (shared (list inner inner inner)))
  (list (let ((print-circle t)) (prin1-to-string shared))
        (let ((print-circle nil))
          (condition-case e (prin1-to-string shared) (error (car e))))))
"##,
        expect,
    )
}

#[test]
fn div_cx279_prin1_print_circle_circular_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r##""OK (\"#1=(1 2 3 . #1#)\" \"(1 2 3 1 2 . #2)\")""##]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((circular (list 1 2 3)))
  (setcdr (cddr circular) circular)
  (list (let ((print-circle t)) (prin1-to-string circular))
        (let ((print-circle nil))
          (condition-case e (prin1-to-string circular) (error (car e))))))
"##,
        expect,
    )
}

#[test]
fn div_cx279_read_circle_shared_round_trip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"(#1=(1 2 3) #1#)\" t (1 2 3) (1 2 3))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((a (list 1 2 3))
       (data (list a a))
       (printed (let ((print-circle t)) (prin1-to-string data)))
       (read-back (let ((read-circle t)) (car (read-from-string printed)))))
  (list printed
        (eq (car read-back) (cadr read-back))
        (car read-back) (cadr read-back)))
"##,
        expect,
    )
}

#[test]
fn div_cx279_prin1_print_gensym_uninterned() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r##""OK (\"G-0\" \"#:G-0\" \"G-0\")""##]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((gs (gensym "G-")))
  (list (symbol-name gs)
        (let ((print-gensym t)) (prin1-to-string gs))
        (let ((print-gensym nil)) (prin1-to-string gs))))
"##,
        expect,
    )
}

#[test]
fn div_cx279_print_length_and_level_combined() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"((...) (1 2 3 ...))\" \"...\" \"(((((\\\"deep\\\")))) (1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30 31 32 33 34 35 36 37 38 39 40 41 42 43 44 45 46 47 48 49 50))\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((deep '(((("deep")))))
      (long (number-sequence 1 50)))
  (list (let ((print-length 3) (print-level 2))
          (prin1-to-string (list deep long)))
        (let ((print-length 0) (print-level 0))
          (prin1-to-string (list deep long)))
        (let ((print-length nil) (print-level nil))
          (prin1-to-string (list deep long)))))
"##,
        expect,
    )
}

#[test]
fn div_cx279_format_with_nan_and_inf() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"inf\" \"inf\" \"inf\" \"-nan\" \"inf\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (condition-case e (format "%f" (/ 1.0 0.0)) (error (cons :err (car e))))
      (condition-case e (format "%e" (/ 1.0 0.0)) (error (cons :err (car e))))
      (condition-case e (format "%g" (/ 1.0 0.0)) (error (cons :err (car e))))
      (condition-case e (format "%f" (/ 0.0 0.0)) (error (cons :err (car e))))
      (condition-case e (format "%d" (/ 1.0 0.0)) (error (cons :err (car e)))))
"##,
        expect,
    )
}

#[test]
fn div_cx279_format_with_bignum_and_ratio() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable 355/113)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((big (expt 2 128))
      (ratio 355/113))
  (list (format "%d" big)
        (format "%x" big)
        (format "%o" big)
        (format "%b" big)
        (format "%S" ratio)
        (format "%f" ratio)
        (format "%.10f" ratio)))
"##,
        expect,
    )
}

#[test]
fn div_cx279_format_read_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((shared (list 1 2 3))
       (data (list shared shared (list :a :b)))
       (printed (let ((print-circle t)) (prin1-to-string data))))
  (with-temp-buffer
    (buffer-enable-undo)
    (insert printed)
    (put-text-property 1 6 'face 'bold)
    (let ((m (set-marker (make-marker) 8))
          (ov (make-overlay 4 14)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (narrow-to-region 2 18)
      (let ((state (list printed
                         (car (read-from-string printed))
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
