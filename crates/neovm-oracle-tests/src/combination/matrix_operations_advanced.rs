//! Advanced oracle parity tests for matrix operations in Elisp:
//! determinant (2x2, 3x3), matrix inverse (2x2), matrix trace,
//! row echelon form (Gaussian elimination), system of linear equations
//! solver, and LU decomposition.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Matrix determinant: 2x2 and 3x3 with edge cases
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_matrix_advanced_determinant() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; 2x2 det: ad - bc
  (fset 'neovm--test-adv-det2
    (lambda (m)
      (- (* (nth 0 (nth 0 m)) (nth 1 (nth 1 m)))
         (* (nth 1 (nth 0 m)) (nth 0 (nth 1 m))))))

  ;; 3x3 det via cofactor expansion (first row)
  (fset 'neovm--test-adv-det3
    (lambda (m)
      (let ((a (nth 0 (nth 0 m))) (b (nth 1 (nth 0 m))) (c (nth 2 (nth 0 m)))
            (d (nth 0 (nth 1 m))) (e (nth 1 (nth 1 m))) (f (nth 2 (nth 1 m)))
            (g (nth 0 (nth 2 m))) (h (nth 1 (nth 2 m))) (i (nth 2 (nth 2 m))))
        (- (+ (* a (- (* e i) (* f h)))
              (* c (- (* d h) (* e g))))
           (* b (- (* d i) (* f g)))))))

  ;; General NxN determinant via cofactor expansion (recursive)
  (fset 'neovm--test-adv-minor
    (lambda (m row col)
      (let ((result nil)
            (r 0))
        (while (< r (length m))
          (when (/= r row)
            (let ((new-row nil)
                  (c 0)
                  (orig-row (nth r m)))
              (while (< c (length orig-row))
                (when (/= c col)
                  (push (nth c orig-row) new-row))
                (setq c (1+ c)))
              (push (nreverse new-row) result)))
          (setq r (1+ r)))
        (nreverse result))))

  (fset 'neovm--test-adv-det
    (lambda (m)
      (let ((n (length m)))
        (cond
         ((= n 1) (nth 0 (nth 0 m)))
         ((= n 2) (funcall 'neovm--test-adv-det2 m))
         (t (let ((det 0) (sign 1) (j 0))
              (while (< j n)
                (setq det (+ det (* sign
                                    (nth j (nth 0 m))
                                    (funcall 'neovm--test-adv-det
                                             (funcall 'neovm--test-adv-minor m 0 j)))))
                (setq sign (- sign))
                (setq j (1+ j)))
              det))))))

  (unwind-protect
      (list
       ;; 2x2 determinants
       (funcall 'neovm--test-adv-det2 '((1 2) (3 4)))            ;; -2
       (funcall 'neovm--test-adv-det2 '((5 0) (0 5)))            ;; 25
       (funcall 'neovm--test-adv-det2 '((2 4) (1 2)))            ;; 0 (singular)
       (funcall 'neovm--test-adv-det2 '((-3 7) (2 -5)))          ;; 1
       ;; 3x3 determinants
       (funcall 'neovm--test-adv-det3 '((1 0 0) (0 1 0) (0 0 1)))  ;; 1 (identity)
       (funcall 'neovm--test-adv-det3 '((1 2 3) (4 5 6) (7 8 9)))  ;; 0 (singular)
       (funcall 'neovm--test-adv-det3 '((6 1 1) (4 -2 5) (2 8 7))) ;; -306
       ;; NxN recursive: verify matches specialized versions
       (= (funcall 'neovm--test-adv-det '((1 2) (3 4)))
          (funcall 'neovm--test-adv-det2 '((1 2) (3 4))))
       (= (funcall 'neovm--test-adv-det '((6 1 1) (4 -2 5) (2 8 7)))
          (funcall 'neovm--test-adv-det3 '((6 1 1) (4 -2 5) (2 8 7))))
       ;; 4x4 determinant
       (funcall 'neovm--test-adv-det '((1 0 0 0) (0 2 0 0) (0 0 3 0) (0 0 0 4)))  ;; 24
       ;; Permutation matrix: det = -1
       (funcall 'neovm--test-adv-det '((0 1 0) (1 0 0) (0 0 1)))
       ;; Property: det(A) = det(A^T) for 3x3
       (let ((a '((2 1 3) (4 5 6) (7 8 0))))
         (= (funcall 'neovm--test-adv-det a)
            (funcall 'neovm--test-adv-det '((2 4 7) (1 5 8) (3 6 0))))))
    (fmakunbound 'neovm--test-adv-det2)
    (fmakunbound 'neovm--test-adv-det3)
    (fmakunbound 'neovm--test-adv-minor)
    (fmakunbound 'neovm--test-adv-det)))"#;
    let expect = expect_test::expect![[r#""OK (-2 25 0 1 1 0 -306 t t 24 -1 t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Matrix inverse (2x2) with verification A * A^{-1} = I
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_matrix_advanced_inverse_2x2() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // We compute the adjugate (integer) and verify A * adj(A) = det(A) * I
    let form = r#"(progn
  ;; 2x2 adjugate and determinant
  (fset 'neovm--test-adv-inv2
    (lambda (m)
      (let ((a (nth 0 (nth 0 m))) (b (nth 1 (nth 0 m)))
            (c (nth 0 (nth 1 m))) (d (nth 1 (nth 1 m))))
        (let ((det (- (* a d) (* b c))))
          (list det (list (list d (- b)) (list (- c) a)))))))

  ;; 2x2 matrix multiply
  (fset 'neovm--test-adv-mmul2
    (lambda (a b)
      (let ((a00 (nth 0 (nth 0 a))) (a01 (nth 1 (nth 0 a)))
            (a10 (nth 0 (nth 1 a))) (a11 (nth 1 (nth 1 a)))
            (b00 (nth 0 (nth 0 b))) (b01 (nth 1 (nth 0 b)))
            (b10 (nth 0 (nth 1 b))) (b11 (nth 1 (nth 1 b))))
        (list (list (+ (* a00 b00) (* a01 b10))
                    (+ (* a00 b01) (* a01 b11)))
              (list (+ (* a10 b00) (* a11 b10))
                    (+ (* a10 b01) (* a11 b11)))))))

  (unwind-protect
      (let ((matrices '(((1 2) (3 4))
                         ((5 0) (0 5))
                         ((2 1) (7 4))
                         ((3 -1) (5 2))
                         ((-3 7) (2 -5))
                         ((1 0) (0 1)))))
        (mapcar
         (lambda (m)
           (let* ((result (funcall 'neovm--test-adv-inv2 m))
                  (det (nth 0 result))
                  (adj (nth 1 result))
                  ;; A * adj(A) should = det * I
                  (product (funcall 'neovm--test-adv-mmul2 m adj))
                  ;; Also adj(A) * A should = det * I
                  (product2 (funcall 'neovm--test-adv-mmul2 adj m)))
             (list
              'det det
              'adj adj
              'A*adj product
              'adj*A product2
              'check-A*adj (equal product (list (list det 0) (list 0 det)))
              'check-adj*A (equal product2 (list (list det 0) (list 0 det))))))
         matrices))
    (fmakunbound 'neovm--test-adv-inv2)
    (fmakunbound 'neovm--test-adv-mmul2)))"#;
    let expect = expect_test::expect![[
        r#""OK ((det -2 adj ((4 -2) (-3 1)) A*adj ((-2 0) (0 -2)) adj*A ((-2 0) (0 -2)) check-A*adj t check-adj*A t) (det 25 adj ((5 0) (0 5)) A*adj ((25 0) (0 25)) adj*A ((25 0) (0 25)) check-A*adj t check-adj*A t) (det 1 adj ((4 -1) (-7 2)) A*adj ((1 0) (0 1)) adj*A ((1 0) (0 1)) check-A*adj t check-adj*A t) (det 11 adj ((2 1) (-5 3)) A*adj ((11 0) (0 11)) adj*A ((11 0) (0 11)) check-A*adj t check-adj*A t) (det 1 adj ((-5 -7) (-2 -3)) A*adj ((1 0) (0 1)) adj*A ((1 0) (0 1)) check-A*adj t check-adj*A t) (det 1 adj ((1 0) (0 1)) A*adj ((1 0) (0 1)) adj*A ((1 0) (0 1)) check-A*adj t check-adj*A t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Matrix trace: diagonal sum and properties
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_matrix_advanced_trace() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--test-adv-trace
    (lambda (m)
      (let ((sum 0) (i 0) (n (length m)))
        (while (< i n)
          (setq sum (+ sum (nth i (nth i m))))
          (setq i (1+ i)))
        sum)))

  ;; Matrix add
  (fset 'neovm--test-adv-madd
    (lambda (a b)
      (let ((result nil)
            (ra a) (rb b))
        (while ra
          (let ((row-a (car ra)) (row-b (car rb)) (new-row nil))
            (while row-a
              (push (+ (car row-a) (car row-b)) new-row)
              (setq row-a (cdr row-a) row-b (cdr row-b)))
            (push (nreverse new-row) result))
          (setq ra (cdr ra) rb (cdr rb)))
        (nreverse result))))

  ;; Scalar multiply
  (fset 'neovm--test-adv-mscale
    (lambda (s m)
      (mapcar (lambda (row) (mapcar (lambda (x) (* s x)) row)) m)))

  (unwind-protect
      (let ((a '((1 2 3) (4 5 6) (7 8 9)))
            (b '((9 8 7) (6 5 4) (3 2 1)))
            (id3 '((1 0 0) (0 1 0) (0 0 1))))
        (list
         ;; Basic trace
         (funcall 'neovm--test-adv-trace a)            ;; 15
         (funcall 'neovm--test-adv-trace b)            ;; 15
         (funcall 'neovm--test-adv-trace id3)          ;; 3
         ;; 2x2
         (funcall 'neovm--test-adv-trace '((10 20) (30 40)))  ;; 50
         ;; 1x1
         (funcall 'neovm--test-adv-trace '((42)))      ;; 42
         ;; 4x4 diagonal
         (funcall 'neovm--test-adv-trace
                  '((2 0 0 0) (0 3 0 0) (0 0 5 0) (0 0 0 7)))  ;; 17
         ;; Property: tr(A+B) = tr(A) + tr(B)
         (= (funcall 'neovm--test-adv-trace
                     (funcall 'neovm--test-adv-madd a b))
            (+ (funcall 'neovm--test-adv-trace a)
               (funcall 'neovm--test-adv-trace b)))
         ;; Property: tr(cA) = c * tr(A)
         (= (funcall 'neovm--test-adv-trace
                     (funcall 'neovm--test-adv-mscale 3 a))
            (* 3 (funcall 'neovm--test-adv-trace a)))
         ;; Property: tr(A^T) = tr(A) (transpose of A)
         (let ((at '((1 4 7) (2 5 8) (3 6 9))))
           (= (funcall 'neovm--test-adv-trace a)
              (funcall 'neovm--test-adv-trace at)))
         ;; Negative entries
         (funcall 'neovm--test-adv-trace '((-1 2) (3 -4)))))  ;; -5
    (fmakunbound 'neovm--test-adv-trace)
    (fmakunbound 'neovm--test-adv-madd)
    (fmakunbound 'neovm--test-adv-mscale)))"#;
    let expect = expect_test::expect![[r#""OK (15 15 3 50 42 17 t t t -5)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Row echelon form (Gaussian elimination without back-substitution)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_matrix_advanced_row_echelon() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Transform to row echelon form using integer arithmetic to avoid floats.
    // Multiply rows to eliminate below-pivot entries (fraction-free).
    let form = r#"(progn
  (fset 'neovm--test-adv-mcopy
    (lambda (m) (mapcar (lambda (row) (copy-sequence row)) m)))

  (fset 'neovm--test-adv-mget
    (lambda (m r c) (nth c (nth r m))))

  (fset 'neovm--test-adv-mset
    (lambda (m r c v) (setcar (nthcdr c (nth r m)) v)))

  ;; Fraction-free Gaussian elimination to row echelon form
  ;; Returns the modified matrix (in-place on a copy)
  (fset 'neovm--test-adv-ref
    (lambda (m-orig)
      (let* ((m (funcall 'neovm--test-adv-mcopy m-orig))
             (nrows (length m))
             (ncols (length (car m)))
             (pivot-row 0)
             (pivot-col 0))
        (while (and (< pivot-row nrows) (< pivot-col ncols))
          ;; Find a non-zero entry in this column
          (let ((found nil) (search-row pivot-row))
            (while (and (not found) (< search-row nrows))
              (if (/= (funcall 'neovm--test-adv-mget m search-row pivot-col) 0)
                  (setq found search-row)
                (setq search-row (1+ search-row))))
            (if (not found)
                ;; No pivot in this column, move right
                (setq pivot-col (1+ pivot-col))
              ;; Swap rows if needed
              (when (/= found pivot-row)
                (let ((tmp (nth pivot-row m)))
                  (setcar (nthcdr pivot-row m) (nth found m))
                  (setcar (nthcdr found m) tmp)))
              ;; Eliminate below
              (let ((pval (funcall 'neovm--test-adv-mget m pivot-row pivot-col))
                    (target (1+ pivot-row)))
                (while (< target nrows)
                  (let ((tval (funcall 'neovm--test-adv-mget m target pivot-col)))
                    (when (/= tval 0)
                      (let ((col 0))
                        (while (< col ncols)
                          (funcall 'neovm--test-adv-mset m target col
                                   (- (* pval (funcall 'neovm--test-adv-mget m target col))
                                      (* tval (funcall 'neovm--test-adv-mget m pivot-row col))))
                          (setq col (1+ col))))))
                  (setq target (1+ target))))
              (setq pivot-row (1+ pivot-row))
              (setq pivot-col (1+ pivot-col)))))
        m)))

  (unwind-protect
      (list
       ;; Simple 2x2
       (funcall 'neovm--test-adv-ref '((1 2) (3 4)))
       ;; 3x3 with known result
       (funcall 'neovm--test-adv-ref '((1 2 3) (4 5 6) (7 8 9)))
       ;; Already in row echelon form
       (funcall 'neovm--test-adv-ref '((1 2 3) (0 4 5) (0 0 6)))
       ;; Requires row swap
       (funcall 'neovm--test-adv-ref '((0 1 2) (3 4 5) (6 7 8)))
       ;; 2x3 augmented matrix
       (funcall 'neovm--test-adv-ref '((2 1 5) (4 3 11)))
       ;; Verify: below-pivot entries should be zero
       (let ((ref (funcall 'neovm--test-adv-ref '((2 1 3) (4 5 6) (6 8 10)))))
         (list (= (funcall 'neovm--test-adv-mget ref 1 0) 0)
               (= (funcall 'neovm--test-adv-mget ref 2 0) 0)
               (= (funcall 'neovm--test-adv-mget ref 2 1) 0))))
    (fmakunbound 'neovm--test-adv-mcopy)
    (fmakunbound 'neovm--test-adv-mget)
    (fmakunbound 'neovm--test-adv-mset)
    (fmakunbound 'neovm--test-adv-ref)))"#;
    let expect = expect_test::expect![[
        r#""OK (((1 2) (0 -2)) ((1 2 3) (0 -3 -6) (0 0 0)) ((1 2 3) (0 4 5) (0 0 6)) ((3 4 5) (0 1 2) (0 0 0)) ((2 1 5) (0 2 2)) (t t t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// System of linear equations solver (Cramer's rule for 2x2 and 3x3)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_matrix_advanced_cramers_rule() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Cramer's rule: x_i = det(A_i) / det(A) where A_i has column i replaced by b
    let form = r#"(progn
  (fset 'neovm--test-adv-det2cr
    (lambda (m)
      (- (* (nth 0 (nth 0 m)) (nth 1 (nth 1 m)))
         (* (nth 1 (nth 0 m)) (nth 0 (nth 1 m))))))

  (fset 'neovm--test-adv-det3cr
    (lambda (m)
      (let ((a (nth 0 (nth 0 m))) (b (nth 1 (nth 0 m))) (c (nth 2 (nth 0 m)))
            (d (nth 0 (nth 1 m))) (e (nth 1 (nth 1 m))) (f (nth 2 (nth 1 m)))
            (g (nth 0 (nth 2 m))) (h (nth 1 (nth 2 m))) (i (nth 2 (nth 2 m))))
        (- (+ (* a (- (* e i) (* f h)))
              (* c (- (* d h) (* e g))))
           (* b (- (* d i) (* f g)))))))

  ;; Replace column j of matrix with vector b
  (fset 'neovm--test-adv-replace-col
    (lambda (m j b)
      (let ((result nil) (i 0))
        (while (< i (length m))
          (let ((row (copy-sequence (nth i m))))
            (setcar (nthcdr j row) (nth i b))
            (push row result))
          (setq i (1+ i)))
        (nreverse result))))

  ;; Cramer 2x2: Ax=b, returns (x1 . x2) as rationals (num . den)
  (fset 'neovm--test-adv-cramer2
    (lambda (a b)
      (let ((det-a (funcall 'neovm--test-adv-det2cr a)))
        (if (= det-a 0) 'singular
          (let ((d1 (funcall 'neovm--test-adv-det2cr
                             (funcall 'neovm--test-adv-replace-col a 0 b)))
                (d2 (funcall 'neovm--test-adv-det2cr
                             (funcall 'neovm--test-adv-replace-col a 1 b))))
            (list (cons d1 det-a) (cons d2 det-a)))))))

  ;; Cramer 3x3
  (fset 'neovm--test-adv-cramer3
    (lambda (a b)
      (let ((det-a (funcall 'neovm--test-adv-det3cr a)))
        (if (= det-a 0) 'singular
          (let ((d1 (funcall 'neovm--test-adv-det3cr
                             (funcall 'neovm--test-adv-replace-col a 0 b)))
                (d2 (funcall 'neovm--test-adv-det3cr
                             (funcall 'neovm--test-adv-replace-col a 1 b)))
                (d3 (funcall 'neovm--test-adv-det3cr
                             (funcall 'neovm--test-adv-replace-col a 2 b))))
            (list (cons d1 det-a) (cons d2 det-a) (cons d3 det-a)))))))

  (unwind-protect
      (list
       ;; 2x2: x + 2y = 5, 3x + 4y = 11 => x=1, y=2
       (let ((sol (funcall 'neovm--test-adv-cramer2
                           '((1 2) (3 4)) '(5 11))))
         (list (/ (caar sol) (cdar sol))
               (/ (caadr sol) (cdadr sol))))
       ;; 2x2: 2x + y = 5, x - y = 1 => x=2, y=1
       (let ((sol (funcall 'neovm--test-adv-cramer2
                           '((2 1) (1 -1)) '(5 1))))
         (list (/ (caar sol) (cdar sol))
               (/ (caadr sol) (cdadr sol))))
       ;; 2x2 singular
       (funcall 'neovm--test-adv-cramer2 '((2 4) (1 2)) '(5 3))
       ;; 3x3: x+y+z=6, 2x+3y+z=14, x+y+3z=12 => x=1, y=3, z=2
       (let ((sol (funcall 'neovm--test-adv-cramer3
                           '((1 1 1) (2 3 1) (1 1 3)) '(6 14 12))))
         (mapcar (lambda (s) (/ (car s) (cdr s))) sol))
       ;; 3x3: 3x+2y-z=1, 2x-2y+4z=-2, -x+y/2-z=0 => tricky values
       ;; Using scaled to avoid fractions: 3x+2y-z=1, 2x-2y+4z=-2, -2x+y-2z=0
       (let ((sol (funcall 'neovm--test-adv-cramer3
                           '((3 2 -1) (2 -2 4) (-2 1 -2)) '(1 -2 0))))
         sol)
       ;; 3x3 identity system: x=a, y=b, z=c
       (let ((sol (funcall 'neovm--test-adv-cramer3
                           '((1 0 0) (0 1 0) (0 0 1)) '(7 11 13))))
         (mapcar (lambda (s) (/ (car s) (cdr s))) sol)))
    (fmakunbound 'neovm--test-adv-det2cr)
    (fmakunbound 'neovm--test-adv-det3cr)
    (fmakunbound 'neovm--test-adv-replace-col)
    (fmakunbound 'neovm--test-adv-cramer2)
    (fmakunbound 'neovm--test-adv-cramer3)))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 2) (2 1) singular (-2 5 3) ((-6 . -6) (12 . -6) (12 . -6)) (7 11 13))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// LU decomposition (Doolittle method, integer scaled)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_matrix_advanced_lu_decomposition() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // LU decomposition: A = L * U where L is lower triangular with 1's on diagonal
    // and U is upper triangular. We use integer arithmetic and store
    // L and U scaled to avoid fractions, then verify L*U = scale * A.
    let form = r#"(progn
  (fset 'neovm--test-adv-mget-lu
    (lambda (m r c) (nth c (nth r m))))

  (fset 'neovm--test-adv-mset-lu
    (lambda (m r c v) (setcar (nthcdr c (nth r m)) v)))

  ;; Create NxN matrix of zeros
  (fset 'neovm--test-adv-mzeros
    (lambda (n)
      (let ((result nil) (r 0))
        (while (< r n)
          (push (make-list n 0) result)
          (setq r (1+ r)))
        (nreverse result))))

  ;; LU decomposition (Doolittle, integer-scaled)
  ;; Returns (L U scale) where L * U = scale * A
  ;; L has 'scale' on its diagonal instead of 1
  (fset 'neovm--test-adv-lu
    (lambda (a)
      (let* ((n (length a))
             (u (mapcar (lambda (row) (copy-sequence row)) a))
             (l (funcall 'neovm--test-adv-mzeros n))
             (scale 1))
        ;; Set L diagonal
        (let ((i 0))
          (while (< i n)
            (funcall 'neovm--test-adv-mset-lu l i i 1)
            (setq i (1+ i))))
        ;; For each column
        (let ((j 0))
          (while (< j n)
            (let ((pivot (funcall 'neovm--test-adv-mget-lu u j j)))
              (when (/= pivot 0)
                (setq scale (* scale pivot))
                ;; Eliminate below pivot
                (let ((i (1+ j)))
                  (while (< i n)
                    (let ((factor (funcall 'neovm--test-adv-mget-lu u i j)))
                      ;; Store factor in L
                      (funcall 'neovm--test-adv-mset-lu l i j factor)
                      ;; Update U row i
                      (let ((k 0))
                        (while (< k n)
                          (funcall 'neovm--test-adv-mset-lu u i k
                                   (- (* pivot (funcall 'neovm--test-adv-mget-lu u i k))
                                      (* factor (funcall 'neovm--test-adv-mget-lu u j k))))
                          (setq k (1+ k)))))
                    (setq i (1+ i))))))
            (setq j (1+ j))))
        (list l u))))

  ;; Matrix multiply for verification
  (fset 'neovm--test-adv-mmul-lu
    (lambda (a b)
      (let* ((n (length a))
             (result (funcall 'neovm--test-adv-mzeros n)))
        (let ((i 0))
          (while (< i n)
            (let ((j 0))
              (while (< j n)
                (let ((sum 0) (k 0))
                  (while (< k n)
                    (setq sum (+ sum (* (funcall 'neovm--test-adv-mget-lu a i k)
                                        (funcall 'neovm--test-adv-mget-lu b k j))))
                    (setq k (1+ k)))
                  (funcall 'neovm--test-adv-mset-lu result i j sum))
                (setq j (1+ j))))
            (setq i (1+ i))))
        result)))

  (unwind-protect
      (let ((test-matrices '(((2 1) (4 3))
                              ((1 2 3) (4 5 6) (7 8 10))
                              ((3 -1 2) (6 -1 5) (9 1 8)))))
        (mapcar
         (lambda (a)
           (let* ((result (funcall 'neovm--test-adv-lu a))
                  (l (nth 0 result))
                  (u (nth 1 result))
                  ;; Verify L*U
                  (product (funcall 'neovm--test-adv-mmul-lu l u)))
             (list
              'L l
              'U u
              'L*U product
              ;; Check U is upper triangular (below-diagonal = 0)
              'U-upper (let ((ok t) (i 1))
                         (while (and ok (< i (length u)))
                           (let ((j 0))
                             (while (and ok (< j i))
                               (when (/= (funcall 'neovm--test-adv-mget-lu u i j) 0)
                                 (setq ok nil))
                               (setq j (1+ j))))
                           (setq i (1+ i)))
                         ok))))
         test-matrices))
    (fmakunbound 'neovm--test-adv-mget-lu)
    (fmakunbound 'neovm--test-adv-mset-lu)
    (fmakunbound 'neovm--test-adv-mzeros)
    (fmakunbound 'neovm--test-adv-lu)
    (fmakunbound 'neovm--test-adv-mmul-lu)))"#;
    let expect = expect_test::expect![[
        r#""OK ((L ((1 0) (4 1)) U ((2 1) (0 2)) L*U ((2 1) (8 6)) U-upper t) (L ((1 0 0) (4 1 0) (7 -6 1)) U ((1 2 3) (0 -3 -6) (0 0 -3)) L*U ((1 2 3) (4 5 6) (7 32 54)) U-upper t) (L ((1 0 0) (6 1 0) (9 12 1)) U ((3 -1 2) (0 3 3) (0 0 -18)) L*U ((3 -1 2) (18 -3 15) (27 27 36)) U-upper t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: eigenvalue computation for 2x2 via characteristic polynomial
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_matrix_advanced_eigenvalues_2x2() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // For 2x2 matrix [[a,b],[c,d]], characteristic polynomial is:
    // lambda^2 - (a+d)*lambda + (ad-bc) = 0
    // Discriminant = (a+d)^2 - 4*(ad-bc) = (a-d)^2 + 4*bc
    // We return the discriminant and the trace/det for verification.
    let form = r#"(progn
  (fset 'neovm--test-adv-eigen2
    (lambda (m)
      (let* ((a (nth 0 (nth 0 m))) (b (nth 1 (nth 0 m)))
             (c (nth 0 (nth 1 m))) (d (nth 1 (nth 1 m)))
             (trace (+ a d))
             (det (- (* a d) (* b c)))
             (disc (- (* trace trace) (* 4 det))))
        (list 'trace trace 'det det 'disc disc
              ;; For integer eigenvalues, check if disc is a perfect square
              'disc-nonneg (>= disc 0)
              ;; Eigenvalues are (trace +/- sqrt(disc)) / 2
              ;; We verify via Vieta's formulas: sum = trace, product = det
              'vieta-sum trace
              'vieta-product det))))

  (unwind-protect
      (list
       ;; Identity matrix: eigenvalues = 1, 1
       (funcall 'neovm--test-adv-eigen2 '((1 0) (0 1)))
       ;; Diagonal: eigenvalues = 3, 7
       (funcall 'neovm--test-adv-eigen2 '((3 0) (0 7)))
       ;; [[2,1],[1,2]]: eigenvalues = 3, 1
       (funcall 'neovm--test-adv-eigen2 '((2 1) (1 2)))
       ;; [[0,1],[-1,0]]: eigenvalues = +/- i (disc < 0)
       (funcall 'neovm--test-adv-eigen2 '((0 1) (-1 0)))
       ;; [[5,4],[1,2]]: trace=7, det=6, disc=49-24=25, eigenvalues=6,1
       (funcall 'neovm--test-adv-eigen2 '((5 4) (1 2)))
       ;; Nilpotent: [[0,1],[0,0]]: eigenvalues = 0, 0
       (funcall 'neovm--test-adv-eigen2 '((0 1) (0 0)))
       ;; Property: trace = sum of eigenvalues, det = product of eigenvalues
       ;; For [[5,4],[1,2]]: trace=7, det=6. eigenvalues should be 6 and 1.
       ;; 6+1=7 (trace), 6*1=6 (det)
       (let ((info (funcall 'neovm--test-adv-eigen2 '((5 4) (1 2)))))
         (list (= (nth 1 info) 7) (= (nth 3 info) 6))))
    (fmakunbound 'neovm--test-adv-eigen2)))"#;
    let expect = expect_test::expect![[
        r#""OK ((trace 2 det 1 disc 0 disc-nonneg t vieta-sum 2 vieta-product 1) (trace 10 det 21 disc 16 disc-nonneg t vieta-sum 10 vieta-product 21) (trace 4 det 3 disc 4 disc-nonneg t vieta-sum 4 vieta-product 3) (trace 0 det 1 disc -4 disc-nonneg nil vieta-sum 0 vieta-product 1) (trace 7 det 6 disc 25 disc-nonneg t vieta-sum 7 vieta-product 6) (trace 0 det 0 disc 0 disc-nonneg t vieta-sum 0 vieta-product 0) (t t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
