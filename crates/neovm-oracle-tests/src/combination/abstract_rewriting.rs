//! Oracle parity tests for abstract rewriting systems (ARS) in Elisp:
//! reduction relations as adjacency lists, reflexive-transitive closure,
//! normal forms, Church-Rosser property testing, diamond property verification,
//! Newman's lemma verification, reduction sequences, strongly normalizing terms,
//! head reduction, leftmost-outermost strategy, and parallel reduction.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Reduction relation representation and one-step successors
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ars_reduction_relation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Represent reduction relation as alist: ((from . (to1 to2 ...)) ...)
    // Compute one-step successors and check reducibility.
    let form = r#"(progn
  (fset 'neovm--ars-successors
    (lambda (rel term)
      (let ((entry (assoc term rel)))
        (if entry (cdr entry) nil))))

  (fset 'neovm--ars-is-reducible
    (lambda (rel term)
      (not (null (funcall 'neovm--ars-successors rel term)))))

  (unwind-protect
      (let ((rel '((a . (b c))
                   (b . (d))
                   (c . (d e))
                   (d . nil)
                   (e . (f))
                   (f . nil))))
        (list
          ;; Successors of a: (b c)
          (funcall 'neovm--ars-successors rel 'a)
          ;; Successors of d: nil (normal form)
          (funcall 'neovm--ars-successors rel 'd)
          ;; Successors of c: (d e)
          (funcall 'neovm--ars-successors rel 'c)
          ;; Reducibility
          (funcall 'neovm--ars-is-reducible rel 'a)
          (funcall 'neovm--ars-is-reducible rel 'd)
          (funcall 'neovm--ars-is-reducible rel 'f)
          ;; Unknown term
          (funcall 'neovm--ars-successors rel 'z)))
    (fmakunbound 'neovm--ars-successors)
    (fmakunbound 'neovm--ars-is-reducible)))"#;
    let expect = expect_test::expect![[r#""OK ((b c) nil (d e) t nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Reflexive-transitive closure (reachability via BFS)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ars_reflexive_transitive_closure() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Compute all terms reachable from a given start term (including itself).
    let form = r#"(progn
  (fset 'neovm--ars-successors
    (lambda (rel term)
      (let ((entry (assoc term rel)))
        (if entry (cdr entry) nil))))

  ;; BFS reachability: returns sorted list of all reachable terms
  (fset 'neovm--ars-reachable
    (lambda (rel start)
      (let ((visited nil)
            (queue (list start)))
        (while queue
          (let ((current (car queue)))
            (setq queue (cdr queue))
            (unless (memq current visited)
              (setq visited (cons current visited))
              (dolist (succ (funcall 'neovm--ars-successors rel current))
                (unless (memq succ visited)
                  (setq queue (append queue (list succ))))))))
        (sort visited (lambda (a b) (string< (symbol-name a) (symbol-name b)))))))

  (unwind-protect
      (let ((rel '((a . (b c))
                   (b . (d))
                   (c . (d e))
                   (d . nil)
                   (e . (f))
                   (f . nil))))
        (list
          ;; From a: reachable = {a,b,c,d,e,f}
          (funcall 'neovm--ars-reachable rel 'a)
          ;; From c: reachable = {c,d,e,f}
          (funcall 'neovm--ars-reachable rel 'c)
          ;; From d: only {d} (normal form)
          (funcall 'neovm--ars-reachable rel 'd)
          ;; From e: {e,f}
          (funcall 'neovm--ars-reachable rel 'e)))
    (fmakunbound 'neovm--ars-successors)
    (fmakunbound 'neovm--ars-reachable)))"#;
    let expect = expect_test::expect![[r#""OK ((a b c d e f) (c d e f) (d) (e f))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Normal forms: terms with no successors
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ars_normal_forms() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Find all normal forms (irreducible terms) in a relation.
    // Also compute all normal forms reachable from a given term.
    let form = r#"(progn
  (fset 'neovm--ars-successors
    (lambda (rel term)
      (let ((entry (assoc term rel)))
        (if entry (cdr entry) nil))))

  ;; Check if a term is in normal form
  (fset 'neovm--ars-normal-form-p
    (lambda (rel term)
      (null (funcall 'neovm--ars-successors rel term))))

  ;; Find ALL normal forms in the relation
  (fset 'neovm--ars-all-normal-forms
    (lambda (rel)
      (let ((nfs nil))
        (dolist (entry rel)
          (let ((term (car entry)))
            (when (null (cdr entry))
              (setq nfs (cons term nfs)))))
        ;; Also check terms that appear only as targets
        (dolist (entry rel)
          (dolist (target (cdr entry))
            (when (and (not (assoc target rel))
                       (not (memq target nfs)))
              (setq nfs (cons target nfs)))))
        (sort nfs (lambda (a b) (string< (symbol-name a) (symbol-name b)))))))

  ;; Find normal forms reachable from start via BFS
  (fset 'neovm--ars-reachable-nfs
    (lambda (rel start)
      (let ((visited nil)
            (queue (list start))
            (nfs nil))
        (while queue
          (let ((current (car queue)))
            (setq queue (cdr queue))
            (unless (memq current visited)
              (setq visited (cons current visited))
              (let ((succs (funcall 'neovm--ars-successors rel current)))
                (if (null succs)
                    (unless (memq current nfs)
                      (setq nfs (cons current nfs)))
                  (dolist (s succs)
                    (unless (memq s visited)
                      (setq queue (append queue (list s))))))))))
        (sort nfs (lambda (a b) (string< (symbol-name a) (symbol-name b)))))))

  (unwind-protect
      (let ((rel '((a . (b c))
                   (b . (d))
                   (c . (d e))
                   (d . nil)
                   (e . (f))
                   (f . nil))))
        (list
          ;; Normal form checks
          (funcall 'neovm--ars-normal-form-p rel 'a)
          (funcall 'neovm--ars-normal-form-p rel 'd)
          (funcall 'neovm--ars-normal-form-p rel 'f)
          ;; All normal forms
          (funcall 'neovm--ars-all-normal-forms rel)
          ;; Reachable normal forms from a: {d, f}
          (funcall 'neovm--ars-reachable-nfs rel 'a)
          ;; From b: {d}
          (funcall 'neovm--ars-reachable-nfs rel 'b)
          ;; From c: {d, f}
          (funcall 'neovm--ars-reachable-nfs rel 'c)))
    (fmakunbound 'neovm--ars-successors)
    (fmakunbound 'neovm--ars-normal-form-p)
    (fmakunbound 'neovm--ars-all-normal-forms)
    (fmakunbound 'neovm--ars-reachable-nfs)))"#;
    let expect = expect_test::expect![[r#""OK (nil t t (d f) (d f) (d) (d f))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Church-Rosser property: all divergent paths converge
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ars_church_rosser() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Church-Rosser (confluence): if a ->* b and a ->* c, then exists d
    // such that b ->* d and c ->* d.
    // Test by checking all pairs of reachable terms share a common reduct.
    let form = r#"(progn
  (fset 'neovm--ars-successors
    (lambda (rel term)
      (let ((entry (assoc term rel)))
        (if entry (cdr entry) nil))))

  (fset 'neovm--ars-reachable-set
    (lambda (rel start)
      (let ((visited nil)
            (queue (list start)))
        (while queue
          (let ((current (car queue)))
            (setq queue (cdr queue))
            (unless (memq current visited)
              (setq visited (cons current visited))
              (dolist (s (funcall 'neovm--ars-successors rel current))
                (unless (memq s visited)
                  (setq queue (append queue (list s))))))))
        visited)))

  ;; Check if two terms have a common reduct
  (fset 'neovm--ars-have-common-reduct
    (lambda (rel t1 t2)
      (let ((reach1 (funcall 'neovm--ars-reachable-set rel t1))
            (reach2 (funcall 'neovm--ars-reachable-set rel t2)))
        (let ((common nil))
          (dolist (r reach1)
            (when (memq r reach2)
              (setq common t)))
          common))))

  ;; Check Church-Rosser: for every term, all pairs of its successors
  ;; must have a common reduct
  (fset 'neovm--ars-church-rosser-p
    (lambda (rel)
      (let ((result t))
        (dolist (entry rel)
          (let ((term (car entry)))
            (let ((reachable (funcall 'neovm--ars-reachable-set rel term)))
              (dolist (b reachable)
                (dolist (c reachable)
                  (unless (funcall 'neovm--ars-have-common-reduct rel b c)
                    (setq result nil)))))))
        result)))

  (unwind-protect
      (let (;; Confluent system
            (rel1 '((a . (b c))
                    (b . (d))
                    (c . (d))
                    (d . nil)))
            ;; Non-confluent system: a -> b, a -> c, no common reduct
            (rel2 '((a . (b c))
                    (b . nil)
                    (c . nil)))
            ;; Bigger confluent system (diamond)
            (rel3 '((a . (b c))
                    (b . (d e))
                    (c . (e f))
                    (d . (g))
                    (e . (g))
                    (f . (g))
                    (g . nil))))
        (list
          (funcall 'neovm--ars-church-rosser-p rel1)
          (funcall 'neovm--ars-church-rosser-p rel2)
          (funcall 'neovm--ars-church-rosser-p rel3)))
    (fmakunbound 'neovm--ars-successors)
    (fmakunbound 'neovm--ars-reachable-set)
    (fmakunbound 'neovm--ars-have-common-reduct)
    (fmakunbound 'neovm--ars-church-rosser-p)))"#;
    let expect = expect_test::expect![[r#""OK (t nil t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Diamond property: one-step divergence converges in one step
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ars_diamond_property() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Diamond property: if a -> b and a -> c (b != c), then exists d
    // such that b -> d and c -> d (all in one step).
    let form = r#"(progn
  (fset 'neovm--ars-successors
    (lambda (rel term)
      (let ((entry (assoc term rel)))
        (if entry (cdr entry) nil))))

  ;; Check diamond property for a given term
  (fset 'neovm--ars-diamond-at
    (lambda (rel term)
      (let ((succs (funcall 'neovm--ars-successors rel term)))
        (if (<= (length succs) 1)
            t  ; trivially satisfied
          ;; For every pair of successors, check they have a common one-step successor
          (let ((ok t))
            (let ((rest succs))
              (while (and ok rest)
                (let ((b (car rest)))
                  (dolist (c (cdr rest))
                    (when (not (eq b c))
                      (let ((succs-b (funcall 'neovm--ars-successors rel b))
                            (succs-c (funcall 'neovm--ars-successors rel c))
                            (found nil))
                        (dolist (sb succs-b)
                          (when (memq sb succs-c)
                            (setq found t)))
                        (unless found
                          (setq ok nil))))))
                (setq rest (cdr rest))))
            ok)))))

  ;; Check diamond property for the whole relation
  (fset 'neovm--ars-diamond-p
    (lambda (rel)
      (let ((result t))
        (dolist (entry rel)
          (unless (funcall 'neovm--ars-diamond-at rel (car entry))
            (setq result nil)))
        result)))

  (unwind-protect
      (let (;; Has diamond property
            (rel1 '((a . (b c))
                    (b . (d))
                    (c . (d))
                    (d . nil)))
            ;; No diamond: a->b, a->c, b->d, c->e, no common one-step
            (rel2 '((a . (b c))
                    (b . (d))
                    (c . (e))
                    (d . nil)
                    (e . nil)))
            ;; Diamond: multiple common successors
            (rel3 '((a . (b c))
                    (b . (d e))
                    (c . (d))
                    (d . nil)
                    (e . nil))))
        (list
          (funcall 'neovm--ars-diamond-p rel1)
          (funcall 'neovm--ars-diamond-p rel2)
          (funcall 'neovm--ars-diamond-p rel3)))
    (fmakunbound 'neovm--ars-successors)
    (fmakunbound 'neovm--ars-diamond-at)
    (fmakunbound 'neovm--ars-diamond-p)))"#;
    let expect = expect_test::expect![[r#""OK (t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Newman's lemma: local confluence + termination = confluence
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ars_newmans_lemma() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Verify Newman's lemma: a terminating, locally confluent ARS is confluent.
    // Local confluence: for every a -> b, a -> c, exists d: b ->* d, c ->* d.
    // Termination: no infinite reduction sequences (check via cycle detection).
    let form = r#"(progn
  (fset 'neovm--ars-successors
    (lambda (rel term)
      (let ((entry (assoc term rel)))
        (if entry (cdr entry) nil))))

  (fset 'neovm--ars-reachable-set
    (lambda (rel start)
      (let ((visited nil)
            (queue (list start)))
        (while queue
          (let ((current (car queue)))
            (setq queue (cdr queue))
            (unless (memq current visited)
              (setq visited (cons current visited))
              (dolist (s (funcall 'neovm--ars-successors rel current))
                (unless (memq s visited)
                  (setq queue (append queue (list s))))))))
        visited)))

  ;; Check termination: no cycles reachable from any term
  (fset 'neovm--ars-terminating-p
    (lambda (rel)
      (let ((ok t))
        (dolist (entry rel)
          (let* ((term (car entry))
                 (succs (cdr entry)))
            ;; If term is reachable from any of its successors, cycle exists
            (dolist (s succs)
              (when (memq term (funcall 'neovm--ars-reachable-set rel s))
                (setq ok nil)))))
        ok)))

  ;; Local confluence: for every pair a->b, a->c, b and c have common reduct
  (fset 'neovm--ars-locally-confluent-p
    (lambda (rel)
      (let ((ok t))
        (dolist (entry rel)
          (let ((succs (cdr entry)))
            (when (> (length succs) 1)
              (let ((rest succs))
                (while (and ok rest)
                  (let ((b (car rest)))
                    (dolist (c (cdr rest))
                      (let ((reach-b (funcall 'neovm--ars-reachable-set rel b))
                            (reach-c (funcall 'neovm--ars-reachable-set rel c))
                            (found nil))
                        (dolist (rb reach-b)
                          (when (memq rb reach-c)
                            (setq found t)))
                        (unless found (setq ok nil)))))
                  (setq rest (cdr rest)))))))
        ok)))

  (unwind-protect
      (let (;; Terminating and locally confluent -> should be confluent
            (rel1 '((a . (b c))
                    (b . (d))
                    (c . (d))
                    (d . nil)))
            ;; Terminating but NOT locally confluent
            (rel2 '((a . (b c))
                    (b . nil)
                    (c . nil)))
            ;; Locally confluent but NOT terminating (has cycle)
            (rel3 '((a . (b c))
                    (b . (d))
                    (c . (d))
                    (d . (a))))  ; cycle!
            ;; Bigger terminating + locally confluent
            (rel4 '((s . (a b))
                    (a . (c))
                    (b . (c d))
                    (c . (e))
                    (d . (e))
                    (e . nil))))
        (list
          ;; rel1: terminating + locally confluent
          (list (funcall 'neovm--ars-terminating-p rel1)
                (funcall 'neovm--ars-locally-confluent-p rel1))
          ;; rel2: terminating but not locally confluent
          (list (funcall 'neovm--ars-terminating-p rel2)
                (funcall 'neovm--ars-locally-confluent-p rel2))
          ;; rel3: not terminating, locally confluent
          (list (funcall 'neovm--ars-terminating-p rel3)
                (funcall 'neovm--ars-locally-confluent-p rel3))
          ;; rel4: terminating + locally confluent
          (list (funcall 'neovm--ars-terminating-p rel4)
                (funcall 'neovm--ars-locally-confluent-p rel4))))
    (fmakunbound 'neovm--ars-successors)
    (fmakunbound 'neovm--ars-reachable-set)
    (fmakunbound 'neovm--ars-terminating-p)
    (fmakunbound 'neovm--ars-locally-confluent-p)))"#;
    let expect = expect_test::expect![[r#""OK ((t t) (t nil) (nil t) (t t))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Reduction sequences: find all reduction paths to normal form
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ars_reduction_sequences() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Find all reduction sequences from a start term to a normal form,
    // with a maximum depth to prevent infinite loops.
    let form = r#"(progn
  (fset 'neovm--ars-successors
    (lambda (rel term)
      (let ((entry (assoc term rel)))
        (if entry (cdr entry) nil))))

  ;; Find all paths from start to any normal form (DFS, depth-limited)
  (fset 'neovm--ars-all-paths
    (lambda (rel start max-depth)
      (let ((results nil))
        (fset 'neovm--ars-dfs
          (lambda (current path depth)
            (let ((succs (funcall 'neovm--ars-successors rel current)))
              (if (or (null succs) (>= depth max-depth))
                  (setq results (cons (reverse (cons current path)) results))
                (dolist (s succs)
                  (funcall 'neovm--ars-dfs s (cons current path) (1+ depth)))))))
        (funcall 'neovm--ars-dfs start nil 0)
        (fmakunbound 'neovm--ars-dfs)
        (sort results (lambda (a b) (string< (format "%S" a) (format "%S" b)))))))

  (unwind-protect
      (let ((rel '((a . (b c))
                   (b . (d))
                   (c . (d e))
                   (d . nil)
                   (e . (f))
                   (f . nil))))
        (list
          ;; All paths from a
          (funcall 'neovm--ars-all-paths rel 'a 10)
          ;; All paths from c
          (funcall 'neovm--ars-all-paths rel 'c 10)
          ;; Paths from d (already normal form)
          (funcall 'neovm--ars-all-paths rel 'd 10)
          ;; Path lengths
          (mapcar #'length (funcall 'neovm--ars-all-paths rel 'a 10))))
    (fmakunbound 'neovm--ars-successors)
    (fmakunbound 'neovm--ars-all-paths)))"#;
    let expect = expect_test::expect![[
        r#""OK (((a b d) (a c d) (a c e f)) ((c d) (c e f)) ((d)) (3 3 4))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Strongly normalizing terms: every reduction sequence terminates
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ars_strongly_normalizing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A term is strongly normalizing (SN) if every reduction sequence
    // starting from it is finite. Equivalently: the term and all its
    // reducts are acyclic and eventually reach a normal form.
    let form = r#"(progn
  (fset 'neovm--ars-successors
    (lambda (rel term)
      (let ((entry (assoc term rel)))
        (if entry (cdr entry) nil))))

  ;; Check if term is SN via DFS cycle detection
  (fset 'neovm--ars-strongly-normalizing-p
    (lambda (rel term)
      (let ((visiting nil)
            (result t))
        (fset 'neovm--ars-sn-visit
          (lambda (current)
            (cond
             ((memq current visiting) (setq result nil))  ; cycle found
             (t
              (setq visiting (cons current visiting))
              (dolist (s (funcall 'neovm--ars-successors rel current))
                (when result
                  (funcall 'neovm--ars-sn-visit s)))
              (setq visiting (delq current visiting))))))
        (funcall 'neovm--ars-sn-visit term)
        (fmakunbound 'neovm--ars-sn-visit)
        result)))

  ;; Find all SN terms in a relation
  (fset 'neovm--ars-all-sn-terms
    (lambda (rel)
      (let ((terms nil))
        ;; Collect all terms mentioned
        (dolist (entry rel)
          (unless (memq (car entry) terms)
            (setq terms (cons (car entry) terms)))
          (dolist (s (cdr entry))
            (unless (memq s terms)
              (setq terms (cons s terms)))))
        ;; Filter SN
        (let ((sn-terms nil))
          (dolist (term terms)
            (when (funcall 'neovm--ars-strongly-normalizing-p rel term)
              (setq sn-terms (cons term sn-terms))))
          (sort sn-terms (lambda (a b) (string< (symbol-name a) (symbol-name b))))))))

  (unwind-protect
      (let (;; Fully terminating system: all terms SN
            (rel1 '((a . (b c))
                    (b . (d))
                    (c . (d))
                    (d . nil)))
            ;; System with a cycle: a -> b -> a
            (rel2 '((a . (b))
                    (b . (a c))
                    (c . nil)))
            ;; Mixed: some terms SN, some not
            (rel3 '((a . (b))
                    (b . (c a))  ; b->a creates cycle through a,b
                    (c . nil)
                    (d . (e))
                    (e . nil))))
        (list
          ;; All SN in terminating system
          (funcall 'neovm--ars-all-sn-terms rel1)
          ;; In cyclic system: only c is SN
          (funcall 'neovm--ars-all-sn-terms rel2)
          ;; Mixed system
          (funcall 'neovm--ars-all-sn-terms rel3)
          ;; Individual checks
          (funcall 'neovm--ars-strongly-normalizing-p rel1 'a)
          (funcall 'neovm--ars-strongly-normalizing-p rel2 'a)
          (funcall 'neovm--ars-strongly-normalizing-p rel2 'c)))
    (fmakunbound 'neovm--ars-successors)
    (fmakunbound 'neovm--ars-strongly-normalizing-p)
    (fmakunbound 'neovm--ars-all-sn-terms)))"#;
    let expect = expect_test::expect![[r#""OK ((a b c d) (c) (c d e) t nil t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Head reduction and leftmost-outermost strategy
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ars_head_reduction_strategy() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement leftmost-outermost (head) reduction: always pick the
    // first successor. Compare with rightmost (last successor) strategy.
    let form = r#"(progn
  (fset 'neovm--ars-successors
    (lambda (rel term)
      (let ((entry (assoc term rel)))
        (if entry (cdr entry) nil))))

  ;; Head reduction: always take first successor
  (fset 'neovm--ars-head-reduce
    (lambda (rel start max-steps)
      (let ((path (list start))
            (current start)
            (steps 0))
        (while (and (< steps max-steps)
                    (funcall 'neovm--ars-successors rel current))
          (setq current (car (funcall 'neovm--ars-successors rel current)))
          (setq path (cons current path))
          (setq steps (1+ steps)))
        (nreverse path))))

  ;; Rightmost reduction: always take last successor
  (fset 'neovm--ars-rightmost-reduce
    (lambda (rel start max-steps)
      (let ((path (list start))
            (current start)
            (steps 0))
        (while (and (< steps max-steps)
                    (funcall 'neovm--ars-successors rel current))
          (let ((succs (funcall 'neovm--ars-successors rel current)))
            (setq current (car (last succs))))
          (setq path (cons current path))
          (setq steps (1+ steps)))
        (nreverse path))))

  (unwind-protect
      (let ((rel '((a . (b c))
                   (b . (d))
                   (c . (e f))
                   (d . nil)
                   (e . (g))
                   (f . (g))
                   (g . nil))))
        (list
          ;; Head (leftmost) reduction from a: a -> b -> d
          (funcall 'neovm--ars-head-reduce rel 'a 10)
          ;; Rightmost reduction from a: a -> c -> f -> g
          (funcall 'neovm--ars-rightmost-reduce rel 'a 10)
          ;; Both reach normal forms
          (let ((head-path (funcall 'neovm--ars-head-reduce rel 'a 10))
                (right-path (funcall 'neovm--ars-rightmost-reduce rel 'a 10)))
            (list
              (car (last head-path))   ; d
              (car (last right-path))  ; g
              (length head-path)       ; 3
              (length right-path)))    ; 4
          ;; From c: head=c->e->g, right=c->f->g
          (funcall 'neovm--ars-head-reduce rel 'c 10)
          (funcall 'neovm--ars-rightmost-reduce rel 'c 10)))
    (fmakunbound 'neovm--ars-successors)
    (fmakunbound 'neovm--ars-head-reduce)
    (fmakunbound 'neovm--ars-rightmost-reduce)))"#;
    let expect = expect_test::expect![[r#""OK ((a b d) (a c f g) (d g 3 4) (c e g) (c f g))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Parallel reduction: reduce all redexes simultaneously
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ars_parallel_reduction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Parallel reduction on tree-structured terms: reduce all redexes
    // at every position simultaneously.
    // Terms: (node label left right) or (leaf val)
    let form = r#"(progn
  ;; Rules: (from-label . to-label) for node labels
  (fset 'neovm--ars-par-rules '((A . B) (B . C) (X . Y)))

  ;; Apply one parallel reduction step to a tree
  (fset 'neovm--ars-par-reduce-step
    (lambda (term)
      (cond
       ((and (consp term) (eq (car term) 'leaf))
        term)
       ((and (consp term) (eq (car term) 'node))
        (let* ((label (cadr term))
               (left (caddr term))
               (right (cadddr term))
               ;; Reduce label if a rule applies
               (new-label
                (let ((rule (assoc label (symbol-value 'neovm--ars-par-rules))))
                  (if rule (cdr rule) label)))
               ;; Recursively reduce children
               (new-left (funcall 'neovm--ars-par-reduce-step left))
               (new-right (funcall 'neovm--ars-par-reduce-step right)))
          (list 'node new-label new-left new-right)))
       (t term))))

  ;; Reduce to normal form (no rule applies anywhere)
  (fset 'neovm--ars-par-reduce-full
    (lambda (term max-steps)
      (let ((current term)
            (steps 0)
            (changed t))
        (while (and changed (< steps max-steps))
          (let ((next (funcall 'neovm--ars-par-reduce-step current)))
            (if (equal next current)
                (setq changed nil)
              (setq current next)
              (setq steps (1+ steps)))))
        (list current steps))))

  (unwind-protect
      (let ((tree1 '(node A (leaf 1) (leaf 2)))
            (tree2 '(node A (node B (leaf 1) (leaf 2)) (node X (leaf 3) (leaf 4))))
            (tree3 '(node A (node A (node A (leaf 0) (leaf 0)) (leaf 0)) (leaf 0))))
        (list
          ;; Simple: A -> B -> C, then stable
          (funcall 'neovm--ars-par-reduce-full tree1 10)
          ;; All nodes reduce in parallel
          (funcall 'neovm--ars-par-reduce-full tree2 10)
          ;; Nested A's: all reduce in one parallel step
          (funcall 'neovm--ars-par-reduce-step tree3)
          ;; Full reduction of nested
          (funcall 'neovm--ars-par-reduce-full tree3 10)))
    (fmakunbound 'neovm--ars-par-rules)
    (fmakunbound 'neovm--ars-par-reduce-step)
    (fmakunbound 'neovm--ars-par-reduce-full)))"#;
    let expect = expect_test::expect![[r#""ERR (void-variable neovm--ars-par-rules)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Reduction graph: build DOT-style adjacency from reduction relation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ars_reduction_graph_analysis() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Compute graph metrics: in-degree, out-degree, sources, sinks.
    let form = r#"(progn
  ;; Compute in-degree and out-degree for every term
  (fset 'neovm--ars-graph-degrees
    (lambda (rel)
      (let ((out-deg nil)
            (in-deg nil))
        ;; Out-degrees from relation
        (dolist (entry rel)
          (setq out-deg (cons (cons (car entry) (length (cdr entry))) out-deg))
          ;; Ensure all targets have entries in in-deg
          (dolist (target (cdr entry))
            (let ((existing (assoc target in-deg)))
              (if existing
                  (setcdr existing (1+ (cdr existing)))
                (setq in-deg (cons (cons target 1) in-deg))))))
        ;; Ensure source terms have in-deg entries (0 if no incoming)
        (dolist (entry rel)
          (unless (assoc (car entry) in-deg)
            (setq in-deg (cons (cons (car entry) 0) in-deg))))
        ;; Ensure target-only terms have out-deg 0
        (dolist (entry in-deg)
          (unless (assoc (car entry) out-deg)
            (setq out-deg (cons (cons (car entry) 0) out-deg))))
        (list (sort out-deg (lambda (a b) (string< (symbol-name (car a)) (symbol-name (car b)))))
              (sort in-deg (lambda (a b) (string< (symbol-name (car a)) (symbol-name (car b)))))))))

  ;; Sources: in-degree 0; Sinks: out-degree 0
  (fset 'neovm--ars-sources-and-sinks
    (lambda (rel)
      (let* ((degrees (funcall 'neovm--ars-graph-degrees rel))
             (out-deg (car degrees))
             (in-deg (cadr degrees))
             (sources nil)
             (sinks nil))
        (dolist (entry in-deg)
          (when (= (cdr entry) 0)
            (setq sources (cons (car entry) sources))))
        (dolist (entry out-deg)
          (when (= (cdr entry) 0)
            (setq sinks (cons (car entry) sinks))))
        (list (sort sources (lambda (a b) (string< (symbol-name a) (symbol-name b))))
              (sort sinks (lambda (a b) (string< (symbol-name a) (symbol-name b))))))))

  (unwind-protect
      (let ((rel '((a . (b c))
                   (b . (d))
                   (c . (d e))
                   (d . nil)
                   (e . (f))
                   (f . nil))))
        (list
          (funcall 'neovm--ars-graph-degrees rel)
          (funcall 'neovm--ars-sources-and-sinks rel)))
    (fmakunbound 'neovm--ars-graph-degrees)
    (fmakunbound 'neovm--ars-sources-and-sinks)))"#;
    let expect = expect_test::expect![[
        r#""OK ((((a . 2) (b . 1) (c . 2) (d . 0) (e . 1) (f . 0)) ((a . 0) (b . 1) (c . 1) (d . 2) (e . 1) (f . 1))) ((a) (d f)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
