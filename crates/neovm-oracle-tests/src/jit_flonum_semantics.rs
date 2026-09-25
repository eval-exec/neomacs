//! Oracle parity for the JIT's unboxed float results ("flonums",
//! `NEOVM_JIT_FLONUM`): each form byte-compiles a helper, warms it well past
//! the tier-up threshold with floats, and then prints results that probe
//! what unboxing must not change -- object identity (GNU makes one float
//! per arithmetic result: its copies are `eq`, two results are not),
//! fixnum results at float sites, contagion, signed zero and NaN bits, a
//! float reaching a fixnum-only site, a handler reading unboxed floats, a
//! collection in the middle of a chain, and a whole nbody run.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_prop_jit_flonum_identity() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (defalias 'neovm--jit-flonum-identity
    (byte-compile
     (lambda (v a b)
       (let ((x (* a b)))
         (aset v 0 x)
         (aset v 1 x)
         (list (eq (aref v 0) (aref v 1))
               (eq (aset v 2 x) x)
               (eq (* a b) (* a b))
               (eql (* a b) (* a b))
               x)))))
  (let ((v (make-vector 3 nil)) (r nil))
    (dotimes (_ 3000) (setq r (neovm--jit-flonum-identity v 1.5 2.0)))
    (let ((c (copy-sequence v)))
      (list r
            (eq (aref c 0) (aref c 1))
            (eq (aref c 1) (aref c 2))
            (neovm--jit-flonum-identity v 3 4)))))"#;
    let expect = expect_test::expect![[r#""OK ((t t nil t 3.0) t t (t t t t 12))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_jit_flonum_fixnum_and_mixed_chains() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (defalias 'neovm--jit-flonum-chain
    (byte-compile (lambda (a b c) (- (* (+ a b) c) (/ a c)))))
  (dotimes (_ 3000) (neovm--jit-flonum-chain 1.5 2.5 4.0))
  (list (neovm--jit-flonum-chain 1.5 2.5 4.0)
        (neovm--jit-flonum-chain 2 3 4)
        (neovm--jit-flonum-chain 7 2 2)
        (neovm--jit-flonum-chain 2 3 4.0)
        (neovm--jit-flonum-chain 2.0 3 4)
        (neovm--jit-flonum-chain 9007199254740992 1 1)
        (neovm--jit-flonum-chain most-positive-fixnum 1 1)
        (condition-case err (neovm--jit-flonum-chain 1.5 'x 2.0)
          (error (car err)))))"#;
    let expect = expect_test::expect![[r#""OK (15.625 20 15 19.5 19.5 1 1 wrong-type-argument)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_jit_flonum_signed_zero_and_nan() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (defalias 'neovm--jit-flonum-bits
    (byte-compile (lambda (a b c) (/ (- (* a b) c) b))))
  (dotimes (_ 3000) (neovm--jit-flonum-bits 1.5 2.0 0.5))
  (format "%S"
          (list (neovm--jit-flonum-bits -0.0 1.0 0.0)
                (neovm--jit-flonum-bits 0.0 -1.0 0.0)
                (neovm--jit-flonum-bits -0.0 -0.0 -0.0)
                (neovm--jit-flonum-bits 1.0e+INF 1.0 0.0)
                (neovm--jit-flonum-bits -1.0e+INF 2.0 1.0)
                (neovm--jit-flonum-bits -0.0e+NaN 1.0 0.0)
                (neovm--jit-flonum-bits 1.0 0.0e+NaN 0.0)
                (neovm--jit-flonum-bits 0.0 0.0 0.0)
                (neovm--jit-flonum-bits 1.0 0.0 1.0))))"#;
    let expect = expect_test::expect![[
        r#""OK \"(-0.0 0.0 -0.0e+NaN 1.0e+INF -1.0e+INF -0.0e+NaN 0.0e+NaN -0.0e+NaN -1.0e+INF)\"""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_jit_flonum_into_a_cold_add1() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    // The `1+` never runs while warming, so its site stays fixnum-only.
    let form = r#"(progn
  (defalias 'neovm--jit-flonum-add1
    (byte-compile
     (lambda (a b n) (let ((x (* a b))) (if (> n 100) (1+ x) x)))))
  (dotimes (_ 3000) (neovm--jit-flonum-add1 1.5 2.0 3))
  (list (neovm--jit-flonum-add1 1.5 2.0 200)
        (neovm--jit-flonum-add1 2 3 200)
        (neovm--jit-flonum-add1 1.5 2.0 3)))"#;
    let expect = expect_test::expect![[r#""OK (4.0 7 3.0)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_jit_flonum_handler_reads_unboxed_floats() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (defalias 'neovm--jit-flonum-signal (lambda () (signal 'error '("boom"))))
  (defalias 'neovm--jit-flonum-handler
    (byte-compile
     (lambda (a b)
       (let ((x 0.0) (y 0.0))
         (condition-case nil
             (progn (setq y (setq x (* a b))) (neovm--jit-flonum-signal))
           (error (list x y (eq x y))))))))
  (dotimes (_ 3000) (neovm--jit-flonum-handler 1.5 2.0))
  (list (neovm--jit-flonum-handler 1.5 2.0)
        (neovm--jit-flonum-handler 2 3)))"#;
    let expect = expect_test::expect![[r#""OK ((3.0 3.0 t) (6 6 t))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_jit_flonum_collection_mid_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    // The callee is called on every iteration and collects on the last.
    let form = r#"(progn
  (defvar neovm--jit-flonum-calls 0)
  (defalias 'neovm--jit-flonum-maybe-gc
    (lambda ()
      (when (= (setq neovm--jit-flonum-calls (1+ neovm--jit-flonum-calls)) 3001)
        (garbage-collect))
      nil))
  (defalias 'neovm--jit-flonum-gc
    (byte-compile
     (lambda (a b)
       (let* ((x (* a b)) (y (+ x a)))
         (neovm--jit-flonum-maybe-gc)
         (list (+ x y) (* y y) (eq x x))))))
  (dotimes (_ 3000) (neovm--jit-flonum-gc 1.5 2.0))
  (list (neovm--jit-flonum-gc 1.5 2.0) neovm--jit-flonum-calls))"#;
    let expect = expect_test::expect![[r#""OK ((7.5 20.25 t) 3001)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_jit_flonum_nbody_energy() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    // elisp-benchmarks' nbody with the struct accessors written as the
    // `aref`/`aset` they inline to: 1,000 steps, energy printed exactly.
    let form = r#"(progn
  (defun neovm--nb-applyforces (a b dt)
    (let* ((dx (- (aref a 0) (aref b 0)))
           (dy (- (aref a 1) (aref b 1)))
           (dz (- (aref a 2) (aref b 2)))
           (distance (sqrt (+ (* dx dx) (* dy dy) (* dz dz))))
           (mag (/ dt (* distance distance distance)))
           (dxmag (* dx mag))
           (dymag (* dy mag))
           (dzmag (* dz mag)))
      (aset a 3 (- (aref a 3) (* dxmag (aref b 6))))
      (aset a 4 (- (aref a 4) (* dymag (aref b 6))))
      (aset a 5 (- (aref a 5) (* dzmag (aref b 6))))
      (aset b 3 (+ (aref b 3) (* dxmag (aref a 6))))
      (aset b 4 (+ (aref b 4) (* dymag (aref a 6))))
      (aset b 5 (+ (aref b 5) (* dzmag (aref a 6)))))
    nil)
  (defun neovm--nb-advance (system dt)
    (let ((s system))
      (while s
        (let ((a (car s)) (rest (cdr s)))
          (while rest
            (neovm--nb-applyforces a (car rest) dt)
            (setq rest (cdr rest))))
        (setq s (cdr s))))
    (dolist (b system)
      (aset b 0 (+ (aref b 0) (* dt (aref b 3))))
      (aset b 1 (+ (aref b 1) (* dt (aref b 4))))
      (aset b 2 (+ (aref b 2) (* dt (aref b 5)))))
    nil)
  (defun neovm--nb-energy (system)
    (let ((e 0.0) (s system))
      (while s
        (let ((a (car s)) (rest (cdr s)))
          (setq e (+ e (* 0.5 (aref a 6)
                          (+ (* (aref a 3) (aref a 3))
                             (* (aref a 4) (aref a 4))
                             (* (aref a 5) (aref a 5))))))
          (while rest
            (let* ((b (car rest))
                   (dx (- (aref a 0) (aref b 0)))
                   (dy (- (aref a 1) (aref b 1)))
                   (dz (- (aref a 2) (aref b 2)))
                   (dist (sqrt (+ (* dx dx) (* dy dy) (* dz dz)))))
              (setq e (- e (/ (* (aref a 6) (aref b 6)) dist))))
            (setq rest (cdr rest))))
        (setq s (cdr s)))
      e))
  (byte-compile 'neovm--nb-applyforces)
  (byte-compile 'neovm--nb-advance)
  (byte-compile 'neovm--nb-energy)
  (let* ((days 365.24)
         (solar (* 4 float-pi float-pi))
         (system
          (list (vector 0.0 0.0 0.0 0.0 0.0 0.0 solar)
                (vector 4.84143144246472090 -1.16032004402742839
                        -1.03622044471123109e-1
                        (* 1.66007664274403694e-3 days)
                        (* 7.69901118419740425e-3 days)
                        (* -6.90460016972063023e-5 days)
                        (* 9.54791938424326609e-4 solar))
                (vector 8.34336671824457987 4.12479856412430479
                        -4.03523417114321381e-1
                        (* -2.76742510726862411e-3 days)
                        (* 4.99852801234917238e-3 days)
                        (* 2.30417297573763929e-5 days)
                        (* 2.85885980666130812e-4 solar))
                (vector 1.28943695621391310e1 -1.51111514016986312e1
                        -2.23307578892655734e-1
                        (* 2.96460137564761618e-03 days)
                        (* 2.37847173959480950e-03 days)
                        (* -2.96589568540237556e-05 days)
                        (* 4.36624404335156298e-05 solar))
                (vector 1.53796971148509165e+01 -2.59193146099879641e+01
                        1.79258772950371181e-01
                        (* 2.68067772490389322e-03 days)
                        (* 1.62824170038242295e-03 days)
                        (* -9.51592254519715870e-05 days)
                        (* 5.15138902046611451e-05 solar)))))
    (dotimes (_ 1000) (neovm--nb-advance system 0.01))
    (list (format "%.17g" (neovm--nb-energy system))
          (format "%.17g" (aref (nth 1 system) 0))
          (format "%.17g" (aref (nth 4 system) 5)))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"-0.16930106330263495\" \"1.5419460155769258\" \"-0.034808518486852409\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
