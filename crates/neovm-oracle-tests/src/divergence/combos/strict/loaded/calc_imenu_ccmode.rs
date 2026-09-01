//! Strict combo oracle probes, batch 48: math/index/language loaded libraries
//! via assert_oracle_parity_with_load — calc/calc.el (calc-eval arithmetic/
//! sqrt/exact fractions), imenu.el (imenu--make-index-alist over elisp), and
//! progmodes/cc-mode.el (C indentation of a region).
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity_with_load;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_i5_calc_eval_arithmetic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"20\" \"4\" \"1024\" \"5.66666666667\" \"6.66666666667\" \"100\")""#
    ]];
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(list (calc-eval "(2+3)*4")
      (calc-eval "sqrt(16)")
      (calc-eval "2^10")
      (calc-eval "17 / 3")
      (calc-eval "17 / 3 + 1")
      (calc-eval "100! / 99!"))
"##,
        &["calc/calc.el"],
        expect,
    );
}

#[test]
fn div_i5_calc_eval_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"0\" \"1.\" \"6\" \"30 choose\" \"fib(10)\")""#]];
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(list (calc-eval "sin(0)")
      (calc-eval "ln(exp(1))")
      (calc-eval "gcd(12, 18)")
      (calc-eval "10 choose 3")
      (calc-eval "fib(10)"))
"##,
        &["calc/calc.el"],
        expect,
    );
}

#[test]
fn div_i5_imenu_index_elisp() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"*Rescan*\" \"Variables\" \"bar\" \"foo\")""#]];
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(with-temp-buffer
  (emacs-lisp-mode)
  (insert "(defun foo ())\n(defun bar ())\n(defvar baz 1)\n")
  (let ((index (imenu--make-index-alist t)))
    (sort (mapcar (lambda (e) (if (consp e) (car e) e)) index) #'string<)))
"##,
        &["imenu.el"],
        expect,
    );
}

#[test]
fn div_i5_cc_mode_indent_region() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"int main() {\\n  if (x) {\\n    return 0;\\n  }\\n}\\n\"""#
    ]];
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(with-temp-buffer
  (c-mode)
  (insert "int main() {\nif (x) {\nreturn 0;\n}\n}\n")
  (indent-region (point-min) (point-max))
  (buffer-string))
"##,
        &["progmodes/cc-mode.el", "progmodes/cc-engine.el"],
        expect,
    );
}
