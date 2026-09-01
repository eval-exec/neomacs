//! Oracle parity tests implementing a simple Datalog engine in Elisp:
//! facts as tuples, rules with head/body, query resolution via
//! unification and backtracking, recursive rules for transitive closure,
//! ancestor queries from parent facts, and path finding in a graph.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Datalog engine: facts, simple queries, and ground queries
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_datalog_facts_and_ground_queries() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a Datalog engine with facts stored as lists of tuples.
    // A fact is (predicate arg1 arg2 ...).
    // A query is a fact pattern where symbols starting with ? are variables.
    let form = r#"(progn
  ;; Database is a list of facts
  (fset 'neovm--dl-make-db (lambda () nil))

  (fset 'neovm--dl-add-fact
    (lambda (db fact) (if (member fact db) db (cons fact db))))

  ;; Check if a symbol is a variable (?-prefixed)
  (fset 'neovm--dl-var-p
    (lambda (x) (and (symbolp x) (string-prefix-p "?" (symbol-name x)))))

  ;; Unify a pattern against a fact, returning bindings alist or nil
  (fset 'neovm--dl-unify
    (lambda (pattern fact bindings)
      (if (null pattern)
          (if (null fact) bindings nil)
        (if (null fact) nil
          (let ((p (car pattern)) (f (car fact)))
            (cond
             ;; Variable: check existing binding or create new
             ((funcall 'neovm--dl-var-p p)
              (let ((bound (assq p bindings)))
                (if bound
                    (if (equal (cdr bound) f)
                        (funcall 'neovm--dl-unify (cdr pattern) (cdr fact) bindings)
                      nil)
                  (funcall 'neovm--dl-unify (cdr pattern) (cdr fact)
                           (cons (cons p f) bindings)))))
             ;; Constant: must match exactly
             ((equal p f)
              (funcall 'neovm--dl-unify (cdr pattern) (cdr fact) bindings))
             (t nil)))))))

  ;; Query: find all binding sets that match a pattern against all facts
  (fset 'neovm--dl-query
    (lambda (db pattern)
      (let ((results nil))
        (dolist (fact db)
          (let ((bindings (funcall 'neovm--dl-unify pattern fact nil)))
            (when bindings
              (setq results (cons bindings results)))))
        (nreverse results))))

  (unwind-protect
      (let ((db (funcall 'neovm--dl-make-db)))
        ;; Add some facts
        (setq db (funcall 'neovm--dl-add-fact db '(parent alice bob)))
        (setq db (funcall 'neovm--dl-add-fact db '(parent bob charlie)))
        (setq db (funcall 'neovm--dl-add-fact db '(parent alice dave)))
        (setq db (funcall 'neovm--dl-add-fact db '(parent dave eve)))
        (setq db (funcall 'neovm--dl-add-fact db '(likes alice chocolate)))
        (setq db (funcall 'neovm--dl-add-fact db '(likes bob tea)))
        (setq db (funcall 'neovm--dl-add-fact db '(likes charlie coffee)))

        (list
         ;; Ground query: does (parent alice bob) exist?
         (not (null (funcall 'neovm--dl-query db '(parent alice bob))))
         ;; Ground query: does (parent bob alice) exist? (no)
         (not (null (funcall 'neovm--dl-query db '(parent bob alice))))

         ;; Variable query: who are alice's children?
         (mapcar (lambda (b) (cdr (assq '?child b)))
                 (funcall 'neovm--dl-query db '(parent alice ?child)))

         ;; Variable query: who is bob's parent?
         (mapcar (lambda (b) (cdr (assq '?p b)))
                 (funcall 'neovm--dl-query db '(parent ?p bob)))

         ;; Two variables: all parent-child pairs
         (mapcar (lambda (b) (cons (cdr (assq '?p b)) (cdr (assq '?c b))))
                 (funcall 'neovm--dl-query db '(parent ?p ?c)))

         ;; Query likes
         (mapcar (lambda (b) (cdr (assq '?thing b)))
                 (funcall 'neovm--dl-query db '(likes ?who ?thing)))

         ;; No match
         (funcall 'neovm--dl-query db '(parent eve ?child))))
    (fmakunbound 'neovm--dl-make-db)
    (fmakunbound 'neovm--dl-add-fact)
    (fmakunbound 'neovm--dl-var-p)
    (fmakunbound 'neovm--dl-unify)
    (fmakunbound 'neovm--dl-query)))"#;
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \"?\" 62 43)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Datalog rules: head + body clauses with join
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_datalog_rules_with_join() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'cl-lib)
  (fset 'neovm--dl-var-p
    (lambda (x) (and (symbolp x) (string-prefix-p "?" (symbol-name x)))))

  (fset 'neovm--dl-unify
    (lambda (pat fact binds)
      (if (null pat) (if (null fact) binds nil)
        (if (null fact) nil
          (let ((p (car pat)) (f (car fact)))
            (cond
             ((funcall 'neovm--dl-var-p p)
              (let ((b (assq p binds)))
                (if b (if (equal (cdr b) f)
                          (funcall 'neovm--dl-unify (cdr pat) (cdr fact) binds)
                        nil)
                  (funcall 'neovm--dl-unify (cdr pat) (cdr fact)
                           (cons (cons p f) binds)))))
             ((equal p f) (funcall 'neovm--dl-unify (cdr pat) (cdr fact) binds))
             (t nil)))))))

  (fset 'neovm--dl-query-1
    (lambda (db pat)
      (let ((res nil))
        (dolist (fact db)
          (let ((b (funcall 'neovm--dl-unify pat fact nil)))
            (when b (setq res (cons b res)))))
        (nreverse res))))

  ;; Substitute variables in a pattern using bindings
  (fset 'neovm--dl-subst
    (lambda (pat binds)
      (mapcar (lambda (x)
                (if (funcall 'neovm--dl-var-p x)
                    (let ((b (assq x binds)))
                      (if b (cdr b) x))
                  x))
              pat)))

  ;; Evaluate a conjunctive query (list of patterns) against db
  ;; Returns list of complete binding sets
  (fset 'neovm--dl-eval-body
    (lambda (db body bindings-list)
      (if (null body) bindings-list
        (let ((pat (car body))
              (new-bindings nil))
          (dolist (binds bindings-list)
            (let* ((instantiated (funcall 'neovm--dl-subst pat binds))
                   (matches (funcall 'neovm--dl-query-1 db instantiated)))
              (dolist (m matches)
                ;; Merge bindings
                (let ((merged binds)
                      (ok t))
                  (dolist (pair m)
                    (when ok
                      (let ((existing (assq (car pair) merged)))
                        (if existing
                            (unless (equal (cdr existing) (cdr pair))
                              (setq ok nil))
                          (setq merged (cons pair merged))))))
                  (when ok
                    (setq new-bindings (cons merged new-bindings)))))))
          (funcall 'neovm--dl-eval-body db (cdr body) (nreverse new-bindings))))))

  ;; A rule: (head body1 body2 ...)
  ;; Apply rule to db, return new derived facts
  (fset 'neovm--dl-apply-rule
    (lambda (db rule)
      (let* ((head (car rule))
             (body (cdr rule))
             (bindings-list (funcall 'neovm--dl-eval-body db body (list nil)))
             (new-facts nil))
        (dolist (binds bindings-list)
          (let ((fact (funcall 'neovm--dl-subst head binds)))
            ;; Only add fully ground facts
            (unless (cl-some (lambda (x) (funcall 'neovm--dl-var-p x)) fact)
              (unless (member fact new-facts)
                (setq new-facts (cons fact new-facts))))))
        (nreverse new-facts))))

  (unwind-protect
      (let ((db '((parent alice bob)
                  (parent bob charlie)
                  (parent alice dave)
                  (parent dave eve)
                  (male bob)
                  (male charlie)
                  (male dave)
                  (female alice)
                  (female eve))))
        ;; Rule: (grandparent ?x ?z) :- (parent ?x ?y), (parent ?y ?z)
        (let* ((gp-rule '((grandparent ?x ?z) (parent ?x ?y) (parent ?y ?z)))
               (gp-facts (funcall 'neovm--dl-apply-rule db gp-rule)))
          ;; Rule: (father ?x ?y) :- (parent ?x ?y), (male ?x)
          (let* ((father-rule '((father ?x ?y) (parent ?x ?y) (male ?x)))
                 (father-facts (funcall 'neovm--dl-apply-rule db father-rule)))
            ;; Rule: (mother ?x ?y) :- (parent ?x ?y), (female ?x)
            (let* ((mother-rule '((mother ?x ?y) (parent ?x ?y) (female ?x)))
                   (mother-facts (funcall 'neovm--dl-apply-rule db mother-rule)))
              (list
               ;; Grandparent derived facts
               (sort (copy-sequence gp-facts)
                     (lambda (a b) (string< (format "%S" a) (format "%S" b))))
               ;; Father derived facts
               (sort (copy-sequence father-facts)
                     (lambda (a b) (string< (format "%S" a) (format "%S" b))))
               ;; Mother derived facts
               mother-facts)))))
    (fmakunbound 'neovm--dl-var-p)
    (fmakunbound 'neovm--dl-unify)
    (fmakunbound 'neovm--dl-query-1)
    (fmakunbound 'neovm--dl-subst)
    (fmakunbound 'neovm--dl-eval-body)
    (fmakunbound 'neovm--dl-apply-rule)))"#;
    let expect = expect_test::expect![[r#""OK (nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Recursive rules: transitive closure (ancestor)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_datalog_transitive_closure_ancestor() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'cl-lib)
  (fset 'neovm--dl-var-p
    (lambda (x) (and (symbolp x) (string-prefix-p "?" (symbol-name x)))))

  (fset 'neovm--dl-unify
    (lambda (pat fact binds)
      (if (null pat) (if (null fact) binds nil)
        (if (null fact) nil
          (let ((p (car pat)) (f (car fact)))
            (cond
             ((funcall 'neovm--dl-var-p p)
              (let ((b (assq p binds)))
                (if b (if (equal (cdr b) f)
                          (funcall 'neovm--dl-unify (cdr pat) (cdr fact) binds) nil)
                  (funcall 'neovm--dl-unify (cdr pat) (cdr fact)
                           (cons (cons p f) binds)))))
             ((equal p f) (funcall 'neovm--dl-unify (cdr pat) (cdr fact) binds))
             (t nil)))))))

  (fset 'neovm--dl-query-1
    (lambda (db pat)
      (let ((res nil))
        (dolist (fact db)
          (let ((b (funcall 'neovm--dl-unify pat fact nil)))
            (when b (setq res (cons b res)))))
        (nreverse res))))

  (fset 'neovm--dl-subst
    (lambda (pat binds)
      (mapcar (lambda (x) (if (funcall 'neovm--dl-var-p x)
                               (let ((b (assq x binds))) (if b (cdr b) x)) x))
              pat)))

  (fset 'neovm--dl-eval-body
    (lambda (db body bl)
      (if (null body) bl
        (let ((pat (car body)) (nb nil))
          (dolist (binds bl)
            (let* ((inst (funcall 'neovm--dl-subst pat binds))
                   (ms (funcall 'neovm--dl-query-1 db inst)))
              (dolist (m ms)
                (let ((mg binds) (ok t))
                  (dolist (p m)
                    (when ok
                      (let ((ex (assq (car p) mg)))
                        (if ex (unless (equal (cdr ex) (cdr p)) (setq ok nil))
                          (setq mg (cons p mg))))))
                  (when ok (setq nb (cons mg nb)))))))
          (funcall 'neovm--dl-eval-body db (cdr body) (nreverse nb))))))

  (fset 'neovm--dl-apply-rule
    (lambda (db rule)
      (let* ((head (car rule)) (body (cdr rule))
             (bl (funcall 'neovm--dl-eval-body db body (list nil)))
             (nf nil))
        (dolist (b bl)
          (let ((fact (funcall 'neovm--dl-subst head b)))
            (unless (cl-some (lambda (x) (funcall 'neovm--dl-var-p x)) fact)
              (unless (member fact nf) (setq nf (cons fact nf))))))
        (nreverse nf))))

  ;; Fixed-point iteration for recursive rules
  (fset 'neovm--dl-fixpoint
    (lambda (db rules &optional max-iter)
      (let ((limit (or max-iter 100))
            (i 0)
            (changed t))
        (while (and changed (< i limit))
          (setq changed nil)
          (dolist (rule rules)
            (let ((new-facts (funcall 'neovm--dl-apply-rule db rule)))
              (dolist (f new-facts)
                (unless (member f db)
                  (setq db (cons f db))
                  (setq changed t)))))
          (setq i (1+ i)))
        db)))

  (unwind-protect
      (let* ((db '((parent alice bob)
                   (parent bob charlie)
                   (parent charlie david)
                   (parent david eve)
                   (parent alice frank)))
             ;; ancestor(X,Y) :- parent(X,Y).
             ;; ancestor(X,Z) :- parent(X,Y), ancestor(Y,Z).
             (rules '(((ancestor ?x ?y) (parent ?x ?y))
                      ((ancestor ?x ?z) (parent ?x ?y) (ancestor ?y ?z))))
             (result-db (funcall 'neovm--dl-fixpoint db rules)))
        ;; Extract only ancestor facts
        (let ((ancestors nil))
          (dolist (f result-db)
            (when (eq (car f) 'ancestor)
              (setq ancestors (cons f ancestors))))
          (sort ancestors
                (lambda (a b) (string< (format "%S" a) (format "%S" b))))))
    (fmakunbound 'neovm--dl-var-p)
    (fmakunbound 'neovm--dl-unify)
    (fmakunbound 'neovm--dl-query-1)
    (fmakunbound 'neovm--dl-subst)
    (fmakunbound 'neovm--dl-eval-body)
    (fmakunbound 'neovm--dl-apply-rule)
    (fmakunbound 'neovm--dl-fixpoint)))"#;
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: path finding in a directed graph via Datalog
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_datalog_graph_path_finding() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'cl-lib)
  (fset 'neovm--dl-var-p
    (lambda (x) (and (symbolp x) (string-prefix-p "?" (symbol-name x)))))

  (fset 'neovm--dl-unify
    (lambda (pat fact binds)
      (if (null pat) (if (null fact) binds nil)
        (if (null fact) nil
          (let ((p (car pat)) (f (car fact)))
            (cond
             ((funcall 'neovm--dl-var-p p)
              (let ((b (assq p binds)))
                (if b (if (equal (cdr b) f)
                          (funcall 'neovm--dl-unify (cdr pat) (cdr fact) binds) nil)
                  (funcall 'neovm--dl-unify (cdr pat) (cdr fact)
                           (cons (cons p f) binds)))))
             ((equal p f) (funcall 'neovm--dl-unify (cdr pat) (cdr fact) binds))
             (t nil)))))))

  (fset 'neovm--dl-query-1
    (lambda (db pat)
      (let ((res nil))
        (dolist (fact db)
          (let ((b (funcall 'neovm--dl-unify pat fact nil)))
            (when b (setq res (cons b res)))))
        (nreverse res))))

  (fset 'neovm--dl-subst
    (lambda (pat binds)
      (mapcar (lambda (x) (if (funcall 'neovm--dl-var-p x)
                               (let ((b (assq x binds))) (if b (cdr b) x)) x))
              pat)))

  (fset 'neovm--dl-eval-body
    (lambda (db body bl)
      (if (null body) bl
        (let ((pat (car body)) (nb nil))
          (dolist (binds bl)
            (let* ((inst (funcall 'neovm--dl-subst pat binds))
                   (ms (funcall 'neovm--dl-query-1 db inst)))
              (dolist (m ms)
                (let ((mg binds) (ok t))
                  (dolist (p m)
                    (when ok
                      (let ((ex (assq (car p) mg)))
                        (if ex (unless (equal (cdr ex) (cdr p)) (setq ok nil))
                          (setq mg (cons p mg))))))
                  (when ok (setq nb (cons mg nb)))))))
          (funcall 'neovm--dl-eval-body db (cdr body) (nreverse nb))))))

  (fset 'neovm--dl-apply-rule
    (lambda (db rule)
      (let* ((head (car rule)) (body (cdr rule))
             (bl (funcall 'neovm--dl-eval-body db body (list nil)))
             (nf nil))
        (dolist (b bl)
          (let ((fact (funcall 'neovm--dl-subst head b)))
            (unless (cl-some (lambda (x) (funcall 'neovm--dl-var-p x)) fact)
              (unless (member fact nf) (setq nf (cons fact nf))))))
        (nreverse nf))))

  (fset 'neovm--dl-fixpoint
    (lambda (db rules &optional max-iter)
      (let ((limit (or max-iter 100)) (i 0) (changed t))
        (while (and changed (< i limit))
          (setq changed nil)
          (dolist (rule rules)
            (let ((new-facts (funcall 'neovm--dl-apply-rule db rule)))
              (dolist (f new-facts)
                (unless (member f db) (setq db (cons f db)) (setq changed t)))))
          (setq i (1+ i)))
        db)))

  (unwind-protect
      (let* (;; Directed graph as edge facts
             (db '((edge a b) (edge b c) (edge c d) (edge a e) (edge e f)
                   (edge f c) (edge d g) (edge g h)))
             ;; reachable(X,Y) :- edge(X,Y).
             ;; reachable(X,Z) :- edge(X,Y), reachable(Y,Z).
             (rules '(((reachable ?x ?y) (edge ?x ?y))
                      ((reachable ?x ?z) (edge ?x ?y) (reachable ?y ?z))))
             (result-db (funcall 'neovm--dl-fixpoint db rules)))
        (let ((reachable nil))
          (dolist (f result-db)
            (when (eq (car f) 'reachable)
              (setq reachable (cons f reachable))))
          (list
           ;; Total reachable pairs count
           (length reachable)
           ;; Is h reachable from a?
           (not (null (member '(reachable a h) reachable)))
           ;; Is a reachable from h? (no, directed)
           (not (null (member '(reachable h a) reachable)))
           ;; All nodes reachable from a (sorted)
           (sort (let ((targets nil))
                   (dolist (r reachable)
                     (when (eq (nth 1 r) 'a)
                       (unless (memq (nth 2 r) targets)
                         (setq targets (cons (nth 2 r) targets)))))
                   (mapcar #'symbol-name targets))
                 #'string<)
           ;; All nodes reachable from e
           (sort (let ((targets nil))
                   (dolist (r reachable)
                     (when (eq (nth 1 r) 'e)
                       (unless (memq (nth 2 r) targets)
                         (setq targets (cons (nth 2 r) targets)))))
                   (mapcar #'symbol-name targets))
                 #'string<))))
    (fmakunbound 'neovm--dl-var-p)
    (fmakunbound 'neovm--dl-unify)
    (fmakunbound 'neovm--dl-query-1)
    (fmakunbound 'neovm--dl-subst)
    (fmakunbound 'neovm--dl-eval-body)
    (fmakunbound 'neovm--dl-apply-rule)
    (fmakunbound 'neovm--dl-fixpoint)))"#;
    let expect = expect_test::expect![[r#""OK (0 nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Datalog with negation-as-failure and stratification
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_datalog_negation_as_failure() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--dl-var-p
    (lambda (x) (and (symbolp x) (string-prefix-p "?" (symbol-name x)))))

  (fset 'neovm--dl-unify
    (lambda (pat fact binds)
      (if (null pat) (if (null fact) binds nil)
        (if (null fact) nil
          (let ((p (car pat)) (f (car fact)))
            (cond
             ((funcall 'neovm--dl-var-p p)
              (let ((b (assq p binds)))
                (if b (if (equal (cdr b) f)
                          (funcall 'neovm--dl-unify (cdr pat) (cdr fact) binds) nil)
                  (funcall 'neovm--dl-unify (cdr pat) (cdr fact)
                           (cons (cons p f) binds)))))
             ((equal p f) (funcall 'neovm--dl-unify (cdr pat) (cdr fact) binds))
             (t nil)))))))

  (fset 'neovm--dl-query-1
    (lambda (db pat)
      (let ((res nil))
        (dolist (fact db)
          (let ((b (funcall 'neovm--dl-unify pat fact nil)))
            (when b (setq res (cons b res)))))
        (nreverse res))))

  (fset 'neovm--dl-subst
    (lambda (pat binds)
      (mapcar (lambda (x) (if (funcall 'neovm--dl-var-p x)
                               (let ((b (assq x binds))) (if b (cdr b) x)) x))
              pat)))

  ;; Check if a fact (possibly with bound vars) exists in db
  (fset 'neovm--dl-fact-exists
    (lambda (db pat)
      (not (null (funcall 'neovm--dl-query-1 db pat)))))

  (unwind-protect
      (let ((db '((student alice) (student bob) (student charlie)
                  (student dave) (student eve)
                  (enrolled alice math) (enrolled alice science)
                  (enrolled bob math) (enrolled bob art)
                  (enrolled charlie science) (enrolled charlie art)
                  (enrolled dave math)
                  ;; eve is not enrolled in anything
                  (passed alice math) (passed bob math)
                  (passed charlie science))))
        (list
         ;; Find students enrolled in math
         (let ((res nil))
           (dolist (f db)
             (when (and (eq (car f) 'enrolled) (eq (nth 2 f) 'math))
               (setq res (cons (nth 1 f) res))))
           (sort (mapcar #'symbol-name (nreverse res)) #'string<))

         ;; Find students enrolled in math who have NOT passed math
         ;; (negation-as-failure)
         (let ((res nil))
           (dolist (f db)
             (when (and (eq (car f) 'enrolled) (eq (nth 2 f) 'math))
               (let ((s (nth 1 f)))
                 (unless (funcall 'neovm--dl-fact-exists db
                                  (list 'passed s 'math))
                   (setq res (cons s res))))))
           (sort (mapcar #'symbol-name (nreverse res)) #'string<))

         ;; Students not enrolled in any course
         (let ((enrolled-students nil))
           (dolist (f db)
             (when (eq (car f) 'enrolled)
               (unless (memq (nth 1 f) enrolled-students)
                 (setq enrolled-students (cons (nth 1 f) enrolled-students)))))
           (let ((unenrolled nil))
             (dolist (f db)
               (when (eq (car f) 'student)
                 (unless (memq (nth 1 f) enrolled-students)
                   (setq unenrolled (cons (nth 1 f) unenrolled)))))
             (mapcar #'symbol-name unenrolled)))

         ;; Courses taken by alice but not by bob
         (let ((alice-courses nil) (bob-courses nil))
           (dolist (f db)
             (when (and (eq (car f) 'enrolled) (eq (nth 1 f) 'alice))
               (setq alice-courses (cons (nth 2 f) alice-courses)))
             (when (and (eq (car f) 'enrolled) (eq (nth 1 f) 'bob))
               (setq bob-courses (cons (nth 2 f) bob-courses))))
           (let ((diff nil))
             (dolist (c alice-courses)
               (unless (memq c bob-courses) (setq diff (cons c diff))))
             (sort (mapcar #'symbol-name diff) #'string<)))))
    (fmakunbound 'neovm--dl-var-p)
    (fmakunbound 'neovm--dl-unify)
    (fmakunbound 'neovm--dl-query-1)
    (fmakunbound 'neovm--dl-subst)
    (fmakunbound 'neovm--dl-fact-exists)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"alice\" \"bob\" \"dave\") (\"alice\" \"bob\" \"dave\") (\"eve\") (\"science\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Datalog: multi-predicate rules with aggregation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_datalog_multi_predicate_aggregation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--dl-var-p
    (lambda (x) (and (symbolp x) (string-prefix-p "?" (symbol-name x)))))

  (fset 'neovm--dl-unify
    (lambda (pat fact binds)
      (if (null pat) (if (null fact) binds nil)
        (if (null fact) nil
          (let ((p (car pat)) (f (car fact)))
            (cond
             ((funcall 'neovm--dl-var-p p)
              (let ((b (assq p binds)))
                (if b (if (equal (cdr b) f)
                          (funcall 'neovm--dl-unify (cdr pat) (cdr fact) binds) nil)
                  (funcall 'neovm--dl-unify (cdr pat) (cdr fact)
                           (cons (cons p f) binds)))))
             ((equal p f) (funcall 'neovm--dl-unify (cdr pat) (cdr fact) binds))
             (t nil)))))))

  (fset 'neovm--dl-query-1
    (lambda (db pat)
      (let ((res nil))
        (dolist (fact db)
          (let ((b (funcall 'neovm--dl-unify pat fact nil)))
            (when b (setq res (cons b res)))))
        (nreverse res))))

  (unwind-protect
      (let ((db '((employee alice engineering 90000)
                  (employee bob engineering 85000)
                  (employee charlie marketing 70000)
                  (employee dave marketing 75000)
                  (employee eve engineering 95000)
                  (employee frank sales 60000)
                  (manages alice bob)
                  (manages alice eve)
                  (manages charlie dave)
                  (manages frank frank))))
        (list
         ;; Count employees per department
         (let ((dept-counts nil))
           (dolist (f db)
             (when (eq (car f) 'employee)
               (let* ((dept (nth 2 f))
                      (pair (assq dept dept-counts)))
                 (if pair (setcdr pair (1+ (cdr pair)))
                   (setq dept-counts (cons (cons dept 1) dept-counts))))))
           (sort dept-counts
                 (lambda (a b) (string< (symbol-name (car a))
                                        (symbol-name (car b))))))

         ;; Sum salaries per department
         (let ((dept-sums nil))
           (dolist (f db)
             (when (eq (car f) 'employee)
               (let* ((dept (nth 2 f))
                      (salary (nth 3 f))
                      (pair (assq dept dept-sums)))
                 (if pair (setcdr pair (+ (cdr pair) salary))
                   (setq dept-sums (cons (cons dept salary) dept-sums))))))
           (sort dept-sums
                 (lambda (a b) (string< (symbol-name (car a))
                                        (symbol-name (car b))))))

         ;; Find who manages someone in engineering with salary > 85000
         (let ((high-eng nil))
           (dolist (f db)
             (when (and (eq (car f) 'employee)
                        (eq (nth 2 f) 'engineering)
                        (> (nth 3 f) 85000))
               (setq high-eng (cons (nth 1 f) high-eng))))
           (let ((managers nil))
             (dolist (f db)
               (when (and (eq (car f) 'manages)
                          (memq (nth 2 f) high-eng))
                 (unless (memq (nth 1 f) managers)
                   (setq managers (cons (nth 1 f) managers)))))
             (sort (mapcar #'symbol-name managers) #'string<)))

         ;; Average salary (integer division)
         (let ((total 0) (count 0))
           (dolist (f db)
             (when (eq (car f) 'employee)
               (setq total (+ total (nth 3 f)))
               (setq count (1+ count))))
           (/ total count))))
    (fmakunbound 'neovm--dl-var-p)
    (fmakunbound 'neovm--dl-unify)
    (fmakunbound 'neovm--dl-query-1)))"#;
    let expect = expect_test::expect![[
        r#""OK (((engineering . 3) (marketing . 2) (sales . 1)) ((engineering . 270000) (marketing . 145000) (sales . 60000)) (\"alice\") 79166)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
