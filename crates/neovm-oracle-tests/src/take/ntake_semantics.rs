//! Oracle parity tests for GNU `take`/`ntake` edge semantics.
//!
//! GNU implements these in `src/fns.c`.  `take` allocates a fresh prefix for
//! positive N, even when N covers the whole list; `ntake` mutates and returns
//! the original list for positive N, or nil for N <= 0.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_take_fresh_prefix_and_large_n_copy() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((lst (list 'a 'b 'c))
       (small (take 2 lst))
       (large (take 99 lst)))
  (setcar small 'changed-small)
  (setcar large 'changed-large)
  (list small
        large
        lst
        (eq small lst)
        (eq large lst)
        (eq (cdr large) (cdr lst))))
"#;

    let expect = expect_test::expect![[
        r#""OK ((changed-small b) (changed-large b c) (a b c) nil nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_take_negative_zero_type_and_improper_errors() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (take -1 '(a b c))
 (take 0 '(a b c))
 (condition-case err
     (take 'x '(a b c))
   (error (list (car err) (cdr err))))
 (condition-case err
     (take 2 '(a b . c))
   (error (list (car err) (cdr err))))
 (condition-case err
     (take 3 '(a b . c))
   (error (list (car err) (cdr err))))
 (condition-case err
     (take 1 42)
   (error (list (car err) (cdr err)))))
"#;

    let expect = expect_test::expect![[
        r#""OK (nil nil (wrong-type-argument (integerp x)) (a b) (wrong-type-argument (listp c)) (wrong-type-argument (listp 42)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_ntake_destructive_identity_and_large_n() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((lst (list 1 2 3 4))
       (first lst)
       (short (ntake 2 lst))
       (large-list (list 'a 'b 'c))
       (large (ntake 99 large-list)))
  (list short
        lst
        (eq short first)
        large
        large-list
        (eq large large-list)))
"#;

    let expect = expect_test::expect![[r#""OK ((1 2) (1 2) t (a b c) (a b c) t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_ntake_negative_zero_and_error_payloads() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((lst (list 'a 'b 'c)))
  (list
   (ntake -1 lst)
   lst
   (ntake 0 lst)
   lst
   (condition-case err
       (ntake 'x lst)
     (error (list (car err) (cdr err))))
   (condition-case err
       (ntake 2 '(a b . c))
     (error (list (car err) (cdr err))))
   (condition-case err
       (ntake 3 '(a b . c))
     (error (list (car err) (cdr err))))
   (condition-case err
       (ntake 1 42)
     (error (list (car err) (cdr err)))))))
"#;

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 19 43)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
