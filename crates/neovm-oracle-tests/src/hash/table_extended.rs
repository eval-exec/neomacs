//! Oracle parity tests for extended hash table operations:
//! keyword args, introspection, copy semantics, clrhash, maphash,
//! frequency counting, nested hash tables, and alist interconversion.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// make-hash-table with all keyword args + introspection
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_hash_table_full_keyword_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Create hash tables with different :test values and verify introspection
    let form = r#"(let ((h-eq (make-hash-table :test 'eq :size 16))
                        (h-eql (make-hash-table :test 'eql :size 32))
                        (h-equal (make-hash-table :test 'equal
                                                  :size 64
                                                  :rehash-size 2.0
                                                  :rehash-threshold 0.8)))
                    (puthash 'a 1 h-eq)
                    (puthash 'b 2 h-eq)
                    (puthash 42 'answer h-eql)
                    (puthash 42.0 'float-answer h-eql)
                    (puthash "hello" 'world h-equal)
                    (puthash '(1 2 3) 'triple h-equal)
                    (list
                     (hash-table-test h-eq)
                     (hash-table-count h-eq)
                     (hash-table-test h-eql)
                     (hash-table-count h-eql)
                     ;; eql distinguishes 42 from 42.0
                     (gethash 42 h-eql)
                     (gethash 42.0 h-eql)
                     (hash-table-test h-equal)
                     (hash-table-count h-equal)
                     (gethash "hello" h-equal)
                     (gethash '(1 2 3) h-equal)))"#;
    let expect =
        expect_test::expect![[r#""OK (eq 2 eql 2 answer float-answer equal 2 world triple)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// hash-table-count, hash-table-size, hash-table-test after mutations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_hash_table_count_size_test_after_mutations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Track count through insertions, updates, and deletions
    let form = r#"(let ((h (make-hash-table :test 'equal)))
                    (let ((counts nil))
                      ;; Phase 1: insert 20 items
                      (dotimes (i 20)
                        (puthash (format "key-%d" i) (* i 10) h))
                      (setq counts (cons (hash-table-count h) counts))
                      ;; Phase 2: overwrite 10 items (count should not change)
                      (dotimes (i 10)
                        (puthash (format "key-%d" i) (* i 100) h))
                      (setq counts (cons (hash-table-count h) counts))
                      ;; Phase 3: remove 5 items
                      (dotimes (i 5)
                        (remhash (format "key-%d" i) h))
                      (setq counts (cons (hash-table-count h) counts))
                      ;; Phase 4: remove nonexistent keys (count unchanged)
                      (remhash "nonexistent-1" h)
                      (remhash "nonexistent-2" h)
                      (setq counts (cons (hash-table-count h) counts))
                      ;; Verify some values
                      (list (nreverse counts)
                            (hash-table-test h)
                            (gethash "key-0" h)    ; was removed
                            (gethash "key-5" h)    ; was overwritten
                            (gethash "key-15" h))) ; original value
                  )"#;
    let expect = expect_test::expect![[r#""OK ((20 20 15 15) equal nil 500 150)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// copy-hash-table: shallow copy semantics
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_hash_table_copy_shallow_semantics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Verify copy-hash-table creates an independent table that shares structure
    let form = r#"(let ((h1 (make-hash-table :test 'equal)))
                    (puthash "nums" (list 1 2 3) h1)
                    (puthash "strs" (list "a" "b" "c") h1)
                    (puthash "nested" (list (list 'x 'y) (list 'z)) h1)
                    (let ((h2 (copy-hash-table h1)))
                      ;; Modify h2 only
                      (puthash "extra" 'only-in-h2 h2)
                      (remhash "strs" h2)
                      ;; Mutate a shared list through h1
                      (setcar (gethash "nums" h1) 999)
                      (list
                       ;; h1 unchanged structurally
                       (hash-table-count h1)
                       (hash-table-count h2)
                       ;; "extra" only in h2
                       (gethash "extra" h1)
                       (gethash "extra" h2)
                       ;; "strs" removed from h2 only
                       (gethash "strs" h1)
                       (gethash "strs" h2)
                       ;; Shared list mutation visible in both (shallow copy)
                       (gethash "nums" h1)
                       (gethash "nums" h2)
                       ;; Tests are preserved
                       (eq (hash-table-test h1) (hash-table-test h2)))))"#;
    let expect = expect_test::expect![[
        r#""OK (3 3 nil only-in-h2 (\"a\" \"b\" \"c\") nil (999 2 3) (999 2 3) t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// clrhash and verify emptied
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_hash_table_clrhash_verify_empty() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Fill a table, clear it, verify it's empty, then refill
    let form = r#"(let ((h (make-hash-table :test 'equal)))
                    ;; Fill
                    (dotimes (i 50)
                      (puthash i (format "val-%d" i) h))
                    (let ((before-count (hash-table-count h))
                          (before-val (gethash 25 h)))
                      ;; Clear
                      (clrhash h)
                      (let ((after-count (hash-table-count h))
                            (after-val (gethash 25 h)))
                        ;; Verify maphash yields nothing
                        (let ((mapped nil))
                          (maphash (lambda (k v) (setq mapped t)) h)
                          ;; Refill with different data
                          (puthash 'new-key 'new-val h)
                          (list before-count before-val
                                after-count after-val
                                mapped
                                (hash-table-count h)
                                (gethash 'new-key h))))))"#;
    let expect = expect_test::expect![[r#""OK (50 \"val-25\" 0 nil nil 1 new-val)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// maphash: building a new hash from an old one (invert + filter)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_hash_table_maphash_transform() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a grade table, then use maphash to create a filtered inverted index
    let form = r#"(let ((grades (make-hash-table :test 'equal))
                        (by-grade (make-hash-table :test 'eq)))
                    ;; Populate student grades
                    (dolist (entry '(("Alice" . 95)
                                     ("Bob" . 82)
                                     ("Carol" . 95)
                                     ("Dave" . 71)
                                     ("Eve" . 88)
                                     ("Frank" . 82)
                                     ("Grace" . 95)
                                     ("Heidi" . 60)))
                      (puthash (car entry) (cdr entry) grades))
                    ;; Group students by grade level (A/B/C/D)
                    (maphash (lambda (name score)
                               (let ((grade (cond ((>= score 90) 'A)
                                                  ((>= score 80) 'B)
                                                  ((>= score 70) 'C)
                                                  (t 'D))))
                                 (puthash grade
                                          (cons name (gethash grade by-grade nil))
                                          by-grade)))
                             grades)
                    ;; Sort each group and collect
                    (let ((result nil))
                      (dolist (g '(A B C D))
                        (let ((students (gethash g by-grade)))
                          (when students
                            (setq result (cons (cons g (sort (copy-sequence students)
                                                            #'string<))
                                               result)))))
                      (nreverse result)))"#;
    let expect = expect_test::expect![[
        r#""OK ((A \"Alice\" \"Carol\" \"Grace\") (B \"Bob\" \"Eve\" \"Frank\") (C \"Dave\") (D \"Heidi\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Hash table as frequency counter with complex keys
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_hash_table_frequency_counter_complex_keys() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Count frequency of bigrams (pairs of consecutive elements) in a sequence
    let form = r#"(let ((seq '(a b c a b d a b c a b c d e))
                        (bigram-freq (make-hash-table :test 'equal)))
                    ;; Count bigrams
                    (let ((prev nil))
                      (dolist (item seq)
                        (when prev
                          (let ((bigram (list prev item)))
                            (puthash bigram (1+ (gethash bigram bigram-freq 0))
                                     bigram-freq)))
                        (setq prev item)))
                    ;; Collect and sort by frequency descending, then alphabetically
                    (let ((pairs nil))
                      (maphash (lambda (k v)
                                 (setq pairs (cons (cons k v) pairs)))
                               bigram-freq)
                      (sort pairs
                            (lambda (a b)
                              (or (> (cdr a) (cdr b))
                                  (and (= (cdr a) (cdr b))
                                       (string< (format "%S" (car a))
                                                (format "%S" (car b)))))))))"#;
    let expect = expect_test::expect![[
        r#""OK (((a b) . 4) ((b c) . 3) ((c a) . 2) ((b d) . 1) ((c d) . 1) ((d a) . 1) ((d e) . 1))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// User-defined hash-table tests: GNU mutability guard semantics
// ---------------------------------------------------------------------------

#[test]
fn oracle_user_hash_function_cannot_mutate_same_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU fns.c marks the table immutable while a user hash function runs.
    // Mutating that same table from the hash function must signal an error.
    let form = r#"
(let ((h nil)
      (log nil))
  (define-hash-table-test
   'neomacs-oracle-mutating-hash
   (lambda (a b) (equal a b))
   (lambda (key)
     (setq log (cons key log))
     (puthash 'inside 'bad h)
     0))
  (setq h (make-hash-table :test 'neomacs-oracle-mutating-hash))
  (list
   (condition-case err
       (puthash 'outer 'value h)
     (error (list (car err) (cadr err))))
   (hash-table-count h)
   (nreverse log)))
"#;

    let expect =
        expect_test::expect![[r#""OK ((error \"hash table test modifies table\") 0 (outer))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_user_hash_function_cannot_mutate_same_table_during_gethash() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU Emacs protects user hash callbacks during lookup as well as
    // insertion.  A `gethash` callback that mutates the same table signals
    // immediately instead of re-entering the table operation.
    let form = r#"
(let ((h nil)
      (log nil))
  (define-hash-table-test
   'neomacs-oracle-mutating-gethash-hash
   (lambda (a b) (equal a b))
   (lambda (key)
     (setq log (cons key log))
     (puthash 'inside 'bad h)
     0))
  (setq h (make-hash-table :test 'neomacs-oracle-mutating-gethash-hash))
  (condition-case nil
      (puthash 'seed 'value h)
    (error nil))
  (setq log nil)
  (let ((result
         (condition-case err
             (gethash 'seed h 'missing)
           (error (list (car err) (cadr err))))))
    (list result
          (hash-table-count h)
          (length log)
          (car log))))
"#;

    let expect =
        expect_test::expect![[r#""OK ((error \"hash table test modifies table\") 0 1 seed)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_user_compare_function_cannot_mutate_same_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A second insertion with the same user hash reaches the comparison
    // function; GNU protects the table for that callback too.
    let form = r#"
(let ((h nil)
      (log nil))
  (define-hash-table-test
   'neomacs-oracle-mutating-compare
   (lambda (a b)
     (setq log (cons (list a b) log))
     (remhash a h)
     (equal a b))
   (lambda (key) 0))
  (setq h (make-hash-table :test 'neomacs-oracle-mutating-compare))
  (puthash 'first 'one h)
  (list
   (condition-case err
       (puthash 'second 'two h)
     (error (list (car err) (cadr err))))
   (hash-table-count h)
   (gethash 'first h 'missing)
   (gethash 'second h 'missing)
   (nreverse log)))
"#;

    let expect = expect_test::expect![[
        r#""ERR (error \"hash table test modifies table\" #s(hash-table test neomacs-oracle-mutating-compare data (first one)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Nested hash tables (hash of hashes)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_hash_table_nested() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Two-level hash: department -> (employee -> salary)
    let form = r#"(let ((company (make-hash-table :test 'equal)))
                    ;; Build nested structure
                    (dolist (record '(("Engineering" "Alice" 120000)
                                      ("Engineering" "Bob" 110000)
                                      ("Engineering" "Carol" 130000)
                                      ("Marketing" "Dave" 90000)
                                      ("Marketing" "Eve" 95000)
                                      ("Sales" "Frank" 85000)
                                      ("Sales" "Grace" 88000)
                                      ("Sales" "Heidi" 92000)))
                      (let* ((dept (nth 0 record))
                             (name (nth 1 record))
                             (salary (nth 2 record))
                             (dept-table (or (gethash dept company)
                                             (let ((new-ht (make-hash-table :test 'equal)))
                                               (puthash dept new-ht company)
                                               new-ht))))
                        (puthash name salary dept-table)))
                    ;; Compute department stats
                    (let ((stats nil))
                      (maphash
                       (lambda (dept dept-table)
                         (let ((total 0) (count 0) (max-sal 0) (max-name nil))
                           (maphash
                            (lambda (name salary)
                              (setq total (+ total salary)
                                    count (1+ count))
                              (when (> salary max-sal)
                                (setq max-sal salary
                                      max-name name)))
                            dept-table)
                           (setq stats (cons (list dept count total
                                                   (/ total count)
                                                   max-name max-sal)
                                             stats))))
                       company)
                      (sort stats (lambda (a b) (string< (car a) (car b))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"Engineering\" 3 360000 120000 \"Carol\" 130000) (\"Marketing\" 2 185000 92500 \"Eve\" 95000) (\"Sales\" 3 265000 88333 \"Heidi\" 92000))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Hash table <-> alist interconversion
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_hash_table_alist_interconversion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Convert alist -> hash -> modified hash -> alist, verifying round-trip semantics
    let form = r#"(let ((original-alist '((name . "test-project")
                                           (version . "1.2.3")
                                           (deps . 5)
                                           (active . t)
                                           (score . 42))))
                    ;; alist -> hash
                    (let ((h (make-hash-table :test 'equal)))
                      (dolist (pair original-alist)
                        (puthash (symbol-name (car pair)) (cdr pair) h))
                      ;; Modify some entries
                      (puthash "version" "2.0.0" h)
                      (puthash "deps" (1+ (gethash "deps" h)) h)
                      (puthash "new-field" 'added h)
                      (remhash "active" h)
                      ;; hash -> sorted alist
                      (let ((result-alist nil))
                        (maphash (lambda (k v)
                                   (setq result-alist
                                         (cons (cons k v) result-alist)))
                                 h)
                        (let ((sorted (sort result-alist
                                           (lambda (a b) (string< (car a) (car b))))))
                          ;; Verify specific properties
                          (list (length sorted)
                                (assoc "version" sorted)
                                (assoc "deps" sorted)
                                (assoc "active" sorted)
                                (assoc "new-field" sorted)
                                ;; Reconstruct a second hash from the alist and compare count
                                (let ((h2 (make-hash-table :test 'equal)))
                                  (dolist (pair sorted)
                                    (puthash (car pair) (cdr pair) h2))
                                  (= (hash-table-count h) (hash-table-count h2))))))))"#;
    let expect = expect_test::expect![[
        r#""OK (5 (\"version\" . \"2.0.0\") (\"deps\" . 6) nil (\"new-field\" . added) t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Hash table: set operations (union, intersection, difference)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_hash_table_set_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use hash tables to implement set operations on lists
    let form = r#"(let ((list-a '(1 2 3 4 5 6 7 8))
                        (list-b '(5 6 7 8 9 10 11 12)))
                    ;; Build sets
                    (let ((set-a (make-hash-table :test 'eql))
                          (set-b (make-hash-table :test 'eql)))
                      (dolist (x list-a) (puthash x t set-a))
                      (dolist (x list-b) (puthash x t set-b))
                      ;; Intersection
                      (let ((inter nil))
                        (maphash (lambda (k v)
                                   (when (gethash k set-b)
                                     (setq inter (cons k inter))))
                                 set-a)
                        ;; Union
                        (let ((union-set (make-hash-table :test 'eql)))
                          (maphash (lambda (k v) (puthash k t union-set)) set-a)
                          (maphash (lambda (k v) (puthash k t union-set)) set-b)
                          (let ((union-list nil))
                            (maphash (lambda (k v) (setq union-list (cons k union-list)))
                                     union-set)
                            ;; Difference A - B
                            (let ((diff nil))
                              (maphash (lambda (k v)
                                         (unless (gethash k set-b)
                                           (setq diff (cons k diff))))
                                       set-a)
                              (list (sort inter #'<)
                                    (sort union-list #'<)
                                    (sort diff #'<))))))))"#;
    let expect =
        expect_test::expect![[r#""OK ((5 6 7 8) (1 2 3 4 5 6 7 8 9 10 11 12) (1 2 3 4))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
