//! Oracle parity tests for abstract set theory operations in Elisp.
//!
//! Implements sets as sorted lists without duplicates. Tests union,
//! intersection, difference, symmetric difference, power set, Cartesian
//! product, De Morgan's laws, distributive laws, and relation operations
//! including composition and transitive closure.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Set representation and basic operations: union, intersection, difference
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_abstract_set_basic_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Create a set from an unsorted list with possible duplicates
  (fset 'neovm--as-make-set
    (lambda (lst)
      "Create a sorted set (no duplicates) from LST."
      (let ((sorted (sort (copy-sequence lst) #'<))
            (result nil))
        (dolist (x sorted)
          (unless (and result (= x (car result)))
            (setq result (cons x result))))
        (nreverse result))))

  (fset 'neovm--as-union
    (lambda (a b)
      "Union of sorted sets A and B."
      (cond
        ((null a) b)
        ((null b) a)
        ((< (car a) (car b))
         (cons (car a) (funcall 'neovm--as-union (cdr a) b)))
        ((> (car a) (car b))
         (cons (car b) (funcall 'neovm--as-union a (cdr b))))
        (t (cons (car a) (funcall 'neovm--as-union (cdr a) (cdr b)))))))

  (fset 'neovm--as-intersect
    (lambda (a b)
      "Intersection of sorted sets A and B."
      (cond
        ((or (null a) (null b)) nil)
        ((< (car a) (car b)) (funcall 'neovm--as-intersect (cdr a) b))
        ((> (car a) (car b)) (funcall 'neovm--as-intersect a (cdr b)))
        (t (cons (car a) (funcall 'neovm--as-intersect (cdr a) (cdr b)))))))

  (fset 'neovm--as-diff
    (lambda (a b)
      "Set difference A \\ B for sorted sets."
      (cond
        ((null a) nil)
        ((null b) a)
        ((< (car a) (car b))
         (cons (car a) (funcall 'neovm--as-diff (cdr a) b)))
        ((> (car a) (car b))
         (funcall 'neovm--as-diff a (cdr b)))
        (t (funcall 'neovm--as-diff (cdr a) (cdr b))))))

  (fset 'neovm--as-sym-diff
    (lambda (a b)
      "Symmetric difference: elements in exactly one of A or B."
      (funcall 'neovm--as-union
               (funcall 'neovm--as-diff a b)
               (funcall 'neovm--as-diff b a))))

  (unwind-protect
      (let ((a (funcall 'neovm--as-make-set '(5 3 1 7 9 3 5)))
            (b (funcall 'neovm--as-make-set '(2 3 5 8 11 3 2)))
            (c (funcall 'neovm--as-make-set '(1 2 3 4 5 6 7 8 9 10))))
        (list
          ;; Set creation removes duplicates and sorts
          a b
          ;; Basic operations
          (funcall 'neovm--as-union a b)
          (funcall 'neovm--as-intersect a b)
          (funcall 'neovm--as-diff a b)
          (funcall 'neovm--as-diff b a)
          (funcall 'neovm--as-sym-diff a b)
          ;; Edge cases: empty set
          (funcall 'neovm--as-union nil a)
          (funcall 'neovm--as-union a nil)
          (funcall 'neovm--as-intersect nil a)
          (funcall 'neovm--as-diff a nil)
          (funcall 'neovm--as-diff nil a)
          (funcall 'neovm--as-sym-diff nil a)
          ;; Idempotent: A union A = A, A intersect A = A
          (equal (funcall 'neovm--as-union a a) a)
          (equal (funcall 'neovm--as-intersect a a) a)
          ;; A diff A = empty
          (funcall 'neovm--as-diff a a)
          ;; Sym diff self = empty
          (funcall 'neovm--as-sym-diff a a)
          ;; Cardinalities
          (list (length a) (length b)
                (length (funcall 'neovm--as-union a b))
                (length (funcall 'neovm--as-intersect a b)))))
    (fmakunbound 'neovm--as-make-set)
    (fmakunbound 'neovm--as-union)
    (fmakunbound 'neovm--as-intersect)
    (fmakunbound 'neovm--as-diff)
    (fmakunbound 'neovm--as-sym-diff)))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 3 5 7 9) (2 3 5 8 11) (1 2 3 5 7 8 9 11) (3 5) (1 7 9) (2 8 11) (1 2 7 8 9 11) (1 3 5 7 9) (1 3 5 7 9) nil (1 3 5 7 9) nil (1 3 5 7 9) t t nil nil (5 5 8 2))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Power set computation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_abstract_set_power_set() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--as-power-set
    (lambda (s)
      "Compute the power set of S using recursive doubling."
      (if (null s)
          '(nil)
        (let* ((rest-ps (funcall 'neovm--as-power-set (cdr s)))
               (head (car s))
               (with-head (mapcar (lambda (subset) (cons head subset)) rest-ps)))
          (append rest-ps with-head)))))

  (fset 'neovm--as-sort-set-of-sets
    (lambda (ss)
      "Sort a set of sets for deterministic comparison."
      (sort (copy-sequence ss)
            (lambda (a b)
              (cond
                ((and (null a) (null b)) nil)
                ((null a) t)
                ((null b) nil)
                ((< (car a) (car b)) t)
                ((> (car a) (car b)) nil)
                (t (funcall 'neovm--as-sort-compare (cdr a) (cdr b))))))))

  (fset 'neovm--as-sort-compare
    (lambda (a b)
      (cond
        ((and (null a) (null b)) nil)
        ((null a) t)
        ((null b) nil)
        ((< (car a) (car b)) t)
        ((> (car a) (car b)) nil)
        (t (funcall 'neovm--as-sort-compare (cdr a) (cdr b))))))

  (unwind-protect
      (let* ((s0 nil)
             (s1 '(1))
             (s2 '(1 2))
             (s3 '(1 2 3))
             (s4 '(1 2 3 4))
             (ps0 (funcall 'neovm--as-power-set s0))
             (ps1 (funcall 'neovm--as-power-set s1))
             (ps2 (funcall 'neovm--as-power-set s2))
             (ps3 (funcall 'neovm--as-power-set s3))
             (ps4 (funcall 'neovm--as-power-set s4)))
        (list
          ;; Sizes: 2^n
          (length ps0) (length ps1) (length ps2) (length ps3) (length ps4)
          ;; P({}) = { {} }
          ps0
          ;; P({1}) = { {}, {1} }
          (funcall 'neovm--as-sort-set-of-sets ps1)
          ;; P({1,2}) sorted
          (funcall 'neovm--as-sort-set-of-sets ps2)
          ;; Every subset of {1,2,3} has at most 3 elements
          (let ((all-ok t))
            (dolist (subset ps3)
              (when (> (length subset) 3)
                (setq all-ok nil)))
            all-ok)
          ;; Empty set is always in power set
          (if (member nil ps3) t nil)
          ;; Full set is always in power set
          (if (member '(1 2 3) ps3) t nil)))
    (fmakunbound 'neovm--as-power-set)
    (fmakunbound 'neovm--as-sort-set-of-sets)
    (fmakunbound 'neovm--as-sort-compare)))"#;
    let expect =
        expect_test::expect![[r#""OK (1 2 4 8 16 (nil) (nil (1)) (nil (1) (1 2) (2)) t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Cartesian product
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_abstract_set_cartesian_product() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--as-cartesian
    (lambda (a b)
      "Compute the Cartesian product A x B as list of (x . y) pairs."
      (let ((result nil))
        (dolist (x a)
          (dolist (y b)
            (setq result (cons (cons x y) result))))
        (nreverse result))))

  (fset 'neovm--as-cartesian-3
    (lambda (a b c)
      "Compute A x B x C as list of (x y z) triples."
      (let ((result nil))
        (dolist (x a)
          (dolist (y b)
            (dolist (z c)
              (setq result (cons (list x y z) result)))))
        (nreverse result))))

  (unwind-protect
      (let ((a '(1 2 3))
            (b '(10 20))
            (c '(100 200)))
        (list
          ;; A x B
          (funcall 'neovm--as-cartesian a b)
          ;; |A x B| = |A| * |B|
          (= (length (funcall 'neovm--as-cartesian a b)) (* (length a) (length b)))
          ;; B x A (different from A x B for non-equal sets)
          (funcall 'neovm--as-cartesian b a)
          ;; A x {} = {}
          (funcall 'neovm--as-cartesian a nil)
          ;; {} x B = {}
          (funcall 'neovm--as-cartesian nil b)
          ;; A x {single} has |A| elements
          (= (length (funcall 'neovm--as-cartesian a '(99))) (length a))
          ;; Triple product
          (funcall 'neovm--as-cartesian-3 '(1 2) '(3 4) '(5 6))
          ;; |A x B x C| = |A| * |B| * |C|
          (= (length (funcall 'neovm--as-cartesian-3 a b c))
             (* (length a) (length b) (length c)))
          ;; Self-product A x A
          (funcall 'neovm--as-cartesian '(1 2) '(1 2))
          ;; Diagonal of self-product
          (let ((diag nil))
            (dolist (pair (funcall 'neovm--as-cartesian a a))
              (when (= (car pair) (cdr pair))
                (setq diag (cons pair diag))))
            (nreverse diag))))
    (fmakunbound 'neovm--as-cartesian)
    (fmakunbound 'neovm--as-cartesian-3)))"#;
    let expect = expect_test::expect![[
        r#""OK (((1 . 10) (1 . 20) (2 . 10) (2 . 20) (3 . 10) (3 . 20)) t ((10 . 1) (10 . 2) (10 . 3) (20 . 1) (20 . 2) (20 . 3)) nil nil t ((1 3 5) (1 3 6) (1 4 5) (1 4 6) (2 3 5) (2 3 6) (2 4 5) (2 4 6)) t ((1 . 1) (1 . 2) (2 . 1) (2 . 2)) ((1 . 1) (2 . 2) (3 . 3)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// De Morgan's laws and distributive laws verification
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_abstract_set_algebra_laws() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Verify set algebra laws:
    // De Morgan: complement(A union B) = complement(A) intersect complement(B)
    // Distributive: A intersect (B union C) = (A intersect B) union (A intersect C)
    let form = r#"(progn
  (fset 'neovm--as2-union
    (lambda (a b)
      (cond ((null a) b) ((null b) a)
            ((< (car a) (car b)) (cons (car a) (funcall 'neovm--as2-union (cdr a) b)))
            ((> (car a) (car b)) (cons (car b) (funcall 'neovm--as2-union a (cdr b))))
            (t (cons (car a) (funcall 'neovm--as2-union (cdr a) (cdr b)))))))

  (fset 'neovm--as2-intersect
    (lambda (a b)
      (cond ((or (null a) (null b)) nil)
            ((< (car a) (car b)) (funcall 'neovm--as2-intersect (cdr a) b))
            ((> (car a) (car b)) (funcall 'neovm--as2-intersect a (cdr b)))
            (t (cons (car a) (funcall 'neovm--as2-intersect (cdr a) (cdr b)))))))

  (fset 'neovm--as2-diff
    (lambda (a b)
      (cond ((null a) nil) ((null b) a)
            ((< (car a) (car b)) (cons (car a) (funcall 'neovm--as2-diff (cdr a) b)))
            ((> (car a) (car b)) (funcall 'neovm--as2-diff a (cdr b)))
            (t (funcall 'neovm--as2-diff (cdr a) (cdr b))))))

  (fset 'neovm--as2-complement
    (lambda (s universe)
      "Complement of S with respect to UNIVERSE."
      (funcall 'neovm--as2-diff universe s)))

  (unwind-protect
      (let ((u '(1 2 3 4 5 6 7 8 9 10))  ;; universe
            (a '(1 2 3 4 5))
            (b '(3 4 5 6 7))
            (c '(5 6 7 8 9)))
        (list
          ;; De Morgan 1: comp(A union B) = comp(A) intersect comp(B)
          (let ((lhs (funcall 'neovm--as2-complement
                              (funcall 'neovm--as2-union a b) u))
                (rhs (funcall 'neovm--as2-intersect
                              (funcall 'neovm--as2-complement a u)
                              (funcall 'neovm--as2-complement b u))))
            (list (equal lhs rhs) lhs rhs))

          ;; De Morgan 2: comp(A intersect B) = comp(A) union comp(B)
          (let ((lhs (funcall 'neovm--as2-complement
                              (funcall 'neovm--as2-intersect a b) u))
                (rhs (funcall 'neovm--as2-union
                              (funcall 'neovm--as2-complement a u)
                              (funcall 'neovm--as2-complement b u))))
            (list (equal lhs rhs) lhs rhs))

          ;; Distributive 1: A intersect (B union C) = (A int B) union (A int C)
          (let ((lhs (funcall 'neovm--as2-intersect a
                              (funcall 'neovm--as2-union b c)))
                (rhs (funcall 'neovm--as2-union
                              (funcall 'neovm--as2-intersect a b)
                              (funcall 'neovm--as2-intersect a c))))
            (list (equal lhs rhs) lhs rhs))

          ;; Distributive 2: A union (B intersect C) = (A union B) intersect (A union C)
          (let ((lhs (funcall 'neovm--as2-union a
                              (funcall 'neovm--as2-intersect b c)))
                (rhs (funcall 'neovm--as2-intersect
                              (funcall 'neovm--as2-union a b)
                              (funcall 'neovm--as2-union a c))))
            (list (equal lhs rhs) lhs rhs))

          ;; Absorption: A union (A intersect B) = A
          (equal (funcall 'neovm--as2-union a (funcall 'neovm--as2-intersect a b)) a)

          ;; Absorption: A intersect (A union B) = A
          (equal (funcall 'neovm--as2-intersect a (funcall 'neovm--as2-union a b)) a)

          ;; Double complement: comp(comp(A)) = A
          (equal (funcall 'neovm--as2-complement
                          (funcall 'neovm--as2-complement a u) u)
                 a)))
    (fmakunbound 'neovm--as2-union)
    (fmakunbound 'neovm--as2-intersect)
    (fmakunbound 'neovm--as2-diff)
    (fmakunbound 'neovm--as2-complement)))"#;
    let expect = expect_test::expect![[
        r#""OK ((t (8 9 10) (8 9 10)) (t (1 2 6 7 8 9 10) (1 2 6 7 8 9 10)) (t (3 4 5) (3 4 5)) (t (1 2 3 4 5 6 7) (1 2 3 4 5 6 7)) t t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Relation operations: composition
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_abstract_set_relation_composition() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A relation is a set of (a . b) pairs. Composition of R and S:
    // R;S = { (a,c) | exists b: (a,b) in R and (b,c) in S }
    let form = r#"(progn
  (fset 'neovm--as-rel-compose
    (lambda (r s)
      "Compose relations R and S. Each is a list of (a . b) pairs."
      (let ((result nil))
        (dolist (r-pair r)
          (dolist (s-pair s)
            (when (equal (cdr r-pair) (car s-pair))
              (let ((new-pair (cons (car r-pair) (cdr s-pair))))
                (unless (member new-pair result)
                  (setq result (cons new-pair result)))))))
        (sort result (lambda (a b)
                       (or (< (car a) (car b))
                           (and (= (car a) (car b))
                                (< (cdr a) (cdr b)))))))))

  (fset 'neovm--as-rel-reflexive-closure
    (lambda (r domain)
      "Add identity pairs for all elements in DOMAIN."
      (let ((result (copy-sequence r)))
        (dolist (x domain)
          (let ((pair (cons x x)))
            (unless (member pair result)
              (setq result (cons pair result)))))
        (sort result (lambda (a b)
                       (or (< (car a) (car b))
                           (and (= (car a) (car b))
                                (< (cdr a) (cdr b)))))))))

  (fset 'neovm--as-rel-symmetric-closure
    (lambda (r)
      "Add reverse of each pair."
      (let ((result (copy-sequence r)))
        (dolist (pair r)
          (let ((rev (cons (cdr pair) (car pair))))
            (unless (member rev result)
              (setq result (cons rev result)))))
        (sort result (lambda (a b)
                       (or (< (car a) (car b))
                           (and (= (car a) (car b))
                                (< (cdr a) (cdr b)))))))))

  (unwind-protect
      (let ((r '((1 . 2) (2 . 3) (3 . 4)))
            (s '((2 . 5) (3 . 6) (4 . 7)))
            (domain '(1 2 3 4)))
        (list
          ;; R;S composition
          (funcall 'neovm--as-rel-compose r s)
          ;; R;R (self-composition = "2-step reachability")
          (funcall 'neovm--as-rel-compose r r)
          ;; R;R;R (3-step)
          (funcall 'neovm--as-rel-compose
                   (funcall 'neovm--as-rel-compose r r) r)
          ;; Reflexive closure
          (funcall 'neovm--as-rel-reflexive-closure r domain)
          ;; Symmetric closure
          (funcall 'neovm--as-rel-symmetric-closure r)
          ;; Symmetric closure of symmetric closure = same
          (let ((sym (funcall 'neovm--as-rel-symmetric-closure r)))
            (equal (funcall 'neovm--as-rel-symmetric-closure sym) sym))
          ;; Identity relation composed with R = R
          (let ((id (mapcar (lambda (x) (cons x x)) domain)))
            (equal (funcall 'neovm--as-rel-compose id r) r))
          ;; R composed with identity = R
          (let ((id (mapcar (lambda (x) (cons x x)) domain)))
            (equal (funcall 'neovm--as-rel-compose r id) r))))
    (fmakunbound 'neovm--as-rel-compose)
    (fmakunbound 'neovm--as-rel-reflexive-closure)
    (fmakunbound 'neovm--as-rel-symmetric-closure)))"#;
    let expect = expect_test::expect![[
        r#""OK (((1 . 5) (2 . 6) (3 . 7)) ((1 . 3) (2 . 4)) ((1 . 4)) ((1 . 1) (1 . 2) (2 . 2) (2 . 3) (3 . 3) (3 . 4) (4 . 4)) ((1 . 2) (2 . 1) (2 . 3) (3 . 2) (3 . 4) (4 . 3)) t t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Transitive closure of a relation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_abstract_set_transitive_closure() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--as3-compose
    (lambda (r s)
      (let ((result nil))
        (dolist (rp r)
          (dolist (sp s)
            (when (equal (cdr rp) (car sp))
              (let ((np (cons (car rp) (cdr sp))))
                (unless (member np result)
                  (setq result (cons np result)))))))
        (sort result (lambda (a b)
                       (or (< (car a) (car b))
                           (and (= (car a) (car b))
                                (< (cdr a) (cdr b)))))))))

  (fset 'neovm--as3-rel-union
    (lambda (r s)
      "Union of two relations."
      (let ((result (copy-sequence r)))
        (dolist (p s)
          (unless (member p result)
            (setq result (cons p result))))
        (sort result (lambda (a b)
                       (or (< (car a) (car b))
                           (and (= (car a) (car b))
                                (< (cdr a) (cdr b)))))))))

  (fset 'neovm--as3-transitive-closure
    (lambda (r)
      "Compute transitive closure via iterated composition."
      (let ((tc (copy-sequence r))
            (changed t)
            (iters 0))
        (while (and changed (< iters 10))
          (setq changed nil)
          (setq iters (1+ iters))
          (let ((new-pairs (funcall 'neovm--as3-compose tc r)))
            (dolist (p new-pairs)
              (unless (member p tc)
                (setq tc (cons p tc))
                (setq changed t)))))
        (sort tc (lambda (a b)
                   (or (< (car a) (car b))
                       (and (= (car a) (car b))
                            (< (cdr a) (cdr b)))))))))

  (fset 'neovm--as3-is-transitive
    (lambda (r)
      "Check if relation R is transitive."
      (let ((ok t))
        (dolist (p1 r)
          (dolist (p2 r)
            (when (and ok (equal (cdr p1) (car p2)))
              (unless (member (cons (car p1) (cdr p2)) r)
                (setq ok nil)))))
        ok)))

  (unwind-protect
      (let ((r1 '((1 . 2) (2 . 3) (3 . 4)))
            (r2 '((1 . 2) (2 . 3) (3 . 1)))   ;; cycle
            (r3 '((1 . 2) (1 . 3) (2 . 4)))   ;; tree-like
            (r4 '((1 . 1))))                   ;; reflexive single
        (list
          ;; TC of linear chain: all (i,j) where i < j
          (funcall 'neovm--as3-transitive-closure r1)
          ;; TC of cycle: complete graph on {1,2,3}
          (funcall 'neovm--as3-transitive-closure r2)
          ;; TC of tree
          (funcall 'neovm--as3-transitive-closure r3)
          ;; TC of reflexive single
          (funcall 'neovm--as3-transitive-closure r4)
          ;; TC is always transitive
          (funcall 'neovm--as3-is-transitive
                   (funcall 'neovm--as3-transitive-closure r1))
          (funcall 'neovm--as3-is-transitive
                   (funcall 'neovm--as3-transitive-closure r2))
          ;; Original may not be transitive
          (funcall 'neovm--as3-is-transitive r1)
          ;; TC contains original
          (let ((tc (funcall 'neovm--as3-transitive-closure r1))
                (all-ok t))
            (dolist (p r1)
              (unless (member p tc)
                (setq all-ok nil)))
            all-ok)
          ;; TC of TC = TC (idempotent)
          (let ((tc (funcall 'neovm--as3-transitive-closure r1)))
            (equal (funcall 'neovm--as3-transitive-closure tc) tc))))
    (fmakunbound 'neovm--as3-compose)
    (fmakunbound 'neovm--as3-rel-union)
    (fmakunbound 'neovm--as3-transitive-closure)
    (fmakunbound 'neovm--as3-is-transitive)))"#;
    let expect = expect_test::expect![[
        r#""OK (((1 . 2) (1 . 3) (1 . 4) (2 . 3) (2 . 4) (3 . 4)) ((1 . 1) (1 . 2) (1 . 3) (2 . 1) (2 . 2) (2 . 3) (3 . 1) (3 . 2) (3 . 3)) ((1 . 2) (1 . 3) (1 . 4) (2 . 4)) ((1 . 1)) t t nil t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
