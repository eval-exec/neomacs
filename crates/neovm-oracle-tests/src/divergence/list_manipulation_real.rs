//! Divergence tests: real list manipulation behavioral differences.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_list_manipulation_real() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 (2 3) 2 3 1 3 nil (3) 3 3)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((l '(1 2 3)))
  (list (car l)
        (cdr l)
        (cadr l)
        (caddr l)
        (nth 0 l)
        (nth 2 l)
        (nth 10 l)
        (last l)
        (length l)
        (safe-length l))) ",
        expect,
    );
}

#[test]
fn divergence_push_pop_nreverse() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((2) 3 (1 2))""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((l nil))
  (push 1 l)
  (push 2 l)
  (push 3 l)
  (let ((popped (pop l)))
    (list l popped (nreverse l)))) ",
        expect,
    );
}

#[test]
fn divergence_append_nconc_concat() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function copy-list)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((a '(1 2)) (b '(3 4)))
  (list (append a b)
        (append a b '(5))
        (equal (append a b) '(1 2 3 4))
        (let ((c (copy-list a)))
          (nconc c b)
          c)
        (concat \"hello\" \" \" \"world\"))) ",
        expect,
    );
}

#[test]
fn divergence_mapcar_mapc_mapconcat() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((2 3 4 5) (\"a\" \"b\" \"c\") (1 2 3) \"1-2-3\" (1 4 9 16 25))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(list
  (mapcar #'1+ '(1 2 3 4))
  (mapcar #'symbol-name '(a b c))
  (mapc #'identity '(1 2 3))
  (mapconcat #'number-to-string '(1 2 3) \"-\")
  (mapcar (lambda (x) (* x x)) '(1 2 3 4 5))) ",
        expect,
    );
}

#[test]
fn divergence_member_assoc_assq_real() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK ((b . 2) (c . 3) 2 (3 4 5) (2 3 4 5) (4 5) (b . 2))""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((alist '((a . 1) (b . 2) (c . 3)))
        (lst '(1 2 3 4 5)))
  (list (assoc 'b alist)
        (assq 'c alist)
        (assoc-default 'b alist)
        (member 3 lst)
        (memq 2 lst)
        (memql 4 lst)
        (rassoc 2 alist))) ",
        expect,
    );
}

#[test]
fn divergence_destructuring_bind() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-destructuring-bind)""#]];
    crate::common::assert_oracle_parity_expect(
        "(cl-destructuring-bind (a (b c) &rest d) '(1 (2 3) 4 5)
  (list a b c d)) ",
        expect,
    );
}

#[test]
fn divergence_tree_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function tree-equal)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((tree '(1 (2 (3 4)) (5 6))))
  (list tree
        (copy-tree tree)
        (equal tree (copy-tree tree))
        (eq tree (copy-tree tree))
        (tree-equal tree '(1 (2 (3 4)) (5 6))))) ",
        expect,
    );
}

#[test]
fn divergence_set_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-intersection)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((a '(1 2 3 4))
        (b '(3 4 5 6)))
  (list (cl-intersection a b)
        (cl-union a b)
        (cl-set-difference a b)
        (cl-set-exclusive-or a b)
        (cl-subsetp '(3 4) a))) ",
        expect,
    );
}

#[test]
fn deficiency_subseq_butlast() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-subseq)""#]];
    crate::common::assert_oracle_parity_expect(
        "(list
  (cl-subseq '(1 2 3 4 5) 1 3)
  (cl-subseq '(1 2 3 4 5) 2)
  (butlast '(1 2 3 4 5))
  (butlast '(1 2 3 4 5) 2)
  (nbutlast (list 1 2 3 4 5) 3)
  (last '(1 2 3 4 5) 2)) ",
        expect,
    );
}

#[test]
fn divergence_number_sequencing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((1 2 3 4 5) (0 2 4 6 8 10) (5 4 3 2 1) 100 (3))""#]];
    crate::common::assert_oracle_parity_expect(
        "(list
  (number-sequence 1 5)
  (number-sequence 0 10 2)
  (number-sequence 5 1 -1)
  (length (number-sequence 1 100))
  (number-sequence 3 3)) ",
        expect,
    );
}
