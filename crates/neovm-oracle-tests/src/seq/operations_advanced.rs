//! Advanced oracle parity tests for `seq.el` operations:
//! seq-mapn, seq-group-by, seq-sort-by, seq-min/seq-max,
//! seq-partition, seq-subseq, complex data analysis pipelines,
//! and set operations with custom equality.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// seq-mapn: map over multiple sequences simultaneously
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_seq_mapn_multiple_sequences() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // seq-mapn maps a function over corresponding elements of N sequences.
    // Stops at the shortest sequence.
    let form = r#"(progn (require (quote cl-lib)) (list
                    ;; Two lists, element-wise addition
                    (seq-mapn #'+ '(1 2 3 4) '(10 20 30 40))
                    ;; Three lists, build triples
                    (seq-mapn #'list '(a b c) '(1 2 3) '(x y z))
                    ;; Mismatched lengths: stops at shortest
                    (seq-mapn #'cons '(a b c d e) '(1 2 3))
                    ;; With vector and list
                    (seq-mapn #'* [2 3 4] '(10 20 30))
                    ;; Single sequence degenerates to seq-map
                    (seq-mapn #'1+ '(5 6 7))))"#;
    let expect = expect_test::expect![[
        r#""OK ((11 22 33 44) ((a 1 x) (b 2 y) (c 3 z)) ((a . 1) (b . 2) (c . 3)) (20 60 120) (6 7 8))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_seq_mapn_truncation_strings_and_error_contracts() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU seq-mapn first converts every input with seq-into 'list, then maps
    // until any converted sequence is nil.  This pins truncation, string char
    // handling, single-sequence behavior, call order, and non-sequence errors.
    let form = r#"(progn
  (require 'seq)
  (list
    ;; Mapping stops at the shortest sequence across list/vector/list inputs.
    (seq-mapn #'list '(a b c d) '(1 2) [x y z])
    ;; Strings contribute character codes.
    (seq-mapn (lambda (c n) (list c n)) "abc" '(10 20 30 40))
    ;; A single sequence degenerates to seq-map-like behavior.
    (seq-mapn (lambda (x) (* x 2)) '(1 2 3))
    ;; Empty leading sequence stops immediately.
    (seq-mapn #'list nil '(1 2))
    ;; FUNCTION receives arguments in sequence order for each row.
    (let ((calls nil))
      (list
        (seq-mapn (lambda (&rest args)
                    (push args calls)
                    args)
                  '(a b) '(1 2 3))
        calls))
    ;; Non-sequence input signals through seq-into.
    (condition-case err
        (seq-mapn #'list 42 '(1 2))
      (error (list (car err) (cadr err))))))"#;
    let expect = expect_test::expect![[
        r#""OK (((a 1 x) (b 2 y)) ((97 10) (98 20) (99 30)) (2 4 6) nil (((a 1) (b 2)) ((b 2) (a 1))) (wrong-type-argument sequencep))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// seq-group-by: group elements by a classifier function
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_seq_group_by() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // seq-group-by returns an alist of (key . elements).
    // We sort the result for deterministic comparison.
    let form = r#"((require (quote cl-lib)) let ((result (seq-group-by #'cl-evenp '(1 2 3 4 5 6 7 8))))
                    ;; Sort by key for determinism
                    (sort result (lambda (a b)
                                   (and (null (car a)) (car b)))))"#;
    let expect = expect_test::expect![[r#""ERR (invalid-function (require 'cl-lib))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_seq_group_by_complex_key() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Group words by their length
    let form = r#"((require (quote cl-lib)) let ((words '("a" "bb" "ccc" "dd" "e" "fff" "gg")))
                    (let ((groups (seq-group-by #'length words)))
                      ;; Sort by key (length) for determinism
                      (sort groups (lambda (a b) (< (car a) (car b))))))"#;
    let expect = expect_test::expect![[r#""ERR (invalid-function (require 'cl-lib))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// seq-sort-by: sort by key extraction function
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_seq_sort_by() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // seq-sort-by sorts a sequence by comparing extracted keys.
    let form = r#"(progn (require (quote cl-lib)) (list
                    ;; Sort strings by length
                    (seq-sort-by #'length #'<
                                 '("banana" "fig" "apple" "kiwi" "elderberry"))
                    ;; Sort alist entries by cdr value
                    (seq-sort-by #'cdr #'<
                                 '((alice . 30) (bob . 25) (carol . 35) (dave . 28)))
                    ;; Sort numbers by absolute value
                    (seq-sort-by #'abs #'< '(-5 3 -1 4 -2 0))))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"fig\" \"kiwi\" \"apple\" \"banana\" \"elderberry\") ((bob . 25) (dave . 28) (alice . 30) (carol . 35)) (0 -1 -2 3 4 -5))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// seq-min / seq-max
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_seq_min_max() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // seq-min and seq-max return the minimum/maximum element.
    let form = r#"(progn (require (quote cl-lib)) (list
                    (seq-min '(3 1 4 1 5 9 2 6))
                    (seq-max '(3 1 4 1 5 9 2 6))
                    (seq-min [100 50 200 25 300])
                    (seq-max [100 50 200 25 300])
                    ;; Single element
                    (seq-min '(42))
                    (seq-max '(42))
                    ;; Negative numbers
                    (seq-min '(-10 -5 -20 -1))
                    (seq-max '(-10 -5 -20 -1))))"#;
    let expect = expect_test::expect![[r#""OK (1 9 25 300 42 42 -20 -1)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// seq-partition: split by predicate into two groups
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_seq_partition() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // seq-partition returns a list of two lists: (matching non-matching)
    let form = r#"(progn (require (quote cl-lib)) (list
                    ;; Partition by evenness
                    (seq-partition #'cl-evenp '(1 2 3 4 5 6 7 8 9 10))
                    ;; Partition by type
                    (seq-partition #'stringp '(1 "a" 2 "b" 3 "c"))
                    ;; All match
                    (seq-partition #'numberp '(1 2 3))
                    ;; None match
                    (seq-partition #'numberp '("a" "b" "c"))
                    ;; Empty
                    (seq-partition #'numberp nil)))"#;
    let expect = expect_test::expect![[
        r#""ERR (wrong-type-argument number-or-marker-p (1 2 3 4 5 6 7 8 9 10))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// seq-subseq: subsequence extraction
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_seq_subseq() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // seq-subseq extracts a portion of a sequence.
    let form = r#"(progn (require (quote cl-lib)) (list
                    ;; List subseq with start and end
                    (seq-subseq '(a b c d e f) 1 4)
                    ;; List subseq with only start (to end)
                    (seq-subseq '(a b c d e f) 3)
                    ;; Vector subseq
                    (seq-subseq [10 20 30 40 50] 1 3)
                    ;; String subseq
                    (seq-subseq "hello world" 6)
                    ;; Start=0, end=length (full copy)
                    (seq-subseq '(1 2 3) 0 3)
                    ;; Empty result
                    (seq-subseq '(1 2 3) 2 2)
                    ;; Negative indices (from end)
                    (seq-subseq '(a b c d e) -3)
                    (seq-subseq "abcdef" -4 -1)))"#;
    let expect = expect_test::expect![[
        r#""OK ((b c d) (d e f) [20 30] \"world\" (1 2 3) nil (c d e) \"cde\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: seq-based data analysis pipeline
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_seq_data_analysis_pipeline() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Multi-step data analysis: students with grades across subjects.
    // Compute per-student averages, find top performers, group by tier.
    let form = r#"((require (quote cl-lib)) let ((students
                         '((alice (math . 92) (eng . 88) (sci . 95))
                           (bob (math . 75) (eng . 82) (sci . 70))
                           (carol (math . 98) (eng . 94) (sci . 91))
                           (dave (math . 60) (eng . 65) (sci . 72))
                           (eve (math . 85) (eng . 90) (sci . 87)))))
                    ;; Step 1: Compute average for each student
                    (let ((with-avg
                           (seq-map
                            (lambda (s)
                              (let* ((name (car s))
                                     (grades (cdr s))
                                     (total (seq-reduce
                                             (lambda (acc g) (+ acc (cdr g)))
                                             grades 0))
                                     (avg (/ total (length grades))))
                                (list name avg)))
                            students)))
                      ;; Step 2: Sort by average descending
                      (let ((sorted (seq-sort-by #'cadr (lambda (a b) (> a b))
                                                 with-avg)))
                        ;; Step 3: Partition into pass (>=70) and fail
                        (let ((partitioned
                               (seq-partition (lambda (s) (>= (cadr s) 70))
                                              sorted)))
                          ;; Step 4: Group passing students by tier
                          (let ((tiers
                                 (seq-group-by
                                  (lambda (s)
                                    (let ((avg (cadr s)))
                                      (cond ((>= avg 90) 'A)
                                            ((>= avg 80) 'B)
                                            (t 'C))))
                                  (car partitioned))))
                            (list
                             ;; Ranking
                             (seq-map #'car sorted)
                             ;; Top student average
                             (cadr (car sorted))
                             ;; Number passing / failing
                             (length (car partitioned))
                             (length (cadr partitioned))
                             ;; Tier groups (sorted by tier name)
                             (sort (seq-map
                                    (lambda (tier)
                                      (cons (car tier)
                                            (seq-map #'car (cdr tier))))
                                    tiers)
                                   (lambda (a b)
                                     (string-lessp (symbol-name (car a))
                                                   (symbol-name (car b)))))))))))"#;
    let expect = expect_test::expect![[r#""ERR (invalid-function (require 'cl-lib))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: seq-based set operations with custom equality
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_seq_custom_set_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Set operations on structured data using custom equality (by 'id field).
    let form = r#"((require (quote cl-lib)) let ((set-a '((:id 1 :name "Alice")
                                  (:id 2 :name "Bob")
                                  (:id 3 :name "Carol")))
                        (set-b '((:id 2 :name "Bob-Updated")
                                  (:id 4 :name "Dave")
                                  (:id 3 :name "Carol"))))
                    (let ((id-of (lambda (x) (plist-get x :id)))
                          (has-id (lambda (id set)
                                    (seq-some
                                     (lambda (x) (= (plist-get x :id) id))
                                     set))))
                      ;; Union by id (prefer set-b entries on conflict)
                      (let ((union
                             (append
                              (seq-remove (lambda (x)
                                            (funcall has-id (funcall id-of x) set-b))
                                          set-a)
                              set-b)))
                        ;; Intersection by id (entries from set-a whose id is in set-b)
                        (let ((inter
                               (seq-filter (lambda (x)
                                             (funcall has-id (funcall id-of x) set-b))
                                           set-a)))
                          ;; Difference: set-a minus set-b (by id)
                          (let ((diff
                                 (seq-remove (lambda (x)
                                               (funcall has-id (funcall id-of x) set-b))
                                             set-a)))
                            ;; Symmetric difference: in one but not both
                            (let ((sym-diff
                                   (append
                                    (seq-remove (lambda (x)
                                                  (funcall has-id (funcall id-of x) set-b))
                                                set-a)
                                    (seq-remove (lambda (x)
                                                  (funcall has-id (funcall id-of x) set-a))
                                                set-b))))
                              (list
                               ;; Union ids sorted
                               (sort (seq-map id-of union) #'<)
                               ;; Intersection ids
                               (sort (seq-map id-of inter) #'<)
                               ;; Difference ids (a - b)
                               (seq-map id-of diff)
                               ;; Symmetric difference ids sorted
                               (sort (seq-map id-of sym-diff) #'<)
                               ;; Union count
                               (length union))))))))"#;
    let expect = expect_test::expect![[r#""ERR (invalid-function (require 'cl-lib))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
