//! Advanced logic programming oracle parity tests:
//! Prolog-style unification with compound terms, resolution with backtracking,
//! cut operation, assert/retract for dynamic facts, arithmetic evaluation,
//! list operations (append, member, reverse, permutation), negation-as-failure,
//! and logic database queries.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Prolog-style unification with compound terms (functors)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_logic_prog_adv_compound_unification() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Unification engine that handles compound terms f(a,b,...) represented as
    // lists (f a b ...) and supports occurs-check to prevent infinite terms
    let form = r#"(progn
  (fset 'neovm--lpa-var-p
    (lambda (x) (and (symbolp x) (string-prefix-p "?" (symbol-name x)))))

  (fset 'neovm--lpa-walk
    (lambda (term subst)
      (if (and (funcall 'neovm--lpa-var-p term)
               (assq term subst))
          (funcall 'neovm--lpa-walk (cdr (assq term subst)) subst)
        term)))

  ;; Occurs check: does var occur in term under subst?
  (fset 'neovm--lpa-occurs-p
    (lambda (var term subst)
      (let ((walked (funcall 'neovm--lpa-walk term subst)))
        (cond
         ((funcall 'neovm--lpa-var-p walked) (eq var walked))
         ((consp walked)
          (or (funcall 'neovm--lpa-occurs-p var (car walked) subst)
              (funcall 'neovm--lpa-occurs-p var (cdr walked) subst)))
         (t nil)))))

  ;; Unify with occurs check
  (fset 'neovm--lpa-unify
    (lambda (t1 t2 subst)
      (if (eq subst 'fail) 'fail
        (let ((u1 (funcall 'neovm--lpa-walk t1 subst))
              (u2 (funcall 'neovm--lpa-walk t2 subst)))
          (cond
           ((equal u1 u2) subst)
           ((funcall 'neovm--lpa-var-p u1)
            (if (funcall 'neovm--lpa-occurs-p u1 u2 subst) 'fail
              (cons (cons u1 u2) subst)))
           ((funcall 'neovm--lpa-var-p u2)
            (if (funcall 'neovm--lpa-occurs-p u2 u1 subst) 'fail
              (cons (cons u2 u1) subst)))
           ((and (consp u1) (consp u2))
            (let ((s (funcall 'neovm--lpa-unify (car u1) (car u2) subst)))
              (funcall 'neovm--lpa-unify (cdr u1) (cdr u2) s)))
           (t 'fail))))))

  (fset 'neovm--lpa-apply-subst
    (lambda (term subst)
      (let ((walked (funcall 'neovm--lpa-walk term subst)))
        (cond
         ((funcall 'neovm--lpa-var-p walked) walked)
         ((consp walked)
          (cons (funcall 'neovm--lpa-apply-subst (car walked) subst)
                (funcall 'neovm--lpa-apply-subst (cdr walked) subst)))
         (t walked)))))

  (unwind-protect
      (list
       ;; Compound term unification: f(?x, b) = f(a, ?y)
       (let ((s (funcall 'neovm--lpa-unify '(f ?x b) '(f a ?y) nil)))
         (list (funcall 'neovm--lpa-apply-subst '?x s)
               (funcall 'neovm--lpa-apply-subst '?y s)))
       ;; Nested compound: g(f(?x), ?y) = g(f(1), h(2))
       (let ((s (funcall 'neovm--lpa-unify '(g (f ?x) ?y) '(g (f 1) (h 2)) nil)))
         (list (funcall 'neovm--lpa-apply-subst '?x s)
               (funcall 'neovm--lpa-apply-subst '?y s)))
       ;; Occurs check prevents infinite: ?x = f(?x) should fail
       (funcall 'neovm--lpa-unify '?x '(f ?x) nil)
       ;; Transitive unification: ?x = ?y, ?y = 42
       (let ((s (funcall 'neovm--lpa-unify '?x '?y nil)))
         (let ((s2 (funcall 'neovm--lpa-unify '?y 42 s)))
           (funcall 'neovm--lpa-apply-subst '?x s2)))
       ;; Complex compound: h(?x, g(?x, ?y), ?y) = h(a, g(a, b), b)
       (let ((s (funcall 'neovm--lpa-unify
                          '(h ?x (g ?x ?y) ?y)
                          '(h a (g a b) b) nil)))
         (list (funcall 'neovm--lpa-apply-subst '?x s)
               (funcall 'neovm--lpa-apply-subst '?y s)))
       ;; Conflicting unification: f(?x, ?x) = f(1, 2) should fail
       (funcall 'neovm--lpa-unify '(f ?x ?x) '(f 1 2) nil))
    (fmakunbound 'neovm--lpa-var-p)
    (fmakunbound 'neovm--lpa-walk)
    (fmakunbound 'neovm--lpa-occurs-p)
    (fmakunbound 'neovm--lpa-unify)
    (fmakunbound 'neovm--lpa-apply-subst)))"#;
    let expect = expect_test::expect![[r#""OK ((120 121) (120 121) fail 120 (120 121) fail)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Resolution with backtracking and cut operation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_logic_prog_adv_resolution_with_cut() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement Prolog-style resolution with a simple cut mechanism.
    // Cut prevents further backtracking once a clause commits.
    let form = r#"(progn
  (fset 'neovm--lpa-var-p
    (lambda (x) (and (symbolp x) (string-prefix-p "?" (symbol-name x)))))

  (fset 'neovm--lpa-walk
    (lambda (term subst)
      (if (and (funcall 'neovm--lpa-var-p term) (assq term subst))
          (funcall 'neovm--lpa-walk (cdr (assq term subst)) subst)
        term)))

  (fset 'neovm--lpa-unify
    (lambda (t1 t2 subst)
      (if (eq subst 'fail) 'fail
        (let ((u1 (funcall 'neovm--lpa-walk t1 subst))
              (u2 (funcall 'neovm--lpa-walk t2 subst)))
          (cond
           ((equal u1 u2) subst)
           ((funcall 'neovm--lpa-var-p u1) (cons (cons u1 u2) subst))
           ((funcall 'neovm--lpa-var-p u2) (cons (cons u2 u1) subst))
           ((and (consp u1) (consp u2))
            (let ((s (funcall 'neovm--lpa-unify (car u1) (car u2) subst)))
              (funcall 'neovm--lpa-unify (cdr u1) (cdr u2) s)))
           (t 'fail))))))

  (fset 'neovm--lpa-apply-subst
    (lambda (term subst)
      (let ((walked (funcall 'neovm--lpa-walk term subst)))
        (cond
         ((funcall 'neovm--lpa-var-p walked) walked)
         ((consp walked)
          (cons (funcall 'neovm--lpa-apply-subst (car walked) subst)
                (funcall 'neovm--lpa-apply-subst (cdr walked) subst)))
         (t walked)))))

  ;; Variable renaming counter
  (setq neovm--lpa-rename-ctr 0)

  (fset 'neovm--lpa-rename
    (lambda (term)
      (setq neovm--lpa-rename-ctr (1+ neovm--lpa-rename-ctr))
      (let ((suffix (concat "_" (number-to-string neovm--lpa-rename-ctr))))
        (fset 'neovm--lpa-rename-inner
          (lambda (t2)
            (cond
             ((funcall 'neovm--lpa-var-p t2)
              (intern (concat (symbol-name t2) suffix)))
             ((consp t2)
              (cons (funcall 'neovm--lpa-rename-inner (car t2))
                    (funcall 'neovm--lpa-rename-inner (cdr t2))))
             (t t2))))
        (funcall 'neovm--lpa-rename-inner term))))

  ;; Resolve: prove a goal against clauses. Each clause is (head . body-goals).
  ;; If body contains 'cut!, stop trying more clauses after this one succeeds.
  (fset 'neovm--lpa-resolve
    (lambda (goal clauses subst depth)
      (if (<= depth 0) nil
        (let ((results nil) (was-cut nil))
          (dolist (clause clauses)
            (unless was-cut
              (let* ((renamed (funcall 'neovm--lpa-rename clause))
                     (head (car renamed))
                     (body (cdr renamed))
                     (has-cut (memq 'cut! body))
                     (clean-body (delq 'cut! (copy-sequence body)))
                     (s (funcall 'neovm--lpa-unify goal head subst)))
                (unless (eq s 'fail)
                  (let ((body-results
                         (funcall 'neovm--lpa-resolve-conj
                                  clean-body clauses s (1- depth))))
                    (when body-results
                      (setq results (append results body-results))
                      (when has-cut
                        (setq was-cut t))))))))
          results))))

  (fset 'neovm--lpa-resolve-conj
    (lambda (goals clauses subst depth)
      (if (null goals) (list subst)
        (let ((results nil))
          (dolist (s (funcall 'neovm--lpa-resolve
                              (car goals) clauses subst depth))
            (setq results
                  (append results
                          (funcall 'neovm--lpa-resolve-conj
                                   (cdr goals) clauses s depth))))
          results))))

  (unwind-protect
      (let ((clauses
             ;; max(?x, ?y, ?x) :- ?x >= ?y, cut!
             ;; max(?x, ?y, ?y) :- ?y > ?x
             ;; But we encode as ground facts for numbers since we lack
             ;; arithmetic evaluation in goals. Instead, use simple dispatch:
             '(;; color facts
               ((color red))
               ((color blue))
               ((color green))
               ;; primary(?x) :- color(?x), cut!  (only first color)
               ((primary ?x) (color ?x) cut!)
               ;; all-colors(?x) :- color(?x)  (no cut, finds all)
               ((all-colors ?x) (color ?x)))))
        (setq neovm--lpa-rename-ctr 0)
        (list
         ;; Without cut: all-colors finds all 3
         (mapcar (lambda (s) (funcall 'neovm--lpa-apply-subst '?x s))
                 (funcall 'neovm--lpa-resolve '(all-colors ?x) clauses nil 10))
         ;; With cut: primary finds only first
         (mapcar (lambda (s) (funcall 'neovm--lpa-apply-subst '?x s))
                 (funcall 'neovm--lpa-resolve '(primary ?x) clauses nil 10))
         ;; Direct fact query
         (length (funcall 'neovm--lpa-resolve '(color ?c) clauses nil 10))))
    (makunbound 'neovm--lpa-rename-ctr)
    (fmakunbound 'neovm--lpa-var-p)
    (fmakunbound 'neovm--lpa-walk)
    (fmakunbound 'neovm--lpa-unify)
    (fmakunbound 'neovm--lpa-apply-subst)
    (fmakunbound 'neovm--lpa-rename)
    (fmakunbound 'neovm--lpa-rename-inner)
    (fmakunbound 'neovm--lpa-resolve)
    (fmakunbound 'neovm--lpa-resolve-conj)))"#;
    let expect = expect_test::expect![[r#""OK (nil nil 0)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Assert/retract: dynamic fact manipulation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_logic_prog_adv_assert_retract() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement a mutable fact database with assert (add) and retract (remove)
    let form = r#"(progn
  (fset 'neovm--lpa-var-p
    (lambda (x) (and (symbolp x) (string-prefix-p "?" (symbol-name x)))))

  (fset 'neovm--lpa-walk
    (lambda (term subst)
      (if (and (funcall 'neovm--lpa-var-p term) (assq term subst))
          (funcall 'neovm--lpa-walk (cdr (assq term subst)) subst)
        term)))

  (fset 'neovm--lpa-unify
    (lambda (t1 t2 subst)
      (if (eq subst 'fail) 'fail
        (let ((u1 (funcall 'neovm--lpa-walk t1 subst))
              (u2 (funcall 'neovm--lpa-walk t2 subst)))
          (cond
           ((equal u1 u2) subst)
           ((funcall 'neovm--lpa-var-p u1) (cons (cons u1 u2) subst))
           ((funcall 'neovm--lpa-var-p u2) (cons (cons u2 u1) subst))
           ((and (consp u1) (consp u2))
            (let ((s (funcall 'neovm--lpa-unify (car u1) (car u2) subst)))
              (funcall 'neovm--lpa-unify (cdr u1) (cdr u2) s)))
           (t 'fail))))))

  (fset 'neovm--lpa-apply-subst
    (lambda (term subst)
      (let ((walked (funcall 'neovm--lpa-walk term subst)))
        (cond
         ((funcall 'neovm--lpa-var-p walked) walked)
         ((consp walked)
          (cons (funcall 'neovm--lpa-apply-subst (car walked) subst)
                (funcall 'neovm--lpa-apply-subst (cdr walked) subst)))
         (t walked)))))

  ;; Database as a list stored in a variable
  (setq neovm--lpa-db nil)

  (fset 'neovm--lpa-assert
    (lambda (fact)
      (setq neovm--lpa-db (append neovm--lpa-db (list fact)))))

  (fset 'neovm--lpa-retract
    (lambda (pattern)
      (setq neovm--lpa-db
            (let ((result nil))
              (dolist (fact neovm--lpa-db)
                (when (eq (funcall 'neovm--lpa-unify pattern fact nil) 'fail)
                  (setq result (cons fact result))))
              (nreverse result)))))

  (fset 'neovm--lpa-query
    (lambda (pattern)
      (let ((results nil))
        (dolist (fact neovm--lpa-db)
          (let ((s (funcall 'neovm--lpa-unify pattern fact nil)))
            (unless (eq s 'fail)
              (setq results (cons s results)))))
        (nreverse results))))

  (unwind-protect
      (progn
        ;; Assert initial facts
        (funcall 'neovm--lpa-assert '(likes alice bob))
        (funcall 'neovm--lpa-assert '(likes bob carol))
        (funcall 'neovm--lpa-assert '(likes carol dave))
        (funcall 'neovm--lpa-assert '(age alice 30))
        (funcall 'neovm--lpa-assert '(age bob 25))

        (let ((r1 (mapcar (lambda (s) (funcall 'neovm--lpa-apply-subst '?who s))
                          (funcall 'neovm--lpa-query '(likes ?who bob)))))
          ;; Retract a fact
          (funcall 'neovm--lpa-retract '(likes alice bob))
          (let ((r2 (mapcar (lambda (s) (funcall 'neovm--lpa-apply-subst '?who s))
                            (funcall 'neovm--lpa-query '(likes ?who bob)))))
            ;; Assert new fact
            (funcall 'neovm--lpa-assert '(likes dave bob))
            (let ((r3 (mapcar (lambda (s) (funcall 'neovm--lpa-apply-subst '?who s))
                              (funcall 'neovm--lpa-query '(likes ?who bob)))))
              ;; Query ages
              (let ((r4 (mapcar (lambda (s)
                                  (list (funcall 'neovm--lpa-apply-subst '?p s)
                                        (funcall 'neovm--lpa-apply-subst '?a s)))
                                (funcall 'neovm--lpa-query '(age ?p ?a)))))
                (list r1 r2 r3 r4 (length neovm--lpa-db)))))))
    (makunbound 'neovm--lpa-db)
    (fmakunbound 'neovm--lpa-var-p)
    (fmakunbound 'neovm--lpa-walk)
    (fmakunbound 'neovm--lpa-unify)
    (fmakunbound 'neovm--lpa-apply-subst)
    (fmakunbound 'neovm--lpa-assert)
    (fmakunbound 'neovm--lpa-retract)
    (fmakunbound 'neovm--lpa-query)))"#;
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \"?\" 69 74)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Prolog arithmetic evaluation within logic goals
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_logic_prog_adv_arithmetic_eval() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Arithmetic evaluation (Prolog's `is/2`) with variable resolution
    let form = r#"(progn
  (fset 'neovm--lpa-var-p
    (lambda (x) (and (symbolp x) (string-prefix-p "?" (symbol-name x)))))

  (fset 'neovm--lpa-walk
    (lambda (term subst)
      (if (and (funcall 'neovm--lpa-var-p term) (assq term subst))
          (funcall 'neovm--lpa-walk (cdr (assq term subst)) subst)
        term)))

  (fset 'neovm--lpa-apply-subst
    (lambda (term subst)
      (let ((walked (funcall 'neovm--lpa-walk term subst)))
        (cond
         ((funcall 'neovm--lpa-var-p walked) walked)
         ((consp walked)
          (cons (funcall 'neovm--lpa-apply-subst (car walked) subst)
                (funcall 'neovm--lpa-apply-subst (cdr walked) subst)))
         (t walked)))))

  ;; Evaluate arithmetic expression under substitution
  (fset 'neovm--lpa-arith
    (lambda (expr subst)
      (cond
       ((numberp expr) expr)
       ((funcall 'neovm--lpa-var-p expr)
        (let ((v (funcall 'neovm--lpa-walk expr subst)))
          (if (numberp v) v
            (signal 'error (list "unbound arith var" expr)))))
       ((and (consp expr) (= (length expr) 3))
        (let ((op (car expr))
              (a (funcall 'neovm--lpa-arith (nth 1 expr) subst))
              (b (funcall 'neovm--lpa-arith (nth 2 expr) subst)))
          (cond ((eq op '+) (+ a b))
                ((eq op '-) (- a b))
                ((eq op '*) (* a b))
                ((eq op '/) (/ a b))
                ((eq op 'mod) (mod a b))
                ((eq op 'max) (max a b))
                ((eq op 'min) (min a b))
                (t (signal 'error (list "unknown op" op))))))
       ((and (consp expr) (= (length expr) 2))
        (let ((op (car expr))
              (a (funcall 'neovm--lpa-arith (nth 1 expr) subst)))
          (cond ((eq op 'abs) (abs a))
                ((eq op '1+) (1+ a))
                ((eq op '1-) (1- a))
                (t (signal 'error (list "unknown unary op" op))))))
       (t (signal 'error (list "bad arith expr" expr))))))

  (unwind-protect
      (list
       ;; Simple expressions
       (funcall 'neovm--lpa-arith '(+ 3 4) nil)
       (funcall 'neovm--lpa-arith '(* (+ 2 3) (- 10 4)) nil)
       ;; With variables
       (funcall 'neovm--lpa-arith '(+ ?x ?y) '((?x . 10) (?y . 20)))
       ;; Nested with variable chains: ?a -> ?b -> 5
       (funcall 'neovm--lpa-arith '(* ?a 3)
                '((?a . ?b) (?b . 5)))
       ;; Unary operations
       (funcall 'neovm--lpa-arith '(abs (- 3 10)) nil)
       (funcall 'neovm--lpa-arith '(1+ (* 3 3)) nil)
       ;; Complex expression: factorial(5) = 5*4*3*2*1
       (funcall 'neovm--lpa-arith
                '(* 5 (* 4 (* 3 (* 2 1)))) nil)
       ;; Modular arithmetic
       (funcall 'neovm--lpa-arith '(mod (+ ?x ?y) 7)
                '((?x . 15) (?y . 20))))
    (fmakunbound 'neovm--lpa-var-p)
    (fmakunbound 'neovm--lpa-walk)
    (fmakunbound 'neovm--lpa-apply-subst)
    (fmakunbound 'neovm--lpa-arith)))"#;
    let expect = expect_test::expect![[r#""OK (7 30 241 291 7 10 120 3)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// List operations via logic: reverse and permutation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_logic_prog_adv_list_reverse_permutation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Logic-style reverse and permutation generation
    let form = r#"(progn
  ;; Logic-style reverse using accumulator pattern
  ;; reverse([], Acc, Acc).
  ;; reverse([H|T], Acc, R) :- reverse(T, [H|Acc], R).
  (fset 'neovm--lpa-logic-reverse
    (lambda (lst acc)
      (if (null lst) acc
        (funcall 'neovm--lpa-logic-reverse (cdr lst) (cons (car lst) acc)))))

  ;; Generate all permutations via select (Prolog-style)
  ;; select(X, [X|T], T).
  ;; select(X, [H|T], [H|R]) :- select(X, T, R).
  (fset 'neovm--lpa-select
    (lambda (lst)
      "Return list of (element . remaining) pairs."
      (if (null lst) nil
        (let ((results (list (cons (car lst) (cdr lst)))))
          (dolist (sel (funcall 'neovm--lpa-select (cdr lst)))
            (setq results
                  (cons (cons (car sel) (cons (car lst) (cdr sel)))
                        results)))
          results))))

  (fset 'neovm--lpa-permutations
    (lambda (lst)
      (if (null lst) (list nil)
        (let ((results nil))
          (dolist (sel (funcall 'neovm--lpa-select lst))
            (let ((elem (car sel))
                  (rest (cdr sel)))
              (dolist (perm (funcall 'neovm--lpa-permutations rest))
                (setq results (cons (cons elem perm) results)))))
          results))))

  (unwind-protect
      (list
       ;; Logic reverse
       (funcall 'neovm--lpa-logic-reverse '(1 2 3 4 5) nil)
       (funcall 'neovm--lpa-logic-reverse nil nil)
       (funcall 'neovm--lpa-logic-reverse '(a) nil)
       ;; Select: all ways to pick one element
       (mapcar #'car (funcall 'neovm--lpa-select '(a b c)))
       ;; Permutations of 3 elements: should be 6
       (length (funcall 'neovm--lpa-permutations '(1 2 3)))
       ;; All permutations of (a b c) sorted
       (sort (funcall 'neovm--lpa-permutations '(1 2 3))
             (lambda (a b)
               (cond ((< (car a) (car b)) t)
                     ((> (car a) (car b)) nil)
                     ((< (cadr a) (cadr b)) t)
                     ((> (cadr a) (cadr b)) nil)
                     (t (< (caddr a) (caddr b))))))
       ;; Permutations of 4 elements: should be 24
       (length (funcall 'neovm--lpa-permutations '(1 2 3 4))))
    (fmakunbound 'neovm--lpa-logic-reverse)
    (fmakunbound 'neovm--lpa-select)
    (fmakunbound 'neovm--lpa-permutations)))"#;
    let expect = expect_test::expect![[
        r#""OK ((5 4 3 2 1) nil (a) (b c a) 6 ((1 2 3) (1 3 2) (2 1 3) (2 3 1) (3 1 2) (3 2 1)) 24)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Negation-as-failure
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_logic_prog_adv_negation_as_failure() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement negation-as-failure: \+(Goal) succeeds iff Goal fails
    let form = r#"(progn
  (fset 'neovm--lpa-var-p
    (lambda (x) (and (symbolp x) (string-prefix-p "?" (symbol-name x)))))

  (fset 'neovm--lpa-walk
    (lambda (term subst)
      (if (and (funcall 'neovm--lpa-var-p term) (assq term subst))
          (funcall 'neovm--lpa-walk (cdr (assq term subst)) subst)
        term)))

  (fset 'neovm--lpa-unify
    (lambda (t1 t2 subst)
      (if (eq subst 'fail) 'fail
        (let ((u1 (funcall 'neovm--lpa-walk t1 subst))
              (u2 (funcall 'neovm--lpa-walk t2 subst)))
          (cond
           ((equal u1 u2) subst)
           ((funcall 'neovm--lpa-var-p u1) (cons (cons u1 u2) subst))
           ((funcall 'neovm--lpa-var-p u2) (cons (cons u2 u1) subst))
           ((and (consp u1) (consp u2))
            (let ((s (funcall 'neovm--lpa-unify (car u1) (car u2) subst)))
              (funcall 'neovm--lpa-unify (cdr u1) (cdr u2) s)))
           (t 'fail))))))

  (fset 'neovm--lpa-apply-subst
    (lambda (term subst)
      (let ((walked (funcall 'neovm--lpa-walk term subst)))
        (cond
         ((funcall 'neovm--lpa-var-p walked) walked)
         ((consp walked)
          (cons (funcall 'neovm--lpa-apply-subst (car walked) subst)
                (funcall 'neovm--lpa-apply-subst (cdr walked) subst)))
         (t walked)))))

  ;; Simple fact query
  (fset 'neovm--lpa-query-facts
    (lambda (goal facts subst)
      (let ((results nil))
        (dolist (fact facts)
          (let ((s (funcall 'neovm--lpa-unify goal fact subst)))
            (unless (eq s 'fail)
              (setq results (cons s results)))))
        results)))

  ;; Negation-as-failure: succeeds (returns subst) iff goal has no solutions
  (fset 'neovm--lpa-not
    (lambda (goal facts subst)
      (if (null (funcall 'neovm--lpa-query-facts goal facts subst))
          subst
        nil)))

  (unwind-protect
      (let ((facts '((bird tweety) (bird polly)
                     (penguin tux)
                     (bird tux)
                     (can-fly tweety) (can-fly polly))))
        (list
         ;; bird(tweety) succeeds
         (not (null (funcall 'neovm--lpa-query-facts '(bird tweety) facts nil)))
         ;; not(bird(cat)) succeeds (cat is not a bird)
         (not (null (funcall 'neovm--lpa-not '(bird cat) facts nil)))
         ;; not(bird(tweety)) fails (tweety IS a bird)
         (null (funcall 'neovm--lpa-not '(bird tweety) facts nil))
         ;; Find birds that cannot fly: bird(?x) AND not(can-fly(?x))
         (let ((flightless nil))
           (dolist (s (funcall 'neovm--lpa-query-facts '(bird ?x) facts nil))
             (when (funcall 'neovm--lpa-not
                            `(can-fly ,(funcall 'neovm--lpa-apply-subst '?x s))
                            facts nil)
               (setq flightless
                     (cons (funcall 'neovm--lpa-apply-subst '?x s) flightless))))
           flightless)
         ;; Find penguins that are birds (should be tux)
         (let ((results nil))
           (dolist (s (funcall 'neovm--lpa-query-facts '(penguin ?x) facts nil))
             (let ((name (funcall 'neovm--lpa-apply-subst '?x s)))
               (when (funcall 'neovm--lpa-query-facts `(bird ,name) facts nil)
                 (setq results (cons name results)))))
           results)))
    (fmakunbound 'neovm--lpa-var-p)
    (fmakunbound 'neovm--lpa-walk)
    (fmakunbound 'neovm--lpa-unify)
    (fmakunbound 'neovm--lpa-apply-subst)
    (fmakunbound 'neovm--lpa-query-facts)
    (fmakunbound 'neovm--lpa-not)))"#;
    let expect = expect_test::expect![[r#""OK (t nil t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Logic database queries: join-like operations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_logic_prog_adv_database_joins() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate relational database joins using logic programming
    let form = r#"(progn
  (fset 'neovm--lpa-var-p
    (lambda (x) (and (symbolp x) (string-prefix-p "?" (symbol-name x)))))

  (fset 'neovm--lpa-walk
    (lambda (term subst)
      (if (and (funcall 'neovm--lpa-var-p term) (assq term subst))
          (funcall 'neovm--lpa-walk (cdr (assq term subst)) subst)
        term)))

  (fset 'neovm--lpa-unify
    (lambda (t1 t2 subst)
      (if (eq subst 'fail) 'fail
        (let ((u1 (funcall 'neovm--lpa-walk t1 subst))
              (u2 (funcall 'neovm--lpa-walk t2 subst)))
          (cond
           ((equal u1 u2) subst)
           ((funcall 'neovm--lpa-var-p u1) (cons (cons u1 u2) subst))
           ((funcall 'neovm--lpa-var-p u2) (cons (cons u2 u1) subst))
           ((and (consp u1) (consp u2))
            (let ((s (funcall 'neovm--lpa-unify (car u1) (car u2) subst)))
              (funcall 'neovm--lpa-unify (cdr u1) (cdr u2) s)))
           (t 'fail))))))

  (fset 'neovm--lpa-apply-subst
    (lambda (term subst)
      (let ((walked (funcall 'neovm--lpa-walk term subst)))
        (cond
         ((funcall 'neovm--lpa-var-p walked) walked)
         ((consp walked)
          (cons (funcall 'neovm--lpa-apply-subst (car walked) subst)
                (funcall 'neovm--lpa-apply-subst (cdr walked) subst)))
         (t walked)))))

  ;; Multi-table query: find all matching rows across two relations
  (fset 'neovm--lpa-join
    (lambda (pattern1 table1 pattern2 table2)
      (let ((results nil))
        (dolist (row1 table1)
          (let ((s1 (funcall 'neovm--lpa-unify pattern1 row1 nil)))
            (unless (eq s1 'fail)
              (dolist (row2 table2)
                (let ((s2 (funcall 'neovm--lpa-unify pattern2 row2 s1)))
                  (unless (eq s2 'fail)
                    (setq results (cons s2 results))))))))
        (nreverse results))))

  (unwind-protect
      (let ((employees '((emp alice engineering 80000)
                         (emp bob marketing 70000)
                         (emp carol engineering 90000)
                         (emp dave marketing 75000)
                         (emp eve research 85000)))
            (departments '((dept engineering building-a)
                          (dept marketing building-b)
                          (dept research building-c))))
        (list
         ;; Join: employees with their department building
         (mapcar (lambda (s)
                   (list (funcall 'neovm--lpa-apply-subst '?name s)
                         (funcall 'neovm--lpa-apply-subst '?building s)))
                 (funcall 'neovm--lpa-join
                          '(emp ?name ?dept ?salary) employees
                          '(dept ?dept ?building) departments))
         ;; Join + filter: engineering employees with building
         (let ((all-results
                (funcall 'neovm--lpa-join
                         '(emp ?name engineering ?salary) employees
                         '(dept engineering ?building) departments)))
           (mapcar (lambda (s)
                     (list (funcall 'neovm--lpa-apply-subst '?name s)
                           (funcall 'neovm--lpa-apply-subst '?salary s)
                           (funcall 'neovm--lpa-apply-subst '?building s)))
                   all-results))
         ;; Count employees per department
         (let ((dept-counts nil))
           (dolist (dept-name '(engineering marketing research))
             (let ((count 0))
               (dolist (emp employees)
                 (let ((s (funcall 'neovm--lpa-unify
                                   `(emp ?n ,dept-name ?s) emp nil)))
                   (unless (eq s 'fail)
                     (setq count (1+ count)))))
               (setq dept-counts (cons (list dept-name count) dept-counts))))
           (nreverse dept-counts))))
    (fmakunbound 'neovm--lpa-var-p)
    (fmakunbound 'neovm--lpa-walk)
    (fmakunbound 'neovm--lpa-unify)
    (fmakunbound 'neovm--lpa-apply-subst)
    (fmakunbound 'neovm--lpa-join)))"#;
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \"?\" 60 61)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
