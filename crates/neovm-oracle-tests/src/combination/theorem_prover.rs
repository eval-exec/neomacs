//! Oracle parity tests for theorem prover patterns in Elisp:
//! resolution-based theorem proving, clause normalization (CNF conversion),
//! unification for first-order logic, Skolemization, paramodulation
//! for equality, Davis-Putnam procedure, and unit propagation.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// CNF conversion: propositional logic formula to conjunctive normal form
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_theorem_prover_cnf_conversion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Convert propositional formulas to CNF via:
    // 1. Eliminate implications (=> and <=>)
    // 2. Push negation inward (De Morgan, double negation)
    // 3. Distribute OR over AND
    let form = r#"(progn
  ;; Eliminate implications: (=> A B) -> (or (not A) B)
  ;; (iff A B) -> (and (or (not A) B) (or (not B) A))
  (fset 'neovm--tp-elim-impl
    (lambda (f)
      (cond
       ((symbolp f) f)
       ((eq (car f) 'not)
        (list 'not (funcall 'neovm--tp-elim-impl (cadr f))))
       ((eq (car f) '=>)
        (list 'or
              (list 'not (funcall 'neovm--tp-elim-impl (cadr f)))
              (funcall 'neovm--tp-elim-impl (caddr f))))
       ((eq (car f) '<=>)
        (let ((a (funcall 'neovm--tp-elim-impl (cadr f)))
              (b (funcall 'neovm--tp-elim-impl (caddr f))))
          (list 'and
                (list 'or (list 'not a) b)
                (list 'or (list 'not b) a))))
       ((memq (car f) '(and or))
        (cons (car f)
              (mapcar 'neovm--tp-elim-impl (cdr f))))
       (t f))))

  ;; Push negation inward: De Morgan's laws, double negation elimination
  (fset 'neovm--tp-nnf
    (lambda (f)
      (cond
       ((symbolp f) f)
       ((and (eq (car f) 'not) (symbolp (cadr f))) f)
       ((and (eq (car f) 'not) (eq (caadr f) 'not))
        (funcall 'neovm--tp-nnf (caddr (cadr f))))
       ((and (eq (car f) 'not) (eq (caadr f) 'and))
        (cons 'or (mapcar (lambda (x)
                            (funcall 'neovm--tp-nnf (list 'not x)))
                          (cdadr f))))
       ((and (eq (car f) 'not) (eq (caadr f) 'or))
        (cons 'and (mapcar (lambda (x)
                             (funcall 'neovm--tp-nnf (list 'not x)))
                           (cdadr f))))
       ((memq (car f) '(and or))
        (cons (car f)
              (mapcar 'neovm--tp-nnf (cdr f))))
       (t f))))

  ;; Distribute OR over AND: (or A (and B C)) -> (and (or A B) (or A C))
  (fset 'neovm--tp-distribute
    (lambda (f)
      (cond
       ((symbolp f) f)
       ((eq (car f) 'and)
        (cons 'and (mapcar 'neovm--tp-distribute (cdr f))))
       ((eq (car f) 'or)
        (let ((args (mapcar 'neovm--tp-distribute (cdr f))))
          ;; Find first AND in args
          (let ((and-idx nil) (i 0))
            (dolist (a args)
              (when (and (consp a) (eq (car a) 'and) (not and-idx))
                (setq and-idx i))
              (setq i (1+ i)))
            (if and-idx
                (let* ((and-arg (nth and-idx args))
                       (others (append (seq-take args and-idx)
                                       (seq-drop args (1+ and-idx)))))
                  (funcall 'neovm--tp-distribute
                           (cons 'and
                                 (mapcar (lambda (c)
                                           (cons 'or (cons c others)))
                                         (cdr and-arg)))))
              (cons 'or args)))))
       (t f))))

  ;; Full CNF pipeline
  (fset 'neovm--tp-to-cnf
    (lambda (f)
      (funcall 'neovm--tp-distribute
               (funcall 'neovm--tp-nnf
                        (funcall 'neovm--tp-elim-impl f)))))

  (unwind-protect
      (list
       ;; Eliminate implication: (=> P Q) -> (or (not P) Q)
       (funcall 'neovm--tp-elim-impl '(=> P Q))
       ;; Eliminate biconditional
       (funcall 'neovm--tp-elim-impl '(<=> P Q))
       ;; NNF of (not (and P Q)) -> (or (not P) (not Q))
       (funcall 'neovm--tp-nnf '(not (and P Q)))
       ;; NNF of (not (or P Q)) -> (and (not P) (not Q))
       (funcall 'neovm--tp-nnf '(not (or P Q)))
       ;; Double negation elimination
       (funcall 'neovm--tp-nnf '(not (not P)))
       ;; Full CNF: (=> P (and Q R)) -> (and (or (not P) Q) (or (not P) R))
       (funcall 'neovm--tp-to-cnf '(=> P (and Q R)))
       ;; Full CNF: (or A (and B C))
       (funcall 'neovm--tp-to-cnf '(or A (and B C)))
       ;; Full CNF of implication chain
       (funcall 'neovm--tp-to-cnf '(=> (=> P Q) (=> (not Q) (not P)))))
    (fmakunbound 'neovm--tp-elim-impl)
    (fmakunbound 'neovm--tp-nnf)
    (fmakunbound 'neovm--tp-distribute)
    (fmakunbound 'neovm--tp-to-cnf)))"#;
    let expect = expect_test::expect![[
        r#""OK ((or (not P) Q) (and (or (not P) Q) (or (not Q) P)) (or (not P) (not Q)) (and (not P) (not Q)) nil (and (or Q (not P)) (or R (not P))) (and (or B A) (or C A)) (and (or nil (or nil (not P))) (or (not Q) (or nil (not P)))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Unification for first-order logic terms
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_theorem_prover_unification() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Variable check: symbols starting with ?
  (fset 'neovm--tp-var-p
    (lambda (t1) (and (symbolp t1) (string-prefix-p "?" (symbol-name t1)))))

  ;; Occurs check: does var occur in term?
  (fset 'neovm--tp-occurs-p
    (lambda (var term subst)
      (let ((t2 (funcall 'neovm--tp-walk var subst)))
        (cond
         ((equal t2 term) nil)
         ((funcall 'neovm--tp-var-p term)
          (equal t2 (funcall 'neovm--tp-walk term subst)))
         ((consp term)
          (or (funcall 'neovm--tp-occurs-p var (car term) subst)
              (funcall 'neovm--tp-occurs-p var (cdr term) subst)))
         (t nil)))))

  ;; Walk: follow variable bindings in substitution
  (fset 'neovm--tp-walk
    (lambda (term subst)
      (if (funcall 'neovm--tp-var-p term)
          (let ((binding (assq term subst)))
            (if binding
                (funcall 'neovm--tp-walk (cdr binding) subst)
              term))
        term)))

  ;; Unify two terms, returning substitution or 'fail
  (fset 'neovm--tp-unify
    (lambda (t1 t2 subst)
      (if (eq subst 'fail) 'fail
        (let ((s1 (funcall 'neovm--tp-walk t1 subst))
              (s2 (funcall 'neovm--tp-walk t2 subst)))
          (cond
           ((equal s1 s2) subst)
           ((funcall 'neovm--tp-var-p s1)
            (if (funcall 'neovm--tp-occurs-p s1 s2 subst)
                'fail
              (cons (cons s1 s2) subst)))
           ((funcall 'neovm--tp-var-p s2)
            (if (funcall 'neovm--tp-occurs-p s2 s1 subst)
                'fail
              (cons (cons s2 s1) subst)))
           ((and (consp s1) (consp s2))
            (funcall 'neovm--tp-unify
                     (cdr s1) (cdr s2)
                     (funcall 'neovm--tp-unify (car s1) (car s2) subst)))
           (t 'fail))))))

  ;; Apply substitution to a term (walk*)
  (fset 'neovm--tp-apply-subst
    (lambda (term subst)
      (let ((walked (funcall 'neovm--tp-walk term subst)))
        (cond
         ((funcall 'neovm--tp-var-p walked) walked)
         ((consp walked)
          (cons (funcall 'neovm--tp-apply-subst (car walked) subst)
                (funcall 'neovm--tp-apply-subst (cdr walked) subst)))
         (t walked)))))

  (unwind-protect
      (list
       ;; Unify variable with constant
       (funcall 'neovm--tp-unify '?x 'a nil)
       ;; Unify two variables
       (funcall 'neovm--tp-unify '?x '?y nil)
       ;; Unify compound terms: (f ?x b) with (f a ?y)
       (funcall 'neovm--tp-unify '(f ?x b) '(f a ?y) nil)
       ;; Unify with occurs check failure: ?x with (f ?x)
       (funcall 'neovm--tp-unify '?x '(f ?x) nil)
       ;; Unify constants: succeed
       (funcall 'neovm--tp-unify 'a 'a nil)
       ;; Unify constants: fail
       (funcall 'neovm--tp-unify 'a 'b nil)
       ;; Chain unification: ?x=a, then (g ?x) with (g ?y)
       (let ((s1 (funcall 'neovm--tp-unify '?x 'a nil)))
         (funcall 'neovm--tp-unify '(g ?x) '(g ?y) s1))
       ;; Apply substitution
       (let ((subst '((?x . a) (?y . (f ?x)))))
         (funcall 'neovm--tp-apply-subst '(h ?x ?y) subst))
       ;; Complex nested unification
       (funcall 'neovm--tp-unify '(f (g ?x) ?y) '(f (g a) (h ?x)) nil)
       ;; Unify lists of different lengths: fail
       (funcall 'neovm--tp-unify '(f ?x) '(f ?x ?y) nil))
    (fmakunbound 'neovm--tp-var-p)
    (fmakunbound 'neovm--tp-occurs-p)
    (fmakunbound 'neovm--tp-walk)
    (fmakunbound 'neovm--tp-unify)
    (fmakunbound 'neovm--tp-apply-subst)))"#;
    let expect =
        expect_test::expect![[r#""OK (fail fail fail fail nil fail fail (h 120 121) fail fail)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Resolution: resolving two clauses on a complementary literal
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_theorem_prover_resolution() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Propositional resolution: given clauses as lists of literals
    // (integers), resolve on a complementary pair.
    let form = r#"(progn
  ;; Find a complementary literal between two clauses
  ;; Returns the literal in c1 that has its negation in c2, or nil
  (fset 'neovm--tp-find-complement
    (lambda (c1 c2)
      (let ((found nil))
        (dolist (lit c1)
          (when (and (not found) (memq (- lit) c2))
            (setq found lit)))
        found)))

  ;; Resolve two clauses on a given literal
  ;; Result: union of both clauses minus the literal and its negation
  (fset 'neovm--tp-resolve
    (lambda (c1 c2 lit)
      (let ((r1 (remove lit c1))
            (r2 (remove (- lit) c2)))
        ;; Remove duplicates
        (let ((result r1))
          (dolist (l r2)
            (unless (memq l result)
              (push l result)))
          (sort result #'<)))))

  ;; Check if a clause is a tautology (contains both p and -p)
  (fset 'neovm--tp-tautology-p
    (lambda (clause)
      (let ((taut nil))
        (dolist (lit clause)
          (when (memq (- lit) clause)
            (setq taut t)))
        taut)))

  ;; Try all resolutions between two clause sets
  (fset 'neovm--tp-all-resolvents
    (lambda (c1 c2)
      (let ((results nil))
        (let ((comp (funcall 'neovm--tp-find-complement c1 c2)))
          (when comp
            (let ((resolvent (funcall 'neovm--tp-resolve c1 c2 comp)))
              (unless (funcall 'neovm--tp-tautology-p resolvent)
                (push resolvent results)))))
        results)))

  (unwind-protect
      (list
       ;; Find complement between (1 2 3) and (-2 4 5)
       (funcall 'neovm--tp-find-complement '(1 2 3) '(-2 4 5))
       ;; Resolve: {P, Q} and {-P, R} on P -> {Q, R}
       (funcall 'neovm--tp-resolve '(1 2) '(-1 3) 1)
       ;; Resolve: {P} and {-P} -> empty clause (contradiction)
       (funcall 'neovm--tp-resolve '(1) '(-1) 1)
       ;; Resolve: {P, Q, R} and {-Q, S} on Q -> {P, R, S}
       (funcall 'neovm--tp-resolve '(1 2 3) '(-2 4) 2)
       ;; Tautology check: {P, -P, Q}
       (funcall 'neovm--tp-tautology-p '(1 -1 2))
       ;; Not tautology: {P, Q, R}
       (funcall 'neovm--tp-tautology-p '(1 2 3))
       ;; All resolvents
       (funcall 'neovm--tp-all-resolvents '(1 2) '(-1 3))
       ;; No complement
       (funcall 'neovm--tp-find-complement '(1 2) '(3 4))
       ;; Resolve with duplicates in result
       (funcall 'neovm--tp-resolve '(1 2 3) '(-1 2 4) 1))
    (fmakunbound 'neovm--tp-find-complement)
    (fmakunbound 'neovm--tp-resolve)
    (fmakunbound 'neovm--tp-tautology-p)
    (fmakunbound 'neovm--tp-all-resolvents)))"#;
    let expect = expect_test::expect![[r#""OK (2 (2 3) nil (1 3 4) t nil ((2 3)) nil (2 3 4))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Unit propagation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_theorem_prover_unit_propagation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Propagate a unit literal through all clauses:
  ;; - Remove clauses containing the literal (satisfied)
  ;; - Remove the negation from remaining clauses
  (fset 'neovm--tp-propagate-unit
    (lambda (lit clauses)
      (let ((result nil))
        (dolist (clause clauses)
          (cond
           ;; Clause contains the literal: satisfied, remove it
           ((memq lit clause) nil)
           ;; Clause contains the negation: remove negation
           ((memq (- lit) clause)
            (let ((new-clause (remove (- lit) clause)))
              (push new-clause result)))
           ;; Clause unaffected
           (t (push clause result))))
        (nreverse result))))

  ;; Find all unit clauses (clauses with exactly one literal)
  (fset 'neovm--tp-find-units
    (lambda (clauses)
      (let ((units nil))
        (dolist (c clauses)
          (when (= (length c) 1)
            (push (car c) units)))
        (nreverse units))))

  ;; Full unit propagation: repeat until no more unit clauses
  (fset 'neovm--tp-unit-propagate
    (lambda (clauses)
      (let ((cs clauses)
            (assignment nil)
            (changed t)
            (steps 0))
        (while (and changed (< steps 50))
          (setq changed nil)
          (setq steps (1+ steps))
          (let ((units (funcall 'neovm--tp-find-units cs)))
            (when units
              (setq changed t)
              (dolist (u units)
                (push u assignment)
                (setq cs (funcall 'neovm--tp-propagate-unit u cs))))))
        (list cs (nreverse assignment)))))

  (unwind-protect
      (list
       ;; Propagate unit literal 1 through clauses
       ;; {{1,2}, {-1,3}, {2,3}}
       ;; After propagating 1: remove {1,2}, {-1,3} becomes {3}, {2,3} stays
       (funcall 'neovm--tp-propagate-unit 1 '((1 2) (-1 3) (2 3)))
       ;; Find unit clauses
       (funcall 'neovm--tp-find-units '((1) (2 3) (-4) (5 6 7)))
       ;; Full propagation on a simple problem
       ;; {{1}, {-1, 2}, {-2, 3}, {-3, 4}}
       ;; Unit 1 -> removes {1}, {-1,2} becomes {2}
       ;; Unit 2 -> removes {2}, {-2,3} becomes {3}
       ;; Unit 3 -> removes {3}, {-3,4} becomes {4}
       ;; Unit 4 -> removes {4}
       (funcall 'neovm--tp-unit-propagate '((1) (-1 2) (-2 3) (-3 4)))
       ;; Propagation leading to empty clause (contradiction)
       ;; {{1}, {-1}}
       (funcall 'neovm--tp-unit-propagate '((1) (-1)))
       ;; No unit clauses: nothing changes
       (funcall 'neovm--tp-unit-propagate '((1 2) (3 4) (-1 -3)))
       ;; Multiple initial units
       (funcall 'neovm--tp-unit-propagate '((1) (2) (-1 -2 3) (-3 4))))
    (fmakunbound 'neovm--tp-propagate-unit)
    (fmakunbound 'neovm--tp-find-units)
    (fmakunbound 'neovm--tp-unit-propagate)))"#;
    let expect = expect_test::expect![[
        r#""OK (((3) (2 3)) (1 -4) (nil (1 2 3 4)) ((nil) (1 -1)) (((1 2) (3 4) (-1 -3)) nil) (nil (1 2 3 4)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Davis-Putnam (DPLL) procedure for propositional satisfiability
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_theorem_prover_dpll() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--tp-propagate-unit
    (lambda (lit clauses)
      (let ((result nil))
        (dolist (clause clauses)
          (cond
           ((memq lit clause) nil)
           ((memq (- lit) clause)
            (push (remove (- lit) clause) result))
           (t (push clause result))))
        (nreverse result))))

  (fset 'neovm--tp-find-units
    (lambda (clauses)
      (let ((units nil))
        (dolist (c clauses)
          (when (= (length c) 1)
            (push (car c) units)))
        units)))

  ;; Collect all variables from clauses
  (fset 'neovm--tp-variables
    (lambda (clauses)
      (let ((vars nil))
        (dolist (c clauses)
          (dolist (lit c)
            (let ((v (abs lit)))
              (unless (memq v vars)
                (push v vars)))))
        (sort vars #'<))))

  ;; DPLL solver: returns assignment list or nil (unsatisfiable)
  (fset 'neovm--tp-dpll
    (lambda (clauses assignment)
      (cond
       ;; No clauses left: satisfiable
       ((null clauses) assignment)
       ;; Empty clause found: unsatisfiable
       ((memq nil clauses) nil)
       (t
        ;; Unit propagation
        (let ((units (funcall 'neovm--tp-find-units clauses))
              (cs clauses)
              (asgn assignment))
          (while units
            (let ((u (car units)))
              (push u asgn)
              (setq cs (funcall 'neovm--tp-propagate-unit u cs))
              (setq units (funcall 'neovm--tp-find-units cs))))
          (cond
           ((null cs) asgn)
           ((memq nil cs) nil)
           (t
            ;; Choose first variable from first clause
            (let ((var (abs (caar cs))))
              ;; Try positive
              (or (funcall 'neovm--tp-dpll
                           (funcall 'neovm--tp-propagate-unit var cs)
                           (cons var asgn))
                  ;; Try negative
                  (funcall 'neovm--tp-dpll
                           (funcall 'neovm--tp-propagate-unit (- var) cs)
                           (cons (- var) asgn)))))))))))

  (unwind-protect
      (list
       ;; Satisfiable: (P or Q) and (-P or Q) -> Q=true
       (let ((result (funcall 'neovm--tp-dpll '((1 2) (-1 2)) nil)))
         (and result (sort (copy-sequence result) (lambda (a b) (< (abs a) (abs b))))))
       ;; Unsatisfiable: (P) and (-P)
       (funcall 'neovm--tp-dpll '((1) (-1)) nil)
       ;; Satisfiable: (P or Q) and (-P or -Q) and (P or -Q)
       (let ((result (funcall 'neovm--tp-dpll '((1 2) (-1 -2) (1 -2)) nil)))
         (and result t))
       ;; Trivially satisfiable: single positive clause
       (let ((result (funcall 'neovm--tp-dpll '((1)) nil)))
         (sort (copy-sequence result) (lambda (a b) (< (abs a) (abs b)))))
       ;; 3-SAT instance: satisfiable
       ;; (1 or 2 or 3) and (-1 or -2 or 3) and (1 or -2 or -3)
       (let ((result (funcall 'neovm--tp-dpll
                              '((1 2 3) (-1 -2 3) (1 -2 -3)) nil)))
         (and result t))
       ;; Pigeon hole: 2 pigeons, 1 hole -> unsatisfiable
       ;; P1_in_H1 and P2_in_H1 and not(P1_in_H1 and P2_in_H1)
       (funcall 'neovm--tp-dpll '((1) (2) (-1 -2)) nil)
       ;; Variables extraction
       (funcall 'neovm--tp-variables '((1 -2 3) (-1 4) (2 -3 -4))))
    (fmakunbound 'neovm--tp-propagate-unit)
    (fmakunbound 'neovm--tp-find-units)
    (fmakunbound 'neovm--tp-variables)
    (fmakunbound 'neovm--tp-dpll)))"#;
    let expect = expect_test::expect![[r#""OK ((1 2) nil t (1) t nil (1 2 3 4))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Skolemization: removing existential quantifiers
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_theorem_prover_skolemization() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Skolemization replaces existentially quantified variables with
    // Skolem functions of the universally quantified variables in scope.
    let form = r#"(progn
  ;; Generate fresh Skolem function name
  (fset 'neovm--tp-skolem-counter 0)
  (fset 'neovm--tp-fresh-skolem
    (lambda ()
      (fset 'neovm--tp-skolem-counter
            (1+ (symbol-function 'neovm--tp-skolem-counter)))
      (intern (format "sk%d" (symbol-function 'neovm--tp-skolem-counter)))))

  ;; Skolemize a formula
  ;; univ-vars: list of universally quantified variables in scope
  ;; Formula structure: (forall ?x body), (exists ?x body), or nested
  (fset 'neovm--tp-skolemize
    (lambda (formula univ-vars)
      (cond
       ((symbolp formula) formula)
       ((eq (car formula) 'forall)
        (let ((var (cadr formula))
              (body (caddr formula)))
          (funcall 'neovm--tp-skolemize body (cons var univ-vars))))
       ((eq (car formula) 'exists)
        (let* ((var (cadr formula))
               (body (caddr formula))
               (skolem-name (funcall 'neovm--tp-fresh-skolem))
               (skolem-term (if univ-vars
                                (cons skolem-name (reverse univ-vars))
                              skolem-name)))
          ;; Substitute var with skolem-term in body
          (funcall 'neovm--tp-skolemize
                   (funcall 'neovm--tp-subst-term var skolem-term body)
                   univ-vars)))
       ((consp formula)
        (mapcar (lambda (f) (funcall 'neovm--tp-skolemize f univ-vars))
                formula))
       (t formula))))

  ;; Substitute a variable with a term in a formula
  (fset 'neovm--tp-subst-term
    (lambda (var term formula)
      (cond
       ((eq formula var) term)
       ((symbolp formula) formula)
       ((consp formula)
        (mapcar (lambda (f) (funcall 'neovm--tp-subst-term var term f))
                formula))
       (t formula))))

  ;; Reset counter for deterministic testing
  (fset 'neovm--tp-skolem-counter 0)

  (unwind-protect
      (list
       ;; exists x. P(x) -> P(sk1)
       (funcall 'neovm--tp-skolemize '(exists ?x (P ?x)) nil)
       ;; forall x. exists y. R(x,y) -> forall x. R(x, sk2(x))
       ;; But we drop forall, so result is R(x, (sk2 x))
       (funcall 'neovm--tp-skolemize '(forall ?x (exists ?y (R ?x ?y))) nil)
       ;; forall x. forall y. exists z. F(x,y,z)
       ;; -> F(x, y, (sk3 y x))
       (funcall 'neovm--tp-skolemize
        '(forall ?x (forall ?y (exists ?z (F ?x ?y ?z)))) nil)
       ;; Pure universal: no change (just drops quantifier)
       (funcall 'neovm--tp-skolemize '(forall ?x (P ?x)) nil)
       ;; Nested existentials
       (progn
         (fset 'neovm--tp-skolem-counter 10)
         (funcall 'neovm--tp-skolemize
          '(exists ?x (exists ?y (and (P ?x) (Q ?y)))) nil))
       ;; Substitution helper
       (funcall 'neovm--tp-subst-term '?x '(f a) '(P ?x (Q ?x ?y))))
    (fmakunbound 'neovm--tp-skolem-counter)
    (fmakunbound 'neovm--tp-fresh-skolem)
    (fmakunbound 'neovm--tp-skolemize)
    (fmakunbound 'neovm--tp-subst-term)))"#;
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp 120)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Paramodulation: equality reasoning in resolution
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_theorem_prover_paramodulation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Paramodulation applies an equality s=t to rewrite a term
    // in another clause.
    let form = r#"(progn
  ;; Replace all occurrences of `old` with `new` in a term
  (fset 'neovm--tp-replace-term
    (lambda (term old new)
      (cond
       ((equal term old) new)
       ((consp term)
        (mapcar (lambda (t1) (funcall 'neovm--tp-replace-term t1 old new))
                term))
       (t term))))

  ;; Find all subterms of a term
  (fset 'neovm--tp-subterms
    (lambda (term)
      (if (consp term)
          (cons term
                (apply #'append
                       (mapcar 'neovm--tp-subterms (cdr term))))
        (list term))))

  ;; Paramodulate: given an equality (= s t) and a literal,
  ;; produce all possible rewrites of the literal using the equality.
  (fset 'neovm--tp-paramodulate
    (lambda (eq-lhs eq-rhs literal)
      (let ((subs (funcall 'neovm--tp-subterms literal))
            (results nil))
        (dolist (sub subs)
          (when (equal sub eq-lhs)
            (let ((new-lit (funcall 'neovm--tp-replace-term literal eq-lhs eq-rhs)))
              (unless (equal new-lit literal)
                (push new-lit results)))))
        (nreverse results))))

  ;; Demodulation: simplify a term using a set of oriented equations
  (fset 'neovm--tp-demodulate
    (lambda (term equations)
      (let ((changed t) (current term) (steps 0))
        (while (and changed (< steps 20))
          (setq changed nil)
          (setq steps (1+ steps))
          (dolist (eq equations)
            (let* ((lhs (car eq))
                   (rhs (cdr eq))
                   (new (funcall 'neovm--tp-replace-term current lhs rhs)))
              (unless (equal new current)
                (setq current new)
                (setq changed t)))))
        current)))

  (unwind-protect
      (list
       ;; Replace term: (f a b) with a->c => (f c b)
       (funcall 'neovm--tp-replace-term '(f a b) 'a 'c)
       ;; Replace nested: (f (g a) a) with a->x => (f (g x) x)
       (funcall 'neovm--tp-replace-term '(f (g a) a) 'a 'x)
       ;; Subterms of (f (g a) b)
       (funcall 'neovm--tp-subterms '(f (g a) b))
       ;; Paramodulate: a=b applied to P(a)
       (funcall 'neovm--tp-paramodulate 'a 'b '(P a))
       ;; Paramodulate: (f a) = c applied to (Q (f a) (g (f a)))
       (funcall 'neovm--tp-paramodulate '(f a) 'c '(Q (f a) (g (f a))))
       ;; Demodulate with equations: a->b, (f b)->c
       (funcall 'neovm--tp-demodulate
                '(g a (f a))
                '((a . b) ((f b) . c)))
       ;; Demodulate with chain: x->y, y->z
       (funcall 'neovm--tp-demodulate 'x '((x . y) (y . z)))
       ;; Subterms of a constant
       (funcall 'neovm--tp-subterms 'a)
       ;; Replace with compound term
       (funcall 'neovm--tp-replace-term '(P ?x) '?x '(f a)))
    (fmakunbound 'neovm--tp-replace-term)
    (fmakunbound 'neovm--tp-subterms)
    (fmakunbound 'neovm--tp-paramodulate)
    (fmakunbound 'neovm--tp-demodulate)))"#;
    let expect = expect_test::expect![[
        r#""OK ((f c b) (f (g x) x) ((f (g a) b) (g a) a b) ((P b)) ((Q c (g c)) (Q c (g c))) (g b c) z (a) (P (f a)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
