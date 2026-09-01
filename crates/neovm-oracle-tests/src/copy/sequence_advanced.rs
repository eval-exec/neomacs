//! Advanced oracle parity tests for `copy-sequence`:
//! copy list with independence verification, copy vector, copy string,
//! copy cons cell, deep vs shallow copy semantics, copy alist with
//! mutation isolation, and combined with sort (sort copy, original unchanged).

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

// ---------------------------------------------------------------------------
// Copy list and verify structural independence via nested mutation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_copy_sequence_advanced_list_independence() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Copy a list, mutate the copy at multiple positions (setcar, setcdr,
    // append), and verify the original is completely untouched.
    let form = r#"(let* ((orig (list 1 2 3 4 5))
                         (copy (copy-sequence orig)))
                   ;; Mutate copy: replace first element
                   (setcar copy 99)
                   ;; Mutate copy: replace second element
                   (setcar (cdr copy) 88)
                   ;; Mutate copy: truncate after third element
                   (setcdr (cddr copy) nil)
                   ;; Verify original is untouched and copy is changed
                   (list
                    (equal orig '(1 2 3 4 5))
                    orig
                    copy
                    (length orig)
                    (length copy)
                    (eq orig copy)
                    ;; The cdrs should NOT be eq (top-level spine is copied)
                    (eq (cdr orig) (cdr copy))))"#;
    let expect = expect_test::expect![[r#""OK (t (1 2 3 4 5) (99 88 3) 5 3 nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_copy_sequence_improper_list_tail_error_payload() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU src/fns.c:Fcopy_sequence copies cons cells one by one and then
    // CHECK_LIST_END reports the offending non-list tail, not the original
    // dotted list.  This exact payload matters for condition-case consumers.
    let form = r#"
(list
 (condition-case err
     (copy-sequence '(a b . c))
   (error (list (car err) (cdr err))))
 (condition-case err
     (copy-sequence '(a . (b . c)))
   (error (list (car err) (cdr err))))
 (condition-case err
     (copy-sequence 42)
   (error (list (car err) (cdr err)))))
"#;

    let expect = expect_test::expect![[
        r#""OK ((wrong-type-argument (listp c)) (wrong-type-argument (listp c)) (wrong-type-argument (sequencep 42)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Copy vector: verify independence and element-level mutation isolation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_copy_sequence_advanced_vector_independence() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Copy vector, mutate copy via aset, verify original unchanged.
    // Also test with nested mutable elements to show shallow semantics.
    let form = r#"(let* ((orig (vector 10 20 30 40 50))
                         (copy (copy-sequence orig)))
                   ;; Mutate copy
                   (aset copy 0 999)
                   (aset copy 4 111)
                   ;; Verify independence
                   (list
                    (aref orig 0)
                    (aref orig 4)
                    (aref copy 0)
                    (aref copy 4)
                    (equal orig (vector 10 20 30 40 50))
                    (equal copy (vector 999 20 30 40 111))
                    (eq orig copy)
                    (length orig)
                    (length copy)))"#;
    let expect = expect_test::expect![[r#""OK (10 50 999 111 t t nil 5 5)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Copy string: mutation isolation and multi-byte string handling
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_copy_sequence_advanced_string_mutation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Copy a string, mutate the copy via aset, verify original unchanged.
    // Also test with unicode content.
    let form = r#"(let* ((orig "hello world")
                         (copy (copy-sequence orig)))
                   ;; Mutate copy
                   (aset copy 0 ?H)
                   (aset copy 6 ?W)
                   (list
                    orig
                    copy
                    (string= orig "hello world")
                    (string= copy "Hello World")
                    (eq orig copy)
                    (equal orig copy)
                    ;; Empty string copy
                    (let* ((e "") (ec (copy-sequence e)))
                      (list (string= e ec) (eq e ec)))
                    ;; Single char string
                    (let* ((s "x") (sc (copy-sequence s)))
                      (aset sc 0 ?y)
                      (list s sc))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"hello world\" \"Hello World\" t t nil nil (t t) (\"x\" \"y\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Shallow copy semantics: nested structures share identity
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_copy_sequence_advanced_shallow_semantics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Demonstrate that copy-sequence is shallow: nested mutable objects
    // in the copy are the SAME objects as in the original.
    let form = r#"(let* ((inner1 (list 'a 'b))
                         (inner2 (vector 1 2 3))
                         (orig (list inner1 inner2 "hello"))
                         (copy (copy-sequence orig)))
                   ;; The top-level cons cells are different
                   (let ((top-different (not (eq orig copy))))
                     ;; But the nested objects are the SAME (shared)
                     (let ((inner1-shared (eq (car orig) (car copy)))
                           (inner2-shared (eq (cadr orig) (cadr copy)))
                           (str-shared (eq (caddr orig) (caddr copy))))
                       ;; Mutating a nested object through copy affects original
                       (setcar (car copy) 'MUTATED)
                       (aset (cadr copy) 0 999)
                       (list
                        top-different
                        inner1-shared
                        inner2-shared
                        str-shared
                        ;; Original's inner1 is also mutated (shared!)
                        (car orig)
                        (car copy)
                        ;; Original's inner2 is also mutated (shared!)
                        (aref (cadr orig) 0)
                        (aref (cadr copy) 0)))))"#;
    let expect = expect_test::expect![[r#""OK (t t t t (MUTATED b) (MUTATED b) 999 999)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Copy alist: modify original, copy is independent at top level
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_copy_sequence_advanced_alist_isolation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Copy an alist, then add/remove entries from original.
    // The copy's top-level structure should be independent.
    // But since copy-sequence is shallow, the cons cells (key . value)
    // are shared -- mutating a value cell affects both.
    let form = r#"(let* ((orig (list (cons 'a 1) (cons 'b 2) (cons 'c 3)))
                         (copy (copy-sequence orig)))
                   ;; Add to original (push a new pair)
                   (setq orig (cons (cons 'd 4) orig))
                   (let ((orig-has-d (assq 'd orig))
                         (copy-has-d (assq 'd copy))
                         ;; Copy should still have 3 elements
                         (copy-len (length copy))
                         (orig-len (length orig)))
                     ;; Now mutate a shared cons cell through copy
                     (setcdr (assq 'a copy) 100)
                     ;; Since the pair (a . 1) is shared, original sees it too
                     (let ((orig-a-val (cdr (assq 'a orig)))
                           (copy-a-val (cdr (assq 'a copy))))
                       (list
                        (consp orig-has-d)
                        (null copy-has-d)
                        orig-len
                        copy-len
                        orig-a-val
                        copy-a-val
                        ;; Verify the rest of copy is untouched
                        (cdr (assq 'b copy))
                        (cdr (assq 'c copy))))))"#;
    let expect = expect_test::expect![[r#""OK (t t 4 3 100 100 2 3)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Sort a copy, original remains unchanged
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_copy_sequence_advanced_sort_copy() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // sort is destructive. Copy first, sort the copy, verify original order.
    let form = r#"(let* ((orig (list 5 3 8 1 9 2 7 4 6 10))
                         (sorted (sort (copy-sequence orig) #'<))
                         (reverse-sorted (sort (copy-sequence orig) #'>)))
                   (list
                    ;; Original should be the same as its initial value
                    ;; (sort is destructive, but we sorted a copy)
                    orig
                    sorted
                    reverse-sorted
                    ;; Verify sorted is actually sorted
                    (equal sorted '(1 2 3 4 5 6 7 8 9 10))
                    (equal reverse-sorted '(10 9 8 7 6 5 4 3 2 1))
                    ;; Verify lengths match
                    (= (length orig) (length sorted))
                    (= (length orig) (length reverse-sorted))
                    ;; Sort strings by copy
                    (let* ((words (list "banana" "apple" "cherry" "date"))
                           (sorted-words (sort (copy-sequence words) #'string<)))
                      (list words sorted-words))))"#;
    let expect = expect_test::expect![[
        r#""OK ((5 3 8 1 9 2 7 4 6 10) (1 2 3 4 5 6 7 8 9 10) (10 9 8 7 6 5 4 3 2 1) t t t t ((\"banana\" \"apple\" \"cherry\" \"date\") (\"apple\" \"banana\" \"cherry\" \"date\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Copy-sequence with various types and combined pipeline
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_copy_sequence_advanced_pipeline() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a data processing pipeline that relies on copy-sequence
    // to create working copies at each stage.
    let form = r#"(let* ((data (list 42 17 93 8 55 31 76 64 22 49))
                         ;; Stage 1: sort ascending
                         (stage1 (sort (copy-sequence data) #'<))
                         ;; Stage 2: filter evens from sorted
                         (stage2 (let ((result nil))
                                   (dolist (x stage1)
                                     (when (= (% x 2) 0)
                                       (setq result (cons x result))))
                                   (nreverse result)))
                         ;; Stage 3: from original, get top 5
                         (stage3 (let* ((sorted-desc (sort (copy-sequence data) #'>))
                                        (top5 nil)
                                        (count 0))
                                   (while (and sorted-desc (< count 5))
                                     (setq top5 (cons (car sorted-desc) top5))
                                     (setq sorted-desc (cdr sorted-desc))
                                     (setq count (1+ count)))
                                   (nreverse top5)))
                         ;; Stage 4: compute stats from a copy
                         (working (copy-sequence data))
                         (sum (apply #'+ working))
                         (count (length working))
                         (mean (/ (float sum) count))
                         ;; Stage 5: vector pipeline
                         (vec-orig (vconcat data))
                         (vec-copy (copy-sequence vec-orig)))
                   ;; Mutate vec-copy
                   (aset vec-copy 0 0)
                   (aset vec-copy 1 0)
                   (list
                    ;; Original data unchanged through all stages
                    data
                    stage1
                    stage2
                    stage3
                    sum
                    mean
                    ;; Vector original untouched
                    (aref vec-orig 0)
                    (aref vec-orig 1)
                    (aref vec-copy 0)
                    (aref vec-copy 1)
                    ;; Bool vector copy
                    (let* ((bv (make-bool-vector 8 nil))
                           (bv-copy (copy-sequence bv)))
                      (aset bv-copy 0 t)
                      (aset bv-copy 3 t)
                      (list (aref bv 0) (aref bv 3)
                            (aref bv-copy 0) (aref bv-copy 3)))))"#;
    let expect = expect_test::expect![[
        r#""OK ((42 17 93 8 55 31 76 64 22 49) (8 17 22 31 42 49 55 64 76 93) (8 22 42 64 76) (93 76 64 55 49) 457 45.7 42 17 0 0 (nil nil t t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Copy-sequence on dotted pairs and edge cases
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_copy_sequence_advanced_edge_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test copy-sequence with nil, single element lists, and nested lists.
    // Also verify that copy-sequence of a copy is independent from both.
    let form = r#"(list
                   ;; nil
                   (copy-sequence nil)
                   ;; Single element
                   (let* ((o (list 42)) (c (copy-sequence o)))
                     (setcar c 99)
                     (list o c))
                   ;; Nested list (shallow copy)
                   (let* ((o (list (list 1 2) (list 3 4)))
                          (c (copy-sequence o)))
                     ;; Top-level spine is different
                     (list (eq o c) (eq (cdr o) (cdr c))
                           ;; But nested lists are shared
                           (eq (car o) (car c))))
                   ;; Chain of copies: copy of copy is independent
                   (let* ((a (list 1 2 3))
                          (b (copy-sequence a))
                          (c (copy-sequence b)))
                     (setcar a 10)
                     (setcar b 20)
                     (setcar c 30)
                     (list a b c))
                   ;; Large list copy
                   (let* ((big (let ((result nil))
                                 (dotimes (i 100)
                                   (setq result (cons i result)))
                                 (nreverse result)))
                          (big-copy (copy-sequence big)))
                     (list (equal big big-copy)
                           (eq big big-copy)
                           (length big-copy)
                           (car big-copy)
                           (car (last big-copy))))
                   ;; Copy empty vector
                   (let* ((v (vector)) (vc (copy-sequence v)))
                     (list (equal v vc) (eq v vc) (length vc))))"#;
    let expect = expect_test::expect![[
        r#""OK (nil ((42) (99)) (nil nil t) ((10 2 3) (20 2 3) (30 2 3)) (t nil 100 0 99) (t t 0))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
