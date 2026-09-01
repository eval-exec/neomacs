//! Oracle parity tests for GNU `delete`, `remove`, and `remq` edge semantics.
//!
//! GNU implements `delete` in `src/fns.c`; list inputs are spliced
//! destructively, but vector and string inputs return the original object when
//! no element is removed.  GNU `remove` and `remq` are Lisp wrappers in
//! `lisp/subr.el`, so their sharing behavior follows those exact wrappers.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_delete_remove_non_list_identity_and_string_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((s (copy-sequence "abacad"))
       (_ (put-text-property 0 6 'face 'bold s))
       (delete-nohit (delete 'a s))
       (delete-hit (delete ?a s))
       (remove-nohit (remove 'a s))
       (remove-hit (remove ?a s))
       (v (vector "a" "b" "a"))
       (delete-vec-nohit (delete "z" v))
       (delete-vec-hit (delete "a" v))
       (remove-vec-nohit (remove "z" v))
       (remove-vec-hit (remove "a" v)))
  (list
   (list delete-nohit (eq delete-nohit s) (text-properties-at 0 delete-nohit))
   (list delete-hit (eq delete-hit s) (text-properties-at 0 delete-hit))
   (list remove-nohit (eq remove-nohit s) (text-properties-at 0 remove-nohit))
   (list remove-hit (eq remove-hit s) (text-properties-at 0 remove-hit))
   (list delete-vec-nohit (eq delete-vec-nohit v))
   (list delete-vec-hit (eq delete-vec-hit v))
   (list remove-vec-nohit (eq remove-vec-nohit v))
   (list remove-vec-hit (eq remove-vec-hit v))
   v))
"#;

    let expect = expect_test::expect![[
        r#""OK ((#(\"abacad\" 0 6 (face bold)) t (face bold)) (\"bcd\" nil nil) (#(\"abacad\" 0 6 (face bold)) t (face bold)) (\"bcd\" nil nil) ([\"a\" \"b\" \"a\"] t) ([\"b\"] nil) ([\"a\" \"b\" \"a\"] t) ([\"b\"] nil) [\"a\" \"b\" \"a\"])""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_remq_leading_match_sharing_and_copy_boundary() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((leading-only (list 'x 'a 'b 'c))
       (leading-only-tail (cdr leading-only))
       (leading-again (list 'x 'a 'x 'b))
       (leading-again-tail (cdr leading-again))
       (leading-only-result (remq 'x leading-only))
       (leading-again-result (remq 'x leading-again)))
  (list
   (list leading-only-result
         (eq leading-only-result leading-only)
         (eq leading-only-result leading-only-tail)
         leading-only)
   (list leading-again-result
         (eq leading-again-result leading-again)
         (eq leading-again-result leading-again-tail)
         leading-again)))
"#;

    let expect =
        expect_test::expect![[r#""OK (((a b c) nil t (x a b c)) ((a b) nil nil (x a x b)))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_remq_improper_list_error_payloads_follow_gnu_wrapper_order() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU lisp/subr.el:remq first strips leading eq elements with `car' and
    // `cdr', then calls `memq', and only copies/delqs the tail if memq finds
    // another match.  These paths expose different exact error payloads.
    let form = r#"
(list
 (condition-case e
     (remq 'x 'tail)
   (error (list (car e) (cdr e))))
 (let ((xs (cons 'x 'tail)))
   (list
    (condition-case e
        (remq 'x xs)
      (error (list (car e) (cdr e))))
    xs))
 (let ((xs (cons 'x (cons 'a 'tail))))
   (list
    (condition-case e
        (remq 'x xs)
      (error (list (car e) (cdr e))))
    xs))
 (let ((xs (cons 'a (cons 'x 'tail))))
   (list
    (condition-case e
        (remq 'x xs)
      (error (list (car e) (cdr e))))
    xs))
 (let ((xs (cons 'a (cons 'b 'tail))))
   (list
    (condition-case e
        (remq 'x xs)
      (error (list (car e) (cdr e))))
    xs)))
"#;

    let expect = expect_test::expect![[
        r#""OK ((wrong-type-argument (listp tail)) ((wrong-type-argument (listp tail)) (x . tail)) ((wrong-type-argument (listp (a . tail))) (x a . tail)) ((wrong-type-argument (listp tail)) (a x . tail)) ((wrong-type-argument (listp (a b . tail))) (a b . tail)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_delq_delete_dotted_list_mutation_before_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU src/fns.c:Fdelq and Fdelete splice matching cons cells while walking
    // the list, then call CHECK_LIST_END.  On dotted inputs, the signaled
    // `wrong-type-argument' payload is the post-splice list, and earlier
    // destructive edits remain visible.
    let form = r#"
(list
 (let ((xs (cons 'keep (cons 'drop 'tail))))
   (list
    (condition-case e
        (delq 'drop xs)
      (error (list (car e) (cdr e))))
    xs))
 (let ((xs (cons (copy-sequence "drop") (cons 'keep 'tail))))
   (list
    (condition-case e
        (delete "drop" xs)
      (error (list (car e) (cdr e))))
    xs))
 (let ((xs (cons 'drop (cons 'keep 'tail))))
   (list
    (condition-case e
        (delq 'drop xs)
      (error (list (car e) (cdr e))))
    xs)))
"#;

    let expect = expect_test::expect![[
        r#""OK (((wrong-type-argument (listp (keep . tail))) (keep . tail)) ((wrong-type-argument (listp (keep . tail))) (\"drop\" keep . tail)) ((wrong-type-argument (listp (keep . tail))) (drop keep . tail)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_delete_remove_reject_bool_vector_like_gnu() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU Emacs implements `delete` in src/fns.c:Fdelete.  That code accepts
    // nil, cons, vector, and string inputs explicitly, then calls
    // wrong_type_argument(Qsequencep, seq) for all other objects; bool-vectors
    // are not treated as vectors here.  `remove` is the Lisp wrapper in
    // lisp/subr.el and inherits the same boundary.
    let form = r#"
(list
 (condition-case e
     (delete t (bool-vector t nil t nil))
   (error (list (car e) (cdr e))))
 (condition-case e
     (remove nil (bool-vector t nil t nil))
   (error (list (car e) (cdr e))))
 (condition-case e
     (delq t (bool-vector t nil t nil))
   (error (list (car e) (cdr e)))))
"#;

    let expect = expect_test::expect![[
        r#""OK ((wrong-type-argument (sequencep #&4\"\u{5}\")) (wrong-type-argument (sequencep #&4\"\u{5}\")) (wrong-type-argument (listp #&4\"\u{5}\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
