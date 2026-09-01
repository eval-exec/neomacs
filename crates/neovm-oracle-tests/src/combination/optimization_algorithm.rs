//! Oracle parity tests for optimization algorithms implemented in pure Elisp.
//!
//! Covers: gradient descent (1D and 2D), simulated annealing, hill climbing
//! with random restarts, binary search optimization, golden section search,
//! Newton's method for root finding, Nelder-Mead simplex method (2D).

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

// ---------------------------------------------------------------------------
// 1D Gradient descent on a quadratic
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_optimization_gradient_descent_1d() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Minimize f(x) = (x - 3)^2 + 1 using gradient descent.
  ;; f'(x) = 2*(x - 3). Minimum at x=3, f(3)=1.
  ;; Using integer arithmetic scaled by 1000 to avoid floats.

  (fset 'neovm--opt-gd1-f
    (lambda (x)
      "f(x) = (x - 3000)^2 / 1000 + 1000. x is scaled by 1000."
      (+ (/ (* (- x 3000) (- x 3000)) 1000) 1000)))

  (fset 'neovm--opt-gd1-grad
    (lambda (x)
      "f'(x) = 2*(x - 3000) / 1000. Returns gradient scaled by 1000."
      (/ (* 2 (- x 3000)) 1)))

  ;; Gradient descent: x_{n+1} = x_n - lr * grad(x_n)
  ;; lr = 1/10 (i.e., lr_num=1, lr_den=10)
  (fset 'neovm--opt-gd1-step
    (lambda (x lr-num lr-den)
      (let ((grad (funcall 'neovm--opt-gd1-grad x)))
        (- x (/ (* lr-num grad) lr-den)))))

  ;; Run N iterations
  (fset 'neovm--opt-gd1-run
    (lambda (x0 lr-num lr-den n)
      (let ((x x0)
            (history nil)
            (i 0))
        (while (< i n)
          (push (list x (funcall 'neovm--opt-gd1-f x)) history)
          (setq x (funcall 'neovm--opt-gd1-step x lr-num lr-den))
          (setq i (1+ i)))
        (push (list x (funcall 'neovm--opt-gd1-f x)) history)
        (nreverse history))))

  (unwind-protect
      (let ((trace (funcall 'neovm--opt-gd1-run 0 1 10 20)))
        (list
         ;; Starting point and value
         (car trace)
         ;; Final point and value
         (car (last trace))
         ;; Trace length
         (length trace)
         ;; Final x should be closer to 3000 than initial x=0
         (let* ((final-x (car (car (last trace))))
                (init-x (car (car trace))))
           (< (abs (- final-x 3000)) (abs (- init-x 3000))))
         ;; Function values should be decreasing
         (let ((decreasing t)
               (rest trace)
               (prev most-positive-fixnum))
           (while rest
             (let ((fval (nth 1 (car rest))))
               (when (> fval prev) (setq decreasing nil))
               (setq prev fval))
             (setq rest (cdr rest)))
           decreasing)
         ;; Single step from x=0
         (funcall 'neovm--opt-gd1-step 0 1 10)
         ;; Single step from x=6000 (overshoot)
         (funcall 'neovm--opt-gd1-step 6000 1 10)))
    (fmakunbound 'neovm--opt-gd1-f)
    (fmakunbound 'neovm--opt-gd1-grad)
    (fmakunbound 'neovm--opt-gd1-step)
    (fmakunbound 'neovm--opt-gd1-run)))"#;
    let expect = expect_test::expect![[r#""OK ((0 10000) (2964 1001) 21 t t 600 5400)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// 2D Gradient descent on Rosenbrock-like function
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_optimization_gradient_descent_2d() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Minimize f(x,y) = (x-1)^2 + 10*(y-2)^2 (elliptic paraboloid)
  ;; Minimum at (1, 2), f(1,2) = 0.
  ;; All values scaled by 100.

  (fset 'neovm--opt-gd2-f
    (lambda (x y)
      "f = (x-100)^2/100 + 10*(y-200)^2/100."
      (+ (/ (* (- x 100) (- x 100)) 100)
         (/ (* 10 (- y 200) (- y 200)) 100))))

  (fset 'neovm--opt-gd2-grad-x
    (lambda (x y)
      "df/dx = 2*(x-100)/100, scaled."
      (/ (* 2 (- x 100)) 1)))

  (fset 'neovm--opt-gd2-grad-y
    (lambda (x y)
      "df/dy = 20*(y-200)/100, scaled."
      (/ (* 20 (- y 200)) 1)))

  ;; Gradient descent step
  (fset 'neovm--opt-gd2-step
    (lambda (x y lr-den)
      "lr = 1/lr-den."
      (let ((gx (funcall 'neovm--opt-gd2-grad-x x y))
            (gy (funcall 'neovm--opt-gd2-grad-y x y)))
        (list (- x (/ gx lr-den))
              (- y (/ gy lr-den))))))

  ;; Run N iterations, return trajectory
  (fset 'neovm--opt-gd2-run
    (lambda (x0 y0 lr-den n)
      (let ((x x0) (y y0)
            (history nil)
            (i 0))
        (while (< i n)
          (push (list x y (funcall 'neovm--opt-gd2-f x y)) history)
          (let ((next (funcall 'neovm--opt-gd2-step x y lr-den)))
            (setq x (car next) y (nth 1 next)))
          (setq i (1+ i)))
        (push (list x y (funcall 'neovm--opt-gd2-f x y)) history)
        (nreverse history))))

  (unwind-protect
      (let ((trace (funcall 'neovm--opt-gd2-run 500 500 100 30)))
        (list
         ;; Starting point
         (car trace)
         ;; Final point
         (car (last trace))
         ;; Final should be closer to (100,200) than start (500,500)
         (let* ((final (car (last trace)))
                (fx (car final)) (fy (nth 1 final)))
           (< (+ (abs (- fx 100)) (abs (- fy 200)))
              (+ (abs (- 500 100)) (abs (- 500 200)))))
         ;; Function value at optimum approximation
         (nth 2 (car (last trace)))
         ;; Single step from (0, 0)
         (funcall 'neovm--opt-gd2-step 0 0 100)))
    (fmakunbound 'neovm--opt-gd2-f)
    (fmakunbound 'neovm--opt-gd2-grad-x)
    (fmakunbound 'neovm--opt-gd2-grad-y)
    (fmakunbound 'neovm--opt-gd2-step)
    (fmakunbound 'neovm--opt-gd2-run)))"#;
    let expect = expect_test::expect![[r#""OK ((500 500 10600) (329 204 525) t 525 (2 40))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Simulated annealing (deterministic via seeded PRNG)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_optimization_simulated_annealing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Minimize f(x) = x^2 - 10*x + 30 (minimum at x=5, f(5)=5)
  ;; All values scaled by 100.
  ;; SA: accept worse solutions with probability exp(-delta/T).
  ;; We use integer-only approximation of acceptance criterion.

  (fset 'neovm--opt-sa-f
    (lambda (x)
      "f(x) = x^2/100 - 10*x + 3000."
      (+ (/ (* x x) 100) (- (* 10 x)) 3000)))

  (fset 'neovm--opt-sa-prng
    (lambda (state)
      (% (+ (* state 1103515245) 12345) 2147483648)))

  ;; Neighbor: perturb x by a deterministic random offset
  (fset 'neovm--opt-sa-neighbor
    (lambda (x state)
      (let* ((s (funcall 'neovm--opt-sa-prng state))
             ;; Offset in range [-200, 199] (scaled)
             (offset (- (% (/ s 65536) 400) 200)))
        (cons (+ x offset) s))))

  ;; Accept decision: always accept if better; accept worse with
  ;; probability proportional to temperature
  (fset 'neovm--opt-sa-accept
    (lambda (delta temp state)
      "DELTA = new_f - old_f. TEMP = temperature. Returns (accepted? . new-state)."
      (if (<= delta 0)
          (cons t state)  ;; improvement: always accept
        (let* ((s (funcall 'neovm--opt-sa-prng state))
               (threshold (/ s 65536))
               ;; Rough approximation: accept if random < temp/(temp+delta)
               (cutoff (/ (* 32768 temp) (+ temp delta))))
          (cons (< threshold cutoff) s)))))

  ;; Run SA
  (fset 'neovm--opt-sa-run
    (lambda (x0 initial-temp cooling-rate-den steps state)
      "cooling-rate-den: temperature *= (cooling-rate-den - 1)/cooling-rate-den each step."
      (let ((x x0)
            (best-x x0)
            (best-f (funcall 'neovm--opt-sa-f x0))
            (temp initial-temp)
            (s state)
            (i 0))
        (while (< i steps)
          (let* ((nb (funcall 'neovm--opt-sa-neighbor x s))
                 (new-x (car nb))
                 (old-f (funcall 'neovm--opt-sa-f x))
                 (new-f (funcall 'neovm--opt-sa-f new-x))
                 (delta (- new-f old-f))
                 (acc (funcall 'neovm--opt-sa-accept delta temp (cdr nb))))
            (setq s (cdr acc))
            (when (car acc)
              (setq x new-x))
            (when (< new-f best-f)
              (setq best-x new-x best-f new-f))
            ;; Cool down
            (setq temp (/ (* temp (1- cooling-rate-den)) cooling-rate-den)))
          (setq i (1+ i)))
        (list best-x best-f x (funcall 'neovm--opt-sa-f x)))))

  (unwind-protect
      (let ((result (funcall 'neovm--opt-sa-run 0 1000 100 50 12345)))
        (list
         ;; Best x found, best f
         result
         ;; Best f should be <= f(0) = 3000
         (<= (nth 1 result) (funcall 'neovm--opt-sa-f 0))
         ;; Function evaluation at known points
         (funcall 'neovm--opt-sa-f 0)
         (funcall 'neovm--opt-sa-f 500)  ;; optimal region
         ;; Neighbor generation is deterministic
         (funcall 'neovm--opt-sa-neighbor 500 99999)
         ;; Acceptance with negative delta (improvement): always accept
         (car (funcall 'neovm--opt-sa-accept -100 500 42))))
    (fmakunbound 'neovm--opt-sa-f)
    (fmakunbound 'neovm--opt-sa-prng)
    (fmakunbound 'neovm--opt-sa-neighbor)
    (fmakunbound 'neovm--opt-sa-accept)
    (fmakunbound 'neovm--opt-sa-run)))"#;
    let expect =
        expect_test::expect![[r#""OK ((504 500 747 1110) t 3000 500 (416 . 1973744620) t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Hill climbing with random restarts
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_optimization_hill_climbing_restarts() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Maximize f(x) = -(x-7)^2 + 100 (maximum at x=7, f(7)=100)
  ;; scaled by 100. Hill climbing with steepest ascent.

  (fset 'neovm--opt-hc-f
    (lambda (x)
      "f(x) = -(x-700)^2/100 + 10000."
      (- 10000 (/ (* (- x 700) (- x 700)) 100))))

  (fset 'neovm--opt-hc-prng
    (lambda (state)
      (% (+ (* state 1103515245) 12345) 2147483648)))

  ;; Steepest ascent: try ±step, take the better one
  (fset 'neovm--opt-hc-climb
    (lambda (x step max-iter)
      (let ((current x)
            (current-f (funcall 'neovm--opt-hc-f x))
            (i 0)
            (improved t))
        (while (and (< i max-iter) improved)
          (setq improved nil)
          (let* ((left (- current step))
                 (right (+ current step))
                 (fl (funcall 'neovm--opt-hc-f left))
                 (fr (funcall 'neovm--opt-hc-f right)))
            (cond
             ((and (> fl current-f) (>= fl fr))
              (setq current left current-f fl improved t))
             ((> fr current-f)
              (setq current right current-f fr improved t))))
          (setq i (1+ i)))
        (list current current-f i))))

  ;; Random restarts: run climb from multiple starting points
  (fset 'neovm--opt-hc-restarts
    (lambda (num-restarts step max-iter state)
      (let ((best-x 0)
            (best-f most-negative-fixnum)
            (s state)
            (results nil)
            (r 0))
        (while (< r num-restarts)
          (setq s (funcall 'neovm--opt-hc-prng s))
          ;; Start from random x in [0, 1500]
          (let* ((x0 (% (/ s 65536) 1500))
                 (result (funcall 'neovm--opt-hc-climb x0 10 100)))
            (push (list x0 result) results)
            (when (> (nth 1 result) best-f)
              (setq best-x (car result) best-f (nth 1 result))))
          (setq r (1+ r)))
        (list best-x best-f (nreverse results)))))

  (unwind-protect
      (let ((result (funcall 'neovm--opt-hc-restarts 5 10 100 42)))
        (list
         ;; Best x and f found across restarts
         (car result) (nth 1 result)
         ;; Number of restarts
         (length (nth 2 result))
         ;; All restart results
         (nth 2 result)
         ;; Single climb from x=0
         (funcall 'neovm--opt-hc-climb 0 10 100)
         ;; Single climb from x=700 (already at optimum)
         (funcall 'neovm--opt-hc-climb 700 10 100)))
    (fmakunbound 'neovm--opt-hc-f)
    (fmakunbound 'neovm--opt-hc-prng)
    (fmakunbound 'neovm--opt-hc-climb)
    (fmakunbound 'neovm--opt-hc-restarts)))"#;
    let expect = expect_test::expect![[
        r#""OK (701 10000 5 ((1081 (701 10000 39)) (533 (693 10000 17)) (269 (699 10000 44)) (1461 (701 10000 77)) (356 (696 10000 35))) (700 10000 71) (700 10000 1))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Binary search optimization (unimodal function)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_optimization_binary_search() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Ternary search to find minimum of a unimodal function on [lo, hi].
  ;; f(x) = (x - 5)^2 + 3, minimum at x=5, f(5)=3.
  ;; Scaled by 100.

  (fset 'neovm--opt-bs-f
    (lambda (x)
      "f(x) = (x-500)^2/100 + 300."
      (+ (/ (* (- x 500) (- x 500)) 100) 300)))

  ;; Ternary search: divide [lo,hi] into thirds
  (fset 'neovm--opt-bs-ternary
    (lambda (lo hi max-iter)
      (let ((l lo) (h hi)
            (i 0)
            (history nil))
        (while (and (< i max-iter) (> (- h l) 1))
          (let* ((m1 (+ l (/ (- h l) 3)))
                 (m2 (- h (/ (- h l) 3)))
                 (f1 (funcall 'neovm--opt-bs-f m1))
                 (f2 (funcall 'neovm--opt-bs-f m2)))
            (push (list l h m1 m2 f1 f2) history)
            (if (< f1 f2)
                (setq h m2)
              (setq l m1)))
          (setq i (1+ i)))
        (let ((mid (/ (+ l h) 2)))
          (list mid (funcall 'neovm--opt-bs-f mid) i (nreverse history))))))

  (unwind-protect
      (let ((result (funcall 'neovm--opt-bs-ternary 0 1000 30)))
        (list
         ;; Found x, f(x), iterations
         (car result) (nth 1 result) (nth 2 result)
         ;; First few search steps
         (let ((h (nth 3 result)))
           (if (> (length h) 3) (list (car h) (nth 1 h) (nth 2 h)) h))
         ;; Result should be near 500
         (< (abs (- (car result) 500)) 10)
         ;; Function value should be near 300
         (< (abs (- (nth 1 result) 300)) 10)
         ;; Various function evaluations
         (funcall 'neovm--opt-bs-f 0)
         (funcall 'neovm--opt-bs-f 500)
         (funcall 'neovm--opt-bs-f 1000)))
    (fmakunbound 'neovm--opt-bs-f)
    (fmakunbound 'neovm--opt-bs-ternary)))"#;
    let expect = expect_test::expect![[
        r#""OK (509 300 30 ((0 1000 333 667 578 578) (333 1000 555 778 330 1072) (333 778 481 630 303 469)) t t 2800 300 2800)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Golden section search
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_optimization_golden_section_search() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Golden section search for unimodal function minimum.
  ;; Uses the golden ratio phi = (1+sqrt(5))/2 ≈ 1.618
  ;; We approximate with integer ratio: 1000/618 ≈ phi, 382/1000 ≈ 1/phi.
  ;; f(x) = (x - 4)^2 + 2, minimum at x=4, f(4)=2.
  ;; Scaled by 1000.

  (fset 'neovm--opt-gs-f
    (lambda (x)
      "(x-4000)^2/1000 + 2000."
      (+ (/ (* (- x 4000) (- x 4000)) 1000) 2000)))

  ;; Golden section search
  (fset 'neovm--opt-gs-search
    (lambda (lo hi max-iter)
      (let ((a lo) (b hi)
            (i 0)
            ;; Golden ratio: probe points at 38.2% and 61.8% of interval
            ;; c = a + 0.382*(b-a), d = a + 0.618*(b-a)
            (c (+ lo (/ (* 382 (- hi lo)) 1000)))
            (d (+ lo (/ (* 618 (- hi lo)) 1000)))
            (fc nil) (fd nil))
        (setq fc (funcall 'neovm--opt-gs-f c))
        (setq fd (funcall 'neovm--opt-gs-f d))
        (while (and (< i max-iter) (> (- b a) 1))
          (if (< fc fd)
              (progn
                (setq b d d c fd fc)
                (setq c (+ a (/ (* 382 (- b a)) 1000)))
                (setq fc (funcall 'neovm--opt-gs-f c)))
            (progn
              (setq a c c d fc fd)
              (setq d (+ a (/ (* 618 (- b a)) 1000)))
              (setq fd (funcall 'neovm--opt-gs-f d))))
          (setq i (1+ i)))
        (let ((mid (/ (+ a b) 2)))
          (list mid (funcall 'neovm--opt-gs-f mid) i)))))

  (unwind-protect
      (let ((result (funcall 'neovm--opt-gs-search 0 8000 40)))
        (list
         ;; Result: x, f(x), iterations
         result
         ;; Should be near 4000
         (< (abs (- (car result) 4000)) 20)
         ;; f(result) should be near 2000
         (< (abs (- (nth 1 result) 2000)) 20)
         ;; Narrower initial range
         (funcall 'neovm--opt-gs-search 3000 5000 20)
         ;; Function evaluations
         (funcall 'neovm--opt-gs-f 0)
         (funcall 'neovm--opt-gs-f 4000)
         (funcall 'neovm--opt-gs-f 8000)))
    (fmakunbound 'neovm--opt-gs-f)
    (fmakunbound 'neovm--opt-gs-search)))"#;
    let expect =
        expect_test::expect![[r#""OK ((4031 2000 20) nil t (4031 2000 16) 18000 2000 18000)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Newton's method for root finding
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_optimization_newtons_method() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Newton's method: x_{n+1} = x_n - f(x_n)/f'(x_n)
  ;; Find roots of f(x) = x^2 - 9 (roots at x=3 and x=-3)
  ;; Scaled by 1000.

  (fset 'neovm--opt-nr-f
    (lambda (x)
      "f(x) = x^2/1000 - 9000."
      (- (/ (* x x) 1000) 9000)))

  (fset 'neovm--opt-nr-fprime
    (lambda (x)
      "f'(x) = 2x/1000, scaled."
      (/ (* 2 x) 1)))

  ;; Newton step: x - f(x)/f'(x)
  (fset 'neovm--opt-nr-step
    (lambda (x)
      (let ((fx (funcall 'neovm--opt-nr-f x))
            (fpx (funcall 'neovm--opt-nr-fprime x)))
        (if (= fpx 0)
            x  ;; avoid division by zero
          (- x (/ (* fx 1000) fpx))))))

  ;; Run Newton's method for N iterations
  (fset 'neovm--opt-nr-run
    (lambda (x0 max-iter tolerance)
      "TOLERANCE: stop if |f(x)| < tolerance."
      (let ((x x0)
            (i 0)
            (history nil)
            (converged nil))
        (while (and (< i max-iter) (not converged))
          (let ((fx (funcall 'neovm--opt-nr-f x)))
            (push (list x fx) history)
            (if (< (abs fx) tolerance)
                (setq converged t)
              (setq x (funcall 'neovm--opt-nr-step x))))
          (setq i (1+ i)))
        (list x (funcall 'neovm--opt-nr-f x) converged i (nreverse history)))))

  ;; Also: Newton's method for optimization (find where f'(x) = 0)
  ;; Minimize g(x) = (x-5)^2. g'(x) = 2(x-5), g''(x) = 2.
  ;; Newton step: x - g'(x)/g''(x) = x - (x-5)/1 = 5.
  (fset 'neovm--opt-nr-optim-step
    (lambda (x)
      "Newton step for minimizing (x-5000)^2."
      (let ((gprime (* 2 (- x 5000)))
            (g2prime 2))
        (- x (/ gprime g2prime)))))

  (unwind-protect
      (list
       ;; Root finding from x=1000 (converges to 3000)
       (let ((r (funcall 'neovm--opt-nr-run 1000 20 10)))
         (list (car r) (nth 2 r) (nth 3 r)))
       ;; Root finding from x=10000 (converges to 3000)
       (let ((r (funcall 'neovm--opt-nr-run 10000 20 10)))
         (list (car r) (nth 2 r)))
       ;; Root finding from x=-5000 (converges to -3000)
       (let ((r (funcall 'neovm--opt-nr-run -5000 20 10)))
         (list (car r) (nth 2 r)))
       ;; Single Newton step from x=1000
       (funcall 'neovm--opt-nr-step 1000)
       ;; Optimization: one step from x=0 should jump to x=5000
       (funcall 'neovm--opt-nr-optim-step 0)
       ;; Optimization: already at optimum
       (funcall 'neovm--opt-nr-optim-step 5000)
       ;; f(3000) should be near 0
       (funcall 'neovm--opt-nr-f 3000))
    (fmakunbound 'neovm--opt-nr-f)
    (fmakunbound 'neovm--opt-nr-fprime)
    (fmakunbound 'neovm--opt-nr-step)
    (fmakunbound 'neovm--opt-nr-run)
    (fmakunbound 'neovm--opt-nr-optim-step)))"#;
    let expect = expect_test::expect![[r#""OK ((3001 t 5) (3001 t) (-3001 t) 5000 5000 5000 0)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Nelder-Mead simplex method (2D)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_optimization_nelder_mead_2d() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Nelder-Mead for 2D: minimize f(x,y) = (x-2)^2 + (y-3)^2.
  ;; Simplex is a triangle (3 vertices in 2D).
  ;; All coordinates scaled by 100.

  (fset 'neovm--opt-nm-f
    (lambda (pt)
      "f(x,y) = (x-200)^2/100 + (y-300)^2/100."
      (let ((x (car pt)) (y (cdr pt)))
        (+ (/ (* (- x 200) (- x 200)) 100)
           (/ (* (- y 300) (- y 300)) 100)))))

  ;; Sort simplex vertices by function value (ascending)
  (fset 'neovm--opt-nm-sort-simplex
    (lambda (simplex)
      (sort (copy-sequence simplex)
            (lambda (a b)
              (< (funcall 'neovm--opt-nm-f a)
                 (funcall 'neovm--opt-nm-f b))))))

  ;; Centroid of all points except the worst
  (fset 'neovm--opt-nm-centroid
    (lambda (sorted-simplex)
      "Centroid of first n-1 points (excluding worst = last)."
      (let ((n (1- (length sorted-simplex)))
            (sx 0) (sy 0)
            (pts sorted-simplex)
            (i 0))
        (while (< i n)
          (setq sx (+ sx (car (car pts))))
          (setq sy (+ sy (cdr (car pts))))
          (setq pts (cdr pts))
          (setq i (1+ i)))
        (cons (/ sx n) (/ sy n)))))

  ;; Reflection: reflected = centroid + alpha*(centroid - worst)
  ;; alpha = 1 (standard)
  (fset 'neovm--opt-nm-reflect
    (lambda (centroid worst)
      (cons (- (* 2 (car centroid)) (car worst))
            (- (* 2 (cdr centroid)) (cdr worst)))))

  ;; Contraction: contracted = centroid + rho*(worst - centroid)
  ;; rho = 0.5 -> midpoint
  (fset 'neovm--opt-nm-contract
    (lambda (centroid worst)
      (cons (/ (+ (car centroid) (car worst)) 2)
            (/ (+ (cdr centroid) (cdr worst)) 2))))

  ;; One NM iteration
  (fset 'neovm--opt-nm-step
    (lambda (simplex)
      (let* ((sorted (funcall 'neovm--opt-nm-sort-simplex simplex))
             (best (car sorted))
             (worst (car (last sorted)))
             (cent (funcall 'neovm--opt-nm-centroid sorted))
             (reflected (funcall 'neovm--opt-nm-reflect cent worst))
             (fr (funcall 'neovm--opt-nm-f reflected))
             (fb (funcall 'neovm--opt-nm-f best))
             (fw (funcall 'neovm--opt-nm-f worst))
             (second-worst (nth (- (length sorted) 2) sorted))
             (fsw (funcall 'neovm--opt-nm-f second-worst)))
        (cond
         ;; Reflected is better than second worst but not best: accept reflection
         ((and (<= fr fsw) (>= fr fb))
          (let ((new-simplex (copy-sequence sorted)))
            (setcar (last new-simplex) reflected)
            new-simplex))
         ;; Reflected is best: accept
         ((< fr fb)
          (let ((new-simplex (copy-sequence sorted)))
            (setcar (last new-simplex) reflected)
            new-simplex))
         ;; Otherwise: contract
         (t
          (let* ((contracted (funcall 'neovm--opt-nm-contract cent worst))
                 (fc (funcall 'neovm--opt-nm-f contracted)))
            (if (< fc fw)
                (let ((new-simplex (copy-sequence sorted)))
                  (setcar (last new-simplex) contracted)
                  new-simplex)
              ;; Shrink: move all points towards best
              (let ((new-simplex (list best)))
                (dolist (pt (cdr sorted))
                  (push (cons (/ (+ (car best) (car pt)) 2)
                              (/ (+ (cdr best) (cdr pt)) 2))
                        new-simplex))
                (nreverse new-simplex)))))))))

  ;; Run NM for N steps
  (fset 'neovm--opt-nm-run
    (lambda (simplex n)
      (let ((s simplex) (i 0))
        (while (< i n)
          (setq s (funcall 'neovm--opt-nm-step s))
          (setq i (1+ i)))
        (funcall 'neovm--opt-nm-sort-simplex s))))

  (unwind-protect
      (let* ((initial (list (cons 0 0) (cons 500 0) (cons 0 500)))
             (result (funcall 'neovm--opt-nm-run initial 30)))
        (let ((best (car result)))
          (list
           ;; Best vertex after 30 iterations
           best
           ;; Function value at best
           (funcall 'neovm--opt-nm-f best)
           ;; Should be near (200, 300)
           (< (+ (abs (- (car best) 200)) (abs (- (cdr best) 300))) 100)
           ;; Function values at initial vertices
           (mapcar 'neovm--opt-nm-f initial)
           ;; Centroid of initial (sorted)
           (funcall 'neovm--opt-nm-centroid
                    (funcall 'neovm--opt-nm-sort-simplex initial))
           ;; Single step result
           (let ((one-step (funcall 'neovm--opt-nm-step initial)))
             (list (car one-step)
                   (funcall 'neovm--opt-nm-f (car one-step)))))))
    (fmakunbound 'neovm--opt-nm-f)
    (fmakunbound 'neovm--opt-nm-sort-simplex)
    (fmakunbound 'neovm--opt-nm-centroid)
    (fmakunbound 'neovm--opt-nm-reflect)
    (fmakunbound 'neovm--opt-nm-contract)
    (fmakunbound 'neovm--opt-nm-step)
    (fmakunbound 'neovm--opt-nm-run)))"#;
    let expect = expect_test::expect![[
        r#""OK ((195 . 296) 0 t (1300 1800 800) (0 . 250) ((0 . 500) 800))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
