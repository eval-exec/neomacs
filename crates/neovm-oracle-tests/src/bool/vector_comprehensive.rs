//! Comprehensive oracle parity tests for bool-vector operations:
//! `make-bool-vector`, `bool-vector`, `bool-vector-p`,
//! `bool-vector-count-population`, `bool-vector-count-consecutive`,
//! `bool-vector-subsetp`, `bool-vector-not`, `bool-vector-union`,
//! `bool-vector-intersection`, `bool-vector-set-difference`,
//! `bool-vector-exclusive-or`, `aref`/`aset`, `length`, bit manipulation.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// bool-vector constructor and bool-vector-p
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_bool_vector_comprehensive_constructor_and_predicate() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let* ((bv1 (bool-vector t nil t nil t))
       (bv2 (bool-vector))
       (bv3 (bool-vector nil nil nil))
       (bv4 (bool-vector t t t t t t t t)))
  (list
    ;; predicate checks
    (bool-vector-p bv1)
    (bool-vector-p bv2)
    (bool-vector-p (make-bool-vector 0 nil))
    (bool-vector-p [1 2 3])
    (bool-vector-p "str")
    (bool-vector-p nil)
    ;; lengths
    (length bv1)
    (length bv2)
    (length bv3)
    (length bv4)
    ;; read back bv1 elements
    (aref bv1 0) (aref bv1 1) (aref bv1 2) (aref bv1 3) (aref bv1 4)
    ;; read back bv4
    (aref bv4 0) (aref bv4 7)))"#;
    let expect = expect_test::expect![r#""OK (t t t nil nil nil 5 0 3 8 t nil t nil t t t)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// bool-vector-count-population
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_bool_vector_comprehensive_count_population() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; All nil
  (bool-vector-count-population (make-bool-vector 32 nil))
  ;; All t
  (bool-vector-count-population (make-bool-vector 32 t))
  ;; Empty
  (bool-vector-count-population (make-bool-vector 0 nil))
  ;; Manual pattern
  (bool-vector-count-population (bool-vector t nil t nil t nil t nil))
  ;; Single element
  (bool-vector-count-population (bool-vector t))
  (bool-vector-count-population (bool-vector nil))
  ;; Large with specific bits set
  (let ((bv (make-bool-vector 100 nil)))
    (aset bv 0 t) (aset bv 50 t) (aset bv 99 t)
    (bool-vector-count-population bv))
  ;; After modification
  (let ((bv (make-bool-vector 16 t)))
    (let ((before (bool-vector-count-population bv)))
      (aset bv 0 nil) (aset bv 5 nil) (aset bv 10 nil)
      (list before (bool-vector-count-population bv)))))"#;
    let expect = expect_test::expect![r#""OK (0 32 0 4 1 0 3 (16 13))""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// bool-vector-count-consecutive
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_bool_vector_comprehensive_count_consecutive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((bv (bool-vector t t t nil nil t t nil t)))
  (list
    ;; Count consecutive t from index 0
    (bool-vector-count-consecutive bv t 0)
    ;; Count consecutive nil from index 3
    (bool-vector-count-consecutive bv nil 3)
    ;; Count consecutive t from index 5
    (bool-vector-count-consecutive bv t 5)
    ;; Count consecutive t from index 7 (it's nil)
    (bool-vector-count-consecutive bv t 7)
    ;; Count consecutive nil from index 0 (it's t)
    (bool-vector-count-consecutive bv nil 0)
    ;; Count consecutive t from last index
    (bool-vector-count-consecutive bv t 8)
    ;; All-true vector
    (bool-vector-count-consecutive (make-bool-vector 20 t) t 0)
    ;; All-false vector
    (bool-vector-count-consecutive (make-bool-vector 20 nil) nil 0)
    ;; Edge: count from end boundary
    (bool-vector-count-consecutive (make-bool-vector 10 t) t 9)))"#;
    let expect = expect_test::expect![r#""OK (3 2 2 0 0 1 20 20 1)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// bool-vector-subsetp
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_bool_vector_comprehensive_subsetp() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Identical vectors: subset of each other
  (bool-vector-subsetp (bool-vector t nil t) (bool-vector t nil t))
  ;; Subset: a is subset of b (a has fewer bits set)
  (bool-vector-subsetp (bool-vector t nil nil) (bool-vector t nil t))
  ;; Not subset: a has a bit b doesn't
  (bool-vector-subsetp (bool-vector t nil t) (bool-vector t nil nil))
  ;; Empty is subset of anything
  (bool-vector-subsetp (make-bool-vector 5 nil) (make-bool-vector 5 nil))
  (bool-vector-subsetp (make-bool-vector 5 nil) (make-bool-vector 5 t))
  ;; Full is subset of full
  (bool-vector-subsetp (make-bool-vector 8 t) (make-bool-vector 8 t))
  ;; Full is not subset of empty
  (bool-vector-subsetp (make-bool-vector 8 t) (make-bool-vector 8 nil))
  ;; Complex pattern
  (let ((a (bool-vector t nil t nil nil nil t nil))
        (b (bool-vector t t t nil t nil t t)))
    (list (bool-vector-subsetp a b)
          (bool-vector-subsetp b a))))"#;
    let expect = expect_test::expect![r#""OK (t t nil t t t nil (t nil))""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// bool-vector-not
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_bool_vector_comprehensive_not() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let* ((bv (bool-vector t nil t nil t nil t nil))
       (neg (bool-vector-not bv)))
  (list
    ;; Length preserved
    (length neg)
    ;; Bits flipped
    (aref neg 0) (aref neg 1) (aref neg 2) (aref neg 3)
    (aref neg 4) (aref neg 5) (aref neg 6) (aref neg 7)
    ;; Double negation returns original values
    (let ((dbl (bool-vector-not neg)))
      (list (aref dbl 0) (aref dbl 1) (aref dbl 2) (aref dbl 3)))
    ;; Not of all-true
    (bool-vector-count-population (bool-vector-not (make-bool-vector 16 t)))
    ;; Not of all-false
    (bool-vector-count-population (bool-vector-not (make-bool-vector 16 nil)))
    ;; Not of empty
    (length (bool-vector-not (make-bool-vector 0 nil)))))"#;
    let expect = expect_test::expect![r#""OK (8 nil t nil t nil t nil t (t nil t nil) 0 16 0)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// bool-vector-union
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_bool_vector_comprehensive_union() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let* ((a (bool-vector t nil t nil nil nil))
       (b (bool-vector nil nil t nil t t))
       (u (bool-vector-union a b)))
  (list
    (length u)
    (aref u 0) (aref u 1) (aref u 2) (aref u 3) (aref u 4) (aref u 5)
    ;; Union with all-nil yields original
    (let ((r (bool-vector-union a (make-bool-vector 6 nil))))
      (list (aref r 0) (aref r 2)))
    ;; Union with all-t yields all-t
    (bool-vector-count-population
      (bool-vector-union a (make-bool-vector 6 t)))
    ;; Union of empty vectors
    (length (bool-vector-union (make-bool-vector 0 nil) (make-bool-vector 0 nil)))
    ;; Self-union
    (let ((self-u (bool-vector-union a a)))
      (list (aref self-u 0) (aref self-u 1) (aref self-u 2)))))"#;
    let expect = expect_test::expect![r#""OK (6 t nil t nil t t (t t) 6 0 (t nil t))""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// bool-vector-intersection
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_bool_vector_comprehensive_intersection() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let* ((a (bool-vector t t nil t nil t))
       (b (bool-vector t nil nil t t t))
       (inter (bool-vector-intersection a b)))
  (list
    (length inter)
    (aref inter 0) (aref inter 1) (aref inter 2)
    (aref inter 3) (aref inter 4) (aref inter 5)
    ;; Intersection with all-t yields original
    (let ((r (bool-vector-intersection a (make-bool-vector 6 t))))
      (list (aref r 0) (aref r 1) (aref r 2) (aref r 3) (aref r 4) (aref r 5)))
    ;; Intersection with all-nil yields all-nil
    (bool-vector-count-population
      (bool-vector-intersection a (make-bool-vector 6 nil)))
    ;; Self intersection = self
    (let ((self-i (bool-vector-intersection b b)))
      (bool-vector-count-population self-i))))"#;
    let expect = expect_test::expect![r#""OK (6 t nil nil t nil t (t t nil t nil t) 0 4)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// bool-vector-set-difference and bool-vector-exclusive-or
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_bool_vector_comprehensive_setdiff_xor() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let* ((a (bool-vector t t nil nil t t))
       (b (bool-vector t nil t nil t nil))
       (diff-ab (bool-vector-set-difference a b))
       (diff-ba (bool-vector-set-difference b a))
       (xor-ab (bool-vector-exclusive-or a b)))
  (list
    ;; set-difference: bits in a but not in b
    (aref diff-ab 0) (aref diff-ab 1) (aref diff-ab 2)
    (aref diff-ab 3) (aref diff-ab 4) (aref diff-ab 5)
    ;; set-difference: bits in b but not in a
    (aref diff-ba 0) (aref diff-ba 1) (aref diff-ba 2)
    (aref diff-ba 3) (aref diff-ba 4) (aref diff-ba 5)
    ;; exclusive-or: bits that differ
    (aref xor-ab 0) (aref xor-ab 1) (aref xor-ab 2)
    (aref xor-ab 3) (aref xor-ab 4) (aref xor-ab 5)
    ;; XOR with self yields all-nil
    (bool-vector-count-population (bool-vector-exclusive-or a a))
    ;; set-difference with self yields all-nil
    (bool-vector-count-population (bool-vector-set-difference a a))
    ;; XOR is symmetric: pop(a XOR b) == pop(b XOR a)
    (= (bool-vector-count-population (bool-vector-exclusive-or a b))
       (bool-vector-count-population (bool-vector-exclusive-or b a)))))"#;
    let expect = expect_test::expect![
        r#""OK (nil t nil nil nil t nil nil t nil nil nil nil t t nil nil t 0 0 t)""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: implementing a bitset-based set intersection counter
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_bool_vector_comprehensive_bitset_set_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Represent small integer sets (0-31) as bool-vectors
  (fset 'neovm--bvc-make-set
    (lambda (elements)
      "Create a bool-vector bitset from a list of integers 0-31."
      (let ((bv (make-bool-vector 32 nil)))
        (dolist (e elements)
          (aset bv e t))
        bv)))

  (fset 'neovm--bvc-to-list
    (lambda (bv)
      "Convert a bool-vector bitset back to a sorted list."
      (let ((result nil) (i 0))
        (while (< i (length bv))
          (when (aref bv i)
            (setq result (cons i result)))
          (setq i (1+ i)))
        (nreverse result))))

  (unwind-protect
      (let* ((evens (funcall 'neovm--bvc-make-set '(0 2 4 6 8 10 12 14 16 18 20)))
             (odds (funcall 'neovm--bvc-make-set '(1 3 5 7 9 11 13 15 17 19 21)))
             (primes (funcall 'neovm--bvc-make-set '(2 3 5 7 11 13 17 19 23 29)))
             (small (funcall 'neovm--bvc-make-set '(0 1 2 3 4 5 6 7 8 9)))
             ;; Even primes
             (even-primes (bool-vector-intersection evens primes))
             ;; Odd primes
             (odd-primes (bool-vector-intersection odds primes))
             ;; Small non-primes
             (small-non-primes (bool-vector-set-difference small primes))
             ;; Union of evens and odds should cover 0-21
             (all-low (bool-vector-union evens odds))
             ;; Evens XOR odds (no overlap)
             (xor-eo (bool-vector-exclusive-or evens odds)))
        (list
          (funcall 'neovm--bvc-to-list even-primes)
          (funcall 'neovm--bvc-to-list odd-primes)
          (funcall 'neovm--bvc-to-list small-non-primes)
          (bool-vector-count-population all-low)
          ;; Evens and odds have no overlap, so XOR == union
          (equal (funcall 'neovm--bvc-to-list xor-eo)
                 (funcall 'neovm--bvc-to-list all-low))
          ;; Subset checks
          (bool-vector-subsetp even-primes evens)
          (bool-vector-subsetp even-primes primes)
          (bool-vector-subsetp odd-primes evens)
          ;; De Morgan: NOT(A AND B) == NOT(A) OR NOT(B)
          (let* ((not-and (bool-vector-not (bool-vector-intersection evens primes)))
                 (or-nots (bool-vector-union (bool-vector-not evens) (bool-vector-not primes))))
            (equal (funcall 'neovm--bvc-to-list not-and)
                   (funcall 'neovm--bvc-to-list or-nots)))))
    (fmakunbound 'neovm--bvc-make-set)
    (fmakunbound 'neovm--bvc-to-list)))"#;
    let expect =
        expect_test::expect![r#""OK ((2) (3 5 7 11 13 17 19) (0 1 4 6 8 9) 22 t t t nil t)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Bool-vector destination argument (in-place mutation variants)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_bool_vector_comprehensive_in_place_destination() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Several bool-vector bitwise ops accept an optional destination arg
    let form = r#"(let* ((a (bool-vector t nil t nil t nil t nil))
       (b (bool-vector nil t nil t nil t nil t))
       (dest (make-bool-vector 8 nil)))
  ;; Union into dest
  (bool-vector-union a b dest)
  (let ((union-pop (bool-vector-count-population dest)))
    ;; Intersection into dest (reuse)
    (bool-vector-intersection a b dest)
    (let ((inter-pop (bool-vector-count-population dest)))
      ;; XOR into dest
      (bool-vector-exclusive-or a b dest)
      (let ((xor-pop (bool-vector-count-population dest)))
        ;; Not into dest
        (bool-vector-not a dest)
        (let ((not-pop (bool-vector-count-population dest)))
          (list union-pop inter-pop xor-pop not-pop
                ;; Verify dest contents after not
                (aref dest 0) (aref dest 1) (aref dest 2) (aref dest 3)))))))"#;
    let expect = expect_test::expect![r#""OK (8 0 8 4 nil t nil t)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}
