//! Deep combo: print-circle + circular lists + nconc + reverse + identity.
//! Tests circular reference handling in print/read with list manipulation.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn deficiency_print_circle_with_shared_tail() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((shared '(x y z)))\n\
         (let ((l1 (cons 'a (cons 'b shared)))\n\
         (l2 (cons 'c (cons 'd shared))))\n\
         (let ((printed (let ((print-circle t) (print-gensym t))\n\
         (prin1-to-string (list l1 l2)))))\n\
         (list printed\n\
         (eq (nthcdr 2 l1) (nthcdr 2 l2))))))",
        expect,
    );
}

#[test]
fn deficiency_make_circular_list_print_circle() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((l (list 1 2 3)))\n\
         (setcdr (nthcdr 2 l) l)\n\
         (let ((printed (let ((print-circle t))\n\
         (prin1-to-string l))))\n\
         (list (string-match \"#0=\" printed)\n\
         (string-match \"#0#\" printed)))))",
        expect,
    );
}

#[test]
fn deficiency_nconc_builds_shared_structure() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((a b z) (c d z) t t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((shared '(z)))\n\
         (let ((l1 (nconc (list 'a 'b) shared))\n\
         (l2 (nconc (list 'c 'd) shared)))\n\
         (list l1 l2\n\
         (eq (nthcdr 2 l1) (nthcdr 2 l2))\n\
         (eq (nth 2 l1) (nth 2 l2))))))",
        expect,
    );
}

#[test]
fn deficiency_reverse_vs_nreverse_on_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((5 4 3 2 1) (5 4 3 2 1) t (1 2 3 4 5) (1))""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((original '(1 2 3 4 5))\n\
         (copy (list 1 2 3 4 5)))\n\
         (let ((r1 (reverse original))\n\
         (r2 (nreverse copy)))\n\
         (list r1 r2 (equal r1 r2)\n\
         original copy))))",
        expect,
    );
}

#[test]
fn deficiency_nconc_mutates_last_pair() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((1 2 3 4) (3 4) t 4)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((a (list 1 2))\n\
         (b (list 3 4)))\n\
         (nconc a b)\n\
         (list a b\n\
         (eq (nthcdr 2 a) b)\n\
         (length a))))",
        expect,
    );
}

#[test]
fn deficiency_append_does_not_mutate() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((1 2 3 4) 4 t (1 2) (3 4))""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((a (list 1 2))\n\
         (b (list 3 4)))\n\
         (let ((c (append a b)))\n\
         (list c (length c)\n\
         (eq (nthcdr 2 c) b)\n\
         a b))))",
        expect,
    );
}

#[test]
fn deficiency_copy_tree_vs_copy_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (((X 2) (3 (4 5)) 6) ((1 2) (3 (4 5)) 6) t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((orig '((1 2) (3 (4 5)) 6)))\n\
         (let ((shallow (copy-sequence orig))\n\
         (deep (copy-tree orig)))\n\
         (setcar (nth 0 shallow) 'X)\n\
         (list shallow deep\n\
         (eq (nth 0 orig) (nth 0 shallow))\n\
         (eq (nth 0 orig) (nth 0 deep))))))",
        expect,
    );
}

#[test]
fn deficiency_list_length_vs_safe_length() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function list-length)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((l (list 1 2 3 4 5)))\n\
         (list (length l)\n\
         (safe-length l)\n\
         (list-length l))))",
        expect,
    );
}

#[test]
fn deficiency_plist_put_mutates_in_place() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((a 1 b 99 c 3) 99 3)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((pl (list 'a 1 'b 2 'c 3)))\n\
         (plist-put pl 'b 99)\n\
         (list pl (plist-get pl 'b)\n\
         (plist-get pl 'c))))",
        expect,
    );
}

#[test]
fn deficiency_assoc_set_alist_mutation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (((c . 3) (a . 10) (b . 2)) 10 2 3 3)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((alist '((a . 1) (b . 2))))\n\
         (setcdr (assoc 'a alist) 10)\n\
         (let ((new (cons 'c 3)))\n\
         (push new alist)\n\
         (list alist\n\
         (alist-get 'a alist)\n\
         (alist-get 'b alist)\n\
         (alist-get 'c alist)\n\
         (length alist)))))",
        expect,
    );
}
