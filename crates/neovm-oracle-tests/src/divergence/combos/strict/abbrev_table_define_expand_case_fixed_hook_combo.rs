//! Strict combo oracle probes, batch 209: abbrev tables deep. make-abbrev-
//! table, define-abbrev with :case-fixed and :count, abbrev-symbol/
//! abbrev-expansion, expand-abbrev including hook invocation, and
//! clear-abbrev-table / abbrev-table-p.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_abbrev_table_define_symbol_expansion() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((table (make-abbrev-table)))
  (define-abbrev table "ab" "about")
  (define-abbrev table "btw" "by the way" nil :case-fixed t)
  (define-abbrev table "xp" "expand")
  (list (abbrev-table-p table)
        (abbrev-symbol "ab" table)
        (abbrev-symbol "missing" table)
        (abbrev-expansion "ab" table)
        (abbrev-expansion "btw" table)
        (abbrev-expansion "xp" table)))
"##;
    let expect = expect_test::expect![[r#""OK (t ab nil \"about\" \"by the way\" \"expand\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_expand_abbrev_case_fixed_hook_invocation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((table (make-abbrev-table))
      (hook-fired nil))
  (define-abbrev table "ab" "about")
  (define-abbrev table "hk" "hooked" (lambda () (push 'fired hook-fired)))
  (list (with-temp-buffer
          (setq-local local-abbrev-table table)
          (insert "ab")
          (expand-abbrev)
          (buffer-string))
        (with-temp-buffer
          (setq-local local-abbrev-table table)
          (insert "hk")
          (expand-abbrev)
          (buffer-string))
        hook-fired
        (with-temp-buffer
          (setq-local local-abbrev-table table)
          (insert "AB")
          (expand-abbrev)
          (buffer-string))))
"##;
    let expect = expect_test::expect![[r#""OK (\"about\" \"hooked\" (fired) \"ABOUT\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_clear_abbrev_table_count_and_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((table (make-abbrev-table)))
  (define-abbrev table "ab" "about" nil :count 5)
  (define-abbrev table "cd" "cooldown")
  (let ((count-before (abbrev-get (abbrev-symbol "ab" table) :count)))
    (list count-before
          (abbrev-expansion "cd" table)
          (progn (clear-abbrev-table table) (abbrev-symbol "ab" table))
          (abbrev-symbol "cd" table)
          (abbrev-table-p table))))
"##;
    let expect = expect_test::expect![[r#""OK (5 \"cooldown\" nil nil t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
