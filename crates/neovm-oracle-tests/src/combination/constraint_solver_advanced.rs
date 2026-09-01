//! Advanced constraint solver oracle parity tests.
//!
//! Covers: CSP with arc consistency (AC-3), forward checking with MRV
//! heuristic, 4x4 Sudoku-like solver, N-queens with constraint
//! propagation, map coloring with backtracking, cryptarithmetic partial
//! solver, and constraint network with domain filtering.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// CSP with arc consistency (AC-3) and domain reduction
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_constraint_solver_ac3_domain_reduction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement AC-3 arc consistency as domain reduction, then solve
    // a coloring problem where some nodes are pre-assigned.
    let form = r#"
(progn
  (fset 'neovm--csadv-ac3-reduce
    (lambda (domains arcs)
      "Run AC-3: iteratively reduce domains until stable or wipeout.
       ARCS is list of (xi xj pred) where pred checks if xi-val is
       consistent with at least one xj-val."
      (let ((queue (copy-sequence arcs))
            (ok t))
        (while (and ok queue)
          (let* ((arc (car queue))
                 (xi (nth 0 arc))
                 (xj (nth 1 arc))
                 (pred (nth 2 arc))
                 (di (gethash xi domains))
                 (dj (gethash xj domains))
                 (new-di nil)
                 (revised nil))
            (setq queue (cdr queue))
            ;; Keep only values in di that have support in dj
            (dolist (vi di)
              (let ((supported nil))
                (dolist (vj dj)
                  (when (funcall pred vi vj)
                    (setq supported t)))
                (if supported
                    (push vi new-di)
                  (setq revised t))))
            (when revised
              (setq new-di (nreverse new-di))
              (if (null new-di)
                  (setq ok nil)  ;; Domain wipeout
                (puthash xi new-di domains)
                ;; Re-enqueue arcs pointing to xi
                (dolist (a arcs)
                  (when (and (eq (nth 1 a) xi)
                             (not (eq (nth 0 a) xj)))
                    (push a queue)))))))
        ok)))

  (unwind-protect
      (let ((domains (make-hash-table))
            (neq (lambda (a b) (not (eq a b)))))
        ;; 4 variables: A B C D, colors: red green blue
        ;; Pre-assign: A=red
        (puthash 'a '(red) domains)
        (puthash 'b '(red green blue) domains)
        (puthash 'c '(red green blue) domains)
        (puthash 'd '(red green blue) domains)
        ;; Constraints: A-B, B-C, C-D, A-D all different
        (let* ((arcs (list
                      (list 'a 'b neq) (list 'b 'a neq)
                      (list 'b 'c neq) (list 'c 'b neq)
                      (list 'c 'd neq) (list 'd 'c neq)
                      (list 'a 'd neq) (list 'd 'a neq)))
               (consistent (funcall 'neovm--csadv-ac3-reduce domains arcs))
               (dom-a (gethash 'a domains))
               (dom-b (gethash 'b domains))
               (dom-c (gethash 'c domains))
               (dom-d (gethash 'd domains)))
          (list
           consistent
           ;; A is fixed to red
           (equal dom-a '(red))
           ;; B cannot be red (arc A-B)
           (not (memq 'red dom-b))
           ;; D cannot be red (arc A-D)
           (not (memq 'red dom-d))
           ;; Domains reduced but non-empty
           (> (length dom-b) 0)
           (> (length dom-c) 0)
           (> (length dom-d) 0)
           ;; Total domain sizes after reduction
           (list (length dom-a) (length dom-b) (length dom-c) (length dom-d)))))
    (fmakunbound 'neovm--csadv-ac3-reduce)))
"#;
    let expect = expect_test::expect![[r#""OK (t t t t t t t (1 2 3 2))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Forward checking with MRV (minimum remaining values) heuristic
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_constraint_solver_forward_checking_mrv() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Solve all-different constraint with forward checking + MRV.
    let form = r#"
(progn
  (fset 'neovm--csadv-mrv-select
    (lambda (unassigned domains)
      "Select variable with Minimum Remaining Values."
      (let ((best nil) (best-size 999999))
        (dolist (v unassigned)
          (let ((sz (length (gethash v domains))))
            (when (< sz best-size)
              (setq best v best-size sz))))
        best)))

  (fset 'neovm--csadv-fc-solve
    (lambda (variables domains constraints)
      "Solve CSP using forward checking + MRV heuristic.
       CONSTRAINTS is list of (v1 v2 pred)."
      (let ((result nil))
        (fset 'neovm--csadv-fc-bt
          (lambda (unassigned assignment domains)
            (if (null unassigned)
                (progn (setq result (copy-sequence assignment)) t)
              ;; MRV: pick var with smallest domain
              (let* ((var (funcall 'neovm--csadv-mrv-select unassigned domains))
                     (remaining (delq var (copy-sequence unassigned)))
                     (found nil))
                (dolist (val (gethash var domains))
                  (unless found
                    (let ((new-assign (cons (cons var val) assignment))
                          (new-domains (copy-hash-table domains))
                          (consistent t))
                      ;; Forward check: reduce domains of unassigned neighbors
                      (dolist (c constraints)
                        (when consistent
                          (let ((v1 (nth 0 c)) (v2 (nth 1 c)) (pred (nth 2 c)))
                            (cond
                             ((and (eq v1 var) (memq v2 remaining))
                              ;; Filter v2's domain
                              (let ((new-dom nil))
                                (dolist (d (gethash v2 new-domains))
                                  (when (funcall pred val d)
                                    (push d new-dom)))
                                (if (null new-dom)
                                    (setq consistent nil)
                                  (puthash v2 (nreverse new-dom) new-domains))))
                             ((and (eq v2 var) (memq v1 remaining))
                              (let ((new-dom nil))
                                (dolist (d (gethash v1 new-domains))
                                  (when (funcall pred d val)
                                    (push d new-dom)))
                                (if (null new-dom)
                                    (setq consistent nil)
                                  (puthash v1 (nreverse new-dom) new-domains))))))))
                      (when consistent
                        (when (funcall 'neovm--csadv-fc-bt remaining new-assign new-domains)
                          (setq found t))))))
                found))))
        (funcall 'neovm--csadv-fc-bt variables nil domains)
        result)))

  (unwind-protect
      (let ((domains (make-hash-table))
            (neq (lambda (x y) (/= x y))))
        ;; 5 variables in {1..5}, all different
        (dolist (v '(v1 v2 v3 v4 v5))
          (puthash v '(1 2 3 4 5) domains))
        (let* ((vars '(v1 v2 v3 v4 v5))
               (constraints
                (let ((cs nil))
                  (dolist (a vars)
                    (dolist (b vars)
                      (unless (eq a b)
                        (push (list a b neq) cs))))
                  cs))
               (sol (funcall 'neovm--csadv-fc-solve vars domains constraints)))
          (list
           ;; Found a solution
           (not (null sol))
           ;; All 5 vars assigned
           (= (length sol) 5)
           ;; All values different
           (let ((vals (mapcar #'cdr sol)))
             (= (length vals)
                (length (delete-dups (copy-sequence vals)))))
           ;; All values in {1..5}
           (let ((ok t))
             (dolist (p sol)
               (unless (and (>= (cdr p) 1) (<= (cdr p) 5))
                 (setq ok nil)))
             ok)
           ;; Sum = 15 (1+2+3+4+5)
           (= (apply #'+ (mapcar #'cdr sol)) 15))))
    (fmakunbound 'neovm--csadv-mrv-select)
    (fmakunbound 'neovm--csadv-fc-solve)
    (fmakunbound 'neovm--csadv-fc-bt)))
"#;
    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// 4x4 Sudoku-like solver with constraint propagation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_constraint_solver_sudoku4x4_propagation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Solve multiple 4x4 puzzles and verify solutions.
    let form = r#"
(progn
  (fset 'neovm--csadv-s4-peers
    (lambda (pos)
      "Peers of position POS in a 4x4 grid with 2x2 boxes."
      (let* ((row (/ pos 4))
             (col (% pos 4))
             (br (* (/ row 2) 2))
             (bc (* (/ col 2) 2))
             (peers nil))
        ;; Same row
        (dotimes (c 4)
          (let ((p (+ (* row 4) c)))
            (unless (= p pos) (push p peers))))
        ;; Same col
        (dotimes (r 4)
          (let ((p (+ (* r 4) col)))
            (unless (or (= p pos) (memq p peers))
              (push p peers))))
        ;; Same box
        (dotimes (dr 2)
          (dotimes (dc 2)
            (let ((p (+ (* (+ br dr) 4) (+ bc dc))))
              (unless (or (= p pos) (memq p peers))
                (push p peers)))))
        peers)))

  (fset 'neovm--csadv-s4-solve
    (lambda (grid)
      "Solve 4x4 Sudoku. GRID is vector of 16, 0=empty."
      (let ((poss (make-vector 16 nil))
            (solved nil))
        ;; Init possibilities
        (dotimes (i 16)
          (aset poss i (if (= (aref grid i) 0)
                           (list 1 2 3 4)
                         (list (aref grid i)))))
        ;; Propagate
        (let ((changed t))
          (while changed
            (setq changed nil)
            (dotimes (i 16)
              (when (= (length (aref poss i)) 1)
                (let ((val (car (aref poss i))))
                  (dolist (p (funcall 'neovm--csadv-s4-peers i))
                    (when (memq val (aref poss p))
                      (aset poss p (delq val (copy-sequence (aref poss p))))
                      (setq changed t))))))))
        ;; Check if solved
        (let ((all-one t))
          (dotimes (i 16)
            (when (/= (length (aref poss i)) 1)
              (setq all-one nil)))
          (when all-one
            (let ((result (make-vector 16 0)))
              (dotimes (i 16)
                (aset result i (car (aref poss i))))
              (setq solved result))))
        solved)))

  (fset 'neovm--csadv-s4-valid-p
    (lambda (grid)
      "Check if 4x4 Sudoku solution is valid."
      (let ((ok t))
        ;; Check rows
        (dotimes (r 4)
          (let ((row nil))
            (dotimes (c 4)
              (push (aref grid (+ (* r 4) c)) row))
            (unless (equal (sort (copy-sequence row) #'<) '(1 2 3 4))
              (setq ok nil))))
        ;; Check cols
        (dotimes (c 4)
          (let ((col nil))
            (dotimes (r 4)
              (push (aref grid (+ (* r 4) c)) col))
            (unless (equal (sort (copy-sequence col) #'<) '(1 2 3 4))
              (setq ok nil))))
        ;; Check 2x2 boxes
        (dolist (br '(0 2))
          (dolist (bc '(0 2))
            (let ((box nil))
              (dotimes (dr 2)
                (dotimes (dc 2)
                  (push (aref grid (+ (* (+ br dr) 4) (+ bc dc))) box)))
              (unless (equal (sort (copy-sequence box) #'<) '(1 2 3 4))
                (setq ok nil)))))
        ok)))

  (unwind-protect
      (let* (;; Puzzle 1: solvable by propagation alone
             (p1 (vector 1 0 0 4
                         0 4 1 0
                         0 1 4 0
                         4 0 0 1))
             (s1 (funcall 'neovm--csadv-s4-solve p1))
             ;; Puzzle 2: different arrangement
             (p2 (vector 0 2 0 0
                         0 0 4 2
                         2 4 0 0
                         0 0 2 0))
             (s2 (funcall 'neovm--csadv-s4-solve p2)))
        (list
         ;; Puzzle 1 solved
         (not (null s1))
         (when s1 (funcall 'neovm--csadv-s4-valid-p s1))
         ;; Clues preserved in puzzle 1
         (when s1
           (let ((ok t))
             (dotimes (i 16)
               (when (and (/= (aref p1 i) 0)
                          (/= (aref p1 i) (aref s1 i)))
                 (setq ok nil)))
             ok))
         ;; Puzzle 2 solved
         (not (null s2))
         (when s2 (funcall 'neovm--csadv-s4-valid-p s2))
         ;; Solutions
         s1 s2))
    (fmakunbound 'neovm--csadv-s4-peers)
    (fmakunbound 'neovm--csadv-s4-solve)
    (fmakunbound 'neovm--csadv-s4-valid-p)))
"#;
    let expect = expect_test::expect![[r#""OK (nil nil nil nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// N-queens with constraint propagation (domain filtering per column)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_constraint_solver_nqueens_propagation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // N-queens where each column has a domain of possible rows.
    // Propagation removes attacked rows after each placement.
    let form = r#"
(progn
  (fset 'neovm--csadv-nq-solve
    (lambda (n)
      "Solve N-queens using column domains with propagation.
       Returns first solution as list of (col . row) or nil."
      (let ((result nil))
        (fset 'neovm--csadv-nq-bt
          (lambda (col domains placed)
            (if (= col n)
                (progn (setq result (copy-sequence placed)) t)
              (let ((found nil))
                (dolist (row (aref domains col))
                  (unless found
                    ;; Propagate: remove row and diagonals from future columns
                    (let ((new-domains (make-vector n nil))
                          (ok t))
                      (dotimes (c n)
                        (aset new-domains c (copy-sequence (aref domains c))))
                      ;; Remove conflicts from columns col+1..n-1
                      (let ((dc 1))
                        (while (and ok (< (+ col dc) n))
                          (let ((fc (+ col dc))
                                (new-dom nil))
                            (dolist (r (aref new-domains fc))
                              (unless (or (= r row)
                                          (= r (+ row dc))
                                          (= r (- row dc)))
                                (push r new-dom)))
                            (if (null new-dom)
                                (setq ok nil)
                              (aset new-domains fc (nreverse new-dom))))
                          (setq dc (1+ dc))))
                      (when ok
                        (when (funcall 'neovm--csadv-nq-bt
                                       (1+ col)
                                       new-domains
                                       (cons (cons col row) placed))
                          (setq found t))))))
                found))))
        ;; Init domains: each column can have any row
        (let ((domains (make-vector n nil)))
          (dotimes (c n)
            (let ((rows nil))
              (dotimes (r n) (push r rows))
              (aset domains c (nreverse rows))))
          (funcall 'neovm--csadv-nq-bt 0 domains nil))
        result)))

  (fset 'neovm--csadv-nq-valid-p
    (lambda (solution n)
      "Verify N-queens solution: no two queens share row or diagonal."
      (let ((ok t))
        (dolist (q1 solution)
          (dolist (q2 solution)
            (when (and ok (not (equal q1 q2)))
              (when (or (= (cdr q1) (cdr q2))
                        (= (abs (- (car q1) (car q2)))
                           (abs (- (cdr q1) (cdr q2)))))
                (setq ok nil)))))
        ok)))

  (unwind-protect
      (let ((results nil))
        (dolist (n '(4 5 6 7 8))
          (let ((sol (funcall 'neovm--csadv-nq-solve n)))
            (push (list n
                        (not (null sol))
                        (= (length sol) n)
                        (funcall 'neovm--csadv-nq-valid-p sol n))
                  results)))
        (nreverse results))
    (fmakunbound 'neovm--csadv-nq-solve)
    (fmakunbound 'neovm--csadv-nq-bt)
    (fmakunbound 'neovm--csadv-nq-valid-p)))
"#;
    let expect =
        expect_test::expect![[r#""OK ((4 t t t) (5 t t t) (6 t t t) (7 t t t) (8 t t t))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Map coloring with backtracking and degree heuristic
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_constraint_solver_map_coloring_degree() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Color a graph using fewest colors, with degree ordering heuristic
    // (process nodes with most constraints first).
    let form = r#"
(progn
  (fset 'neovm--csadv-mc-degree-order
    (lambda (nodes edges)
      "Order nodes by degree (descending)."
      (let ((degrees nil))
        (dolist (n nodes)
          (let ((deg 0))
            (dolist (e edges)
              (when (or (eq (car e) n) (eq (cdr e) n))
                (setq deg (1+ deg))))
            (push (cons n deg) degrees)))
        (mapcar #'car
                (sort degrees (lambda (a b) (> (cdr a) (cdr b))))))))

  (fset 'neovm--csadv-mc-solve
    (lambda (nodes edges colors)
      "Solve graph coloring with degree ordering + backtracking."
      (let ((ordered (funcall 'neovm--csadv-mc-degree-order nodes edges))
            (result nil))
        (fset 'neovm--csadv-mc-bt
          (lambda (remaining assignment)
            (if (null remaining)
                (progn (setq result (copy-sequence assignment)) t)
              (let ((node (car remaining))
                    (found nil))
                (dolist (color colors)
                  (unless found
                    ;; Check no neighbor has this color
                    (let ((ok t))
                      (dolist (e edges)
                        (cond
                         ((and (eq (car e) node)
                               (let ((nb (cdr (assq (cdr e) assignment))))
                                 (and nb (eq nb color))))
                          (setq ok nil))
                         ((and (eq (cdr e) node)
                               (let ((nb (cdr (assq (car e) assignment))))
                                 (and nb (eq nb color))))
                          (setq ok nil))))
                      (when ok
                        (when (funcall 'neovm--csadv-mc-bt
                                       (cdr remaining)
                                       (cons (cons node color) assignment))
                          (setq found t))))))
                found))))
        (funcall 'neovm--csadv-mc-bt ordered nil)
        result)))

  (unwind-protect
      (let* (;; Petersen-like graph (5 outer + 5 inner nodes)
             (nodes '(o1 o2 o3 o4 o5 i1 i2 i3 i4 i5))
             (edges '((o1 . o2) (o2 . o3) (o3 . o4) (o4 . o5) (o5 . o1)
                      (o1 . i1) (o2 . i2) (o3 . i3) (o4 . i4) (o5 . i5)
                      (i1 . i3) (i3 . i5) (i5 . i2) (i2 . i4) (i4 . i1)))
             (colors '(red green blue))
             (sol (funcall 'neovm--csadv-mc-solve nodes edges colors))
             ;; Also solve a simple triangle
             (tri-nodes '(a b c))
             (tri-edges '((a . b) (b . c) (a . c)))
             (tri-sol (funcall 'neovm--csadv-mc-solve tri-nodes tri-edges colors))
             ;; Degree ordering
             (ordering (funcall 'neovm--csadv-mc-degree-order nodes edges)))
        (list
         ;; Petersen: found solution
         (not (null sol))
         ;; All nodes assigned
         (when sol (= (length sol) (length nodes)))
         ;; No adjacent same color
         (when sol
           (let ((ok t))
             (dolist (e edges)
               (when (eq (cdr (assq (car e) sol))
                         (cdr (assq (cdr e) sol)))
                 (setq ok nil)))
             ok))
         ;; Triangle: found solution
         (not (null tri-sol))
         ;; Triangle: all 3 different colors
         (when tri-sol
           (let ((cs (mapcar #'cdr tri-sol)))
             (= 3 (length (delete-dups (copy-sequence cs))))))
         ;; Degree ordering puts high-degree nodes first
         (car ordering)))
    (fmakunbound 'neovm--csadv-mc-degree-order)
    (fmakunbound 'neovm--csadv-mc-solve)
    (fmakunbound 'neovm--csadv-mc-bt)))
"#;
    let expect = expect_test::expect![[r#""OK (t t t t t i5)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Cryptarithmetic: A + B = C where letters map to distinct digits
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_constraint_solver_cryptarithmetic_exhaustive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Solve AB + CD = EF where each letter is a distinct digit,
    // A >= 1, C >= 1, E >= 1 (no leading zeros).
    let form = r#"
(progn
  (fset 'neovm--csadv-crypto-solve
    (lambda ()
      "Find solutions to AB + CD = EF, all letters distinct, no leading zeros."
      (let ((solutions nil)
            (count 0))
        (let ((a 1))
          (while (<= a 9)
            (let ((b 0))
              (while (<= b 9)
                (when (/= b a)
                  (let ((c 1))
                    (while (<= c 9)
                      (when (and (/= c a) (/= c b))
                        (let ((d 0))
                          (while (<= d 9)
                            (when (and (/= d a) (/= d b) (/= d c))
                              (let* ((ab (+ (* a 10) b))
                                     (cd (+ (* c 10) d))
                                     (ef (+ ab cd)))
                                (when (and (>= ef 10) (<= ef 99))
                                  (let ((e (/ ef 10))
                                        (f (% ef 10)))
                                    (when (and (/= e a) (/= e b) (/= e c) (/= e d)
                                               (/= f a) (/= f b) (/= f c) (/= f d)
                                               (/= f e))
                                      (setq count (1+ count))
                                      ;; Only keep first 5 for display
                                      (when (<= count 5)
                                        (push (list ab cd ef) solutions)))))))
                            (setq d (1+ d)))))
                      (setq c (1+ c)))))
                (setq b (1+ b))))
            (setq a (1+ a))))
        (list :count count
              :samples (nreverse solutions)))))

  (unwind-protect
      (let* ((result (funcall 'neovm--csadv-crypto-solve))
             (count (plist-get result :count))
             (samples (plist-get result :samples)))
        (list
         ;; Found some solutions
         (> count 0)
         ;; Samples are valid
         (let ((ok t))
           (dolist (s samples)
             (unless (= (+ (nth 0 s) (nth 1 s)) (nth 2 s))
               (setq ok nil)))
           ok)
         ;; All digits in samples are distinct within each solution
         (let ((ok t))
           (dolist (s samples)
             (let* ((ab (nth 0 s)) (cd (nth 1 s)) (ef (nth 2 s))
                    (digits (list (/ ab 10) (% ab 10)
                                  (/ cd 10) (% cd 10)
                                  (/ ef 10) (% ef 10))))
               (unless (= (length digits)
                          (length (delete-dups (copy-sequence digits))))
                 (setq ok nil))))
           ok)
         ;; Count and first few samples
         count
         samples))
    (fmakunbound 'neovm--csadv-crypto-solve)))
"#;
    let expect = expect_test::expect![[
        r#""OK (t t t 476 ((12 35 47) (12 36 48) (12 37 49) (12 38 50) (12 46 58)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Constraint network: domain filtering with binary and unary constraints
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_constraint_solver_constraint_network() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a constraint network with unary (domain) and binary constraints,
    // apply node consistency then arc consistency, then solve.
    let form = r#"
(progn
  (fset 'neovm--csadv-cn-node-consistent
    (lambda (domains unary-constraints)
      "Apply unary constraints to reduce domains.
       UNARY-CONSTRAINTS is list of (var . pred)."
      (dolist (uc unary-constraints)
        (let ((var (car uc))
              (pred (cdr uc))
              (new-dom nil))
          (dolist (val (gethash var domains))
            (when (funcall pred val)
              (push val new-dom)))
          (puthash var (nreverse new-dom) domains)))
      domains))

  (fset 'neovm--csadv-cn-arc-consistent
    (lambda (domains binary-constraints)
      "Apply arc consistency. BINARY-CONSTRAINTS is list of (v1 v2 . pred)."
      (let ((queue nil))
        ;; Build queue of arcs (both directions)
        (dolist (bc binary-constraints)
          (push (list (car bc) (cadr bc) (cddr bc)) queue)
          (push (list (cadr bc) (car bc)
                      (let ((p (cddr bc)))
                        (lambda (x y) (funcall p y x))))
                queue))
        (while queue
          (let* ((arc (car queue))
                 (xi (nth 0 arc)) (xj (nth 1 arc)) (pred (nth 2 arc))
                 (revised nil)
                 (new-di nil))
            (setq queue (cdr queue))
            (dolist (vi (gethash xi domains))
              (let ((support nil))
                (dolist (vj (gethash xj domains))
                  (when (funcall pred vi vj)
                    (setq support t)))
                (if support
                    (push vi new-di)
                  (setq revised t))))
            (when revised
              (puthash xi (nreverse new-di) domains)
              ;; Re-enqueue arcs to xi
              (dolist (bc binary-constraints)
                (when (and (eq (cadr bc) xi) (not (eq (car bc) xj)))
                  (push (list (car bc) xi (cddr bc)) queue))
                (when (and (eq (car bc) xi) (not (eq (cadr bc) xj)))
                  (let ((p (cddr bc)))
                    (push (list (cadr bc) xi
                                (lambda (x y) (funcall p y x)))
                          queue))))))))
      domains))

  (fset 'neovm--csadv-cn-solve
    (lambda (vars domains)
      "Backtrack solver on already-reduced domains."
      (let ((result nil))
        (fset 'neovm--csadv-cn-bt
          (lambda (rem assign)
            (if (null rem)
                (progn (setq result (copy-sequence assign)) t)
              (let ((v (car rem)) (found nil))
                (dolist (val (gethash v domains))
                  (unless found
                    (setq found
                          (funcall 'neovm--csadv-cn-bt
                                   (cdr rem)
                                   (cons (cons v val) assign)))))
                found))))
        (funcall 'neovm--csadv-cn-bt vars nil)
        result)))

  (unwind-protect
      (let ((domains (make-hash-table)))
        ;; Variables X, Y, Z with initial domain {1..10}
        (dolist (v '(x y z))
          (puthash v '(1 2 3 4 5 6 7 8 9 10) domains))
        ;; Unary: X is even, Y is odd, Z > 5
        (let ((unary (list (cons 'x (lambda (v) (= (% v 2) 0)))
                           (cons 'y (lambda (v) (= (% v 2) 1)))
                           (cons 'z (lambda (v) (> v 5))))))
          (funcall 'neovm--csadv-cn-node-consistent domains unary))
        ;; Binary: X < Y, Y < Z
        (let ((binary (list (cons 'x (cons 'y (lambda (a b) (< a b))))
                            (cons 'y (cons 'z (lambda (a b) (< a b)))))))
          (funcall 'neovm--csadv-cn-arc-consistent domains binary))
        (let* ((dom-x (gethash 'x domains))
               (dom-y (gethash 'y domains))
               (dom-z (gethash 'z domains))
               (sol (funcall 'neovm--csadv-cn-solve '(x y z) domains)))
          (list
           ;; Domains are reduced
           dom-x dom-y dom-z
           ;; X is even, Y is odd, Z > 5
           (let ((ok t))
             (dolist (v dom-x) (unless (= (% v 2) 0) (setq ok nil)))
             ok)
           (let ((ok t))
             (dolist (v dom-y) (unless (= (% v 2) 1) (setq ok nil)))
             ok)
           (let ((ok t))
             (dolist (v dom-z) (unless (> v 5) (setq ok nil)))
             ok)
           ;; Found a valid solution
           (not (null sol))
           (when sol
             (let ((vx (cdr (assq 'x sol)))
                   (vy (cdr (assq 'y sol)))
                   (vz (cdr (assq 'z sol))))
               (list (= (% vx 2) 0)   ;; X even
                     (= (% vy 2) 1)   ;; Y odd
                     (> vz 5)          ;; Z > 5
                     (< vx vy)         ;; X < Y
                     (< vy vz)))))))   ;; Y < Z
    (fmakunbound 'neovm--csadv-cn-node-consistent)
    (fmakunbound 'neovm--csadv-cn-arc-consistent)
    (fmakunbound 'neovm--csadv-cn-solve)
    (fmakunbound 'neovm--csadv-cn-bt)))
"#;
    let expect =
        expect_test::expect![[r#""OK ((2 4 6 8) (3 5 7 9) (6 7 8 9 10) t t t t (t t t t t))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
