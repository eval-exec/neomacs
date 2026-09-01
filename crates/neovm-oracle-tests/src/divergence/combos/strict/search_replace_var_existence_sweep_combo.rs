//! Strict combo oracle probes, batch 250: search/replace/isearch variable
//! existence sweep. boundp over standard search/replace/isearch defcustoms and
//! ring variables -- any nil-in-Neomacs/t-in-GNU is a missing-variable bug
//! (same class as the search-spaces-regexp void divergence from batch 248).
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_search_replace_var_existence_boundp_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'search-spaces-regexp)
      (boundp 'search-upper-case)
      (boundp 'search-default-mode)
      (boundp 'search-invisible)
      (boundp 'search-exit-option)
      (boundp 'words-include-escapes)
      (boundp 'replace-char-function)
      (boundp 'replace-search-function)
      (boundp 'replace-re-search-function)
      (boundp 'replace-lax-whitespace)
      (boundp 'replace-regexp-lax-whitespace)
      (boundp 'query-replace-skip-read-only)
      (boundp 'query-replace-show-replacement)
      (boundp 'query-replace-to-history-variable)
      (boundp 'isearch-lax-whitespace)
      (boundp 'isearch-regexp-lax-whitespace)
      (boundp 'regexp-search-ring)
      (boundp 'regexp-search-ring-max)
      (boundp 'search-ring)
      (boundp 'search-ring-max)
      (boundp 'isearch-word)
      (boundp 'lazy-highlight-buffer))
"##;
    let expect = expect_test::expect![[r#""OK (t t t t t t nil t t t t t t t t t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_isearch_facility_var_existence_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'isearch-mode)
      (boundp 'isearch-forward)
      (boundp 'isearch-string)
      (boundp 'isearch-regexp)
      (boundp 'isearch-case-fold-search)
      (boundp 'isearch-lazy-highlight)
      (boundp 'isearch-search-fun-function)
      (boundp 'isearch-filter-predicate)
      (boundp 'isearch-wrap-function)
      (boundp 'search-highlight-submatches)
      (boundp 'char-fold-include)
      (boundp 'char-fold-exclude)
      (boundp 'search-whitespace-regexp))
"##;
    let expect = expect_test::expect![[r#""OK (t t t t t t t t t t nil nil t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_search_spaces_regexp_defined_default_value() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'search-spaces-regexp)
      (default-boundp 'search-spaces-regexp)
      (and (boundp 'search-spaces-regexp) (default-value 'search-spaces-regexp))
      (boundp 'search-whitespace-regexp)
      (and (boundp 'search-whitespace-regexp) (default-value 'search-whitespace-regexp)))
"##;
    let expect = expect_test::expect![[r#""OK (t t nil t \"[ \t]+\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
