//! Oracle parity tests for a propositional logic engine in Elisp:
//! formula representation, truth table evaluation, tautology/contradiction/
//! satisfiability checks, formula simplification (double negation,
//! De Morgan's laws), CNF/DNF conversion, and resolution proof method.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

// ---------------------------------------------------------------------------
// Formula evaluation and truth table generation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_logic_engine_eval_and_truth_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Formulas are s-expressions:
    //   atom: 'p, 'q, 'r (propositional variables), t, nil (constants)
    //   (not F), (and F1 F2), (or F1 F2), (implies F1 F2), (iff F1 F2)
    // Environment is an alist: ((p . t) (q . nil) ...)
    let form = r#"(progn
  ;; Evaluate a formula under an environment
  (fset 'neovm--logic-eval
    (lambda (formula env)
      (cond
       ((eq formula t) t)
       ((eq formula nil) nil)
       ((symbolp formula)
        (cdr (assq formula env)))
       ((eq (car formula) 'not)
        (not (funcall 'neovm--logic-eval (cadr formula) env)))
       ((eq (car formula) 'and)
        (and (funcall 'neovm--logic-eval (cadr formula) env)
             (funcall 'neovm--logic-eval (caddr formula) env)))
       ((eq (car formula) 'or)
        (or (funcall 'neovm--logic-eval (cadr formula) env)
            (funcall 'neovm--logic-eval (caddr formula) env)))
       ((eq (car formula) 'implies)
        (or (not (funcall 'neovm--logic-eval (cadr formula) env))
            (funcall 'neovm--logic-eval (caddr formula) env)))
       ((eq (car formula) 'iff)
        (let ((a (funcall 'neovm--logic-eval (cadr formula) env))
              (b (funcall 'neovm--logic-eval (caddr formula) env)))
          (eq (not (not a)) (not (not b)))))
       (t (error "Unknown formula: %S" formula)))))

  ;; Collect free variables from a formula
  (fset 'neovm--logic-vars
    (lambda (formula)
      (cond
       ((eq formula t) nil)
       ((eq formula nil) nil)
       ((symbolp formula) (list formula))
       ((eq (car formula) 'not)
        (funcall 'neovm--logic-vars (cadr formula)))
       (t
        (let ((left (funcall 'neovm--logic-vars (cadr formula)))
              (right (funcall 'neovm--logic-vars (caddr formula)))
              (result nil))
          ;; Union (remove duplicates)
          (dolist (v (append left right))
            (unless (memq v result)
              (setq result (cons v result))))
          (nreverse result))))))

  ;; Generate all truth assignments for a list of variables
  ;; Returns list of alists
  (fset 'neovm--logic-all-envs
    (lambda (vars)
      (if (null vars)
          (list nil)
        (let ((rest (funcall 'neovm--logic-all-envs (cdr vars)))
              (v (car vars))
              (result nil))
          (dolist (env rest)
            (setq result (cons (cons (cons v t) env) result))
            (setq result (cons (cons (cons v nil) env) result)))
          (nreverse result)))))

  ;; Generate truth table: list of (env . result) pairs
  (fset 'neovm--logic-truth-table
    (lambda (formula)
      (let* ((vars (funcall 'neovm--logic-vars formula))
             (envs (funcall 'neovm--logic-all-envs vars)))
        (mapcar (lambda (env)
                  (cons env (if (funcall 'neovm--logic-eval formula env) t nil)))
                envs))))

  (unwind-protect
      (let ((f1 '(and p q))
            (f2 '(or p q))
            (f3 '(implies p q))
            (f4 '(iff p q))
            (f5 '(not p)))
        (list
         ;; Variables
         (funcall 'neovm--logic-vars f1)
         (funcall 'neovm--logic-vars '(implies (and p q) (or r s)))
         ;; Evaluate under specific environment
         (funcall 'neovm--logic-eval f1 '((p . t) (q . t)))
         (funcall 'neovm--logic-eval f1 '((p . t) (q . nil)))
         (funcall 'neovm--logic-eval f3 '((p . nil) (q . nil)))
         (funcall 'neovm--logic-eval f3 '((p . t) (q . nil)))
         ;; Truth tables (simplified: just results column)
         (mapcar #'cdr (funcall 'neovm--logic-truth-table f1))
         (mapcar #'cdr (funcall 'neovm--logic-truth-table f2))
         (mapcar #'cdr (funcall 'neovm--logic-truth-table f3))
         (mapcar #'cdr (funcall 'neovm--logic-truth-table f4))
         (mapcar #'cdr (funcall 'neovm--logic-truth-table f5))))
    (fmakunbound 'neovm--logic-eval)
    (fmakunbound 'neovm--logic-vars)
    (fmakunbound 'neovm--logic-all-envs)
    (fmakunbound 'neovm--logic-truth-table)))"#;
    let expect = expect_test::expect![[
        r#""OK ((p q) (p q r s) t nil t nil (t nil nil nil) (t t t nil) (t t nil t) (t nil nil t) (nil t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Tautology, contradiction, and satisfiability checks
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_logic_engine_taut_contra_sat() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--logic-eval
    (lambda (formula env)
      (cond
       ((eq formula t) t)
       ((eq formula nil) nil)
       ((symbolp formula) (cdr (assq formula env)))
       ((eq (car formula) 'not)
        (not (funcall 'neovm--logic-eval (cadr formula) env)))
       ((eq (car formula) 'and)
        (and (funcall 'neovm--logic-eval (cadr formula) env)
             (funcall 'neovm--logic-eval (caddr formula) env)))
       ((eq (car formula) 'or)
        (or (funcall 'neovm--logic-eval (cadr formula) env)
            (funcall 'neovm--logic-eval (caddr formula) env)))
       ((eq (car formula) 'implies)
        (or (not (funcall 'neovm--logic-eval (cadr formula) env))
            (funcall 'neovm--logic-eval (caddr formula) env)))
       ((eq (car formula) 'iff)
        (let ((a (funcall 'neovm--logic-eval (cadr formula) env))
              (b (funcall 'neovm--logic-eval (caddr formula) env)))
          (eq (not (not a)) (not (not b))))))))

  (fset 'neovm--logic-vars
    (lambda (formula)
      (cond
       ((eq formula t) nil)
       ((eq formula nil) nil)
       ((symbolp formula) (list formula))
       ((eq (car formula) 'not)
        (funcall 'neovm--logic-vars (cadr formula)))
       (t (let ((left (funcall 'neovm--logic-vars (cadr formula)))
                (right (funcall 'neovm--logic-vars (caddr formula)))
                (result nil))
            (dolist (v (append left right))
              (unless (memq v result) (setq result (cons v result))))
            (nreverse result))))))

  (fset 'neovm--logic-all-envs
    (lambda (vars)
      (if (null vars) (list nil)
        (let ((rest (funcall 'neovm--logic-all-envs (cdr vars)))
              (v (car vars)) (result nil))
          (dolist (env rest)
            (setq result (cons (cons (cons v t) env) result))
            (setq result (cons (cons (cons v nil) env) result)))
          (nreverse result)))))

  ;; Check if tautology (true for all assignments)
  (fset 'neovm--logic-tautology-p
    (lambda (formula)
      (let* ((vars (funcall 'neovm--logic-vars formula))
             (envs (funcall 'neovm--logic-all-envs vars))
             (all-true t))
        (dolist (env envs)
          (unless (funcall 'neovm--logic-eval formula env)
            (setq all-true nil)))
        all-true)))

  ;; Check if contradiction (false for all assignments)
  (fset 'neovm--logic-contradiction-p
    (lambda (formula)
      (let* ((vars (funcall 'neovm--logic-vars formula))
             (envs (funcall 'neovm--logic-all-envs vars))
             (all-false t))
        (dolist (env envs)
          (when (funcall 'neovm--logic-eval formula env)
            (setq all-false nil)))
        all-false)))

  ;; Check if satisfiable (true for at least one assignment)
  (fset 'neovm--logic-satisfiable-p
    (lambda (formula)
      (let* ((vars (funcall 'neovm--logic-vars formula))
             (envs (funcall 'neovm--logic-all-envs vars))
             (found nil))
        (dolist (env envs)
          (when (funcall 'neovm--logic-eval formula env)
            (setq found t)))
        found)))

  ;; Find a satisfying assignment (or nil)
  (fset 'neovm--logic-find-sat
    (lambda (formula)
      (let* ((vars (funcall 'neovm--logic-vars formula))
             (envs (funcall 'neovm--logic-all-envs vars))
             (result nil))
        (dolist (env envs)
          (when (and (not result) (funcall 'neovm--logic-eval formula env))
            (setq result env)))
        result)))

  (unwind-protect
      (list
       ;; Tautologies
       (funcall 'neovm--logic-tautology-p '(or p (not p)))
       (funcall 'neovm--logic-tautology-p '(implies p p))
       (funcall 'neovm--logic-tautology-p '(implies (and p q) p))
       (funcall 'neovm--logic-tautology-p
                '(implies (implies p q)
                          (implies (not q) (not p))))
       ;; Non-tautologies
       (funcall 'neovm--logic-tautology-p '(and p q))
       (funcall 'neovm--logic-tautology-p '(implies p q))
       ;; Contradictions
       (funcall 'neovm--logic-contradiction-p '(and p (not p)))
       (funcall 'neovm--logic-contradiction-p
                '(and (or p q) (and (not p) (not q))))
       ;; Non-contradictions
       (funcall 'neovm--logic-contradiction-p '(or p q))
       (funcall 'neovm--logic-contradiction-p '(and p q))
       ;; Satisfiability
       (funcall 'neovm--logic-satisfiable-p '(and p q))
       (funcall 'neovm--logic-satisfiable-p '(and p (not p)))
       (funcall 'neovm--logic-satisfiable-p
                '(and (or p q) (and (not p) q)))
       ;; Find satisfying assignment
       (let ((sat (funcall 'neovm--logic-find-sat '(and p (not q)))))
         (list (cdr (assq 'p sat)) (cdr (assq 'q sat)))))
    (fmakunbound 'neovm--logic-eval)
    (fmakunbound 'neovm--logic-vars)
    (fmakunbound 'neovm--logic-all-envs)
    (fmakunbound 'neovm--logic-tautology-p)
    (fmakunbound 'neovm--logic-contradiction-p)
    (fmakunbound 'neovm--logic-satisfiable-p)
    (fmakunbound 'neovm--logic-find-sat)))"#;
    let expect = expect_test::expect![[r#""OK (t t t t nil nil t t nil nil t nil t (t nil))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Formula simplification: double negation, De Morgan's laws, constants
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_logic_engine_simplify() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Simplify a formula using algebraic laws
  (fset 'neovm--logic-simplify
    (lambda (formula)
      (cond
       ;; Atoms
       ((eq formula t) t)
       ((eq formula nil) nil)
       ((symbolp formula) formula)
       ;; NOT
       ((eq (car formula) 'not)
        (let ((inner (funcall 'neovm--logic-simplify (cadr formula))))
          (cond
           ;; Double negation: (not (not A)) -> A
           ((and (consp inner) (eq (car inner) 'not))
            (cadr inner))
           ;; (not t) -> nil, (not nil) -> t
           ((eq inner t) nil)
           ((eq inner nil) t)
           ;; De Morgan: (not (and A B)) -> (or (not A) (not B))
           ((and (consp inner) (eq (car inner) 'and))
            (funcall 'neovm--logic-simplify
                     (list 'or
                           (list 'not (cadr inner))
                           (list 'not (caddr inner)))))
           ;; De Morgan: (not (or A B)) -> (and (not A) (not B))
           ((and (consp inner) (eq (car inner) 'or))
            (funcall 'neovm--logic-simplify
                     (list 'and
                           (list 'not (cadr inner))
                           (list 'not (caddr inner)))))
           (t (list 'not inner)))))
       ;; AND
       ((eq (car formula) 'and)
        (let ((a (funcall 'neovm--logic-simplify (cadr formula)))
              (b (funcall 'neovm--logic-simplify (caddr formula))))
          (cond
           ((eq a nil) nil)
           ((eq b nil) nil)
           ((eq a t) b)
           ((eq b t) a)
           ((equal a b) a)
           ;; (and A (not A)) -> nil
           ((and (consp b) (eq (car b) 'not) (equal a (cadr b))) nil)
           ((and (consp a) (eq (car a) 'not) (equal b (cadr a))) nil)
           (t (list 'and a b)))))
       ;; OR
       ((eq (car formula) 'or)
        (let ((a (funcall 'neovm--logic-simplify (cadr formula)))
              (b (funcall 'neovm--logic-simplify (caddr formula))))
          (cond
           ((eq a t) t)
           ((eq b t) t)
           ((eq a nil) b)
           ((eq b nil) a)
           ((equal a b) a)
           ;; (or A (not A)) -> t
           ((and (consp b) (eq (car b) 'not) (equal a (cadr b))) t)
           ((and (consp a) (eq (car a) 'not) (equal b (cadr a))) t)
           (t (list 'or a b)))))
       ;; IMPLIES: (implies A B) -> (or (not A) B)
       ((eq (car formula) 'implies)
        (funcall 'neovm--logic-simplify
                 (list 'or
                       (list 'not (cadr formula))
                       (caddr formula))))
       ;; IFF: (iff A B) -> (and (implies A B) (implies B A))
       ((eq (car formula) 'iff)
        (funcall 'neovm--logic-simplify
                 (list 'and
                       (list 'implies (cadr formula) (caddr formula))
                       (list 'implies (caddr formula) (cadr formula)))))
       (t formula))))

  (unwind-protect
      (list
       ;; Double negation
       (funcall 'neovm--logic-simplify '(not (not p)))
       (funcall 'neovm--logic-simplify '(not (not (not p))))
       ;; Constants
       (funcall 'neovm--logic-simplify '(and p t))
       (funcall 'neovm--logic-simplify '(and p nil))
       (funcall 'neovm--logic-simplify '(or p t))
       (funcall 'neovm--logic-simplify '(or p nil))
       ;; De Morgan's
       (funcall 'neovm--logic-simplify '(not (and p q)))
       (funcall 'neovm--logic-simplify '(not (or p q)))
       ;; Complementary
       (funcall 'neovm--logic-simplify '(and p (not p)))
       (funcall 'neovm--logic-simplify '(or p (not p)))
       ;; Idempotent
       (funcall 'neovm--logic-simplify '(and p p))
       (funcall 'neovm--logic-simplify '(or p p))
       ;; Implies elimination
       (funcall 'neovm--logic-simplify '(implies p q))
       ;; Iff elimination
       (funcall 'neovm--logic-simplify '(iff p q))
       ;; Complex nested
       (funcall 'neovm--logic-simplify
                '(not (and (not (not p)) (not q))))
       ;; (not (and p (not q))) -> (or (not p) q)
       (funcall 'neovm--logic-simplify '(not (and p (not q)))))
    (fmakunbound 'neovm--logic-simplify)))"#;
    let expect = expect_test::expect![[
        r#""OK (p (not p) p nil t p (or (not p) (not q)) (and (not p) (not q)) nil t p p (or (not p) q) (and (or (not p) q) (or (not q) p)) (or (not p) q) (or (not p) q))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// CNF conversion (Conjunctive Normal Form)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_logic_engine_cnf() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Convert to CNF: eliminate implies/iff, push NOT inward (NNF),
    // then distribute OR over AND.
    let form = r#"(progn
  ;; Step 1: Eliminate implies and iff
  (fset 'neovm--logic-elim-impl
    (lambda (f)
      (cond
       ((symbolp f) f)
       ((eq f t) t)
       ((eq f nil) nil)
       ((eq (car f) 'not)
        (list 'not (funcall 'neovm--logic-elim-impl (cadr f))))
       ((eq (car f) 'implies)
        (list 'or
              (list 'not (funcall 'neovm--logic-elim-impl (cadr f)))
              (funcall 'neovm--logic-elim-impl (caddr f))))
       ((eq (car f) 'iff)
        (let ((a (funcall 'neovm--logic-elim-impl (cadr f)))
              (b (funcall 'neovm--logic-elim-impl (caddr f))))
          (list 'and
                (list 'or (list 'not a) b)
                (list 'or (list 'not b) a))))
       (t (list (car f)
                (funcall 'neovm--logic-elim-impl (cadr f))
                (funcall 'neovm--logic-elim-impl (caddr f)))))))

  ;; Step 2: Push NOT inward (Negation Normal Form)
  (fset 'neovm--logic-nnf
    (lambda (f)
      (cond
       ((symbolp f) f)
       ((eq f t) t)
       ((eq f nil) nil)
       ((eq (car f) 'not)
        (let ((inner (cadr f)))
          (cond
           ((symbolp inner) f)
           ((eq inner t) nil)
           ((eq inner nil) t)
           ;; Double negation
           ((eq (car inner) 'not)
            (funcall 'neovm--logic-nnf (cadr inner)))
           ;; De Morgan
           ((eq (car inner) 'and)
            (list 'or
                  (funcall 'neovm--logic-nnf (list 'not (cadr inner)))
                  (funcall 'neovm--logic-nnf (list 'not (caddr inner)))))
           ((eq (car inner) 'or)
            (list 'and
                  (funcall 'neovm--logic-nnf (list 'not (cadr inner)))
                  (funcall 'neovm--logic-nnf (list 'not (caddr inner)))))
           (t f))))
       (t (list (car f)
                (funcall 'neovm--logic-nnf (cadr f))
                (funcall 'neovm--logic-nnf (caddr f)))))))

  ;; Step 3: Distribute OR over AND
  ;; (or A (and B C)) -> (and (or A B) (or A C))
  (fset 'neovm--logic-dist-or
    (lambda (f)
      (cond
       ((symbolp f) f)
       ((eq f t) t)
       ((eq f nil) nil)
       ((eq (car f) 'not) f)
       ((eq (car f) 'and)
        (list 'and
              (funcall 'neovm--logic-dist-or (cadr f))
              (funcall 'neovm--logic-dist-or (caddr f))))
       ((eq (car f) 'or)
        (let ((a (funcall 'neovm--logic-dist-or (cadr f)))
              (b (funcall 'neovm--logic-dist-or (caddr f))))
          (cond
           ((and (consp a) (eq (car a) 'and))
            (list 'and
                  (funcall 'neovm--logic-dist-or (list 'or (cadr a) b))
                  (funcall 'neovm--logic-dist-or (list 'or (caddr a) b))))
           ((and (consp b) (eq (car b) 'and))
            (list 'and
                  (funcall 'neovm--logic-dist-or (list 'or a (cadr b)))
                  (funcall 'neovm--logic-dist-or (list 'or a (caddr b)))))
           (t (list 'or a b)))))
       (t f))))

  ;; Full CNF conversion
  (fset 'neovm--logic-to-cnf
    (lambda (f)
      (funcall 'neovm--logic-dist-or
               (funcall 'neovm--logic-nnf
                        (funcall 'neovm--logic-elim-impl f)))))

  (unwind-protect
      (list
       ;; Simple cases
       (funcall 'neovm--logic-to-cnf 'p)
       (funcall 'neovm--logic-to-cnf '(not p))
       (funcall 'neovm--logic-to-cnf '(and p q))
       (funcall 'neovm--logic-to-cnf '(or p q))
       ;; Implies -> CNF
       (funcall 'neovm--logic-to-cnf '(implies p q))
       ;; (or p (and q r)) -> (and (or p q) (or p r))
       (funcall 'neovm--logic-to-cnf '(or p (and q r)))
       ;; Iff -> CNF
       (funcall 'neovm--logic-to-cnf '(iff p q))
       ;; Complex: (implies (and p q) r)
       (funcall 'neovm--logic-to-cnf '(implies (and p q) r))
       ;; De Morgan through CNF
       (funcall 'neovm--logic-to-cnf '(not (and p q))))
    (fmakunbound 'neovm--logic-elim-impl)
    (fmakunbound 'neovm--logic-nnf)
    (fmakunbound 'neovm--logic-dist-or)
    (fmakunbound 'neovm--logic-to-cnf)))"#;
    let expect = expect_test::expect![[
        r#""OK (p (not p) (and p q) (or p q) (or (not p) q) (and (or p q) (or p r)) (and (or (not p) q) (or (not q) p)) (or (or (not p) (not q)) r) (or (not p) (not q)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// DNF conversion (Disjunctive Normal Form)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_logic_engine_dnf() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // DNF: distribute AND over OR
    // (and A (or B C)) -> (or (and A B) (and A C))
    let form = r#"(progn
  (fset 'neovm--logic-elim-impl
    (lambda (f)
      (cond
       ((symbolp f) f) ((eq f t) t) ((eq f nil) nil)
       ((eq (car f) 'not) (list 'not (funcall 'neovm--logic-elim-impl (cadr f))))
       ((eq (car f) 'implies)
        (list 'or (list 'not (funcall 'neovm--logic-elim-impl (cadr f)))
              (funcall 'neovm--logic-elim-impl (caddr f))))
       ((eq (car f) 'iff)
        (let ((a (funcall 'neovm--logic-elim-impl (cadr f)))
              (b (funcall 'neovm--logic-elim-impl (caddr f))))
          (list 'and (list 'or (list 'not a) b) (list 'or (list 'not b) a))))
       (t (list (car f) (funcall 'neovm--logic-elim-impl (cadr f))
                (funcall 'neovm--logic-elim-impl (caddr f)))))))

  (fset 'neovm--logic-nnf
    (lambda (f)
      (cond
       ((symbolp f) f) ((eq f t) t) ((eq f nil) nil)
       ((eq (car f) 'not)
        (let ((inner (cadr f)))
          (cond
           ((symbolp inner) f) ((eq inner t) nil) ((eq inner nil) t)
           ((eq (car inner) 'not) (funcall 'neovm--logic-nnf (cadr inner)))
           ((eq (car inner) 'and)
            (list 'or (funcall 'neovm--logic-nnf (list 'not (cadr inner)))
                  (funcall 'neovm--logic-nnf (list 'not (caddr inner)))))
           ((eq (car inner) 'or)
            (list 'and (funcall 'neovm--logic-nnf (list 'not (cadr inner)))
                  (funcall 'neovm--logic-nnf (list 'not (caddr inner)))))
           (t f))))
       (t (list (car f) (funcall 'neovm--logic-nnf (cadr f))
                (funcall 'neovm--logic-nnf (caddr f)))))))

  ;; Distribute AND over OR for DNF
  (fset 'neovm--logic-dist-and
    (lambda (f)
      (cond
       ((symbolp f) f) ((eq f t) t) ((eq f nil) nil)
       ((eq (car f) 'not) f)
       ((eq (car f) 'or)
        (list 'or (funcall 'neovm--logic-dist-and (cadr f))
              (funcall 'neovm--logic-dist-and (caddr f))))
       ((eq (car f) 'and)
        (let ((a (funcall 'neovm--logic-dist-and (cadr f)))
              (b (funcall 'neovm--logic-dist-and (caddr f))))
          (cond
           ((and (consp a) (eq (car a) 'or))
            (list 'or
                  (funcall 'neovm--logic-dist-and (list 'and (cadr a) b))
                  (funcall 'neovm--logic-dist-and (list 'and (caddr a) b))))
           ((and (consp b) (eq (car b) 'or))
            (list 'or
                  (funcall 'neovm--logic-dist-and (list 'and a (cadr b)))
                  (funcall 'neovm--logic-dist-and (list 'and a (caddr b)))))
           (t (list 'and a b)))))
       (t f))))

  (fset 'neovm--logic-to-dnf
    (lambda (f)
      (funcall 'neovm--logic-dist-and
               (funcall 'neovm--logic-nnf
                        (funcall 'neovm--logic-elim-impl f)))))

  (unwind-protect
      (list
       ;; Simple
       (funcall 'neovm--logic-to-dnf 'p)
       (funcall 'neovm--logic-to-dnf '(or p q))
       (funcall 'neovm--logic-to-dnf '(and p q))
       ;; (and p (or q r)) -> (or (and p q) (and p r))
       (funcall 'neovm--logic-to-dnf '(and p (or q r)))
       ;; (and (or a b) (or c d)) ->
       ;; (or (or (and a c) (and a d)) (or (and b c) (and b d)))
       (funcall 'neovm--logic-to-dnf '(and (or a b) (or c d)))
       ;; implies in DNF
       (funcall 'neovm--logic-to-dnf '(implies p q))
       ;; Negation + DNF
       (funcall 'neovm--logic-to-dnf '(not (or p q))))
    (fmakunbound 'neovm--logic-elim-impl)
    (fmakunbound 'neovm--logic-nnf)
    (fmakunbound 'neovm--logic-dist-and)
    (fmakunbound 'neovm--logic-to-dnf)))"#;
    let expect = expect_test::expect![[
        r#""OK (p (or p q) (and p q) (or (and p q) (and p r)) (or (or (and a c) (and a d)) (or (and b c) (and b d))) (or (not p) q) (and (not p) (not q)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Resolution proof method
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_logic_engine_resolution() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Resolution: given a set of clauses (lists of literals),
    // repeatedly resolve pairs to derive new clauses.
    // If empty clause is derived, the set is unsatisfiable.
    // A literal is a symbol or (not symbol).
    let form = r#"(progn
  ;; Check if two literals are complementary
  (fset 'neovm--logic-complement-p
    (lambda (a b)
      (or (and (symbolp a) (consp b) (eq (car b) 'not) (eq a (cadr b)))
          (and (consp a) (eq (car a) 'not) (symbolp b) (eq (cadr a) b)))))

  ;; Negate a literal
  (fset 'neovm--logic-negate-lit
    (lambda (lit)
      (if (consp lit) (cadr lit) (list 'not lit))))

  ;; Remove an element from a list (by equal)
  (fset 'neovm--logic-remove
    (lambda (elt lst)
      (let ((result nil))
        (dolist (x lst)
          (unless (equal x elt)
            (setq result (cons x result))))
        (nreverse result))))

  ;; Remove duplicates
  (fset 'neovm--logic-unique
    (lambda (lst)
      (let ((result nil))
        (dolist (x lst)
          (unless (member x result)
            (setq result (cons x result))))
        (nreverse result))))

  ;; Resolve two clauses on a complementary literal pair
  ;; Returns the resolvent clause, or 'no-resolve if no complement found
  (fset 'neovm--logic-resolve-pair
    (lambda (c1 c2)
      (let ((result 'no-resolve))
        (dolist (lit c1)
          (when (eq result 'no-resolve)
            (dolist (lit2 c2)
              (when (and (eq result 'no-resolve)
                         (funcall 'neovm--logic-complement-p lit lit2))
                (let ((new-clause
                       (funcall 'neovm--logic-unique
                                (append (funcall 'neovm--logic-remove lit c1)
                                        (funcall 'neovm--logic-remove lit2 c2)))))
                  (setq result new-clause))))))
        result)))

  ;; Check if a clause set contains the empty clause
  (fset 'neovm--logic-has-empty-p
    (lambda (clauses)
      (let ((found nil))
        (dolist (c clauses)
          (when (null c) (setq found t)))
        found)))

  ;; Check if clause is already in set
  (fset 'neovm--logic-clause-in-set-p
    (lambda (clause clauses)
      (let ((sorted (sort (copy-sequence clause)
                          (lambda (a b)
                            (string< (format "%S" a) (format "%S" b)))))
            (found nil))
        (dolist (c clauses)
          (let ((sc (sort (copy-sequence c)
                          (lambda (a b)
                            (string< (format "%S" a) (format "%S" b))))))
            (when (equal sorted sc) (setq found t))))
        found)))

  ;; Resolution: try to derive empty clause (limited steps)
  (fset 'neovm--logic-resolution
    (lambda (clauses)
      (let ((all clauses)
            (steps 0)
            (max-steps 50)
            (done nil)
            (result 'unknown))
        (while (and (not done) (< steps max-steps))
          (setq steps (1+ steps))
          (let ((new-clauses nil)
                (found-empty nil))
            (let ((i 0))
              (while (and (not found-empty) (< i (length all)))
                (let ((j (1+ i)))
                  (while (and (not found-empty) (< j (length all)))
                    (let ((resolvent (funcall 'neovm--logic-resolve-pair
                                              (nth i all) (nth j all))))
                      (when (not (eq resolvent 'no-resolve))
                        (if (null resolvent)
                            (setq found-empty t)
                          (unless (funcall 'neovm--logic-clause-in-set-p
                                           resolvent all)
                            (unless (funcall 'neovm--logic-clause-in-set-p
                                             resolvent new-clauses)
                              (setq new-clauses (cons resolvent new-clauses)))))))
                    (setq j (1+ j))))
                (setq i (1+ i))))
            (cond
             (found-empty (setq result 'unsatisfiable done t))
             ((null new-clauses) (setq result 'satisfiable done t))
             (t (setq all (append all (nreverse new-clauses)))))))
        (list result steps))))

  (unwind-protect
      (list
       ;; {p, ~p} -> empty clause (unsatisfiable)
       (funcall 'neovm--logic-resolution '((p) ((not p))))
       ;; {p or q, ~p, ~q} -> unsatisfiable
       (funcall 'neovm--logic-resolution '((p q) ((not p)) ((not q))))
       ;; {p or q, ~p or q, p or ~q, ~p or ~q} -> unsatisfiable
       (funcall 'neovm--logic-resolution
                '((p q) ((not p) q) (p (not q)) ((not p) (not q))))
       ;; {p, q} -> satisfiable (no contradictions)
       (funcall 'neovm--logic-resolution '((p) (q)))
       ;; {p or q, ~p or r} -> can derive {q, r} -> satisfiable
       (funcall 'neovm--logic-resolution '((p q) ((not p) r)))
       ;; Single complementary pair resolution
       (funcall 'neovm--logic-resolve-pair '(p q) '((not p) r))
       (funcall 'neovm--logic-resolve-pair '(p) '((not p)))
       ;; No complementary literals
       (funcall 'neovm--logic-resolve-pair '(p q) '(r s)))
    (fmakunbound 'neovm--logic-complement-p)
    (fmakunbound 'neovm--logic-negate-lit)
    (fmakunbound 'neovm--logic-remove)
    (fmakunbound 'neovm--logic-unique)
    (fmakunbound 'neovm--logic-resolve-pair)
    (fmakunbound 'neovm--logic-has-empty-p)
    (fmakunbound 'neovm--logic-clause-in-set-p)
    (fmakunbound 'neovm--logic-resolution)))"#;
    let expect = expect_test::expect![[
        r#""OK ((unsatisfiable 1) (unsatisfiable 2) (unsatisfiable 2) (satisfiable 1) (satisfiable 2) (q r) nil no-resolve)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// End-to-end: simplify + verify equivalence via truth table
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_logic_engine_simplify_verify() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simplify formulas and verify the simplified version is logically
    // equivalent to the original by comparing truth tables.
    let form = r#"(progn
  (fset 'neovm--logic-eval
    (lambda (formula env)
      (cond
       ((eq formula t) t)
       ((eq formula nil) nil)
       ((symbolp formula) (cdr (assq formula env)))
       ((eq (car formula) 'not)
        (not (funcall 'neovm--logic-eval (cadr formula) env)))
       ((eq (car formula) 'and)
        (and (funcall 'neovm--logic-eval (cadr formula) env)
             (funcall 'neovm--logic-eval (caddr formula) env)))
       ((eq (car formula) 'or)
        (or (funcall 'neovm--logic-eval (cadr formula) env)
            (funcall 'neovm--logic-eval (caddr formula) env)))
       ((eq (car formula) 'implies)
        (or (not (funcall 'neovm--logic-eval (cadr formula) env))
            (funcall 'neovm--logic-eval (caddr formula) env)))
       ((eq (car formula) 'iff)
        (let ((a (funcall 'neovm--logic-eval (cadr formula) env))
              (b (funcall 'neovm--logic-eval (caddr formula) env)))
          (eq (not (not a)) (not (not b))))))))

  (fset 'neovm--logic-vars
    (lambda (formula)
      (cond
       ((eq formula t) nil) ((eq formula nil) nil)
       ((symbolp formula) (list formula))
       ((eq (car formula) 'not) (funcall 'neovm--logic-vars (cadr formula)))
       (t (let ((left (funcall 'neovm--logic-vars (cadr formula)))
                (right (funcall 'neovm--logic-vars (caddr formula)))
                (result nil))
            (dolist (v (append left right))
              (unless (memq v result) (setq result (cons v result))))
            (nreverse result))))))

  (fset 'neovm--logic-all-envs
    (lambda (vars)
      (if (null vars) (list nil)
        (let ((rest (funcall 'neovm--logic-all-envs (cdr vars)))
              (v (car vars)) (result nil))
          (dolist (env rest)
            (setq result (cons (cons (cons v t) env) result))
            (setq result (cons (cons (cons v nil) env) result)))
          (nreverse result)))))

  (fset 'neovm--logic-simplify
    (lambda (formula)
      (cond
       ((eq formula t) t) ((eq formula nil) nil) ((symbolp formula) formula)
       ((eq (car formula) 'not)
        (let ((inner (funcall 'neovm--logic-simplify (cadr formula))))
          (cond
           ((and (consp inner) (eq (car inner) 'not)) (cadr inner))
           ((eq inner t) nil) ((eq inner nil) t)
           ((and (consp inner) (eq (car inner) 'and))
            (funcall 'neovm--logic-simplify
                     (list 'or (list 'not (cadr inner)) (list 'not (caddr inner)))))
           ((and (consp inner) (eq (car inner) 'or))
            (funcall 'neovm--logic-simplify
                     (list 'and (list 'not (cadr inner)) (list 'not (caddr inner)))))
           (t (list 'not inner)))))
       ((eq (car formula) 'and)
        (let ((a (funcall 'neovm--logic-simplify (cadr formula)))
              (b (funcall 'neovm--logic-simplify (caddr formula))))
          (cond
           ((eq a nil) nil) ((eq b nil) nil)
           ((eq a t) b) ((eq b t) a) ((equal a b) a)
           ((and (consp b) (eq (car b) 'not) (equal a (cadr b))) nil)
           ((and (consp a) (eq (car a) 'not) (equal b (cadr a))) nil)
           (t (list 'and a b)))))
       ((eq (car formula) 'or)
        (let ((a (funcall 'neovm--logic-simplify (cadr formula)))
              (b (funcall 'neovm--logic-simplify (caddr formula))))
          (cond
           ((eq a t) t) ((eq b t) t)
           ((eq a nil) b) ((eq b nil) a) ((equal a b) a)
           ((and (consp b) (eq (car b) 'not) (equal a (cadr b))) t)
           ((and (consp a) (eq (car a) 'not) (equal b (cadr a))) t)
           (t (list 'or a b)))))
       ((eq (car formula) 'implies)
        (funcall 'neovm--logic-simplify
                 (list 'or (list 'not (cadr formula)) (caddr formula))))
       ((eq (car formula) 'iff)
        (funcall 'neovm--logic-simplify
                 (list 'and
                       (list 'implies (cadr formula) (caddr formula))
                       (list 'implies (caddr formula) (cadr formula)))))
       (t formula))))

  ;; Check logical equivalence via truth table
  (fset 'neovm--logic-equiv-p
    (lambda (f1 f2)
      (let* ((vars1 (funcall 'neovm--logic-vars f1))
             (vars2 (funcall 'neovm--logic-vars f2))
             (all-vars nil)
             (equiv t))
        ;; Union of variables
        (dolist (v (append vars1 vars2))
          (unless (memq v all-vars) (setq all-vars (cons v all-vars))))
        (setq all-vars (nreverse all-vars))
        (dolist (env (funcall 'neovm--logic-all-envs all-vars))
          (let ((r1 (funcall 'neovm--logic-eval f1 env))
                (r2 (funcall 'neovm--logic-eval f2 env)))
            (unless (eq (not (not r1)) (not (not r2)))
              (setq equiv nil))))
        equiv)))

  (unwind-protect
      (let ((formulas (list
                       '(not (not p))
                       '(and p (not p))
                       '(or p (not p))
                       '(implies p q)
                       '(iff p q)
                       '(not (and p q))
                       '(not (or (not p) (not q)))
                       '(and (or p q) t)
                       '(or (and p q) nil))))
        (mapcar
         (lambda (f)
           (let ((simplified (funcall 'neovm--logic-simplify f)))
             (list
              f
              simplified
              (funcall 'neovm--logic-equiv-p f simplified))))
         formulas))
    (fmakunbound 'neovm--logic-eval)
    (fmakunbound 'neovm--logic-vars)
    (fmakunbound 'neovm--logic-all-envs)
    (fmakunbound 'neovm--logic-simplify)
    (fmakunbound 'neovm--logic-equiv-p)))"#;
    let expect = expect_test::expect![[
        r#""OK (((not (not p)) p t) ((and p (not p)) nil t) ((or p (not p)) t t) ((implies p q) (or (not p) q) t) ((iff p q) (and (or (not p) q) (or (not q) p)) t) ((not (and p q)) (or (not p) (not q)) t) ((not (or (not p) (not q))) (and p q) t) ((and (or p q) t) (or p q) t) ((or (and p q) nil) (and p q) t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex multi-variable formulas: 3-variable truth table comparison
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_logic_engine_three_var_complex() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test with 3-variable formulas: 8 rows in truth table.
    // Verify specific logical equivalences and properties.
    let form = r#"(progn
  (fset 'neovm--logic-eval
    (lambda (formula env)
      (cond
       ((eq formula t) t) ((eq formula nil) nil)
       ((symbolp formula) (cdr (assq formula env)))
       ((eq (car formula) 'not)
        (not (funcall 'neovm--logic-eval (cadr formula) env)))
       ((eq (car formula) 'and)
        (and (funcall 'neovm--logic-eval (cadr formula) env)
             (funcall 'neovm--logic-eval (caddr formula) env)))
       ((eq (car formula) 'or)
        (or (funcall 'neovm--logic-eval (cadr formula) env)
            (funcall 'neovm--logic-eval (caddr formula) env)))
       ((eq (car formula) 'implies)
        (or (not (funcall 'neovm--logic-eval (cadr formula) env))
            (funcall 'neovm--logic-eval (caddr formula) env)))
       ((eq (car formula) 'iff)
        (let ((a (funcall 'neovm--logic-eval (cadr formula) env))
              (b (funcall 'neovm--logic-eval (caddr formula) env)))
          (eq (not (not a)) (not (not b))))))))

  (fset 'neovm--logic-all-envs
    (lambda (vars)
      (if (null vars) (list nil)
        (let ((rest (funcall 'neovm--logic-all-envs (cdr vars)))
              (v (car vars)) (result nil))
          (dolist (env rest)
            (setq result (cons (cons (cons v t) env) result))
            (setq result (cons (cons (cons v nil) env) result)))
          (nreverse result)))))

  ;; Count true assignments
  (fset 'neovm--logic-count-true
    (lambda (formula vars)
      (let ((count 0))
        (dolist (env (funcall 'neovm--logic-all-envs vars))
          (when (funcall 'neovm--logic-eval formula env)
            (setq count (1+ count))))
        count)))

  (unwind-protect
      (let ((vars '(p q r)))
        (list
         ;; Majority function: at least 2 of 3 are true
         (let ((majority '(or (and p q) (or (and p r) (and q r)))))
           (list
            (funcall 'neovm--logic-count-true majority vars)
            ;; Evaluate at specific points
            (funcall 'neovm--logic-eval majority '((p . t) (q . t) (r . nil)))
            (funcall 'neovm--logic-eval majority '((p . t) (q . nil) (r . nil)))
            (funcall 'neovm--logic-eval majority '((p . nil) (q . nil) (r . nil)))))
         ;; XOR via (iff (not ...))
         (let ((xor-pq '(and (or p q) (not (and p q)))))
           (funcall 'neovm--logic-count-true xor-pq '(p q)))
         ;; Distributivity: (and p (or q r)) equiv (or (and p q) (and p r))
         (let ((f1 '(and p (or q r)))
               (f2 '(or (and p q) (and p r)))
               (envs (funcall 'neovm--logic-all-envs vars))
               (all-eq t))
           (dolist (env envs)
             (unless (eq (not (not (funcall 'neovm--logic-eval f1 env)))
                         (not (not (funcall 'neovm--logic-eval f2 env))))
               (setq all-eq nil)))
           all-eq)
         ;; Absorption: (or p (and p q)) equiv p
         (let ((envs (funcall 'neovm--logic-all-envs '(p q)))
               (all-eq t))
           (dolist (env envs)
             (unless (eq (not (not (funcall 'neovm--logic-eval '(or p (and p q)) env)))
                         (not (not (funcall 'neovm--logic-eval 'p env))))
               (setq all-eq nil)))
           all-eq)
         ;; Modus ponens: if p and (implies p q), then q
         ;; Check: (and p (implies p q)) implies q is a tautology
         (let ((mp '(implies (and p (implies p q)) q))
               (envs (funcall 'neovm--logic-all-envs '(p q)))
               (is-taut t))
           (dolist (env envs)
             (unless (funcall 'neovm--logic-eval mp env)
               (setq is-taut nil)))
           is-taut)
         ;; Hypothetical syllogism: (implies p q) and (implies q r) -> (implies p r)
         (let ((hs '(implies (and (implies p q) (implies q r)) (implies p r)))
               (envs (funcall 'neovm--logic-all-envs vars))
               (is-taut t))
           (dolist (env envs)
             (unless (funcall 'neovm--logic-eval hs env)
               (setq is-taut nil)))
           is-taut)))
    (fmakunbound 'neovm--logic-eval)
    (fmakunbound 'neovm--logic-all-envs)
    (fmakunbound 'neovm--logic-count-true)))"#;
    let expect = expect_test::expect![[r#""OK ((4 t nil nil) 2 t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
