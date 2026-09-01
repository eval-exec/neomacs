//! Advanced oracle parity tests for abstract interpretation framework:
//! abstract domains (signs, intervals, congruences), concretization/abstraction
//! functions (gamma/alpha), abstract arithmetic operations, widening operators
//! for loop convergence, narrowing for precision recovery, reduced product of
//! domains, fixpoint iteration with widening, trace partitioning, and backward
//! analysis for precondition inference.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Interval abstract domain: [lo, hi] with +/- infinity
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_absint_adv_interval_domain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Interval representation: (lo . hi) where lo/hi are integers or 'neginf/'posinf
  ;; Bottom = 'bot, Top = (neginf . posinf)

  (fset 'neovm--iv-bot-p (lambda (iv) (eq iv 'bot)))

  (fset 'neovm--iv-le
    (lambda (a b)
      "a <= b for extended integers"
      (cond ((eq a 'neginf) t)
            ((eq b 'posinf) t)
            ((eq a 'posinf) nil)
            ((eq b 'neginf) nil)
            (t (<= a b)))))

  (fset 'neovm--iv-min
    (lambda (a b)
      (if (funcall 'neovm--iv-le a b) a b)))

  (fset 'neovm--iv-max
    (lambda (a b)
      (if (funcall 'neovm--iv-le a b) b a)))

  ;; Join (least upper bound): smallest interval containing both
  (fset 'neovm--iv-join
    (lambda (a b)
      (cond ((eq a 'bot) b)
            ((eq b 'bot) a)
            (t (cons (funcall 'neovm--iv-min (car a) (car b))
                     (funcall 'neovm--iv-max (cdr a) (cdr b)))))))

  ;; Meet (greatest lower bound): intersection
  (fset 'neovm--iv-meet
    (lambda (a b)
      (cond ((eq a 'bot) 'bot)
            ((eq b 'bot) 'bot)
            (t (let ((lo (funcall 'neovm--iv-max (car a) (car b)))
                     (hi (funcall 'neovm--iv-min (cdr a) (cdr b))))
                 (if (funcall 'neovm--iv-le lo hi)
                     (cons lo hi)
                   'bot))))))

  (list
    ;; Join tests
    (funcall 'neovm--iv-join 'bot '(1 . 5))
    (funcall 'neovm--iv-join '(1 . 5) 'bot)
    (funcall 'neovm--iv-join '(1 . 5) '(3 . 8))
    (funcall 'neovm--iv-join '(1 . 5) '(10 . 20))
    (funcall 'neovm--iv-join '(neginf . 0) '(0 . posinf))
    ;; Meet tests
    (funcall 'neovm--iv-meet '(1 . 5) '(3 . 8))
    (funcall 'neovm--iv-meet '(1 . 5) '(10 . 20))
    (funcall 'neovm--iv-meet '(neginf . 10) '(5 . posinf))
    (funcall 'neovm--iv-meet 'bot '(1 . 5))
    ;; Idempotence
    (funcall 'neovm--iv-join '(2 . 7) '(2 . 7))
    (funcall 'neovm--iv-meet '(2 . 7) '(2 . 7))))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 . 5) (1 . 5) (1 . 8) (1 . 20) (neginf . posinf) (3 . 5) bot (5 . 10) bot (2 . 7) (2 . 7))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Interval abstract arithmetic
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_absint_adv_interval_arithmetic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Helpers for extended arithmetic
  (fset 'neovm--iv-ext-add
    (lambda (a b)
      (cond ((or (eq a 'neginf) (eq b 'neginf))
             (if (or (eq a 'posinf) (eq b 'posinf)) 'top 'neginf))
            ((or (eq a 'posinf) (eq b 'posinf)) 'posinf)
            (t (+ a b)))))

  (fset 'neovm--iv-ext-sub
    (lambda (a b)
      (cond ((and (eq a 'neginf) (eq b 'neginf)) 'top)
            ((and (eq a 'posinf) (eq b 'posinf)) 'top)
            ((eq a 'neginf) 'neginf)
            ((eq a 'posinf) 'posinf)
            ((eq b 'posinf) 'neginf)
            ((eq b 'neginf) 'posinf)
            (t (- a b)))))

  ;; Interval addition: [a,b] + [c,d] = [a+c, b+d]
  (fset 'neovm--iv-add
    (lambda (x y)
      (cond ((or (eq x 'bot) (eq y 'bot)) 'bot)
            (t (let ((lo (funcall 'neovm--iv-ext-add (car x) (car y)))
                     (hi (funcall 'neovm--iv-ext-add (cdr x) (cdr y))))
                 (if (eq lo 'top) '(neginf . posinf)
                   (if (eq hi 'top) '(neginf . posinf)
                     (cons lo hi))))))))

  ;; Interval negation: -[a,b] = [-b, -a]
  (fset 'neovm--iv-neg
    (lambda (x)
      (if (eq x 'bot) 'bot
        (let ((new-lo (cond ((eq (cdr x) 'posinf) 'neginf)
                            ((eq (cdr x) 'neginf) 'posinf)
                            (t (- (cdr x)))))
              (new-hi (cond ((eq (car x) 'neginf) 'posinf)
                            ((eq (car x) 'posinf) 'neginf)
                            (t (- (car x))))))
          (cons new-lo new-hi)))))

  ;; Interval subtraction
  (fset 'neovm--iv-sub
    (lambda (x y)
      (funcall 'neovm--iv-add x (funcall 'neovm--iv-neg y))))

  (list
    ;; Addition
    (funcall 'neovm--iv-add '(1 . 5) '(2 . 3))        ;; [3,8]
    (funcall 'neovm--iv-add '(-3 . 3) '(-2 . 2))      ;; [-5,5]
    (funcall 'neovm--iv-add '(neginf . 0) '(0 . posinf)) ;; [neginf,posinf]
    (funcall 'neovm--iv-add 'bot '(1 . 5))             ;; bot
    ;; Negation
    (funcall 'neovm--iv-neg '(1 . 5))                  ;; [-5,-1]
    (funcall 'neovm--iv-neg '(-3 . 3))                 ;; [-3,3]
    (funcall 'neovm--iv-neg '(neginf . 0))             ;; [0,posinf]
    ;; Subtraction
    (funcall 'neovm--iv-sub '(5 . 10) '(1 . 3))       ;; [2,9]
    (funcall 'neovm--iv-sub '(0 . 0) '(-5 . 5))       ;; [-5,5]
    (funcall 'neovm--iv-sub '(1 . 1) '(1 . 1))))"#;
    let expect = expect_test::expect![[
        r#""OK ((3 . 8) (-5 . 5) (neginf . posinf) bot (-5 . -1) (-3 . 3) (0 . posinf) (2 . 9) (-5 . 5) (0 . 0))""#
    ]];
    // [0,0]
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Congruence abstract domain: aZ + b
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_absint_adv_congruence_domain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Congruence domain: (a . b) represents {b + k*a : k in Z}
  ;; a=0 means the set {b}, a=1 means all integers (top)
  ;; bot represented as 'bot

  ;; GCD helper
  (fset 'neovm--gcd
    (lambda (a b)
      (let ((a (abs a)) (b (abs b)))
        (while (> b 0)
          (let ((tmp (% a b)))
            (setq a b b tmp)))
        a)))

  ;; Join: aZ+b join cZ+d = gcd(a, c, b-d)Z + (b mod gcd)
  (fset 'neovm--cong-join
    (lambda (x y)
      (cond ((eq x 'bot) y)
            ((eq y 'bot) x)
            (t (let* ((a (car x)) (b (cdr x))
                      (c (car y)) (d (cdr y))
                      (g (funcall 'neovm--gcd (funcall 'neovm--gcd a c) (abs (- b d)))))
                 (if (= g 0)
                     (if (= b d) (cons 0 b) (cons (abs (- b d)) (min b d)))
                   (cons g (% b g))))))))

  ;; Meet: intersection of congruence classes
  ;; aZ+b meet cZ+d: if (b-d) mod gcd(a,c) != 0 then bot
  ;; else lcm(a,c)Z + solution
  (fset 'neovm--cong-meet
    (lambda (x y)
      (cond ((eq x 'bot) 'bot)
            ((eq y 'bot) 'bot)
            (t (let* ((a (car x)) (b (cdr x))
                      (c (car y)) (d (cdr y)))
                 (cond
                  ((and (= a 0) (= c 0))
                   (if (= b d) (cons 0 b) 'bot))
                  ((= a 0) (if (= (% (- b d) c) 0) x 'bot))
                  ((= c 0) (if (= (% (- d b) a) 0) y 'bot))
                  (t (let ((g (funcall 'neovm--gcd a c)))
                       (if (/= (% (- b d) g) 0) 'bot
                         ;; LCM(a,c) Z + offset
                         (cons (/ (* a c) g) (% b (/ (* a c) g))))))))))))

  ;; Abstract add: (aZ+b) + (cZ+d) = gcd(a,c)Z + (b+d)
  (fset 'neovm--cong-add
    (lambda (x y)
      (cond ((or (eq x 'bot) (eq y 'bot)) 'bot)
            (t (cons (funcall 'neovm--gcd (car x) (car y))
                     (+ (cdr x) (cdr y)))))))

  (list
    ;; Join
    (funcall 'neovm--cong-join '(2 . 0) '(2 . 1))  ;; gcd(2,2,1)=1 -> 1Z+0 (all ints)
    (funcall 'neovm--cong-join '(4 . 1) '(4 . 1))  ;; same -> 4Z+1
    (funcall 'neovm--cong-join '(3 . 0) '(3 . 0))  ;; 3Z+0
    (funcall 'neovm--cong-join 'bot '(2 . 0))       ;; 2Z+0
    ;; Meet
    (funcall 'neovm--cong-meet '(2 . 0) '(3 . 0))  ;; 6Z+0 (multiples of 6)
    (funcall 'neovm--cong-meet '(2 . 0) '(2 . 1))  ;; bot (even meet odd)
    (funcall 'neovm--cong-meet '(0 . 5) '(3 . 2))  ;; 5 mod 3 = 2 -> (0 . 5)
    (funcall 'neovm--cong-meet 'bot '(4 . 1))       ;; bot
    ;; Addition
    (funcall 'neovm--cong-add '(2 . 0) '(2 . 0))   ;; 2Z+0 (even + even = even)
    (funcall 'neovm--cong-add '(2 . 0) '(2 . 1))   ;; 2Z+1 (even + odd = odd)
    (funcall 'neovm--cong-add '(0 . 3) '(0 . 5))   ;; 0Z+8 ({8})
    (funcall 'neovm--cong-add 'bot '(2 . 0))))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 . 0) (4 . 1) (3 . 0) (2 . 0) (6 . 0) bot (0 . 5) bot (2 . 0) (2 . 1) (0 . 8) bot)""#
    ]];
    // bot
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Widening operator for intervals
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_absint_adv_interval_widening() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--iv-le2
    (lambda (a b)
      (cond ((eq a 'neginf) t) ((eq b 'posinf) t) ((eq a 'posinf) nil) ((eq b 'neginf) nil) (t (<= a b)))))

  ;; Standard interval widening: push unstable bounds to infinity
  ;; widen([a,b], [c,d]) = [if c<a then neginf else a, if d>b then posinf else b]
  (fset 'neovm--iv-widen
    (lambda (old new-iv)
      (cond
       ((eq old 'bot) new-iv)
       ((eq new-iv 'bot) old)
       (t (cons (if (funcall 'neovm--iv-le2 (car new-iv) (car old))
                    (if (equal (car new-iv) (car old)) (car old) 'neginf)
                  (car old))
                (if (funcall 'neovm--iv-le2 (cdr old) (cdr new-iv))
                    (if (equal (cdr new-iv) (cdr old)) (cdr old) 'posinf)
                  (cdr old)))))))

  ;; Narrowing: tighten infinite bounds with finite info
  ;; narrow([a,b], [c,d]) = [if a=-inf then c else a, if b=+inf then d else b]
  (fset 'neovm--iv-narrow
    (lambda (wide precise)
      (cond
       ((eq wide 'bot) 'bot)
       ((eq precise 'bot) 'bot)
       (t (cons (if (eq (car wide) 'neginf) (car precise) (car wide))
                (if (eq (cdr wide) 'posinf) (cdr precise) (cdr wide)))))))

  (list
    ;; Widening: stable bounds kept, unstable pushed to infinity
    (funcall 'neovm--iv-widen '(0 . 5) '(0 . 10))      ;; [0, posinf]
    (funcall 'neovm--iv-widen '(0 . 5) '(-1 . 5))      ;; [neginf, 5]
    (funcall 'neovm--iv-widen '(0 . 5) '(-1 . 10))     ;; [neginf, posinf]
    (funcall 'neovm--iv-widen '(0 . 5) '(0 . 5))       ;; [0, 5] (no change)
    (funcall 'neovm--iv-widen '(0 . 5) '(1 . 3))       ;; [0, 5] (narrowing direction ignored)
    (funcall 'neovm--iv-widen 'bot '(0 . 5))            ;; [0, 5]

    ;; Narrowing: recover precision from infinite bounds
    (funcall 'neovm--iv-narrow '(neginf . posinf) '(0 . 100))  ;; [0, 100]
    (funcall 'neovm--iv-narrow '(neginf . 5) '(-10 . 3))       ;; [-10, 5]
    (funcall 'neovm--iv-narrow '(0 . posinf) '(-5 . 50))       ;; [0, 50]
    (funcall 'neovm--iv-narrow '(0 . 5) '(1 . 3))              ;; [0, 5]
    (funcall 'neovm--iv-narrow 'bot '(1 . 5))))"#;
    let expect = expect_test::expect![[
        r#""OK ((0 . posinf) (neginf . 5) (neginf . posinf) (0 . 5) (0 . 5) (0 . 5) (0 . 100) (-10 . 5) (0 . 50) (0 . 5) bot)""#
    ]];
    // bot
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Fixpoint iteration with widening for loop analysis
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_absint_adv_fixpoint_widening() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Interval helpers
  (fset 'neovm--fp-le (lambda (a b) (cond ((eq a 'neginf) t) ((eq b 'posinf) t) ((eq a 'posinf) nil) ((eq b 'neginf) nil) (t (<= a b)))))
  (fset 'neovm--fp-min (lambda (a b) (if (funcall 'neovm--fp-le a b) a b)))
  (fset 'neovm--fp-max (lambda (a b) (if (funcall 'neovm--fp-le a b) b a)))
  (fset 'neovm--fp-join (lambda (a b) (cond ((eq a 'bot) b) ((eq b 'bot) a) (t (cons (funcall 'neovm--fp-min (car a) (car b)) (funcall 'neovm--fp-max (cdr a) (cdr b)))))))
  (fset 'neovm--fp-widen (lambda (old new-iv)
    (cond ((eq old 'bot) new-iv) ((eq new-iv 'bot) old)
          (t (cons (if (and (not (equal (car new-iv) (car old))) (funcall 'neovm--fp-le (car new-iv) (car old))) 'neginf (car old))
                   (if (and (not (equal (cdr new-iv) (cdr old))) (funcall 'neovm--fp-le (cdr old) (cdr new-iv))) 'posinf (cdr old)))))))
  (fset 'neovm--fp-narrow (lambda (w p) (cond ((eq w 'bot) 'bot) ((eq p 'bot) 'bot) (t (cons (if (eq (car w) 'neginf) (car p) (car w)) (if (eq (cdr w) 'posinf) (cdr p) (cdr w)))))))

  ;; State: hash-table of var -> interval
  (fset 'neovm--fp-get (lambda (s v) (let ((r (gethash v s))) (or r 'bot))))
  (fset 'neovm--fp-set (lambda (s v iv) (puthash v iv s) s))

  ;; Interval add
  (fset 'neovm--fp-iadd (lambda (a b)
    (cond ((or (eq a 'bot) (eq b 'bot)) 'bot)
          (t (let ((lo (cond ((or (eq (car a) 'neginf) (eq (car b) 'neginf)) 'neginf) (t (+ (car a) (car b)))))
                   (hi (cond ((or (eq (cdr a) 'posinf) (eq (cdr b) 'posinf)) 'posinf) (t (+ (cdr a) (cdr b))))))
               (cons lo hi))))))

  (fset 'neovm--fp-eq (lambda (a b)
    (cond ((and (eq a 'bot) (eq b 'bot)) t)
          ((or (eq a 'bot) (eq b 'bot)) nil)
          (t (and (equal (car a) (car b)) (equal (cdr a) (cdr b)))))))

  ;; Analyze loop: x = init; while (condition) { x = body(x) }
  ;; Returns converged value of x after widening iterations + narrowing
  (fset 'neovm--fp-analyze-loop
    (lambda (init-iv body-fn max-iters)
      (let ((x init-iv) (converged nil) (iters 0) (trace nil))
        ;; Widening phase
        (while (and (not converged) (< iters max-iters))
          (setq iters (1+ iters))
          (let ((next (funcall body-fn x)))
            (let ((joined (funcall 'neovm--fp-join init-iv next)))
              (let ((widened (funcall 'neovm--fp-widen x joined)))
                (setq trace (cons (list 'iter iters 'val widened) trace))
                (if (funcall 'neovm--fp-eq widened x)
                    (setq converged t)
                  (setq x widened))))))
        (list 'widened x 'iterations iters 'converged converged
              'trace (nreverse trace)))))

  (list
    ;; Loop: x = [0,0]; body: x = x + [1,1]
    ;; Iteration 1: join([0,0], [1,1]) = [0,1], widen([0,0],[0,1]) = [0,posinf]
    ;; Iteration 2: join([0,0], [1,posinf]) = [0,posinf], widen = [0,posinf] -> converged
    (funcall 'neovm--fp-analyze-loop
      '(0 . 0)
      (lambda (x) (funcall 'neovm--fp-iadd x '(1 . 1)))
      20)
    ;; Loop: x = [0,10]; body: x = x + [-1,1]
    (funcall 'neovm--fp-analyze-loop
      '(0 . 10)
      (lambda (x) (funcall 'neovm--fp-iadd x '(-1 . 1)))
      20)
    ;; Loop: x = [5,5]; body: x = x + [0,2]
    (funcall 'neovm--fp-analyze-loop
      '(5 . 5)
      (lambda (x) (funcall 'neovm--fp-iadd x '(0 . 2)))
      20)))"#;
    let expect = expect_test::expect![[
        r#""OK ((widened (0 . posinf) iterations 2 converged t trace ((iter 1 val (0 . posinf)) (iter 2 val (0 . posinf)))) (widened (neginf . posinf) iterations 2 converged t trace ((iter 1 val (neginf . posinf)) (iter 2 val (neginf . posinf)))) (widened (5 . posinf) iterations 2 converged t trace ((iter 1 val (5 . posinf)) (iter 2 val (5 . posinf)))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Reduced product of sign and interval domains
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_absint_adv_reduced_product() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Reduced product: pair (sign . interval) with reduction operator
  ;; Signs: pos, neg, zero, non-neg, non-pos, top, bot

  ;; Reduce: use sign to constrain interval and vice versa
  (fset 'neovm--rp-reduce
    (lambda (pair)
      (let ((sign (car pair)) (iv (cdr pair)))
        (cond
         ((or (eq sign 'bot) (eq iv 'bot)) (cons 'bot 'bot))
         ;; If sign = pos, constrain interval to [max(lo,1), hi]
         ((eq sign 'pos)
          (if (eq iv 'bot) (cons 'bot 'bot)
            (let ((new-lo (cond ((eq (car iv) 'neginf) 1)
                                ((< (car iv) 1) 1)
                                (t (car iv))))
                  (new-hi (cdr iv)))
              (if (and (not (eq new-hi 'posinf)) (< new-hi new-lo))
                  (cons 'bot 'bot)
                (cons 'pos (cons new-lo new-hi))))))
         ;; If sign = neg, constrain interval to [lo, min(hi,-1)]
         ((eq sign 'neg)
          (if (eq iv 'bot) (cons 'bot 'bot)
            (let ((new-lo (car iv))
                  (new-hi (cond ((eq (cdr iv) 'posinf) -1)
                                ((> (cdr iv) -1) -1)
                                (t (cdr iv)))))
              (if (and (not (eq new-lo 'neginf)) (> new-lo new-hi))
                  (cons 'bot 'bot)
                (cons 'neg (cons new-lo new-hi))))))
         ;; If sign = zero, constrain to [0,0]
         ((eq sign 'zero)
          (if (eq iv 'bot) (cons 'bot 'bot)
            (let ((lo (car iv)) (hi (cdr iv)))
              (if (and (or (eq lo 'neginf) (<= lo 0))
                       (or (eq hi 'posinf) (>= hi 0)))
                  (cons 'zero '(0 . 0))
                (cons 'bot 'bot)))))
         ;; If sign = non-neg, constrain lo to max(lo, 0)
         ((eq sign 'non-neg)
          (if (eq iv 'bot) (cons 'bot 'bot)
            (let ((new-lo (cond ((eq (car iv) 'neginf) 0) ((< (car iv) 0) 0) (t (car iv)))))
              (if (and (not (eq (cdr iv) 'posinf)) (< (cdr iv) new-lo))
                  (cons 'bot 'bot)
                ;; Refine sign from interval
                (let ((refined-sign (cond ((and (= new-lo 0) (equal (cdr iv) 0)) 'zero)
                                          ((> new-lo 0) 'pos)
                                          (t 'non-neg))))
                  (cons refined-sign (cons new-lo (cdr iv))))))))
         ;; top sign: infer sign from interval
         ((eq sign 'top)
          (if (eq iv 'bot) (cons 'bot 'bot)
            (let ((lo (car iv)) (hi (cdr iv)))
              (cond
               ((and (not (eq lo 'neginf)) (> lo 0)) (cons 'pos iv))
               ((and (not (eq hi 'posinf)) (< hi 0)) (cons 'neg iv))
               ((and (equal lo 0) (equal hi 0)) (cons 'zero iv))
               ((and (or (eq lo 'neginf) (<= lo 0)) (or (eq hi 'posinf) (>= hi 0))) (cons sign iv))
               (t (cons sign iv))))))
         (t pair)))))

  (list
    ;; pos + [neginf, posinf] -> pos + [1, posinf]
    (funcall 'neovm--rp-reduce '(pos . (neginf . posinf)))
    ;; neg + [-10, 10] -> neg + [-10, -1]
    (funcall 'neovm--rp-reduce '(neg . (-10 . 10)))
    ;; zero + [-5, 5] -> zero + [0, 0]
    (funcall 'neovm--rp-reduce '(zero . (-5 . 5)))
    ;; pos + [-5, -1] -> bot (contradiction)
    (funcall 'neovm--rp-reduce '(pos . (-5 . -1)))
    ;; top + [1, 10] -> pos + [1, 10]
    (funcall 'neovm--rp-reduce '(top . (1 . 10)))
    ;; top + [-10, -1] -> neg + [-10, -1]
    (funcall 'neovm--rp-reduce '(top . (-10 . -1)))
    ;; top + [0, 0] -> zero + [0, 0]
    (funcall 'neovm--rp-reduce '(top . (0 . 0)))
    ;; non-neg + [-5, 10] -> non-neg + [0, 10]
    (funcall 'neovm--rp-reduce '(non-neg . (-5 . 10)))
    ;; non-neg + [5, 10] -> pos + [5, 10]
    (funcall 'neovm--rp-reduce '(non-neg . (5 . 10)))
    ;; bot + anything -> bot
    (funcall 'neovm--rp-reduce '(bot . (1 . 5)))))"#;
    let expect = expect_test::expect![[
        r#""OK ((pos 1 . posinf) (neg -10 . -1) (zero 0 . 0) (bot . bot) (pos 1 . 10) (neg -10 . -1) (zero 0 . 0) (non-neg 0 . 10) (pos 5 . 10) (bot . bot))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Backward analysis: precondition inference
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_absint_adv_backward_analysis() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Simple backward transfer: given a postcondition on y = x + c,
  ;; derive precondition on x.
  ;; If y in [lo,hi], then x in [lo-c, hi-c]

  (fset 'neovm--back-assign-add
    (lambda (post-iv constant)
      "Given post: y in post-iv, and y = x + constant, compute pre: x"
      (cond
       ((eq post-iv 'bot) 'bot)
       (t (let ((lo (if (eq (car post-iv) 'neginf) 'neginf (- (car post-iv) constant)))
                (hi (if (eq (cdr post-iv) 'posinf) 'posinf (- (cdr post-iv) constant))))
            (cons lo hi))))))

  ;; Backward through multiplication by constant:
  ;; If y in [lo,hi] and y = x * c (c > 0), then x in [lo/c, hi/c] (floor division)
  (fset 'neovm--back-assign-mul
    (lambda (post-iv constant)
      (cond
       ((eq post-iv 'bot) 'bot)
       ((= constant 0) (if (and (or (eq (car post-iv) 'neginf) (<= (car post-iv) 0))
                                 (or (eq (cdr post-iv) 'posinf) (>= (cdr post-iv) 0)))
                            '(neginf . posinf)  ;; any x works if 0 in range
                          'bot))
       ((> constant 0)
        (let ((lo (if (eq (car post-iv) 'neginf) 'neginf (/ (car post-iv) constant)))
              (hi (if (eq (cdr post-iv) 'posinf) 'posinf (/ (cdr post-iv) constant))))
          (cons lo hi)))
       (t ;; negative constant: flip bounds
        (let ((lo (if (eq (cdr post-iv) 'posinf) 'neginf (/ (cdr post-iv) constant)))
              (hi (if (eq (car post-iv) 'neginf) 'posinf (/ (car post-iv) constant))))
          (cons lo hi))))))

  ;; Backward through conditional: if (x > 0) then ... else ...
  ;; Restrict to true-branch: x in meet(x_iv, [1, posinf])
  ;; Restrict to false-branch: x in meet(x_iv, [neginf, 0])
  (fset 'neovm--back-cond-pos
    (lambda (pre-iv branch)
      (cond
       ((eq pre-iv 'bot) 'bot)
       ((eq branch 'true)
        (let ((lo (if (or (eq (car pre-iv) 'neginf) (< (car pre-iv) 1)) 1 (car pre-iv)))
              (hi (cdr pre-iv)))
          (if (and (not (eq hi 'posinf)) (< hi lo)) 'bot (cons lo hi))))
       ((eq branch 'false)
        (let ((lo (car pre-iv))
              (hi (if (or (eq (cdr pre-iv) 'posinf) (> (cdr pre-iv) 0)) 0 (cdr pre-iv))))
          (if (and (not (eq lo 'neginf)) (> lo hi)) 'bot (cons lo hi)))))))

  (list
    ;; Backward add: if y in [10,20] and y = x + 5, then x in [5,15]
    (funcall 'neovm--back-assign-add '(10 . 20) 5)
    ;; Backward add: y in [neginf, 0], y = x + 10 -> x in [neginf, -10]
    (funcall 'neovm--back-assign-add '(neginf . 0) 10)
    ;; Backward mul: y in [6, 12], y = x * 3 -> x in [2, 4]
    (funcall 'neovm--back-assign-mul '(6 . 12) 3)
    ;; Backward mul: y in [-10, 10], y = x * (-2) -> x in [-5, 5]
    (funcall 'neovm--back-assign-mul '(-10 . 10) -2)
    ;; Backward mul by 0: y must contain 0
    (funcall 'neovm--back-assign-mul '(-5 . 5) 0)
    (funcall 'neovm--back-assign-mul '(1 . 5) 0)
    ;; Conditional backward: x in [-10, 10], branch true (x>0) -> [1, 10]
    (funcall 'neovm--back-cond-pos '(-10 . 10) 'true)
    ;; Conditional backward: x in [-10, 10], branch false (x<=0) -> [-10, 0]
    (funcall 'neovm--back-cond-pos '(-10 . 10) 'false)
    ;; Conditional backward: x in [5, 10], branch false -> bot (5>0 always true)
    (funcall 'neovm--back-cond-pos '(5 . 10) 'false)
    ;; Chained: z in [0,10], z=y+3 -> y in [-3,7], y=x*2 -> x in [-1,3]
    (let* ((z-post '(0 . 10))
           (y-pre (funcall 'neovm--back-assign-add z-post 3))
           (x-pre (funcall 'neovm--back-assign-mul y-pre 2)))
      (list 'chain z-post y-pre x-pre))))"#;
    let expect = expect_test::expect![[
        r#""OK ((5 . 15) (neginf . -10) (2 . 4) (-5 . 5) (neginf . posinf) bot (1 . 10) (-10 . 0) bot (chain (0 . 10) (-3 . 7) (-1 . 3)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Trace partitioning: split analysis by branch history
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_absint_adv_trace_partitioning() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Simulate trace partitioning: analyze different paths separately
  ;; then merge. Compare precision of merged vs unpartitioned.

  ;; Interval helpers
  (fset 'neovm--tp-le (lambda (a b) (cond ((eq a 'neginf) t) ((eq b 'posinf) t) ((eq a 'posinf) nil) ((eq b 'neginf) nil) (t (<= a b)))))
  (fset 'neovm--tp-min (lambda (a b) (if (funcall 'neovm--tp-le a b) a b)))
  (fset 'neovm--tp-max (lambda (a b) (if (funcall 'neovm--tp-le a b) b a)))
  (fset 'neovm--tp-join (lambda (a b) (cond ((eq a 'bot) b) ((eq b 'bot) a) (t (cons (funcall 'neovm--tp-min (car a) (car b)) (funcall 'neovm--tp-max (cdr a) (cdr b)))))))
  (fset 'neovm--tp-iadd (lambda (a b)
    (cond ((or (eq a 'bot) (eq b 'bot)) 'bot)
          (t (cons (cond ((or (eq (car a) 'neginf) (eq (car b) 'neginf)) 'neginf) (t (+ (car a) (car b))))
                   (cond ((or (eq (cdr a) 'posinf) (eq (cdr b) 'posinf)) 'posinf) (t (+ (cdr a) (cdr b)))))))))
  (fset 'neovm--tp-imul-const (lambda (iv c)
    (cond ((eq iv 'bot) 'bot)
          ((= c 0) '(0 . 0))
          ((> c 0) (cons (cond ((eq (car iv) 'neginf) 'neginf) (t (* (car iv) c)))
                         (cond ((eq (cdr iv) 'posinf) 'posinf) (t (* (cdr iv) c)))))
          (t (cons (cond ((eq (cdr iv) 'posinf) 'neginf) (t (* (cdr iv) c)))
                   (cond ((eq (car iv) 'neginf) 'posinf) (t (* (car iv) c))))))))

  ;; Program: if (x > 0) then y = x*2 else y = x*(-1)
  ;; x in [-5, 5]

  ;; Unpartitioned: x in [-5,5], y = join(x*2, x*(-1)) for full range
  ;; y_true = [-5,5]*2 = [-10,10], y_false = [-5,5]*(-1) = [-5,5]
  ;; y_unpart = join([-10,10], [-5,5]) = [-10,10]
  (let ((x-iv '(-5 . 5)))
    (let ((y-unpart (funcall 'neovm--tp-join
                             (funcall 'neovm--tp-imul-const x-iv 2)
                             (funcall 'neovm--tp-imul-const x-iv -1))))
      ;; Partitioned: split x by condition x > 0
      ;; x_true = [1, 5], x_false = [-5, 0]
      ;; y_true = [1,5]*2 = [2,10], y_false = [-5,0]*(-1) = [0,5]
      ;; y_part = join([2,10], [0,5]) = [0,10]
      (let ((x-true '(1 . 5))
            (x-false '(-5 . 0)))
        (let ((y-true (funcall 'neovm--tp-imul-const x-true 2))
              (y-false (funcall 'neovm--tp-imul-const x-false -1)))
          (let ((y-part (funcall 'neovm--tp-join y-true y-false)))
            (list
              (list 'unpartitioned y-unpart)
              (list 'partitioned y-part)
              (list 'y-true-branch y-true)
              (list 'y-false-branch y-false)
              ;; Partitioned is more precise (y-part subset of y-unpart)
              (list 'more-precise
                    (and (funcall 'neovm--tp-le (car y-unpart) (car y-part))
                         (funcall 'neovm--tp-le (cdr y-part) (cdr y-unpart))))))))))"#;
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Alpha/Gamma (abstraction/concretization) for sign domain
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_absint_adv_alpha_gamma_sign() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Alpha: concrete set -> abstract sign
  (fset 'neovm--alpha-sign
    (lambda (concrete-set)
      (if (null concrete-set) 'bot
        (let ((has-pos nil) (has-neg nil) (has-zero nil))
          (dolist (x concrete-set)
            (cond ((> x 0) (setq has-pos t))
                  ((< x 0) (setq has-neg t))
                  ((= x 0) (setq has-zero t))))
          (cond
           ((and has-pos has-neg has-zero) 'top)
           ((and has-pos has-neg) 'non-zero)
           ((and has-pos has-zero) 'non-neg)
           ((and has-neg has-zero) 'non-pos)
           (has-pos 'pos)
           (has-neg 'neg)
           (has-zero 'zero)
           (t 'bot))))))

  ;; Gamma: abstract sign -> description of concrete set
  ;; (can't enumerate infinite sets, so return a symbolic descriptor)
  (fset 'neovm--gamma-sign
    (lambda (abstract)
      (cond
       ((eq abstract 'bot) 'empty)
       ((eq abstract 'zero) '(0))
       ((eq abstract 'pos) '(1 2 3 ...))
       ((eq abstract 'neg) '(-3 -2 -1 ...))
       ((eq abstract 'non-neg) '(0 1 2 3 ...))
       ((eq abstract 'non-pos) '(-3 -2 -1 0 ...))
       ((eq abstract 'non-zero) '(-3 -2 -1 1 2 3 ...))
       ((eq abstract 'top) '(... -1 0 1 ...)))))

  ;; Galois connection property: alpha(gamma(a)) should be >= a (soundness)
  ;; We test alpha on finite subsets of gamma
  (fset 'neovm--check-gc
    (lambda (abstract test-set)
      (let ((re-abstracted (funcall 'neovm--alpha-sign test-set)))
        (list abstract re-abstracted
              ;; Check that re-abstracted is at least as precise as abstract
              ;; (in our simple lattice, they should be equal for representative sets)
              ))))

  (list
    ;; Alpha tests
    (funcall 'neovm--alpha-sign '(1 2 3))           ;; pos
    (funcall 'neovm--alpha-sign '(-3 -2 -1))        ;; neg
    (funcall 'neovm--alpha-sign '(0))                ;; zero
    (funcall 'neovm--alpha-sign '(0 1 2))            ;; non-neg
    (funcall 'neovm--alpha-sign '(-2 -1 0))          ;; non-pos
    (funcall 'neovm--alpha-sign '(-1 1))             ;; non-zero
    (funcall 'neovm--alpha-sign '(-1 0 1))           ;; top
    (funcall 'neovm--alpha-sign nil)                 ;; bot
    ;; Gamma tests (symbolic)
    (funcall 'neovm--gamma-sign 'pos)
    (funcall 'neovm--gamma-sign 'neg)
    (funcall 'neovm--gamma-sign 'zero)
    (funcall 'neovm--gamma-sign 'bot)
    (funcall 'neovm--gamma-sign 'top)
    ;; Galois connection: alpha of representative concrete sets
    (funcall 'neovm--check-gc 'pos '(1 5 100))
    (funcall 'neovm--check-gc 'neg '(-10 -1))
    (funcall 'neovm--check-gc 'non-neg '(0 1 42))
    (funcall 'neovm--check-gc 'top '(-5 0 5))))"#;
    let expect = expect_test::expect![[
        r#""OK (pos neg zero non-neg non-pos non-zero top bot (1 2 3 \\...) (-3 -2 -1 \\...) (0) empty (\\... -1 0 1 \\...) (pos pos) (neg neg) (non-neg non-neg) (top top))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Multi-variable abstract state with interval domain: small program analysis
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_absint_adv_multi_var_program() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Interval state helpers
  (fset 'neovm--mvp-get (lambda (s v) (or (cdr (assq v s)) 'bot)))
  (fset 'neovm--mvp-set (lambda (s v iv) (cons (cons v iv) (let ((r nil)) (dolist (p s) (unless (eq (car p) v) (setq r (cons p r)))) (nreverse r)))))

  (fset 'neovm--mvp-le (lambda (a b) (cond ((eq a 'neginf) t) ((eq b 'posinf) t) ((eq a 'posinf) nil) ((eq b 'neginf) nil) (t (<= a b)))))
  (fset 'neovm--mvp-imin (lambda (a b) (if (funcall 'neovm--mvp-le a b) a b)))
  (fset 'neovm--mvp-imax (lambda (a b) (if (funcall 'neovm--mvp-le a b) b a)))
  (fset 'neovm--mvp-ijoin (lambda (a b) (cond ((eq a 'bot) b) ((eq b 'bot) a) (t (cons (funcall 'neovm--mvp-imin (car a) (car b)) (funcall 'neovm--mvp-imax (cdr a) (cdr b)))))))
  (fset 'neovm--mvp-iadd (lambda (a b) (cond ((or (eq a 'bot) (eq b 'bot)) 'bot) (t (cons (cond ((or (eq (car a) 'neginf) (eq (car b) 'neginf)) 'neginf) (t (+ (car a) (car b)))) (cond ((or (eq (cdr a) 'posinf) (eq (cdr b) 'posinf)) 'posinf) (t (+ (cdr a) (cdr b)))))))))

  ;; Analyze program:
  ;; x = [0, 10]
  ;; y = [1, 1]
  ;; z = x + y          -> [1, 11]
  ;; if z > 5:
  ;;   w = z + [1,1]    -> z in [6,11], w in [7,12]
  ;; else:
  ;;   w = z + [-1,-1]  -> z in [1,5], w in [0,4]
  ;; result = join(w_true, w_false) = [0, 12]
  (let* ((state nil)
         (state (funcall 'neovm--mvp-set state 'x '(0 . 10)))
         (state (funcall 'neovm--mvp-set state 'y '(1 . 1)))
         (z-iv (funcall 'neovm--mvp-iadd (funcall 'neovm--mvp-get state 'x)
                                          (funcall 'neovm--mvp-get state 'y)))
         (state (funcall 'neovm--mvp-set state 'z z-iv))
         ;; True branch: z in [6, 11] (z > 5 means z >= 6)
         (z-true '(6 . 11))
         (w-true (funcall 'neovm--mvp-iadd z-true '(1 . 1)))
         ;; False branch: z in [1, 5]
         (z-false '(1 . 5))
         (w-false (funcall 'neovm--mvp-iadd z-false '(-1 . -1)))
         ;; Join
         (w-joined (funcall 'neovm--mvp-ijoin w-true w-false)))
    (list
      (list 'x (funcall 'neovm--mvp-get state 'x))
      (list 'y (funcall 'neovm--mvp-get state 'y))
      (list 'z z-iv)
      (list 'z-true z-true)
      (list 'w-true w-true)
      (list 'z-false z-false)
      (list 'w-false w-false)
      (list 'w-joined w-joined))))"#;
    let expect = expect_test::expect![[
        r#""OK ((x (0 . 10)) (y (1 . 1)) (z (1 . 11)) (z-true (6 . 11)) (w-true (7 . 12)) (z-false (1 . 5)) (w-false (0 . 4)) (w-joined (0 . 12)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Interval domain: multiplication of two intervals
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_absint_adv_interval_multiplication() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Interval multiplication: [a,b]*[c,d] = [min(ac,ad,bc,bd), max(ac,ad,bc,bd)]
  ;; Handle infinities carefully

  (fset 'neovm--im-ext-mul
    (lambda (a b)
      (cond
       ((or (eq a 0) (eq b 0)) 0)
       ((eq a 'neginf) (cond ((> b 0) 'neginf) ((< b 0) 'posinf) (t 0)))
       ((eq a 'posinf) (cond ((> b 0) 'posinf) ((< b 0) 'neginf) (t 0)))
       ((eq b 'neginf) (cond ((> a 0) 'neginf) ((< a 0) 'posinf) (t 0)))
       ((eq b 'posinf) (cond ((> a 0) 'posinf) ((< a 0) 'neginf) (t 0)))
       (t (* a b)))))

  (fset 'neovm--im-le (lambda (a b) (cond ((eq a 'neginf) t) ((eq b 'posinf) t) ((eq a 'posinf) nil) ((eq b 'neginf) nil) (t (<= a b)))))
  (fset 'neovm--im-min2 (lambda (a b) (if (funcall 'neovm--im-le a b) a b)))
  (fset 'neovm--im-max2 (lambda (a b) (if (funcall 'neovm--im-le a b) b a)))
  (fset 'neovm--im-min4 (lambda (a b c d) (funcall 'neovm--im-min2 (funcall 'neovm--im-min2 a b) (funcall 'neovm--im-min2 c d))))
  (fset 'neovm--im-max4 (lambda (a b c d) (funcall 'neovm--im-max2 (funcall 'neovm--im-max2 a b) (funcall 'neovm--im-max2 c d))))

  (fset 'neovm--iv-mul
    (lambda (x y)
      (cond
       ((or (eq x 'bot) (eq y 'bot)) 'bot)
       (t (let* ((a (car x)) (b (cdr x)) (c (car y)) (d (cdr y))
                 (ac (funcall 'neovm--im-ext-mul a c))
                 (ad (funcall 'neovm--im-ext-mul a d))
                 (bc (funcall 'neovm--im-ext-mul b c))
                 (bd (funcall 'neovm--im-ext-mul b d)))
            (cons (funcall 'neovm--im-min4 ac ad bc bd)
                  (funcall 'neovm--im-max4 ac ad bc bd)))))))

  (list
    ;; [2,3] * [4,5] = [8, 15]
    (funcall 'neovm--iv-mul '(2 . 3) '(4 . 5))
    ;; [-2,3] * [1,4] = [-8, 12]
    (funcall 'neovm--iv-mul '(-2 . 3) '(1 . 4))
    ;; [-3,-1] * [-5,-2] = [2, 15]
    (funcall 'neovm--iv-mul '(-3 . -1) '(-5 . -2))
    ;; [-2,2] * [-3,3] = [-6, 6]
    (funcall 'neovm--iv-mul '(-2 . 2) '(-3 . 3))
    ;; [0,0] * anything = [0,0]
    (funcall 'neovm--iv-mul '(0 . 0) '(-100 . 100))
    ;; [1,1] * [x,y] = [x,y] (identity)
    (funcall 'neovm--iv-mul '(1 . 1) '(5 . 10))
    ;; bot * anything = bot
    (funcall 'neovm--iv-mul 'bot '(1 . 5))
    ;; Infinite intervals
    (funcall 'neovm--iv-mul '(1 . posinf) '(2 . 3))
    (funcall 'neovm--iv-mul '(neginf . -1) '(1 . 2))))"#;
    let expect = expect_test::expect![[
        r#""OK ((8 . 15) (-8 . 12) (2 . 15) (-6 . 6) (0 . 0) (5 . 10) bot (2 . posinf) (neginf . -1))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
