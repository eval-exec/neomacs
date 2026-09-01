//! Strict combo oracle probes, batch 171: custom syntax tables. make-syntax-
//! table + modify-syntax-entry (word/prefix/string-quote chars), char-syntax
//! lookups in the custom table, skip-syntax-forward traversal, and
//! forward-sexp / forward-word / parse-partial-sexp behavior under a custom
//! syntax table (e.g. underscore-as-word, apostrophe-as-string-quote).
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_syntax_table_custom_modify_char_syntax_lookup() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((st (make-syntax-table)))
  (modify-syntax-entry ?_ "w" st)
  (modify-syntax-entry ?' "\"" st)
  (modify-syntax-entry ?# "'" st)
  (modify-syntax-entry ?@ "." st)
  (modify-syntax-entry ?< "(>" st)
  (modify-syntax-entry ?> ")<" st)
  (list (char-syntax ?_ st)
        (char-syntax ?' st)
        (char-syntax ?# st)
        (char-syntax ?@ st)
        (char-syntax ?< st)
        (char-syntax ?> st)
        (char-syntax ?a st)
        (char-syntax ?  st)))
"##;
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments char-syntax 2)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_syntax_table_custom_traversal_forward_sexp() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((st (make-syntax-table)))
  (modify-syntax-entry ?_ "w" st)
  (with-temp-buffer
    (set-syntax-table st)
    (insert "hello_world rest 'str val'")
    (goto-char 1)
    (let ((after-word (progn (skip-syntax-forward "w_") (point)))
          (after-space (progn (skip-syntax-forward " ") (point)))
          (sexp-pos (progn (goto-char 1) (forward-word) (point)))
          (parse-state (parse-partial-sexp 1 (point-max))))
      (list after-word
            after-space
            sexp-pos
            (nth 0 parse-state)
            (nth 3 parse-state)))))
"##;
    let expect = expect_test::expect![[r#""OK (12 13 12 0 nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_syntax_table_delim_pairs_comment_class() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((st (make-syntax-table)))
  (modify-syntax-entry ?{ "(}" st)
  (modify-syntax-entry ?} "){" st)
  (modify-syntax-entry ?/ ". 12" st)   ;; comment start (/) + second char (/)
  (modify-syntax-entry ?* ". 23" st)   ;; comment end (* /) + second
  (with-temp-buffer
    (set-syntax-table st)
    (insert "{ a ( b ) c }")
    (goto-char 1)
    (let ((balance (progn (forward-list) (point)))
          (depth-state (parse-partial-sexp 1 11)))
      (list (char-syntax ?{ st)
            (char-syntax ?} st)
            balance
            (nth 0 depth-state)
            (scan-lists 1 1 0)
            (scan-lists 11 -1 0)))))
"##;
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments char-syntax 2)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
