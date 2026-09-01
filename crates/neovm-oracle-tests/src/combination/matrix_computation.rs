//! Oracle parity tests for matrix computation framework.
//!
//! Implements matrix operations in pure Elisp: matrix creation (make-matrix),
//! element access/set, row/column extraction, matrix addition/subtraction/
//! scalar-multiply, matrix multiplication (n*m * m*p), transpose, identity
//! matrix, determinant (cofactor expansion), trace, row echelon form
//! (Gaussian elimination), matrix equality with epsilon for floats,
//! Hadamard product, Kronecker product, matrix power.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Matrix creation and element access
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_matrix_creation_and_access() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; make-matrix: create rows x cols matrix filled with init
  (fset 'neovm--mc-make
    (lambda (rows cols init)
      (let ((result nil) (r 0))
        (while (< r rows)
          (setq result (cons (make-list cols init) result))
          (setq r (1+ r)))
        (nreverse result))))

  ;; get element at (r, c)
  (fset 'neovm--mc-ref
    (lambda (mat r c)
      (nth c (nth r mat))))

  ;; set element at (r, c) — destructive
  (fset 'neovm--mc-set!
    (lambda (mat r c val)
      (setcar (nthcdr c (nth r mat)) val)
      mat))

  ;; number of rows and cols
  (fset 'neovm--mc-rows (lambda (mat) (length mat)))
  (fset 'neovm--mc-cols (lambda (mat) (if mat (length (car mat)) 0)))

  ;; Extract row and column
  (fset 'neovm--mc-row (lambda (mat r) (nth r mat)))
  (fset 'neovm--mc-col
    (lambda (mat c)
      (mapcar (lambda (row) (nth c row)) mat)))

  (unwind-protect
      (list
        ;; 3x4 zero matrix
        (funcall 'neovm--mc-make 3 4 0)
        ;; 2x2 with value 5
        (funcall 'neovm--mc-make 2 2 5)
        ;; 1x1
        (funcall 'neovm--mc-make 1 1 42)
        ;; Dimensions
        (let ((m (funcall 'neovm--mc-make 3 4 0)))
          (list (funcall 'neovm--mc-rows m) (funcall 'neovm--mc-cols m)))
        ;; Element access
        (let ((m '((1 2 3) (4 5 6) (7 8 9))))
          (list (funcall 'neovm--mc-ref m 0 0)
                (funcall 'neovm--mc-ref m 0 2)
                (funcall 'neovm--mc-ref m 1 1)
                (funcall 'neovm--mc-ref m 2 0)
                (funcall 'neovm--mc-ref m 2 2)))
        ;; Set element
        (let ((m (funcall 'neovm--mc-make 2 2 0)))
          (funcall 'neovm--mc-set! m 0 0 10)
          (funcall 'neovm--mc-set! m 1 1 20)
          m)
        ;; Row and column extraction
        (let ((m '((1 2 3) (4 5 6) (7 8 9))))
          (list (funcall 'neovm--mc-row m 0)
                (funcall 'neovm--mc-row m 2)
                (funcall 'neovm--mc-col m 0)
                (funcall 'neovm--mc-col m 1)
                (funcall 'neovm--mc-col m 2))))
    (fmakunbound 'neovm--mc-make)
    (fmakunbound 'neovm--mc-ref)
    (fmakunbound 'neovm--mc-set!)
    (fmakunbound 'neovm--mc-rows)
    (fmakunbound 'neovm--mc-cols)
    (fmakunbound 'neovm--mc-row)
    (fmakunbound 'neovm--mc-col)))"#;
    let expect = expect_test::expect![[
        r#""OK (((0 0 0 0) (0 0 0 0) (0 0 0 0)) ((5 5) (5 5)) ((42)) (3 4) (1 3 5 7 9) ((10 0) (0 20)) ((1 2 3) (7 8 9) (1 4 7) (2 5 8) (3 6 9)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Matrix addition, subtraction, scalar multiply
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_matrix_arithmetic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--mc-map2
    (lambda (f a b)
      (let ((result nil) (ra a) (rb b))
        (while ra
          (let ((row-a (car ra)) (row-b (car rb)) (new-row nil))
            (while row-a
              (setq new-row (cons (funcall f (car row-a) (car row-b)) new-row))
              (setq row-a (cdr row-a) row-b (cdr row-b)))
            (setq result (cons (nreverse new-row) result)))
          (setq ra (cdr ra) rb (cdr rb)))
        (nreverse result))))

  (fset 'neovm--mc-add (lambda (a b) (funcall 'neovm--mc-map2 #'+ a b)))
  (fset 'neovm--mc-sub (lambda (a b) (funcall 'neovm--mc-map2 #'- a b)))

  (fset 'neovm--mc-scale
    (lambda (s mat)
      (mapcar (lambda (row) (mapcar (lambda (x) (* s x)) row)) mat)))

  (unwind-protect
      (let ((a '((1 2 3) (4 5 6)))
            (b '((7 8 9) (10 11 12)))
            (i2 '((1 0) (0 1))))
        (list
          ;; A + B
          (funcall 'neovm--mc-add a b)
          ;; A - B
          (funcall 'neovm--mc-sub a b)
          ;; B - A
          (funcall 'neovm--mc-sub b a)
          ;; A - A = zero
          (funcall 'neovm--mc-sub a a)
          ;; 3 * A
          (funcall 'neovm--mc-scale 3 a)
          ;; 0 * B = zero
          (funcall 'neovm--mc-scale 0 b)
          ;; -1 * A
          (funcall 'neovm--mc-scale -1 a)
          ;; A + (-1)*B = A - B
          (equal (funcall 'neovm--mc-add a (funcall 'neovm--mc-scale -1 b))
                 (funcall 'neovm--mc-sub a b))
          ;; 2A + 3B
          (funcall 'neovm--mc-add
                   (funcall 'neovm--mc-scale 2 a)
                   (funcall 'neovm--mc-scale 3 b))
          ;; Scale identity
          (funcall 'neovm--mc-scale 5 i2)
          ;; Commutativity: A+B = B+A
          (equal (funcall 'neovm--mc-add a b)
                 (funcall 'neovm--mc-add b a))))
    (fmakunbound 'neovm--mc-map2)
    (fmakunbound 'neovm--mc-add)
    (fmakunbound 'neovm--mc-sub)
    (fmakunbound 'neovm--mc-scale)))"#;
    let expect = expect_test::expect![[
        r#""OK (((8 10 12) (14 16 18)) ((-6 -6 -6) (-6 -6 -6)) ((6 6 6) (6 6 6)) ((0 0 0) (0 0 0)) ((3 6 9) (12 15 18)) ((0 0 0) (0 0 0)) ((-1 -2 -3) (-4 -5 -6)) t ((23 28 33) (38 43 48)) ((5 0) (0 5)) t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Transpose and identity matrix
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_matrix_transpose_and_identity() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--mc-transpose
    (lambda (mat)
      (if (null mat) nil
        (let ((ncols (length (car mat))) (result nil) (c 0))
          (while (< c ncols)
            (let ((col nil) (rows mat))
              (while rows
                (setq col (cons (nth c (car rows)) col))
                (setq rows (cdr rows)))
              (setq result (cons (nreverse col) result)))
            (setq c (1+ c)))
          (nreverse result)))))

  (fset 'neovm--mc-identity
    (lambda (n)
      (let ((result nil) (r 0))
        (while (< r n)
          (let ((row (make-list n 0)))
            (setcar (nthcdr r row) 1)
            (setq result (cons row result)))
          (setq r (1+ r)))
        (nreverse result))))

  (unwind-protect
      (list
        ;; Transpose of 2x3
        (funcall 'neovm--mc-transpose '((1 2 3) (4 5 6)))
        ;; Transpose of 3x1
        (funcall 'neovm--mc-transpose '((1) (2) (3)))
        ;; Transpose of 1x3
        (funcall 'neovm--mc-transpose '((1 2 3)))
        ;; Transpose of symmetric matrix = self
        (let ((sym '((1 2 3) (2 4 5) (3 5 6))))
          (equal sym (funcall 'neovm--mc-transpose sym)))
        ;; Double transpose = original
        (let ((m '((1 2 3) (4 5 6))))
          (equal m (funcall 'neovm--mc-transpose
                            (funcall 'neovm--mc-transpose m))))
        ;; Identity matrices
        (funcall 'neovm--mc-identity 1)
        (funcall 'neovm--mc-identity 2)
        (funcall 'neovm--mc-identity 3)
        (funcall 'neovm--mc-identity 4)
        ;; Transpose of identity = identity
        (equal (funcall 'neovm--mc-identity 3)
               (funcall 'neovm--mc-transpose (funcall 'neovm--mc-identity 3)))
        ;; Transpose of square matrix
        (funcall 'neovm--mc-transpose '((1 2) (3 4))))
    (fmakunbound 'neovm--mc-transpose)
    (fmakunbound 'neovm--mc-identity)))"#;
    let expect = expect_test::expect![[
        r#""OK (((1 4) (2 5) (3 6)) ((1 2 3)) ((1) (2) (3)) t t ((1)) ((1 0) (0 1)) ((1 0 0) (0 1 0) (0 0 1)) ((1 0 0 0) (0 1 0 0) (0 0 1 0) (0 0 0 1)) t ((1 3) (2 4)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Matrix multiplication (n*m * m*p)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_matrix_mult_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--mc-transpose
    (lambda (mat)
      (if (null mat) nil
        (let ((ncols (length (car mat))) (result nil) (c 0))
          (while (< c ncols)
            (let ((col nil) (rows mat))
              (while rows
                (setq col (cons (nth c (car rows)) col))
                (setq rows (cdr rows)))
              (setq result (cons (nreverse col) result)))
            (setq c (1+ c)))
          (nreverse result)))))

  (fset 'neovm--mc-dot
    (lambda (a b)
      (let ((sum 0))
        (while a
          (setq sum (+ sum (* (car a) (car b))))
          (setq a (cdr a) b (cdr b)))
        sum)))

  (fset 'neovm--mc-mult
    (lambda (a b)
      (let ((bt (funcall 'neovm--mc-transpose b)))
        (mapcar (lambda (row-a)
                  (mapcar (lambda (col-b)
                            (funcall 'neovm--mc-dot row-a col-b))
                          bt))
                a))))

  (fset 'neovm--mc-identity
    (lambda (n)
      (let ((result nil) (r 0))
        (while (< r n)
          (let ((row (make-list n 0)))
            (setcar (nthcdr r row) 1)
            (setq result (cons row result)))
          (setq r (1+ r)))
        (nreverse result))))

  (unwind-protect
      (let ((a '((1 2) (3 4) (5 6)))
            (b '((7 8 9) (10 11 12)))
            (sq '((1 2) (3 4)))
            (i3 (funcall 'neovm--mc-identity 3)))
        (list
          ;; 3x2 * 2x3 = 3x3
          (funcall 'neovm--mc-mult a b)
          ;; 2x3 * 3x2 = 2x2
          (funcall 'neovm--mc-mult b a)
          ;; Square * Identity = Square
          (equal sq (funcall 'neovm--mc-mult sq (funcall 'neovm--mc-identity 2)))
          ;; Identity * Square = Square
          (equal sq (funcall 'neovm--mc-mult (funcall 'neovm--mc-identity 2) sq))
          ;; 1x3 * 3x1 = 1x1
          (funcall 'neovm--mc-mult '((1 2 3)) '((4) (5) (6)))
          ;; 3x1 * 1x3 = 3x3
          (funcall 'neovm--mc-mult '((1) (2) (3)) '((4 5 6)))
          ;; 2x2 * 2x2
          (funcall 'neovm--mc-mult sq '((5 6) (7 8)))
          ;; (AB)^T = B^T A^T
          (let* ((ab (funcall 'neovm--mc-mult sq '((5 6) (7 8))))
                 (bt-at (funcall 'neovm--mc-mult
                                 (funcall 'neovm--mc-transpose '((5 6) (7 8)))
                                 (funcall 'neovm--mc-transpose sq))))
            (equal (funcall 'neovm--mc-transpose ab) bt-at))
          ;; Associativity: (AB)C = A(BC) for compatible sizes
          (let* ((p '((1 0) (0 1) (1 1)))   ;; 3x2
                 (q '((2 3) (4 5)))           ;; 2x2
                 (r2 '((1) (0)))              ;; 2x1
                 (pq-r (funcall 'neovm--mc-mult
                                (funcall 'neovm--mc-mult p q) r2))
                 (p-qr (funcall 'neovm--mc-mult
                                p (funcall 'neovm--mc-mult q r2))))
            (equal pq-r p-qr))))
    (fmakunbound 'neovm--mc-transpose)
    (fmakunbound 'neovm--mc-dot)
    (fmakunbound 'neovm--mc-mult)
    (fmakunbound 'neovm--mc-identity)))"#;
    let expect = expect_test::expect![[
        r#""OK (((27 30 33) (61 68 75) (95 106 117)) ((76 100) (103 136)) t t ((32)) ((4 5 6) (8 10 12) (12 15 18)) ((19 22) (43 50)) t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Determinant via cofactor expansion (general nxn)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_matrix_determinant_general() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Minor: matrix with row r and column c removed
  (fset 'neovm--mc-minor
    (lambda (mat r c)
      (let ((result nil) (ri 0))
        (dolist (row mat)
          (unless (= ri r)
            (let ((new-row nil) (ci 0))
              (dolist (val row)
                (unless (= ci c)
                  (setq new-row (cons val new-row)))
                (setq ci (1+ ci)))
              (setq result (cons (nreverse new-row) result))))
          (setq ri (1+ ri)))
        (nreverse result))))

  ;; Determinant via cofactor expansion along first row
  (fset 'neovm--mc-det
    (lambda (mat)
      (let ((n (length mat)))
        (cond
         ((= n 1) (caar mat))
         ((= n 2)
          (- (* (nth 0 (nth 0 mat)) (nth 1 (nth 1 mat)))
             (* (nth 1 (nth 0 mat)) (nth 0 (nth 1 mat)))))
         (t
          (let ((sum 0) (c 0) (sign 1))
            (dolist (val (car mat))
              (setq sum (+ sum (* sign val
                                  (funcall 'neovm--mc-det
                                           (funcall 'neovm--mc-minor mat 0 c)))))
              (setq sign (- sign))
              (setq c (1+ c)))
            sum))))))

  (unwind-protect
      (list
        ;; 1x1 determinant
        (funcall 'neovm--mc-det '((5)))
        ;; 2x2 determinants
        (funcall 'neovm--mc-det '((1 2) (3 4)))
        (funcall 'neovm--mc-det '((5 0) (0 5)))
        (funcall 'neovm--mc-det '((1 0) (0 1)))
        ;; Singular 2x2
        (funcall 'neovm--mc-det '((2 4) (1 2)))
        ;; 3x3 determinants
        (funcall 'neovm--mc-det '((1 0 0) (0 1 0) (0 0 1)))
        (funcall 'neovm--mc-det '((1 2 3) (4 5 6) (7 8 9)))
        (funcall 'neovm--mc-det '((6 1 1) (4 -2 5) (2 8 7)))
        (funcall 'neovm--mc-det '((2 1 1) (1 3 2) (1 0 0)))
        ;; 4x4 determinant
        (funcall 'neovm--mc-det '((1 0 2 -1)
                                   (3 0 0 5)
                                   (2 1 4 -3)
                                   (1 0 5 0)))
        ;; Permutation matrix: det = +1 or -1
        (funcall 'neovm--mc-det '((0 1 0) (0 0 1) (1 0 0)))
        ;; det of negative identity
        (funcall 'neovm--mc-det '((-1 0) (0 -1)))
        ;; Minor computation
        (funcall 'neovm--mc-minor '((1 2 3) (4 5 6) (7 8 9)) 0 0)
        (funcall 'neovm--mc-minor '((1 2 3) (4 5 6) (7 8 9)) 1 1))
    (fmakunbound 'neovm--mc-minor)
    (fmakunbound 'neovm--mc-det)))"#;
    let expect = expect_test::expect![[
        r#""OK (5 -2 25 1 0 1 0 -306 -1 30 1 1 ((5 6) (8 9)) ((1 3) (7 9)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Trace and Hadamard product
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_matrix_trace_and_hadamard() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Trace: sum of diagonal
  (fset 'neovm--mc-trace
    (lambda (mat)
      (let ((sum 0) (i 0) (n (length mat)))
        (while (< i n)
          (setq sum (+ sum (nth i (nth i mat))))
          (setq i (1+ i)))
        sum)))

  ;; Hadamard (element-wise) product
  (fset 'neovm--mc-hadamard
    (lambda (a b)
      (let ((result nil) (ra a) (rb b))
        (while ra
          (let ((row-a (car ra)) (row-b (car rb)) (new-row nil))
            (while row-a
              (setq new-row (cons (* (car row-a) (car row-b)) new-row))
              (setq row-a (cdr row-a) row-b (cdr row-b)))
            (setq result (cons (nreverse new-row) result)))
          (setq ra (cdr ra) rb (cdr rb)))
        (nreverse result))))

  (unwind-protect
      (list
        ;; Trace of identity
        (funcall 'neovm--mc-trace '((1 0 0) (0 1 0) (0 0 1)))
        ;; Trace of 2x2
        (funcall 'neovm--mc-trace '((5 3) (2 7)))
        ;; Trace of 3x3
        (funcall 'neovm--mc-trace '((1 2 3) (4 5 6) (7 8 9)))
        ;; Trace of 1x1
        (funcall 'neovm--mc-trace '((42)))
        ;; Trace property: tr(A+B) = tr(A) + tr(B)
        (let ((a '((1 2) (3 4)))
              (b '((5 6) (7 8))))
          (let ((sum-trace (+ (funcall 'neovm--mc-trace a)
                              (funcall 'neovm--mc-trace b)))
                (add-mat (list (list (+ 1 5) (+ 2 6))
                               (list (+ 3 7) (+ 4 8)))))
            (= sum-trace (funcall 'neovm--mc-trace add-mat))))
        ;; Hadamard product
        (funcall 'neovm--mc-hadamard '((1 2) (3 4)) '((5 6) (7 8)))
        ;; Hadamard with identity
        (funcall 'neovm--mc-hadamard '((2 3) (4 5)) '((1 0) (0 1)))
        ;; Hadamard with zeros
        (funcall 'neovm--mc-hadamard '((1 2) (3 4)) '((0 0) (0 0)))
        ;; Hadamard commutativity
        (let ((a '((1 2 3) (4 5 6)))
              (b '((7 8 9) (10 11 12))))
          (equal (funcall 'neovm--mc-hadamard a b)
                 (funcall 'neovm--mc-hadamard b a)))
        ;; Hadamard with self = element-wise square
        (funcall 'neovm--mc-hadamard '((2 3) (4 5)) '((2 3) (4 5))))
    (fmakunbound 'neovm--mc-trace)
    (fmakunbound 'neovm--mc-hadamard)))"#;
    let expect = expect_test::expect![[
        r#""OK (3 12 15 42 t ((5 12) (21 32)) ((2 0) (0 5)) ((0 0) (0 0)) t ((4 9) (16 25)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Kronecker product
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_matrix_kronecker_product() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Kronecker product: A (mxn) kron B (pxq) = (mp x nq) matrix
  ;; Each element a_ij of A is replaced by a_ij * B
  (fset 'neovm--mc-kronecker
    (lambda (a b)
      (let ((result nil)
            (p (length b))
            (q (if b (length (car b)) 0)))
        (dolist (row-a a)
          ;; For each row of A, produce p rows in the result
          (let ((block-rows (make-list p nil)))
            ;; Initialize block-rows as empty lists
            (let ((i 0))
              (while (< i p) (setcar (nthcdr i block-rows) nil) (setq i (1+ i))))
            (dolist (a-val row-a)
              ;; Append a_val * row-b to each corresponding block row
              (let ((bi 0))
                (dolist (row-b b)
                  (let ((scaled nil))
                    (dolist (b-val row-b)
                      (setq scaled (cons (* a-val b-val) scaled)))
                    (setcar (nthcdr bi block-rows)
                            (append (nth bi block-rows) (nreverse scaled))))
                  (setq bi (1+ bi)))))
            (dolist (br block-rows)
              (setq result (cons br result)))))
        (nreverse result))))

  (unwind-protect
      (list
        ;; 2x2 kron 2x2
        (funcall 'neovm--mc-kronecker '((1 2) (3 4)) '((0 5) (6 7)))
        ;; I2 kron A = block diagonal
        (funcall 'neovm--mc-kronecker '((1 0) (0 1)) '((1 2) (3 4)))
        ;; A kron I2
        (funcall 'neovm--mc-kronecker '((1 2) (3 4)) '((1 0) (0 1)))
        ;; 1x1 kron anything = scaled matrix
        (funcall 'neovm--mc-kronecker '((3)) '((1 2) (3 4)))
        ;; 2x2 kron 1x1
        (funcall 'neovm--mc-kronecker '((1 2) (3 4)) '((5)))
        ;; Dimension check: 2x2 kron 3x3 = 6x6
        (let ((result (funcall 'neovm--mc-kronecker
                               '((1 0) (0 1))
                               '((1 0 0) (0 1 0) (0 0 1)))))
          (list (length result) (length (car result))))
        ;; Kronecker with zero matrix
        (funcall 'neovm--mc-kronecker '((1 2) (3 4)) '((0 0) (0 0))))
    (fmakunbound 'neovm--mc-kronecker)))"#;
    let expect = expect_test::expect![[
        r#""OK (((0 5 0 10) (6 7 12 14) (0 15 0 20) (18 21 24 28)) ((1 2 0 0) (3 4 0 0) (0 0 1 2) (0 0 3 4)) ((1 0 2 0) (0 1 0 2) (3 0 4 0) (0 3 0 4)) ((3 6) (9 12)) ((5 10) (15 20)) (6 6) ((0 0 0 0) (0 0 0 0) (0 0 0 0) (0 0 0 0)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Row echelon form via Gaussian elimination
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_matrix_row_echelon() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Deep copy a matrix
  (fset 'neovm--mc-copy
    (lambda (mat) (mapcar #'copy-sequence mat)))

  ;; Get/Set element helpers
  (fset 'neovm--mc-get (lambda (mat r c) (nth c (nth r mat))))
  (fset 'neovm--mc-set!
    (lambda (mat r c val) (setcar (nthcdr c (nth r mat)) val)))

  ;; Swap two rows (destructive)
  (fset 'neovm--mc-swap-rows!
    (lambda (mat r1 r2)
      (when (/= r1 r2)
        (let ((tmp (nth r1 mat)))
          (setcar (nthcdr r1 mat) (nth r2 mat))
          (setcar (nthcdr r2 mat) tmp)))
      mat))

  ;; Integer row echelon form (no division — uses scaling to avoid fractions)
  ;; Returns the matrix in REF (not reduced)
  (fset 'neovm--mc-ref
    (lambda (mat-orig)
      (let* ((mat (funcall 'neovm--mc-copy mat-orig))
             (m (length mat))
             (n (if mat (length (car mat)) 0))
             (pivot-row 0)
             (pivot-col 0))
        (while (and (< pivot-row m) (< pivot-col n))
          ;; Find pivot in column
          (let ((max-row pivot-row)
                (found nil)
                (r pivot-row))
            (while (< r m)
              (when (/= (funcall 'neovm--mc-get mat r pivot-col) 0)
                (setq max-row r found t)
                (setq r m))  ;; break
              (setq r (1+ r)))
            (if (not found)
                (setq pivot-col (1+ pivot-col))
              ;; Swap pivot row
              (funcall 'neovm--mc-swap-rows! mat pivot-row max-row)
              ;; Eliminate below
              (let ((pivot-val (funcall 'neovm--mc-get mat pivot-row pivot-col))
                    (r (1+ pivot-row)))
                (while (< r m)
                  (let ((factor (funcall 'neovm--mc-get mat r pivot-col)))
                    (when (/= factor 0)
                      (let ((c 0))
                        (while (< c n)
                          (funcall 'neovm--mc-set! mat r c
                                   (- (* pivot-val (funcall 'neovm--mc-get mat r c))
                                      (* factor (funcall 'neovm--mc-get mat pivot-row c))))
                          (setq c (1+ c))))))
                  (setq r (1+ r))))
              (setq pivot-row (1+ pivot-row))
              (setq pivot-col (1+ pivot-col)))))
        mat)))

  (unwind-protect
      (list
        ;; Already in REF
        (funcall 'neovm--mc-ref '((1 2 3) (0 1 2) (0 0 1)))
        ;; Needs elimination
        (funcall 'neovm--mc-ref '((2 1 -1) (-3 -1 2) (-2 1 2)))
        ;; Singular matrix (row of zeros appears)
        (funcall 'neovm--mc-ref '((1 2 3) (2 4 6) (1 3 5)))
        ;; 2x3 matrix
        (funcall 'neovm--mc-ref '((1 2 3) (4 5 6)))
        ;; Identity is already in REF
        (funcall 'neovm--mc-ref '((1 0 0) (0 1 0) (0 0 1)))
        ;; 1x1
        (funcall 'neovm--mc-ref '((7)))
        ;; Need row swap
        (funcall 'neovm--mc-ref '((0 1) (1 0)))
        ;; 4x4
        (funcall 'neovm--mc-ref '((1 2 3 4) (2 3 4 5) (3 4 5 6) (4 5 6 7))))
    (fmakunbound 'neovm--mc-copy)
    (fmakunbound 'neovm--mc-get)
    (fmakunbound 'neovm--mc-set!)
    (fmakunbound 'neovm--mc-swap-rows!)
    (fmakunbound 'neovm--mc-ref)))"#;
    let expect = expect_test::expect![[
        r#""OK (((1 2 3) (0 1 2) (0 0 1)) ((2 1 -1) (0 1 1) (0 0 -2)) ((1 2 3) (0 1 2) (0 0 0)) ((1 2 3) (0 -3 -6)) ((1 0 0) (0 1 0) (0 0 1)) ((7)) ((1 0) (0 1)) ((1 2 3 4) (0 -1 -2 -3) (0 0 0 0) (0 0 0 0)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Matrix equality with epsilon for floats
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_matrix_equality_epsilon() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Approximate matrix equality: all elements within epsilon
  (fset 'neovm--mc-approx-equal
    (lambda (a b epsilon)
      (let ((ok t) (ra a) (rb b))
        (while (and ok ra)
          (let ((row-a (car ra)) (row-b (car rb)))
            (while (and ok row-a)
              (unless (<= (abs (- (float (car row-a)) (float (car row-b)))) epsilon)
                (setq ok nil))
              (setq row-a (cdr row-a) row-b (cdr row-b))))
          (setq ra (cdr ra) rb (cdr rb)))
        ok)))

  (unwind-protect
      (list
        ;; Exact equality
        (funcall 'neovm--mc-approx-equal
                 '((1 2) (3 4)) '((1 2) (3 4)) 0.0)
        ;; Within epsilon
        (funcall 'neovm--mc-approx-equal
                 '((1.0 2.0) (3.0 4.0))
                 '((1.001 1.999) (3.001 3.999))
                 0.01)
        ;; Not within epsilon
        (funcall 'neovm--mc-approx-equal
                 '((1.0 2.0)) '((1.1 2.0)) 0.01)
        ;; Float vs integer
        (funcall 'neovm--mc-approx-equal
                 '((1 2 3)) '((1.0 2.0 3.0)) 0.0001)
        ;; Large epsilon: everything matches
        (funcall 'neovm--mc-approx-equal
                 '((0 0) (0 0)) '((100 200) (300 400)) 500.0)
        ;; Zero matrix vs small perturbation
        (funcall 'neovm--mc-approx-equal
                 '((0.0 0.0) (0.0 0.0))
                 '((1e-10 -1e-10) (1e-10 -1e-10))
                 1e-8)
        ;; Negative numbers
        (funcall 'neovm--mc-approx-equal
                 '((-1.0 -2.0)) '((-1.005 -2.005)) 0.01)
        ;; Single element
        (funcall 'neovm--mc-approx-equal '((3.14)) '((3.14159)) 0.01))
    (fmakunbound 'neovm--mc-approx-equal)))"#;
    let expect = expect_test::expect![[r#""OK (t t nil t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Matrix power (repeated multiplication)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_matrix_power() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--mc-transpose
    (lambda (mat)
      (if (null mat) nil
        (let ((ncols (length (car mat))) (result nil) (c 0))
          (while (< c ncols)
            (let ((col nil) (rows mat))
              (while rows
                (setq col (cons (nth c (car rows)) col))
                (setq rows (cdr rows)))
              (setq result (cons (nreverse col) result)))
            (setq c (1+ c)))
          (nreverse result)))))

  (fset 'neovm--mc-dot
    (lambda (a b)
      (let ((sum 0))
        (while a
          (setq sum (+ sum (* (car a) (car b))))
          (setq a (cdr a) b (cdr b)))
        sum)))

  (fset 'neovm--mc-mult
    (lambda (a b)
      (let ((bt (funcall 'neovm--mc-transpose b)))
        (mapcar (lambda (row-a)
                  (mapcar (lambda (col-b)
                            (funcall 'neovm--mc-dot row-a col-b))
                          bt))
                a))))

  (fset 'neovm--mc-identity
    (lambda (n)
      (let ((result nil) (r 0))
        (while (< r n)
          (let ((row (make-list n 0)))
            (setcar (nthcdr r row) 1)
            (setq result (cons row result)))
          (setq r (1+ r)))
        (nreverse result))))

  ;; Matrix power: A^n using repeated multiplication
  (fset 'neovm--mc-power
    (lambda (mat n)
      (if (= n 0)
          (funcall 'neovm--mc-identity (length mat))
        (let ((result mat)
              (i 1))
          (while (< i n)
            (setq result (funcall 'neovm--mc-mult result mat))
            (setq i (1+ i)))
          result))))

  (unwind-protect
      (let ((a '((1 1) (0 1)))
            (b '((2 0) (0 3))))
        (list
          ;; A^0 = I
          (funcall 'neovm--mc-power a 0)
          ;; A^1 = A
          (equal a (funcall 'neovm--mc-power a 1))
          ;; A^2
          (funcall 'neovm--mc-power a 2)
          ;; A^3
          (funcall 'neovm--mc-power a 3)
          ;; A^5
          (funcall 'neovm--mc-power a 5)
          ;; Diagonal matrix power: each diagonal element raised to n
          (funcall 'neovm--mc-power b 2)
          (funcall 'neovm--mc-power b 3)
          ;; Identity^n = Identity
          (equal (funcall 'neovm--mc-identity 3)
                 (funcall 'neovm--mc-power (funcall 'neovm--mc-identity 3) 10))
          ;; A^2 = A*A
          (equal (funcall 'neovm--mc-power a 2)
                 (funcall 'neovm--mc-mult a a))
          ;; A^3 = A*A*A
          (equal (funcall 'neovm--mc-power a 3)
                 (funcall 'neovm--mc-mult a (funcall 'neovm--mc-mult a a)))
          ;; Nilpotent-like: ((0 1) (0 0))^2 = zero
          (funcall 'neovm--mc-power '((0 1) (0 0)) 2)
          ;; 3x3 power
          (funcall 'neovm--mc-power '((1 1 0) (0 1 1) (0 0 1)) 3)))
    (fmakunbound 'neovm--mc-transpose)
    (fmakunbound 'neovm--mc-dot)
    (fmakunbound 'neovm--mc-mult)
    (fmakunbound 'neovm--mc-identity)
    (fmakunbound 'neovm--mc-power)))"#;
    let expect = expect_test::expect![[
        r#""OK (((1 0) (0 1)) t ((1 2) (0 1)) ((1 3) (0 1)) ((1 5) (0 1)) ((4 0) (0 9)) ((8 0) (0 27)) t t t ((0 0) (0 0)) ((1 3 3) (0 1 3) (0 0 1)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Linear system solver (Gaussian elimination with back substitution)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_matrix_linear_solver() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--mc-copy (lambda (mat) (mapcar #'copy-sequence mat)))
  (fset 'neovm--mc-get (lambda (mat r c) (nth c (nth r mat))))
  (fset 'neovm--mc-set!
    (lambda (mat r c val) (setcar (nthcdr c (nth r mat)) val)))

  ;; Solve Ax=b using integer Gaussian elimination.
  ;; Input: augmented matrix [A|b].
  ;; Returns list of (numerator . denominator) pairs.
  (fset 'neovm--mc-solve
    (lambda (aug-orig)
      (let* ((aug (funcall 'neovm--mc-copy aug-orig))
             (n (length aug))
             (ncols (length (car aug)))
             (pivot 0))
        ;; Forward elimination
        (while (< pivot n)
          (let ((pv (funcall 'neovm--mc-get aug pivot pivot))
                (r (1+ pivot)))
            (while (< r n)
              (let ((factor (funcall 'neovm--mc-get aug r pivot)))
                (when (/= factor 0)
                  (let ((c 0))
                    (while (< c ncols)
                      (funcall 'neovm--mc-set! aug r c
                               (- (* pv (funcall 'neovm--mc-get aug r c))
                                  (* factor (funcall 'neovm--mc-get aug pivot c))))
                      (setq c (1+ c))))))
              (setq r (1+ r))))
          (setq pivot (1+ pivot)))
        ;; Back substitution
        (let ((sol (make-list n nil))
              (row (1- n)))
          (while (>= row 0)
            (let ((rhs (funcall 'neovm--mc-get aug row (1- ncols)))
                  (denom (funcall 'neovm--mc-get aug row row))
                  (col (1+ row)))
              (while (< col n)
                (let ((s (nth col sol)))
                  (when s
                    (setq rhs (- (* rhs (cdr s))
                                 (* (funcall 'neovm--mc-get aug row col) (car s))))
                    (setq denom (* denom (cdr s)))))
                (setq col (1+ col)))
              (setcar (nthcdr row sol) (cons rhs denom)))
            (setq row (1- row)))
          sol))))

  (unwind-protect
      (list
        ;; x + 2y = 5, 3x + 4y = 11 => x=1, y=2
        (let ((sol (funcall 'neovm--mc-solve '((1 2 5) (3 4 11)))))
          (mapcar (lambda (s) (/ (car s) (cdr s))) sol))
        ;; 2x + y = 5, x - y = 1 => x=2, y=1
        (let ((sol (funcall 'neovm--mc-solve '((2 1 5) (1 -1 1)))))
          (mapcar (lambda (s) (/ (car s) (cdr s))) sol))
        ;; 3x3: x+y+z=6, 2x+3y+z=14, x+y+3z=12 => x=1, y=3, z=2
        (let ((sol (funcall 'neovm--mc-solve '((1 1 1 6) (2 3 1 14) (1 1 3 12)))))
          (mapcar (lambda (s) (/ (car s) (cdr s))) sol))
        ;; 2x2: 3x + 2y = 12, x + 4y = 10 => x=2.8, y=1.8
        ;; Use rational form
        (let ((sol (funcall 'neovm--mc-solve '((3 2 12) (1 4 10)))))
          (mapcar (lambda (s) (cons (car s) (cdr s))) sol))
        ;; Diagonal system: 2x=6, 3y=9 => x=3, y=3
        (let ((sol (funcall 'neovm--mc-solve '((2 0 6) (0 3 9)))))
          (mapcar (lambda (s) (/ (car s) (cdr s))) sol)))
    (fmakunbound 'neovm--mc-copy)
    (fmakunbound 'neovm--mc-get)
    (fmakunbound 'neovm--mc-set!)
    (fmakunbound 'neovm--mc-solve)))"#;
    let expect =
        expect_test::expect![[r#""OK ((1 2) (2 1) (0 5 3) ((84 . 30) (18 . 10)) (3 3))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Matrix from-list constructor and Frobenius norm
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_matrix_from_list_and_frobenius() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Create matrix from flat list: (from-list '(1 2 3 4 5 6) 2 3) => ((1 2 3) (4 5 6))
  (fset 'neovm--mc-from-list
    (lambda (flat rows cols)
      (let ((result nil) (r 0) (lst flat))
        (while (< r rows)
          (let ((row nil) (c 0))
            (while (< c cols)
              (setq row (cons (car lst) row))
              (setq lst (cdr lst))
              (setq c (1+ c)))
            (setq result (cons (nreverse row) result)))
          (setq r (1+ r)))
        (nreverse result))))

  ;; Flatten matrix to list
  (fset 'neovm--mc-to-list
    (lambda (mat)
      (apply #'append mat)))

  ;; Frobenius norm squared: sum of squares of all elements
  (fset 'neovm--mc-frobenius-sq
    (lambda (mat)
      (let ((sum 0))
        (dolist (row mat)
          (dolist (x row)
            (setq sum (+ sum (* x x)))))
        sum)))

  ;; Matrix map: apply function to every element
  (fset 'neovm--mc-map
    (lambda (f mat)
      (mapcar (lambda (row) (mapcar f row)) mat)))

  (unwind-protect
      (list
        ;; from-list basic
        (funcall 'neovm--mc-from-list '(1 2 3 4 5 6) 2 3)
        (funcall 'neovm--mc-from-list '(1 2 3 4) 2 2)
        (funcall 'neovm--mc-from-list '(42) 1 1)
        (funcall 'neovm--mc-from-list '(1 2 3 4 5 6 7 8 9) 3 3)
        ;; to-list roundtrip
        (funcall 'neovm--mc-to-list '((1 2 3) (4 5 6)))
        (equal (funcall 'neovm--mc-to-list
                        (funcall 'neovm--mc-from-list '(1 2 3 4 5 6) 2 3))
               '(1 2 3 4 5 6))
        ;; Frobenius norm squared
        (funcall 'neovm--mc-frobenius-sq '((1 0 0) (0 1 0) (0 0 1)))
        (funcall 'neovm--mc-frobenius-sq '((1 2) (3 4)))
        (funcall 'neovm--mc-frobenius-sq '((0 0) (0 0)))
        (funcall 'neovm--mc-frobenius-sq '((3)))
        ;; Matrix map: negate all elements
        (funcall 'neovm--mc-map #'- '((1 2) (3 4)))
        ;; Matrix map: double
        (funcall 'neovm--mc-map (lambda (x) (* 2 x)) '((1 2 3) (4 5 6)))
        ;; Matrix map: absolute value
        (funcall 'neovm--mc-map #'abs '((-1 2) (-3 4))))
    (fmakunbound 'neovm--mc-from-list)
    (fmakunbound 'neovm--mc-to-list)
    (fmakunbound 'neovm--mc-frobenius-sq)
    (fmakunbound 'neovm--mc-map)))"#;
    let expect = expect_test::expect![[
        r#""OK (((1 2 3) (4 5 6)) ((1 2) (3 4)) ((42)) ((1 2 3) (4 5 6) (7 8 9)) (1 2 3 4 5 6) t 3 30 0 9 ((-1 -2) (-3 -4)) ((2 4 6) (8 10 12)) ((1 2) (3 4)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Comprehensive integration: determinant, trace, eigenvalue relation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_matrix_integration_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Utility functions for integration tests
  (fset 'neovm--mc-minor
    (lambda (mat r c)
      (let ((result nil) (ri 0))
        (dolist (row mat)
          (unless (= ri r)
            (let ((new-row nil) (ci 0))
              (dolist (val row)
                (unless (= ci c)
                  (setq new-row (cons val new-row)))
                (setq ci (1+ ci)))
              (setq result (cons (nreverse new-row) result))))
          (setq ri (1+ ri)))
        (nreverse result))))

  (fset 'neovm--mc-det
    (lambda (mat)
      (let ((n (length mat)))
        (cond
         ((= n 1) (caar mat))
         ((= n 2) (- (* (nth 0 (nth 0 mat)) (nth 1 (nth 1 mat)))
                     (* (nth 1 (nth 0 mat)) (nth 0 (nth 1 mat)))))
         (t (let ((sum 0) (c 0) (sign 1))
              (dolist (val (car mat))
                (setq sum (+ sum (* sign val
                                    (funcall 'neovm--mc-det
                                             (funcall 'neovm--mc-minor mat 0 c)))))
                (setq sign (- sign))
                (setq c (1+ c)))
              sum))))))

  (fset 'neovm--mc-trace
    (lambda (mat)
      (let ((sum 0) (i 0) (n (length mat)))
        (while (< i n)
          (setq sum (+ sum (nth i (nth i mat))))
          (setq i (1+ i)))
        sum)))

  (fset 'neovm--mc-transpose
    (lambda (mat)
      (if (null mat) nil
        (let ((ncols (length (car mat))) (result nil) (c 0))
          (while (< c ncols)
            (let ((col nil) (rows mat))
              (while rows
                (setq col (cons (nth c (car rows)) col))
                (setq rows (cdr rows)))
              (setq result (cons (nreverse col) result)))
            (setq c (1+ c)))
          (nreverse result)))))

  (fset 'neovm--mc-scale
    (lambda (s mat)
      (mapcar (lambda (row) (mapcar (lambda (x) (* s x)) row)) mat)))

  (unwind-protect
      (let ((a '((1 2) (3 4)))
            (b '((5 6) (7 8)))
            (c3 '((2 1 1) (1 3 2) (1 0 0))))
        (list
          ;; det(A) for 2x2
          (funcall 'neovm--mc-det a)
          ;; trace(A) for 2x2
          (funcall 'neovm--mc-trace a)
          ;; Property: det(kA) = k^n * det(A) for n×n matrix
          (let ((k 3) (n 2))
            (= (funcall 'neovm--mc-det (funcall 'neovm--mc-scale k a))
               (* (expt k n) (funcall 'neovm--mc-det a))))
          ;; det(A^T) = det(A)
          (= (funcall 'neovm--mc-det a)
             (funcall 'neovm--mc-det (funcall 'neovm--mc-transpose a)))
          ;; trace(A^T) = trace(A)
          (= (funcall 'neovm--mc-trace a)
             (funcall 'neovm--mc-trace (funcall 'neovm--mc-transpose a)))
          ;; trace(kA) = k * trace(A)
          (= (funcall 'neovm--mc-trace (funcall 'neovm--mc-scale 5 a))
             (* 5 (funcall 'neovm--mc-trace a)))
          ;; 3x3 tests
          (funcall 'neovm--mc-det c3)
          (funcall 'neovm--mc-trace c3)
          ;; det(I) = 1
          (funcall 'neovm--mc-det '((1 0 0) (0 1 0) (0 0 1)))
          ;; Singular matrix: det = 0
          (funcall 'neovm--mc-det '((1 2 3) (4 5 6) (7 8 9)))
          ;; det of upper triangular = product of diagonal
          (let ((upper '((2 3 1) (0 4 5) (0 0 6))))
            (= (funcall 'neovm--mc-det upper) (* 2 4 6)))))
    (fmakunbound 'neovm--mc-minor)
    (fmakunbound 'neovm--mc-det)
    (fmakunbound 'neovm--mc-trace)
    (fmakunbound 'neovm--mc-transpose)
    (fmakunbound 'neovm--mc-scale)))"#;
    let expect = expect_test::expect![[r#""OK (-2 5 t t t t -1 5 1 0 t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
