//! Oracle parity tests for signal processing algorithms in Elisp.
//!
//! Tests moving average filter, exponential smoothing, peak detection,
//! signal normalization (min-max scaling), discrete convolution,
//! and signal differentiation (finite differences).

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Moving average filter
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sigproc_moving_average() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (defvar neovm--sp-ma-state nil)

  (fset 'neovm--sp-moving-average
    (lambda (signal window-size)
      "Apply a simple moving average filter with WINDOW-SIZE to SIGNAL (list of numbers).
Returns a list of averaged values (shorter by window-size - 1)."
      (let ((result nil)
            (sig-vec (vconcat signal))
            (n (length signal)))
        (let ((i 0))
          (while (<= (+ i window-size) n)
            (let ((sum 0) (j 0))
              (while (< j window-size)
                (setq sum (+ sum (aref sig-vec (+ i j))))
                (setq j (1+ j)))
              (setq result (cons (/ (float sum) window-size) result)))
            (setq i (1+ i))))
        (nreverse result))))

  (unwind-protect
      (let* (;; Test signal: noisy sine approximation (integer scaled)
             (raw-signal '(0 31 59 81 95 100 95 81 59 31 0 -31 -59 -81 -95 -100 -95 -81 -59 -31))
             ;; Window size 3
             (ma3 (funcall 'neovm--sp-moving-average raw-signal 3))
             ;; Window size 5
             (ma5 (funcall 'neovm--sp-moving-average raw-signal 5))
             ;; Window size 1 (identity)
             (ma1 (funcall 'neovm--sp-moving-average raw-signal 1))
             ;; Constant signal should stay constant
             (constant-sig '(42 42 42 42 42 42 42 42))
             (ma-const (funcall 'neovm--sp-moving-average constant-sig 3))
             ;; Linear ramp: moving average of ramp is shifted ramp
             (ramp '(0 10 20 30 40 50 60 70 80 90 100))
             (ma-ramp (funcall 'neovm--sp-moving-average ramp 3)))
        (list
         :ma3-length (length ma3)
         :ma5-length (length ma5)
         :ma1-is-identity (equal (mapcar #'float raw-signal) ma1)
         :ma3-first-3 (take 3 ma3)
         :ma5-first-3 (take 3 ma5)
         :ma-const-all-42
         (let ((ok t))
           (dolist (v ma-const)
             (unless (= v 42.0) (setq ok nil)))
           ok)
         :ma-ramp-first-3 (take 3 ma-ramp)
         ;; Moving average should reduce variance
         :ma3-range
         (let ((mn 1e10) (mx -1e10))
           (dolist (v ma3)
             (when (< v mn) (setq mn v))
             (when (> v mx) (setq mx v)))
           (list mn mx))
         :raw-range
         (let ((mn 1e10) (mx -1e10))
           (dolist (v raw-signal)
             (when (< v mn) (setq mn v))
             (when (> v mx) (setq mx v)))
           (list mn mx))))
    (fmakunbound 'neovm--sp-moving-average)
    (makunbound 'neovm--sp-ma-state)))"#;
    let expect = expect_test::expect![[
        r#""OK (:ma3-length 18 :ma5-length 16 :ma1-is-identity t :ma3-first-3 (30.0 57.0 78.33333333333333) :ma5-first-3 (53.2 73.2 86.0) :ma-const-all-42 t :ma-ramp-first-3 (10.0 20.0 30.0) :ma3-range (-96.66666666666667 96.66666666666667) :raw-range (-100 100))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Exponential smoothing
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sigproc_exponential_smoothing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--sp-exp-smooth
    (lambda (signal alpha)
      "Apply exponential smoothing: s[0] = x[0], s[t] = alpha*x[t] + (1-alpha)*s[t-1].
ALPHA is in [0, 1]. Returns smoothed signal (same length)."
      (if (null signal)
          nil
        (let ((result (list (float (car signal))))
              (prev (float (car signal)))
              (rest (cdr signal)))
          (while rest
            (let ((smoothed (+ (* alpha (float (car rest)))
                               (* (- 1.0 alpha) prev))))
              (setq result (cons smoothed result))
              (setq prev smoothed)
              (setq rest (cdr rest))))
          (nreverse result)))))

  (unwind-protect
      (let* ((signal '(10 12 15 13 17 20 18 22 25 23 28 30))
             ;; Low alpha (heavy smoothing)
             (smooth-02 (funcall 'neovm--sp-exp-smooth signal 0.2))
             ;; Medium alpha
             (smooth-05 (funcall 'neovm--sp-exp-smooth signal 0.5))
             ;; High alpha (light smoothing)
             (smooth-09 (funcall 'neovm--sp-exp-smooth signal 0.9))
             ;; Alpha = 1.0 should be identity
             (smooth-10 (funcall 'neovm--sp-exp-smooth signal 1.0))
             ;; Alpha = 0.0 should be all first value
             (smooth-00 (funcall 'neovm--sp-exp-smooth signal 0.0))
             ;; Step signal
             (step-signal '(0 0 0 0 0 100 100 100 100 100))
             (step-smooth (funcall 'neovm--sp-exp-smooth step-signal 0.3)))
        (list
         :length-preserved (= (length smooth-02) (length signal))
         :first-preserved (= (car smooth-02) (float (car signal)))
         ;; Alpha=1.0 is identity (as floats)
         :alpha-1-identity (equal smooth-10 (mapcar #'float signal))
         ;; Alpha=0.0 all same as first
         :alpha-0-constant
         (let ((first-val (car smooth-00)) (ok t))
           (dolist (v smooth-00)
             (unless (= v first-val) (setq ok nil)))
           ok)
         ;; Higher alpha tracks signal more closely
         :smooth-02-last (car (last smooth-02))
         :smooth-05-last (car (last smooth-05))
         :smooth-09-last (car (last smooth-09))
         ;; Step response: smoothed values should monotonically increase
         :step-monotonic
         (let ((prev -1.0) (ok t))
           (dolist (v step-smooth)
             (when (< v prev) (setq ok nil))
             (setq prev v))
           ok)
         :step-smooth-last-3 (last step-smooth 3)))
    (fmakunbound 'neovm--sp-exp-smooth)))"#;
    let expect = expect_test::expect![[
        r#""OK (:length-preserved t :first-preserved t :alpha-1-identity t :alpha-0-constant t :smooth-02-last 22.683047976960005 :smooth-05-last 27.6796875 :smooth-09-last 29.75166166168 :step-monotonic t :step-smooth-last-3 (65.69999999999999 75.98999999999998 83.19299999999998))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Peak detection algorithm
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sigproc_peak_detection() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (defvar neovm--sp-peak-state nil)

  (fset 'neovm--sp-find-peaks
    (lambda (signal threshold)
      "Find local peaks in SIGNAL where value > THRESHOLD.
A peak is a point higher than both its neighbors.
Returns list of (index . value) pairs."
      (let ((peaks nil)
            (vec (vconcat signal))
            (n (length signal)))
        (when (> n 2)
          (let ((i 1))
            (while (< i (1- n))
              (let ((prev (aref vec (1- i)))
                    (curr (aref vec i))
                    (next (aref vec (1+ i))))
                (when (and (> curr prev)
                           (> curr next)
                           (> curr threshold))
                  (setq peaks (cons (cons i curr) peaks))))
              (setq i (1+ i)))))
        (nreverse peaks))))

  (fset 'neovm--sp-find-valleys
    (lambda (signal threshold)
      "Find local valleys in SIGNAL where value < THRESHOLD."
      (let ((valleys nil)
            (vec (vconcat signal))
            (n (length signal)))
        (when (> n 2)
          (let ((i 1))
            (while (< i (1- n))
              (let ((prev (aref vec (1- i)))
                    (curr (aref vec i))
                    (next (aref vec (1+ i))))
                (when (and (< curr prev)
                           (< curr next)
                           (< curr threshold))
                  (setq valleys (cons (cons i curr) valleys))))
              (setq i (1+ i)))))
        (nreverse valleys))))

  (unwind-protect
      (let* (;; Synthetic signal with known peaks
             (signal '(0 5 10 7 3 8 15 12 4 2 9 20 18 6 1 3 11 8 2))
             (peaks (funcall 'neovm--sp-find-peaks signal 0))
             (high-peaks (funcall 'neovm--sp-find-peaks signal 10))
             (valleys (funcall 'neovm--sp-find-valleys signal 100))
             ;; Monotonic signal should have no peaks (except endpoints)
             (mono-up '(1 2 3 4 5 6 7 8 9 10))
             (mono-peaks (funcall 'neovm--sp-find-peaks mono-up 0))
             ;; Constant signal: no peaks
             (const-sig '(5 5 5 5 5 5 5))
             (const-peaks (funcall 'neovm--sp-find-peaks const-sig 0))
             ;; Alternating signal: every other point is a peak
             (alt-sig '(0 10 0 10 0 10 0 10 0))
             (alt-peaks (funcall 'neovm--sp-find-peaks alt-sig 0))
             ;; Peak-to-valley analysis
             (peak-valley-pairs
              (let ((p (funcall 'neovm--sp-find-peaks signal 0))
                    (v (funcall 'neovm--sp-find-valleys signal 100))
                    (pairs nil))
                (dolist (pk p)
                  (let ((nearest-valley nil)
                        (min-dist 999))
                    (dolist (vl v)
                      (let ((dist (abs (- (car pk) (car vl)))))
                        (when (< dist min-dist)
                          (setq min-dist dist)
                          (setq nearest-valley vl))))
                    (when nearest-valley
                      (setq pairs (cons (list :peak pk :valley nearest-valley
                                              :amplitude (- (cdr pk) (cdr nearest-valley)))
                                        pairs)))))
                (nreverse pairs))))
        (list
         :all-peaks peaks
         :high-peaks high-peaks
         :valleys valleys
         :mono-peaks mono-peaks
         :const-peaks const-peaks
         :alt-peaks alt-peaks
         :alt-peak-count (length alt-peaks)
         :peak-valley-pairs peak-valley-pairs
         :total-peaks (length peaks)
         :total-valleys (length valleys)))
    (fmakunbound 'neovm--sp-find-peaks)
    (fmakunbound 'neovm--sp-find-valleys)
    (makunbound 'neovm--sp-peak-state)))"#;
    let expect = expect_test::expect![[
        r#""OK (:all-peaks ((2 . 10) (6 . 15) (11 . 20) (16 . 11)) :high-peaks ((6 . 15) (11 . 20) (16 . 11)) :valleys ((4 . 3) (9 . 2) (14 . 1)) :mono-peaks nil :const-peaks nil :alt-peaks ((1 . 10) (3 . 10) (5 . 10) (7 . 10)) :alt-peak-count 4 :peak-valley-pairs ((:peak (2 . 10) :valley (4 . 3) :amplitude 7) (:peak (6 . 15) :valley (4 . 3) :amplitude 12) (:peak (11 . 20) :valley (9 . 2) :amplitude 18) (:peak (16 . 11) :valley (14 . 1) :amplitude 10)) :total-peaks 4 :total-valleys 3)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Signal normalization (min-max scaling)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sigproc_normalization() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--sp-normalize
    (lambda (signal new-min new-max)
      "Normalize SIGNAL to [NEW-MIN, NEW-MAX] range using min-max scaling.
Returns list of floats."
      (if (null signal)
          nil
        (let ((sig-min (car signal))
              (sig-max (car signal)))
          ;; Find min and max
          (dolist (v signal)
            (when (< v sig-min) (setq sig-min v))
            (when (> v sig-max) (setq sig-max v)))
          (let ((sig-range (- sig-max sig-min))
                (new-range (- (float new-max) (float new-min))))
            (if (= sig-range 0)
                ;; Constant signal: map to midpoint
                (let ((mid (/ (+ (float new-min) (float new-max)) 2.0)))
                  (mapcar (lambda (_) mid) signal))
              (mapcar
               (lambda (v)
                 (+ (float new-min)
                    (* new-range (/ (- (float v) sig-min) (float sig-range)))))
               signal)))))))

  (unwind-protect
      (let* ((signal '(-100 -50 0 50 100 200 300))
             ;; Normalize to [0, 1]
             (norm-01 (funcall 'neovm--sp-normalize signal 0 1))
             ;; Normalize to [-1, 1]
             (norm-neg (funcall 'neovm--sp-normalize signal -1 1))
             ;; Normalize to [0, 100]
             (norm-pct (funcall 'neovm--sp-normalize signal 0 100))
             ;; Constant signal normalization
             (const-norm (funcall 'neovm--sp-normalize '(5 5 5 5) 0 1))
             ;; Single element
             (single-norm (funcall 'neovm--sp-normalize '(42) 0 1))
             ;; Already normalized signal
             (already '(0.0 0.25 0.5 0.75 1.0))
             (re-norm (funcall 'neovm--sp-normalize already 0 1)))
        (list
         :norm-01 norm-01
         :norm-01-min (car norm-01)
         :norm-01-max (car (last norm-01))
         :norm-neg norm-neg
         :norm-pct norm-pct
         ;; Verify [0,1] boundaries
         :norm-01-range-ok (and (= (car norm-01) 0.0) (= (car (last norm-01)) 1.0))
         ;; Verify [-1,1] boundaries
         :norm-neg-range-ok (and (= (car norm-neg) -1.0) (= (car (last norm-neg)) 1.0))
         ;; Constant signal maps to midpoint
         :const-norm const-norm
         :single-norm single-norm
         ;; Re-normalizing [0,1] should be identity
         :re-norm re-norm
         ;; Monotonicity preserved
         :monotonic
         (let ((prev -1e10) (ok t))
           (dolist (v norm-01)
             (when (< v prev) (setq ok nil))
             (setq prev v))
           ok)))
    (fmakunbound 'neovm--sp-normalize)))"#;
    let expect = expect_test::expect![[
        r#""OK (:norm-01 (0.0 0.125 0.25 0.375 0.5 0.75 1.0) :norm-01-min 0.0 :norm-01-max 1.0 :norm-neg (-1.0 -0.75 -0.5 -0.25 0.0 0.5 1.0) :norm-pct (0.0 12.5 25.0 37.5 50.0 75.0 100.0) :norm-01-range-ok t :norm-neg-range-ok t :const-norm (0.5 0.5 0.5 0.5) :single-norm (0.5) :re-norm (0.0 0.25 0.5 0.75 1.0) :monotonic t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Discrete convolution
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sigproc_convolution() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (defvar neovm--sp-conv-state nil)

  (fset 'neovm--sp-convolve
    (lambda (signal kernel)
      "Compute discrete convolution of SIGNAL with KERNEL.
Output length = len(signal) + len(kernel) - 1."
      (let* ((sig-vec (vconcat signal))
             (ker-vec (vconcat kernel))
             (sig-len (length sig-vec))
             (ker-len (length ker-vec))
             (out-len (1- (+ sig-len ker-len)))
             (result nil))
        (let ((n 0))
          (while (< n out-len)
            (let ((sum 0.0)
                  (k 0))
              (while (< k ker-len)
                (let ((sig-idx (- n k)))
                  (when (and (>= sig-idx 0) (< sig-idx sig-len))
                    (setq sum (+ sum (* (float (aref sig-vec sig-idx))
                                        (float (aref ker-vec k)))))))
                (setq k (1+ k)))
              (setq result (cons sum result)))
            (setq n (1+ n))))
        (nreverse result))))

  (unwind-protect
      (let* (;; Delta function convolution: identity
             (delta '(0 0 0 1 0 0 0))
             (signal '(1 2 3 4 5))
             (delta-conv (funcall 'neovm--sp-convolve signal delta))
             ;; Box filter (uniform kernel)
             (box-kernel '(0.333333 0.333333 0.333333))
             (box-conv (funcall 'neovm--sp-convolve '(0 0 0 10 0 0 0) box-kernel))
             ;; Edge detection kernel
             (edge-kernel '(-1 0 1))
             (step-signal '(0 0 0 0 10 10 10 10))
             (edge-conv (funcall 'neovm--sp-convolve step-signal edge-kernel))
             ;; Gaussian-like kernel
             (gauss-kernel '(0.1 0.2 0.4 0.2 0.1))
             (noisy '(5 2 8 3 9 1 7 4 10 6))
             (gauss-conv (funcall 'neovm--sp-convolve noisy gauss-kernel))
             ;; Self-convolution of [1, 1, 1]
             (self-conv (funcall 'neovm--sp-convolve '(1 1 1) '(1 1 1))))
        (list
         ;; Output lengths
         :delta-len (length delta-conv)
         :box-len (length box-conv)
         :edge-len (length edge-conv)
         :gauss-len (length gauss-conv)
         :self-conv self-conv
         ;; Self-convolution of [1,1,1] should be [1,2,3,2,1]
         :self-conv-expected (equal self-conv '(1.0 2.0 3.0 2.0 1.0))
         ;; Edge detection: should show spike at transition
         :edge-conv edge-conv
         ;; Box filter result
         :box-conv box-conv
         ;; Gauss smoothed
         :gauss-first-3 (take 3 gauss-conv)
         :gauss-last-3 (last gauss-conv 3)))
    (fmakunbound 'neovm--sp-convolve)
    (makunbound 'neovm--sp-conv-state)))"#;
    let expect = expect_test::expect![[
        r#""OK (:delta-len 11 :box-len 9 :edge-len 10 :gauss-len 14 :self-conv (1.0 2.0 3.0 2.0 1.0) :self-conv-expected t :edge-conv (0.0 0.0 0.0 0.0 -10.0 -10.0 0.0 0.0 10.0 10.0) :box-conv (0.0 0.0 0.0 3.33333 3.33333 3.33333 0.0 0.0 0.0) :gauss-first-3 (0.5 1.2 3.2) :gauss-last-3 (4.800000000000001 2.2 0.6000000000000001))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Signal differentiation (finite differences)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sigproc_differentiation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--sp-diff
    (lambda (signal)
      "Compute first-order forward differences: d[i] = x[i+1] - x[i].
Output length = len(signal) - 1."
      (let ((result nil)
            (vec (vconcat signal))
            (n (length signal)))
        (let ((i 0))
          (while (< i (1- n))
            (setq result (cons (- (aref vec (1+ i)) (aref vec i)) result))
            (setq i (1+ i))))
        (nreverse result))))

  (fset 'neovm--sp-diff2
    (lambda (signal)
      "Compute second-order differences: d2[i] = x[i+2] - 2*x[i+1] + x[i]."
      (let ((result nil)
            (vec (vconcat signal))
            (n (length signal)))
        (let ((i 0))
          (while (< i (- n 2))
            (setq result (cons (+ (aref vec (+ i 2))
                                  (- (* 2 (aref vec (1+ i))))
                                  (aref vec i))
                               result))
            (setq i (1+ i))))
        (nreverse result))))

  (fset 'neovm--sp-integrate
    (lambda (diffs initial)
      "Integrate (cumulative sum) DIFFS starting from INITIAL value.
Inverse of differentiation."
      (let ((result (list initial))
            (acc initial))
        (dolist (d diffs)
          (setq acc (+ acc d))
          (setq result (cons acc result)))
        (nreverse result))))

  (unwind-protect
      (let* (;; Linear signal: derivative is constant
             (linear '(0 3 6 9 12 15 18 21 24 27))
             (d-linear (funcall 'neovm--sp-diff linear))
             ;; Quadratic signal: x^2 for x in 0..9
             (quadratic (let ((r nil) (i 0))
                          (while (<= i 9)
                            (setq r (cons (* i i) r))
                            (setq i (1+ i)))
                          (nreverse r)))
             (d-quad (funcall 'neovm--sp-diff quadratic))
             (d2-quad (funcall 'neovm--sp-diff2 quadratic))
             ;; Step function: derivative shows impulse
             (step-sig '(0 0 0 0 10 10 10 10))
             (d-step (funcall 'neovm--sp-diff step-sig))
             ;; Sinusoidal approximation
             (sine-sig '(0 31 59 81 95 100 95 81 59 31 0))
             (d-sine (funcall 'neovm--sp-diff sine-sig))
             (d2-sine (funcall 'neovm--sp-diff2 sine-sig))
             ;; Integration roundtrip: integrate(diff(signal)) == signal
             (roundtrip (funcall 'neovm--sp-integrate
                                 (funcall 'neovm--sp-diff linear)
                                 (car linear)))
             ;; Constant signal: all diffs are 0
             (const-diff (funcall 'neovm--sp-diff '(7 7 7 7 7 7))))
        (list
         ;; Linear: constant derivative
         :d-linear d-linear
         :d-linear-constant
         (let ((first (car d-linear)) (ok t))
           (dolist (v d-linear)
             (unless (= v first) (setq ok nil)))
           ok)
         ;; Quadratic: linear first derivative (odd numbers: 1,3,5,7,...)
         :d-quad d-quad
         ;; Quadratic: constant second derivative (always 2)
         :d2-quad d2-quad
         :d2-quad-constant
         (let ((ok t))
           (dolist (v d2-quad)
             (unless (= v 2) (setq ok nil)))
           ok)
         ;; Step: impulse at transition
         :d-step d-step
         ;; Sine derivative: cosine-like (positive then negative)
         :d-sine d-sine
         :d2-sine d2-sine
         ;; Integration roundtrip
         :roundtrip roundtrip
         :roundtrip-ok (equal roundtrip linear)
         ;; Constant: all zeros
         :const-diff const-diff
         :const-all-zero
         (let ((ok t))
           (dolist (v const-diff)
             (unless (= v 0) (setq ok nil)))
           ok)))
    (fmakunbound 'neovm--sp-diff)
    (fmakunbound 'neovm--sp-diff2)
    (fmakunbound 'neovm--sp-integrate)))"#;
    let expect = expect_test::expect![[
        r#""OK (:d-linear (3 3 3 3 3 3 3 3 3) :d-linear-constant t :d-quad (1 3 5 7 9 11 13 15 17) :d2-quad (2 2 2 2 2 2 2 2) :d2-quad-constant t :d-step (0 0 0 10 0 0 0) :d-sine (31 28 22 14 5 -5 -14 -22 -28 -31) :d2-sine (-3 -6 -8 -9 -10 -9 -8 -6 -3) :roundtrip (0 3 6 9 12 15 18 21 24 27) :roundtrip-ok t :const-diff (0 0 0 0 0) :const-all-zero t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: full signal processing pipeline
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sigproc_full_pipeline() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Generate a noisy signal, smooth it, detect peaks, normalize,
    // and compute statistics
    let form = r#"(progn
  (defvar neovm--sp-pipeline-state nil)

  (fset 'neovm--sp-ma
    (lambda (signal window)
      (let ((result nil) (vec (vconcat signal)) (n (length signal)))
        (let ((i 0))
          (while (<= (+ i window) n)
            (let ((sum 0) (j 0))
              (while (< j window)
                (setq sum (+ sum (aref vec (+ i j))))
                (setq j (1+ j)))
              (setq result (cons (/ (float sum) window) result)))
            (setq i (1+ i))))
        (nreverse result))))

  (fset 'neovm--sp-peaks
    (lambda (signal threshold)
      (let ((peaks nil) (vec (vconcat signal)) (n (length signal)))
        (when (> n 2)
          (let ((i 1))
            (while (< i (1- n))
              (let ((prev (aref vec (1- i)))
                    (curr (aref vec i))
                    (next (aref vec (1+ i))))
                (when (and (> curr prev) (> curr next) (> curr threshold))
                  (setq peaks (cons (cons i curr) peaks))))
              (setq i (1+ i)))))
        (nreverse peaks))))

  (fset 'neovm--sp-norm
    (lambda (signal)
      (if (null signal) nil
        (let ((mn (car signal)) (mx (car signal)))
          (dolist (v signal)
            (when (< v mn) (setq mn v))
            (when (> v mx) (setq mx v)))
          (let ((range (- mx mn)))
            (if (= range 0)
                (mapcar (lambda (_) 0.5) signal)
              (mapcar (lambda (v) (/ (- (float v) mn) (float range))) signal)))))))

  (fset 'neovm--sp-stats
    (lambda (signal)
      "Compute mean and variance of a signal."
      (let ((n (length signal))
            (sum 0.0)
            (sum-sq 0.0))
        (dolist (v signal)
          (setq sum (+ sum (float v)))
          (setq sum-sq (+ sum-sq (* (float v) (float v)))))
        (let ((mean (/ sum n))
              (variance (- (/ sum-sq n) (* (/ sum n) (/ sum n)))))
          (list :mean mean :variance variance :n n)))))

  (unwind-protect
      (let* (;; Generate noisy signal: base sine + noise via hash
             (raw (let ((r nil) (i 0))
                    (while (< i 40)
                      (let* ((base (* 100 (sin (* i 0.3))))
                             ;; Deterministic "noise" via modular arithmetic
                             (noise (- (mod (* i 7 + 13) 20) 10))
                             (val (+ base noise)))
                        (setq r (cons val r)))
                      (setq i (1+ i)))
                    (nreverse r)))
             ;; Step 1: Smooth
             (smoothed (funcall 'neovm--sp-ma raw 5))
             ;; Step 2: Detect peaks on smoothed signal
             (peaks (funcall 'neovm--sp-peaks smoothed 0))
             ;; Step 3: Normalize smoothed signal
             (normalized (funcall 'neovm--sp-norm smoothed))
             ;; Step 4: Statistics on raw vs smoothed
             (raw-stats (funcall 'neovm--sp-stats raw))
             (smooth-stats (funcall 'neovm--sp-stats smoothed))
             ;; Step 5: Peak density (peaks per length)
             (peak-density (if (> (length smoothed) 0)
                               (/ (* 100 (length peaks)) (length smoothed))
                             0)))
        (list
         :raw-length (length raw)
         :smoothed-length (length smoothed)
         :peak-count (length peaks)
         :peaks peaks
         :normalized-length (length normalized)
         :norm-min (let ((mn 1e10))
                     (dolist (v normalized) (when (< v mn) (setq mn v))) mn)
         :norm-max (let ((mx -1e10))
                     (dolist (v normalized) (when (> v mx) (setq mx v))) mx)
         :raw-stats raw-stats
         :smooth-stats smooth-stats
         :peak-density peak-density
         ;; Smoothing should reduce variance
         :variance-reduced (<= (plist-get smooth-stats :variance)
                               (plist-get raw-stats :variance))))
    (fmakunbound 'neovm--sp-ma)
    (fmakunbound 'neovm--sp-peaks)
    (fmakunbound 'neovm--sp-norm)
    (fmakunbound 'neovm--sp-stats)
    (makunbound 'neovm--sp-pipeline-state)))"#;
    let expect = expect_test::expect![[r#""ERR (void-variable +)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
