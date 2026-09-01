//! Divergence tests: cl-extra, cl-seq, cl-macs deep.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_cl_remove_if() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((1 3 5) (2 4 6) (3 2 1 4))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(require 'cl-lib)
(list
  (cl-remove-if #'cl-evenp '(1 2 3 4 5 6))
  (cl-remove-if-not #'cl-evenp '(1 2 3 4 5 6))
  (cl-remove-duplicates '(1 2 3 2 1 4)))"#,
        expect,
    );
}

#[test]
fn divergence_cl_sort() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 1 2 3 4 5 6 9)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(require 'cl-lib)
(cl-sort '(3 1 4 1 5 9 2 6) #'<)"#,
        expect,
    );
}

#[test]
fn divergence_cl_subseq() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((b c) (c d e) \"ell\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(require 'cl-lib)
(list
  (cl-subseq '(a b c d e) 1 3)
  (cl-subseq '(a b c d e) 2)
  (cl-subseq "hello" 1 4))"#,
        expect,
    );
}

#[test]
fn divergence_cl_position() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 3 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(require 'cl-lib)
(list
  (cl-position 'b '(a b c b a))
  (cl-position 'b '(a b c b a) :from-end t)
  (cl-position 'z '(a b c)))"#,
        expect,
    );
}

#[test]
fn divergence_cl_count() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (3 3 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(require 'cl-lib)
(list
  (cl-count 'a '(a b a c a))
  (cl-count-if #'cl-evenp '(1 2 3 4 5 6))
  (cl-count-if-not #'cl-evenp '(1 2 3 4 5 6)))"#,
        expect,
    );
}

#[test]
fn divergence_cl_reduce() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (20 24 \"abXYZf\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(require 'cl-lib)
(list
  (cl-reduce #'+ '(1 2 3 4) :initial-value 10)
  (cl-reduce #'* '(1 2 3 4))
  (cl-replace (copy-sequence "abcdef") "XYZ" :start1 2))"#,
        expect,
    );
}

#[test]
fn divergence_cl_merge() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 2 3 4 5 6 7 8)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(require 'cl-lib)
(cl-merge 'list '(1 3 5 7) '(2 4 6 8) #'<)"#,
        expect,
    );
}

#[test]
fn divergence_cl_dolist_dotimes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-dolist)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((result nil))
  (cl-dolist (x '(a b c) result)
    (push x result)))"#,
        expect,
    );
}

#[test]
fn divergence_cl_destructuring_bind() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((1 2 (3 4 5)) (10 20))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(require 'cl-lib)
(list
  (cl-destructuring-bind (a b . c) '(1 2 3 4 5)
    (list a b c))
  (cl-destructuring-bind (&key x y) '(:x 10 :y 20)
    (list x y)))"#,
        expect,
    );
}

#[test]
fn divergence_cl_the_check() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (42 \"hello\" t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(require 'cl-lib)
(list
  (cl-the fixnum 42)
  (cl-the string "hello")
  (cl-typep 42 'integer)
  (cl-typep "hello" 'string)
  (cl-typep 42 'string))"#,
        expect,
    );
}

#[test]
fn divergence_cl_assoc_rassoc() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((b . 2) (c . 3) (b c) (d a b c))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(require 'cl-lib)
(let ((alist '((a . 1) (b . 2) (c . 3))))
  (list (cl-assoc 'b alist)
        (cl-rassoc 3 alist)
        (cl-member 'b '(a b c))
        (cl-adjoin 'd '(a b c))))"#,
        expect,
    );
}
