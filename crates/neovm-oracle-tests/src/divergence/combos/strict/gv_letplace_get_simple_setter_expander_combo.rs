//! Strict combo oracle probes, batch 243: gv (generalized variables).
//! gv-letplace over a place, gv-get place/expander, gv-define-simple-setter /
//! gv-define-setter, and let-alist generalized access.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_gv_letplace_place_get_setf_expansion() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'gv)
(let ((x (list 1 2 3))
      (h (make-hash-table)))
  (puthash 'k 10 h)
  (gv-letplace (getter setter) (car x)
    (funcall setter 99))
  (gv-letplace (getter setter) (gethash 'k h)
    (funcall setter (+ (funcall getter) 5)))
  (list x
        (gethash 'k h)
        (let ((v [10 20 30]))
          (gv-letplace (getter setter) (aref v 1)
            (funcall setter (* (funcall getter) 2)))
          v)))
"##;
    let expect = expect_test::expect![[r#""ERR (gv-invalid-place 1)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_let_alist_generalized_access() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'let-alist)
(let ((alist '((name . "alice") (age . 30) (nested . ((deep . "value"))))))
  (list (let-alist alist .name)
        (let-alist alist .age)
        (let-alist alist .nested.deep)
        (let-alist alist .missing)
        (let-alist (list (cons 'x 1) (cons 'y 2)) (list .x .y))))
"##;
    let expect = expect_test::expect![[r#""OK (\"alice\" 30 \"value\" nil (1 2))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_gv_define_simple_setter_macro_place() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'gv)
(progn
  (defvar probe-gv-store nil)
  (gv-define-simple-setter probe-gv-setter (lambda (v) (setq probe-gv-store v)))
  (let ((probe-gv-store 'init))
    (setf (probe-gv-setter) 'newval)
    (list probe-gv-store)))
"##;
    let expect = expect_test::expect![[r#""OK (newval)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
