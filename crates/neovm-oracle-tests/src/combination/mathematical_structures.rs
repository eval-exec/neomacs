//! Complex oracle parity tests for mathematical structure implementations.
//!
//! Tests rational number arithmetic with GCD simplification, complex number
//! arithmetic, polynomial addition and multiplication, permutation group
//! operations (composition, inverse), set theory operations (power set,
//! Cartesian product), and interval arithmetic.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Rational number arithmetic (add, mul, simplify with GCD)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_math_rational_arithmetic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Rational: (num . den), always simplified, den > 0
  (fset 'neovm--test-gcd
    (lambda (a b)
      (let ((a (abs a)) (b (abs b)))
        (while (/= b 0)
          (let ((tmp b))
            (setq b (% a b))
            (setq a tmp)))
        a)))
  (fset 'neovm--test-rat-make
    (lambda (n d)
      (if (= d 0) (error "division by zero")
        (let* ((sign (if (< (* n d) 0) -1 1))
               (n (abs n)) (d (abs d))
               (g (funcall 'neovm--test-gcd n d)))
          (cons (* sign (/ n g)) (/ d g))))))
  (fset 'neovm--test-rat-add
    (lambda (a b)
      (funcall 'neovm--test-rat-make
        (+ (* (car a) (cdr b)) (* (car b) (cdr a)))
        (* (cdr a) (cdr b)))))
  (fset 'neovm--test-rat-sub
    (lambda (a b)
      (funcall 'neovm--test-rat-make
        (- (* (car a) (cdr b)) (* (car b) (cdr a)))
        (* (cdr a) (cdr b)))))
  (fset 'neovm--test-rat-mul
    (lambda (a b)
      (funcall 'neovm--test-rat-make
        (* (car a) (car b))
        (* (cdr a) (cdr b)))))
  (fset 'neovm--test-rat-div
    (lambda (a b)
      (funcall 'neovm--test-rat-make
        (* (car a) (cdr b))
        (* (cdr a) (car b)))))
  (fset 'neovm--test-rat-equal
    (lambda (a b) (and (= (car a) (car b)) (= (cdr a) (cdr b)))))
  (fset 'neovm--test-rat-to-string
    (lambda (r) (format "%d/%d" (car r) (cdr r))))
  (unwind-protect
      (let* ((half (funcall 'neovm--test-rat-make 1 2))
             (third (funcall 'neovm--test-rat-make 1 3))
             (quarter (funcall 'neovm--test-rat-make 1 4))
             (two-thirds (funcall 'neovm--test-rat-make 2 3))
             (neg-half (funcall 'neovm--test-rat-make -1 2)))
        (list
          ;; Simplification
          (funcall 'neovm--test-rat-make 6 8)     ;; -> 3/4
          (funcall 'neovm--test-rat-make 12 18)    ;; -> 2/3
          (funcall 'neovm--test-rat-make -4 6)     ;; -> -2/3
          ;; Addition: 1/2 + 1/3 = 5/6
          (funcall 'neovm--test-rat-add half third)
          ;; 1/2 + 1/4 = 3/4
          (funcall 'neovm--test-rat-add half quarter)
          ;; Subtraction: 2/3 - 1/3 = 1/3
          (funcall 'neovm--test-rat-sub two-thirds third)
          ;; Multiplication: 1/2 * 2/3 = 1/3
          (funcall 'neovm--test-rat-mul half two-thirds)
          ;; Division: (1/2) / (1/3) = 3/2
          (funcall 'neovm--test-rat-div half third)
          ;; Negative: -1/2 + 1/2 = 0/1
          (funcall 'neovm--test-rat-add neg-half half)
          ;; Chain: 1/2 + 1/3 + 1/4 = 13/12
          (funcall 'neovm--test-rat-add
            (funcall 'neovm--test-rat-add half third) quarter)
          ;; Multiplicative inverse: (2/3) * (3/2) = 1/1
          (funcall 'neovm--test-rat-mul
            two-thirds (funcall 'neovm--test-rat-div
                          (funcall 'neovm--test-rat-make 1 1) two-thirds))
          ;; String representation
          (funcall 'neovm--test-rat-to-string
            (funcall 'neovm--test-rat-add half third))))
    (fmakunbound 'neovm--test-gcd)
    (fmakunbound 'neovm--test-rat-make)
    (fmakunbound 'neovm--test-rat-add)
    (fmakunbound 'neovm--test-rat-sub)
    (fmakunbound 'neovm--test-rat-mul)
    (fmakunbound 'neovm--test-rat-div)
    (fmakunbound 'neovm--test-rat-equal)
    (fmakunbound 'neovm--test-rat-to-string)))"#;
    let expect = expect_test::expect![[
        r#""OK ((3 . 4) (2 . 3) (-2 . 3) (5 . 6) (3 . 4) (1 . 3) (1 . 3) (3 . 2) (0 . 1) (13 . 12) (1 . 1) \"5/6\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex number arithmetic (add, mul, magnitude, conjugate)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_math_complex_arithmetic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use integer-scaled arithmetic (x1000) to avoid float precision issues
    let form = r#"(progn
  ;; Complex: (real . imag), both integers (scaled by 1000)
  (fset 'neovm--test-cx-make (lambda (r i) (cons r i)))
  (fset 'neovm--test-cx-real (lambda (z) (car z)))
  (fset 'neovm--test-cx-imag (lambda (z) (cdr z)))
  (fset 'neovm--test-cx-add
    (lambda (a b)
      (cons (+ (car a) (car b)) (+ (cdr a) (cdr b)))))
  (fset 'neovm--test-cx-sub
    (lambda (a b)
      (cons (- (car a) (car b)) (- (cdr a) (cdr b)))))
  (fset 'neovm--test-cx-mul
    (lambda (a b)
      ;; (a+bi)(c+di) = (ac-bd) + (ad+bc)i
      (cons (- (* (car a) (car b)) (* (cdr a) (cdr b)))
            (+ (* (car a) (cdr b)) (* (cdr a) (car b))))))
  (fset 'neovm--test-cx-conjugate
    (lambda (z) (cons (car z) (- (cdr z)))))
  (fset 'neovm--test-cx-magnitude-sq
    (lambda (z) (+ (* (car z) (car z)) (* (cdr z) (cdr z)))))
  (fset 'neovm--test-cx-to-string
    (lambda (z)
      (if (>= (cdr z) 0)
          (format "%d+%di" (car z) (cdr z))
        (format "%d%di" (car z) (cdr z)))))
  (unwind-protect
      (let* ((z1 (funcall 'neovm--test-cx-make 3 4))    ;; 3+4i
             (z2 (funcall 'neovm--test-cx-make 1 -2))   ;; 1-2i
             (z3 (funcall 'neovm--test-cx-make 0 1))    ;; i
             (z4 (funcall 'neovm--test-cx-make -1 0))   ;; -1
             (zero (funcall 'neovm--test-cx-make 0 0)))
        (list
          ;; Addition: (3+4i) + (1-2i) = 4+2i
          (funcall 'neovm--test-cx-add z1 z2)
          ;; Subtraction: (3+4i) - (1-2i) = 2+6i
          (funcall 'neovm--test-cx-sub z1 z2)
          ;; Multiplication: (3+4i)(1-2i) = 3-6i+4i-8i^2 = 11-2i
          (funcall 'neovm--test-cx-mul z1 z2)
          ;; i * i = -1
          (funcall 'neovm--test-cx-mul z3 z3)
          ;; Conjugate of 3+4i = 3-4i
          (funcall 'neovm--test-cx-conjugate z1)
          ;; z * conj(z) = |z|^2 (always real)
          (let ((prod (funcall 'neovm--test-cx-mul z1 (funcall 'neovm--test-cx-conjugate z1))))
            (list prod (= (cdr prod) 0)))
          ;; |3+4i|^2 = 25
          (funcall 'neovm--test-cx-magnitude-sq z1)
          ;; Additive identity: z + 0 = z
          (equal (funcall 'neovm--test-cx-add z1 zero) z1)
          ;; z - z = 0
          (equal (funcall 'neovm--test-cx-sub z1 z1) zero)
          ;; String representations
          (funcall 'neovm--test-cx-to-string z1)
          (funcall 'neovm--test-cx-to-string z2)
          ;; Chain: (1+i)^4 = ((1+i)^2)^2 = (2i)^2 = -4
          (let* ((one-i (funcall 'neovm--test-cx-make 1 1))
                 (sq (funcall 'neovm--test-cx-mul one-i one-i))
                 (fourth (funcall 'neovm--test-cx-mul sq sq)))
            fourth)))
    (fmakunbound 'neovm--test-cx-make)
    (fmakunbound 'neovm--test-cx-real)
    (fmakunbound 'neovm--test-cx-imag)
    (fmakunbound 'neovm--test-cx-add)
    (fmakunbound 'neovm--test-cx-sub)
    (fmakunbound 'neovm--test-cx-mul)
    (fmakunbound 'neovm--test-cx-conjugate)
    (fmakunbound 'neovm--test-cx-magnitude-sq)
    (fmakunbound 'neovm--test-cx-to-string)))"#;
    let expect = expect_test::expect![[
        r#""OK ((4 . 2) (2 . 6) (11 . -2) (-1 . 0) (3 . -4) ((25 . 0) t) 25 t t \"3+4i\" \"1-2i\" (-4 . 0))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Polynomial addition and multiplication
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_math_polynomial_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Polynomial: list of coefficients, index = degree
  ;; e.g., (1 2 3) = 1 + 2x + 3x^2
  (fset 'neovm--test-poly-degree
    (lambda (p)
      (let ((d (1- (length p))))
        ;; strip leading zeros
        (while (and (> d 0) (= (nth d p) 0))
          (setq d (1- d)))
        d)))
  (fset 'neovm--test-poly-normalize
    (lambda (p)
      (let ((d (funcall 'neovm--test-poly-degree p)))
        (let ((result nil) (i 0))
          (while (<= i d)
            (setq result (cons (nth i p) result))
            (setq i (1+ i)))
          (nreverse result)))))
  (fset 'neovm--test-poly-add
    (lambda (a b)
      (let* ((la (length a)) (lb (length b))
             (maxl (max la lb))
             (result nil) (i 0))
        (while (< i maxl)
          (let ((ca (if (< i la) (nth i a) 0))
                (cb (if (< i lb) (nth i b) 0)))
            (setq result (cons (+ ca cb) result)))
          (setq i (1+ i)))
        (funcall 'neovm--test-poly-normalize (nreverse result)))))
  (fset 'neovm--test-poly-scale
    (lambda (p c)
      (mapcar (lambda (x) (* x c)) p)))
  (fset 'neovm--test-poly-mul
    (lambda (a b)
      (let* ((la (length a)) (lb (length b))
             (result-len (1- (+ la lb)))
             (result (make-list result-len 0))
             (i 0))
        (while (< i la)
          (let ((j 0))
            (while (< j lb)
              (let ((k (+ i j))
                    (prod (* (nth i a) (nth j b))))
                (setcar (nthcdr k result) (+ (nth k result) prod)))
              (setq j (1+ j))))
          (setq i (1+ i)))
        (funcall 'neovm--test-poly-normalize result))))
  (fset 'neovm--test-poly-eval
    (lambda (p x)
      ;; Horner's method
      (let ((result 0)
            (i (1- (length p))))
        (while (>= i 0)
          (setq result (+ (* result x) (nth i p)))
          (setq i (1- i)))
        result)))
  (fset 'neovm--test-poly-to-string
    (lambda (p)
      (let ((terms nil) (i 0))
        (dolist (c p)
          (when (/= c 0)
            (setq terms
                  (cons (cond
                          ((= i 0) (format "%d" c))
                          ((= i 1)
                           (if (= c 1) "x"
                             (if (= c -1) "-x"
                               (format "%dx" c))))
                          (t
                           (if (= c 1) (format "x^%d" i)
                             (if (= c -1) (format "-x^%d" i)
                               (format "%dx^%d" c i)))))
                        terms)))
          (setq i (1+ i)))
        (if terms
            (mapconcat 'identity (nreverse terms) " + ")
          "0"))))
  (unwind-protect
      (let* (;; p1 = 1 + 2x + 3x^2
             (p1 '(1 2 3))
             ;; p2 = 2 + x
             (p2 '(2 1))
             ;; p3 = x^2 - 1
             (p3 '(-1 0 1)))
        (list
          ;; Addition: (1+2x+3x^2) + (2+x) = 3+3x+3x^2
          (funcall 'neovm--test-poly-add p1 p2)
          ;; Multiplication: (2+x)(x^2-1) = -2-x+2x^2+x^3
          (funcall 'neovm--test-poly-mul p2 p3)
          ;; (1+x)^2 = 1+2x+x^2
          (funcall 'neovm--test-poly-mul '(1 1) '(1 1))
          ;; Evaluation: p1(2) = 1+4+12 = 17
          (funcall 'neovm--test-poly-eval p1 2)
          ;; Evaluation: p3(3) = 9-1 = 8
          (funcall 'neovm--test-poly-eval p3 3)
          ;; Scale: 3 * (1+2x) = 3+6x
          (funcall 'neovm--test-poly-scale '(1 2) 3)
          ;; Degree
          (funcall 'neovm--test-poly-degree p1)
          (funcall 'neovm--test-poly-degree p2)
          ;; String representations
          (funcall 'neovm--test-poly-to-string p1)
          (funcall 'neovm--test-poly-to-string p2)
          (funcall 'neovm--test-poly-to-string p3)
          ;; Distributive: p2 * (p1 + p3) = p2*p1 + p2*p3
          (equal
            (funcall 'neovm--test-poly-mul p2
              (funcall 'neovm--test-poly-add p1 p3))
            (funcall 'neovm--test-poly-add
              (funcall 'neovm--test-poly-mul p2 p1)
              (funcall 'neovm--test-poly-mul p2 p3)))))
    (fmakunbound 'neovm--test-poly-degree)
    (fmakunbound 'neovm--test-poly-normalize)
    (fmakunbound 'neovm--test-poly-add)
    (fmakunbound 'neovm--test-poly-scale)
    (fmakunbound 'neovm--test-poly-mul)
    (fmakunbound 'neovm--test-poly-eval)
    (fmakunbound 'neovm--test-poly-to-string)))"#;
    let expect = expect_test::expect![[
        r#""OK ((3 3 3) (-2 -1 2 1) (1 2 1) 17 8 (3 6) 2 1 \"1 + 2x + 3x^2\" \"2 + x\" \"-1 + x^2\" t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Permutation group operations (compose, inverse)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_math_permutation_group() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Permutation represented as a vector: perm[i] = where i goes
  ;; e.g., [1 0 2] means 0->1, 1->0, 2->2 (swap first two)
  (fset 'neovm--test-perm-identity
    (lambda (n)
      (let ((p (make-vector n 0)) (i 0))
        (while (< i n) (aset p i i) (setq i (1+ i)))
        p)))
  (fset 'neovm--test-perm-compose
    (lambda (a b)
      ;; apply b first, then a: (a . b)(x) = a(b(x))
      (let* ((n (length a))
             (result (make-vector n 0))
             (i 0))
        (while (< i n)
          (aset result i (aref a (aref b i)))
          (setq i (1+ i)))
        result)))
  (fset 'neovm--test-perm-inverse
    (lambda (p)
      (let* ((n (length p))
             (inv (make-vector n 0))
             (i 0))
        (while (< i n)
          (aset inv (aref p i) i)
          (setq i (1+ i)))
        inv)))
  (fset 'neovm--test-perm-apply
    (lambda (p lst)
      (let ((vec (apply 'vector lst))
            (n (length p))
            (result (make-vector (length p) nil))
            (i 0))
        (while (< i n)
          (aset result (aref p i) (aref vec i))
          (setq i (1+ i)))
        (append result nil))))
  (fset 'neovm--test-perm-order
    (lambda (p)
      ;; smallest k > 0 such that p^k = identity
      (let ((id (funcall 'neovm--test-perm-identity (length p)))
            (current (copy-sequence p))
            (k 1))
        (while (not (equal current id))
          (setq current (funcall 'neovm--test-perm-compose p current))
          (setq k (1+ k)))
        k)))
  (fset 'neovm--test-perm-cycles
    (lambda (p)
      (let* ((n (length p))
             (visited (make-vector n nil))
             (cycles nil))
        (let ((i 0))
          (while (< i n)
            (when (not (aref visited i))
              (let ((cycle nil) (j i))
                (while (not (aref visited j))
                  (aset visited j t)
                  (setq cycle (cons j cycle))
                  (setq j (aref p j)))
                (when (> (length cycle) 1)
                  (setq cycles (cons (nreverse cycle) cycles)))))
            (setq i (1+ i))))
        (nreverse cycles))))
  (unwind-protect
      (let* ((id (funcall 'neovm--test-perm-identity 4))
             ;; swap 0 and 1: (0 1)
             (swap01 (vector 1 0 2 3))
             ;; cycle (0 1 2): 0->1, 1->2, 2->0
             (cycle012 (vector 1 2 0 3))
             ;; 4-cycle: (0 1 2 3)
             (cycle4 (vector 1 2 3 0)))
        (list
          ;; identity composed with anything = itself
          (equal (funcall 'neovm--test-perm-compose id swap01)
                 swap01)
          ;; swap composed with itself = identity
          (equal (funcall 'neovm--test-perm-compose swap01 swap01)
                 id)
          ;; p * p^{-1} = identity
          (equal (funcall 'neovm--test-perm-compose cycle012
                   (funcall 'neovm--test-perm-inverse cycle012))
                 id)
          ;; Apply permutation to list
          (funcall 'neovm--test-perm-apply swap01 '(a b c d))
          (funcall 'neovm--test-perm-apply cycle012 '(a b c d))
          ;; Orders
          (funcall 'neovm--test-perm-order swap01)     ;; 2
          (funcall 'neovm--test-perm-order cycle012)   ;; 3
          (funcall 'neovm--test-perm-order cycle4)     ;; 4
          (funcall 'neovm--test-perm-order id)         ;; 1
          ;; Cycle decomposition
          (funcall 'neovm--test-perm-cycles swap01)
          (funcall 'neovm--test-perm-cycles cycle012)
          (funcall 'neovm--test-perm-cycles cycle4)
          (funcall 'neovm--test-perm-cycles id)
          ;; Non-commutative: swap01 . cycle012 != cycle012 . swap01
          (let ((ab (funcall 'neovm--test-perm-compose swap01 cycle012))
                (ba (funcall 'neovm--test-perm-compose cycle012 swap01)))
            (list (not (equal ab ba)) ab ba))))
    (fmakunbound 'neovm--test-perm-identity)
    (fmakunbound 'neovm--test-perm-compose)
    (fmakunbound 'neovm--test-perm-inverse)
    (fmakunbound 'neovm--test-perm-apply)
    (fmakunbound 'neovm--test-perm-order)
    (fmakunbound 'neovm--test-perm-cycles)))"#;
    let expect = expect_test::expect![[
        r#""OK (t t t (b a c d) (c a b d) 2 3 4 1 ((0 1)) ((0 1 2)) ((0 1 2 3)) nil (t [0 2 1 3] [2 1 0 3]))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Set theory operations (power set, Cartesian product)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_math_set_theory() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Sets as sorted lists (no duplicates)
  (fset 'neovm--test-set-make
    (lambda (lst)
      (let ((sorted (sort (copy-sequence lst)
                          (lambda (a b)
                            (cond
                              ((and (numberp a) (numberp b)) (< a b))
                              ((and (symbolp a) (symbolp b))
                               (string-lessp (symbol-name a) (symbol-name b)))
                              (t (string-lessp (format "%s" a) (format "%s" b)))))))
            (result nil))
        ;; deduplicate
        (dolist (x sorted)
          (unless (and result (equal x (car result)))
            (setq result (cons x result))))
        (nreverse result))))
  (fset 'neovm--test-set-union
    (lambda (a b)
      (funcall 'neovm--test-set-make (append a b))))
  (fset 'neovm--test-set-intersection
    (lambda (a b)
      (let ((result nil))
        (dolist (x a)
          (when (member x b) (setq result (cons x result))))
        (funcall 'neovm--test-set-make result))))
  (fset 'neovm--test-set-difference
    (lambda (a b)
      (let ((result nil))
        (dolist (x a)
          (unless (member x b) (setq result (cons x result))))
        (funcall 'neovm--test-set-make result))))
  (fset 'neovm--test-set-symmetric-diff
    (lambda (a b)
      (funcall 'neovm--test-set-union
        (funcall 'neovm--test-set-difference a b)
        (funcall 'neovm--test-set-difference b a))))
  (fset 'neovm--test-power-set
    (lambda (s)
      (if (null s)
          '(nil)
        (let* ((rest-power (funcall 'neovm--test-power-set (cdr s)))
               (with-first (mapcar (lambda (subset)
                                     (cons (car s) subset))
                                   rest-power)))
          (append rest-power with-first)))))
  (fset 'neovm--test-cartesian-product
    (lambda (a b)
      (let ((result nil))
        (dolist (x a)
          (dolist (y b)
            (setq result (cons (cons x y) result))))
        (nreverse result))))
  (unwind-protect
      (let* ((s1 (funcall 'neovm--test-set-make '(1 3 5 7 3 1)))
             (s2 (funcall 'neovm--test-set-make '(2 3 5 8)))
             (s3 (funcall 'neovm--test-set-make '(1 2 3))))
        (list
          ;; Basic set from list (dedup + sort)
          s1 s2
          ;; Union
          (funcall 'neovm--test-set-union s1 s2)
          ;; Intersection
          (funcall 'neovm--test-set-intersection s1 s2)
          ;; Difference
          (funcall 'neovm--test-set-difference s1 s2)
          (funcall 'neovm--test-set-difference s2 s1)
          ;; Symmetric difference
          (funcall 'neovm--test-set-symmetric-diff s1 s2)
          ;; Power set of {1, 2, 3}
          (let ((ps (funcall 'neovm--test-power-set '(1 2 3))))
            (list (length ps) (sort (mapcar 'length ps) '<)))
          ;; Cartesian product of {1,2} x {a,b}
          (funcall 'neovm--test-cartesian-product '(1 2) '(a b))
          ;; |A x B| = |A| * |B|
          (= (length (funcall 'neovm--test-cartesian-product s3 '(x y)))
             (* (length s3) 2))
          ;; De Morgan's: (A union B)^c relative to universe
          ;; A inter B = universe - ((universe - A) union (universe - B))
          (let ((universe (funcall 'neovm--test-set-make '(1 2 3 4 5 6 7 8))))
            (equal
              (funcall 'neovm--test-set-intersection s1 s2)
              (funcall 'neovm--test-set-difference universe
                (funcall 'neovm--test-set-union
                  (funcall 'neovm--test-set-difference universe s1)
                  (funcall 'neovm--test-set-difference universe s2)))))))
    (fmakunbound 'neovm--test-set-make)
    (fmakunbound 'neovm--test-set-union)
    (fmakunbound 'neovm--test-set-intersection)
    (fmakunbound 'neovm--test-set-difference)
    (fmakunbound 'neovm--test-set-symmetric-diff)
    (fmakunbound 'neovm--test-power-set)
    (fmakunbound 'neovm--test-cartesian-product)))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 3 5 7) (2 3 5 8) (1 2 3 5 7 8) (3 5) (1 7) (2 8) (1 2 7 8) (8 (0 1 1 1 2 2 2 3)) ((1 . a) (1 . b) (2 . a) (2 . b)) t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Interval arithmetic (add, mul, contains, intersect)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_math_interval_arithmetic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Interval: (lo . hi) where lo <= hi, integer-valued
  (fset 'neovm--test-iv-make
    (lambda (lo hi) (if (<= lo hi) (cons lo hi) (cons hi lo))))
  (fset 'neovm--test-iv-lo (lambda (iv) (car iv)))
  (fset 'neovm--test-iv-hi (lambda (iv) (cdr iv)))
  (fset 'neovm--test-iv-width
    (lambda (iv) (- (cdr iv) (car iv))))
  (fset 'neovm--test-iv-contains
    (lambda (iv x) (and (>= x (car iv)) (<= x (cdr iv)))))
  (fset 'neovm--test-iv-add
    (lambda (a b) (cons (+ (car a) (car b)) (+ (cdr a) (cdr b)))))
  (fset 'neovm--test-iv-sub
    (lambda (a b) (cons (- (car a) (cdr b)) (- (cdr a) (car b)))))
  (fset 'neovm--test-iv-mul
    (lambda (a b)
      (let* ((products (list (* (car a) (car b))
                             (* (car a) (cdr b))
                             (* (cdr a) (car b))
                             (* (cdr a) (cdr b))))
             (lo (apply 'min products))
             (hi (apply 'max products)))
        (cons lo hi))))
  (fset 'neovm--test-iv-intersect
    (lambda (a b)
      (let ((lo (max (car a) (car b)))
            (hi (min (cdr a) (cdr b))))
        (if (<= lo hi) (cons lo hi) nil))))
  (fset 'neovm--test-iv-union-hull
    (lambda (a b)
      (cons (min (car a) (car b)) (max (cdr a) (cdr b)))))
  (fset 'neovm--test-iv-subset
    (lambda (a b)
      ;; a subset of b?
      (and (>= (car a) (car b)) (<= (cdr a) (cdr b)))))
  (unwind-protect
      (let* ((i1 (funcall 'neovm--test-iv-make 1 5))     ;; [1,5]
             (i2 (funcall 'neovm--test-iv-make 3 8))     ;; [3,8]
             (i3 (funcall 'neovm--test-iv-make -2 2))    ;; [-2,2]
             (i4 (funcall 'neovm--test-iv-make 10 20))   ;; [10,20]
             (neg (funcall 'neovm--test-iv-make -5 -1))) ;; [-5,-1]
        (list
          ;; Addition: [1,5] + [3,8] = [4,13]
          (funcall 'neovm--test-iv-add i1 i2)
          ;; Subtraction: [1,5] - [3,8] = [-7,2]
          (funcall 'neovm--test-iv-sub i1 i2)
          ;; Multiplication: [1,5] * [3,8] = [3,40]
          (funcall 'neovm--test-iv-mul i1 i2)
          ;; Multiplication with negative: [-2,2] * [3,8] = [-16,16]
          (funcall 'neovm--test-iv-mul i3 i2)
          ;; Multiplication two negatives: [-5,-1] * [-2,2] = [-10,10]
          (funcall 'neovm--test-iv-mul neg i3)
          ;; Contains
          (funcall 'neovm--test-iv-contains i1 3)
          (funcall 'neovm--test-iv-contains i1 6)
          (funcall 'neovm--test-iv-contains i1 1)
          (funcall 'neovm--test-iv-contains i1 5)
          ;; Intersection: [1,5] & [3,8] = [3,5]
          (funcall 'neovm--test-iv-intersect i1 i2)
          ;; Disjoint intersection: [1,5] & [10,20] = nil
          (funcall 'neovm--test-iv-intersect i1 i4)
          ;; Union hull: [1,5] U [10,20] = [1,20]
          (funcall 'neovm--test-iv-union-hull i1 i4)
          ;; Width
          (funcall 'neovm--test-iv-width i1)
          (funcall 'neovm--test-iv-width i4)
          ;; Subset
          (funcall 'neovm--test-iv-subset (funcall 'neovm--test-iv-make 2 4) i1)
          (funcall 'neovm--test-iv-subset i1 i2)
          ;; Associativity of addition: (a+b)+c = a+(b+c)
          (equal
            (funcall 'neovm--test-iv-add
              (funcall 'neovm--test-iv-add i1 i2) i3)
            (funcall 'neovm--test-iv-add
              i1 (funcall 'neovm--test-iv-add i2 i3)))))
    (fmakunbound 'neovm--test-iv-make)
    (fmakunbound 'neovm--test-iv-lo)
    (fmakunbound 'neovm--test-iv-hi)
    (fmakunbound 'neovm--test-iv-width)
    (fmakunbound 'neovm--test-iv-contains)
    (fmakunbound 'neovm--test-iv-add)
    (fmakunbound 'neovm--test-iv-sub)
    (fmakunbound 'neovm--test-iv-mul)
    (fmakunbound 'neovm--test-iv-intersect)
    (fmakunbound 'neovm--test-iv-union-hull)
    (fmakunbound 'neovm--test-iv-subset)))"#;
    let expect = expect_test::expect![[
        r#""OK ((4 . 13) (-7 . 2) (3 . 40) (-16 . 16) (-10 . 10) t nil t t (3 . 5) nil (1 . 20) 4 10 t nil t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Matrix operations: addition, multiplication, transpose, determinant
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_math_matrix_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Matrix: vector of row vectors
  ;; Access: (aref (aref mat row) col)
  (fset 'neovm--test-mat-rows (lambda (m) (length m)))
  (fset 'neovm--test-mat-cols (lambda (m) (length (aref m 0))))
  (fset 'neovm--test-mat-get (lambda (m r c) (aref (aref m r) c)))
  (fset 'neovm--test-mat-make
    (lambda (rows cols init)
      (let ((m (make-vector rows nil)) (r 0))
        (while (< r rows)
          (aset m r (make-vector cols init))
          (setq r (1+ r)))
        m)))
  (fset 'neovm--test-mat-set
    (lambda (m r c val) (aset (aref m r) c val) m))
  (fset 'neovm--test-mat-add
    (lambda (a b)
      (let* ((rows (length a))
             (cols (length (aref a 0)))
             (result (funcall 'neovm--test-mat-make rows cols 0))
             (r 0))
        (while (< r rows)
          (let ((c 0))
            (while (< c cols)
              (aset (aref result r) c
                    (+ (aref (aref a r) c) (aref (aref b r) c)))
              (setq c (1+ c))))
          (setq r (1+ r)))
        result)))
  (fset 'neovm--test-mat-mul
    (lambda (a b)
      (let* ((ra (length a))
             (ca (length (aref a 0)))
             (cb (length (aref b 0)))
             (result (funcall 'neovm--test-mat-make ra cb 0))
             (r 0))
        (while (< r ra)
          (let ((c 0))
            (while (< c cb)
              (let ((sum 0) (k 0))
                (while (< k ca)
                  (setq sum (+ sum (* (aref (aref a r) k)
                                      (aref (aref b k) c))))
                  (setq k (1+ k)))
                (aset (aref result r) c sum))
              (setq c (1+ c))))
          (setq r (1+ r)))
        result)))
  (fset 'neovm--test-mat-transpose
    (lambda (m)
      (let* ((rows (length m))
             (cols (length (aref m 0)))
             (result (funcall 'neovm--test-mat-make cols rows 0))
             (r 0))
        (while (< r rows)
          (let ((c 0))
            (while (< c cols)
              (aset (aref result c) r (aref (aref m r) c))
              (setq c (1+ c))))
          (setq r (1+ r)))
        result)))
  (fset 'neovm--test-mat-det
    (lambda (m)
      ;; Recursive cofactor expansion for small matrices
      (let ((n (length m)))
        (cond
          ((= n 1) (aref (aref m 0) 0))
          ((= n 2) (- (* (aref (aref m 0) 0) (aref (aref m 1) 1))
                      (* (aref (aref m 0) 1) (aref (aref m 1) 0))))
          (t
           (let ((det 0) (j 0))
             (while (< j n)
               ;; build minor: remove row 0, column j
               (let* ((minor (funcall 'neovm--test-mat-make (1- n) (1- n) 0))
                      (mr 0) (r 1))
                 (while (< r n)
                   (let ((mc 0) (c 0))
                     (while (< c n)
                       (unless (= c j)
                         (aset (aref minor mr) mc (aref (aref m r) c))
                         (setq mc (1+ mc)))
                       (setq c (1+ c))))
                   (setq mr (1+ mr))
                   (setq r (1+ r)))
                 (let ((cofactor (* (if (= (% j 2) 0) 1 -1)
                                   (aref (aref m 0) j)
                                   (funcall 'neovm--test-mat-det minor))))
                   (setq det (+ det cofactor))))
               (setq j (1+ j)))
             det))))))
  (fset 'neovm--test-mat-to-list
    (lambda (m)
      (let ((result nil) (r 0))
        (while (< r (length m))
          (setq result (cons (append (aref m r) nil) result))
          (setq r (1+ r)))
        (nreverse result))))
  (unwind-protect
      (let* (;; 2x2 identity
             (id2 (vector (vector 1 0) (vector 0 1)))
             ;; 2x2 matrix
             (a (vector (vector 1 2) (vector 3 4)))
             ;; 3x3 matrix
             (b (vector (vector 1 2 3) (vector 4 5 6) (vector 7 8 9)))
             ;; 3x3 with known determinant
             (c (vector (vector 2 1 0) (vector 0 3 1) (vector 1 0 2))))
        (list
          ;; A * I = A
          (funcall 'neovm--test-mat-to-list
            (funcall 'neovm--test-mat-mul a id2))
          ;; Determinant of 2x2: 1*4-2*3 = -2
          (funcall 'neovm--test-mat-det a)
          ;; Determinant of 3x3 singular matrix = 0
          (funcall 'neovm--test-mat-det b)
          ;; Determinant of c
          (funcall 'neovm--test-mat-det c)
          ;; Transpose of a
          (funcall 'neovm--test-mat-to-list
            (funcall 'neovm--test-mat-transpose a))
          ;; (A^T)^T = A
          (equal
            (funcall 'neovm--test-mat-to-list
              (funcall 'neovm--test-mat-transpose
                (funcall 'neovm--test-mat-transpose a)))
            (funcall 'neovm--test-mat-to-list a))
          ;; Addition: A + A = 2A
          (funcall 'neovm--test-mat-to-list
            (funcall 'neovm--test-mat-add a a))))
    (fmakunbound 'neovm--test-mat-rows)
    (fmakunbound 'neovm--test-mat-cols)
    (fmakunbound 'neovm--test-mat-get)
    (fmakunbound 'neovm--test-mat-make)
    (fmakunbound 'neovm--test-mat-set)
    (fmakunbound 'neovm--test-mat-add)
    (fmakunbound 'neovm--test-mat-mul)
    (fmakunbound 'neovm--test-mat-transpose)
    (fmakunbound 'neovm--test-mat-det)
    (fmakunbound 'neovm--test-mat-to-list)))"#;
    let expect =
        expect_test::expect![[r#""OK (((1 2) (3 4)) -2 0 13 ((1 3) (2 4)) t ((2 4) (6 8)))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
