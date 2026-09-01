//! Deep combo: completion + try-completion + all-completions + test-completion.
//! Tests completion system with various collection types.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn deficiency_try_completion_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"alpha\" \"bravo\" nil \"alpha\")""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((coll '(\"alpha\" \"bravo\" \"charlie\" \"delta\")))\n\
         (list (try-completion \"al\" coll)\n\
         (try-completion \"br\" coll)\n\
         (try-completion \"xyz\" coll)\n\
         (try-completion \"a\" coll))))",
        expect,
    );
}

#[test]
fn deficiency_all_completions_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((\"apple\" \"apricot\") (\"banana\") nil)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((coll '(\"apple\" \"apricot\" \"banana\" \"cherry\")))\n\
         (list (all-completions \"ap\" coll)\n\
         (all-completions \"b\" coll)\n\
         (all-completions \"z\" coll))))",
        expect,
    );
}

#[test]
fn deficiency_test_completion_predicate() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((coll '(\"alpha\" \"bravo\" \"charlie\")))\n\
         (list (test-completion \"alpha\" coll)\n\
         (test-completion \"alpha\" coll)\n\
         (test-completion \"bravo\" coll)\n\
         (test-completion \"missing\" coll))))",
        expect,
    );
}

#[test]
fn completion_with_obarray() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"compute-\" (\"compute-result\" \"compute-value\") (\"calculate-value\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((ob (make-vector 7 0)))\n\
         (intern \"compute-value\" ob)\n\
         (intern \"compute-result\" ob)\n\
         (intern \"calculate-value\" ob)\n\
         (list (try-completion \"compute\" ob)\n\
         (all-completions \"compute\" ob)\n\
         (all-completions \"calc\" ob))))",
        expect,
    );
}

#[test]
fn completion_with_lambda_predicate() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((\"file1.txt\" \"file4.txt\") \"file2.el\")""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((coll '(\"file1.txt\" \"file2.el\" \"file3.rs\" \"file4.txt\")))\n\
         (list (all-completions \"file\" coll\n\
         (lambda (cand) (string-match \"\\\\.txt$\" cand)))\n\
         (try-completion \"file\" coll\n\
         (lambda (cand) (string-match \"\\\\.el$\" cand))))))",
        expect,
    );
}

#[test]
fn completion_with_alist() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"ap\" (\"banana\" \"broccoli\") t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((alist '((\"apple\" . fruit) (\"apricot\" . fruit)\n\
         (\"banana\" . fruit) (\"broccoli\" . veg))))\n\
         (list (try-completion \"ap\" alist)\n\
         (all-completions \"b\" alist)\n\
         (test-completion \"apple\" alist))))",
        expect,
    );
}

#[test]
fn completion_case_sensitivity() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"hello\" (\"hello\") \"HELLO\")""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((coll '(\"Hello\" \"HELLO\" \"hello\")))\n\
         (list (try-completion \"hel\" coll)\n\
         (all-completions \"hel\" coll)\n\
         (try-completion \"HEL\" coll))))",
        expect,
    );
}

#[test]
fn completion_unique_vs_ambiguous() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"uni\" \"unique\" t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((coll '(\"unique\" \"unit\" \"university\")))\n\
         (list (try-completion \"uni\" coll)\n\
         (try-completion \"uniq\" coll)\n\
         (eq t (try-completion \"unique\" coll)))))",
        expect,
    );
}

#[test]
fn completion_with_hyphenated_names() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"my-\" (\"my-function\" \"my-variable\" \"my-constant\") (\"other-function\" \"other-variable\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((coll '(\"my-function\" \"my-variable\" \"my-constant\"\n\
         \"other-function\" \"other-variable\")))\n\
         (list (try-completion \"my-\" coll)\n\
         (all-completions \"my-\" coll)\n\
         (all-completions \"other-\" coll))))",
        expect,
    );
}

#[test]
fn completion_empty_and_full_match() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"a\" \"a\" \"ab\" t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((coll '(\"a\" \"ab\" \"abc\" \"abd\")))\n\
         (list (try-completion \"\" coll)\n\
         (try-completion \"a\" coll)\n\
         (try-completion \"ab\" coll)\n\
         (try-completion \"abc\" coll))))",
        expect,
    );
}
