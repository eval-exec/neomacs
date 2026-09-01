//! Divergence tests: character folding, char-fold search deep.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_char_fold() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable char-fold-symmetric)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'char-fold-to-regexp)
  (fboundp 'char-fold-make-table)
  (boundp 'char-fold-symmetric)
  (booleanp char-fold-symmetric)) "#,
        expect,
    );
}

#[test]
fn divergence_char_fold_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'char-fold-table)
  (fboundp 'char-fold-ascii-table)
  (fboundp 'search-char-fold-threshold)
  (boundp 'search-char-fold-threshold)) "#,
        expect,
    );
}

#[test]
fn divergence_isearch_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'isearch-forward)
  (fboundp 'isearch-backward)
  (fboundp 'isearch-forward-regexp)
  (fboundp 'isearch-backward-regexp)
  (boundp 'isearch-mode-map)
  (keymapp isearch-mode-map)) "#,
        expect,
    );
}

#[test]
fn divergence_isearch_vars() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (boundp 'search-slow-speed)
  (boundp 'search-slow-window-lines)
  (boundp 'search-whitespace-regexp)
  (stringp search-whitespace-regexp)
  (boundp 'search-upper-case)
  (stringp search-whitespace-regexp)) "#,
        expect,
    );
}

#[test]
fn divergence_occur_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'occur)
  (fboundp 'multi-occur)
  (fboundp 'multi-occur-in-matching-buffers)
  (fboundp 'how-many)
  (fboundp 'match-lines)) "#,
        expect,
    );
}

#[test]
fn divergence_keep_flush_lines() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'keep-lines)
  (fboundp 'flush-lines)
  (fboundp 'delete-matching-lines)
  (fboundp 'delete-non-matching-lines)) "#,
        expect,
    );
}

#[test]
fn divergence_replace_regex() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'replace-regexp)
  (fboundp 'replace-string)
  (fboundp 'query-replace)
  (fboundp 'query-replace-regexp)
  (boundp 'query-replace-defaults)
  (listp query-replace-defaults)) "#,
        expect,
    );
}

#[test]
fn divergence_replace_preserve_case() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (boundp 'case-replace)
  (booleanp case-replace)
  (boundp 'case-fold-search)
  (booleanp case-fold-search)) "#,
        expect,
    );
}

#[test]
fn divergence_lazy_highlight() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (boundp 'isearch-lazy-highlight)
  (booleanp isearch-lazy-highlight)
  (boundp 'lazy-highlight-initial-delay)
  (numberp lazy-highlight-initial-delay)
  (boundp 'lazy-highlight-interval)
  (numberp lazy-highlight-interval)) "#,
        expect,
    );
}

#[test]
fn divergence_regexp_opt() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'regexp-opt)
  (stringp (regexp-opt '("foo" "bar" "baz")))
  (fboundp 'regexp-opt-depth)
  (fboundp 'regexp-quote)
  (stringp (regexp-quote "hello.world"))
  (= (regexp-opt-depth (regexp-opt '("foo" "bar"))) 0)) "#,
        expect,
    );
}
