//! Complex oracle parity tests for problem-solving patterns in Elisp.
//!
//! Tests maze solver (BFS), sudoku solver (constraint propagation + backtracking
//! for 4x4), expression evaluator with variables and assignment, simple regex
//! engine, countdown numbers game solver, and water jug problem (BFS).

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

// ---------------------------------------------------------------------------
// Maze solver (BFS on grid)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ps_maze_solver_bfs() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // BFS on a 2D grid maze. 0=open, 1=wall. Find shortest path from
    // top-left to bottom-right.
    let form = r#"(progn
  (fset 'neovm--ps-maze-solve
    (lambda (maze rows cols)
      "BFS from (0,0) to (rows-1, cols-1). Returns path length or nil."
      (let ((visited (make-hash-table :test 'equal))
            (queue (list (list 0 0 0)))   ;; (row col dist)
            (target-r (1- rows))
            (target-c (1- cols))
            (result nil)
            (dirs '((0 1) (0 -1) (1 0) (-1 0))))
        (puthash (cons 0 0) t visited)
        (while (and queue (not result))
          (let* ((cur (car queue))
                 (r (nth 0 cur)) (c (nth 1 cur)) (d (nth 2 cur)))
            (setq queue (cdr queue))
            (if (and (= r target-r) (= c target-c))
                (setq result d)
              (dolist (dir dirs)
                (let ((nr (+ r (car dir))) (nc (+ c (cadr dir))))
                  (when (and (>= nr 0) (< nr rows) (>= nc 0) (< nc cols)
                             (= (aref (aref maze nr) nc) 0)
                             (not (gethash (cons nr nc) visited)))
                    (puthash (cons nr nc) t visited)
                    (setq queue (append queue (list (list nr nc (1+ d)))))))))))
        result)))

  (unwind-protect
      (let (;; Simple 5x5 maze with clear path
            (maze1 (vector [0 0 1 0 0]
                           [0 0 1 0 1]
                           [1 0 0 0 1]
                           [1 1 1 0 0]
                           [0 0 0 0 0]))
            ;; Maze with no path
            (maze2 (vector [0 1 0]
                           [1 1 0]
                           [0 0 0]))
            ;; Trivial 1x1 maze
            (maze3 (vector [0]))
            ;; 4x4 maze with multiple paths
            (maze4 (vector [0 0 0 0]
                           [0 0 0 0]
                           [0 0 0 0]
                           [0 0 0 0])))
        (list
          ;; Shortest path in maze1 (should be 8)
          (funcall 'neovm--ps-maze-solve maze1 5 5)
          ;; No path in maze2
          (funcall 'neovm--ps-maze-solve maze2 3 3)
          ;; Trivial maze: distance 0
          (funcall 'neovm--ps-maze-solve maze3 1 1)
          ;; Open 4x4 maze: shortest path is 6 (Manhattan distance)
          (funcall 'neovm--ps-maze-solve maze4 4 4)))
    (fmakunbound 'neovm--ps-maze-solve)))"#;
    let expect = expect_test::expect![[r#""OK (8 nil 0 6)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Sudoku solver (constraint propagation + backtracking for 4x4)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ps_sudoku_4x4_solver() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Solve a 4x4 Sudoku (values 1-4, 2x2 boxes)
    let form = r#"(progn
  (fset 'neovm--ps-sdk-copy-board
    (lambda (board)
      (let ((new (make-vector 4 nil)))
        (dotimes (r 4)
          (aset new r (copy-sequence (aref board r))))
        new)))

  (fset 'neovm--ps-sdk-valid-p
    (lambda (board r c val)
      "Check if placing VAL at (r,c) is valid in 4x4 sudoku."
      (let ((ok t))
        ;; Check row
        (dotimes (j 4)
          (when (= (aref (aref board r) j) val) (setq ok nil)))
        ;; Check column
        (dotimes (i 4)
          (when (= (aref (aref board i) c) val) (setq ok nil)))
        ;; Check 2x2 box
        (let ((br (* 2 (/ r 2))) (bc (* 2 (/ c 2))))
          (dotimes (dr 2)
            (dotimes (dc 2)
              (when (= (aref (aref board (+ br dr)) (+ bc dc)) val)
                (setq ok nil)))))
        ok)))

  (fset 'neovm--ps-sdk-find-empty
    (lambda (board)
      "Find first empty cell (value 0). Return (r . c) or nil."
      (let ((found nil) (r 0))
        (while (and (< r 4) (not found))
          (let ((c 0))
            (while (and (< c 4) (not found))
              (when (= (aref (aref board r) c) 0)
                (setq found (cons r c)))
              (setq c (1+ c))))
          (setq r (1+ r)))
        found)))

  (fset 'neovm--ps-sdk-solve
    (lambda (board)
      "Solve 4x4 sudoku by backtracking. Returns solved board or nil."
      (let ((empty (funcall 'neovm--ps-sdk-find-empty board)))
        (if (null empty)
            board  ;; all filled = solved
          (let ((r (car empty)) (c (cdr empty))
                (result nil))
            (let ((v 1))
              (while (and (<= v 4) (not result))
                (when (funcall 'neovm--ps-sdk-valid-p board r c v)
                  (let ((new-board (funcall 'neovm--ps-sdk-copy-board board)))
                    (aset (aref new-board r) c v)
                    (let ((sol (funcall 'neovm--ps-sdk-solve new-board)))
                      (when sol (setq result sol)))))
                (setq v (1+ v))))
            result)))))

  (fset 'neovm--ps-sdk-board-to-list
    (lambda (board)
      (let ((result nil))
        (dotimes (r 4)
          (setq result (cons (append (aref board r) nil) result)))
        (nreverse result))))

  (unwind-protect
      (let (;; Puzzle 1: partially filled 4x4
            (p1 (vector [1 0 0 0]
                        [0 0 3 0]
                        [0 3 0 0]
                        [0 0 0 2]))
            ;; Puzzle 2: more constrained
            (p2 (vector [0 2 0 0]
                        [0 0 0 1]
                        [1 0 0 0]
                        [0 0 4 0])))
        (let ((sol1 (funcall 'neovm--ps-sdk-solve p1))
              (sol2 (funcall 'neovm--ps-sdk-solve p2)))
          (list
            ;; Verify solutions exist
            (not (null sol1))
            (not (null sol2))
            ;; Show solutions
            (funcall 'neovm--ps-sdk-board-to-list sol1)
            (funcall 'neovm--ps-sdk-board-to-list sol2)
            ;; Verify no zeros remain
            (let ((no-zeros t))
              (dotimes (r 4)
                (dotimes (c 4)
                  (when (= (aref (aref sol1 r) c) 0)
                    (setq no-zeros nil))))
              no-zeros))))
    (fmakunbound 'neovm--ps-sdk-copy-board)
    (fmakunbound 'neovm--ps-sdk-valid-p)
    (fmakunbound 'neovm--ps-sdk-find-empty)
    (fmakunbound 'neovm--ps-sdk-solve)
    (fmakunbound 'neovm--ps-sdk-board-to-list)))"#;
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument arrayp nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Expression evaluator with variables and assignment
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ps_expression_evaluator() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A simple expression evaluator supporting +, -, *, /, let-bindings,
    // if-expressions, and variable references.
    let form = r#"(progn
  (fset 'neovm--ps-eval-expr
    (lambda (expr env)
      "Evaluate an expression in an environment (alist)."
      (cond
        ;; Number literal
        ((numberp expr) expr)
        ;; Variable reference
        ((symbolp expr)
         (let ((binding (assq expr env)))
           (if binding (cdr binding)
             (error "Unbound variable: %s" expr))))
        ;; Compound expression
        ((consp expr)
         (let ((op (car expr)))
           (cond
             ;; Arithmetic: (+ e1 e2), (- e1 e2), (* e1 e2), (/ e1 e2)
             ((memq op '(+ - * /))
              (let ((a (funcall 'neovm--ps-eval-expr (nth 1 expr) env))
                    (b (funcall 'neovm--ps-eval-expr (nth 2 expr) env)))
                (cond ((eq op '+) (+ a b))
                      ((eq op '-) (- a b))
                      ((eq op '*) (* a b))
                      ((eq op '/) (/ a b)))))
             ;; Comparison: (< e1 e2), (> e1 e2), (= e1 e2)
             ((memq op '(< > =))
              (let ((a (funcall 'neovm--ps-eval-expr (nth 1 expr) env))
                    (b (funcall 'neovm--ps-eval-expr (nth 2 expr) env)))
                (cond ((eq op '<) (< a b))
                      ((eq op '>) (> a b))
                      ((eq op '=) (= a b)))))
             ;; Let binding: (let ((var val)) body)
             ((eq op 'let)
              (let* ((bindings (nth 1 expr))
                     (body (nth 2 expr))
                     (new-env env))
                (dolist (b bindings)
                  (let ((var (car b))
                        (val (funcall 'neovm--ps-eval-expr (cadr b) env)))
                    (setq new-env (cons (cons var val) new-env))))
                (funcall 'neovm--ps-eval-expr body new-env)))
             ;; If expression: (if cond then else)
             ((eq op 'if)
              (if (funcall 'neovm--ps-eval-expr (nth 1 expr) env)
                  (funcall 'neovm--ps-eval-expr (nth 2 expr) env)
                (funcall 'neovm--ps-eval-expr (nth 3 expr) env)))
             ;; Sequence: (progn e1 e2 ... en)
             ((eq op 'progn)
              (let ((result nil))
                (dolist (e (cdr expr))
                  (setq result (funcall 'neovm--ps-eval-expr e env)))
                result))
             (t (error "Unknown operator: %s" op)))))
        (t (error "Invalid expression: %s" expr)))))

  (unwind-protect
      (list
        ;; Simple arithmetic
        (funcall 'neovm--ps-eval-expr '(+ 3 4) nil)
        ;; Nested arithmetic
        (funcall 'neovm--ps-eval-expr '(* (+ 2 3) (- 10 4)) nil)
        ;; Let bindings
        (funcall 'neovm--ps-eval-expr
                 '(let ((x 5) (y 3)) (+ x y)) nil)
        ;; Nested let
        (funcall 'neovm--ps-eval-expr
                 '(let ((x 10))
                    (let ((y (* x 2)))
                      (+ x y))) nil)
        ;; If expression
        (funcall 'neovm--ps-eval-expr
                 '(if (> 5 3) (+ 1 2) (- 1 2)) nil)
        ;; Complex: factorial via repeated let (unrolled for n=5)
        (funcall 'neovm--ps-eval-expr
                 '(let ((n 5))
                    (let ((f1 1))
                      (let ((f2 (* f1 2)))
                        (let ((f3 (* f2 3)))
                          (let ((f4 (* f3 4)))
                            (let ((f5 (* f4 5)))
                              f5)))))) nil)
        ;; Progn
        (funcall 'neovm--ps-eval-expr
                 '(progn (+ 1 2) (* 3 4) (- 10 5)) nil)
        ;; Nested if with comparison
        (funcall 'neovm--ps-eval-expr
                 '(let ((x 15))
                    (if (> x 20)
                        100
                      (if (> x 10)
                          50
                        0))) nil))
    (fmakunbound 'neovm--ps-eval-expr)))"#;
    let expect = expect_test::expect![[r#""OK (7 30 8 30 3 120 5 50)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Simple regex engine (literal, ., *, concatenation)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ps_simple_regex_engine() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A minimal regex engine supporting: literal chars, . (any char),
    // * (zero or more of preceding), and concatenation.
    let form = r#"(progn
  (fset 'neovm--ps-re-match-here
    (lambda (re-chars text-chars)
      "Match RE-CHARS against TEXT-CHARS at current position."
      (cond
        ;; Empty pattern matches anything
        ((null re-chars) t)
        ;; Kleene star: char* or .*
        ((and (cdr re-chars) (eq (cadr re-chars) ?*))
         (funcall 'neovm--ps-re-match-star
                  (car re-chars) (cddr re-chars) text-chars))
        ;; End of text: pattern must be empty
        ((null text-chars) nil)
        ;; Dot matches any character
        ((or (eq (car re-chars) ?.)
             (eq (car re-chars) (car text-chars)))
         (funcall 'neovm--ps-re-match-here (cdr re-chars) (cdr text-chars)))
        (t nil))))

  (fset 'neovm--ps-re-match-star
    (lambda (c re-rest text-chars)
      "Match c* followed by RE-REST against TEXT-CHARS."
      ;; Try matching zero occurrences first, then one, two, ...
      (if (funcall 'neovm--ps-re-match-here re-rest text-chars)
          t
        (if (and text-chars
                 (or (eq c ?.) (eq c (car text-chars))))
            (funcall 'neovm--ps-re-match-star c re-rest (cdr text-chars))
          nil))))

  (fset 'neovm--ps-re-search
    (lambda (pattern text)
      "Search for PATTERN anywhere in TEXT. Return t if found."
      (let ((re-chars (append pattern nil))
            (text-chars (append text nil))
            (found nil)
            (pos text-chars))
        ;; Try matching at each position
        (while (and (not found) pos)
          (when (funcall 'neovm--ps-re-match-here re-chars pos)
            (setq found t))
          (setq pos (cdr pos)))
        ;; Also try at the very end (empty string match)
        (unless found
          (when (funcall 'neovm--ps-re-match-here re-chars nil)
            (setq found t)))
        found)))

  (fset 'neovm--ps-re-full-match
    (lambda (pattern text)
      "Match PATTERN against entire TEXT."
      (funcall 'neovm--ps-re-match-here
               (append pattern nil) (append text nil))))

  (unwind-protect
      (list
        ;; Literal matches
        (funcall 'neovm--ps-re-full-match "abc" "abc")
        (funcall 'neovm--ps-re-full-match "abc" "abd")
        ;; Dot matches any
        (funcall 'neovm--ps-re-full-match "a.c" "abc")
        (funcall 'neovm--ps-re-full-match "a.c" "axc")
        (funcall 'neovm--ps-re-full-match "a.c" "ac")
        ;; Star: zero or more
        (funcall 'neovm--ps-re-full-match "ab*c" "ac")
        (funcall 'neovm--ps-re-full-match "ab*c" "abc")
        (funcall 'neovm--ps-re-full-match "ab*c" "abbbc")
        ;; Dot-star: match anything
        (funcall 'neovm--ps-re-full-match "a.*c" "ac")
        (funcall 'neovm--ps-re-full-match "a.*c" "aXYZc")
        (funcall 'neovm--ps-re-full-match "a.*c" "aXYZ")
        ;; Search (substring match)
        (funcall 'neovm--ps-re-search "bc" "abcd")
        (funcall 'neovm--ps-re-search "xy" "abcd")
        ;; Complex patterns
        (funcall 'neovm--ps-re-full-match "a*b*c*" "")
        (funcall 'neovm--ps-re-full-match "a*b*c*" "aabbbcc")
        (funcall 'neovm--ps-re-full-match ".*" "anything")
        ;; Pattern with multiple stars
        (funcall 'neovm--ps-re-full-match "a*b*" "aaabbb")
        (funcall 'neovm--ps-re-full-match "a*b*" "ba"))
    (fmakunbound 'neovm--ps-re-match-here)
    (fmakunbound 'neovm--ps-re-match-star)
    (fmakunbound 'neovm--ps-re-search)
    (fmakunbound 'neovm--ps-re-full-match)))"#;
    let expect = expect_test::expect![[r#""ERR (void-variable text-chars)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Countdown numbers game solver
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ps_countdown_solver() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Given a set of numbers and a target, find an arithmetic expression
    // using +, -, *, / that reaches the target. Each number used at most once.
    let form = r#"(progn
  (fset 'neovm--ps-cd-remove-nth
    (lambda (lst n)
      "Remove nth element from list."
      (let ((result nil) (i 0))
        (dolist (x lst)
          (unless (= i n)
            (setq result (cons x result)))
          (setq i (1+ i)))
        (nreverse result))))

  (fset 'neovm--ps-cd-solve
    (lambda (numbers target)
      "Find if target is reachable. Returns expression string or nil."
      (let ((found nil))
        ;; Base: check if any number equals target
        (let ((i 0))
          (dolist (n numbers)
            (when (and (not found) (= n target))
              (setq found (number-to-string n)))
            (setq i (1+ i))))
        ;; Try all pairs with all operations
        (when (and (not found) (>= (length numbers) 2))
          (let ((len (length numbers)) (i 0))
            (while (and (< i len) (not found))
              (let ((j 0))
                (while (and (< j len) (not found))
                  (when (/= i j)
                    (let ((a (nth i numbers))
                          (b (nth j numbers))
                          (rest (funcall 'neovm--ps-cd-remove-nth
                                         (funcall 'neovm--ps-cd-remove-nth
                                                  numbers (max i j))
                                         (min i j))))
                      ;; Try each operation
                      (dolist (op '(+ - * /))
                        (unless found
                          (let ((result nil) (valid t))
                            (cond
                              ((eq op '+) (setq result (+ a b)))
                              ((eq op '-) (setq result (- a b)))
                              ((eq op '*) (setq result (* a b)))
                              ((eq op '/)
                               (if (and (/= b 0) (= (% a b) 0))
                                   (setq result (/ a b))
                                 (setq valid nil))))
                            (when (and valid result (> result 0))
                              (let ((sub (funcall 'neovm--ps-cd-solve
                                                  (cons result rest) target)))
                                (when sub
                                  (setq found
                                        (format "(%s %s %s) -> %s"
                                                a op b sub))))))))))
                  (setq j (1+ j))))
              (setq i (1+ i)))))
        found)))

  (unwind-protect
      (list
        ;; Simple: 25+75=100
        (not (null (funcall 'neovm--ps-cd-solve '(25 75) 100)))
        ;; Using multiplication: 5*20=100
        (not (null (funcall 'neovm--ps-cd-solve '(5 20) 100)))
        ;; Three numbers: various combinations
        (not (null (funcall 'neovm--ps-cd-solve '(2 3 7) 13)))
        ;; Harder: 4 numbers to reach 24 (classic "24 game")
        (not (null (funcall 'neovm--ps-cd-solve '(1 5 5 5) 24)))
        ;; Impossible target
        (null (funcall 'neovm--ps-cd-solve '(1 2) 100))
        ;; Single number matches target
        (not (null (funcall 'neovm--ps-cd-solve '(42) 42)))
        ;; Single number doesn't match
        (null (funcall 'neovm--ps-cd-solve '(42) 43)))
    (fmakunbound 'neovm--ps-cd-remove-nth)
    (fmakunbound 'neovm--ps-cd-solve)))"#;
    let expect = expect_test::expect![[r#""OK (t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Water jug problem (BFS state space search)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ps_water_jug_bfs() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Classic water jug problem: two jugs of capacity A and B,
    // find steps to measure exactly T liters.
    let form = r#"(progn
  (fset 'neovm--ps-wj-solve
    (lambda (cap-a cap-b target)
      "BFS to find minimum steps to measure TARGET liters in either jug.
       Returns number of steps or nil if impossible."
      (let ((visited (make-hash-table :test 'equal))
            (queue (list (list 0 0 0)))  ;; (jug-a jug-b steps)
            (result nil))
        (puthash (cons 0 0) t visited)
        (while (and queue (not result))
          (let* ((state (car queue))
                 (a (nth 0 state)) (b (nth 1 state)) (steps (nth 2 state)))
            (setq queue (cdr queue))
            ;; Check if target reached
            (when (or (= a target) (= b target))
              (setq result steps))
            (unless result
              ;; Generate all possible next states
              (let ((next-states
                     (list
                       ;; Fill A
                       (list cap-a b)
                       ;; Fill B
                       (list a cap-b)
                       ;; Empty A
                       (list 0 b)
                       ;; Empty B
                       (list a 0)
                       ;; Pour A -> B
                       (let ((pour (min a (- cap-b b))))
                         (list (- a pour) (+ b pour)))
                       ;; Pour B -> A
                       (let ((pour (min b (- cap-a a))))
                         (list (+ a pour) (- b pour))))))
                (dolist (ns next-states)
                  (let ((key (cons (car ns) (cadr ns))))
                    (unless (gethash key visited)
                      (puthash key t visited)
                      (setq queue (append queue
                                          (list (list (car ns) (cadr ns)
                                                      (1+ steps))))))))))))
        result)))

  (unwind-protect
      (list
        ;; Classic: 3L and 5L jugs, measure 4L
        (funcall 'neovm--ps-wj-solve 3 5 4)
        ;; 3L and 5L, measure 1L
        (funcall 'neovm--ps-wj-solve 3 5 1)
        ;; 3L and 5L, measure 3L (just fill A)
        (funcall 'neovm--ps-wj-solve 3 5 3)
        ;; 3L and 5L, measure 5L (just fill B)
        (funcall 'neovm--ps-wj-solve 3 5 5)
        ;; 3L and 5L, measure 0L (already there)
        (funcall 'neovm--ps-wj-solve 3 5 0)
        ;; 4L and 9L, measure 6L
        (funcall 'neovm--ps-wj-solve 4 9 6)
        ;; 2L and 6L, measure 5L (impossible: gcd(2,6)=2, 5 not divisible by 2)
        (funcall 'neovm--ps-wj-solve 2 6 5)
        ;; 7L and 11L, measure 1L
        (funcall 'neovm--ps-wj-solve 7 11 1))
    (fmakunbound 'neovm--ps-wj-solve)))"#;
    let expect = expect_test::expect![[r#""OK (6 4 1 1 0 8 nil 8)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// N-Queens solver (place N non-attacking queens on NxN board)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ps_n_queens() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Solve N-Queens for small N and count solutions
    let form = r#"(progn
  (fset 'neovm--ps-nq-safe-p
    (lambda (queens row col)
      "Check if placing queen at (row, col) is safe given existing QUEENS.
       QUEENS is alist of (row . col)."
      (let ((safe t))
        (dolist (q queens)
          (let ((qr (car q)) (qc (cdr q)))
            (when (or (= qc col)
                      (= (abs (- qr row)) (abs (- qc col))))
              (setq safe nil))))
        safe)))

  (fset 'neovm--ps-nq-solve
    (lambda (n)
      "Return the count of distinct solutions for n-queens."
      (let ((count 0))
        (fset 'neovm--ps-nq-bt
          (lambda (row queens)
            (if (= row n)
                (setq count (1+ count))
              (let ((col 0))
                (while (< col n)
                  (when (funcall 'neovm--ps-nq-safe-p queens row col)
                    (funcall 'neovm--ps-nq-bt (1+ row)
                             (cons (cons row col) queens)))
                  (setq col (1+ col)))))))
        (funcall 'neovm--ps-nq-bt 0 nil)
        count)))

  (fset 'neovm--ps-nq-first-solution
    (lambda (n)
      "Return first solution as list of column positions."
      (let ((result nil))
        (fset 'neovm--ps-nq-bt1
          (lambda (row queens)
            (if (= row n)
                (setq result (mapcar #'cdr (reverse queens)))
              (let ((col 0))
                (while (and (< col n) (not result))
                  (when (funcall 'neovm--ps-nq-safe-p queens row col)
                    (funcall 'neovm--ps-nq-bt1 (1+ row)
                             (cons (cons row col) queens)))
                  (setq col (1+ col)))))))
        (funcall 'neovm--ps-nq-bt1 0 nil)
        result)))

  (unwind-protect
      (list
        ;; Count solutions
        (funcall 'neovm--ps-nq-solve 1)   ;; 1
        (funcall 'neovm--ps-nq-solve 2)   ;; 0
        (funcall 'neovm--ps-nq-solve 3)   ;; 0
        (funcall 'neovm--ps-nq-solve 4)   ;; 2
        (funcall 'neovm--ps-nq-solve 5)   ;; 10
        (funcall 'neovm--ps-nq-solve 6)   ;; 4
        ;; First solution for 4-queens
        (funcall 'neovm--ps-nq-first-solution 4)
        ;; First solution for 5-queens
        (funcall 'neovm--ps-nq-first-solution 5)
        ;; Verify 1-queen solution
        (funcall 'neovm--ps-nq-first-solution 1))
    (fmakunbound 'neovm--ps-nq-safe-p)
    (fmakunbound 'neovm--ps-nq-solve)
    (fmakunbound 'neovm--ps-nq-bt)
    (fmakunbound 'neovm--ps-nq-first-solution)
    (fmakunbound 'neovm--ps-nq-bt1)))"#;
    let expect = expect_test::expect![[r#""OK (1 0 0 2 10 4 (1 3 0 2) (0 2 4 1 3) (0))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Tower of Hanoi solver with move recording
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ps_tower_of_hanoi() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--ps-hanoi-solve
    (lambda (n from to aux)
      "Solve Tower of Hanoi for N disks. Return list of moves."
      (if (= n 0)
          nil
        (append
         (funcall 'neovm--ps-hanoi-solve (1- n) from aux to)
         (list (list from to))
         (funcall 'neovm--ps-hanoi-solve (1- n) aux to from)))))

  (fset 'neovm--ps-hanoi-verify
    (lambda (n moves)
      "Verify the move sequence is valid using three stacks."
      (let ((pegs (vector nil nil nil))
            (valid t))
        ;; Initialize: all disks on peg 0 (largest first)
        (let ((i n))
          (while (> i 0)
            (aset pegs 0 (cons i (aref pegs 0)))
            (setq i (1- i))))
        ;; Execute moves
        (dolist (move moves)
          (when valid
            (let* ((from-peg (nth 0 move))
                   (to-peg (nth 1 move))
                   (from-stack (aref pegs from-peg))
                   (to-stack (aref pegs to-peg)))
              (if (null from-stack)
                  (setq valid nil)
                (let ((disk (car from-stack)))
                  (when (and to-stack (< (car to-stack) disk))
                    (setq valid nil))
                  (aset pegs from-peg (cdr from-stack))
                  (aset pegs to-peg (cons disk to-stack)))))))
        ;; Check: all disks on target peg
        (and valid
             (null (aref pegs 0))
             (null (aref pegs 1))
             (= (length (aref pegs 2)) n)))))

  (unwind-protect
      (let ((moves1 (funcall 'neovm--ps-hanoi-solve 1 0 2 1))
            (moves2 (funcall 'neovm--ps-hanoi-solve 2 0 2 1))
            (moves3 (funcall 'neovm--ps-hanoi-solve 3 0 2 1))
            (moves4 (funcall 'neovm--ps-hanoi-solve 4 0 2 1)))
        (list
          ;; Move counts: 2^n - 1
          (length moves1) (length moves2) (length moves3) (length moves4)
          ;; Actual moves for n=3
          moves3
          ;; Verify all solutions
          (funcall 'neovm--ps-hanoi-verify 1 moves1)
          (funcall 'neovm--ps-hanoi-verify 2 moves2)
          (funcall 'neovm--ps-hanoi-verify 3 moves3)
          (funcall 'neovm--ps-hanoi-verify 4 moves4)))
    (fmakunbound 'neovm--ps-hanoi-solve)
    (fmakunbound 'neovm--ps-hanoi-verify)))"#;
    let expect = expect_test::expect![[
        r#""OK (1 3 7 15 ((0 2) (0 1) (2 1) (0 2) (1 0) (1 2) (0 2)) t t t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
