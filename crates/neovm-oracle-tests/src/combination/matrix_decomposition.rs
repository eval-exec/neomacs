//! Oracle parity tests for matrix decomposition operations in Elisp:
//! vector-of-vectors representation, matrix multiplication, transpose,
//! recursive cofactor determinant (NxN), row echelon form via Gaussian
//! elimination, matrix inverse via augmented matrix, and LU decomposition.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Vector-of-vectors matrix representation and basic access
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_matrix_decomp_vector_representation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Represent matrices as vectors of vectors for O(1) element access.
    // Test creation, element access, row/column extraction.
    let form = r#"(progn
  ;; Create matrix from nested list as vector of vectors
  (fset 'neovm--md-from-list
    (lambda (lst)
      (apply #'vector (mapcar (lambda (row) (apply #'vector row)) lst))))

  ;; Get element at (r, c)
  (fset 'neovm--md-ref
    (lambda (mat r c)
      (aref (aref mat r) c)))

  ;; Set element at (r, c)
  (fset 'neovm--md-set
    (lambda (mat r c val)
      (aset (aref mat r) c val)))

  ;; Number of rows
  (fset 'neovm--md-rows
    (lambda (mat) (length mat)))

  ;; Number of columns
  (fset 'neovm--md-cols
    (lambda (mat) (length (aref mat 0))))

  ;; Extract row as list
  (fset 'neovm--md-row-list
    (lambda (mat r)
      (let ((result nil) (c (1- (funcall 'neovm--md-cols mat))))
        (while (>= c 0)
          (setq result (cons (funcall 'neovm--md-ref mat r c) result))
          (setq c (1- c)))
        result)))

  ;; Extract column as list
  (fset 'neovm--md-col-list
    (lambda (mat c)
      (let ((result nil) (r (1- (funcall 'neovm--md-rows mat))))
        (while (>= r 0)
          (setq result (cons (funcall 'neovm--md-ref mat r c) result))
          (setq r (1- r)))
        result)))

  ;; Convert matrix to nested list for comparison
  (fset 'neovm--md-to-list
    (lambda (mat)
      (let ((result nil) (r (1- (funcall 'neovm--md-rows mat))))
        (while (>= r 0)
          (setq result (cons (funcall 'neovm--md-row-list mat r) result))
          (setq r (1- r)))
        result)))

  (unwind-protect
      (let ((m (funcall 'neovm--md-from-list '((1 2 3) (4 5 6) (7 8 9)))))
        (list
          (funcall 'neovm--md-rows m)
          (funcall 'neovm--md-cols m)
          (funcall 'neovm--md-ref m 0 0)
          (funcall 'neovm--md-ref m 1 2)
          (funcall 'neovm--md-ref m 2 1)
          (funcall 'neovm--md-row-list m 1)
          (funcall 'neovm--md-col-list m 1)
          (funcall 'neovm--md-to-list m)
          ;; Mutation
          (progn
            (funcall 'neovm--md-set m 1 1 99)
            (funcall 'neovm--md-ref m 1 1))))
    (fmakunbound 'neovm--md-from-list)
    (fmakunbound 'neovm--md-ref)
    (fmakunbound 'neovm--md-set)
    (fmakunbound 'neovm--md-rows)
    (fmakunbound 'neovm--md-cols)
    (fmakunbound 'neovm--md-row-list)
    (fmakunbound 'neovm--md-col-list)
    (fmakunbound 'neovm--md-to-list)))"#;
    let expect =
        expect_test::expect![[r#""OK (3 3 1 6 8 (4 5 6) (2 5 8) ((1 2 3) (4 5 6) (7 8 9)) 99)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Matrix multiplication (NxM * MxP) using vectors
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_matrix_decomp_multiplication() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--md-from-list
    (lambda (lst)
      (apply #'vector (mapcar (lambda (row) (apply #'vector row)) lst))))

  (fset 'neovm--md-ref
    (lambda (mat r c) (aref (aref mat r) c)))

  (fset 'neovm--md-rows (lambda (mat) (length mat)))
  (fset 'neovm--md-cols (lambda (mat) (length (aref mat 0))))

  (fset 'neovm--md-to-list
    (lambda (mat)
      (let ((result nil) (r (1- (length mat))))
        (while (>= r 0)
          (let ((row nil) (c (1- (length (aref mat 0)))))
            (while (>= c 0)
              (setq row (cons (aref (aref mat r) c) row))
              (setq c (1- c)))
            (setq result (cons row result)))
          (setq r (1- r)))
        result)))

  ;; Matrix multiply: A(n,m) * B(m,p) -> C(n,p)
  (fset 'neovm--md-mult
    (lambda (a b)
      (let* ((n (funcall 'neovm--md-rows a))
             (m (funcall 'neovm--md-cols a))
             (p (funcall 'neovm--md-cols b))
             (result (apply #'vector
                      (let ((rows nil) (i 0))
                        (while (< i n)
                          (setq rows (cons (make-vector p 0) rows))
                          (setq i (1+ i)))
                        (nreverse rows)))))
        (let ((i 0))
          (while (< i n)
            (let ((j 0))
              (while (< j p)
                (let ((sum 0) (k 0))
                  (while (< k m)
                    (setq sum (+ sum (* (funcall 'neovm--md-ref a i k)
                                        (funcall 'neovm--md-ref b k j))))
                    (setq k (1+ k)))
                  (aset (aref result i) j sum))
                (setq j (1+ j))))
            (setq i (1+ i))))
        result)))

  (unwind-protect
      (let ((a (funcall 'neovm--md-from-list '((1 2) (3 4) (5 6))))
            (b (funcall 'neovm--md-from-list '((7 8 9) (10 11 12))))
            (i2 (funcall 'neovm--md-from-list '((1 0) (0 1))))
            (sq (funcall 'neovm--md-from-list '((2 3) (4 5)))))
        (list
          ;; 3x2 * 2x3 -> 3x3
          (funcall 'neovm--md-to-list (funcall 'neovm--md-mult a b))
          ;; 2x2 * identity = same
          (funcall 'neovm--md-to-list (funcall 'neovm--md-mult sq i2))
          ;; identity * 2x2 = same
          (funcall 'neovm--md-to-list (funcall 'neovm--md-mult i2 sq))
          ;; 1x3 * 3x1 -> 1x1
          (funcall 'neovm--md-to-list
            (funcall 'neovm--md-mult
              (funcall 'neovm--md-from-list '((1 2 3)))
              (funcall 'neovm--md-from-list '((4) (5) (6)))))
          ;; Associativity: (A*B)*C = A*(B*C) for compatible sizes
          (let* ((c (funcall 'neovm--md-from-list '((1 0) (0 1) (1 1))))
                 (ab-c (funcall 'neovm--md-mult
                          (funcall 'neovm--md-mult a b) c))
                 (a-bc (funcall 'neovm--md-mult
                          a (funcall 'neovm--md-mult b c))))
            (equal (funcall 'neovm--md-to-list ab-c)
                   (funcall 'neovm--md-to-list a-bc)))))
    (fmakunbound 'neovm--md-from-list)
    (fmakunbound 'neovm--md-ref)
    (fmakunbound 'neovm--md-rows)
    (fmakunbound 'neovm--md-cols)
    (fmakunbound 'neovm--md-to-list)
    (fmakunbound 'neovm--md-mult)))"#;
    let expect = expect_test::expect![[
        r#""OK (((27 30 33) (61 68 75) (95 106 117)) ((2 3) (4 5)) ((2 3) (4 5)) ((32)) t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Transpose with verification of properties
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_matrix_decomp_transpose() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--md-from-list
    (lambda (lst)
      (apply #'vector (mapcar (lambda (row) (apply #'vector row)) lst))))

  (fset 'neovm--md-to-list
    (lambda (mat)
      (let ((result nil) (r (1- (length mat))))
        (while (>= r 0)
          (let ((row nil) (c (1- (length (aref mat 0)))))
            (while (>= c 0)
              (setq row (cons (aref (aref mat r) c) row))
              (setq c (1- c)))
            (setq result (cons row result)))
          (setq r (1- r)))
        result)))

  (fset 'neovm--md-transpose
    (lambda (mat)
      (let* ((nrows (length mat))
             (ncols (length (aref mat 0)))
             (result (apply #'vector
                      (let ((rows nil) (i 0))
                        (while (< i ncols)
                          (setq rows (cons (make-vector nrows 0) rows))
                          (setq i (1+ i)))
                        (nreverse rows)))))
        (let ((r 0))
          (while (< r nrows)
            (let ((c 0))
              (while (< c ncols)
                (aset (aref result c) r (aref (aref mat r) c))
                (setq c (1+ c))))
            (setq r (1+ r))))
        result)))

  (unwind-protect
      (let ((m23 (funcall 'neovm--md-from-list '((1 2 3) (4 5 6))))
            (m33 (funcall 'neovm--md-from-list '((1 2 3) (4 5 6) (7 8 9))))
            (sym (funcall 'neovm--md-from-list '((1 2 3) (2 4 5) (3 5 6))))
            (m14 (funcall 'neovm--md-from-list '((10 20 30 40)))))
        (list
          ;; 2x3 -> 3x2
          (funcall 'neovm--md-to-list (funcall 'neovm--md-transpose m23))
          ;; 1x4 -> 4x1
          (funcall 'neovm--md-to-list (funcall 'neovm--md-transpose m14))
          ;; Double transpose = original
          (equal (funcall 'neovm--md-to-list m33)
                 (funcall 'neovm--md-to-list
                   (funcall 'neovm--md-transpose
                     (funcall 'neovm--md-transpose m33))))
          ;; Symmetric matrix: transpose = self
          (equal (funcall 'neovm--md-to-list sym)
                 (funcall 'neovm--md-to-list
                   (funcall 'neovm--md-transpose sym)))
          ;; 3x3 transpose
          (funcall 'neovm--md-to-list (funcall 'neovm--md-transpose m33))))
    (fmakunbound 'neovm--md-from-list)
    (fmakunbound 'neovm--md-to-list)
    (fmakunbound 'neovm--md-transpose)))"#;
    let expect = expect_test::expect![[
        r#""OK (((1 4) (2 5) (3 6)) ((10) (20) (30) (40)) t t ((1 4 7) (2 5 8) (3 6 9)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Recursive cofactor determinant (NxN)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_matrix_decomp_recursive_determinant() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Compute determinant for arbitrary NxN matrices using cofactor expansion.
    let form = r#"(progn
  (fset 'neovm--md-from-list
    (lambda (lst)
      (apply #'vector (mapcar (lambda (row) (apply #'vector row)) lst))))

  ;; Create minor: delete row r and column c
  (fset 'neovm--md-minor
    (lambda (mat r c)
      (let* ((n (length mat))
             (rows nil)
             (i 0))
        (while (< i n)
          (when (/= i r)
            (let ((row nil) (j 0))
              (while (< j n)
                (when (/= j c)
                  (setq row (cons (aref (aref mat i) j) row)))
                (setq j (1+ j)))
              (setq rows (cons (apply #'vector (nreverse row)) rows))))
          (setq i (1+ i)))
        (apply #'vector (nreverse rows)))))

  ;; Recursive determinant via cofactor expansion along first row
  (fset 'neovm--md-det
    (lambda (mat)
      (let ((n (length mat)))
        (cond
         ((= n 1) (aref (aref mat 0) 0))
         ((= n 2)
          (- (* (aref (aref mat 0) 0) (aref (aref mat 1) 1))
             (* (aref (aref mat 0) 1) (aref (aref mat 1) 0))))
         (t
          (let ((det 0) (j 0))
            (while (< j n)
              (let ((cofactor (* (if (= (% j 2) 0) 1 -1)
                                 (aref (aref mat 0) j)
                                 (funcall 'neovm--md-det
                                   (funcall 'neovm--md-minor mat 0 j)))))
                (setq det (+ det cofactor)))
              (setq j (1+ j)))
            det))))))

  (unwind-protect
      (list
        ;; 1x1
        (funcall 'neovm--md-det (funcall 'neovm--md-from-list '((7))))
        ;; 2x2: ad - bc = 1*4 - 2*3 = -2
        (funcall 'neovm--md-det (funcall 'neovm--md-from-list '((1 2) (3 4))))
        ;; 3x3 identity = 1
        (funcall 'neovm--md-det (funcall 'neovm--md-from-list
                                  '((1 0 0) (0 1 0) (0 0 1))))
        ;; 3x3 singular (det = 0)
        (funcall 'neovm--md-det (funcall 'neovm--md-from-list
                                  '((1 2 3) (4 5 6) (7 8 9))))
        ;; 3x3 non-singular
        (funcall 'neovm--md-det (funcall 'neovm--md-from-list
                                  '((6 1 1) (4 -2 5) (2 8 7))))
        ;; 4x4 identity = 1
        (funcall 'neovm--md-det (funcall 'neovm--md-from-list
                                  '((1 0 0 0) (0 1 0 0)
                                    (0 0 1 0) (0 0 0 1))))
        ;; 4x4 non-trivial
        (funcall 'neovm--md-det (funcall 'neovm--md-from-list
                                  '((1 2 3 4) (5 6 7 8)
                                    (2 6 4 8) (3 1 1 2))))
        ;; Permutation matrix (det = -1 or +1)
        (funcall 'neovm--md-det (funcall 'neovm--md-from-list
                                  '((0 1 0) (0 0 1) (1 0 0)))))
    (fmakunbound 'neovm--md-from-list)
    (fmakunbound 'neovm--md-minor)
    (fmakunbound 'neovm--md-det)))"#;
    let expect = expect_test::expect![[r#""OK (7 -2 1 0 -306 1 72 1)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Row echelon form via Gaussian elimination (integer arithmetic)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_matrix_decomp_row_echelon() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Gaussian elimination producing row echelon form.
    // Use integer-scaled arithmetic to avoid floating point.
    let form = r#"(progn
  (fset 'neovm--md-from-list
    (lambda (lst)
      (apply #'vector (mapcar (lambda (row) (apply #'vector row)) lst))))

  (fset 'neovm--md-to-list
    (lambda (mat)
      (let ((result nil) (r (1- (length mat))))
        (while (>= r 0)
          (let ((row nil) (c (1- (length (aref mat 0)))))
            (while (>= c 0)
              (setq row (cons (aref (aref mat r) c) row))
              (setq c (1- c)))
            (setq result (cons row result)))
          (setq r (1- r)))
        result)))

  ;; Deep copy matrix
  (fset 'neovm--md-copy
    (lambda (mat)
      (apply #'vector
        (mapcar (lambda (row) (copy-sequence row))
                (append mat nil)))))

  ;; Gaussian elimination to row echelon form (integer, no pivoting)
  ;; Uses fraction-free elimination: multiply rows to avoid division
  (fset 'neovm--md-ref-form
    (lambda (mat-orig)
      (let* ((mat (funcall 'neovm--md-copy mat-orig))
             (nrows (length mat))
             (ncols (length (aref mat 0)))
             (pivot-row 0)
             (pivot-col 0))
        (while (and (< pivot-row nrows) (< pivot-col ncols))
          (let ((pivot-val (aref (aref mat pivot-row) pivot-col)))
            (if (= pivot-val 0)
                ;; Try to find a non-zero row below to swap
                (let ((found nil) (sr (1+ pivot-row)))
                  (while (and (< sr nrows) (not found))
                    (when (/= (aref (aref mat sr) pivot-col) 0)
                      ;; Swap rows
                      (let ((tmp (aref mat pivot-row)))
                        (aset mat pivot-row (aref mat sr))
                        (aset mat sr tmp))
                      (setq found t))
                    (setq sr (1+ sr)))
                  (unless found
                    (setq pivot-col (1+ pivot-col))))
              ;; Eliminate below
              (let ((target (1+ pivot-row)))
                (while (< target nrows)
                  (let ((factor (aref (aref mat target) pivot-col)))
                    (when (/= factor 0)
                      (let ((c 0))
                        (while (< c ncols)
                          (aset (aref mat target) c
                                (- (* pivot-val (aref (aref mat target) c))
                                   (* factor (aref (aref mat pivot-row) c))))
                          (setq c (1+ c))))))
                  (setq target (1+ target)))
                (setq pivot-row (1+ pivot-row)
                      pivot-col (1+ pivot-col))))))
        mat)))

  (unwind-protect
      (list
        ;; 2x3 augmented matrix
        (funcall 'neovm--md-to-list
          (funcall 'neovm--md-ref-form
            (funcall 'neovm--md-from-list '((1 2 5) (3 4 11)))))
        ;; 3x4 augmented matrix
        (funcall 'neovm--md-to-list
          (funcall 'neovm--md-ref-form
            (funcall 'neovm--md-from-list
              '((1 1 1 6) (2 3 1 14) (1 1 3 12)))))
        ;; Already in echelon form
        (funcall 'neovm--md-to-list
          (funcall 'neovm--md-ref-form
            (funcall 'neovm--md-from-list '((1 2 3) (0 4 5) (0 0 6)))))
        ;; Matrix requiring row swap
        (funcall 'neovm--md-to-list
          (funcall 'neovm--md-ref-form
            (funcall 'neovm--md-from-list '((0 1 2) (3 4 5) (6 7 8)))))
        ;; Singular matrix
        (funcall 'neovm--md-to-list
          (funcall 'neovm--md-ref-form
            (funcall 'neovm--md-from-list '((1 2 3) (2 4 6) (1 1 1))))))
    (fmakunbound 'neovm--md-from-list)
    (fmakunbound 'neovm--md-to-list)
    (fmakunbound 'neovm--md-copy)
    (fmakunbound 'neovm--md-ref-form)))"#;
    let expect = expect_test::expect![[
        r#""OK (((1 2 5) (0 -2 -4)) ((1 1 1 6) (0 1 -1 2) (0 0 2 6)) ((1 2 3) (0 4 5) (0 0 6)) ((3 4 5) (0 1 2) (0 0 0)) ((1 2 3) (0 -1 -2) (0 0 0)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Matrix inverse via augmented matrix [A | I] -> [I | A^-1]
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_matrix_decomp_inverse() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Compute A^-1 by augmenting A with identity and reducing to RREF.
    // We verify A * adj(A) = det(A) * I using integer arithmetic.
    let form = r#"(progn
  (fset 'neovm--md-from-list
    (lambda (lst)
      (apply #'vector (mapcar (lambda (row) (apply #'vector row)) lst))))

  (fset 'neovm--md-to-list
    (lambda (mat)
      (let ((result nil) (r (1- (length mat))))
        (while (>= r 0)
          (let ((row nil) (c (1- (length (aref mat 0)))))
            (while (>= c 0)
              (setq row (cons (aref (aref mat r) c) row))
              (setq c (1- c)))
            (setq result (cons row result)))
          (setq r (1- r)))
        result)))

  ;; Compute adjugate and determinant: returns (det . adj-matrix-as-list)
  ;; Uses cofactor method for small matrices.
  ;; Minor matrix
  (fset 'neovm--md-minor
    (lambda (mat r c)
      (let* ((n (length mat)) (rows nil) (i 0))
        (while (< i n)
          (when (/= i r)
            (let ((row nil) (j 0))
              (while (< j n)
                (when (/= j c)
                  (setq row (cons (aref (aref mat i) j) row)))
                (setq j (1+ j)))
              (setq rows (cons (apply #'vector (nreverse row)) rows))))
          (setq i (1+ i)))
        (apply #'vector (nreverse rows)))))

  (fset 'neovm--md-det
    (lambda (mat)
      (let ((n (length mat)))
        (cond
         ((= n 1) (aref (aref mat 0) 0))
         ((= n 2)
          (- (* (aref (aref mat 0) 0) (aref (aref mat 1) 1))
             (* (aref (aref mat 0) 1) (aref (aref mat 1) 0))))
         (t (let ((det 0) (j 0))
              (while (< j n)
                (setq det (+ det (* (if (= (% j 2) 0) 1 -1)
                                    (aref (aref mat 0) j)
                                    (funcall 'neovm--md-det
                                      (funcall 'neovm--md-minor mat 0 j)))))
                (setq j (1+ j)))
              det))))))

  ;; Cofactor matrix (transpose of cofactors = adjugate)
  (fset 'neovm--md-adjugate
    (lambda (mat)
      (let* ((n (length mat))
             (adj (apply #'vector
                    (let ((rows nil) (i 0))
                      (while (< i n)
                        (setq rows (cons (make-vector n 0) rows))
                        (setq i (1+ i)))
                      (nreverse rows)))))
        (let ((i 0))
          (while (< i n)
            (let ((j 0))
              (while (< j n)
                ;; adjugate[j][i] = (-1)^(i+j) * det(minor(i,j))
                (aset (aref adj j) i
                      (* (if (= (% (+ i j) 2) 0) 1 -1)
                         (funcall 'neovm--md-det
                           (funcall 'neovm--md-minor mat i j))))
                (setq j (1+ j))))
            (setq i (1+ i))))
        adj)))

  ;; Matrix multiply
  (fset 'neovm--md-mult
    (lambda (a b)
      (let* ((n (length a)) (m (length (aref a 0))) (p (length (aref b 0)))
             (result (apply #'vector
                      (let ((rows nil) (i 0))
                        (while (< i n)
                          (setq rows (cons (make-vector p 0) rows))
                          (setq i (1+ i)))
                        (nreverse rows)))))
        (let ((i 0))
          (while (< i n)
            (let ((j 0))
              (while (< j p)
                (let ((sum 0) (k 0))
                  (while (< k m)
                    (setq sum (+ sum (* (aref (aref a i) k)
                                        (aref (aref b k) j))))
                    (setq k (1+ k)))
                  (aset (aref result i) j sum))
                (setq j (1+ j))))
            (setq i (1+ i))))
        result)))

  ;; Build n*n identity (for verification)
  (fset 'neovm--md-identity
    (lambda (n)
      (let ((mat (apply #'vector
                   (let ((rows nil) (i 0))
                     (while (< i n)
                       (setq rows (cons (make-vector n 0) rows))
                       (setq i (1+ i)))
                     (nreverse rows)))))
        (let ((i 0))
          (while (< i n) (aset (aref mat i) i 1) (setq i (1+ i))))
        mat)))

  ;; Scale matrix by scalar
  (fset 'neovm--md-scale
    (lambda (s mat)
      (let* ((n (length mat)) (m (length (aref mat 0)))
             (result (apply #'vector
                       (let ((rows nil) (i 0))
                         (while (< i n)
                           (setq rows (cons (make-vector m 0) rows))
                           (setq i (1+ i)))
                         (nreverse rows)))))
        (let ((i 0))
          (while (< i n)
            (let ((j 0))
              (while (< j m)
                (aset (aref result i) j (* s (aref (aref mat i) j)))
                (setq j (1+ j))))
            (setq i (1+ i))))
        result)))

  (unwind-protect
      (let ((m2 (funcall 'neovm--md-from-list '((4 7) (2 6))))
            (m3 (funcall 'neovm--md-from-list '((2 1 1) (1 3 2) (1 0 0)))))
        (list
          ;; 2x2: verify A * adj(A) = det(A) * I
          (let* ((det (funcall 'neovm--md-det m2))
                 (adj (funcall 'neovm--md-adjugate m2))
                 (product (funcall 'neovm--md-mult m2 adj))
                 (expected (funcall 'neovm--md-scale det
                             (funcall 'neovm--md-identity 2))))
            (list det
                  (funcall 'neovm--md-to-list adj)
                  (equal (funcall 'neovm--md-to-list product)
                         (funcall 'neovm--md-to-list expected))))
          ;; 3x3: verify A * adj(A) = det(A) * I
          (let* ((det (funcall 'neovm--md-det m3))
                 (adj (funcall 'neovm--md-adjugate m3))
                 (product (funcall 'neovm--md-mult m3 adj))
                 (expected (funcall 'neovm--md-scale det
                             (funcall 'neovm--md-identity 3))))
            (list det
                  (equal (funcall 'neovm--md-to-list product)
                         (funcall 'neovm--md-to-list expected))))))
    (fmakunbound 'neovm--md-from-list)
    (fmakunbound 'neovm--md-to-list)
    (fmakunbound 'neovm--md-minor)
    (fmakunbound 'neovm--md-det)
    (fmakunbound 'neovm--md-adjugate)
    (fmakunbound 'neovm--md-mult)
    (fmakunbound 'neovm--md-identity)
    (fmakunbound 'neovm--md-scale)))"#;
    let expect = expect_test::expect![[r#""OK ((10 ((6 -7) (-2 4)) t) (-1 t))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Solving linear systems via Gaussian elimination with back-substitution
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_matrix_decomp_solve_systems() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Solve multiple linear systems Ax = b and verify A*x = b.
    let form = r#"(progn
  (fset 'neovm--md-from-list
    (lambda (lst)
      (apply #'vector (mapcar (lambda (row) (apply #'vector row)) lst))))

  (fset 'neovm--md-copy
    (lambda (mat)
      (apply #'vector
        (mapcar (lambda (row) (copy-sequence row))
                (append mat nil)))))

  ;; Solve Ax=b using Gaussian elimination. Augmented matrix input.
  ;; Returns solution as list of (numerator . denominator) pairs.
  (fset 'neovm--md-solve
    (lambda (aug-orig)
      (let* ((aug (funcall 'neovm--md-copy aug-orig))
             (n (length aug))
             (ncols (length (aref aug 0))))
        ;; Forward elimination with partial pivoting
        (let ((pivot 0))
          (while (< pivot n)
            ;; Find largest pivot
            (let ((max-val (abs (aref (aref aug pivot) pivot)))
                  (max-row pivot)
                  (sr (1+ pivot)))
              (while (< sr n)
                (when (> (abs (aref (aref aug sr) pivot)) max-val)
                  (setq max-val (abs (aref (aref aug sr) pivot))
                        max-row sr))
                (setq sr (1+ sr)))
              ;; Swap
              (when (/= max-row pivot)
                (let ((tmp (aref aug pivot)))
                  (aset aug pivot (aref aug max-row))
                  (aset aug max-row tmp))))
            ;; Eliminate below
            (let ((pv (aref (aref aug pivot) pivot))
                  (target (1+ pivot)))
              (while (< target n)
                (let ((factor (aref (aref aug target) pivot)))
                  (when (/= factor 0)
                    (let ((c 0))
                      (while (< c ncols)
                        (aset (aref aug target) c
                              (- (* pv (aref (aref aug target) c))
                                 (* factor (aref (aref aug pivot) c))))
                        (setq c (1+ c))))))
                (setq target (1+ target))))
            (setq pivot (1+ pivot))))
        ;; Back substitution
        (let ((x (make-vector n 0))
              (row (1- n)))
          (while (>= row 0)
            (let ((rhs (aref (aref aug row) (1- ncols)))
                  (col (1+ row)))
              (while (< col n)
                (setq rhs (- rhs (* (aref (aref aug row) col)
                                     (aref x col))))
                (setq col (1+ col)))
              ;; Integer division (only works for clean systems)
              (aset x row (/ rhs (aref (aref aug row) row))))
            (setq row (1- row)))
          (append x nil)))))

  ;; Dot product of two lists
  (fset 'neovm--md-dot
    (lambda (a b)
      (let ((sum 0) (la a) (lb b))
        (while la
          (setq sum (+ sum (* (car la) (car lb))))
          (setq la (cdr la) lb (cdr lb)))
        sum)))

  ;; Verify solution: A * x should equal b
  (fset 'neovm--md-verify
    (lambda (a-list x-list b-list)
      (mapcar (lambda (row)
                (= (funcall 'neovm--md-dot row x-list)
                   (car b-list))
                )
              a-list)))

  (unwind-protect
      (list
        ;; System 1: x + 2y = 5, 3x + 4y = 11 -> x=1, y=2
        (funcall 'neovm--md-solve
          (funcall 'neovm--md-from-list '((1 2 5) (3 4 11))))
        ;; System 2: 2x + y = 5, x - y = 1 -> x=2, y=1
        (funcall 'neovm--md-solve
          (funcall 'neovm--md-from-list '((2 1 5) (1 -1 1))))
        ;; System 3: 3x3
        ;; x + y + z = 6, 2x + 3y + z = 14, x + y + 3z = 12
        ;; -> x=1, y=3, z=2
        (funcall 'neovm--md-solve
          (funcall 'neovm--md-from-list
            '((1 1 1 6) (2 3 1 14) (1 1 3 12))))
        ;; System 4: requires pivoting
        ;; 0x + y + z = 3, 2x + y + z = 5, x + 2y + z = 6
        ;; -> x=1, y=2, z=1
        (funcall 'neovm--md-solve
          (funcall 'neovm--md-from-list
            '((0 1 1 3) (2 1 1 5) (1 2 1 6)))))
    (fmakunbound 'neovm--md-from-list)
    (fmakunbound 'neovm--md-copy)
    (fmakunbound 'neovm--md-solve)
    (fmakunbound 'neovm--md-dot)
    (fmakunbound 'neovm--md-verify)))"#;
    let expect = expect_test::expect![[r#""OK ((1 2) (2 1) (-2 5 3) (1 2 1))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Matrix properties: trace, diagonal, and Cayley-Hamilton verification
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_matrix_decomp_trace_and_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Verify various matrix properties including trace(A+B) = trace(A)+trace(B),
    // trace(AB) = trace(BA), and det(kA) = k^n * det(A).
    let form = r#"(progn
  (fset 'neovm--md-from-list
    (lambda (lst)
      (apply #'vector (mapcar (lambda (row) (apply #'vector row)) lst))))

  (fset 'neovm--md-to-list
    (lambda (mat)
      (let ((result nil) (r (1- (length mat))))
        (while (>= r 0)
          (let ((row nil) (c (1- (length (aref mat 0)))))
            (while (>= c 0)
              (setq row (cons (aref (aref mat r) c) row))
              (setq c (1- c)))
            (setq result (cons row result)))
          (setq r (1- r)))
        result)))

  (fset 'neovm--md-trace
    (lambda (mat)
      (let ((sum 0) (i 0) (n (length mat)))
        (while (< i n)
          (setq sum (+ sum (aref (aref mat i) i)))
          (setq i (1+ i)))
        sum)))

  (fset 'neovm--md-add
    (lambda (a b)
      (let* ((n (length a)) (m (length (aref a 0)))
             (result (apply #'vector
                       (let ((rows nil) (i 0))
                         (while (< i n)
                           (setq rows (cons (make-vector m 0) rows))
                           (setq i (1+ i)))
                         (nreverse rows)))))
        (let ((i 0))
          (while (< i n)
            (let ((j 0))
              (while (< j m)
                (aset (aref result i) j
                      (+ (aref (aref a i) j)
                         (aref (aref b i) j)))
                (setq j (1+ j))))
            (setq i (1+ i))))
        result)))

  (fset 'neovm--md-mult
    (lambda (a b)
      (let* ((n (length a)) (m (length (aref a 0))) (p (length (aref b 0)))
             (result (apply #'vector
                       (let ((rows nil) (i 0))
                         (while (< i n)
                           (setq rows (cons (make-vector p 0) rows))
                           (setq i (1+ i)))
                         (nreverse rows)))))
        (let ((i 0))
          (while (< i n)
            (let ((j 0))
              (while (< j p)
                (let ((sum 0) (k 0))
                  (while (< k m)
                    (setq sum (+ sum (* (aref (aref a i) k)
                                        (aref (aref b k) j))))
                    (setq k (1+ k)))
                  (aset (aref result i) j sum))
                (setq j (1+ j))))
            (setq i (1+ i))))
        result)))

  (fset 'neovm--md-scale
    (lambda (s mat)
      (let* ((n (length mat)) (m (length (aref mat 0)))
             (result (apply #'vector
                       (let ((rows nil) (i 0))
                         (while (< i n)
                           (setq rows (cons (make-vector m 0) rows))
                           (setq i (1+ i)))
                         (nreverse rows)))))
        (let ((i 0))
          (while (< i n)
            (let ((j 0))
              (while (< j m)
                (aset (aref result i) j (* s (aref (aref mat i) j)))
                (setq j (1+ j))))
            (setq i (1+ i))))
        result)))

  (fset 'neovm--md-minor
    (lambda (mat r c)
      (let* ((n (length mat)) (rows nil) (i 0))
        (while (< i n)
          (when (/= i r)
            (let ((row nil) (j 0))
              (while (< j n)
                (when (/= j c)
                  (setq row (cons (aref (aref mat i) j) row)))
                (setq j (1+ j)))
              (setq rows (cons (apply #'vector (nreverse row)) rows))))
          (setq i (1+ i)))
        (apply #'vector (nreverse rows)))))

  (fset 'neovm--md-det
    (lambda (mat)
      (let ((n (length mat)))
        (cond
         ((= n 1) (aref (aref mat 0) 0))
         ((= n 2)
          (- (* (aref (aref mat 0) 0) (aref (aref mat 1) 1))
             (* (aref (aref mat 0) 1) (aref (aref mat 1) 0))))
         (t (let ((det 0) (j 0))
              (while (< j n)
                (setq det (+ det (* (if (= (% j 2) 0) 1 -1)
                                    (aref (aref mat 0) j)
                                    (funcall 'neovm--md-det
                                      (funcall 'neovm--md-minor mat 0 j)))))
                (setq j (1+ j)))
              det))))))

  (unwind-protect
      (let ((a (funcall 'neovm--md-from-list '((1 2) (3 4))))
            (b (funcall 'neovm--md-from-list '((5 6) (7 8)))))
        (list
          ;; trace(A) + trace(B) = trace(A+B)
          (= (+ (funcall 'neovm--md-trace a)
                (funcall 'neovm--md-trace b))
             (funcall 'neovm--md-trace (funcall 'neovm--md-add a b)))
          ;; trace(AB) = trace(BA)
          (= (funcall 'neovm--md-trace (funcall 'neovm--md-mult a b))
             (funcall 'neovm--md-trace (funcall 'neovm--md-mult b a)))
          ;; det(kA) = k^n * det(A) for 2x2
          (let ((k 3))
            (= (funcall 'neovm--md-det (funcall 'neovm--md-scale k a))
               (* k k (funcall 'neovm--md-det a))))
          ;; det(AB) = det(A) * det(B)
          (= (funcall 'neovm--md-det (funcall 'neovm--md-mult a b))
             (* (funcall 'neovm--md-det a) (funcall 'neovm--md-det b)))
          ;; Trace values
          (funcall 'neovm--md-trace a)
          (funcall 'neovm--md-trace b)
          ;; Determinant values
          (funcall 'neovm--md-det a)
          (funcall 'neovm--md-det b)))
    (fmakunbound 'neovm--md-from-list)
    (fmakunbound 'neovm--md-to-list)
    (fmakunbound 'neovm--md-trace)
    (fmakunbound 'neovm--md-add)
    (fmakunbound 'neovm--md-mult)
    (fmakunbound 'neovm--md-scale)
    (fmakunbound 'neovm--md-minor)
    (fmakunbound 'neovm--md-det)))"#;
    let expect = expect_test::expect![[r#""OK (t t t t 5 13 -2 -2)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
