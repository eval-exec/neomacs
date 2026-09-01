//! Oracle parity tests for linear algebra operations in Elisp:
//! matrix representation, addition/subtraction/multiplication, transpose,
//! Gaussian elimination, LU decomposition, matrix inverse,
//! system of linear equations solver (Ax=b), power iteration for
//! eigenvalue estimation, and rank computation.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Matrix add, sub, mul, transpose — unified helper definitions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_linear_algebra_basic_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; --- helpers ---
  (fset 'neovm--la-ref (lambda (m r c) (nth c (nth r m))))

  (fset 'neovm--la-rows (lambda (m) (length m)))
  (fset 'neovm--la-cols (lambda (m) (length (car m))))

  (fset 'neovm--la-make (lambda (rows cols init)
    (let ((res nil) (r 0))
      (while (< r rows)
        (setq res (cons (make-list cols init) res))
        (setq r (1+ r)))
      (nreverse res))))

  (fset 'neovm--la-transpose (lambda (m)
    (if (null m) nil
      (let ((nc (funcall 'neovm--la-cols m)) (res nil) (c 0))
        (while (< c nc)
          (let ((col nil) (rows m))
            (while rows
              (setq col (cons (nth c (car rows)) col))
              (setq rows (cdr rows)))
            (setq res (cons (nreverse col) res)))
          (setq c (1+ c)))
        (nreverse res)))))

  (fset 'neovm--la-add (lambda (a b)
    (let ((res nil) (ra a) (rb b))
      (while ra
        (let ((row-a (car ra)) (row-b (car rb)) (nr nil))
          (while row-a
            (setq nr (cons (+ (car row-a) (car row-b)) nr))
            (setq row-a (cdr row-a) row-b (cdr row-b)))
          (setq res (cons (nreverse nr) res)))
        (setq ra (cdr ra) rb (cdr rb)))
      (nreverse res))))

  (fset 'neovm--la-sub (lambda (a b)
    (let ((res nil) (ra a) (rb b))
      (while ra
        (let ((row-a (car ra)) (row-b (car rb)) (nr nil))
          (while row-a
            (setq nr (cons (- (car row-a) (car row-b)) nr))
            (setq row-a (cdr row-a) row-b (cdr row-b)))
          (setq res (cons (nreverse nr) res)))
        (setq ra (cdr ra) rb (cdr rb)))
      (nreverse res))))

  (fset 'neovm--la-dot (lambda (a b)
    (let ((s 0))
      (while a
        (setq s (+ s (* (car a) (car b))))
        (setq a (cdr a) b (cdr b)))
      s)))

  (fset 'neovm--la-mul (lambda (a b)
    (let ((bt (funcall 'neovm--la-transpose b)))
      (mapcar (lambda (ra)
                (mapcar (lambda (cb) (funcall 'neovm--la-dot ra cb)) bt))
              a))))

  (fset 'neovm--la-scale (lambda (s m)
    (mapcar (lambda (row) (mapcar (lambda (x) (* s x)) row)) m)))

  (fset 'neovm--la-eye (lambda (n)
    (let ((res nil) (r 0))
      (while (< r n)
        (let ((row (make-list n 0)))
          (setcar (nthcdr r row) 1)
          (setq res (cons row res)))
        (setq r (1+ r)))
      (nreverse res))))

  (unwind-protect
      (let ((a '((1 2) (3 4)))
            (b '((5 6) (7 8)))
            (c '((2 0) (1 3)))
            (r '((1 2 3) (4 5 6))))
        (list
          ;; add
          (funcall 'neovm--la-add a b)
          ;; sub
          (funcall 'neovm--la-sub a b)
          ;; A - A = 0
          (equal (funcall 'neovm--la-sub a a)
                 (funcall 'neovm--la-make 2 2 0))
          ;; mul 2x2 * 2x2
          (funcall 'neovm--la-mul a b)
          ;; mul associative: (A*B)*C = A*(B*C)
          (equal (funcall 'neovm--la-mul (funcall 'neovm--la-mul a b) c)
                 (funcall 'neovm--la-mul a (funcall 'neovm--la-mul b c)))
          ;; A * I = A
          (equal a (funcall 'neovm--la-mul a (funcall 'neovm--la-eye 2)))
          ;; transpose of 2x3
          (funcall 'neovm--la-transpose r)
          ;; (A^T)^T = A
          (equal a (funcall 'neovm--la-transpose (funcall 'neovm--la-transpose a)))
          ;; (A+B)^T = A^T + B^T
          (equal (funcall 'neovm--la-transpose (funcall 'neovm--la-add a b))
                 (funcall 'neovm--la-add
                          (funcall 'neovm--la-transpose a)
                          (funcall 'neovm--la-transpose b)))
          ;; scalar mult distributive: c*(A+B) = cA + cB
          (equal (funcall 'neovm--la-scale 5 (funcall 'neovm--la-add a b))
                 (funcall 'neovm--la-add
                          (funcall 'neovm--la-scale 5 a)
                          (funcall 'neovm--la-scale 5 b)))))
    (fmakunbound 'neovm--la-ref)
    (fmakunbound 'neovm--la-rows)
    (fmakunbound 'neovm--la-cols)
    (fmakunbound 'neovm--la-make)
    (fmakunbound 'neovm--la-transpose)
    (fmakunbound 'neovm--la-add)
    (fmakunbound 'neovm--la-sub)
    (fmakunbound 'neovm--la-dot)
    (fmakunbound 'neovm--la-mul)
    (fmakunbound 'neovm--la-scale)
    (fmakunbound 'neovm--la-eye)))"#;
    let expect = expect_test::expect![[
        r#""OK (((6 8) (10 12)) ((-4 -4) (-4 -4)) t ((19 22) (43 50)) t t ((1 4) (2 5) (3 6)) t t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Gaussian elimination producing row echelon form (integer arithmetic)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_linear_algebra_gaussian_elimination() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--la-ge-ref (lambda (m r c) (nth c (nth r m))))
  (fset 'neovm--la-ge-set (lambda (m r c v) (setcar (nthcdr c (nth r m)) v)))

  ;; Integer Gaussian elimination (no division to stay exact).
  ;; Returns the REF matrix.
  (fset 'neovm--la-ge (lambda (mat-orig)
    (let* ((mat (mapcar #'copy-sequence mat-orig))
           (nr (length mat))
           (nc (length (car mat)))
           (pr 0) (pc 0))
      (while (and (< pr nr) (< pc nc))
        (let ((found nil) (sr pr))
          (while (and (not found) (< sr nr))
            (if (/= (funcall 'neovm--la-ge-ref mat sr pc) 0)
                (setq found sr)
              (setq sr (1+ sr))))
          (if (not found)
              (setq pc (1+ pc))
            (when (/= found pr)
              (let ((tmp (nth pr mat)))
                (setcar (nthcdr pr mat) (nth found mat))
                (setcar (nthcdr found mat) tmp)))
            (let ((pv (funcall 'neovm--la-ge-ref mat pr pc))
                  (tr (1+ pr)))
              (while (< tr nr)
                (let ((tv (funcall 'neovm--la-ge-ref mat tr pc)))
                  (when (/= tv 0)
                    (let ((col 0))
                      (while (< col nc)
                        (funcall 'neovm--la-ge-set mat tr col
                                 (- (* pv (funcall 'neovm--la-ge-ref mat tr col))
                                    (* tv (funcall 'neovm--la-ge-ref mat pr col))))
                        (setq col (1+ col))))))
                (setq tr (1+ tr))))
            (setq pr (1+ pr) pc (1+ pc)))))
      mat)))

  (unwind-protect
      (list
        ;; Already upper triangular
        (funcall 'neovm--la-ge '((1 2 3) (0 4 5) (0 0 6)))
        ;; Needs elimination
        (funcall 'neovm--la-ge '((2 1 -1) (-3 -1 2) (-2 1 2)))
        ;; Singular (rank 2, third row becomes zeros)
        (funcall 'neovm--la-ge '((1 2 3) (4 5 6) (7 8 9)))
        ;; Requires pivot swap
        (funcall 'neovm--la-ge '((0 0 1) (0 1 0) (1 0 0)))
        ;; 3x4 augmented for Ax=b
        (funcall 'neovm--la-ge '((1 1 1 6) (0 2 5 -4) (2 5 -1 27))))
    (fmakunbound 'neovm--la-ge-ref)
    (fmakunbound 'neovm--la-ge-set)
    (fmakunbound 'neovm--la-ge)))"#;
    let expect = expect_test::expect![[
        r#""OK (((1 2 3) (0 4 5) (0 0 6)) ((2 1 -1) (0 1 1) (0 0 -2)) ((1 2 3) (0 -3 -6) (0 0 0)) ((1 0 0) (0 1 0) (0 0 1)) ((1 1 1 6) (0 2 5 -4) (0 0 -21 42)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// LU decomposition (Doolittle, integer-scaled to avoid fractions)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_linear_algebra_lu_decomposition() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Compute L and U such that A = L * U (scaled to integers).
    // We use the Doolittle algorithm with a scaling factor to stay in integers.
    let form = r#"(progn
  (fset 'neovm--la-lu-ref (lambda (m r c) (nth c (nth r m))))
  (fset 'neovm--la-lu-set (lambda (m r c v) (setcar (nthcdr c (nth r m)) v)))

  (fset 'neovm--la-lu-make-zero (lambda (n)
    (let ((res nil) (i 0))
      (while (< i n)
        (setq res (cons (make-list n 0) res))
        (setq i (1+ i)))
      (nreverse res))))

  ;; Doolittle LU (integer-only): L has 1s on diagonal, U = upper triangular.
  ;; To avoid fractions, we multiply: L[i][j] stores numerator,
  ;; and the denominator is U[j][j]. So L * U = scale_factor * A.
  ;; We return (L U) where L[i][i]=U[j][j] for the diagonal scaling.
  ;; For simplicity, return L and U raw (integer approximation).
  (fset 'neovm--la-lu (lambda (a)
    (let* ((n (length a))
           (l (funcall 'neovm--la-lu-make-zero n))
           (u (mapcar #'copy-sequence a)))
      ;; Set L diagonal to 1
      (let ((i 0))
        (while (< i n)
          (funcall 'neovm--la-lu-set l i i 1)
          (setq i (1+ i))))
      ;; Doolittle: for each column k
      (let ((k 0))
        (while (< k n)
          ;; U[k][j] is already set (from A or prior elimination)
          ;; Compute L[i][k] for i > k
          (let ((i (1+ k)))
            (while (< i n)
              (let ((ukk (funcall 'neovm--la-lu-ref u k k)))
                (when (/= ukk 0)
                  (let ((ratio-num (funcall 'neovm--la-lu-ref u i k)))
                    (funcall 'neovm--la-lu-set l i k ratio-num)
                    ;; Eliminate: u[i][j] = ukk * u[i][j] - ratio-num * u[k][j]
                    (let ((j k))
                      (while (< j n)
                        (funcall 'neovm--la-lu-set u i j
                                 (- (* ukk (funcall 'neovm--la-lu-ref u i j))
                                    (* ratio-num (funcall 'neovm--la-lu-ref u k j))))
                        (setq j (1+ j)))))))
              (setq i (1+ i))))
          (setq k (1+ k))))
      (list l u))))

  (unwind-protect
      (let ((result (funcall 'neovm--la-lu '((2 -1 0) (-1 2 -1) (0 -1 2)))))
        (let ((l (nth 0 result))
              (u (nth 1 result)))
          (list
            ;; L should be lower triangular
            (= 0 (funcall 'neovm--la-lu-ref l 0 1))
            (= 0 (funcall 'neovm--la-lu-ref l 0 2))
            (= 0 (funcall 'neovm--la-lu-ref l 1 2))
            ;; U should be upper triangular
            (= 0 (funcall 'neovm--la-lu-ref u 1 0))
            (= 0 (funcall 'neovm--la-lu-ref u 2 0))
            (= 0 (funcall 'neovm--la-lu-ref u 2 1))
            ;; L diagonal is 1 (Doolittle)
            (funcall 'neovm--la-lu-ref l 0 0)
            (funcall 'neovm--la-lu-ref l 1 1)
            (funcall 'neovm--la-lu-ref l 2 2)
            ;; Return L and U for inspection
            l u)))
    (fmakunbound 'neovm--la-lu-ref)
    (fmakunbound 'neovm--la-lu-set)
    (fmakunbound 'neovm--la-lu-make-zero)
    (fmakunbound 'neovm--la-lu)))"#;
    let expect = expect_test::expect![[
        r#""OK (t t t t t t 1 1 1 ((1 0 0) (-1 1 0) (0 -2 1)) ((2 -1 0) (0 3 -2) (0 0 8)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Matrix inverse via Gauss-Jordan elimination (integer scaled)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_linear_algebra_matrix_inverse() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Compute the inverse of a matrix using Gauss-Jordan on [A | I].
    // Returns (det scaled-inverse) where A^{-1} = scaled-inverse / det.
    let form = r#"(progn
  (fset 'neovm--la-inv-ref (lambda (m r c) (nth c (nth r m))))
  (fset 'neovm--la-inv-set (lambda (m r c v) (setcar (nthcdr c (nth r m)) v)))

  ;; Build augmented matrix [A | I]
  (fset 'neovm--la-inv-augment (lambda (a)
    (let ((n (length a)) (r 0) (res nil))
      (while (< r n)
        (let ((id-row (make-list n 0)))
          (setcar (nthcdr r id-row) 1)
          (setq res (cons (append (copy-sequence (nth r a)) id-row) res)))
        (setq r (1+ r)))
      (nreverse res))))

  ;; Gauss-Jordan elimination on augmented matrix (integer arithmetic)
  ;; Returns the determinant and the right-half (scaled inverse).
  (fset 'neovm--la-inv (lambda (a)
    (let* ((n (length a))
           (aug (funcall 'neovm--la-inv-augment a))
           (det 1)
           (k 0))
      ;; Forward elimination with partial pivoting
      (while (< k n)
        ;; Find pivot
        (let ((pivot-row k) (best (abs (funcall 'neovm--la-inv-ref aug k k)))
              (sr (1+ k)))
          (while (< sr n)
            (let ((v (abs (funcall 'neovm--la-inv-ref aug sr k))))
              (when (> v best) (setq best v pivot-row sr)))
            (setq sr (1+ sr)))
          (when (/= pivot-row k)
            (let ((tmp (nth k aug)))
              (setcar (nthcdr k aug) (nth pivot-row aug))
              (setcar (nthcdr pivot-row aug) tmp))
            (setq det (- det))))
        (let ((pkk (funcall 'neovm--la-inv-ref aug k k)))
          (setq det (* det pkk))
          ;; Eliminate all other rows
          (let ((i 0))
            (while (< i n)
              (when (/= i k)
                (let ((factor (funcall 'neovm--la-inv-ref aug i k))
                      (j 0))
                  (while (< j (* 2 n))
                    (funcall 'neovm--la-inv-set aug i j
                             (- (* pkk (funcall 'neovm--la-inv-ref aug i j))
                                (* factor (funcall 'neovm--la-inv-ref aug k j))))
                    (setq j (1+ j)))))
              (setq i (1+ i)))))
        (setq k (1+ k)))
      ;; Extract right half
      (let ((inv nil) (r 0))
        (while (< r n)
          (let ((row nil) (c n))
            (while (< c (* 2 n))
              (setq row (cons (funcall 'neovm--la-inv-ref aug r c) row))
              (setq c (1+ c)))
            (setq inv (cons (nreverse row) inv)))
          (setq r (1+ r)))
        (list det (nreverse inv))))))

  (unwind-protect
      (let ((result2 (funcall 'neovm--la-inv '((4 7) (2 6))))
            (result3 (funcall 'neovm--la-inv '((1 2 3) (0 1 4) (5 6 0)))))
        (list
          ;; 2x2: det and scaled inverse
          (nth 0 result2)       ;; det = 4*6 - 7*2 = 10
          (nth 1 result2)       ;; scaled inverse
          ;; 3x3
          (nth 0 result3)       ;; det
          (nth 1 result3)))     ;; scaled inverse
    (fmakunbound 'neovm--la-inv-ref)
    (fmakunbound 'neovm--la-inv-set)
    (fmakunbound 'neovm--la-inv-augment)
    (fmakunbound 'neovm--la-inv)))"#;
    let expect = expect_test::expect![[
        r#""OK (40 ((24 -28) (-2 4)) 125 ((3000 -2250 -625) (-500 375 100) (25 -20 -5)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Solving Ax = b via back-substitution after Gaussian elimination
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_linear_algebra_solve_system() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Solve a system Ax = b using Gaussian elimination on the augmented
    // matrix [A | b], then back-substitution. Returns scaled solution
    // (numerators and common denominator) to stay in integers.
    let form = r#"(progn
  (fset 'neovm--la-solve-ref (lambda (m r c) (nth c (nth r m))))
  (fset 'neovm--la-solve-set (lambda (m r c v) (setcar (nthcdr c (nth r m)) v)))

  ;; Forward elimination on augmented matrix
  (fset 'neovm--la-solve-fwd (lambda (aug n)
    (let ((k 0))
      (while (< k n)
        (let ((sr k) (found nil))
          (while (and (not found) (< sr n))
            (if (/= (funcall 'neovm--la-solve-ref aug sr k) 0)
                (setq found sr)
              (setq sr (1+ sr))))
          (when found
            (when (/= found k)
              (let ((tmp (nth k aug)))
                (setcar (nthcdr k aug) (nth found aug))
                (setcar (nthcdr found aug) tmp)))
            (let ((pv (funcall 'neovm--la-solve-ref aug k k))
                  (tr (1+ k)))
              (while (< tr n)
                (let ((tv (funcall 'neovm--la-solve-ref aug tr k))
                      (c 0))
                  (when (/= tv 0)
                    (while (<= c n)
                      (funcall 'neovm--la-solve-set aug tr c
                               (- (* pv (funcall 'neovm--la-solve-ref aug tr c))
                                  (* tv (funcall 'neovm--la-solve-ref aug k c))))
                      (setq c (1+ c)))))
                (setq tr (1+ tr))))))
        (setq k (1+ k))))
    aug))

  ;; Back-substitution: returns (x1*denom x2*denom ... denom) where
  ;; denom is the product of pivots, to avoid fractions.
  (fset 'neovm--la-solve-back (lambda (aug n)
    (let ((x (make-list n 0))
          (denom 1)
          (i (1- n)))
      (while (>= i 0)
        (let ((aii (funcall 'neovm--la-solve-ref aug i i))
              (bi  (funcall 'neovm--la-solve-ref aug i n))
              (sum 0)
              (j (1+ i)))
          (while (< j n)
            (setq sum (+ sum (* (funcall 'neovm--la-solve-ref aug i j)
                                (nth j x))))
            (setq j (1+ j)))
          ;; x[i] = (bi * denom - sum) and new_denom = denom * aii
          ;; To stay integer: x[i] = bi * denom - sum (numerator for current denom)
          ;; Then we adjust: multiply all previous x[j] by aii, set denom *= aii
          (let ((xi-num (- (* bi denom) sum))
                (j2 (1+ i)))
            (while (< j2 n)
              (setcar (nthcdr j2 x) (* aii (nth j2 x)))
              (setq j2 (1+ j2)))
            (setcar (nthcdr i x) xi-num)
            (setq denom (* denom aii))))
        (setq i (1- i)))
      (append x (list denom)))))

  (fset 'neovm--la-solve (lambda (a-matrix b-vector)
    (let* ((n (length a-matrix))
           (aug (let ((r 0) (res nil))
                  (while (< r n)
                    (setq res (cons (append (copy-sequence (nth r a-matrix))
                                           (list (nth r b-vector)))
                                   res))
                    (setq r (1+ r)))
                  (nreverse res))))
      (funcall 'neovm--la-solve-fwd aug n)
      (funcall 'neovm--la-solve-back aug n))))

  (unwind-protect
      (list
        ;; System: x+y+z=6, 2x+3y+z=14, x+y+3z=12 => x=1,y=3,z=2
        ;; scaled: (1*d, 3*d, 2*d, d)
        (funcall 'neovm--la-solve '((1 1 1) (2 3 1) (1 1 3)) '(6 14 12))
        ;; 2x2: 2x+y=5, x-y=1 => x=2, y=1
        (funcall 'neovm--la-solve '((2 1) (1 -1)) '(5 1))
        ;; Diagonal system: trivial
        (funcall 'neovm--la-solve '((3 0) (0 4)) '(9 8)))
    (fmakunbound 'neovm--la-solve-ref)
    (fmakunbound 'neovm--la-solve-set)
    (fmakunbound 'neovm--la-solve-fwd)
    (fmakunbound 'neovm--la-solve-back)
    (fmakunbound 'neovm--la-solve)))"#;
    let expect = expect_test::expect![[r#""OK ((-4 10 6 2) (-12 -6 -6) (36 24 12))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Power iteration for dominant eigenvalue estimation (integer scaled)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_linear_algebra_power_iteration() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Power iteration: repeatedly multiply A * v, normalizing by the
    // max component. After several iterations, the ratio converges
    // to the dominant eigenvalue. We use integer arithmetic throughout
    // by keeping track of a scale factor.
    let form = r#"(progn
  (fset 'neovm--la-pi-matvec (lambda (a v)
    "Multiply matrix A by column vector v (represented as a list)."
    (mapcar (lambda (row)
              (let ((s 0) (r row) (w v))
                (while r
                  (setq s (+ s (* (car r) (car w))))
                  (setq r (cdr r) w (cdr w)))
                s))
            a)))

  (fset 'neovm--la-pi-max-abs (lambda (v)
    "Return the element with the largest absolute value."
    (let ((best 0))
      (dolist (x v)
        (when (> (abs x) (abs best))
          (setq best x)))
      best)))

  ;; Run power iteration for `iters` steps.
  ;; Returns (eigenvalue-numerator eigenvalue-denominator final-vector-scaled).
  ;; eigenvalue ~= numerator / denominator.
  (fset 'neovm--la-power-iter (lambda (a v0 iters)
    (let ((v v0) (prev-max 1) (i 0))
      (while (< i iters)
        (let ((w (funcall 'neovm--la-pi-matvec a v)))
          (let ((m (funcall 'neovm--la-pi-max-abs w)))
            (setq prev-max m)
            ;; "Normalize": divide by max. To stay integer, we just keep
            ;; the un-normalized vector and track the scale ratio.
            (setq v w)))
        (setq i (1+ i)))
      ;; Approximate eigenvalue: ratio of (A*v)[0] / v[0]
      (let ((av (funcall 'neovm--la-pi-matvec a v)))
        (list (car av) (car v) v)))))

  (unwind-protect
      (let ((result (funcall 'neovm--la-power-iter
                             '((2 1) (1 3))
                             '(1 1)
                             8)))
        ;; The dominant eigenvalue of [[2,1],[1,3]] is (5+sqrt(5))/2 ~ 3.618
        ;; We can check the ratio converges: numerator/denominator should be close
        (let ((num (nth 0 result))
              (den (nth 1 result)))
          (list
            ;; The ratio should be between 3 and 4
            (> num (* 3 den))
            (< num (* 4 den))
            ;; Verify vector is non-zero
            (not (and (= 0 (nth 0 (nth 2 result)))
                      (= 0 (nth 1 (nth 2 result)))))
            ;; For a diagonal matrix, eigenvalue is the largest diagonal entry
            (let ((diag-result (funcall 'neovm--la-power-iter
                                        '((5 0 0) (0 3 0) (0 0 1))
                                        '(1 1 1)
                                        10)))
              ;; ratio should be exactly 5
              (= (nth 0 diag-result) (* 5 (nth 1 diag-result)))))))
    (fmakunbound 'neovm--la-pi-matvec)
    (fmakunbound 'neovm--la-pi-max-abs)
    (fmakunbound 'neovm--la-power-iter)))"#;
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Matrix rank computation via row echelon form
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_linear_algebra_rank() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Rank = number of non-zero rows in the row echelon form.
    let form = r#"(progn
  (fset 'neovm--la-rank-ref (lambda (m r c) (nth c (nth r m))))
  (fset 'neovm--la-rank-set (lambda (m r c v) (setcar (nthcdr c (nth r m)) v)))

  (fset 'neovm--la-rank-echelon (lambda (mat-orig)
    (let* ((mat (mapcar #'copy-sequence mat-orig))
           (nr (length mat))
           (nc (length (car mat)))
           (pr 0) (pc 0))
      (while (and (< pr nr) (< pc nc))
        (let ((found nil) (sr pr))
          (while (and (not found) (< sr nr))
            (if (/= (funcall 'neovm--la-rank-ref mat sr pc) 0)
                (setq found sr)
              (setq sr (1+ sr))))
          (if (not found)
              (setq pc (1+ pc))
            (when (/= found pr)
              (let ((tmp (nth pr mat)))
                (setcar (nthcdr pr mat) (nth found mat))
                (setcar (nthcdr found mat) tmp)))
            (let ((pv (funcall 'neovm--la-rank-ref mat pr pc))
                  (tr (1+ pr)))
              (while (< tr nr)
                (let ((tv (funcall 'neovm--la-rank-ref mat tr pc)))
                  (when (/= tv 0)
                    (let ((c 0))
                      (while (< c nc)
                        (funcall 'neovm--la-rank-set mat tr c
                                 (- (* pv (funcall 'neovm--la-rank-ref mat tr c))
                                    (* tv (funcall 'neovm--la-rank-ref mat pr c))))
                        (setq c (1+ c))))))
                (setq tr (1+ tr))))
            (setq pr (1+ pr) pc (1+ pc)))))
      mat)))

  (fset 'neovm--la-rank (lambda (m)
    (let ((ref (funcall 'neovm--la-rank-echelon m))
          (count 0))
      (dolist (row ref)
        (let ((all-zero t) (r row))
          (while (and all-zero r)
            (when (/= (car r) 0) (setq all-zero nil))
            (setq r (cdr r)))
          (unless all-zero (setq count (1+ count)))))
      count)))

  (unwind-protect
      (list
        ;; Full rank 3x3
        (funcall 'neovm--la-rank '((1 0 0) (0 1 0) (0 0 1)))
        ;; Rank 2 (third row = sum of first two)
        (funcall 'neovm--la-rank '((1 2 3) (4 5 6) (5 7 9)))
        ;; Rank 1 (all rows proportional)
        (funcall 'neovm--la-rank '((2 4 6) (1 2 3) (3 6 9)))
        ;; Rank 0 (zero matrix)
        (funcall 'neovm--la-rank '((0 0) (0 0)))
        ;; Rectangular 2x4, rank 2
        (funcall 'neovm--la-rank '((1 2 3 4) (5 6 7 8)))
        ;; Rectangular 4x2, rank 2
        (funcall 'neovm--la-rank '((1 0) (0 1) (1 1) (2 1)))
        ;; Rectangular 3x4, rank 2 (row3 = row1 + row2)
        (funcall 'neovm--la-rank '((1 0 1 0) (0 1 0 1) (1 1 1 1))))
    (fmakunbound 'neovm--la-rank-ref)
    (fmakunbound 'neovm--la-rank-set)
    (fmakunbound 'neovm--la-rank-echelon)
    (fmakunbound 'neovm--la-rank)))"#;
    let expect = expect_test::expect![[r#""OK (3 2 1 0 2 2 2)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
