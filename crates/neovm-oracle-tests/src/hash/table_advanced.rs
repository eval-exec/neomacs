//! Advanced oracle parity tests for hash table operations.
//!
//! Tests make-hash-table parameters, count tracking through put/rem cycles,
//! maphash with side effects, set operations (union/intersection),
//! memoization of recursive functions, copy-hash-table independence,
//! symbol vs string key semantics, and trie (prefix tree) implementation.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// make-hash-table with :size, :test, :weakness params
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_hash_table_creation_params() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Verify hash-table-test, hash-table-size roundtrip
    let form = "(let ((h1 (make-hash-table :test 'eq :size 100))
                      (h2 (make-hash-table :test 'equal :size 10))
                      (h3 (make-hash-table :test 'eql))
                      (h4 (make-hash-table)))
                  (list
                    (hash-table-test h1)
                    (hash-table-test h2)
                    (hash-table-test h3)
                    (hash-table-test h4)
                    ;; size is a hint >= requested
                    (>= (hash-table-size h1) 100)
                    (hash-table-p h1)
                    (hash-table-p '(not a hash))
                    (hash-table-count h1)))";
    let expect = expect_test::expect![[r#""OK (eq equal eql eql t t nil 0)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// hash-table-count tracking through put/rem cycles
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_hash_table_count_cycles() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Insert, delete, re-insert, overwrite — track count precisely
    let form = "(let ((h (make-hash-table :test 'eq)))
                  (let ((counts nil))
                    ;; Phase 1: insert 10 keys
                    (dotimes (i 10) (puthash i (* i i) h))
                    (setq counts (cons (hash-table-count h) counts))
                    ;; Phase 2: remove even keys (0 2 4 6 8)
                    (dotimes (i 5) (remhash (* i 2) h))
                    (setq counts (cons (hash-table-count h) counts))
                    ;; Phase 3: overwrite remaining keys (no count change)
                    (dolist (k '(1 3 5 7 9))
                      (puthash k 'overwritten h))
                    (setq counts (cons (hash-table-count h) counts))
                    ;; Phase 4: re-insert some removed keys
                    (puthash 0 'back h)
                    (puthash 4 'back h)
                    (setq counts (cons (hash-table-count h) counts))
                    ;; Phase 5: clrhash
                    (clrhash h)
                    (setq counts (cons (hash-table-count h) counts))
                    ;; Phase 6: re-use after clear
                    (puthash 'new-key 'new-val h)
                    (setq counts (cons (hash-table-count h) counts))
                    (list (nreverse counts)
                          (gethash 'new-key h)
                          (gethash 0 h))))";
    let expect = expect_test::expect![[r#""OK ((10 5 5 7 0 1) new-val nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// maphash with side effects: modifying another hash table
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_maphash_cross_modification() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a reversed mapping and a filtered copy via maphash
    let form = "(let ((src (make-hash-table :test 'eq))
                      (reversed (make-hash-table :test 'eq))
                      (filtered (make-hash-table :test 'eq)))
                  ;; Populate source
                  (puthash 'a 1 src)
                  (puthash 'b 2 src)
                  (puthash 'c 3 src)
                  (puthash 'd 4 src)
                  (puthash 'e 5 src)
                  ;; Reverse: val -> key (integers as keys)
                  (maphash (lambda (k v) (puthash v k reversed)) src)
                  ;; Filter: only odd values
                  (maphash (lambda (k v)
                             (when (= 1 (% v 2))
                               (puthash k v filtered)))
                           src)
                  ;; Collect reversed mapping
                  (let ((rev-pairs nil))
                    (maphash (lambda (k v)
                               (setq rev-pairs (cons (cons k v) rev-pairs)))
                             reversed)
                    (let ((filt-pairs nil))
                      (maphash (lambda (k v)
                                 (setq filt-pairs (cons (cons k v) filt-pairs)))
                               filtered)
                      (list
                        (hash-table-count reversed)
                        (hash-table-count filtered)
                        (gethash 1 reversed)
                        (gethash 3 reversed)
                        (sort rev-pairs (lambda (a b) (< (car a) (car b))))
                        (sort filt-pairs (lambda (a b)
                                           (string-lessp (symbol-name (car a))
                                                         (symbol-name (car b)))))))))";
    let expect = expect_test::expect![[
        r#""OK (5 3 a c ((1 . a) (2 . b) (3 . c) (4 . d) (5 . e)) ((a . 1) (c . 3) (e . 5)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Hash table as set: membership, union, intersection, difference
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_hash_table_set_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((set-a (make-hash-table :test 'eq))
                      (set-b (make-hash-table :test 'eq)))
                  ;; Populate sets
                  (dolist (x '(a b c d e)) (puthash x t set-a))
                  (dolist (x '(c d e f g)) (puthash x t set-b))
                  ;; Union
                  (let ((union (make-hash-table :test 'eq)))
                    (maphash (lambda (k _v) (puthash k t union)) set-a)
                    (maphash (lambda (k _v) (puthash k t union)) set-b)
                    ;; Intersection
                    (let ((inter (make-hash-table :test 'eq)))
                      (maphash (lambda (k _v)
                                 (when (gethash k set-b)
                                   (puthash k t inter)))
                               set-a)
                      ;; Difference: A - B
                      (let ((diff (make-hash-table :test 'eq)))
                        (maphash (lambda (k _v)
                                   (unless (gethash k set-b)
                                     (puthash k t diff)))
                                 set-a)
                        ;; Collect and sort results
                        (let ((collect (lambda (ht)
                                         (let ((items nil))
                                           (maphash (lambda (k _v)
                                                      (setq items (cons k items)))
                                                    ht)
                                           (sort items (lambda (a b)
                                                         (string-lessp
                                                          (symbol-name a)
                                                          (symbol-name b))))))))
                          (list
                            (funcall collect union)
                            (funcall collect inter)
                            (funcall collect diff)
                            ;; Symmetric difference: (A-B) union (B-A)
                            (let ((sym-diff (make-hash-table :test 'eq)))
                              (maphash (lambda (k _v)
                                         (unless (gethash k set-b)
                                           (puthash k t sym-diff)))
                                       set-a)
                              (maphash (lambda (k _v)
                                         (unless (gethash k set-a)
                                           (puthash k t sym-diff)))
                                       set-b)
                              (funcall collect sym-diff))))))))";
    let expect = expect_test::expect![[r#""OK ((a b c d e f g) (c d e) (a b) (a b f g))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Hash table for memoization of recursive function
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_hash_table_memoization() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Memoized Catalan number computation
    let form = "(progn
  (fset 'neovm--test-catalan
    (lambda (n memo)
      (cond
        ((= n 0) 1)
        ((gethash n memo))
        (t (let ((result 0))
             (dotimes (i n)
               (setq result
                     (+ result
                        (* (funcall 'neovm--test-catalan i memo)
                           (funcall 'neovm--test-catalan (- n 1 i) memo)))))
             (puthash n result memo)
             result)))))
  (unwind-protect
      (let ((memo (make-hash-table)))
        (list
          (funcall 'neovm--test-catalan 0 memo)
          (funcall 'neovm--test-catalan 1 memo)
          (funcall 'neovm--test-catalan 2 memo)
          (funcall 'neovm--test-catalan 3 memo)
          (funcall 'neovm--test-catalan 4 memo)
          (funcall 'neovm--test-catalan 5 memo)
          (funcall 'neovm--test-catalan 8 memo)
          ;; Verify memoization populated
          (hash-table-count memo)))
    (fmakunbound 'neovm--test-catalan)))";
    let expect = expect_test::expect![[r#""OK (1 1 2 5 14 42 1430 8)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// copy-hash-table independence verification
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_copy_hash_table_deep_independence() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Copy a hash table, then modify both independently
    let form = "(let ((orig (make-hash-table :test 'equal)))
                  ;; Populate with various key types
                  (puthash 'sym 100 orig)
                  (puthash 42 200 orig)
                  (puthash \"str\" 300 orig)
                  (puthash '(a b) 400 orig)
                  (let ((copy (copy-hash-table orig)))
                    ;; Modify original
                    (puthash 'sym 999 orig)
                    (remhash 42 orig)
                    (puthash 'new-key 500 orig)
                    ;; Modify copy differently
                    (puthash \"str\" 888 copy)
                    (puthash 'copy-only 600 copy)
                    ;; Verify independence
                    (list
                      ;; Original state
                      (gethash 'sym orig)
                      (gethash 42 orig)
                      (gethash \"str\" orig)
                      (gethash 'new-key orig)
                      (hash-table-count orig)
                      ;; Copy state (should retain old values)
                      (gethash 'sym copy)
                      (gethash 42 copy)
                      (gethash \"str\" copy)
                      (gethash 'new-key copy)
                      (gethash 'copy-only copy)
                      (hash-table-count copy)
                      ;; Test preserved
                      (eq (hash-table-test orig) (hash-table-test copy)))))";
    let expect = expect_test::expect![[r#""OK (999 nil 300 500 4 100 200 888 nil 600 5 t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Symbol keys (eq) vs string keys (equal) semantics
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_hash_table_eq_vs_equal_keys() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Demonstrate that eq test treats each string literal as different object,
    // while equal test treats string content as same key
    let form = r#"(let ((h-eq (make-hash-table :test 'eq))
                        (h-equal (make-hash-table :test 'equal)))
                    ;; Symbols: always eq-identical, so both should work
                    (puthash 'foo 1 h-eq)
                    (puthash 'foo 1 h-equal)
                    ;; Lists: eq won't match new cons cells, equal will
                    (let ((key-list (list 1 2 3)))
                      (puthash key-list 'found-eq h-eq)
                      (puthash key-list 'found-equal h-equal)
                      ;; Lookup with a structurally equal but different cons cell
                      (let ((other-list (list 1 2 3)))
                        (list
                          ;; Symbol lookup works for both
                          (gethash 'foo h-eq)
                          (gethash 'foo h-equal)
                          ;; Same object lookup works for both
                          (gethash key-list h-eq)
                          (gethash key-list h-equal)
                          ;; Different object, same structure:
                          ;; eq fails, equal succeeds
                          (gethash other-list h-eq)
                          (gethash other-list h-equal)
                          ;; Number keys: eql/equal match by value
                          (progn (puthash 42 'num-eq h-eq)
                                 (gethash 42 h-eq))
                          (progn (puthash 42 'num-equal h-equal)
                                 (gethash 42 h-equal))))))"#;
    let expect = expect_test::expect![[
        r#""OK (1 1 found-eq found-equal nil found-equal num-eq num-equal)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Hash-table-based trie (prefix tree) for string lookup
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_hash_table_trie() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a trie from words, then query prefixes and full words
    let form = r#"(progn
  (fset 'neovm--trie-insert
    (lambda (trie word)
      (let ((node trie)
            (i 0)
            (len (length word)))
        (while (< i len)
          (let ((ch (aref word i)))
            (let ((child (gethash ch node)))
              (unless child
                (setq child (make-hash-table))
                (puthash ch child node))
              (setq node child)))
          (setq i (1+ i)))
        ;; Mark end-of-word
        (puthash 'end t node))))
  (fset 'neovm--trie-search
    (lambda (trie word)
      (let ((node trie)
            (i 0)
            (len (length word))
            (found t))
        (while (and (< i len) found)
          (let ((child (gethash (aref word i) node)))
            (if child
                (setq node child)
              (setq found nil)))
          (setq i (1+ i)))
        (and found (gethash 'end node nil)))))
  (fset 'neovm--trie-starts-with
    (lambda (trie prefix)
      (let ((node trie)
            (i 0)
            (len (length prefix))
            (found t))
        (while (and (< i len) found)
          (let ((child (gethash (aref prefix i) node)))
            (if child
                (setq node child)
              (setq found nil)))
          (setq i (1+ i)))
        found)))
  (unwind-protect
      (let ((trie (make-hash-table)))
        (funcall 'neovm--trie-insert trie "apple")
        (funcall 'neovm--trie-insert trie "app")
        (funcall 'neovm--trie-insert trie "application")
        (funcall 'neovm--trie-insert trie "banana")
        (funcall 'neovm--trie-insert trie "band")
        (list
          ;; Exact matches
          (funcall 'neovm--trie-search trie "apple")
          (funcall 'neovm--trie-search trie "app")
          (funcall 'neovm--trie-search trie "application")
          (funcall 'neovm--trie-search trie "banana")
          ;; Not inserted
          (funcall 'neovm--trie-search trie "appl")
          (funcall 'neovm--trie-search trie "ban")
          (funcall 'neovm--trie-search trie "cherry")
          ;; Prefix queries
          (funcall 'neovm--trie-starts-with trie "app")
          (funcall 'neovm--trie-starts-with trie "ban")
          (funcall 'neovm--trie-starts-with trie "che")
          (funcall 'neovm--trie-starts-with trie "apples")))
    (fmakunbound 'neovm--trie-insert)
    (fmakunbound 'neovm--trie-search)
    (fmakunbound 'neovm--trie-starts-with)))"#;
    let expect = expect_test::expect![[r#""OK (t t t t nil nil nil t t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
