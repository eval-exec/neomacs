//! Oracle parity tests for deductive database (Datalog-style) operations:
//! fact assertion, rule definition with body conjunctions, forward chaining
//! inference, stratified negation, recursive rules (transitive closure),
//! magic sets optimization, query evaluation with variable binding.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Fact assertion and basic query evaluation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_deductive_db_fact_assertion_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Database: list of (relation . args) tuples
  (fset 'neovm--ddb-make-db (lambda () nil))

  (fset 'neovm--ddb-assert-fact
    (lambda (db relation &rest args)
      (let ((fact (cons relation args)))
        (if (member fact db) db (cons fact db)))))

  ;; Check if symbol is variable (starts with ?)
  (fset 'neovm--ddb-var-p
    (lambda (x) (and (symbolp x) (string-prefix-p "?" (symbol-name x)))))

  ;; Unify pattern against fact, returning extended bindings or nil
  (fset 'neovm--ddb-unify
    (lambda (pattern fact bindings)
      (cond
       ((and (null pattern) (null fact)) bindings)
       ((or (null pattern) (null fact)) nil)
       (t (let ((p (car pattern)) (f (car fact)))
            (cond
             ((funcall 'neovm--ddb-var-p p)
              (let ((bound (assq p bindings)))
                (if bound
                    (if (equal (cdr bound) f)
                        (funcall 'neovm--ddb-unify (cdr pattern) (cdr fact) bindings)
                      nil)
                  (funcall 'neovm--ddb-unify (cdr pattern) (cdr fact)
                           (cons (cons p f) bindings)))))
             ((equal p f)
              (funcall 'neovm--ddb-unify (cdr pattern) (cdr fact) bindings))
             (t nil)))))))

  ;; Query all facts matching a pattern
  (fset 'neovm--ddb-query
    (lambda (db pattern)
      (let ((results nil))
        (dolist (fact db)
          (let ((b (funcall 'neovm--ddb-unify pattern fact nil)))
            (when b (setq results (cons b results)))))
        (nreverse results))))

  ;; Extract variable value from bindings
  (fset 'neovm--ddb-binding-val
    (lambda (bindings var)
      (let ((p (assq var bindings))) (if p (cdr p) nil))))

  (unwind-protect
      (let ((db (funcall 'neovm--ddb-make-db)))
        ;; Assert facts about a company
        (setq db (funcall 'neovm--ddb-assert-fact db 'employee 'alice 'engineering 90000))
        (setq db (funcall 'neovm--ddb-assert-fact db 'employee 'bob 'engineering 85000))
        (setq db (funcall 'neovm--ddb-assert-fact db 'employee 'charlie 'marketing 70000))
        (setq db (funcall 'neovm--ddb-assert-fact db 'employee 'dave 'sales 60000))
        (setq db (funcall 'neovm--ddb-assert-fact db 'employee 'eve 'engineering 95000))
        (setq db (funcall 'neovm--ddb-assert-fact db 'manages 'alice 'bob))
        (setq db (funcall 'neovm--ddb-assert-fact db 'manages 'alice 'eve))
        (setq db (funcall 'neovm--ddb-assert-fact db 'manages 'charlie 'dave))
        ;; Duplicate assertion should be idempotent
        (setq db (funcall 'neovm--ddb-assert-fact db 'employee 'alice 'engineering 90000))

        (list
         ;; Ground query
         (not (null (funcall 'neovm--ddb-query db '(employee alice engineering 90000))))
         ;; Nonexistent fact
         (null (funcall 'neovm--ddb-query db '(employee alice marketing 90000)))
         ;; Variable query: all engineers
         (sort (mapcar (lambda (b) (funcall 'neovm--ddb-binding-val b '?name))
                       (funcall 'neovm--ddb-query db '(employee ?name engineering ?sal)))
               (lambda (a b) (string< (symbol-name a) (symbol-name b))))
         ;; Two-variable query: all name-salary pairs in engineering
         (sort (mapcar (lambda (b) (cons (funcall 'neovm--ddb-binding-val b '?name)
                                         (funcall 'neovm--ddb-binding-val b '?sal)))
                       (funcall 'neovm--ddb-query db '(employee ?name engineering ?sal)))
               (lambda (a b) (string< (symbol-name (car a)) (symbol-name (car b)))))
         ;; Who does alice manage?
         (sort (mapcar (lambda (b) (funcall 'neovm--ddb-binding-val b '?sub))
                       (funcall 'neovm--ddb-query db '(manages alice ?sub)))
               (lambda (a b) (string< (symbol-name a) (symbol-name b))))
         ;; Count total facts (no duplicates)
         (length db)))
    (fmakunbound 'neovm--ddb-make-db)
    (fmakunbound 'neovm--ddb-assert-fact)
    (fmakunbound 'neovm--ddb-var-p)
    (fmakunbound 'neovm--ddb-unify)
    (fmakunbound 'neovm--ddb-query)
    (fmakunbound 'neovm--ddb-binding-val)))"#;
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \"?\" 68 73)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Rule definition with body conjunctions and forward chaining
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_deductive_db_rules_forward_chaining() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'cl-lib)
  (fset 'neovm--ddb2-var-p
    (lambda (x) (and (symbolp x) (string-prefix-p "?" (symbol-name x)))))

  (fset 'neovm--ddb2-unify
    (lambda (pat fact binds)
      (if (null pat) (if (null fact) binds nil)
        (if (null fact) nil
          (let ((p (car pat)) (f (car fact)))
            (cond
             ((funcall 'neovm--ddb2-var-p p)
              (let ((b (assq p binds)))
                (if b (if (equal (cdr b) f)
                          (funcall 'neovm--ddb2-unify (cdr pat) (cdr fact) binds) nil)
                  (funcall 'neovm--ddb2-unify (cdr pat) (cdr fact)
                           (cons (cons p f) binds)))))
             ((equal p f) (funcall 'neovm--ddb2-unify (cdr pat) (cdr fact) binds))
             (t nil)))))))

  (fset 'neovm--ddb2-query-1
    (lambda (db pat)
      (let ((res nil))
        (dolist (fact db)
          (let ((b (funcall 'neovm--ddb2-unify pat fact nil)))
            (when b (setq res (cons b res)))))
        (nreverse res))))

  (fset 'neovm--ddb2-subst
    (lambda (pat binds)
      (mapcar (lambda (x) (if (funcall 'neovm--ddb2-var-p x)
                               (let ((b (assq x binds))) (if b (cdr b) x)) x))
              pat)))

  ;; Evaluate conjunctive body against db
  (fset 'neovm--ddb2-eval-body
    (lambda (db body bindings-list)
      (if (null body) bindings-list
        (let ((pat (car body)) (new-bl nil))
          (dolist (binds bindings-list)
            (let* ((inst (funcall 'neovm--ddb2-subst pat binds))
                   (matches (funcall 'neovm--ddb2-query-1 db inst)))
              (dolist (m matches)
                (let ((merged binds) (ok t))
                  (dolist (pair m)
                    (when ok
                      (let ((existing (assq (car pair) merged)))
                        (if existing
                            (unless (equal (cdr existing) (cdr pair))
                              (setq ok nil))
                          (setq merged (cons pair merged))))))
                  (when ok (setq new-bl (cons merged new-bl)))))))
          (funcall 'neovm--ddb2-eval-body db (cdr body) (nreverse new-bl))))))

  ;; Apply a single rule, return new derived facts
  (fset 'neovm--ddb2-apply-rule
    (lambda (db rule)
      (let* ((head (car rule)) (body (cdr rule))
             (bl (funcall 'neovm--ddb2-eval-body db body (list nil)))
             (new-facts nil))
        (dolist (b bl)
          (let ((fact (funcall 'neovm--ddb2-subst head b)))
            (unless (cl-some (lambda (x) (funcall 'neovm--ddb2-var-p x)) fact)
              (unless (member fact new-facts)
                (setq new-facts (cons fact new-facts))))))
        (nreverse new-facts))))

  ;; Forward chaining: apply all rules until fixpoint
  (fset 'neovm--ddb2-forward-chain
    (lambda (db rules max-iters)
      (let ((changed t) (iters 0))
        (while (and changed (< iters max-iters))
          (setq changed nil iters (1+ iters))
          (dolist (rule rules)
            (dolist (f (funcall 'neovm--ddb2-apply-rule db rule))
              (unless (member f db)
                (setq db (cons f db))
                (setq changed t)))))
        (list 'db db 'iterations iters))))

  (unwind-protect
      (let* ((db '((parent tom bob) (parent tom liz) (parent bob ann)
                   (parent bob pat) (parent pat jim) (parent liz joe)))
             ;; Rules:
             ;; grandparent(?x,?z) :- parent(?x,?y), parent(?y,?z)
             ;; sibling(?x,?y) :- parent(?p,?x), parent(?p,?y)  [x!=y handled post-hoc]
             (rules '(((grandparent ?x ?z) (parent ?x ?y) (parent ?y ?z))
                      ((sibling ?x ?y) (parent ?p ?x) (parent ?p ?y))))
             (result (funcall 'neovm--ddb2-forward-chain db rules 10))
             (full-db (cadr result)))
        ;; Extract derived relations
        (let ((gp nil) (sib nil))
          (dolist (f full-db)
            (when (eq (car f) 'grandparent) (setq gp (cons f gp)))
            (when (and (eq (car f) 'sibling) (not (eq (nth 1 f) (nth 2 f))))
              (setq sib (cons f sib))))
          (list
           ;; Grandparent facts (sorted)
           (sort gp (lambda (a b) (string< (format "%S" a) (format "%S" b))))
           ;; Sibling facts (sorted)
           (sort sib (lambda (a b) (string< (format "%S" a) (format "%S" b))))
           ;; Iteration count
           (caddr result))))
    (fmakunbound 'neovm--ddb2-var-p)
    (fmakunbound 'neovm--ddb2-unify)
    (fmakunbound 'neovm--ddb2-query-1)
    (fmakunbound 'neovm--ddb2-subst)
    (fmakunbound 'neovm--ddb2-eval-body)
    (fmakunbound 'neovm--ddb2-apply-rule)
    (fmakunbound 'neovm--ddb2-forward-chain)))"#;
    let expect = expect_test::expect![[r#""OK (nil nil iterations)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Recursive rules: transitive closure
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_deductive_db_recursive_transitive_closure() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'cl-lib)
  (fset 'neovm--ddb3-var-p
    (lambda (x) (and (symbolp x) (string-prefix-p "?" (symbol-name x)))))

  (fset 'neovm--ddb3-unify
    (lambda (pat fact binds)
      (if (null pat) (if (null fact) binds nil)
        (if (null fact) nil
          (let ((p (car pat)) (f (car fact)))
            (cond
             ((funcall 'neovm--ddb3-var-p p)
              (let ((b (assq p binds)))
                (if b (if (equal (cdr b) f)
                          (funcall 'neovm--ddb3-unify (cdr pat) (cdr fact) binds) nil)
                  (funcall 'neovm--ddb3-unify (cdr pat) (cdr fact)
                           (cons (cons p f) binds)))))
             ((equal p f) (funcall 'neovm--ddb3-unify (cdr pat) (cdr fact) binds))
             (t nil)))))))

  (fset 'neovm--ddb3-query-1
    (lambda (db pat)
      (let ((res nil))
        (dolist (fact db) (let ((b (funcall 'neovm--ddb3-unify pat fact nil)))
                            (when b (setq res (cons b res)))))
        (nreverse res))))

  (fset 'neovm--ddb3-subst
    (lambda (pat binds)
      (mapcar (lambda (x) (if (funcall 'neovm--ddb3-var-p x)
                               (let ((b (assq x binds))) (if b (cdr b) x)) x))
              pat)))

  (fset 'neovm--ddb3-eval-body
    (lambda (db body bl)
      (if (null body) bl
        (let ((pat (car body)) (nb nil))
          (dolist (binds bl)
            (let* ((inst (funcall 'neovm--ddb3-subst pat binds))
                   (ms (funcall 'neovm--ddb3-query-1 db inst)))
              (dolist (m ms)
                (let ((mg binds) (ok t))
                  (dolist (p m)
                    (when ok
                      (let ((ex (assq (car p) mg)))
                        (if ex (unless (equal (cdr ex) (cdr p)) (setq ok nil))
                          (setq mg (cons p mg))))))
                  (when ok (setq nb (cons mg nb)))))))
          (funcall 'neovm--ddb3-eval-body db (cdr body) (nreverse nb))))))

  (fset 'neovm--ddb3-apply-rule
    (lambda (db rule)
      (let* ((head (car rule)) (body (cdr rule))
             (bl (funcall 'neovm--ddb3-eval-body db body (list nil)))
             (nf nil))
        (dolist (b bl)
          (let ((fact (funcall 'neovm--ddb3-subst head b)))
            (unless (cl-some (lambda (x) (funcall 'neovm--ddb3-var-p x)) fact)
              (unless (member fact nf) (setq nf (cons fact nf))))))
        (nreverse nf))))

  (fset 'neovm--ddb3-fixpoint
    (lambda (db rules max-iter)
      (let ((changed t) (iters 0))
        (while (and changed (< iters max-iter))
          (setq changed nil iters (1+ iters))
          (dolist (rule rules)
            (dolist (f (funcall 'neovm--ddb3-apply-rule db rule))
              (unless (member f db) (setq db (cons f db)) (setq changed t)))))
        db)))

  (unwind-protect
      (let* (;; Directed graph with cycles
             (db '((edge a b) (edge b c) (edge c d) (edge d e)
                   (edge e b) ;; cycle b->c->d->e->b
                   (edge a f) (edge f g)))
             ;; reachable(X,Y) :- edge(X,Y).
             ;; reachable(X,Z) :- edge(X,Y), reachable(Y,Z).
             (rules '(((reachable ?x ?y) (edge ?x ?y))
                      ((reachable ?x ?z) (edge ?x ?y) (reachable ?y ?z))))
             (result-db (funcall 'neovm--ddb3-fixpoint db rules 20)))
        (let ((reachable nil))
          (dolist (f result-db)
            (when (eq (car f) 'reachable)
              (setq reachable (cons f reachable))))
          (list
           ;; Total reachable pairs
           (length reachable)
           ;; Is g reachable from a?
           (not (null (member '(reachable a g) reachable)))
           ;; Cycle: b can reach itself (b->c->d->e->b)
           (not (null (member '(reachable b b) reachable)))
           ;; c can reach b (via cycle)
           (not (null (member '(reachable c b) reachable)))
           ;; g cannot reach a (directed)
           (null (member '(reachable g a) reachable))
           ;; All nodes reachable from a (sorted)
           (sort (let ((targets nil))
                   (dolist (r reachable)
                     (when (eq (nth 1 r) 'a)
                       (unless (memq (nth 2 r) targets)
                         (setq targets (cons (nth 2 r) targets)))))
                   (mapcar #'symbol-name targets))
                 #'string<))))
    (fmakunbound 'neovm--ddb3-var-p)
    (fmakunbound 'neovm--ddb3-unify)
    (fmakunbound 'neovm--ddb3-query-1)
    (fmakunbound 'neovm--ddb3-subst)
    (fmakunbound 'neovm--ddb3-eval-body)
    (fmakunbound 'neovm--ddb3-apply-rule)
    (fmakunbound 'neovm--ddb3-fixpoint)))"#;
    let expect = expect_test::expect![[r#""OK (0 nil nil nil t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Stratified negation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_deductive_db_stratified_negation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--ddb4-var-p
    (lambda (x) (and (symbolp x) (string-prefix-p "?" (symbol-name x)))))

  (fset 'neovm--ddb4-unify
    (lambda (pat fact binds)
      (if (null pat) (if (null fact) binds nil)
        (if (null fact) nil
          (let ((p (car pat)) (f (car fact)))
            (cond
             ((funcall 'neovm--ddb4-var-p p)
              (let ((b (assq p binds)))
                (if b (if (equal (cdr b) f)
                          (funcall 'neovm--ddb4-unify (cdr pat) (cdr fact) binds) nil)
                  (funcall 'neovm--ddb4-unify (cdr pat) (cdr fact)
                           (cons (cons p f) binds)))))
             ((equal p f) (funcall 'neovm--ddb4-unify (cdr pat) (cdr fact) binds))
             (t nil)))))))

  (fset 'neovm--ddb4-query-1
    (lambda (db pat)
      (let ((res nil))
        (dolist (fact db) (let ((b (funcall 'neovm--ddb4-unify pat fact nil)))
                            (when b (setq res (cons b res)))))
        (nreverse res))))

  (fset 'neovm--ddb4-subst
    (lambda (pat binds)
      (mapcar (lambda (x) (if (funcall 'neovm--ddb4-var-p x)
                               (let ((b (assq x binds))) (if b (cdr b) x)) x))
              pat)))

  ;; Check if pattern has any match in db (for negation check)
  (fset 'neovm--ddb4-exists-p
    (lambda (db pat)
      (not (null (funcall 'neovm--ddb4-query-1 db pat)))))

  (unwind-protect
      (let ((db '((student alice) (student bob) (student charlie)
                  (student dave) (student eve)
                  (course math) (course physics) (course art)
                  (enrolled alice math) (enrolled alice physics)
                  (enrolled bob math) (enrolled bob art)
                  (enrolled charlie physics)
                  (enrolled dave art)
                  ;; eve not enrolled in anything
                  (passed alice math) (passed bob math)
                  (passed charlie physics))))
        (list
         ;; Stratum 0: base facts only
         ;; Students enrolled in math
         (sort (let ((res nil))
                 (dolist (f db) (when (and (eq (car f) 'enrolled) (eq (nth 2 f) 'math))
                                  (setq res (cons (nth 1 f) res))))
                 (mapcar #'symbol-name res))
               #'string<)

         ;; Stratum 1: with negation on base facts
         ;; Students enrolled in math who have NOT passed math
         (sort (let ((res nil))
                 (dolist (f db)
                   (when (and (eq (car f) 'enrolled) (eq (nth 2 f) 'math))
                     (let ((s (nth 1 f)))
                       (unless (funcall 'neovm--ddb4-exists-p db
                                        (list 'passed s 'math))
                         (setq res (cons s res))))))
                 (mapcar #'symbol-name res))
               #'string<)

         ;; Students not enrolled in any course
         (let ((enrolled-set nil))
           (dolist (f db) (when (eq (car f) 'enrolled)
                            (unless (memq (nth 1 f) enrolled-set)
                              (setq enrolled-set (cons (nth 1 f) enrolled-set)))))
           (sort (let ((res nil))
                   (dolist (f db) (when (eq (car f) 'student)
                                    (unless (memq (nth 1 f) enrolled-set)
                                      (setq res (cons (nth 1 f) res)))))
                   (mapcar #'symbol-name res))
                 #'string<))

         ;; Stratum 2: courses with ALL enrolled students having passed
         ;; (complement: no enrolled student who hasn't passed)
         (sort (let ((res nil))
                 (dolist (cf db)
                   (when (eq (car cf) 'course)
                     (let ((course-name (nth 1 cf))
                           (all-passed t)
                           (has-students nil))
                       (dolist (ef db)
                         (when (and (eq (car ef) 'enrolled) (eq (nth 2 ef) course-name))
                           (setq has-students t)
                           (unless (funcall 'neovm--ddb4-exists-p db
                                            (list 'passed (nth 1 ef) course-name))
                             (setq all-passed nil))))
                       (when (and has-students all-passed)
                         (setq res (cons course-name res))))))
                 (mapcar #'symbol-name res))
               #'string<)

         ;; Courses where alice is enrolled but bob is NOT
         (sort (let ((res nil))
                 (dolist (f db)
                   (when (and (eq (car f) 'enrolled) (eq (nth 1 f) 'alice))
                     (unless (funcall 'neovm--ddb4-exists-p db
                                      (list 'enrolled 'bob (nth 2 f)))
                       (setq res (cons (nth 2 f) res)))))
                 (mapcar #'symbol-name res))
               #'string<)))
    (fmakunbound 'neovm--ddb4-var-p)
    (fmakunbound 'neovm--ddb4-unify)
    (fmakunbound 'neovm--ddb4-query-1)
    (fmakunbound 'neovm--ddb4-subst)
    (fmakunbound 'neovm--ddb4-exists-p)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"alice\" \"bob\") (\"alice\" \"bob\") (\"eve\") nil (\"math\" \"physics\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Magic sets optimization simulation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_deductive_db_magic_sets() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Magic sets: restrict evaluation to facts relevant to a specific query
    // by propagating "magic" seed facts top-down before bottom-up evaluation.
    let form = r#"(progn
  (require 'cl-lib)
  (fset 'neovm--ddb5-var-p
    (lambda (x) (and (symbolp x) (string-prefix-p "?" (symbol-name x)))))

  (fset 'neovm--ddb5-unify
    (lambda (pat fact binds)
      (if (null pat) (if (null fact) binds nil)
        (if (null fact) nil
          (let ((p (car pat)) (f (car fact)))
            (cond
             ((funcall 'neovm--ddb5-var-p p)
              (let ((b (assq p binds)))
                (if b (if (equal (cdr b) f)
                          (funcall 'neovm--ddb5-unify (cdr pat) (cdr fact) binds) nil)
                  (funcall 'neovm--ddb5-unify (cdr pat) (cdr fact)
                           (cons (cons p f) binds)))))
             ((equal p f) (funcall 'neovm--ddb5-unify (cdr pat) (cdr fact) binds))
             (t nil)))))))

  (fset 'neovm--ddb5-query-1
    (lambda (db pat)
      (let ((res nil))
        (dolist (fact db) (let ((b (funcall 'neovm--ddb5-unify pat fact nil)))
                            (when b (setq res (cons b res)))))
        (nreverse res))))

  (fset 'neovm--ddb5-subst
    (lambda (pat binds)
      (mapcar (lambda (x) (if (funcall 'neovm--ddb5-var-p x)
                               (let ((b (assq x binds))) (if b (cdr b) x)) x))
              pat)))

  ;; Magic set: given query ancestor(alice, ?x), generate magic_ancestor(alice)
  ;; Then restrict rule evaluation:
  ;; magic_ancestor(?y) :- magic_ancestor(?x), parent(?x, ?y)
  ;; ancestor(?x, ?y) :- magic_ancestor(?x), parent(?x, ?y)
  ;; ancestor(?x, ?z) :- magic_ancestor(?x), parent(?x, ?y), ancestor(?y, ?z)
  (fset 'neovm--ddb5-magic-ancestor
    (lambda (db seed-nodes max-iters)
      (let ((magic nil) (ancestors nil) (changed t) (iters 0))
        ;; Initialize magic set with seed nodes
        (dolist (s seed-nodes)
          (setq magic (cons (list 'magic s) magic)))
        ;; Iterate
        (while (and changed (< iters max-iters))
          (setq changed nil iters (1+ iters))
          ;; For each magic(X), find parent(X, Y) -> magic(Y) and ancestor(X, Y)
          (dolist (m magic)
            (let ((x (nth 1 m)))
              ;; Find all children of x
              (dolist (f db)
                (when (and (eq (car f) 'parent) (eq (nth 1 f) x))
                  (let ((y (nth 2 f)))
                    ;; Add magic(Y) if new
                    (let ((new-magic (list 'magic y)))
                      (unless (member new-magic magic)
                        (setq magic (cons new-magic magic))
                        (setq changed t)))
                    ;; Add ancestor(X, Y) if new
                    (let ((new-anc (list 'ancestor x y)))
                      (unless (member new-anc ancestors)
                        (setq ancestors (cons new-anc ancestors))
                        (setq changed t)))))))))
        ;; Also derive transitive ancestors
        (setq changed t iters 0)
        (while (and changed (< iters max-iters))
          (setq changed nil iters (1+ iters))
          (dolist (a1 ancestors)
            (dolist (a2 ancestors)
              (when (eq (nth 2 a1) (nth 1 a2))
                (let ((new-anc (list 'ancestor (nth 1 a1) (nth 2 a2))))
                  (unless (member new-anc ancestors)
                    (setq ancestors (cons new-anc ancestors))
                    (setq changed t)))))))
        (list 'magic magic 'ancestors ancestors))))

  (unwind-protect
      (let ((db '((parent alice bob) (parent bob charlie) (parent charlie dave)
                  (parent dave eve) (parent alice frank) (parent frank grace)
                  ;; Unreachable from alice (different tree)
                  (parent henry irene) (parent irene jack))))
        (let* ((result (funcall 'neovm--ddb5-magic-ancestor db '(alice) 20))
               (magic-set (cadr result))
               (ancestors (cadddr result)))
          (list
           ;; Magic set contains only nodes reachable from alice
           (sort (mapcar (lambda (m) (symbol-name (nth 1 m))) magic-set) #'string<)
           ;; Ancestor set: only pairs starting from alice's tree
           (sort (mapcar (lambda (a) (format "%s->%s" (nth 1 a) (nth 2 a))) ancestors) #'string<)
           ;; henry/irene/jack should NOT appear in magic set
           (null (member '(magic henry) magic-set))
           (null (member '(magic irene) magic-set))
           ;; alice is ancestor of dave (through bob->charlie->dave)
           (not (null (member '(ancestor alice dave) ancestors)))
           ;; henry is not ancestor of anyone in our result (not seeded)
           (null (cl-find-if (lambda (a) (eq (nth 1 a) 'henry)) ancestors)))))
    (fmakunbound 'neovm--ddb5-var-p)
    (fmakunbound 'neovm--ddb5-unify)
    (fmakunbound 'neovm--ddb5-query-1)
    (fmakunbound 'neovm--ddb5-subst)
    (fmakunbound 'neovm--ddb5-magic-ancestor)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"alice\" \"bob\" \"charlie\" \"dave\" \"eve\" \"frank\" \"grace\") (\"alice->bob\" \"alice->charlie\" \"alice->dave\" \"alice->eve\" \"alice->frank\" \"alice->grace\" \"bob->charlie\" \"bob->dave\" \"bob->eve\" \"charlie->dave\" \"charlie->eve\" \"dave->eve\" \"frank->grace\") t t t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Query evaluation with complex variable bindings
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_deductive_db_complex_query_bindings() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--ddb6-var-p
    (lambda (x) (and (symbolp x) (string-prefix-p "?" (symbol-name x)))))

  (fset 'neovm--ddb6-unify
    (lambda (pat fact binds)
      (if (null pat) (if (null fact) binds nil)
        (if (null fact) nil
          (let ((p (car pat)) (f (car fact)))
            (cond
             ((funcall 'neovm--ddb6-var-p p)
              (let ((b (assq p binds)))
                (if b (if (equal (cdr b) f)
                          (funcall 'neovm--ddb6-unify (cdr pat) (cdr fact) binds) nil)
                  (funcall 'neovm--ddb6-unify (cdr pat) (cdr fact)
                           (cons (cons p f) binds)))))
             ((equal p f) (funcall 'neovm--ddb6-unify (cdr pat) (cdr fact) binds))
             (t nil)))))))

  (fset 'neovm--ddb6-query-1
    (lambda (db pat)
      (let ((res nil))
        (dolist (fact db) (let ((b (funcall 'neovm--ddb6-unify pat fact nil)))
                            (when b (setq res (cons b res)))))
        (nreverse res))))

  (fset 'neovm--ddb6-subst
    (lambda (pat binds)
      (mapcar (lambda (x) (if (funcall 'neovm--ddb6-var-p x)
                               (let ((b (assq x binds))) (if b (cdr b) x)) x))
              pat)))

  ;; Join query: match two patterns, merge bindings
  (fset 'neovm--ddb6-join-query
    (lambda (db pat1 pat2)
      (let ((result nil))
        (dolist (b1 (funcall 'neovm--ddb6-query-1 db pat1))
          (let ((inst2 (funcall 'neovm--ddb6-subst pat2 b1)))
            (dolist (b2 (funcall 'neovm--ddb6-query-1 db inst2))
              ;; Merge bindings
              (let ((merged b1) (ok t))
                (dolist (pair b2)
                  (when ok
                    (let ((existing (assq (car pair) merged)))
                      (if existing
                          (unless (equal (cdr existing) (cdr pair))
                            (setq ok nil))
                        (setq merged (cons pair merged))))))
                (when ok (setq result (cons merged result)))))))
        (nreverse result))))

  (unwind-protect
      (let ((db '((person alice 30) (person bob 25) (person charlie 35)
                  (person dave 30) (person eve 28)
                  (works-at alice acme) (works-at bob acme)
                  (works-at charlie megacorp) (works-at dave megacorp)
                  (works-at eve acme)
                  (department acme engineering) (department acme sales)
                  (department megacorp research)
                  (in-dept alice engineering) (in-dept bob sales)
                  (in-dept charlie research) (in-dept dave research)
                  (in-dept eve engineering)
                  (salary alice 90000) (salary bob 70000) (salary charlie 85000)
                  (salary dave 80000) (salary eve 95000))))
        (list
         ;; Query: people who work at acme and are in engineering
         (sort (let ((res nil))
                 (dolist (b (funcall 'neovm--ddb6-join-query db
                              '(works-at ?name acme) '(in-dept ?name engineering)))
                   (let ((n (cdr (assq '?name b))))
                     (unless (memq n res) (setq res (cons n res)))))
                 (mapcar #'symbol-name res))
               #'string<)

         ;; Query: people of same age (self-join with constraint)
         (sort (let ((res nil))
                 (dolist (b (funcall 'neovm--ddb6-join-query db
                              '(person ?x ?age) '(person ?y ?age)))
                   (let ((x (cdr (assq '?x b))) (y (cdr (assq '?y b))))
                     (when (string< (symbol-name x) (symbol-name y))
                       (setq res (cons (list x y) res)))))
                 res)
               (lambda (a b) (string< (format "%S" a) (format "%S" b))))

         ;; Query: for each company, who earns the most?
         (let ((companies nil))
           (dolist (f db) (when (eq (car f) 'works-at)
                            (unless (memq (nth 2 f) companies)
                              (setq companies (cons (nth 2 f) companies)))))
           (sort (mapcar (lambda (company)
                    (let ((best-name nil) (best-sal 0))
                      (dolist (b (funcall 'neovm--ddb6-join-query db
                                   (list 'works-at '?name company)
                                   '(salary ?name ?sal)))
                        (let ((sal (cdr (assq '?sal b)))
                              (name (cdr (assq '?name b))))
                          (when (> sal best-sal)
                            (setq best-sal sal best-name name))))
                      (list company best-name best-sal)))
                  companies)
                 (lambda (a b) (string< (symbol-name (car a)) (symbol-name (car b))))))

         ;; Three-way join: person(?name, ?age), works-at(?name, ?co), salary(?name, ?sal)
         ;; where age >= 30
         (sort (let ((res nil))
                 (dolist (b1 (funcall 'neovm--ddb6-query-1 db '(person ?name ?age)))
                   (when (>= (cdr (assq '?age b1)) 30)
                     (let ((name (cdr (assq '?name b1))))
                       (dolist (b2 (funcall 'neovm--ddb6-query-1 db
                                     (list 'works-at name '?co)))
                         (dolist (b3 (funcall 'neovm--ddb6-query-1 db
                                       (list 'salary name '?sal)))
                           (setq res (cons (list name
                                                 (cdr (assq '?age b1))
                                                 (cdr (assq '?co b2))
                                                 (cdr (assq '?sal b3)))
                                           res)))))))
                 res)
               (lambda (a b) (string< (symbol-name (car a)) (symbol-name (car b)))))))
    (fmakunbound 'neovm--ddb6-var-p)
    (fmakunbound 'neovm--ddb6-unify)
    (fmakunbound 'neovm--ddb6-query-1)
    (fmakunbound 'neovm--ddb6-subst)
    (fmakunbound 'neovm--ddb6-join-query)))"#;
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \"?\" 69 43)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Datalog with aggregation and grouping
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_deductive_db_aggregation_grouping() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--ddb7-var-p
    (lambda (x) (and (symbolp x) (string-prefix-p "?" (symbol-name x)))))

  (fset 'neovm--ddb7-unify
    (lambda (pat fact binds)
      (if (null pat) (if (null fact) binds nil)
        (if (null fact) nil
          (let ((p (car pat)) (f (car fact)))
            (cond
             ((funcall 'neovm--ddb7-var-p p)
              (let ((b (assq p binds)))
                (if b (if (equal (cdr b) f)
                          (funcall 'neovm--ddb7-unify (cdr pat) (cdr fact) binds) nil)
                  (funcall 'neovm--ddb7-unify (cdr pat) (cdr fact)
                           (cons (cons p f) binds)))))
             ((equal p f) (funcall 'neovm--ddb7-unify (cdr pat) (cdr fact) binds))
             (t nil)))))))

  (fset 'neovm--ddb7-query-1
    (lambda (db pat)
      (let ((res nil))
        (dolist (fact db) (let ((b (funcall 'neovm--ddb7-unify pat fact nil)))
                            (when b (setq res (cons b res)))))
        (nreverse res))))

  ;; Group-by aggregation: given query results, group by key-vars and aggregate val-var
  (fset 'neovm--ddb7-group-by
    (lambda (bindings-list key-vars val-var agg-fn init-val)
      (let ((groups nil))
        (dolist (b bindings-list)
          (let* ((key (mapcar (lambda (v) (cdr (assq v b))) key-vars))
                 (val (cdr (assq val-var b)))
                 (existing (assoc key groups)))
            (if existing
                (setcdr existing (funcall agg-fn (cdr existing) val))
              (setq groups (cons (cons key (funcall agg-fn init-val val)) groups)))))
        (nreverse groups))))

  (unwind-protect
      (let ((db '((sale alice jan 100) (sale alice jan 200) (sale alice feb 150)
                  (sale bob jan 300) (sale bob feb 250) (sale bob feb 50)
                  (sale charlie jan 400) (sale charlie mar 200)
                  (sale dave feb 100) (sale dave mar 350))))
        (list
         ;; Total sales per person (group by person, sum amounts)
         (sort (funcall 'neovm--ddb7-group-by
                 (funcall 'neovm--ddb7-query-1 db '(sale ?person ?month ?amount))
                 '(?person) '?amount #'+ 0)
               (lambda (a b) (string< (symbol-name (car (car a)))
                                      (symbol-name (car (car b))))))

         ;; Total sales per month
         (sort (funcall 'neovm--ddb7-group-by
                 (funcall 'neovm--ddb7-query-1 db '(sale ?person ?month ?amount))
                 '(?month) '?amount #'+ 0)
               (lambda (a b) (string< (symbol-name (car (car a)))
                                      (symbol-name (car (car b))))))

         ;; Count transactions per person
         (sort (funcall 'neovm--ddb7-group-by
                 (funcall 'neovm--ddb7-query-1 db '(sale ?person ?month ?amount))
                 '(?person) '?amount (lambda (acc _val) (1+ acc)) 0)
               (lambda (a b) (string< (symbol-name (car (car a)))
                                      (symbol-name (car (car b))))))

         ;; Max sale per person
         (sort (funcall 'neovm--ddb7-group-by
                 (funcall 'neovm--ddb7-query-1 db '(sale ?person ?month ?amount))
                 '(?person) '?amount #'max 0)
               (lambda (a b) (string< (symbol-name (car (car a)))
                                      (symbol-name (car (car b))))))

         ;; Grand total
         (let ((total 0))
           (dolist (b (funcall 'neovm--ddb7-query-1 db '(sale ?p ?m ?a)))
             (setq total (+ total (cdr (assq '?a b)))))
           total)))
    (fmakunbound 'neovm--ddb7-var-p)
    (fmakunbound 'neovm--ddb7-unify)
    (fmakunbound 'neovm--ddb7-query-1)
    (fmakunbound 'neovm--ddb7-group-by)))"#;
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \"?\" 48 59)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
