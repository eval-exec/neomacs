//! Advanced oracle parity tests for type system concepts in Elisp:
//! bidirectional type checking, polymorphic type inference,
//! type unification with occurs check, subtype checking with variance,
//! record types with structural subtyping, union/intersection types,
//! and generic instantiation.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Bidirectional type checking (check mode vs synthesis mode)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_type_system_adv_bidirectional() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Bidirectional type system: synthesize types bottom-up, check types top-down.
    // Expressions: (lit value type), (var name), (app fn arg), (lam param body),
    //              (ann expr type) for type annotations.
    let form = r#"(progn
  ;; Type representation: (int), (bool), (str), (-> arg ret)
  ;; Context: alist of (name . type)

  (fset 'neovm--ts-type-equal
    (lambda (t1 t2)
      (equal t1 t2)))

  (fset 'neovm--ts-synth
    (lambda (ctx expr)
      "Synthesize the type of EXPR in context CTX. Returns type or (error msg)."
      (cond
       ;; Literal: (lit value type)
       ((eq (car expr) 'lit) (nth 2 expr))
       ;; Variable: (var name)
       ((eq (car expr) 'var)
        (let ((entry (assq (cadr expr) ctx)))
          (if entry (cdr entry) (list 'error "unbound variable"))))
       ;; Application: (app fn arg)
       ((eq (car expr) 'app)
        (let ((fn-type (funcall 'neovm--ts-synth ctx (cadr expr))))
          (if (and (consp fn-type) (eq (car fn-type) '->))
              (let ((arg-ok (funcall 'neovm--ts-check ctx (nth 2 expr) (cadr fn-type))))
                (if arg-ok
                    (nth 2 fn-type)
                  (list 'error "argument type mismatch")))
            (list 'error "not a function type"))))
       ;; Annotated: (ann expr type) -- check mode
       ((eq (car expr) 'ann)
        (let ((ok (funcall 'neovm--ts-check ctx (cadr expr) (nth 2 expr))))
          (if ok (nth 2 expr) (list 'error "annotation check failed"))))
       (t (list 'error "cannot synthesize")))))

  (fset 'neovm--ts-check
    (lambda (ctx expr expected)
      "Check that EXPR has type EXPECTED in CTX. Returns t or nil."
      (cond
       ;; Lambda: (lam param body) checked against (-> arg ret)
       ((and (eq (car expr) 'lam)
             (consp expected) (eq (car expected) '->))
        (let ((param (cadr expr))
              (body (nth 2 expr))
              (arg-type (cadr expected))
              (ret-type (nth 2 expected)))
          (let ((new-ctx (cons (cons param arg-type) ctx)))
            (funcall 'neovm--ts-check new-ctx body ret-type))))
       ;; Fallback: synthesize and compare
       (t
        (let ((synth-type (funcall 'neovm--ts-synth ctx expr)))
          (funcall 'neovm--ts-type-equal synth-type expected))))))

  (unwind-protect
      (let ((ctx '((x . (int)) (y . (bool)) (f . (-> (int) (bool))))))
        (list
         ;; Synthesize literal
         (funcall 'neovm--ts-synth ctx '(lit 42 (int)))
         ;; Synthesize variable
         (funcall 'neovm--ts-synth ctx '(var x))
         ;; Synthesize application f(x)
         (funcall 'neovm--ts-synth ctx '(app (var f) (var x)))
         ;; Check lambda: (\p -> p) against (-> (int) (int))
         (funcall 'neovm--ts-check ctx '(lam p (var p)) '(-> (int) (int)))
         ;; Check lambda: (\p -> p) against (-> (bool) (bool))
         (funcall 'neovm--ts-check ctx '(lam p (var p)) '(-> (bool) (bool)))
         ;; Mismatch: f(y) where f expects int but y is bool
         (funcall 'neovm--ts-synth ctx '(app (var f) (var y)))
         ;; Annotation check
         (funcall 'neovm--ts-synth ctx '(ann (lit 42 (int)) (int)))
         ;; Annotation mismatch
         (funcall 'neovm--ts-synth ctx '(ann (lit 42 (int)) (bool)))
         ;; Nested lambda: (\a -> \b -> a) against (-> (int) (-> (bool) (int)))
         (funcall 'neovm--ts-check ctx
                  '(lam a (lam b (var a)))
                  '(-> (int) (-> (bool) (int))))))
    (fmakunbound 'neovm--ts-type-equal)
    (fmakunbound 'neovm--ts-synth)
    (fmakunbound 'neovm--ts-check)))"#;
    let expect = expect_test::expect![[
        r#""OK ((int) (int) (bool) t t (error \"argument type mismatch\") (int) (error \"annotation check failed\") t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Type unification with occurs check
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_type_system_adv_unification_occurs_check() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Type unification: find a substitution that makes two types equal.
    // Occurs check prevents infinite types (e.g., T = T -> int).
    let form = r#"(progn
  (fset 'neovm--ts-subst-apply
    (lambda (subst ty)
      "Apply substitution (alist of (tvar-id . type)) to a type."
      (cond
       ((and (consp ty) (eq (car ty) 'tvar))
        (let ((binding (assq (cadr ty) subst)))
          (if binding
              (funcall 'neovm--ts-subst-apply subst (cdr binding))
            ty)))
       ((and (consp ty) (eq (car ty) '->))
        (list '-> (funcall 'neovm--ts-subst-apply subst (cadr ty))
              (funcall 'neovm--ts-subst-apply subst (nth 2 ty))))
       ((and (consp ty) (eq (car ty) 'list-of))
        (list 'list-of (funcall 'neovm--ts-subst-apply subst (cadr ty))))
       (t ty))))

  (fset 'neovm--ts-occurs-in
    (lambda (var-id ty)
      "Check if type variable VAR-ID occurs in TY."
      (cond
       ((and (consp ty) (eq (car ty) 'tvar))
        (eq (cadr ty) var-id))
       ((and (consp ty) (eq (car ty) '->))
        (or (funcall 'neovm--ts-occurs-in var-id (cadr ty))
            (funcall 'neovm--ts-occurs-in var-id (nth 2 ty))))
       ((and (consp ty) (eq (car ty) 'list-of))
        (funcall 'neovm--ts-occurs-in var-id (cadr ty)))
       (t nil))))

  (fset 'neovm--ts-unify
    (lambda (subst t1 t2)
      "Unify T1 and T2 under SUBST. Returns updated subst or (error msg)."
      (let ((t1a (funcall 'neovm--ts-subst-apply subst t1))
            (t2a (funcall 'neovm--ts-subst-apply subst t2)))
        (cond
         ((equal t1a t2a) subst)
         ;; t1 is tvar
         ((and (consp t1a) (eq (car t1a) 'tvar))
          (if (funcall 'neovm--ts-occurs-in (cadr t1a) t2a)
              (list 'error "occurs check failed")
            (cons (cons (cadr t1a) t2a) subst)))
         ;; t2 is tvar
         ((and (consp t2a) (eq (car t2a) 'tvar))
          (if (funcall 'neovm--ts-occurs-in (cadr t2a) t1a)
              (list 'error "occurs check failed")
            (cons (cons (cadr t2a) t1a) subst)))
         ;; Both are ->
         ((and (consp t1a) (eq (car t1a) '->)
               (consp t2a) (eq (car t2a) '->))
          (let ((s1 (funcall 'neovm--ts-unify subst (cadr t1a) (cadr t2a))))
            (if (and (consp s1) (eq (car s1) 'error)) s1
              (funcall 'neovm--ts-unify s1 (nth 2 t1a) (nth 2 t2a)))))
         ;; Both are list-of
         ((and (consp t1a) (eq (car t1a) 'list-of)
               (consp t2a) (eq (car t2a) 'list-of))
          (funcall 'neovm--ts-unify subst (cadr t1a) (cadr t2a)))
         (t (list 'error (concat "cannot unify "
                                 (prin1-to-string t1a) " with "
                                 (prin1-to-string t2a))))))))

  (unwind-protect
      (list
       ;; Simple: int = int
       (funcall 'neovm--ts-unify nil '(int) '(int))
       ;; tvar: T0 = int => T0 -> int
       (funcall 'neovm--ts-unify nil '(tvar T0) '(int))
       ;; Function: (T0 -> T1) = (int -> bool) => T0->int, T1->bool
       (funcall 'neovm--ts-unify nil '(-> (tvar T0) (tvar T1)) '(-> (int) (bool)))
       ;; Transitive: T0 = T1, T1 = int => T0->int via T1
       (let ((s1 (funcall 'neovm--ts-unify nil '(tvar T0) '(tvar T1))))
         (funcall 'neovm--ts-unify s1 '(tvar T1) '(int)))
       ;; Occurs check fail: T0 = (T0 -> int)
       (funcall 'neovm--ts-unify nil '(tvar T0) '(-> (tvar T0) (int)))
       ;; List unification: list-of(T0) = list-of(int)
       (funcall 'neovm--ts-unify nil '(list-of (tvar T0)) '(list-of (int)))
       ;; Mismatch: int != bool
       (funcall 'neovm--ts-unify nil '(int) '(bool)))
    (fmakunbound 'neovm--ts-subst-apply)
    (fmakunbound 'neovm--ts-occurs-in)
    (fmakunbound 'neovm--ts-unify)))"#;
    let expect = expect_test::expect![[
        r#""OK (nil ((T0 int)) ((T1 bool) (T0 int)) ((T1 int) (T0 tvar T1)) (error \"occurs check failed\") ((T0 int)) (error \"cannot unify (int) with (bool)\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Subtype checking with variance (covariant return, contravariant arg)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_type_system_adv_subtype_variance() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Subtype lattice: int <: number, string <: any, number <: any, bool <: any
    // Function types: (-> A B) <: (-> C D) iff C <: A (contravariant) and B <: D (covariant)
    let form = r#"(progn
  (fset 'neovm--ts-is-subtype
    (lambda (sub super)
      "Check if SUB is a subtype of SUPER."
      (cond
       ;; Same type
       ((equal sub super) t)
       ;; Base subtype relations
       ((and (equal sub '(int)) (equal super '(number))) t)
       ((and (equal sub '(float)) (equal super '(number))) t)
       ((and (equal sub '(int)) (equal super '(any))) t)
       ((and (equal sub '(float)) (equal super '(any))) t)
       ((and (equal sub '(number)) (equal super '(any))) t)
       ((and (equal sub '(string)) (equal super '(any))) t)
       ((and (equal sub '(bool)) (equal super '(any))) t)
       ;; Transitivity: int <: number <: any
       ((and (equal sub '(int)) (equal super '(any)))
        t)
       ;; Function subtyping: contravariant in arg, covariant in return
       ((and (consp sub) (eq (car sub) '->)
             (consp super) (eq (car super) '->))
        (and (funcall 'neovm--ts-is-subtype (cadr super) (cadr sub))   ;; contravariant
             (funcall 'neovm--ts-is-subtype (nth 2 sub) (nth 2 super)))) ;; covariant
       ;; list-of is covariant
       ((and (consp sub) (eq (car sub) 'list-of)
             (consp super) (eq (car super) 'list-of))
        (funcall 'neovm--ts-is-subtype (cadr sub) (cadr super)))
       (t nil))))

  (unwind-protect
      (list
       ;; Base subtypes
       (funcall 'neovm--ts-is-subtype '(int) '(number))
       (funcall 'neovm--ts-is-subtype '(float) '(number))
       (funcall 'neovm--ts-is-subtype '(int) '(any))
       (funcall 'neovm--ts-is-subtype '(number) '(any))
       (funcall 'neovm--ts-is-subtype '(string) '(any))
       ;; Not subtype
       (funcall 'neovm--ts-is-subtype '(string) '(int))
       (funcall 'neovm--ts-is-subtype '(number) '(int))
       ;; Function covariance/contravariance:
       ;; (number -> int) <: (int -> number) because
       ;;   arg: int <: number (contra) and ret: int <: number (co)
       (funcall 'neovm--ts-is-subtype '(-> (number) (int)) '(-> (int) (number)))
       ;; Not subtype: (int -> number) is NOT <: (number -> int)
       (funcall 'neovm--ts-is-subtype '(-> (int) (number)) '(-> (number) (int)))
       ;; list-of covariance
       (funcall 'neovm--ts-is-subtype '(list-of (int)) '(list-of (number)))
       (funcall 'neovm--ts-is-subtype '(list-of (number)) '(list-of (int)))
       ;; Reflexivity
       (funcall 'neovm--ts-is-subtype '(-> (int) (bool)) '(-> (int) (bool))))
    (fmakunbound 'neovm--ts-is-subtype)))"#;
    let expect = expect_test::expect![[r#""OK (t t t t t nil nil t nil t nil t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Record types with structural subtyping
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_type_system_adv_record_structural_subtyping() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Record types: (record (field1 . type1) (field2 . type2) ...)
    // Structural subtyping: A <: B if A has all fields of B with subtype-compatible types.
    // Width subtyping: extra fields are allowed.
    let form = r#"(progn
  (fset 'neovm--ts-base-subtype
    (lambda (sub super)
      (or (equal sub super)
          (and (equal sub '(int)) (equal super '(number)))
          (and (equal sub '(float)) (equal super '(number)))
          (and (equal sub '(int)) (equal super '(any)))
          (and (equal sub '(float)) (equal super '(any)))
          (and (equal sub '(number)) (equal super '(any)))
          (and (equal sub '(string)) (equal super '(any)))
          (and (equal sub '(bool)) (equal super '(any))))))

  (fset 'neovm--ts-record-subtype
    (lambda (sub-fields super-fields)
      "Check if record with SUB-FIELDS is a subtype of record with SUPER-FIELDS.
Each field is (name . type)."
      (let ((ok t))
        (dolist (sf super-fields)
          (let* ((fname (car sf))
                 (ftype (cdr sf))
                 (sub-entry (assq fname sub-fields)))
            (unless (and sub-entry
                         (funcall 'neovm--ts-base-subtype (cdr sub-entry) ftype))
              (setq ok nil))))
        ok)))

  (unwind-protect
      (let (;; Point: {x: int, y: int}
            (point '((x . (int)) (y . (int))))
            ;; Point3D: {x: int, y: int, z: int}
            (point3d '((x . (int)) (y . (int)) (z . (int))))
            ;; NumPoint: {x: number, y: number}
            (numpoint '((x . (number)) (y . (number))))
            ;; Labeled: {x: int, y: int, label: string}
            (labeled '((x . (int)) (y . (int)) (label . (string))))
            ;; Empty record
            (empty nil))
        (list
         ;; Point3D <: Point (width subtyping: extra z field)
         (funcall 'neovm--ts-record-subtype point3d point)
         ;; Point NOT <: Point3D (missing z)
         (funcall 'neovm--ts-record-subtype point point3d)
         ;; Point <: NumPoint (depth subtyping: int <: number)
         (funcall 'neovm--ts-record-subtype point numpoint)
         ;; NumPoint NOT <: Point (number NOT <: int)
         (funcall 'neovm--ts-record-subtype numpoint point)
         ;; Labeled <: Point (width: extra label)
         (funcall 'neovm--ts-record-subtype labeled point)
         ;; Everything <: empty record
         (funcall 'neovm--ts-record-subtype point empty)
         (funcall 'neovm--ts-record-subtype empty empty)
         ;; Point3D <: NumPoint (width + depth)
         (funcall 'neovm--ts-record-subtype point3d numpoint)))
    (fmakunbound 'neovm--ts-base-subtype)
    (fmakunbound 'neovm--ts-record-subtype)))"#;
    let expect = expect_test::expect![[r#""OK (t nil t nil t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Union and intersection types
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_type_system_adv_union_intersection() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Union types: (union type1 type2 ...) -- value can be any of the listed types
    // Intersection types: (inter type1 type2 ...) -- value must satisfy all types
    // Subtype rules: T <: (union ... T ...), (inter ... T ...) <: T
    let form = r#"(progn
  (fset 'neovm--ts-base-eq-or-sub
    (lambda (sub super)
      (or (equal sub super)
          (and (equal sub '(int)) (equal super '(number)))
          (and (equal sub '(float)) (equal super '(number))))))

  (fset 'neovm--ts-subtype-ext
    (lambda (sub super)
      "Extended subtype check with union/intersection."
      (cond
       ((equal sub super) t)
       ;; sub <: (union t1 t2 ...) if sub <: any ti
       ((and (consp super) (eq (car super) 'union))
        (let ((ok nil))
          (dolist (ti (cdr super))
            (when (funcall 'neovm--ts-subtype-ext sub ti)
              (setq ok t)))
          ok))
       ;; (union t1 t2 ...) <: super if every ti <: super
       ((and (consp sub) (eq (car sub) 'union))
        (let ((ok t))
          (dolist (ti (cdr sub))
            (unless (funcall 'neovm--ts-subtype-ext ti super)
              (setq ok nil)))
          ok))
       ;; (inter t1 t2 ...) <: super if any ti <: super
       ((and (consp sub) (eq (car sub) 'inter))
        (let ((ok nil))
          (dolist (ti (cdr sub))
            (when (funcall 'neovm--ts-subtype-ext ti super)
              (setq ok t)))
          ok))
       ;; sub <: (inter t1 t2 ...) if sub <: every ti
       ((and (consp super) (eq (car super) 'inter))
        (let ((ok t))
          (dolist (ti (cdr super))
            (unless (funcall 'neovm--ts-subtype-ext sub ti)
              (setq ok nil)))
          ok))
       ;; Base subtype
       (t (funcall 'neovm--ts-base-eq-or-sub sub super)))))

  (unwind-protect
      (list
       ;; int <: (union int string)
       (funcall 'neovm--ts-subtype-ext '(int) '(union (int) (string)))
       ;; string <: (union int string)
       (funcall 'neovm--ts-subtype-ext '(string) '(union (int) (string)))
       ;; bool NOT <: (union int string)
       (funcall 'neovm--ts-subtype-ext '(bool) '(union (int) (string)))
       ;; (union int float) <: number (both subtypes of number)
       (funcall 'neovm--ts-subtype-ext '(union (int) (float)) '(number))
       ;; (union int string) NOT <: number (string not <: number)
       (funcall 'neovm--ts-subtype-ext '(union (int) (string)) '(number))
       ;; (inter int number) <: int (int <: int)
       (funcall 'neovm--ts-subtype-ext '(inter (int) (number)) '(int))
       ;; int <: (inter int number)? int <: int AND int <: number
       (funcall 'neovm--ts-subtype-ext '(int) '(inter (int) (number)))
       ;; string NOT <: (inter int number)
       (funcall 'neovm--ts-subtype-ext '(string) '(inter (int) (number)))
       ;; Nested: int <: (union (inter (int) (number)) (string))
       (funcall 'neovm--ts-subtype-ext '(int) '(union (inter (int) (number)) (string))))
    (fmakunbound 'neovm--ts-base-eq-or-sub)
    (fmakunbound 'neovm--ts-subtype-ext)))"#;
    let expect = expect_test::expect![[r#""OK (t t nil t nil t t nil t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Generic type instantiation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_type_system_adv_generic_instantiation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Generic types: (forall (T0 T1 ...) body-type)
    // Instantiation: substitute type variables with concrete types.
    let form = r#"(progn
  (fset 'neovm--ts-instantiate
    (lambda (forall-type type-args)
      "Instantiate a (forall (vars...) body) with TYPE-ARGS."
      (let ((vars (cadr forall-type))
            (body (nth 2 forall-type))
            (subst nil))
        ;; Build substitution
        (let ((i 0))
          (dolist (v vars)
            (setq subst (cons (cons v (nth i type-args)) subst))
            (setq i (1+ i))))
        ;; Apply substitution
        (funcall 'neovm--ts-inst-apply subst body))))

  (fset 'neovm--ts-inst-apply
    (lambda (subst ty)
      (cond
       ((and (consp ty) (eq (car ty) 'tvar))
        (let ((binding (assq (cadr ty) subst)))
          (if binding (cdr binding) ty)))
       ((and (consp ty) (eq (car ty) '->))
        (list '-> (funcall 'neovm--ts-inst-apply subst (cadr ty))
              (funcall 'neovm--ts-inst-apply subst (nth 2 ty))))
       ((and (consp ty) (eq (car ty) 'list-of))
        (list 'list-of (funcall 'neovm--ts-inst-apply subst (cadr ty))))
       ((and (consp ty) (eq (car ty) 'pair))
        (list 'pair (funcall 'neovm--ts-inst-apply subst (cadr ty))
              (funcall 'neovm--ts-inst-apply subst (nth 2 ty))))
       (t ty))))

  (unwind-protect
      (let (;; identity: forall T. T -> T
            (id-type '(forall (T) (-> (tvar T) (tvar T))))
            ;; const: forall A B. A -> B -> A
            (const-type '(forall (A B) (-> (tvar A) (-> (tvar B) (tvar A)))))
            ;; map: forall A B. (A -> B) -> list(A) -> list(B)
            (map-type '(forall (A B) (-> (-> (tvar A) (tvar B))
                                         (-> (list-of (tvar A)) (list-of (tvar B))))))
            ;; pair: forall A B. A -> B -> pair(A, B)
            (pair-type '(forall (A B) (-> (tvar A) (-> (tvar B) (pair (tvar A) (tvar B)))))))
        (list
         ;; identity<int> = int -> int
         (funcall 'neovm--ts-instantiate id-type '((int)))
         ;; identity<string> = string -> string
         (funcall 'neovm--ts-instantiate id-type '((string)))
         ;; const<int, bool> = int -> bool -> int
         (funcall 'neovm--ts-instantiate const-type '((int) (bool)))
         ;; map<string, int> = (string -> int) -> list(string) -> list(int)
         (funcall 'neovm--ts-instantiate map-type '((string) (int)))
         ;; pair<int, string> = int -> string -> pair(int, string)
         (funcall 'neovm--ts-instantiate pair-type '((int) (string)))
         ;; Nested: identity<(-> (int) (bool))> = (int -> bool) -> (int -> bool)
         (funcall 'neovm--ts-instantiate id-type '((-> (int) (bool))))))
    (fmakunbound 'neovm--ts-instantiate)
    (fmakunbound 'neovm--ts-inst-apply)))"#;
    let expect = expect_test::expect![[
        r#""OK ((-> (int) (int)) (-> (string) (string)) (-> (int) (-> (bool) (int))) (-> (-> (string) (int)) (-> (list-of (string)) (list-of (int)))) (-> (int) (-> (string) (pair (int) (string)))) (-> (-> (int) (bool)) (-> (int) (bool))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Polymorphic type inference (Hindley-Milner let-generalization)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_type_system_adv_hm_let_generalization() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Hindley-Milner style: let-bound definitions are generalized to polymorphic types.
    // Simulates type inference for a small expression language with let-polymorphism.
    let form = r#"(progn
  ;; Fresh type variable counter
  (fset 'neovm--ts-hm-fresh-counter (lambda () (list 0)))
  (fset 'neovm--ts-hm-fresh
    (lambda (counter)
      (let ((n (car counter)))
        (setcar counter (1+ n))
        (list 'tvar (intern (concat "T" (number-to-string n)))))))

  ;; Free type variables in a type
  (fset 'neovm--ts-hm-free-vars
    (lambda (ty)
      (cond
       ((and (consp ty) (eq (car ty) 'tvar)) (list (cadr ty)))
       ((and (consp ty) (eq (car ty) '->))
        (append (funcall 'neovm--ts-hm-free-vars (cadr ty))
                (funcall 'neovm--ts-hm-free-vars (nth 2 ty))))
       (t nil))))

  ;; Free vars in an environment
  (fset 'neovm--ts-hm-env-free-vars
    (lambda (env)
      (let ((result nil))
        (dolist (entry env)
          (let ((ty (cdr entry)))
            (if (and (consp ty) (eq (car ty) 'forall))
                ;; forall vars are NOT free
                (let ((bound (cadr ty))
                      (fvs (funcall 'neovm--ts-hm-free-vars (nth 2 ty))))
                  (dolist (v fvs)
                    (unless (memq v bound)
                      (setq result (cons v result)))))
              (setq result (append (funcall 'neovm--ts-hm-free-vars ty) result)))))
        result)))

  ;; Generalize: turn free type vars (not in env) into forall
  (fset 'neovm--ts-hm-generalize
    (lambda (env ty)
      (let ((env-fvs (funcall 'neovm--ts-hm-env-free-vars env))
            (ty-fvs (funcall 'neovm--ts-hm-free-vars ty))
            (gen-vars nil))
        (dolist (v ty-fvs)
          (unless (or (memq v env-fvs) (memq v gen-vars))
            (setq gen-vars (cons v gen-vars))))
        (if gen-vars
            (list 'forall (nreverse gen-vars) ty)
          ty))))

  (unwind-protect
      (let ((env '((x . (int)) (y . (bool)))))
        (list
         ;; Generalize T0 -> T0 in env with x:int, y:bool
         ;; T0 is free in type but not in env => forall (T0) (-> (tvar T0) (tvar T0))
         (funcall 'neovm--ts-hm-generalize env '(-> (tvar T0) (tvar T0)))
         ;; Generalize (-> (int) (tvar T1)) in env
         ;; T1 not in env => forall (T1) (-> (int) (tvar T1))
         (funcall 'neovm--ts-hm-generalize env '(-> (int) (tvar T1)))
         ;; Generalize (int) in env => no free tvars => (int) (no forall)
         (funcall 'neovm--ts-hm-generalize env '(int))
         ;; Free vars of env
         (funcall 'neovm--ts-hm-env-free-vars env)
         ;; Free vars of (-> (tvar A) (-> (tvar B) (tvar A)))
         (sort (funcall 'neovm--ts-hm-free-vars '(-> (tvar A) (-> (tvar B) (tvar A))))
               (lambda (a b) (string< (symbol-name a) (symbol-name b))))))
    (fmakunbound 'neovm--ts-hm-fresh-counter)
    (fmakunbound 'neovm--ts-hm-fresh)
    (fmakunbound 'neovm--ts-hm-free-vars)
    (fmakunbound 'neovm--ts-hm-env-free-vars)
    (fmakunbound 'neovm--ts-hm-generalize)))"#;
    let expect = expect_test::expect![[
        r#""OK ((forall (T0) (-> (tvar T0) (tvar T0))) (forall (T1) (-> (int) (tvar T1))) (int) nil (A A B))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
