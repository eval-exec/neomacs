//! Oracle parity tests for advanced signal processing algorithms in Elisp.
//!
//! Implements discrete Fourier transform (DFT), inverse DFT, linear and
//! circular convolution, FIR filter design and application, moving average
//! filter with edge handling, simple waveform synthesis (sine/square/sawtooth),
//! and signal energy/RMS computation.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Discrete Fourier Transform (DFT) — real-valued input
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sigproc_adv_dft() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; DFT: X[k] = sum_{n=0}^{N-1} x[n] * (cos(2*pi*k*n/N) - i*sin(2*pi*k*n/N))
  ;; We store complex as (real . imag)
  (defvar neovm--dft-state nil)

  (fset 'neovm--dft-forward
    (lambda (signal)
      "Compute DFT of real-valued SIGNAL. Returns list of (real . imag) pairs."
      (let* ((N (length signal))
             (vec (vconcat signal))
             (result nil))
        (let ((k 0))
          (while (< k N)
            (let ((re 0.0) (im 0.0) (n 0))
              (while (< n N)
                (let ((angle (* -2.0 float-pi k n (/ 1.0 N))))
                  (setq re (+ re (* (float (aref vec n)) (cos angle))))
                  (setq im (+ im (* (float (aref vec n)) (sin angle)))))
                (setq n (1+ n)))
              (setq result (cons (cons re im) result)))
            (setq k (1+ k))))
        (nreverse result))))

  (fset 'neovm--dft-magnitude
    (lambda (spectrum)
      "Compute magnitude of each DFT bin: |X[k]| = sqrt(re^2 + im^2)."
      (mapcar (lambda (c)
                (sqrt (+ (* (car c) (car c)) (* (cdr c) (cdr c)))))
              spectrum)))

  (unwind-protect
      (let* (;; DC signal: all 5s -> DFT should have X[0] = N*5, rest ~0
             (dc-sig '(5 5 5 5 5 5 5 5))
             (dc-dft (funcall 'neovm--dft-forward dc-sig))
             (dc-mag (funcall 'neovm--dft-magnitude dc-dft))
             ;; Impulse: [1 0 0 0] -> flat spectrum
             (impulse '(1 0 0 0))
             (imp-dft (funcall 'neovm--dft-forward impulse))
             (imp-mag (funcall 'neovm--dft-magnitude imp-dft))
             ;; Pure cosine at k=1: x[n] = cos(2*pi*n/N)
             (N 8)
             (cosine (let ((r nil) (n 0))
                       (while (< n N)
                         (setq r (cons (cos (* 2.0 float-pi n (/ 1.0 N))) r))
                         (setq n (1+ n)))
                       (nreverse r)))
             (cos-dft (funcall 'neovm--dft-forward cosine))
             (cos-mag (funcall 'neovm--dft-magnitude cos-dft))
             ;; Alternating signal: [1 -1 1 -1] -> energy at Nyquist
             (alt-sig '(1 -1 1 -1))
             (alt-dft (funcall 'neovm--dft-forward alt-sig))
             (alt-mag (funcall 'neovm--dft-magnitude alt-dft)))
        (list
         ;; DC: X[0] should be N*5 = 40
         :dc-x0-re (car (car dc-dft))
         :dc-x0-im (cdr (car dc-dft))
         :dc-mag-0 (car dc-mag)
         ;; Impulse: all magnitudes should be 1.0
         :imp-mag imp-mag
         ;; Cosine: peak at k=1 and k=N-1
         :cos-mag-0 (nth 0 cos-mag)
         :cos-mag-1 (nth 1 cos-mag)
         ;; Alternating: peak at k=N/2
         :alt-mag alt-mag
         :alt-peak-at-2 (nth 2 alt-mag)))
    (fmakunbound 'neovm--dft-forward)
    (fmakunbound 'neovm--dft-magnitude)
    (makunbound 'neovm--dft-state)))"#;
    let expect = expect_test::expect![[
        r#""OK (:dc-x0-re 40.0 :dc-x0-im 0.0 :dc-mag-0 40.0 :imp-mag (1.0 1.0 1.0 1.0) :cos-mag-0 5.551115123125783e-16 :cos-mag-1 4.0 :alt-mag (0.0 1.326935047191146e-16 4.0 5.527085992185683e-16) :alt-peak-at-2 4.0)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Inverse DFT and round-trip verification
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sigproc_adv_inverse_dft() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (defvar neovm--idft-state nil)

  (fset 'neovm--idft-forward
    (lambda (signal)
      "Forward DFT of real signal."
      (let* ((N (length signal))
             (vec (vconcat signal))
             (result nil)
             (k 0))
        (while (< k N)
          (let ((re 0.0) (im 0.0) (n 0))
            (while (< n N)
              (let ((angle (* -2.0 float-pi k n (/ 1.0 N))))
                (setq re (+ re (* (float (aref vec n)) (cos angle))))
                (setq im (+ im (* (float (aref vec n)) (sin angle)))))
              (setq n (1+ n)))
            (setq result (cons (cons re im) result)))
          (setq k (1+ k)))
        (nreverse result))))

  (fset 'neovm--idft-inverse
    (lambda (spectrum)
      "Inverse DFT: x[n] = (1/N) * sum_{k=0}^{N-1} X[k] * e^{i*2*pi*k*n/N}.
Returns list of real parts (imaginary should be ~0 for real signals)."
      (let* ((N (length spectrum))
             (spec-vec (vconcat spectrum))
             (result nil)
             (n 0))
        (while (< n N)
          (let ((re 0.0) (k 0))
            (while (< k N)
              (let* ((Xk (aref spec-vec k))
                     (Xr (car Xk))
                     (Xi (cdr Xk))
                     (angle (* 2.0 float-pi k n (/ 1.0 N)))
                     (cos-a (cos angle))
                     (sin-a (sin angle)))
                ;; (Xr + i*Xi) * (cos + i*sin) -> real part = Xr*cos - Xi*sin
                (setq re (+ re (- (* Xr cos-a) (* Xi sin-a)))))
              (setq k (1+ k)))
            (setq result (cons (/ re N) result)))
          (setq n (1+ n)))
        (nreverse result))))

  (fset 'neovm--idft-approx-equal
    (lambda (a b tol)
      "Check if all elements of lists A and B are within TOL."
      (let ((ok t))
        (while (and a b ok)
          (when (> (abs (- (float (car a)) (float (car b)))) tol)
            (setq ok nil))
          (setq a (cdr a))
          (setq b (cdr b)))
        (and ok (null a) (null b)))))

  (unwind-protect
      (let* (;; Round-trip test 1: simple signal
             (sig1 '(1 2 3 4 5 6 7 8))
             (spec1 (funcall 'neovm--idft-forward sig1))
             (recovered1 (funcall 'neovm--idft-inverse spec1))
             (rt1-ok (funcall 'neovm--idft-approx-equal
                              (mapcar #'float sig1) recovered1 1e-6))
             ;; Round-trip test 2: signal with negative values
             (sig2 '(10 -5 3 -8 12 -1 7 -3))
             (spec2 (funcall 'neovm--idft-forward sig2))
             (recovered2 (funcall 'neovm--idft-inverse spec2))
             (rt2-ok (funcall 'neovm--idft-approx-equal
                              (mapcar #'float sig2) recovered2 1e-6))
             ;; Round-trip test 3: DC signal
             (sig3 '(42 42 42 42))
             (spec3 (funcall 'neovm--idft-forward sig3))
             (recovered3 (funcall 'neovm--idft-inverse spec3))
             (rt3-ok (funcall 'neovm--idft-approx-equal
                              (mapcar #'float sig3) recovered3 1e-6))
             ;; Round-trip test 4: impulse
             (sig4 '(1 0 0 0 0 0 0 0))
             (spec4 (funcall 'neovm--idft-forward sig4))
             (recovered4 (funcall 'neovm--idft-inverse spec4))
             (rt4-ok (funcall 'neovm--idft-approx-equal
                              (mapcar #'float sig4) recovered4 1e-6)))
        (list
         :rt1-ok rt1-ok
         :rt2-ok rt2-ok
         :rt3-ok rt3-ok
         :rt4-ok rt4-ok
         :recovered1-first-3 (take 3 recovered1)
         :recovered2-first-3 (take 3 recovered2)))
    (fmakunbound 'neovm--idft-forward)
    (fmakunbound 'neovm--idft-inverse)
    (fmakunbound 'neovm--idft-approx-equal)
    (makunbound 'neovm--idft-state)))"#;
    let expect = expect_test::expect![[
        r#""OK (:rt1-ok t :rt2-ok t :rt3-ok t :rt4-ok t :recovered1-first-3 (0.999999999999999 2.0000000000000018 3.000000000000006) :recovered2-first-3 (9.999999999999996 -5.000000000000003 2.9999999999999987))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Linear and circular convolution
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sigproc_adv_convolution_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (defvar neovm--conv-state nil)

  (fset 'neovm--conv-linear
    (lambda (x h)
      "Linear convolution: output length = len(x) + len(h) - 1."
      (let* ((xv (vconcat x))
             (hv (vconcat h))
             (xn (length xv))
             (hn (length hv))
             (yn (1- (+ xn hn)))
             (result nil)
             (n 0))
        (while (< n yn)
          (let ((sum 0.0) (k 0))
            (while (< k hn)
              (let ((xi (- n k)))
                (when (and (>= xi 0) (< xi xn))
                  (setq sum (+ sum (* (float (aref xv xi))
                                      (float (aref hv k)))))))
              (setq k (1+ k)))
            (setq result (cons sum result)))
          (setq n (1+ n)))
        (nreverse result))))

  (fset 'neovm--conv-circular
    (lambda (x h)
      "Circular convolution: both X and H assumed length N. Output length = N."
      (let* ((N (length x))
             (xv (vconcat x))
             (hv (vconcat h))
             (result nil)
             (n 0))
        (while (< n N)
          (let ((sum 0.0) (k 0))
            (while (< k N)
              (let ((xi (% (+ (- n k) (* 10 N)) N)))
                (setq sum (+ sum (* (float (aref xv xi))
                                    (float (aref hv k))))))
              (setq k (1+ k)))
            (setq result (cons sum result)))
          (setq n (1+ n)))
        (nreverse result))))

  (unwind-protect
      (let* (;; Linear convolution of two short signals
             (x1 '(1 2 3))
             (h1 '(1 0 1))
             (lin1 (funcall 'neovm--conv-linear x1 h1))
             ;; Circular convolution of same signals (zero-pad h to len 3)
             (circ1 (funcall 'neovm--conv-circular x1 h1))
             ;; Linear: identity convolution with delta
             (delta4 '(0 0 1 0))
             (sig '(10 20 30 40 50))
             (lin-delta (funcall 'neovm--conv-linear sig delta4))
             ;; Circular self-convolution
             (self '(1 1 1 1))
             (circ-self (funcall 'neovm--conv-circular self self))
             ;; Commutativity test: conv(a,b) == conv(b,a)
             (a '(1 3 5))
             (b '(2 4))
             (ab (funcall 'neovm--conv-linear a b))
             (ba (funcall 'neovm--conv-linear b a))
             ;; Linearity test: conv(x, a*h1 + b*h2) == a*conv(x,h1) + b*conv(x,h2)
             ;; We'll just check lengths and a specific property
             (h2 '(0 1 0))
             (lin-h1 (funcall 'neovm--conv-linear x1 h1))
             (lin-h2 (funcall 'neovm--conv-linear x1 h2)))
        (list
         :lin1 lin1
         :circ1 circ1
         :lin1-length (length lin1)
         :circ1-length (length circ1)
         :lin-delta lin-delta
         :circ-self circ-self
         :commutativity (equal ab ba)
         :ab ab
         :ba ba
         :lin-h1 lin-h1
         :lin-h2 lin-h2))
    (fmakunbound 'neovm--conv-linear)
    (fmakunbound 'neovm--conv-circular)
    (makunbound 'neovm--conv-state)))"#;
    let expect = expect_test::expect![[
        r#""OK (:lin1 (1.0 2.0 4.0 2.0 3.0) :circ1 (3.0 5.0 4.0) :lin1-length 5 :circ1-length 3 :lin-delta (0.0 0.0 10.0 20.0 30.0 40.0 50.0 0.0) :circ-self (4.0 4.0 4.0 4.0) :commutativity t :ab (2.0 10.0 22.0 20.0) :ba (2.0 10.0 22.0 20.0) :lin-h1 (1.0 2.0 4.0 2.0 3.0) :lin-h2 (0.0 1.0 2.0 3.0 0.0))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// FIR filter design and application
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sigproc_adv_fir_filter() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (defvar neovm--fir-state nil)

  (fset 'neovm--fir-lowpass-coeffs
    (lambda (order cutoff-ratio)
      "Design simple FIR lowpass filter using sinc window method.
ORDER is filter length (odd), CUTOFF-RATIO is normalized cutoff [0,1].
Returns list of coefficients."
      (let* ((M (/ (1- order) 2))
             (coeffs nil)
             (n 0)
             (sum 0.0))
        ;; Compute ideal sinc coefficients
        (while (< n order)
          (let* ((nm (- n M))
                 (h (if (= nm 0)
                        (* 2.0 cutoff-ratio)
                      (/ (sin (* 2.0 float-pi cutoff-ratio nm))
                         (* float-pi nm))))
                 ;; Hamming window
                 (w (- 0.54 (* 0.46 (cos (/ (* 2.0 float-pi n) (1- order))))))
                 (coeff (* h w)))
            (setq sum (+ sum coeff))
            (setq coeffs (cons coeff coeffs)))
          (setq n (1+ n)))
        ;; Normalize so coefficients sum to 1
        (mapcar (lambda (c) (/ c sum)) (nreverse coeffs)))))

  (fset 'neovm--fir-apply
    (lambda (signal coeffs)
      "Apply FIR filter with COEFFS to SIGNAL. Output length = len(signal)."
      (let* ((sv (vconcat signal))
             (cv (vconcat coeffs))
             (sn (length sv))
             (cn (length cv))
             (half (/ (1- cn) 2))
             (result nil)
             (n 0))
        (while (< n sn)
          (let ((sum 0.0) (k 0))
            (while (< k cn)
              (let ((si (- n (- k half))))
                (when (and (>= si 0) (< si sn))
                  (setq sum (+ sum (* (float (aref sv si))
                                      (float (aref cv k)))))))
              (setq k (1+ k)))
            (setq result (cons sum result)))
          (setq n (1+ n)))
        (nreverse result))))

  (unwind-protect
      (let* (;; Design a 7-tap lowpass filter with cutoff at 0.25
             (coeffs (funcall 'neovm--fir-lowpass-coeffs 7 0.25))
             ;; Coefficients should sum to ~1.0
             (coeff-sum (let ((s 0.0)) (dolist (c coeffs) (setq s (+ s c))) s))
             ;; Apply to a mixed signal: low frequency + high frequency
             (mixed (let ((r nil) (n 0))
                      (while (< n 32)
                        (let ((low (* 10 (sin (* 2.0 float-pi n (/ 1.0 16)))))
                              (high (* 5 (sin (* 2.0 float-pi n (/ 1.0 3))))))
                          (setq r (cons (+ low high) r)))
                        (setq n (1+ n)))
                      (nreverse r)))
             (filtered (funcall 'neovm--fir-apply mixed coeffs))
             ;; Apply to constant signal: should stay constant
             (const-sig (make-list 20 10.0))
             (const-filt (funcall 'neovm--fir-apply const-sig coeffs))
             ;; Filter length preserved
             (len-preserved (= (length filtered) (length mixed))))
        (list
         :coeffs coeffs
         :coeff-count (length coeffs)
         :coeff-sum coeff-sum
         :len-preserved len-preserved
         :filtered-first-5 (take 5 filtered)
         :filtered-length (length filtered)
         :const-middle (nth 10 const-filt)))
    (fmakunbound 'neovm--fir-lowpass-coeffs)
    (fmakunbound 'neovm--fir-apply)
    (makunbound 'neovm--fir-state)))"#;
    let expect = expect_test::expect![[
        r#""OK (:coeffs (-0.008721828105096871 6.2084235941293e-18 0.25184278653467207 0.5137580831408495 0.2518427865346722 6.2084235941293e-18 -0.008721828105096871) :coeff-count 7 :coeff-sum 1.0 :len-preserved t :filtered-first-5 (1.9736926894453999 4.7560060860525075 5.746363759662703 8.984058425648602 10.782868022332094) :filtered-length 32 :const-middle 9.999999999999998)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Moving average filter with edge handling modes
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sigproc_adv_moving_average_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (defvar neovm--ma-state nil)

  (fset 'neovm--ma-valid
    (lambda (signal window)
      "Moving average, 'valid' mode: output only where full window fits.
Output length = len(signal) - window + 1."
      (let* ((sv (vconcat signal))
             (n (length sv))
             (result nil)
             (i 0))
        (while (<= (+ i window) n)
          (let ((sum 0.0) (j 0))
            (while (< j window)
              (setq sum (+ sum (float (aref sv (+ i j)))))
              (setq j (1+ j)))
            (setq result (cons (/ sum window) result)))
          (setq i (1+ i)))
        (nreverse result))))

  (fset 'neovm--ma-same
    (lambda (signal window)
      "Moving average, 'same' mode: output same length as input.
Uses partial windows at edges."
      (let* ((sv (vconcat signal))
             (n (length sv))
             (half (/ (1- window) 2))
             (result nil)
             (i 0))
        (while (< i n)
          (let ((sum 0.0) (count 0) (j (- i half)))
            (while (< j (+ (- i half) window))
              (when (and (>= j 0) (< j n))
                (setq sum (+ sum (float (aref sv j))))
                (setq count (1+ count)))
              (setq j (1+ j)))
            (setq result (cons (if (> count 0) (/ sum count) 0.0) result)))
          (setq i (1+ i)))
        (nreverse result))))

  (unwind-protect
      (let* ((signal '(10 20 30 40 50 60 70 80 90 100))
             ;; Valid mode: window 3
             (valid-3 (funcall 'neovm--ma-valid signal 3))
             ;; Same mode: window 3
             (same-3 (funcall 'neovm--ma-same signal 3))
             ;; Valid mode: window 5
             (valid-5 (funcall 'neovm--ma-valid signal 5))
             ;; Same mode: window 5
             (same-5 (funcall 'neovm--ma-same signal 5))
             ;; Window 1 = identity
             (valid-1 (funcall 'neovm--ma-valid signal 1))
             ;; Step signal
             (step '(0 0 0 0 0 100 100 100 100 100))
             (step-valid (funcall 'neovm--ma-valid step 3))
             (step-same (funcall 'neovm--ma-same step 3)))
        (list
         :valid-3 valid-3
         :valid-3-len (length valid-3)
         :same-3 same-3
         :same-3-len (length same-3)
         :valid-5 valid-5
         :same-5 same-5
         :identity-ok (equal (mapcar #'float signal) valid-1)
         :step-valid step-valid
         :step-same step-same))
    (fmakunbound 'neovm--ma-valid)
    (fmakunbound 'neovm--ma-same)
    (makunbound 'neovm--ma-state)))"#;
    let expect = expect_test::expect![[
        r#""OK (:valid-3 (20.0 30.0 40.0 50.0 60.0 70.0 80.0 90.0) :valid-3-len 8 :same-3 (15.0 20.0 30.0 40.0 50.0 60.0 70.0 80.0 90.0 95.0) :same-3-len 10 :valid-5 (30.0 40.0 50.0 60.0 70.0 80.0) :same-5 (20.0 25.0 30.0 40.0 50.0 60.0 70.0 80.0 85.0 90.0) :identity-ok t :step-valid (0.0 0.0 0.0 33.333333333333336 66.66666666666667 100.0 100.0 100.0) :step-same (0.0 0.0 0.0 0.0 33.333333333333336 66.66666666666667 100.0 100.0 100.0 100.0))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Waveform synthesis: sine, square, sawtooth
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sigproc_adv_waveform_synthesis() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (defvar neovm--wave-state nil)

  (fset 'neovm--wave-sine
    (lambda (freq samples sample-rate amplitude)
      "Generate sine wave: A * sin(2*pi*f*t)."
      (let ((result nil) (i 0))
        (while (< i samples)
          (let ((t-val (/ (float i) sample-rate)))
            (setq result (cons (* amplitude (sin (* 2.0 float-pi freq t-val)))
                               result)))
          (setq i (1+ i)))
        (nreverse result))))

  (fset 'neovm--wave-square
    (lambda (freq samples sample-rate amplitude)
      "Generate square wave: +A or -A based on sign of sine."
      (let ((result nil) (i 0))
        (while (< i samples)
          (let* ((t-val (/ (float i) sample-rate))
                 (phase (sin (* 2.0 float-pi freq t-val))))
            (setq result (cons (if (>= phase 0) amplitude (- amplitude))
                               result)))
          (setq i (1+ i)))
        (nreverse result))))

  (fset 'neovm--wave-sawtooth
    (lambda (freq samples sample-rate amplitude)
      "Generate sawtooth wave: linear ramp from -A to +A per period."
      (let ((result nil) (i 0))
        (while (< i samples)
          (let* ((t-val (/ (float i) sample-rate))
                 (phase (- (* 2.0 (- (* freq t-val) (floor (* freq t-val)))) 1.0)))
            (setq result (cons (* amplitude phase) result)))
          (setq i (1+ i)))
        (nreverse result))))

  (fset 'neovm--wave-energy
    (lambda (signal)
      "Total energy of signal: sum of x[n]^2."
      (let ((e 0.0))
        (dolist (x signal)
          (setq e (+ e (* (float x) (float x)))))
        e)))

  (fset 'neovm--wave-rms
    (lambda (signal)
      "RMS (root mean square) of signal."
      (let ((N (length signal)))
        (if (= N 0) 0.0
          (sqrt (/ (funcall 'neovm--wave-energy signal) N))))))

  (unwind-protect
      (let* ((freq 4.0)
             (samples 32)
             (sr 32.0)
             (amp 1.0)
             ;; Generate waveforms
             (sine (funcall 'neovm--wave-sine freq samples sr amp))
             (square (funcall 'neovm--wave-square freq samples sr amp))
             (saw (funcall 'neovm--wave-sawtooth freq samples sr amp))
             ;; Compute energy
             (sine-energy (funcall 'neovm--wave-energy sine))
             (square-energy (funcall 'neovm--wave-energy square))
             (saw-energy (funcall 'neovm--wave-energy saw))
             ;; RMS values
             (sine-rms (funcall 'neovm--wave-rms sine))
             (square-rms (funcall 'neovm--wave-rms square))
             (saw-rms (funcall 'neovm--wave-rms saw))
             ;; Lengths
             (all-same-len (and (= (length sine) samples)
                                (= (length square) samples)
                                (= (length saw) samples)))
             ;; Square wave RMS should equal amplitude (for whole-period signals)
             ;; Sine RMS should be amp/sqrt(2)
             ;; DC offset test: sum of full-period sine should be ~0
             (sine-dc (let ((s 0.0)) (dolist (x sine) (setq s (+ s x))) s)))
        (list
         :sine-first-8 (take 8 sine)
         :square-first-8 (take 8 square)
         :saw-first-8 (take 8 saw)
         :sine-energy sine-energy
         :square-energy square-energy
         :saw-energy saw-energy
         :sine-rms sine-rms
         :square-rms square-rms
         :saw-rms saw-rms
         :all-same-len all-same-len
         :sine-dc-near-zero (< (abs sine-dc) 1e-6)
         ;; Square wave should have highest energy for same amplitude
         :square-highest-energy (and (> square-energy sine-energy)
                                     (> square-energy saw-energy))))
    (fmakunbound 'neovm--wave-sine)
    (fmakunbound 'neovm--wave-square)
    (fmakunbound 'neovm--wave-sawtooth)
    (fmakunbound 'neovm--wave-energy)
    (fmakunbound 'neovm--wave-rms)
    (makunbound 'neovm--wave-state)))"#;
    let expect = expect_test::expect![[
        r#""OK (:sine-first-8 (0.0 0.7071067811865475 1.0 0.7071067811865476 1.2246467991473532e-16 -0.7071067811865475 -1.0 -0.7071067811865477) :square-first-8 (1.0 1.0 1.0 1.0 1.0 -1.0 -1.0 -1.0) :saw-first-8 (-1.0 -0.75 -0.5 -0.25 0.0 0.25 0.5 0.75) :sine-energy 16.00000000000001 :square-energy 32.0 :saw-energy 11.0 :sine-rms 0.7071067811865478 :square-rms 1.0 :saw-rms 0.5863019699779287 :all-same-len t :sine-dc-near-zero t :square-highest-energy t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Signal energy and RMS with windowed analysis
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sigproc_adv_energy_rms_windowed() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (defvar neovm--erms-state nil)

  (fset 'neovm--erms-windowed-energy
    (lambda (signal window-size hop-size)
      "Compute short-time energy in windows of WINDOW-SIZE, advancing by HOP-SIZE."
      (let* ((sv (vconcat signal))
             (n (length sv))
             (result nil)
             (pos 0))
        (while (<= (+ pos window-size) n)
          (let ((e 0.0) (j 0))
            (while (< j window-size)
              (let ((val (float (aref sv (+ pos j)))))
                (setq e (+ e (* val val))))
              (setq j (1+ j)))
            (setq result (cons e result)))
          (setq pos (+ pos hop-size)))
        (nreverse result))))

  (fset 'neovm--erms-windowed-rms
    (lambda (signal window-size hop-size)
      "Compute short-time RMS."
      (mapcar (lambda (e) (sqrt (/ e window-size)))
              (funcall 'neovm--erms-windowed-energy signal window-size hop-size))))

  (fset 'neovm--erms-zero-crossing-rate
    (lambda (signal)
      "Count zero crossings per sample."
      (let* ((sv (vconcat signal))
             (n (length sv))
             (crossings 0)
             (i 1))
        (while (< i n)
          (when (or (and (>= (aref sv (1- i)) 0) (< (aref sv i) 0))
                    (and (< (aref sv (1- i)) 0) (>= (aref sv i) 0)))
            (setq crossings (1+ crossings)))
          (setq i (1+ i)))
        (if (> (1- n) 0)
            (/ (float crossings) (1- n))
          0.0))))

  (unwind-protect
      (let* (;; Signal with varying amplitude sections
             (signal (append (make-list 16 0)          ;; silence
                             (let ((r nil) (i 0))       ;; sine burst
                               (while (< i 16)
                                 (setq r (cons (* 10 (sin (* 2.0 float-pi i (/ 1.0 8)))) r))
                                 (setq i (1+ i)))
                               (nreverse r))
                             (make-list 16 0)           ;; silence again
                             (let ((r nil) (i 0))       ;; louder sine
                               (while (< i 16)
                                 (setq r (cons (* 20 (sin (* 2.0 float-pi i (/ 1.0 8)))) r))
                                 (setq i (1+ i)))
                               (nreverse r))))
             ;; Windowed energy with window=8, hop=8
             (win-energy (funcall 'neovm--erms-windowed-energy signal 8 8))
             ;; Windowed RMS
             (win-rms (funcall 'neovm--erms-windowed-rms signal 8 8))
             ;; Zero crossing rate of different signals
             (zcr-sine (funcall 'neovm--erms-zero-crossing-rate
                                (let ((r nil) (i 0))
                                  (while (< i 32)
                                    (setq r (cons (sin (* 2.0 float-pi i (/ 1.0 8))) r))
                                    (setq i (1+ i)))
                                  (nreverse r))))
             (zcr-dc (funcall 'neovm--erms-zero-crossing-rate (make-list 32 5.0)))
             ;; Alternating signal: maximum zero crossings
             (zcr-alt (funcall 'neovm--erms-zero-crossing-rate
                               '(1 -1 1 -1 1 -1 1 -1 1 -1))))
        (list
         :win-energy win-energy
         :win-rms win-rms
         :energy-count (length win-energy)
         ;; Silence windows should have ~0 energy
         :silence-1-energy (nth 0 win-energy)
         :silence-2-energy (nth 1 win-energy)
         ;; Loud section should have higher energy than quiet
         :loud-vs-quiet (> (nth 6 win-energy) (nth 2 win-energy))
         :zcr-sine zcr-sine
         :zcr-dc zcr-dc
         :zcr-alt zcr-alt
         ;; DC signal should have 0 zero crossings
         :dc-no-crossings (= zcr-dc 0.0)))
    (fmakunbound 'neovm--erms-windowed-energy)
    (fmakunbound 'neovm--erms-windowed-rms)
    (fmakunbound 'neovm--erms-zero-crossing-rate)
    (makunbound 'neovm--erms-state)))"#;
    let expect = expect_test::expect![[
        r#""OK (:win-energy (0.0 0.0 400.0 400.0000000000002 0.0 0.0 1600.0 1600.000000000001) :win-rms (0.0 0.0 7.0710678118654755 7.071067811865477 0.0 0.0 14.142135623730951 14.142135623730955) :energy-count 8 :silence-1-energy 0.0 :silence-2-energy 0.0 :loud-vs-quiet t :zcr-sine 0.22580645161290322 :zcr-dc 0.0 :zcr-alt 1.0 :dc-no-crossings t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
