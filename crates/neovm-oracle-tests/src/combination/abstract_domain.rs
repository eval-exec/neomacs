//! Oracle parity tests for abstract domain analysis in Elisp:
//! interval domain with widening/narrowing, sign domain, congruence domain,
//! reduced product of domains, abstract interpretation of loops with
//! widening, domain meet/join operations, transfer functions for arithmetic.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Interval domain: representation, join, meet, widening, narrowing
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_abstract_domain_interval_lattice_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Interval domain: (lo . hi) where lo/hi are integers or 'neginf/'posinf.
    // bot = nil, top = (neginf . posinf).
    let form = r#"(progn
  ;; Comparison helpers for extended integers
  (fset 'neovm--iad-le
    (lambda (a b)
      (cond ((eq a 'neginf) t)
            ((eq b 'posinf) t)
            ((eq b 'neginf) nil)
            ((eq a 'posinf) nil)
            (t (<= a b)))))

  (fset 'neovm--iad-min
    (lambda (a b)
      (if (funcall 'neovm--iad-le a b) a b)))

  (fset 'neovm--iad-max
    (lambda (a b)
      (if (funcall 'neovm--iad-le a b) b a)))

  ;; Join: smallest interval containing both
  (fset 'neovm--iad-join
    (lambda (a b)
      (cond ((null a) b)
            ((null b) a)
            (t (cons (funcall 'neovm--iad-min (car a) (car b))
                     (funcall 'neovm--iad-max (cdr a) (cdr b)))))))

  ;; Meet: intersection, or nil if empty
  (fset 'neovm--iad-meet
    (lambda (a b)
      (cond ((null a) nil)
            ((null b) nil)
            (t (let ((lo (funcall 'neovm--iad-max (car a) (car b)))
                     (hi (funcall 'neovm--iad-min (cdr a) (cdr b))))
                 (if (funcall 'neovm--iad-le lo hi)
                     (cons lo hi)
                   nil))))))

  ;; Widening: if bounds changed, push to infinity
  (fset 'neovm--iad-widen
    (lambda (old new)
      (cond ((null old) new)
            ((null new) nil)
            (t (cons (if (and (not (eq (car old) 'neginf))
                              (not (funcall 'neovm--iad-le (car old) (car new))))
                         'neginf (car old))
                     (if (and (not (eq (cdr old) 'posinf))
                              (not (funcall 'neovm--iad-le (cdr new) (cdr old))))
                         'posinf (cdr old)))))))

  ;; Narrowing: tighten from infinity toward new bounds
  (fset 'neovm--iad-narrow
    (lambda (old new)
      (cond ((null old) nil)
            ((null new) nil)
            (t (cons (if (eq (car old) 'neginf) (car new) (car old))
                     (if (eq (cdr old) 'posinf) (cdr new) (cdr old)))))))

  (unwind-protect
      (list
       ;; Join tests
       (funcall 'neovm--iad-join '(1 . 5) '(3 . 8))        ;; (1 . 8)
       (funcall 'neovm--iad-join '(1 . 5) '(10 . 20))      ;; (1 . 20)
       (funcall 'neovm--iad-join nil '(3 . 7))              ;; (3 . 7)
       (funcall 'neovm--iad-join '(-5 . 5) nil)             ;; (-5 . 5)
       (funcall 'neovm--iad-join '(neginf . 0) '(0 . posinf)) ;; (neginf . posinf)

       ;; Meet tests
       (funcall 'neovm--iad-meet '(1 . 10) '(5 . 15))      ;; (5 . 10)
       (funcall 'neovm--iad-meet '(1 . 5) '(10 . 20))      ;; nil (disjoint)
       (funcall 'neovm--iad-meet '(1 . 10) '(1 . 10))      ;; (1 . 10)
       (funcall 'neovm--iad-meet nil '(1 . 5))              ;; nil
       (funcall 'neovm--iad-meet '(neginf . 5) '(0 . posinf)) ;; (0 . 5)

       ;; Widening tests
       (funcall 'neovm--iad-widen '(0 . 5) '(0 . 10))      ;; (0 . posinf)
       (funcall 'neovm--iad-widen '(0 . 5) '(-1 . 5))      ;; (neginf . 5)
       (funcall 'neovm--iad-widen '(0 . 5) '(-1 . 10))     ;; (neginf . posinf)
       (funcall 'neovm--iad-widen '(0 . 5) '(0 . 5))       ;; (0 . 5) unchanged
       (funcall 'neovm--iad-widen nil '(3 . 7))             ;; (3 . 7)

       ;; Narrowing tests
       (funcall 'neovm--iad-narrow '(neginf . posinf) '(0 . 100)) ;; (0 . 100)
       (funcall 'neovm--iad-narrow '(neginf . 10) '(0 . 5))      ;; (0 . 10)
       (funcall 'neovm--iad-narrow '(0 . posinf) '(-5 . 50))     ;; (0 . 50)
       (funcall 'neovm--iad-narrow '(0 . 10) '(0 . 10))          ;; (0 . 10)
       )
    (fmakunbound 'neovm--iad-le)
    (fmakunbound 'neovm--iad-min)
    (fmakunbound 'neovm--iad-max)
    (fmakunbound 'neovm--iad-join)
    (fmakunbound 'neovm--iad-meet)
    (fmakunbound 'neovm--iad-widen)
    (fmakunbound 'neovm--iad-narrow)))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 . 8) (1 . 20) (3 . 7) (-5 . 5) (neginf . posinf) (5 . 10) nil (1 . 10) nil (0 . 5) (0 . posinf) (neginf . 5) (neginf . posinf) (0 . 5) (3 . 7) (0 . 100) (0 . 10) (0 . 50) (0 . 10))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Sign domain with full transfer functions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_abstract_domain_sign_transfer_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Sign domain: bot, neg, zero, pos, top
  ;; Full transfer functions for add, sub, mul, div, mod, abs, negate
  (fset 'neovm--sd-join
    (lambda (a b)
      (cond ((eq a 'bot) b) ((eq b 'bot) a)
            ((eq a 'top) 'top) ((eq b 'top) 'top)
            ((eq a b) a) (t 'top))))

  (fset 'neovm--sd-meet
    (lambda (a b)
      (cond ((eq a 'top) b) ((eq b 'top) a)
            ((eq a 'bot) 'bot) ((eq b 'bot) 'bot)
            ((eq a b) a) (t 'bot))))

  (fset 'neovm--sd-add
    (lambda (a b)
      (cond ((or (eq a 'bot) (eq b 'bot)) 'bot)
            ((or (eq a 'top) (eq b 'top)) 'top)
            ((eq a 'zero) b) ((eq b 'zero) a)
            ((and (eq a 'pos) (eq b 'pos)) 'pos)
            ((and (eq a 'neg) (eq b 'neg)) 'neg)
            (t 'top))))

  (fset 'neovm--sd-negate
    (lambda (a)
      (cond ((eq a 'pos) 'neg) ((eq a 'neg) 'pos)
            ((eq a 'zero) 'zero) (t a))))

  (fset 'neovm--sd-sub
    (lambda (a b)
      (funcall 'neovm--sd-add a (funcall 'neovm--sd-negate b))))

  (fset 'neovm--sd-mul
    (lambda (a b)
      (cond ((or (eq a 'bot) (eq b 'bot)) 'bot)
            ((or (eq a 'zero) (eq b 'zero)) 'zero)
            ((or (eq a 'top) (eq b 'top)) 'top)
            ((and (eq a 'pos) (eq b 'pos)) 'pos)
            ((and (eq a 'neg) (eq b 'neg)) 'pos)
            (t 'neg))))

  (fset 'neovm--sd-abs
    (lambda (a)
      (cond ((eq a 'bot) 'bot) ((eq a 'neg) 'pos)
            ((eq a 'zero) 'zero) ((eq a 'pos) 'pos)
            ((eq a 'top) 'top))))

  (fset 'neovm--sd-div
    (lambda (a b)
      (cond ((or (eq a 'bot) (eq b 'bot)) 'bot)
            ((eq b 'zero) 'bot)
            ((eq a 'zero) 'zero)
            ((or (eq a 'top) (eq b 'top)) 'top)
            ((and (eq a 'pos) (eq b 'pos)) 'top)  ;; could be zero
            ((and (eq a 'neg) (eq b 'neg)) 'top)
            (t 'top))))

  (unwind-protect
      (list
       ;; Arithmetic transfer functions
       (funcall 'neovm--sd-add 'pos 'pos)    ;; pos
       (funcall 'neovm--sd-add 'neg 'neg)    ;; neg
       (funcall 'neovm--sd-add 'pos 'neg)    ;; top
       (funcall 'neovm--sd-sub 'pos 'neg)    ;; pos
       (funcall 'neovm--sd-sub 'neg 'pos)    ;; neg
       (funcall 'neovm--sd-sub 'pos 'pos)    ;; top
       (funcall 'neovm--sd-mul 'neg 'neg)    ;; pos
       (funcall 'neovm--sd-mul 'pos 'neg)    ;; neg
       (funcall 'neovm--sd-mul 'zero 'top)   ;; zero
       (funcall 'neovm--sd-negate 'pos)      ;; neg
       (funcall 'neovm--sd-negate 'neg)      ;; pos
       (funcall 'neovm--sd-negate 'zero)     ;; zero
       (funcall 'neovm--sd-abs 'neg)         ;; pos
       (funcall 'neovm--sd-abs 'top)         ;; top
       (funcall 'neovm--sd-div 'pos 'zero)   ;; bot
       (funcall 'neovm--sd-div 'pos 'pos)    ;; top (could be zero if a<b)
       ;; Lattice properties
       (funcall 'neovm--sd-join 'bot 'pos)   ;; pos
       (funcall 'neovm--sd-join 'pos 'neg)   ;; top
       (funcall 'neovm--sd-meet 'top 'neg)   ;; neg
       (funcall 'neovm--sd-meet 'pos 'neg)   ;; bot
       ;; Idempotence
       (funcall 'neovm--sd-join 'pos 'pos)   ;; pos
       (funcall 'neovm--sd-meet 'neg 'neg))  ;; neg
    (fmakunbound 'neovm--sd-join)
    (fmakunbound 'neovm--sd-meet)
    (fmakunbound 'neovm--sd-add)
    (fmakunbound 'neovm--sd-negate)
    (fmakunbound 'neovm--sd-sub)
    (fmakunbound 'neovm--sd-mul)
    (fmakunbound 'neovm--sd-abs)
    (fmakunbound 'neovm--sd-div)))"#;
    let expect = expect_test::expect![[
        r#""OK (pos neg top pos neg top pos neg zero neg pos zero pos top bot top pos top neg bot pos neg)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Congruence domain: mod-k analysis
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_abstract_domain_congruence_domain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Congruence domain: (mod . rem) means value = rem (mod mod)
    // top = (0 . 0) meaning any value, bot = nil
    let form = r#"(progn
  ;; A congruence is (mod . rem), where mod=0 means top
  ;; bot is nil
  (fset 'neovm--cd-make (lambda (m r) (if (= m 0) '(0 . 0) (cons m (mod r m)))))
  (fset 'neovm--cd-is-bot (lambda (c) (null c)))
  (fset 'neovm--cd-is-top (lambda (c) (and c (= (car c) 0))))

  ;; GCD helper
  (fset 'neovm--cd-gcd
    (lambda (a b)
      (let ((a (abs a)) (b (abs b)))
        (while (/= b 0)
          (let ((t1 b))
            (setq b (mod a b))
            (setq a t1)))
        a)))

  ;; Join: gcd of moduli
  (fset 'neovm--cd-join
    (lambda (a b)
      (cond ((null a) b) ((null b) a)
            ((funcall 'neovm--cd-is-top a) a)
            ((funcall 'neovm--cd-is-top b) b)
            (t (let ((g (funcall 'neovm--cd-gcd (car a) (car b))))
                 (if (= g 1)
                     '(0 . 0)  ;; top
                   (let ((g2 (funcall 'neovm--cd-gcd g (abs (- (cdr a) (cdr b))))))
                     (if (= g2 0) '(0 . 0)
                       (cons g2 (mod (cdr a) g2))))))))))

  ;; Meet: lcm of moduli if remainders compatible
  (fset 'neovm--cd-meet
    (lambda (a b)
      (cond ((null a) nil) ((null b) nil)
            ((funcall 'neovm--cd-is-top a) b)
            ((funcall 'neovm--cd-is-top b) a)
            (t (let* ((g (funcall 'neovm--cd-gcd (car a) (car b))))
                 (if (= (mod (- (cdr a) (cdr b)) g) 0)
                     (let ((l (/ (* (car a) (car b)) g)))
                       (cons l (mod (cdr a) l)))
                   nil))))))

  ;; Add congruences: (m1.r1) + (m2.r2) = (gcd(m1,m2) . (r1+r2) mod gcd(m1,m2))
  (fset 'neovm--cd-add
    (lambda (a b)
      (cond ((or (null a) (null b)) nil)
            ((funcall 'neovm--cd-is-top a) '(0 . 0))
            ((funcall 'neovm--cd-is-top b) '(0 . 0))
            (t (let ((g (funcall 'neovm--cd-gcd (car a) (car b))))
                 (if (= g 0) '(0 . 0)
                   (cons g (mod (+ (cdr a) (cdr b)) g))))))))

  ;; Mul congruences: (m1.r1) * (m2.r2)
  (fset 'neovm--cd-mul
    (lambda (a b)
      (cond ((or (null a) (null b)) nil)
            ((funcall 'neovm--cd-is-top a) '(0 . 0))
            ((funcall 'neovm--cd-is-top b) '(0 . 0))
            (t (let ((g (funcall 'neovm--cd-gcd
                          (funcall 'neovm--cd-gcd (* (car a) (car b))
                                   (* (cdr a) (car b)))
                          (* (car a) (cdr b)))))
                 (if (= g 0) (cons 0 (* (cdr a) (cdr b)))
                   (cons g (mod (* (cdr a) (cdr b)) g))))))))

  (unwind-protect
      (list
       ;; Representation
       (funcall 'neovm--cd-make 4 7)           ;; (4 . 3) since 7 mod 4 = 3
       (funcall 'neovm--cd-make 3 6)           ;; (3 . 0) since 6 mod 3 = 0
       (funcall 'neovm--cd-make 0 0)           ;; (0 . 0) top

       ;; Join: x=1(mod2) join x=3(mod4) = gcd(2,4)=2, |1-3|=2, gcd(2,2)=2 -> (2.1)
       (funcall 'neovm--cd-join '(2 . 1) '(4 . 3))
       ;; Join: x=0(mod3) join x=0(mod5) -> gcd(3,5)=1 -> top
       (funcall 'neovm--cd-join '(3 . 0) '(5 . 0))
       ;; Join with bot
       (funcall 'neovm--cd-join nil '(4 . 2))
       (funcall 'neovm--cd-join '(6 . 3) nil)

       ;; Meet: x=1(mod2) meet x=1(mod3) -> lcm=6, (6.1)
       (funcall 'neovm--cd-meet '(2 . 1) '(3 . 1))
       ;; Meet: x=0(mod4) meet x=2(mod4) -> incompatible -> bot
       (funcall 'neovm--cd-meet '(4 . 0) '(4 . 2))
       ;; Meet with top
       (funcall 'neovm--cd-meet '(0 . 0) '(3 . 1))

       ;; Addition
       (funcall 'neovm--cd-add '(2 . 0) '(2 . 1))  ;; even + odd = (2.1) odd
       (funcall 'neovm--cd-add '(3 . 1) '(3 . 2))  ;; (3.0) since 1+2=3 mod 3=0

       ;; Multiplication
       (funcall 'neovm--cd-mul '(2 . 0) '(3 . 0))  ;; even * mult-of-3 = (0.0)?
       )
    (fmakunbound 'neovm--cd-make)
    (fmakunbound 'neovm--cd-is-bot)
    (fmakunbound 'neovm--cd-is-top)
    (fmakunbound 'neovm--cd-gcd)
    (fmakunbound 'neovm--cd-join)
    (fmakunbound 'neovm--cd-meet)
    (fmakunbound 'neovm--cd-add)
    (fmakunbound 'neovm--cd-mul)))"#;
    let expect = expect_test::expect![[
        r#""OK ((4 . 3) (3 . 0) (0 . 0) (2 . 1) (0 . 0) (4 . 2) (6 . 3) (6 . 1) nil (3 . 1) (2 . 1) (3 . 0) (6 . 0))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Reduced product of sign and interval domains
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_abstract_domain_reduced_product() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Reduced product: (sign . interval). After each operation,
    // reduce by using sign info to narrow interval and vice versa.
    let form = r#"(progn
  ;; Sign helpers
  (fset 'neovm--rp-sign-of-interval
    (lambda (itv)
      (cond ((null itv) 'bot)
            ((and (numberp (car itv)) (> (car itv) 0)) 'pos)
            ((and (numberp (cdr itv)) (< (cdr itv) 0)) 'neg)
            ((and (equal (car itv) 0) (equal (cdr itv) 0)) 'zero)
            (t 'top))))

  (fset 'neovm--rp-interval-of-sign
    (lambda (s)
      (cond ((eq s 'bot) nil)
            ((eq s 'pos) '(1 . posinf))
            ((eq s 'neg) '(neginf . -1))
            ((eq s 'zero) '(0 . 0))
            (t '(neginf . posinf)))))

  ;; Extended int comparison
  (fset 'neovm--rp-le
    (lambda (a b)
      (cond ((eq a 'neginf) t) ((eq b 'posinf) t)
            ((eq b 'neginf) nil) ((eq a 'posinf) nil)
            (t (<= a b)))))
  (fset 'neovm--rp-max2
    (lambda (a b) (if (funcall 'neovm--rp-le a b) b a)))
  (fset 'neovm--rp-min2
    (lambda (a b) (if (funcall 'neovm--rp-le a b) a b)))

  ;; Interval meet
  (fset 'neovm--rp-itv-meet
    (lambda (a b)
      (cond ((null a) nil) ((null b) nil)
            (t (let ((lo (funcall 'neovm--rp-max2 (car a) (car b)))
                     (hi (funcall 'neovm--rp-min2 (cdr a) (cdr b))))
                 (if (funcall 'neovm--rp-le lo hi) (cons lo hi) nil))))))

  ;; Sign meet
  (fset 'neovm--rp-sign-meet
    (lambda (a b)
      (cond ((eq a 'top) b) ((eq b 'top) a)
            ((eq a 'bot) 'bot) ((eq b 'bot) 'bot)
            ((eq a b) a) (t 'bot))))

  ;; Reduce: tighten both components using info from the other
  (fset 'neovm--rp-reduce
    (lambda (pair)
      (let* ((s (car pair)) (i (cdr pair))
             ;; Derive sign from interval
             (s2 (funcall 'neovm--rp-sign-meet s (funcall 'neovm--rp-sign-of-interval i)))
             ;; Derive interval from sign
             (i2 (funcall 'neovm--rp-itv-meet i (funcall 'neovm--rp-interval-of-sign s2))))
        (if (or (eq s2 'bot) (null i2))
            '(bot . nil)
          (cons s2 i2)))))

  (unwind-protect
      (list
       ;; pos with [5,10] -> reduces to (pos . (5 . 10))
       (funcall 'neovm--rp-reduce (cons 'pos '(5 . 10)))
       ;; neg with [-100, 50] -> sign restricts interval to [-100, -1]
       (funcall 'neovm--rp-reduce (cons 'neg '(-100 . 50)))
       ;; zero with [-5, 5] -> (zero . (0 . 0))
       (funcall 'neovm--rp-reduce (cons 'zero '(-5 . 5)))
       ;; pos with [-10, -1] -> contradiction -> bot
       (funcall 'neovm--rp-reduce (cons 'pos '(-10 . -1)))
       ;; top with [3, 7] -> sign becomes pos (since lo>0)
       (funcall 'neovm--rp-reduce (cons 'top '(3 . 7)))
       ;; top with [-3, 3] -> top (spans zero)
       (funcall 'neovm--rp-reduce (cons 'top '(-3 . 3)))
       ;; neg with [0, 0] -> contradiction -> bot
       (funcall 'neovm--rp-reduce (cons 'neg '(0 . 0)))
       ;; pos with (neginf . posinf) -> (pos . (1 . posinf))
       (funcall 'neovm--rp-reduce (cons 'pos '(neginf . posinf))))
    (fmakunbound 'neovm--rp-sign-of-interval)
    (fmakunbound 'neovm--rp-interval-of-sign)
    (fmakunbound 'neovm--rp-le)
    (fmakunbound 'neovm--rp-max2)
    (fmakunbound 'neovm--rp-min2)
    (fmakunbound 'neovm--rp-itv-meet)
    (fmakunbound 'neovm--rp-sign-meet)
    (fmakunbound 'neovm--rp-reduce)))"#;
    let expect = expect_test::expect![[
        r#""OK ((pos 5 . 10) (neg -100 . -1) (zero 0 . 0) (bot) (pos 3 . 7) (top -3 . 3) (bot) (pos 1 . posinf))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Abstract interpretation of loops with widening + narrowing
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_abstract_domain_loop_widening_narrowing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Interval domain helpers
  (fset 'neovm--lw-le
    (lambda (a b) (cond ((eq a 'neginf) t) ((eq b 'posinf) t) ((eq b 'neginf) nil) ((eq a 'posinf) nil) (t (<= a b)))))
  (fset 'neovm--lw-min2
    (lambda (a b) (if (funcall 'neovm--lw-le a b) a b)))
  (fset 'neovm--lw-max2
    (lambda (a b) (if (funcall 'neovm--lw-le a b) b a)))

  (fset 'neovm--lw-join
    (lambda (a b)
      (cond ((null a) b) ((null b) a)
            (t (cons (funcall 'neovm--lw-min2 (car a) (car b))
                     (funcall 'neovm--lw-max2 (cdr a) (cdr b)))))))

  (fset 'neovm--lw-widen
    (lambda (old new)
      (cond ((null old) new) ((null new) nil)
            (t (cons (if (funcall 'neovm--lw-le (car old) (car new)) (car old) 'neginf)
                     (if (funcall 'neovm--lw-le (cdr new) (cdr old)) (cdr old) 'posinf))))))

  (fset 'neovm--lw-narrow
    (lambda (old new)
      (cond ((null old) nil) ((null new) nil)
            (t (cons (if (eq (car old) 'neginf) (car new) (car old))
                     (if (eq (cdr old) 'posinf) (cdr new) (cdr old)))))))

  (fset 'neovm--lw-eq
    (lambda (a b)
      (cond ((and (null a) (null b)) t)
            ((or (null a) (null b)) nil)
            (t (and (equal (car a) (car b)) (equal (cdr a) (cdr b)))))))

  ;; State ops (alist of var -> interval)
  (fset 'neovm--lw-get (lambda (s v) (let ((p (assq v s))) (if p (cdr p) nil))))
  (fset 'neovm--lw-set
    (lambda (s v val)
      (cons (cons v val) (let ((r nil)) (dolist (p s) (unless (eq (car p) v) (setq r (cons p r)))) (nreverse r)))))

  ;; Interval addition (simple: just add bounds)
  (fset 'neovm--lw-iadd
    (lambda (a b)
      (cond ((or (null a) (null b)) nil)
            (t (let ((lo (if (or (eq (car a) 'neginf) (eq (car b) 'neginf)) 'neginf (+ (car a) (car b))))
                     (hi (if (or (eq (cdr a) 'posinf) (eq (cdr b) 'posinf)) 'posinf (+ (cdr a) (cdr b)))))
                 (cons lo hi))))))

  ;; State-level widening
  (fset 'neovm--lw-state-widen
    (lambda (old new)
      (let ((vars nil))
        (dolist (p old) (unless (memq (car p) vars) (setq vars (cons (car p) vars))))
        (dolist (p new) (unless (memq (car p) vars) (setq vars (cons (car p) vars))))
        (let ((r nil))
          (dolist (v vars)
            (setq r (cons (cons v (funcall 'neovm--lw-widen
                                           (funcall 'neovm--lw-get old v)
                                           (funcall 'neovm--lw-get new v))) r)))
          r))))

  (fset 'neovm--lw-state-eq
    (lambda (s1 s2)
      (let ((vars nil) (res t))
        (dolist (p s1) (unless (memq (car p) vars) (setq vars (cons (car p) vars))))
        (dolist (p s2) (unless (memq (car p) vars) (setq vars (cons (car p) vars))))
        (dolist (v vars)
          (unless (funcall 'neovm--lw-eq (funcall 'neovm--lw-get s1 v) (funcall 'neovm--lw-get s2 v))
            (setq res nil)))
        res)))

  ;; Analyze: for (init; cond; body), iterate with widening then narrowing
  (fset 'neovm--lw-analyze
    (lambda (init body-fn max-w max-n)
      (let ((st init) (converged nil) (iters 0))
        ;; Widening phase
        (while (and (not converged) (< iters max-w))
          (setq iters (1+ iters))
          (let* ((next (funcall body-fn st))
                 (wide (funcall 'neovm--lw-state-widen st next)))
            (if (funcall 'neovm--lw-state-eq wide st)
                (setq converged t)
              (setq st wide))))
        (let ((w-iters iters) (w-state st))
          ;; Narrowing phase
          (setq iters 0 converged nil)
          (while (and (not converged) (< iters max-n))
            (setq iters (1+ iters))
            (let* ((next (funcall body-fn st))
                   (vars nil))
              (dolist (p st) (unless (memq (car p) vars) (setq vars (cons (car p) vars))))
              (let ((new-st nil))
                (dolist (v vars)
                  (setq new-st (cons (cons v (funcall 'neovm--lw-narrow
                                                      (funcall 'neovm--lw-get st v)
                                                      (funcall 'neovm--lw-get next v))) new-st)))
                (if (funcall 'neovm--lw-state-eq new-st st)
                    (setq converged t)
                  (setq st new-st)))))
          (list 'widening-iters w-iters 'narrowing-iters iters 'final st)))))

  (unwind-protect
      (list
       ;; Loop: x=0; while(x<100): x=x+1
       ;; Body: add [1,1] to x
       (funcall 'neovm--lw-analyze
         (list (cons 'x '(0 . 0)))
         (lambda (s) (funcall 'neovm--lw-set s 'x
                       (funcall 'neovm--lw-iadd (funcall 'neovm--lw-get s 'x) '(1 . 1))))
         10 5)

       ;; Loop: x=10; while(x>0): x=x-1
       (funcall 'neovm--lw-analyze
         (list (cons 'x '(10 . 10)))
         (lambda (s) (funcall 'neovm--lw-set s 'x
                       (funcall 'neovm--lw-iadd (funcall 'neovm--lw-get s 'x) '(-1 . -1))))
         10 5)

       ;; Two variables: x=0, y=100; x=x+1, y=y-1
       (funcall 'neovm--lw-analyze
         (list (cons 'x '(0 . 0)) (cons 'y '(100 . 100)))
         (lambda (s)
           (let ((s2 (funcall 'neovm--lw-set s 'x
                       (funcall 'neovm--lw-iadd (funcall 'neovm--lw-get s 'x) '(1 . 1)))))
             (funcall 'neovm--lw-set s2 'y
               (funcall 'neovm--lw-iadd (funcall 'neovm--lw-get s2 'y) '(-1 . -1)))))
         10 5))
    (fmakunbound 'neovm--lw-le)
    (fmakunbound 'neovm--lw-min2)
    (fmakunbound 'neovm--lw-max2)
    (fmakunbound 'neovm--lw-join)
    (fmakunbound 'neovm--lw-widen)
    (fmakunbound 'neovm--lw-narrow)
    (fmakunbound 'neovm--lw-eq)
    (fmakunbound 'neovm--lw-get)
    (fmakunbound 'neovm--lw-set)
    (fmakunbound 'neovm--lw-iadd)
    (fmakunbound 'neovm--lw-state-widen)
    (fmakunbound 'neovm--lw-state-eq)
    (fmakunbound 'neovm--lw-analyze)))"#;
    let expect = expect_test::expect![[
        r#""OK ((widening-iters 2 narrowing-iters 1 final ((x 0 . posinf))) (widening-iters 2 narrowing-iters 1 final ((x neginf . 10))) (widening-iters 2 narrowing-iters 1 final ((x 0 . posinf) (y neginf . 100))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Domain meet/join algebraic properties (lattice laws)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_abstract_domain_lattice_laws() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Test lattice algebraic laws on sign domain
  (fset 'neovm--ll-join
    (lambda (a b)
      (cond ((eq a 'bot) b) ((eq b 'bot) a)
            ((eq a 'top) 'top) ((eq b 'top) 'top)
            ((eq a b) a) (t 'top))))

  (fset 'neovm--ll-meet
    (lambda (a b)
      (cond ((eq a 'top) b) ((eq b 'top) a)
            ((eq a 'bot) 'bot) ((eq b 'bot) 'bot)
            ((eq a b) a) (t 'bot))))

  (let ((elems '(bot neg zero pos top)))
    (let ((commutativity-join t) (commutativity-meet t)
          (associativity-join t) (associativity-meet t)
          (absorption-1 t) (absorption-2 t)
          (idempotence-join t) (idempotence-meet t))
      ;; Check all pairs
      (dolist (a elems)
        (dolist (b elems)
          ;; Commutativity
          (unless (eq (funcall 'neovm--ll-join a b) (funcall 'neovm--ll-join b a))
            (setq commutativity-join nil))
          (unless (eq (funcall 'neovm--ll-meet a b) (funcall 'neovm--ll-meet b a))
            (setq commutativity-meet nil))
          ;; Idempotence
          (when (eq a b)
            (unless (eq (funcall 'neovm--ll-join a a) a)
              (setq idempotence-join nil))
            (unless (eq (funcall 'neovm--ll-meet a a) a)
              (setq idempotence-meet nil)))
          ;; Absorption: join(a, meet(a,b)) = a
          (unless (eq (funcall 'neovm--ll-join a (funcall 'neovm--ll-meet a b)) a)
            (setq absorption-1 nil))
          ;; Absorption: meet(a, join(a,b)) = a
          (unless (eq (funcall 'neovm--ll-meet a (funcall 'neovm--ll-join a b)) a)
            (setq absorption-2 nil))
          ;; Associativity (check one triple per pair)
          (dolist (c elems)
            (unless (eq (funcall 'neovm--ll-join (funcall 'neovm--ll-join a b) c)
                        (funcall 'neovm--ll-join a (funcall 'neovm--ll-join b c)))
              (setq associativity-join nil))
            (unless (eq (funcall 'neovm--ll-meet (funcall 'neovm--ll-meet a b) c)
                        (funcall 'neovm--ll-meet a (funcall 'neovm--ll-meet b c)))
              (setq associativity-meet nil)))))
      (list
       commutativity-join commutativity-meet
       associativity-join associativity-meet
       absorption-1 absorption-2
       idempotence-join idempotence-meet
       ;; Bot/top identity
       (eq (funcall 'neovm--ll-join 'bot 'pos) 'pos)
       (eq (funcall 'neovm--ll-join 'top 'pos) 'top)
       (eq (funcall 'neovm--ll-meet 'top 'neg) 'neg)
       (eq (funcall 'neovm--ll-meet 'bot 'neg) 'bot))))"#;
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Transfer functions for compound arithmetic expressions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_abstract_domain_compound_transfer_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Evaluate compound expressions in the interval domain
    let form = r#"(progn
  (fset 'neovm--ctf-le
    (lambda (a b) (cond ((eq a 'neginf) t) ((eq b 'posinf) t) ((eq b 'neginf) nil) ((eq a 'posinf) nil) (t (<= a b)))))
  (fset 'neovm--ctf-min2
    (lambda (a b) (if (funcall 'neovm--ctf-le a b) a b)))
  (fset 'neovm--ctf-max2
    (lambda (a b) (if (funcall 'neovm--ctf-le a b) b a)))

  ;; Interval arithmetic
  (fset 'neovm--ctf-iadd
    (lambda (a b)
      (cond ((or (null a) (null b)) nil)
            (t (cons (if (or (eq (car a) 'neginf) (eq (car b) 'neginf)) 'neginf (+ (car a) (car b)))
                     (if (or (eq (cdr a) 'posinf) (eq (cdr b) 'posinf)) 'posinf (+ (cdr a) (cdr b))))))))

  (fset 'neovm--ctf-isub
    (lambda (a b)
      (cond ((or (null a) (null b)) nil)
            (t (cons (if (or (eq (car a) 'neginf) (eq (cdr b) 'posinf)) 'neginf (- (car a) (cdr b)))
                     (if (or (eq (cdr a) 'posinf) (eq (car b) 'neginf)) 'posinf (- (cdr a) (car b))))))))

  (fset 'neovm--ctf-imul
    (lambda (a b)
      (cond ((or (null a) (null b)) nil)
            ;; For finite intervals, compute all 4 products, take min/max
            ((and (numberp (car a)) (numberp (cdr a)) (numberp (car b)) (numberp (cdr b)))
             (let* ((p1 (* (car a) (car b))) (p2 (* (car a) (cdr b)))
                    (p3 (* (cdr a) (car b))) (p4 (* (cdr a) (cdr b)))
                    (lo (min p1 p2 p3 p4)) (hi (max p1 p2 p3 p4)))
               (cons lo hi)))
            (t '(neginf . posinf)))))

  ;; Evaluate expr in a state: (+ e1 e2), (- e1 e2), (* e1 e2), (const lo hi), var
  (fset 'neovm--ctf-eval
    (lambda (state expr)
      (cond
       ((and (listp expr) (eq (car expr) 'const))
        (cons (cadr expr) (caddr expr)))
       ((symbolp expr)
        (let ((p (assq expr state))) (if p (cdr p) nil)))
       ((and (listp expr) (eq (car expr) '+))
        (funcall 'neovm--ctf-iadd (funcall 'neovm--ctf-eval state (cadr expr))
                                   (funcall 'neovm--ctf-eval state (caddr expr))))
       ((and (listp expr) (eq (car expr) '-))
        (funcall 'neovm--ctf-isub (funcall 'neovm--ctf-eval state (cadr expr))
                                   (funcall 'neovm--ctf-eval state (caddr expr))))
       ((and (listp expr) (eq (car expr) '*))
        (funcall 'neovm--ctf-imul (funcall 'neovm--ctf-eval state (cadr expr))
                                   (funcall 'neovm--ctf-eval state (caddr expr))))
       (t nil))))

  (unwind-protect
      (let ((state (list (cons 'x '(1 . 10)) (cons 'y '(-5 . 5)) (cons 'z '(0 . 100)))))
        (list
         ;; x + y = [1+(-5), 10+5] = [-4, 15]
         (funcall 'neovm--ctf-eval state '(+ x y))
         ;; x - y = [1-5, 10-(-5)] = [-4, 15]
         (funcall 'neovm--ctf-eval state '(- x y))
         ;; x * y: 4 products: 1*-5=-5, 1*5=5, 10*-5=-50, 10*5=50 -> [-50, 50]
         (funcall 'neovm--ctf-eval state '(* x y))
         ;; x + (const 100 200) = [101, 210]
         (funcall 'neovm--ctf-eval state '(+ x (const 100 200)))
         ;; (x + y) * z = [-4,15] * [0,100]
         ;; products: -4*0=0, -4*100=-400, 15*0=0, 15*100=1500 -> [-400, 1500]
         (funcall 'neovm--ctf-eval state '(* (+ x y) z))
         ;; x - x = [1-10, 10-1] = [-9, 9]
         (funcall 'neovm--ctf-eval state '(- x x))
         ;; (x * x) = [1*1, ..., 10*10] but also 1*10=10, 10*1=10 -> [1, 100]
         (funcall 'neovm--ctf-eval state '(* x x))
         ;; Nested: (x + y) - (x - y) = effectively 2*y
         ;; [1+(-5), 10+5] - [1-5, 10-(-5)] = [-4,15] - [-4,15] = [-4-15, 15-(-4)] = [-19, 19]
         (funcall 'neovm--ctf-eval state '(- (+ x y) (- x y)))))
    (fmakunbound 'neovm--ctf-le)
    (fmakunbound 'neovm--ctf-min2)
    (fmakunbound 'neovm--ctf-max2)
    (fmakunbound 'neovm--ctf-iadd)
    (fmakunbound 'neovm--ctf-isub)
    (fmakunbound 'neovm--ctf-imul)
    (fmakunbound 'neovm--ctf-eval)))"#;
    let expect = expect_test::expect![[
        r#""OK ((-4 . 15) (-4 . 15) (-50 . 50) (101 . 210) (-400 . 1500) (-9 . 9) (1 . 100) (-19 . 19))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
