//! Divergence tests: treesit, tree-sitter integration deep.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_treesit_available() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (featurep 'treesit)
  (fboundp 'treesit-available-p)
  (fboundp 'treesit-language-available-p))"#,
        expect,
    );
}

#[test]
fn divergence_treesit_parser() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'treesit-parser-create)
  (fboundp 'treesit-parser-delete)
  (fboundp 'treesit-parser-root-node)
  (fboundp 'treesit-parse-string))"#,
        expect,
    );
}

#[test]
fn divergence_treesit_node() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t nil t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'treesit-node-type)
  (fboundp 'treesit-node-text)
  (fboundp 'treesit-node-start)
  (fboundp 'treesit-node-end)
  (fboundp 'treesit-node-parent))"#,
        expect,
    );
}

#[test]
fn divergence_treesit_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'treesit-query-compile)
  (fboundp 'treesit-query-capture)
  (fboundp 'treesit-query-string))"#,
        expect,
    );
}

#[test]
fn divergence_treesit_indent() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'treesit-simple-indent-rules)
  (fboundp 'treesit-indent)
  (fboundp 'treesit-check-indent))"#,
        expect,
    );
}

#[test]
fn divergence_treesit_fontify() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'treesit-font-lock-rules)
  (fboundp 'treesit-font-lock-feature-list))"#,
        expect,
    );
}

#[test]
fn divergence_treesit_search() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'treesit-search-subtree)
  (fboundp 'treesit-search-forward)
  (fboundp 'treesit-search-forward-goto))"#,
        expect,
    );
}

#[test]
fn divergence_treesit_thing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'treesit-thing-at-point)
  (fboundp 'treesit-nav-start-of-name)
  (fboundp 'treesit-nav-end-of-name))"#,
        expect,
    );
}

#[test]
fn divergence_treesit_transpose() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'treesit-transpose-sexps)
  (fboundp 'treesit-forward-sexp)
  (fboundp 'treesit-backward-sexp))"#,
        expect,
    );
}

#[test]
fn divergence_treesit_inspect() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'treesit-explore-mode)
  (fboundp 'treesit-inspect-mode)
  (fboundp 'treesit-node-check))"#,
        expect,
    );
}
