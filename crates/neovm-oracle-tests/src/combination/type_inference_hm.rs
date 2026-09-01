//! Oracle parity tests for Hindley-Milner type inference in Elisp:
//! type representation (type variables, function types, basic types),
//! unification algorithm with occurs check, type environment,
//! inference for lambda/application/let with let-polymorphism,
//! and polymorphic type instantiation and generalization.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

// ---------------------------------------------------------------------------
// Type representation and basic operations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_hm_type_representation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  ;; Type representation:
  ;;   (tcon name)             -- type constructor (int, bool, string)
  ;;   (tvar id)               -- type variable
  ;;   (tfun arg ret)          -- function type
  ;;   (tlist elem)            -- list type
  ;;   (tpair fst snd)         -- pair type
  ;;   (scheme vars body)      -- polymorphic type scheme

  ;; Predicates
  (fset 'neovm--hm-tcon-p  (lambda (ty) (and (consp ty) (eq (car ty) 'tcon))))
  (fset 'neovm--hm-tvar-p  (lambda (ty) (and (consp ty) (eq (car ty) 'tvar))))
  (fset 'neovm--hm-tfun-p  (lambda (ty) (and (consp ty) (eq (car ty) 'tfun))))
  (fset 'neovm--hm-tlist-p (lambda (ty) (and (consp ty) (eq (car ty) 'tlist))))
  (fset 'neovm--hm-tpair-p (lambda (ty) (and (consp ty) (eq (car ty) 'tpair))))
  (fset 'neovm--hm-scheme-p (lambda (ty) (and (consp ty) (eq (car ty) 'scheme))))

  ;; Constructors
  (fset 'neovm--hm-tcon  (lambda (name) (list 'tcon name)))
  (fset 'neovm--hm-tvar  (lambda (id) (list 'tvar id)))
  (fset 'neovm--hm-tfun  (lambda (arg ret) (list 'tfun arg ret)))
  (fset 'neovm--hm-tlist (lambda (elem) (list 'tlist elem)))
  (fset 'neovm--hm-tpair (lambda (fst snd) (list 'tpair fst snd)))

  ;; Collect free type variables
  (fset 'neovm--hm-ftv
    (lambda (ty)
      (cond
       ((null ty) nil)
       ((funcall 'neovm--hm-tvar-p ty) (list (nth 1 ty)))
       ((funcall 'neovm--hm-tcon-p ty) nil)
       ((funcall 'neovm--hm-tfun-p ty)
        (let ((a (funcall 'neovm--hm-ftv (nth 1 ty)))
              (b (funcall 'neovm--hm-ftv (nth 2 ty)))
              (result nil))
          (dolist (v (append a b))
            (unless (memq v result) (push v result)))
          (nreverse result)))
       ((funcall 'neovm--hm-tlist-p ty)
        (funcall 'neovm--hm-ftv (nth 1 ty)))
       ((funcall 'neovm--hm-tpair-p ty)
        (let ((a (funcall 'neovm--hm-ftv (nth 1 ty)))
              (b (funcall 'neovm--hm-ftv (nth 2 ty)))
              (result nil))
          (dolist (v (append a b))
            (unless (memq v result) (push v result)))
          (nreverse result)))
       ((funcall 'neovm--hm-scheme-p ty)
        (let ((bound (nth 1 ty))
              (body-ftv (funcall 'neovm--hm-ftv (nth 2 ty)))
              (result nil))
          (dolist (v body-ftv)
            (unless (memq v bound) (push v result)))
          (nreverse result)))
       (t nil))))

  (unwind-protect
      (list
       ;; Type construction
       (funcall 'neovm--hm-tcon 'int)
       (funcall 'neovm--hm-tvar 1)
       (funcall 'neovm--hm-tfun '(tcon int) '(tcon bool))
       (funcall 'neovm--hm-tlist '(tcon int))
       (funcall 'neovm--hm-tpair '(tcon int) '(tcon bool))
       ;; Predicates
       (funcall 'neovm--hm-tcon-p '(tcon int))
       (funcall 'neovm--hm-tvar-p '(tvar 1))
       (funcall 'neovm--hm-tfun-p '(tfun (tcon int) (tcon bool)))
       (funcall 'neovm--hm-tcon-p '(tvar 1))
       (funcall 'neovm--hm-tvar-p '(tcon int))
       ;; Free type variables
       (funcall 'neovm--hm-ftv '(tvar 1))
       (funcall 'neovm--hm-ftv '(tcon int))
       (funcall 'neovm--hm-ftv '(tfun (tvar 1) (tvar 2)))
       (funcall 'neovm--hm-ftv '(tfun (tvar 1) (tvar 1)))
       (funcall 'neovm--hm-ftv '(tlist (tvar 3)))
       (funcall 'neovm--hm-ftv '(tpair (tvar 4) (tvar 5)))
       ;; Scheme: forall {1}. (1 -> 1) has no free vars for 1
       (funcall 'neovm--hm-ftv '(scheme (1) (tfun (tvar 1) (tvar 1))))
       ;; Scheme: forall {1}. (1 -> 2) has free var 2
       (funcall 'neovm--hm-ftv '(scheme (1) (tfun (tvar 1) (tvar 2)))))
    (fmakunbound 'neovm--hm-tcon-p)
    (fmakunbound 'neovm--hm-tvar-p)
    (fmakunbound 'neovm--hm-tfun-p)
    (fmakunbound 'neovm--hm-tlist-p)
    (fmakunbound 'neovm--hm-tpair-p)
    (fmakunbound 'neovm--hm-scheme-p)
    (fmakunbound 'neovm--hm-tcon)
    (fmakunbound 'neovm--hm-tvar)
    (fmakunbound 'neovm--hm-tfun)
    (fmakunbound 'neovm--hm-tlist)
    (fmakunbound 'neovm--hm-tpair)
    (fmakunbound 'neovm--hm-ftv)))
"#;
    let expect = expect_test::expect![[
        r#""OK ((tcon int) (tvar 1) (tfun (tcon int) (tcon bool)) (tlist (tcon int)) (tpair (tcon int) (tcon bool)) t t t nil nil (1) nil (1 2) (1) (3) (4 5) nil (2))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Substitution: apply and compose
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_hm_substitution() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  ;; Substitution is an alist: ((var-id . type) ...)
  (fset 'neovm--hm-s-apply
    (lambda (subst ty)
      (cond
       ((null ty) nil)
       ((and (consp ty) (eq (car ty) 'tvar))
        (let ((b (assq (nth 1 ty) subst)))
          (if b (funcall 'neovm--hm-s-apply subst (cdr b)) ty)))
       ((and (consp ty) (eq (car ty) 'tcon)) ty)
       ((and (consp ty) (eq (car ty) 'tfun))
        (list 'tfun
              (funcall 'neovm--hm-s-apply subst (nth 1 ty))
              (funcall 'neovm--hm-s-apply subst (nth 2 ty))))
       ((and (consp ty) (eq (car ty) 'tlist))
        (list 'tlist (funcall 'neovm--hm-s-apply subst (nth 1 ty))))
       ((and (consp ty) (eq (car ty) 'tpair))
        (list 'tpair
              (funcall 'neovm--hm-s-apply subst (nth 1 ty))
              (funcall 'neovm--hm-s-apply subst (nth 2 ty))))
       (t ty))))

  ;; Compose substitutions: s1 after s2
  (fset 'neovm--hm-s-compose
    (lambda (s1 s2)
      (let ((result nil))
        ;; Apply s1 to all bindings in s2
        (dolist (b s2)
          (push (cons (car b) (funcall 'neovm--hm-s-apply s1 (cdr b))) result))
        ;; Add s1 bindings not already in result
        (dolist (b s1)
          (unless (assq (car b) result)
            (push b result)))
        result)))

  (unwind-protect
      (list
       ;; Apply empty subst
       (funcall 'neovm--hm-s-apply nil '(tvar 1))
       ;; Apply: tvar 1 -> int
       (funcall 'neovm--hm-s-apply '((1 . (tcon int))) '(tvar 1))
       ;; Apply to concrete: unchanged
       (funcall 'neovm--hm-s-apply '((1 . (tcon int))) '(tcon bool))
       ;; Apply to function type
       (funcall 'neovm--hm-s-apply '((1 . (tcon int)) (2 . (tcon bool)))
                '(tfun (tvar 1) (tvar 2)))
       ;; Chain: tvar 3 -> tvar 1 -> int
       (funcall 'neovm--hm-s-apply '((3 . (tvar 1)) (1 . (tcon int))) '(tvar 3))
       ;; Apply to list type
       (funcall 'neovm--hm-s-apply '((1 . (tcon string))) '(tlist (tvar 1)))
       ;; Apply to pair type
       (funcall 'neovm--hm-s-apply '((1 . (tcon int)) (2 . (tcon bool)))
                '(tpair (tvar 1) (tvar 2)))
       ;; Compose: {1->int} after {2->tvar 1} = {2->int, 1->int}
       (let* ((s1 '((1 . (tcon int))))
              (s2 '((2 . (tvar 1))))
              (composed (funcall 'neovm--hm-s-compose s1 s2)))
         (list (funcall 'neovm--hm-s-apply composed '(tvar 1))
               (funcall 'neovm--hm-s-apply composed '(tvar 2))))
       ;; Compose with overlapping keys: s1 wins for direct lookup
       (let* ((s1 '((1 . (tcon int))))
              (s2 '((1 . (tcon bool))))
              (composed (funcall 'neovm--hm-s-compose s1 s2)))
         (funcall 'neovm--hm-s-apply composed '(tvar 1))))
    (fmakunbound 'neovm--hm-s-apply)
    (fmakunbound 'neovm--hm-s-compose)))
"#;
    let expect = expect_test::expect![[
        r#""OK ((tvar 1) (tcon int) (tcon bool) (tfun (tcon int) (tcon bool)) (tcon int) (tlist (tcon string)) (tpair (tcon int) (tcon bool)) ((tcon int) (tcon int)) (tcon bool))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Unification with occurs check
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_hm_unification() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (fset 'neovm--hm-u-apply
    (lambda (subst ty)
      (cond
       ((null ty) nil)
       ((and (consp ty) (eq (car ty) 'tvar))
        (let ((b (assq (nth 1 ty) subst)))
          (if b (funcall 'neovm--hm-u-apply subst (cdr b)) ty)))
       ((and (consp ty) (eq (car ty) 'tcon)) ty)
       ((and (consp ty) (eq (car ty) 'tfun))
        (list 'tfun (funcall 'neovm--hm-u-apply subst (nth 1 ty))
              (funcall 'neovm--hm-u-apply subst (nth 2 ty))))
       ((and (consp ty) (eq (car ty) 'tlist))
        (list 'tlist (funcall 'neovm--hm-u-apply subst (nth 1 ty))))
       ((and (consp ty) (eq (car ty) 'tpair))
        (list 'tpair (funcall 'neovm--hm-u-apply subst (nth 1 ty))
              (funcall 'neovm--hm-u-apply subst (nth 2 ty))))
       (t ty))))

  ;; Occurs check
  (fset 'neovm--hm-u-occurs
    (lambda (var-id ty subst)
      (let ((ty (funcall 'neovm--hm-u-apply subst ty)))
        (cond
         ((and (consp ty) (eq (car ty) 'tvar)) (= var-id (nth 1 ty)))
         ((and (consp ty) (eq (car ty) 'tcon)) nil)
         ((and (consp ty) (eq (car ty) 'tfun))
          (or (funcall 'neovm--hm-u-occurs var-id (nth 1 ty) subst)
              (funcall 'neovm--hm-u-occurs var-id (nth 2 ty) subst)))
         ((and (consp ty) (eq (car ty) 'tlist))
          (funcall 'neovm--hm-u-occurs var-id (nth 1 ty) subst))
         ((and (consp ty) (eq (car ty) 'tpair))
          (or (funcall 'neovm--hm-u-occurs var-id (nth 1 ty) subst)
              (funcall 'neovm--hm-u-occurs var-id (nth 2 ty) subst)))
         (t nil)))))

  ;; Bind variable, checking occurs
  (fset 'neovm--hm-u-bind
    (lambda (var-id ty subst)
      (if (and (consp ty) (eq (car ty) 'tvar) (= (nth 1 ty) var-id))
          (cons 'ok subst)
        (if (funcall 'neovm--hm-u-occurs var-id ty subst)
            (cons 'err (format "occurs: %d in %S" var-id ty))
          (cons 'ok (cons (cons var-id ty) subst))))))

  ;; Unify two types
  (fset 'neovm--hm-u-unify
    (lambda (t1 t2 subst)
      (let ((t1 (funcall 'neovm--hm-u-apply subst t1))
            (t2 (funcall 'neovm--hm-u-apply subst t2)))
        (cond
         ((equal t1 t2) (cons 'ok subst))
         ((and (consp t1) (eq (car t1) 'tvar))
          (funcall 'neovm--hm-u-bind (nth 1 t1) t2 subst))
         ((and (consp t2) (eq (car t2) 'tvar))
          (funcall 'neovm--hm-u-bind (nth 1 t2) t1 subst))
         ((and (consp t1) (eq (car t1) 'tfun)
               (consp t2) (eq (car t2) 'tfun))
          (let ((r1 (funcall 'neovm--hm-u-unify (nth 1 t1) (nth 1 t2) subst)))
            (if (eq (car r1) 'err) r1
              (funcall 'neovm--hm-u-unify (nth 2 t1) (nth 2 t2) (cdr r1)))))
         ((and (consp t1) (eq (car t1) 'tlist)
               (consp t2) (eq (car t2) 'tlist))
          (funcall 'neovm--hm-u-unify (nth 1 t1) (nth 1 t2) subst))
         ((and (consp t1) (eq (car t1) 'tpair)
               (consp t2) (eq (car t2) 'tpair))
          (let ((r1 (funcall 'neovm--hm-u-unify (nth 1 t1) (nth 1 t2) subst)))
            (if (eq (car r1) 'err) r1
              (funcall 'neovm--hm-u-unify (nth 2 t1) (nth 2 t2) (cdr r1)))))
         (t (cons 'err (format "unify: %S vs %S" t1 t2)))))))

  (unwind-protect
      (list
       ;; Unify tvar with concrete
       (funcall 'neovm--hm-u-unify '(tvar 1) '(tcon int) nil)
       ;; Unify two concrete (same)
       (funcall 'neovm--hm-u-unify '(tcon int) '(tcon int) nil)
       ;; Unify two concrete (different) -> error
       (car (funcall 'neovm--hm-u-unify '(tcon int) '(tcon bool) nil))
       ;; Unify function types
       (let ((r (funcall 'neovm--hm-u-unify
                          '(tfun (tvar 1) (tvar 2))
                          '(tfun (tcon int) (tcon bool)) nil)))
         (list (car r)
               (funcall 'neovm--hm-u-apply (cdr r) '(tvar 1))
               (funcall 'neovm--hm-u-apply (cdr r) '(tvar 2))))
       ;; Occurs check: tvar 1 = tfun (tvar 1) (tcon int) -> error
       (car (funcall 'neovm--hm-u-unify '(tvar 1) '(tfun (tvar 1) (tcon int)) nil))
       ;; Chain: t1 = t2, t2 = int
       (let* ((r1 (funcall 'neovm--hm-u-unify '(tvar 1) '(tvar 2) nil))
              (r2 (funcall 'neovm--hm-u-unify '(tvar 2) '(tcon int) (cdr r1))))
         (list (car r2)
               (funcall 'neovm--hm-u-apply (cdr r2) '(tvar 1))
               (funcall 'neovm--hm-u-apply (cdr r2) '(tvar 2))))
       ;; Unify list types
       (let ((r (funcall 'neovm--hm-u-unify '(tlist (tvar 3)) '(tlist (tcon int)) nil)))
         (list (car r) (funcall 'neovm--hm-u-apply (cdr r) '(tvar 3))))
       ;; Unify pair types
       (let ((r (funcall 'neovm--hm-u-unify
                          '(tpair (tvar 4) (tvar 5))
                          '(tpair (tcon int) (tcon bool)) nil)))
         (list (car r)
               (funcall 'neovm--hm-u-apply (cdr r) '(tvar 4))
               (funcall 'neovm--hm-u-apply (cdr r) '(tvar 5))))
       ;; Unify complex nested: (t1 -> list t2) = (int -> list bool)
       (let ((r (funcall 'neovm--hm-u-unify
                          '(tfun (tvar 1) (tlist (tvar 2)))
                          '(tfun (tcon int) (tlist (tcon bool))) nil)))
         (list (car r)
               (funcall 'neovm--hm-u-apply (cdr r) '(tvar 1))
               (funcall 'neovm--hm-u-apply (cdr r) '(tvar 2)))))
    (fmakunbound 'neovm--hm-u-apply)
    (fmakunbound 'neovm--hm-u-occurs)
    (fmakunbound 'neovm--hm-u-bind)
    (fmakunbound 'neovm--hm-u-unify)))
"#;
    let expect = expect_test::expect![[
        r#""OK ((ok (1 tcon int)) (ok) err (ok (tcon int) (tcon bool)) err (ok (tcon int) (tcon int)) (ok (tcon int)) (ok (tcon int) (tcon bool)) (ok (tcon int) (tcon bool)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Type environment and generalization/instantiation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_hm_generalization_instantiation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (defvar neovm--hm-gi-counter 0)
  (fset 'neovm--hm-gi-fresh
    (lambda ()
      (setq neovm--hm-gi-counter (1+ neovm--hm-gi-counter))
      (list 'tvar neovm--hm-gi-counter)))

  (fset 'neovm--hm-gi-apply
    (lambda (subst ty)
      (cond
       ((null ty) nil)
       ((and (consp ty) (eq (car ty) 'tvar))
        (let ((b (assq (nth 1 ty) subst)))
          (if b (funcall 'neovm--hm-gi-apply subst (cdr b)) ty)))
       ((and (consp ty) (eq (car ty) 'tcon)) ty)
       ((and (consp ty) (eq (car ty) 'tfun))
        (list 'tfun (funcall 'neovm--hm-gi-apply subst (nth 1 ty))
              (funcall 'neovm--hm-gi-apply subst (nth 2 ty))))
       ((and (consp ty) (eq (car ty) 'tlist))
        (list 'tlist (funcall 'neovm--hm-gi-apply subst (nth 1 ty))))
       ((and (consp ty) (eq (car ty) 'tpair))
        (list 'tpair (funcall 'neovm--hm-gi-apply subst (nth 1 ty))
              (funcall 'neovm--hm-gi-apply subst (nth 2 ty))))
       (t ty))))

  ;; Collect FTVs
  (fset 'neovm--hm-gi-ftv
    (lambda (ty)
      (cond
       ((null ty) nil)
       ((and (consp ty) (eq (car ty) 'tvar)) (list (nth 1 ty)))
       ((and (consp ty) (eq (car ty) 'tcon)) nil)
       ((and (consp ty) (eq (car ty) 'tfun))
        (let ((r nil))
          (dolist (v (append (funcall 'neovm--hm-gi-ftv (nth 1 ty))
                             (funcall 'neovm--hm-gi-ftv (nth 2 ty))))
            (unless (memq v r) (push v r)))
          (nreverse r)))
       ((and (consp ty) (eq (car ty) 'tlist))
        (funcall 'neovm--hm-gi-ftv (nth 1 ty)))
       ((and (consp ty) (eq (car ty) 'tpair))
        (let ((r nil))
          (dolist (v (append (funcall 'neovm--hm-gi-ftv (nth 1 ty))
                             (funcall 'neovm--hm-gi-ftv (nth 2 ty))))
            (unless (memq v r) (push v r)))
          (nreverse r)))
       (t nil))))

  ;; FTVs of environment
  (fset 'neovm--hm-gi-env-ftv
    (lambda (env)
      (let ((result nil))
        (dolist (binding env)
          (let ((ty (cdr binding)))
            ;; For schemes, collect free vars only
            (let ((vars (if (and (consp ty) (eq (car ty) 'scheme))
                            (let ((bound (nth 1 ty))
                                  (body-fv (funcall 'neovm--hm-gi-ftv (nth 2 ty)))
                                  (r nil))
                              (dolist (v body-fv) (unless (memq v bound) (push v r)))
                              (nreverse r))
                          (funcall 'neovm--hm-gi-ftv ty))))
              (dolist (v vars)
                (unless (memq v result) (push v result))))))
        (nreverse result))))

  ;; Generalize: abstract over tvars not free in env
  (fset 'neovm--hm-gi-generalize
    (lambda (env ty)
      (let ((env-fv (funcall 'neovm--hm-gi-env-ftv env))
            (ty-fv (funcall 'neovm--hm-gi-ftv ty))
            (free nil))
        (dolist (v ty-fv)
          (unless (memq v env-fv) (push v free)))
        (if free
            (list 'scheme (nreverse free) ty)
          ty))))

  ;; Instantiate: replace bound vars with fresh tvars
  (fset 'neovm--hm-gi-instantiate
    (lambda (scheme)
      (if (and (consp scheme) (eq (car scheme) 'scheme))
          (let ((bound (nth 1 scheme))
                (body (nth 2 scheme))
                (mapping nil))
            (dolist (v bound)
              (push (cons v (funcall 'neovm--hm-gi-fresh)) mapping))
            (funcall 'neovm--hm-gi-apply mapping body))
        scheme)))

  (unwind-protect
      (progn
        (setq neovm--hm-gi-counter 0)
        (list
         ;; Generalize t1->t1 in empty env: forall {t1}. t1->t1
         (funcall 'neovm--hm-gi-generalize nil '(tfun (tvar 100) (tvar 100)))
         ;; Generalize with env containing t100: t100 not generalized
         (funcall 'neovm--hm-gi-generalize
                  '((x . (tvar 100)))
                  '(tfun (tvar 100) (tvar 101)))
         ;; Generalize concrete type: no change
         (funcall 'neovm--hm-gi-generalize nil '(tfun (tcon int) (tcon bool)))
         ;; Instantiate forall {100}. t100->t100
         (progn
           (setq neovm--hm-gi-counter 0)
           (funcall 'neovm--hm-gi-instantiate
                    '(scheme (100) (tfun (tvar 100) (tvar 100)))))
         ;; Instantiate twice produces different tvars
         (progn
           (setq neovm--hm-gi-counter 10)
           (list (funcall 'neovm--hm-gi-instantiate
                          '(scheme (1 2) (tfun (tvar 1) (tvar 2))))
                 (funcall 'neovm--hm-gi-instantiate
                          '(scheme (1 2) (tfun (tvar 1) (tvar 2))))))
         ;; Instantiate non-scheme: unchanged
         (funcall 'neovm--hm-gi-instantiate '(tcon int))
         ;; Env FTVs with scheme
         (funcall 'neovm--hm-gi-env-ftv
                  (list (cons 'f '(scheme (1) (tfun (tvar 1) (tvar 2))))
                        (cons 'x '(tvar 3))))))
    (fmakunbound 'neovm--hm-gi-fresh)
    (fmakunbound 'neovm--hm-gi-apply)
    (fmakunbound 'neovm--hm-gi-ftv)
    (fmakunbound 'neovm--hm-gi-env-ftv)
    (fmakunbound 'neovm--hm-gi-generalize)
    (fmakunbound 'neovm--hm-gi-instantiate)
    (makunbound 'neovm--hm-gi-counter)))
"#;
    let expect = expect_test::expect![[
        r#""OK ((scheme (100) (tfun (tvar 100) (tvar 100))) (scheme (101) (tfun (tvar 100) (tvar 101))) (tfun (tcon int) (tcon bool)) (tfun (tvar 1) (tvar 1)) ((tfun (tvar 11) (tvar 12)) (tfun (tvar 13) (tvar 14))) (tcon int) (2 3))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Type inference for lambda, application, let
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_hm_infer_expressions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  ;; Full W-algorithm (Algorithm W for Hindley-Milner)
  ;; Expressions:
  ;;   (evar name)
  ;;   (elit type-name value)  -- literal with known type
  ;;   (elam param body)
  ;;   (eapp fn arg)
  ;;   (elet name val body)    -- let-polymorphism

  (defvar neovm--hm-w-counter 0)
  (fset 'neovm--hm-w-fresh
    (lambda ()
      (setq neovm--hm-w-counter (1+ neovm--hm-w-counter))
      (list 'tvar neovm--hm-w-counter)))

  (fset 'neovm--hm-w-apply
    (lambda (s ty)
      (cond
       ((null ty) nil)
       ((and (consp ty) (eq (car ty) 'tvar))
        (let ((b (assq (nth 1 ty) s)))
          (if b (funcall 'neovm--hm-w-apply s (cdr b)) ty)))
       ((and (consp ty) (eq (car ty) 'tcon)) ty)
       ((and (consp ty) (eq (car ty) 'tfun))
        (list 'tfun (funcall 'neovm--hm-w-apply s (nth 1 ty))
              (funcall 'neovm--hm-w-apply s (nth 2 ty))))
       ((and (consp ty) (eq (car ty) 'tlist))
        (list 'tlist (funcall 'neovm--hm-w-apply s (nth 1 ty))))
       (t ty))))

  (fset 'neovm--hm-w-occurs
    (lambda (id ty s)
      (let ((ty (funcall 'neovm--hm-w-apply s ty)))
        (cond
         ((and (consp ty) (eq (car ty) 'tvar)) (= id (nth 1 ty)))
         ((and (consp ty) (eq (car ty) 'tfun))
          (or (funcall 'neovm--hm-w-occurs id (nth 1 ty) s)
              (funcall 'neovm--hm-w-occurs id (nth 2 ty) s)))
         ((and (consp ty) (eq (car ty) 'tlist))
          (funcall 'neovm--hm-w-occurs id (nth 1 ty) s))
         (t nil)))))

  (fset 'neovm--hm-w-unify
    (lambda (t1 t2 s)
      (let ((t1 (funcall 'neovm--hm-w-apply s t1))
            (t2 (funcall 'neovm--hm-w-apply s t2)))
        (cond
         ((equal t1 t2) (cons 'ok s))
         ((and (consp t1) (eq (car t1) 'tvar))
          (if (funcall 'neovm--hm-w-occurs (nth 1 t1) t2 s)
              (cons 'err "occurs") (cons 'ok (cons (cons (nth 1 t1) t2) s))))
         ((and (consp t2) (eq (car t2) 'tvar))
          (if (funcall 'neovm--hm-w-occurs (nth 1 t2) t1 s)
              (cons 'err "occurs") (cons 'ok (cons (cons (nth 1 t2) t1) s))))
         ((and (consp t1) (eq (car t1) 'tfun) (consp t2) (eq (car t2) 'tfun))
          (let ((r (funcall 'neovm--hm-w-unify (nth 1 t1) (nth 1 t2) s)))
            (if (eq (car r) 'err) r
              (funcall 'neovm--hm-w-unify (nth 2 t1) (nth 2 t2) (cdr r)))))
         ((and (consp t1) (eq (car t1) 'tlist) (consp t2) (eq (car t2) 'tlist))
          (funcall 'neovm--hm-w-unify (nth 1 t1) (nth 1 t2) s))
         (t (cons 'err (format "mismatch %S %S" t1 t2)))))))

  ;; FTV helpers
  (fset 'neovm--hm-w-ftv
    (lambda (ty)
      (cond
       ((null ty) nil)
       ((and (consp ty) (eq (car ty) 'tvar)) (list (nth 1 ty)))
       ((and (consp ty) (eq (car ty) 'tcon)) nil)
       ((and (consp ty) (eq (car ty) 'tfun))
        (let ((r nil))
          (dolist (v (append (funcall 'neovm--hm-w-ftv (nth 1 ty))
                             (funcall 'neovm--hm-w-ftv (nth 2 ty))))
            (unless (memq v r) (push v r))) (nreverse r)))
       ((and (consp ty) (eq (car ty) 'tlist))
        (funcall 'neovm--hm-w-ftv (nth 1 ty)))
       (t nil))))

  (fset 'neovm--hm-w-env-ftv
    (lambda (env)
      (let ((r nil))
        (dolist (b env)
          (let ((ty (cdr b)))
            (let ((fv (if (and (consp ty) (eq (car ty) 'scheme))
                          (let ((bound (nth 1 ty))
                                (bfv (funcall 'neovm--hm-w-ftv (nth 2 ty)))
                                (res nil))
                            (dolist (v bfv) (unless (memq v bound) (push v res))) res)
                        (funcall 'neovm--hm-w-ftv ty))))
              (dolist (v fv) (unless (memq v r) (push v r)))))) r)))

  (fset 'neovm--hm-w-generalize
    (lambda (env ty)
      (let ((efv (funcall 'neovm--hm-w-env-ftv env))
            (tfv (funcall 'neovm--hm-w-ftv ty))
            (free nil))
        (dolist (v tfv) (unless (memq v efv) (push v free)))
        (if free (list 'scheme (nreverse free) ty) ty))))

  (fset 'neovm--hm-w-instantiate
    (lambda (scheme)
      (if (and (consp scheme) (eq (car scheme) 'scheme))
          (let ((mapping nil))
            (dolist (v (nth 1 scheme))
              (push (cons v (funcall 'neovm--hm-w-fresh)) mapping))
            (funcall 'neovm--hm-w-apply mapping (nth 2 scheme)))
        scheme)))

  ;; Apply subst to all env bindings
  (fset 'neovm--hm-w-apply-env
    (lambda (s env)
      (mapcar (lambda (b)
                (cons (car b)
                      (if (and (consp (cdr b)) (eq (car (cdr b)) 'scheme))
                          (cdr b)  ;; don't apply subst to schemes
                        (funcall 'neovm--hm-w-apply s (cdr b)))))
              env)))

  ;; Infer: returns (ok type subst) or (err msg)
  (fset 'neovm--hm-w-infer
    (lambda (env expr s)
      (cond
       ;; Literal
       ((eq (car expr) 'elit)
        (list 'ok (list 'tcon (nth 1 expr)) s))
       ;; Variable
       ((eq (car expr) 'evar)
        (let ((b (assq (nth 1 expr) env)))
          (if b
              (list 'ok (funcall 'neovm--hm-w-instantiate (cdr b)) s)
            (list 'err (format "unbound: %S" (nth 1 expr))))))
       ;; Lambda
       ((eq (car expr) 'elam)
        (let* ((param (nth 1 expr))
               (body (nth 2 expr))
               (tv (funcall 'neovm--hm-w-fresh))
               (new-env (cons (cons param tv) env))
               (r (funcall 'neovm--hm-w-infer new-env body s)))
          (if (eq (car r) 'err) r
            (let ((body-ty (nth 1 r)) (s2 (nth 2 r)))
              (list 'ok
                    (list 'tfun (funcall 'neovm--hm-w-apply s2 tv) body-ty)
                    s2)))))
       ;; Application
       ((eq (car expr) 'eapp)
        (let* ((fn-r (funcall 'neovm--hm-w-infer env (nth 1 expr) s)))
          (if (eq (car fn-r) 'err) fn-r
            (let* ((fn-ty (nth 1 fn-r)) (s1 (nth 2 fn-r))
                   (arg-r (funcall 'neovm--hm-w-infer
                                    (funcall 'neovm--hm-w-apply-env s1 env)
                                    (nth 2 expr) s1)))
              (if (eq (car arg-r) 'err) arg-r
                (let* ((arg-ty (nth 1 arg-r)) (s2 (nth 2 arg-r))
                       (ret-tv (funcall 'neovm--hm-w-fresh))
                       (u (funcall 'neovm--hm-w-unify
                                    (funcall 'neovm--hm-w-apply s2 fn-ty)
                                    (list 'tfun arg-ty ret-tv)
                                    s2)))
                  (if (eq (car u) 'err) u
                    (list 'ok (funcall 'neovm--hm-w-apply (cdr u) ret-tv) (cdr u)))))))))
       ;; Let (with polymorphism)
       ((eq (car expr) 'elet)
        (let* ((name (nth 1 expr))
               (val-r (funcall 'neovm--hm-w-infer env (nth 2 expr) s)))
          (if (eq (car val-r) 'err) val-r
            (let* ((val-ty (nth 1 val-r)) (s1 (nth 2 val-r))
                   (applied-env (funcall 'neovm--hm-w-apply-env s1 env))
                   (gen-ty (funcall 'neovm--hm-w-generalize applied-env
                                     (funcall 'neovm--hm-w-apply s1 val-ty)))
                   (new-env (cons (cons name gen-ty) applied-env)))
              (funcall 'neovm--hm-w-infer new-env (nth 3 expr) s1)))))
       (t (list 'err (format "unknown: %S" (car expr)))))))

  ;; Helper: infer and return resolved type
  (fset 'neovm--hm-w-infer-type
    (lambda (env expr)
      (setq neovm--hm-w-counter 0)
      (let ((r (funcall 'neovm--hm-w-infer env expr nil)))
        (if (eq (car r) 'err) (list 'err (nth 1 r))
          (list 'ok (funcall 'neovm--hm-w-apply (nth 2 r) (nth 1 r)))))))

  (unwind-protect
      (let ((base-env (list (cons 'add '(tfun (tcon int) (tfun (tcon int) (tcon int))))
                            (cons 'not '(tfun (tcon bool) (tcon bool)))
                            (cons 'eq '(tfun (tcon int) (tfun (tcon int) (tcon bool)))))))
        (list
         ;; Literal
         (funcall 'neovm--hm-w-infer-type base-env '(elit int 42))
         ;; Variable
         (funcall 'neovm--hm-w-infer-type base-env '(evar add))
         ;; Application: add 1
         (funcall 'neovm--hm-w-infer-type base-env '(eapp (evar add) (elit int 1)))
         ;; Application: add 1 2
         (funcall 'neovm--hm-w-infer-type base-env
                  '(eapp (eapp (evar add) (elit int 1)) (elit int 2)))
         ;; Lambda: \x -> x (identity, gets tvar -> tvar)
         (let ((r (funcall 'neovm--hm-w-infer-type nil '(elam x (evar x)))))
           ;; Should be (ok (tfun (tvar N) (tvar N))) for some N
           (list (car r)
                 (equal (nth 1 (nth 1 r)) (nth 2 (nth 1 r)))))
         ;; Let-polymorphism: let id = \x -> x in (id 42, id true)
         ;; id gets used at both int and bool types
         (funcall 'neovm--hm-w-infer-type nil
                  '(elet id (elam x (evar x))
                         (eapp (evar id) (elit int 42))))
         ;; Type error: not applied to int
         (car (funcall 'neovm--hm-w-infer-type base-env
                       '(eapp (evar not) (elit int 42))))
         ;; Unbound variable
         (car (funcall 'neovm--hm-w-infer-type nil '(evar unknown)))))
    (fmakunbound 'neovm--hm-w-fresh)
    (fmakunbound 'neovm--hm-w-apply)
    (fmakunbound 'neovm--hm-w-occurs)
    (fmakunbound 'neovm--hm-w-unify)
    (fmakunbound 'neovm--hm-w-ftv)
    (fmakunbound 'neovm--hm-w-env-ftv)
    (fmakunbound 'neovm--hm-w-generalize)
    (fmakunbound 'neovm--hm-w-instantiate)
    (fmakunbound 'neovm--hm-w-apply-env)
    (fmakunbound 'neovm--hm-w-infer)
    (fmakunbound 'neovm--hm-w-infer-type)
    (makunbound 'neovm--hm-w-counter)))
"#;
    let expect = expect_test::expect![[
        r#""ERR (wrong-type-argument listp \"mismatch (tcon bool) (tcon int)\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Let-polymorphism: using the same function at multiple types
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_hm_let_polymorphism() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  ;; Reuse W-algorithm infrastructure (minimal self-contained version)
  (defvar neovm--hm-lp-counter 0)
  (fset 'neovm--hm-lp-fresh (lambda () (setq neovm--hm-lp-counter (1+ neovm--hm-lp-counter)) (list 'tvar neovm--hm-lp-counter)))

  (fset 'neovm--hm-lp-apply
    (lambda (s ty)
      (cond ((null ty) nil)
            ((and (consp ty) (eq (car ty) 'tvar)) (let ((b (assq (nth 1 ty) s))) (if b (funcall 'neovm--hm-lp-apply s (cdr b)) ty)))
            ((and (consp ty) (eq (car ty) 'tcon)) ty)
            ((and (consp ty) (eq (car ty) 'tfun)) (list 'tfun (funcall 'neovm--hm-lp-apply s (nth 1 ty)) (funcall 'neovm--hm-lp-apply s (nth 2 ty))))
            ((and (consp ty) (eq (car ty) 'tlist)) (list 'tlist (funcall 'neovm--hm-lp-apply s (nth 1 ty))))
            (t ty))))

  (fset 'neovm--hm-lp-occurs (lambda (id ty s) (let ((ty (funcall 'neovm--hm-lp-apply s ty))) (cond ((and (consp ty) (eq (car ty) 'tvar)) (= id (nth 1 ty))) ((and (consp ty) (eq (car ty) 'tfun)) (or (funcall 'neovm--hm-lp-occurs id (nth 1 ty) s) (funcall 'neovm--hm-lp-occurs id (nth 2 ty) s))) ((and (consp ty) (eq (car ty) 'tlist)) (funcall 'neovm--hm-lp-occurs id (nth 1 ty) s)) (t nil)))))

  (fset 'neovm--hm-lp-unify
    (lambda (t1 t2 s) (let ((t1 (funcall 'neovm--hm-lp-apply s t1)) (t2 (funcall 'neovm--hm-lp-apply s t2))) (cond ((equal t1 t2) (cons 'ok s)) ((and (consp t1) (eq (car t1) 'tvar)) (if (funcall 'neovm--hm-lp-occurs (nth 1 t1) t2 s) (cons 'err "occurs") (cons 'ok (cons (cons (nth 1 t1) t2) s)))) ((and (consp t2) (eq (car t2) 'tvar)) (if (funcall 'neovm--hm-lp-occurs (nth 1 t2) t1 s) (cons 'err "occurs") (cons 'ok (cons (cons (nth 1 t2) t1) s)))) ((and (consp t1) (eq (car t1) 'tfun) (consp t2) (eq (car t2) 'tfun)) (let ((r (funcall 'neovm--hm-lp-unify (nth 1 t1) (nth 1 t2) s))) (if (eq (car r) 'err) r (funcall 'neovm--hm-lp-unify (nth 2 t1) (nth 2 t2) (cdr r))))) ((and (consp t1) (eq (car t1) 'tlist) (consp t2) (eq (car t2) 'tlist)) (funcall 'neovm--hm-lp-unify (nth 1 t1) (nth 1 t2) s)) (t (cons 'err "mismatch"))))))

  (fset 'neovm--hm-lp-ftv (lambda (ty) (cond ((null ty) nil) ((and (consp ty) (eq (car ty) 'tvar)) (list (nth 1 ty))) ((and (consp ty) (eq (car ty) 'tcon)) nil) ((and (consp ty) (eq (car ty) 'tfun)) (let ((r nil)) (dolist (v (append (funcall 'neovm--hm-lp-ftv (nth 1 ty)) (funcall 'neovm--hm-lp-ftv (nth 2 ty)))) (unless (memq v r) (push v r))) (nreverse r))) ((and (consp ty) (eq (car ty) 'tlist)) (funcall 'neovm--hm-lp-ftv (nth 1 ty))) (t nil))))

  (fset 'neovm--hm-lp-env-ftv (lambda (env) (let ((r nil)) (dolist (b env) (let ((fv (if (and (consp (cdr b)) (eq (car (cdr b)) 'scheme)) (let ((bound (nth 1 (cdr b))) (bfv (funcall 'neovm--hm-lp-ftv (nth 2 (cdr b)))) (res nil)) (dolist (v bfv) (unless (memq v bound) (push v res))) res) (funcall 'neovm--hm-lp-ftv (cdr b))))) (dolist (v fv) (unless (memq v r) (push v r))))) r)))

  (fset 'neovm--hm-lp-generalize (lambda (env ty) (let ((efv (funcall 'neovm--hm-lp-env-ftv env)) (tfv (funcall 'neovm--hm-lp-ftv ty)) (free nil)) (dolist (v tfv) (unless (memq v efv) (push v free))) (if free (list 'scheme (nreverse free) ty) ty))))

  (fset 'neovm--hm-lp-instantiate (lambda (scheme) (if (and (consp scheme) (eq (car scheme) 'scheme)) (let ((mapping nil)) (dolist (v (nth 1 scheme)) (push (cons v (funcall 'neovm--hm-lp-fresh)) mapping)) (funcall 'neovm--hm-lp-apply mapping (nth 2 scheme))) scheme)))

  (fset 'neovm--hm-lp-apply-env (lambda (s env) (mapcar (lambda (b) (cons (car b) (if (and (consp (cdr b)) (eq (car (cdr b)) 'scheme)) (cdr b) (funcall 'neovm--hm-lp-apply s (cdr b))))) env)))

  (fset 'neovm--hm-lp-infer
    (lambda (env expr s)
      (cond
       ((eq (car expr) 'elit) (list 'ok (list 'tcon (nth 1 expr)) s))
       ((eq (car expr) 'evar) (let ((b (assq (nth 1 expr) env))) (if b (list 'ok (funcall 'neovm--hm-lp-instantiate (cdr b)) s) (list 'err "unbound"))))
       ((eq (car expr) 'elam) (let* ((tv (funcall 'neovm--hm-lp-fresh)) (r (funcall 'neovm--hm-lp-infer (cons (cons (nth 1 expr) tv) env) (nth 2 expr) s))) (if (eq (car r) 'err) r (list 'ok (list 'tfun (funcall 'neovm--hm-lp-apply (nth 2 r) tv) (nth 1 r)) (nth 2 r)))))
       ((eq (car expr) 'eapp) (let* ((fr (funcall 'neovm--hm-lp-infer env (nth 1 expr) s))) (if (eq (car fr) 'err) fr (let* ((ar (funcall 'neovm--hm-lp-infer (funcall 'neovm--hm-lp-apply-env (nth 2 fr) env) (nth 2 expr) (nth 2 fr)))) (if (eq (car ar) 'err) ar (let* ((rv (funcall 'neovm--hm-lp-fresh)) (u (funcall 'neovm--hm-lp-unify (funcall 'neovm--hm-lp-apply (nth 2 ar) (nth 1 fr)) (list 'tfun (nth 1 ar) rv) (nth 2 ar)))) (if (eq (car u) 'err) u (list 'ok (funcall 'neovm--hm-lp-apply (cdr u) rv) (cdr u)))))))))
       ((eq (car expr) 'elet) (let* ((vr (funcall 'neovm--hm-lp-infer env (nth 2 expr) s))) (if (eq (car vr) 'err) vr (let* ((ae (funcall 'neovm--hm-lp-apply-env (nth 2 vr) env)) (gt (funcall 'neovm--hm-lp-generalize ae (funcall 'neovm--hm-lp-apply (nth 2 vr) (nth 1 vr))))) (funcall 'neovm--hm-lp-infer (cons (cons (nth 1 expr) gt) ae) (nth 3 expr) (nth 2 vr))))))
       (t (list 'err "unknown")))))

  (fset 'neovm--hm-lp-typeof
    (lambda (env expr)
      (setq neovm--hm-lp-counter 0)
      (let ((r (funcall 'neovm--hm-lp-infer env expr nil)))
        (if (eq (car r) 'err) (list 'err (nth 1 r))
          (list 'ok (funcall 'neovm--hm-lp-apply (nth 2 r) (nth 1 r)))))))

  (unwind-protect
      (let ((env (list (cons 'add '(tfun (tcon int) (tfun (tcon int) (tcon int))))
                       (cons 'neg '(tfun (tcon bool) (tcon bool))))))
        (list
         ;; let id = \x -> x in (id 5)  => int
         (funcall 'neovm--hm-lp-typeof env
                  '(elet id (elam x (evar x))
                         (eapp (evar id) (elit int 5))))
         ;; let id = \x -> x in (id true)  => bool
         (funcall 'neovm--hm-lp-typeof env
                  '(elet id (elam x (evar x))
                         (eapp (evar id) (elit bool #t))))
         ;; let const = \x -> \y -> x in (const 5 true)  => int
         (funcall 'neovm--hm-lp-typeof env
                  '(elet const (elam x (elam y (evar x)))
                         (eapp (eapp (evar const) (elit int 5)) (elit bool #t))))
         ;; Nested let: let f = \x -> add x 1 in let g = \y -> f (f y) in g 3
         (funcall 'neovm--hm-lp-typeof env
                  '(elet f (elam x (eapp (eapp (evar add) (evar x)) (elit int 1)))
                         (elet g (elam y (eapp (evar f) (eapp (evar f) (evar y))))
                                (eapp (evar g) (elit int 3)))))
         ;; Higher-order: let apply = \f -> \x -> f x in apply neg true
         (funcall 'neovm--hm-lp-typeof env
                  '(elet apply (elam f (elam x (eapp (evar f) (evar x))))
                         (eapp (eapp (evar apply) (evar neg)) (elit bool #t))))
         ;; Type error: let f = \x -> add x 1 in f true
         (car (funcall 'neovm--hm-lp-typeof env
                       '(elet f (elam x (eapp (eapp (evar add) (evar x)) (elit int 1)))
                              (eapp (evar f) (elit bool #t)))))))
    (fmakunbound 'neovm--hm-lp-fresh)
    (fmakunbound 'neovm--hm-lp-apply)
    (fmakunbound 'neovm--hm-lp-occurs)
    (fmakunbound 'neovm--hm-lp-unify)
    (fmakunbound 'neovm--hm-lp-ftv)
    (fmakunbound 'neovm--hm-lp-env-ftv)
    (fmakunbound 'neovm--hm-lp-generalize)
    (fmakunbound 'neovm--hm-lp-instantiate)
    (fmakunbound 'neovm--hm-lp-apply-env)
    (fmakunbound 'neovm--hm-lp-infer)
    (fmakunbound 'neovm--hm-lp-typeof)
    (makunbound 'neovm--hm-lp-counter)))
"#;
    let expect = expect_test::expect![[r##""ERR (invalid-read-syntax \"#t\" 59 54)""##]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Occurs check: preventing infinite types
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_hm_occurs_check_detailed() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (fset 'neovm--hm-oc-apply
    (lambda (s ty)
      (cond ((null ty) nil)
            ((and (consp ty) (eq (car ty) 'tvar)) (let ((b (assq (nth 1 ty) s))) (if b (funcall 'neovm--hm-oc-apply s (cdr b)) ty)))
            ((and (consp ty) (eq (car ty) 'tcon)) ty)
            ((and (consp ty) (eq (car ty) 'tfun)) (list 'tfun (funcall 'neovm--hm-oc-apply s (nth 1 ty)) (funcall 'neovm--hm-oc-apply s (nth 2 ty))))
            ((and (consp ty) (eq (car ty) 'tlist)) (list 'tlist (funcall 'neovm--hm-oc-apply s (nth 1 ty))))
            (t ty))))

  (fset 'neovm--hm-oc-occurs
    (lambda (id ty s)
      (let ((ty (funcall 'neovm--hm-oc-apply s ty)))
        (cond
         ((and (consp ty) (eq (car ty) 'tvar)) (= id (nth 1 ty)))
         ((and (consp ty) (eq (car ty) 'tfun))
          (or (funcall 'neovm--hm-oc-occurs id (nth 1 ty) s)
              (funcall 'neovm--hm-oc-occurs id (nth 2 ty) s)))
         ((and (consp ty) (eq (car ty) 'tlist))
          (funcall 'neovm--hm-oc-occurs id (nth 1 ty) s))
         (t nil)))))

  (fset 'neovm--hm-oc-unify
    (lambda (t1 t2 s)
      (let ((t1 (funcall 'neovm--hm-oc-apply s t1))
            (t2 (funcall 'neovm--hm-oc-apply s t2)))
        (cond
         ((equal t1 t2) (cons 'ok s))
         ((and (consp t1) (eq (car t1) 'tvar))
          (if (funcall 'neovm--hm-oc-occurs (nth 1 t1) t2 s)
              (cons 'err (format "infinite: t%d in %S" (nth 1 t1) t2))
            (cons 'ok (cons (cons (nth 1 t1) t2) s))))
         ((and (consp t2) (eq (car t2) 'tvar))
          (if (funcall 'neovm--hm-oc-occurs (nth 1 t2) t1 s)
              (cons 'err (format "infinite: t%d in %S" (nth 1 t2) t1))
            (cons 'ok (cons (cons (nth 1 t2) t1) s))))
         ((and (consp t1) (eq (car t1) 'tfun) (consp t2) (eq (car t2) 'tfun))
          (let ((r (funcall 'neovm--hm-oc-unify (nth 1 t1) (nth 1 t2) s)))
            (if (eq (car r) 'err) r
              (funcall 'neovm--hm-oc-unify (nth 2 t1) (nth 2 t2) (cdr r)))))
         ((and (consp t1) (eq (car t1) 'tlist) (consp t2) (eq (car t2) 'tlist))
          (funcall 'neovm--hm-oc-unify (nth 1 t1) (nth 1 t2) s))
         (t (cons 'err "mismatch"))))))

  (unwind-protect
      (list
       ;; Direct occurs: t1 = t1 -> int  (infinite type)
       (car (funcall 'neovm--hm-oc-unify '(tvar 1) '(tfun (tvar 1) (tcon int)) nil))
       ;; Indirect occurs: t1 = t2, t2 = t1 -> int
       (let* ((r1 (funcall 'neovm--hm-oc-unify '(tvar 1) '(tvar 2) nil)))
         (if (eq (car r1) 'err) 'err-step1
           (car (funcall 'neovm--hm-oc-unify '(tvar 2) '(tfun (tvar 1) (tcon int)) (cdr r1)))))
       ;; Deeply nested occurs: t1 = list (t1)
       (car (funcall 'neovm--hm-oc-unify '(tvar 1) '(tlist (tvar 1)) nil))
       ;; No occurs: t1 = t2 -> int (different vars)
       (car (funcall 'neovm--hm-oc-unify '(tvar 1) '(tfun (tvar 2) (tcon int)) nil))
       ;; Self-unification: t1 = t1 (ok, no binding needed)
       (funcall 'neovm--hm-oc-unify '(tvar 1) '(tvar 1) nil)
       ;; Occurs in function return: t1 = int -> t1
       (car (funcall 'neovm--hm-oc-unify '(tvar 1) '(tfun (tcon int) (tvar 1)) nil))
       ;; Chain without occurs: t1=t2, t2=t3, t3=int (all ok)
       (let* ((r1 (funcall 'neovm--hm-oc-unify '(tvar 1) '(tvar 2) nil))
              (r2 (funcall 'neovm--hm-oc-unify '(tvar 2) '(tvar 3) (cdr r1)))
              (r3 (funcall 'neovm--hm-oc-unify '(tvar 3) '(tcon int) (cdr r2))))
         (list (car r3)
               (funcall 'neovm--hm-oc-apply (cdr r3) '(tvar 1))
               (funcall 'neovm--hm-oc-apply (cdr r3) '(tvar 2))
               (funcall 'neovm--hm-oc-apply (cdr r3) '(tvar 3)))))
    (fmakunbound 'neovm--hm-oc-apply)
    (fmakunbound 'neovm--hm-oc-occurs)
    (fmakunbound 'neovm--hm-oc-unify)))
"#;
    let expect = expect_test::expect![[
        r#""OK (err err err ok (ok) err (ok (tcon int) (tcon int) (tcon int)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
