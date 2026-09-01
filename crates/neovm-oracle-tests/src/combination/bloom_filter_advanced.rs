//! Advanced oracle parity tests for a sophisticated Bloom filter in Elisp:
//! multiple hash functions, configurable false-positive rate, union/intersection
//! of two filters, spell-checker with Bloom filter pre-check, and set
//! membership testing with false-positive rate estimation.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Multiple hash functions using different combinations of char values
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_bloom_adv_multiple_hash_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test that different hash function indices produce different hash values
    // for the same key, providing better bit distribution.
    let form = r#"(progn
  ;; Hash family: h_k(key) uses prime[k], with different polynomial base and offset
  (fset 'neovm--bfa-hash
    (lambda (key k size)
      "Compute k-th hash of KEY string for filter of SIZE bits.
       Uses polynomial rolling hash with prime[k] as base and k-dependent offset."
      (let* ((primes '(31 37 41 43 47 53 59 61 67 71 73 79 83 89 97))
             (prime (nth (% k (length primes)) primes))
             (offset (* (1+ k) 13))
             (h offset)
             (i 0)
             (s (if (stringp key) key (format "%s" key)))
             (len (length s)))
        (while (< i len)
          ;; Mix: h = (h * prime + char + k * 7) mod large-prime
          (setq h (% (+ (* h prime) (aref s i) (* k 7)) 1000000007))
          (setq i (1+ i)))
        (% (abs h) size))))

  (unwind-protect
      ;; Verify that different k values produce different hashes for same key
      (let* ((size 256)
             (keys '("hello" "world" "bloom" "filter" "test"))
             (k-values '(0 1 2 3 4 5 6 7)))
        (mapcar
          (lambda (key)
            (let ((hashes (mapcar
                            (lambda (k) (funcall 'neovm--bfa-hash key k size))
                            k-values)))
              ;; Return key, its hash values, and count of unique hashes
              (list key
                    hashes
                    (let ((seen (make-hash-table :test 'eql))
                          (unique 0))
                      (dolist (h hashes)
                        (unless (gethash h seen)
                          (puthash h t seen)
                          (setq unique (1+ unique))))
                      unique))))
          keys))
    (fmakunbound 'neovm--bfa-hash)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"hello\" (229 95 245 39 228 38 203 254) 8) (\"world\" (165 43 121 143 4 82 140 83) 8) (\"bloom\" (22 60 234 48 181 99 116 235) 8) (\"filter\" (99 164 168 103 12 153 54 35) 8) (\"test\" (31 82 71 42 227 6 194 57) 8))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Configurable false-positive rate via number of hash functions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_bloom_adv_configurable_fp_rate() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // More hash functions generally means lower false-positive rate (up to a point).
    // Test filters with 1, 3, 5, 7 hash functions on the same data.
    let form = r#"(progn
  (fset 'neovm--bfr-hash
    (lambda (key k size)
      (let* ((primes '(31 37 41 43 47 53 59 61 67 71 73 79 83))
             (prime (nth (% k (length primes)) primes))
             (h (* (1+ k) 13))
             (i 0)
             (s (if (stringp key) key (format "%s" key)))
             (len (length s)))
        (while (< i len)
          (setq h (% (+ (* h prime) (aref s i) (* k 7)) 1000000007))
          (setq i (1+ i)))
        (% (abs h) size))))

  (fset 'neovm--bfr-create
    (lambda (size num-hashes)
      (list size (make-vector size nil) num-hashes)))

  (fset 'neovm--bfr-add
    (lambda (bf key)
      (let ((k 0))
        (while (< k (nth 2 bf))
          (aset (nth 1 bf) (funcall 'neovm--bfr-hash key k (nth 0 bf)) t)
          (setq k (1+ k))))))

  (fset 'neovm--bfr-check
    (lambda (bf key)
      (let ((k 0) (present t))
        (while (and present (< k (nth 2 bf)))
          (unless (aref (nth 1 bf) (funcall 'neovm--bfr-hash key k (nth 0 bf)))
            (setq present nil))
          (setq k (1+ k)))
        present)))

  (fset 'neovm--bfr-count-bits
    (lambda (bf)
      (let ((count 0) (i 0) (size (nth 0 bf)))
        (while (< i size)
          (when (aref (nth 1 bf) i) (setq count (1+ count)))
          (setq i (1+ i)))
        count)))

  (unwind-protect
      (let ((size 512)
            (members nil)
            (non-members nil))
        ;; Generate member and non-member keys
        (let ((i 0))
          (while (< i 30)
            (setq members (cons (format "member-%d" i) members))
            (setq non-members (cons (format "nonmember-%d" i) non-members))
            (setq i (1+ i))))
        ;; Test with different numbers of hash functions
        (mapcar
          (lambda (num-h)
            (let ((bf (funcall 'neovm--bfr-create size num-h)))
              ;; Add all members
              (dolist (m members)
                (funcall 'neovm--bfr-add bf m))
              ;; Count false positives among non-members
              (let ((fp 0) (tn 0))
                (dolist (nm non-members)
                  (if (funcall 'neovm--bfr-check bf nm)
                      (setq fp (1+ fp))
                    (setq tn (1+ tn))))
                ;; Verify no false negatives
                (let ((fn-count 0))
                  (dolist (m members)
                    (unless (funcall 'neovm--bfr-check bf m)
                      (setq fn-count (1+ fn-count))))
                  (list
                    num-h
                    (funcall 'neovm--bfr-count-bits bf)
                    fn-count    ;; should always be 0
                    fp
                    tn
                    (= fn-count 0)
                    (= (+ fp tn) 30))))))
          '(1 3 5 7)))
    (fmakunbound 'neovm--bfr-hash)
    (fmakunbound 'neovm--bfr-create)
    (fmakunbound 'neovm--bfr-add)
    (fmakunbound 'neovm--bfr-check)
    (fmakunbound 'neovm--bfr-count-bits)))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 30 0 1 29 t t) (3 90 0 0 30 t t) (5 148 0 0 30 t t) (7 175 0 0 30 t t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Union of two Bloom filters
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_bloom_adv_union() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Union of two Bloom filters = bitwise OR of their bit vectors.
    // The union filter should report "maybe" for all elements of both sets.
    let form = r#"(progn
  (fset 'neovm--bfu-hash
    (lambda (key k size)
      (let* ((primes '(31 37 41 43 47 53 59 61))
             (prime (nth (% k (length primes)) primes))
             (h (* (1+ k) 13))
             (i 0)
             (s (if (stringp key) key (format "%s" key)))
             (len (length s)))
        (while (< i len)
          (setq h (% (+ (* h prime) (aref s i) (* k 7)) 1000000007))
          (setq i (1+ i)))
        (% (abs h) size))))

  (fset 'neovm--bfu-create
    (lambda (size num-hashes)
      (list size (make-vector size nil) num-hashes)))

  (fset 'neovm--bfu-add
    (lambda (bf key)
      (let ((k 0))
        (while (< k (nth 2 bf))
          (aset (nth 1 bf) (funcall 'neovm--bfu-hash key k (nth 0 bf)) t)
          (setq k (1+ k))))))

  (fset 'neovm--bfu-check
    (lambda (bf key)
      (let ((k 0) (present t))
        (while (and present (< k (nth 2 bf)))
          (unless (aref (nth 1 bf) (funcall 'neovm--bfu-hash key k (nth 0 bf)))
            (setq present nil))
          (setq k (1+ k)))
        present)))

  (fset 'neovm--bfu-union
    (lambda (bf1 bf2)
      "Bitwise OR union of two bloom filters of same size and hash count."
      (let* ((size (nth 0 bf1))
             (num-h (nth 2 bf1))
             (result (funcall 'neovm--bfu-create size num-h))
             (bits-r (nth 1 result))
             (bits-1 (nth 1 bf1))
             (bits-2 (nth 1 bf2))
             (i 0))
        (while (< i size)
          (aset bits-r i (or (aref bits-1 i) (aref bits-2 i)))
          (setq i (1+ i)))
        result)))

  (fset 'neovm--bfu-bit-count
    (lambda (bf)
      (let ((count 0) (i 0) (size (nth 0 bf)))
        (while (< i size)
          (when (aref (nth 1 bf) i) (setq count (1+ count)))
          (setq i (1+ i)))
        count)))

  (unwind-protect
      (let ((bf-a (funcall 'neovm--bfu-create 128 4))
            (bf-b (funcall 'neovm--bfu-create 128 4)))
        ;; Set A: fruits
        (dolist (item '("apple" "banana" "cherry" "date" "elderberry"))
          (funcall 'neovm--bfu-add bf-a item))
        ;; Set B: vegetables
        (dolist (item '("asparagus" "broccoli" "carrot" "daikon" "eggplant"))
          (funcall 'neovm--bfu-add bf-b item))
        ;; Union
        (let ((bf-u (funcall 'neovm--bfu-union bf-a bf-b)))
          (list
            ;; All items from A should be in union
            (mapcar (lambda (x) (funcall 'neovm--bfu-check bf-u x))
                    '("apple" "banana" "cherry" "date" "elderberry"))
            ;; All items from B should be in union
            (mapcar (lambda (x) (funcall 'neovm--bfu-check bf-u x))
                    '("asparagus" "broccoli" "carrot" "daikon" "eggplant"))
            ;; A items NOT in B
            (mapcar (lambda (x) (funcall 'neovm--bfu-check bf-b x))
                    '("apple" "banana" "cherry"))
            ;; B items NOT in A
            (mapcar (lambda (x) (funcall 'neovm--bfu-check bf-a x))
                    '("asparagus" "broccoli" "carrot"))
            ;; Bit counts: union >= max(A, B)
            (let ((bc-a (funcall 'neovm--bfu-bit-count bf-a))
                  (bc-b (funcall 'neovm--bfu-bit-count bf-b))
                  (bc-u (funcall 'neovm--bfu-bit-count bf-u)))
              (list bc-a bc-b bc-u
                    (>= bc-u bc-a)
                    (>= bc-u bc-b)
                    ;; Union bits <= A + B bits (overlapping bits counted once)
                    (<= bc-u (+ bc-a bc-b)))))))
    (fmakunbound 'neovm--bfu-hash)
    (fmakunbound 'neovm--bfu-create)
    (fmakunbound 'neovm--bfu-add)
    (fmakunbound 'neovm--bfu-check)
    (fmakunbound 'neovm--bfu-union)
    (fmakunbound 'neovm--bfu-bit-count)))"#;
    let expect = expect_test::expect![[
        r#""OK ((t t t t t) (t t t t t) (nil nil nil) (nil nil nil) (18 19 34 t t t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Intersection approximation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_bloom_adv_intersection() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Bitwise AND of two Bloom filters approximates intersection:
    // if an element is in both A and B, it will be in the AND filter.
    // But false positives from both filters can also appear.
    let form = r#"(progn
  (fset 'neovm--bfi-hash
    (lambda (key k size)
      (let* ((primes '(31 37 41 43 47 53 59 61))
             (prime (nth (% k (length primes)) primes))
             (h (* (1+ k) 13))
             (i 0)
             (s (if (stringp key) key (format "%s" key)))
             (len (length s)))
        (while (< i len)
          (setq h (% (+ (* h prime) (aref s i) (* k 7)) 1000000007))
          (setq i (1+ i)))
        (% (abs h) size))))

  (fset 'neovm--bfi-create
    (lambda (size num-hashes)
      (list size (make-vector size nil) num-hashes)))

  (fset 'neovm--bfi-add
    (lambda (bf key)
      (let ((k 0))
        (while (< k (nth 2 bf))
          (aset (nth 1 bf) (funcall 'neovm--bfi-hash key k (nth 0 bf)) t)
          (setq k (1+ k))))))

  (fset 'neovm--bfi-check
    (lambda (bf key)
      (let ((k 0) (present t))
        (while (and present (< k (nth 2 bf)))
          (unless (aref (nth 1 bf) (funcall 'neovm--bfi-hash key k (nth 0 bf)))
            (setq present nil))
          (setq k (1+ k)))
        present)))

  (fset 'neovm--bfi-intersect
    (lambda (bf1 bf2)
      "Bitwise AND intersection of two bloom filters."
      (let* ((size (nth 0 bf1))
             (num-h (nth 2 bf1))
             (result (funcall 'neovm--bfi-create size num-h))
             (bits-r (nth 1 result))
             (bits-1 (nth 1 bf1))
             (bits-2 (nth 1 bf2))
             (i 0))
        (while (< i size)
          (aset bits-r i (and (aref bits-1 i) (aref bits-2 i)))
          (setq i (1+ i)))
        result)))

  (fset 'neovm--bfi-bit-count
    (lambda (bf)
      (let ((count 0) (i 0) (size (nth 0 bf)))
        (while (< i size)
          (when (aref (nth 1 bf) i) (setq count (1+ count)))
          (setq i (1+ i)))
        count)))

  (unwind-protect
      (let ((bf-a (funcall 'neovm--bfi-create 128 4))
            (bf-b (funcall 'neovm--bfi-create 128 4)))
        ;; Set A: {1,2,3,4,5,6,7}
        (dolist (item '("1" "2" "3" "4" "5" "6" "7"))
          (funcall 'neovm--bfi-add bf-a item))
        ;; Set B: {5,6,7,8,9,10,11}
        (dolist (item '("5" "6" "7" "8" "9" "10" "11"))
          (funcall 'neovm--bfi-add bf-b item))
        ;; Intersection
        (let ((bf-i (funcall 'neovm--bfi-intersect bf-a bf-b)))
          (list
            ;; Elements in both (5,6,7): should be in intersection
            (funcall 'neovm--bfi-check bf-i "5")
            (funcall 'neovm--bfi-check bf-i "6")
            (funcall 'neovm--bfi-check bf-i "7")
            ;; Elements only in A (1,2,3): likely NOT in intersection
            ;; (may have false positives but unlikely with 128 bits)
            (funcall 'neovm--bfi-check bf-i "1")
            (funcall 'neovm--bfi-check bf-i "2")
            ;; Elements only in B (8,9,10): likely NOT in intersection
            (funcall 'neovm--bfi-check bf-i "10")
            (funcall 'neovm--bfi-check bf-i "11")
            ;; Bit count: intersection <= min(A, B)
            (let ((bc-a (funcall 'neovm--bfi-bit-count bf-a))
                  (bc-b (funcall 'neovm--bfi-bit-count bf-b))
                  (bc-i (funcall 'neovm--bfi-bit-count bf-i)))
              (list bc-a bc-b bc-i
                    (<= bc-i bc-a)
                    (<= bc-i bc-b))))))
    (fmakunbound 'neovm--bfi-hash)
    (fmakunbound 'neovm--bfi-create)
    (fmakunbound 'neovm--bfi-add)
    (fmakunbound 'neovm--bfi-check)
    (fmakunbound 'neovm--bfi-intersect)
    (fmakunbound 'neovm--bfi-bit-count)))"#;
    let expect = expect_test::expect![[r#""OK (t t t nil nil nil nil (22 24 14 t t))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: spell-checker with Bloom filter pre-check
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_bloom_adv_spell_checker() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a "dictionary" Bloom filter, then use it as a fast pre-check
    // before doing an expensive exact lookup in a hash table.
    let form = r#"(progn
  (fset 'neovm--bfsc-hash
    (lambda (key k size)
      (let* ((primes '(31 37 41 43 47 53 59 61 67 71))
             (prime (nth (% k (length primes)) primes))
             (h (* (1+ k) 13))
             (i 0) (len (length key)))
        (while (< i len)
          (setq h (% (+ (* h prime) (aref key i) (* k 7)) 1000000007))
          (setq i (1+ i)))
        (% (abs h) size))))

  (fset 'neovm--bfsc-create
    (lambda (size num-hashes)
      (list size (make-vector size nil) num-hashes)))

  (fset 'neovm--bfsc-add
    (lambda (bf key)
      (let ((k 0))
        (while (< k (nth 2 bf))
          (aset (nth 1 bf) (funcall 'neovm--bfsc-hash key k (nth 0 bf)) t)
          (setq k (1+ k))))))

  (fset 'neovm--bfsc-check
    (lambda (bf key)
      (let ((k 0) (present t))
        (while (and present (< k (nth 2 bf)))
          (unless (aref (nth 1 bf) (funcall 'neovm--bfsc-hash key k (nth 0 bf)))
            (setq present nil))
          (setq k (1+ k)))
        present)))

  (fset 'neovm--bfsc-spellcheck
    (lambda (bf dict words)
      "Check each word: use bloom filter first, then exact dict lookup.
       Returns list of (word status bloom-checked dict-checked) tuples."
      (let ((result nil)
            (bloom-checks 0)
            (dict-checks 0))
        (dolist (w words)
          (setq bloom-checks (1+ bloom-checks))
          (if (not (funcall 'neovm--bfsc-check bf w))
              ;; Bloom says definitely not in dictionary
              (setq result (cons (list w 'misspelled bloom-checks dict-checks) result))
            ;; Bloom says maybe: check dict
            (setq dict-checks (1+ dict-checks))
            (if (gethash w dict)
                (setq result (cons (list w 'correct bloom-checks dict-checks) result))
              (setq result (cons (list w 'misspelled bloom-checks dict-checks) result)))))
        (list (nreverse result)
              bloom-checks
              dict-checks))))

  (unwind-protect
      (let ((bf (funcall 'neovm--bfsc-create 256 5))
            (dict (make-hash-table :test 'equal))
            (dictionary-words '("the" "quick" "brown" "fox" "jumps" "over"
                                "lazy" "dog" "hello" "world" "programming"
                                "language" "computer" "science" "algorithm"
                                "data" "structure" "function" "variable"
                                "constant")))
        ;; Build dictionary
        (dolist (w dictionary-words)
          (funcall 'neovm--bfsc-add bf w)
          (puthash w t dict))
        ;; Test words: mix of correct and misspelled
        (let ((test-words '("the" "quik" "brown" "foxx" "jumps"
                             "ovr" "lazy" "dogg" "hello" "wrld"
                             "programming" "languge" "xyz" "algorithm"
                             "dat" "structure")))
          (funcall 'neovm--bfsc-spellcheck bf dict test-words)))
    (fmakunbound 'neovm--bfsc-hash)
    (fmakunbound 'neovm--bfsc-create)
    (fmakunbound 'neovm--bfsc-add)
    (fmakunbound 'neovm--bfsc-check)
    (fmakunbound 'neovm--bfsc-spellcheck)))"#;
    let expect = expect_test::expect![[
        r#""OK (((\"the\" correct 1 1) (\"quik\" misspelled 2 1) (\"brown\" correct 3 2) (\"foxx\" misspelled 4 2) (\"jumps\" correct 5 3) (\"ovr\" misspelled 6 3) (\"lazy\" correct 7 4) (\"dogg\" misspelled 8 4) (\"hello\" correct 9 5) (\"wrld\" misspelled 10 5) (\"programming\" correct 11 6) (\"languge\" misspelled 12 6) (\"xyz\" misspelled 13 6) (\"algorithm\" correct 14 7) (\"dat\" misspelled 15 7) (\"structure\" correct 16 8)) 16 8)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: set membership testing with false-positive rate estimation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_bloom_adv_fp_rate_estimation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Empirically estimate false-positive rate by testing many non-member items.
    // Compare filters of different sizes to verify that larger filters have
    // lower FP rates.
    let form = r#"(progn
  (fset 'neovm--bfe-hash
    (lambda (key k size)
      (let* ((primes '(31 37 41 43 47 53 59 61 67 71))
             (prime (nth (% k (length primes)) primes))
             (h (* (1+ k) 13))
             (i 0)
             (s (if (stringp key) key (format "%s" key)))
             (len (length s)))
        (while (< i len)
          (setq h (% (+ (* h prime) (aref s i) (* k 7)) 1000000007))
          (setq i (1+ i)))
        (% (abs h) size))))

  (fset 'neovm--bfe-create
    (lambda (size num-hashes)
      (list size (make-vector size nil) num-hashes)))

  (fset 'neovm--bfe-add
    (lambda (bf key)
      (let ((k 0))
        (while (< k (nth 2 bf))
          (aset (nth 1 bf) (funcall 'neovm--bfe-hash key k (nth 0 bf)) t)
          (setq k (1+ k))))))

  (fset 'neovm--bfe-check
    (lambda (bf key)
      (let ((k 0) (present t))
        (while (and present (< k (nth 2 bf)))
          (unless (aref (nth 1 bf) (funcall 'neovm--bfe-hash key k (nth 0 bf)))
            (setq present nil))
          (setq k (1+ k)))
        present)))

  (fset 'neovm--bfe-bit-count
    (lambda (bf)
      (let ((count 0) (i 0) (size (nth 0 bf)))
        (while (< i size)
          (when (aref (nth 1 bf) i) (setq count (1+ count)))
          (setq i (1+ i)))
        count)))

  (fset 'neovm--bfe-estimate-fp-rate
    (lambda (bf num-tests prefix)
      "Estimate FP rate by testing NUM-TESTS items with given PREFIX."
      (let ((fp 0) (i 0))
        (while (< i num-tests)
          (when (funcall 'neovm--bfe-check bf (format "%s-%d" prefix i))
            (setq fp (1+ fp)))
          (setq i (1+ i)))
        (list fp num-tests))))

  (unwind-protect
      (let ((num-items 50)
            (num-hashes 4)
            (test-count 100)
            ;; Test with different filter sizes
            (sizes '(64 128 256 512 1024)))
        (mapcar
          (lambda (size)
            (let ((bf (funcall 'neovm--bfe-create size num-hashes)))
              ;; Add items with "item-" prefix
              (let ((i 0))
                (while (< i num-items)
                  (funcall 'neovm--bfe-add bf (format "item-%d" i))
                  (setq i (1+ i))))
              ;; Verify no false negatives
              (let ((fn-count 0) (i 0))
                (while (< i num-items)
                  (unless (funcall 'neovm--bfe-check bf (format "item-%d" i))
                    (setq fn-count (1+ fn-count)))
                  (setq i (1+ i)))
                ;; Estimate FP rate with "probe-" prefix (never added)
                (let ((fp-result (funcall 'neovm--bfe-estimate-fp-rate
                                          bf test-count "probe")))
                  (list
                    size
                    num-hashes
                    (funcall 'neovm--bfe-bit-count bf)
                    fn-count  ;; should be 0
                    (nth 0 fp-result)  ;; false positives
                    (nth 1 fp-result)  ;; total tests
                    (= fn-count 0))))))
          sizes))
    (fmakunbound 'neovm--bfe-hash)
    (fmakunbound 'neovm--bfe-create)
    (fmakunbound 'neovm--bfe-add)
    (fmakunbound 'neovm--bfe-check)
    (fmakunbound 'neovm--bfe-bit-count)
    (fmakunbound 'neovm--bfe-estimate-fp-rate)))"#;
    let expect = expect_test::expect![[
        r#""OK ((64 4 61 0 87 100 t) (128 4 102 0 59 100 t) (256 4 159 0 29 100 t) (512 4 169 0 6 100 t) (1024 4 179 0 0 100 t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: counting Bloom filter (supports approximate deletion)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_bloom_adv_counting_filter() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Instead of bits, use counters. Add increments counters, delete decrements.
    // Check: all counters at hash positions > 0.
    let form = r#"(progn
  (fset 'neovm--bfc-hash
    (lambda (key k size)
      (let* ((primes '(31 37 41 43 47 53 59 61))
             (prime (nth (% k (length primes)) primes))
             (h (* (1+ k) 13))
             (i 0)
             (s (if (stringp key) key (format "%s" key)))
             (len (length s)))
        (while (< i len)
          (setq h (% (+ (* h prime) (aref s i) (* k 7)) 1000000007))
          (setq i (1+ i)))
        (% (abs h) size))))

  (fset 'neovm--bfc-create
    (lambda (size num-hashes)
      (list size (make-vector size 0) num-hashes)))

  (fset 'neovm--bfc-add
    (lambda (bf key)
      (let ((k 0))
        (while (< k (nth 2 bf))
          (let ((idx (funcall 'neovm--bfc-hash key k (nth 0 bf))))
            (aset (nth 1 bf) idx (1+ (aref (nth 1 bf) idx))))
          (setq k (1+ k))))))

  (fset 'neovm--bfc-remove
    (lambda (bf key)
      "Decrement counters for KEY. Only call if KEY was previously added."
      (let ((k 0))
        (while (< k (nth 2 bf))
          (let ((idx (funcall 'neovm--bfc-hash key k (nth 0 bf))))
            (when (> (aref (nth 1 bf) idx) 0)
              (aset (nth 1 bf) idx (1- (aref (nth 1 bf) idx)))))
          (setq k (1+ k))))))

  (fset 'neovm--bfc-check
    (lambda (bf key)
      (let ((k 0) (present t))
        (while (and present (< k (nth 2 bf)))
          (let ((idx (funcall 'neovm--bfc-hash key k (nth 0 bf))))
            (when (= (aref (nth 1 bf) idx) 0)
              (setq present nil)))
          (setq k (1+ k)))
        present)))

  (unwind-protect
      (let ((bf (funcall 'neovm--bfc-create 128 4)))
        ;; Add items
        (dolist (item '("alpha" "beta" "gamma" "delta" "epsilon"))
          (funcall 'neovm--bfc-add bf item))
        ;; All present
        (let ((before-remove
                (mapcar (lambda (x) (funcall 'neovm--bfc-check bf x))
                        '("alpha" "beta" "gamma" "delta" "epsilon"))))
          ;; Remove some items
          (funcall 'neovm--bfc-remove bf "beta")
          (funcall 'neovm--bfc-remove bf "delta")
          ;; Check after removal
          (let ((after-remove
                  (mapcar (lambda (x) (funcall 'neovm--bfc-check bf x))
                          '("alpha" "beta" "gamma" "delta" "epsilon"))))
            ;; Add beta back
            (funcall 'neovm--bfc-add bf "beta")
            (let ((after-readd
                    (mapcar (lambda (x) (funcall 'neovm--bfc-check bf x))
                            '("alpha" "beta" "gamma" "delta" "epsilon"))))
              (list
                before-remove        ;; all t
                after-remove         ;; alpha t, beta nil(?), gamma t, delta nil(?), epsilon t
                after-readd          ;; beta back to t
                ;; Non-member should still be absent
                (funcall 'neovm--bfc-check bf "zeta")
                (funcall 'neovm--bfc-check bf "omega"))))))
    (fmakunbound 'neovm--bfc-hash)
    (fmakunbound 'neovm--bfc-create)
    (fmakunbound 'neovm--bfc-add)
    (fmakunbound 'neovm--bfc-remove)
    (fmakunbound 'neovm--bfc-check)))"#;
    let expect =
        expect_test::expect![[r#""OK ((t t t t t) (t nil t nil t) (t t t nil t) nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
