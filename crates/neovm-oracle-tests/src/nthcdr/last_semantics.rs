//! Oracle parity tests for GNU `nthcdr`, `nth`, and `last` edge semantics.
//!
//! GNU implements `nthcdr`/`nth` in `src/fns.c`; negative N returns the input
//! list unchanged, and improper-list errors report the original list object.
//! GNU implements `last` in `lisp/subr.el`, where negative N returns nil.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_nthcdr_negative_and_oversized_counts() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((lst '(a b c)))
  (list (nthcdr -3 lst)
        (eq (nthcdr -1 lst) lst)
        (nthcdr 0 lst)
        (nthcdr 99 lst)
        (nth -2 lst)
        (nth 99 lst)))
"#;

    let expect = expect_test::expect![[r#""OK ((a b c) t (a b c) nil a nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_nthcdr_improper_list_error_payloads() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (condition-case err
     (nthcdr 0 '(a b . c))
   (error (list (car err) (cdr err))))
 (condition-case err
     (nthcdr 1 '(a b . c))
   (error (list (car err) (cdr err))))
 (condition-case err
     (nthcdr 2 '(a b . c))
   (error (list (car err) (cdr err))))
 (condition-case err
     (nthcdr 3 '(a b . c))
   (error (list (car err) (cdr err))))
 (condition-case err
     (nth 3 '(a b . c))
   (error (list (car err) (cdr err)))))
"#;

    let expect = expect_test::expect![[
        r#""OK ((a b . c) (b . c) c (wrong-type-argument (listp (a b . c))) (wrong-type-argument (listp (a b . c))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_nthcdr_argument_type_error_payloads() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (condition-case err
     (nthcdr 'x '(a b))
   (error (list (car err) (cdr err))))
 (condition-case err
     (nthcdr 1 42)
   (error (list (car err) (cdr err))))
 (condition-case err
     (nth 'x '(a b))
   (error (list (car err) (cdr err)))))
"#;

    let expect = expect_test::expect![[
        r#""OK ((wrong-type-argument (integerp x)) (wrong-type-argument (listp 42)) (wrong-type-argument (integerp x)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_nthcdr_circular_large_and_bignum_counts() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((x (list 'a 'b 'c))
       (_ (setcdr (last x) x)))
  (list (eq (nthcdr 3 x) x)
        (car (nthcdr 4 x))
        (car (nthcdr 100000 x))
        (car (nthcdr 100000000000000000000000000000000000001 x))
        (nth 100000 x)
        (nth 100000000000000000000000000000000000001 x)))
"#;

    let expect = expect_test::expect![[r#""OK (t b b c b c)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_last_negative_zero_and_improper_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (last '(a b c) -1)
 (last '(a b c) 0)
 (last '(a b c) 99)
 (condition-case err
     (last '(a b . c))
   (error (list (car err) (cdr err))))
 (condition-case err
     (last '(a b . c) 1)
   (error (list (car err) (cdr err)))))
"#;

    let expect = expect_test::expect![[r#""OK (nil nil (a b c) (b . c) (b . c))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
