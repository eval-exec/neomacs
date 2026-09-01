//! Strict combo oracle probes, batch 350: with-syntax-table scoped changes.
//! with-syntax-table restoring default syntax, char-syntax under custom table,
//! forward-word / skip-syntax-forward under scoped table.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_with_syntax_table_scoped_char_syntax() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((default-syntax (char-syntax ?_))
      (custom-st (make-syntax-table)))
  (modify-syntax-entry ?_ "w" custom-st)
  (list (char-syntax ?_)
        (with-syntax-table custom-st (char-syntax ?_))
        (char-syntax ?_)   ;; restored
        default-syntax))
"##;
    let expect = expect_test::expect![[r#""OK (95 119 95 95)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_with_syntax_table_forward_word_scoped() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (insert "hello_world test_case")
  (let ((default-result (progn (goto-char 1) (skip-syntax-forward "w_") (point))))
    (let ((custom-st (make-syntax-table)))
      (modify-syntax-entry ?_ "_" custom-st)  ;; underscore NOT word
      (with-syntax-table custom-st
        (goto-char 1)
        (skip-syntax-forward "w"))
      (list default-result
            (with-syntax-table custom-st
              (goto-char 1) (skip-syntax-forward "w") (point)))))))
"##;
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 12 67)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
