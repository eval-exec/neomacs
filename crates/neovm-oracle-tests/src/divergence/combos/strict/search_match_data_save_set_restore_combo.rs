//! Strict combo oracle probes, batch 190: regexp search + match-data
//! management. re-search-forward accumulation across multiple matches, multi-
//! group match-data vectors, save-match-data isolation, set-match-data
//! restore, and match-substitute-replacement.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_re_search_forward_accumulate_multi_group() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (insert "foo1 bar2 baz3 foo4 bar5")
  (goto-char 1)
  (let (results)
    (while (re-search-forward "\\(\\w+\\)\\([0-9]\\)" nil t)
      (push (list (match-beginning 0) (match-end 0)
                  (match-beginning 1) (match-end 1)
                  (match-string 1) (match-string 2))
            results))
    (nreverse results)))
"##;
    let expect = expect_test::expect![[
        r#""OK ((1 5 1 4 \"foo\" \"1\") (6 10 6 9 \"bar\" \"2\") (11 15 11 14 \"baz\" \"3\") (16 20 16 19 \"foo\" \"4\") (21 25 21 24 \"bar\" \"5\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_match_data_save_set_restore_isolation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(progn
  (string-match "a\\(.\\)c" "abc aXc")
  (let ((outer (match-data))
        (inner-result (save-match-data
                        (string-match "x\\(.\\)z" "xyz xWz")
                        (list (match-data) (match-string 1 "xyz xWz")))))
    (list outer
          (match-string 1 "abc aXc")
          inner-result
          (match-data)
          (progn (set-match-data nil) (match-data))
          (progn (set-match-data outer) (match-string 1 "abc aXc")))))
"##;
    let expect =
        expect_test::expect![[r#""OK ((0 3 1 2) \"b\" ((0 3 1 2) \"y\") (0 3 1 2) nil \"b\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_re_search_backward_match_substitute_replacement() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (insert "a1 b2 c3 d4 e5")
  (goto-char (point-max))
  (let ((backward-hit (re-search-backward "\\([a-z]\\)\\([0-9]\\)" nil t)))
    (list backward-hit
          (match-string 0)
          (match-string 1)
          (match-substitute-replacement "[\\1\\2]"))))
"##;
    let expect = expect_test::expect![[r#""OK (13 \"e5\" \"e\" \"[e5]\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
