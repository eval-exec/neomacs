//! Oracle parity tests for hash-based algorithm implementations in Elisp.
//!
//! Tests two-sum, anagram grouping, LRU cache simulation,
//! frequency-based encoding, duplicate detection in nested structures,
//! and set operations via hash tables.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Two-sum problem using hash table
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_hash_algo_two_sum() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Given a list of numbers and a target, find two indices whose values sum to target
    let form = "(progn
  (fset 'neovm--test-two-sum
    (lambda (nums target)
      (let ((seen (make-hash-table :test 'eql))
            (result nil)
            (idx 0))
        (while (and (not result) nums)
          (let* ((num (car nums))
                 (complement (- target num))
                 (prev-idx (gethash complement seen)))
            (if prev-idx
                (setq result (list prev-idx idx))
              (puthash num idx seen)))
          (setq nums (cdr nums))
          (setq idx (1+ idx)))
        result)))

  (unwind-protect
      (list
        ;; Basic case
        (funcall 'neovm--test-two-sum '(2 7 11 15) 9)
        ;; Target at end
        (funcall 'neovm--test-two-sum '(3 2 4) 6)
        ;; Same number twice
        (funcall 'neovm--test-two-sum '(3 3) 6)
        ;; No solution
        (funcall 'neovm--test-two-sum '(1 2 3) 10)
        ;; Negative numbers
        (funcall 'neovm--test-two-sum '(-1 -2 -3 -4 -5) -8)
        ;; Mixed positive and negative
        (funcall 'neovm--test-two-sum '(5 -3 8 -1 4 2) 1)
        ;; Larger input
        (funcall 'neovm--test-two-sum '(10 20 30 40 50 60 70 80 90 100) 150)
        ;; Verify the indices are correct
        (let* ((nums '(1 5 3 8 2 7))
               (target 10)
               (indices (funcall 'neovm--test-two-sum nums target)))
          (if indices
              (= target (+ (nth (car indices) nums)
                           (nth (cadr indices) nums)))
            nil)))
    (fmakunbound 'neovm--test-two-sum)))";
    let expect = expect_test::expect![[r#""OK ((0 1) (1 2) (0 1) nil (2 4) (1 4) (6 7) t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Anagram grouping using sorted-string keys
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_hash_algo_anagram_grouping() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Sort a string's characters to produce a canonical key
  (fset 'neovm--test-sort-string
    (lambda (s)
      (concat (sort (append s nil) #'<))))

  ;; Group words by anagram equivalence class
  (fset 'neovm--test-group-anagrams
    (lambda (words)
      (let ((groups (make-hash-table :test 'equal)))
        (dolist (word words)
          (let ((key (funcall 'neovm--test-sort-string word)))
            (puthash key (cons word (gethash key groups nil)) groups)))
        ;; Collect groups, sort each group, then sort groups by first element
        (let ((result nil))
          (maphash (lambda (_k v)
                     (setq result (cons (sort v #'string<) result)))
                   groups)
          (sort result (lambda (a b) (string< (car a) (car b))))))))

  (unwind-protect
      (list
        ;; Classic anagram grouping
        (funcall 'neovm--test-group-anagrams
                 '("eat" "tea" "tan" "ate" "nat" "bat"))
        ;; Single-char words
        (funcall 'neovm--test-group-anagrams '("a" "b" "a" "c" "b"))
        ;; No anagrams (all unique)
        (funcall 'neovm--test-group-anagrams '("abc" "def" "ghi"))
        ;; All anagrams of each other
        (funcall 'neovm--test-group-anagrams '("abc" "bca" "cab" "bac"))
        ;; Empty input
        (funcall 'neovm--test-group-anagrams nil)
        ;; Mixed lengths
        (funcall 'neovm--test-group-anagrams
                 '("listen" "silent" "hello" "world" "enlist" "inlets"))
        ;; Count groups
        (length (funcall 'neovm--test-group-anagrams
                         '("eat" "tea" "tan" "ate" "nat" "bat"))))
    (fmakunbound 'neovm--test-sort-string)
    (fmakunbound 'neovm--test-group-anagrams)))"#;
    let expect = expect_test::expect![[
        r#""OK (((\"ate\" \"eat\" \"tea\") (\"bat\") (\"nat\" \"tan\")) ((\"a\" \"a\") (\"b\" \"b\") (\"c\")) ((\"abc\") (\"def\") (\"ghi\")) ((\"abc\" \"bac\" \"bca\" \"cab\")) nil ((\"enlist\" \"inlets\" \"listen\" \"silent\") (\"hello\") (\"world\")) 3)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// LRU cache simulation with hash table + ordered list
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_hash_algo_lru_cache() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate an LRU cache with capacity limit.
    // Uses a hash table for O(1) lookup and an ordered list for recency.
    let form = "(progn
  ;; LRU cache: (capacity hash-table order-list)
  ;; order-list: most recently used at front
  (fset 'neovm--test-lru-make
    (lambda (capacity)
      (list capacity (make-hash-table :test 'equal) nil)))

  (fset 'neovm--test-lru-get
    (lambda (cache key)
      (let ((ht (nth 1 cache))
            (val (gethash key (nth 1 cache))))
        (when val
          ;; Move key to front of order list
          (setcar (cddr cache)
                  (cons key (delq key (nth 2 cache)))))
        val)))

  (fset 'neovm--test-lru-put
    (lambda (cache key value)
      (let ((ht (nth 1 cache))
            (cap (nth 0 cache)))
        ;; If key exists, update and move to front
        (if (gethash key ht)
            (progn
              (puthash key value ht)
              (setcar (cddr cache)
                      (cons key (delq key (nth 2 cache)))))
          ;; New key: check capacity
          (when (>= (hash-table-count ht) cap)
            ;; Evict least recently used (last in order list)
            (let* ((order (nth 2 cache))
                   (lru-key (car (last order))))
              (remhash lru-key ht)
              (setcar (cddr cache)
                      (butlast order))))
          (puthash key value ht)
          (setcar (cddr cache)
                  (cons key (nth 2 cache)))))
      value))

  (fset 'neovm--test-lru-keys
    (lambda (cache)
      (nth 2 cache)))

  (unwind-protect
      (let ((cache (funcall 'neovm--test-lru-make 3)))
        ;; Insert 3 items (at capacity)
        (funcall 'neovm--test-lru-put cache 'a 10)
        (funcall 'neovm--test-lru-put cache 'b 20)
        (funcall 'neovm--test-lru-put cache 'c 30)
        (let ((state1 (list
                        (funcall 'neovm--test-lru-keys cache)
                        (funcall 'neovm--test-lru-get cache 'a)
                        (funcall 'neovm--test-lru-get cache 'b)
                        (funcall 'neovm--test-lru-get cache 'c))))
          ;; Access 'a' to make it most recent
          (funcall 'neovm--test-lru-get cache 'a)
          (let ((state2 (funcall 'neovm--test-lru-keys cache)))
            ;; Insert 'd' — should evict 'b' (least recent)
            (funcall 'neovm--test-lru-put cache 'd 40)
            (let ((state3 (list
                            (funcall 'neovm--test-lru-keys cache)
                            (funcall 'neovm--test-lru-get cache 'a)
                            (funcall 'neovm--test-lru-get cache 'b)
                            (funcall 'neovm--test-lru-get cache 'c)
                            (funcall 'neovm--test-lru-get cache 'd))))
              ;; Insert 'e' — should evict least recent
              (funcall 'neovm--test-lru-put cache 'e 50)
              (list state1 state2 state3
                    (funcall 'neovm--test-lru-keys cache)
                    (hash-table-count (nth 1 cache)))))))
    (fmakunbound 'neovm--test-lru-make)
    (fmakunbound 'neovm--test-lru-get)
    (fmakunbound 'neovm--test-lru-put)
    (fmakunbound 'neovm--test-lru-keys)))";
    let expect =
        expect_test::expect![[r#""OK (((c) 10 20 30) (a c b) ((d) 10 nil 30 40) (e d c) 3)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Frequency-based encoding (Huffman-like code assignment)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_hash_algo_frequency_encoding() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Count character frequencies, assign variable-length codes
    // based on frequency ranking (most frequent = shortest code)
    let form = r#"(progn
  ;; Count character frequencies in a string
  (fset 'neovm--test-char-freq
    (lambda (str)
      (let ((freq (make-hash-table :test 'eql)))
        (dotimes (i (length str))
          (let ((ch (aref str i)))
            (puthash ch (1+ (gethash ch freq 0)) freq)))
        freq)))

  ;; Convert hash table to sorted alist (by frequency descending)
  (fset 'neovm--test-freq-to-sorted
    (lambda (freq-ht)
      (let ((pairs nil))
        (maphash (lambda (k v)
                   (setq pairs (cons (cons k v) pairs)))
                 freq-ht)
        (sort pairs (lambda (a b)
                      (or (> (cdr a) (cdr b))
                          (and (= (cdr a) (cdr b))
                               (< (car a) (car b)))))))))

  ;; Assign codes: rank 0 gets "0", rank 1 gets "10", rank 2 gets "110", etc.
  ;; (prefix-free unary coding for simplicity)
  (fset 'neovm--test-assign-codes
    (lambda (sorted-freq)
      (let ((codes (make-hash-table :test 'eql))
            (rank 0))
        (dolist (pair sorted-freq)
          (let ((code (if (= rank 0) "0"
                        (concat (make-string rank ?1) "0"))))
            (puthash (car pair) code codes))
          (setq rank (1+ rank)))
        codes)))

  ;; Encode a string using the code table
  (fset 'neovm--test-encode
    (lambda (str codes)
      (let ((result ""))
        (dotimes (i (length str))
          (setq result (concat result (gethash (aref str i) codes))))
        result)))

  (unwind-protect
      (let* ((text "abracadabra")
             (freq (funcall 'neovm--test-char-freq text))
             (sorted (funcall 'neovm--test-freq-to-sorted freq))
             (codes (funcall 'neovm--test-assign-codes sorted))
             (encoded (funcall 'neovm--test-encode text codes)))
        (list
          ;; Frequency counts
          sorted
          ;; Code assignments (as alist)
          (let ((code-alist nil))
            (maphash (lambda (k v)
                       (setq code-alist (cons (cons k v) code-alist)))
                     codes)
            (sort code-alist (lambda (a b) (< (car a) (car b)))))
          ;; Encoded result
          encoded
          ;; Encoded length vs original
          (list (length text) (length encoded))
          ;; Most frequent char should have shortest code
          (let ((most-freq-char (caar sorted)))
            (length (gethash most-freq-char codes)))
          ;; Total unique characters
          (hash-table-count freq)))
    (fmakunbound 'neovm--test-char-freq)
    (fmakunbound 'neovm--test-freq-to-sorted)
    (fmakunbound 'neovm--test-assign-codes)
    (fmakunbound 'neovm--test-encode)))"#;
    let expect = expect_test::expect![[
        r#""OK (((97 . 5) (98 . 2) (114 . 2) (99 . 1) (100 . 1)) ((97 . \"0\") (98 . \"10\") (99 . \"1110\") (100 . \"11110\") (114 . \"110\")) \"010110011100111100101100\" (11 24) 1 5)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Hash-based duplicate detection in nested structures
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_hash_algo_nested_duplicate_detection() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Walk nested lists/vectors, serialize each sub-structure,
    // detect duplicates by checking a hash table of seen serializations
    let form = r#"(progn
  ;; Recursively find all sub-expressions and detect duplicates
  (fset 'neovm--test-find-duplicates
    (lambda (structure)
      (let ((seen (make-hash-table :test 'equal))
            (dups (make-hash-table :test 'equal)))
        ;; Walk and record all sub-structures
        (fset 'neovm--test-walk
          (lambda (node)
            (when (or (consp node) (vectorp node))
              (let ((key (prin1-to-string node)))
                (if (gethash key seen)
                    (puthash key (1+ (gethash key dups 1)) dups)
                  (puthash key t seen)))
              (cond
                ((consp node)
                 (funcall 'neovm--test-walk (car node))
                 (funcall 'neovm--test-walk (cdr node)))
                ((vectorp node)
                 (dotimes (i (length node))
                   (funcall 'neovm--test-walk (aref node i))))))))
        (funcall 'neovm--test-walk structure)
        ;; Collect duplicates as sorted alist
        (let ((result nil))
          (maphash (lambda (k v)
                     (setq result (cons (cons k v) result)))
                   dups)
          (sort result (lambda (a b) (string< (car a) (car b))))))))

  (unwind-protect
      (list
        ;; Structure with repeated sub-lists
        (funcall 'neovm--test-find-duplicates
                 '((1 2) (3 4) (1 2) (5 6) (3 4) (1 2)))
        ;; Nested with duplicates at different levels
        (funcall 'neovm--test-find-duplicates
                 '((a b) ((a b) c) ((a b) c)))
        ;; No duplicates
        (funcall 'neovm--test-find-duplicates
                 '((1) (2) (3) (4)))
        ;; Vectors with duplicates
        (funcall 'neovm--test-find-duplicates
                 (list [1 2] [3 4] [1 2]))
        ;; Deeply nested
        (funcall 'neovm--test-find-duplicates
                 '((x (y z)) (a (y z)) (x (y z)))))
    (fmakunbound 'neovm--test-walk)
    (fmakunbound 'neovm--test-find-duplicates)))"#;
    let expect = expect_test::expect![[
        r#""OK (((\"(1 2)\" . 3) (\"(2)\" . 3) (\"(3 4)\" . 2) (\"(4)\" . 2)) ((\"((a b) c)\" . 2) (\"(a b)\" . 3) (\"(b)\" . 3) (\"(c)\" . 2)) nil ((\"[1 2]\" . 2)) ((\"((y z))\" . 3) (\"(x (y z))\" . 2) (\"(y z)\" . 3) (\"(z)\" . 3)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Set operations: intersection, union, symmetric difference via hash tables
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_hash_algo_set_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn
  ;; Build hash-set from list
  (fset 'neovm--test-make-set
    (lambda (lst)
      (let ((s (make-hash-table :test 'equal)))
        (dolist (x lst) (puthash x t s))
        s)))

  ;; Set to sorted list
  (fset 'neovm--test-set-to-list
    (lambda (s)
      (let ((r nil))
        (maphash (lambda (k _v) (setq r (cons k r))) s)
        (sort r #'<))))

  ;; Union: elements in A or B
  (fset 'neovm--test-set-union
    (lambda (a b)
      (let ((result (make-hash-table :test 'equal)))
        (maphash (lambda (k _v) (puthash k t result)) a)
        (maphash (lambda (k _v) (puthash k t result)) b)
        result)))

  ;; Intersection: elements in both A and B
  (fset 'neovm--test-set-intersection
    (lambda (a b)
      (let ((result (make-hash-table :test 'equal)))
        (maphash (lambda (k _v)
                   (when (gethash k b)
                     (puthash k t result)))
                 a)
        result)))

  ;; Symmetric difference: elements in A xor B
  (fset 'neovm--test-set-sym-diff
    (lambda (a b)
      (let ((result (make-hash-table :test 'equal)))
        (maphash (lambda (k _v)
                   (unless (gethash k b)
                     (puthash k t result)))
                 a)
        (maphash (lambda (k _v)
                   (unless (gethash k a)
                     (puthash k t result)))
                 b)
        result)))

  ;; Subset check: is A a subset of B?
  (fset 'neovm--test-set-subset-p
    (lambda (a b)
      (let ((is-subset t))
        (maphash (lambda (k _v)
                   (unless (gethash k b)
                     (setq is-subset nil)))
                 a)
        is-subset)))

  (unwind-protect
      (let ((a (funcall 'neovm--test-make-set '(1 2 3 4 5 6)))
            (b (funcall 'neovm--test-make-set '(4 5 6 7 8 9)))
            (c (funcall 'neovm--test-make-set '(4 5)))
            (empty (funcall 'neovm--test-make-set nil)))
        (list
          ;; Union
          (funcall 'neovm--test-set-to-list
                   (funcall 'neovm--test-set-union a b))
          ;; Intersection
          (funcall 'neovm--test-set-to-list
                   (funcall 'neovm--test-set-intersection a b))
          ;; Symmetric difference
          (funcall 'neovm--test-set-to-list
                   (funcall 'neovm--test-set-sym-diff a b))
          ;; Subset checks
          (funcall 'neovm--test-set-subset-p c a)
          (funcall 'neovm--test-set-subset-p a c)
          (funcall 'neovm--test-set-subset-p empty a)
          ;; Union with empty
          (funcall 'neovm--test-set-to-list
                   (funcall 'neovm--test-set-union a empty))
          ;; Intersection with empty
          (funcall 'neovm--test-set-to-list
                   (funcall 'neovm--test-set-intersection a empty))
          ;; Symmetric difference with itself (should be empty)
          (funcall 'neovm--test-set-to-list
                   (funcall 'neovm--test-set-sym-diff a a))
          ;; |A union B| = |A| + |B| - |A intersect B|
          (let ((union-count (hash-table-count
                               (funcall 'neovm--test-set-union a b)))
                (inter-count (hash-table-count
                               (funcall 'neovm--test-set-intersection a b))))
            (= union-count
               (- (+ (hash-table-count a) (hash-table-count b))
                  inter-count)))))
    (fmakunbound 'neovm--test-make-set)
    (fmakunbound 'neovm--test-set-to-list)
    (fmakunbound 'neovm--test-set-union)
    (fmakunbound 'neovm--test-set-intersection)
    (fmakunbound 'neovm--test-set-sym-diff)
    (fmakunbound 'neovm--test-set-subset-p)))";
    let expect = expect_test::expect![[
        r#""OK ((1 2 3 4 5 6 7 8 9) (4 5 6) (1 2 3 7 8 9) t nil t (1 2 3 4 5 6) nil nil t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
