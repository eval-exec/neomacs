//! Advanced oracle parity tests for math function patterns:
//! `expt` with integer/float bases and exponents, `sqrt` precision,
//! `log` with optional base argument, `exp`, combined for compound
//! interest, geometric series, logarithmic scale conversion, and
//! Newton's method root finding.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// expt: integer and float, positive and negative exponents
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_expt_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Integer base, integer exponent
  (expt 2 0)
  (expt 2 1)
  (expt 2 10)
  (expt 2 20)
  (expt 3 5)
  (expt 10 6)
  (expt -2 3)    ;; negative base, odd exponent -> negative
  (expt -2 4)    ;; negative base, even exponent -> positive
  (expt -3 0)    ;; anything^0 = 1
  (expt 1 1000)  ;; 1^anything = 1
  (expt 0 5)     ;; 0^positive = 0
  ;; Float base, integer exponent
  (expt 2.0 10)
  (expt 0.5 3)
  (expt 1.5 4)
  ;; Float base, float exponent
  (expt 2.0 0.5)   ;; sqrt(2)
  (expt 4.0 0.5)   ;; sqrt(4) = 2.0
  (expt 27.0 (/ 1.0 3.0))  ;; cube root of 27
  ;; Negative float exponent
  (expt 2.0 -1.0)  ;; 0.5
  (expt 10.0 -2.0) ;; 0.01
  ;; Identity: x^1 = x
  (= (expt 42 1) 42)
  (= (expt 3.14 1) 3.14))"#;
    let expect = expect_test::expect![[
        r#""OK (1 2 1024 1048576 243 1000000 -8 16 1 1 0 1024.0 0.125 5.0625 1.4142135623730951 2.0 3.0 0.5 0.01 t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_expt_laws_verification() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Verify exponent laws hold with float arithmetic
    let form = r#"(let ((a 2.0) (b 3.0) (m 4.0) (n 5.0))
  (list
    ;; a^m * a^n = a^(m+n)
    (< (abs (- (* (expt a m) (expt a n))
               (expt a (+ m n))))
       1e-8)
    ;; (a^m)^n = a^(m*n)
    (< (abs (- (expt (expt a m) n)
               (expt a (* m n))))
       1e-4)
    ;; (a*b)^n = a^n * b^n
    (< (abs (- (expt (* a b) n)
               (* (expt a n) (expt b n))))
       1e-8)
    ;; a^(-n) = 1/(a^n)
    (< (abs (- (expt a (- n))
               (/ 1.0 (expt a n))))
       1e-10)
    ;; a^0 = 1
    (= (expt a 0) 1.0)
    (= (expt b 0) 1.0)))"#;
    let expect = expect_test::expect![[r#""OK (t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// sqrt precision and relationships
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sqrt_precision_and_identities() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Perfect squares
  (sqrt 0.0)
  (sqrt 1.0)
  (sqrt 4.0)
  (sqrt 9.0)
  (sqrt 16.0)
  (sqrt 25.0)
  (sqrt 100.0)
  (sqrt 10000.0)
  ;; Non-perfect squares
  (sqrt 2.0)
  (sqrt 3.0)
  (sqrt 5.0)
  ;; Verify sqrt(x)^2 = x for various values
  (let ((vals '(1.0 2.0 3.0 7.5 100.0 0.01 9999.0))
        (all-ok t))
    (dolist (v vals)
      (let ((s (sqrt v)))
        (unless (< (abs (- (* s s) v)) 1e-8)
          (setq all-ok nil))))
    all-ok)
  ;; sqrt(a*b) = sqrt(a) * sqrt(b)
  (let ((a 12.0) (b 3.0))
    (< (abs (- (sqrt (* a b))
               (* (sqrt a) (sqrt b))))
       1e-10))
  ;; sqrt(a/b) = sqrt(a) / sqrt(b)
  (let ((a 50.0) (b 2.0))
    (< (abs (- (sqrt (/ a b))
               (/ (sqrt a) (sqrt b))))
       1e-10)))"#;
    let expect = expect_test::expect![[
        r#""OK (0.0 1.0 2.0 3.0 4.0 5.0 10.0 100.0 1.4142135623730951 1.7320508075688772 2.23606797749979 t t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// log: natural log, log with base, relationships
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_log_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Natural log (base e)
  (log 1)       ;; 0.0
  (log (exp 1)) ;; 1.0
  (log (exp 2)) ;; ~2.0
  ;; Log with base argument
  (log 8 2)     ;; 3.0 (log base 2 of 8)
  (log 100 10)  ;; 2.0
  (log 27 3)    ;; 3.0
  (log 1 10)    ;; 0.0
  (log 10 10)   ;; 1.0
  ;; Log laws: log(a*b) = log(a) + log(b)
  (let ((a 5.0) (b 7.0))
    (< (abs (- (log (* a b))
               (+ (log a) (log b))))
       1e-10))
  ;; Log law: log(a/b) = log(a) - log(b)
  (let ((a 20.0) (b 4.0))
    (< (abs (- (log (/ a b))
               (- (log a) (log b))))
       1e-10))
  ;; Log law: log(a^n) = n * log(a)
  (let ((a 3.0) (n 5.0))
    (< (abs (- (log (expt a n))
               (* n (log a))))
       1e-8))
  ;; Change of base: log_b(x) = log(x) / log(b)
  (let ((x 100.0) (b 10.0))
    (< (abs (- (log x b)
               (/ (log x) (log b))))
       1e-10))
  ;; log(exp(x)) roundtrip
  (let ((x 3.7))
    (< (abs (- (log (exp x)) x)) 1e-10))
  ;; exp(log(x)) roundtrip
  (let ((x 42.0))
    (< (abs (- (exp (log x)) x)) 1e-8)))"#;
    let expect = expect_test::expect![[r#""OK (0.0 1.0 2.0 3.0 2.0 3.0 0.0 1.0 t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Compound interest calculation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_compound_interest_calculations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; A = P * (1 + r/n)^(n*t)
  ;; P=principal, r=annual rate, n=compounds per year, t=years
  (fset 'neovm--test-compound-interest
    (lambda (principal rate compounds-per-year years)
      (let ((base (+ 1.0 (/ rate (float compounds-per-year))))
            (exp (* compounds-per-year years)))
        (* (float principal) (expt base exp)))))

  ;; Time to double: t = log(2) / (n * log(1 + r/n))
  (fset 'neovm--test-doubling-time
    (lambda (rate compounds-per-year)
      (/ (log 2.0)
         (* compounds-per-year
            (log (+ 1.0 (/ rate (float compounds-per-year))))))))

  ;; Continuous compounding: A = P * e^(r*t)
  (fset 'neovm--test-continuous-compound
    (lambda (principal rate years)
      (* (float principal) (exp (* rate (float years))))))

  ;; Effective annual rate: (1 + r/n)^n - 1
  (fset 'neovm--test-effective-rate
    (lambda (nominal-rate compounds)
      (- (expt (+ 1.0 (/ nominal-rate (float compounds)))
               (float compounds))
         1.0)))

  (unwind-protect
      (list
        ;; $1000 at 5% compounded annually for 10 years
        (let ((result (funcall 'neovm--test-compound-interest 1000 0.05 1 10)))
          (floor (* result 100)))  ;; cents, truncated
        ;; $1000 at 5% compounded monthly for 10 years
        (let ((result (funcall 'neovm--test-compound-interest 1000 0.05 12 10)))
          (floor (* result 100)))
        ;; $1000 at 5% compounded daily for 10 years
        (let ((result (funcall 'neovm--test-compound-interest 1000 0.05 365 10)))
          (floor (* result 100)))
        ;; Continuous compounding for comparison
        (let ((result (funcall 'neovm--test-continuous-compound 1000 0.05 10)))
          (floor (* result 100)))
        ;; Doubling time at 7% compounded monthly
        (let ((years (funcall 'neovm--test-doubling-time 0.07 12)))
          (floor (* years 100)))  ;; years * 100 for precision
        ;; Rule of 72 approximation: 72/r years
        (let ((r 7))
          (floor (* (/ 72.0 r) 100)))
        ;; Effective annual rate: 12% nominal compounded monthly
        (let ((ear (funcall 'neovm--test-effective-rate 0.12 12)))
          (floor (* ear 10000)))  ;; basis points
        ;; Effective annual rate: 12% nominal compounded daily
        (let ((ear (funcall 'neovm--test-effective-rate 0.12 365)))
          (floor (* ear 10000)))
        ;; Growth comparison: $1000 at different rates for 30 years
        (let ((results nil))
          (dolist (rate '(0.03 0.05 0.07 0.10))
            (setq results
                  (cons (floor (funcall 'neovm--test-compound-interest 1000 rate 12 30))
                        results)))
          (nreverse results)))
    (fmakunbound 'neovm--test-compound-interest)
    (fmakunbound 'neovm--test-doubling-time)
    (fmakunbound 'neovm--test-continuous-compound)
    (fmakunbound 'neovm--test-effective-rate)))"#;
    let expect = expect_test::expect![[
        r#""OK (162889 164700 164866 164872 993 1028 1268 1274 (2456 4467 8116 19837))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Geometric series and logarithmic scale conversion
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_geometric_series_and_log_scale() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Geometric series sum: a * (1 - r^n) / (1 - r)
  (fset 'neovm--test-geometric-sum
    (lambda (a r n)
      (if (= r 1.0)
          (* a (float n))
        (* (float a) (/ (- 1.0 (expt r n))
                        (- 1.0 r))))))

  ;; Infinite geometric series sum (|r| < 1): a / (1 - r)
  (fset 'neovm--test-geometric-infinite-sum
    (lambda (a r)
      (/ (float a) (- 1.0 r))))

  ;; Linear to decibel: dB = 20 * log10(x)
  (fset 'neovm--test-to-decibels
    (lambda (linear)
      (* 20.0 (log (float linear) 10))))

  ;; Decibel to linear: x = 10^(dB/20)
  (fset 'neovm--test-from-decibels
    (lambda (db)
      (expt 10.0 (/ db 20.0))))

  ;; Richter scale: magnitude = log10(amplitude)
  ;; Energy ratio: 10^(1.5 * (m2 - m1))
  (fset 'neovm--test-richter-energy-ratio
    (lambda (m1 m2)
      (expt 10.0 (* 1.5 (- (float m2) (float m1))))))

  (unwind-protect
      (list
        ;; Geometric series: 1 + 2 + 4 + 8 + 16 (a=1, r=2, n=5)
        (floor (funcall 'neovm--test-geometric-sum 1 2.0 5))
        ;; Geometric series: 1 + 0.5 + 0.25 + ... (n=10)
        (let ((result (funcall 'neovm--test-geometric-sum 1 0.5 10)))
          (floor (* result 1000)))
        ;; Infinite series 1 + 0.5 + 0.25 + ... = 2
        (let ((result (funcall 'neovm--test-geometric-infinite-sum 1 0.5)))
          (floor (* result 1000)))
        ;; Compare finite (n=20) with infinite for r=0.5
        (let ((finite (funcall 'neovm--test-geometric-sum 1 0.5 20))
              (infinite (funcall 'neovm--test-geometric-infinite-sum 1 0.5)))
          (< (abs (- finite infinite)) 0.001))
        ;; Decibel conversions
        (floor (funcall 'neovm--test-to-decibels 1.0))   ;; 0 dB
        (floor (funcall 'neovm--test-to-decibels 10.0))  ;; 20 dB
        (floor (funcall 'neovm--test-to-decibels 100.0)) ;; 40 dB
        (floor (funcall 'neovm--test-to-decibels 0.1))   ;; -20 dB
        ;; Roundtrip: linear -> dB -> linear
        (let ((original 42.0))
          (< (abs (- (funcall 'neovm--test-from-decibels
                       (funcall 'neovm--test-to-decibels original))
                     original))
             1e-8))
        ;; Richter scale: magnitude 7 vs 5 energy ratio
        (floor (funcall 'neovm--test-richter-energy-ratio 5 7))
        ;; Each magnitude step = ~31.6x energy
        (floor (funcall 'neovm--test-richter-energy-ratio 0 1)))
    (fmakunbound 'neovm--test-geometric-sum)
    (fmakunbound 'neovm--test-geometric-infinite-sum)
    (fmakunbound 'neovm--test-to-decibels)
    (fmakunbound 'neovm--test-from-decibels)
    (fmakunbound 'neovm--test-richter-energy-ratio)))"#;
    let expect = expect_test::expect![[r#""OK (31 1998 2000 t 0 20 40 -20 t 1000 31)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Newton's method for root finding
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_newtons_method_root_finding() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Generic Newton's method: find x where f(x) = 0
  ;; x_{n+1} = x_n - f(x_n) / f'(x_n)
  (fset 'neovm--test-newton
    (lambda (f df x0 tolerance max-iter)
      (let ((x (float x0))
            (iter 0)
            (converged nil)
            (history nil))
        (while (and (< iter max-iter) (not converged))
          (let* ((fx (funcall f x))
                 (dfx (funcall df x))
                 (step (if (= dfx 0.0) 1e-10 (/ fx dfx)))
                 (x-new (- x step)))
            (setq history (cons (list iter x fx) history))
            (when (< (abs step) tolerance)
              (setq converged t))
            (setq x x-new)
            (setq iter (1+ iter))))
        (list :root x
              :converged converged
              :iterations iter
              :history-len (length history)))))

  ;; Newton's method for sqrt: find x where x^2 - n = 0
  (fset 'neovm--test-newton-sqrt
    (lambda (n)
      (let ((result (funcall 'neovm--test-newton
                      (lambda (x) (- (* x x) (float n)))
                      (lambda (x) (* 2.0 x))
                      (float n)
                      1e-12
                      100)))
        (plist-get result :root))))

  ;; Newton's method for cube root: find x where x^3 - n = 0
  (fset 'neovm--test-newton-cbrt
    (lambda (n)
      (let ((result (funcall 'neovm--test-newton
                      (lambda (x) (- (* x x x) (float n)))
                      (lambda (x) (* 3.0 x x))
                      (float n)
                      1e-12
                      100)))
        (plist-get result :root))))

  ;; Newton's method for nth root: find x where x^n - val = 0
  (fset 'neovm--test-newton-nthroot
    (lambda (val n)
      (let ((nf (float n)))
        (let ((result (funcall 'neovm--test-newton
                        (lambda (x) (- (expt x nf) (float val)))
                        (lambda (x) (* nf (expt x (- nf 1.0))))
                        (float val)
                        1e-10
                        200)))
          (plist-get result :root)))))

  (unwind-protect
      (list
        ;; Sqrt via Newton vs built-in sqrt
        (let ((test-vals '(2.0 3.0 10.0 100.0 0.25)))
          (let ((all-close t))
            (dolist (v test-vals)
              (let ((newton-result (funcall 'neovm--test-newton-sqrt v))
                    (builtin-result (sqrt v)))
                (unless (< (abs (- newton-result builtin-result)) 1e-8)
                  (setq all-close nil))))
            all-close))
        ;; Cube root: 8 -> 2, 27 -> 3, 125 -> 5
        (floor (* 1000 (funcall 'neovm--test-newton-cbrt 8.0)))
        (floor (* 1000 (funcall 'neovm--test-newton-cbrt 27.0)))
        (floor (* 1000 (funcall 'neovm--test-newton-cbrt 125.0)))
        ;; 4th root of 16 = 2
        (floor (* 1000 (funcall 'neovm--test-newton-nthroot 16.0 4)))
        ;; 5th root of 32 = 2
        (floor (* 1000 (funcall 'neovm--test-newton-nthroot 32.0 5)))
        ;; Solve x^2 - 5x + 6 = 0 (roots at 2 and 3)
        ;; Starting near 1.5 should converge to 2
        (let ((result (funcall 'neovm--test-newton
                        (lambda (x) (+ (* x x) (* -5.0 x) 6.0))
                        (lambda (x) (+ (* 2.0 x) -5.0))
                        1.5
                        1e-10
                        50)))
          (floor (* 1000 (plist-get result :root))))
        ;; Starting near 4.0 should converge to 3
        (let ((result (funcall 'neovm--test-newton
                        (lambda (x) (+ (* x x) (* -5.0 x) 6.0))
                        (lambda (x) (+ (* 2.0 x) -5.0))
                        4.0
                        1e-10
                        50)))
          (floor (* 1000 (plist-get result :root)))))
    (fmakunbound 'neovm--test-newton)
    (fmakunbound 'neovm--test-newton-sqrt)
    (fmakunbound 'neovm--test-newton-cbrt)
    (fmakunbound 'neovm--test-newton-nthroot)))"#;
    let expect = expect_test::expect![[r#""OK (t 2000 3000 5000 2000 2000 2000 3000)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// exp/log in numerical algorithms: sigmoid, softmax, log-sum-exp
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_exp_log_numerical_algorithms() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Sigmoid: 1 / (1 + exp(-x))
  (fset 'neovm--test-sigmoid
    (lambda (x)
      (/ 1.0 (+ 1.0 (exp (- x))))))

  ;; Log-sum-exp trick: log(sum(exp(x_i))) = max + log(sum(exp(x_i - max)))
  ;; Numerically stable version
  (fset 'neovm--test-log-sum-exp
    (lambda (values)
      (let ((max-val (apply 'max values)))
        (+ max-val
           (log (let ((sum 0.0))
                  (dolist (v values)
                    (setq sum (+ sum (exp (- v max-val)))))
                  sum))))))

  ;; Softmax: exp(x_i) / sum(exp(x_j)) using log-sum-exp for stability
  (fset 'neovm--test-softmax
    (lambda (values)
      (let ((lse (funcall 'neovm--test-log-sum-exp values)))
        (mapcar (lambda (v) (exp (- v lse))) values))))

  ;; Entropy: -sum(p_i * log(p_i))
  (fset 'neovm--test-entropy
    (lambda (probs)
      (let ((h 0.0))
        (dolist (p probs)
          (when (> p 0.0)
            (setq h (- h (* p (log p))))))
        h)))

  (unwind-protect
      (list
        ;; Sigmoid properties
        (floor (* 1000 (funcall 'neovm--test-sigmoid 0.0)))    ;; 500 (0.5)
        (< (funcall 'neovm--test-sigmoid -10.0) 0.001)          ;; near 0
        (> (funcall 'neovm--test-sigmoid 10.0) 0.999)           ;; near 1
        ;; Sigmoid symmetry: sigmoid(x) + sigmoid(-x) = 1
        (let ((x 3.0))
          (< (abs (- (+ (funcall 'neovm--test-sigmoid x)
                        (funcall 'neovm--test-sigmoid (- x)))
                     1.0))
             1e-10))
        ;; Softmax of equal values should give uniform distribution
        (let ((probs (funcall 'neovm--test-softmax '(1.0 1.0 1.0 1.0))))
          (let ((all-close t))
            (dolist (p probs)
              (unless (< (abs (- p 0.25)) 0.001)
                (setq all-close nil)))
            all-close))
        ;; Softmax sums to 1
        (let ((probs (funcall 'neovm--test-softmax '(1.0 2.0 3.0))))
          (< (abs (- (apply '+ probs) 1.0)) 1e-8))
        ;; Softmax preserves ordering
        (let ((probs (funcall 'neovm--test-softmax '(1.0 3.0 2.0))))
          (and (< (nth 0 probs) (nth 2 probs))
               (< (nth 2 probs) (nth 1 probs))))
        ;; Entropy: uniform distribution has max entropy
        (let ((uniform-h (funcall 'neovm--test-entropy '(0.25 0.25 0.25 0.25)))
              (peaked-h (funcall 'neovm--test-entropy '(0.7 0.1 0.1 0.1))))
          (> uniform-h peaked-h))
        ;; Entropy of certainty is 0
        (< (funcall 'neovm--test-entropy '(1.0 0.0 0.0)) 1e-10)
        ;; Log-sum-exp handles large values without overflow
        (let ((result (funcall 'neovm--test-log-sum-exp '(1000.0 1001.0 1002.0))))
          (> result 999.0)))
    (fmakunbound 'neovm--test-sigmoid)
    (fmakunbound 'neovm--test-log-sum-exp)
    (fmakunbound 'neovm--test-softmax)
    (fmakunbound 'neovm--test-entropy)))"#;
    let expect = expect_test::expect![[r#""OK (500 t t t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
