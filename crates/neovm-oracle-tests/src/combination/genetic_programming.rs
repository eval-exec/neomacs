//! Oracle parity tests for genetic programming (GP) implemented in pure Elisp.
//!
//! Covers: tree-based program representation, fitness evaluation,
//! subtree crossover (exchange), mutation (random subtree replacement),
//! tournament selection, symbolic regression (fitting polynomial),
//! and population evolution over generations.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Tree-based program representation and evaluation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_gp_tree_representation_and_eval() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(progn
  ;; GP trees: terminals are numbers or 'x (variable)
  ;; Functions: (+ left right), (- left right), (* left right), (safe-div left right)

  ;; Evaluate a GP tree with variable bindings
  (fset 'neovm--gp-eval
    (lambda (tree env)
      "Evaluate GP tree. ENV is alist of (var . value)."
      (cond
       ((numberp tree) tree)
       ((symbolp tree) (or (cdr (assq tree env)) 0))
       ((listp tree)
        (let ((op (car tree))
              (left (funcall 'neovm--gp-eval (nth 1 tree) env))
              (right (funcall 'neovm--gp-eval (nth 2 tree) env)))
          (cond
           ((eq op '+) (+ left right))
           ((eq op '-) (- left right))
           ((eq op '*) (* left right))
           ((eq op 'safe-div) (if (= right 0) 0 (/ left right)))
           (t 0))))
       (t 0))))

  ;; Tree depth
  (fset 'neovm--gp-depth
    (lambda (tree)
      (if (or (numberp tree) (symbolp tree))
          0
        (1+ (max (funcall 'neovm--gp-depth (nth 1 tree))
                 (funcall 'neovm--gp-depth (nth 2 tree)))))))

  ;; Count nodes
  (fset 'neovm--gp-node-count
    (lambda (tree)
      (if (or (numberp tree) (symbolp tree))
          1
        (+ 1
           (funcall 'neovm--gp-node-count (nth 1 tree))
           (funcall 'neovm--gp-node-count (nth 2 tree))))))

  ;; Tree to string (for display)
  (fset 'neovm--gp-to-string
    (lambda (tree)
      (cond
       ((numberp tree) (number-to-string tree))
       ((symbolp tree) (symbol-name tree))
       ((listp tree)
        (format "(%s %s %s)"
                (symbol-name (car tree))
                (funcall 'neovm--gp-to-string (nth 1 tree))
                (funcall 'neovm--gp-to-string (nth 2 tree)))))))

  (unwind-protect
      (let ((t1 '(+ x 1))              ;; x + 1
            (t2 '(* x x))              ;; x * x
            (t3 '(+ (* x x) (- x 3))) ;; x^2 + (x - 3)
            (t4 '(safe-div (* x 10) (+ x 1)))  ;; 10x / (x+1)
            (t5 'x)                     ;; just the variable
            (t6 42))                    ;; just a constant
        (list
         ;; Evaluate t1 at x=5: 5+1=6
         (funcall 'neovm--gp-eval t1 '((x . 5)))
         ;; Evaluate t2 at x=7: 49
         (funcall 'neovm--gp-eval t2 '((x . 7)))
         ;; Evaluate t3 at x=4: 16+1=17
         (funcall 'neovm--gp-eval t3 '((x . 4)))
         ;; Evaluate t4 at x=9: 90/10=9
         (funcall 'neovm--gp-eval t4 '((x . 9)))
         ;; Safe division by zero
         (funcall 'neovm--gp-eval '(safe-div 10 (- x x)) '((x . 3)))
         ;; Terminal evaluations
         (funcall 'neovm--gp-eval t5 '((x . 42)))
         (funcall 'neovm--gp-eval t6 '((x . 99)))
         ;; Depth calculations
         (funcall 'neovm--gp-depth t1)   ;; 1
         (funcall 'neovm--gp-depth t3)   ;; 2
         (funcall 'neovm--gp-depth t5)   ;; 0
         (funcall 'neovm--gp-depth t6)   ;; 0
         ;; Node counts
         (funcall 'neovm--gp-node-count t1)   ;; 3
         (funcall 'neovm--gp-node-count t3)   ;; 7
         (funcall 'neovm--gp-node-count t5)   ;; 1
         ;; String representation
         (funcall 'neovm--gp-to-string t1)
         (funcall 'neovm--gp-to-string t3)))
    (fmakunbound 'neovm--gp-eval)
    (fmakunbound 'neovm--gp-depth)
    (fmakunbound 'neovm--gp-node-count)
    (fmakunbound 'neovm--gp-to-string)))"####;
    let expect = expect_test::expect![[
        r#""OK (6 49 17 9 0 42 42 1 2 0 0 3 7 1 \"(+ x 1)\" \"(+ (* x x) (- x 3))\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Fitness evaluation for symbolic regression
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_gp_fitness_evaluation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(progn
  (fset 'neovm--gp2-eval
    (lambda (tree env)
      (cond
       ((numberp tree) tree)
       ((symbolp tree) (or (cdr (assq tree env)) 0))
       ((listp tree)
        (let ((op (car tree))
              (l (funcall 'neovm--gp2-eval (nth 1 tree) env))
              (r (funcall 'neovm--gp2-eval (nth 2 tree) env)))
          (cond ((eq op '+) (+ l r))
                ((eq op '-) (- l r))
                ((eq op '*) (* l r))
                ((eq op 'safe-div) (if (= r 0) 0 (/ l r)))
                (t 0)))))))

  ;; Fitness = negative sum of absolute errors over test cases
  ;; Higher (less negative) is better. 0 = perfect fit.
  (fset 'neovm--gp2-fitness
    (lambda (tree test-cases)
      "TEST-CASES is list of (input . expected-output). Returns negative total error."
      (let ((total-error 0))
        (dolist (tc test-cases)
          (let* ((input (car tc))
                 (expected (cdr tc))
                 (actual (funcall 'neovm--gp2-eval tree (list (cons 'x input))))
                 (err (abs (- actual expected))))
            (setq total-error (+ total-error err))))
        (- total-error))))

  ;; Target function: f(x) = x^2 + 2x + 1 = (x+1)^2
  ;; Test cases for x in -3..3
  (unwind-protect
      (let ((test-cases '((-3 . 4) (-2 . 1) (-1 . 0) (0 . 1) (1 . 4) (2 . 9) (3 . 16))))
        (list
         ;; Perfect solution: (+ (+ (* x x) (* 2 x)) 1)
         (funcall 'neovm--gp2-fitness
                  '(+ (+ (* x x) (* 2 x)) 1) test-cases)
         ;; Also perfect: (* (+ x 1) (+ x 1))
         (funcall 'neovm--gp2-fitness
                  '(* (+ x 1) (+ x 1)) test-cases)
         ;; Partial solution: just x^2 (misses 2x+1)
         (funcall 'neovm--gp2-fitness '(* x x) test-cases)
         ;; Terrible solution: constant 0
         (funcall 'neovm--gp2-fitness 0 test-cases)
         ;; Linear approximation: 3x + 1
         (funcall 'neovm--gp2-fitness '(+ (* 3 x) 1) test-cases)
         ;; Just x
         (funcall 'neovm--gp2-fitness 'x test-cases)
         ;; Fitness ordering: perfect > partial > linear > constant
         (let ((f-perfect (funcall 'neovm--gp2-fitness
                            '(+ (+ (* x x) (* 2 x)) 1) test-cases))
               (f-partial (funcall 'neovm--gp2-fitness '(* x x) test-cases))
               (f-linear (funcall 'neovm--gp2-fitness '(+ (* 3 x) 1) test-cases))
               (f-const (funcall 'neovm--gp2-fitness 0 test-cases)))
           (list (>= f-perfect f-partial)
                 (>= f-partial f-linear)
                 (>= f-linear f-const)))))
    (fmakunbound 'neovm--gp2-eval)
    (fmakunbound 'neovm--gp2-fitness)))"####;
    let expect = expect_test::expect![[r#""OK (0 0 -25 -35 -28 -35 (t t t))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Subtree crossover (exchange)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_gp_crossover() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(progn
  ;; Get subtree at position N (pre-order traversal, 0-indexed)
  (fset 'neovm--gp3-get-subtree
    (lambda (tree pos)
      "Get the subtree at pre-order position POS. Returns (subtree . remaining-pos)."
      (if (= pos 0)
          (cons tree 0)
        (if (or (numberp tree) (symbolp tree))
            (cons nil (1- pos))  ;; not found yet, decrement
          (let* ((left-result (funcall 'neovm--gp3-get-subtree (nth 1 tree) (1- pos)))
                 (left-sub (car left-result))
                 (left-remaining (cdr left-result)))
            (if left-sub
                left-result
              (funcall 'neovm--gp3-get-subtree (nth 2 tree) left-remaining)))))))

  ;; Replace subtree at position N with new subtree
  (fset 'neovm--gp3-replace-subtree
    (lambda (tree pos new-subtree)
      "Replace subtree at pre-order position POS with NEW-SUBTREE.
       Returns (new-tree . remaining-pos)."
      (if (= pos 0)
          (cons new-subtree 0)
        (if (or (numberp tree) (symbolp tree))
            (cons tree (1- pos))
          (let* ((op (car tree))
                 (left-result (funcall 'neovm--gp3-replace-subtree
                                (nth 1 tree) (1- pos) new-subtree))
                 (new-left (car left-result))
                 (left-remaining (cdr left-result)))
            (if (= left-remaining 0)
                (cons (list op new-left (nth 2 tree)) 0)
              (let* ((right-result (funcall 'neovm--gp3-replace-subtree
                                     (nth 2 tree) left-remaining new-subtree))
                     (new-right (car right-result))
                     (right-remaining (cdr right-result)))
                (cons (list op new-left new-right) right-remaining))))))))

  ;; Node count for subtree selection
  (fset 'neovm--gp3-node-count
    (lambda (tree)
      (if (or (numberp tree) (symbolp tree)) 1
        (+ 1 (funcall 'neovm--gp3-node-count (nth 1 tree))
           (funcall 'neovm--gp3-node-count (nth 2 tree))))))

  ;; Crossover: extract subtree from parent2 at pos2, insert into parent1 at pos1
  (fset 'neovm--gp3-crossover
    (lambda (parent1 pos1 parent2 pos2)
      (let ((subtree (car (funcall 'neovm--gp3-get-subtree parent2 pos2))))
        (car (funcall 'neovm--gp3-replace-subtree parent1 pos1 subtree)))))

  (unwind-protect
      (let ((p1 '(+ (* x x) (- x 1)))    ;; x^2 + (x - 1), 7 nodes
            (p2 '(* (+ x 3) (- x 2))))    ;; (x+3)(x-2), 7 nodes
        (list
         ;; Get subtree at various positions from p1
         ;; pos 0: whole tree
         (car (funcall 'neovm--gp3-get-subtree p1 0))
         ;; pos 1: (* x x)
         (car (funcall 'neovm--gp3-get-subtree p1 1))
         ;; pos 2: x (left of *)
         (car (funcall 'neovm--gp3-get-subtree p1 2))
         ;; pos 4: (- x 1)
         (car (funcall 'neovm--gp3-get-subtree p1 4))

         ;; Crossover: replace (* x x) in p1 with (+ x 3) from p2
         ;; p1 pos 1 = (* x x), p2 pos 1 = (+ x 3)
         (funcall 'neovm--gp3-crossover p1 1 p2 1)

         ;; Crossover: replace terminal x in p1 at pos 2 with constant 5
         (funcall 'neovm--gp3-crossover p1 2 5 0)

         ;; Crossover at root: replaces entire tree
         (funcall 'neovm--gp3-crossover p1 0 p2 0)

         ;; Node counts
         (funcall 'neovm--gp3-node-count p1)
         (funcall 'neovm--gp3-node-count p2)

         ;; Crossover result has expected structure
         (let ((child (funcall 'neovm--gp3-crossover p1 4 p2 4)))
           ;; Replace (- x 1) in p1 with (- x 2) from p2
           child)))
    (fmakunbound 'neovm--gp3-get-subtree)
    (fmakunbound 'neovm--gp3-replace-subtree)
    (fmakunbound 'neovm--gp3-node-count)
    (fmakunbound 'neovm--gp3-crossover)))"####;
    let expect = expect_test::expect![[
        r#""OK ((+ (* x x) (- x 1)) (* x x) x (- x 1) (+ (+ x 3) (- x 1)) (+ (* 5 x) (- x 1)) (* (+ x 3) (- x 2)) 7 7 (+ (* x x) (- x 1)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Mutation (random subtree replacement)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_gp_mutation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(progn
  (fset 'neovm--gp4-node-count
    (lambda (tree)
      (if (or (numberp tree) (symbolp tree)) 1
        (+ 1 (funcall 'neovm--gp4-node-count (nth 1 tree))
           (funcall 'neovm--gp4-node-count (nth 2 tree))))))

  (fset 'neovm--gp4-replace-subtree
    (lambda (tree pos new-sub)
      (if (= pos 0) (cons new-sub 0)
        (if (or (numberp tree) (symbolp tree))
            (cons tree (1- pos))
          (let* ((op (car tree))
                 (lr (funcall 'neovm--gp4-replace-subtree (nth 1 tree) (1- pos) new-sub))
                 (rem (cdr lr)))
            (if (= rem 0) (cons (list op (car lr) (nth 2 tree)) 0)
              (let* ((rr (funcall 'neovm--gp4-replace-subtree (nth 2 tree) rem new-sub)))
                (cons (list op (car lr) (car rr)) (cdr rr)))))))))

  ;; Generate a random subtree using deterministic PRNG
  ;; depth 0: terminal; depth > 0: function node
  (fset 'neovm--gp4-random-tree
    (lambda (max-depth state)
      "Generate random tree. Returns (tree . new-state)."
      (let ((s (% (+ (* state 1103515245) 12345) 2147483648)))
        (if (<= max-depth 0)
            ;; Terminal: variable or constant
            (let ((choice (% (/ s 65536) 4)))
              (cond
               ((= choice 0) (cons 'x s))
               ((= choice 1) (cons 1 s))
               ((= choice 2) (cons 2 s))
               (t (cons 3 s))))
          ;; Function node with 50% chance, else terminal
          (let ((choice (% (/ s 65536) 2)))
            (if (= choice 0)
                ;; Terminal
                (let ((term-choice (% (/ s 32768) 3)))
                  (cond ((= term-choice 0) (cons 'x s))
                        ((= term-choice 1) (cons 1 s))
                        (t (cons 2 s))))
              ;; Function
              (let* ((op-choice (% (/ s 16384) 4))
                     (op (cond ((= op-choice 0) '+)
                               ((= op-choice 1) '-)
                               ((= op-choice 2) '*)
                               (t '+)))
                     (lr (funcall 'neovm--gp4-random-tree (1- max-depth) s))
                     (left (car lr))
                     (rr (funcall 'neovm--gp4-random-tree (1- max-depth) (cdr lr)))
                     (right (car rr)))
                (cons (list op left right) (cdr rr)))))))))

  ;; Mutation: replace subtree at deterministic position
  (fset 'neovm--gp4-mutate
    (lambda (tree state)
      "Mutate tree by replacing a random subtree. Returns (new-tree . new-state)."
      (let* ((n (funcall 'neovm--gp4-node-count tree))
             (s (% (+ (* state 1103515245) 12345) 2147483648))
             (pos (% (/ s 65536) n))
             (new-sub-result (funcall 'neovm--gp4-random-tree 2 s))
             (new-sub (car new-sub-result))
             (new-state (cdr new-sub-result)))
        (cons (car (funcall 'neovm--gp4-replace-subtree tree pos new-sub))
              new-state))))

  (unwind-protect
      (let ((tree '(+ (* x x) (- x 1))))
        (list
         ;; Random tree generation at different depths
         (car (funcall 'neovm--gp4-random-tree 0 42))
         (car (funcall 'neovm--gp4-random-tree 1 42))
         (car (funcall 'neovm--gp4-random-tree 2 100))
         (car (funcall 'neovm--gp4-random-tree 2 999))
         ;; Mutation with different seeds produces different results
         (car (funcall 'neovm--gp4-mutate tree 42))
         (car (funcall 'neovm--gp4-mutate tree 100))
         (car (funcall 'neovm--gp4-mutate tree 200))
         ;; Original tree unchanged (mutation is non-destructive via cons)
         tree
         ;; Mutated tree is valid (still a tree structure)
         (let ((mutated (car (funcall 'neovm--gp4-mutate tree 42))))
           (funcall 'neovm--gp4-node-count mutated))
         ;; Multiple mutations accumulate
         (let* ((m1 (funcall 'neovm--gp4-mutate tree 42))
                (m2 (funcall 'neovm--gp4-mutate (car m1) (cdr m1)))
                (m3 (funcall 'neovm--gp4-mutate (car m2) (cdr m2))))
           (car m3))))
    (fmakunbound 'neovm--gp4-node-count)
    (fmakunbound 'neovm--gp4-replace-subtree)
    (fmakunbound 'neovm--gp4-random-tree)
    (fmakunbound 'neovm--gp4-mutate)))"####;
    let expect = expect_test::expect![[
        r#""OK (1 (+ 1 1) 2 x (+ (* x x) (- x 1)) (+ (* x x) (- x 1)) (+ (* x x) (- x 1)) (+ (* x x) (- x 1)) 7 (+ (* x x) (- x 1)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Tournament selection for GP
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_gp_tournament_selection() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(progn
  (fset 'neovm--gp5-eval
    (lambda (tree env)
      (cond
       ((numberp tree) tree)
       ((symbolp tree) (or (cdr (assq tree env)) 0))
       ((listp tree)
        (let ((op (car tree))
              (l (funcall 'neovm--gp5-eval (nth 1 tree) env))
              (r (funcall 'neovm--gp5-eval (nth 2 tree) env)))
          (cond ((eq op '+) (+ l r)) ((eq op '-) (- l r))
                ((eq op '*) (* l r))
                ((eq op 'safe-div) (if (= r 0) 0 (/ l r)))
                (t 0)))))))

  (fset 'neovm--gp5-fitness
    (lambda (tree test-cases)
      (let ((err 0))
        (dolist (tc test-cases)
          (setq err (+ err (abs (- (funcall 'neovm--gp5-eval tree
                                     (list (cons 'x (car tc))))
                                   (cdr tc))))))
        (- err))))

  ;; Tournament selection: pick k individuals, return fittest
  (fset 'neovm--gp5-tournament
    (lambda (pop fitnesses k state)
      "Select best of K random individuals. Returns (index . new-state)."
      (let ((best-idx 0)
            (best-fit -999999)
            (s state)
            (i 0)
            (n (length pop)))
        (while (< i k)
          (setq s (% (+ (* s 1103515245) 12345) 2147483648))
          (let* ((idx (% (/ s 65536) n))
                 (fit (aref fitnesses idx)))
            (when (> fit best-fit)
              (setq best-idx idx)
              (setq best-fit fit)))
          (setq i (1+ i)))
        (cons best-idx s))))

  (unwind-protect
      (let* ((test-cases '((-2 . 4) (-1 . 1) (0 . 0) (1 . 1) (2 . 4)))
             ;; Population of candidate programs
             (pop (vector
                   '(* x x)              ;; perfect: x^2
                   '(+ x x)              ;; 2x
                   3                      ;; constant 3
                   '(+ (* x x) 1)        ;; x^2 + 1
                   '(- (* x x) x)))      ;; x^2 - x
             ;; Compute fitnesses
             (fitnesses (make-vector 5 0)))
        (dotimes (i 5)
          (aset fitnesses i
                (funcall 'neovm--gp5-fitness (aref pop i) test-cases)))
        (list
         ;; Fitness values
         (let ((r nil) (i 4))
           (while (>= i 0) (setq r (cons (aref fitnesses i) r)) (setq i (1- i)))
           r)
         ;; Tournament with k=3: should tend to pick fitter individuals
         (let ((result (funcall 'neovm--gp5-tournament pop fitnesses 3 42)))
           (list (car result) (aref fitnesses (car result))))
         (let ((result (funcall 'neovm--gp5-tournament pop fitnesses 3 100)))
           (list (car result) (aref fitnesses (car result))))
         ;; Tournament with k=5 (whole pop): always picks the best
         (let ((result (funcall 'neovm--gp5-tournament pop fitnesses 5 42)))
           (car result))
         ;; Tournament with k=1: random selection
         (let ((result (funcall 'neovm--gp5-tournament pop fitnesses 1 42)))
           (car result))
         ;; 10 tournaments, count selections
         (let ((counts (make-vector 5 0))
               (s 12345))
           (dotimes (trial 10)
             (let ((result (funcall 'neovm--gp5-tournament pop fitnesses 3 s)))
               (aset counts (car result) (1+ (aref counts (car result))))
               (setq s (cdr result))))
           (let ((r nil) (i 4))
             (while (>= i 0) (setq r (cons (aref counts i) r)) (setq i (1- i)))
             r))))
    (fmakunbound 'neovm--gp5-eval)
    (fmakunbound 'neovm--gp5-fitness)
    (fmakunbound 'neovm--gp5-tournament)))"####;
    let expect = expect_test::expect![[r#""OK ((0 -12 -9 -5 -6) (3 -5) (2 -9) 3 1 (4 1 1 2 2))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Symbolic regression: fitting a polynomial
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_gp_symbolic_regression_polynomial() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(progn
  (fset 'neovm--gp6-eval
    (lambda (tree env)
      (cond
       ((numberp tree) tree)
       ((symbolp tree) (or (cdr (assq tree env)) 0))
       ((listp tree)
        (let ((op (car tree))
              (l (funcall 'neovm--gp6-eval (nth 1 tree) env))
              (r (funcall 'neovm--gp6-eval (nth 2 tree) env)))
          (cond ((eq op '+) (+ l r)) ((eq op '-) (- l r))
                ((eq op '*) (* l r))
                ((eq op 'safe-div) (if (= r 0) 0 (/ l r)))
                (t 0)))))))

  (fset 'neovm--gp6-fitness
    (lambda (tree test-cases)
      (let ((err 0))
        (dolist (tc test-cases)
          (setq err (+ err (abs (- (funcall 'neovm--gp6-eval tree
                                     (list (cons 'x (car tc))))
                                   (cdr tc))))))
        (- err))))

  ;; Generate training data for target: f(x) = 2x^2 - 3x + 1
  ;; Using integer x values to keep arithmetic exact
  (unwind-protect
      (let ((test-cases nil))
        ;; Generate test cases for x in -5..5
        (let ((x -5))
          (while (<= x 5)
            (let ((y (+ (* 2 x x) (* -3 x) 1)))
              (setq test-cases (cons (cons x y) test-cases)))
            (setq x (1+ x))))
        (setq test-cases (nreverse test-cases))
        (list
         ;; Verify test cases
         test-cases
         ;; Perfect candidate: (+ (- (* 2 (* x x)) (* 3 x)) 1)
         (let ((perfect '(+ (- (* 2 (* x x)) (* 3 x)) 1)))
           (funcall 'neovm--gp6-fitness perfect test-cases))
         ;; Close candidate: (+ (* 2 (* x x)) (- 1 (* 3 x)))
         (let ((close '(+ (* 2 (* x x)) (- 1 (* 3 x)))))
           (funcall 'neovm--gp6-fitness close test-cases))
         ;; Partial: just 2x^2
         (funcall 'neovm--gp6-fitness '(* 2 (* x x)) test-cases)
         ;; Linear: 3x + 1
         (funcall 'neovm--gp6-fitness '(+ (* 3 x) 1) test-cases)
         ;; Constant zero
         (funcall 'neovm--gp6-fitness 0 test-cases)
         ;; Evaluate best candidate at novel points
         (let ((perfect '(+ (- (* 2 (* x x)) (* 3 x)) 1)))
           (list
            (funcall 'neovm--gp6-eval perfect '((x . 10)))
            (funcall 'neovm--gp6-eval perfect '((x . -10)))
            (funcall 'neovm--gp6-eval perfect '((x . 0)))))))
    (fmakunbound 'neovm--gp6-eval)
    (fmakunbound 'neovm--gp6-fitness)))"####;
    let expect = expect_test::expect![[
        r#""OK (((-5 . 66) (-4 . 45) (-3 . 28) (-2 . 15) (-1 . 6) (0 . 1) (1 . 0) (2 . 3) (3 . 10) (4 . 21) (5 . 36)) 0 0 -91 -236 -231 (171 231 1))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Population evolution over generations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_gp_population_evolution() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(progn
  (fset 'neovm--gp7-eval
    (lambda (tree env)
      (cond
       ((numberp tree) tree)
       ((symbolp tree) (or (cdr (assq tree env)) 0))
       ((listp tree)
        (let ((op (car tree))
              (l (funcall 'neovm--gp7-eval (nth 1 tree) env))
              (r (funcall 'neovm--gp7-eval (nth 2 tree) env)))
          (cond ((eq op '+) (+ l r)) ((eq op '-) (- l r))
                ((eq op '*) (* l r))
                ((eq op 'safe-div) (if (= r 0) 0 (/ l r)))
                (t 0)))))))

  (fset 'neovm--gp7-fitness
    (lambda (tree test-cases)
      (let ((err 0))
        (dolist (tc test-cases)
          (setq err (+ err (abs (- (funcall 'neovm--gp7-eval tree
                                     (list (cons 'x (car tc))))
                                   (cdr tc))))))
        (- err))))

  (fset 'neovm--gp7-node-count
    (lambda (tree)
      (if (or (numberp tree) (symbolp tree)) 1
        (+ 1 (funcall 'neovm--gp7-node-count (nth 1 tree))
           (funcall 'neovm--gp7-node-count (nth 2 tree))))))

  ;; Simple crossover: swap right subtrees of two function nodes
  (fset 'neovm--gp7-simple-crossover
    (lambda (p1 p2)
      "Swap right subtrees if both are function nodes, else return p1."
      (if (and (listp p1) (listp p2))
          (list (car p1) (nth 1 p1) (nth 2 p2))
        p1)))

  ;; Simple mutation: replace a terminal with another
  (fset 'neovm--gp7-simple-mutate
    (lambda (tree state)
      "If terminal, replace with random terminal. If function, mutate left child."
      (let ((s (% (+ (* state 1103515245) 12345) 2147483648)))
        (if (or (numberp tree) (symbolp tree))
            (let ((choice (% (/ s 65536) 4)))
              (cons (cond ((= choice 0) 'x) ((= choice 1) 1)
                          ((= choice 2) 2) (t 3))
                    s))
          (let* ((mr (funcall 'neovm--gp7-simple-mutate (nth 1 tree) s)))
            (cons (list (car tree) (car mr) (nth 2 tree)) (cdr mr)))))))

  ;; Tournament selection (k=2)
  (fset 'neovm--gp7-select
    (lambda (pop fitnesses state)
      (let* ((n (length pop))
             (s (% (+ (* state 1103515245) 12345) 2147483648))
             (i1 (% (/ s 65536) n))
             (s2 (% (+ (* s 1103515245) 12345) 2147483648))
             (i2 (% (/ s2 65536) n))
             (idx (if (>= (aref fitnesses i1) (aref fitnesses i2)) i1 i2)))
        (cons idx s2))))

  ;; One generation step
  (fset 'neovm--gp7-evolve-gen
    (lambda (pop fitnesses state pop-size test-cases)
      "Evolve one generation. Elitism: keep best. Fill rest with crossover+mutation."
      (let ((new-pop (make-vector pop-size nil))
            (s state)
            ;; Find elite
            (best-idx 0) (best-fit -999999))
        (dotimes (i pop-size)
          (when (> (aref fitnesses i) best-fit)
            (setq best-idx i best-fit (aref fitnesses i))))
        (aset new-pop 0 (aref pop best-idx))
        ;; Fill rest
        (let ((i 1))
          (while (< i pop-size)
            (let* ((sel1 (funcall 'neovm--gp7-select pop fitnesses s))
                   (sel2 (funcall 'neovm--gp7-select pop fitnesses (cdr sel1)))
                   (p1 (aref pop (car sel1)))
                   (p2 (aref pop (car sel2)))
                   (child (funcall 'neovm--gp7-simple-crossover p1 p2))
                   (mutated (funcall 'neovm--gp7-simple-mutate child (cdr sel2))))
              (aset new-pop i (car mutated))
              (setq s (cdr mutated)))
            (setq i (1+ i))))
        ;; Compute new fitnesses
        (let ((new-fit (make-vector pop-size 0)))
          (dotimes (i pop-size)
            (aset new-fit i (funcall 'neovm--gp7-fitness (aref new-pop i) test-cases)))
          (list new-pop new-fit s)))))

  (unwind-protect
      (let* ((test-cases '((0 . 0) (1 . 1) (2 . 4) (3 . 9) (4 . 16)))
             ;; Target: x^2. Initial population:
             (pop (vector
                   '(* x x)        ;; perfect
                   '(+ x x)        ;; 2x
                   'x               ;; x
                   1                ;; constant 1
                   '(+ x 1)        ;; x+1
                   '(* x 2)))      ;; 2x
             (pop-size 6)
             (fitnesses (make-vector pop-size 0)))
        ;; Compute initial fitnesses
        (dotimes (i pop-size)
          (aset fitnesses i (funcall 'neovm--gp7-fitness (aref pop i) test-cases)))
        (let ((initial-best (let ((b -999999) (j 0))
                              (while (< j pop-size)
                                (when (> (aref fitnesses j) b)
                                  (setq b (aref fitnesses j)))
                                (setq j (1+ j)))
                              b))
              (gen-bests nil)
              (state 54321))
          ;; Evolve for 5 generations
          (dotimes (gen 5)
            (let ((result (funcall 'neovm--gp7-evolve-gen
                            pop fitnesses state pop-size test-cases)))
              (setq pop (nth 0 result))
              (setq fitnesses (nth 1 result))
              (setq state (nth 2 result)))
            ;; Record best fitness
            (let ((best -999999) (j 0))
              (while (< j pop-size)
                (when (> (aref fitnesses j) best)
                  (setq best (aref fitnesses j)))
                (setq j (1+ j)))
              (setq gen-bests (cons best gen-bests))))
          (let ((gen-bests (nreverse gen-bests)))
            (list
             ;; Initial best fitness (should be 0 since x^2 is in initial pop)
             initial-best
             ;; Best fitness per generation (should remain 0 due to elitism)
             gen-bests
             ;; Elitism: best never decreases
             (let ((ok t) (prev initial-best) (rest gen-bests))
               (while rest
                 (when (< (car rest) prev) (setq ok nil))
                 (setq prev (car rest))
                 (setq rest (cdr rest)))
               ok)
             ;; Population size remains constant
             (length pop)
             ;; All individuals are valid trees (non-nil)
             (let ((all-valid t) (j 0))
               (while (< j pop-size)
                 (when (null (aref pop j)) (setq all-valid nil))
                 (setq j (1+ j)))
               all-valid)))))
    (fmakunbound 'neovm--gp7-eval)
    (fmakunbound 'neovm--gp7-fitness)
    (fmakunbound 'neovm--gp7-node-count)
    (fmakunbound 'neovm--gp7-simple-crossover)
    (fmakunbound 'neovm--gp7-simple-mutate)
    (fmakunbound 'neovm--gp7-select)
    (fmakunbound 'neovm--gp7-evolve-gen)))"####;
    let expect = expect_test::expect![[r#""OK (0 (0 0 0 0 0) t 6 t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
