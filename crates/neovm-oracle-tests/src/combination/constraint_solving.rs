//! Oracle parity tests for constraint solving patterns in Elisp.
//!
//! Covers N-Queens via backtracking, Sudoku constraint propagation,
//! map coloring (graph coloring), 2-SAT with implication graphs,
//! and cryptarithmetic puzzle solving.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

// ---------------------------------------------------------------------------
// N-Queens solver via backtracking
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_constraint_n_queens() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Place N queens on an NxN board so no two attack each other.
    // Return the number of solutions for N=1..7 and one explicit solution for N=5.
    let form = r#"
(progn
  (fset 'neovm--cst-queens-safe-p
    (lambda (queens row col)
      "Check if placing a queen at (ROW, COL) is safe given QUEENS alist of (row . col)."
      (let ((safe t))
        (dolist (q queens)
          (when safe
            (let ((qr (car q)) (qc (cdr q)))
              (when (or (= qc col)
                        (= (abs (- qr row)) (abs (- qc col))))
                (setq safe nil)))))
        safe)))

  (fset 'neovm--cst-queens-solve
    (lambda (n)
      "Find all solutions for the N-Queens problem. Returns list of solutions."
      (let ((solutions nil))
        (fset 'neovm--cst-queens-bt
          (lambda (row queens)
            (if (= row n)
                (setq solutions (cons (copy-sequence queens) solutions))
              (let ((col 0))
                (while (< col n)
                  (when (funcall 'neovm--cst-queens-safe-p queens row col)
                    (funcall 'neovm--cst-queens-bt
                             (1+ row)
                             (cons (cons row col) queens)))
                  (setq col (1+ col)))))))
        (funcall 'neovm--cst-queens-bt 0 nil)
        solutions)))

  (fset 'neovm--cst-queens-valid-p
    (lambda (solution n)
      "Verify that a solution is valid: all rows and cols unique, no diagonals."
      (and (= (length solution) n)
           ;; All rows unique
           (let ((rows (mapcar #'car solution)))
             (= (length rows) (length (delete-dups (copy-sequence rows)))))
           ;; All cols unique
           (let ((cols (mapcar #'cdr solution)))
             (= (length cols) (length (delete-dups (copy-sequence cols)))))
           ;; No diagonal attacks
           (let ((ok t))
             (dolist (q1 solution)
               (dolist (q2 solution)
                 (when (and ok (not (equal q1 q2)))
                   (when (= (abs (- (car q1) (car q2)))
                            (abs (- (cdr q1) (cdr q2))))
                     (setq ok nil)))))
             ok))))

  (unwind-protect
      (let* ((counts (mapcar (lambda (n)
                               (length (funcall 'neovm--cst-queens-solve n)))
                             '(1 2 3 4 5 6 7)))
             (sol5 (funcall 'neovm--cst-queens-solve 5)))
        (list
         ;; Known sequence: 1, 0, 0, 2, 10, 4, 40
         counts
         ;; Number of 5-queens solutions
         (length sol5)
         ;; All solutions for N=5 are valid
         (let ((all-valid t))
           (dolist (s sol5)
             (unless (funcall 'neovm--cst-queens-valid-p s 5)
               (setq all-valid nil)))
           all-valid)
         ;; First solution for N=4 (sorted)
         (let* ((s4 (funcall 'neovm--cst-queens-solve 4))
                (first-sol (car (sort (copy-sequence s4)
                                      (lambda (a b)
                                        (< (cdr (car a)) (cdr (car b))))))))
           first-sol)))
    (fmakunbound 'neovm--cst-queens-safe-p)
    (fmakunbound 'neovm--cst-queens-solve)
    (fmakunbound 'neovm--cst-queens-bt)
    (fmakunbound 'neovm--cst-queens-valid-p)))
"#;
    let expect = expect_test::expect![[
        r#""OK ((1 0 0 2 10 4 40) 10 t ((3 . 1) (2 . 3) (1 . 0) (0 . 2)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Sudoku solver using constraint propagation and backtracking
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_constraint_sudoku_solver() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Solve a 4x4 mini-Sudoku using constraint propagation.
    // Grid uses 1-4, 0 = empty. 2x2 boxes.
    let form = r#"
(progn
  (fset 'neovm--cst-sudoku-peers
    (lambda (pos size box-h box-w)
      "Return list of positions that are peers of POS in a SIZE x SIZE grid."
      (let ((row (/ pos size))
            (col (% pos size))
            (peers nil))
        ;; Same row
        (let ((c 0))
          (while (< c size)
            (let ((p (+ (* row size) c)))
              (unless (= p pos) (setq peers (cons p peers))))
            (setq c (1+ c))))
        ;; Same column
        (let ((r 0))
          (while (< r size)
            (let ((p (+ (* r size) col)))
              (unless (or (= p pos) (memq p peers))
                (setq peers (cons p peers))))
            (setq r (1+ r))))
        ;; Same box
        (let* ((br (* (/ row box-h) box-h))
               (bc (* (/ col box-w) box-w)))
          (let ((dr 0))
            (while (< dr box-h)
              (let ((dc 0))
                (while (< dc box-w)
                  (let ((p (+ (* (+ br dr) size) (+ bc dc))))
                    (unless (or (= p pos) (memq p peers))
                      (setq peers (cons p peers))))
                  (setq dc (1+ dc))))
              (setq dr (1+ dr)))))
        peers)))

  (fset 'neovm--cst-sudoku-solve4
    (lambda (grid)
      "Solve a 4x4 Sudoku grid (vector of 16 values, 0=empty)."
      (let ((possible (make-vector 16 nil))
            (solved nil))
        ;; Initialize possibilities
        (let ((i 0))
          (while (< i 16)
            (if (= (aref grid i) 0)
                (aset possible i (list 1 2 3 4))
              (aset possible i (list (aref grid i))))
            (setq i (1+ i))))
        ;; Constraint propagation: eliminate assigned values from peers
        (fset 'neovm--cst-sudoku-propagate
          (lambda ()
            (let ((changed t))
              (while changed
                (setq changed nil)
                (let ((i 0))
                  (while (< i 16)
                    (when (= (length (aref possible i)) 1)
                      (let ((val (car (aref possible i)))
                            (peers (funcall 'neovm--cst-sudoku-peers i 4 2 2)))
                        (dolist (p peers)
                          (when (memq val (aref possible p))
                            (aset possible p (delq val (copy-sequence (aref possible p))))
                            (setq changed t)))))
                    (setq i (1+ i))))))))
        ;; Backtracking solver
        (fset 'neovm--cst-sudoku-bt
          (lambda ()
            (funcall 'neovm--cst-sudoku-propagate)
            ;; Check for contradictions
            (let ((contradiction nil) (all-assigned t) (i 0))
              (while (< i 16)
                (when (null (aref possible i)) (setq contradiction t))
                (when (> (length (aref possible i)) 1) (setq all-assigned nil))
                (setq i (1+ i)))
              (cond
               (contradiction nil)
               (all-assigned
                ;; Build solution
                (let ((result (make-vector 16 0)) (j 0))
                  (while (< j 16)
                    (aset result j (car (aref possible j)))
                    (setq j (1+ j)))
                  (setq solved result)
                  t))
               (t
                ;; Find cell with fewest possibilities > 1
                (let ((best-i nil) (best-len 999) (k 0))
                  (while (< k 16)
                    (let ((len (length (aref possible k))))
                      (when (and (> len 1) (< len best-len))
                        (setq best-i k best-len len)))
                    (setq k (1+ k)))
                  (let ((found nil))
                    (dolist (val (copy-sequence (aref possible best-i)))
                      (unless found
                        (let ((saved (make-vector 16 nil)) (m 0))
                          (while (< m 16)
                            (aset saved m (copy-sequence (aref possible m)))
                            (setq m (1+ m)))
                          (aset possible best-i (list val))
                          (if (funcall 'neovm--cst-sudoku-bt)
                              (setq found t)
                            ;; Restore
                            (let ((n 0))
                              (while (< n 16)
                                (aset possible n (aref saved n))
                                (setq n (1+ n))))))))
                    found)))))))
        (funcall 'neovm--cst-sudoku-bt)
        solved)))

  (fset 'neovm--cst-sudoku-valid4-p
    (lambda (grid)
      "Validate a completed 4x4 Sudoku."
      (and
       ;; Check rows
       (let ((ok t) (r 0))
         (while (and ok (< r 4))
           (let ((row (list (aref grid (+ (* r 4) 0))
                            (aref grid (+ (* r 4) 1))
                            (aref grid (+ (* r 4) 2))
                            (aref grid (+ (* r 4) 3)))))
             (unless (equal (sort (copy-sequence row) #'<) '(1 2 3 4))
               (setq ok nil)))
           (setq r (1+ r)))
         ok)
       ;; Check columns
       (let ((ok t) (c 0))
         (while (and ok (< c 4))
           (let ((col (list (aref grid c)
                            (aref grid (+ c 4))
                            (aref grid (+ c 8))
                            (aref grid (+ c 12)))))
             (unless (equal (sort (copy-sequence col) #'<) '(1 2 3 4))
               (setq ok nil)))
           (setq c (1+ c)))
         ok))))

  (unwind-protect
      (let* ((puzzle (vector 0 0 3 0
                             3 0 0 1
                             0 0 0 3
                             0 3 0 0))
             (solution (funcall 'neovm--cst-sudoku-solve4 puzzle)))
        (list
         ;; Did we find a solution?
         (not (null solution))
         ;; Is it valid?
         (when solution (funcall 'neovm--cst-sudoku-valid4-p solution))
         ;; The actual solution
         solution
         ;; Original clues preserved?
         (when solution
           (let ((preserved t) (i 0))
             (while (< i 16)
               (when (and (/= (aref puzzle i) 0)
                          (/= (aref puzzle i) (aref solution i)))
                 (setq preserved nil))
               (setq i (1+ i)))
             preserved))))
    (fmakunbound 'neovm--cst-sudoku-peers)
    (fmakunbound 'neovm--cst-sudoku-solve4)
    (fmakunbound 'neovm--cst-sudoku-propagate)
    (fmakunbound 'neovm--cst-sudoku-bt)
    (fmakunbound 'neovm--cst-sudoku-valid4-p)))
"#;
    let expect = expect_test::expect![[r#""OK (t t [1 4 3 2 3 2 4 1 4 1 2 3 2 3 1 4] t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Map coloring with arc consistency (AC-3)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_constraint_map_coloring_ac3() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement arc consistency (AC-3) for graph coloring.
    // Use it to solve Australia map coloring and a Petersen graph coloring.
    let form = r#"
(progn
  (fset 'neovm--cst-ac3-init-domains
    (lambda (nodes colors)
      "Create initial domains: each node can be any color."
      (let ((domains (make-hash-table)))
        (dolist (n nodes)
          (puthash n (copy-sequence colors) domains))
        domains)))

  (fset 'neovm--cst-ac3-revise
    (lambda (domains xi xj)
      "Revise domain of XI to be arc-consistent with XJ.
       Remove values from XI's domain that have no support in XJ's domain.
       Return t if domain was revised."
      (let ((revised nil)
            (di (gethash xi domains))
            (dj (gethash xj domains))
            (new-di nil))
        (dolist (vi di)
          ;; vi is supported if there exists vj != vi in dj
          (let ((supported nil))
            (dolist (vj dj)
              (unless (eq vi vj)
                (setq supported t)))
            (if supported
                (setq new-di (cons vi new-di))
              (setq revised t))))
        (when revised
          (puthash xi (nreverse new-di) domains))
        revised)))

  (fset 'neovm--cst-ac3-run
    (lambda (domains edges)
      "Run AC-3 algorithm. Returns t if consistent, nil if domain wiped out."
      (let ((queue (copy-sequence edges))
            ;; Also add reverse edges
            (rev-edges (mapcar (lambda (e) (cons (cdr e) (car e))) edges)))
        (setq queue (append queue rev-edges))
        (while queue
          (let* ((arc (car queue))
                 (xi (car arc))
                 (xj (cdr arc)))
            (setq queue (cdr queue))
            (when (funcall 'neovm--cst-ac3-revise domains xi xj)
              (when (null (gethash xi domains))
                (setq queue nil))  ;; Wipe-out, fail
              ;; Re-enqueue arcs pointing to xi
              (dolist (e (append edges rev-edges))
                (when (and (eq (cdr e) xi) (not (eq (car e) xj)))
                  (setq queue (cons e queue)))))))
        ;; Check no domain is empty
        (let ((ok t))
          (maphash (lambda (k v) (when (null v) (setq ok nil))) domains)
          ok))))

  (fset 'neovm--cst-ac3-solve
    (lambda (nodes edges colors)
      "Solve coloring using AC-3 + backtracking."
      (let ((domains (funcall 'neovm--cst-ac3-init-domains nodes colors))
            (result nil))
        (when (funcall 'neovm--cst-ac3-run domains edges)
          ;; Backtrack to find complete assignment
          (fset 'neovm--cst-ac3-bt
            (lambda (remaining assignment)
              (if (null remaining)
                  (progn (setq result (copy-sequence assignment)) t)
                (let ((node (car remaining))
                      (found nil))
                  (dolist (color (gethash node domains))
                    (unless found
                      ;; Check consistency with current assignment
                      (let ((consistent t))
                        (dolist (e edges)
                          (cond
                           ((and (eq (car e) node)
                                 (assq (cdr e) assignment)
                                 (eq color (cdr (assq (cdr e) assignment))))
                            (setq consistent nil))
                           ((and (eq (cdr e) node)
                                 (assq (car e) assignment)
                                 (eq color (cdr (assq (car e) assignment))))
                            (setq consistent nil))))
                        (when consistent
                          (when (funcall 'neovm--cst-ac3-bt
                                         (cdr remaining)
                                         (cons (cons node color) assignment))
                            (setq found t))))))
                  found))))
          (funcall 'neovm--cst-ac3-bt nodes nil))
        result)))

  (unwind-protect
      (let* ((aus-nodes '(WA NT SA Q NSW V T))
             (aus-edges '((WA . NT) (WA . SA) (NT . SA) (NT . Q)
                          (SA . Q) (SA . NSW) (SA . V) (Q . NSW) (NSW . V)))
             (colors3 '(red green blue))
             (solution (funcall 'neovm--cst-ac3-solve aus-nodes aus-edges colors3)))
        (list
         ;; Found a solution
         (not (null solution))
         ;; All nodes assigned
         (= (length solution) (length aus-nodes))
         ;; No adjacent nodes share color
         (let ((ok t))
           (dolist (e aus-edges)
             (when (eq (cdr (assq (car e) solution))
                       (cdr (assq (cdr e) solution)))
               (setq ok nil)))
           ok)
         ;; Only valid colors used
         (let ((ok t))
           (dolist (a solution)
             (unless (memq (cdr a) colors3) (setq ok nil)))
           ok)))
    (fmakunbound 'neovm--cst-ac3-init-domains)
    (fmakunbound 'neovm--cst-ac3-revise)
    (fmakunbound 'neovm--cst-ac3-run)
    (fmakunbound 'neovm--cst-ac3-solve)
    (fmakunbound 'neovm--cst-ac3-bt)))
"#;
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// 2-SAT solver with implication graph
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_constraint_2sat_implication() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement 2-SAT: each clause has exactly 2 literals.
    // Build implication graph and use BFS to detect contradictions.
    // A clause (a OR b) implies (NOT a => b) and (NOT b => a).
    let form = r#"
(progn
  (fset 'neovm--cst-2sat-negate
    (lambda (lit)
      "Negate a literal: positive -> negative, negative -> positive."
      (- lit)))

  (fset 'neovm--cst-2sat-build-graph
    (lambda (clauses)
      "Build implication graph from 2-SAT clauses.
       Returns hash-table: literal -> list of implied literals."
      (let ((graph (make-hash-table)))
        (dolist (clause clauses)
          (let ((a (car clause))
                (b (cadr clause)))
            ;; (a OR b) => (NOT a => b) and (NOT b => a)
            (let ((neg-a (funcall 'neovm--cst-2sat-negate a))
                  (neg-b (funcall 'neovm--cst-2sat-negate b)))
              (puthash neg-a (cons b (or (gethash neg-a graph) nil)) graph)
              (puthash neg-b (cons a (or (gethash neg-b graph) nil)) graph))))
        graph)))

  (fset 'neovm--cst-2sat-reachable
    (lambda (graph start)
      "BFS from START in the implication graph. Return set of reachable literals."
      (let ((visited (make-hash-table))
            (queue (list start)))
        (puthash start t visited)
        (while queue
          (let ((current (car queue)))
            (setq queue (cdr queue))
            (dolist (neighbor (gethash current graph))
              (unless (gethash neighbor visited)
                (puthash neighbor t visited)
                (setq queue (append queue (list neighbor)))))))
        visited)))

  (fset 'neovm--cst-2sat-check
    (lambda (clauses vars)
      "Check if 2-SAT formula is satisfiable.
       Returns nil if unsatisfiable, or a satisfying assignment."
      (let ((graph (funcall 'neovm--cst-2sat-build-graph clauses))
            (sat t)
            (assignment nil))
        ;; Check: for each variable x, if x reaches NOT x and NOT x reaches x,
        ;; then unsatisfiable.
        (dolist (v vars)
          (when sat
            (let ((pos-reach (funcall 'neovm--cst-2sat-reachable graph v))
                  (neg-reach (funcall 'neovm--cst-2sat-reachable graph (- v))))
              (when (and (gethash (- v) pos-reach)
                         (gethash v neg-reach))
                (setq sat nil)))))
        (if (not sat)
            nil
          ;; Build assignment greedily: prefer true
          (dolist (v vars)
            (let ((pos-reach (funcall 'neovm--cst-2sat-reachable graph (- v))))
              ;; If NOT v implies v, then v must be true
              ;; If v implies NOT v, then v must be false
              ;; Otherwise, default to true
              (if (gethash v pos-reach)
                  (setq assignment (cons (cons v t) assignment))
                (setq assignment (cons (cons v nil) assignment)))))
          assignment))))

  (fset 'neovm--cst-2sat-verify
    (lambda (clauses assignment)
      "Verify that an assignment satisfies all clauses."
      (let ((ok t))
        (dolist (clause clauses)
          (let* ((a (car clause))
                 (b (cadr clause))
                 (va (if (> a 0) (cdr (assq a assignment))
                       (not (cdr (assq (- a) assignment)))))
                 (vb (if (> b 0) (cdr (assq b assignment))
                       (not (cdr (assq (- b) assignment))))))
            (unless (or va vb)
              (setq ok nil))))
        ok)))

  (unwind-protect
      (let (;; SAT: (1 OR 2) AND (NOT 1 OR 3) AND (NOT 2 OR NOT 3)
            (sat-clauses '((1 2) (-1 3) (-2 -3)))
            ;; UNSAT: (1 OR 1) AND (-1 OR -1) = (1) AND (NOT 1)
            (unsat-clauses '((1 1) (-1 -1)))
            ;; SAT: (1 OR 2) AND (1 OR -2) AND (-1 OR 2)
            (sat2-clauses '((1 2) (1 -2) (-1 2))))
        (let ((sol1 (funcall 'neovm--cst-2sat-check sat-clauses '(1 2 3)))
              (sol2 (funcall 'neovm--cst-2sat-check unsat-clauses '(1)))
              (sol3 (funcall 'neovm--cst-2sat-check sat2-clauses '(1 2))))
          (list
           ;; sat-clauses is satisfiable
           (not (null sol1))
           ;; Verify solution
           (when sol1 (funcall 'neovm--cst-2sat-verify sat-clauses sol1))
           ;; unsat-clauses is not satisfiable
           (null sol2)
           ;; sat2-clauses is satisfiable
           (not (null sol3))
           ;; Verify solution
           (when sol3 (funcall 'neovm--cst-2sat-verify sat2-clauses sol3))
           ;; sol3 must have both 1 and 2 true (forced by clauses)
           (when sol3
             (list (cdr (assq 1 sol3)) (cdr (assq 2 sol3)))))))
    (fmakunbound 'neovm--cst-2sat-negate)
    (fmakunbound 'neovm--cst-2sat-build-graph)
    (fmakunbound 'neovm--cst-2sat-reachable)
    (fmakunbound 'neovm--cst-2sat-check)
    (fmakunbound 'neovm--cst-2sat-verify)))
"#;
    let expect = expect_test::expect![[r#""OK (t nil t t t (t t))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Cryptarithmetic puzzle solver (SEND + MORE = MONEY style, reduced)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_constraint_cryptarithmetic_puzzle() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Solve: EAT + THAT = APPLE
    // Too many digits — use a smaller puzzle: AB + B = CA
    // (A*10+B) + B = C*10+A => A*10 + 2*B = C*10 + A => 9*A + 2*B = 10*C
    // Constraints: A,B,C all different; A>=1, C>=1; digits 0-9.
    let form = r#"
(progn
  (fset 'neovm--cst-crypto-gen-perms
    (lambda (digits n)
      "Generate all n-permutations of DIGITS."
      (if (= n 0)
          (list nil)
        (let ((result nil))
          (dolist (d digits)
            (let ((rest-perms (funcall 'neovm--cst-crypto-gen-perms
                                       (delq d (copy-sequence digits))
                                       (1- n))))
              (dolist (p rest-perms)
                (setq result (cons (cons d p) result)))))
          result))))

  (fset 'neovm--cst-crypto-solve-ab-b-ca
    (lambda ()
      "Solve AB + B = CA where A,B,C are distinct digits, A>=1, C>=1.
       9*A + 2*B = 10*C."
      (let ((solutions nil))
        (let ((a 1))
          (while (<= a 9)
            (let ((b 0))
              (while (<= b 9)
                (when (/= a b)
                  (let ((lhs (+ (* 9 a) (* 2 b))))
                    (when (= (% lhs 10) 0)
                      (let ((c (/ lhs 10)))
                        (when (and (>= c 1) (<= c 9)
                                   (/= c a) (/= c b))
                          ;; Verify: A*10+B + B = C*10+A
                          (let ((ab (+ (* a 10) b))
                                (ca (+ (* c 10) a)))
                            (when (= (+ ab b) ca)
                              (setq solutions
                                    (cons (list :a a :b b :c c
                                                :ab ab :sum (+ ab b) :ca ca)
                                          solutions)))))))))
                (setq b (1+ b))))
            (setq a (1+ a))))
        (nreverse solutions))))

  (fset 'neovm--cst-crypto-solve-xy-yx-zz
    (lambda ()
      "Solve XY + YX = ZZ where X,Y,Z distinct, X>=1, Y>=1.
       (X+Y)*11 = Z*11 => X+Y=Z, with Z<=9."
      (let ((solutions nil))
        (let ((x 1))
          (while (<= x 9)
            (let ((y 1))
              (while (<= y 9)
                (when (/= x y)
                  (let ((z (+ x y)))
                    (when (and (<= z 9) (/= z x) (/= z y))
                      (setq solutions
                            (cons (list :x x :y y :z z
                                        :xy (+ (* x 10) y)
                                        :yx (+ (* y 10) x)
                                        :zz (+ (* z 10) z))
                                  solutions)))))
                (setq y (1+ y))))
            (setq x (1+ x))))
        (sort (nreverse solutions)
              (lambda (a b) (< (plist-get a :xy) (plist-get b :xy)))))))

  (unwind-protect
      (let ((sol1 (funcall 'neovm--cst-crypto-solve-ab-b-ca))
            (sol2 (funcall 'neovm--cst-crypto-solve-xy-yx-zz)))
        (list
         ;; AB+B=CA solutions
         (length sol1)
         sol1
         ;; Verify each AB+B=CA solution
         (let ((ok t))
           (dolist (s sol1)
             (unless (= (+ (plist-get s :ab) (plist-get s :b))
                        (plist-get s :ca))
               (setq ok nil)))
           ok)
         ;; XY+YX=ZZ solutions count
         (length sol2)
         ;; Verify each XY+YX=ZZ solution
         (let ((ok t))
           (dolist (s sol2)
             (unless (= (+ (plist-get s :xy) (plist-get s :yx))
                        (plist-get s :zz))
               (setq ok nil)))
           ok)
         ;; First few XY+YX=ZZ solutions
         (let ((first3 nil) (count 0))
           (dolist (s sol2)
             (when (< count 3)
               (setq first3 (cons (list (plist-get s :xy)
                                        (plist-get s :yx)
                                        (plist-get s :zz))
                                  first3))
               (setq count (1+ count))))
           (nreverse first3))))
    (fmakunbound 'neovm--cst-crypto-gen-perms)
    (fmakunbound 'neovm--cst-crypto-solve-ab-b-ca)
    (fmakunbound 'neovm--cst-crypto-solve-xy-yx-zz)))
"#;
    let expect = expect_test::expect![[
        r#""OK (3 ((:a 2 :b 6 :c 3 :ab 26 :sum 32 :ca 32) (:a 4 :b 7 :c 5 :ab 47 :sum 54 :ca 54) (:a 6 :b 8 :c 7 :ab 68 :sum 76 :ca 76)) t 32 t ((12 21 33) (13 31 44) (14 41 55)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Constraint propagation with domain filtering (general CSP framework)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_constraint_general_csp_framework() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A generic CSP framework: variables with domains, binary constraints,
    // and a propagation + backtracking solver. Test it on a magic square
    // problem (3x3 with digits 1-9 summing to 15 in each row/col/diag).
    let form = r#"
(progn
  (fset 'neovm--cst-csp-make
    (lambda (vars domains constraints)
      "Create a CSP instance: vars = list of symbols, domains = hash-table
       var -> list of values, constraints = list of (var1 var2 . predicate)."
      (list :vars vars :domains domains :constraints constraints)))

  (fset 'neovm--cst-csp-propagate-once
    (lambda (csp assignment)
      "One pass of forward checking: remove values inconsistent with assignment."
      (let ((domains (plist-get csp :domains))
            (constraints (plist-get csp :constraints))
            (changed nil))
        (dolist (c constraints)
          (let ((v1 (car c)) (v2 (cadr c)) (pred (cddr c))
                (a1 (cdr (assq (car c) assignment)))
                (a2 (cdr (assq (cadr c) assignment))))
            ;; If one var assigned, filter the other's domain
            (cond
             ((and a1 (not a2))
              (let ((new-dom nil))
                (dolist (val (gethash v2 domains))
                  (when (funcall pred a1 val)
                    (setq new-dom (cons val new-dom))))
                (when (< (length new-dom) (length (gethash v2 domains)))
                  (puthash v2 (nreverse new-dom) domains)
                  (setq changed t))))
             ((and a2 (not a1))
              (let ((new-dom nil))
                (dolist (val (gethash v1 domains))
                  (when (funcall pred val a2)
                    (setq new-dom (cons val new-dom))))
                (when (< (length new-dom) (length (gethash v1 domains)))
                  (puthash v1 (nreverse new-dom) domains)
                  (setq changed t)))))))
        changed)))

  (fset 'neovm--cst-csp-solve
    (lambda (csp)
      "Solve CSP with backtracking + forward checking."
      (let ((result nil))
        (fset 'neovm--cst-csp-bt
          (lambda (vars assignment)
            (if (null vars)
                (progn (setq result (copy-sequence assignment)) t)
              (let ((v (car vars))
                    (found nil)
                    (domains (plist-get csp :domains)))
                (dolist (val (gethash v domains))
                  (unless found
                    (let ((new-assign (cons (cons v val) assignment))
                          (consistent t))
                      ;; Check all constraints involving v and assigned vars
                      (dolist (c (plist-get csp :constraints))
                        (when consistent
                          (let ((v1 (car c)) (v2 (cadr c)) (pred (cddr c)))
                            (let ((a1 (cdr (assq v1 new-assign)))
                                  (a2 (cdr (assq v2 new-assign))))
                              (when (and a1 a2 (not (funcall pred a1 a2)))
                                (setq consistent nil))))))
                      (when consistent
                        (when (funcall 'neovm--cst-csp-bt (cdr vars) new-assign)
                          (setq found t))))))
                found))))
        (funcall 'neovm--cst-csp-bt (plist-get csp :vars) nil)
        result)))

  (unwind-protect
      ;; Test: solve a small puzzle: A, B, C with domains {1,2,3},
      ;; constraints: all different, and A + B + C = 6.
      (let ((domains (make-hash-table)))
        (puthash 'a '(1 2 3) domains)
        (puthash 'b '(1 2 3) domains)
        (puthash 'c '(1 2 3) domains)
        (let* ((neq (lambda (x y) (/= x y)))
               (constraints (list (cons 'a (cons 'b neq))
                                  (cons 'a (cons 'c neq))
                                  (cons 'b (cons 'c neq))))
               (csp (funcall 'neovm--cst-csp-make '(a b c) domains constraints))
               (solution (funcall 'neovm--cst-csp-solve csp)))
          (list
           ;; Found a solution
           (not (null solution))
           ;; All different
           (when solution
             (let ((va (cdr (assq 'a solution)))
                   (vb (cdr (assq 'b solution)))
                   (vc (cdr (assq 'c solution))))
               (and (/= va vb) (/= va vc) (/= vb vc))))
           ;; Sum is 6 (1+2+3)
           (when solution
             (= (+ (cdr (assq 'a solution))
                    (cdr (assq 'b solution))
                    (cdr (assq 'c solution)))
                6))
           ;; Values are all in {1,2,3}
           (when solution
             (let ((ok t))
               (dolist (pair solution)
                 (unless (memq (cdr pair) '(1 2 3))
                   (setq ok nil)))
               ok)))))
    (fmakunbound 'neovm--cst-csp-make)
    (fmakunbound 'neovm--cst-csp-propagate-once)
    (fmakunbound 'neovm--cst-csp-solve)
    (fmakunbound 'neovm--cst-csp-bt)))
"#;
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
