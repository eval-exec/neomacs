//! Completion subsystem divergence probes (calibration).
//!
//! Probes try-completion / all-completions / test-completion with list, alist,
//! and obarray collections, predicates, completion-ignore-case, and the
//! completion-styles (basic / partial / substring / flex / initials), plus
//! completion-boundaries. Self-contained, deterministic.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_comp_try_completion_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"ab\" \"ab\" t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (try-completion "a" '("abc" "abd" "xyz"))
      (try-completion "ab" '("abc" "abd"))
      (try-completion "abc" '("abc" "abd")))
"##,
        expect,
    );
}

#[test]
fn div_comp_try_completion_nomatch() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (try-completion "q" '("abc" "abd"))
      (try-completion "abc" '("abc" "abd") (lambda (x) nil)))
"##,
        expect,
    );
}

#[test]
fn div_comp_all_completions_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((\"abc\" \"abd\" \"axy\") 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (all-completions "a" '("abc" "abd" "axy" "xyz"))
      (length (all-completions "" '("abc" "abd"))))
"##,
        expect,
    );
}

#[test]
fn div_comp_test_completion_exact() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (test-completion "abc" '("abc" "abd"))
      (test-completion "ab" '("abc" "abd")))
"##,
        expect,
    );
}

#[test]
fn div_comp_alist_collection() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"alpha\" (\"beta\") t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((coll '(("alpha" . 1) ("beta" . 2) ("gamma" . 3))))
  (list (try-completion "a" coll)
        (all-completions "b" coll)
        (test-completion "alpha" coll)))
"##,
        expect,
    );
}

#[test]
fn div_comp_obarray_collection() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function obarray)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ob (obarray)))
  (intern "foobar" ob)
  (intern "foobaz" ob)
  (intern "quux" ob)
  (list (try-completion "foo" ob)
        (all-completions "foo" ob)))
"##,
        expect,
    );
}

#[test]
fn div_comp_predicate_filter() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil \"abx\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((coll '("abc" "abd" "abx" "aby")))
  (list (all-completions "a" coll (lambda (s) (> (length s) 3)))
        (try-completion "a" coll (lambda (s) (string-match-p "x" s)))))
"##,
        expect,
    );
}

#[test]
fn div_comp_ignore_case() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"AB\" (\"apple\" \"APPLE\" \"apricot\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((completion-ignore-case t))
  (list (try-completion "A" '("abc" "ABC" "abd"))
        (all-completions "A" '("apple" "APPLE" "apricot"))))
"##,
        expect,
    );
}

#[test]
fn div_comp_style_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"ab\" (\"abc\" \"abd\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((completion-styles '(basic)))
  (list (try-completion "ab" '("abc" "abd" "aBd"))
        (all-completions "ab" '("abc" "abd" "aBd"))))
"##,
        expect,
    );
}

#[test]
fn div_comp_style_partial() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((completion-styles '(partial)))
  (all-completions "b" '("abc" "xab" "abx")))
"##,
        expect,
    );
}

#[test]
fn div_comp_style_substring() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((completion-styles '(substring)))
  (all-completions "bc" '("abcd" "xbcd" "xyz")))
"##,
        expect,
    );
}

#[test]
fn div_comp_style_flex() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((completion-styles '(flex)))
  (all-completions "abc" '("aXbYc" "axbyc" "xyz" "axb")))
"##,
        expect,
    );
}

#[test]
fn div_comp_style_initials() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((completion-styles '(initials)))
  (all-completions "abc" '("a-b-c" "another-big-claim" "axbyc")))
"##,
        expect,
    );
}

#[test]
fn div_comp_styles_fallthrough() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"abcdef\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((completion-styles '(basic flex)))
  (all-completions "abc" '("abcdef" "aXbYcZ")))
"##,
        expect,
    );
}

#[test]
fn div_comp_completion_boundaries() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 . 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((coll '("alpha" "beta")))
  (completion-boundaries "al" coll nil "pha"))
"##,
        expect,
    );
}

#[test]
fn div_comp_hash_table_collection() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"alpha\" (\"alpha\" \"beta\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ht (make-hash-table :test 'equal)))
  (puthash "alpha" 1 ht)
  (puthash "beta" 2 ht)
  (list (try-completion "a" ht)
        (sort (all-completions "" ht) #'string<)))
"##,
        expect,
    );
}

#[test]
fn div_comp_uniquify_common_prefix() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"apple-\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(try-completion "a" '("apple-pie" "apple-sauce" "banana"))
"##,
        expect,
    );
}
