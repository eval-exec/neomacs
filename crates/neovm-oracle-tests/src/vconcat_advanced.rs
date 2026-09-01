//! Oracle parity tests for `vconcat` with ALL parameter combinations:
//! no args, single list/vector/string, mixed types, nil args, element type
//! preservation, and complex patterns like matrix flattening and interleaving.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// vconcat with no args (empty vector)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_vconcat_no_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
      (vconcat)
      (equal (vconcat) [])
      (length (vconcat))
      (vectorp (vconcat))
      ;; vconcat of empty sequences
      (vconcat [])
      (vconcat '())
      (vconcat "")
      (vconcat [] [] [])
      (vconcat '() '() '())
      (vconcat "" "" "")
      (vconcat [] '() "" [] '()))"#;
    let expect = expect_test::expect![[r#""OK ([] t 0 t [] [] [] [] [] [] [])""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// vconcat with single list/vector/string argument
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_vconcat_single_arg_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
      ;; Single vector (identity-like)
      (vconcat [1 2 3])
      (vconcat [a b c])
      (vconcat ["hello" "world"])
      ;; Single list
      (vconcat '(1 2 3))
      (vconcat '(a b c))
      (vconcat '("hello" "world"))
      ;; Single string (characters become integers)
      (vconcat "hello")
      (vconcat "ABC")
      (vconcat "")
      ;; Single nil (same as empty list)
      (vconcat nil)
      ;; Verify types are preserved inside the vector
      (let ((v (vconcat '(1 "two" 3.0 nil t sym))))
        (list (aref v 0)
              (aref v 1)
              (aref v 2)
              (aref v 3)
              (aref v 4)
              (aref v 5)
              (length v))))"#;
    let expect = expect_test::expect![[
        r#""OK ([1 2 3] [a b c] [\"hello\" \"world\"] [1 2 3] [a b c] [\"hello\" \"world\"] [104 101 108 108 111] [65 66 67] [] [] (1 \"two\" 3.0 nil t sym 6))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// vconcat with multiple arguments of mixed types
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_vconcat_mixed_type_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
      ;; Vector + list
      (vconcat [1 2] '(3 4))
      ;; List + vector
      (vconcat '(1 2) [3 4])
      ;; Vector + string
      (vconcat [1 2] "ab")
      ;; String + vector
      (vconcat "ab" [1 2])
      ;; List + string
      (vconcat '(1 2) "ab")
      ;; String + list
      (vconcat "ab" '(1 2))
      ;; Three different types
      (vconcat [1 2] '(3 4) "ef")
      ;; Many mixed args
      (vconcat [1] '(2) "3" [4] '(5) "6")
      ;; String + string (char codes)
      (vconcat "abc" "def")
      ;; All types together
      (vconcat [10 20] '(30 40) "AB" nil [50])
      ;; Order matters - verify left-to-right concatenation
      (equal (vconcat [1] [2] [3]) [1 2 3])
      (equal (vconcat '(a) '(b) '(c)) [a b c]))"#;
    let expect = expect_test::expect![[
        r#""OK ([1 2 3 4] [1 2 3 4] [1 2 97 98] [97 98 1 2] [1 2 97 98] [97 98 1 2] [1 2 3 4 101 102] [1 2 51 4 5 54] [97 98 99 100 101 102] [10 20 30 40 65 66 50] t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// vconcat preserving element types inside the result
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_vconcat_element_type_preservation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((v (vconcat [1 2.5 "str" nil t] '(sym (nested list)) [?a])))
      (list
        ;; Check each element type
        (integerp (aref v 0))
        (floatp (aref v 1))
        (stringp (aref v 2))
        (null (aref v 3))
        (eq t (aref v 4))
        (symbolp (aref v 5))
        (consp (aref v 6))
        (integerp (aref v 7))
        ;; Values
        (aref v 0)
        (aref v 1)
        (aref v 2)
        (aref v 3)
        (aref v 4)
        (aref v 5)
        (aref v 6)
        (aref v 7)
        (length v)
        ;; vconcat result is always a new vector (not eq)
        (let ((orig [1 2 3]))
          (eq orig (vconcat orig)))
        ;; But is equal
        (let ((orig [1 2 3]))
          (equal orig (vconcat orig)))))"#;
    let expect = expect_test::expect![[
        r#""OK (t t t t t t t t 1 2.5 \"str\" nil t sym (nested list) 97 8 nil t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// vconcat with nil arguments (nil treated as empty sequence)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_vconcat_nil_handling() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
      ;; nil alone
      (vconcat nil)
      ;; nil with other args
      (vconcat nil [1 2 3])
      (vconcat [1 2 3] nil)
      (vconcat nil nil nil)
      (vconcat nil [1] nil [2] nil [3] nil)
      ;; nil between other types
      (vconcat '(a b) nil [c d])
      (vconcat "ab" nil "cd")
      ;; nil is equivalent to empty list
      (equal (vconcat nil) (vconcat '()))
      (equal (vconcat [1] nil [2]) (vconcat [1] '() [2]))
      ;; Multiple nils should produce empty vector
      (equal (vconcat nil nil nil nil) [])
      (length (vconcat nil nil nil nil)))"#;
    let expect = expect_test::expect![[
        r#""OK ([] [1 2 3] [1 2 3] [] [1 2 3] [a b c d] [97 98 99 100] t t t 0)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: matrix flattening using vconcat
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_vconcat_matrix_flattening() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--test-make-matrix
    (lambda (rows cols)
      "Create a rows x cols matrix as vector of vectors, values = row*cols+col."
      (let ((m (make-vector rows nil)))
        (dotimes (r rows)
          (let ((row (make-vector cols 0)))
            (dotimes (c cols)
              (aset row c (+ (* r cols) c)))
            (aset m r row)))
        m)))

  (fset 'neovm--test-flatten-matrix
    (lambda (matrix)
      "Flatten a matrix (vector of vectors) into a single vector using vconcat."
      (let ((result []))
        (dotimes (i (length matrix))
          (setq result (vconcat result (aref matrix i))))
        result)))

  (fset 'neovm--test-matrix-row
    (lambda (flat cols row)
      "Extract row from flattened matrix."
      (let ((start (* row cols))
            (result []))
        (dotimes (c cols)
          (setq result (vconcat result (vector (aref flat (+ start c))))))
        result)))

  (fset 'neovm--test-matrix-col
    (lambda (flat cols rows col)
      "Extract column from flattened matrix."
      (let ((result []))
        (dotimes (r rows)
          (setq result (vconcat result (vector (aref flat (+ (* r cols) col))))))
        result)))

  (unwind-protect
      (let* ((m3x4 (funcall 'neovm--test-make-matrix 3 4))
             (flat (funcall 'neovm--test-flatten-matrix m3x4)))
        (list
          ;; Original matrix rows
          (aref m3x4 0)
          (aref m3x4 1)
          (aref m3x4 2)
          ;; Flattened result
          flat
          (length flat)
          ;; Extract rows from flattened
          (funcall 'neovm--test-matrix-row flat 4 0)
          (funcall 'neovm--test-matrix-row flat 4 1)
          (funcall 'neovm--test-matrix-row flat 4 2)
          ;; Extract columns from flattened
          (funcall 'neovm--test-matrix-col flat 4 3 0)
          (funcall 'neovm--test-matrix-col flat 4 3 1)
          ;; Flatten empty matrix
          (funcall 'neovm--test-flatten-matrix [])
          ;; Flatten single-row matrix
          (funcall 'neovm--test-flatten-matrix (vector [10 20 30]))))
    (fmakunbound 'neovm--test-make-matrix)
    (fmakunbound 'neovm--test-flatten-matrix)
    (fmakunbound 'neovm--test-matrix-row)
    (fmakunbound 'neovm--test-matrix-col)))"#;
    let expect = expect_test::expect![[
        r#""OK ([0 1 2 3] [4 5 6 7] [8 9 10 11] [0 1 2 3 4 5 6 7 8 9 10 11] 12 [0 1 2 3] [4 5 6 7] [8 9 10 11] [0 4 8] [1 5 9] [] [10 20 30])""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: vector interleaving and deinterleaving using vconcat
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_vconcat_interleave_deinterleave() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--test-interleave-n
    (lambda (vectors)
      "Interleave N vectors: take element 0 from each, then element 1, etc."
      (let* ((n (length vectors))
             (max-len (apply #'max (mapcar #'length vectors)))
             (result []))
        (dotimes (i max-len)
          (dotimes (j n)
            (let ((v (nth j vectors)))
              (when (< i (length v))
                (setq result (vconcat result (vector (aref v i))))))))
        result)))

  (fset 'neovm--test-deinterleave
    (lambda (vec n)
      "Deinterleave a vector into N vectors (round-robin split)."
      (let ((results (make-vector n [])))
        (dotimes (i (length vec))
          (let ((bucket (% i n)))
            (aset results bucket
                  (vconcat (aref results bucket) (vector (aref vec i))))))
        results)))

  (unwind-protect
      (let* ((a [1 2 3 4])
             (b [10 20 30 40])
             (c [100 200 300 400])
             (interleaved (funcall 'neovm--test-interleave-n (list a b c)))
             (deinterleaved (funcall 'neovm--test-deinterleave interleaved 3)))
        (list
          ;; Interleaved result
          interleaved
          ;; Deinterleaved should recover originals
          (aref deinterleaved 0)
          (aref deinterleaved 1)
          (aref deinterleaved 2)
          ;; Round-trip check
          (equal (aref deinterleaved 0) a)
          (equal (aref deinterleaved 1) b)
          (equal (aref deinterleaved 2) c)
          ;; Unequal length interleaving
          (funcall 'neovm--test-interleave-n (list [1 2 3] [a b] [x]))
          ;; Interleave with single vector
          (funcall 'neovm--test-interleave-n (list [1 2 3]))
          ;; Interleave two strings as char vectors
          (funcall 'neovm--test-interleave-n
                   (list (vconcat "abc") (vconcat "ABC")))))
    (fmakunbound 'neovm--test-interleave-n)
    (fmakunbound 'neovm--test-deinterleave)))"#;
    let expect = expect_test::expect![[
        r#""OK ([1 10 100 2 20 200 3 30 300 4 40 400] [1 2 3 4] [10 20 30 40] [100 200 300 400] t t t [1 a x 2 b 3] [1 2 3] [97 65 98 66 99 67])""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: run-length encoding/decoding using vconcat
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_vconcat_run_length_encoding() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--test-rle-encode
    (lambda (vec)
      "Run-length encode a vector: returns list of (value . count) pairs."
      (if (= (length vec) 0) nil
        (let ((result nil)
              (current (aref vec 0))
              (count 1))
          (let ((i 1))
            (while (< i (length vec))
              (if (equal (aref vec i) current)
                  (setq count (1+ count))
                (setq result (cons (cons current count) result))
                (setq current (aref vec i))
                (setq count 1))
              (setq i (1+ i))))
          (setq result (cons (cons current count) result))
          (nreverse result)))))

  (fset 'neovm--test-rle-decode
    (lambda (pairs)
      "Decode run-length encoded pairs back to a vector using vconcat."
      (let ((result []))
        (dolist (pair pairs)
          (let ((val (car pair))
                (cnt (cdr pair)))
            (setq result (vconcat result (make-vector cnt val)))))
        result)))

  (unwind-protect
      (let* ((original [1 1 1 2 2 3 3 3 3 4 5 5])
             (encoded (funcall 'neovm--test-rle-encode original))
             (decoded (funcall 'neovm--test-rle-decode encoded)))
        (list
          encoded
          decoded
          (equal original decoded)
          ;; Single element
          (funcall 'neovm--test-rle-encode [42])
          (funcall 'neovm--test-rle-decode '((42 . 1)))
          ;; All same
          (funcall 'neovm--test-rle-encode [a a a a a])
          (funcall 'neovm--test-rle-decode '((a . 5)))
          ;; All different
          (funcall 'neovm--test-rle-encode [1 2 3 4 5])
          ;; Empty
          (funcall 'neovm--test-rle-encode [])
          (funcall 'neovm--test-rle-decode nil)
          ;; Round-trip with symbols
          (let* ((sym-vec [x x y z z z])
                 (enc (funcall 'neovm--test-rle-encode sym-vec))
                 (dec (funcall 'neovm--test-rle-decode enc)))
            (list enc (equal sym-vec dec)))))
    (fmakunbound 'neovm--test-rle-encode)
    (fmakunbound 'neovm--test-rle-decode)))"#;
    let expect = expect_test::expect![[
        r#""OK (((1 . 3) (2 . 2) (3 . 4) (4 . 1) (5 . 2)) [1 1 1 2 2 3 3 3 3 4 5 5] t ((42 . 1)) [42] ((a . 5)) [a a a a a] ((1 . 1) (2 . 1) (3 . 1) (4 . 1) (5 . 1)) nil [] (((x . 2) (y . 1) (z . 3)) t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
