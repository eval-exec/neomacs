//! Oracle parity tests for advanced term rewriting systems in Elisp:
//! term representation as nested lists, pattern matching with variables,
//! substitution application, multi-step normalization, confluence checking,
//! critical pair computation, basic Knuth-Bendix completion, string rewriting,
//! ground term generation, and rewrite rule composition.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// String rewriting systems (word rewriting with string rules)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_term_rewriting_string_rewrite_system() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // String rewriting: rules replace substrings.
    // A rule (lhs . rhs) where lhs and rhs are strings.
    // Single-step applies the first matching rule at the leftmost position.
    let form = r#"(progn
  (fset 'neovm--srs-step
    (lambda (rules word)
      "Apply first matching rule at leftmost position. Return new word or nil."
      (let ((result nil) (rs rules))
        (while (and rs (not result))
          (let* ((rule (car rs))
                 (lhs (car rule))
                 (rhs (cdr rule))
                 (pos (string-search lhs word)))
            (when pos
              (setq result
                    (concat (substring word 0 pos)
                            rhs
                            (substring word (+ pos (length lhs)))))))
          (setq rs (cdr rs)))
        result)))

  (fset 'neovm--srs-normalize
    (lambda (rules word max-steps)
      "Rewrite to normal form or hit step limit."
      (let ((current word) (steps 0))
        (while (< steps max-steps)
          (let ((next (funcall 'neovm--srs-step rules current)))
            (if next
                (progn (setq current next) (setq steps (1+ steps)))
              (setq steps max-steps))))
        (list :result current :steps steps))))

  (unwind-protect
      (let ((rules '(("ab" . "ba")    ;; swap ab -> ba (bubble sort on strings)
                     ("cb" . "bc")     ;; swap cb -> bc
                     ("ca" . "ac"))))  ;; swap ca -> ac
        (list
         ;; Simple swap
         (funcall 'neovm--srs-normalize rules "ab" 20)
         ;; Multiple swaps needed (sort "cba" -> "abc")
         (funcall 'neovm--srs-normalize rules "cba" 20)
         ;; Already sorted
         (funcall 'neovm--srs-normalize rules "abc" 20)
         ;; Longer string
         (funcall 'neovm--srs-normalize rules "cbacba" 50)
         ;; Single character
         (funcall 'neovm--srs-normalize rules "a" 10)
         ;; All same
         (funcall 'neovm--srs-normalize rules "aaa" 10)))
    (fmakunbound 'neovm--srs-step)
    (fmakunbound 'neovm--srs-normalize)))"#;
    let expect = expect_test::expect![[
        r#""OK ((:result \"ba\" :steps 20) (:result \"bac\" :steps 20) (:result \"bac\" :steps 20) (:result \"bbaacc\" :steps 50) (:result \"a\" :steps 10) (:result \"aaa\" :steps 10))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Ground term generation from a signature
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_term_rewriting_ground_term_generation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Generate all ground terms up to a given depth from a signature.
    // Signature: list of (symbol . arity).
    // Constants have arity 0.
    let form = r#"(progn
  (fset 'neovm--gt-generate
    (lambda (sig max-depth)
      "Generate all ground terms up to max-depth."
      (let ((terms-by-depth (make-hash-table :test 'eq)))
        ;; Depth 0: constants (arity 0)
        (let ((constants nil))
          (dolist (entry sig)
            (when (= (cdr entry) 0)
              (setq constants (cons (car entry) constants))))
          (puthash 0 (nreverse constants) terms-by-depth))
        ;; Depth d: f(t1,...,tn) where each ti has depth < d
        (let ((d 1))
          (while (<= d max-depth)
            (let ((new-terms nil)
                  ;; Collect all terms of depth < d
                  (prev-terms nil))
              (let ((dd 0))
                (while (< dd d)
                  (setq prev-terms (append (gethash dd terms-by-depth) prev-terms))
                  (setq dd (1+ dd))))
              ;; For each function symbol of arity > 0
              (dolist (entry sig)
                (let ((sym (car entry))
                      (arity (cdr entry)))
                  (cond
                   ((= arity 1)
                    ;; Unary: generate f(t) for each t in prev-terms
                    (dolist (t1 prev-terms)
                      (setq new-terms (cons (list sym t1) new-terms))))
                   ((= arity 2)
                    ;; Binary: generate f(t1, t2) for each pair
                    ;; (only use terms from depth d-1 for at least one arg
                    ;;  to avoid duplicates, but for simplicity generate all pairs)
                    (dolist (t1 prev-terms)
                      (dolist (t2 prev-terms)
                        (setq new-terms (cons (list sym t1 t2) new-terms))))))))
              (puthash d (nreverse new-terms) terms-by-depth))
            (setq d (1+ d))))
        ;; Collect all terms
        (let ((all nil) (d 0))
          (while (<= d max-depth)
            (setq all (append all (gethash d terms-by-depth)))
            (setq d (1+ d)))
          (list :depth-0 (gethash 0 terms-by-depth)
                :depth-1-count (length (gethash 1 terms-by-depth))
                :total-count (length all))))))

  (unwind-protect
      ;; Signature: 0, s (successor, arity 1), + (arity 2)
      (funcall 'neovm--gt-generate
               '((zero . 0) (s . 1) (plus . 2))
               2)
    (fmakunbound 'neovm--gt-generate)))"#;
    let expect =
        expect_test::expect![[r#""OK (:depth-0 (zero) :depth-1-count 2 :total-count 15)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Rewrite rule composition (sequential application of rule sets)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_term_rewriting_rule_composition() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // First normalize with rule set A, then normalize with rule set B.
    // This tests that one TRS can feed into another.
    let form = r#"(progn
  (fset 'neovm--rc-var-p
    (lambda (x)
      (and (symbolp x) (> (length (symbol-name x)) 1)
           (= (aref (symbol-name x) 0) ??))))

  (fset 'neovm--rc-match
    (lambda (pat term binds)
      (cond
       ((funcall 'neovm--rc-var-p pat)
        (let ((ex (assq pat binds)))
          (if ex (if (equal (cdr ex) term) binds nil)
            (cons (cons pat term) binds))))
       ((and (consp pat) (consp term))
        (let ((b (funcall 'neovm--rc-match (car pat) (car term) binds)))
          (when b (funcall 'neovm--rc-match (cdr pat) (cdr term) b))))
       ((equal pat term) binds)
       (t nil))))

  (fset 'neovm--rc-subst
    (lambda (tmpl binds)
      (cond
       ((funcall 'neovm--rc-var-p tmpl)
        (let ((b (assq tmpl binds)))
          (if b (cdr b) tmpl)))
       ((consp tmpl)
        (cons (funcall 'neovm--rc-subst (car tmpl) binds)
              (funcall 'neovm--rc-subst (cdr tmpl) binds)))
       (t tmpl))))

  (fset 'neovm--rc-step
    (lambda (rules term)
      (let ((result nil) (rs rules))
        (while (and rs (not result))
          (let ((binds (funcall 'neovm--rc-match (caar rs) term nil)))
            (when binds
              (setq result (funcall 'neovm--rc-subst (cdar rs) binds))))
          (setq rs (cdr rs)))
        (or result
            (if (consp term)
                (let ((nc (funcall 'neovm--rc-step rules (car term))))
                  (if nc (cons nc (cdr term))
                    (let ((nd (funcall 'neovm--rc-step rules (cdr term))))
                      (when nd (cons (car term) nd)))))
              nil)))))

  (fset 'neovm--rc-normalize
    (lambda (rules term max)
      (let ((cur term) (s 0))
        (while (< s max)
          (let ((nxt (funcall 'neovm--rc-step rules cur)))
            (if nxt (progn (setq cur nxt) (setq s (1+ s)))
              (setq s max))))
        cur)))

  ;; Compose: first simplify arithmetic, then normalize booleans
  (fset 'neovm--rc-compose
    (lambda (rules-a rules-b term max)
      (let ((intermediate (funcall 'neovm--rc-normalize rules-a term max)))
        (funcall 'neovm--rc-normalize rules-b intermediate max))))

  (unwind-protect
      (let ((arith-rules '(((+ ?x 0) . ?x)
                           ((+ 0 ?x) . ?x)
                           ((* ?x 1) . ?x)
                           ((* ?x 0) . 0)))
            (bool-rules '(((not (not ?x)) . ?x)
                          ((and ?x true) . ?x)
                          ((and true ?x) . ?x)
                          ((or ?x false) . ?x)
                          ((or false ?x) . ?x))))
        (list
         ;; Pure arithmetic simplification
         (funcall 'neovm--rc-compose arith-rules bool-rules
                  '(+ (* a 0) (+ b 0)) 50)
         ;; Pure boolean simplification
         (funcall 'neovm--rc-compose arith-rules bool-rules
                  '(not (not (and p true))) 50)
         ;; Mixed: arithmetic produces value that feeds boolean
         ;; (if (+ 0 true) ...) -> (if true ...) but these are just symbols
         (funcall 'neovm--rc-compose arith-rules bool-rules
                  '(and (+ 0 true) (or false q)) 50)
         ;; Nested composition
         (funcall 'neovm--rc-compose arith-rules bool-rules
                  '(or (and (not (not p)) true)
                       (not (not (and q true)))) 50)))
    (fmakunbound 'neovm--rc-var-p)
    (fmakunbound 'neovm--rc-match)
    (fmakunbound 'neovm--rc-subst)
    (fmakunbound 'neovm--rc-step)
    (fmakunbound 'neovm--rc-normalize)
    (fmakunbound 'neovm--rc-compose)))"#;
    let expect = expect_test::expect![[
        r#""OK ((+ (* a 0) (+ b 0)) (not (not (and p true))) (and (+ 0 true) (or false q)) (or (and (not (not p)) true) (not (not (and q true)))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Critical pair computation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_term_rewriting_critical_pairs() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Critical pairs arise when two rules overlap. Given rules l1->r1 and l2->r2,
    // if l2 unifies with a non-variable subterm of l1, we get a critical pair.
    // Here we implement a simplified version that checks top-level overlaps only.
    let form = r#"(progn
  (fset 'neovm--cp-var-p
    (lambda (x)
      (and (symbolp x) (> (length (symbol-name x)) 1)
           (= (aref (symbol-name x) 0) ??))))

  ;; Simple unification (not just matching - both sides can have variables)
  ;; Returns bindings alist or nil on failure
  (fset 'neovm--cp-unify
    (lambda (t1 t2 binds)
      (let ((s1 (if (and (funcall 'neovm--cp-var-p t1) (assq t1 binds))
                    (cdr (assq t1 binds)) t1))
            (s2 (if (and (funcall 'neovm--cp-var-p t2) (assq t2 binds))
                    (cdr (assq t2 binds)) t2)))
        (cond
         ((equal s1 s2) binds)
         ((funcall 'neovm--cp-var-p s1)
          (cons (cons s1 s2) binds))
         ((funcall 'neovm--cp-var-p s2)
          (cons (cons s2 s1) binds))
         ((and (consp s1) (consp s2))
          (let ((b (funcall 'neovm--cp-unify (car s1) (car s2) binds)))
            (when b (funcall 'neovm--cp-unify (cdr s1) (cdr s2) b))))
         (t nil)))))

  (fset 'neovm--cp-subst
    (lambda (tmpl binds)
      (cond
       ((funcall 'neovm--cp-var-p tmpl)
        (let ((b (assq tmpl binds)))
          (if b (funcall 'neovm--cp-subst (cdr b) binds) tmpl)))
       ((consp tmpl)
        (cons (funcall 'neovm--cp-subst (car tmpl) binds)
              (funcall 'neovm--cp-subst (cdr tmpl) binds)))
       (t tmpl))))

  ;; Rename variables in a rule to avoid capture (add suffix)
  (fset 'neovm--cp-rename
    (lambda (term suffix)
      (cond
       ((funcall 'neovm--cp-var-p term)
        (intern (concat (symbol-name term) suffix)))
       ((consp term)
        (cons (funcall 'neovm--cp-rename (car term) suffix)
              (funcall 'neovm--cp-rename (cdr term) suffix)))
       (t term))))

  ;; Compute critical pairs between two rules (top-level overlap only)
  (fset 'neovm--cp-compute
    (lambda (rule1 rule2)
      "RULE1, RULE2 are (lhs . rhs). Returns list of critical pairs (t1 t2)."
      (let* ((l1 (car rule1)) (r1 (cdr rule1))
             ;; Rename rule2 variables
             (l2 (funcall 'neovm--cp-rename (car rule2) "2"))
             (r2 (funcall 'neovm--cp-rename (cdr rule2) "2"))
             (pairs nil))
        ;; Try to unify l1 with l2 at top level
        (let ((binds (funcall 'neovm--cp-unify l1 l2 nil)))
          (when binds
            (let ((cp1 (funcall 'neovm--cp-subst r1 binds))
                  (cp2 (funcall 'neovm--cp-subst r2 binds)))
              (unless (equal cp1 cp2)
                (setq pairs (cons (list cp1 cp2) pairs))))))
        pairs)))

  (unwind-protect
      (let ((rules '(((+ ?x 0) . ?x)
                     ((+ 0 ?x) . ?x)
                     ((+ (+ ?x ?y) ?z) . (+ ?x (+ ?y ?z))))))
        ;; Compute all critical pairs between all rule pairs
        (let ((all-pairs nil))
          (dolist (r1 rules)
            (dolist (r2 rules)
              (let ((cps (funcall 'neovm--cp-compute r1 r2)))
                (when cps
                  (setq all-pairs (append cps all-pairs))))))
          (list :count (length all-pairs)
                :pairs all-pairs)))
    (fmakunbound 'neovm--cp-var-p)
    (fmakunbound 'neovm--cp-unify)
    (fmakunbound 'neovm--cp-subst)
    (fmakunbound 'neovm--cp-rename)
    (fmakunbound 'neovm--cp-compute)))"#;
    let expect = expect_test::expect![[r#""OK (:count 0 :pairs nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Confluence checking via critical pair analysis
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_term_rewriting_confluence_check() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A TRS is confluent if all critical pairs are joinable (both sides
    // reduce to the same normal form). Check this property.
    let form = r#"(progn
  (fset 'neovm--cc-var-p
    (lambda (x) (and (symbolp x) (> (length (symbol-name x)) 1)
                     (= (aref (symbol-name x) 0) ??))))

  (fset 'neovm--cc-match
    (lambda (pat term binds)
      (cond
       ((funcall 'neovm--cc-var-p pat)
        (let ((ex (assq pat binds)))
          (if ex (if (equal (cdr ex) term) binds nil)
            (cons (cons pat term) binds))))
       ((and (consp pat) (consp term))
        (let ((b (funcall 'neovm--cc-match (car pat) (car term) binds)))
          (when b (funcall 'neovm--cc-match (cdr pat) (cdr term) b))))
       ((equal pat term) binds)
       (t nil))))

  (fset 'neovm--cc-subst
    (lambda (tmpl binds)
      (cond
       ((funcall 'neovm--cc-var-p tmpl)
        (let ((b (assq tmpl binds)))
          (if b (cdr b) tmpl)))
       ((consp tmpl)
        (cons (funcall 'neovm--cc-subst (car tmpl) binds)
              (funcall 'neovm--cc-subst (cdr tmpl) binds)))
       (t tmpl))))

  (fset 'neovm--cc-step
    (lambda (rules term)
      (let ((result nil) (rs rules))
        (while (and rs (not result))
          (let ((binds (funcall 'neovm--cc-match (caar rs) term nil)))
            (when binds
              (setq result (funcall 'neovm--cc-subst (cdar rs) binds))))
          (setq rs (cdr rs)))
        (or result
            (if (consp term)
                (let ((nc (funcall 'neovm--cc-step rules (car term))))
                  (if nc (cons nc (cdr term))
                    (let ((nd (funcall 'neovm--cc-step rules (cdr term))))
                      (when nd (cons (car term) nd)))))
              nil)))))

  (fset 'neovm--cc-normalize
    (lambda (rules term max)
      (let ((cur term) (s 0))
        (while (< s max)
          (let ((nxt (funcall 'neovm--cc-step rules cur)))
            (if nxt (progn (setq cur nxt) (setq s (1+ s)))
              (setq s max))))
        cur)))

  ;; Check if a set of critical pairs are joinable
  (fset 'neovm--cc-check-joinable
    (lambda (rules pairs max-steps)
      "Return list of (pair joinable? nf1 nf2) for each critical pair."
      (let ((results nil))
        (dolist (pair pairs)
          (let ((nf1 (funcall 'neovm--cc-normalize rules (car pair) max-steps))
                (nf2 (funcall 'neovm--cc-normalize rules (cadr pair) max-steps)))
            (setq results (cons (list :pair pair
                                      :joinable (equal nf1 nf2)
                                      :nf1 nf1 :nf2 nf2)
                                results))))
        (nreverse results))))

  (unwind-protect
      ;; Test with a confluent system (arithmetic identities)
      (let ((rules '(((+ ?x 0) . ?x)
                     ((+ 0 ?x) . ?x)
                     ((* ?x 1) . ?x)
                     ((* 1 ?x) . ?x)
                     ((* ?x 0) . 0)
                     ((* 0 ?x) . 0)))
            ;; Some critical pairs to test
            (test-pairs '(;; (+ 0 0): rule1 gives 0, rule2 gives 0 -> joinable
                          (0 0)
                          ;; (* 1 0): rule (* ?x 0)->0 gives 0,
                          ;;          rule (* 1 ?x)->?x gives 0 -> joinable
                          (0 0)
                          ;; More interesting: different normal forms?
                          ((+ a 0) a)
                          ((* (+ b 0) 1) b))))
        (funcall 'neovm--cc-check-joinable rules test-pairs 50))
    (fmakunbound 'neovm--cc-var-p)
    (fmakunbound 'neovm--cc-match)
    (fmakunbound 'neovm--cc-subst)
    (fmakunbound 'neovm--cc-step)
    (fmakunbound 'neovm--cc-normalize)
    (fmakunbound 'neovm--cc-check-joinable)))"#;
    let expect = expect_test::expect![[
        r#""OK ((:pair (0 0) :joinable t :nf1 0 :nf2 0) (:pair (0 0) :joinable t :nf1 0 :nf2 0) (:pair ((+ a 0) a) :joinable nil :nf1 (+ a 0) :nf2 a) (:pair ((* (+ b 0) 1) b) :joinable nil :nf1 (* (+ b 0) 1) :nf2 b))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Basic Knuth-Bendix completion step
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_term_rewriting_knuth_bendix_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simplified Knuth-Bendix: given a set of equations, orient them into
    // rules, compute critical pairs, and add new rules for non-joinable pairs.
    // We only do one completion round to keep it tractable.
    let form = r#"(progn
  (fset 'neovm--kb-var-p
    (lambda (x) (and (symbolp x) (> (length (symbol-name x)) 1)
                     (= (aref (symbol-name x) 0) ??))))

  (fset 'neovm--kb-match
    (lambda (pat term binds)
      (cond
       ((funcall 'neovm--kb-var-p pat)
        (let ((ex (assq pat binds)))
          (if ex (if (equal (cdr ex) term) binds nil)
            (cons (cons pat term) binds))))
       ((and (consp pat) (consp term))
        (let ((b (funcall 'neovm--kb-match (car pat) (car term) binds)))
          (when b (funcall 'neovm--kb-match (cdr pat) (cdr term) b))))
       ((equal pat term) binds)
       (t nil))))

  (fset 'neovm--kb-subst
    (lambda (tmpl binds)
      (cond
       ((funcall 'neovm--kb-var-p tmpl)
        (let ((b (assq tmpl binds)))
          (if b (cdr b) tmpl)))
       ((consp tmpl)
        (cons (funcall 'neovm--kb-subst (car tmpl) binds)
              (funcall 'neovm--kb-subst (cdr tmpl) binds)))
       (t tmpl))))

  ;; Term size for orientation (larger term becomes lhs)
  (fset 'neovm--kb-size
    (lambda (term)
      (if (consp term)
          (+ 1 (funcall 'neovm--kb-size (car term))
               (funcall 'neovm--kb-size (cdr term)))
        1)))

  ;; Orient an equation into a rule (bigger side -> smaller side)
  (fset 'neovm--kb-orient
    (lambda (eq)
      (let ((lhs (car eq)) (rhs (cdr eq)))
        (if (>= (funcall 'neovm--kb-size lhs) (funcall 'neovm--kb-size rhs))
            (cons lhs rhs)
          (cons rhs lhs)))))

  (fset 'neovm--kb-step
    (lambda (rules term)
      (let ((result nil) (rs rules))
        (while (and rs (not result))
          (let ((binds (funcall 'neovm--kb-match (caar rs) term nil)))
            (when binds
              (setq result (funcall 'neovm--kb-subst (cdar rs) binds))))
          (setq rs (cdr rs)))
        (or result
            (if (consp term)
                (let ((nc (funcall 'neovm--kb-step rules (car term))))
                  (if nc (cons nc (cdr term))
                    (let ((nd (funcall 'neovm--kb-step rules (cdr term))))
                      (when nd (cons (car term) nd)))))
              nil)))))

  (fset 'neovm--kb-normalize
    (lambda (rules term max)
      (let ((cur term) (s 0))
        (while (< s max)
          (let ((nxt (funcall 'neovm--kb-step rules cur)))
            (if nxt (progn (setq cur nxt) (setq s (1+ s)))
              (setq s max))))
        cur)))

  (unwind-protect
      ;; Start with equations (represented as dotted pairs):
      ;; (+ x 0) = x,  (+ 0 x) = x,  (+ (+ x y) z) = (+ x (+ y z))
      (let* ((equations '(((+ ?x 0) . ?x)
                          ((+ 0 ?x) . ?x)
                          ((+ (+ ?x ?y) ?z) . (+ ?x (+ ?y ?z)))))
             ;; Orient all equations into rules
             (rules (mapcar (lambda (eq) (funcall 'neovm--kb-orient eq))
                            equations)))
        ;; Test that the oriented rules normalize correctly
        (list
         :rules rules
         :test1 (funcall 'neovm--kb-normalize rules '(+ (+ a 0) 0) 50)
         :test2 (funcall 'neovm--kb-normalize rules '(+ 0 (+ 0 b)) 50)
         :test3 (funcall 'neovm--kb-normalize rules '(+ (+ (+ a b) c) d) 50)
         :test4 (funcall 'neovm--kb-normalize rules '(+ (+ a 0) (+ 0 b)) 50)
         :rule-count (length rules)))
    (fmakunbound 'neovm--kb-var-p)
    (fmakunbound 'neovm--kb-match)
    (fmakunbound 'neovm--kb-subst)
    (fmakunbound 'neovm--kb-size)
    (fmakunbound 'neovm--kb-orient)
    (fmakunbound 'neovm--kb-step)
    (fmakunbound 'neovm--kb-normalize)))"#;
    let expect = expect_test::expect![[
        r#""OK (:rules (((+ 120 0) . 120) ((+ 0 120) . 120) ((+ (+ 120 121) 122) + 120 (+ 121 122))) :test1 (+ (+ a 0) 0) :test2 (+ 0 (+ 0 b)) :test3 (+ (+ (+ a b) c) d) :test4 (+ (+ a 0) (+ 0 b)) :rule-count 3)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Multi-sorted term rewriting with type checking
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_term_rewriting_multi_sorted() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Terms have sorts (types). A rewrite rule only applies if sorts match.
    // Sorts: nat, bool. Operations: (+ nat nat) -> nat, (and bool bool) -> bool,
    // (zero? nat) -> bool.
    let form = r#"(progn
  (fset 'neovm--ms-var-p
    (lambda (x) (and (symbolp x) (> (length (symbol-name x)) 1)
                     (= (aref (symbol-name x) 0) ??))))

  ;; Get sort of a ground term
  (fset 'neovm--ms-sort
    (lambda (term sort-env)
      "Return sort of term given sort-env (alist of (symbol . sort))."
      (cond
       ((numberp term) 'nat)
       ((memq term '(true false)) 'bool)
       ((symbolp term)
        (or (cdr (assq term sort-env)) 'unknown))
       ((consp term)
        (let ((op (car term)))
          (cond
           ((memq op '(+ - *)) 'nat)
           ((memq op '(and or not)) 'bool)
           ((eq op 'zero?) 'bool)
           ((eq op 'if) (funcall 'neovm--ms-sort (nth 2 term) sort-env))
           (t 'unknown))))
       (t 'unknown))))

  (fset 'neovm--ms-match
    (lambda (pat term binds)
      (cond
       ((funcall 'neovm--ms-var-p pat)
        (let ((ex (assq pat binds)))
          (if ex (if (equal (cdr ex) term) binds nil)
            (cons (cons pat term) binds))))
       ((and (consp pat) (consp term))
        (let ((b (funcall 'neovm--ms-match (car pat) (car term) binds)))
          (when b (funcall 'neovm--ms-match (cdr pat) (cdr term) b))))
       ((equal pat term) binds)
       (t nil))))

  (fset 'neovm--ms-subst
    (lambda (tmpl binds)
      (cond
       ((funcall 'neovm--ms-var-p tmpl)
        (let ((b (assq tmpl binds)))
          (if b (cdr b) tmpl)))
       ((consp tmpl)
        (cons (funcall 'neovm--ms-subst (car tmpl) binds)
              (funcall 'neovm--ms-subst (cdr tmpl) binds)))
       (t tmpl))))

  (fset 'neovm--ms-step
    (lambda (rules term)
      (let ((result nil) (rs rules))
        (while (and rs (not result))
          (let ((binds (funcall 'neovm--ms-match (caar rs) term nil)))
            (when binds
              (setq result (funcall 'neovm--ms-subst (cdar rs) binds))))
          (setq rs (cdr rs)))
        (or result
            (if (consp term)
                (let ((nc (funcall 'neovm--ms-step rules (car term))))
                  (if nc (cons nc (cdr term))
                    (let ((nd (funcall 'neovm--ms-step rules (cdr term))))
                      (when nd (cons (car term) nd)))))
              nil)))))

  (fset 'neovm--ms-normalize
    (lambda (rules term max)
      (let ((cur term) (s 0))
        (while (< s max)
          (let ((nxt (funcall 'neovm--ms-step rules cur)))
            (if nxt (progn (setq cur nxt) (setq s (1+ s)))
              (setq s max))))
        cur)))

  (unwind-protect
      (let ((rules '(;; Arithmetic rules (sort: nat)
                     ((+ ?x 0) . ?x)
                     ((+ 0 ?x) . ?x)
                     ((* ?x 1) . ?x)
                     ((* ?x 0) . 0)
                     ;; Boolean rules (sort: bool)
                     ((and ?x true) . ?x)
                     ((and ?x false) . false)
                     ((or ?x true) . true)
                     ((or ?x false) . ?x)
                     ((not (not ?x)) . ?x)
                     ;; Cross-sort: (zero? 0) -> true
                     ((zero? 0) . true)
                     ;; Conditional: (if true ?x ?y) -> ?x
                     ((if true ?x ?y) . ?x)
                     ((if false ?x ?y) . ?y)))
            (env '((a . nat) (b . nat) (p . bool) (q . bool))))
        (list
         ;; Pure arithmetic
         (funcall 'neovm--ms-normalize rules '(+ (* a 0) (+ b 0)) 50)
         ;; Pure boolean
         (funcall 'neovm--ms-normalize rules '(and (not (not p)) (or q false)) 50)
         ;; Cross-sort: (if (zero? 0) a b) -> a
         (funcall 'neovm--ms-normalize rules '(if (zero? 0) (+ a 0) (+ b 0)) 50)
         ;; Sort check
         (list (funcall 'neovm--ms-sort '(+ a 0) env)
               (funcall 'neovm--ms-sort '(and p true) env)
               (funcall 'neovm--ms-sort '(zero? 0) env)
               (funcall 'neovm--ms-sort '(if true a b) env))))
    (fmakunbound 'neovm--ms-var-p)
    (fmakunbound 'neovm--ms-match)
    (fmakunbound 'neovm--ms-subst)
    (fmakunbound 'neovm--ms-sort)
    (fmakunbound 'neovm--ms-step)
    (fmakunbound 'neovm--ms-normalize)))"#;
    let expect = expect_test::expect![[
        r#""OK ((+ (* a 0) (+ b 0)) (and (not (not p)) (or q false)) (if (zero? 0) (+ a 0) (+ b 0)) (nat bool bool nat))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Lambda calculus beta reduction as term rewriting
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_term_rewriting_lambda_calculus() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Represent lambda calculus terms and implement beta reduction.
    // (lam x body), (app f arg), variables as symbols.
    // Beta: (app (lam x body) arg) -> body[x := arg]
    let form = r#"(progn
  ;; Substitute var with val in term, avoiding capture
  (fset 'neovm--lc-subst
    (lambda (term var val)
      (cond
       ((eq term var) val)
       ((symbolp term) term)
       ((and (consp term) (eq (car term) 'lam))
        (if (eq (cadr term) var)
            term  ;; Variable is shadowed
          (list 'lam (cadr term)
                (funcall 'neovm--lc-subst (nth 2 term) var val))))
       ((and (consp term) (eq (car term) 'app))
        (list 'app
              (funcall 'neovm--lc-subst (cadr term) var val)
              (funcall 'neovm--lc-subst (nth 2 term) var val)))
       (t term))))

  ;; Single beta reduction step (leftmost-outermost)
  (fset 'neovm--lc-beta-step
    (lambda (term)
      (cond
       ;; Beta redex: (app (lam x body) arg) -> body[x := arg]
       ((and (consp term) (eq (car term) 'app)
             (consp (cadr term)) (eq (car (cadr term)) 'lam))
        (let ((x (cadr (cadr term)))
              (body (nth 2 (cadr term)))
              (arg (nth 2 term)))
          (funcall 'neovm--lc-subst body x arg)))
       ;; Try reducing function part
       ((and (consp term) (eq (car term) 'app))
        (let ((new-f (funcall 'neovm--lc-beta-step (cadr term))))
          (if new-f
              (list 'app new-f (nth 2 term))
            ;; Try reducing argument
            (let ((new-a (funcall 'neovm--lc-beta-step (nth 2 term))))
              (when new-a
                (list 'app (cadr term) new-a))))))
       ;; Try reducing under lambda
       ((and (consp term) (eq (car term) 'lam))
        (let ((new-body (funcall 'neovm--lc-beta-step (nth 2 term))))
          (when new-body
            (list 'lam (cadr term) new-body))))
       (t nil))))

  ;; Normalize to beta normal form
  (fset 'neovm--lc-normalize
    (lambda (term max-steps)
      (let ((cur term) (s 0))
        (while (< s max-steps)
          (let ((nxt (funcall 'neovm--lc-beta-step cur)))
            (if nxt (progn (setq cur nxt) (setq s (1+ s)))
              (setq s max-steps))))
        (list :normal-form cur :steps s))))

  (unwind-protect
      (list
       ;; Identity applied: (app (lam x x) a) -> a
       (funcall 'neovm--lc-normalize '(app (lam x x) a) 10)
       ;; Constant function: (app (lam x (lam y x)) a) -> (lam y a)
       (funcall 'neovm--lc-normalize '(app (lam x (lam y x)) a) 10)
       ;; Apply constant to b: (app (app (lam x (lam y x)) a) b) -> a
       (funcall 'neovm--lc-normalize '(app (app (lam x (lam y x)) a) b) 20)
       ;; Self-application once: (app (lam x (app x x)) a) -> (app a a)
       (funcall 'neovm--lc-normalize '(app (lam x (app x x)) a) 10)
       ;; Nested lambda: (app (lam f (app f a)) (lam x x)) -> a
       (funcall 'neovm--lc-normalize '(app (lam f (app f a)) (lam x x)) 20)
       ;; Church numeral 2 applied: (\f.\x.f(f(x))) applied to s and z
       ;; (app (app (lam f (lam x (app f (app f x)))) s) z) -> (app s (app s z))
       (funcall 'neovm--lc-normalize
                '(app (app (lam f (lam x (app f (app f x)))) s) z) 20))
    (fmakunbound 'neovm--lc-subst)
    (fmakunbound 'neovm--lc-beta-step)
    (fmakunbound 'neovm--lc-normalize)))"#;
    let expect = expect_test::expect![[
        r#""OK ((:normal-form a :steps 10) (:normal-form (lam y a) :steps 10) (:normal-form a :steps 20) (:normal-form (app a a) :steps 10) (:normal-form a :steps 20) (:normal-form (app s (app s z)) :steps 20))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Rewriting with strategies: innermost vs outermost
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_term_rewriting_strategies() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Compare innermost (eager) vs outermost (lazy) reduction strategies.
    // Innermost: reduce deepest redex first.
    // Outermost: reduce shallowest redex first.
    let form = r#"(progn
  (fset 'neovm--st-var-p
    (lambda (x) (and (symbolp x) (> (length (symbol-name x)) 1)
                     (= (aref (symbol-name x) 0) ??))))

  (fset 'neovm--st-match
    (lambda (pat term binds)
      (cond
       ((funcall 'neovm--st-var-p pat)
        (let ((ex (assq pat binds)))
          (if ex (if (equal (cdr ex) term) binds nil)
            (cons (cons pat term) binds))))
       ((and (consp pat) (consp term))
        (let ((b (funcall 'neovm--st-match (car pat) (car term) binds)))
          (when b (funcall 'neovm--st-match (cdr pat) (cdr term) b))))
       ((equal pat term) binds)
       (t nil))))

  (fset 'neovm--st-subst
    (lambda (tmpl binds)
      (cond
       ((funcall 'neovm--st-var-p tmpl)
        (let ((b (assq tmpl binds)))
          (if b (cdr b) tmpl)))
       ((consp tmpl)
        (cons (funcall 'neovm--st-subst (car tmpl) binds)
              (funcall 'neovm--st-subst (cdr tmpl) binds)))
       (t tmpl))))

  ;; Try rules at top level only
  (fset 'neovm--st-try-top
    (lambda (rules term)
      (let ((result nil) (rs rules))
        (while (and rs (not result))
          (let ((binds (funcall 'neovm--st-match (caar rs) term nil)))
            (when binds
              (setq result (funcall 'neovm--st-subst (cdar rs) binds))))
          (setq rs (cdr rs)))
        result)))

  ;; Outermost: try top first, then recurse
  (fset 'neovm--st-outer-step
    (lambda (rules term)
      (or (funcall 'neovm--st-try-top rules term)
          (if (consp term)
              (let ((nc (funcall 'neovm--st-outer-step rules (car term))))
                (if nc (cons nc (cdr term))
                  (let ((nd (funcall 'neovm--st-outer-step rules (cdr term))))
                    (when nd (cons (car term) nd)))))
            nil))))

  ;; Innermost: recurse first, then try top
  (fset 'neovm--st-inner-step
    (lambda (rules term)
      (if (consp term)
          (let ((nc (funcall 'neovm--st-inner-step rules (car term))))
            (if nc (cons nc (cdr term))
              (let ((nd (funcall 'neovm--st-inner-step rules (cdr term))))
                (if nd (cons (car term) nd)
                  (funcall 'neovm--st-try-top rules term)))))
        (funcall 'neovm--st-try-top rules term))))

  (fset 'neovm--st-normalize
    (lambda (step-fn rules term max)
      (let ((cur term) (s 0) (trace nil))
        (while (< s max)
          (let ((nxt (funcall step-fn rules cur)))
            (if nxt
                (progn
                  (setq trace (cons cur trace))
                  (setq cur nxt)
                  (setq s (1+ s)))
              (setq s max))))
        (list :result cur :steps s :trace-length (length trace)))))

  (unwind-protect
      (let ((rules '(((+ ?x 0) . ?x)
                     ((+ 0 ?x) . ?x)
                     ((* ?x 1) . ?x)
                     ((* 1 ?x) . ?x)
                     ((* ?x 0) . 0))))
        (let ((term '(* 1 (+ (+ a 0) (* b 0)))))
          (list
           :outermost (funcall 'neovm--st-normalize
                               'neovm--st-outer-step rules term 50)
           :innermost (funcall 'neovm--st-normalize
                               'neovm--st-inner-step rules term 50))))
    (fmakunbound 'neovm--st-var-p)
    (fmakunbound 'neovm--st-match)
    (fmakunbound 'neovm--st-subst)
    (fmakunbound 'neovm--st-try-top)
    (fmakunbound 'neovm--st-outer-step)
    (fmakunbound 'neovm--st-inner-step)
    (fmakunbound 'neovm--st-normalize)))"#;
    let expect = expect_test::expect![[
        r#""OK (:outermost (:result (* 1 (+ (+ a 0) (* b 0))) :steps 50 :trace-length 0) :innermost (:result (* 1 (+ (+ a 0) (* b 0))) :steps 50 :trace-length 0))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Term rewriting with associativity-commutativity (AC matching)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_term_rewriting_ac_normalization() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Normalize terms with commutative+associative operators by sorting arguments.
    // This is a pre-processing step that makes standard matching work modulo AC.
    let form = r#"(progn
  ;; Flatten nested applications of an AC operator
  (fset 'neovm--ac-flatten
    (lambda (op term)
      "Flatten nested (op a (op b c)) into (op a b c) flat arglist."
      (if (and (consp term) (eq (car term) op))
          (let ((args nil))
            (dolist (arg (cdr term))
              (let ((flat (funcall 'neovm--ac-flatten op arg)))
                (if (and (consp flat) (eq (car flat) op))
                    (setq args (append args (cdr flat)))
                  (setq args (append args (list flat))))))
            (cons op args))
        term)))

  ;; Sort arguments of commutative operators for canonical form
  (fset 'neovm--ac-sort-args
    (lambda (term)
      "Sort arguments of + and * for canonical ordering."
      (if (consp term)
          (let ((sorted-children (mapcar (lambda (x) (funcall 'neovm--ac-sort-args x))
                                         (cdr term))))
            (if (memq (car term) '(+ *))
                (cons (car term)
                      (sort sorted-children
                            (lambda (a b) (string< (format "%S" a) (format "%S" b)))))
              (cons (car term) sorted-children)))
        term)))

  ;; Full AC normalization: flatten then sort
  (fset 'neovm--ac-normalize
    (lambda (term)
      (let ((flat term))
        ;; Flatten all AC operators
        (dolist (op '(+ *))
          (setq flat (funcall 'neovm--ac-flatten op flat)))
        ;; Sort arguments
        (funcall 'neovm--ac-sort-args flat))))

  (unwind-protect
      (list
       ;; Flatten: (+ a (+ b c)) -> (+ a b c)
       (funcall 'neovm--ac-normalize '(+ a (+ b c)))
       ;; Sort: (+ c a b) -> (+ a b c)
       (funcall 'neovm--ac-normalize '(+ c a b))
       ;; Both: (+ c (+ b a)) -> (+ a b c)
       (funcall 'neovm--ac-normalize '(+ c (+ b a)))
       ;; Nested operators: (* b (* a c)) -> (* a b c)
       (funcall 'neovm--ac-normalize '(* b (* a c)))
       ;; Mixed: (+ (* b a) (* d c)) -> (+ (* a b) (* c d))
       (funcall 'neovm--ac-normalize '(+ (* b a) (* d c)))
       ;; Deep: (+ (+ z y) (+ x (+ w v)))
       (funcall 'neovm--ac-normalize '(+ (+ z y) (+ x (+ w v))))
       ;; Non-AC operator preserved: (- a b) stays (- a b)
       (funcall 'neovm--ac-normalize '(- a b))
       ;; Mixed AC and non-AC
       (funcall 'neovm--ac-normalize '(+ (- c d) (+ b a))))
    (fmakunbound 'neovm--ac-flatten)
    (fmakunbound 'neovm--ac-sort-args)
    (fmakunbound 'neovm--ac-normalize)))"#;
    let expect = expect_test::expect![[
        r#""OK ((+ a b c) (+ a b c) (+ a b c) (* a b c) (+ (* a b) (* c d)) (+ v w x y z) (- a b) (+ (- c d) a b))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
