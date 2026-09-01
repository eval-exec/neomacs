//! Complex oracle parity tests for logic puzzle solving in Elisp.
//!
//! Tests constraint satisfaction, state-space search, graph coloring,
//! cryptarithmetic solving, SAT checking, and logic gate simulation.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

// ---------------------------------------------------------------------------
// Sudoku row/column constraint checker
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sudoku_constraint_checker() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement a Sudoku validator that checks rows, columns, and 3x3 boxes.
    // The board is a 9-element vector of 9-element vectors (0 = empty).
    let form = r#"
(progn
  (fset 'neovm--lp-sudoku-valid-group
    (lambda (group)
      "Check that a list of 9 numbers has no duplicates (ignoring 0s)."
      (let ((seen (make-hash-table))
            (valid t))
        (dolist (n group)
          (when (and (/= n 0) valid)
            (if (gethash n seen)
                (setq valid nil)
              (puthash n t seen))))
        valid)))

  (fset 'neovm--lp-sudoku-check
    (lambda (board)
      "Check all rows, columns, and 3x3 boxes."
      (let ((valid t))
        ;; Check rows
        (let ((r 0))
          (while (and valid (< r 9))
            (unless (funcall 'neovm--lp-sudoku-valid-group
                             (append (aref board r) nil))
              (setq valid nil))
            (setq r (1+ r))))
        ;; Check columns
        (let ((c 0))
          (while (and valid (< c 9))
            (let ((col nil) (r 0))
              (while (< r 9)
                (setq col (cons (aref (aref board r) c) col))
                (setq r (1+ r)))
              (unless (funcall 'neovm--lp-sudoku-valid-group col)
                (setq valid nil)))
            (setq c (1+ c))))
        ;; Check 3x3 boxes
        (let ((br 0))
          (while (and valid (< br 3))
            (let ((bc 0))
              (while (and valid (< bc 3))
                (let ((box nil))
                  (dotimes (dr 3)
                    (dotimes (dc 3)
                      (setq box (cons (aref (aref board (+ (* br 3) dr))
                                            (+ (* bc 3) dc))
                                      box))))
                  (unless (funcall 'neovm--lp-sudoku-valid-group box)
                    (setq valid nil)))
                (setq bc (1+ bc))))
            (setq br (1+ br))))
        valid)))

  (unwind-protect
      (let ((valid-board
             (vector [5 3 4 6 7 8 9 1 2]
                     [6 7 2 1 9 5 3 4 8]
                     [1 9 8 3 4 2 5 6 7]
                     [8 5 9 7 6 1 4 2 3]
                     [4 2 6 8 5 3 7 9 1]
                     [7 1 3 9 2 4 8 5 6]
                     [9 6 1 5 3 7 2 8 4]
                     [2 8 7 4 1 9 6 3 5]
                     [3 4 5 2 8 6 1 7 9]))
            (invalid-board
             (vector [5 3 4 6 7 8 9 1 2]
                     [6 7 2 1 9 5 3 4 8]
                     [1 9 8 3 4 2 5 6 7]
                     [8 5 9 7 6 1 4 2 3]
                     [4 2 6 8 5 3 7 9 1]
                     [7 1 3 9 2 4 8 5 6]
                     [9 6 1 5 3 7 2 8 4]
                     [2 8 7 4 1 9 6 3 5]
                     [3 4 5 2 8 6 1 7 5])))  ;; duplicate 5 in last row
        (list (funcall 'neovm--lp-sudoku-check valid-board)
              (funcall 'neovm--lp-sudoku-check invalid-board)))
    (fmakunbound 'neovm--lp-sudoku-valid-group)
    (fmakunbound 'neovm--lp-sudoku-check)))
"#;
    let expect = expect_test::expect![[r#""OK (t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Eight puzzle (sliding tile) state representation and move generation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_eight_puzzle_moves() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Represent the 8-puzzle as a vector of 9 elements (0 = blank).
    // Implement move generation and goal checking.
    let form = r#"
(progn
  (fset 'neovm--lp-puzzle-find-blank
    (lambda (state)
      (let ((i 0) (pos nil))
        (while (and (< i 9) (not pos))
          (when (= (aref state i) 0)
            (setq pos i))
          (setq i (1+ i)))
        pos)))

  (fset 'neovm--lp-puzzle-neighbors
    (lambda (pos)
      "Return list of positions adjacent to POS in a 3x3 grid."
      (let ((row (/ pos 3))
            (col (% pos 3))
            (result nil))
        (when (> row 0) (setq result (cons (- pos 3) result)))  ;; up
        (when (< row 2) (setq result (cons (+ pos 3) result)))  ;; down
        (when (> col 0) (setq result (cons (- pos 1) result)))  ;; left
        (when (< col 2) (setq result (cons (+ pos 1) result)))  ;; right
        result)))

  (fset 'neovm--lp-puzzle-swap
    (lambda (state pos1 pos2)
      (let ((new-state (copy-sequence state)))
        (aset new-state pos1 (aref state pos2))
        (aset new-state pos2 (aref state pos1))
        new-state)))

  (fset 'neovm--lp-puzzle-moves
    (lambda (state)
      "Generate all possible next states from STATE."
      (let* ((blank (funcall 'neovm--lp-puzzle-find-blank state))
             (neighbors (funcall 'neovm--lp-puzzle-neighbors blank)))
        (mapcar (lambda (n)
                  (funcall 'neovm--lp-puzzle-swap state blank n))
                neighbors))))

  (fset 'neovm--lp-puzzle-goal-p
    (lambda (state)
      (equal state [1 2 3 4 5 6 7 8 0])))

  (unwind-protect
      (let ((start [1 2 3 4 0 5 7 8 6]))
        (list
          ;; Blank position
          (funcall 'neovm--lp-puzzle-find-blank start)
          ;; Neighbors of blank
          (funcall 'neovm--lp-puzzle-neighbors
                   (funcall 'neovm--lp-puzzle-find-blank start))
          ;; Number of possible moves
          (length (funcall 'neovm--lp-puzzle-moves start))
          ;; The moves themselves
          (funcall 'neovm--lp-puzzle-moves start)
          ;; Is start the goal?
          (funcall 'neovm--lp-puzzle-goal-p start)
          ;; Is [1 2 3 4 5 6 7 8 0] the goal?
          (funcall 'neovm--lp-puzzle-goal-p [1 2 3 4 5 6 7 8 0])
          ;; One move from goal
          (let ((near-goal [1 2 3 4 5 6 7 0 8]))
            (let ((next-states (funcall 'neovm--lp-puzzle-moves near-goal)))
              (let ((found-goal nil))
                (dolist (s next-states)
                  (when (funcall 'neovm--lp-puzzle-goal-p s)
                    (setq found-goal t)))
                found-goal)))))
    (fmakunbound 'neovm--lp-puzzle-find-blank)
    (fmakunbound 'neovm--lp-puzzle-neighbors)
    (fmakunbound 'neovm--lp-puzzle-swap)
    (fmakunbound 'neovm--lp-puzzle-moves)
    (fmakunbound 'neovm--lp-puzzle-goal-p)))
"#;
    let expect = expect_test::expect![[
        r#""OK (4 (5 3 7 1) 4 ([1 2 3 4 5 0 7 8 6] [1 2 3 0 4 5 7 8 6] [1 2 3 4 8 5 7 0 6] [1 0 3 4 2 5 7 8 6]) nil t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Map coloring (graph coloring with constraints)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_map_coloring() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Solve a small map coloring problem: given a graph of regions and
    // their adjacencies, find a valid 3-coloring.
    let form = r#"
(progn
  (fset 'neovm--lp-color-valid
    (lambda (coloring edges)
      "Check if no two adjacent nodes share a color."
      (let ((valid t))
        (dolist (edge edges)
          (let ((c1 (cdr (assq (car edge) coloring)))
                (c2 (cdr (assq (cdr edge) coloring))))
            (when (and c1 c2 (eq c1 c2))
              (setq valid nil))))
        valid)))

  (fset 'neovm--lp-color-solve
    (lambda (nodes edges colors)
      "Solve by backtracking: try each color for each node."
      (let ((coloring nil)
            (solved nil))
        (fset 'neovm--lp-color-bt
          (lambda (remaining)
            (if (null remaining)
                (progn (setq solved (copy-sequence coloring)) t)
              (let ((node (car remaining))
                    (found nil))
                (dolist (color colors)
                  (unless found
                    (setq coloring (cons (cons node color) coloring))
                    (if (funcall 'neovm--lp-color-valid coloring edges)
                        (when (funcall 'neovm--lp-color-bt (cdr remaining))
                          (setq found t))
                      nil)
                    (unless found
                      (setq coloring (cdr coloring)))))
                found))))
        (funcall 'neovm--lp-color-bt nodes)
        solved)))

  (unwind-protect
      (let* ((nodes '(WA NT SA Q NSW V T))
             (edges '((WA . NT) (WA . SA) (NT . SA) (NT . Q)
                      (SA . Q) (SA . NSW) (SA . V) (Q . NSW) (NSW . V)))
             (colors '(red green blue))
             (solution (funcall 'neovm--lp-color-solve nodes edges colors)))
        (list
          ;; Did we find a solution?
          (not (null solution))
          ;; Is the solution valid?
          (funcall 'neovm--lp-color-valid solution edges)
          ;; All nodes assigned?
          (= (length solution) (length nodes))
          ;; Only valid colors used?
          (let ((all-valid t))
            (dolist (entry solution)
              (unless (memq (cdr entry) colors)
                (setq all-valid nil)))
            all-valid)))
    (fmakunbound 'neovm--lp-color-valid)
    (fmakunbound 'neovm--lp-color-solve)
    (fmakunbound 'neovm--lp-color-bt)))
"#;
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Cryptarithmetic solver (SEND + MORE = MONEY)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cryptarithmetic_solver() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Solve a simpler cryptarithmetic: AB + CD = EF (2-digit version)
    // where each letter is a unique digit 0-9, and leading digits != 0.
    // Find all solutions to: AB + BA = CC (A*10+B + B*10+A = C*10+C = C*11)
    // So: (A+B)*11 = C*11, meaning A+B = C with A,B,C distinct, A!=0, B!=0.
    let form = r#"
(progn
  (fset 'neovm--lp-crypto-solve
    (lambda ()
      "Find all solutions to AB + BA = CC."
      (let ((solutions nil))
        (let ((a 1))  ;; A >= 1 (leading digit)
          (while (<= a 9)
            (let ((b 1))  ;; B >= 1 (leading digit)
              (while (<= b 9)
                (when (/= a b)
                  (let ((c (+ a b)))
                    (when (and (<= c 9)
                               (/= c a)
                               (/= c b))
                      (let ((ab (+ (* a 10) b))
                            (ba (+ (* b 10) a))
                            (cc (+ (* c 10) c)))
                        (when (= (+ ab ba) cc)
                          (setq solutions
                                (cons (list ab ba cc) solutions)))))))
                (setq b (1+ b))))
            (setq a (1+ a))))
        (sort solutions (lambda (x y) (< (car x) (car y)))))))

  (unwind-protect
      (let ((solutions (funcall 'neovm--lp-crypto-solve)))
        (list
          (length solutions)
          solutions
          ;; Verify each solution
          (let ((all-valid t))
            (dolist (sol solutions)
              (unless (= (+ (nth 0 sol) (nth 1 sol)) (nth 2 sol))
                (setq all-valid nil)))
            all-valid)))
    (fmakunbound 'neovm--lp-crypto-solve)))
"#;
    let expect = expect_test::expect![[
        r#""OK (32 ((12 21 33) (13 31 44) (14 41 55) (15 51 66) (16 61 77) (17 71 88) (18 81 99) (21 12 33) (23 32 55) (24 42 66) (25 52 77) (26 62 88) (27 72 99) (31 13 44) (32 23 55) (34 43 77) (35 53 88) (36 63 99) (41 14 55) (42 24 66) (43 34 77) (45 54 99) (51 15 66) (52 25 77) (53 35 88) (54 45 99) (61 16 77) (62 26 88) (63 36 99) (71 17 88) (72 27 99) (81 18 99)) t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Boolean satisfiability (simple SAT checker)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_sat_checker() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement a brute-force SAT checker for CNF formulas.
    // A clause is a list of literals (positive = variable, negative = negated).
    // A formula is a list of clauses. Check all 2^n assignments.
    let form = r#"
(progn
  (fset 'neovm--lp-sat-eval-literal
    (lambda (lit assignment)
      "Evaluate a literal under an assignment (alist of var -> bool)."
      (if (< lit 0)
          (not (cdr (assq (- lit) assignment)))
        (cdr (assq lit assignment)))))

  (fset 'neovm--lp-sat-eval-clause
    (lambda (clause assignment)
      "A clause is satisfied if any literal is true."
      (let ((satisfied nil))
        (dolist (lit clause)
          (when (funcall 'neovm--lp-sat-eval-literal lit assignment)
            (setq satisfied t)))
        satisfied)))

  (fset 'neovm--lp-sat-eval-formula
    (lambda (formula assignment)
      "A formula is satisfied if all clauses are satisfied."
      (let ((satisfied t))
        (dolist (clause formula)
          (unless (funcall 'neovm--lp-sat-eval-clause clause assignment)
            (setq satisfied nil)))
        satisfied)))

  (fset 'neovm--lp-sat-solve
    (lambda (formula vars)
      "Try all 2^n assignments for VARS. Return first satisfying one or nil."
      (let ((n (length vars))
            (result nil))
        ;; Iterate over all 2^n possibilities
        (let ((max (ash 1 n))
              (i 0))
          (while (and (< i max) (not result))
            (let ((assignment nil) (j 0))
              (dolist (v vars)
                (setq assignment
                      (cons (cons v (if (= (logand i (ash 1 j)) 0) nil t))
                            assignment))
                (setq j (1+ j)))
              (when (funcall 'neovm--lp-sat-eval-formula formula assignment)
                (setq result assignment)))
            (setq i (1+ i))))
        result)))

  (unwind-protect
      (let (;; (x OR y) AND (NOT x OR z) AND (NOT y OR NOT z)
            (formula1 '((1 2) (-1 3) (-2 -3)))
            ;; (x) AND (NOT x) — unsatisfiable
            (formula2 '((1) (-1)))
            ;; (x OR y) AND (x OR NOT y) AND (NOT x OR y) AND (NOT x OR NOT y) — unsat
            (formula3 '((1 2) (1 -2) (-1 2) (-1 -2))))
        (list
          ;; formula1 is satisfiable
          (not (null (funcall 'neovm--lp-sat-solve formula1 '(1 2 3))))
          ;; Verify the found assignment
          (let ((sol (funcall 'neovm--lp-sat-solve formula1 '(1 2 3))))
            (funcall 'neovm--lp-sat-eval-formula formula1 sol))
          ;; formula2 is unsatisfiable
          (null (funcall 'neovm--lp-sat-solve formula2 '(1)))
          ;; formula3 is unsatisfiable
          (null (funcall 'neovm--lp-sat-solve formula3 '(1 2)))))
    (fmakunbound 'neovm--lp-sat-eval-literal)
    (fmakunbound 'neovm--lp-sat-eval-clause)
    (fmakunbound 'neovm--lp-sat-eval-formula)
    (fmakunbound 'neovm--lp-sat-solve)))
"#;
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Logic gate simulator (AND, OR, NOT, XOR circuits)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_logic_gate_simulator() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a circuit simulator. Gates are represented as lists:
    // (gate-type input1 input2) or (not input).
    // Inputs are either symbols (looked up in environment) or nested gates.
    let form = r#"
(progn
  (fset 'neovm--lp-gate-eval
    (lambda (circuit env)
      "Evaluate a logic circuit under an environment."
      (cond
        ((symbolp circuit)
         (if (cdr (assq circuit env)) t nil))
        ((eq (car circuit) 'not)
         (not (funcall 'neovm--lp-gate-eval (nth 1 circuit) env)))
        ((eq (car circuit) 'and)
         (and (funcall 'neovm--lp-gate-eval (nth 1 circuit) env)
              (funcall 'neovm--lp-gate-eval (nth 2 circuit) env)))
        ((eq (car circuit) 'or)
         (or (funcall 'neovm--lp-gate-eval (nth 1 circuit) env)
             (funcall 'neovm--lp-gate-eval (nth 2 circuit) env)))
        ((eq (car circuit) 'xor)
         (let ((a (funcall 'neovm--lp-gate-eval (nth 1 circuit) env))
               (b (funcall 'neovm--lp-gate-eval (nth 2 circuit) env)))
           (and (or a b) (not (and a b)))))
        ((eq (car circuit) 'nand)
         (not (and (funcall 'neovm--lp-gate-eval (nth 1 circuit) env)
                   (funcall 'neovm--lp-gate-eval (nth 2 circuit) env))))
        (t (error "Unknown gate: %s" (car circuit))))))

  ;; Build a half-adder: sum = A XOR B, carry = A AND B
  (fset 'neovm--lp-half-adder
    (lambda (env)
      (list (if (funcall 'neovm--lp-gate-eval '(xor a b) env) 1 0)
            (if (funcall 'neovm--lp-gate-eval '(and a b) env) 1 0))))

  ;; Build a full adder using half-adders:
  ;; sum = (a XOR b) XOR cin, cout = (a AND b) OR ((a XOR b) AND cin)
  (fset 'neovm--lp-full-adder
    (lambda (env)
      (list (if (funcall 'neovm--lp-gate-eval
                         '(xor (xor a b) cin) env) 1 0)
            (if (funcall 'neovm--lp-gate-eval
                         '(or (and a b) (and (xor a b) cin)) env) 1 0))))

  (unwind-protect
      (list
        ;; Half-adder truth table: (a b) -> (sum carry)
        (funcall 'neovm--lp-half-adder '((a . nil) (b . nil)))
        (funcall 'neovm--lp-half-adder '((a . t) (b . nil)))
        (funcall 'neovm--lp-half-adder '((a . nil) (b . t)))
        (funcall 'neovm--lp-half-adder '((a . t) (b . t)))
        ;; Full-adder truth table: (a b cin) -> (sum cout)
        (funcall 'neovm--lp-full-adder '((a . nil) (b . nil) (cin . nil)))
        (funcall 'neovm--lp-full-adder '((a . t) (b . nil) (cin . nil)))
        (funcall 'neovm--lp-full-adder '((a . nil) (b . t) (cin . nil)))
        (funcall 'neovm--lp-full-adder '((a . t) (b . t) (cin . nil)))
        (funcall 'neovm--lp-full-adder '((a . nil) (b . nil) (cin . t)))
        (funcall 'neovm--lp-full-adder '((a . t) (b . nil) (cin . t)))
        (funcall 'neovm--lp-full-adder '((a . nil) (b . t) (cin . t)))
        (funcall 'neovm--lp-full-adder '((a . t) (b . t) (cin . t)))
        ;; Complex circuit: De Morgan's law verification
        ;; NOT(A AND B) == (NOT A) OR (NOT B)
        (let ((envs '(((a . nil) (b . nil))
                      ((a . t) (b . nil))
                      ((a . nil) (b . t))
                      ((a . t) (b . t)))))
          (mapcar (lambda (env)
                    (eq (funcall 'neovm--lp-gate-eval '(nand a b) env)
                        (funcall 'neovm--lp-gate-eval
                                 '(or (not a) (not b)) env)))
                  envs)))
    (fmakunbound 'neovm--lp-gate-eval)
    (fmakunbound 'neovm--lp-half-adder)
    (fmakunbound 'neovm--lp-full-adder)))
"#;
    let expect = expect_test::expect![[
        r#""OK ((0 0) (1 0) (1 0) (0 1) (0 0) (1 0) (1 0) (0 1) (1 0) (0 1) (0 1) (1 1) (t t t t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
