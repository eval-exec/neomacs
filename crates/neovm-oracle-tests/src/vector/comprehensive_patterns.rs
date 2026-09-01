//! Oracle parity tests for vector operations — comprehensive coverage of
//! make-vector, vector, vconcat, aref/aset, nested vectors, fillarray,
//! copy-sequence, sort, seq-* operations, equal, and data structure building.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// make-vector with various inits, vector constructor, vconcat
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_vector_creation_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; make-vector with various init types
  (make-vector 5 0)
  (make-vector 3 nil)
  (make-vector 4 t)
  (make-vector 3 'symbol)
  (make-vector 2 "string-init")
  (make-vector 3 '(a b c))
  (make-vector 2 [nested])
  (make-vector 0 42)
  ;; All elements share the same object (eq, not just equal)
  (let ((v (make-vector 3 '(shared))))
    (list (eq (aref v 0) (aref v 1))
          (eq (aref v 1) (aref v 2))))
  ;; vector constructor with heterogeneous types
  (vector 1 "two" 'three nil t '(4 5) [6 7])
  (vector)  ;; empty vector
  (vector 42)  ;; single element
  ;; vconcat: combine multiple sequences
  (vconcat [1 2] '(3 4) "AB" [5])
  (vconcat nil [1] nil [2] nil)
  (vconcat [1 2 3] [1 2 3])
  (vconcat "hello")  ;; string to vector of char codes
  (vconcat)  ;; no args => empty vector
  (vconcat nil)
  ;; Length of results
  (length (make-vector 100 0))
  (length (vector 'a 'b 'c 'd 'e))
  (length (vconcat [1 2] '(3 4 5))))"#;
    let expect = expect_test::expect![[
        r#""OK ([0 0 0 0 0] [nil nil nil] [t t t t] [symbol symbol symbol] [\"string-init\" \"string-init\"] [(a b c) (a b c) (a b c)] [[nested] [nested]] [] (t t) [1 \"two\" three nil t (4 5) [6 7]] [] [42] [1 2 3 4 65 66 5] [1 2] [1 2 3 1 2 3] [104 101 108 108 111] [] [] 100 5 5)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// aref/aset with various indices and boundary probing
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_vector_aref_aset_boundaries() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Basic aref
  (let ((v [10 20 30 40 50]))
    (list (aref v 0) (aref v 2) (aref v 4)))
  ;; aset returns the value that was set
  (let ((v (make-vector 3 0)))
    (list (aset v 0 'first)
          (aset v 1 'second)
          (aset v 2 'third)
          v))
  ;; aset with different types
  (let ((v (make-vector 6 nil)))
    (aset v 0 42)
    (aset v 1 "hello")
    (aset v 2 'sym)
    (aset v 3 '(a b))
    (aset v 4 [inner])
    (aset v 5 3.14)
    v)
  ;; Overwrite existing values
  (let ((v [1 2 3 4 5]))
    (aset v 0 99)
    (aset v 4 99)
    (aset v 2 99)
    v)
  ;; Sequential overwrites at same index
  (let ((v (make-vector 1 0)))
    (let ((results nil))
      (dotimes (i 10)
        (aset v 0 i)
        (setq results (cons (aref v 0) results)))
      (list v (nreverse results))))
  ;; Out of bounds should signal error
  (condition-case err
      (aref [1 2 3] 5)
    (args-out-of-range (list 'caught (car err))))
  (condition-case err
      (aref [1 2 3] -1)
    (args-out-of-range (list 'caught (car err))))
  (condition-case err
      (aset [1 2 3] 3 'x)
    (args-out-of-range (list 'caught (car err)))))"#;
    let expect = expect_test::expect![[
        r#""OK ((10 30 50) (first second third [first second third]) [42 \"hello\" sym (a b) [inner] 3.14] [99 2 99 4 99] ([9] (0 1 2 3 4 5 6 7 8 9)) (caught args-out-of-range) (caught args-out-of-range) (caught args-out-of-range))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Nested vectors (vector of vectors) — deep access and mutation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_nested_vectors_deep_access() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; 3-level nesting
  (let ((v (vector (vector (vector 'deep)))))
    (aref (aref (aref v 0) 0) 0))
  ;; Build a 4x4 identity matrix
  (let ((mat (make-vector 4 nil)))
    (dotimes (i 4)
      (let ((row (make-vector 4 0)))
        (aset row i 1)
        (aset mat i row)))
    (list (aref mat 0) (aref mat 1) (aref mat 2) (aref mat 3)))
  ;; Mutate nested vector and verify parent reflects change
  (let ((inner (vector 'a 'b 'c))
        (outer (make-vector 2 nil)))
    (aset outer 0 inner)
    (aset outer 1 inner)  ;; both slots point to same inner
    (aset inner 1 'CHANGED)
    (list (aref (aref outer 0) 1)
          (aref (aref outer 1) 1)
          (eq (aref outer 0) (aref outer 1))))
  ;; Jagged array (rows of different lengths)
  (let ((jagged (vector [1] [2 3] [4 5 6] [7 8 9 10])))
    (list (length (aref jagged 0))
          (length (aref jagged 1))
          (length (aref jagged 2))
          (length (aref jagged 3))
          (aref (aref jagged 2) 2)
          (aref (aref jagged 3) 3)))
  ;; Flatten nested vectors
  (let ((nested [[[1 2] [3 4]] [[5 6] [7 8]]]))
    (let ((result nil))
      (dotimes (i (length nested))
        (let ((mid (aref nested i)))
          (dotimes (j (length mid))
            (let ((inner (aref mid j)))
              (dotimes (k (length inner))
                (setq result (cons (aref inner k) result)))))))
      (nreverse result))))"#;
    let expect = expect_test::expect![[
        r#""OK (deep ([1 0 0 0] [0 1 0 0] [0 0 1 0] [0 0 0 1]) (CHANGED CHANGED t) (1 2 3 4 6 10) (1 2 3 4 5 6 7 8))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// fillarray on vectors — comprehensive
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_fillarray_vector_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Basic fillarray
  (let ((v (vector 1 2 3 4 5)))
    (fillarray v 0)
    v)
  ;; fillarray returns the array itself (eq)
  (let ((v (vector 'a 'b 'c)))
    (eq v (fillarray v nil)))
  ;; fillarray with various types
  (let ((v1 (make-vector 3 0))
        (v2 (make-vector 3 0))
        (v3 (make-vector 3 0))
        (v4 (make-vector 3 0)))
    (fillarray v1 'sym)
    (fillarray v2 "str")
    (fillarray v3 '(list val))
    (fillarray v4 42)
    (list v1 v2 v3 v4))
  ;; fillarray with nil on non-nil vector
  (let ((v [1 2 3 4 5]))
    (fillarray v nil)
    v)
  ;; fillarray on empty vector (no-op, no error)
  (let ((v (make-vector 0 0)))
    (fillarray v 99)
    (list (length v) v))
  ;; fillarray then aset specific elements
  (let ((v (make-vector 5 ?-)))
    (fillarray v ?.)
    (aset v 0 ?[)
    (aset v 4 ?])
    (append v nil))  ;; convert to list of char codes
  ;; fillarray shares object identity
  (let ((v (make-vector 3 nil))
        (shared-list '(x y z)))
    (fillarray v shared-list)
    (list (eq (aref v 0) (aref v 1))
          (eq (aref v 1) (aref v 2))
          (eq (aref v 0) shared-list))))"#;
    let expect = expect_test::expect![[
        r#""OK ([0 0 0 0 0] t ([sym sym sym] [\"str\" \"str\" \"str\"] [(list val) (list val) (list val)] [42 42 42]) [nil nil nil nil nil] (0 []) (91 46 46 46 93) (t t t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// copy-sequence on vectors — verify deep independence
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_copy_sequence_vector_independence() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Basic copy
  (let* ((orig [1 2 3 4 5])
         (copy (copy-sequence orig)))
    (list (equal orig copy)
          (eq orig copy)
          copy))
  ;; Mutation of copy does not affect original
  (let* ((orig [10 20 30])
         (copy (copy-sequence orig)))
    (aset copy 1 999)
    (list orig copy (equal orig copy)))
  ;; Mutation of original does not affect copy
  (let* ((orig [10 20 30])
         (copy (copy-sequence orig)))
    (aset orig 0 -1)
    (list orig copy))
  ;; copy-sequence of empty vector
  (let* ((orig (vector))
         (copy (copy-sequence orig)))
    (list (length copy) (equal orig copy) (eq orig copy)))
  ;; Shallow copy: nested vector shares inner objects
  (let* ((inner [1 2 3])
         (orig (vector inner 'b))
         (copy (copy-sequence orig)))
    (aset inner 0 999)
    ;; Both orig and copy see the change in inner
    (list (aref (aref orig 0) 0)
          (aref (aref copy 0) 0)
          (eq (aref orig 0) (aref copy 0))))
  ;; copy-sequence preserves types
  (let* ((orig (vector 1 "two" 'three nil t 3.14))
         (copy (copy-sequence orig)))
    (list (equal orig copy)
          (aref copy 0) (aref copy 1) (aref copy 2)
          (aref copy 3) (aref copy 4) (aref copy 5))))"#;
    let expect = expect_test::expect![[
        r#""OK ((t nil [1 2 3 4 5]) ([10 20 30] [10 999 30] nil) ([-1 20 30] [10 20 30]) (0 t t) (999 999 t) (t 1 \"two\" three nil t 3.14))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Vector comparison with equal — deep structural equality
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_vector_equal_comparison() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Same contents
  (equal [1 2 3] [1 2 3])
  (equal (vector 'a 'b) (vector 'a 'b))
  ;; Different contents
  (equal [1 2 3] [1 2 4])
  (equal [1 2] [1 2 3])
  (equal [1 2 3] [1 2])
  ;; Nested equal
  (equal [[1 2] [3 4]] [[1 2] [3 4]])
  (equal [[1 2] [3 4]] [[1 2] [3 5]])
  ;; Mixed types
  (equal [1 "two" three] [1 "two" three])
  (equal [1 "two" three] [1 "TWO" three])
  ;; eq vs equal for vectors
  (let ((v [1 2 3]))
    (list (eq v v)
          (equal v v)
          (equal v [1 2 3])
          (eq v [1 2 3])))
  ;; Empty vectors
  (equal [] [])
  (equal (vector) (make-vector 0 nil))
  ;; Vector vs list (never equal)
  (equal [1 2 3] '(1 2 3))
  ;; Deep nesting equality
  (equal [[[1]]] [[[1]]])
  (equal [[[1]]] [[[2]]]))"#;
    let expect =
        expect_test::expect![[r#""OK (t t nil nil nil t nil t nil (t t t nil) t t nil t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Building data structures with vectors (records, tables)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_vector_data_structures() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Record-style struct: [name age city]
  (fset 'neovm--test-make-person (lambda (name age city) (vector name age city)))
  (fset 'neovm--test-person-name (lambda (p) (aref p 0)))
  (fset 'neovm--test-person-age (lambda (p) (aref p 1)))
  (fset 'neovm--test-person-city (lambda (p) (aref p 2)))
  (fset 'neovm--test-person-set-age (lambda (p age) (aset p 1 age) p))

  ;; Table: vector of records
  (fset 'neovm--test-make-table (lambda (records) (vconcat records)))
  (fset 'neovm--test-table-find
    (lambda (table pred)
      (let ((result nil) (i 0) (len (length table)))
        (while (and (null result) (< i len))
          (when (funcall pred (aref table i))
            (setq result (aref table i)))
          (setq i (1+ i)))
        result)))
  (fset 'neovm--test-table-filter
    (lambda (table pred)
      (let ((result nil) (i 0) (len (length table)))
        (while (< i len)
          (when (funcall pred (aref table i))
            (setq result (cons (aref table i) result)))
          (setq i (1+ i)))
        (vconcat (nreverse result)))))

  (unwind-protect
      (let* ((alice (funcall 'neovm--test-make-person "Alice" 30 "NYC"))
             (bob (funcall 'neovm--test-make-person "Bob" 25 "LA"))
             (carol (funcall 'neovm--test-make-person "Carol" 35 "NYC"))
             (dave (funcall 'neovm--test-make-person "Dave" 28 "SF"))
             (table (funcall 'neovm--test-make-table (list alice bob carol dave))))
        (list
         ;; Access fields
         (funcall 'neovm--test-person-name alice)
         (funcall 'neovm--test-person-age bob)
         (funcall 'neovm--test-person-city carol)
         ;; Mutation
         (funcall 'neovm--test-person-set-age bob 26)
         (funcall 'neovm--test-person-age bob)
         ;; Table operations
         (length table)
         ;; Find person older than 30
         (let ((found (funcall 'neovm--test-table-find table
                        (lambda (p) (> (funcall 'neovm--test-person-age p) 30)))))
           (funcall 'neovm--test-person-name found))
         ;; Filter people in NYC
         (let ((nyc-people (funcall 'neovm--test-table-filter table
                             (lambda (p) (string= (funcall 'neovm--test-person-city p) "NYC")))))
           (list (length nyc-people)
                 (funcall 'neovm--test-person-name (aref nyc-people 0))
                 (funcall 'neovm--test-person-name (aref nyc-people 1))))
         ;; Sort table by age (destructive)
         (let ((sorted (sort (copy-sequence table)
                        (lambda (a b) (< (aref a 1) (aref b 1))))))
           (list (funcall 'neovm--test-person-name (aref sorted 0))
                 (funcall 'neovm--test-person-name (aref sorted 1))
                 (funcall 'neovm--test-person-name (aref sorted 2))
                 (funcall 'neovm--test-person-name (aref sorted 3))))))
    (fmakunbound 'neovm--test-make-person)
    (fmakunbound 'neovm--test-person-name)
    (fmakunbound 'neovm--test-person-age)
    (fmakunbound 'neovm--test-person-city)
    (fmakunbound 'neovm--test-person-set-age)
    (fmakunbound 'neovm--test-make-table)
    (fmakunbound 'neovm--test-table-find)
    (fmakunbound 'neovm--test-table-filter)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"Alice\" 25 \"NYC\" [\"Bob\" 26 \"LA\"] 26 4 \"Carol\" (2 \"Alice\" \"Carol\") (\"Bob\" \"Dave\" \"Alice\" \"Carol\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Vectors with sort — various predicates and stability
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_vector_sort_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Sort integers ascending/descending
  (sort (vector 5 1 9 3 7 2 8 4 6 10) #'<)
  (sort (vector 5 1 9 3 7 2 8 4 6 10) #'>)
  ;; Sort strings lexicographically
  (sort (vector "banana" "apple" "cherry" "date") #'string<)
  ;; Sort by computed key (length of string)
  (let ((v (sort (copy-sequence ["elephant" "cat" "dog" "hippopotamus" "bee"])
                 (lambda (a b) (< (length a) (length b))))))
    (append v nil))
  ;; Sort empty vector
  (sort (vector) #'<)
  ;; Sort single element
  (sort (vector 42) #'<)
  ;; Sort already sorted
  (sort (vector 1 2 3 4 5) #'<)
  ;; Sort reverse-sorted
  (sort (vector 5 4 3 2 1) #'<)
  ;; Sort with duplicates
  (sort (vector 3 1 4 1 5 9 2 6 5 3 5) #'<)
  ;; Sort preserves all elements (no loss)
  (let ((orig [5 3 1 4 2])
        (sorted (sort (copy-sequence [5 3 1 4 2]) #'<)))
    (list (= (seq-reduce #'+ orig 0)
             (seq-reduce #'+ sorted 0))
          (= (length orig) (length sorted)))))"#;
    let expect = expect_test::expect![[
        r#""OK ([1 2 3 4 5 6 7 8 9 10] [10 9 8 7 6 5 4 3 2 1] [\"apple\" \"banana\" \"cherry\" \"date\"] (\"cat\" \"dog\" \"bee\" \"elephant\" \"hippopotamus\") [] [42] [1 2 3 4 5] [1 2 3 4 5] [1 1 2 3 3 4 5 5 5 6 9] (t t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
