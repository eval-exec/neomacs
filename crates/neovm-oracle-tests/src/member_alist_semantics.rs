//! Oracle parity tests for GNU membership and alist lookup edge semantics.
//!
//! GNU implements `member`, `memq`, `memql`, `assq`, `assoc`, `rassq`, and
//! `rassoc` in `src/fns.c`.  These primitives walk lists with checked tail
//! iteration, skip non-cons alist entries, and optimize some `equal` lookups to
//! `eq` paths for symbols and fixnums.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_member_family_dotted_tail_error_payloads() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (condition-case err
     (memq 'z '(a b . c))
   (error (list (car err) (cdr err))))
 (condition-case err
     (member "z" '("a" "b" . c))
   (error (list (car err) (cdr err))))
 (condition-case err
     (memql 1.25 '(1.0 2.0 . c))
   (error (list (car err) (cdr err))))
 (condition-case err
     (memq 'a 42)
   (error (list (car err) (cdr err)))))
"#;

    let expect = expect_test::expect![[
        r#""OK ((wrong-type-argument (listp (a b . c))) (wrong-type-argument (listp (\"a\" \"b\" . c))) (wrong-type-argument (listp (1.0 2.0 . c))) (wrong-type-argument (listp 42)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_member_family_returns_tail_before_bad_tail() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (memq 'b '(a b . c))
 (member "b" '("a" "b" . c))
 (memql 1.25 '(0.5 1.25 . c))
 (memql 1000000000000000000000001
        '(1 1000000000000000000000001 . c)))
"#;

    let expect = expect_test::expect![[
        r#""OK ((b . c) (\"b\" . c) (1.25 . c) (1000000000000000000000001 . c))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_member_family_float_bignum_and_equal_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((nan-a (/ 0.0 0.0))
      (nan-b (/ 0.0 0.0)))
  (list
   (member (list 1 2) '((0) (1 2) (3)))
   (memq (list 1 2) '((0) (1 2) (3)))
   (memql 1.0 '(0 1 1.0 2.0))
   (memql 1.0 '(0 1.0 2.0))
   (memql 1000000000000000000000001
          '(1000000000000000000000000 1000000000000000000000001))
   (memql nan-a (list nan-b))))
"#;

    let expect = expect_test::expect![[
        r#""OK (((1 2) (3)) nil (1.0 2.0) (1.0 2.0) (1000000000000000000000001) (-0.0e+NaN))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_member_family_circular_lists() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((x (list 'a 'b 'c))
       (_ (setcdr (last x) x)))
  (list
   (memq 'b x)
   (condition-case err
       (memq 'z x)
     (error (list (car err) (cdr err))))
   (condition-case err
       (member "z" x)
     (error (list (car err) (cdr err))))
   (condition-case err
       (memql 1.25 x)
     (error (list (car err) (cdr err))))))
"#;

    let expect = expect_test::expect![[
        r#""OK ((b c a b c . #2) (circular-list ((c a b c a . #2))) (circular-list ((c a b c a . #2))) (circular-list ((c a b c a . #2))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_alist_lookup_skips_non_cons_but_checks_tail() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (assq 'b '(loose (a . 1) 42 (b . 2) . tail))
 (assoc "b" '(loose ("a" . 1) 42 ("b" . 2) . tail))
 (rassq 2 '(loose (a . 1) 42 (b . 2) . tail))
 (rassoc "two" '(loose (a . "one") 42 (b . "two") . tail))
 (condition-case err
     (assq 'z '(loose (a . 1) 42 . tail))
   (error (list (car err) (cdr err))))
 (condition-case err
     (assoc "z" '(loose ("a" . 1) 42 . tail))
   (error (list (car err) (cdr err))))
 (condition-case err
     (rassq 9 '(loose (a . 1) 42 . tail))
   (error (list (car err) (cdr err))))
 (condition-case err
     (rassoc "z" '(loose (a . "one") 42 . tail))
   (error (list (car err) (cdr err)))))
"#;

    let expect = expect_test::expect![[
        r#""OK ((b . 2) (\"b\" . 2) (b . 2) (b . \"two\") (wrong-type-argument (listp (loose (a . 1) 42 . tail))) (wrong-type-argument (listp (loose (\"a\" . 1) 42 . tail))) (wrong-type-argument (listp (loose (a . 1) 42 . tail))) (wrong-type-argument (listp (loose (a . \"one\") 42 . tail))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_alist_lookup_leading_non_cons_improper_tail_payloads() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU fns.c uses FOR_EACH_TAIL plus CHECK_LIST_END for these alist
    // lookups.  A leading non-cons entry is skipped, but the final signal
    // data still reports the original improper list object.
    let form = r#"
(list
 (condition-case err
     (assq 'z (cons 'loose 'tail))
   (error (list (car err) (cdr err))))
 (condition-case err
     (assoc 'z (cons 'loose 'tail))
   (error (list (car err) (cdr err))))
 (condition-case err
     (rassq 'z (cons 'loose 'tail))
   (error (list (car err) (cdr err))))
 (condition-case err
     (rassoc 'z (cons 'loose 'tail))
   (error (list (car err) (cdr err)))))
"#;

    let expect = expect_test::expect![[
        r#""OK ((wrong-type-argument (listp (loose . tail))) (wrong-type-argument (listp (loose . tail))) (wrong-type-argument (listp (loose . tail))) (wrong-type-argument (listp (loose . tail))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_assoc_testfn_argument_order_and_tail_validation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((calls nil))
  (list
   (assoc 'needle
          '((hay . 1) (needle . 2))
          (lambda (alist-key key)
            (push (list alist-key key) calls)
            (eq key alist-key)))
   (nreverse calls)
   (condition-case err
       (assoc 'missing
              '((hay . 1) loose . bad-tail)
              (lambda (alist-key key)
                (push (list alist-key key) calls)
                (eq key alist-key)))
     (error (list (car err) (cdr err))))))
"#;

    let expect = expect_test::expect![[
        r#""OK ((needle . 2) ((hay needle) (needle needle)) (wrong-type-argument (listp ((hay . 1) loose . bad-tail))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_alist_lookup_circular_lists() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((alist (list '(a . 1) 'loose '(b . 2)))
       (_ (setcdr (last alist) alist)))
  (list
   (assq 'b alist)
   (assoc 'b alist)
   (rassq 2 alist)
   (rassoc 2 alist)))
"#;

    let expect = expect_test::expect![[r#""OK ((b . 2) (b . 2) (b . 2) (b . 2))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
