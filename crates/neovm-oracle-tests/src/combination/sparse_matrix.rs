//! Oracle parity tests for sparse matrix operations using alists in pure Elisp.
//!
//! Implements: sparse matrix creation, set/get elements, addition, scalar
//! multiplication, transpose, matrix-vector multiply, count non-zeros,
//! and conversion to dense representation. Tests with matrices that have
//! mostly zero entries.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

// ---------------------------------------------------------------------------
// Sparse matrix creation, set, get operations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sparse_matrix_basic_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Sparse matrix represented as (rows cols entries) where entries is an
    // alist of ((row . col) . value) pairs. Only non-zero values stored.
    let form = r#"(progn
  ;; Create empty sparse matrix with given dimensions
  (fset 'neovm--sm-create
    (lambda (rows cols)
      (list rows cols nil)))

  (fset 'neovm--sm-rows (lambda (m) (nth 0 m)))
  (fset 'neovm--sm-cols (lambda (m) (nth 1 m)))
  (fset 'neovm--sm-entries (lambda (m) (nth 2 m)))

  ;; Set element at (r, c) to val. If val is 0, remove the entry.
  (fset 'neovm--sm-set
    (lambda (m r c val)
      (let* ((key (cons r c))
             (entries (nth 2 m))
             (existing (assoc key entries)))
        (if (= val 0)
            ;; Remove entry if it exists
            (list (nth 0 m) (nth 1 m)
                  (if existing
                      (let ((result nil))
                        (dolist (e entries)
                          (unless (equal (car e) key)
                            (setq result (cons e result))))
                        (nreverse result))
                    entries))
          ;; Set or update entry
          (if existing
              (progn (setcdr existing val) m)
            (list (nth 0 m) (nth 1 m)
                  (cons (cons key val) entries)))))))

  ;; Get element at (r, c). Returns 0 if not present.
  (fset 'neovm--sm-get
    (lambda (m r c)
      (let ((entry (assoc (cons r c) (nth 2 m))))
        (if entry (cdr entry) 0))))

  ;; Count non-zero entries
  (fset 'neovm--sm-nnz
    (lambda (m)
      (length (nth 2 m))))

  (unwind-protect
      (let ((m (funcall 'neovm--sm-create 5 5)))
        ;; Set some sparse entries in a 5x5 matrix
        (setq m (funcall 'neovm--sm-set m 0 0 10))
        (setq m (funcall 'neovm--sm-set m 0 3 20))
        (setq m (funcall 'neovm--sm-set m 1 1 30))
        (setq m (funcall 'neovm--sm-set m 2 4 40))
        (setq m (funcall 'neovm--sm-set m 4 2 50))
        (list
         ;; Dimensions
         (funcall 'neovm--sm-rows m)
         (funcall 'neovm--sm-cols m)
         ;; Get stored values
         (funcall 'neovm--sm-get m 0 0)
         (funcall 'neovm--sm-get m 0 3)
         (funcall 'neovm--sm-get m 1 1)
         (funcall 'neovm--sm-get m 2 4)
         (funcall 'neovm--sm-get m 4 2)
         ;; Get zero entries (not stored)
         (funcall 'neovm--sm-get m 0 1)
         (funcall 'neovm--sm-get m 3 3)
         (funcall 'neovm--sm-get m 4 4)
         ;; Count non-zeros
         (funcall 'neovm--sm-nnz m)
         ;; Update existing entry
         (let ((m2 (funcall 'neovm--sm-set m 0 0 99)))
           (funcall 'neovm--sm-get m2 0 0))
         ;; Set to zero removes entry
         (let ((m3 (funcall 'neovm--sm-set m 1 1 0)))
           (list (funcall 'neovm--sm-get m3 1 1)
                 (funcall 'neovm--sm-nnz m3)))))
    (fmakunbound 'neovm--sm-create)
    (fmakunbound 'neovm--sm-rows)
    (fmakunbound 'neovm--sm-cols)
    (fmakunbound 'neovm--sm-entries)
    (fmakunbound 'neovm--sm-set)
    (fmakunbound 'neovm--sm-get)
    (fmakunbound 'neovm--sm-nnz)))"#;
    let expect = expect_test::expect![[r#""OK (5 5 10 20 30 40 50 0 0 0 5 99 (0 4))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Sparse matrix addition
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sparse_matrix_addition() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Add two sparse matrices: merge entries, summing values at shared positions.
    // Remove entries where sum is zero.
    let form = r#"(progn
  (fset 'neovm--sma-create (lambda (rows cols) (list rows cols nil)))

  (fset 'neovm--sma-set
    (lambda (m r c val)
      (let* ((key (cons r c))
             (entries (nth 2 m))
             (existing (assoc key entries)))
        (if (= val 0)
            (list (nth 0 m) (nth 1 m)
                  (let ((result nil))
                    (dolist (e entries)
                      (unless (equal (car e) key)
                        (setq result (cons e result))))
                    (nreverse result)))
          (if existing
              (progn (setcdr existing val) m)
            (list (nth 0 m) (nth 1 m)
                  (cons (cons key val) entries)))))))

  (fset 'neovm--sma-get
    (lambda (m r c)
      (let ((entry (assoc (cons r c) (nth 2 m))))
        (if entry (cdr entry) 0))))

  (fset 'neovm--sma-nnz (lambda (m) (length (nth 2 m))))

  ;; Add two sparse matrices
  (fset 'neovm--sma-add
    (lambda (a b)
      (let ((result (funcall 'neovm--sma-create (nth 0 a) (nth 1 a))))
        ;; Add all entries from A
        (dolist (entry (nth 2 a))
          (let* ((key (car entry))
                 (val (cdr entry)))
            (setq result (funcall 'neovm--sma-set result (car key) (cdr key) val))))
        ;; Add all entries from B (summing with existing)
        (dolist (entry (nth 2 b))
          (let* ((key (car entry))
                 (r (car key))
                 (c (cdr key))
                 (val (cdr entry))
                 (existing (funcall 'neovm--sma-get result r c))
                 (new-val (+ existing val)))
            (setq result (funcall 'neovm--sma-set result r c new-val))))
        result)))

  (unwind-protect
      (let ((a (funcall 'neovm--sma-create 3 3))
            (b (funcall 'neovm--sma-create 3 3)))
        ;; Matrix A: sparse diagonal
        (setq a (funcall 'neovm--sma-set a 0 0 1))
        (setq a (funcall 'neovm--sma-set a 1 1 2))
        (setq a (funcall 'neovm--sma-set a 2 2 3))
        ;; Matrix B: sparse off-diagonal + one shared position
        (setq b (funcall 'neovm--sma-set b 0 1 4))
        (setq b (funcall 'neovm--sma-set b 1 1 -2))  ;; cancels A's (1,1)
        (setq b (funcall 'neovm--sma-set b 2 0 5))
        (let ((sum (funcall 'neovm--sma-add a b)))
          (list
           ;; Values in sum
           (funcall 'neovm--sma-get sum 0 0)  ;; 1 + 0 = 1
           (funcall 'neovm--sma-get sum 0 1)  ;; 0 + 4 = 4
           (funcall 'neovm--sma-get sum 1 1)  ;; 2 + (-2) = 0
           (funcall 'neovm--sma-get sum 2 0)  ;; 0 + 5 = 5
           (funcall 'neovm--sma-get sum 2 2)  ;; 3 + 0 = 3
           ;; Zero positions
           (funcall 'neovm--sma-get sum 0 2)
           (funcall 'neovm--sma-get sum 1 0)
           ;; NNZ: (1,1) cancelled, so 4 entries remain
           (funcall 'neovm--sma-nnz sum)
           ;; Addition is commutative
           (let ((sum2 (funcall 'neovm--sma-add b a)))
             (and (= (funcall 'neovm--sma-get sum2 0 0)
                     (funcall 'neovm--sma-get sum 0 0))
                  (= (funcall 'neovm--sma-get sum2 0 1)
                     (funcall 'neovm--sma-get sum 0 1))
                  (= (funcall 'neovm--sma-get sum2 2 0)
                     (funcall 'neovm--sma-get sum 2 0)))))))
    (fmakunbound 'neovm--sma-create)
    (fmakunbound 'neovm--sma-set)
    (fmakunbound 'neovm--sma-get)
    (fmakunbound 'neovm--sma-nnz)
    (fmakunbound 'neovm--sma-add)))"#;
    let expect = expect_test::expect![[r#""OK (1 4 0 5 3 0 0 4 t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Sparse matrix scalar multiplication and transpose
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sparse_matrix_scale_and_transpose() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Scalar multiplication: multiply all values by scalar.
    // Transpose: swap row/col indices for every entry.
    let form = r#"(progn
  (fset 'neovm--smt-create (lambda (rows cols) (list rows cols nil)))

  (fset 'neovm--smt-set
    (lambda (m r c val)
      (let* ((key (cons r c)) (entries (nth 2 m)))
        (if (= val 0)
            (list (nth 0 m) (nth 1 m)
                  (let ((result nil))
                    (dolist (e entries)
                      (unless (equal (car e) key)
                        (setq result (cons e result))))
                    (nreverse result)))
          (let ((existing (assoc key entries)))
            (if existing
                (progn (setcdr existing val) m)
              (list (nth 0 m) (nth 1 m)
                    (cons (cons key val) entries))))))))

  (fset 'neovm--smt-get
    (lambda (m r c)
      (let ((entry (assoc (cons r c) (nth 2 m))))
        (if entry (cdr entry) 0))))

  (fset 'neovm--smt-nnz (lambda (m) (length (nth 2 m))))

  ;; Scalar multiplication
  (fset 'neovm--smt-scale
    (lambda (scalar m)
      (if (= scalar 0)
          (funcall 'neovm--smt-create (nth 0 m) (nth 1 m))
        (let ((result (funcall 'neovm--smt-create (nth 0 m) (nth 1 m))))
          (dolist (entry (nth 2 m))
            (let* ((key (car entry))
                   (val (* scalar (cdr entry))))
              (setq result (funcall 'neovm--smt-set result
                                    (car key) (cdr key) val))))
          result))))

  ;; Transpose: swap row and col in each entry key
  (fset 'neovm--smt-transpose
    (lambda (m)
      (let ((result (funcall 'neovm--smt-create (nth 1 m) (nth 0 m))))
        (dolist (entry (nth 2 m))
          (let* ((key (car entry))
                 (r (car key))
                 (c (cdr key))
                 (val (cdr entry)))
            (setq result (funcall 'neovm--smt-set result c r val))))
        result)))

  (unwind-protect
      (let ((m (funcall 'neovm--smt-create 3 4)))
        (setq m (funcall 'neovm--smt-set m 0 0 2))
        (setq m (funcall 'neovm--smt-set m 0 3 5))
        (setq m (funcall 'neovm--smt-set m 1 2 7))
        (setq m (funcall 'neovm--smt-set m 2 1 -3))
        (let ((scaled (funcall 'neovm--smt-scale 3 m))
              (negated (funcall 'neovm--smt-scale -1 m))
              (zeroed (funcall 'neovm--smt-scale 0 m))
              (transposed (funcall 'neovm--smt-transpose m)))
          (list
           ;; Scaled by 3
           (funcall 'neovm--smt-get scaled 0 0)   ;; 6
           (funcall 'neovm--smt-get scaled 0 3)   ;; 15
           (funcall 'neovm--smt-get scaled 1 2)   ;; 21
           (funcall 'neovm--smt-get scaled 2 1)   ;; -9
           (funcall 'neovm--smt-nnz scaled)        ;; 4
           ;; Negated
           (funcall 'neovm--smt-get negated 0 0)  ;; -2
           (funcall 'neovm--smt-get negated 2 1)  ;; 3
           ;; Zeroed: all gone
           (funcall 'neovm--smt-nnz zeroed)        ;; 0
           ;; Transpose: dimensions swapped
           (funcall 'neovm--smt-get transposed 0 0)  ;; was (0,0)=2
           (funcall 'neovm--smt-get transposed 3 0)  ;; was (0,3)=5
           (funcall 'neovm--smt-get transposed 2 1)  ;; was (1,2)=7
           (funcall 'neovm--smt-get transposed 1 2)  ;; was (2,1)=-3
           ;; Transpose dimensions
           (nth 0 transposed)  ;; 4 (was 3 cols)
           (nth 1 transposed)  ;; 3 (was 4 rows... wait: rows=3,cols=4, transposed=(cols,rows)=(4,3))
           ;; Double transpose = original values
           (let ((tt (funcall 'neovm--smt-transpose transposed)))
             (and (= (funcall 'neovm--smt-get tt 0 0) 2)
                  (= (funcall 'neovm--smt-get tt 0 3) 5)
                  (= (funcall 'neovm--smt-get tt 1 2) 7)
                  (= (funcall 'neovm--smt-get tt 2 1) -3))))))
    (fmakunbound 'neovm--smt-create)
    (fmakunbound 'neovm--smt-set)
    (fmakunbound 'neovm--smt-get)
    (fmakunbound 'neovm--smt-nnz)
    (fmakunbound 'neovm--smt-scale)
    (fmakunbound 'neovm--smt-transpose)))"#;
    let expect = expect_test::expect![[r#""OK (6 15 21 -9 4 -2 3 0 2 5 7 -3 4 3 t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Sparse matrix-vector multiplication
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sparse_matrix_vector_multiply() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Multiply sparse matrix (m x n) by dense vector (n x 1) -> dense vector (m x 1).
    // Only iterate over non-zero entries.
    let form = r#"(progn
  (fset 'neovm--smv-create (lambda (rows cols) (list rows cols nil)))

  (fset 'neovm--smv-set
    (lambda (m r c val)
      (let* ((key (cons r c)) (entries (nth 2 m)))
        (if (= val 0)
            (list (nth 0 m) (nth 1 m)
                  (let ((result nil))
                    (dolist (e entries)
                      (unless (equal (car e) key)
                        (setq result (cons e result))))
                    (nreverse result)))
          (let ((existing (assoc key entries)))
            (if existing
                (progn (setcdr existing val) m)
              (list (nth 0 m) (nth 1 m)
                    (cons (cons key val) entries))))))))

  (fset 'neovm--smv-get
    (lambda (m r c)
      (let ((entry (assoc (cons r c) (nth 2 m))))
        (if entry (cdr entry) 0))))

  ;; Sparse matrix * dense vector multiplication
  ;; vec is a plain list of numbers
  (fset 'neovm--smv-matvec
    (lambda (m vec)
      "Multiply sparse matrix M by dense vector VEC."
      (let* ((rows (nth 0 m))
             (result (make-list rows 0)))
        ;; For each non-zero entry, accumulate into result
        (dolist (entry (nth 2 m))
          (let* ((key (car entry))
                 (r (car key))
                 (c (cdr key))
                 (val (cdr entry))
                 (vec-val (nth c vec)))
            (setcar (nthcdr r result)
                    (+ (nth r result) (* val vec-val)))))
        result)))

  ;; Dense matrix-vector multiply for verification
  (fset 'neovm--smv-dense-matvec
    (lambda (m rows cols vec)
      (let ((result nil)
            (r 0))
        (while (< r rows)
          (let ((sum 0) (c 0))
            (while (< c cols)
              (setq sum (+ sum (* (funcall 'neovm--smv-get m r c)
                                  (nth c vec))))
              (setq c (1+ c)))
            (setq result (cons sum result)))
          (setq r (1+ r)))
        (nreverse result))))

  (unwind-protect
      (let ((m (funcall 'neovm--smv-create 4 4)))
        ;; Sparse 4x4 matrix (only 5 non-zero entries out of 16)
        (setq m (funcall 'neovm--smv-set m 0 0 2))
        (setq m (funcall 'neovm--smv-set m 0 2 1))
        (setq m (funcall 'neovm--smv-set m 1 1 3))
        (setq m (funcall 'neovm--smv-set m 2 3 -1))
        (setq m (funcall 'neovm--smv-set m 3 0 4))
        (let ((v1 '(1 2 3 4))
              (v2 '(0 0 0 0))
              (v3 '(1 0 0 0))
              (v4 '(1 1 1 1)))
          (list
           ;; M * v1
           (funcall 'neovm--smv-matvec m v1)
           ;; M * zero vector = zero vector
           (funcall 'neovm--smv-matvec m v2)
           ;; M * e1 (first standard basis) = first column
           (funcall 'neovm--smv-matvec m v3)
           ;; M * ones
           (funcall 'neovm--smv-matvec m v4)
           ;; Verify sparse == dense multiplication
           (equal (funcall 'neovm--smv-matvec m v1)
                  (funcall 'neovm--smv-dense-matvec m 4 4 v1))
           (equal (funcall 'neovm--smv-matvec m v4)
                  (funcall 'neovm--smv-dense-matvec m 4 4 v4))
           ;; Rectangular matrix: 2x3 * 3-vector
           (let ((rect (funcall 'neovm--smv-create 2 3)))
             (setq rect (funcall 'neovm--smv-set rect 0 1 5))
             (setq rect (funcall 'neovm--smv-set rect 1 2 7))
             (funcall 'neovm--smv-matvec rect '(1 2 3))))))
    (fmakunbound 'neovm--smv-create)
    (fmakunbound 'neovm--smv-set)
    (fmakunbound 'neovm--smv-get)
    (fmakunbound 'neovm--smv-matvec)
    (fmakunbound 'neovm--smv-dense-matvec)))"#;
    let expect =
        expect_test::expect![[r#""OK ((5 6 -4 4) (0 0 0 0) (2 0 0 4) (3 3 -1 4) t t (10 21))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Convert sparse matrix to/from dense representation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sparse_matrix_dense_conversion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Convert sparse to dense (list of rows), and dense to sparse.
    // Verify round-trip preserves values.
    let form = r#"(progn
  (fset 'neovm--smd-create (lambda (rows cols) (list rows cols nil)))

  (fset 'neovm--smd-set
    (lambda (m r c val)
      (let* ((key (cons r c)) (entries (nth 2 m)))
        (if (= val 0)
            (list (nth 0 m) (nth 1 m)
                  (let ((result nil))
                    (dolist (e entries)
                      (unless (equal (car e) key)
                        (setq result (cons e result))))
                    (nreverse result)))
          (let ((existing (assoc key entries)))
            (if existing
                (progn (setcdr existing val) m)
              (list (nth 0 m) (nth 1 m)
                    (cons (cons key val) entries))))))))

  (fset 'neovm--smd-get
    (lambda (m r c)
      (let ((entry (assoc (cons r c) (nth 2 m))))
        (if entry (cdr entry) 0))))

  (fset 'neovm--smd-nnz (lambda (m) (length (nth 2 m))))

  ;; Sparse -> dense: produce list of rows
  (fset 'neovm--smd-to-dense
    (lambda (m)
      (let ((rows (nth 0 m))
            (cols (nth 1 m))
            (result nil)
            (r 0))
        (while (< r rows)
          (let ((row nil) (c 0))
            (while (< c cols)
              (setq row (cons (funcall 'neovm--smd-get m r c) row))
              (setq c (1+ c)))
            (setq result (cons (nreverse row) result)))
          (setq r (1+ r)))
        (nreverse result))))

  ;; Dense -> sparse: scan nested list for non-zeros
  (fset 'neovm--smd-from-dense
    (lambda (dense)
      (let* ((rows (length dense))
             (cols (length (car dense)))
             (m (funcall 'neovm--smd-create rows cols))
             (r 0))
        (dolist (row dense)
          (let ((c 0))
            (dolist (val row)
              (unless (= val 0)
                (setq m (funcall 'neovm--smd-set m r c val)))
              (setq c (1+ c))))
          (setq r (1+ r)))
        m)))

  (unwind-protect
      (let ((m (funcall 'neovm--smd-create 3 4)))
        ;; Very sparse: only 3 entries in a 3x4 matrix (12 elements)
        (setq m (funcall 'neovm--smd-set m 0 2 8))
        (setq m (funcall 'neovm--smd-set m 1 0 -5))
        (setq m (funcall 'neovm--smd-set m 2 3 12))
        (let ((dense (funcall 'neovm--smd-to-dense m)))
          (list
           ;; Dense representation
           dense
           ;; Expected: ((0 0 8 0) (-5 0 0 0) (0 0 0 12))
           ;; Round-trip: dense -> sparse -> dense should match
           (let* ((m2 (funcall 'neovm--smd-from-dense dense))
                  (dense2 (funcall 'neovm--smd-to-dense m2)))
             (equal dense dense2))
           ;; NNZ preserved in round-trip
           (let ((m2 (funcall 'neovm--smd-from-dense dense)))
             (= (funcall 'neovm--smd-nnz m2) (funcall 'neovm--smd-nnz m)))
           ;; Dense of zero matrix
           (funcall 'neovm--smd-to-dense (funcall 'neovm--smd-create 2 3))
           ;; Dense identity-like sparse
           (let ((eye (funcall 'neovm--smd-create 3 3)))
             (setq eye (funcall 'neovm--smd-set eye 0 0 1))
             (setq eye (funcall 'neovm--smd-set eye 1 1 1))
             (setq eye (funcall 'neovm--smd-set eye 2 2 1))
             (funcall 'neovm--smd-to-dense eye))
           ;; From dense with all zeros -> empty sparse
           (funcall 'neovm--smd-nnz
                    (funcall 'neovm--smd-from-dense '((0 0) (0 0)))))))
    (fmakunbound 'neovm--smd-create)
    (fmakunbound 'neovm--smd-set)
    (fmakunbound 'neovm--smd-get)
    (fmakunbound 'neovm--smd-nnz)
    (fmakunbound 'neovm--smd-to-dense)
    (fmakunbound 'neovm--smd-from-dense)))"#;
    let expect = expect_test::expect![[
        r#""OK (((0 0 8 0) (-5 0 0 0) (0 0 0 12)) t t ((0 0 0) (0 0 0)) ((1 0 0) (0 1 0) (0 0 1)) 0)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Sparse matrix-matrix multiplication
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sparse_matrix_multiplication() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Multiply two sparse matrices: C[i,j] = sum_k A[i,k] * B[k,j].
    // Only iterate over non-zero entries of A and B.
    let form = r#"(progn
  (fset 'neovm--smm-create (lambda (rows cols) (list rows cols nil)))

  (fset 'neovm--smm-set
    (lambda (m r c val)
      (let* ((key (cons r c)) (entries (nth 2 m)))
        (if (= val 0)
            (list (nth 0 m) (nth 1 m)
                  (let ((result nil))
                    (dolist (e entries)
                      (unless (equal (car e) key)
                        (setq result (cons e result))))
                    (nreverse result)))
          (let ((existing (assoc key entries)))
            (if existing
                (progn (setcdr existing val) m)
              (list (nth 0 m) (nth 1 m)
                    (cons (cons key val) entries))))))))

  (fset 'neovm--smm-get
    (lambda (m r c)
      (let ((entry (assoc (cons r c) (nth 2 m))))
        (if entry (cdr entry) 0))))

  (fset 'neovm--smm-nnz (lambda (m) (length (nth 2 m))))

  ;; Sparse matrix multiply: for each entry (i,k,v1) in A and (k,j,v2) in B,
  ;; accumulate v1*v2 into C[i,j].
  (fset 'neovm--smm-multiply
    (lambda (a b)
      (let* ((rows-a (nth 0 a))
             (cols-b (nth 1 b))
             ;; Build a hash-table keyed by row of B for fast lookup
             (b-by-row (make-hash-table :test 'equal))
             (result (funcall 'neovm--smm-create rows-a cols-b)))
        ;; Index B entries by their row
        (dolist (entry (nth 2 b))
          (let* ((key (car entry))
                 (br (car key))
                 (bc (cdr key))
                 (bv (cdr entry))
                 (existing (gethash br b-by-row)))
            (puthash br (cons (cons bc bv) existing) b-by-row)))
        ;; For each entry in A, multiply with matching B entries
        (dolist (a-entry (nth 2 a))
          (let* ((a-key (car a-entry))
                 (ar (car a-key))
                 (ac (cdr a-key))
                 (av (cdr a-entry))
                 ;; B entries where B's row == A's col
                 (b-entries (gethash ac b-by-row)))
            (dolist (b-pair b-entries)
              (let* ((bc (car b-pair))
                     (bv (cdr b-pair))
                     (product (* av bv))
                     (cur (funcall 'neovm--smm-get result ar bc)))
                (setq result (funcall 'neovm--smm-set result ar bc
                                      (+ cur product)))))))
        result)))

  ;; Dense conversion for verification
  (fset 'neovm--smm-to-dense
    (lambda (m)
      (let ((result nil) (r 0))
        (while (< r (nth 0 m))
          (let ((row nil) (c 0))
            (while (< c (nth 1 m))
              (setq row (cons (funcall 'neovm--smm-get m r c) row))
              (setq c (1+ c)))
            (setq result (cons (nreverse row) result)))
          (setq r (1+ r)))
        (nreverse result))))

  (unwind-protect
      (let ((a (funcall 'neovm--smm-create 2 3))
            (b (funcall 'neovm--smm-create 3 2)))
        ;; A: 2x3 sparse
        (setq a (funcall 'neovm--smm-set a 0 0 1))
        (setq a (funcall 'neovm--smm-set a 0 2 3))
        (setq a (funcall 'neovm--smm-set a 1 1 2))
        ;; B: 3x2 sparse
        (setq b (funcall 'neovm--smm-set b 0 0 4))
        (setq b (funcall 'neovm--smm-set b 1 1 5))
        (setq b (funcall 'neovm--smm-set b 2 0 6))
        (let ((c (funcall 'neovm--smm-multiply a b)))
          (list
           ;; Result as dense: A*B should be 2x2
           ;; C[0,0] = 1*4 + 0*0 + 3*6 = 22
           ;; C[0,1] = 1*0 + 0*5 + 3*0 = 0
           ;; C[1,0] = 0*4 + 2*0 + 0*6 = 0
           ;; C[1,1] = 0*0 + 2*5 + 0*0 = 10
           (funcall 'neovm--smm-to-dense c)
           ;; Individual element checks
           (funcall 'neovm--smm-get c 0 0)  ;; 22
           (funcall 'neovm--smm-get c 0 1)  ;; 0
           (funcall 'neovm--smm-get c 1 0)  ;; 0
           (funcall 'neovm--smm-get c 1 1)  ;; 10
           ;; NNZ of result
           (funcall 'neovm--smm-nnz c)
           ;; Multiply by identity-like sparse
           (let ((eye (funcall 'neovm--smm-create 3 3)))
             (setq eye (funcall 'neovm--smm-set eye 0 0 1))
             (setq eye (funcall 'neovm--smm-set eye 1 1 1))
             (setq eye (funcall 'neovm--smm-set eye 2 2 1))
             ;; A * I = A
             (let ((ai (funcall 'neovm--smm-multiply a eye)))
               (funcall 'neovm--smm-to-dense ai))))))
    (fmakunbound 'neovm--smm-create)
    (fmakunbound 'neovm--smm-set)
    (fmakunbound 'neovm--smm-get)
    (fmakunbound 'neovm--smm-nnz)
    (fmakunbound 'neovm--smm-multiply)
    (fmakunbound 'neovm--smm-to-dense)))"#;
    let expect = expect_test::expect![[r#""OK (((22 0) (0 10)) 22 0 0 10 2 ((1 0 3) (0 2 0)))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Sparse matrix row/column operations and sparsity analysis
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sparse_matrix_row_col_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Extract rows/columns, compute row sums, column sums,
    // and sparsity ratio for a large sparse matrix.
    let form = r#"(progn
  (fset 'neovm--smr-create (lambda (rows cols) (list rows cols nil)))

  (fset 'neovm--smr-set
    (lambda (m r c val)
      (let* ((key (cons r c)) (entries (nth 2 m)))
        (if (= val 0)
            (list (nth 0 m) (nth 1 m)
                  (let ((result nil))
                    (dolist (e entries)
                      (unless (equal (car e) key)
                        (setq result (cons e result))))
                    (nreverse result)))
          (let ((existing (assoc key entries)))
            (if existing (progn (setcdr existing val) m)
              (list (nth 0 m) (nth 1 m)
                    (cons (cons key val) entries))))))))

  (fset 'neovm--smr-get
    (lambda (m r c)
      (let ((entry (assoc (cons r c) (nth 2 m))))
        (if entry (cdr entry) 0))))

  (fset 'neovm--smr-nnz (lambda (m) (length (nth 2 m))))

  ;; Get all entries in a specific row as alist of (col . val)
  (fset 'neovm--smr-get-row
    (lambda (m r)
      (let ((result nil))
        (dolist (entry (nth 2 m))
          (when (= (car (car entry)) r)
            (setq result (cons (cons (cdr (car entry)) (cdr entry)) result))))
        (sort result (lambda (a b) (< (car a) (car b)))))))

  ;; Get all entries in a specific column as alist of (row . val)
  (fset 'neovm--smr-get-col
    (lambda (m c)
      (let ((result nil))
        (dolist (entry (nth 2 m))
          (when (= (cdr (car entry)) c)
            (setq result (cons (cons (car (car entry)) (cdr entry)) result))))
        (sort result (lambda (a b) (< (car a) (car b)))))))

  ;; Row sums: sum of all entries in each row
  (fset 'neovm--smr-row-sums
    (lambda (m)
      (let ((sums (make-list (nth 0 m) 0)))
        (dolist (entry (nth 2 m))
          (let ((r (car (car entry)))
                (val (cdr entry)))
            (setcar (nthcdr r sums) (+ (nth r sums) val))))
        sums)))

  ;; Column sums
  (fset 'neovm--smr-col-sums
    (lambda (m)
      (let ((sums (make-list (nth 1 m) 0)))
        (dolist (entry (nth 2 m))
          (let ((c (cdr (car entry)))
                (val (cdr entry)))
            (setcar (nthcdr c sums) (+ (nth c sums) val))))
        sums)))

  ;; Sparsity: fraction of zeros (as percentage integer)
  (fset 'neovm--smr-sparsity-pct
    (lambda (m)
      (let* ((total (* (nth 0 m) (nth 1 m)))
             (nnz (funcall 'neovm--smr-nnz m)))
        (/ (* (- total nnz) 100) total))))

  (unwind-protect
      (let ((m (funcall 'neovm--smr-create 5 6)))
        ;; Populate a 5x6 matrix with 7 non-zeros (out of 30 total)
        (setq m (funcall 'neovm--smr-set m 0 0 1))
        (setq m (funcall 'neovm--smr-set m 0 5 2))
        (setq m (funcall 'neovm--smr-set m 1 2 3))
        (setq m (funcall 'neovm--smr-set m 2 2 4))
        (setq m (funcall 'neovm--smr-set m 2 4 5))
        (setq m (funcall 'neovm--smr-set m 3 1 6))
        (setq m (funcall 'neovm--smr-set m 4 3 7))
        (list
         ;; Row extraction
         (funcall 'neovm--smr-get-row m 0)   ;; ((0 . 1) (5 . 2))
         (funcall 'neovm--smr-get-row m 2)   ;; ((2 . 4) (4 . 5))
         (funcall 'neovm--smr-get-row m 3)   ;; ((1 . 6))
         ;; Empty row
         (funcall 'neovm--smr-get-row m 4)   ;; ((3 . 7)) -- not empty, row 4 has entry
         ;; Column extraction
         (funcall 'neovm--smr-get-col m 2)   ;; ((1 . 3) (2 . 4))
         (funcall 'neovm--smr-get-col m 0)   ;; ((0 . 1))
         ;; Empty column
         (funcall 'neovm--smr-get-col m 5)   ;; ((0 . 2))
         ;; Row sums
         (funcall 'neovm--smr-row-sums m)    ;; (3 3 9 6 7)
         ;; Column sums
         (funcall 'neovm--smr-col-sums m)    ;; (1 6 7 7 5 2)
         ;; Sparsity
         (funcall 'neovm--smr-sparsity-pct m)  ;; (30-7)*100/30 = 76
         ;; NNZ
         (funcall 'neovm--smr-nnz m)))
    (fmakunbound 'neovm--smr-create)
    (fmakunbound 'neovm--smr-set)
    (fmakunbound 'neovm--smr-get)
    (fmakunbound 'neovm--smr-nnz)
    (fmakunbound 'neovm--smr-get-row)
    (fmakunbound 'neovm--smr-get-col)
    (fmakunbound 'neovm--smr-row-sums)
    (fmakunbound 'neovm--smr-col-sums)
    (fmakunbound 'neovm--smr-sparsity-pct)))"#;
    let expect = expect_test::expect![[
        r#""OK (((0 . 1) (5 . 2)) ((2 . 4) (4 . 5)) ((1 . 6)) ((3 . 7)) ((1 . 3) (2 . 4)) ((0 . 1)) ((0 . 2)) (3 3 9 6 7) (1 6 7 7 5 2) 76 7)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
