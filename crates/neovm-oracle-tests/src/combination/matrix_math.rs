//! Oracle parity tests for matrix mathematics in Elisp:
//! matrix as list-of-lists, matrix addition, matrix multiplication,
//! transpose, determinant (2x2, 3x3), identity matrix, scalar multiplication,
//! trace, row operations, and row echelon form.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Matrix addition with various dimensions and edge cases
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_matrix_math_addition() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--test-mm-add
    (lambda (a b)
      (let ((result nil) (ra a) (rb b))
        (while ra
          (let ((row-a (car ra)) (row-b (car rb)) (new-row nil))
            (while row-a
              (setq new-row (cons (+ (car row-a) (car row-b)) new-row))
              (setq row-a (cdr row-a) row-b (cdr row-b)))
            (setq result (cons (nreverse new-row) result)))
          (setq ra (cdr ra) rb (cdr rb)))
        (nreverse result))))

  (fset 'neovm--test-mm-zeros
    (lambda (rows cols)
      (let ((result nil) (r 0))
        (while (< r rows)
          (setq result (cons (make-list cols 0) result))
          (setq r (1+ r)))
        (nreverse result))))

  (fset 'neovm--test-mm-negate
    (lambda (mat)
      (mapcar (lambda (row) (mapcar (lambda (x) (- x)) row)) mat)))

  (unwind-protect
      (let ((a '((1 2 3) (4 5 6) (7 8 9)))
            (b '((9 8 7) (6 5 4) (3 2 1)))
            (c '((-1 -2 -3) (-4 -5 -6) (-7 -8 -9))))
        (list
         ;; Basic addition
         (funcall 'neovm--test-mm-add a b)
         ;; A + zero = A
         (equal a (funcall 'neovm--test-mm-add a (funcall 'neovm--test-mm-zeros 3 3)))
         ;; A + (-A) = zero
         (equal (funcall 'neovm--test-mm-zeros 3 3)
                (funcall 'neovm--test-mm-add a (funcall 'neovm--test-mm-negate a)))
         ;; Commutativity: A + B = B + A
         (equal (funcall 'neovm--test-mm-add a b)
                (funcall 'neovm--test-mm-add b a))
         ;; Associativity: (A + B) + C = A + (B + C)
         (equal (funcall 'neovm--test-mm-add (funcall 'neovm--test-mm-add a b) c)
                (funcall 'neovm--test-mm-add a (funcall 'neovm--test-mm-add b c)))
         ;; 1x1 matrix addition
         (funcall 'neovm--test-mm-add '((5)) '((3)))
         ;; 2x4 rectangle
         (funcall 'neovm--test-mm-add '((1 2 3 4) (5 6 7 8))
                                      '((8 7 6 5) (4 3 2 1)))))
    (fmakunbound 'neovm--test-mm-add)
    (fmakunbound 'neovm--test-mm-zeros)
    (fmakunbound 'neovm--test-mm-negate)))"#;
    let expect = expect_test::expect![[
        r#""OK (((10 10 10) (10 10 10) (10 10 10)) t t t t ((8)) ((9 9 9 9) (9 9 9 9)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Matrix multiplication with dimension checks
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_matrix_math_multiplication() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--test-mm-transpose
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

  (fset 'neovm--test-mm-dot
    (lambda (a b)
      (let ((sum 0))
        (while a
          (setq sum (+ sum (* (car a) (car b))))
          (setq a (cdr a) b (cdr b)))
        sum)))

  (fset 'neovm--test-mm-mult
    (lambda (a b)
      (let ((bt (funcall 'neovm--test-mm-transpose b)))
        (mapcar (lambda (row-a)
                  (mapcar (lambda (col-b)
                            (funcall 'neovm--test-mm-dot row-a col-b))
                          bt))
                a))))

  (fset 'neovm--test-mm-identity
    (lambda (n)
      (let ((result nil) (r 0))
        (while (< r n)
          (let ((row (make-list n 0)))
            (setcar (nthcdr r row) 1)
            (setq result (cons row result)))
          (setq r (1+ r)))
        (nreverse result))))

  (unwind-protect
      (let ((a '((1 2) (3 4)))
            (b '((5 6) (7 8)))
            (rect '((1 2 3) (4 5 6)))
            (col '((1) (2) (3)))
            (row '((1 2 3)))
            (i3 (funcall 'neovm--test-mm-identity 3)))
        (list
         ;; 2x2 * 2x2
         (funcall 'neovm--test-mm-mult a b)
         ;; A * I = A
         (let ((i2 (funcall 'neovm--test-mm-identity 2)))
           (equal a (funcall 'neovm--test-mm-mult a i2)))
         ;; I * A = A
         (let ((i2 (funcall 'neovm--test-mm-identity 2)))
           (equal a (funcall 'neovm--test-mm-mult i2 a)))
         ;; Non-commutativity: A*B != B*A (generally)
         (equal (funcall 'neovm--test-mm-mult a b)
                (funcall 'neovm--test-mm-mult b a))
         ;; Rectangular: (2x3) * (3x1) = (2x1)
         (funcall 'neovm--test-mm-mult rect col)
         ;; (1x3) * (3x1) = (1x1)
         (funcall 'neovm--test-mm-mult row col)
         ;; (3x1) * (1x3) = (3x3)
         (funcall 'neovm--test-mm-mult col row)
         ;; Associativity: (A*B)*C = A*(B*C) for compatible matrices
         (let ((c '((2 0) (0 3))))
           (equal (funcall 'neovm--test-mm-mult
                           (funcall 'neovm--test-mm-mult a b) c)
                  (funcall 'neovm--test-mm-mult
                           a (funcall 'neovm--test-mm-mult b c))))))
    (fmakunbound 'neovm--test-mm-transpose)
    (fmakunbound 'neovm--test-mm-dot)
    (fmakunbound 'neovm--test-mm-mult)
    (fmakunbound 'neovm--test-mm-identity)))"#;
    let expect = expect_test::expect![[
        r#""OK (((19 22) (43 50)) t t nil ((14) (32)) ((14)) ((1 2 3) (2 4 6) (3 6 9)) t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Transpose properties
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_matrix_math_transpose_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--test-mm-tr
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

  (fset 'neovm--test-mm-scale
    (lambda (s mat)
      (mapcar (lambda (row) (mapcar (lambda (x) (* s x)) row)) mat)))

  (fset 'neovm--test-mm-add2
    (lambda (a b)
      (let ((result nil) (ra a) (rb b))
        (while ra
          (let ((row-a (car ra)) (row-b (car rb)) (new-row nil))
            (while row-a
              (setq new-row (cons (+ (car row-a) (car row-b)) new-row))
              (setq row-a (cdr row-a) row-b (cdr row-b)))
            (setq result (cons (nreverse new-row) result)))
          (setq ra (cdr ra) rb (cdr rb)))
        (nreverse result))))

  (unwind-protect
      (let ((m '((1 2 3) (4 5 6)))
            (sq '((1 2 3) (4 5 6) (7 8 9)))
            (sym '((1 7 3) (7 4 5) (3 5 6))))
        (list
         ;; (M^T)^T = M
         (equal m (funcall 'neovm--test-mm-tr
                           (funcall 'neovm--test-mm-tr m)))
         ;; Symmetric matrix: M^T = M
         (equal sym (funcall 'neovm--test-mm-tr sym))
         ;; (A+B)^T = A^T + B^T
         (let ((a '((1 2) (3 4) (5 6)))
               (b '((7 8) (9 10) (11 12))))
           (equal (funcall 'neovm--test-mm-tr
                           (funcall 'neovm--test-mm-add2 a b))
                  (funcall 'neovm--test-mm-add2
                           (funcall 'neovm--test-mm-tr a)
                           (funcall 'neovm--test-mm-tr b))))
         ;; (cA)^T = c * A^T
         (equal (funcall 'neovm--test-mm-tr
                         (funcall 'neovm--test-mm-scale 5 m))
                (funcall 'neovm--test-mm-scale 5
                         (funcall 'neovm--test-mm-tr m)))
         ;; Dimensions: 2x3 -> 3x2
         (let ((tr (funcall 'neovm--test-mm-tr m)))
           (list (length tr) (length (car tr))))
         ;; 1x4 -> 4x1
         (funcall 'neovm--test-mm-tr '((10 20 30 40)))
         ;; 4x1 -> 1x4
         (funcall 'neovm--test-mm-tr '((10) (20) (30) (40)))))
    (fmakunbound 'neovm--test-mm-tr)
    (fmakunbound 'neovm--test-mm-scale)
    (fmakunbound 'neovm--test-mm-add2)))"#;
    let expect =
        expect_test::expect![[r#""OK (t t t t (3 2) ((10) (20) (30) (40)) ((10 20 30 40)))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Determinant: 2x2 and 3x3 with various cases
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_matrix_math_determinant() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--test-mm-det2
    (lambda (m)
      (- (* (nth 0 (nth 0 m)) (nth 1 (nth 1 m)))
         (* (nth 1 (nth 0 m)) (nth 0 (nth 1 m))))))

  (fset 'neovm--test-mm-det3
    (lambda (m)
      (let ((a (nth 0 (nth 0 m))) (b (nth 1 (nth 0 m))) (c (nth 2 (nth 0 m)))
            (d (nth 0 (nth 1 m))) (e (nth 1 (nth 1 m))) (f (nth 2 (nth 1 m)))
            (g (nth 0 (nth 2 m))) (h (nth 1 (nth 2 m))) (i (nth 2 (nth 2 m))))
        (+ (* a (- (* e i) (* f h)))
           (* (- b) (- (* d i) (* f g)))
           (* c (- (* d h) (* e g)))))))

  (unwind-protect
      (list
       ;; 2x2 identity: det = 1
       (funcall 'neovm--test-mm-det2 '((1 0) (0 1)))
       ;; 2x2 general
       (funcall 'neovm--test-mm-det2 '((3 7) (1 -4)))
       ;; 2x2 singular (det = 0): rows are proportional
       (funcall 'neovm--test-mm-det2 '((2 4) (3 6)))
       ;; 2x2 negative determinant
       (funcall 'neovm--test-mm-det2 '((0 1) (1 0)))
       ;; 2x2 with large values
       (funcall 'neovm--test-mm-det2 '((100 200) (300 401)))
       ;; 3x3 identity: det = 1
       (funcall 'neovm--test-mm-det3 '((1 0 0) (0 1 0) (0 0 1)))
       ;; 3x3 singular (linearly dependent rows): det = 0
       (funcall 'neovm--test-mm-det3 '((1 2 3) (4 5 6) (7 8 9)))
       ;; 3x3 general
       (funcall 'neovm--test-mm-det3 '((6 1 1) (4 -2 5) (2 8 7)))
       ;; 3x3 diagonal matrix: det = product of diagonal
       (funcall 'neovm--test-mm-det3 '((2 0 0) (0 3 0) (0 0 5)))
       ;; 3x3 upper triangular: det = product of diagonal
       (funcall 'neovm--test-mm-det3 '((2 3 4) (0 5 6) (0 0 7)))
       ;; Property: det(cA) = c^n * det(A) for n=2
       (let ((a '((1 2) (3 4))))
         (= (funcall 'neovm--test-mm-det2
                     (mapcar (lambda (row) (mapcar (lambda (x) (* 3 x)) row)) a))
            (* 9 (funcall 'neovm--test-mm-det2 a)))))
    (fmakunbound 'neovm--test-mm-det2)
    (fmakunbound 'neovm--test-mm-det3)))"#;
    let expect = expect_test::expect![[r#""OK (1 -19 0 -1 -19900 1 0 -306 30 70 t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Identity matrix and scalar multiplication
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_matrix_math_identity_and_scalar() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--test-mm-ident
    (lambda (n)
      (let ((result nil) (r 0))
        (while (< r n)
          (let ((row (make-list n 0)))
            (setcar (nthcdr r row) 1)
            (setq result (cons row result)))
          (setq r (1+ r)))
        (nreverse result))))

  (fset 'neovm--test-mm-scmul
    (lambda (s mat)
      (mapcar (lambda (row) (mapcar (lambda (x) (* s x)) row)) mat)))

  (unwind-protect
      (list
       ;; Identity matrices of various sizes
       (funcall 'neovm--test-mm-ident 1)
       (funcall 'neovm--test-mm-ident 2)
       (funcall 'neovm--test-mm-ident 3)
       (funcall 'neovm--test-mm-ident 4)
       ;; Scalar multiplication
       (funcall 'neovm--test-mm-scmul 0 '((1 2) (3 4)))
       (funcall 'neovm--test-mm-scmul 1 '((1 2) (3 4)))
       (funcall 'neovm--test-mm-scmul -1 '((1 2) (3 4)))
       (funcall 'neovm--test-mm-scmul 10 '((1 2) (3 4)))
       ;; Scalar mult distributivity: c*(A+B) = cA + cB
       (let ((a '((1 2) (3 4)))
             (b '((5 6) (7 8)))
             (c 3))
         (let ((sum-then-scale
                (funcall 'neovm--test-mm-scmul c
                         (mapcar (lambda (pair)
                                   (let ((ra (car pair)) (rb (cadr pair)))
                                     (list (+ (nth 0 ra) (nth 0 rb))
                                           (+ (nth 1 ra) (nth 1 rb)))))
                                 (list (list (nth 0 a) (nth 0 b))
                                       (list (nth 1 a) (nth 1 b))))))
               (scale-then-sum
                (let ((ca (funcall 'neovm--test-mm-scmul c a))
                      (cb (funcall 'neovm--test-mm-scmul c b)))
                  (list (list (+ (nth 0 (nth 0 ca)) (nth 0 (nth 0 cb)))
                              (+ (nth 1 (nth 0 ca)) (nth 1 (nth 0 cb))))
                        (list (+ (nth 0 (nth 1 ca)) (nth 0 (nth 1 cb)))
                              (+ (nth 1 (nth 1 ca)) (nth 1 (nth 1 cb))))))))
           (equal sum-then-scale scale-then-sum)))
       ;; Scalar mult of identity = diagonal matrix
       (funcall 'neovm--test-mm-scmul 7 (funcall 'neovm--test-mm-ident 3)))
    (fmakunbound 'neovm--test-mm-ident)
    (fmakunbound 'neovm--test-mm-scmul)))"#;
    let expect = expect_test::expect![[
        r#""OK (((1)) ((1 0) (0 1)) ((1 0 0) (0 1 0) (0 0 1)) ((1 0 0 0) (0 1 0 0) (0 0 1 0) (0 0 0 1)) ((0 0) (0 0)) ((1 2) (3 4)) ((-1 -2) (-3 -4)) ((10 20) (30 40)) t ((7 0 0) (0 7 0) (0 0 7)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Trace: sum of diagonal elements
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_matrix_math_trace() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--test-mm-trace
    (lambda (mat)
      (let ((sum 0) (i 0) (n (length mat)))
        (while (< i n)
          (setq sum (+ sum (nth i (nth i mat))))
          (setq i (1+ i)))
        sum)))

  (fset 'neovm--test-mm-tr2
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

  (unwind-protect
      (let ((i3 '((1 0 0) (0 1 0) (0 0 1)))
            (a '((5 3 1) (2 7 4) (6 8 9)))
            (b '((1 4 7) (2 5 8) (3 6 9))))
        (list
         ;; tr(I) = n
         (funcall 'neovm--test-mm-trace i3)
         ;; tr(A) for general matrix
         (funcall 'neovm--test-mm-trace a)
         ;; tr(A) = tr(A^T)
         (= (funcall 'neovm--test-mm-trace a)
            (funcall 'neovm--test-mm-trace (funcall 'neovm--test-mm-tr2 a)))
         ;; tr(A+B) = tr(A) + tr(B)
         (let ((apb (list
                     (list (+ 5 1) (+ 3 4) (+ 1 7))
                     (list (+ 2 2) (+ 7 5) (+ 4 8))
                     (list (+ 6 3) (+ 8 6) (+ 9 9)))))
           (= (funcall 'neovm--test-mm-trace apb)
              (+ (funcall 'neovm--test-mm-trace a)
                 (funcall 'neovm--test-mm-trace b))))
         ;; tr(cA) = c * tr(A)
         (let ((scaled (mapcar (lambda (row) (mapcar (lambda (x) (* 4 x)) row)) a)))
           (= (funcall 'neovm--test-mm-trace scaled)
              (* 4 (funcall 'neovm--test-mm-trace a))))
         ;; 1x1 trace
         (funcall 'neovm--test-mm-trace '((42)))
         ;; 2x2 trace
         (funcall 'neovm--test-mm-trace '((10 20) (30 40)))
         ;; Diagonal matrix trace = sum of diagonal entries
         (funcall 'neovm--test-mm-trace '((3 0 0 0) (0 5 0 0) (0 0 7 0) (0 0 0 11)))))
    (fmakunbound 'neovm--test-mm-trace)
    (fmakunbound 'neovm--test-mm-tr2)))"#;
    let expect = expect_test::expect![[r#""OK (3 21 t t t 42 50 26)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Row operations: swap, scale, add-multiple
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_matrix_math_row_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Deep copy matrix
  (fset 'neovm--test-mm-copy
    (lambda (mat) (mapcar #'copy-sequence mat)))

  ;; Swap rows i and j
  (fset 'neovm--test-mm-swap-rows
    (lambda (mat i j)
      (let ((m (funcall 'neovm--test-mm-copy mat)))
        (let ((tmp (nth i m)))
          (setcar (nthcdr i m) (nth j m))
          (setcar (nthcdr j m) tmp))
        m)))

  ;; Scale row i by factor k
  (fset 'neovm--test-mm-scale-row
    (lambda (mat i k)
      (let ((m (funcall 'neovm--test-mm-copy mat)))
        (setcar (nthcdr i m) (mapcar (lambda (x) (* k x)) (nth i m)))
        m)))

  ;; Add k times row j to row i
  (fset 'neovm--test-mm-add-row-multiple
    (lambda (mat i j k)
      (let ((m (funcall 'neovm--test-mm-copy mat)))
        (let ((row-i (nth i m))
              (row-j (nth j m)))
          (setcar (nthcdr i m)
                  (let ((new-row nil) (ri row-i) (rj row-j))
                    (while ri
                      (setq new-row (cons (+ (car ri) (* k (car rj))) new-row))
                      (setq ri (cdr ri) rj (cdr rj)))
                    (nreverse new-row))))
        m)))

  (unwind-protect
      (let ((m '((1 2 3) (4 5 6) (7 8 9))))
        (list
         ;; Swap rows 0 and 2
         (funcall 'neovm--test-mm-swap-rows m 0 2)
         ;; Double-swap returns original
         (equal m (funcall 'neovm--test-mm-swap-rows
                           (funcall 'neovm--test-mm-swap-rows m 0 2) 0 2))
         ;; Swap row with itself = no change
         (equal m (funcall 'neovm--test-mm-swap-rows m 1 1))
         ;; Scale row 1 by 3
         (funcall 'neovm--test-mm-scale-row m 1 3)
         ;; Scale by 1 = no change
         (equal m (funcall 'neovm--test-mm-scale-row m 0 1))
         ;; Scale by 0
         (funcall 'neovm--test-mm-scale-row m 2 0)
         ;; Add -4 times row 0 to row 1 (elimination step)
         (funcall 'neovm--test-mm-add-row-multiple m 1 0 -4)
         ;; Chain of operations: eliminate below pivot
         (let ((step1 (funcall 'neovm--test-mm-add-row-multiple m 1 0 -4)))
           (funcall 'neovm--test-mm-add-row-multiple step1 2 0 -7))))
    (fmakunbound 'neovm--test-mm-copy)
    (fmakunbound 'neovm--test-mm-swap-rows)
    (fmakunbound 'neovm--test-mm-scale-row)
    (fmakunbound 'neovm--test-mm-add-row-multiple)))"#;
    let expect = expect_test::expect![[
        r#""OK (((7 8 9) (4 5 6) (1 2 3)) t t ((1 2 3) (12 15 18) (7 8 9)) t ((1 2 3) (4 5 6) (0 0 0)) ((1 2 3) (0 -3 -6) (7 8 9)) ((1 2 3) (0 -3 -6) (0 -6 -12)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Row echelon form via Gaussian elimination
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_matrix_math_row_echelon_form() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--test-mm-ref-get
    (lambda (mat r c) (nth c (nth r mat))))

  (fset 'neovm--test-mm-ref-set
    (lambda (mat r c val) (setcar (nthcdr c (nth r mat)) val)))

  ;; Compute row echelon form using integer-only Gaussian elimination
  ;; (multiply rows instead of dividing to stay in integers)
  (fset 'neovm--test-mm-row-echelon
    (lambda (mat-orig)
      (let* ((mat (mapcar #'copy-sequence mat-orig))
             (nrows (length mat))
             (ncols (length (car mat)))
             (pivot-row 0)
             (pivot-col 0))
        (while (and (< pivot-row nrows) (< pivot-col ncols))
          ;; Find non-zero entry in current column at or below pivot-row
          (let ((found nil) (search-row pivot-row))
            (while (and (not found) (< search-row nrows))
              (if (/= (funcall 'neovm--test-mm-ref-get mat search-row pivot-col) 0)
                  (setq found search-row)
                (setq search-row (1+ search-row))))
            (if (not found)
                ;; No pivot in this column, move to next column
                (setq pivot-col (1+ pivot-col))
              ;; Swap found row with pivot-row if different
              (when (/= found pivot-row)
                (let ((tmp (nth pivot-row mat)))
                  (setcar (nthcdr pivot-row mat) (nth found mat))
                  (setcar (nthcdr found mat) tmp)))
              ;; Eliminate entries below pivot
              (let ((pivot-val (funcall 'neovm--test-mm-ref-get mat pivot-row pivot-col))
                    (target (1+ pivot-row)))
                (while (< target nrows)
                  (let ((target-val (funcall 'neovm--test-mm-ref-get mat target pivot-col)))
                    (when (/= target-val 0)
                      ;; target-row = pivot-val * target-row - target-val * pivot-row
                      (let ((col 0))
                        (while (< col ncols)
                          (funcall 'neovm--test-mm-ref-set mat target col
                                   (- (* pivot-val (funcall 'neovm--test-mm-ref-get mat target col))
                                      (* target-val (funcall 'neovm--test-mm-ref-get mat pivot-row col))))
                          (setq col (1+ col))))))
                  (setq target (1+ target))))
              (setq pivot-row (1+ pivot-row)
                    pivot-col (1+ pivot-col)))))
        mat)))

  (unwind-protect
      (list
       ;; Already in REF
       (funcall 'neovm--test-mm-row-echelon '((1 2 3) (0 4 5) (0 0 6)))
       ;; Simple 2x2
       (funcall 'neovm--test-mm-row-echelon '((2 4) (1 3)))
       ;; 3x3 with zero-out
       (funcall 'neovm--test-mm-row-echelon '((1 2 3) (4 5 6) (7 8 9)))
       ;; Requires row swap (first column starts with 0)
       (funcall 'neovm--test-mm-row-echelon '((0 1 2) (3 4 5) (6 7 8)))
       ;; 3x4 augmented matrix (for solving systems)
       (funcall 'neovm--test-mm-row-echelon '((1 1 1 6) (2 3 1 14) (1 1 3 12)))
       ;; Identity is already in REF
       (funcall 'neovm--test-mm-row-echelon '((1 0 0) (0 1 0) (0 0 1)))
       ;; Singular matrix: last row becomes all zeros
       (let ((ref (funcall 'neovm--test-mm-row-echelon '((1 2 3) (2 4 6) (3 5 7)))))
         (list ref
               ;; Last row should be all zeros (since row 2 = 2*row 1)
               (equal (nth 2 ref) (make-list (length (car ref)) 0)))))
    (fmakunbound 'neovm--test-mm-ref-get)
    (fmakunbound 'neovm--test-mm-ref-set)
    (fmakunbound 'neovm--test-mm-row-echelon)))"#;
    let expect = expect_test::expect![[
        r#""OK (((1 2 3) (0 4 5) (0 0 6)) ((2 4) (0 2)) ((1 2 3) (0 -3 -6) (0 0 0)) ((3 4 5) (0 1 2) (0 0 0)) ((1 1 1 6) (0 1 -1 2) (0 0 2 6)) ((1 0 0) (0 1 0) (0 0 1)) (((1 2 3) (0 -1 -2) (0 0 0)) t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
