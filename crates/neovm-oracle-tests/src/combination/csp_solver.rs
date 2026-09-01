//! Oracle parity tests for a constraint satisfaction problem (CSP)
//! solver implemented in Elisp. Covers variable/domain representation,
//! constraint checking, simple backtracking, N-queens as CSP,
//! map coloring, and constraint propagation (arc consistency).

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

// ---------------------------------------------------------------------------
// CSP framework: variables, domains, constraints, and backtracking solver
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_csp_solver_basic_framework() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  ;; A CSP variable: (name . domain-list)
  ;; A constraint: (var1 var2 predicate)
  ;; An assignment: alist ((var . val) ...)

  ;; Check if assignment is consistent with all constraints
  (fset 'neovm--csp-consistent-p
    (lambda (assignment constraints)
      (let ((ok t))
        (dolist (c constraints)
          (when ok
            (let ((v1 (nth 0 c))
                  (v2 (nth 1 c))
                  (pred (nth 2 c)))
              (let ((a1 (assq v1 assignment))
                    (a2 (assq v2 assignment)))
                ;; Only check if both variables are assigned
                (when (and a1 a2)
                  (unless (funcall pred (cdr a1) (cdr a2))
                    (setq ok nil)))))))
        ok)))

  ;; Simple backtracking solver
  (fset 'neovm--csp-solve
    (lambda (variables domains constraints)
      "VARIABLES is a list of symbols, DOMAINS is hash-table var->list,
       CONSTRAINTS is list of (var1 var2 pred).
       Returns assignment alist or nil."
      (let ((result nil))
        (fset 'neovm--csp-bt
          (lambda (remaining assignment)
            (if (null remaining)
                (if (funcall 'neovm--csp-consistent-p assignment constraints)
                    (progn (setq result (copy-sequence assignment)) t)
                  nil)
              (let ((var (car remaining))
                    (found nil))
                (dolist (val (gethash var domains))
                  (unless found
                    (let ((new-assign (cons (cons var val) assignment)))
                      (when (funcall 'neovm--csp-consistent-p new-assign constraints)
                        (when (funcall 'neovm--csp-bt (cdr remaining) new-assign)
                          (setq found t))))))
                found))))
        (funcall 'neovm--csp-bt variables nil)
        result)))

  (unwind-protect
      (let ((results nil))
        ;; Simple test: 3 variables A, B, C each in {1, 2, 3}, all different
        (let ((domains (make-hash-table))
              (neq (lambda (x y) (/= x y))))
          (puthash 'a '(1 2 3) domains)
          (puthash 'b '(1 2 3) domains)
          (puthash 'c '(1 2 3) domains)
          (let* ((constraints (list (list 'a 'b neq)
                                    (list 'a 'c neq)
                                    (list 'b 'c neq)))
                 (sol (funcall 'neovm--csp-solve '(a b c) domains constraints)))
            (push (list 'all-diff
                        (not (null sol))
                        ;; All values different
                        (when sol
                          (let ((va (cdr (assq 'a sol)))
                                (vb (cdr (assq 'b sol)))
                                (vc (cdr (assq 'c sol))))
                            (and (/= va vb) (/= va vc) (/= vb vc))))
                        ;; Sum is 6
                        (when sol
                          (= 6 (+ (cdr (assq 'a sol))
                                   (cdr (assq 'b sol))
                                   (cdr (assq 'c sol))))))
                  results)))

        ;; Two variables, same domain, must be equal
        (let ((domains (make-hash-table))
              (eq-pred (lambda (x y) (= x y))))
          (puthash 'p '(10 20 30) domains)
          (puthash 'q '(10 20 30) domains)
          (let* ((constraints (list (list 'p 'q eq-pred)))
                 (sol (funcall 'neovm--csp-solve '(p q) domains constraints)))
            (push (list 'must-equal
                        (not (null sol))
                        (when sol (= (cdr (assq 'p sol)) (cdr (assq 'q sol)))))
                  results)))

        ;; Unsatisfiable: 3 vars, domain {1, 2}, all different
        (let ((domains (make-hash-table))
              (neq (lambda (x y) (/= x y))))
          (puthash 'x '(1 2) domains)
          (puthash 'y '(1 2) domains)
          (puthash 'z '(1 2) domains)
          (let* ((constraints (list (list 'x 'y neq)
                                    (list 'x 'z neq)
                                    (list 'y 'z neq)))
                 (sol (funcall 'neovm--csp-solve '(x y z) domains constraints)))
            (push (list 'unsatisfiable (null sol)) results)))

        ;; Ordering constraint: a < b < c
        (let ((domains (make-hash-table))
              (lt-pred (lambda (x y) (< x y))))
          (puthash 'a '(1 2 3 4 5) domains)
          (puthash 'b '(1 2 3 4 5) domains)
          (puthash 'c '(1 2 3 4 5) domains)
          (let* ((constraints (list (list 'a 'b lt-pred)
                                    (list 'b 'c lt-pred)))
                 (sol (funcall 'neovm--csp-solve '(a b c) domains constraints)))
            (push (list 'ordering
                        (not (null sol))
                        (when sol
                          (< (cdr (assq 'a sol))
                             (cdr (assq 'b sol))))
                        (when sol
                          (< (cdr (assq 'b sol))
                             (cdr (assq 'c sol)))))
                  results)))

        (nreverse results))
    (fmakunbound 'neovm--csp-consistent-p)
    (fmakunbound 'neovm--csp-solve)
    (fmakunbound 'neovm--csp-bt)))
"#;
    let expect = expect_test::expect![[
        r#""OK ((all-diff t t t) (must-equal t t) (unsatisfiable t) (ordering t t t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// N-queens (4-queens) as CSP
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_csp_solver_4_queens() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  ;; 4-queens: variables q0..q3 represent column of queen in each row
  ;; Domains: {0, 1, 2, 3} for each
  ;; Constraints: no two queens share column or diagonal

  (fset 'neovm--csp-consistent-p
    (lambda (assignment constraints)
      (let ((ok t))
        (dolist (c constraints)
          (when ok
            (let ((v1 (nth 0 c)) (v2 (nth 1 c)) (pred (nth 2 c)))
              (let ((a1 (assq v1 assignment)) (a2 (assq v2 assignment)))
                (when (and a1 a2)
                  (unless (funcall pred (cdr a1) (cdr a2))
                    (setq ok nil)))))))
        ok)))

  (fset 'neovm--csp-solve-all
    (lambda (variables domains constraints)
      "Find ALL solutions."
      (let ((all-solutions nil))
        (fset 'neovm--csp-bt-all
          (lambda (remaining assignment)
            (if (null remaining)
                (when (funcall 'neovm--csp-consistent-p assignment constraints)
                  (setq all-solutions (cons (copy-sequence assignment) all-solutions)))
              (let ((var (car remaining)))
                (dolist (val (gethash var domains))
                  (let ((new-assign (cons (cons var val) assignment)))
                    (when (funcall 'neovm--csp-consistent-p new-assign constraints)
                      (funcall 'neovm--csp-bt-all (cdr remaining) new-assign))))))))
        (funcall 'neovm--csp-bt-all variables nil)
        (nreverse all-solutions))))

  (unwind-protect
      (let ((domains (make-hash-table))
            (constraints nil))
        ;; Set domains
        (dolist (q '(q0 q1 q2 q3))
          (puthash q '(0 1 2 3) domains))
        ;; Build constraints: for each pair of rows
        (let ((rows '((q0 . 0) (q1 . 1) (q2 . 2) (q3 . 3))))
          (dolist (r1 rows)
            (dolist (r2 rows)
              (when (< (cdr r1) (cdr r2))
                (let ((v1 (car r1)) (ri (cdr r1))
                      (v2 (car r2)) (rj (cdr r2)))
                  ;; Different column and different diagonal
                  (push (list v1 v2
                              (let ((row-diff (- rj ri)))
                                (lambda (c1 c2)
                                  (and (/= c1 c2)
                                       (/= (abs (- c1 c2)) row-diff)))))
                        constraints))))))
        (let ((solutions (funcall 'neovm--csp-solve-all
                                   '(q0 q1 q2 q3) domains constraints)))
          (list
           ;; Number of solutions for 4-queens = 2
           (length solutions)
           ;; Verify each solution
           (let ((all-valid t))
             (dolist (sol solutions)
               (let ((cols (mapcar #'cdr sol)))
                 ;; All columns different
                 (unless (= 4 (length (delete-dups (copy-sequence cols))))
                   (setq all-valid nil))
                 ;; No diagonal attacks
                 (let ((i 0))
                   (while (< i 4)
                     (let ((j (1+ i)))
                       (while (< j 4)
                         (let ((ci (cdr (nth i sol)))
                               (cj (cdr (nth j sol)))
                               (ri i) (rj j))
                           (when (= (abs (- ci cj)) (abs (- ri rj)))
                             (setq all-valid nil)))
                         (setq j (1+ j))))
                     (setq i (1+ i))))))
             all-valid)
           ;; The two solutions (sorted by first column)
           (let ((sorted (sort (copy-sequence solutions)
                                (lambda (a b)
                                  (< (cdr (assq 'q0 a))
                                     (cdr (assq 'q0 b)))))))
             (mapcar (lambda (sol)
                       (list (cdr (assq 'q0 sol))
                             (cdr (assq 'q1 sol))
                             (cdr (assq 'q2 sol))
                             (cdr (assq 'q3 sol))))
                     sorted)))))
    (fmakunbound 'neovm--csp-consistent-p)
    (fmakunbound 'neovm--csp-solve-all)
    (fmakunbound 'neovm--csp-bt-all)))
"#;
    let expect = expect_test::expect![[r#""OK (2 t ((1 3 0 2) (2 0 3 1)))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Map coloring problem (3 regions, 3 colors)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_csp_solver_map_coloring() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (fset 'neovm--csp-consistent-p
    (lambda (assignment constraints)
      (let ((ok t))
        (dolist (c constraints)
          (when ok
            (let ((v1 (nth 0 c)) (v2 (nth 1 c)) (pred (nth 2 c)))
              (let ((a1 (assq v1 assignment)) (a2 (assq v2 assignment)))
                (when (and a1 a2)
                  (unless (funcall pred (cdr a1) (cdr a2))
                    (setq ok nil)))))))
        ok)))

  (fset 'neovm--csp-solve
    (lambda (variables domains constraints)
      (let ((result nil))
        (fset 'neovm--csp-bt
          (lambda (remaining assignment)
            (if (null remaining)
                (if (funcall 'neovm--csp-consistent-p assignment constraints)
                    (progn (setq result (copy-sequence assignment)) t)
                  nil)
              (let ((var (car remaining)) (found nil))
                (dolist (val (gethash var domains))
                  (unless found
                    (let ((new-assign (cons (cons var val) assignment)))
                      (when (funcall 'neovm--csp-consistent-p new-assign constraints)
                        (when (funcall 'neovm--csp-bt (cdr remaining) new-assign)
                          (setq found t))))))
                found))))
        (funcall 'neovm--csp-bt variables nil)
        result)))

  (fset 'neovm--csp-count-solutions
    (lambda (variables domains constraints)
      "Count all solutions."
      (let ((count 0))
        (fset 'neovm--csp-bt-count
          (lambda (remaining assignment)
            (if (null remaining)
                (when (funcall 'neovm--csp-consistent-p assignment constraints)
                  (setq count (1+ count)))
              (let ((var (car remaining)))
                (dolist (val (gethash var domains))
                  (let ((new-assign (cons (cons var val) assignment)))
                    (when (funcall 'neovm--csp-consistent-p new-assign constraints)
                      (funcall 'neovm--csp-bt-count (cdr remaining) new-assign))))))))
        (funcall 'neovm--csp-bt-count variables nil)
        count)))

  (unwind-protect
      (let ((results nil)
            (neq-sym (lambda (x y) (not (eq x y)))))
        ;; Simple triangle: 3 regions all adjacent
        (let ((domains (make-hash-table)))
          (dolist (r '(r1 r2 r3))
            (puthash r '(red green blue) domains))
          (let ((constraints (list (list 'r1 'r2 neq-sym)
                                   (list 'r1 'r3 neq-sym)
                                   (list 'r2 'r3 neq-sym))))
            ;; Find a solution
            (let ((sol (funcall 'neovm--csp-solve '(r1 r2 r3) domains constraints)))
              (push (list 'triangle
                          (not (null sol))
                          ;; All different
                          (when sol
                            (let ((c1 (cdr (assq 'r1 sol)))
                                  (c2 (cdr (assq 'r2 sol)))
                                  (c3 (cdr (assq 'r3 sol))))
                              (and (not (eq c1 c2))
                                   (not (eq c1 c3))
                                   (not (eq c2 c3)))))
                          ;; Count solutions: 3! = 6
                          (funcall 'neovm--csp-count-solutions
                                   '(r1 r2 r3) domains constraints))
                    results))))

        ;; Line graph: r1-r2-r3 (r1 and r3 not adjacent)
        (let ((domains (make-hash-table)))
          (dolist (r '(r1 r2 r3))
            (puthash r '(red green blue) domains))
          (let ((constraints (list (list 'r1 'r2 neq-sym)
                                   (list 'r2 'r3 neq-sym))))
            (push (list 'line
                        (funcall 'neovm--csp-count-solutions
                                 '(r1 r2 r3) domains constraints))
                  results)))

        ;; 4 regions forming a square: r1-r2, r2-r3, r3-r4, r4-r1
        (let ((domains (make-hash-table)))
          (dolist (r '(r1 r2 r3 r4))
            (puthash r '(red green blue) domains))
          (let ((constraints (list (list 'r1 'r2 neq-sym)
                                   (list 'r2 'r3 neq-sym)
                                   (list 'r3 'r4 neq-sym)
                                   (list 'r4 'r1 neq-sym))))
            (let ((sol (funcall 'neovm--csp-solve '(r1 r2 r3 r4) domains constraints)))
              (push (list 'square
                          (not (null sol))
                          (funcall 'neovm--csp-count-solutions
                                   '(r1 r2 r3 r4) domains constraints))
                    results))))

        ;; 2 colors on triangle: unsatisfiable (chromatic number = 3)
        (let ((domains (make-hash-table)))
          (dolist (r '(r1 r2 r3))
            (puthash r '(red green) domains))
          (let ((constraints (list (list 'r1 'r2 neq-sym)
                                   (list 'r1 'r3 neq-sym)
                                   (list 'r2 'r3 neq-sym))))
            (push (list 'two-color-triangle
                        (null (funcall 'neovm--csp-solve '(r1 r2 r3) domains constraints)))
                  results)))

        (nreverse results))
    (fmakunbound 'neovm--csp-consistent-p)
    (fmakunbound 'neovm--csp-solve)
    (fmakunbound 'neovm--csp-bt)
    (fmakunbound 'neovm--csp-count-solutions)
    (fmakunbound 'neovm--csp-bt-count)))
"#;
    let expect = expect_test::expect![[
        r#""OK ((triangle t t 6) (line 12) (square t 18) (two-color-triangle t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Constraint propagation: arc consistency (AC-3)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_csp_solver_arc_consistency() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  ;; AC-3: revise domains based on binary constraints
  ;; Domains stored in hash-table, constraints as (var1 var2 pred)

  (fset 'neovm--csp-ac3-revise
    (lambda (domains xi xj pred)
      "Remove values from XI's domain that have no support in XJ's domain.
       Returns t if domain was revised."
      (let ((revised nil)
            (new-di nil))
        (dolist (vi (gethash xi domains))
          (let ((supported nil))
            (dolist (vj (gethash xj domains))
              (when (funcall pred vi vj)
                (setq supported t)))
            (if supported
                (push vi new-di)
              (setq revised t))))
        (when revised
          (puthash xi (nreverse new-di) domains))
        revised)))

  (fset 'neovm--csp-ac3
    (lambda (domains constraints)
      "Run AC-3 algorithm. Returns t if all domains non-empty."
      ;; Build queue of all arcs
      (let ((queue nil))
        (dolist (c constraints)
          (push (list (nth 0 c) (nth 1 c) (nth 2 c)) queue)
          ;; Reverse arc: pred must be symmetric or we invert
          (push (list (nth 1 c) (nth 0 c)
                      (let ((p (nth 2 c)))
                        (lambda (x y) (funcall p y x))))
                queue))
        (while queue
          (let* ((arc (car queue))
                 (xi (nth 0 arc))
                 (xj (nth 1 arc))
                 (pred (nth 2 arc)))
            (setq queue (cdr queue))
            (when (funcall 'neovm--csp-ac3-revise domains xi xj pred)
              (when (null (gethash xi domains))
                (setq queue nil))  ;; Early exit on wipeout
              ;; Re-enqueue neighbors of xi
              (dolist (c constraints)
                (cond
                 ((and (eq (nth 1 c) xi) (not (eq (nth 0 c) xj)))
                  (push (list (nth 0 c) xi (nth 2 c)) queue))
                 ((and (eq (nth 0 c) xi) (not (eq (nth 1 c) xj)))
                  (push (list (nth 1 c) xi
                              (let ((p (nth 2 c)))
                                (lambda (x y) (funcall p y x))))
                        queue)))))))
        ;; Check no domain empty
        (let ((ok t))
          (maphash (lambda (k v) (when (null v) (setq ok nil))) domains)
          ok))))

  (unwind-protect
      (let ((results nil)
            (neq (lambda (x y) (/= x y))))
        ;; AC-3 on all-different with 3 vars, domain {1,2,3}
        (let ((domains (make-hash-table)))
          (puthash 'a '(1 2 3) domains)
          (puthash 'b '(1 2 3) domains)
          (puthash 'c '(1 2 3) domains)
          (let ((constraints (list (list 'a 'b neq)
                                   (list 'a 'c neq)
                                   (list 'b 'c neq))))
            (let ((consistent (funcall 'neovm--csp-ac3 domains constraints)))
              (push (list 'ac3-basic
                          consistent
                          ;; Domains should still have all values (AC-3 alone
                          ;; cannot fully solve all-different for 3 vars / 3 values)
                          (length (gethash 'a domains))
                          (length (gethash 'b domains))
                          (length (gethash 'c domains)))
                    results))))

        ;; AC-3 with one pre-assigned variable forces propagation
        (let ((domains (make-hash-table)))
          (puthash 'a '(1) domains)  ;; a is assigned to 1
          (puthash 'b '(1 2 3) domains)
          (puthash 'c '(1 2 3) domains)
          (let ((constraints (list (list 'a 'b neq)
                                   (list 'a 'c neq)
                                   (list 'b 'c neq))))
            (funcall 'neovm--csp-ac3 domains constraints)
            (push (list 'ac3-propagate
                        ;; b and c should not contain 1
                        (not (memq 1 (gethash 'b domains)))
                        (not (memq 1 (gethash 'c domains)))
                        ;; b and c should have {2, 3}
                        (sort (copy-sequence (gethash 'b domains)) #'<)
                        (sort (copy-sequence (gethash 'c domains)) #'<))
                  results)))

        ;; AC-3 detects wipeout (impossible)
        (let ((domains (make-hash-table)))
          (puthash 'a '(1) domains)
          (puthash 'b '(1) domains)
          (let ((constraints (list (list 'a 'b neq))))
            (push (list 'ac3-wipeout
                        (not (funcall 'neovm--csp-ac3 domains constraints)))
                  results)))

        ;; AC-3 with inequality constraint on larger domain
        (let ((domains (make-hash-table))
              (lt (lambda (x y) (< x y))))
          (puthash 'x '(1 2 3 4 5) domains)
          (puthash 'y '(1 2 3 4 5) domains)
          (let ((constraints (list (list 'x 'y lt))))
            (funcall 'neovm--csp-ac3 domains constraints)
            ;; x should lose 5 (nothing greater), y should lose 1
            (push (list 'ac3-lt
                        (sort (copy-sequence (gethash 'x domains)) #'<)
                        (sort (copy-sequence (gethash 'y domains)) #'<))
                  results)))

        (nreverse results))
    (fmakunbound 'neovm--csp-ac3-revise)
    (fmakunbound 'neovm--csp-ac3)))
"#;
    let expect = expect_test::expect![[
        r#""OK ((ac3-basic t 3 3 3) (ac3-propagate t t (2 3) (2 3)) (ac3-wipeout t) (ac3-lt (1 2 3 4) (2 3 4 5)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// CSP with forward checking optimization
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_csp_solver_forward_checking() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  ;; Forward checking: when assigning a variable, immediately
  ;; prune domains of unassigned neighbors.

  (fset 'neovm--csp-fc-solve
    (lambda (variables domains constraints)
      "Solve CSP with forward checking."
      (let ((result nil))
        (fset 'neovm--csp-fc-bt
          (lambda (remaining assignment domains-copy)
            (if (null remaining)
                (progn (setq result (copy-sequence assignment)) t)
              (let ((var (car remaining))
                    (found nil))
                (dolist (val (gethash var domains-copy))
                  (unless found
                    (let ((new-assign (cons (cons var val) assignment))
                          ;; Deep copy domains for this branch
                          (new-domains (copy-hash-table domains-copy))
                          (wipeout nil))
                      ;; Forward check: prune domains of unassigned vars
                      (dolist (c constraints)
                        (unless wipeout
                          (let ((v1 (nth 0 c)) (v2 (nth 1 c)) (pred (nth 2 c)))
                            (cond
                             ((and (eq v1 var) (not (assq v2 new-assign)))
                              ;; Prune v2's domain
                              (let ((new-dom nil))
                                (dolist (dv (gethash v2 new-domains))
                                  (when (funcall pred val dv)
                                    (push dv new-dom)))
                                (puthash v2 (nreverse new-dom) new-domains)
                                (when (null (gethash v2 new-domains))
                                  (setq wipeout t))))
                             ((and (eq v2 var) (not (assq v1 new-assign)))
                              (let ((new-dom nil))
                                (dolist (dv (gethash v1 new-domains))
                                  (when (funcall pred dv val)
                                    (push dv new-dom)))
                                (puthash v1 (nreverse new-dom) new-domains)
                                (when (null (gethash v1 new-domains))
                                  (setq wipeout t))))))))
                      (unless wipeout
                        (when (funcall 'neovm--csp-fc-bt
                                       (cdr remaining) new-assign new-domains)
                          (setq found t))))))
                found))))
        (funcall 'neovm--csp-fc-bt variables nil (copy-hash-table domains))
        result)))

  (unwind-protect
      (let ((results nil)
            (neq (lambda (x y) (/= x y))))
        ;; All-different 4 vars in {1,2,3,4}
        (let ((domains (make-hash-table)))
          (dolist (v '(a b c d))
            (puthash v '(1 2 3 4) domains))
          (let ((constraints (list (list 'a 'b neq)
                                   (list 'a 'c neq)
                                   (list 'a 'd neq)
                                   (list 'b 'c neq)
                                   (list 'b 'd neq)
                                   (list 'c 'd neq))))
            (let ((sol (funcall 'neovm--csp-fc-solve '(a b c d) domains constraints)))
              (push (list 'fc-4diff
                          (not (null sol))
                          ;; Sum = 1+2+3+4 = 10
                          (when sol
                            (= 10 (+ (cdr (assq 'a sol))
                                      (cdr (assq 'b sol))
                                      (cdr (assq 'c sol))
                                      (cdr (assq 'd sol)))))
                          ;; All values unique
                          (when sol
                            (let ((vals (mapcar #'cdr sol)))
                              (= 4 (length (delete-dups (copy-sequence vals)))))))
                    results))))

        ;; Ordering + inequality combined
        (let ((domains (make-hash-table))
              (lt (lambda (x y) (< x y))))
          (puthash 'x '(1 2 3 4 5) domains)
          (puthash 'y '(1 2 3 4 5) domains)
          (puthash 'z '(1 2 3 4 5) domains)
          (let ((constraints (list (list 'x 'y lt)
                                   (list 'y 'z lt))))
            (let ((sol (funcall 'neovm--csp-fc-solve '(x y z) domains constraints)))
              (push (list 'fc-ordered
                          (not (null sol))
                          (when sol
                            (and (< (cdr (assq 'x sol)) (cdr (assq 'y sol)))
                                 (< (cdr (assq 'y sol)) (cdr (assq 'z sol))))))
                    results))))

        ;; Unsatisfiable with FC
        (let ((domains (make-hash-table))
              (neq (lambda (x y) (/= x y))))
          (puthash 'a '(1 2) domains)
          (puthash 'b '(1 2) domains)
          (puthash 'c '(1 2) domains)
          (let ((constraints (list (list 'a 'b neq)
                                   (list 'a 'c neq)
                                   (list 'b 'c neq))))
            (push (list 'fc-unsat
                        (null (funcall 'neovm--csp-fc-solve '(a b c) domains constraints)))
                  results)))

        (nreverse results))
    (fmakunbound 'neovm--csp-fc-solve)
    (fmakunbound 'neovm--csp-fc-bt)))
"#;
    let expect = expect_test::expect![[r#""OK ((fc-4diff t t t) (fc-ordered t t) (fc-unsat t))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// CSP: magic square (2x2 solvability test)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_csp_solver_magic_square() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A 2x2 magic square with values 1-4 where both rows and both columns
    // sum to the same value. (This is actually impossible for a true 2x2
    // magic square, but we test the solver's ability to determine that,
    // and also test a relaxed version.)
    let form = r#"
(progn
  (fset 'neovm--csp-consistent-p
    (lambda (assignment constraints)
      (let ((ok t))
        (dolist (c constraints)
          (when ok
            (let ((v1 (nth 0 c)) (v2 (nth 1 c)) (pred (nth 2 c)))
              (let ((a1 (assq v1 assignment)) (a2 (assq v2 assignment)))
                (when (and a1 a2)
                  (unless (funcall pred (cdr a1) (cdr a2))
                    (setq ok nil)))))))
        ok)))

  (fset 'neovm--csp-solve
    (lambda (variables domains constraints)
      (let ((result nil))
        (fset 'neovm--csp-bt
          (lambda (remaining assignment)
            (if (null remaining)
                (if (funcall 'neovm--csp-consistent-p assignment constraints)
                    (progn (setq result (copy-sequence assignment)) t)
                  nil)
              (let ((var (car remaining)) (found nil))
                (dolist (val (gethash var domains))
                  (unless found
                    (let ((new-assign (cons (cons var val) assignment)))
                      (when (funcall 'neovm--csp-consistent-p new-assign constraints)
                        (when (funcall 'neovm--csp-bt (cdr remaining) new-assign)
                          (setq found t))))))
                found))))
        (funcall 'neovm--csp-bt variables nil)
        result)))

  (fset 'neovm--csp-count-solutions
    (lambda (variables domains constraints)
      (let ((count 0))
        (fset 'neovm--csp-bt-count
          (lambda (remaining assignment)
            (if (null remaining)
                (when (funcall 'neovm--csp-consistent-p assignment constraints)
                  (setq count (1+ count)))
              (let ((var (car remaining)))
                (dolist (val (gethash var domains))
                  (let ((new-assign (cons (cons var val) assignment)))
                    (when (funcall 'neovm--csp-consistent-p new-assign constraints)
                      (funcall 'neovm--csp-bt-count (cdr remaining) new-assign))))))))
        (funcall 'neovm--csp-bt-count variables nil)
        count)))

  (unwind-protect
      (let ((results nil))
        ;; Relaxed problem: 2x2 grid, values 1-4 (not all-different),
        ;; each row sums to 5, each column sums to 5.
        ;; a b   => a+b=5, c+d=5, a+c=5, b+d=5
        ;; c d
        (let ((domains (make-hash-table))
              ;; sum-to-5 as a binary constraint
              (sum5 (lambda (x y) (= (+ x y) 5))))
          (dolist (v '(a b c d))
            (puthash v '(1 2 3 4) domains))
          (let ((constraints (list (list 'a 'b sum5)
                                   (list 'c 'd sum5)
                                   (list 'a 'c sum5)
                                   (list 'b 'd sum5))))
            (let ((sol (funcall 'neovm--csp-solve '(a b c d) domains constraints)))
              (push (list 'relaxed-2x2
                          (not (null sol))
                          (when sol
                            (list (cdr (assq 'a sol)) (cdr (assq 'b sol))
                                  (cdr (assq 'c sol)) (cdr (assq 'd sol))))
                          (when sol
                            (and (= 5 (+ (cdr (assq 'a sol)) (cdr (assq 'b sol))))
                                 (= 5 (+ (cdr (assq 'c sol)) (cdr (assq 'd sol))))
                                 (= 5 (+ (cdr (assq 'a sol)) (cdr (assq 'c sol))))
                                 (= 5 (+ (cdr (assq 'b sol)) (cdr (assq 'd sol)))))))
                    results))
            ;; Count all solutions
            (push (list 'relaxed-count
                        (funcall 'neovm--csp-count-solutions '(a b c d) domains constraints))
                  results)))

        ;; Latin square 3x3: each row and column has {1,2,3} exactly once
        ;; Variables: c00 c01 c02 c10 c11 c12 c20 c21 c22
        (let ((domains (make-hash-table))
              (neq (lambda (x y) (/= x y))))
          (dolist (v '(c00 c01 c02 c10 c11 c12 c20 c21 c22))
            (puthash v '(1 2 3) domains))
          (let ((constraints nil))
            ;; Row constraints
            (dolist (row '((c00 c01 c02) (c10 c11 c12) (c20 c21 c22)))
              (dotimes (i 3)
                (let ((j (1+ i)))
                  (while (< j 3)
                    (push (list (nth i row) (nth j row) neq) constraints)
                    (setq j (1+ j))))))
            ;; Column constraints
            (dolist (col '((c00 c10 c20) (c01 c11 c21) (c02 c12 c22)))
              (dotimes (i 3)
                (let ((j (1+ i)))
                  (while (< j 3)
                    (push (list (nth i col) (nth j col) neq) constraints)
                    (setq j (1+ j))))))
            (let ((sol (funcall 'neovm--csp-solve
                                '(c00 c01 c02 c10 c11 c12 c20 c21 c22)
                                domains constraints)))
              (push (list 'latin-3x3
                          (not (null sol))
                          ;; Verify rows have distinct values
                          (when sol
                            (and (= 3 (length (delete-dups
                                               (list (cdr (assq 'c00 sol))
                                                     (cdr (assq 'c01 sol))
                                                     (cdr (assq 'c02 sol))))))
                                 (= 3 (length (delete-dups
                                               (list (cdr (assq 'c10 sol))
                                                     (cdr (assq 'c11 sol))
                                                     (cdr (assq 'c12 sol))))))
                                 (= 3 (length (delete-dups
                                               (list (cdr (assq 'c20 sol))
                                                     (cdr (assq 'c21 sol))
                                                     (cdr (assq 'c22 sol)))))))))
                    results))))

        (nreverse results))
    (fmakunbound 'neovm--csp-consistent-p)
    (fmakunbound 'neovm--csp-solve)
    (fmakunbound 'neovm--csp-bt)
    (fmakunbound 'neovm--csp-count-solutions)
    (fmakunbound 'neovm--csp-bt-count)))
"#;
    let expect = expect_test::expect![[
        r#""OK ((relaxed-2x2 t (1 4 4 1) t) (relaxed-count 4) (latin-3x3 t t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
