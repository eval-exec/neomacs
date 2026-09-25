//! Oracle parity for lever P0.11 (the bignum allocation diet): bignums in an
//! arena class, results built in place, the arithmetic opcodes answering
//! integer operands directly, and the rounding functions as fixed 1..2
//! argument subrs. Values must be exact, `eq` must stay object identity
//! (fresh arithmetic results; GNU's `rounding_driver` returns an integer
//! numerator ITSELF for an omitted divisor), and markers, floats and
//! non-numbers must still take GNU's coercing or signalling paths.
//!
//! Expectations are GNU's, refreshed with `NEOVM_ORACLE_MODE=refresh
//! UPDATE_EXPECT=1 NEOVM_FORCE_ORACLE_PATH=...`; never hand-written.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_prop_bignum_rounding_returns_integer_numerator_itself() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(let ((b (* most-positive-fixnum 4)))
                    (list (eq (truncate b) b)
                          (eq (floor b nil) b)
                          (eq (ceiling b) b)
                          (eq (round b nil) b)
                          (eq (round 5) 5)
                          (truncate 7.5)
                          (floor -7.5)
                          (round 2.5)
                          (truncate b 3)
                          ;; Bignum quotients: the harness zeroes fixnums
                          ;; above 10^12 in magnitude.
                          (floor (- (expt 2 80)) 7)
                          (ceiling (expt 2 80) -7)
                          (round (expt 2 80) 3)))"#;
    let expect = expect_test::expect![[
        r#""OK (t t t t t 7 -8 2 3074457345618258601 -172703688516375596386597 -172703688516375596386596 402975273204876391568725)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_bignum_rounding_arity_and_type_errors() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(list (func-arity 'truncate)
                        (func-arity 'round)
                        (condition-case e (truncate) (error (car e)))
                        (condition-case e (floor 1 2 3) (error (car e)))
                        (condition-case e (ceiling 'x) (error e))
                        (condition-case e (round 1 'x) (error e))
                        (condition-case e (truncate 1.0e+INF) (error (car e)))
                        (condition-case e (floor (expt 2 70) 0) (error (car e))))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 . 2) (1 . 2) wrong-number-of-arguments wrong-number-of-arguments (wrong-type-argument numberp x) (wrong-type-argument numberp x) overflow-error arith-error)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_bignum_arithmetic_results_are_fresh_and_exact() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(let ((b (* most-positive-fixnum 4))
                        (n (- (expt 3 90))))
                    (list (eq (+ b 0) b)
                          (eq (* b 1) b)
                          (eq (- b 0) b)
                          (eq (1+ (1- b)) b)
                          (= (+ b 0) b)
                          (* b n)
                          (+ b n)
                          (- n b)
                          (1+ most-positive-fixnum)
                          (1- most-negative-fixnum)
                          (- most-negative-fixnum)
                          (* most-positive-fixnum most-negative-fixnum)
                          (- (expt 2 64) (expt 2 64))
                          (list (< b n) (> b n) (= b b) (<= n n) (>= n b))))"#;
    let expect = expect_test::expect![[
        r#""OK (nil nil nil nil t -80501255112587440860371321016528640558138256898892874962299996 -8727963568087712425891388256104690485265645 -8727963568087712425891406702848764194817253 2305843009213693952 -2305843009213693953 2305843009213693952 -5316911983139663489309385231907684352 0 (nil t t t nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_bignum_sxhash_equal_agrees_across_construction_paths() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    // Raw sxhash values are not comparable across engines; equality is.
    let form = r#"(let* ((x (expt 7 50))
                         (y (* (expt 7 25) (expt 7 25)))
                         (z (1- (1+ x)))
                         (w (truncate (* x 3) 3)))
                    (list (eql x y) (equal x z) (eq x y)
                          (= (sxhash-equal x) (sxhash-equal y))
                          (= (sxhash-equal x) (sxhash-equal z))
                          (= (sxhash-eql x) (sxhash-eql w))
                          (let ((h (make-hash-table :test 'eql)))
                            (puthash x 'found h)
                            (list (gethash y h) (gethash w h)))))"#;
    let expect = expect_test::expect![[r#""OK (t t nil t t t (found found))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_bignum_markers_and_floats_take_the_full_builtin() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(with-temp-buffer
                    (insert "abcdef")
                    (goto-char 4)
                    (let ((b (expt 2 70)))
                      (list (+ b (point-marker))
                            (- b (point-marker))
                            (* (point-marker) b)
                            (< (point-marker) b)
                            (* b 1.5)
                            (+ b 0.5)
                            (< b 1.0e30)
                            (> b 1.0e30)
                            (= b (float b))
                            (condition-case e (+ b "s") (error e))
                            (condition-case e (* b nil) (error e)))))"#;
    let expect = expect_test::expect![[
        r#""OK (1180591620717411303428 1180591620717411303420 4722366482869645213696 t 1.770887431076117e+21 1.1805916207174113e+21 t nil t (wrong-type-argument number-or-marker-p \"s\") (wrong-type-argument number-or-marker-p nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_bignum_pidigits_sixty_digits() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    // The elisp-benchmarks pidigits spigot, lexically bound.
    let form = r#"(let ((acc 0) (den 1) (num 1) (k 0) (res nil) (i 0))
                    (while (< i 60)
                      (setq k (1+ k))
                      (let ((k2 (1+ (* k 2))))
                        (setq acc (* (+ acc (* num 2)) k2)
                              den (* den k2)
                              num (* num k)))
                      (unless (> num acc)
                        (let ((d (truncate (+ (* num 3) acc) den)))
                          (when (= d (truncate (+ (* num 4) acc) den))
                            (push d res)
                            (setq i (1+ i))
                            (setq acc (* (- acc (* den d)) 10)
                                  num (* num 10))))))
                    (apply #'concat (mapcar #'number-to-string (nreverse res))))"#;
    let expect = expect_test::expect![[
        r#""OK \"314159265358979323846264338327950288419716939937510582097494\"""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
