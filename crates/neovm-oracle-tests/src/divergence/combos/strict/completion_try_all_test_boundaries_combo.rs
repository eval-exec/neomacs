//! Strict combo oracle probes, batch 183: completion machinery. try-completion
//! (prefix + exact + miss), all-completions, test-completion, and
//! completion-boundaries over list and obarray collections.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_completion_try_all_test_list_collection() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((coll '("alpha" "alphabet" "beta" "gamma" "gamma2")))
  (list (try-completion "al" coll)
        (try-completion "alphab" coll)
        (try-completion "alphabet" coll)
        (try-completion "b" coll)
        (try-completion "xyz" coll)
        (all-completions "al" coll)
        (all-completions "g" coll)
        (all-completions "" coll)
        (test-completion "alpha" coll)
        (test-completion "xyz" coll)))
"##;
    let expect = expect_test::expect![[
        r#""OK (\"alpha\" \"alphabet\" t \"beta\" nil (\"alpha\" \"alphabet\") (\"gamma\" \"gamma2\") (\"alpha\" \"alphabet\" \"beta\" \"gamma\" \"gamma2\") t nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_completion_boundaries_obarray() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((coll '("car" "cdr" "car-safe" "cadr" "cdr-safe" "concat")))
  (list (completion-boundaries "ca" coll nil "r-safe")
        (completion-boundaries "c" coll nil "")
        (completion-boundaries "" coll nil "ar")
        (try-completion "car-" coll)
        (all-completions "ca" coll)
        (completion-boundaries "xyz" coll nil "abc")))
"##;
    let expect = expect_test::expect![[
        r#""OK ((0 . 6) (0 . 0) (0 . 2) \"car-safe\" (\"car\" \"car-safe\" \"cadr\") (0 . 3))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_completion_predicate_lambda_and_style() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((coll '("apple" "apricot" "banana" "avocado" "cherry")))
  (list (all-completions "a" coll (lambda (s) (> (length s) 5)))
        (try-completion "a" coll (lambda (s) (> (length s) 5)))
        (all-completions "a" coll)
        (length (all-completions "a" coll))
        (test-completion "apple" coll)))
"##;
    let expect = expect_test::expect![[
        r#""OK ((\"apricot\" \"avocado\") \"a\" (\"apple\" \"apricot\" \"avocado\") 3 t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
