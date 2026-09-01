//! Strict combo oracle probes, batch 166: pcase deep. or/and patterns, pred
//! + guard predicates with binding, backquote destructuring (proper + dotted),
//! app (transform) pattern, pcase-let / pcase-let*, and rx-pattern inside
//! pcase.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_pcase_or_and_pred_guard_backquote() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (pcase 5 ((or 1 2 3) 'small) ((or 4 5 6) 'mid) (_ 'other))
      (pcase "hello" ((pred stringp) 'str) (_ 'no))
      (pcase 10 ((and (pred integerp) x) (guard (> x 5)) 'big) (_ 'small))
      (pcase '(1 2 3) (`(,a . ,rest) (list a rest)))
      (pcase '(1 2) (`(,a ,b) (+ a b)))
      (pcase '(foo 42) (`(foo ,(and n (pred numberp))) n))
      (pcase '(foo "x") (`(foo ,(and s (pred stringp))) s))
      (pcase 3 ((app (lambda (x) (* x 2)) doubled) doubled))
      (pcase '() ((or (pred null) `(,a . ,_)) 'empty-or-list)))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function guard)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_pcase_let_let_star_multiple() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (pcase-let ((`(,a ,b) '(1 2)) (`(,c ,d) '(3 4)))
        (list a b c d))
      (pcase-let* ((`(,a ,b) '(1 2)) (`(,c (+ ,a ,b)) '(3 3)))
        (list a b c))
      (pcase '(1 (2 3) 4)
        (`(,x (,y ,z) ,w) (list x y z w)))
      (pcase '((a . 1) (b . 2))
        ((or `(a . ,av) (and _ (let av 'fallback))) av))
      (pcase 7
        ((and (pred integerp) n (guard (zerop (% n 2)))) 'even)
        (_ 'odd)))
"##;
    let expect = expect_test::expect![[r#""OK ((1 2 3 4) (nil nil 3) (1 2 3 4) fallback odd)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_pcase_pred_functions_and_maps() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (pcase "key" ((pred (string-match "ke")) 'matched-prefix) (_ 'no))
      (pcase "test@example.com" ((pred (lambda (s) (string-match-p "@" s))) 'has-at) (_ 'no))
      (pcase 100 ((pred (> 50)) 'gt-50) (_ 'le-50))
      (pcase '(a b c d e) (`(,_ ,_ ,mid . ,_) mid))
      (pcase '(1 2 3) ((pred (lambda (l) (> (length l) 2))) 'long) (_ 'short))
      (mapcar (lambda (x)
                (pcase x
                  ((pred numberp) 'num)
                  ((pred stringp) 'str)
                  ((pred listp) 'list)))
              '(42 "hi" (a b))))
"##;
    let expect =
        expect_test::expect![[r#""OK (matched-prefix has-at le-50 c long (num str list))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
