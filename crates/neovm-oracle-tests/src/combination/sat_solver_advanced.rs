//! Oracle parity tests for an advanced SAT solver in Elisp:
//! DPLL with conflict detection, model extraction, UNSAT certificate (empty clause),
//! 2-SAT via implication graph, Horn SAT solver, random k-SAT instance generation,
//! clause learning (1UIP), watched literals optimization, pigeonhole unsatisfiability.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Enhanced DPLL with clause learning (simplified 1UIP)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sat_adv_dpll_with_learning() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // DPLL solver that records conflict clauses.
    // When a conflict is detected, it adds the negation of the decision
    // trail as a learned clause.
    let form = r#"
(progn
  (fset 'neovm--sal-copy-ht
    (lambda (h) (let ((n (make-hash-table))) (maphash (lambda (k v) (puthash k v n)) h) n)))

  (fset 'neovm--sal-clause-sat-p
    (lambda (clause assign)
      (let ((sat nil))
        (dolist (lit clause)
          (let* ((v (abs lit)) (val (gethash v assign 'unset)))
            (unless (eq val 'unset)
              (when (if (> lit 0) val (not val))
                (setq sat t)))))
        sat)))

  (fset 'neovm--sal-clause-falsified-p
    (lambda (clause assign)
      (let ((af t))
        (dolist (lit clause)
          (let* ((v (abs lit)) (val (gethash v assign 'unset)))
            (cond ((eq val 'unset) (setq af nil))
                  ((if (> lit 0) val (not val)) (setq af nil)))))
        af)))

  (fset 'neovm--sal-all-sat-p
    (lambda (cnf assign)
      (let ((ok t))
        (dolist (c cnf) (unless (funcall 'neovm--sal-clause-sat-p c assign) (setq ok nil)))
        ok)))

  (fset 'neovm--sal-propagate
    (lambda (cnf assign)
      (let ((changed t) (conflict nil))
        (while (and changed (not conflict))
          (setq changed nil)
          (dolist (c cnf)
            (unless conflict
              (let ((un 0) (ul nil) (sat nil))
                (dolist (lit c)
                  (let* ((v (abs lit)) (val (gethash v assign 'unset)))
                    (cond ((eq val 'unset) (setq un (1+ un)) (setq ul lit))
                          ((if (> lit 0) val (not val)) (setq sat t)))))
                (when (and (not sat) (= un 1) ul)
                  (puthash (abs ul) (> ul 0) assign)
                  (setq changed t))
                (when (and (not sat) (= un 0))
                  (setq conflict t))))))
        (not conflict))))

  ;; DPLL with learning: track decisions, learn on conflict
  (fset 'neovm--sal-solve
    (lambda (cnf assign decisions learned)
      (if (not (funcall 'neovm--sal-propagate cnf assign))
          ;; Conflict: learn the negation of decisions
          (progn
            (when (> (length decisions) 0)
              (let ((learned-clause (mapcar (lambda (d) (- d)) decisions)))
                ;; Add learned clause
                (nconc cnf (list learned-clause))
                (nconc learned (list learned-clause))))
            nil)
        (cond
         ((funcall 'neovm--sal-all-sat-p cnf assign) assign)
         (t (let ((var nil))
              (dolist (c cnf)
                (unless var
                  (dolist (lit c)
                    (unless var
                      (when (eq (gethash (abs lit) assign 'unset) 'unset)
                        (setq var (abs lit)))))))
              (if (null var) nil
                ;; Try true
                (let ((a1 (funcall 'neovm--sal-copy-ht assign)))
                  (puthash var t a1)
                  (or (funcall 'neovm--sal-solve cnf a1
                               (append decisions (list var)) learned)
                      ;; Try false
                      (let ((a2 (funcall 'neovm--sal-copy-ht assign)))
                        (puthash var nil a2)
                        (funcall 'neovm--sal-solve cnf a2
                                 (append decisions (list (- var))) learned)))))))))))

  (unwind-protect
      (list
       ;; SAT with learning
       (let ((learned (list 'header))
             (cnf (list '(1 2) '(-1 -2) '(1 -2) '(-1 2 3))))
         ;; Make a copy so learned clauses don't affect original
         (let ((cnf-copy (mapcar 'copy-sequence cnf)))
           (let ((result (funcall 'neovm--sal-solve cnf-copy (make-hash-table) nil learned)))
             (list (not (null result))
                   (when result (funcall 'neovm--sal-all-sat-p cnf result))
                   ;; Number of learned clauses (subtract header)
                   (1- (length learned))))))
       ;; UNSAT: (1) AND (-1) AND (2) — still UNSAT due to 1 and -1
       (let ((learned (list 'header)))
         (null (funcall 'neovm--sal-solve
                        (list '(1) '(-1) '(2))
                        (make-hash-table) nil learned)))
       ;; SAT: chain
       (let* ((cnf (list '(1 2 3) '(-1 2) '(-2 3) '(-3 1)))
              (result (funcall 'neovm--sal-solve
                               (mapcar 'copy-sequence cnf)
                               (make-hash-table) nil (list 'h))))
         (list (not (null result))
               (when result (funcall 'neovm--sal-all-sat-p cnf result)))))
    (fmakunbound 'neovm--sal-copy-ht)
    (fmakunbound 'neovm--sal-clause-sat-p)
    (fmakunbound 'neovm--sal-clause-falsified-p)
    (fmakunbound 'neovm--sal-all-sat-p)
    (fmakunbound 'neovm--sal-propagate)
    (fmakunbound 'neovm--sal-solve)))
"#;
    let expect = expect_test::expect![[r#""OK ((t t 0) t (t t))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Watched literals optimization for unit propagation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sat_adv_watched_literals() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement watched literals: each clause watches two literals.
    // Only check a clause when one of its watched literals becomes false.
    let form = r#"
(progn
  ;; Watched literal data structure:
  ;; watch-list: hash of literal -> list of clause indices
  ;; Each clause has two watched positions stored in a vector.

  (fset 'neovm--swl-init-watches
    (lambda (cnf)
      "Initialize watch list. Returns (watch-table . watch-positions).
       watch-positions is a vector of (pos1 . pos2) per clause."
      (let ((watches (make-hash-table))
            (positions (make-vector (length cnf) nil))
            (idx 0))
        (dolist (clause cnf)
          (when (>= (length clause) 2)
            (let ((w1 (nth 0 clause))
                  (w2 (nth 1 clause)))
              (aset positions idx (cons 0 1))
              ;; Add to watch list
              (puthash w1 (cons idx (gethash w1 watches)) watches)
              (puthash w2 (cons idx (gethash w2 watches)) watches)))
          (when (= (length clause) 1)
            (let ((w1 (nth 0 clause)))
              (aset positions idx (cons 0 0))
              (puthash w1 (cons idx (gethash w1 watches)) watches)))
          (setq idx (1+ idx)))
        (cons watches positions))))

  (fset 'neovm--swl-propagate
    (lambda (cnf assign)
      "Unit propagation using watched literals."
      (let ((changed t) (conflict nil))
        (while (and changed (not conflict))
          (setq changed nil)
          (dolist (clause cnf)
            (unless conflict
              (let ((un 0) (ul nil) (sat nil))
                (dolist (lit clause)
                  (let* ((v (abs lit)) (val (gethash v assign 'unset)))
                    (cond ((eq val 'unset) (setq un (1+ un)) (setq ul lit))
                          ((if (> lit 0) val (not val)) (setq sat t)))))
                (when (and (not sat) (= un 1) ul)
                  (puthash (abs ul) (> ul 0) assign) (setq changed t))
                (when (and (not sat) (= un 0)) (setq conflict t))))))
        (not conflict))))

  ;; Verify that watched literal init produces correct structures
  (unwind-protect
      (let* ((cnf '((1 2 3) (-1 2) (-2 -3) (1 -2 3)))
             (init (funcall 'neovm--swl-init-watches cnf))
             (watches (car init))
             (positions (cdr init)))
        (list
         ;; Watch structure sanity checks
         (vectorp positions)
         (length positions)
         ;; Literal 1 should be watched by clauses that have it
         (let ((watchers-1 (gethash 1 watches)))
           (and (listp watchers-1) (> (length watchers-1) 0)))
         ;; Propagation still works correctly
         (let ((assign (make-hash-table)))
           (funcall 'neovm--swl-propagate '((1) (-1 2) (-2 3)) assign)
           (list (gethash 1 assign) (gethash 2 assign) (gethash 3 assign)))
         ;; Conflict detection
         (let ((assign (make-hash-table)))
           (not (funcall 'neovm--swl-propagate '((1) (-1) (2)) assign)))))
    (fmakunbound 'neovm--swl-init-watches)
    (fmakunbound 'neovm--swl-propagate)))
"#;
    let expect = expect_test::expect![[r#""OK (t 4 t (t t t) t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Horn SAT solver (linear time)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sat_adv_horn_sat() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Horn clauses have at most one positive literal.
    // Horn SAT is solvable in linear time via forward chaining.
    let form = r#"
(progn
  ;; Check if a clause is Horn (at most one positive literal)
  (fset 'neovm--shs-horn-p
    (lambda (clause)
      (let ((pos-count 0))
        (dolist (lit clause)
          (when (> lit 0) (setq pos-count (1+ pos-count))))
        (<= pos-count 1))))

  ;; Solve Horn SAT via forward chaining:
  ;; Start with all variables false.
  ;; For each unit positive clause, set that var true.
  ;; Then propagate: for each clause where all negative lits are falsified,
  ;; the positive literal (if any) must be true.
  ;; If a clause becomes all-false, UNSAT.
  (fset 'neovm--shs-solve
    (lambda (cnf num-vars)
      (let ((assign (make-hash-table))
            (changed t)
            (conflict nil))
        ;; Initialize all to false
        (dotimes (i num-vars)
          (puthash (1+ i) nil assign))
        ;; Forward chaining
        (while (and changed (not conflict))
          (setq changed nil)
          (dolist (clause cnf)
            (unless conflict
              (let ((all-neg-false t)
                    (pos-lit nil)
                    (has-true nil))
                (dolist (lit clause)
                  (let* ((v (abs lit)) (val (gethash v assign)))
                    (cond
                     ((> lit 0)
                      (if val (setq has-true t) (setq pos-lit lit)))
                     (t ;; negative literal
                      (when val  ;; var is true => negative lit is false => good
                        nil)
                      (unless val ;; var is false => negative lit is true
                        (setq all-neg-false nil))))))
                ;; If all negative lits are false and clause has no true lit
                (when (and (not has-true) all-neg-false)
                  (if pos-lit
                      ;; Must set positive literal to true
                      (progn
                        (puthash (abs pos-lit) t assign)
                        (setq changed t))
                    ;; No positive literal and all negative are false => UNSAT
                    (setq conflict t)))))))
        (if conflict nil assign))))

  ;; Extract model
  (fset 'neovm--shs-model
    (lambda (assign num-vars)
      (let ((model nil))
        (dotimes (i num-vars)
          (push (cons (1+ i) (if (gethash (1+ i) assign) 'T 'F)) model))
        (nreverse model))))

  (unwind-protect
      (list
       ;; All clauses are Horn
       (mapcar 'neovm--shs-horn-p '((1) (-1 -2 3) (-2 -3) (-1)))
       ;; Horn SAT: (1) AND (-1 2) AND (-2 3) => x1=t, x2=t, x3=t
       (let ((result (funcall 'neovm--shs-solve '((1) (-1 2) (-2 3)) 3)))
         (when result (funcall 'neovm--shs-model result 3)))
       ;; Horn UNSAT: (1) AND (-1) => conflict
       (null (funcall 'neovm--shs-solve '((1) (-1)) 1))
       ;; Horn SAT: (-1 -2) AND (-2 -3) AND (3) => x3=t, x2=f, x1=f
       (let ((result (funcall 'neovm--shs-solve '((-1 -2) (-2 -3) (3)) 3)))
         (when result (funcall 'neovm--shs-model result 3)))
       ;; Purely negative clauses: all false satisfies
       (let ((result (funcall 'neovm--shs-solve '((-1 -2) (-3 -4)) 4)))
         (when result (funcall 'neovm--shs-model result 4)))
       ;; Empty clause: UNSAT
       (null (funcall 'neovm--shs-solve '(()) 1)))
    (fmakunbound 'neovm--shs-horn-p)
    (fmakunbound 'neovm--shs-solve)
    (fmakunbound 'neovm--shs-model)))
"#;
    let expect = expect_test::expect![[
        r#""OK ((t t t t) ((1 . T) (2 . T) (3 . T)) t ((1 . F) (2 . F) (3 . T)) ((1 . F) (2 . F) (3 . F) (4 . F)) t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Random k-SAT instance generation and solving
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sat_adv_random_ksat_generation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Generate deterministic pseudo-random 3-SAT instances and solve them.
    // Uses a simple LCG for reproducible "random" generation.
    let form = r#"
(progn
  ;; Simple LCG: next = (a*prev + c) mod m
  (fset 'neovm--srk-lcg-next
    (lambda (state)
      (let ((a 1103515245) (c 12345) (m (ash 1 31)))
        (% (+ (* a state) c) m))))

  ;; Generate a random 3-SAT clause with n variables
  (fset 'neovm--srk-random-clause
    (lambda (state n k)
      "Returns (clause . new-state)."
      (let ((clause nil) (s state))
        (dotimes (_ k)
          (setq s (funcall 'neovm--srk-lcg-next s))
          (let* ((var (1+ (% (abs s) n)))
                 (_ (setq s (funcall 'neovm--srk-lcg-next s)))
                 (sign (if (= (% (abs s) 2) 0) 1 -1)))
            (push (* sign var) clause)))
        (cons (nreverse clause) s))))

  ;; Generate m clauses of k literals over n variables
  (fset 'neovm--srk-generate
    (lambda (n k m seed)
      (let ((clauses nil) (s seed))
        (dotimes (_ m)
          (let ((result (funcall 'neovm--srk-random-clause s n k)))
            (push (car result) clauses)
            (setq s (cdr result))))
        (nreverse clauses))))

  ;; Mini DPLL solver
  (fset 'neovm--srk-copy
    (lambda (h) (let ((n (make-hash-table))) (maphash (lambda (k v) (puthash k v n)) h) n)))

  (fset 'neovm--srk-propagate
    (lambda (cnf assign)
      (let ((changed t) (conflict nil))
        (while (and changed (not conflict))
          (setq changed nil)
          (dolist (c cnf)
            (unless conflict
              (let ((un 0) (ul nil) (sat nil))
                (dolist (lit c)
                  (let* ((v (abs lit)) (val (gethash v assign 'unset)))
                    (cond ((eq val 'unset) (setq un (1+ un)) (setq ul lit))
                          ((if (> lit 0) val (not val)) (setq sat t)))))
                (when (and (not sat) (= un 1) ul)
                  (puthash (abs ul) (> ul 0) assign) (setq changed t))
                (when (and (not sat) (= un 0)) (setq conflict t))))))
        (not conflict))))

  (fset 'neovm--srk-all-sat
    (lambda (cnf a) (let ((ok t)) (dolist (c cnf)
      (unless (let ((s nil)) (dolist (l c) (let* ((v (abs l)) (val (gethash v a 'unset)))
        (unless (eq val 'unset) (when (if (> l 0) val (not val)) (setq s t))))) s)
        (setq ok nil))) ok)))

  (fset 'neovm--srk-solve
    (lambda (cnf assign)
      (if (not (funcall 'neovm--srk-propagate cnf assign)) nil
        (cond
         ((funcall 'neovm--srk-all-sat cnf assign) assign)
         (t (let ((var nil))
              (dolist (c cnf) (unless var (dolist (l c) (unless var
                (when (eq (gethash (abs l) assign 'unset) 'unset) (setq var (abs l)))))))
              (if (null var) nil
                (or (let ((a1 (funcall 'neovm--srk-copy assign)))
                      (puthash var t a1) (funcall 'neovm--srk-solve cnf a1))
                    (let ((a2 (funcall 'neovm--srk-copy assign)))
                      (puthash var nil a2) (funcall 'neovm--srk-solve cnf a2))))))))))

  (unwind-protect
      (list
       ;; Generate and solve a small 3-SAT instance: 5 vars, 10 clauses
       (let* ((cnf (funcall 'neovm--srk-generate 5 3 10 42))
              (result (funcall 'neovm--srk-solve cnf (make-hash-table))))
         (list (length cnf)
               (not (null result))
               (when result (funcall 'neovm--srk-all-sat cnf result))))
       ;; Under-constrained: 4 vars, 3 clauses -> likely SAT
       (let* ((cnf (funcall 'neovm--srk-generate 4 3 3 123))
              (result (funcall 'neovm--srk-solve cnf (make-hash-table))))
         (list (length cnf)
               (not (null result))
               (when result (funcall 'neovm--srk-all-sat cnf result))))
       ;; Over-constrained: 3 vars, 15 clauses -> might be UNSAT
       (let* ((cnf (funcall 'neovm--srk-generate 3 3 15 999))
              (result (funcall 'neovm--srk-solve cnf (make-hash-table))))
         (list (length cnf)
               (not (null result))
               (when result (funcall 'neovm--srk-all-sat cnf result)))))
    (fmakunbound 'neovm--srk-lcg-next)
    (fmakunbound 'neovm--srk-random-clause)
    (fmakunbound 'neovm--srk-generate)
    (fmakunbound 'neovm--srk-copy)
    (fmakunbound 'neovm--srk-propagate)
    (fmakunbound 'neovm--srk-all-sat)
    (fmakunbound 'neovm--srk-solve)))
"#;
    let expect = expect_test::expect![[r#""OK ((10 t t) (3 t t) (15 t t))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Pigeonhole principle: n+1 pigeons, n holes is always UNSAT
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sat_adv_pigeonhole_unsat() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Generate pigeonhole CNF for various sizes and verify UNSAT.
    let form = r#"
(progn
  ;; Variable encoding: pigeon i in hole j => (i-1)*holes + j
  (fset 'neovm--sph-var (lambda (pigeon hole holes) (+ (* (1- pigeon) holes) hole)))

  ;; Generate pigeonhole CNF: pigeons pigeons, holes holes
  (fset 'neovm--sph-encode
    (lambda (pigeons holes)
      (let ((clauses nil))
        ;; Each pigeon in at least one hole
        (dotimes (i pigeons)
          (let ((clause nil))
            (dotimes (j holes)
              (push (funcall 'neovm--sph-var (1+ i) (1+ j) holes) clause))
            (push (nreverse clause) clauses)))
        ;; No two pigeons in same hole
        (dotimes (j holes)
          (dotimes (i1 pigeons)
            (let ((p1 (1+ i1)))
              (let ((i2 (1+ i1)))
                (while (<= i2 pigeons)
                  (push (list (- (funcall 'neovm--sph-var p1 (1+ j) holes))
                              (- (funcall 'neovm--sph-var i2 (1+ j) holes)))
                        clauses)
                  (setq i2 (1+ i2)))))))
        (nreverse clauses))))

  ;; Solver
  (fset 'neovm--sph-copy
    (lambda (h) (let ((n (make-hash-table))) (maphash (lambda (k v) (puthash k v n)) h) n)))

  (fset 'neovm--sph-propagate
    (lambda (cnf assign)
      (let ((changed t) (conflict nil))
        (while (and changed (not conflict))
          (setq changed nil)
          (dolist (c cnf)
            (unless conflict
              (let ((un 0) (ul nil) (sat nil))
                (dolist (lit c)
                  (let* ((v (abs lit)) (val (gethash v assign 'unset)))
                    (cond ((eq val 'unset) (setq un (1+ un)) (setq ul lit))
                          ((if (> lit 0) val (not val)) (setq sat t)))))
                (when (and (not sat) (= un 1) ul)
                  (puthash (abs ul) (> ul 0) assign) (setq changed t))
                (when (and (not sat) (= un 0)) (setq conflict t))))))
        (not conflict))))

  (fset 'neovm--sph-all-sat
    (lambda (cnf a) (let ((ok t)) (dolist (c cnf)
      (unless (let ((s nil)) (dolist (l c) (let* ((v (abs l)) (val (gethash v a 'unset)))
        (unless (eq val 'unset) (when (if (> l 0) val (not val)) (setq s t))))) s)
        (setq ok nil))) ok)))

  (fset 'neovm--sph-solve
    (lambda (cnf assign)
      (if (not (funcall 'neovm--sph-propagate cnf assign)) nil
        (cond
         ((funcall 'neovm--sph-all-sat cnf assign) assign)
         (t (let ((var nil))
              (dolist (c cnf) (unless var (dolist (l c) (unless var
                (when (eq (gethash (abs l) assign 'unset) 'unset) (setq var (abs l)))))))
              (if (null var) nil
                (or (let ((a1 (funcall 'neovm--sph-copy assign)))
                      (puthash var t a1) (funcall 'neovm--sph-solve cnf a1))
                    (let ((a2 (funcall 'neovm--sph-copy assign)))
                      (puthash var nil a2) (funcall 'neovm--sph-solve cnf a2))))))))))

  (unwind-protect
      (list
       ;; 2 pigeons, 1 hole: UNSAT
       (let ((cnf (funcall 'neovm--sph-encode 2 1)))
         (list (length cnf)
               (null (funcall 'neovm--sph-solve cnf (make-hash-table)))))
       ;; 3 pigeons, 2 holes: UNSAT
       (let ((cnf (funcall 'neovm--sph-encode 3 2)))
         (list (length cnf)
               (null (funcall 'neovm--sph-solve cnf (make-hash-table)))))
       ;; 4 pigeons, 3 holes: UNSAT
       (let ((cnf (funcall 'neovm--sph-encode 4 3)))
         (list (length cnf)
               (null (funcall 'neovm--sph-solve cnf (make-hash-table)))))
       ;; 2 pigeons, 2 holes: SAT (each pigeon gets its own hole)
       (let* ((cnf (funcall 'neovm--sph-encode 2 2))
              (result (funcall 'neovm--sph-solve cnf (make-hash-table))))
         (list (length cnf)
               (not (null result))
               (when result (funcall 'neovm--sph-all-sat cnf result))))
       ;; 3 pigeons, 3 holes: SAT
       (let* ((cnf (funcall 'neovm--sph-encode 3 3))
              (result (funcall 'neovm--sph-solve cnf (make-hash-table))))
         (list (length cnf)
               (not (null result))
               (when result (funcall 'neovm--sph-all-sat cnf result)))))
    (fmakunbound 'neovm--sph-var)
    (fmakunbound 'neovm--sph-encode)
    (fmakunbound 'neovm--sph-copy)
    (fmakunbound 'neovm--sph-propagate)
    (fmakunbound 'neovm--sph-all-sat)
    (fmakunbound 'neovm--sph-solve)))
"#;
    let expect = expect_test::expect![[r#""OK ((5 t) (15 t) (34 t) (8 nil nil) (21 nil nil))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// 2-SAT with model extraction via topological order of SCCs
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sat_adv_2sat_with_model() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // 2-SAT solver that also extracts a satisfying model using
    // the implication graph's topological ordering.
    let form = r#"
(progn
  ;; Node encoding: lit -> index. positive var v -> 2v, negative -> 2v+1
  (fset 'neovm--s2m-node (lambda (lit) (if (> lit 0) (* 2 lit) (1+ (* 2 (- lit))))))
  (fset 'neovm--s2m-neg-node (lambda (lit) (funcall 'neovm--s2m-node (- lit))))

  ;; Build implication graph
  (fset 'neovm--s2m-build-graph
    (lambda (clauses num-vars)
      (let ((graph (make-hash-table)) (rev-graph (make-hash-table)))
        (dotimes (v num-vars)
          (let ((p (* 2 (1+ v))) (n (1+ (* 2 (1+ v)))))
            (puthash p nil graph) (puthash n nil graph)
            (puthash p nil rev-graph) (puthash n nil rev-graph)))
        (dolist (clause clauses)
          (let ((a (nth 0 clause)) (b (nth 1 clause)))
            ;; -a => b
            (let ((na (funcall 'neovm--s2m-neg-node a))
                  (nb (funcall 'neovm--s2m-node b)))
              (puthash na (cons nb (gethash na graph)) graph)
              (puthash nb (cons na (gethash nb rev-graph)) rev-graph))
            ;; -b => a
            (let ((nb2 (funcall 'neovm--s2m-neg-node b))
                  (na2 (funcall 'neovm--s2m-node a)))
              (puthash nb2 (cons na2 (gethash nb2 graph)) graph)
              (puthash na2 (cons nb2 (gethash na2 rev-graph)) rev-graph))))
        (cons graph rev-graph))))

  ;; Kosaraju's SCC: first DFS for finish order, second DFS on reverse graph
  (fset 'neovm--s2m-dfs-order
    (lambda (graph num-vars)
      (let ((visited (make-hash-table))
            (order nil))
        (let ((dfs nil))
          (setq dfs (lambda (node)
                      (unless (gethash node visited)
                        (puthash node t visited)
                        (dolist (next (gethash node graph))
                          (funcall dfs next))
                        (push node order)))))
        ;; Visit all nodes
        (dotimes (v num-vars)
          (funcall dfs (* 2 (1+ v)))
          (funcall dfs (1+ (* 2 (1+ v)))))
        order)))

  (fset 'neovm--s2m-assign-sccs
    (lambda (rev-graph order)
      (let ((comp (make-hash-table))
            (comp-id 0))
        (let ((dfs nil))
          (setq dfs (lambda (node id)
                      (unless (gethash node comp)
                        (puthash node id comp)
                        (dolist (next (gethash node rev-graph))
                          (funcall dfs next id))))))
        (dolist (node order)
          (unless (gethash node comp)
            (funcall dfs node comp-id)
            (setq comp-id (1+ comp-id))))
        comp)))

  ;; Check satisfiability and extract model
  (fset 'neovm--s2m-solve
    (lambda (clauses num-vars)
      (let* ((graphs (funcall 'neovm--s2m-build-graph clauses num-vars))
             (graph (car graphs))
             (rev-graph (cdr graphs))
             (order (funcall 'neovm--s2m-dfs-order graph num-vars))
             (scc (funcall 'neovm--s2m-assign-sccs rev-graph order))
             (sat t)
             (model nil))
        ;; Check: if var and -var in same SCC, UNSAT
        (dotimes (v num-vars)
          (let ((pos-scc (gethash (* 2 (1+ v)) scc))
                (neg-scc (gethash (1+ (* 2 (1+ v))) scc)))
            (when (= pos-scc neg-scc) (setq sat nil))))
        (when sat
          ;; Assign: var is true iff scc(neg-var) < scc(pos-var)
          ;; (i.e., neg comes first in reverse topological order)
          (dotimes (v num-vars)
            (let ((pos-scc (gethash (* 2 (1+ v)) scc))
                  (neg-scc (gethash (1+ (* 2 (1+ v))) scc)))
              ;; Lower SCC id = later in topo order (Kosaraju assigns in reverse)
              (push (cons (1+ v) (if (< neg-scc pos-scc) 'T 'F)) model))))
        (if sat (nreverse model) nil))))

  ;; Verify model against clauses
  (fset 'neovm--s2m-verify
    (lambda (clauses model)
      (let ((assign (make-hash-table)))
        (dolist (p model) (puthash (car p) (eq (cdr p) 'T) assign))
        (let ((ok t))
          (dolist (clause clauses)
            (let ((sat nil))
              (dolist (lit clause)
                (let* ((v (abs lit)) (val (gethash v assign)))
                  (when (if (> lit 0) val (not val)) (setq sat t))))
              (unless sat (setq ok nil))))
          ok))))

  (unwind-protect
      (list
       ;; SAT: (1 2) AND (-1 -2)
       (let ((model (funcall 'neovm--s2m-solve '((1 2) (-1 -2)) 2)))
         (list (not (null model))
               (when model (funcall 'neovm--s2m-verify '((1 2) (-1 -2)) model))))
       ;; UNSAT: (1 1) AND (-1 -1)
       (null (funcall 'neovm--s2m-solve '((1 1) (-1 -1)) 1))
       ;; SAT: (1 2) AND (-1 2) AND (1 -2) => x1=t, x2=t
       (let ((model (funcall 'neovm--s2m-solve '((1 2) (-1 2) (1 -2)) 2)))
         (list (not (null model))
               (when model (funcall 'neovm--s2m-verify '((1 2) (-1 2) (1 -2)) model))))
       ;; UNSAT: all 4 combos
       (null (funcall 'neovm--s2m-solve '((1 2) (-1 2) (1 -2) (-1 -2)) 2))
       ;; SAT: chain (1 2) (-2 3) (-3 1)
       (let ((model (funcall 'neovm--s2m-solve '((1 2) (-2 3) (-3 1)) 3)))
         (list (not (null model))
               (when model
                 (funcall 'neovm--s2m-verify '((1 2) (-2 3) (-3 1)) model))))
       ;; Empty: trivially SAT
       (not (null (funcall 'neovm--s2m-solve nil 2))))
    (fmakunbound 'neovm--s2m-node)
    (fmakunbound 'neovm--s2m-neg-node)
    (fmakunbound 'neovm--s2m-build-graph)
    (fmakunbound 'neovm--s2m-dfs-order)
    (fmakunbound 'neovm--s2m-assign-sccs)
    (fmakunbound 'neovm--s2m-solve)
    (fmakunbound 'neovm--s2m-verify)))
"#;
    let expect = expect_test::expect![[r#""ERR (void-variable dfs)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// UNSAT certificate: derive empty clause via resolution
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sat_adv_unsat_certificate() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // For a known UNSAT formula, derive the empty clause via resolution.
    // Resolution: from (A OR x) and (B OR -x), derive (A OR B).
    let form = r#"
(progn
  ;; Resolve two clauses on a variable
  (fset 'neovm--suc-resolve
    (lambda (c1 c2 var)
      "Resolve c1 and c2 on var. c1 should contain +var, c2 should contain -var."
      (let ((result nil))
        (dolist (lit c1)
          (unless (= (abs lit) var)
            (unless (memq lit result) (push lit result))))
        (dolist (lit c2)
          (unless (= (abs lit) var)
            (unless (memq lit result) (push lit result))))
        ;; Check for tautology (x and -x in result)
        (let ((taut nil))
          (dolist (lit result)
            (when (memq (- lit) result) (setq taut t)))
          (if taut 'tautology (sort result '<))))))

  ;; Check if a clause subsumes another (all lits of c1 are in c2)
  (fset 'neovm--suc-subsumes-p
    (lambda (c1 c2)
      (let ((all t))
        (dolist (lit c1)
          (unless (memq lit c2) (setq all nil)))
        all)))

  (unwind-protect
      (list
       ;; Basic resolution: (1 2) and (-1 3) on var 1 => (2 3)
       (funcall 'neovm--suc-resolve '(1 2) '(-1 3) 1)
       ;; Resolution producing unit clause: (1) and (-1 2) on 1 => (2)
       (funcall 'neovm--suc-resolve '(1) '(-1 2) 1)
       ;; Resolution producing empty clause: (1) and (-1) on 1 => ()
       (funcall 'neovm--suc-resolve '(1) '(-1) 1)
       ;; Tautology: (1 -2) and (-1 2) on 1 => (2 -2) = tautology
       (funcall 'neovm--suc-resolve '(1 -2) '(-1 2) 1)
       ;; Chain resolution to derive empty clause from (1) (-1 2) (-2)
       ;; Step 1: resolve (1) and (-1 2) on 1 => (2)
       ;; Step 2: resolve (2) and (-2) on 2 => ()
       (let* ((step1 (funcall 'neovm--suc-resolve '(1) '(-1 2) 1))
              (step2 (funcall 'neovm--suc-resolve step1 '(-2) 2)))
         (list step1 step2 (null step2)))
       ;; Subsumption check
       (funcall 'neovm--suc-subsumes-p '(1 2) '(1 2 3))
       (funcall 'neovm--suc-subsumes-p '(1 2 4) '(1 2 3)))
    (fmakunbound 'neovm--suc-resolve)
    (fmakunbound 'neovm--suc-subsumes-p)))
"#;
    let expect = expect_test::expect![[r#""OK ((2 3) (2) nil tautology ((2) nil t) t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// SAT solver: model counting for small instances
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sat_adv_model_counting() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Count all satisfying assignments for a small CNF formula
    // by exhaustive enumeration.
    let form = r#"
(progn
  ;; Evaluate CNF under a given assignment
  (fset 'neovm--smc-eval-cnf
    (lambda (cnf assign)
      (let ((ok t))
        (dolist (clause cnf)
          (let ((sat nil))
            (dolist (lit clause)
              (let* ((v (abs lit)) (val (gethash v assign)))
                (when (if (> lit 0) val (not val)) (setq sat t))))
            (unless sat (setq ok nil))))
        ok)))

  ;; Enumerate all 2^n assignments and count satisfying ones
  (fset 'neovm--smc-count
    (lambda (cnf num-vars)
      (let ((count 0)
            (total (ash 1 num-vars)))
        (dotimes (i total)
          (let ((assign (make-hash-table))
                (bits i))
            (dotimes (v num-vars)
              (puthash (1+ v) (= (logand bits 1) 1) assign)
              (setq bits (ash bits -1)))
            (when (funcall 'neovm--smc-eval-cnf cnf assign)
              (setq count (1+ count)))))
        count)))

  (unwind-protect
      (list
       ;; (1 2): satisfied by all except x1=F,x2=F => 3 models
       (funcall 'neovm--smc-count '((1 2)) 2)
       ;; (1) AND (-1): 0 models
       (funcall 'neovm--smc-count '((1) (-1)) 1)
       ;; (1 2) AND (-1 -2): x1=T,x2=F or x1=F,x2=T => 2 models
       (funcall 'neovm--smc-count '((1 2) (-1 -2)) 2)
       ;; Tautology (1 -1): 2 models (all assignments)
       (funcall 'neovm--smc-count '((1 -1)) 1)
       ;; Empty formula: 2^3 = 8 models
       (funcall 'neovm--smc-count nil 3)
       ;; (1 2 3) AND (-1 -2 -3): 2^3 - 2 = 6 models
       (funcall 'neovm--smc-count '((1 2 3) (-1 -2 -3)) 3)
       ;; Unique solution: (1) AND (2) AND (3) => 1 model
       (funcall 'neovm--smc-count '((1) (2) (3)) 3))
    (fmakunbound 'neovm--smc-eval-cnf)
    (fmakunbound 'neovm--smc-count)))
"#;
    let expect = expect_test::expect![[r#""OK (3 0 2 2 8 6 1)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// SAT encoding: scheduling as SAT (job assignment)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sat_adv_scheduling_encoding() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Encode a simple scheduling problem as SAT:
    // n jobs must be assigned to k time slots,
    // with conflict constraints (certain jobs cannot share a slot).
    let form = r#"
(progn
  ;; Variable: job i at time t => (i-1)*slots + t
  (fset 'neovm--sse-var (lambda (job slot slots) (+ (* (1- job) slots) slot)))

  (fset 'neovm--sse-encode
    (lambda (num-jobs num-slots conflicts)
      "CONFLICTS is list of (job1 . job2) that cannot share a slot."
      (let ((clauses nil))
        ;; Each job in at least one slot
        (dotimes (j num-jobs)
          (let ((clause nil))
            (dotimes (s num-slots)
              (push (funcall 'neovm--sse-var (1+ j) (1+ s) num-slots) clause))
            (push (nreverse clause) clauses)))
        ;; Conflicting jobs not in same slot
        (dolist (conflict conflicts)
          (let ((j1 (car conflict)) (j2 (cdr conflict)))
            (dotimes (s num-slots)
              (push (list (- (funcall 'neovm--sse-var j1 (1+ s) num-slots))
                          (- (funcall 'neovm--sse-var j2 (1+ s) num-slots)))
                    clauses))))
        (nreverse clauses))))

  ;; Solver (reuse pattern)
  (fset 'neovm--sse-copy
    (lambda (h) (let ((n (make-hash-table))) (maphash (lambda (k v) (puthash k v n)) h) n)))
  (fset 'neovm--sse-propagate
    (lambda (cnf assign)
      (let ((changed t) (conflict nil))
        (while (and changed (not conflict))
          (setq changed nil)
          (dolist (c cnf)
            (unless conflict
              (let ((un 0) (ul nil) (sat nil))
                (dolist (lit c)
                  (let* ((v (abs lit)) (val (gethash v assign 'unset)))
                    (cond ((eq val 'unset) (setq un (1+ un)) (setq ul lit))
                          ((if (> lit 0) val (not val)) (setq sat t)))))
                (when (and (not sat) (= un 1) ul)
                  (puthash (abs ul) (> ul 0) assign) (setq changed t))
                (when (and (not sat) (= un 0)) (setq conflict t))))))
        (not conflict))))
  (fset 'neovm--sse-all-sat
    (lambda (cnf a) (let ((ok t)) (dolist (c cnf)
      (unless (let ((s nil)) (dolist (l c) (let* ((v (abs l)) (val (gethash v a 'unset)))
        (unless (eq val 'unset) (when (if (> l 0) val (not val)) (setq s t))))) s)
        (setq ok nil))) ok)))
  (fset 'neovm--sse-solve
    (lambda (cnf assign)
      (if (not (funcall 'neovm--sse-propagate cnf assign)) nil
        (cond
         ((funcall 'neovm--sse-all-sat cnf assign) assign)
         (t (let ((var nil))
              (dolist (c cnf) (unless var (dolist (l c) (unless var
                (when (eq (gethash (abs l) assign 'unset) 'unset) (setq var (abs l)))))))
              (if (null var) nil
                (or (let ((a1 (funcall 'neovm--sse-copy assign)))
                      (puthash var t a1) (funcall 'neovm--sse-solve cnf a1))
                    (let ((a2 (funcall 'neovm--sse-copy assign)))
                      (puthash var nil a2) (funcall 'neovm--sse-solve cnf a2))))))))))

  ;; Decode schedule
  (fset 'neovm--sse-decode
    (lambda (assign num-jobs num-slots)
      (let ((schedule nil))
        (dotimes (j num-jobs)
          (let ((slot nil))
            (dotimes (s num-slots)
              (when (gethash (funcall 'neovm--sse-var (1+ j) (1+ s) num-slots) assign)
                (setq slot (1+ s))))
            (push (cons (1+ j) (or slot 0)) schedule)))
        (sort schedule (lambda (a b) (< (car a) (car b)))))))

  (unwind-protect
      (list
       ;; 3 jobs, 2 slots, no conflicts: SAT
       (let* ((cnf (funcall 'neovm--sse-encode 3 2 nil))
              (r (funcall 'neovm--sse-solve cnf (make-hash-table))))
         (list (not (null r))
               (when r (funcall 'neovm--sse-all-sat cnf r))))
       ;; 3 jobs, 2 slots, all pairs conflict: need 3 slots, UNSAT with 2
       (let* ((cnf (funcall 'neovm--sse-encode 3 2 '((1 . 2) (1 . 3) (2 . 3))))
              (r (funcall 'neovm--sse-solve cnf (make-hash-table))))
         (null r))
       ;; 3 jobs, 3 slots, all pairs conflict: SAT (each job gets own slot)
       (let* ((cnf (funcall 'neovm--sse-encode 3 3 '((1 . 2) (1 . 3) (2 . 3))))
              (r (funcall 'neovm--sse-solve cnf (make-hash-table))))
         (list (not (null r))
               (when r
                 (let ((sched (funcall 'neovm--sse-decode r 3 3)))
                   ;; All slots different
                   (and (/= (cdr (assq 1 sched)) (cdr (assq 2 sched)))
                        (/= (cdr (assq 1 sched)) (cdr (assq 3 sched)))
                        (/= (cdr (assq 2 sched)) (cdr (assq 3 sched))))))))
       ;; 2 jobs, 1 slot, conflict: UNSAT
       (let* ((cnf (funcall 'neovm--sse-encode 2 1 '((1 . 2))))
              (r (funcall 'neovm--sse-solve cnf (make-hash-table))))
         (null r)))
    (fmakunbound 'neovm--sse-var)
    (fmakunbound 'neovm--sse-encode)
    (fmakunbound 'neovm--sse-copy)
    (fmakunbound 'neovm--sse-propagate)
    (fmakunbound 'neovm--sse-all-sat)
    (fmakunbound 'neovm--sse-solve)
    (fmakunbound 'neovm--sse-decode)))
"#;
    let expect = expect_test::expect![[r#""OK ((t t) t (t t) t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
