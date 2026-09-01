//! Advanced oracle parity tests for abstract algebra structures in Elisp:
//! group operations and axiom verification, cyclic group generation,
//! group homomorphism verification, ring operations in Z/nZ,
//! polynomial ring operations, and GCD via Euclidean algorithm in polynomial ring.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Group operations and verification: closure, associativity, identity, inverse
// (Dihedral group D3 - symmetries of the triangle)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_algebra_adv_dihedral_group_d3() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  ;; D3 (order 6): rotations r0,r1,r2 and reflections s0,s1,s2
  ;; Encoded as (type . index): (r . 0), (r . 1), (r . 2), (s . 0), (s . 1), (s . 2)
  ;; Composition table for D3:
  ;; ri * rj = r((i+j) mod 3)
  ;; ri * sj = s((i+j) mod 3)
  ;; si * rj = s((i-j+3) mod 3)
  ;; si * sj = r((i-j+3) mod 3)

  (fset 'neovm--d3-compose
    (lambda (a b)
      (let ((ta (car a)) (ia (cdr a))
            (tb (car b)) (ib (cdr b)))
        (cond
          ((and (eq ta 'r) (eq tb 'r))
           (cons 'r (% (+ ia ib) 3)))
          ((and (eq ta 'r) (eq tb 's))
           (cons 's (% (+ ia ib) 3)))
          ((and (eq ta 's) (eq tb 'r))
           (cons 's (% (+ (- ia ib) 3) 3)))
          ((and (eq ta 's) (eq tb 's))
           (cons 'r (% (+ (- ia ib) 3) 3)))))))

  (fset 'neovm--d3-inverse
    (lambda (a)
      (let ((ta (car a)) (ia (cdr a)))
        (if (eq ta 'r)
            (cons 'r (% (- 3 ia) 3))
          (cons 's ia)))))  ;; reflections are self-inverse

  (fset 'neovm--d3-identity (lambda () '(r . 0)))

  (fset 'neovm--d3-equal
    (lambda (a b)
      (and (eq (car a) (car b)) (= (cdr a) (cdr b)))))

  (unwind-protect
      (let* ((e (funcall 'neovm--d3-identity))
             ;; All 6 elements
             (elems (list '(r . 0) '(r . 1) '(r . 2) '(s . 0) '(s . 1) '(s . 2)))
             ;; Verify closure
             (closure-ok t)
             ;; Verify identity
             (identity-ok t)
             ;; Verify inverse
             (inverse-ok t)
             ;; Verify associativity (sample triples)
             (assoc-ok t))
        ;; Closure: product of any two is in the group
        (dolist (a elems)
          (dolist (b elems)
            (let ((c (funcall 'neovm--d3-compose a b))
                  (found nil))
              (dolist (g elems)
                (when (funcall 'neovm--d3-equal c g)
                  (setq found t)))
              (unless found (setq closure-ok nil)))))
        ;; Identity: e*a = a*e = a
        (dolist (a elems)
          (unless (and (funcall 'neovm--d3-equal
                         (funcall 'neovm--d3-compose e a) a)
                       (funcall 'neovm--d3-equal
                         (funcall 'neovm--d3-compose a e) a))
            (setq identity-ok nil)))
        ;; Inverse: a*a^{-1} = a^{-1}*a = e
        (dolist (a elems)
          (let ((a-inv (funcall 'neovm--d3-inverse a)))
            (unless (and (funcall 'neovm--d3-equal
                           (funcall 'neovm--d3-compose a a-inv) e)
                         (funcall 'neovm--d3-equal
                           (funcall 'neovm--d3-compose a-inv a) e))
              (setq inverse-ok nil))))
        ;; Associativity: (a*b)*c = a*(b*c) for all triples
        (dolist (a elems)
          (dolist (b elems)
            (dolist (c elems)
              (unless (funcall 'neovm--d3-equal
                        (funcall 'neovm--d3-compose
                          (funcall 'neovm--d3-compose a b) c)
                        (funcall 'neovm--d3-compose
                          a (funcall 'neovm--d3-compose b c)))
                (setq assoc-ok nil)))))
        ;; Non-commutativity: r1*s0 != s0*r1
        (let ((non-abelian
               (not (funcall 'neovm--d3-equal
                      (funcall 'neovm--d3-compose '(r . 1) '(s . 0))
                      (funcall 'neovm--d3-compose '(s . 0) '(r . 1))))))
          ;; Order of elements
          (let ((orders nil))
            (dolist (a elems)
              (let ((curr a) (ord 1))
                (while (not (funcall 'neovm--d3-equal curr e))
                  (setq curr (funcall 'neovm--d3-compose curr a))
                  (setq ord (1+ ord)))
                (setq orders (cons (cons a ord) orders))))
            (list closure-ok identity-ok inverse-ok assoc-ok
                  non-abelian
                  (length elems)
                  (nreverse orders)))))
    (fmakunbound 'neovm--d3-compose)
    (fmakunbound 'neovm--d3-inverse)
    (fmakunbound 'neovm--d3-identity)
    (fmakunbound 'neovm--d3-equal)))"#;
    let expect = expect_test::expect![[
        r#""OK (t t t t t 6 (((r . 0) . 1) ((r . 1) . 3) ((r . 2) . 3) ((s . 0) . 2) ((s . 1) . 2) ((s . 2) . 2)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Cyclic group generation: find generators and subgroups
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_algebra_adv_cyclic_group_generators() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (fset 'neovm--cyc-order
    (lambda (a n)
      "Order of element a in Z/nZ."
      (if (= a 0) 1
        (let ((k 1) (cur a))
          (while (/= (% cur n) 0)
            (setq cur (+ cur a))
            (setq k (1+ k)))
          k))))

  (fset 'neovm--cyc-generate
    (lambda (g n)
      "Generate the cyclic subgroup <g> in Z/nZ."
      (let ((subgroup nil) (cur 0))
        (dotimes (_ (funcall 'neovm--cyc-order g n))
          (setq subgroup (cons cur subgroup))
          (setq cur (% (+ cur g) n)))
        (sort subgroup #'<))))

  (unwind-protect
      (let ((n 12))
        ;; Find all generators of Z/12Z (elements with order 12)
        (let ((generators nil)
              (orders nil)
              (subgroups nil))
          ;; Compute order and generated subgroup for each element
          (dotimes (a n)
            (let ((ord (funcall 'neovm--cyc-order a n)))
              (setq orders (cons (cons a ord) orders))
              (when (= ord n)
                (setq generators (cons a generators)))
              ;; Only record distinct subgroups
              (let ((sg (funcall 'neovm--cyc-generate a n))
                    (already nil))
                (dolist (existing subgroups)
                  (when (equal sg (cdr existing))
                    (setq already t)))
                (unless already
                  (setq subgroups (cons (cons a sg) subgroups))))))
          ;; Generators of Z/12Z are elements coprime to 12: {1,5,7,11}
          ;; Subgroup lattice: divisors of 12 = {1,2,3,4,6,12}
          (list (sort (nreverse generators) #'<)
                (nreverse orders)
                (length subgroups)
                ;; List all distinct subgroups sorted by size
                (sort (mapcar (lambda (sg) (list (car sg) (cdr sg)))
                              (nreverse subgroups))
                      (lambda (a b) (< (length (cadr a)) (length (cadr b))))))))
    (fmakunbound 'neovm--cyc-order)
    (fmakunbound 'neovm--cyc-generate)))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 5 7 11) ((0 . 1) (1 . 12) (2 . 6) (3 . 4) (4 . 3) (5 . 12) (6 . 2) (7 . 12) (8 . 3) (9 . 4) (10 . 6) (11 . 12)) 6 ((0 (0)) (6 (0 6)) (4 (0 4 8)) (3 (0 3 6 9)) (2 (0 2 4 6 8 10)) (1 (0 1 2 3 4 5 6 7 8 9 10 11))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Group homomorphism: Z/12Z -> Z/6Z x Z/2Z (Chinese Remainder Theorem)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_algebra_adv_crt_homomorphism() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  ;; CRT isomorphism: Z/12Z -> Z/4Z x Z/3Z
  ;; phi(x) = (x mod 4, x mod 3)
  (fset 'neovm--crt-phi
    (lambda (x) (list (% x 4) (% x 3))))

  ;; Product group operation: (a1,a2) + (b1,b2) = ((a1+b1) mod 4, (a2+b2) mod 3)
  (fset 'neovm--crt-prod-op
    (lambda (a b)
      (list (% (+ (car a) (car b)) 4)
            (% (+ (cadr a) (cadr b)) 3))))

  ;; CRT inverse: given (r1,r2) find x in Z/12Z with x=r1 mod 4, x=r2 mod 3
  (fset 'neovm--crt-inverse
    (lambda (pair)
      (let ((r1 (car pair)) (r2 (cadr pair)) (found nil))
        (dotimes (x 12)
          (when (and (= (% x 4) r1) (= (% x 3) r2))
            (setq found x)))
        found)))

  (unwind-protect
      (let ((n 12))
        ;; Verify homomorphism: phi(a+b) = phi(a) + phi(b)
        (let ((homo-ok t)
              (image nil))
          (dotimes (a n)
            (dotimes (b n)
              (let ((lhs (funcall 'neovm--crt-phi (% (+ a b) n)))
                    (rhs (funcall 'neovm--crt-prod-op
                           (funcall 'neovm--crt-phi a)
                           (funcall 'neovm--crt-phi b))))
                (unless (equal lhs rhs)
                  (setq homo-ok nil)))))
          ;; Verify bijectivity (isomorphism)
          (dotimes (x n)
            (let ((img (funcall 'neovm--crt-phi x)))
              (unless (member img image)
                (setq image (cons img image)))))
          ;; Verify roundtrip: inverse(phi(x)) = x for all x
          (let ((roundtrip-ok t))
            (dotimes (x n)
              (unless (= (funcall 'neovm--crt-inverse (funcall 'neovm--crt-phi x)) x)
                (setq roundtrip-ok nil)))
            ;; Image has exactly 12 distinct elements (4*3)
            (list homo-ok
                  (= (length image) n)
                  roundtrip-ok
                  ;; Show mapping for first few elements
                  (let ((mapping nil))
                    (dotimes (x n)
                      (setq mapping (cons (cons x (funcall 'neovm--crt-phi x)) mapping)))
                    (nreverse mapping))))))
    (fmakunbound 'neovm--crt-phi)
    (fmakunbound 'neovm--crt-prod-op)
    (fmakunbound 'neovm--crt-inverse)))"#;
    let expect = expect_test::expect![[
        r#""OK (t t t ((0 0 0) (1 1 1) (2 2 2) (3 3 0) (4 0 1) (5 1 2) (6 2 0) (7 3 1) (8 0 2) (9 1 0) (10 2 1) (11 3 2)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Ring operations Z/nZ: units, nilpotents, idempotents
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_algebra_adv_ring_structure() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (fset 'neovm--ring-mul (lambda (a b n) (% (* a b) n)))
  (fset 'neovm--ring-add (lambda (a b n) (% (+ a b) n)))
  (fset 'neovm--ring-pow
    (lambda (base exp n)
      "Compute base^exp mod n."
      (let ((result 1) (b (% base n)) (e exp))
        (while (> e 0)
          (when (= (% e 2) 1)
            (setq result (% (* result b) n)))
          (setq e (/ e 2))
          (setq b (% (* b b) n)))
        result)))

  (unwind-protect
      (let ((n 12))
        ;; Find units (invertible elements)
        (let ((units nil))
          (dotimes (a n)
            (let ((inv nil))
              (dotimes (b n)
                (when (= (funcall 'neovm--ring-mul a b n) 1)
                  (setq inv b)))
              (when inv
                (setq units (cons (cons a inv) units)))))
          ;; Find nilpotents: a^k = 0 for some k > 0
          (let ((nilpotents nil))
            (dotimes (a n)
              (let ((power a) (k 1) (found nil))
                (while (and (<= k n) (not found))
                  (when (= (funcall 'neovm--ring-pow a k n) 0)
                    (setq found k))
                  (setq k (1+ k)))
                (when found
                  (setq nilpotents (cons (list a found) nilpotents)))))
            ;; Find idempotents: a^2 = a
            (let ((idempotents nil))
              (dotimes (a n)
                (when (= (funcall 'neovm--ring-mul a a n) a)
                  (setq idempotents (cons a idempotents))))
              ;; Find zero divisors
              (let ((zero-divs nil))
                (dotimes (a n)
                  (when (/= a 0)
                    (dotimes (b n)
                      (when (and (/= b 0)
                                 (= (funcall 'neovm--ring-mul a b n) 0)
                                 (not (assq a zero-divs)))
                        (setq zero-divs (cons (cons a b) zero-divs))))))
                ;; Wilson's-like: product of all units mod n
                (let ((unit-product 1))
                  (dolist (u (nreverse units))
                    (setq unit-product (funcall 'neovm--ring-mul
                                         unit-product (car u) n)))
                  (list (nreverse units)
                        (nreverse nilpotents)
                        (sort (nreverse idempotents) #'<)
                        (nreverse zero-divs)
                        unit-product)))))))
    (fmakunbound 'neovm--ring-mul)
    (fmakunbound 'neovm--ring-add)
    (fmakunbound 'neovm--ring-pow)))"#;
    let expect = expect_test::expect![[
        r#""OK (((11 . 11)) ((0 1) (6 2)) (0 1 4 9) ((2 . 6) (3 . 4) (4 . 3) (6 . 2) (8 . 3) (9 . 4) (10 . 6)) 1)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Polynomial ring: addition, multiplication, evaluation, derivative
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_algebra_adv_polynomial_ring_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  ;; Polynomials as coefficient lists: (a0 a1 a2 ...) = a0 + a1*x + a2*x^2 + ...
  (fset 'neovm--poly-trim
    (lambda (p)
      "Remove trailing zero coefficients."
      (let ((r (reverse p)))
        (while (and (cdr r) (= (car r) 0))
          (setq r (cdr r)))
        (nreverse r))))

  (fset 'neovm--poly-add
    (lambda (p q)
      (let ((result nil) (pp p) (qq q))
        (while (or pp qq)
          (setq result (cons (+ (or (car pp) 0) (or (car qq) 0)) result))
          (setq pp (cdr pp) qq (cdr qq)))
        (funcall 'neovm--poly-trim (nreverse result)))))

  (fset 'neovm--poly-scale
    (lambda (c p)
      (funcall 'neovm--poly-trim (mapcar (lambda (x) (* c x)) p))))

  (fset 'neovm--poly-mul
    (lambda (p q)
      (if (or (null p) (null q)) '(0)
        (let* ((dp (1- (length p)))
               (dq (1- (length q)))
               (result (make-list (+ dp dq 1) 0)))
          (let ((i 0))
            (dolist (a p)
              (let ((j 0))
                (dolist (b q)
                  (let ((k (+ i j)))
                    (setcar (nthcdr k result)
                            (+ (nth k result) (* a b))))
                  (setq j (1+ j))))
              (setq i (1+ i))))
          (funcall 'neovm--poly-trim result)))))

  (fset 'neovm--poly-eval
    (lambda (p x)
      "Horner's method."
      (let ((result 0))
        (dolist (c (reverse p) result)
          (setq result (+ (* result x) c))))))

  (fset 'neovm--poly-derivative
    (lambda (p)
      "Formal derivative of polynomial P."
      (if (or (null p) (null (cdr p))) '(0)
        (let ((result nil) (i 1) (rest (cdr p)))
          (while rest
            (setq result (cons (* i (car rest)) result))
            (setq i (1+ i))
            (setq rest (cdr rest)))
          (funcall 'neovm--poly-trim (nreverse result))))))

  (fset 'neovm--poly-neg
    (lambda (p)
      (mapcar (lambda (c) (- c)) p)))

  (fset 'neovm--poly-sub
    (lambda (p q)
      (funcall 'neovm--poly-add p (funcall 'neovm--poly-neg q))))

  (unwind-protect
      (let* (;; p(x) = 2 + 3x + x^2
             (p '(2 3 1))
             ;; q(x) = 1 - x + 2x^2
             (q '(1 -1 2))
             ;; r(x) = x^3 - 1
             (r '(-1 0 0 1))
             ;; Arithmetic
             (sum-pq (funcall 'neovm--poly-add p q))
             (diff-pq (funcall 'neovm--poly-sub p q))
             (prod-pq (funcall 'neovm--poly-mul p q))
             (prod-pr (funcall 'neovm--poly-mul p r))
             ;; Derivatives
             (dp (funcall 'neovm--poly-derivative p))
             (dq (funcall 'neovm--poly-derivative q))
             (dr (funcall 'neovm--poly-derivative r))
             ;; Second derivative of r
             (d2r (funcall 'neovm--poly-derivative dr))
             ;; Verify product rule sample: (p*q)' at x=2
             ;; Should equal p'(2)*q(2) + p(2)*q'(2)
             (x 2)
             (pq-deriv (funcall 'neovm--poly-derivative prod-pq))
             (lhs (funcall 'neovm--poly-eval pq-deriv x))
             (rhs (+ (* (funcall 'neovm--poly-eval dp x)
                        (funcall 'neovm--poly-eval q x))
                     (* (funcall 'neovm--poly-eval p x)
                        (funcall 'neovm--poly-eval dq x))))
             ;; Verify evaluation at multiple points
             (eval-ok t))
        (dolist (xi '(-3 -2 -1 0 1 2 3))
          (unless (= (funcall 'neovm--poly-eval prod-pq xi)
                     (* (funcall 'neovm--poly-eval p xi)
                        (funcall 'neovm--poly-eval q xi)))
            (setq eval-ok nil)))
        (list sum-pq diff-pq prod-pq prod-pr
              dp dq dr d2r
              (= lhs rhs)
              eval-ok
              ;; p(0), p(1), p(2)
              (funcall 'neovm--poly-eval p 0)
              (funcall 'neovm--poly-eval p 1)
              (funcall 'neovm--poly-eval p 2)))
    (fmakunbound 'neovm--poly-trim)
    (fmakunbound 'neovm--poly-add)
    (fmakunbound 'neovm--poly-scale)
    (fmakunbound 'neovm--poly-mul)
    (fmakunbound 'neovm--poly-eval)
    (fmakunbound 'neovm--poly-derivative)
    (fmakunbound 'neovm--poly-neg)
    (fmakunbound 'neovm--poly-sub)))"#;
    let expect = expect_test::expect![[
        r#""OK ((3 2 3) (1 4 -1) (2 1 2 5 2) (-2 -3 -1 2 3 1) (3 2) (-1 4) (0 0 3) (0 6) t t 2 6 12)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// GCD via Euclidean algorithm in polynomial ring
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_algebra_adv_polynomial_gcd() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (fset 'neovm--poly-trim
    (lambda (p)
      (let ((r (reverse p)))
        (while (and (cdr r) (= (car r) 0))
          (setq r (cdr r)))
        (nreverse r))))

  (fset 'neovm--poly-add
    (lambda (p q)
      (let ((result nil) (pp p) (qq q))
        (while (or pp qq)
          (setq result (cons (+ (or (car pp) 0) (or (car qq) 0)) result))
          (setq pp (cdr pp) qq (cdr qq)))
        (funcall 'neovm--poly-trim (nreverse result)))))

  (fset 'neovm--poly-scale
    (lambda (c p)
      (funcall 'neovm--poly-trim (mapcar (lambda (x) (* c x)) p))))

  (fset 'neovm--poly-mul
    (lambda (p q)
      (if (or (null p) (null q)) '(0)
        (let* ((dp (1- (length p)))
               (dq (1- (length q)))
               (result (make-list (+ dp dq 1) 0)))
          (let ((i 0))
            (dolist (a p)
              (let ((j 0))
                (dolist (b q)
                  (let ((k (+ i j)))
                    (setcar (nthcdr k result)
                            (+ (nth k result) (* a b))))
                  (setq j (1+ j))))
              (setq i (1+ i))))
          (funcall 'neovm--poly-trim result)))))

  (fset 'neovm--poly-degree
    (lambda (p)
      (let ((trimmed (funcall 'neovm--poly-trim p)))
        (1- (length trimmed)))))

  (fset 'neovm--poly-leading
    (lambda (p)
      (let ((trimmed (funcall 'neovm--poly-trim p)))
        (car (last trimmed)))))

  (fset 'neovm--poly-neg
    (lambda (p) (mapcar (lambda (c) (- c)) p)))

  (fset 'neovm--poly-sub
    (lambda (p q)
      (funcall 'neovm--poly-add p (funcall 'neovm--poly-neg q))))

  (fset 'neovm--poly-shift
    (lambda (p n)
      "Multiply polynomial P by x^n."
      (let ((result p))
        (dotimes (_ n result)
          (setq result (cons 0 result))))))

  ;; Polynomial pseudo-division (for integer coefficients)
  ;; Returns (quotient . remainder) using pseudo-division
  (fset 'neovm--poly-divmod
    (lambda (a b)
      "Pseudo-divide A by B. Returns (quotient . remainder)."
      (let* ((a (funcall 'neovm--poly-trim a))
             (b (funcall 'neovm--poly-trim b))
             (db (funcall 'neovm--poly-degree b))
             (lb (funcall 'neovm--poly-leading b))
             (q (list 0))
             (r a))
        (while (and (>= (funcall 'neovm--poly-degree r) db)
                    (not (equal (funcall 'neovm--poly-trim r) '(0))))
          (let* ((dr (funcall 'neovm--poly-degree r))
                 (lr (funcall 'neovm--poly-leading r))
                 (shift (- dr db))
                 ;; monomial: lr * x^shift
                 (mono (funcall 'neovm--poly-shift (list lr) shift)))
            ;; q = q * lb + mono
            (setq q (funcall 'neovm--poly-add
                      (funcall 'neovm--poly-scale lb q) mono))
            ;; r = r * lb - mono * b
            (setq r (funcall 'neovm--poly-sub
                      (funcall 'neovm--poly-scale lb r)
                      (funcall 'neovm--poly-mul mono b)))))
        (cons (funcall 'neovm--poly-trim q)
              (funcall 'neovm--poly-trim r)))))

  (fset 'neovm--poly-gcd
    (lambda (a b)
      "Compute GCD of polynomials A and B via Euclidean algorithm."
      (let ((aa (funcall 'neovm--poly-trim a))
            (bb (funcall 'neovm--poly-trim b)))
        (while (not (equal bb '(0)))
          (let ((rem (cdr (funcall 'neovm--poly-divmod aa bb))))
            (setq aa bb)
            (setq bb (funcall 'neovm--poly-trim rem))))
        ;; Make monic (divide by leading coefficient)
        (let ((lc (funcall 'neovm--poly-leading aa)))
          (if (and lc (/= lc 0))
              (funcall 'neovm--poly-trim
                (mapcar (lambda (c) (/ c lc)) aa))
            aa)))))

  (fset 'neovm--poly-eval
    (lambda (p x)
      (let ((result 0))
        (dolist (c (reverse p) result)
          (setq result (+ (* result x) c))))))

  (unwind-protect
      (let* (;; p(x) = (x-1)(x-2) = x^2 - 3x + 2
             (p '(2 -3 1))
             ;; q(x) = (x-2)(x-3) = x^2 - 5x + 6
             (q '(6 -5 1))
             ;; gcd should be (x-2) = -2 + x, normalized to (1 . (-2 1)) monic
             (g (funcall 'neovm--poly-gcd p q))
             ;; Verify: gcd divides both p and q
             (rem-p (cdr (funcall 'neovm--poly-divmod p g)))
             (rem-q (cdr (funcall 'neovm--poly-divmod q g)))
             ;; Another example: gcd of (x^3 - 1) and (x^2 - 1)
             ;; (x^3 - 1) = (x-1)(x^2+x+1), (x^2-1) = (x-1)(x+1)
             ;; gcd = (x-1)
             (r1 '(-1 0 0 1))  ;; x^3 - 1
             (r2 '(-1 0 1))    ;; x^2 - 1
             (g2 (funcall 'neovm--poly-gcd r1 r2))
             ;; Coprime polynomials: x^2+1 and x+1
             (s1 '(1 0 1))     ;; x^2 + 1
             (s2 '(1 1))       ;; x + 1
             (g3 (funcall 'neovm--poly-gcd s1 s2)))
        ;; Verify at evaluation points
        (let ((g-divides-ok t))
          (dolist (x '(-3 -2 -1 0 1 2 3 4 5))
            ;; If g(x)=0 then both p(x)=0 and q(x)=0
            (when (= (funcall 'neovm--poly-eval g x) 0)
              (unless (and (= (funcall 'neovm--poly-eval p x) 0)
                           (= (funcall 'neovm--poly-eval q x) 0))
                (setq g-divides-ok nil))))
          (list g
                (equal rem-p '(0))
                (equal rem-q '(0))
                g2
                g3
                g-divides-ok
                ;; gcd(p, p) should be p (monic)
                (funcall 'neovm--poly-gcd p p))))
    (fmakunbound 'neovm--poly-trim)
    (fmakunbound 'neovm--poly-add)
    (fmakunbound 'neovm--poly-scale)
    (fmakunbound 'neovm--poly-mul)
    (fmakunbound 'neovm--poly-degree)
    (fmakunbound 'neovm--poly-leading)
    (fmakunbound 'neovm--poly-neg)
    (fmakunbound 'neovm--poly-sub)
    (fmakunbound 'neovm--poly-shift)
    (fmakunbound 'neovm--poly-divmod)
    (fmakunbound 'neovm--poly-gcd)
    (fmakunbound 'neovm--poly-eval)))"#;
    let expect = expect_test::expect![[r#""OK ((-2 1) t t (-1 1) (1) t (2 -3 1))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
