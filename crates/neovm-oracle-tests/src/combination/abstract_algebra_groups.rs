//! Oracle parity tests for abstract algebra - group theory:
//! group operation tables, cyclic group generation, subgroup testing via
//! Lagrange's theorem, coset computation, group homomorphism verification,
//! symmetric group (permutation composition), dihedral group construction,
//! and group center computation.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Cayley table (operation table) for Z/5Z and verification of group axioms
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_algebra_groups_cayley_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (fset 'neovm--grp-cayley-table
    (lambda (n op)
      "Build the Cayley table for group Z/nZ under operation OP.
       Returns a list of rows, each row is a list of results."
      (let ((table nil))
        (dotimes (i n)
          (let ((row nil))
            (dotimes (j n)
              (setq row (cons (funcall op i j n) row)))
            (setq table (cons (nreverse row) table))))
        (nreverse table))))

  (fset 'neovm--grp-verify-closure
    (lambda (table n)
      "Verify every entry in the Cayley table is in {0, ..., n-1}."
      (let ((ok t))
        (dolist (row table)
          (dolist (entry row)
            (when (or (< entry 0) (>= entry n))
              (setq ok nil))))
        ok)))

  (fset 'neovm--grp-z-add
    (lambda (a b n) (% (+ a b) n)))

  (fset 'neovm--grp-z-mul
    (lambda (a b n) (% (* a b) n)))

  (unwind-protect
      (let* ((add-table (funcall 'neovm--grp-cayley-table 5 'neovm--grp-z-add))
             (mul-table (funcall 'neovm--grp-cayley-table 5 'neovm--grp-z-mul))
             ;; Additive table should have closure
             (add-closed (funcall 'neovm--grp-verify-closure add-table 5))
             (mul-closed (funcall 'neovm--grp-verify-closure mul-table 5))
             ;; Verify each row of additive table is a permutation of {0..4}
             (add-permutation-ok t))
        (dolist (row add-table)
          (let ((sorted (sort (copy-sequence row) #'<)))
            (unless (equal sorted '(0 1 2 3 4))
              (setq add-permutation-ok nil))))
        (list
          add-table
          add-closed
          mul-closed
          add-permutation-ok
          ;; Diagonal of additive table: a+a mod 5
          (let ((diag nil) (i 0))
            (dolist (row add-table)
              (setq diag (cons (nth i row) diag))
              (setq i (1+ i)))
            (nreverse diag))
          ;; Multiplicative table (not a group since 0 has no inverse)
          mul-table))
    (fmakunbound 'neovm--grp-cayley-table)
    (fmakunbound 'neovm--grp-verify-closure)
    (fmakunbound 'neovm--grp-z-add)
    (fmakunbound 'neovm--grp-z-mul)))"#;
    let expect = expect_test::expect![[
        r#""OK (((0 1 2 3 4) (1 2 3 4 0) (2 3 4 0 1) (3 4 0 1 2) (4 0 1 2 3)) t t t (0 2 4 1 3) ((0 0 0 0 0) (0 1 2 3 4) (0 2 4 1 3) (0 3 1 4 2) (0 4 3 2 1)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Cyclic group generation: <g> = {g^0, g^1, ..., g^(ord-1)}
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_algebra_groups_cyclic_generation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (fset 'neovm--grp-generate-cyclic
    (lambda (generator n)
      "Generate the cyclic subgroup <generator> in Z/nZ under addition."
      (let ((elements nil)
            (current 0))
        (dotimes (_ n)
          (unless (member current elements)
            (setq elements (cons current elements)))
          (setq current (% (+ current generator) n)))
        (sort elements #'<))))

  (fset 'neovm--grp-element-order
    (lambda (g n)
      "Compute the order of g in Z/nZ."
      (if (= g 0) 1
        (let ((current g) (order 1))
          (while (/= current 0)
            (setq current (% (+ current g) n))
            (setq order (1+ order)))
          order))))

  (unwind-protect
      (let ((n 12))
        (list
          ;; <1> generates all of Z/12Z
          (funcall 'neovm--grp-generate-cyclic 1 n)
          ;; <2> = {0, 2, 4, 6, 8, 10}
          (funcall 'neovm--grp-generate-cyclic 2 n)
          ;; <3> = {0, 3, 6, 9}
          (funcall 'neovm--grp-generate-cyclic 3 n)
          ;; <4> = {0, 4, 8}
          (funcall 'neovm--grp-generate-cyclic 4 n)
          ;; <6> = {0, 6}
          (funcall 'neovm--grp-generate-cyclic 6 n)
          ;; Orders: ord(1)=12, ord(2)=6, ord(3)=4, ord(4)=3, ord(6)=2
          (mapcar (lambda (g) (cons g (funcall 'neovm--grp-element-order g n)))
                  '(0 1 2 3 4 5 6 7 8 9 10 11))
          ;; Generators of Z/12Z: elements with order 12 (coprime to 12)
          (let ((generators nil))
            (dotimes (g n)
              (when (= (funcall 'neovm--grp-element-order g n) n)
                (setq generators (cons g generators))))
            (nreverse generators))
          ;; Verify: |<g>| = ord(g) for all elements
          (let ((check t))
            (dotimes (g n)
              (let ((subgroup (funcall 'neovm--grp-generate-cyclic g n))
                    (order (funcall 'neovm--grp-element-order g n)))
                (unless (= (length subgroup) order)
                  (setq check nil))))
            check)))
    (fmakunbound 'neovm--grp-generate-cyclic)
    (fmakunbound 'neovm--grp-element-order)))"#;
    let expect = expect_test::expect![[
        r#""OK ((0 1 2 3 4 5 6 7 8 9 10 11) (0 2 4 6 8 10) (0 3 6 9) (0 4 8) (0 6) ((0 . 1) (1 . 12) (2 . 6) (3 . 4) (4 . 3) (5 . 12) (6 . 2) (7 . 12) (8 . 3) (9 . 4) (10 . 6) (11 . 12)) (1 5 7 11) t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Subgroup testing and Lagrange's theorem in Z/nZ
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_algebra_groups_subgroup_lagrange() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (fset 'neovm--grp-is-subgroup
    (lambda (H n)
      "Test if H is a subgroup of Z/nZ.
       Must contain identity(0), be closed under +, and have inverses."
      (and
        ;; Contains identity
        (member 0 H)
        ;; Closed under addition
        (let ((closed t))
          (dolist (a H)
            (dolist (b H)
              (unless (member (% (+ a b) n) H)
                (setq closed nil))))
          closed)
        ;; Has inverses
        (let ((inverses t))
          (dolist (a H)
            (unless (member (% (- n a) n) H)
              (setq inverses nil)))
          inverses))))

  (fset 'neovm--grp-all-cyclic-subgroups
    (lambda (n)
      "Find all distinct cyclic subgroups of Z/nZ."
      (let ((subgroups nil))
        (dotimes (g n)
          (let ((H nil) (current 0))
            (dotimes (_ n)
              (unless (member current H)
                (setq H (cons current H)))
              (setq current (% (+ current g) n)))
            (setq H (sort H #'<))
            (unless (member H subgroups)
              (setq subgroups (cons H subgroups)))))
        ;; Sort by size then lexicographically
        (sort subgroups (lambda (a b)
                          (if (= (length a) (length b))
                              (let ((result nil) (aa a) (bb b))
                                (while (and aa bb (null result))
                                  (cond ((< (car aa) (car bb)) (setq result t))
                                        ((> (car aa) (car bb)) (setq result nil) (setq aa nil))
                                        (t (setq aa (cdr aa)) (setq bb (cdr bb)))))
                                result)
                            (< (length a) (length b))))))))

  (unwind-protect
      (let* ((n 12)
             (subgroups (funcall 'neovm--grp-all-cyclic-subgroups n))
             ;; Verify each is actually a subgroup
             (all-valid (let ((ok t))
                          (dolist (H subgroups)
                            (unless (funcall 'neovm--grp-is-subgroup H n)
                              (setq ok nil)))
                          ok))
             ;; Lagrange: |H| divides |G|=12 for every subgroup H
             (lagrange-ok (let ((ok t))
                            (dolist (H subgroups)
                              (unless (= (% n (length H)) 0)
                                (setq ok nil)))
                            ok)))
        (list
          ;; All cyclic subgroups and their sizes
          (mapcar (lambda (H) (cons (length H) H)) subgroups)
          all-valid
          lagrange-ok
          ;; Number of distinct subgroups
          (length subgroups)
          ;; Distinct subgroup sizes (divisors of 12)
          (let ((sizes nil))
            (dolist (H subgroups)
              (unless (member (length H) sizes)
                (setq sizes (cons (length H) sizes))))
            (sort sizes #'<))))
    (fmakunbound 'neovm--grp-is-subgroup)
    (fmakunbound 'neovm--grp-all-cyclic-subgroups)))"#;
    let expect = expect_test::expect![[
        r#""OK (((1 0) (2 0 6) (3 0 4 8) (4 0 3 6 9) (6 0 2 4 6 8 10) (12 0 1 2 3 4 5 6 7 8 9 10 11)) t t 6 (1 2 3 4 6 12))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Symmetric group S4: permutation composition and properties
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_algebra_groups_symmetric_s4_permutations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (fset 'neovm--grp-perm-compose
    (lambda (p q)
      "Compose permutations: result(x) = p(q(x))."
      (let ((n (length p)) (result nil))
        (dotimes (i n)
          (setq result (cons (aref p (aref q i)) result)))
        (vconcat (nreverse result)))))

  (fset 'neovm--grp-perm-inverse
    (lambda (p)
      (let* ((n (length p)) (inv (make-vector n 0)))
        (dotimes (i n) (aset inv (aref p i) i))
        inv)))

  (fset 'neovm--grp-perm-order
    (lambda (p)
      "Order of permutation = lcm of cycle lengths."
      (let* ((n (length p))
             (id (make-vector n 0))
             (current (copy-sequence p))
             (order 1))
        (dotimes (i n) (aset id i i))
        (while (not (equal current id))
          (setq current (funcall 'neovm--grp-perm-compose current p))
          (setq order (1+ order)))
        order)))

  (fset 'neovm--grp-perm-cycle-type
    (lambda (p)
      "Compute cycle type of permutation (sorted list of cycle lengths)."
      (let* ((n (length p))
             (visited (make-vector n nil))
             (cycles nil))
        (dotimes (i n)
          (unless (aref visited i)
            (let ((cycle-len 0) (j i))
              (while (not (aref visited j))
                (aset visited j t)
                (setq j (aref p j))
                (setq cycle-len (1+ cycle-len)))
              (setq cycles (cons cycle-len cycles)))))
        (sort cycles #'>))))

  (unwind-protect
      (let* ((id  [0 1 2 3])
             ;; (0 1 2 3) -> 4-cycle
             (cyc4 [1 2 3 0])
             ;; (0 1)(2 3) -> product of two transpositions
             (t12-t34 [1 0 3 2])
             ;; (0 1 2) -> 3-cycle
             (cyc3 [1 2 0 3])
             ;; (0 1) -> transposition
             (trans [1 0 2 3]))
        (list
          ;; Composition: cyc4 * cyc4 = 2-step
          (funcall 'neovm--grp-perm-compose cyc4 cyc4)
          ;; cyc4^4 = identity
          (equal (funcall 'neovm--grp-perm-compose
                   (funcall 'neovm--grp-perm-compose cyc4 cyc4)
                   (funcall 'neovm--grp-perm-compose cyc4 cyc4))
                 id)
          ;; Inverse of cyc4
          (funcall 'neovm--grp-perm-inverse cyc4)
          ;; p * p^-1 = id
          (equal (funcall 'neovm--grp-perm-compose cyc4
                   (funcall 'neovm--grp-perm-inverse cyc4))
                 id)
          ;; Orders
          (funcall 'neovm--grp-perm-order id)      ;; 1
          (funcall 'neovm--grp-perm-order cyc4)     ;; 4
          (funcall 'neovm--grp-perm-order t12-t34)  ;; 2
          (funcall 'neovm--grp-perm-order cyc3)     ;; 3
          (funcall 'neovm--grp-perm-order trans)    ;; 2
          ;; Cycle types
          (funcall 'neovm--grp-perm-cycle-type id)      ;; (1 1 1 1)
          (funcall 'neovm--grp-perm-cycle-type cyc4)     ;; (4)
          (funcall 'neovm--grp-perm-cycle-type t12-t34)  ;; (2 2)
          (funcall 'neovm--grp-perm-cycle-type cyc3)     ;; (3 1)
          (funcall 'neovm--grp-perm-cycle-type trans)    ;; (2 1 1)
          ;; Non-commutativity: cyc4 * trans != trans * cyc4
          (let ((ab (funcall 'neovm--grp-perm-compose cyc4 trans))
                (ba (funcall 'neovm--grp-perm-compose trans cyc4)))
            (list (not (equal ab ba)) ab ba))))
    (fmakunbound 'neovm--grp-perm-compose)
    (fmakunbound 'neovm--grp-perm-inverse)
    (fmakunbound 'neovm--grp-perm-order)
    (fmakunbound 'neovm--grp-perm-cycle-type)))"#;
    let expect = expect_test::expect![[
        r#""OK ([2 3 0 1] t [3 0 1 2] t 1 4 2 3 2 (1 1 1 1) (4) (2 2) (3 1) (2 1 1) (t [2 1 3 0] [0 2 3 1]))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Dihedral group D_n: rotations and reflections
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_algebra_groups_dihedral_group() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  ;; D_4 (symmetries of a square): 8 elements
  ;; Represented as (type . k): (r . k) = rotation by k*90 degrees,
  ;;                             (s . k) = reflection then rotate by k*90
  ;; Multiplication rules:
  ;;   r_i * r_j = r_{(i+j) mod n}
  ;;   r_i * s_j = s_{(i+j) mod n}
  ;;   s_i * r_j = s_{(i-j) mod n}
  ;;   s_i * s_j = r_{(i-j) mod n}

  (fset 'neovm--grp-dihedral-mul
    (lambda (a b n)
      "Multiply elements of D_n."
      (let ((a-type (car a)) (a-k (cdr a))
            (b-type (car b)) (b-k (cdr b)))
        (cond
          ((and (eq a-type 'r) (eq b-type 'r))
           (cons 'r (% (+ a-k b-k) n)))
          ((and (eq a-type 'r) (eq b-type 's))
           (cons 's (% (+ a-k b-k) n)))
          ((and (eq a-type 's) (eq b-type 'r))
           (cons 's (% (+ (- a-k b-k) n) n)))
          ((and (eq a-type 's) (eq b-type 's))
           (cons 'r (% (+ (- a-k b-k) n) n)))))))

  (fset 'neovm--grp-dihedral-inv
    (lambda (a n)
      "Inverse in D_n."
      (let ((a-type (car a)) (a-k (cdr a)))
        (if (eq a-type 'r)
            (cons 'r (% (- n a-k) n))
          ;; s_k * s_k = r_0, so reflections are self-inverse
          a))))

  (fset 'neovm--grp-dihedral-elements
    (lambda (n)
      "All 2n elements of D_n."
      (let ((elems nil))
        (dotimes (k n) (setq elems (cons (cons 'r k) elems)))
        (dotimes (k n) (setq elems (cons (cons 's k) elems)))
        (nreverse elems))))

  (unwind-protect
      (let* ((n 4)
             (elems (funcall 'neovm--grp-dihedral-elements n))
             (id (cons 'r 0)))
        (list
          ;; Group has 2n = 8 elements
          (length elems)
          ;; Verify identity: e*g = g = g*e for all g
          (let ((id-ok t))
            (dolist (g elems)
              (unless (and (equal (funcall 'neovm--grp-dihedral-mul id g n) g)
                          (equal (funcall 'neovm--grp-dihedral-mul g id n) g))
                (setq id-ok nil)))
            id-ok)
          ;; Verify inverses: g*g^-1 = g^-1*g = e
          (let ((inv-ok t))
            (dolist (g elems)
              (let ((gi (funcall 'neovm--grp-dihedral-inv g n)))
                (unless (and (equal (funcall 'neovm--grp-dihedral-mul g gi n) id)
                            (equal (funcall 'neovm--grp-dihedral-mul gi g n) id))
                  (setq inv-ok nil))))
            inv-ok)
          ;; All reflections are self-inverse (order 2)
          (let ((refl-inv-ok t))
            (dotimes (k n)
              (let ((s (cons 's k)))
                (unless (equal (funcall 'neovm--grp-dihedral-mul s s n) id)
                  (setq refl-inv-ok nil))))
            refl-inv-ok)
          ;; r1^4 = identity (rotation order divides n)
          (let* ((r1 (cons 'r 1))
                 (r2 (funcall 'neovm--grp-dihedral-mul r1 r1 n))
                 (r3 (funcall 'neovm--grp-dihedral-mul r2 r1 n))
                 (r4 (funcall 'neovm--grp-dihedral-mul r3 r1 n)))
            (list r2 r3 r4 (equal r4 id)))
          ;; Non-abelian: r1*s0 != s0*r1
          (let ((r1 (cons 'r 1)) (s0 (cons 's 0)))
            (not (equal (funcall 'neovm--grp-dihedral-mul r1 s0 n)
                        (funcall 'neovm--grp-dihedral-mul s0 r1 n))))))
    (fmakunbound 'neovm--grp-dihedral-mul)
    (fmakunbound 'neovm--grp-dihedral-inv)
    (fmakunbound 'neovm--grp-dihedral-elements)))"#;
    let expect = expect_test::expect![[r#""OK (8 t t t ((r . 2) (r . 3) (r . 0) t) t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Group center: Z(G) = {g in G : g*x = x*g for all x in G}
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_algebra_groups_center_computation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  ;; Compute center of D_4 (dihedral group of order 8)
  (fset 'neovm--grp-d4-mul
    (lambda (a b)
      "Multiply elements of D_4."
      (let ((n 4)
            (a-type (car a)) (a-k (cdr a))
            (b-type (car b)) (b-k (cdr b)))
        (cond
          ((and (eq a-type 'r) (eq b-type 'r))
           (cons 'r (% (+ a-k b-k) n)))
          ((and (eq a-type 'r) (eq b-type 's))
           (cons 's (% (+ a-k b-k) n)))
          ((and (eq a-type 's) (eq b-type 'r))
           (cons 's (% (+ (- a-k b-k) n) n)))
          ((and (eq a-type 's) (eq b-type 's))
           (cons 'r (% (+ (- a-k b-k) n) n)))))))

  (fset 'neovm--grp-center
    (lambda (elements mul)
      "Compute center Z(G) = {g : g*x = x*g for all x}."
      (let ((center nil))
        (dolist (g elements)
          (let ((commutes t))
            (dolist (x elements)
              (unless (equal (funcall mul g x) (funcall mul x g))
                (setq commutes nil)))
            (when commutes
              (setq center (cons g center)))))
        (nreverse center))))

  (fset 'neovm--grp-commutator
    (lambda (a b mul inv-fn)
      "Commutator [a,b] = a*b*a^-1*b^-1."
      (funcall mul
        (funcall mul a b)
        (funcall mul (funcall inv-fn a) (funcall inv-fn b)))))

  (unwind-protect
      (let* ((n 4)
             (elems nil))
        ;; Build all elements of D_4
        (dotimes (k n) (setq elems (cons (cons 'r k) elems)))
        (dotimes (k n) (setq elems (cons (cons 's k) elems)))
        (setq elems (nreverse elems))

        (let ((center (funcall 'neovm--grp-center elems 'neovm--grp-d4-mul)))
          (list
            ;; Center of D_4 is {r0, r2} (identity and rotation by 180)
            center
            (length center)
            ;; Verify center elements commute with everyone
            (let ((ok t))
              (dolist (z center)
                (dolist (g elems)
                  (unless (equal (funcall 'neovm--grp-d4-mul z g)
                                (funcall 'neovm--grp-d4-mul g z))
                    (setq ok nil))))
              ok)
            ;; For Z/6Z (abelian), center = entire group
            (let ((z6-elems '(0 1 2 3 4 5))
                  (z6-center nil))
              (dolist (g z6-elems)
                (let ((commutes t))
                  (dolist (x z6-elems)
                    (unless (= (% (+ g x) 6) (% (+ x g) 6))
                      (setq commutes nil)))
                  (when commutes
                    (setq z6-center (cons g z6-center)))))
              (list (nreverse z6-center)
                    (= (length z6-center) 6))))))
    (fmakunbound 'neovm--grp-d4-mul)
    (fmakunbound 'neovm--grp-center)
    (fmakunbound 'neovm--grp-commutator)))"#;
    let expect = expect_test::expect![[r#""OK (((r . 0) (r . 2)) 2 t ((0 1 2 3 4 5) nil))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Group homomorphism verification between Z/12Z and Z/4Z x Z/3Z
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_algebra_groups_homomorphism_verification() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  ;; phi: Z/12Z -> Z/4Z x Z/3Z defined by phi(x) = (x mod 4, x mod 3)
  ;; This is an isomorphism by CRT since gcd(4,3)=1
  (fset 'neovm--grp-phi
    (lambda (x) (list (% x 4) (% x 3))))

  (fset 'neovm--grp-prod-add
    (lambda (a b)
      "Add in Z/4Z x Z/3Z."
      (list (% (+ (car a) (car b)) 4)
            (% (+ (cadr a) (cadr b)) 3))))

  (unwind-protect
      (let ((n 12))
        ;; Verify homomorphism: phi(a+b) = phi(a) + phi(b)
        (let ((homo-ok t)
              (injective-ok t)
              (image nil))
          ;; Check homomorphism property
          (dotimes (a n)
            (dotimes (b n)
              (let ((lhs (funcall 'neovm--grp-phi (% (+ a b) n)))
                    (rhs (funcall 'neovm--grp-prod-add
                           (funcall 'neovm--grp-phi a)
                           (funcall 'neovm--grp-phi b))))
                (unless (equal lhs rhs)
                  (setq homo-ok nil)))))
          ;; Check injectivity: phi(a) = phi(b) => a = b
          (dotimes (a n)
            (dotimes (b n)
              (when (and (/= a b) (equal (funcall 'neovm--grp-phi a)
                                          (funcall 'neovm--grp-phi b)))
                (setq injective-ok nil))))
          ;; Collect image
          (dotimes (x n)
            (let ((img (funcall 'neovm--grp-phi x)))
              (unless (member img image)
                (setq image (cons img image)))))
          ;; Surjectivity: image size = 4*3 = 12
          (let ((surjective-ok (= (length image) (* 4 3))))
            (list
              homo-ok
              injective-ok
              surjective-ok
              ;; Image of each element
              (let ((mapping nil))
                (dotimes (x n)
                  (setq mapping (cons (cons x (funcall 'neovm--grp-phi x)) mapping)))
                (nreverse mapping))
              ;; Kernel: should be just {0} since it's an isomorphism
              (let ((kernel nil))
                (dotimes (x n)
                  (when (equal (funcall 'neovm--grp-phi x) '(0 0))
                    (setq kernel (cons x kernel))))
                (nreverse kernel))))))
    (fmakunbound 'neovm--grp-phi)
    (fmakunbound 'neovm--grp-prod-add)))"#;
    let expect = expect_test::expect![[
        r#""OK (t t t ((0 0 0) (1 1 1) (2 2 2) (3 3 0) (4 0 1) (5 1 2) (6 2 0) (7 3 1) (8 0 2) (9 1 0) (10 2 1) (11 3 2)) (0))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Coset decomposition and quotient group G/H
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_algebra_groups_coset_decomposition() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (fset 'neovm--grp-left-cosets
    (lambda (H n)
      "Compute all distinct left cosets g+H in Z/nZ."
      (let ((cosets nil) (covered nil))
        (dotimes (g n)
          (unless (member g covered)
            (let ((coset nil))
              (dolist (h H)
                (let ((elem (% (+ g h) n)))
                  (setq coset (cons elem coset))
                  (unless (member elem covered)
                    (setq covered (cons elem covered)))))
              (setq cosets (cons (sort coset #'<) cosets)))))
        (nreverse cosets))))

  (fset 'neovm--grp-right-cosets
    (lambda (H n)
      "Compute all distinct right cosets H+g in Z/nZ."
      (let ((cosets nil) (covered nil))
        (dotimes (g n)
          (unless (member g covered)
            (let ((coset nil))
              (dolist (h H)
                (let ((elem (% (+ h g) n)))
                  (setq coset (cons elem coset))
                  (unless (member elem covered)
                    (setq covered (cons elem covered)))))
              (setq cosets (cons (sort coset #'<) cosets)))))
        (nreverse cosets))))

  (unwind-protect
      (let* ((n 12)
             (H '(0 4 8))  ;; subgroup of order 3
             (left (funcall 'neovm--grp-left-cosets H n))
             (right (funcall 'neovm--grp-right-cosets H n)))
        (list
          ;; Left cosets
          left
          ;; Right cosets (should be same since Z/nZ is abelian)
          right
          ;; Left = Right (normal subgroup since G is abelian)
          (equal left right)
          ;; Number of cosets = [G:H] = 12/3 = 4
          (length left)
          ;; Cosets partition G: each element appears exactly once
          (let ((all nil))
            (dolist (coset left)
              (dolist (elem coset)
                (setq all (cons elem all))))
            (equal (sort all #'<) '(0 1 2 3 4 5 6 7 8 9 10 11)))
          ;; Quotient group operation on cosets: (g+H) + (g'+H) = (g+g')+H
          (let* ((c0 (car left))
                 (c1 (cadr left))
                 (rep0 (car c0))
                 (rep1 (car c1))
                 (sum-rep (% (+ rep0 rep1) n))
                 ;; Find the coset containing sum-rep
                 (result-coset nil))
            (dolist (coset left)
              (when (member sum-rep coset)
                (setq result-coset coset)))
            (list rep0 rep1 sum-rep result-coset))))
    (fmakunbound 'neovm--grp-left-cosets)
    (fmakunbound 'neovm--grp-right-cosets)))"#;
    let expect = expect_test::expect![[
        r#""OK (((0 4 8) (1 5 9) (2 6 10) (3 7 11)) ((0 4 8) (1 5 9) (2 6 10) (3 7 11)) t 4 t (0 1 1 (1 5 9)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
