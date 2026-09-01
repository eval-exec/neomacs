//! Oracle parity tests for probability and statistics in pure Elisp.
//!
//! Covers: mean/median/mode/variance/stddev computation, probability distributions
//! (binomial coefficient, Poisson), cumulative distribution, z-score, percentile
//! computation, correlation coefficient, chi-square statistic, Fisher-Yates shuffle.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

// ---------------------------------------------------------------------------
// Mean, median, mode, variance, stddev
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_stats_descriptive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--stat-mean
    (lambda (xs)
      (/ (apply '+ (mapcar 'float xs)) (float (length xs)))))

  (fset 'neovm--stat-variance
    (lambda (xs)
      "Population variance."
      (let ((mu (funcall 'neovm--stat-mean xs))
            (n (float (length xs))))
        (/ (apply '+ (mapcar (lambda (x)
                               (let ((d (- (float x) mu)))
                                 (* d d)))
                             xs))
           n))))

  (fset 'neovm--stat-stddev
    (lambda (xs) (sqrt (funcall 'neovm--stat-variance xs))))

  (fset 'neovm--stat-median
    (lambda (xs)
      (let* ((sorted (sort (copy-sequence xs) '<))
             (n (length sorted))
             (mid (/ n 2)))
        (if (= (% n 2) 0)
            (/ (+ (float (nth (1- mid) sorted))
                  (float (nth mid sorted)))
               2.0)
          (float (nth mid sorted))))))

  (fset 'neovm--stat-mode
    (lambda (xs)
      "Return the mode (most frequent). If tied, return smallest."
      (let ((counts (make-hash-table :test 'equal))
            (max-count 0)
            (mode nil))
        (dolist (x xs)
          (let ((c (1+ (gethash x counts 0))))
            (puthash x c counts)))
        (maphash (lambda (k v)
                   (when (or (> v max-count)
                             (and (= v max-count)
                                  (or (null mode) (< k mode))))
                     (setq max-count v)
                     (setq mode k)))
                 counts)
        (list mode max-count))))

  (unwind-protect
      (let* ((data '(4 7 13 2 1 9 7 11 3 7 5 8))
             (mu (funcall 'neovm--stat-mean data))
             (var (funcall 'neovm--stat-variance data))
             (sd (funcall 'neovm--stat-stddev data))
             (med (funcall 'neovm--stat-median data))
             (mode-result (funcall 'neovm--stat-mode data))
             ;; Edge cases
             (single (funcall 'neovm--stat-mean '(42)))
             (two (funcall 'neovm--stat-median '(10 20)))
             ;; Uniform: all same => variance = 0
             (uniform-var (funcall 'neovm--stat-variance '(5 5 5 5 5)))
             ;; Symmetric data: mean = median
             (sym-data '(1 2 3 4 5 6 7))
             (sym-mean (funcall 'neovm--stat-mean sym-data))
             (sym-median (funcall 'neovm--stat-median sym-data)))
        (list
         :mean mu
         :variance var
         :stddev sd
         :median med
         :mode mode-result
         :single-mean single
         :two-median two
         :uniform-variance uniform-var
         :variance-zero (= uniform-var 0.0)
         :sym-mean-eq-median (= sym-mean sym-median)
         ;; stddev^2 ~ variance
         :sd-squared-near-var (< (abs (- (* sd sd) var)) 1e-10)))
    (fmakunbound 'neovm--stat-mean)
    (fmakunbound 'neovm--stat-variance)
    (fmakunbound 'neovm--stat-stddev)
    (fmakunbound 'neovm--stat-median)
    (fmakunbound 'neovm--stat-mode)))"#;
    let expect = expect_test::expect![[
        r#""OK (:mean 6.416666666666667 :variance 11.909722222222223 :stddev 3.451046540141443 :median 7.0 :mode (7 3) :single-mean 42.0 :two-median 15.0 :uniform-variance 0.0 :variance-zero t :sym-mean-eq-median t :sd-squared-near-var t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Binomial coefficient and binomial distribution
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_stats_binomial() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--stat-factorial
    (lambda (n)
      (if (<= n 1) 1
        (let ((result 1) (i 2))
          (while (<= i n)
            (setq result (* result i))
            (setq i (1+ i)))
          result))))

  (fset 'neovm--stat-choose
    (lambda (n k)
      "Binomial coefficient C(n,k) = n! / (k! * (n-k)!)"
      (if (or (< k 0) (> k n)) 0
        (/ (funcall 'neovm--stat-factorial n)
           (* (funcall 'neovm--stat-factorial k)
              (funcall 'neovm--stat-factorial (- n k)))))))

  (fset 'neovm--stat-binom-pmf
    (lambda (n k p)
      "P(X=k) for X ~ Binomial(n,p).
Uses integer C(n,k) and float p."
      (let ((coeff (funcall 'neovm--stat-choose n k)))
        (* (float coeff)
           (expt (float p) k)
           (expt (- 1.0 (float p)) (- n k))))))

  (unwind-protect
      (let* (;; Pascal's triangle row 5: C(5,0)..C(5,5)
             (pascal-5 (let ((r nil) (k 0))
                         (while (<= k 5)
                           (push (funcall 'neovm--stat-choose 5 k) r)
                           (setq k (1+ k)))
                         (nreverse r)))
             ;; Pascal's triangle row 10
             (pascal-10 (let ((r nil) (k 0))
                          (while (<= k 10)
                            (push (funcall 'neovm--stat-choose 10 k) r)
                            (setq k (1+ k)))
                          (nreverse r)))
             ;; Sum of row should equal 2^n
             (sum-5 (apply '+ pascal-5))
             (sum-10 (apply '+ pascal-10))
             ;; Binomial PMF: fair coin, 10 flips
             (pmf-5-of-10 (funcall 'neovm--stat-binom-pmf 10 5 0.5))
             ;; P(X=0) + ... + P(X=10) should ~ 1.0
             (total-pmf (let ((sum 0.0) (k 0))
                          (while (<= k 10)
                            (setq sum (+ sum (funcall 'neovm--stat-binom-pmf 10 k 0.5)))
                            (setq k (1+ k)))
                          sum))
             ;; Edge cases
             (c-0-0 (funcall 'neovm--stat-choose 0 0))
             (c-5-0 (funcall 'neovm--stat-choose 5 0))
             (c-5-5 (funcall 'neovm--stat-choose 5 5))
             (c-5-6 (funcall 'neovm--stat-choose 5 6)))
        (list
         :pascal-5 pascal-5
         :pascal-10 pascal-10
         :sum-5 sum-5
         :sum-5-correct (= sum-5 32)
         :sum-10-correct (= sum-10 1024)
         :pmf-5-of-10 pmf-5-of-10
         :total-pmf-near-1 (< (abs (- total-pmf 1.0)) 1e-10)
         :c-0-0 c-0-0
         :c-5-0 c-5-0
         :c-5-5 c-5-5
         :c-5-6 c-5-6
         ;; Symmetry: C(n,k) = C(n, n-k)
         :symmetry (= (funcall 'neovm--stat-choose 10 3)
                       (funcall 'neovm--stat-choose 10 7))))
    (fmakunbound 'neovm--stat-factorial)
    (fmakunbound 'neovm--stat-choose)
    (fmakunbound 'neovm--stat-binom-pmf)))"#;
    let expect = expect_test::expect![[
        r#""OK (:pascal-5 (1 5 10 10 5 1) :pascal-10 (1 10 45 120 210 252 210 120 45 10 1) :sum-5 32 :sum-5-correct t :sum-10-correct t :pmf-5-of-10 0.24609375 :total-pmf-near-1 t :c-0-0 1 :c-5-0 1 :c-5-5 1 :c-5-6 0 :symmetry t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Poisson distribution approximation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_stats_poisson() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--stat-factorial
    (lambda (n)
      (if (<= n 1) 1
        (let ((result 1) (i 2))
          (while (<= i n)
            (setq result (* result i))
            (setq i (1+ i)))
          result))))

  (fset 'neovm--stat-poisson-pmf
    (lambda (lambda-param k)
      "P(X=k) for X ~ Poisson(lambda) = e^(-lambda) * lambda^k / k!"
      (/ (* (exp (- (float lambda-param)))
            (expt (float lambda-param) k))
         (float (funcall 'neovm--stat-factorial k)))))

  (fset 'neovm--stat-poisson-cdf
    (lambda (lambda-param k-max)
      "P(X <= k-max) = sum_{k=0}^{k-max} pmf(k)."
      (let ((sum 0.0) (k 0))
        (while (<= k k-max)
          (setq sum (+ sum (funcall 'neovm--stat-poisson-pmf lambda-param k)))
          (setq k (1+ k)))
        sum)))

  (unwind-protect
      (let* (;; Lambda = 3: compute PMF for k=0..8
             (pmf-values (let ((r nil) (k 0))
                           (while (<= k 8)
                             (push (funcall 'neovm--stat-poisson-pmf 3.0 k) r)
                             (setq k (1+ k)))
                           (nreverse r)))
             ;; CDF at k=8 should be close to 1
             (cdf-8 (funcall 'neovm--stat-poisson-cdf 3.0 8))
             ;; CDF at k=20 should be very close to 1
             (cdf-20 (funcall 'neovm--stat-poisson-cdf 3.0 20))
             ;; Lambda = 1
             (p-1-0 (funcall 'neovm--stat-poisson-pmf 1.0 0))
             (p-1-1 (funcall 'neovm--stat-poisson-pmf 1.0 1))
             ;; P(X=0) when lambda=1 should be e^-1
             (e-inv (exp -1.0)))
        (list
         :pmf-values pmf-values
         :sum-pmf (apply '+ pmf-values)
         :cdf-8 cdf-8
         :cdf-8-near-1 (> cdf-8 0.99)
         :cdf-20-near-1 (> cdf-20 0.9999)
         :p-1-0 p-1-0
         :p-1-0-is-e-inv (< (abs (- p-1-0 e-inv)) 1e-14)
         :p-1-1 p-1-1
         ;; P(X=0, lambda=1) = P(X=1, lambda=1) = e^-1
         :p1-0-eq-p1-1 (< (abs (- p-1-0 p-1-1)) 1e-14)))
    (fmakunbound 'neovm--stat-factorial)
    (fmakunbound 'neovm--stat-poisson-pmf)
    (fmakunbound 'neovm--stat-poisson-cdf)))"#;
    let expect = expect_test::expect![[
        r#""OK (:pmf-values (0.049787068367863944 0.14936120510359183 0.22404180765538775 0.22404180765538775 0.16803135574154082 0.10081881344492448 0.05040940672246225 0.02160403145248382 0.008101511794681432) :sum-pmf 0.9961970079383241 :cdf-8 0.9961970079383241 :cdf-8-near-1 t :cdf-20-near-1 t :p-1-0 0.36787944117144233 :p-1-0-is-e-inv t :p-1-1 0.36787944117144233 :p1-0-eq-p1-1 t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Z-score and percentile computation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_stats_zscore_percentile() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--stat-mean
    (lambda (xs) (/ (apply '+ (mapcar 'float xs)) (float (length xs)))))

  (fset 'neovm--stat-stddev
    (lambda (xs)
      (let* ((mu (funcall 'neovm--stat-mean xs))
             (n (float (length xs)))
             (var (/ (apply '+ (mapcar (lambda (x)
                                         (let ((d (- (float x) mu))) (* d d)))
                                       xs))
                     n)))
        (sqrt var))))

  (fset 'neovm--stat-zscore
    (lambda (x mu sigma)
      "Standard score: (x - mu) / sigma."
      (if (= sigma 0.0) 0.0
        (/ (- (float x) (float mu)) (float sigma)))))

  (fset 'neovm--stat-zscores
    (lambda (xs)
      "Compute z-scores for each element."
      (let ((mu (funcall 'neovm--stat-mean xs))
            (sd (funcall 'neovm--stat-stddev xs)))
        (mapcar (lambda (x) (funcall 'neovm--stat-zscore x mu sd)) xs))))

  (fset 'neovm--stat-percentile
    (lambda (xs p)
      "Compute the P-th percentile (0-100) using nearest rank method."
      (let* ((sorted (sort (copy-sequence xs) '<))
             (n (length sorted))
             (rank (max 0 (min (1- n)
                               (1- (ceiling (/ (* p (float n)) 100.0)))))))
        (float (nth rank sorted)))))

  (unwind-protect
      (let* ((data '(12 15 18 22 25 28 30 35 40 45 50))
             (zscores (funcall 'neovm--stat-zscores data))
             ;; Mean of z-scores should be ~0
             (z-mean (funcall 'neovm--stat-mean zscores))
             ;; Stddev of z-scores should be ~1
             (z-sd (funcall 'neovm--stat-stddev zscores))
             ;; Percentiles
             (p25 (funcall 'neovm--stat-percentile data 25))
             (p50 (funcall 'neovm--stat-percentile data 50))
             (p75 (funcall 'neovm--stat-percentile data 75))
             (p0 (funcall 'neovm--stat-percentile data 0))
             (p100 (funcall 'neovm--stat-percentile data 100)))
        (list
         :zscores zscores
         :z-mean-near-0 (< (abs z-mean) 1e-10)
         :z-sd-near-1 (< (abs (- z-sd 1.0)) 1e-10)
         :p25 p25
         :p50 p50
         :p75 p75
         :p0-is-min (= p0 12.0)
         :p100-is-max (= p100 50.0)
         ;; p25 <= p50 <= p75
         :percentile-order (and (<= p25 p50) (<= p50 p75))))
    (fmakunbound 'neovm--stat-mean)
    (fmakunbound 'neovm--stat-stddev)
    (fmakunbound 'neovm--stat-zscore)
    (fmakunbound 'neovm--stat-zscores)
    (fmakunbound 'neovm--stat-percentile)))"#;
    let expect = expect_test::expect![[
        r#""OK (:zscores (-1.4506241932805972 -1.1959933508430456 -0.941362508405494 -0.6018547184887584 -0.3472238760512067 -0.09259303361365506 0.0771608613447127 0.5015455987406321 0.9259303361365515 1.350315073532471 1.7746998109283905) :z-mean-near-0 t :z-sd-near-1 t :p25 18.0 :p50 28.0 :p75 40.0 :p0-is-min t :p100-is-max t :percentile-order t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Pearson correlation coefficient
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_stats_correlation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--stat-mean
    (lambda (xs) (/ (apply '+ (mapcar 'float xs)) (float (length xs)))))

  (fset 'neovm--stat-correlation
    (lambda (xs ys)
      "Pearson correlation coefficient r between XS and YS."
      (let* ((n (float (length xs)))
             (mx (funcall 'neovm--stat-mean xs))
             (my (funcall 'neovm--stat-mean ys))
             (sum-xy 0.0)
             (sum-x2 0.0)
             (sum-y2 0.0)
             (rx xs) (ry ys))
        (while rx
          (let ((dx (- (float (car rx)) mx))
                (dy (- (float (car ry)) my)))
            (setq sum-xy (+ sum-xy (* dx dy)))
            (setq sum-x2 (+ sum-x2 (* dx dx)))
            (setq sum-y2 (+ sum-y2 (* dy dy))))
          (setq rx (cdr rx) ry (cdr ry)))
        (if (or (= sum-x2 0.0) (= sum-y2 0.0)) 0.0
          (/ sum-xy (sqrt (* sum-x2 sum-y2)))))))

  (unwind-protect
      (let* (;; Perfect positive correlation
             (x1 '(1 2 3 4 5 6 7 8 9 10))
             (y1 '(2 4 6 8 10 12 14 16 18 20))
             (r1 (funcall 'neovm--stat-correlation x1 y1))
             ;; Perfect negative correlation
             (y-neg '(20 18 16 14 12 10 8 6 4 2))
             (r-neg (funcall 'neovm--stat-correlation x1 y-neg))
             ;; No correlation (roughly)
             (y-rand '(5 2 8 1 9 3 7 4 10 6))
             (r-rand (funcall 'neovm--stat-correlation x1 y-rand))
             ;; Self-correlation = 1
             (r-self (funcall 'neovm--stat-correlation x1 x1))
             ;; Constant data: correlation = 0
             (y-const '(5 5 5 5 5 5 5 5 5 5))
             (r-const (funcall 'neovm--stat-correlation x1 y-const)))
        (list
         :perfect-positive r1
         :perfect-positive-is-1 (< (abs (- r1 1.0)) 1e-10)
         :perfect-negative r-neg
         :perfect-negative-is-neg1 (< (abs (- r-neg -1.0)) 1e-10)
         :random-r r-rand
         :random-abs-lt-1 (< (abs r-rand) 1.0)
         :self-correlation r-self
         :self-is-1 (< (abs (- r-self 1.0)) 1e-10)
         :constant-r r-const))
    (fmakunbound 'neovm--stat-mean)
    (fmakunbound 'neovm--stat-correlation)))"#;
    let expect = expect_test::expect![[
        r#""OK (:perfect-positive 1.0 :perfect-positive-is-1 t :perfect-negative -1.0 :perfect-negative-is-neg1 t :random-r 0.34545454545454546 :random-abs-lt-1 t :self-correlation 1.0 :self-is-1 t :constant-r 0.0)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Chi-square statistic
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_stats_chi_square() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--stat-chi-square
    (lambda (observed expected)
      "Compute chi-square statistic: sum((O-E)^2 / E)."
      (let ((sum 0.0)
            (obs observed)
            (exp expected))
        (while obs
          (let* ((o (float (car obs)))
                 (e (float (car exp)))
                 (diff (- o e)))
            (when (> e 0.0)
              (setq sum (+ sum (/ (* diff diff) e)))))
          (setq obs (cdr obs) exp (cdr exp)))
        sum)))

  (unwind-protect
      (let* (;; Fair die: 60 rolls, expect 10 each
             (observed '(8 12 10 11 9 10))
             (expected '(10 10 10 10 10 10))
             (chi2-fair (funcall 'neovm--stat-chi-square observed expected))
             ;; Perfectly matching data: chi2 = 0
             (chi2-perfect (funcall 'neovm--stat-chi-square expected expected))
             ;; Heavily skewed: all in one category
             (skewed '(60 0 0 0 0 0))
             (chi2-skewed (funcall 'neovm--stat-chi-square skewed expected))
             ;; Coin flip: 100 flips, 55 heads, 45 tails
             (coin-obs '(55 45))
             (coin-exp '(50 50))
             (chi2-coin (funcall 'neovm--stat-chi-square coin-obs coin-exp))
             ;; Degrees of freedom = categories - 1
             (df-die (1- (length observed)))
             (df-coin (1- (length coin-obs))))
        (list
         :chi2-fair chi2-fair
         :chi2-perfect chi2-perfect
         :perfect-is-zero (= chi2-perfect 0.0)
         :chi2-skewed chi2-skewed
         :chi2-coin chi2-coin
         :df-die df-die
         :df-coin df-coin
         ;; Fair die chi2 should be small (< 11.07 for p=0.05, df=5)
         :fair-not-significant (< chi2-fair 11.07)
         ;; Skewed should be large
         :skewed-significant (> chi2-skewed 11.07)))
    (fmakunbound 'neovm--stat-chi-square)))"#;
    let expect = expect_test::expect![[
        r#""OK (:chi2-fair 1.0 :chi2-perfect 0.0 :perfect-is-zero t :chi2-skewed 300.0 :chi2-coin 1.0 :df-die 5 :df-coin 1 :fair-not-significant t :skewed-significant t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Fisher-Yates shuffle (deterministic with seeded PRNG)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_stats_fisher_yates_shuffle() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Simple linear congruential generator for deterministic "random"
  (defvar neovm--stat-rng-state 12345)

  (fset 'neovm--stat-lcg-next
    (lambda ()
      "Return next pseudo-random non-negative integer."
      ;; LCG: state = (a*state + c) mod m
      (setq neovm--stat-rng-state
            (% (+ (* 1103515245 neovm--stat-rng-state) 12345)
               2147483648))
      (abs neovm--stat-rng-state)))

  (fset 'neovm--stat-shuffle
    (lambda (vec)
      "Fisher-Yates shuffle on a vector (destructive), using our LCG."
      (let ((n (length vec))
            (i 0))
        (setq i (1- n))
        (while (> i 0)
          (let* ((j (% (funcall 'neovm--stat-lcg-next) (1+ i)))
                 (tmp (aref vec i)))
            (aset vec i (aref vec j))
            (aset vec j tmp))
          (setq i (1- i))))
      vec))

  (unwind-protect
      (let* (;; Reset RNG
             (_ (setq neovm--stat-rng-state 12345))
             ;; Shuffle 1..10
             (arr1 (vconcat (number-sequence 1 10)))
             (shuffled1 (funcall 'neovm--stat-shuffle arr1))
             ;; Reset and shuffle again — should give same result (deterministic)
             (_ (setq neovm--stat-rng-state 12345))
             (arr2 (vconcat (number-sequence 1 10)))
             (shuffled2 (funcall 'neovm--stat-shuffle arr2))
             ;; Shuffle preserves all elements (sorted should match original)
             (sorted-back (sort (append shuffled1 nil) '<))
             ;; Different seed gives different result
             (_ (setq neovm--stat-rng-state 99999))
             (arr3 (vconcat (number-sequence 1 10)))
             (shuffled3 (funcall 'neovm--stat-shuffle arr3))
             ;; Shuffle of single element
             (_ (setq neovm--stat-rng-state 12345))
             (single (funcall 'neovm--stat-shuffle (vector 42)))
             ;; Shuffle of two elements
             (_ (setq neovm--stat-rng-state 12345))
             (pair (funcall 'neovm--stat-shuffle (vector 1 2))))
        (list
         :shuffled1 (append shuffled1 nil)
         :shuffled2 (append shuffled2 nil)
         :deterministic (equal (append shuffled1 nil) (append shuffled2 nil))
         :preserves-elements (equal sorted-back (number-sequence 1 10))
         :shuffled3 (append shuffled3 nil)
         :different-seed-differs (not (equal (append shuffled1 nil)
                                             (append shuffled3 nil)))
         :single (append single nil)
         :pair (append pair nil)
         :length-preserved (= (length shuffled1) 10)))
    (fmakunbound 'neovm--stat-lcg-next)
    (fmakunbound 'neovm--stat-shuffle)
    (makunbound 'neovm--stat-rng-state)))"#;
    let expect = expect_test::expect![[
        r#""OK (:shuffled1 (10 9 4 1 8 3 2 5 6 7) :shuffled2 (10 9 4 1 8 3 2 5 6 7) :deterministic t :preserves-elements t :shuffled3 (5 6 4 8 2 9 10 3 7 1) :different-seed-differs t :single (42) :pair (2 1) :length-preserved t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
