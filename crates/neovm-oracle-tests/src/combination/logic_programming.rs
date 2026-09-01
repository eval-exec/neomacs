//! Oracle parity tests for a simple logic programming engine in Elisp:
//! facts database, queries with variable binding, conjunction of goals,
//! backtracking, list operations in logic (append, member, reverse),
//! and arithmetic constraints.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Facts database and simple queries with variable binding
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_logic_prog_facts_and_queries() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A facts database stores ground facts as lists of atoms.
    // A query can contain variables (symbols starting with ?).
    // Unification binds variables to values.
    let form = r#"(progn
  ;; Check if a term is a logic variable
  (fset 'neovm--lp-var-p
    (lambda (x)
      (and (symbolp x)
           (string-prefix-p "?" (symbol-name x)))))

  ;; Look up a variable in a substitution (alist)
  (fset 'neovm--lp-lookup
    (lambda (var subst)
      (let ((binding (assq var subst)))
        (if binding
            (let ((val (cdr binding)))
              ;; Walk: if val is also a variable, follow the chain
              (if (funcall 'neovm--lp-var-p val)
                  (funcall 'neovm--lp-lookup val subst)
                val))
          var))))

  ;; Unify two terms under a substitution. Returns updated subst or 'fail.
  (fset 'neovm--lp-unify
    (lambda (t1 t2 subst)
      (cond
       ((eq subst 'fail) 'fail)
       ((equal t1 t2) subst)
       ((funcall 'neovm--lp-var-p t1)
        (let ((val (funcall 'neovm--lp-lookup t1 subst)))
          (if (funcall 'neovm--lp-var-p val)
              (cons (cons val t2) subst)
            (funcall 'neovm--lp-unify val t2 subst))))
       ((funcall 'neovm--lp-var-p t2)
        (funcall 'neovm--lp-unify t2 t1 subst))
       ((and (consp t1) (consp t2))
        (funcall 'neovm--lp-unify
                 (cdr t1) (cdr t2)
                 (funcall 'neovm--lp-unify (car t1) (car t2) subst)))
       (t 'fail))))

  ;; Apply a substitution to a term (resolve all variables)
  (fset 'neovm--lp-apply-subst
    (lambda (term subst)
      (cond
       ((funcall 'neovm--lp-var-p term)
        (let ((val (funcall 'neovm--lp-lookup term subst)))
          (if (funcall 'neovm--lp-var-p val)
              val
            (funcall 'neovm--lp-apply-subst val subst))))
       ((consp term)
        (cons (funcall 'neovm--lp-apply-subst (car term) subst)
              (funcall 'neovm--lp-apply-subst (cdr term) subst)))
       (t term))))

  (unwind-protect
      (list
       ;; Variable detection
       (funcall 'neovm--lp-var-p '?x)
       (funcall 'neovm--lp-var-p 'foo)
       (funcall 'neovm--lp-var-p '?person)
       ;; Simple unification
       (funcall 'neovm--lp-unify 'a 'a nil)
       (funcall 'neovm--lp-unify 'a 'b nil)
       (funcall 'neovm--lp-unify '?x 'hello nil)
       (funcall 'neovm--lp-unify '?x '?y nil)
       ;; Unify structures
       (funcall 'neovm--lp-unify '(parent ?x bob) '(parent alice bob) nil)
       (funcall 'neovm--lp-unify '(parent ?x ?y) '(parent alice bob) nil)
       ;; Unification failure
       (funcall 'neovm--lp-unify '(parent alice ?x) '(parent bob ?y) nil)
       ;; Apply substitution
       (funcall 'neovm--lp-apply-subst '(likes ?x ?y)
                '((?x . alice) (?y . bob)))
       (funcall 'neovm--lp-apply-subst '(f ?x (g ?y))
                '((?x . 1) (?y . 2))))
    (fmakunbound 'neovm--lp-var-p)
    (fmakunbound 'neovm--lp-lookup)
    (fmakunbound 'neovm--lp-unify)
    (fmakunbound 'neovm--lp-apply-subst)))"#;
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \"?\" 58 36)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Conjunction of goals and backtracking
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_logic_prog_conjunction_backtracking() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Query a database of facts with conjunction and backtracking.
    // Returns all solutions (list of substitutions).
    let form = r#"(progn
  (fset 'neovm--lp-var-p
    (lambda (x) (and (symbolp x) (string-prefix-p "?" (symbol-name x)))))

  (fset 'neovm--lp-lookup
    (lambda (var subst)
      (let ((binding (assq var subst)))
        (if binding
            (let ((val (cdr binding)))
              (if (funcall 'neovm--lp-var-p val)
                  (funcall 'neovm--lp-lookup val subst) val))
          var))))

  (fset 'neovm--lp-unify
    (lambda (t1 t2 subst)
      (cond
       ((eq subst 'fail) 'fail)
       ((equal t1 t2) subst)
       ((funcall 'neovm--lp-var-p t1)
        (let ((val (funcall 'neovm--lp-lookup t1 subst)))
          (if (funcall 'neovm--lp-var-p val)
              (cons (cons val t2) subst) (funcall 'neovm--lp-unify val t2 subst))))
       ((funcall 'neovm--lp-var-p t2) (funcall 'neovm--lp-unify t2 t1 subst))
       ((and (consp t1) (consp t2))
        (funcall 'neovm--lp-unify (cdr t1) (cdr t2)
                 (funcall 'neovm--lp-unify (car t1) (car t2) subst)))
       (t 'fail))))

  (fset 'neovm--lp-apply-subst
    (lambda (term subst)
      (cond
       ((funcall 'neovm--lp-var-p term)
        (let ((val (funcall 'neovm--lp-lookup term subst)))
          (if (funcall 'neovm--lp-var-p val) val
            (funcall 'neovm--lp-apply-subst val subst))))
       ((consp term)
        (cons (funcall 'neovm--lp-apply-subst (car term) subst)
              (funcall 'neovm--lp-apply-subst (cdr term) subst)))
       (t term))))

  ;; Query a single goal against the database, returning list of substitutions
  (fset 'neovm--lp-query-goal
    (lambda (goal db subst)
      (let ((results nil))
        (dolist (fact db)
          (let ((s (funcall 'neovm--lp-unify goal fact subst)))
            (unless (eq s 'fail)
              (setq results (cons s results)))))
        (nreverse results))))

  ;; Query a conjunction of goals (AND): each goal must succeed,
  ;; threading substitutions through.
  (fset 'neovm--lp-query-conj
    (lambda (goals db subst)
      (if (null goals)
          (list subst)  ;; All goals satisfied
        (let ((results nil))
          (dolist (s (funcall 'neovm--lp-query-goal (car goals) db subst))
            (let ((rest-results (funcall 'neovm--lp-query-conj (cdr goals) db s)))
              (setq results (append results rest-results))))
          results))))

  (unwind-protect
      (let ((db '((parent tom bob)
                  (parent tom liz)
                  (parent bob ann)
                  (parent bob pat)
                  (parent pat jim)
                  (male tom) (male bob) (male pat) (male jim)
                  (female liz) (female ann))))
        (list
         ;; Simple query: who is tom's child?
         (mapcar (lambda (s) (funcall 'neovm--lp-apply-subst '?x s))
                 (funcall 'neovm--lp-query-goal '(parent tom ?x) db nil))
         ;; Query: who is bob's child?
         (mapcar (lambda (s) (funcall 'neovm--lp-apply-subst '?x s))
                 (funcall 'neovm--lp-query-goal '(parent bob ?x) db nil))
         ;; Query: who is pat's child?
         (mapcar (lambda (s) (funcall 'neovm--lp-apply-subst '?x s))
                 (funcall 'neovm--lp-query-goal '(parent pat ?x) db nil))
         ;; All parent-child pairs
         (length (funcall 'neovm--lp-query-goal '(parent ?x ?y) db nil))
         ;; Conjunction: parent(tom, ?x) AND male(?x)
         ;; Should find bob (tom's male child)
         (mapcar (lambda (s) (funcall 'neovm--lp-apply-subst '?x s))
                 (funcall 'neovm--lp-query-conj
                          '((parent tom ?x) (male ?x)) db nil))
         ;; Conjunction: parent(tom, ?x) AND female(?x)
         (mapcar (lambda (s) (funcall 'neovm--lp-apply-subst '?x s))
                 (funcall 'neovm--lp-query-conj
                          '((parent tom ?x) (female ?x)) db nil))
         ;; Grandparent: parent(?x, ?z) AND parent(tom, ?x) => tom's grandchildren
         (mapcar (lambda (s) (funcall 'neovm--lp-apply-subst '?z s))
                 (funcall 'neovm--lp-query-conj
                          '((parent tom ?x) (parent ?x ?z)) db nil))
         ;; No match: parent(jim, ?x) => empty
         (funcall 'neovm--lp-query-goal '(parent jim ?x) db nil)))
    (fmakunbound 'neovm--lp-var-p)
    (fmakunbound 'neovm--lp-lookup)
    (fmakunbound 'neovm--lp-unify)
    (fmakunbound 'neovm--lp-apply-subst)
    (fmakunbound 'neovm--lp-query-goal)
    (fmakunbound 'neovm--lp-query-conj)))"#;
    let expect = expect_test::expect![[r#""OK (nil nil nil 0 nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Rules with head and body (Horn clauses)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_logic_prog_rules() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Extend the engine with rules: (rule HEAD BODY...)
    // A rule matches if HEAD unifies with the query and all BODY goals succeed.
    let form = r#"(progn
  (fset 'neovm--lp-var-p
    (lambda (x) (and (symbolp x) (string-prefix-p "?" (symbol-name x)))))

  (fset 'neovm--lp-lookup
    (lambda (var subst)
      (let ((binding (assq var subst)))
        (if binding
            (let ((val (cdr binding)))
              (if (funcall 'neovm--lp-var-p val)
                  (funcall 'neovm--lp-lookup val subst) val))
          var))))

  (fset 'neovm--lp-unify
    (lambda (t1 t2 subst)
      (cond
       ((eq subst 'fail) 'fail) ((equal t1 t2) subst)
       ((funcall 'neovm--lp-var-p t1)
        (let ((val (funcall 'neovm--lp-lookup t1 subst)))
          (if (funcall 'neovm--lp-var-p val) (cons (cons val t2) subst)
            (funcall 'neovm--lp-unify val t2 subst))))
       ((funcall 'neovm--lp-var-p t2) (funcall 'neovm--lp-unify t2 t1 subst))
       ((and (consp t1) (consp t2))
        (funcall 'neovm--lp-unify (cdr t1) (cdr t2)
                 (funcall 'neovm--lp-unify (car t1) (car t2) subst)))
       (t 'fail))))

  (fset 'neovm--lp-apply-subst
    (lambda (term subst)
      (cond
       ((funcall 'neovm--lp-var-p term)
        (let ((val (funcall 'neovm--lp-lookup term subst)))
          (if (funcall 'neovm--lp-var-p val) val
            (funcall 'neovm--lp-apply-subst val subst))))
       ((consp term)
        (cons (funcall 'neovm--lp-apply-subst (car term) subst)
              (funcall 'neovm--lp-apply-subst (cdr term) subst)))
       (t term))))

  ;; Rename variables in a rule to avoid capture (alpha-rename)
  ;; Appends a counter suffix to each variable
  (fset 'neovm--lp-rename-counter 0)

  (fset 'neovm--lp-collect-vars
    (lambda (term)
      (cond
       ((funcall 'neovm--lp-var-p term) (list term))
       ((consp term)
        (let ((result nil))
          (dolist (v (append (funcall 'neovm--lp-collect-vars (car term))
                             (funcall 'neovm--lp-collect-vars (cdr term))))
            (unless (memq v result) (setq result (cons v result))))
          result))
       (t nil))))

  (fset 'neovm--lp-rename-vars
    (lambda (term)
      (setq neovm--lp-rename-counter (1+ neovm--lp-rename-counter))
      (let* ((vars (funcall 'neovm--lp-collect-vars term))
             (mapping nil))
        (dolist (v vars)
          (let ((new-name (intern (concat (symbol-name v) "_"
                                          (number-to-string neovm--lp-rename-counter)))))
            (setq mapping (cons (cons v new-name) mapping))))
        (funcall 'neovm--lp-apply-subst term mapping))))

  ;; Prove a goal against facts and rules with depth limit
  (fset 'neovm--lp-prove
    (lambda (goal facts rules subst depth)
      (if (<= depth 0) nil
        (let ((results nil))
          ;; Try facts first
          (dolist (fact facts)
            (let ((s (funcall 'neovm--lp-unify goal fact subst)))
              (unless (eq s 'fail)
                (setq results (cons s results)))))
          ;; Try rules
          (dolist (rule rules)
            (let* ((renamed (funcall 'neovm--lp-rename-vars rule))
                   (head (nth 1 renamed))
                   (body (cddr renamed))
                   (s (funcall 'neovm--lp-unify goal head subst)))
              (unless (eq s 'fail)
                (let ((body-results (funcall 'neovm--lp-prove-conj
                                             body facts rules s (1- depth))))
                  (setq results (append results body-results))))))
          results))))

  ;; Prove conjunction of goals
  (fset 'neovm--lp-prove-conj
    (lambda (goals facts rules subst depth)
      (if (null goals) (list subst)
        (let ((results nil))
          (dolist (s (funcall 'neovm--lp-prove (car goals) facts rules subst depth))
            (setq results (append results
                                  (funcall 'neovm--lp-prove-conj
                                           (cdr goals) facts rules s depth))))
          results))))

  (unwind-protect
      (let ((facts '((parent tom bob)
                     (parent tom liz)
                     (parent bob ann)
                     (parent bob pat)
                     (parent pat jim)))
            (rules '(;; grandparent(?x, ?z) :- parent(?x, ?y), parent(?y, ?z)
                     (rule (grandparent ?x ?z) (parent ?x ?y) (parent ?y ?z))
                     ;; ancestor(?x, ?y) :- parent(?x, ?y)
                     (rule (ancestor ?x ?y) (parent ?x ?y))
                     ;; ancestor(?x, ?z) :- parent(?x, ?y), ancestor(?y, ?z)
                     (rule (ancestor ?x ?z) (parent ?x ?y) (ancestor ?y ?z))
                     ;; sibling(?x, ?y) :- parent(?z, ?x), parent(?z, ?y)
                     (rule (sibling ?x ?y) (parent ?z ?x) (parent ?z ?y)))))
        (setq neovm--lp-rename-counter 0)
        (list
         ;; Grandparent query: grandparent(tom, ?who)?
         (mapcar (lambda (s) (funcall 'neovm--lp-apply-subst '?who s))
                 (funcall 'neovm--lp-prove '(grandparent tom ?who) facts rules nil 10))
         ;; Ancestor query: ancestor(tom, ?who)?
         (sort (mapcar (lambda (s)
                         (symbol-name (funcall 'neovm--lp-apply-subst '?who s)))
                       (funcall 'neovm--lp-prove '(ancestor tom ?who) facts rules nil 10))
               #'string<)
         ;; Sibling query: sibling(ann, ?who)?
         ;; Note: will include ann herself since parent(bob,ann) parent(bob,ann) matches
         (mapcar (lambda (s) (funcall 'neovm--lp-apply-subst '?who s))
                 (funcall 'neovm--lp-prove '(sibling ann ?who) facts rules nil 10))
         ;; Who is a grandparent? grandparent(?who, ?gc)
         (let ((results (funcall 'neovm--lp-prove '(grandparent ?who ?gc) facts rules nil 10)))
           (sort (seq-uniq (mapcar (lambda (s)
                                    (symbol-name (funcall 'neovm--lp-apply-subst '?who s)))
                                  results))
                 #'string<))))
    (makunbound 'neovm--lp-rename-counter)
    (fmakunbound 'neovm--lp-var-p)
    (fmakunbound 'neovm--lp-lookup)
    (fmakunbound 'neovm--lp-unify)
    (fmakunbound 'neovm--lp-apply-subst)
    (fmakunbound 'neovm--lp-collect-vars)
    (fmakunbound 'neovm--lp-rename-vars)
    (fmakunbound 'neovm--lp-prove)
    (fmakunbound 'neovm--lp-prove-conj)))"#;
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \"?\" 117 64)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// List operations in logic: append, member, reverse
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_logic_prog_list_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement classic Prolog list operations as logic rules.
    // Lists represented as nested conses: (cons a (cons b nil)) for [a, b]
    let form = r#"(progn
  (fset 'neovm--lp-var-p
    (lambda (x) (and (symbolp x) (string-prefix-p "?" (symbol-name x)))))

  (fset 'neovm--lp-lookup
    (lambda (var subst)
      (let ((binding (assq var subst)))
        (if binding
            (let ((val (cdr binding)))
              (if (funcall 'neovm--lp-var-p val)
                  (funcall 'neovm--lp-lookup val subst) val))
          var))))

  (fset 'neovm--lp-unify
    (lambda (t1 t2 subst)
      (cond
       ((eq subst 'fail) 'fail) ((equal t1 t2) subst)
       ((funcall 'neovm--lp-var-p t1)
        (let ((val (funcall 'neovm--lp-lookup t1 subst)))
          (if (funcall 'neovm--lp-var-p val) (cons (cons val t2) subst)
            (funcall 'neovm--lp-unify val t2 subst))))
       ((funcall 'neovm--lp-var-p t2) (funcall 'neovm--lp-unify t2 t1 subst))
       ((and (consp t1) (consp t2))
        (funcall 'neovm--lp-unify (cdr t1) (cdr t2)
                 (funcall 'neovm--lp-unify (car t1) (car t2) subst)))
       (t 'fail))))

  (fset 'neovm--lp-apply-subst
    (lambda (term subst)
      (cond
       ((funcall 'neovm--lp-var-p term)
        (let ((val (funcall 'neovm--lp-lookup term subst)))
          (if (funcall 'neovm--lp-var-p val) val
            (funcall 'neovm--lp-apply-subst val subst))))
       ((consp term)
        (cons (funcall 'neovm--lp-apply-subst (car term) subst)
              (funcall 'neovm--lp-apply-subst (cdr term) subst)))
       (t term))))

  ;; Manually implement append as a logic relation
  ;; append([], L, L).
  ;; append([H|T], L, [H|R]) :- append(T, L, R).
  (fset 'neovm--lp-append-query
    (lambda (a b result subst depth)
      (if (<= depth 0) nil
        (let ((results nil))
          ;; Base case: append(nil, L, L)
          (let ((s (funcall 'neovm--lp-unify a nil subst)))
            (unless (eq s 'fail)
              (let ((s2 (funcall 'neovm--lp-unify b result s)))
                (unless (eq s2 'fail)
                  (setq results (cons s2 results))))))
          ;; Recursive case: append([H|T], L, [H|R]) :- append(T, L, R)
          (when (or (consp a) (funcall 'neovm--lp-var-p a))
            (let* ((h (make-symbol "?h"))
                   (t-var (make-symbol "?t"))
                   (r-var (make-symbol "?r"))
                   ;; Unify a with (cons h t-var)
                   (s1 (funcall 'neovm--lp-unify a (cons h t-var) subst)))
              (unless (eq s1 'fail)
                ;; Unify result with (cons h r-var)
                (let ((s2 (funcall 'neovm--lp-unify result (cons h r-var) s1)))
                  (unless (eq s2 'fail)
                    ;; Recurse: append(t-var, b, r-var)
                    (let ((sub-results (funcall 'neovm--lp-append-query
                                                t-var b r-var s2 (1- depth))))
                      (setq results (append results sub-results))))))))
          results))))

  ;; Implement member as logic relation
  ;; member(X, [X|_]).
  ;; member(X, [_|T]) :- member(X, T).
  (fset 'neovm--lp-member-query
    (lambda (x lst subst depth)
      (if (<= depth 0) nil
        (let ((results nil))
          ;; Base: member(X, [X|_])
          (when (consp (funcall 'neovm--lp-apply-subst lst subst))
            (let* ((resolved-lst (funcall 'neovm--lp-apply-subst lst subst))
                   (s (funcall 'neovm--lp-unify x (car resolved-lst) subst)))
              (unless (eq s 'fail)
                (setq results (cons s results)))))
          ;; Recursive: member(X, [_|T]) :- member(X, T)
          (when (consp (funcall 'neovm--lp-apply-subst lst subst))
            (let* ((resolved-lst (funcall 'neovm--lp-apply-subst lst subst))
                   (tail (cdr resolved-lst)))
              (when (consp tail)
                (let ((sub-results (funcall 'neovm--lp-member-query
                                            x tail subst (1- depth))))
                  (setq results (append results sub-results))))))
          results))))

  (unwind-protect
      (list
       ;; append([1,2], [3,4], ?result)
       (let ((results (funcall 'neovm--lp-append-query
                                '(1 2) '(3 4) (make-symbol "?r") nil 10)))
         (mapcar (lambda (s)
                   (funcall 'neovm--lp-apply-subst (make-symbol "?r") s))
                 results))
       ;; append(nil, [a,b], ?result) = [a,b]
       (let* ((r-var (make-symbol "?r"))
              (results (funcall 'neovm--lp-append-query nil '(a b) r-var nil 10)))
         (when results
           (funcall 'neovm--lp-apply-subst r-var (car results))))
       ;; append([x], [y,z], ?result) = [x,y,z]
       (let* ((r-var (make-symbol "?r"))
              (results (funcall 'neovm--lp-append-query '(x) '(y z) r-var nil 10)))
         (when results
           (funcall 'neovm--lp-apply-subst r-var (car results))))
       ;; member(2, [1,2,3])
       (let ((results (funcall 'neovm--lp-member-query 2 '(1 2 3) nil 10)))
         (not (null results)))
       ;; member(5, [1,2,3])
       (let ((results (funcall 'neovm--lp-member-query 5 '(1 2 3) nil 10)))
         (not (null results)))
       ;; member(a, [a,b,c]) -- first element
       (let ((results (funcall 'neovm--lp-member-query 'a '(a b c) nil 10)))
         (not (null results)))
       ;; member(c, [a,b,c]) -- last element
       (let ((results (funcall 'neovm--lp-member-query 'c '(a b c) nil 10)))
         (not (null results))))
    (fmakunbound 'neovm--lp-var-p)
    (fmakunbound 'neovm--lp-lookup)
    (fmakunbound 'neovm--lp-unify)
    (fmakunbound 'neovm--lp-apply-subst)
    (fmakunbound 'neovm--lp-append-query)
    (fmakunbound 'neovm--lp-member-query)))"#;
    let expect = expect_test::expect![[r#""OK ((\\?r) (a b) (x y z) t nil t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Arithmetic constraints and evaluation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_logic_prog_arithmetic_constraints() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Extend the logic engine with arithmetic evaluation and constraints
    let form = r#"(progn
  (fset 'neovm--lp-var-p
    (lambda (x) (and (symbolp x) (string-prefix-p "?" (symbol-name x)))))

  (fset 'neovm--lp-lookup
    (lambda (var subst)
      (let ((binding (assq var subst)))
        (if binding
            (let ((val (cdr binding)))
              (if (funcall 'neovm--lp-var-p val)
                  (funcall 'neovm--lp-lookup val subst) val))
          var))))

  (fset 'neovm--lp-unify
    (lambda (t1 t2 subst)
      (cond
       ((eq subst 'fail) 'fail) ((equal t1 t2) subst)
       ((funcall 'neovm--lp-var-p t1)
        (let ((val (funcall 'neovm--lp-lookup t1 subst)))
          (if (funcall 'neovm--lp-var-p val) (cons (cons val t2) subst)
            (funcall 'neovm--lp-unify val t2 subst))))
       ((funcall 'neovm--lp-var-p t2) (funcall 'neovm--lp-unify t2 t1 subst))
       ((and (consp t1) (consp t2))
        (funcall 'neovm--lp-unify (cdr t1) (cdr t2)
                 (funcall 'neovm--lp-unify (car t1) (car t2) subst)))
       (t 'fail))))

  (fset 'neovm--lp-apply-subst
    (lambda (term subst)
      (cond
       ((funcall 'neovm--lp-var-p term)
        (let ((val (funcall 'neovm--lp-lookup term subst)))
          (if (funcall 'neovm--lp-var-p val) val
            (funcall 'neovm--lp-apply-subst val subst))))
       ((consp term)
        (cons (funcall 'neovm--lp-apply-subst (car term) subst)
              (funcall 'neovm--lp-apply-subst (cdr term) subst)))
       (t term))))

  ;; Evaluate an arithmetic expression, resolving variables from subst
  (fset 'neovm--lp-arith-eval
    (lambda (expr subst)
      (cond
       ((numberp expr) expr)
       ((funcall 'neovm--lp-var-p expr)
        (let ((val (funcall 'neovm--lp-apply-subst expr subst)))
          (if (numberp val) val
            (signal 'error (list "unbound in arith" expr)))))
       ((consp expr)
        (let ((op (car expr))
              (a (funcall 'neovm--lp-arith-eval (nth 1 expr) subst))
              (b (when (nth 2 expr)
                   (funcall 'neovm--lp-arith-eval (nth 2 expr) subst))))
          (cond
           ((eq op '+) (+ a b))
           ((eq op '-) (if b (- a b) (- a)))
           ((eq op '*) (* a b))
           ((eq op '/) (/ a b))
           ((eq op 'mod) (mod a b))
           (t (signal 'error (list "unknown arith op" op))))))
       (t (signal 'error (list "invalid arith expr" expr))))))

  ;; Solve: (is ?var EXPR) -- evaluate EXPR and bind ?var
  ;; Returns updated subst or 'fail
  (fset 'neovm--lp-is
    (lambda (var expr subst)
      (condition-case nil
          (let ((val (funcall 'neovm--lp-arith-eval expr subst)))
            (funcall 'neovm--lp-unify var val subst))
        (error 'fail))))

  ;; Check arithmetic constraint
  (fset 'neovm--lp-check
    (lambda (op a b subst)
      (condition-case nil
          (let ((va (funcall 'neovm--lp-arith-eval a subst))
                (vb (funcall 'neovm--lp-arith-eval b subst)))
            (cond
             ((eq op '>) (if (> va vb) subst 'fail))
             ((eq op '<) (if (< va vb) subst 'fail))
             ((eq op '>=) (if (>= va vb) subst 'fail))
             ((eq op '<=) (if (<= va vb) subst 'fail))
             ((eq op '=:=) (if (= va vb) subst 'fail))
             ((eq op '=\=) (if (/= va vb) subst 'fail))
             (t 'fail)))
        (error 'fail))))

  ;; Generate-and-test: find numbers 1..N satisfying constraints
  (fset 'neovm--lp-range-search
    (lambda (var lo hi constraints subst)
      (let ((results nil) (i lo))
        (while (<= i hi)
          (let ((s (funcall 'neovm--lp-unify var i subst)))
            (unless (eq s 'fail)
              (let ((ok t))
                (dolist (c constraints)
                  (when ok
                    (let ((check-result
                            (funcall 'neovm--lp-check
                                     (nth 0 c) (nth 1 c) (nth 2 c) s)))
                      (when (eq check-result 'fail)
                        (setq ok nil)))))
                (when ok
                  (setq results (cons s results))))))
          (setq i (1+ i)))
        (nreverse results))))

  (unwind-protect
      (list
       ;; is: ?result = 3 + 4 * 2
       (let ((s (funcall 'neovm--lp-is '?result '(+ 3 (* 4 2)) nil)))
         (funcall 'neovm--lp-apply-subst '?result s))
       ;; is with bound variables: ?x=5, ?result = ?x * ?x + 1
       (let* ((s1 '((?x . 5)))
              (s2 (funcall 'neovm--lp-is '?result '(+ (* ?x ?x) 1) s1)))
         (funcall 'neovm--lp-apply-subst '?result s2))
       ;; Check: 5 > 3
       (not (eq (funcall 'neovm--lp-check '> 5 3 nil) 'fail))
       ;; Check: 3 > 5 (should fail)
       (eq (funcall 'neovm--lp-check '> 3 5 nil) 'fail)
       ;; Check with variables: ?x=10, ?x > 5
       (not (eq (funcall 'neovm--lp-check '> '?x 5 '((?x . 10))) 'fail))
       ;; Range search: find all x in 1..20 where x > 5 and x < 10
       (mapcar (lambda (s) (funcall 'neovm--lp-apply-subst '?x s))
               (funcall 'neovm--lp-range-search '?x 1 20
                        '((> ?x 5) (< ?x 10)) nil))
       ;; Range search: find all x in 1..20 where x mod 3 = 0 and x mod 5 = 0
       (mapcar (lambda (s) (funcall 'neovm--lp-apply-subst '?x s))
               (funcall 'neovm--lp-range-search '?x 1 20
                        '((=:= (mod ?x 3) 0) (=:= (mod ?x 5) 0)) nil))
       ;; Pythagorean triples: find a,b in 1..10 where a^2 + b^2 = 25
       (let ((results nil))
         (dolist (sa (funcall 'neovm--lp-range-search '?a 1 10 nil nil))
           (dolist (sb (funcall 'neovm--lp-range-search '?b 1 10
                                '((=:= (+ (* ?a ?a) (* ?b ?b)) 25))
                                sa))
             (setq results (cons (list (funcall 'neovm--lp-apply-subst '?a sb)
                                       (funcall 'neovm--lp-apply-subst '?b sb))
                                 results))))
         (sort results (lambda (a b) (< (car a) (car b))))))
    (fmakunbound 'neovm--lp-var-p)
    (fmakunbound 'neovm--lp-lookup)
    (fmakunbound 'neovm--lp-unify)
    (fmakunbound 'neovm--lp-apply-subst)
    (fmakunbound 'neovm--lp-arith-eval)
    (fmakunbound 'neovm--lp-is)
    (fmakunbound 'neovm--lp-check)
    (fmakunbound 'neovm--lp-range-search)))"#;
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \"?\" 111 42)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: path finding using logic programming
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_logic_prog_path_finding() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Find paths in a directed graph using logic programming style
    let form = r#"(progn
  ;; Edge database as alist: (from . ((to1 cost1) (to2 cost2) ...))
  (fset 'neovm--lp-get-edges
    (lambda (node graph)
      (let ((entry (assq node graph)))
        (if entry (cdr entry) nil))))

  ;; Find all paths from START to END with cycle detection
  ;; Returns list of (path . total-cost) pairs
  (fset 'neovm--lp-find-paths
    (lambda (start end graph visited max-depth)
      (cond
       ((<= max-depth 0) nil)
       ((eq start end)
        (list (cons (list end) 0)))
       ((memq start visited) nil)
       (t
        (let ((results nil)
              (edges (funcall 'neovm--lp-get-edges start graph))
              (new-visited (cons start visited)))
          (dolist (edge edges)
            (let* ((next (car edge))
                   (cost (cadr edge))
                   (sub-paths (funcall 'neovm--lp-find-paths
                                       next end graph
                                       new-visited (1- max-depth))))
              (dolist (sp sub-paths)
                (setq results
                      (cons (cons (cons start (car sp))
                                  (+ cost (cdr sp)))
                            results)))))
          results)))))

  ;; Find shortest path
  (fset 'neovm--lp-shortest-path
    (lambda (start end graph)
      (let ((paths (funcall 'neovm--lp-find-paths start end graph nil 10))
            (best nil)
            (best-cost nil))
        (dolist (p paths)
          (when (or (null best-cost) (< (cdr p) best-cost))
            (setq best (car p))
            (setq best-cost (cdr p))))
        (if best (list best best-cost) nil))))

  (unwind-protect
      (let ((graph '((a (b 1) (c 4))
                     (b (c 2) (d 5))
                     (c (d 1))
                     (d (e 3))
                     (e))))
        (list
         ;; All paths from a to d
         (let ((paths (funcall 'neovm--lp-find-paths 'a 'd graph nil 10)))
           (sort (mapcar (lambda (p) (list (car p) (cdr p))) paths)
                 (lambda (a b) (< (cadr a) (cadr b)))))
         ;; Shortest path from a to d
         (funcall 'neovm--lp-shortest-path 'a 'd graph)
         ;; All paths from a to e
         (let ((paths (funcall 'neovm--lp-find-paths 'a 'e graph nil 10)))
           (sort (mapcar (lambda (p) (list (car p) (cdr p))) paths)
                 (lambda (a b) (< (cadr a) (cadr b)))))
         ;; Shortest a to e
         (funcall 'neovm--lp-shortest-path 'a 'e graph)
         ;; No path from e to a (directed graph)
         (funcall 'neovm--lp-find-paths 'e 'a graph nil 10)
         ;; Direct edge
         (funcall 'neovm--lp-shortest-path 'a 'b graph)
         ;; Reachability: list all nodes reachable from a
         (let ((reachable nil))
           (dolist (target '(a b c d e))
             (when (funcall 'neovm--lp-find-paths 'a target graph nil 10)
               (setq reachable (cons target reachable))))
           (sort reachable (lambda (a b) (string< (symbol-name a) (symbol-name b)))))))
    (fmakunbound 'neovm--lp-get-edges)
    (fmakunbound 'neovm--lp-find-paths)
    (fmakunbound 'neovm--lp-shortest-path)))"#;
    let expect = expect_test::expect![[
        r#""OK ((((a b c d) 4) ((a c d) 5) ((a b d) 6)) ((a b c d) 4) (((a b c d e) 7) ((a c d e) 8) ((a b d e) 9)) ((a b c d e) 7) nil ((a b) 1) (a b c d e))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: constraint satisfaction with generate-and-test
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_logic_prog_constraint_satisfaction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Solve constraint satisfaction problems using logic programming approach
    let form = r#"(progn
  ;; Solve: assign distinct digits to variables satisfying constraints
  ;; Simple version: find all permutations of values satisfying a test

  ;; Remove an element from a list
  (fset 'neovm--lp-remove-one
    (lambda (x lst)
      (cond
       ((null lst) nil)
       ((equal x (car lst)) (cdr lst))
       (t (cons (car lst) (funcall 'neovm--lp-remove-one x (cdr lst)))))))

  ;; Generate permutations of a given length from a list
  (fset 'neovm--lp-permutations
    (lambda (lst n)
      (if (= n 0) (list nil)
        (let ((results nil))
          (dolist (x lst)
            (dolist (rest (funcall 'neovm--lp-permutations
                                   (funcall 'neovm--lp-remove-one x lst) (1- n)))
              (setq results (cons (cons x rest) results))))
          results))))

  ;; Solve SEND + MORE = MONEY (simplified: find a,b,c where a+b=c, all distinct)
  ;; Actually solve: find x,y,z in 1..9 where x+y=z and all different
  (fset 'neovm--lp-solve-sum
    (lambda (values)
      (let ((results nil))
        (dolist (perm (funcall 'neovm--lp-permutations values 3))
          (let ((x (nth 0 perm))
                (y (nth 1 perm))
                (z (nth 2 perm)))
            (when (= (+ x y) z)
              (setq results (cons (list x y z) results)))))
        (sort results (lambda (a b)
                        (or (< (car a) (car b))
                            (and (= (car a) (car b))
                                 (< (cadr a) (cadr b)))))))))

  ;; N-queens constraint check
  (fset 'neovm--lp-queens-safe-p
    (lambda (queens)
      "Check if a list of (row . col) queens placement is valid."
      (let ((safe t) (i 0))
        (while (and safe (< i (length queens)))
          (let ((q1 (nth i queens))
                (j (1+ i)))
            (while (and safe (< j (length queens)))
              (let ((q2 (nth j queens)))
                (when (or (= (car q1) (car q2))     ;; same row
                          (= (cdr q1) (cdr q2))     ;; same col
                          (= (abs (- (car q1) (car q2)))
                             (abs (- (cdr q1) (cdr q2))))) ;; same diagonal
                  (setq safe nil)))
              (setq j (1+ j))))
          (setq i (1+ i)))
        safe)))

  ;; Solve N-queens for small N using backtracking
  (fset 'neovm--lp-nqueens
    (lambda (n)
      (let ((solutions nil))
        (fset 'neovm--lp-nq-solve
          (lambda (col placed)
            (if (= col n)
                (when (funcall 'neovm--lp-queens-safe-p placed)
                  (setq solutions (cons (copy-sequence placed) solutions)))
              (let ((row 0))
                (while (< row n)
                  (let ((new-placed (append placed (list (cons row col)))))
                    (when (funcall 'neovm--lp-queens-safe-p new-placed)
                      (funcall 'neovm--lp-nq-solve (1+ col) new-placed)))
                  (setq row (1+ row)))))))
        (funcall 'neovm--lp-nq-solve 0 nil)
        solutions)))

  (unwind-protect
      (list
       ;; Find x,y,z in {1..6} where x+y=z, all distinct
       (funcall 'neovm--lp-solve-sum '(1 2 3 4 5 6))
       ;; Permutations count
       (length (funcall 'neovm--lp-permutations '(1 2 3) 2))
       (length (funcall 'neovm--lp-permutations '(1 2 3) 3))
       ;; N-queens: count solutions for N=4
       (length (funcall 'neovm--lp-nqueens 4))
       ;; N-queens: count solutions for N=5
       (length (funcall 'neovm--lp-nqueens 5))
       ;; Queens safety check
       (funcall 'neovm--lp-queens-safe-p '((0 . 0) (1 . 1)))  ;; diagonal conflict
       (funcall 'neovm--lp-queens-safe-p '((0 . 0) (2 . 1)))  ;; safe
       (funcall 'neovm--lp-queens-safe-p '((0 . 0) (0 . 1)))  ;; same row
       ;; A valid 4-queens solution exists
       (not (null (funcall 'neovm--lp-nqueens 4))))
    (fmakunbound 'neovm--lp-remove-one)
    (fmakunbound 'neovm--lp-permutations)
    (fmakunbound 'neovm--lp-solve-sum)
    (fmakunbound 'neovm--lp-queens-safe-p)
    (fmakunbound 'neovm--lp-nqueens)
    (fmakunbound 'neovm--lp-nq-solve)))"#;
    let expect = expect_test::expect![[
        r#""OK (((1 2 3) (1 3 4) (1 4 5) (1 5 6) (2 1 3) (2 3 5) (2 4 6) (3 1 4) (3 2 5) (4 1 5) (4 2 6) (5 1 6)) 6 6 2 10 nil t nil t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
