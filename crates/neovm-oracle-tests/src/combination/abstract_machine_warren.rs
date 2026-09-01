//! Oracle parity tests for a Warren Abstract Machine (WAM)-inspired Prolog
//! engine in Elisp: heap representation for terms, unification algorithm,
//! register allocation for queries, knowledge base queries, and list
//! processing predicates.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// WAM heap representation: store terms in a flat vector
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_wam_heap_representation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // The WAM stores terms on a heap as tagged cells:
    //   (ref . addr)  -- reference/variable cell pointing to addr
    //   (str . addr)  -- structure cell pointing to functor at addr
    //   (con . value) -- constant cell
    //   (fun . (name . arity)) -- functor cell
    // Build terms on a heap and read them back.
    let form = r#"(progn
  ;; Create a new WAM state: heap vector + heap pointer
  (fset 'neovm--wam-make
    (lambda (size)
      (list (make-vector size nil) 0)))

  ;; Get/set heap cell
  (fset 'neovm--wam-heap (lambda (wam) (car wam)))
  (fset 'neovm--wam-hp (lambda (wam) (cadr wam)))
  (fset 'neovm--wam-set-hp (lambda (wam val) (setcar (cdr wam) val)))
  (fset 'neovm--wam-get (lambda (wam addr) (aref (car wam) addr)))
  (fset 'neovm--wam-put
    (lambda (wam addr cell)
      (aset (car wam) addr cell)
      addr))

  ;; Push a cell onto the heap, return its address
  (fset 'neovm--wam-push
    (lambda (wam cell)
      (let ((hp (funcall 'neovm--wam-hp wam)))
        (funcall 'neovm--wam-put wam hp cell)
        (funcall 'neovm--wam-set-hp wam (1+ hp))
        hp)))

  ;; Build a constant term on the heap
  (fset 'neovm--wam-put-const
    (lambda (wam value)
      (funcall 'neovm--wam-push wam (cons 'con value))))

  ;; Build a new unbound variable (self-referencing REF cell)
  (fset 'neovm--wam-put-var
    (lambda (wam)
      (let ((hp (funcall 'neovm--wam-hp wam)))
        (funcall 'neovm--wam-push wam (cons 'ref hp)))))

  ;; Build a structure f/n(args...) on the heap
  (fset 'neovm--wam-put-structure
    (lambda (wam name arity args)
      (let ((str-addr (funcall 'neovm--wam-hp wam)))
        ;; Push STR cell pointing to next cell (the functor)
        (funcall 'neovm--wam-push wam (cons 'str (1+ str-addr)))
        ;; Push functor cell
        (funcall 'neovm--wam-push wam (cons 'fun (cons name arity)))
        ;; Push argument addresses (they must already be on heap)
        (dolist (arg args)
          (funcall 'neovm--wam-push wam (cons 'ref arg)))
        str-addr)))

  (unwind-protect
      (let ((wam (funcall 'neovm--wam-make 64)))
        ;; Build: a (constant)
        (let ((a-addr (funcall 'neovm--wam-put-const wam 'a)))
          ;; Build: X (unbound variable)
          (let ((x-addr (funcall 'neovm--wam-put-var wam)))
            ;; Build: f(a, X) -- structure with 2 args
            (let ((f-addr (funcall 'neovm--wam-put-structure
                                   wam 'f 2 (list a-addr x-addr))))
              (list
               ;; Heap pointer advanced correctly
               (funcall 'neovm--wam-hp wam)
               ;; Read back the constant
               (funcall 'neovm--wam-get wam a-addr)
               ;; Read back the variable (self-ref)
               (funcall 'neovm--wam-get wam x-addr)
               ;; Structure cell
               (funcall 'neovm--wam-get wam f-addr)
               ;; Functor cell
               (funcall 'neovm--wam-get wam (1+ f-addr))
               ;; Arguments (ref cells)
               (funcall 'neovm--wam-get wam (+ f-addr 2))
               (funcall 'neovm--wam-get wam (+ f-addr 3)))))))
    (fmakunbound 'neovm--wam-make)
    (fmakunbound 'neovm--wam-heap)
    (fmakunbound 'neovm--wam-hp)
    (fmakunbound 'neovm--wam-set-hp)
    (fmakunbound 'neovm--wam-get)
    (fmakunbound 'neovm--wam-put)
    (fmakunbound 'neovm--wam-push)
    (fmakunbound 'neovm--wam-put-const)
    (fmakunbound 'neovm--wam-put-var)
    (fmakunbound 'neovm--wam-put-structure)))"#;
    let expect = expect_test::expect![[
        r#""OK (6 (con . a) (ref . 1) (str . 3) (fun f . 2) (ref . 0) (ref . 1))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// WAM deref and unification on the heap
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_wam_unification() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement deref (follow reference chains) and unify on the WAM heap.
    let form = r#"(progn
  (fset 'neovm--wam-make
    (lambda (size) (list (make-vector size nil) 0)))
  (fset 'neovm--wam-hp (lambda (w) (cadr w)))
  (fset 'neovm--wam-set-hp (lambda (w v) (setcar (cdr w) v)))
  (fset 'neovm--wam-get (lambda (w a) (aref (car w) a)))
  (fset 'neovm--wam-put (lambda (w a c) (aset (car w) a c) a))
  (fset 'neovm--wam-push
    (lambda (w c)
      (let ((hp (funcall 'neovm--wam-hp w)))
        (aset (car w) hp c)
        (funcall 'neovm--wam-set-hp w (1+ hp))
        hp)))
  (fset 'neovm--wam-put-const
    (lambda (w v) (funcall 'neovm--wam-push w (cons 'con v))))
  (fset 'neovm--wam-put-var
    (lambda (w)
      (let ((hp (funcall 'neovm--wam-hp w)))
        (funcall 'neovm--wam-push w (cons 'ref hp)))))

  ;; Dereference: follow REF chains to the final cell
  (fset 'neovm--wam-deref
    (lambda (w addr)
      (let ((cell (funcall 'neovm--wam-get w addr)))
        (if (and (eq (car cell) 'ref) (/= (cdr cell) addr))
            (funcall 'neovm--wam-deref w (cdr cell))
          addr))))

  ;; Bind: make one variable point to another address
  (fset 'neovm--wam-bind
    (lambda (w a1 a2)
      (funcall 'neovm--wam-put w a1 (cons 'ref a2))))

  ;; Unify two heap addresses
  (fset 'neovm--wam-unify
    (lambda (w a1 a2)
      (let ((stack (list (cons a1 a2)))
            (fail nil))
        (while (and stack (not fail))
          (let* ((pair (car stack))
                 (d1 (funcall 'neovm--wam-deref w (car pair)))
                 (d2 (funcall 'neovm--wam-deref w (cdr pair)))
                 (c1 (funcall 'neovm--wam-get w d1))
                 (c2 (funcall 'neovm--wam-get w d2)))
            (setq stack (cdr stack))
            (unless (= d1 d2)
              (cond
               ;; Both REF (unbound): bind the lower to the higher
               ((and (eq (car c1) 'ref) (eq (car c2) 'ref))
                (if (< d1 d2)
                    (funcall 'neovm--wam-bind w d1 d2)
                  (funcall 'neovm--wam-bind w d2 d1)))
               ;; One is REF: bind it to the other
               ((eq (car c1) 'ref) (funcall 'neovm--wam-bind w d1 d2))
               ((eq (car c2) 'ref) (funcall 'neovm--wam-bind w d2 d1))
               ;; Both constants: must be equal
               ((and (eq (car c1) 'con) (eq (car c2) 'con))
                (unless (equal (cdr c1) (cdr c2))
                  (setq fail t)))
               ;; Both structures: check functor then unify args
               ((and (eq (car c1) 'str) (eq (car c2) 'str))
                (let ((f1 (funcall 'neovm--wam-get w (cdr c1)))
                      (f2 (funcall 'neovm--wam-get w (cdr c2))))
                  (if (not (equal f1 f2))
                      (setq fail t)
                    ;; Push argument pairs onto stack
                    (let ((arity (cddr f1))
                          (i 1))
                      (while (<= i arity)
                        (push (cons (+ (cdr c1) i) (+ (cdr c2) i)) stack)
                        (setq i (1+ i)))))))
               (t (setq fail t))))))
        (not fail))))

  ;; Read back the value at an address (fully dereferenced)
  (fset 'neovm--wam-read-term
    (lambda (w addr)
      (let* ((d (funcall 'neovm--wam-deref w addr))
             (cell (funcall 'neovm--wam-get w d)))
        (cond
         ((eq (car cell) 'con) (cdr cell))
         ((eq (car cell) 'ref) (list 'var d))
         (t (list 'cell (car cell) d))))))

  (unwind-protect
      (let ((w (funcall 'neovm--wam-make 128)))
        ;; Build X (var), Y (var), const 'a', const 'b'
        (let ((x (funcall 'neovm--wam-put-var w))
              (y (funcall 'neovm--wam-put-var w))
              (ca (funcall 'neovm--wam-put-const w 'a))
              (cb (funcall 'neovm--wam-put-const w 'b)))
          ;; Test 1: unify X with const 'a' -- should succeed
          (let ((r1 (funcall 'neovm--wam-unify w x ca)))
            ;; Test 2: X now reads as 'a'
            (let ((v1 (funcall 'neovm--wam-read-term w x)))
              ;; Test 3: unify Y with X -- Y should become 'a'
              (let ((r2 (funcall 'neovm--wam-unify w y x)))
                (let ((v2 (funcall 'neovm--wam-read-term w y)))
                  ;; Test 4: build new Z, unify Z with 'b'
                  (let* ((z (funcall 'neovm--wam-put-var w))
                         (r3 (funcall 'neovm--wam-unify w z cb))
                         (v3 (funcall 'neovm--wam-read-term w z)))
                    ;; Test 5: try unify X (='a') with 'b' -- should fail
                    (let ((r4 (funcall 'neovm--wam-unify w x cb)))
                      ;; Test 6: unify two unbound vars
                      (let* ((p (funcall 'neovm--wam-put-var w))
                             (q (funcall 'neovm--wam-put-var w))
                             (r5 (funcall 'neovm--wam-unify w p q))
                             ;; Bind p/q to const 'c' via p
                             (cc (funcall 'neovm--wam-put-const w 'c))
                             (r6 (funcall 'neovm--wam-unify w p cc)))
                        (list
                         r1 v1 r2 v2 r3 v3 r4 r5 r6
                         ;; Both p and q should read as 'c'
                         (funcall 'neovm--wam-read-term w p)
                         (funcall 'neovm--wam-read-term w q))))))))))
    (fmakunbound 'neovm--wam-make)
    (fmakunbound 'neovm--wam-hp)
    (fmakunbound 'neovm--wam-set-hp)
    (fmakunbound 'neovm--wam-get)
    (fmakunbound 'neovm--wam-put)
    (fmakunbound 'neovm--wam-push)
    (fmakunbound 'neovm--wam-put-const)
    (fmakunbound 'neovm--wam-put-var)
    (fmakunbound 'neovm--wam-deref)
    (fmakunbound 'neovm--wam-bind)
    (fmakunbound 'neovm--wam-unify)
    (fmakunbound 'neovm--wam-read-term)))"#;
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// WAM register allocation: put/get structure instructions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_wam_register_allocation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate WAM register file: allocate terms to registers,
    // then build structures referencing registers.
    let form = r#"(progn
  ;; WAM with registers: (heap hp registers)
  (fset 'neovm--wamr-make
    (lambda (heap-size num-regs)
      (list (make-vector heap-size nil) 0 (make-vector num-regs nil))))
  (fset 'neovm--wamr-heap (lambda (w) (car w)))
  (fset 'neovm--wamr-hp (lambda (w) (cadr w)))
  (fset 'neovm--wamr-set-hp (lambda (w v) (setcar (cdr w) v)))
  (fset 'neovm--wamr-regs (lambda (w) (nth 2 w)))
  (fset 'neovm--wamr-get-reg
    (lambda (w i) (aref (nth 2 w) i)))
  (fset 'neovm--wamr-set-reg
    (lambda (w i v) (aset (nth 2 w) i v)))
  (fset 'neovm--wamr-push
    (lambda (w cell)
      (let ((hp (funcall 'neovm--wamr-hp w)))
        (aset (car w) hp cell)
        (funcall 'neovm--wamr-set-hp w (1+ hp))
        hp)))
  (fset 'neovm--wamr-get-cell
    (lambda (w a) (aref (car w) a)))

  ;; put_structure f/n, Xi: create STR+FUN on heap, store in register
  (fset 'neovm--wamr-put-structure
    (lambda (w name arity reg)
      (let ((addr (funcall 'neovm--wamr-hp w)))
        (funcall 'neovm--wamr-push w (cons 'str (1+ addr)))
        (funcall 'neovm--wamr-push w (cons 'fun (cons name arity)))
        (funcall 'neovm--wamr-set-reg w reg addr)
        addr)))

  ;; set_variable Xi: push new REF cell, store addr in register
  (fset 'neovm--wamr-set-variable
    (lambda (w reg)
      (let ((hp (funcall 'neovm--wamr-hp w)))
        (funcall 'neovm--wamr-push w (cons 'ref hp))
        (funcall 'neovm--wamr-set-reg w reg hp)
        hp)))

  ;; set_value Xi: push REF to whatever is in register
  (fset 'neovm--wamr-set-value
    (lambda (w reg)
      (let ((val (funcall 'neovm--wamr-get-reg w reg)))
        (funcall 'neovm--wamr-push w (cons 'ref val)))))

  ;; set_constant c: push constant
  (fset 'neovm--wamr-set-constant
    (lambda (w value)
      (funcall 'neovm--wamr-push w (cons 'con value))))

  (unwind-protect
      (let ((w (funcall 'neovm--wamr-make 64 8)))
        ;; Build the term: p(Z, h(Z, W), f(W))
        ;; Step 1: put_structure h/2, X3
        (funcall 'neovm--wamr-put-structure w 'h 2 3)
        ;; set_variable X2 (this is Z)
        (funcall 'neovm--wamr-set-variable w 2)
        ;; set_variable X4 (this is W)
        (funcall 'neovm--wamr-set-variable w 4)

        ;; Step 2: put_structure f/1, X5
        (funcall 'neovm--wamr-put-structure w 'f 1 5)
        ;; set_value X4 (W)
        (funcall 'neovm--wamr-set-value w 4)

        ;; Step 3: put_structure p/3, X1
        (funcall 'neovm--wamr-put-structure w 'p 3 1)
        ;; set_value X2 (Z)
        (funcall 'neovm--wamr-set-value w 2)
        ;; set_value X3 (h(Z,W))
        (funcall 'neovm--wamr-set-value w 3)
        ;; set_value X5 (f(W))
        (funcall 'neovm--wamr-set-value w 5)

        (list
         ;; Heap pointer
         (funcall 'neovm--wamr-hp w)
         ;; Register X1 points to p/3 structure
         (funcall 'neovm--wamr-get-cell w (funcall 'neovm--wamr-get-reg w 1))
         ;; Register X3 points to h/2 structure
         (funcall 'neovm--wamr-get-cell w (funcall 'neovm--wamr-get-reg w 3))
         ;; Register X5 points to f/1 structure
         (funcall 'neovm--wamr-get-cell w (funcall 'neovm--wamr-get-reg w 5))
         ;; X2 (Z) is an unbound ref
         (funcall 'neovm--wamr-get-cell w (funcall 'neovm--wamr-get-reg w 2))
         ;; X4 (W) is an unbound ref
         (funcall 'neovm--wamr-get-cell w (funcall 'neovm--wamr-get-reg w 4))
         ;; Verify functor of X1's structure
         (funcall 'neovm--wamr-get-cell w
                  (1+ (funcall 'neovm--wamr-get-reg w 1)))))
    (fmakunbound 'neovm--wamr-make)
    (fmakunbound 'neovm--wamr-heap)
    (fmakunbound 'neovm--wamr-hp)
    (fmakunbound 'neovm--wamr-set-hp)
    (fmakunbound 'neovm--wamr-regs)
    (fmakunbound 'neovm--wamr-get-reg)
    (fmakunbound 'neovm--wamr-set-reg)
    (fmakunbound 'neovm--wamr-push)
    (fmakunbound 'neovm--wamr-get-cell)
    (fmakunbound 'neovm--wamr-put-structure)
    (fmakunbound 'neovm--wamr-set-variable)
    (fmakunbound 'neovm--wamr-set-value)
    (fmakunbound 'neovm--wamr-set-constant)))"#;
    let expect = expect_test::expect![[
        r#""OK (12 (str . 8) (str . 1) (str . 5) (ref . 2) (ref . 3) (fun p . 3))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: query a simple knowledge base using WAM-style unification
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_wam_knowledge_base_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A simplified Prolog engine: facts as (functor . args) lists,
    // queries with variables (?x, ?y), unification-based matching,
    // and result extraction.
    let form = r#"(progn
  ;; Variable check
  (fset 'neovm--wkb-var-p
    (lambda (t1) (and (symbolp t1) (string-prefix-p "?" (symbol-name t1)))))

  ;; Apply substitution
  (fset 'neovm--wkb-apply
    (lambda (subst term)
      (cond
       ((funcall 'neovm--wkb-var-p term)
        (let ((b (assq term subst)))
          (if b (funcall 'neovm--wkb-apply subst (cdr b)) term)))
       ((consp term)
        (mapcar (lambda (t1) (funcall 'neovm--wkb-apply subst t1)) term))
       (t term))))

  ;; Occurs check
  (fset 'neovm--wkb-occurs
    (lambda (var term subst)
      (cond
       ((equal var term) t)
       ((and (funcall 'neovm--wkb-var-p term) (assq term subst))
        (funcall 'neovm--wkb-occurs var (cdr (assq term subst)) subst))
       ((consp term)
        (let ((found nil))
          (dolist (s term) (when (funcall 'neovm--wkb-occurs var s subst) (setq found t)))
          found))
       (t nil))))

  ;; Unify
  (fset 'neovm--wkb-unify
    (lambda (t1 t2 subst)
      (cond
       ((eq subst 'fail) 'fail)
       ((equal t1 t2) subst)
       ((funcall 'neovm--wkb-var-p t1)
        (let ((b (assq t1 subst)))
          (if b (funcall 'neovm--wkb-unify (cdr b) t2 subst)
            (if (funcall 'neovm--wkb-occurs t1 t2 subst) 'fail
              (cons (cons t1 t2) subst)))))
       ((funcall 'neovm--wkb-var-p t2)
        (funcall 'neovm--wkb-unify t2 t1 subst))
       ((and (consp t1) (consp t2) (= (length t1) (length t2)))
        (let ((s subst) (i 0) (len (length t1)))
          (while (and (not (eq s 'fail)) (< i len))
            (setq s (funcall 'neovm--wkb-unify (nth i t1) (nth i t2) s))
            (setq i (1+ i)))
          s))
       (t 'fail))))

  ;; Query: find all matching facts
  (fset 'neovm--wkb-query
    (lambda (db pattern)
      (let ((results nil))
        (dolist (fact db)
          (let ((s (funcall 'neovm--wkb-unify pattern fact nil)))
            (unless (eq s 'fail)
              (push (funcall 'neovm--wkb-apply s pattern) results))))
        (nreverse results))))

  ;; Extract variables from a pattern
  (fset 'neovm--wkb-extract-vars
    (lambda (pattern subst)
      (let ((vars nil))
        (dolist (term (cdr pattern))
          (when (funcall 'neovm--wkb-var-p term)
            (push (cons term (funcall 'neovm--wkb-apply subst term)) vars)))
        (nreverse vars))))

  (unwind-protect
      (let ((db '((parent tom bob)
                  (parent tom liz)
                  (parent bob ann)
                  (parent bob pat)
                  (age tom 63)
                  (age bob 35)
                  (age liz 31)
                  (age ann 8)
                  (age pat 5)
                  (male tom)
                  (male bob)
                  (male pat)
                  (female liz)
                  (female ann))))
        (list
         ;; Who are tom's children?
         (funcall 'neovm--wkb-query db '(parent tom ?child))
         ;; Who are the males?
         (funcall 'neovm--wkb-query db '(male ?who))
         ;; Who has age > 30? (manual filter)
         (let ((age-results (funcall 'neovm--wkb-query db '(age ?who ?age))))
           (let ((filtered nil))
             (dolist (r age-results)
               (when (> (nth 2 r) 30)
                 (push (nth 1 r) filtered)))
             (nreverse filtered)))
         ;; Who is ann's parent?
         (funcall 'neovm--wkb-query db '(parent ?p ann))
         ;; All female members
         (funcall 'neovm--wkb-query db '(female ?f))
         ;; Non-matching query
         (funcall 'neovm--wkb-query db '(sibling ?x ?y))
         ;; Fully ground query
         (funcall 'neovm--wkb-query db '(parent tom bob))
         ;; Fully ground query that fails
         (funcall 'neovm--wkb-query db '(parent bob tom))))
    (fmakunbound 'neovm--wkb-var-p)
    (fmakunbound 'neovm--wkb-apply)
    (fmakunbound 'neovm--wkb-occurs)
    (fmakunbound 'neovm--wkb-unify)
    (fmakunbound 'neovm--wkb-query)
    (fmakunbound 'neovm--wkb-extract-vars)))"#;
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \"?\" 87 54)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: list processing predicates in the WAM style
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_wam_list_predicates() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement list-based predicates (append, member, length, reverse)
    // using a Prolog-style search with unification and backtracking.
    let form = r#"(progn
  (fset 'neovm--wlp-var-p
    (lambda (t1) (and (symbolp t1) (string-prefix-p "?" (symbol-name t1)))))

  (fset 'neovm--wlp-apply
    (lambda (subst term)
      (cond
       ((funcall 'neovm--wlp-var-p term)
        (let ((b (assq term subst)))
          (if b (funcall 'neovm--wlp-apply subst (cdr b)) term)))
       ((consp term)
        (cons (funcall 'neovm--wlp-apply subst (car term))
              (funcall 'neovm--wlp-apply subst (cdr term))))
       (t term))))

  (fset 'neovm--wlp-unify
    (lambda (t1 t2 subst)
      (cond
       ((eq subst 'fail) 'fail)
       ((equal t1 t2) subst)
       ((funcall 'neovm--wlp-var-p t1)
        (let ((b (assq t1 subst)))
          (if b (funcall 'neovm--wlp-unify (cdr b) t2 subst)
            (cons (cons t1 t2) subst))))
       ((funcall 'neovm--wlp-var-p t2)
        (funcall 'neovm--wlp-unify t2 t1 subst))
       ((and (consp t1) (consp t2))
        (let ((s (funcall 'neovm--wlp-unify (car t1) (car t2) subst)))
          (if (eq s 'fail) 'fail
            (funcall 'neovm--wlp-unify (cdr t1) (cdr t2) s))))
       (t 'fail))))

  ;; Prolog append/3: append([], L, L). append([H|T1], L, [H|T2]) :- append(T1, L, T2).
  ;; We encode as clauses and use depth-limited search.
  ;; Simpler approach: direct recursive Prolog-like solver.
  (fset 'neovm--wlp-append
    (lambda (l1 l2)
      "Append two lists (ground terms)."
      (if (null l1) l2
        (cons (car l1) (funcall 'neovm--wlp-append (cdr l1) l2)))))

  ;; Prolog-style member check
  (fset 'neovm--wlp-member
    (lambda (elem lst)
      (cond
       ((null lst) nil)
       ((equal elem (car lst)) t)
       (t (funcall 'neovm--wlp-member elem (cdr lst))))))

  ;; Prolog-style reverse with accumulator
  (fset 'neovm--wlp-reverse
    (lambda (lst acc)
      (if (null lst) acc
        (funcall 'neovm--wlp-reverse (cdr lst) (cons (car lst) acc)))))

  ;; Prolog-style length
  (fset 'neovm--wlp-length
    (lambda (lst)
      (if (null lst) 0
        (1+ (funcall 'neovm--wlp-length (cdr lst))))))

  ;; Prolog-style permutation (generate all permutations)
  (fset 'neovm--wlp-remove-one
    (lambda (elem lst)
      "Remove first occurrence of ELEM from LST."
      (cond
       ((null lst) nil)
       ((equal (car lst) elem) (cdr lst))
       (t (cons (car lst) (funcall 'neovm--wlp-remove-one elem (cdr lst)))))))

  (fset 'neovm--wlp-permutations
    (lambda (lst)
      "Generate all permutations of LST."
      (if (null lst) '(())
        (let ((result nil))
          (dolist (elem lst)
            (let ((rest (funcall 'neovm--wlp-remove-one elem lst)))
              (dolist (perm (funcall 'neovm--wlp-permutations rest))
                (push (cons elem perm) result))))
          (nreverse result)))))

  (unwind-protect
      (list
       ;; append
       (funcall 'neovm--wlp-append '(1 2 3) '(4 5))
       (funcall 'neovm--wlp-append nil '(a b))
       (funcall 'neovm--wlp-append '(x) nil)
       (funcall 'neovm--wlp-append nil nil)
       ;; member
       (funcall 'neovm--wlp-member 'b '(a b c))
       (funcall 'neovm--wlp-member 'd '(a b c))
       (funcall 'neovm--wlp-member 1 '(1))
       ;; reverse
       (funcall 'neovm--wlp-reverse '(1 2 3 4) nil)
       (funcall 'neovm--wlp-reverse nil nil)
       (funcall 'neovm--wlp-reverse '(a) nil)
       ;; length
       (funcall 'neovm--wlp-length '(a b c d e))
       (funcall 'neovm--wlp-length nil)
       ;; permutations
       (funcall 'neovm--wlp-permutations '(1 2 3))
       (funcall 'neovm--wlp-permutations '(a))
       (length (funcall 'neovm--wlp-permutations '(1 2 3 4)))
       ;; Unification with list structures
       (funcall 'neovm--wlp-unify '(1 . (2 . (3 . nil)))
                                   '(1 2 3)
                                   nil)
       (funcall 'neovm--wlp-unify '(?h . ?t) '(a b c) nil))
    (fmakunbound 'neovm--wlp-var-p)
    (fmakunbound 'neovm--wlp-apply)
    (fmakunbound 'neovm--wlp-unify)
    (fmakunbound 'neovm--wlp-append)
    (fmakunbound 'neovm--wlp-member)
    (fmakunbound 'neovm--wlp-reverse)
    (fmakunbound 'neovm--wlp-length)
    (fmakunbound 'neovm--wlp-remove-one)
    (fmakunbound 'neovm--wlp-permutations)))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 2 3 4 5) (a b) (x) nil t nil t (4 3 2 1) nil (a) 5 0 ((1 2 3) (1 3 2) (2 1 3) (2 3 1) (3 1 2) (3 2 1)) ((a)) 24 nil fail)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: rule-based inference with WAM-style chaining
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_wam_rule_inference() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Define facts and rules, then use forward chaining to derive new facts.
    // Rules: grandparent(X,Z) :- parent(X,Y), parent(Y,Z).
    //        ancestor(X,Y) :- parent(X,Y).
    //        ancestor(X,Z) :- parent(X,Y), ancestor(Y,Z).
    let form = r#"(progn
  (fset 'neovm--wri-var-p
    (lambda (t1) (and (symbolp t1) (string-prefix-p "?" (symbol-name t1)))))

  (fset 'neovm--wri-apply
    (lambda (subst term)
      (cond
       ((funcall 'neovm--wri-var-p term)
        (let ((b (assq term subst)))
          (if b (funcall 'neovm--wri-apply subst (cdr b)) term)))
       ((consp term)
        (mapcar (lambda (t1) (funcall 'neovm--wri-apply subst t1)) term))
       (t term))))

  (fset 'neovm--wri-unify
    (lambda (t1 t2 subst)
      (cond
       ((eq subst 'fail) 'fail)
       ((equal t1 t2) subst)
       ((funcall 'neovm--wri-var-p t1)
        (let ((b (assq t1 subst)))
          (if b (funcall 'neovm--wri-unify (cdr b) t2 subst)
            (cons (cons t1 t2) subst))))
       ((funcall 'neovm--wri-var-p t2)
        (funcall 'neovm--wri-unify t2 t1 subst))
       ((and (consp t1) (consp t2) (= (length t1) (length t2)))
        (let ((s subst) (i 0) (len (length t1)))
          (while (and (not (eq s 'fail)) (< i len))
            (setq s (funcall 'neovm--wri-unify (nth i t1) (nth i t2) s))
            (setq i (1+ i)))
          s))
       (t 'fail))))

  ;; Query facts
  (fset 'neovm--wri-query-facts
    (lambda (db pattern)
      (let ((results nil))
        (dolist (fact db)
          (let ((s (funcall 'neovm--wri-unify pattern fact nil)))
            (unless (eq s 'fail)
              (push s results))))
        (nreverse results))))

  ;; Derive grandparent: for each parent(X,Y) find parent(Y,Z)
  (fset 'neovm--wri-grandparents
    (lambda (db)
      (let ((results nil))
        (dolist (s1 (funcall 'neovm--wri-query-facts db '(parent ?x ?y)))
          (let ((y-val (funcall 'neovm--wri-apply s1 '?y))
                (x-val (funcall 'neovm--wri-apply s1 '?x)))
            (dolist (s2 (funcall 'neovm--wri-query-facts
                                 db (list 'parent y-val '?z)))
              (let ((z-val (funcall 'neovm--wri-apply s2 '?z)))
                (push (list 'grandparent x-val z-val) results)))))
        (nreverse results))))

  ;; Derive ancestors (depth-limited to avoid infinite recursion)
  (fset 'neovm--wri-ancestors
    (lambda (db person depth)
      (if (= depth 0) nil
        (let ((results nil))
          ;; Direct parents are ancestors
          (dolist (s (funcall 'neovm--wri-query-facts
                              db (list 'parent '?anc person)))
            (let ((anc (funcall 'neovm--wri-apply s '?anc)))
              (unless (member anc results)
                (push anc results)
                ;; Their ancestors are also ancestors
                (dolist (a (funcall 'neovm--wri-ancestors db anc (1- depth)))
                  (unless (member a results)
                    (push a results))))))
          (nreverse results)))))

  (unwind-protect
      (let ((db '((parent alice bob)
                  (parent alice carol)
                  (parent bob dave)
                  (parent bob eve)
                  (parent carol frank)
                  (parent carol grace)
                  (parent dave henry))))
        (list
         ;; Grandparents
         (funcall 'neovm--wri-grandparents db)
         ;; Ancestors of henry
         (funcall 'neovm--wri-ancestors db 'henry 5)
         ;; Ancestors of dave
         (funcall 'neovm--wri-ancestors db 'dave 5)
         ;; Ancestors of alice (none -- she's the root)
         (funcall 'neovm--wri-ancestors db 'alice 5)
         ;; Direct children of alice
         (let ((subs (funcall 'neovm--wri-query-facts db '(parent alice ?c))))
           (mapcar (lambda (s) (funcall 'neovm--wri-apply s '?c)) subs))
         ;; Number of grandparent relationships
         (length (funcall 'neovm--wri-grandparents db))))
    (fmakunbound 'neovm--wri-var-p)
    (fmakunbound 'neovm--wri-apply)
    (fmakunbound 'neovm--wri-unify)
    (fmakunbound 'neovm--wri-query-facts)
    (fmakunbound 'neovm--wri-grandparents)
    (fmakunbound 'neovm--wri-ancestors)))"#;
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \"?\" 64 50)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
