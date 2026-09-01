//! Strict combo oracle probes, batch 196: obarray + symbol operations.
//! make-obarray, intern/intern-soft, mapatoms enumeration, unintern, obarrayp,
//! and symbol-name/symbol-function/symbol-value over a private obarray.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_obarray_intern_soft_mapatoms() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((ob (make-obarray 100)))
  (intern "probe-sym1" ob)
  (intern "probe-sym2" ob)
  (intern "probe-sym3" ob)
  (list (symbolp (intern-soft "probe-sym1" ob))
        (intern-soft "missing" ob)
        (let ((count 0))
          (mapatoms (lambda (s) (setq count (1+ count))) ob)
          count)
        (eq (intern "probe-sym1" ob) (intern "probe-sym1" ob))
        (eq (intern "probe-sym1" ob) (intern "probe-sym2" ob))
        (obarrayp ob)
        (obarrayp 'not-obarray)
        (obarrayp [0 0 0])))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function make-obarray)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_unintern_symbol_plist_reintern() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((ob (make-obarray 50)))
  (let ((s1 (intern "probe-key" ob)))
    (put s1 'prop 'val)
    (list (intern-soft "probe-key" ob)
          (get s1 'prop)
          (progn (unintern "probe-key" ob) (intern-soft "probe-key" ob))
          (let ((s2 (intern "probe-key" ob)))
            (eq s1 s2))
          (intern-soft "probe-key" ob))))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function make-obarray)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_global_obmap_default_obarray_intern_soft() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(progn
  (intern "probe-global-sym-xyz")
  (list (symbolp (intern-soft "probe-global-sym-xyz"))
        (eq (intern-soft "probe-global-sym-xyz")
            (intern "probe-global-sym-xyz"))
        (symbol-name (intern "probe-global-sym-xyz"))
        (intern-soft "probe-not-interned-anywhere")
        (unintern "probe-global-sym-xyz" obarray)
        (intern-soft "probe-global-sym-xyz")))
"##;
    let expect = expect_test::expect![[r#""OK (t t \"probe-global-sym-xyz\" nil t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
