//! Complex oracle parity tests for abstract algebra concepts in Elisp:
//! cyclic groups, permutation groups, group axiom verification, cosets,
//! Lagrange's theorem verification, ring operations, polynomial rings.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Cyclic group Z/nZ: elements, operation, inverse, identity, order
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_algebra_cyclic_group() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  ;; Z/7Z: the cyclic group of integers mod 7
  (fset 'neovm--alg-zn-op
    (lambda (a b n) (% (+ a b) n)))
  (fset 'neovm--alg-zn-inv
    (lambda (a n) (% (- n a) n)))
  (fset 'neovm--alg-zn-identity
    (lambda (n) 0))
  (fset 'neovm--alg-zn-order
    (lambda (a n)
      "Compute the order of element a in Z/nZ (smallest k>0 s.t. k*a = 0 mod n)."
      (if (= a 0) 1
        (let ((k 1) (cur a))
          (while (/= cur 0)
            (setq cur (% (+ cur a) n))
            (setq k (1+ k)))
          k))))

  (unwind-protect
      (let ((n 7))
        ;; Verify identity: a + 0 = a for all a
        (let ((identity-check t))
          (dotimes (a n)
            (unless (= (funcall 'neovm--alg-zn-op a (funcall 'neovm--alg-zn-identity n) n) a)
              (setq identity-check nil)))
          ;; Verify inverse: a + (-a) = 0 for all a
          (let ((inverse-check t))
            (dotimes (a n)
              (unless (= (funcall 'neovm--alg-zn-op a (funcall 'neovm--alg-zn-inv a n) n) 0)
                (setq inverse-check nil)))
            ;; Verify associativity: (a+b)+c = a+(b+c) for sample triples
            (let ((assoc-check t))
              (dolist (triple '((1 2 3) (3 5 6) (0 4 2) (6 6 6)))
                (let ((a (nth 0 triple)) (b (nth 1 triple)) (c (nth 2 triple)))
                  (unless (= (funcall 'neovm--alg-zn-op
                               (funcall 'neovm--alg-zn-op a b n) c n)
                             (funcall 'neovm--alg-zn-op
                               a (funcall 'neovm--alg-zn-op b c n) n))
                    (setq assoc-check nil))))
              ;; Compute orders of all elements
              (let ((orders nil))
                (dotimes (a n)
                  (setq orders (cons (cons a (funcall 'neovm--alg-zn-order a n)) orders)))
                ;; In Z/7Z (prime), every non-zero element has order 7
                (list identity-check inverse-check assoc-check
                      (nreverse orders)))))))
    (fmakunbound 'neovm--alg-zn-op)
    (fmakunbound 'neovm--alg-zn-inv)
    (fmakunbound 'neovm--alg-zn-identity)
    (fmakunbound 'neovm--alg-zn-order)))"#;
    let expect = expect_test::expect![[
        r#""OK (t t t ((0 . 1) (1 . 7) (2 . 7) (3 . 7) (4 . 7) (5 . 7) (6 . 7)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Permutation group S3: compose, inverse, verify group axioms
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_algebra_permutation_group_s3() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  ;; Permutations as vectors: [a b c] means 0->a, 1->b, 2->c
  (fset 'neovm--alg-perm-compose
    (lambda (p q)
      "Compose permutations: (p . q)(x) = p(q(x))."
      (let ((n (length p))
            (result nil))
        (dotimes (i n)
          (setq result (cons (aref p (aref q i)) result)))
        (vconcat (nreverse result)))))

  (fset 'neovm--alg-perm-inverse
    (lambda (p)
      "Compute the inverse permutation."
      (let* ((n (length p))
             (inv (make-vector n 0)))
        (dotimes (i n)
          (aset inv (aref p i) i))
        inv)))

  (fset 'neovm--alg-perm-identity
    (lambda (n) (let ((v (make-vector n 0))) (dotimes (i n) (aset v i i)) v)))

  (fset 'neovm--alg-perm-equal
    (lambda (p q) (equal p q)))

  (unwind-protect
      (let* ((id (funcall 'neovm--alg-perm-identity 3))
             ;; All 6 elements of S3
             (e  [0 1 2])    ;; identity
             (r1 [1 2 0])    ;; rotation by 1 (123)
             (r2 [2 0 1])    ;; rotation by 2 (132)
             (s1 [1 0 2])    ;; swap 0,1
             (s2 [0 2 1])    ;; swap 1,2
             (s3 [2 1 0])    ;; swap 0,2
             (all (list e r1 r2 s1 s2 s3)))
        ;; Closure: compose any two elements, result must be in the group
        (let ((closure-ok t))
          (dolist (a all)
            (dolist (b all)
              (let ((c (funcall 'neovm--alg-perm-compose a b))
                    (found nil))
                (dolist (g all)
                  (when (funcall 'neovm--alg-perm-equal c g)
                    (setq found t)))
                (unless found (setq closure-ok nil)))))
          ;; Identity check
          (let ((id-ok t))
            (dolist (a all)
              (unless (and (funcall 'neovm--alg-perm-equal
                            (funcall 'neovm--alg-perm-compose a e) a)
                          (funcall 'neovm--alg-perm-equal
                            (funcall 'neovm--alg-perm-compose e a) a))
                (setq id-ok nil)))
            ;; Inverse check
            (let ((inv-ok t))
              (dolist (a all)
                (let ((a-inv (funcall 'neovm--alg-perm-inverse a)))
                  (unless (and (funcall 'neovm--alg-perm-equal
                                (funcall 'neovm--alg-perm-compose a a-inv) e)
                              (funcall 'neovm--alg-perm-equal
                                (funcall 'neovm--alg-perm-compose a-inv a) e))
                    (setq inv-ok nil))))
              ;; r1 composed with r1 = r2, r1^3 = id
              (let ((r1r1 (funcall 'neovm--alg-perm-compose r1 r1))
                    (r1r1r1 (funcall 'neovm--alg-perm-compose
                              (funcall 'neovm--alg-perm-compose r1 r1) r1)))
                (list closure-ok id-ok inv-ok
                      (funcall 'neovm--alg-perm-equal r1r1 r2)
                      (funcall 'neovm--alg-perm-equal r1r1r1 e)
                      ;; s1^2 = identity (involution)
                      (funcall 'neovm--alg-perm-equal
                        (funcall 'neovm--alg-perm-compose s1 s1) e)
                      ;; |S3| = 6
                      (length all)))))))
    (fmakunbound 'neovm--alg-perm-compose)
    (fmakunbound 'neovm--alg-perm-inverse)
    (fmakunbound 'neovm--alg-perm-identity)
    (fmakunbound 'neovm--alg-perm-equal)))"#;
    let expect = expect_test::expect![[r#""OK (t t t t t t 6)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Cosets and Lagrange's theorem: |H| divides |G|
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_algebra_cosets_lagrange() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  ;; Work in Z/12Z under addition
  (fset 'neovm--alg-z12-op (lambda (a b) (% (+ a b) 12)))

  (fset 'neovm--alg-left-coset
    (lambda (g subgroup)
      "Compute left coset g + H = {g + h : h in H} in Z/12Z."
      (let ((coset nil))
        (dolist (h subgroup)
          (let ((elem (funcall 'neovm--alg-z12-op g h)))
            (unless (member elem coset)
              (setq coset (cons elem coset)))))
        (sort coset #'<))))

  (fset 'neovm--alg-all-cosets
    (lambda (subgroup)
      "Compute all distinct left cosets of SUBGROUP in Z/12Z."
      (let ((cosets nil)
            (seen nil))
        (dotimes (g 12)
          (unless (member g seen)
            (let ((coset (funcall 'neovm--alg-left-coset g subgroup)))
              (setq cosets (cons coset cosets))
              ;; Mark all elements of this coset as seen
              (dolist (elem coset)
                (unless (member elem seen)
                  (setq seen (cons elem seen)))))))
        (nreverse cosets))))

  (unwind-protect
      (let* (;; H = {0, 3, 6, 9} — subgroup of order 4
             (H '(0 3 6 9))
             (cosets (funcall 'neovm--alg-all-cosets H))
             (num-cosets (length cosets))
             ;; Lagrange: |G| = |H| * number of cosets
             ;; 12 = 4 * 3
             (lagrange-ok (= 12 (* (length H) num-cosets)))
             ;; Cosets should be disjoint
             (disjoint-ok t))
        (let ((i 0))
          (while (< i num-cosets)
            (let ((j (1+ i)))
              (while (< j num-cosets)
                (let ((ci (nth i cosets))
                      (cj (nth j cosets)))
                  (dolist (elem ci)
                    (when (member elem cj)
                      (setq disjoint-ok nil))))
                (setq j (1+ j))))
            (setq i (1+ i))))
        ;; Cosets should cover all of Z/12Z
        (let ((all-elems nil))
          (dolist (coset cosets)
            (dolist (elem coset)
              (setq all-elems (cons elem all-elems))))
          (let ((covers-ok (= (length (delete-dups all-elems)) 12)))
            ;; Also test H2 = {0, 4, 8} — subgroup of order 3
            (let* ((H2 '(0 4 8))
                   (cosets2 (funcall 'neovm--alg-all-cosets H2))
                   (lagrange2-ok (= 12 (* (length H2) (length cosets2)))))
              (list lagrange-ok disjoint-ok covers-ok
                    num-cosets
                    cosets
                    lagrange2-ok
                    (length cosets2))))))
    (fmakunbound 'neovm--alg-z12-op)
    (fmakunbound 'neovm--alg-left-coset)
    (fmakunbound 'neovm--alg-all-cosets)))"#;
    let expect = expect_test::expect![[r#""OK (t t t 3 ((0 3 6 9) (1 4 7 10) (2 5 8 11)) t 4)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Ring Z/nZ under addition and multiplication
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_algebra_ring_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (fset 'neovm--alg-ring-add (lambda (a b n) (% (+ a b) n)))
  (fset 'neovm--alg-ring-mul (lambda (a b n) (% (* a b) n)))
  (fset 'neovm--alg-ring-neg (lambda (a n) (% (- n a) n)))

  (unwind-protect
      (let ((n 6))
        ;; Verify ring axioms for Z/6Z
        ;; 1. Additive group axioms (identity=0, inverse, assoc, commutative)
        (let ((add-comm t) (add-assoc t) (mul-assoc t)
              (distrib-left t) (distrib-right t) (mul-comm t))
          ;; Check commutativity of addition and multiplication
          (dotimes (a n)
            (dotimes (b n)
              (unless (= (funcall 'neovm--alg-ring-add a b n)
                         (funcall 'neovm--alg-ring-add b a n))
                (setq add-comm nil))
              (unless (= (funcall 'neovm--alg-ring-mul a b n)
                         (funcall 'neovm--alg-ring-mul b a n))
                (setq mul-comm nil))))
          ;; Check associativity (sample)
          (dolist (triple '((1 2 3) (4 5 2) (3 3 3) (0 5 1)))
            (let ((a (nth 0 triple)) (b (nth 1 triple)) (c (nth 2 triple)))
              (unless (= (funcall 'neovm--alg-ring-add
                           (funcall 'neovm--alg-ring-add a b n) c n)
                         (funcall 'neovm--alg-ring-add
                           a (funcall 'neovm--alg-ring-add b c n) n))
                (setq add-assoc nil))
              (unless (= (funcall 'neovm--alg-ring-mul
                           (funcall 'neovm--alg-ring-mul a b n) c n)
                         (funcall 'neovm--alg-ring-mul
                           a (funcall 'neovm--alg-ring-mul b c n) n))
                (setq mul-assoc nil))
              ;; Left distributivity: a*(b+c) = a*b + a*c
              (unless (= (funcall 'neovm--alg-ring-mul a
                           (funcall 'neovm--alg-ring-add b c n) n)
                         (funcall 'neovm--alg-ring-add
                           (funcall 'neovm--alg-ring-mul a b n)
                           (funcall 'neovm--alg-ring-mul a c n) n))
                (setq distrib-left nil))
              ;; Right distributivity: (a+b)*c = a*c + b*c
              (unless (= (funcall 'neovm--alg-ring-mul
                           (funcall 'neovm--alg-ring-add a b n) c n)
                         (funcall 'neovm--alg-ring-add
                           (funcall 'neovm--alg-ring-mul a c n)
                           (funcall 'neovm--alg-ring-mul b c n) n))
                (setq distrib-right nil))))
          ;; Find zero divisors: a*b=0 with a,b != 0
          (let ((zero-divisors nil))
            (dotimes (a n)
              (dotimes (b n)
                (when (and (/= a 0) (/= b 0)
                           (= (funcall 'neovm--alg-ring-mul a b n) 0))
                  (setq zero-divisors (cons (cons a b) zero-divisors)))))
            ;; Find units: elements with multiplicative inverse
            (let ((units nil))
              (dotimes (a n)
                (let ((has-inv nil))
                  (dotimes (b n)
                    (when (= (funcall 'neovm--alg-ring-mul a b n) 1)
                      (setq has-inv b)))
                  (when has-inv
                    (setq units (cons (cons a has-inv) units)))))
              (list add-comm mul-comm add-assoc mul-assoc
                    distrib-left distrib-right
                    (nreverse zero-divisors)
                    (nreverse units))))))
    (fmakunbound 'neovm--alg-ring-add)
    (fmakunbound 'neovm--alg-ring-mul)
    (fmakunbound 'neovm--alg-ring-neg)))"#;
    let expect = expect_test::expect![[
        r#""OK (t t t t t t ((2 . 3) (3 . 2) (3 . 4) (4 . 3)) ((1 . 1) (5 . 5)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Polynomial ring: add, multiply, evaluate polynomials over Z
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_algebra_polynomial_ring() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  ;; Polynomial represented as list of coefficients: (a0 a1 a2 ...) = a0 + a1*x + a2*x^2 + ...
  (fset 'neovm--alg-poly-add
    (lambda (p q)
      "Add two polynomials."
      (let ((result nil)
            (pp p) (qq q))
        (while (or pp qq)
          (let ((a (or (car pp) 0))
                (b (or (car qq) 0)))
            (setq result (cons (+ a b) result))
            (setq pp (cdr pp) qq (cdr qq))))
        ;; Remove trailing zeros
        (let ((r (nreverse result)))
          (while (and (cdr r) (= (car (last r)) 0))
            (setq r (butlast r)))
          r))))

  (fset 'neovm--alg-poly-scale
    (lambda (c p)
      "Multiply polynomial P by scalar C."
      (mapcar (lambda (coeff) (* c coeff)) p)))

  (fset 'neovm--alg-poly-mul
    (lambda (p q)
      "Multiply two polynomials via schoolbook algorithm."
      (if (or (null p) (null q)) '(0)
        (let* ((deg-p (1- (length p)))
               (deg-q (1- (length q)))
               (deg-r (+ deg-p deg-q))
               (result (make-list (1+ deg-r) 0)))
          (let ((i 0))
            (dolist (a p)
              (let ((j 0))
                (dolist (b q)
                  (let ((k (+ i j)))
                    (setcar (nthcdr k result)
                            (+ (nth k result) (* a b))))
                  (setq j (1+ j))))
              (setq i (1+ i))))
          ;; Remove trailing zeros
          (while (and (cdr result) (= (car (last result)) 0))
            (setq result (butlast result)))
          result))))

  (fset 'neovm--alg-poly-eval
    (lambda (p x)
      "Evaluate polynomial P at X using Horner's method."
      (let ((coeffs (reverse p))
            (result 0))
        (dolist (c coeffs)
          (setq result (+ (* result x) c)))
        result)))

  (fset 'neovm--alg-poly-degree
    (lambda (p)
      (1- (length p))))

  (unwind-protect
      (let* (;; p(x) = 1 + 2x + 3x^2
             (p '(1 2 3))
             ;; q(x) = 4 + 5x
             (q '(4 5))
             ;; r(x) = -1 + 0x + 1x^2  (x^2 - 1)
             (r '(-1 0 1))
             ;; Addition: p + q = 5 + 7x + 3x^2
             (sum-pq (funcall 'neovm--alg-poly-add p q))
             ;; Multiplication: p * q = 4 + 13x + 22x^2 + 15x^3
             (prod-pq (funcall 'neovm--alg-poly-mul p q))
             ;; (x^2 - 1) * (x^2 - 1) = 1 - 2x^2 + x^4
             (r-squared (funcall 'neovm--alg-poly-mul r r))
             ;; Evaluate p at x=2: 1 + 4 + 12 = 17
             (p-at-2 (funcall 'neovm--alg-poly-eval p 2))
             ;; Evaluate q at x=3: 4 + 15 = 19
             (q-at-3 (funcall 'neovm--alg-poly-eval q 3))
             ;; Verify distributivity: p*(q+r) = p*q + p*r
             (lhs (funcall 'neovm--alg-poly-mul p (funcall 'neovm--alg-poly-add q r)))
             (rhs (funcall 'neovm--alg-poly-add
                    (funcall 'neovm--alg-poly-mul p q)
                    (funcall 'neovm--alg-poly-mul p r)))
             (distrib-ok (equal lhs rhs))
             ;; Verify at multiple points
             (eval-check t))
        (dolist (x '(-2 -1 0 1 2 3))
          (unless (= (funcall 'neovm--alg-poly-eval prod-pq x)
                     (* (funcall 'neovm--alg-poly-eval p x)
                        (funcall 'neovm--alg-poly-eval q x)))
            (setq eval-check nil)))
        (list sum-pq prod-pq r-squared
              p-at-2 q-at-3
              distrib-ok eval-check
              (funcall 'neovm--alg-poly-degree prod-pq)))
    (fmakunbound 'neovm--alg-poly-add)
    (fmakunbound 'neovm--alg-poly-scale)
    (fmakunbound 'neovm--alg-poly-mul)
    (fmakunbound 'neovm--alg-poly-eval)
    (fmakunbound 'neovm--alg-poly-degree)))"#;
    let expect = expect_test::expect![[r#""OK ((5 7 3) (4 13 22 15) (1 0 -2 0 1) 17 19 t t 3)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Group homomorphism: verify kernel and image properties
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_algebra_group_homomorphism() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  ;; Homomorphism phi: Z/12Z -> Z/4Z defined by phi(x) = x mod 4
  (fset 'neovm--alg-phi (lambda (x) (% x 4)))

  (unwind-protect
      (let ((n 12) (m 4))
        ;; Verify homomorphism property: phi(a+b) = phi(a)+phi(b) (mod m)
        (let ((homo-ok t))
          (dotimes (a n)
            (dotimes (b n)
              (unless (= (funcall 'neovm--alg-phi (% (+ a b) n))
                         (% (+ (funcall 'neovm--alg-phi a)
                               (funcall 'neovm--alg-phi b))
                            m))
                (setq homo-ok nil))))
          ;; Compute kernel: {x in Z/12Z : phi(x) = 0}
          (let ((kernel nil))
            (dotimes (x n)
              (when (= (funcall 'neovm--alg-phi x) 0)
                (setq kernel (cons x kernel))))
            (setq kernel (nreverse kernel))
            ;; Compute image: {phi(x) : x in Z/12Z}
            (let ((image nil))
              (dotimes (x n)
                (let ((y (funcall 'neovm--alg-phi x)))
                  (unless (member y image)
                    (setq image (cons y image)))))
              (setq image (sort image #'<))
              ;; First isomorphism theorem: |G|/|ker| = |im|
              (let ((first-iso-ok (= (/ n (length kernel)) (length image)))
                    ;; Verify kernel is a subgroup (closed under addition mod n)
                    (kernel-closed t))
                (dolist (a kernel)
                  (dolist (b kernel)
                    (unless (member (% (+ a b) n) kernel)
                      (setq kernel-closed nil))))
                (list homo-ok
                      kernel
                      image
                      first-iso-ok
                      kernel-closed
                      (length kernel)
                      (length image)))))))
    (fmakunbound 'neovm--alg-phi)))"#;
    let expect = expect_test::expect![[r#""OK (t (0 4 8) (0 1 2 3) t t 3 4)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Modular exponentiation and Euler's theorem
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_algebra_modular_exponentiation_euler() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (fset 'neovm--alg-mod-pow
    (lambda (base exp mod)
      "Compute (base^exp) mod mod via repeated squaring."
      (let ((result 1)
            (b (% base mod))
            (e exp))
        (while (> e 0)
          (when (= (% e 2) 1)
            (setq result (% (* result b) mod)))
          (setq e (/ e 2))
          (setq b (% (* b b) mod)))
        result)))

  (fset 'neovm--alg-gcd
    (lambda (a b)
      (let ((a (abs a)) (b (abs b)))
        (while (/= b 0)
          (let ((tmp b))
            (setq b (% a b))
            (setq a tmp)))
        a)))

  (fset 'neovm--alg-euler-totient
    (lambda (n)
      "Compute Euler's totient phi(n) = count of integers 1..n-1 coprime to n."
      (let ((count 0))
        (dotimes (i (1- n))
          (when (= (funcall 'neovm--alg-gcd (1+ i) n) 1)
            (setq count (1+ count))))
        count)))

  (unwind-protect
      (let ((n 15))  ;; n = 15 = 3 * 5
        (let ((phi-n (funcall 'neovm--alg-euler-totient n)))
          ;; Euler's theorem: a^phi(n) = 1 (mod n) for gcd(a,n) = 1
          (let ((euler-ok t)
                (coprimes nil))
            (dotimes (i (1- n))
              (let ((a (1+ i)))
                (when (= (funcall 'neovm--alg-gcd a n) 1)
                  (setq coprimes (cons a coprimes))
                  (unless (= (funcall 'neovm--alg-mod-pow a phi-n n) 1)
                    (setq euler-ok nil)))))
            ;; Fermat's little theorem special case: a^(p-1) = 1 (mod p) for prime p
            (let ((p 7)
                  (fermat-ok t))
              (dotimes (i (1- p))
                (let ((a (1+ i)))
                  (unless (= (funcall 'neovm--alg-mod-pow a (1- p) p) 1)
                    (setq fermat-ok nil))))
              ;; Some specific computations
              (list phi-n
                    euler-ok
                    fermat-ok
                    (nreverse coprimes)
                    (length coprimes)
                    ;; 2^10 mod 15 = 1024 mod 15 = 4
                    (funcall 'neovm--alg-mod-pow 2 10 n)
                    ;; 7^3 mod 15 = 343 mod 15 = 13
                    (funcall 'neovm--alg-mod-pow 7 3 n))))))
    (fmakunbound 'neovm--alg-mod-pow)
    (fmakunbound 'neovm--alg-gcd)
    (fmakunbound 'neovm--alg-euler-totient)))"#;
    let expect = expect_test::expect![[r#""OK (8 t t (1 2 4 7 8 11 13 14) 1 4 13)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
