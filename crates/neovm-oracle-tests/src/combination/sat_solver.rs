//! Oracle parity tests for a SAT solver implemented in Elisp:
//! CNF clause representation, unit propagation, pure literal elimination,
//! DPLL algorithm, satisfiability checking, and model extraction.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

// ---------------------------------------------------------------------------
// CNF representation and basic clause operations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sat_cnf_representation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  ;; A literal is an integer: positive = variable, negative = negation
  ;; A clause is a list of literals
  ;; A CNF formula is a list of clauses

  ;; Negate a literal
  (fset 'neovm--sat-negate (lambda (lit) (- lit)))

  ;; Is a clause satisfied by an assignment (hash: var -> t/nil)?
  (fset 'neovm--sat-clause-satisfied-p
    (lambda (clause assignment)
      (let ((sat nil))
        (dolist (lit clause)
          (let* ((var (abs lit))
                 (val (gethash var assignment 'unset)))
            (unless (eq val 'unset)
              (when (if (> lit 0) val (not val))
                (setq sat t)))))
        sat)))

  ;; Is a clause unit (exactly one unassigned literal, rest falsified)?
  (fset 'neovm--sat-unit-clause-p
    (lambda (clause assignment)
      "Returns the unit literal or nil."
      (let ((unassigned-count 0)
            (unit-lit nil)
            (falsified t))
        (dolist (lit clause)
          (let* ((var (abs lit))
                 (val (gethash var assignment 'unset)))
            (cond
             ((eq val 'unset)
              (setq unassigned-count (1+ unassigned-count))
              (setq unit-lit lit))
             ;; Is this literal true under assignment?
             ((if (> lit 0) val (not val))
              (setq falsified nil)))))
        (if (and falsified (= unassigned-count 1))
            unit-lit
          nil))))

  ;; Collect all variables from a CNF formula
  (fset 'neovm--sat-variables
    (lambda (cnf)
      (let ((vars nil))
        (dolist (clause cnf)
          (dolist (lit clause)
            (let ((v (abs lit)))
              (unless (memq v vars)
                (push v vars)))))
        (sort vars #'<))))

  (unwind-protect
      (let ((assign (make-hash-table)))
        (puthash 1 t assign)
        (puthash 2 nil assign)
        (list
         ;; Negation
         (funcall 'neovm--sat-negate 3)
         (funcall 'neovm--sat-negate -3)
         ;; Clause satisfied: (1 OR NOT 2) with x1=t, x2=nil => both true
         (funcall 'neovm--sat-clause-satisfied-p '(1 -2) assign)
         ;; Clause unsatisfied: (NOT 1 AND 2) both false
         (funcall 'neovm--sat-clause-satisfied-p '(-1 2) assign)
         ;; Clause with unassigned var: (3 OR 1) => satisfied by x1=t
         (funcall 'neovm--sat-clause-satisfied-p '(3 1) assign)
         ;; Unit clause: (-1 3) with x1=t => -1 is false, 3 unassigned => unit=3
         (funcall 'neovm--sat-unit-clause-p '(-1 3) assign)
         ;; Not unit: both unassigned
         (funcall 'neovm--sat-unit-clause-p '(3 4) assign)
         ;; Not unit: already satisfied
         (funcall 'neovm--sat-unit-clause-p '(1 3) assign)
         ;; Variables extraction
         (funcall 'neovm--sat-variables '((1 -2 3) (-1 2) (3 -4)))
         ;; Empty formula
         (funcall 'neovm--sat-variables nil)))
    (fmakunbound 'neovm--sat-negate)
    (fmakunbound 'neovm--sat-clause-satisfied-p)
    (fmakunbound 'neovm--sat-unit-clause-p)
    (fmakunbound 'neovm--sat-variables)))
"#;
    let expect = expect_test::expect![[r#""OK (-3 3 t nil t 3 nil nil (1 2 3 4) nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Unit propagation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sat_unit_propagation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (fset 'neovm--sat-up-unit-p
    (lambda (clause assignment)
      (let ((unassigned 0) (unit-lit nil) (sat nil))
        (dolist (lit clause)
          (let* ((var (abs lit))
                 (val (gethash var assignment 'unset)))
            (cond
             ((eq val 'unset)
              (setq unassigned (1+ unassigned))
              (setq unit-lit lit))
             ((if (> lit 0) val (not val))
              (setq sat t)))))
        (if (and (not sat) (= unassigned 1)) unit-lit nil))))

  (fset 'neovm--sat-up-clause-sat-p
    (lambda (clause assignment)
      (let ((sat nil))
        (dolist (lit clause)
          (let* ((var (abs lit))
                 (val (gethash var assignment 'unset)))
            (unless (eq val 'unset)
              (when (if (> lit 0) val (not val))
                (setq sat t)))))
        sat)))

  (fset 'neovm--sat-up-clause-falsified-p
    (lambda (clause assignment)
      "All literals assigned and all false."
      (let ((all-false t))
        (dolist (lit clause)
          (let* ((var (abs lit))
                 (val (gethash var assignment 'unset)))
            (cond
             ((eq val 'unset) (setq all-false nil))
             ((if (> lit 0) val (not val))
              (setq all-false nil)))))
        all-false)))

  ;; Unit propagation: repeatedly find unit clauses and assign
  ;; Returns (ok . assignment) or (conflict . nil)
  (fset 'neovm--sat-unit-propagate
    (lambda (cnf assignment)
      (let ((changed t) (conflict nil))
        (while (and changed (not conflict))
          (setq changed nil)
          (dolist (clause cnf)
            (unless conflict
              (let ((unit (funcall 'neovm--sat-up-unit-p clause assignment)))
                (when unit
                  (let* ((var (abs unit))
                         (val (> unit 0)))
                    (puthash var val assignment)
                    (setq changed t))))
              ;; Check for conflict (empty clause under current assignment)
              (when (funcall 'neovm--sat-up-clause-falsified-p clause assignment)
                (setq conflict t)))))
        (if conflict
            (cons 'conflict nil)
          (cons 'ok assignment)))))

  (unwind-protect
      (list
       ;; Simple unit propagation: (1) AND (-1 2) AND (-2 3)
       ;; => x1=t => x2=t => x3=t
       (let ((assign (make-hash-table)))
         (let ((result (funcall 'neovm--sat-unit-propagate
                                '((1) (-1 2) (-2 3)) assign)))
           (list (car result)
                 (gethash 1 (cdr result))
                 (gethash 2 (cdr result))
                 (gethash 3 (cdr result)))))
       ;; Conflict: (1) AND (-1)
       (let ((assign (make-hash-table)))
         (car (funcall 'neovm--sat-unit-propagate '((1) (-1)) assign)))
       ;; No unit clauses: nothing happens
       (let ((assign (make-hash-table)))
         (let ((result (funcall 'neovm--sat-unit-propagate
                                '((1 2) (-1 3) (2 -3)) assign)))
           (list (car result)
                 ;; No variables assigned
                 (hash-table-count (cdr result)))))
       ;; Chain propagation: (-1) AND (1 2) AND (-2 3) AND (-3 4)
       ;; => x1=nil => x2=t => x3=t => x4=t (from clause (1 2): -1 false, so 2 must be t)
       (let ((assign (make-hash-table)))
         (let ((result (funcall 'neovm--sat-unit-propagate
                                '((-1) (1 2) (-2 3) (-3 4)) assign)))
           (list (car result)
                 (gethash 1 (cdr result))
                 (gethash 2 (cdr result))
                 (gethash 3 (cdr result))
                 (gethash 4 (cdr result))))))
    (fmakunbound 'neovm--sat-up-unit-p)
    (fmakunbound 'neovm--sat-up-clause-sat-p)
    (fmakunbound 'neovm--sat-up-clause-falsified-p)
    (fmakunbound 'neovm--sat-unit-propagate)))
"#;
    let expect = expect_test::expect![[r#""OK ((ok t t t) conflict (ok 0) (ok nil t t t))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Pure literal elimination
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sat_pure_literal_elimination() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  ;; Find pure literals: variables that appear only positively or only negatively
  ;; in unresolved clauses
  (fset 'neovm--sat-ple-find-pure
    (lambda (cnf assignment)
      "Return alist of (var . polarity) for pure literals."
      (let ((positive (make-hash-table))
            (negative (make-hash-table))
            (result nil))
        ;; Scan all unresolved clauses
        (dolist (clause cnf)
          ;; Skip satisfied clauses
          (let ((sat nil))
            (dolist (lit clause)
              (let* ((var (abs lit))
                     (val (gethash var assignment 'unset)))
                (unless (eq val 'unset)
                  (when (if (> lit 0) val (not val))
                    (setq sat t)))))
            (unless sat
              (dolist (lit clause)
                (let ((var (abs lit)))
                  (when (eq (gethash var assignment 'unset) 'unset)
                    (if (> lit 0)
                        (puthash var t positive)
                      (puthash var t negative))))))))
        ;; Pure: appears in only one polarity
        (maphash (lambda (var _)
                   (unless (gethash var negative)
                     (push (cons var t) result)))
                 positive)
        (maphash (lambda (var _)
                   (unless (gethash var positive)
                     (push (cons var nil) result)))
                 negative)
        (sort result (lambda (a b) (< (car a) (car b)))))))

  ;; Apply pure literal elimination
  (fset 'neovm--sat-ple-eliminate
    (lambda (cnf assignment)
      "Assign all pure literals. Returns updated assignment."
      (let ((pures (funcall 'neovm--sat-ple-find-pure cnf assignment)))
        (dolist (p pures)
          (puthash (car p) (cdr p) assignment))
        assignment)))

  (unwind-protect
      (list
       ;; x1 appears only positive, x2 only negative, x3 both
       ;; (1 3) AND (-2 -3) AND (1 -2)
       (let ((assign (make-hash-table)))
         (funcall 'neovm--sat-ple-find-pure '((1 3) (-2 -3) (1 -2)) assign))
       ;; All pure: (1) (2) (3) => all positive
       (let ((assign (make-hash-table)))
         (funcall 'neovm--sat-ple-find-pure '((1) (2) (3)) assign))
       ;; No pure: (1 -2) (-1 2)
       (let ((assign (make-hash-table)))
         (funcall 'neovm--sat-ple-find-pure '((1 -2) (-1 2)) assign))
       ;; With some already assigned: x1=t, check remaining
       ;; (1 2) (-2 3) (-3) => x1 assigned, skip clauses with x1 satisfied
       (let ((assign (make-hash-table)))
         (puthash 1 t assign)
         ;; clause (1 2) is satisfied by x1=t, so x2 doesn't count there
         ;; remaining: (-2 3) (-3) => x2 neg, x3 both => x2 is pure negative
         (funcall 'neovm--sat-ple-find-pure '((1 2) (-2 3) (-3)) assign))
       ;; Elimination updates assignment
       (let ((assign (make-hash-table)))
         (funcall 'neovm--sat-ple-eliminate '((1 3) (-2 -3) (1 -2)) assign)
         (list (gethash 1 assign 'unset)
               (gethash 2 assign 'unset)
               (gethash 3 assign 'unset))))
    (fmakunbound 'neovm--sat-ple-find-pure)
    (fmakunbound 'neovm--sat-ple-eliminate)))
"#;
    let expect = expect_test::expect![[
        r#""OK (((1 . t) (2)) ((1 . t) (2 . t) (3 . t)) nil ((2)) (t nil unset))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Full DPLL algorithm
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sat_dpll_algorithm() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  ;; Helpers
  (fset 'neovm--sat-d-clause-sat-p
    (lambda (clause assign)
      (let ((sat nil))
        (dolist (lit clause)
          (let* ((v (abs lit))
                 (val (gethash v assign 'unset)))
            (unless (eq val 'unset)
              (when (if (> lit 0) val (not val))
                (setq sat t)))))
        sat)))

  (fset 'neovm--sat-d-clause-falsified-p
    (lambda (clause assign)
      (let ((all-false t))
        (dolist (lit clause)
          (let* ((v (abs lit))
                 (val (gethash v assign 'unset)))
            (cond
             ((eq val 'unset) (setq all-false nil))
             ((if (> lit 0) val (not val))
              (setq all-false nil)))))
        all-false)))

  (fset 'neovm--sat-d-all-sat-p
    (lambda (cnf assign)
      (let ((all t))
        (dolist (clause cnf)
          (unless (funcall 'neovm--sat-d-clause-sat-p clause assign)
            (setq all nil)))
        all)))

  (fset 'neovm--sat-d-any-falsified-p
    (lambda (cnf assign)
      (let ((found nil))
        (dolist (clause cnf)
          (when (funcall 'neovm--sat-d-clause-falsified-p clause assign)
            (setq found t)))
        found)))

  ;; Unit propagation (inline)
  (fset 'neovm--sat-d-propagate
    (lambda (cnf assign)
      (let ((changed t) (conflict nil))
        (while (and changed (not conflict))
          (setq changed nil)
          (dolist (clause cnf)
            (unless conflict
              (let ((unassigned 0) (unit-lit nil) (sat nil))
                (dolist (lit clause)
                  (let* ((v (abs lit))
                         (val (gethash v assign 'unset)))
                    (cond
                     ((eq val 'unset)
                      (setq unassigned (1+ unassigned))
                      (setq unit-lit lit))
                     ((if (> lit 0) val (not val))
                      (setq sat t)))))
                (when (and (not sat) (= unassigned 1) unit-lit)
                  (puthash (abs unit-lit) (> unit-lit 0) assign)
                  (setq changed t))
                (when (and (not sat) (= unassigned 0))
                  (setq conflict t))))))
        (not conflict))))

  ;; Pick first unassigned variable
  (fset 'neovm--sat-d-pick-var
    (lambda (cnf assign)
      (let ((found nil))
        (dolist (clause cnf)
          (unless found
            (dolist (lit clause)
              (unless found
                (let ((v (abs lit)))
                  (when (eq (gethash v assign 'unset) 'unset)
                    (setq found v)))))))
        found)))

  ;; Copy hash table
  (fset 'neovm--sat-d-copy-assign
    (lambda (assign)
      (let ((new (make-hash-table)))
        (maphash (lambda (k v) (puthash k v new)) assign)
        new)))

  ;; DPLL: returns assignment hash or nil
  (fset 'neovm--sat-d-solve
    (lambda (cnf assign)
      ;; Unit propagation
      (if (not (funcall 'neovm--sat-d-propagate cnf assign))
          nil  ;; conflict
        (cond
         ;; All clauses satisfied
         ((funcall 'neovm--sat-d-all-sat-p cnf assign) assign)
         ;; Any clause falsified
         ((funcall 'neovm--sat-d-any-falsified-p cnf assign) nil)
         ;; Choose variable and branch
         (t (let ((var (funcall 'neovm--sat-d-pick-var cnf assign)))
              (if (null var)
                  ;; All assigned but not all satisfied => UNSAT
                  nil
                ;; Try true
                (let ((a1 (funcall 'neovm--sat-d-copy-assign assign)))
                  (puthash var t a1)
                  (let ((r (funcall 'neovm--sat-d-solve cnf a1)))
                    (if r r
                      ;; Try false
                      (let ((a2 (funcall 'neovm--sat-d-copy-assign assign)))
                        (puthash var nil a2)
                        (funcall 'neovm--sat-d-solve cnf a2))))))))))))

  (unwind-protect
      (list
       ;; SAT: (1 2) AND (-1 2) AND (1 -2)
       ;; Solution: x1=t, x2=t satisfies all
       (let* ((cnf '((1 2) (-1 2) (1 -2)))
              (result (funcall 'neovm--sat-d-solve cnf (make-hash-table))))
         (list (not (null result))
               (when result
                 (funcall 'neovm--sat-d-all-sat-p cnf result))))
       ;; UNSAT: (1) AND (-1)
       (null (funcall 'neovm--sat-d-solve '((1) (-1)) (make-hash-table)))
       ;; SAT: simple tautology (1 -1)
       (not (null (funcall 'neovm--sat-d-solve '((1 -1)) (make-hash-table))))
       ;; UNSAT: (1 2) AND (-1 -2) AND (1 -2) AND (-1 2) (XOR + anti-XOR)
       (null (funcall 'neovm--sat-d-solve
                      '((1 2) (-1 -2) (1 -2) (-1 2))
                      (make-hash-table)))
       ;; SAT: 3-SAT instance
       ;; (1 2 3) AND (-1 -2 3) AND (1 -2 -3) AND (-1 2 -3)
       (let* ((cnf '((1 2 3) (-1 -2 3) (1 -2 -3) (-1 2 -3)))
              (result (funcall 'neovm--sat-d-solve cnf (make-hash-table))))
         (list (not (null result))
               (when result
                 (funcall 'neovm--sat-d-all-sat-p cnf result))))
       ;; Empty formula: trivially SAT
       (not (null (funcall 'neovm--sat-d-solve nil (make-hash-table))))
       ;; Single empty clause: UNSAT
       (null (funcall 'neovm--sat-d-solve '(()) (make-hash-table))))
    (fmakunbound 'neovm--sat-d-clause-sat-p)
    (fmakunbound 'neovm--sat-d-clause-falsified-p)
    (fmakunbound 'neovm--sat-d-all-sat-p)
    (fmakunbound 'neovm--sat-d-any-falsified-p)
    (fmakunbound 'neovm--sat-d-propagate)
    (fmakunbound 'neovm--sat-d-pick-var)
    (fmakunbound 'neovm--sat-d-copy-assign)
    (fmakunbound 'neovm--sat-d-solve)))
"#;
    let expect = expect_test::expect![[r#""OK ((t t) t t t (t t) t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Model extraction and verification
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sat_model_extraction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  ;; Reuse solve infrastructure (minimal)
  (fset 'neovm--sat-m-propagate
    (lambda (cnf assign)
      (let ((changed t) (conflict nil))
        (while (and changed (not conflict))
          (setq changed nil)
          (dolist (clause cnf)
            (unless conflict
              (let ((unassigned 0) (unit-lit nil) (sat nil))
                (dolist (lit clause)
                  (let* ((v (abs lit)) (val (gethash v assign 'unset)))
                    (cond ((eq val 'unset) (setq unassigned (1+ unassigned)) (setq unit-lit lit))
                          ((if (> lit 0) val (not val)) (setq sat t)))))
                (when (and (not sat) (= unassigned 1) unit-lit)
                  (puthash (abs unit-lit) (> unit-lit 0) assign) (setq changed t))
                (when (and (not sat) (= unassigned 0)) (setq conflict t))))))
        (not conflict))))

  (fset 'neovm--sat-m-all-sat-p
    (lambda (cnf assign)
      (let ((all t))
        (dolist (c cnf) (unless (let ((sat nil))
                                  (dolist (lit c) (let* ((v (abs lit)) (val (gethash v assign 'unset)))
                                                    (unless (eq val 'unset)
                                                      (when (if (> lit 0) val (not val)) (setq sat t))))) sat)
                          (setq all nil))) all)))

  (fset 'neovm--sat-m-any-falsified-p
    (lambda (cnf assign)
      (let ((found nil))
        (dolist (c cnf) (when (let ((af t))
                                (dolist (lit c) (let* ((v (abs lit)) (val (gethash v assign 'unset)))
                                                  (cond ((eq val 'unset) (setq af nil))
                                                        ((if (> lit 0) val (not val)) (setq af nil))))) af)
                          (setq found t))) found)))

  (fset 'neovm--sat-m-copy
    (lambda (a) (let ((n (make-hash-table))) (maphash (lambda (k v) (puthash k v n)) a) n)))

  (fset 'neovm--sat-m-solve
    (lambda (cnf assign)
      (if (not (funcall 'neovm--sat-m-propagate cnf assign)) nil
        (cond
         ((funcall 'neovm--sat-m-all-sat-p cnf assign) assign)
         ((funcall 'neovm--sat-m-any-falsified-p cnf assign) nil)
         (t (let ((var nil))
              (dolist (c cnf)
                (unless var (dolist (lit c)
                              (unless var (let ((v (abs lit)))
                                            (when (eq (gethash v assign 'unset) 'unset) (setq var v)))))))
              (if (null var) nil
                (let ((a1 (funcall 'neovm--sat-m-copy assign)))
                  (puthash var t a1)
                  (or (funcall 'neovm--sat-m-solve cnf a1)
                      (let ((a2 (funcall 'neovm--sat-m-copy assign)))
                        (puthash var nil a2)
                        (funcall 'neovm--sat-m-solve cnf a2)))))))))))

  ;; Extract model as sorted alist
  (fset 'neovm--sat-m-extract
    (lambda (assign vars)
      (let ((model nil))
        (dolist (v vars)
          (push (cons v (if (gethash v assign) 'true 'false)) model))
        (sort model (lambda (a b) (< (car a) (car b)))))))

  ;; Verify model against CNF
  (fset 'neovm--sat-m-verify
    (lambda (cnf model-alist)
      "Check every clause has at least one true literal."
      (let ((assign (make-hash-table)))
        (dolist (p model-alist)
          (puthash (car p) (eq (cdr p) 'true) assign))
        (funcall 'neovm--sat-m-all-sat-p cnf assign))))

  (unwind-protect
      (list
       ;; Solve and extract model for 3-variable problem
       (let* ((cnf '((1 2) (-1 3) (-2 -3) (1 3)))
              (result (funcall 'neovm--sat-m-solve cnf (make-hash-table))))
         (when result
           (let ((model (funcall 'neovm--sat-m-extract result '(1 2 3))))
             (list model
                   (funcall 'neovm--sat-m-verify cnf model)))))
       ;; 4-variable satisfiable: (1 2 3 4) AND (-1 -2) AND (-3 -4) AND (1 3) AND (2 4)
       (let* ((cnf '((1 2 3 4) (-1 -2) (-3 -4) (1 3) (2 4)))
              (result (funcall 'neovm--sat-m-solve cnf (make-hash-table))))
         (list (not (null result))
               (when result
                 (funcall 'neovm--sat-m-verify cnf
                          (funcall 'neovm--sat-m-extract result '(1 2 3 4))))))
       ;; Pigeonhole: 3 pigeons, 2 holes (UNSAT)
       ;; Variables: p(i,j) = pigeon i in hole j
       ;; p11=1, p12=2, p21=3, p22=4, p31=5, p32=6
       ;; Each pigeon in at least one hole: (1 2) (3 4) (5 6)
       ;; No two pigeons in same hole: (-1 -3) (-1 -5) (-3 -5) (-2 -4) (-2 -6) (-4 -6)
       (let* ((cnf '((1 2) (3 4) (5 6)
                      (-1 -3) (-1 -5) (-3 -5)
                      (-2 -4) (-2 -6) (-4 -6)))
              (result (funcall 'neovm--sat-m-solve cnf (make-hash-table))))
         (null result))
       ;; Verify with known model
       (funcall 'neovm--sat-m-verify
                '((1 2) (-1 3))
                '((1 . true) (2 . true) (3 . true))))
    (fmakunbound 'neovm--sat-m-propagate)
    (fmakunbound 'neovm--sat-m-all-sat-p)
    (fmakunbound 'neovm--sat-m-any-falsified-p)
    (fmakunbound 'neovm--sat-m-copy)
    (fmakunbound 'neovm--sat-m-solve)
    (fmakunbound 'neovm--sat-m-extract)
    (fmakunbound 'neovm--sat-m-verify)))
"#;
    let expect =
        expect_test::expect![[r#""OK ((((1 . true) (2 . false) (3 . true)) t) (t t) t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// SAT encoding of graph coloring and verification
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sat_graph_coloring_encoding() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  ;; Encode k-coloring of a graph as SAT
  ;; Variable x(v,c) = node v has color c
  ;; var-id = (v-1)*k + c  (1-indexed)

  (fset 'neovm--sat-gc-var
    (lambda (node color k)
      "Variable id for node NODE having color COLOR (both 1-indexed)."
      (+ (* (1- node) k) color)))

  ;; Generate CNF for k-coloring
  (fset 'neovm--sat-gc-encode
    (lambda (num-nodes k edges)
      "Generate CNF for k-coloring. EDGES is list of (u . v) pairs."
      (let ((clauses nil))
        ;; At least one color per node
        (dotimes (v num-nodes)
          (let ((clause nil))
            (dotimes (c k)
              (push (funcall 'neovm--sat-gc-var (1+ v) (1+ c) k) clause))
            (push (nreverse clause) clauses)))
        ;; Adjacent nodes have different colors
        (dolist (edge edges)
          (let ((u (car edge)) (v (cdr edge)))
            (dotimes (c k)
              ;; NOT (x(u,c) AND x(v,c)) => (-x(u,c) OR -x(v,c))
              (push (list (- (funcall 'neovm--sat-gc-var u (1+ c) k))
                          (- (funcall 'neovm--sat-gc-var v (1+ c) k)))
                    clauses))))
        (nreverse clauses))))

  ;; Minimal DPLL solver
  (fset 'neovm--sat-gc-propagate
    (lambda (cnf assign)
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

  (fset 'neovm--sat-gc-all-sat
    (lambda (cnf a) (let ((ok t)) (dolist (c cnf) (unless (let ((s nil))
      (dolist (l c) (let* ((v (abs l)) (val (gethash v a 'unset)))
        (unless (eq val 'unset) (when (if (> l 0) val (not val)) (setq s t))))) s)
      (setq ok nil))) ok)))

  (fset 'neovm--sat-gc-copy
    (lambda (a) (let ((n (make-hash-table))) (maphash (lambda (k v) (puthash k v n)) a) n)))

  (fset 'neovm--sat-gc-solve
    (lambda (cnf assign)
      (if (not (funcall 'neovm--sat-gc-propagate cnf assign)) nil
        (if (funcall 'neovm--sat-gc-all-sat cnf assign) assign
          (let ((var nil))
            (dolist (c cnf) (unless var (dolist (l c) (unless var
              (let ((v (abs l))) (when (eq (gethash v assign 'unset) 'unset) (setq var v)))))))
            (if (null var) nil
              (let ((a1 (funcall 'neovm--sat-gc-copy assign)))
                (puthash var t a1)
                (or (funcall 'neovm--sat-gc-solve cnf a1)
                    (let ((a2 (funcall 'neovm--sat-gc-copy assign)))
                      (puthash var nil a2)
                      (funcall 'neovm--sat-gc-solve cnf a2))))))))))

  ;; Decode coloring from assignment
  (fset 'neovm--sat-gc-decode
    (lambda (assign num-nodes k)
      (let ((coloring nil))
        (dotimes (v num-nodes)
          (let ((color nil))
            (dotimes (c k)
              (when (gethash (funcall 'neovm--sat-gc-var (1+ v) (1+ c) k) assign)
                (setq color (1+ c))))
            (push (cons (1+ v) (or color 0)) coloring)))
        (sort coloring (lambda (a b) (< (car a) (car b)))))))

  (unwind-protect
      (list
       ;; Triangle graph, 3 colors: should be SAT
       (let* ((cnf (funcall 'neovm--sat-gc-encode 3 3 '((1 . 2) (2 . 3) (1 . 3))))
              (result (funcall 'neovm--sat-gc-solve cnf (make-hash-table))))
         (list (not (null result))
               (when result
                 (let ((coloring (funcall 'neovm--sat-gc-decode result 3 3)))
                   ;; All adjacent have different colors
                   (and (/= (cdr (assq 1 coloring)) (cdr (assq 2 coloring)))
                        (/= (cdr (assq 2 coloring)) (cdr (assq 3 coloring)))
                        (/= (cdr (assq 1 coloring)) (cdr (assq 3 coloring))))))))
       ;; Triangle graph, 2 colors: should be UNSAT (chromatic number = 3)
       (let* ((cnf (funcall 'neovm--sat-gc-encode 3 2 '((1 . 2) (2 . 3) (1 . 3))))
              (result (funcall 'neovm--sat-gc-solve cnf (make-hash-table))))
         (null result))
       ;; Path graph 1-2-3, 2 colors: should be SAT
       (let* ((cnf (funcall 'neovm--sat-gc-encode 3 2 '((1 . 2) (2 . 3))))
              (result (funcall 'neovm--sat-gc-solve cnf (make-hash-table))))
         (list (not (null result))
               (when result
                 (let ((c (funcall 'neovm--sat-gc-decode result 3 2)))
                   (/= (cdr (assq 1 c)) (cdr (assq 2 c)))))))
       ;; Complete graph K4, 3 colors: UNSAT (needs 4)
       (let* ((cnf (funcall 'neovm--sat-gc-encode 4 3
                             '((1 . 2) (1 . 3) (1 . 4) (2 . 3) (2 . 4) (3 . 4))))
              (result (funcall 'neovm--sat-gc-solve cnf (make-hash-table))))
         (null result)))
    (fmakunbound 'neovm--sat-gc-var)
    (fmakunbound 'neovm--sat-gc-encode)
    (fmakunbound 'neovm--sat-gc-propagate)
    (fmakunbound 'neovm--sat-gc-all-sat)
    (fmakunbound 'neovm--sat-gc-copy)
    (fmakunbound 'neovm--sat-gc-solve)
    (fmakunbound 'neovm--sat-gc-decode)))
"#;
    let expect = expect_test::expect![[r#""OK ((t t) t (t t) t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// SAT: implication graph and 2-SAT in polynomial time
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sat_two_sat() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  ;; 2-SAT solver using implication graph + SCC approach (simplified)
  ;; Each 2-clause (a OR b) is equivalent to (-a => b) AND (-b => a)
  ;; Build implication graph, find SCCs, check consistency

  ;; Build adjacency list for implication graph
  ;; Nodes: positive and negative literals
  ;; Node encoding: lit -> if positive: 2*var, if negative: 2*var+1
  (fset 'neovm--sat2-node
    (lambda (lit)
      (if (> lit 0) (* 2 lit) (1+ (* 2 (- lit))))))

  (fset 'neovm--sat2-neg-node
    (lambda (lit)
      (funcall 'neovm--sat2-node (- lit))))

  ;; Build graph from 2-SAT clauses
  (fset 'neovm--sat2-build-graph
    (lambda (clauses num-vars)
      (let ((graph (make-hash-table)))
        ;; Initialize
        (dotimes (v num-vars)
          (let ((pos (* 2 (1+ v))) (neg (1+ (* 2 (1+ v)))))
            (puthash pos nil graph)
            (puthash neg nil graph)))
        ;; Add implications
        (dolist (clause clauses)
          (let ((a (nth 0 clause)) (b (nth 1 clause)))
            ;; -a => b
            (let ((na (funcall 'neovm--sat2-neg-node a))
                  (nb (funcall 'neovm--sat2-node b)))
              (puthash na (cons nb (gethash na graph)) graph))
            ;; -b => a
            (let ((nb2 (funcall 'neovm--sat2-neg-node b))
                  (na2 (funcall 'neovm--sat2-node a)))
              (puthash nb2 (cons na2 (gethash nb2 graph)) graph))))
        graph)))

  ;; Simplified reachability check: can node reach target?
  (fset 'neovm--sat2-reachable
    (lambda (graph start target)
      (let ((visited (make-hash-table))
            (stack (list start))
            (found nil))
        (while (and stack (not found))
          (let ((node (car stack)))
            (setq stack (cdr stack))
            (unless (gethash node visited)
              (puthash node t visited)
              (when (= node target) (setq found t))
              (dolist (next (gethash node graph))
                (unless (gethash next visited)
                  (push next stack))))))
        found)))

  ;; Check 2-SAT satisfiability
  ;; UNSAT iff exists var where var reaches -var AND -var reaches var
  (fset 'neovm--sat2-satisfiable-p
    (lambda (clauses num-vars)
      (let ((graph (funcall 'neovm--sat2-build-graph clauses num-vars))
            (sat t))
        (dotimes (i num-vars)
          (when sat
            (let ((pos (* 2 (1+ i)))
                  (neg (1+ (* 2 (1+ i)))))
              (when (and (funcall 'neovm--sat2-reachable graph pos neg)
                         (funcall 'neovm--sat2-reachable graph neg pos))
                (setq sat nil)))))
        sat)))

  (unwind-protect
      (list
       ;; SAT: (1 2) AND (-1 -2) => satisfiable (x1=t,x2=nil or x1=nil,x2=t)
       (funcall 'neovm--sat2-satisfiable-p '((1 2) (-1 -2)) 2)
       ;; UNSAT: (1) AND (-1) encoded as 2-SAT: (1 1) AND (-1 -1)
       (funcall 'neovm--sat2-satisfiable-p '((1 1) (-1 -1)) 1)
       ;; SAT: (1 2) AND (-1 2) AND (1 -2) => x1=t, x2=t works
       (funcall 'neovm--sat2-satisfiable-p '((1 2) (-1 2) (1 -2)) 2)
       ;; UNSAT: all four combinations => no assignment works
       (funcall 'neovm--sat2-satisfiable-p '((1 2) (-1 2) (1 -2) (-1 -2)) 2)
       ;; SAT: chain implications (1 2) AND (-2 3) AND (-3 1)
       (funcall 'neovm--sat2-satisfiable-p '((1 2) (-2 3) (-3 1)) 3)
       ;; Empty clause set: trivially SAT
       (funcall 'neovm--sat2-satisfiable-p nil 3))
    (fmakunbound 'neovm--sat2-node)
    (fmakunbound 'neovm--sat2-neg-node)
    (fmakunbound 'neovm--sat2-build-graph)
    (fmakunbound 'neovm--sat2-reachable)
    (fmakunbound 'neovm--sat2-satisfiable-p)))
"#;
    let expect = expect_test::expect![[r#""OK (t nil t nil t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
