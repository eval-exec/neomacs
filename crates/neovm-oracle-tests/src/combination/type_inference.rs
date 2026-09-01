//! Oracle parity tests for a simple type inference engine in Elisp:
//! type environment management, type variables, unification algorithm,
//! type checking for lambda/application/let expressions,
//! constraint generation, constraint solving, and polymorphic types.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

// ---------------------------------------------------------------------------
// Type environment: create, extend, lookup with lexical scoping
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_type_infer_type_environment() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  ;; Type representation:
  ;;   (int), (bool), (string)             -- base types
  ;;   (-> arg-type ret-type)              -- function type
  ;;   (tvar N)                            -- type variable
  ;;   (forall (vars...) type)             -- polymorphic type
  ;;   (list-of type)                      -- list type

  ;; Type environment: alist of (name . type)
  (fset 'neovm--ti-env-empty (lambda () nil))
  (fset 'neovm--ti-env-extend
    (lambda (env name type)
      (cons (cons name type) env)))
  (fset 'neovm--ti-env-lookup
    (lambda (env name)
      (let ((entry (assq name env)))
        (if entry (cdr entry) nil))))
  (fset 'neovm--ti-env-extend-many
    (lambda (env bindings)
      "Extend env with multiple (name . type) pairs."
      (let ((result env))
        (dolist (b bindings)
          (setq result (cons b result)))
        result)))

  (unwind-protect
      (let* ((env0 (funcall 'neovm--ti-env-empty))
             (env1 (funcall 'neovm--ti-env-extend env0 'x '(int)))
             (env2 (funcall 'neovm--ti-env-extend env1 'y '(bool)))
             (env3 (funcall 'neovm--ti-env-extend env2 'f '(-> (int) (bool))))
             ;; Shadowing: extend x again with different type
             (env4 (funcall 'neovm--ti-env-extend env3 'x '(string)))
             ;; Multiple bindings at once
             (env5 (funcall 'neovm--ti-env-extend-many env0
                            '((a . (int)) (b . (bool)) (c . (string))))))
        (list
         ;; Lookups
         (funcall 'neovm--ti-env-lookup env1 'x)
         (funcall 'neovm--ti-env-lookup env2 'y)
         (funcall 'neovm--ti-env-lookup env3 'f)
         ;; Not found
         (funcall 'neovm--ti-env-lookup env0 'z)
         ;; Shadowing: x in env4 should be string, not int
         (funcall 'neovm--ti-env-lookup env4 'x)
         ;; But y is still accessible
         (funcall 'neovm--ti-env-lookup env4 'y)
         ;; Multiple bindings
         (funcall 'neovm--ti-env-lookup env5 'a)
         (funcall 'neovm--ti-env-lookup env5 'b)
         (funcall 'neovm--ti-env-lookup env5 'c)
         (funcall 'neovm--ti-env-lookup env5 'd)))
    (fmakunbound 'neovm--ti-env-empty)
    (fmakunbound 'neovm--ti-env-extend)
    (fmakunbound 'neovm--ti-env-lookup)
    (fmakunbound 'neovm--ti-env-extend-many)))
"#;
    let expect = expect_test::expect![[
        r#""OK ((int) (bool) (-> (int) (bool)) nil (string) (bool) (int) (bool) (string) nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Type variables and substitution
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_type_infer_type_variables_and_substitution() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  ;; Fresh type variable counter
  (defvar neovm--ti-tvar-counter 0)
  (fset 'neovm--ti-fresh-tvar
    (lambda ()
      (setq neovm--ti-tvar-counter (1+ neovm--ti-tvar-counter))
      (list 'tvar neovm--ti-tvar-counter)))

  ;; Substitution: alist of (tvar-id . type)
  (fset 'neovm--ti-subst-empty (lambda () nil))
  (fset 'neovm--ti-subst-add
    (lambda (subst tvar-id type)
      (cons (cons tvar-id type) subst)))

  ;; Apply substitution to a type
  (fset 'neovm--ti-apply-subst
    (lambda (subst type)
      (cond
       ((null type) nil)
       ((eq (car type) 'tvar)
        (let ((binding (assq (cadr type) subst)))
          (if binding
              ;; Recursively apply in case the binding itself contains tvars
              (funcall 'neovm--ti-apply-subst subst (cdr binding))
            type)))
       ((eq (car type) '->)
        (list '->
              (funcall 'neovm--ti-apply-subst subst (nth 1 type))
              (funcall 'neovm--ti-apply-subst subst (nth 2 type))))
       ((eq (car type) 'list-of)
        (list 'list-of
              (funcall 'neovm--ti-apply-subst subst (nth 1 type))))
       (t type))))

  ;; Collect free type variables
  (fset 'neovm--ti-free-tvars
    (lambda (type)
      (cond
       ((null type) nil)
       ((eq (car type) 'tvar) (list (cadr type)))
       ((eq (car type) '->)
        (append (funcall 'neovm--ti-free-tvars (nth 1 type))
                (funcall 'neovm--ti-free-tvars (nth 2 type))))
       ((eq (car type) 'list-of)
        (funcall 'neovm--ti-free-tvars (nth 1 type)))
       ((eq (car type) 'forall)
        (let ((bound (nth 1 type))
              (body-tvars (funcall 'neovm--ti-free-tvars (nth 2 type))))
          (let ((result nil))
            (dolist (tv body-tvars)
              (unless (memq tv bound)
                (setq result (cons tv result))))
            (nreverse result))))
       (t nil))))

  (unwind-protect
      (progn
        (setq neovm--ti-tvar-counter 0)
        (let* ((t1 (funcall 'neovm--ti-fresh-tvar))
               (t2 (funcall 'neovm--ti-fresh-tvar))
               (t3 (funcall 'neovm--ti-fresh-tvar))
               ;; Function type: t1 -> t2
               (fn-type (list '-> t1 t2))
               ;; Substitution: t1 = int, t2 = bool
               (subst (funcall 'neovm--ti-subst-add
                                (funcall 'neovm--ti-subst-add
                                          (funcall 'neovm--ti-subst-empty)
                                          1 '(int))
                                2 '(bool)))
               ;; Chain substitution: t3 = t1, then apply with t1 = int
               (subst2 (funcall 'neovm--ti-subst-add subst 3 '(tvar 1))))
          (list
           ;; Fresh tvars
           t1 t2 t3
           ;; Apply subst to fn-type: should become (-> (int) (bool))
           (funcall 'neovm--ti-apply-subst subst fn-type)
           ;; Apply subst to unbound tvar
           (funcall 'neovm--ti-apply-subst subst t3)
           ;; Apply chained subst: t3 -> t1 -> int
           (funcall 'neovm--ti-apply-subst subst2 t3)
           ;; Apply to nested: (-> t3 (-> t1 t2))
           (funcall 'neovm--ti-apply-subst subst2
                    (list '-> t3 (list '-> t1 t2)))
           ;; Free tvars
           (funcall 'neovm--ti-free-tvars fn-type)
           (funcall 'neovm--ti-free-tvars '(int))
           (funcall 'neovm--ti-free-tvars
                    (list 'forall '(1) (list '-> '(tvar 1) '(tvar 2))))
           ;; Apply subst to list type
           (funcall 'neovm--ti-apply-subst subst (list 'list-of t1)))))
    (fmakunbound 'neovm--ti-fresh-tvar)
    (fmakunbound 'neovm--ti-subst-empty)
    (fmakunbound 'neovm--ti-subst-add)
    (fmakunbound 'neovm--ti-apply-subst)
    (fmakunbound 'neovm--ti-free-tvars)
    (makunbound 'neovm--ti-tvar-counter)))
"#;
    let expect = expect_test::expect![[
        r#""OK ((tvar 1) (tvar 2) (tvar 3) (-> (int) (bool)) (tvar 3) (int) (-> (int) (-> (int) (bool))) (1 2) nil (2) (list-of (int)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Unification algorithm: unify two types, producing a substitution
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_type_infer_unification() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  ;; Apply substitution (standalone for this test)
  (fset 'neovm--ti-u-apply
    (lambda (subst type)
      (cond
       ((null type) nil)
       ((eq (car type) 'tvar)
        (let ((b (assq (cadr type) subst)))
          (if b (funcall 'neovm--ti-u-apply subst (cdr b)) type)))
       ((eq (car type) '->)
        (list '-> (funcall 'neovm--ti-u-apply subst (nth 1 type))
              (funcall 'neovm--ti-u-apply subst (nth 2 type))))
       ((eq (car type) 'list-of)
        (list 'list-of (funcall 'neovm--ti-u-apply subst (nth 1 type))))
       ((eq (car type) 'pair)
        (list 'pair (funcall 'neovm--ti-u-apply subst (nth 1 type))
              (funcall 'neovm--ti-u-apply subst (nth 2 type))))
       (t type))))

  ;; Occurs check: does tvar-id occur in type?
  (fset 'neovm--ti-u-occurs
    (lambda (tvar-id type)
      (cond
       ((null type) nil)
       ((eq (car type) 'tvar) (= (cadr type) tvar-id))
       ((eq (car type) '->)
        (or (funcall 'neovm--ti-u-occurs tvar-id (nth 1 type))
            (funcall 'neovm--ti-u-occurs tvar-id (nth 2 type))))
       ((eq (car type) 'list-of)
        (funcall 'neovm--ti-u-occurs tvar-id (nth 1 type)))
       ((eq (car type) 'pair)
        (or (funcall 'neovm--ti-u-occurs tvar-id (nth 1 type))
            (funcall 'neovm--ti-u-occurs tvar-id (nth 2 type))))
       (t nil))))

  ;; Compose substitutions
  (fset 'neovm--ti-u-compose
    (lambda (s1 s2)
      "Compose s1 after s2: apply s1 to all bindings in s2, then add s1."
      (let ((result nil))
        (dolist (b s2)
          (setq result (cons (cons (car b)
                                    (funcall 'neovm--ti-u-apply s1 (cdr b)))
                              result)))
        (dolist (b s1)
          (unless (assq (car b) result)
            (setq result (cons b result))))
        result)))

  ;; Unify two types, return (ok . subst) or (error . msg)
  (fset 'neovm--ti-u-unify
    (lambda (t1 t2 subst)
      (let ((t1 (funcall 'neovm--ti-u-apply subst t1))
            (t2 (funcall 'neovm--ti-u-apply subst t2)))
        (cond
         ;; Same type
         ((equal t1 t2) (cons 'ok subst))
         ;; t1 is tvar
         ((and (consp t1) (eq (car t1) 'tvar))
          (if (funcall 'neovm--ti-u-occurs (cadr t1) t2)
              (cons 'error "occurs-check")
            (cons 'ok (cons (cons (cadr t1) t2) subst))))
         ;; t2 is tvar
         ((and (consp t2) (eq (car t2) 'tvar))
          (if (funcall 'neovm--ti-u-occurs (cadr t2) t1)
              (cons 'error "occurs-check")
            (cons 'ok (cons (cons (cadr t2) t1) subst))))
         ;; Both function types
         ((and (consp t1) (eq (car t1) '->)
               (consp t2) (eq (car t2) '->))
          (let ((r1 (funcall 'neovm--ti-u-unify (nth 1 t1) (nth 1 t2) subst)))
            (if (eq (car r1) 'error)
                r1
              (funcall 'neovm--ti-u-unify (nth 2 t1) (nth 2 t2) (cdr r1)))))
         ;; Both list types
         ((and (consp t1) (eq (car t1) 'list-of)
               (consp t2) (eq (car t2) 'list-of))
          (funcall 'neovm--ti-u-unify (nth 1 t1) (nth 1 t2) subst))
         ;; Both pair types
         ((and (consp t1) (eq (car t1) 'pair)
               (consp t2) (eq (car t2) 'pair))
          (let ((r1 (funcall 'neovm--ti-u-unify (nth 1 t1) (nth 1 t2) subst)))
            (if (eq (car r1) 'error) r1
              (funcall 'neovm--ti-u-unify (nth 2 t1) (nth 2 t2) (cdr r1)))))
         ;; Mismatch
         (t (cons 'error (format "cannot unify %S with %S" t1 t2)))))))

  (unwind-protect
      (list
       ;; Unify tvar with concrete type
       (funcall 'neovm--ti-u-unify '(tvar 1) '(int) nil)
       ;; Unify two concrete types (same)
       (funcall 'neovm--ti-u-unify '(int) '(int) nil)
       ;; Unify two concrete types (different) -> error
       (funcall 'neovm--ti-u-unify '(int) '(bool) nil)
       ;; Unify function types
       (funcall 'neovm--ti-u-unify '(-> (tvar 1) (tvar 2)) '(-> (int) (bool)) nil)
       ;; Unify with existing substitution
       (funcall 'neovm--ti-u-unify '(tvar 2) '(int) '((1 . (bool))))
       ;; Occurs check: tvar 1 = (-> (tvar 1) (int)) should fail
       (funcall 'neovm--ti-u-unify '(tvar 1) '(-> (tvar 1) (int)) nil)
       ;; Chain: unify t1=t2, then t2=int -> both resolve to int
       (let* ((r1 (funcall 'neovm--ti-u-unify '(tvar 1) '(tvar 2) nil))
              (r2 (funcall 'neovm--ti-u-unify '(tvar 2) '(int) (cdr r1))))
         (list r2
               (funcall 'neovm--ti-u-apply (cdr r2) '(tvar 1))
               (funcall 'neovm--ti-u-apply (cdr r2) '(tvar 2))))
       ;; Unify list types
       (funcall 'neovm--ti-u-unify '(list-of (tvar 3)) '(list-of (int)) nil)
       ;; Unify pair types
       (funcall 'neovm--ti-u-unify '(pair (tvar 4) (tvar 5))
                '(pair (int) (bool)) nil))
    (fmakunbound 'neovm--ti-u-apply)
    (fmakunbound 'neovm--ti-u-occurs)
    (fmakunbound 'neovm--ti-u-compose)
    (fmakunbound 'neovm--ti-u-unify)))
"#;
    let expect = expect_test::expect![[
        r#""OK ((ok (1 int)) (ok) (error . \"cannot unify (int) with (bool)\") (ok (2 bool) (1 int)) (ok (2 int) (1 bool)) (error . \"occurs-check\") ((ok (2 int) (1 tvar 2)) (int) (int)) (ok (3 int)) (ok (5 bool) (4 int)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Type checking: lambda, application, let expressions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_type_infer_type_checking_expressions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  ;; Minimal type inference engine
  ;; Expression representation:
  ;;   (lit-int N)       -> (int)
  ;;   (lit-bool B)      -> (bool)
  ;;   (var NAME)        -> env lookup
  ;;   (lam PARAM BODY)  -> (-> param-type body-type)
  ;;   (app FN ARG)      -> apply function
  ;;   (let-expr NAME VAL BODY)  -> let binding

  (defvar neovm--ti-tc-counter 0)
  (fset 'neovm--ti-tc-fresh
    (lambda ()
      (setq neovm--ti-tc-counter (1+ neovm--ti-tc-counter))
      (list 'tvar neovm--ti-tc-counter)))

  (fset 'neovm--ti-tc-apply
    (lambda (subst type)
      (cond
       ((null type) nil)
       ((eq (car type) 'tvar)
        (let ((b (assq (cadr type) subst)))
          (if b (funcall 'neovm--ti-tc-apply subst (cdr b)) type)))
       ((eq (car type) '->)
        (list '-> (funcall 'neovm--ti-tc-apply subst (nth 1 type))
              (funcall 'neovm--ti-tc-apply subst (nth 2 type))))
       (t type))))

  (fset 'neovm--ti-tc-occurs
    (lambda (id type)
      (cond
       ((null type) nil)
       ((eq (car type) 'tvar) (= (cadr type) id))
       ((eq (car type) '->)
        (or (funcall 'neovm--ti-tc-occurs id (nth 1 type))
            (funcall 'neovm--ti-tc-occurs id (nth 2 type))))
       (t nil))))

  (fset 'neovm--ti-tc-unify
    (lambda (t1 t2 subst)
      (let ((t1 (funcall 'neovm--ti-tc-apply subst t1))
            (t2 (funcall 'neovm--ti-tc-apply subst t2)))
        (cond
         ((equal t1 t2) (cons 'ok subst))
         ((and (consp t1) (eq (car t1) 'tvar))
          (if (funcall 'neovm--ti-tc-occurs (cadr t1) t2)
              (cons 'error "occurs")
            (cons 'ok (cons (cons (cadr t1) t2) subst))))
         ((and (consp t2) (eq (car t2) 'tvar))
          (if (funcall 'neovm--ti-tc-occurs (cadr t2) t1)
              (cons 'error "occurs")
            (cons 'ok (cons (cons (cadr t2) t1) subst))))
         ((and (consp t1) (eq (car t1) '->)
               (consp t2) (eq (car t2) '->))
          (let ((r (funcall 'neovm--ti-tc-unify (nth 1 t1) (nth 1 t2) subst)))
            (if (eq (car r) 'error) r
              (funcall 'neovm--ti-tc-unify (nth 2 t1) (nth 2 t2) (cdr r)))))
         (t (cons 'error (format "type-mismatch: %S vs %S" t1 t2)))))))

  ;; Infer type of expression in environment
  ;; Returns (ok type . subst) or (error . msg)
  (fset 'neovm--ti-tc-infer
    (lambda (env expr subst)
      (cond
       ;; Integer literal
       ((eq (car expr) 'lit-int)
        (list 'ok '(int) subst))
       ;; Boolean literal
       ((eq (car expr) 'lit-bool)
        (list 'ok '(bool) subst))
       ;; Variable reference
       ((eq (car expr) 'var)
        (let ((t (cdr (assq (cadr expr) env))))
          (if t (list 'ok t subst)
            (list 'error (format "unbound: %S" (cadr expr))))))
       ;; Lambda: (lam param body)
       ((eq (car expr) 'lam)
        (let* ((param (cadr expr))
               (body (nth 2 expr))
               (param-type (funcall 'neovm--ti-tc-fresh))
               (new-env (cons (cons param param-type) env))
               (body-result (funcall 'neovm--ti-tc-infer new-env body subst)))
          (if (eq (car body-result) 'error)
              body-result
            (let ((body-type (nth 1 body-result))
                  (new-subst (nth 2 body-result)))
              (list 'ok
                    (list '->
                          (funcall 'neovm--ti-tc-apply new-subst param-type)
                          body-type)
                    new-subst)))))
       ;; Application: (app fn arg)
       ((eq (car expr) 'app)
        (let* ((fn-expr (cadr expr))
               (arg-expr (nth 2 expr))
               (fn-result (funcall 'neovm--ti-tc-infer env fn-expr subst)))
          (if (eq (car fn-result) 'error) fn-result
            (let* ((fn-type (nth 1 fn-result))
                   (s1 (nth 2 fn-result))
                   (arg-result (funcall 'neovm--ti-tc-infer env arg-expr s1)))
              (if (eq (car arg-result) 'error) arg-result
                (let* ((arg-type (nth 1 arg-result))
                       (s2 (nth 2 arg-result))
                       (ret-type (funcall 'neovm--ti-tc-fresh))
                       (expected-fn (list '-> arg-type ret-type))
                       (u-result (funcall 'neovm--ti-tc-unify
                                           (funcall 'neovm--ti-tc-apply s2 fn-type)
                                           expected-fn s2)))
                  (if (eq (car u-result) 'error) u-result
                    (list 'ok
                          (funcall 'neovm--ti-tc-apply (cdr u-result) ret-type)
                          (cdr u-result)))))))))
       ;; Let: (let-expr name val body)
       ((eq (car expr) 'let-expr)
        (let* ((name (cadr expr))
               (val-expr (nth 2 expr))
               (body-expr (nth 3 expr))
               (val-result (funcall 'neovm--ti-tc-infer env val-expr subst)))
          (if (eq (car val-result) 'error) val-result
            (let* ((val-type (nth 1 val-result))
                   (s1 (nth 2 val-result))
                   (new-env (cons (cons name val-type) env)))
              (funcall 'neovm--ti-tc-infer new-env body-expr s1)))))
       (t (list 'error (format "unknown expr: %S" expr))))))

  (unwind-protect
      (progn
        (setq neovm--ti-tc-counter 0)
        (let ((env (list (cons 'add '(-> (int) (-> (int) (int))))
                         (cons 'not '(-> (bool) (bool)))
                         (cons 'eq '(-> (int) (-> (int) (bool)))))))
          (list
           ;; Infer literal
           (let ((r (funcall 'neovm--ti-tc-infer env '(lit-int 42) nil)))
             (list (car r) (nth 1 r)))
           ;; Infer variable
           (let ((r (funcall 'neovm--ti-tc-infer env '(var add) nil)))
             (list (car r) (nth 1 r)))
           ;; Infer application: add 1
           (setq neovm--ti-tc-counter 0)
           (let ((r (funcall 'neovm--ti-tc-infer env
                              '(app (var add) (lit-int 1)) nil)))
             (list (car r) (funcall 'neovm--ti-tc-apply (nth 2 r) (nth 1 r))))
           ;; Infer application: add 1 2
           (setq neovm--ti-tc-counter 0)
           (let ((r (funcall 'neovm--ti-tc-infer env
                              '(app (app (var add) (lit-int 1)) (lit-int 2)) nil)))
             (list (car r) (funcall 'neovm--ti-tc-apply (nth 2 r) (nth 1 r))))
           ;; Infer lambda: (lam x (lit-int 0)) -> ? -> int
           (setq neovm--ti-tc-counter 0)
           (let ((r (funcall 'neovm--ti-tc-infer env '(lam x (lit-int 0)) nil)))
             (list (car r) (funcall 'neovm--ti-tc-apply (nth 2 r) (nth 1 r))))
           ;; Infer let: let x = 5 in add x x
           (setq neovm--ti-tc-counter 0)
           (let ((r (funcall 'neovm--ti-tc-infer env
                              '(let-expr x (lit-int 5)
                                         (app (app (var add) (var x)) (var x)))
                              nil)))
             (list (car r) (funcall 'neovm--ti-tc-apply (nth 2 r) (nth 1 r))))
           ;; Type error: not applied to int
           (setq neovm--ti-tc-counter 0)
           (let ((r (funcall 'neovm--ti-tc-infer env
                              '(app (var not) (lit-int 42)) nil)))
             (car r))
           ;; Unbound variable
           (let ((r (funcall 'neovm--ti-tc-infer env '(var unknown) nil)))
             (car r)))))
    (fmakunbound 'neovm--ti-tc-fresh)
    (fmakunbound 'neovm--ti-tc-apply)
    (fmakunbound 'neovm--ti-tc-occurs)
    (fmakunbound 'neovm--ti-tc-unify)
    (fmakunbound 'neovm--ti-tc-infer)
    (makunbound 'neovm--ti-tc-counter)))
"#;
    let expect = expect_test::expect![[r#""ERR (setting-constant t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Constraint generation: collect type constraints from expressions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_type_infer_constraint_generation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (defvar neovm--ti-cg-counter 0)
  (fset 'neovm--ti-cg-fresh
    (lambda ()
      (setq neovm--ti-cg-counter (1+ neovm--ti-cg-counter))
      (list 'tvar neovm--ti-cg-counter)))

  ;; Generate constraints: returns (type . constraints)
  ;; where constraints is a list of (type1 = type2) pairs
  (fset 'neovm--ti-cg-generate
    (lambda (env expr)
      (cond
       ((eq (car expr) 'lit-int)
        (cons '(int) nil))
       ((eq (car expr) 'lit-bool)
        (cons '(bool) nil))
       ((eq (car expr) 'var)
        (let ((t (cdr (assq (cadr expr) env))))
          (cons (or t (list 'error 'unbound (cadr expr))) nil)))
       ((eq (car expr) 'lam)
        (let* ((param (cadr expr))
               (body (nth 2 expr))
               (param-t (funcall 'neovm--ti-cg-fresh))
               (new-env (cons (cons param param-t) env))
               (body-result (funcall 'neovm--ti-cg-generate new-env body))
               (body-t (car body-result))
               (body-constraints (cdr body-result)))
          (cons (list '-> param-t body-t) body-constraints)))
       ((eq (car expr) 'app)
        (let* ((fn-result (funcall 'neovm--ti-cg-generate env (cadr expr)))
               (fn-t (car fn-result))
               (fn-constraints (cdr fn-result))
               (arg-result (funcall 'neovm--ti-cg-generate env (nth 2 expr)))
               (arg-t (car arg-result))
               (arg-constraints (cdr arg-result))
               (ret-t (funcall 'neovm--ti-cg-fresh)))
          (cons ret-t
                (append fn-constraints arg-constraints
                        (list (list fn-t '= (list '-> arg-t ret-t)))))))
       ((eq (car expr) 'let-expr)
        (let* ((name (cadr expr))
               (val-result (funcall 'neovm--ti-cg-generate env (nth 2 expr)))
               (val-t (car val-result))
               (val-constraints (cdr val-result))
               (new-env (cons (cons name val-t) env))
               (body-result (funcall 'neovm--ti-cg-generate new-env (nth 3 expr)))
               (body-t (car body-result))
               (body-constraints (cdr body-result)))
          (cons body-t (append val-constraints body-constraints))))
       (t (cons (list 'error 'unknown) nil)))))

  (unwind-protect
      (progn
        (setq neovm--ti-cg-counter 0)
        (let ((env (list (cons 'add '(-> (int) (-> (int) (int))))
                         (cons 'is-zero '(-> (int) (bool))))))
          (list
           ;; Literal generates no constraints
           (progn (setq neovm--ti-cg-counter 0)
                  (funcall 'neovm--ti-cg-generate env '(lit-int 5)))
           ;; Variable lookup
           (progn (setq neovm--ti-cg-counter 0)
                  (funcall 'neovm--ti-cg-generate env '(var add)))
           ;; Lambda generates param tvar
           (progn (setq neovm--ti-cg-counter 0)
                  (funcall 'neovm--ti-cg-generate env '(lam x (var x))))
           ;; Application generates equality constraint
           (progn (setq neovm--ti-cg-counter 0)
                  (funcall 'neovm--ti-cg-generate env
                            '(app (var is-zero) (lit-int 42))))
           ;; Let expression
           (progn (setq neovm--ti-cg-counter 0)
                  (funcall 'neovm--ti-cg-generate env
                            '(let-expr x (lit-int 10) (app (var is-zero) (var x)))))
           ;; Nested application: add 1 2
           (progn (setq neovm--ti-cg-counter 0)
                  (funcall 'neovm--ti-cg-generate env
                            '(app (app (var add) (lit-int 1)) (lit-int 2)))))))
    (fmakunbound 'neovm--ti-cg-fresh)
    (fmakunbound 'neovm--ti-cg-generate)
    (makunbound 'neovm--ti-cg-counter)))
"#;
    let expect = expect_test::expect![[r#""ERR (setting-constant t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Constraint solving: solve a set of type constraints
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_type_infer_constraint_solving() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (fset 'neovm--ti-cs-apply
    (lambda (subst type)
      (cond
       ((null type) nil)
       ((eq (car type) 'tvar)
        (let ((b (assq (cadr type) subst)))
          (if b (funcall 'neovm--ti-cs-apply subst (cdr b)) type)))
       ((eq (car type) '->)
        (list '-> (funcall 'neovm--ti-cs-apply subst (nth 1 type))
              (funcall 'neovm--ti-cs-apply subst (nth 2 type))))
       ((eq (car type) 'list-of)
        (list 'list-of (funcall 'neovm--ti-cs-apply subst (nth 1 type))))
       (t type))))

  (fset 'neovm--ti-cs-occurs
    (lambda (id type)
      (cond
       ((null type) nil)
       ((eq (car type) 'tvar) (= (cadr type) id))
       ((eq (car type) '->)
        (or (funcall 'neovm--ti-cs-occurs id (nth 1 type))
            (funcall 'neovm--ti-cs-occurs id (nth 2 type))))
       ((eq (car type) 'list-of)
        (funcall 'neovm--ti-cs-occurs id (nth 1 type)))
       (t nil))))

  (fset 'neovm--ti-cs-unify-one
    (lambda (t1 t2 subst)
      (let ((t1 (funcall 'neovm--ti-cs-apply subst t1))
            (t2 (funcall 'neovm--ti-cs-apply subst t2)))
        (cond
         ((equal t1 t2) (cons 'ok subst))
         ((and (consp t1) (eq (car t1) 'tvar))
          (if (funcall 'neovm--ti-cs-occurs (cadr t1) t2)
              (cons 'error "occurs")
            (cons 'ok (cons (cons (cadr t1) t2) subst))))
         ((and (consp t2) (eq (car t2) 'tvar))
          (if (funcall 'neovm--ti-cs-occurs (cadr t2) t1)
              (cons 'error "occurs")
            (cons 'ok (cons (cons (cadr t2) t1) subst))))
         ((and (consp t1) (eq (car t1) '->)
               (consp t2) (eq (car t2) '->))
          (let ((r (funcall 'neovm--ti-cs-unify-one (nth 1 t1) (nth 1 t2) subst)))
            (if (eq (car r) 'error) r
              (funcall 'neovm--ti-cs-unify-one (nth 2 t1) (nth 2 t2) (cdr r)))))
         ((and (consp t1) (eq (car t1) 'list-of)
               (consp t2) (eq (car t2) 'list-of))
          (funcall 'neovm--ti-cs-unify-one (nth 1 t1) (nth 1 t2) subst))
         (t (cons 'error (format "mismatch %S %S" t1 t2)))))))

  ;; Solve a list of (type1 = type2) constraints
  (fset 'neovm--ti-cs-solve
    (lambda (constraints)
      (let ((subst nil)
            (ok t))
        (dolist (c constraints)
          (when ok
            (let ((t1 (nth 0 c))
                  (t2 (nth 2 c)))
              (let ((r (funcall 'neovm--ti-cs-unify-one t1 t2 subst)))
                (if (eq (car r) 'error)
                    (setq ok nil subst (cdr r))
                  (setq subst (cdr r)))))))
        (if ok (cons 'ok subst) (cons 'error subst)))))

  (unwind-protect
      (list
       ;; Simple constraint set: t1 = int, t2 = bool, t3 = (-> t1 t2)
       (let ((r (funcall 'neovm--ti-cs-solve
                          '(((tvar 1) = (int))
                            ((tvar 2) = (bool))
                            ((tvar 3) = (-> (tvar 1) (tvar 2)))))))
         (when (eq (car r) 'ok)
           (list (car r)
                 (funcall 'neovm--ti-cs-apply (cdr r) '(tvar 1))
                 (funcall 'neovm--ti-cs-apply (cdr r) '(tvar 2))
                 (funcall 'neovm--ti-cs-apply (cdr r) '(tvar 3)))))
       ;; Chained constraints: t1 = t2, t2 = t3, t3 = int
       (let ((r (funcall 'neovm--ti-cs-solve
                          '(((tvar 1) = (tvar 2))
                            ((tvar 2) = (tvar 3))
                            ((tvar 3) = (int))))))
         (when (eq (car r) 'ok)
           (list (funcall 'neovm--ti-cs-apply (cdr r) '(tvar 1))
                 (funcall 'neovm--ti-cs-apply (cdr r) '(tvar 2))
                 (funcall 'neovm--ti-cs-apply (cdr r) '(tvar 3)))))
       ;; Contradictory: t1 = int, t1 = bool
       (let ((r (funcall 'neovm--ti-cs-solve
                          '(((tvar 1) = (int))
                            ((tvar 1) = (bool))))))
         (car r))
       ;; Function type constraints
       (let ((r (funcall 'neovm--ti-cs-solve
                          '(((-> (tvar 1) (tvar 2)) = (-> (int) (bool)))))))
         (when (eq (car r) 'ok)
           (list (funcall 'neovm--ti-cs-apply (cdr r) '(tvar 1))
                 (funcall 'neovm--ti-cs-apply (cdr r) '(tvar 2)))))
       ;; List type constraints
       (let ((r (funcall 'neovm--ti-cs-solve
                          '(((list-of (tvar 4)) = (list-of (int)))))))
         (when (eq (car r) 'ok)
           (funcall 'neovm--ti-cs-apply (cdr r) '(tvar 4))))
       ;; Empty constraint set
       (funcall 'neovm--ti-cs-solve nil))
    (fmakunbound 'neovm--ti-cs-apply)
    (fmakunbound 'neovm--ti-cs-occurs)
    (fmakunbound 'neovm--ti-cs-unify-one)
    (fmakunbound 'neovm--ti-cs-solve)))
"#;
    let expect = expect_test::expect![[
        r#""OK ((ok (int) (bool) (-> (int) (bool))) ((int) (int) (int)) error ((int) (bool)) (int) (ok))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Polymorphic types: instantiation, generalization, let-polymorphism
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_type_infer_polymorphism() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (defvar neovm--ti-poly-counter 0)
  (fset 'neovm--ti-poly-fresh
    (lambda ()
      (setq neovm--ti-poly-counter (1+ neovm--ti-poly-counter))
      (list 'tvar neovm--ti-poly-counter)))

  (fset 'neovm--ti-poly-apply
    (lambda (subst type)
      (cond
       ((null type) nil)
       ((eq (car type) 'tvar)
        (let ((b (assq (cadr type) subst)))
          (if b (funcall 'neovm--ti-poly-apply subst (cdr b)) type)))
       ((eq (car type) '->)
        (list '-> (funcall 'neovm--ti-poly-apply subst (nth 1 type))
              (funcall 'neovm--ti-poly-apply subst (nth 2 type))))
       ((eq (car type) 'list-of)
        (list 'list-of (funcall 'neovm--ti-poly-apply subst (nth 1 type))))
       ((eq (car type) 'forall) type)
       (t type))))

  ;; Collect all tvar ids in a type
  (fset 'neovm--ti-poly-tvars
    (lambda (type)
      (cond
       ((null type) nil)
       ((eq (car type) 'tvar) (list (cadr type)))
       ((eq (car type) '->)
        (let ((left (funcall 'neovm--ti-poly-tvars (nth 1 type)))
              (right (funcall 'neovm--ti-poly-tvars (nth 2 type))))
          (let ((result left))
            (dolist (v right)
              (unless (memq v result) (setq result (cons v result))))
            result)))
       ((eq (car type) 'list-of)
        (funcall 'neovm--ti-poly-tvars (nth 1 type)))
       (t nil))))

  ;; Collect tvars from environment
  (fset 'neovm--ti-poly-env-tvars
    (lambda (env)
      (let ((result nil))
        (dolist (binding env)
          (dolist (v (funcall 'neovm--ti-poly-tvars (cdr binding)))
            (unless (memq v result) (setq result (cons v result)))))
        result)))

  ;; Generalize: create forall type for tvars not in environment
  (fset 'neovm--ti-poly-generalize
    (lambda (env type)
      (let ((env-tvars (funcall 'neovm--ti-poly-env-tvars env))
            (type-tvars (funcall 'neovm--ti-poly-tvars type))
            (free nil))
        (dolist (v type-tvars)
          (unless (memq v env-tvars)
            (setq free (cons v free))))
        (if free
            (list 'forall (nreverse free) type)
          type))))

  ;; Instantiate: replace bound tvars with fresh ones
  (fset 'neovm--ti-poly-instantiate
    (lambda (scheme)
      (if (and (consp scheme) (eq (car scheme) 'forall))
          (let ((bound-vars (nth 1 scheme))
                (body (nth 2 scheme))
                (mapping nil))
            (dolist (v bound-vars)
              (setq mapping (cons (cons v (funcall 'neovm--ti-poly-fresh)) mapping)))
            (funcall 'neovm--ti-poly-apply mapping body))
        scheme)))

  (unwind-protect
      (progn
        (setq neovm--ti-poly-counter 0)
        (list
         ;; Generalize: id function type (-> t1 t1) with empty env
         (let ((id-type '(-> (tvar 100) (tvar 100))))
           (funcall 'neovm--ti-poly-generalize nil id-type))
         ;; Generalize with env containing t100: should not generalize t100
         (let ((env (list (cons 'x '(tvar 100))))
               (fn-type '(-> (tvar 100) (tvar 101))))
           (funcall 'neovm--ti-poly-generalize env fn-type))
         ;; Instantiate: (forall (100) (-> (tvar 100) (tvar 100)))
         ;; should produce (-> (tvar N) (tvar N)) with fresh N
         (progn
           (setq neovm--ti-poly-counter 0)
           (let ((scheme '(forall (100) (-> (tvar 100) (tvar 100)))))
             (funcall 'neovm--ti-poly-instantiate scheme)))
         ;; Instantiate twice: should get different tvars each time
         (progn
           (setq neovm--ti-poly-counter 10)
           (let ((scheme '(forall (1 2) (-> (tvar 1) (tvar 2)))))
             (list (funcall 'neovm--ti-poly-instantiate scheme)
                   (funcall 'neovm--ti-poly-instantiate scheme))))
         ;; Non-polymorphic: instantiate returns unchanged
         (funcall 'neovm--ti-poly-instantiate '(int))
         ;; Generalize with no free tvars
         (funcall 'neovm--ti-poly-generalize nil '(-> (int) (bool)))
         ;; tvars extraction
         (funcall 'neovm--ti-poly-tvars '(-> (tvar 1) (-> (tvar 2) (tvar 1))))
         ;; env tvars
         (funcall 'neovm--ti-poly-env-tvars
                  (list (cons 'x '(tvar 5)) (cons 'y '(-> (tvar 6) (int)))))))
    (fmakunbound 'neovm--ti-poly-fresh)
    (fmakunbound 'neovm--ti-poly-apply)
    (fmakunbound 'neovm--ti-poly-tvars)
    (fmakunbound 'neovm--ti-poly-env-tvars)
    (fmakunbound 'neovm--ti-poly-generalize)
    (fmakunbound 'neovm--ti-poly-instantiate)
    (makunbound 'neovm--ti-poly-counter)))
"#;
    let expect = expect_test::expect![[
        r#""OK ((forall (100) (-> (tvar 100) (tvar 100))) (forall (101) (-> (tvar 100) (tvar 101))) (-> (tvar 1) (tvar 1)) ((-> (tvar 11) (tvar 12)) (-> (tvar 13) (tvar 14))) (int) (-> (int) (bool)) (2 1) (6 5))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
