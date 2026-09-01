//! Oracle parity tests for lattice theory in Elisp:
//! partial order from pairs, Hasse diagram construction, join and meet
//! computation, lattice validation, sublattice extraction, distributive
//! lattice check, complemented lattice check, Boolean algebra verification,
//! fixed-point computation (Knaster-Tarski), ideal generation, filter generation.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Partial order from pairs and reflexive-transitive closure
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_lattice_partial_order() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a partial order from a set of pairs (a <= b) and compute
    // the reflexive-transitive closure.
    let form = r#"(progn
  ;; Representation: list of (a . b) meaning a <= b
  ;; Elements: extract all unique elements from pairs
  (fset 'neovm--lat-elements
    (lambda (pairs)
      (let ((elts nil))
        (dolist (p pairs)
          (unless (memq (car p) elts) (setq elts (cons (car p) elts)))
          (unless (memq (cdr p) elts) (setq elts (cons (cdr p) elts))))
        (sort elts (lambda (a b) (string< (symbol-name a) (symbol-name b)))))))

  ;; Direct successors: a < b (given directly)
  (fset 'neovm--lat-direct-above
    (lambda (pairs elem)
      (let ((result nil))
        (dolist (p pairs)
          (when (eq (car p) elem)
            (setq result (cons (cdr p) result))))
        (sort result (lambda (a b) (string< (symbol-name a) (symbol-name b)))))))

  ;; Transitive closure: all elements above elem
  (fset 'neovm--lat-all-above
    (lambda (pairs elem)
      (let ((visited nil)
            (queue (list elem)))
        (while queue
          (let ((current (car queue)))
            (setq queue (cdr queue))
            (unless (memq current visited)
              (setq visited (cons current visited))
              (dolist (above (funcall 'neovm--lat-direct-above pairs current))
                (unless (memq above visited)
                  (setq queue (append queue (list above))))))))
        (sort (delq elem visited)
              (lambda (a b) (string< (symbol-name a) (symbol-name b)))))))

  ;; Check: a <= b (is b reachable from a?)
  (fset 'neovm--lat-leq
    (lambda (pairs a b)
      (or (eq a b)
          (memq b (funcall 'neovm--lat-all-above pairs a)))))

  (unwind-protect
      (let ((pairs '((bot . a) (bot . b) (a . c) (a . d) (b . d) (b . e)
                     (c . top) (d . top) (e . top))))
        (list
          ;; All elements
          (funcall 'neovm--lat-elements pairs)
          ;; Direct above bot: a, b
          (funcall 'neovm--lat-direct-above pairs 'bot)
          ;; All above bot: a, b, c, d, e, top
          (funcall 'neovm--lat-all-above pairs 'bot)
          ;; All above a: c, d, top
          (funcall 'neovm--lat-all-above pairs 'a)
          ;; leq checks
          (funcall 'neovm--lat-leq pairs 'bot 'top)
          (funcall 'neovm--lat-leq pairs 'a 'd)
          (funcall 'neovm--lat-leq pairs 'd 'a)
          (funcall 'neovm--lat-leq pairs 'c 'e)
          ;; Reflexivity
          (funcall 'neovm--lat-leq pairs 'a 'a)))
    (fmakunbound 'neovm--lat-elements)
    (fmakunbound 'neovm--lat-direct-above)
    (fmakunbound 'neovm--lat-all-above)
    (fmakunbound 'neovm--lat-leq)))"#;
    let expect = expect_test::expect![[
        r#""OK ((a b bot c d e top) (a b) (a b c d e top) (c d top) (top) (d top) nil nil t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Hasse diagram: remove transitive edges
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_lattice_hasse_diagram() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Hasse diagram: keep only edges (a,b) where there's no intermediate c
    // with a < c < b.
    let form = r#"(progn
  (require 'cl-lib)
  (fset 'neovm--lat-direct-above
    (lambda (pairs elem)
      (let ((result nil))
        (dolist (p pairs)
          (when (eq (car p) elem)
            (setq result (cons (cdr p) result))))
        result)))

  ;; Compute full transitive closure of a relation
  (fset 'neovm--lat-trans-closure
    (lambda (pairs)
      (let ((result (copy-sequence pairs))
            (changed t))
        (while changed
          (setq changed nil)
          (dolist (p1 result)
            (dolist (p2 result)
              (when (eq (cdr p1) (car p2))
                (let ((new-pair (cons (car p1) (cdr p2))))
                  (unless (cl-find new-pair result :test #'equal)
                    (setq result (cons new-pair result))
                    (setq changed t)))))))
        result)))

  ;; Hasse: edge (a,b) is in Hasse iff no c with (a,c) and (c,b) both in pairs
  (fset 'neovm--lat-hasse
    (lambda (pairs)
      (let ((closure (funcall 'neovm--lat-trans-closure pairs))
            (hasse nil))
        (dolist (edge pairs)
          (let ((a (car edge))
                (b (cdr edge))
                (has-intermediate nil))
            (dolist (c-pair closure)
              (when (and (eq (car c-pair) a)
                         (not (eq (cdr c-pair) a))
                         (not (eq (cdr c-pair) b)))
                (let ((c (cdr c-pair)))
                  (when (cl-find (cons c b) closure :test #'equal)
                    (setq has-intermediate t)))))
            (unless has-intermediate
              (setq hasse (cons edge hasse)))))
        (sort hasse (lambda (a b)
                      (or (string< (symbol-name (car a)) (symbol-name (car b)))
                          (and (eq (car a) (car b))
                               (string< (symbol-name (cdr a)) (symbol-name (cdr b))))))))))

  (unwind-protect
      (let (;; Full order with transitive edges
            (pairs '((bot . a) (bot . b) (bot . c) (bot . top)
                     (a . c) (a . top) (b . top) (c . top))))
        (list
          ;; Full transitive closure
          (length (funcall 'neovm--lat-trans-closure pairs))
          ;; Hasse diagram: remove transitive shortcuts
          (funcall 'neovm--lat-hasse pairs)))
    (fmakunbound 'neovm--lat-direct-above)
    (fmakunbound 'neovm--lat-trans-closure)
    (fmakunbound 'neovm--lat-hasse)))"#;
    let expect =
        expect_test::expect![[r#""OK (8 ((a . c) (b . top) (bot . a) (bot . b) (c . top)))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Join (least upper bound) and meet (greatest lower bound) computation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_lattice_join_meet() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Join(a,b) = least element c such that a <= c and b <= c.
    // Meet(a,b) = greatest element c such that c <= a and c <= b.
    let form = r#"(progn
  (fset 'neovm--lat-all-above
    (lambda (pairs elem)
      (let ((visited nil)
            (queue (list elem)))
        (while queue
          (let ((current (car queue)))
            (setq queue (cdr queue))
            (unless (memq current visited)
              (setq visited (cons current visited))
              (dolist (p pairs)
                (when (eq (car p) current)
                  (unless (memq (cdr p) visited)
                    (setq queue (append queue (list (cdr p))))))))))
        visited)))

  (fset 'neovm--lat-all-below
    (lambda (pairs elem)
      (let ((visited nil)
            (queue (list elem)))
        (while queue
          (let ((current (car queue)))
            (setq queue (cdr queue))
            (unless (memq current visited)
              (setq visited (cons current visited))
              (dolist (p pairs)
                (when (eq (cdr p) current)
                  (unless (memq (car p) visited)
                    (setq queue (append queue (list (car p))))))))))
        visited)))

  (fset 'neovm--lat-leq
    (lambda (pairs a b)
      (memq b (funcall 'neovm--lat-all-above pairs a))))

  ;; Join: least upper bound
  (fset 'neovm--lat-join
    (lambda (pairs all-elts a b)
      (let ((above-a (funcall 'neovm--lat-all-above pairs a))
            (above-b (funcall 'neovm--lat-all-above pairs b)))
        ;; Upper bounds: elements in both above-a and above-b
        (let ((upper-bounds nil))
          (dolist (e above-a)
            (when (memq e above-b)
              (setq upper-bounds (cons e upper-bounds))))
          ;; Least: no other upper bound is below it
          (let ((least nil))
            (dolist (u upper-bounds)
              (when (cl-every (lambda (other)
                                (or (eq u other)
                                    (funcall 'neovm--lat-leq pairs u other)))
                              upper-bounds)
                (setq least u)))
            least)))))

  ;; Meet: greatest lower bound
  (fset 'neovm--lat-meet
    (lambda (pairs all-elts a b)
      (let ((below-a (funcall 'neovm--lat-all-below pairs a))
            (below-b (funcall 'neovm--lat-all-below pairs b)))
        (let ((lower-bounds nil))
          (dolist (e below-a)
            (when (memq e below-b)
              (setq lower-bounds (cons e lower-bounds))))
          (let ((greatest nil))
            (dolist (l lower-bounds)
              (when (cl-every (lambda (other)
                                (or (eq l other)
                                    (funcall 'neovm--lat-leq pairs other l)))
                              lower-bounds)
                (setq greatest l)))
            greatest)))))

  (unwind-protect
      (let ((pairs '((bot . a) (bot . b) (a . top) (b . top)))
            (elts '(bot a b top)))
        (list
          ;; Join(a,b) = top
          (funcall 'neovm--lat-join pairs elts 'a 'b)
          ;; Meet(a,b) = bot
          (funcall 'neovm--lat-meet pairs elts 'a 'b)
          ;; Join(bot,a) = a
          (funcall 'neovm--lat-join pairs elts 'bot 'a)
          ;; Meet(top,a) = a
          (funcall 'neovm--lat-meet pairs elts 'top 'a)
          ;; Join(a,a) = a
          (funcall 'neovm--lat-join pairs elts 'a 'a)
          ;; Meet(a,a) = a
          (funcall 'neovm--lat-meet pairs elts 'a 'a)
          ;; Join(bot,top) = top
          (funcall 'neovm--lat-join pairs elts 'bot 'top)
          ;; Meet(bot,top) = bot
          (funcall 'neovm--lat-meet pairs elts 'bot 'top)))
    (fmakunbound 'neovm--lat-all-above)
    (fmakunbound 'neovm--lat-all-below)
    (fmakunbound 'neovm--lat-leq)
    (fmakunbound 'neovm--lat-join)
    (fmakunbound 'neovm--lat-meet)))"#;
    let expect = expect_test::expect![[r#""ERR (void-function cl-every)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Lattice validation: every pair has both join and meet
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_lattice_validation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A set with a partial order is a lattice iff every pair of elements
    // has both a join and a meet.
    let form = r#"(progn
  (require 'cl-lib)
  (fset 'neovm--lat-all-above
    (lambda (pairs elem)
      (let ((visited nil) (queue (list elem)))
        (while queue
          (let ((current (car queue)))
            (setq queue (cdr queue))
            (unless (memq current visited)
              (setq visited (cons current visited))
              (dolist (p pairs)
                (when (eq (car p) current)
                  (unless (memq (cdr p) visited)
                    (setq queue (append queue (list (cdr p))))))))))
        visited)))

  (fset 'neovm--lat-all-below
    (lambda (pairs elem)
      (let ((visited nil) (queue (list elem)))
        (while queue
          (let ((current (car queue)))
            (setq queue (cdr queue))
            (unless (memq current visited)
              (setq visited (cons current visited))
              (dolist (p pairs)
                (when (eq (cdr p) current)
                  (unless (memq (car p) visited)
                    (setq queue (append queue (list (car p))))))))))
        visited)))

  (fset 'neovm--lat-leq
    (lambda (pairs a b)
      (memq b (funcall 'neovm--lat-all-above pairs a))))

  (fset 'neovm--lat-has-join
    (lambda (pairs a b)
      (let ((above-a (funcall 'neovm--lat-all-above pairs a))
            (above-b (funcall 'neovm--lat-all-above pairs b)))
        (let ((upper-bounds nil))
          (dolist (e above-a)
            (when (memq e above-b)
              (setq upper-bounds (cons e upper-bounds))))
          (cl-some (lambda (u)
                     (cl-every (lambda (o)
                                 (or (eq u o)
                                     (funcall 'neovm--lat-leq pairs u o)))
                               upper-bounds))
                   upper-bounds)))))

  (fset 'neovm--lat-has-meet
    (lambda (pairs a b)
      (let ((below-a (funcall 'neovm--lat-all-below pairs a))
            (below-b (funcall 'neovm--lat-all-below pairs b)))
        (let ((lower-bounds nil))
          (dolist (e below-a)
            (when (memq e below-b)
              (setq lower-bounds (cons e lower-bounds))))
          (cl-some (lambda (l)
                     (cl-every (lambda (o)
                                 (or (eq l o)
                                     (funcall 'neovm--lat-leq pairs o l)))
                               lower-bounds))
                   lower-bounds)))))

  ;; Validate lattice: check all pairs
  (fset 'neovm--lat-is-lattice
    (lambda (pairs elts)
      (let ((ok t))
        (dolist (a elts)
          (dolist (b elts)
            (unless (and (funcall 'neovm--lat-has-join pairs a b)
                         (funcall 'neovm--lat-has-meet pairs a b))
              (setq ok nil))))
        ok)))

  (unwind-protect
      (list
        ;; Diamond lattice: is a lattice
        (funcall 'neovm--lat-is-lattice
          '((bot . a) (bot . b) (a . top) (b . top))
          '(bot a b top))
        ;; Linear order: is a lattice
        (funcall 'neovm--lat-is-lattice
          '((a . b) (b . c) (c . d))
          '(a b c d))
        ;; Not a lattice: two incomparable elements with no join
        (funcall 'neovm--lat-is-lattice
          '((bot . a) (bot . b))
          '(bot a b)))
    (fmakunbound 'neovm--lat-all-above)
    (fmakunbound 'neovm--lat-all-below)
    (fmakunbound 'neovm--lat-leq)
    (fmakunbound 'neovm--lat-has-join)
    (fmakunbound 'neovm--lat-has-meet)
    (fmakunbound 'neovm--lat-is-lattice)))"#;
    let expect = expect_test::expect![[r#""OK (t t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Distributive lattice check
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_lattice_distributive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A lattice is distributive if a ^ (b v c) = (a ^ b) v (a ^ c)
    // for all a, b, c.
    let form = r#"(progn
  (fset 'neovm--lat-all-above
    (lambda (pairs elem)
      (let ((visited nil) (queue (list elem)))
        (while queue
          (let ((current (car queue)))
            (setq queue (cdr queue))
            (unless (memq current visited)
              (setq visited (cons current visited))
              (dolist (p pairs)
                (when (eq (car p) current)
                  (unless (memq (cdr p) visited)
                    (setq queue (append queue (list (cdr p))))))))))
        visited)))

  (fset 'neovm--lat-all-below
    (lambda (pairs elem)
      (let ((visited nil) (queue (list elem)))
        (while queue
          (let ((current (car queue)))
            (setq queue (cdr queue))
            (unless (memq current visited)
              (setq visited (cons current visited))
              (dolist (p pairs)
                (when (eq (cdr p) current)
                  (unless (memq (car p) visited)
                    (setq queue (append queue (list (car p))))))))))
        visited)))

  (fset 'neovm--lat-leq
    (lambda (pairs a b)
      (memq b (funcall 'neovm--lat-all-above pairs a))))

  (fset 'neovm--lat-join
    (lambda (pairs a b)
      (let ((above-a (funcall 'neovm--lat-all-above pairs a))
            (above-b (funcall 'neovm--lat-all-above pairs b))
            (upper nil))
        (dolist (e above-a)
          (when (memq e above-b)
            (setq upper (cons e upper))))
        (let ((least nil))
          (dolist (u upper)
            (when (cl-every (lambda (o) (or (eq u o) (funcall 'neovm--lat-leq pairs u o)))
                            upper)
              (setq least u)))
          least))))

  (fset 'neovm--lat-meet
    (lambda (pairs a b)
      (let ((below-a (funcall 'neovm--lat-all-below pairs a))
            (below-b (funcall 'neovm--lat-all-below pairs b))
            (lower nil))
        (dolist (e below-a)
          (when (memq e below-b)
            (setq lower (cons e lower))))
        (let ((greatest nil))
          (dolist (l lower)
            (when (cl-every (lambda (o) (or (eq l o) (funcall 'neovm--lat-leq pairs o l)))
                            lower)
              (setq greatest l)))
          greatest))))

  ;; Distributive check: a ^ (b v c) = (a ^ b) v (a ^ c) for all triples
  (fset 'neovm--lat-distributive-p
    (lambda (pairs elts)
      (let ((ok t))
        (dolist (a elts)
          (dolist (b elts)
            (dolist (c elts)
              (let* ((b-join-c (funcall 'neovm--lat-join pairs b c))
                     (lhs (funcall 'neovm--lat-meet pairs a b-join-c))
                     (a-meet-b (funcall 'neovm--lat-meet pairs a b))
                     (a-meet-c (funcall 'neovm--lat-meet pairs a c))
                     (rhs (funcall 'neovm--lat-join pairs a-meet-b a-meet-c)))
                (unless (eq lhs rhs)
                  (setq ok nil))))))
        ok)))

  (unwind-protect
      (list
        ;; Diamond lattice {bot, a, b, top}: distributive
        (funcall 'neovm--lat-distributive-p
          '((bot . a) (bot . b) (a . top) (b . top))
          '(bot a b top))
        ;; Chain lattice: always distributive
        (funcall 'neovm--lat-distributive-p
          '((a . b) (b . c))
          '(a b c))
        ;; Pentagon lattice N5 {0, a, b, c, 1} is NOT distributive
        ;; 0 < a < b < 1, 0 < c < 1, c incomparable with a and b
        (funcall 'neovm--lat-distributive-p
          '((n0 . na) (na . nb) (nb . n1) (n0 . nc) (nc . n1))
          '(n0 na nb nc n1)))
    (fmakunbound 'neovm--lat-all-above)
    (fmakunbound 'neovm--lat-all-below)
    (fmakunbound 'neovm--lat-leq)
    (fmakunbound 'neovm--lat-join)
    (fmakunbound 'neovm--lat-meet)
    (fmakunbound 'neovm--lat-distributive-p)))"#;
    let expect = expect_test::expect![[r#""ERR (void-function cl-every)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complemented lattice and Boolean algebra check
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_lattice_complemented_boolean() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Complemented lattice: every element a has a complement a' such that
    // a ^ a' = bot and a v a' = top.
    // Boolean algebra = complemented + distributive lattice.
    let form = r#"(progn
  (fset 'neovm--lat-all-above
    (lambda (pairs elem)
      (let ((visited nil) (queue (list elem)))
        (while queue
          (let ((current (car queue)))
            (setq queue (cdr queue))
            (unless (memq current visited)
              (setq visited (cons current visited))
              (dolist (p pairs) (when (eq (car p) current)
                (unless (memq (cdr p) visited)
                  (setq queue (append queue (list (cdr p))))))))))
        visited)))

  (fset 'neovm--lat-all-below
    (lambda (pairs elem)
      (let ((visited nil) (queue (list elem)))
        (while queue
          (let ((current (car queue)))
            (setq queue (cdr queue))
            (unless (memq current visited)
              (setq visited (cons current visited))
              (dolist (p pairs) (when (eq (cdr p) current)
                (unless (memq (car p) visited)
                  (setq queue (append queue (list (car p))))))))))
        visited)))

  (fset 'neovm--lat-leq
    (lambda (pairs a b)
      (memq b (funcall 'neovm--lat-all-above pairs a))))

  (fset 'neovm--lat-join
    (lambda (pairs a b)
      (let ((above-a (funcall 'neovm--lat-all-above pairs a))
            (above-b (funcall 'neovm--lat-all-above pairs b)) (upper nil))
        (dolist (e above-a) (when (memq e above-b) (setq upper (cons e upper))))
        (let ((least nil))
          (dolist (u upper)
            (when (cl-every (lambda (o) (or (eq u o) (funcall 'neovm--lat-leq pairs u o))) upper)
              (setq least u)))
          least))))

  (fset 'neovm--lat-meet
    (lambda (pairs a b)
      (let ((below-a (funcall 'neovm--lat-all-below pairs a))
            (below-b (funcall 'neovm--lat-all-below pairs b)) (lower nil))
        (dolist (e below-a) (when (memq e below-b) (setq lower (cons e lower))))
        (let ((greatest nil))
          (dolist (l lower)
            (when (cl-every (lambda (o) (or (eq l o) (funcall 'neovm--lat-leq pairs o l))) lower)
              (setq greatest l)))
          greatest))))

  ;; Find complement of an element
  (fset 'neovm--lat-complement
    (lambda (pairs elts bot top elem)
      (let ((comp nil))
        (dolist (c elts)
          (when (and (eq (funcall 'neovm--lat-meet pairs elem c) bot)
                     (eq (funcall 'neovm--lat-join pairs elem c) top))
            (setq comp (cons c comp))))
        comp)))

  ;; Complemented: every element has at least one complement
  (fset 'neovm--lat-complemented-p
    (lambda (pairs elts bot top)
      (cl-every (lambda (a)
                  (not (null (funcall 'neovm--lat-complement pairs elts bot top a))))
                elts)))

  (unwind-protect
      (let (;; Boolean: 2-element {0,1}
            (bool2-pairs '((b0 . b1)))
            (bool2-elts '(b0 b1))
            ;; Diamond {bot, a, b, top}: complemented (a' = b, b' = a)
            (diamond-pairs '((bot . a) (bot . b) (a . top) (b . top)))
            (diamond-elts '(bot a b top)))
        (list
          ;; 2-element: complemented
          (funcall 'neovm--lat-complemented-p bool2-pairs bool2-elts 'b0 'b1)
          ;; Diamond: complemented
          (funcall 'neovm--lat-complemented-p diamond-pairs diamond-elts 'bot 'top)
          ;; Complements of each element in diamond
          (sort (funcall 'neovm--lat-complement diamond-pairs diamond-elts 'bot 'top 'a)
                (lambda (x y) (string< (symbol-name x) (symbol-name y))))
          (sort (funcall 'neovm--lat-complement diamond-pairs diamond-elts 'bot 'top 'bot)
                (lambda (x y) (string< (symbol-name x) (symbol-name y))))
          ;; Chain {a < b < c}: NOT complemented (b has no complement)
          (funcall 'neovm--lat-complemented-p
            '((ca . cb) (cb . cc)) '(ca cb cc) 'ca 'cc)))
    (fmakunbound 'neovm--lat-all-above)
    (fmakunbound 'neovm--lat-all-below)
    (fmakunbound 'neovm--lat-leq)
    (fmakunbound 'neovm--lat-join)
    (fmakunbound 'neovm--lat-meet)
    (fmakunbound 'neovm--lat-complement)
    (fmakunbound 'neovm--lat-complemented-p)))"#;
    let expect = expect_test::expect![[r#""ERR (void-function cl-every)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Fixed-point computation (Knaster-Tarski)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_lattice_knaster_tarski_fixed_point() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Knaster-Tarski: every monotone function on a complete lattice has a
    // least fixed point = meet of all x such that f(x) <= x.
    // We use the powerset lattice of a small set.
    let form = r#"(progn
  ;; Powerset lattice: subsets of {1,2,3} ordered by inclusion
  ;; Represent subsets as sorted lists

  ;; Subset test
  (fset 'neovm--lat-subset-p
    (lambda (a b)
      (cl-every (lambda (x) (memq x b)) a)))

  ;; Union
  (fset 'neovm--lat-set-union
    (lambda (a b)
      (let ((result (copy-sequence a)))
        (dolist (x b)
          (unless (memq x result)
            (setq result (cons x result))))
        (sort result #'<))))

  ;; Intersection
  (fset 'neovm--lat-set-intersect
    (lambda (a b)
      (let ((result nil))
        (dolist (x a)
          (when (memq x b)
            (setq result (cons x result))))
        (sort result #'<))))

  ;; All subsets of {1,2,3}
  (fset 'neovm--lat-powerset
    (lambda ()
      '(() (1) (2) (3) (1 2) (1 3) (2 3) (1 2 3))))

  ;; Least fixed point: meet of {x : f(x) subset-of x}
  (fset 'neovm--lat-lfp
    (lambda (f)
      (let ((all (funcall 'neovm--lat-powerset))
            (prefixed nil))
        ;; Collect pre-fixed points: f(x) <= x
        (dolist (x all)
          (when (funcall 'neovm--lat-subset-p (funcall f x) x)
            (setq prefixed (cons x prefixed))))
        ;; Meet of prefixed points = intersection of all
        (if (null prefixed)
            nil
          (let ((result (car prefixed)))
            (dolist (p (cdr prefixed))
              (setq result (funcall 'neovm--lat-set-intersect result p)))
            result)))))

  ;; Iterative computation: start from bottom, apply f until stable
  (fset 'neovm--lat-lfp-iter
    (lambda (f max-iters)
      (let ((current nil)
            (steps 0))
        (while (< steps max-iters)
          (let ((next (funcall f current)))
            (if (equal next current)
                (setq steps max-iters)  ; converged
              (setq current next)
              (setq steps (1+ steps)))))
        current)))

  (unwind-protect
      (list
        ;; f(x) = x ∪ {1}: lfp = {1}
        (funcall 'neovm--lat-lfp
          (lambda (x) (funcall 'neovm--lat-set-union x '(1))))
        ;; Iterative agrees
        (funcall 'neovm--lat-lfp-iter
          (lambda (x) (funcall 'neovm--lat-set-union x '(1)))
          10)
        ;; f(x) = x ∪ {2,3}: lfp = {2,3}
        (funcall 'neovm--lat-lfp
          (lambda (x) (funcall 'neovm--lat-set-union x '(2 3))))
        ;; Identity: lfp = {} (bottom)
        (funcall 'neovm--lat-lfp #'identity)
        ;; Constant function to {1,2}: lfp = {1,2}
        (funcall 'neovm--lat-lfp (lambda (_x) '(1 2)))
        ;; Iterative: f(x) = if |x|<2 then x∪{|x|+1} else x
        (funcall 'neovm--lat-lfp-iter
          (lambda (x)
            (if (< (length x) 2)
                (funcall 'neovm--lat-set-union x (list (1+ (length x))))
              x))
          10))
    (fmakunbound 'neovm--lat-subset-p)
    (fmakunbound 'neovm--lat-set-union)
    (fmakunbound 'neovm--lat-set-intersect)
    (fmakunbound 'neovm--lat-powerset)
    (fmakunbound 'neovm--lat-lfp)
    (fmakunbound 'neovm--lat-lfp-iter)))"#;
    let expect = expect_test::expect![[r#""ERR (void-function cl-every)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Ideal generation in a lattice
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_lattice_ideal_generation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // An ideal I of a lattice is a non-empty downward-closed subset
    // closed under joins: if a,b in I then a v b in I, and if a in I
    // and b <= a then b in I.
    // Principal ideal generated by x: all elements <= x.
    let form = r#"(progn
  (fset 'neovm--lat-all-below
    (lambda (pairs elem)
      (let ((visited nil) (queue (list elem)))
        (while queue
          (let ((current (car queue)))
            (setq queue (cdr queue))
            (unless (memq current visited)
              (setq visited (cons current visited))
              (dolist (p pairs) (when (eq (cdr p) current)
                (unless (memq (car p) visited)
                  (setq queue (append queue (list (car p))))))))))
        visited)))

  (fset 'neovm--lat-all-above
    (lambda (pairs elem)
      (let ((visited nil) (queue (list elem)))
        (while queue
          (let ((current (car queue)))
            (setq queue (cdr queue))
            (unless (memq current visited)
              (setq visited (cons current visited))
              (dolist (p pairs) (when (eq (car p) current)
                (unless (memq (cdr p) visited)
                  (setq queue (append queue (list (cdr p))))))))))
        visited)))

  (fset 'neovm--lat-leq
    (lambda (pairs a b)
      (memq b (funcall 'neovm--lat-all-above pairs a))))

  ;; Principal ideal: downward closure of x
  (fset 'neovm--lat-principal-ideal
    (lambda (pairs x)
      (sort (funcall 'neovm--lat-all-below pairs x)
            (lambda (a b) (string< (symbol-name a) (symbol-name b))))))

  ;; Check if a subset is an ideal
  (fset 'neovm--lat-is-ideal-p
    (lambda (pairs subset)
      (and
       ;; Non-empty
       (not (null subset))
       ;; Downward closed: if a in subset and b <= a, then b in subset
       (cl-every
        (lambda (a)
          (cl-every
           (lambda (b)
             (if (and (funcall 'neovm--lat-leq pairs b a)
                      (not (eq a b)))
                 (memq b subset)
               t))
           ;; Check all elements that might be below
           (funcall 'neovm--lat-all-below pairs a)))
        subset))))

  (unwind-protect
      (let ((pairs '((bot . a) (bot . b) (a . c) (b . c) (c . top))))
        (list
          ;; Principal ideal of top: everything
          (funcall 'neovm--lat-principal-ideal pairs 'top)
          ;; Principal ideal of c: {bot, a, b, c}
          (funcall 'neovm--lat-principal-ideal pairs 'c)
          ;; Principal ideal of a: {bot, a}
          (funcall 'neovm--lat-principal-ideal pairs 'a)
          ;; Principal ideal of bot: {bot}
          (funcall 'neovm--lat-principal-ideal pairs 'bot)
          ;; Is {bot, a} an ideal? yes
          (funcall 'neovm--lat-is-ideal-p pairs '(bot a))
          ;; Is {a, c} an ideal? no (bot <= a but bot not in subset)
          (funcall 'neovm--lat-is-ideal-p pairs '(a c))
          ;; Is {bot} an ideal? yes
          (funcall 'neovm--lat-is-ideal-p pairs '(bot))))
    (fmakunbound 'neovm--lat-all-below)
    (fmakunbound 'neovm--lat-all-above)
    (fmakunbound 'neovm--lat-leq)
    (fmakunbound 'neovm--lat-principal-ideal)
    (fmakunbound 'neovm--lat-is-ideal-p)))"#;
    let expect = expect_test::expect![[r#""ERR (void-function cl-every)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Filter generation in a lattice
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_lattice_filter_generation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A filter F is the dual of an ideal: upward-closed and closed under meets.
    // Principal filter generated by x: all elements >= x.
    let form = r#"(progn
  (fset 'neovm--lat-all-above
    (lambda (pairs elem)
      (let ((visited nil) (queue (list elem)))
        (while queue
          (let ((current (car queue)))
            (setq queue (cdr queue))
            (unless (memq current visited)
              (setq visited (cons current visited))
              (dolist (p pairs) (when (eq (car p) current)
                (unless (memq (cdr p) visited)
                  (setq queue (append queue (list (cdr p))))))))))
        visited)))

  (fset 'neovm--lat-all-below
    (lambda (pairs elem)
      (let ((visited nil) (queue (list elem)))
        (while queue
          (let ((current (car queue)))
            (setq queue (cdr queue))
            (unless (memq current visited)
              (setq visited (cons current visited))
              (dolist (p pairs) (when (eq (cdr p) current)
                (unless (memq (car p) visited)
                  (setq queue (append queue (list (car p))))))))))
        visited)))

  (fset 'neovm--lat-leq
    (lambda (pairs a b)
      (memq b (funcall 'neovm--lat-all-above pairs a))))

  ;; Principal filter: upward closure of x
  (fset 'neovm--lat-principal-filter
    (lambda (pairs x)
      (sort (funcall 'neovm--lat-all-above pairs x)
            (lambda (a b) (string< (symbol-name a) (symbol-name b))))))

  ;; Check if a subset is a filter
  (fset 'neovm--lat-is-filter-p
    (lambda (pairs subset)
      (and
       (not (null subset))
       ;; Upward closed: if a in subset and a <= b, then b in subset
       (cl-every
        (lambda (a)
          (cl-every
           (lambda (b)
             (if (and (funcall 'neovm--lat-leq pairs a b)
                      (not (eq a b)))
                 (memq b subset)
               t))
           (funcall 'neovm--lat-all-above pairs a)))
        subset))))

  (unwind-protect
      (let ((pairs '((bot . a) (bot . b) (a . c) (b . c) (c . top))))
        (list
          ;; Principal filter of bot: everything
          (funcall 'neovm--lat-principal-filter pairs 'bot)
          ;; Principal filter of c: {c, top}
          (funcall 'neovm--lat-principal-filter pairs 'c)
          ;; Principal filter of top: {top}
          (funcall 'neovm--lat-principal-filter pairs 'top)
          ;; Principal filter of a: {a, c, top}
          (funcall 'neovm--lat-principal-filter pairs 'a)
          ;; Is {c, top} a filter? yes
          (funcall 'neovm--lat-is-filter-p pairs '(c top))
          ;; Is {a, b} a filter? no (a <= c but c not in subset)
          (funcall 'neovm--lat-is-filter-p pairs '(a b))
          ;; Is {top} a filter? yes
          (funcall 'neovm--lat-is-filter-p pairs '(top))
          ;; Duality: principal ideal of x and principal filter of x
          ;; together cover the entire lattice if x is neither bot nor top
          (let* ((ideal-a (funcall 'neovm--lat-all-below pairs 'a))
                 (filter-a (funcall 'neovm--lat-all-above pairs 'a))
                 (combined nil))
            (dolist (e ideal-a) (unless (memq e combined) (setq combined (cons e combined))))
            (dolist (e filter-a) (unless (memq e combined) (setq combined (cons e combined))))
            (sort combined (lambda (x y) (string< (symbol-name x) (symbol-name y)))))))
    (fmakunbound 'neovm--lat-all-above)
    (fmakunbound 'neovm--lat-all-below)
    (fmakunbound 'neovm--lat-leq)
    (fmakunbound 'neovm--lat-principal-filter)
    (fmakunbound 'neovm--lat-is-filter-p)))"#;
    let expect = expect_test::expect![[r#""ERR (void-function cl-every)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Sublattice extraction
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_lattice_sublattice() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A sublattice S of lattice L is a subset closed under join and meet.
    // Given a subset, check if it's a sublattice by verifying closure.
    let form = r#"(progn
  (fset 'neovm--lat-all-above
    (lambda (pairs elem)
      (let ((visited nil) (queue (list elem)))
        (while queue
          (let ((current (car queue)))
            (setq queue (cdr queue))
            (unless (memq current visited)
              (setq visited (cons current visited))
              (dolist (p pairs) (when (eq (car p) current)
                (unless (memq (cdr p) visited)
                  (setq queue (append queue (list (cdr p))))))))))
        visited)))

  (fset 'neovm--lat-all-below
    (lambda (pairs elem)
      (let ((visited nil) (queue (list elem)))
        (while queue
          (let ((current (car queue)))
            (setq queue (cdr queue))
            (unless (memq current visited)
              (setq visited (cons current visited))
              (dolist (p pairs) (when (eq (cdr p) current)
                (unless (memq (car p) visited)
                  (setq queue (append queue (list (car p))))))))))
        visited)))

  (fset 'neovm--lat-leq
    (lambda (pairs a b)
      (memq b (funcall 'neovm--lat-all-above pairs a))))

  (fset 'neovm--lat-join
    (lambda (pairs a b)
      (let ((above-a (funcall 'neovm--lat-all-above pairs a))
            (above-b (funcall 'neovm--lat-all-above pairs b)) (upper nil))
        (dolist (e above-a) (when (memq e above-b) (setq upper (cons e upper))))
        (let ((least nil))
          (dolist (u upper)
            (when (cl-every (lambda (o) (or (eq u o) (funcall 'neovm--lat-leq pairs u o))) upper)
              (setq least u)))
          least))))

  (fset 'neovm--lat-meet
    (lambda (pairs a b)
      (let ((below-a (funcall 'neovm--lat-all-below pairs a))
            (below-b (funcall 'neovm--lat-all-below pairs b)) (lower nil))
        (dolist (e below-a) (when (memq e below-b) (setq lower (cons e lower))))
        (let ((greatest nil))
          (dolist (l lower)
            (when (cl-every (lambda (o) (or (eq l o) (funcall 'neovm--lat-leq pairs o l))) lower)
              (setq greatest l)))
          greatest))))

  ;; Check sublattice: closed under join and meet
  (fset 'neovm--lat-is-sublattice-p
    (lambda (pairs subset)
      (let ((ok t))
        (dolist (a subset)
          (dolist (b subset)
            (let ((j (funcall 'neovm--lat-join pairs a b))
                  (m (funcall 'neovm--lat-meet pairs a b)))
              (unless (and (memq j subset) (memq m subset))
                (setq ok nil)))))
        ok)))

  (unwind-protect
      (let ((pairs '((bot . a) (bot . b) (a . top) (b . top))))
        (list
          ;; {bot, a, top}: sublattice (join(bot,a)=a, meet(bot,a)=bot, etc.)
          (funcall 'neovm--lat-is-sublattice-p pairs '(bot a top))
          ;; {bot, b, top}: sublattice
          (funcall 'neovm--lat-is-sublattice-p pairs '(bot b top))
          ;; {a, b}: NOT sublattice (join(a,b)=top not in subset)
          (funcall 'neovm--lat-is-sublattice-p pairs '(a b))
          ;; {bot, top}: sublattice
          (funcall 'neovm--lat-is-sublattice-p pairs '(bot top))
          ;; Full set: trivially sublattice
          (funcall 'neovm--lat-is-sublattice-p pairs '(bot a b top))
          ;; Singleton: sublattice
          (funcall 'neovm--lat-is-sublattice-p pairs '(a))))
    (fmakunbound 'neovm--lat-all-above)
    (fmakunbound 'neovm--lat-all-below)
    (fmakunbound 'neovm--lat-leq)
    (fmakunbound 'neovm--lat-join)
    (fmakunbound 'neovm--lat-meet)
    (fmakunbound 'neovm--lat-is-sublattice-p)))"#;
    let expect = expect_test::expect![[r#""ERR (void-function cl-every)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
