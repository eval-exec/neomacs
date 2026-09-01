//! Strict combo oracle probes, batch 349: cl-do-symbols + mapatoms over private
//! obarray. cl-do-symbols enumeration with COUNT, mapatoms over private obarray,
//! and obarrayp / obarray internals.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_cl_do_symbols_private_obarray() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'cl-lib)
(let ((ob (make-obarray 100))
      (collected nil))
  (intern "probe-a" ob)
  (intern "probe-b" ob)
  (intern "probe-c" ob)
  (cl-do-symbols (s 0 ob) (push s collected))
  (sort collected #'string<))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function make-obarray)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_mapatoms_collect_sort_private_obarray() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((ob (make-obarray 50))
      (collected nil))
  (intern "x" ob)
  (intern "y" ob)
  (intern "z" ob)
  (mapatoms (lambda (s) (push s collected)) ob)
  (sort (mapcar #'symbol-name collected) #'string<))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function make-obarray)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_cl_do_all_symbols_count_subset() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'cl-lib)
(let ((count 0))
  (cl-do-all-symbols (s)
    (when (eq s 'car) (setq count (1+ count))))
  (list (> count 0)
        (let ((priv-count 0))
          (let ((ob (make-obarray 20)))
            (intern "only-here" ob)
            (cl-do-symbols (s 0 ob) (setq priv-count (1+ priv-count))))
          priv-count)))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function make-obarray)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
