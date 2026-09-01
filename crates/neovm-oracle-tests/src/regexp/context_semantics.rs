//! Oracle parity tests for GNU regexp context helpers in `subr.el`.
//!
//! GNU implements `looking-back` with `re-search-backward` against an
//! end-anchored regexp, so it returns a boolean while leaving match data for
//! the suffix that matched.  `subregexp-context-p` reuses the GNU regexp parser
//! and interprets selected `invalid-regexp` messages as non-subregexp context.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

#[test]
fn oracle_prop_gnu_looking_back_match_data_and_limit() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(with-temp-buffer
  (insert "abc123abc")
  (goto-char 7)
  (list
   (looking-back "[a-z]+[0-9]+" nil)
   (match-string 0)
   (progn
     (goto-char 10)
     (looking-back "[a-z]+" 7))
   (match-string 0)
   (progn
     (goto-char 10)
     (looking-back "[a-z]+" 8))
   (match-string 0)))
"#;

    let expect = expect_test::expect![[r#""OK (t \"c123\" t \"c\" t \"c\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_gnu_looking_back_greedy_extends_match_data() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(with-temp-buffer
  (insert "foofoo")
  (goto-char (point-max))
  (list
   (looking-back "foo" nil)
   (match-string 0)
   (match-beginning 0)
   (match-end 0)
   (progn
     (goto-char (point-max))
     (looking-back "foo" nil t))
   (match-string 0)
   (match-beginning 0)
   (match-end 0)))
"#;

    let expect = expect_test::expect![[r#""OK (t \"foo\" 4 7 t \"foo\" 4 7)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_gnu_subregexp_context_parser_contracts() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (subregexp-context-p "abc" 1)
 (subregexp-context-p "[abc]" 2)
 (subregexp-context-p "a\\{2\\}" 3)
 (subregexp-context-p "a\\" 2)
 (subregexp-context-p "\\(a\\|b\\)" 0)
 (subregexp-context-p "\\(a\\|b\\)" 2)
 (subregexp-context-p "[abc" 4)
 (subregexp-context-p "a\\{2" 4)
 (subregexp-context-p "a\\(b" 4)
 (subregexp-context-p "a\\|b" 3)
 (subregexp-context-p "[[:alpha:]]" 4))
"#;

    let expect = expect_test::expect![[r#""OK (t nil nil nil t t nil nil t t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
