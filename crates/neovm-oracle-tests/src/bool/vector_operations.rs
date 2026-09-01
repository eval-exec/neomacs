//! Oracle parity tests for bool-vector operations: `make-bool-vector`,
//! `bool-vector-p`, `aref`/`aset`, `length`, bitset patterns, sieve of
//! Eratosthenes, and bit manipulation algorithms.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// make-bool-vector and bool-vector-p predicate
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_bool_vector_make_and_predicate() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let* ((bv-true  (make-bool-vector 10 t))
       (bv-false (make-bool-vector 10 nil))
       (bv-empty (make-bool-vector 0 t)))
  (list
    (bool-vector-p bv-true)
    (bool-vector-p bv-false)
    (bool-vector-p bv-empty)
    (bool-vector-p [1 2 3])
    (bool-vector-p "hello")
    (bool-vector-p nil)
    (bool-vector-p 42)
    (bool-vector-p '(a b))
    (length bv-true)
    (length bv-false)
    (length bv-empty)))"#;
    let expect = expect_test::expect![r#""OK (t t t nil nil nil nil nil 10 10 0)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// aref and aset on bool-vectors
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_bool_vector_aref_aset() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((bv (make-bool-vector 16 nil)))
  ;; Set specific bits
  (aset bv 0 t)
  (aset bv 3 t)
  (aset bv 7 t)
  (aset bv 15 t)
  ;; Read all bits back
  (let ((result nil)
        (i 0))
    (while (< i 16)
      (setq result (cons (aref bv i) result))
      (setq i (1+ i)))
    ;; Also test clearing a previously-set bit
    (aset bv 3 nil)
    (let ((after-clear (list (aref bv 2) (aref bv 3) (aref bv 4))))
      (list (nreverse result) after-clear))))"#;
    let expect = expect_test::expect![
        r#""OK ((t nil nil t nil nil nil t nil nil nil nil nil nil nil t) (nil nil nil))""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Bool-vector as bitset: set/clear/test bits, count set bits
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_bool_vector_bitset_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--test-bv-set-bit
    (lambda (bv idx) (aset bv idx t)))

  (fset 'neovm--test-bv-clear-bit
    (lambda (bv idx) (aset bv idx nil)))

  (fset 'neovm--test-bv-test-bit
    (lambda (bv idx) (aref bv idx)))

  (fset 'neovm--test-bv-popcount
    (lambda (bv)
      (let ((count 0) (i 0) (len (length bv)))
        (while (< i len)
          (when (aref bv i) (setq count (1+ count)))
          (setq i (1+ i)))
        count)))

  (unwind-protect
      (let ((bv (make-bool-vector 32 nil)))
        ;; Set bits at positions 0, 5, 10, 15, 20, 25, 31
        (dolist (pos '(0 5 10 15 20 25 31))
          (funcall 'neovm--test-bv-set-bit bv pos))
        (let* ((pop1 (funcall 'neovm--test-bv-popcount bv))
               ;; Clear some bits
               (_ (funcall 'neovm--test-bv-clear-bit bv 10))
               (_ (funcall 'neovm--test-bv-clear-bit bv 20))
               (pop2 (funcall 'neovm--test-bv-popcount bv))
               ;; Test specific bits
               (tests (list
                        (funcall 'neovm--test-bv-test-bit bv 0)
                        (funcall 'neovm--test-bv-test-bit bv 1)
                        (funcall 'neovm--test-bv-test-bit bv 5)
                        (funcall 'neovm--test-bv-test-bit bv 10)
                        (funcall 'neovm--test-bv-test-bit bv 15)
                        (funcall 'neovm--test-bv-test-bit bv 31))))
          (list pop1 pop2 tests)))
    (fmakunbound 'neovm--test-bv-set-bit)
    (fmakunbound 'neovm--test-bv-clear-bit)
    (fmakunbound 'neovm--test-bv-test-bit)
    (fmakunbound 'neovm--test-bv-popcount)))"#;
    let expect = expect_test::expect![r#""OK (7 5 (t nil t nil t t))""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Length on bool-vectors of various sizes including edge cases
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_bool_vector_length_various() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  (length (make-bool-vector 0 nil))
  (length (make-bool-vector 1 t))
  (length (make-bool-vector 7 nil))
  (length (make-bool-vector 8 t))
  (length (make-bool-vector 9 nil))
  (length (make-bool-vector 31 t))
  (length (make-bool-vector 32 nil))
  (length (make-bool-vector 33 t))
  (length (make-bool-vector 64 nil))
  (length (make-bool-vector 100 t))
  (length (make-bool-vector 255 nil)))"#;
    let expect = expect_test::expect![r#""OK (0 1 7 8 9 31 32 33 64 100 255)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Bool-vector init values: all true then read, all false then read
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_bool_vector_init_values() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((bv-t (make-bool-vector 20 t))
       (bv-nil (make-bool-vector 20 nil)))
  (let ((all-t t) (all-nil t) (i 0))
    (while (< i 20)
      (unless (aref bv-t i) (setq all-t nil))
      (when (aref bv-nil i) (setq all-nil nil))
      (setq i (1+ i)))
    ;; Flip every other bit in bv-t
    (setq i 0)
    (while (< i 20)
      (when (= (% i 2) 0) (aset bv-t i nil))
      (setq i (1+ i)))
    ;; Read back pattern: odd indices should be t, even nil
    (let ((pattern nil))
      (setq i 0)
      (while (< i 10)
        (setq pattern (cons (list (aref bv-t (* i 2))
                                  (aref bv-t (1+ (* i 2))))
                            pattern))
        (setq i (1+ i)))
      (list all-t all-nil (nreverse pattern)))))"#;
    let expect = expect_test::expect![
        r#""OK (t t ((nil t) (nil t) (nil t) (nil t) (nil t) (nil t) (nil t) (nil t) (nil t) (nil t)))""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: sieve of Eratosthenes using bool-vector
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_bool_vector_sieve_of_eratosthenes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--test-bv-sieve
    (lambda (limit)
      "Return list of primes up to LIMIT using bool-vector sieve."
      (let ((is-prime (make-bool-vector (1+ limit) t)))
        ;; 0 and 1 are not prime
        (aset is-prime 0 nil)
        (when (> limit 0) (aset is-prime 1 nil))
        ;; Sieve: mark composites
        (let ((p 2))
          (while (<= (* p p) limit)
            (when (aref is-prime p)
              ;; Mark multiples of p starting from p*p
              (let ((m (* p p)))
                (while (<= m limit)
                  (aset is-prime m nil)
                  (setq m (+ m p)))))
            (setq p (1+ p))))
        ;; Collect primes
        (let ((primes nil) (i limit))
          (while (>= i 2)
            (when (aref is-prime i)
              (setq primes (cons i primes)))
            (setq i (1- i)))
          primes))))

  (unwind-protect
      (let* ((primes-50  (funcall 'neovm--test-bv-sieve 50))
             (primes-2   (funcall 'neovm--test-bv-sieve 2))
             (primes-1   (funcall 'neovm--test-bv-sieve 1))
             (primes-100 (funcall 'neovm--test-bv-sieve 100))
             ;; Count primes up to 100
             (count-100  (length primes-100))
             ;; Verify specific primes
             (has-2   (memq 2 primes-50))
             (has-47  (memq 47 primes-50))
             (no-49   (not (memq 49 primes-50)))
             (no-50   (not (memq 50 primes-50))))
        (list primes-50 primes-2 primes-1 count-100
              (and has-2 t) (and has-47 t) no-49 no-50))
    (fmakunbound 'neovm--test-bv-sieve)))"#;
    let expect = expect_test::expect![
        r#""OK ((2 3 5 7 11 13 17 19 23 29 31 37 41 43 47) (2) nil 25 t t t t)""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: bit manipulation — gray code, bit reversal, hamming distance
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_bool_vector_bit_manipulation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Convert integer to bool-vector of given width (LSB first)
  (fset 'neovm--test-bv-from-int
    (lambda (n width)
      (let ((bv (make-bool-vector width nil))
            (i 0)
            (val n))
        (while (< i width)
          (when (= (% val 2) 1) (aset bv i t))
          (setq val (/ val 2))
          (setq i (1+ i)))
        bv)))

  ;; Convert bool-vector back to integer (LSB first)
  (fset 'neovm--test-bv-to-int
    (lambda (bv)
      (let ((result 0) (i (1- (length bv))))
        (while (>= i 0)
          (setq result (+ (* result 2) (if (aref bv i) 1 0)))
          (setq i (1- i)))
        result)))

  ;; Hamming distance: count differing bits between two bool-vectors
  (fset 'neovm--test-bv-hamming
    (lambda (bv1 bv2)
      (let ((dist 0) (i 0) (len (length bv1)))
        (while (< i len)
          (unless (eq (aref bv1 i) (aref bv2 i))
            (setq dist (1+ dist)))
          (setq i (1+ i)))
        dist)))

  ;; Reverse bits in a bool-vector (return new)
  (fset 'neovm--test-bv-reverse
    (lambda (bv)
      (let* ((len (length bv))
             (result (make-bool-vector len nil))
             (i 0))
        (while (< i len)
          (aset result (- len 1 i) (aref bv i))
          (setq i (1+ i)))
        result)))

  ;; Gray code: n XOR (n >> 1), implemented with bool-vectors
  (fset 'neovm--test-bv-gray-encode
    (lambda (n width)
      (let ((bv-n (funcall 'neovm--test-bv-from-int n width))
            (bv-shifted (funcall 'neovm--test-bv-from-int (/ n 2) width))
            (result (make-bool-vector width nil))
            (i 0))
        (while (< i width)
          ;; XOR: different bits produce t
          (aset result i (not (eq (aref bv-n i) (aref bv-shifted i))))
          (setq i (1+ i)))
        result)))

  (unwind-protect
      (let* (;; Round-trip: int -> bv -> int
             (rt1 (funcall 'neovm--test-bv-to-int
                           (funcall 'neovm--test-bv-from-int 42 8)))
             (rt2 (funcall 'neovm--test-bv-to-int
                           (funcall 'neovm--test-bv-from-int 255 8)))
             (rt3 (funcall 'neovm--test-bv-to-int
                           (funcall 'neovm--test-bv-from-int 0 8)))
             ;; Hamming distances
             (h1 (funcall 'neovm--test-bv-hamming
                          (funcall 'neovm--test-bv-from-int 7 8)
                          (funcall 'neovm--test-bv-from-int 0 8)))
             (h2 (funcall 'neovm--test-bv-hamming
                          (funcall 'neovm--test-bv-from-int 255 8)
                          (funcall 'neovm--test-bv-from-int 0 8)))
             (h3 (funcall 'neovm--test-bv-hamming
                          (funcall 'neovm--test-bv-from-int 170 8)
                          (funcall 'neovm--test-bv-from-int 85 8)))
             ;; Bit reversal
             (rev1 (funcall 'neovm--test-bv-to-int
                            (funcall 'neovm--test-bv-reverse
                                     (funcall 'neovm--test-bv-from-int 1 8))))
             (rev2 (funcall 'neovm--test-bv-to-int
                            (funcall 'neovm--test-bv-reverse
                                     (funcall 'neovm--test-bv-from-int 6 8))))
             ;; Gray codes for 0..7
             (grays (let ((g nil) (n 0))
                      (while (< n 8)
                        (setq g (cons (funcall 'neovm--test-bv-to-int
                                               (funcall 'neovm--test-bv-gray-encode n 4))
                                      g))
                        (setq n (1+ n)))
                      (nreverse g)))
             ;; Adjacent gray codes should differ by exactly 1 bit
             (gray-diffs (let ((d nil) (i 0))
                           (while (< i 7)
                             (setq d (cons
                                      (funcall 'neovm--test-bv-hamming
                                               (funcall 'neovm--test-bv-from-int
                                                        (nth i grays) 4)
                                               (funcall 'neovm--test-bv-from-int
                                                        (nth (1+ i) grays) 4))
                                      d))
                             (setq i (1+ i)))
                           (nreverse d))))
        (list rt1 rt2 rt3 h1 h2 h3 rev1 rev2 grays gray-diffs))
    (fmakunbound 'neovm--test-bv-from-int)
    (fmakunbound 'neovm--test-bv-to-int)
    (fmakunbound 'neovm--test-bv-hamming)
    (fmakunbound 'neovm--test-bv-reverse)
    (fmakunbound 'neovm--test-bv-gray-encode)))"#;
    let expect =
        expect_test::expect![r#""OK (42 255 0 3 8 8 128 96 (0 1 3 2 6 7 5 4) (1 1 1 1 1 1 1))""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}
