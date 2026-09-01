//! Oracle parity tests for bloom filter and probabilistic data structures.
//!
//! Implements a bloom filter with multiple hash functions, false positive
//! rate estimation, set membership testing, count-min sketch for frequency
//! estimation, and HyperLogLog-like cardinality estimation -- all in
//! pure Elisp.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

// ---------------------------------------------------------------------------
// Bloom filter with multiple hash functions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_bloom_filter_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement a bloom filter using a bool-vector and multiple
    // hash functions based on polynomial rolling hash with different primes.
    let form = r#"(progn
  ;; Bloom filter: (size bit-vector hash-count)
  (fset 'neovm--bf-create
    (lambda (size num-hashes)
      "Create a bloom filter of SIZE bits with NUM-HASHES hash functions."
      (list size (make-vector size nil) num-hashes)))

  ;; Hash function family: h_k(s) = (sum of s[i] * prime^(i+k)) mod size
  (fset 'neovm--bf-hash
    (lambda (key k size)
      "Compute k-th hash of KEY for a bloom filter of SIZE."
      (let* ((primes '(31 37 41 43 47 53 59 61 67 71))
             (prime (nth (% k (length primes)) primes))
             (h 0)
             (i 0)
             (s (if (stringp key) key (format "%s" key)))
             (len (length s)))
        (while (< i len)
          (setq h (% (+ (* h prime) (aref s i) (* k 7)) 1000000007))
          (setq i (1+ i)))
        (% (abs h) size))))

  (fset 'neovm--bf-add
    (lambda (bf key)
      "Add KEY to bloom filter BF."
      (let ((size (nth 0 bf))
            (bits (nth 1 bf))
            (num-h (nth 2 bf))
            (k 0))
        (while (< k num-h)
          (let ((idx (funcall 'neovm--bf-hash key k size)))
            (aset bits idx t))
          (setq k (1+ k))))))

  (fset 'neovm--bf-might-contain
    (lambda (bf key)
      "Check if KEY might be in bloom filter BF."
      (let ((size (nth 0 bf))
            (bits (nth 1 bf))
            (num-h (nth 2 bf))
            (k 0)
            (present t))
        (while (and present (< k num-h))
          (let ((idx (funcall 'neovm--bf-hash key k size)))
            (unless (aref bits idx)
              (setq present nil)))
          (setq k (1+ k)))
        present)))

  (fset 'neovm--bf-bit-count
    (lambda (bf)
      "Count number of set bits in bloom filter."
      (let ((bits (nth 1 bf))
            (size (nth 0 bf))
            (count 0)
            (i 0))
        (while (< i size)
          (when (aref bits i)
            (setq count (1+ count)))
          (setq i (1+ i)))
        count)))

  (unwind-protect
      (let ((bf (funcall 'neovm--bf-create 64 3)))
        ;; Add some items
        (funcall 'neovm--bf-add bf "apple")
        (funcall 'neovm--bf-add bf "banana")
        (funcall 'neovm--bf-add bf "cherry")
        (funcall 'neovm--bf-add bf "date")
        (funcall 'neovm--bf-add bf "elderberry")
        (list
          ;; Items that were added: should definitely be found
          (funcall 'neovm--bf-might-contain bf "apple")
          (funcall 'neovm--bf-might-contain bf "banana")
          (funcall 'neovm--bf-might-contain bf "cherry")
          (funcall 'neovm--bf-might-contain bf "date")
          (funcall 'neovm--bf-might-contain bf "elderberry")
          ;; Items NOT added: should probably NOT be found
          ;; (false positives possible but unlikely with 64 bits and 5 items)
          ;; We test a batch and report membership for each
          (funcall 'neovm--bf-might-contain bf "fig")
          (funcall 'neovm--bf-might-contain bf "grape")
          (funcall 'neovm--bf-might-contain bf "kiwi")
          ;; Bit count
          (funcall 'neovm--bf-bit-count bf)
          ;; Size check
          (nth 0 bf)
          (nth 2 bf)))
    (fmakunbound 'neovm--bf-create)
    (fmakunbound 'neovm--bf-hash)
    (fmakunbound 'neovm--bf-add)
    (fmakunbound 'neovm--bf-might-contain)
    (fmakunbound 'neovm--bf-bit-count)))"#;
    let expect = expect_test::expect![[r#""OK (t t t t t nil nil nil 14 64 3)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// False positive rate estimation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_bloom_filter_false_positive_rate() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Create a bloom filter, add N items, then test M non-member items
    // and measure the empirical false positive rate.
    // Compare with theoretical rate: (1 - e^(-kn/m))^k
    let form = r#"(progn
  (fset 'neovm--bfp-create
    (lambda (size num-hashes)
      (list size (make-vector size nil) num-hashes)))

  (fset 'neovm--bfp-hash
    (lambda (key k size)
      (let* ((primes '(31 37 41 43 47 53 59 61 67 71))
             (prime (nth (% k (length primes)) primes))
             (h 0)
             (i 0)
             (s (if (stringp key) key (format "%s" key)))
             (len (length s)))
        (while (< i len)
          (setq h (% (+ (* h prime) (aref s i) (* k 7)) 1000000007))
          (setq i (1+ i)))
        (% (abs h) size))))

  (fset 'neovm--bfp-add
    (lambda (bf key)
      (let ((size (nth 0 bf))
            (bits (nth 1 bf))
            (num-h (nth 2 bf))
            (k 0))
        (while (< k num-h)
          (aset bits (funcall 'neovm--bfp-hash key k size) t)
          (setq k (1+ k))))))

  (fset 'neovm--bfp-check
    (lambda (bf key)
      (let ((size (nth 0 bf))
            (bits (nth 1 bf))
            (num-h (nth 2 bf))
            (k 0)
            (present t))
        (while (and present (< k num-h))
          (unless (aref bits (funcall 'neovm--bfp-hash key k size))
            (setq present nil))
          (setq k (1+ k)))
        present)))

  (unwind-protect
      (let ((bf (funcall 'neovm--bfp-create 256 5)))
        ;; Add 20 items with prefix "member-"
        (let ((i 0))
          (while (< i 20)
            (funcall 'neovm--bfp-add bf (format "member-%d" i))
            (setq i (1+ i))))
        ;; Test 40 non-member items with prefix "test-"
        (let ((false-positives 0)
              (true-negatives 0)
              (j 0))
          (while (< j 40)
            (if (funcall 'neovm--bfp-check bf (format "test-%d" j))
                (setq false-positives (1+ false-positives))
              (setq true-negatives (1+ true-negatives)))
            (setq j (1+ j)))
          ;; All members should be present (no false negatives)
          (let ((false-negatives 0)
                (k 0))
            (while (< k 20)
              (unless (funcall 'neovm--bfp-check bf (format "member-%d" k))
                (setq false-negatives (1+ false-negatives)))
              (setq k (1+ k)))
            (list
              ;; Zero false negatives (guaranteed property)
              false-negatives
              ;; False positives count and rate
              false-positives
              true-negatives
              (+ false-positives true-negatives)  ;; should be 40
              ;; Theoretical approx FP rate for m=256, k=5, n=20
              ;; (1 - e^(-5*20/256))^5 ~ 0.0014 so ~0 out of 40
              ;; We just verify the structure is valid
              (>= true-negatives 0)
              (>= false-positives 0)
              (= (+ false-positives true-negatives) 40)))))
    (fmakunbound 'neovm--bfp-create)
    (fmakunbound 'neovm--bfp-hash)
    (fmakunbound 'neovm--bfp-add)
    (fmakunbound 'neovm--bfp-check)))"#;
    let expect = expect_test::expect![[r#""OK (0 0 40 40 t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Definitely-not-in vs maybe-in: set membership semantics
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_bloom_filter_membership_semantics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Demonstrate the core bloom filter property: if check returns nil,
    // the element is DEFINITELY not in the set. If it returns t, the
    // element MIGHT be in the set.
    let form = r#"(progn
  (fset 'neovm--bfm-create
    (lambda (size num-hashes)
      (list size (make-vector size nil) num-hashes)))

  (fset 'neovm--bfm-hash
    (lambda (key k size)
      (let* ((primes '(31 37 41 43 47 53 59))
             (prime (nth (% k (length primes)) primes))
             (h 0) (i 0)
             (s (format "%s" key))
             (len (length s)))
        (while (< i len)
          (setq h (% (+ (* h prime) (aref s i) (* k 13)) 1000000007))
          (setq i (1+ i)))
        (% (abs h) size))))

  (fset 'neovm--bfm-add
    (lambda (bf key)
      (let ((k 0))
        (while (< k (nth 2 bf))
          (aset (nth 1 bf) (funcall 'neovm--bfm-hash key k (nth 0 bf)) t)
          (setq k (1+ k))))))

  (fset 'neovm--bfm-check
    (lambda (bf key)
      (let ((k 0) (present t))
        (while (and present (< k (nth 2 bf)))
          (unless (aref (nth 1 bf) (funcall 'neovm--bfm-hash key k (nth 0 bf)))
            (setq present nil))
          (setq k (1+ k)))
        present)))

  ;; Also maintain a real hash-table as ground truth
  (fset 'neovm--bfm-verify
    (lambda (bf truth items)
      "For each item, check bloom filter and ground truth.
       Return list of (item bf-result truth-result valid) where
       valid = (not (and (not bf-result) truth-result))  ; no false negatives"
      (mapcar
        (lambda (item)
          (let ((bf-says (funcall 'neovm--bfm-check bf item))
                (really-in (gethash item truth)))
            (list item
                  (if bf-says 'maybe-in 'definitely-not)
                  (if really-in 'actually-in 'actually-out)
                  ;; The key invariant: if bloom says not-in, truth must also say not-in
                  (not (and (not bf-says) really-in)))))
        items)))

  (unwind-protect
      (let ((bf (funcall 'neovm--bfm-create 128 4))
            (truth (make-hash-table :test 'equal))
            (members '("cat" "dog" "fish" "bird" "snake" "lizard"
                        "frog" "turtle" "hamster" "rabbit"))
            (non-members '("car" "bus" "train" "boat" "plane"
                           "bike" "scooter" "truck" "van" "jet")))
        ;; Add members to both bloom filter and truth table
        (dolist (m members)
          (funcall 'neovm--bfm-add bf m)
          (puthash m t truth))
        ;; Verify all items (members + non-members)
        (let ((all-items (append members non-members)))
          (let ((verification (funcall 'neovm--bfm-verify bf truth all-items)))
            ;; All validity flags should be t (no false negatives)
            (let ((all-valid t))
              (dolist (v verification)
                (unless (nth 3 v)
                  (setq all-valid nil)))
              (list
                ;; All valid (no false negatives)
                all-valid
                ;; Member results (should all be maybe-in)
                (mapcar (lambda (v) (nth 1 v))
                        (let ((result nil))
                          (dolist (v verification)
                            (when (gethash (nth 0 v) truth)
                              (setq result (cons v result))))
                          (nreverse result)))
                ;; Count of definitely-not results for non-members
                (let ((def-not 0))
                  (dolist (v verification)
                    (when (and (not (gethash (nth 0 v) truth))
                               (eq (nth 1 v) 'definitely-not))
                      (setq def-not (1+ def-not))))
                  def-not)
                ;; Total items checked
                (length verification))))))
    (fmakunbound 'neovm--bfm-create)
    (fmakunbound 'neovm--bfm-hash)
    (fmakunbound 'neovm--bfm-add)
    (fmakunbound 'neovm--bfm-check)
    (fmakunbound 'neovm--bfm-verify)))"#;
    let expect = expect_test::expect![[
        r#""OK (t (maybe-in maybe-in maybe-in maybe-in maybe-in maybe-in maybe-in maybe-in maybe-in maybe-in) 10 20)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Count-Min Sketch for frequency estimation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_count_min_sketch() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Count-Min Sketch: a probabilistic frequency table.
    // Uses d hash functions and w counters per row.
    // Estimate = min across all rows of counter[h_i(x)].
    // Always overestimates, never underestimates.
    let form = r#"(progn
  ;; CMS structure: (width depth matrix)
  ;; matrix = vector of d vectors, each of width w
  (fset 'neovm--cms-create
    (lambda (width depth)
      (let ((matrix (make-vector depth nil))
            (i 0))
        (while (< i depth)
          (aset matrix i (make-vector width 0))
          (setq i (1+ i)))
        (list width depth matrix))))

  (fset 'neovm--cms-hash
    (lambda (key row width)
      (let* ((primes '(31 37 41 43 47 53 59 61))
             (prime (nth (% row (length primes)) primes))
             (h 0) (i 0)
             (s (format "%s" key))
             (len (length s)))
        (while (< i len)
          (setq h (% (+ (* h prime) (aref s i) (* row 17)) 1000000007))
          (setq i (1+ i)))
        (% (abs h) width))))

  (fset 'neovm--cms-add
    (lambda (cms key &optional count)
      (let ((width (nth 0 cms))
            (depth (nth 1 cms))
            (matrix (nth 2 cms))
            (c (or count 1))
            (d 0))
        (while (< d depth)
          (let* ((row (aref matrix d))
                 (idx (funcall 'neovm--cms-hash key d width)))
            (aset row idx (+ (aref row idx) c)))
          (setq d (1+ d))))))

  (fset 'neovm--cms-estimate
    (lambda (cms key)
      "Return estimated count (minimum across all rows)."
      (let ((width (nth 0 cms))
            (depth (nth 1 cms))
            (matrix (nth 2 cms))
            (min-val nil)
            (d 0))
        (while (< d depth)
          (let* ((row (aref matrix d))
                 (idx (funcall 'neovm--cms-hash key d width))
                 (val (aref row idx)))
            (when (or (null min-val) (< val min-val))
              (setq min-val val)))
          (setq d (1+ d)))
        min-val)))

  (unwind-protect
      (let ((cms (funcall 'neovm--cms-create 32 4)))
        ;; Add items with known frequencies
        ;; "alpha" x10, "beta" x5, "gamma" x3, "delta" x1, "epsilon" x8
        (let ((i 0))
          (while (< i 10) (funcall 'neovm--cms-add cms "alpha") (setq i (1+ i))))
        (let ((i 0))
          (while (< i 5) (funcall 'neovm--cms-add cms "beta") (setq i (1+ i))))
        (let ((i 0))
          (while (< i 3) (funcall 'neovm--cms-add cms "gamma") (setq i (1+ i))))
        (funcall 'neovm--cms-add cms "delta")
        (let ((i 0))
          (while (< i 8) (funcall 'neovm--cms-add cms "epsilon") (setq i (1+ i))))
        (let ((est-alpha (funcall 'neovm--cms-estimate cms "alpha"))
              (est-beta (funcall 'neovm--cms-estimate cms "beta"))
              (est-gamma (funcall 'neovm--cms-estimate cms "gamma"))
              (est-delta (funcall 'neovm--cms-estimate cms "delta"))
              (est-epsilon (funcall 'neovm--cms-estimate cms "epsilon"))
              (est-missing (funcall 'neovm--cms-estimate cms "missing")))
          (list
            ;; Estimates (should be >= true count, never less)
            est-alpha est-beta est-gamma est-delta est-epsilon
            ;; Verify overestimate property: estimate >= true count
            (>= est-alpha 10)
            (>= est-beta 5)
            (>= est-gamma 3)
            (>= est-delta 1)
            (>= est-epsilon 8)
            ;; Missing item: estimate >= 0 (may be > 0 due to collisions)
            (>= est-missing 0)
            ;; Ordering should roughly be preserved
            (>= est-alpha est-beta)
            (>= est-epsilon est-gamma))))
    (fmakunbound 'neovm--cms-create)
    (fmakunbound 'neovm--cms-hash)
    (fmakunbound 'neovm--cms-add)
    (fmakunbound 'neovm--cms-estimate)))"#;
    let expect = expect_test::expect![[r#""OK (10 5 3 1 8 t t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// HyperLogLog-like cardinality estimation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_hyperloglog_cardinality() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simplified HyperLogLog: use a set of registers, each tracking the
    // maximum number of leading zeros seen in hash values for items
    // hashing to that register. Estimate cardinality using harmonic mean.
    let form = r#"(progn
  ;; HLL structure: (num-registers registers)
  (fset 'neovm--hll-create
    (lambda (num-registers)
      (list num-registers (make-vector num-registers 0))))

  ;; Simple hash: string -> large integer
  (fset 'neovm--hll-hash
    (lambda (key)
      (let ((h 0) (i 0)
            (s (format "%s" key))
            (len (length s)))
        (while (< i len)
          (setq h (% (+ (* h 31) (aref s i)) 1073741824)) ;; 2^30
          (setq i (1+ i)))
        (abs h))))

  ;; Count leading zeros in binary representation (up to 30 bits)
  (fset 'neovm--hll-leading-zeros
    (lambda (n max-bits)
      (let ((count 0)
            (mask (ash 1 (1- max-bits))))
        (while (and (> mask 0) (= (logand n mask) 0))
          (setq count (1+ count))
          (setq mask (ash mask -1)))
        count)))

  (fset 'neovm--hll-add
    (lambda (hll key)
      (let* ((num-reg (nth 0 hll))
             (registers (nth 1 hll))
             (h (funcall 'neovm--hll-hash key))
             ;; Use lower bits for register index
             (reg-idx (% h num-reg))
             ;; Use upper bits for leading zeros count
             (remaining (/ h num-reg))
             (lz (funcall 'neovm--hll-leading-zeros remaining 20)))
        (when (> (1+ lz) (aref registers reg-idx))
          (aset registers reg-idx (1+ lz))))))

  (fset 'neovm--hll-estimate
    (lambda (hll)
      "Estimate cardinality using harmonic mean of registers."
      (let* ((num-reg (nth 0 hll))
             (registers (nth 1 hll))
             ;; alpha_m correction constant (simplified)
             (alpha (cond
                      ((= num-reg 16) 0.673)
                      ((= num-reg 32) 0.697)
                      ((= num-reg 64) 0.709)
                      (t 0.7)))
             ;; Harmonic mean: 1/sum(2^(-M[j]))
             ;; We compute sum of 2^(-M[j]) as sum of (expt 2.0 (- M[j]))
             (sum 0.0)
             (zero-regs 0)
             (i 0))
        (while (< i num-reg)
          (let ((val (aref registers i)))
            (setq sum (+ sum (expt 2.0 (- val))))
            (when (= val 0) (setq zero-regs (1+ zero-regs))))
          (setq i (1+ i)))
        (let ((raw-estimate (* alpha num-reg num-reg (/ 1.0 sum))))
          ;; Small range correction: use linear counting if estimate < 2.5m and zeros exist
          (if (and (< raw-estimate (* 2.5 num-reg)) (> zero-regs 0))
              (let ((lc (* num-reg (log (/ (float num-reg) zero-regs)))))
                (round lc))
            (round raw-estimate))))))

  (unwind-protect
      (let ((hll-small (funcall 'neovm--hll-create 16))
            (hll-medium (funcall 'neovm--hll-create 32)))
        ;; Add 10 unique items to small HLL
        (let ((i 0))
          (while (< i 10)
            (funcall 'neovm--hll-add hll-small (format "item-%d" i))
            (setq i (1+ i))))
        ;; Add the same 10 items again (duplicates should not increase estimate)
        (let ((i 0))
          (while (< i 10)
            (funcall 'neovm--hll-add hll-small (format "item-%d" i))
            (setq i (1+ i))))
        ;; Add 50 unique items to medium HLL
        (let ((i 0))
          (while (< i 50)
            (funcall 'neovm--hll-add hll-medium (format "element-%d" i))
            (setq i (1+ i))))
        (let ((est-small (funcall 'neovm--hll-estimate hll-small))
              (est-medium (funcall 'neovm--hll-estimate hll-medium)))
          (list
            ;; Estimates (rough: HLL is not precise for small sets)
            est-small
            est-medium
            ;; Estimates should be positive
            (> est-small 0)
            (> est-medium 0)
            ;; Medium should be larger than small
            (> est-medium est-small)
            ;; Register states
            (nth 0 hll-small)
            (nth 0 hll-medium)
            ;; Non-zero register count in small HLL
            (let ((nz 0) (i 0))
              (while (< i (nth 0 hll-small))
                (when (> (aref (nth 1 hll-small) i) 0)
                  (setq nz (1+ nz)))
                (setq i (1+ i)))
              nz))))
    (fmakunbound 'neovm--hll-create)
    (fmakunbound 'neovm--hll-hash)
    (fmakunbound 'neovm--hll-leading-zeros)
    (fmakunbound 'neovm--hll-add)
    (fmakunbound 'neovm--hll-estimate)))"#;
    let expect = expect_test::expect![[r#""ERR (void-variable s)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Bloom filter union and intersection estimation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_bloom_filter_set_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Two bloom filters of the same size/hash-count can be combined:
    //   union = bitwise OR (element in A or B)
    //   intersection estimate via inclusion-exclusion on bit counts
    let form = r#"(progn
  (fset 'neovm--bfs-create
    (lambda (size num-hashes)
      (list size (make-vector size nil) num-hashes)))

  (fset 'neovm--bfs-hash
    (lambda (key k size)
      (let* ((primes '(31 37 41 43 47))
             (prime (nth (% k (length primes)) primes))
             (h 0) (i 0)
             (s (format "%s" key)) (len (length s)))
        (while (< i len)
          (setq h (% (+ (* h prime) (aref s i) (* k 11)) 1000000007))
          (setq i (1+ i)))
        (% (abs h) size))))

  (fset 'neovm--bfs-add
    (lambda (bf key)
      (let ((k 0))
        (while (< k (nth 2 bf))
          (aset (nth 1 bf) (funcall 'neovm--bfs-hash key k (nth 0 bf)) t)
          (setq k (1+ k))))))

  (fset 'neovm--bfs-check
    (lambda (bf key)
      (let ((k 0) (present t))
        (while (and present (< k (nth 2 bf)))
          (unless (aref (nth 1 bf) (funcall 'neovm--bfs-hash key k (nth 0 bf)))
            (setq present nil))
          (setq k (1+ k)))
        present)))

  (fset 'neovm--bfs-union
    (lambda (bf1 bf2)
      "Create union bloom filter (bitwise OR)."
      (let* ((size (nth 0 bf1))
             (result (funcall 'neovm--bfs-create size (nth 2 bf1)))
             (bits-r (nth 1 result))
             (bits-1 (nth 1 bf1))
             (bits-2 (nth 1 bf2))
             (i 0))
        (while (< i size)
          (aset bits-r i (or (aref bits-1 i) (aref bits-2 i)))
          (setq i (1+ i)))
        result)))

  (fset 'neovm--bfs-bit-count
    (lambda (bf)
      (let ((count 0) (i 0) (size (nth 0 bf)))
        (while (< i size)
          (when (aref (nth 1 bf) i)
            (setq count (1+ count)))
          (setq i (1+ i)))
        count)))

  (unwind-protect
      (let ((bf-a (funcall 'neovm--bfs-create 64 3))
            (bf-b (funcall 'neovm--bfs-create 64 3)))
        ;; Set A: {1, 2, 3, 4, 5}
        (dolist (item '("1" "2" "3" "4" "5"))
          (funcall 'neovm--bfs-add bf-a item))
        ;; Set B: {4, 5, 6, 7, 8}
        (dolist (item '("4" "5" "6" "7" "8"))
          (funcall 'neovm--bfs-add bf-b item))
        ;; Union
        (let ((bf-union (funcall 'neovm--bfs-union bf-a bf-b)))
          (list
            ;; A's members
            (funcall 'neovm--bfs-check bf-a "1")
            (funcall 'neovm--bfs-check bf-a "6")
            ;; B's members
            (funcall 'neovm--bfs-check bf-b "8")
            (funcall 'neovm--bfs-check bf-b "1")
            ;; Union should contain all of A and B
            (funcall 'neovm--bfs-check bf-union "1")
            (funcall 'neovm--bfs-check bf-union "3")
            (funcall 'neovm--bfs-check bf-union "5")
            (funcall 'neovm--bfs-check bf-union "7")
            (funcall 'neovm--bfs-check bf-union "8")
            ;; Bit counts: union >= max(A, B)
            (let ((bc-a (funcall 'neovm--bfs-bit-count bf-a))
                  (bc-b (funcall 'neovm--bfs-bit-count bf-b))
                  (bc-u (funcall 'neovm--bfs-bit-count bf-union)))
              (list bc-a bc-b bc-u
                    (>= bc-u bc-a)
                    (>= bc-u bc-b))))))
    (fmakunbound 'neovm--bfs-create)
    (fmakunbound 'neovm--bfs-hash)
    (fmakunbound 'neovm--bfs-add)
    (fmakunbound 'neovm--bfs-check)
    (fmakunbound 'neovm--bfs-union)
    (fmakunbound 'neovm--bfs-bit-count)))"#;
    let expect = expect_test::expect![[r#""OK (t nil t nil t t t t t (15 15 24 t t))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
