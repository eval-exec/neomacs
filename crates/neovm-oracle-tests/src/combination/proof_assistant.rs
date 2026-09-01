//! Oracle parity tests for proof assistant patterns in Elisp:
//! propositional logic with natural deduction (intro/elim rules),
//! proof tree construction, modus ponens chains, hypothetical reasoning,
//! proof by contradiction, De Morgan transformations, sequent calculus,
//! and proof validation.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Propositional formula evaluation and truth tables
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_proof_formula_evaluation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  ;; Evaluate propositional formulas under a variable assignment (alist)
  ;; Formulas: t | nil | symbol | (not F) | (and F1 F2) | (or F1 F2) | (implies F1 F2)
  (fset 'neovm--pa-eval
    (lambda (formula env)
      (cond
       ((eq formula t) t)
       ((eq formula nil) nil)
       ((symbolp formula)
        (not (null (cdr (assq formula env)))))
       ((eq (car formula) 'not)
        (not (funcall 'neovm--pa-eval (nth 1 formula) env)))
       ((eq (car formula) 'and)
        (and (funcall 'neovm--pa-eval (nth 1 formula) env)
             (funcall 'neovm--pa-eval (nth 2 formula) env)))
       ((eq (car formula) 'or)
        (or (funcall 'neovm--pa-eval (nth 1 formula) env)
            (funcall 'neovm--pa-eval (nth 2 formula) env)))
       ((eq (car formula) 'implies)
        (or (not (funcall 'neovm--pa-eval (nth 1 formula) env))
            (funcall 'neovm--pa-eval (nth 2 formula) env)))
       (t (error "Unknown formula: %S" formula)))))

  ;; Generate all truth assignments for n variables
  (fset 'neovm--pa-all-envs
    (lambda (vars)
      (if (null vars)
          '(nil)
        (let ((rest-envs (funcall 'neovm--pa-all-envs (cdr vars)))
              (v (car vars))
              (result nil))
          (dolist (env rest-envs)
            (push (cons (cons v nil) env) result)
            (push (cons (cons v t) env) result))
          (nreverse result)))))

  ;; Check if formula is a tautology
  (fset 'neovm--pa-tautology-p
    (lambda (formula vars)
      (let ((envs (funcall 'neovm--pa-all-envs vars))
            (all-true t))
        (dolist (env envs)
          (unless (funcall 'neovm--pa-eval formula env)
            (setq all-true nil)))
        all-true)))

  (unwind-protect
      (list
       ;; p -> p is tautology
       (funcall 'neovm--pa-tautology-p '(implies p p) '(p))
       ;; p or (not p) is tautology (excluded middle)
       (funcall 'neovm--pa-tautology-p '(or p (not p)) '(p))
       ;; p and (not p) is NOT tautology
       (funcall 'neovm--pa-tautology-p '(and p (not p)) '(p))
       ;; Modus ponens pattern: ((p -> q) and p) -> q
       (funcall 'neovm--pa-tautology-p
                '(implies (and (implies p q) p) q) '(p q))
       ;; Hypothetical syllogism: ((p->q) and (q->r)) -> (p->r)
       (funcall 'neovm--pa-tautology-p
                '(implies (and (implies p q) (implies q r))
                          (implies p r))
                '(p q r))
       ;; Simple evaluation under specific env
       (funcall 'neovm--pa-eval '(and p (not q))
                '((p . t) (q . nil)))
       (funcall 'neovm--pa-eval '(implies p q)
                '((p . t) (q . nil))))
    (fmakunbound 'neovm--pa-eval)
    (fmakunbound 'neovm--pa-all-envs)
    (fmakunbound 'neovm--pa-tautology-p)))"#;
    let expect = expect_test::expect![[r#""OK (t t nil t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Modus ponens chain: derive conclusions from a knowledge base
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_proof_modus_ponens_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  ;; Knowledge base: list of (implies P Q) and atomic facts
  ;; Forward-chain modus ponens until fixpoint
  (fset 'neovm--pa-member-equal
    (lambda (elt lst)
      (let ((found nil))
        (while (and lst (not found))
          (when (equal elt (car lst))
            (setq found t))
          (setq lst (cdr lst)))
        found)))

  (fset 'neovm--pa-forward-chain
    (lambda (kb)
      (let ((facts nil) (rules nil) (changed t))
        ;; Separate facts from rules
        (dolist (item kb)
          (if (and (listp item) (eq (car item) 'implies))
              (push item rules)
            (push item facts)))
        ;; Iterate until no new facts
        (while changed
          (setq changed nil)
          (dolist (rule rules)
            (let ((antecedent (nth 1 rule))
                  (consequent (nth 2 rule)))
              (when (and (funcall 'neovm--pa-member-equal antecedent facts)
                         (not (funcall 'neovm--pa-member-equal consequent facts)))
                (push consequent facts)
                (setq changed t)))))
        (sort facts (lambda (a b) (string< (format "%S" a) (format "%S" b)))))))

  (unwind-protect
      (list
       ;; Chain: A, A->B, B->C, C->D => derive A,B,C,D
       (funcall 'neovm--pa-forward-chain
                '(A (implies A B) (implies B C) (implies C D)))
       ;; Two independent chains
       (funcall 'neovm--pa-forward-chain
                '(X Y (implies X P) (implies Y Q)))
       ;; Longer chain with branching
       (funcall 'neovm--pa-forward-chain
                '(start
                  (implies start middle)
                  (implies middle left)
                  (implies middle right)
                  (implies left end1)
                  (implies right end2)))
       ;; No applicable rule
       (funcall 'neovm--pa-forward-chain
                '(A (implies B C)))
       ;; Already complete
       (funcall 'neovm--pa-forward-chain '(A B C)))
    (fmakunbound 'neovm--pa-member-equal)
    (fmakunbound 'neovm--pa-forward-chain)))"#;
    let expect = expect_test::expect![[
        r#""OK ((A B C D) (P Q X Y) (end1 end2 left middle right start) (A) (A B C))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// De Morgan transformations and normal form conversion
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_proof_demorgan_transformations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  ;; Push negation inward using De Morgan's laws:
  ;; not(and A B) => or(not A, not B)
  ;; not(or A B) => and(not A, not B)
  ;; not(not A) => A
  ;; not(implies A B) => and(A, not B)
  (fset 'neovm--pa-push-neg
    (lambda (formula)
      (cond
       ((or (eq formula t) (eq formula nil) (symbolp formula))
        formula)
       ((eq (car formula) 'not)
        (let ((inner (nth 1 formula)))
          (cond
           ;; Double negation elimination
           ((and (listp inner) (eq (car inner) 'not))
            (funcall 'neovm--pa-push-neg (nth 1 inner)))
           ;; De Morgan: not(and A B) => or(not A, not B)
           ((and (listp inner) (eq (car inner) 'and))
            (list 'or
                  (funcall 'neovm--pa-push-neg (list 'not (nth 1 inner)))
                  (funcall 'neovm--pa-push-neg (list 'not (nth 2 inner)))))
           ;; De Morgan: not(or A B) => and(not A, not B)
           ((and (listp inner) (eq (car inner) 'or))
            (list 'and
                  (funcall 'neovm--pa-push-neg (list 'not (nth 1 inner)))
                  (funcall 'neovm--pa-push-neg (list 'not (nth 2 inner)))))
           ;; not(implies A B) => and(A, not B)
           ((and (listp inner) (eq (car inner) 'implies))
            (list 'and
                  (funcall 'neovm--pa-push-neg (nth 1 inner))
                  (funcall 'neovm--pa-push-neg (list 'not (nth 2 inner)))))
           ;; Atomic negation stays
           (t formula))))
       ;; Recurse into and/or/implies
       ((eq (car formula) 'and)
        (list 'and
              (funcall 'neovm--pa-push-neg (nth 1 formula))
              (funcall 'neovm--pa-push-neg (nth 2 formula))))
       ((eq (car formula) 'or)
        (list 'or
              (funcall 'neovm--pa-push-neg (nth 1 formula))
              (funcall 'neovm--pa-push-neg (nth 2 formula))))
       ((eq (car formula) 'implies)
        ;; Eliminate implies: A->B => or(not A, B)
        (list 'or
              (funcall 'neovm--pa-push-neg (list 'not (nth 1 formula)))
              (funcall 'neovm--pa-push-neg (nth 2 formula))))
       (t formula))))

  (unwind-protect
      (list
       ;; not(and p q) => (or (not p) (not q))
       (funcall 'neovm--pa-push-neg '(not (and p q)))
       ;; not(or p q) => (and (not p) (not q))
       (funcall 'neovm--pa-push-neg '(not (or p q)))
       ;; Double negation: not(not p) => p
       (funcall 'neovm--pa-push-neg '(not (not p)))
       ;; Triple negation
       (funcall 'neovm--pa-push-neg '(not (not (not p))))
       ;; Implies elimination: (implies p q) => (or (not p) q)
       (funcall 'neovm--pa-push-neg '(implies p q))
       ;; Complex nested: not(implies (and p q) (or r s))
       (funcall 'neovm--pa-push-neg '(not (implies (and p q) (or r s))))
       ;; Already in NNF (no change needed)
       (funcall 'neovm--pa-push-neg '(and (not p) q)))
    (fmakunbound 'neovm--pa-push-neg)))"#;
    let expect = expect_test::expect![[
        r#""OK ((or (not p) (not q)) (and (not p) (not q)) p (not p) (or (not p) q) (and (and p q) (and (not r) (not s))) (and (not p) q))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Proof tree construction and validation (natural deduction style)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_proof_tree_natural_deduction() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  ;; A proof tree node: (rule conclusion . premises)
  ;; Rules: assumption, and-intro, and-elim-l, and-elim-r, or-intro-l, or-intro-r, mp
  ;; Validate: check that each node's conclusion follows from its premises

  (fset 'neovm--pa-validate
    (lambda (proof assumptions)
      (let ((rule (car proof))
            (conclusion (cadr proof))
            (premises (cddr proof)))
        (cond
         ;; Assumption: conclusion must be in assumption set
         ((eq rule 'assumption)
          (not (null (member conclusion assumptions))))

         ;; And-intro: from A and B, derive (and A B)
         ((eq rule 'and-intro)
          (let ((p1 (car premises)) (p2 (cadr premises)))
            (and (funcall 'neovm--pa-validate p1 assumptions)
                 (funcall 'neovm--pa-validate p2 assumptions)
                 (equal conclusion
                        (list 'and (cadr p1) (cadr p2))))))

         ;; And-elim-l: from (and A B), derive A
         ((eq rule 'and-elim-l)
          (let ((p1 (car premises)))
            (and (funcall 'neovm--pa-validate p1 assumptions)
                 (listp (cadr p1))
                 (eq (car (cadr p1)) 'and)
                 (equal conclusion (nth 1 (cadr p1))))))

         ;; And-elim-r: from (and A B), derive B
         ((eq rule 'and-elim-r)
          (let ((p1 (car premises)))
            (and (funcall 'neovm--pa-validate p1 assumptions)
                 (listp (cadr p1))
                 (eq (car (cadr p1)) 'and)
                 (equal conclusion (nth 2 (cadr p1))))))

         ;; Modus ponens: from A and (implies A B), derive B
         ((eq rule 'mp)
          (let ((p-ant (car premises)) (p-imp (cadr premises)))
            (and (funcall 'neovm--pa-validate p-ant assumptions)
                 (funcall 'neovm--pa-validate p-imp assumptions)
                 (listp (cadr p-imp))
                 (eq (car (cadr p-imp)) 'implies)
                 (equal (cadr p-ant) (nth 1 (cadr p-imp)))
                 (equal conclusion (nth 2 (cadr p-imp))))))

         (t nil)))))

  (unwind-protect
      (let ((assumes '(p q (implies p r))))
        (list
         ;; Valid assumption
         (funcall 'neovm--pa-validate
                  '(assumption p) assumes)
         ;; And-intro: from p, q derive (and p q)
         (funcall 'neovm--pa-validate
                  '(and-intro (and p q) (assumption p) (assumption q))
                  assumes)
         ;; And-elim-l: from (and p q) derive p
         (funcall 'neovm--pa-validate
                  '(and-elim-l p
                    (and-intro (and p q) (assumption p) (assumption q)))
                  assumes)
         ;; Modus ponens: from p and (implies p r), derive r
         (funcall 'neovm--pa-validate
                  '(mp r (assumption p) (assumption (implies p r)))
                  assumes)
         ;; Invalid: assumption not in set
         (funcall 'neovm--pa-validate
                  '(assumption z) assumes)
         ;; Invalid: wrong conclusion for and-intro
         (funcall 'neovm--pa-validate
                  '(and-intro (and p r) (assumption p) (assumption q))
                  assumes)))
    (fmakunbound 'neovm--pa-validate)))"#;
    let expect = expect_test::expect![[r#""OK (t t t t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Sequent calculus: Gentzen-style sequent validation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_proof_sequent_calculus() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  ;; Sequent: (gamma . delta) where gamma is list of antecedents, delta is list of succedents
  ;; A sequent gamma |- delta holds iff: conjunction of gamma implies disjunction of delta
  ;; We check this semantically via truth table enumeration

  (fset 'neovm--pa-eval-seq
    (lambda (formula env)
      (cond
       ((eq formula t) t)
       ((eq formula nil) nil)
       ((symbolp formula) (not (null (cdr (assq formula env)))))
       ((eq (car formula) 'not)
        (not (funcall 'neovm--pa-eval-seq (nth 1 formula) env)))
       ((eq (car formula) 'and)
        (and (funcall 'neovm--pa-eval-seq (nth 1 formula) env)
             (funcall 'neovm--pa-eval-seq (nth 2 formula) env)))
       ((eq (car formula) 'or)
        (or (funcall 'neovm--pa-eval-seq (nth 1 formula) env)
            (funcall 'neovm--pa-eval-seq (nth 2 formula) env)))
       (t nil))))

  (fset 'neovm--pa-collect-vars
    (lambda (formula)
      (cond
       ((null formula) nil)
       ((eq formula t) nil)
       ((symbolp formula) (list formula))
       ((listp formula)
        (let ((result nil))
          (dolist (sub (cdr formula))
            (dolist (v (funcall 'neovm--pa-collect-vars sub))
              (unless (memq v result)
                (push v result))))
          result))
       (t nil))))

  (fset 'neovm--pa-all-envs-seq
    (lambda (vars)
      (if (null vars) '(nil)
        (let ((rest (funcall 'neovm--pa-all-envs-seq (cdr vars)))
              (v (car vars)) (result nil))
          (dolist (env rest)
            (push (cons (cons v nil) env) result)
            (push (cons (cons v t) env) result))
          result))))

  ;; Check sequent validity: for all envs, if all gamma true then some delta true
  (fset 'neovm--pa-valid-sequent
    (lambda (gamma delta)
      (let* ((all-formulas (append gamma delta))
             (vars nil)
             (valid t))
        (dolist (f all-formulas)
          (dolist (v (funcall 'neovm--pa-collect-vars f))
            (unless (memq v vars) (push v vars))))
        (let ((envs (funcall 'neovm--pa-all-envs-seq vars)))
          (dolist (env envs)
            (let ((gamma-true t) (delta-true nil))
              (dolist (g gamma)
                (unless (funcall 'neovm--pa-eval-seq g env)
                  (setq gamma-true nil)))
              (when gamma-true
                (dolist (d delta)
                  (when (funcall 'neovm--pa-eval-seq d env)
                    (setq delta-true t)))
                (unless delta-true
                  (setq valid nil))))))
        valid)))

  (unwind-protect
      (list
       ;; Identity: p |- p
       (funcall 'neovm--pa-valid-sequent '(p) '(p))
       ;; Weakening: p |- p, q
       (funcall 'neovm--pa-valid-sequent '(p) '(p q))
       ;; Cut: p, (not p) |- (contradiction, should be valid for any delta)
       (funcall 'neovm--pa-valid-sequent '(p (not p)) '(q))
       ;; p, q |- (and p q)
       (funcall 'neovm--pa-valid-sequent '(p q) '((and p q)))
       ;; (or p q) |- p, q
       (funcall 'neovm--pa-valid-sequent '((or p q)) '(p q))
       ;; Invalid: p |- q (not valid in general)
       (funcall 'neovm--pa-valid-sequent '(p) '(q))
       ;; Invalid: |- p (can't derive p from nothing)
       (funcall 'neovm--pa-valid-sequent nil '(p)))
    (fmakunbound 'neovm--pa-eval-seq)
    (fmakunbound 'neovm--pa-collect-vars)
    (fmakunbound 'neovm--pa-all-envs-seq)
    (fmakunbound 'neovm--pa-valid-sequent)))"#;
    let expect = expect_test::expect![[r#""OK (t t t t t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Proof by contradiction (reductio ad absurdum)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_proof_by_contradiction() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  ;; Prove a formula by adding its negation and checking for contradiction
  ;; using unit propagation on CNF clauses

  ;; Convert implications to disjunctions for CNF-like reasoning
  (fset 'neovm--pa-implies-to-or
    (lambda (f)
      (cond
       ((atom f) f)
       ((eq (car f) 'implies)
        (list 'or (list 'not (funcall 'neovm--pa-implies-to-or (nth 1 f)))
              (funcall 'neovm--pa-implies-to-or (nth 2 f))))
       (t (cons (car f) (mapcar 'neovm--pa-implies-to-or (cdr f)))))))

  ;; Simple unit propagation on a set of literals
  ;; Returns 'contradiction if both p and (not p) found, else the literal set
  (fset 'neovm--pa-propagate
    (lambda (literals)
      (let ((pos nil) (neg nil) (contradiction nil))
        (dolist (lit literals)
          (if (and (listp lit) (eq (car lit) 'not))
              (let ((v (nth 1 lit)))
                (if (memq v pos) (setq contradiction t)
                  (unless (memq v neg) (push v neg))))
            (if (memq lit neg) (setq contradiction t)
              (unless (memq lit pos) (push lit pos)))))
        (if contradiction 'contradiction
          (append pos (mapcar (lambda (v) (list 'not v)) neg))))))

  ;; Proof by contradiction: assume (not conclusion), add to known facts,
  ;; check if contradiction arises
  (fset 'neovm--pa-proof-by-contra
    (lambda (known-facts conclusion)
      (let* ((negated (if (and (listp conclusion) (eq (car conclusion) 'not))
                          (nth 1 conclusion)
                        (list 'not conclusion)))
             (all-literals (cons negated known-facts)))
        (eq (funcall 'neovm--pa-propagate all-literals) 'contradiction))))

  (unwind-protect
      (list
       ;; From p, prove p (adding (not p) contradicts p)
       (funcall 'neovm--pa-proof-by-contra '(p) 'p)
       ;; From p and q, prove p
       (funcall 'neovm--pa-proof-by-contra '(p q) 'p)
       ;; From (not p), prove (not p)
       (funcall 'neovm--pa-proof-by-contra '((not p)) '(not p))
       ;; Cannot prove q from p alone
       (funcall 'neovm--pa-proof-by-contra '(p) 'q)
       ;; Propagation: {p, (not p)} is contradiction
       (funcall 'neovm--pa-propagate '(p (not p)))
       ;; No contradiction in consistent set
       (funcall 'neovm--pa-propagate '(p q (not r))))
    (fmakunbound 'neovm--pa-implies-to-or)
    (fmakunbound 'neovm--pa-propagate)
    (fmakunbound 'neovm--pa-proof-by-contra)))"#;
    let expect = expect_test::expect![[r#""OK (t t t nil contradiction (q p (not r)))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Hypothetical reasoning: conditional proof construction
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_proof_hypothetical_reasoning() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  ;; Build conditional proofs:
  ;; To prove (implies A B), assume A and derive B
  ;; Proof context is a stack of assumption sets

  (fset 'neovm--pa-derive
    (lambda (goal kb depth)
      "Try to derive GOAL from knowledge base KB. DEPTH limits recursion."
      (cond
       ;; Depth limit
       ((<= depth 0) nil)
       ;; Goal is already in KB
       ((let ((found nil))
          (dolist (item kb) (when (equal item goal) (setq found t)))
          found)
        t)
       ;; Goal is (implies A B): assume A, try to derive B
       ((and (listp goal) (eq (car goal) 'implies))
        (let ((a (nth 1 goal)) (b (nth 2 goal)))
          (funcall 'neovm--pa-derive b (cons a kb) (1- depth))))
       ;; Goal is (and A B): derive both
       ((and (listp goal) (eq (car goal) 'and))
        (and (funcall 'neovm--pa-derive (nth 1 goal) kb (1- depth))
             (funcall 'neovm--pa-derive (nth 2 goal) kb (1- depth))))
       ;; Try modus ponens: find (implies X goal) in KB where X is derivable
       (t
        (let ((derived nil))
          (dolist (item kb)
            (when (and (not derived)
                       (listp item)
                       (eq (car item) 'implies)
                       (equal (nth 2 item) goal))
              (when (funcall 'neovm--pa-derive (nth 1 item) kb (1- depth))
                (setq derived t))))
          derived)))))

  (unwind-protect
      (list
       ;; Derive p from {p}
       (funcall 'neovm--pa-derive 'p '(p) 5)
       ;; Derive (implies p p) from {} (conditional proof)
       (funcall 'neovm--pa-derive '(implies p p) nil 5)
       ;; Derive q from {p, (implies p q)}
       (funcall 'neovm--pa-derive 'q '(p (implies p q)) 5)
       ;; Chain: derive r from {p, p->q, q->r}
       (funcall 'neovm--pa-derive 'r '(p (implies p q) (implies q r)) 5)
       ;; Derive (implies p q) from {(implies p q)} — trivial
       (funcall 'neovm--pa-derive '(implies p q) '((implies p q)) 5)
       ;; Derive (and p q) from {p, q}
       (funcall 'neovm--pa-derive '(and p q) '(p q) 5)
       ;; Cannot derive r from {p} alone
       (funcall 'neovm--pa-derive 'r '(p) 5)
       ;; Nested conditional: (implies p (implies q p))
       (funcall 'neovm--pa-derive '(implies p (implies q p)) nil 5))
    (fmakunbound 'neovm--pa-derive)))"#;
    let expect = expect_test::expect![[r#""OK (t t t t t t nil t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
