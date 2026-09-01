//! Strict combo oracle probes, batch 135: number radix operations, ratio/
//! decimal fraction, ascii/unibyte edge cases, cl-struct inheritance with
//! setf accessors, and pcase-lambda destructuring.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_u9_number_radix_and_fraction() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (format "%#o" 0)
      (format "%#x" 0)
      (format "%#o" 1)
      (format "%#x" 1)
      (format "%#X" 255)
      (format "%o" 0)
      (format "%x" 0)
      (+ 1/2 1/3)
      (* 2/3 3/4)
      (- 5/6 1/2)
      (/ 1/2)
      (denominator 6/9)
      (numerator 6/9)
      (= 1/2 0.5)
      (< 1/3 0.34)
      (numberp 1/2))
"##;
    let expect = expect_test::expect![[r#""ERR (void-variable 1/2)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_u9_ascii_and_unibyte_edge() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((u (string 65 66 67))
      (m (string 233 260 277))
      (raw (string-make-unibyte (string 200 201 202))))
  (list (unibyte-string 65)
        (multibyte-string-p u)
        (multibyte-string-p m)
        (multibyte-string-p raw)
        (enable-multibyte-characters)
        (string-as-unibyte m)
        (length (string-as-unibyte m))
        (aref raw 0)
        (aref (string-as-multibyte raw) 0)
        (string= u "ABC")
        (string= (string-as-unibyte u) "ABC")))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function enable-multibyte-characters)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_u9_cl_struct_inheritance_setf() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(progn
  (cl-defstruct (probe-inh-base (:constructor probe-inh-base-create (x))
                                (:copier nil))
    x)
  (cl-defstruct (probe-inh-child (:include probe-inh-base (x 99))
                                  (:constructor probe-inh-child-create (x y)))
    y)
  (let ((c (probe-inh-child-create 1 2)))
    (list (probe-inh-base-p c)
          (probe-inh-child-p c)
          (probe-inh-base-x c)
          (probe-inh-child-y c)
          (type-of c)
          (progn (setf (probe-inh-child-y c) 'changed)
                 (probe-inh-child-y c))
          (progn (setf (probe-inh-base-x c) 'also-changed)
                 (probe-inh-base-x c))
          (let ((b (probe-inh-base-create 42)))
            (list (probe-inh-base-p b)
                  (probe-inh-child-p b)
                  (probe-inh-base-x b)
                  (condition-case err (probe-inh-child-y b) (error (car err)))))))
"##;
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_u9_pcase_lambda_destructuring() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((fn (pcase-lambda (_ a (&optional b) (&rest c))
           (list a b c))))
  (list (funcall fn nil 1 nil)
        (funcall fn nil 1 2 nil)
        (funcall fn nil 1 2 '(3 4 5))))
"##;
    let expect = expect_test::expect![[
        r#""ERR (wrong-number-of-arguments (closure (t) (_ a arg0 arg1) (pcase-let* (((&optional b) arg0) ((&rest c) arg1)) (list a b c))) 3)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_u9_format_mode_line_with_state_change() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (rename-buffer "probe-fmt-ml")
  (insert "content")
  (let ((ml1 (format-mode-line "%b %* %m")))
    (setq buffer-read-only t)
    (let ((ml2 (format-mode-line "%b %* %m")))
      (narrow-to-region 1 4)
      (let ((ml3 (format-mode-line "%b %* %m %n")))
        (widen)
        (list ml1 ml2 ml3))))
"##;
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
