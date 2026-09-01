//! Complex oracle parity tests for bitset operations implemented in Elisp
//! using integers: set/clear/toggle bits, union (logior), intersection (logand),
//! difference, symmetric difference (logxor), popcount, subset testing,
//! power set generation, and bit manipulation puzzles.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Bitset fundamentals: set, clear, toggle, test individual bits
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_bitset_fundamental_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (fset 'neovm--bs-set
    (lambda (bitset bit)
      "Set bit BIT in BITSET."
      (logior bitset (ash 1 bit))))

  (fset 'neovm--bs-clear
    (lambda (bitset bit)
      "Clear bit BIT in BITSET."
      (logand bitset (lognot (ash 1 bit)))))

  (fset 'neovm--bs-toggle
    (lambda (bitset bit)
      "Toggle bit BIT in BITSET."
      (logxor bitset (ash 1 bit))))

  (fset 'neovm--bs-test
    (lambda (bitset bit)
      "Test if bit BIT is set in BITSET."
      (/= (logand bitset (ash 1 bit)) 0)))

  (fset 'neovm--bs-from-list
    (lambda (bits)
      "Create a bitset from a list of bit positions."
      (let ((bs 0))
        (dolist (b bits bs)
          (setq bs (funcall 'neovm--bs-set bs b))))))

  (fset 'neovm--bs-to-list
    (lambda (bitset max-bit)
      "Convert a bitset to a sorted list of set bit positions."
      (let ((result nil))
        (dotimes (i max-bit)
          (when (funcall 'neovm--bs-test bitset i)
            (setq result (cons i result))))
        (nreverse result))))

  (unwind-protect
      (let* ((empty 0)
             (s1 (funcall 'neovm--bs-from-list '(0 2 4 6 8)))
             (s2 (funcall 'neovm--bs-from-list '(1 3 5 7 9))))
        ;; Test individual bits
        (let ((tests nil))
          (dotimes (i 10)
            (setq tests (cons (list i
                                    (funcall 'neovm--bs-test s1 i)
                                    (funcall 'neovm--bs-test s2 i))
                              tests)))
          ;; Set and clear
          (let* ((s3 (funcall 'neovm--bs-set empty 5))
                 (s4 (funcall 'neovm--bs-set s3 10))
                 (s5 (funcall 'neovm--bs-clear s4 5)))
            ;; Toggle
            (let* ((s6 (funcall 'neovm--bs-toggle s1 0))   ;; clear bit 0
                   (s7 (funcall 'neovm--bs-toggle s1 1)))  ;; set bit 1
              (list (nreverse tests)
                    (funcall 'neovm--bs-to-list s1 10)
                    (funcall 'neovm--bs-to-list s2 10)
                    (funcall 'neovm--bs-to-list s4 12)
                    (funcall 'neovm--bs-to-list s5 12)
                    (funcall 'neovm--bs-to-list s6 10)
                    (funcall 'neovm--bs-to-list s7 10))))))
    (fmakunbound 'neovm--bs-set)
    (fmakunbound 'neovm--bs-clear)
    (fmakunbound 'neovm--bs-toggle)
    (fmakunbound 'neovm--bs-test)
    (fmakunbound 'neovm--bs-from-list)
    (fmakunbound 'neovm--bs-to-list)))"#;
    let expect = expect_test::expect![[
        r#""OK (((0 t nil) (1 nil t) (2 t nil) (3 nil t) (4 t nil) (5 nil t) (6 t nil) (7 nil t) (8 t nil) (9 nil t)) (0 2 4 6 8) (1 3 5 7 9) (5 10) (10) (2 4 6 8) (0 1 2 4 6 8))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Bitset union, intersection, difference, symmetric difference
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_bitset_set_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (fset 'neovm--bs-from-list
    (lambda (bits)
      (let ((bs 0))
        (dolist (b bits bs)
          (setq bs (logior bs (ash 1 b)))))))

  (fset 'neovm--bs-to-list
    (lambda (bitset max-bit)
      (let ((result nil))
        (dotimes (i max-bit)
          (when (/= (logand bitset (ash 1 i)) 0)
            (setq result (cons i result))))
        (nreverse result))))

  (fset 'neovm--bs-union
    (lambda (a b) (logior a b)))

  (fset 'neovm--bs-intersection
    (lambda (a b) (logand a b)))

  (fset 'neovm--bs-difference
    (lambda (a b)
      "A \\ B = A AND (NOT B)."
      (logand a (lognot b))))

  (fset 'neovm--bs-symmetric-diff
    (lambda (a b) (logxor a b)))

  (unwind-protect
      (let* ((a (funcall 'neovm--bs-from-list '(1 2 3 5 8 13)))
             (b (funcall 'neovm--bs-from-list '(2 3 5 7 11 13)))
             (c (funcall 'neovm--bs-from-list '(0 1 4 9 16)))
             (max-b 20)
             ;; A union B
             (ab-union (funcall 'neovm--bs-union a b))
             ;; A intersect B
             (ab-inter (funcall 'neovm--bs-intersection a b))
             ;; A - B
             (a-minus-b (funcall 'neovm--bs-difference a b))
             ;; B - A
             (b-minus-a (funcall 'neovm--bs-difference b a))
             ;; A xor B (symmetric difference)
             (ab-xor (funcall 'neovm--bs-symmetric-diff a b)))
        ;; Verify: (A-B) union (B-A) = A xor B
        (let ((verify-symdiff
               (= (funcall 'neovm--bs-union a-minus-b b-minus-a) ab-xor))
              ;; Verify: (A inter B) union (A xor B) = A union B
              (verify-partition
               (= (funcall 'neovm--bs-union ab-inter ab-xor) ab-union))
              ;; Triple operations
              (abc-union (funcall 'neovm--bs-union
                           (funcall 'neovm--bs-union a b) c))
              (abc-inter (funcall 'neovm--bs-intersection
                           (funcall 'neovm--bs-intersection a b) c)))
          (list (funcall 'neovm--bs-to-list ab-union max-b)
                (funcall 'neovm--bs-to-list ab-inter max-b)
                (funcall 'neovm--bs-to-list a-minus-b max-b)
                (funcall 'neovm--bs-to-list b-minus-a max-b)
                (funcall 'neovm--bs-to-list ab-xor max-b)
                verify-symdiff
                verify-partition
                (funcall 'neovm--bs-to-list abc-union max-b)
                (funcall 'neovm--bs-to-list abc-inter max-b))))
    (fmakunbound 'neovm--bs-from-list)
    (fmakunbound 'neovm--bs-to-list)
    (fmakunbound 'neovm--bs-union)
    (fmakunbound 'neovm--bs-intersection)
    (fmakunbound 'neovm--bs-difference)
    (fmakunbound 'neovm--bs-symmetric-diff)))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 2 3 5 7 8 11 13) (2 3 5 13) (1 8) (7 11) (1 7 8 11) t t (0 1 2 3 4 5 7 8 9 11 13 16) nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Popcount: count set bits
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_bitset_popcount() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (fset 'neovm--bs-popcount
    (lambda (n)
      "Count the number of set bits in non-negative integer N."
      (let ((count 0) (x n))
        (while (> x 0)
          (setq count (+ count (logand x 1)))
          (setq x (ash x -1)))
        count)))

  (fset 'neovm--bs-popcount-kernighan
    (lambda (n)
      "Count set bits using Kernighan's trick: x & (x-1) clears lowest set bit."
      (let ((count 0) (x n))
        (while (> x 0)
          (setq x (logand x (1- x)))
          (setq count (1+ count)))
        count)))

  (unwind-protect
      (let ((test-values '(0 1 2 3 7 8 15 16 31 32 63 64 127 128 255 256
                           1023 4095 65535))
            (results nil))
        ;; Verify both methods agree and collect results
        (dolist (v test-values)
          (let ((pc1 (funcall 'neovm--bs-popcount v))
                (pc2 (funcall 'neovm--bs-popcount-kernighan v)))
            (setq results (cons (list v pc1 (= pc1 pc2)) results))))
        ;; Additional: popcount of logior = popcount_a + popcount_b - popcount(a&b)
        (let* ((a 170) (b 204)  ;; 10101010 and 11001100 in binary
               (pc-a (funcall 'neovm--bs-popcount a))
               (pc-b (funcall 'neovm--bs-popcount b))
               (pc-and (funcall 'neovm--bs-popcount (logand a b)))
               (pc-or (funcall 'neovm--bs-popcount (logior a b)))
               (inclusion-exclusion (= pc-or (- (+ pc-a pc-b) pc-and))))
          (list (nreverse results) pc-a pc-b pc-and pc-or inclusion-exclusion)))
    (fmakunbound 'neovm--bs-popcount)
    (fmakunbound 'neovm--bs-popcount-kernighan)))"#;
    let expect = expect_test::expect![[
        r#""OK (((0 0 t) (1 1 t) (2 1 t) (3 2 t) (7 3 t) (8 1 t) (15 4 t) (16 1 t) (31 5 t) (32 1 t) (63 6 t) (64 1 t) (127 7 t) (128 1 t) (255 8 t) (256 1 t) (1023 10 t) (4095 12 t) (65535 16 t)) 4 4 2 6 t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Subset testing via bitwise operations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_bitset_subset_testing() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (fset 'neovm--bs-from-list
    (lambda (bits)
      (let ((bs 0))
        (dolist (b bits bs)
          (setq bs (logior bs (ash 1 b)))))))

  (fset 'neovm--bs-subset-p
    (lambda (sub super)
      "Test if SUB is a subset of SUPER: sub & super = sub."
      (= (logand sub super) sub)))

  (fset 'neovm--bs-proper-subset-p
    (lambda (sub super)
      "Test if SUB is a proper subset of SUPER."
      (and (= (logand sub super) sub)
           (/= sub super))))

  (fset 'neovm--bs-equal-p
    (lambda (a b) (= a b)))

  (fset 'neovm--bs-disjoint-p
    (lambda (a b)
      "Test if A and B are disjoint (no common elements)."
      (= (logand a b) 0)))

  (unwind-protect
      (let* ((empty 0)
             (a (funcall 'neovm--bs-from-list '(1 3 5)))
             (b (funcall 'neovm--bs-from-list '(1 2 3 4 5)))
             (c (funcall 'neovm--bs-from-list '(1 3 5)))
             (d (funcall 'neovm--bs-from-list '(6 7 8)))
             (universe (funcall 'neovm--bs-from-list '(0 1 2 3 4 5 6 7 8 9))))
        (list
          ;; Subset tests
          (funcall 'neovm--bs-subset-p a b)       ;; t: {1,3,5} <= {1,2,3,4,5}
          (funcall 'neovm--bs-subset-p b a)       ;; nil
          (funcall 'neovm--bs-subset-p a c)       ;; t: equal sets are subsets
          (funcall 'neovm--bs-subset-p empty a)   ;; t: empty is subset of everything
          (funcall 'neovm--bs-subset-p a universe) ;; t
          ;; Proper subset
          (funcall 'neovm--bs-proper-subset-p a b) ;; t
          (funcall 'neovm--bs-proper-subset-p a c) ;; nil: equal
          (funcall 'neovm--bs-proper-subset-p empty empty) ;; nil
          ;; Equality
          (funcall 'neovm--bs-equal-p a c)        ;; t
          (funcall 'neovm--bs-equal-p a b)        ;; nil
          ;; Disjoint
          (funcall 'neovm--bs-disjoint-p a d)     ;; t
          (funcall 'neovm--bs-disjoint-p a b)     ;; nil: share {1,3,5}
          (funcall 'neovm--bs-disjoint-p empty d)  ;; t
          ;; Complement relative to universe
          (let ((complement-a (logand (lognot a) universe)))
            (list (funcall 'neovm--bs-disjoint-p a complement-a)
                  (= (logior a complement-a) universe)))))
    (fmakunbound 'neovm--bs-from-list)
    (fmakunbound 'neovm--bs-subset-p)
    (fmakunbound 'neovm--bs-proper-subset-p)
    (fmakunbound 'neovm--bs-equal-p)
    (fmakunbound 'neovm--bs-disjoint-p)))"#;
    let expect = expect_test::expect![[r#""OK (t nil t t t t nil nil t nil t nil t (t t))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Power set generation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_bitset_power_set() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (fset 'neovm--bs-to-list
    (lambda (bitset max-bit)
      (let ((result nil))
        (dotimes (i max-bit)
          (when (/= (logand bitset (ash 1 i)) 0)
            (setq result (cons i result))))
        (nreverse result))))

  (fset 'neovm--bs-power-set
    (lambda (elements)
      "Generate all subsets of ELEMENTS (a list of bit positions).
       Returns list of lists."
      (let* ((n (length elements))
             (total (ash 1 n))
             (subsets nil)
             (i 0))
        (while (< i total)
          (let ((subset nil) (j 0))
            (while (< j n)
              (when (/= (logand i (ash 1 j)) 0)
                (setq subset (cons (nth j elements) subset)))
              (setq j (1+ j)))
            (setq subsets (cons (nreverse subset) subsets)))
          (setq i (1+ i)))
        (nreverse subsets))))

  (fset 'neovm--bs-popcount
    (lambda (n)
      (let ((count 0) (x n))
        (while (> x 0)
          (setq count (+ count (logand x 1)))
          (setq x (ash x -1)))
        count)))

  (unwind-protect
      (let* ((elements '(0 1 2 3))
             (powset (funcall 'neovm--bs-power-set elements))
             ;; |P(S)| = 2^|S|
             (size-ok (= (length powset) (ash 1 (length elements))))
             ;; Group by subset size (binomial coefficients)
             (by-size nil))
        (dotimes (k (1+ (length elements)))
          (let ((count 0))
            (dolist (s powset)
              (when (= (length s) k)
                (setq count (1+ count))))
            (setq by-size (cons (cons k count) by-size))))
        ;; Verify subset lattice: every pair has meet (intersection) and join (union)
        ;; Just count total subsets and check sizes
        (list size-ok
              (nreverse by-size)
              ;; First few subsets
              (nth 0 powset)
              (nth 1 powset)
              (nth (1- (length powset)) powset)
              ;; Sum of binomial coefficients = 2^n
              (let ((sum 0))
                (dolist (entry (nreverse by-size) sum)
                  (setq sum (+ sum (cdr entry)))))))
    (fmakunbound 'neovm--bs-to-list)
    (fmakunbound 'neovm--bs-power-set)
    (fmakunbound 'neovm--bs-popcount)))"#;
    let expect = expect_test::expect![[
        r#""OK (t ((0 . 1) (1 . 4) (2 . 6) (3 . 4) (4 . 1)) nil (0) (0 1 2 3) 1)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Bit manipulation puzzles
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_bitset_manipulation_puzzles() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  ;; Isolate lowest set bit: x & (-x)
  (fset 'neovm--bs-lowest-set-bit
    (lambda (x)
      (logand x (- x))))

  ;; Clear lowest set bit: x & (x-1)
  (fset 'neovm--bs-clear-lowest
    (lambda (x)
      (logand x (1- x))))

  ;; Check if power of 2: x & (x-1) = 0 and x > 0
  (fset 'neovm--bs-power-of-2-p
    (lambda (x)
      (and (> x 0) (= (logand x (1- x)) 0))))

  ;; Next power of 2 >= x
  (fset 'neovm--bs-next-power-of-2
    (lambda (x)
      (if (<= x 1) 1
        (let ((p 1))
          (while (< p x)
            (setq p (ash p 1)))
          p))))

  ;; Bit reversal of n-bit number
  (fset 'neovm--bs-reverse-bits
    (lambda (x n)
      "Reverse the lowest N bits of X."
      (let ((result 0) (i 0))
        (while (< i n)
          (when (/= (logand x (ash 1 i)) 0)
            (setq result (logior result (ash 1 (- n 1 i)))))
          (setq i (1+ i)))
        result)))

  ;; Gray code: n -> n ^ (n >> 1)
  (fset 'neovm--bs-to-gray
    (lambda (n)
      (logxor n (ash n -1))))

  ;; Gray code decode
  (fset 'neovm--bs-from-gray
    (lambda (gray)
      (let ((n gray) (mask (ash gray -1)))
        (while (> mask 0)
          (setq n (logxor n mask))
          (setq mask (ash mask -1)))
        n)))

  (unwind-protect
      (let ((results nil))
        ;; Lowest set bit tests
        (dolist (x '(0 1 2 6 12 40 128))
          (setq results (cons (list 'lowest x (funcall 'neovm--bs-lowest-set-bit x)) results)))
        ;; Clear lowest tests
        (dolist (x '(1 6 12 15 128))
          (setq results (cons (list 'clear-low x (funcall 'neovm--bs-clear-lowest x)) results)))
        ;; Power of 2 tests
        (let ((pow2-results nil))
          (dolist (x '(0 1 2 3 4 7 8 15 16 31 32 64 100 128 256))
            (setq pow2-results (cons (cons x (funcall 'neovm--bs-power-of-2-p x)) pow2-results)))
          ;; Next power of 2
          (let ((next-p2 nil))
            (dolist (x '(0 1 2 3 5 7 8 9 15 16 17 100))
              (setq next-p2 (cons (cons x (funcall 'neovm--bs-next-power-of-2 x)) next-p2)))
            ;; Bit reversal (8-bit)
            (let ((reversals nil))
              (dolist (x '(0 1 128 170 255 85))
                (setq reversals (cons (list x (funcall 'neovm--bs-reverse-bits x 8)) reversals)))
              ;; Gray code roundtrip
              (let ((gray-ok t))
                (dotimes (i 32)
                  (let ((gray (funcall 'neovm--bs-to-gray i)))
                    (unless (= (funcall 'neovm--bs-from-gray gray) i)
                      (setq gray-ok nil))))
                ;; Gray codes for 0..7
                (let ((gray-codes nil))
                  (dotimes (i 8)
                    (setq gray-codes (cons (funcall 'neovm--bs-to-gray i) gray-codes)))
                  (list (nreverse results)
                        (nreverse pow2-results)
                        (nreverse next-p2)
                        (nreverse reversals)
                        gray-ok
                        (nreverse gray-codes))))))))
    (fmakunbound 'neovm--bs-lowest-set-bit)
    (fmakunbound 'neovm--bs-clear-lowest)
    (fmakunbound 'neovm--bs-power-of-2-p)
    (fmakunbound 'neovm--bs-next-power-of-2)
    (fmakunbound 'neovm--bs-reverse-bits)
    (fmakunbound 'neovm--bs-to-gray)
    (fmakunbound 'neovm--bs-from-gray)))"#;
    let expect = expect_test::expect![[
        r#""OK (((lowest 0 0) (lowest 1 1) (lowest 2 2) (lowest 6 2) (lowest 12 4) (lowest 40 8) (lowest 128 128) (clear-low 1 0) (clear-low 6 4) (clear-low 12 8) (clear-low 15 14) (clear-low 128 0)) ((0) (1 . t) (2 . t) (3) (4 . t) (7) (8 . t) (15) (16 . t) (31) (32 . t) (64 . t) (100) (128 . t) (256 . t)) ((0 . 1) (1 . 1) (2 . 2) (3 . 4) (5 . 8) (7 . 8) (8 . 8) (9 . 16) (15 . 16) (16 . 16) (17 . 32) (100 . 128)) ((0 0) (1 128) (128 1) (170 85) (255 255) (85 170)) t (0 1 3 2 6 7 5 4))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
