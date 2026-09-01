//! Divergence tests: cl-lib sequence operations, reduce, every, some.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_cl_reduce() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-reduce)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (cl-reduce '+ '(1 2 3 4))
  (cl-reduce '* '(1 2 3 4))
  (cl-replace "hello" "world" :start1 0 :end1 5)
  (fboundp 'cl-reduce)) "#,
        expect,
    );
}

#[test]
fn divergence_cl_every_some() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-every)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (cl-every 'integerp '(1 2 3))
  (cl-every 'integerp '(1 "a" 3))
  (cl-some 'null '(nil nil nil))
  (cl-some 'null '(1 nil 3))
  (cl-notany 'null '(1 2 3))
  (cl-notevery 'integerp '(1 "a" 3))) "#,
        expect,
    );
}

#[test]
fn divergence_cl_position() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-position)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (cl-position 3 '(1 2 3 4 5))
  (cl-position 6 '(1 2 3 4 5))
  (cl-position ?a "xyzabc" :start 3)
  (cl-count ?a "banana")) "#,
        expect,
    );
}

#[test]
fn divergence_cl_remove_delete() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-remove)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (cl-remove 3 '(1 2 3 4 3 5))
  (cl-remove-duplicates '(1 2 3 2 1))
  (cl-delete 3 (list 1 2 3 4 3 5))
  (cl-substitute 99 3 '(1 2 3 4 3))) "#,
        expect,
    );
}

#[test]
fn divergence_cl_sort_stable() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-sort)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (cl-sort (copy-sequence '(3 1 4 1 5 9)) '<)
  (fboundp 'cl-stable-sort)
  (sort (copy-sequence '(3 1 4 1 5)) '<)) "#,
        expect,
    );
}

#[test]
fn divergence_cl_merge() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-merge)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (cl-merge 'list '(1 3 5) '(2 4 6) '<)
  (fboundp 'cl-merge)) "#,
        expect,
    );
}

#[test]
fn divergence_cl_search_mismatch() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-search)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (cl-search '(2 3) '(1 2 3 4))
  (cl-search "bar" "foobar")
  (cl-mismatch '(1 2 3) '(1 2 4))
  (cl-mismatch "abc" "abc")) "#,
        expect,
    );
}

#[test]
fn divergence_cl_subseq() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-subseq)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (cl-subseq '(a b c d e) 1 3)
  (cl-subseq '(a b c d e) 2)
  (cl-subseq "hello" 1 4)) "#,
        expect,
    );
}

#[test]
fn divergence_seq_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'seq-map)
  (fboundp 'seq-filter)
  (fboundp 'seq-reduce)
  (fboundp 'seq-find)
  (fboundp 'seq-some)
  (fboundp 'seq-every-p)
  (featurep 'seq)) "#,
        expect,
    );
}

#[test]
fn divergence_seq_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-oddp)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (seq-map #'1+ '(1 2 3))
  (seq-filter #'cl-oddp '(1 2 3 4 5))
  (seq-reduce #'+ '(1 2 3 4) 0)
  (seq-find #'cl-oddp '(2 4 5 6))
  (seq-contains '(1 2 3) 2)
  (seq-position '(a b c d) 'c)) "#,
        expect,
    );
}
