//! Complex oracle parity tests implementing interval arithmetic in Elisp:
//! interval representation [lo, hi], addition, subtraction, multiplication,
//! division, containment/overlap predicates, union/intersection, interval
//! Newton's method for root finding, and uncertainty propagation.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Interval representation and basic arithmetic
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_interval_arith_basic_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Interval: (lo . hi) as a cons cell
  (fset 'neovm--ia-make (lambda (lo hi) (cons (float lo) (float hi))))
  (fset 'neovm--ia-lo (lambda (iv) (car iv)))
  (fset 'neovm--ia-hi (lambda (iv) (cdr iv)))

  ;; Addition: [a,b] + [c,d] = [a+c, b+d]
  (fset 'neovm--ia-add
    (lambda (a b)
      (funcall 'neovm--ia-make
               (+ (funcall 'neovm--ia-lo a) (funcall 'neovm--ia-lo b))
               (+ (funcall 'neovm--ia-hi a) (funcall 'neovm--ia-hi b)))))

  ;; Subtraction: [a,b] - [c,d] = [a-d, b-c]
  (fset 'neovm--ia-sub
    (lambda (a b)
      (funcall 'neovm--ia-make
               (- (funcall 'neovm--ia-lo a) (funcall 'neovm--ia-hi b))
               (- (funcall 'neovm--ia-hi a) (funcall 'neovm--ia-lo b)))))

  ;; Multiplication: [a,b] * [c,d] = [min(ac,ad,bc,bd), max(ac,ad,bc,bd)]
  (fset 'neovm--ia-mul
    (lambda (a b)
      (let* ((al (funcall 'neovm--ia-lo a)) (ah (funcall 'neovm--ia-hi a))
             (bl (funcall 'neovm--ia-lo b)) (bh (funcall 'neovm--ia-hi b))
             (p1 (* al bl)) (p2 (* al bh))
             (p3 (* ah bl)) (p4 (* ah bh)))
        (funcall 'neovm--ia-make
                 (min p1 p2 p3 p4)
                 (max p1 p2 p3 p4)))))

  ;; Division: [a,b] / [c,d] where 0 not in [c,d]
  ;; = [a,b] * [1/d, 1/c]
  (fset 'neovm--ia-div
    (lambda (a b)
      (if (and (<= (funcall 'neovm--ia-lo b) 0)
               (>= (funcall 'neovm--ia-hi b) 0))
          (cons -1.0e+INF 1.0e+INF)  ;; Division by interval containing zero
        (funcall 'neovm--ia-mul a
                 (funcall 'neovm--ia-make
                          (/ 1.0 (funcall 'neovm--ia-hi b))
                          (/ 1.0 (funcall 'neovm--ia-lo b)))))))

  (unwind-protect
      (let ((a (funcall 'neovm--ia-make 1.0 3.0))
            (b (funcall 'neovm--ia-make 2.0 5.0))
            (c (funcall 'neovm--ia-make -1.0 4.0))
            (d (funcall 'neovm--ia-make 2.0 3.0)))
        (list
         ;; [1,3] + [2,5] = [3,8]
         (funcall 'neovm--ia-add a b)
         ;; [1,3] - [2,5] = [-4,1]
         (funcall 'neovm--ia-sub a b)
         ;; [1,3] * [2,5] = [2,15]
         (funcall 'neovm--ia-mul a b)
         ;; [-1,4] * [2,5] = [-5,20]
         (funcall 'neovm--ia-mul c b)
         ;; [1,3] / [2,3] = [1/3, 3/2]
         (funcall 'neovm--ia-div a d)
         ;; Subtraction is not same as negation of addition
         (let ((sub-result (funcall 'neovm--ia-sub a a)))
           ;; [1,3] - [1,3] = [-2,2], NOT [0,0]
           sub-result)))
    (fmakunbound 'neovm--ia-make)
    (fmakunbound 'neovm--ia-lo)
    (fmakunbound 'neovm--ia-hi)
    (fmakunbound 'neovm--ia-add)
    (fmakunbound 'neovm--ia-sub)
    (fmakunbound 'neovm--ia-mul)
    (fmakunbound 'neovm--ia-div)))"#;
    let expect = expect_test::expect![[
        r#""OK ((3.0 . 8.0) (-4.0 . 1.0) (2.0 . 15.0) (-5.0 . 20.0) (0.3333333333333333 . 1.5) (-2.0 . 2.0))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Contains-point and overlap predicates
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_interval_arith_predicates() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--ia-make (lambda (lo hi) (cons (float lo) (float hi))))
  (fset 'neovm--ia-lo (lambda (iv) (car iv)))
  (fset 'neovm--ia-hi (lambda (iv) (cdr iv)))

  ;; Contains point: lo <= x <= hi
  (fset 'neovm--ia-contains-p
    (lambda (iv x)
      (and (<= (funcall 'neovm--ia-lo iv) x)
           (<= x (funcall 'neovm--ia-hi iv)))))

  ;; Overlap: two intervals overlap iff lo1 <= hi2 AND lo2 <= hi1
  (fset 'neovm--ia-overlap-p
    (lambda (a b)
      (and (<= (funcall 'neovm--ia-lo a) (funcall 'neovm--ia-hi b))
           (<= (funcall 'neovm--ia-lo b) (funcall 'neovm--ia-hi a)))))

  ;; Width of interval
  (fset 'neovm--ia-width
    (lambda (iv)
      (- (funcall 'neovm--ia-hi iv) (funcall 'neovm--ia-lo iv))))

  ;; Midpoint
  (fset 'neovm--ia-mid
    (lambda (iv)
      (/ (+ (funcall 'neovm--ia-lo iv) (funcall 'neovm--ia-hi iv)) 2.0)))

  (unwind-protect
      (let ((a (funcall 'neovm--ia-make 1.0 5.0))
            (b (funcall 'neovm--ia-make 3.0 7.0))
            (c (funcall 'neovm--ia-make 6.0 10.0))
            (d (funcall 'neovm--ia-make -2.0 0.5)))
        (list
         ;; Contains-point tests
         (funcall 'neovm--ia-contains-p a 3.0)   ;; t (inside)
         (funcall 'neovm--ia-contains-p a 1.0)   ;; t (left boundary)
         (funcall 'neovm--ia-contains-p a 5.0)   ;; t (right boundary)
         (funcall 'neovm--ia-contains-p a 0.5)   ;; nil (below)
         (funcall 'neovm--ia-contains-p a 5.1)   ;; nil (above)
         ;; Overlap tests
         (funcall 'neovm--ia-overlap-p a b)  ;; t (overlapping)
         (funcall 'neovm--ia-overlap-p a c)  ;; nil (disjoint, but touching at boundary is an edge case)
         (funcall 'neovm--ia-overlap-p b c)  ;; t (overlapping)
         (funcall 'neovm--ia-overlap-p a d)  ;; nil (disjoint)
         ;; Width and midpoint
         (funcall 'neovm--ia-width a)   ;; 4.0
         (funcall 'neovm--ia-width d)   ;; 2.5
         (funcall 'neovm--ia-mid a)     ;; 3.0
         (funcall 'neovm--ia-mid d)))   ;; -0.75
    (fmakunbound 'neovm--ia-make)
    (fmakunbound 'neovm--ia-lo)
    (fmakunbound 'neovm--ia-hi)
    (fmakunbound 'neovm--ia-contains-p)
    (fmakunbound 'neovm--ia-overlap-p)
    (fmakunbound 'neovm--ia-width)
    (fmakunbound 'neovm--ia-mid)))"#;
    let expect = expect_test::expect![[r#""OK (t t t nil nil t nil t nil 4.0 2.5 3.0 -0.75)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Interval union and intersection
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_interval_arith_union_intersection() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--ia-make (lambda (lo hi) (cons (float lo) (float hi))))
  (fset 'neovm--ia-lo (lambda (iv) (car iv)))
  (fset 'neovm--ia-hi (lambda (iv) (cdr iv)))

  ;; Intersection: returns nil if disjoint, otherwise [max(lo1,lo2), min(hi1,hi2)]
  (fset 'neovm--ia-intersect
    (lambda (a b)
      (let ((lo (max (funcall 'neovm--ia-lo a) (funcall 'neovm--ia-lo b)))
            (hi (min (funcall 'neovm--ia-hi a) (funcall 'neovm--ia-hi b))))
        (if (<= lo hi)
            (funcall 'neovm--ia-make lo hi)
          nil))))

  ;; Union of overlapping intervals: [min(lo1,lo2), max(hi1,hi2)]
  ;; Returns nil if disjoint (no single interval can represent)
  (fset 'neovm--ia-union
    (lambda (a b)
      (if (and (<= (funcall 'neovm--ia-lo a) (funcall 'neovm--ia-hi b))
               (<= (funcall 'neovm--ia-lo b) (funcall 'neovm--ia-hi a)))
          (funcall 'neovm--ia-make
                   (min (funcall 'neovm--ia-lo a) (funcall 'neovm--ia-lo b))
                   (max (funcall 'neovm--ia-hi a) (funcall 'neovm--ia-hi b)))
        nil)))

  ;; Merge a list of intervals into non-overlapping sorted intervals
  (fset 'neovm--ia-merge-all
    (lambda (intervals)
      (if (null intervals) nil
        (let* ((sorted (sort (copy-sequence intervals)
                             (lambda (a b) (< (funcall 'neovm--ia-lo a)
                                              (funcall 'neovm--ia-lo b)))))
               (result (list (car sorted)))
               (rest (cdr sorted)))
          (dolist (iv rest)
            (let ((top (car result)))
              (if (>= (funcall 'neovm--ia-hi top) (funcall 'neovm--ia-lo iv))
                  ;; Overlapping: extend top
                  (setcar result (funcall 'neovm--ia-make
                                          (funcall 'neovm--ia-lo top)
                                          (max (funcall 'neovm--ia-hi top)
                                               (funcall 'neovm--ia-hi iv))))
                ;; Disjoint: push new
                (setq result (cons iv result)))))
          (nreverse result)))))

  (unwind-protect
      (let ((a (funcall 'neovm--ia-make 1.0 5.0))
            (b (funcall 'neovm--ia-make 3.0 7.0))
            (c (funcall 'neovm--ia-make 8.0 10.0))
            (d (funcall 'neovm--ia-make -1.0 2.0)))
        (list
         ;; Intersection of overlapping intervals
         (funcall 'neovm--ia-intersect a b)  ;; [3,5]
         ;; Intersection of disjoint intervals
         (funcall 'neovm--ia-intersect a c)  ;; nil
         ;; Union of overlapping intervals
         (funcall 'neovm--ia-union a b)  ;; [1,7]
         ;; Union of disjoint intervals
         (funcall 'neovm--ia-union a c)  ;; nil
         ;; Merge a collection of intervals
         (funcall 'neovm--ia-merge-all
                  (list (funcall 'neovm--ia-make 1.0 3.0)
                        (funcall 'neovm--ia-make 2.0 6.0)
                        (funcall 'neovm--ia-make 8.0 10.0)
                        (funcall 'neovm--ia-make 15.0 18.0)
                        (funcall 'neovm--ia-make 9.0 12.0)))
         ;; Merge with all overlapping
         (funcall 'neovm--ia-merge-all
                  (list (funcall 'neovm--ia-make 1.0 5.0)
                        (funcall 'neovm--ia-make 2.0 6.0)
                        (funcall 'neovm--ia-make 4.0 8.0)))
         ;; Merge with none overlapping
         (funcall 'neovm--ia-merge-all
                  (list (funcall 'neovm--ia-make 1.0 2.0)
                        (funcall 'neovm--ia-make 5.0 6.0)
                        (funcall 'neovm--ia-make 9.0 10.0)))))
    (fmakunbound 'neovm--ia-make)
    (fmakunbound 'neovm--ia-lo)
    (fmakunbound 'neovm--ia-hi)
    (fmakunbound 'neovm--ia-intersect)
    (fmakunbound 'neovm--ia-union)
    (fmakunbound 'neovm--ia-merge-all)))"#;
    let expect = expect_test::expect![[
        r#""OK ((3.0 . 5.0) nil (1.0 . 7.0) nil ((1.0 . 6.0) (8.0 . 12.0) (15.0 . 18.0)) ((1.0 . 8.0)) ((1.0 . 2.0) (5.0 . 6.0) (9.0 . 10.0)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Interval Newton's method for root finding
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_interval_arith_newton_root_finding() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Interval Newton's method: given f and f', and an initial interval,
    // iteratively narrow the interval containing a root.
    // For f(x) = x^2 - 2 (root at sqrt(2) ~= 1.41421356...)
    let form = r#"(progn
  (fset 'neovm--ia-make (lambda (lo hi) (cons (float lo) (float hi))))
  (fset 'neovm--ia-lo (lambda (iv) (car iv)))
  (fset 'neovm--ia-hi (lambda (iv) (cdr iv)))
  (fset 'neovm--ia-mid
    (lambda (iv) (/ (+ (funcall 'neovm--ia-lo iv) (funcall 'neovm--ia-hi iv)) 2.0)))
  (fset 'neovm--ia-width
    (lambda (iv) (- (funcall 'neovm--ia-hi iv) (funcall 'neovm--ia-lo iv))))

  ;; Interval Newton step:
  ;; N(X) = m - f(m)/F'(X) intersected with X
  ;; where m = mid(X), F'(X) is the interval evaluation of f' over X
  (fset 'neovm--ia-newton-step
    (lambda (f-at-mid fprime-interval x-interval)
      (let* ((m (funcall 'neovm--ia-mid x-interval))
             ;; m - f(m) / F'(X): compute interval division
             (fplo (funcall 'neovm--ia-lo fprime-interval))
             (fphi (funcall 'neovm--ia-hi fprime-interval)))
        ;; If F'(X) contains 0, we can't narrow well; keep X
        (if (and (<= fplo 0.0) (>= fphi 0.0))
            x-interval
          ;; N = m - [f(m)/fphi, f(m)/fplo] (swap if fplo > fphi)
          (let* ((r1 (/ f-at-mid fplo))
                 (r2 (/ f-at-mid fphi))
                 (nlo (- m (max r1 r2)))
                 (nhi (- m (min r1 r2)))
                 ;; Intersect with x-interval
                 (ilo (max nlo (funcall 'neovm--ia-lo x-interval)))
                 (ihi (min nhi (funcall 'neovm--ia-hi x-interval))))
            (if (<= ilo ihi)
                (funcall 'neovm--ia-make ilo ihi)
              x-interval))))))

  ;; f(x) = x^2 - 2, f'(x) = 2x
  ;; F'([a,b]) = [2a, 2b] (since f' is monotonically increasing for positive x)
  (fset 'neovm--ia-newton-sqrt2
    (lambda (x-interval max-iters)
      (let ((x x-interval) (i 0))
        (while (and (< i max-iters) (> (funcall 'neovm--ia-width x) 1.0e-10))
          (let* ((m (funcall 'neovm--ia-mid x))
                 (f-at-m (- (* m m) 2.0))
                 ;; f'(x) = 2x evaluated over x-interval
                 (fp-iv (funcall 'neovm--ia-make
                                 (* 2.0 (funcall 'neovm--ia-lo x))
                                 (* 2.0 (funcall 'neovm--ia-hi x)))))
            (setq x (funcall 'neovm--ia-newton-step f-at-m fp-iv x)))
          (setq i (1+ i)))
        (list x i))))

  (unwind-protect
      (let* ((initial (funcall 'neovm--ia-make 1.0 2.0))
             (result (funcall 'neovm--ia-newton-sqrt2 initial 50))
             (final-iv (car result))
             (iters (cadr result))
             (lo (funcall 'neovm--ia-lo final-iv))
             (hi (funcall 'neovm--ia-hi final-iv))
             (mid (funcall 'neovm--ia-mid final-iv))
             (width (funcall 'neovm--ia-width final-iv)))
        (list
         ;; Converged in reasonable iterations
         (< iters 50)
         ;; Width is very small
         (< width 1.0e-8)
         ;; Interval contains sqrt(2)
         (and (<= lo (sqrt 2.0)) (<= (sqrt 2.0) hi))
         ;; Midpoint is close to sqrt(2)
         (< (abs (- mid (sqrt 2.0))) 1.0e-8)
         ;; Number of iterations
         iters))
    (fmakunbound 'neovm--ia-make)
    (fmakunbound 'neovm--ia-lo)
    (fmakunbound 'neovm--ia-hi)
    (fmakunbound 'neovm--ia-mid)
    (fmakunbound 'neovm--ia-width)
    (fmakunbound 'neovm--ia-newton-step)
    (fmakunbound 'neovm--ia-newton-sqrt2)))"#;
    let expect = expect_test::expect![[r#""OK (t t t t 4)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Uncertainty propagation through computation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_interval_arith_uncertainty_propagation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Model physical measurements with uncertainty as intervals,
    // propagate through formulas, observe uncertainty growth.
    let form = r#"(progn
  (fset 'neovm--ia-make (lambda (lo hi) (cons (float lo) (float hi))))
  (fset 'neovm--ia-lo (lambda (iv) (car iv)))
  (fset 'neovm--ia-hi (lambda (iv) (cdr iv)))
  (fset 'neovm--ia-width
    (lambda (iv) (- (funcall 'neovm--ia-hi iv) (funcall 'neovm--ia-lo iv))))
  (fset 'neovm--ia-mid
    (lambda (iv) (/ (+ (funcall 'neovm--ia-lo iv) (funcall 'neovm--ia-hi iv)) 2.0)))

  (fset 'neovm--ia-add
    (lambda (a b)
      (funcall 'neovm--ia-make
               (+ (funcall 'neovm--ia-lo a) (funcall 'neovm--ia-lo b))
               (+ (funcall 'neovm--ia-hi a) (funcall 'neovm--ia-hi b)))))

  (fset 'neovm--ia-sub
    (lambda (a b)
      (funcall 'neovm--ia-make
               (- (funcall 'neovm--ia-lo a) (funcall 'neovm--ia-hi b))
               (- (funcall 'neovm--ia-hi a) (funcall 'neovm--ia-lo b)))))

  (fset 'neovm--ia-mul
    (lambda (a b)
      (let* ((al (funcall 'neovm--ia-lo a)) (ah (funcall 'neovm--ia-hi a))
             (bl (funcall 'neovm--ia-lo b)) (bh (funcall 'neovm--ia-hi b))
             (p1 (* al bl)) (p2 (* al bh)) (p3 (* ah bl)) (p4 (* ah bh)))
        (funcall 'neovm--ia-make (min p1 p2 p3 p4) (max p1 p2 p3 p4)))))

  ;; Scalar multiply: c * [a,b]
  (fset 'neovm--ia-scale
    (lambda (c iv)
      (if (>= c 0.0)
          (funcall 'neovm--ia-make (* c (funcall 'neovm--ia-lo iv))
                                   (* c (funcall 'neovm--ia-hi iv)))
        (funcall 'neovm--ia-make (* c (funcall 'neovm--ia-hi iv))
                                 (* c (funcall 'neovm--ia-lo iv))))))

  ;; Square of an interval [a,b]:
  ;; If 0 in [a,b]: [0, max(a^2, b^2)]
  ;; If a >= 0: [a^2, b^2]
  ;; If b <= 0: [b^2, a^2]
  (fset 'neovm--ia-square
    (lambda (iv)
      (let ((a (funcall 'neovm--ia-lo iv))
            (b (funcall 'neovm--ia-hi iv)))
        (cond
         ((>= a 0.0) (funcall 'neovm--ia-make (* a a) (* b b)))
         ((<= b 0.0) (funcall 'neovm--ia-make (* b b) (* a a)))
         (t (funcall 'neovm--ia-make 0.0 (max (* a a) (* b b))))))))

  (unwind-protect
      ;; Physics: compute kinetic energy E = 0.5 * m * v^2
      ;; with uncertain mass and velocity
      (let* ((mass (funcall 'neovm--ia-make 9.8 10.2))       ;; 10 +/- 0.2 kg
             (velocity (funcall 'neovm--ia-make 4.9 5.1))    ;; 5 +/- 0.1 m/s
             (v-squared (funcall 'neovm--ia-square velocity))
             (mv2 (funcall 'neovm--ia-mul mass v-squared))
             (energy (funcall 'neovm--ia-scale 0.5 mv2))
             ;; Also compute momentum p = m * v
             (momentum (funcall 'neovm--ia-mul mass velocity))
             ;; Difference of two uncertain quantities
             (mass2 (funcall 'neovm--ia-make 9.9 10.1))
             (mass-diff (funcall 'neovm--ia-sub mass mass2))
             ;; Chained computation: (a+b)*(a-b) = a^2 - b^2
             (a (funcall 'neovm--ia-make 3.0 3.1))
             (b (funcall 'neovm--ia-make 1.0 1.1))
             (apb (funcall 'neovm--ia-add a b))
             (amb (funcall 'neovm--ia-sub a b))
             (product-form (funcall 'neovm--ia-mul apb amb))
             (sq-a (funcall 'neovm--ia-square a))
             (sq-b (funcall 'neovm--ia-square b))
             (diff-form (funcall 'neovm--ia-sub sq-a sq-b)))
        (list
         ;; Energy interval contains the nominal value 0.5 * 10 * 25 = 125
         (and (<= (funcall 'neovm--ia-lo energy) 125.0)
              (>= (funcall 'neovm--ia-hi energy) 125.0))
         ;; Energy uncertainty is larger than input uncertainty (amplification)
         (> (funcall 'neovm--ia-width energy) 0.4)
         ;; Momentum
         momentum
         ;; Mass difference can be negative
         (< (funcall 'neovm--ia-lo mass-diff) 0.0)
         ;; (a+b)(a-b) should overlap with a^2-b^2 but may be wider
         ;; (dependency problem in interval arithmetic)
         (let ((p-lo (funcall 'neovm--ia-lo product-form))
               (p-hi (funcall 'neovm--ia-hi product-form))
               (d-lo (funcall 'neovm--ia-lo diff-form))
               (d-hi (funcall 'neovm--ia-hi diff-form)))
           (list
            ;; Both contain the true value ~8.0
            (and (<= p-lo 8.0) (>= p-hi 8.0))
            (and (<= d-lo 8.0) (>= d-hi 8.0))
            ;; Product form is typically wider due to dependency
            (>= (- p-hi p-lo) (- d-hi d-lo))))))
    (fmakunbound 'neovm--ia-make)
    (fmakunbound 'neovm--ia-lo)
    (fmakunbound 'neovm--ia-hi)
    (fmakunbound 'neovm--ia-width)
    (fmakunbound 'neovm--ia-mid)
    (fmakunbound 'neovm--ia-add)
    (fmakunbound 'neovm--ia-sub)
    (fmakunbound 'neovm--ia-mul)
    (fmakunbound 'neovm--ia-scale)
    (fmakunbound 'neovm--ia-square)))"#;
    let expect =
        expect_test::expect![[r#""OK (t t (48.02000000000001 . 52.019999999999996) t (t t t))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Interval bisection search for root isolation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_interval_arith_bisection_search() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use interval arithmetic to do bisection: evaluate function over interval,
    // if result interval doesn't contain 0, no root in that half.
    // Find root of f(x) = x^3 - x - 1 (real root near 1.3247...)
    let form = r#"(progn
  (fset 'neovm--ia-make (lambda (lo hi) (cons (float lo) (float hi))))
  (fset 'neovm--ia-lo (lambda (iv) (car iv)))
  (fset 'neovm--ia-hi (lambda (iv) (cdr iv)))
  (fset 'neovm--ia-width
    (lambda (iv) (- (funcall 'neovm--ia-hi iv) (funcall 'neovm--ia-lo iv))))
  (fset 'neovm--ia-mid
    (lambda (iv) (/ (+ (funcall 'neovm--ia-lo iv) (funcall 'neovm--ia-hi iv)) 2.0)))

  (fset 'neovm--ia-add
    (lambda (a b)
      (funcall 'neovm--ia-make
               (+ (funcall 'neovm--ia-lo a) (funcall 'neovm--ia-lo b))
               (+ (funcall 'neovm--ia-hi a) (funcall 'neovm--ia-hi b)))))

  (fset 'neovm--ia-sub
    (lambda (a b)
      (funcall 'neovm--ia-make
               (- (funcall 'neovm--ia-lo a) (funcall 'neovm--ia-hi b))
               (- (funcall 'neovm--ia-hi a) (funcall 'neovm--ia-lo b)))))

  (fset 'neovm--ia-mul
    (lambda (a b)
      (let* ((al (funcall 'neovm--ia-lo a)) (ah (funcall 'neovm--ia-hi a))
             (bl (funcall 'neovm--ia-lo b)) (bh (funcall 'neovm--ia-hi b))
             (p1 (* al bl)) (p2 (* al bh)) (p3 (* ah bl)) (p4 (* ah bh)))
        (funcall 'neovm--ia-make (min p1 p2 p3 p4) (max p1 p2 p3 p4)))))

  ;; Evaluate f(x) = x^3 - x - 1 over an interval
  (fset 'neovm--ia-eval-f
    (lambda (x-iv)
      (let* ((x2 (funcall 'neovm--ia-mul x-iv x-iv))
             (x3 (funcall 'neovm--ia-mul x2 x-iv))
             (minus-x (funcall 'neovm--ia-sub x3 x-iv))
             (one (funcall 'neovm--ia-make 1.0 1.0)))
        (funcall 'neovm--ia-sub minus-x one))))

  ;; Check if interval contains zero
  (fset 'neovm--ia-contains-zero-p
    (lambda (iv)
      (and (<= (funcall 'neovm--ia-lo iv) 0.0)
           (>= (funcall 'neovm--ia-hi iv) 0.0))))

  ;; Bisection: repeatedly halve the interval, keeping the half that might contain root
  (fset 'neovm--ia-bisect
    (lambda (x-iv max-iters)
      (let ((x x-iv) (i 0))
        (while (and (< i max-iters) (> (funcall 'neovm--ia-width x) 1.0e-10))
          (let* ((m (funcall 'neovm--ia-mid x))
                 (left (funcall 'neovm--ia-make (funcall 'neovm--ia-lo x) m))
                 (right (funcall 'neovm--ia-make m (funcall 'neovm--ia-hi x)))
                 (f-left (funcall 'neovm--ia-eval-f left)))
            (if (funcall 'neovm--ia-contains-zero-p f-left)
                (setq x left)
              (setq x right)))
          (setq i (1+ i)))
        (list x i))))

  (unwind-protect
      (let* ((initial (funcall 'neovm--ia-make 1.0 2.0))
             (result (funcall 'neovm--ia-bisect initial 100))
             (final-iv (car result))
             (iters (cadr result))
             (mid (funcall 'neovm--ia-mid final-iv))
             (width (funcall 'neovm--ia-width final-iv)))
        (list
         ;; Converged
         (< width 1.0e-8)
         ;; Root is near 1.3247
         (< (abs (- mid 1.3247179572)) 1.0e-4)
         ;; Verify: f(mid) is near zero
         (< (abs (- (* mid mid mid) mid 1.0)) 1.0e-4)
         ;; Reasonable number of iterations
         (< iters 100)
         iters))
    (fmakunbound 'neovm--ia-make)
    (fmakunbound 'neovm--ia-lo)
    (fmakunbound 'neovm--ia-hi)
    (fmakunbound 'neovm--ia-width)
    (fmakunbound 'neovm--ia-mid)
    (fmakunbound 'neovm--ia-add)
    (fmakunbound 'neovm--ia-sub)
    (fmakunbound 'neovm--ia-mul)
    (fmakunbound 'neovm--ia-eval-f)
    (fmakunbound 'neovm--ia-contains-zero-p)
    (fmakunbound 'neovm--ia-bisect)))"#;
    let expect = expect_test::expect![[r#""OK (t nil nil t 34)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Interval power and polynomial evaluation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_interval_arith_polynomial_evaluation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Evaluate polynomials using interval arithmetic with Horner's method
    // to reduce dependency problem.
    let form = r#"(progn
  (fset 'neovm--ia-make (lambda (lo hi) (cons (float lo) (float hi))))
  (fset 'neovm--ia-lo (lambda (iv) (car iv)))
  (fset 'neovm--ia-hi (lambda (iv) (cdr iv)))
  (fset 'neovm--ia-width
    (lambda (iv) (- (funcall 'neovm--ia-hi iv) (funcall 'neovm--ia-lo iv))))

  (fset 'neovm--ia-add
    (lambda (a b)
      (funcall 'neovm--ia-make
               (+ (funcall 'neovm--ia-lo a) (funcall 'neovm--ia-lo b))
               (+ (funcall 'neovm--ia-hi a) (funcall 'neovm--ia-hi b)))))

  (fset 'neovm--ia-mul
    (lambda (a b)
      (let* ((al (funcall 'neovm--ia-lo a)) (ah (funcall 'neovm--ia-hi a))
             (bl (funcall 'neovm--ia-lo b)) (bh (funcall 'neovm--ia-hi b))
             (p1 (* al bl)) (p2 (* al bh)) (p3 (* ah bl)) (p4 (* ah bh)))
        (funcall 'neovm--ia-make (min p1 p2 p3 p4) (max p1 p2 p3 p4)))))

  ;; Horner's method for interval polynomial evaluation:
  ;; p(x) = a_n * x^n + a_{n-1} * x^{n-1} + ... + a_0
  ;; coefficients given as (a_n a_{n-1} ... a_0)
  ;; Horner: result = a_n; for each a_i: result = result * x + a_i
  (fset 'neovm--ia-horner
    (lambda (coeffs x-iv)
      (let ((result (funcall 'neovm--ia-make (float (car coeffs)) (float (car coeffs)))))
        (dolist (c (cdr coeffs))
          (let ((c-iv (funcall 'neovm--ia-make (float c) (float c))))
            (setq result (funcall 'neovm--ia-add
                                  (funcall 'neovm--ia-mul result x-iv)
                                  c-iv))))
        result)))

  ;; Naive evaluation for comparison: sum a_i * x^i
  (fset 'neovm--ia-naive-poly
    (lambda (coeffs x-iv)
      (let* ((rcoeffs (reverse coeffs))
             (result (funcall 'neovm--ia-make 0.0 0.0))
             (x-power (funcall 'neovm--ia-make 1.0 1.0)))
        (dolist (c rcoeffs)
          (let ((c-iv (funcall 'neovm--ia-make (float c) (float c))))
            (setq result (funcall 'neovm--ia-add result
                                  (funcall 'neovm--ia-mul c-iv x-power)))
            (setq x-power (funcall 'neovm--ia-mul x-power x-iv))))
        result)))

  (unwind-protect
      ;; Polynomial: p(x) = x^3 - 6x^2 + 11x - 6 = (x-1)(x-2)(x-3)
      ;; Coefficients: (1 -6 11 -6)
      (let ((coeffs '(1 -6 11 -6)))
        (list
         ;; Evaluate at point intervals (width=0)
         (funcall 'neovm--ia-horner coeffs (funcall 'neovm--ia-make 0.0 0.0))   ;; -6
         (funcall 'neovm--ia-horner coeffs (funcall 'neovm--ia-make 1.0 1.0))   ;; 0
         (funcall 'neovm--ia-horner coeffs (funcall 'neovm--ia-make 2.0 2.0))   ;; 0
         (funcall 'neovm--ia-horner coeffs (funcall 'neovm--ia-make 3.0 3.0))   ;; 0
         (funcall 'neovm--ia-horner coeffs (funcall 'neovm--ia-make 4.0 4.0))   ;; 6
         ;; Evaluate over interval [0.5, 1.5] - should contain 0 (root at 1)
         (let ((iv-result (funcall 'neovm--ia-horner coeffs (funcall 'neovm--ia-make 0.5 1.5))))
           (list iv-result
                 (and (<= (funcall 'neovm--ia-lo iv-result) 0.0)
                      (>= (funcall 'neovm--ia-hi iv-result) 0.0))))
         ;; Compare Horner vs naive: Horner should give tighter bounds
         (let* ((x-wide (funcall 'neovm--ia-make -1.0 4.0))
                (horner-result (funcall 'neovm--ia-horner coeffs x-wide))
                (naive-result (funcall 'neovm--ia-naive-poly coeffs x-wide)))
           (list
            (funcall 'neovm--ia-width horner-result)
            (funcall 'neovm--ia-width naive-result)
            ;; Horner is typically tighter or equal
            (<= (funcall 'neovm--ia-width horner-result)
                (funcall 'neovm--ia-width naive-result))))))
    (fmakunbound 'neovm--ia-make)
    (fmakunbound 'neovm--ia-lo)
    (fmakunbound 'neovm--ia-hi)
    (fmakunbound 'neovm--ia-width)
    (fmakunbound 'neovm--ia-add)
    (fmakunbound 'neovm--ia-mul)
    (fmakunbound 'neovm--ia-horner)
    (fmakunbound 'neovm--ia-naive-poly)))"#;
    let expect = expect_test::expect![[
        r#""OK ((-6.0 . -6.0) (0.0 . 0.0) (0.0 . 0.0) (0.0 . 0.0) (6.0 . 6.0) ((-4.625 . 7.125) t) (140.0 255.0 t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
