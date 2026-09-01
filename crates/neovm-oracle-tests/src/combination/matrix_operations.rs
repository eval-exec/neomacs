//! Oracle parity tests for matrix operations implemented in pure Elisp.
//!
//! Covers: matrix creation (zeros, identity, from nested list), addition,
//! scalar multiplication, matrix multiplication (NxM * MxP), transpose,
//! determinant (2x2, 3x3), matrix inverse (2x2), and Gaussian elimination
//! for solving linear systems.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Matrix creation: zeros, identity, from nested list
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_matrix_creation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Matrix represented as list of rows (each row is a list)
  ;; zeros: create an NxM matrix of zeros
  (fset 'neovm--test-mat-zeros
    (lambda (rows cols)
      (let ((result nil) (r 0))
        (while (< r rows)
          (setq result (cons (make-list cols 0) result))
          (setq r (1+ r)))
        (nreverse result))))

  ;; identity: create an NxN identity matrix
  (fset 'neovm--test-mat-identity
    (lambda (n)
      (let ((result nil) (r 0))
        (while (< r n)
          (let ((row (make-list n 0))
                (c 0))
            (setcar (nthcdr r row) 1)
            (setq result (cons row result)))
          (setq r (1+ r)))
        (nreverse result))))

  ;; mat-ref: get element at (row, col)
  (fset 'neovm--test-mat-ref
    (lambda (mat r c)
      (nth c (nth r mat))))

  (unwind-protect
      (list
        ;; 2x3 zeros
        (funcall 'neovm--test-mat-zeros 2 3)
        ;; 3x3 identity
        (funcall 'neovm--test-mat-identity 3)
        ;; 4x4 identity
        (funcall 'neovm--test-mat-identity 4)
        ;; 1x1 identity
        (funcall 'neovm--test-mat-identity 1)
        ;; Element access on identity
        (let ((I3 (funcall 'neovm--test-mat-identity 3)))
          (list (funcall 'neovm--test-mat-ref I3 0 0)
                (funcall 'neovm--test-mat-ref I3 0 1)
                (funcall 'neovm--test-mat-ref I3 1 1)
                (funcall 'neovm--test-mat-ref I3 2 2))))
    (fmakunbound 'neovm--test-mat-zeros)
    (fmakunbound 'neovm--test-mat-identity)
    (fmakunbound 'neovm--test-mat-ref)))"#;
    let expect = expect_test::expect![[
        r#""OK (((0 0 0) (0 0 0)) ((1 0 0) (0 1 0) (0 0 1)) ((1 0 0 0) (0 1 0 0) (0 0 1 0) (0 0 0 1)) ((1)) (1 0 1 1))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Matrix addition and scalar multiplication
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_matrix_add_and_scalar_mult() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Element-wise addition
  (fset 'neovm--test-mat-add
    (lambda (a b)
      (let ((result nil)
            (ra a) (rb b))
        (while ra
          (let ((row-a (car ra))
                (row-b (car rb))
                (new-row nil))
            (while row-a
              (setq new-row (cons (+ (car row-a) (car row-b)) new-row))
              (setq row-a (cdr row-a) row-b (cdr row-b)))
            (setq result (cons (nreverse new-row) result)))
          (setq ra (cdr ra) rb (cdr rb)))
        (nreverse result))))

  ;; Scalar multiplication
  (fset 'neovm--test-mat-scale
    (lambda (s mat)
      (mapcar (lambda (row)
                (mapcar (lambda (x) (* s x)) row))
              mat)))

  (unwind-protect
      (let ((a '((1 2 3) (4 5 6)))
            (b '((7 8 9) (10 11 12))))
        (list
          ;; Addition
          (funcall 'neovm--test-mat-add a b)
          ;; Scalar mult
          (funcall 'neovm--test-mat-scale 3 a)
          ;; Scale by 0
          (funcall 'neovm--test-mat-scale 0 b)
          ;; Scale by -1
          (funcall 'neovm--test-mat-scale -1 a)
          ;; A + (-1)*B = A - B
          (funcall 'neovm--test-mat-add a
                   (funcall 'neovm--test-mat-scale -1 b))
          ;; 2A + 3B
          (funcall 'neovm--test-mat-add
                   (funcall 'neovm--test-mat-scale 2 a)
                   (funcall 'neovm--test-mat-scale 3 b))))
    (fmakunbound 'neovm--test-mat-add)
    (fmakunbound 'neovm--test-mat-scale)))"#;
    let expect = expect_test::expect![[
        r#""OK (((8 10 12) (14 16 18)) ((3 6 9) (12 15 18)) ((0 0 0) (0 0 0)) ((-1 -2 -3) (-4 -5 -6)) ((-6 -6 -6) (-6 -6 -6)) ((23 28 33) (38 43 48)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Matrix multiplication (NxM * MxP)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_matrix_multiplication() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Transpose helper (needed for column access in multiplication)
  (fset 'neovm--test-mat-transpose
    (lambda (mat)
      (if (null mat) nil
        (let ((ncols (length (car mat)))
              (result nil)
              (c 0))
          (while (< c ncols)
            (let ((col nil)
                  (rows mat))
              (while rows
                (setq col (cons (nth c (car rows)) col))
                (setq rows (cdr rows)))
              (setq result (cons (nreverse col) result)))
            (setq c (1+ c)))
          (nreverse result)))))

  ;; Dot product of two lists
  (fset 'neovm--test-dot
    (lambda (a b)
      (let ((sum 0))
        (while a
          (setq sum (+ sum (* (car a) (car b))))
          (setq a (cdr a) b (cdr b)))
        sum)))

  ;; Matrix multiplication: A(n,m) * B(m,p) -> C(n,p)
  (fset 'neovm--test-mat-mult
    (lambda (a b)
      (let ((bt (funcall 'neovm--test-mat-transpose b)))
        (mapcar (lambda (row-a)
                  (mapcar (lambda (col-b)
                            (funcall 'neovm--test-dot row-a col-b))
                          bt))
                a))))

  (unwind-protect
      (let ((a '((1 2) (3 4) (5 6)))
            (b '((7 8 9) (10 11 12)))
            (i2 '((1 0) (0 1)))
            (sq '((1 2) (3 4))))
        (list
          ;; 3x2 * 2x3 = 3x3
          (funcall 'neovm--test-mat-mult a b)
          ;; 2x2 * identity = same matrix
          (funcall 'neovm--test-mat-mult sq i2)
          ;; identity * 2x2 = same matrix
          (funcall 'neovm--test-mat-mult i2 sq)
          ;; 2x2 * 2x2
          (funcall 'neovm--test-mat-mult sq '((5 6) (7 8)))
          ;; 1x3 * 3x1 = 1x1
          (funcall 'neovm--test-mat-mult '((1 2 3)) '((4) (5) (6)))
          ;; Verify (AB)^T = B^T A^T
          (let ((ab (funcall 'neovm--test-mat-mult sq '((5 6) (7 8))))
                (bt-at (funcall 'neovm--test-mat-mult
                                (funcall 'neovm--test-mat-transpose '((5 6) (7 8)))
                                (funcall 'neovm--test-mat-transpose sq))))
            (equal (funcall 'neovm--test-mat-transpose ab) bt-at))))
    (fmakunbound 'neovm--test-mat-transpose)
    (fmakunbound 'neovm--test-dot)
    (fmakunbound 'neovm--test-mat-mult)))"#;
    let expect = expect_test::expect![[
        r#""OK (((27 30 33) (61 68 75) (95 106 117)) ((1 2) (3 4)) ((1 2) (3 4)) ((19 22) (43 50)) ((32)) t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Matrix transpose (standalone tests)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_matrix_transpose() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--test-mat-transpose
    (lambda (mat)
      (if (null mat) nil
        (let ((ncols (length (car mat)))
              (result nil)
              (c 0))
          (while (< c ncols)
            (let ((col nil)
                  (rows mat))
              (while rows
                (setq col (cons (nth c (car rows)) col))
                (setq rows (cdr rows)))
              (setq result (cons (nreverse col) result)))
            (setq c (1+ c)))
          (nreverse result)))))

  (unwind-protect
      (list
        ;; 2x3 -> 3x2
        (funcall 'neovm--test-mat-transpose '((1 2 3) (4 5 6)))
        ;; 3x2 -> 2x3
        (funcall 'neovm--test-mat-transpose '((1 4) (2 5) (3 6)))
        ;; 1x4 -> 4x1
        (funcall 'neovm--test-mat-transpose '((1 2 3 4)))
        ;; Symmetric matrix: transpose = self
        (let ((sym '((1 2 3) (2 4 5) (3 5 6))))
          (equal sym (funcall 'neovm--test-mat-transpose sym)))
        ;; Double transpose = original
        (let ((m '((1 2 3) (4 5 6))))
          (equal m (funcall 'neovm--test-mat-transpose
                            (funcall 'neovm--test-mat-transpose m))))
        ;; Square matrix
        (funcall 'neovm--test-mat-transpose '((1 2) (3 4))))
    (fmakunbound 'neovm--test-mat-transpose)))"#;
    let expect = expect_test::expect![[
        r#""OK (((1 4) (2 5) (3 6)) ((1 2 3) (4 5 6)) ((1) (2) (3) (4)) t t ((1 3) (2 4)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Determinant calculation (2x2 and 3x3)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_matrix_determinant() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; 2x2 determinant: ad - bc
  (fset 'neovm--test-det2x2
    (lambda (mat)
      (let ((a (nth 0 (nth 0 mat)))
            (b (nth 1 (nth 0 mat)))
            (c (nth 0 (nth 1 mat)))
            (d (nth 1 (nth 1 mat))))
        (- (* a d) (* b c)))))

  ;; 3x3 determinant via cofactor expansion along first row
  (fset 'neovm--test-det3x3
    (lambda (mat)
      (let ((a (nth 0 (nth 0 mat)))
            (b (nth 1 (nth 0 mat)))
            (c (nth 2 (nth 0 mat)))
            ;; Row 1
            (d (nth 0 (nth 1 mat)))
            (e (nth 1 (nth 1 mat)))
            (f (nth 2 (nth 1 mat)))
            ;; Row 2
            (g (nth 0 (nth 2 mat)))
            (h (nth 1 (nth 2 mat)))
            (i (nth 2 (nth 2 mat))))
        (+ (* a (- (* e i) (* f h)))
           (* (- b) (- (* d i) (* f g)))
           (* c (- (* d h) (* e g)))))))

  (unwind-protect
      (list
        ;; 2x2 determinants
        (funcall 'neovm--test-det2x2 '((1 2) (3 4)))
        (funcall 'neovm--test-det2x2 '((5 0) (0 5)))
        (funcall 'neovm--test-det2x2 '((1 0) (0 1)))
        ;; Singular matrix (det = 0)
        (funcall 'neovm--test-det2x2 '((2 4) (1 2)))
        ;; 3x3 determinants
        (funcall 'neovm--test-det3x3 '((1 0 0) (0 1 0) (0 0 1)))
        (funcall 'neovm--test-det3x3 '((1 2 3) (4 5 6) (7 8 9)))
        (funcall 'neovm--test-det3x3 '((2 1 1) (1 3 2) (1 0 0)))
        (funcall 'neovm--test-det3x3 '((6 1 1) (4 -2 5) (2 8 7)))
        ;; Negative determinant
        (funcall 'neovm--test-det2x2 '((0 1) (1 0))))
    (fmakunbound 'neovm--test-det2x2)
    (fmakunbound 'neovm--test-det3x3)))"#;
    let expect = expect_test::expect![[r#""OK (-2 25 1 0 1 0 -1 -306 -1)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Matrix inverse (2x2)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_matrix_inverse_2x2() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // For 2x2 matrix [[a,b],[c,d]], inverse = (1/det) * [[d,-b],[-c,a]]
    // We use integer arithmetic and return (det, adjugate) to avoid float issues.
    let form = r#"(progn
  ;; Returns (det . adjugate-matrix) — caller divides each element by det
  ;; to get the true inverse. We verify A * adj(A) = det(A) * I.
  (fset 'neovm--test-mat-adjugate2x2
    (lambda (mat)
      (let ((a (nth 0 (nth 0 mat)))
            (b (nth 1 (nth 0 mat)))
            (c (nth 0 (nth 1 mat)))
            (d (nth 1 (nth 1 mat))))
        (let ((det (- (* a d) (* b c))))
          (cons det (list (list d (- b))
                          (list (- c) a)))))))

  ;; Dot product
  (fset 'neovm--test-dot2
    (lambda (a b)
      (let ((sum 0))
        (while a
          (setq sum (+ sum (* (car a) (car b))))
          (setq a (cdr a) b (cdr b)))
        sum)))

  ;; Matrix multiply for verification
  (fset 'neovm--test-mmul2
    (lambda (a b)
      (let ((bt (list (list (nth 0 (nth 0 b)) (nth 0 (nth 1 b)))
                      (list (nth 1 (nth 0 b)) (nth 1 (nth 1 b))))))
        (mapcar (lambda (row-a)
                  (mapcar (lambda (col-b)
                            (funcall 'neovm--test-dot2 row-a col-b))
                          bt))
                a))))

  (unwind-protect
      (let ((tests '(((1 2) (3 4))
                      ((5 0) (0 5))
                      ((2 1) (7 4))
                      ((3 -1) (5 2)))))
        (mapcar
         (lambda (mat)
           (let* ((result (funcall 'neovm--test-mat-adjugate2x2 mat))
                  (det (car result))
                  (adj (cdr result))
                  ;; A * adj(A) should = det * I
                  (product (funcall 'neovm--test-mmul2 mat adj)))
             (list det adj product
                   ;; Verify: product should be ((det 0) (0 det))
                   (equal product (list (list det 0) (list 0 det))))))
         tests))
    (fmakunbound 'neovm--test-mat-adjugate2x2)
    (fmakunbound 'neovm--test-dot2)
    (fmakunbound 'neovm--test-mmul2)))"#;
    let expect = expect_test::expect![[
        r#""OK ((-2 ((4 -2) (-3 1)) ((-2 0) (0 -2)) t) (25 ((5 0) (0 5)) ((25 0) (0 25)) t) (1 ((4 -1) (-7 2)) ((1 0) (0 1)) t) (11 ((2 1) (-5 3)) ((11 0) (0 11)) t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Gaussian elimination for solving linear systems
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_matrix_gaussian_elimination() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Solve Ax = b using Gaussian elimination with back substitution.
    // We work with integer arithmetic scaled by a factor to avoid floats.
    // The system is: given augmented matrix [A|b], produce solution x.
    let form = r#"(progn
  ;; Copy a matrix (deep copy of lists)
  (fset 'neovm--test-mat-copy
    (lambda (mat)
      (mapcar (lambda (row) (copy-sequence row)) mat)))

  ;; Set element in matrix (destructive)
  (fset 'neovm--test-mat-set
    (lambda (mat r c val)
      (setcar (nthcdr c (nth r mat)) val)))

  ;; Get element
  (fset 'neovm--test-mat-get
    (lambda (mat r c)
      (nth c (nth r mat))))

  ;; Solve system using integer Gaussian elimination:
  ;; Returns list of (numerator . denominator) pairs for each variable.
  ;; Input: augmented matrix [[a11..a1n|b1]..[an1..ann|bn]]
  (fset 'neovm--test-gauss-solve
    (lambda (aug-orig)
      (let* ((aug (funcall 'neovm--test-mat-copy aug-orig))
             (n (length aug))
             (ncols (length (car aug))))
        ;; Forward elimination
        (let ((pivot-row 0))
          (while (< pivot-row n)
            (let ((pivot-val (funcall 'neovm--test-mat-get aug pivot-row pivot-row)))
              ;; Eliminate below
              (let ((target-row (1+ pivot-row)))
                (while (< target-row n)
                  (let ((factor (funcall 'neovm--test-mat-get aug target-row pivot-row)))
                    (when (/= factor 0)
                      (let ((col 0))
                        (while (< col ncols)
                          (funcall 'neovm--test-mat-set aug target-row col
                                   (- (* pivot-val
                                         (funcall 'neovm--test-mat-get aug target-row col))
                                      (* factor
                                         (funcall 'neovm--test-mat-get aug pivot-row col))))
                          (setq col (1+ col))))))
                  (setq target-row (1+ target-row)))))
            (setq pivot-row (1+ pivot-row))))
        ;; Back substitution: extract solution as (num . denom) pairs
        (let ((solution (make-list n nil))
              (row (1- n)))
          (while (>= row 0)
            (let ((rhs (funcall 'neovm--test-mat-get aug row (1- ncols)))
                  (col (1+ row)))
              ;; Subtract known variables
              (while (< col n)
                (let ((s (nth col solution)))
                  (when s
                    ;; rhs = rhs * denom(s) - coeff * num(s)
                    (let ((coeff (funcall 'neovm--test-mat-get aug row col)))
                      (setq rhs (- (* rhs (cdr s))
                                   (* coeff (car s)))))))
                (setq col (1+ col)))
              ;; Compute denominator product for all used solutions
              (let ((denom (funcall 'neovm--test-mat-get aug row row)))
                ;; Multiply denom by denominators of solutions used
                (let ((col2 (1+ row)))
                  (while (< col2 n)
                    (let ((s (nth col2 solution)))
                      (when s
                        (setq denom (* denom (cdr s)))))
                    (setq col2 (1+ col2))))
                (setcar (nthcdr row solution) (cons rhs denom))))
            (setq row (1- row)))
          solution))))

  (unwind-protect
      (list
        ;; System 1: x + 2y = 5, 3x + 4y = 11
        ;; Solution: x=1, y=2
        (let ((sol (funcall 'neovm--test-gauss-solve
                            '((1 2 5) (3 4 11)))))
          (mapcar (lambda (s) (/ (car s) (cdr s))) sol))
        ;; System 2: 2x + y = 5, x - y = 1
        ;; Solution: x=2, y=1
        (let ((sol (funcall 'neovm--test-gauss-solve
                            '((2 1 5) (1 -1 1)))))
          (mapcar (lambda (s) (/ (car s) (cdr s))) sol))
        ;; System 3: 3x3 system
        ;; x + y + z = 6, 2x + 3y + z = 14, x + y + 3z = 12
        ;; Solution: x=1, y=3, z=2
        (let ((sol (funcall 'neovm--test-gauss-solve
                            '((1 1 1 6) (2 3 1 14) (1 1 3 12)))))
          (mapcar (lambda (s) (/ (car s) (cdr s))) sol)))
    (fmakunbound 'neovm--test-mat-copy)
    (fmakunbound 'neovm--test-mat-set)
    (fmakunbound 'neovm--test-mat-get)
    (fmakunbound 'neovm--test-gauss-solve)))"#;
    let expect = expect_test::expect![[r#""OK ((1 2) (2 1) (0 5 3))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Matrix trace and Frobenius norm squared
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_matrix_trace_and_frobenius() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Trace: sum of diagonal elements
  (fset 'neovm--test-mat-trace
    (lambda (mat)
      (let ((sum 0) (i 0) (n (length mat)))
        (while (< i n)
          (setq sum (+ sum (nth i (nth i mat))))
          (setq i (1+ i)))
        sum)))

  ;; Frobenius norm squared: sum of squares of all elements
  (fset 'neovm--test-mat-frobenius-sq
    (lambda (mat)
      (let ((sum 0))
        (mapc (lambda (row)
                (mapc (lambda (x) (setq sum (+ sum (* x x)))) row))
              mat)
        sum)))

  (unwind-protect
      (list
        ;; Trace of identity
        (funcall 'neovm--test-mat-trace '((1 0 0) (0 1 0) (0 0 1)))
        ;; Trace of 2x2
        (funcall 'neovm--test-mat-trace '((5 3) (2 7)))
        ;; Trace of 3x3
        (funcall 'neovm--test-mat-trace '((1 2 3) (4 5 6) (7 8 9)))
        ;; Frobenius norm squared of identity
        (funcall 'neovm--test-mat-frobenius-sq '((1 0 0) (0 1 0) (0 0 1)))
        ;; Frobenius norm squared of general matrix
        (funcall 'neovm--test-mat-frobenius-sq '((1 2) (3 4)))
        ;; Property: trace(A) = trace(A^T) for any square matrix
        (let ((m '((1 2 3) (4 5 6) (7 8 9))))
          ;; Manual transpose for verification
          (let ((mt (list (list 1 4 7) (list 2 5 8) (list 3 6 9))))
            (= (funcall 'neovm--test-mat-trace m)
               (funcall 'neovm--test-mat-trace mt)))))
    (fmakunbound 'neovm--test-mat-trace)
    (fmakunbound 'neovm--test-mat-frobenius-sq)))"#;
    let expect = expect_test::expect![[r#""OK (3 12 15 3 30 t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
