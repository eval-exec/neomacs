//! Strict combo oracle probes, batch 240: pcase deep patterns. Pattern guard
//! with app + and + or + let, function-arg pred patterns, rx-pattern inside
//! pcase, and pcase-exhaustive / pcase--match semantics via return values.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_pcase_app_and_or_guard_combined() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (pcase "hello"
        ((and (pred stringp) s (app length 5)) (list 'five s))
        (_ 'other))
      (pcase 42
        ((or 1 2 3) 'small)
        ((and (pred integerp) x (guard (> x 10))) 'big-int)
        (_ 'rest))
      (pcase '("a" "b" "c")
        (`(,a ,b ,c) (list a b c))
        (_ 'not-list))
      (pcase '(foo 1 2 3)
        (`(foo ,(and (pred numberp) a) ,(and (pred numberp) b)) (+ a b))
        (_ 'no-match))
      (pcase "test"
        ((app (lambda (s) (length s)) 4) 'len4)
        (_ 'other-len)))
"##;
    let expect = expect_test::expect![[
        r#""OK ((five \"hello\") big-int (\"a\" \"b\" \"c\") no-match len4)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_pcase_pred_function_partial_application_patterns() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (pcase 100 ((pred (> 50)) 'gt50) (_ 'le50))
      (pcase 3 ((pred (< 10)) 'lt10) (_ 'ge10))
      (pcase "x@y.z" ((pred (lambda (s) (string-match-p "@" s))) 'has-at) (_ 'no))
      (pcase '(1 2 3) ((pred (lambda (l) (> (length l) 2))) 'long) (_ 'short))
      (pcase '() ((or (pred null) `(,_ . ,_)) 'empty-or-cons))
      (pcase '(1) ((or (pred null) `(,_ . ,_)) 'empty-or-cons)))
"##;
    let expect =
        expect_test::expect![[r#""OK (le50 ge10 has-at long empty-or-cons empty-or-cons)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_pcase_let_with_nested_destructure_and_clauses() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (pcase-let ((`(,a ,b ,c) (list 1 2 3))) (list a b c))
      (pcase-let* ((`(,a ,b) '(10 20)) (`(,c (+ ,a ,b)) '(3 30))) (list a b c))
      (pcase '((1 . one) (2 . two))
        ((and `(,k . ,v) (guard (> k 1))) v)
        (_ 'first))
      (pcase '(group 1 2 3)
        (`(group . ,rest) rest)
        (_ 'no))
      (pcase '(:ok 42)
        (`(:ok ,code) code)
        (`(:err ,msg) msg)))
"##;
    let expect =
        expect_test::expect![[r#""ERR (wrong-type-argument number-or-marker-p (1 . one))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
