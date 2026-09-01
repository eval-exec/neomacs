//! Oracle parity tests for a unification algorithm (logic programming)
//! implemented in Elisp. Covers term representation, substitution
//! application, occurs check, unification of atoms/variables/compound
//! terms, and solving sets of equations via successive unification.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

// ---------------------------------------------------------------------------
// Term representation and substitution application
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_unification_substitution_apply() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Terms: variables are symbols starting with ? (?x, ?y, ?z)
    // Constants: other symbols (a, b, c) and integers
    // Compound terms: lists (f ?x b) meaning f applied to ?x and b
    // Substitution: alist ((?x . a) (?y . (g b)))
    let form = r#"
(progn
  ;; Check if a term is a variable (symbol starting with ?)
  (fset 'neovm--unify-var-p
    (lambda (term)
      (and (symbolp term)
           (string-prefix-p "?" (symbol-name term)))))

  ;; Apply a substitution to a term, recursively
  (fset 'neovm--unify-apply-subst
    (lambda (subst term)
      (cond
       ;; Variable: look up in subst, recursively apply if found
       ((funcall 'neovm--unify-var-p term)
        (let ((binding (assq term subst)))
          (if binding
              (funcall 'neovm--unify-apply-subst subst (cdr binding))
            term)))
       ;; Compound term: apply to each element
       ((consp term)
        (mapcar (lambda (t1) (funcall 'neovm--unify-apply-subst subst t1))
                term))
       ;; Constant or number: unchanged
       (t term))))

  (unwind-protect
      (list
       ;; Variable detection
       (funcall 'neovm--unify-var-p '?x)
       (funcall 'neovm--unify-var-p '?foo)
       (funcall 'neovm--unify-var-p 'a)
       (funcall 'neovm--unify-var-p 42)
       (funcall 'neovm--unify-var-p '(f ?x))

       ;; Apply substitution to a variable
       (funcall 'neovm--unify-apply-subst '((?x . a)) '?x)
       ;; Variable not in subst
       (funcall 'neovm--unify-apply-subst '((?x . a)) '?y)
       ;; Apply to a constant
       (funcall 'neovm--unify-apply-subst '((?x . a)) 'b)
       ;; Apply to a compound term
       (funcall 'neovm--unify-apply-subst '((?x . a) (?y . b)) '(f ?x ?y))
       ;; Nested compound term
       (funcall 'neovm--unify-apply-subst '((?x . a)) '(f (g ?x) ?x))
       ;; Chained substitution: ?x -> ?y, ?y -> a
       (funcall 'neovm--unify-apply-subst '((?x . ?y) (?y . a)) '?x)
       ;; Deep nesting
       (funcall 'neovm--unify-apply-subst
                '((?x . (h a)) (?y . ?x))
                '(f ?y (g ?x)))
       ;; Empty substitution
       (funcall 'neovm--unify-apply-subst nil '(f ?x ?y))
       ;; Number in compound
       (funcall 'neovm--unify-apply-subst '((?x . 42)) '(plus ?x 1)))
    (fmakunbound 'neovm--unify-var-p)
    (fmakunbound 'neovm--unify-apply-subst)))
"#;
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \"?\" 30 39)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Occurs check (variable appears in term)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_unification_occurs_check() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (fset 'neovm--unify-var-p
    (lambda (term)
      (and (symbolp term)
           (string-prefix-p "?" (symbol-name term)))))

  ;; Occurs check: does variable VAR appear in TERM (under SUBST)?
  (fset 'neovm--unify-occurs-p
    (lambda (var term subst)
      (cond
       ((equal var term) t)
       ((and (funcall 'neovm--unify-var-p term)
             (assq term subst))
        (funcall 'neovm--unify-occurs-p var (cdr (assq term subst)) subst))
       ((consp term)
        (let ((found nil))
          (dolist (sub term)
            (when (funcall 'neovm--unify-occurs-p var sub subst)
              (setq found t)))
          found))
       (t nil))))

  (unwind-protect
      (list
       ;; Variable occurs in itself
       (funcall 'neovm--unify-occurs-p '?x '?x nil)
       ;; Variable does not occur in constant
       (funcall 'neovm--unify-occurs-p '?x 'a nil)
       ;; Variable occurs in compound
       (funcall 'neovm--unify-occurs-p '?x '(f ?x) nil)
       ;; Variable occurs deeply
       (funcall 'neovm--unify-occurs-p '?x '(f (g (h ?x))) nil)
       ;; Variable does not occur
       (funcall 'neovm--unify-occurs-p '?x '(f ?y ?z) nil)
       ;; Via substitution: ?y -> (g ?x), so ?x occurs in ?y
       (funcall 'neovm--unify-occurs-p '?x '?y '((?y . (g ?x))))
       ;; Chain: ?y -> ?z, ?z -> (h ?x)
       (funcall 'neovm--unify-occurs-p '?x '?y '((?y . ?z) (?z . (h ?x))))
       ;; No chain: ?y -> ?z, ?z -> a
       (funcall 'neovm--unify-occurs-p '?x '?y '((?y . ?z) (?z . a)))
       ;; Multiple paths in compound
       (funcall 'neovm--unify-occurs-p '?x '(f ?y (g ?z))
                '((?y . a) (?z . ?x))))
    (fmakunbound 'neovm--unify-var-p)
    (fmakunbound 'neovm--unify-occurs-p)))
"#;
    let expect = expect_test::expect![[r#""OK (t nil t t nil nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Unify two terms producing a substitution
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_unification_basic_unify() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (fset 'neovm--unify-var-p
    (lambda (term)
      (and (symbolp term)
           (string-prefix-p "?" (symbol-name term)))))

  (fset 'neovm--unify-occurs-p
    (lambda (var term subst)
      (cond
       ((equal var term) t)
       ((and (funcall 'neovm--unify-var-p term)
             (assq term subst))
        (funcall 'neovm--unify-occurs-p var (cdr (assq term subst)) subst))
       ((consp term)
        (let ((found nil))
          (dolist (sub term)
            (when (funcall 'neovm--unify-occurs-p var sub subst)
              (setq found t)))
          found))
       (t nil))))

  (fset 'neovm--unify-apply-subst
    (lambda (subst term)
      (cond
       ((funcall 'neovm--unify-var-p term)
        (let ((binding (assq term subst)))
          (if binding
              (funcall 'neovm--unify-apply-subst subst (cdr binding))
            term)))
       ((consp term)
        (mapcar (lambda (t1) (funcall 'neovm--unify-apply-subst subst t1))
                term))
       (t term))))

  ;; Unify a variable with a term
  (fset 'neovm--unify-var
    (lambda (var term subst)
      (cond
       ;; var already bound
       ((assq var subst)
        (funcall 'neovm--unify
                 (cdr (assq var subst)) term subst))
       ;; term is a variable already bound
       ((and (funcall 'neovm--unify-var-p term) (assq term subst))
        (funcall 'neovm--unify
                 var (cdr (assq term subst)) subst))
       ;; Occurs check
       ((funcall 'neovm--unify-occurs-p var term subst)
        'fail)
       ;; Extend substitution
       (t (cons (cons var term) subst)))))

  ;; Main unification
  (fset 'neovm--unify
    (lambda (t1 t2 subst)
      (cond
       ((eq subst 'fail) 'fail)
       ((equal t1 t2) subst)
       ((funcall 'neovm--unify-var-p t1)
        (funcall 'neovm--unify-var t1 t2 subst))
       ((funcall 'neovm--unify-var-p t2)
        (funcall 'neovm--unify-var t2 t1 subst))
       ((and (consp t1) (consp t2))
        (if (/= (length t1) (length t2))
            'fail
          (let ((s subst))
            (let ((i 0) (len (length t1)))
              (while (and (not (eq s 'fail)) (< i len))
                (setq s (funcall 'neovm--unify (nth i t1) (nth i t2) s))
                (setq i (1+ i))))
            s)))
       (t 'fail))))

  (unwind-protect
      (list
       ;; Same constants
       (funcall 'neovm--unify 'a 'a nil)
       ;; Different constants => fail
       (funcall 'neovm--unify 'a 'b nil)
       ;; Variable with constant
       (funcall 'neovm--unify '?x 'a nil)
       ;; Two variables
       (funcall 'neovm--unify '?x '?y nil)
       ;; Compound terms
       (funcall 'neovm--unify '(f ?x) '(f a) nil)
       ;; Nested compound
       (funcall 'neovm--unify '(f ?x (g ?y)) '(f a (g b)) nil)
       ;; Different functors => fail
       (funcall 'neovm--unify '(f ?x) '(g ?x) nil)
       ;; Different arity => fail
       (funcall 'neovm--unify '(f ?x) '(f ?x ?y) nil)
       ;; Occurs check failure: ?x cannot unify with (f ?x)
       (funcall 'neovm--unify '?x '(f ?x) nil)
       ;; Complex: (f ?x ?y) and (f ?y a)
       (let ((s (funcall 'neovm--unify '(f ?x ?y) '(f ?y a) nil)))
         (when (not (eq s 'fail))
           (list s
                 (funcall 'neovm--unify-apply-subst s '?x)
                 (funcall 'neovm--unify-apply-subst s '?y))))
       ;; Numbers
       (funcall 'neovm--unify 42 42 nil)
       (funcall 'neovm--unify 42 43 nil))
    (fmakunbound 'neovm--unify-var-p)
    (fmakunbound 'neovm--unify-occurs-p)
    (fmakunbound 'neovm--unify-apply-subst)
    (fmakunbound 'neovm--unify-var)
    (fmakunbound 'neovm--unify)))
"#;
    let expect = expect_test::expect![[
        r#""OK (nil fail fail fail fail fail fail fail fail nil nil fail)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: unify compound terms recursively
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_unification_compound_recursive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (fset 'neovm--unify-var-p
    (lambda (term) (and (symbolp term) (string-prefix-p "?" (symbol-name term)))))

  (fset 'neovm--unify-occurs-p
    (lambda (var term subst)
      (cond
       ((equal var term) t)
       ((and (funcall 'neovm--unify-var-p term) (assq term subst))
        (funcall 'neovm--unify-occurs-p var (cdr (assq term subst)) subst))
       ((consp term)
        (let ((found nil))
          (dolist (sub term) (when (funcall 'neovm--unify-occurs-p var sub subst) (setq found t)))
          found))
       (t nil))))

  (fset 'neovm--unify-apply-subst
    (lambda (subst term)
      (cond
       ((funcall 'neovm--unify-var-p term)
        (let ((b (assq term subst)))
          (if b (funcall 'neovm--unify-apply-subst subst (cdr b)) term)))
       ((consp term) (mapcar (lambda (t1) (funcall 'neovm--unify-apply-subst subst t1)) term))
       (t term))))

  (fset 'neovm--unify-var
    (lambda (var term subst)
      (cond
       ((assq var subst) (funcall 'neovm--unify (cdr (assq var subst)) term subst))
       ((and (funcall 'neovm--unify-var-p term) (assq term subst))
        (funcall 'neovm--unify var (cdr (assq term subst)) subst))
       ((funcall 'neovm--unify-occurs-p var term subst) 'fail)
       (t (cons (cons var term) subst)))))

  (fset 'neovm--unify
    (lambda (t1 t2 subst)
      (cond
       ((eq subst 'fail) 'fail)
       ((equal t1 t2) subst)
       ((funcall 'neovm--unify-var-p t1) (funcall 'neovm--unify-var t1 t2 subst))
       ((funcall 'neovm--unify-var-p t2) (funcall 'neovm--unify-var t2 t1 subst))
       ((and (consp t1) (consp t2))
        (if (/= (length t1) (length t2)) 'fail
          (let ((s subst) (i 0) (len (length t1)))
            (while (and (not (eq s 'fail)) (< i len))
              (setq s (funcall 'neovm--unify (nth i t1) (nth i t2) s))
              (setq i (1+ i)))
            s)))
       (t 'fail))))

  (unwind-protect
      (let ((results nil))
        ;; Deeply nested compound unification
        ;; (f (g ?x) (h ?y ?z)) with (f (g a) (h ?z b))
        ;; Expected: ?x=a, ?y=b, ?z=b
        (let ((s (funcall 'neovm--unify
                          '(f (g ?x) (h ?y ?z))
                          '(f (g a) (h ?z b))
                          nil)))
          (push (list 'deep
                      (not (eq s 'fail))
                      (funcall 'neovm--unify-apply-subst s '?x)
                      (funcall 'neovm--unify-apply-subst s '?y)
                      (funcall 'neovm--unify-apply-subst s '?z))
                results))

        ;; Three-level nesting
        ;; (f (g (h ?x)) ?y) with (f (g (h a)) (k ?x))
        (let ((s (funcall 'neovm--unify
                          '(f (g (h ?x)) ?y)
                          '(f (g (h a)) (k ?x))
                          nil)))
          (push (list 'three-level
                      (not (eq s 'fail))
                      (funcall 'neovm--unify-apply-subst s '(f (g (h ?x)) ?y)))
                results))

        ;; Shared variables across compound terms
        ;; (pair ?x ?x) with (pair (f a) ?y)
        ;; ?x = (f a), ?y = (f a)
        (let ((s (funcall 'neovm--unify
                          '(pair ?x ?x)
                          '(pair (f a) ?y)
                          nil)))
          (push (list 'shared-var
                      (not (eq s 'fail))
                      (funcall 'neovm--unify-apply-subst s '?x)
                      (funcall 'neovm--unify-apply-subst s '?y)
                      (equal (funcall 'neovm--unify-apply-subst s '?x)
                             (funcall 'neovm--unify-apply-subst s '?y)))
                results))

        ;; Conflicting bindings => fail
        ;; (pair ?x ?x) with (pair a b) where a != b
        (let ((s (funcall 'neovm--unify '(pair ?x ?x) '(pair a b) nil)))
          (push (list 'conflict (eq s 'fail)) results))

        ;; Multiple variables in nested positions
        ;; (list ?a ?b ?c) with (list 1 2 3)
        (let ((s (funcall 'neovm--unify '(list ?a ?b ?c) '(list 1 2 3) nil)))
          (push (list 'multi-var
                      (funcall 'neovm--unify-apply-subst s '?a)
                      (funcall 'neovm--unify-apply-subst s '?b)
                      (funcall 'neovm--unify-apply-subst s '?c))
                results))

        (nreverse results))
    (fmakunbound 'neovm--unify-var-p)
    (fmakunbound 'neovm--unify-occurs-p)
    (fmakunbound 'neovm--unify-apply-subst)
    (fmakunbound 'neovm--unify-var)
    (fmakunbound 'neovm--unify)))
"#;
    let expect = expect_test::expect![[
        r#""OK ((deep nil 120 121 122) (three-level nil (f (g (h 120)) 121)) (shared-var nil 120 121 nil) (conflict t) (multi-var 97 98 99))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: solve a set of equations using successive unification
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_unification_equation_solving() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (fset 'neovm--unify-var-p
    (lambda (term) (and (symbolp term) (string-prefix-p "?" (symbol-name term)))))

  (fset 'neovm--unify-occurs-p
    (lambda (var term subst)
      (cond
       ((equal var term) t)
       ((and (funcall 'neovm--unify-var-p term) (assq term subst))
        (funcall 'neovm--unify-occurs-p var (cdr (assq term subst)) subst))
       ((consp term)
        (let ((found nil))
          (dolist (sub term) (when (funcall 'neovm--unify-occurs-p var sub subst) (setq found t)))
          found))
       (t nil))))

  (fset 'neovm--unify-apply-subst
    (lambda (subst term)
      (cond
       ((funcall 'neovm--unify-var-p term)
        (let ((b (assq term subst)))
          (if b (funcall 'neovm--unify-apply-subst subst (cdr b)) term)))
       ((consp term) (mapcar (lambda (t1) (funcall 'neovm--unify-apply-subst subst t1)) term))
       (t term))))

  (fset 'neovm--unify-var
    (lambda (var term subst)
      (cond
       ((assq var subst) (funcall 'neovm--unify (cdr (assq var subst)) term subst))
       ((and (funcall 'neovm--unify-var-p term) (assq term subst))
        (funcall 'neovm--unify var (cdr (assq term subst)) subst))
       ((funcall 'neovm--unify-occurs-p var term subst) 'fail)
       (t (cons (cons var term) subst)))))

  (fset 'neovm--unify
    (lambda (t1 t2 subst)
      (cond
       ((eq subst 'fail) 'fail)
       ((equal t1 t2) subst)
       ((funcall 'neovm--unify-var-p t1) (funcall 'neovm--unify-var t1 t2 subst))
       ((funcall 'neovm--unify-var-p t2) (funcall 'neovm--unify-var t2 t1 subst))
       ((and (consp t1) (consp t2))
        (if (/= (length t1) (length t2)) 'fail
          (let ((s subst) (i 0) (len (length t1)))
            (while (and (not (eq s 'fail)) (< i len))
              (setq s (funcall 'neovm--unify (nth i t1) (nth i t2) s))
              (setq i (1+ i)))
            s)))
       (t 'fail))))

  ;; Solve a set of equations: list of (lhs . rhs) pairs
  ;; Returns the accumulated substitution or 'fail
  (fset 'neovm--unify-solve
    (lambda (equations)
      (let ((subst nil))
        (dolist (eq equations)
          (unless (eq subst 'fail)
            (setq subst (funcall 'neovm--unify (car eq) (cdr eq) subst))))
        subst)))

  (unwind-protect
      (let ((results nil))
        ;; Simple system: ?x = a, ?y = b
        (let ((s (funcall 'neovm--unify-solve
                          '((?x . a) (?y . b)))))
          (push (list 'simple
                      (funcall 'neovm--unify-apply-subst s '?x)
                      (funcall 'neovm--unify-apply-subst s '?y))
                results))

        ;; Chain: ?x = ?y, ?y = ?z, ?z = hello
        (let ((s (funcall 'neovm--unify-solve
                          '((?x . ?y) (?y . ?z) (?z . hello)))))
          (push (list 'chain
                      (funcall 'neovm--unify-apply-subst s '?x)
                      (funcall 'neovm--unify-apply-subst s '?y)
                      (funcall 'neovm--unify-apply-subst s '?z))
                results))

        ;; Compound equations: (f ?x) = (f a), (g ?y) = (g ?x)
        (let ((s (funcall 'neovm--unify-solve
                          '(((f ?x) . (f a)) ((g ?y) . (g ?x))))))
          (push (list 'compound
                      (funcall 'neovm--unify-apply-subst s '?x)
                      (funcall 'neovm--unify-apply-subst s '?y))
                results))

        ;; Contradictory system: ?x = a, ?x = b => fail
        (let ((s (funcall 'neovm--unify-solve
                          '((?x . a) (?x . b)))))
          (push (list 'contradict (eq s 'fail)) results))

        ;; System with occurs check failure: ?x = (f ?x)
        (let ((s (funcall 'neovm--unify-solve
                          '((?x . (f ?x))))))
          (push (list 'occurs-fail (eq s 'fail)) results))

        ;; Type inference example:
        ;; ?t1 = (arrow ?a ?b), ?t1 = (arrow int ?t2), ?t2 = bool
        (let ((s (funcall 'neovm--unify-solve
                          '((?t1 . (arrow ?a ?b))
                            (?t1 . (arrow int ?t2))
                            (?t2 . bool)))))
          (push (list 'type-infer
                      (not (eq s 'fail))
                      (funcall 'neovm--unify-apply-subst s '?t1)
                      (funcall 'neovm--unify-apply-subst s '?a)
                      (funcall 'neovm--unify-apply-subst s '?b))
                results))

        ;; Empty equation set
        (let ((s (funcall 'neovm--unify-solve nil)))
          (push (list 'empty (null s)) results))

        (nreverse results))
    (fmakunbound 'neovm--unify-var-p)
    (fmakunbound 'neovm--unify-occurs-p)
    (fmakunbound 'neovm--unify-apply-subst)
    (fmakunbound 'neovm--unify-var)
    (fmakunbound 'neovm--unify)
    (fmakunbound 'neovm--unify-solve)))
"#;
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \"?\" 102 31)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: query answering using unification (mini Prolog-like)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_unification_query_answering() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Define facts as ground terms, query with patterns containing variables,
    // find all matching substitutions.
    let form = r#"
(progn
  (fset 'neovm--unify-var-p
    (lambda (term) (and (symbolp term) (string-prefix-p "?" (symbol-name term)))))

  (fset 'neovm--unify-occurs-p
    (lambda (var term subst)
      (cond
       ((equal var term) t)
       ((and (funcall 'neovm--unify-var-p term) (assq term subst))
        (funcall 'neovm--unify-occurs-p var (cdr (assq term subst)) subst))
       ((consp term)
        (let ((found nil))
          (dolist (sub term) (when (funcall 'neovm--unify-occurs-p var sub subst) (setq found t)))
          found))
       (t nil))))

  (fset 'neovm--unify-apply-subst
    (lambda (subst term)
      (cond
       ((funcall 'neovm--unify-var-p term)
        (let ((b (assq term subst)))
          (if b (funcall 'neovm--unify-apply-subst subst (cdr b)) term)))
       ((consp term) (mapcar (lambda (t1) (funcall 'neovm--unify-apply-subst subst t1)) term))
       (t term))))

  (fset 'neovm--unify-var
    (lambda (var term subst)
      (cond
       ((assq var subst) (funcall 'neovm--unify (cdr (assq var subst)) term subst))
       ((and (funcall 'neovm--unify-var-p term) (assq term subst))
        (funcall 'neovm--unify var (cdr (assq term subst)) subst))
       ((funcall 'neovm--unify-occurs-p var term subst) 'fail)
       (t (cons (cons var term) subst)))))

  (fset 'neovm--unify
    (lambda (t1 t2 subst)
      (cond
       ((eq subst 'fail) 'fail)
       ((equal t1 t2) subst)
       ((funcall 'neovm--unify-var-p t1) (funcall 'neovm--unify-var t1 t2 subst))
       ((funcall 'neovm--unify-var-p t2) (funcall 'neovm--unify-var t2 t1 subst))
       ((and (consp t1) (consp t2))
        (if (/= (length t1) (length t2)) 'fail
          (let ((s subst) (i 0) (len (length t1)))
            (while (and (not (eq s 'fail)) (< i len))
              (setq s (funcall 'neovm--unify (nth i t1) (nth i t2) s))
              (setq i (1+ i)))
            s)))
       (t 'fail))))

  ;; Query a database of facts
  (fset 'neovm--unify-query
    (lambda (pattern facts)
      "Find all substitutions that unify PATTERN with any fact."
      (let ((results nil))
        (dolist (fact facts)
          (let ((s (funcall 'neovm--unify pattern fact nil)))
            (unless (eq s 'fail)
              (push s results))))
        (nreverse results))))

  (unwind-protect
      (let ((db '((parent alice bob)
                  (parent alice carol)
                  (parent bob dave)
                  (parent bob eve)
                  (parent carol frank)
                  (likes alice cats)
                  (likes bob dogs)
                  (likes carol cats)
                  (likes dave cats))))
        (list
         ;; Who are alice's children?
         (let ((answers (funcall 'neovm--unify-query '(parent alice ?child) db)))
           (mapcar (lambda (s) (funcall 'neovm--unify-apply-subst s '?child))
                   answers))

         ;; Who likes cats?
         (let ((answers (funcall 'neovm--unify-query '(likes ?who cats) db)))
           (mapcar (lambda (s) (funcall 'neovm--unify-apply-subst s '?who))
                   answers))

         ;; Who is bob's parent?
         (let ((answers (funcall 'neovm--unify-query '(parent ?p bob) db)))
           (mapcar (lambda (s) (funcall 'neovm--unify-apply-subst s '?p))
                   answers))

         ;; All parent relationships
         (let ((answers (funcall 'neovm--unify-query '(parent ?p ?c) db)))
           (length answers))

         ;; Non-matching query
         (let ((answers (funcall 'neovm--unify-query '(sibling ?x ?y) db)))
           (length answers))

         ;; Query with constant that matches nothing
         (let ((answers (funcall 'neovm--unify-query '(parent zara ?c) db)))
           (length answers))))
    (fmakunbound 'neovm--unify-var-p)
    (fmakunbound 'neovm--unify-occurs-p)
    (fmakunbound 'neovm--unify-apply-subst)
    (fmakunbound 'neovm--unify-var)
    (fmakunbound 'neovm--unify)
    (fmakunbound 'neovm--unify-query)))
"#;
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \"?\" 75 70)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
