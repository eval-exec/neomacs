//! Oracle parity tests for comprehensive vector operations:
//! `make-vector`, `vector`, `vconcat`, `aref`/`aset`, `length`, `fillarray`,
//! `copy-sequence` on vectors, `cl-coerce` vector<->list, `sort` on vectors,
//! `seq-map`/`seq-filter`/`seq-reduce` on vectors, `seq-into` conversion,
//! `cl-map` with result-type vector, `cl-substitute`, vector comparison.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// make-vector with various init values and aref/aset round-trip
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_vector_ops_comp_make_vector_aref_aset_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"((require (quote cl-lib)) let ((v (make-vector 8 nil)))
  ;; Fill with computed values: index * 10 + 3
  (let ((i 0))
    (while (< i 8)
      (aset v i (+ (* i 10) 3))
      (setq i (1+ i))))
  ;; Read back all and verify length
  (list (length v)
        (aref v 0) (aref v 3) (aref v 7)
        ;; Overwrite middle element with a string
        (progn (aset v 4 "hello") (aref v 4))
        ;; Overwrite with a nested vector
        (progn (aset v 5 [nested 1 2]) (aref v 5))
        ;; Overwrite with nil, symbol, float
        (progn (aset v 6 nil) (aset v 7 3.14) (list (aref v 6) (aref v 7)))
        ;; Original values unchanged
        (aref v 0) (aref v 1) (aref v 2)))"#;
    let expect = expect_test::expect![[r#""ERR (invalid-function (require 'cl-lib))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// vector constructor and vconcat with mixed types
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_vector_ops_comp_vector_constructor_and_vconcat_mixed() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"((require (quote cl-lib)) let* ((v1 (vector 1 2 3))
       (v2 (vector 'a 'b 'c))
       (v3 (vector "x" "y"))
       (v4 (vector nil t 0 1.5))
       ;; vconcat with mixed argument types: vectors, lists, strings
       (cat1 (vconcat v1 v2))
       (cat2 (vconcat v1 '(10 20 30)))
       (cat3 (vconcat "abc" [100 200]))
       (cat4 (vconcat v1 v2 v3 v4))
       (cat5 (vconcat nil))
       (cat6 (vconcat [] [] [42])))
  (list cat1 cat2 cat3 cat4 cat5 cat6
        (length cat1) (length cat4)
        ;; vconcat with empty inputs
        (vconcat) (vconcat []) (vconcat '() "" [])))"#;
    let expect = expect_test::expect![[r#""ERR (invalid-function (require 'cl-lib))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// fillarray on vectors with various fill values
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_vector_ops_comp_fillarray_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"((require (quote cl-lib)) let ((v1 (make-vector 5 0))
      (v2 (vector 1 2 3 4 5))
      (v3 (make-vector 3 'old)))
  ;; fillarray returns the array itself
  (let ((ret1 (fillarray v1 99))
        (ret2 (fillarray v2 nil))
        (ret3 (fillarray v3 'new-sym)))
    (list
     ;; fillarray returns the same object
     (eq ret1 v1)
     ;; All elements are now the fill value
     v1 v2 v3
     ;; Fill with 0-length vector is fine
     (let ((v4 (make-vector 0 42)))
       (fillarray v4 100)
       v4)
     ;; Fill with nested structure
     (let ((v5 (make-vector 3 nil)))
       (fillarray v5 '(a b c))
       ;; All elements share the same list object
       (list v5 (eq (aref v5 0) (aref v5 1)) (eq (aref v5 1) (aref v5 2)))))))"#;
    let expect = expect_test::expect![[r#""ERR (invalid-function (require 'cl-lib))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// copy-sequence on vectors: deep independence
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_vector_ops_comp_copy_sequence_independence() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"((require (quote cl-lib)) let* ((orig (vector 1 2 3 4 5))
       (copy (copy-sequence orig)))
  ;; Copy is equal but not eq
  (let ((pre-equal (equal orig copy))
        (pre-eq (eq orig copy)))
    ;; Mutate copy, original unchanged
    (aset copy 0 999)
    (aset copy 4 888)
    (list pre-equal pre-eq
          orig copy
          ;; After mutation: no longer equal
          (equal orig copy)
          ;; copy-sequence on empty vector
          (let* ((e (vector))
                 (ec (copy-sequence e)))
            (list (equal e ec) (eq e ec) (length ec)))
          ;; copy-sequence preserves types of elements
          (let* ((mixed (vector "str" 42 3.14 nil t 'sym '(a b)))
                 (mc (copy-sequence mixed)))
            (list (equal mixed mc)
                  (aref mc 0) (aref mc 3) (aref mc 6))))))"#;
    let expect = expect_test::expect![[r#""ERR (invalid-function (require 'cl-lib))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// sort on vectors (destructive)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_vector_ops_comp_sort_vectors() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn (require (quote cl-lib)) (list
  ;; Sort integers
  (let ((v (vector 5 3 8 1 9 2 7 4 6)))
    (sort v #'<))
  ;; Sort strings
  (let ((v (vector "banana" "apple" "cherry" "date")))
    (sort v #'string<))
  ;; Sort with custom comparator: descending
  (let ((v (vector 10 40 20 50 30)))
    (sort v #'>))
  ;; Sort preserves length
  (let ((v (vector 3 1 2)))
    (list (length (sort v #'<)) (sort (copy-sequence v) #'<)))
  ;; Sort single element and empty
  (list (sort (vector 42) #'<) (sort (vector) #'<))
  ;; Sort with equal elements
  (sort (vector 5 3 5 1 3 1) #'<)
  ;; Sort by absolute value using a wrapper
  (let ((v (vector -3 1 -5 2 -1 4)))
    (sort v (lambda (a b) (< (abs a) (abs b)))))))"#;
    let expect = expect_test::expect![[
        r#""OK ([1 2 3 4 5 6 7 8 9] [\"apple\" \"banana\" \"cherry\" \"date\"] [50 40 30 20 10] (3 [1 2 3]) ([42] []) [1 1 3 3 5 5] [1 -1 2 -3 4 -5])""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// cl-coerce vector<->list round-trip
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_vector_ops_comp_cl_coerce_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"((require (quote cl-lib)) progn
  (require 'cl-lib)
  (list
    ;; vector -> list
    (cl-coerce [1 2 3] 'list)
    ;; list -> vector
    (cl-coerce '(a b c) 'vector)
    ;; round-trip: vector -> list -> vector
    (let* ((v [10 20 30])
           (l (cl-coerce v 'list))
           (v2 (cl-coerce l 'vector)))
      (list (equal v v2) l v2))
    ;; empty vector -> list
    (cl-coerce [] 'list)
    ;; empty list -> vector
    (cl-coerce nil 'vector)
    ;; string -> list (char codes)
    (cl-coerce "abc" 'list)
    ;; list of chars -> string
    (cl-coerce '(65 66 67) 'string)
    ;; vector -> vector (identity)
    (let ((v [1 2 3]))
      (equal v (cl-coerce v 'vector)))))"#;
    let expect = expect_test::expect![[r#""ERR (invalid-function (require 'cl-lib))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// seq-map, seq-filter, seq-reduce on vectors
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_vector_ops_comp_seq_map_filter_reduce() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"((require (quote cl-lib)) progn
  (require 'seq)
  (list
    ;; seq-map: double each element
    (seq-map (lambda (x) (* x 2)) [1 2 3 4 5])
    ;; seq-map: stringify
    (seq-map #'number-to-string [10 20 30])
    ;; seq-filter: keep evens
    (seq-filter #'cl-evenp [1 2 3 4 5 6 7 8])
    ;; seq-filter: keep positive from mixed
    (seq-filter (lambda (x) (> x 0)) [-3 -1 0 1 3 5])
    ;; seq-reduce: sum
    (seq-reduce #'+ [1 2 3 4 5] 0)
    ;; seq-reduce: product
    (seq-reduce #'* [1 2 3 4 5] 1)
    ;; seq-reduce: build string
    (seq-reduce (lambda (acc x) (concat acc (number-to-string x) ","))
                [10 20 30] "")
    ;; Chained: filter then map then reduce
    (seq-reduce #'+
                (seq-map (lambda (x) (* x x))
                         (seq-filter #'cl-oddp [1 2 3 4 5 6 7]))
                0)
    ;; On empty vectors
    (list (seq-map #'1+ []) (seq-filter #'cl-evenp []) (seq-reduce #'+ [] 0))))"#;
    let expect = expect_test::expect![[r#""ERR (invalid-function (require 'cl-lib))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// seq-into conversion between sequence types
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_vector_ops_comp_seq_into_conversions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"((require (quote cl-lib)) progn
  (require 'seq)
  (list
    ;; vector -> list via seq-into
    (seq-into [1 2 3] 'list)
    ;; list -> vector via seq-into
    (seq-into '(a b c) 'vector)
    ;; string -> vector (char codes)
    (seq-into "hello" 'vector)
    ;; string -> list
    (seq-into "hi" 'list)
    ;; vector -> string (if elements are chars)
    (seq-into [72 101 108 108 111] 'string)
    ;; Empty conversions
    (list (seq-into [] 'list) (seq-into nil 'vector) (seq-into "" 'vector))
    ;; Filtered result into vector
    (seq-into (seq-filter #'cl-evenp '(1 2 3 4 5 6)) 'vector)
    ;; Mapped result into list from vector
    (seq-into (seq-map #'1+ [10 20 30]) 'list)))"#;
    let expect = expect_test::expect![[r#""ERR (invalid-function (require 'cl-lib))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// cl-map with result-type vector
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_vector_ops_comp_cl_map_result_type_vector() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"((require (quote cl-lib)) progn
  (require 'cl-lib)
  (list
    ;; cl-map with result-type 'vector over a vector
    (cl-map 'vector #'1+ [1 2 3 4 5])
    ;; cl-map with result-type 'vector over a list
    (cl-map 'vector #'* '(1 2 3) '(10 20 30))
    ;; cl-map with result-type 'list over a vector
    (cl-map 'list #'1+ [10 20 30])
    ;; cl-map 'vector with two vector arguments
    (cl-map 'vector #'+ [1 2 3] [10 20 30])
    ;; cl-map 'vector with mismatched lengths (stops at shortest)
    (cl-map 'vector #'+ [1 2 3 4 5] [10 20])
    ;; cl-map nil (for side effects, returns nil)
    (let ((sum 0))
      (cl-map nil (lambda (x) (setq sum (+ sum x))) [1 2 3 4 5])
      sum)
    ;; cl-map 'string
    (cl-map 'string #'upcase "hello")))"#;
    let expect = expect_test::expect![[r#""ERR (invalid-function (require 'cl-lib))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// cl-substitute on vectors
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_vector_ops_comp_cl_substitute() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"((require (quote cl-lib)) progn
  (require 'cl-lib)
  (list
    ;; Basic substitution in vector
    (cl-substitute 99 2 [1 2 3 2 4 2 5])
    ;; Substitute with :test
    (cl-substitute 'X 3 [1 2 3 4 3 5] :test #'=)
    ;; Substitute with :count (only first N occurrences)
    (cl-substitute 0 1 [1 1 1 1 1] :count 3)
    ;; Substitute with :start and :end
    (cl-substitute 99 2 [2 2 2 2 2] :start 1 :end 4)
    ;; Substitute with :from-end and :count
    (cl-substitute 0 1 [1 1 1 1 1] :count 2 :from-end t)
    ;; No matches -> unchanged copy
    (let* ((v [1 2 3])
           (v2 (cl-substitute 99 42 v)))
      (list v2 (equal v v2)))
    ;; Substitute strings
    (cl-substitute "NEW" "old" ["old" "keep" "old" "stay"] :test #'string=)))"#;
    let expect = expect_test::expect![[r#""ERR (invalid-function (require 'cl-lib))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Vector equality and comparison patterns
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_vector_ops_comp_equality_comparison() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn (require (quote cl-lib)) (list
  ;; eq: same object
  (let ((v [1 2 3])) (eq v v))
  ;; eq: different objects with same content
  (eq [1 2 3] [1 2 3])
  ;; equal: structural equality
  (equal [1 2 3] [1 2 3])
  ;; equal: nested vectors
  (equal [[1 2] [3 4]] [[1 2] [3 4]])
  ;; equal: different lengths
  (equal [1 2] [1 2 3])
  ;; equal: different content
  (equal [1 2 3] [1 2 4])
  ;; equal: vector vs list (not equal)
  (equal [1 2 3] '(1 2 3))
  ;; equal: empty vectors
  (equal [] [])
  ;; equal: mixed types inside
  (equal (vector 1 "a" nil 'b) (vector 1 "a" nil 'b))
  ;; Predicate checks
  (list (vectorp [1 2 3]) (vectorp '(1 2 3)) (vectorp "abc")
        (arrayp [1 2 3]) (arrayp "abc") (arrayp '(1 2))
        (sequencep [1]) (sequencep '(1)) (sequencep "a"))))"#;
    let expect =
        expect_test::expect![[r#""OK (t nil t t nil nil nil t t (t nil nil t t nil t t t))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Vectors as function arguments and return values, complex nesting
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_vector_ops_comp_nesting_and_higher_order() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"((require (quote cl-lib)) progn
  (require 'seq)
  (let* (;; Matrix as vector of vectors
         (matrix (vector (vector 1 2 3) (vector 4 5 6) (vector 7 8 9)))
         ;; Access element [1][2]
         (elem (aref (aref matrix 1) 2))
         ;; Transpose via seq-map
         (cols (let ((result nil))
                 (dotimes (j 3)
                   (let ((col (make-vector 3 0)))
                     (dotimes (i 3)
                       (aset col i (aref (aref matrix i) j)))
                     (setq result (cons col result))))
                 (vconcat (nreverse result))))
         ;; Flatten matrix to single vector
         (flat (apply #'vconcat (mapcar #'identity (append matrix nil))))
         ;; Sum all elements
         (total (seq-reduce #'+ flat 0))
         ;; Map over rows: sum each row
         (row-sums (seq-map (lambda (row) (seq-reduce #'+ row 0)) matrix)))
    (list elem cols flat total row-sums
          (length matrix) (length flat)
          ;; Nested vector modification
          (progn (aset (aref matrix 0) 0 100)
                 (aref (aref matrix 0) 0)))))"#;
    let expect = expect_test::expect![[r#""ERR (invalid-function (require 'cl-lib))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
