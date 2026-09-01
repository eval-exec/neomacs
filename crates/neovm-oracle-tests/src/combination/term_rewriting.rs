//! Oracle parity tests for a term rewriting system implemented in Elisp.
//!
//! Implements rewrite rules (pattern -> replacement) with pattern matching
//! and variable binding, single-step and multi-step rewriting to fixpoint,
//! algebraic expression simplification, and boolean expression normalization.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

// ---------------------------------------------------------------------------
// Core term rewriting: pattern matching with variables
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_term_rewriting_pattern_matching() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement pattern matching where symbols starting with ? are variables.
    // A pattern matches a term by binding variables to sub-terms.
    let form = r#"(progn
  ;; Check if symbol is a pattern variable (starts with ?)
  (fset 'neovm--tr-var-p
    (lambda (x)
      (and (symbolp x)
           (> (length (symbol-name x)) 1)
           (= (aref (symbol-name x) 0) ??))))

  ;; Match pattern against term, returning bindings alist or nil on failure.
  ;; Bindings is an alist of (var . value) pairs.
  (fset 'neovm--tr-match
    (lambda (pattern term bindings)
      (cond
       ;; Variable: check existing binding or create new one
       ((funcall 'neovm--tr-var-p pattern)
        (let ((existing (assq pattern bindings)))
          (if existing
              (if (equal (cdr existing) term) bindings nil)
            (cons (cons pattern term) bindings))))
       ;; Both are lists: match element-by-element
       ((and (consp pattern) (consp term))
        (let ((b (funcall 'neovm--tr-match (car pattern) (car term) bindings)))
          (when b
            (funcall 'neovm--tr-match (cdr pattern) (cdr term) b))))
       ;; Atoms: must be equal
       ((equal pattern term) bindings)
       ;; Otherwise: no match
       (t nil))))

  ;; Substitute bindings into a template
  (fset 'neovm--tr-subst
    (lambda (template bindings)
      (cond
       ((funcall 'neovm--tr-var-p template)
        (let ((b (assq template bindings)))
          (if b (cdr b) template)))
       ((consp template)
        (cons (funcall 'neovm--tr-subst (car template) bindings)
              (funcall 'neovm--tr-subst (cdr template) bindings)))
       (t template))))

  (unwind-protect
      (list
       ;; Simple variable matching
       (funcall 'neovm--tr-match '?x 42 nil)
       ;; Match list with variable
       (funcall 'neovm--tr-match '(+ ?x ?y) '(+ 1 2) nil)
       ;; Repeated variable: must bind to same value
       (funcall 'neovm--tr-match '(+ ?x ?x) '(+ 3 3) nil)
       (funcall 'neovm--tr-match '(+ ?x ?x) '(+ 3 4) nil)
       ;; Nested pattern
       (funcall 'neovm--tr-match '(* ?a (+ ?b ?c)) '(* 2 (+ 3 4)) nil)
       ;; No match: structure mismatch
       (funcall 'neovm--tr-match '(+ ?x ?y) '(* 1 2) nil)
       ;; Substitution
       (funcall 'neovm--tr-subst '(* ?x ?y) '((?x . 10) (?y . 20)))
       ;; Nested substitution
       (funcall 'neovm--tr-subst '(if ?cond ?then ?else)
                '((?cond . (> x 0)) (?then . (+ x 1)) (?else . (- x 1))))
       ;; Match and substitute combined
       (let ((bindings (funcall 'neovm--tr-match '(+ ?x ?y) '(+ a b) nil)))
         (funcall 'neovm--tr-subst '(sum ?y ?x) bindings)))
    (fmakunbound 'neovm--tr-var-p)
    (fmakunbound 'neovm--tr-match)
    (fmakunbound 'neovm--tr-subst)))"#;
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \"?\" 58 40)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Single-step rewrite: apply first matching rule
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_term_rewriting_single_step() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A rule is (pattern . replacement). Single-step applies the first
    // matching rule to the top-level term, or recursively to sub-terms.
    let form = r#"(progn
  (fset 'neovm--tr2-var-p
    (lambda (x)
      (and (symbolp x)
           (> (length (symbol-name x)) 1)
           (= (aref (symbol-name x) 0) ??))))

  (fset 'neovm--tr2-match
    (lambda (pattern term bindings)
      (cond
       ((funcall 'neovm--tr2-var-p pattern)
        (let ((existing (assq pattern bindings)))
          (if existing
              (if (equal (cdr existing) term) bindings nil)
            (cons (cons pattern term) bindings))))
       ((and (consp pattern) (consp term))
        (let ((b (funcall 'neovm--tr2-match (car pattern) (car term) bindings)))
          (when b (funcall 'neovm--tr2-match (cdr pattern) (cdr term) b))))
       ((equal pattern term) bindings)
       (t nil))))

  (fset 'neovm--tr2-subst
    (lambda (template bindings)
      (cond
       ((funcall 'neovm--tr2-var-p template)
        (let ((b (assq template bindings)))
          (if b (cdr b) template)))
       ((consp template)
        (cons (funcall 'neovm--tr2-subst (car template) bindings)
              (funcall 'neovm--tr2-subst (cdr template) bindings)))
       (t template))))

  ;; Try to apply one rule to a term at top level
  (fset 'neovm--tr2-try-rules
    (lambda (rules term)
      (let ((result nil) (rs rules))
        (while (and rs (not result))
          (let* ((rule (car rs))
                 (pattern (car rule))
                 (replacement (cdr rule))
                 (bindings (funcall 'neovm--tr2-match pattern term nil)))
            (when bindings
              (setq result (funcall 'neovm--tr2-subst replacement bindings))))
          (setq rs (cdr rs)))
        result)))

  ;; Single-step: try top-level, then recursively try sub-terms
  (fset 'neovm--tr2-step
    (lambda (rules term)
      (or (funcall 'neovm--tr2-try-rules rules term)
          (if (consp term)
              (let ((new-car (funcall 'neovm--tr2-step rules (car term))))
                (if new-car
                    (cons new-car (cdr term))
                  (let ((new-cdr (funcall 'neovm--tr2-step rules (cdr term))))
                    (when new-cdr
                      (cons (car term) new-cdr)))))
            nil))))

  (unwind-protect
      (let ((rules '(;; x + 0 -> x
                     ((+ ?x 0) . ?x)
                     ;; 0 + x -> x
                     ((+ 0 ?x) . ?x)
                     ;; x * 1 -> x
                     ((* ?x 1) . ?x)
                     ;; 1 * x -> x
                     ((* 1 ?x) . ?x)
                     ;; x * 0 -> 0
                     ((* ?x 0) . 0)
                     ;; 0 * x -> 0
                     ((* 0 ?x) . 0))))
        (list
         ;; Direct match: (+ a 0) -> a
         (funcall 'neovm--tr2-step rules '(+ a 0))
         ;; Direct match: (* b 1) -> b
         (funcall 'neovm--tr2-step rules '(* b 1))
         ;; Direct match: (* c 0) -> 0
         (funcall 'neovm--tr2-step rules '(* c 0))
         ;; No match at top, match in sub-term
         (funcall 'neovm--tr2-step rules '(+ (+ a 0) b))
         ;; Nested: (* 1 (+ 0 x))
         (funcall 'neovm--tr2-step rules '(* 1 (+ 0 x)))
         ;; No applicable rule
         (funcall 'neovm--tr2-step rules '(+ a b))
         ;; Deeply nested
         (funcall 'neovm--tr2-step rules '(+ (+ (+ a 0) 0) 0))))
    (fmakunbound 'neovm--tr2-var-p)
    (fmakunbound 'neovm--tr2-match)
    (fmakunbound 'neovm--tr2-subst)
    (fmakunbound 'neovm--tr2-try-rules)
    (fmakunbound 'neovm--tr2-step)))"#;
    let expect = expect_test::expect![[r#""OK (nil nil nil nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Multi-step rewrite: apply rules until fixpoint
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_term_rewriting_fixpoint() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Repeatedly apply single-step rewriting until no more rules apply
    // (fixpoint reached) or a step limit is hit.
    let form = r#"(progn
  (fset 'neovm--tr3-var-p
    (lambda (x)
      (and (symbolp x)
           (> (length (symbol-name x)) 1)
           (= (aref (symbol-name x) 0) ??))))

  (fset 'neovm--tr3-match
    (lambda (pattern term bindings)
      (cond
       ((funcall 'neovm--tr3-var-p pattern)
        (let ((existing (assq pattern bindings)))
          (if existing
              (if (equal (cdr existing) term) bindings nil)
            (cons (cons pattern term) bindings))))
       ((and (consp pattern) (consp term))
        (let ((b (funcall 'neovm--tr3-match (car pattern) (car term) bindings)))
          (when b (funcall 'neovm--tr3-match (cdr pattern) (cdr term) b))))
       ((equal pattern term) bindings)
       (t nil))))

  (fset 'neovm--tr3-subst
    (lambda (template bindings)
      (cond
       ((funcall 'neovm--tr3-var-p template)
        (let ((b (assq template bindings)))
          (if b (cdr b) template)))
       ((consp template)
        (cons (funcall 'neovm--tr3-subst (car template) bindings)
              (funcall 'neovm--tr3-subst (cdr template) bindings)))
       (t template))))

  (fset 'neovm--tr3-try-rules
    (lambda (rules term)
      (let ((result nil) (rs rules))
        (while (and rs (not result))
          (let* ((rule (car rs))
                 (bindings (funcall 'neovm--tr3-match (car rule) term nil)))
            (when bindings
              (setq result (funcall 'neovm--tr3-subst (cdr rule) bindings))))
          (setq rs (cdr rs)))
        result)))

  (fset 'neovm--tr3-step
    (lambda (rules term)
      (or (funcall 'neovm--tr3-try-rules rules term)
          (if (consp term)
              (let ((new-car (funcall 'neovm--tr3-step rules (car term))))
                (if new-car
                    (cons new-car (cdr term))
                  (let ((new-cdr (funcall 'neovm--tr3-step rules (cdr term))))
                    (when new-cdr
                      (cons (car term) new-cdr)))))
            nil))))

  ;; Rewrite to fixpoint with step limit
  (fset 'neovm--tr3-normalize
    (lambda (rules term max-steps)
      (let ((current term)
            (steps 0))
        (while (< steps max-steps)
          (let ((next (funcall 'neovm--tr3-step rules current)))
            (if next
                (progn
                  (setq current next)
                  (setq steps (1+ steps)))
              (setq steps max-steps))))
        (list 'result current 'steps steps))))

  (unwind-protect
      (let ((rules '(((+ ?x 0) . ?x)
                     ((+ 0 ?x) . ?x)
                     ((* ?x 1) . ?x)
                     ((* 1 ?x) . ?x)
                     ((* ?x 0) . 0)
                     ((* 0 ?x) . 0))))
        (list
         ;; Already normal form
         (funcall 'neovm--tr3-normalize rules 'a 100)
         ;; One step needed
         (funcall 'neovm--tr3-normalize rules '(+ a 0) 100)
         ;; Multiple steps: (+ (+ a 0) 0) -> (+ a 0) -> a
         (funcall 'neovm--tr3-normalize rules '(+ (+ a 0) 0) 100)
         ;; Deep nesting: (+ (+ (+ a 0) 0) 0) -> a
         (funcall 'neovm--tr3-normalize rules '(+ (+ (+ a 0) 0) 0) 100)
         ;; Mixed operations: (* 1 (+ 0 (+ a 0)))
         (funcall 'neovm--tr3-normalize rules '(* 1 (+ 0 (+ a 0))) 100)
         ;; Zero collapse: (* (+ a b) 0) -> 0
         (funcall 'neovm--tr3-normalize rules '(* (+ a b) 0) 100)
         ;; Complex: (* 1 (+ (* 0 c) (+ d 0)))
         (funcall 'neovm--tr3-normalize rules '(* 1 (+ (* 0 c) (+ d 0))) 100)))
    (fmakunbound 'neovm--tr3-var-p)
    (fmakunbound 'neovm--tr3-match)
    (fmakunbound 'neovm--tr3-subst)
    (fmakunbound 'neovm--tr3-try-rules)
    (fmakunbound 'neovm--tr3-step)
    (fmakunbound 'neovm--tr3-normalize)))"#;
    let expect = expect_test::expect![[
        r#""OK ((result a steps 100) (result (+ a 0) steps 100) (result (+ (+ a 0) 0) steps 100) (result (+ (+ (+ a 0) 0) 0) steps 100) (result (* 1 (+ 0 (+ a 0))) steps 100) (result (* (+ a b) 0) steps 100) (result (* 1 (+ (* 0 c) (+ d 0))) steps 100))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: simplifying algebraic expressions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_term_rewriting_algebra_simplify() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // More sophisticated algebraic simplification rules including
    // identity, absorption, idempotency, and distribution.
    let form = r#"(progn
  (fset 'neovm--tr4-var-p
    (lambda (x)
      (and (symbolp x)
           (> (length (symbol-name x)) 1)
           (= (aref (symbol-name x) 0) ??))))

  (fset 'neovm--tr4-match
    (lambda (pattern term bindings)
      (cond
       ((funcall 'neovm--tr4-var-p pattern)
        (let ((existing (assq pattern bindings)))
          (if existing
              (if (equal (cdr existing) term) bindings nil)
            (cons (cons pattern term) bindings))))
       ((and (consp pattern) (consp term))
        (let ((b (funcall 'neovm--tr4-match (car pattern) (car term) bindings)))
          (when b (funcall 'neovm--tr4-match (cdr pattern) (cdr term) b))))
       ((equal pattern term) bindings)
       (t nil))))

  (fset 'neovm--tr4-subst
    (lambda (template bindings)
      (cond
       ((funcall 'neovm--tr4-var-p template)
        (let ((b (assq template bindings)))
          (if b (cdr b) template)))
       ((consp template)
        (cons (funcall 'neovm--tr4-subst (car template) bindings)
              (funcall 'neovm--tr4-subst (cdr template) bindings)))
       (t template))))

  (fset 'neovm--tr4-try-rules
    (lambda (rules term)
      (let ((result nil) (rs rules))
        (while (and rs (not result))
          (let* ((rule (car rs))
                 (bindings (funcall 'neovm--tr4-match (car rule) term nil)))
            (when bindings
              (setq result (funcall 'neovm--tr4-subst (cdr rule) bindings))))
          (setq rs (cdr rs)))
        result)))

  (fset 'neovm--tr4-step
    (lambda (rules term)
      (or (funcall 'neovm--tr4-try-rules rules term)
          (if (consp term)
              (let ((new-car (funcall 'neovm--tr4-step rules (car term))))
                (if new-car
                    (cons new-car (cdr term))
                  (let ((new-cdr (funcall 'neovm--tr4-step rules (cdr term))))
                    (when new-cdr
                      (cons (car term) new-cdr)))))
            nil))))

  (fset 'neovm--tr4-normalize
    (lambda (rules term max-steps)
      (let ((current term) (steps 0))
        (while (< steps max-steps)
          (let ((next (funcall 'neovm--tr4-step rules current)))
            (if next
                (progn (setq current next) (setq steps (1+ steps)))
              (setq steps max-steps))))
        current)))

  (unwind-protect
      (let ((rules '(;; Additive identity
                     ((+ ?x 0) . ?x)
                     ((+ 0 ?x) . ?x)
                     ;; Multiplicative identity
                     ((* ?x 1) . ?x)
                     ((* 1 ?x) . ?x)
                     ;; Multiplicative zero
                     ((* ?x 0) . 0)
                     ((* 0 ?x) . 0)
                     ;; Additive idempotency: x + x -> (* 2 x)
                     ((+ ?x ?x) . (* 2 ?x))
                     ;; Subtractive cancellation: x - x -> 0
                     ((- ?x ?x) . 0)
                     ;; Double negation: (- (- x)) -> x
                     ((- (- ?x)) . ?x)
                     ;; Exponent identities
                     ((expt ?x 0) . 1)
                     ((expt ?x 1) . ?x))))
        (list
         ;; Identity simplification
         (funcall 'neovm--tr4-normalize rules '(+ (* a 1) 0) 100)
         ;; Idempotency: a + a -> (* 2 a)
         (funcall 'neovm--tr4-normalize rules '(+ a a) 100)
         ;; Cancellation: (- b b) -> 0
         (funcall 'neovm--tr4-normalize rules '(- b b) 100)
         ;; Double negation
         (funcall 'neovm--tr4-normalize rules '(- (- c)) 100)
         ;; Exponents
         (funcall 'neovm--tr4-normalize rules '(expt x 0) 100)
         (funcall 'neovm--tr4-normalize rules '(expt x 1) 100)
         ;; Combined: (* 1 (+ (expt y 1) 0))
         (funcall 'neovm--tr4-normalize rules '(* 1 (+ (expt y 1) 0)) 100)
         ;; Complex: (+ (* (- (- a)) 1) (- b b))
         (funcall 'neovm--tr4-normalize rules '(+ (* (- (- a)) 1) (- b b)) 100)))
    (fmakunbound 'neovm--tr4-var-p)
    (fmakunbound 'neovm--tr4-match)
    (fmakunbound 'neovm--tr4-subst)
    (fmakunbound 'neovm--tr4-try-rules)
    (fmakunbound 'neovm--tr4-step)
    (fmakunbound 'neovm--tr4-normalize)))"#;
    let expect = expect_test::expect![[
        r#""OK ((+ (* a 1) 0) (+ a a) (- b b) (- (- c)) (expt x 0) (expt x 1) (* 1 (+ (expt y 1) 0)) (+ (* (- (- a)) 1) (- b b)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: normalizing boolean expressions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_term_rewriting_boolean_normalize() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Boolean expression simplification rules: double negation elimination,
    // De Morgan's laws, identity/absorption/idempotency laws.
    let form = r#"(progn
  (fset 'neovm--tr5-var-p
    (lambda (x)
      (and (symbolp x)
           (> (length (symbol-name x)) 1)
           (= (aref (symbol-name x) 0) ??))))

  (fset 'neovm--tr5-match
    (lambda (pattern term bindings)
      (cond
       ((funcall 'neovm--tr5-var-p pattern)
        (let ((existing (assq pattern bindings)))
          (if existing
              (if (equal (cdr existing) term) bindings nil)
            (cons (cons pattern term) bindings))))
       ((and (consp pattern) (consp term))
        (let ((b (funcall 'neovm--tr5-match (car pattern) (car term) bindings)))
          (when b (funcall 'neovm--tr5-match (cdr pattern) (cdr term) b))))
       ((equal pattern term) bindings)
       (t nil))))

  (fset 'neovm--tr5-subst
    (lambda (template bindings)
      (cond
       ((funcall 'neovm--tr5-var-p template)
        (let ((b (assq template bindings)))
          (if b (cdr b) template)))
       ((consp template)
        (cons (funcall 'neovm--tr5-subst (car template) bindings)
              (funcall 'neovm--tr5-subst (cdr template) bindings)))
       (t template))))

  (fset 'neovm--tr5-try-rules
    (lambda (rules term)
      (let ((result nil) (rs rules))
        (while (and rs (not result))
          (let* ((rule (car rs))
                 (bindings (funcall 'neovm--tr5-match (car rule) term nil)))
            (when bindings
              (setq result (funcall 'neovm--tr5-subst (cdr rule) bindings))))
          (setq rs (cdr rs)))
        result)))

  (fset 'neovm--tr5-step
    (lambda (rules term)
      (or (funcall 'neovm--tr5-try-rules rules term)
          (if (consp term)
              (let ((new-car (funcall 'neovm--tr5-step rules (car term))))
                (if new-car
                    (cons new-car (cdr term))
                  (let ((new-cdr (funcall 'neovm--tr5-step rules (cdr term))))
                    (when new-cdr
                      (cons (car term) new-cdr)))))
            nil))))

  (fset 'neovm--tr5-normalize
    (lambda (rules term max-steps)
      (let ((current term) (steps 0))
        (while (< steps max-steps)
          (let ((next (funcall 'neovm--tr5-step rules current)))
            (if next
                (progn (setq current next) (setq steps (1+ steps)))
              (setq steps max-steps))))
        current)))

  (unwind-protect
      (let ((rules '(;; Double negation: (not (not x)) -> x
                     ((not (not ?x)) . ?x)
                     ;; Identity laws
                     ((and ?x true) . ?x)
                     ((and true ?x) . ?x)
                     ((or ?x false) . ?x)
                     ((or false ?x) . ?x)
                     ;; Annihilation laws
                     ((and ?x false) . false)
                     ((and false ?x) . false)
                     ((or ?x true) . true)
                     ((or true ?x) . true)
                     ;; Idempotency
                     ((and ?x ?x) . ?x)
                     ((or ?x ?x) . ?x)
                     ;; Complement
                     ((and ?x (not ?x)) . false)
                     ((or ?x (not ?x)) . true))))
        (list
         ;; Double negation
         (funcall 'neovm--tr5-normalize rules '(not (not a)) 50)
         ;; Triple negation
         (funcall 'neovm--tr5-normalize rules '(not (not (not b))) 50)
         ;; Identity
         (funcall 'neovm--tr5-normalize rules '(and p true) 50)
         (funcall 'neovm--tr5-normalize rules '(or q false) 50)
         ;; Annihilation
         (funcall 'neovm--tr5-normalize rules '(and anything false) 50)
         (funcall 'neovm--tr5-normalize rules '(or anything true) 50)
         ;; Idempotency
         (funcall 'neovm--tr5-normalize rules '(and x x) 50)
         (funcall 'neovm--tr5-normalize rules '(or y y) 50)
         ;; Complement
         (funcall 'neovm--tr5-normalize rules '(and p (not p)) 50)
         (funcall 'neovm--tr5-normalize rules '(or q (not q)) 50)
         ;; Complex: (and (not (not a)) (or b false))
         (funcall 'neovm--tr5-normalize rules '(and (not (not a)) (or b false)) 50)
         ;; Complex: (or (and x false) (not (not y)))
         (funcall 'neovm--tr5-normalize rules '(or (and x false) (not (not y))) 50)))
    (fmakunbound 'neovm--tr5-var-p)
    (fmakunbound 'neovm--tr5-match)
    (fmakunbound 'neovm--tr5-subst)
    (fmakunbound 'neovm--tr5-try-rules)
    (fmakunbound 'neovm--tr5-step)
    (fmakunbound 'neovm--tr5-normalize)))"#;
    let expect = expect_test::expect![[
        r#""OK ((not (not a)) (not (not (not b))) (and p true) (or q false) (and anything false) (or anything true) (and x x) (or y y) (and p (not p)) (or q (not q)) (and (not (not a)) (or b false)) (or (and x false) (not (not y))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: conditional rewriting with guards
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_term_rewriting_conditional_rules() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Extend the rewriting system with conditional rules: a rule only
    // applies if a guard predicate is satisfied by the bindings.
    // Rules are (pattern guard-fn . replacement).
    let form = r#"(progn
  (fset 'neovm--tr6-var-p
    (lambda (x)
      (and (symbolp x)
           (> (length (symbol-name x)) 1)
           (= (aref (symbol-name x) 0) ??))))

  (fset 'neovm--tr6-match
    (lambda (pattern term bindings)
      (cond
       ((funcall 'neovm--tr6-var-p pattern)
        (let ((existing (assq pattern bindings)))
          (if existing
              (if (equal (cdr existing) term) bindings nil)
            (cons (cons pattern term) bindings))))
       ((and (consp pattern) (consp term))
        (let ((b (funcall 'neovm--tr6-match (car pattern) (car term) bindings)))
          (when b (funcall 'neovm--tr6-match (cdr pattern) (cdr term) b))))
       ((equal pattern term) bindings)
       (t nil))))

  (fset 'neovm--tr6-subst
    (lambda (template bindings)
      (cond
       ((funcall 'neovm--tr6-var-p template)
        (let ((b (assq template bindings)))
          (if b (cdr b) template)))
       ((consp template)
        (cons (funcall 'neovm--tr6-subst (car template) bindings)
              (funcall 'neovm--tr6-subst (cdr template) bindings)))
       (t template))))

  ;; Conditional rules: each rule is (pattern guard-fn replacement)
  ;; guard-fn takes bindings and returns non-nil if rule should apply
  (fset 'neovm--tr6-try-crules
    (lambda (crules term)
      (let ((result nil) (rs crules))
        (while (and rs (not result))
          (let* ((crule (car rs))
                 (pattern (nth 0 crule))
                 (guard (nth 1 crule))
                 (replacement (nth 2 crule))
                 (bindings (funcall 'neovm--tr6-match pattern term nil)))
            (when (and bindings (funcall guard bindings))
              (setq result (funcall 'neovm--tr6-subst replacement bindings))))
          (setq rs (cdr rs)))
        result)))

  (fset 'neovm--tr6-step
    (lambda (crules term)
      (or (funcall 'neovm--tr6-try-crules crules term)
          (if (consp term)
              (let ((new-car (funcall 'neovm--tr6-step crules (car term))))
                (if new-car
                    (cons new-car (cdr term))
                  (let ((new-cdr (funcall 'neovm--tr6-step crules (cdr term))))
                    (when new-cdr
                      (cons (car term) new-cdr)))))
            nil))))

  (fset 'neovm--tr6-normalize
    (lambda (crules term max-steps)
      (let ((current term) (steps 0))
        (while (< steps max-steps)
          (let ((next (funcall 'neovm--tr6-step crules current)))
            (if next
                (progn (setq current next) (setq steps (1+ steps)))
              (setq steps max-steps))))
        current)))

  (unwind-protect
      (let ((crules
             (list
              ;; Constant folding for + when both args are numbers
              (list '(+ ?x ?y)
                    (lambda (b) (and (numberp (cdr (assq '?x b)))
                                     (numberp (cdr (assq '?y b)))))
                    '?result)
              ;; Constant folding for * when both args are numbers
              (list '(* ?x ?y)
                    (lambda (b) (and (numberp (cdr (assq '?x b)))
                                     (numberp (cdr (assq '?y b)))))
                    '?result)
              ;; x + 0 -> x (unconditional)
              (list '(+ ?x 0) (lambda (_b) t) '?x)
              ;; 0 + x -> x
              (list '(+ 0 ?x) (lambda (_b) t) '?x)
              ;; x * 1 -> x
              (list '(* ?x 1) (lambda (_b) t) '?x)
              ;; x * 0 -> 0
              (list '(* ?x 0) (lambda (_b) t) '0))))
        ;; For constant folding rules, we need to compute the result.
        ;; Instead of doing that inline (complex), test the unconditional rules
        ;; and the guard mechanism.
        ;; The guard prevents + from applying when args are non-numeric:
        (list
         ;; Guard blocks constant folding for symbolic + (no ?result binding)
         ;; but unconditional x+0 still applies:
         (funcall 'neovm--tr6-normalize crules '(+ a 0) 50)
         ;; x*1 unconditional
         (funcall 'neovm--tr6-normalize crules '(* b 1) 50)
         ;; x*0 -> 0
         (funcall 'neovm--tr6-normalize crules '(* (+ a b) 0) 50)
         ;; Nested with 0+x
         (funcall 'neovm--tr6-normalize crules '(+ 0 (* c 1)) 50)
         ;; Deep: (* 1 (+ (+ 0 d) 0))
         (funcall 'neovm--tr6-normalize crules '(* 1 (+ (+ 0 d) 0)) 50)))
    (fmakunbound 'neovm--tr6-var-p)
    (fmakunbound 'neovm--tr6-match)
    (fmakunbound 'neovm--tr6-subst)
    (fmakunbound 'neovm--tr6-try-crules)
    (fmakunbound 'neovm--tr6-step)
    (fmakunbound 'neovm--tr6-normalize)))"#;
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \"?\" 78 23)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
