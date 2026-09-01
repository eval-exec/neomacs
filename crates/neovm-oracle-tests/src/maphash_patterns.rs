//! Oracle parity tests for `maphash` and related hash-table iteration patterns:
//! maphash with lambda, collecting keys/values, counting with predicates,
//! hash table inversion, merging with conflict resolution, grouping entries,
//! and nested maphash operations.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// maphash with inline lambda collecting keys and values
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_maphash_collect_keys_values() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // maphash iterates over all entries, collecting keys and values into
    // separate sorted lists, and also computing aggregate statistics.
    let form = r#"(let ((h (make-hash-table :test 'equal)))
                    (puthash "alpha" 100 h)
                    (puthash "beta" 200 h)
                    (puthash "gamma" 50 h)
                    (puthash "delta" 300 h)
                    (puthash "epsilon" 150 h)
                    (let ((keys nil)
                          (vals nil)
                          (count 0)
                          (total 0)
                          (max-val -1)
                          (max-key nil))
                      (maphash (lambda (k v)
                                 (setq keys (cons k keys))
                                 (setq vals (cons v vals))
                                 (setq count (1+ count))
                                 (setq total (+ total v))
                                 (when (> v max-val)
                                   (setq max-val v)
                                   (setq max-key k)))
                               h)
                      (list (sort keys #'string<)
                            (sort vals #'<)
                            count
                            total
                            max-key
                            max-val)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"alpha\" \"beta\" \"delta\" \"epsilon\" \"gamma\") (50 100 150 200 300) 5 800 \"delta\" 300)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// maphash return value is always nil
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_maphash_return_value() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // maphash always returns nil regardless of hash table content or function body
    let form = r#"(let ((h1 (make-hash-table))
                        (h2 (make-hash-table)))
                    (puthash 'a 1 h1)
                    (puthash 'b 2 h1)
                    (list
                      ;; maphash on non-empty hash returns nil
                      (maphash (lambda (k v) (+ k v)) h1)
                      ;; maphash on empty hash returns nil
                      (maphash (lambda (k v) nil) h2)
                      ;; maphash with side-effecting lambda still returns nil
                      (let ((acc 0))
                        (list (maphash (lambda (k v) (setq acc (+ acc v))) h1)
                              acc))))"#;
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument number-or-marker-p a)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Counting entries matching predicates via maphash
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_maphash_count_predicates() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use maphash to count entries matching various predicates:
    // even values, keys starting with certain prefix, values above threshold
    let form = r#"(let ((h (make-hash-table :test 'equal)))
                    (puthash "user-alice" 25 h)
                    (puthash "user-bob" 30 h)
                    (puthash "admin-carol" 35 h)
                    (puthash "user-dave" 22 h)
                    (puthash "admin-eve" 28 h)
                    (puthash "user-frank" 40 h)
                    (puthash "admin-grace" 33 h)
                    (let ((even-count 0)
                          (user-count 0)
                          (admin-count 0)
                          (above-30 0)
                          (user-total-age 0)
                          (admin-total-age 0))
                      (maphash (lambda (k v)
                                 (when (= (% v 2) 0)
                                   (setq even-count (1+ even-count)))
                                 (when (string-prefix-p "user-" k)
                                   (setq user-count (1+ user-count))
                                   (setq user-total-age (+ user-total-age v)))
                                 (when (string-prefix-p "admin-" k)
                                   (setq admin-count (1+ admin-count))
                                   (setq admin-total-age (+ admin-total-age v)))
                                 (when (> v 30)
                                   (setq above-30 (1+ above-30))))
                               h)
                      (list even-count
                            user-count admin-count
                            above-30
                            user-total-age admin-total-age)))"#;
    let expect = expect_test::expect![[r#""OK (4 4 3 3 117 96)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Hash table inversion: swap keys and values
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_maphash_invert_hash_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build an inverted hash table where original values become keys
    // and original keys become values. Also handle the case where
    // multiple keys map to the same value (collect into list).
    let form = r#"(progn
  ;; Simple inversion (bijective mapping)
  (let ((h (make-hash-table))
        (inv (make-hash-table)))
    (puthash 'a 1 h)
    (puthash 'b 2 h)
    (puthash 'c 3 h)
    (puthash 'd 4 h)
    (maphash (lambda (k v) (puthash v k inv)) h)
    (let ((result1 (list (gethash 1 inv) (gethash 2 inv)
                         (gethash 3 inv) (gethash 4 inv)
                         (gethash 5 inv))))
      ;; Multi-value inversion: collect keys with same value into lists
      (let ((h2 (make-hash-table))
            (inv2 (make-hash-table)))
        (puthash 'x 10 h2)
        (puthash 'y 20 h2)
        (puthash 'z 10 h2)
        (puthash 'w 20 h2)
        (puthash 'v 30 h2)
        (maphash (lambda (k v)
                   (puthash v (cons k (gethash v inv2 nil)) inv2))
                 h2)
        (list result1
              (sort (gethash 10 inv2) #'string<)
              (sort (gethash 20 inv2) #'string<)
              (gethash 30 inv2)
              (hash-table-count inv2))))))"#;
    let expect = expect_test::expect![[r#""OK ((a b c d nil) (x z) (w y) (v) 3)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Hash table merge with conflict resolution
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_maphash_merge_with_conflict_resolution() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Merge two hash tables with different conflict resolution strategies:
    // keep-first, keep-second, sum, max
    let form = r#"(progn
  (fset 'neovm--ht-merge
    (lambda (h1 h2 resolve)
      "Merge H2 into a copy of H1, using RESOLVE for conflicts.
RESOLVE is a function (lambda (key val1 val2) -> merged-val)."
      (let ((result (copy-hash-table h1)))
        (maphash (lambda (k v2)
                   (let ((v1 (gethash k result)))
                     (if v1
                         (puthash k (funcall resolve k v1 v2) result)
                       (puthash k v2 result))))
                 h2)
        result)))

  (unwind-protect
      (let ((h1 (make-hash-table))
            (h2 (make-hash-table)))
        (puthash 'a 10 h1) (puthash 'b 20 h1) (puthash 'c 30 h1)
        (puthash 'b 25 h2) (puthash 'c 35 h2) (puthash 'd 40 h2)

        ;; Merge with keep-first (h1 wins)
        (let ((m1 (funcall 'neovm--ht-merge h1 h2
                           (lambda (k v1 v2) v1))))
          ;; Merge with keep-second (h2 wins)
          (let ((m2 (funcall 'neovm--ht-merge h1 h2
                             (lambda (k v1 v2) v2))))
            ;; Merge with sum
            (let ((m3 (funcall 'neovm--ht-merge h1 h2
                               (lambda (k v1 v2) (+ v1 v2)))))
              ;; Merge with max
              (let ((m4 (funcall 'neovm--ht-merge h1 h2
                                 (lambda (k v1 v2) (max v1 v2)))))
                (list
                  ;; keep-first results
                  (list (gethash 'a m1) (gethash 'b m1)
                        (gethash 'c m1) (gethash 'd m1))
                  ;; keep-second results
                  (list (gethash 'a m2) (gethash 'b m2)
                        (gethash 'c m2) (gethash 'd m2))
                  ;; sum results
                  (list (gethash 'a m3) (gethash 'b m3)
                        (gethash 'c m3) (gethash 'd m3))
                  ;; max results
                  (list (gethash 'a m4) (gethash 'b m4)
                        (gethash 'c m4) (gethash 'd m4))
                  ;; all merged tables have 4 entries
                  (list (hash-table-count m1) (hash-table-count m2)
                        (hash-table-count m3) (hash-table-count m4))))))))
    (fmakunbound 'neovm--ht-merge)))"#;
    let expect = expect_test::expect![[
        r#""OK ((10 20 30 40) (10 25 35 40) (10 45 65 40) (10 25 35 40) (4 4 4 4))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Grouping hash table entries by computed key
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_maphash_group_entries() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Group hash table entries by a derived key (e.g., value range buckets)
    let form = r#"(let ((scores (make-hash-table)))
                    (puthash 'alice 92 scores)
                    (puthash 'bob 85 scores)
                    (puthash 'carol 78 scores)
                    (puthash 'dave 95 scores)
                    (puthash 'eve 63 scores)
                    (puthash 'frank 71 scores)
                    (puthash 'grace 88 scores)
                    (puthash 'hank 55 scores)
                    ;; Group by grade: A(90-100) B(80-89) C(70-79) D(60-69) F(<60)
                    (let ((groups (make-hash-table)))
                      (maphash (lambda (name score)
                                 (let ((grade (cond ((>= score 90) 'A)
                                                    ((>= score 80) 'B)
                                                    ((>= score 70) 'C)
                                                    ((>= score 60) 'D)
                                                    (t 'F))))
                                   (puthash grade
                                            (cons name (gethash grade groups nil))
                                            groups)))
                               scores)
                      ;; Sort each group and collect results
                      (list
                        (sort (gethash 'A groups nil) #'string<)
                        (sort (gethash 'B groups nil) #'string<)
                        (sort (gethash 'C groups nil) #'string<)
                        (sort (gethash 'D groups nil) #'string<)
                        (sort (gethash 'F groups nil) #'string<)
                        ;; Count per group
                        (let ((counts nil))
                          (maphash (lambda (grade students)
                                     (setq counts
                                           (cons (cons grade (length students))
                                                 counts)))
                                   groups)
                          (sort counts (lambda (a b)
                                         (string< (symbol-name (car a))
                                                  (symbol-name (car b)))))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((alice dave) (bob grace) (carol frank) (eve) (hank) ((A . 2) (B . 2) (C . 2) (D . 1) (F . 1)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// maphash with nested hash tables (hash of hashes)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_maphash_nested_hash_tables() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a hash of hashes (department -> (employee -> salary)),
    // then use nested maphash to compute department-level and
    // company-level statistics.
    let form = r#"(let ((company (make-hash-table)))
                    ;; Build department hash tables
                    (let ((eng (make-hash-table))
                          (sales (make-hash-table))
                          (hr (make-hash-table)))
                      (puthash 'alice 120 eng)
                      (puthash 'bob 110 eng)
                      (puthash 'carol 130 eng)
                      (puthash 'dave 95 sales)
                      (puthash 'eve 100 sales)
                      (puthash 'frank 85 hr)
                      (puthash 'grace 90 hr)
                      (puthash 'eng eng company)
                      (puthash 'sales sales company)
                      (puthash 'hr hr company))
                    ;; Compute per-department stats and company total
                    (let ((dept-stats nil)
                          (company-total 0)
                          (company-count 0)
                          (all-employees nil))
                      (maphash
                       (lambda (dept dept-ht)
                         (let ((dept-total 0) (dept-count 0)
                               (dept-employees nil))
                           (maphash
                            (lambda (emp salary)
                              (setq dept-total (+ dept-total salary))
                              (setq dept-count (1+ dept-count))
                              (setq dept-employees (cons emp dept-employees))
                              (setq all-employees (cons emp all-employees))
                              (setq company-total (+ company-total salary))
                              (setq company-count (1+ company-count)))
                            dept-ht)
                           (setq dept-stats
                                 (cons (list dept dept-count dept-total
                                             (sort dept-employees #'string<))
                                       dept-stats))))
                       company)
                      (list
                        (sort dept-stats
                              (lambda (a b) (string< (symbol-name (car a))
                                                     (symbol-name (car b)))))
                        company-total
                        company-count
                        (sort all-employees #'string<))))"#;
    let expect = expect_test::expect![[
        r#""OK (((eng 3 360 (alice bob carol)) (hr 2 175 (frank grace)) (sales 2 195 (dave eve))) 730 7 (alice bob carol dave eve frank grace))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// maphash to build a histogram and find mode/median
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_maphash_histogram_statistics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a frequency histogram from a list, then use maphash to find
    // the mode (most frequent value) and compute distribution statistics.
    let form = r#"(let ((data '(3 1 4 1 5 9 2 6 5 3 5 8 9 7 9 3 2 3 8 4))
                        (freq (make-hash-table)))
                    ;; Build frequency table
                    (dolist (x data)
                      (puthash x (1+ (gethash x freq 0)) freq))
                    ;; Find mode, max frequency, and collect distribution
                    (let ((mode nil)
                          (mode-count 0)
                          (unique-count 0)
                          (pairs nil)
                          (total-entries 0))
                      (maphash (lambda (val count)
                                 (setq unique-count (1+ unique-count))
                                 (setq total-entries (+ total-entries count))
                                 (setq pairs (cons (cons val count) pairs))
                                 (when (> count mode-count)
                                   (setq mode val)
                                   (setq mode-count count)))
                               freq)
                      ;; Sort pairs by value for deterministic output
                      (setq pairs (sort pairs (lambda (a b) (< (car a) (car b)))))
                      ;; Find values that appear exactly once
                      (let ((singletons nil))
                        (maphash (lambda (val count)
                                   (when (= count 1)
                                     (setq singletons (cons val singletons))))
                                 freq)
                        (list pairs
                              mode mode-count
                              unique-count total-entries
                              (sort singletons #'<)))))"#;
    let expect = expect_test::expect![[
        r#""OK (((1 . 2) (2 . 2) (3 . 4) (4 . 2) (5 . 3) (6 . 1) (7 . 1) (8 . 2) (9 . 3)) 3 4 9 20 (6 7))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// maphash with hash table used as a set (all values t)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_maphash_set_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use hash tables as sets and implement set operations via maphash:
    // union, intersection, difference, symmetric-difference
    let form = r#"(progn
  (fset 'neovm--set-from-list
    (lambda (lst)
      (let ((s (make-hash-table)))
        (dolist (x lst) (puthash x t s))
        s)))

  (fset 'neovm--set-to-sorted-list
    (lambda (s)
      (let ((result nil))
        (maphash (lambda (k v) (setq result (cons k result))) s)
        (sort result #'<))))

  (fset 'neovm--set-union
    (lambda (s1 s2)
      (let ((result (copy-hash-table s1)))
        (maphash (lambda (k v) (puthash k t result)) s2)
        result)))

  (fset 'neovm--set-intersection
    (lambda (s1 s2)
      (let ((result (make-hash-table)))
        (maphash (lambda (k v)
                   (when (gethash k s2)
                     (puthash k t result)))
                 s1)
        result)))

  (fset 'neovm--set-difference
    (lambda (s1 s2)
      (let ((result (make-hash-table)))
        (maphash (lambda (k v)
                   (unless (gethash k s2)
                     (puthash k t result)))
                 s1)
        result)))

  (fset 'neovm--set-symmetric-difference
    (lambda (s1 s2)
      (let ((result (make-hash-table)))
        (maphash (lambda (k v)
                   (unless (gethash k s2)
                     (puthash k t result)))
                 s1)
        (maphash (lambda (k v)
                   (unless (gethash k s1)
                     (puthash k t result)))
                 s2)
        result)))

  (unwind-protect
      (let ((a (funcall 'neovm--set-from-list '(1 2 3 4 5)))
            (b (funcall 'neovm--set-from-list '(3 4 5 6 7))))
        (list
          ;; Union: {1,2,3,4,5,6,7}
          (funcall 'neovm--set-to-sorted-list
                   (funcall 'neovm--set-union a b))
          ;; Intersection: {3,4,5}
          (funcall 'neovm--set-to-sorted-list
                   (funcall 'neovm--set-intersection a b))
          ;; Difference A-B: {1,2}
          (funcall 'neovm--set-to-sorted-list
                   (funcall 'neovm--set-difference a b))
          ;; Difference B-A: {6,7}
          (funcall 'neovm--set-to-sorted-list
                   (funcall 'neovm--set-difference b a))
          ;; Symmetric difference: {1,2,6,7}
          (funcall 'neovm--set-to-sorted-list
                   (funcall 'neovm--set-symmetric-difference a b))
          ;; Edge case: empty set operations
          (let ((empty (funcall 'neovm--set-from-list nil)))
            (list
              (funcall 'neovm--set-to-sorted-list
                       (funcall 'neovm--set-union a empty))
              (funcall 'neovm--set-to-sorted-list
                       (funcall 'neovm--set-intersection a empty))
              (funcall 'neovm--set-to-sorted-list
                       (funcall 'neovm--set-difference a empty))))))
    (fmakunbound 'neovm--set-from-list)
    (fmakunbound 'neovm--set-to-sorted-list)
    (fmakunbound 'neovm--set-union)
    (fmakunbound 'neovm--set-intersection)
    (fmakunbound 'neovm--set-difference)
    (fmakunbound 'neovm--set-symmetric-difference)))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 2 3 4 5 6 7) (3 4 5) (1 2) (6 7) (1 2 6 7) ((1 2 3 4 5) nil (1 2 3 4 5)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
