//! Oracle parity tests for `plist-get`, `plist-put`, `plist-member`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

#[test]
fn oracle_prop_plist_get_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 2""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect("(plist-get '(:a 1 :b 2 :c 3) :b)", expect);
    assert_ok_eq("2", &o, &n);

    let expect = expect_test::expect![[r#""OK 1""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect("(plist-get '(:a 1 :b 2 :c 3) :a)", expect);
    assert_ok_eq("1", &o, &n);

    let expect = expect_test::expect![[r#""OK 3""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect("(plist-get '(:a 1 :b 2 :c 3) :c)", expect);
    assert_ok_eq("3", &o, &n);
}

#[test]
fn oracle_prop_plist_get_missing_key() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(plist-get '(:a 1 :b 2) :z)", expect);
    assert_ok_eq("nil", &o, &n);
}

#[test]
fn oracle_prop_plist_get_empty() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(plist-get nil :a)", expect);
    assert_ok_eq("nil", &o, &n);
}

#[test]
fn oracle_prop_plist_put_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((pl '(:a 1 :b 2)))
                  (plist-get (plist-put pl :c 3) :c))";
    let expect = expect_test::expect![[r#""OK 3""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("3", &o, &n);
}

#[test]
fn oracle_prop_plist_put_overwrite() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((pl '(:a 1 :b 2)))
                  (plist-get (plist-put pl :a 99) :a))";
    let expect = expect_test::expect![[r#""OK 99""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("99", &o, &n);
}

#[test]
fn oracle_prop_plist_member_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (:b 2 :c 3)""#]];
    // plist-member returns the tail starting from the matching key
    crate::common::assert_oracle_parity_expect("(plist-member '(:a 1 :b 2 :c 3) :b)", expect);
}

#[test]
fn oracle_prop_plist_member_missing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect("(plist-member '(:a 1 :b 2) :z)", expect);
    assert_ok_eq("nil", &o, &n);
}

#[test]
fn oracle_prop_plist_chained_puts() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let* ((pl nil)
                       (pl (plist-put pl :x 10))
                       (pl (plist-put pl :y 20))
                       (pl (plist-put pl :z 30)))
                  (list (plist-get pl :x)
                        (plist-get pl :y)
                        (plist-get pl :z)))";
    let expect = expect_test::expect![[r#""OK (10 20 30)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_plist_with_non_keyword_keys() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 2""#]];
    // plist-get/plist-put work with any eq-comparable keys
    let (o, n) =
        crate::common::eval_oracle_and_neovm_expect("(plist-get '(a 1 b 2 c 3) 'b)", expect);
    assert_ok_eq("2", &o, &n);
}

#[test]
fn oracle_prop_plist_complex_values() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(plist-get '(:data (1 2 3) :name \"test\" :flag t) :data)";
    let expect = expect_test::expect![[r#""OK (1 2 3)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_plist_optional_predicate_semantics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((key-a (copy-sequence "key"))
       (key-b (copy-sequence "key"))
       (plist (list key-a 1 :other 2)))
  (list
   (plist-get plist key-b)
   (plist-get plist key-b 'equal)
   (plist-member plist key-b)
   (plist-member plist key-b 'equal)
   (let ((copy (copy-sequence plist)))
     (list (eq (plist-put copy key-b 9 'equal) copy)
           (plist-get copy key-a)
           copy))
   (let ((copy (copy-sequence plist)))
     (plist-put copy key-b 9)
     copy)))
"#;

    let expect = expect_test::expect![[
        r#""OK (nil 1 nil (\"key\" 1 :other 2) (t 9 (\"key\" 9 :other 2)) (\"key\" 1 :other 2 \"key\" 9))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_plist_malformed_tail_error_boundaries() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (plist-get '(:a 1 . bogus) :z)
 (plist-get '(:a) :z)
 (plist-get '(:a . bogus) :z)
 (condition-case err
     (plist-member '(:a 1 . bogus) :z)
   (error (list (car err) (cdr err))))
 (condition-case err
     (plist-member '(:a) :z)
   (error (list (car err) (cdr err))))
 (condition-case err
     (plist-member '(:a . bogus) :z)
   (error (list (car err) (cdr err))))
 (condition-case err
     (plist-put '(:a 1 . bogus) :z 3)
   (error (list (car err) (cdr err))))
 (condition-case err
     (plist-put '(:a) :z 3)
   (error (list (car err) (cdr err))))
 (condition-case err
     (plist-put '(:a . bogus) :z 3)
   (error (list (car err) (cdr err)))))
"#;

    let expect = expect_test::expect![[
        r#""OK (nil nil nil (wrong-type-argument (plistp (:a 1 . bogus))) nil (wrong-type-argument (plistp (:a . bogus))) (wrong-type-argument (plistp (:a 1 . bogus))) (wrong-type-argument (plistp (:a))) (wrong-type-argument (plistp (:a . bogus))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
