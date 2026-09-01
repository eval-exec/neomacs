//! Oracle parity tests for extended hash table operations:
//! `:test` parameter semantics (eq, eql, equal), `:size`, `:weakness`,
//! `hash-table-rehash-size`, `hash-table-rehash-threshold`, iteration
//! with `maphash` + concurrent modification, `copy-hash-table` independence,
//! hash table as frequency counter, and memoization patterns.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// :test parameter semantics — eq vs eql vs equal with edge cases
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_hash_table_ops_ext_test_param_edge_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Explore boundary behavior where eq, eql, equal diverge:
    // - eq: identity-only (symbols and fixnums)
    // - eql: like eq but also compares floats by value
    // - equal: deep structural comparison (strings, lists, vectors)
    let form = r#"(let ((h-eq (make-hash-table :test 'eq))
                        (h-eql (make-hash-table :test 'eql))
                        (h-equal (make-hash-table :test 'equal)))
                    ;; Test 1: Strings — eq fails on distinct copies, equal succeeds
                    (let ((s1 (copy-sequence "hello"))
                          (s2 (copy-sequence "hello")))
                      (puthash s1 'found-eq h-eq)
                      (puthash s1 'found-eql h-eql)
                      (puthash s1 'found-equal h-equal)
                      ;; Lookup with a different but equal string
                      (let ((eq-result (gethash s2 h-eq))
                            (eql-result (gethash s2 h-eql))
                            (equal-result (gethash s2 h-equal)))
                        ;; Test 2: Integer 1 vs float 1.0 — eql treats as different, equal treats as different
                        (let ((h-eql2 (make-hash-table :test 'eql))
                              (h-equal2 (make-hash-table :test 'equal)))
                          (puthash 1 'int-one h-eql2)
                          (puthash 1 'int-one h-equal2)
                          (let ((eql-int (gethash 1 h-eql2))
                                (eql-float (gethash 1.0 h-eql2))
                                (equal-int (gethash 1 h-equal2))
                                (equal-float (gethash 1.0 h-equal2)))
                            ;; Test 3: Cons cells — equal compares structure
                            (puthash '(a b c) 'triple h-equal)
                            (let ((list-lookup (gethash (list 'a 'b 'c) h-equal)))
                              ;; Test 4: Vectors — equal compares element-wise
                              (puthash [1 2 3] 'vec-triple h-equal)
                              (let ((vec-lookup (gethash (vector 1 2 3) h-equal)))
                                (list
                                 ;; String tests
                                 eq-result     ; nil — eq on different string objects
                                 eql-result    ; nil — eql on strings is like eq
                                 equal-result  ; found-equal
                                 ;; Int vs float
                                 eql-int eql-float
                                 equal-int equal-float
                                 ;; List structural equality
                                 list-lookup
                                 ;; Vector structural equality
                                 vec-lookup
                                 ;; Verify test functions reported correctly
                                 (hash-table-test h-eq)
                                 (hash-table-test h-eql)
                                 (hash-table-test h-equal)))))))))"#;
    let expect = expect_test::expect![[
        r#""OK (nil nil found-equal int-one nil int-one nil triple vec-triple eq eql equal)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// :size parameter and rehash introspection
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_hash_table_ops_ext_size_and_rehash_introspection() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Create tables with different sizes, fill them, and inspect rehash parameters
    let form = r#"(let ((h1 (make-hash-table :size 5))
                        (h2 (make-hash-table :size 100))
                        (h3 (make-hash-table :size 1)))
                    ;; Fill h1 past its initial size to trigger rehash
                    (dotimes (i 50)
                      (puthash i (* i i) h1))
                    ;; h2 stays below capacity
                    (dotimes (i 10)
                      (puthash i (+ i 100) h2))
                    ;; h3 with minimal size
                    (puthash 'only 'one h3)
                    (list
                     ;; Counts are correct regardless of internal sizing
                     (hash-table-count h1)
                     (hash-table-count h2)
                     (hash-table-count h3)
                     ;; hash-table-size returns current allocated size (implementation detail,
                     ;; but >= count must hold)
                     (>= (hash-table-size h1) (hash-table-count h1))
                     (>= (hash-table-size h2) (hash-table-count h2))
                     (>= (hash-table-size h3) (hash-table-count h3))
                     ;; Values are retrievable
                     (gethash 0 h1)
                     (gethash 49 h1)
                     (gethash 7 h2)
                     (gethash 'only h3)
                     ;; rehash-size and rehash-threshold are numbers
                     (numberp (hash-table-rehash-size h1))
                     (numberp (hash-table-rehash-threshold h1))
                     ;; hash-table-p
                     (hash-table-p h1)
                     (hash-table-p '(not a hash))))"#;
    let expect = expect_test::expect![[r#""OK (50 10 1 t t t 0 2401 107 one t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// maphash iteration with accumulation and post-iteration modification
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_hash_table_ops_ext_maphash_accumulate_then_modify() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use maphash to collect keys meeting a predicate, then modify the table
    // based on the collected results (avoiding modification during iteration).
    let form = r#"(let ((inventory (make-hash-table :test 'equal)))
                    ;; Stock inventory: item -> (quantity . price)
                    (dolist (item '(("apple" 50 . 1.5)
                                    ("banana" 200 . 0.75)
                                    ("cherry" 5 . 3.0)
                                    ("date" 0 . 5.0)
                                    ("elderberry" 2 . 8.0)
                                    ("fig" 100 . 2.0)
                                    ("grape" 0 . 1.25)
                                    ("honeydew" 15 . 4.0)))
                      (puthash (car item) (cdr item) inventory))
                    ;; Phase 1: Collect items with zero or low stock (< 10)
                    (let ((low-stock nil)
                          (zero-stock nil)
                          (total-value 0))
                      (maphash (lambda (name qty-price)
                                 (let ((qty (car qty-price))
                                       (price (cdr qty-price)))
                                   (setq total-value (+ total-value (* qty price)))
                                   (cond
                                    ((= qty 0) (setq zero-stock (cons name zero-stock)))
                                    ((< qty 10) (setq low-stock (cons name low-stock))))))
                               inventory)
                      ;; Phase 2: Remove zero-stock items, restock low-stock to 50
                      (dolist (name zero-stock)
                        (remhash name inventory))
                      (dolist (name low-stock)
                        (let ((old (gethash name inventory)))
                          (puthash name (cons 50 (cdr old)) inventory)))
                      ;; Phase 3: Verify final state
                      (let ((final-count (hash-table-count inventory))
                            (final-items nil))
                        (maphash (lambda (k v) (setq final-items (cons (list k (car v)) final-items)))
                                 inventory)
                        (list
                         (sort zero-stock #'string<)
                         (sort low-stock #'string<)
                         final-count
                         (sort final-items (lambda (a b) (string< (car a) (car b))))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"date\" \"grape\") (\"cherry\" \"elderberry\") 6 ((\"apple\" 50) (\"banana\" 200) (\"cherry\" 50) (\"elderberry\" 50) (\"fig\" 100) (\"honeydew\" 15)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// copy-hash-table deep independence test
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_hash_table_ops_ext_copy_independence() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Copy a hash table, then extensively mutate both original and copy
    // to verify they are fully independent at the key-value mapping level
    // (though values themselves are shared references for non-scalar types).
    let form = r#"(let ((orig (make-hash-table :test 'equal)))
                    ;; Populate with various value types
                    (puthash "int" 42 orig)
                    (puthash "str" "hello" orig)
                    (puthash "list" (list 1 2 3) orig)
                    (puthash "nested" (list (list 'a) (list 'b)) orig)
                    (puthash "sym" 'alpha orig)
                    (let ((copy (copy-hash-table orig)))
                      ;; Verify initial equality
                      (let ((initial-match
                             (and (equal (gethash "int" orig) (gethash "int" copy))
                                  (equal (gethash "str" orig) (gethash "str" copy))
                                  (equal (gethash "list" orig) (gethash "list" copy))
                                  (equal (gethash "sym" orig) (gethash "sym" copy)))))
                        ;; Mutate original: change values, add new keys, remove keys
                        (puthash "int" 999 orig)
                        (puthash "new-in-orig" 'orig-only orig)
                        (remhash "str" orig)
                        ;; Mutate copy: change values, add new keys, remove keys
                        (puthash "int" -1 copy)
                        (puthash "new-in-copy" 'copy-only copy)
                        (remhash "sym" copy)
                        ;; Mutate shared list through copy (shallow copy means shared structure)
                        (setcar (gethash "list" copy) 999)
                        (list
                         initial-match
                         ;; orig state
                         (gethash "int" orig)         ; 999
                         (gethash "str" orig)          ; nil (removed)
                         (gethash "new-in-orig" orig)  ; orig-only
                         (gethash "new-in-copy" orig)  ; nil
                         (gethash "sym" orig)           ; alpha
                         (hash-table-count orig)
                         ;; copy state
                         (gethash "int" copy)          ; -1
                         (gethash "str" copy)           ; "hello" (still there)
                         (gethash "new-in-orig" copy)  ; nil
                         (gethash "new-in-copy" copy)  ; copy-only
                         (gethash "sym" copy)           ; nil (removed)
                         (hash-table-count copy)
                         ;; Shared structure: mutating list through copy visible in orig
                         (gethash "list" orig)
                         (gethash "list" copy)
                         ;; Test function preserved
                         (eq (hash-table-test orig) (hash-table-test copy))))))"#;
    let expect = expect_test::expect![[
        r#""OK (t 999 nil orig-only nil alpha 5 -1 \"hello\" nil copy-only nil 5 (999 2 3) (999 2 3) t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Hash table as frequency counter with multi-pass analysis
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_hash_table_ops_ext_frequency_counter_multipass() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Count character frequencies in a string, then compute statistics:
    // most frequent, least frequent, unique count, histogram
    let form = r#"(let ((text "the quick brown fox jumps over the lazy dog and the fox")
                        (freq (make-hash-table :test 'eql)))
                    ;; Pass 1: Count character frequencies (skip spaces)
                    (let ((i 0) (len (length text)))
                      (while (< i len)
                        (let ((ch (aref text i)))
                          (unless (= ch ?\s)
                            (puthash ch (1+ (gethash ch freq 0)) freq)))
                        (setq i (1+ i))))
                    ;; Pass 2: Find max and min frequencies
                    (let ((max-freq 0) (max-chars nil)
                          (min-freq most-positive-fixnum) (min-chars nil)
                          (total-chars 0)
                          (unique-count 0))
                      (maphash (lambda (ch count)
                                 (setq total-chars (+ total-chars count)
                                       unique-count (1+ unique-count))
                                 (cond
                                  ((> count max-freq)
                                   (setq max-freq count max-chars (list ch)))
                                  ((= count max-freq)
                                   (setq max-chars (cons ch max-chars))))
                                 (cond
                                  ((< count min-freq)
                                   (setq min-freq count min-chars (list ch)))
                                  ((= count min-freq)
                                   (setq min-chars (cons ch min-chars)))))
                               freq)
                      ;; Pass 3: Build frequency histogram (freq -> count-of-chars-with-that-freq)
                      (let ((histogram (make-hash-table :test 'eql)))
                        (maphash (lambda (ch count)
                                   (puthash count (1+ (gethash count histogram 0)) histogram))
                                 freq)
                        (let ((hist-pairs nil))
                          (maphash (lambda (k v) (setq hist-pairs (cons (cons k v) hist-pairs)))
                                   histogram)
                          (list
                           unique-count
                           total-chars
                           max-freq
                           (sort (mapcar #'string max-chars) #'string<)
                           min-freq
                           (length min-chars)
                           (sort hist-pairs (lambda (a b) (< (car a) (car b)))))))))"#;
    let expect = expect_test::expect![[
        r#""OK (26 44 5 (\"o\") 1 15 ((1 . 15) (2 . 7) (3 . 2) (4 . 1) (5 . 1)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Hash table for memoization — fibonacci and recursive computation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_hash_table_ops_ext_memoization_fibonacci() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement memoized fibonacci using a hash table cache
    // Compare results and verify cache is populated correctly
    let form = r#"(progn
  (fset 'neovm--test-memo-fib
    (lambda (n cache)
      (or (gethash n cache)
          (let ((result
                 (cond
                  ((= n 0) 0)
                  ((= n 1) 1)
                  (t (+ (funcall 'neovm--test-memo-fib (- n 1) cache)
                        (funcall 'neovm--test-memo-fib (- n 2) cache))))))
            (puthash n result cache)
            result))))

  ;; Also implement a naive recursive factorial with memoization
  (fset 'neovm--test-memo-fact
    (lambda (n cache)
      (or (gethash n cache)
          (let ((result
                 (if (<= n 1) 1
                   (* n (funcall 'neovm--test-memo-fact (- n 1) cache)))))
            (puthash n result cache)
            result))))

  (unwind-protect
      (let ((fib-cache (make-hash-table :test 'eql))
            (fact-cache (make-hash-table :test 'eql)))
        ;; Compute fibonacci sequence 0..20
        (let ((fibs (let ((result nil) (i 0))
                      (while (<= i 20)
                        (setq result (cons (funcall 'neovm--test-memo-fib i fib-cache) result))
                        (setq i (1+ i)))
                      (nreverse result))))
          ;; Compute factorials 0..12
          (let ((facts (let ((result nil) (i 0))
                         (while (<= i 12)
                           (setq result (cons (funcall 'neovm--test-memo-fact i fact-cache) result))
                           (setq i (1+ i)))
                         (nreverse result))))
            (list
             ;; First 21 fibonacci numbers
             fibs
             ;; First 13 factorials
             facts
             ;; Cache sizes — should have entries for all computed values
             (hash-table-count fib-cache)
             (hash-table-count fact-cache)
             ;; Verify specific cached values
             (gethash 10 fib-cache)   ; fib(10) = 55
             (gethash 20 fib-cache)   ; fib(20) = 6765
             (gethash 5 fact-cache)   ; 5! = 120
             (gethash 10 fact-cache)  ; 10! = 3628800
             ;; Re-calling should return cached value (same result)
             (= (funcall 'neovm--test-memo-fib 15 fib-cache)
                (gethash 15 fib-cache))))))
    (fmakunbound 'neovm--test-memo-fib)
    (fmakunbound 'neovm--test-memo-fact)))"#;
    let expect = expect_test::expect![[
        r#""OK ((0 1 1 2 3 5 8 13 21 34 55 89 144 233 377 610 987 1597 2584 4181 6765) (1 1 2 6 24 120 720 5040 40320 362880 3628800 39916800 479001600) 21 13 55 6765 120 3628800 t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Hash table merge utility and conflict resolution
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_hash_table_ops_ext_merge_with_conflict_resolution() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Merge two hash tables with a user-supplied conflict resolution function
    let form = r#"(progn
  ;; merge-hash: merges src into dst, using resolve-fn for conflicts
  ;; resolve-fn takes (key dst-val src-val) and returns the value to keep
  (fset 'neovm--test-merge-hash
    (lambda (dst src resolve-fn)
      (maphash (lambda (k v)
                 (let ((existing (gethash k dst)))
                   (if existing
                       (puthash k (funcall resolve-fn k existing v) dst)
                     (puthash k v dst))))
               src)
      dst))

  (unwind-protect
      (let ((config-defaults (make-hash-table :test 'equal))
            (config-user (make-hash-table :test 'equal))
            (config-env (make-hash-table :test 'equal)))
        ;; Layer 1: defaults
        (dolist (pair '(("port" . 8080) ("host" . "localhost") ("debug" . nil)
                        ("timeout" . 30) ("retries" . 3) ("log-level" . "info")))
          (puthash (car pair) (cdr pair) config-defaults))
        ;; Layer 2: user overrides
        (dolist (pair '(("port" . 3000) ("debug" . t) ("log-level" . "debug")
                        ("theme" . "dark")))
          (puthash (car pair) (cdr pair) config-user))
        ;; Layer 3: environment overrides (highest priority)
        (dolist (pair '(("port" . 9090) ("host" . "0.0.0.0")
                        ("ssl" . t)))
          (puthash (car pair) (cdr pair) config-env))
        ;; Merge: defaults <- user (take user value on conflict)
        (let ((merged (copy-hash-table config-defaults)))
          (funcall 'neovm--test-merge-hash merged config-user
                   (lambda (k old new) new))
          ;; Merge: merged <- env (take env value on conflict)
          (funcall 'neovm--test-merge-hash merged config-env
                   (lambda (k old new) new))
          ;; Collect sorted final config
          (let ((final nil))
            (maphash (lambda (k v) (setq final (cons (cons k v) final))) merged)
            (let ((sorted (sort final (lambda (a b) (string< (car a) (car b))))))
              (list
               sorted
               (hash-table-count merged)
               ;; Verify priority: env > user > default
               (gethash "port" merged)      ; 9090 (env)
               (gethash "host" merged)      ; "0.0.0.0" (env)
               (gethash "debug" merged)     ; t (user)
               (gethash "timeout" merged)   ; 30 (default)
               (gethash "theme" merged)     ; "dark" (user)
               (gethash "ssl" merged))))))  ; t (env)
    (fmakunbound 'neovm--test-merge-hash)))"#;
    let expect = expect_test::expect![[
        r#""OK (((\"debug\" . t) (\"host\" . \"0.0.0.0\") (\"log-level\" . \"debug\") (\"port\" . 9090) (\"retries\" . 3) (\"ssl\" . t) (\"theme\" . \"dark\") (\"timeout\" . 30)) 8 9090 \"0.0.0.0\" t 30 \"dark\" t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Hash table: LRU cache simulation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_hash_table_ops_ext_lru_cache_simulation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate an LRU cache using a hash table + ordered access list.
    // On access: move to front. On insert when full: evict from tail.
    let form = r#"(let ((cache (make-hash-table :test 'equal))
                        (order nil)    ; list of keys, most recent first
                        (capacity 4)
                        (log nil))     ; track operations for verification
                    (let ((cache-get
                           (lambda (key)
                             (let ((val (gethash key cache)))
                               (when val
                                 ;; Move to front of order
                                 (setq order (cons key (delete key order))))
                               val)))
                          (cache-put
                           (lambda (key val)
                             ;; If key exists, just update and move to front
                             (if (gethash key cache)
                                 (progn
                                   (puthash key val cache)
                                   (setq order (cons key (delete key order))))
                               ;; New key: evict if at capacity
                               (when (>= (hash-table-count cache) capacity)
                                 (let ((victim (car (last order))))
                                   (setq log (cons (list 'evict victim) log))
                                   (remhash victim cache)
                                   (setq order (butlast order))))
                               (puthash key val cache)
                               (setq order (cons key order))))))
                      ;; Simulate access pattern
                      (funcall cache-put "a" 1)
                      (funcall cache-put "b" 2)
                      (funcall cache-put "c" 3)
                      (funcall cache-put "d" 4)
                      ;; Cache full: [d c b a]
                      ;; Access "a" — moves to front: [a d c b]
                      (funcall cache-get "a")
                      ;; Insert "e" — evicts "b" (tail): [e a d c]
                      (funcall cache-put "e" 5)
                      ;; Access "d" — moves to front: [d e a c]
                      (funcall cache-get "d")
                      ;; Insert "f" — evicts "c" (tail): [f d e a]
                      (funcall cache-put "f" 6)
                      (list
                       order
                       (hash-table-count cache)
                       ;; "b" and "c" should have been evicted
                       (gethash "a" cache)
                       (gethash "b" cache)
                       (gethash "c" cache)
                       (gethash "d" cache)
                       (gethash "e" cache)
                       (gethash "f" cache)
                       ;; Eviction log
                       (nreverse log))))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"f\" \"d\" \"e\" \"a\") 4 1 nil nil 4 5 6 ((evict \"b\") (evict \"c\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
