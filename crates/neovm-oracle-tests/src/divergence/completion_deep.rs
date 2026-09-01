//! Completion deep coverage (transformers, metadata, styles).
//!
//! Beyond the known divergences (completion-ignore-case prefix case, extra
//! calendar-month category), completion is faithful. This batch covers the
//! table transformers (in-turn, merge), metadata, boundaries, try-completion,
//! and the style variants to pin that parity.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cdt_try_completion() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"ap\" . 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(completion-try-completion "ap" '("apple" "apricot" "banana") nil 2)"##,
        expect,
    );
}

#[test]
fn div_cdt_all_completions_metadata_tail() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((#(\"apple\" 0 2 (face completions-common-part) 2 3 (face completions-first-difference)) #(\"apricot\" 0 2 (face completions-common-part) 2 3 (face completions-first-difference)) . 0) t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ac (completion-all-completions "ap" '("apple" "apricot") nil 2)))
  (list ac (and (consp ac) (consp (last ac)))))
"##,
        expect,
    );
}

#[test]
fn div_cdt_completion_boundaries() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 . 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(completion-boundaries "ap" '("apple" "apricot") nil "x")"##,
        expect,
    );
}

#[test]
fn div_cdt_table_in_turn() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((\"abc\" \"abd\") \"ab\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((tbl (completion-table-in-turn '("abc" "abd") '("xyz"))))
  (list (all-completions "" tbl) (try-completion "a" tbl)))
"##,
        expect,
    );
}

#[test]
fn div_cdt_table_merge() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"a\" \"b\" \"c\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((tbl (completion-table-merge '("a" "b") '("c"))))
  (all-completions "" tbl))
"##,
        expect,
    );
}

#[test]
fn div_cdt_metadata() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((md (completion-metadata "ap" '("apple") nil)))
  (list (consp md) (completion-metadata-get md 'category)))
"##,
        expect,
    );
}

#[test]
fn div_cdt_flex_style() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((completion-styles '(flex)))
  (list (all-completions "abc" '("axbycz" "aXbYc" "axbyc"))
        (try-completion "abc" '("axbycz"))))
"##,
        expect,
    );
}

#[test]
fn div_cdt_initials_style() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((completion-styles '(initials)))
  (all-completions "abc" '("a-big-cat" "another-bigger-cat" "axbyc")))
"##,
        expect,
    );
}

#[test]
fn div_cdt_partial_style() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((completion-styles '(partial)))
  (all-completions "bc" '("abcd" "xabcd" "xbcd")))
"##,
        expect,
    );
}

#[test]
fn div_cdt_ignored_extensions() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"f.o\" \"f.elc\" \"f.c\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((completion-ignored-extensions '(".o" ".elc")))
  (all-completions "f" '("f.o" "f.elc" "f.c")))
"##,
        expect,
    );
}
